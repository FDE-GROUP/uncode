# 会话导出 HTML

## 背景

用户需要将会话内容导出为可分享的格式，用于：
- 团队协作中分享 AI 辅助编码的过程
- 保留问题解决过程的文档记录
- 向非技术人员展示工作成果

## 目标

- 支持 `uncode export <session_id>` 导出会话为 HTML 文件
- HTML 文件自包含（内联 CSS/JS），可直接在浏览器打开
- 导出内容包括：对话文本、代码块（语法高亮）、工具调用结果

## 设计

### CLI 命令

```
uncode export <session_id>              导出为 HTML 到 stdout
uncode export <session_id> -o out.html  导出到文件
uncode export --latest                  导出最近一次会话
```

### HTML 模板

生成自包含的 HTML 文件，包含：
- 内联 CSS（类似 GitHub Markdown 样式）
- 代码块使用 `<pre><code>` + CSS 类名（可接 highlight.js CDN）
- 无外部依赖，离线可用

### 消息渲染

| 消息类型 | HTML 渲染 |
|---------|----------|
| User 文本 | `<div class="msg user">` |
| Assistant 文本 | `<div class="msg assistant">` |
| Thinking | `<details><summary>思考过程</summary>` |
| Tool Call | `<div class="tool-call">` 显示工具名和参数 |
| Tool Result | `<details><summary>工具结果</summary>` |
| Error | `<div class="error">` |

### 元数据

HTML 头部包含会话元数据：
- 标题、日期、模型
- Token 用量统计
- 总轮次数

### 实现位置

在 `uncode-session` crate 中新增 `export` 模块：

```rust
pub fn export_html(session: &Session, theme: &str) -> Result<String>;
```

## 验收标准

- [ ] `uncode export <id>` 生成有效 HTML
- [ ] HTML 自包含，浏览器直接打开可阅读
- [ ] 代码块有语法高亮类名
- [ ] 工具调用折叠展示
- [ ] `-o` 参数输出到文件
