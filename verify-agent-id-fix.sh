#!/bin/bash
# 验证 Agent ID 自动刷新功能

echo "=== 验证 Agent ID 自动刷新功能 ==="
echo ""

# 1. 获取当前 agent ID
echo "1. 当前 shipinfabu-hand agent ID:"
CURRENT_ID=$(openfang hand active | grep shipinfabu-hand | awk '{print $2}' | tr -d '()')
echo "   $CURRENT_ID"
echo ""

# 2. 模拟 agent 重建（deactivate + activate）
echo "2. 模拟 agent 重建..."
echo "   停用 hand..."
openfang hand deactivate shipinfabu-hand 2>&1 | head -3
sleep 2

echo "   重新激活 hand..."
openfang hand activate shipinfabu-hand 2>&1 | head -3
sleep 3

# 3. 获取新的 agent ID
echo ""
echo "3. 重建后的 shipinfabu-hand agent ID:"
NEW_ID=$(openfang hand active | grep shipinfabu-hand | awk '{print $2}' | tr -d '()')
echo "   $NEW_ID"
echo ""

# 4. 检查日志中是否有自动更新记录
echo "4. 检查日志中的自动更新记录:"
sleep 2
UPDATED=$(tail -100 ~/.openfang/daemon-reconcile.stdout.log | grep -i "Updated.*Telegram.*default agent" | tail -1)

if [ -n "$UPDATED" ]; then
    echo "   ✅ 找到自动更新记录:"
    echo "   $UPDATED"
else
    echo "   ⚠️  未找到自动更新记录（可能需要等待几秒）"
fi
echo ""

# 5. 验证结果
echo "5. 验证结果:"
if [ "$CURRENT_ID" != "$NEW_ID" ]; then
    echo "   ✅ Agent ID 已更新: $CURRENT_ID -> $NEW_ID"
    if [ -n "$UPDATED" ]; then
        echo "   ✅ 后台监听任务正常工作"
        echo ""
        echo "🎉 修复验证成功！Agent ID 自动刷新功能正常工作。"
    else
        echo "   ⚠️  Agent ID 已更新，但未在日志中找到自动更新记录"
        echo "   建议：等待几秒后再次检查日志"
    fi
else
    echo "   ❌ Agent ID 未更新（可能是 hand 重建失败）"
fi
echo ""

# 6. 测试建议
echo "6. 下一步测试:"
echo "   - 在 Telegram 中给 @linyiagibot 发送测试消息"
echo "   - 验证 bot 能正常响应"
echo "   - 观察日志: tail -f ~/.openfang/daemon-reconcile.stdout.log"
