#!/bin/bash
# Telegram Local API 快速配置脚本

set -e

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "=== Telegram Local API 配置向导 ==="
echo ""

# 检查二进制文件
if [ ! -x "$HOME/.openfang/bin/telegram-bot-api" ]; then
    echo "❌ 错误：telegram-bot-api 二进制文件未找到或不可执行"
    echo "   位置：$HOME/.openfang/bin/telegram-bot-api"
    echo ""
    echo "请先从仓库内 third_party 源码安装："
    echo "  ./scripts/install-telegram-local-api.sh"
    echo ""
    echo "如果 third_party 子模块还没拉下来，先执行："
    echo "  git submodule update --init --recursive third_party/telegram-bot-api"
    exit 1
fi

echo "✅ 找到 telegram-bot-api 二进制文件"
echo ""

# 检查环境变量
if [ -z "$TELEGRAM_BOT_TOKEN" ]; then
    echo "⚠️  警告：TELEGRAM_BOT_TOKEN 环境变量未设置"
    echo "   请设置：export TELEGRAM_BOT_TOKEN='你的bot_token'"
    echo ""
fi

if [ -z "$TELEGRAM_API_HASH" ]; then
    echo "⚠️  警告：TELEGRAM_API_HASH 环境变量未设置"
    echo "   请设置：export TELEGRAM_API_HASH='你的api_hash'"
    echo ""
fi

# 获取 API ID
echo "请输入你的 Telegram API ID（从 https://my.telegram.org/apps 获取）："
read -r API_ID

if [ -z "$API_ID" ]; then
    echo "❌ 错误：API ID 不能为空"
    exit 1
fi

echo ""
echo "API ID: $API_ID"
echo ""

# 创建下载目录
DOWNLOAD_DIR="/tmp/openfang-telegram-downloads"
mkdir -p "$DOWNLOAD_DIR"
echo "✅ 创建下载目录：$DOWNLOAD_DIR"
echo ""

# 备份配置文件
CONFIG_FILE="$HOME/.openfang/config.toml"
if [ -f "$CONFIG_FILE" ]; then
    BACKUP_FILE="$CONFIG_FILE.backup.$(date +%Y%m%d_%H%M%S)"
    cp "$CONFIG_FILE" "$BACKUP_FILE"
    echo "✅ 备份配置文件：$BACKUP_FILE"
    echo ""
fi

# 更新配置
echo "正在更新配置文件..."

# 检查是否已有 [channels.telegram] 配置
if grep -q "\[channels.telegram\]" "$CONFIG_FILE"; then
    echo "⚠️  检测到现有 Telegram 配置，请手动更新以下字段："
    echo ""
    echo "  use_local_api = true"
    echo "  auto_start_local_api = true"
    echo "  telegram_api_id = \"$API_ID\""
    echo "  telegram_api_hash_env = \"TELEGRAM_API_HASH\""
    echo "  local_api_port = 8081"
    echo "  api_url = \"http://localhost:8081\""
    echo "  max_download_size = 2147483648  # 2GB"
    echo ""
else
    # 添加新配置
    cat >> "$CONFIG_FILE" << EOF

[channels.telegram]
default_agent = "shipinfabu-hand"
poll_interval_secs = 1
download_enabled = true
download_dir = "$DOWNLOAD_DIR"
max_download_size = 2147483648  # 2GB

# Local Bot API Server 配置（支持 >20MB 文件下载）
use_local_api = true
auto_start_local_api = true
telegram_api_id = "$API_ID"
telegram_api_hash_env = "TELEGRAM_API_HASH"
local_api_port = 8081
api_url = "http://localhost:8081"

[channels.telegram.overrides]
dm_policy = "respond"
group_policy = "all"
EOF
    echo "✅ 配置已更新"
    echo ""
fi

# 显示下一步
echo "=== 配置完成 ==="
echo ""
echo "下一步："
echo "1. 确保环境变量已设置："
echo "   export TELEGRAM_BOT_TOKEN='你的bot_token'"
echo "   export TELEGRAM_API_HASH='你的api_hash'"
echo ""
echo "2. 启动 OpenFang："
echo "   cd $REPO_ROOT"
echo "   target/release/openfang start"
echo ""
echo "3. 测试大文件下载："
echo "   在 Telegram 发送一个 >20MB 的视频"
echo ""
echo "4. 查看日志："
echo "   tail -f ~/.openfang/logs/openfang.log"
echo ""
echo "详细文档：docs/telegram-deployment-guide.md"
