# Case Study: Issue #3 - Fix CI/CD Pipeline

## Summary

The CI/CD pipeline on `main` branch was failing during the Auto Release job. The `create-github-release.rs` script panicked at runtime due to an unsupported regex feature (look-ahead) in Rust's `regex` crate.

## Timeline of Events

| Timestamp (UTC) | Run ID | Branch | Event | Result |
|-----------------|--------|--------|-------|--------|
| 2026-03-24T07:40:11Z | 23478411987 | main | push | FAILURE (`cargo package --list` - Cargo.lock uncommitted, old workflow) |
| 2026-03-24T07:43:44Z | 23478527123 | issue-1-bbfff7e97f20 | PR | SUCCESS |
| 2026-03-24T08:00:48Z | 23479084066 | issue-1-bbfff7e97f20 | PR | SUCCESS |
| 2026-03-24T08:06:45Z | 23479289779 | issue-1-bbfff7e97f20 | PR | SUCCESS (merged as PR #2) |
| 2026-03-24T08:17:53Z | 23479684794 | main | push (PR #2 merge) | **FAILURE** (regex look-ahead panic) |

## Root Cause Analysis

### Primary Root Cause: Unsupported Regex Look-Ahead in Rust

**File:** `scripts/create-github-release.rs`, line 47

**Broken pattern:**
```rust
let pattern = format!(r"(?s)## \[{}\].*?\n(.*?)(?=\n## \[|$)", escaped_version);
```

The `(?=\n## \[|$)` part is a **positive look-ahead assertion**. Rust's `regex` crate explicitly does not support look-ahead (or any look-around) because it guarantees linear-time matching. This caused a runtime panic:

```
thread 'main' panicked at scripts/create-github-release.rs:48:35:
called `Result::unwrap()` on an `Err` value: Syntax(
regex parse error:
    (?s)## \[0\.3\.0\].*?\n(.*?)(?=\n## \[|$)
                                 ^^^
error: look-around, including look-ahead and look-behind, is not supported
)
```

### Impact

- The Auto Release job completed publishing to crates.io (v0.3.0 was published successfully)
- The GitHub Release creation step failed, leaving no GitHub Release for v0.3.0
- The release was partially completed: crates.io had the package but GitHub had no release

### Secondary Issue (Run 23478411987, older commit)

A separate failure on an earlier commit was caused by `cargo package --list` detecting uncommitted `Cargo.lock` changes. This was from the pre-rewrite state (package named `my-package v0.2.0`) and was resolved by the PR #2 merge that rewrote the project.

## Fix Applied

**Changed pattern to use non-capturing group instead of look-ahead:**

```rust
// Before (broken):
let pattern = format!(r"(?s)## \[{}\].*?\n(.*?)(?=\n## \[|$)", escaped_version);

// After (fixed):
let pattern = format!(r"(?s)## \[{}\][^\n]*\n(.*?)(?:\n## \[|$)", escaped_version);
```

**Two changes:**
1. `(?=\n## \[|$)` (look-ahead) replaced with `(?:\n## \[|$)` (non-capturing group) - the look-ahead was unnecessary since the `.*?` non-greedy quantifier already stops at the first match
2. `.*?` after the version header replaced with `[^\n]*` - more precise, since the header is always a single line

### Reference

This fix was already applied in the reference project [`linksplatform/Numbers`](https://github.com/linksplatform/Numbers) in their `scripts/create-github-release.rs`, which uses the same CI/CD pipeline template.

## Lessons Learned

1. **Rust's `regex` crate does not support look-around assertions.** When porting regex patterns from other languages (JavaScript, Python, PCRE), look-ahead/look-behind must be rewritten using alternative patterns.

2. **Non-greedy quantifiers (`.*?`) often make look-ahead unnecessary.** The `(?s).*?(?:\n## \[|$)` pattern naturally stops at the first occurrence of `\n## [` because `.*?` is non-greedy.

3. **Partial releases are dangerous.** The pipeline published to crates.io before the GitHub Release step, meaning a script failure left the release in an inconsistent state. Consider validating all steps (including regex compilation) before starting irreversible operations like publishing.

## Verification

The fix was verified by:
1. Creating a test script (`experiments/test-regex-fix.rs`) that validates the new pattern extracts changelog sections correctly
2. Confirming the old pattern fails to compile (as expected)
3. Running all existing tests (29 passed, 0 failed)
4. Passing `cargo fmt` and `cargo clippy` checks
