# Steer/Interrupt 改进

## 背景

当前 uncode TUI 已有消息队列和 steering 消息机制（Agent 正在执行时用户可输入新消息），但体验不够直观：

1. 用户不知道当前是在"等待 Agent 完成"还是"可以输入新指令"
2. 没有 Interrupt（立即停止）功能
3. Follow-up 消息缺乏视觉反馈

参考项目 Pi 区分 Steer 模式（中断当前执行）和 Follow-up 模式（追加到队列）。

## 目标

- 支持明确的中断操作（Ctrl+C 停止当前 Agent 执行）
- 改进 Follow-up 输入的视觉反馈
- 区分"Agent 运行中"和"等待输入"状态

## 设计

### 状态模型

```
enum AgentState {
    Idle,                    // 等待用户输入
    Running { turn: u64 },   // Agent 正在执行第 N 轮
    Streaming,               // 正在接收流式响应
    Compacting,              // 正在压缩上下文
}
```

### 中断机制

**Ctrl+C 行为变更**：
- `Idle` 状态：退出 TUI（当前行为）
- `Running`/`Streaming` 状态：发送中断信号，停止当前 Agent 执行
  - 取消当前的 LLM 请求
  - 跳过后续工具调用
  - 保留已接收的部分响应
  - 回到 `Idle` 状态

**实现**：使用 `tokio::sync::CancellationToken`

```rust
pub struct AgentLoop {
    cancel_token: CancellationToken,
    // ...
}

impl AgentLoop {
    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }
}
```

在 Agent loop 的关键等待点检查 `cancel_token.is_cancelled()`。

### Follow-up 输入改进

当 Agent 处于 `Running` 状态时：
- 输入框显示不同颜色/样式（如黄色边框）
- 提示文字变为 "Follow-up (queued):"
- 用户输入的消息进入队列，Agent 完成当前轮次后处理

### 状态指示器

Footer 中显示当前 Agent 状态：
- `● IDLE` — 等待输入（绿色）
- `◉ RUNNING (turn 3)` — 正在执行（黄色）
- `◉ STREAMING` — 接收响应（蓝色）

## 验收标准

- [ ] Agent 运行中 Ctrl+C 中断执行，不退出 TUI
- [ ] 中断后保留已接收的部分响应
- [ ] Follow-up 输入有视觉区分（边框颜色/提示文字）
- [ ] Footer 显示 Agent 当前状态
- [ ] Idle 状态 Ctrl+C 正常退出
