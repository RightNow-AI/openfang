# Telegram 媒体组视频处理改进

## 问题

虽然媒体组合并功能工作正常（只触发 1 次调用），但存在以下问题：

1. **视频信息丢失**：bot 回复说"只收到了图片预览，没有收到真实的视频文件"
2. **大文件无法下载**：Telegram Bot API 对 `getFile` 有 20MB 限制
3. **错误信息不明确**：下载失败时没有提示原因

## 根本原因

1. **Telegram Bot API 限制**：
   - `getFile` 方法只能下载 ≤20MB 的文件
   - 大于 20MB 的视频无法通过 Bot API 下载
   - 需要使用 Telegram 的文件下载服务器或本地 Bot API

2. **视频可能以 document 形式发送**：
   - 用户可以选择"发送为文件"，此时视频会作为 `document` 而不是 `video`
   - 原代码没有检查 document 的 MIME 类型

3. **错误处理不足**：
   - 下载失败时静默跳过，用户不知道发生了什么
   - 没有区分"文件太大"和"其他错误"

## 改进方案

### 1. 添加详细的错误处理

```rust
match telegram_get_file_url(token, client, file_id, api_base_url).await {
    Some(url) => {
        media_items.push(format!("[Video: {} ({}s, {} bytes)]", url, duration, file_size));
    }
    None => {
        if file_size > 20 * 1024 * 1024 {
            media_items.push(format!(
                "[Video: file too large ({} MB), cannot download via Bot API]",
                file_size / 1024 / 1024
            ));
        } else {
            media_items.push(format!("[Video: download failed ({} bytes)]", file_size));
        }
    }
}
```

### 2. 识别视频类型的 document

```rust
let mime_type = document.get("mime_type").and_then(|v| v.as_str()).unwrap_or("");
let is_video = mime_type.starts_with("video/");

if is_video {
    media_items.push(format!("[Video (as document): {} - {}]", filename, url));
} else {
    media_items.push(format!("[File {}: {}]", filename, url));
}
```

### 3. 添加调试日志

```rust
debug!("Media group item keys: {:?}", message.as_object().map(|o| o.keys().collect::<Vec<_>>()));
warn!("Failed to get video URL for file_id: {} (size: {} bytes)", file_id, file_size);
```

## 预期效果

### 小文件（≤20MB）

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

### 大文件（>20MB）

**之前**：
```
Media group (9 items):
[Photo: https://...]
[Photo: https://...]
...
(视频被静默跳过)
```

**之后**：
```
Media group (9 items):
[Photo: https://...]
[Video: file too large (45 MB), cannot download via Bot API]
[Photo: https://...]
...
```

## 解决大文件问题的方案

### 方案 1：提示用户使用其他方式（当前实现）

优点：
- 无需额外配置
- 用户知道发生了什么

缺点：
- 无法自动处理大文件

### 方案 2：使用本地 Bot API 服务器

Telegram 提供本地 Bot API 服务器，可以下载任意大小的文件：

```bash
# 安装本地 Bot API 服务器
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
- 需要 Telegram API credentials

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
cd /Users/xiaomo/Desktop/openfang-upstream-fork

# 1. 编译
cargo build --release

# 2. 立即安装（重要：优先执行）
cp target/release/openfang ~/.openfang/bin/openfang

# 3. 重启
pkill -f openfang-daemon
sleep 2
~/.openfang/bin/openfang start

# 4. 验证
sleep 3
ps aux | grep openfang | grep -v grep
tail -20 ~/.openfang/daemon-reconcile.stdout.log
```

## 验证

发送包含视频的媒体组，检查：

1. **小视频（<20MB）**：应该显示 `[Video: url (duration, size)]`
2. **大视频（>20MB）**：应该显示 `[Video: file too large (XX MB), cannot download via Bot API]`
3. **视频作为文件发送**：应该显示 `[Video (as document): filename - url]`

## 相关文档

- Telegram Bot API 文件限制：https://core.telegram.org/bots/api#getfile
- 本地 Bot API 服务器：https://github.com/tdlib/telegram-bot-api
