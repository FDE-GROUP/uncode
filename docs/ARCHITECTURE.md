# uncode 架构详细设计

## 一、分层架构总览

```
┌─────────────────────────────────────────────────────────┐
│                    uncode-cli                            │
│         二进制入口 / CLI参数解析 / 模式分发               │
│         (clap + tracing-subscriber)                      │
├──────────────────────┬──────────────────────────────────┤
│     uncode-tui       │     uncode-platform               │
│   ratatui + crossterm│   axum/actix-web REST + WS        │
│   对话驱动终端界面   │   分析监控平台服务端               │
├──────────────────────┴──────────────────────────────────┤
│                  uncode-agent                            │
│   代理循环引擎 / 系统提示构建 / Token估算 / 上下文压缩       │
├──────────┬──────────┬──────────┬────────────────────────┤
│ uncode-  │ uncode-  │ uncode-  │ uncode-extensions       │
│ llm     │ tools    │ session  │ WASM运行时 / 生命周期钩子  │
│ LLM驱动  │ 内置工具  │ 会话持久化 │                         │
├──────────┴──────────┴──────────┴────────────────────────┤
│                  uncode-core                             │
│   共享类型 / trait定义 / 错误类型 / 事件枚举               │
├─────────────────────────────────────────────────────────┤
│                  uncode-macros                           │
│   过程宏 (#[tool], #[derive(Event)] 等)                   │
└─────────────────────────────────────────────────────────┘
```

依赖方向：**上层依赖下层，下层不依赖上层。** core 和 macros 是叶子节点，不依赖任何内部 crate。

---

## 二、Crate 依赖关系

```
                    ┌──────────────┐
                    │  uncode-cli  │
                    └──────┬───────┘
            ┌──────────────┼───────────────┐
            │              │               │
     ┌──────▼──────┐ ┌─────▼──────┐ ┌──────▼────────┐
     │  uncode-tui │ │ uncode-    │ │ uncode-rpc    │
     │             │ │ platform   │ │ (规划中)       │
     └──────┬──────┘ └─────┬──────┘ └──────┬────────┘
            │              │               │
            └──────────────┼───────────────┘
                           │
                    ┌──────▼──────┐
                    │ uncode-agent│
                    └──┬──┬──┬──┬─┘
            ┌──────────┘  │  │  └──────────┐
            │             │  │             │
     ┌──────▼───┐ ┌───────▼──▼──┐ ┌───────▼────────┐
     │ uncode-  │ │  uncode-    │ │  uncode-       │
     │ llm     │ │  session    │ │  extensions    │
     └────┬─────┘ └──────┬──────┘ └───────┬────────┘
          │              │               │
          │       ┌──────▼──────┐        │
          └───────┤ uncode-tools│────────┘
                  └──────┬──────┘
                         │
                  ┌──────▼──────┐
                  │ uncode-core │
                  └─────────────┘
                  ┌──────────────┐
                  │ uncode-macros│ (编译时，不参与运行时依赖)
                  └──────────────┘
```

### 2.1 各 Crate 职责与内部依赖

| Crate | 职责 | 内部依赖 |
|-------|------|---------|
| `uncode-core` | 共享数据类型、trait、错误、事件 | 无 |
| `uncode-macros` | 过程宏 `#[tool]` 等代码生成 | 无 |
| `uncode-llm` | LLM 驱动 trait + 7 个供应商实现 + 注册表 | core |
| `uncode-session` | 会话 JSONL 存储、会话管理器 | core |
| `uncode-tools` | 内置工具实现 + 工具注册表 | core |
| `uncode-extensions` | WASM 扩展运行时、生命周期钩子 | core |
| `uncode-agent` | 代理循环引擎、系统提示、Token估算 | core + llm + session + tools + extensions |
| `uncode-tui` | 对话驱动终端 UI | core + agent + session |
| `uncode-platform` | 分析监控平台服务端 | core + session |
| `uncode-rpc` | JSON-RPC 外部接口（规划中） | core + agent |
| `uncode-cli` | 命令行入口 + 模式分发 | 所有 |

---

## 三、核心数据流

### 3.1 Agent 对话主循环

```
用户输入
    │
    ▼
┌─────────────────────────────────────────────────────┐
│                   AgentLoop                          │
│                                                      │
│  1. 构建请求（系统提示 + 历史消息 + 工具定义 + 用户输入）│
│  2. 调用 LLM（流式）                                   │
│  3. 解析响应流：                                       │
│     ├── 文本 delta → 发送 ContentDelta 事件            │
│     ├── 工具调用开始 → 发送 ToolCallStart 事件          │
│     ├── 工具调用参数 delta → 缓存                      │
│     └── 工具调用结束 → 执行工具 → 发送 ToolCallEnd 事件  │
│  4. 工具结果追加到消息历史                              │
│  5. 判断是否继续：                                     │
│     ├── stop_reason == "end_turn" → 发送 PhaseSummary  │
│     └── stop_reason == "tool_use" → 回到步骤 2         │
│  6. 返回最终响应                                       │
└─────────────────────────────────────────────────────┘
```

### 3.2 事件广播流

```
AgentLoop
    │
    ├──TaskUpdate──────────→ tokio::sync::broadcast ──→ TUI(状态栏任务显示)
    ├──ContentDelta────────→ tokio::sync::broadcast ──→ TUI(对话区Agent回复)
    ├──ToolCallStart───────→ tokio::sync::broadcast ──→ TUI(对话区工具调用)
    ├──ToolCallProgress────→ tokio::sync::broadcast ──→ TUI(工具调用进度)
    ├──ToolCallEnd─────────→ tokio::sync::broadcast ──→ TUI(工具调用结果)
    ├──PhaseSummary────────→ tokio::sync::broadcast ──→ TUI(对话区总结卡片)
    ├──Error───────────────→ tokio::sync::broadcast ──→ TUI(对话区错误消息)
    ├──SessionStart────────→ tokio::sync::broadcast ──→ 日志
    ├──TurnEnd─────────────→ tokio::sync::broadcast ──→ 会话存储
    └──SessionEnd──────────→ tokio::sync::broadcast ──→ 会话存储
                                                        │
                                                        ▼
                                                JSONL 文件
                                                        │
                                                        ▼
                                              Platform（文件读取）
```

### 3.3 会话数据流（TUI → Platform）

```
Phase 1-2                            Phase 3
─────────                            ────────
TUI/AgentLoop                        Platform 服务端
    │                                     │
    ├── 写入 JSONL ──→ ~/.uncode/         │
    │                 sessions/{id}.jsonl  │
    │                                     ├── 文件监听/定时扫描
    │                                     ├── 解析 JSONL
│                                     ├── 存入 SurrealDB SurrealKV（本地）
│                                     │   或 SurrealDB TiKV（团队）
    │                                     ├── REST API 暴露数据
    │                                     │       ↓
    │                                     └── TypeScript 前端展示
```

---

## 四、LLM 驱动抽象

### 4.1 LlmDriver Trait

```rust
#[async_trait]
pub trait LlmDriver: Send + Sync {
    /// 发送完成请求，返回流式事件流
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<'static, StreamEvent>, UncodeError>;

    /// 供应商名称标识
    fn provider_name(&self) -> &'static str;
}
```

### 4.2 CompletionRequest

```rust
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub system: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub tools: Vec<ToolDefinition>,
}
```

### 4.3 StreamEvent

```rust
pub enum StreamEvent {
    TextDelta(String),
    ToolCallStart { id: String, name: String },
    ToolCallDelta { id: String, arguments: String },
    ToolCallEnd { id: String, name: String, arguments: Value },
    Usage(UsageInfo),
    Error(String),
    Done,
}
```

### 4.4 ProviderRegistry

```rust
pub struct ProviderRegistry {
    drivers: RwLock<HashMap<String, Arc<dyn LlmDriver>>>,
}
```

- `register(name, driver)` — 注册一个 LLM 驱动
- `get(name)` — 按名称获取驱动
- `list()` — 列出所有已注册的驱动名称

### 4.5 供应商实现规划

| Phase | 供应商 | 实现要点 |
|-------|--------|---------|
| 1 | GLM | REST API + SSE 流式解析 |
| 1 | DeepSeek | OpenAI 兼容接口 |
| 1 | Ollama | 本地 HTTP API |
| 2 | OpenAI | 原生 OpenAI API（含 Responses API） |
| 2 | Anthropic | Messages API + extended thinking |
| 2 | Gemini | Gemini API + Vertex AI 兼容 |
| 2 | OpenRouter | OpenAI 兼容中转 |

---

## 五、工具系统

### 5.1 ToolExecutor Trait

```rust
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// 返回工具定义（名称、描述、参数 Schema）
    fn definition(&self) -> ToolDefinition;

    /// 执行工具，参数为 JSON Value
    async fn execute(
        &self,
        arguments: serde_json::Value,
    ) -> Result<String, UncodeError>;
}
```

### 5.2 ToolRegistry

```rust
pub struct ToolRegistry {
    tools: RwLock<HashMap<String, Arc<dyn ToolExecutor>>>,
}
```

- `register(name, executor)` — 注册工具
- `get(name)` — 按名称获取工具
- `definitions()` — 获取所有工具定义（传递给 LLM）

### 5.3 内置工具清单

| 工具 | 职责 | 关键参数 |
|------|------|---------|
| `read` | 读取文件内容 | path, offset, limit |
| `write` | 写入文件内容 | path, content |
| `edit` | 文件内字符串替换 | path, old_string, new_string |
| `grep` | 正则搜索文件内容 | pattern, path, include |
| `find` | 按文件名模式查找 | pattern, path |
| `bash` | 执行 shell 命令 | command, timeout, workdir |
| `ls` | 列出目录内容 | path |
| `glob` | 按 glob 模式匹配文件 | pattern, path |
| `webfetch` | 获取 URL 内容 | url, format |

### 5.4 #[tool] 过程宏

```rust
/// 读取文件内容到字符串
/// 
/// 支持按偏移量和行数限制读取范围
#[tool]
fn read(path: String, offset: Option<usize>, limit: Option<usize>) -> ToolResult { ... }
```

宏展开后自动生成对应的 `ToolDefinition`（从函数签名推断参数 Schema）。

---

## 六、会话存储

### 6.1 JSONL 格式

每个会话一个 `.jsonl` 文件，存储在 `~/.uncode/sessions/{id}.jsonl`。

**头行（会话元数据）：**
```json
{"type":"header","id":"uuid","created_at":"ISO8601","model":"deepseek-v3","title":"实现登录功能"}
```

**消息条目：**
```json
{"type":"message","timestamp":"ISO8601","role":"user","content":[{"type":"text","text":"帮我实现登录功能"}]}
{"type":"message","timestamp":"ISO8601","role":"assistant","content":[{"type":"text","text":"好的，我先分析一下项目结构"}],"usage":{"input":230,"output":45}}
{"type":"message","timestamp":"ISO8601","role":"assistant","content":[{"type":"tool_call","id":"call_1","name":"read","arguments":{"path":"src/main.rs"}}]}
{"type":"message","timestamp":"ISO8601","role":"tool","content":[{"type":"tool_result","tool_call_id":"call_1","content":"文件内容...","is_error":false}]}
```

**系统条目：**
```json
{"type":"system","timestamp":"ISO8601","event":"phase_summary","data":{"completed":["分析项目"],"next":["编写登录代码"]}}
```

### 6.2 SessionManager

```
SessionManager
├── create(title?) → SessionMetadata
├── list() → Vec<SessionMetadata>
├── append(session_id, entry) → ()
├── load(session_id) → Vec<SessionEntry>
├── destroy(session_id) → ()
└── compact(session_id) → ()  # 压缩旧消息
```

### 6.3 上下文压缩

当会话消息的估算 token 数超过模型上下文窗口的 80% 时自动触发：
1. 保留最近 N 轮完整对话
2. 对更早的消息调用 LLM 生成摘要
3. 用摘要替换原始消息，在消息列表开头插入

---

## 七、TUI 架构

### 7.1 组件树

```
App
├── StatusBar（顶部状态栏：版本/模型/会话/Token）
├── ChatView（主区域：可滚动对话历史）
│   ├── UserMessage（用户消息）
│   ├── AssistantMessage（Agent 回复，Markdown 渲染）
│   ├── ThinkingBlock（思考过程，默认折叠）
│   ├── ToolCallBlock（工具调用，可折叠展开）
│   │   ├── DiffView（编辑操作的内联 diff）
│   │   └── PermissionPrompt（权限确认请求）
│   ├── ErrorMessage（错误提示）
│   └── SummaryCard（阶段总结）
├── InputEditor（底部输入栏，多行编辑）
├── KeyHintBar（快捷键提示栏）
└── OverlaySelector（模型/会话选择弹出层）
```

### 7.2 渲染循环

```rust
struct TuiEngine {
    messages: Vec<ChatMessage>,
    terminal: Terminal<CrosstermBackend<Stdout>>,
    event_rx: broadcast::Receiver<AgentEvent>,
    editor: InputEditor,
    scroll_offset: usize,
    auto_scroll: bool,
}

impl TuiEngine {
    async fn run(&mut self) {
        loop {
            tokio::select! {
                event = self.event_rx.recv() => self.handle_agent_event(event),
                key = read_key_event() => self.handle_key_event(key),
            }
            self.render()?;
        }
    }
}
```

### 7.3 对话消息更新策略

| 消息类型 | 更新频率 | 触发条件 |
|---------|---------|---------|
| Agent 文本 | 高频（流式） | ContentDelta(Text) 事件 |
| 思考过程 | 高频（流式） | ContentDelta(Thinking) 事件 |
| 工具调用 | 中频 | ToolCallStart/Progress/End 事件 |
| 权限请求 | 极低频 | 工具调用需确认时 |
| 阶段总结 | 极低频 | PhaseSummary 事件 |

---

## 八、Platform 架构

### 8.1 数据存储

Platform 使用 **SurrealDB SurrealKV** 嵌入模式作为查询层，与 TOGAF TURBO 统一技术栈。

- **本地模式**：SurrealKV 嵌入，零配置，二进制文件存储在 `~/.uncode/surrealkv/`
- **团队模式**：SurrealDB TiKV 分布式集群，支持多用户并发查询
- **规范存储不变**：JSONL 文件仍是会话的权威数据源，Platform 仅在此基础上建立索引

### 8.2 服务端

```
Platform Server (axum/actix-web)
├── /api/sessions          GET    列出所有会话
├── /api/sessions/:id      GET    获取会话详情
├── /api/sessions/:id/events  GET 获取会话事件流
├── /api/metrics           GET    获取全局指标
├── /ws/events             WS     实时事件推送
├── Issues Panel API
│   ├── /api/issues            GET    列出 Issues
│   ├── /api/issues/:number    GET    Issue 详情
│   └── /api/issues/:number/link  POST 关联到会话
└── SurrealDB SurrealKV（本地）或 TiKV（团队）
```

### 8.3 前端

```typescript
// React 19 + TanStack Router 组件树
<App>
  <Sidebar>
    <SessionList />
    <MetricsPanel />
  </Sidebar>
  <MainContent>
    <SessionTimeline />    // 时间线视图（TanStack Virtual）
    <ToolCallInspector />  // 工具调用详情 + diff
    <IssuesPanel />        // GitHub Issues 面板（TanStack Table）
    <OptimizationHints />  // 数据驱动优化建议
  </MainContent>
</App>
```

---

## 九、扩展系统

### 9.1 WASM 扩展模型

```
用户扩展（.wasm）
    │
    ▼
WasmRuntime（wasmtime）
    ├── 沙箱隔离（内存、文件系统、网络）
    ├── 导出函数：on_event(event), register_tools()
    └── 宿主提供：log(), http_fetch(), file_read()
```

### 9.2 生命周期钩子

| 钩子 | 触发时机 | 扩展可做什么 |
|------|---------|-------------|
| `session_start` | 会话开始 | 初始化状态、注册工具 |
| `turn_start` | 每轮对话开始 | 注入额外上下文 |
| `message_received` | 收到用户消息后 | 预处理用户输入 |
| `message_sending` | 发送给 LLM 前 | 修改提示词 |
| `tool_call_before` | 工具调用前 | 拦截或修改参数 |
| `tool_call_after` | 工具调用后 | 后处理结果 |
| `turn_end` | 每轮对话结束 | 记录日志、触发通知 |
| `session_end` | 会话结束 | 清理资源、生成报告 |

---

## 十、CLI 入口

### 10.1 命令结构

```
uncode [OPTIONS] [PROMPT]

OPTIONS:
    --model <MODEL>          指定 LLM 模型
    --session <ID>           恢复指定会话
    --issue <NUMBER>         从 GitHub Issue 开始工作
    --print                  非交互 print 模式（默认）
    --interactive, -i        交互 TUI 模式
    --config <PATH>          指定配置文件路径
    --version                显示版本
    --help                   显示帮助

SUBCOMMANDS:
    completions <SHELL>      生成 Shell 补全脚本
    config                   管理配置
```

### 10.2 模式分发

```
main()
  ├── 解析 CLI 参数（clap）
  ├── 加载配置（~/.uncode/config.toml）
  ├── 注册 LLM 驱动（ProviderRegistry）
  ├── 注册内置工具（ToolRegistry）
  ├── 加载扩展（ExtensionLoader）
  ├── 创建/恢复会话（SessionManager）
  ├── 构建系统提示（SystemPromptBuilder）
  └── 模式分发：
       ├── --interactive → run_tui()
       ├── --print       → run_print()
       └── (future)      → run_rpc()
```

---

## 十一、未决架构决策

以下架构决策留待详细设计或实现阶段确定：

| 决策 | 待定项 |
|------|--------|
| TUI ↔ Platform 通信 | 确认 JSONL 文件桥接是否足以支持团队场景 |
| 扩展系统具体 API | WASM 宿主函数签名、权限模型 |
| Platform 前端框架 | React vs Vue |
| Platform 后端框架 | axum vs actix-web |
| 多 session 并行 | AgentLoop 多实例管理策略 |
| 分布式会话 | 多机器共享会话的方案 |

---

## 十二、核心设计决策（参考 Pi 工程分析）

以下章节记录了 uncode 的核心工程决策及其设计原因，参照 Pi 的 17 维度工程分析撰写。

### 12.1 Agent 循环：单层 ReAct + Compaction

```
turn 0: 加载上下文 → 构建系统提示
    ↓
loop: 检查压缩 → 构建请求 → 调用LLM（流式）→ 解析响应
    ├── 文本 delta → 广播 ContentDelta 事件
    ├── 工具调用 → 执行工具 → 追加结果 → continue loop
    └── Done → break
```

**设计决策：** 选择单层循环而非 Pi 的双层 while 结构。理由是当前阶段不需要 steering/followUp 消息队列；单 Agent 循环更简单，后续可通过 `stop.rs` 的 `StopCondition` trait 扩展。

### 12.2 工具系统：Trait + 注册表 + 过程宏

```rust
// 工具注册（内置）
tool_registry.register("read".into(), Arc::new(ReadTool::new()));

// 工具注册（扩展）
extension_api.register_extension(ext, hooks);
```

**设计决策：** 选择 Rust trait 而非声明式 Schema（Pi 用 TypeBox）。理由是 Rust 的编译时类型安全比运行时 Schema 校验更可靠；`#[tool]` 宏从函数签名自动推导 JSON Schema，弥补声明式的便利性缺失。

### 12.3 错误处理：{content, isError} 统一返回

```rust
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: String,
    pub is_error: bool,      // ← LLM 看到此标记自动反思
}
```

**设计决策：** 参照 Pi，工具成功和失败使用同一数据结构。LLM 在 ReAct 循环中看到 `is_error: true` 自动触发纠错推理，无需额外 Critic Agent。`AgentLoop` 将工具异常捕获后统一包装为 `is_error: true` 的 `ToolResult`。

### 12.4 上下文压缩：结构化摘要格式

```
[上下文摘要]
## 目标              — 当前任务目标
## 已完成             — Done 列表
## 进行中             — In Progress 列表
## 受阻               — Blocked 列表
## 关键决策           — 已做出的重要决策
## 后续步骤           — Next Steps
```

**设计决策：** 参照 Pi 的 `Goal/Progress/NextSteps` 格式。结构化的 Done/InProgress/Blocked 三种状态使 LLM 在后续轮次中能快速定位任务进展。

### 12.5 提示词结构：时间锚点 + 动态工具指南

```rust
SystemPromptBuilder::new()
    .base("你是 AI 编程助手。")
    .add_tool_guide(&tools)       // 动态注入活跃工具
    .add_context(&agents_content) // 项目上下文
    .add_skills(&skills)          // 用户定义的技能
    .add_rules("无 unsafe 代码")  // 项目规则
    .build()
```

**设计决策：** 参照 Pi 的动态 Guidelines + Skill 命令机制。`add_tool_guide()` 根据当前注册的工具集动态生成工具说明，禁用某工具后对应指南自动消失。`ContextLoader` 从工作目录向上遍历加载 AGENTS.md/CLAUDE.md。

### 12.6 会话持久化：JSONL + 分支

```
~/.uncode/sessions/{id}.jsonl
第1行: {"type":"header","id":"uuid","model":"deepseek-v3",...}
第2行: {"type":"message","role":"user",...}
第3行: {"type":"message","role":"assistant",...}
第N行: {"type":"branch","parent_id":"...","reason":"探索替代方案"}
```

**设计决策：** 参照 Pi 的 JSONL 追加写入策略。三个工程优势：
1. **崩溃不丢数据** — append-only，每行独立
2. **天然回放** — 从头读到尾 = 完整会话
3. **分支支持** — `SessionEntry::Branch` 记录分叉点，可从任意历史节点分叉

### 12.7 安全与权限：当前策略

| 有 | 无 |
|----|----|
| 进程超时保护（BashTool timeout） | 文件路径 Jail |
| tokio::time::timeout 中断 | 命令白名单/黑名单 |
| JSONL 本地文件存储 | 敏感信息脱敏 |

**设计决策：** 当前阶段把安全责任交给操作系统层（文件权限、进程隔离）。生产就绪前需要：
- Bash 工具增加 `allowed_paths` 校验
- Provider 配置中的 API key 使用 `secrecy::SecretString`
- Platform 团队模式增加认证中间件

### 12.8 可观测性：当前状态

| 有 | 无 |
|----|----|
| tracing 结构化日志 | 无 OpenTelemetry 集成 |
| Token 消耗记录（UsageInfo） | 无费用格式化展示 |
| AgentEvent 广播流 | 无指标仪表板 |

**设计决策：** 当前阶段以 tracing + AgentEvent 事件流为基础。Platform（Phase 3）将提供：
- 会话 Token 消耗趋势图
- 工具调用成功率统计
- 费用估算（按模型定价表计算）

### 12.9 评估体系：当前缺口

**当前状态：** 42 个单元测试覆盖类型/macro/工具/会话。无自动化 Agent 行为质量评估。

**需要补充：**
- Golden Set 测试：一组标准 Issue → 期望的代码变更，用于回归验证 Agent 行为
- 工具调用成功率基准：bash/read/write/edit 各工具的失败率趋势
- 提示词质量评估：`SystemPromptBuilder` 不同组合对 Agent 效果的 A/B 测试

---

## 十三、与 Pi 工程分析的对齐表

| Pi 决策 | uncode 实现 | 对齐度 |
|---------|------------|--------|
| 单体 ReAct + 扩展口 | AgentLoop + ToolRegistry + ExtensionApi | 90% |
| 双层 while + terminate | 单层 loop + StopCondition trait | 70% |
| {content, isError} | ToolResult.is_error | 100% |
| 声明式 Schema | `#[tool]` 宏 + JSON Schema 推导 | 80% |
| JSONL + 结构化压缩 | SESSION_SCHEMA + compaction 模块 | 90% |
| 会话分支 | SessionManager::branch_session | 80% |
| 异步 steering | ❌ 未实现 | 0% |
| 时间锚点 + 动态指南 | SystemPromptBuilder | 80% |
| Skill 命令 | SKILL.md 加载 | 70% |
| 扩展钩子（9个） | 8 个钩子定义 | 80% |
| 模型热切换 | ❌ 未实现 | 0% |
| 多模态 | ❌ 未实现 | 0% |
| 安全/可观测/评估 | ⚠️ 基础阶段 | 40% |

### 13.1 安全增强（Phase 5 实施中）

**API Key 保护：**
- LLM 驱动中的 `api_key: String` 字段标记为敏感
- 所有 Provider 的 `Debug` 输出不包含 API key
- 日志中自动脱敏（tracing filter 拦截 `Authorization:` 头）

**Bash 工具安全：**
- `timeout` 参数：默认 120s，通过 `tokio::time::timeout` 强制执行（已在 `std` 审查中修复遗留 bug）
- 进程树清理：`tokio::process::Command` 在 timeout 后自动 kill
- 后续：添加 `allowed_paths` 白名单校验

**Platform 安全：**
- 本地模式：仅监听 127.0.0.1
- 团队模式：Basic Auth 中间件（规划中）
- CORS：严格配置允许的来源域

**当前安全基线：**

| 已有 | 规划中 |
|------|--------|
| Bash 超时 + 进程保护 | 文件路径 Jail（allowed_paths） |
| tracing 日志脱敏 | secrect::SecretString 替换 String |
| 127.0.0.1 绑定 | Platform 认证中间件 |
