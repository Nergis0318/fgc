mod benchmark;
mod cli;
mod config;
mod error;
mod git;
mod github;
mod lfs;
mod progress;
mod resume;
mod strategy;
mod tui;

use clap::Parser;
use cli::{BenchmarkArgs, Cli, CloneArgs, Commands, LfsBackend, Strategy};
use config::{expand_tilde, Config};
use error::Result;
use git::{apply_sparse_checkout, default_dest, run_clone, CloneRunOptions};
use lfs::{pull_lfs, LfsPullOptions};
use progress::CloneStatus;
use resume::{
    check_resume, should_skip_git_clone, should_skip_lfs, should_skip_sparse, ClonePhase,
    CloneState,
};
use std::sync::{Arc, Mutex};
use strategy::analyze;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    let cfg = Config::load();

    match cli.command {
        Commands::Clone(args) => clone_repo(apply_config_to_clone(args, &cfg)).await,
        Commands::Benchmark(args) => run_benchmark(apply_config_to_benchmark(args, &cfg)).await,
    }
}

fn apply_config_to_clone(mut args: CloneArgs, cfg: &Config) -> CloneArgs {
    if args.strategy == Strategy::Auto {
        if let Some(s) = cfg.default_strategy() {
            args.strategy = s;
        }
    }
    if let Some(jobs) = cfg.lfs_jobs {
        if args.lfs_jobs == 8 {
            args.lfs_jobs = jobs;
        }
    }
    if let Some(depth) = cfg.depth {
        if args.depth == 1 {
            args.depth = depth;
        }
    }
    if args.reference.is_none() {
        args.reference = cfg.expand_reference();
    } else if let Some(ref r) = args.reference {
        args.reference = Some(expand_tilde(r));
    }
    if !args.tui {
        if let Some(tui) = cfg.tui {
            args.tui = tui;
        }
    }
    if args.lfs_backend == LfsBackend::Auto {
        if let Some(backend) = cfg.lfs_backend() {
            args.lfs_backend = backend;
        }
    }
    if let Some(conns) = cfg.aria2c_connections {
        if args.aria2c_connections == 16 {
            args.aria2c_connections = conns;
        }
    }
    args
}

fn apply_config_to_benchmark(mut args: BenchmarkArgs, cfg: &Config) -> BenchmarkArgs {
    if let Some(depth) = cfg.depth {
        if args.depth == 1 {
            args.depth = depth;
        }
    }
    if args.reference.is_none() {
        args.reference = cfg.expand_reference();
    } else if let Some(ref r) = args.reference {
        args.reference = Some(expand_tilde(r));
    }
    args
}

async fn clone_repo(args: CloneArgs) -> Result<()> {
    let dest = args.dest.clone().unwrap_or_else(|| default_dest(&args.url));
    let resolved = analyze(&args).await;

    if !args.tui {
        println!("fgc: strategy={} ({})", resolved.strategy, resolved.reason);
        if let Some(ref reference) = args.reference {
            println!("fgc: using reference repository at {reference}");
        }
    }

    let mut state = match check_resume(&args, &dest)? {
        Some(existing) => existing,
        None => CloneState::from_args(&args, &dest, &resolved),
    };

    let status = if args.tui {
        Some(Arc::new(Mutex::new(CloneStatus::new(
            &resolved.strategy.to_string(),
            &args.url,
            &dest,
        ))))
    } else {
        None
    };

    let tui_handle = status
        .as_ref()
        .map(|s| tui::TuiHandle::spawn(Arc::clone(s)));

    let clone_options = CloneRunOptions {
        reference: args.reference.clone(),
        quiet: args.tui,
        enable_fallback: true,
        status: status.clone(),
    };

    let clone_result = (async {
        if !should_skip_git_clone(&state) {
            state.phase = ClonePhase::GitClone;
            state.save()?;

            if let Err(e) = run_clone(&resolved, &args.url, &dest, &clone_options) {
                state.phase = ClonePhase::Failed;
                state.save().ok();
                return Err(e);
            }

            state.phase = ClonePhase::SparseCheckout;
            state.save()?;
        } else if !args.tui {
            println!("Skipping git clone (resuming from phase {:?})", state.phase);
        }

        if resolved.strategy == Strategy::Sparse && !resolved.sparse_paths.is_empty() {
            if !should_skip_sparse(&state) {
                if let Some(ref s) = status {
                    if let Ok(mut st) = s.lock() {
                        st.set_phase_active(5);
                        st.set_message("Applying sparse checkout");
                    }
                }
                apply_sparse_checkout(&dest, &resolved.sparse_paths)?;
                state.phase = ClonePhase::LfsPull;
                state.save()?;
            }
        } else {
            state.phase = ClonePhase::LfsPull;
            state.save()?;
        }

        if !should_skip_lfs(&state) {
            if let Some(ref s) = status {
                if let Ok(mut st) = s.lock() {
                    st.set_phase_active(5);
                    st.set_message("Pulling LFS files");
                }
            }

            let lfs_options = LfsPullOptions {
                jobs: args.lfs_jobs,
                backend: args.lfs_backend,
                aria2c_connections: args.aria2c_connections,
                status: status.clone(),
            };

            let mut progress = git::CloneProgress::new();
            if !args.tui {
                progress.start_post_processing();
            }
            pull_lfs(&dest, &lfs_options, &mut progress).await?;
            if !args.tui {
                progress.finish_post_processing();
            }
        }

        state.phase = ClonePhase::Completed;
        state.save()?;
        CloneState::remove(&dest)?;

        Ok::<(), error::FgcError>(())
    })
    .await;

    if let Some(ref s) = status {
        if let Ok(mut st) = s.lock() {
            st.mark_done(clone_result.is_ok());
            st.set_message(if clone_result.is_ok() {
                "Clone completed successfully"
            } else {
                "Clone failed"
            });
        }
    }

    if let Some(handle) = tui_handle {
        handle.join();
    }

    clone_result?;

    if !args.tui {
        println!("Clone completed successfully.");
    }
    Ok(())
}

async fn run_benchmark(args: BenchmarkArgs) -> Result<()> {
    let strategies: Vec<Strategy> = args
        .strategies
        .iter()
        .filter(|s| **s != Strategy::Auto)
        .copied()
        .collect();

    if strategies.is_empty() {
        return Err(error::FgcError::new(
            "benchmark requires at least one concrete strategy (not auto)",
        ));
    }

    benchmark::run(benchmark::BenchmarkArgs {
        url: args.url,
        strategies,
        paths: args.paths,
        depth: args.depth,
        reference: args.reference,
        keep: args.keep,
        work_dir: args.work_dir,
    })
    .await?;
    Ok(())
}
