# Harness Engineering 深度解读

> 从 Prompt Engineering、Context Engineering 到 Harness Engineering：如何理解 AI Agent 系统设计中的三层工作，以及它为何在 2026 年成为工程讨论的焦点。

| 项 | 说明 |
|----|------|
| **文档类型** | 行业概念综述（非产品实现规范） |
| **路径** | `docs/technologies/HARNESS_ENGINEERING.md` |
| **术语索引** | [`HARNESS_ENGINEERING_GLOSSARY.md`](HARNESS_ENGINEERING_GLOSSARY.md)（中英对照） |
| **开发指南** | [`CODING_AGENT_TOOL_DEVELOPMENT.md`](CODING_AGENT_TOOL_DEVELOPMENT.md)（如何自研 Coding Agent 工具） |
| **最后更新** | 2026-05 |

---

## 前言

继 **Prompt Engineering**、**Context Engineering** 之后，业界在 **2026 年初**集中讨论一个新词：**Harness Engineering**（挽具工程 / 驾驭工程）。

| 日期 | 来源 | 要点 |
|------|------|------|
| 2026-02-05 | Mitchell Hashimoto — [*My AI Adoption Journey*](https://mitchellh.com/writing/my-ai-adoption-journey) | 提出「Engineer the Harness」：Agent 犯错时，用 `AGENTS.md` 与可执行工具让错误**不再重复** |
| 2026-02-11 | OpenAI — [*Harness engineering: leveraging Codex in an agent-first world*](https://openai.com/index/harness-engineering/) | 约 5 个月、近百万行、零手写代码实验；**Humans steer, agents execute** |
| 2026-02-17 | Martin Fowler — [初稿备忘录](https://martinfowler.com/articles/exploring-gen-ai/harness-engineering-memo.html) | Coding Agent 场景的 Harness 与控制论式框架 |
| 2026-03-10 前后 | LangChain — [*The Anatomy of an Agent Harness*](https://blog.langchain.com/the-anatomy-of-an-agent-harness/) | 明确 **Agent = Model + Harness**，从「期望行为」反推组件 |
| 2026-03-24 | Anthropic — [*Harness design for long-running application development*](https://www.anthropic.com/engineering/harness-design-long-running-apps) | Planner / Generator / Evaluator；生成与评估分离 |
| 2026-04-02 | Martin Fowler — [*Harness engineering for coding agent users*](https://martinfowler.com/articles/harness-engineering.html) | Guides / Sensors、可维护性 / 行为正确性等维度 |

有人认为这是把旧工程换了个新名字；也有人认为它终于给「Agent 工程」提供了可讨论的**统一框架**。

**本文目标**：厘清概念、对照公开案例、区分不同作者下的**两种 Harness 含义**，并说明 Harness 与 Prompt / Context 如何配合。

---

## 术语：两种常见用法（阅读前先读）

同一词在不同文章里**范围不同**，混读容易误解：

| 用法 | 谁在用 | 含义 |
|------|--------|------|
| **广义 Harness** | LangChain、OpenAI、Martin Fowler | **Agent 中除模型以外的一切**：编排、工具、沙箱、文档、测试、多 Agent…… |
| **狭义 Harness Engineering** | Mitchell Hashimoto | **迭代式纠错工程**：每次 Agent 犯错的根因，沉淀为 `AGENTS.md` 规则或可执行验证工具，使同类错误不再发生 |

二者不矛盾：Hashimoto 的狭义实践，是广义 Harness 里「反馈与治理」层的具体方法论。下文默认用**广义**，并在涉及 Hashimoto 时标明**狭义**。

---

## 一、三个层次：问什么、给什么、搭什么

三者解决不同问题，但**不是严格的历史阶段**，也**不是互斥工种**。更准确的图景是：

- **Prompt Engineering** — 单次（或单轮）输入如何表述；
- **Context Engineering** — 模型上下文窗口里**装什么、何时装、如何省着用**；
- **Harness Engineering** — 围绕模型的一整套**可运行、可验证、可演进**的系统，通常**已包含**前两者。

Anthropic 在 [*Effective context engineering for AI agents*](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) 中将 Context Engineering 描述为：在有限上下文窗口内，**策划与维护**对任务最有用的 token 集合（系统提示、工具定义、检索结果、历史、状态等），并应对 **context rot**（上下文越满，推理质量往往越差）。

| 层次 | 核心问题 | 典型手段 |
|------|----------|----------|
| **Prompt Engineering** | 怎么把需求说清楚？ | 角色、约束、Few-shot、输出格式 |
| **Context Engineering** | 窗口里放什么？ | 压缩（compaction）、RAG、渐进披露、工具输出卸载到文件 |
| **Harness Engineering** | 如何稳定交付？ | ReAct 循环、沙箱、测试/Linter、多 Agent、可观测性、SSOT 文档、架构门禁 |

**关系示意**（概念模型，非唯一实现）：

```
Harness
├── 编排（循环、阶段、多 Agent、路由）
├── Context 策略（压缩 / reset / 检索 / Skills 披露）
├── Prompt 与知识（AGENTS.md、Skills、Spec）
├── 工具与环境（bash、MCP、浏览器、沙箱）
├── 验证（测试、Linter、Playwright、LLM Judge）
└── Model API 调用
```

Martin Fowler 强调：Harness 与 Context Engineering **配合**，而非后者被「淘汰」。他用 **Guides（前馈）** 与 **Sensors（反馈）** 描述控制回路，并区分 **计算型**（确定性）与 **推理型**（语义评审）执行器。

---

## 二、Harness 是什么

### 2.1 词源与双重含义

**Harness（挽具）** 让人能**引导**并**使用**马力，而非仅「拴住」。

映射到 AI 时：

| 维度 | 作用 |
|------|------|
| **约束** | 权限、架构边界、类型与 Linter、减少幻觉与漂移 |
| **赋能** | 状态、工具、文件系统、沙箱、多步循环、跨会话持久化 |

LangChain 指出：裸模型输入输出文本；**不能**自带持久状态、执行代码、获取实时知识、自建环境——这些均为 Harness 能力。连「聊天」也需要 Harness 里的 `while` 循环来维护消息历史。

### 2.2 工作定义与公式

> **Agent = Model + Harness**  
> 等价于：**Harness = Agent − Model**

这是**职责划分**的工作定义，不是数学公理。模型负责推理与生成；Harness 负责让推理发生在**正确的环境、上下文与反馈**之中。

### 2.3 Harness 组件清单（归纳自公开材料）

不同产品裁剪不同，但反复出现的组件包括：

| 组件 | 解决的问题 | 典型实现 |
|------|------------|----------|
| **编排循环** | 多步推理—行动—观察 | ReAct、`while`、子 Agent、handoff |
| **文件系统 / Git** | 持久化、协作面、跨会话状态 | workspace、`AGENTS.md`、plan 文件 |
| **通用执行** | 不必为每个任务预置工具 | bash / 代码执行 |
| **沙箱** | 安全、隔离、可扩展 | 容器、网络隔离、按需创建/销毁 |
| **浏览器 / 可观测性** | UI 与运行时验证 | Chrome DevTools Protocol、LogQL、PromQL |
| **Context 中间件** | context rot、窗口溢出 | compaction、tool 输出卸载、Skills 渐进披露 |
| **长时续跑** | 跨窗口任务 | context **reset** + 结构化交接、Ralph Loop |
| **规划与评估** | 复杂任务分解、抑制自我感动 | Planner、独立 Evaluator、Sprint Contract |
| **架构门禁** | 防止 Agent 堆屎山 | 自定义 Linter、结构测试、分层依赖规则 |
| **持续治理** | 文档与代码漂移 | 定期扫描、自动修复 PR（garbage collection） |

设计思路（LangChain）：**期望行为 → Harness 设计**；先定义 Agent 应如何工作，再反推需要哪些原语。

### 2.4 Context：Compaction、Reset 与 Harness 的分工

长时任务里，Anthropic 明确区分：

| 机制 | 行为 | 适用 |
|------|------|------|
| **Compaction** | 在**同一会话**内压缩/摘要早期内容，历史变短但 Agent 未换脑 | 需连续性、焦虑未严重时 |
| **Context reset** | **清空窗口**，新 Agent 凭**结构化交接产物**接续 | context anxiety 严重、需「干净 slate」 |

Harness 选择 reset 还是 compaction，是**工程决策**，不是 Prompt 能替代的。模型升级（如 Opus 4.5）可能降低 reset 频率，但不会消除「窗口有限」这一物理约束。

### 2.5 Prompt / Context 是否「过时」？

没有。模型变强后，手写超长 Prompt 的需求可能下降，但：

- Prompt 常沉淀为 **Skills、`AGENTS.md`、评审 Rubric**；
- Context 策略（加载哪份 `docs/`、何时 compact）仍是 Harness 核心子问题。

OpenAI 的教训是：不要给 Agent「1000 页手册」，而应给**地图（~100 行 `AGENTS.md`）+ 可导航的 `docs/` SSOT**。

---

## 三、OpenAI 的 Harness Engineering 实践

> 来源：Ryan Lopopolo，OpenAI，2026-02-11。以下数据均为**团队公开自述**，外推需谨慎。

### 3.1 实验概况

- **起点**：2025 年 8 月下旬，**空 Git 仓库**首 commit；脚手架、CI、`AGENTS.md` 初版均由 Codex 生成。  
- **周期**：约 **5 个月**至 2026 年 2 月发文。  
- **规模**：约 **100 万行**代码；约 **1500** 个已合并 PR；团队约 **3→7** 人；约 **3.5 PR / 人 / 天**（团队称随人数增加吞吐仍上升）。  
- **约束**：**无人工手写代码**（哲学性规则）；人通过 **Prompt / 评审意图 / Harness 设计** 介入。  
- **效率声称**：约为手工编码 **1/10** 时间——依赖特定产品、基础设施与极高 Agent 吞吐量，**非普适 KPI**。

**Humans steer. Agents execute.**

人优先投入：拆目标、补能力缺口、设计反馈；Agent 在 Harness 内实现、测试、开 PR、常做 **Agent 间评审**（接近 [Ralph Wiggum Loop](https://ghuntley.com/loop/)：自审、请求其他 Agent 评审、迭代至满意）。

单次 Codex 运行**常见 6 小时以上**（含人休息时后台运行）。

### 3.2 知识：地图、SSOT 与 Agent 可读性

**失败**：单文件巨型 `AGENTS.md` — 挤占任务相关上下文、一切皆重要等于都不重要、易腐烂、难机械校验。

**成功**：

- `AGENTS.md` **~100 行**，作目录，指向 `docs/` 下分层知识库（设计文档、架构、`exec-plans`、质量评分、安全规范等）；  
- **渐进披露**：按需深入，而非一次灌入；  
- **SSOT**：Slack/口头对齐若未进仓库，对 Agent **等同不存在**；  
- **机械校验**：Linter / CI 检查文档结构、交叉链接、新鲜度；**doc-gardening** Agent 扫描过时文档并开修复 PR。

目标不仅是 Context Engineering，而是 **agent legibility（Agent 可读性）**：仓库优先为 Codex 导航优化，类似为新同事优化 onboarding。

### 3.3 验证：让应用与可观测性「对 Agent 可读」

瓶颈从「写代码」转向「人做 QA 的能力」后，团队让 **UI、日志、指标** 可直接被 Agent 使用：

- **按 Git worktree 启动应用实例**，每变更隔离运行；  
- **Chrome DevTools Protocol** + DOM 快照 / 截图 / 导航 Skills；  
- **临时可观测性栈**：LogQL（日志）、PromQL（指标）；任务结束环境销毁；  
- 使「启动 <800ms」「关键链路 span <2s」类目标对 Agent **可验证**。

### 3.4 架构：约束不变量，而非微管实现

- 业务域内**固定分层**与**严格依赖方向**（如 Types → Config → Repo → Service → Runtime → UI）；  
- **自定义 Linter + 结构测试** 机械执行；错误信息内嵌**修复指引**（注入 Agent 上下文）；  
- 边界中央强制、实现局部自治；代码风格可不符合人类审美，但须**正确、可维护、对未来 Agent 可读**。  
- 人类品味通过 Review 评论、重构 PR、缺陷**回流**为文档或工具规则。

### 3.5 吞吐与合并哲学

Agent 吞吐超过人类注意力时，**短生命周期 PR、较少阻塞合并门、测试 flake 常重跑而非无限阻塞** 可能成为理性权衡——这在低吞吐团队里通常**不负责任**，但在该实验前提下被描述为合理。

### 3.6 技术债：Golden Principles 与 Garbage Collection

Agent 会复制仓库里已有模式（包括次优模式）→ **漂移**。

团队曾每周五用 20% 时间人工清理「AI slop」，后改为：

- 将 **golden principles** 写入仓库（如：优先共享 utility、禁止未类型边界的「YOLO 探测」）；  
- **定期后台 Codex 任务**扫描偏离、更新质量评分、开小型重构 PR（常 <1 分钟人审即可合并）。

类比 **垃圾回收**：持续小额还债优于债务爆发式偿还。

### 3.7 自治水平（不宜外推）

在充分投资后，单 Prompt 可驱动端到端流程（验证现状 → 复现 Bug → 录屏 → 修复 → 再验证 → 开 PR → 响应反馈 → 处理 CI 失败 → 仅在需判断时升级给人 → 合并）。  

OpenAI **明确警告**：该行为高度依赖此仓库的结构与工具链，**不应假设**未经类似投入即可复现。

### 3.8 可提炼原则

1. 进展慢时先问「缺什么**能力**」，而非「让模型再努力一点」。  
2. 给**地图 + SSOT**，不给百科全书。  
3. 确定性检查尽可能**计算化**；语义判断用受控推理型 Sensor。  
4. 最难问题变成：**环境、反馈回路、控制系统**的设计。

---

## 四、Anthropic：长时任务、生成/评估分离与 Harness 演化

> 主文：2026-03-24。早期基础：[Effective harnesses for long-running agents](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents)（Initializer + 逐功能 Coding Agent + context reset）。

### 4.1 为何 Solo 不够

长时、多功能的应用构建中，常见失败模式：

1. **上下文相干性**：窗口填满后逻辑断裂；**context anxiety**（误以为快超限而提前收尾）。  
2. **自我评估偏松**：模型易「表扬自己的产出」，UI 能看但核心功能坏掉。  
3. **评估与生成未分离**：同一 Agent 既写又判，难以严格。

**对照实验**（一句 Prompt：2D 复古游戏制作器；Opus 4.5；Anthropic 原文）：

| 模式 | 耗时 | 成本 | 结果概览 |
|------|------|------|----------|
| Solo | ~20 min | ~$9 | 有界面；实体/输入常未正确接通，难以可玩 |
| Full Harness（P+G+E） | ~6 hr | ~$200 | 范围更大（动画、音效、AI 辅助等）；**可玩性**显著更好 |

成本约 **20×+**，购买的是**可验证质量**，不是延迟。

### 4.2 Full Harness：三 Agent 与 Sprint Contract

| 角色 | 职责 |
|------|------|
| **Planner** | 短 Prompt → 产品级 Spec；偏范围与高层设计，避免错误细节固化 |
| **Generator** | 按 Spec **分 sprint** 实现（React/Vite/FastAPI/SQLite 等栈）； sprint 末自测 |
| **Evaluator** | **Playwright MCP** 操作用户路径；按 Rubric 判 pass/fail；失败则具体反馈 |

**Sprint Contract**：Spec 保持高层；每个 sprint 开始前 Generator 与 Evaluator **协商可测试的完成定义**，再编码。Agent 通过**文件**读写交接。

Evaluator **出厂并不靠谱**：早期会「发现问题又自我说服放过」；需通过日志与 Prompt 迭代培养**怀疑论**，仍可能存在漏测。

### 4.3 前端实验：主观质量的可评分化

同一研究路线中，**Generator / Evaluator 分离**也用于前端：四维度（设计整体性、原创性、工艺、功能），Evaluator 用 Playwright 交互后评分，Generator 多轮迭代（可达数小时）。说明 Harness 可覆盖**主观质量**，前提是标准可 operationalize。

### 4.4 模型升级与 Harness 精简（V1 → V2）

| 变化 | 背景 | 做法 |
|------|------|------|
| Opus 4.5 | context anxiety 减轻 | 可减少 **context reset**；长会话可更连续 |
| Opus 4.6 | 规划、长时连贯、自审增强 | 去掉 **per-sprint** 切分；Generator 连续工作；Evaluator **阶段末**评估 |

**DAW（浏览器 Web Audio API）V2 实验**（Anthropic 原文）：总长约 **3h50m**，约 **$124.70**；Planner 极便宜；主要成本在 Builder；QA 仍发现「功能完整度」类硬缺口。

**结论（Anthropic 原话精神）**：

- 拆掉的是**不再承重**的脚手架（load-bearing analysis）；  
- Evaluator 在任务仍超出 Generator 可靠边界时**仍有价值**；  
- **有趣的 Harness 组合不会消失，而是迁移（moves）**——模型越强，可接更复杂任务，Harness 随之重组。

### 4.5 与「生成对抗」思想的联系

全栈 Harness 受 **GAN** 启发：**生成与评判分离**，缓解自我感动。前端与后端路线共享这一结构，但工程实现是 Planner + 文件交接 + 浏览器 QA，而非训练对抗网络。

---

## 五、Martin Fowler：Coding Agent 的 Harness 框架

Birgitta Böckeler（Thoughtworks）长文（2026-04-02）将 Coding Agent 外层 Harness 形式化为控制论问题：

### 5.1 两个控制维度

| | **Guides（前馈）** | **Sensors（反馈）** |
|--|-------------------|---------------------|
| **目的** | 行动前降低犯错概率 | 行动后纠偏 |
| **计算型** | Skills、bootstrap 脚本、OpenRewrite | 测试、Linter、ArchUnit |
| **推理型** | `AGENTS.md`、设计规范 | LLM 评审、Judge |

仅反馈 → Agent 重复同类错误；仅前馈 → 不知规则是否奏效。

### 5.2 三类调节目标

| 类别 | 调节什么 | 成熟度 |
|------|----------|--------|
| **可维护性 Harness** | 风格、复杂度、重复、架构边界 | 工具较成熟 |
| **架构适配 Harness** | 性能、可靠性、可观测性约定 | Fitness Function |
| **行为 Harness** | 功能是否符合用户意图 | **最难**；测试全绿 ≠ 产品做对 |

### 5.3 Harnessability

代码库是否易于挂 Harness：**强类型、清晰模块边界、模板化拓扑** 更易；遗留、高债务代码库往往**最需要却最难**建 Harness。

### 5.4 人的角色

Harness 试图外显化资深工程师的隐性知识，但**不能**完全替代：组织语境、商业权衡、审美与责任归属。目标不是消除人，而是把人的注意力引向**最高杠杆**处。

---

## 六、Mitchell Hashimoto：狭义 Harness Engineering

六步采纳路径中，**Step 5: Engineer the Harness** 是狭义用法：

> 每当 Agent 做了一件坏事，就工程化一项对策，使其**不再**做这件事。

两类手段：

1. **隐式 Prompt 治理** — 更新 `AGENTS.md`（例：[Ghostty `AGENTS.md`](https://github.com/ghostty-org/ghostty/blob/main/AGENTS.md) 中许多行对应一次坏行为）；  
2. **可编程工具** — 截图脚本、过滤测试、包装命令等，并在 `AGENTS.md` 中声明。

这与 OpenAI 的「golden principles + 机械校验」、Fowler 的「Steering loop」同族，但 Hashimoto 强调**个人/小团队可每日执行**的微观闭环。

---

## 七、常见误解（校对表）

| 误解 | 更准确的说法 |
|------|----------------|
| Harness = 只写更好的 Prompt | Harness 含环境、工具、测试、编排、治理；Prompt 是子集 |
| Context Engineering 已被 Harness 取代 | Context 是 Harness 内核心子问题 |
| 模型越强 Harness 越无用 | 脚手架会瘦身，但组合会**迁移**；确定性门禁与合规仍必要 |
| OpenAI 百万行人人可复制 | 依赖极端约束、基础设施与团队自述 KPI |
| Evaluator 可有可无 | 任务超出 Generator 可靠区时，独立 Evaluator 仍有显著收益 |
| 测试全绿 = 产品正确 | Fowler / Anthropic 均指出行为正确性仍最难 |

---

## 八、争议与定位

### 8.1 「新瓶装旧酒」

零件（CI、Linter、评审、拆解）早已存在。新处在于：**命名、边界、Agent 可读仓库、Agent 间评审回路**。

### 8.2 「终局还是过渡」

| 命题 | 判断 |
|------|------|
| 全新科学理论？ | **否** |
| 纯营销？ | **否**；有可拆解实践与公开案例 |
| 值得投入？ | **是**（Coding Agent、长时自治、高可靠自动化） |
| 形态固定？ | **否**；随模型与 co-training 演化 |

LangChain 与 OpenAI 均指出：**模型与 Harness 协同训练**（post-training in the loop）会带来耦合——换 Harness 实现可能影响表现；反之，**针对任务优化 Harness** 仍有巨大空间（如 Terminal Bench 上换 Harness 显著提升排名）。

---

## 九、实践自检清单（团队用）

1. **Steer / Execute 是否分清？** 人是否主要在定意图、验收与 Harness 迭代？  
2. **SSOT 是否在仓库？** Agent 能否不靠聊天历史获得架构与规范？  
3. **`AGENTS.md` 是地图还是百科全书？** 是否 <200 行且指向 `docs/`？  
4. **有哪些计算型 Sensor？** 测试、Linter、类型、结构规则是否 PR 必经？  
5. **UI / 运行时是否可验证？** 浏览器、日志、指标是否对 Agent 开放？  
6. **长时任务如何跨窗口？** compaction、reset、Ralph Loop、plan 文件是否明确？  
7. **生成与评估是否分离？** 关键路径是否有独立 Evaluator 或等价门禁？  
8. **是否在积累 Golden Principles？** 重复错误是否回流为规则/工具？  
9. **是否做 load-bearing 分析？** 模型升级后是否拆掉不再承重的脚手架？  
10. **成本与质量是否量化？** Solo vs Full Harness 的 $/小时/缺陷是否可接受？

---

## 十、时间线（可核对）

| 日期 | 事件 |
|------|------|
| 2025-08（下旬） | OpenAI：空仓库首 commit，实验开始 |
| 2026-02-05 | Hashimoto：*My AI Adoption Journey* |
| 2026-02-11 | OpenAI：Harness Engineering 长文 |
| 2026-02-17 | Martin Fowler：Harness 备忘录 |
| 2026-03-10 前后 | LangChain：*The Anatomy of an Agent Harness* |
| 2026-03-24 | Anthropic：长时应用 Harness 设计 |
| 2026-04-02 | Martin Fowler：Harness 长文定稿 |

---

## 十一、参考文献

### 核心

1. Mitchell Hashimoto — [My AI Adoption Journey](https://mitchellh.com/writing/my-ai-adoption-journey)（2026-02-05）  
2. OpenAI — [Harness engineering: leveraging Codex in an agent-first world](https://openai.com/index/harness-engineering/)（2026-02-11）  
3. Martin Fowler — [Harness engineering for coding agent users](https://martinfowler.com/articles/harness-engineering.html)（2026-04-02）  
4. LangChain — [The Anatomy of an Agent Harness](https://blog.langchain.com/the-anatomy-of-an-agent-harness/)（2026-03）  
5. Anthropic — [Harness design for long-running application development](https://www.anthropic.com/engineering/harness-design-long-running-apps)（2026-03-24）  

### 延伸

6. Anthropic — [Effective harnesses for long-running agents](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents)  
7. Anthropic — [Effective context engineering for AI agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)  
8. Anthropic — [Building effective agents](https://www.anthropic.com/research/building-effective-agents)（「从简单方案开始」原则）  
9. Martin Fowler — [Harness engineering memo](https://martinfowler.com/articles/exploring-gen-ai/harness-engineering-memo.html)（2026-02-17 初稿）  
10. Geoffrey Huntley — [The Ralph Wiggum Loop](https://ghuntley.com/loop/)（长时续跑模式）  
11. OpenAI — [Codex execution plans](https://cookbook.openai.com/articles/codex_exec_plans)（`exec-plans` 实践）  

---

## 结语

**Harness Engineering（广义）** 是在非确定性模型之上，构建**环境、约束、工具与反馈**的学科；**狭义**则强调把每次失败沉淀为规则与可执行检查。

- **Prompt** → Harness 内的表达层；  
- **Context** → Harness 内的资源调度层；  
- **软件工程** → 确定性门禁长期有效，执行者扩展为「人 + Agent」。

不必迷信百万行叙事，但应能回答：

> 我们的 Harness 里有哪些 Guides 与 Sensors？谁在 steer，谁在 execute？哪些脚手架仍承重，哪些该拆掉？

模型会变强，Harness 会变样；**组合会迁移，而非整体消失**。

---

> **范式整合**：Harness Engineering 是"认知与决策驱动设计"范式**治理层的工程实践子层**。  
> 本综述中的 Harness 组件在范式中有精确的架构位置：  
> - Orchestrator / State Machine → `decision/adjudication.rs`  
> - Tool Registry / Sandbox → `decision/firewall.rs` + `tools/registry.rs`  
> - Context / Memory → `cognition/` (WorkingMemory → EpisodeMemory → MemoryManager)  
> - Observability → `AgentEvent` 30 variants + `EventDetailLevel` + `SessionStore`  
> - Evolution → 当前空缺（`GuardrailConfig` 为静态）  
> 范式定义：`docs/ai-agent-archi/cognition-decision-driven-design.md` §4  
> 术语表：`docs/others/HARNESS_ENGINEERING_GLOSSARY.md`

---

*文档类型：行业概念综述（非 uncode 实现规范）。路径：`docs/technologies/HARNESS_ENGINEERING.md`。最后更新：2026-05。*
