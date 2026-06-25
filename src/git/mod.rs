mod executor;
mod progress;

pub use executor::{
    apply_sparse_checkout, default_dest, dir_size, run_clone, run_clone_timed, CloneRunOptions,
};
pub use progress::CloneProgress;
