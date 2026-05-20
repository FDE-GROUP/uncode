# OpenCode 工具系统

> ToolRegistry、内置工具、MCP、Plugin 与 Permission

---

## 1. 架构

```
SessionPrompt
    → ToolRegistry.tools({ providerID, modelID, agent })
        → builtin[] + custom[] + MCP 转换的工具
        → 包装为 AI SDK tool()
    → SessionProcessor 执行
        → Permission 规则集校验
        → Plugin 钩子
        → 结果写入 MessageV2 ToolPart
```

**文件锚点**：

- `tool/registry.ts` — 注册与聚合
- `tool/tool.ts` — 工具定义基类
- `session/prompt.ts` — 注入 LLM

---

## 2. ToolRegistry

**Service 接口**（`registry.ts`）：

| 方法 | 说明 |
|------|------|
| `ids()` | 全部工具 ID |
| `all()` | 全部 `Tool.Def` |
| `named()` | 具名工具（如 `task`、`read`） |
| `tools(model, agent)` | 按模型与 Agent 过滤后的可调用集 |

**状态**：

- `builtin`：仓库内置
- `custom`：配置/目录加载
- 依赖 **Flag** 开关（如 Exa/Parallel 搜索）

---

## 3. 内置工具（节选）

| 工具 | 文件 | 说明 |
|------|------|------|
| read / write / edit | `read.ts`, `write.ts`, `edit.ts` | 文件读写改 |
| grep / glob | `grep.ts`, `glob.ts` | 搜索 |
| shell | `shell.ts` | 命令执行（含 Shell 事件） |
| task | `task.ts` | **子 Agent 会话** |
| todo | `todo.ts` | 会话待办 |
| question | `question.ts` | 向用户提问 |
| plan | `plan.ts` | plan 模式进出 |
| webfetch / websearch | `webfetch.ts`, `websearch.ts` | 网络 |
| codesearch | `codesearch.ts` | 语义代码搜索 |
| apply_patch | `apply_patch.ts` | 补丁应用 |
| lsp | `lsp.ts` | LSP 查询 |
| skill | `skill.ts` | 技能调用 |
| repo_clone / repo_overview | `repo_*.ts` | 仓库操作 |
| invalid | `invalid.ts` | 占位/错误工具 |

**与 Pi 默认四件套**：OpenCode **明显更宽**，平台化工具集；Pi 倾向 read/write/edit/bash + 扩展。

---

## 4. MCP

**目录**：`packages/opencode/src/mcp/`

- CLI：`opencode mcp` 子命令
- 配置化 MCP 服务器列表
- 在 **SessionPrompt** 路径并入工具列表（与 `ToolRegistry` 协同）
- HTTP handlers 暴露 MCP 管理（server 路由）

**哲学对照**：OpenCode **MCP 一等公民**；Pi 文档建议 Skills/CLI 替代 MCP 主路径。

---

## 5. Plugin（@opencode-ai/plugin）

- 第三方声明 **ToolDefinition**（JSON Schema 参数）
- **TUI 钩子**（渲染、快捷键等）
- `Plugin` Service 在 Processor/Prompt 层注入

加载路径与 `opencode.json` / 项目 `.opencode` 配置相关。

---

## 6. Permission

**模块**：`permission/`

- `Permission.Ruleset` 挂在 **Agent** 与 **session** 行上。
- **plan** Agent 默认更严格（只读倾向）。
- 工具执行前 consult Permission；拒绝时走 `tool.failed` 事件。

无 Pi 式 **Hook 返回值 patch**；策略以 **规则集 + 产品 UI** 为主。

---

## 7. 工具输出与截断

- `tool/truncate.ts`：输出长度限制
- `Truncate` 服务在 Agent 生成标题/摘要时复用
- Shell 输出关联 `session.next.shell.*` 事件

---

## 8. 与 uncode 工具对照

| 维度 | OpenCode | uncode |
|------|----------|--------|
| 定义方式 | TS + Zod/JSON Schema + Plugin | `#[tool]` 宏 + `ToolExecutor` |
| 沙箱 | ExecutionEnv / 项目目录 + Permission | `resolve_path` + CWD |
| 默认数量 | 十余内置 + MCP | CLI 默认 7，可注册 9 |
| 子 Agent | TaskTool 内置 | 未内建同等 Task 会话 |

---

## 相关文档

- [OPENCODE_LOOP_ENGINE.md](OPENCODE_LOOP_ENGINE.md)
- [OPENCODE_AGENT_ARCHITECTURE.md](OPENCODE_AGENT_ARCHITECTURE.md)
- [../technologies/CODING_AGENT_TOOL_DEVELOPMENT.md](../technologies/CODING_AGENT_TOOL_DEVELOPMENT.md)
- [../pi-technologies/PI_TOOL_SYSTEM.md](../pi-technologies/PI_TOOL_SYSTEM.md)
