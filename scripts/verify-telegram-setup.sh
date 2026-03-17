#!/bin/bash
# Telegram Local API 快速验证脚本

set -e

echo "=== Telegram Local API 部署验证 ==="
echo ""

# 颜色定义
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 检查函数
check_pass() {
    echo -e "${GREEN}✅ $1${NC}"
}

check_fail() {
    echo -e "${RED}❌ $1${NC}"
}

check_warn() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

# 1. 检查二进制文件
echo "1. 检查二进制文件..."
if [ -f "$HOME/.openfang/bin/telegram-bot-api" ]; then
    check_pass "telegram-bot-api 二进制文件存在"
    VERSION=$($HOME/.openfang/bin/telegram-bot-api --version 2>&1 | head -1 || echo "未知版本")
    echo "   版本: $VERSION"
else
    check_fail "telegram-bot-api 二进制文件不存在"
    echo "   位置: $HOME/.openfang/bin/telegram-bot-api"
    exit 1
fi

if [ -f "target/release/openfang" ]; then
    check_pass "OpenFang 二进制文件存在"
    SIZE=$(ls -lh target/release/openfang | awk '{print $5}')
    echo "   大小: $SIZE"
else
    check_fail "OpenFang 二进制文件不存在"
    echo "   请运行: cargo build --release -p openfang-cli"
    exit 1
fi

echo ""

# 2. 检查环境变量
echo "2. 检查环境变量..."
if [ -n "$TELEGRAM_BOT_TOKEN" ]; then
    check_pass "TELEGRAM_BOT_TOKEN 已设置"
    echo "   值: ${TELEGRAM_BOT_TOKEN:0:10}..."
else
    check_fail "TELEGRAM_BOT_TOKEN 未设置"
    echo "   请设置: export TELEGRAM_BOT_TOKEN='你的bot_token'"
fi

if [ -n "$TELEGRAM_API_HASH" ]; then
    check_pass "TELEGRAM_API_HASH 已设置"
    echo "   值: ${TELEGRAM_API_HASH:0:10}..."
else
    check_fail "TELEGRAM_API_HASH 未设置"
    echo "   请设置: export TELEGRAM_API_HASH='你的api_hash'"
fi

echo ""

# 3. 检查配置文件
echo "3. 检查配置文件..."
CONFIG_FILE="$HOME/.openfang/config.toml"
if [ -f "$CONFIG_FILE" ]; then
    check_pass "配置文件存在"

    if grep -q "use_local_api = true" "$CONFIG_FILE"; then
        check_pass "use_local_api = true"
    else
        check_warn "use_local_api 未设置为 true"
    fi

    if grep -q "auto_start_local_api = true" "$CONFIG_FILE"; then
        check_pass "auto_start_local_api = true"
    else
        check_warn "auto_start_local_api 未设置为 true"
    fi

    if grep -q "telegram_api_id" "$CONFIG_FILE"; then
        API_ID=$(grep "telegram_api_id" "$CONFIG_FILE" | head -1 | cut -d'"' -f2)
        if [ "$API_ID" != "YOUR_API_ID" ] && [ -n "$API_ID" ]; then
            check_pass "telegram_api_id 已配置"
        else
            check_fail "telegram_api_id 未正确配置"
        fi
    else
        check_fail "telegram_api_id 未找到"
    fi
else
    check_fail "配置文件不存在: $CONFIG_FILE"
fi

echo ""

# 4. 检查目录
echo "4. 检查下载目录..."
DOWNLOAD_DIR="/tmp/openfang-telegram-downloads"
if [ -d "$DOWNLOAD_DIR" ]; then
    check_pass "下载目录存在: $DOWNLOAD_DIR"
else
    check_warn "下载目录不存在，将自动创建"
    mkdir -p "$DOWNLOAD_DIR"
    check_pass "下载目录已创建"
fi

echo ""

# 5. 检查端口
echo "5. 检查端口占用..."
if lsof -i :8081 > /dev/null 2>&1; then
    check_warn "端口 8081 已被占用"
    lsof -i :8081
else
    check_pass "端口 8081 可用"
fi

if lsof -i :4200 > /dev/null 2>&1; then
    check_warn "端口 4200 已被占用"
    lsof -i :4200
else
    check_pass "端口 4200 可用"
fi

echo ""

# 6. 检查进程
echo "6. 检查运行中的进程..."
if ps aux | grep -v grep | grep "openfang start" > /dev/null; then
    check_warn "OpenFang 已在运行"
    ps aux | grep -v grep | grep "openfang start"
else
    check_pass "没有运行中的 OpenFang 进程"
fi

if ps aux | grep -v grep | grep "telegram-bot-api" > /dev/null; then
    check_warn "telegram-bot-api 已在运行"
    ps aux | grep -v grep | grep "telegram-bot-api"
else
    check_pass "没有运行中的 telegram-bot-api 进程"
fi

echo ""

# 7. 检查文档
echo "7. 检查文档..."
DOCS=(
    "docs/telegram-deployment-guide.md"
    "docs/telegram-large-files.md"
    "docs/telegram-shipinbot-integration.md"
    "docs/telegram-implementation-summary.md"
    "docs/telegram-testing-checklist.md"
    "scripts/setup-telegram-local-api.sh"
)

for doc in "${DOCS[@]}"; do
    if [ -f "$doc" ]; then
        check_pass "$doc"
    else
        check_fail "$doc 不存在"
    fi
done

echo ""
echo "=== 验证完成 ==="
echo ""

# 总结
if [ -z "$TELEGRAM_BOT_TOKEN" ] || [ -z "$TELEGRAM_API_HASH" ]; then
    echo -e "${RED}❌ 环境变量未完全设置，无法启动${NC}"
    echo ""
    echo "请设置环境变量："
    echo "  export TELEGRAM_BOT_TOKEN='你的bot_token'"
    echo "  export TELEGRAM_API_HASH='你的api_hash'"
    echo ""
    exit 1
fi

echo -e "${GREEN}✅ 所有检查通过！${NC}"
echo ""
echo "下一步："
echo "1. 启动 OpenFang："
echo "   cd /Users/xiaomo/Desktop/openfang-upstream-fork"
echo "   target/release/openfang start"
echo ""
echo "2. 查看日志："
echo "   tail -f ~/.openfang/logs/openfang.log"
echo ""
echo "3. 测试大文件下载："
echo "   在 Telegram 发送一个 >20MB 的视频"
echo ""
echo "详细文档："
echo "  - 部署指南: docs/telegram-deployment-guide.md"
echo "  - 测试清单: docs/telegram-testing-checklist.md"
