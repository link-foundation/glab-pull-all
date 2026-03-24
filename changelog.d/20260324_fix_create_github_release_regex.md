---
bump: patch
---

### Fixed
- Fix `create-github-release.rs` regex panic: replace unsupported look-ahead `(?=...)` with non-capturing group `(?:...)` compatible with Rust's `regex` crate
