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

pub fn get(client: &Client, url: &str, token: Option<String>) -> AnyResult<String> {
    info!("GET: {url}");
    let mut builder = client.get(url).header("User-Agent", USER_AGENT);
    if token.is_some() {
        // 可选设置github_token. 速率: 50 次/小时  ==> 3000 次/小时
        builder = builder.header("Authorization", format!("token {}", token.unwrap()));
    }
    let res = builder.send()?;
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
    debug!("param: {}", serde_json::to_string(json)?);
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
    let text = extract_response_text(res)?;
    debug!("response: {text}");
    Ok(())
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
        bail!("download file error: {}", file_path.file_name().unwrap().display());
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
        bail!("upload file error: {}", file_path.file_name().unwrap().display());
    }
    Ok(())
}

fn get_progress_bar(size: u64) -> AnyResult<ProgressBar> {
    let pb = ProgressBar::new(size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{elapsed_precise:.white.dim} [{wide_bar:.cyan}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")?
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

#[cfg(test)]
mod tests {
    use version_compare::Version;
    use super::*;
    use crate::model::Release;

    #[test]
    fn test_get() -> AnyResult<()> {
        // 测试反序列化失败（body为null, 需要定义为Option<T>）
        let result = r#"[{"url":"https://api.github.com/repos/hepengju/release2gitee/releases/272775542","assets_url":"https://api.github.com/repos/hepengju/release2gitee/releases/272775542/assets","upload_url":"https://uploads.github.com/repos/hepengju/release2gitee/releases/272775542/assets{?name,label}","html_url":"https://github.com/hepengju/release2gitee/releases/tag/v0.9.4","id":272775542,"author":{"login":"github-actions[bot]","id":41898282,"node_id":"MDM6Qm90NDE4OTgyODI=","avatar_url":"https://avatars.githubusercontent.com/in/15368?v=4","gravatar_id":"","url":"https://api.github.com/users/github-actions%5Bbot%5D","html_url":"https://github.com/apps/github-actions","followers_url":"https://api.github.com/users/github-actions%5Bbot%5D/followers","following_url":"https://api.github.com/users/github-actions%5Bbot%5D/following{/other_user}","gists_url":"https://api.github.com/users/github-actions%5Bbot%5D/gists{/gist_id}","starred_url":"https://api.github.com/users/github-actions%5Bbot%5D/starred{/owner}{/repo}","subscriptions_url":"https://api.github.com/users/github-actions%5Bbot%5D/subscriptions","organizations_url":"https://api.github.com/users/github-actions%5Bbot%5D/orgs","repos_url":"https://api.github.com/users/github-actions%5Bbot%5D/repos","events_url":"https://api.github.com/users/github-actions%5Bbot%5D/events{/privacy}","received_events_url":"https://api.github.com/users/github-actions%5Bbot%5D/received_events","type":"Bot","user_view_type":"public","site_admin":false},"node_id":"RE_kwDOQr-QxM4QQjl2","tag_name":"v0.9.4","target_commitish":"master","name":"v0.9.4","draft":false,"immutable":false,"prerelease":false,"created_at":"2025-12-25T08:18:09Z","updated_at":"2025-12-25T08:22:42Z","published_at":"2025-12-25T08:18:58Z","assets":[{"url":"https://api.github.com/repos/hepengju/release2gitee/releases/assets/332775978","id":332775978,"node_id":"RA_kwDOQr-QxM4T1cIq","name":"release2gitee-universal-apple-darwin.tar.gz","label":"","uploader":{"login":"github-actions[bot]","id":41898282,"node_id":"MDM6Qm90NDE4OTgyODI=","avatar_url":"https://avatars.githubusercontent.com/in/15368?v=4","gravatar_id":"","url":"https://api.github.com/users/github-actions%5Bbot%5D","html_url":"https://github.com/apps/github-actions","followers_url":"https://api.github.com/users/github-actions%5Bbot%5D/followers","following_url":"https://api.github.com/users/github-actions%5Bbot%5D/following{/other_user}","gists_url":"https://api.github.com/users/github-actions%5Bbot%5D/gists{/gist_id}","starred_url":"https://api.github.com/users/github-actions%5Bbot%5D/starred{/owner}{/repo}","subscriptions_url":"https://api.github.com/users/github-actions%5Bbot%5D/subscriptions","organizations_url":"https://api.github.com/users/github-actions%5Bbot%5D/orgs","repos_url":"https://api.github.com/users/github-actions%5Bbot%5D/repos","events_url":"https://api.github.com/users/github-actions%5Bbot%5D/events{/privacy}","received_events_url":"https://api.github.com/users/github-actions%5Bbot%5D/received_events","type":"Bot","user_view_type":"public","site_admin":false},"content_type":"application/x-gtar","state":"uploaded","size":5785442,"digest":"sha256:86e7a244bfb7e8ff95eb7f9cc741c5c964062f727758a6abb0c09df3c11486b2","download_count":0,"created_at":"2025-12-25T08:22:41Z","updated_at":"2025-12-25T08:22:42Z","browser_download_url":"https://github.com/hepengju/release2gitee/releases/download/v0.9.4/release2gitee-universal-apple-darwin.tar.gz"},{"url":"https://api.github.com/repos/hepengju/release2gitee/releases/assets/332775768","id":332775768,"node_id":"RA_kwDOQr-QxM4T1cFY","name":"release2gitee-x86_64-pc-windows-msvc.zip","label":"","uploader":{"login":"github-actions[bot]","id":41898282,"node_id":"MDM6Qm90NDE4OTgyODI=","avatar_url":"https://avatars.githubusercontent.com/in/15368?v=4","gravatar_id":"","url":"https://api.github.com/users/github-actions%5Bbot%5D","html_url":"https://github.com/apps/github-actions","followers_url":"https://api.github.com/users/github-actions%5Bbot%5D/followers","following_url":"https://api.github.com/users/github-actions%5Bbot%5D/following{/other_user}","gists_url":"https://api.github.com/users/github-actions%5Bbot%5D/gists{/gist_id}","starred_url":"https://api.github.com/users/github-actions%5Bbot%5D/starred{/owner}{/repo}","subscriptions_url":"https://api.github.com/users/github-actions%5Bbot%5D/subscriptions","organizations_url":"https://api.github.com/users/github-actions%5Bbot%5D/orgs","repos_url":"https://api.github.com/users/github-actions%5Bbot%5D/repos","events_url":"https://api.github.com/users/github-actions%5Bbot%5D/events{/privacy}","received_events_url":"https://api.github.com/users/github-actions%5Bbot%5D/received_events","type":"Bot","user_view_type":"public","site_admin":false},"content_type":"application/zip","state":"uploaded","size":2402653,"digest":"sha256:df68af431fe3ab73460c30c0117754a9222a921bde1412b99be6431264ef9ccf","download_count":0,"created_at":"2025-12-25T08:21:56Z","updated_at":"2025-12-25T08:21:57Z","browser_download_url":"https://github.com/hepengju/release2gitee/releases/download/v0.9.4/release2gitee-x86_64-pc-windows-msvc.zip"},{"url":"https://api.github.com/repos/hepengju/release2gitee/releases/assets/332775270","id":332775270,"node_id":"RA_kwDOQr-QxM4T1b9m","name":"release2gitee-x86_64-unknown-linux-gnu.tar.gz","label":"","uploader":{"login":"github-actions[bot]","id":41898282,"node_id":"MDM6Qm90NDE4OTgyODI=","avatar_url":"https://avatars.githubusercontent.com/in/15368?v=4","gravatar_id":"","url":"https://api.github.com/users/github-actions%5Bbot%5D","html_url":"https://github.com/apps/github-actions","followers_url":"https://api.github.com/users/github-actions%5Bbot%5D/followers","following_url":"https://api.github.com/users/github-actions%5Bbot%5D/following{/other_user}","gists_url":"https://api.github.com/users/github-actions%5Bbot%5D/gists{/gist_id}","starred_url":"https://api.github.com/users/github-actions%5Bbot%5D/starred{/owner}{/repo}","subscriptions_url":"https://api.github.com/users/github-actions%5Bbot%5D/subscriptions","organizations_url":"https://api.github.com/users/github-actions%5Bbot%5D/orgs","repos_url":"https://api.github.com/users/github-actions%5Bbot%5D/repos","events_url":"https://api.github.com/users/github-actions%5Bbot%5D/events{/privacy}","received_events_url":"https://api.github.com/users/github-actions%5Bbot%5D/received_events","type":"Bot","user_view_type":"public","site_admin":false},"content_type":"application/x-gtar","state":"uploaded","size":3070120,"digest":"sha256:94fca392f3244bcdbebbd0d17566c09f541d7e425cd7d7f7e897f7fda6551a40","download_count":0,"created_at":"2025-12-25T08:20:23Z","updated_at":"2025-12-25T08:20:24Z","browser_download_url":"https://github.com/hepengju/release2gitee/releases/download/v0.9.4/release2gitee-x86_64-unknown-linux-gnu.tar.gz"}],"tarball_url":"https://api.github.com/repos/hepengju/release2gitee/tarball/v0.9.4","zipball_url":"https://api.github.com/repos/hepengju/release2gitee/zipball/v0.9.4","body":null},{"url":"https://api.github.com/repos/hepengju/release2gitee/releases/272772969","assets_url":"https://api.github.com/repos/hepengju/release2gitee/releases/272772969/assets","upload_url":"https://uploads.github.com/repos/hepengju/release2gitee/releases/272772969/assets{?name,label}","html_url":"https://github.com/hepengju/release2gitee/releases/tag/v0.9.3","id":272772969,"author":{"login":"github-actions[bot]","id":41898282,"node_id":"MDM6Qm90NDE4OTgyODI=","avatar_url":"https://avatars.githubusercontent.com/in/15368?v=4","gravatar_id":"","url":"https://api.github.com/users/github-actions%5Bbot%5D","html_url":"https://github.com/apps/github-actions","followers_url":"https://api.github.com/users/github-actions%5Bbot%5D/followers","following_url":"https://api.github.com/users/github-actions%5Bbot%5D/following{/other_user}","gists_url":"https://api.github.com/users/github-actions%5Bbot%5D/gists{/gist_id}","starred_url":"https://api.github.com/users/github-actions%5Bbot%5D/starred{/owner}{/repo}","subscriptions_url":"https://api.github.com/users/github-actions%5Bbot%5D/subscriptions","organizations_url":"https://api.github.com/users/github-actions%5Bbot%5D/orgs","repos_url":"https://api.github.com/users/github-actions%5Bbot%5D/repos","events_url":"https://api.github.com/users/github-actions%5Bbot%5D/events{/privacy}","received_events_url":"https://api.github.com/users/github-actions%5Bbot%5D/received_events","type":"Bot","user_view_type":"public","site_admin":false},"node_id":"RE_kwDOQr-QxM4QQi9p","tag_name":"v0.9.3","target_commitish":"master","name":"v0.9.3","draft":false,"immutable":false,"prerelease":false,"created_at":"2025-12-25T07:36:20Z","updated_at":"2025-12-25T07:40:04Z","published_at":"2025-12-25T07:37:07Z","assets":[{"url":"https://api.github.com/repos/hepengju/release2gitee/releases/assets/332767841","id":332767841,"node_id":"RA_kwDOQr-QxM4T1aJh","name":"release2gitee-universal-apple-darwin.tar.gz","label":"","uploader":{"login":"github-actions[bot]","id":41898282,"node_id":"MDM6Qm90NDE4OTgyODI=","avatar_url":"https://avatars.githubusercontent.com/in/15368?v=4","gravatar_id":"","url":"https://api.github.com/users/github-actions%5Bbot%5D","html_url":"https://github.com/apps/github-actions","followers_url":"https://api.github.com/users/github-actions%5Bbot%5D/followers","following_url":"https://api.github.com/users/github-actions%5Bbot%5D/following{/other_user}","gists_url":"https://api.github.com/users/github-actions%5Bbot%5D/gists{/gist_id}","starred_url":"https://api.github.com/users/github-actions%5Bbot%5D/starred{/owner}{/repo}","subscriptions_url":"https://api.github.com/users/github-actions%5Bbot%5D/subscriptions","organizations_url":"https://api.github.com/users/github-actions%5Bbot%5D/orgs","repos_url":"https://api.github.com/users/github-actions%5Bbot%5D/repos","events_url":"https://api.github.com/users/github-actions%5Bbot%5D/events{/privacy}","received_events_url":"https://api.github.com/users/github-actions%5Bbot%5D/received_events","type":"Bot","user_view_type":"public","site_admin":false},"content_type":"application/x-gtar","state":"uploaded","size":5783698,"digest":"sha256:9e7cc0f7098fa60dfd346ed9012e5b1af2bb8572eeff6d422db9559220e8c882","download_count":1,"created_at":"2025-12-25T07:39:40Z","updated_at":"2025-12-25T07:39:41Z","browser_download_url":"https://github.com/hepengju/release2gitee/releases/download/v0.9.3/release2gitee-universal-apple-darwin.tar.gz"},{"url":"https://api.github.com/repos/hepengju/release2gitee/releases/assets/332767891","id":332767891,"node_id":"RA_kwDOQr-QxM4T1aKT","name":"release2gitee-x86_64-pc-windows-msvc.zip","label":"","uploader":{"login":"github-actions[bot]","id":41898282,"node_id":"MDM6Qm90NDE4OTgyODI=","avatar_url":"https://avatars.githubusercontent.com/in/15368?v=4","gravatar_id":"","url":"https://api.github.com/users/github-actions%5Bbot%5D","html_url":"https://github.com/apps/github-actions","followers_url":"https://api.github.com/users/github-actions%5Bbot%5D/followers","following_url":"https://api.github.com/users/github-actions%5Bbot%5D/following{/other_user}","gists_url":"https://api.github.com/users/github-actions%5Bbot%5D/gists{/gist_id}","starred_url":"https://api.github.com/users/github-actions%5Bbot%5D/starred{/owner}{/repo}","subscriptions_url":"https://api.github.com/users/github-actions%5Bbot%5D/subscriptions","organizations_url":"https://api.github.com/users/github-actions%5Bbot%5D/orgs","repos_url":"https://api.github.com/users/github-actions%5Bbot%5D/repos","events_url":"https://api.github.com/users/github-actions%5Bbot%5D/events{/privacy}","received_events_url":"https://api.github.com/users/github-actions%5Bbot%5D/received_events","type":"Bot","user_view_type":"public","site_admin":false},"content_type":"application/zip","state":"uploaded","size":2402029,"digest":"sha256:f3352e886eb1df9f6bb4d4ffd1427ba6a0bada8c3367cdff005229d07097b475","download_count":1,"created_at":"2025-12-25T07:40:03Z","updated_at":"2025-12-25T07:40:04Z","browser_download_url":"https://github.com/hepengju/release2gitee/releases/download/v0.9.3/release2gitee-x86_64-pc-windows-msvc.zip"},{"url":"https://api.github.com/repos/hepengju/release2gitee/releases/assets/332767657","id":332767657,"node_id":"RA_kwDOQr-QxM4T1aGp","name":"release2gitee-x86_64-unknown-linux-gnu.tar.gz","label":"","uploader":{"login":"github-actions[bot]","id":41898282,"node_id":"MDM6Qm90NDE4OTgyODI=","avatar_url":"https://avatars.githubusercontent.com/in/15368?v=4","gravatar_id":"","url":"https://api.github.com/users/github-actions%5Bbot%5D","html_url":"https://github.com/apps/github-actions","followers_url":"https://api.github.com/users/github-actions%5Bbot%5D/followers","following_url":"https://api.github.com/users/github-actions%5Bbot%5D/following{/other_user}","gists_url":"https://api.github.com/users/github-actions%5Bbot%5D/gists{/gist_id}","starred_url":"https://api.github.com/users/github-actions%5Bbot%5D/starred{/owner}{/repo}","subscriptions_url":"https://api.github.com/users/github-actions%5Bbot%5D/subscriptions","organizations_url":"https://api.github.com/users/github-actions%5Bbot%5D/orgs","repos_url":"https://api.github.com/users/github-actions%5Bbot%5D/repos","events_url":"https://api.github.com/users/github-actions%5Bbot%5D/events{/privacy}","received_events_url":"https://api.github.com/users/github-actions%5Bbot%5D/received_events","type":"Bot","user_view_type":"public","site_admin":false},"content_type":"application/x-gtar","state":"uploaded","size":3069000,"digest":"sha256:5e219d1ff714597fd681ce9ee68c935ed7825ac2fc0dd08ff9cd428e1f859cc1","download_count":1,"created_at":"2025-12-25T07:38:28Z","updated_at":"2025-12-25T07:38:28Z","browser_download_url":"https://github.com/hepengju/release2gitee/releases/download/v0.9.3/release2gitee-x86_64-unknown-linux-gnu.tar.gz"}],"tarball_url":"https://api.github.com/repos/hepengju/release2gitee/tarball/v0.9.3","zipball_url":"https://api.github.com/repos/hepengju/release2gitee/zipball/v0.9.3","body":null},{"url":"https://api.github.com/repos/hepengju/release2gitee/releases/272765438","assets_url":"https://api.github.com/repos/hepengju/release2gitee/releases/272765438/assets","upload_url":"https://uploads.github.com/repos/hepengju/release2gitee/releases/272765438/assets{?name,label}","html_url":"https://github.com/hepengju/release2gitee/releases/tag/v0.9.2","id":272765438,"author":{"login":"github-actions[bot]","id":41898282,"node_id":"MDM6Qm90NDE4OTgyODI=","avatar_url":"https://avatars.githubusercontent.com/in/15368?v=4","gravatar_id":"","url":"https://api.github.com/users/github-actions%5Bbot%5D","html_url":"https://github.com/apps/github-actions","followers_url":"https://api.github.com/users/github-actions%5Bbot%5D/followers","following_url":"https://api.github.com/users/github-actions%5Bbot%5D/following{/other_user}","gists_url":"https://api.github.com/users/github-actions%5Bbot%5D/gists{/gist_id}","starred_url":"https://api.github.com/users/github-actions%5Bbot%5D/starred{/owner}{/repo}","subscriptions_url":"https://api.github.com/users/github-actions%5Bbot%5D/subscriptions","organizations_url":"https://api.github.com/users/github-actions%5Bbot%5D/orgs","repos_url":"https://api.github.com/users/github-actions%5Bbot%5D/repos","events_url":"https://api.github.com/users/github-actions%5Bbot%5D/events{/privacy}","received_events_url":"https://api.github.com/users/github-actions%5Bbot%5D/received_events","type":"Bot","user_view_type":"public","site_admin":false},"node_id":"RE_kwDOQr-QxM4QQhH-","tag_name":"v0.9.2","target_commitish":"master","name":"v0.9.2","draft":false,"immutable":false,"prerelease":false,"created_at":"2025-12-25T05:38:16Z","updated_at":"2025-12-25T07:07:39Z","published_at":"2025-12-25T05:39:19Z","assets":[{"url":"https://api.github.com/repos/hepengju/release2gitee/releases/assets/332748813","id":332748813,"node_id":"RA_kwDOQr-QxM4T1VgN","name":"release2gitee-universal-apple-darwin.tar.gz","label":"","uploader":{"login":"github-actions[bot]","id":41898282,"node_id":"MDM6Qm90NDE4OTgyODI=","avatar_url":"https://avatars.githubusercontent.com/in/15368?v=4","gravatar_id":"","url":"https://api.github.com/users/github-actions%5Bbot%5D","html_url":"https://github.com/apps/github-actions","followers_url":"https://api.github.com/users/github-actions%5Bbot%5D/followers","following_url":"https://api.github.com/users/github-actions%5Bbot%5D/following{/other_user}","gists_url":"https://api.github.com/users/github-actions%5Bbot%5D/gists{/gist_id}","starred_url":"https://api.github.com/users/github-actions%5Bbot%5D/starred{/owner}{/repo}","subscriptions_url":"https://api.github.com/users/github-actions%5Bbot%5D/subscriptions","organizations_url":"https://api.github.com/users/github-actions%5Bbot%5D/orgs","repos_url":"https://api.github.com/users/github-actions%5Bbot%5D/repos","events_url":"https://api.github.com/users/github-actions%5Bbot%5D/events{/privacy}","received_events_url":"https://api.github.com/users/github-actions%5Bbot%5D/received_events","type":"Bot","user_view_type":"public","site_admin":false},"content_type":"application/x-gtar","state":"uploaded","size":5783864,"digest":"sha256:e5a00e9f5b33bcc9d676cea45c9047fae6c7a7fbe81800e3cd24e6d2f678fcac","download_count":2,"created_at":"2025-12-25T05:41:45Z","updated_at":"2025-12-25T05:41:46Z","browser_download_url":"https://github.com/hepengju/release2gitee/releases/download/v0.9.2/release2gitee-universal-apple-darwin.tar.gz"},{"url":"https://api.github.com/repos/hepengju/release2gitee/releases/assets/332748897","id":332748897,"node_id":"RA_kwDOQr-QxM4T1Vhh","name":"release2gitee-x86_64-pc-windows-msvc.zip","label":"","uploader":{"login":"github-actions[bot]","id":41898282,"node_id":"MDM6Qm90NDE4OTgyODI=","avatar_url":"https://avatars.githubusercontent.com/in/15368?v=4","gravatar_id":"","url":"https://api.github.com/users/github-actions%5Bbot%5D","html_url":"https://github.com/apps/github-actions","followers_url":"https://api.github.com/users/github-actions%5Bbot%5D/followers","following_url":"https://api.github.com/users/github-actions%5Bbot%5D/following{/other_user}","gists_url":"https://api.github.com/users/github-actions%5Bbot%5D/gists{/gist_id}","starred_url":"https://api.github.com/users/github-actions%5Bbot%5D/starred{/owner}{/repo}","subscriptions_url":"https://api.github.com/users/github-actions%5Bbot%5D/subscriptions","organizations_url":"https://api.github.com/users/github-actions%5Bbot%5D/orgs","repos_url":"https://api.github.com/users/github-actions%5Bbot%5D/repos","events_url":"https://api.github.com/users/github-actions%5Bbot%5D/events{/privacy}","received_events_url":"https://api.github.com/users/github-actions%5Bbot%5D/received_events","type":"Bot","user_view_type":"public","site_admin":false},"content_type":"application/zip","state":"uploaded","size":2402243,"digest":"sha256:c68f4f680a152906a292bc1bfc7ba787235f0f5eb71a629541266d0eabd8fae9","download_count":3,"created_at":"2025-12-25T05:42:21Z","updated_at":"2025-12-25T05:42:21Z","browser_download_url":"https://github.com/hepengju/release2gitee/releases/download/v0.9.2/release2gitee-x86_64-pc-windows-msvc.zip"},{"url":"https://api.github.com/repos/hepengju/release2gitee/releases/assets/332748639","id":332748639,"node_id":"RA_kwDOQr-QxM4T1Vdf","name":"release2gitee-x86_64-unknown-linux-gnu.tar.gz","label":"","uploader":{"login":"github-actions[bot]","id":41898282,"node_id":"MDM6Qm90NDE4OTgyODI=","avatar_url":"https://avatars.githubusercontent.com/in/15368?v=4","gravatar_id":"","url":"https://api.github.com/users/github-actions%5Bbot%5D","html_url":"https://github.com/apps/github-actions","followers_url":"https://api.github.com/users/github-actions%5Bbot%5D/followers","following_url":"https://api.github.com/users/github-actions%5Bbot%5D/following{/other_user}","gists_url":"https://api.github.com/users/github-actions%5Bbot%5D/gists{/gist_id}","starred_url":"https://api.github.com/users/github-actions%5Bbot%5D/starred{/owner}{/repo}","subscriptions_url":"https://api.github.com/users/github-actions%5Bbot%5D/subscriptions","organizations_url":"https://api.github.com/users/github-actions%5Bbot%5D/orgs","repos_url":"https://api.github.com/users/github-actions%5Bbot%5D/repos","events_url":"https://api.github.com/users/github-actions%5Bbot%5D/events{/privacy}","received_events_url":"https://api.github.com/users/github-actions%5Bbot%5D/received_events","type":"Bot","user_view_type":"public","site_admin":false},"content_type":"application/x-gtar","state":"uploaded","size":3069156,"digest":"sha256:b29a23c7526a58a73d68f9b1b7210acf5495c8072fe0d5bae9ff0a75117a699d","download_count":2,"created_at":"2025-12-25T05:40:35Z","updated_at":"2025-12-25T05:40:36Z","browser_download_url":"https://github.com/hepengju/release2gitee/releases/download/v0.9.2/release2gitee-x86_64-unknown-linux-gnu.tar.gz"}],"tarball_url":"https://api.github.com/repos/hepengju/release2gitee/tarball/v0.9.2","zipball_url":"https://api.github.com/repos/hepengju/release2gitee/zipball/v0.9.2","body":"优化清理gitee的旧release逻辑（考虑新增的同步个数）"},{"url":"https://api.github.com/repos/hepengju/release2gitee/releases/272641536","assets_url":"https://api.github.com/repos/hepengju/release2gitee/releases/272641536/assets","upload_url":"https://uploads.github.com/repos/hepengju/release2gitee/releases/272641536/assets{?name,label}","html_url":"https://github.com/hepengju/release2gitee/releases/tag/v0.9.0","id":272641536,"author":{"login":"hepengju","id":26279882,"node_id":"MDQ6VXNlcjI2Mjc5ODgy","avatar_url":"https://avatars.githubusercontent.com/u/26279882?v=4","gravatar_id":"","url":"https://api.github.com/users/hepengju","html_url":"https://github.com/hepengju","followers_url":"https://api.github.com/users/hepengju/followers","following_url":"https://api.github.com/users/hepengju/following{/other_user}","gists_url":"https://api.github.com/users/hepengju/gists{/gist_id}","starred_url":"https://api.github.com/users/hepengju/starred{/owner}{/repo}","subscriptions_url":"https://api.github.com/users/hepengju/subscriptions","organizations_url":"https://api.github.com/users/hepengju/orgs","repos_url":"https://api.github.com/users/hepengju/repos","events_url":"https://api.github.com/users/hepengju/events{/privacy}","received_events_url":"https://api.github.com/users/hepengju/received_events","type":"User","user_view_type":"public","site_admin":false},"node_id":"RE_kwDOQr-QxM4QQC4A","tag_name":"v0.9.0","target_commitish":"master","name":"v0.9.0","draft":false,"immutable":false,"prerelease":false,"created_at":"2025-12-24T08:15:54Z","updated_at":"2025-12-24T08:23:50Z","published_at":"2025-12-24T08:18:35Z","assets":[{"url":"https://api.github.com/repos/hepengju/release2gitee/releases/assets/332445226","id":332445226,"node_id":"RA_kwDOQr-QxM4T0LYq","name":"release2gitee.exe","label":null,"uploader":{"login":"hepengju","id":26279882,"node_id":"MDQ6VXNlcjI2Mjc5ODgy","avatar_url":"https://avatars.githubusercontent.com/u/26279882?v=4","gravatar_id":"","url":"https://api.github.com/users/hepengju","html_url":"https://github.com/hepengju","followers_url":"https://api.github.com/users/hepengju/followers","following_url":"https://api.github.com/users/hepengju/following{/other_user}","gists_url":"https://api.github.com/users/hepengju/gists{/gist_id}","starred_url":"https://api.github.com/users/hepengju/starred{/owner}{/repo}","subscriptions_url":"https://api.github.com/users/hepengju/subscriptions","organizations_url":"https://api.github.com/users/hepengju/orgs","repos_url":"https://api.github.com/users/hepengju/repos","events_url":"https://api.github.com/users/hepengju/events{/privacy}","received_events_url":"https://api.github.com/users/hepengju/received_events","type":"User","user_view_type":"public","site_admin":false},"content_type":"application/x-msdownload","state":"uploaded","size":5863936,"digest":"sha256:a2a485b3fd73f761ebc587494e06cdd9cbdbbd11d0065e000e52d9368b4a8c5e","download_count":3,"created_at":"2025-12-24T08:18:14Z","updated_at":"2025-12-24T08:18:33Z","browser_download_url":"https://github.com/hepengju/release2gitee/releases/download/v0.9.0/release2gitee.exe"}],"tarball_url":"https://api.github.com/repos/hepengju/release2gitee/tarball/v0.9.0","zipball_url":"https://api.github.com/repos/hepengju/release2gitee/zipball/v0.9.0","body":"- reqwest的http请求支持重试\r\n- 支持配置仅保留N个gitee的release版本\r\n- 上传下载均支持进度条显示\r\n- 命令行日志输出支持verbosity"},{"url":"https://api.github.com/repos/hepengju/release2gitee/releases/271999059","assets_url":"https://api.github.com/repos/hepengju/release2gitee/releases/271999059/assets","upload_url":"https://uploads.github.com/repos/hepengju/release2gitee/releases/271999059/assets{?name,label}","html_url":"https://github.com/hepengju/release2gitee/releases/tag/v0.1.0","id":271999059,"author":{"login":"hepengju","id":26279882,"node_id":"MDQ6VXNlcjI2Mjc5ODgy","avatar_url":"https://avatars.githubusercontent.com/u/26279882?v=4","gravatar_id":"","url":"https://api.github.com/users/hepengju","html_url":"https://github.com/hepengju","followers_url":"https://api.github.com/users/hepengju/followers","following_url":"https://api.github.com/users/hepengju/following{/other_user}","gists_url":"https://api.github.com/users/hepengju/gists{/gist_id}","starred_url":"https://api.github.com/users/hepengju/starred{/owner}{/repo}","subscriptions_url":"https://api.github.com/users/hepengju/subscriptions","organizations_url":"https://api.github.com/users/hepengju/orgs","repos_url":"https://api.github.com/users/hepengju/repos","events_url":"https://api.github.com/users/hepengju/events{/privacy}","received_events_url":"https://api.github.com/users/hepengju/received_events","type":"User","user_view_type":"public","site_admin":false},"node_id":"RE_kwDOQr-QxM4QNmBT","tag_name":"v0.1.0","target_commitish":"master","name":"v0.1.0","draft":false,"immutable":false,"prerelease":false,"created_at":"2025-12-21T06:19:37Z","updated_at":"2025-12-21T06:20:36Z","published_at":"2025-12-21T06:20:36Z","assets":[{"url":"https://api.github.com/repos/hepengju/release2gitee/releases/assets/331276764","id":331276764,"node_id":"RA_kwDOQr-QxM4TvuHc","name":"release2gitee.exe","label":null,"uploader":{"login":"hepengju","id":26279882,"node_id":"MDQ6VXNlcjI2Mjc5ODgy","avatar_url":"https://avatars.githubusercontent.com/u/26279882?v=4","gravatar_id":"","url":"https://api.github.com/users/hepengju","html_url":"https://github.com/hepengju","followers_url":"https://api.github.com/users/hepengju/followers","following_url":"https://api.github.com/users/hepengju/following{/other_user}","gists_url":"https://api.github.com/users/hepengju/gists{/gist_id}","starred_url":"https://api.github.com/users/hepengju/starred{/owner}{/repo}","subscriptions_url":"https://api.github.com/users/hepengju/subscriptions","organizations_url":"https://api.github.com/users/hepengju/orgs","repos_url":"https://api.github.com/users/hepengju/repos","events_url":"https://api.github.com/users/hepengju/events{/privacy}","received_events_url":"https://api.github.com/users/hepengju/received_events","type":"User","user_view_type":"public","site_admin":false},"content_type":"application/x-msdownload","state":"uploaded","size":7053312,"digest":"sha256:afdc03b71e9f4ddabf1ee39a3e236a1ef9c863334ee3696c1a4c4f54669f8dd5","download_count":3,"created_at":"2025-12-21T06:20:03Z","updated_at":"2025-12-21T06:20:32Z","browser_download_url":"https://github.com/hepengju/release2gitee/releases/download/v0.1.0/release2gitee.exe"}],"tarball_url":"https://api.github.com/repos/hepengju/release2gitee/tarball/v0.1.0","zipball_url":"https://api.github.com/repos/hepengju/release2gitee/zipball/v0.1.0","body":"- 完成Github的Release同步到Gitee的功能\r\n- 支持release_body 和 lastest_json里面的download_url自动替换\r\n- 支持多次重试，可复用下载的附件及进行release的body和asserts的对比功能"}]"#;
        let releases: Vec<Release> = serde_json::from_str(&result)?;
        println!("{:?}", releases);
        Ok(())
    }

    #[test]
    fn test_version() {
        assert_eq!(Version::from("1.2.3"), Version::from("v1.2.3"));
        assert_eq!(Version::from("v0.9.1") > Version::from("v0.9.0"), true);
        assert_eq!(Version::from("v0.9.11") > Version::from("v0.9.9"), true);
        //assert_eq!(Version::from("v11.9.11") > Version::from("v9.9.9"), true);

        println!("{:?}", Version::from("v0.9.1"));
        println!("{:?}", Version::from("v11.9.1"));
        println!("{:?}", Version::from("v9.9.1"));
        println!("{:?}", Version::from("11.9.1"));
        println!("{:?}", Version::from("9.9.1"));
    }
}
