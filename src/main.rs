//! glab-pull-all: Sync all repositories from a GitLab group or user account.

use clap::Parser;
use glab_pull_all::cli::Args;
use glab_pull_all::display::{self, colors};
use glab_pull_all::gitlab;
use glab_pull_all::runner;
use std::path::PathBuf;
use std::process;

#[tokio::main]
async fn main() {
    let args = Args::parse();

    if let Err(e) = args.validate() {
        display::log(colors::RED, &format!("❌ {e}"));
        process::exit(1);
    }

    let target_name = args.target_name().to_string();
    let target_type = args.target_type();
    let concurrency = args.concurrency();
    let live_updates = args.effective_live_updates();

    // Resolve target directory
    let target_dir = if args.dir == "." {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        PathBuf::from(&args.dir)
    };

    // Get token: CLI flag > env var > glab CLI
    let mut token = args.token.clone();
    if token.is_none() {
        if let Some(glab_token) = gitlab::get_glab_token().await {
            token = Some(glab_token);
            display::log(colors::CYAN, "🔑 Using GitLab token from glab CLI");
        }
    }

    if args.delete {
        display::log(
            colors::RED,
            &format!("🗑️  Starting {target_name} {target_type} repository deletion..."),
        );
        display::log(
            colors::CYAN,
            &format!("📁 Target directory: {}", target_dir.display()),
        );
        display::log(
            colors::CYAN,
            &format!(
                "⚡ Concurrency: {} {}",
                concurrency,
                if concurrency == 1 {
                    "thread (sequential)"
                } else {
                    "threads (parallel)"
                }
            ),
        );

        if !runner::ask_confirmation(
            "⚠️  Are you sure you want to delete all repositories? (y/N): ",
        ) {
            display::log(colors::YELLOW, "✖️  Operation cancelled");
            process::exit(0);
        }
    } else {
        display::log(
            colors::BLUE,
            &format!("🚀 Starting {target_name} {target_type} repository sync..."),
        );
        display::log(
            colors::CYAN,
            &format!("📁 Target directory: {}", target_dir.display()),
        );
        display::log(
            colors::CYAN,
            &format!(
                "🔗 Using {} for cloning",
                if args.ssh { "SSH" } else { "HTTPS" }
            ),
        );
        if args.pull_from_default {
            display::log(colors::CYAN, "🔀 Pull from default branch: enabled");
        }
        if args.switch_to_default {
            display::log(colors::CYAN, "🔄 Switch to default branch: enabled");
        }
        display::log(
            colors::CYAN,
            &format!(
                "⚡ Concurrency: {} {}",
                concurrency,
                if concurrency == 1 {
                    "thread (sequential)"
                } else {
                    "threads (parallel)"
                }
            ),
        );
    }

    // Ensure target directory exists
    if let Err(e) = tokio::fs::create_dir_all(&target_dir).await {
        display::log(
            colors::RED,
            &format!("❌ Cannot create target directory: {e}"),
        );
        process::exit(1);
    }

    // Fetch repositories: try glab CLI first, then API
    display::log(
        colors::BLUE,
        &format!("🔍 Fetching repositories from {target_name} {target_type}..."),
    );

    let repos = if let Some(repos) =
        gitlab::get_repos_from_glab_cli(args.group.as_deref(), args.user.as_deref()).await
    {
        display::log(
            colors::CYAN,
            "📋 Using glab CLI to fetch repositories (includes private repos)",
        );
        repos
    } else {
        display::log(colors::CYAN, "📋 Using GitLab API to fetch repositories");
        match gitlab::get_repos_from_api(
            &args.gitlab_url,
            args.group.as_deref(),
            args.user.as_deref(),
            token.as_deref(),
        )
        .await
        {
            Ok(repos) => repos,
            Err(e) => {
                display::log(colors::RED, &format!("❌ {e}"));
                if token.is_none() {
                    display::log(
                        colors::YELLOW,
                        "💡 Try providing a GitLab personal access token with --token flag",
                    );
                }
                process::exit(1);
            }
        }
    };

    display::log(
        colors::GREEN,
        &format!("✅ Found {} repositories", repos.len()),
    );

    if repos.is_empty() {
        display::log(colors::YELLOW, "No repositories found. Nothing to do.");
        return;
    }

    let mut repos: Vec<_> = if args.preserve_namespace {
        repos
            .into_iter()
            .map(gitlab::RepoInfo::with_preserved_namespace)
            .collect()
    } else {
        repos
    };
    repos.sort_by_key(|r| r.local_path.to_lowercase());

    // Run operations
    runner::run(
        &repos,
        &target_dir,
        args.ssh,
        token.as_deref(),
        args.pull_from_default,
        args.switch_to_default,
        args.delete,
        concurrency,
        live_updates,
    )
    .await;
}
