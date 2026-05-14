# uncode 会话数据 JSONL Schema

## 一、概述

会话数据以 JSONL（JSON Lines）格式存储在 `~/.uncode/sessions/{session_id}.jsonl` 中。每行是一个完整的 JSON 对象，代表一个会话条目。

TUI/Agent 写入，Platform 读取。此 Schema 在 Phase 0 锁定，Phase 1-2 严格实现，确保 Phase 3 的 Platform 零迁移成本。

---

## 二、文件结构

```
第 1 行（头行）：  SessionHeader    — 会话元数据
第 2 行起：        SessionEntry      — 消息/系统/分支条目，按时间顺序追加
```

示例文件：

```jsonl
{"type":"header","id":"abc123","created_at":"2026-05-14T10:30:00Z","updated_at":"2026-05-14T10:35:00Z","model":"deepseek-v3","title":"实现登录功能","working_dir":"/home/user/project"}
{"type":"message","timestamp":"2026-05-14T10:30:01Z","role":"user","content":[{"type":"text","text":"帮我实现登录功能"}]}
{"type":"message","timestamp":"2026-05-14T10:30:05Z","role":"assistant","content":[{"type":"thinking","text":"用户需要登录功能，我先看看项目现有的认证方式"}],"usage":{"input_tokens":230,"output_tokens":45}}
{"type":"message","timestamp":"2026-05-14T10:30:06Z","role":"assistant","content":[{"type":"tool_call","id":"call_1","name":"read","arguments":{"path":"src/main.rs"}}]}
{"type":"message","timestamp":"2026-05-14T10:30:06Z","role":"tool","content":[{"type":"tool_result","tool_call_id":"call_1","content":"// main.rs content...","is_error":false}]}
{"type":"message","timestamp":"2026-05-14T10:30:10Z","role":"assistant","content":[{"type":"text","text":"我看到项目使用 Actix-web，认证通过 JWT + middleware 实现。我来在 src/auth/ 下添加登录接口。"}]}
{"type":"system","timestamp":"2026-05-14T10:30:15Z","event":"phase_summary","data":{"phase":1,"completed":["分析项目结构","理解认证模式"],"issues":["测试文件缺失"],"next_steps":["实现登录接口","编写测试"],"token_usage":{"input":560,"output":120}}}
```

---

## 三、类型定义

### 3.1 SessionHeader（头行）

文件第一行，必须存在。

```json
{
  "type": "header",
  "id": "string (uuid v4)",
  "created_at": "string (ISO 8601 UTC)",
  "updated_at": "string (ISO 8601 UTC)",
  "model": "string (LLM 模型标识)",
  "title": "string | null (会话标题)",
  "working_dir": "string (工作目录绝对路径)"
}
```

| 字段 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `type` | `"header"` | 是 | 标识此行类型 |
| `id` | string | 是 | 会话 UUID v4 |
| `created_at` | string | 是 | 创建时间，ISO 8601 UTC |
| `updated_at` | string | 是 | 最后修改时间，ISO 8601 UTC |
| `model` | string | 是 | 使用的 LLM 模型标识，如 `"deepseek-v3"` |
| `title` | string \| null | 否 | 会话标题，首次用户输入的前 60 字符 |
| `working_dir` | string | 是 | 会话的工作目录绝对路径 |

### 3.2 SessionEntry（消息/系统行）

头行之后的所有行都是 `SessionEntry`，有三种变体：

#### 3.2.1 MessageEntry

```json
{
  "type": "message",
  "timestamp": "string (ISO 8601 UTC)",
  "role": "user | assistant | system | tool",
  "content": [ContentBlock, ...],
  "usage": UsageInfo | null
}
```

| 字段 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `type` | `"message"` | 是 | 标识此行类型 |
| `timestamp` | string | 是 | 消息时间，ISO 8601 UTC |
| `role` | string | 是 | 消息角色 |
| `content` | ContentBlock[] | 是 | 内容块数组 |
| `usage` | UsageInfo \| null | 否 | Token 用量（仅 assistant 消息） |

#### 3.2.2 SystemEntry

```json
{
  "type": "system",
  "timestamp": "string (ISO 8601 UTC)",
  "event": "phase_summary | session_start | session_end | error | compaction",
  "data": object
}
```

| 字段 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `type` | `"system"` | 是 | 标识此行类型 |
| `timestamp` | string | 是 | 事件时间，ISO 8601 UTC |
| `event` | string | 是 | 系统事件类型 |
| `data` | object | 是 | 事件数据，依 event 类型而定 |

#### 3.2.3 BranchEntry

```json
{
  "type": "branch",
  "timestamp": "string (ISO 8601 UTC)",
  "parent_id": "string (父会话 UUID)",
  "reason": "string (分支原因)"
}
```

| 字段 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `type` | `"branch"` | 是 | 标识此行类型 |
| `timestamp` | string | 是 | 分支时间，ISO 8601 UTC |
| `parent_id` | string | 是 | 父会话 UUID |
| `reason` | string | 是 | 分支原因简述 |

---

## 四、ContentBlock 类型

每条消息的 `content` 数组由以下类型的块组成：

```typescript
type ContentBlock =
  | TextBlock
  | ThinkingBlock
  | ToolCallBlock
  | ToolResultBlock
```

### 4.1 TextBlock

```json
{ "type": "text", "text": "string" }
```

### 4.2 ThinkingBlock

```json
{ "type": "thinking", "text": "string" }
```

表示 LLM 的思考/推理内容（extended thinking / reasoning tokens）。

### 4.3 ToolCallBlock

```json
{
  "type": "tool_call",
  "id": "string (工具调用唯一标识)",
  "name": "string (工具名称)",
  "arguments": object (JSON 对象)
}
```

### 4.4 ToolResultBlock

```json
{
  "type": "tool_result",
  "tool_call_id": "string (对应的 ToolCallBlock.id)",
  "content": "string (工具执行输出)",
  "is_error": boolean
}
```

---

## 五、UsageInfo

```json
{
  "input_tokens": number,
  "output_tokens": number
}
```

| 字段 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `input_tokens` | number | 是 | 输入 token 数（含缓存命中） |
| `output_tokens` | number | 是 | 输出 token 数 |

---

## 六、SystemEntry 事件类型

### 6.1 phase_summary

Agent 完成一组关联任务后自动生成的阶段总结。

```json
{
  "type": "system",
  "timestamp": "2026-05-14T10:30:15Z",
  "event": "phase_summary",
  "data": {
    "phase": 1,
    "completed": ["分析项目结构", "理解认证模式"],
    "issues": ["测试文件缺失"],
    "next_steps": ["实现登录接口", "编写测试"],
    "token_usage": { "input": 560, "output": 120 }
  }
}
```

### 6.2 session_start

```json
{
  "type": "system",
  "timestamp": "2026-05-14T10:30:00Z",
  "event": "session_start",
  "data": {
    "model": "deepseek-v3",
    "working_dir": "/home/user/project",
    "tools": ["read", "write", "edit", "grep", "bash", "webfetch"]
  }
}
```

### 6.3 session_end

```json
{
  "type": "system",
  "timestamp": "2026-05-14T10:35:00Z",
  "event": "session_end",
  "data": {
    "total_turns": 3,
    "total_tokens": { "input": 1230, "output": 340 },
    "tool_calls": { "total": 5, "successful": 5, "failed": 0 },
    "files_modified": ["src/auth/login.rs", "src/auth/mod.rs"],
    "exit_reason": "user_stop | task_complete | error | timeout"
  }
}
```

### 6.4 error

```json
{
  "type": "system",
  "timestamp": "2026-05-14T10:31:00Z",
  "event": "error",
  "data": {
    "category": "llm | tool | network | config",
    "message": "LLM API 请求超时（GLM）",
    "recoverable": true,
    "recovered": true
  }
}
```

### 6.5 compaction

```json
{
  "type": "system",
  "timestamp": "2026-05-14T10:32:00Z",
  "event": "compaction",
  "data": {
    "messages_before": 25,
    "messages_after": 12,
    "summarized_range": [1, 13],
    "summary": "早期对话摘要：用户请求实现登录功能，Agent 分析了项目结构..."
  }
}
```

---

## 七、完整角色枚举

| Role | 描述 | ContentBlock 类型 |
|------|------|------------------|
| `user` | 用户输入 | TextBlock |
| `assistant` | Agent 响应 | TextBlock, ThinkingBlock, ToolCallBlock |
| `tool` | 工具执行结果 | ToolResultBlock |
| `system` | 系统消息 | TextBlock |

---

## 八、Platform 索引策略

Platform 使用 SurrealDB（SurrealKV 嵌入）作为查询层。启动时扫描 JSONL 目录，增量更新索引。

**SurrealQL 表定义：**

```sql
-- 会话表
DEFINE TABLE session SCHEMAFULL;
DEFINE FIELD id ON session TYPE string;
DEFINE FIELD created_at ON session TYPE string;
DEFINE FIELD updated_at ON session TYPE string;
DEFINE FIELD model ON session TYPE string;
DEFINE FIELD title ON session TYPE option<string>;
DEFINE FIELD working_dir ON session TYPE string;
DEFINE FIELD jsonl_path ON session TYPE string;
DEFINE FIELD message_count ON session TYPE int DEFAULT 0;
DEFINE FIELD total_input_tokens ON session TYPE int DEFAULT 0;
DEFINE FIELD total_output_tokens ON session TYPE int DEFAULT 0;

-- 事件表（关联到会话）
DEFINE TABLE session_event SCHEMAFULL;
DEFINE FIELD session_id ON session_event TYPE record<session>;
DEFINE FIELD jsonl_line ON session_event TYPE int;
DEFINE FIELD timestamp ON session_event TYPE string;
DEFINE FIELD event_type ON session_event TYPE string;
DEFINE FIELD role ON session_event TYPE option<string>;
DEFINE FIELD content_summary ON session_event TYPE option<string>;
DEFINE FIELD tool_name ON session_event TYPE option<string>;
DEFINE FIELD tool_success ON session_event TYPE option<bool>;
DEFINE FIELD system_event ON session_event TYPE option<string>;
DEFINE FIELD input_tokens ON session_event TYPE option<int>;
DEFINE FIELD output_tokens ON session_event TYPE option<int>;

-- 图边：会话包含事件
RELATE session->contains->session_event;
```

**查询示例（SurrealQL）：**

```sql
-- 查找某 Issue 关联的所有会话
SELECT <-linked<-session FROM issue:42;

-- 获取会话的完整时间线
SELECT * FROM session_event WHERE session_id = session:abc123 ORDER BY jsonl_line ASC;

-- 全文搜索会话内容
SELECT * FROM session_event WHERE content_summary @@ '登录';
```

Platform 启动时扫描 JSONL 目录，增量更新索引。SurrealDB 的 SurrealKV 嵌入模式确保本地零配置启动。

---

## 九、扩展预留

以下字段为未来版本预留，当前 Phase 1-2 不实现但 Schema 中保留位置：

| 字段 | 预留用途 |
|------|---------|
| `MessageEntry.metadata` | 用户自定义标签 |
| `SystemEntry.data.custom` | 扩展自定义事件数据 |
| `SessionHeader.tags` | 会话标签（方便分类筛选） |
| `SessionHeader.parent_id` | 父会话 ID（分支场景） |
