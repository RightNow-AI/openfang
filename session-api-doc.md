# OpenFang 会话历史 API 对接文档

## 1. 概述

会话历史 API 提供了获取代理会话历史的功能，包括聊天记录、消息内容和相关元数据。

## 2. 接口详情

### 2.1 接口路径

```
GET /api/agents/:id/session
```

### 2.2 方法

GET

### 2.3 路径参数

| 参数 | 类型 | 描述 | 必需 |
|------|------|------|------|
| `id` | string | 代理 ID | 是 |

### 2.4 查询参数

无

### 2.5 请求头

| 头名称 | 类型 | 描述 | 必需 |
|--------|------|------|------|
| `Authorization` | string | Bearer 认证令牌 | 否（如果配置了 API 密钥则需要） |

### 2.6 响应状态码

| 状态码 | 描述 |
|--------|------|
| 200 OK | 成功获取会话历史 |
| 400 Bad Request | 无效的代理 ID |
| 404 Not Found | 代理不存在 |
| 500 Internal Server Error | 会话加载失败 |

## 3. 响应格式

### 3.1 成功响应

```json
{
  "session_id": "会话 ID",
  "agent_id": "代理 ID",
  "message_count": 消息数量,
  "context_window_tokens": 上下文窗口令牌数,
  "label": "会话标签",
  "messages": [
    {
      "role": "user|assistant|system",
      "content": "消息内容",
      "tools": [
        {
          "name": "工具名称",
          "running": false,
          "expanded": false,
          "result": "工具执行结果预览",
          "is_error": false
        }
      ]
    }
  ]
}
```

### 3.2 错误响应

```json
{
  "error": "错误信息"
}
```

## 4. 字段说明

### 4.1 会话级字段

| 字段 | 类型 | 描述 |
|------|------|------|
| `session_id` | string | 会话的唯一标识符 |
| `agent_id` | string | 关联的代理 ID |
| `message_count` | number | 会话中的消息数量 |
| `context_window_tokens` | number | 上下文窗口的令牌数 |
| `label` | string | 会话的可选标签 |
| `messages` | array | 消息列表 |

### 4.2 消息级字段

| 字段 | 类型 | 描述 |
|------|------|------|
| `role` | string | 消息发送者角色：user（用户）、assistant（助手）或 system（系统） |
| `content` | string | 消息内容，对于图片会显示 "[Image]" |
| `tools` | array | 工具使用信息（仅当消息包含工具使用时存在） |

### 4.3 工具级字段

| 字段 | 类型 | 描述 |
|------|------|------|
| `name` | string | 工具名称 |
| `running` | boolean | 工具是否正在运行 |
| `expanded` | boolean | 工具信息是否展开 |
| `result` | string | 工具执行结果的预览（最多 300 个字符） |
| `is_error` | boolean | 工具执行是否出错 |

## 5. 实现细节

### 5.1 消息处理

- 文本消息：直接返回文本内容
- 图片消息：返回 "[Image]" 占位符
- 工具使用：提取工具名称和执行结果
- 纯工具结果消息：被过滤掉，不包含在响应中

### 5.2 安全措施

- 验证代理 ID 的有效性
- 确保只返回指定代理的会话信息
- 处理会话加载失败的情况

## 6. 示例

### 6.1 请求示例

```bash
# 使用 curl 请求会话历史
curl -X GET "http://localhost:3000/api/agents/12345/session"

# 使用认证令牌
curl -X GET "http://localhost:3000/api/agents/12345/session" \
  -H "Authorization: Bearer your-api-key"
```

### 6.2 响应示例

#### 成功响应

```json
{
  "session_id": "123e4567-e89b-12d3-a456-426614174000",
  "agent_id": "12345",
  "message_count": 3,
  "context_window_tokens": 1000,
  "label": "General Chat",
  "messages": [
    {
      "role": "user",
      "content": "Hello, how are you?"
    },
    {
      "role": "assistant",
      "content": "I'm doing well, thank you! How can I help you today?"
    },
    {
      "role": "user",
      "content": "What's the weather like today?"
    }
  ]
}
```

#### 包含工具使用的响应

```json
{
  "session_id": "123e4567-e89b-12d3-a456-426614174000",
  "agent_id": "12345",
  "message_count": 2,
  "context_window_tokens": 1500,
  "label": "Tool Test",
  "messages": [
    {
      "role": "user",
      "content": "Search for OpenFang project"
    },
    {
      "role": "assistant",
      "content": "Let me search for information about the OpenFang project.",
      "tools": [
        {
          "name": "web_search",
          "running": false,
          "expanded": false,
          "result": "OpenFang is an open-source AI agent framework that provides a unified interface for working with various AI models and tools.",
          "is_error": false
        }
      ]
    }
  ]
}
```

#### 错误响应

```json
// 无效的代理 ID
{
  "error": "Invalid agent ID"
}

// 代理不存在
{
  "error": "Agent not found"
}

// 会话加载失败
{
  "error": "Session load failed"
}
```

## 7. 注意事项

- 会话历史可能包含大量消息，请注意处理响应大小
- 对于包含工具使用的消息，工具结果会被截断为 300 个字符的预览
- 纯工具结果消息会被过滤掉，不包含在响应中
- 图片消息会显示为 "[Image]" 占位符，不包含实际图片数据

## 8. 代码实现

服务器端实现位于 `openfang-api/src/routes.rs` 文件中的 `get_agent_session` 函数：

```rust
/// GET /api/agents/:id/session — Get agent session (conversation history).
pub async fn get_agent_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let agent_id: AgentId = match id.parse() {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid agent ID"})),
            );
        }
    };

    let entry = match state.kernel.registry.get(agent_id) {
        Some(e) => e,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Agent not found"})),
            );
        }
    };

    match state.kernel.memory.get_session(entry.session_id) {
        Ok(Some(session)) => {
            let messages: Vec<serde_json::Value> = session
                .messages
                .iter()
                .filter_map(|m| {
                    let mut tools: Vec<serde_json::Value> = Vec::new();
                    let content = match &m.content {
                        openfang_types::message::MessageContent::Text(t) => t.clone(),
                        openfang_types::message::MessageContent::Blocks(blocks) => {
                            // Extract human-readable text and tool info from blocks
                            let mut texts = Vec::new();
                            for b in blocks {
                                match b {
                                    openfang_types::message::ContentBlock::Text { text } => {
                                        texts.push(text.clone());
                                    }
                                    openfang_types::message::ContentBlock::Image { .. } => {
                                        texts.push("[Image]".to_string());
                                    }
                                    openfang_types::message::ContentBlock::ToolUse {
                                        name, ..
                                    } => {
                                        tools.push(serde_json::json!({
                                            "name": name,
                                            "running": false,
                                            "expanded": false,
                                        }));
                                    }
                                    openfang_types::message::ContentBlock::ToolResult {
                                        content: result,
                                        is_error,
                                        ..
                                    } => {
                                        // Attach result to the most recent tool without a result
                                        if let Some(last_tool) = tools.last_mut() {
                                            let preview: String = result.chars().take(300).collect();
                                            last_tool["result"] = serde_json::Value::String(preview);
                                            last_tool["is_error"] = serde_json::Value::Bool(*is_error);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            texts.join("\n")
                        }
                    };
                    // Skip messages that are purely tool results (User role with only ToolResult blocks)
                    if content.is_empty() && tools.is_empty() {
                        return None;
                    }
                    let mut msg = serde_json::json!({
                        "role": format!("{:?}", m.role),
                        "content": content,
                    });
                    if !tools.is_empty() {
                        msg["tools"] = serde_json::Value::Array(tools);
                    }
                    Some(msg)
                })
                .collect();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "session_id": session.id.0.to_string(),
                    "agent_id": session.agent_id.0.to_string(),
                    "message_count": session.messages.len(),
                    "context_window_tokens": session.context_window_tokens,
                    "label": session.label,
                    "messages": messages,
                })),
            )
        }
        Ok(None) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "session_id": entry.session_id.0.to_string(),
                "agent_id": agent_id.to_string(),
                "message_count": 0,
                "context_window_tokens": 0,
                "messages": [],
            })),
        ),
        Err(e) => {
            tracing::warn!("Session load failed for agent {id}: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Session load failed"
                })),
            )
        }
    }
}
```

## 9. 总结

会话历史 API 提供了一种获取代理聊天记录的方法，返回结构化的会话信息，包括消息内容、工具使用情况等。通过这个接口，客户端可以查看完整的对话历史，了解代理与用户之间的交互过程。

接口设计简洁明了，支持错误处理和边界情况，为客户端提供了可靠的会话历史访问能力。