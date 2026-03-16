#!/bin/bash
# 部署媒体组修复到 144.48.4.99

set -e

echo "=== 部署 OpenFang 媒体组修复 ==="
echo ""

# 1. 复制二进制文件到服务器
echo "1. 上传 openfang 二进制文件..."
scp target/release/openfang root@144.48.4.99:/tmp/openfang-new

# 2. 在服务器上备份并替换
echo ""
echo "2. 在服务器上备份并替换..."
ssh root@144.48.4.99 << 'ENDSSH'
set -e

# 备份当前版本
if [ -f ~/.cargo/bin/openfang ]; then
    cp ~/.cargo/bin/openfang ~/.cargo/bin/openfang.backup.$(date +%Y%m%d_%H%M%S)
    echo "   已备份当前版本"
fi

# 替换新版本
mv /tmp/openfang-new ~/.cargo/bin/openfang
chmod +x ~/.cargo/bin/openfang
echo "   已安装新版本"

# 重启 OpenFang daemon
echo ""
echo "3. 重启 OpenFang daemon..."
openfang stop || true
sleep 2
openfang start

# 等待启动
sleep 3

# 检查状态
echo ""
echo "4. 检查服务状态..."
openfang status
echo ""
openfang hand active

ENDSSH

echo ""
echo "=== 部署完成 ==="
echo ""
echo "下一步测试："
echo "  1. 在 Telegram 中发送媒体组（多张图片）"
echo "  2. 验证只触发一次 agent 调用"
echo "  3. 检查日志: ssh root@144.48.4.99 'tail -f ~/.openfang/daemon-reconcile.stdout.log'"
