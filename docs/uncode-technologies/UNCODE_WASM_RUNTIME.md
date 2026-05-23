# UNCODE_WASM_RUNTIME — WASM 扩展运行时机制与原理

> **实现层**：`crates/uncode-extensions/src/wasm/`
> **运行时引擎**：wasmtime 29（Bytecode Alliance，纯 Rust，Cranelift JIT）
> **相关文档**：`UNCODE_EXTENSION_RUNTIME.md`（Pi 对比与差距分析）

---

## 1 设计目标

uncode 的 WASM 扩展运行时解决一个核心问题：**如何安全地执行不可信的第三方代码？**

Pi 使用 jiti 在宿主进程内直接加载 TypeScript 扩展——零隔离，扩展可以访问 Node.js 全部 API。uncode 选择 WebAssembly 作为扩展执行环境，实现三个安全保障：

| 维度 | Pi (jiti + TS) | uncode (WASM + Rust) |
|:---|:---|:---|
| 内存隔离 | 无（共享进程内存） | WASM 线性内存隔离 |
| 文件系统 | 完全访问 | 无（仅通过宿主函数） |
| 网络 | 完全访问 | 无（仅通过宿主函数） |
| CPU 限制 | 无 | fuel 指令计数 |
| 执行超时 | 无 | 可配置（默认 5s） |
| 语言支持 | 仅 TypeScript | 任何可编译为 WASM 的语言 |

---

## 2 架构总览

```
┌─────────────────────────────────────────────────────────────────────┐
│  uncode-cli (main.rs)                                               │
│                                                                     │
│  ExtensionApi ─── ExtensionLoader ─── ~/.uncode/extensions/         │
│       │                    │              ├── hello.wasm             │
│       │                    │              └── hello.json             │
│       │                    ▼                                        │
│       │              ┌──────────┐                                   │
│       │              │WasmEngine│  单例 Engine + Linker              │
│       │              └────┬─────┘                                   │
│       │                   │ instantiate()                            │
│       │                   ▼                                          │
│       │    ┌──────────────────────────────────────┐                 │
│       │    │ (WasmInstance, Vec<WasmExtensionTool>) │                │
│       │    │                                      │                 │
│       │    │  WasmInstance                         │                 │
│       │    │  ├ Store (独立 wasmtime Store)        │                 │
│       │    │  ├ WasmExports (缓存导出函数引用)     │                 │
│       │    │  └ disabled (trap 后自动禁用)         │                 │
│       │    │                                      │                 │
│       │    │  WasmExtensionTool[] (共享 inner Arc) │                 │
│       │    │  └ ExtensionTool trait 实现          │                 │
│       │    └──────────────────────────────────────┘                 │
│       │                                                              │
│       ├──── 钩子路径:                                                │
│       │    ExtensionLifecycleBridge ── HookRegistry.fire()          │
│       │         │                                                    │
│       │         ▼                                                    │
│       │    WasmInstance::on_hook()                                   │
│       │         │ spawn_blocking                                     │
│       │         ▼                                                    │
│       │    __uncode_on_hook(ctx_ptr, ctx_len, out_ptr)               │
│       │    返回 HookResult (Continue / Block / Modify)               │
│       │                                                              │
│       └──── 工具路径:                                                │
│            ExtensionApi.register_tool()                              │
│                 │                                                    │
│                 ▼                                                    │
│            ToolRegistry → ExtensionToolExecutor                      │
│                 │                                                    │
│                 ▼                                                    │
│            WasmExtensionTool::execute()                              │
│                 │ spawn_blocking                                     │
│                 ▼                                                    │
│            __uncode_tool_execute(name, args, out)                    │
│            返回 JSON 结果字符串                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### 模块结构

```
crates/uncode-extensions/src/wasm/
├── mod.rs             # WasmError 错误类型 + 常量默认值
├── engine.rs          # WasmEngine — Engine/Linker/工具收集
├── instance.rs        # WasmInstance — Extension trait 适配器
├── tool.rs            # WasmExtensionTool — ExtensionTool trait 适配器
├── host_imports.rs    # 宿主导入函数（WASM 模块可调用的宿主能力）
├── memory.rs          # HostState + WasmExports + 线性内存辅助
└── manifest.rs        # ExtensionManifest — JSON 伴生文件解析
```

---

## 3 双向通信 ABI

WASM 模块与宿主之间通过**扁平 ABI** 通信：所有复杂数据以 UTF-8 JSON 编码，通过 `(指针, 长度)` 对在 WASM 线性内存中传递。

### 3.1 模块导出（宿主 → WASM）

WASM 模块**必须**导出以下 5 个函数和 1 个内存：

```
┌────────────────────────────────────────────────────────────────┐
│  WASM 模块导出                                                  │
├─────────────────────────┬──────────────────────────────────────┤
│  __uncode_init          │ (api_handle: i32) → ()               │
│                         │ 初始化入口。扩展在此调用宿主导入      │
│                         │ 函数注册钩子、工具、命令。            │
├─────────────────────────┼──────────────────────────────────────┤
│  __uncode_on_hook       │ (ctx_ptr, ctx_len, out_ptr: i32)     │
│                         │   → i32                              │
│                         │ 钩子回调。ctx 指向 JSON HookContext， │
│                         │ 返回 out 缓冲区中结果 JSON 长度。    │
│                         │ 返回 0 = Continue。                  │
├─────────────────────────┼──────────────────────────────────────┤
│  __uncode_tool_execute  │ (name_ptr, name_len, args_ptr,       │
│                         │  args_len, out_ptr: i32) → i32       │
│                         │ 工具执行回调。                       │
├─────────────────────────┼──────────────────────────────────────┤
│  __uncode_allocate      │ (size: i32) → i32                    │
│                         │ 在 WASM 线性内存中分配空间。         │
├─────────────────────────┼──────────────────────────────────────┤
│  __uncode_deallocate    │ (ptr, size: i32) → ()                │
│                         │ 释放已分配的内存。                   │
├─────────────────────────┼──────────────────────────────────────┤
│  memory                 │ WASM 线性内存（最少 1 页 = 64KB）    │
└─────────────────────────┴──────────────────────────────────────┘
```

### 3.2 宿主导入（WASM → 宿主）

WASM 模块通过 `uncode` 模块命名空间导入宿主函数：

```
┌────────────────────────────────────────────────────────────────┐
│  宿主导入（模块名: "uncode"）                                    │
├──────────────────────────────────┬─────────────────────────────┤
│  __uncode_host_register_hook     │ (handle, ptr, len: i32)     │
│                                  │ 注册生命周期钩子。          │
│                                  │ ptr 指向 UTF-8 钩子名称。   │
├──────────────────────────────────┼─────────────────────────────┤
│  __uncode_host_register_tool     │ (handle, meta_ptr,          │
│                                  │  meta_len: i32) → i32       │
│                                  │ 注册 LLM 可调用工具。       │
│                                  │ meta 为 JSON ExtensionTool  │
│                                  │ Metadata。返回 tool_id。    │
├──────────────────────────────────┼─────────────────────────────┤
│  __uncode_host_register_command  │ (handle, cmd_ptr,           │
│                                  │  cmd_len: i32)              │
│                                  │ 注册斜杠命令。              │
│                                  │ cmd 为 JSON Command         │
│                                  │ Registration。              │
├──────────────────────────────────┼─────────────────────────────┤
│  __uncode_host_register_shortcut │ (handle, sc_ptr,            │
│                                  │  sc_len: i32)               │
│                                  │ 注册键盘快捷键。            │
│                                  │ sc 为 JSON Shortcut         │
│                                  │ Registration。              │
├──────────────────────────────────┼─────────────────────────────┤
│  __uncode_host_log               │ (level, msg_ptr,            │
│                                  │  msg_len: i32)              │
│                                  │ 输出日志。                  │
│                                  │ level: 0=trace, 1=debug,    │
│                                  │ 2=info, 3=warn, 4+=error。  │
├──────────────────────────────────┼─────────────────────────────┤
│  __uncode_host_get_cwd           │ (out_ptr: i32) → i32        │
│                                  │ 获取当前工作目录。          │
└──────────────────────────────────┴─────────────────────────────┘
```

### 3.3 数据传输协议

所有复杂数据通过 WASM 线性内存的共享缓冲区传递，流程如下：

```
宿主写入 → WASM 读取                    WASM 写入 → 宿主读取
─────────────────────                  ─────────────────────

1. 宿主调用 __uncode_allocate(len)      1. WASM 写入 out_ptr 缓冲区
   ↓ 返回 ptr                          2. WASM 返回写入长度 n
2. 宿主拷贝数据到 memory[ptr..ptr+len]  3. 宿主读取 memory[out_ptr..out_ptr+n]
3. 宿主调用导出函数(ptr, len, ...)      4. 宿主调用 __uncode_deallocate
4. 宿主调用 __uncode_deallocate(ptr)
```

**JSON 编码规则**：
- `HookContext`：`{"session_id": "sess-xxx"}`
- `HookResult`：`{"type": "continue"}` / `{"type": "block", "reason": "..."}` / `{"type": "modify", ...}`
- `ExtensionToolMetadata`、`CommandRegistration`、`ShortcutRegistration`：各自的 serde JSON 序列化

---

## 4 加载流程

```
~/.uncode/extensions/
├── hello.wasm          ← WASM 二进制
└── hello.json          ← 可选的伴生清单
```

### 4.1 清单格式（ExtensionManifest）

```json
{
  "name": "hello-world",
  "version": "0.1.0",
  "description": "示例扩展",
  "hooks": ["session_start", "turn_start"],
  "permissions": {
    "filesystem": false,
    "network": false
  },
  "memory_limit_mb": 64,
  "fuel_limit": 10000000,
  "timeout_secs": 5
}
```

如果 `.json` 清单不存在，使用默认值：
- `name`：从 `.wasm` 文件名派生
- `hooks`：所有钩子
- `memory_limit_mb`：64
- `fuel_limit`：10,000,000
- `timeout_secs`：5

### 4.2 实例化序列

```
ExtensionLoader::load_from_dir()
  │
  ├─ 扫描 *.wasm 文件
  │
  ├─ 对每个 .wasm:
  │   │
  │   ├─ ExtensionManifest::load()        ← 解析伴生 JSON
  │   │
  │   ├─ WasmEngine::instantiate()
  │   │   │
  │   │   ├─ Module::from_binary()        ← Cranelift 编译
  │   │   │
  │   │   ├─ Store::new(HostState)        ← 创建独立 Store
  │   │   │   └─ set_fuel(limit)          ← 设置 CPU fuel
  │   │   │
  │   │   ├─ Linker::instantiate()        ← 连接宿主导入函数
  │   │   │
  │   │   ├─ WasmExports::from_instance() ← 校验必须的导出
  │   │   │   └─ 缺少任何导出 → WasmError::MissingExport
  │   │   │
  │   │   ├─ __uncode_init(1)             ← 调用扩展初始化
  │   │   │   ├─ 扩展调用 host_register_hook
  │   │   │   └─ 扩展调用 host_register_tool
  │   │   │
  │   │   ├─ 收集 registered_hooks        ← 从 HostState 取出
  │   │   ├─ 收集 registered_tools        ← 从 HostState 取出
  │   │   │
  │   │   └─ 返回 (WasmInstance, Vec<WasmExtensionTool>)
  │   │       └─ 工具共享 instance 的 inner Arc<Mutex>
  │   │
  │   ├─ HookRegistry::register()         ← 注册到全局钩子表
  │   │
  │   ├─ ExtensionApi::register_tool()    ← 注册每个 WASM 工具
  │   │   └─ ToolRegistrationCallback     ← CLI 注入的回调
  │   │
  │   └─ 失败 → tracing::warn! + 继续下一个
  │
  └─ 返回成功加载数量
```

**错误隔离原则**：单个扩展加载失败不会影响其他扩展或主流程。

---

## 5 沙箱机制

### 5.1 内存隔离

每个 WASM 扩展拥有独立的 wasmtime `Store`，其线性内存与其他扩展和宿主完全隔离。扩展无法访问宿主进程的内存。

```
宿主进程内存
├── uncode-agent        ← Rust 原生
├── uncode-tui          ← Rust 原生
└── wasmtime 运行时
    ├── Store A (hello.wasm)   ← 独立线性内存
    ├── Store B (foo.wasm)     ← 独立线性内存
    └── Store C (bar.wasm)     ← 独立线性内存
```

### 5.2 CPU 限制（Fuel）

wasmtime 的 fuel 机制为每条 WASM 指令消耗一定 fuel。当 fuel 耗尽时，执行被中断并返回 trap。

```rust
// engine.rs — 创建时启用 fuel
config.consume_fuel(true);

// instance.rs — 每次 hook 调用前重置
store.set_fuel(10_000_000);
```

默认 fuel 限制：**10,000,000 指令/hook 调用**（约等于数十毫秒的 CPU 时间）。

### 5.3 执行超时

WASM 调用在 `spawn_blocking` 线程中执行（wasmtime 是同步 API），超时由宿主侧的 tokio 异步机制保证：

```rust
// instance.rs
tokio::task::spawn_blocking(move || call_on_hook(&inner, &ctx_bytes))
    .await
    .map_err(|_| WasmError::Timeout(self.timeout))?
```

默认超时：**5 秒**。

### 5.4 能力控制（无 WASI）

uncode 的 WASM 运行时**不启用 WASI**。扩展没有：

- 文件系统访问
- 网络访问
- 标准输入/输出
- 环境变量
- 系统时间（高精度）
- 线程

扩展的所有能力都必须通过宿主导入函数显式获取：

```
WASM 扩展可做的：
  ✓ 注册钩子（on session_start, turn_end...）
  ✓ 注册工具（LLM 可调用）
  ✓ 注册命令和快捷键
  ✓ 通过宿主日志系统输出
  ✗ 直接读写文件
  ✗ 发起网络请求
  ✗ 访问环境变量
  ✗ 创建线程
```

### 5.5 容错与自动禁用

当 WASM 扩展 trap（崩溃、超时、fuel 耗尽）时，实例被标记为 `disabled`：

```rust
// instance.rs
Err(e) => {
    tracing::warn!("WASM extension {ext_name} trapped: {e}");
    *disabled = true;
    Ok(HookResult::Continue)  // 降级为静默跳过
}
```

后续所有对该扩展的 `on_hook` 调用直接返回 `Continue`，不再进入 WASM 运行时。

---

## 6 运行时调用路径

以 `session_start` 钩子为例，完整调用链：

```
AgentLoop::run()
  │
  ├─ ExtensionLifecycleBridge::fire_session_start(session_id)
  │    │
  │    └─ HookRegistry::fire(SessionStart, ctx)
  │         │
  │         └─ 对每个注册了 SessionStart 的扩展：
  │              │
  │              ├─ [内置扩展] ext.on_hook(ctx).await
  │              │    └─ 直接调用 Rust 实现
  │              │
  │              └─ [WASM 扩展] WasmInstance::on_hook(ctx).await
  │                   │
  │                   ├─ 序列化 HookContext → JSON
  │                   │
  │                   ├─ spawn_blocking → call_on_hook()
  │                   │    │
  │                   │    ├─ Mutex lock
  │                   │    ├─ 检查 disabled 标志
  │                   │    │
  │                   │    ├─ __uncode_allocate(len) → ctx_ptr
  │                   │    ├─ 拷贝 JSON 到 WASM 线性内存
  │                   │    ├─ __uncode_allocate(1024) → out_ptr
  │                   │    ├─ 重置 fuel = 10,000,000
  │                   │    │
  │                   │    ├─ __uncode_on_hook(ctx_ptr, len, out_ptr)
  │                   │    │    └─ [WASM 内部执行]
  │                   │    │
  │                   │    ├─ __uncode_deallocate(ctx_ptr, len)
  │                   │    │
  │                   │    └─ 读取 out_ptr 处的结果 JSON
  │                   │         └─ parse_hook_result(json)
  │                   │              ├─ {"type":"continue"} → Continue
  │                   │              ├─ {"type":"block",...} → Block
  │                   │              └─ {"type":"modify",...} → Modify
  │                   │
  │                   └─ 返回 HookResult
  │                        │
  │                        ├─ Continue → 继续下一个扩展
  │                        ├─ Block → 停止，返回给调用者
  │                        └─ Modify → 继续但带修改数据
  │
  └─ 钩子执行完毕，AgentLoop 继续
```

### Rust 借用问题的解决

`WasmInstance` 内部的 `wasmtime::Store` 需要 `&mut` 才能操作，但 `Extension::on_hook(&self)` 只提供 `&self`。解决方案：

1. **`std::sync::Mutex`** 包裹 `Store` + `exports`
2. **解构模式借用**：`let WasmInstanceInner { ref mut store, ref exports, .. } = *guard;` 让 Rust 编译器理解 `store` 和 `exports` 是不同字段
3. **`spawn_blocking`**：wasmtime 是同步 API，在 tokio 的阻塞线程池中执行，不占用异步工作线程

```rust
// 解构避免同时可变/不可变借用的编译错误
let WasmInstanceInner {
    ref mut store,   // &mut Store
    ref exports,     // &WasmExports
    ref mut disabled, // &mut bool
} = *guard;

// 现在可以同时使用 exports（不可变）和 store（可变）
exports.allocate.call(&mut *store, (size,))?;
```

---

## 7 工具执行调用路径

WASM 扩展注册的工具被 LLM 调用时，完整链路：

```
LLM 返回 tool_call: "hello_greet"
  │
  └─ AgentLoop → ToolRegistry::execute("hello_greet", args)
       │
       └─ ExtensionToolExecutor::execute()
            │  ← 适配器：ExtensionTool → ToolExecutor trait
            │
            └─ WasmExtensionTool::execute(args)
                 │
                 ├─ serde_json::to_vec(&arguments)    ← 序列化参数
                 │
                 ├─ spawn_blocking(move || { ... })
                 │    │
                 │    ├─ Mutex lock                    ← 与 hook 共享同一把锁
                 │    ├─ 检查 disabled 标志
                 │    │
                 │    ├─ __uncode_allocate(name_len)   ← 分配工具名空间
                 │    ├─ 拷贝 name → WASM 线性内存
                 │    ├─ __uncode_allocate(args_len)   ← 分配参数空间
                 │    ├─ 拷贝 args → WASM 线性内存
                 │    ├─ __uncode_allocate(4096)        ← 分配输出缓冲区
                 │    ├─ 重置 fuel = 10,000,000
                 │    │
                 │    ├─ __uncode_tool_execute(
                 │    │    name_ptr, name_len,
                 │    │    args_ptr, args_len,
                 │    │    out_ptr)
                 │    │  └─ [WASM 内部执行]
                 │    │
                 │    ├─ __uncode_deallocate(name_ptr)  ← 回收输入缓冲区
                 │    ├─ __uncode_deallocate(args_ptr)
                 │    │
                 │    └─ 读取 memory[out_ptr..out_ptr+n]
                 │         └─ UTF-8 → String
                 │
                 └─ 返回 String 给 LLM
```

### 关键设计：Arc<Mutex> 共享

`WasmInstance` 和 `WasmExtensionTool` 共享同一个 `Arc<Mutex<WasmInstanceInner>>`：

```rust
// engine.rs — 实例化后共享 inner
let instance = WasmInstance::new(name, store, exports, hooks, timeout);
let inner = instance.inner_clone();  // Arc clone
let tools: Vec<WasmExtensionTool> = tool_metas
    .into_iter()
    .map(|meta| WasmExtensionTool::new(meta, inner.clone()))
    .collect();
```

这保证同一扩展的 hook 调用和 tool 调用互斥——不会并发访问 wasmtime Store。代价是 hook 和 tool 不能并行执行，但对扩展场景足够（hook 是瞬时的，tool 是按需的）。

---

## 8 Hello-World 示例端到端分析

> **源码**：`examples/extension-hello/src/lib.rs`
> **构建**：`cargo build --release --target wasm32-unknown-unknown`
> **产物**：7.8KB WASM 二进制（`uncode_ext_hello.wasm`）

### 8.1 扩展做了什么

hello-world 是一个完整的端到端示例，演示 WASM 扩展能做的两件事：

1. **注册生命周期钩子** — `session_start` 时打日志
2. **注册 LLM 可调用工具** — `hello_greet` 工具，接受 `{"name": "Alice"}`，返回问候语

### 8.2 初始化阶段

```rust
#[unsafe(no_mangle)]
pub extern "C" fn __uncode_init(api_handle: i32) {
    // 1. 注册钩子
    __uncode_host_register_hook(api_handle, "session_start", 13);

    // 2. 注册工具（JSON 元数据）
    __uncode_host_register_tool(api_handle, TOOL_META_JSON, len);
}
```

工具元数据 JSON 常量：

```json
{
  "name": "hello_greet",
  "description": "Generate a greeting from the hello-world WASM extension. ...",
  "parameters": {
    "type": "object",
    "properties": {
      "name": { "type": "string", "description": "The name to greet" }
    },
    "required": ["name"]
  },
  "sequential": false
}
```

这段 JSON 被传递给宿主的 `__uncode_host_register_tool`，宿主解析为 `ExtensionToolMetadata`，校验后存入 `HostState.registered_tools`。引擎实例化完成后从中取出，创建 `WasmExtensionTool`。

### 8.3 钩子回调

```rust
#[unsafe(no_mangle)]
pub extern "C" fn __uncode_on_hook(
    _ctx_ptr: i32, _ctx_len: i32, _out_ptr: i32,
) -> i32 {
    host_log(2, "hello-world: session_start 钩子已触发");
    0  // Continue — 不拦截
}
```

每次新会话启动时，`AgentLoop` → `HookRegistry::fire(SessionStart)` → `WasmInstance::on_hook()` → `__uncode_on_hook`。返回 0 表示放行。

### 8.4 工具执行回调

这是端到端链路的核心。当 LLM 决定调用 `hello_greet` 工具时：

```rust
#[unsafe(no_mangle)]
pub extern "C" fn __uncode_tool_execute(
    name_ptr: i32, name_len: i32,
    args_ptr: i32, args_len: i32,
    out_ptr: i32,
) -> i32 {
    // 1. 从 WASM 线性内存读取工具名和参数
    let name = slice_from_raw(name_ptr, name_len);  // "hello_greet"
    let args = slice_from_raw(args_ptr, args_len);  // {"name":"Alice"}

    // 2. 校验工具名
    if name != b"hello_greet" { return 0; }

    // 3. 从 JSON 参数中提取 name 字段
    let greet_name = extract_json_string(args, b"name")
        .unwrap_or(b"world");                        // "Alice"

    // 4. 拼接结果写入 out_ptr
    // {"result":"Hello, Alice! Greetings from uncode WASM extension."}
    write_to(out_ptr, prefix);
    write_to(out_ptr + offset, greet_name);
    write_to(out_ptr + offset, suffix);

    // 5. 返回结果字节长度
    total_len
}
```

### 8.5 no_std JSON 解析

hello-world 在 `#![no_std]` 环境下无法使用 `serde_json`。它实现了一个极简的 JSON 字符串提取器：

```rust
/// 从 JSON 字节中提取 "key":"value" 的 value 部分
fn extract_json_string<'a>(json: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    // 扫描 "key" 模式 → 跳过 : 和空白 → 提取引号内的值
    // 纯字节级操作，无堆分配
}
```

这仅用于演示。生产扩展可以选择：
- 编译时链接 `serde_json`（需要 `alloc`）
- 使用 `uncode-ext-sdk`（未来方向）

### 8.6 内存管理

```
┌─────────────────────────────────────────────┐
│  WASM 线性内存 (64KB)                       │
│                                             │
│  ┌─────────┐  BumpAlloc: 只增不减           │
│  │ 数据段  │  OFFSET ↑                      │
│  ├─────────┤                                │
│  │ 堆      │  ← __uncode_allocate 从此分配  │
│  │  ↑      │  每次分配 OFFSET += aligned    │
│  │ ...     │  不支持释放（bump allocator）   │
│  │         │                                │
│  │ 空闲    │  ~60KB 可用                     │
│  └─────────┘                                │
└─────────────────────────────────────────────┘
```

bump allocator 对于 init 阶段的有限分配足够。每次 hook/tool 调用中分配的内存不会被复用，但 WASM 线性内存的上限（默认 64MB）远超单次调用所需。

### 8.7 完整端到端流程图

```
用户启动 uncode CLI
  │
  ├─ ExtensionLoader::load_from_dir(~/.uncode/extensions/)
  │    │
  │    ├─ 发现 hello.wasm → WasmEngine::instantiate()
  │    │    │
  │    │    ├─ 编译 + 创建 Store + 设置 fuel
  │    │    │
  │    │    ├─ __uncode_init(1)
  │    │    │    ├─ host_register_hook("session_start")  → HostState.hooks
  │    │    │    └─ host_register_tool(metadata_json)    → HostState.tools
  │    │    │
  │    │    ├─ drain HostState → (WasmInstance, [WasmExtensionTool])
  │    │    │
  │    │    └─ 返回 (instance, [tool])
  │    │
  │    ├─ HookRegistry.register(instance, [SessionStart])
  │    └─ ExtensionApi.register_tool(tool)
  │         └─ ToolRegistrationCallback → ToolRegistry
  │
  ├─ 用户输入: "跟 Alice 打个招呼"
  │    │
  │    ├─ LLM 流式响应 → tool_call: hello_greet({"name":"Alice"})
  │    │
  │    ├─ AgentLoop → ToolRegistry → ExtensionToolExecutor
  │    │    │
  │    │    └─ WasmExtensionTool::execute({"name":"Alice"})
  │    │         │
  │    │         └─ spawn_blocking → __uncode_tool_execute(...)
  │    │              └─ extract "Alice" → 拼接问候语
  │    │                   → {"result":"Hello, Alice! Greetings from ..."}
  │    │
  │    └─ LLM 收到工具结果 → 生成回复
  │
  └─ 用户看到: "我已经通过扩展向 Alice 打了招呼！..."
```

---

## 9 依赖与构建配置

### 9.1 Feature Gate

wasmtime 编译时间较长（含 Cranelift JIT 编译器）。通过 feature gate 允许下游 crate 按需启用：

```toml
# crates/uncode-extensions/Cargo.toml
[dependencies]
wasmtime = { workspace = true, optional = true }

[features]
default = ["wasm"]
wasm = ["wasmtime"]
```

不依赖 WASM 运行时的 crate（如 `uncode-tui`）只需使用类型定义，无需编译 wasmtime。

### 9.2 wasmtime 配置

```rust
// engine.rs
let mut config = wasmtime::Config::new();
config.consume_fuel(true);                    // CPU 限制
config.strategy(wasmtime::Strategy::Cranelift); // JIT 编译器
```

未启用的特性：
- `component-model` — 组件模型（Phase 4+ 可选）
- `wasi` — WASI 预览版（无文件/网络需求）
- `pooling-allocator` — 实例池化（扩展数量少时无需）

---

## 10 扩展开发指南

### 10.1 最小扩展模板

```rust
// lib.rs — 编译目标: wasm32-unknown-unknown
#![no_std]

#[global_allocator]
static ALLOC: MyAlloc = MyAlloc;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }

unsafe extern "C" {
    fn __uncode_host_register_hook(handle: i32, ptr: *const u8, len: i32);
    fn __uncode_host_log(level: i32, ptr: *const u8, len: i32);
}

#[unsafe(no_mangle)]
pub extern "C" fn __uncode_init(handle: i32) {
    // 在此注册钩子、工具、命令
}

#[unsafe(no_mangle)]
pub extern "C" fn __uncode_on_hook(_: i32, _: i32, _: i32) -> i32 {
    0 // Continue
}

#[unsafe(no_mangle)]
pub extern "C" fn __uncode_tool_execute(_: i32, _: i32, _: i32, _: i32, _: i32) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn __uncode_allocate(size: i32) -> i32 { /* ... */ }

#[unsafe(no_mangle)]
pub extern "C" fn __uncode_deallocate(_: i32, _: i32) {}
```

### 10.2 构建与部署

```bash
# 构建
cd my-extension/
cargo build --release --target wasm32-unknown-unknown

# 部署
cp target/wasm32-unknown-unknown/release/my_ext.wasm \
   ~/.uncode/extensions/my_ext.wasm

# 可选：创建伴生清单
cat > ~/.uncode/extensions/my_ext.json << 'EOF'
{
  "name": "my-extension",
  "version": "1.0.0",
  "hooks": ["session_start", "turn_end"],
  "timeout_secs": 10
}
EOF
```

---

## 11 生命周期钩子一览

| 钩子名称 | LifecycleHook 枚举 | 触发时机 |
|:---|:---|:---|
| `session_start` | `SessionStart` | 新会话创建 |
| `turn_start` | `TurnStart` | 每轮对话开始 |
| `message_received` | `MessageReceived` | 收到用户消息 |
| `message_sending` | `MessageSending` | 即将发送 LLM 消息 |
| `tool_call_before` | `ToolCallBefore` | 工具执行前（可拦截） |
| `tool_call_after` | `ToolCallAfter` | 工具执行后 |
| `turn_end` | `TurnEnd` | 每轮对话结束 |
| `session_end` | `SessionEnd` | 会话关闭 |

**拦截语义**（HookResult）：

- **Continue**：放行，继续执行后续扩展
- **Block { reason }**：阻断操作，返回原因
- **Modify(modification)**：放行但修改传输数据

首个返回 `Block` 或 `Modify` 的扩展终止后续扩展的调用。

---

## 12 未来方向

| 方向 | 说明 | 优先级 |
|:---|:---|:---|
| WASI 支持 | 允许扩展在沙箱内访问受限文件系统 | 中 |
| Component Model | 用 WIT 定义类型化接口，替代扁平 ABI | 低 |
| 扩展发现与热重载 | 监听 `~/.uncode/extensions/` 目录变化，运行时加载/卸载 | 中 |
| 扩展 SDK | 提供 `uncode-ext-sdk` crate，封装 ABI 细节 | 中 |
| 内存池化 | wasmtime `pooling-allocator`，支持大量扩展实例 | 低 |

---

## 参考源码

| 文件 | 职责 |
|:---|:---|
| `crates/uncode-extensions/src/wasm/engine.rs` | WasmEngine — Engine/Linker/工具收集 |
| `crates/uncode-extensions/src/wasm/instance.rs` | WasmInstance — Extension trait 适配器 |
| `crates/uncode-extensions/src/wasm/tool.rs` | WasmExtensionTool — ExtensionTool trait 适配器 |
| `crates/uncode-extensions/src/wasm/host_imports.rs` | 宿主导入函数定义 |
| `crates/uncode-extensions/src/wasm/memory.rs` | HostState + WasmExports + 内存操作 |
| `crates/uncode-extensions/src/wasm/manifest.rs` | ExtensionManifest JSON 解析 |
| `crates/uncode-extensions/src/loader.rs` | ExtensionLoader — 目录扫描、加载、工具注册 |
| `crates/uncode-cli/src/main.rs` | CLI 入口 — 启动时加载扩展 |
| `examples/extension-hello/` | Hello-world 端到端示例扩展（含工具注册） |
