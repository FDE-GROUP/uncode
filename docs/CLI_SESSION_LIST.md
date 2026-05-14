# 会话列表管理

## 背景

当前 uncode 只能通过 `--session <id>` 恢复会话，用户无法查看有哪些历史会话、各自的摘要和状态。需要提供会话列表和管理的界面。

参考项目 Pi 支持 `/sessions` 命令和会话元数据展示。

## 目标

- CLI 支持 `uncode sessions` 子命令列出历史会话
- TUI 支持会话列表 overlay 选择历史会话
- 显示会话元数据：标题、模型、时间、消息数、token 用量

## 设计

### CLI 子命令

```
uncode sessions              列出所有会话（最近 20 条）
uncode sessions --all        列出全部会话
uncode sessions --json       JSON 格式输出（供脚本消费）
```

输出格式：
```
ID           TITLE                    MODEL          MESSAGES  TOKENS     UPDATED
abc123       Fix auth middleware      deepseek-v3     15       12.5k      2h ago
def456       Add unit tests           glm-5.1         8        5.2k       1d ago
```

### SessionStore 扩展

新增方法：

```rust
impl SessionStore {
    /// 列出所有会话，按更新时间降序
    pub fn list_sessions(&self) -> Result<Vec<SessionSummary>>;

    /// 搜索会话（按标题、模型）
    pub fn search_sessions(&self, query: &str) -> Result<Vec<SessionSummary>>;
}

pub struct SessionSummary {
    pub id: String,
    pub title: Option<String>,
    pub model: String,
    pub message_count: usize,
    pub total_tokens: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub working_dir: Option<String>,
}
```

### TUI 会话列表

在 TUI 中通过快捷键或 slash 命令打开会话列表 overlay：

- `/sessions` 或 `Ctrl+X s`：打开会话列表
- 上下箭头选择，Enter 恢复
- 显示与 CLI 相同的元数据
- 支持搜索过滤

### 会话标题自动生成

- 取第一条用户消息的前 50 字符作为默认标题
- 或在会话结束时用 LLM 生成摘要标题（可选）

## 验收标准

- [ ] `uncode sessions` 列出历史会话
- [ ] `uncode sessions --json` 输出 JSON 格式
- [ ] TUI 中 `/sessions` 打开会话列表 overlay
- [ ] 可以从列表中选择并恢复会话
- [ ] 会话有合理的默认标题
