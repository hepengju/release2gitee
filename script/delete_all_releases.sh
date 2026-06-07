#!/bin/bash

# 删除 Gitee 上所有 Release 的脚本
# 使用前请设置环境变量或修改下面的变量

# 配置（可以从环境变量读取）
GITEE_OWNER="${gitee_owner:-hepengju}"
GITEE_REPO="${gitee_repo:-redis-me}"
GITEE_TOKEN="${gitee_token}"

# 检查 token 是否设置
if [ -z "$GITEE_TOKEN" ]; then
    echo "错误: 请设置 gitee_token 环境变量"
    echo "例如: export gitee_token=your_token_here"
    exit 1
fi

echo "准备删除 Gitee 仓库 ${GITEE_OWNER}/${GITEE_REPO} 的所有 Release..."
echo "警告: 此操作不可逆！"
echo ""

# 获取所有 Release
echo "正在获取 Release 列表..."
RELEASES=$(curl -s -X GET \
  "https://gitee.com/api/v5/repos/${GITEE_OWNER}/${GITEE_REPO}/releases?per_page=100&page=1" \
  -H "Authorization: token ${GITEE_TOKEN}")

# 检查是否获取成功
if [ $? -ne 0 ]; then
    echo "错误: 获取 Release 列表失败"
    exit 1
fi

# 提取 Release ID 和 tag_name（使用更精确的匹配）
# 先提取每个 release 对象，再从中提取 id 和 tag_name
RELEASE_DATA=$(echo "$RELEASES" | grep -o '{[^}]*"tag_name":"[^"]*"[^}]*}' | while read -r line; do
    ID=$(echo "$line" | grep -o '"id":[0-9]*' | head -1 | grep -o '[0-9]*')
    TAG=$(echo "$line" | grep -o '"tag_name":"[^"]*"' | grep -o '"[^"]*"$' | tr -d '"')
    if [ -n "$ID" ] && [ -n "$TAG" ]; then
        echo "${ID}:${TAG}"
    fi
done)

# 转换为数组
IFS=$'\n' read -r -d '' -a DATA_ARRAY <<< "$RELEASE_DATA"

COUNT=${#DATA_ARRAY[@]}

if [ "$COUNT" -eq 0 ]; then
    echo "没有找到任何 Release"
    exit 0
fi

echo "找到 ${COUNT} 个 Release:"
declare -a IDS_ARRAY
declare -a TAGS_ARRAY
for i in "${!DATA_ARRAY[@]}"; do
    ID="${DATA_ARRAY[$i]%%:*}"
    TAG="${DATA_ARRAY[$i]##*:}"
    IDS_ARRAY[$i]="$ID"
    TAGS_ARRAY[$i]="$TAG"
    echo "  - ${TAG} (ID: ${ID})"
done

echo ""
read -p "确认删除以上所有 Release？(yes/no): " CONFIRM

if [ "$CONFIRM" != "yes" ]; then
    echo "已取消操作"
    exit 0
fi

echo ""
echo "开始删除..."

DELETED=0
FAILED=0

for i in "${!IDS_ARRAY[@]}"; do
    ID=${IDS_ARRAY[$i]}
    TAG=${TAGS_ARRAY[$i]}
    
    echo -n "删除 ${TAG} (ID: ${ID})... "
    
    RESPONSE=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE \
      "https://gitee.com/api/v5/repos/${GITEE_OWNER}/${GITEE_REPO}/releases/${ID}" \
      -H "Authorization: token ${GITEE_TOKEN}")
    
    if [ "$RESPONSE" = "204" ] || [ "$RESPONSE" = "200" ]; then
        echo "✓ 成功"
        DELETED=$((DELETED + 1))
    else
        echo "✗ 失败 (HTTP ${RESPONSE})"
        FAILED=$((FAILED + 1))
    fi
    
    # 避免请求过快
    sleep 1
done

echo ""
echo "删除完成！"
echo "  成功: ${DELETED}"
echo "  失败: ${FAILED}"
