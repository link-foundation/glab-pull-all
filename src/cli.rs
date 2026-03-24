//! CLI argument parsing for glab-pull-all.

use clap::Parser;

/// A powerful command-line tool for efficiently syncing all repositories
/// from a GitLab group or user account with parallel processing,
/// real-time status updates, and advanced features.
#[derive(Parser, Debug, Clone)]
#[command(name = "glab-pull-all", version = crate::VERSION)]
#[command(about = "Sync all repositories from a GitLab group or user account")]
#[allow(clippy::struct_excessive_bools)]
pub struct Args {
    /// GitLab group (organization) name
    #[arg(short = 'g', long)]
    pub group: Option<String>,

    /// GitLab username
    #[arg(short, long)]
    pub user: Option<String>,

    /// GitLab personal access token (optional for public repos, defaults to `GITLAB_TOKEN` env var)
    #[arg(short, long, env = "GITLAB_TOKEN")]
    pub token: Option<String>,

    /// Use SSH URLs for cloning (requires SSH key setup)
    #[arg(short, long, default_value_t = false)]
    pub ssh: bool,

    /// Target directory for repositories
    #[arg(short, long, default_value = ".")]
    pub dir: String,

    /// Number of concurrent operations
    #[arg(short = 'j', long, default_value_t = 8)]
    pub threads: usize,

    /// Run operations sequentially (equivalent to --threads 1)
    #[arg(long, default_value_t = false)]
    pub single_thread: bool,

    /// Enable live in-place status updates (use --no-live-updates to disable)
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub live_updates: bool,

    /// Disable live in-place status updates for terminal history preservation
    #[arg(long, default_value_t = false)]
    pub no_live_updates: bool,

    /// Delete all cloned repositories (skips repos with uncommitted changes)
    #[arg(long, default_value_t = false)]
    pub delete: bool,

    /// Pull changes from the default branch (main/master) into the current branch if behind
    #[arg(long, default_value_t = false)]
    pub pull_from_default: bool,

    /// Switch to the default branch (main/master) in each repository
    #[arg(long, default_value_t = false)]
    pub switch_to_default: bool,

    /// GitLab API base URL (defaults to <https://gitlab.com>)
    #[arg(long, default_value = "https://gitlab.com")]
    pub gitlab_url: String,
}

impl Args {
    /// Validate CLI arguments and return errors if invalid.
    pub fn validate(&self) -> Result<(), String> {
        if self.group.is_none() && self.user.is_none() {
            return Err("You must specify either --group or --user".to_string());
        }
        if self.group.is_some() && self.user.is_some() {
            return Err("You cannot specify both --group and --user".to_string());
        }
        if self.threads < 1 {
            return Err("Thread count must be at least 1".to_string());
        }
        if self.single_thread && self.threads != 8 {
            return Err("Cannot specify both --single-thread and --threads".to_string());
        }
        if self.pull_from_default && self.switch_to_default {
            return Err(
                "Cannot specify both --pull-from-default and --switch-to-default".to_string(),
            );
        }
        Ok(())
    }

    /// Get the effective concurrency limit.
    #[must_use]
    pub const fn concurrency(&self) -> usize {
        if self.single_thread {
            1
        } else {
            self.threads
        }
    }

    /// Get the effective live updates setting.
    #[must_use]
    pub const fn effective_live_updates(&self) -> bool {
        if self.no_live_updates {
            return false;
        }
        self.live_updates
    }

    /// Get the target name (group or user).
    #[must_use]
    pub fn target_name(&self) -> &str {
        self.group.as_deref().or(self.user.as_deref()).unwrap_or("")
    }

    /// Get the target type label.
    #[must_use]
    pub const fn target_type(&self) -> &str {
        if self.group.is_some() {
            "group"
        } else {
            "user"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_args(group: Option<&str>, user: Option<&str>) -> Args {
        Args {
            group: group.map(String::from),
            user: user.map(String::from),
            token: None,
            ssh: false,
            dir: ".".to_string(),
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

    #[test]
    fn test_validate_requires_group_or_user() {
        let args = make_args(None, None);
        assert!(args.validate().is_err());
    }

    #[test]
    fn test_validate_rejects_both_group_and_user() {
        let args = make_args(Some("mygroup"), Some("myuser"));
        assert!(args.validate().is_err());
    }

    #[test]
    fn test_validate_accepts_group_only() {
        let args = make_args(Some("mygroup"), None);
        assert!(args.validate().is_ok());
    }

    #[test]
    fn test_validate_accepts_user_only() {
        let args = make_args(None, Some("myuser"));
        assert!(args.validate().is_ok());
    }

    #[test]
    fn test_validate_rejects_pull_and_switch() {
        let mut args = make_args(Some("mygroup"), None);
        args.pull_from_default = true;
        args.switch_to_default = true;
        assert!(args.validate().is_err());
    }

    #[test]
    fn test_concurrency_single_thread() {
        let mut args = make_args(Some("g"), None);
        args.single_thread = true;
        assert_eq!(args.concurrency(), 1);
    }

    #[test]
    fn test_concurrency_multi_thread() {
        let mut args = make_args(Some("g"), None);
        args.threads = 16;
        assert_eq!(args.concurrency(), 16);
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
}
