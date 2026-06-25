use crate::error::{FgcError, Result};
use serde::Deserialize;
use url::Url;

#[derive(Debug, Clone)]
pub struct RepoMetadata {
    pub size_kb: u64,
    pub has_lfs: bool,
}

#[derive(Debug, Deserialize)]
struct GitHubRepo {
    size: u64,
}

pub fn parse_github_url(url: &str) -> Option<(String, String)> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    if !host.contains("github.com") {
        return None;
    }

    let segments: Vec<&str> = parsed.path_segments()?.filter(|s| !s.is_empty()).collect();

    if segments.len() < 2 {
        return None;
    }

    let owner = segments[0].to_string();
    let name = segments[1].trim_end_matches(".git").to_string();
    Some((owner, name))
}

pub async fn fetch_metadata(url: &str) -> Result<Option<RepoMetadata>> {
    let (owner, name) = match parse_github_url(url) {
        Some(parts) => parts,
        None => return Ok(None),
    };

    let api_url = format!("https://api.github.com/repos/{owner}/{name}");
    let client = reqwest::Client::builder()
        .user_agent("fgc/0.1")
        .build()
        .map_err(|e| FgcError::new(format!("Failed to create HTTP client: {e}")))?;

    let response = client
        .get(&api_url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| FgcError::new(format!("GitHub API request failed: {e}")))?;

    if !response.status().is_success() {
        return Ok(None);
    }

    let repo: GitHubRepo = response
        .json()
        .await
        .map_err(|e| FgcError::new(format!("Failed to parse GitHub API response: {e}")))?;

    let has_lfs = check_lfs(&client, &owner, &name).await.unwrap_or(false);

    Ok(Some(RepoMetadata {
        size_kb: repo.size,
        has_lfs,
    }))
}

async fn check_lfs(client: &reqwest::Client, owner: &str, name: &str) -> Result<bool> {
    let url = format!("https://api.github.com/repos/{owner}/{name}/contents/.gitattributes");
    let response = client
        .get(&url)
        .header("Accept", "application/vnd.github.raw")
        .send()
        .await
        .map_err(|e| FgcError::new(format!("LFS check failed: {e}")))?;

    if !response.status().is_success() {
        return Ok(false);
    }

    let content = response
        .text()
        .await
        .map_err(|e| FgcError::new(format!("Failed to read .gitattributes: {e}")))?;

    Ok(content.contains("filter=lfs"))
}
