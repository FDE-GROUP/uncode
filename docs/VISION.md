# uncode — 项目愿景与设计蓝图

## 一、项目定位

**uncode** 是一个面向中国大陆地区用户、Rust 原生的 Agent Coding 系统。它由两个核心组件组成：

| 组件 | 面向用户 | 定位 |
|------|---------|------|
| **TUI**（终端交互界面） | 开发人员（前线部署工程师、软件工程师） | "开发者工作台"——对话驱动，最大程度满足程序员需求 |
| **Platform**（Web 分析监控平台） | 软件工程师、技术管理者、项目参与者、非软件专业人员 | "可视化驾驶舱"——分析 Agent 数据、关联源码、管理 Issues、降低非技术用户门槛 |

**一句话：** TUI 让开发人员高效指挥 AI 写代码；Platform 让所有人都能理解和追踪 Agent 的工作。

**参考项目：** [earendil-works/pi](https://github.com/earendil-works/pi)，遵循其设计理念和框架，使用 Rust 重构，不涉及 Pi 的 web-ui 部分。

---

## 项目背景

随着 LLM 能力的持续增强，Agent Coding 已经使软件开发的工作方式发生了深刻变化：

- **架构设计、需求文档和技术文档的编写**等工作任务更加显得重要——这些是 AI 难以独立完成的创造性工作
- **代码编写和测试**等工作任务正逐步由 AI 工具来承担——从"人写代码"转向"人指挥 AI 写代码"
- 非软件专业人员也可以参与到软件开发中来，出现了**"氛围编程"（Vibe Coding）**的热潮——通过自然语言描述需求，由 AI 代为完成编码
- **前线部署工程师（FDE）**需求暴增——2025 年职缺量年增 800%，OpenAI、Anthropic 等 AI 公司大举招募。FDE 的核心价值在于"缩短 AI 应用与实际价值之间的落差"：MIT 研究表明 95% 的企业 GenAI 专案未带来投资回报，而那 5% 成功的企业普遍进行了深度客制化与流程整合——这正是 FDE 的任务

然而，目前的软件开发工具仍普遍**以代码为中心**，不能全面地反映 AI Agent 的工作过程和状态：
- 工具界面围绕代码文件、diff、终端输出等传统开发者概念构建
- 即便是专业开发者，也难以直观地追踪 Agent 的推理链、任务进度和决策过程
- 非软件专业人员想要参与 AI 驱动的软件开发，缺乏合适的可视化入口

**核心判断：** TUI 和 Platform 各有分工——TUI 是程序员的锋利工具，追求极致的键盘效率和开发体验；Platform 是面向所有人的可视化窗口，让非技术用户也能理解和追踪 Agent 的工作过程。

---

## 二、目标用户

uncode 的 TUI 和 Platform 面向不同用户群，各有侧重：

### 2.1 开发人员（TUI 用户）——核心用户

- 前线部署工程师（FDE）、软件工程师——**合格的程序员**
- TUI 设计目标：**最大程度满足开发人员的需求**，键盘优先、内联 diff、@ 文件引用、工具调用自定义渲染
- 不为非技术用户做任何妥协——终端操作、快捷键体系、权限确认都是开发者熟悉的概念

### 2.2 非软件专业人员（Platform 用户）——扩展用户

- 业务分析师、流程设计师、产品经理
- 不熟悉编程但有自动化需求
- 通过 Platform 的 Web 界面理解和追踪 Agent 工作过程——**非程序员友好由 Platform 承担，不是 TUI 的设计目标**

### 2.3 软件工程师（Platform 高级用户）

- 需要审计、分析、优化 Agent 的行为
- 核心需求：**数据驱动决策**，将 Agent 活动与源码关联

---

## 三、模型供应商策略

优先支持以下供应商（按优先级排序），确保中国大陆地区用户友好：

| 优先级 | 供应商 | 接入方式 | 说明 |
|--------|--------|---------|------|
| 1 | **GLM**（智谱） | API | 国内首选，中文理解最佳 |
| 2 | **DeepSeek** | API | 性价比极高，代码能力强 |
| 3 | **OpenRouter** | API 中转 | 多模型路由，降低接入门槛 |
| 4 | **Ollama** | 本地部署 | 完全离线，数据安全 |
| 5 | **OpenAI** | API | 国际主流，生态最完善 |
| 6 | **Anthropic** | API | 代码能力顶尖，长文本优势 |
| 7 | **Gemini** | API | Google 生态，多模态能力强 |

以上供应商分两批实现：

- **Phase 1**：GLM、DeepSeek、Ollama（中国大陆核心三选，首批可用）
- **Phase 2**：OpenRouter、OpenAI、Anthropic、Gemini（扩展国际覆盖）

---

## 四、TUI 设计理念

TUI 面向**开发人员**，参照 Pi 的对话式 TUI 设计，追求极致的开发体验。主区域是可滚动的对话历史，Agent 的思考、工具调用、代码变更自然地嵌入对话流中。**非程序员友好不是 TUI 的设计目标**——那是 Platform 的职责。

### 4.1 设计原则

- **对话驱动**：主区域是用户与 Agent 的对话流，工具调用内联展示
- **开发人员优先**：键盘优先操作、Markdown 渲染、内联 diff、@ 文件引用——不为非技术用户做任何妥协
- **即时反馈**：Agent 流式输出实时渲染，工具调用状态即时更新
- **权限控制**：危险操作（写入、执行命令）需用户确认后执行
- **渐进披露**：工具调用和思考过程默认折叠，按需展开

### 4.2 核心布局

```
┌──────────────────────────────────────────────────────────┐
│ uncode v0.2 │ DeepSeek-V3 │ session:abc3 │ 12.3k tokens │  ← 状态栏
├──────────────────────────────────────────────────────────┤
│                                                          │
│  > 帮我实现用户登录功能                                    │  ← 用户消息
│                                                          │
│  uncode:                                                 │  ← Agent 回复（Markdown）
│  我来分析现有代码结构，然后实现登录功能。                    │
│                                                          │
│  ┌─ 🛠 read src/auth.rs ─────────────────── ✅ 23ms ─┐  │  ← 内联工具调用
│  │  读取了 142 行代码                                  │  │
│  └────────────────────────────────────────────────────┘  │
│                                                          │
│  现有代码使用 JWT + Actix-web middleware 模式...          │
│                                                          │
│  ┌─ 🛠 edit src/auth.rs ──── ⚠️ 等待确认 ────────────┐  │  ← 权限请求
│  │  - fn login_handler() { ... }                       │  │  ← 内联 diff
│  │  + fn login_handler(req: HttpRequest) -> ... {       │  │
│  │  [Y] 确认  [N] 拒绝  [E] 编辑                        │  │
│  └────────────────────────────────────────────────────┘  │
│                                                          │
├──────────────────────────────────────────────────────────┤
│ > _                                                      │  ← 输入栏
├──────────────────────────────────────────────────────────┤
│ Ctrl+X 命令 │ ↑↓ 历史 │ Tab 补全 │ Enter 发送            │  ← 快捷键提示栏
└──────────────────────────────────────────────────────────┘
```

### 4.3 核心交互特性

1. **内联工具调用** — 工具调用以折叠方框嵌入对话流
   - 折叠显示工具名、文件路径、耗时
   - 展开显示完整输出、diff、错误详情
   - 每个工具有独立的 renderCall/renderResult 渲染策略（Pi 对齐）

2. **@ 文件引用** — 输入 `@file` 引用文件，Agent 自动读取
   - 支持 Tab 补全文件路径

3. **! Shell 快捷执行** — 两种 bash 模式（Pi 对齐）
   - `!command`：Agent 观察输出并继续工作
   - `!!command`：直接执行，Agent 不参与

4. **权限确认系统** — 写入/执行操作需用户确认
   - 只读命令（ls、grep、cargo check 等）自动允许

5. **斜杠命令** — /model、/session、/compact、/undo、/thinking、/tree 等

6. **快捷键体系** — Leader Key + 直接快捷键
   - `Ctrl+X` 前缀触发高级操作
   - `Ctrl+O` 切换工具输出、`Ctrl+T` 切换思考、`Ctrl+L` 模型选择

7. **会话管理** — 多会话创建、切换、恢复 + 树结构导航
   - `/tree` 命令展示会话分支树（Pi 对齐）

8. **计划/执行模式** — /plan 模式只分析不执行，/build 模式直接执行

9. **消息队列系统** — Agent 工作时用户可排队发送指令（Pi 对齐）
   - steering 消息：工具间隙插入，修正 Agent 方向
   - follow-up 消息：Agent 完成后追加

10. **思考级别系统** — 6 级思考深度控制（Pi 对齐）
    - Shift+Tab 循环切换，输入栏边框颜色随级别变化
    - 映射到 LLM thinking token 分配

11. **信息丰富页脚** — 两行页脚提供持续可见的关键信息（Pi 对齐）
    - 第 1 行：工作目录 + Git 分支 + 会话名
    - 第 2 行：Token 统计 + 费用 + 上下文使用率 + 模型 + 思考级别

12. **结构化主题系统** — ~50 命名色，JSON 配置，热重载（Pi 对齐）

### 4.4 技术栈

- **ratatui** + **crossterm** — 终端 UI 框架
- **tree-sitter** — 语法高亮
- **pulldown-cmark** — Markdown 渲染
- **tokio** — 异步运行时

---

## 五、Platform（分析监控平台）设计理念

Platform 面向程序员，是 Agent 活动的"事后分析"工具。前后端分离架构。

### 5.1 功能需求

1. **Agent 数据分析**
   - 按会话、项目、任务类型分类浏览
   - 时间线视图：Agent 的每一步操作按时间排列
   - 关键指标：Token 消耗、工具调用次数、任务完成率

2. **Issues 面板**
   - 浏览、筛选、管理 GitHub Issues
   - 与 TUI 任务清单的内容同步（推送为主、拉取为辅、关闭即终结）
   - 代码与 Issue 关联追溯（"为什么改 ↔ 改了什么"）

3. **源码/文档关联**
   - 点击工具调用记录可直接跳转到操作的文件
   - 展示每次编辑的 diff
   - 将会话与 git commit / PR 关联

4. **数据驱动优化**
   - 识别 Agent 的低效行为模式
   - 提示词优化建议
   - 工具调用成功率的趋势分析

### 5.2 技术栈

| 层 | 技术 |
|----|------|
| 前端 | TypeScript + React 19 + TanStack 全家桶（与 TOGAF TURBO 统一） |
| 后端 | Rust（axum / actix-web） |
| 数据存储 | SurrealDB（SurrealKV 嵌入 / TiKV 分布式） |
| API | REST + WebSocket（实时推送） |

### 5.3 Issues 面板设计

GitHub Issues 不应只作为 bug 追踪器，而应成为**项目全生命周期的意见平台**：需求提出、方案讨论、任务分解、进度追踪全在 Issues 中完成。Issues 面板是 Platform 的内置功能，负责 Issue 的浏览、筛选和管理。

**核心理念：**

- **需求即 Issue**：任何用户需求、功能请求、bug 报告都从创建 Issue 开始
- **内容同步，而非时间同步**：TUI 操作即时推送至 GitHub Issues，外部变更由用户通过 TUI 命令按需拉取
- **代码 ↔ 问题可追溯**：每个 commit/PR 都关联到 Issue，形成"为什么改 ↔ 改了什么"的完整链路

**同步模型（推送为主，拉取为辅）：**

```
TUI ──(即时推送)──→ GitHub Issues     ← Agent 操作时自动同步
TUI ←──(命令拉取)── GitHub Issues     ← 用户执行 /issues pull 手动刷新
```

- **TUI → Issues（推送）**：Agent 创建、更新 Issue 时，立即调用 GitHub API 推送变更。已关闭的 Issue 不再推送任何修改，保护历史记录完整性。
- **Issues → TUI（拉取）**：外部对 Issue 的变更（他人评论、状态修改等），由用户通过 `/issues` 命令主动拉取。
- **关闭即终结**：已关闭的 Issue 视为已定论的历史记录，TUI 不向其推送任何变更。如需继续工作，应基于原 Issue 创建新 Issue 启动新一轮任务。

**典型流程：**

1. 用户（任何角色）通过 Platform Issues 面板或直接在 GitHub 创建 Issue
2. 用户通过 CLI 参数（Phase 1：`uncode --issue 42`）或 TUI 命令（Phase 2+：`/issues pull`）拉取 Issue
3. Agent 将选中的 Issue 拆解为子任务，展示在 TUI 任务清单
4. 子任务完成时 Agent 自动推送状态到 Issue，完成后关闭 Issue

---

## 六、核心设计原则

### 6.1 性能优先

- Rust 原生，零成本抽象
- 异步 I/O 全链路
- 流式处理，尽早渲染，不等待完整响应

### 6.2 分层架构

```
┌─────────────────────────────────────────────────┐
│  uncode-cli         命令行入口                    │
├─────────────────────┬───────────────────────────┤
│  uncode-tui         │  uncode-platform           │
│  终端交互界面         │  分析监控平台服务端          │
├─────────────────────┴───────────────────────────┤
│  uncode-agent       代理循环引擎                  │
├──────────┬──────────┬──────────┬────────────────┤
│ uncode-  │ uncode-  │ uncode-  │ uncode-        │
│ llm     │ tools    │ session  │ extensions     │
│ LLM抽象  │ 内置工具  │ 会话管理  │ 扩展系统        │
├──────────┴──────────┴──────────┴────────────────┤
│  uncode-core        共享类型/trait/事件/错误       │
└─────────────────────────────────────────────────┘
```

### 6.3 数据通道

TUI 和 Platform 通过 **JSONL 会话文件** 桥接：

```
TUI/Agent ──(写入)──→ ~/.uncode/sessions/{id}.jsonl ←──(读取)── Platform
```

- TUI 产生的所有会话数据按 SESSION_SCHEMA.md 规范写入本地 JSONL 文件
- Platform 通过文件监听或定时扫描读取会话数据进行分析
- TUI 和 Platform 完全解耦——即使 Platform 未运行，TUI 也正常工作
- 团队场景下，Platform 后端可将 JSONL 数据导入 SurrealDB 分布式集群支持多用户查询

### 6.4 流式优先

- 所有 LLM 通信基于异步流
- 工具调用在 LLM 流式返回中即时触发，不等待完整响应
- 事件驱动的跨层通信（TUI/Platform 共享同一事件流抽象）

核心事件类型（Agent → TUI/Platform）：

| 事件 | 含义 | 对应 TUI 区域 |
|------|------|-------------|
| `SessionStart` | 会话开始 | — |
| `TaskUpdate` | 任务状态变更 | 对话区任务标签 |
| `ContentDelta` | LLM 返回的文本增量 | 对话区 Agent 消息/思考 |
| `ToolCallStart` | 工具调用开始 | 对话区内联工具方框 |
| `ToolCallProgress` | 工具执行进度 | 工具方框内 |
| `ToolCallEnd` | 工具调用完成 | 工具方框状态更新 |
| `PhaseSummary` | 阶段总结生成 | 对话区总结卡片 |
| `Error` | 发生错误 | 对话区错误卡片 |
| `TurnEnd` | 一轮对话结束 | 页脚 Token/费用更新 |
| `SessionEnd` | 会话结束 | — |
| `MessageQueued` | 用户消息进入排队（Pi 衍生） | 对话区底部排队预览 |
| `MessageDelivered` | 排队消息被投递（Pi 衍生） | 排队预览消除 |
| `CompactionComplete` | 上下文压缩完成（Pi 衍生） | 对话区压缩摘要卡片 |
| `BranchCreated` | 会话分支创建（Pi 衍生） | 对话区分支摘要卡片 |

### 6.5 可扩展

- 工具统一接口（内置工具与扩展工具无差别）
- 生命周期钩子全覆盖
- WASM 沙箱执行第三方扩展
- 斜杠命令可注册

---

## 七、分发策略

uncode CLI 二进制通过以下渠道分发：

- **GitHub Releases**：预编译的 Linux / macOS / Windows 二进制（CI 自动构建）
- **crates.io**：`cargo install uncode-cli` 供 Rust 开发者使用
- 后续考虑：Homebrew（macOS）、Scoop（Windows）、APT（Linux）

CI/CD 流水线在 Phase 1 配置，每次 Release 自动构建并发布二进制。

---

## 八、项目结构

```
uncode/
├── Cargo.toml                # Workspace 根
├── rust-toolchain.toml
├── .gitignore
├── AGENTS.md                 # AI 协作指引
├── docs/                     # 设计文档
│   ├── VISION.md             # 项目愿景（本文档）
│   ├── TUI_DESIGN.md         # TUI 交互设计详案
│   ├── PLATFORM_DESIGN.md    # Platform 设计详案
│   ├── ARCHITECTURE.md       # 架构详细设计
│   └── SESSION_SCHEMA.md     # 会话数据 JSONL Schema
├── crates/
│   ├── uncode-core/          # 共享类型、trait、错误、事件
│   ├── uncode-macros/        # 过程宏
│   ├── uncode-llm/           # LLM 驱动抽象 + 供应商实现
│   ├── uncode-session/       # 会话持久化（JSONL）
│   ├── uncode-tools/         # 内置工具集
│   ├── uncode-extensions/    # 扩展系统（规划中）
│   ├── uncode-agent/         # 代理循环引擎
│   ├── uncode-tui/           # 终端 UI（对话驱动 TUI，Pi 对齐）
│   ├── uncode-platform/      # Platform 服务端（规划中）
│   └── uncode-cli/           # 命令行入口
├── platform/                 # Platform 前端（TypeScript，规划中）
├── tests/                    # 集成测试
└── .github/workflows/        # CI/CD
```

---

## 九、与 Pi 的差异和改进

| 维度 | Pi (piso) | uncode |
|------|-----------|--------|
| **语言** | TS → Rust 渐进迁移 | **纯 Rust 从头构建** |
| **TUI 理念** | 对话驱动型（ChatContainer + 内联工具） | **对话驱动型**（Pi 对齐：内联工具调用/diff/权限确认/Bash 独立样式） |
| **消息队列** | steering + follow-up 双队列 | **对齐 Pi**（消息排队系统） |
| **思考级别** | 6 级 + 边框颜色反馈 | **对齐 Pi**（off/minimal/low/medium/high/xhigh） |
| **页脚** | 两行（位置+Token+费用+模型+思考级别） | **对齐 Pi**（同等信息密度） |
| **会话结构** | JSONL 树结构（id/parentId），/tree 导航 | **对齐 Pi**（分支导航） |
| **主题** | ~50 命名色，JSON，热重载 | **对齐 Pi**（结构化颜色分组） |
| **渲染** | 手动 ANSI diff + CSI 2026 同步 + 16ms 节流 | ratatui 全量重绘 + 虚拟滚动 + 渲染节流 |
| **第二组件** | 无 | **Platform**（分析监控平台） |
| **Issues 面板** | 无 | **Platform 内置 Issues 面板**（浏览/管理/关联） |
| **目标用户** | 程序员 | **开发人员（TUI 专属），非程序员由 Platform 覆盖** |
| **供应商优先级** | 国际（Claude/GPT 优先） | **中国大陆优先**（GLM/DeepSeek/Ollama） |
| **扩展系统** | 初步插件 API | **WASM 沙箱 + 9 个生命周期钩子** |
| **会话分析** | 无 | **Platform 数据分类 + 源码关联** |
| **代码仓库** | [pi-mono](https://github.com/earendil-works/pi) TypeScript | **[FDE-GROUP/uncode](https://github.com/FDE-GROUP/uncode) 纯 Rust** |

---

## 十、最小可体验版本（MTE）

MTE 是项目"用户第一次真正能用的"里程碑，为每个 Phase 提供明确的验收锚点。

| 维度 | 定义 |
|------|------|
| **首批用户** | 前线部署工程师 |
| **体验场景** | 从 GitHub Issue 到 PR 的全自动流程 |
| **验收标准** | 通过 CLI 参数传入 Issue 编号（如 `uncode --issue 42`），Agent 拉取 → 分析 → 拆任务 → 编码 → 测试 → 提交 PR，且 CI 绿灯通过 |

MTE 需要以下组件协同工作：
- Agent 循环引擎（uncode-agent）
- 文件读写编辑工具（uncode-tools）
- GitHub API 集成（拉取 Issue、提交 PR）
- 至少一个 LLM 供应商（GLM/DeepSeek/Ollama）
- 命令行入口（uncode-cli，print 模式）

TUI 界面和 Platform 不属于 MTE 范围——MTE 验证的是核心代理能力，交互界面和数据分析在后续阶段逐步加入。

---

## 十一、开发路线图

### Phase 0: 设计阶段

- [x] VISION.md（本文档）
- [x] TUI_DESIGN.md — TUI 交互设计详案
- [x] PLATFORM_DESIGN.md — Platform 设计详案
- [x] ARCHITECTURE.md — 架构详细设计
- [x] SESSION_SCHEMA.md — 会话数据 JSONL Schema
- [x] FDE_INSIGHT.md — FDE 角色深度解读

### Phase 1: 核心骨架

- [x] uncode-core 类型系统完善
- [x] uncode-llm 驱动接口 + GLM/DeepSeek/Ollama 实现
- [x] uncode-session JSONL 存储（按 SESSION_SCHEMA.md 规范）
- [x] uncode-tools 基础工具集（read, write, edit, grep, bash）
- [x] uncode-agent 基础代理循环
- [x] GitHub API 集成（Issue 拉取、PR 提交）
- [x] uncode-cli print 模式可用
- [x] **分层测试**：单元测试（core/llm/session/tools）+ 集成测试（Agent 端到端 golden test）+ CI 流水线
- [x] **CI 配置**：GitHub Actions 跑 cargo build / test / fmt / clippy
- [x] **MTE 达标**：Issue→PR 全流程，CI 绿灯

### Phase 2: TUI 原型

- [x] 4 大展示模块初步实现（任务清单 / 工具调用 / 思考过程 / 阶段总结）
- [x] 专业开发者代码细节视图（语法高亮、diff）
- [x] 输入编辑器（历史、Emacs编辑、Tab补全）
- [x] Markdown 渲染（pulldown-cmark）
- [x] Slash 命令系统（/help、/quit、/think 等）
- [x] Diff 查看器（多文件、n/p 导航）
- [x] 覆盖选择器（模型/会话选择）

### Phase 2.5: 扩展系统 + 国际供应商

- [x] WASM 扩展运行时基础框架（HookRegistry + Extension trait）
- [x] 生命周期钩子系统（8 个钩子）
- [x] 扩展工具注册 API
- [x] 补充国际供应商：OpenRouter、OpenAI、Anthropic、Gemini

### Phase 3: Agent 引擎增强（Pi 对齐）

- [x] 上下文压缩（Token估算 + 80%阈值 + LLM摘要）
- [x] SystemPromptBuilder（builder模式 + 工具指南 + 上下文 + 技能注入）
- [x] ContextLoader（CWD向上遍历 AGENTS.md/CLAUDE.md）
- [x] 技能系统（SKILL.md 加载注入）
- [x] Token 估算 + 费用计算（7 模型定价）
- [x] 会话分支（SessionManager::branch_session）
- [x] StopCondition trait（step_count_is / text_contains）
- [x] CompletionRequestBuilder（builder 模式）
- [x] `#[tool]` 宏增强（自动推导 JSON Schema）
- [x] Shell 补全生成（--completions）

### Phase 4: Platform 原型

- [x] Rust 后端服务（axum REST + WebSocket）
- [x] TypeScript 前端框架搭建（React 19 + TanStack Router/Query + Vite）
- [x] 会话数据展示 + 源码关联（Session 列表/详情/Diff 查看/Metrics API）
- [x] Issues 面板（GitHub Issues 代理 + 状态筛选）

### Phase 5: TUI 功能对齐（Pi 对标）

基于 CLI_TUI_COMPARISON.md 对比报告，缩小与 Pi 的功能差距：

- [ ] 斜杠命令扩展（/clear /compact /model /new /fork /export /branch /sessions /tree 等）
- [ ] 快捷键扩展（Ctrl+P 模型循环 / Ctrl+R 重试 / Ctrl+N 新会话 / Ctrl+/ 撤销）
- [ ] 输入编辑器增强（Shift+Enter 多行 / Undo-Redo / 单词导航 Alt+Left/Right / 外部编辑器 Ctrl+G）
- [ ] 选择器系统（会话选择器 / 设置选择器 / 模糊匹配）
- [ ] Markdown 渲染补全（表格 / 任务列表 / 数学 / OSC 8 链接）
- [ ] 会话管理命令（/sessions 列表 / /fork 分支 / /export HTML-JSONL / /tree 树形导航）
- [ ] JSON-RPC 模式（18 命令 + 25 事件，IDE 集成基础）
- [ ] 完善文档
- [ ] Agent 行为质量评估（Golden Set 测试）
- [ ] 安全增强（命令沙箱、SecretString、认证中间件）

---

*本文档是 uncode 项目的顶层设计指引，所有后续设计文档和代码实现均以此为准。*

当前进度：Phase 0-4 完成，Phase 5（TUI 功能对齐）进行中。
