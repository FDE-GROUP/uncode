# 会话管理层对比分析：uncode vs Pi

> 基于 Pi (`earendil-works/pi`) 和 uncode 当前代码事实的逐层对比。

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

### uncode 会话管理分层

```
crates/uncode-core/src/session.rs            ← 共享类型定义
  SessionHeader, SessionEntry, SessionMetadata,
  SessionTree, SessionNode, MessageEntry,
  BranchEntry, SystemEntry, SystemEventType

crates/uncode-session/src/                   ← 存储实现
  store.rs             SessionStore (JSONL 文件)
  manager.rs           SessionManager (高层封装)
  export.rs            HTML 导出

crates/uncode-agent/src/                     ← 上下文管理
  compaction.rs        上下文压缩 (简单阈值)
  loop_engine.rs       Agent 循环引擎
```

### 架构差异总结

| 维度 | Pi | uncode |
|------|-----|--------|
| 语言 | TypeScript | Rust |
| 存储抽象 | `SessionStorage` 接口 + 2 后端 (JSONL/Memory) | `SessionStore` 结构体 (仅 JSONL) |
| 仓库模式 | `SessionRepo` 接口 + 2 实现 | 无独立仓库层 |
| 会话包装 | `Session` 类包装任意 `SessionStorage` | `SessionManager` 直接持有 `SessionStore` |
| 编排层 | `AgentSession` (完整编排器) | `LoopEngine` (嵌入在 agent 循环中) |

## 2. 存储格式对比

### 文件布局

**Pi**：`~/.pi/agent/sessions/<cwd-hash>/<timestamp>_<sessionId>.jsonl`
- CWD 编码为安全目录名 (`/` → `-`)
- 文件名包含时间戳，支持按时间排序
- 跨项目会话隔离

**uncode**：`{base_dir}/{session_id}.jsonl`
- 扁平目录，文件名仅为 UUID
- 无 CWD 隔离
- 无时间戳排序支持

### 文件内容

**Pi JSONL 格式 (version 3)**：

```jsonl
{"type":"session","version":3,"id":"uuid","timestamp":"ISO","cwd":"/path","parentSession":"optional"}
{"type":"message","id":"a1b2c3d4","parentId":null,"timestamp":"ISO","message":{...}}
{"type":"message","id":"e5f6g7h8","parentId":"a1b2c3d4","timestamp":"ISO","message":{...}}
{"type":"leaf","id":"i9j0k1l2","parentId":"e5f6g7h8","timestamp":"ISO","targetId":"e5f6g7h8"}
```

**uncode JSONL 格式**：

```jsonl
{"type":"header","id":"uuid","created_at":"ISO","updated_at":"ISO","model":"deepseek-v3","title":null,"working_dir":"/path"}
{"type":"message","timestamp":"ISO","role":"user","content":[{"type":"text","text":"hello"}]}
{"type":"message","timestamp":"ISO","role":"assistant","content":[{"type":"text","text":"..."}],"usage":{...}}
{"type":"branch","timestamp":"ISO","parent_id":"uuid","reason":"explore alternative"}
```

### 格式差异

| 维度 | Pi | uncode |
|------|-----|--------|
| 版本控制 | `version: 3`，支持 v1→v2→v3 自动迁移 | 无版本号 |
| Entry 关系 | 树结构 (`id` + `parentId`) | 线性序列（无 id/parentId） |
| 当前位置 | `LeafEntry` 指针 | 无（隐式为最后一条） |
| 父会话 | `parentSession` 字段支持 fork 来源 | `BranchEntry.parent_id` 引用父会话 |
| Header 模型 | 无模型字段（模型通过 `ModelChangeEntry` 追踪） | `model` 字段固定在 Header |
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

### uncode：3 种 Entry 类型

| Entry 类型 | 说明 | 参与 LLM 上下文 |
|------------|------|-----------------|
| `MessageEntry` | 消息 (role + content) | ✓ |
| `SystemEntry` | 系统事件 (SessionStart/End/PhaseSummary/Error/Compaction) | ✗ |
| `BranchEntry` | 分支记录 (parent_id + reason) | ✗ |

### 差距分析

| Pi 功能 | uncode 状态 | 影响 |
|---------|------------|------|
| 思考级别变更追踪 | **缺失** | 切换 thinking_level 后无法从会话恢复 |
| 模型变更追踪 | **缺失** | 切换模型后无法从会话恢复 |
| 压缩摘要条目 | 部分实现（`SystemEventType::Compaction` 但无摘要内容/firstKeptEntryId） | 无法从压缩点重建上下文 |
| 分支摘要 | **缺失** | 切换分支时丢失上下文 |
| 自定义扩展条目 | **缺失** | WASM 扩展无法持久化状态 |
| 自定义扩展消息 | **缺失** | 扩展无法注入 LLM 可见消息 |
| 用户书签 | **缺失** | 无法标记/导航特定条目 |
| 会话名称 | Header.title 静态字段 | 无法动态重命名 |

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

### uncode：线性 + 外部分支

```
Session 是线性序列：
  条目无 id/parentId，仅追加
  分支通过创建新会话文件实现 (fork_session)
  BranchEntry 记录来源，但原会话和新会话独立
```

- **分支**：`fork_session()` 创建新 JSONL 文件，复制父会话头部信息
- **树构建**：`build_tree()` 通过 `BranchEntry.parent_id` 跨文件构建树
- **无原地分支**：每次分支产生新文件，无法在同一文件内切换分支

### 对比

| 维度 | Pi | uncode |
|------|-----|--------|
| 分支模型 | 原地树（单文件多分支） | 外部分支（每分支一个文件） |
| 分支成本 | O(1)，仅追加 LeafEntry | O(n)，复制整个会话 |
| 分支导航 | 即时，无需创建文件 | 需切换文件 |
| 分支可视化 | 树形 UI（折叠/展开） | 仅文件列表 |
| 历史保留 | 完整保留所有分支路径 | 分支后只有独立文件 |
| 上下文切换 | `moveTo()` + 分支摘要 | 加载不同文件 |

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

### uncode：LoopEngine 上下文构建

```
1. 加载全部 entries
2. 简单过滤：跳过 System/Branch 类型
3. 全部 MessageEntry 加入 messages
4. 无压缩边界处理
5. 无分支摘要注入
```

### 对比

| 维度 | Pi | uncode |
|------|-----|--------|
| 路径选择 | 树路径（leaf → root） | 全量线性加载 |
| 压缩处理 | `firstKeptEntryId` 精确边界 | 无边界，全部加载后压缩 |
| 分支摘要 | 自动注入 | 不支持 |
| 模型恢复 | 从 `ModelChangeEntry` | 从 Header 固定字段 |
| 思考级别恢复 | 从 `ThinkingLevelChangeEntry` | 不支持 |
| 扩展消息 | `CustomMessageEntry` 参与 | 不支持 |

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

```
触发条件：estimate_context_tokens > 80% × context_window

1. 阈值检测：字符数/4 估算 token 数
2. 简单保留：保留最近 5 个回合
3. LLM 摘要：对旧消息生成摘要
4. 替换：旧消息 → 系统摘要消息
5. 无文件追踪
6. 无分割回合处理
7. 无迭代摘要合并
```

### 对比

| 维度 | Pi | uncode |
|------|-----|--------|
| 触发条件 | `contextTokens > window - reserveTokens` | `tokens > 80% × window` |
| Token 估算 | 精确：优先用 usage 数据，回退到 chars/4 | 粗略：chars/4 |
| 保留策略 | 按 token 量保留（20k tokens） | 按回合数保留（5 回合） |
| 切割点 | 精确选择（排除 toolResult） | 无精确切割 |
| 分割回合 | 双摘要合并 | 不处理 |
| 迭代摘要 | 合并上一轮摘要 | 每次重新生成 |
| 文件追踪 | 读写/编辑文件列表 + XML 标记 | 无 |
| 持久化 | CompactionEntry + firstKeptEntryId | SystemEntry(Compaction) 无摘要 |
| 上下文重建 | 从 firstKeptEntryId 精确重建 | 重建时无法跳过已压缩部分 |
| 可配置性 | 3 个设置项 | 无配置 |
| 扩展钩子 | `session_before_compact` | 无 |

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

**不支持。** 分支切换时完全丢弃原分支上下文。

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

### 差距

| Pi 功能 | uncode |
|---------|--------|
| `BashExecution` 独立消息类型 | Bash 结果通过 ToolResult 传递 |
| `CustomMessage` 扩展注入 | 不支持 |
| `BranchSummaryMessage` | 不支持 |
| `CompactionSummaryMessage` | 不支持（压缩后直接替换消息） |
| `convertToLlm()` 转换桥 | 无独立转换层，消息直接传给 LLM |

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

无 CWD 校验。会话 Header 存储 `working_dir` 但不验证其存在。

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

无版本号，无迁移机制。格式变更需要手动处理或忽略旧会话。

## 12. 会话操作对比

| 操作 | Pi SessionManager | uncode SessionManager |
|------|-------------------|----------------------|
| 创建 | `create()` / `inMemory()` | `create_session()` |
| 打开 | `open(path)` | 通过 `SessionStore` 读取 |
| 恢复最近 | `continueRecent()` | `find_most_recent()` + 加载 |
| 列表 | `list()` / `listAll()` | `list_sessions()` |
| 删除 | `delete()` | 无（需手动删除文件） |
| Fork | `forkFrom()` 支持指定位置 | `fork_session()` 简单复制 |
| 追加消息 | `appendMessage()` | `append_entry()` |
| 分支 | `branch(id)` / `branchWithSummary()` | 无原地分支 |
| 导航 | `getTree()` + `moveTo()` | 无 |
| 命名 | `appendSessionInfo()` 动态 | Header.title 静态 |
| 压缩 | `appendCompaction()` | `SystemEntry(Compaction)` |
| 导出 | HTML + JSONL | HTML |
| 统计 | `getSessionStats()` 详细 | `message_count()` 仅计数 |

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

```
crates/uncode-session/src/tests.rs   基础存储和管理测试
```

### 对比

| 维度 | Pi | uncode |
|------|-----|--------|
| 测试文件数 | ~15 | 1 |
| 存储层测试 | ✓ (JSONL + Memory) | ✓ (仅 JSONL) |
| 分支测试 | ✓ (原地树 + 摘要) | ✓ (简单 fork) |
| 压缩测试 | ✓ (切割/分割回合/迭代) | 无 |
| CWD 测试 | ✓ | 无 |
| 迁移测试 | ✓ | 无 |
| 上下文构建测试 | ✓ | 无 |
| 选择器测试 | ✓ (3 个) | 无 |

## 14. 整体差距评估

### 功能覆盖率

| 模块 | Pi | uncode | 覆盖率 |
|------|-----|--------|--------|
| JSONL 存储 | ✓ | ✓ | **100%** |
| 内存后端 | ✓ | ✗ | 0% |
| 会话仓库 | ✓ | ✗ | 0% |
| 树结构 (id/parentId) | ✓ | ✗ | 0% |
| 原地分支 | ✓ | ✗ | 0% |
| Leaf 指针 | ✓ | ✗ | 0% |
| 9 种 Entry 类型 | ✓ | 3 种 | **33%** |
| 上下文构建 (树路径) | ✓ | 线性加载 | **30%** |
| 压缩 (精确算法) | ✓ | 简单阈值 | **25%** |
| 分支摘要 | ✓ | ✗ | 0% |
| 模型/思考级别追踪 | ✓ | ✗ | 0% |
| CWD 校验 | ✓ | ✗ | 0% |
| 版本迁移 | ✓ | ✗ | 0% |
| 会话资源清理 | ✓ | ✗ | 0% |
| 扩展消息注入 | ✓ | ✗ | 0% |
| 用户书签 | ✓ | ✗ | 0% |
| 会话统计 | ✓ | 部分 | **30%** |
| HTML 导出 | ✓ | ✓ | **100%** |
| 测试覆盖 | ~15 文件 | 1 文件 | **~7%** |

### 总体评估

**uncode 会话管理覆盖率约 20%。** Pi 的会话层是一个成熟的、面向生产级使用的设计，支持树形分支、精确压缩、分支摘要、扩展集成等高级功能。uncode 目前实现了基础的 JSONL 持久化和简单分支，但在会话模型的表达能力和智能化管理方面存在显著差距。

### 关键差距优先级

| 优先级 | 差距 | 理由 |
|--------|------|------|
| **P0** | 树结构 (id/parentId + LeafEntry) | 所有高级功能的基础，无此则无法实现原地分支、精确压缩 |
| **P0** | CompactionEntry (summary + firstKeptEntryId) | 当前压缩无法从压缩点重建上下文，每次加载全量 |
| **P1** | ModelChangeEntry / ThinkingLevelChangeEntry | 会话恢复后丢失模型和思考级别设置 |
| **P1** | 分支摘要 | 切换分支时丢失上下文，用户体验断裂 |
| **P2** | CWD 校验 | 恢复会话时可能使用不存在的目录 |
| **P2** | 版本号 + 迁移 | 格式变更后无法向后兼容 |
| **P2** | CustomEntry / CustomMessageEntry | WASM 扩展系统需要持久化和消息注入 |
| **P3** | 内存存储后端 | 测试和临时会话场景需要 |
| **P3** | 会话资源清理 | 扩展生命周期管理 |
| **P3** | 用户书签 (LabelEntry) | 大型会话导航体验 |
