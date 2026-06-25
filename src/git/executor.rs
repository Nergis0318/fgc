use crate::cli::Strategy;
use crate::error::{FgcError, Result};
use crate::git::progress::CloneProgress;
use crate::progress::{apply_git_line, CloneStatus};
use crate::strategy::ResolvedStrategy;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct CloneRunOptions {
    pub reference: Option<String>,
    pub quiet: bool,
    pub enable_fallback: bool,
    pub status: Option<Arc<Mutex<CloneStatus>>>,
}

pub fn default_dest(url: &str) -> String {
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("repo")
        .trim_end_matches(".git")
        .to_string()
}

pub fn build_clone_args(
    resolved: &ResolvedStrategy,
    url: &str,
    dest: &str,
    options: &CloneRunOptions,
) -> Vec<String> {
    let mut args = vec!["clone".to_string(), "--progress".to_string()];

    if let Some(reference) = &options.reference {
        args.push(format!("--reference={reference}"));
        args.push("--dissociate".to_string());
    }

    match resolved.strategy {
        Strategy::Blobless | Strategy::Sparse => {
            args.push("--filter=blob:none".to_string());
            if resolved.strategy == Strategy::Sparse {
                args.push("--sparse".to_string());
            }
        }
        Strategy::Shallow => {
            args.push(format!("--depth={}", resolved.depth));
            args.push("--single-branch".to_string());
        }
        Strategy::Full | Strategy::Auto => {}
    }

    args.push(url.to_string());
    args.push(dest.to_string());
    args
}

pub fn run_clone(
    resolved: &ResolvedStrategy,
    url: &str,
    dest: &str,
    options: &CloneRunOptions,
) -> Result<()> {
    run_clone_timed(resolved, url, dest, options).map(|_| ())
}

pub fn run_clone_timed(
    resolved: &ResolvedStrategy,
    url: &str,
    dest: &str,
    options: &CloneRunOptions,
) -> Result<Duration> {
    let start = Instant::now();
    match execute_clone(resolved, url, dest, options) {
        Ok(()) => Ok(start.elapsed()),
        Err(e) if options.enable_fallback => try_fallback(resolved, url, dest, options, e),
        Err(e) => Err(e),
    }
}

fn try_fallback(
    resolved: &ResolvedStrategy,
    url: &str,
    dest: &str,
    options: &CloneRunOptions,
    original_err: FgcError,
) -> Result<Duration> {
    if !is_partial_clone_error(&original_err) {
        return Err(original_err);
    }

    cleanup_dest(dest);

    let fallbacks = match resolved.strategy {
        Strategy::Blobless | Strategy::Sparse => vec![Strategy::Shallow, Strategy::Full],
        Strategy::Shallow => vec![Strategy::Full],
        _ => vec![],
    };

    for strategy in fallbacks {
        eprintln!(
            "fgc: server does not support {:?} clone; falling back to {strategy}...",
            resolved.strategy
        );

        let fallback = ResolvedStrategy {
            strategy,
            reason: format!("fallback from {}", resolved.strategy),
            sparse_paths: resolved.sparse_paths.clone(),
            depth: resolved.depth,
        };

        let no_fallback = CloneRunOptions {
            enable_fallback: false,
            ..options.clone()
        };

        match run_clone_timed(&fallback, url, dest, &no_fallback) {
            Ok(duration) => return Ok(duration),
            Err(e) if is_partial_clone_error(&e) || strategy == Strategy::Shallow => continue,
            Err(e) => return Err(e),
        }
    }

    Err(original_err)
}

fn is_partial_clone_error(err: &FgcError) -> bool {
    let msg = err.to_string().to_lowercase();
    msg.contains("filter")
        || msg.contains("partial clone")
        || msg.contains("unknown option")
        || msg.contains("does not support")
}

fn cleanup_dest(dest: &str) {
    let path = Path::new(dest);
    if path.exists() {
        std::fs::remove_dir_all(path).ok();
    }
    let sidecar = format!("{dest}.fgc-state.json");
    if Path::new(&sidecar).exists() {
        std::fs::remove_file(&sidecar).ok();
    }
}

fn execute_clone(
    resolved: &ResolvedStrategy,
    url: &str,
    dest: &str,
    options: &CloneRunOptions,
) -> Result<()> {
    let git_args = build_clone_args(resolved, url, dest, options);

    if options.status.is_some() || options.quiet {
        return execute_clone_capture(resolved, &git_args, options);
    }

    let progress = Arc::new(Mutex::new(CloneProgress::new()));

    let mut child = Command::new("git")
        .args(&git_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| FgcError::new(format!("Failed to spawn git clone: {e}")))?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| FgcError::new("Failed to capture git stderr"))?;

    let progress_clone = Arc::clone(&progress);
    let stderr_handle = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        let mut lines = Vec::new();
        for line in reader.lines().map_while(|l| l.ok()) {
            if let Ok(mut p) = progress_clone.lock() {
                p.process_line(&line);
            }
            lines.push(line);
        }
        lines
    });

    let status = child
        .wait()
        .map_err(|e| FgcError::new(format!("git clone wait failed: {e}")))?;

    let stderr_lines = stderr_handle.join().unwrap_or_default();

    {
        let mut p = progress
            .lock()
            .map_err(|_| FgcError::new("progress lock poisoned"))?;
        p.advance_to_checkout();
        p.finish_checkout();
    }

    if !status.success() {
        let stderr_text = stderr_lines.join("\n");
        return Err(clone_error_from_stderr(resolved, &stderr_text));
    }

    {
        let p = progress
            .lock()
            .map_err(|_| FgcError::new("progress lock poisoned"))?;
        p.print_summary(dest, &resolved.strategy.to_string());
    }

    Ok(())
}

fn execute_clone_capture(
    resolved: &ResolvedStrategy,
    git_args: &[String],
    options: &CloneRunOptions,
) -> Result<()> {
    let mut child = Command::new("git")
        .args(git_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| FgcError::new(format!("Failed to spawn git clone: {e}")))?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| FgcError::new("Failed to capture git stderr"))?;

    let status_clone = options.status.clone();
    let stderr_handle = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        let mut lines = Vec::new();
        for line in reader.lines().map_while(|l| l.ok()) {
            if let Some(ref shared) = status_clone {
                if let Ok(mut s) = shared.lock() {
                    apply_git_line(&mut s, &line);
                }
            }
            lines.push(line);
        }
        lines
    });

    let wait_status = child
        .wait()
        .map_err(|e| FgcError::new(format!("git clone wait failed: {e}")))?;

    let stderr_lines = stderr_handle.join().unwrap_or_default();

    if let Some(ref shared) = options.status {
        if let Ok(mut s) = shared.lock() {
            s.set_phase_active(4);
            s.finish_phase(4);
            s.set_message("Git clone complete");
        }
    }

    if !wait_status.success() {
        let stderr_text = stderr_lines.join("\n");
        return Err(clone_error_from_stderr(resolved, &stderr_text));
    }

    Ok(())
}

fn clone_error(resolved: &ResolvedStrategy, stderr: &[u8]) -> FgcError {
    clone_error_from_stderr(resolved, &String::from_utf8_lossy(stderr))
}

fn clone_error_from_stderr(resolved: &ResolvedStrategy, stderr_text: &str) -> FgcError {
    if resolved.strategy == Strategy::Blobless || resolved.strategy == Strategy::Sparse {
        if stderr_text.to_lowercase().contains("filter") || stderr_text.contains("unknown option") {
            return FgcError::new(format!(
                "Server does not support partial clone ({strategy}). \
                 Use --strategy shallow or --strategy full.",
                strategy = resolved.strategy
            ));
        }
    }
    FgcError::new(format!("git clone failed:\n{stderr_text}"))
}

pub fn apply_sparse_checkout(dest: &str, paths: &[String]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }

    let dest_path = Path::new(dest);
    if !dest_path.exists() {
        return Err(FgcError::new(format!("Destination not found: {dest}")));
    }

    let init = Command::new("git")
        .args(["sparse-checkout", "init", "--cone"])
        .current_dir(dest_path)
        .output()
        .map_err(|e| FgcError::new(format!("sparse-checkout init failed: {e}")))?;

    if !init.status.success() {
        return Err(FgcError::new(format!(
            "sparse-checkout init failed: {}",
            String::from_utf8_lossy(&init.stderr)
        )));
    }

    let mut set_args = vec!["sparse-checkout".to_string(), "set".to_string()];
    set_args.extend(paths.iter().cloned());

    let set = Command::new("git")
        .args(&set_args)
        .current_dir(dest_path)
        .output()
        .map_err(|e| FgcError::new(format!("sparse-checkout set failed: {e}")))?;

    if !set.status.success() {
        return Err(FgcError::new(format!(
            "sparse-checkout set failed: {}",
            String::from_utf8_lossy(&set.stderr)
        )));
    }

    Ok(())
}

pub fn dir_size(path: &str) -> Result<u64> {
    let path = PathBuf::from(path);
    if !path.exists() {
        return Ok(0);
    }
    dir_size_recursive(&path)
}

fn dir_size_recursive(path: &Path) -> Result<u64> {
    let mut total = 0;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)
            .map_err(|e| FgcError::new(format!("Failed to read dir {}: {e}", path.display())))?
        {
            let entry =
                entry.map_err(|e| FgcError::new(format!("Failed to read dir entry: {e}")))?;
            total += dir_size_recursive(&entry.path())?;
        }
    } else {
        total += std::fs::metadata(path)
            .map_err(|e| FgcError::new(format!("Failed to stat {}: {e}", path.display())))?
            .len();
    }
    Ok(total)
}
