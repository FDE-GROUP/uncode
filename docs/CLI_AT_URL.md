# `@<url>` URL 内容抓取

## 背景

开发过程中经常需要参考文档、API 说明等在线资源。将 URL 内容注入对话上下文可以大幅提升 Agent 对外部资源的理解。

本文档是 `@<file>` 功能的扩展，独立拆分以便分阶段实现。

## 目标

- 支持 `@https://...` 在用户输入中引用 URL 内容
- 支持抓取 HTML 页面并提取正文
- 支持抓取纯文本/API 响应

## 设计

### 解析规则

在 `@` 上下文解析中，识别 `http://` 或 `https://` 开头的 URL：

```
请参考 @https://docs.rs/tokio/latest/tokio/ 实现异步版本
```

### 抓取策略

1. **HTTP GET** 请求获取内容
2. **Content-Type 判断**：
   - `text/html`：提取 `<body>` 正文，去除标签，保留结构化文本
   - `text/plain` / `application/json` / `text/markdown`：直接使用
   - 其他类型：跳过并提示用户
3. **截断**：限制到 10KB，超出部分截断并标注 `[truncated]`
4. **超时**：请求超时 10 秒

### HTML 提取

使用轻量级 HTML 解析（`scraper` crate 或类似）：
- 移除 `<script>`, `<style>`, `<nav>`, `<footer>` 等无关内容
- 提取 `<article>` / `<main>` / `<body>` 区域
- 保留标题结构和链接文本

### 注入格式

```
<!-- @https://docs.rs/tokio/latest/tokio/ -->
[URL: https://docs.rs/tokio/latest/tokio/]

# Tokio Documentation
...extracted content...

[Source: docs.rs | 8.2KB of 45KB]
```

### 安全限制

- 仅允许 `http://` 和 `https://` 协议
- 禁止内网地址（`10.x`, `172.16-31.x`, `192.168.x`, `localhost`）
- 可通过配置白名单/黑名单控制
- 速率限制：单次请求最多引用 3 个 URL

## 验收标准

- [ ] `@https://...` 抓取并注入 URL 内容
- [ ] HTML 页面提取正文，去除无关标签
- [ ] JSON/纯文本直接使用
- [ ] 大页面有截断提示
- [ ] 内网地址被拒绝
- [ ] 超时有合理错误提示
