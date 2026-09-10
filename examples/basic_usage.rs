//! Basic usage example for glab-pull-all library.
//!
//! Demonstrates how to use the library components programmatically.
//!
//! Run with: `cargo run --example basic_usage`

use glab_pull_all::display::{self, colors, RepoStatus, StatusDisplay};
use glab_pull_all::gitlab::RepoInfo;

fn main() {
    println!("glab-pull-all v{}", glab_pull_all::VERSION);
    println!();

    // Example 1: Status display
    println!("Example 1: Status display");
    let display = StatusDisplay::new(false, 1);
    display.add_repo("my-project");
    display.add_repo("another-project");
    display.update_repo("my-project", RepoStatus::Pulling, "Pulling changes...");
    display.update_repo("my-project", RepoStatus::Success, "Successfully pulled");
    display.update_repo("another-project", RepoStatus::Skipped, "Not a git repo");
    display.print_summary();
    println!();

    // Example 2: RepoInfo creation
    println!("Example 2: Working with RepoInfo");
    let repo = RepoInfo {
        name: "example-repo".to_string(),
        path_with_namespace: "test/example-repo".to_string(),
        local_path: "example-repo".to_string(),
        clone_url: "https://gitlab.com/group/example-repo.git".to_string(),
        ssh_url: "git@gitlab.com:group/example-repo.git".to_string(),
        web_url: "https://gitlab.com/group/example-repo".to_string(),
        is_private: false,
    };
    display::log(
        colors::GREEN,
        &format!("Repo: {} ({})", repo.name, repo.web_url),
    );
}
