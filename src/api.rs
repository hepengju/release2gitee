use crate::model::{Cli, Release};
use crate::{GITEE_API_URL, GITHUB_API_URL, USER_AGENT};
use anyhow::bail;
use indicatif::ProgressBar;
use log::{error, info};
use reqwest::blocking::{Client, Response, multipart};
use std::fs;
use std::fs::{File, write};
use std::io::{Read, Write};
use std::path::Path;

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
    let releases: Vec<Release> = serde_json::from_str(&result)?;
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
    // 下载github附件到本地
    download_release_asserts(client, release)?;

    // 如果gitee的release不存在则创建
    if er.is_none() {
        gitee_release_create(client, cli, &release)?;
    }

    // 上传附件到gitee
    upload_release_asserts(client, cli, &release)?;
    Ok(())
}

/// gitee创建Release
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

    info!("Gitee仓库Release创建成功: {}!", &release.tag_name);
    Ok(())
}

/// 下载Github Release的附件
fn download_release_asserts(client: &Client, release: &Release) -> anyhow::Result<()> {
    info!("创建目录: {}", &release.tag_name);
    fs::create_dir(&release.tag_name)?;

    info!("开始下载附件");
    for asset in &release.assets {
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

        // 获取内容长度用于进度条
        let total_size = response.content_length().unwrap_or(0);
        let pb = ProgressBar::new(total_size);

        // 创建文件
        let mut file = File::create(file_path)?;

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
    Ok(())
}

// 上传Github Release的附件
fn upload_release_asserts(client: &Client, cli: &Cli, release: &Release) -> anyhow::Result<()> {
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
