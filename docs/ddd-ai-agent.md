# 当确定性遇到概率性：DDD在AI Agent系统中的适应与重生

## 引言：一个引发争议的问题

“DDD并不适合用于AI Agent应用的开发”——这个观点在软件架构圈中正引发越来越多的讨论。作为一个在确定性业务领域（如电商、银行、ERP）中被证明行之有效的领域建模方法，领域驱动设计在面对以LLM为核心的AI Agent系统时，确实显得力不从心。

然而，简单的“适合/不适合”二分法可能掩盖了一个更复杂的事实。通过对GitHub上52.5k Stars的开源项目`pi`（earendil-works/pi）的深入分析，以及重新审视DDD的核心思想，我们发现：**需要变革的不是DDD本身，而是我们对DDD的理解方式**。

本文将从冲突分析、适应性调整、以及`pi`项目的实践验证三个维度，系统探讨DDD在AI Agent时代的“重生”之路。

---

## 第一部分：冲突——为什么经典DDD会“水土不服”？

### 1.1 确定性vs.概率性的根本分歧

经典DDD建立在一个基本假设之上：**给定相同的输入，领域模型产生确定性的输出**。无论是聚合根的状态转换、领域服务的计算、还是规约模式的判断，其结果都应该是可预测和可重复的。

AI Agent的核心决策引擎——大语言模型——恰恰是**概率性**的。相同的提示词、相同的上下文，LLM可能产生不同的计划和代码。这种不确定性让传统的领域模型设计失效：

```typescript
// 传统DDD中，领域服务的结果是确定的
class PricingService {
  calculatePrice(Order order): Price {
    // 同样的order，永远计算出同样的price
    return order.items.sum() * (1 - order.discount);
  }
}

// 在AI Agent中，计划生成服务的结果是不确定的
class PlanGenerationService {
  async generatePlan(Task task): Promise<Plan> {
    // 同样的task，每次可能生成不同的Plan
    // 可能选择不同的工具、不同的步骤顺序、甚至不同的实现方案
    return await llm.invoke(buildPrompt(task));
  }
}
```

这种不确定性直接冲击了DDD的核心价值——通过显式的领域模型捕捉业务规则，使系统行为可理解和可预测。

#### 不确定性的三层解构

将"概率性"当作一个整体来讨论是不够精确的。AI Agent系统中的不确定性实际上可以拆解为三种不同性质：

| 不确定性类型 | 来源 | 表现 | 适配策略 |
|:---|:---|:---|:---|
| **生成不确定性** | LLM采样机制 | 同一输入产生不同候选（计划、代码、文本） | 约束+验证（Schema、规则、类型系统） |
| **认知不完全性** | 上下文不足、信息缺失 | 决策偏差、"幻觉"、遗漏关键约束 | 记忆与检索建模——这本身就是一类新的"领域对象" |
| **执行不确定性** | 外部系统、工具调用 | API失败、文件冲突、环境变化 | 补偿事务/事件溯源（Saga、重试、回滚） |

这三类不确定性在领域模型中不应该被混为一个笼统的"失败"概念。举个例子：同样是"Agent 生成代码失败"——

- 如果是**生成不确定性**：多样本生成 + rerank，选最优输出
- 如果是**认知不完全性**：补充缺失的上下文（读取更多文件、查阅文档）后再生成
- 如果是**执行不确定性**：retry 或回滚工作区

```typescript
// 不确定性分层的领域建模
type AgentFailure =
  | { kind: 'generation'; candidates: string[]; threshold: number }
  | { kind: 'cognition'; missingContext: string[]; suggestion: string }
  | { kind: 'execution'; error: Error; retryCount: number; maxRetries: number };

class FailureHandler {
  handle(failure: AgentFailure): NextAction {
    switch (failure.kind) {
      case 'generation':
        return new RerankAndSelect(failure.candidates);
      case 'cognition':
        return new EnrichContext(failure.missingContext);
      case 'execution':
        return new RetryOrCompensate(failure.error, failure.retryCount);
    }
  }
}
```

对 DDD 的启示：**DDD 的适配策略应对这三类不确定性分别设计**——不是用一个笼统的"概率性"标签抹平所有差异，而是在领域模型内部为每一类不确定性提供专属的处理路径。

### 1.2 短事务vs.长流程的事务边界冲突

DDD中的聚合根设计强依赖于**事务边界**。一个聚合内部保证强一致性，事务通常只在毫秒到秒级完成。聚合之间的最终一致性通过领域事件实现。

AI Agent的一个典型任务（如“为用户登录功能添加双因素认证”）可能涉及：
- 5-10次LLM调用（理解需求、生成计划、代码生成、自我审查、修复错误）
- 多次工具执行（文件读取、代码搜索、测试运行、Git操作）
- 可能的人工反馈循环（等待用户确认或审查意见）

整个过程可能持续**数分钟甚至更久**。传统的聚合事务边界完全无法覆盖这样的生命周期。如果坚持用单个聚合管理`Task`的所有状态，要么导致长时间锁定资源，要么被迫放弃DDD的一致性保证。

### 1.3 显式规则vs.涌现行为的表达力鸿沟

Eric Evans在《领域驱动设计》中强调：**领域逻辑应该被显式地建模为代码**。规约模式、策略模式、值对象的不变性检查——这些都是将隐性知识转化为显式构造的手段。

AI Agent中的复杂行为（如自我纠错、工具选择策略、从失败中学习）并非由程序员显式编写，而是从**提示词、上下文示例和模型权重中“涌现”** 出来的。一个Agent在观察到测试失败后决定回滚到前一个版本并尝试不同的实现方案，这个决策过程并不存在于代码的if-else分支中，而是LLM在推理时动态生成的。

这导致了领域层表达力的根本性削弱——大量业务逻辑消失在黑箱中，无法被审查、测试或版本控制。

### 1.4 无状态服务vs.深度状态依赖

DDD中的领域服务被设计为**无状态**的。它们接收输入，执行计算，返回结果，不保留任何跨调用的状态。这种设计支持了服务的高内聚、低耦合和易于测试。

AI Agent的服务调用则呈现出**深度的状态依赖性**：
- Agent下一步行动严重依赖完整的会话历史
- 工具的输出影响后续决策
- 代码库的当前状态（文件树、未提交变更）是决策的关键上下文

```typescript
// 传统DDD的无状态服务
class CodeReviewService {
  review(changeSet: CodeChangeSet): ReviewResult {
    // 只基于当前输入的changeSet做判断
    return this.checkRules(changeSet);
  }
}

// AI Agent中有状态的服务
class AgentDecisionService {
  async decideNextAction(
    currentState: AgentState,      // 显式状态
    memory: ConversationMemory,     // 历史记忆
    workspace: WorkspaceSnapshot    // 环境状态
  ): Promise<Action> {
    // 决策依赖大量上下文
    return await llm.invoke({
      state: currentState,
      history: memory.getRecent(),
      workspace: workspace.getDiff()
    });
  }
}
```

有状态的服务打破了DDD服务层的纯粹性，迫使我们在架构中引入显式的状态管理机制。

---

## 第二部分：融合——适应性DDD的实践形态

尽管存在上述冲突，**放弃DDD的战略价值同样是代价高昂的**。限界上下文、上下文映射、通用语言这些战略模式，在处理AI Agent系统的复杂性时反而变得更加重要。

### 2.1 限界上下文作为“概率-确定性”隔离墙

在`pi`项目中，核心设计决策就是通过npm包边界严格分离了不同性质的子系统：

```
@earendil-works/pi-ai          # 概率性区域：LLM调用、提示词管理
@earendil-works/pi-agent-core  # 确定性区域：状态机、工具调度
@earendil-works/pi-coding-agent # 混合区域：用户交互、流程编排
```

**关键调整**：确定性区域（`pi-agent-core`）不直接调用LLM。相反，它通过一个明确的**防腐层**与概率性区域交互。LLM的输出被严格验证，并解析为确定性领域层可以理解的命令或事件。

这种隔离使得核心业务逻辑（Agent状态转换、工具执行顺序、回滚策略）仍然可以用传统DDD的方式建模，而不被LLM的非确定性污染。

#### 防腐层升级：从"数据转换"到"语义防火墙"

在 AI Agent 场景下，防腐层（ACL）的功能被显著放大——它不再仅仅是 DTO 转换，而是承担了**语义压缩 + 风险过滤 + 结构化**三重职责。建议将其设计为三层架构：

| 层 | 职责 | 输入 | 输出 |
|:---|:---|:---|:---|
| **Parsing 层** | 将 LLM 的原始自然语言输出转为结构化数据 | 自由文本 | JSON / AST / 结构化指令 |
| **Validation 层** | Schema + Rule + Policy 校验 | 结构化数据 | 通过/拒绝 + 违规列表 |
| **Normalization 层** | 消歧义、标准化引用（路径、依赖、命名） | 经验证的结构化数据 | 确定性领域命令 |

一个具体例子：

```json
// LLM 原始输出（自然语言，不可靠）
"Let's modify auth.ts to add 2FA"

// Parsing 层输出（结构化）
{
  "action": "modify_file",
  "target": "src/auth.ts",
  "intent": "add_2fa",
  "confidence": 0.82
}

// Validation 层输出（通过策略检查）
{
  "approved": true,
  "normalized_target": "src/auth/mod.ts",
  "guardrails": ["no_schema_change", "existing_file_only"]
}

// Normalization 层输出（确定性命令）
ModifyFileCommand {
  file: "src/auth/mod.ts",
  operation: "append",
  scope: "two_factor_auth"
}
```

**核心原则：领域层永远不接触自然语言——只接触"压缩后的语义"。**

这从根本上解决了 LLM 输出不可靠的问题：概率性停留在防腐层之外，领域模型内部收到的始终是经过验证的确定性指令。


```typescript
// 确定性核心域中的聚合根
class Agent {
  private state: AgentState;  // 'idle' | 'working' | 'waiting'
  private currentTask: Task | null;
  
  assignTask(task: Task): void {
    // 确定性的状态转换逻辑
    if (this.state !== 'idle') {
      throw new AgentBusyError();
    }
    this.state = 'working';
    this.currentTask = task;
    this.addDomainEvent(new TaskAssignedToAgent(task.id, this.id));
  }
  
  // 处理来自LLM的不确定性响应
  handlePlanProposal(proposedPlan: ProposedPlan): Plan {
    // 验证、过滤、转换
    const validated = this.validatePlan(proposedPlan);
    this.addDomainEvent(new PlanProposed(validated));
    return validated;
  }
}
```

### 2.2 聚合重新定义：从“事务边界”到“快照验证”

传统DDD中，聚合是事务一致性的边界。在AI Agent的长流程场景下，我们转向一种**混合模型**：

1. **核心聚合保持精简**：`Agent`聚合根只包含身份、状态机、当前版本号。任何`Task`执行的生命周期不由单个聚合管理。

2. **事件流成为“真相源”**：每次LLM决策、工具执行、人工反馈都记录为不可变的**领域事件**。Agent的任务执行不是通过修改聚合状态，而是通过追加事件来实现。

3. **快照提供读模型**：从事件流中定期生成`AgentSnapshot`和`TaskSnapshot`，用于查询和显示，但不作为写操作的一致性边界。

```typescript
// 事件溯源风格的事件记录
interface DomainEvent {
  aggregateId: string;
  timestamp: Date;
  sequence: number;
}

class PlanProposed implements DomainEvent {
  type = 'PlanProposed';
  constructor(
    public aggregateId: string,
    public sequence: number,
    public plan: Plan,
    public rationale: string  // LLM的决策理由
  ) {}
}

class ToolExecuted implements DomainEvent {
  type = 'ToolExecuted';
  constructor(
    public aggregateId: string,
    public sequence: number,
    public tool: string,
    public input: any,
    public output: any,
    public duration: number
  ) {}
}
```

**一致性策略**：通过**最终一致性和补偿动作**来保证系统正确性。当Agent的后续步骤发现之前的决策错误时（例如生成的代码导致测试失败），它会追加`PlanFailed`和`PlanRevised`事件来修正路径，而不是回滚已提交的事件。

#### 聚合的真正变化：从"一致性边界"到"决策边界"

上述"快照验证"模式是正确的，但可以再往前走一步。在 AI Agent 系统里，聚合的本质角色发生了微妙但关键的变化：

> **传统 DDD：聚合保证"数据一致"**
> **Agent 系统：聚合保证"决策合法"**

换言之，聚合不再是事务单元，而是**"决策授权单元"**。LLM 负责"想"，聚合负责"裁决"：

```typescript
class Agent {
  decide(action: ProposedAction): ApprovedAction {
    // 不是执行，而是"裁决"
    if (!this.policy.allows(action)) {
      throw new PolicyViolation(action, this.policy.getViolations(action));
    }
    return this.guardrails.normalize(action);
  }
}
```

这里引入了一个新的领域概念——**Guardrails（护栏）**——它本质上就是 DDD 中规约模式（Specification）和策略模式（Strategy）在 Agent 场景下的自然延伸。区别在于：传统规约判断"数据是否合法"，Agent 护栏判断"行为是否合法"。

这实际上把 DDD 从"业务规则引擎"升级成了"行为约束系统"。


### 2.3 领域事件作为一等公民

在传统DDD中，领域事件主要服务于**通知**——其他限界上下文得知某件事已经发生，以便做出响应。

在AI Agent系统中，**领域事件链本身就是核心产品**。`pi`项目特别强调“分享公开会话记录”，正是将Agent的完整决策轨迹视为可复用、可分析的核心资产。

事件链支持三大关键能力：

1. **可观测性**：理解Agent为何做出某次代码更改。通过回放事件，我们可以重现Agent的“思考过程”。

2. **离线优化**：用失败的事件流微调提示词或模型权重。识别出Agent在哪些决策点上频繁出错，针对性地改进。

3. **指标计算**：从事件流中计算业务指标——首次通过率、平均工具调用次数、常见失败模式、任务完成时间分布。

```typescript
// 从事件流中计算指标
class AgentMetricsCalculator {
  calculateFromEvents(events: DomainEvent[]): Metrics {
    const toolCalls = events.filter(e => e.type === 'ToolExecuted');
    const failures = events.filter(e => e.type === 'PlanFailed');
    const revisions = events.filter(e => e.type === 'PlanRevised');
    
    return {
      totalToolCalls: toolCalls.length,
      failureRate: failures.length / events.length,
      avgRevisionsPerTask: revisions.length / this.getUniqueTasks(events).length
    };
  }
}
```

#### 事件溯源的关键升级：从"记录"到"训练数据"

你已经提到事件链是资产，这一点可以再强化为一条核心论断：

> **事件流 = 在线系统 + 离线训练数据的统一接口**

这带来一个设计上的重要变化——事件的结构不仅要"对业务友好"，还要"对训练友好"。一个更 AI-native 的事件模型应明确区分：

| 事件维度 | 含义 | 用途 |
|:---|:---|:---|
| **action** | Agent 执行了什么操作（工具调用、计划选择） | 在线：观测与调试；离线：行为策略学习 |
| **observation** | 操作后环境发生了什么变化（文件变更、测试结果） | 在线：下一步决策依据；离线：环境建模 |
| **reward / feedback** | 人类或自动化评价（通过/失败、👍/👎） | 在线：即时纠正；离线：偏好对齐 |

对应的领域事件建模：

```typescript
class AgentStep {
  state: State;           // 决策前的状态快照
  action: Action;         // Agent 采取的行动
  observation: Observation; // 行动后的观察结果
  feedback?: Feedback;    // 人类或自动化评价信号
}
```

这实际上已经在向强化学习（RL）中的 trajectory 建模靠拢——领域事件不仅是"发生了什么"，更是"为什么这样做"和"结果好不好"的完整记录。当事件流同时服务于在线推理和离线训练时，它就成为了连接"实时系统"与"持续改进"的真正枢纽。

#### 一个必须警惕的风险：过度事件化

事件溯源非常契合 Agent 系统，但有一个现实问题需要强调：

> **不是所有东西都值得成为事件。**

如果不加节制，很容易陷入：
- **事件爆炸**：token 级别的记录（每次 LLM 采样都记为一个事件），存储成本失控
- **信噪比下降**：大量无价值的中间步骤稀释了关键决策轨迹
- **分析退化**：当事件流 90% 是低价值步骤时，调试和训练都变得低效

实践上需要一层明确的**事件采样策略**：

| 事件类型 | 策略 | 说明 |
|:---|:---|:---|
| **关键决策**（计划选择、策略切换、人工反馈） | 必须完整记录 | 不可丢失 |
| **中间推理**（LLM thought process、工具调用参数调整） | 按比例采样 | 如每 N 条保留 1 条 |
| **低价值步骤**（冗余重试、噪音输出、确定性验证） | 丢弃或聚合压缩 | 仅保留统计摘要 |

这个采样策略本身就是领域知识——什么样的决策值得被记住、什么样的步骤可以遗忘，正是领域专家与系统架构师需要共同回答的问题。


### 2.4 有状态的领域服务：引入“会话上下文”

承认AI Agent服务的状态依赖性，我们不回避它，而是**显式建模**。服务接口将所需的状态作为参数接受，而不是从私有字段读取：

```typescript
class AgentOrchestrationService {
  // 不保留内部状态，但接受完整的状态参数
  async determineNextAction(
    agentState: AgentState,      // 显式传入
    eventHistory: DomainEvent[],  // 历史事件
    workspaceDelta: WorkspaceDiff, // 环境变化
    toolRegistry: ToolRegistry     // 可用工具
  ): Promise<Action> {
    const context = {
      state: agentState,
      recentEvents: eventHistory.slice(-20), // 最近20个事件
      workspace: workspaceDelta,
      availableTools: toolRegistry.list()
    };
    
    const llmResponse = await this.llmService.invoke(
      this.buildDecisionPrompt(context)
    );
    
    return this.parseAction(llmResponse);
  }
}
```

这种设计保持了服务的**可测试性**（因为所有依赖都通过参数注入）和**无副作用**（服务不修改传入的状态），同时满足了AI Agent对上下文深度的需求。

#### 视角转换：服务依然无状态，状态外化为一等输入

关于服务"变有状态"的讨论，其实可以用一个更贴近 DDD 的方式重新表述：

> **服务仍然是无状态的，只是状态被显式外化为一等输入。**

这个表述很关键，因为它保住了三个重要的架构属性：

- **可测试性**：任何 `determineNextAction` 的调用都可以通过构造特定的 `AgentState` + `DomainEvent[]` 来独立验证
- **可替换性**：服务的实现可以自由切换（不同的 LLM、不同的决策策略），只要接口保持 `(State, History, Workspace) → Action` 的函数签名
- **可组合性**：多个服务可以串联为流水线，状态在它们之间以不可变的方式传递

换句话说：**不是 DDD 被打破了，而是我们终于被迫正确使用它了。**

经典 DDD 本来就强调"服务应该是无状态的"——只是在过去的实践中，我们常常偷偷地把状态藏在服务的私有字段或全局变量里。AI Agent 对状态的强烈依赖迫使我们把状态提升为一等公民，通过参数显式传递。这不是对 DDD 的背离，恰恰是对 DDD 原则的更深层实践。


---

## 第三部分：验证——从`pi`项目看适应性DDD的落地

### 3.1 `pi`的架构映射

通过代码结构和文档分析，我们可以将`pi`的模块映射到适应性DDD的模式：

| DDD概念 | `pi`中的实现 | 适应性调整 |
|:---|:---|:---|
| **限界上下文** | `@earendil-works/pi-ai`<br>`@earendil-works/pi-agent-core`<br>`@earendil-works/pi-coding-agent` | 通过npm包强制执行边界，AI上下文与核心上下文隔离 |
| **聚合根** | `Agent`（在`pi-agent-core`中） | 只包含状态机和版本号，不包含长任务细节 |
| **领域事件** | 会话记录（可导出到Hugging Face） | 事件链作为主要产品，支持回放、分析和优化 |
| **领域服务** | `LLMService`（统一多供应商API）<br>`AgentRuntime` | 服务接受显式的状态参数，管理概率性交互 |
| **防腐层** | `pi-ai`与`pi-agent-core`之间的接口 | LLM输出被严格验证和解析，转换为确定性命令 |
| **仓储** | `CodeRepository`（Git操作封装） | 抽象基础设施细节 |

### 3.2 `pi`验证的设计原则

从`pi`的成功（52.5k Stars，活跃维护）可以提炼出AI Agent系统设计的几条原则：

1. **隔离概率性**：将LLM调用封装在独立的限界上下文中，核心领域逻辑保持确定性。

2. **事件为纲**：以领域事件链作为主要的持久化和分析单元，而非传统的聚合状态。

3. **接受不确定性**：不为非确定性行为强行建模为确定性规则；相反，用验证层和补偿动作管理不确定性。

4. **保持可观测**：记录完整的决策轨迹，这是调试、优化和建立信任的基础。

5. **模块化作为战略**：用包的边界对应限界上下文，使不同的治理模式可以共存。

### 3.3 仍然存在的问题

适应性DDD并非银弹。在AI Agent系统中，我们仍然面临未解决的挑战：

- **提示词即代码**：LLM提示词包含了大量的领域逻辑，但它们存在于字符串中，无法享受类型检查、版本控制和重构工具的支持。
- **测试的困境**：概率性输出使得传统单元测试失效。我们转向“评估”（evaluation）而非“测试”，但评估的工程化程度远低于测试。
- **可解释性的成本**：完整的事件链提供了可观测性，但存储和分析的代价高昂（`pi`一个项目可能产生数千个会话）。

---


### 3.4 效果对比：同一需求，三种方案

为了让"进化而不是抛弃DDD"的论点更直观，我们用同一个具体需求来做对比——**"给登录系统加 2FA"**——分别看三种建模方式的差异：

| 对比维度 | 传统 DDD | 天真 Agent（无 DDD） | 适应性 DDD |
|:---|:---|:---|:---|
| **可控性** | 高。所有业务规则显式建模为领域对象。 | 低。Agent 自主决策，行为不可预测。 | 高。核心约束由聚合和护栏保证，LLM 在边界内自由发挥。 |
| **可观测性** | 高。状态转换通过领域事件追溯，路径确定。 | 极低。Agent 内部推理过程不可见，只能看最终结果。 | 极高。完整事件链记录每一步决策+理由+结果，支持回放分析。 |
| **可演进性** | 中。修改规则需要改代码+测试+部署。 | 低。"改提示词看效果"式调参，缺乏系统性。 | 高。约束层可独立迭代，优化层（提示词/模型）可独立升级，事件流提供数据驱动改进。 |
| **开发效率** | 低。需要前置建模、编写聚合、定义事件。 | 高（初期）。几分钟写出一个能跑的 Agent。 | 中（初期），高（长期）。前期投入建模成本，但随着事件流积累，调试和优化效率远超其他方案。 |
| **边界保护** | 强。限界上下文、聚合、防腐层构成多层防护。 | 弱。Agent 可能修改任意文件、执行任意命令。 | 强。防腐层升级为语义防火墙，聚合升级为决策授权单元。 |
| **适用场景** | 确定性业务（支付、库存、合规） | 探索性原型、个人工具 | 生产级 Agent 系统、需要审计和持续优化的 AI 应用 |

**结论**：传统 DDD 适合确定性业务，天真 Agent 适合快速原型，而适应性 DDD 适合需要**可控+可观测+可演进**的生产级 Agent 系统。它们不是互相替代，而是各自服务不同的设计目标。

## 第四部分：展望——从“领域驱动设计”到“认知驱动设计”

### 4.1 新的设计范式

基于以上分析，我建议将AI Agent系统的设计称为 **认知驱动设计（Cognitive-Driven Design, CDD）** ，它是DDD在概率性、认知性系统中的自然演化。

关于"CDD"这个命名的两点提醒：

首先，**"认知驱动设计"作为一个概念，可能过于宽泛**——"认知"几乎涵盖了一切 AI 系统的范畴，容易与已有的 Cognitive Architecture 术语混淆。一个更工程化的表述可能是 **Probabilistic Domain Design** 或 **Agent-Oriented DDD**。不过，如果面向文章传播，"CDD"仍然是成立的，关键在于给出可操作的定义边界：

- 它不是要取代 DDD，而是 DDD 在概率性系统中的自然延伸
- 它的核心操作不是"认知建模"，而是"为不确定性留出结构化空间"
- 它的方法论基础仍然是限界上下文、上下文映射、通用语言——只是每个概念被赋予了新的内涵

其次，CDD的核心要素包括：

1. **认知边界**（Cognitive Bounded Context）：不仅隔离业务逻辑，还隔离“认知模式”——确定性计算、概率性推理、涌现行为各自独立。

2. **事件认知溯源**（Event Cognitive Sourcing）：事件链不仅是状态变更记录，更是Agent决策认知过程的完整捕获。

3. **可验证而非可预测**（Verifiable over Predictable）：我们不要求系统行为可预测，但要求关键属性（安全性、资源限制、行为约束）可验证。

4. **人机协同的领域语言**：领域模型同时为人类和Agent提供共同语言。`AGENTS.md`文件（`pi`项目中存在）正是这种二元受众的产物——既指导人类贡献者，也指导Agent行为。

### 4.2 实用建议：如何开始

如果你正在构建AI Agent系统，并希望借鉴DDD的思想，以下是一个务实的起点：

1. **画限界上下文地图**：识别系统中的确定性区域（如工具执行、状态机）和概率性区域（LLM决策、计划生成）。为前者应用经典DDD，为后者接受事件溯源和评估驱动开发。

2. **设计事件为第一接口**：无论Agent内部如何实现，对外的关键接口是“领域事件流”。这为观测、调试和优化留下空间。

3. **建立验证层**：在概率性区域和确定性区域之间建立防腐层。LLM的所有输出经过验证、过滤和转换，才被允许进入核心域。

4. **用快照处理长流程**：不为长任务维护单一的聚合状态。采用事件流+定期快照的模式，查询使用快照，写操作追加事件。

5. **投资可观测性**：设计时就考虑事件导出（`pi`导出到Hugging Face就是好例子）、性能指标、失败模式分析。

---

## 结论：DDD没有死，它在进化

“DDD并不适合AI Agent开发”——这个结论对2004年版的经典DDD是正确的。但正如软件架构本身在演化，DDD作为一种思想体系也在演化。

**战略DDD**（限界上下文、上下文映射、通用语言）在AI Agent系统中不仅适用，而且变得更加重要。**战术DDD**（聚合、实体、值对象）需要调整——缩小聚合边界，引入事件溯源作为一等公民，接受有状态的服务。

`pi`项目的成功证明：**融合了DDD战略思想和事件溯源实践的适应性设计，能够有效管理AI Agent系统的复杂性**。

或许，我们需要的不是放弃DDD，而是将DDD的核心洞察——**以领域为中心，通过边界管理复杂性**——带入新的时代。这个时代的领域，不再是确定性的业务规则，而是人、代码与概率性认知系统共同演化的复杂生态。

DDD没有死，它在进化。而我们，正处在见证这种进化的最佳时刻。

---

## 参考文献与延伸阅读

1. Eric Evans. *Domain-Driven Design: Tackling Complexity in the Heart of Software*. 2004.
2. Martin Fowler. *Event Sourcing*. martinfowler.com, 2005.
3. `earendil-works/pi` GitHub Repository: https://github.com/earendil-works/pi
4. OpenAI. *Building agents with the Assistants API*. 2023.
5. 本文分析的DDD-AI Agent冲突矩阵（作者整理，可在线获取）。