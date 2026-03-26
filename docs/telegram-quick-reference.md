# Telegram 大文件下载 - 快速参考

## 🚀 一键启动

```bash
# 0. 首次准备 third_party 源码并安装 telegram-bot-api
git submodule update --init --recursive third_party/telegram-bot-api
./scripts/install-telegram-local-api.sh

# 1. 设置环境变量（首次）
export TELEGRAM_BOT_TOKEN="你的bot_token"
export TELEGRAM_API_HASH="你的api_hash"

# 2. 启动服务（在仓库根目录）
target/release/openfang start
```

## 核心前提

- `TELEGRAM_BOT_TOKEN` 只负责机器人收发消息
- 要真正绕过 Telegram 官方 Bot API 的 20MB 下载限制，必须再配：
  - `telegram_api_id`
  - `TELEGRAM_API_HASH`
  - `telegram-bot-api`
- 对本仓库的媒体组工作流，这个自建 Local Bot API Server 是主链路依赖，不是可选装饰

## ⚙️ 配置检查

```bash
# 验证部署
./scripts/verify-telegram-setup.sh

# 查看日志
tail -f ~/.openfang/logs/openfang.log

# 检查进程
ps aux | grep telegram-bot-api
```

## 📋 配置文件位置

```
~/.openfang/config.toml
```

必需字段：
```toml
[channels.telegram]
use_local_api = true
auto_start_local_api = true
telegram_api_id = "12345678"  # 从 https://my.telegram.org/apps 获取
telegram_api_hash_env = "TELEGRAM_API_HASH"
```

## 🔍 故障排查

| 问题 | 命令 |
|------|------|
| 检查二进制 | `ls -l ~/.openfang/bin/telegram-bot-api` |
| 检查端口 | `lsof -i :8081` |
| 检查进程 | `ps aux \| grep telegram-bot-api` |
| 查看错误 | `grep -i error ~/.openfang/logs/openfang.log` |
| 重启服务 | `pkill openfang && target/release/openfang start` |

## 📚 文档索引

| 文档 | 用途 |
|------|------|
| `docs/telegram-deployment-guide.md` | 完整部署指南 |
| `docs/telegram-large-files.md` | 技术细节与配置参考 |
| `docs/telegram-testing-checklist.md` | 测试清单 |
| `scripts/install-telegram-local-api.sh` | 从仓库内 third_party 源码编译并安装二进制 |
| `scripts/setup-telegram-local-api.sh` | 配置向导 |
| `scripts/verify-telegram-setup.sh` | 部署验证 |

## ✅ 成功标志

启动后应看到：
```
INFO Telegram Local Bot API Server started with PID 12345
INFO Telegram Local Bot API mode enabled (supports files >20MB)
INFO Starting Telegram channel adapter...
```

下载大文件时：
```
INFO Downloading file xxx (565 MB)...
INFO Download progress: 15%
INFO Download progress: 45%
INFO Download complete: /tmp/openfang-telegram-downloads/xxx.dat
```

## 🎯 测试步骤

1. 在 Telegram 发送 >20MB 视频
2. 观察日志中的下载进度
3. 检查文件：`ls -lh /tmp/openfang-telegram-downloads/`
4. 验证智能体能访问文件

## 🔗 获取 API Credentials

1. 访问：https://my.telegram.org/apps
2. 登录 Telegram 账号
3. 创建新应用
4. 获取 `api_id`（数字）和 `api_hash`（32位字符串）

## 💡 提示

- 环境变量建议添加到 `~/.zshrc`
- 下载目录默认：`/tmp/openfang-telegram-downloads`
- 最大文件大小：2GB
- 端口：8081（Local API）、4200（OpenFang API）

## 🆘 需要帮助？

查看完整文档：
```bash
cat docs/telegram-deployment-guide.md
```

运行配置向导：
```bash
./scripts/install-telegram-local-api.sh
./scripts/setup-telegram-local-api.sh
```
