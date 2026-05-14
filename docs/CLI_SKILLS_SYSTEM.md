# Skills 系统

## 背景

AI 编码助手的能力扩展是一个重要方向。当前的 uncode 通过内置工具（read/write/edit/grep/bash）提供基础能力，但缺少高级技能的封装和复用机制。

参考项目 Pi 的 Skills 系统允许用户定义可组合的技能单元，每个 Skill 封装特定的 prompt + 工具组合。

## 目标

- 支持用户自定义 Skill（prompt + 工具约束 + 输入 schema）
- Skill 可在 CLI 和 TUI 中通过 `/skill_name` 调用
- Skill 可组合和嵌套
- 内置常用 Skills

## 设计

### Skill 定义

Skill 是一个 Markdown 文件，存放在 `~/.uncode/skills/`：

```markdown
---
name: code-review
description: 代码审查技能
tools: [read, grep, bash]
inputs:
  - name: path
    description: 要审查的文件或目录
    required: true
---

你是一位资深代码审查专家。

请审查以下代码：{{path}}

审查维度：
1. 安全性
2. 性能
3. 可维护性
4. 测试覆盖

输出格式：
- 按严重程度排序
- 每个问题标注位置和建议修改
```

### Skill 加载

```rust
pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
}

pub struct Skill {
    pub name: String,
    pub description: String,
    pub tools: Vec<String>,
    pub inputs: Vec<SkillInput>,
    pub prompt_template: String,
}
```

### 调用方式

**CLI**:
```
uncode /code-review path=src/main.rs
```

**TUI**:
```
/code-review path=src/main.rs
```

### 执行流程

1. 解析 Skill 定义，验证输入参数
2. 构建 system prompt（Skill 的 prompt 模板 + 变量替换）
3. 限制可用工具范围为 Skill 声明的 `tools`
4. 启动 Agent 循环执行
5. 输出结果

### 内置 Skills

1. `code-review` — 代码审查
2. `explain` — 代码解释
3. `test-gen` — 测试生成
4. `refactor` — 重构建议
5. `security-audit` — 安全审计

### 与 Prompt 模板的关系

- Prompt 模板：单轮 prompt，无工具限制
- Skill：多轮 agent 循环，限定工具集，可组合

Skill 是 Prompt 模板的超集。

## 验收标准

- [ ] `~/.uncode/skills/` 目录下的 Skill 被加载
- [ ] TUI `/skill_name` 调用 Skill
- [ ] Skill 限定工具范围
- [ ] 变量插值正常工作
- [ ] 内置 Skill 开箱可用
