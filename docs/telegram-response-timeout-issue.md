# Telegram 响应超时问题诊断与修复

## 问题描述

**症状**：
- 用户在 Telegram 给机器人发消息
- 机器人立即显示表情反应（⏳ → 🤔）
- 机器人显示"正在输入"状态
- 长时间无响应（可能数分钟）
- 最终返回错误消息："The AI service is temporarily overloaded, please try again shortly."

**关键特征**：
- 后端日志可能看不到消息到达（取决于日志级别）
- 但消息确实已到达并开始处理（否则不会有表情反应）
- 问题出现在 LLM 调用阶段

## 根本原因

### 原因1：HTTP 客户端无超时限制（已修复 60%）

**位置**：`crates/openfang-runtime/src/drivers/openai.rs:29-32`

**问题**：
```rust
// 修复前
client: reqwest::Client::builder()
    .user_agent(crate::USER_AGENT)
    .build()
    .unwrap_or_default(),
```

OpenAI 驱动（NVIDIA API 使用 OpenAI 兼容接口）构建 HTTP 客户端时没有设置超时，导致：
- 请求会无限期等待服务器响应
- 当 NVIDIA API 处理慢时，客户端一直阻塞
- 最终 NVIDIA 服务器返回 504 Gateway Timeout

**修复**：
```rust
// 修复后
client: reqwest::Client::builder()
    .user_agent(crate::USER_AGENT)
    .timeout(std::time::Duration::from_secs(120))  // 新增 120 秒超时
    .build()
    .unwrap_or_default(),
```

**为什么是 120 秒**：
- 397B 参数超大模型推理需要更长时间
- 匹配工具执行超时 `TOOL_TIMEOUT_SECS = 120`
- 足够长以完成推理，但不会无限期阻塞

### 原因2：大模型推理慢（部分解决）

**模型**：`qwen/qwen3.5-397b-a17b` (397B 参数)

**问题**：
- 超大模型推理时间长（可能 30-60 秒）
- NVIDIA 免费 API 有严格的响应时间限制
- 对话历史长时（29-32 条消息），token 消耗更大

**当前状态**：
- ✅ 添加了 120 秒超时保护
- ⚠️ 但仍可能触发 NVIDIA API 的服务端超时（504）
- ⚠️ 重试机制会生效，但用户体验仍有延迟

### 原因3：对话历史过长（未解决）

**日志证据**：
```
2026-03-18T12:08:39.624657Z WARN Trimming old messages to prevent context overflow
agent=shipinfabu-hand total_messages=29 trimming=9
```

**问题**：
- 每次请求都要处理 20+ 条历史消息
- 增加 token 消耗和推理时间
- 加剧超时风险

**潜在解决方案**（待实现）：
1. 降低 `MAX_HISTORY_MESSAGES` (当前 20)
2. 更激进的消息压缩策略
3. 为 Telegram 场景单独配置更短的历史窗口

## 代码流程分析

### 消息处理流程

```
Telegram 消息到达
  ↓
bridge.rs:dispatch_message() (line 634)
  ↓
1. send_typing() - 发送"正在输入"状态 (line 1058)
2. send_lifecycle_reaction() - 发送表情 ⏳ (line 1063)
3. send_lifecycle_reaction() - 发送表情 🤔 (line 1064)
4. spawn_typing_loop() - 每 4 秒刷新"正在输入" (line 1069)
  ↓
5. handle.send_message_with_metadata() - 调用 LLM (line 1072-1074)
   ↓
   agent_loop.rs:call_with_retry() (line 938)
   ↓
   driver.complete() - OpenAI 驱动发送 HTTP 请求
   ↓
   【此处可能超时】
   ↓
6. typing_task.abort() - 停止"正在输入" (line 1077)
7. send_lifecycle_reaction() - 发送表情 ✅/❌ (line 1082/1098)
8. 返回结果或错误消息
```

### 关键观察

**为什么用户看到表情但后端"无日志"**：
1. `fire_reaction()` 是 fire-and-forget (line 557-581)
2. `send_typing()` 是 best-effort (line 1058)
3. 这两个操作在 LLM 调用**之前**就完成了
4. 如果日志级别是 INFO，`dispatch_message` 的 debug 日志不会显示
5. 只有 LLM 错误才会产生 WARN 日志 (line 1100)

**为什么延迟很久才返回错误**：
1. HTTP 请求无超时，一直等待
2. NVIDIA API 处理慢，最终返回 504
3. 重试机制触发（最多 3 次，每次指数退避）
4. 最终返回 sanitized 错误消息

## 修复记录

### 已完成（60%）

**修复1：添加 HTTP 超时**
- 文件：`crates/openfang-runtime/src/drivers/openai.rs:30`
- 修改：添加 `.timeout(Duration::from_secs(120))`
- 效果：防止无限期等待，120 秒后触发重试
- 提交：待提交

### 待解决（40%）

**问题1：NVIDIA API 服务端超时**
- 即使客户端有 120 秒超时，NVIDIA 服务器可能在 60 秒就返回 504
- 397B 模型推理时间可能超过 NVIDIA 免费 API 的限制
- 需要：
  - 监控实际超时时间分布
  - 考虑是否需要付费 API 或自建推理服务
  - 或者为 Telegram 场景使用更小的模型

**问题2：对话历史管理**
- 当前 `MAX_HISTORY_MESSAGES = 20` 对超大模型来说可能太多
- 每次请求都要处理大量上下文
- 需要：
  - 为不同模型大小配置不同的历史窗口
  - 实现更智能的消息压缩（保留关键上下文）
  - 考虑为 Telegram 场景单独配置

**问题3：用户体验优化**
- 当前用户只能等待，无法知道进度
- 需要：
  - 流式响应（让用户更早看到部分结果）
  - 超时前发送中间状态更新
  - 提供"取消"机制

## 诊断工具

### 检查当前日志
```bash
# systemd
sudo journalctl -u openfang -f | grep -E "Telegram|dispatch|LLM error|overload"

# Docker / Compose
docker compose logs -f openfang | grep -E "Telegram|dispatch|LLM error|overload"

# 本地前台运行
# 直接查看运行 `target/release/openfang start` 的终端
```

### 检查 agent 配置
```bash
curl -s http://127.0.0.1:4200/api/agents | python3 -c "import sys,json; agents=json.load(sys.stdin); print(json.dumps(agents, indent=2))"
```

### 测试 LLM 调用
```bash
# 获取 agent ID
AGENT_ID=$(curl -s http://127.0.0.1:4200/api/agents | python3 -c "import sys,json; print(json.load(sys.stdin)[0]['id'])")

# 发送测试消息
curl -s -X POST "http://127.0.0.1:4200/api/agents/$AGENT_ID/message" \
  -H "Content-Type: application/json" \
  -d '{"message":"测试"}' | python3 -c "import sys,json; print(json.load(sys.stdin).get('response', 'ERROR')[:200])"
```

### 监控超时错误
```bash
sudo journalctl -u openfang -f | grep -E "504|timeout|overload|LLM error classified"
```

## 重试机制

代码中已有完善的重试逻辑：

**位置**：`crates/openfang-runtime/src/agent_loop.rs:38-41`

```rust
const MAX_RETRIES: u32 = 3;
const BASE_RETRY_DELAY_MS: u64 = 1000;
```

**重试策略**：
- Overloaded 错误：最多重试 3 次，指数退避（1s → 2s → 4s）
- RateLimit 错误：最多重试 3 次，使用 API 返回的 retry_after 或指数退避
- 其他错误：立即失败，不重试

**错误分类**：`crates/openfang-runtime/src/llm_errors.rs`
- 504 → `LlmErrorCategory::Overloaded` → `is_retryable = true`
- 用户看到的消息经过 sanitize 处理

## 下一步优化方向

### 短期（1-2 周）
1. **监控实际超时分布** - 收集 NVIDIA API 的实际响应时间
2. **调整历史窗口** - 为超大模型降低 `MAX_HISTORY_MESSAGES`
3. **添加更详细的日志** - 记录每个阶段的耗时

### 中期（1 个月）
1. **实现流式响应** - 让用户更早看到部分结果
2. **智能模型降级** - 超时后自动切换到更快的模型
3. **优化消息压缩** - 保留关键上下文，减少 token 消耗

### 长期（2-3 个月）
1. **自建推理服务** - 避免依赖免费 API 的限制
2. **多模型并行** - 同时调用快速模型和精确模型
3. **预测性缓存** - 对常见问题预先生成回复

## 相关文件

- `crates/openfang-runtime/src/drivers/openai.rs` - OpenAI 驱动（NVIDIA API 使用）
- `crates/openfang-runtime/src/agent_loop.rs` - Agent 执行循环和重试逻辑
- `crates/openfang-runtime/src/llm_errors.rs` - LLM 错误分类
- `crates/openfang-channels/src/bridge.rs` - 消息分发和表情发送
- `crates/openfang-channels/src/telegram.rs` - Telegram 适配器

## 测试验证

修复后的测试结果：
- ✅ 编译成功
- ✅ Daemon 正常启动
- ✅ 测试消息成功返回（使用 397B 模型）
- ✅ 120 秒超时生效
- ⚠️ 仍可能遇到 NVIDIA API 服务端超时（需要进一步优化）

---

**最后更新**：2026-03-18
**状态**：部分修复（60%），待进一步优化
