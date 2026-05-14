# uncode — 项目愿景与设计蓝图

## 一、项目定位

**uncode** 是一个面向中国大陆地区用户、Rust 原生的 Agent Coding 系统。它由两个核心组件组成：

| 组件 | 面向用户 | 定位 |
|------|---------|------|
| **TUI**（终端交互界面） | 前线部署工程师、非软件专业人员 | "AI 工作台"——反映 Agent 工作过程 |
| **Platform**（分析监控平台） | 软件工程师、技术管理者、项目参与者 | "开发者驾驶舱"——分析 Agent 数据、关联源码、管理 Issues |

**一句话：** 让前线部署工程师更快交付方案，让非专业人员也能指挥 AI 写代码。过程可视化是增值，专业能力是根基。

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
- 非软件专业人员面对这些界面时感到困惑和排斥
- 即便是专业开发者，也难以直观地追踪 Agent 的推理链、任务进度和决策过程

**核心判断：** 随着 Agent Coding 成为软件开发的一种主流方式，软件开发工具应当从**"以代码为中心"向"以 Agent 运行状态为中心"**转变。uncode 正是基于这一判断而设计——让工具反映 Agent 的工作过程，而非仅仅展示它产生的代码。同时弥补现有工具对非软件专业人员不够友好的体验缺陷。

---

## 二、目标用户

uncode 首先是一款开发工具，**满足专业开发人员的使用需求是基本要求**。在此基础上，额外降低非软件专业人员的门槛。

### 2.1 前线部署工程师（Forward Deployed Engineer）——核心用户

- 合格的程序员，派驻到客户前线，在核心产品与客户需求之间架桥
- 需要快速为客户定制、集成、部署解决方案——"把研发成果带到客户所在领域，做客制化调整协助客户完成任务"
- 另一核心职责：把在前线观察到的客户痛点和场景带回核心产品团队，推动产品迭代
- 核心需求：**专业能力不打折**，比手动操作更快、更可靠，减少重复性的客制化开发工作

### 2.2 非软件专业人员——扩展用户

- 业务分析师、流程设计师、产品经理
- 不熟悉编程但有自动化需求
- 核心需求：**不被代码吓到**，关注"AI 在做什么"和"做到了哪一步"

### 2.3 软件工程师（Platform 用户）

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

TUI 是本项目的核心亮点。与传统代码编辑器型 TUI 不同，uncode TUI **以 Agent 工作过程为中心进行设计，但不牺牲代码查看能力**——专业开发者随时可以深入代码细节，非软件专业人员默认看到过程视图。

### 4.1 设计原则

- **技术人员优先**：默认展示完整四面板视图，信息密度匹配 IDE/tmux 用户习惯
- **专业功能不减配**：代码查看、diff、语法高亮、命令行交互等开发者必需功能完整保留
- **对非程序员友好**：提供 `/simple` 命令一键切换为简化视图（仅显示任务清单 + 阶段总结），无需理解代码即可理解 Agent 在做什么
- **过程可见**：Agent 的思考、决策、行动全程可视化
- **渐进式披露**：代码细节按需展开，非技术用户默认只看到过程的自然语言描述

### 4.2 核心展示区域

```
┌─────────────────────────────────────────────┐
│  📋 任务清单                                 │
│  ├── ✅ 1. 分析项目结构                       │
│  ├── 🔄 2. 实现用户认证模块    ← 正在进行       │
│  └── ⏳ 3. 编写测试                           │
├─────────────────────────────────────────────┤
│  🛠️ 工具调用                                 │
│  ├── read src/main.rs                        │
│  ├── grep "fn login"                        │
│  └── write src/auth.rs  ← 最新              │
├─────────────────────────────────────────────┤
│  💭 思考过程                                 │
│  │ 用户需要一个认证模块。我看到现有的项目结构   │
│  │ 使用 Actix-web，应该在 src/auth.rs 中...   │
├─────────────────────────────────────────────┤
│  📝 阶段总结                                 │
│  已完成：项目结构分析、认证模块代码生成          │
│  下一步：集成测试、配置文件更新                 │
└─────────────────────────────────────────────┘
```

### 4.3 四大展示模块

1. **任务清单** — Agent 的计划和进度
   - 自动拆解复杂的用户请求为子任务
   - 实时显示每个任务的状态（待办/进行中/已完成/受阻）
   - 用户可手动调整优先级和顺序
   - **与 GitHub Issues 内容同步**：TUI 任务完成后自动推送至 Issue，Issues 面板位于 Platform 中

2. **工具调用** — Agent 正在执行的操作
   - 展示被调用的工具名称、参数摘要
   - 实时显示执行状态（成功/失败/进行中）
   - 对大文件读取、长命令执行提供进度指示

3. **思考过程** — Agent 的推理链路
   - 展示 LLM 的思考内容（thinking/reasoning tokens）
   - 对技术用户可展示详细的推理链
   - 对非技术用户自动提炼为要点列表

4. **阶段总结** — 里程碑式进度汇报
   - 每完成一组关联任务后自动生成阶段总结
   - 列出已完成的工作、产生的问题、下一步计划
   - 支持用户确认后继续或调整方向

### 4.4 技术栈

- **ratatui** + **crossterm** — 终端 UI 框架
- **tree-sitter** — 语法高亮（仅代码细节视图启用）
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

| 事件 | 含义 | 对应 TUI 面板 |
|------|------|-------------|
| `SessionStart` | 会话开始 | — |
| `TaskUpdate` | 任务状态变更 | 任务清单 |
| `ContentDelta` | LLM 返回的文本增量 | 思考过程 |
| `ToolCallStart` | 工具调用开始 | 工具调用 |
| `ToolCallProgress` | 工具执行进度 | 工具调用 |
| `ToolCallEnd` | 工具调用完成 | 工具调用 |
| `PhaseSummary` | 阶段总结生成 | 阶段总结 |
| `Error` | 发生错误 | 全局 |
| `TurnEnd` | 一轮对话结束 | — |
| `SessionEnd` | 会话结束 | — |

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
│   ├── uncode-tui/           # 终端 UI（4 大展示模块）
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
| **TUI 理念** | 代码编辑器型 | **过程可视化型**（任务清单/工具调用/思考/总结） |
| **第二组件** | 无 | **Platform**（分析监控平台） |
| **Issues 面板** | 无 | **Platform 内置 Issues 面板**（浏览/管理/关联） |
| **目标用户** | 程序员 | **专业开发者为主，兼顾非专业人员** |
| **供应商优先级** | 国际（Claude/GPT 优先） | **中国大陆优先**（GLM/DeepSeek/Ollama） |
| **扩展系统** | 初步插件 API | **WASM 沙箱 + 生命周期钩子** |
| **会话分析** | 无 | **Platform 数据分类 + 源码关联** |

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

### Phase 0: 设计阶段（当前）

- [x] VISION.md（本文档）✓
- [ ] TUI_DESIGN.md — TUI 交互详案
- [ ] PLATFORM_DESIGN.md — Platform 设计详案
- [ ] ARCHITECTURE.md — 架构详细设计
- [ ] SESSION_SCHEMA.md — 会话数据 JSONL Schema 设计

### Phase 1: 核心骨架（→ MTE 达标）

- [ ] uncode-core 类型系统完善
- [ ] uncode-llm 驱动接口 + GLM/DeepSeek/Ollama 实现
- [ ] uncode-session JSONL 存储（按 SESSION_SCHEMA.md 规范）
- [ ] uncode-tools 基础工具集（read, write, edit, grep, bash）
- [ ] uncode-agent 基础代理循环
- [ ] GitHub API 集成（Issue 拉取、PR 提交）
- [ ] uncode-cli print 模式可用
- [ ] **分层测试**：单元测试（core/llm/session/tools）+ 集成测试（Agent 端到端 golden test）+ CI 流水线
- [ ] **CI 配置**：GitHub Actions 跑 cargo build / test / fmt / clippy
- [ ] **MTE 达标**：Issue→PR 全流程，CI 绿灯

### Phase 2: TUI 原型

- [ ] 4 大展示模块初步实现（任务清单 / 工具调用 / 思考过程 / 阶段总结）
- [ ] 非程序员友好的默认视图
- [ ] 专业开发者代码细节视图（语法高亮、diff）
- [ ] TUI 斜杠命令支持（`/issues pull` 等）

### Phase 2.5: 扩展系统 + 国际供应商

- [ ] WASM 扩展运行时 + 沙箱
- [ ] 生命周期钩子系统
- [ ] 扩展工具注册 API
- [ ] 补充国际供应商：OpenRouter、OpenAI、Anthropic、Gemini

### Phase 3: Platform 原型

- [ ] Rust 后端服务
- [ ] TypeScript 前端框架搭建
- [ ] 会话数据展示 + 源码关联
- [ ] Issues 面板

### Phase 4: 生产就绪

- [ ] 上下文压缩
- [ ] JSON-RPC 模式
- [ ] TUI 错误态完善（非程序员友好）
- [ ] Token 计数与成本追踪
- [ ] 完善文档

---

*本文档是 uncode 项目的顶层设计指引，所有后续设计文档和代码实现均以此为准。*
