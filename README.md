## sync github releases to gitee releases

- 体积非常小: 约6M
- 执行速度快: 基于Rust编写, reqwest执行http请求
- 跨平台支持: Windows、MacOS、Linux 等都可以支持
- 进度条显示: 下载上传附件都支持进度条显示
- 操作幂等性: 所有步骤都可随意阻断或停止，可重复执行不影响（复用已下载的附件等）
- 智能附件管理:
    * 所有 Release 都会同步到 Gitee
    * 仅保留最新 N 个 Release 的额外附件，其他只保留元数据 + 2个源码附件（节省存储空间）
    * 通过 `gitee_retain_release_attach_files_count` 配置保留数量（默认3个）
- 其他定制化:
    * 可选配置是否支持替换 response body 或 latest.json 文件中的github下载地址为gitee下载地址(默认为true)
    * 可选设置github_token. 速率: 50 次/小时 ==> 3000 次/小时(默认None)
    * 可选指定gitee仓库分支名称，不指定则自动获取默认分支(默认auto)
    * 可选-v参数查看命令执行详细信息(默认info级别)

```shell
# 推荐参数配置到环境变量中
vim ~/.bashrc

# release2gitee
export github_owner=hepengju
export github_repo=redis-me
export gitee_owner=hepengju
export gitee_repo=redis-me
export gitee_token=449cb0c5************************

# 可选配置
export release2gitee__github_latest_release_count=99        # 从GitHub获取最新的N个Release（默认5）
export release2gitee__gitee_retain_release_attach_files_count=3  # Gitee保留带附件的Release数量（默认3）
export release2gitee__release_body_url_replace=true          # 是否替换body中的URL（默认true）
export release2gitee__latest_json_url_replace=true           # 是否替换latest.json中的URL（默认true）

source ~/.bashrc
```

```shell
# 查看帮助
$ ./release2gitee.exe --help

# 示例: 执行同步 (参数配置到环境变量中，临时修改个别参数)
$ ./release2gitee --github-repo=release2gitee --gitee-repo=release2gitee

# 示例: 指定gitee仓库分支名称
$ ./release2gitee --gitee-branch master
```

## 核心策略

### 🎯 设计理念
**所有 Release 都同步到 Gitee，但只有最新 N 个保留额外附件**

### 📋 执行流程

#### 1️⃣ 确定白名单
- 合并 GitHub 和 Gitee 的所有 Releases
- 按版本号排序（最新的在前）
- 取前 N 个作为"带附件白名单"（由 `gitee_retain_release_attach_files_count` 配置）

#### 2️⃣ 清理阶段
遍历 Gitee 上已有的 Releases：
```
如果 Release 不在白名单中：
  ├─ assets.len() > 2 → 有额外附件，需要清理
  │   ├─ 删除 Release
  │   ├─ 睡眠 1 秒
  │   └─ 重新创建（不上传额外附件，Gitee 自动保留 2 个源码附件）
  └─ assets.len() == 2 → 只有源码附件，已清理过，跳过 ✅
```

#### 3️⃣ 同步阶段
遍历 GitHub 的 Releases：
```
如果 Release 已存在于 Gitee：
  └─ 直接跳过 ✅（完全幂等）

如果 Release 不存在：
  ├─ 在白名单中 → 创建并上传所有附件
  └─ 不在白名单 → 只创建元数据，不上传附件
```

### ✨ 关键特性

1. **幂等性**: 第二次运行不会重复操作，所有已存在的 Release 都会被跳过
2. **智能判断**: 通过 `assets.len() > 2` 准确识别是否有额外附件（Gitee 默认包含 2 个源码附件）
3. **URL 替换**: body 中的 github.com → gitee.com 自动替换
4. **速率控制**: 创建后延时 3 秒，保证 Gitee 上的顺序正确
5. **节省空间**: 旧版本只保留元数据 + 2 个源码附件，删除额外附件
6. **原子性操作**: 先下载所有附件再创建 Release，避免中间失败导致状态不一致

### 🎯 最终效果

```
Gitee 上的 Release（假设配置保留2个）：
├─ v3.8.0 (最新) → 元数据 + 2个源码附件 + N个额外附件 ✅
├─ v3.7.0        → 元数据 + 2个源码附件 + N个额外附件 ✅
├─ v3.6.0        → 元数据 + 2个源码附件（无额外附件）
├─ v3.5.0        → 元数据 + 2个源码附件（无额外附件）
└─ ...           → 同上
```

# 背景

> 基于Tauri编写的桌面应用: [RedisME](https://github.com/hepengju/redis-me) 在Github打包发布，
> 国内网络环境导致应用自动升级比较困难，故想将Github的Release同步到Gitee，方便用户下载与软件的自动升级。

# 调研

- [Gitee-Sync-Tool](https://github.com/XingHeYuZhuan/Gitee-Sync-Tool/blob/main/.github/workflows/gitee-batch-sync.yml)

> 纯Shell脚本实现，有些特殊场景的处理，维护起来比较麻烦

- [sync-action](https://github.com/H-TWINKLE/sync-action)

> 基于Python脚本实现，比较简单，但需要安装Python环境。而且github的打包机器上传gitee附件特别慢

- [sync-release-gitee](https://github.com/trustedinster/sync-release-gitee/tree/v1.1)