//! Runner: orchestrates parallel/sequential repository processing.

use crate::display::{self, RepoStatus, StatusDisplay};
use crate::git_ops;
use crate::gitlab::RepoInfo;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::{interval, Duration};

/// Run the main sync operation on all repos.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub async fn run(
    repos: &[RepoInfo],
    target_dir: &Path,
    use_ssh: bool,
    token: Option<&str>,
    pull_from_default: bool,
    switch_to_default: bool,
    delete_mode: bool,
    concurrency: usize,
    live_updates: bool,
) -> Vec<git_ops::GitResult> {
    let status_display = StatusDisplay::new(live_updates, concurrency);

    // Add all repos to display
    for repo in repos {
        status_display.add_repo(&repo.name);
    }

    // Start render loop if using live updates
    let render_display = status_display.clone();
    let render_handle = if status_display.uses_in_place_updates() {
        Some(tokio::spawn(async move {
            let mut tick = interval(Duration::from_millis(100)); // 10 FPS
            loop {
                tick.tick().await;
                render_display.render();
            }
        }))
    } else {
        None
    };

    let results = if concurrency == 1 {
        run_sequential(
            repos,
            target_dir,
            use_ssh,
            token,
            pull_from_default,
            switch_to_default,
            delete_mode,
            &status_display,
        )
        .await
    } else {
        run_parallel(
            repos,
            target_dir,
            use_ssh,
            token,
            pull_from_default,
            switch_to_default,
            delete_mode,
            concurrency,
            &status_display,
        )
        .await
    };

    // Stop render loop
    if let Some(handle) = render_handle {
        handle.abort();
        // Final render
        status_display.render();
    }

    // Print summary
    status_display.print_summary();

    results
}

/// Process repos sequentially.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
async fn run_sequential(
    repos: &[RepoInfo],
    target_dir: &Path,
    use_ssh: bool,
    token: Option<&str>,
    pull_from_default: bool,
    switch_to_default: bool,
    delete_mode: bool,
    status_display: &StatusDisplay,
) -> Vec<git_ops::GitResult> {
    let mut results = Vec::with_capacity(repos.len());

    for repo in repos {
        let sd = status_display.clone();
        let name = repo.name.clone();
        let on_status = move |msg: &str| {
            let status = match msg {
                m if m.contains("Cloning") => RepoStatus::Cloning,
                m if m.contains("Pulling")
                    || m.contains("Fetching")
                    || m.contains("Merging")
                    || m.contains("Switching")
                    || m.contains("Detecting")
                    || m.contains("Creating")
                    || m.contains("Pushing") =>
                {
                    RepoStatus::Pulling
                }
                m if m.contains("Checking") => RepoStatus::Checking,
                m if m.contains("Deleting") => RepoStatus::Deleting,
                _ => RepoStatus::Pulling,
            };
            sd.update_repo(&name, status, msg);
        };

        let result = git_ops::process_repo(
            repo,
            target_dir,
            use_ssh,
            token,
            pull_from_default,
            switch_to_default,
            delete_mode,
            &on_status,
        )
        .await;

        // Update final status
        let final_status = if result.success {
            if result.op_type == git_ops::OpType::Uncommitted {
                RepoStatus::Uncommitted
            } else if result.op_type == git_ops::OpType::Skipped {
                RepoStatus::Skipped
            } else {
                RepoStatus::Success
            }
        } else {
            RepoStatus::Failed
        };
        status_display.update_repo(&repo.name, final_status, &result.message);

        results.push(result);
    }

    results
}

/// Process repos in parallel with a worker pool.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
async fn run_parallel(
    repos: &[RepoInfo],
    target_dir: &Path,
    use_ssh: bool,
    token: Option<&str>,
    pull_from_default: bool,
    switch_to_default: bool,
    delete_mode: bool,
    concurrency: usize,
    status_display: &StatusDisplay,
) -> Vec<git_ops::GitResult> {
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let target_dir = target_dir.to_path_buf();
    let token = token.map(String::from);

    let mut handles = Vec::with_capacity(repos.len());

    for repo in repos {
        let sem = semaphore.clone();
        let sd = status_display.clone();
        let repo = repo.clone();
        let target_dir = target_dir.clone();
        let token = token.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();

            let sd_clone = sd.clone();
            let name = repo.name.clone();
            let on_status = move |msg: &str| {
                let status = match msg {
                    m if m.contains("Cloning") => RepoStatus::Cloning,
                    m if m.contains("Pulling")
                        || m.contains("Fetching")
                        || m.contains("Merging")
                        || m.contains("Switching")
                        || m.contains("Detecting")
                        || m.contains("Creating")
                        || m.contains("Pushing") =>
                    {
                        RepoStatus::Pulling
                    }
                    m if m.contains("Checking") => RepoStatus::Checking,
                    m if m.contains("Deleting") => RepoStatus::Deleting,
                    _ => RepoStatus::Pulling,
                };
                sd_clone.update_repo(&name, status, msg);
            };

            let result = git_ops::process_repo(
                &repo,
                &target_dir,
                use_ssh,
                token.as_deref(),
                pull_from_default,
                switch_to_default,
                delete_mode,
                &on_status,
            )
            .await;

            // Update final status
            let final_status = if result.success {
                if result.op_type == git_ops::OpType::Uncommitted {
                    RepoStatus::Uncommitted
                } else if result.op_type == git_ops::OpType::Skipped {
                    RepoStatus::Skipped
                } else {
                    RepoStatus::Success
                }
            } else {
                RepoStatus::Failed
            };
            sd.update_repo(&repo.name, final_status, &result.message);

            result
        });

        handles.push(handle);
    }

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        match handle.await {
            Ok(result) => results.push(result),
            Err(e) => results.push(git_ops::GitResult {
                success: false,
                op_type: git_ops::OpType::Failed,
                message: format!("Task panicked: {e}"),
            }),
        }
    }

    results
}

/// Ask user for confirmation (delete mode).
#[must_use]
pub fn ask_confirmation(question: &str) -> bool {
    use std::io::BufRead;

    eprint!(
        "{}{question}{}",
        display::colors::YELLOW,
        display::colors::RESET
    );
    let stdin = std::io::stdin();
    let mut line = String::new();
    if stdin.lock().read_line(&mut line).is_ok() {
        let answer = line.trim().to_lowercase();
        return answer == "y" || answer == "yes";
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_run_empty_repos() {
        let dir = std::env::temp_dir().join("glab-pull-all-test-runner");
        let _ = tokio::fs::create_dir_all(&dir).await;
        let results = run(&[], &dir, false, None, false, false, false, 1, false).await;
        assert!(results.is_empty());
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
