# Rust API 文档（rustdoc）

uncode 使用 Rust 自带的 [rustdoc](https://doc.rust-lang.org/rustdoc/index.html) 从源码注释生成 HTML API 文档。设计文档在 `docs/`；**可浏览的 Rust API** 用本指南中的命令生成。

## 前置条件

- Rust 1.91+（与 workspace `rust-version` 一致）
- 已在仓库根目录执行过至少一次 `cargo build`

## 一键命令（推荐）

仓库根目录已配置 [`.cargo/config.toml`](../../.cargo/config.toml) 别名：

```bash
# 生成 workspace 内所有 library crate 的文档（不含依赖，较快）
cargo api-doc

# 生成并打开 core / agent / ai 三个核心 crate（浏览器）
cargo api-doc-open

# 只打开某一个 crate
cargo api-doc-core
cargo api-doc-agent
cargo api-doc-ai
```

> 不能自定义名为 `doc` 的别名（会与 Cargo 内置 `doc` 冲突）；请使用 `api-doc*`。

生成结果目录：`target/doc/<crate_name>/index.html`

> `uncode-cli`、`uncode-platform` 仅为 binary crate，默认不生成独立 API 页；查 CLI 请用 `cargo run -p uncode-cli -- --help`。

## 手动命令

```bash
# 与 CI 一致：文档警告视为错误
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

# 包含依赖文档（体积大、编译慢，查 std / tokio 时有用）
cargo doc --workspace --open

# 单个 crate
cargo doc -p uncode-agent --no-deps --open
```

`.cargo/config.toml` 里已设置 `rustdocflags = ["-D", "warnings"]`，因此运行 `cargo doc` 或 `cargo api-doc` 都会启用该检查。

## 与 Pi 对照的阅读方式

核心 crate 的 `pub` API 在 rustdoc 中带 **`Pi:`** 行，与 [UNCODE_PI_MECHANISM_MAP.md](../uncode-technologies/UNCODE_PI_MECHANISM_MAP.md) 对照阅读：

| Crate | 入口页 | 重点符号 |
|-------|--------|----------|
| `uncode-core` | `SessionEntry`、`AgentEvent`、`ToolExecutor` | 会话树、事件、工具 |
| `uncode-agent` | `AgentHarness`、`LoopEngine`、会话/压缩模块 | Agent 循环 |
| `uncode-ai` | `Api`、`StreamEvent`、`Model` | LLM 驱动 |

## 贡献者：如何写文档注释

新增 **核心** `pub` API 时，在 `CONTRIBUTING.md`「术语与 Pi 对照」要求下增加 rustdoc，模板见 [术语分层重构方案 Phase 3](../technologies/TERMINOLOGY_LAYERED_REFACTOR_PLAN.md#phase-3--rustdoc-pi-映射优先级-p3)：

```rust
/// 从会话存储构建发往 LLM 的消息列表与有效模型配置。
///
/// **Pi:** 对应 `transformContext` 之后、`convertToLlm` 之前的上下文组装。
pub async fn build_context(...) -> ...
```

- 模块/crate 总览用 `//!`（见各 `src/lib.rs`）
- 项说明用 `///`
- 同 crate 内链接可写 `` [`TypeName`] ``，不必写 `` [`Type`](module::Type) ``（冗余目标会触发 rustdoc 警告）

## 文档测试

`///` 或模块文档里带 ` ```rust ` 的示例会在 `cargo test` 时作为 **doctest** 执行。仅展示、不运行的示例用 ` ```rust,no_run ` 或 ` ```ignore `。

## 常见问题

| 现象 | 处理 |
|------|------|
| `could not document` + `redundant explicit link target` | 去掉链接里的显式路径，只保留 `` [`Api`] `` |
| 文档生成很慢 | 使用 `--no-deps`（别名 `cargo doc` 已包含） |
| 想查第三方 crate | 使用 [docs.rs](https://docs.rs) 或去掉 `--no-deps` 后 `cargo doc --open` |

## 相关文档

- [CONTRIBUTING.md](../../CONTRIBUTING.md) — 贡献流程与 Pi rustdoc 约定
- [UNCODE_PI_MECHANISM_MAP.md](../uncode-technologies/UNCODE_PI_MECHANISM_MAP.md) — L1 机制对照
- [Rust 官方 rustdoc 书](https://doc.rust-lang.org/rustdoc/index.html)
