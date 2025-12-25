## sync github releases to gitee releases
- 体积非常小: 约6M
- 执行速度快: 基于Rust编写, reqwest执行http请求
- 跨平台支持: Windows、MacOS、Linux 等都可以支持
- 进度条显示: 下载上传附件都支持进度条显示
- 操作幂等性: 所有步骤都可随意阻断或停止，可重复执行不影响（复用已下载的附件等）
- 其他定制化:
  * 支持替换response body 或 latest.json 文件中的github下载地址为gitee下载地址
  * 支持设置gitee releases保留个数，自动清理旧的标签
  * 可选设置github_token. 速率: 50 次/小时  ==> 3000 次/小时

```shell
# 推荐参数配置到环境变量中
vim ~/.bashrc

# release2gitee
export github_owner=hepengju
export github_repo=redis-me
export gitee_owner=hepengju
export gitee_repo=redis-me
export gitee_token=449cb0c5************************
#export release2gitee__github_latest_release_count=5
#export release2gitee__gitee_retain_release_count=5
#export release2gitee__release_body_url_replace=false
#export release2gitee__latest_json_url_replace=false

source ~/.bashrc
```

```shell
# 查看帮助
$ ./release2gitee.exe --help
sync github releases to gitee releases

Usage: release2gitee.exe [OPTIONS] --github-owner <GITHUB_OWNER> --github-repo <GITHUB_REPO> --gitee-owner <GITEE_OWNER> --gitee-repo <GITEE_REPO> --gitee-token <GITEE_TOKEN>

Options:
      --github-owner <GITHUB_OWNER>
          [env: GITHUB_OWNER=hepengju]
      --github-repo <GITHUB_REPO>
          [env: GITHUB_REPO=redis-me]
      --gitee-owner <GITEE_OWNER>
          [env: GITEE_OWNER=hepengju]
      --gitee-repo <GITEE_REPO>
          [env: GITEE_REPO=redis-me]
      --gitee-token <GITEE_TOKEN>
          [env: GITEE_TOKEN=449cb0c5************************]
      --github-latest-release-count <GITHUB_LATEST_RELEASE_COUNT>
          [env: release2gitee__github_latest_release_count=] [default: 5]
      --gitee-retain-release-count <GITEE_RETAIN_RELEASE_COUNT>
          [env: release2gitee__gitee_retain_release_count=] [default: 999]
      --release-body-url-replace
          [env: release2gitee__release_body_url_replace=]
      --latest-json-url-replace
          [env: release2gitee__latest_json_url_replace=]
  -v, --verbose...
          Increase logging verbosity
  -q, --quiet...
          Decrease logging verbosity
  -h, --help
          Print help
  -V, --version
          Print version
```

```shell
# 示例: 执行同步 (参数配置到环境变量中)
# 网络问题可能出错，可重试执行，会复用已下载的文件及对比release分支的内容和附件列表
$ ./release2gitee
[2025-12-24T08:20:06Z INFO ] params: github_owner: hepengju, github_repo: release2gitee, gitee_owner: hepengju, gitee_repo: release2gitee, gitee_token: 449cb0c5************************, github_latest_release_count: 5, gitee_retain_release_count: 999, release_body_url_replace: true, latest_json_url_replace: true
[2025-12-24T08:20:06Z INFO ] GET: https://api.github.com/repos/hepengju/release2gitee/releases?per_page=5&page=1
[2025-12-24T08:20:07Z INFO ] github releases获取最新的2个: v0.9.0, v0.1.0
[2025-12-24T08:20:07Z INFO ] GET: https://gitee.com/api/v5/repos/hepengju/release2gitee/releases?per_page=100&page=1
[2025-12-24T08:20:08Z INFO ] gitee releases获取到1个: v0.1.0
[2025-12-24T08:20:08Z INFO ] gitee releases 无需清理
[2025-12-24T08:20:08Z INFO ] PATCH: https://gitee.com/api/v5/repos/hepengju/release2gitee/releases/560076
[2025-12-24T08:20:08Z INFO ] gitee release更新成功: v0.1.0!
[2025-12-24T08:20:08Z INFO ] gitee release与github release附件相同: v0.1.0!
[2025-12-24T08:20:08Z INFO ] POST: https://gitee.com/api/v5/repos/hepengju/release2gitee/releases
[2025-12-24T08:20:08Z INFO ] gitee release创建成功: v0.9.0!
[2025-12-24T08:20:08Z INFO ] 临时目录创建: C:\Users\he_pe\AppData\Local\Temp\release2gitee\v0.9.0
[2025-12-24T08:20:08Z INFO ] downloading: https://github.com/hepengju/release2gitee/releases/download/v0.9.0/release2gitee.exe
00:00:04 [#################################################################] 5.59 MiB/5.59 MiB (1.16 MiB/s, 0s)
[2025-12-24T08:20:15Z INFO ] 临时目录存在: C:\Users\he_pe\AppData\Local\Temp\release2gitee\v0.9.0
[2025-12-24T08:20:15Z INFO ] uploading: https://gitee.com/api/v5/repos/hepengju/release2gitee/releases/561151/attach_files, file: release2gitee.exe
00:00:01 [#################################################################] 5.59 MiB/5.59 MiB (4.39 MiB/s, 0s)
[2025-12-24T08:20:16Z INFO ] 同步程序执行完成

# 示例: 执行同步 (参数配置到环境变量中，临时修改个别参数)
$ ./release2gitee --github-repo=release2gitee --gitee-repo=release2gitee
```

# 背景
> 基于Tauri编写的桌面应用: [RedisME](https://github.com/hepengju/redis-me) 在Github打包发布，
国内网络环境导致应用自动升级比较困难，故想将Github的Release同步到Gitee，方便用户下载与软件的自动升级。

# 调研
- [Gitee-Sync-Tool](https://github.com/XingHeYuZhuan/Gitee-Sync-Tool/blob/main/.github/workflows/gitee-batch-sync.yml)
> 纯Shell脚本实现，有些特殊场景的处理，维护起来比较麻烦
- [sync-action](https://github.com/H-TWINKLE/sync-action)
> 基于Python脚本实现，比较简单，但需要安装Python环境。而且github的打包机器上传gitee附件特别慢
- [sync-release-gitee](https://github.com/trustedinster/sync-release-gitee/tree/v1.1)
> 同上，基本一致