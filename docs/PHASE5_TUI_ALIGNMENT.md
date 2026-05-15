# Phase 5: TUI 功能对齐（Pi 对标）

> 基于 `docs/CLI_TUI_COMPARISON.md` 对比报告，系统性缩小 uncode TUI 与 Pi 的功能差距。
> 当前综合完成度约 35%，目标提升至 70%+。

## 一、优先级排序

按 **影响 × 成本** 排序，分 4 个批次递进实现：

| 批次 | 范围 | 预估 Issue 数 | 完成度提升 |
|------|------|-------------|-----------|
| P5-A | 斜杠命令扩展（高影响、低门槛） | 3 | 26% → 45% |
| P5-B | 快捷键 + 选择器系统 | 2 | 14% → 35% |
| P5-C | 输入编辑器增强 | 2 | 30% → 60% |
| P5-D | Markdown 补全 + 高级功能 | 2 | 50% → 70% |

---

## 二、P5-A：斜杠命令扩展

### 2.1 新增斜杠命令清单

| 命令 | 功能 | Pi 对应 | 优先级 |
|------|------|---------|--------|
| `/clear` | 清空对话区，开始新对话 | Pi /clear | P0 |
| `/compact` | 手动触发上下文压缩 | Pi /compact | P0 |
| `/model [name]` | 切换模型（无参数弹出选择器） | Pi /model | P0 |
| `/new` | 新建会话 | Pi /new | P0 |
| `/fork [id]` | 从指定会话创建分支 | Pi /fork | P1 |
| `/export [format]` | 导出会话（HTML/JSONL） | Pi /export | P1 |
| `/sessions` | 列出历史会话 | Pi /sessions | P1 |
| `/branch` | 显示当前会话分支信息 | Pi /branch | P2 |
| `/tree` | 会话分支树形导航 | Pi /tree | P2 |
| `/name [title]` | 设置当前会话标题 | Pi /name | P2 |
| `/copy` | 复制最后一条 Agent 回复到剪贴板 | Pi /copy | P2 |
| `/usage` | 显示当前会话 Token 用量统计 | Pi /usage | P2 |
| `/reload` | 重载配置和上下文文件 | Pi /reload | P2 |
| `/diff` | 显示未提交的文件变更 | Pi /diff | P2 |

### 2.2 实现架构

现有 `slash.rs` 已有命令分发框架。新增命令遵循同样模式：

```
SlashCommand {
    name: "/clear",
    description: "清空对话区",
    handler: fn(&mut TuiState) -> SlashResult,
}
```

### 2.3 拆分 Issue 建议

- **#N-1**: `/clear` `/compact` `/model` `/new` — 核心会话控制命令
- **#N-2**: `/fork` `/export` `/sessions` `/branch` — 会话管理命令
- **#N-3**: `/tree` `/name` `/copy` `/usage` `/reload` `/diff` — 辅助命令

---

## 三、P5-B：快捷键 + 选择器系统

### 3.1 新增快捷键

| 快捷键 | 功能 | Pi 对应 |
|--------|------|---------|
| `Ctrl+P` | 循环切换模型 | Pi Ctrl+P |
| `Shift+Ctrl+P` | 反向循环模型 | Pi Shift+Ctrl+P |
| `Ctrl+R` | 重试上一条消息 | Pi Ctrl+R |
| `Ctrl+N` | 新建会话 | Pi Ctrl+N |
| `Ctrl+/` | 撤销上一轮对话 | Pi Ctrl+/ |
| `Ctrl+G` | 打开外部编辑器 | Pi Ctrl+G |

### 3.2 选择器系统

| 选择器 | 功能 | 复杂度 |
|--------|------|--------|
| 会话选择器 | 历史会话列表，支持搜索和排序 | 中 |
| 设置选择器 | bool/choice/number 配置项 | 中 |
| 模糊匹配引擎 | 通用 fuzzy filter 用于所有选择器 | 低 |

### 3.3 拆分 Issue 建议

- **#N-4**: 快捷键扩展 — Ctrl+P/R/N/G/
- **#N-5**: 选择器系统 — 会话选择器 + 设置选择器 + 模糊匹配

---

## 四、P5-C：输入编辑器增强

### 4.1 功能清单

| 功能 | 说明 | Pi 对应 |
|------|------|---------|
| Shift+Enter 多行 | 插入换行而非发送 | Pi 多行编辑器 |
| Undo/Redo | 50 步历史 + 词级合并 | Pi UndoManager |
| 单词导航 | Alt+Left/Right 跳单词 | Pi word motion |
| 单词删除 | Alt+D 删除后续单词 | Pi Alt+D |
| Kill Ring | 10 项环形缓冲，Ctrl+Y/Alt+Y | Pi KillRing |
| Paste 处理 | 大段粘贴折叠标记 | Pi paste handling |
| 外部编辑器 | Ctrl+G 启动 $EDITOR | Pi Ctrl+G |

### 4.2 拆分 Issue 建议

- **#N-6**: 多行输入 + Undo/Redo + 单词导航
- **#N-7**: Kill Ring + Paste 处理 + 外部编辑器

---

## 五、P5-D：Markdown 补全 + 高级功能

### 5.1 Markdown 渲染补全

| 特性 | 说明 | 当前状态 |
|------|------|---------|
| 表格渲染 | 3 列分块对齐 | ❌ |
| 任务列表 | ☑/☐ checkbox | ❌ |
| 数学公式 | 基础数学符号 | ❌ |
| OSC 8 链接 | 终端可点击链接 | ❌ |
| 截断策略 | 前 50 + 省略 + 后 50 | 当前: 最近 20 行 |

### 5.2 高级功能

| 功能 | 说明 |
|------|------|
| JSON-RPC 模式 | 18 命令 + 25 事件，IDE 集成基础 |
| 终端图片 | Kitty/Ghostty/WezTerm 协议 |

### 5.3 拆分 Issue 建议

- **#N-8**: Markdown 表格 + 任务列表 + OSC 8 链接 + 截断策略
- **#N-9**: JSON-RPC 模式基础（命令注册 + 事件广播）

---

## 六、关键文件清单

| 文件 | 改动类型 |
|------|---------|
| `crates/uncode-tui/src/slash.rs` | 新增 14 条斜杠命令 |
| `crates/uncode-tui/src/input.rs` | 多行/Undo/KillRing/Paste |
| `crates/uncode-tui/src/lib.rs` | 快捷键注册 + 新选择器 |
| `crates/uncode-tui/src/markdown.rs` | 表格/任务列表/链接 |
| `crates/uncode-tui/src/chat.rs` | /clear /compact 交互 |
| `crates/uncode-tui/src/selector.rs` | 会话/设置选择器 + 模糊匹配 |
| `crates/uncode-session/src/manager.rs` | /fork /export /sessions 后端 |
| `crates/uncode-agent/src/loop_engine.rs` | /compact /undo 交互 |
| `crates/uncode-rpc/` | JSON-RPC 新模块 |

---

*本文档与 VISION.md Phase 5 对应，具体实现通过 GitHub Issues 追踪。*
