# Pi 工具执行系统

> AgentTool 定义、执行模式、执行流水线、ExecutionEnv 环境抽象、Shell 输出处理

---

## AgentTool 定义

```typescript
interface AgentTool<TParameters, TDetails> {
    name: string;
    label: string;                // UI 展示名
    description: string;
    parameters: TSchema;          // TypeBox schema
    executionMode?: "sequential" | "parallel";  // 覆盖全局设置
    prepareArguments?: (args: unknown) => Static<TParameters>;  // 兼容性垫片
    execute: (
        toolCallId: string,
        params: Static<TParameters>,
        signal?: AbortSignal,
        onUpdate?: AgentToolUpdateCallback,  // 流式进度回调
    ) => Promise<AgentToolResult<TDetails>>;
}
```

---

## AgentToolResult

```typescript
interface AgentToolResult<T = unknown> {
    content: (TextContent | ImageContent)[];  // 多模态内容
    details: T;                                // 结构化详情
    terminate?: boolean;                       // 终止标志
}
```

---

## 执行模式

| 模式 | 行为 |
|------|------|
| `sequential` | 逐个执行：prepare → validate → execute → finalize → 下一个 |
| `parallel` | prepare 串行（需 beforeToolCall 结果），execute 并发 `Promise.all`，`tool_execution_end` 按完成顺序发射，`toolResult` 消息按 assistant 源码顺序发射 |

**混合规则**：如果任何工具标记为 `sequential`，则**整批降级为串行**。

---

## 工具执行流水线

```
raw toolCall (来自 LLM)
    │
    ▼ prepareArguments()        ← 预处理参数（兼容性垫片）
    │
    ▼ validateToolArguments()   ← TypeBox 校验
    │
    ▼ beforeToolCall()          ← hook: 可 block 执行
    │
    ▼ tool.execute()            ← 实际执行，可流式 onUpdate
    │
    ▼ afterToolCall()           ← hook: 可覆盖 content/details/isError/terminate
    │
    ▼ createToolResultMessage() ← 构建标准 toolResult 消息
```

### 错误处理

**工具失败时抛出异常**，Agent 自动包装为 `isError: true` 的 toolResult 反馈给 LLM。

---

## ExecutionEnv 环境抽象

`ExecutionEnv = FileSystem + Shell`，解耦 agent 与运行时环境。

### FileSystem 接口

```typescript
interface FileSystem {
    readTextFile(path): Promise<Result<string>>;
    readTextLines(path, start, end): Promise<Result<string[]>>;
    readBinaryFile(path): Promise<Result<Uint8Array>>;
    writeFile(path, content): Promise<Result<void>>;
    appendFile(path, content): Promise<Result<void>>;
    fileInfo(path): Promise<Result<FileInfo>>;
    listDir(path): Promise<Result<DirEntry[]>>;
    canonicalPath(path): Promise<Result<string>>;
    exists(path): Promise<Result<boolean>>;
    createDir(path): Promise<Result<void>>;
    remove(path, options?): Promise<Result<void>>;
    createTempDir(): Promise<Result<string>>;
    createTempFile(): Promise<Result<string>>;
    // ...
}
```

所有操作返回 `Result<T>`（不抛异常），错误通过 `FileError` + stable error code 返回。

### Shell 接口

```typescript
interface Shell {
    exec(command, options?: {
        timeout?: number;
        signal?: AbortSignal;
        onStdout?: (data: string) => void;
        onStderr?: (data: string) => void;
    }): Promise<ShellResult>;
}
```

### 参考实现

`NodeExecutionEnv` 实现了 `FileSystem + Shell`，提供完整的 Node.js 运行时环境。

---

## Shell 输出处理

`executeShellWithCapture()` 提供流式输出捕获：

- **自动截断**：50KB 默认限制，超出部分溢出到临时文件
- **二进制清理**：非 UTF-8 内容自动过滤
- **行级截断**：`truncateHead()` / `truncateTail()` 支持 line/byte 限制
- **grep 输出**：`truncateLine()` 500 字符限制

---

*本文档基于 Pi 源码 (`@earendil-works/pi-agent-core`) 编写。*
