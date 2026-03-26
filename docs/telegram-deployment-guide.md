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

- `ready`: 媒体已在消息生产侧下载成功。优先使用 `local_path`，但跨容器/跨宿主机场景下仍可能需要 `download_hint` 回退重拉
- `needs_project_download`: 媒体超过安全下载阈值，需要项目侧下载器处理
- `skipped_safe_limit`: 媒体超过 Local Bot API 安全阈值（100MB），已跳过 `getFile` 调用
- `download_failed`: 下载尝试失败

### shipinfabu-hand 集成

当 Telegram 媒体组发送给 `shipinfabu-hand` 时，bridge 层会自动将 `telegram_media_batch` 写入：

```
~/.openfang/workspaces/shipinfabu-hand/inbox/telegram/<batch_key>.json
```

如果运行时设置了 `OPENFANG_HOME=/var/lib/openfang`，对应路径会变成：

```
/var/lib/openfang/workspaces/shipinfabu-hand/inbox/telegram/<batch_key>.json
```

bridge 会优先使用 agent 的真实 workspace；只有在旧内核/测试桩没有返回 workspace metadata 时，才回退到 `$OPENFANG_HOME/workspaces/<agent-name>`，而在未设置 `OPENFANG_HOME` 的开发机上继续回退到 `~/.openfang/workspaces/<agent-name>`。

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
  - 对媒体组 / 大视频场景，这意味着只配 bot token 不够；还必须补上 `api_id + api_hash + telegram-bot-api`

## 快速开始

### 前提条件

1. **获取 Telegram API Credentials**
   - 访问 https://my.telegram.org/apps
   - 登录你的 Telegram 账号
   - 创建新应用，获取 `api_id` 和 `api_hash`

2. **安装 telegram-bot-api 二进制文件**
   - 建议放在：`~/.openfang/bin/telegram-bot-api`
   - OpenFang 会管理这个子进程，但不会把二进制随自己一起打包
   - 当前仓库推荐把源码作为 `third_party/telegram-bot-api` 子模块维护，再用 `./scripts/install-telegram-local-api.sh` 安装
   - 如需重新安装，参考 `docs/telegram-large-files.md`

### 配置步骤

#### 1. 设置环境变量

```bash
# 添加到 ~/.zshrc 或 ~/.bashrc
export TELEGRAM_BOT_TOKEN="你的bot_token"
export TELEGRAM_API_HASH="你的api_hash"
# 同时导出默认模型所需的 provider key，例如：
export GROQ_API_KEY="你的groq_api_key"

# 重新加载
source ~/.zshrc
```

`GROQ_API_KEY` 只是示例；请替换为你在 `[default_model].api_key_env` 里实际使用的环境变量。

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

**验证服务就绪**：

```bash
curl -s http://127.0.0.1:4200/api/health
openfang status
```

如果启用了 API 鉴权，再补一条：

```bash
curl -s -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/health/detail
```

不要用 `ps eww`、`env` 或 shell 历史直接打印 token/hash。认证状态优先通过 `/api/health/detail`、`/api/providers` 和 [Health Check Guide](health-check-guide.md) / [Troubleshooting](troubleshooting.md#17-telegram-bot-connected-but-not-receiving-messages) 排查。

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

### 服务器部署建议：优先使用 Docker 托管 Local Bot API

在长期运行的 Linux 服务器上，更推荐把 `telegram-bot-api` 作为独立 Docker 容器运行，而不是让 OpenFang 自己拉起本地二进制。这样升级、回滚和故障排查都更简单。

推荐配置：

```toml
[channels.telegram]
default_agent = "shipinfabu-hand"
poll_interval_secs = 1
download_enabled = true
download_dir = "/opt/openfang/data/telegram-intake"
max_download_size = 2147483648

use_local_api = true
auto_start_local_api = false
telegram_api_id = "12345678"
telegram_api_hash_env = "TELEGRAM_API_HASH"
local_api_port = 8081
api_url = "http://127.0.0.1:8081"
```

推荐容器启动方式：

```bash
mkdir -p /var/lib/telegram-bot-api

docker run -d \
  --name telegram-bot-api \
  --restart unless-stopped \
  -p 127.0.0.1:8081:8081 \
  -e TELEGRAM_API_ID=12345678 \
  -e TELEGRAM_API_HASH="$TELEGRAM_API_HASH" \
  -e TELEGRAM_LOCAL=1 \
  -v /var/lib/telegram-bot-api:/var/lib/telegram-bot-api \
  aiogram/telegram-bot-api:latest
```

为什么这里强调把宿主机和容器都挂到 `/var/lib/telegram-bot-api`：

- Local Bot API 的 `getFile` 在本地模式下会返回绝对路径，例如 `/var/lib/telegram-bot-api/<bot-token>/videos/file_9.mp4`
- 如果容器内路径和宿主机路径不一致，例如把宿主机挂到 `/root/telegram-bot-api-data:/var/lib/telegram-bot-api`
- 那么 OpenFang 或下游 bridge 在宿主机上读取 `/var/lib/telegram-bot-api/...` 时会报 `No such file or directory`

换句话说，只要你的下游逻辑会把 `getFile.result.file_path` 当作宿主机上的本地路径使用，就必须让宿主机看到同一条绝对路径。

同一条链上还要一起检查这些变量，不要只盯一个：

- OpenFang：`use_local_api`、`auto_start_local_api`、`api_url`、`download_dir`
- shipinfabu / bridge：`local_media_intake_dir`、`local_source_staging_dir`、`local_media_intake_retention_hours`

如果 OpenFang 用的是容器内 Local Bot API，而 bridge 跑在另一个文件系统视图里：

- `file://...` 不等于 bridge 可读
- `ready` 也不等于一定可直接提交
- 新版本 bridge 会优先按 manifest 里的 `download_hint` 重拉；如果拿到的仍是容器私有路径，会明确报 `TELEGRAM_LOCAL_API_PATH_NOT_SHARED`

### 启动服务

#### 方式 1：本地源码启动（适合单机调试）

```bash
cargo build --release -p openfang-cli
TELEGRAM_BOT_TOKEN=xxx \
TELEGRAM_API_HASH=xxx \
GROQ_API_KEY=xxx \
target/release/openfang start
```

将 `GROQ_API_KEY` 替换为 `[default_model].api_key_env` 对应的实际环境变量。如果你用 systemd 或 Docker 部署，请按 [deployment.md](deployment.md) 和 [operations-runbook.md](operations-runbook.md) 的方式注入同样的环境变量，而不是依赖当前交互 shell。

#### 方式 2：前台运行（仅用于临时调试）

```bash
TELEGRAM_BOT_TOKEN=xxx \
TELEGRAM_API_HASH=xxx \
GROQ_API_KEY=xxx \
target/release/openfang start
```

不要再使用裸 `nohup target/release/openfang start`。
这种方式依赖当前 shell 是否已经注入全部环境变量，容易产生“能启动但模型/Telegram 不可用”的假成功。

### 验证部署

#### 1. 检查健康与日志

```bash
curl -s http://127.0.0.1:4200/api/health

# systemd
sudo journalctl -u openfang -n 100 --no-pager

# Docker / Compose
docker compose logs --tail=100 openfang

# 本地前台运行
# 直接查看运行 `target/release/openfang start` 的终端
```

至少应该看到：
```
INFO Starting Telegram channel adapter...
INFO Telegram bot @your_bot connected
INFO Telegram polling loop started
```

如果 `auto_start_local_api = true`，还会额外看到 Local Bot API Server 启动相关日志。

#### 2. 检查 Local Bot API 进程或容器

```bash
ps aux | grep telegram-bot-api
docker ps --filter name=telegram-bot-api
```

如果你使用 Docker 托管 Local Bot API，重点看容器是否在运行；如果是 OpenFang 自管二进制，重点看本机进程是否存在。

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

# 如果不存在，优先使用仓库内 third_party 源码重新安装
git submodule update --init --recursive third_party/telegram-bot-api
./scripts/install-telegram-local-api.sh
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
3. `auto_start_local_api` 是否与你的部署方式一致：
   - OpenFang 自管二进制：`true`
   - Docker / 外部服务托管：`false`
4. telegram-bot-api 进程或容器是否正在运行？
5. 如果你使用 Docker，本机是否真的能访问 `getFile` 返回的绝对路径？

   ```bash
   curl -sS -H 'Content-Type: application/json' \
     -d '{"file_id":"<FILE_ID>"}' \
     "http://127.0.0.1:8081/bot$TELEGRAM_BOT_TOKEN/getFile"
   ```

   拿到 `file_path` 后，直接在宿主机检查：

   ```bash
   ls -l /var/lib/telegram-bot-api/...
   ```

   如果 `getFile` 返回成功，但宿主机上 `ls` 不到同一路径，说明是挂载路径不一致，不是 token 或权限问题。
6. 查看详细日志：
   - systemd: `journalctl -u openfang -n 100 --no-pager`
   - Docker / Compose: `docker compose logs --tail=100 openfang`
   - 本地前台运行：查看当前终端输出

#### 问题 5：图片已下载，但提交阶段提示“article_images 类型不支持”

**症状：**
```
article_images 类型不支持，仅支持常见图片格式
```

**原因：**

- 某些 Local Bot API 部署会把 Telegram 图片落盘成 `.dat`
- 文件内容其实是 JPEG / PNG，但下游只按扩展名校验，导致提交阶段被拒

**排查方式：**

1. 检查 staged 文件后缀是否是 `.dat`
2. 检查文件头是否仍然是常见图片格式

例如 JPEG 文件头通常以 `ff d8 ff` 开头。

**建议修复：**

- 在 bridge / staging 阶段根据文件头或 MIME type 重新归一化扩展名
- 不要直接信任 Telegram 落盘时的 `.dat` 后缀

如果你的工作流会把 Telegram 图片再提交给严格校验的媒体服务，这一步尤其重要。

### 停止服务

```bash
# 本地源码启动
target/release/openfang stop

# systemd
sudo systemctl stop openfang

# Docker / Compose
docker compose stop openfang
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
git submodule update --init --recursive third_party/telegram-bot-api
./scripts/install-telegram-local-api.sh
```

#### 查看日志

```bash
# systemd
sudo journalctl -u openfang -f

# Docker / Compose
docker compose logs -f openfang

# 本地前台运行
# 直接查看当前启动终端，或自行把 stdout/stderr 重定向到文件后再 grep
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
