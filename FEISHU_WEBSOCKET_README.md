# Feishu WebSocket 长连接实现说明

## 实现概述

OpenFang 现已支持飞书 WebSocket 长连接模式，无需公网 IP 或内网穿透工具即可在本地环境中接收飞书事件。

## 配置方式

### 1. 配置文件 (`~/.openfang/config.toml`)

```toml
[channels.feishu]
app_id = "cli_a922bea3c6785cc8"
app_secret_env = "FEISHU_APP_SECRET"
connection_mode = "websocket"  # 改为 websocket
webhook_port = 8453  # WebSocket 模式下不需要
default_agent = "assistant"
```

### 2. 支持的连接模式

| 模式 | 说明 | 需求 |
|------|------|------|
| `webhook` | HTTP Webhook 回调（默认） | 公网 IP 或内网穿透 |
| `websocket` | WebSocket 长连接 | 仅需出网访问能力 |

## 安装步骤

```bash
# 1. 编译已完成，位于 target/release/openfang-cli

# 2. 安装到系统
sudo cp target/release/openfang-cli /usr/local/bin/openfang
sudo chmod +x /usr/local/bin/openfang

# 3. 停止旧版本（如果运行中）
openfang stop

# 4. 启动新版本
openfang start

# 5. 查看日志确认 WebSocket 连接状态
openfang logs
```

## 验证方法

### 方法1：检查日志

```bash
openfang logs
```

期望看到：
```
Feishu adapter authenticated as [bot名称]
Feishu: Using WebSocket long connection mode
Feishu: Connecting to WebSocket at wss://open.feishu.cn/ws-3
Feishu: WebSocket hello message sent
```

### 方法2：飞书开发者后台配置

1. 登录飞书开发者后台
2. 进入你的应用
3. 选择 **事件订阅** → **订阅方式**
4. 选择 **使用长连接接收事件**
5. 保存配置

配置保存后，飞书会检测长连接是否建立成功。

## 实现细节

### WebSocket 协议

1. **连接端点**: `wss://open.feishu.cn/ws-3`
2. **认证方式**: 发送包含 app_id 和 app_secret 的 JSON 消息
3. **事件格式**: 与 Webhook 模式相同的 JSON 格式

### 自动重连

- 连接断开后会自动重连（间隔 5 秒）
- 收到 shutdown 信号时会停止重连
- 错误会被记录到日志中

### 代码变更

**新增的文件结构**:
- `openfang-types/src/config.rs`: 添加了 `FeishuConnectionMode` 枚举
- `openfang-channels/src/feishu.rs`: 添加了 WebSocket 连接逻辑
- `openfang-api/src/channel_bridge.rs`: 更新了适配器初始化

**核心函数**:
```rust
// WebSocket 连接处理
async fn run_websocket(&self, tx: mpsc::Sender<ChannelMessage>)
    -> Result<(), Box<dyn std::error::Error + Send + Sync>>

// start() 方法根据 connection_mode 分发
match self.connection_mode {
    FeishuConnectionMode::WebSocket => { /* WebSocket 逻辑 */ }
    FeishuConnectionMode::Webhook => { /* Webhook 逻辑 */ }
}
```

## 与 OpenClaw 的对比

| 特性 | OpenClaw (Node.js) | OpenFang (Rust) |
|------|-------------------|----------------|
| SDK | 官方 @larksuiteoapi/node-sdk | 手动实现 |
| WebSocket 支持 | ✅ 原生支持 | ✅ 已实现 |
| 认证方式 | SDK 内部处理 | 手动发送认证消息 |
| 事件解析 | SDK 内部处理 | 手动解析 JSON |

## 故障排查

### 问题：连接失败

```bash
# 检查网络
ping open.feishu.cn

# 检查应用凭证
openfang vault list | grep FEISHU
```

### 问题：认证失败

确认 app_id 和 app_secret 正确：
```bash
# 查看 vault 中的密钥
openfang vault list
```

### 问题：未收到事件

1. 确认飞书后台配置了事件订阅
2. 确认选择了"使用长连接接收事件"
3. 检查日志中是否有 WebSocket 连接成功的日志

## 下一步

1. 安装新编译的二进制文件
2. 重启 OpenFang
3. 在飞书后台配置长连接模式
4. 测试发送消息到飞书应用

## 参考文档

- [飞书长连接文档](https://open.feishu.cn/document/server-docs/event-subscription-guide/event-subscription-configure-/request-url-configuration-case)
- [OpenClaw Feishu 实现](~/.nvm/versions/node/v24.13.1/lib/node_modules/openclaw/extensions/feishu/)
