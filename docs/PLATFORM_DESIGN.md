# uncode Platform 设计详案

## 一、定位与目标

Platform 是面向**软件工程师和技术管理者**的 Web 分析监控平台。它是 Agent 活动的"事后分析"工具，与 TUI（"实时工作"）互补，共同组成完整的 Agent Coding 系统。

| 维度 | TUI | Platform |
|------|-----|----------|
| 用户 | FDE、非软件专业人员 | 软件工程师、技术管理者 |
| 时机 | 实时交互 | 事后分析 / 持续监控 |
| 形态 | 终端界面 | Web 应用 |
| 模式 | "做"——指挥 Agent 工作 | "看"——理解 Agent 行为 |

---

## 二、技术选型

### 2.1 前端

与 TOGAF TURBO 统一技术栈，采用 TanStack 全家桶 + React 19。

| 库 | 用途 | 场景 |
|----|------|------|
| **TanStack Router** | 类型安全路由 | 6 页面路由（仪表板/会话列表/会话详情/Issues/指标/设置） |
| **TanStack Query** | 服务端状态管理 | 缓存 REST API 调用，自动重试，后台刷新 |
| **TanStack Table** | 数据表格 | 会话列表、Issue 列表、指标排名 |
| **TanStack Form** | 表单验证 | Issue 创建、设置表单 |
| **TanStack Virtual** | 虚拟滚动 | 会话时间线（>100 条事件高性能渲染） |
| **TanStack Hotkeys** | 键盘快捷键 | 全局搜索 Cmd+K、面板切换 |

| 决策 | 选择 | 理由 |
|------|------|------|
| 框架 | React 19 + TypeScript 5 | 与 TOGAF TURBO 一致 |
| 构建工具 | Vite 8 | 快速 HMR，TanStack 官方推荐 |
| 样式 | TailwindCSS v4 | 与 TOGAF TURBO 一致 |
| 图表 | Recharts（推荐）或 ECharts | 仪表板趋势图 |
| 图可视化 | Sigma.js（如果需要图视图） | TOGAF TURBO 已验证 |
| 状态管理 | Zustand（客户端）+ TanStack Query（服务端） | Zustand 处理 UI 状态，Query 处理 API 数据 |

### 2.2 后端

| 决策 | 选择 | 理由 |
|------|------|------|
| 框架 | **axum** | 与 TOGAF TURBO 统一，Rust 生态最活跃的 Web 框架 |
| 数据库 | SurrealDB（SurrealKV 嵌入） | 与 TOGAF TURBO 统一技术栈 |
| 实时通信 | WebSocket（axum 内置） | 会话事件实时推送 |
| API 风格 | REST + WebSocket | 查询用 REST，实时用 WS |

### 2.3 实施计划

与 TOGAF TURBO 分阶段 PR 策略一致：

**Step 1: Router + Query（基础框架）**
- TanStack Router 建立 6 页面路由结构
- TanStack Query 封装 `backend-client.ts`（所有 REST API 调用 → query hooks）
- 共享 Layout 组件（Header/Sidebar）

**Step 2: Table + Form（数据交互）**
- 会话列表页（TanStack Table，排序/筛选/分页）
- Issue 列表页（TanStack Table + TanStack Form 创建 Issue）
- 设置页（TanStack Form 表单验证）

**Step 3: Virtual + Hotkeys（性能 + 体验）**
- 会话时间线页（TanStack Virtual，>100 条事件流畅滚动）
- TanStack Hotkeys 全局快捷键（`Cmd+K` 搜索、`Cmd+[`/`]` 切换面板）
- 图表仪表板（Recharts 趋势图）

### 2.4 部署

| 场景 | 方案 |
|------|------|
| 本地单用户 | `uncode platform` 命令启动，绑定 localhost |
| 团队共享 | Docker 部署到内网服务器 |
| CI 集成 | 作为 GitHub Action 步骤运行分析 |

## 三、功能模块

### 3.1 会话数据展示

**会话列表页：**

```
┌─────────────────────────────────────────────────────┐
│  🔍 搜索会话...                        📊 全局指标   │
├─────────────────────────────────────────────────────┤
│  会话                        模型       时间    Token │
│  ─────────────────────────────────────────────────── │
│  #42 实现登录功能              DeepSeek  2h ago  12.3k│
│  #41 重构数据库层              GLM-4    1d ago  45.1k│
│  #40 编写集成测试              DeepSeek  2d ago  8.7k │
│  ...                                                 │
└─────────────────────────────────────────────────────┘
```

**会话详情页（时间线视图）：**

```
┌──────────────────────────────────────────────────────┐
│  ← 返回    #42 实现登录功能                            │
│  模型: DeepSeek-V3  |  Token: 12,340  |  消息: 28      │
├──────────────────────────────────────────────────────┤
│                                                       │
│  ● 14:30  用户输入 "帮我实现登录功能"                    │
│  │                                                    │
│  ● 14:30  思考  "用户需要登录功能，我先看看项目结构"      │
│  │                                                    │
│  ● 14:30  工具调用 read src/main.rs          (12ms)    │
│  │                                                    │
│  ● 14:31  工具调用 grep "fn auth"     (8ms)           │
│  │                                                    │
│  ● 14:31  思考  "项目使用 JWT 认证模式..."              │
│  │                                                    │
│  ● 14:32  工具调用 write src/auth/login.rs  (45ms)    │
│  │         [查看 diff]                                │
│  │                                                    │
│  ● 14:33  阶段总结 "已完成登录功能实现，下一步..."        │
│                                                       │
├──────────────────────────────────────────────────────┤
│  关键指标                                             │
│  工具调用: 12次  成功率: 100%  平均耗时: 35ms            │
│  编辑文件: 3个   新增行: 142   删除行: 8                │
└──────────────────────────────────────────────────────┘
```

### 3.2 源码与文档关联

**工具调用 → 源码跳转：**

- 点击 `read src/auth/login.rs` → 展开文件内容（带语法高亮）
- 点击 `write src/auth/login.rs` → 展示完整 diff（类似 GitHub PR 对比视图）
- 点击 `bash cargo test` → 展示测试输出

**会话 → git commit 关联：**

- Agent 提交的 PR 自动在时间线中显示链接
- 支持从 commit 反向查找到触发该变更的会话

```
Agent 完成 "#42 实现登录功能"
    │
    ├── 在时间线中记录 session #42
    │
    └── PR #128 合并到 main
         │
         └── Platform 自动关联: commit abc123 ← session #42
```

### 3.3 Issues 面板

**核心功能：**

```
┌───────────────────────────────────────────────────────┐
│  Issues                                    + 新建 Issue│
├───────────────────────────────────────────────────────┤
│  筛选: [All] [Open] [Closed]   排序: [最新] [优先级]    │
├───────────────────────────────────────────────────────┤
│  #42 实现登录功能                        Open  🔴       │
│  由 uncore 创建 | 关联会话: #42 | PR: #128              │
│  ───────────────────────────────────────────────────── │
│  需求：用户需要邮箱+密码的登录方式                         │
│  方案：使用 JWT，参考现有 middleware 模式                  │
│  任务：✅ 分析项目结构  ✅ 实现登录接口  ⏳ 编写测试        │
├───────────────────────────────────────────────────────┤
│  #41 优化数据库查询性能                   Closed 🟢     │
│  由 alice 创建 | 关联会话: #41 | PR: #125               │
│  ...                                                  │
└───────────────────────────────────────────────────────┘
```

**Issue 详情页：**

- Issue 描述和讨论（同步自 GitHub）
- 关联的 Agent 任务清单和完成状态（TUI 推送）
- 关联的 PR 列表
- 关联的会话列表

### 3.4 数据驱动优化

**Agent 行为分析：**

| 指标 | 描述 | 用途 |
|------|------|------|
| 工具调用成功率 | 各工具的成功/失败占比 | 识别不稳定工具 |
| 平均任务完成轮次 | 完成任务需要的对话轮数 | 评估 Agent 效率 |
| Token 消耗趋势 | 每个会话/任务的 Token 用量 | 成本控制 |
| 重复操作检测 | 同一文件被反复读取 | 识别上下文问题 |
| 错误类型分布 | 最常见的错误类别 | 优先级排序修复 |

**提示词优化建议：**

- 分析 Agent 在多轮对话中反复询问同一问题的模式 → 建议将关键信息加入系统提示
- 分析工具调用失败后的重试行为 → 建议添加错误恢复策略

**仪表板：**

```
┌──────────────────────────────────────────────────────┐
│  📊 全局仪表板                          最近 7 天      │
├──────────────────────────────────────────────────────┤
│  总会话: 47   总任务: 89   工具调用: 1,203              │
│  平均成功率: 94%   平均完成轮次: 3.2                     │
├──────────────────────┬───────────────────────────────┤
│  工具调用成功率趋势    │  Token 消耗趋势                 │
│  (echarts 折线图)     │  (echarts 柱状图)               │
├──────────────────────┴───────────────────────────────┤
│  常见错误                                             │
│  1. LLM 超时 (12次)    2. 文件写入冲突 (5次)            │
│  3. 测试失败 (3次)     4. 语法错误 (2次)                │
└──────────────────────────────────────────────────────┘
```

---

## 四、API 设计

### 4.1 REST API

| 方法 | 路径 | 描述 |
|------|------|------|
| `GET` | `/api/sessions` | 列出所有会话（支持分页、筛选） |
| `GET` | `/api/sessions/:id` | 获取会话详情（含时间线） |
| `GET` | `/api/sessions/:id/events` | 获取会话事件流（支持分页） |
| `GET` | `/api/sessions/:id/metrics` | 获取会话指标 |
| `GET` | `/api/metrics` | 获取全局指标 |
| `GET` | `/api/issues` | 列出 Issues（含关联会话/PR） |
| `GET` | `/api/issues/:number` | 获取单个 Issue 详情 |
| `POST` | `/api/issues/:number/link` | 关联 Issue 到会话或 PR |
| `GET` | `/api/suggestions` | 获取优化建议 |

### 4.2 WebSocket API

```
ws://localhost:3000/ws/events

→ 订阅: { "type": "subscribe", "session_id": "uuid" }
← 事件: { "type": "new_event", "session_id": "uuid", "event": {...} }
← 事件: { "type": "session_complete", "session_id": "uuid", "metrics": {...} }
```

---

## 五、数据存储

### 5.1 本地模式（SurrealKV 嵌入）

```
~/.uncode/
├── sessions/          # JSONL 会话文件（由 TUI 写入）
│   ├── abc123.jsonl
│   └── def456.jsonl
├── surrealkv/         # SurrealDB SurrealKV 数据（Platform 写入）
│   └── ...            # 二进制存储，零配置
└── config.toml        # 统一配置文件
```

### 5.2 团队模式（SurrealDB TiKV 分布式）

```
┌──────────┐    ┌──────────┐
│  TUI A   │    │  TUI B   │
│ (本地)    │    │ (本地)    │
└────┬─────┘    └────┬─────┘
     │ JSONL         │ JSONL
     ▼               ▼
┌──────────────────────────┐
│   Platform Server        │
│   (Rust / axum)          │
│   ├── 定时扫描 JSONL      │
│   └── 写入 SurrealDB      │

     ┌─────▼─────┐
     │ SurrealDB │
     │ (TiKV)    │
     └───────────┘
           │
     ┌─────▼─────┐
     │ Platform  │
     │ Frontend  │
     └───────────┘
```

### 5.3 数据量估算

| 场景 | 每天会话数 | 每会话 JSONL 大小 | 月数据量 |
|------|-----------|-------------------|---------|
| 个人开发 | 5-20 | ~50KB | 7.5-30MB |
| 5 人团队 | 25-100 | ~50KB | 37-150MB |
| 20 人团队 | 100-400 | ~50KB | 150-600MB |

所有场景下 JSONL + SurrealDB 方案都足够。SurrealDB SurrealKV 嵌入模式覆盖个人使用，TiKV 分布式模式覆盖团队使用，两种模式共用同一套 SurrealQL 查询代码。

---

## 六、前端页面路由

```
/                          首页（全局仪表板）
/sessions                  会话列表
/sessions/:id              会话详情（时间线视图）
/sessions/:id/diff/:file   文件 diff 详情
/issues                     Issues 列表
/issues/:number             Issue 详情
/metrics                   全局指标面板
/settings                  设置（数据源、团队配置）
```

---

## 七、安全考量

| 层面 | 措施 |
|------|------|
| 本地模式 | 仅监听 localhost，不对外暴露 |
| 团队模式 | 支持 Basic Auth / OAuth2（GitHub 登录） |
| 数据隔离 | 团队模式下按用户/项目隔离数据访问 |
| API 密钥 | API 密钥存储在 `~/.uncode/config.toml`，不通过 Platform API 暴露 |
| CORS | 团队模式下严格配置允许的来源域 |

---

## 八、开发阶段

### Phase 3: Platform 原型

- [ ] Rust 后端框架搭建（axum + SurrealDB SurrealKV）
- [ ] JSONL 文件扫描与解析
- [ ] 会话列表 + 详情 API
- [ ] TypeScript 前端框架搭建（React 19 + Vite 8 + TanStack Router + TanStack Query）
- [ ] 会话列表页 + 会话详情页（时间线视图）
- [ ] Issues 面板（列表 + 详情 + 关联）

### Phase 4: 生产就绪

- [ ] 全局仪表板（指标图表）
- [ ] 数据驱动优化建议
- [ ] WebSocket 实时推送
- [ ] 团队模式（SurrealDB TiKV + 认证）
- [ ] Docker 部署支持
