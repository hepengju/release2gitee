use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Parser, Debug)]
pub struct Cli {
    #[clap(env = "github_owner")]
    pub github_owner: String,

    #[clap(env = "github_repo")]
    pub github_repo: String,

    #[clap(env = "gitee_owner")]
    pub gitee_owner: String,

    #[clap(env = "gitee_repo")]
    pub gitee_repo: String,

    #[clap(env = "gitee_token")]
    pub gitee_token: String,

    #[clap(default_value_t = 5)]
    pub lastest_release_count: u8,

    #[clap(long)]
    pub skip_release_body_url_replace: bool,

    #[clap(long)]
    pub skip_lastest_json_url_replace: bool,
}

impl Display for Cli {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let masked_token = if self.gitee_token.len() > 8 {
            let prefix = &self.gitee_token[..8];
            let asterisks = "*".repeat(self.gitee_token.len() - 8);
            format!("{}{}", prefix, asterisks)
        } else {
            "*".repeat(self.gitee_token.len())
        };

        write!(
            f,
            "github_owner: {}, github_repo: {}, gitee_owner: {}, gitee_repo: {}, gitee_token: {}, lastest_release_count: {}, skip_release_body_url_replace: {}, skip_lastest_json_url_replace: {}",
            self.github_owner,
            self.github_repo,
            self.gitee_owner,
            self.gitee_owner,
            masked_token,
            self.lastest_release_count,
            self.skip_release_body_url_replace,
            self.skip_lastest_json_url_replace
        )
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
