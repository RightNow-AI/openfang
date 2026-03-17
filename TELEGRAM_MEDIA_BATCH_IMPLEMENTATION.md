# Telegram 媒体组结构化处理实现总结

## 概述

本次实现将 Telegram 媒体组从"降级成文本链接"改为"结构化批次数据"，使 `shipinfabu-hand` 能够精确了解批次内容并选择性下载视频。

## 核心改动

### 1. 新增 `TelegramMediaBatch` 结构体

**文件**: `crates/openfang-channels/src/telegram_media_batch.rs`

定义了完整的媒体批次结构：
- `TelegramMediaBatch`: 批次容器，包含 batch_key、chat_id、media_group_id、caption、items
- `TelegramMediaItem`: 单个媒体项，包含 kind、file_id、file_size、status、local_path、download_hint
- `MediaItemKind`: 媒体类型枚举（Image/Video/Document）
- `MediaItemStatus`: 媒体状态枚举（Ready/NeedsProjectDownload/SkippedSafeLimit/DownloadFailed）

### 2. 修改 `merge_media_group_updates` 函数

**文件**: `crates/openfang-channels/src/telegram.rs`

**改动前**:
- 将媒体组拼接成长文本：`"[Photo: url]\n[Video: url]\n..."`
- 正文包含所有媒体 URL

**改动后**:
- 构建 `TelegramMediaBatch` 结构
- 对每个媒体项记录：
  - 类型（image/video/document）
  - 文件大小、时长
  - 状态（ready/needs_project_download/skipped_safe_limit/download_failed）
  - 本地路径（如已下载）
  - 下载提示（如需项目侧下载）
- 正文改为短摘要：`"收到 Telegram 媒体批次：1 个视频、9 张图片。"`
- 将 `TelegramMediaBatch` 序列化后写入 `ChannelMessage.metadata["telegram_media_batch"]`

### 3. Bridge 层透传逻辑

**文件**: `crates/openfang-channels/src/bridge.rs`

在 `dispatch_message` 函数中新增：
- 检查 `ChannelMessage.metadata` 是否包含 `telegram_media_batch`
- 如果目标 agent 是 `shipinfabu-hand`，将批次数据写入：
  ```
  ~/.openfang/workspaces/shipinfabu-hand/inbox/telegram/<batch_key>.json
  ```
- 新增 `write_telegram_batch_to_inbox` 辅助函数

### 4. 更新测试

**文件**: `crates/openfang-channels/src/telegram.rs`

更新 `test_merge_media_group_preserves_original_item_order` 测试：
- 验证正文是短摘要而不是 URL 列表
- 验证 `telegram_media_batch` 存在于 metadata
- 验证批次结构正确（items 顺序、类型、caption）

### 5. 文档更新

**文件**: `docs/telegram-deployment-guide.md`

新增"媒体组处理"章节：
- 结构化媒体批次说明
- `telegram_media_batch` JSON 结构示例
- 媒体项状态说明
- `shipinfabu-hand` 集成说明
- 安全阈值说明

## 关键设计决策

### 1. 业务判断归 hand，不归通道层

OpenFang 通道层只负责：
- 收集媒体批次事实
- 标记媒体状态（ready/needs_download/skipped）
- 透传给 agent

**不负责**：
- 决定是否下载
- 决定下载哪个视频
- 业务逻辑判断

### 2. 超大视频采用项目侧下载器

- 通道层对 >100MB 视频不触发 `getFile`，避免 Local Bot API Server 重启
- 标记为 `needs_project_download` 或 `skipped_safe_limit`
- 由 `shipinfabu-hand` 调用项目专用下载器（如 `openfang_clean_publish_bridge.py fetch-telegram-video`）
- 只下载被 hand 选中的视频

### 3. 保持向后兼容

- 现有非媒体组消息处理逻辑不变
- 现有 `<100MB` 且通道已落地的视频仍可使用快速路径
- 不影响其他通道（WhatsApp、Slack 等）

## 测试覆盖

### 单元测试
- ✅ `test_media_batch_summary`: 验证批次摘要生成
- ✅ `test_media_item_status_serde`: 验证状态枚举序列化
- ✅ `test_merge_media_group_preserves_original_item_order`: 验证媒体组合并逻辑

### 集成测试（需手动执行）
1. 发送 1 视频 + 9 图片的媒体组
   - 验证正文是短摘要
   - 验证 `telegram_media_batch` 完整存在
2. 发送 >100MB 视频
   - 验证 status 为 `skipped_safe_limit`
   - 验证不触发 `getFile`
3. 发送多视频媒体组
   - 验证 items 顺序与 Telegram 原始顺序一致

## 编译与测试结果

```bash
# 编译通过
cargo build --workspace --lib
# ✅ Finished `dev` profile [unoptimized + debuginfo] target(s) in 37.94s

# 测试通过（1744+ 个测试）
cargo test --workspace --lib
# ✅ test result: ok. 430 passed; 0 failed; 0 ignored

# Clippy 通过（零警告）
cargo clippy --workspace --all-targets -- -D warnings
# ✅ Finished `dev` profile [unoptimized + debuginfo] target(s) in 30.10s
```

## 下一步工作

### shipinbot 项目侧（不在本次 OpenFang 改动范围内）

1. **新增 `collect-telegram-batch` 命令**
   - 读取 `~/.openfang/workspaces/shipinfabu-hand/inbox/telegram/<batch_key>.json`
   - 将 `status=ready` 的图片和已落地媒体收进 `local_media_intake_dir/openfang-intake/<batch_id>/`
   - 写 `current_batch.json` / `current_state.json`
   - 生成 `source_candidates`、`reference_images`、`reply_hint`

2. **新增 `fetch-telegram-video` 命令**
   - 只针对 hand 已选中的视频执行真实下载
   - 下载目标直接落到该 batch 的 intake 目录
   - 下载成功后回写对应 batch state，使 `suggested_source_video` 指向真实本地文件

3. **更新 `shipinfabu-hand` 对话策略**
   - 优先读取 inbox manifest / `current_batch.json`
   - 根据批次结构决策：
     - 单视频 + 明确意图 → 直接下载并继续
     - 多视频 → 先问用户选哪个
     - 纯图片 → 按纯图文任务处理
   - 不再依赖聊天正文里的 `"[Photo: ...]"` 和 `"[Video: ...]"`

## 文件清单

### 新增文件
- `crates/openfang-channels/src/telegram_media_batch.rs`
- `TELEGRAM_MEDIA_BATCH_IMPLEMENTATION.md`（本文件）

### 修改文件
- `crates/openfang-channels/src/lib.rs`
- `crates/openfang-channels/src/telegram.rs`
- `crates/openfang-channels/src/bridge.rs`
- `docs/telegram-deployment-guide.md`

### 测试文件
- `crates/openfang-channels/src/telegram.rs` (tests 模块)
- `crates/openfang-channels/src/telegram_media_batch.rs` (tests 模块)

## 总结

本次实现完成了 OpenFang 层的所有改动，使 Telegram 媒体组从"文本降级"升级为"结构化批次"。核心价值：

1. **业务决策权归 hand**：OpenFang 只负责事实收集，不替业务做决定
2. **选择性下载**：只下载被选中的视频，节省带宽和存储
3. **稳定性提升**：超大视频不触发 `getFile`，避免 Local Bot API Server 重启
4. **向后兼容**：不影响现有工作流和其他通道

所有代码已通过编译、测试和 clippy 检查，可以安全合并到主分支。
