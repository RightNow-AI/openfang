# Telegram 大文件下载功能 - 部署指南

## 媒体组处理（v0.4.4+）

### 结构化媒体批次

从 v0.4.4 开始，Telegram 媒体组（media group）会生成结构化的 `telegram_media_batch` metadata，而不是降级成文本链接。这使得下游 agent（如 `shipinfabu-hand`）可以：

1. **精确了解批次内容**：知道有多少视频、多少图片、每个文件的大小和状态
2. **选择性下载**：只下载需要的视频，而不是全部下载
3. **业务决策**：根据批次结构决定是否继续、是否需要用户确认

### `telegram_media_batch` 结构示例

```json
{
  "batch_key": "group-123_456_789",
  "chat_id": 456,
  "message_id": 789,
  "media_group_id": "group-123",
  "caption": "用户提供的说明文字",
  "items": [
    {
      "kind": "video",
      "file_id": "BAACAgIAAxkBAAI...",
      "file_size": 150000000,
      "duration_seconds": 30,
      "status": "needs_project_download",
      "local_path": null,
      "download_hint": "Video exceeds 100MB safe limit, needs project-side download"
    },
    {
      "kind": "image",
      "file_id": "AgACAgIAAxkBAAI...",
      "file_size": 500000,
      "status": "ready",
      "local_path": "/tmp/openfang-telegram-downloads/photo_123.jpg",
      "download_hint": null
    }
  ]
}
```

### 媒体项状态

- `ready`: 媒体已下载到本地，`local_path` 可用
- `needs_project_download`: 媒体超过安全下载阈值，需要项目侧下载器处理
- `skipped_safe_limit`: 媒体超过 Local Bot API 安全阈值（100MB），已跳过 `getFile` 调用
- `download_failed`: 下载尝试失败

### shipinfabu-hand 集成

当 Telegram 媒体组发送给 `shipinfabu-hand` 时，bridge 层会自动将 `telegram_media_batch` 写入：

```
~/.openfang/workspaces/shipinfabu-hand/inbox/telegram/<batch_key>.json
```

`shipinfabu-hand` 可以：
1. 读取 inbox manifest 获取完整批次信息
2. 根据批次内容决定是否需要用户确认（多视频场景）
3. 调用项目侧下载器（如 `openfang_clean_publish_bridge.py fetch-telegram-video`）下载选中的视频
4. 只下载被选中的视频，避免浪费带宽和存储

### 安全阈值

- **Local Bot API 安全阈值**: 100MB
  - 超过此阈值的文件不会触发 `getFile` 调用，避免 Local Bot API Server 重启
  - 状态标记为 `skipped_safe_limit` 或 `needs_project_download`
- **官方 Bot API 限制**: 20MB
  - 使用官方 API 时，超过 20MB 的文件无法下载

## 快速开始

### 前提条件

1. **获取 Telegram API Credentials**
   - 访问 https://my.telegram.org/apps
   - 登录你的 Telegram 账号
   - 创建新应用，获取 `api_id` 和 `api_hash`

2. **安装 telegram-bot-api 二进制文件**
   - 已安装到：`~/.openfang/bin/telegram-bot-api`
   - 版本：Bot API 9.5
   - 如需重新安装，参考 `docs/telegram-large-files.md`

### 配置步骤

#### 1. 设置环境变量

```bash
# 添加到 ~/.zshrc 或 ~/.bashrc
export TELEGRAM_BOT_TOKEN="你的bot_token"
export TELEGRAM_API_HASH="你的api_hash"
export NVIDIA_INTEGRATE_API_KEY="你的nvidia_api_key"

# 重新加载
source ~/.zshrc
```

**重要提示**：如果使用启动脚本，确保环境变量在脚本执行前已设置。避免使用空变量替换：

```bash
# ❌ 错误：如果变量未设置，会导出空字符串
export TELEGRAM_BOT_TOKEN="${TELEGRAM_BOT_TOKEN}"

# ✅ 正确：先 source 环境文件
source .env.telegram
./target/release/openfang start

# 或者直接在脚本中设置
export TELEGRAM_BOT_TOKEN="实际的token值"
```

**验证环境变量已加载**：

```bash
# 启动后检查进程环境
ps eww -p $(pgrep openfang) | tr ' ' '\n' | grep TELEGRAM_BOT_TOKEN

# 应该看到实际的 token 值，而不是空字符串
# ✅ TELEGRAM_BOT_TOKEN=8698293972:AAFT...
# ❌ TELEGRAM_BOT_TOKEN=
```

如果看到空值，参考 [Health Check Guide](health-check-guide.md) 和 [Troubleshooting](troubleshooting.md#17-telegram-bot-connected-but-not-receiving-messages)。

#### 2. 更新配置文件

编辑 `~/.openfang/config.toml`，找到 `[channels.telegram]` 部分，替换为：

```toml
[channels.telegram]
default_agent = "shipinfabu-hand"
poll_interval_secs = 1
download_enabled = true
download_dir = "/tmp/openfang-telegram-downloads"
max_download_size = 2147483648  # 2GB

# Local Bot API Server 配置（支持 >20MB 文件下载）
use_local_api = true
auto_start_local_api = true
telegram_api_id = "12345678"  # 替换为你的 api_id
telegram_api_hash_env = "TELEGRAM_API_HASH"
local_api_port = 8081
api_url = "http://localhost:8081"

[channels.telegram.overrides]
dm_policy = "respond"
group_policy = "all"
```

**重要：** 将 `telegram_api_id` 替换为你从 https://my.telegram.org/apps 获取的实际值。

#### 3. 创建下载目录

```bash
mkdir -p /tmp/openfang-telegram-downloads
```

### 启动服务

#### 方式 1：生产启动（推荐）

```bash
cd /Users/xiaomo/Desktop/openfang-upstream-fork
scripts/start-telegram-production.sh
```

这个脚本会做四件事：

1. 检查 `TELEGRAM_BOT_TOKEN`、`TELEGRAM_API_HASH`、`NVIDIA_INTEGRATE_API_KEY`
2. 清理旧的 `openfang start` 和 `telegram-bot-api` 进程
3. 启动 release 二进制并写入 `~/.openfang/logs/openfang.log`
4. 等待 `/api/health` 通过后再返回成功

#### 方式 2：前台运行（仅用于临时调试）

```bash
cd /Users/xiaomo/Desktop/openfang-upstream-fork
TELEGRAM_BOT_TOKEN=xxx TELEGRAM_API_HASH=xxx NVIDIA_INTEGRATE_API_KEY=xxx target/release/openfang start
```

不要再使用裸 `nohup target/release/openfang start`。
这种方式依赖当前 shell 是否已经注入全部环境变量，容易产生“能启动但模型/Telegram 不可用”的假成功。

### 验证部署

#### 1. 检查日志

```bash
tail -f ~/.openfang/logs/openfang.log
```

应该看到：
```
INFO Telegram Local Bot API Server started with PID 12345
INFO Telegram Local Bot API mode enabled (supports files >20MB)
INFO Starting Telegram channel adapter...
```

#### 2. 检查进程

```bash
ps aux | grep telegram-bot-api
```

应该看到 telegram-bot-api 进程正在运行。

#### 3. 测试大文件下载

1. 在 Telegram 发送一个 >20MB 的视频
2. 观察日志，应该看到：
   ```
   INFO Downloading file xxx (565 MB)...
   INFO Download progress: 15%
   INFO Download progress: 45%
   INFO Download complete: /tmp/openfang-telegram-downloads/xxx.dat
   ```
3. 检查文件是否存在：
   ```bash
   ls -lh /tmp/openfang-telegram-downloads/
   ```

### 故障排查

#### 问题 1：telegram-bot-api 未启动

**症状：**
```
ERROR Failed to spawn telegram-bot-api: No such file or directory
```

**解决方案：**
```bash
# 检查二进制文件是否存在
ls -l ~/.openfang/bin/telegram-bot-api

# 如果不存在，重新安装
cd /tmp
git clone --recursive https://github.com/tdlib/telegram-bot-api.git
cd telegram-bot-api
mkdir build && cd build
cmake -DCMAKE_BUILD_TYPE=Release ..
cmake --build . --target install
cp telegram-bot-api ~/.openfang/bin/
```

#### 问题 2：端口 8081 被占用

**症状：**
```
ERROR Address already in use (port 8081)
```

**解决方案：**
```bash
# 检查占用端口的进程
lsof -i :8081

# 修改配置使用其他端口
# 在 config.toml 中：
local_api_port = 8082
api_url = "http://localhost:8082"
```

#### 问题 3：API credentials 错误

**症状：**
```
ERROR telegram-bot-api exited with status: 1
```

**解决方案：**
```bash
# 检查环境变量
echo $TELEGRAM_API_HASH

# 检查 config.toml 中的 api_id 是否正确
grep telegram_api_id ~/.openfang/config.toml

# 重新获取 credentials：https://my.telegram.org/apps
```

#### 问题 4：下载仍然失败

**检查清单：**
1. `use_local_api = true` 是否设置？
2. `api_url` 是否指向 `http://localhost:8081`？
3. telegram-bot-api 进程是否正在运行？
4. 查看详细日志：`tail -100 ~/.openfang/logs/openfang.log`

### 停止服务

```bash
# 查找进程
ps aux | grep openfang

# 停止 OpenFang（会自动停止 telegram-bot-api）
kill <pid>

# 或使用 pkill
pkill -f openfang
```

### 与 shipinbot 集成

如果你需要将下载的视频发送到 shipinbot 处理：

1. 确保 shipinbot 的 `local_media_intake_dir` 配置为同一目录：
   ```toml
   local_media_intake_dir = "/tmp/openfang-telegram-downloads"
   ```

2. 或者在智能体对话中手动触发：
   ```
   用户：把这个视频发送到 shipinbot 处理
   ```

详细集成说明参考：`docs/telegram-shipinbot-integration.md`

### 下一步

现在你可以：
1. 测试发送大视频（>20MB）到 Telegram bot
2. 观察下载进度和结果
3. 让智能体处理下载的视频
4. 可选：集成 shipinbot 进行视频处理

### 维护

#### 定期清理下载目录

```bash
# 手动清理
rm -rf /tmp/openfang-telegram-downloads/*

# 或设置 cron 任务（每天清理）
echo "0 2 * * * rm -rf /tmp/openfang-telegram-downloads/*" | crontab -
```

#### 更新 telegram-bot-api

```bash
cd /tmp/telegram-bot-api
git pull
cd build
cmake --build . --target install
cp telegram-bot-api ~/.openfang/bin/
```

#### 查看日志

```bash
# 实时查看
tail -f ~/.openfang/logs/openfang.log

# 搜索 Telegram 相关日志
grep -i telegram ~/.openfang/logs/openfang.log

# 搜索错误
grep -i error ~/.openfang/logs/openfang.log
```

## 技术细节

### 架构

```
┌─────────────────────────────────────────────────────────┐
│                    OpenFang Kernel                       │
│  ┌────────────────────────────────────────────────┐     │
│  │  Telegram Channel Adapter                      │     │
│  │  - 轮询消息                                     │     │
│  │  - 检测大文件 (>20MB)                          │     │
│  │  - 调用 Local Bot API 下载                     │     │
│  └────────────────┬───────────────────────────────┘     │
│                   │                                      │
│  ┌────────────────▼───────────────────────────────┐     │
│  │  telegram_local_api.rs                         │     │
│  │  - 管理 telegram-bot-api 子进程                │     │
│  │  - 自动启动/停止/重启                          │     │
│  │  - 崩溃恢复（最多3次）                         │     │
│  └────────────────────────────────────────────────┘     │
└───────────────────┼──────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────┐
│         telegram-bot-api (子进程)                        │
│  - 监听端口 8081                                         │
│  - 支持最大 2GB 文件下载                                 │
│  - 文件保存到 /tmp/openfang-telegram-downloads/         │
└─────────────────────────────────────────────────────────┘
```

### 进程管理

- **启动时机：** OpenFang 启动时，如果 `auto_start_local_api = true`
- **停止时机：** OpenFang 停止时自动停止
- **重启策略：** 崩溃后自动重启，延迟 5s/10s/20s，最多3次
- **信号处理：** 使用 SIGTERM 优雅停止（Unix）或 taskkill（Windows）

### 文件命名

下载的文件命名格式：
```
{file_id}_{timestamp}.dat
```

例如：
```
AgACAgQAAxkBAAIBY2..._1710654321000.dat
```

### 配置字段说明

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `use_local_api` | bool | false | 是否使用 Local Bot API |
| `auto_start_local_api` | bool | false | 是否自动启动子进程 |
| `telegram_api_id` | string | - | Telegram API ID |
| `telegram_api_hash_env` | string | - | API Hash 环境变量名 |
| `local_api_port` | u16 | 8081 | 监听端口 |
| `api_url` | string | - | API 地址 |
| `download_dir` | string | - | 下载目录 |
| `max_download_size` | u64 | 2GB | 最大文件大小 |

## 参考文档

- [Telegram 大文件下载完整指南](./telegram-large-files.md)
- [shipinbot 集成指南](./telegram-shipinbot-integration.md)
- [实现总结](./telegram-implementation-summary.md)
- [Local Bot API Server GitHub](https://github.com/tdlib/telegram-bot-api)
- [获取 API Credentials](https://my.telegram.org/apps)
