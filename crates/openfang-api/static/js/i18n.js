'use strict';

(function() {
  var STORAGE_KEY = 'openfang-lang';
  var DEFAULT_LOCALE = 'en';
  var SUPPORTED = ['en', 'zh-CN'];

  var zhMap = {
    'OpenFang Dashboard': 'OpenFang 控制台',

    'API Key Required': '需要 API 密钥',
    'This instance requires an API key. Enter the key from your config.toml.': '该实例需要 API 密钥。请输入你在 config.toml 中配置的密钥。',
    'Enter API key...': '输入 API 密钥...',
    'Unlock Dashboard': '解锁控制台',

    'Light': '浅色',
    'System': '系统',
    'Follow System': '跟随系统',
    'Dark': '深色',

    'Language': '语言',

    'Connecting...': '正在连接...',
    'Reconnecting...': '正在重连...',
    'disconnected': '已断开连接',
    'Healthy': '健康',
    'Unreachable': '不可达',

    'Chat': '聊天',
    'Monitor': '监控',
    'Overview': '概览',
    'Analytics': '分析',
    'Logs': '日志',
    'Agents': '代理',
    'Sessions': '会话',
    'Approvals': '审批',
    'Automation': '自动化',
    'Workflows': '工作流',
    'Scheduler': '计划任务',
    'Extensions': '扩展',
    'Channels': '渠道',
    'Skills': '技能',
    'Hands': '能力包',
    'Settings': '设置',
    'Ctrl+K agents | Ctrl+N new': 'Ctrl+K 代理 | Ctrl+N 新建',

    'Your Agents': '你的代理',
    'Start Chatting': '开始聊天',
    'Or Start a New Agent': '或创建一个新代理',
    'General': '通用',
    'Development': '开发',
    'Research': '研究',
    'Writing': '写作',
    'Business': '业务',
    'General Assistant': '通用助手',
    'A versatile conversational agent that can help with everyday tasks, answer questions, and provide recommendations.': '一个多用途对话代理，可帮助处理日常任务、回答问题并提供建议。',
    'Code Helper': '代码助手',
    'A programming-focused agent that writes, reviews, and debugs code across multiple languages.': '一个专注编程的代理，可编写、审查并调试多种语言的代码。',
    'Researcher': '研究员',
    'An analytical agent that breaks down complex topics, synthesizes information, and provides cited summaries.': '一个分析型代理，可拆解复杂主题、整合信息并提供带引用的摘要。',
    'Writer': '写作者',
    'A creative writing agent that helps with drafting, editing, and improving written content of all kinds.': '一个创意写作代理，可帮助起草、编辑并提升各类书面内容。',
    'Data Analyst': '数据分析师',
    'A data-focused agent that helps analyze datasets, create queries, and interpret statistical results.': '一个数据导向型代理，可帮助分析数据集、编写查询并解读统计结果。',
    'DevOps Engineer': 'DevOps 工程师',
    'A systems-focused agent for CI/CD, infrastructure, Docker, and deployment troubleshooting.': '一个系统导向型代理，擅长 CI/CD、基础设施、Docker 与部署排障。',
    'Customer Support': '客户支持',
    'A professional, empathetic agent for handling customer inquiries and resolving issues.': '一个专业且富有同理心的代理，用于处理客户咨询并解决问题。',
    'Tutor': '导师',
    'A patient educational agent that explains concepts step-by-step and adapts to the learner\'s level.': '一个耐心的教学代理，可循序渐进讲解概念，并根据学习者水平调整表达。',
    'API Designer': 'API 设计师',
    'An agent specialized in RESTful API design, OpenAPI specs, and integration architecture.': '一个专注于 RESTful API 设计、OpenAPI 规范与集成架构的代理。',
    'Meeting Notes': '会议纪要',
    'Summarizes meeting transcripts into structured notes with action items and key decisions.': '将会议记录总结为结构化纪要，包含行动项和关键决策。',
    'Info': '信息',
    'Files': '文件',
    'Config': '配置',
    'State': '状态',
    'Mode': '模式',
    'Observe': '观察',
    'Assist': '协助',
    'Full': '完全',
    'Profile': '配置档',
    'Provider': '提供商',
    'Model': '模型',
    'Created': '创建时间',
    'Chat': '聊天',
    'Clone': '克隆',
    'Stop': '停止',
    'Loading files...': '正在加载文件...',
    'Not created': '未创建',
    'No workspace files found': '未找到工作区文件',
    'Back': '返回',
    'Save': '保存',
    'Saving...': '保存中...',
    'Name': '名称',
    'System Prompt': '系统提示词',
    'Emoji': '表情',
    'Color': '颜色',
    'Archetype': '原型',
    'None': '无',
    'Vibe': '风格',

    'Scheduler': '计划任务',
    '+ New Job': '+ 新建任务',
    'Scheduled Jobs': '定时任务',
    'Event Triggers': '事件触发器',
    'Run History': '运行历史',
    'Loading scheduled jobs...': '正在加载定时任务...',
    'Retry': '重试',
    'Create cron-based scheduled jobs that send messages to agents on a recurring schedule.': '创建基于 Cron 的定时任务，按计划周期性向代理发送消息。',
    'Use cron expressions like': '使用 Cron 表达式，例如',
    '(every 5 min) or': '（每 5 分钟）或',
    '(weekdays at 9am). You can also run any job manually with the "Run Now" button.': '（工作日早上 9 点）。你也可以通过 “立即运行” 按钮手动执行任务。',
    'Name': '名称',
    'Schedule': '计划',
    'Agent': '代理',
    'Status': '状态',
    'Last Run': '上次运行',
    'Next Run': '下次运行',
    'Actions': '操作',
    'Active': '启用',
    'Paused': '已暂停',
    'Run': '运行',
    'Pause': '暂停',
    'Enable': '启用',
    'Del': '删',
    'No scheduled jobs': '暂无定时任务',
    'Create a cron job to run agents on a recurring schedule. Jobs are stored persistently and survive restarts.': '创建一个 Cron 任务，让代理按周期运行。任务会持久化保存，并在重启后继续存在。',
    '+ Create Scheduled Job': '+ 创建定时任务',
    'Create Scheduled Job': '创建定时任务',
    'Job Name': '任务名称',
    'Cron Expression': 'Cron 表达式',
    'Format:': '格式：',
    'Quick Presets': '快捷预设',
    'Target Agent': '目标代理',
    'Any available agent': '任意可用代理',
    'No agents running.': '当前没有正在运行的代理。',
    'Spawn one first.': '先创建一个。',
    'Message to Send': '发送内容',
    'The message sent to the agent each time this job runs.': '每次任务运行时发送给代理的消息。',
    'Enabled (will start running immediately)': '已启用（会立即开始运行）',
    'Disabled (create paused)': '已禁用（创建后暂停）',
    'Create Schedule': '创建计划',
    'Creating...': '创建中...',
    'Loading triggers...': '正在加载触发器...',
    'Event triggers fire agents in response to system events (agent lifecycle, memory updates, custom events).': '事件触发器会在系统事件发生时触发代理执行（如代理生命周期、记忆更新、自定义事件）。',
    'Create and manage triggers on the': '可在',
    'page.': '页面创建和管理触发器。',
    'This view shows all active triggers for monitoring.': '此视图展示所有活动中的触发器，便于监控。',
    'Pattern': '模式',
    'Prompt': '提示词',
    'Fires': '触发次数',
    'Enabled': '启用',
    'Delete': '删除',
    'No event triggers': '暂无事件触发器',
    'History shows recent job runs and trigger fires.': '历史记录展示最近的任务运行和触发器触发情况。',
    'No runs yet': '暂无运行记录',

    'Hands — Curated Autonomous Capability Packages': '能力包：精选自治能力套件',
    'Hands are pre-configured AI agents that autonomously handle specific tasks. Each hand includes a tuned system prompt, required tools, and a dashboard for tracking work.': '能力包是为特定任务预配置的 AI 代理。每个能力包都包含调优后的系统提示词、所需工具以及用于跟踪工作的仪表盘。',
    'Loading hands...': '正在加载能力包...',
    'Loading active hands...': '正在加载已激活能力包...',
    'No hands available': '暂无可用能力包',
    'Hands are curated AI capability packages. They will appear once the kernel loads bundled hands.': '能力包是精选的 AI 能力套件。内核加载内置能力包后，它们会显示在这里。',
    'No active hands': '暂无已激活能力包',
    'Available': '可用',
    'Ready': '就绪',
    'Setup needed': '需要设置',
    'REQUIREMENTS': '依赖要求',
    'Details': '详情',
    'Activate': '激活',
    'Activated:': '已激活：',
    'Activate a hand from the Available tab to get started. Each hand spawns a dedicated agent.': '从“可用”标签页激活一个能力包即可开始。每个能力包都会生成一个专属代理。',
    'No configuration needed for this hand. Click Next to continue.': '这个能力包无需额外配置。点击“下一步”继续。',
    'Missing': '缺失',
    'Dependencies': '依赖',
    'Install All': '全部安装',
    'Installing...': '安装中...',
    'Checking...': '检查中...',
    'Verify': '验证',
    'Next': '下一步',
    'Activating...': '激活中...',
    'Refresh': '刷新',
    'Close': '关闭',
    'Loading browser state...': '正在加载浏览器状态...',
    'Title:': '标题：',
    'Could not load hands.': '无法加载能力包。',
    'Could not load hand details:': '无法加载能力包详情：',
    'All dependencies already installed!': '所有依赖均已安装！',
    'content': '内容',
    'communication': '沟通',
    'data': '数据',
    'productivity': '效率',

    'Browser Hand': '浏览器能力包',
    'Autonomous web browser — navigates sites, fills forms, clicks buttons, and completes multi-step web tasks with user approval for purchases': '自治网页浏览器，可访问网站、填写表单、点击按钮，并在用户批准购买的前提下完成多步骤网页任务。',
    'Python 3 must be installed': '必须安装 Python 3',
    'Playwright must be installed': '必须安装 Playwright',
    'Clip Hand': '剪辑能力包',
    'Turns long-form video into viral short clips with captions and thumbnails': '将长视频切成带字幕和缩略图的爆款短视频。',
    'FFmpeg must be installed': '必须安装 FFmpeg',
    'FFprobe must be installed (ships with FFmpeg)': '必须安装 FFprobe（随 FFmpeg 一起提供）',
    'yt-dlp must be installed': '必须安装 yt-dlp',
    'Collector Hand': '采集能力包',
    'Autonomous intelligence collector — monitors any target continuously with change detection and knowledge graphs': '自治情报采集器，可持续监控任意目标，并进行变化检测与知识图谱构建。',
    'Lead Hand': '线索能力包',
    'Autonomous lead generation — discovers, enriches, and delivers qualified leads on a schedule': '自治获客能力包，可按计划发现、补全并交付合格销售线索。',
    'Predictor Hand': '预测能力包',
    'Autonomous future predictor — collects signals, builds reasoning chains, makes calibrated predictions, and tracks accuracy': '自治预测引擎，可收集信号、构建推理链、做出校准预测并跟踪准确率。',
    'Researcher Hand': '研究能力包',
    'Autonomous deep researcher — exhaustive investigation, cross-referencing, fact-checking, and structured reports': '自治深度研究能力包，可进行深入调查、交叉核验、事实检查并生成结构化报告。',
    'Twitter Hand': 'Twitter 能力包',
    'Autonomous Twitter/X manager — content creation, scheduled posting, engagement, and performance tracking': '自治 Twitter/X 管理器，可生成内容、定时发帖、互动并跟踪效果。',
    'Twitter API Bearer Token': 'Twitter API Bearer Token',

    'Providers': '提供商',
    'Models': '模型',
    'Tools': '工具',
    'Security': '安全',
    'Network': '网络',
    'Budget': '预算',
    'Migration': '迁移',
    'LLM Providers': 'LLM 提供商',
    'OpenFang supports 12 LLM providers out of the box. Configure API keys to unlock models from each provider. Set environment variables and restart, or use the form below to save keys directly.': 'OpenFang 开箱即支持 12 个 LLM 提供商。配置 API Key 后即可解锁各提供商模型。你可以设置环境变量并重启，或直接使用下方表单保存密钥。',
    'Not Set': '未设置',
    'Configured': '已配置',
    'No Key Needed': '无需密钥',
    'Test': '测试',
    'Remove Key': '移除密钥',
    'Or set': '或设置',
    'in your environment and restart': '到环境变量后重启',
    'No API key needed — runs locally or is free': '无需 API Key，可本地运行或免费使用',
    'Base URL': '基础 URL',
    'All Providers': '全部提供商',
    'Browse all available models across providers. Models marked "Available" have their provider configured and ready to use.': '浏览所有提供商下可用的模型。标记为 “Available” 的模型表示对应提供商已配置并可立即使用。',
    'Runtime Configuration': '运行时配置',
    'Needs Key': '需要密钥',
    'Budget & Spending Limits': '预算与花费限制',
    'Monitor and control spending across all agents.': '监控并控制所有代理的花费。',
    'Edit Limits': '编辑限制',
    'Loading budget...': '正在加载预算...',
    'Hourly': '每小时',
    'Daily': '每日',
    'Monthly': '每月',
    'Alert threshold:': '提醒阈值：',
    'of any limit': '（任意限制）',
    'Hourly Limit ($)': '每小时限制（美元）',
    'Daily Limit ($)': '每日限制（美元）',
    'Monthly Limit ($)': '每月限制（美元）',
    'Alert (%)': '提醒（%）',
    'Set to 0 for unlimited. Changes apply immediately (in-memory, not persisted to config.toml).': '设为 0 表示无限制。修改会立即生效（仅内存中，不会持久化到 config.toml）。',
    'Top Spenders (Today)': '今日最高花费',
    'Today': '今日',
    'No spending recorded today.': '今天暂无花费记录。',
    'Migrate from OpenClaw': '从 OpenClaw 迁移',
    'Seamlessly transfer your agents, memory, workspace files, and channel configurations from OpenClaw to OpenFang.': '将你的代理、记忆、工作区文件和渠道配置从 OpenClaw 无缝迁移到 OpenFang。',
    'Converts agent.yaml to agent.toml with proper capabilities': '将 agent.yaml 转换为具备正确能力声明的 agent.toml',
    'Maps tools (read_file → file_read, execute_command → shell_exec, etc.)': '映射工具名称（如 read_file → file_read，execute_command → shell_exec 等）',
    'Merges channel configs into config.toml': '将渠道配置合并进 config.toml',
    'Copies workspace files and memory data': '复制工作区文件和记忆数据',
    'Auto-Detect OpenClaw': '自动检测 OpenClaw',
    'Scanning...': '扫描中...',
    'Enter Path Manually': '手动输入路径',
    'Specify OpenClaw Path': '指定 OpenClaw 路径',
    'OpenClaw Home Directory': 'OpenClaw 主目录',
    'OpenFang Target Directory': 'OpenFang 目标目录',
    'Scan Directory': '扫描目录',
    'OpenClaw Workspace Found': '发现 OpenClaw 工作区',
    'Ready to Migrate': '可迁移',
    'Migrate Now': '立即迁移',
    'Dry Run': '演练运行',
    'Start Over': '重新开始',
    'Dry Run Complete': '演练完成',
    'Migration Complete!': '迁移完成！',
    'SUCCESS': '成功',
    'FAILED': '失败',
    'Run Migration for Real': '执行正式迁移',
    'Start New Migration': '开始新的迁移',
    'Loading settings...': '正在加载设置...',
    'Could not load settings.': '无法加载设置。',
    'System Health': '系统健康',
    'Security Systems': '安全系统',
    'Popular Providers': '热门提供商',
    'Other Providers': '其他提供商',
    'Model Catalog': '模型目录',
    'Search models...': '搜索模型...',
    'All Tiers': '全部层级',
    'Search tools...': '搜索工具...',
    'No providers found': '未找到提供商',
    'Provider information could not be loaded. Check that the API is running.': '无法加载提供商信息。请确认 API 正在运行。',
    'Platform': '平台',
    'Default Model': '默认模型',
    'View and edit the active configuration. Changes are applied immediately. For advanced edits, modify': '查看并编辑当前生效的配置。更改会立即应用。若需高级编辑，请修改',
    'Raw Config JSON (click to toggle)': '原始配置 JSON（点击展开/收起）',
    'Loading security data...': '正在加载安全数据...',
    'Defense in Depth': '纵深防御',
    'OpenFang implements 15 layered security features across the entire stack — from network ingress to agent sandboxing to cryptographic audit trails. Core protections cannot be disabled.': 'OpenFang 在整个技术栈中实现了 15 层安全能力，从网络入口、代理沙箱到加密审计链全部覆盖。核心防护不可关闭。',
    'Core Protections': '核心防护',
    'Always active. Cannot be disabled.': '始终启用，无法关闭。',
    'Protects against:': '可防御：',
    'Configurable Controls': '可配置控制项',
    'Active with tunable parameters.': '当前启用，可调整参数。',
    'Monitoring & Analysis': '监控与分析',
    'Active monitoring systems.': '当前启用的监控系统。',
    'Audit Chain Integrity': '审计链完整性',
    'Verify Now': '立即验证',
    'Run cryptographic verification of the entire SHA-256 Merkle hash chain.': '对整条 SHA-256 Merkle 哈希链执行加密校验。',
    'Security Dependencies': '安全依赖',
    'Peer Networking (OFP)': '节点网络（OFP）',
    'Link multiple OpenFang instances into a mesh via the OFP wire protocol.': '通过 OFP 线协议将多个 OpenFang 实例连接成网状网络。',
    'Enabled': '已启用',
    'Disabled': '已禁用',
    'Node': '节点',
    'Address': '地址',
    'State': '状态',
    'Protocol': '协议',
    'Total Peers': '节点总数',
    'No peers connected': '当前没有已连接节点',
    'A2A External Agents': 'A2A 外部代理',
    'Discovered agents on other OpenFang/A2A-compatible instances that this node can communicate with.': '在其他 OpenFang / A2A 兼容实例上发现的代理，本节点可与其通信。',
    'Discover': '发现',
    'No external agents discovered yet. Enter a URL above to discover one.': '尚未发现外部代理。在上方输入 URL 以发现一个。',
    'Total Spend': '总支出',
    'Today\'s Spend': '今日支出',
    'Projected Monthly': '预计月支出',
    'Avg Cost / Message': '平均每条消息成本',
    'Cost by Provider': '按提供商统计成本',
    'No cost data yet.': '暂无成本数据。',
    'Daily Cost (Last 7 Days)': '每日成本（最近 7 天）',
    'Cost by Model': '按模型统计成本',
    'No model cost data yet.': '暂无模型成本数据。',
    'No daily data yet.': '暂无每日数据。',
    'Total Tokens': '总 Token 数',
    'Estimated Cost': '预估成本',
    'API Calls': 'API 调用次数',
    'Summary': '汇总',
    'By Model': '按模型',
    'By Agent': '按代理',
    'Costs': '成本',
    'Token Breakdown': 'Token 明细',
    'Input Tokens': '输入 Token',
    'Output Tokens': '输出 Token',
    'No model usage data yet.': '暂无模型使用数据。',
    'No agent usage data yet.': '暂无代理使用数据。',
    'Usage': '占比',
    'Listening on': '监听地址',
    'Go to Dashboard': '前往控制台',
    'Main navigation': '主导航',
    'All': '全部',
    'Pending': '待处理',
    'Approved': '已批准',
    'Rejected': '已拒绝',
    'Unknown': '未知',

    'Getting Started': '快速开始',
    'Get started quickly with the guided Setup Wizard, or configure manually:': '通过引导式安装向导快速开始，或手动配置：',
    'Launch Setup Wizard': '启动安装向导',
    'Configure Manually': '手动配置',
    'Dismiss': '关闭',
    'Agents Running': '运行中的代理',
    'Tokens Used': 'Token 用量',
    'Total Cost': '总花费',
    'Uptime': '运行时长',
    'Version': '版本',
    'Recent Activity': '最近活动',
    'View All': '查看全部',
    'No Recent Activity': '暂无最近活动',
    'Activity will appear here once agents start processing.': '代理开始处理任务后，活动会显示在这里。',
    'Quick Actions': '快捷操作',
    'New Agent': '新建代理',
    'Add Channel': '添加渠道',
    'Create Agent': '创建代理',
    'Spawn a new agent': '创建一个新的代理',
    'Configure Provider': '配置提供商',
    'Set up an LLM provider': '设置一个 LLM 提供商',
    'Browse Skills': '浏览技能',
    'Browse or install a skill': '浏览或安装技能',
    'Explore available skills': '查看可用技能',
    'Go': '前往',
    'Connected Channels': '已连接渠道',
    'MCP Servers': 'MCP 服务器',
    'Tool Calls': '工具调用',
    'Chat with an Agent': '与代理聊天',
    'Configure an LLM provider': '配置一个 LLM 提供商',
    'Create your first agent': '创建你的第一个代理',
    'Send your first message': '发送你的第一条消息',
    'Connect a messaging channel': '连接一个消息渠道',
    'Welcome to OpenFang': '欢迎使用 OpenFang',
    'Merkle Audit': 'Merkle 审计',
    'Taint Tracking': '污点追踪',
    'WASM Sandbox': 'WASM 沙箱',
    'GCRA Rate Limit': 'GCRA 限流',
    'Ed25519 Signing': 'Ed25519 签名',
    'SSRF Protection': 'SSRF 防护',
    'Path Traversal Prevention': '路径穿越防护',
    'Capability-Based Access Control': '基于能力的访问控制',
    'Privilege Escalation Prevention': '权限提升防护',
    'Subprocess Environment Isolation': '子进程环境隔离',
    'Security Headers': '安全响应头',
    'Wire Protocol Authentication': '线协议认证',
    'Request ID Tracking': '请求 ID 跟踪',
    'API Rate Limiting': 'API 限流',
    'WebSocket Connection Limits': 'WebSocket 连接限制',
    'WASM Dual Metering': 'WASM 双重计量',
    'Bearer Token Authentication': 'Bearer 令牌认证',
    'Merkle Audit Trail': 'Merkle 审计链',
    'Information Flow Taint Tracking': '信息流污点追踪',
    'Ed25519 Manifest Signing': 'Ed25519 清单签名',
    'ALWAYS ON': '始终开启',
    'CONFIGURABLE': '可配置',
    'MONITORING': '监控中',
    'Secret Zeroize': '敏感信息清零',
    'Loop Guard': '循环保护',
    'Session Repair': '会话修复',
    'Blocks directory escape attacks (../) in all file operations. Two-phase validation: syntactic rejection of path components, then canonicalization to normalize symlinks.': '在所有文件操作中拦截目录逃逸攻击（../）。采用两阶段校验：先从语法上拒绝危险路径片段，再通过规范化处理符号链接。',
    'Directory escape, privilege escalation via symlinks': '目录逃逸、通过符号链接提权',
    'Blocks outbound requests to private IPs, localhost, and cloud metadata endpoints (AWS/GCP/Azure). Validates DNS resolution results to defeat rebinding attacks.': '拦截发往私有 IP、localhost 和云元数据端点（AWS/GCP/Azure）的出站请求。还会校验 DNS 解析结果，以防御 DNS rebinding 攻击。',
    'Internal network reconnaissance, cloud credential theft': '内网探测、云凭证窃取',
    'Deny-by-default permission system. Every agent operation (file I/O, network, shell, memory, spawn) requires an explicit capability grant in the manifest.': '默认拒绝的权限系统。每个代理操作（文件 I/O、网络、Shell、记忆、派生子代理）都必须在清单中显式授予能力。',
    'Unauthorized resource access, sandbox escape': '未授权资源访问、沙箱逃逸',
    'When a parent agent spawns a child, the kernel enforces child capabilities are a subset of parent capabilities. No agent can grant rights it does not have.': '父代理派生子代理时，内核会强制子代理能力必须是父代理能力的子集。任何代理都不能授予自己没有的权限。',
    'Capability escalation through agent spawning chains': '通过代理派生链进行能力升级',
    'Child processes (shell tools) inherit only a safe allow-list of environment variables. API keys, database passwords, and secrets are never leaked to subprocesses.': '子进程（Shell 工具）只会继承安全白名单中的环境变量。API Key、数据库密码和其他密钥不会泄露给子进程。',
    'Secret exfiltration via child process environment': '通过子进程环境窃取密钥',
    'Every HTTP response includes CSP, X-Frame-Options: DENY, X-Content-Type-Options: nosniff, Referrer-Policy, and X-XSS-Protection headers.': '每个 HTTP 响应都包含 CSP、X-Frame-Options: DENY、X-Content-Type-Options: nosniff、Referrer-Policy 和 X-XSS-Protection 等安全头。',
    'XSS, clickjacking, MIME sniffing, content injection': 'XSS、点击劫持、MIME 嗅探、内容注入',
    'Agent-to-agent OFP connections use HMAC-SHA256 mutual authentication with nonce-based handshake and constant-time signature comparison (subtle crate).': '代理间 OFP 连接使用 HMAC-SHA256 双向认证，包含基于 nonce 的握手以及常量时间签名比较（subtle crate）。',
    'Man-in-the-middle attacks on mesh network': '网状网络中的中间人攻击',
    'Every API request receives a unique UUID (x-request-id header) and is logged with method, path, status code, and latency for full traceability.': '每个 API 请求都会分配唯一 UUID（x-request-id 头），并记录请求方法、路径、状态码和时延，便于完整追踪。',
    'Untraceable actions, forensic blind spots': '无法追踪的操作、取证盲区',
    'GCRA (Generic Cell Rate Algorithm) with cost-aware tokens. Different endpoints cost different amounts — spawning an agent costs 50 tokens, health check costs 1.': '使用 GCRA（通用单元速率算法）与按成本计价的令牌机制。不同端点消耗不同令牌，例如派生代理消耗 50 个令牌，健康检查消耗 1 个。',
    'Per-IP connection cap prevents connection exhaustion. Idle timeout closes abandoned connections. Message rate limiting prevents flooding.': '按 IP 限制连接数，防止连接耗尽。空闲超时会关闭废弃连接，消息速率限制可防止洪泛。',
    'WASM modules run with two independent resource limits: fuel metering (CPU instruction count) and epoch interruption (wall-clock timeout with watchdog thread).': 'WASM 模块运行时受两套独立资源限制：fuel 计量（CPU 指令数）和 epoch 中断（基于 watchdog 线程的墙钟超时）。',
    'All non-health endpoints require Authorization: Bearer header. When no API key is configured, all requests are restricted to localhost only.': '除健康检查外，所有端点都要求携带 Authorization: Bearer 请求头。未配置 API Key 时，所有请求仅允许从 localhost 访问。',
    'Every security-critical action is appended to an immutable, tamper-evident log. Each entry is cryptographically linked to the previous via SHA-256 hash chain.': '每个安全关键操作都会追加到不可变、可感知篡改的日志中。每条记录都会通过 SHA-256 哈希链与上一条记录进行加密关联。',
    'Labels data by provenance (ExternalNetwork, UserInput, PII, Secret, UntrustedAgent) and blocks unsafe flows: external data cannot reach shell_exec, secrets cannot reach network.': '按来源为数据打标签（ExternalNetwork、UserInput、PII、Secret、UntrustedAgent），并阻断不安全流向：外部数据不能进入 shell_exec，密钥不能流向网络。',
    'Agent manifests can be cryptographically signed with Ed25519. Verify manifest integrity before loading to prevent supply chain tampering.': '代理清单可使用 Ed25519 进行加密签名。在加载前验证清单完整性，可防止供应链篡改。',
    'Hard-coded: 500 tokens/minute per IP. Edit rate_limiter.rs to tune.': '硬编码：每 IP 每分钟 500 个令牌。可编辑 rate_limiter.rs 调整。',
    'Hard-coded: 5 connections/IP, 30min idle timeout, 64KB max message. Edit ws.rs to tune.': '硬编码：每 IP 5 个连接、空闲超时 30 分钟、最大消息 64KB。可编辑 ws.rs 调整。',
    'Default: 1M fuel units, 30s timeout. Configurable per-agent via SandboxConfig.': '默认值：100 万 fuel 单位、30 秒超时。可通过 SandboxConfig 按代理配置。',
    'Set api_key in ~/.openfang/config.toml for remote access. Empty = localhost only.': '在 ~/.openfang/config.toml 中设置 api_key 以启用远程访问。留空则仅允许 localhost。',
    'Always active. Verify chain integrity from the Audit Log page.': '始终启用。可在审计日志页面验证链完整性。',
    'Always active. Prevents data flow attacks automatically.': '始终启用。会自动阻止数据流攻击。',
    'Available for use. Sign manifests with ed25519-dalek for verification.': '可直接使用。可用 ed25519-dalek 对清单签名并验证。',
    'host_functions.rs — safe_resolve_path() + safe_resolve_parent()': '实现：host_functions.rs · safe_resolve_path() + safe_resolve_parent()',
    'host_functions.rs — is_ssrf_target() + is_private_ip()': '实现：host_functions.rs · is_ssrf_target() + is_private_ip()',
    'host_functions.rs — check_capability() on every host function': '实现：host_functions.rs · 在每个 host function 上执行 check_capability()',
    'kernel_handle.rs — spawn_agent_checked()': '实现：kernel_handle.rs · spawn_agent_checked()',
    'subprocess_sandbox.rs — env_clear() + SAFE_ENV_VARS': '实现：subprocess_sandbox.rs · env_clear() + SAFE_ENV_VARS',
    'middleware.rs — security_headers()': '实现：middleware.rs · security_headers()',
    'peer.rs — hmac_sign() + hmac_verify()': '实现：peer.rs · hmac_sign() + hmac_verify()',
    'middleware.rs — request_logging()': '实现：middleware.rs · request_logging()',
    'sha2 SHA-256 · hmac HMAC-SHA256 · subtle constant-time · ed25519-dalek signing · zeroize secret wiping · rand randomness · governor rate limiting': 'sha2 SHA-256 · hmac HMAC-SHA256 · subtle 常量时间比较 · ed25519-dalek 签名 · zeroize 密钥清零 · rand 随机数 · governor 限流',

    'Execution Approvals': '执行审批',
    'No approvals': '暂无审批项',
    'When agents request permission for sensitive actions, they\'ll appear here.': '当代理请求执行敏感操作时，它们会显示在这里。',
    'Reject Action': '拒绝操作',
    'Are you sure you want to reject this action?': '你确定要拒绝这个操作吗？',
    'Could not load approvals.': '无法加载审批。',

    'Conversation Sessions': '会话列表',
    'Filter by agent...': '按代理筛选...',
    'No sessions yet': '暂无会话',
    'Sessions are created when you chat with agents. Start a conversation to see session history here.': '当你与代理聊天时会创建会话。开始一次对话后，可在这里查看会话历史。',
    'Memory': '记忆',
    'Select an Agent': '选择一个代理',
    'Agent Memory': '代理记忆',
    'Each agent has its own key-value memory store. Agents use memory to persist preferences, notes, and context between conversations.': '每个代理都有自己的键值记忆存储。代理会用记忆保存偏好、备注以及跨会话上下文。',
    'Loading memory...': '正在加载记忆...',
    'No keys stored': '暂无已存储键',
    'This agent has no memory entries yet. Agents create memory entries automatically during conversations, or you can add them manually.': '这个代理还没有记忆条目。代理会在对话中自动创建记忆，你也可以手动添加。',
    '+ Add First Key': '+ 添加第一个键',
    'Add Key': '添加键',
    'Select agent...': '选择代理...',
    'Delete Session': '删除会话',
    'This will permanently remove the session and its messages.': '这会永久删除该会话及其消息。',
    'Session deleted': '会话已删除',
    'Could not load sessions.': '无法加载会话。',
    'Could not load memory data.': '无法加载记忆数据。',
    'Delete Key': '删除键',
    'Skills & Ecosystem': '技能与生态',
    'Skills extend your agents with new capabilities. OpenFang supports the OpenClaw/ClawHub ecosystem (3,000+ community skills) plus local skills.': '技能可为你的代理扩展新能力。OpenFang 支持 OpenClaw / ClawHub 生态（3,000+ 社区技能）以及本地技能。',
    'Skills extend your agents with new capabilities. OpenFang supports the': '技能可为你的代理扩展新能力。OpenFang 支持',
    'ecosystem (3,000+ community skills) plus local skills.': '生态（3,000+ 社区技能）以及本地技能。',
    'Prompt-only': '仅提示词',
    'inject context and instructions into the agent\'s system prompt (most ClawHub skills)': '将上下文和指令注入代理系统提示词中（大多数 ClawHub 技能属于此类）',
    '— inject context and instructions into the agent\'s system prompt (most ClawHub skills)': '— 将上下文和指令注入代理系统提示词中（大多数 ClawHub 技能属于此类）',
    'Python / Node.js — executable tools that agents can call during conversations': 'Python / Node.js：代理可在对话中调用的可执行工具',
    '— executable tools that agents can call during conversations': '— 代理可在对话中调用的可执行工具',
    'external tools via Model Context Protocol (GitHub, filesystem, databases, etc.)': '通过 Model Context Protocol 提供的外部工具（GitHub、文件系统、数据库等）',
    '— external tools via Model Context Protocol (GitHub, filesystem, databases, etc.)': '— 通过 Model Context Protocol 提供的外部工具（GitHub、文件系统、数据库等）',
    'Installed': '已安装',
    'Quick Start': '快速开始',
    'Loading skills...': '正在加载技能...',
    'No skills installed': '尚未安装技能',
    'Skills add new capabilities to your agents. Browse ClawHub for 3,000+ community skills or create your own.': '技能可以为你的代理增加新能力。你可以浏览 ClawHub 上 3,000+ 个社区技能，或自己创建。',
    'Browse ClawHub': '浏览 ClawHub',
    'Search ClawHub skills... (type to search)': '搜索 ClawHub 技能……（输入即可搜索）',
    'Clear search (Esc)': '清除搜索（Esc）',
    'Trending': '热门趋势',
    'Most Downloaded': '下载最多',
    'Most Starred': '收藏最多',
    'Recently Updated': '最近更新',
    'CATEGORIES': '分类',
    'Searching ClawHub...': '正在搜索 ClawHub...',
    'ClawHub may be temporarily unavailable. The OpenClaw ecosystem is hosted at clawhub.ai.': 'ClawHub 可能暂时不可用。OpenClaw 生态托管在 clawhub.ai。',
    'Clear search': '清除搜索',
    'Load More': '加载更多',
    'Back to browse': '返回浏览',
    'MCP Servers (Model Context Protocol)': 'MCP 服务器（Model Context Protocol）',
    'MCP servers provide external tools to your agents — GitHub, filesystem, databases, APIs, and more. OpenFang is compatible with all OpenClaw MCP servers.': 'MCP 服务器可为你的代理提供外部工具，例如 GitHub、文件系统、数据库、API 等。OpenFang 兼容所有 OpenClaw MCP 服务器。',
    'Add a [network] section to config.toml with shared_secret and peer addresses.': '在 config.toml 中添加 [network] 配置段，并设置 shared_secret 与节点地址。',
    'Configure MCP servers in your': '请在你的',
    'Connected': '已连接',
    'Tools:': '工具：',
    'No MCP servers configured': '尚未配置 MCP 服务器',
    'MCP servers extend your agents with external tools. Add servers to your config.toml:': 'MCP 服务器可为你的代理扩展外部工具。可在 config.toml 中添加服务器：',
    'OpenFang supports all OpenClaw-compatible MCP servers.': 'OpenFang 支持所有兼容 OpenClaw 的 MCP 服务器。',
    'Quick Start Skills': '快速开始技能',
    'Create prompt-only skills with one click. These inject context into your agent\'s system prompt — no code required. Perfect for adding domain expertise or workflow guidelines.': '一键创建仅提示词技能。它们会把上下文注入代理的系统提示词中，无需编写代码。非常适合补充领域知识或工作流规范。',
    'Created Skill': '已创建',
    'Create Skill': '创建技能',
    'Loading skill details...': '正在加载技能详情...',
    'Security Warnings': '安全警告',
    'Already Installed': '已安装',
    'Install from ClawHub': '从 ClawHub 安装',
    'Skills are security-scanned before installation. Prompt injection and malware patterns are blocked.': '技能在安装前会经过安全扫描。提示词注入和恶意模式都会被拦截。',
    'Built-in': '内置',
    'Local': '本地',
    'Could not load skills.': '无法加载技能。',
    'Skill is already installed': '技能已安装',
    'Skill blocked by security scan': '技能被安全扫描拦截',
    'Uninstall Skill': '卸载技能',
    'Please enter an API key': '请输入 API Key',
    'Please enter a base URL': '请输入基础 URL',
    'URL must start with http:// or https://': 'URL 必须以 http:// 或 https:// 开头',
    'Test failed:': '测试失败：',
    'Install failed': '安装失败',
    'Coding & IDEs': '编码与 IDE',
    'Git & GitHub': 'Git 与 GitHub',
    'Web & Frontend': 'Web 与前端',
    'DevOps & Cloud': 'DevOps 与云',
    'Browser & Automation': '浏览器与自动化',
    'Search & Research': '搜索与研究',
    'AI & LLMs': 'AI 与 LLM',
    'Data & Analytics': '数据与分析',
    'Productivity': '效率工具',
    'Communication': '沟通协作',
    'Media & Streaming': '媒体与流处理',
    'Notes & PKM': '笔记与知识管理',
    'CLI Utilities': 'CLI 工具',
    'Marketing & Sales': '市场与销售',
    'Finance': '金融',
    'Smart Home & IoT': '智能家居与物联网',
    'PDF & Documents': 'PDF 与文档',
    'Install': '安装',
    'Installing...': '安装中...',
    'Connected servers': '已连接服务器',
    'Configured servers': '已配置服务器',
    'ClawHub': 'ClawHub',
    'OpenClaw': 'OpenClaw',

    'List': '列表',
    'Loading workflows...': '正在加载工作流...',
    'What are Workflows?': '什么是工作流？',
    'Workflows chain multiple agents into automated pipelines. Each step runs an agent with a prompt template, passing output from one step as input to the next. Steps can run sequentially, fan out in parallel, loop, or branch conditionally.': '工作流会把多个代理串成自动化流水线。每一步都会用提示词模板运行一个代理，并把上一步输出传给下一步作为输入。步骤可以顺序执行、并行分发、循环执行，或按条件分支。',
    'Try the': '试试',
    'Visual Builder': '可视化编排',
    'to drag and drop workflow steps.': '来拖拽工作流步骤。',
    '+ New Workflow': '+ 新建工作流',
    'No workflows yet': '暂无工作流',
    'Chain multiple agents into automated pipelines with branching, fan-out, and loops.': '将多个代理串联成带分支、扇出和循环的自动化流水线。',
    'Create Workflow': '创建工作流',
    'Description': '描述',
    'Conditional': '条件分支',
    'Step name': '步骤名称',
    'Agent name': '代理名称',
    '+ Add Step': '+ 添加步骤',
    'Input': '输入',
    'Enter workflow input...': '输入工作流内容...',
    'Execute': '执行',
    'Result': '结果',
    'Could not load workflows.': '无法加载工作流。',
    'Workflow completed': '工作流执行完成',
    'Node Palette': '节点面板',
    'Drag nodes onto the canvas': '将节点拖到画布上',
    'Workflow': '工作流',
    'Workflow name': '工作流名称',
    'Export TOML': '导出 TOML',
    'Save Workflow': '保存工作流',
    'Auto Layout': '自动布局',
    'Fit': '适配',
    'Agent Step': '代理步骤',
    'Parallel Fan-out': '并行分发',
    'Condition': '条件判断',
    'Loop': '循环',
    'Collect': '汇总',
    'Start': '开始',
    'End': '结束',
    'No agent': '未选择代理',
    'No condition': '未设置条件',
    'Prompt Template': '提示词模板',
    'Model (optional)': '模型（可选）',
    'Default model': '默认模型',
    'Expression': '表达式',
    'Top port = true, bottom port = false': '上方端口为 true，下方端口为 false',
    'Max Iterations': '最大迭代次数',
    'Until (stop condition)': '直到（停止条件）',
    'Fan-out Count': '扇出数量',
    'Strategy': '策略',
    'Wait for all': '等待全部完成',
    'First to finish': '最先完成者',
    'Majority vote': '多数投票',
    'Duplicate': '复制',
    'Connection selected': '已选中连接线',
    'Delete Connection': '删除连接',
    'Generated TOML': '生成的 TOML',
    'Copy to Clipboard': '复制到剪贴板',

    'Messaging': '消息',
    'Social': '社交',
    'Enterprise': '企业',
    'Developer': '开发者',
    'Notifications': '通知',
    'Edit': '编辑',
    'Configure': '配置',
    'Not Configured': '未配置',
    'Missing Token': '缺少令牌',
    'Configured': '已配置',
    'Search channels...': '搜索渠道...',
    'No channels match your search.': '没有匹配搜索条件的渠道。',
    'Set up': '设置',
    'Connect your personal WhatsApp via QR scan': '通过扫码连接你的个人 WhatsApp',
    'Telegram Bot API — long-polling adapter': 'Telegram Bot API 长轮询适配器',
    'Slack Socket Mode + Events API': 'Slack Socket Mode + Events API 适配器',
    'Signal via signal-cli REST API': '通过 signal-cli REST API 连接 Signal',
    'Matrix/Element bot via homeserver': '通过 homeserver 连接 Matrix/Element 机器人',
    'Discord Gateway bot adapter': 'Discord Gateway 机器人适配器',
    'IMAP/SMTP email adapter': 'IMAP/SMTP 邮件适配器',
    'LINE Messaging API adapter': 'LINE Messaging API 适配器',
    'Viber Bot API adapter': 'Viber Bot API 适配器',
    'Facebook Messenger Platform adapter': 'Facebook Messenger 平台适配器',
    'Threema Gateway adapter': 'Threema Gateway 适配器',
    'Keybase chat bot adapter': 'Keybase 聊天机器人适配器',
    'Reddit API bot adapter': 'Reddit API 机器人适配器',
    'Mastodon Streaming API adapter': 'Mastodon Streaming API 适配器',
    'Bluesky/AT Protocol adapter': 'Bluesky / AT Protocol 适配器',
    'LinkedIn Messaging API adapter': 'LinkedIn Messaging API 适配器',
    'Nostr relay protocol adapter': 'Nostr 中继协议适配器',
    'Teams Bot Framework adapter': 'Teams Bot Framework 适配器',
    'Cisco Webex bot adapter': 'Cisco Webex 机器人适配器',
    'DingTalk Robot API adapter': '钉钉机器人 API 适配器',
    'Feishu/Lark Open Platform adapter': '飞书 / Lark 开放平台适配器',
    'Nextcloud Talk REST adapter': 'Nextcloud Talk REST 适配器',
    'Rocket.Chat REST adapter': 'Rocket.Chat REST 适配器',
    'Mattermost WebSocket adapter': 'Mattermost WebSocket 适配器',
    'Zulip event queue adapter': 'Zulip 事件队列适配器',
    'IRC raw TCP adapter': 'IRC 原始 TCP 适配器',
    'XMPP/Jabber protocol adapter': 'XMPP / Jabber 协议适配器',
    'Google Chat service account adapter': 'Google Chat 服务账号适配器',
    'Discourse forum API adapter': 'Discourse 论坛 API 适配器',
    'Gitter Streaming API adapter': 'Gitter Streaming API 适配器',
    'Guilded bot adapter': 'Guilded 机器人适配器',
    'Twist API v3 adapter': 'Twist API v3 适配器',
    'Pumble bot adapter': 'Pumble 机器人适配器',
    'Flock bot adapter': 'Flock 机器人适配器',
    'Mumble text chat adapter': 'Mumble 文本聊天适配器',
    'Gotify WebSocket notification adapter': 'Gotify WebSocket 通知适配器',
    'ntfy.sh pub/sub notification adapter': 'ntfy.sh 发布 / 订阅通知适配器',
    'Generic HMAC-signed webhook adapter': '通用 HMAC 签名 Webhook 适配器',
    'Revolt bot adapter': 'Revolt 机器人适配器',
    'Twitch IRC gateway adapter': 'Twitch IRC 网关适配器',

    'Live': '实时',
    'Audit Trail': '审计链',
    'Connecting to log stream...': '正在连接日志流...',
    'No log entries yet': '暂无日志记录',
    'Activity will appear here as agents run.': '代理运行后，活动会显示在这里。',
    'Clear': '清空',
    'Export': '导出',
    'Auto-scroll': '自动滚动',
    'Scroll locked': '滚动已锁定',
    'Paused': '已暂停',
    'Polling': '轮询中',
    'Disconnected': '已断开',
    'Tool Completed': '工具已完成',
    'Login Success': '登录成功',
    'Login Failed': '登录失败',
    'Permission Denied': '权限不足',
    'Rate Limited': '已限流',
    'Verify Chain': '验证链',
    'Tamper-Evident Audit Trail': '防篡改审计链',
    'Every agent action is logged with a cryptographic hash chain. Use "Verify Chain" to confirm no entries have been altered or deleted.': '每个代理操作都会被记录到加密哈希链中。使用“验证链”来确认没有条目被篡改或删除。',
    'All Actions': '全部操作',
    'Agent Created': '代理已创建',
    'Agent Stopped': '代理已停止',
    'Tool Used': '已使用工具',
    'Network Access': '网络访问',
    'Shell Command': 'Shell 命令',
    'File Access': '文件访问',
    'Memory Access': '记忆访问',
    'Login Attempt': '登录尝试',
    'No audit entries yet': '暂无审计记录',
    'Could not load logs.': '无法加载日志。',
    'Could not load audit log.': '无法加载审计日志。',
    'Audit chain broken!': '审计链已损坏！',

    'Setup Wizard': '安装向导',
    'Skip Setup': '跳过安装',
    'Welcome': '欢迎',
    'Try It': '试一试',
    'Channel': '渠道',
    'Done': '完成',
    'This wizard will help you:': '这个向导将帮助你：',
    'Connect an LLM provider (Anthropic, OpenAI, Gemini, etc.)': '连接一个 LLM 提供商（Anthropic、OpenAI、Gemini 等）',
    'Create your first AI agent from 10 templates': '从 10 个模板中创建你的第一个 AI 代理',
    'Try it out with a quick test message': '用一条简短测试消息试用它',
    'Optionally connect a messaging channel (Telegram, Discord, Slack)': '可选连接一个消息渠道（Telegram、Discord、Slack）',
    'Takes about 2 minutes. You can skip any step and configure later.': '大约需要 2 分钟。你可以跳过任一步骤，之后再配置。',
    'Get Started': '开始使用',
    'Connect an LLM Provider': '连接一个 LLM 提供商',
    'OpenFang needs at least one LLM provider to power your agents. Select a provider and enter your API key.': 'OpenFang 至少需要一个 LLM 提供商来驱动你的代理。请选择一个提供商并输入 API Key。',
    'Provider Already Configured': '提供商已配置',
    'You already have at least one provider set up. You can continue to the next step or configure additional providers.': '你已经至少配置了一个提供商。你可以继续下一步，或继续配置更多提供商。',
    'READY': '已就绪',
    'Environment variable:': '环境变量：',
    'API Key': 'API Key',
    'Save & Test': '保存并测试',
    'Connected successfully': '连接成功',
    'Connection failed': '连接失败',
    'You can test the connection or continue to the next step.': '你可以测试连接，或继续下一步。',
    'Test Connection': '测试连接',
    'Connected': '已连接',
    'Create Your First Agent': '创建你的第一个代理',
    'Pick a template to get started quickly. You can customize the agent later or create more from the Agents page.': '选择一个模板以快速开始。你稍后可以再自定义这个代理，或在“代理”页面创建更多代理。',
    'Agent Name': '代理名称',
    'Try Your Agent': '试用你的代理',
    'Send a quick message to test your new agent. Try one of the suggestions below or type your own.': '发送一条简短消息来测试你的新代理。你可以点下面的建议，也可以自己输入。',
    'Thinking...': '思考中...',
    'Type a message...': '输入一条消息...',
    'Send': '发送',
    'Continue': '继续',
    'Connect a Channel': '连接渠道',
    'Optional': '可选',
    'Channels let your agent communicate via messaging platforms. This is optional — you can always use the built-in web chat.': '渠道可让你的代理通过消息平台进行沟通。这一步是可选的，你始终可以使用内置网页聊天。',
    'You can skip this step. The built-in web chat is always available from the Agents page. Add channels any time from Settings → Channels.': '你可以跳过这一步。内置网页聊天始终可在“代理”页面使用。你也可以随时在“设置 → 渠道”中添加渠道。',
    'Channel will activate automatically.': '渠道会自动启用。',
    'Edit Config': '编辑配置',
    'Connecting to WhatsApp Web gateway...': '正在连接 WhatsApp Web 网关...',
    'WhatsApp linked successfully!': 'WhatsApp 已连接成功！',
    'WhatsApp Web gateway not available': 'WhatsApp Web 网关不可用',
    'Business API': 'Business API',
    'Use Business API instead': '改用 Business API',
    'Have a Meta Business account?': '有 Meta Business 账号？',
    'Refresh QR': '刷新二维码',
    'Testing...': '测试中...',
    'Back to QR scan': '返回扫码',
    'How to get credentials': '如何获取凭据',
    'Hide advanced': '隐藏高级选项',
    'Update': '更新',
    'Your channel is configured and verified. It will activate automatically.': '你的渠道已完成配置并验证，系统会自动启用。',
    'Configure via WhatsApp Cloud API (requires a Meta Business developer account).': '通过 WhatsApp Cloud API 配置（需要 Meta Business 开发者账号）。',
    'Remove': '移除',
    'Remove Channel': '移除渠道',
    'You\'re All Set!': '全部就绪！',
    'Here is a summary of what was set up:': '以下是已完成设置的摘要：',
    'LLM Provider': 'LLM 提供商',
    'First Agent': '第一个代理',
    'Pre-configured': '预先配置',
    'Skipped': '已跳过',
    'None (web chat available)': '无（可使用网页聊天）',
    'Next Steps': '下一步',
    'Check Settings for advanced configuration': '前往“设置”查看高级配置',
    'Visit Channels to connect messaging platforms': '前往“渠道”连接消息平台',
    'Minimal': '最小',
    'Read-only file access': '只读文件访问',
    'Coding': '编码',
    'Files + shell + web fetch': '文件 + shell + 网页抓取',
    'Balanced': '均衡',
    'General-purpose tool set': '通用工具集',
    'Precise': '精准',
    'Focused tool set for accuracy': '偏重准确性的工具集',
    'Creative': '创意',
    'Full tools with creative emphasis': '带创意偏重的完整工具集',
    'All 35+ tools': '全部 35+ 工具',
    'What can you help me with?': '你能帮我做什么？',
    'Tell me a fun fact': '告诉我一个有趣的冷知识',
    'Summarize the latest AI news': '总结一下最新 AI 新闻',
    'Write a Python hello world': '写一个 Python hello world',
    'Explain async/await': '解释一下 async/await',
    'Review this code snippet': '帮我审查这段代码',
    'Explain quantum computing simply': '用简单的话解释量子计算',
    'Compare React vs Vue': '比较一下 React 和 Vue',
    'What are the latest trends in AI?': 'AI 最新趋势是什么？',
    'Help me write a professional email': '帮我写一封专业邮件',
    'Improve this paragraph': '润色这段文字',
    'Write a blog intro about AI': '写一段关于 AI 的博客开头',
    'Draft a meeting agenda': '起草一份会议议程',
    'How do I handle a complaint?': '我该如何处理投诉？',
    'Create a project status update': '写一份项目状态更新',
    'Connect your agent to a Telegram bot for messaging.': '将你的代理连接到 Telegram 机器人以进行消息交互。',
    'Connect your agent to a Discord server via bot token.': '通过机器人 Token 将你的代理连接到 Discord 服务器。',
    'Connect your agent to a Slack workspace.': '将你的代理连接到 Slack 工作区。',
    'Create a bot via @BotFather on Telegram to get your token.': '在 Telegram 中通过 @BotFather 创建机器人以获取 Token。',
    'Create a Discord application at discord.com/developers and add a bot.': '在 discord.com/developers 创建一个 Discord 应用并添加机器人。',
    'Create a Slack app at api.slack.com/apps and install it to your workspace.': '在 api.slack.com/apps 创建 Slack 应用并安装到你的工作区。',

    'Copy': '复制',
    'Copied!': '已复制！',
    'Copied to clipboard': '已复制到剪贴板',
    'Copy failed': '复制失败',
    'Loading...': '加载中...',
    'Search...': '搜索...',
    'Welcome to OpenFang Chat!': '欢迎使用 OpenFang 聊天！',
    'Type / for commands': '输入 / 查看命令',
    '/think on for reasoning': '/think on 开启扩展推理',
    'Ctrl+Shift+F for focus mode': 'Ctrl+Shift+F 切换专注模式',
    'Drag files to attach': '拖拽文件即可附加',
    '/model to switch models': '/model 切换模型',
    '/context to check usage': '/context 查看上下文占用',
    '/verbose off to hide tool details': '/verbose off 隐藏工具细节',
    'Show available commands': '显示可用命令',
    'Switch to Agents page': '切换到代理页面',
    'Reset session (clear history)': '重置会话（清空历史）',
    'Trigger LLM session compaction': '触发 LLM 会话压缩',
    'Show or switch model (/model [name])': '显示或切换模型（/model [name]）',
    'Cancel current agent run': '取消当前代理运行',
    'Show session token usage & cost': '显示会话 Token 用量与成本',
    'Toggle extended thinking (/think [on|off|stream])': '切换扩展思考（/think [on|off|stream]）',
    'Show context window usage & pressure': '显示上下文窗口用量与压力',
    'Cycle tool detail level (/verbose [off|on|full])': '切换工具详情级别（/verbose [off|on|full]）',
    'Check if agent is processing': '查看代理是否正在处理',
    'Show system status': '显示系统状态',
    'Clear chat display': '清空聊天显示',
    'Disconnect from agent': '断开与代理的连接',
    'Show spending limits and current costs': '显示支出限制与当前成本',
    'Show OFP peer network status': '显示 OFP 节点网络状态',
    'List discovered external A2A agents': '列出已发现的外部 A2A 代理',

    'Cancel': '取消',
    'Confirm': '确认',

    'Cannot reach daemon — is openfang running?': '无法连接到守护进程 — openfang 是否正在运行？',
    'Not authorized — check your API key': '未授权 — 请检查 API 密钥',
    'Permission denied': '权限不足',
    'Resource not found': '资源不存在',
    'Rate limited — slow down and try again': '触发限流 — 请稍后再试',
    'Request too large': '请求过大',
    'Server error — check daemon logs': '服务器错误 — 请查看守护进程日志',
    'Daemon unavailable — is it running?': '守护进程不可用 — 是否正在运行？',
    'Connection Error': '连接错误',

    'Reconnected': '已重新连接',
    'Connection lost, reconnecting...': '连接已断开，正在重连...',
    'Connection lost — switched to HTTP mode': '连接已断开 — 已切换到 HTTP 模式',

    'Upload failed': '上传失败'
  };

  function normalizeLocale(loc) {
    if (!loc) return DEFAULT_LOCALE;
    var l = String(loc).trim();
    if (l === 'zh' || l.toLowerCase() === 'zh-cn' || l.toLowerCase() === 'zh_cn') return 'zh-CN';
    if (l.toLowerCase().startsWith('zh-')) return 'zh-CN';
    if (SUPPORTED.indexOf(l) >= 0) return l;
    return DEFAULT_LOCALE;
  }

  function getNavigatorLocale() {
    var lang = (navigator.languages && navigator.languages[0]) || navigator.language || navigator.userLanguage;
    return normalizeLocale(lang);
  }

  function getStoredLocale() {
    var stored;
    try {
      stored = localStorage.getItem(STORAGE_KEY);
    } catch (e) {
      return null;
    }
    return stored ? normalizeLocale(stored) : null;
  }

  var _locale = getStoredLocale() || getNavigatorLocale() || DEFAULT_LOCALE;
  _locale = normalizeLocale(_locale);
  var _titleOriginal = '';
  var _textOriginal = (typeof WeakMap !== 'undefined') ? new WeakMap() : null;
  var _attrOriginal = (typeof WeakMap !== 'undefined') ? new WeakMap() : null;

  function setLocale(loc) {
    _locale = normalizeLocale(loc);
    try { localStorage.setItem(STORAGE_KEY, _locale); } catch (e) {}

    try {
      document.documentElement.lang = _locale === 'zh-CN' ? 'zh-CN' : 'en';
      if (document.title) {
        if (!_titleOriginal) _titleOriginal = document.title;
        document.title = translateTextForLocale(_titleOriginal, _locale);
      }
    } catch (e) {}
  }

  function getLocale() {
    return _locale;
  }

  function translateExactZh(enText) {
    return zhMap[enText] || enText;
  }

  function translatePatternsZh(text) {
    if (!text) return text;

    var m = String(text).match(/^\s*(\d+)\s+agent\(s\)\s+running\s*$/);
    if (m) return m[1] + ' 个代理运行中';

    if (String(text).startsWith('disconnected — ')) {
      return '已断开连接 — ' + String(text).slice('disconnected — '.length);
    }

    if (/^Error:\s*/.test(String(text))) {
      return String(text).replace(/^Error:\s*/, '错误：');
    }

    if (String(text) === 'never') return '从不';
    if (String(text) === 'just now') return '刚刚';
    if (String(text) === 'in <1m') return '不到 1 分钟后';
    m = String(text).match(/^in\s+(\d+)m$/);
    if (m) return m[1] + ' 分钟后';
    m = String(text).match(/^in\s+(\d+)h$/);
    if (m) return m[1] + ' 小时后';
    m = String(text).match(/^in\s+(\d+)d$/);
    if (m) return m[1] + ' 天后';
    m = String(text).match(/^(\d+)m\s+ago$/);
    if (m) return m[1] + ' 分钟前';
    m = String(text).match(/^(\d+)h\s+ago$/);
    if (m) return m[1] + ' 小时前';
    m = String(text).match(/^(\d+)d\s+ago$/);
    if (m) return m[1] + ' 天前';

    if (String(text).startsWith('Daily at ')) {
      return '每天 ' + String(text).slice('Daily at '.length);
    }
    if (String(text).startsWith('Activated: ')) {
      return '已激活：' + String(text).slice('Activated: '.length);
    }
    if (String(text).startsWith('Run: ')) {
      return '运行：' + String(text).slice('Run: '.length);
    }
    if (String(text).startsWith('Env: ')) {
      return '环境变量：' + String(text).slice('Env: '.length);
    }
    if (String(text).startsWith('Requires: ')) {
      return '依赖：' + String(text).slice('Requires: '.length);
    }
    if (String(text).startsWith('Activate ')) {
      return '激活 ' + String(text).slice('Activate '.length);
    }
    if (String(text).startsWith('Configure ')) {
      return '配置 ' + String(text).slice('Configure '.length);
    }
    if (String(text).startsWith('Set up ')) {
      return '设置 ' + String(text).slice('Set up '.length);
    }
    if (String(text).startsWith('Will use ')) {
      return '将使用 ' + String(text).slice('Will use '.length);
    }
    m = String(text).match(/^Or set\s+(.+)\s+in your environment$/);
    if (m) return '或在环境变量中设置 ' + m[1];
    m = String(text).match(/^(.+)\s+is ready!$/);
    if (m) return m[1] + ' 已就绪！';

    if (String(text).indexOf('Create cron-based scheduled jobs that send messages to agents on a recurring schedule.') >= 0 ||
        String(text).indexOf('Use cron expressions like') >= 0 ||
        String(text).indexOf('(every 5 min) or') >= 0 ||
        String(text).indexOf('(weekdays at 9am). You can also run any job manually with the "Run Now" button.') >= 0) {
      return String(text)
        .replace('Create cron-based scheduled jobs that send messages to agents on a recurring schedule.', '创建基于 Cron 的定时任务，按计划周期性向代理发送消息。')
        .replace('Use cron expressions like', '使用 Cron 表达式，例如')
        .replace('(every 5 min) or', '（每 5 分钟）或')
        .replace('(weekdays at 9am). You can also run any job manually with the "Run Now" button.', '（工作日早上 9 点）。你也可以通过 “立即运行” 按钮手动执行任务。');
    }

    m = String(text).match(/^(\d+)\s+model\(s\)\s+available$/);
    if (m) return m[1] + ' 个模型可用';
    m = String(text).match(/^(\d+)\s+models$/);
    if (m) return m[1] + ' 个模型';
    m = String(text).match(/^(\d+)\/(\d+)\s+configured$/);
    if (m) return '已配置 ' + m[1] + '/' + m[2];
    m = String(text).match(/^(\d+)\s+of\s+(\d+)\s+steps completed$/);
    if (m) return '已完成 ' + m[1] + ' / ' + m[2] + ' 步';
    m = String(text).match(/^(\d+)\s+of\s+(\d+)\s+models$/);
    if (m) return m[1] + ' / ' + m[2] + ' 个模型';
    m = String(text).match(/^of\s+(.+)$/);
    if (m) return '上限：' + (m[1] === 'unlimited' ? '无限制' : m[1]);
    m = String(text).match(/^(\d+)\s+pending$/);
    if (m) return m[1] + ' 待处理';
    m = String(text).match(/^Show advanced\s+\((\d+)\)$/);
    if (m) return '显示高级选项（' + m[1] + '）';
    m = String(text).match(/^(\d+)\s+channel\(s\)\s+connected$/);
    if (m) return '已连接 ' + m[1] + ' 个渠道';
    m = String(text).match(/^(\d+)\s+tool\(s\)\s+available$/);
    if (m) return m[1] + ' 个工具可用';
    m = String(text).match(/^(\d+)\s+result\(s\)\s+for\s+"([^"]+)"$/);
    if (m) return '“' + m[2] + '”共有 ' + m[1] + ' 条结果';
    m = String(text).match(/^(\d+)\s+downloads$/);
    if (m) return m[1] + ' 次下载';
    m = String(text).match(/^(\d+)\s+stars$/);
    if (m) return m[1] + ' 星标';
    m = String(text).match(/^(\d+)\s+defense-in-depth systems active$/);
    if (m) return '已启用 ' + m[1] + ' 个纵深防护系统';
    m = String(text).match(/^(\d+)\s+tool\(s\)$/);
    if (m) return m[1] + ' 个工具';
    m = String(text).match(/^(\d+)\s+metric\(s\)$/);
    if (m) return m[1] + ' 个指标';
    m = String(text).match(/^(\d+)\s+tool\(s\)\s+(\d+)\s+metric\(s\)$/);
    if (m) return m[1] + ' 个工具 / ' + m[2] + ' 个指标';
    m = String(text).match(/^(\d+)\s+steps,\s+(\d+)\s+connections$/);
    if (m) return m[1] + ' 个步骤，' + m[2] + ' 条连接';
    m = String(text).match(/^(\d+)\/(\d+)\s+active$/);
    if (m) return m[1] + '/' + m[2] + ' 已启用';
    m = String(text).match(/^(\d+)\s+of\s+(\d+)\s+ready$/);
    if (m) return m[1] + ' / ' + m[2] + ' 已就绪';
    m = String(text).match(/^Enter\s+([A-Z0-9_]+)$/);
    if (m) return '输入 ' + m[1];
    m = String(text).match(/^Enter\s+([A-Z0-9_]+)\.\.\.$/);
    if (m) return '输入 ' + m[1] + '...';
    m = String(text).match(/^(.*)\s+at\s+(\d{1,2}:\d{2}\s+[AP]M)$/);
    if (m) return m[1] + ' ' + m[2];

    m = String(text).match(/^Delete\s+"([^"]+)"\?\s+This cannot be undone\.$/);
    if (m) return '删除“' + m[1] + '”？此操作无法撤销。';

    m = String(text).match(/^Delete key\s+"([^"]+)"\?\s+This cannot be undone\.$/);
    if (m) return '删除密钥“' + m[1] + '”？此操作无法撤销。';

    if (String(text) === 'Cannot connect to daemon — is openfang running?') {
      return '无法连接到守护进程 — openfang 是否正在运行？';
    }
    if (String(text) === 'Cannot reach daemon — is openfang running?') {
      return '无法连接到守护进程 — openfang 是否正在运行？';
    }
    m = String(text).match(/^(.+)\s+—\s+not configured$/);
    if (m) return m[1] + ' — 未配置';
    m = String(text).match(/^(.+)\s+—\s+ready$/);
    if (m) return m[1] + ' — 已就绪';
    m = String(text).match(/^(\d+)s\s+ago$/);
    if (m) return m[1] + ' 秒前';
    m = String(text).match(/^(All|Messaging|Social|Enterprise|Developer|Notifications)\s+\((\d+)\/(\d+)\)$/);
    if (m) {
      var catMap = {
        'All': '全部',
        'Messaging': '消息',
        'Social': '社交',
        'Enterprise': '企业',
        'Developer': '开发者',
        'Notifications': '通知'
      };
      return catMap[m[1]] + '（' + m[2] + '/' + m[3] + '）';
    }
    m = String(text).match(/^(Easy|Medium|Hard)\s+·\s+~(\d+)\s+min$/);
    if (m) {
      var diffMap = { 'Easy': '简单', 'Medium': '中等', 'Hard': '困难' };
      return diffMap[m[1]] + ' · 约 ' + m[2] + ' 分钟';
    }
    m = String(text).match(/^max\s+(\d+)\s+iters$/);
    if (m) return '最多 ' + m[1] + ' 次迭代';
    m = String(text).match(/^(\d+)\s+branches$/);
    if (m) return m[1] + ' 个分支';
    m = String(text).match(/^\.\.\.\s+and\s+(\d+)\s+more$/);
    if (m) return '……以及另外 ' + m[1] + ' 项';
    m = String(text).match(/^Agent "([^"]+)" created successfully$/);
    if (m) return '代理“' + m[1] + '”创建成功';
    m = String(text).match(/^Skill "([^"]+)" created$/);
    if (m) return '技能“' + m[1] + '”已创建';
    m = String(text).match(/^Skill "([^"]+)" uninstalled$/);
    if (m) return '技能“' + m[1] + '”已卸载';
    m = String(text).match(/^Workflow "([^"]+)" created$/);
    if (m) return '工作流“' + m[1] + '”已创建';
    m = String(text).match(/^Failed to create workflow:\s*(.+)$/);
    if (m) return '创建工作流失败：' + m[1];
    m = String(text).match(/^Workflow failed:\s*(.+)$/);
    if (m) return '工作流执行失败：' + m[1];
    m = String(text).match(/^Install failed:\s*(.+)$/);
    if (m) return '安装失败：' + m[1];
    m = String(text).match(/^Failed to uninstall skill:\s*(.+)$/);
    if (m) return '卸载技能失败：' + m[1];
    m = String(text).match(/^Failed to create skill:\s*(.+)$/);
    if (m) return '创建技能失败：' + m[1];
    m = String(text).match(/^API key saved for\s+(.+)$/);
    if (m) return m[1] + ' 的 API Key 已保存';
    m = String(text).match(/^API key removed for\s+(.+)$/);
    if (m) return m[1] + ' 的 API Key 已移除';
    m = String(text).match(/^Saved\s+(.+)$/);
    if (m) return '已保存 ' + m[1];
    m = String(text).match(/^Connected peers:\s*(\d+)\s*\/\s*(\d+)$/);
    if (m) return '已连接节点：' + m[1] + ' / ' + m[2];
    m = String(text).match(/^(.+)\s+configured and activated\.$/);
    if (m) return m[1] + ' 已配置并启用。';
    m = String(text).match(/^([A-Z0-9_]+)\s+is set$/);
    if (m) return m[1] + ' 已设置';
    if (String(text) === 'unlimited') {
      return '无限制';
    }
    if (String(text) === '0 = unlimited') {
      return '0 = 无限制';
    }
    m = String(text).match(/^(.+)\s+\(comma-separated\)$/);
    if (m) return m[1] + '（用逗号分隔）';
    m = String(text).match(/^Status:\s*(Enabled|Disabled)$/);
    if (m) return '状态：' + (m[1] === 'Enabled' ? '已启用' : '已禁用');
    m = String(text).match(/^Algorithm:\s*([^|]+)\s*\|\s*(\d+)\s+tokens\/min per IP$/);
    if (m) return '算法：' + m[1].trim() + ' | 每 IP 每分钟 ' + m[2] + ' 个令牌';
    m = String(text).match(/^Max\s+(\d+)\s+conn\/IP\s*\|\s*(\d+)min idle timeout\s*\|\s*(\d+)KB max msg$/);
    if (m) return '每 IP 最多 ' + m[1] + ' 个连接 | 空闲超时 ' + m[2] + ' 分钟 | 最大消息 ' + m[3] + 'KB';
    m = String(text).match(/^Fuel:\s*(ON|OFF)\s*\|\s*Epoch:\s*(ON|OFF)\s*\|\s*Timeout:\s*(\d+)s$/);
    if (m) return 'Fuel：' + (m[1] === 'ON' ? '开启' : '关闭') + ' | Epoch：' + (m[2] === 'ON' ? '开启' : '关闭') + ' | 超时：' + m[3] + ' 秒';
    m = String(text).match(/^Mode:\s*([^(|]+)\s*(\((?:key configured|no key set)\))?$/);
    if (m) {
      var authTail = '';
      if (m[2] === '(key configured)') authTail = '（已配置密钥）';
      if (m[2] === '(no key set)') authTail = '（未设置密钥）';
      var modeLabel = m[1].trim();
      if (modeLabel === 'localhost_only') modeLabel = '仅 localhost';
      return '模式：' + modeLabel + authTail;
    }
    m = String(text).match(/^(Active|Disabled)\s*\|\s*([^|]+)\s*\|\s*(\d+)\s+entries logged$/);
    if (m) {
      var auditLabel = m[2].trim();
      if (auditLabel === 'SHA-256 Merkle Chain') auditLabel = 'SHA-256 Merkle 链';
      return (m[1] === 'Active' ? '启用' : '禁用') + ' | ' + auditLabel + ' | 已记录 ' + m[3] + ' 条';
    }
    m = String(text).match(/^(Active|Disabled)\s*\|\s*Tracking:\s*(.+)$/);
    if (m) return (m[1] === 'Active' ? '启用' : '禁用') + ' | 跟踪：' + m[2];
    m = String(text).match(/^Algorithm:\s*([^|]+)\s*\|\s*(Available|Not available)$/);
    if (m) return '算法：' + m[1].trim() + ' | ' + (m[2] === 'Available' ? '可用' : '不可用');
    m = String(text).match(/^Remove\s+(.+)\s+configuration\?\s+This will deactivate the channel\.$/);
    if (m) return '移除 ' + m[1] + ' 的配置？这会停用该渠道。';
    m = String(text).match(/^Failed to load run history:\s*(.+)$/);
    if (m) return '加载运行历史失败：' + m[1];
    m = String(text).match(/^Audit chain verified\s+—\s+(\d+)\s+entries valid$/);
    if (m) return '审计链验证通过 — ' + m[1] + ' 条记录有效';
    m = String(text).match(/^Chain verification failed:\s*(.+)$/);
    if (m) return '验证链失败：' + m[1];
    m = String(text).match(/^CHAIN VALID\s+—\s+(\d+)\s+entries verified$/);
    if (m) return '链路有效 — 已验证 ' + m[1] + ' 条记录';
    m = String(text).match(/^CHAIN BROKEN\s+—\s+(.+)$/);
    if (m) return '链路损坏 — ' + m[1];

    return text;
  }

  function translateTextForLocale(text, locale) {
    var s;
    var trimmed;
    var normalized;
    var translated;
    var patterned;
    if (text === null || text === undefined) return text;
    s = String(text);
    trimmed = s.trim();
    if (!trimmed) return s;
    if (locale !== 'zh-CN') return s;
    normalized = trimmed.replace(/\s+/g, ' ');
    translated = translateExactZh(trimmed);
    if (translated === trimmed && normalized !== trimmed) {
      translated = translateExactZh(normalized);
    }
    translated = translatePatternsZh(translated);
    if (translated !== trimmed) {
      return s.replace(trimmed, translated);
    }
    patterned = translatePatternsZh(trimmed);
    if (patterned === trimmed && normalized !== trimmed) {
      patterned = translatePatternsZh(normalized);
    }
    if (patterned !== trimmed) return s.replace(trimmed, patterned);
    return s;
  }

  function translateText(text) {
    return translateTextForLocale(text, _locale);
  }

  function hasOwn(obj, key) {
    return Object.prototype.hasOwnProperty.call(obj, key);
  }

  function hasAnyOwn(obj) {
    var k;
    if (!obj) return false;
    for (k in obj) {
      if (hasOwn(obj, k)) return true;
    }
    return false;
  }

  function shouldSkipNode(node) {
    if (!node) return true;
    var p = node.parentElement;
    if (!p) return false;
    if (p.closest && p.closest('[data-no-i18n]')) return true;
    var tag = (p.tagName || '').toUpperCase();
    if (tag === 'SCRIPT' || tag === 'STYLE') return true;
    if (tag === 'CODE' || tag === 'PRE' || tag === 'KBD' || tag === 'SAMP') return true;
    return false;
  }

  function translateAttributes(el) {
    var attrs;
    var i;
    var attr;
    var v;
    var tv;
    var store;
    var restored;
    if (!el || !el.getAttribute) return;
    attrs = ['title', 'placeholder', 'aria-label'];

    if (_locale === 'zh-CN') {
      for (i = 0; i < attrs.length; i++) {
        attr = attrs[i];
        v = el.getAttribute(attr);
        if (!v) continue;
        if (_attrOriginal) {
          store = _attrOriginal.get(el);
          if (!store) {
            store = {};
            _attrOriginal.set(el, store);
          }
          if (!hasOwn(store, attr)) {
            store[attr] = v;
          }
          tv = translateTextForLocale(store[attr], 'zh-CN');
        } else {
          tv = translateTextForLocale(v, 'zh-CN');
        }
        if (tv !== v) el.setAttribute(attr, tv);
      }
      return;
    }

    if (!_attrOriginal) return;
    store = _attrOriginal.get(el);
    if (!store) return;

    restored = false;
    for (i = 0; i < attrs.length; i++) {
      attr = attrs[i];
      if (!hasOwn(store, attr)) continue;
      if (store[attr] === null || store[attr] === undefined) {
        el.removeAttribute(attr);
      } else {
        el.setAttribute(attr, store[attr]);
      }
      delete store[attr];
      restored = true;
    }
    if (restored && !hasAnyOwn(store)) {
      _attrOriginal.delete(el);
    }
  }

  function translateTextNode(node) {
    var v;
    var entry;
    var source;
    var translated;
    if (!node) return;
    v = node.nodeValue;

    if (_locale === 'zh-CN') {
      if (!v || !v.trim()) return;
      source = v;
      if (_textOriginal) {
        entry = _textOriginal.get(node);
        if (entry && v === entry.translated) {
          source = entry.original;
        }
      }
      translated = translateTextForLocale(source, 'zh-CN');
      if (translated !== source) {
        if (_textOriginal) {
          _textOriginal.set(node, { original: source, translated: translated });
        }
        if (translated !== v) node.nodeValue = translated;
      } else if (_textOriginal) {
        _textOriginal.delete(node);
      }
      return;
    }

    if (!_textOriginal) return;
    entry = _textOriginal.get(node);
    if (!entry) return;
    if (v !== entry.original) node.nodeValue = entry.original;
    _textOriginal.delete(node);
  }

  function apply(root) {
    if (!root) return;

    if (root.nodeType === 1) translateAttributes(root);

    var n;
    var v;
    var tv;
    var walker = document.createTreeWalker(
      root,
      NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT,
      {
        acceptNode: function(node) {
          if (node.nodeType === 3) {
            if (shouldSkipNode(node)) return NodeFilter.FILTER_REJECT;
            if (!node.nodeValue || !node.nodeValue.trim()) return NodeFilter.FILTER_REJECT;
            return NodeFilter.FILTER_ACCEPT;
          }
          if (node.nodeType === 1) {
            return NodeFilter.FILTER_ACCEPT;
          }
          return NodeFilter.FILTER_REJECT;
        }
      }
    );

    n = walker.nextNode();
    while (n) {
      if (n.nodeType === 1) {
        translateAttributes(n);
      } else if (n.nodeType === 3) {
        translateTextNode(n);
      }
      n = walker.nextNode();
    }
  }

  var _observer = null;
  var _pending = false;
  function scheduleApply() {
    if (_pending) return;
    _pending = true;
    setTimeout(function() {
      _pending = false;
      try { apply(document.body); } catch (e) {}
    }, 0);
  }

  function installObserver() {
    if (_observer) return;
    if (typeof MutationObserver === 'undefined') return;
    _observer = new MutationObserver(function(mutations) {
      var i;
      var m;
      var j;
      var node;
      if (_locale === 'en') return;
      for (i = 0; i < mutations.length; i++) {
        m = mutations[i];
        if (m.type === 'childList') {
          for (j = 0; j < m.addedNodes.length; j++) {
            node = m.addedNodes[j];
            if (node && node.nodeType === 1) scheduleApply(node);
            if (node && node.nodeType === 3) scheduleApply(node.parentNode);
          }
        } else if (m.type === 'characterData') {
          scheduleApply(m.target && m.target.parentNode);
        }
      }
    });

    _observer.observe(document.body, { childList: true, subtree: true, characterData: true });
  }

  function init() {
    setLocale(_locale);
    try { apply(document.body); } catch (e) {}
    installObserver();
  }

  window.OpenFangI18n = {
    init: init,
    setLocale: function(loc) { setLocale(loc); scheduleApply(document.body); },
    getLocale: getLocale,
    intlLocale: function() { return _locale === 'zh-CN' ? 'zh-CN' : 'en-US'; },
    translateText: translateText,
    apply: apply,
    supported: function() { return SUPPORTED.slice(); }
  };

  try { init(); } catch (e) {}
})();
