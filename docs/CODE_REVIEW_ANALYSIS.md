# crates 代码审查报告

> 生成时间：2026-05-16 | 审查范围：全部 11 个 crate（72 个源文件）
> 修订时间：2026-05-16 | 基于代码交叉验证修正

## 构建与测试状态

- `cargo check --workspace` — ✅ 通过
- `cargo test --workspace` — ✅ 全部通过（213 tests）
- `cargo clippy --workspace --all-features` — ⚠️ 13 个 warning（0 error），涉及 `uncode-session`、`uncode-agent`、`uncode-tui`、`uncode-rpc`

---

## 一、严重缺陷

### 1.1 uncode-tools — 全局路径遍历漏洞

- **文件**：`crates/uncode-tools/src/lib.rs:26-33`
- **严重度**：🔴 Critical
- **验证**：✅ 已确认

`resolve_path` 函数未验证解析后的路径是否在项目根目录内。恶意输入 `../../../etc/passwd` 可通过所有工具（read、write、edit、find、grep、ls）读写任意文件。`read.rs:49-56` 中存在该逻辑的完全重复实现。

**修复方向**：使用 `fs::canonicalize` 后检查路径前缀，或引入 `path-clean` 等 crate 进行路径规范化验证。

---

### 1.2 uncode-tools — read 工具 OOM 风险

- **文件**：`crates/uncode-tools/src/read.rs:88-97`
- **严重度**：🔴 Critical
- **验证**：✅ 已确认

文件内容**先通过 `read_to_string` 完全读入内存**（line 88），**之后**才检查大小限制（line 92）。若用户指向 10GB 日志文件，进程会先耗尽所有可用内存才被拒绝。

**修复方向**：在 `read_to_string` 之前使用 `fs::metadata(&resolved)?.len()` 进行快速检查。

---

### 1.3 uncode-tools — grep include 过滤器失效

- **文件**：`crates/uncode-tools/src/grep.rs:55-58`
- **严重度**：🟠 High（功能缺陷，非安全漏洞）
- **验证**：✅ 已确认

`include` 参数仅处理 `*.ext` 格式的字符串，使用 `strip_prefix("*.")` 提取扩展名。任何其他模式（`**/*.rs`、`src/*.rs`、`file.txt`）将导致**所有文件被静默跳过，返回零结果**。该逻辑完全没有测试覆盖。

**修复方向**：使用 glob 匹配库来解析 `include` 参数，而非手工字符串前缀提取。

---

### 1.4 uncode-llm — Gemini API key 安全漏洞

- **文件**：`crates/uncode-llm/src/providers/gemini.rs:68-69`
- **严重度**：🔴 Critical / Security
- **验证**：✅ 已确认

API key 通过 URL query string 传递：`format!("...&key={}", self.api_key)`。该密钥会被 HTTP 代理、负载均衡器、CDN 及所有中间跳转的服务器访问日志记录。Google Gemini API 支持 `x-goog-api-key` 请求头作为安全替代方案。此外，密钥未进行 URL 编码，含特殊字符时会静默失败。

**修复方向**：将 API key 移至 `x-goog-api-key` HTTP 请求头。

---

### 1.5 uncode-llm — 6/7 提供商缺少工具调用支持

- **文件**：`providers/anthropic.rs`, `openai.rs`, `openrouter.rs`, `glm.rs`, `gemini.rs`, `ollama.rs`
- **严重度**：🔴 High
- **验证**：✅ 已确认

仅 DeepSeek 实现了完整的工具调用（tool calls）解析。其余 6 个提供商：
- 均未在请求体中发送 `tools` 定义
- 流式解析器只处理 `delta.content`（文本），完全忽略 `delta.tool_calls`
- Ollama 和 Gemini 甚至不允许工具调用

这是整个 `uncode-llm` crate 最大的功能缺口。

**修复方向**：在 `common.rs` 中添加工具定义序列化，在各提供商 SSE 解析器中添加 `delta.tool_calls` 处理（参考 `deepseek.rs` 的状态机实现）。

---

### 1.6 uncode-llm — Ollama 流式输出被禁用

- **文件**：`crates/uncode-llm/src/providers/ollama.rs:34`
- **严重度**：🔴 High
- **验证**：✅ 已确认

请求体中 `"stream": false` 导致 Ollama 以单次阻塞请求返回完整响应，TUI 被冻结直至完成。Ollama 原生支持 `"stream": true` 的 SSE 流式输出，应启用。

---

### 1.7 uncode-llm — 工具参数序列化 Bug

- **文件**：`crates/uncode-llm/src/providers/common.rs:51`
- **严重度**：🔴 High
- **验证**：✅ 已确认

使用 `tc.arguments.to_string()`（即 `Display` trait）序列化 `serde_json::Value`：
- `Value::String("hello")` 产生 `"hello"`（带引号），嵌入 JSON 后成为 `"arguments":"\"hello\""`（双重引号，格式错误）
- `Value::Object({...})` 产生紧凑 JSON（正确）
- `Value::Number(42)` 产生 `"42"`（不带引号，但应符合规范）

行为在变体间不一致。

**修复方向**：统一使用 `serde_json::to_string(&tc.arguments)`。

---

### 1.8 uncode-agent — loop_engine 取消流程数据丢失

- **文件**：`crates/uncode-agent/src/loop_engine.rs:347`
- **严重度**：🟠 High（仅 tool_results 丢失）
- **验证**：⚠️ **原报告过度陈述，已修正**

`tokio::select!` 取消分支中的 `break` 跳出的是**内层流处理循环**，而非外层 turn 循环。

**实际影响**：
- ✅ 流式输出的部分文本/思考内容**已保存**（cancel 分支在 break 前将 current_text/current_thinking 推入 messages，见 line 241-253）
- ❌ `tool_results` 确实丢失（跳过了 line 398 的 `tool_results.push()`）
- 当前 turn 已完成的工具执行结果被丢弃

**修复方向**：break 前将已收集的 tool_results 推入 messages。

---

### 1.9 uncode-agent — `is_error: false` 硬编码

- **文件**：`crates/uncode-agent/src/loop_engine.rs:402`
- **严重度**：🟠 High
- **验证**：✅ 已确认

工具执行失败时返回 `ToolResult { content: "error: ...", is_error: false }`。`is_error` 始终硬编码为 `false`。后续重试/路由逻辑无法区分成功与失败结果。

**修复方向**：根据 `exec_result` 的错误状态正确设置 `is_error`。

---

### 1.10 uncode-agent — 流式 JSON 损坏时静默回退

- **文件**：`crates/uncode-agent/src/loop_engine.rs:444`
- **严重度**：🟠 High
- **验证**：✅ 已确认

`serde_json::from_str(arguments).unwrap_or_default()` — 流式 JSON 损坏时静默产生 `Value::Null`（注意：不是 `Value::Object({})`，而是 `Value::Null`），工具将以空/null 参数执行，可能导致不可预期的副作用。

**修复方向**：解析失败时记录日志并跳过执行，或发出 `StreamEvent::Error`。

---

### 1.11 uncode-core — 模板变量注入

- **文件**：`crates/uncode-core/src/skill.rs:75-77`, `template.rs:66-68`
- **严重度**：🟡 Medium（变量来源为项目配置文件，非外部用户输入）
- **验证**：✅ 已确认，**严重度已降级**
- **降级理由**：模板变量来自 `UNCODE.md`/`AGENTS.md` 项目配置文件，由项目开发者控制，非不可信的外部输入

变量替换使用循环 `result.replace(&format!("{{{{{key}}}}}"), value)`。若变量值自身包含 `{{other_var}}`，会在后续迭代中被替换为 `other_var` 的值：

```
vars = {"a": "{{b}}", "b": "injected"}
template: {{a}}
result: injected（而非 {{b}}）
```

**修复方向**：使用单次扫描替换所有变量（收集所有占位符后一次性替换），而非循环调用 `str::replace`。

---

### 1.12 uncode-core — UTF-8 字节截取（已有防护）

- **文件**：`crates/uncode-core/src/context.rs:128-134`
- **严重度**：🟢 Low（代码已有修正逻辑）
- **验证**：⚠️ **原报告未提及后续修正，已修正**

`&content[..max_bytes]`（line 128）确实按字节索引切片，但 **line 130-134 紧接着用 `char_indices().last()` 回退到最后一个完整字符边界**。当前代码不会产生 UTF-8 panic。

```rust
let truncated = &content[..max_bytes];                    // line 128
let truncated = truncated                                 // line 130-134
    .char_indices().last()
    .map(|(i, _)| &content[..i])
    .unwrap_or(truncated);
```

**注意**：`char_indices().last()` 回退到 max_bytes 之前的最后一个字符边界，而非之后。这意味着如果 max_bytes 切断了多字节字符，该字符会被**丢弃**而非 panic。这是正确但保守的行为，无需立即修复。

---

### 1.13 uncode-core — `Message → MessageEntry` 静默丢失 ID

- **文件**：`crates/uncode-core/src/session.rs:70-79`
- **严重度**：🟠 High
- **验证**：✅ 已确认

`From<Message> for MessageEntry` 转换中丢弃了 `Message.id` 字段。`MessageEntry` 有 `timestamp` 但无 `id`。这意味着无法通过 ID 关联工具调用结果与其请求，也无法实现消息的幂等去重。

**修复方向**：在 `MessageEntry` 中添加 `id: Option<String>` 字段。

---

### 1.14 uncode-tui — 权限系统实为虚设

- **文件**：`crates/uncode-tui/src/permission.rs:109`
- **严重度**：🔴 Critical
- **验证**：✅ 已确认

`confirm(_choice: ConfirmOption)` 方法的 `_choice` 参数被**完全忽略**，无条件调用 `self.pending.take()`。`lib.rs` 中 `.confirm(Allow)` 和 `.confirm(Edit)` 的区分被丢弃。**权限系统从不拒绝任何操作**。

**修复方向**：根据 `_choice` 参数实际分发允许/拒绝/编辑行为。

---

### 1.15 uncode-tui — 键盘事件 I/O 错误静默

- **文件**：`crates/uncode-tui/src/lib.rs:371-374`
- **严重度**：🟠 High
- **验证**：✅ 已确认

`event::read().unwrap_or(Event::Key(KeyEvent::new(KeyCode::Null, ...)))` 将所有 I/O 错误静默吞掉。终端断开时此路径会无限循环产生 Null 键事件，CPU 空转 100% 且无法正常退出。

**修复方向**：I/O 错误时发出退出信号或优雅终止。

---

### 1.16 uncode-tui — 增量渲染 O(n) 克隆

- **文件**：`crates/uncode-tui/src/chat.rs:332`
- **严重度**：🟡 Medium（性能问题，非功能缺陷）
- **验证**：✅ 已确认，**严重度已降级**
- **降级理由**：仅在流式追加最后一条消息时触发，非每条消息每帧都克隆

增量渲染路径每帧克隆整个已缓存行向量 `self.line_counts[idx].cached_lines.clone()`。流式输出数万行时，每 50ms 重新分配整个 Vec，导致帧率下降。

**修复方向**：使用 `std::mem::take` 取走所有权，或在 `cached_lines` 被覆盖前避免完整克隆。

---

## 二、中度缺陷

### 2.1 安全/稳定性

| 文件 | 行号 | 问题 |
|------|------|------|
| `tools/write.rs` | 46 | **非原子写入**：直接用 `fs::write` 覆盖，崩溃或磁盘满时文件被截断损坏。应采用"写临时文件 → rename"模式 |
| `tools/edit.rs` | 44 | **空字符串匹配**：`content.matches("")` 返回 `content.len() + 1`（字符间空位数量），空字符串 old_string 会触发"发现多处匹配"误报 |
| `tools/grep.rs` | 64 | **无大小阈值**：将每个匹配文件完整读入内存，大文件导致 OOM |
| `tui/chat.rs` | 293 | **unwrap panic 风险**：`msg_lines.pop().unwrap()` 假设 Vec 非空，边界条件不可靠 |
| `core/session.rs` | 137 | **递归无界类型**：`SessionNode.children: Vec<SessionNode>` 恶意导入可导致栈溢出 |
| `llm/common.rs` | 51 | **图片数据丢失**：图像内容被替换为 `"[image]"` 字符串占位 |
| `cli/main.rs` | 243 | **GitHub API 路径错误**：工作目录为 `/` 时仓库名为空字符串 |

### 2.2 逻辑正确性

| 文件 | 行号 | 问题 |
|------|------|------|
| `agent/token.rs` | 36-44 | 模型定价使用 `contains` 子串匹配，拼写错误导致错误定价分配 |
| `agent/model_switch.rs` | 19 | 模型切换记录为 `SessionEnd` 事件类型，语义错误 |
| `agent/steering.rs` | 42 | `wait_follow_up()` 方法名含"wait"但使用 `try_recv()`（非阻塞） |
| `llm/anthropic.rs` | 32-34 + common.rs:22-28 | 系统提示词**同时出现在 `body["system"]`（Anthropic API 格式）和 `messages[]`（common.rs 作为 system role）中**，重复发送 |
| `llm/anthropic.rs` | 91-95 | 忽略 Anthropic 的 `input_json_delta` 和 `thinking_delta` 事件 |
| `llm/registry.rs` | 62 | 检查 provider 名而非 model ID 是否存在配置 |
| `core/skill.rs` | 107-160 | **手写 YAML 解析器**不支持多行字符串、列表嵌套等基本语法，脆弱易错 |
| `tui/message_queue.rs` | 50-57 | `drain_steering` 仅在测试中被调用，生产代码中为死代码 |
| `tui/tool_renderer.rs` | 348-365 | 自定义 JSON 解析器无法处理转义引号、部分 JSON、空格变体 |
| `tui/lib.rs` | 524 | `session_id[..8]` 当 ID 长度 < 8 时 panic |
| `tui/lib.rs` | 574-583 | 模型选择器 Enter 后只更新 `self.model`，未更新 `self.model_index`，Ctrl+P 循环错位 |
| `tui/highlight.rs` | 45-52 | 每行创建新 `HighlightLines` 实例，丢失多行语法状态（块注释、多行字符串） |
| `tui/diff_viewer.rs` | 112-159 | `diff --git` 行被当作文件路径显示 |
| `session/export.rs` | 153 | `**bold**` Markdown 解析产生未闭合的 `<strong>` 标签 |
| `session/store.rs` | 82 | 文件 mtime 失败静默回退为 1970-01-01 |
| `session/manager.rs` | 46-60 | `branch_session` 与 `fork_session` 功能高度重叠 |
| `extensions/loader.rs` | 13-21 | WASM 加载器为存根函数（始终返回 `Ok(0)`），核心功能未实现 |

### 2.3 已移除/修正的条目

| 原始条目 | 修正原因 |
|---------|---------|
| `tui/code_detail.rs` 未在 lib.rs 中声明 | ✅ **确认为真**，文件存在但未被 `pub mod` 声明，为死代码 |
| `llm/glm.rs:96` `[DONE]` 后 break 丢弃后续数据 | ⚠️ **已移除**：SSE 规范中 `[DONE]` 是流的终止标记，break 是正确行为，非 bug |

---

## 三、性能问题

### 3.1 O(N²) / 内存膨胀

| 文件 | 行号 | 问题 |
|------|------|------|
| `tui/chat.rs` | 332 | 增量渲染每帧克隆整个缓存行向量 |
| `tui/input.rs` | 390 | 每帧对全文做 `UnicodeWidthStr::width()` 的 O(n) 扫描 |
| `tui/highlight.rs` | 49-52 | 每行创建新的语法高亮器（1000 行文件 × 60 FPS = 60000 实例/秒） |
| `agent/loop_engine.rs` | 190 | 每轮完全克隆 `Vec<Message>` 发送给 LLM |
| `session/store.rs` | 149 | `load_entries` 将整个 JSONL 文件一次性读入内存，无流式处理 |
| `session/store.rs` | 295-349 | `get_metrics` 全表扫描所有 session 的全部 entries |
| `platform/main.rs` | 301-350 | `get_metrics` 同样全量加载所有 entries |
| `tools/grep.rs` | 64 | 将每个匹配文件完整读入内存后搜索 |

### 3.2 不必要的分配

| 文件 | 行号 | 问题 |
|------|------|------|
| `agent/compaction.rs` | 57-65 | 先收集为 `Vec<String>` 再 `join("\n")`，额外中间分配 |
| `core/skill.rs` | 74-78 | `render()` 先克隆模板再循环替换，N 个变量产生 N+1 次分配 |
| `core/template.rs` | 63-69 | 同上模式 |
| `session/store.rs` | 46-61 | `list_sessions` 先 `collect::<Vec<_>>()` 再过滤 |
| `session/store.rs` | 88-91 | `find_most_recent` O(N log N) 排序取最大，应改为 `max_by_key` O(N) |
| `tui/chat.rs` | 1053-1057 | 先全量 `.collect::<Vec<&str>>()` 再 `.take(100)` |
| `tui/chat.rs` | 589-606 | `MessageDelivered` 事件重建整个 `line_counts` 而非移除单条 |
| `tui/markdown.rs` | 456 | 对于宽列，`"─".repeat(*w + 2)` 分配超长字符串 |

---

## 四、代码质量

### 4.1 重复代码

| 位置 1 | 位置 2 | 重复内容 |
|--------|--------|----------|
| `llm/anthropic.rs:67-77` | `llm/deepseek.rs:114-126`, `gemini.rs:84-110`, `glm.rs:70-84`, `openai.rs:67-77`, `openrouter.rs:57-67` | 流构造模式 `bytes_stream().flat_map().chain()` 出现 6 次 |
| `tools/lib.rs:26-33` | `tools/read.rs:49-56` | `resolve_path` 函数完全重复 |
| `tools/read.rs:62-86` | `tools/ls.rs:29-50` | 目录列表逻辑重复 |
| `tui/chat.rs:1069-1077` | `tui/tool_renderer.rs:367-373`, `tui/permission.rs:136-143` | `extract_command` 出现 3 次 |
| `tui/selector.rs:91-97` | `tui/welcome.rs:67-73` | `centered_rect` 工具函数重复 |
| `core/config.rs:17-27` | `core/model.rs:4-12` | `ModelConfig` 与 `ModelInfo` 字段高度重复，应合并 |
| `core/skill.rs` 整体 | `core/template.rs` 整体 | `SkillRegistry` 与 `TemplateStore` API 镜像，可提取公共 trait |

### 4.2 魔法数字

| 文件 | 行号 | 值 | 含义 |
|------|------|-----|------|
| `tools/bash.rs` | 12 | `120` | 命令执行超时（秒），无命名常量 |
| `tools/find.rs` | 36 | `200` | 结果截断数量 |
| `tools/grep.rs` | 40 | `50` | 结果截断数量 |
| `tools/grep.rs` | 43 | `20` | 最大搜索深度 |
| `tools/ls.rs` | 34 | `500` | 目录条目截断数量 |
| `tools/read.rs` | 14 | `1048576`（1MB） | 默认最大文件大小 |
| `agent/compaction.rs` | 44 | `+ 4` | 含义不明 |
| `tui/chat.rs` | 34 | `37` | 截断行数，含义不明 |

### 4.3 设计问题

| 文件 | 问题 |
|------|------|
| `agent/loop_engine.rs:116-523` | `run()` 方法 ~400 行，应拆分为 `process_stream_event()`、`handle_tool_call_end()` 等 |
| `agent/compaction.rs:81` | `use` 语句写在函数内部而非文件顶部 |
| `llm/stream.rs` | 空文件，文件存在但无内容，也未在 lib.rs 中声明为模块 |
| `core/error.rs` | 错误变体使用裸 `String` 而非结构化类型 |
| `core/tool.rs:18-21` | `ToolExecutor` trait 使用非类型化的 `serde_json::Value` 传递参数和返回值 |
| `core/session.rs` | 缺少统一的 `Session { header, entries }` 结构体来组合头部和条目 |
| `core/event.rs` | `TaskStatus` 派生 `PartialEq`，但 `DeltaType`、`ProgressType` 等未派生 |
| `tui/code_detail.rs` | 文件存在但**未在 `lib.rs` 中声明为模块**，是完全的死代码 |
| `tui/lib.rs:651-749` | `handle_submit` 98 行硬编码 match，与 `slash.rs` 注册系统并行维护 |
| `tui/permission.rs` | `request_confirmation` 创建确认请求但**从未渲染权限对话框 UI** |
| `tui/slash.rs:3` | `CommandFn` 对静态函数使用 `Box<dyn Fn>`，可改为 `fn(&str) -> String` |
| `tui/theme.rs` | `SyntaxColors` 结构体字段定义但未被 highlight.rs 使用（syntect 有自己的配色系统） |
| `llm/Cargo.toml:25` | `eventsource-stream` 依赖声明但未使用，增加编译时间 |
| `macros/src/lib.rs:89-93` | 过程宏只处理 `Pat::Ident`，跳过元组解构等模式 |
| `macros/src/lib.rs:140-143` | 未知类型静默回退为 `"string"` 而非产生编译错误 |

---

## 五、Clippy Warning 清单

| crate | 类型 | 说明 |
|-------|------|------|
| `uncode-session` | `while_let_on_iterator` | 循环可写为 `for` 循环 |
| `uncode-session` | `unnecessary_sort_by` | 可用 `sort_by_key` 替代 |
| `uncode-agent` | `incompatible_msrv` | 使用了 stable since 1.91.0 的特性，MSRV 为 1.85.0 |
| `uncode-tui` | `unnecessary_map_or` × 2 | `chat.rs:263,276` 可分别用 `is_none_or` / `is_some_and` |
| `uncode-rpc` | `needless_question_mark` × 3 | `Ok(...?)` 可简化为去掉外层 `Ok` 和 `?` |

---

## 六、建议修复优先级

### P0 — 立即修复

1. **路径遍历** (`tools/lib.rs:26-33`) — 安全威胁，影响所有文件操作
2. **权限系统虚设** (`tui/permission.rs:109`) — 无条件批准工具调用
3. **read OOM** (`tools/read.rs:88-97`) — 可用性崩溃
4. **grep include 过滤器失效** (`tools/grep.rs:55-58`) — 功能完全不可用

### P1 — 尽快修复

5. **Gemini API 密钥泄漏** (`llm/gemini.rs:69`) — 安全漏洞
6. **loop_engine 取消流程 tool_results 丢失** (`agent/loop_engine.rs:347`) — 数据丢失（注：文本/思考内容已保存）
7. **Message ID 丢失** (`core/session.rs:70-79`) — 数据完整性
8. **Ollama 流式禁用** (`llm/ollama.rs:34`) — 用户体验退化
9. **工具参数序列化 Bug** (`llm/common.rs:51`) — 功能正确性
10. **TUI I/O 错误静默** (`tui/lib.rs:371-374`) — 稳定性
11. **Anthropic 系统提示词重复** (`llm/anthropic.rs:32-34` + `common.rs:22-28`) — API 调用冗余
12. **模型选择器未同步 model_index** (`tui/lib.rs:574-583`) — Ctrl+P 循环错位

### P2 — 本迭代修复

13. 非原子写入 (`tools/write.rs`)
14. `is_error: false` 硬编码 (`agent/loop_engine.rs:402`)
15. JSON 损坏静默回退 (`agent/loop_engine.rs:444`)
16. 6/7 提供商缺少工具调用 (`llm/` 多文件)
17. 流构造模式重复 (`llm/` 6 个文件重构)
18. 增量渲染 O(n) 克隆优化 (`tui/chat.rs:332`)

### P3 — 后续迭代

19. 消除所有重复代码（`resolve_path`、`ModelConfig`/`ModelInfo`、`SkillRegistry`/`TemplateStore` 等）
20. 魔法数字替换为命名常量
21. `code_detail.rs` 死代码清理
22. `loop_engine.rs` 方法拆分
23. `load_entries` 流式改造 (`session/store.rs`)
24. `eventsource-stream` 依赖清理
25. 模板注入防护 (`core/skill.rs`, `core/template.rs`)

---

## 七、修订记录

| 条目 | 原始描述 | 修正内容 |
|------|---------|---------|
| 1.8 | "已收集的 tool_results 全部丢失 + 流式输出的部分文本/思考内容全部丢失" | 文本/思考内容在 break 前已保存，仅 tool_results 丢失 |
| 1.11 | 严重度 🔴 Critical | 降级为 🟡 Medium：变量来源为项目配置文件，非外部用户输入 |
| 1.12 | "UTF-8 字节截取 panic" | 降级为 🟢 Low：line 130-134 已有 `char_indices()` 回退修正逻辑 |
| 1.16 | 严重度 🔴 High | 降级为 🟡 Medium：仅流式追加时触发，非每帧每消息 |
| 2.2 | `glm.rs:96` break 丢弃后续数据 | 移除：SSE `[DONE]` 是终止标记，break 是正确行为 |
| 2.2 | `llm/anthropic.rs:32-34` 系统提示词同时出现在两处 | ✅ 确认：`common.rs:22-28` 放入 messages[], `anthropic.rs:32-34` 放入 body["system"]，确有重复 |
| 2.2 | `tui/lib.rs:847-848` 模型切换未更新 model_index | ✅ 确认：selector Enter 只更新 self.model，不影响 self.model_index |
