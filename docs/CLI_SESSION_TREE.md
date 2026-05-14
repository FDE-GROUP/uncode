# 会话树与分支导航

## 背景

AI 编码助手的使用模式中，用户经常需要从某个中间状态分叉探索不同方案。当前 uncode 的 session 有 branch 支持（JSONL 中记录 branch event），但缺少导航和可视化界面。

参考项目 Pi 支持 `/tree` 命令查看会话分支树，在不同分支间切换。

## 目标

- TUI 中支持 `/tree` 命令显示会话分支树
- 支持在不同分支间切换浏览
- CLI 支持 `--fork <session_id>` 从指定会话分叉

## 设计

### 会话树数据结构

```rust
pub struct SessionTree {
    pub root: SessionNode,
}

pub struct SessionNode {
    pub session_id: String,
    pub title: Option<String>,
    pub model: String,
    pub message_count: usize,
    pub created_at: DateTime<Utc>,
    pub children: Vec<SessionNode>,
}
```

### TUI `/tree` 命令

打开 overlay 面板，树形展示当前会话的所有分支：

```
abc123  Fix auth middleware          (15 msgs, deepseek-v3)
├── def456  Try OAuth approach      (8 msgs, deepseek-v3)
│   └── ghi789  Switch to JWT       (3 msgs, glm-5.1)
└── jkl012  Try session approach    (5 msgs, deepseek-v3)
```

交互：
- 上下箭头导航节点
- `Enter` 切换到选中分支的视图
- `b` 从当前节点创建新分支
- `q` / `Esc` 关闭树视图

### CLI `--fork`

```
uncode --fork abc123 "try different approach"
```

从指定会话的最后状态创建新分支，加载历史消息，追加新 prompt。

### SessionStore 扩展

```rust
impl SessionStore {
    /// 构建以指定会话为根的分支树
    pub fn build_tree(&self, session_id: &str) -> Result<SessionTree>;

    /// 获取指定会话的所有子分支
    pub fn get_branches(&self, session_id: &str) -> Result<Vec<SessionSummary>>;
}
```

### 渲染

使用 `ratatui::widgets::Block` + 自定义渲染绘制树形结构。每个节点显示：会话 ID（缩写）、标题、消息数、模型。

## 验收标准

- [ ] TUI `/tree` 显示会话分支树
- [ ] 可在分支间导航切换
- [ ] `--fork` 可从指定会话创建分支
- [ ] 分支信息持久化在 JSONL 中
