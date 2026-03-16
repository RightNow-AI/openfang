# OpenFang Agent ID 缓存问题修复总结

## 问题回顾

**症状**：
- 发送 Telegram 媒体组时，收到 9 条 "Agent not found: 6680daef-ba64-4a94-95e6-f3abeadcf251" 错误
- 每次 OpenFang daemon 重启后，Telegram bot 都会失效
- 需要手动重启 daemon 才能恢复

**根本原因**：
1. OpenFang 的 reconcile 机制会定期 kill 和重建 agent
2. Agent 重建后会生成新的 UUID
3. Telegram channel bridge 在启动时缓存了 agent ID，但不会监听 agent 生命周期事件
4. 导致后续消息路由到已失效的 agent ID

## 解决方案

### 核心思路

在 channel bridge 启动时，添加一个后台任务监听 `LifecycleEvent::Spawned` 事件，当检测到 agent 重建时自动更新 router 中的缓存。

### 代码修改

#### 1. 修改 `crates/openfang-api/src/channel_bridge.rs`

在 `start_channel_bridge_with_config()` 函数中添加后台监听任务：

```rust
// 启动后台任务监听 agent 生命周期事件
let kernel_clone = kernel.clone();
let router_clone = router.clone();
tokio::spawn(async move {
    use openfang_types::event::{EventPayload, LifecycleEvent};
    let mut event_rx = kernel_clone.event_bus.subscribe_all();

    loop {
        match event_rx.recv().await {
            Ok(event) => {
                if let EventPayload::Lifecycle(LifecycleEvent::Spawned { agent_id, name }) = event.payload {
                    // 更新 router 的 agent name cache
                    router_clone.register_agent(name.clone(), agent_id);

                    // 检查是否是某个 channel 的 default agent
                    for channel_key in ["Telegram", "Discord", "Slack", ...] {
                        if let Some(expected_name) = router_clone.channel_default_name(channel_key) {
                            if expected_name == name {
                                router_clone.update_channel_default(channel_key, agent_id);
                                info!("Updated {channel_key} default agent ID after respawn");
                            }
                        }
                    }
                }
            }
            Err(_) => break,
        }
    }
});
```

同时修改初始化代码，使用 `set_channel_default_with_name()` 保存 agent 名称：

```rust
// 旧代码
router.set_channel_default(channel_key, agent_id);

// 新代码
router.set_channel_default_with_name(channel_key, agent_id, name.clone());
```

#### 2. 利用现有的 `AgentRouter` API

`crates/openfang-channels/src/router.rs` 中已经提供了所需的方法：
- `set_channel_default_with_name()` - 同时保存 agent ID 和 name
- `channel_default_name()` - 查询 channel 的 default agent name
- `update_channel_default()` - 更新 channel 的 agent ID
- `register_agent()` - 更新 agent name -> ID 映射

无需修改 router.rs，直接使用现有 API。

### 测试

创建了 `crates/openfang-api/tests/agent_respawn_test.rs`：

```bash
$ cargo test --package openfang-api --test agent_respawn_test

running 2 tests
test test_agent_respawn_updates_router ... ok
test test_agent_respawn_only_updates_matching_channel ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## 工作流程

### 修复前

```
1. OpenFang daemon 启动
   └─> Telegram bridge 缓存 agent ID: 6680daef-...

2. Agent reconcile 触发
   └─> Kill agent 6680daef-...
   └─> 创建新 agent b31df143-...

3. 用户发送 Telegram 消息
   └─> Bridge 使用缓存的 6680daef-...
   └─> ❌ "Agent not found" 错误

4. 手动重启 daemon
   └─> Bridge 重新读取配置
   └─> 缓存新的 agent ID
   └─> ✅ 恢复正常
```

### 修复后

```
1. OpenFang daemon 启动
   └─> Telegram bridge 缓存 agent ID: 6680daef-...
   └─> 同时保存 agent name: "shipinfabu-hand"
   └─> 启动后台监听任务

2. Agent reconcile 触发
   └─> Kill agent 6680daef-...
   └─> 创建新 agent b31df143-...
   └─> 发布 LifecycleEvent::Spawned 事件

3. 后台监听任务收到事件
   └─> 检测到 "shipinfabu-hand" 重建
   └─> 自动更新 Telegram bridge 缓存: b31df143-...
   └─> 📝 日志: "Updated Telegram default agent ID after respawn"

4. 用户发送 Telegram 消息
   └─> Bridge 使用最新的 b31df143-...
   └─> ✅ 正常处理，无需手动干预
```

## 优势

1. **零停机时间**：agent 重建后立即自动更新，无需手动重启
2. **透明修复**：用户无感知，不会出现 "Agent not found" 错误
3. **精确更新**：只更新匹配的 channel，不影响其他 channel
4. **可扩展**：支持所有 channel 类型（Telegram、Discord、Slack 等）
5. **向后兼容**：不影响现有配置和行为

## 验证步骤

1. 编译修改后的 OpenFang：
   ```bash
   cd /Users/xiaomo/Desktop/openfang-upstream-fork
   cargo build --release
   ```

2. 替换本地 OpenFang 二进制：
   ```bash
   cp target/release/openfang ~/.cargo/bin/openfang
   # 或者
   cargo install --path crates/openfang-cli
   ```

3. 重启 OpenFang daemon：
   ```bash
   openfang stop
   openfang start
   ```

4. 观察日志：
   ```bash
   tail -f ~/.openfang/daemon-reconcile.stdout.log
   ```

5. 等待 agent 自然重建（或手动触发）：
   ```bash
   # 手动触发 agent 重建（可选）
   openfang hand deactivate shipinfabu-hand
   openfang hand activate shipinfabu-hand
   ```

6. 检查日志中是否出现：
   ```
   Updated Telegram default agent ID after respawn
   ```

7. 在 Telegram 中发送测试消息，验证 bot 正常响应

## 文件清单

### 修改的文件
- `crates/openfang-api/src/channel_bridge.rs`
  - 添加后台监听任务（约 40 行）
  - 修改初始化逻辑使用 `set_channel_default_with_name()`

### 新增的文件
- `crates/openfang-api/tests/agent_respawn_test.rs` - 单元测试
- `docs/agent-id-auto-refresh.md` - 详细技术文档
- `AGENT_ID_FIX_SUMMARY.md` - 本文档

### 未修改的文件
- `crates/openfang-channels/src/router.rs` - 已有所需 API，无需修改

## 部署建议

### 本地环境（Mac）

1. 编译并安装：
   ```bash
   cd /Users/xiaomo/Desktop/openfang-upstream-fork
   cargo install --path crates/openfang-cli
   ```

2. 重启服务：
   ```bash
   openfang stop && sleep 2 && openfang start
   ```

3. 验证修复：
   - 在 Telegram 中给 `@linyiagibot` 发送测试消息
   - 观察日志确认没有 "Agent not found" 错误

### 远程服务器（如果需要）

1. 同步代码到服务器：
   ```bash
   rsync -avz /Users/xiaomo/Desktop/openfang-upstream-fork/ root@144.48.4.99:/root/openfang-upstream-fork/
   ```

2. SSH 登录并编译：
   ```bash
   ssh root@144.48.4.99
   cd /root/openfang-upstream-fork
   cargo install --path crates/openfang-cli
   ```

3. 重启服务：
   ```bash
   openfang stop && sleep 2 && openfang start
   ```

## 相关文档

- `docs/agent-id-auto-refresh.md` - 详细技术文档
- `/Users/xiaomo/Desktop/技能仓库/shipinbot-openfang-cluster-ops/references/agent-id-cache-issue.md` - 原始问题分析
- `/Users/xiaomo/Desktop/shipinbot/docs/问题汇总与优先级-修正版.md` - 问题汇总

## 后续工作

1. ✅ 代码实现完成
2. ✅ 单元测试通过
3. ✅ 编译验证通过
4. ⏳ 本地环境部署验证
5. ⏳ 观察生产环境稳定性
6. ⏳ 考虑向 OpenFang 官方提交 PR

## 总结

这个修复从根本上解决了 agent ID 缓存失效的问题，通过监听 agent 生命周期事件实现了自动刷新机制。修复后，用户不再需要手动重启 daemon，系统会自动处理 agent 重建带来的 ID 变化。

修复方案简洁、高效、可靠，充分利用了 OpenFang 现有的事件总线和 router API，没有引入额外的复杂性。
