use crate::error::{FgcError, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct LfsObject {
    pub oid: String,
    pub size: u64,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct DownloadAction {
    pub oid: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
}

#[derive(Serialize)]
struct BatchRequest {
    operation: &'static str,
    transfers: Vec<&'static str>,
    objects: Vec<BatchObject>,
}

#[derive(Serialize)]
struct BatchObject {
    oid: String,
    size: u64,
}

#[derive(Deserialize)]
struct BatchResponse {
    objects: Vec<BatchResponseObject>,
}

#[derive(Deserialize)]
struct BatchResponseObject {
    oid: String,
    actions: Option<BatchActions>,
    error: Option<BatchError>,
}

#[derive(Deserialize)]
struct BatchActions {
    download: Option<BatchAction>,
}

#[derive(Deserialize)]
struct BatchAction {
    href: String,
    header: Option<std::collections::HashMap<String, String>>,
}

#[derive(Deserialize)]
struct BatchError {
    message: String,
}

pub fn list_missing_objects(dest: &str) -> Result<Vec<LfsObject>> {
    let output = Command::new("git")
        .args(["lfs", "ls-files", "-l"])
        .current_dir(dest)
        .output()
        .map_err(|e| FgcError::new(format!("git lfs ls-files failed: {e}")))?;

    if !output.status.success() {
        return Ok(vec![]);
    }

    let mut missing = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }
        let oid = parts[0].to_string();
        let size: u64 = parts[1].parse().unwrap_or(0);
        let path = parts[3..].join(" ");

        if !object_exists(dest, &oid) {
            missing.push(LfsObject { oid, size, path });
        }
    }
    Ok(missing)
}

fn object_exists(dest: &str, oid: &str) -> bool {
    if oid.len() < 4 {
        return false;
    }
    let path = Path::new(dest)
        .join(".git/lfs/objects")
        .join(&oid[0..2])
        .join(&oid[2..4])
        .join(oid);
    path.exists()
}

pub async fn fetch_download_actions(
    dest: &str,
    objects: &[LfsObject],
) -> Result<Vec<DownloadAction>> {
    if objects.is_empty() {
        return Ok(vec![]);
    }

    let endpoint = lfs_batch_endpoint(dest)?;
    let auth_headers = git_auth_headers(dest)?;

    let request = BatchRequest {
        operation: "download",
        transfers: vec!["basic"],
        objects: objects
            .iter()
            .map(|o| BatchObject {
                oid: o.oid.clone(),
                size: o.size,
            })
            .collect(),
    };

    let client = reqwest::Client::builder()
        .user_agent("fgc/0.1")
        .build()
        .map_err(|e| FgcError::new(format!("HTTP client error: {e}")))?;

    let mut req = client
        .post(&endpoint)
        .header("Accept", "application/vnd.git-lfs+json")
        .header("Content-Type", "application/vnd.git-lfs+json")
        .json(&request);

    for (k, v) in &auth_headers {
        req = req.header(k, v);
    }

    let response = req
        .send()
        .await
        .map_err(|e| FgcError::new(format!("LFS batch request failed: {e}")))?;

    if !response.status().is_success() {
        return Err(FgcError::new(format!(
            "LFS batch API returned {}",
            response.status()
        )));
    }

    let body: BatchResponse = response
        .json()
        .await
        .map_err(|e| FgcError::new(format!("Failed to parse LFS batch response: {e}")))?;

    let mut actions = Vec::new();
    for obj in body.objects {
        if let Some(err) = obj.error {
            return Err(FgcError::new(format!(
                "LFS batch error for {}: {}",
                obj.oid, err.message
            )));
        }
        if let Some(download) = obj.actions.and_then(|a| a.download) {
            let headers = download
                .header
                .map(|h| h.into_iter().collect())
                .unwrap_or_default();
            actions.push(DownloadAction {
                oid: obj.oid,
                url: download.href,
                headers,
            });
        }
    }

    Ok(actions)
}

fn lfs_batch_endpoint(dest: &str) -> Result<String> {
    if let Ok(url) = git_config(dest, "lfs.url") {
        if !url.is_empty() {
            return Ok(format!("{}/objects/batch", url.trim_end_matches('/')));
        }
    }

    let remote = git_config(dest, "remote.origin.url")?;
    let base = normalize_remote_url(&remote);
    Ok(format!("{base}/info/lfs/objects/batch"))
}

fn git_config(dest: &str, key: &str) -> Result<String> {
    let output = Command::new("git")
        .args(["config", "--get", key])
        .current_dir(dest)
        .output()
        .map_err(|e| FgcError::new(format!("git config failed: {e}")))?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn normalize_remote_url(url: &str) -> String {
    let url = url.trim_end_matches('/').trim_end_matches(".git");
    if url.starts_with("git@github.com:") {
        let path = url.trim_start_matches("git@github.com:");
        return format!("https://github.com/{path}");
    }
    url.to_string()
}

fn git_auth_headers(dest: &str) -> Result<Vec<(String, String)>> {
    let remote = git_config(dest, "remote.origin.url")?;
    let (host, path) = parse_remote_host_path(&remote)?;

    let mut child = Command::new("git")
        .args([
            "credential",
            "fill",
            &format!("protocol=https"),
            &format!("host={host}"),
            &format!("path={path}"),
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| FgcError::new(format!("git credential fill failed: {e}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = stdin.write_all(b"\n");
    }

    let output = child
        .wait_with_output()
        .map_err(|e| FgcError::new(format!("git credential wait failed: {e}")))?;

    let mut headers = Vec::new();
    let mut username = String::new();
    let mut password = String::new();

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(val) = line.strip_prefix("username=") {
            username = val.to_string();
        } else if let Some(val) = line.strip_prefix("password=") {
            password = val.to_string();
        }
    }

    if !username.is_empty() && !password.is_empty() {
        let token = base64_encode(&format!("{username}:{password}"));
        headers.push(("Authorization".to_string(), format!("Basic {token}")));
    }

    Ok(headers)
}

fn parse_remote_host_path(url: &str) -> Result<(String, String)> {
    if url.starts_with("git@") {
        let rest = url.split_once(':').map(|(_, r)| r).unwrap_or(url);
        let path = rest.trim_end_matches(".git");
        return Ok(("github.com".to_string(), path.to_string()));
    }
    let parsed =
        url::Url::parse(url).map_err(|e| FgcError::new(format!("Invalid remote URL: {e}")))?;
    let host = parsed.host_str().unwrap_or("").to_string();
    let path = parsed
        .path()
        .trim_start_matches('/')
        .trim_end_matches(".git")
        .to_string();
    Ok((host, path))
}

fn base64_encode(input: &str) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine};
    STANDARD.encode(input)
}
