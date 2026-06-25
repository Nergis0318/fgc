use crate::cli::{CloneArgs, Strategy};
use crate::github::{self, RepoMetadata};

const LARGE_REPO_THRESHOLD_KB: u64 = 2 * 1024 * 1024; // 2 GB

#[derive(Debug, Clone)]
pub struct ResolvedStrategy {
    pub strategy: Strategy,
    pub reason: String,
    pub sparse_paths: Vec<String>,
    pub depth: u32,
}

pub async fn analyze(args: &CloneArgs) -> ResolvedStrategy {
    if args.strategy != Strategy::Auto {
        return ResolvedStrategy {
            strategy: args.strategy,
            reason: format!("User specified --strategy {}", args.strategy),
            sparse_paths: args.paths.clone(),
            depth: args.depth,
        };
    }

    let metadata = github::fetch_metadata(&args.url).await.ok().flatten();

    resolve_auto(args, metadata.as_ref())
}

fn resolve_auto(args: &CloneArgs, metadata: Option<&RepoMetadata>) -> ResolvedStrategy {
    if std::env::var("CI").ok().as_deref() == Some("true") {
        return ResolvedStrategy {
            strategy: Strategy::Shallow,
            reason: "CI environment detected (CI=true)".to_string(),
            sparse_paths: args.paths.clone(),
            depth: args.depth,
        };
    }

    let Some(meta) = metadata else {
        return ResolvedStrategy {
            strategy: Strategy::Blobless,
            reason: "No GitHub metadata available; defaulting to blobless".to_string(),
            sparse_paths: args.paths.clone(),
            depth: args.depth,
        };
    };

    let size_gb = meta.size_kb as f64 / (1024.0 * 1024.0);

    if meta.size_kb > LARGE_REPO_THRESHOLD_KB || meta.has_lfs {
        let use_sparse = !args.paths.is_empty();
        let strategy = if use_sparse {
            Strategy::Sparse
        } else {
            Strategy::Blobless
        };

        let reason = if meta.has_lfs && meta.size_kb > LARGE_REPO_THRESHOLD_KB {
            format!("Large repo ({size_gb:.1} GB) with LFS detected on GitHub",)
        } else if meta.has_lfs {
            "LFS usage detected on GitHub".to_string()
        } else {
            format!("Large repo detected ({size_gb:.1} GB)")
        };

        return ResolvedStrategy {
            strategy,
            reason,
            sparse_paths: args.paths.clone(),
            depth: args.depth,
        };
    }

    ResolvedStrategy {
        strategy: Strategy::Blobless,
        reason: format!("Moderate repo size ({size_gb:.1} GB); blobless recommended",),
        sparse_paths: args.paths.clone(),
        depth: args.depth,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ci_env_selects_shallow() {
        std::env::set_var("CI", "true");
        let args = CloneArgs {
            url: "https://github.com/example/repo.git".to_string(),
            dest: None,
            strategy: Strategy::Auto,
            paths: vec![],
            depth: 1,
            lfs_jobs: 8,
            lfs_backend: crate::cli::LfsBackend::Auto,
            aria2c_connections: 16,
            reference: None,
            tui: false,
            no_resume: false,
            yes: false,
        };

        let resolved = resolve_auto(&args, None);
        assert_eq!(resolved.strategy, Strategy::Shallow);
        std::env::remove_var("CI");
    }

    #[test]
    fn large_repo_selects_blobless() {
        let args = CloneArgs {
            url: "https://github.com/torvalds/linux.git".to_string(),
            dest: None,
            strategy: Strategy::Auto,
            paths: vec![],
            depth: 1,
            lfs_jobs: 8,
            lfs_backend: crate::cli::LfsBackend::Auto,
            aria2c_connections: 16,
            reference: None,
            tui: false,
            no_resume: false,
            yes: false,
        };

        let meta = RepoMetadata {
            size_kb: 3 * 1024 * 1024,
            has_lfs: false,
        };

        let resolved = resolve_auto(&args, Some(&meta));
        assert_eq!(resolved.strategy, Strategy::Blobless);
    }
}
