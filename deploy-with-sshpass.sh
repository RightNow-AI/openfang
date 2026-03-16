#!/usr/bin/env bash
# 使用 sshpass 部署 OpenFang 媒体组修复到 144.48.4.99

set -euo pipefail

HOST="${SHIPINBOT_CLUSTER_HOST:-144.48.4.99}"
PORT="${SHIPINBOT_CLUSTER_PORT:-22}"
USER_NAME="${SHIPINBOT_CLUSTER_USER:-root}"
PASSWORD="${SHIPINBOT_CLUSTER_PASSWORD:-}"

if [ -z "$PASSWORD" ]; then
  echo "错误: 请设置 SHIPINBOT_CLUSTER_PASSWORD 环境变量" >&2
  echo "用法: export SHIPINBOT_CLUSTER_PASSWORD='your-password' && ./deploy-with-sshpass.sh" >&2
  exit 1
fi

if ! command -v sshpass >/dev/null 2>&1; then
  echo "错误: 需要安装 sshpass" >&2
  echo "安装方法: brew install sshpass" >&2
  exit 1
fi

run_ssh() {
  sshpass -p "$PASSWORD" ssh \
    -o StrictHostKeyChecking=no \
    -o PreferredAuthentications=password \
    -o PubkeyAuthentication=no \
    -p "$PORT" \
    "${USER_NAME}@${HOST}" \
    "$@"
}

run_scp() {
  sshpass -p "$PASSWORD" scp \
    -o StrictHostKeyChecking=no \
    -o PreferredAuthentications=password \
    -o PubkeyAuthentication=no \
    -P "$PORT" \
    "$@"
}

echo "=== 部署 OpenFang 媒体组修复 ==="
echo "目标服务器: ${HOST}"
echo ""

# 1. 上传二进制文件
echo "1. 上传 openfang 二进制文件..."
run_scp target/release/openfang "${USER_NAME}@${HOST}:/tmp/openfang-new"
echo "✅ 上传完成"
echo ""

# 2. 备份并替换
echo "2. 备份并替换..."
run_ssh << 'ENDSSH'
set -e

# 备份当前版本
if [ -f ~/.cargo/bin/openfang ]; then
    cp ~/.cargo/bin/openfang ~/.cargo/bin/openfang.backup.$(date +%Y%m%d_%H%M%S)
    echo "   ✅ 已备份当前版本"
fi

# 替换新版本
mv /tmp/openfang-new ~/.cargo/bin/openfang
chmod +x ~/.cargo/bin/openfang
echo "   ✅ 已安装新版本"
ENDSSH

echo ""

# 3. 重启 daemon
echo "3. 重启 OpenFang daemon..."
run_ssh "openfang stop" || true
sleep 2
run_ssh "openfang start"
echo "✅ Daemon 已重启"
sleep 3

# 4. 检查状态
echo ""
echo "4. 检查服务状态..."
run_ssh "openfang status"
echo ""
run_ssh "openfang hand active"

echo ""
echo "=== 部署完成 ==="
echo ""
echo "下一步测试："
echo "  1. 在 Telegram 中发送媒体组（多张图片）"
echo "  2. 验证只触发一次 agent 调用，不再出现 9 次重复错误"
echo "  3. 检查日志: export SHIPINBOT_CLUSTER_PASSWORD='...' && sshpass -p \"\$PASSWORD\" ssh root@144.48.4.99 'tail -f ~/.openfang/daemon-reconcile.stdout.log'"
