use clap::Parser;
use serde::{Deserialize, Serialize};

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

    #[clap(default_value_t = 10)]
    pub lastest_release_count: u8,

    #[clap(long, default_value_t = true)]
    pub release_body_url_replace: bool,

    #[clap(long, default_value_t = true)]
    pub lastest_json_url_replace: bool,
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
