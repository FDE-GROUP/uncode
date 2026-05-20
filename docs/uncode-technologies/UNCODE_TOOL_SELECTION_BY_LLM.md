# LLM 如何选择工具（uncode 视角）

> 说明「为什么 AI 会自己选工具」：责任边界、数据流、可配置项与常见误解。  
> 与实现层工具生命周期见 [`UNCODE_TOOL_SYSTEM.md`](UNCODE_TOOL_SYSTEM.md)；循环细节见 [`UNCODE_LOOP_ENGINE.md`](UNCODE_LOOP_ENGINE.md)。

---

## 1. 结论（先读这一段）

在 uncode 中，**没有**「用户说了 X，框架就固定调用 `read`」这类规则引擎。

- **大模型（LLM）** 在每一轮推理中决定：要不要用工具、用哪一个、参数是什么。
- **uncode** 决定：注册哪些工具、**哪些工具对模型可见**（active 集）、如何校验与执行、如何把结果写回对话。
- **用户** 通过自然语言任务、项目上下文（`AGENTS.md`、skills）和 CLI 开关，**间接**影响模型的选择倾向。

一句话：**uncode 负责端菜、验单、做菜；不负责替客人点菜。**

---

## 2. 责任边界

| 角色 | 职责 | 典型能力 |
|------|------|----------|
| **LLM** | 工具调用的决策方 | 根据对话与工具说明选择 `read` / `grep` / `bash` 等，生成 JSON 参数 |
| **uncode-agent** | 工具运行时 | `ToolRegistry`、`prepare` / `validate` / `execute`、并行批次、Hooks |
| **uncode-ai** | 协议适配 | 将 `ToolDefinition` 转为 OpenAI / Anthropic / Gemini 等 API 的 tools 字段 |
| **uncode-cli / TUI** | 产品入口 | 注册默认工具、`--tools` 限制 active 集、系统提示中的工具指南、权限确认 |
| **用户** | 任务与策略 | 提示词、工作目录、是否允许 bash、是否缩小工具集 |

**uncode 不能**在框架层替模型「选一次该用哪个工具」；**可以**缩小模型看到的候选列表（对齐 Pi 的 `setActiveTools`）。

---

## 3. 机制：Function Calling / Tool Use

现代 **Chat Completions** 类 API 在「对话补全」之上增加了 **工具调用（tool use / function calling）**：请求携带工具目录（name + description + JSON Schema），模型可返回 `tool_calls` 而非仅文本；宿主执行后把结果写回 `messages`，模型再推理。uncode 用 `Context.tools` 注入工具列表，流式对齐 Pi 的 `ToolCallStart` → `ToolCallDelta` → `ToolCallEnd`。

**选工具是模型推理的一部分**，不是 uncode 里的 `match` 分支。协议细节、与普通聊天的区别、各厂商字段差异见 **[附录 A](#附录-a-chat-completions-类-api-与工具调用)**。

---

## 4. 数据流（从注册到执行）

```mermaid
sequenceDiagram
    participant User as 用户
    participant CLI as CLI / Harness
    participant Reg as ToolRegistry
    participant Loop as AgentLoop
    participant LLM as LLM API
    participant Tool as ToolExecutor

    User->>CLI: 任务描述
    CLI->>Reg: register + set_active_tools
    CLI->>Loop: system_prompt + add_tool_guide
    Loop->>Reg: definitions() 仅 active 工具
    Loop->>LLM: Context { messages, tools }
    LLM-->>Loop: ToolCall(name, arguments)
    Loop->>Reg: prepare → validate → hooks → execute
    Reg->>Tool: execute
    Tool-->>Loop: ToolResult
    Loop->>LLM: tool 消息（结果）
    LLM-->>User: 文本或继续 tool_calls
```

### 4.1 每轮请求如何带上工具

`AgentLoop` 在发起流式请求前构造 `Context`，其中 `tools` 来自注册表的 **当前 active 定义**：

```rust
// crates/uncode-agent/src/loop_engine.rs（节选）
let tools = self.tool_registry.definitions();
let context = Context {
    system_prompt: Some(self.system_prompt.clone()),
    messages: messages.clone(),
    tools: tools.clone(),
};
```

`definitions()` 只返回 **active** 工具；未 active 的已注册工具（如默认不暴露的 `web_fetch`）**不会**出现在 API 的 tools 列表中，模型无法合法调用它们。

### 4.2 系统提示中的工具指南

CLI 构建系统提示时，会把 active 工具的名称与 `description` 写入「可用工具」章节，辅助模型理解语义（与 API schema 互补，不替代 schema）：

```rust
// crates/uncode-cli/src/main.rs（节选）
SystemPromptBuilder::new()
    .base("…遇到需要分析代码的任务时，请主动使用工具读取文件。")
    .add_tool_guide(&tool_registry.definitions())
    // …
```

实现见 `crates/uncode-agent/src/system_prompt.rs` 的 `add_tool_guide`。

### 4.3 驱动层序列化

`uncode-ai` 将 `Vec<ToolDefinition>` 按模型协议写入请求体（例如 OpenAI `tools`、Anthropic `tools`）。详见 [`UNCODE_LLM_LAYER.md`](UNCODE_LLM_LAYER.md) 与各 `providers/*.rs`。

---

## 5. 模型依据什么「选」工具？

选择是 **概率性、上下文相关** 的，常见信号如下。

| 信号类型 | 来源 | 作用 |
|----------|------|------|
| 工具 `description` | `ToolDefinition` | 自然语言说明用途（「正则搜索代码库」→ 倾向 `grep`） |
| 参数 JSON Schema | `ToolDefinition.parameters` | 约束字段名与类型，影响能否生成合法调用 |
| 用户消息 | 对话 | 「跑测试」→ 倾向 `bash`；「打开 foo.rs」→ 倾向 `read` |
| 历史轮次 | `messages` | 刚 `read` 过某文件，下一轮更可能 `edit` |
| 系统提示 | `SystemPromptBuilder` | 工作目录、语言、是否鼓励主动用工具 |
| 项目上下文 | `AGENTS.md`、skills | 团队约定的工作流（先 grep 再 read 等） |
| 截断提示 | 工具输出中的省略说明 | 大输出被截断时，模型可能改用 `read` 精读 |

**没有**单独的「工具选择器」模块；上述信号共同进入模型的单次 forward / 采样过程。

---

## 6. 模型选定之后：uncode 做什么？

流式事件解析出 `tool_name` + `arguments` 后，进入统一生命周期（对齐 Pi 顺序）：

1. **prepare_arguments** — 工具自定义规范化（可选）
2. **coerce** — 有限类型宽松转换（如字符串 → 整数，Pi `Value.Convert` 子集）
3. **validate** — 按 JSON Schema 校验 required / 类型 / enum 等
4. **before_tool_call** — Hooks（如 TUI 权限门控）
5. **execute** — `ToolExecutor`；同批中仅 **execute** 可并行（`Parallel` 模式）
6. 结果写入会话，作为 tool 消息进入下一轮 LLM 请求

若校验或 Hook 拒绝，**不会**改模型已选的名称，而是向模型返回错误文本，由模型决定是否换工具或改参数。详见 [`UNCODE_TOOL_SYSTEM.md`](UNCODE_TOOL_SYSTEM.md) § 执行生命周期。

整体模式即 **ReAct**：推理（Reason）→ 行动（Act）→ 再推理。

---

## 7. 我们能控制什么？

### 7.1 控制「菜单」，不控制「点菜」

| 手段 | 效果 | 说明 |
|------|------|------|
| `set_active_tools` / `clear_active_tools` | 限制对 LLM 可见的工具 | 对齐 Pi `setActiveTools`；`AgentLoop::set_active_tools` |
| CLI `--tools` / `-t` | 启动时指定 active 列表 | 与 `--no-tools` 互斥 |
| `--no-tools` | 不提供任何工具 | 模型只能纯文本回复 |
| `--no-builtin-tools` | 不注册 Pi 七件套 | 与自定义 `--tools` 组合使用 |
| 默认策略 | Pi 七件套 active | `read` `write` `edit` `grep` `bash` `find` `ls`；`web_*` 注册但默认不暴露 |
| `before_tool_call` Hook | 拦截**执行** | 可拒绝危险调用，不能替模型改名 |
| 强化 `description` / 系统提示 | 提高选对概率 | 产品侧调优，非确定性 |

### 7.2 与「Plan 模式」的关系

**Plan 模式**（若作为扩展/Harness 能力实现）通常也是：**规划阶段缩小 active 集**（只读），**执行阶段再放开** `write` / `edit`。  
这仍是限制候选工具，**不是** uncode 在代码里写死「第 3 步必须调用 grep」。

Plan 模式不属于工具系统内核；工具系统只提供注册表与 active 过滤能力。

---

## 8. 常见误解

| 误解 | 事实 |
|------|------|
| 「Agent 根据关键词选工具」 | 无关键词路由表；选择来自 LLM |
| 「TUI 帮用户选了 read」 | TUI 可在执行前 **确认/拒绝**，不改变模型已发出的 tool name |
| 「注册了 web_fetch 模型就会用」 | 默认未 active 时，模型 **看不到** 该工具 |
| 「validate 失败会换成别的工具」 | validate 失败返回错误给模型，由模型 **下一轮** 自行调整 |
| 「并行批次会改变选哪个工具」 | 并行只影响 **多個已选定** 工具的 execute 顺序与并发 |

---

## 9. 选错工具时如何改进

属于 **模型能力 + 提示工程 + 产品约束** 问题，而非单一 bug：

1. **写清 `description`** — 与 Pi 工具说明对齐，避免 `grep` / `find` 语义重叠含糊。
2. **缩小 active 集** — 只读任务用 `--tools read,grep,ls`，减少误用 `bash`。
3. **系统提示策略** — 明确「改代码前必须先 read」「禁止未经确认的 rm -rf」等（执行层仍靠 Hook）。
4. **权限门控** — 对 `bash` / `write` 做 TUI 确认（见 `tool_permission` / `permission_gate`）。
5. **换更强或更听指令的模型** — 同一套 tools 列表，不同模型工具调用质量差异很大。

---

## 10. 与 Pi 的对齐点

| Pi 概念 | uncode 对应 |
|---------|-------------|
| `setActiveTools(names)` | `ToolRegistry::set_active_tools` + `AgentLoop::set_active_tools` |
| 工具定义进入 LLM 请求 | `Context.tools` ← `definitions()` |
| 工具失败 → `isError` 反馈模型 | `ToolResult.is_error` + 消息回灌 |
| 无宿主侧「工具选择器」 | 同样由模型 function calling 决策 |

权威对照见 [`UNCODE_PI_ALIGNMENT_AND_EVALUATION.md`](../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md) § 工具系统。

---

## 附录 A：Chat Completions 类 API 与工具调用

本附录解释 §3 中「现代 Chat Completions 类 API 支持工具调用」具体指什么，与 uncode / Pi 类 Coding Agent 的关系。

### A.1 什么是 Chat Completions？

**Chat Completions（对话补全）** 指一类 HTTP API：客户端提交 `messages` 数组（`system` / `user` / `assistant` 等角色 + 内容），服务端返回模型的下一条 assistant 输出。OpenAI 的 `POST /v1/chat/completions` 是典型代表；DeepSeek、GLM、Groq、Ollama 兼容端点等多半沿用相同或极相近的请求/响应形状。

**早期**这类 API 的 assistant 输出几乎只有 **纯文本**。  
**后来**在同一条对话协议上扩展：assistant 除文字外，还可声明 **「请宿主代我调用某个工具」**——即 **工具调用（tool use）** 或 **function calling（函数调用）**。两个英文名常混用，在本仓库文档中指同一能力。

### A.2 与普通聊天有何不同？

| 维度 | 普通聊天 | 带工具调用的聊天 |
|------|----------|------------------|
| 模型输出 | 仅自然语言 | 文本 **和/或** 结构化 tool call |
| 宿主职责 | 展示回复 | 解析 tool call → **执行** → 结果回灌 |
| 对话轮次 | 常一轮问答 | 常 **多轮**：模型 → 工具 → 模型 → … |
| 副作用 | 无（仅生成文本） | 可有（读文件、跑 shell 等，由宿主执行） |

重要边界：**模型不在你的机器上执行工具**。它只输出意图，例如「调用 `read`，参数 `{"path":"src/main.rs","limit":20}`」。真正读文件的是 **Agent 宿主**（uncode 的 `ToolExecutor`）。

### A.3 请求里多了什么？

除 `model`、`messages` 外，增加 **工具目录**（各协议字段名略有不同，见 A.6）。每个工具条目通常包含：

| 字段 | 作用 |
|------|------|
| `name` | 调用标识，如 `read`、`bash` |
| `description` | 自然语言说明，供模型选择时匹配任务 |
| `parameters` | JSON Schema：参数对象有哪些属性、类型、必填项 |

OpenAI Chat Completions 风格（概念示例）：

```json
{
  "model": "gpt-4o",
  "messages": [
    { "role": "user", "content": "读一下 src/main.rs 前 20 行" }
  ],
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "read",
        "description": "Read file contents from the workspace.",
        "parameters": {
          "type": "object",
          "properties": {
            "path": { "type": "string", "description": "File path" },
            "limit": { "type": "integer", "description": "Max lines" }
          },
          "required": ["path"]
        }
      }
    }
  ]
}
```

uncode 侧：`ToolRegistry::definitions()` → `Context.tools` → `uncode-ai` 各 provider 序列化为上述结构。未进入 active 集的工具 **不会**出现在该列表中。

### A.4 响应里多了什么？

一次 completion 可能包含：

1. **assistant 文本**：`content` 或 content 块中的 text（「我先看一下这个文件…」）。
2. **工具调用**：结构化列表，例如 OpenAI 的 `message.tool_calls[]`，每项含 `id`、`function.name`、`function.arguments`（JSON 字符串，需解析）。

**流式（streaming）** 时不会一次性给出完整 tool call，而是分片到达。uncode 对齐 Pi，在驱动层统一为：

| 事件 | 含义 |
|------|------|
| `ToolCallStart` | 开始一次调用（已知 tool name / call id） |
| `ToolCallDelta` | 参数 JSON 片段增量 |
| `ToolCallEnd` | 参数完整，宿主可进入 prepare / validate / execute |

每条流必须以 `StreamEvent::Done` 结束（见 `uncode-ai` 流式约定）。

模型也可 **只回复文字、不调工具**；是否调用由模型根据任务与工具说明自行决定。

### A.5 执行完成后如何继续对话？

宿主执行工具后，必须把结果 **追加到 message 历史**，再发起下一次 Chat Completions 请求。各协议封装不同，语义一致：**「你上次请求的行动已完成，结果是 …」**。

| 协议风格 | 常见形态 |
|----------|----------|
| OpenAI Chat Completions | `role: "tool"` + `tool_call_id` + 结果字符串 |
| Anthropic Messages | `user` 消息中的 `tool_result` 内容块 |
| Google Generative AI | `functionResponse` 等 |

uncode 在 `loop_engine` 里把 `ToolResult` 转成当前模型协议要求的 message 形状，再进入下一轮 `stream_simple`。

模型读到结果后可以：

- 继续调用其他工具；
- 或直接向用户输出最终文字答案。

这就是 **ReAct**（Reason → Act → Re-Reason）；`AgentLoop` 的内外循环即实现该模式。

### A.6 术语：function calling vs tool use

| 术语 | 说明 |
|------|------|
| **function calling** | OpenAI 早期文档用语，把每个工具视作可调用的「函数」 |
| **tool use** | 更中性的行业说法，强调模型使用外部工具，不限于纯函数语义 |
| **tools**（字段名） | Anthropic、Google、OpenAI 新版本请求体中的工具数组 |
| **functions**（遗留） | 部分 OpenAI 兼容层仍使用 `functions` / `function_call` 命名 |

uncode 采用 **API-first**（见 [`UNCODE_LLM_LAYER.md`](UNCODE_LLM_LAYER.md)）：内部统一为 `ToolDefinition` + `StreamEvent`，按协议在 `openai_completions`、`anthropic_messages`、`gemini_generative`、`ollama_native` 中分别序列化，不为每个厂商单独写一套 Agent 逻辑。

### A.7 「现代」通常指哪些能力？

大致指 2023 中后期至今的主流产品能力（具体模型支持度不一）：

- 单次请求声明 **多个** 工具，由模型从中选择；
- **流式** 生成 tool call 参数，降低首字节延迟；
- 与 **扩展思考（thinking）**、多模态等并存于同一对话 API；
- 对 JSON Schema 约束的参数生成相对稳定（仍会偶发错字段、漏必填，需宿主 `validate`）。

并非所有模型都擅长工具调用：同一 `tools` 列表下，有的模型主动、准确，有的很少调用或参数混乱——属于 **模型能力** 与 **提示工程** 问题，不是 uncode 独有缺陷。

### A.8 与 uncode 的对应关系（小结）

```text
ToolDefinition (uncode-core)
    → definitions() 过滤 active
    → Context.tools (loop_engine)
    → provider 请求体 tools/functions
    → 模型返回 tool_calls / 流式 ToolCall*
    → ToolRegistry 执行
    → tool 结果 message
    → 下一轮 Context.messages
```

- **API 提供**：「可以调哪些工具、参数长什么样」。
- **模型提供**：「这一次调不调、调哪个、参数值是什么」。
- **uncode 提供**：注册、active 集、校验、Hooks、执行、回灌与会话持久化。

更深的产品层叙述见本文 §1–§7；实现层见 [`UNCODE_TOOL_SYSTEM.md`](UNCODE_TOOL_SYSTEM.md)、[`UNCODE_LOOP_ENGINE.md`](UNCODE_LOOP_ENGINE.md)。

---

## 11. 相关源码索引

| 主题 | 路径 |
|------|------|
| 每轮 `tools` 注入 | `crates/uncode-agent/src/loop_engine.rs` |
| active 过滤与校验 | `crates/uncode-agent/src/tools/registry.rs` |
| Pi 默认七件套 / CLI | `crates/uncode-agent/src/tools/builtin.rs` |
| 系统提示工具章节 | `crates/uncode-agent/src/system_prompt.rs` |
| CLI 组装 | `crates/uncode-cli/src/main.rs` |
| API 序列化 | `crates/uncode-ai/src/providers/*.rs`、`providers.rs` |
| 类型定义 | `crates/uncode-core/src/tool.rs`（经 `uncode-ai` re-export） |

---

## 12. 相关文档

- [`UNCODE_TOOL_SYSTEM.md`](UNCODE_TOOL_SYSTEM.md) — 工具 trait、生命周期、CLI、并行批次
- [`UNCODE_LOOP_ENGINE.md`](UNCODE_LOOP_ENGINE.md) — 内外循环、流式事件、工具批次
- [`UNCODE_LLM_LAYER.md`](UNCODE_LLM_LAYER.md) — API-first 驱动与 `StreamEvent`
- [`../guides/TOOL_SYSTEM.md`](../guides/TOOL_SYSTEM.md) — 面向使用者的工具指南
- [`../pi-technologies/PI_TOOL_SYSTEM.md`](../pi-technologies/PI_TOOL_SYSTEM.md) — Pi 侧工具管线对照

---

*文档版本：2026-05（含附录 A：Chat Completions / function calling）；基于 uncode 当前 Agent 循环与 ToolRegistry 实现编写。*
