# OpenFang CLI - 简体中文语言包
# 此文件包含 OpenFang CLI 的所有用户界面字符串。

# ─────────────────────────────────────────────────────────────────────────────
# 应用信息
# ─────────────────────────────────────────────────────────────────────────────
app-name = OpenFang 智能体操作系统
app-tagline = 开源智能体操作系统

# ─────────────────────────────────────────────────────────────────────────────
# 守护进程命令
# ─────────────────────────────────────────────────────────────────────────────
daemon-starting = 正在启动守护进程...
daemon-stopped = OpenFang 守护进程已停止。
daemon-already-running = 守护进程已在运行中。
daemon-not-running = 当前没有运行中的守护进程。
daemon-stopping = 正在停止守护进程...

# ─────────────────────────────────────────────────────────────────────────────
# 内核状态
# ─────────────────────────────────────────────────────────────────────────────
kernel-booted = 内核已启动 ({ $provider }/{ $model })
kernel-boot-failed = 内核启动失败
models-available = { $count } 个模型可用
agents-loaded = 已加载 { $count } 个智能体

# ─────────────────────────────────────────────────────────────────────────────
# 智能体操作
# ─────────────────────────────────────────────────────────────────────────────
agent-spawned = 智能体创建成功！
agent-spawned-id = ID：{ $id }
agent-spawned-name = 名称：{ $name }
agent-spawn-failed = 创建智能体失败：{ $error }
agent-killed = 智能体已终止：{ $id }
agent-not-found = 未找到智能体：{ $id }

# ─────────────────────────────────────────────────────────────────────────────
# 设置与配置
# ─────────────────────────────────────────────────────────────────────────────
setup-welcome = 欢迎使用 OpenFang 设置向导！
setup-cancelled = 设置已取消。
setup-complete = 设置完成！
setup-select-provider = 请选择您的 LLM 提供商：
setup-enter-api-key = 请输入您的 API 密钥：
setup-api-key-saved = API 密钥保存成功。
setup-config-created = 配置文件已创建。

# ─────────────────────────────────────────────────────────────────────────────
# 诊断检查
# ─────────────────────────────────────────────────────────────────────────────
doctor-title = OpenFang 诊断检查
doctor-checking = 正在运行健康检查...
doctor-config-ok = 配置文件已找到
doctor-config-missing = 配置文件缺失
doctor-api-key-ok = API 密钥已配置
doctor-api-key-missing = API 密钥未配置
doctor-provider-ok = 提供商 { $provider } 连接成功
doctor-provider-failed = 提供商 { $provider } 连接失败
doctor-all-ok = 所有检查通过！
doctor-issues-found = 发现 { $count } 个问题

# ─────────────────────────────────────────────────────────────────────────────
# 对话
# ─────────────────────────────────────────────────────────────────────────────
chat-welcome = 欢迎使用 OpenFang 对话！
chat-type-message = 请输入消息（输入 'exit' 退出）：
chat-thinking = 思考中...
chat-error = 错误：{ $error }
chat-goodbye = 再见！

# ─────────────────────────────────────────────────────────────────────────────
# 技能
# ─────────────────────────────────────────────────────────────────────────────
skills-installed = 已安装的技能：
skills-available = 可用技能：
skills-install-success = 技能「{ $name }」安装成功。
skills-install-failed = 安装技能失败：{ $error }
skills-remove-success = 技能「{ $name }」已移除。
skills-not-found = 未找到技能：{ $name }

# ─────────────────────────────────────────────────────────────────────────────
# 频道
# ─────────────────────────────────────────────────────────────────────────────
channels-list = 已配置的频道：
channel-enabled = 频道「{ $name }」已启用。
channel-disabled = 频道「{ $name }」已禁用。
channel-test-success = 频道「{ $name }」测试成功。
channel-test-failed = 频道「{ $name }」测试失败：{ $error }

# ─────────────────────────────────────────────────────────────────────────────
# 错误
# ─────────────────────────────────────────────────────────────────────────────
error-generic = 错误：{ $message }
error-reading-file = 读取文件错误：{ $path }
error-writing-file = 写入文件错误：{ $path }
error-parsing-config = 解析配置错误：{ $error }
error-network = 网络错误：{ $error }
error-api-key-required = 需要 API 密钥。请运行 'openfang setup' 进行配置。
error-daemon-connection = 无法连接到守护进程。请确认是否正在运行？

# ─────────────────────────────────────────────────────────────────────────────
# UI 标签
# ─────────────────────────────────────────────────────────────────────────────
label-provider = 提供商
label-model = 模型
label-api = API
label-dashboard = 控制台
label-status = 状态
label-version = 版本
label-hint = 提示

# ─────────────────────────────────────────────────────────────────────────────
# 状态消息
# ─────────────────────────────────────────────────────────────────────────────
status-connected = 已连接
status-disconnected = 已断开
status-reconnecting = 重新连接中...
status-online = 在线
status-offline = 离线

# ─────────────────────────────────────────────────────────────────────────────
# 提示
# ─────────────────────────────────────────────────────────────────────────────
hint-open-dashboard = 在浏览器中打开控制台，或运行 `openfang chat`
hint-stop-daemon = 按 Ctrl+C 停止守护进程
hint-run-setup = 运行 'openfang setup' 配置您的 API 密钥

# ─────────────────────────────────────────────────────────────────────────────
# TUI 终端界面
# ─────────────────────────────────────────────────────────────────────────────
tui-title = OpenFang 终端界面
tui-chat = 与智能体对话
tui-dashboard = 打开控制台
tui-terminal = 启动终端界面
tui-desktop = 打开桌面应用
tui-settings = 设置
tui-all-commands = 显示所有命令
tui-navigate = 导航
tui-select = 选择
tui-quit = 退出
