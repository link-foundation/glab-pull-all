//! Integration tests for glab-pull-all.
//!
//! These tests verify the public API works correctly.

use glab_pull_all::cli::Args;
use glab_pull_all::display::{RepoStatus, StatusDisplay};
use glab_pull_all::git_ops::{self, OpType};
use glab_pull_all::gitlab::RepoInfo;

mod cli_tests {
    use super::*;

    #[test]
    fn test_args_validate_group_only() {
        let args = make_args(Some("mygroup"), None);
        assert!(args.validate().is_ok());
    }

    #[test]
    fn test_args_validate_user_only() {
        let args = make_args(None, Some("myuser"));
        assert!(args.validate().is_ok());
    }

    #[test]
    fn test_args_validate_neither() {
        let args = make_args(None, None);
        let err = args.validate().unwrap_err();
        assert!(err.contains("must specify"));
    }

    #[test]
    fn test_args_validate_both() {
        let args = make_args(Some("g"), Some("u"));
        let err = args.validate().unwrap_err();
        assert!(err.contains("cannot specify both"));
    }

    #[test]
    fn test_args_validate_pull_and_switch_conflict() {
        let mut args = make_args(Some("g"), None);
        args.pull_from_default = true;
        args.switch_to_default = true;
        assert!(args.validate().is_err());
    }

    #[test]
    fn test_concurrency_default() {
        let args = make_args(Some("g"), None);
        assert_eq!(args.concurrency(), 8);
    }

    #[test]
    fn test_concurrency_single_thread() {
        let mut args = make_args(Some("g"), None);
        args.single_thread = true;
        assert_eq!(args.concurrency(), 1);
    }

    #[test]
    fn test_concurrency_custom() {
        let mut args = make_args(Some("g"), None);
        args.threads = 16;
        assert_eq!(args.concurrency(), 16);
    }

    #[test]
    fn test_target_name_group() {
        let args = make_args(Some("mygroup"), None);
        assert_eq!(args.target_name(), "mygroup");
    }

    #[test]
    fn test_target_name_user() {
        let args = make_args(None, Some("myuser"));
        assert_eq!(args.target_name(), "myuser");
    }

    #[test]
    fn test_target_type_group() {
        let args = make_args(Some("g"), None);
        assert_eq!(args.target_type(), "group");
    }

    #[test]
    fn test_target_type_user() {
        let args = make_args(None, Some("u"));
        assert_eq!(args.target_type(), "user");
    }

    #[test]
    fn test_effective_live_updates_default() {
        let args = make_args(Some("g"), None);
        assert!(args.effective_live_updates());
    }

    #[test]
    fn test_effective_live_updates_disabled() {
        let mut args = make_args(Some("g"), None);
        args.no_live_updates = true;
        assert!(!args.effective_live_updates());
    }

    fn make_args(group: Option<&str>, user: Option<&str>) -> Args {
        Args {
            group: group.map(String::from),
            user: user.map(String::from),
            token: None,
            ssh: false,
            dir: ".".to_string(),
            preserve_namespace: false,
            threads: 8,
            single_thread: false,
            live_updates: true,
            no_live_updates: false,
            delete: false,
            pull_from_default: false,
            switch_to_default: false,
            gitlab_url: "https://gitlab.com".to_string(),
        }
    }
}

mod display_tests {
    use super::*;

    #[test]
    fn test_status_display_creation() {
        let display = StatusDisplay::new(false, 1);
        display.add_repo("test-repo");
        display.update_repo("test-repo", RepoStatus::Success, "done");
    }

    #[test]
    fn test_status_display_error_tracking() {
        let display = StatusDisplay::new(false, 1);
        display.add_repo("repo1");
        display.add_repo("repo2");
        display.update_repo("repo1", RepoStatus::Failed, "error 1");
        display.update_repo("repo2", RepoStatus::Failed, "error 2");
    }

    #[test]
    fn test_status_transitions() {
        let display = StatusDisplay::new(false, 1);
        display.add_repo("repo");
        display.update_repo("repo", RepoStatus::Cloning, "Cloning...");
        display.update_repo("repo", RepoStatus::Success, "done");
    }

    #[test]
    fn test_repo_status_is_terminal() {
        assert!(RepoStatus::Success.is_terminal());
        assert!(RepoStatus::Failed.is_terminal());
        assert!(RepoStatus::Skipped.is_terminal());
        assert!(RepoStatus::Uncommitted.is_terminal());
        assert!(!RepoStatus::Pending.is_terminal());
        assert!(!RepoStatus::Pulling.is_terminal());
        assert!(!RepoStatus::Cloning.is_terminal());
    }
}

mod git_ops_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_git_result_types() {
        let result = git_ops::GitResult {
            success: true,
            op_type: OpType::Cloned,
            message: "ok".to_string(),
        };
        assert!(result.success);
        assert_eq!(result.op_type, OpType::Cloned);
    }

    #[test]
    fn test_op_type_equality() {
        assert_eq!(OpType::Pulled, OpType::Pulled);
        assert_ne!(OpType::Pulled, OpType::Cloned);
        assert_ne!(OpType::MergeConflict, OpType::Failed);
    }

    #[tokio::test]
    async fn test_has_uncommitted_changes_nonexistent() {
        let result = git_ops::has_uncommitted_changes(Path::new("/nonexistent")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_current_branch_nonexistent() {
        let result = git_ops::current_branch(Path::new("/nonexistent")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_default_branch_fallback() {
        let branch = git_ops::get_default_branch(Path::new("/nonexistent")).await;
        assert_eq!(branch, "main");
    }

    #[tokio::test]
    async fn test_delete_repo_nonexistent() {
        let dir = std::env::temp_dir().join("glab-test-delete-nonexistent");
        let _ = tokio::fs::create_dir_all(&dir).await;
        let result = git_ops::delete_repo("nonexistent", &dir, &|_| {}).await;
        assert!(result.success);
        assert_eq!(result.op_type, OpType::Skipped);
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn test_process_repo_private_no_token() {
        let dir = std::env::temp_dir().join("glab-test-private-skip");
        let _ = tokio::fs::create_dir_all(&dir).await;
        let repo = RepoInfo {
            name: "private-repo".to_string(),
            path_with_namespace: "test/private-repo".to_string(),
            local_path: "private-repo".to_string(),
            clone_url: "https://gitlab.com/test/private-repo.git".to_string(),
            ssh_url: "git@gitlab.com:test/private-repo.git".to_string(),
            web_url: "https://gitlab.com/test/private-repo".to_string(),
            is_private: true,
        };
        let result =
            git_ops::process_repo(&repo, &dir, false, None, false, false, false, &|_| {}).await;
        assert!(result.success);
        assert_eq!(result.op_type, OpType::Skipped);
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}

mod preserve_namespace_tests {
    use super::*;
    use std::process::Command;

    /// Create a local bare repository with one commit and return its path.
    fn make_origin(root: &std::path::Path) -> std::path::PathBuf {
        let work = root.join("work");
        std::fs::create_dir_all(&work).unwrap();
        let git = |args: &[&str], cwd: &std::path::Path| {
            let status = Command::new("git")
                .args(args)
                .current_dir(cwd)
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?} failed");
        };
        git(&["init", "-q", "-b", "main"], &work);
        git(&["config", "user.email", "t@example.com"], &work);
        git(&["config", "user.name", "t"], &work);
        std::fs::write(work.join("README"), "x").unwrap();
        git(&["add", "."], &work);
        git(&["commit", "-q", "-m", "init"], &work);
        let bare = root.join("origin.git");
        git(
            &[
                "clone",
                "-q",
                "--bare",
                work.to_str().unwrap(),
                bare.to_str().unwrap(),
            ],
            root,
        );
        bare
    }

    fn repo(origin: &std::path::Path, preserve: bool) -> RepoInfo {
        let repo = RepoInfo {
            name: "project".to_string(),
            path_with_namespace: "group/sub/project".to_string(),
            local_path: "project".to_string(),
            clone_url: origin.to_str().unwrap().to_string(),
            ssh_url: String::new(),
            web_url: String::new(),
            is_private: false,
        };
        if preserve {
            repo.with_preserved_namespace()
        } else {
            repo
        }
    }

    #[tokio::test]
    async fn test_clone_flat_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        let origin = make_origin(tmp.path());
        let target = tmp.path().join("target");
        std::fs::create_dir_all(&target).unwrap();
        let result = git_ops::process_repo(
            &repo(&origin, false),
            &target,
            false,
            None,
            false,
            false,
            false,
            &|_| {},
        )
        .await;
        assert!(result.success, "{}", result.message);
        assert_eq!(result.op_type, OpType::Cloned);
        assert!(target.join("project").join(".git").is_dir());
        assert!(!target.join("group").exists());
    }

    #[tokio::test]
    async fn test_clone_into_namespace_path() {
        let tmp = tempfile::tempdir().unwrap();
        let origin = make_origin(tmp.path());
        let target = tmp.path().join("target");
        std::fs::create_dir_all(&target).unwrap();
        let r = repo(&origin, true);
        let result =
            git_ops::process_repo(&r, &target, false, None, false, false, false, &|_| {}).await;
        assert!(result.success, "{}", result.message);
        assert_eq!(result.op_type, OpType::Cloned);
        let nested = target.join("group").join("sub").join("project");
        assert!(nested.join(".git").is_dir());
        assert!(!target.join("project").exists());

        // Second run must find the existing clone and pull, not clone again.
        let result =
            git_ops::process_repo(&r, &target, false, None, false, false, false, &|_| {}).await;
        assert!(result.success, "{}", result.message);
        assert_ne!(result.op_type, OpType::Cloned);

        // Delete mode must remove the nested clone.
        let result =
            git_ops::process_repo(&r, &target, false, None, false, false, true, &|_| {}).await;
        assert!(result.success, "{}", result.message);
        assert!(!nested.exists());
    }
}

mod gitlab_tests {
    use super::*;

    #[test]
    fn test_repo_info_clone() {
        let repo = RepoInfo {
            name: "test".to_string(),
            path_with_namespace: "test/test".to_string(),
            local_path: "test".to_string(),
            clone_url: "https://gitlab.com/test.git".to_string(),
            ssh_url: "git@gitlab.com:test.git".to_string(),
            web_url: "https://gitlab.com/test".to_string(),
            is_private: false,
        };
        let cloned = repo;
        assert_eq!(cloned.name, "test");
        assert!(!cloned.is_private);
    }
}

mod runner_tests {

    #[tokio::test]
    async fn test_run_empty_repos() {
        let dir = std::env::temp_dir().join("glab-test-runner-empty");
        let _ = tokio::fs::create_dir_all(&dir).await;
        let results =
            glab_pull_all::runner::run(&[], &dir, false, None, false, false, false, 1, false).await;
        assert!(results.is_empty());
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}

mod version_tests {
    #[test]
    fn test_version_is_not_empty() {
        assert!(!glab_pull_all::VERSION.is_empty());
    }

    #[test]
    fn test_version_format() {
        // Should be semver format
        let parts: Vec<&str> = glab_pull_all::VERSION.split('.').collect();
        assert_eq!(parts.len(), 3);
        for part in parts {
            assert!(part.parse::<u32>().is_ok());
        }
    }
}
