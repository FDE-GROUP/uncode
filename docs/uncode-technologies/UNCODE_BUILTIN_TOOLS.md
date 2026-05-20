# uncode 内置工具详解

> 逐个说明 `uncode-agent` 已实现的 **9 个** `ToolExecutor` 的用途、参数、设计原理与限制。  
> 注册与 active 策略见 [`UNCODE_TOOL_SYSTEM.md`](UNCODE_TOOL_SYSTEM.md)；模型如何选用工具见 [`UNCODE_TOOL_SELECTION_BY_LLM.md`](UNCODE_TOOL_SELECTION_BY_LLM.md)。

---

## 1. 总览

| 工具名 | 实现 | Pi 内置 | 默认对 LLM active | 执行模式 | 一句话用途 |
|--------|------|---------|-------------------|----------|------------|
| `read` | `read.rs` | 是 | 是 | Parallel | 读文件/列目录，支持分页与 hashline 锚点 |
| `write` | `write.rs` | 是 | 是 | Parallel | 创建或整文件覆写 |
| `edit` | `edit.rs` | 是 | 是 | Parallel | hashline 行级编辑或 legacy 唯一字符串替换 |
| `grep` | `grep.rs` | 是 | 是 | Parallel | 目录内正则搜索文件**内容** |
| `find` | `find.rs` | 是 | 是 | Parallel | 按 glob 按**文件名**找路径 |
| `ls` | `ls.rs` | 是 | 是 | Parallel | 列出单层目录条目 |
| `bash` | `bash.rs` | 是 | 是 | **Sequential** | 在项目内执行 shell 命令 |
| `web_fetch` | `web_fetch.rs` | 否（扩展） | 否 | Parallel | HTTP(S) 拉取并转为纯文本 |
| `web_search` | `web_search.rs` | 否（扩展） | 否（需 Tavily key） | Parallel | Tavily 联网搜索 |

**非工具模块**（不单独注册为 LLM 工具，但被工具依赖）：

| 模块 | 文件 | 作用 |
|------|------|------|
| `hashline` | `hashline.rs` | 行哈希锚点 `N#AB`，供 `read(hashline=true)` 与 `edit` |
| `diff` | `diff.rs` | `write`/`edit` 返回 unified diff 摘要 |
| `local_env` | `local_env.rs` | `FileSystem` / `Shell` trait 的本地实现（面向未来 `ExecutionEnv`） |
| `resolve_path` | `mod.rs` | CWD 沙箱：路径规范化 + 禁止逃出工作区 |

入口：`register_coding_tools`（`builtin.rs`），默认 `apply_pi_default_active_tools`（七件套，无 `web_*`）。

---

## 2. 共享设计原理

### 2.1 文本协议

所有工具对 LLM 的输入/输出以 **字符串（JSON 参数 → 执行 → 文本结果）** 为主，与 Pi 一致：不强制结构化 tool result schema，便于任意模型协议回灌。

### 2.2 CWD 沙箱

凡涉及本地路径的工具均经 `resolve_path`：

- 相对路径相对于进程 **当前工作目录**（通常为项目根）。
- `canonicalize` + 祖先解析，拒绝解析到 CWD 之外的最终路径（`SandboxViolation`）。
- **bash** 的 `workdir` 为子目录切换，仍应在项目树内由调用方约束；文件类工具硬拒绝越界。

设计意图：Coding Agent 默认「只动当前仓库」，降低误删系统文件风险；**不是**容器级隔离，恶意 `bash` 仍可能破坏用户环境，需 TUI 权限门控与人工监督。

### 2.3 输出截断

大结果会截断并提示，避免撑爆上下文窗口；模型应改用 `read` 精读或缩小搜索范围。

| 工具 | 典型上限 |
|------|----------|
| `read` | 单文件 1MB（读前检查 `metadata`） |
| `grep` | 最多 50 条匹配 |
| `find` | 最多 200 条路径 |
| `ls` | 最多 500 条 |
| `bash` | stdout/stderr 各约 50KB（`truncate_output`） |
| `web_fetch` | 响应体 1MB；返回文本默认 50KB |

### 2.4 原子写

`write` 与 `edit` 采用 **写临时文件 → `rename`**，降低中途崩溃导致半文件的概率。

### 2.5 阻塞 I/O 与异步

`grep` / `find` / `ls` 在 `spawn_blocking` 中做目录遍历，避免阻塞 tokio 运行时。`bash` 在 `execute_with_context` 中流式读 stdout 并支持取消。

### 2.6 并行批次

除 `bash` 为 `ExecutionMode::Sequential` 外，其余默认为 **Parallel**。同一批次中若包含 `bash`，**整批串行**（对齐 Pi：避免并行 shell 副作用）。见 [`UNCODE_TOOL_SYSTEM.md`](UNCODE_TOOL_SYSTEM.md)。

---

## 3. `read` — 读取与探查

**源码：** `crates/uncode-agent/src/tools/read.rs`

### 用途

- 读取源码、配置、日志等文本文件（分页）。
- 路径为目录时，返回排序后的条目列表（目录名带 `/` 后缀），等价于轻量「看一眼目录」。
- 开启 `hashline` 时为后续 `edit` 提供 **行号 + 行哈希锚点**。

### 何时由模型选用

用户/任务需要「看文件内容」「确认某段代码」「列出某目录有什么」；大文件应先 `offset`/`limit` 或配合 `grep` 定位再 `read`。

### 参数

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `path` | string | 是 | 文件或目录 |
| `offset` | integer | 否 | 起始行（0 起始语义与实现一致：从第 offset 行开始 skip） |
| `limit` | integer | 否 | 最多行数 |
| `hashline` | boolean | 否 | 为每行前缀 `行号#哈希`（如 `     5#KJ content`） |

### 设计原理

1. **先 `metadata` 再读**：超过 `max_size`（默认 1MB）直接报错，避免 OOM（#191）。
2. **目录即列表**：减少「目录误当文件读」的失败轮次；与 `ls` 分工：`read` 可顺带看单目录，`ls` 专用于列目录 API 更清晰。
3. **hashline 与 edit 闭环**：锚点基于行内容 `trim_end` 后 xxHash32 低字节 → 2 字符字母表编码；`edit` 应用前 `validate_anchors` 防止文件已变仍盲改。
4. **普通行号模式**：无 hashline 时输出 `行号: 内容`，便于人类与模型阅读，但不参与 edit 校验。

### 限制

- 二进制文件按 UTF-8 读可能失败或乱码；应用 `bash`/`file` 或勿读大二进制。
- 沙箱仅约束路径，不过滤敏感文件路径名。

---

## 4. `write` — 整文件写入

**源码：** `crates/uncode-agent/src/tools/write.rs`

### 用途

新建文件，或对已有文件 **完整替换** 内容（非补丁式）。

### 何时由模型选用

新模块、新测试、整文件重写；细粒度修改应优先 `edit` 以降低冲突与 token 浪费。

### 参数

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `path` | string | 是 | 目标文件 |
| `content` | string | 是 | 全文内容 |

### 设计原理

1. **覆写语义简单**：模型只需给出完整新文本，实现侧无需 diff 应用器，适合小文件或全新文件。
2. **自动 `create_dir_all`**：父目录不存在则创建，减少「路径不存在」往返。
3. **返回 unified diff**：委托 `uncode_core::diff::Patch`，让模型与用户看到变更摘要（最多约 50 行 hunk），便于确认。
4. **原子 rename**：与 `edit` 相同，避免半写状态。

### 限制

- 大文件整写占用上下文；超大变更可考虑分步 `edit`。
- 无「仅追加」模式；追加需 `edit` 的 `append` 或读-改-写。

---

## 5. `edit` — 精确修改

**源码：** `crates/uncode-agent/src/tools/edit.rs`（依赖 `hashline.rs`、`diff.rs`）

### 用途

在不大段复述原文的前提下修改文件：**hashline 锚点编辑**（推荐）或 **legacy 唯一子串替换**。

### 何时由模型选用

改几行、插删函数、修 typo；应用 `read(hashline=true)` 后再 `edit` 可精确定位。

### 参数（二选一模式）

**Hashline 模式**

| 参数 | 说明 |
|------|------|
| `path` | 文件路径 |
| `edits` | 数组，每项见下表 |

| 字段 | 必填 | 说明 |
|------|------|------|
| `op` | 是 | `replace` \| `prepend` \| `append` |
| `pos` | 是 | 起始锚点，如 `5#KJ`（来自 read） |
| `end` | 否 | 范围替换的结束锚点 |
| `lines` | 是 | 插入/替换的文本（可多行） |

**Legacy 模式**

| 参数 | 必填 | 说明 |
|------|------|------|
| `path` | 是 | 文件路径 |
| `old_string` | 是 | 必须 **唯一** 匹配 |
| `new_string` | 是 | 替换内容 |

### 设计原理

1. **Hashline 解决「行漂移」**：仅行号会在插入后失效；内容哈希锚点在行未改时稳定，对齐 Pi hashline 协议。
2. **三阶段应用**：① 解析并校验所有锚点；② 检测编辑区间不交叠；③ **自下而上** splice，避免行号被前序编辑打乱。
3. **Legacy 兼容**：与 Aider/Cursor 类「old_string/new_string」一致；0 次或多次匹配均报错，强制模型提供足够上下文。
4. **保留末尾换行**：若原文件以 `\n` 结尾，结果保持，减少「无 newline at EOF」噪音 diff。
5. **无变更短路**：内容相同返回 `no changes`，省一次无意义 diff。

### 限制

- 锚点校验失败时需重新 `read` 获取新锚点。
- 多文件批量改需多次调用；并行批次可并行多个 `edit`（不同 path）。

---

## 6. `grep` — 按内容搜索

**源码：** `crates/uncode-agent/src/tools/grep.rs`

### 用途

在目录树内用 **正则** 搜索**文件内容**，返回 `路径:行号:行文本`。

### 何时由模型选用

「哪里定义了 X」「哪些文件 import Y」「搜 TODO/FIXME」；先 `grep` 再 `read` 是典型工作流。

### 参数

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `pattern` | string | 是 | Rust `regex` 语法 |
| `path` | string | 否 | 搜索根，默认 `.` |
| `include` | string | 否 | 文件名 glob，如 `*.rs` |

### 设计原理

1. **WalkDir + 深度 20**：平衡覆盖面与性能，避免无限深 node_modules（仍可能很慢，依赖 `include` 收窄）。
2. **跳过不可读文件**：无权限或非 UTF-8 静默跳过，保证工具不中断。
3. **结果上限 50 条**：防止一次塞满上下文；截断时提示，引导缩小 `path`/`include` 或更精确 pattern。
4. **spawn_blocking**：目录遍历与读文件在阻塞线程池，不卡住 agent 事件循环。

### 与 `find` 的分工

| | `grep` | `find` |
|---|--------|--------|
| 匹配对象 | 文件**内容** | 文件**路径/名** |
| 模式 | 正则 | glob（`**/*.rs`） |

### 限制

- 非 UTF-8 二进制文件通常无匹配行。
- 极大仓库首次搜索可能慢；应用 `include` 或更浅 `path`。

---

## 7. `find` — 按路径模式查找

**源码：** `crates/uncode-agent/src/tools/find.rs`

### 用途

按 **glob** 收集匹配路径列表（如 `**/*.rs`、`src/**/*.toml`）。

### 何时由模型选用

枚举某类文件、找 `Cargo.toml`、列测试目录下所有 `*_test.rs`。

### 参数

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `pattern` | string | 是 | glob，相对 `path` |
| `path` | string | 否 | 根目录，默认 `.` |

### 设计原理

1. **实现简单**：`glob::glob("{root}/{pattern}")`，与 shell `find` 心智接近但跨平台一致。
2. **上限 200 条**：与 `grep` 同理控制 token。
3. **沙箱根路径**：`resolve_path` 保证搜索根在 CWD 内。

### 限制

- glob 拼在 `{root}/{pattern}`；复杂模式需注意是否多一层 `**`。
- 不搜索文件内容；内容用 `grep`。

---

## 8. `ls` — 列目录

**源码：** `crates/uncode-agent/src/tools/ls.rs`

### 用途

列出**单层**目录下的文件与子目录名（目录以 `/` 结尾），排序后换行输出。

### 何时由模型选用

快速看当前文件夹结构；比 `read` 目录语义更单一，schema 更简单。

### 参数

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `path` | string | 否 | 默认 `.` |

### 设计原理

1. **非递归**：避免与 `find`/`grep` 职责重叠；深层结构用 `find` 或 `read` 子目录。
2. **500 条上限**：防止超大目录刷屏。
3. **与 read 列目录的差异**：`read` 对目录返回带前缀说明的 listing；`ls` 仅名称列表，适合工具链中「纯 ls」意图。

### 限制

- 不显示权限、大小、修改时间（有意保持输出短）。

---

## 9. `bash` — Shell 执行

**源码：** `crates/uncode-agent/src/tools/bash.rs`（输出清理见 `local_env.rs`）

### 用途

运行构建、测试、git、包管理器等 **命令行** 任务；唯一默认 **Sequential** 工具。

### 何时由模型选用

需要编译、跑测试、安装依赖、git status 等无法仅靠静态读文件完成的操作。

### 参数

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `command` | string | 是 | 传给 `bash -c` 的脚本 |
| `description` | string | 否 | 5–10 词简述（供日志/UI，非执行逻辑） |
| `workdir` | string | 否 | 工作目录，默认 `.` |
| `timeout` | integer | 否 | 秒，默认 120，schema 限制 1–86400 |

### 设计原理

1. **Sequential 执行模式**：含 `bash` 的批次整批串行，避免并行 `cargo build` 等争用锁或污染同一目录。
2. **进程组 + SIGKILL（Unix）**：`process_group(0)`，取消时杀整组，避免孤儿进程。
3. **双路径执行**：
   - 简单路径：`execute` → `timeout` + `output()` 一次性收集；
   - 带上下文：`execute_with_context` → 行级 `on_progress`（TUI 可展示日志）+ `cancel_token`。
4. **输出清理**：`clean_binary_output` 剔除不可打印字符；`truncate_output` 限制体积。
5. **非沙箱 shell**：设计取舍是 FDE 需要真实环境能力；安全靠权限 Hook、用户确认与组织规范，而非假 bash。

### 限制

- Windows 上进程组行为与 Unix 不同（见 `cfg` 分支）。
- 长驻进程可能超时；需调大 `timeout` 或拆命令。
- `workdir` 不经过与文件工具相同的 `resolve_path` 越界检测（调用方应保持在项目内）。

---

## 10. `web_fetch` — 抓取网页

**源码：** `crates/uncode-agent/src/tools/web_fetch.rs`

### 用途

GET 指定 `http(s)` URL，将 HTML 转为近似纯文本（`html2text`），供模型阅读文档/API 页等。

### 何时由模型选用

需要阅读在线文档、Issue 页面、发布说明等；默认 **未 active**，需 `--tools web_fetch` 或 `--no-builtin-tools` 场景。

### 参数

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `url` | string | 是 | 仅 http/https |
| `max_length` | integer | 否 | 返回文本上限，默认 50KB |

### 设计原理

1. **uncode 扩展**：不在 Pi 默认七件套；注册即存在，避免未配置网络时模型误用。
2. **HTML → 文本**：降低 token 中标签噪音；非 HTML 按 UTF-8 文本处理。
3. **硬上限 1MB 响应体**：防止恶意/巨大页面拖垮内存。
4. **reqwest + 30s 超时**：简单可靠，无浏览器 JS 执行。

### 限制

- 不执行 JavaScript；SPA 可能只见空壳。
- 无 cookie/认证；内网或登录页需其他方式。
- 默认不对 LLM 暴露，需显式启用。

---

## 11. `web_search` — 联网搜索

**源码：** `crates/uncode-agent/src/tools/web_search.rs`

### 用途

通过 **Tavily API** 搜索互联网，返回摘要 answer（若有）+ 若干条 title/url/snippet。

### 何时由模型选用

需要较新外部知识、库文档、错误信息检索；**仅当**配置 `tavily` API key 时注册。

### 参数

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `query` | string | 是 | 搜索词 |
| `max_results` | integer | 否 | 默认 5 |

### 设计原理

1. **可选注册**：`WebSearchTool::try_new`，无 key 则不注册，避免运行时报错占工具槽。
2. **Tavily**：面向 agent 的搜索 API，`include_answer: true` 减少模型再摘要轮次。
3. **与 `web_fetch` 配合**：搜索得 URL 后再 fetch 精读（模型自行组合）。

### 限制

- 依赖第三方服务与密钥；非 Pi 内置。
- 默认不 active；成本与合规需团队自行评估。

---

## 12. 辅助模块（非独立工具）

### 12.1 `hashline`

**源码：** `crates/uncode-agent/src/tools/hashline.rs`

- **用途：** 为每一行生成 `N#AB` 锚点；`parse_anchor` / `validate_anchors` 供 `edit` 使用。
- **原理：** 行尾 trim 后 xxHash32 → 2 字符 16 字母表编码；校验时对比当前文件重算哈希，防止并发修改后误编辑。
- **设计对齐：** Pi Agent Rust hashline 协议。

### 12.2 `diff`

**源码：** `crates/uncode-agent/src/tools/diff.rs` → `uncode_core::diff::Patch`

- **用途：** `write`/`edit` 成功后返回 unified diff（最多约 50 行），无变更时 `no changes: path`。
- **原理：** 工具层只负责「给用户/模型看的变更摘要」，核心 diff 算法在 `uncode-core` 复用。

### 12.3 `local_env`

**源码：** `crates/uncode-agent/src/tools/local_env.rs`

- **用途：** 实现 `ExecutionEnv` 所需的 `LocalFileSystem`、`LocalShell`；`bash` 输出截断/清理。
- **原理：** 为将来沙盒 FS、远程执行预留抽象；当前 bash 仍主要直接用 `tokio::process`。

---

## 13. 工具组合与工作流建议

```text
探查结构     ls / find / read(目录)
定位内容     grep → read(offset/limit)
精读+修改    read(hashline=true) → edit
新建/重写    write
验证         bash (test/build)
外部资料     web_search → web_fetch   # 需显式启用 + API key
```

设计目标：读多写少、先搜后读、小改用 edit、大改用 write、重活交给 bash；网络工具默认隐藏以降低误用与泄露面。

---

## 14. 缺陷与优化（审计）

逐项源码审查见 [`UNCODE_BUILTIN_TOOLS_AUDIT.md`](UNCODE_BUILTIN_TOOLS_AUDIT.md)（P0–P3、测试缺口、修复优先级）。

---

## 15. 相关文档

- [`UNCODE_TOOL_SYSTEM.md`](UNCODE_TOOL_SYSTEM.md) — trait、生命周期、CLI、校验
- [`UNCODE_TOOL_SELECTION_BY_LLM.md`](UNCODE_TOOL_SELECTION_BY_LLM.md) — 模型如何选工具
- [`../guides/TOOL_SYSTEM.md`](../guides/TOOL_SYSTEM.md) — 面向使用者的简表
- [`../pi-technologies/PI_TOOL_SYSTEM.md`](../pi-technologies/PI_TOOL_SYSTEM.md) — Pi 对照

---

*文档版本：2026-05；与 `crates/uncode-agent/src/tools/` 源码同步。*
