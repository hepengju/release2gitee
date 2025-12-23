use anyhow::bail;
use log::info;
use reqwest::blocking::{Client, Response};
use std::time::Duration;

const USER_AGENT: &str = "reqwest";

pub fn init_client() -> anyhow::Result<Client> {
    let client = Client::builder()
        .retry(reqwest::retry::for_host("api.github.com")) // github的查询和下载进行重试
        .timeout(Duration::from_secs(60))
        .build()?;
    Ok(client)
}

pub fn get(client: &Client, url: &str) -> anyhow::Result<String> {
    info!("GET: {url}");
    let res = client.get(url).header("User-Agent", USER_AGENT).send()?;
    let text = extract_response_text(res)?;
    Ok(text)
}

fn extract_response_text(res: Response) -> anyhow::Result<String> {
    if res.status().is_success() {
        let text = res.text()?;
        Ok(text)
    } else {
        bail!("response err: {:?}", res)
    }
}
