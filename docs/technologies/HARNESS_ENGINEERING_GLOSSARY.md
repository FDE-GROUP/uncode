# Harness Engineering 术语索引

> 与 [`HARNESS_ENGINEERING.md`](HARNESS_ENGINEERING.md) 配套的中英对照术语表，便于查阅与统一团队用语。

| 项 | 说明 |
|----|------|
| **文档类型** | 术语索引 / Glossary |
| **路径** | `docs/technologies/HARNESS_ENGINEERING_GLOSSARY.md` |
| **最后更新** | 2026-05 |

---

## 使用说明

- 每条术语格式：**中文** | **English** — 简要定义；必要时标注参见主文档章节。
- 英文术语以业界常见写法为准；若作者原文有专名（如 `AGENTS.md`），保留原文。
- 索引分**主题分类**与**字母序附录**（英文 A–Z、中文拼音首字）两种查法。

---

## 一、范式与层次

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| 提示词工程 | Prompt Engineering | 研究如何组织单次或单轮输入（角色、约束、示例、格式等），使模型更准确理解意图。 | 主文档 §一 |
| 上下文工程 | Context Engineering | 在有限上下文窗口内策划与维护对任务最有用的 token 集合（系统提示、工具定义、历史、检索结果等），并应对上下文退化。 | 主文档 §一、§2.4 |
| 挽具工程 / 驾驭工程 | Harness Engineering | 围绕大模型构建可运行、可验证、可演进的 Agent 系统；广义上指 Agent 中除模型以外的一切。 | 主文档全文 |
| 广义 Harness | Harness (broad sense) | Agent 中除模型外的全部：编排、工具、沙箱、文档、测试、多 Agent、治理等。 | 主文档「术语」节 |
| 狭义 Harness Engineering | Harness Engineering (narrow sense) | 每次 Agent 犯错后，将根因沉淀为 `AGENTS.md` 规则或可执行工具，避免同类错误重演（Hashimoto）。 | 主文档 §六 |
| Agent | Agent | 能在外部环境中多步推理、调用工具并持续运行的 LLM 应用形态；非裸模型单次调用。 | 主文档 §二 |
| 模型 | Model | LLM 本体，负责推理与文本（及工具调用）生成。 | 主文档 §二 |
| Agent 公式 | Agent = Model + Harness | 工作定义：完整 Agent 由模型与 Harness 共同构成。 | 主文档 §2.2 |
| Harness 公式 | Harness = Agent − Model | 与上式等价，强调除模型外的工程职责边界。 | 主文档 §2.2 |

---

## 二、Harness 组成与能力

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| 编排循环 | Orchestration loop | 多步「推理—行动—观察」的控制循环，如 ReAct、`while`、子 Agent 调度。 | 主文档 §2.3 |
| ReAct | ReAct (Reason + Act) | 模型交替推理与调用工具、读取结果的 Agent 模式。 | 主文档 §2.3 |
| 工具 | Tools | Agent 可调用的外部能力（读文件、执行命令、HTTP、MCP 等）。 | 主文档 §2.3 |
| 技能 | Skills | 可渐进加载的能力说明包，常含专用 Prompt 与工具使用指引。 | 主文档 §2.3、§2.5 |
| MCP | Model Context Protocol (MCP) | 标准化工具/上下文服务协议；如 Playwright MCP 供 Evaluator 操作浏览器。 | 主文档 §4.2 |
| 沙箱 | Sandbox | 隔离、安全的代码与命令执行环境，可按任务创建与销毁。 | 主文档 §2.3 |
| 文件系统原语 | Filesystem primitive | Harness 基础能力：持久化工作区、跨会话状态、Agent 间协作面。 | 主文档 §2.3 |
| 通用执行 | General-purpose execution | 通过 bash / 代码执行让模型自造工具，而非为每任务预置全部工具。 | 主文档 §2.3 |
| 子 Agent | Subagent | 由主 Agent 派生的并行或分工实例，常带隔离上下文。 | 主文档 §2.3 |
| 交接 / 转手 | Handoff | 将任务或上下文从一环 Agent 传给下一环。 | 主文档 §2.3 |
| 中间件 / 钩子 | Hooks / Middleware | 在确定性节点插入的逻辑（压缩、续跑、Lint 检查等）。 | 主文档 §2.3 |
| 期望行为反推设计 | Behavior → Harness design | 先定义 Agent 应有行为，再反推 Harness 组件（LangChain）。 | 主文档 §2.3 |

---

## 三、上下文管理

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| 上下文 | Context | 模型单次调用可见的全部输入（提示、历史、工具列表、检索内容等）。 | 主文档 §一 |
| 上下文窗口 | Context window | 模型单次可处理的 token 上限。 | 主文档 §一 |
| 上下文退化 / 腐烂 | Context rot | 上下文越满，推理与任务质量往往越差的现象。 | 主文档 §一 |
| 上下文焦虑 | Context anxiety | 模型误以为接近上下文上限而提前收尾、压缩产出的倾向。 | 主文档 §4.1、§4.4 |
| 上下文压缩 | Compaction / Context compaction | 在同一会话内摘要或压缩早期内容，缩短历史但不断开同一 Agent。 | 主文档 §2.4 |
| 上下文重置 | Context reset | 清空上下文窗口，由新 Agent 凭结构化交接产物继续任务。 | 主文档 §2.4 |
| 渐进披露 | Progressive disclosure | 先给少量入口信息，按需加载更深文档或 Skills，避免一次性塞满窗口。 | 主文档 §3.2 |
| 工具输出卸载 | Tool output offloading | 超大工具结果写入文件，上下文仅保留摘要或头尾，按需再读。 | 主文档 §2.3 |
| 检索增强生成 | RAG (Retrieval-Augmented Generation) | 从外部知识库检索内容注入上下文。 | 主文档 §一 |

---

## 四、控制回路与质量门禁

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| 前馈 / 引导 | Guides (feedforward controls) | 在 Agent 行动**前**降低犯错概率的控制（规范、Skills、架构约束等）。 | 主文档 §五 |
| 反馈 / 传感 | Sensors (feedback controls) | 在 Agent 行动**后**观测结果并触发纠偏的控制（测试、Linter、评审等）。 | 主文档 §五 |
| 计算型控制 | Computational controls | 确定性、快速执行的控制（测试、类型检查、结构测试）。 | 主文档 §五 |
| 推理型控制 | Inferential controls | 依赖 LLM 语义判断的控制（代码评审 Agent、LLM Judge）。 | 主文档 §五 |
| 转向循环 | Steering loop | 人根据反复出现的失败持续改进 Guides/Sensors 的闭环。 | 主文档 §五、§六 |
| 静态分析 / Linter | Linter / Static analysis | 对代码或文档的机械规则检查；错误信息可注入 Agent 以自修复。 | 主文档 §3.4 |
| 结构测试 | Structural tests | 验证架构分层、依赖方向等不变量的测试（如 ArchUnit）。 | 主文档 §3.4、§五 |
| 架构门禁 | Architectural guardrails | 通过分层、依赖规则与机械校验防止架构漂移。 | 主文档 §3.4 |
| 适架构性函数 | Architectural fitness function | 度量架构目标（性能、可靠性等）是否满足的自动化检查。 | 主文档 §5.2 |
| LLM 评审 / 裁判 | LLM Judge / LLM as judge | 用模型对产出做语义质量或符合性评判。 | 主文档 §五 |
| 评审量表 | Rubric | 可操作的评分维度与通过标准，用于 Evaluator 等。 | 主文档 §4.2、§4.3 |

---

## 五、Fowler 三类 Harness 目标

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| 可维护性 Harness | Maintainability harness | 调节代码风格、复杂度、重复、架构边界等内部质量。 | 主文档 §5.2 |
| 架构适配 Harness | Architecture fitness harness | 调节性能、可观测性、可靠性等架构特性。 | 主文档 §5.2 |
| 行为 Harness | Behaviour harness | 调节功能是否符合用户意图；目前最难，测试全绿不等于产品做对。 | 主文档 §5.2 |
| 可挂挽具性 | Harnessability | 代码库是否易于接入 Harness（类型、模块边界、模板化等）。 | 主文档 §5.3 |

---

## 六、多 Agent 架构与角色

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| 单 Agent 模式 | Solo (single-agent mode) | 单一 Agent 端到端完成任务的基线方式。 | 主文档 §4.1 |
| 完整 Harness | Full Harness | 含 Planner、Generator、Evaluator 等多角色的编排架构。 | 主文档 §4.1 |
| 规划器 | Planner | 将短 Prompt 扩展为产品级 Spec，偏范围与高层设计。 | 主文档 §4.2 |
| 生成器 | Generator | 按 Spec 实现代码/产品，常分 sprint 并响应 Evaluator 反馈。 | 主文档 §4.2 |
| 评估器 | Evaluator | 独立于 Generator 的评判 Agent，常用浏览器自动化验证行为。 | 主文档 §4.2 |
| 初始化 Agent | Initializer agent | Anthropic 早期长时 Harness 中负责分解任务列表的 Agent。 | 主文档 §四 首段 |
| 编码 Agent | Coding agent | 早期 Harness 中逐功能实现的 Agent。 | 主文档 §四 首段 |
| 冲刺契约 | Sprint contract | Sprint 开始前 Generator 与 Evaluator 协商的可测试「完成定义」。 | 主文档 §4.2 |
| 冲刺 | Sprint | 一次可交付、可评估的工作增量。 | 主文档 §4.2 |
| 生成与评估分离 | Separation of generation and evaluation | 避免同一 Agent 既实现又评判；缓解自我感动。 | 主文档 §4.1、§4.5 |
| 承重分析 | Load-bearing analysis | 判断 Harness 某组件去掉后质量是否崩塌，以决定能否随模型升级拆除。 | 主文档 §4.4 |
| 脚手架 | Scaffolding | 为弥补当前模型不足而设的 Harness 结构；模型变强后可能不再承重。 | 主文档 §4.4、§8.2 |

---

## 七、知识与文档治理

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| Agent 指令文件 | `AGENTS.md` | 仓库内指导 Coding Agent 的入口文件；宜作地图而非百科全书。 | 主文档 §3.2、§六 |
| 单一事实来源 | SSOT (Single Source of Truth) | 权威信息集中在可版本化的仓库资产中，而非聊天或口头。 | 主文档 §3.2 |
| Agent 可读性 | Agent legibility | 仓库结构与文档使 Agent 能自行导航业务与规范，类似新人 onboarding。 | 主文档 §3.2 |
| 执行计划 | Execution plan / `exec-plans` | 复杂工作的版本化计划与进度、决策日志（OpenAI 实践）。 | 主文档 §3.2 |
| 文档园艺 | Doc-gardening | Agent 定期扫描过时文档并开修复 PR 的治理活动。 | 主文档 §3.2 |
| 黄金原则 | Golden principles | 写入仓库的、机械可执行的偏好与禁止项（OpenAI）。 | 主文档 §3.6 |
| 垃圾回收（治理） | Garbage collection (governance) | 持续小额自动清理技术债与漂移，类比运行时 GC。 | 主文档 §3.6 |
| AI 糟粕 / 低质堆积 | AI slop | Agent 生成导致的重复、偏离规范或低质代码/文档堆积。 | 主文档 §3.6 |
| 漂移 | Drift | 代码或文档逐渐偏离规范与真实行为。 | 主文档 §3.6 |

---

## 八、验证、可观测与运行时

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| 可观测性 | Observability | 日志、指标、追踪等运行时信号，供人与 Agent 排查。 | 主文档 §3.3 |
| Chrome 开发者工具协议 | Chrome DevTools Protocol (CDP) | 供 Agent 驱动浏览器、截图、检查 DOM 的协议。 | 主文档 §3.3 |
| LogQL | LogQL | Grafana Loki 等使用的日志查询语言；OpenAI 实验中对 Agent 开放。 | 主文档 §3.3 |
| PromQL | PromQL | Prometheus 指标查询语言。 | 主文档 §3.3 |
| Git 工作树 | Git worktree | 每变更独立检出与运行应用实例，便于隔离验证。 | 主文档 §3.3 |
| Playwright | Playwright | 浏览器自动化框架；Anthropic Evaluator 通过 MCP 使用。 | 主文档 §4.2 |
| 自验证闭环 | Self-verification loop | Agent 写代码 → 跑测试/看日志/点 UI → 根据结果修复的循环。 | 主文档 §2.3 |

---

## 九、协作分工与流程

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| 人掌舵、Agent 执行 | Humans steer, agents execute | 人定方向、规则与关键判断；Agent 在 Harness 内实现与集成。 | 主文档 §3.1 |
| 掌舵 | Steer | 人侧：意图、优先级、验收、Harness 迭代。 | 主文档 §3.1、§九 |
| 执行 | Execute | Agent 侧：编码、测试、开 PR、响应反馈。 | 主文档 §3.1、§九 |
| Agent 间评审 | Agent-to-agent review | PR 评审主要由 Agent 完成，人非必须。 | 主文档 §3.1 |
| Ralph Wiggum 循环 | Ralph Wiggum Loop | 拦截 Agent 退出意图、注入目标 Prompt，在新上下文继续迭代的 Harness 模式。 | 主文档 §3.1 |
| 编码 Agent（产品） | Coding Agent | 面向软件开发的 Agent 产品形态（如 Codex CLI、Claude Code）。 | 主文档 §五 |
| Agent 优先 | Agent-first | 仓库与流程优先为 Agent 可导航、可验证而设计。 | 主文档 §3.2 |
| 零手写代码（实验约束） | Zero manually-written code | OpenAI 实验的哲学性约束：实现代码不由人直接编写。 | 主文档 §3.1 |

---

## 十、模型协同与演化

| 中文 | English | 定义 | 参见 |
|------|---------|------|------|
| 上下文协同训练 | Post-training in the loop | 模型与特定 Harness 在训练/后训练中协同优化，换 Harness 可能影响表现。 | 主文档 §8.2 |
| Harness 迁移 | Harness moves | 模型变强后 Harness 组合转移而非整体消失（Anthropic）。 | 主文档 §4.4、结语 |
| 生成对抗网络（类比） | GAN (Generative Adversarial Network) | Anthropic 借用的「生成 vs 评判」分离思想；工程上非训练 GAN。 | 主文档 §4.5 |
| 从简单方案开始 | Start simple, increase complexity when needed | Anthropic 构建 Agent 的原则：先最小 Harness，再按需加复杂度。 | 参考文献 8 |

---

## 十一、缩写与专名速查

| 缩写 / 专名 | 英文全称或说明 | 中文 |
|-------------|----------------|------|
| P+G+E | Planner + Generator + Evaluator | 三 Agent 完整 Harness |
| PR | Pull Request | 拉取请求 |
| CI | Continuous Integration | 持续集成 |
| CDP | Chrome DevTools Protocol | 见上表 |
| MCP | Model Context Protocol | 见上表 |
| RAG | Retrieval-Augmented Generation | 检索增强生成 |
| SSOT | Single Source of Truth | 单一事实来源 |
| DAW | Digital Audio Workstation | 数字音频工作站（Anthropic V2 实验场景） |
| Spec | Specification | 产品/功能规格说明 |

---

## 附录 A：英文术语索引（A–Z）

| English | 中文 |
|---------|------|
| Agent | Agent / 智能体 |
| Agent legibility | Agent 可读性 |
| Agent-to-agent review | Agent 间评审 |
| Agent-first | Agent 优先 |
| Agent = Model + Harness | Agent 公式 |
| AI slop | AI 糟粕 |
| Architectural fitness function | 适架构性函数 |
| Architectural guardrails | 架构门禁 |
| Behaviour harness | 行为 Harness |
| Behavior → Harness design | 期望行为反推设计 |
| Chrome DevTools Protocol (CDP) | Chrome 开发者工具协议 |
| Coding agent | 编码 Agent |
| Compaction | 上下文压缩 |
| Computational controls | 计算型控制 |
| Context | 上下文 |
| Context anxiety | 上下文焦虑 |
| Context engineering | 上下文工程 |
| Context reset | 上下文重置 |
| Context rot | 上下文退化 |
| Context window | 上下文窗口 |
| Doc-gardening | 文档园艺 |
| Evaluator | 评估器 |
| Execution plan | 执行计划 |
| Filesystem primitive | 文件系统原语 |
| Full Harness | 完整 Harness |
| Garbage collection (governance) | 垃圾回收（治理） |
| GAN (analogy) | 生成对抗网络（类比） |
| General-purpose execution | 通用执行 |
| Generator | 生成器 |
| Golden principles | 黄金原则 |
| Guides (feedforward) | 前馈 / 引导 |
| Handoff | 交接 |
| Harness | 挽具 / Harness |
| Harness (broad sense) | 广义 Harness |
| Harness (narrow sense) | 狭义 Harness Engineering |
| Harness = Agent − Model | Harness 公式 |
| Harness engineering | 挽具工程 / Harness Engineering |
| Harness moves | Harness 迁移 |
| Harnessability | 可挂挽具性 |
| Hooks / Middleware | 中间件 / 钩子 |
| Humans steer, agents execute | 人掌舵、Agent 执行 |
| Inferential controls | 推理型控制 |
| Initializer agent | 初始化 Agent |
| LLM Judge | LLM 评审 |
| Load-bearing analysis | 承重分析 |
| Maintainability harness | 可维护性 Harness |
| Model | 模型 |
| Model Context Protocol (MCP) | MCP |
| Observability | 可观测性 |
| Orchestration loop | 编排循环 |
| Planner | 规划器 |
| Progressive disclosure | 渐进披露 |
| Prompt engineering | 提示词工程 |
| Ralph Wiggum Loop | Ralph Wiggum 循环 |
| RAG | 检索增强生成 |
| ReAct | ReAct |
| Rubric | 评审量表 |
| Sandbox | 沙箱 |
| Scaffolding | 脚手架 |
| Self-verification loop | 自验证闭环 |
| Sensors (feedback) | 反馈 / 传感 |
| Separation of generation and evaluation | 生成与评估分离 |
| Skills | 技能 |
| Solo (single-agent) | 单 Agent 模式 |
| Sprint | 冲刺 |
| Sprint contract | 冲刺契约 |
| SSOT | 单一事实来源 |
| Steering loop | 转向循环 |
| Structural tests | 结构测试 |
| Subagent | 子 Agent |
| Tools | 工具 |
| Tool output offloading | 工具输出卸载 |
| Zero manually-written code | 零手写代码 |

---

## 附录 B：中文术语索引（拼音序）

| 中文 | English |
|------|---------|
| Agent | Agent |
| Agent 公式 | Agent = Model + Harness |
| Agent 可读性 | Agent legibility |
| Agent 优先 | Agent-first |
| Agent 间评审 | Agent-to-agent review |
| AI 糟粕 | AI slop |
| 编码 Agent | Coding agent |
| 编排循环 | Orchestration loop |
| 架构门禁 | Architectural guardrails |
| 架构适配 Harness | Architecture fitness harness |
| 行为 Harness | Behaviour harness |
| 上下文 | Context |
| 上下文工程 | Context engineering |
| 上下文焦虑 | Context anxiety |
| 上下文压缩 | Compaction |
| 上下文窗口 | Context window |
| 上下文重置 | Context reset |
| 上下文退化 | Context rot |
| 冲刺 | Sprint |
| 冲刺契约 | Sprint contract |
| 单 Agent 模式 | Solo |
| 文档园艺 | Doc-gardening |
| 反馈 / 传感 | Sensors |
| 工具 | Tools |
| 工具输出卸载 | Tool output offloading |
| 广义 Harness | Harness (broad sense) |
| 黄金原则 | Golden principles |
| 规划器 | Planner |
| 渐进披露 | Progressive disclosure |
| 检索增强生成 | RAG |
| 可观测性 | Observability |
| 可维护性 Harness | Maintainability harness |
| 可挂挽具性 | Harnessability |
| 可编程工具（狭义） | Programmed tools (Hashimoto) |
| 垃圾回收（治理） | Garbage collection |
| 模型 | Model |
| 评估器 | Evaluator |
| 期望行为反推设计 | Behavior → Harness design |
| 前馈 / 引导 | Guides |
| 人掌舵、Agent 执行 | Humans steer, agents execute |
| 沙箱 | Sandbox |
| 生成器 | Generator |
| 生成与评估分离 | Separation of generation and evaluation |
| 技能 | Skills |
| 承重分析 | Load-bearing analysis |
| 挽具工程 | Harness Engineering |
| 完整 Harness | Full Harness |
| 狭义 Harness Engineering | Harness Engineering (narrow) |
| 漂移 | Drift |
| 提示词工程 | Prompt Engineering |
| 转向循环 | Steering loop |
| 掌舵 | Steer |
| 执行 | Execute |
| 执行计划 | Execution plan |
| 子 Agent | Subagent |
| 自我验证闭环 | Self-verification loop |
| 脚手架 | Scaffolding |
| 单一事实来源 | SSOT |
| 文件系统原语 | Filesystem primitive |
| 狭义 Harness | 见「狭义 Harness Engineering」 |
| 评审量表 | Rubric |
| 驾驭工程 | Harness Engineering（同挽具工程） |
| 零手写代码 | Zero manually-written code |
| Agent 指令文件 | `AGENTS.md` |
| Harness 公式 | Harness = Agent − Model |

---

## 相关文档

- [Harness Engineering 深度解读](HARNESS_ENGINEERING.md) — 概念、案例与参考文献正文
- [Coding Agent 工具开发指南](CODING_AGENT_TOOL_DEVELOPMENT.md) — 基于 Harness 的自研产品路径

---

*文档类型：术语索引。路径：`docs/technologies/HARNESS_ENGINEERING_GLOSSARY.md`。最后更新：2026-05。*
