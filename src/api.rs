use crate::model::{Cli, Release};
use anyhow::bail;
use indicatif::{ProgressBar, ProgressStyle};
use log::{error, info};
use reqwest::blocking::{Client, Response, multipart};
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

const GITHUB_API_URL: &str = "https://api.github.com/repos";
const GITEE_API_URL: &str = "https://gitee.com/api/v5/repos";
const USER_AGENT: &str = "reqwest";

/// 获取Github仓库Releases信息
pub fn github_releases(client: &Client, cli: &Cli) -> anyhow::Result<Vec<Release>> {
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
    let mut releases: Vec<Release> = serde_json::from_str(&result)?;
    releases.reverse();
    info!(
        "Github仓库releases获取最近的{}个成功: {}",
        releases.len(),
        get_tag_names(&releases)
    );
    Ok(releases)
}

/// 获取Gitee仓库Releases信息
pub fn gitee_releases(client: &Client, cli: &Cli) -> anyhow::Result<Vec<Release>> {
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

/// 日志显示tag名称列表
fn get_tag_names(releases: &Vec<Release>) -> String {
    releases
        .iter()
        .map(|release| release.tag_name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// 同步Gitee仓库Release
pub fn sync_gitee_release(
    client: &Client,
    cli: &Cli,
    release: &Release,
    er: Option<&Release>,
) -> anyhow::Result<()> {
    // 如果gitee的release不存在则创建
    let gitee_release = if er.is_none() {
        &gitee_release_create(client, cli, &release)?
    } else {
        er.unwrap()
    };

    // 下载github附件到本地
    download_release_asserts(client, cli, release, gitee_release)?;

    // 上传附件到gitee
    upload_release_asserts(client, cli, release, gitee_release)?;
    Ok(())
}

/// gitee创建Release
fn gitee_release_create(client: &Client, cli: &Cli, release: &Release) -> anyhow::Result<Release> {
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

    let result = response.text()?;
    let release: Release = serde_json::from_str(&result)?;
    info!("Gitee仓库Release创建成功: {}!", &release.tag_name);
    Ok(release)
}

/// 下载附件
fn download_release_asserts(
    client: &Client,
    cli: &Cli,
    release: &Release,
    gitee_release: &Release,
) -> anyhow::Result<()> {
    info!("创建目录: {}", &release.tag_name);
    if !Path::new(&release.tag_name).exists() {
        fs::create_dir(&release.tag_name)?;
    }

    for asset in &release.assets {
        if let Some(_) = gitee_release.assets.iter().find(|a| a.name == asset.name) {
            info!("Gitee附件文件已存在，忽略下载: {}", &asset.name);
            continue;
        }

        // 先判断文件是否存在，存在且大小一致则忽略下载
        let file_path = format!("{}/{}", &release.tag_name, &asset.name);
        if Path::new(&file_path).exists() {
            // 如果文件存在，检查大小是否一致
            if let Some(asset_size) = asset.size {
                if let Ok(metadata) = fs::metadata(&file_path) {
                    if metadata.len() == asset_size {
                        info!("文件已存在且大小一致，跳过下载: {}", &asset.name);
                        continue;
                    }
                }
            }
        }

        info!("开始下载附件: {}", &asset.name);
        let mut response = client
            .get(&asset.browser_download_url)
            .header("User-Agent", USER_AGENT)
            .send()?;

        if response.status().is_success() {
            // 获取内容长度用于进度条
            let total_size = response.content_length().unwrap_or(0);
            let pb = ProgressBar::new(total_size);

            pb.set_style(
                ProgressStyle::with_template(
                    "{elapsed_precise:.white.dim} {wide_bar:.cyan} {bytes}/{total_bytes} ({bytes_per_sec}, {eta})",
                )?.progress_chars("█▉▊▋▌▍▎▏  "),
            );

            // 创建文件
            let mut file = File::create(&file_path)?;

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
            pb.finish_with_message("");
            info!("下载附件成功: {}", &asset.name);

            // 如果是latest.json, 则替换其中的下载地址
            if asset.name == "latest.json" {
                info!("latest.json文件替换里面的下载地址");
                let content = fs::read_to_string(&file_path)?;
                // https://github.com/hepengju/redis-me
                // https://gitee.com/hepengju/redis-me
                let src = format!(
                    "https://github.com/{}/{}",
                    cli.github_owner, cli.github_repo
                );
                let tar = format!("https://gitee.com/{}/{}", cli.gitee_owner, cli.gitee_repo);
                let content = content.replace(&src, &tar);
                fs::write(&file_path, content)?;
            }
        } else {
            error!("下载附件失败: {}", &asset.name);
        }
    }
    Ok(())
}

/// 上传附件
fn upload_release_asserts(
    client: &Client,
    cli: &Cli,
    release: &Release,
    gitee_release: &Release,
) -> anyhow::Result<()> {
    for asset in &release.assets {
        let file_path = &format!("{}/{}", &release.tag_name, &asset.name);

        // 如果文件已存在则跳过上传
        if let Some(_) = gitee_release.assets.iter().find(|a| a.name == asset.name) {
            info!("Gitee附件文件已存在，忽略上传: {}", &asset.name);
            continue;
        }

        // 检查文件是否存在
        if !Path::new(file_path).exists() {
            error!("本地文件不存在，跳过上传: {}", file_path);
            continue;
        }

        // 构造上传URL
        let upload_url = format!(
            "{}/{}/{}/releases/{}/attach_files",
            GITEE_API_URL, cli.gitee_owner, cli.gitee_repo, gitee_release.id,
        );

        let form = multipart::Form::new().file("file", file_path)?;

        // 上传文件到Gitee
        let upload_response = client
            .post(&upload_url)
            .header("Authorization", format!("token {}", cli.gitee_token))
            .multipart(form)
            .send()?;

        if upload_response.status().is_success() {
            info!("上传附件到Gitee成功: {}", &asset.name);
        } else {
            error!("上传附件到Gitee失败: {}", &asset.name);
        }
    }
    Ok(())
}
