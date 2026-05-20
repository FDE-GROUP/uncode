# uncode 对 Pi 的复刻：深度对比、对齐度与评价

> 分析对象：**uncode**（本仓库，Rust）与 **Pi**（[earendil-works/pi](https://github.com/earendil-works/pi)，TypeScript）  
> 前提：uncode 以 Rust 重写，目标是在**架构哲学**与**核心功能**上对齐 Pi，而非对齐其他 Rust Agent 发行版。

---

## 1. 定位与分层映射

Pi 将「可复用的 Agent 能力」与「终端 Harness」拆成三个 npm 包；uncode 用 **Cargo workspace** 做了同构拆分，依赖方向自上而下，与 Pi 的「库 → Agent → UI」一致。

| Pi（TypeScript） | 职责摘要 | uncode（Rust）对应 |
|------------------|----------|-------------------|
| `@earendil-works/pi-ai` | 多供应商 LLM、流式、工具协议、OAuth 等 | `uncode-ai`：`Api` trait、四种协议实现、`StreamEvent`、`CompatConfig`、内置模型表 |
| `@earendil-works/pi-agent-core` | 有状态循环、`AgentMessage`、事件流、工具并行/串行、`transformContext` / `convertToLlm`、`terminate` 语义 | `uncode-core`（事件/会话/工具类型）+ `uncode-agent`（`LoopEngine`、`AgentHarness`、压缩、工具实现） |
| `@earendil-works/pi-coding-agent` | CLI、交互/打印/RPC、默认工具、会话、扩展加载 | `uncode-cli` + `uncode-tui`；`uncode-rpc` / `uncode-platform` 为产品化延伸（Pi 本体无 Web 后端） |
| `@earendil-works/pi-tui` | 差分渲染终端 UI | `uncode-tui`：ratatui + crossterm（技术栈不同，角色相同） |
| Extensions（TS）+ Pi Packages | 可扩展内核 | `uncode-extensions`：WASM 方向 + `SkillRegistry` / 模板（与 Pi 的 Skill/Prompt 包路径一致） |

**结论（对齐度）**：分层职责与 Pi **高度同构**；uncode 额外增加 **Platform / JSON-RPC**，属于在 Pi 哲学之上的**交付面扩张**，不改变「Agent 核心在 agent+ai」这一主轴。

---

## 2. Pi 公开哲学与 uncode 对照

Pi 在 `pi-coding-agent` README 的 **Philosophy** 中明确若干「核心不做」，以换取极简内核与可组合扩展。下表评价 uncode 的**还原意图**与**当前实现状态**。

| Pi 立场 | 设计意图 | uncode 现状 | 评价 |
|---------|----------|---------------|------|
| **No MCP** | 用 Skill/CLI/Extension 组合替代 MCP 协议 | 主代码路径未见 MCP 宿主；能力以工具 + Skill 文件为主 | **一致**：与 Pi 同向；若未来为对接编辑器引入 MCP，应视为**显式偏离**，需写设计文档权衡 |
| **No sub-agents** | 不内置子代理；多实例或扩展自建 | 未见一等「子 Agent」产品化；循环为单 harness 双环 | **大体一致** |
| **No permission popups** | 安全策略交给环境/扩展 | 沙箱以 CWD 为界（`normalize_path` / `resolve_path`），无内置交互式权限 UI | **一致** |
| **No plan mode** | 计划写文件或用扩展 | 无与 OpenCode 类似的 build/plan 双模式产品化 | **一致** |
| **No built-in to-dos** | 避免干扰模型；用 TODO.md 等 | 未见内置 todo 工具为默认标配（以工具集文档为准） | **一致** |
| **No background bash** | 用 tmux 保持可观测 | Bash 工具为同步会话式执行 + 取消；非「隐形后台 agent」 | **一致** |

**总评**：在 Pi **明确写进 README 的哲学条款**上，uncode 的当前形态**整体遵循**「最小默认 + 扩展位」的路线，适合作为「Rust 版 Pi 系 Harness」的自我定位。

---

## 3. 核心机制深度对照

### 3.1 事件流与 UI 解耦

- **Pi**：`agent.subscribe(event => …)`，文档给出 `prompt()` 下完整事件序列（含工具阶段的 `tool_execution_*`）。
- **uncode**：`AgentEvent`（约 18 类）经 `broadcast` 分发；TUI/Platform 只订阅事件，不直接驱动循环。

**评价**：二者都是 **Harness 与 UI 解耦** 的同一哲学；uncode 用 Rust 枚举 + serde 得到**更强类型与可测试性**，代价是新增事件类型需改核心枚举并协调序列化稳定性（`#[non_exhaustive]` 已预留演进）。

### 3.2 Agent 循环：ReAct + 外层延续

- **Pi**：`agentLoop` + `turn` 语义；工具批量可 `parallel` / `sequential`；**全部** `terminate: true` 才结束跟进 LLM（与 README 一致）。
- **uncode**：`LoopEngine` 双层循环（文档明确与 Pi 双 `while` **同构**）；`ToolResult::terminate` 使用 **AND 语义**（与 Pi 一致）；`MessageQueue` 的 `steering` / `follow_up` / `next_turn` 对应「中途纠偏与会话延续」，与 Pi 的队列式输入模型同类。

**评价**：循环语义 **对齐度高**，属于 uncode 最值得保留的「Pi 灵魂」实现；Steering 三通道是 Rust 侧对交互复杂度的**显式建模**，优于隐式全局状态。

### 3.3 上下文管线：`transformContext` vs `transform_context`

- **Pi**：`AgentMessage[]` → 可选 `transformContext` → `convertToLlm` → LLM；支持声明合并扩展自定义消息类型。
- **uncode**：`build_context` + 可注入 `transform_context` 回调（见循环引擎文档），在发往模型前修改 `Vec<Message>`。

**评价**：**概念对齐**；Rust 用单一 `Message` 模型而非 TS 声明合并，扩展性略逊于 Pi 的「应用自定义 AgentMessage」，但工程上更简单、更可静态检查。

### 3.4 LLM 抽象：供应商爆炸 vs API-first

- **Pi `pi-ai`**：以**供应商**为粒度维护大量端点、OAuth、工具能力模型表。
- **uncode `uncode-ai`**：**协议优先**（OpenAI Completions、Anthropic Messages、Gemini、Ollama Native），新供应商主要通过 **Compat** 与模型表接入。

**评价**：这是 uncode 相对 Pi **有意强化的架构选择**（与仓库内 `LLM_DRIVER_*` 文档一致）：减少重复协议代码，**哲学上仍算「API-first 的 Pi 精神」变体**——把「统一」从包边界挪到协议边界。代价是：Pi 上某些「单供应商独有」的高级开关，在 uncode 中要嘛进 `CompatConfig`，要嘛显式扩展协议层。

### 3.5 工具系统

| 维度 | Pi | uncode |
|------|-----|--------|
| 默认集 | read / write / edit / bash（+ grep/find/ls 等） | 同左 + `find`/`ls` 已注册；`web_fetch`/`web_search` 注册但默认不对 LLM 暴露 |
| 运行时工具集 | `setActiveTools` | `set_active_tools` + CLI `--tools` / `--no-tools` / `--no-builtin-tools` |
| 执行流水线 | prepare → validate → before → execute | `prepare_and_validate` + hooks，顺序已对齐 |
| 参数校验 | TypeBox 全量 | 轻量 JSON Schema + prepare 后 **coerce**（string→int/bool 等子集） |
| 并行批次 | prepare/before 串行，execute 并发 | 同左（`loop_engine` 两阶段批次） |
| 批次串行降级 | 任一批次含 `sequential` 则整批串行 | 同左；`bash` 为 `Sequential` |
| 扩展方式 | Skill markdown、Extension、Pi Package | `#[tool]` 宏注册 + Skill 目录 + 未来 WASM |

**评价**：默认工具面略宽（网络类可选），但可通过 `set_active_tools` 收紧到 Pi 七件套；**机制层（流水线、批次串行、active 集）对齐度高**。TypeBox 级校验仍为工程差距，非哲学分歧。实现细节见 [`UNCODE_TOOL_SYSTEM.md`](../uncode-technologies/UNCODE_TOOL_SYSTEM.md)。

---

## 4. 会话与持久化：同构模型、异构存储

- **Pi 终端侧**：会话、分支、压缩等与 **JSONL + 文件树** 强绑定（用户心智：可拷贝、可 grep）。
- **uncode**：**逻辑模型**仍为树状 `SessionEntry`、分支、压缩、分支摘要等（与 Pi **概念同构**）；**物理存储**以 **SurrealDB（嵌入式）** 为主，并保留 **JSONL 导入/导出** 以兼容迁移与审计。

**评价**：

- **优点**：查询、索引、多客户端（TUI + Platform）并发访问更自然；团队场景优于纯 JSONL。
- **相对 Pi 的代价**：丢失「单目录纯文本即真相」的极简运维故事；调试需依赖导出工具或 DB 视图。
- **建议**：在对外叙事中明确写清 **「逻辑格式对齐 Pi，存储实现为工程化取舍」**，避免读者误以为 uncode =「仅 JSONL 换皮」。

---

## 5. 扩展与生态位

可组合扩展宿主 API（`set_active_tools`、扩展命令、Plan 模式拼装）的设计说明与路线图见 [`EXTENSION_COMPOSABLE_HARNESS_DESIGN.md`](EXTENSION_COMPOSABLE_HARNESS_DESIGN.md)。**Turn 与 Plan 模式粒度澄清**（每个 Turn 不内建 Plan 能力）见该文 **§2.3**。

| 维度 | Pi | uncode |
|------|-----|--------|
| 运行时扩展 | TypeScript Extension + npm/git 包 | WASM（`uncode-extensions`）+ Hook 生命周期 |
| 技能 | `SKILL.md` + 包管理器发现 | `.uncode/skills` + 与 opencode 路径兼容的扫描 |
| 包分发 | `pi install/remove/update` | 尚无对等 CLI 包管理 story（可后续补） |

**评价**：扩展 **哲学一致（内核小、外置大）**，**技术路线不同**：WASM 上限高、宿主接口需精心设计；当前成熟度通常**落后于** Pi 已有多年的 TS 扩展生态——这是 Rust 复刻的**正常阶段风险**，不是设计错误。

---

## 6. 综合评价矩阵

| 维度 | 对齐 Pi 的程度 | 简评 |
|------|----------------|------|
| 分层与依赖向 | ★★★★★ | workspace 与 Pi mono 分包一一对应 |
| 循环与工具语义 | ★★★★★ | 双环、terminate AND、并行工具与 Pi 文档一致 |
| 事件驱动 UI | ★★★★☆ | 强类型事件更「硬」，扩展字段需版本纪律 |
| LLM 层哲学 | ★★★★☆ | API-first 是 Pi 精神的强化版，非 1:1 抄 pi-ai |
| 终端极简默认值 | ★★★☆☆ | 网络工具等略多于 Pi 四件套 |
| 存储与运维故事 | ★★★☆☆ | 能力上可导出 JSONL，默认不如纯文件直观 |
| 扩展生态成熟度 | ★★☆☆☆ | WASM 与 Hook 仍在路上，较 Pi 落后 |
| 多界面交付 | ★（超出 Pi） | Platform/RPC 非 Pi 核心，属增量 |

**一句话**：uncode 在 **Agent 内核哲学与主路径行为** 上，已具备「Pi 的 Rust 方言」特征；在 **生态与存储工程化** 上做了符合团队产品的取舍，需在文档中自觉标注与 Pi 的差异，以免「复刻」被误读为「字节级兼容」。

---

## 7. 建议路线（面向「更 Pi」或「更产品」）

1. **文档**：保持 `UNCODE_*` 系列与实现同步（例如会话层 SurrealDB 与 JSONL 叙事并存时，须在总览中写清「逻辑 vs 物理」）。  
2. **默认值**：若强调「极致 Pi」，可考虑将 web_* 等列为「可选插件层」而非默认注册。  
3. **扩展**：优先落地 WASM 宿主 **最小能力面**（文件/子进程/HTTP 白名单），向 Pi 的 extension 能力曲线靠拢。  
4. **互操作**：若需编辑器生态，单独立项讨论 MCP；与 Pi 哲学冲突时应在 `docs/` 中做决策记录。  
5. **验证**：对 Pi 公开事件序列做 **fixture 级对齐测试**（同一 prompt 脚本下事件顺序与关键字段），用数据证明「行为级复刻」进度。

---

## 8. 参考阅读

- **Pi**：根 `README.md`；`packages/coding-agent/README.md`（Philosophy）；`packages/agent/README.md`（事件与循环）；`packages/ai/README.md`（供应商与工具流）。  
- **uncode**：`docs/uncode-technologies/UNCODE_OVERVIEW.md`、`UNCODE_LOOP_ENGINE.md`、`UNCODE_EVENT_SYSTEM.md`、`UNCODE_TOOL_SYSTEM.md`、`UNCODE_SESSION_MODEL.md`。  
- **与另一 Rust 项目的对比**（非 Pi）：`docs/guides/COMPARISON_PI_AGENT_RUST.md`（对象为 `pi_agent_rust`，勿与本文 Pi 混淆）。  
- **历史会话层长文**（已按 SurrealDB 修订 uncode 侧）：`docs/pi-technologies/SESSION_LAYER_COMPARISON_PI.md`（顶部勘误框 + SSOT 链接）。

---

*本文档为架构层评价，随 uncode 与上游 Pi 演进应定期修订。*
