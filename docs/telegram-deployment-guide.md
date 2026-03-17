# Telegram 大文件下载功能 - 部署指南

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

# 重新加载
source ~/.zshrc
```

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

#### 方式 1：前台运行（推荐用于测试）

```bash
cd /Users/xiaomo/Desktop/openfang-upstream-fork
TELEGRAM_BOT_TOKEN=xxx TELEGRAM_API_HASH=xxx target/release/openfang start
```

#### 方式 2：后台运行

```bash
cd /Users/xiaomo/Desktop/openfang-upstream-fork
nohup target/release/openfang start > ~/.openfang/logs/openfang.log 2>&1 &
```

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
