# 内置工具审计：缺陷与优化机会

> 基于 `crates/uncode-agent/src/tools/` 源码与测试的逐项审查（2026-05）。  
> 工具说明见 [`UNCODE_BUILTIN_TOOLS.md`](UNCODE_BUILTIN_TOOLS.md)。  
> **状态列**：与 `main` 同步至 2026-05-21（含 #309–#312 合并后预期）。

## 已落地修复（关联 Issue）

| Issue | 修复摘要 |
|-------|----------|
| [#281](https://github.com/FDE-GROUP/uncode/issues/281) | `atomic_write()` + `tempfile::NamedTempFile`；`write`/`edit` 不再使用 `with_extension("tmp")` |
| [#282](https://github.com/FDE-GROUP/uncode/issues/282) | `grep` `include` 匹配相对路径 + 文件名；补集成测试 |
| [#283](https://github.com/FDE-GROUP/uncode/issues/283) | `bash` `workdir` 走 `resolve_path`；超时/取消杀进程组；流式 stdout 累积上限 |
| [#284](https://github.com/FDE-GROUP/uncode/issues/284) | `url_safety::ensure_public_http_url`；`web_fetch` 限制重定向 |
| [#285](https://github.com/FDE-GROUP/uncode/issues/285) | `grep` 使用 `ignore` 遍历（`.gitignore`）+ 单文件 1MB 上限；集成测试 |
| [#286](https://github.com/FDE-GROUP/uncode/issues/286) | `read`：`spawn_blocking` + `offset` schema 澄清 + 边界测试 |
| [#287](https://github.com/FDE-GROUP/uncode/issues/287) | 权限确认状态栏展示工具说明；`web_search` 输出截断 |
| [#290](https://github.com/FDE-GROUP/uncode/issues/290) | `find` 使用 `ignore`；`write`/`edit` `spawn_blocking`；`test_find_respects_gitignore` |
| [#292](https://github.com/FDE-GROUP/uncode/issues/292) | `AppConfig.tools`：`max_file_bytes`、`max_grep_results` 配置化并注入 `read`/`grep` |
| （切片，#244） | `ToolContext.execution_env`；文件工具走 `FileSystem`；`bash`/`LocalShell` 共享 `bash_exec` |
| [#299](https://github.com/FDE-GROUP/uncode/issues/299) | 七件套 `prepare_arguments`：`path`/`workdir` 沙箱解析与相对路径回写 |
| [#244](https://github.com/FDE-GROUP/uncode/issues/244) | `ExecutionEnv` 切片 + `mock_env` 注入测试（read/ls） |
| [#303](https://github.com/FDE-GROUP/uncode/issues/303) | `grep` 优先 ripgrep 后端（`details.backend`） |
| [#305–#309](https://github.com/FDE-GROUP/uncode/pull/305) | `web_fetch`/`web_search` wiremock、SSRF、html 降级、details、snippet 截断 |
| [#310–#311](https://github.com/FDE-GROUP/uncode/pull/310) | bash 取消测试；`execute` 与流式路径统一 + 全程 deadline |
| [#312](https://github.com/FDE-GROUP/uncode/pull/312) | bash 模型 `description` → TUI 确认栏 + 日志（`approval_description`） |

额外（无单独 Issue）：`read` 目录 listing 上限 500 条，与 `ls` 一致。

---

## 1. 审计摘要

| 严重程度 | 数量 | 典型项 |
|----------|------|--------|
| **P0 安全/正确性** | 0（已修） | 见「已落地修复」表 |
| **P1 可靠性** | 0（已修） | 七件套 + web + bash |
| **P2 体验/对齐** | 3 | 非 UTF-8/binary 预览、跨 mount rename、描述语言混用 |
| **P3 增强** | 若干 | Platform 审批 UI、`ExecutionEnv` 全覆盖、语义编辑 |

**沙箱路径（`resolve_path`）**：对 `..` 与 canonicalize 后落在 CWD 外的路径会拒绝，行为正确。

**主要缺口（剩余）**：Platform 审批 UI 复用 `approval_description`；§2 各工具「局限/优化」为增强项而非未修缺陷。

---

## 2. 逐项审查

### 2.1 `read`

| 类型 | 发现 |
|------|------|
| **已修复** | `offset` schema 写明「跳过的行数（0-based）」与显示行号关系（#286）。 |
| **已修复** | 目录 listing **500 条上限** + `entry_limit` details，与 `ls` 一致。 |
| **已修复** | 文件读取走 `spawn_blocking`（#286）。 |
| **局限** | 非 UTF-8 文件直接报错；无 binary/hex 预览。 |
| **局限** | `hashline` 描述为英文，与其余中文 description 不统一。 |
| **优化** | 大文件默认建议 `limit`；非法 path 沙箱用例可再补。 |
| **测试** | 有 offset/limit、hashline、mock_env；可补超大目录边界。 |

---

### 2.2 `write`

| 类型 | 发现 |
|------|------|
| **已修复** | `atomic_write()` + 唯一临时文件（#281）；`test_write_distinct_temp_paths_for_same_stem`。 |
| **已修复** | `spawn_blocking` + `details.bytes_written`（#290/#301）。 |
| **局限** | 仅全文覆写；无 `append`、无 `mode`/`executable` 位。 |
| **优化** | 跨设备 `rename` 未 fallback `copy`；写入前 mtime 检测（并发编辑）。 |
| **测试** | 基本写、父目录、tmp 碰撞；无跨 mount 专项。 |

---

### 2.3 `edit`

| 类型 | 发现 |
|------|------|
| **已修复** | 与 `write` 共用 `atomic_write`（#281）。 |
| **已修复** | `spawn_blocking`（#290）。 |
| **局限** | Legacy 模式要求 `old_string` 全局唯一；`op` 大小写敏感。 |
| **局限** | CRLF 文件写回可能变 LF-only。 |
| **优化** | hashline 工作流说明、锚点过期 hint。 |
| **测试** | hashline / legacy 覆盖较好。 |

---

### 2.4 `grep`

| 类型 | 发现 |
|------|------|
| **已修复** | `include` 匹配相对路径 + 文件名（#282）；`test_grep_include_matches_relative_path`。 |
| **已修复** | `ignore` + 单文件 1MB 上限（#285）；`test_grep_respects_gitignore`。 |
| **已修复** | 集成测试 + 条件 ripgrep（#303）；`test_grep_native_backend_details_when_no_rg`。 |
| **已修复** | `max_grep_results` 等可配置（#292）。 |
| **局限** | `max_depth(20)` 硬编码；长行不截断。 |
| **优化** | 每文件条数、`head_limit` 参数。 |
| **测试** | 覆盖 include、gitignore、rg/native backend。 |

---

### 2.5 `find`

| 类型 | 发现 |
|------|------|
| **已修复** | `ignore` 遍历（#290）；`test_find_respects_gitignore`。 |
| **局限** | pattern 含前导 `/` 或 Windows 路径未专门规范化。 |
| **优化** | 返回 mtime/size；明确目录是否列入结果。 |
| **测试** | happy path + gitignore；可补 200 条截断边界。 |

---

### 2.6 `ls`

| 类型 | 发现 |
|------|------|
| **已修复** | `read(目录)` 与 `ls` 均为 500 条上限（逻辑对齐）。 |
| **局限** | 不显示隐藏文件（非 `ls -a`）；非递归。 |
| **优化** | 可选 `all: true`；与 `read` 目录模式文档合并说明。 |
| **测试** | 空目录、不存在路径；较好。 |

---

### 2.7 `bash`

| 类型 | 发现 |
|------|------|
| **已修复** | `workdir` 经 `resolve_path`（#283）；越界拒绝测试。 |
| **已修复** | `exec_bash_streaming` 全程 `deadline`；超时/取消 `kill_process_group`；流式 stdout 字节上限。 |
| **已修复** | 模型 `description` → TUI 确认栏 + 日志（`approval_description`，#312）。 |
| **已修复** | `execute()` 委托 `execute_with_context`；非零退出码语义一致（#311）。 |
| **局限** | 非 Unix 进程组弱；固定 `bash -c`；无 stdin 喂入。 |
| **优化** | stderr 走 `on_progress`；与 `LocalShell` 进一步合并。 |
| **测试** | echo、timeout、truncation、沙箱、取消、exit code。 |

---

### 2.8 `web_fetch`

| 类型 | 发现 |
|------|------|
| **已修复** | SSRF（#284）、重定向上限、`html2text` 降级（#306）、`details`（#308）。 |
| **局限** | 无 JS、cookie、认证头。 |
| **测试** | wiremock + SSRF + details + html 单元测试。 |

---

### 2.9 `web_search`

| 类型 | 发现 |
|------|------|
| **已修复** | 总输出 50KB（#287）、`max_results` 1–20（#307）、snippet/answer 截断（#309）。 |
| **局限** | 强依赖 Tavily；日志勿泄露 API key。 |
| **测试** | wiremock 成功/401 + clamp。 |

---

### 2.10 辅助模块

#### `hashline`

| 类型 | 发现 |
|------|------|
| **局限** | 2 字符哈希碰撞概率低但存在。 |
| **优化** | 碰撞时更长 hash 或行内容摘要。 |

#### `diff`

| 类型 | 发现 |
|------|------|
| **良好** | 委托 `uncode_core`，`MAX_DIFF_LINES` 控制输出。 |

#### `local_env` / `resolve_path`

| 类型 | 发现 |
|------|------|
| **良好** | `resolve_path` 对 `..` 与外链出 CWD 有效。 |
| **已修复（切片）** | read/ls 等可走 `ExecutionEnv`（#244）；`bash_exec` 共享。 |
| **优化** | 全部 FS/Shell 经 `ExecutionEnv`（远程沙箱）。 |

---

## 3. 横切问题

| 主题 | 说明 |
|------|------|
| **测试覆盖** | 七件套 + mock_env + web wiremock + grep rg/native + bash 取消/超时。 |
| **async 一致性** | `read`/`write`/`edit`/`grep`/`find`/`ls` 主要路径已 `spawn_blocking`。 |
| **描述语言** | 中英混用（`read.hashline`、`edit` 大段英文）。 |
| **Pi 对齐** | `prepare_arguments`、`ExecutionEnv` 切片、bash sequential、`ToolResult.details` + `duration_ms`。 |
| **可观测性** | 退出码/截断/bytes/url 等写入 `details`。 |

---

## 4. 建议修复优先级（工程）

### 4.1 P0–P1

**均已落地**（见 §「已落地修复」）。

### 4.2 P2（可选）

1. 描述语言中英统一。  
2. `read` 非 UTF-8 / binary 预览。  
3. `write`/`edit` 跨 mount `rename` fallback。

### 4.3 P3（架构）

1. Platform 审批 UI 复用 `approval_description`。  
2. `ExecutionEnv` 全覆盖（远程/测试注入）。  
3. 语义编辑（按符号名锚点）等 Pi 增强项。

---

## 5. 与 Pi 的差异（非缺陷）

| 能力 | Pi | uncode 现状 |
|------|-----|-------------|
| 参数校验 | TypeBox 全量 | 轻量 JSON Schema 子集 |
| 文件/Shell | `ExecutionEnv` + 错误码 | 切片已落地；部分仍直连 FS |
| 临时文件 | `createTempFile` 等 | `tempfile::NamedTempFile`（#281） |
| 搜索 | 常接 ripgrep | 已优先 `rg`，否则 ignore + regex |

---

## 6. 相关文档

- [`UNCODE_BUILTIN_TOOLS.md`](UNCODE_BUILTIN_TOOLS.md)  
- [`UNCODE_TOOL_SYSTEM.md`](UNCODE_TOOL_SYSTEM.md)  
- [`../pi-technologies/PI_TOOL_SYSTEM.md`](../pi-technologies/PI_TOOL_SYSTEM.md)  

---

*审计版本：2026-05-21；审查范围：`uncode-agent/src/tools/*.rs` 与 `tools/tests.rs`。*
