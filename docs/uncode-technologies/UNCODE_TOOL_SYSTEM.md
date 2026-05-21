# uncode 工具系统

> ToolExecutor trait + 9 工具实现 + 沙箱 + Hooks | 基于源码分析，2026-05 修订

uncode 的工具系统由三层构成：`uncode-core` 定义 trait，`uncode-macros` 编译时生成 Schema，`uncode-agent` 提供具体实现。所有文件操作工具共享沙箱路径校验和统一的执行生命周期。

**谁决定调用哪个工具？** 由 LLM 在 function calling 下自行选择；框架只限制 active 工具集并负责执行。详见 [`UNCODE_TOOL_SELECTION_BY_LLM.md`](UNCODE_TOOL_SELECTION_BY_LLM.md)。

---

## ToolExecutor trait

```rust
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    async fn execute(&self, arguments: serde_json::Value) -> Result<String, UncodeError>;
    fn prepare_arguments(&self, arguments: serde_json::Value) -> Result<serde_json::Value, UncodeError> {
        Ok(arguments)  // 默认透传
    }
    async fn execute_with_context(
        &self,
        arguments: serde_json::Value,
        _ctx: ToolContext,
    ) -> Result<ToolResult, UncodeError> {
        // 默认：调用 execute() → 包装为 ToolResult
    }
}
```

### ToolResult

```rust
pub struct ToolResult {
    pub content: Vec<ToolContent>,    // Text 或 Image
    pub is_error: bool,
    pub details: Option<serde_json::Value>,
    pub terminate: bool,              // 请求终止 agent 循环
}
```

### ToolDefinition

```rust
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,  // JSON Schema
    pub label: Option<String>,          // UI 显示名
    pub execution_mode: ExecutionMode,  // Parallel（默认）或 Sequential
}
```

---

## #[tool] 宏

编译时从函数签名自动生成 `ToolDefinition`：

```rust
#[tool(label = "Read File", execution_mode = "sequential")]
/// 读取文件内容
fn read(path: String, offset: Option<usize>) -> String { ... }
```

生成：
- 原函数保持不变
- 伴生函数 `__tool_schema_read()` 返回完整的 `ToolDefinition`
  - `description`：从 `///` doc comment 提取
  - `parameters`：按参数名和类型自动构建 JSON Schema
  - `required`：非 `Option` 参数自动标记

类型映射：`String` → `"string"`、数值类型 → `"number"`、`bool` → `"boolean"`、其他 → `"string"`。`Option<T>` 标记为非 required，内部类型用于 Schema。

---

## 已实现的工具（9 个）

逐个工具的用途、参数、设计原理与限制见 **[`UNCODE_BUILTIN_TOOLS.md`](UNCODE_BUILTIN_TOOLS.md)**。

| 工具 | 实现文件 | 功能 | CLI 注册 | 执行模式 |
|------|----------|------|----------|----------|
| `ReadTool` | `read.rs` | 读取文件（支持 offset/limit 行范围、hashline） | 是 | Parallel |
| `WriteTool` | `write.rs` | 创建或完整覆写文件 | 是 | Parallel |
| `EditTool` | `edit.rs` | hashline 精确编辑 + legacy 字符串替换 | 是 | Parallel |
| `GrepTool` | `grep.rs` | 正则搜索文件内容 | 是 | Parallel |
| `BashTool` | `bash.rs` | 执行 bash 命令（支持 timeout、workdir、实时取消） | 是 | **Sequential**（对齐 Pi：含 bash 的批次整批串行） |
| `WebFetch` | `web_fetch.rs` | HTTP 抓取网页内容 | 是（默认不 active） | Parallel |
| `WebSearch` | `web_search.rs` | Tavily 网络搜索（需 API key） | 条件（默认不 active） | Parallel |
| `FindTool` | `find.rs` | 按名称/模式查找文件 | 是 | Parallel |
| `LsTool` | `ls.rs` | 列出目录内容 | 是 | Parallel |

辅助模块（非工具）：`diff.rs`（统一 diff 生成）、`hashline.rs`（行哈希锚点）、`local_env.rs`（文件系统/Shell 抽象）。

---

## 沙箱路径校验

所有文件操作工具使用两层路径安全机制：

```rust
// tools/mod.rs
fn resolve_path(raw: &str) -> Result<PathBuf, UncodeError> {
    let cwd = std::env::current_dir()?;
    let path = normalize_path(raw);         // 处理 ./ ../ 等
    let resolved = cwd.join(path);
    let canonical = resolved.canonicalize() // 或查找最近存在的祖先
        .unwrap_or_else(|_| find_existing_ancestor(&resolved));
    // 拒绝解析到 CWD 之外的路径
    if !canonical.starts_with(&cwd) {
        return Err(FileError::SandboxViolation.into());
    }
    Ok(canonical)
}
```

**关键**：文件必须保持在 CWD 内。尝试操作 `../../../etc/passwd` 会被拒绝并返回 `SandboxViolation(1003)` 错误。

---

## 执行生命周期

单个工具的执行流程（`execute_single_tool`）：

```
① prepare_arguments（对齐 Pi prepareArguments）
    ↓ 工具特定的参数转换 / 垫片
② ToolRegistry::validate（对齐 Pi validateToolArguments，校验 prepared 结果）
    ↓ Err → `Validation failed for tool "…"`（含参数 JSON）
③ before_tool_call hook（入参为 prepared args）
    ↓ 返回 Some(reason) → 拒绝执行，返回错误
    ↓ 返回 None → 继续
④ 创建子 CancellationToken + ToolContext
⑤ execute_with_context(prepared_args, ctx)
    ↓ tokio::select! { 执行 / 取消 }
⑥ after_tool_call hook
    ↓ 可修改 content、details、is_error、terminate
⑦ 返回 ToolResult
```

### CLI 与 Pi 默认工具集

| 选项 | 行为 |
|------|------|
| （默认） | Pi 七件套 active；`web_*` 已注册但不对 LLM 暴露 |
| `--tools a,b` | 白名单 active |
| `--no-tools` | 空 active |
| `--no-builtin-tools` | 仅扩展工具 active（通常 `web_fetch` / `web_search`） |

入口：`register_coding_tools_and_configure`（`builtin.rs`）；测试/ harness 可用 `new_pi_coding_registry`。

### JSON Schema 校验（轻量）

`ToolRegistry::validate` / `prepare_and_validate` 在 Pi 流水线中校验 **prepare 之后** 的参数，当前支持：

- `required`、`properties` 类型
- `enum`（如 `edit.edits[].op`）
- `array` 的 `items` 嵌套
- `object` 嵌套 `required` / `properties`
- `additionalProperties: false`

已支持：`minimum`/`maximum`（整数）、`minLength`（字符串）；`prepare_and_validate` 前对 schema 做轻量 **coerce**（字符串 → integer/number/boolean，对齐 Pi `Value.Convert` 子集）。

未覆盖：TypeBox 级 `allOf`/`oneOf` 等（与 Pi TypeBox 仍有差距）。

### ToolContext

```rust
pub struct ToolContext {
    pub cancel_token: CancellationToken,   // 从 AgentLoop 透传
    pub on_progress: Option<Box<dyn Fn(ToolProgress) + Send + Sync>>,
    pub tool_call_id: String,
    pub execution_env: Option<Arc<dyn ExecutionEnv>>,  // AgentLoop 注入 LocalExecutionEnv
}
```

`CancellationToken` 从 `AgentLoop` 一路透传到最底层工具——与 Pi 的 AbortController 模式一致。

`execution_env` 为 `None` 时，工具通过 `default_execution_env()` 回退到进程内 `LocalExecutionEnv`。当前经 `FileSystem` 接入：`read`、`ls`、`write`/`edit`（读旧内容）、`grep`（`file_info` + `read_text_file`）；`find` 仅目录遍历（`ignore`），不读文件内容。

---

## 批量执行：并行 vs 串行

```rust
let has_sequential = executions.iter().any(|(_, name, _)| {
    self.tool_registry.execution_mode(name) == ExecutionMode::Sequential
});

if has_sequential {
    // 整批串行：每个工具完整走 execute_single_tool
    for (id, name, args) in executions { ... }
} else {
    // Pi 并行批次：prepare → validate → before 按顺序逐个；execute 再 join_all 并发
    for item in &executions { prepare_tool_call(...).await; }
    join_all(ready.map(|item| execute_prepared_tool_shared(...)));
}
```

**策略**：批次中任一工具为 `Sequential`（含 `bash`）→ 整批串行。否则 prepare/before 串行、execute 并行（对齐 Pi `parallel` 模式）。

---

## Hooks 系统

### ToolHooks trait

```rust
#[async_trait]
pub trait ToolHooks: Send + Sync {
    async fn before_tool_call(&self, ctx: &BeforeToolCallContext) -> BeforeToolCallResult {
        None  // 默认允许
    }
    async fn after_tool_call(&self, ctx: &AfterToolCallContext, result: &mut ToolResult)
        -> AfterToolCallResult { ... }
}
```

### BeforeToolCallContext

```rust
pub struct BeforeToolCallContext {
    pub tool_call_id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
}
// 返回 None = 允许执行
// 返回 Some(reason) = 拒绝执行
```

### AfterToolCallResult

```rust
pub struct AfterToolCallResult {
    pub content: Option<Vec<ToolContent>>,   // 替换输出内容
    pub details: Option<serde_json::Value>,  // 替换详情
    pub is_error: Option<bool>,              // 修改错误标志
    pub terminate: Option<bool>,             // 修改终止标志
}
```

---

## ExecutionEnv 抽象

工具不直接调用文件系统或 shell，而是通过 trait 抽象：

```rust
pub trait FileSystem: Send + Sync {
    fn read_text_file(&self, path: &Path) -> Result<String>;
    fn write_file(&self, path: &Path, content: &str) -> Result<()>;
    fn file_info(&self, path: &Path) -> Result<FileInfo>;
    fn list_dir(&self, path: &Path) -> Result<Vec<DirEntry>>;
    fn exists(&self, path: &Path) -> bool;
    fn canonical_path(&self, path: &Path) -> Result<PathBuf>;
}

pub trait Shell: Send + Sync {
    fn exec(&self, cmd: &str, opts: ShellOptions) -> Result<ShellResult>;
}

pub trait ExecutionEnv: Send + Sync {
    fn fs(&self) -> &dyn FileSystem;
    fn shell(&self) -> &dyn Shell;
}
```

当前唯一实现：`LocalFileSystem` + `LocalShell`（`local_env.rs`）。设计为未来可替换为沙盒或远程执行环境。

---

## 大输出截断

BashTool 和 GrepTool 对大输出自动截断：

```
... (N lines omitted) ...
[Showing lines {start}-{end} of {total}]
```

LLM 知道完整输出被截断，可以用 `read` 工具按需读取。

---

*本文档基于 uncode 源码（`crates/uncode-agent/src/tools/`、`crates/uncode-core/src/tool.rs`）编写。*
