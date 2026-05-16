# 代码审查跟进报告

> 更新时间：2026-05-16
> 基于：`docs/CODE_REVIEW_ANALYSIS.md` 审查报告
> 范围：P0-P2 全部 17 个 Issue + Ollama 工具调用补全

## 修复总览

| 优先级 | Issue 范围 | 数量 | 提交 |
|--------|-----------|------|------|
| P0 | #189-#192 | 4 | `d718b02` |
| P1 | #193-#200 | 8 | `1b3f67a` |
| P2 | #201-#205 | 5 | `262b508`, `1f0895e`, `85d331d` |
| **合计** | | **17** | **5 commits** |

CI 状态：213 tests passed / 0 failures / clippy clean / fmt clean

---

## 一、P0 修复（安全 + 关键缺陷）

| # | 问题 | 文件 | 修复 |
|---|------|------|------|
| 189 | 路径遍历漏洞 | `tools/lib.rs` | `normalize_path()` 规范化 + `resolve_path()` canonicalize + `starts_with(cwd)` 沙箱。6 个工具统一调用 |
| 190 | 权限系统 confirm() 虚设 | `tui/permission.rs` | `confirm()` 根据 choice 分发：Allow/Edit → Some, Deny → None |
| 191 | read 工具 OOM | `tools/read.rs` | `fs::metadata().len()` 在 `read_to_string` 前检查文件大小 |
| 192 | grep include 过滤器失效 | `tools/grep.rs` | `glob::Pattern::new()` + `pat.matches()` 替换 `strip_prefix("*.")` |

---

## 二、P1 修复（高优先级）

| # | 问题 | 文件 | 修复 |
|---|------|------|------|
| 193 | Gemini API key 泄漏 | `llm/providers/gemini.rs` | URL query → `x-goog-api-key` header |
| 194 | cancel tool_results 丢失 | `agent/loop_engine.rs` | break 前 `tool_results.drain(..)` 推入 messages |
| 195 | Message ID 丢失 | `core/session.rs` | `MessageEntry` 添加 `id: Option<String>`，From 保留 msg.id |
| 196 | Ollama 流式禁用 | `llm/providers/ollama.rs` | `stream: false` → `stream: true` + NDJSON 解析 |
| 197 | 工具参数序列化双重引号 | — | 经验证为误报：Value.to_string() 输出合法 JSON，TUI 正确解析 |
| 198 | TUI 键盘 I/O 死循环 | `tui/lib.rs` | poll 失败时返回 Err 退出主循环 |
| 199 | Anthropic 系统提示词重复 | `llm/providers/anthropic.rs` | 过滤 messages 中 system 角色，避免和 body["system"] 重复 |
| 200 | 模型选择器 model_index 错位 | `tui/lib.rs` | selector Enter + /model 命令同步更新 model_index |

---

## 三、P2 修复

| # | 问题 | 文件 | 修复 |
|---|------|------|------|
| 201 | write 非原子写入 | `tools/write.rs` | 先写 `.tmp` 再 `fs::rename`，失败时清理临时文件 |
| 202 | ToolResult.is_error 硬编码 | `agent/loop_engine.rs` | 工具执行返回 `(String, bool)` 元组，失败时 is_error=true |
| 203 | JSON 解析失败静默 | `llm/providers/deepseek.rs` | 解析失败发出 `StreamEvent::Error` 而非静默产生空对象 |
| 204 | 6/7 提供商无工具调用 | `llm/providers/` | 详见下方"工具调用重构"章节 |
| 205 | 增量渲染 clone 优化 | `tui/chat.rs` | `take()` 取走所有权替代 `.clone()` |

---

## 四、工具调用重构（#204）

### 通用模块（`common.rs`）

提取 OpenAI 兼容格式的共享逻辑：

- `build_tools_json()` — 构建标准 tools 数组
- `OpenAiStreamState` — 跨 chunk 跟踪工具调用（id/name/args）
- `parse_openai_tool_calls()` — 从 choice delta 提取 Start/Delta 事件
- `flush_tool_calls()` — finish_reason 时输出 ToolCallEnd
- `extract_usage()` — 提取 usage 信息

### 各 Provider 状态

| Provider | 工具调用 | 改动 |
|----------|---------|------|
| DeepSeek | ✅ | 重构复用通用模块，保留 reasoning_content |
| OpenAI | ✅ | flat_map→scan + 工具调用解析 + tools 字段 |
| GLM | ✅ | 同上 |
| OpenRouter | ✅ | 同上 |
| Anthropic | ✅ | 独立实现 content_block_start/delta/stop 解析 |
| Ollama | ✅ | NDJSON + message.tool_calls 解析 |
| Gemini | ❌ | 待后续适配（functionDeclarations 格式） |

---

## 五、审查报告条目完整对照

| 编号 | 报告条目 | Issue | 状态 |
|------|---------|-------|------|
| 1.1 | 路径遍历漏洞 | #189 | ✅ 已修复 |
| 1.2 | read OOM | #191 | ✅ 已修复 |
| 1.3 | grep include 过滤器 | #192 | ✅ 已修复 |
| 1.4 | Gemini API key 泄漏 | #193 | ✅ 已修复 |
| 1.5 | 6/7 提供商工具调用 | #204 | ✅ 已修复（6/7） |
| 1.6 | Ollama 流式禁用 | #196 | ✅ 已修复 |
| 1.7 | 工具参数序列化 | #197 | ✅ 误报关闭 |
| 1.8 | cancel tool_results 丢失 | #194 | ✅ 已修复 |
| 1.9 | is_error 硬编码 | #202 | ✅ 已修复 |
| 1.10 | JSON 损坏静默 | #203 | ✅ 已修复 |
| 1.11 | 模板变量注入 | — | ⏳ P3 后续 |
| 1.12 | UTF-8 字节截取 | — | ✅ 已有防护 |
| 1.13 | Message ID 丢失 | #195 | ✅ 已修复 |
| 1.14 | 权限系统虚设 | #190 | ✅ 已修复 |
| 1.15 | 键盘 I/O 错误 | #198 | ✅ 已修复 |
| 1.16 | 增量渲染 clone | #205 | ✅ 已修复 |

### 重复代码消除

| 原始重复 | 状态 |
|---------|------|
| `resolve_path` 在 lib.rs 和 read.rs 中重复 | ✅ P0 修复中统一为 lib.rs 版本 |
| 流构造模式在 6 个 provider 中重复 | ✅ #204 重构中提取通用模块 |

---

## 六、未处理条目（P3+）

| 编号 | 条目 | 优先级 |
|------|------|--------|
| 1.11 | 模板变量注入（变量来源为项目配置文件，风险可控） | P3 |
| 2.1 | edit 空字符串匹配边界条件 | P3 |
| 2.1 | grep 无大小阈值 | P3 |
| 2.2 | highlight 多行状态（syntect 已缓解） | P3 |
| — | Gemini 工具调用（functionDeclarations 格式） | P2 |
| — | loop_engine run() 方法拆分 | P3 |
| — | 魔法数字常量化 | P3 |
| — | load_entries 流式改造 | P3 |
