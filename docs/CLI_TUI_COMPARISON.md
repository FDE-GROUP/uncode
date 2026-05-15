# uncode vs Pi（piso）CLI + TUI 功能深度对比报告

## 一、CLI 对比

### 1.1 规模

| 指标 | uncode | Pi (piso) |
|------|--------|-----------|
| 模块数 | 1（main.rs） | 8 模块 |
| 总行数 | ~280 | **3,866** |
| 启动参数 | 5 | **25+** |

### 1.2 参数对比

| 功能 | uncode | Pi |
|------|--------|-----|
| 模型选择 | `-m, --model` | `-m, --model` + compact syntax `provider/id:thinking` |
| 交互模式 | `-i, --interactive` | 默认交互，`--mode interactive` |
| Print 模式 | `--prompt` 或 pipeline | `-p, --print`，pipeline 自动检测 |
| 会话恢复 | `--session <ID>` | `-c/--continue`, `-r/--resume`, `--session <ID>` |
| 会话 fork | ❌ | `--fork <ID>` |
| Issue 集成 | `--issue <N>` ✅ | ❌（通过工具完成） |
| RPC 模式 | 计划中 | `--mode rpc` ✅（18 命令，25 事件） |
| 列出模型 | ❌ | `--list-models [FILTER]` |
| 列出会话 | ❌ | `--list-sessions` |
| 主题加载 | ❌ | `--theme <NAME>`（重复） |
| 技能加载 | ❌ | `--skill <NAME>`（重复） |
| 初始化 | ❌ | `--init`（生成模板配置） |
| Shell 补全 | `--completions` ✅ | `completions <SHELL>` 子命令 |
| 扩展管理 | ❌ | `install/remove/update/list` 子命令 |
| OAuth 登录 | ❌（仅 env var） | `login` / `logout`（GitHub Copilot device flow） |
| 版本检查 | ❌ | 后台异步检查 crates.io |
| 上下文文件 | ContextLoader | `--no-context-files`，可禁用 |
| 工具控制 | ❌ | `--no-tools` / `--tools allowlist` |
| 模型循环 | ❌ | `--models <PATTERNS>` 支持 Ctrl+P 切换 |

### 1.3 模式分发

| 模式 | uncode | Pi |
|------|--------|-----|
| Interactive（TUI） | `-i` | 默认（无参数 → TUI） |
| Print | `--prompt` 或无参数 → REPL | `-p` / pipeline |
| RPC | 计划中 | `--mode rpc` |
| ListModels | ❌ | `--list-models` |
| ListSessions | ❌ | `--list-sessions` |
| Init | ❌ | `--init` |

### 1.4 Print 模式管线深度

| 步骤 | uncode | Pi |
|------|--------|-----|
| 配置加载 | config.json | settings.json + 项目级覆盖 |
| Auth 优先级 | env var only | env var > auth.json > models.json > TS 兼容 |
| 模型解析 | 简单字符串 | compact 语法 `provider/id:thinking` |
| Provider 解析 | 注册表查找 | CLI > config > env var > 默认 |
| 工具注册 | 5 个内置工具 | 7 个（+Find +Ls） |
| 工具白名单 | ❌ | `--tools` 参数 |
| 会话解析 | `--session` | 4 种方式 + fork |
| 系统提示构建 | SystemPromptBuilder | ctx 文件 + skills + tool guides |
| 流式输出 | `print_messages()` | StreamSink + 实时 stdout |
| @file 引用 | ❌ | ✅（含图片 base64） |

---

## 二、TUI 对比

### 2.1 规模

| 指标 | uncode | Pi |
|------|--------|-----|
| 源文件 | 14 个 | **29 个** |
| 总行数 | ~3,000 | **11,133** |
| 测试 | 74 | — |

### 2.2 渲染方式

| 特性 | uncode | Pi |
|------|--------|-----|
| 终端库 | ratatui + crossterm | ratatui + crossterm |
| 布局 | 对话流 + 双行页脚 | 4+1 区域（chat/status/editor/footer + sidebar） |
| 宽屏适配 | ❌ | >=120 cols 启用 42-char sidebar |
| 增量渲染 | ratatui diff | ✅ |
| 滚动捕获 | ❌ | 仅鼠标滚轮（保留文本选择） |
| Unicode 宽度 | 部分 | ✅ 完整（CJK/Emoji/符号） |

### 2.3 对话渲染

| 特性 | uncode | Pi |
|------|--------|-----|
| Markdown 渲染 | pulldown-cmark ✅ | pulldown-cmark ✅（更完整） |
| – 标题 | ✅ | ✅（h1/h2 有分隔线） |
| – 代码块 | ✅ | ✅ + tree-sitter 高亮 |
| – Diff 块 | ❌ | ✅ 自动检测 + 语言高亮 + 颜色前缀 |
| – 列表 | ✅ | ✅（跟踪缩进深度） |
| – 引用 | ✅ | ✅（│ 前缀） |
| – 表格 | ❌ | ✅（3 列分块） |
| – 任务列表 | ❌ | ✅（☑/☐） |
| – 数学 | ❌ | ✅ |
| – 链接 | ❌ | ✅（OSC 8 hyperlinks） |
| – 截断 | 最近 20 行 | 前 50 + 省略 + 后 50 |
| 语法高亮 | 关键词匹配（5 语言） | **tree-sitter AST 级**（10 语言） |
| 工具渲染 | 自定义渲染器（7 种） | 自定义渲染（tool/bash）+ 状态图标 |
| 思考折叠 | ❌ | ✅（Ctrl+T 切换） |
| 工具输出折叠 | ❌ | ✅（Ctrl+O 切换） |
| Bash 独立样式 | ❌ | `▶` 运行 / `✗` 错误 / `※` 上下文排除 |
| 权限确认 | 3 级分类 ✅ | ❌（无内置权限 UI） |

### 2.4 输入编辑器

| 特性 | uncode | Pi |
|------|--------|-----|
| 基本编辑 | ✅ | ✅ |
| 多行 | ❌ | ✅（Shift+Enter） |
| 历史 | ✅（100 条） | ✅（100 条） |
| Undo/Redo | ❌ | ✅（50 步 + 词合并） |
| Kill Ring | ❌ | ✅（10 项，Ctrl+Y/Alt+Y） |
| Tab 补全 | ✅（slash + 路径） | ✅（slash + 模型 + @file + 路径） |
| Paste 处理 | ❌ | ✅（大段折叠标记） |
| 单词导航 | ❌ | ✅（Alt+Left/Right） |
| 单词删除 | ❌（Ctrl+W 有 UTF-8 问题） | ✅（Ctrl+W/Alt+D） |
| 行首/行尾删除 | ❌ | ✅（Ctrl+U/Ctrl+K） |
| 外部编辑器 | ❌ | ✅（Ctrl+G → $EDITOR） |

### 2.5 快捷键系统

| 特性 | uncode | Pi |
|------|--------|-----|
| 可配置快捷键 | ❌（硬编码） | ✅（JSON 加载，28 个动作） |
| 默认快捷键数 | 4（Ctrl+C/D/E/L） | **28** |
| 模型循环 | ❌ | ✅（Ctrl+P / Shift+Ctrl+P） |
| 思考切换 | ❌ | ✅（Ctrl+T） |
| 工具输出切换 | ❌ | ✅（Ctrl+O） |
| 重试 | ❌ | ✅（Ctrl+R） |
| 新建会话 | ❌ | ✅（Ctrl+N） |
| 撤销 | ❌ | ✅（Ctrl+/） |

### 2.6 选择器 / 弹窗

| 特性 | uncode | Pi |
|------|--------|-----|
| 模型选择器 | ❌ | ✅（296 行，toggle/enable/disable） |
| 会话选择器 | ❌ | ✅（274 行，sort/scope/delete） |
| 设置选择器 | ❌ | ✅（327 行，bool/choice/number） |
| 会话树选择器 | ❌ | ✅（479 行，5 种过滤模式） |
| 通用选择器 | OverlaySelector（j/k） | ✅（158 行 + fuzzy 过滤） |
| 模糊匹配 | ❌ | ✅（114 行评分算法） |
| Diff 查看器 | ✅（多文件 n/p） | ✅（360 行，j/k 滚动） |

### 2.7 主题系统

| 特性 | uncode | Pi |
|------|--------|-----|
| 主题数量 | 2（default/light） | **12** + 自定义 JSON |
| 颜色 token | ~50 命名色 | 25 语义 token |
| 热重载 | ❌ | ✅ |
| 灰度生成 | ❌ | ✅（`gray_steps(N)`） |
| 自定义加载 | JSON 文件 | JSON 文件 + `base` 继承 |

### 2.8 页脚 / 状态栏

| 特性 | uncode | Pi |
|------|--------|-----|
| 双行页脚 | ✅ | ✅ |
| Token 统计 | ✅（format_tokens） | ✅（↑N ↓M + k/M 格式） |
| 费用 | ✅ | ❌ |
| 上下文使用率 | ✅（>80% 红色） | ✅（<50%绿/50-80%黄/>80%红） |
| Git 分支 | ✅ | ✅（+ dirty 标记） |
| 模型 + 思考级别 | ✅ | ✅ |

### 2.9 特色功能

| 特性 | uncode | Pi |
|------|--------|-----|
| 终端图片 | ❌ | ✅（Kitty/Ghostty/WezTerm） |
| Git 集成 | ✅ | ✅ |
| 对话缓存 | ❌ | ✅（TranscriptCache） |
| ANSI 剥离 | ❌ | ✅ |
| 事件总线 | broadcast ✅ | ✅（8 种事件） |
| 消息队列 | ✅ | ✅（Steering + FollowUp） |
| 思考级别 | ✅（6 级） | ✅（6 级 + Shift+Tab） |
| 权限系统 | ✅（3 级） | ❌（无内置 UI） |
| 斜杠命令数 | ~8 | **31** |
| 工具渲染器 | ✅（7 种 + fallback） | ✅（内联 tool/bash 渲染） |

---

## 三、斜杠命令对比

| 命令 | uncode | Pi |
|------|--------|-----|
| /help | ✅ | ✅ |
| /quit | ✅ | ✅ |
| /simple / /full | ✅ | ❌（无此概念） |
| /think simple/full | ✅ | ✅（/compact 有 thinking 参数） |
| /unlock | ✅ | ❌ |
| /issues pull | 计划中 | ❌ |
| /clear | ❌ | ✅ |
| /compact | ❌ | ✅ |
| /model | ❌ | ✅（+ 参数和选择器） |
| /branch | ❌ | ✅ |
| /export | ❌ | ✅（HTML/JSONL） |
| /usage /cost | ❌ | ✅ |
| /find /grep | ❌ | ✅ |
| /new | ❌ | ✅ |
| /reload | ❌ | ✅ |
| /copy | ❌ | ✅（OSC 52 clipboard） |
| /fork | ❌ | ✅ |
| /session /info | ❌ | ✅ |
| /name | ❌ | ✅ |
| /import /clone | ❌ | ✅ |
| /sessions | ❌ | ✅ |
| /skill:<name> | ❌ | ✅ |
| /diff | ❌ | ✅ |
| /login /logout | ❌ | ✅ |
| /theme | ❌ | ✅ |
| /hotkeys | ❌ | ✅ |
| /settings | ❌ | ✅ |
| /tree | ❌ | ✅ |
| /delete | ❌ | ✅ |
| /scoped-models | ❌ | ✅ |

---

## 四、差异总结

### uncode 优势

1. **GitHub Issue 集成** — `--issue` 直接拉取 Issue 并自动处理
2. **权限系统** — 3 级权限（只读/写入/bash），比 Pi 的工具确认机制更结构化
3. **费用估算** — 7 模型定价表，页脚直接显示
4. **CLI REPL** — 无参数启动进入持续对话模式
5. **平台完整性** — Platform 前端 + 后端（Pi 无此概念）
6. **Rust 原生** — 无 JS 遗产，统一代码库

### Pi 优势

1. **CLI 成熟度** — 25+ 参数、18 RPC 命令、OAuth、版本检查
2. **TUI 成熟度** — 29 模块、11K 行、12 主题、28 快捷键、31 斜杠命令
3. **输入编辑器** — Undo/KillRing/多行/单词导航/外部编辑器
4. **选择器系统** — 4 种选择器 + 模糊匹配 + 树形过滤
5. **Markdown 完整度** — 表格/任务列表/数学/链接/截断策略
6. **语法高亮** — tree-sitter AST 级 vs 关键词匹配
7. **终端图片** — Kitty/Ghostty/WezTerm 协议
8. **扩展管理** — install/remove/update/list 子命令
9. **会话 fork/export** — JSONL 分支 + HTML 导出

### 差距量化

| 维度 | uncode 完成度 |
|------|-------------|
| CLI 参数 | **20%**（5/25） |
| TUI 模块 | **48%**（14/29） |
| TUI 行数 | **27%**（3K/11K） |
| 快捷键 | **14%**（4/28） |
| 斜杠命令 | **26%**（8/31） |
| 输入编辑器 | **30%** |
| Markdown 渲染 | **50%** |
| 选择器系统 | **20%** |
| 主题系统 | **40%** |

**综合 TUI 完成度：约 35%**
