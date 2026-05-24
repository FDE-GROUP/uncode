# Phase 5b 工程设计：EventRouter async hook dispatch

> **依赖**：Phase 3（EventRouter 结构 + sync dispatch）
> **预计工期**：0.5 天

---

## 一、目标

建立 async hook dispatch 管道：harness 层在 `prompt()` 执行期间用 `tokio::spawn` 后台消费事件流，调用 `dispatch_hooks()` 并 log 结果。本 Phase 仅建立管道，不实现 Block/Patch 实际控制。

---

## 二、改动清单

### 2.1 event_router 字段改为 Arc

**文件**：`crates/uncode-agent/src/loop_engine.rs`

将 `event_router: std::sync::Mutex<EventRouter>` 改为 `Arc<std::sync::Mutex<EventRouter>>`，使其可 clone 到 spawned task。

新增 `event_router_arc()` getter。

### 2.2 prompt() 中启动 hook dispatcher

**文件**：`crates/uncode-agent/src/harness.rs`

在 `agent.run()` 之前 subscribe 并 spawn 后台 task，run 结束后 abort。

---

## 三、文件变更

| 文件 | 改动 |
|------|------|
| `loop_engine.rs` | event_router 改为 Arc<Mutex>，新增 getter |
| `harness.rs` | prompt() 中 spawn hook dispatcher |

---

## 四、不做的事

- 不实现 HookResult::Block 实际拦截
- 不实现 PatchMessages / PatchToolResult
- 仅 log hook results
