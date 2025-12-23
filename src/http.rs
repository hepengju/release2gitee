use std::time::Duration;
use anyhow::bail;
use log::info;
use reqwest::blocking::{Client, Response};

const USER_AGENT: &str = "reqwest";


pub fn init_client() -> anyhow::Result<Client> {
    let client = Client::builder()
        .retry(reqwest::retry::for_host("api.github.com")) // github的查询和下载进行重试
        .timeout(Duration::from_secs(60)).build()?;
    Ok(client)
}

pub fn get(client: &Client, url: &str) -> anyhow::Result<String> {
    info!("GET: {url}");
    let response: Response = client.get(url).header("User-Agent", USER_AGENT).send()?;
    if response.status().is_success() {
        let text = response.text()?;
        Ok(text)
    } else {
        bail!("response err: {:?}", response)
    }
}
