# Pi 扩展系统

> Skills 技能系统、Prompt Templates 模板系统、Resources 资源容器

---

## Skills 系统

### 技能加载

从 `.pi/skills/*.md` 或 `SKILL.md` 加载，YAML frontmatter 定义元数据：

```yaml
---
name: git-release
description: Create git releases
disable-model-invocation: true   # 仅应用可用，模型不可见
---
```

- 递归目录遍历（尊重 `.gitignore` / `.ignore` / `.fdignore`）
- `loadSourcedSkills()` 支持 tagged provenance
- 诊断系统（`SkillDiagnostic` codes）

### 技能注入

`formatSkillsForSystemPrompt()` 生成 `<available_skills>` XML 块注入 system prompt。`formatSkillInvocation()` 包裹内容 + 位置上下文。

---

## Prompt Templates 系统

从 `.md` 文件 + YAML frontmatter 加载模板：

```
promptFromTemplate("refactor", "src/lib.rs --dry-run")
    → 加载 refactor.md
    → 替换 $1 → "src/lib.rs"
    → 替换 $@ → "src/lib.rs --dry-run"
```

### 占位符语法

| 占位符 | 说明 |
|--------|------|
| `$1`, `$2`, ... | 位置参数 |
| `$@` / `$ARGUMENTS` | 全部参数 |
| `${@:N}` | 从第 N 个参数开始 |
| `${@:N:L}` | 从第 N 个参数开始，取 L 个 |

Shell 风格参数解析（支持引号），通过 `promptFromTemplate()` 调用。

---

## Resources 系统

`AgentHarnessResources<TSkill, TPromptTemplate>` 是 skills 和 templates 的泛型容器：

- 每个 turn 开始时快照当前 resources，传给 system prompt callback
- 应用自行管理加载/重载，调用 `setResources()` 更新
- 变更时发射 `resources_update` 事件

---

*本文档基于 Pi 源码 (`@earendil-works/pi-agent-core`) 编写。*
