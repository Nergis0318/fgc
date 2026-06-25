mod aria2c;
mod batch;

use crate::cli::LfsBackend;
use crate::error::{FgcError, Result};
use crate::git::CloneProgress;
use crate::progress::{apply_lfs_progress, CloneStatus};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct LfsPullOptions {
    pub jobs: u32,
    pub backend: LfsBackend,
    pub aria2c_connections: u32,
    pub status: Option<Arc<Mutex<CloneStatus>>>,
}

impl Default for LfsPullOptions {
    fn default() -> Self {
        Self {
            jobs: 8,
            backend: LfsBackend::Auto,
            aria2c_connections: 16,
            status: None,
        }
    }
}

pub fn is_lfs_available() -> bool {
    Command::new("git")
        .args(["lfs", "version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn repo_uses_lfs(dest: &str) -> bool {
    let gitattributes = Path::new(dest).join(".gitattributes");
    if !gitattributes.exists() {
        return false;
    }
    std::fs::read_to_string(&gitattributes)
        .map(|c| c.contains("filter=lfs"))
        .unwrap_or(false)
}

pub async fn pull_lfs(
    dest: &str,
    options: &LfsPullOptions,
    progress: &mut CloneProgress,
) -> Result<()> {
    if !is_lfs_available() {
        eprintln!(
            "Warning: git-lfs is not installed. Skipping LFS file download.\n\
             Install git-lfs: https://git-lfs.com"
        );
        return Ok(());
    }

    if !repo_uses_lfs(dest) {
        return Ok(());
    }

    update_lfs_status(options, 0, 100, "starting");

    let use_aria2c = match options.backend {
        LfsBackend::Aria2c => true,
        LfsBackend::Git => false,
        LfsBackend::Auto => aria2c::is_available(),
    };

    if use_aria2c {
        match pull_lfs_aria2c(dest, options).await {
            Ok(()) => {
                update_lfs_status(options, 100, 100, "done (aria2c)");
                progress.set_lfs_progress(100, 100, "done");
                progress.finish_lfs();
                return Ok(());
            }
            Err(e) => {
                eprintln!("fgc: aria2c LFS failed ({e}); falling back to git lfs pull");
            }
        }
    }

    pull_lfs_git(dest, options, progress)
}

async fn pull_lfs_aria2c(dest: &str, options: &LfsPullOptions) -> Result<()> {
    let objects = batch::list_missing_objects(dest)?;
    if objects.is_empty() {
        return Ok(());
    }

    update_lfs_status(
        options,
        0,
        objects.len() as u64,
        &format!("aria2c: {} files", objects.len()),
    );

    let actions = batch::fetch_download_actions(dest, &objects).await?;
    aria2c::download_objects(
        dest,
        &objects,
        &actions,
        options.jobs,
        options.aria2c_connections,
    )?;
    aria2c::checkout(dest)?;

    update_lfs_status(options, objects.len() as u64, objects.len() as u64, "done");
    Ok(())
}

fn pull_lfs_git(dest: &str, options: &LfsPullOptions, progress: &mut CloneProgress) -> Result<()> {
    progress.set_lfs_progress(0, 100, "git lfs pull");

    let output = Command::new("git")
        .args([
            "lfs",
            "pull",
            "--include=*",
            &format!("--jobs={}", options.jobs),
        ])
        .current_dir(dest)
        .output()
        .map_err(|e| FgcError::new(format!("git lfs pull failed: {e}")))?;

    for line in String::from_utf8_lossy(&output.stderr).lines() {
        if line.contains('%') {
            if let Some(pct) = extract_percent(line) {
                progress.set_lfs_progress(pct, 100, line);
                update_lfs_status(options, pct, 100, line);
            }
        }
    }

    if !output.status.success() {
        return Err(FgcError::new(format!(
            "git lfs pull failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    progress.set_lfs_progress(100, 100, "done");
    progress.finish_lfs();
    Ok(())
}

fn update_lfs_status(options: &LfsPullOptions, current: u64, total: u64, msg: &str) {
    if let Some(status) = &options.status {
        if let Ok(mut s) = status.lock() {
            apply_lfs_progress(&mut s, current, total, msg);
        }
    }
}

fn extract_percent(line: &str) -> Option<u64> {
    line.split_whitespace()
        .find(|s| s.ends_with('%'))
        .and_then(|s| s.trim_end_matches('%').parse().ok())
}
