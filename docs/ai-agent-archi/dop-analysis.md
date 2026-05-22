# Decision-Oriented Programming (DOP) 论文分析与启示

> 论文：*Decision-Oriented Programming with Aporia* (arXiv:2604.05203, 2026-04-06)
> 作者：Kasibatla, Rothkopf, Peleg, Pierce, Lerner, Goldstein, Polikarpova
> 分析目标：评估 DOP 与"认知与决策驱动设计"范式的关系，提取可整合的洞察

---

## 一、论文核心观点

### 1.1 问题定位

论文直面 AI Agent 编程的核心矛盾：

**"AI agents 降低了认知负担，但代价是开发者在不自知的情况下让渡了决策权。"**

> "The default interaction model of today's coding agents—prompt, generate, review—makes it easy to let the agent drive."

这与我们的范式 §1 对"确定性 vs 概率性"冲突的分析完全同源：LLM 自主填充设计细节，开发者被动审查。

### 1.2 DOP 的三项设计目标

| 目标 | 含义 | Aporia 实现 |
|:---|:---|:---|
| **DG1**：决策显式化、结构化 | 决策是持久可编辑的第一公民 | Decision Bank（UI 中的结构化记录） |
| **DG2**：决策由人机协同创作 | Agent 主动提问 > 程序员被动审查 | Question Bubbles + Goal Field |
| **DG3**：决策可追溯到代码 | 每个决策编码为可执行测试 | 每个 decision 生成 test suite |

### 1.3 实验结论

**N=14 程序员，within-subjects 对比实验**（Aporia vs Claude Code 基线）

| 指标 | 结果 |
|:---|:---|
| 决策发现量 | Aporia 用户显著更多（p<0.01） |
| 心智模型准确度 | 79% 更低的不匹配率（**5x** 改善） |
| 认知负荷 | 无显著差异（说明不需要额外 mental effort） |
| 用户偏好 | Decision Bank 评分最高 (M=4.4/5) |

**关键引述**：

> "Participants had a significantly more thorough understanding of their solutions with Aporia."

> "Aporia helped participants discover design considerations they had not thought of on their own." (9/14 participants)

> "Using Aporia, participants were 5x less likely to have their mental model disagree with the actual implementation."

---

## 二、与"认知与决策驱动设计"的对应关系

### 2.1 结构对比

| 维度 | 认知与决策驱动设计 | DOP (Aporia) | 关系 |
|:---|:---|:---|:---|
| **"决策"的地位** | 决策 = 第一公民（决策层核心） | 决策 = 第一公民（Decision Bank 核心） | **完全一致** |
| **决策的记录** | `DecisionRecord` + `AuditTrail`（系统内部） | Decision Bank（用户可见 UI） | **互补**：一个对内一个对外 |
| **决策的验证** | `Adjudicator` + `DecisionPolicy`（系统裁决） | Test suite per decision（可执行验证） | **互补**：裁决是"能做吗"，测试是"做对了吗" |
| **决策的发现** | `ProposalAccumulator` 从 LLM stream 提取 | Question Bubbles **主动提问**程序员 | **互补**：一个提取LLM输出，一个向人类提问 |
| **三层代理** | 认知层(L L M) → 防火墙 → 决策层(系统) | questioner → planner → implementer | **不同分工**：我们是认知vs决策分离，他们是提问vs计划vs实现 |

### 2.2 关键差异

| Aporia 有但我们没有的 | 我们有的但 Aporia 没有的 |
|:---|:---|
| **用户可见的决策界面**（Decision Bank UI） | **系统内部的裁决链**（Proposal → Firewall → Adjudicator） |
| **主动提问机制**（agent向人类提问） | **自动评估**（H0-H3 EvaluationScore） |
| **每个决策生成测试**（test suite per decision） | **自适应进化**（EvolutionEngine pattern detection） |
| **人机协同创作**（交互式设计探索） | **语义防火墙**（自然语言→结构化命令的硬转换） |
| **实证用户研究**（N=14, 5x 理解改善） | **工程架构完整度**（5模块 + 397 tests） |

### 2.3 深层共识

两套范式在三个根本点上一致：

1. **决策应该是第一公民**。Aporia 的 Decision Bank 和我们的 DecisionRecord 是同一个概念的不同表现形式——前者面向程序员，后者面向系统。

2. **决策应该脱离自然语言**。Aporia 用 Test Suites 将决策形式化；我们用 `NormalizedAction` + `ApprovedAction` 将决策结构化。手段不同，原理相同。

3. **被动审查应被主动交互替代**。Aporia 的 Question Bubbles 替换了"prompt → plan → review"的被动模式；我们的 `ProposalAccumulator` + `Adjudicator` 替换了"LLM输出 → 直接执行"的无防护模式。

---

## 三、对"认知与决策驱动设计"的启示

### 3.1 可整合的洞察

| 论文洞察 | 如何融入我们的范式 |
|:---|:---|
| **DG2：Agent主动提问** | 认知层增加 `QuestionGenerator` 组件：在 turn 开始时向用户提问可能的决策点 |
| **DG3：每个决策 → 测试套件** | 审计层扩展：`DecisionRecord` 关联 `TestCase`，Evaluator 从"评估单次执行"升级为"评估决策实现" |
| **Decision Bank 作为用户界面** | 治理层增加 `user-facing` 子层：Decision Bank 作为开发者与 Agent 的共享视图 |
| **实证验证：5x 理解改善** | 可作为范式论文的实验支撑——DOP 的用户研究证明了"决策第一公民"对开发者的价值 |
| **Plan vs Decision 对比** | 论文明确区分 plan-oriented 与 decision-oriented——我们可借此定位：CDDD 是 decision-oriented + system-internal architecture |

### 3.2 范式公式可补充

当前公式：

> LLM 负责认知与生成，系统架构负责决策与治理

可补充为：

> **LLM 负责认知与生成，系统架构负责决策与治理，Decision Bank 负责人机共识。**

其中 Decision Bank = 开发者可见的决策视图（对应 DG1 + DG2）。

### 3.3 推荐行动

| 优先级 | 行动 | 说明 |
|:---:|:---|:---|
| **P0** | 范式文档引用 DOP 作为独立验证 | 将论文 §5 的实证结论作为"决策第一公民"的外部证据 |
| **P1** | 设计 `QuestionGenerator` 组件 | 对应 DG2：认知层主动向用户提问 |
| **P2** | `DecisionRecord` 关联 `TestCase` | 对应 DG3：每个决策产生可执行验证 |
| **P3** | 用户可见的 Decision Bank UI | DG1 的系统内向开发者可见的延伸 |

---

## 四、总结

DOP 与"认知与决策驱动设计"**内核一致，视角互补**：

- DOP 从 HCI 角度出发，关注**人类开发者**如何在 AI 辅助下保持决策权，用 Decision Bank + Questions + Tests 实现。
- 我们从软件架构角度出发，关注**系统内部**如何组织认知与决策的分层，用 Cognition Layer + Firewall + Decision Layer + Governance Layer 实现。

两套范式可以互相引用：DOP 提供了实证证据（5x 理解改善），我们的范式提供了工程实现路径（5模块 + 397 tests + Rust 编译验证）。二者的结合可以形成一个"从用户界面到系统内核"的完整决策驱动架构。
