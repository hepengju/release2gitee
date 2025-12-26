extern crate core;

mod http;
pub mod model;

use crate::model::{Assert, Cli, Release};
use log::{error, info, warn};
use reqwest::blocking::Client;
use std::cmp::Ordering::Equal;
use std::path::{Path, PathBuf};
use std::{env, fs};
use version_compare::{Cmp, compare};

const GITHUB_API_URL: &str = "https://api.github.com/repos";
const GITEE_API_URL: &str = "https://gitee.com/api/v5/repos";
pub type AnyResult<T> = anyhow::Result<T>;

pub fn sync_github_releases_to_gitee(cli: &Cli) -> AnyResult<()> {
    // http请求较多，复用client
    let client = &http::init_client()?;

    // 1. 获取github的releases信息: 新的在前面
    let github_releases = &github_releases(client, cli)?;

    // 2. 获取gitee的releases信息: 新的在前面
    let gitee_releases = &gitee_releases(client, cli)?;

    // 3. 计算哪些版本需要同步: ①保留前几个 ②比gitee最新版本小的忽略同步
    let github_releases = filter_github_releases(cli, &gitee_releases, github_releases);

    // 4. 循环release进行对比并同步: 倒序处理, 先同步旧的版本
    for github_release in github_releases.iter().rev() {
        let gitee_release = gitee_releases
            .iter()
            .find(|gr| gr.tag_name == github_release.tag_name);
        sync_release(client, cli, github_release, gitee_release)?;
    }

    // 5. 清理gitee中旧的release(免费的容量空间有限)
    clean_oldest_gitee_releases(client, cli)?;
    Ok(())
}

/// 获取Github仓库Releases信息
pub fn github_releases(client: &Client, cli: &Cli) -> AnyResult<Vec<Release>> {
    let url = format!(
        "{}/{}/{}/releases?per_page={}&page=1",
        GITHUB_API_URL, cli.github_owner, cli.github_repo, cli.github_latest_release_count
    );
    let result = http::get(client, &url, cli.github_token.clone())?;
    let mut releases: Vec<Release> = serde_json::from_str(&result)?;
    releases.sort_by_key(|r| r.id);
    releases.reverse(); // 倒序, 这样保证同步到gitee时，先处理旧的，再处理新的

    // 如果body为空则设置为tag_name
    for release in releases.iter_mut() {
        if release.body.clone().unwrap_or_default().is_empty() {
            release.body = Some(release.tag_name.clone());
        }
    }

    // 记录日志
    let tag_names = get_tags(&releases);
    info!(
        "github releases fetch {}: {}",
        releases.len(),
        tag_names.join(", ")
    );
    Ok(releases)
}

/// 获取Gitee仓库Releases信息
pub fn gitee_releases(client: &Client, cli: &Cli) -> AnyResult<Vec<Release>> {
    let url = format!(
        "{}/{}/{}/releases?per_page=100&page=1", // 最近100个
        GITEE_API_URL, cli.gitee_owner, cli.gitee_repo
    );
    let result = http::get(client, &url, Some(cli.gitee_token.clone()))?;
    let mut releases: Vec<Release> = serde_json::from_str(&result)?;
    releases.sort_by_key(|r| r.id);
    releases.reverse();

    // 记录日志
    let tag_names = get_tags(&releases);
    info!(
        "gitee releases fetch {}: {}",
        releases.len(),
        tag_names.join(", ")
    );
    Ok(releases)
}

/// 日志显示tag名称列表
fn get_tags(releases: &Vec<Release>) -> Vec<String> {
    releases
        .iter()
        .map(|release| release.tag_name.clone())
        .collect::<Vec<_>>()
}

/// 清理Gitee仓库最老的Releases: 查询最近100个，仅保留最新的N个
fn clean_oldest_gitee_releases(
    client: &Client,
    cli: &Cli,
) -> AnyResult<()> {
    // 重新查询后清理
    let gitee_releases = gitee_releases(client, cli)?;

    // 新同步的个数: github有，gitee没有的tag
    if cli.gitee_retain_release_count >= gitee_releases.len() {
        info!("gitee releases , no need to clean");
    } else {
        let clean_count = gitee_releases.len() + cli.gitee_retain_release_count;
        info!(
            "gitee releases: {}个, need clean count: {}",
            gitee_releases.len(),
            clean_count
        );

        let skip_count = cli.gitee_retain_release_count;
        for release in gitee_releases.iter().skip(skip_count) {
            gitee_release_delete(client, cli, release.id)?;
            info!("gitee release delete success: {}", release.tag_name);
        }
    }

    Ok(())
}

/// 过滤Github仓库Release: 仅保留最新的N个, 且过滤掉版本小的
fn filter_github_releases(
    cli: &Cli,
    gitee_releases: &Vec<Release>,
    github_releases: &Vec<Release>,
) -> Vec<Release> {
    let mut retain_github_releases = github_releases.clone();

    // 仅保留最新的N个用于同步
    if cli.gitee_retain_release_count > retain_github_releases.len() {
        retain_github_releases = retain_github_releases
            .into_iter()
            .take(cli.gitee_retain_release_count)
            .collect();
    }

    // 计算gitee中最大的版本并输出（以tag_name为依据, version-compare的方法）
    if cli.ignore_lt_gitee_max_version && !gitee_releases.is_empty() {
        // 找到Gitee中版本最大的tag
        if let Some(max_gitee_tag) = gitee_releases
            .iter()
            .map(|release| &release.tag_name)
            .max_by(|a, b| compare(&a, &b).unwrap_or(Cmp::Eq).ord().unwrap_or(Equal))
        {
            info!("gitee max_tag_name: {}", max_gitee_tag);

            // 过滤github中版本小的，并打印日志
            retain_github_releases = retain_github_releases
                .into_iter()
                .filter(|release| {
                    match compare(&max_gitee_tag, &release.tag_name) {
                        Ok(ord) => {
                            if ord == Cmp::Gt || ord == Cmp::Eq {
                                info!(
                                    "github tag_name: {} <= {}, ignore sync",
                                    release.tag_name, max_gitee_tag
                                );
                                false
                            } else {
                                true
                            }
                        }
                        Err(_) => {
                            // 如果版本号比较失败，保留该发布（以防无法比较的情况）
                            warn!("compare version error: {} and {}", release.tag_name, max_gitee_tag);
                            true
                        }
                    }
                })
                .collect();
        }
    }

    info!(
        "github releases retain count: {}",
        retain_github_releases.len()
    );
    retain_github_releases
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
        info!("gitee/github release asserts is some: {tag_name}!",);
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
        let new_body = replace_release_body_url(cli, release.body.clone().unwrap_or_default());

        if release.name != er.name
            || new_body != er.body.clone().unwrap_or_default()
            || release.prerelease != er.prerelease
        //|| release.target_commitish != er.target_commitish
        //  ==> 某些场景下github返回的releases中target_commitish为master, 而gitee返回的为具体哈希值导致永远不一致，因此注释掉
        {
            // gitee不允许body为空，因此如果body为空则使用tag_name
            let new_er = Release {
                id: er.id,
                tag_name: er.tag_name.clone(),
                assets: er.assets.clone(),
                name: release.name.clone(),
                body: release.body.clone(),
                prerelease: release.prerelease.clone(),
                target_commitish: release.target_commitish.clone(),
            };
            gitee_release_update(client, cli, &new_er)?;
            Ok(new_er)
        } else {
            info!(
                "gitee/github release name/body/prerelease is some: {}!",
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
    info!("gitee release update success: {}!", &release.tag_name);
    Ok(())
}

fn gitee_release_create(client: &Client, cli: &Cli, release: &Release) -> AnyResult<Release> {
    let url = format!(
        "{}/{}/{}/releases",
        GITEE_API_URL, cli.gitee_owner, cli.gitee_repo
    );
    let result = http::post(client, &url, &cli.gitee_token, release)?;
    let release: Release = serde_json::from_str(&result)?;
    info!("gitee release create success: {}!", &release.tag_name);
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
                        info!("file exists and size is some, skip download: {}", &asset.name);
                        continue;
                    }
                }
            }
        }

        http::download(client, &asset.browser_download_url, &file_path)?;

        // 如果是latest.json, 则替换其中的下载地址
        if cli.latest_json_url_replace && asset.name == "latest.json" {
            let content = fs::read_to_string(&file_path)?;
            let content = replace_download_url(cli, content);
            fs::write(&file_path, content)?;
            info!("latest.json's content is replaced (download url)");
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
            error!("local file not exits, skip upload: {}", file_path.display());
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
        info!("tmp dir create: {}", &tmp_dir.display())
    } else {
        info!("tmp dir exits: {}", &tmp_dir.display());
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
