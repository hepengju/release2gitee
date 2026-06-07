extern crate core;

mod http;
pub mod model;

use crate::model::{Assert, Cli, Release};
use log::{error, info};
use reqwest::blocking::Client;
use std::cmp::Ordering::Equal;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
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
    let gitee_releases_before_clean = &gitee_releases(client, cli)?;

    // 3. 获取需要同步的GitHub Releases（全部同步）
    let github_releases_to_sync = github_releases.clone();
    info!("will sync {} releases from github", github_releases_to_sync.len());
    
    // 4. 合并所有需要考虑的Releases（已有的 + 新的），按版本号排序，确定哪些应该带附件
    let mut all_releases = gitee_releases_before_clean.clone();
    all_releases.extend(github_releases_to_sync.clone());
    
    // 按版本号排序（新的在前）
    all_releases.sort_by(|a, b| {
        compare(&b.tag_name, &a.tag_name)
            .unwrap_or(Cmp::Eq)
            .ord()
            .unwrap_or(Equal)
    });
    
    // 去重（以tag_name为准，保留第一个出现的）
    let mut seen_tags = std::collections::HashSet::new();
    all_releases.retain(|r| seen_tags.insert(r.tag_name.clone()));
    
    // 确定哪些tag应该带附件（最新的N个）
    let retain_count = cli.gitee_retain_release_attach_files_count;
    let tags_with_assets: Vec<String> = all_releases
        .iter()
        .take(retain_count)
        .map(|r| r.tag_name.clone())
        .collect();
    
    // 转换为HashSet以提高查找效率
    let tags_with_assets_set: std::collections::HashSet<String> = tags_with_assets.iter().cloned().collect();
    
    info!("will retain assets for {} releases: {:?}", tags_with_assets.len(), tags_with_assets);
    
    // 5. 先清理gitee中旧的release附件(释放空间，避免同步时容量不足)
    //    只保留那些"最终应该带附件"的Release的附件
    clean_oldest_gitee_releases(client, cli, gitee_releases_before_clean, &tags_with_assets_set)?;

    // 6. 清理后重新获取gitee的releases信息（因为清理操作可能改变了release的ID）
    let gitee_releases = &gitee_releases(client, cli)?;

    // 7. 循环release进行对比并同步: 倒序处理, 先同步旧的版本
    for github_release in github_releases_to_sync.iter().rev() {
        let gitee_release = gitee_releases
            .iter()
            .find(|gr| gr.tag_name == github_release.tag_name);
        
        // 判断这个Release是否应该带附件
        let should_have_assets = tags_with_assets_set.contains(&github_release.tag_name);
        
        sync_release_with_asset_control(client, cli, github_release, gitee_release, should_have_assets)?;
    }

    info!("sync success finish");
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

/// 清理Gitee仓库最老的Releases附件: 仅保留指定的tags的附件
/// gitee_releases: 已查询的gitee releases列表，避免重复查询
/// tags_with_assets: 应该保留附件的tag列表（HashSet提高查找效率）
fn clean_oldest_gitee_releases(client: &Client, cli: &Cli, gitee_releases: &[Release], tags_with_assets: &std::collections::HashSet<String>) -> AnyResult<()> {
    info!("clean gitee releases assets");

    let release_count = gitee_releases.len();
    
    info!("gitee releases count: {release_count}, will retain assets for {} releases", tags_with_assets.len());

    // 删除不在tags_with_assets列表中的Release的附件
    for release in gitee_releases {
        if !tags_with_assets.contains(&release.tag_name) && !release.assets.is_empty() {
            // 先删除Release（会同时删除附件），然后重新创建不带附件的Release
            let tag_name = release.tag_name.clone();
            let name = release.name.clone();
            let body = release.body.clone();
            let prerelease = release.prerelease;
            let target_commitish = release.target_commitish.clone();
            
            // 删除旧的Release
            gitee_release_delete(client, cli, release.id)?;
            info!("gitee release deleted (with assets): {}", tag_name);
            
            // 睡眠1秒，避免删除后立即创建导致问题
            thread::sleep(Duration::from_secs(1));
            
            // 重新创建不带附件的Release
            let new_release = Release {
                id: 0, // 创建时会分配新ID
                tag_name: tag_name.clone(),
                name,
                body,
                prerelease,
                target_commitish,
                assets: vec![], // 清空附件
            };
            gitee_release_create(client, cli, &new_release)?;
            info!("gitee release recreated (without assets): {}", tag_name);
        }
    }

    Ok(())
}

/// 同步Gitee仓库Release（可控制是否上传附件）
pub fn sync_release_with_asset_control(
    client: &Client,
    cli: &Cli,
    release: &Release,
    er: Option<&Release>,
    should_upload_assets: bool,
) -> AnyResult<()> {
    // 如果gitee的release不存在则创建, 存在且内容不一致则更新, 否则无需处理
    let gitee_release = &gitee_release_create_or_update(client, cli, release, er)?;

    // 如果不应该上传附件，直接返回
    if !should_upload_assets {
        let tag_name = &release.tag_name;
        info!("gitee release created/updated without assets: {tag_name}");
        return Ok(());
    }

    // 如果gitee的release 和 github的release的附件完全一致，则无需处理
    let diff_asserts = &release_asserts_diff(release, gitee_release);
    if diff_asserts.is_empty() {
        let tag_name = &release.tag_name;
        info!("gitee/github release asserts is some: {tag_name}",);
        return Ok(());
    }

    // 下载github附件到本地
    download_release_asserts(client, cli, release, diff_asserts)?;

    // 上传附件到gitee
    upload_release_asserts(client, cli, release, gitee_release, diff_asserts)?;
    Ok(())
}

/// 同步Gitee仓库Release（默认上传附件）
pub fn sync_release(
    client: &Client,
    cli: &Cli,
    release: &Release,
    er: Option<&Release>,
) -> AnyResult<()> {
    sync_release_with_asset_control(client, cli, release, er, true)
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
                body: Some(new_body), // 使用替换后的body
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
    
    // 替换body中的URL
    let new_body = replace_release_body_url(cli, release.body.clone().unwrap_or_default());
    let release_with_replaced_body = Release {
        id: release.id,
        tag_name: release.tag_name.clone(),
        name: release.name.clone(),
        body: Some(new_body),
        prerelease: release.prerelease,
        target_commitish: release.target_commitish.clone(),
        assets: release.assets.clone(),
    };
    
    let result = http::post(client, &url, &cli.gitee_token, &release_with_replaced_body)?;
    let release: Release = serde_json::from_str(&result)?;
    info!("gitee release create success: {}!", &release.tag_name);
    
    // 睡眠3秒，避免创建速度太快导致顺序混乱
    thread::sleep(Duration::from_secs(3));
    
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
                        info!(
                            "file exists and size is some, skip download: {}",
                            &asset.name
                        );
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
