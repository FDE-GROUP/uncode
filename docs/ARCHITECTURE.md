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
│   终端四面板交互界面   │   分析监控平台服务端               │
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
| `uncode-tui` | 终端 UI 四面板 | core + agent + session |
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
    ├──TaskUpdate──────────→ tokio::sync::broadcast ──→ TUI(任务清单面板)
    ├──ContentDelta────────→ tokio::sync::broadcast ──→ TUI(思考过程面板)
    ├──ToolCallStart───────→ tokio::sync::broadcast ──→ TUI(工具调用面板)
    ├──ToolCallProgress────→ tokio::sync::broadcast ──→ TUI(工具调用面板)
    ├──ToolCallEnd─────────→ tokio::sync::broadcast ──→ TUI(工具调用面板)
    ├──PhaseSummary────────→ tokio::sync::broadcast ──→ TUI(阶段总结面板)
    ├──Error───────────────→ tokio::sync::broadcast ──→ TUI(全局通知)
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
├── StatusBar（顶部状态栏：版本/会话/模型/Token/时间）
├── MainLayout
│   ├── TaskListPanel（左上：任务清单）
│   ├── ToolCallsPanel（右上：工具调用）
│   ├── ThinkingPanel（左下：思考过程）
│   └── SummaryPanel（右下：阶段总结）
├── InputEditor（底部命令行输入）
└── PopupOverlay
    ├── ErrorDialog
    ├── ConfirmDialog
    └── CodeDetailView（全屏/半屏代码细节）
```

### 7.2 渲染循环

```rust
struct TuiEngine {
    app_state: AppState,
    terminal: Terminal<CrosstermBackend<Stdout>>,
    event_rx: broadcast::Receiver<AgentEvent>,
    key_rx: mpsc::UnboundedReceiver<KeyEvent>,
}

impl TuiEngine {
    async fn run(&mut self) {
        loop {
            tokio::select! {
                event = self.event_rx.recv() => self.handle_agent_event(event),
                key = self.key_rx.recv() => self.handle_key_event(key),
            }
            self.render()?;
        }
    }
}
```

### 7.3 面板更新策略

| 面板 | 更新频率 | 触发条件 |
|------|---------|---------|
| 任务清单 | 低频 | TaskUpdate 事件 |
| 工具调用 | 中频 | ToolCallStart/Progress/End 事件 |
| 思考过程 | 高频（流式） | ContentDelta 事件 |
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
