# Contributing to uncode

## 开发工作流

遵循 GitHub Flow：

```
main ← PR(审查) ← feature-branch ← 编码
```

1. **创建 Issue** — 描述任务需求
2. **创建分支** — `git checkout -b feat/N-description`
3. **编码 + 测试** — `cargo test --workspace` 必须通过
4. **创建 PR** — 描述中写 `closes #N`
5. **审查合并** — 审查通过后合并到 main

## 构建

```bash
cargo build --workspace     # Rust 全量构建
cargo test --workspace      # 运行所有测试（42+ tests）
cargo fmt --check           # Rust 格式检查
cargo clippy                # Rust lint

cd apps/platform
bun install                 # 前端依赖
bun run build               # 前端构建
```

## 代码规范

- **Rust**: `cargo fmt` + `cargo clippy`，零警告
- **TypeScript**: `biome check apps/`，零错误
- **提交信息**: 遵循 `type: description (closes #N)` 格式
  - `feat:` 新功能
  - `fix:` 修复
  - `docs:` 文档
  - `refactor:` 重构
  - `test:` 测试
  - `perf:` 性能
  - `chore:` 构建/工具

## 架构分层

```
core → macros/llm/session/tools/extensions → agent → tui/rpc/platform → cli
```

- 下层不依赖上层
- 跨层通信通过事件流（`AgentEvent` broadcast）
- 工具通过 `ToolRegistry` 注册

## 术语与 Pi 对照（策略 C）

新增或修改 **L1 机制**（Turn、Steering、Session 树、Compaction 等）时：

1. 在 PR 说明中标注属于 **L0 / L1 / L2 / L3** 哪一层（见 [术语分层重构方案](docs/technologies/TERMINOLOGY_LAYERED_REFACTOR_PLAN.md)）。
2. 更新 [`UNCODE_TECHNOLOGIES_GLOSSARY.md`](docs/uncode-technologies/UNCODE_TECHNOLOGIES_GLOSSARY.md) 的 **Pi 对应** 列（L1 词条必填）。
3. 若涉及事件/循环/会话机制，同步 [`UNCODE_PI_MECHANISM_MAP.md`](docs/uncode-technologies/UNCODE_PI_MECHANISM_MAP.md)。
4. 新增 **核心** `pub` API 时，在 rustdoc 中增加 `/// **Pi:** …` 行（模板见重构方案 Phase 3）。

默认 **不** 将 Rust 公开符号改名为 Pi 的 TypeScript 专名。

## 文档

- [VISION.md](docs/VISION.md) — 项目愿景
- [ARCHITECTURE.md](docs/ARCHITECTURE.md) — 架构设计
- [TUI_DESIGN.md](docs/TUI_DESIGN.md) — TUI 交互设计
- [PLATFORM_DESIGN.md](docs/PLATFORM_DESIGN.md) — Platform 设计
- [SESSION_SCHEMA.md](docs/SESSION_SCHEMA.md) — 会话数据格式
- [术语对齐策略](docs/technologies/TERMINOLOGY_ALIGNMENT_STRATEGY.md) — 与 Pi/OpenCode 的命名策略
- [Pi 机制对照](docs/uncode-technologies/UNCODE_PI_MECHANISM_MAP.md) — L1 一页纸

## 许可证

MIT
