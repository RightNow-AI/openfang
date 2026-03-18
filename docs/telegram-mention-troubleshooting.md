# Telegram @Mention 故障排查指南

## 问题现象

用户报告 Telegram 群组中无法使用 `@botname 消息` 格式发送消息，表现为：
- 输入 `@botname ` 后发送按钮消失或无法点击
- 私聊正常，但群组 @mention 失败
- 有时能成功，但不稳定（成功几次后失效）

## 根本原因

### 1. UTF-16 字节索引 Bug（已修复）

**问题代码**（commit 52ccc8a 之前）：
```rust
// telegram.rs:2412 (旧版本)
let offset = entity["offset"].as_i64().unwrap_or(0) as usize;
let length = entity["length"].as_i64().unwrap_or(0) as usize;
if offset + length <= text.len() {
    let mention_text = &text[offset..offset + length];  // ❌ 直接使用字节索引
}
```

**问题分析**：
- Telegram API 返回的 `offset` 和 `length` 是 **UTF-16 编码单元**
- Rust 字符串使用 **UTF-8 编码**
- 直接用 UTF-16 offset 切片 UTF-8 字符串会导致 panic
- 当消息包含中文字符（如 `：@botname 你好`）时触发

**错误信息**：
```
thread 'tokio-rt-worker' panicked at crates/openfang-channels/src/telegram.rs:2412:45:
byte index 1 is not a char boundary; it is inside '：' (bytes 0..3) of `：@linyiagibot 你好`
```

**修复方案**（commit 52ccc8a）：
```rust
// telegram.rs:2509-2517 (新版本)
let offset = entity["offset"].as_i64().unwrap_or(0) as usize;
let length = entity["length"].as_i64().unwrap_or(0) as usize;
if let Some(range) = telegram_entity_utf16_range_to_bytes(text, offset, length) {
    let mention_text = &text[range];  // ✅ 使用 UTF-16 转换函数
    if mention_text.to_lowercase() == bot_mention {
        return true;
    }
}
```

**转换函数**（telegram.rs:1536-1568）：
```rust
fn telegram_entity_utf16_range_to_bytes(
    text: &str,
    offset_utf16: usize,
    length_utf16: usize,
) -> Option<std::ops::Range<usize>> {
    let end_utf16 = offset_utf16.checked_add(length_utf16)?;
    let mut utf16_index = 0usize;
    let mut start_byte = None;
    let mut end_byte = None;

    for (byte_index, ch) in text.char_indices() {
        if start_byte.is_none() && utf16_index == offset_utf16 {
            start_byte = Some(byte_index);
        }
        if end_byte.is_none() && utf16_index == end_utf16 {
            end_byte = Some(byte_index);
            break;
        }
        utf16_index += ch.len_utf16();
    }

    // 处理边界情况
    if start_byte.is_none() && utf16_index == offset_utf16 {
        start_byte = Some(text.len());
    }
    if end_byte.is_none() && utf16_index == end_utf16 {
        end_byte = Some(text.len());
    }

    match (start_byte, end_byte) {
        (Some(start), Some(end)) if start <= end => Some(start..end),
        _ => None,
    }
}
```

### 2. BotFather Inline Mode 配置问题

**问题现象**：
- 输入 `@botname ` 后出现 "Search..." 提示
- 无法发送普通 @mention 消息
- 发送按钮消失或变灰

**根本原因**：
- Inline Mode 开启时，`@botname ` 后的空格会触发 inline 搜索模式
- OpenFang 当前不支持 inline query 处理
- Telegram 客户端等待 inline 结果，阻止普通消息发送

**解决方案**：
1. 打开 @BotFather
2. 发送 `/mybots`
3. 选择你的 bot
4. Bot Settings → Inline Mode → **Turn off**
5. 重启 Telegram 客户端

**验证**：
- Inline Mode 关闭后，Bot Settings 中应显示 "Turn on"
- 输入 `@botname ` 不再出现 "Search..." 提示
- 可以正常发送 `@botname 消息`

### 3. Group Privacy 配置

**正确配置**：
- Group Privacy = **ON**（开启）
- 作用：Bot 只响应 @mention、回复、命令
- 这是推荐配置，避免 bot 监听所有群消息

**常见误解**：
- ❌ 误解：Group Privacy ON 会阻止 @mention
- ✅ 事实：Group Privacy ON 是为了让 bot **只响应** @mention

## 完整排查流程

### 步骤 1：检查代码版本

```bash
cd /path/to/openfang-upstream-fork
git log --oneline -1
```

确认当前 commit 是 `52ccc8a` 或更新版本。如果不是，需要更新代码：

```bash
git pull origin main
cargo clean -p openfang-cli
cargo build --release -p openfang-cli
cp target/release/openfang ~/.openfang/bin/openfang
```

### 步骤 2：检查 BotFather 配置

1. **Group Privacy**：
   - 应该是 **ON**（开启）
   - 验证：Bot Settings → Group Privacy → 显示 "Turn off" 说明当前是 ON

2. **Inline Mode**：
   - 应该是 **OFF**（关闭）
   - 验证：Bot Settings → Inline Mode → 显示 "Turn on" 说明当前是 OFF
   - 如果是 ON，点击 "Turn off" 关闭

### 步骤 3：重启服务

```bash
# 停止所有进程
pkill -9 openfang
pkill -9 telegram-bot-api
sleep 3

# 启动 OpenFang
openfang start
```

### 步骤 4：测试

1. **私聊测试**：
   - 搜索 @botname
   - 发送 "你好"
   - 应该收到回复

2. **群组测试**：
   - 在群组中发送 `@botname 你好`
   - 应该收到回复

### 步骤 5：查看日志

```bash
# 查找最新日志文件
ls -lt ~/Desktop/openfang-*.log ~/Library/Logs/openfang-*.log | head -5

# 查看日志
tail -f /path/to/latest.log
```

**正常日志**：
```
INFO openfang_channels::telegram: Telegram bot @botname connected
INFO openfang_channels::telegram: Telegram polling loop started
INFO openfang_channels::telegram: Telegram getUpdates returned messages count=1
INFO openfang_runtime::agent_loop: Agent loop completed
```

**异常日志**：
```
thread 'tokio-rt-worker' panicked at crates/openfang-channels/src/telegram.rs:2412:45:
byte index 1 is not a char boundary
```
→ 说明代码版本过旧，需要更新

## 常见问题

### Q1: 为什么 `@botname你好`（无空格）能发送，但 `@botname 你好`（有空格）不能？

**A**: Inline Mode 开启导致。关闭 Inline Mode 即可解决。

### Q2: 为什么之前能成功几次，后来就失败了？

**A**: UTF-16 bug 导致 Telegram 通道崩溃。当收到包含中文字符的 @mention 时触发 panic，通道断开。重启后能临时恢复，但再次遇到中文字符又会崩溃。

### Q3: 私聊正常，群组不正常，是什么原因？

**A**: 可能的原因：
1. Bot 不在群组成员列表中（需要重新添加）
2. Inline Mode 开启（需要关闭）
3. 群组是私密群组且 bot 没有管理员权限（建议给予管理员权限）

### Q4: 如何确认 UTF-16 bug 已修复？

**A**: 检查代码中是否使用了 `telegram_entity_utf16_range_to_bytes` 函数：

```bash
grep -n "telegram_entity_utf16_range_to_bytes" crates/openfang-channels/src/telegram.rs
```

应该看到类似输出：
```
1536:fn telegram_entity_utf16_range_to_bytes(
2511:    if let Some(range) = telegram_entity_utf16_range_to_bytes(text, offset, length)
```

### Q5: 如何测试 UTF-16 处理是否正确？

**A**: 发送包含中文字符的 @mention：
```
：@botname 你好
中文：@botname 测试
```

如果不 panic 且能正常响应，说明修复成功。

## 预防措施

1. **定期更新代码**：
   ```bash
   cd /path/to/openfang-upstream-fork
   git pull origin main
   cargo build --release -p openfang-cli
   cp target/release/openfang ~/.openfang/bin/openfang
   ```

2. **监控日志**：
   - 设置日志轮转，避免日志文件过大
   - 定期检查是否有 panic 或 ERROR

3. **BotFather 配置检查清单**：
   - ✅ Group Privacy = ON
   - ✅ Inline Mode = OFF
   - ✅ Bot 在群组成员列表中
   - ✅ Bot 有必要的权限（建议给管理员权限）

## 相关文档

- [Telegram Group Setup Guide](./telegram-group-setup.md)
- [CLAUDE.md - Telegram Configuration](../CLAUDE.md#telegram-group-configuration)
- [Telegram Bot API - Entities](https://core.telegram.org/bots/api#messageentity)

## 更新历史

- 2026-03-18: 初始版本，记录 UTF-16 bug 修复和 Inline Mode 配置问题
