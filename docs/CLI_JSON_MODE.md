# JSON 输出模式

## 背景

AI 编码工具经常需要嵌入到其他工具链中（编辑器插件、CI 流水线、自动化脚本）。当前 uncode 只有人类可读的 TUI/REPL 输出，无法被程序消费。

参考项目 Pi 支持 `--mode json` 以 JSON 格式输出 LLM 响应和工具调用结果。

## 目标

- 支持 `--mode json` / `-m json` 以 JSON Lines 格式输出所有事件
- 输出可被 `jq`、脚本、编辑器插件等消费
- 覆盖完整的 Agent 生命周期：响应文本、工具调用、错误

## 设计

### CLI 参数

```
uncode --mode json "prompt"          JSON 模式执行
uncode --mode json --session s1      JSON 模式恢复会话
```

`--mode` 与 `--repl` / 默认 TUI 互斥。

### 输出格式

JSON Lines（每行一个 JSON 对象），方便流式消费：

```jsonl
{"type":"session_start","session_id":"abc123","model":"deepseek-v3","timestamp":"2026-05-15T10:00:00Z"}
{"type":"text_delta","content":"Hello"}
{"type":"text_delta","content":" world"}
{"type":"thinking_delta","content":"Let me analyze..."}
{"type":"tool_call_start","id":"tc1","name":"read_file","arguments":{}}
{"type":"tool_call_end","id":"tc1","name":"read_file","result":"file contents..."}
{"type":"usage","input_tokens":1500,"output_tokens":800}
{"type":"error","message":"file not found","recoverable":true}
{"type":"session_end","total_turns":3,"total_input_tokens":5000,"total_output_tokens":2000}
```

### 事件类型映射

| AgentEvent | JSON type |
|-----------|-----------|
| SessionStart | session_start |
| ContentDelta(Text) | text_delta |
| ContentDelta(Thinking) | thinking_delta |
| ToolCallStart | tool_call_start |
| ToolCallProgress | tool_call_progress |
| ToolCallEnd | tool_call_end |
| Error | error |
| TurnEnd | turn_end |
| SessionEnd | session_end |

### 实现方案

在 `uncode-cli` 中新增 `JsonMode` 结构体，实现事件消费：

```rust
struct JsonMode {
    output: Box<dyn Write>,
}

impl JsonMode {
    fn handle_event(&mut self, event: AgentEvent) {
        let json = serde_json::to_string(&event).unwrap();
        writeln!(self.output, "{json}").unwrap();
    }
}
```

- 订阅 `broadcast::Receiver<AgentEvent>`
- 每收到事件立即序列化输出（不缓冲）
- `output` 默认 stdout，可配置为文件

### 序列化

为 `AgentEvent` 及相关类型派生 `Serialize`。JSON 字段使用 snake_case。

## 验收标准

- [ ] `uncode --mode json "hello"` 输出 JSON Lines
- [ ] `jq` 可正确解析输出
- [ ] 包含完整事件流：文本、工具调用、错误、用量
- [ ] 无 JSON 语法错误（每行独立可解析）
- [ ] 退出码：0=成功，1=LLM 错误，2=工具错误
