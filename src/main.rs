use clap::Parser;
use log::{info};
use release2gitee::model::Cli;
use release2gitee::{check_cli, sync_github_releases_to_gitee};

// [Rust 中的命令行应用程序](https://cli.rust-lang.net.cn/book/index.html)
fn main() -> anyhow::Result<()> {
    // 参数解析和日志配置
    let cli = &Cli::parse();
    env_logger::Builder::new()
        .filter_level(cli.verbosity.into())
        .format_target(false)
        .init();

    info!("params: {cli}");
    check_cli(cli)?;

    // 同步程序
    sync_github_releases_to_gitee(cli)?;
    info!("finish");
    Ok(())
}
