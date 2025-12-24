extern crate core;

mod http;
pub mod model;

use crate::model::{Assert, Cli, Release};
use anyhow::bail;
use log::{error, info};
use reqwest::blocking::{Client};
use std::path::{Path, PathBuf};
use std::{env, fs};

const GITHUB_API_URL: &str = "https://api.github.com/repos";
const GITEE_API_URL: &str = "https://gitee.com/api/v5/repos";
pub type AnyResult<T> = anyhow::Result<T>;

pub fn check_cli(cli: &Cli) -> AnyResult<()> {
    if cli.github_latest_release_count < 1 {
        bail!("github_latest_release_count must be greater than 0.")
    }

    if cli.gitee_retain_release_count < 1 {
        bail!("gitee_retain_release_count must be greater than 0.")
    }

    if cli.gitee_retain_release_count < cli.github_latest_release_count {
        bail!(
            "gitee_retain_release_count ({}) must be greater than or equal to github_latest_release_count ({}).",
            cli.gitee_retain_release_count,
            cli.github_latest_release_count
        )
    }
    Ok(())
}

pub fn sync_github_releases_to_gitee(cli: &Cli) -> AnyResult<()> {
    // http请求较多，复用client
    let client = &http::init_client()?;

    // 1. 获取github的releases信息: 新的在前面
    let github_releases = &github_releases(client, cli)?;

    // 2. 获取gitee的releases信息: 新的在前面
    let gitee_releases = &gitee_releases(client, cli)?;

    // 3. 清理gitee中旧的release(免费的容量空间有限)
    clean_oldest_gitee_releases(client, cli, gitee_releases)?;

    // 4. 循环release进行对比并同步: 倒序处理, 先同步旧的版本
    for github_release in github_releases.iter().rev() {
        let gitee_release = gitee_releases
            .iter()
            .find(|gr| gr.tag_name == github_release.tag_name);
        sync_release(client, cli, github_release, gitee_release)?;
    }
    Ok(())
}

/// 获取Github仓库Releases信息
pub fn github_releases(client: &Client, cli: &Cli) -> AnyResult<Vec<Release>> {
    let url = format!(
        "{}/{}/{}/releases?per_page={}&page=1",
        GITHUB_API_URL, cli.github_owner, cli.github_repo, cli.github_latest_release_count
    );
    let result = http::get(client, &url)?;
    let mut releases: Vec<Release> = serde_json::from_str(&result)?;
    releases.sort_by_key(|r| r.id);
    releases.reverse(); // 倒序, 这样保证同步到gitee时，先处理旧的，再处理新的

    // 记录日志
    let tag_names = get_tag_names(&releases);
    info!("github releases最近的{}个成功: {tag_names}", releases.len());
    Ok(releases)
}

/// 获取Gitee仓库Releases信息
pub fn gitee_releases(client: &Client, cli: &Cli) -> AnyResult<Vec<Release>> {
    let url = format!(
        "{}/{}/{}/releases?per_page=100&page=1", // 最近100个
        GITEE_API_URL, cli.gitee_owner, cli.gitee_repo
    );
    let result = http::get(client, &url)?;
    let mut releases: Vec<Release> = serde_json::from_str(&result)?;
    releases.sort_by_key(|r| r.id);
    releases.reverse();

    // 记录日志
    let tag_names = get_tag_names(&releases);
    info!("gitee releases获取到{}个: {}", releases.len(), tag_names);
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

/// 清理Gitee仓库最老的Releases: 查询最近100个，仅保留最新的N个
fn clean_oldest_gitee_releases(
    client: &Client,
    cli: &Cli,
    releases: &Vec<Release>,
) -> AnyResult<()> {
    if cli.gitee_retain_release_count >= releases.len() {
        info!("gitee releases 无需清理");
        return Ok(());
    } else {
        let clean_count = releases.len() - cli.gitee_retain_release_count;
        info!(
            "gitee releases: {}个, 需清理: {}个",
            releases.len(),
            clean_count
        );
        for release in releases.iter().skip(cli.gitee_retain_release_count) {
            gitee_release_delete(client, cli, release.id)?;
            info!("gitee release删除成功: {}", release.tag_name);
        }
    }
    Ok(())
}

/// 同步Gitee仓库Release
pub fn sync_release(
    client: &Client,
    cli: &Cli,
    release: &Release,
    er: Option<&Release>,
) -> AnyResult<()> {
    // 如果gitee的release不存在则创建, 存在且内容不一致则更新, 否则无需处理
    let gitee_release = &gitee_release_create_or_update(client, cli, release, er)?;

    // 如果gitee的release 和 github的release的附件完全一致，则无需处理
    let diff_asserts = &release_asserts_diff(release, gitee_release);
    if diff_asserts.is_empty() {
        let tag_name = &release.tag_name;
        info!("gitee release与github release附件相同: {tag_name}!",);
        return Ok(());
    }

    // 下载github附件到本地
    download_release_asserts(client, cli, release, diff_asserts)?;

    // 上传附件到gitee
    upload_release_asserts(client, cli, release, gitee_release, diff_asserts)?;
    Ok(())
}

fn gitee_release_delete(client: &Client, cli: &Cli, id: u64) -> AnyResult<()> {
    let url = format!(
        "{}/{}/{}/releases/{}",
        GITEE_API_URL, cli.gitee_owner, cli.gitee_repo, id
    );
    http::delete(client, &url, &cli.gitee_token)
}

fn gitee_release_create_or_update(
    client: &Client,
    cli: &Cli,
    release: &Release,
    gitee_release: Option<&Release>,
) -> AnyResult<Release> {
    if gitee_release.is_none() {
        Ok(gitee_release_create(client, cli, &release)?)
    } else {
        let er = gitee_release.unwrap();
        let new_body = replace_release_body_url(cli, release.body.clone());

        if release.name != er.name
            || new_body != er.body
            || release.prerelease != er.prerelease
            || release.target_commitish != er.target_commitish
        {
            let new_er = Release {
                id: er.id,
                tag_name: er.tag_name.clone(),
                assets: er.assets.clone(),
                name: release.name.clone(),
                body: new_body,
                prerelease: release.prerelease.clone(),
                target_commitish: release.target_commitish.clone(),
            };
            gitee_release_update(client, cli, &new_er)?;
            Ok(new_er)
        } else {
            info!(
                "gitee release与github release信息相同: {}!",
                &release.tag_name
            );
            Ok(er.clone())
        }
    }
}

fn gitee_release_update(client: &Client, cli: &Cli, er: &Release) -> AnyResult<()> {
    let url = format!(
        "{}/{}/{}/releases/{}",
        GITEE_API_URL, cli.gitee_owner, cli.gitee_repo, er.id
    );
    let result = http::patch(client, &url, &cli.gitee_token, er)?;
    let release: Release = serde_json::from_str(&result)?;
    info!("gitee release更新成功: {}!", &release.tag_name);
    Ok(())
}

fn gitee_release_create(client: &Client, cli: &Cli, release: &Release) -> AnyResult<Release> {
    let url = format!(
        "{}/{}/{}/releases",
        GITEE_API_URL, cli.gitee_owner, cli.gitee_repo
    );
    let result = http::post(client, &url, &cli.gitee_token, release)?;
    let release: Release = serde_json::from_str(&result)?;
    info!("gitee release创建成功: {}!", &release.tag_name);
    Ok(release)
}

/// 寻找附件差异: Github附件有，但Gitee没有的
fn release_asserts_diff(release: &Release, gitee_release: &Release) -> Vec<Assert> {
    let mut diff_assets = Vec::new();
    for asset in &release.assets {
        if !gitee_release
            .assets
            .iter()
            .any(|gitee_asset| gitee_asset.name == asset.name)
        {
            diff_assets.push(asset.clone());
        }
    }
    diff_assets
}

/// 下载附件
fn download_release_asserts(
    client: &Client,
    cli: &Cli,
    release: &Release,
    diff_asserts: &Vec<Assert>,
) -> AnyResult<()> {
    let tmp_dir = tmp_dir_repo_tag(cli, release)?;

    for asset in diff_asserts {
        // 先判断文件是否存在，存在且大小一致则忽略下载
        let file_path = tmp_dir.join(&asset.name);
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
        http::download(client, &asset.browser_download_url, &file_path)?;

        // 如果是latest.json, 则替换其中的下载地址
        if cli.latest_json_url_replace && asset.name == "latest.json" {
            info!("latest.json文件替换里面的下载地址");
            let content = fs::read_to_string(&file_path)?;
            let content = replace_download_url(cli, content);
            fs::write(&file_path, content)?;
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
    diff_asserts: &Vec<Assert>,
) -> AnyResult<()> {
    let tmp_dir = tmp_dir_repo_tag(cli, release)?;

    for asset in diff_asserts {
        //let file_path = &format!("{}/{}", &release.tag_name, &asset.name);
        let file_path = tmp_dir.join(&asset.name);

        // 检查文件是否存在
        if !file_path.exists() {
            error!("本地文件不存在，跳过上传: {}", file_path.display());
            continue;
        }

        // 构造上传URL
        let upload_url = format!(
            "{}/{}/{}/releases/{}/attach_files",
            GITEE_API_URL, cli.gitee_owner, cli.gitee_repo, gitee_release.id,
        );
        http::upload(client, &upload_url, &cli.gitee_token, &file_path)?;
    }
    Ok(())
}

/// 创建临时目录: ~/tmp/github_repo/tag_name
fn tmp_dir_repo_tag(cli: &Cli, release: &Release) -> AnyResult<PathBuf> {
    let mut tmp_dir = env::temp_dir();
    tmp_dir.push(cli.github_repo.clone());
    tmp_dir.push(release.tag_name.clone());

    if !tmp_dir.exists() {
        fs::create_dir_all(&tmp_dir)?;
        info!("创建临时目录: {}", &tmp_dir.display())
    } else {
        info!("临时目录已存在: {}", &tmp_dir.display());
    }
    Ok(tmp_dir)
}

// 替换下载地址
fn replace_download_url(cli: &Cli, content: String) -> String {
    // https://github.com/hepengju/redis-me
    // https://gitee.com/hepengju/redis-me
    let src = format!(
        "https://github.com/{}/{}",
        cli.github_owner, cli.github_repo
    );
    let tar = format!("https://gitee.com/{}/{}", cli.gitee_owner, cli.gitee_repo);
    let content = content.replace(&src, &tar);
    content
}

fn replace_release_body_url(cli: &Cli, content: String) -> String {
    if cli.release_body_url_replace {
        replace_download_url(cli, content)
    } else {
        content
    }
}
