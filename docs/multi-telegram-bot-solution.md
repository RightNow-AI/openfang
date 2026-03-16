# 多 Telegram Bot 支持方案

## 当前问题

OpenFang 的 `config.toml` 中 `channels.telegram` 只支持配置一个 Telegram bot：

```toml
[channels.telegram]
default_agent = "shipinfabu-hand"
poll_interval_secs = 1
```

所有 hand 共享同一个 bot（`@linyiagibot`），无法实现每个 hand 对应独立的 bot。

## 方案对比

### 方案 1：使用 Agent Bindings（推荐）

利用 OpenFang 现有的 binding 机制，根据 Telegram user ID 路由到不同的 agent。

**优点**：
- 无需修改 OpenFang 源码
- 配置简单，立即可用
- 一个 bot 服务多个用户/场景

**缺点**：
- 仍然是单个 bot，无法实现完全独立的 bot 身份
- 所有用户看到的都是同一个 bot 名称和头像

**配置示例**：

```toml
# config.toml

[channels.telegram]
# 不设置 default_agent，让 bindings 决定路由
poll_interval_secs = 1

# 根据 Telegram user ID 路由到不同的 hand
[[bindings]]
agent = "shipinfabu-hand"
[bindings.match_rule]
channel = "telegram"
peer_id = "8522660072"  # 你的 Telegram user ID

[[bindings]]
agent = "browser-hand"
[bindings.match_rule]
channel = "telegram"
peer_id = "6334669965"  # 另一个用户的 ID
```

### 方案 2：修改 OpenFang 支持多 Telegram 实例

修改 OpenFang 源码，支持配置多个 Telegram bot。

**优点**：
- 每个 hand 有独立的 bot 身份（名称、头像、token）
- 完全隔离，互不干扰

**缺点**：
- 需要修改 OpenFang 源码（约 200-300 行）
- 需要为每个 hand 创建独立的 Telegram bot
- 配置更复杂

**实现思路**：

1. 修改 `ChannelsConfig` 支持多个 Telegram 配置：
   ```rust
   pub struct ChannelsConfig {
       pub telegram: Option<TelegramConfig>,           // 保留向后兼容
       pub telegram_bots: Vec<TelegramBotConfig>,      // 新增：多 bot 支持
       // ...
   }

   pub struct TelegramBotConfig {
       pub name: String,                    // bot 标识，如 "shipinfabu-bot"
       pub bot_token_env: String,           // 环境变量名
       pub default_agent: Option<String>,   // 绑定的 agent
       pub allowed_users: Vec<String>,
       // ...
   }
   ```

2. 修改 `channel_bridge.rs` 启动多个 Telegram adapter：
   ```rust
   // 为每个 telegram_bots 配置启动独立的 adapter
   for bot_config in config.telegram_bots {
       let adapter = TelegramAdapter::new(...);
       // 每个 adapter 有独立的 router 和 default_agent
   }
   ```

3. 配置示例：
   ```toml
   [[channels.telegram_bots]]
   name = "shipinfabu-bot"
   bot_token_env = "TELEGRAM_BOT_TOKEN_SHIPINFABU"
   default_agent = "shipinfabu-hand"
   allowed_users = ["8522660072"]

   [[channels.telegram_bots]]
   name = "browser-bot"
   bot_token_env = "TELEGRAM_BOT_TOKEN_BROWSER"
   default_agent = "browser-hand"
   allowed_users = ["8522660072"]
   ```

4. secrets.env：
   ```bash
   TELEGRAM_BOT_TOKEN_SHIPINFABU=8698293972:AAF...
   TELEGRAM_BOT_TOKEN_BROWSER=7234567890:BBG...
   ```

### 方案 3：运行多个 OpenFang 实例（不推荐）

为每个 hand 运行独立的 OpenFang daemon。

**优点**：
- 完全隔离
- 无需修改代码

**缺点**：
- 资源消耗大（每个实例独立进程）
- 管理复杂（多个配置文件、多个端口）
- hand 之间无法通信

## 推荐方案

### 短期：方案 1（Agent Bindings）

如果你的需求是"不同用户使用不同的 hand"，使用 bindings 即可：

```toml
# ~/.openfang/config.toml

[channels.telegram]
poll_interval_secs = 1
# 不设置 default_agent

# 你自己的消息路由到 shipinfabu-hand
[[bindings]]
agent = "shipinfabu-hand"
[bindings.match_rule]
channel = "telegram"
peer_id = "8522660072"

# 其他管理员的消息路由到 browser-hand
[[bindings]]
agent = "browser-hand"
[bindings.match_rule]
channel = "telegram"
peer_id = "6334669965"
```

### 长期：方案 2（多 Bot 支持）

如果你需要每个 hand 有独立的 bot 身份（不同的名称、头像），我可以帮你实现方案 2。

## 实施步骤

### 如果选择方案 1（立即可用）

1. 修改 `~/.openfang/config.toml`，添加 bindings
2. 重启 OpenFang：`openfang stop && openfang start`
3. 测试：不同用户发送消息，验证路由到不同的 hand

### 如果选择方案 2（需要开发）

1. 修改 OpenFang 源码（我可以帮你实现）
2. 为每个 hand 创建独立的 Telegram bot（通过 @BotFather）
3. 配置多个 bot token
4. 编译部署新版本 OpenFang
5. 测试多 bot 功能

## 你的选择？

请告诉我：
1. 你是否需要每个 hand 有独立的 bot 身份（名称、头像）？
2. 还是只需要根据用户 ID 路由到不同的 hand？

如果是后者，我可以立即帮你配置方案 1。
如果是前者，我可以帮你实现方案 2 的代码修改。
