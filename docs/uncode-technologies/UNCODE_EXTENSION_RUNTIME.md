# uncode Extension 运行时：Pi 源码分析与差距评估

> 基于 Pi (`earendil-works/pi`) 源码的 Extension 系统深度分析，对照 uncode 当前实现状态，评估差距并提出分阶段实施路径。

---

## 1 Pi Extension 系统架构

### 1.1 三层能力矩阵

Pi 扩展系统由三层接口构成，扩展开发者的入口是 `ExtensionAPI`：

```
┌─────────────────────────────────────────────────────────┐
│                   ExtensionAPI                            │
├──────────────┬──────────────────┬────────────────────────┤
│  事件订阅    │   工具注册        │   命令/快捷键/CLI flag  │
│  (on)        │   (registerTool) │   (registerCommand)    │
│              │                  │   (registerShortcut)   │
│  28 种事件   │   LLM 可调用工具  │   (registerFlag)       │
│  可拦截/修改  │   自定义渲染      │                        │
│  可阻断      │   流式更新        │                        │
├──────────────┴──────────────────┴────────────────────────┤
│                   ExtensionContext                        │
│  ui (14 个交互方法) │ sessionManager │ modelRegistry       │
│  cwd │ signal │ abort │ compact │ getSystemPrompt        │
├──────────────────────────────────────────────────────────┤
│                   ExtensionUIContext                      │
│  select │ confirm │ input │ notify │ setStatus           │
│  setWorkingMessage │ setWidget │ setFooter │ setHeader   │
│  custom (overlay) │ editor │ pasteToEditor │ setTheme    │
└──────────────────────────────────────────────────────────┘
```

源码位置：`packages/coding-agent/src/core/extensions/`

| 文件 | 职责 | 规模 |
|:---|:---|:---|
| `types.ts` | 类型定义（事件、工具、命令、UI 上下文） | ~1200 行 |
| `runner.ts` | 扩展执行与事件分发 | ~54K 行 |
| `loader.ts` | 发现、加载、运行时管理 | ~19K 行 |
| `wrapper.ts` | 工具注册辅助 | — |
| `index.ts` | 统一导出 | — |

### 1.2 扩展生命周期

```
发现 (discover) → 加载 (jiti) → 注册 (hooks/tools/commands) → 运行 (event dispatch) → 热重载 (/reload)
```

**发现路径**（按优先级）：

1. 项目本地：`cwd/.pi/extensions/`
2. 全局目录：`~/.pi/agent/extensions/`
3. 配置文件：`settings.json` 的 `extensions` 和 `packages` 字段

**加载机制**：使用 jiti 直接加载 TypeScript，无需预编译。支持单文件 `.ts`、目录 `index.ts`、以及带 `package.json` 的完整包（可声明自己的依赖）。

**分发方式**：

```json
// settings.json
{
  "packages": ["npm:@foo/bar@1.0.0", "git:github.com/user/repo@v1"],
  "extensions": ["/path/to/local/extension.ts"]
}
```

### 1.3 扩展开发范式

Pi 扩展是一个**导出默认工厂函数的 TypeScript 模块**：

```typescript
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";

export default function (pi: ExtensionAPI) {
  // 1. 注册 LLM 可调用的工具
  pi.registerTool({
    name: "todo",
    label: "待办事项",
    description: "管理待办事项列表",
    parameters: Type.Object({ action: Type.String(), item: Type.Optional(Type.String()) }),
    async execute(toolCallId, params, signal, onUpdate, ctx) {
      return { content: [{ type: "text", text: "完成" }], details: { state: {} } };
    },
  });

  // 2. 订阅事件（可拦截、修改、阻断）
  pi.on("tool_call", async (event, ctx) => {
    if (event.toolName === "bash" && event.input.command.includes("rm -rf")) {
      return { block: true, reason: "危险操作被拦截" };
    }
  });

  // 3. 注册命令
  pi.registerCommand("summarize", {
    description: "总结对话",
    handler: async (args, ctx) => { /* ... */ },
  });
}
```

---

## 2 Pi 的 28 种事件钩子详解

### 2.1 完整事件清单

#### 会话事件（8 种）

| 事件 | 能力 | 返回值 |
|:---|:---|:---|
| `session_start` | 会话启动/加载/重载 | — |
| `session_before_switch` | 会话切换前（可取消） | `{ cancel?: boolean }` |
| `session_before_fork` | 会话分叉前（可取消） | `{ cancel?: boolean }` |
| `session_before_compact` | 上下文压缩前（可取消或提供自定义压缩） | `{ cancel?, compaction? }` |
| `session_compact` | 压缩完成 | — |
| `session_before_tree` | 树导航前（可取消或提供自定义摘要） | `{ cancel?, summary?, customInstructions? }` |
| `session_tree` | 树导航完成 | — |
| `session_shutdown` | 运行时关闭（quit/reload/会话替换） | — |

#### Agent 循环事件（6 种）

| 事件 | 能力 | 返回值 |
|:---|:---|:---|
| `before_agent_start` | Agent 循环启动前（可注入消息、替换 system prompt） | `{ message?, systemPrompt? }` |
| `agent_start` | Agent 循环开始 | — |
| `agent_end` | Agent 循环结束 | — |
| `turn_start` | Turn 开始 | — |
| `turn_end` | Turn 结束 | — |
| `context` | **修改发送给 LLM 的消息数组** | `{ messages?: AgentMessage[] }` |

#### 消息事件（3 种）

| 事件 | 能力 | 返回值 |
|:---|:---|:---|
| `message_start` | 消息开始 | — |
| `message_update` | 消息流式更新 | — |
| `message_end` | 消息结束（可替换消息） | `{ message?: AgentMessage }` |

#### 工具事件（5 种）

| 事件 | 能力 | 返回值 |
|:---|:---|:---|
| `tool_execution_start` | 工具执行开始 | — |
| `tool_call` | **工具调用前：可阻断，可修改参数** | `{ block?: boolean, reason?: string }` |
| `tool_execution_update` | 工具流式输出 | — |
| `tool_result` | **工具结果：可替换 content/details/isError** | `{ content?, details?, isError? }` |
| `tool_execution_end` | 工具执行结束 | — |

#### Provider 事件（2 种）

| 事件 | 能力 | 返回值 |
|:---|:---|:---|
| `before_provider_request` | LLM 请求发送前（可替换 payload） | `unknown` |
| `after_provider_response` | LLM 响应接收后 | — |

#### 模型事件（2 种）

| 事件 | 能力 | 返回值 |
|:---|:---|:---|
| `model_select` | 模型切换 | — |
| `thinking_level_select` | 思考级别切换 | — |

#### 用户事件（2 种）

| 事件 | 能力 | 返回值 |
|:---|:---|:---|
| `input` | 用户输入（可转换/拦截/处理） | `{ action: "continue" | "transform" | "handled" }` |
| `user_bash` | 用户 Bash 命令（可替换执行） | `{ operations?, result? }` |

### 2.2 事件能力的三级分类

Pi 事件不仅是"通知"，而是具有三级控制能力：

| 能力级别 | 语义 | 示例 |
|:---|:---|:---|
| **观察** (Observe) | 接收事件，不能修改 | `agent_start`, `turn_end` |
| **修改** (Modify) | 接收事件，可修改传递给下游的数据 | `context` → 修改 messages, `tool_result` → 替换内容 |
| **阻断** (Block) | 接收事件，可阻止后续执行 | `tool_call` → `{ block: true }`, `session_before_*` → `{ cancel: true }` |

**关键设计**：`tool_call` 事件的 `event.input` 是**可变引用**——扩展可以直接修改工具参数，后续处理器看到的是修改后的值。

---

## 3 Pi 的工具注册系统

### 3.1 ToolDefinition 接口

```typescript
interface ToolDefinition<TParams, TDetails, TState> {
  name: string;                          // 工具名（LLM 调用使用）
  label: string;                         // UI 显示名
  description: string;                   // LLM 看到的描述
  promptSnippet?: string;                // system prompt 中的工具简介
  promptGuidelines?: string[];           // system prompt 中追加的使用指南
  parameters: TSchema;                   // TypeBox 参数 schema
  renderShell?: "default" | "self";      // 渲染模式
  executionMode?: "sequential" | "parallel";
  prepareArguments?: (args: unknown) => Static<TParams>;  // 参数兼容性 shim

  execute(
    toolCallId: string,
    params: Static<TParams>,
    signal: AbortSignal | undefined,
    onUpdate: AgentToolUpdateCallback<TDetails> | undefined,
    ctx: ExtensionContext,
  ): Promise<AgentToolResult<TDetails>>;

  renderCall?: (args, theme, context) => Component;      // 自定义调用渲染
  renderResult?: (result, options, theme, context) => Component;  // 自定义结果渲染
}
```

### 3.2 工具注册后的效果

扩展通过 `pi.registerTool()` 注册的工具会：

1. 出现在 LLM 可用工具列表中（自动注入 system prompt）
2. 由 `ToolRegistry` 统一调度（与内置工具地位等同）
3. 支持自定义 TUI 渲染（`renderCall` / `renderResult`）
4. 支持流式更新（`onUpdate` 回调）
5. 状态可跨渲染保持（`TState` 泛型）
6. 可被其他扩展的 `tool_call` / `tool_result` 事件拦截

---

## 4 Pi 的 UI 集成能力

### 4.1 ExtensionUIContext（14 个方法）

| 方法 | 能力 |
|:---|:---|
| `select(title, options)` | 选择器对话框 |
| `confirm(title, message)` | 确认对话框 |
| `input(title, placeholder)` | 文本输入对话框 |
| `notify(message, type)` | 桌面通知 |
| `onTerminalInput(handler)` | 原始终端输入监听 |
| `setStatus(key, text)` | 状态栏文字 |
| `setWorkingMessage(message)` | 加载提示文字 |
| `setWorkingIndicator(options)` | 加载动画自定义 |
| `setWidget(key, content)` | 编辑器上方/下方组件 |
| `setFooter(factory)` | 自定义 footer 组件 |
| `setHeader(factory)` | 自定义 header 组件 |
| `custom(factory, options)` | 自定义 overlay 组件 |
| `setEditorComponent(factory)` | 自定义编辑器（如 Vim 模式） |
| `addAutocompleteProvider(factory)` | 自动补全增强 |

所有对话框支持 `timeout` 和 `signal` 参数，可实现自动消失。

---

## 5 Pi 的 76 个示例扩展分类

### 5.1 按能力类别

| 类别 | 数量 | 代表扩展 |
|:---|:---|:---|
| 自定义工具 | 15+ | `hello`, `todo`, `dynamic-tools`, `structured-output`, `ssh`, `truncate-tool` |
| 安全护栏 | 5+ | `permission-gate`, `protected-paths`, `confirm-destructive`, `dirty-repo-guard`, `sandbox` |
| 命令系统 | 10+ | `pirate`, `summarize`, `handoff`, `preset`, `tools`, `commands` |
| UI 增强 | 10+ | `status-line`, `custom-footer`, `notify`, `modal-editor`, `rainbow-editor` |
| 会话管理 | 5+ | `session-name`, `bookmark`, `git-checkpoint`, `auto-commit-on-exit`, `plan-mode` |
| 复杂系统 | 5+ | `plan-mode/`, `subagent/`, `interactive-shell`, `dynamic-resources/`, `custom-provider-*` |
| 游戏/Fun | 4 | `snake`, `tic-tac-toe`, `doom-overlay`, `space-invaders` |
| Provider 集成 | 2 | `custom-provider-anthropic/`, `custom-provider-gitlab-duo/` |

### 5.2 按使用的核心 API

| 依赖的 API | 扩展数量 | 说明 |
|:---|:---|:---|
| `on("tool_call")` 阻断 | 5+ | 所有安全护栏类扩展 |
| `on("tool_result")` 修改 | 3+ | `tool-override`, `minimal-mode`, `built-in-tool-renderer` |
| `registerTool()` | 15+ | 所有自定义工具扩展 |
| `registerCommand()` | 10+ | 所有命令类扩展 |
| `ctx.ui.*` | 10+ | 所有 UI 增强扩展 |
| `on("context")` 修改消息 | 3+ | `pirate`, `system-prompt-header`, `claude-rules` |
| `on("input")` 拦截输入 | 2+ | `input-transform`, `inline-bash` |

---

## 6 uncode 当前扩展系统状态

### 6.1 已实现

```
uncode-extensions/
├── hooks.rs   — LifecycleHook (8 枚举) + Extension trait + HookRegistry
│                ✅ 能注册扩展到指定钩子
│                ✅ 能异步分发钩子事件
│                ✅ 20+ 个测试覆盖注册/分发/过滤
│                ❌ 只能观察，不能拦截/修改/阻断
│
├── api.rs     — ExtensionApi (注册入口)
│                ✅ register_extension() 可用
│                ❌ 无 registerTool / registerCommand / registerShortcut
│
├── loader.rs  — ExtensionLoader
│                ❌ load_from_dir() 永远返回 Ok(0)
│                ❌ 无 wasmtime/wasmer 依赖
│
└── Cargo.toml — 仅有基础依赖（tokio, serde, dashmap 等）
                  ❌ 未被 agent/tui/cli 任何 crate 引用（孤立 crate）
```

### 6.2 LifecycleHook 对比

| uncode LifecycleHook | Pi 对应事件 | uncode 能力 | Pi 能力 |
|:---|:---|:---|:---|
| `SessionStart` | `session_start` | 观察 | 观察 |
| `TurnStart` | `turn_start` | 观察 | 观察 |
| `MessageReceived` | `message_start` | 观察 | 观察 |
| `MessageSending` | — | 观察 | — |
| `ToolCallBefore` | `tool_call` | 观察 | **拦截/阻断/修改参数** |
| `ToolCallAfter` | `tool_result` | 观察 | **修改结果** |
| `TurnEnd` | `turn_end` | 观察 | 观察 |
| `SessionEnd` | `session_shutdown` | 观察 | 观察 |

缺失的 20 个 Pi 事件在 uncode 中完全没有对应。

### 6.3 Extension trait 对比

```rust
// uncode — 只能观察
#[async_trait]
pub trait Extension: Send + Sync {
    fn name(&self) -> &str;
    async fn on_hook(&self, ctx: &HookContext) -> anyhow::Result<()>;
    //                                            返回空 ← 无控制能力
}
```

```typescript
// Pi — 可观察、修改、阻断
type ExtensionHandler<E, R = undefined> = (event: E, ctx: ExtensionContext) => Promise<R | void> | R | void;
// R 可以是 ToolCallEventResult { block, reason }、ContextEventResult { messages } 等
```

---

## 7 差距量化

| 维度 | Pi | uncode | 差距 |
|:---|:---|:---|:---|
| 事件类型 | 28 种（含类型化输入/输出） | 8 种（枚举，无类型化数据） | **3.5x** |
| 事件能力 | 拦截 + 修改 + 阻断 | 仅观察 | **质变** |
| 工具注册 | `registerTool` + 自定义渲染 + 流式更新 | 无 | **0%** |
| 命令系统 | `registerCommand` + `registerShortcut` + `registerFlag` | 无 | **0%** |
| UI 上下文 | 14 个交互方法 + overlay + 自定义组件 | 无 | **0%** |
| WASM 运行时 | 不使用（jiti 直接加载 TS） | 声称 WASM 但未实现 | **0%** |
| 扩展发现 | 3 层自动发现 + npm/git 分发 | 无 | **0%** |
| 热重载 | `/reload` 命令 | 无 | **0%** |
| 生态 | 76 个示例扩展 | 0 个 | **0%** |
| Agent 集成 | 深度集成（loop/harness/tools/TUI 全部接入） | 完全孤立（无其他 crate 依赖） | **0%** |

---

## 8 技术栈选型差异：Pi (jiti + 进程内 TS) vs uncode (WASM 沙箱)

### 8.1 选型总览

| 维度 | Pi | uncode |
|:---|:---|:---|
| 宿主语言 | TypeScript / Node.js / Bun | Rust |
| 扩展语言 | TypeScript（jiti 运行时加载） | Rust → 编译为 WASM（计划） |
| 运行时隔离 | **无隔离**——扩展与宿主同进程 | **WASM 沙箱**——扩展运行在独立实例 |
| 模块加载器 | jiti v2.7（运行时 TS→JS 转译，无需预编译） | wasmtime（计划，当前未引入） |
| Schema 系统 | TypeBox（运行时 JSON Schema + TS 类型推导） | JSON Schema via serde_json（已用） |
| 序列化边界 | **无**——共享内存，对象引用直传 | **WASM 线性内存**——需显式序列化/反序列化 |
| 扩展依赖管理 | 自带 package.json，npm install 安装 | 需编译为独立 .wasm，无运行时依赖安装 |
| 热重载 | `/reload` 命令，jiti 重新 import | 需重新实例化 WASM module |

### 8.2 加载机制

#### Pi：jiti 运行时加载

Pi 使用 **jiti** 在宿主进程内直接加载 TypeScript 源码，无需编译步骤：

```
用户编写 extension.ts → jiti 运行时转译为 JS → 同进程执行 → 共享宿主所有类型和状态
```

**关键特性**：

- **零构建步骤**：开发者写 `.ts` 文件，放入 `.pi/extensions/` 即可使用
- **虚拟模块注入**：Pi 二进制版通过 virtualModules 将 TypeBox、pi-coding-agent 等 SDK 预打包，扩展无需安装依赖即可 import
- **开发/生产双模式**：
  - Bun binary 模式：虚拟模块解析，单文件分发
  - Node.js 开发模式：别名解析到 node_modules，支持热修改
- **工厂函数模式**：扩展导出 `export default function (pi: ExtensionAPI)`，宿主调用后获得注册入口

```typescript
// Pi 的加载链路（简化）
const module = await jiti.import(extensionPath);        // jiti 转译 TS → JS
const factory = module.default;                          // 取工厂函数
const api = new ExtensionAPI(runtime);                   // 创建注册入口
await factory(api);                                      // 执行注册
```

#### uncode：WASM 沙箱加载（计划）

uncode 的设计是让扩展编译为 `.wasm` 文件，由宿主通过 WASM 运行时加载并沙箱化执行：

```
开发者编写 Rust 扩展 → cargo build --target wasm32-wasip2 → .wasm 文件
→ wasmtime 加载实例化 → 通过 host functions 暴露 API → 扩展调用 host 能力
```

**关键特性**：

- **编译前置**：开发者需提前编译为 WASM，不能直接加载源码
- **ABI 边界**：host 与扩展之间通过 `wasmtime::Func` 通信，所有数据需序列化
- **安全隔离**：扩展默认无法访问文件系统、网络、宿主内存
- **能力白名单**：扩展只能调用宿主显式暴露的 host functions

```rust
// uncode 的计划加载链路（当前未实现）
let engine = Engine::default();
let module = Module::from_file(&engine, wasm_path)?;
let mut store = Store::new(&engine, HostState::new());
let instance = linker.instantiate(&mut store, &module)?;
// 通过 linker.define_func() 暴露 host API
```

### 8.3 数据传递模型

这是两种技术栈最根本的差异。

#### Pi：引用传递 + 可变共享

```typescript
// Pi 扩展直接修改宿主内存中的对象
pi.on("tool_call", async (event) => {
  event.input.command = event.input.command.replace(/rm -rf/, "echo blocked");
  // event 是宿主内存中的引用，修改立即生效
  // 无序列化开销，无数据拷贝
});
```

- 零拷贝：事件数据是宿主内存中的引用
- 可变共享：扩展可以直接修改 event 对象，后续处理器看到修改后的值
- `structuredClone()` 仅在特定场景（如消息链式修改）使用
- 类型安全由 TypeScript 编译时保证，运行时无额外验证

#### uncode：序列化边界（需设计）

WASM 扩展与宿主之间通过线性内存通信，**所有跨边界的数据传递都需要序列化**：

```
宿主 (Rust)                     WASM 扩展
┌──────────────┐               ┌──────────────┐
│ HookContext  │ ──serde_json──→│ 线性内存     │
│              │               │              │
│ Host State   │←──host func──│ 扩展逻辑     │
│              │               │              │
│ HookResult   │←─serde_json──│ 返回值       │
└──────────────┘               └──────────────┘
```

**需要解决的问题**：

1. **ABI 定义**：host functions 的签名、参数传递约定、内存分配/释放策略
2. **序列化格式**：JSON（通用但慢）vs bincode/MessagePack（快但需要两端共识）
3. **内存管理**：谁分配、谁释放、如何避免内存泄漏
4. **类型映射**：Rust 的 `AgentEvent` 31 个变体如何映射到 WASM 侧的类型

**可选方案对比**：

| 方案 | 优点 | 缺点 |
|:---|:---|:---|
| **wasmtime + JSON ABI** | 简单通用，扩展可用任意语言编写 | 序列化开销大，高频事件（message_update）性能差 |
| **wasmtime + witx/wit** | 类型安全的 ABI 定义，wasm-toolchain 标准化 | 引入 wasm-component-model 复杂度 |
| **wasmer + 自定义 ABI** | 可精细控制性能 | 非标准，维护成本高 |
| **不用 WASM，用动态加载 (.so/.dll)** | 零序列化开销，原生 Rust 性能 | 无安全隔离，跨平台兼容性差 |
| **不用 WASM，用 Lua/ Rhai 脚本** | 嵌入简单，热重载容易 | 性能受限，类型安全弱，生态小 |

### 8.4 安全模型

#### Pi：信任模型

Pi 的扩展**运行在宿主进程中，没有任何隔离**：

- 扩展拥有与宿主完全相同的权限（文件系统、网络、进程）
- 没有内置的权限系统——"Only install extensions from sources you trust"
- 可选的沙箱能力通过 `sandbox` 扩展实现（覆盖 `bash` 工具，用 bubblewrap/sandbox-exec 包裹命令）
- 扩展的错误不会崩溃宿主（try/catch 包裹），但恶意扩展可以执行任意代码

#### uncode：能力隔离模型（设计空间）

WASM 天然提供三层隔离：

| 层级 | 保护范围 | 机制 |
|:---|:---|:---|
| **内存隔离** | 扩展无法读写宿主内存 | WASM 线性内存边界 |
| **能力隔离** | 扩展无法调用未授权的 host function | wasmtime linker 白名单 |
| **资源隔离** | 限制 CPU 时间、内存用量、文件系统访问 | wasmtime config（fuel、memory limit） |

**需要设计的安全策略**：

1. **文件系统访问**：通过 host function 暴露受限的文件操作（限定 CWD 内）
2. **网络访问**：默认禁止，通过 host function 按需开放
3. **资源限制**：设置 fuel limit 防止无限循环，memory limit 防止 OOM
4. **能力声明**：扩展 manifest 中声明所需权限（类似 WebExtension 的 permissions）

### 8.5 开发者体验对比

#### Pi 的 DX

```bash
# 编写扩展
vim ~/.pi/extensions/my-tool.ts

# 立即生效（热重载）
/reload

# 或者项目级别
vim .pi/extensions/my-tool.ts   # 项目启动自动加载
```

- **零工具链**：只需一个文本编辑器
- **即时反馈**：保存后 `/reload` 即可测试
- **TypeScript 类型提示**：IDE 自动补全 ExtensionAPI 的所有方法
- **依赖管理**：扩展可以有自己的 `package.json`，`npm install` 安装依赖
- **模板丰富**：76 个示例覆盖各种场景

#### uncode 的 DX（设计空间）

```bash
# 编写扩展（需要 Rust 工具链）
cargo new --lib my-extension
cd my-extension
# 编辑 src/lib.rs，实现 Extension trait
# 编辑 Cargo.toml，添加 uncode-extension-sdk 依赖

# 编译为 WASM
cargo build --target wasm32-wasip2 --release

# 安装
cp target/wasm32-wasip2/release/my_extension.wasm ~/.uncode/extensions/

# 测试（需重启 Agent）
uncode
```

**DX 挑战与缓解策略**：

| 挑战 | 缓解策略 |
|:---|:---|
| 需要 Rust 工具链 | 提供 `uncode ext init` / `uncode ext build` CLI 命令封装编译流程 |
| 编译等待时间 | 提供 SDK crate 预编译，扩展只需编译自身代码 |
| 无热重载 | 可实现 watch 模式：文件变更 → 自动重编译 → 自动重载 WASM |
| 类型提示依赖 SDK crate | 发布 `uncode-extension-sdk` crate，提供 Extension trait + 所有事件类型 |
| 调试困难 | 提供 `uncode ext run --local` 模式，跳过 WASM 直接在宿主进程加载（开发模式） |
| 示例不足 | 随 SDK 提供模板项目：`cargo generate uncode/extension-template` |

**关键建议**：提供两种运行模式——

1. **开发模式**：扩展编译为 .so/.dll 动态库，直接加载到宿主进程（零序列化，可调试）
2. **生产模式**：扩展编译为 .wasm，WASM 沙箱隔离执行（安全隔离）

这样开发者获得 Pi 级别的 DX（快速迭代、直接调试），生产环境获得 WASM 的安全保障。

### 8.6 技术栈选型的深层原因

这不是简单的"用什么语言"的选择——两种技术栈反映了不同的**安全哲学**：

| | Pi | uncode |
|:---|:---|:---|
| **信任假设** | 用户信任自己安装的扩展 | 不信任任何第三方扩展 |
| **安全哲学** | "Don't install what you don't trust" | "Verify, don't trust" |
| **隔离需求** | 低——用户自己管理扩展来源 | 高——可能支持社区扩展市场 |
| **开放性** | 完全开放——扩展可以做到宿主能做的一切 | 白名单开放——扩展只能做到宿主允许的 |
| **生态策略** | 社区驱动，GitHub/npm 分发 | 先内部使用，未来可能开放生态 |

**两者都是合理的**：Pi 追求最大灵活性和最低开发门槛，适合个人/小团队快速定制；uncode 追求安全隔离和可审计性，适合企业级部署和未来的扩展市场。

### 8.7 技术栈选型对照表

| 维度 | Pi (jiti + TS) | uncode (wasmtime + Rust) |
|:---|:---|:---|
| 安全性 | 无沙箱，扩展信任执行 | WASM 沙箱隔离 |
| 性能 | 原生 JS 性能，共享内存 | 需序列化跨越 WASM 边界（高频场景有开销） |
| 开发体验 | TypeScript 直接编写，热重载 | 需 Rust 工具链 + WASM 编译 |
| 生态门槛 | 前端开发者即可编写 | 需要 Rust 或 WASM 工具链 |
| ABI 复杂度 | 共享类型定义即可 | 需定义 host function ABI + 序列化约定 |
| 扩展能力 | 无限制（可访问宿主全部 API） | 受限于暴露的 host functions（白名单） |
| 跨语言支持 | 仅 TypeScript/JavaScript | 理论上支持所有可编译为 WASM 的语言 |
| 内存安全 | 依赖 V8/Bun GC | WASM 天然内存安全（Rust 编译保障） |
| 调试能力 | 宿主进程内可直接断点调试 | 需 wasmtime 调试支持或开发模式回退 |
| 分发格式 | 源码 `.ts` / npm 包 | 预编译 `.wasm` 二进制 |
| 版本兼容 | 依赖运行时类型检查 | 需要定义稳定的 ABI 版本协议 |

---

## 9 分阶段实施建议

### Phase 1 — 拦截器能力（从观察到控制）

**目标**：将钩子系统从观察者模式升级为拦截器模式。

**关键改动**：

1. 修改 `Extension::on_hook()` 返回值：
```rust
pub enum HookResult {
    Continue,
    Modify(HookContext),
    Block { reason: String },
}
```

2. 在 agent loop 和 harness 中插入钩子调用点，处理 `Block` 和 `Modify` 返回值

3. 将 `uncode-extensions` 加入 `uncode-agent` 的依赖，消除孤立状态

**影响评估**：需要修改 `hooks.rs`（trait + registry）、`loop_engine.rs`（调用点）、`harness.rs`（调用点）

### Phase 2 — 工具注册（扩展 LLM 能力边界）

**目标**：扩展可以注册 LLM 可调用的自定义工具。

**关键改动**：

1. 定义 `ExtensionToolDefinition` trait，包含 name、description、parameters、execute
2. 实现 `ExtensionToolWrapper`，将扩展工具适配为 `ToolExecutor`
3. 在 `ToolRegistry` 中支持动态注册扩展工具
4. 在 `loop_engine.rs` 中将扩展工具加入 LLM 可用工具列表

**影响评估**：需要修改 `uncode-agent/src/tools/registry.rs`、新增 `uncode-extensions/src/tool.rs`

### Phase 3 — 事件扩展 + 命令系统

**目标**：从 8 个 `LifecycleHook` 扩展到覆盖 Pi 的核心事件类型；实现命令注册。

**关键改动**：

1. 将 `LifecycleHook` 枚举扩展为结构化事件类型（参考 Pi 的 28 种事件）
2. 每种事件携带类型化数据（而非泛化的 `HookEvent`）
3. 实现 `registerCommand()` 接口
4. 在 TUI 的 slash command 系统中接入扩展命令

**影响评估**：需要重构 `hooks.rs`、新增 `uncode-extensions/src/command.rs`、修改 `uncode-tui` 的命令系统

### Phase 4 — WASM 运行时

**目标**：实现真正的 WASM 扩展加载和执行。

**关键改动**：

1. 引入 wasmtime 依赖
2. 定义 WASM ↔ Host ABI（host functions 暴露工具注册、事件订阅等能力）
3. 实现序列化层（跨越 WASM 边界的数据传输）
4. 实现 `ExtensionLoader::load_from_dir()` 真正加载 `.wasm` 文件
5. 安全策略：限制文件系统访问、网络访问、CPU 时间

**影响评估**：Cargo.toml 新增 wasmtime 依赖，`loader.rs` 完全重写，新增 ABI 定义模块

### Phase 5 — 发现与生态

**目标**：自动发现、热重载、示例扩展库。

**关键改动**：

1. 自动发现 `~/.uncode/extensions/` 和项目本地 `.uncode/extensions/`
2. `/reload` 命令支持扩展热重载
3. 参照 Pi 的 76 个示例，编写 10-15 个 uncode 示例扩展（Rust → WASM）
4. 优先覆盖：安全护栏、自定义工具、会话管理三类

---

## 10 关键结论

1. **Pi 扩展系统的核心竞争力不是"有插件"，而是"插件能控制 Agent 行为"。** 28 种事件 + 拦截/修改/阻断三级能力 + 工具注册 + UI 集成，构成了一个深度可编程的 Agent 平台。

2. **uncode 当前的扩展系统是"有骨架无血肉"的孤立 crate。** 类型定义和注册机制存在，但无控制能力、无工具注册、无 Agent 集成、无 WASM 运行时。

3. **最高优先级是 Phase 1（拦截器能力）。** 没有拦截/修改/阻断能力，后续的工具注册、命令系统、安全护栏都无法实现。这是 76 个 Pi 扩展中超过 70% 所依赖的基础能力。

4. **WASM 沙箱是正确的方向但增加了工程复杂度。** 需要在 Phase 4 中认真设计 ABI，并考虑为扩展开发者提供良好的开发体验（可能需要 SDK crate）。

5. **建议参照 Pi 的 top-10 示例优先实现。** `permission-gate`、`todo`、`status-line`、`session-name`、`custom-footer`、`git-checkpoint`、`preset`、`handoff`、`plan-mode`、`subagent` 这 10 个扩展覆盖了 Pi 最核心的差异化能力。
