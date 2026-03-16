# Telegram 媒体组修复

## 问题描述

当用户在 Telegram 中发送媒体组（多张图片/视频）时，OpenFang 会触发 9 次独立的 agent 调用，导致：

1. 每张图片都被当作独立消息处理
2. 触发 9 次 "Agent not found: 6680daef-ba64-4a94-95e6-f3abeadcf251" 错误
3. 如果 agent 在处理过程中被重建，后续调用会失败

## 根本原因

OpenFang 的 Telegram 适配器 (`crates/openfang-channels/src/telegram.rs`) **完全没有处理 `media_group_id`**。

当用户发送媒体组时，Telegram API 会为每张媒体单独发送一个 update，每个 update 都有相同的 `media_group_id`。OpenFang 当前的实现把每个 update 都当作独立消息处理。

## 修复方案

在 `telegram.rs` 中实现媒体组合并逻辑：

### 1. 检测 `media_group_id`

在主循环中检查每个 update 是否包含 `media_group_id`：

```rust
let media_group_id = message
    .and_then(|m| m.get("media_group_id"))
    .and_then(|v| v.as_str())
    .map(String::from);
```

### 2. 缓存同组消息

使用 HashMap 暂存同一组的 updates：

```rust
let mut media_groups: HashMap<String, (Vec<serde_json::Value>, tokio::time::Instant)> = HashMap::new();
const MEDIA_GROUP_WAIT_MS: u64 = 500; // 等待 500ms 收集所有媒体
```

### 3. 延迟处理

设置 500ms 延迟，等待同组的所有消息到达后再处理：

```rust
if let Some(group_id) = media_group_id {
    let entry = media_groups.entry(group_id).or_insert_with(|| (Vec::new(), now));
    entry.0.push(update.clone());
    entry.1 = now; // 更新最后接收时间
    continue; // 不立即处理
}
```

### 4. 合并消息

实现 `merge_media_group_updates()` 函数，将多个 update 合并成一条消息：

```rust
async fn merge_media_group_updates(
    updates: &[serde_json::Value],
    ...
) -> Option<ChannelMessage> {
    // 使用第一个 update 的元数据（发送者、聊天、时间戳）
    // 收集所有媒体 URL 和 caption
    // 合并成一条文本消息
}
```

## 修改的文件

- `crates/openfang-channels/src/telegram.rs`
  - 第 451-587 行：修改主循环，添加媒体组检测和缓存逻辑
  - 第 698-778 行：新增 `merge_media_group_updates()` 函数

## 部署步骤

1. 编译项目：
   ```bash
   cd /Users/xiaomo/Desktop/openfang-upstream-fork
   cargo build --release
   ```

2. 部署到服务器：
   ```bash
   export SHIPINBOT_CLUSTER_PASSWORD='your-password'
   ./deploy-with-sshpass.sh
   ```

3. 验证修复：
   - 在 Telegram 中发送媒体组（多张图片）
   - 验证只触发一次 agent 调用
   - 不再出现 9 次重复的 "Agent not found" 错误

## 预期效果

修复后：
- 发送 9 张图片的媒体组 → 只触发 1 次 agent 调用
- Agent 收到的消息格式：
  ```
  [用户的 caption]

  Media group (9 items):
  [Photo: https://api.telegram.org/file/bot.../photo1.jpg]
  [Photo: https://api.telegram.org/file/bot.../photo2.jpg]
  ...
  ```

## 技术细节

### 为什么是 500ms 延迟？

Telegram API 在发送媒体组时，会在极短时间内（通常 < 100ms）连续发送所有 update。500ms 的延迟足够收集所有媒体，同时不会让用户感觉到明显的延迟。

### 为什么合并成文本消息？

因为 `ChannelContent` 枚举目前不支持多媒体消息类型。将媒体 URL 列表作为文本发送给 agent，agent 可以：
1. 看到完整的媒体组上下文
2. 根据需要下载和处理每个媒体文件
3. 只触发一次处理流程

### 边界情况处理

1. **单张图片**：不会被识别为媒体组（没有 `media_group_id`），立即处理
2. **混合消息**：如果媒体组中有 caption，只保留第一个 caption
3. **超时清理**：每次循环都检查超时的媒体组并处理，避免内存泄漏

## 相关问题

这个修复同时解决了之前的 Agent ID 缓存问题。当 agent 被重建时：
1. 媒体组只触发一次调用，减少了竞态条件
2. 即使 agent ID 过期，也只会失败一次而不是 9 次

## 测试建议

1. **正常媒体组**：发送 2-10 张图片，验证只触发一次调用
2. **带 caption**：发送带文字说明的媒体组，验证 caption 被正确提取
3. **单张图片**：发送单张图片，验证不受影响
4. **混合测试**：交替发送单张图片和媒体组，验证都能正确处理
