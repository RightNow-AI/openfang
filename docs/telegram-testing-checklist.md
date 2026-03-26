# Telegram 大文件下载功能 - 测试检查清单

## 部署前检查

### 1. 二进制文件
- [ ] `~/.openfang/bin/telegram-bot-api` 存在
- [ ] 文件有执行权限：`ls -l ~/.openfang/bin/telegram-bot-api`
- [ ] 版本检查：`~/.openfang/bin/telegram-bot-api --version`

### 2. 环境变量
- [ ] `TELEGRAM_BOT_TOKEN` 已设置：`echo $TELEGRAM_BOT_TOKEN`
- [ ] `TELEGRAM_API_HASH` 已设置：`echo $TELEGRAM_API_HASH`
- [ ] 环境变量已添加到 `~/.zshrc` 或 `~/.bashrc`

### 3. 配置文件
- [ ] `~/.openfang/config.toml` 存在
- [ ] `[channels.telegram]` 部分已配置
- [ ] `use_local_api = true`
- [ ] `auto_start_local_api = true`
- [ ] `telegram_api_id` 已填写（不是 "YOUR_API_ID"）
- [ ] `telegram_api_hash_env = "TELEGRAM_API_HASH"`
- [ ] `local_api_port = 8081`
- [ ] `api_url = "http://localhost:8081"`

### 4. 目录权限
- [ ] `/tmp/openfang-telegram-downloads` 目录存在
- [ ] 目录有写权限：`ls -ld /tmp/openfang-telegram-downloads`

### 5. 端口可用性
- [ ] 端口 8081 未被占用：`lsof -i :8081`
- [ ] 端口 4200 未被占用：`lsof -i :4200`

## 启动检查

### 1. 启动 OpenFang
```bash
cd /Users/xiaomo/Desktop/openfang-upstream-fork
TELEGRAM_BOT_TOKEN=xxx TELEGRAM_API_HASH=xxx target/release/openfang start
```

### 2. 日志检查
- [ ] 看到：`INFO Telegram Local Bot API Server started with PID xxxxx`
- [ ] 看到：`INFO Telegram Local Bot API mode enabled (supports files >20MB)`
- [ ] 看到：`INFO Starting Telegram channel adapter...`
- [ ] 没有错误：`grep -i error ~/.openfang/logs/openfang.log`

### 3. 进程检查
- [ ] OpenFang 进程运行中：`ps aux | grep openfang`
- [ ] telegram-bot-api 进程运行中：`ps aux | grep telegram-bot-api`

### 4. 端口检查
- [ ] 端口 8081 已监听：`lsof -i :8081`
- [ ] 端口 4200 已监听：`lsof -i :4200`

## 功能测试

### 测试 1：小文件下载（<20MB）
- [ ] 在 Telegram 发送一个 <20MB 的图片或视频
- [ ] 智能体收到消息
- [ ] 文件正常下载
- [ ] 智能体能看到文件内容

**预期结果：** 正常下载，无警告

### 测试 2：中等文件下载（20MB-100MB）
- [ ] 在 Telegram 发送一个 20-100MB 的视频
- [ ] 看到下载进度消息
- [ ] 文件下载到 `/tmp/openfang-telegram-downloads/`
- [ ] 文件大小正确：`ls -lh /tmp/openfang-telegram-downloads/`
- [ ] 智能体收到下载完成通知

**预期结果：**
```
⬇️ 下载中... 15%
⬇️ 下载中... 45%
⬇️ 下载中... 78%
✅ 下载完成
```

### 测试 3：大文件下载（>100MB）
- [ ] 在 Telegram 发送一个 >100MB 的视频（如 565MB）
- [ ] 下载进度实时更新
- [ ] 下载完成后文件存在
- [ ] 文件大小与原始文件一致
- [ ] 智能体能访问文件路径

**预期结果：** 成功下载，无错误

### 测试 4：多文件并发下载
- [ ] 连续发送 3 个大文件
- [ ] 所有文件都能下载
- [ ] 没有进程崩溃
- [ ] 没有文件损坏

**预期结果：** 所有文件正常下载

### 测试 5：进程崩溃恢复
- [ ] 手动杀死 telegram-bot-api 进程：`pkill telegram-bot-api`
- [ ] 等待 5-10 秒
- [ ] 检查进程是否自动重启：`ps aux | grep telegram-bot-api`
- [ ] 发送测试文件验证功能恢复

**预期结果：** 进程自动重启，功能恢复

### 测试 6：优雅停止
- [ ] 停止 OpenFang：`pkill openfang`
- [ ] 检查 telegram-bot-api 是否也停止：`ps aux | grep telegram-bot-api`
- [ ] 检查端口是否释放：`lsof -i :8081`

**预期结果：** 两个进程都停止，端口释放

## 集成测试（可选）

### 与 shipinbot 集成
- [ ] shipinbot media-pipeline-service 运行中
- [ ] 下载的视频可以被 shipinbot 访问
- [ ] 智能体能调用 bridge 脚本
- [ ] 视频处理任务成功提交

**测试命令：**
```bash
# 启动 shipinbot
cd /Users/xiaomo/Desktop/openfang-upstream-fork/projects/shipinbot
./scripts/start_media_web.sh

# 验证
curl http://127.0.0.1:8000/healthz

# 测试 bridge
.venv/bin/python scripts/openfang_clean_publish_bridge.py \
  --source-video /tmp/openfang-telegram-downloads/xxx.dat \
  --action validate
```

## 性能测试

### 下载速度
- [ ] 记录 100MB 文件下载时间
- [ ] 记录 500MB 文件下载时间
- [ ] 计算平均速度（MB/s）

**参考值：**
- 100MB 文件：约 10-30 秒（取决于网络）
- 500MB 文件：约 50-150 秒

### 内存使用
- [ ] 启动时内存：`ps aux | grep openfang | awk '{print $6}'`
- [ ] 下载大文件时内存峰值
- [ ] 下载完成后内存是否释放

**预期：** 内存使用稳定，无泄漏

### CPU 使用
- [ ] 空闲时 CPU 使用率 <5%
- [ ] 下载时 CPU 使用率 <30%

## 故障场景测试

### 场景 1：网络中断
- [ ] 下载过程中断开网络
- [ ] 观察错误处理
- [ ] 恢复网络后重试

**预期：** 优雅失败，有错误提示

### 场景 2：磁盘空间不足
- [ ] 模拟磁盘满（如果可能）
- [ ] 观察错误处理

**预期：** 错误提示，不崩溃

### 场景 3：无效 API credentials
- [ ] 使用错误的 `telegram_api_id`
- [ ] 观察启动行为

**预期：** telegram-bot-api 启动失败，有明确错误提示

### 场景 4：端口冲突
- [ ] 手动占用 8081 端口：`nc -l 8081`
- [ ] 启动 OpenFang
- [ ] 观察错误处理

**预期：** 启动失败，提示端口被占用

## 日志检查

### 正常日志模式
```
INFO Telegram Local Bot API Server started with PID 12345
INFO Telegram Local Bot API mode enabled (supports files >20MB)
INFO Starting Telegram channel adapter...
INFO Downloading file xxx (565 MB)...
INFO Download progress: 15%
INFO Download progress: 45%
INFO Download complete: /tmp/openfang-telegram-downloads/xxx.dat
```

### 错误日志模式
```
ERROR Failed to spawn telegram-bot-api: ...
ERROR telegram-bot-api exited with status: 1
ERROR Address already in use (port 8081)
WARN File xxx (565 MB) exceeds official Bot API 20MB limit...
```

## 清理检查

### 测试后清理
- [ ] 停止所有进程
- [ ] 清理下载目录：`rm -rf /tmp/openfang-telegram-downloads/*`
- [ ] 检查没有僵尸进程：`ps aux | grep telegram`
- [ ] 检查端口已释放：`lsof -i :8081`

## 文档检查

### 文档完整性
- [ ] `docs/telegram-deployment-guide.md` 存在
- [ ] `docs/telegram-large-files.md` 存在
- [ ] `docs/telegram-config-example.toml` 存在
- [ ] `scripts/setup-telegram-local-api.sh` 存在且可执行
- [ ] README.md 包含 Telegram 部分

### 文档准确性
- [ ] 配置示例可以直接使用
- [ ] 命令可以直接复制执行
- [ ] 故障排查步骤有效
- [ ] 链接都能访问

## 测试结果记录

### 测试环境
- 操作系统：macOS / Linux / Windows
- OpenFang 版本：v0.4.4
- telegram-bot-api 版本：9.5
- 测试日期：____________________

### 测试结果
| 测试项 | 状态 | 备注 |
|--------|------|------|
| 小文件下载 | ✅ / ❌ | |
| 中等文件下载 | ✅ / ❌ | |
| 大文件下载 | ✅ / ❌ | |
| 并发下载 | ✅ / ❌ | |
| 崩溃恢复 | ✅ / ❌ | |
| 优雅停止 | ✅ / ❌ | |
| shipinbot 集成 | ✅ / ❌ | |

### 发现的问题
1. _______________________________________________
2. _______________________________________________
3. _______________________________________________

### 改进建议
1. _______________________________________________
2. _______________________________________________
3. _______________________________________________

## 签字确认

测试人员：____________________
日期：____________________
状态：通过 / 不通过

---

**注意事项：**
1. 所有测试应在干净的环境中进行
2. 记录所有错误日志和截图
3. 测试完成后清理测试数据
4. 发现问题及时记录到 GitHub Issues
