# 工具系统指南

## 概述

UnCode 的工具系统是 Agent 与本地环境交互的唯一通道，遵循三层架构：

```
uncode-core  →  定义 trait、类型
uncode-tools →  实现 8 个内置工具 + 注册表
uncode-agent →  调度执行（查找 → 执行 → 事件广播）
uncode-tui   →  权限拦截 + 渲染展示
```

Agent 不直接访问文件系统或执行命令，而是通过 LLM 生成工具调用 → 注册表分发 → 工具执行 → 结果返回 → LLM 继续推理的循环完成所有操作。

---

## 工具执行生命周期

```
1. Agent 构建请求，携带所有工具的 JSON Schema 给 LLM
2. LLM 决定调用哪个工具，生成 ToolCall { name, arguments }
3. Agent 从注册表按 name 查找 ToolExecutor
4. 权限系统检查是否需要用户确认
5. ToolExecutor::execute(arguments) 执行
6. 结果（成功/失败）以文本形式返回给 LLM
7. LLM 根据结果决定继续调用工具还是回复用户
```

每轮最多调用 50 次工具，防止无限循环。

---

## 8 个内置工具

### read — 读取文件或目录

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `path` | string | 是 | 文件或目录路径 |
| `offset` | integer | 否 | 从第几行开始（0 起始） |
| `limit` | integer | 否 | 最多读取多少行 |

- 目录自动列出内容（目录后缀 `/`）
- 文件最大 1MB，超限返回错误
- 输出格式：`    42: 代码行内容`

### write — 写入文件

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `path` | string | 是 | 目标文件路径 |
| `content` | string | 是 | 要写入的内容 |

- 自动创建不存在的父目录
- 覆盖写入（非追加）

### edit — 精确替换

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `path` | string | 是 | 目标文件 |
| `old_string` | string | 是 | 要被替换的文本 |
| `new_string` | string | 是 | 替换后的文本 |

- `old_string` 必须在文件中恰好出现 **1 次**，0 次或多于 1 次均报错

### grep — 正则搜索

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `pattern` | string | 是 | 正则表达式 |
| `path` | string | 否 | 搜索目录 |
| `include` | string | 否 | 文件过滤，如 `"*.rs"` |

- 最大深度 20 层
- 结果上限 50 条

### find — 按模式查找文件

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `pattern` | string | 是 | glob 模式，如 `"**/*.rs"` |
| `path` | string | 否 | 搜索根目录 |

- 结果上限 200 条

### ls — 列出目录

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `path` | string | 否 | 目录路径，默认当前目录 |

- 结果上限 500 条，按字母排序

### bash — 执行 Shell 命令

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `command` | string | 是 | 要执行的命令 |
| `workdir` | string | 否 | 工作目录 |
| `timeout` | integer | 否 | 超时秒数，默认 120 |

- 通过 `sh -c` 执行
- 返回 stdout、stderr 和 exit code

---

## 权限系统

工具执行前需通过权限检查，分为三级：

### 自动允许（无需确认）

以下工具默认识别为只读，自动放行：

| 工具 | 分类 |
|------|------|
| read | 只读 |
| grep | 只读 |
| find | 只读 |
| ls | 只读 |

### Bash 安全白名单

`bash` 工具在以下情况自动放行：

- 命令在前 25 个安全命令白名单内
- 白名单包含：`ls`、`cat`、`head`、`tail`、`find`、`grep`、`git status`、`git log`、`git diff`、`cargo check`、`cargo test`、`cargo build`、`pwd`、`echo`、`which`、`env`、`wc`、`sort`、`uniq`、`diff`、`tree`、`rg`、`fd`、`cargo clippy`、`cargo fmt`

非白名单命令（如 `rm`、`git push`、`curl`）需用户确认。

### 需确认

以下工具始终需要用户确认：

| 工具 | 原因 |
|------|------|
| write | 写入文件 |
| edit | 修改文件 |
| bash（非白名单）| 执行风险命令 |

确认弹窗支持三种操作：

- `Y` / Enter — 允许执行
- `N` / Esc — 拒绝
- `E` — 编辑参数后执行

---

## 工具渲染

每个工具在 TUI 中有独立的渲染样式：

| 工具 | 执行中图标 | 完成后 | 特殊样式 |
|------|-----------|--------|---------|
| read | 闪烁 ●/○ | ● | 路径高亮 |
| write | 闪烁 ●/○ | ● | 路径高亮 |
| edit | 闪烁 ●/○ | ● | **diff 风格**（绿+红-/@@ 标题） |
| grep | 闪烁 ●/○ | ● | 匹配行显示 |
| bash | 闪烁 ●/○ | ●/✗ | `!shell` 前缀，stdout 输出 |
| find | 闪烁 ●/○ | ● | 文件列表 |
| ls | 闪烁 ●/○ | ● | 条目列表 |

---

## 扩展工具

UnCode 预留了 `#[tool]` 过程宏，用于未来开发自定义工具：

```rust
#[tool]
/// 读取指定 URL 的内容
async fn fetch_url(url: String, timeout: Option<u64>) -> String {
    // 实现
}
```

宏自动生成 `__tool_schema_fetch_url()` 函数返回 `ToolDefinition`，开发者只需实现 `ToolExecutor` trait 并注册到 `ToolRegistry` 即可。

---

## 架构约束

- **无直接文件访问**：Agent 不持有文件句柄，所有 I/O 通过工具
- **文本协议**：工具输入和输出均为纯文本（`String`），兼容所有 LLM
- **错误不中断**：工具失败返回错误文本，Agent 继续下一轮推理
- **线程安全**：`ToolExecutor: Send + Sync`，`ToolRegistry` 使用读写锁
- **分层解耦**：core 只定义 trait，tools 只提供实现，agent 只做调度，tui 只做拦截和展示
