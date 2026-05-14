# Prompt 模板系统

## 背景

用户经常需要重复使用特定的 prompt 模式（代码审查、bug 分析、重构建议等）。当前 uncode 没有模板机制，用户每次都需要手动输入完整 prompt。

参考项目 Pi 支持 prompt 模板文件，用户可创建、管理和复用常用 prompt。

## 目标

- 支持 TOML 格式的 prompt 模板定义
- 模板支持变量插值
- CLI 和 TUI 都可调用模板
- 内置常用模板，用户可自定义

## 设计

### 模板存储

模板文件存放在 `~/.uncode/templates/` 目录，每个模板一个 TOML 文件：

```toml
# ~/.uncode/templates/review.toml
name = "code-review"
description = "代码审查：检查安全性、性能和可维护性"
variables = ["language", "focus"]

system = "你是一位资深代码审查专家。"

prompt = """
请对以下 {{language}} 代码进行审查。

重点关注：{{focus}}

审查维度：
1. 安全性漏洞（OWASP Top 10）
2. 性能问题
3. 可维护性和代码规范
4. 错误处理完整性

请用中文回复，按严重程度排序。
"""
```

### 变量插值

使用 `{{variable}}` 语法，渲染时替换：
- 命名变量：`{{language}}` → 用户提供的值
- 特殊变量：`{{selection}}` → 当前选中的文本（TUI）、`{{file}}` → 当前文件路径

### CLI 调用

```
uncode --template review --var language=rust --var focus="error handling"
uncode -t review "language=rust,focus=error handling"
```

### TUI 调用

- `/template` 命令列出可用模板
- `/template review` 选择模板，弹出变量输入界面
- `/template review language=rust focus="error handling"` 直接填充

### 内置模板

提供以下内置模板（编译时嵌入）：

1. `review` — 代码审查
2. `refactor` — 重构建议
3. `test` — 生成单元测试
4. `explain` — 代码解释
5. `fix` — Bug 修复
6. `document` — 生成文档

用户自定义模板优先级高于内置模板（同名覆盖）。

### 模板加载

```rust
pub struct TemplateStore {
    builtin: HashMap<String, Template>,
    user: HashMap<String, Template>,
}

impl TemplateStore {
    pub fn load() -> Result<Self>;
    pub fn get(&self, name: &str) -> Option<&Template>;
    pub fn list(&self) -> Vec<&Template>;
    pub fn render(&self, name: &str, vars: &HashMap<String, String>) -> Result<String>;
}
```

## 验收标准

- [ ] `~/.uncode/templates/` 目录下的模板可被发现和加载
- [ ] `--template` / `-t` 参数可用
- [ ] 变量插值正常工作
- [ ] 内置模板开箱可用
- [ ] TUI `/template` 命令列出并使用模板
- [ ] 用户自定义模板覆盖同名内置模板
