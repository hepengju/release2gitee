use clap::Parser;
use clap_verbosity_flag::{InfoLevel, Verbosity};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

/// sync github releases to gitee releases
#[derive(Parser, Debug)]
#[command(version, author, about, long_about = None)]
pub struct Cli {
    #[clap(long, env)]
    pub github_owner: String,

    #[clap(long, env)]
    pub github_repo: String,

    #[clap(long, env)]
    pub github_token: Option<String>,

    #[clap(long, env)]
    pub gitee_owner: String,

    #[clap(long, env)]
    pub gitee_repo: String,

    #[clap(long, env)]
    pub gitee_token: String,

    // {github_api}/repos/{owner}/{repo}/releases?per_page={}&page=1
    // github查询最新的N个Releases
    #[clap(
        long,
        env = "release2gitee__github_latest_release_count",
        default_value_t = 5
    )]
    pub github_latest_release_count: usize,

    // gitee保留最近的N个Release(空间容量限制)
    #[clap(
        long,
        env = "release2gitee__gitee_retain_release_count",
        default_value_t = 999
    )]
    pub gitee_retain_release_count: usize,

    // 是否将release body中的github仓库url替换为gitee仓库url
    #[clap(
        long,
        env = "release2gitee__release_body_url_replace",
        default_value_t = true
    )]
    pub release_body_url_replace: bool,

    // 是否将latest.json文件中的github仓库url替换为gitee仓库url（Tauri应用的自动更新依赖文件）
    #[clap(
        long,
        env = "release2gitee__latest_json_url_replace",
        default_value_t = true
    )]
    pub latest_json_url_replace: bool,

    #[command(flatten)]
    pub verbosity: Verbosity<InfoLevel>,
}

impl Display for Cli {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {

        write!(
            f,
            "github_owner: {}, github_repo: {}, github_token: {}, gitee_owner: {}, gitee_repo: {}, gitee_token: {}, github_latest_release_count: {}, gitee_retain_release_count: {}, release_body_url_replace: {}, latest_json_url_replace: {}",
            self.github_owner,
            self.github_repo,
            mask_token(self.github_token.clone()),
            self.gitee_owner,
            self.gitee_repo,
            mask_token(Some(self.gitee_token.clone())),
            self.github_latest_release_count,
            self.gitee_retain_release_count,
            self.release_body_url_replace,
            self.latest_json_url_replace
        )
    }
}

fn mask_token(token: Option<String>) -> String {
    if token.is_none() {
        return "None".to_string();
    }

    let token = token.unwrap();
    if token.len() > 8 {
        let prefix = &token[..8];
        let asterisks = "*".repeat(token.len() - 8);
        format!("{}{}", prefix, asterisks)
    } else {
        "*".repeat(token.len())
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Assert {
    pub name: String,
    pub size: Option<u64>,
    pub browser_download_url: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Release {
    pub id: u64,
    pub tag_name: String,
    pub name: String,
    pub body: String,
    pub prerelease: bool,
    pub target_commitish: String,

    #[serde(skip_serializing)]
    pub assets: Vec<Assert>,
}
