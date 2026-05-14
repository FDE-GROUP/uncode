# CLI `--continue` 继续上次会话

## 背景

用户经常需要延续上一次的对话继续工作。当前 uncode 只支持通过 `--session <id>` 指定会话 ID 恢复，需要用户手动查找和输入 ID，体验不友好。

参考项目 Pi 支持 `--continue` / `-c` 标志，自动恢复最近一次会话。

## 目标

- 支持 `uncode --continue` / `uncode -c` 自动恢复最近一次会话
- 支持 `uncode -c "追加提示"` 在恢复会话的同时发送新消息
- 与现有 `--session <id>` 互不冲突

## 设计

### CLI 参数

```
uncode -c, --continue [prompt]    继续最近一次会话
```

- 无参数：恢复最近会话，进入 TUI 等待用户输入
- 带参数：恢复最近会话，立即发送 prompt 作为新消息

### 实现要点

1. **SessionStore 新增方法** `latest_session() -> Option<Session>`
   - 扫描 `~/.uncode/sessions/` 目录，按 `updated_at` 降序取第一个
   - 排除当前正在进行的会话（如通过锁文件判断）

2. **CLI 参数解析**（`uncode-cli/src/main.rs`）
   - 新增 `--continue` / `-c` 布尔标志
   - 优先级：`--session` > `--continue` > 新建会话
   - 当指定 `-c` 且找到最近会话时，加载历史消息

3. **消息加载**
   - 从 JSONL 文件恢复完整消息历史
   - 保留 system prompt 和所有上下文
   - 如果附带 prompt 参数，作为新的 User 消息追加

### 冲突处理

- `--continue` 与 `--session` 同时指定：`--session` 优先，打印 warning
- 无历史会话时：打印提示并退出，或回退到新建会话

## 验收标准

- [ ] `uncode -c` 恢复最近会话并进入 TUI
- [ ] `uncode -c "fix the bug"` 恢复会话并立即发送消息
- [ ] 无历史会话时有合理提示
- [ ] `--session` 与 `--continue` 互不干扰
- [ ] 现有测试不受影响
