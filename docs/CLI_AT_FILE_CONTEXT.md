# `@<file>` 文件上下文注入

## 背景

AI 编码助手的一个核心交互模式是将文件内容作为上下文注入对话。当前 uncode 需要用户通过 prompt 描述文件路径，依赖 Agent 的工具调用读取文件，效率低且不够直观。

参考项目 Pi 支持 `@<path>` 语法在输入中直接引用文件、目录、URL。

## 目标

- 支持 `@file_path` 语法在用户输入中引用文件
- 支持 `@dir_path` 引用目录（自动生成目录树摘要）
- 支持 `@url` 抓取 URL 内容
- 引用的内容作为用户消息的一部分发送给 LLM

## 设计

### 语法

```
@src/main.rs              注入文件内容
@src/lib/                 注入目录树 + 文件摘要
@https://example.com/api  抓取 URL 内容
```

在用户输入消息中，`@` 前缀的 token 被解析为引用，替换为实际内容。

### 解析规则

1. **Token 提取**：从用户输入中提取所有 `@<path>` 模式的 token
   - 路径以 `/` 或合法路径字符开头
   - 以空格、换行或行尾结束
   - URL 以 `http://` 或 `https://` 开头

2. **文件引用**：读取文件内容，替换为代码块格式
   ```
   <!-- @src/main.rs -->
   `rust
   fn main() { ... }
   `
   ```

3. **目录引用**：生成目录树 + 各文件首行摘要
   ```
   <!-- @src/lib/ -->
   src/lib/
   ├── mod.rs        // Library entry
   ├── driver.rs     // LLM driver trait
   └── providers/    // Provider implementations
   ```

4. **URL 引用**：HTTP GET 抓取内容，截断到合理长度（默认 10KB）

### 实现模块

**新增 `uncode-core/src/context.rs`**：

```rust
pub struct FileContext {
    pub path: String,
    pub content: String,
    pub context_type: ContextType,
}

pub enum ContextType {
    File,
    Directory,
    Url,
}

pub fn inject_contexts(input: &str, working_dir: &Path) -> (String, Vec<FileContext>)
```

**调用位置**：
- CLI: `main.rs` 中构建用户消息之前
- TUI: `lib.rs` 中提交用户输入时
- Agent: `loop_engine.rs` 可选，作为预处理步骤

### 安全限制

- 文件路径限制在工作目录内（防止 `@/etc/passwd`）
- URL 抓取限制大小（默认 10KB）
- 目录引用限制深度（默认 3 层）和文件数（默认 50）
- 二进制文件跳过（通过扩展名或 magic bytes 检测）

## 验收标准

- [ ] `@src/main.rs` 在消息中注入文件内容
- [ ] `@src/lib/` 注入目录树摘要
- [ ] `@https://...` 注入 URL 内容
- [ ] 工作目录外的文件引用被拒绝并提示
- [ ] 大文件/目录有合理截断
- [ ] TUI 和 CLI 都支持
