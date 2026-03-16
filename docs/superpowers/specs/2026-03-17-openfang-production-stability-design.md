# OpenFang 生产稳定性修复设计文档

**日期**: 2026-03-17
**作者**: Claude (Opus 4.6)
**状态**: 待审查

## 1. 问题概述

### 1.1 当前状态
- 项目最新版本 (commit a25be05) 已编译到本机
- 编译、测试、clippy 检查均通过
- **关键问题**: API 服务未运行，守护进程未正确启动

### 1.2 根本原因
1. **代码 bug**: `KernelConfig::default()` 中 `api_listen` 默认值为 `"127.0.0.1:50051"`，但文档、CLI 帮助、代码注释都标注应为 `4200` 端口
2. **配置不一致**: 用户配置文件使用 50051，与预期标准不符
3. **守护进程失效**: `openfang-daemon` 脚本在运行，但实际的 `openfang start` 进程不存在，`daemon.json` 文件缺失
4. **僵尸进程**: 15 个 `openfang` 相关进程残留（多为 `--version` 和 `help` 命令）
5. **缺乏生产保障**: 无系统级服务管理、无健康检查、无自动重启机制

### 1.3 影响范围
- API 服务完全不可用（端口 4200 和 50051 均未监听）
- 无法通过 HTTP API 与 kernel 交互
- Dashboard 无法访问
- 生产环境缺乏可靠性保障

## 2. 设计目标

### 2.1 核心目标
- **功能完整**: API 服务正常运行，所有端点可访问
- **配置标准化**: 统一使用 4200 端口，消除文档与代码不一致
- **生产可靠**: 添加系统级服务管理、健康检查、自动重启
- **可验证性**: 完整的实时集成测试验证

### 2.2 非目标
- 不改变现有功能逻辑
- 不引入新的外部依赖
- 不修改用户数据或 agent 配置

## 3. 技术方案

### 3.1 代码修复

#### 3.1.1 修复默认端口
**文件**: `crates/openfang-types/src/config.rs`

**修改 1 - Default 实现** (约 line 1270):
```rust
impl Default for KernelConfig {
    fn default() -> Self {
        let home_dir = openfang_home_dir();
        Self {
            // ... 其他字段 ...
            api_listen: "127.0.0.1:4200".to_string(),  // 从 50051 改为 4200
            // ... 其他字段 ...
        }
    }
}
```

**修改 2 - 测试断言** (line 3415):
```rust
#[test]
fn test_default_config() {
    let config = KernelConfig::default();
    assert_eq!(config.log_level, "info");
    assert_eq!(config.api_listen, "127.0.0.1:4200");  // 从 50051 改为 4200
    assert!(!config.network_enabled);
}
```

**理由**:
- 代码注释明确标注 `/// API listen address (e.g., "0.0.0.0:4200")`
- CLI 帮助信息显示 `Dashboard: http://127.0.0.1:4200/`
- 文档 `docs/cli-reference.md` 和 `CLAUDE.md` 都使用 4200
- 50051 是 gRPC 常用端口，但 OpenFang 使用 HTTP/REST API
- 必须同步更新测试断言，否则测试失败

### 3.2 配置标准化

#### 3.2.1 更新用户配置
**文件**: `~/.openfang/config.toml`

**修改**:
```toml
api_listen = "127.0.0.1:4200"  # 从 50051 改为 4200
```

**备份策略**:
- 自动创建 `config.toml.bak-YYYYMMDD-HHMMSS` 备份
- 保留最近 5 个备份文件

### 3.3 进程清理

#### 3.3.1 清理策略
1. 识别所有 `openfang` 相关进程
2. 优雅关闭（SIGTERM，等待 10 秒）
3. 强制终止未响应进程（SIGKILL）
4. 清理锁文件：`~/.openfang/.external-hands-reconcile.lock`
5. 等待端口释放（最多 5 秒）

#### 3.3.2 保护机制
- 不杀掉当前 Claude Code 会话进程
- 不杀掉 Python reconcile 脚本（除非必要）

### 3.4 macOS launchd 服务

#### 3.4.1 服务配置
**文件**: `~/Library/LaunchAgents/ai.openfang.daemon.plist` (已存在，需更新)

**现状**:
- 服务已存在，label 为 `ai.openfang.daemon`
- 当前通过 `openfang-daemon-runner.sh` → `openfang-daemon` 脚本启动
- `openfang-daemon` 脚本已处理环境变量加载

**策略**: 保持现有服务 label 和结构，仅更新配置以优化可靠性

**更新后配置**:
```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>ai.openfang.daemon</string>

    <key>ProgramArguments</key>
    <array>
        <string>/Users/xiaomo/.openfang/bin/openfang-daemon-runner.sh</string>
    </array>

    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin</string>
    </dict>

    <key>RunAtLoad</key>
    <true/>

    <key>WorkingDirectory</key>
    <string>/Users/xiaomo</string>

    <key>EnvironmentVariables</key>
    <dict>
        <key>HOME</key>
        <string>/Users/xiaomo</string>
        <key>PATH</key>
        <string>/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>
    </dict>

    <key>RunAtLoad</key>
    <true/>

    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
        <key>Crashed</key>
        <true/>
    </dict>

    <key>ThrottleInterval</key>
    <integer>60</integer>

    <key>ProcessType</key>
    <string>Background</string>

    <key>StandardOutPath</key>
    <string>/Users/xiaomo/Library/Logs/openfang-daemon.out.log</string>

    <key>StandardErrorPath</key>
    <string>/Users/xiaomo/Library/Logs/openfang-daemon.err.log</string>
</dict>
</plist>
```

**功能特性**:
- **开机自启**: `RunAtLoad=true`
- **崩溃重启**: `KeepAlive.Crashed=true`
- **节流保护**: 60 秒内最多重启 1 次
- **日志记录**: 标准输出/错误分离记录到 `~/Library/Logs/`
- **环境变量**: 加载完整 PATH

**关键改进**:
- 添加 `ThrottleInterval=60` 防止快速重启循环
- 添加 `ProcessType=Background` 标记为后台服务
- 保持现有的 `openfang-daemon-runner.sh` → `openfang-daemon` 调用链（已处理环境变量）

#### 3.4.2 环境变量加载
**现状**: `openfang-daemon` 脚本已通过 `dotenv::load_dotenv()` 加载 `~/.openfang/.env`（符号链接到 `secrets.env`）

**验证点**:
- 确认 `.env` 符号链接存在且有效
- 确认 `secrets.env` 包含所有必需的 API keys
- 添加错误处理：如果 `.env` 不存在或不可读，脚本应明确失败并记录错误

**无需额外包装脚本** - 现有的 `openfang-daemon-runner.sh` → `openfang-daemon` 链已处理环境变量加载

### 3.5 健康检查脚本

#### 3.5.1 脚本实现
**文件**: `~/.openfang/bin/health-check.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail

API_URL="http://127.0.0.1:4200/api/health"
TIMEOUT=5

# 检查 1: API 端点响应
if ! curl -sf --max-time "${TIMEOUT}" "${API_URL}" | grep -q '"status":"ok"'; then
    echo "FAIL: API health endpoint not responding" >&2
    exit 1
fi

# 检查 2: 进程存活（匹配实际进程名）
# 注意：launchd 启动的进程名可能是 "openfang" 而非 "openfang start"
if ! pgrep -f "openfang" >/dev/null; then
    echo "FAIL: openfang process not running" >&2
    exit 2
fi

# 检查 3: 端口监听
if ! lsof -i :4200 -sTCP:LISTEN >/dev/null 2>&1; then
    echo "FAIL: Port 4200 not listening" >&2
    exit 3
fi

# 检查 4: 端口可用性（预检）
check_port_available() {
    if lsof -i :4200 >/dev/null 2>&1; then
        echo "WARN: Port 4200 already in use" >&2
        lsof -i :4200 >&2
        return 1
    fi
    return 0
}

echo "OK: All health checks passed"
exit 0
```

**退出码**:
- `0`: 所有检查通过
- `1`: API 端点失败
- `2`: 进程不存在
- `3`: 端口未监听

#### 3.5.2 定期健康检查（可选）
可以通过 cron 或 launchd StartInterval 定期运行健康检查，失败时发送告警。

### 3.6 验证流程

#### 3.6.1 编译验证
```bash
cargo build --workspace --lib
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

#### 3.6.2 实时集成测试
按照 `CLAUDE.md` 中的 "MANDATORY: Live Integration Testing" 流程：

1. **停止旧进程**
```bash
pkill -f "openfang" || true
sleep 3
```

2. **编译 release 版本**
```bash
cargo build --release -p openfang-cli
```

3. **启动守护进程**
```bash
GROQ_API_KEY=<key> target/release/openfang start &
sleep 6
```

4. **测试端点**
```bash
# 健康检查
curl -s http://127.0.0.1:4200/api/health

# 列出 agents
curl -s http://127.0.0.1:4200/api/agents

# 获取第一个 agent ID
AGENT_ID=$(curl -s http://127.0.0.1:4200/api/agents | python3 -c "import sys,json; print(json.load(sys.stdin)[0]['id'])")

# 发送真实 LLM 消息
curl -s -X POST "http://127.0.0.1:4200/api/agents/${AGENT_ID}/message" \
  -H "Content-Type: application/json" \
  -d '{"message": "Say hello in 5 words."}'

# 验证计费追踪
curl -s http://127.0.0.1:4200/api/budget
curl -s http://127.0.0.1:4200/api/budget/agents
```

5. **验证 launchd 服务**
```bash
launchctl list | grep openfang
launchctl print gui/$(id -u)/ai.openfang.daemon
```

#### 3.6.3 成功标准
- ✅ 编译无错误、无警告
- ✅ 所有单元测试通过
- ✅ API 在 4200 端口响应 `{"status":"ok"}`
- ✅ 能列出 agents
- ✅ 真实 LLM 调用返回响应
- ✅ 计费数据更新（cost > 0）
- ✅ launchd 服务状态为 running
- ✅ 健康检查脚本返回 0

## 4. 风险控制

### 4.1 风险点
1. **端口冲突**: 4200 端口可能被其他服务占用
2. **配置迁移**: 用户可能有脚本依赖 50051 端口
3. **进程清理**: 强制杀进程可能丢失未保存数据
4. **环境变量**: launchd 环境变量加载可能失败

### 4.2 缓解措施
1. **端口冲突**: 启动前检查端口占用，提示用户处理
2. **配置迁移**: 自动备份配置，保留回滚路径
3. **进程清理**: 优雅关闭（SIGTERM）+ 超时强制（SIGKILL）
4. **环境变量**: 使用包装脚本显式加载 secrets.env

### 4.3 回滚方案
如果验证失败：
1. 恢复配置文件备份: `cp ~/.openfang/config.toml.bak-* ~/.openfang/config.toml`
2. 卸载 launchd 服务: `launchctl unload ~/Library/LaunchAgents/com.openfang.daemon.plist`
3. 使用原守护进程脚本: `~/.openfang/bin/openfang-daemon`
4. 回退代码修改: `git checkout crates/openfang-types/src/config.rs`

## 5. 实施计划

### 5.1 实施顺序
1. **准备阶段** (5 分钟)
   - 备份配置文件
   - 检查端口占用
   - 记录当前进程状态

2. **代码修复** (10 分钟)
   - 修改 `config.rs` 默认端口
   - 编译验证
   - 运行测试套件

3. **进程清理** (5 分钟)
   - 停止所有 openfang 进程
   - 清理锁文件
   - 等待端口释放

4. **配置更新** (5 分钟)
   - 更新 `config.toml`
   - 创建包装脚本
   - 创建健康检查脚本

5. **服务部署** (10 分钟)
   - 创建 launchd plist
   - 加载服务
   - 验证服务状态

6. **启动验证** (10 分钟)
   - 启动守护进程
   - 等待就绪
   - 运行实时集成测试

7. **监控确认** (5 分钟)
   - 验证日志记录
   - 测试自动重启
   - 运行健康检查

**总计**: 约 50 分钟

### 5.2 验收标准
- [ ] 代码编译通过，无警告
- [ ] 所有单元测试通过
- [ ] API 在 4200 端口正常响应
- [ ] 真实 LLM 调用成功
- [ ] 计费追踪正常工作
- [ ] launchd 服务运行正常
- [ ] 健康检查脚本返回成功
- [ ] 日志文件正常记录
- [ ] 崩溃后自动重启（手动测试）

## 6. 后续改进

### 6.1 短期改进（1-2 周）
- 添加 Prometheus metrics 端点
- 实现日志轮转（避免日志文件无限增长）
- 添加配置热重载功能

### 6.2 中期改进（1-2 月）
- 实现分布式追踪（OpenTelemetry）
- 添加告警通知（Telegram/Email）
- 实现优雅关闭（保存状态后退出）

### 6.3 长期改进（3-6 月）
- 支持多实例部署（负载均衡）
- 实现配置中心（统一管理多节点配置）
- 添加性能监控和自动扩缩容

## 7. 参考文档

- `CLAUDE.md`: 项目指令和集成测试流程
- `docs/production-checklist.md`: 生产发布检查清单
- `docs/cli-reference.md`: CLI 命令参考
- `crates/openfang-api/src/server.rs`: API 服务器实现
- `crates/openfang-types/src/config.rs`: 配置类型定义

---

**文档版本**: 1.0
**最后更新**: 2026-03-17
