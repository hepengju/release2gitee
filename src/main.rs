use clap::Parser;
use log::info;
use reqwest::blocking::Client;
use std::time::Duration;
use release2gitee::model::Cli;
use release2gitee::sync_releases;

// [Rust 中的命令行应用程序](https://cli.rust-lang.net.cn/book/index.html)
fn main() -> anyhow::Result<()> {
    // 参数解析
    let cli = &Cli::parse();

    // 日志配置
    env_logger::Builder::new()
        .filter_level(cli.verbosity.log_level_filter())
        .init();
    let client = &Client::builder().timeout(Duration::from_secs(60)).build()?;
    info!("命令行解析完成: {cli}");

    // 同步程序
    sync_releases(client, cli)?;
    info!("同步程序执行完毕");
    Ok(())
}


