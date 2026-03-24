# glab-pull-all

A powerful command-line tool for efficiently syncing all repositories from a GitLab group or user account with parallel processing, real-time status updates, and advanced features.

[![CI/CD Pipeline](https://github.com/link-foundation/glab-pull-all/workflows/CI%2FCD%20Pipeline/badge.svg)](https://github.com/link-foundation/glab-pull-all/actions)
[![Rust Version](https://img.shields.io/badge/rust-1.70%2B-blue.svg)](https://www.rust-lang.org/)
[![License: Unlicense](https://img.shields.io/badge/license-Unlicense-blue.svg)](http://unlicense.org/)

> Rust port of [gh-pull-all](https://github.com/link-foundation/gh-pull-all), adapted for GitLab (glab CLI).

## Features

- **Clone & Sync**: Clones new repositories and pulls updates to existing ones
- **Group & User Support**: Works with both GitLab groups and individual user accounts
- **Parallel Processing**: Configurable thread count (default 8) for concurrent operations
- **SSH Support**: Option to use SSH URLs instead of HTTPS
- **Private Repository Access**: Auto-detects glab CLI auth, supports GITLAB_TOKEN env var and --token flag
- **Delete Mode**: Safely deletes cloned repositories with uncommitted changes protection
- **Smart Branch Management**:
  - `--pull-from-default`: Merges default branch into current branch when behind
  - `--switch-to-default`: Switches all repos to default branch
- **Real-time Status Display**: Live in-place updates with color-coded progress bar
- **Error Tracking**: Numbered error tracking with detailed error list at completion
- **Custom GitLab URL**: Supports self-hosted GitLab instances via `--gitlab-url`

## Quick Start

### Installation

```bash
# Install from crates.io
cargo install glab-pull-all

# Or build from source
git clone https://github.com/link-foundation/glab-pull-all.git
cd glab-pull-all
cargo install --path .
```

### Usage

```bash
# Sync all repositories from a GitLab group
glab-pull-all --group my-group

# Sync all repositories from a GitLab user
glab-pull-all --user my-username

# Use SSH for cloning into a specific directory
glab-pull-all --group my-group --ssh --dir ./repos

# Use 16 concurrent operations
glab-pull-all --user my-username --threads 16

# Run operations sequentially
glab-pull-all --user my-username --single-thread

# Disable live updates for CI/terminal history
glab-pull-all --group my-group --no-live-updates

# Delete all cloned repositories (with confirmation)
glab-pull-all --user my-username --delete

# Pull from default branch into current branch when behind
glab-pull-all --group my-group --pull-from-default

# Switch all repositories to their default branch
glab-pull-all --group my-group --switch-to-default

# Use a self-hosted GitLab instance
glab-pull-all --group my-group --gitlab-url https://gitlab.example.com
```

## CLI Options

| Option | Short | Description | Default |
|---|---|---|---|
| `--group <name>` | `-g` | GitLab group (organization) name | - |
| `--user <name>` | `-u` | GitLab username | - |
| `--token <token>` | `-t` | GitLab personal access token | `GITLAB_TOKEN` env |
| `--ssh` | `-s` | Use SSH URLs for cloning | `false` |
| `--dir <path>` | `-d` | Target directory for repositories | `.` (current) |
| `--threads <n>` | `-j` | Number of concurrent operations | `8` |
| `--single-thread` | | Run operations sequentially | `false` |
| `--no-live-updates` | | Disable live in-place status updates | `false` |
| `--delete` | | Delete all cloned repositories | `false` |
| `--pull-from-default` | | Pull default branch into current branch | `false` |
| `--switch-to-default` | | Switch repos to default branch | `false` |
| `--gitlab-url <url>` | | GitLab instance URL | `https://gitlab.com` |
| `--help` | `-h` | Show help | - |
| `--version` | `-V` | Show version | - |

**Validation Rules:**
- Must specify either `--group` or `--user` (not both)
- Cannot specify both `--single-thread` and `--threads`
- Cannot specify both `--pull-from-default` and `--switch-to-default`
- Thread count must be >= 1

## Authentication

Authentication is resolved in the following priority order:

1. **glab CLI**: If `glab` is installed and authenticated, the token is used automatically
2. **Environment variable**: `GITLAB_TOKEN` environment variable
3. **CLI flag**: `--token <token>` argument
4. **Public access**: Works without authentication for public repositories

## Status Icons

| Icon | Status | Description |
|---|---|---|
| ⏳ | pending | Repository queued for processing |
| 📦 | cloning | Currently being cloned |
| 📥 | pulling | Currently being pulled |
| 🔍 | checking | Checking repository status |
| 🗑️  | deleting | Being deleted |
| ✅ | success | Operation completed successfully |
| ❌ | failed | Operation failed |
| ⚠️  | skipped | Skipped (private without token, not a git repo) |
| 🔄 | uncommitted | Has uncommitted changes (auto-skipped) |

## Progress Bar

The progress bar shows real-time operation status:

```
[████████████░░░░░░░░] 12/20 (60%)
```

Color coding:
- 🟩 Green = success
- 🟥 Red = failed
- 🟨 Yellow = skipped
- 🟦 Cyan = in progress
- ⬜ Gray = pending

## Requirements

- **Rust**: 1.70+ (for building from source)
- **Git**: Installed and configured
- **glab CLI** (optional): For enhanced authentication and private repo access
- **SSH keys** (optional): For SSH cloning

## Development

```bash
# Build
cargo build

# Run tests
cargo test

# Run with verbose output
cargo test --verbose

# Format code
cargo fmt

# Run clippy lints
cargo clippy --all-targets --all-features

# Run the example
cargo run --example basic_usage
```

## Project Structure

```
.
├── .github/workflows/     # CI/CD pipeline
├── changelog.d/           # Changelog fragments
├── examples/              # Usage examples
├── scripts/               # Build/release scripts
├── src/
│   ├── cli.rs             # CLI argument parsing (clap)
│   ├── display.rs         # Status display, progress bar, colors
│   ├── git_ops.rs         # Git operations (clone, pull, delete, etc.)
│   ├── gitlab.rs          # GitLab API and glab CLI integration
│   ├── lib.rs             # Library entry point
│   ├── main.rs            # Binary entry point
│   └── runner.rs          # Parallel/sequential orchestration
├── tests/
│   └── integration_test.rs
├── Cargo.toml
├── CHANGELOG.md
├── CONTRIBUTING.md
├── LICENSE
└── README.md
```

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

[Unlicense](LICENSE) - Public Domain

## Acknowledgments

Rust port of [gh-pull-all](https://github.com/link-foundation/gh-pull-all) by [Link Foundation](https://github.com/link-foundation).
