# uncode 请求生命周期

> 用户输入 → Context 构建 → LLM 调用 → 流式响应 → 工具执行 | 基于源码分析，2026-05  
> 会话持久化以 **SurrealDB** 为准；下文「store」均指异步 `SessionStore`（非每会话单一 JSONL 文件）。Pi 对齐叙事见 [`../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md`](../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md)。

本文档追踪一条用户输入从 TUI 输入框出发，经 Agent 引擎处理，最终提交给 LLM 的完整链路。

---

## 架构总览

```
用户输入 "帮我优化 input.rs"
        │
        ▼
┌─ TUI 层 ─────────────────────────────────────────────┐
│  InputEditor → handle_submit() → submit_text()       │
│  回调 on_submit(text, cancel_token, model, sid)       │
└────────────────────┬─────────────────────────────────┘
                     │
                     ▼
┌─ Agent 层 — loop_engine.rs ──────────────────────────┐
│  run() 主循环                                         │
│  ├─ 构造 User Message                                 │
│  ├─ 持久化到 SessionStore（SurrealDB）                 │
│  ├─ build_context() 重建完整对话历史                   │
│  ├─ 注入 system prompt + workspace graph              │
│  ├─ 确定 thinking level                               │
│  ├─ uncode_ai::stream() → LLM                        │
│  ├─ 流式处理 StreamEvent                              │
│  ├─ 执行工具 → 结果加入对话                            │
│  └─ 下一 turn (steering / next_turn)                  │
└───────────────────────────────────────────────────────┘
```

---

## 第一阶段：用户输入（TUI 层）

**位置**：`crates/uncode-tui/src/lib.rs`

### 1.1 输入捕获

`InputEditor` 处理键盘事件，Enter 键触发 `InputAction::Submit(text)`。

### 1.2 消息提交

`handle_submit()` 根据文本前缀分发：

| 前缀 | 处理 |
|------|------|
| `/thinking` | 切换 thinking 可见性 |
| `/model name` | 切换 LLM 模型 |
| `/clear` | 清空对话 |
| `/new` | 新建 session |
| `/skill args` | 调用 Skill |
| 普通文本 | → `submit_text()` |

### 1.3 submit_text()

```rust
fn submit_text(&mut self, text: String, on_submit: &F) {
    if self.agent_busy {
        // Agent 忙碌 → 消息排队
        self.queue.enqueue(text, QueueType::FollowUp);
    } else {
        // Agent 空闲 → 直接提交
        self.last_user_input = Some(text.clone());
        self.agent_busy = true;
        self.chat.push_user_message(text.clone());
        let expanded = expand_file_refs(text);  // 展开 @file 引用
        let token = self.new_cancel_token();
        on_submit(expanded, token, self.model.clone(), self.session_id.clone());
    }
}
```

**两种队列模式**：
- `FollowUp`（单条排队）：Agent 完成当前工作后投递
- `Steering`（全部排队）：当前工具调用完成后立即投递

---

## 第二阶段：上下文构建（Agent 层）

**位置**：`crates/uncode-agent/src/loop_engine.rs:run()`

### 2.1 进入 run()

`on_submit` 回调触发 `AgentLoop::run()`：

```rust
pub async fn run(&mut self, user_input: String) -> UncodeResult<()> {
    // 1. 构造 User Message
    let user_message = Message::new(Role::User, vec![
        ContentBlock::Text { text: user_input }
    ]);

    // 2. 持久化到 SessionStore（SurrealDB）
    let user_entry = SessionEntry::Message(Box::new(MessageEntry {
        id: generate_entry_id(),
        parent_id: self.current_leaf.clone(),
        role: Role::User,
        content: user_message.content.clone(),
        timestamp: chrono::Utc::now(),
        usage: None,
    }));
    self.session_store.append_entry(&session_id, &user_entry).await;

    // 3. 通知 TUI
    self.emit(AgentEvent::MessageStart { role: Role::User, message_id: ... });
    self.emit(AgentEvent::MessageEnd { role: Role::User, message_id: ... });
```

### 2.2 重建对话历史

**位置**：`crates/uncode-agent/src/context_builder.rs:build_context()`

从 `SessionStore::load_entries` 加载所有历史 entry，重建 LLM 可用的消息列表（逻辑上等价于按序重放一条 JSONL 流）：

```
┌─ SessionStore（SurrealDB 内嵌条目序列）────────────────┐
│  Message(user,    "问题1")                             │
│  Message(assistant,"回答1")                           │
│  Message(user,    "问题2")                           │
│  Message(assistant,"回答2")                         │
│  Compaction(summary, first_kept_id, …)               │
│  Message(user,    "问题3")                           │
└────────────────────┬──────────────────────────────────┘
                     │
                     ▼
┌─ build_context() 输出 ────────────────────────────────┐
│                                                       │
│  messages = [                                         │
│    System("[上下文摘要]\n之前讨论了..."),   ← 压缩摘要  │
│    User("问题3"),                         ← 跳过旧消息 │
│    Assistant("回答3"),                    ← compaction │
│    User("帮我优化 input.rs"),             ← 新输入     │
│  ]                                                    │
│                                                       │
│  effective_thinking_level: Some(High)    ← session恢复 │
│  effective_model: Some("deepseek-v4-pro")              │
│                                                       │
└───────────────────────────────────────────────────────┘
```

**算法**：
1. 预扫描：找到最后一个 `CompactionEntry`，获取 `skip_before_id` 和摘要文本
2. 注入压缩摘要为第一条 System 消息
3. 遍历 entries：
   - `MessageEntry` → 还原为 `Message`（跳过 compaction 之前的旧消息）
   - `BranchSummaryEntry` → 注入为 System 消息
   - `ModelChangeEntry` → 跟踪当前模型
   - `ThinkingLevelChangeEntry` → 跟踪 thinking 级别
   - `CompactionEntry` → 已在预扫描中处理

### 2.3 注入项目上下文

```rust
// 注入 workspace graph bundle（项目文件结构）
if let Some(ref cache) = self.graph_cache {
    let graph = cache.get_or_build(&cwd).await;
    if !graph.nodes.is_empty() {
        let bundle = workspace_graph::select_bundle(
            &graph, &[], first_text(&user_message), cache.config()
        );
        messages.insert(0, bundle.to_system_message());
    }
}

// 注入 system prompt
messages.insert(0, Message::system(self.system_prompt.clone()));
```

**最终 messages 结构**（提交给 LLM）：

```
messages = [
  System(system_prompt),              // Agent 角色定义、规则
  System(project_structure_bundle),   // 项目文件结构
  System("[上下文摘要]\n..."),         // 压缩摘要（如有）
  User("问题3"),
  Assistant("回答3"),
  User("帮我优化 input.rs"),          // 本次输入
]
```

### 2.4 确定 Thinking Level

```rust
let model_thinking = if model.reasoning {
    Some(ThinkingLevel::High)     // 推理模型默认 High
} else {
    None                          // 非推理模型关闭
};
let thinking_level = effective_thinking_level.or(model_thinking);
```

推理模型（如 deepseek-v4-pro、claude-sonnet-4-6）默认启用 `High` 级别 thinking。

---

## 第三阶段：LLM 调用

**位置**：`crates/uncode-ai/src/`

### 3.1 构造请求

```rust
let context = Context {
    system_prompt: Some(self.system_prompt.clone()),
    messages,                            // 完整对话历史
    tools: self.tool_registry.definitions(), // 工具列表
};

let options = StreamOptions {
    api_key,
    temperature: Some(0.7),
    max_tokens: Some(8192),
    thinking_level,                      // thinking 级别
    ..StreamOptions::default()
};

let mut stream = uncode_ai::stream(model, &context, &options, &self.api_registry).await?;
```

### 3.2 API 路由

`uncode_ai::stream()` 根据 `model.provider` 字段选择对应的 API 实现：

| Provider | 协议实现 | 文件 |
|----------|---------|------|
| `deepseek` | OpenAI Completions | `providers/openai_completions.rs` |
| `openai` | OpenAI Completions | `providers/openai_completions.rs` |
| `anthropic` | Anthropic Messages | `providers/anthropic_messages.rs` |
| `google` | Gemini Generative AI | `providers/gemini_generative.rs` |
| `ollama` | Ollama Native | `providers/ollama_native.rs` |

每个实现将统一的 `Context` 转换为 Provider 特定的 HTTP 请求体：

```json
{
  "model": "deepseek-v4-pro",
  "messages": [
    {"role": "system", "content": "你是 uncode agent..."},
    {"role": "user", "content": "帮我优化 input.rs"}
  ],
  "tools": [{"type": "function", "function": {...}}],
  "stream": true,
  "reasoning_effort": "high",
  "temperature": 0.7,
  "max_tokens": 8192
}
```

### 3.3 超时与取消

- **超时**：`options.timeout_ms` 通过 `tokio::time::timeout` 包装 HTTP 请求（OpenAI、Anthropic、Gemini 均已支持）
- **取消**：`CancellationToken` 在 `tokio::select!` 中监听，触发时中断流处理

---

## 第四阶段：流式响应处理

**位置**：`crates/uncode-agent/src/loop_engine.rs:612-627`

### 4.1 事件循环

```rust
loop {
    if self.cancel_token.is_cancelled() {
        // 取消：保存部分内容，中断
        break;
    }
    match stream.next().await {
        Some(StreamEvent::ThinkingDelta(text)) => {
            current_thinking.push_str(&text);
            self.emit(AgentEvent::ContentDelta {
                delta_type: DeltaType::Thinking,
                content: text,
                content_index: None,
            });
        }
        Some(StreamEvent::TextDelta(text)) => {
            current_text.push_str(&text);
            self.emit(AgentEvent::ContentDelta {
                delta_type: DeltaType::Text,
                content: text,
                content_index: None,
            });
        }
        Some(StreamEvent::ToolCallStart { id, name }) => {
            pending_tool_calls.push((id, name, String::new()));
            self.emit(AgentEvent::ToolCallStart { ... });
        }
        Some(StreamEvent::ToolCallEnd(data)) => {
            pending_executions.push((id, name, data.arguments));
        }
        Some(StreamEvent::Done { reason }) => break,
        Some(StreamEvent::Error { reason, message }) => {
            self.emit(AgentEvent::Error { ... });
        }
        None => break,
    }
}
```

### 4.2 事件流向 TUI

Agent 通过 `broadcast::Sender<AgentEvent>` 发布事件，TUI 通过 `broadcast::Receiver<AgentEvent>` 订阅：

| StreamEvent | → AgentEvent | → TUI 效果 |
|---|---|---|
| `ThinkingDelta` | `ContentDelta(Thinking)` | 显示 "● Thinking..." + 内容流式渲染 |
| `TextDelta` | `ContentDelta(Text)` | "● Writing" + 文本流式追加 |
| `ToolCallStart` | `ToolCallStart` | 显示工具调用卡片 |
| `ToolCallDelta` | `ToolCallProgress` | 更新工具参数显示 |
| `ToolCallEnd` | `(加入 pending_executions)` | — |
| `Done` | — | 开始执行 pending tools |

---

## 第五阶段：工具执行与下一 Turn

### 5.1 工具执行

Done 事件到达后，`pending_executions` 中的工具按顺序执行：

```rust
for (id, name, arguments) in pending_executions {
    let tool = self.tool_registry.get(&name)?;
    let result = tool.execute(arguments).await?;
    self.emit(AgentEvent::ToolCallEnd {
        data: ToolCallEndEventData {
            tool_id: id,
            tool_name: name,
            result_summary: Some(result),
            duration_ms,
            status: ToolCallStatus::Success,
            ...
        }
    });
}
```

### 5.2 工具结果加入对话

工具执行结果作为 `Message(Role::Tool, [ToolResult { content }])` 追加到 session store，成为下一 turn 的上下文。

### 5.3 Steering / 下一 Turn

- **Steering**：工具结果后立即开始新的 LLM 调用（继续同一 turn）
- **Next Turn**：完成后等待用户输入或消费排队消息

```rust
// 消费排队消息
if let Some(next_input) = self.message_queue.lock().await.drain_follow_up().next() {
    // 递归调用 run() 处理排队消息
    self.run(next_input).await?;
}
```

---

## 关键设计决策

| 决策 | 选择 | 理由 |
|------|------|------|
| 会话持久化 | SurrealDB + 异步 `SessionStore`；JSONL 导入/导出 | 逻辑树与 Pi 同构；索引与多面访问；导出满足审计 |
| 上下文重建 | 从 store 按插入序还原 | 压缩后跳过旧消息，保留后续对话 |
| Thinking 默认 | 推理模型 High，非推理 Off | 平衡思考质量与 token 消耗 |
| 工具执行 | Done 后批量执行 | 避免流中断，保证工具调用完整 |
| 流式优先 | 所有 Provider 返回 BoxStream | 实时显示思考和文本，提升体验 |
| 超时控制 | `tokio::time::timeout` 包装 | 防止 LLM 请求无限挂起 |

---

*本文档基于 uncode 源码编写，2026-05。*
