# uncode 日志诊断指南

> 通过 tracing 日志观察 uncode 的运行时行为——模型切换、上下文注入、compaction、扩展加载等。

---

## 一、日志基础设施

### 1.1 配置

日志由 `tracing_subscriber::fmt()` 输出到 **stderr**（`crates/uncode-cli/src/main.rs:159-162`）。级别由 `RUST_LOG` 环境变量控制。

```bash
# 仅显示 agent 核心日志
RUST_LOG=uncode_agent=info cargo run -p uncode-cli -- ...

# 显示所有 crate 的 debug 日志
RUST_LOG=debug cargo run -p uncode-cli -- ...

# 按模块过滤
RUST_LOG=uncode_agent::loop_engine=debug,uncode_agent::compaction=info
```

### 1.2 TUI 模式下查看日志

TUI 占用终端，stderr 会干扰渲染。使用重定向：

```bash
# 启动 TUI，日志写入文件
RUST_LOG=uncode_agent=info cargo run -p uncode-cli -- --model deepseek-v3 2>/tmp/uncode.log

# 另一个终端实时查看
tail -f /tmp/uncode.log

# 过滤关键词
tail -f /tmp/uncode.log | grep "injected\|compaction\|model.switched"
```

---

## 二、关键日志点

### 2.1 会话生命周期

```
INFO  session imported: N entries
INFO  resources updated                    ← skills/mcp 加载完成
WARN  extension '{name}' failed to load    ← 扩展加载失败
INFO  loaded {} extension(s), {} error(s)
```

### 2.2 模型相关

```
INFO  dynamic provider '...' registered N model(s)   ← 动态注册
INFO  model switched: old → new                      ← AgentHarness::set_model()
```

### 2.3 每个 turn 的上下文注入（新增）

```
INFO  injected workspace context files=72 modules=8 modified=0
INFO  injected workspace context files=73 modules=8 modified=2
```

三个字段含义：
| 字段 | 含义 |
|:---|:---|
| `files` | InstanceRegistry 中已注册的 File 实例数 |
| `modules` | 推断出的 Module 数（目录数） |
| `modified` | 当前 session 中通过 write/edit 修改过的文件数 |

**如果始终为 0**：说明 InstanceRegistry 未正确初始化或 WorkspaceGraph 未构建（非 Rust 项目）。

### 2.4 工具执行和决策

```
DEBUG  Done event: reason=Stop, thinking=0, text=120, tool_calls=1, pending_executions=1
DEBUG  firewall flagged proposal {id}: {reason}      ← 工具被防火墙拒绝
WARN   hook blocked: {reason}                         ← hook 拦截
WARN   extension blocked context hook: {reason}       ← 扩展阻止了上下文注入
INFO   input handled by extension, skipping normal flow
INFO   input blocked by extension: {reason}
```

### 2.5 Compaction（上下文压缩）

```
INFO   compaction: summarized N messages -> M messages remaining
INFO   session compaction: summarized N entries, M tokens before
WARN   compaction cut_id not found in entries, skipping
```

### 2.6 演化引擎

```
INFO   evolution engine detected N mutation suggestion(s)
DEBUG  suggested: {...}
```

---

## 三、诊断场景

### 场景 1：确认上下文注入生效

```bash
RUST_LOG=uncode_agent=info cargo run -p uncode-cli -- ... 2>/tmp/uncode.log
# 对项目说 "hello"
# 日志应出现 injected workspace context，files > 0
```

### 场景 2：追踪模型切换

```bash
RUST_LOG=uncode_agent=info cargo run -p uncode-cli -- ... 2>/tmp/uncode.log
# 使用 /model 命令切换
# 日志应出现 model switched: gpt-4o -> deepseek-v3
```

### 场景 3：分析 turn 内工具调用

```bash
RUST_LOG=uncode_agent=debug cargo run -p uncode-cli -- ... 2>/tmp/uncode.log
# Debug 级别输出 Done event，包含 thinking/tool_calls 统计
tail -f /tmp/uncode.log | grep "Done event"
```

### 场景 4：诊断 compaction 行为

```bash
RUST_LOG=uncode_agent=debug cargo run -p uncode-cli -- ... 2>/tmp/uncode.log
tail -f /tmp/uncode.log | grep "compaction\|compact"
```

### 场景 5：追踪扩展加载和错误

```bash
RUST_LOG=uncode_cli=info cargo run -p uncode-cli -- ... 2>/tmp/uncode.log
tail -f /tmp/uncode.log | grep "extension"
```

---

## 四、日志级别指南

| 级别 | 内容 | 典型用途 |
|:---|:---|:---|
| `error` | 致命错误（当前无生产调用） | — |
| `warn` | mutex 中毒恢复、hook 阻断、路径异常 | 排查异常行为 |
| `info` | 注入统计、模型切换、compaction 摘要、扩展加载 | **日常诊断** |
| `debug` | turn 内事件详情、防火墙拒绝理由、演化建议 | 深入追踪 |
| `trace` | 当前无调用 | — |

**建议**：日常使用 `RUST_LOG=uncode_agent=info`，排查问题时切到 `debug`。

---

## 五、日志点分布

| Crate | info | debug | warn | 总计 |
|:---|:---:|:---:|:---:|:---:|
| `uncode-agent` | 9 | 7 | 15 | 31 |
| `uncode-cli` | 4 | 0 | 1 | 5 |
| `uncode-ai` | 0 | 0 | 1 | 1 |
| `uncode-ontology` | 0 | 0 | 0 | 0 |
| `uncode-core` | 0 | 0 | 0 | 0 |
| `uncode-tui` | 0 | 0 | 0 | 0 |

`uncode-agent` 集中了 84% 的日志点，`loop_engine.rs` 是核心（11 条）。

---

## 六、相关文件

| 文件 | 用途 |
|:---|:---|
| `crates/uncode-cli/src/main.rs:159-162` | tracing subscriber 初始化 |
| `crates/uncode-agent/src/loop_engine.rs:1186` | 上下文注入日志（新增） |
| `crates/uncode-agent/src/compaction.rs:99,301` | compaction 日志 |
| `crates/uncode-agent/src/model_switch.rs:27` | 模型切换日志 |
| `crates/uncode-agent/src/harness.rs:365` | 资源更新日志 |
