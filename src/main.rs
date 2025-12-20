use clap::Parser;
use env_logger::Env;
use log::{info};

fn main() {
    // 默认日志级别改为INFO
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();
    info!("配置: {cli:?}");
}

#[derive(Parser, Debug)]
pub struct Cli {
    #[clap(env="github_owner")]
    github_owner: String,

    #[clap(env="github_repo")]
    github_repo: String,

    #[clap(env="gitee_owner")]
    gitee_owner: String,

    #[clap(env="gitee_repo")]
    gitee_repo: String,

    #[clap(env="gitee_token")]
    gitee_token: String,
}
