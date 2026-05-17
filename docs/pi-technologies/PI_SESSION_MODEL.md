# Pi Session 与 Compaction

> Session 树状模型（10 种 entry 类型）、核心操作、存储、Compaction 流程、Branch Summary

---

## Session 树状模型

### 数据结构

会话是**树状条目**，不是平坦列表：

```typescript
interface SessionTreeEntry {
    type: EntryType;
    id: string;           // UUIDv7（时间可排序）
    parentId: string | null;  // 形成树结构
    timestamp: string;    // ISO 格式
}
```

**10 种条目类型**：

| 类型 | 说明 |
|------|------|
| `message` | 用户/助手/工具消息 |
| `thinking_level_change` | 思考级别变更 |
| `model_change` | 模型变更 |
| `compaction` | 压缩摘要 |
| `branch_summary` | 分支摘要 |
| `custom` | 自定义数据 |
| `custom_message` | 自定义消息 |
| `label` | 标签标记 |
| `session_info` | session 元数据 |
| `leaf` | 当前活跃叶节点指针 |

### 核心操作

```typescript
interface Session {
    getBranch(fromId?): SessionTreeEntry[];   // 从 leafId 到 root 的路径
    moveTo(entryId, summary?): void;          // 切换活跃叶节点，可选生成摘要
    buildContext(): SessionContext;            // 重建消息数组 + effective model/thinkingLevel
    fork(options?): Session;                  // 从分支点创建新 session
}
```

分支是**隐含的**（通过 `leafId` 指向不同路径），不是显式对象。`buildContext()` 自动处理压缩条目（找到最近 CompactionEntry，注入 CompactionSummaryMessage，跳过旧条目）。

### 存储

| 后端 | 用途 |
|------|------|
| `JsonlSessionStorage` | 生产环境（CWD 编码目录结构） |
| `InMemorySessionStorage` | 测试环境 |

Model/ThinkingLevel 变更作为 session entry 持久化，`buildContext()` 回放恢复。

### Label 系统

`LabelEntry` 允许为条目打标签，`getLabel()` 通过 label cache 高效查找。

---

## Compaction（上下文压缩）

### 压缩流程

```
compact()
├── shouldCompact()              ← contextTokens > contextWindow - reserveTokens
│                                 reserveTokens=16384, keepRecentTokens=20000
├── findCutPoint()              ← 累积 token 向前找到截断位置（必须在 turn 边界）
├── prepareCompaction()         ← 构造压缩请求
│   ├── 检测 split-turn（cut 跨 turn 中间）
│   ├── 提取 file operations（read/write/edit）
│   └── 复用 previousSummary（增量更新）
├── generateSummary()           ← LLM 生成结构化摘要
│   ├── 首次：SUMMARIZATION_PROMPT
│   ├── 增量：UPDATE_SUMMARIZATION_PROMPT
│   └── split-turn 时：Promise.all([历史摘要, turn-prefix 摘要])
└── appendEntry(compactionEntry) ← 持久化到 session tree
```

### 摘要格式（8 节）

```
## Goal
## Constraints & Preferences
## Progress
### Done
### In Progress
### Blocked
## Key Decisions
## Next Steps
## Critical Context
```

增量更新语义：PRESERVE 已有、ADD 新信息、MOVE 项在 Done/In Progress 间、UPDATE Next Steps。

### File Operation Tracking

`extractFileOpsFromMessage()` 分析 assistant tool call 中的 read/write/edit 操作，跨压缩边界累积，写入摘要的 `<files_read>` / `<files_modified>` XML 标签。压缩后模型仍知道之前操作过哪些文件。

### Token 估算

混合策略：
- **Provider usage 优先**：使用最近 assistant message 的 `usage.totalTokens`
- **Chars/4 兜底**：provider 未报告时使用字符启发式（images 固定 4800 tokens）

### Branch Summarization

导航到不同分支时，`collectEntriesForBranchSummary()` 找到公共祖先，收集旧分支独有条目，生成结构化摘要。前缀 `"The user explored a different conversation branch before returning here."`

---

*本文档基于 Pi 源码 (`@earendil-works/pi-agent-core`) 编写。*
