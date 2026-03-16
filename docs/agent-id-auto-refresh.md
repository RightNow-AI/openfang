# Agent ID 缓存自动刷新机制

## 问题描述

在 OpenFang 框架中，Telegram channel bridge 在启动时会从配置文件读取 `default_agent` 名称，然后解析为 agent ID 并缓存在 `AgentRouter` 中。

当 OpenFang daemon 重启或 agent 被 reconcile 机制重建时：
1. 旧的 agent 被 killed，生成新的 agent ID
2. Telegram bridge 仍然缓存着旧的 agent ID
3. 后续所有 Telegram 消息都会路由到已失效的 agent ID
4. 导致 "Agent not found" 错误

## 根本原因

`AgentRouter` 只在初始化时设置 channel default，没有监听 agent 生命周期事件来更新缓存。

## 解决方案

### 1. 扩展 AgentRouter 功能

在 `crates/openfang-channels/src/router.rs` 中：

- 添加 `channel_default_names: DashMap<String, String>` 字段，存储 channel -> agent_name 映射
- 添加 `set_channel_default_with_name()` 方法，同时保存 agent ID 和 name
- 添加 `channel_default_name()` 方法，查询 channel 的 default agent name
- 添加 `update_channel_default()` 方法，更新 channel 的 agent ID

### 2. 监听 Agent 生命周期事件

在 `crates/openfang-api/src/channel_bridge.rs` 的 `start_channel_bridge_with_config()` 中：

```rust
// 启动后台任务监听 agent 生命周期事件
tokio::spawn(async move {
    let mut event_rx = kernel.event_bus.subscribe_all();

    loop {
        match event_rx.recv().await {
            Ok(event) => {
                if let EventPayload::Lifecycle(LifecycleEvent::Spawned { agent_id, name }) = event.payload {
                    // 更新 router 的 agent name cache
                    router.register_agent(name.clone(), agent_id);

                    // 检查是否是某个 channel 的 default agent
                    for channel_key in ["Telegram", "Discord", ...] {
                        if let Some(expected_name) = router.channel_default_name(channel_key) {
                            if expected_name == name {
                                router.update_channel_default(channel_key, agent_id);
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

### 3. 初始化时保存 Agent 名称

修改 channel default 注册逻辑：

```rust
// 旧代码
router.set_channel_default(channel_key, agent_id);

// 新代码
router.set_channel_default_with_name(channel_key, agent_id, name.clone());
```

## 工作流程

1. **初始化阶段**：
   - 读取配置文件中的 `default_agent = "shipinfabu-hand"`
   - 解析为 agent ID（例如 `6680daef-...`）
   - 调用 `router.set_channel_default_with_name("Telegram", agent_id, "shipinfabu-hand")`
   - 同时保存 ID 和 name

2. **Agent 重建时**：
   - OpenFang reconcile 机制 kill 旧 agent
   - 创建新 agent，生成新 ID（例如 `b31df143-...`）
   - Kernel 发布 `LifecycleEvent::Spawned` 事件

3. **自动更新阶段**：
   - 后台监听任务收到 Spawned 事件
   - 检查 agent name 是否匹配某个 channel 的 default agent
   - 如果匹配，自动更新 router 中的 agent ID 缓存
   - 后续消息自动路由到新 agent ID

## 优势

1. **零停机时间**：agent 重建后立即自动更新，无需手动重启 daemon
2. **透明修复**：用户无感知，不会出现 "Agent not found" 错误
3. **精确更新**：只更新匹配的 channel，不影响其他 channel
4. **可扩展**：支持所有 channel 类型（Telegram、Discord、Slack 等）

## 测试

创建了 `crates/openfang-api/tests/agent_respawn_test.rs`：

- `test_agent_respawn_updates_router`：验证 agent 重建后 router 自动更新
- `test_agent_respawn_only_updates_matching_channel`：验证只更新匹配的 channel

## 影响范围

- 修改文件：
  - `crates/openfang-channels/src/router.rs`（已有代码，无需修改）
  - `crates/openfang-api/src/channel_bridge.rs`（添加后台监听任务）
- 新增文件：
  - `crates/openfang-api/tests/agent_respawn_test.rs`（测试）
  - `docs/agent-id-auto-refresh.md`（本文档）

## 向后兼容性

- 完全向后兼容，不影响现有配置
- 旧的 `set_channel_default()` 方法仍然可用
- 新的 `set_channel_default_with_name()` 是可选的增强功能

## 部署建议

1. 编译并测试修改后的 OpenFang
2. 部署到本地环境验证
3. 观察日志中的 "Updated channel default agent ID after respawn" 消息
4. 确认 Telegram 消息在 agent 重建后仍能正常处理
5. 部署到生产环境

## 相关 Issue

- 原始问题：Telegram 媒体组发送时触发 9 次 "Agent not found" 错误
- 根本原因：Agent ID 缓存失效
- 临时解决方案：手动重启 OpenFang daemon
- 永久解决方案：本文档描述的自动刷新机制
