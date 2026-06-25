mod parser;
mod state;

pub use parser::{apply_git_line, apply_lfs_progress};
pub use state::CloneStatus;
