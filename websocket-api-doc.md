# OpenFang WebSocket 对接文档

## 1. 概述

WebSocket 模块为客户端与代理之间提供实时双向通信通道，支持流式消息传递、命令执行和状态更新。

## 2. 连接建立

### 2.1 连接 URL

```
GET /api/agents/:id/ws
```

### 2.2 认证方式

WebSocket 连接支持两种认证方式：
- **Bearer Token**：在 `Authorization` 头部中使用 `Bearer <token>` 格式
- **查询参数**：在 URL 中使用 `?token=<token>` 格式（适用于无法设置自定义头部的浏览器客户端）

### 2.3 连接限制

- **每 IP 最大连接数**：5 个
- **空闲超时**：30 分钟无消息后自动关闭连接

## 3. 消息格式

### 3.1 客户端发送消息

#### 3.1.1 文本消息

```json
{
  "type": "message",
  "content": "消息内容",
  "attachments": [
    {
      "type": "image",
      "url": "data:image/png;base64,..."
    }
  ]
}
```

#### 3.1.2 命令消息

```json
{
  "type": "command",
  "command": "命令名称",
  "args": "命令参数"
}
```

#### 3.1.3 Ping 消息

```json
{
  "type": "ping"
}
```

### 3.2 服务器响应消息

#### 3.2.1 连接确认

```json
{
  "type": "connected",
  "agent_id": "代理ID"
}
```

#### 3.2.2 打字状态

```json
{
  "type": "typing",
  "state": "start|tool|stop",
  "tool": "工具名称" // 仅在 state 为 tool 时存在
}
```

#### 3.2.3 文本增量

```json
{
  "type": "text_delta",
  "content": "文本内容"
}
```

#### 3.2.4 完整响应

```json
{
  "type": "response",
  "content": "完整响应内容",
  "input_tokens": 100,
  "output_tokens": 50,
  "iterations": 1,
  "cost_usd": 0.001,
  "context_pressure": "low|medium|high|critical"
}
```

#### 3.2.5 错误消息

```json
{
  "type": "error",
  "content": "错误信息"
}
```

#### 3.2.6 代理列表更新

```json
{
  "type": "agents_updated",
  "agents": [
    {
      "id": "代理ID",
      "name": "代理名称",
      "state": "运行状态",
      "model_provider": "模型提供商",
      "model_name": "模型名称"
    }
  ]
}
```

#### 3.2.7 静默完成

```json
{
  "type": "silent_complete",
  "input_tokens": 100,
  "output_tokens": 0
}
```

#### 3.2.8 Canvas 消息

```json
{
  "type": "canvas",
  "canvas_id": "画布ID",
  "html": "HTML内容",
  "title": "画布标题"
}
```

#### 3.2.9 工具相关消息

```json
// 工具开始
{
  "type": "tool_start",
  "tool": "工具名称"
}

// 工具结束
{
  "type": "tool_end",
  "tool": "工具名称",
  "input": "工具输入"
}

// 工具结果
{
  "type": "tool_result",
  "tool": "工具名称",
  "result": "工具结果",
  "is_error": false
}
```

#### 3.2.10 阶段变更

```json
{
  "type": "phase",
  "phase": "阶段名称",
  "detail": "阶段详情"
}
```

#### 3.2.11 Ping 响应

```json
{
  "type": "pong"
}
```

#### 3.2.12 命令结果

```json
{
  "type": "command_result",
  "command": "命令名称",
  "message": "命令执行结果",
  "context_pressure": "low|medium|high|critical" // 仅在 context 命令时存在
}
```

## 4. 命令系统

WebSocket 支持以下命令：

| 命令      | 描述                          | 参数                      | 示例响应                                                                 |
|-----------|-------------------------------|---------------------------|--------------------------------------------------------------------------|
| `new`     | 重置会话，清除聊天历史        | 无                        | `{"type": "command_result", "command": "new", "message": "Session reset. Chat history cleared."}` |
| `reset`   | 同 `new`，重置会话            | 无                        | 同上                                                                     |
| `compact` | 压缩会话上下文                | 无                        | `{"type": "command_result", "command": "compact", "message": "Context compacted. Saved 1000 tokens."}` |
| `stop`    | 停止当前代理运行              | 无                        | `{"type": "command_result", "command": "stop", "message": "Run cancelled."}` |
| `model`   | 查看或切换模型                | 模型名称（可选）          | `{"type": "command_result", "command": "model", "message": "Current model: gpt-4o (provider: openai)"` |
| `usage`   | 查看会话使用情况              | 无                        | `{"type": "command_result", "command": "usage", "message": "Session usage: ~1000 in / ~500 out (~1500 total) | $0.01"}` |
| `context` | 查看上下文报告                | 无                        | `{"type": "command_result", "command": "context", "message": "Context report...", "context_pressure": "low"}` |
| `verbose` | 切换详细程度                  | off/on/full（可选）       | `{"type": "command_result", "command": "verbose", "message": "Verbose level: **full**"}` |
| `queue`   | 查看代理状态                  | 无                        | `{"type": "command_result", "command": "queue", "message": "Agent is idle."}` |
| `budget`  | 查看预算使用情况              | 无                        | `{"type": "command_result", "command": "budget", "message": "Hourly: $0.01 / $10.00  |  Daily: $0.10 / $100.00  |  Monthly: $3.00 / $3000.00"}` |
| `peers`   | 查看网络节点连接情况          | 无                        | `{"type": "command_result", "command": "peers", "message": "No peers connected."}` |
| `a2a`     | 查看外部 A2A 代理发现情况     | 无                        | `{"type": "command_result", "command": "a2a", "message": "No external A2A agents discovered."}` |

## 5. 安全措施

- **消息大小限制**：最大 64KB
- **速率限制**：每连接每分钟最多 10 条消息
- **IP 连接限制**：每 IP 最多 5 个并发连接
- **认证**：必须提供有效的 API 密钥
- **输入 sanitization**：去除控制字符，处理 JSON 信封

## 6. 流式响应处理

WebSocket 支持流式响应，通过 `text_delta` 消息实时传输生成的文本。服务端会对文本增量进行防抖处理：
- **防抖间隔**：100ms
- **字符阈值**：当缓冲区超过 200 字符时立即发送

## 7. 错误处理

服务器会返回用户友好的错误消息，包括：
- 认证错误
- 速率限制错误
- 模型错误（上下文溢出、速率限制、计费问题等）
- 代理错误
- 消息格式错误

## 8. 连接管理

- **心跳**：客户端应定期发送 `ping` 消息以保持连接活跃
- **空闲超时**：30 分钟无活动后自动关闭连接
- **重连**：客户端应实现重连逻辑，在连接断开时重新建立连接

## 9. 示例代码

### 9.1 客户端连接示例（JavaScript）

```javascript
const agentId = "your-agent-id";
const apiKey = "your-api-key";
const wsUrl = `ws://localhost:3000/api/agents/${agentId}/ws?token=${apiKey}`;

const ws = new WebSocket(wsUrl);

ws.onopen = () => {
  console.log("WebSocket connected");
};

ws.onmessage = (event) => {
  const message = JSON.parse(event.data);
  switch (message.type) {
    case "connected":
      console.log("Connected to agent:", message.agent_id);
      break;
    case "typing":
      console.log("Agent is typing:", message.state);
      break;
    case "text_delta":
      console.log("Received text delta:", message.content);
      break;
    case "response":
      console.log("Received complete response:", message.content);
      console.log("Usage:", message.input_tokens, "in,", message.output_tokens, "out");
      break;
    case "error":
      console.error("Error:", message.content);
      break;
    case "agents_updated":
      console.log("Agents updated:", message.agents);
      break;
    case "silent_complete":
      console.log("Agent completed silently");
      break;
    case "canvas":
      console.log("Received canvas:", message.title);
      break;
    case "tool_start":
      console.log("Tool started:", message.tool);
      break;
    case "tool_end":
      console.log("Tool ended:", message.tool);
      break;
    case "tool_result":
      console.log("Tool result:", message.tool, message.result);
      break;
    case "phase":
      console.log("Phase changed:", message.phase);
      break;
    case "pong":
      console.log("Received pong");
      break;
    case "command_result":
      console.log("Command result:", message.command, message.message);
      break;
  }
};

ws.onerror = (error) => {
  console.error("WebSocket error:", error);
};

ws.onclose = () => {
  console.log("WebSocket disconnected");
};

// 发送消息
function sendMessage(content) {
  ws.send(JSON.stringify({
    type: "message",
    content: content
  }));
}

// 发送命令
function sendCommand(command, args = "") {
  ws.send(JSON.stringify({
    type: "command",
    command: command,
    args: args
  }));
}

// 发送 ping
function sendPing() {
  ws.send(JSON.stringify({
    type: "ping"
  }));
}

// 定期发送 ping 以保持连接活跃
setInterval(sendPing, 30000);
```

### 9.2 服务器端实现（Rust）

服务器端使用 Axum 框架实现 WebSocket 处理，主要函数包括：

- `agent_ws`：处理 WebSocket 升级请求
- `handle_agent_ws`：处理 WebSocket 连接
- `handle_text_message`：处理文本消息
- `handle_command`：处理命令
- `map_stream_event`：映射流式事件到 JSON

## 10. 注意事项

- 确保提供有效的 API 密钥进行认证
- 客户端应实现心跳机制以避免空闲超时
- 处理重连逻辑以应对网络中断
- 注意消息大小限制，避免发送过大的消息
- 遵守速率限制，避免发送消息过于频繁
- 对于需要视觉能力的任务，确保使用支持视觉的模型

## 11. 状态码

| 状态码 | 描述                |
|--------|---------------------|
| 200    | 连接成功            |
| 401    | 认证失败            |
| 404    | 代理不存在          |
| 429    | 连接数或速率限制    |

## 12. 总结

WebSocket 模块为 OpenFang 提供了实时双向通信能力，支持流式消息传递、命令执行和状态更新。客户端可以通过 WebSocket 与代理进行实时交互，获取流式响应，并执行各种管理命令。

通过遵循本文档的规范，客户端可以与 OpenFang 服务器建立稳定的 WebSocket 连接，实现实时聊天和代理管理功能。

## 13. 完整消息类型列表

| 类型            | 方向      | 描述                      |
|-----------------|-----------|---------------------------|
| `message`       | 客户端→服务器 | 发送文本消息              |
| `command`       | 客户端→服务器 | 执行命令                  |
| `ping`          | 客户端→服务器 | 发送心跳                  |
| `connected`     | 服务器→客户端 | 连接确认                  |
| `typing`        | 服务器→客户端 | 打字状态更新              |
| `text_delta`    | 服务器→客户端 | 文本增量（流式响应）      |
| `response`      | 服务器→客户端 | 完整响应                  |
| `error`         | 服务器→客户端 | 错误消息                  |
| `agents_updated`| 服务器→客户端 | 代理列表更新              |
| `silent_complete`| 服务器→客户端 | 代理静默完成              |
| `canvas`        | 服务器→客户端 | Canvas 内容               |
| `tool_start`    | 服务器→客户端 | 工具开始执行              |
| `tool_end`      | 服务器→客户端 | 工具执行结束              |
| `tool_result`   | 服务器→客户端 | 工具执行结果              |
| `phase`         | 服务器→客户端 | 阶段变更                  |
| `pong`          | 服务器→客户端 | Ping 响应                 |
| `command_result`| 服务器→客户端 | 命令执行结果              |