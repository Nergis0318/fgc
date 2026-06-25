use crate::error::{FgcError, Result};
use crate::lfs::batch::{DownloadAction, LfsObject};
use std::path::Path;
use std::process::Command;

pub fn is_available() -> bool {
    Command::new("aria2c")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn download_objects(
    dest: &str,
    objects: &[LfsObject],
    actions: &[DownloadAction],
    jobs: u32,
    connections: u32,
) -> Result<()> {
    if actions.is_empty() {
        return Ok(());
    }

    let input_path = std::env::temp_dir().join(format!("fgc-lfs-{}.txt", std::process::id()));
    let mut input = String::new();

    for action in actions {
        let object = objects.iter().find(|o| o.oid == action.oid);
        let out_path = lfs_object_path(dest, &action.oid);

        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| FgcError::new(format!("Failed to create LFS object dir: {e}")))?;
        }

        input.push_str(&action.url);
        input.push('\n');
        input.push_str(&format!("  dir={}\n", out_path.parent().unwrap().display()));
        input.push_str(&format!(
            "  out={}\n",
            out_path.file_name().unwrap().to_string_lossy()
        ));
        for (k, v) in &action.headers {
            input.push_str(&format!("  header={k}: {v}\n"));
        }
        input.push('\n');

        let _ = object;
    }

    std::fs::write(&input_path, &input)
        .map_err(|e| FgcError::new(format!("Failed to write aria2c input: {e}")))?;

    let output = Command::new("aria2c")
        .args([
            &format!("--input-file={}", input_path.display()),
            &format!("--max-concurrent-downloads={jobs}"),
            &format!("--split={connections}"),
            &format!("--max-connection-per-server={connections}"),
            "--min-split-size=1M",
            "--allow-overwrite=true",
            "--auto-file-renaming=false",
            "--console-log-level=warn",
        ])
        .output()
        .map_err(|e| FgcError::new(format!("aria2c failed: {e}")))?;

    std::fs::remove_file(&input_path).ok();

    if !output.status.success() {
        return Err(FgcError::new(format!(
            "aria2c download failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    Ok(())
}

pub fn checkout(dest: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["lfs", "checkout"])
        .current_dir(dest)
        .output()
        .map_err(|e| FgcError::new(format!("git lfs checkout failed: {e}")))?;

    if !output.status.success() {
        return Err(FgcError::new(format!(
            "git lfs checkout failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}

fn lfs_object_path(dest: &str, oid: &str) -> std::path::PathBuf {
    Path::new(dest)
        .join(".git/lfs/objects")
        .join(&oid[0..2])
        .join(&oid[2..4])
        .join(oid)
}
