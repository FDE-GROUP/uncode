# 内置工具审计：缺陷与优化机会

> 基于 `crates/uncode-agent/src/tools/` 源码与测试的逐项审查（2026-05）。  
> 工具说明见 [`UNCODE_BUILTIN_TOOLS.md`](UNCODE_BUILTIN_TOOLS.md)。

## 已落地修复（关联 Issue）

| Issue | 修复摘要 |
|-------|----------|
| [#281](https://github.com/FDE-GROUP/uncode/issues/281) | `atomic_write()` + `tempfile::NamedTempFile`；`write`/`edit` 不再使用 `with_extension("tmp")` |
| [#282](https://github.com/FDE-GROUP/uncode/issues/282) | `grep` `include` 匹配相对路径 + 文件名；补集成测试 |
| [#283](https://github.com/FDE-GROUP/uncode/issues/283) | `bash` `workdir` 走 `resolve_path`；`execute` 超时杀进程组；流式 stdout 累积上限 |
| [#284](https://github.com/FDE-GROUP/uncode/issues/284) | `url_safety::ensure_public_http_url`；`web_fetch` 限制重定向 |

| [#285](https://github.com/FDE-GROUP/uncode/issues/285) | `grep` 使用 `ignore` 遍历（`.gitignore`）+ 单文件 1MB 上限；集成测试 |
| [#286](https://github.com/FDE-GROUP/uncode/issues/286) | `read`：`spawn_blocking` + `offset` schema 澄清 + 边界测试 |
| [#287](https://github.com/FDE-GROUP/uncode/issues/287) | 权限确认状态栏展示 `ToolDefinition.description`；`web_search` 输出截断 |

| [#290](https://github.com/FDE-GROUP/uncode/issues/290) | `find` 使用 `ignore`；`write`/`edit` `spawn_blocking`；`test_find_respects_gitignore` |

| [#292](https://github.com/FDE-GROUP/uncode/issues/292) | `AppConfig.tools`：`max_file_bytes`、`max_grep_results` 配置化并注入 `read`/`grep` |

| （切片，#244） | `ToolContext.execution_env`；文件工具走 `FileSystem`；`bash`/`LocalShell` 共享 `bash_exec` |
| [#299](https://github.com/FDE-GROUP/uncode/issues/299) | 七件套 `prepare_arguments`：`path`/`workdir` 沙箱解析与相对路径回写 |
| [#244](https://github.com/FDE-GROUP/uncode/issues/244) | `ExecutionEnv` 切片 + `mock_env` 注入测试（read/ls） |

额外（无单独 Issue）：`read` 目录 listing 上限 500 条，与 `ls` 一致。

---

## 1. 审计摘要

| 严重程度 | 数量 | 典型项 |
|----------|------|--------|
| **P0 安全/正确性** | 4 | `bash` workdir 越界、`write`/`edit` 临时文件碰撞、`grep` include 与文档不符 |
| **P1 可靠性** | 6 | `bash` 超时未杀进程、`grep` 无测试、目录 listing 无上限不一致 |
| **P2 体验/对齐** | 8 | 阻塞 I/O、`read` offset 语义、SSRF、缺 `.gitignore` |
| **P3 增强** | 10+ | `ExecutionEnv` 统一、Pi 对齐、可观测性 |

**沙箱路径（`resolve_path`）**：对 `..` 与 **canonicalize 后落在 CWD 外** 的路径（含指向项目外的符号链接）会拒绝，行为正确。  
**主要缺口**：`bash` 的 `workdir` **不**走 `resolve_path`，可任意 `cd` 到系统目录（测试 `test_bash_workdir` 显式依赖 `/tmp`）。

---

## 2. 逐项审查

### 2.1 `read`

| 类型 | 发现 |
|------|------|
| **缺陷** | `offset` 在 schema 中写「起始行号」，实现为 `lines().skip(offset)`，即 **0 起始的 skip 计数**；输出行为 **1 起始行号**（`{:>6}:` / hashline）。模型易 off-by-one。 |
| **缺陷** | 目录 listing **无条目上限**；`ls` 限制 500 条，行为不一致。大目录（如误读 `node_modules`）可撑爆上下文。 |
| **缺陷** | `execute` 使用 **`std::fs` 同步 I/O**，在 tokio 运行时上可能阻塞 worker（其它工具部分用 `spawn_blocking`）。 |
| **缺陷** | 无 `offset`/`limit` 时仍一次性 `read_to_string` 整文件（≤1MB）；无「仅元数据/仅头几 KB」模式。 |
| **局限** | 非 UTF-8 文件直接报错；无 binary/hex 预览。 |
| **局限** | `hashline` 描述为英文，与其余中文 description 不统一。 |
| **优化** | 目录 listing 与 `ls` 共用上限与排序策略；大文件默认建议 `limit`；`spawn_blocking` 包装读文件。 |
| **优化** | schema 明确：`offset` = 跳过的行数（0-based），显示行号 = offset + 1。 |
| **测试缺口** | 无超大目录、无 `offset`/`limit` 边界、无非法 path 沙箱用例（`mod/tests.rs` 有部分沙箱，未覆盖 read 越界）。 |

---

### 2.2 `write`

| 类型 | 发现 |
|------|------|
| **P0 缺陷** | 原子写使用 `resolved.with_extension("tmp")`：**替换扩展名**而非追加后缀。例如 `lib.rs` 与 `lib.c` 均得到 `lib.tmp`，**并发或连续写入可互相覆盖临时文件**。应使用 `create_tempfile_in(dir)` 或 `{path}.uncode.tmp.{random}`。 |
| **缺陷** | 同步 `fs::write` / `rename` 在 async `execute` 中阻塞。 |
| **缺陷** | 跨设备 `rename` 可能失败（临时文件与目标不同 mount）；未 fallback `copy`。 |
| **局限** | 仅全文覆写；无 `append`、无 `mode`/`executable` 位。 |
| **优化** | 写入前可选检测「文件已被磁盘修改」（mtime）降低覆盖并发编辑。 |
| **优化** | 返回 diff 已很好；可附带 `bytes_written` 结构化 `details`（对齐 Pi `ToolResult.details`）。 |
| **测试** | 有基本写、父目录创建；**无** tmp 碰撞、跨 mount rename。 |

---

### 2.3 `edit`

| 类型 | 发现 |
|------|------|
| **P0 缺陷** | 与 `write` 相同 **`with_extension("tmp")` 碰撞**风险。 |
| **缺陷** | Hashline：`edits` 应用顺序依赖「自下而上」；**未**在 schema 中说明模型应按 bottom-up 或任意顺序提交（实现会排序，OK）。 |
| **缺陷** | Legacy 模式要求 `old_string` **全局唯一**；对重复子串常见模板不友好，错误信息尚可。 |
| **缺陷** | `op` 大小写敏感；模型传 `Replace` 会失败。 |
| **局限** | 仅支持单行锚点区间；无「按函数名」等语义编辑。 |
| **局限** | CRLF 文件：`lines()` 去掉 `\r`，写回可能变 LF-only。 |
| **优化** | 与 `read(hashline=true)` 联动在 description 中写死工作流（减少 legacy 误用）。 |
| **优化** | 重叠检测已有；可增加「锚点过期」时自动建议 `read` 的 error hint。 |
| **测试** | hashline / legacy 覆盖较好；**无** 多文件并发 edit、**无** tmp 碰撞。 |

---

### 2.4 `grep`

| 类型 | 发现 |
|------|------|
| **P0 缺陷** | `include` 参数文档示例为 `**/*.rs`、`src/*.rs`，实现仅用 **`entry.file_name()`** 做 `glob::Pattern::matches`，**不匹配相对路径**。`src/foo.rs` 不会被 `*.rs` 匹配（除非文件名碰巧）。与 schema 描述严重不符。 |
| **缺陷** | **无单元/集成测试**（`tests.rs` 中无 GrepTool）。 |
| **缺陷** | 每个文件 `read_to_string` 全量读入，**无单文件大小上限**；大文件可导致内存与时间激增。 |
| **缺陷** | 不跳过 `.git`、`node_modules`、`target` 等；噪音与性能差。 |
| **局限** | `max_depth(20)` 硬编码；无 `ignore` 可配置。 |
| **局限** | 结果 50 条全局计数，非「每文件」；长行不截断。 |
| **优化** | `include` 应对 `strip_prefix(root)` 后的相对路径匹配，或改用 `globset`/ripgrep。 |
| **优化** | 对齐 ripgrep：`.gitignore`、并行目录遍历、二进制检测。 |
| **优化** | 使用 `spawn_blocking` 已具备；可加 `type`/`head_limit` 参数。 |

---

### 2.5 `find`

| 类型 | 发现 |
|------|------|
| **缺陷** | `glob("{root}/{pattern}")`：pattern 若已含前导 `/` 或 Windows 反斜杠可能异常；未规范化。 |
| **缺陷** | 无测试覆盖 `**` 深层与 200 条截断边界（有基本 happy path）。 |
| **局限** | 不返回目录；仅文件路径（`flatten` 含目录吗？ `glob` 通常两者都有需确认）— `glob` 返回的可能是目录。 |
| **优化** | 默认排除 `node_modules`、`.git`（与 grep 一致策略）。 |
| **优化** | 返回 mtime/size 可选，减少后续 `read` 次数。 |

---

### 2.6 `ls`

| 类型 | 发现 |
|------|------|
| **缺陷** | 与 `read(目录)` 功能重叠，但 **500 条上限仅 ls 有**；产品语义应统一。 |
| **缺陷** | 不显示隐藏文件（默认 `read_dir` 行为）— 与 Unix `ls -a` 不同，需在 description 说明。 |
| **局限** | 非递归；无 tree。 |
| **优化** | 可选 `all: true` 显示点文件；与 `read` 目录模式合并文档。 |
| **测试** | 有空目录、不存在目录；较好。 |

---

### 2.7 `bash`

| 类型 | 发现 |
|------|------|
| **P0 安全** | **`workdir` 未经 `resolve_path`**，`current_dir` 可为 `/tmp`、用户 home 等（见 `test_bash_workdir`）。描述写「沙箱」易误解；实质是 **全机 shell 能力** + 可选 TUI 门控。 |
| **P1 缺陷** | `execute()`（测试用）在 `timeout` 时 **不 kill 子进程**；`tokio::time::timeout` 丢弃 future 后 `sleep` 等仍运行。Agent 主路径用 `execute_with_context` 会 `kill_process_group`，但 **简单路径仍泄漏**。 |
| **P1 缺陷** | `execute_with_context` 在读取 stdout 时 **无累积上限**，仅最后 `truncate_output`；恶意命令可撑满内存。 |
| **缺陷** | `description` 参数 **未使用**（仅 schema/UI 意图）；应在 TUI 审批或日志中消费。 |
| **缺陷** | 非 Unix 无进程组杀；Windows 上取消/超时行为弱。 |
| **缺陷** | `execute()` 路径非零退出码仍返回 `Ok(String)`（文本含 exit code）；`execute_with_context` 设 `is_error: true`。**双路径语义不一致**（测试走 `execute`）。 |
| **局限** | 固定 `bash -c`；无 `env` 注入、无 stdin 喂入。 |
| **优化** | `workdir` 必须 `resolve_path` 且落在 CWD 内；或显式文档「非文件沙箱」。 |
| **优化** | 超时统一：spawn + kill + 与 `LocalShell` 合并（`local_env.rs` 已有 `sh -c` 实现，重复）。 |
| **优化** | 流式读取时按字节截断并停止读取；stderr 也走 `on_progress`。 |
| **测试** | echo、timeout、truncation、workdir 较好；**无** 取消杀进程断言、**无** workdir 沙箱策略测试。 |

---

### 2.8 `web_fetch`

| 类型 | 发现 |
|------|------|
| **P1 安全** | ~~无 SSRF 防护~~ → `url_safety::ensure_public_http_url`（#284）。 |
| **缺陷** | ~~无重定向上限~~ → `Policy::limited(5)`（#284）。 |
| **缺陷** | ~~`html2text` 失败即整工具失败~~ → 降级为 lossy UTF-8 原始 HTML（#306）。 |
| **局限** | 无 JS、无 cookie、无认证头。 |
| **优化** | ~~返回 `Content-Type`、最终 URL~~ → `ToolResult.details`（#308）。 |
| **测试** | definition + SSRF + wiremock plain/html/503 + details（#305/#308）；`html2text` 单元测试（#306）。 |

---

### 2.9 `web_search`

| 类型 | 发现 |
|------|------|
| **缺陷** | ~~无输出长度上限~~ → `truncate_output` 50KB（#287）；`max_results` 上限 20（#307）。 |
| **缺陷** | API key 随请求发送（Tavily 设计）；需确保日志/on_payload 不泄露。 |
| **局限** | 强依赖 Tavily；无离线/备用搜索。 |
| **优化** | 截断每条 snippet（可选）。 |
| **测试** | key/definition + wiremock 成功/401 + `max_results` clamp（#305/#307）。 |

---

### 2.10 辅助模块

#### `hashline`

| 类型 | 发现 |
|------|------|
| **局限** | 2 字符哈希（256 桶），**碰撞概率低但存在**；碰撞时误拒或误接受（后者更危险，当前会 validate 失败）。 |
| **优化** | 碰撞时 fallback 到更长 hash 或附带行内容摘要。 |

#### `diff`

| 类型 | 发现 |
|------|------|
| **良好** | 委托 `uncode_core`，`MAX_DIFF_LINES` 控制输出。 |
| **优化** | 极大文件 diff 仍可能昂贵；可仅统计 hunks 数。 |

#### `local_env` / `resolve_path`

| 类型 | 发现 |
|------|------|
| **良好** | `resolve_path` 对 `..` 与 canonicalize 外链出 CWD 有效。 |
| **缺口** | 文件工具未统一走 `ExecutionEnv`；`bash` 未复用 `LocalShell`。 |
| **优化** | Pi 对齐：全部 FS/Shell 经 `ExecutionEnv`，便于测试注入与远程沙箱。 |

---

## 3. 横切问题

| 主题 | 说明 |
|------|------|
| **测试覆盖** | 七件套 + `mock_env` + `web_fetch`/`web_search` wiremock；`grep` 条件测 ripgrep。 |
| **async 一致性** | `read`/`write`/`edit` 同步 FS；`grep`/`find`/`ls` 用 `spawn_blocking`。 |
| **描述语言** | 中英混用（`read.hashline`、`edit` 大段英文 description）。 |
| **Pi 对齐** | 七件套已实现 `prepare_arguments`（路径沙箱 + 相对路径回写）；`ExecutionEnv` 切片已落地；`bash` sequential 已对齐。 |
| **可观测性** | 工具 `details` 含退出码/截断/bytes 等；`AgentLoop` 统一注入 `duration_ms`。 |

---

## 4. 建议修复优先级（工程）

### 4.1 建议尽快（P0–P1）

1. **`write`/`edit`**：改用唯一临时路径（`tempfile` 或 `.{name}.uncode.{pid}.tmp`），避免 `with_extension("tmp")` 碰撞。  
2. **`grep`**：修正 `include` 匹配逻辑 **或** 修正 schema 文档为「仅文件名 glob」。  
3. **`bash`**：`workdir` 经 `resolve_path`；`execute()` 超时杀进程组；与文档统一「非文件系统沙箱」。  
4. **`web_fetch`**：基础 SSRF 过滤（私有 IP、metadata IP）。  
5. **`grep`**：补集成测试 + 单文件大小上限。

### 4.2 短期增强（P2）

1. `read` 目录 listing 上限；`offset` schema 澄清。  
2. `bash` 流式读取累积上限；`description` 接入权限 UI/日志。  
3. `grep`/`find` 默认 respect `.gitignore`（可用 `ignore` crate）。  
4. 文件工具 `spawn_blocking` 统一。  
5. `web_search` 输出截断。

### 4.3 中期（P3 / 架构）

1. ~~全面接入 `ExecutionEnv`（对齐 Pi）~~ — 七件套 + `bash_exec` 已落地（#244 切片）。  
2. ~~可选 ripgrep 后端~~ — 已安装 `rg` 时 `grep` 优先走 ripgrep（`details.backend`），否则 `ignore` walk + 内置 regex。  
3. 工具级 `prepare_arguments`（路径规范化、默认 limit）。  
4. 配置化：`max_size`、`max_grep_results` 进 `uncode-shared` config。

---

## 5. 与 Pi 的差异（非缺陷，但影响体验）

| 能力 | Pi | uncode 现状 |
|------|-----|-------------|
| 参数校验 | TypeBox 全量 | 轻量 JSON Schema 子集 |
| 文件/Shell | `ExecutionEnv` + `Result` 错误码 | 直接 `std::fs` / `tokio::process` |
| 临时文件 | `createTempFile` 等 | `tempfile::NamedTempFile`（#281） |
| 搜索 | 生态中常接 ripgrep | 自研 WalkDir + regex |

---

## 6. 相关文档

- [`UNCODE_BUILTIN_TOOLS.md`](UNCODE_BUILTIN_TOOLS.md) — 用途与设计  
- [`UNCODE_TOOL_SYSTEM.md`](UNCODE_TOOL_SYSTEM.md) — 生命周期与 CLI  
- [`../pi-technologies/PI_TOOL_SYSTEM.md`](../pi-technologies/PI_TOOL_SYSTEM.md) — Pi 对照  

---

*审计版本：2026-05；审查范围：`uncode-agent/src/tools/*.rs` 与 `tools/tests.rs`。*
