# Telegram 大文件下载支持

## 问题背景

Telegram 官方 Bot API 的 `getFile` 方法有 **20MB 的硬性限制**，无法下载超过 20MB 的文件。当用户发送大视频（如 565MB）时，OpenFang 会显示"下载失败"。

## 解决方案：Local Bot API Server

Telegram 提供了可自行部署的 [Local Bot API Server](https://github.com/tdlib/telegram-bot-api)，支持下载最大 **2GB** 的文件。

这不是“可有可无的优化”，而是媒体下载链的关键组件：

- 只靠官方 Bot API，`getFile` 仍然受 20MB 硬限制
- 自建 Local Bot API Server 才能真正解锁大视频、媒体组和项目侧二次下载链
- 对本仓库的 Telegram 媒体组工作流来说，`api_id + api_hash + telegram-bot-api` 是绕过官方下载限制的必备组合

OpenFang 支持两种集成方式：
1. **自动启动模式**（推荐）- OpenFang 自动管理 Local API Server 进程
2. **外部部署模式** - 手动部署 Local API Server（Docker 或独立进程）

可直接参考 [telegram-config-example.toml](telegram-config-example.toml) 获取完整示例。

---

## 方式 1：自动启动模式（推荐）

OpenFang 会自动启动和管理 Local Bot API Server 作为子进程。
但它不会自带 `telegram-bot-api` 二进制；自动启动模式的前提是你已经把
二进制安装到系统 PATH、`~/.openfang/bin/telegram-bot-api`，或其他当前实现会查找的位置。

当前仓库推荐把它的源码作为 `third_party/telegram-bot-api` 子模块维护，并用仓库脚本安装到
`~/.openfang/bin/telegram-bot-api`。

### 步骤 1：获取 Telegram API Credentials

1. 访问 https://my.telegram.org/apps
2. 登录你的 Telegram 账号
3. 创建一个新应用，获取 `api_id` 和 `api_hash`

### 步骤 2：配置环境变量

```bash
export TELEGRAM_API_HASH="your_api_hash_here"
```

### 步骤 3：从仓库内源码安装 telegram-bot-api

```bash
git submodule update --init --recursive third_party/telegram-bot-api
./scripts/install-telegram-local-api.sh
```

### 步骤 4：配置 OpenFang

编辑 `~/.openfang/config.toml`：

```toml
[channels.telegram]
default_agent = "your-agent-name"
poll_interval_secs = 1

# 启用文件下载
download_enabled = true
download_dir = "/tmp/openfang-telegram-downloads"
max_download_size = 2147483648  # 2GB

# 启用 Local Bot API Server（关键配置）
use_local_api = true
auto_start_local_api = true  # 自动启动
telegram_api_id = "12345678"  # 你的 API ID
telegram_api_hash_env = "TELEGRAM_API_HASH"  # 环境变量名
local_api_port = 8081  # 可选，默认 8081
# 可选：如果省略，OpenFang 在自动启动成功后会自动使用本地地址
# api_url = "http://127.0.0.1:8081"
```

### 步骤 5：启动 OpenFang

```bash
cargo build --release -p openfang-cli
TELEGRAM_BOT_TOKEN=xxx TELEGRAM_API_HASH=xxx openfang start
```

OpenFang 会自动：
- 检测 telegram-bot-api 二进制文件（系统 PATH 或本地安装）
- 启动 Local API Server 子进程
- 管理进程生命周期（自动重启、随 OpenFang 停止）

如果你的目标是“让 Telegram 媒体组里的大文件真的能下载下来”，这一段不是辅助能力，而是主链路前提。

### 验证

查看日志，应该看到：

```
INFO Telegram Local Bot API Server started on port 8081
INFO Telegram Local Bot API Server is accepting connections on port 8081
INFO Telegram Local Bot API mode enabled (supports files >20MB)
```

---

## 方式 2：外部部署模式

如果你想手动管理 Local API Server（例如使用 Docker），可以使用外部部署模式。

### Docker 部署

```bash
docker run -d \
  --name telegram-bot-api \
  -p 8081:8081 \
  -v /var/lib/telegram-bot-api:/var/lib/telegram-bot-api \
  -e TELEGRAM_API_ID=YOUR_API_ID \
  -e TELEGRAM_API_HASH=YOUR_API_HASH \
  aiogram/telegram-bot-api:latest
```

### 配置 OpenFang

```toml
[channels.telegram]
download_enabled = true
use_local_api = true
auto_start_local_api = false  # 不自动启动
api_url = "http://localhost:8081"
```

如果是远端或非默认端口部署，把 `api_url` 改成实际地址即可。

---

## 配置字段说明

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `download_enabled` | bool | `false` | 是否启用文件下载 |
| `download_dir` | string | 系统临时目录 | 文件下载保存路径 |
| `max_download_size` | u64 | 2GB | 最大下载文件大小（字节） |
| `use_local_api` | bool | `false` | 是否使用 Local Bot API Server |
| `auto_start_local_api` | bool | `false` | 是否自动启动 Local API Server |
| `telegram_api_id` | string | - | Telegram API ID（自动启动时必需） |
| `telegram_api_hash_env` | string | - | API Hash 环境变量名（自动启动时必需） |
| `local_api_port` | u16 | 8081 | Local API Server 监听端口 |
| `api_url` | string | `https://api.telegram.org` | Bot API 服务器地址 |

---

## 工作原理

1. **官方 API 模式**（`use_local_api = false`）：
   - 文件 ≤ 20MB：正常下载
   - 文件 > 20MB：返回 `None`，显示"下载失败"并提示部署 Local API

2. **Local API 模式**（`use_local_api = true`）：
   - 文件 ≤ 2GB：正常下载
   - 文件 > 2GB：超过 `max_download_size` 限制，返回 URL

3. **自动启动模式**（`auto_start_local_api = true`）：
   - OpenFang 启动时自动启动 Local API Server 子进程
   - 默认等待本地端口就绪后再继续桥接 Telegram
   - 进程崩溃时自动重启（最多 3 次）
   - OpenFang 停止时自动停止 Local API Server

---

## 故障排查

### 问题：仍然显示"下载失败"

**检查清单：**
1. `use_local_api = true` 是否设置？
2. 如果是外部部署模式，`api_url` 是否指向 Local API Server？
3. 如果是自动启动模式，日志里是否出现 `accepting connections`？
4. Local API Server 是否正在运行？
   ```bash
   curl "http://127.0.0.1:8081/bot$TELEGRAM_BOT_TOKEN/getMe"
   # 或检查进程
   ps aux | grep telegram-bot-api
   ```
5. 重启 OpenFang 使配置生效

### 问题：自动启动失败

**可能原因：**

1. **telegram-bot-api 二进制文件未找到**

   错误日志：
   ```
   telegram-bot-api binary not found. Please install it...
   ```

   解决方案：
   ```bash
   # 检查是否在 PATH 中
   which telegram-bot-api

   # 如果没有，先确认仓库内子模块和安装脚本：
   git submodule update --init --recursive third_party/telegram-bot-api
   ./scripts/install-telegram-local-api.sh

   # 或手工检查 OpenFang bin 目录：
   ls -l ~/.openfang/bin/telegram-bot-api

   # 也可以使用 Docker 外部部署模式
   ```

2. **API credentials 未配置**

   错误日志：
   ```
   auto_start_local_api enabled but telegram_api_id or telegram_api_hash_env not configured
   ```

   解决方案：检查 config.toml 中的 `telegram_api_id` 和 `telegram_api_hash_env` 字段

3. **环境变量未设置**

   错误日志：
   ```
   auto_start_local_api enabled but TELEGRAM_API_HASH not set
   ```

   解决方案：
   ```bash
   export TELEGRAM_API_HASH="your_api_hash"
   ```

### 问题：端口冲突

如果端口 8081 已被占用：

```toml
[channels.telegram]
local_api_port = 8082  # 使用其他端口
api_url = "http://localhost:8082"
```

---

## 性能考虑

- 大文件下载会占用磁盘空间（默认保存在 `/tmp`）
- 建议定期清理 `download_dir` 中的旧文件
- 可以通过 `max_download_size` 限制单个文件大小
- 自动启动模式下，Local API Server 数据存储在 `~/.openfang/telegram-local-api-data/`
- 外部部署模式下，数据目录由 Docker 挂载或 `telegram-bot-api --dir` 参数决定

---

## 进程管理

### 查看 Local API Server 状态

```bash
# 查看进程
ps aux | grep telegram-bot-api

# 查看日志（如果使用 systemd）
journalctl -u openfang -f
```

### 手动停止

```bash
# 自动启动模式：停止 OpenFang 会自动停止 Local API Server
pkill openfang

# 外部部署模式（Docker）：
docker stop telegram-bot-api
```

---

## 参考资料

- [Telegram Bot API 官方文档](https://core.telegram.org/bots/api#getfile)
- [Local Bot API Server GitHub](https://github.com/tdlib/telegram-bot-api)
- [获取 API credentials](https://my.telegram.org/apps)
- [Docker 镜像](https://hub.docker.com/r/aiogram/telegram-bot-api)
