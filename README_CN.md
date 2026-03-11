<p align="center">
  <img src="public/assets/openfang-logo.png" width="160" alt="OpenFang Logo" />
</p>

<h1 align="center">OpenFang</h1>
<h3 align="center">智能体操作系统</h3>

<p align="center">
  用 Rust 构建的开源智能体操作系统。137K 行代码。14 个 crate。1,767+ 测试用例。零 clippy 警告。<br/>
  <strong>单一二进制文件。久经考验。真正为你工作的智能体。</strong>
</p>

<p align="center">
  <a href="https://openfang.sh/docs">文档</a> &bull;
  <a href="https://openfang.sh/docs/getting-started">快速开始</a> &bull;
  <a href="https://x.com/openfangg">Twitter / X</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/language-Rust-orange?style=flat-square" alt="Rust" />
  <img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" alt="MIT" />
  <img src="https://img.shields.io/badge/version-0.3.30-green?style=flat-square" alt="v0.3.30" />
  <img src="https://img.shields.io/badge/tests-1,767%2B%20passing-brightgreen?style=flat-square" alt="Tests" />
  <img src="https://img.shields.io/badge/clippy-0%20warnings-brightgreen?style=flat-square" alt="Clippy" />
  <a href="https://www.buymeacoffee.com/openfang" target="_blank"><img src="https://img.shields.io/badge/Buy%20Me%20a%20Coffee-FFDD00?style=flat-square&logo=buy-me-a-coffee&logoColor=black" alt="Buy Me A Coffee" /></a>
</p>

---

> **v0.3.30 — 安全加固版本（2026 年 3 月）**
>
> OpenFang 功能已完备，但仍处于 1.0 之前的阶段。你可能会遇到一些粗糙的地方或小版本之间的破坏性变更。我们快速发布，快速修复。在 v1.0 之前，建议生产环境锁定到特定 commit。[在此报告问题。](https://github.com/RightNow-AI/openfang/issues)

---

## 什么是 OpenFang？

OpenFang 是一个**开源智能体操作系统** — 不是聊天机器人框架，不是 LLM 的 Python 封装，不是"多智能体编排器"。它是一个从头用 Rust 构建的完整自主智能体操作系统。

传统智能体框架等着你输入内容。OpenFang 运行的是**为你工作的自主智能体** — 按计划、全天候运行，构建知识图谱、监控目标、生成线索、管理你的社交媒体，并将结果报告到你的仪表板。

整个系统编译成**单一约 32MB 的二进制文件**。一次安装，一条命令，你的智能体就上线了。

```bash
curl -fsSL https://openfang.sh/install | sh
openfang init
openfang start
# 仪表板运行在 http://localhost:4200
```

<details>
<summary><strong>Windows</strong></summary>

```powershell
irm https://openfang.sh/install.ps1 | iex
openfang init
openfang start
```

</details>

---

## 执行器：真正做事的智能体

<p align="center"><em>"传统智能体等你输入。执行器<strong>为</strong>你工作。"</em></p>

**执行器（Hands）** 是 OpenFang 的核心创新 — 预构建的自主能力包，独立运行，按计划执行，无需你提示。这不是聊天机器人。这是一个会在早上 6 点醒来、研究你的竞争对手、构建知识图谱、对发现进行评分、并在你喝咖啡之前将报告发送到你的 Telegram 的智能体。

每个执行器包含：
- **HAND.toml** — 声明工具、设置、要求和仪表板指标的清单
- **系统提示词** — 多阶段操作手册（不是一句话 — 这些是 500+ 字的专家流程）
- **SKILL.md** — 运行时注入上下文的领域专业知识参考
- **安全门控** — 敏感操作的审批门控（例如，浏览器执行器在任何购买前都需要审批）

全部编译进二进制文件。无需下载，无需 pip install，无需 Docker pull。

### 7 个内置执行器

| 执行器 | 功能说明 |
|--------|---------|
| **Clip（剪辑器）** | 获取 YouTube URL，下载视频，识别精彩时刻，剪辑成带字幕和缩略图的竖版短视频，可选添加 AI 配音，并发布到 Telegram 和 WhatsApp。8 阶段流水线。FFmpeg + yt-dlp + 5 个 STT 后端。 |
| **Lead（线索生成器）** | 每日运行。发现符合你理想客户画像的潜在客户，通过网络研究丰富信息，评分 0-100，与现有数据库去重，以 CSV/JSON/Markdown 格式交付合格线索。持续构建 ICP 画像。 |
| **Collector（情报收集器）** | OSINT 级情报。你给它一个目标（公司、人物、主题）。它持续监控 — 变化检测、情感追踪、知识图谱构建，以及重要变化时的关键警报。 |
| **Predictor（预测器）** | 超级预测引擎。从多个来源收集信号，构建校准推理链，做出带置信区间的预测，并使用 Brier 分数跟踪自身准确性。有逆向模式，故意反驳共识。 |
| **Researcher（研究员）** | 深度自主研究员。交叉参考多个来源，使用 CRAAP 标准（时效性、相关性、权威性、准确性、目的）评估可信度，生成带 APA 格式引用的报告，支持多语言。 |
| **Twitter（Twitter 管理器）** | 自主 Twitter/X 账户管理器。以 7 种轮换格式创建内容，安排最佳互动时间发布，回复提及，跟踪性能指标。有审批队列 — 未经你同意不会发布任何内容。 |
| **Browser（浏览器）** | 网页自动化智能体。导航网站，填写表单，点击按钮，处理多步骤工作流。使用带会话持久化的 Playwright 桥接。**强制购买审批门控** — 未经明确确认永远不会花你的钱。 |

```bash
# 激活研究员执行器 — 它立即开始工作
openfang hand activate researcher

# 随时检查进度
openfang hand status researcher

# 按每日计划激活线索生成
openfang hand activate lead

# 暂停但不丢失状态
openfang hand pause lead

# 查看所有可用执行器
openfang hand list
```

**构建你自己的执行器。** 定义一个包含工具、设置和系统提示词的 `HAND.toml`。发布到 FangHub。

---

## OpenFang 与其他框架对比

<p align="center">
  <img src="public/assets/openfang-vs-claws.png" width="600" alt="OpenFang vs OpenClaw vs ZeroClaw" />
</p>

### 基准测试：实测数据，非营销宣传

所有数据来自官方文档和公开仓库 — 2026 年 2 月。

#### 冷启动时间（越低越好）

```
ZeroClaw   ██░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   10 ms
OpenFang   ██████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  180 ms    ★
LangGraph  █████████████████░░░░░░░░░░░░░░░░░░░░░░░░░  2.5 sec
CrewAI     ████████████████████░░░░░░░░░░░░░░░░░░░░░░  3.0 sec
AutoGen    ██████████████████████████░░░░░░░░░░░░░░░░░  4.0 sec
OpenClaw   █████████████████████████████████████████░░  5.98 sec
```

#### 空闲内存使用（越低越好）

```
ZeroClaw   █░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    5 MB
OpenFang   ████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   40 MB    ★
LangGraph  ██████████████████░░░░░░░░░░░░░░░░░░░░░░░░░  180 MB
CrewAI     ████████████████████░░░░░░░░░░░░░░░░░░░░░░░  200 MB
AutoGen    █████████████████████████░░░░░░░░░░░░░░░░░░  250 MB
OpenClaw   ████████████████████████████████████████░░░░  394 MB
```

#### 安装大小（越低越好）

```
ZeroClaw   █░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  8.8 MB
OpenFang   ███░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   32 MB    ★
CrewAI     ████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  100 MB
LangGraph  ████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  150 MB
AutoGen    ████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░  200 MB
OpenClaw   ████████████████████████████████████████░░░░  500 MB
```

#### 安全系统（越高越好）

```
OpenFang   ████████████████████████████████████████████   16      ★
ZeroClaw   ███████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░    6
OpenClaw   ████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    3
AutoGen    █████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    2
LangGraph  █████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    2
CrewAI     ███░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    1
```

#### 频道适配器（越高越好）

```
OpenFang   ████████████████████████████████████████████   40      ★
ZeroClaw   ███████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░   15
OpenClaw   █████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   13
CrewAI     ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    0
AutoGen    ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    0
LangGraph  ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    0
```

#### LLM 提供商（越高越好）

```
ZeroClaw   ████████████████████████████████████████████   28
OpenFang   ██████████████████████████████████████████░░   27      ★
LangGraph  ██████████████████████░░░░░░░░░░░░░░░░░░░░░   15
CrewAI     ██████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   10
OpenClaw   ██████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   10
AutoGen    ███████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    8
```

### 功能逐项对比

| 功能 | OpenFang | OpenClaw | ZeroClaw | CrewAI | AutoGen | LangGraph |
|------|----------|----------|----------|--------|---------|-----------|
| **语言** | **Rust** | TypeScript | **Rust** | Python | Python | Python |
| **自主执行器** | **7 个内置** | 无 | 无 | 无 | 无 | 无 |
| **安全层** | **16 层独立** | 3 层基础 | 6 层 | 1 层基础 | Docker | AES 加密 |
| **智能体沙盒** | **WASM 双重计量** | 无 | 允许列表 | 无 | Docker | 无 |
| **频道适配器** | **40** | 13 | 15 | 0 | 0 | 0 |
| **内置工具** | **53 + MCP + A2A** | 50+ | 12 | 插件 | MCP | LC 工具 |
| **记忆** | **SQLite + 向量** | 文件 | SQLite FTS5 | 4 层 | 外部 | 检查点 |
| **桌面应用** | **Tauri 2.0** | 无 | 无 | 无 | Studio | 无 |
| **审计跟踪** | **默克尔哈希链** | 日志 | 日志 | 追踪 | 日志 | 检查点 |
| **冷启动** | **<200ms** | ~6s | ~10ms | ~3s | ~4s | ~2.5s |
| **安装大小** | **~32 MB** | ~500 MB | ~8.8 MB | ~100 MB | ~200 MB | ~150 MB |
| **许可证** | MIT | MIT | MIT | MIT | Apache 2.0 | MIT |

---

## 16 层安全系统 — 深度防御

OpenFang 不是事后才加上安全措施。每一层都可独立测试，运行时不存在单点故障。

| # | 系统 | 功能说明 |
|---|------|---------|
| 1 | **WASM 双重计量沙盒** | 工具代码在 WebAssembly 中运行，带燃料计量 + 周期中断。看门狗线程会杀死失控代码。 |
| 2 | **默克尔哈希链审计跟踪** | 每个操作都与前一个加密链接。篡改一条记录，整个链条就会断裂。 |
| 3 | **信息流污点追踪** | 标签在执行过程中传播 — 从源头到终点追踪密钥。 |
| 4 | **Ed25519 签名智能体清单** | 每个智能体身份和能力集都经过加密签名。 |
| 5 | **SSRF 防护** | 阻止私有 IP、云元数据端点和 DNS 重绑定攻击。 |
| 6 | **密钥清零** | `Zeroizing<String>` 在 API 密钥不再需要时立即从内存中清除。 |
| 7 | **OFP 双向认证** | 基于 HMAC-SHA256 nonce、常数时间验证的 P2P 网络认证。 |
| 8 | **能力门控** | 基于角色的访问控制 — 智能体声明所需工具，内核强制执行。 |
| 9 | **安全头** | 每个响应都带有 CSP、X-Frame-Options、HSTS、X-Content-Type-Options。 |
| 10 | **健康端点脱敏** | 公开健康检查返回最少信息。完整诊断需要认证。 |
| 11 | **子进程沙盒** | `env_clear()` + 选择性变量透传。跨平台进程树隔离与终止。 |
| 12 | **提示注入扫描器** | 检测覆盖尝试、数据外泄模式和技能中的 shell 引用注入。 |
| 13 | **循环保护** | 基于 SHA256 的工具调用循环检测，带断路器。处理乒乓模式。 |
| 14 | **会话修复** | 7 阶段消息历史验证和损坏自动恢复。 |
| 15 | **路径遍历防护** | 带符号链接逃逸防护的规范化。`../` 在这里不起作用。 |
| 16 | **GCRA 速率限制器** | 成本感知的令牌桶速率限制，带每 IP 跟踪和过期清理。 |

---

## 架构

14 个 Rust crate。137,728 行代码。模块化内核设计。

```
openfang-kernel      编排、工作流、计量、RBAC、调度器、预算跟踪
openfang-runtime     智能体循环、3 个 LLM 驱动、53 个工具、WASM 沙盒、MCP、A2A
openfang-api         140+ REST/WS/SSE 端点、OpenAI 兼容 API、仪表板
openfang-channels    40 个消息适配器，带速率限制、私聊/群聊策略
openfang-memory      SQLite 持久化、向量嵌入、规范会话、压缩
openfang-types       核心类型、污点追踪、Ed25519 清单签名、模型目录
openfang-skills      60 个内置技能、SKILL.md 解析器、FangHub 市场
openfang-hands       7 个自主执行器、HAND.toml 解析器、生命周期管理
openfang-extensions  25 个 MCP 模板、AES-256-GCM 凭据保险库、OAuth2 PKCE
openfang-wire        OFP P2P 协议，带 HMAC-SHA256 双向认证
openfang-cli         CLI，带守护进程管理、TUI 仪表板、MCP 服务器模式
openfang-desktop     Tauri 2.0 原生应用（系统托盘、通知、全局快捷键）
openfang-migrate     OpenClaw、LangChain、AutoGPT 迁移引擎
xtask                构建自动化
```

---

## 40 个频道适配器

将你的智能体连接到用户所在的每个平台。

**核心：** Telegram、Discord、Slack、WhatsApp、Signal、Matrix、邮件（IMAP/SMTP）
**企业：** Microsoft Teams、Mattermost、Google Chat、Webex、飞书/Lark、Zulip
**社交：** LINE、Viber、Facebook Messenger、Mastodon、Bluesky、Reddit、LinkedIn、Twitch
**社区：** IRC、XMPP、Guilded、Revolt、Keybase、Discourse、Gitter
**隐私：** Threema、Nostr、Mumble、Nextcloud Talk、Rocket.Chat、Ntfy、Gotify
**办公：** Pumble、Flock、Twist、钉钉、Zalo、Webhooks

每个适配器支持每频道模型覆盖、私聊/群聊策略、速率限制和输出格式化。

---

## 27 个 LLM 提供商 — 123+ 模型

3 个原生驱动（Anthropic、Gemini、OpenAI 兼容）路由到 27 个提供商：

Anthropic、Gemini、OpenAI、Groq、DeepSeek、OpenRouter、Together、Mistral、Fireworks、Cohere、Perplexity、xAI、AI21、Cerebras、SambaNova、HuggingFace、Replicate、Ollama、vLLM、LM Studio、通义千问、MiniMax、智谱、月之暗面、千帆、Bedrock 等。

智能路由，带任务复杂度评分、自动回退、成本跟踪和每模型定价。

---

## 从 OpenClaw 迁移

已经在运行 OpenClaw？一条命令：

```bash
# 迁移所有内容 — 智能体、记忆、技能、配置
openfang migrate --from openclaw

# 从指定路径迁移
openfang migrate --from openclaw --path ~/.openclaw

# 先模拟运行看看会有什么变化
openfang migrate --from openclaw --dry-run
```

迁移引擎导入你的智能体、对话历史、技能和配置。OpenFang 原生读取 SKILL.md，与 ClawHub 市场兼容。

---

## OpenAI 兼容 API

直接替换。将你现有的工具指向 OpenFang：

```bash
curl -X POST localhost:4200/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "researcher",
    "messages": [{"role": "user", "content": "分析第四季度市场趋势"}],
    "stream": true
  }'
```

140+ REST/WS/SSE 端点，涵盖智能体、记忆、工作流、频道、模型、技能、A2A、执行器等。

---

## 快速开始

```bash
# 1. 安装（macOS/Linux）
curl -fsSL https://openfang.sh/install | sh

# 2. 初始化 — 引导你完成提供商设置
openfang init

# 3. 启动守护进程
openfang start

# 4. 仪表板运行在 http://localhost:4200

# 5. 激活执行器 — 它开始为你工作
openfang hand activate researcher

# 6. 与智能体对话
openfang chat researcher
> "AI 智能体框架的新兴趋势是什么？"

# 7. 创建预构建智能体
openfang agent spawn coder
```

<details>
<summary><strong>Windows (PowerShell)</strong></summary>

```powershell
irm https://openfang.sh/install.ps1 | iex
openfang init
openfang start
```

</details>

---

## 开发

```bash
# 构建工作区
cargo build --workspace --lib

# 运行所有测试（1,767+）
cargo test --workspace

# 检查（必须 0 警告）
cargo clippy --workspace --all-targets -- -D warnings

# 格式化
cargo fmt --all -- --check
```

---

## 稳定性说明

OpenFang v0.3.30 处于 1.0 之前。架构稳固，测试套件完善，安全模型全面。话虽如此：

- v1.0 之前，小版本之间可能会有**破坏性变更**
- **部分执行器**比其他更成熟（浏览器和研究员最经得起考验）
- **边缘情况**存在 — 如果你发现了，[提交 issue](https://github.com/RightNow-AI/openfang/issues)
- v1.0 之前，生产部署建议**锁定到特定 commit**

我们快速发布，快速修复。目标是在 2026 年中期发布坚如磐石的 v1.0。

---

## 安全

报告安全漏洞请发邮件至 **jaber@rightnowai.co**。我们认真对待所有报告，将在 48 小时内响应。

---

## 许可证

MIT — 随意使用。

---

## 链接

- [网站与文档](https://openfang.sh)
- [快速开始指南](https://openfang.sh/docs/getting-started)
- [GitHub](https://github.com/RightNow-AI/openfang)
- [Discord](https://discord.gg/sSJqgNnq6X)
- [Twitter / X](https://x.com/openfangg)

---

## 由 RightNow 构建

<p align="center">
  <a href="https://www.rightnowai.co/">
    <img src="public/assets/rightnow-logo.webp" width="60" alt="RightNow Logo" />
  </a>
</p>

<p align="center">
  OpenFang 由 <a href="https://www.rightnowai.co/"><strong>RightNow</strong></a> 创始人 <a href="https://x.com/Akashi203"><strong>Jaber</strong></a> 构建和维护。
</p>

<p align="center">
  <a href="https://www.rightnowai.co/">网站</a> &bull;
  <a href="https://x.com/Akashi203">Twitter / X</a> &bull;
  <a href="https://www.buymeacoffee.com/openfang" target="_blank"><img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me A Coffee" style="height: 60px !important;width: 217px !important;" ></a>
</p>

---

<p align="center">
  <strong>用 Rust 构建。16 层安全防护。真正为你工作的智能体。</strong>
</p>
