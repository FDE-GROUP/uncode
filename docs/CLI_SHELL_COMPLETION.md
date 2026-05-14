# Shell 补全完善

## 背景

当前 uncode CLI 有 `--completions` 参数但实现不完整。Shell 补全对于 CLI 工具的用户体验至关重要，可以减少记忆负担和输入错误。

## 目标

- 支持 Bash、Zsh、Fish、PowerShell 补全脚本生成
- 补全模型名称、子命令、常用选项
- 集成 `clap_complete` 自动生成

## 设计

### 命令

```
uncode completions bash    > ~/.local/share/bash-completion/completions/uncode
uncode completions zsh     > ~/.zfunc/_uncode
uncode completions fish    > ~/.config/fish/completions/uncode.fish
uncode completions powershell > uncode.ps1
```

### 补全内容

| 类别 | 补全项 |
|------|--------|
| 全局选项 | `--model`, `--session`, `--continue`, `--repl`, `--mode`, `--template` |
| 模型名称 | `deepseek-v3`, `deepseek-v4-pro`, `glm-5.1`, `ollama` 等 |
| 子命令 | `sessions`, `models`, `export`, `completions` |
| 会话 ID | 动态补全最近会话 |

### 实现方案

使用 `clap_complete` crate：

```rust
use clap::CommandFactory;
use clap_complete::{generate, Shell};

#[derive(clap::Subcommand)]
enum Commands {
    /// Generate shell completions
    Completions { shell: Shell },
}

fn print_completions(shell: Shell) {
    let mut cmd = Cli::command();
    let name = "uncode".to_string();
    generate(shell, &mut cmd, name, &mut std::io::stdout());
}
```

### 动态补全

对于会话 ID 和模型名称的动态补全：
- Bash: 使用 `COMPREPLY` + 调用 `uncode sessions --json` 获取列表
- Zsh: 使用 `_arguments` + 调用辅助命令
- Fish: 使用 `complete -f -c uncode -a "(uncode sessions --json)"`

## 验收标准

- [ ] `uncode completions bash` 生成有效的 Bash 补全脚本
- [ ] `uncode completions zsh` 生成有效的 Zsh 补全脚本
- [ ] `uncode completions fish` 生成有效的 Fish 补全脚本
- [ ] 选项、子命令、模型名称均可补全
- [ ] 安装文档说明各 shell 的安装方法
