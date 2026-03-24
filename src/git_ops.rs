//! Git operations: clone, pull, delete, switch-to-default, pull-from-default.

use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

/// Result of a git operation.
#[derive(Debug, Clone)]
pub struct GitResult {
    pub success: bool,
    pub op_type: OpType,
    pub message: String,
}

/// Type of git operation that was performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpType {
    Cloned,
    Pulled,
    PulledDefault,
    MergedFromDefault,
    UpToDateWithDefault,
    SwitchedToDefault,
    AlreadyOnDefault,
    Deleted,
    Uncommitted,
    Skipped,
    Failed,
    MergeConflict,
}

/// Run a git command and capture output.
async fn run_git(repo_path: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Failed to run git: {e}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let msg = if stderr.is_empty() { stdout } else { stderr };
        Err(msg)
    }
}

/// Check if a directory has uncommitted changes.
pub async fn has_uncommitted_changes(repo_path: &Path) -> Result<bool, String> {
    let output = run_git(repo_path, &["status", "--porcelain"]).await?;
    Ok(!output.is_empty())
}

/// Get the current branch name.
pub async fn current_branch(repo_path: &Path) -> Result<String, String> {
    run_git(repo_path, &["rev-parse", "--abbrev-ref", "HEAD"]).await
}

/// Detect the default branch (main/master/etc.) from remote.
pub async fn get_default_branch(repo_path: &Path) -> String {
    // Try symbolic-ref from remote HEAD
    if let Ok(remote_head) = run_git(repo_path, &["symbolic-ref", "refs/remotes/origin/HEAD"]).await
    {
        let branch = remote_head.replace("refs/remotes/origin/", "");
        if !branch.is_empty() {
            return branch;
        }
    }

    // Try to set remote HEAD automatically and retry
    if run_git(repo_path, &["remote", "set-head", "origin", "--auto"])
        .await
        .is_ok()
    {
        if let Ok(remote_head) =
            run_git(repo_path, &["symbolic-ref", "refs/remotes/origin/HEAD"]).await
        {
            let branch = remote_head.replace("refs/remotes/origin/", "");
            if !branch.is_empty() {
                return branch;
            }
        }
    }

    // Fallback: check common default branch names from remote branches
    if let Ok(branches_output) = run_git(repo_path, &["branch", "-r"]).await {
        for line in branches_output.lines() {
            let trimmed = line.trim();
            if trimmed.ends_with("/main") {
                return "main".to_string();
            }
        }
        for line in branches_output.lines() {
            let trimmed = line.trim();
            if trimmed.ends_with("/master") {
                return "master".to_string();
            }
        }
        // Use the first remote branch
        for line in branches_output.lines() {
            let trimmed = line.trim();
            if trimmed.contains('/') && !trimmed.contains("HEAD") {
                if let Some(branch) = trimmed.split('/').next_back() {
                    return branch.to_string();
                }
            }
        }
    }

    "main".to_string()
}

/// Clone a repository.
pub async fn clone_repo(
    clone_url: &str,
    repo_name: &str,
    target_dir: &Path,
    on_status: &(dyn Fn(&str) + Send + Sync),
) -> GitResult {
    on_status("Cloning...");

    let output = Command::new("git")
        .args(["clone", clone_url, repo_name])
        .current_dir(target_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;

    match output {
        Ok(o) if o.status.success() => {
            // Fetch all branches
            let repo_path = target_dir.join(repo_name);
            on_status("Fetching all branches...");
            let _ = run_git(&repo_path, &["fetch", "--all"]).await;
            GitResult {
                success: true,
                op_type: OpType::Cloned,
                message: "Successfully cloned".to_string(),
            }
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
            GitResult {
                success: false,
                op_type: OpType::Failed,
                message: format!("Clone failed: {stderr}"),
            }
        }
        Err(e) => GitResult {
            success: false,
            op_type: OpType::Failed,
            message: format!("Error: {e}"),
        },
    }
}

/// Pull updates for an existing repository.
pub async fn pull_repo(
    repo_name: &str,
    target_dir: &Path,
    pull_from_default: bool,
    on_status: &(dyn Fn(&str) + Send + Sync),
) -> GitResult {
    let repo_path = target_dir.join(repo_name);

    // Check uncommitted changes
    on_status("Checking status...");
    match has_uncommitted_changes(&repo_path).await {
        Ok(true) => {
            return GitResult {
                success: true,
                op_type: OpType::Uncommitted,
                message: "Has uncommitted changes, skipped".to_string(),
            };
        }
        Err(e) => {
            return GitResult {
                success: false,
                op_type: OpType::Failed,
                message: format!("Error checking status: {e}"),
            };
        }
        Ok(false) => {}
    }

    // Fetch all branches
    on_status("Fetching all branches...");
    if let Err(e) = run_git(&repo_path, &["fetch", "--all"]).await {
        return GitResult {
            success: false,
            op_type: OpType::Failed,
            message: format!("Fetch failed: {e}"),
        };
    }

    if pull_from_default {
        pull_from_default_branch(&repo_path, repo_name, on_status).await
    } else {
        // Standard pull
        on_status("Pulling changes...");
        match run_git(&repo_path, &["pull"]).await {
            Ok(_) => GitResult {
                success: true,
                op_type: OpType::Pulled,
                message: "Successfully pulled".to_string(),
            },
            Err(e) => GitResult {
                success: false,
                op_type: OpType::Failed,
                message: format!("Pull failed: {e}"),
            },
        }
    }
}

/// Pull from the default branch into current branch.
async fn pull_from_default_branch(
    repo_path: &Path,
    _repo_name: &str,
    on_status: &(dyn Fn(&str) + Send + Sync),
) -> GitResult {
    on_status("Detecting default branch...");
    let default_branch = get_default_branch(repo_path).await;

    let current = match current_branch(repo_path).await {
        Ok(b) => b,
        Err(e) => {
            return GitResult {
                success: false,
                op_type: OpType::Failed,
                message: format!("Cannot detect current branch: {e}"),
            };
        }
    };

    if current == default_branch {
        // On default branch, just pull normally
        on_status(&format!("Pulling {default_branch} (current branch)..."));
        match run_git(repo_path, &["pull"]).await {
            Ok(_) => GitResult {
                success: true,
                op_type: OpType::PulledDefault,
                message: format!("Successfully pulled {default_branch}"),
            },
            Err(e) => GitResult {
                success: false,
                op_type: OpType::Failed,
                message: format!("Pull failed: {e}"),
            },
        }
    } else {
        // Merge from default branch
        let remote_default = format!("origin/{default_branch}");
        on_status(&format!("Merging changes from {default_branch}..."));

        // Check if remote branch exists
        let branches_output = run_git(repo_path, &["branch", "-r"])
            .await
            .unwrap_or_default();
        if !branches_output
            .lines()
            .any(|l| l.trim().contains(&remote_default))
        {
            // Remote default branch not found, just pull current
            on_status("Pulling current branch...");
            match run_git(repo_path, &["pull"]).await {
                Ok(_) => {
                    return GitResult {
                        success: true,
                        op_type: OpType::Pulled,
                        message: "Successfully pulled".to_string(),
                    };
                }
                Err(e) => {
                    return GitResult {
                        success: false,
                        op_type: OpType::Failed,
                        message: format!("Pull failed: {e}"),
                    };
                }
            }
        }

        match run_git(repo_path, &["merge", &remote_default]).await {
            Ok(output) => {
                let is_up_to_date = output.contains("Already up to date");
                if is_up_to_date {
                    GitResult {
                        success: true,
                        op_type: OpType::UpToDateWithDefault,
                        message: format!("Already up to date with {default_branch}"),
                    }
                } else {
                    // Push merged changes
                    on_status("Pushing merged changes...");
                    let push_msg = match run_git(repo_path, &["push"]).await {
                        Ok(_) => String::new(),
                        Err(e) => format!(" (push failed: {e})"),
                    };
                    GitResult {
                        success: true,
                        op_type: OpType::MergedFromDefault,
                        message: format!(
                            "Successfully merged {default_branch} into {current}{push_msg}"
                        ),
                    }
                }
            }
            Err(e) => {
                // Abort the merge if it failed
                let _ = run_git(repo_path, &["merge", "--abort"]).await;
                GitResult {
                    success: false,
                    op_type: OpType::MergeConflict,
                    message: format!("Merge conflict with {default_branch}: {e}"),
                }
            }
        }
    }
}

/// Switch repository to its default branch.
pub async fn switch_to_default(
    repo_name: &str,
    target_dir: &Path,
    on_status: &(dyn Fn(&str) + Send + Sync),
) -> GitResult {
    let repo_path = target_dir.join(repo_name);

    // Check uncommitted changes
    on_status("Checking status...");
    match has_uncommitted_changes(&repo_path).await {
        Ok(true) => {
            return GitResult {
                success: true,
                op_type: OpType::Uncommitted,
                message: "Has uncommitted changes, skipped".to_string(),
            };
        }
        Err(e) => {
            return GitResult {
                success: false,
                op_type: OpType::Failed,
                message: format!("Error: {e}"),
            };
        }
        Ok(false) => {}
    }

    // Fetch all branches
    on_status("Fetching all branches...");
    let _ = run_git(&repo_path, &["fetch", "--all"]).await;

    // Get current and default branches
    let current = match current_branch(&repo_path).await {
        Ok(b) => b,
        Err(e) => {
            return GitResult {
                success: false,
                op_type: OpType::Failed,
                message: format!("Cannot detect current branch: {e}"),
            };
        }
    };

    on_status("Detecting default branch...");
    let default_branch = get_default_branch(&repo_path).await;

    if current == default_branch {
        return GitResult {
            success: true,
            op_type: OpType::AlreadyOnDefault,
            message: format!("Already on default branch: {default_branch}"),
        };
    }

    // Try to switch
    on_status(&format!("Switching to {default_branch}..."));
    if run_git(&repo_path, &["checkout", &default_branch])
        .await
        .is_ok()
    {
        GitResult {
            success: true,
            op_type: OpType::SwitchedToDefault,
            message: format!("Switched from {current} to {default_branch}"),
        }
    } else {
        // Try creating local branch tracking remote
        on_status(&format!("Creating local {default_branch} branch..."));
        let remote_ref = format!("origin/{default_branch}");
        match run_git(
            &repo_path,
            &["checkout", "-b", &default_branch, &remote_ref],
        )
        .await
        {
            Ok(_) => GitResult {
                success: true,
                op_type: OpType::SwitchedToDefault,
                message: format!("Switched from {current} to {default_branch}"),
            },
            Err(e) => GitResult {
                success: false,
                op_type: OpType::Failed,
                message: format!("Could not switch to {default_branch}: {e}"),
            },
        }
    }
}

/// Delete a cloned repository (skips repos with uncommitted changes).
pub async fn delete_repo(
    repo_name: &str,
    target_dir: &Path,
    on_status: &(dyn Fn(&str) + Send + Sync),
) -> GitResult {
    let repo_path = target_dir.join(repo_name);

    if !repo_path.is_dir() {
        return GitResult {
            success: true,
            op_type: OpType::Skipped,
            message: "Not found locally".to_string(),
        };
    }

    // Check for uncommitted changes
    on_status("Checking for uncommitted changes...");
    match has_uncommitted_changes(&repo_path).await {
        Ok(true) => {
            return GitResult {
                success: true,
                op_type: OpType::Uncommitted,
                message: "Has uncommitted changes, skipped".to_string(),
            };
        }
        Err(_) => {
            return GitResult {
                success: true,
                op_type: OpType::Skipped,
                message: "Not a git repository".to_string(),
            };
        }
        Ok(false) => {}
    }

    // Delete the repository
    on_status("Deleting repository...");
    match tokio::fs::remove_dir_all(&repo_path).await {
        Ok(()) => GitResult {
            success: true,
            op_type: OpType::Deleted,
            message: "Successfully deleted".to_string(),
        },
        Err(e) => GitResult {
            success: false,
            op_type: OpType::Failed,
            message: format!("Delete failed: {e}"),
        },
    }
}

/// Process a single repository: clone, pull, switch, or delete as needed.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub async fn process_repo(
    repo: &crate::gitlab::RepoInfo,
    target_dir: &Path,
    use_ssh: bool,
    token: Option<&str>,
    pull_from_default: bool,
    switch_to_default_flag: bool,
    delete_mode: bool,
    on_status: &(dyn Fn(&str) + Send + Sync),
) -> GitResult {
    if delete_mode {
        return delete_repo(&repo.name, target_dir, on_status).await;
    }

    let repo_path = target_dir.join(&repo.name);
    let exists = repo_path.is_dir();

    // Skip private repo without token if not yet cloned
    if repo.is_private && token.is_none() && !exists {
        return GitResult {
            success: true,
            op_type: OpType::Skipped,
            message: "Private repo, no token provided".to_string(),
        };
    }

    if exists {
        if switch_to_default_flag {
            switch_to_default(&repo.name, target_dir, on_status).await
        } else {
            pull_repo(&repo.name, target_dir, pull_from_default, on_status).await
        }
    } else {
        let clone_url = if use_ssh {
            &repo.ssh_url
        } else {
            &repo.clone_url
        };
        clone_repo(clone_url, &repo.name, target_dir, on_status).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_result_debug() {
        let result = GitResult {
            success: true,
            op_type: OpType::Cloned,
            message: "test".to_string(),
        };
        assert!(result.success);
        assert_eq!(result.op_type, OpType::Cloned);
    }

    #[test]
    fn test_op_type_equality() {
        assert_eq!(OpType::Pulled, OpType::Pulled);
        assert_ne!(OpType::Pulled, OpType::Cloned);
    }

    #[tokio::test]
    async fn test_has_uncommitted_changes_nonexistent_dir() {
        let result = has_uncommitted_changes(Path::new("/nonexistent/dir")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_current_branch_nonexistent_dir() {
        let result = current_branch(Path::new("/nonexistent/dir")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_default_branch_nonexistent_dir() {
        // Should fall back to "main"
        let branch = get_default_branch(Path::new("/nonexistent/dir")).await;
        assert_eq!(branch, "main");
    }

    #[tokio::test]
    async fn test_delete_repo_nonexistent() {
        let target = Path::new("/tmp/glab-pull-all-test-nonexistent");
        let result = delete_repo("nonexistent", target, &|_| {}).await;
        assert!(result.success);
        assert_eq!(result.op_type, OpType::Skipped);
    }
}
