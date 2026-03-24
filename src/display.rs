//! Status display system with real-time terminal output.

use crossterm::terminal;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Status of a repository operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoStatus {
    Pending,
    Cloning,
    Pulling,
    Checking,
    Deleting,
    Success,
    Failed,
    Skipped,
    Uncommitted,
}

impl RepoStatus {
    const fn icon(self) -> &'static str {
        match self {
            Self::Pending => "⏳",
            Self::Cloning => "📦",
            Self::Pulling => "📥",
            Self::Checking => "🔍",
            Self::Deleting => "🗑️ ",
            Self::Success => "✅",
            Self::Failed => "❌",
            Self::Skipped => "⚠️ ",
            Self::Uncommitted => "🔄",
        }
    }

    const fn color(self) -> &'static str {
        match self {
            Self::Pending => "\x1b[2m", // dim
            Self::Cloning | Self::Pulling | Self::Checking | Self::Deleting => "\x1b[36m", // cyan
            Self::Success => "\x1b[32m", // green
            Self::Failed => "\x1b[31m", // red
            Self::Skipped | Self::Uncommitted => "\x1b[33m", // yellow
        }
    }

    const fn is_active(self) -> bool {
        matches!(
            self,
            Self::Cloning | Self::Pulling | Self::Checking | Self::Deleting
        )
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Success | Self::Failed | Self::Skipped | Self::Uncommitted
        )
    }
}

/// ANSI color constants.
pub mod colors {
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const BLUE: &str = "\x1b[34m";
    pub const RED: &str = "\x1b[31m";
    pub const CYAN: &str = "\x1b[36m";
    pub const DIM: &str = "\x1b[2m";
    pub const BOLD: &str = "\x1b[1m";
    pub const RESET: &str = "\x1b[0m";
}

/// Logged message helper.
pub fn log(color: &str, message: &str) {
    println!("{color}{message}{}", colors::RESET);
}

/// Info about a single repo's display state.
#[derive(Debug, Clone)]
struct RepoDisplayInfo {
    status: RepoStatus,
    message: String,
    start_time: Instant,
    end_time: Option<Instant>,
    error_number: Option<usize>,
    _logged: bool,
}

/// Error tracking entry.
#[derive(Debug, Clone)]
struct ErrorEntry {
    number: usize,
    repo: String,
    message: String,
}

/// Thread-safe status display.
#[derive(Clone)]
pub struct StatusDisplay {
    inner: Arc<Mutex<StatusDisplayInner>>,
}

#[allow(clippy::struct_excessive_bools)]
struct StatusDisplayInner {
    repos: BTreeMap<String, RepoDisplayInfo>,
    start_time: Instant,
    errors: Vec<ErrorEntry>,
    error_counter: usize,
    max_name_length: usize,
    live_updates: bool,
    use_in_place_updates: bool,
    threads: usize,
    header_printed: bool,
    rendered_once: bool,
    last_rendered_count: usize,
    completed_repos: Vec<String>,
    terminal_width: u16,
    terminal_height: u16,
}

impl StatusDisplay {
    /// Create a new `StatusDisplay`.
    #[must_use]
    pub fn new(live_updates: bool, threads: usize) -> Self {
        let is_interactive = atty_stdout() && std::env::var("CI").is_err();
        let (tw, th) = terminal::size().unwrap_or((80, 24));

        Self {
            inner: Arc::new(Mutex::new(StatusDisplayInner {
                repos: BTreeMap::new(),
                start_time: Instant::now(),
                errors: Vec::new(),
                error_counter: 0,
                max_name_length: 0,
                live_updates,
                use_in_place_updates: live_updates && is_interactive && threads > 1,
                threads,
                header_printed: false,
                rendered_once: false,
                last_rendered_count: 0,
                completed_repos: Vec::new(),
                terminal_width: tw,
                terminal_height: th,
            })),
        }
    }

    /// Add a repo to the display.
    pub fn add_repo(&self, name: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.max_name_length = inner.max_name_length.max(name.len());
        inner.repos.insert(
            name.to_string(),
            RepoDisplayInfo {
                status: RepoStatus::Pending,
                message: String::new(),
                start_time: Instant::now(),
                end_time: None,
                error_number: None,
                _logged: false,
            },
        );
    }

    /// Update a repo's status.
    pub fn update_repo(&self, name: &str, status: RepoStatus, message: &str) {
        let mut inner = self.inner.lock().unwrap();

        // Check if we need to assign an error number
        let needs_error = status == RepoStatus::Failed
            && inner
                .repos
                .get(name)
                .is_some_and(|r| r.error_number.is_none());

        let error_num = if needs_error {
            inner.error_counter += 1;
            let num = inner.error_counter;
            inner.errors.push(ErrorEntry {
                number: num,
                repo: name.to_string(),
                message: message.to_string(),
            });
            Some(num)
        } else {
            None
        };

        let old_status = inner.repos.get(name).map(|r| r.status);

        if let Some(repo) = inner.repos.get_mut(name) {
            repo.status = status;
            repo.message = message.to_string();
            if status != RepoStatus::Pending {
                repo.end_time = Some(Instant::now());
            }
            if let Some(num) = error_num {
                repo.error_number = Some(num);
            }
        }

        if let Some(old) = old_status {
            if !inner.use_in_place_updates {
                log_status_change(&inner, name, old);
            }
        }
    }

    /// Render live updates (called from render loop).
    pub fn render(&self) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.use_in_place_updates {
            return;
        }
        render_live(&mut inner);
    }

    /// Print error list.
    pub fn print_errors(&self) {
        let inner = self.inner.lock().unwrap();
        if inner.errors.is_empty() {
            return;
        }
        println!();
        log(
            colors::RED,
            &format!(
                "{BOLD}❌ Errors:{RESET}",
                BOLD = colors::BOLD,
                RESET = colors::RESET
            ),
        );
        let width = inner.terminal_width.min(80) as usize;
        println!(
            "{DIM}{sep}{RESET}",
            DIM = colors::DIM,
            sep = "─".repeat(width),
            RESET = colors::RESET
        );
        for error in &inner.errors {
            println!(
                "{RED}#{num:>2} {YELLOW}{repo}{RESET}: {msg}",
                RED = colors::RED,
                num = error.number,
                YELLOW = colors::YELLOW,
                repo = error.repo,
                RESET = colors::RESET,
                msg = error.message
            );
        }
    }

    /// Print final summary.
    pub fn print_summary(&self) {
        self.print_errors();

        let inner = self.inner.lock().unwrap();
        let mut cloned = 0u32;
        let mut pulled = 0u32;
        let mut merged_from_default = 0u32;
        let mut up_to_date_with_default = 0u32;
        let mut switched_to_default = 0u32;
        let mut already_on_default = 0u32;
        let mut deleted = 0u32;
        let mut failed = 0u32;
        let mut skipped = 0u32;
        let mut uncommitted = 0u32;
        let mut merge_conflicts = 0u32;

        for repo in inner.repos.values() {
            match repo.status {
                RepoStatus::Success => {
                    if repo.message.contains("cloned") {
                        cloned += 1;
                    } else if repo.message.contains("merged") && repo.message.contains("into") {
                        merged_from_default += 1;
                    } else if repo.message.contains("up to date with") {
                        up_to_date_with_default += 1;
                    } else if repo.message.contains("Switched from") {
                        switched_to_default += 1;
                    } else if repo.message.contains("Already on default branch") {
                        already_on_default += 1;
                    } else if repo.message.contains("pulled") {
                        pulled += 1;
                    } else if repo.message.contains("deleted") {
                        deleted += 1;
                    }
                }
                RepoStatus::Failed => {
                    if repo.message.contains("Merge conflict") {
                        merge_conflicts += 1;
                    } else {
                        failed += 1;
                    }
                }
                RepoStatus::Skipped => skipped += 1,
                RepoStatus::Uncommitted => uncommitted += 1,
                _ => {}
            }
        }

        println!();
        log(
            colors::BLUE,
            &format!(
                "{BOLD}📊 Summary:{RESET}",
                BOLD = colors::BOLD,
                RESET = colors::RESET
            ),
        );
        if cloned > 0 {
            log(colors::GREEN, &format!("✅ Cloned: {cloned}"));
        }
        if pulled > 0 {
            log(colors::GREEN, &format!("✅ Pulled: {pulled}"));
        }
        if merged_from_default > 0 {
            log(
                colors::GREEN,
                &format!("🔀 Merged from default branch: {merged_from_default}"),
            );
        }
        if up_to_date_with_default > 0 {
            log(
                colors::GREEN,
                &format!("✅ Up to date with default: {up_to_date_with_default}"),
            );
        }
        if switched_to_default > 0 {
            log(
                colors::GREEN,
                &format!("🔄 Switched to default branch: {switched_to_default}"),
            );
        }
        if already_on_default > 0 {
            log(
                colors::GREEN,
                &format!("✅ Already on default branch: {already_on_default}"),
            );
        }
        if deleted > 0 {
            log(colors::GREEN, &format!("✅ Deleted: {deleted}"));
        }
        if uncommitted > 0 {
            log(
                colors::YELLOW,
                &format!("🔄 Uncommitted changes: {uncommitted}"),
            );
        }
        if skipped > 0 {
            log(colors::YELLOW, &format!("⚠️  Skipped: {skipped}"));
        }
        if merge_conflicts > 0 {
            log(
                colors::RED,
                &format!("💥 Merge conflicts: {merge_conflicts}"),
            );
        }
        if failed > 0 {
            log(colors::RED, &format!("❌ Failed: {failed}"));
        }

        let total_time = inner.start_time.elapsed().as_secs_f64();
        log(colors::BLUE, &format!("⏱️  Total time: {total_time:.1}s"));
        log(colors::BLUE, "🎉 Operation completed!");
    }

    /// Whether live in-place updates are enabled.
    #[must_use]
    pub fn uses_in_place_updates(&self) -> bool {
        self.inner.lock().unwrap().use_in_place_updates
    }
}

/// Log a single status change in append-only mode.
fn log_status_change(inner: &StatusDisplayInner, name: &str, old_status: RepoStatus) {
    let Some(repo) = inner.repos.get(name) else {
        return;
    };

    if repo.status == RepoStatus::Pending || repo.status == old_status {
        return;
    }

    // For single thread or no-live-updates, skip intermediate states
    if (inner.threads == 1 || (!inner.live_updates && inner.threads > 1)) && repo.status.is_active()
    {
        return;
    }

    let icon = repo.status.icon();
    let color = repo.status.color();
    let duration = if repo.status.is_terminal() {
        if let Some(end) = repo.end_time {
            format!("{:.1}s", end.duration_since(repo.start_time).as_secs_f64())
        } else {
            format!("{:.1}s", repo.start_time.elapsed().as_secs_f64())
        }
    } else {
        format!("{:.1}s", repo.start_time.elapsed().as_secs_f64())
    };

    let display_message = if repo.status == RepoStatus::Failed {
        if let Some(num) = repo.error_number {
            format!("Failed with error #{num}")
        } else {
            truncate_message(&repo.message, 60)
        }
    } else {
        truncate_message(&repo.message, 60)
    };

    println!(
        "{color}{icon} {name:<width$} {DIM}{dur:>6}{RESET} {msg}",
        color = color,
        icon = icon,
        name = name,
        width = inner.max_name_length,
        DIM = colors::DIM,
        dur = duration,
        RESET = colors::RESET,
        msg = display_message
    );
}

/// Render live in-place updates.
fn render_live(inner: &mut StatusDisplayInner) {
    // Refresh terminal size
    if let Ok((tw, th)) = terminal::size() {
        inner.terminal_width = tw;
        inner.terminal_height = th;
    }

    if !inner.header_printed {
        println!(
            "\n{BOLD}Repository Status{RESET}",
            BOLD = colors::BOLD,
            RESET = colors::RESET
        );
        let sep_width = 80.min(inner.terminal_width as usize);
        println!(
            "{DIM}{sep}{RESET}",
            DIM = colors::DIM,
            sep = "─".repeat(sep_width),
            RESET = colors::RESET
        );
        inner.header_printed = true;
    }

    // Separate active and newly completed repos
    let mut active_repos = Vec::new();
    let mut newly_completed = Vec::new();

    for (name, repo) in &inner.repos {
        if repo.status.is_active() || repo.status == RepoStatus::Pending {
            active_repos.push(name.clone());
        } else if !inner.completed_repos.contains(name) {
            newly_completed.push(name.clone());
        }
    }

    inner.completed_repos.extend(newly_completed.clone());

    // Move cursor up for previously rendered active lines
    if inner.rendered_once && inner.last_rendered_count > 0 {
        print!("\x1b[{}A", inner.last_rendered_count);
    }

    let batch_size = inner
        .threads
        .min((inner.terminal_height as usize).saturating_sub(8))
        .max(1);

    // Print newly completed repos (permanent)
    for name in &newly_completed {
        if let Some(repo) = inner.repos.get(name) {
            print_repo_line(inner, name, repo);
        }
    }

    // Render current active batch
    let mut rendered_count = 0;
    for name in active_repos.iter().take(batch_size) {
        if let Some(repo) = inner.repos.get(name) {
            print!("\x1b[2K"); // Clear line
            print_repo_line(inner, name, repo);
            rendered_count += 1;
        }
    }

    // Progress bar
    print!("\x1b[2K");
    println!();
    rendered_count += 1;

    print!("\x1b[2K");
    println!(
        "{DIM}Progress: {GREEN}█{DIM}=success {RED}█{DIM}=failed {YELLOW}█{DIM}=skipped {CYAN}█{DIM}=in progress {DIM}░=pending{RESET}",
        DIM = colors::DIM, GREEN = colors::GREEN, RED = colors::RED,
        YELLOW = colors::YELLOW, CYAN = colors::CYAN, RESET = colors::RESET
    );
    rendered_count += 1;

    let progress_bar = create_progress_bar(inner);
    if !progress_bar.is_empty() {
        print!("\x1b[2K");
        println!("{progress_bar}");
        rendered_count += 1;
    }

    inner.rendered_once = true;
    inner.last_rendered_count = rendered_count;
}

/// Print a single repo status line.
fn print_repo_line(inner: &StatusDisplayInner, name: &str, repo: &RepoDisplayInfo) {
    let icon = repo.status.icon();
    let color = repo.status.color();
    let duration = if repo.status.is_terminal() {
        if let Some(end) = repo.end_time {
            format!("{:.1}s", end.duration_since(repo.start_time).as_secs_f64())
        } else {
            format!("{:.1}s", repo.start_time.elapsed().as_secs_f64())
        }
    } else {
        format!("{:.1}s", repo.start_time.elapsed().as_secs_f64())
    };

    let display_message = if repo.status == RepoStatus::Failed {
        if let Some(num) = repo.error_number {
            format!("Failed with error #{num}")
        } else {
            truncate_message(&repo.message, 60)
        }
    } else {
        truncate_message(&repo.message, 60)
    };

    println!(
        "{color}{icon} {name:<width$} {DIM}{dur:>6}{RESET} {msg}",
        color = color,
        icon = icon,
        name = name,
        width = inner.max_name_length,
        DIM = colors::DIM,
        dur = duration,
        RESET = colors::RESET,
        msg = display_message
    );
}

/// Create the progress bar string.
fn create_progress_bar(inner: &StatusDisplayInner) -> String {
    let repo_count = inner.repos.len();
    if repo_count == 0 {
        return String::new();
    }

    let mut success = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut in_progress = 0usize;

    for repo in inner.repos.values() {
        match repo.status {
            RepoStatus::Success => success += 1,
            RepoStatus::Failed => failed += 1,
            RepoStatus::Skipped | RepoStatus::Uncommitted => skipped += 1,
            s if s.is_active() => in_progress += 1,
            _ => {} // pending
        }
    }

    let completed = success + failed + skipped;
    let bar_width = 50.min(inner.terminal_width.saturating_sub(40) as usize);

    let success_w = (success * bar_width) / repo_count;
    let failed_w = (failed * bar_width) / repo_count;
    let skipped_w = (skipped * bar_width) / repo_count;
    let in_progress_w = (in_progress * bar_width) / repo_count;
    let pending_w = bar_width.saturating_sub(success_w + failed_w + skipped_w + in_progress_w);

    let bar = format!(
        "{GREEN}{s}{RED}{f}{YELLOW}{sk}{CYAN}{ip}{DIM}{p}{RESET}",
        GREEN = colors::GREEN,
        s = "█".repeat(success_w),
        RED = colors::RED,
        f = "█".repeat(failed_w),
        YELLOW = colors::YELLOW,
        sk = "█".repeat(skipped_w),
        CYAN = colors::CYAN,
        ip = "█".repeat(in_progress_w),
        DIM = colors::DIM,
        p = "░".repeat(pending_w),
        RESET = colors::RESET,
    );

    let percentage = (completed * 100) / repo_count;
    let error_text = if failed > 0 {
        format!(
            " {RED}{failed} errors{RESET}",
            RED = colors::RED,
            RESET = colors::RESET
        )
    } else {
        String::new()
    };

    format!("[{bar}] {completed}/{repo_count} ({percentage}%){error_text}")
}

/// Truncate a message to fit within max length.
fn truncate_message(message: &str, max_length: usize) -> String {
    if message.len() <= max_length {
        message.to_string()
    } else {
        format!("{}...", &message[..max_length.saturating_sub(3)])
    }
}

/// Check if stdout is a TTY.
fn atty_stdout() -> bool {
    crossterm::tty::IsTty::is_tty(&std::io::stdout())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repo_status_icon() {
        assert_eq!(RepoStatus::Pending.icon(), "⏳");
        assert_eq!(RepoStatus::Success.icon(), "✅");
        assert_eq!(RepoStatus::Failed.icon(), "❌");
    }

    #[test]
    fn test_repo_status_is_active() {
        assert!(RepoStatus::Cloning.is_active());
        assert!(RepoStatus::Pulling.is_active());
        assert!(!RepoStatus::Success.is_active());
        assert!(!RepoStatus::Pending.is_active());
    }

    #[test]
    fn test_repo_status_is_terminal() {
        assert!(RepoStatus::Success.is_terminal());
        assert!(RepoStatus::Failed.is_terminal());
        assert!(RepoStatus::Skipped.is_terminal());
        assert!(!RepoStatus::Pending.is_terminal());
        assert!(!RepoStatus::Pulling.is_terminal());
    }

    #[test]
    fn test_truncate_message_short() {
        assert_eq!(truncate_message("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_message_long() {
        let result = truncate_message("this is a very long message", 15);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 15);
    }

    #[test]
    fn test_truncate_message_exact() {
        assert_eq!(truncate_message("12345", 5), "12345");
    }

    #[test]
    fn test_status_display_add_and_update() {
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

        let inner = display.inner.lock().unwrap();
        assert_eq!(inner.errors.len(), 2);
        assert_eq!(inner.errors[0].number, 1);
        assert_eq!(inner.errors[1].number, 2);
    }
}
