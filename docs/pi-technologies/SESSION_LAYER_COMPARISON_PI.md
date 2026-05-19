# 会话管理层对比分析：uncode vs Pi

> 基于 Pi (`earendil-works/pi`) 和 uncode 的对比。

---

> **uncode 侧勘误（2026-05）**  
> 下文关于 uncode 的 **「仅 JSONL 文件存储」「无 parent_id」「uncode-session / crates 路径」** 等描述针对**旧稿/早期实现**，已与当前仓库不符。  
> **请以** [`../uncode-technologies/UNCODE_SESSION_MODEL.md`](../uncode-technologies/UNCODE_SESSION_MODEL.md) **及** [`../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md`](../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md) **为准**：uncode 会话 **逻辑** 为树状 `SessionEntry`，**物理** 默认 **SurrealDB**，JSONL 用于 **导入/导出**。  
> 下列章节仍保留 **Pi** 侧细节与历史对比框架；阅读 uncode 结论时请以上述两篇为 SSOT。

---

## 1. 架构总览

### Pi 会话管理分层

```
packages/agent/src/harness/session/          ← 底层存储抽象
  session.ts           Session 类 + buildSessionContext
  jsonl-storage.ts     JsonlSessionStorage (JSONL 文件后端)
  jsonl-repo.ts        JsonlSessionRepo (会话仓库)
  memory-storage.ts    InMemorySessionStorage (内存后端)
  memory-repo.ts       InMemorySessionRepo (内存仓库)
  repo-utils.ts        共享工具 (UUID, fork 逻辑)
  uuid.ts              UUIDv7 生成

packages/coding-agent/src/core/              ← 上层编排
  session-manager.ts   SessionManager (完整会话管理)
  session-cwd.ts       CWD 校验
  messages.ts          扩展消息类型 + convertToLlm()
  agent-session.ts     AgentSession (中央编排器)
  compaction/          compaction.ts + branch-summarization.ts + utils.ts

packages/ai/src/
  session-resources.ts 会话资源清理注册表
```

### uncode 会话管理分层（当前实现 SSOT）

```
crates/uncode-core/src/session.rs            ← SessionHeader, SessionEntry, …
crates/uncode-agent/src/session/
  store.rs             SessionStore → SurrealSessionStore（SurrealDB）
  surreal_store.rs     嵌入式 DB 表与查询
  import.rs / export   JSONL 迁移与导出
  manager.rs           SessionManager
crates/uncode-agent/src/
  compaction.rs        压缩 + 文件追踪
  loop_engine.rs       Agent 循环
```

（历史稿中 `crates/uncode-session`、纯 JSONL `SessionStore` 已移除。）

### 架构差异总结

| 维度 | Pi | uncode |
|------|-----|--------|
| 语言 | TypeScript | Rust |
| 存储抽象 | `SessionStorage` 接口 + JSONL/Memory | `SessionStore` + SurrealDB（嵌入式）；JSONL 互操作 |
| 仓库模式 | `SessionRepo` 接口 + 2 实现 | `SessionStore` 薄封装 `SurrealSessionStore` |
| 会话包装 | `Session` 类包装任意 `SessionStorage` | `SessionManager` + 异步 `SessionStore` |
| 编排层 | `AgentSession` (完整编排器) | `LoopEngine` (嵌入在 agent 循环中) |

## 2. 存储格式对比

### 文件布局

**Pi**：`~/.pi/agent/sessions/<cwd-hash>/<timestamp>_<sessionId>.jsonl`
- CWD 编码为安全目录名 (`/` → `-`)
- 文件名包含时间戳，支持按时间排序
- 跨项目会话隔离

**uncode（运行时）**：数据在 **SurrealDB** 数据目录中（见 `SessionStore::default_dir()`）；**无**「每会话一个 `{session_id}.jsonl`」的线上布局。  
**uncode（导出）**：可导出为 JSONL 行序列，便于与 Pi 式工具对照或审计。

**uncode（历史/迁移）**：`import_jsonl_dir` 可摄入旧 JSONL 目录。

### 文件内容

**Pi JSONL 格式 (version 3)**：

```jsonl
{"type":"session","version":3,"id":"uuid","timestamp":"ISO","cwd":"/path","parentSession":"optional"}
{"type":"message","id":"a1b2c3d4","parentId":null,"timestamp":"ISO","message":{...}}
{"type":"message","id":"e5f6g7h8","parentId":"a1b2c3d4","timestamp":"ISO","message":{...}}
{"type":"leaf","id":"i9j0k1l2","parentId":"e5f6g7h8","timestamp":"ISO","targetId":"e5f6g7h8"}
```

**uncode 导出 JSONL 示例**（形状随版本演进，以 `UNCODE_SESSION_MODEL` 为准）：

```jsonl
{"type":"header","id":"uuid","created_at":"ISO","updated_at":"ISO","model":"deepseek-v3","title":null,"working_dir":"/path"}
{"type":"message","timestamp":"ISO","role":"user","content":[{"type":"text","text":"hello"}]}
{"type":"message","timestamp":"ISO","role":"assistant","content":[{"type":"text","text":"..."}],"usage":{...}}
{"type":"branch","timestamp":"ISO","parent_id":"uuid","reason":"explore alternative"}
```

### 格式差异

| 维度 | Pi | uncode |
|------|-----|--------|
| 版本控制 | `version: 3`，支持 v1→v2→v3 自动迁移 | Header `version` 与迁移逻辑见 `session/migration`；导入支持旧 JSONL |
| Entry 关系 | 树结构 (`id` + `parentId`) | 树结构（`id` + `parent_id`），与 Pi **逻辑同构** |
| 当前位置 | `LeafEntry` 指针 | `LeafEntry` + `set_leaf` / `get_leaf_id` |
| 父会话 | `parentSession` 字段支持 fork 来源 | `SessionHeader.parent_session` + `BranchEntry` |
| Header 模型 | 无模型字段（模型通过 `ModelChangeEntry` 追踪） | Header 含 `model`；另有 `ModelChangeEntry` 记录切换 |
| 时间戳格式 | ISO 字符串 | `DateTime<Utc>` (chrono) |

## 3. Entry 类型对比

### Pi：9 种 Entry 类型

| Entry 类型 | 说明 | 参与 LLM 上下文 |
|------------|------|-----------------|
| `SessionMessageEntry` | 包装 `AgentMessage` (user/assistant/toolResult) | ✓ |
| `ThinkingLevelChangeEntry` | 记录思考级别变更 | ✗ |
| `ModelChangeEntry` | 记录模型/provider 切换 | ✗ |
| `CompactionEntry` | 压缩摘要 + `firstKeptEntryId` | ✓ (转换为摘要消息) |
| `BranchSummaryEntry` | 分支导航时的摘要 | ✓ (转换为摘要消息) |
| `CustomEntry` | 扩展持久状态 | ✗ |
| `CustomMessageEntry` | 扩展注入的消息 | ✓ |
| `LabelEntry` | 用户书签标记 | ✗ |
| `SessionInfoEntry` | 会话显示名称 | ✗ |

### uncode：`SessionEntry` 枚举（与源码一致）

完整列表与语义见 [`../uncode-technologies/UNCODE_SESSION_MODEL.md`](../uncode-technologies/UNCODE_SESSION_MODEL.md)（含 `Compaction`、`BranchSummary`、`ModelChange`、`ThinkingLevelChange`、`Leaf` 等）。**旧稿「仅 3 种 entry」已作废。**

与 Pi 逐项能力差异的动态评估见 [`../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md`](../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md)。

## 4. 树结构与分支

### Pi：原地树分支

```
Session 是 append-only 树：
  entry.id + entry.parentId 形成树结构
  LeafEntry 指针跟踪当前位置
  branch() 只需移动 LeafEntry，不创建新文件
  同一会话文件内可包含多条分支路径
```

- **分支导航**：`/tree` 命令可视化树结构，支持跳转到任意历史节点
- **分支摘要**：导航时自动摘要被放弃的分支上下文
- **Fork**：从任意节点创建新会话文件，可指定 `position: "before" | "at"`
- **路径重建**：`getPathToRoot(leafId)` 从任意叶节点回溯到根

### uncode：树状条目 + SurrealDB 持久化

- `SessionEntry` 带 `id` / `parent_id`，`LeafEntry` 维护当前指针；`get_path_to_root` 等与树导航等价。  
- **Fork**：`fork_session` 创建新会话标识并写入分支元数据（非「每分支一个 `.jsonl`」的 Pi 文件心智）。  
- **分支摘要**：见 `branch_with_summary` / `BranchSummaryEntry`（`UNCODE_SESSION_MODEL.md`）。  
- **与 Pi 的差异**：Pi 单 JSONL 文件内多分支路径的可 grep 性更强；uncode 以 DB 为主、导出 JSONL 为辅。

### 对比

| 维度 | Pi | uncode |
|------|-----|--------|
| 分支模型 | 原地树（单文件多分支） | 树状 entry；物理为 SurrealDB 记录集 |
| 分支成本 | O(1)，仅追加 LeafEntry | 依存储实现；无整文件复制要求 |
| 分支导航 | 即时，无需创建文件 | TUI 会话选择 + DB 查询 |
| 分支可视化 | 树形 UI（折叠/展开） | 依赖产品 UI 演进 |
| 历史保留 | 完整保留所有分支路径 | 保留于 DB；可导出审计 |
| 上下文切换 | `moveTo()` + 分支摘要 | `set_leaf` + 分支摘要（见实现） |

## 5. 上下文构建 (Context Building)

### Pi：`buildSessionContext()`

```
1. getPathToRoot(leafId) → 获取当前分支路径
2. 遍历路径条目：
   - CompactionEntry → 生成 CompactionSummaryMessage，
     只保留 firstKeptEntryId 之后的条目
   - BranchSummaryEntry → 生成 BranchSummaryMessage
   - MessageEntry → 直接加入
   - CustomMessageEntry → 通过 createCustomMessage 包装
3. 提取最后的 thinkingLevel 和 model
4. 返回 { messages, thinkingLevel, model }
```

**关键特性**：
- 压缩边界精确：`firstKeptEntryId` 指定从哪里开始保留原文
- 分支摘要注入：切换分支时自动保留被放弃路径的摘要
- 模型/思考级别恢复：从路径中的 ChangeEntry 恢复最后设置

### uncode：`build_context()`（LoopEngine）

uncode 在 `context_builder` 中从 `SessionStore::load_entries` 重建消息：支持 **压缩边界**（`CompactionEntry` / `first_kept_entry_id`）、**分支摘要**（`BranchSummaryEntry`）、**模型与 thinking 级别**从对应 Entry 恢复。算法与字段以源码及 [`../uncode-technologies/UNCODE_SESSION_MODEL.md`](../uncode-technologies/UNCODE_SESSION_MODEL.md) 为准。

以下旧稿中的「全量线性加载」「无压缩边界」描述**作废**。

### 对比（定性）

| 维度 | Pi | uncode |
|------|-----|--------|
| 路径选择 | 树路径（leaf → root） | 自 `leaf` 与 `parent_id` 回溯（见实现） |
| 压缩处理 | `firstKeptEntryId` | `CompactionEntry` 语义对齐（见实现） |
| 分支摘要 | 自动注入 | `BranchSummaryEntry` + `[分支摘要]` 注入 |
| 模型 / 思考级别 | ChangeEntry | `ModelChangeEntry` / `ThinkingLevelChangeEntry` |
| 扩展消息 | `CustomMessageEntry` | `CustomMessageEntry`（类型存在；产品化程度见对齐评价文档） |

## 6. 上下文压缩 (Compaction)

### Pi 压缩算法

```
触发条件：contextTokens > contextWindow - reserveTokens (16,384)

1. 边界检测：找到上一个 CompactionEntry 的 firstKeptEntryId
2. 切割点选择：
   - 从最新条目倒推，累积 token 直到 keepRecentTokens (20,000)
   - 只在有效位置切割（user/assistant/custom/bashExecution）
   - 绝不在 toolResult 处切割
3. 分割回合处理：
   - 如果切割点落在回合中间 → 生成两份摘要后合并
4. 迭代摘要：
   - 如果存在上一轮压缩摘要 → 使用更新提示词合并
5. 文件追踪：
   - 从 tool call 中提取读写/编辑的文件列表
   - 追加到摘要末尾
6. 持久化：
   - 写入 CompactionEntry (summary + firstKeptEntryId)
   - 下次 buildSessionContext 从 firstKeptEntryId 开始读取
```

**可配置参数**：
- `compaction.enabled`: 是否启用（默认 true）
- `compaction.reserveTokens`: 预留 token（默认 16,384）
- `compaction.keepRecentTokens`: 保留近期 token（默认 20,000）

**扩展钩子**：`session_before_compact` 可拦截、取消或提供自定义摘要。

### uncode 压缩算法

实现位于 `crates/uncode-agent/src/compaction.rs`：阈值触发、**turn 边界**、**split-turn**、**文件读写追踪**、**迭代摘要**等与 Pi 同类的工程能力已落地；持久化为 `CompactionEntry`。细节以源码与 `UNCODE_SESSION_MODEL.md` 为准。

以下旧稿中的「仅保留 5 回合」「无文件追踪」描述**作废**。

### 对比（若需量化差异请对照源码行级注释）

| 维度 | Pi | uncode |
|------|-----|--------|
| 触发条件 | `contextTokens > window - reserveTokens` | `tokens > 80% × window`（可演进） |
| Token 估算 | usage 优先 | 实现见 `compaction.rs` |
| 切割点 | 排除 toolResult 等 | turn 边界 + split-turn 检测 |
| 文件追踪 | 有 | 有（`files_read` / `files_modified`） |
| 迭代摘要 | 有 | 有 |
| 扩展钩子 | `session_before_compact` | Hook 体系见 `uncode-extensions` / agent |

## 7. 分支摘要 (Branch Summarization)

### Pi

```
触发时机：/tree 导航切换分支时

1. 查找新旧位置的最近公共祖先
2. 收集旧叶节点到公共祖先之间的条目
3. 从最新到最旧遍历，按 token 预算选择条目
4. 摘要条目（compaction/branch_summary）可占预算的 90%
5. 生成结构化摘要 (Goal/Progress/Decisions/Next Steps)
6. 追加文件操作元数据
7. 写入 BranchSummaryEntry
```

- **累积文件追踪**：跨嵌套摘要的文件操作会被合并
- **扩展钩子**：`session_before_tree` 可拦截或提供自定义摘要

### uncode

支持 **`BranchSummaryEntry`** 与 `branch_with_summary` 流程（见 `UNCODE_SESSION_MODEL.md`）。与 Pi `/tree` 交互及扩展钩子的**产品化差异**见 [`../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md`](../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md)。

## 8. 消息类型对比

### Pi AgentMessage 类型

| 消息角色 | 说明 |
|----------|------|
| `user` | 用户文本/图片消息 |
| `assistant` | 助手回复 (text + thinking + toolCall) |
| `toolResult` | 工具执行结果 |
| `bashExecution` | Bash 命令执行（含 command/output/exitCode/truncated） |
| `custom` | 扩展自定义消息 |
| `branchSummary` | 分支摘要 |
| `compactionSummary` | 压缩摘要 |

所有消息通过 `convertToLlm()` 转换为标准 LLM `Message[]`。

### uncode ContentBlock 类型

| 内容块 | 说明 |
|--------|------|
| `Text` | 文本内容 |
| `Thinking` | 思考内容 |
| `ToolCall` | 工具调用 |
| `ToolResult` | 工具结果 |
| `Image` | 图片 |

### 差距（形态差异）

Pi 将会话内容建模为 **AgentMessage 列表**；uncode 将会话建模为 **树状 `SessionEntry`**，其中压缩与分支摘要以条目类型存在，而非「助手消息」形态。

| Pi 功能 | uncode |
|---------|--------|
| `BashExecution` 独立消息类型 | Bash 执行结果通常落在 `ToolResult`；非独立消息类型 |
| `CustomMessage` 扩展注入 | 弱于 Pi；扩展体系见 `uncode-extensions` |
| `BranchSummaryMessage` | 有 **`SessionEntry::BranchSummary`**（会话树条目），不是 Pi 式助手消息 |
| `CompactionSummaryMessage` | 有 **`SessionEntry::Compaction`**（摘要写入树），不是 Pi 式助手消息 |
| `convertToLlm()` 命名层 | 无同名层；向 LLM 组装消息在 agent / provider 路径完成 |

## 9. CWD 管理

### Pi

```typescript
// session-cwd.ts
getMissingSessionCwdIssue()  // 检测会话 CWD 是否仍存在
formatMissingSessionCwdError()  // 格式化错误信息
formatMissingSessionCwdPrompt()  // 提示用户选择
assertSessionCwdExists()  // 断言或抛出 MissingSessionCwdError
```

- 会话恢复时验证工作目录
- 提供优雅的降级选择（使用存储的 CWD 还是当前目录）

### uncode

`SessionStore::read_header` 在读取头信息时若 **`working_dir` 路径不存在** 会 **记录 warn** 并仍返回头（不阻塞、不提供 Pi 式「缺失 CWD 引导重选」）。工具层仍按进程 CWD 做沙箱解析。

## 10. 会话资源管理

### Pi

```typescript
// session-resources.ts
registerSessionResourceCleanup(cleanup) → unregister  // 注册清理回调
cleanupSessionResources(sessionId?)                     // 执行所有清理
```

- 全局注册表，扩展可注册清理回调
- 会话结束时统一执行
- 错误聚合：收集所有清理错误后一次性抛出 `AggregateError`

### uncode

无独立的会话资源管理机制。

## 11. 版本迁移

### Pi

```
v1 → v2: 为所有条目添加 id/parentId 树结构
v2 → v3: 重命名 hookMessage → custom（扩展统一）
```

- `migrateToCurrentVersion()` 在加载时自动执行
- 迁移是原位修改（in-place），无需备份

### uncode

- **主存**：SurrealDB 中的 `SessionHeader.version` 等字段随 schema 演进；无 Pi 那种「打开 JSONL 即自动 v1→v2→v3」的原位迁移链。  
- **JSONL 导入**：`import_jsonl_dir` / `import_single_jsonl` 将目录下 `.jsonl` 读入并按行 `append_entry`（若会话已存在则跳过）。  
- **v1→v2 树链修补**：`migration::migrate_v1_to_v2` 存在于 `migration.rs`，**当前主要供该模块的单元测试与离线修补场景**；`import_single_jsonl` **未直接调用** 它，旧文件能否无损导入取决于行内 JSON 是否已是带 `parent_id` 的 v2 形态。  
- 与 Pi 文件格式的 **逐版本对齐** 非目标；互操作见 [`../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md`](../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md)。

## 12. 会话操作对比

| 操作 | Pi SessionManager | uncode（`SessionManager` / `SessionStore`） |
|------|-------------------|----------------------|
| 创建 | `create()` / `inMemory()` | `SessionManager::create_session()` |
| 打开 | `open(path)` | `SessionStore::read_header` / `load_entries`（SurrealDB） |
| 恢复最近 | `continueRecent()` | `SessionStore::find_most_recent()` + 加载 |
| 列表 | `list()` / `listAll()` | `list_sessions()` |
| 删除 | `delete()` | 无对标的高层 `delete` API（需运维侧清理 DB 或后续扩展） |
| Fork | `forkFrom()` 支持指定位置 | `SessionStore::fork_session()`；`SessionManager::branch_session()` 创建子会话并写入 `Branch` 条目 |
| 追加消息 | `appendMessage()` | `append_entry()`（`SessionEntry::Message` 等） |
| 分支 | `branch(id)` / `branchWithSummary()` | `branch_session` + 树内 `Branch` / `BranchSummary` 条目（非 Pi 同构 API） |
| 导航 | `getTree()` + `moveTo()` | 存储已为树；**交互式 `/tree` 导航** 见 TUI/产品层，与 Pi 未必一一对应 |
| 命名 | `appendSessionInfo()` 动态 | `init_session_with_title` / 元数据字段；动态重命名能力弱于 Pi |
| 压缩 | `appendCompaction()` | `SessionEntry::Compaction` |
| 导出 | HTML + JSONL | **HTML**：`export_html`；**JSONL**：导入/互操作（`import_jsonl_dir`），非「目录即主库」 |
| 统计 | `getSessionStats()` 详细 | `message_count()` 等（较粗） |

## 13. 测试覆盖

### Pi

```
packages/agent/test/harness/
  session.test.ts           核心会话测试
  session-uuid.test.ts      UUIDv7 测试
  storage.test.ts           存储后端测试
  repo.test.ts              仓库测试

packages/coding-agent/test/
  session-manager/          6 个测试文件
    build-context, custom-session-id,
    file-operations, labels,
    migration, save-entry, tree-traversal
  session-cwd.test.ts
  agent-session-compaction.test.ts
  agent-session-branching.test.ts
  compaction*.test.ts       多个压缩测试文件
  session-selector-*.test.ts 3 个选择器测试
```

### uncode

测试与覆盖率以仓库为准：`crates/uncode-agent/src/session/tests.rs` 等。**旧稿中「uncode-session 单 crate、仅 JSONL」路径已失效。**

### 对比（定量表已弃用）

旧表中的 **uncode 列（测试文件数、仅 JSONL、无树结构等）** 基于过时实现，不再维护。若需对比测试范围，请直接对照 Pi `packages/**/test` 与 uncode `crates/uncode-agent` / `uncode-core` 的 `#[test]` / `#[tokio::test]`。

## 14. 整体差距评估（修订版）

uncode 在 **逻辑会话模型**（树状 `SessionEntry`、`Leaf`、`Compaction`、`BranchSummary`、模型/thinking 变更记录）上已对齐 Pi 的主干能力；**物理层**采用 SurrealDB 与 Pi 的「单目录 JSONL」不同，见 [`../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md`](../technologies/UNCODE_PI_ALIGNMENT_AND_EVALUATION.md)。

以下差距分析应 **按源码重新验证** 后使用；旧「~20% 覆盖率」结论**作废**。

### 仍可能存在的产品化差距（需个案确认）

| 优先级 | 主题 | 说明 |
|--------|------|------|
| 待确认 | Pi 式 `/tree` 交互与扩展钩子 | TUI 能力集是否与 Pi 对等 |
| 待确认 | 内存 Session 后端 | 测试是否沿用 `SessionStore::new_memory()` 即可满足 |
| 待确认 | 会话资源清理 / 扩展生命周期 | 与 `uncode-extensions` 路线图一致 |
| 待确认 | CWD 迁移与校验 | 与 `session-cwd` 类行为对齐程度 |

*本文档 Pi 侧内容仍可作参考；uncode 侧请以 `docs/uncode-technologies/` 与对齐评价文档为 SSOT。*
