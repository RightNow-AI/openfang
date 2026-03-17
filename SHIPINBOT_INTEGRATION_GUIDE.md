# shipinbot 项目侧集成指南

## 概述

本文档说明 shipinbot 项目需要实现的功能，以支持 OpenFang Telegram 媒体组结构化处理。

## 背景

OpenFang v0.4.4+ 将 Telegram 媒体组改为结构化 `telegram_media_batch`，并自动写入：
```
~/.openfang/workspaces/shipinfabu-hand/inbox/telegram/<batch_key>.json
```

shipinbot 需要实现两个新命令和更新 hand 对话策略，以支持选择性视频下载。

---

## 1. 新增命令：`collect-telegram-batch`

### 功能

读取 OpenFang 写入的 inbox manifest，将已就绪的媒体收进 intake 目录，生成 batch state。

### 命令签名

```bash
python openfang_clean_publish_bridge.py collect-telegram-batch \
  --manifest <abs-path-to-batch-json> \
  [--output-dir <intake-dir>]
```

### 输入

**Manifest 文件示例** (`~/.openfang/workspaces/shipinfabu-hand/inbox/telegram/group-123_456_789.json`):

```json
{
  "batch_key": "group-123_456_789",
  "chat_id": 456,
  "message_id": 789,
  "media_group_id": "group-123",
  "caption": "这是用户的说明文字",
  "items": [
    {
      "kind": "video",
      "file_id": "BAACAgIAAxkBAAI...",
      "original_name": null,
      "file_size": 150000000,
      "duration_seconds": 30,
      "status": "needs_project_download",
      "local_path": null,
      "download_hint": "Video exceeds 100MB safe limit, needs project-side download"
    },
    {
      "kind": "image",
      "file_id": "AgACAgIAAxkBAAI...",
      "original_name": null,
      "file_size": 500000,
      "duration_seconds": null,
      "status": "ready",
      "local_path": "/tmp/openfang-telegram-downloads/photo_123.jpg",
      "download_hint": null
    },
    {
      "kind": "image",
      "file_id": "AgACAgIAAxkBAAI...",
      "original_name": null,
      "file_size": 480000,
      "duration_seconds": null,
      "status": "ready",
      "local_path": "/tmp/openfang-telegram-downloads/photo_124.jpg",
      "download_hint": null
    }
  ]
}
```

### 处理逻辑

1. **读取 manifest**
   ```python
   import json
   with open(manifest_path) as f:
       batch = json.load(f)
   ```

2. **创建 intake 目录**
   ```python
   batch_id = batch['batch_key']
   intake_dir = f"{local_media_intake_dir}/openfang-intake/{batch_id}"
   os.makedirs(intake_dir, exist_ok=True)
   ```

3. **收集已就绪的媒体**
   ```python
   reference_images = []
   pending_videos = []

   for idx, item in enumerate(batch['items']):
       if item['status'] == 'ready' and item['local_path']:
           # 复制或移动到 intake 目录
           src = item['local_path']
           ext = os.path.splitext(src)[1] or '.jpg'
           dst = f"{intake_dir}/{item['kind']}_{idx}{ext}"
           shutil.copy2(src, dst)

           if item['kind'] == 'image':
               reference_images.append(dst)

       elif item['status'] in ['needs_project_download', 'skipped_safe_limit']:
           if item['kind'] == 'video':
               pending_videos.append({
                   'index': idx,
                   'file_id': item['file_id'],
                   'file_size': item['file_size'],
                   'duration_seconds': item.get('duration_seconds'),
                   'download_hint': item.get('download_hint')
               })
   ```

4. **生成 current_batch.json**
   ```python
   current_batch = {
       'batch_id': batch_id,
       'telegram_manifest': manifest_path,
       'caption': batch.get('caption'),
       'reference_images': reference_images,
       'pending_video_items': pending_videos,
       'selected_video_index': None,  # 待 hand 选择
       'suggested_source_video': None,  # 待下载后填充
       'download_status': 'pending' if pending_videos else 'no_video'
   }

   with open(f"{intake_dir}/current_batch.json", 'w') as f:
       json.dump(current_batch, f, indent=2)
   ```

5. **生成 reply_hint**
   ```python
   if len(pending_videos) == 0:
       reply_hint = "批次中没有视频，只有图片。"
   elif len(pending_videos) == 1:
       video = pending_videos[0]
       size_mb = video['file_size'] / 1024 / 1024
       duration = video.get('duration_seconds', '?')
       reply_hint = f"批次中有 1 个视频（{size_mb:.1f}MB, {duration}秒）待下载。"
   else:
       reply_hint = f"批次中有 {len(pending_videos)} 个视频，需要用户选择下载哪个。"

   return reply_hint
   ```

### 输出

- **Intake 目录**: `{local_media_intake_dir}/openfang-intake/{batch_id}/`
  - `image_0.jpg`, `image_1.jpg`, ... (已就绪的图片)
  - `current_batch.json` (批次状态)

- **返回值**: `reply_hint` 字符串，供 hand 使用

### 错误处理

- Manifest 文件不存在 → 返回错误
- 所有媒体项都是 `download_failed` → 返回 "批次中所有媒体下载失败"
- Intake 目录创建失败 → 返回错误

---

## 2. 新增命令：`fetch-telegram-video`

### 功能

下载 hand 选中的视频，更新 batch state。

### 命令签名

```bash
python openfang_clean_publish_bridge.py fetch-telegram-video \
  --batch-dir <intake-dir> \
  --video-index <n>
```

### 输入

- `--batch-dir`: intake 目录路径（如 `{local_media_intake_dir}/openfang-intake/group-123_456_789`）
- `--video-index`: 视频在 `pending_video_items` 中的索引

### 处理逻辑

1. **读取 current_batch.json**
   ```python
   batch_state_path = f"{batch_dir}/current_batch.json"
   with open(batch_state_path) as f:
       batch_state = json.load(f)
   ```

2. **读取原始 manifest**
   ```python
   manifest_path = batch_state['telegram_manifest']
   with open(manifest_path) as f:
       manifest = json.load(f)
   ```

3. **获取视频信息**
   ```python
   video_item = batch_state['pending_video_items'][video_index]
   file_id = video_item['file_id']
   ```

4. **调用 Telegram Bot API 下载**

   **方法 A：使用 Local Bot API Server**
   ```python
   import requests

   bot_token = os.getenv('TELEGRAM_BOT_TOKEN')
   api_url = os.getenv('TELEGRAM_API_URL', 'http://localhost:8081')

   # 1. getFile
   resp = requests.post(
       f"{api_url}/bot{bot_token}/getFile",
       json={'file_id': file_id}
   )
   file_info = resp.json()['result']
   file_path = file_info['file_path']

   # 2. 下载文件
   download_url = f"{api_url}/file/bot{bot_token}/{file_path}"
   video_resp = requests.get(download_url, stream=True)

   # 3. 保存到 intake 目录
   ext = os.path.splitext(file_path)[1] or '.mp4'
   local_path = f"{batch_dir}/video_{video_index}{ext}"

   with open(local_path, 'wb') as f:
       for chunk in video_resp.iter_content(chunk_size=8192):
           f.write(chunk)
   ```

   **方法 B：使用 python-telegram-bot 库**
   ```python
   from telegram import Bot

   bot = Bot(token=os.getenv('TELEGRAM_BOT_TOKEN'))
   bot.base_url = os.getenv('TELEGRAM_API_URL', 'http://localhost:8081')

   file = bot.get_file(file_id)
   local_path = f"{batch_dir}/video_{video_index}.mp4"
   file.download(local_path)
   ```

5. **更新 batch state**
   ```python
   batch_state['selected_video_index'] = video_index
   batch_state['suggested_source_video'] = local_path
   batch_state['download_status'] = 'completed'

   with open(batch_state_path, 'w') as f:
       json.dump(batch_state, f, indent=2)
   ```

### 输出

- **下载的视频**: `{batch_dir}/video_{index}.mp4`
- **更新的 current_batch.json**: `suggested_source_video` 指向真实本地文件
- **返回值**: 成功消息或错误信息

### 错误处理

- `video_index` 越界 → 返回错误
- Telegram API 调用失败 → 返回错误（包含 API 错误信息）
- 磁盘空间不足 → 返回错误
- 下载超时 → 返回错误（建议设置 timeout=300 秒）

---

## 3. 更新 `shipinfabu-hand` 对话策略

### 当前问题

现有策略依赖聊天正文里的 `"[Photo: ...]"` 和 `"[Video: ...]"` 文本，无法精确判断批次结构。

### 新策略

#### 3.1 入口逻辑

```python
# 优先检查是否有 inbox manifest
inbox_dir = "~/.openfang/workspaces/shipinfabu-hand/inbox/telegram"
manifests = sorted(glob.glob(f"{inbox_dir}/*.json"))

if manifests:
    # 有待处理的 Telegram 批次
    latest_manifest = manifests[-1]
    handle_telegram_batch(latest_manifest)
else:
    # 检查是否有 current_batch.json
    if os.path.exists("current_batch.json"):
        handle_existing_batch()
    else:
        # 普通消息处理
        handle_normal_message()
```

#### 3.2 处理新批次

```python
def handle_telegram_batch(manifest_path):
    # 1. 调用 collect-telegram-batch
    result = subprocess.run([
        'python', 'openfang_clean_publish_bridge.py',
        'collect-telegram-batch',
        '--manifest', manifest_path
    ], capture_output=True, text=True)

    reply_hint = result.stdout.strip()

    # 2. 读取 batch state
    with open('current_batch.json') as f:
        batch = json.load(f)

    # 3. 决策
    pending_videos = batch['pending_video_items']

    if len(pending_videos) == 0:
        # 纯图片批次
        return "收到图片批次，准备处理。" + process_image_only_batch()

    elif len(pending_videos) == 1:
        # 单视频批次
        if user_intent_is_clear():
            # 用户意图明确（如"发布这个视频"），直接下载
            download_video(0)
            return "视频已下载，准备处理。" + process_batch()
        else:
            # 用户意图不明确，先确认
            return f"收到 1 个视频和 {len(batch['reference_images'])} 张图片。请确认是否要处理这个视频？"

    else:
        # 多视频批次，必须让用户选择
        video_list = "\n".join([
            f"{i+1}. 视频 {v['file_size']/1024/1024:.1f}MB, {v.get('duration_seconds', '?')}秒"
            for i, v in enumerate(pending_videos)
        ])
        return f"收到 {len(pending_videos)} 个视频，请选择要处理的视频：\n{video_list}"
```

#### 3.3 下载选中视频

```python
def download_video(video_index):
    batch_dir = get_current_batch_dir()

    result = subprocess.run([
        'python', 'openfang_clean_publish_bridge.py',
        'fetch-telegram-video',
        '--batch-dir', batch_dir,
        '--video-index', str(video_index)
    ], capture_output=True, text=True)

    if result.returncode != 0:
        raise Exception(f"视频下载失败: {result.stderr}")

    # 重新读取 batch state，获取 suggested_source_video
    with open(f"{batch_dir}/current_batch.json") as f:
        batch = json.load(f)

    return batch['suggested_source_video']
```

#### 3.4 用户意图判断

```python
def user_intent_is_clear():
    """
    判断用户这轮消息是否包含明确的处理意图
    """
    user_message = get_latest_user_message().lower()

    intent_keywords = [
        '发布', '提交', '上传', '处理这个',
        'publish', 'submit', 'upload', 'process this'
    ]

    return any(kw in user_message for kw in intent_keywords)
```

#### 3.5 处理用户选择

```python
def handle_user_video_selection(user_message):
    """
    解析用户选择的视频编号
    """
    # 尝试提取数字
    import re
    match = re.search(r'\b(\d+)\b', user_message)

    if match:
        selection = int(match.group(1)) - 1  # 转为 0-based index

        with open('current_batch.json') as f:
            batch = json.load(f)

        if 0 <= selection < len(batch['pending_video_items']):
            download_video(selection)
            return "视频已下载，准备处理。" + process_batch()
        else:
            return f"无效的选择，请输入 1 到 {len(batch['pending_video_items'])} 之间的数字。"

    return "请输入要处理的视频编号。"
```

---

## 4. 完整工作流示例

### 场景 A：单视频 + 明确意图

**用户发送**：1 个视频 + 9 张图片，消息："发布这个视频"

**OpenFang**：
1. 生成 `telegram_media_batch`
2. 写入 `~/.openfang/workspaces/shipinfabu-hand/inbox/telegram/group-123_456_789.json`
3. 发送给 hand："收到 Telegram 媒体批次：1 个视频、9 张图片。"

**shipinfabu-hand**：
1. 检测到 inbox manifest
2. 调用 `collect-telegram-batch`
3. 判断：单视频 + 用户意图明确
4. 调用 `fetch-telegram-video --video-index 0`
5. 继续处理：`process_batch()`

**结果**：直接处理，无需额外确认

---

### 场景 B：多视频批次

**用户发送**：3 个视频 + 5 张图片，消息："帮我处理一下"

**OpenFang**：
1. 生成 `telegram_media_batch`
2. 写入 inbox manifest
3. 发送给 hand："收到 Telegram 媒体批次：3 个视频、5 张图片。"

**shipinfabu-hand**：
1. 检测到 inbox manifest
2. 调用 `collect-telegram-batch`
3. 判断：多视频批次
4. 回复用户：
   ```
   收到 3 个视频，请选择要处理的视频：
   1. 视频 120.5MB, 25秒
   2. 视频 85.3MB, 18秒
   3. 视频 200.1MB, 40秒
   ```

**用户回复**：`"第 2 个"`

**shipinfabu-hand**：
1. 解析选择：index = 1
2. 调用 `fetch-telegram-video --video-index 1`
3. 继续处理：`process_batch()`

**结果**：只下载被选中的视频

---

### 场景 C：纯图片批次

**用户发送**：10 张图片，消息："这些图片做个合集"

**OpenFang**：
1. 生成 `telegram_media_batch`（items 全是 image）
2. 写入 inbox manifest
3. 发送给 hand："收到 Telegram 媒体批次：10 张图片。"

**shipinfabu-hand**：
1. 检测到 inbox manifest
2. 调用 `collect-telegram-batch`
3. 判断：无视频，纯图片
4. 直接处理：`process_image_only_batch()`

**结果**：按纯图文任务处理

---

## 5. 配置与环境变量

### 必需环境变量

```bash
export TELEGRAM_BOT_TOKEN="你的bot_token"
export TELEGRAM_API_URL="http://localhost:8081"  # Local Bot API Server
```

### 可选配置

```python
# openfang_clean_publish_bridge.py 配置
LOCAL_MEDIA_INTAKE_DIR = os.getenv(
    'SHIPINBOT_INTAKE_DIR',
    '/path/to/shipinbot/media_intake'
)

TELEGRAM_DOWNLOAD_TIMEOUT = int(os.getenv(
    'TELEGRAM_DOWNLOAD_TIMEOUT',
    '300'  # 5 分钟
))
```

---

## 6. 测试清单

### 单元测试

- [ ] `collect-telegram-batch` 能正确解析 manifest
- [ ] `collect-telegram-batch` 能复制 `status=ready` 的媒体
- [ ] `collect-telegram-batch` 能生成正确的 `current_batch.json`
- [ ] `fetch-telegram-video` 能下载视频并更新 batch state
- [ ] `fetch-telegram-video` 处理下载失败的情况

### 集成测试

- [ ] 场景 A：单视频 + 明确意图 → 直接处理
- [ ] 场景 B：多视频 → 用户选择 → 下载选中的
- [ ] 场景 C：纯图片 → 直接处理
- [ ] 超大视频（>100MB）能正常下载
- [ ] 下载失败时有清晰的错误提示

---

## 7. 注意事项

### 7.1 并发安全

- inbox manifest 可能同时有多个文件，按时间戳排序处理
- 处理完成后删除或移动 manifest，避免重复处理

### 7.2 磁盘空间

- 下载前检查磁盘空间（至少预留 2GB）
- 下载失败时清理临时文件

### 7.3 超时处理

- 大视频下载可能需要 5-10 分钟
- 设置合理的 timeout（建议 300 秒）
- 超时后提示用户稍后重试

### 7.4 错误恢复

- 下载中断后支持断点续传（如果 Telegram API 支持）
- 或者清理临时文件，重新下载

---

## 8. 实现优先级

### P0（必须实现）

1. `collect-telegram-batch` 命令
2. `fetch-telegram-video` 命令
3. hand 入口逻辑（检测 inbox manifest）

### P1（重要）

4. 单视频 + 明确意图 → 直接处理
5. 多视频 → 用户选择

### P2（优化）

6. 纯图片批次优化
7. 错误恢复和重试
8. 下载进度提示

---

## 9. 参考资料

- OpenFang Telegram 媒体组实现：`TELEGRAM_MEDIA_BATCH_IMPLEMENTATION.md`
- Telegram Bot API 文档：https://core.telegram.org/bots/api#getfile
- Local Bot API Server：https://github.com/tdlib/telegram-bot-api

---

## 附录：完整代码示例

### A. collect-telegram-batch 实现骨架

```python
def collect_telegram_batch(manifest_path, output_dir=None):
    """
    收集 Telegram 媒体批次到 intake 目录
    """
    # 1. 读取 manifest
    with open(manifest_path) as f:
        batch = json.load(f)

    batch_id = batch['batch_key']
    intake_dir = output_dir or f"{LOCAL_MEDIA_INTAKE_DIR}/openfang-intake/{batch_id}"
    os.makedirs(intake_dir, exist_ok=True)

    # 2. 收集媒体
    reference_images = []
    pending_videos = []

    for idx, item in enumerate(batch['items']):
        if item['status'] == 'ready' and item['local_path']:
            src = item['local_path']
            ext = os.path.splitext(src)[1] or ('.jpg' if item['kind'] == 'image' else '.dat')
            dst = f"{intake_dir}/{item['kind']}_{idx}{ext}"
            shutil.copy2(src, dst)

            if item['kind'] == 'image':
                reference_images.append(dst)

        elif item['status'] in ['needs_project_download', 'skipped_safe_limit']:
            if item['kind'] == 'video':
                pending_videos.append({
                    'index': idx,
                    'file_id': item['file_id'],
                    'file_size': item['file_size'],
                    'duration_seconds': item.get('duration_seconds'),
                    'download_hint': item.get('download_hint')
                })

    # 3. 生成 batch state
    batch_state = {
        'batch_id': batch_id,
        'telegram_manifest': manifest_path,
        'caption': batch.get('caption'),
        'reference_images': reference_images,
        'pending_video_items': pending_videos,
        'selected_video_index': None,
        'suggested_source_video': None,
        'download_status': 'pending' if pending_videos else 'no_video'
    }

    with open(f"{intake_dir}/current_batch.json", 'w') as f:
        json.dump(batch_state, f, indent=2)

    # 4. 生成 reply hint
    if len(pending_videos) == 0:
        reply_hint = f"批次中没有视频，收到 {len(reference_images)} 张图片。"
    elif len(pending_videos) == 1:
        v = pending_videos[0]
        size_mb = v['file_size'] / 1024 / 1024
        duration = v.get('duration_seconds', '?')
        reply_hint = f"批次中有 1 个视频（{size_mb:.1f}MB, {duration}秒）和 {len(reference_images)} 张图片。"
    else:
        reply_hint = f"批次中有 {len(pending_videos)} 个视频和 {len(reference_images)} 张图片，需要选择视频。"

    print(reply_hint)
    return 0
```

### B. fetch-telegram-video 实现骨架

```python
def fetch_telegram_video(batch_dir, video_index):
    """
    下载选中的视频
    """
    # 1. 读取 batch state
    batch_state_path = f"{batch_dir}/current_batch.json"
    with open(batch_state_path) as f:
        batch_state = json.load(f)

    # 2. 获取视频信息
    if video_index >= len(batch_state['pending_video_items']):
        print(f"错误：video_index {video_index} 越界", file=sys.stderr)
        return 1

    video_item = batch_state['pending_video_items'][video_index]
    file_id = video_item['file_id']

    # 3. 下载视频
    bot_token = os.getenv('TELEGRAM_BOT_TOKEN')
    api_url = os.getenv('TELEGRAM_API_URL', 'http://localhost:8081')

    try:
        # getFile
        resp = requests.post(
            f"{api_url}/bot{bot_token}/getFile",
            json={'file_id': file_id},
            timeout=30
        )
        resp.raise_for_status()
        file_info = resp.json()['result']
        file_path = file_info['file_path']

        # 下载
        download_url = f"{api_url}/file/bot{bot_token}/{file_path}"
        video_resp = requests.get(download_url, stream=True, timeout=300)
        video_resp.raise_for_status()

        # 保存
        ext = os.path.splitext(file_path)[1] or '.mp4'
        local_path = f"{batch_dir}/video_{video_index}{ext}"

        with open(local_path, 'wb') as f:
            for chunk in video_resp.iter_content(chunk_size=8192):
                f.write(chunk)

        # 4. 更新 batch state
        batch_state['selected_video_index'] = video_index
        batch_state['suggested_source_video'] = local_path
        batch_state['download_status'] = 'completed'

        with open(batch_state_path, 'w') as f:
            json.dump(batch_state, f, indent=2)

        print(f"视频已下载到: {local_path}")
        return 0

    except Exception as e:
        print(f"下载失败: {e}", file=sys.stderr)
        return 1
```

---

**文档版本**: v1.0
**最后更新**: 2026-03-17
**对应 OpenFang 版本**: v0.4.4+
