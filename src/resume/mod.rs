use crate::cli::CloneArgs;
use crate::error::{FgcError, Result};
use crate::strategy::ResolvedStrategy;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloneState {
    pub url: String,
    pub dest: String,
    pub strategy: String,
    pub phase: ClonePhase,
    pub sparse_paths: Vec<String>,
    pub depth: u32,
    pub lfs_jobs: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClonePhase {
    NotStarted,
    GitClone,
    SparseCheckout,
    LfsPull,
    Completed,
    Failed,
}

impl CloneState {
    fn git_state_path(dest: &str) -> PathBuf {
        Path::new(dest).join(".git").join("fgc-state.json")
    }

    fn sidecar_state_path(dest: &str) -> PathBuf {
        PathBuf::from(format!("{dest}.fgc-state.json"))
    }

    pub fn from_args(args: &CloneArgs, dest: &str, resolved: &ResolvedStrategy) -> Self {
        Self {
            url: args.url.clone(),
            dest: dest.to_string(),
            strategy: resolved.strategy.to_string(),
            phase: ClonePhase::NotStarted,
            sparse_paths: resolved.sparse_paths.clone(),
            depth: resolved.depth,
            lfs_jobs: args.lfs_jobs,
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = if self.phase == ClonePhase::GitClone || self.phase == ClonePhase::NotStarted {
            Self::sidecar_state_path(&self.dest)
        } else {
            Self::git_state_path(&self.dest)
        };

        if path == Self::git_state_path(&self.dest) {
            if let Some(parent) = path.parent() {
                if !parent.exists() {
                    return Err(FgcError::new(
                        "Cannot save in-repo state before git clone completes",
                    ));
                }
            }
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| FgcError::new(format!("Failed to serialize state: {e}")))?;
        std::fs::write(&path, json)
            .map_err(|e| FgcError::new(format!("Failed to write state file: {e}")))?;

        if path == Self::sidecar_state_path(&self.dest) {
            let git_path = Self::git_state_path(&self.dest);
            if git_path.parent().is_some_and(|p| p.exists()) {
                std::fs::rename(&path, &git_path).map_err(|e| {
                    FgcError::new(format!("Failed to migrate state into .git: {e}"))
                })?;
            }
        }

        Ok(())
    }

    pub fn load(dest: &str) -> Result<Option<Self>> {
        for path in [Self::git_state_path(dest), Self::sidecar_state_path(dest)] {
            if !path.exists() {
                continue;
            }
            let content = std::fs::read_to_string(&path)
                .map_err(|e| FgcError::new(format!("Failed to read state file: {e}")))?;
            let state: Self = serde_json::from_str(&content)
                .map_err(|e| FgcError::new(format!("Failed to parse state file: {e}")))?;
            return Ok(Some(state));
        }
        Ok(None)
    }

    pub fn remove(dest: &str) -> Result<()> {
        for path in [Self::git_state_path(dest), Self::sidecar_state_path(dest)] {
            if path.exists() {
                std::fs::remove_file(&path)
                    .map_err(|e| FgcError::new(format!("Failed to remove state file: {e}")))?;
            }
        }
        Ok(())
    }
}

pub fn check_resume(args: &CloneArgs, dest: &str) -> Result<Option<CloneState>> {
    if args.no_resume {
        CloneState::remove(dest)?;
        return Ok(None);
    }

    let existing = CloneState::load(dest)?;
    let Some(state) = existing else {
        return Ok(None);
    };

    if state.url != args.url {
        return Ok(None);
    }

    if state.phase == ClonePhase::Completed {
        CloneState::remove(dest)?;
        return Ok(None);
    }

    if args.yes {
        println!("Resuming previous clone for {}", dest);
        return Ok(Some(state));
    }

    print!(
        "Found incomplete clone at {} (phase: {:?}). Resume? [y/N] ",
        dest, state.phase
    );
    io::stdout().flush().ok();

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| FgcError::new(format!("Failed to read input: {e}")))?;

    if input.trim().eq_ignore_ascii_case("y") || input.trim().eq_ignore_ascii_case("yes") {
        Ok(Some(state))
    } else {
        println!("Starting fresh clone (removing existing directory)...");
        if Path::new(dest).exists() {
            std::fs::remove_dir_all(dest)
                .map_err(|e| FgcError::new(format!("Failed to remove {dest}: {e}")))?;
        }
        CloneState::remove(dest)?;
        Ok(None)
    }
}

pub fn should_skip_git_clone(state: &CloneState) -> bool {
    matches!(
        state.phase,
        ClonePhase::SparseCheckout | ClonePhase::LfsPull | ClonePhase::Completed
    )
}

pub fn should_skip_sparse(state: &CloneState) -> bool {
    matches!(state.phase, ClonePhase::LfsPull | ClonePhase::Completed)
}

pub fn should_skip_lfs(state: &CloneState) -> bool {
    state.phase == ClonePhase::Completed
}
