## Github的Release同步到Gitee

```shell
# 推荐参数配置到环境变量中
vim ~/.bashrc

# release2gitee
export github_owner=hepengju
export github_repo=redis-me
export gitee_owner=hepengju
export gitee_repo=redis-me
export gitee_token=449cb0c54b40e82c0bd8861d5d9411fb
#export release2gitee__github_latest_release_count=5
#export release2gitee__gitee_retain_release_count=5
#export release2gitee__release_body_url_replace=false
#export release2gitee__latest_json_url_replace=false

source ~/.bashrc

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
          [env: GITEE_TOKEN=449cb0c54b40e82c0bd8861d5d9411fb]
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

# 执行同步（网络问题可能出错，可重试执行，会复用已下载的文件及对比release分支的内容和附件列表）
$ ./release2gitee
[2025-12-21T06:23:25Z INFO  release2gitee] 命令行解析完成: github_owner: hepengju, github_repo: release2gitee, gitee_owner: hepengju, gitee_repo: hepengju, gitee_token: 449cb0c5************************, lastest_release_count: 10, skip_release_body_url_replace: false, skip_lastest_json_url_replace: false
[2025-12-21T06:23:25Z INFO  release2gitee::sync] GET: https://api.github.com/repos/hepengju/release2gitee/releases?per_page=10&page=1
[2025-12-21T06:23:26Z INFO  release2gitee::sync] github releases最近的1个成功: v0.1.0
[2025-12-21T06:23:26Z INFO  release2gitee::sync] GET: https://gitee.com/api/v5/repos/hepengju/release2gitee/releases?per_page=100&page=1
[2025-12-21T06:23:26Z INFO  release2gitee::sync] gitee releases获取0个:
[2025-12-21T06:23:26Z INFO  release2gitee::sync] POST: https://gitee.com/api/v5/repos/hepengju/release2gitee/releases
[2025-12-21T06:23:27Z INFO  release2gitee::sync] gitee release创建成功: v0.1.0!
[2025-12-21T06:23:27Z INFO  release2gitee::sync] 创建目录: v0.1.0
[2025-12-21T06:23:27Z INFO  release2gitee::sync] 开始下载附件: release2gitee.exe
00:00:08 █████████████████████████████████████████████████████████████████████████████████████████████████████████████████████████████████████ 6.73 MiB/6.73 MiB (818.05 KiB/s, 0s)[2025-12-21T06:23:37Z INFO  release2gitee::sync] 下载附件成功: release2gitee.exe
[2025-12-21T06:23:39Z INFO  release2gitee::sync] 上传附件到gitee成功: release2gitee.exe
[2025-12-21T06:23:39Z INFO  release2gitee] 同步程序执行完毕
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

# 分析
> 仅仅调用几个API接口就可以实现，另外安装Python 或 Node等似乎有点大材小用
> Tauri应用的自动升级，同步时还需要修改 latest.json 文件内容

# 方案: 采用Rust实现编写cli可执行文件
- 体积非常小: 在Github的action中, 直接选择 ubuntu-latest, 运行时间非常短
- 跨平台支持: Windows、MacOS、Linux 等都可以方便的测试验证