# uncode 会话模型

> SessionEntry 树状模型 + JSONL 持久化 + 压缩摘要 | 基于 `crates/uncode-core/src/session.rs` + `crates/uncode-agent/src/session/` 源码分析

uncode 的会话以 JSONL 格式持久化，每行一个独立事件。12 种 Entry 类型构成树状结构，支持分支、压缩和完整回放。

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

## JSONL 持久化

### 文件格式

```
# {base_dir}/{session_id}.jsonl
{"type":"session","id":"0192...","version":2,"model":"deepseek-v3","working_dir":"/home/user/project",...}
{"type":"message","id":"...","parent_id":null,"role":"user","content":[...],...}
{"type":"message","id":"...","parent_id":"...","role":"assistant","content":[...],...}
{"type":"model_change","id":"...","parent_id":"...","provider":"anthropic","model_id":"claude-sonnet-4-6"}
{"type":"compaction","id":"...","summary":"...","first_kept_entry_id":"...","tokens_before":50000,...}
```

第一行是 `SessionHeader`（`"type": "session"`），后续是 `SessionEntry`。

### SessionHeader

```rust
pub struct SessionHeader {
    #[serde(rename = "type")]
    pub entry_type: String,         // 固定 "session"
    pub id: String,
    pub version: u32,               // 构造时设为 2
    pub parent_session: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub model: String,
    pub title: Option<String>,
    pub working_dir: String,
}
```

---

## SessionStore

```rust
pub struct SessionStore {
    base_dir: PathBuf,
    states: RwLock<HashMap<String, SessionState>>,
}

pub struct SessionState {
    pub header: SessionHeader,
    pub by_id: HashMap<String, SessionEntry>,
    pub order: Vec<String>,         // 插入顺序
    pub leaf_id: Option<String>,    // 当前叶指针
}
```

### 关键操作

| 操作 | 说明 |
|------|------|
| `ensure_loaded()` | 惰性加载：首次访问时从 JSONL 读取，v1 自动迁移到 v2 |
| `append_entry()` | 追加到 `by_id` + `order` + 写入 JSONL 文件 |
| `get_path_to_root()` | 从任意 Entry 沿 `parent_id` 回溯到根 |
| `get_leaf_id()` | 获取当前叶指针（分支导航） |
| `fork_session()` | 创建新 JSONL 文件 + BranchEntry 指向父会话 |

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

`build_context()`（`context_builder.rs`）从 SessionStore 重建 LLM 消息数组：

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

```rust
// session/store.rs
fn fork_session(&self, parent_id: &str, reason: &str) -> SessionMetadata {
    // 创建新 JSONL 文件
    // 新 header 的 parent_session 指向父会话
    // 第一条 Entry 是 BranchEntry { parent_session_id, reason }
}
```

### 分支摘要

`branch_with_summary()` 在分支时生成被遗弃分支的结构化摘要：

1. 将 leaf 指针移到目标 Entry
2. 调用 LLM 生成摘要（目标、进展、关键决策）
3. 持久化 `BranchSummaryEntry`
4. 压缩后的上下文包含 `[分支摘要]`，确保 LLM 不丢失被遗弃分支的关键信息

---

## SessionManager（高级 API）

包装 SessionStore，提供更简洁的接口：

```rust
impl SessionManager {
    fn create_session(model, working_dir, title) -> SessionMetadata;
    fn append_entry(session_id, entry);
    fn load_entries(session_id) -> Vec<SessionEntry>;
    fn branch_session(parent_id, reason) -> SessionMetadata;
}
```

---

*本文档基于 uncode 源码（`crates/uncode-core/src/session.rs`、`crates/uncode-agent/src/session/`、`crates/uncode-agent/src/compaction.rs`）编写。*
