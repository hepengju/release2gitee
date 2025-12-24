use crate::{AnyResult, replace_download_url};
use anyhow::bail;
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, error, info, log};
use reqwest::blocking::{Client, RequestBuilder, Response, multipart};
use serde::Serialize;
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
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

pub fn download(client: &Client, url: &str, file_path: &PathBuf) -> AnyResult<()> {
    let name = file_path.file_name().unwrap().display();
    info!("downloading: {}, file: {}", url, name);

    let mut res = client
        .get(url)
        .header("User-Agent", reqwest::header::USER_AGENT)
        .send()?;

    if res.status().is_success() {
        // 获取内容长度用于进度条
        let total_size = res.content_length().unwrap_or(0);
        let pb = ProgressBar::new(total_size);
        set_progress_bar_style(&pb)?;

        // 创建文件
        let mut file = File::create(&file_path)?;

        // 下载并更新进度
        // 分块读取、写入并更新进度
        let mut buffer = [0u8; 8192]; // 8KB 缓冲区
        loop {
            let n = res.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            file.write_all(&buffer[..n])?;
            pb.inc(n as u64);
        }
        pb.finish_with_message("");
        Ok(())
    } else {
        bail!("下载文件失败: {}", name);
    }
}



pub fn upload(client: &Client, url: &str, token: &str, file_path: &PathBuf) -> AnyResult<()> {
    let name = file_path.file_name().unwrap().display();
    info!("uploading: {}, file: {}", url, name);

    let form = multipart::Form::new().file("file", file_path)?;

    // 上传文件到Gitee
    let upload_response = client
        .post(url)
        .header("Authorization", format!("token {}", token))
        .multipart(form)
        .send()?;

    if !upload_response.status().is_success() {
        bail!("上传文件失败: {}", file_path.file_name().unwrap().display());
    }
    Ok(())
}

fn set_progress_bar_style(pb: &ProgressBar) -> AnyResult<()> {
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")?
            .progress_chars("#>-"),
    );
    Ok(())
}