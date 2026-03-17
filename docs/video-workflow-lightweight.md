# 智能视频下载工作流 - 轻量集成方案

## 设计目标

**核心原则：智能体能"看到"视频，表面打通即可，不深度集成 shipinbot**

### 用户期望
1. 智能体收到大视频时，主动询问："检测到视频 (565MB)，是否下载？"
2. 用户确认后开始下载，实时汇报进度
3. 下载完成后，智能体能"看到"视频信息（文件路径、大小、时长等）
4. 智能体可以选择性地调用 shipinbot 工作流处理视频

### 不需要做的
- ❌ 不需要自动触发 shipinbot 流水线
- ❌ 不需要深度集成视频处理逻辑
- ❌ 不需要实现完整的视频分析工具

---

## 实现方案

### 阶段 1：智能询问下载（已有基础）

**当前状态：**
- ✅ 已实现大文件检测（>20MB 警告）
- ✅ 已实现下载进度回调
- ⚠️ 缺少：主动询问用户是否下载

**需要添加：**

在 `telegram.rs` 中，当检测到大视频时：
1. 不立即下载
2. 发送消息："检测到视频 (565MB, 8分钟)，是否下载？回复 '是' 开始下载"
3. 等待用户回复
4. 用户确认后开始下载

**实现位置：**
- `crates/openfang-channels/src/telegram.rs` - 视频检测逻辑
- 修改 `merge_media_group_updates()` 和视频处理部分

---

### 阶段 2：下载完成后的"可见性"

**目标：让智能体能"看到"视频**

下载完成后，智能体收到的消息应该包含：

```
✅ 视频下载完成

文件信息：
- 路径：/tmp/openfang-telegram-downloads/AgACAgQAAxkBAAIBY2...dat
- 大小：565 MB
- 时长：8分15秒
- 分辨率：1920x1080
- 编码：H.264

可用操作：
1. 查看视频信息（video_info）
2. 提取截图（video_screenshot）
3. 发送到 shipinbot 处理（需要手动确认）
```

**实现方式：**
- 下载完成后调用 `ffprobe` 获取视频元信息
- 将信息格式化为消息发送给用户
- 智能体可以看到这些信息并决定下一步

---

### 阶段 3：基础视频工具（轻量）

**只实现最基础的工具，让智能体能"看"视频：**

#### 1. `video_info` - 获取视频元信息
```rust
// 输入：文件路径
// 输出：JSON 格式的视频信息
{
  "duration": 495.2,
  "width": 1920,
  "height": 1080,
  "codec": "h264",
  "bitrate": 9500000,
  "fps": 30,
  "size_bytes": 592445440
}
```

#### 2. `video_screenshot` - 提取单帧截图
```rust
// 输入：文件路径、时间点（秒）
// 输出：截图文件路径
// 用途：让智能体能"看到"视频内容
```

#### 3. `video_send_to_shipinbot` - 可选集成
```rust
// 输入：文件路径、处理选项
// 输出：shipinbot 任务 ID
// 说明：这是一个"桥接"工具，不是必需的
// 智能体可以选择性调用，也可以让用户手动处理
```

---

## 实现优先级

### P0（必须实现）
1. ✅ Local Bot API Server 集成（已完成）
2. 🚧 智能询问下载（简单修改 telegram.rs）
3. 🚧 下载完成后显示视频信息（调用 ffprobe）

### P1（可选实现）
4. ⏸️ `video_info` 工具（如果智能体需要主动查询）
5. ⏸️ `video_screenshot` 工具（如果需要预览）

### P2（暂不实现）
6. ❌ 深度集成 shipinbot（用户说不需要）
7. ❌ 自动触发处理流水线（用户说不需要）
8. ❌ 复杂的视频分析工具（用户说不需要）

---

## 工作流示例

### 场景：用户在 Telegram 发送 565MB 视频

**步骤 1：智能体检测到视频**
```
🎬 检测到视频
- 大小：565 MB
- 时长：约 8 分钟

是否下载到本地？回复 '是' 开始下载
```

**步骤 2：用户确认**
```
用户：是
```

**步骤 3：开始下载**
```
⬇️ 开始下载...
⬇️ 下载中... 15% (87 MB / 565 MB)
⬇️ 下载中... 45% (254 MB / 565 MB)
⬇️ 下载中... 78% (441 MB / 565 MB)
✅ 下载完成！
```

**步骤 4：显示视频信息**
```
✅ 视频已保存

文件信息：
- 路径：/tmp/openfang-telegram-downloads/video_20260317_123456.mp4
- 大小：565 MB
- 时长：8分15秒
- 分辨率：1920x1080
- 编码：H.264

你可以：
- 让我提取截图查看内容
- 发送到 shipinbot 进行处理
- 或者告诉我其他需求
```

**步骤 5：用户决定下一步**
```
用户：提取第 30 秒的截图
智能体：[调用 video_screenshot 工具]
智能体：[发送截图]
```

---

## 与 shipinbot 的集成点

**轻量集成，不深度耦合：**

1. **文件路径传递**
   - 下载的视频保存在 `/tmp/openfang-telegram-downloads/`
   - shipinbot 的 `local_media_intake_dir` 可以配置为同一目录
   - 或者智能体可以调用工具将文件复制到 shipinbot 的收件目录

2. **可选的桥接工具**
   ```rust
   // 伪代码
   fn video_send_to_shipinbot(video_path: &str) -> Result<String> {
       // 1. 复制文件到 shipinbot 收件目录
       // 2. 调用 shipinbot bridge 脚本
       // 3. 返回任务 ID
       // 4. 智能体可以轮询任务状态
   }
   ```

3. **手动触发**
   - 用户可以直接在 Telegram 说："把这个视频发送到 shipinbot 处理"
   - 智能体调用桥接工具
   - 或者用户手动在 shipinbot 中处理

---

## 技术实现细节

### 1. 视频信息获取（使用 ffprobe）

```rust
async fn get_video_info(file_path: &Path) -> Result<VideoInfo> {
    let output = tokio::process::Command::new("ffprobe")
        .args(&[
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
            file_path.to_str().unwrap(),
        ])
        .output()
        .await?;

    let info: serde_json::Value = serde_json::from_slice(&output.stdout)?;

    // 解析 JSON 提取关键信息
    Ok(VideoInfo {
        duration: info["format"]["duration"].as_str().unwrap().parse()?,
        size_bytes: info["format"]["size"].as_str().unwrap().parse()?,
        // ... 其他字段
    })
}
```

### 2. 下载完成后的消息格式

```rust
// 在 download_file() 成功后
let video_info = get_video_info(&dest_path).await?;

let message = format!(
    "✅ 视频下载完成\n\n\
     文件信息：\n\
     - 路径：{}\n\
     - 大小：{} MB\n\
     - 时长：{}\n\
     - 分辨率：{}x{}\n\
     - 编码：{}",
    dest_path.display(),
    video_info.size_bytes / 1024 / 1024,
    format_duration(video_info.duration),
    video_info.width,
    video_info.height,
    video_info.codec,
);

// 发送消息给用户
```

---

## 总结

**这个方案的核心是：**
1. ✅ 智能体能"看到"视频（文件路径、元信息）
2. ✅ 表面打通（下载、显示信息、可选工具）
3. ✅ 不深度集成（不自动触发 shipinbot）
4. ✅ 灵活性（用户可以选择是否使用 shipinbot）

**实现工作量：**
- 核心功能（P0）：约 2-3 小时
- 可选工具（P1）：约 1-2 小时
- 总计：约 3-5 小时

**下一步：**
你想让我先实现 P0 的核心功能吗？还是先看看这个方案是否符合你的需求？
