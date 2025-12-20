mod api;
mod model;

use crate::api::*;
use crate::model::*;
use clap::Parser;
use env_logger::Env;
use reqwest::blocking::Client;
use std::time::Duration;
use reqwest::retry;

const GITHUB_API_URL: &str = "https://api.github.com/repos";
const GITEE_API_URL: &str = "https://gitee.com/api/v5/repos";
const USER_AGENT: &str = "reqwest";

fn main() -> anyhow::Result<()> {
    // 默认日志级别改为INFO
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let cli = &Cli::parse();

    let client = &Client::builder()
        .timeout(Duration::from_mins(1)).build()?;

    // 1.获取github的releases信息
    let github_releases = github_releases(client, cli)?;

    // 2.获取gitee的releases信息
    let gitee_releases = gitee_releases(client, cli)?;

    // 3.逐个release同步
    for hr in github_releases {
        let er = gitee_releases.iter().find(|gr| gr.tag_name == hr.tag_name);
        sync_gitee_release(client, cli, &hr, er)?;
    }
    Ok(())
}
