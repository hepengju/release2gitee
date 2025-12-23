use crate::AnyResult;
use anyhow::bail;
use log::{debug, info};
use reqwest::blocking::{Client, RequestBuilder, Response};
use serde::Serialize;
use std::time::Duration;

const USER_AGENT: &str = "reqwest";


pub fn init_client() -> AnyResult<Client> {
    let client = Client::builder()
        .retry(reqwest::retry::for_host("api.github.com")) // github的查询和下载进行重试
        .timeout(Duration::from_secs(60))
        .build()?;
    Ok(client)
}

pub fn get(client: &Client, url: &str) -> AnyResult<String> {
    info!("GET: {url}");
    let res = client.get(url).header("User-Agent", USER_AGENT).send()?;
    let text = extract_response_text(res)?;
    debug!("response: {}", text);
    Ok(text)
}

pub fn post<T: Serialize + ?Sized>(
    client: &Client,
    url: &str,
    token: &str,
    json: &T,
) -> AnyResult<String> {
    info!("POST: {url}");
    post_or_patch(client.post(url), token, json)
}

pub fn patch<T: Serialize + ?Sized>(
    client: &Client,
    url: &str,
    token: &str,
    json: &T,
) -> AnyResult<String> {
    info!("PATCH: {url}");
    post_or_patch(client.patch(url), token, json)
}

fn post_or_patch<T: Serialize + ?Sized>(
    builder: RequestBuilder,
    token: &str,
    json: &T,
) -> AnyResult<String> {
    let res = builder
        .header("Authorization", format!("token {}", token))
        .header("User-Agent", USER_AGENT)
        .header("Content-Type", "application/json")
        .json(json)
        .send()?;
    let text = extract_response_text(res)?;
    info!("response: {text}");
    Ok(text)
}

pub fn delete(client: &Client, url: &str, token: &str) -> AnyResult<()> {
    info!("DELETE: {url}");
    let res = client
        .delete(url)
        .header("Authorization", format!("token {}", token))
        .header("User-Agent", USER_AGENT)
        .send()?;
    check_response(res)?;
    Ok(())
}

fn check_response(res: Response) -> AnyResult<()> {
    if res.status().is_success() {
        Ok(())
    } else {
        bail!("response err: {:?}", res)
    }
}

fn extract_response_text(res: Response) -> AnyResult<String> {
    if res.status().is_success() {
        let text = res.text()?;
        Ok(text)
    } else {
        bail!("response err: {:?}", res)
    }
}
