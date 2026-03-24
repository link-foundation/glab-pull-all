//! glab-pull-all: Sync all repositories from a GitLab group or user account.
//!
//! A Rust port of gh-pull-all, adapted for GitLab (glab CLI).

pub mod cli;
#[allow(clippy::option_if_let_else, clippy::significant_drop_tightening)]
pub mod display;
pub mod git_ops;
pub mod gitlab;
pub mod runner;

/// Package version (matches Cargo.toml version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
