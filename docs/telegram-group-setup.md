# Telegram 群组配置指南

本文档说明如何配置 OpenFang Telegram bot 在群组中正确工作。

## 核心概念：两层过滤机制

OpenFang 的 Telegram 群组消息处理采用**两层过滤**：

### 第 1 层：Telegram 服务器层（BotFather 设置）

控制 Telegram 服务器**是否将消息发送给 bot**。

**Group Privacy = ON（默认）：**
- ✅ Bot 接收 `/命令` 格式的消息（如 `/start@botname`）
- ❌ Bot **不接收** @mention 消息（如 `@botname 你好`）
- ❌ Bot **不接收** 普通群聊消息

**Group Privacy = OFF：**
- ✅ Bot 接收群组中的**所有消息**
- ✅ 包括 @mention、普通消息、命令等

### 第 2 层：OpenFang 应用层（配置文件）

控制 bot **如何响应**收到的消息。

配置项：`[channels.telegram.overrides]` 中的 `group_policy`

| 选项 | 行为 |
|------|------|
| `ignore` | 忽略所有群组消息 |
| `commands_only` | 只响应 `/命令` |
| `mention_only` | 只响应 @mention（**推荐**） |
| `all` | 响应所有群组消息（不推荐） |

## 推荐配置：只响应 @mention

### 步骤 1：在 BotFather 中关闭 Group Privacy

1. 打开 Telegram，搜索 `@BotFather`
2. 发送 `/mybots`
3. 选择你的 bot
4. 点击 `Bot Settings`
5. 点击 `Group Privacy`
6. 选择 `Turn off`

**验证：** 发送 `/mybots` 再次查看，`Group Privacy` 应显示为 `OFF`。

### 步骤 2：配置 OpenFang

编辑 `~/.openfang/config.toml`：

```toml
[channels.telegram]
bot_token_env = "TELEGRAM_BOT_TOKEN"
default_agent = "your-agent-name"
allowed_users = ["user_id_1", "user_id_2", "-group_id"]  # 群组 ID 以负数表示

[channels.telegram.overrides]
dm_policy = "respond"           # 私聊直接响应
group_policy = "mention_only"   # 群组只响应 @mention
```

### 步骤 3：注册群组为 RBAC 用户

如果启用了 RBAC，需要将群组注册为用户：

```toml
[[users]]
name = "group-123456789"
role = "admin"

[users.channel_bindings]
telegram = "-123456789"  # 群组 ID（负数）
```

### 步骤 4：重启 OpenFang

```bash
# 停止旧进程
pkill -9 openfang
pkill -9 telegram-bot-api

# 加载环境变量并启动
source ~/.openfang/secrets.env
target/release/openfang start &
```

## 工作原理

配置完成后，消息流程如下：

```
用户在群组发送: "@botname 你好"
    ↓
Telegram 服务器（Group Privacy = OFF）
    ↓ 发送消息给 bot
OpenFang 接收消息
    ↓
检查 group_policy = "mention_only"
    ↓
检测到 @mention → 响应 ✅
```

```
用户在群组发送: "普通消息"
    ↓
Telegram 服务器（Group Privacy = OFF）
    ↓ 发送消息给 bot
OpenFang 接收消息
    ↓
检查 group_policy = "mention_only"
    ↓
未检测到 @mention → 忽略 ❌
```

## 常见问题

### Q: 为什么必须关闭 Group Privacy？

A: Telegram 的 Group Privacy 机制不会将 @mention 视为"命令"，所以即使用户 @mention bot，Telegram 也不会发送消息给 bot。只有关闭 Group Privacy，Telegram 才会发送所有消息（包括 @mention）。

### Q: 关闭 Group Privacy 后，bot 会响应所有群聊吗？

A: 不会。虽然 bot 会**接收**所有消息，但 OpenFang 的 `group_policy = "mention_only"` 会在应用层过滤，只响应 @mention 的消息。

### Q: 如何获取群组 ID？

A: 有几种方法：

1. **通过 bot 日志**：将 bot 添加到群组后，查看日志中的 `chat_id`
2. **使用 API**：
   ```bash
   curl -s "https://api.telegram.org/bot<TOKEN>/getUpdates" | jq '.result[].message.chat.id'
   ```
3. **使用第三方 bot**：如 `@userinfobot`，将其添加到群组即可显示群组 ID

### Q: 群组 ID 为什么是负数？

A: Telegram 的规则：
- 私聊用户 ID：正数（如 `123456789`）
- 群组 ID：负数（如 `-987654321`）
- 超级群组 ID：负数，通常以 `-100` 开头（如 `-1001234567890`）

### Q: 如何测试配置是否生效？

A: 在群组中发送：

```
@botname 你好
```

如果 bot 回复，说明配置成功。如果没有回复：
1. 检查 BotFather 中 Group Privacy 是否为 OFF
2. 检查 `~/.openfang/config.toml` 中 `group_policy` 是否为 `mention_only`
3. 检查群组 ID 是否在 `allowed_users` 中
4. 检查群组是否注册为 RBAC 用户（如果启用了 RBAC）
5. 查看日志：`tail -f /tmp/openfang-*.log`

## 其他群组策略

### 只响应命令（commands_only）

适合只提供特定功能的 bot：

```toml
[channels.telegram.overrides]
group_policy = "commands_only"
```

用户需要发送：`/start@botname` 或 `/help@botname`

### 响应所有消息（all）

**不推荐**，会导致 bot 响应群组中的所有消息：

```toml
[channels.telegram.overrides]
group_policy = "all"
```

### 完全忽略群组（ignore）

Bot 不响应任何群组消息：

```toml
[channels.telegram.overrides]
group_policy = "ignore"
```

## 环境变量

确保设置了必要的环境变量：

```bash
# ~/.openfang/secrets.env
TELEGRAM_BOT_TOKEN=123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11
TELEGRAM_API_HASH=abcdef1234567890abcdef1234567890  # 仅使用 Local Bot API 时需要
```

## 相关文档

- [Telegram Bot API 官方文档](https://core.telegram.org/bots/api)
- [BotFather 使用指南](https://core.telegram.org/bots#6-botfather)
- [OpenFang 配置参考](./configuration.md)
- [OpenFang Telegram 部署指南](./telegram-deployment-guide.md)
