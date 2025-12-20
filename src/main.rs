use anyhow::bail;
use clap::Parser;
use env_logger::Env;
use log::info;
use reqwest::blocking::{Client, Response};
use serde::{Deserialize, Serialize};

const GITHUB_API_URL: &str = "https://api.github.com/repos";
const GITEE_API_URL: &str = "https://gitee.com/api/v5/repos";
const USER_AGENT: &str = "reqwest";

fn main() -> anyhow::Result<()> {
    // 默认日志级别改为INFO
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let cli = &Cli::parse();
    let client = &Client::new();

    // 1.获取github的releases信息
    let github_releases = github_releases(client, cli)?;

    // 2.获取gitee的releases信息
    let gitee_releases = gitee_releases(client, cli)?;

    // 3.只同步缺失的releases信息 (以tag_name为唯一标识)
    let releases = github_releases
        .into_iter()
        .filter(|github_release| {
            gitee_releases
                .iter()
                .find(|gitee_release| github_release.tag_name == gitee_release.tag_name)
                .is_none()
        })
        .collect::<Vec<_>>();
    info!("Gitee中缺失的release个数: {}", releases.len());



    Ok(())
}

fn github_releases(client: &Client, cli: &Cli) -> anyhow::Result<Vec<Release>> {
    let response: Response = client
        .get(format!(
            "{}/{}/{}/releases?per_page={}&page=1",
            GITHUB_API_URL, cli.github_owner, cli.github_repo, cli.lastest_release_count
        ))
        .header("User-Agent", USER_AGENT) // Github要求必须有此请求头
        .send()?;

    if !response.status().is_success() {
        bail!("Github仓库Releases信息获取失败!")
    }

    let result = response.text()?;
    let releases: Vec<Release> = serde_json::from_str(&result)?;
    info!("Github仓库Releases信息获取最近的{}个成功", releases.len());
    Ok(releases)
}

fn gitee_releases(client: &Client, cli: &Cli) -> anyhow::Result<Vec<Release>> {
    let response: Response = client
        .get(format!(
            "{}/{}/{}/releases?per_page=100&page=1", // 最近100个
            GITEE_API_URL, cli.gitee_owner, cli.gitee_repo
        ))
        .send()?;

    if !response.status().is_success() {
        bail!("Gitee仓库Releases信息获取失败!")
    }

    let result = response.text()?;
    let releases: Vec<Release> = serde_json::from_str(&result)?;
    info!("Gitee仓库Releases信息获取最近的{}个成功", releases.len());
    Ok(releases)
}

fn gitee_release_create(client: &Client, cli: &Cli, release: &Release) -> anyhow::Result<()> {
    let response: Response = client
        .post(format!("{}/{}/{}/releases", GITEE_API_URL, cli.gitee_owner, cli.gitee_repo))
        .header("Authorization", format!("token {}", cli.gitee_token))
        .header("Content-Type", "application/json")
        .json(release)
        .send()?;

    if !response.status().is_success() {
        bail!("Gitee仓库Release创建失败: {}!", &release.tag_name)
    }

    info!("Gitee仓库Release创建成功: {}, 开始上传附件!", &release.tag_name);

    // 下载github附件


    // 上传到gitee


    Ok(())
}

#[derive(Parser, Debug)]
pub struct Cli {
    #[clap(env = "github_owner")]
    github_owner: String,

    #[clap(env = "github_repo")]
    github_repo: String,

    #[clap(env = "gitee_owner")]
    gitee_owner: String,

    #[clap(env = "gitee_repo")]
    gitee_repo: String,

    #[clap(env = "gitee_token")]
    gitee_token: String,

    #[clap(default_value_t = 5)]
    lastest_release_count: u8,
}

#[derive(Debug, Deserialize)]
pub struct Assert {
    name: String,
    size: Option<u64>,
    browser_download_url: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Release {
    tag_name: String,
    name: String,
    body: String,
    prerelease: bool,
    target_commitish: String,

    #[serde(skip_serializing)]
    assets: Vec<Assert>,
}
