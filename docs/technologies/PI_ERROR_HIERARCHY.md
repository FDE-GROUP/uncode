# Pi 错误层级

> 6 种结构化错误类 + stable error codes

---

## 错误类

| 错误类 | 场景 |
|--------|------|
| `FileError` | 文件操作失败 |
| `ExecutionError` | shell 命令失败 |
| `CompactionError` | 上下文压缩失败 |
| `BranchSummaryError` | 分支摘要失败 |
| `SessionError` | 会话操作失败 |
| `AgentHarnessError` | harness 操作失败（如 busy guard） |

---

## 设计特点

- **Stable error codes**：每种错误有数字 code，跨版本稳定，便于程序化处理
- **Result 类型**：`ExecutionEnv` 的所有操作返回 `Result<T>`（不抛异常），错误通过 error code 传递
- **区分层级**：File/Execution 错误属于工具层，Compaction/BranchSummary 属于 Harness 层，Session/AgentHarness 属于编排层

---

*本文档基于 Pi 源码 (`@earendil-works/pi-agent-core`) 编写。*
