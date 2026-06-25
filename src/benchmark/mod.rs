use crate::cli::Strategy;
use crate::error::{FgcError, Result};
use crate::git::{default_dest, dir_size, run_clone_timed, CloneRunOptions};
use crate::strategy::ResolvedStrategy;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct BenchmarkArgs {
    pub url: String,
    pub strategies: Vec<Strategy>,
    pub paths: Vec<String>,
    pub depth: u32,
    pub reference: Option<String>,
    pub keep: bool,
    pub work_dir: Option<String>,
}

#[derive(Debug)]
pub struct BenchmarkResult {
    pub strategy: Strategy,
    pub duration: Duration,
    pub size_bytes: u64,
    pub success: bool,
    pub error: Option<String>,
}

pub async fn run(args: BenchmarkArgs) -> Result<Vec<BenchmarkResult>> {
    let base_name = default_dest(&args.url);
    let work_root = args
        .work_dir
        .clone()
        .unwrap_or_else(|| std::env::temp_dir().to_string_lossy().to_string());

    let mut results = Vec::new();

    println!("fgc benchmark: {}", args.url);
    println!(
        "Testing {} strategies in {}\n",
        args.strategies.len(),
        work_root
    );

    for strategy in &args.strategies {
        let dest = format!("{work_root}/fgc-bench-{base_name}-{strategy}");
        if std::path::Path::new(&dest).exists() {
            std::fs::remove_dir_all(&dest)
                .map_err(|e| FgcError::new(format!("Failed to clean benchmark dir {dest}: {e}")))?;
        }

        let resolved = ResolvedStrategy {
            strategy: *strategy,
            reason: "benchmark".to_string(),
            sparse_paths: args.paths.clone(),
            depth: args.depth,
        };

        let options = CloneRunOptions {
            reference: args.reference.clone(),
            quiet: true,
            enable_fallback: false,
            status: None,
        };

        print!("  [{strategy}] cloning... ");
        std::io::Write::flush(&mut std::io::stdout()).ok();

        match run_clone_timed(&resolved, &args.url, &dest, &options) {
            Ok(duration) => {
                let size = dir_size(&dest).unwrap_or(0);
                println!("{:.1}s  ({})", duration.as_secs_f64(), format_size(size));
                results.push(BenchmarkResult {
                    strategy: *strategy,
                    duration,
                    size_bytes: size,
                    success: true,
                    error: None,
                });

                if !args.keep {
                    std::fs::remove_dir_all(&dest).ok();
                }
            }
            Err(e) => {
                println!("FAILED");
                results.push(BenchmarkResult {
                    strategy: *strategy,
                    duration: Duration::ZERO,
                    size_bytes: 0,
                    success: false,
                    error: Some(e.to_string()),
                });
                std::fs::remove_dir_all(&dest).ok();
            }
        }
    }

    print_summary(&results);
    Ok(results)
}

fn print_summary(results: &[BenchmarkResult]) {
    println!();
    println!(
        "{:<10} {:>10} {:>12}  {}",
        "Strategy", "Time", "Size", "Status"
    );
    println!("{}", "-".repeat(48));

    for r in results {
        if r.success {
            println!(
                "{:<10} {:>9.1}s {:>12}  ok",
                r.strategy.to_string(),
                r.duration.as_secs_f64(),
                format_size(r.size_bytes),
            );
        } else {
            let err = r.error.as_deref().unwrap_or("unknown");
            let short = if err.len() > 40 {
                format!("{}...", &err[..40])
            } else {
                err.to_string()
            };
            println!(
                "{:<10} {:>10} {:>12}  fail ({short})",
                r.strategy.to_string(),
                "-",
                "-",
            );
        }
    }

    if let Some(fastest) = results
        .iter()
        .filter(|r| r.success)
        .min_by_key(|r| r.duration)
    {
        println!();
        println!(
            "Fastest: {} ({:.1}s, {})",
            fastest.strategy,
            fastest.duration.as_secs_f64(),
            format_size(fastest.size_bytes),
        );
    }
}

fn format_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let b = bytes as f64;
    if b >= GIB {
        format!("{:.2} GiB", b / GIB)
    } else if b >= MIB {
        format!("{:.2} MiB", b / MIB)
    } else if b >= KIB {
        format!("{:.2} KiB", b / KIB)
    } else {
        format!("{bytes} B")
    }
}
