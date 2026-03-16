# Telegram 媒体组视频处理改进

## 问题

虽然媒体组合并功能工作正常（只触发 1 次调用），但存在以下问题：

1. **视频信息丢失**：bot 回复说"只收到了图片预览，没有收到真实的视频文件"
2. **下载失败原因被误判**：旧逻辑把失败直接归因为固定的 20MB 限制
3. **错误信息不明确**：失败时无法告诉操作者该从哪里继续排查

## 根本原因

1. **下载失败并不只有一种原因**：
   - 可能是云端 Bot API 限制
   - 也可能是网络、权限、代理或 Telegram 侧返回异常
   - 某些环境（例如本地 Bot API Server）本来就能处理更大的文件

2. **视频可能以 document 形式发送**：
   - 用户可以选择"发送为文件"，此时视频会作为 `document` 而不是 `video`
   - 原代码没有检查 document 的 MIME 类型

3. **错误处理不足**：
   - 下载失败时只给出过于武断的结论
   - 没有提供 `file_id` 之类的排障线索
   - 没有提示可以改用本地路径或下载链接

## 改进方案

### 1. 失败时不再硬编码 20MB 结论

```rust
match telegram_get_file_url(token, client, file_id, api_base_url).await {
    Some(url) => {
        media_items.push(format!("[Video: {} ({}s, {} bytes)]", url, duration, file_size));
    }
    None => {
        media_items.push(format!(
            "[Video: download failed (file_id: {}, {} MB) - may need local path or download link]",
            file_id,
            file_size / 1024 / 1024
        ));
    }
}
```

### 2. 识别视频类型的 document

```rust
let mime_type = document.get("mime_type").and_then(|v| v.as_str()).unwrap_or("");
let is_video = mime_type.starts_with("video/");

if is_video {
    media_items.push(format!(
        "[Video {}: download failed (file_id: {}, {} MB) - may need local path or download link]",
        filename,
        file_id,
        file_size / 1024 / 1024
    ));
} else {
    media_items.push(format!("[File {}: download failed ({} MB)]", filename, file_size / 1024 / 1024));
}
```

### 3. 添加调试日志

```rust
warn!("Failed to get video URL for file_id: {} (size: {} bytes)", file_id, file_size);
```

## 预期效果

### 下载成功

**之前**：
```
Media group (9 items):
[Photo: https://...]
[Photo: https://...]
...
```

**之后**：
```
Media group (9 items):
[Photo: https://...]
[Video: https://... (120s, 15728640 bytes)]
[Photo: https://...]
...
```

### 下载失败

**之前**：
```
Media group (9 items):
[Photo: https://...]
[Photo: https://...]
...
[Video: file too large (45 MB), cannot download via Bot API]
```

**之后**：
```
Media group (9 items):
[Photo: https://...]
[Video: download failed (file_id: xxx, 45 MB) - may need local path or download link]
[Photo: https://...]
...
```

## 对大文件的实际建议

### 方案 1：提示用户提供其他可访问来源（当前实现）

优点：
- 无需额外配置
- 用户知道发生了什么
- 不会把所有失败都误判成固定大小限制

缺点：
- 无法自动处理大文件

### 方案 2：使用本地 Bot API 服务器

如果你的环境确实依赖更大的 Telegram 文件下载能力，可以部署本地
Bot API Server：

```bash
docker run -d -p 8081:8081 \
  -e TELEGRAM_API_ID=your_api_id \
  -e TELEGRAM_API_HASH=your_api_hash \
  aiogram/telegram-bot-api:latest
```

然后在 OpenFang 配置中指定：

```toml
[channels.telegram]
api_url = "http://localhost:8081"
```

优点：
- 可以下载任意大小的文件
- 更快的下载速度

缺点：
- 需要额外部署服务
- 需要 Telegram API 凭据

### 方案 3：让用户提供直接链接

在 Hand 的提示词中说明：

```
如果视频文件 >20MB，请：
1. 上传到云存储（如 Google Drive、Dropbox）
2. 提供公开下载链接
3. 或者提供本地文件路径
```

## 部署

```bash
cargo build --release -p openfang-cli
openfang stop
target/release/openfang start
```

## 验证

发送包含视频的媒体组，检查：

1. **可下载视频**：应继续显示真实下载地址
2. **下载失败的视频**：应显示带 `file_id` 和大小的通用失败消息
3. **作为 document 发送的视频**：应继续被识别成视频，而不是普通文件

## 相关文档

- Telegram Bot API `getFile`：https://core.telegram.org/bots/api#getfile
- 本地 Bot API 服务器：https://github.com/tdlib/telegram-bot-api
