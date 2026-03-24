---
bump: minor
---

### Added

- Full Rust implementation of glab-pull-all, a GitLab repository sync tool
- CLI argument parsing with clap (--group, --user, --token, --ssh, --dir, --threads, etc.)
- GitLab API integration via glab CLI and REST API for fetching repositories
- Git operations: clone, pull, delete, switch-to-default, pull-from-default
- Real-time status display with progress bar, ANSI colors, and live updates
- Parallel processing with configurable concurrency using tokio semaphore-based worker pool
- Error tracking with numbered errors and detailed summary
- Uncommitted changes detection and safety checks
- Support for self-hosted GitLab instances via --gitlab-url
- Comprehensive test suite with 60+ unit and integration tests
