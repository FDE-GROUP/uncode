# uncode 会话模型

> SessionEntry 树状模型 + SurrealDB 持久化 + JSONL 互操作 + 压缩摘要  
> 基于 `crates/uncode-core/src/session.rs` + `crates/uncode-agent/src/session/` 源码分析

> **L1 机制对齐（Pi）**：`SessionEntry` 树、`parent_id`、Compaction / BranchSummary 与 Pi 会话树**逻辑同构**；**物理存储**为 SurrealDB（非 Pi 的 JSONL 主存）。对照见 [`UNCODE_PI_MECHANISM_MAP.md`](UNCODE_PI_MECHANISM_MAP.md)、[`PI_SESSION_MODEL.md`](../pi-technologies/PI_SESSION_MODEL.md)。

uncode 的会话在 **逻辑上** 与 Pi 终端 harness 的「树状事件流」**同构**：`SessionEntry` 构成带 `parent_id` 的树，支持分支、压缩摘要、分支摘要与完整回放。  
**物理持久化** 默认使用 **嵌入式 SurrealDB v3**（`SurrealSessionStore`，`kv-rocksdb`），由异步 `SessionStore` 封装；**JSONL** 作为 **互操作格式**（旧版迁移导入、导出审计），而非线上主存储。

---

## 逻辑 vs 物理

| 层面 | 内容 |
|------|------|
| **逻辑（对齐 Pi）** | `SessionEntry` / `SessionHeader` 的语义、插入顺序、leaf 指针、`Branch` / `Compaction` / `BranchSummary` 等与 Pi 会话树一致的设计目标 |
| **物理（工程取舍）** | 条目以结构化文档存入 SurrealDB，支持索引与多客户端；调试可依赖 TUI/CLI **导出 JSONL** |

---

## SessionEntry 树状模型

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum SessionEntry {
    Message(Box<MessageEntry>),
    System(Box<SystemEntry>),
    Branch(Box<BranchEntry>),
    Leaf(Box<LeafEntry>),
    Compaction(Box<CompactionEntry>),
    ModelChange(Box<ModelChangeEntry>),
    ThinkingLevelChange(Box<ThinkingLevelChangeEntry>),
    BranchSummary(Box<BranchSummaryEntry>),
    Custom(Box<CustomEntry>),
    CustomMessage(Box<CustomMessageEntry>),
    Label(Box<LabelEntry>),
    SessionInfo(Box<SessionInfoEntry>),
}
```

所有 Entry 共享 `id: String`（UUIDv7）、`parent_id: Option<String>`、`timestamp: DateTime<Utc>`。通过 `parent_id` 链构成树。

### Entry 类型说明

| 类型 | 字段 | 用途 |
|------|------|------|
| `Message` | role, content, usage | 用户/助手/工具消息 |
| `System` | event (Start/End/PhaseSummary/Error/Compaction), data | 系统级事件 |
| `Branch` | parent_session_id, reason | 新会话分支 |
| `Leaf` | target_id | 树导航指针 |
| `Compaction` | summary, first_kept_entry_id, tokens_before, files_read, files_modified | 压缩摘要 |
| `ModelChange` | provider, model_id | 模型切换记录 |
| `ThinkingLevelChange` | thinking_level | 推理级别变更 |
| `BranchSummary` | from_id, summary | 被遗弃分支的结构化摘要 |
| `Custom` | custom_type, data | 扩展数据 |
| `CustomMessage` | custom_type, content, display | 扩展消息（可控制显示） |
| `Label` | target_id, label | 给 Entry 打标签 |
| `SessionInfo` | name | 会话元信息 |

---

## SurrealDB 持久化（主路径）

**位置**：`crates/uncode-agent/src/session/store.rs`、`surreal_store.rs`。

`SessionStore` 为薄封装，所有方法 **`async`**，在 tokio runtime 内调用：

```rust
pub struct SessionStore {
    inner: SurrealSessionStore,
}
```

典型流程：`SessionStore::new(base_dir)` → `init_session` / `append_entry` / `load_entries` / `get_leaf_id` / `set_leaf` / `fork_session` / `list_sessions` 等。具体表结构与索引见 `surreal_store.rs` 与迁移模块。

---

## JSONL 互操作（非主存储）

### 导入

`crates/uncode-agent/src/session/import.rs`：`import_jsonl_dir()` 将历史 **`sessions/*.jsonl`**（或等价布局）导入 SurrealDB，便于从早期仅 JSONL 部署迁移。

### 导出

TUI `/export jsonl`（见 `uncode-tui`）将会话条目序列化为 **JSON Lines**，便于 grep、外部分析或与 Pi 式工具链对接。

### 序列化形状（导出或与 Pi 对照时）

导出的每行仍是完整 `SessionEntry`（或头部 + 条目）的 JSON，与「若用纯 JSONL 文件作为主库」时的可读格式兼容；**线上写入路径**为 SurrealDB `append_entry`，不逐行写单一 `.jsonl` 文件。

### 与 Pi 主存储（JSONL）的对照

| 维度 | Pi | uncode |
|------|-----|--------|
| 线上主库 | 按会话目录 `sessions/*.jsonl` 追加 | SurrealDB（`SessionStore::append_entry`） |
| 树形条目 | `SessionEntry` 一行一条 | 同逻辑模型，见 [`SESSION_SCHEMA`](../SESSION_SCHEMA.md) |
| 迁移/备份 | 直接复制 JSONL | `import_jsonl_dir()` 导入；TUI `/export jsonl` 导出 |
| 互操作目标 | Pi CLI / 外部 grep 工具链 | 导出后与 Pi 式 JSONL **形状兼容**，非字节级同一文件布局 |

导入实现见 `crates/uncode-agent/src/session/import.rs`；行为级事件对照见 [`UNCODE_PI_MECHANISM_MAP`](UNCODE_PI_MECHANISM_MAP.md) §5–§6。

---

## SessionHeader

```rust
pub struct SessionHeader {
    #[serde(rename = "type")]
    pub entry_type: String,         // 固定 "session"
    pub id: String,
    pub version: u32,
    pub parent_session: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub model: String,
    pub title: Option<String>,
    pub working_dir: String,
}
```

---

## 关键操作（SessionStore API 语义）

| 操作 | 说明 |
|------|------|
| `init_session` / `init_session_with_title` | 新建会话元数据 |
| `append_entry` | 原子追加一条 `SessionEntry` |
| `load_entries` | 按会话加载完整条目序列（供 `build_context`） |
| `get_path_to_root` | 从任意 Entry 沿 `parent_id` 回溯到根 |
| `get_leaf_id` / `set_leaf` | 当前叶指针（分支导航） |
| `fork_session` | 新建子会话并写入 `BranchEntry` 语义 |

---

## 压缩系统

### 触发条件

```rust
fn should_compact_session(store, session_id, context_window) -> bool {
    let total_tokens = estimate_entries_tokens(entries);
    total_tokens > context_window * 80 / 100   // 超过 80% 触发
}
```

### 压缩流程

```
① 查找最后一次 CompactionEntry（支持迭代摘要）
② 计算保留量：context_window * 20%
③ find_cut_point()：从末尾向前累积 token
    └── 到达阈值后向前扫描到 User 消息边界（干净 turn 分割）
④ 收集 cut point 之前的所有 Entry
⑤ 从工具调用中提取 files_read / files_modified
⑥ generate_summary()：调用 LLM 生成结构化摘要
    └── 有旧摘要 → UPDATE_SUMMARIZATION_PROMPT
    └── 无旧摘要 → SUMMARIZATION_PROMPT
⑦ 持久化 CompactionEntry { summary, first_kept_entry_id, tokens_before, ... }
⑧ emit CompactionComplete 事件
```

### 摘要格式（8 节结构）

```
## 目标
## 约束与偏好
## 进展
### 已完成
### 进行中
### 受阻
## 关键决策
## 下一步
## 关键上下文
```

### 迭代摘要

当存在之前的 `CompactionEntry` 时，使用 `UPDATE_SUMMARIZATION_PROMPT`，将旧摘要包裹在 `<previous-summary>` 标签中传入，保留历史信息的同时节省 token。

---

## 上下文重建

`build_context()`（`context_builder.rs`）从 `SessionStore::load_entries` 重建 LLM 消息数组：

```
① 预扫描：找最后一个 CompactionEntry
② 将压缩摘要作为 System 消息 "[上下文摘要]" 注入
③ 遍历 Entry：
    CompactionEntry    → 跳过（已在预扫描处理）
    BranchSummaryEntry → 作为 System 消息 "[分支摘要]" 注入
    ModelChangeEntry   → 记录 effective_model（不注入消息）
    ThinkingLevelChangeEntry → 记录 effective_thinking_level
    MessageEntry       → 转换为 Message 注入
④ 跳过 cut point 之前的 Entry（压缩已替代）
⑤ 返回 BuiltContext { messages, effective_model, effective_thinking_level }
```

---

## 分支与摘要

### 分支操作

`fork_session(parent_id, reason)` 在 SurrealDB 中创建新会话记录，并建立与父会话的 `Branch` 语义链接（具体字段见 `surreal_store` 实现）。

### 分支摘要

`branch_with_summary()` 在分支时生成被遗弃分支的结构化摘要：

1. 将 leaf 指针移到目标 Entry  
2. 调用 LLM 生成摘要（目标、进展、关键决策）  
3. 持久化 `BranchSummaryEntry`  
4. 压缩后的上下文包含 `[分支摘要]`，确保 LLM 不丢失被遗弃分支的关键信息  

---

## SessionManager（高级 API）

包装 `SessionStore`，提供更简洁的接口（创建会话、追加条目、列出会话、分支等）。见 `crates/uncode-agent/src/session/manager.rs`。

---

*本文档基于 uncode 源码编写；物理存储以仓库中 `session/store.rs` 与 `surreal_store.rs` 为准。*
