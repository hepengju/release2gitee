use crate::AnyResult;
use anyhow::bail;
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, info};
use multipart::Part;
use reqwest::blocking::{Client, RequestBuilder, Response, multipart};
use serde::Serialize;
use std::fs::File;
use std::io;
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
    debug!("response: {text}");
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
    info!("downloading: {}", url);

    let mut res = client
        .get(url)
        .header("User-Agent", reqwest::header::USER_AGENT)
        .send()?;

    if res.status().is_success() {
        // 获取内容长度用于进度条
        let total_size = res.content_length().unwrap_or(0);
        let pb = get_progress_bar(total_size)?;

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
        bail!("下载文件失败: {}", file_path.file_name().unwrap().display());
    }
}

pub fn upload(client: &Client, url: &str, token: &str, file_path: &PathBuf) -> AnyResult<()> {
    let name = file_path.file_name().unwrap().display();
    info!("uploading: {}, file: {}", url, name);

    let file = File::open(file_path)?;
    let pb = get_progress_bar(file.metadata().unwrap().len())?;

    // 使用自定义的 ProgressRead 包裹文件读取
    let progress_reader = ProgressRead {
        inner: file,
        progress: pb.clone(),
    };

    // 创建 multipart 表单数据
    let full_name = file_path.display().to_string();
    let form =
        multipart::Form::new().part("file", Part::reader(progress_reader).file_name(full_name));
    // 上传文件到Gitee
    let upload_response = client
        .post(url)
        .header("Authorization", format!("token {}", token))
        .multipart(form)
        .send()?;
    pb.finish_with_message("");

    if !upload_response.status().is_success() {
        bail!("上传文件失败: {}", file_path.file_name().unwrap().display());
    }
    Ok(())
}

fn get_progress_bar(size: u64) -> AnyResult<ProgressBar> {
    let pb = ProgressBar::new(size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{elapsed_precise:.white.dim} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")?
            .progress_chars("#>-"),
    );
    Ok(pb)
}

// 自定义实现 Read 来更新进度条
struct ProgressRead<R> {
    inner: R,
    progress: ProgressBar,
}

impl<R: Read> Read for ProgressRead<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.progress.inc(n as u64);
        }
        Ok(n)
    }
}
