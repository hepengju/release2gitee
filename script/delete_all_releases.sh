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

# 获取所有 Release（处理分页）
echo "正在获取 Release 列表..."
ALL_RELEASES_JSON="["
PAGE=1
PER_PAGE=100
FIRST_PAGE=true

while true; do
    RELEASES=$(curl -s -X GET \
      "https://gitee.com/api/v5/repos/${GITEE_OWNER}/${GITEE_REPO}/releases?per_page=${PER_PAGE}&page=${PAGE}" \
      -H "Authorization: token ${GITEE_TOKEN}")
    
    # 检查是否获取成功
    if [ $? -ne 0 ]; then
        echo "错误: 获取 Release 列表失败"
        exit 1
    fi
    
    # 如果返回空数组或错误，退出循环
    if [ "$RELEASES" = "[]" ] || [ -z "$RELEASES" ]; then
        break
    fi
    
    # 移除开头的 [ 和结尾的 ]，然后添加到总数组中
    RELEASES_TRIMMED=$(echo "$RELEASES" | sed '1s/^\[//' | sed '$s/\]$//')
    
    if [ "$FIRST_PAGE" = true ]; then
        ALL_RELEASES_JSON="${ALL_RELEASES_JSON}${RELEASES_TRIMMED}"
        FIRST_PAGE=false
    else
        ALL_RELEASES_JSON="${ALL_RELEASES_JSON},${RELEASES_TRIMMED}"
    fi
    
    # 检查是否还有更多页面（简单判断：如果返回数量小于 per_page，说明是最后一页）
    ITEM_COUNT=$(echo "$RELEASES" | grep -o '"id":' | wc -l)
    if [ "$ITEM_COUNT" -lt "$PER_PAGE" ]; then
        break
    fi
    
    PAGE=$((PAGE + 1))
done

ALL_RELEASES_JSON="${ALL_RELEASES_JSON}]"

if [ "$ALL_RELEASES_JSON" = "[]" ]; then
    echo "没有找到任何 Release"
    exit 0
fi

# 提取 Release ID 和 tag_name
# 使用更可靠的方式解析 JSON
RELEASE_DATA=$(echo "$ALL_RELEASES_JSON" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    if isinstance(data, list):
        for item in data:
            if 'id' in item and 'tag_name' in item:
                print(f\"{item['id']}:{item['tag_name']}\")
except:
    pass
" 2>/dev/null)

# 如果 python3 不可用，回退到 grep 方式
if [ -z "$RELEASE_DATA" ]; then
    RELEASE_DATA=$(echo "$ALL_RELEASES_JSON" | grep -oP '"id":\s*\K[0-9]+(?=.*?"tag_name":\s*"([^"]*)")' | while read -r id_line; do
        TAG=$(echo "$ALL_RELEASES_JSON" | grep -oP "\"id\":\s*${id_line}.*?\"tag_name\":\s*\"\K[^\"]+")
        if [ -n "$TAG" ]; then
            echo "${id_line}:${TAG}"
        fi
    done)
fi

# 转换为数组（过滤空行）
IFS=$'\n' read -r -d '' -a DATA_ARRAY <<< "$(echo "$RELEASE_DATA" | grep -v '^$')"

# 再次检查是否有有效数据
VALID_COUNT=0
for item in "${DATA_ARRAY[@]}"; do
    if [ -n "$item" ]; then
        VALID_COUNT=$((VALID_COUNT + 1))
    fi
done

if [ "$VALID_COUNT" -eq 0 ]; then
    echo "没有找到任何 Release"
    exit 0
fi

echo "找到 ${VALID_COUNT} 个 Release:"
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
