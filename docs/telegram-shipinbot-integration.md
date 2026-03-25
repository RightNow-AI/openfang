# OpenFang Telegram 大文件下载 - shipinbot 集成指南

## 概述

本文档说明 OpenFang 的 Telegram 大文件下载功能如何与 shipinbot 项目对接。

通用的 Local Bot API Server 配置、启动和排障请先参考 [telegram-large-files.md](telegram-large-files.md)。本文只覆盖 shipinbot 场景下的目录约定和对接方式。

## 项目关系

```
┌─────────────────────────────────────────────────────────────┐
│                    OpenFang Agent OS                         │
│  ┌────────────────────────────────────────────────────┐     │
│  │  Telegram Channel Adapter                          │     │
│  │  - 接收消息                                         │     │
│  │  - 下载大文件 (>20MB, 最大 2GB)                    │     │
│  │  - 保存到: /tmp/openfang-telegram-downloads/      │     │
│  └────────────────┬───────────────────────────────────┘     │
│                   │                                          │
│  ┌────────────────▼───────────────────────────────────┐     │
│  │  shipinfabu Hand                                   │     │
│  │  - 接收下载完成的视频路径                          │     │
│  │  - 决定是否发送到 shipinbot 处理                   │     │
│  └────────────────┬───────────────────────────────────┘     │
└───────────────────┼──────────────────────────────────────────┘
                    │
                    │ (可选) 调用 bridge 脚本
                    ▼
┌─────────────────────────────────────────────────────────────┐
│              shipinbot media-pipeline-service                │
│  - 视频去水印                                                │
│  - 文案生成                                                  │
│  - 发布到 PublishHub                                         │
└─────────────────────────────────────────────────────────────┘
```

## 工作流程

### 1. 用户发送大视频到 Telegram

```
用户 → Telegram Bot → OpenFang Telegram Adapter
```

### 2. OpenFang 下载视频

**当前行为：**
- 检测到视频 >20MB
- 如果启用 Local Bot API Server：下载到本地
- 如果未启用：显示"下载失败"并提示配置

**下载位置：**
```
/tmp/openfang-telegram-downloads/
├── AgACAgQAAxkBAAIBY2..._1710654321000.dat
├── AgACAgQAAxkBAAIBY3..._1710654322000.dat
└── ...
```

### 3. shipinfabu Hand 接收通知

**智能体收到的消息：**
```
✅ 视频下载完成

文件信息：
- 路径：/tmp/openfang-telegram-downloads/video_xxx.dat
- 大小：565 MB
- 时长：8分15秒
- 分辨率：1920x1080

你可以：
- 查看视频信息
- 提取截图预览
- 发送到 shipinbot 处理
```

### 4. 与 shipinbot 对接（可选）

**方式 1：手动触发**
```
用户：把这个视频发送到 shipinbot 处理
智能体：[调用 bridge 脚本]
智能体：任务已提交，job_id: abc123
```

**方式 2：自动触发（未实现）**
- 可以配置规则：特定关键词自动触发
- 例如：用户说"发布"时自动调用 shipinbot

## 配置对接

### OpenFang 配置 (`~/.openfang/config.toml`)

```toml
[channels.telegram]
default_agent = "shipinfabu-hand"
poll_interval_secs = 1

# 大文件下载配置
download_enabled = true
download_dir = "/tmp/openfang-telegram-downloads"
max_download_size = 2147483648  # 2GB

# Local Bot API Server 配置
use_local_api = true
auto_start_local_api = true
telegram_api_id = "12345678"
telegram_api_hash_env = "TELEGRAM_API_HASH"
local_api_port = 8081
api_url = "http://localhost:8081"
```

### shipinbot 配置 (`~/.openfang/hands/shipinfabu/HAND.toml`)

```toml
[[settings]]
key = "local_media_intake_dir"
label = "Local Media Intake Dir"
description = "媒体收件目录"
setting_type = "text"
default = "/tmp/openfang-telegram-downloads"

[[settings]]
key = "local_media_intake_retention_hours"
label = "Local Media Intake Retention Hours"
description = "收件目录保留时长（小时）"
setting_type = "text"
default = "12"
```

**关键点：**
- OpenFang 下载目录 = shipinbot 收件目录
- 两者共享同一个文件系统路径
- shipinbot 会自动清理过期文件（12小时）

## 文件路径映射

### OpenFang 下载的文件

下面这个例子只适用于 OpenFang 与后续 bridge 运行在同一文件系统视图的场景：

```
/tmp/openfang-telegram-downloads/
└── AgACAgQAAxkBAAIBY2..._1710654321000.dat
    ↓
    智能体看到：file:///tmp/openfang-telegram-downloads/AgACAgQAAxkBAAIBY2..._1710654321000.dat
```

### shipinbot 接收的文件

```
shipinfabu Hand 调用 bridge 脚本：
python3 /Users/xiaomo/shipinbot-runtime/scripts/openfang_clean_publish_bridge.py \
  --source-video /tmp/openfang-telegram-downloads/AgACAgQAAxkBAAIBY2..._1710654321000.dat \
  --action submit
```

### shipinbot 处理流程

```
1. bridge 脚本检查文件是否在白名单目录
2. 如果不在，复制到 local_source_staging_dir
3. 调用 media-pipeline-service API
4. 返回 job_id 给智能体
5. 智能体轮询任务状态
6. 完成后汇报结果
```

如果 OpenFang / telegram-bot-api / shipinbot bridge 不共享同一文件系统：

- `file://...` 可能不是 bridge 可读文件
- 应优先使用 manifest 里的 `download_hint` 重新拉到 `local_media_intake_dir`
- 如果 Local Bot API 仍返回容器私有绝对路径，bridge 会明确报 `TELEGRAM_LOCAL_API_PATH_NOT_SHARED`

相关变量不要拆开理解：

- OpenFang：`use_local_api`、`auto_start_local_api`、`api_url`、`download_dir`
- shipinfabu：`local_media_intake_dir`、`local_source_staging_dir`、`local_media_intake_retention_hours`

## 环境变量

### OpenFang 需要的环境变量

```bash
export TELEGRAM_BOT_TOKEN="你的bot_token"
export TELEGRAM_API_HASH="你的api_hash"
```

### shipinbot 需要的环境变量

```bash
export WAVESPEED_API_KEY="你的去水印API密钥"
export MEDIA_PIPELINE_DB_SECRET="数据库密钥"
```

## 启动顺序

### 容器化一体部署（推荐）

如果 OpenFang、shipinbot 和 Telegram Local Bot API 都跑容器，不要再拆成三套各自猜路径的私有文件系统。当前仓库支持的生产口径是：

- `docker compose up -d --build`
- `openfang` 与 `media-pipeline-service` 共享 `shipinbot-data`，并且都使用 `/app/data/ingest`
- `openfang` 与 `telegram-bot-api` 共享 `telegram-bot-api-data`，并且都挂到 `/var/lib/telegram-bot-api`

对应配置链要保持一致：

```toml
[channels.telegram]
default_agent = "shipinfabu-hand"
download_enabled = true
use_local_api = true
auto_start_local_api = false
api_url = "http://telegram-bot-api:8081"
```

```env
OPENFANG_BOOTSTRAP_SHIPINBOT=1
SHIPINFABU_MEDIA_API_BASE_URL=http://media-pipeline-service:8000
SHIPINFABU_BRIDGE_SCRIPT_PATH=/app/scripts/openfang_clean_publish_bridge.py
SHIPINFABU_LOCAL_SOURCE_STAGING_DIR=/app/data/ingest
SHIPINFABU_LOCAL_MEDIA_INTAKE_DIR=/app/data/ingest
```

这个拓扑下，`file:///var/lib/telegram-bot-api/...` 和 `/app/data/ingest/...` 才是真路径，不是容器内自我感动的假路径。

### 1. 启动 shipinbot media-pipeline-service

```bash
cd /Users/xiaomo/shipinbot-runtime
./scripts/start_media_web.sh
```

验证：
```bash
curl http://127.0.0.1:8000/healthz
```

### 2. 启动 OpenFang

```bash
cd /Users/xiaomo/Desktop/openfang-upstream-fork
TELEGRAM_BOT_TOKEN=xxx TELEGRAM_API_HASH=xxx openfang start
```

验证：
```bash
# 查看日志
tail -f ~/.openfang/logs/openfang.log

# 应该看到：
# INFO Telegram Local Bot API Server started on port 8081
# INFO Telegram Local Bot API mode enabled (supports files >20MB)
```

## 测试流程

### 1. 测试大文件下载

```bash
# 在 Telegram 发送一个 >20MB 的视频
# 应该看到：
# ⬇️ 下载中... 15%
# ⬇️ 下载中... 45%
# ✅ 下载完成
```

### 2. 测试 shipinbot 对接

```bash
# 在 Telegram 对话中：
用户：把这个视频发送到 shipinbot 处理

# 智能体应该：
# 1. 调用 bridge 脚本
# 2. 返回 job_id
# 3. 开始轮询任务状态
# 4. 汇报处理进度
```

### 3. 验证文件路径

```bash
# 检查下载目录
ls -lh /tmp/openfang-telegram-downloads/

# 检查 shipinbot 是否能访问
cd /Users/xiaomo/shipinbot-runtime
.venv/bin/python scripts/openfang_clean_publish_bridge.py \
  --source-video /tmp/openfang-telegram-downloads/xxx.dat \
  --action validate
```

## 故障排查

### 问题 1：视频下载失败

**症状：**
```
[视频 (565 MB) - 下载失败]
```

**检查清单：**
1. Local Bot API Server 是否运行？
   ```bash
   curl "http://127.0.0.1:8081/bot$TELEGRAM_BOT_TOKEN/getMe"
   ```

2. `use_local_api = true` 是否设置？
   ```bash
   grep use_local_api ~/.openfang/config.toml
   ```

3. 查看日志：
   ```bash
   tail -f ~/.openfang/logs/openfang.log | grep -i telegram
   ```

### 问题 2：shipinbot 找不到文件

**症状：**
```
Error: Source video not in allowlist
```

**解决方案：**
1. 检查 shipinbot 配置：
   ```bash
   cat /Users/xiaomo/shipinbot-runtime/config/project.yaml | grep allowlist
   ```

2. 确保 `/tmp/openfang-telegram-downloads` 在白名单中

3. 或者让 bridge 自动复制文件：
   ```toml
   local_source_staging_dir = "/Users/xiaomo/shipinbot-runtime/data/ingest"
   ```

### 问题 3：两个服务端口冲突

**症状：**
```
Error: Address already in use (port 8081)
```

**解决方案：**
```toml
# 修改 Local Bot API Server 端口
local_api_port = 8082
api_url = "http://localhost:8082"
```

## 维护说明

### 定期清理下载目录

```bash
# 手动清理
rm -rf /tmp/openfang-telegram-downloads/*

# 或者配置自动清理（shipinbot 会处理）
local_media_intake_retention_hours = "12"
```

### 更新 OpenFang

```bash
cd /Users/xiaomo/Desktop/openfang-upstream-fork
git pull
cargo build --release -p openfang-cli

# 重启服务
pkill openfang
TELEGRAM_BOT_TOKEN=xxx TELEGRAM_API_HASH=xxx openfang start
```

### 更新 shipinbot

```bash
cd /Users/xiaomo/Desktop/shipinbot
git pull
python3 scripts/sync_openfang_local_hands.py
./scripts/deploy_media_runtime.sh
```

## 未来增强

### 计划中的功能

1. **智能询问下载**
   - 检测到大视频时先询问用户
   - 用户确认后再下载

2. **视频信息预览**
   - 下载完成后自动显示视频元信息
   - 提取关键帧截图

3. **自动触发 shipinbot**
   - 配置规则自动发送到 shipinbot
   - 例如：特定关键词、特定用户

4. **进度增强**
   - 显示下载速度
   - 显示预计剩余时间
   - 支持取消下载

## 参考文档

- [OpenFang Telegram 大文件下载指南](./telegram-large-files.md)
- [shipinbot README](../../shipinbot/README.md)
- [shipinbot Hand 说明](../../shipinbot/docs/openfang-external-hand.md)
- [Local Bot API Server GitHub](https://github.com/tdlib/telegram-bot-api)
