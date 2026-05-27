# 编码能力提升计划

> 基于 InstanceRegistry 和双推理基础设施，通过上下文注入消除 LLM 的"发现 turn"。

---

## 一、目标

减少每个编码任务的"结构探索 turn"（ls/find/grep 用于摸清项目布局），让 LLM 直接进入"修改 turn"。

---

## 二、能力 1：模块感知上下文注入

### 2.1 效果

```
当前流程（5-6 turn）:
  Turn 1: 用户提需求
  Turn 2: LLM 调 ls 看目录结构
  Turn 3: LLM 调 read 看入口文件
  Turn 4: LLM 调 grep 搜索相关代码
  Turn 5: LLM 调 read 看目标文件
  Turn 6: LLM 调 edit 修改

目标流程（2-3 turn）:
  Turn 1: 用户提需求 → 注入同模块文件列表 + 结构概览
  Turn 2: LLM 调 read 看目标文件
  Turn 3: LLM 调 edit 修改
```

### 2.2 技术设计

**Module 推断规则**（语言无关，纯目录结构）：

```
src/auth/login.rs    → Module("src/auth")
src/auth/register.rs → Module("src/auth")
src/models/user.rs   → Module("src/models")
Cargo.toml           → Module(root)
```

规则：文件路径的父目录 = 模块名。根目录文件归入 root。

**File 实例扩展**：

```rust
EntityInstance {
    type_id: TypeId::from("File"),
    id: "src/auth/login.rs",
    fields: {
        "path": "src/auth/login.rs",
        "module": "src/auth",
        "exists": true,
    },
}
```

**Module 实例**：

```rust
EntityInstance {
    type_id: TypeId::from("Module"),
    id: "src/auth",
    fields: {
        "name": "src/auth",
        "file_count": 3,
    },
}
```

**上下文注入时机**：`rebuild_context_with_injections()`，在 Workspace Files 之后。

**注入格式**：

```
## Workspace Structure

src/auth/
  login.rs
  register.rs  
  middleware.rs
src/models/
  user.rs
  session.rs
Cargo.toml
```

仅当 LLM 在当前 turn 操作了某个文件的路径时，高亮其所在模块（加 `←` 标记）。

**注入规则**：

- 文件数 ≤ 50 时注入完整树
- 文件数 > 50 时仅注入当前操作的模块 + 根目录文件
- 复用 InstanceRegistry 的 `list_by_type(TypeId::from("File"))`

### 2.3 改动点

| # | 位置 | 内容 |
|:---|:---|:---|
| 1 | `loop_engine.rs` `rebuild_context_with_injections()` | File 实例注入时计算 module 字段 |
| 2 | `loop_engine.rs` 同上 | Module 实例注入 |
| 3 | `loop_engine.rs` 同上 | 目录树上下文注入 |
| 4 | `loop_engine.rs` 工具后注册 | 更新 File 的 module 字段 |

估计改动：~60 行。

---

## 三、能力 2：Session 修改追踪

### 3.1 效果

```
当前: compaction 之后 LLM 丢失"我改过哪些文件"的记忆
目标: 每个 turn 注入自会话开始以来所有修改过的文件列表
```

消除两种低效交互：
- LLM 重新 `read` 自己刚改过的文件
- 用户问"你改了哪些文件"时 LLM 困惑

### 3.2 技术设计

**File 实例扩展**：

```rust
EntityInstance {
    type_id: TypeId::from("File"),
    id: "src/auth/login.rs",
    fields: {
        "path": "src/auth/login.rs",
        "module": "src/auth",
        "exists": true,
        "modified_in_session": false,     // 新增
        "last_modified_turn": null,       // 新增
    },
}
```

**更新时机**：write/edit 工具执行成功后，在现有的文件注册代码块中追加字段更新。

**上下文注入**：

```
## Session Changes (1 modified)

  src/auth/login.rs (turn 3)
```

仅当有修改时注入，位置在 Workspace Structure 之后。

**Compaction 持久化**：修改记录已存在于 InstanceRegistry 中（内存），Compaction 不需要额外处理。唯一需要的是：如果文件在 compaction 之后被新 turn 再次修改，`last_modified_turn` 能正确定位。

### 3.3 改动点

| # | 位置 | 内容 |
|:---|:---|:---|
| 1 | `loop_engine.rs` File 注入 | 新增 `modified_in_session`/`last_modified_turn` 默认值 |
| 2 | `loop_engine.rs` 工具后注册 | write/edit 时额外设置 `modified_in_session: true` + `last_modified_turn` |
| 3 | `loop_engine.rs` `rebuild_context_with_injections()` | 注入修改文件列表 |

估计改动：~30 行。

---

## 四、实现路线

**Phase 1**：能力 1 + 2（~90 行，一个 commit）

```
1. File 实例扩展 module/modified_in_session/last_modified_turn 字段
2. Module 实例自动创建
3. 目录树上下文注入
4. Session 修改追踪注入
```

**Phase 2**：能力 3 — 推导成本决策（~10 行，独立 commit）

```
CostBudgetPolicy 使用 total_cost_per_million 简化比较
```

**Phase 3**：能力 4 — 工具链推理（单独讨论，需新模块）

---

## 五、验证

| 场景 | 预期 |
|:---|:---|
| Rust 项目（src/auth/*, src/models/*） | 首次 turn 注入目录树，不显示 module 字段为空 |
| LLM 执行 write 后 | 下个 turn 注入 "Session Changes" |
| 大项目（>50 文件） | 仅注入当前操作文件所在模块 + 根目录 |
| 非 Rust 项目 | 同样生效（模块推断仅依赖目录结构） |
| Compaction 后 | 修改追踪继续有效（位于 InstanceRegistry 内存中） |

---

## 六、相关文件

| 文件 | 用途 |
|:---|:---|
| `crates/uncode-ontology/src/instance.rs` | EntityInstance — 扩展字段 |
| `crates/uncode-agent/src/loop_engine.rs` | 注入 + 注册 + 上下文 |
| `docs/technologies/UNCODE_DUAL_REASONING_ACTIVATION.md` | 基础设施文档 |
