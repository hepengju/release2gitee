## Github的Release同步到Gitee

# 背景
> 基于Tauri编写的桌面应用: [RedisME](https://github.com/hepengju/redis-me) 在Github打包发布，
国内网络环境导致应用自动升级比较困难，故想将Github的Release同步到Gitee，方便用户下载与软件的自动升级。

# 调研
- [Gitee-Sync-Tool](https://github.com/XingHeYuZhuan/Gitee-Sync-Tool/blob/main/.github/workflows/gitee-batch-sync.yml)
> 纯Shell脚本实现，有些特殊场景的处理，维护起来比较麻烦
- [sync-action](https://github.com/H-TWINKLE/sync-action)
> 基于Python脚本实现，比较简单，但需要安装Python环境。实测也遇到一些问题没有完成同步
- [sync-release-gitee](https://github.com/trustedinster/sync-release-gitee/tree/v1.1)
> 同上，基本一致

# 分析
> 仅仅调用几个API接口就可以实现，另外安装Python 或 Node等似乎有点大材小用
> Tauri应用的自动升级，同步时还需要修改 latest.json 文件内容

# 方案: 采用Rust实现编写cli可执行文件
- 体积非常小: 在Github的action中, 直接选择 ubuntu-latest, 运行时间非常短
- 跨平台支持: Windows、MacOS、Linux 等都可以方便的测试验证

# 参考
- [fnm](https://github.com/Schniz/fnm)