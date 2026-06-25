use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    name = "fgc",
    version,
    about = "Fast Git Cloner - optimized cloning for large repositories",
    long_about = None
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Clone a repository with optimized strategy
    Clone(CloneArgs),
    /// Benchmark clone strategies for a repository
    Benchmark(BenchmarkArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct CloneArgs {
    /// Repository URL to clone
    pub url: String,

    /// Clone destination directory (defaults to repo name)
    #[arg(short, long)]
    pub dest: Option<String>,

    /// Clone strategy to use
    #[arg(short, long, value_enum, default_value_t = Strategy::Auto)]
    pub strategy: Strategy,

    /// Sparse checkout paths (comma-separated, used with sparse strategy)
    #[arg(long, value_delimiter = ',')]
    pub paths: Vec<String>,

    /// Shallow clone depth (used with shallow strategy)
    #[arg(long, default_value_t = 1)]
    pub depth: u32,

    /// Number of parallel LFS download jobs
    #[arg(long, default_value_t = 8)]
    pub lfs_jobs: u32,

    /// LFS download backend (auto tries aria2c first)
    #[arg(long, value_enum, default_value_t = LfsBackend::Auto)]
    pub lfs_backend: LfsBackend,

    /// aria2c connections per LFS file
    #[arg(long, default_value_t = 16)]
    pub aria2c_connections: u32,

    /// Local reference repository for object reuse
    #[arg(long)]
    pub reference: Option<String>,

    /// Full-screen TUI dashboard for progress
    #[arg(long)]
    pub tui: bool,

    /// Skip resume prompt and start fresh
    #[arg(long)]
    pub no_resume: bool,

    /// Automatically confirm resume without prompting
    #[arg(long)]
    pub yes: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct BenchmarkArgs {
    /// Repository URL to benchmark
    pub url: String,

    /// Strategies to compare (comma-separated)
    #[arg(long, value_delimiter = ',', default_value = "shallow,blobless,full")]
    pub strategies: Vec<Strategy>,

    /// Sparse checkout paths (for sparse strategy)
    #[arg(long, value_delimiter = ',')]
    pub paths: Vec<String>,

    /// Shallow clone depth
    #[arg(long, default_value_t = 1)]
    pub depth: u32,

    /// Local reference repository for object reuse
    #[arg(long)]
    pub reference: Option<String>,

    /// Keep benchmark clone directories after test
    #[arg(long)]
    pub keep: bool,

    /// Working directory for benchmark clones
    #[arg(long)]
    pub work_dir: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Strategy {
    /// Automatically select the best strategy
    Auto,
    /// Partial clone without blobs (--filter=blob:none)
    Blobless,
    /// Sparse checkout with blobless filter
    Sparse,
    /// Shallow clone (--depth N)
    Shallow,
    /// Standard git clone
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum, Default)]
pub enum LfsBackend {
    /// Use aria2c if available, otherwise git lfs
    #[default]
    Auto,
    /// Force git lfs pull
    Git,
    /// Force aria2c multi-connection download
    Aria2c,
}

impl std::fmt::Display for Strategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Strategy::Auto => write!(f, "auto"),
            Strategy::Blobless => write!(f, "blobless"),
            Strategy::Sparse => write!(f, "sparse"),
            Strategy::Shallow => write!(f, "shallow"),
            Strategy::Full => write!(f, "full"),
        }
    }
}

impl std::fmt::Display for LfsBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LfsBackend::Auto => write!(f, "auto"),
            LfsBackend::Git => write!(f, "git"),
            LfsBackend::Aria2c => write!(f, "aria2c"),
        }
    }
}
