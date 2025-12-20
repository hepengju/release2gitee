use anyhow::bail;
use clap::Parser;
use env_logger::Env;
use indicatif::ProgressBar;
use log::{error, info};
use reqwest::blocking::{Client, Response, multipart};
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::{File, write};
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

const GITHUB_API_URL: &str = "https://api.github.com/repos";
const GITEE_API_URL: &str = "https://gitee.com/api/v5/repos";
const USER_AGENT: &str = "reqwest";

fn main() -> anyhow::Result<()> {
    // 默认日志级别改为INFO
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let cli = &Cli::parse();
    let client = &Client::builder().timeout(Duration::from_mins(1)).build()?;

    // 1.获取github的releases信息
    let github_releases = github_releases(client, cli)?;

    // 2.获取gitee的releases信息
    let gitee_releases = gitee_releases(client, cli)?;

    // 3.只同步缺失的releases信息 (以tag_name为唯一标识)
    let releases = new_releases(github_releases, gitee_releases);

    // 4.逐个创建gitee的release信息并上传附件
    for release in releases {
        gitee_release_create(client, cli, &release)?;
    }
    Ok(())
}

fn new_releases(github_releases: Vec<Release>, gitee_releases: Vec<Release>) -> Vec<Release> {
    let releases = github_releases
        .into_iter()
        .filter(|github_release| {
            gitee_releases
                .iter()
                .find(|gitee_release| github_release.tag_name == gitee_release.tag_name)
                .is_none()
        })
        .collect::<Vec<_>>();
    info!("Gitee中缺失的releases个数: {}", releases.len());
    releases
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
        bail!("Github仓库releases获取失败!")
    }

    let result = response.text()?;
    let releases: Vec<Release> = serde_json::from_str(&result)?;
    info!(
        "Github仓库releases获取最近的{}个成功: {}",
        releases.len(),
        get_tag_names(&releases)
    );
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
        bail!("Gitee仓库releases信息获取失败!")
    }

    let result = response.text()?;
    let releases: Vec<Release> = serde_json::from_str(&result)?;
    info!(
        "Gitee仓库releases信息获取{}个: {}",
        releases.len(),
        get_tag_names(&releases)
    );
    Ok(releases)
}

fn get_tag_names(releases: &Vec<Release>) -> String {
    releases
        .iter()
        .map(|release| release.tag_name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn gitee_release_create(client: &Client, cli: &Cli, release: &Release) -> anyhow::Result<()> {
    let response: Response = client
        .post(format!(
            "{}/{}/{}/releases",
            GITEE_API_URL, cli.gitee_owner, cli.gitee_repo
        ))
        .header("Authorization", format!("token {}", cli.gitee_token))
        .header("Content-Type", "application/json")
        .json(release)
        .send()?;

    if !response.status().is_success() {
        bail!("Gitee仓库Release创建失败: {}!", &release.tag_name)
    }

    info!(
        "Gitee仓库Release创建成功: {}, 开始处理附件!",
        &release.tag_name
    );

    // 下载github附件到本地
    for asset in &release.assets {
        // 发送请求
        let mut response = client
            .get(asset.browser_download_url.as_str())
            .header("User-Agent", USER_AGENT)
            .send()?;

        // 获取内容长度用于进度条
        let total_size = response.content_length().unwrap_or(0);
        let pb = ProgressBar::new(total_size);

        // 创建文件
        let mut file = File::create(asset.name.as_str())?;

        // 下载并更新进度
        // 分块读取、写入并更新进度
        let mut buffer = [0u8; 8192]; // 8KB 缓冲区
        loop {
            let n = response.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            file.write_all(&buffer[..n])?;
            pb.inc(n as u64);
        }

        pb.finish_with_message("下载完成");

        let response = client
            .get(&asset.browser_download_url)
            .header("User-Agent", USER_AGENT)
            .send()?;

        if response.status().is_success() {
            let file_content = response.bytes()?;
            write(&asset.name, file_content)?;
            info!("下载附件成功: {}", &asset.name);
        } else {
            error!("下载附件失败: {}", &asset.name);
        }
    }

    // 上传到gitee
    for asset in &release.assets {
        // 检查文件是否存在
        if !Path::new(&asset.name).exists() {
            error!("本地文件不存在，跳过上传: {}", &asset.name);
            continue;
        }

        // 构造上传URL
        let upload_url = format!(
            "{}/{}/{}/releases/{}/attach_files",
            GITEE_API_URL, cli.gitee_owner, cli.gitee_repo, release.tag_name,
        );

        let form = multipart::Form::new().file("file", &asset.name)?;

        // 上传文件到Gitee
        let upload_response = client
            .post(&upload_url)
            .header("Authorization", format!("token {}", cli.gitee_token))
            .header("Content-Type", "application/octet-stream")
            .multipart(form)
            .send()?;

        if upload_response.status().is_success() {
            info!("上传附件到Gitee成功: {}", &asset.name);

            // 删除本地临时文件
            if let Err(e) = fs::remove_file(&asset.name) {
                error!("删除临时文件失败: {}, 错误: {}", &asset.name, e);
            }
        } else {
            error!("上传附件到Gitee失败: {}", &asset.name,);
        }
    }

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

    #[clap(default_value_t = 1)]
    lastest_release_count: u8,
}

#[derive(Debug, Deserialize)]
pub struct Assert {
    name: String,
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
