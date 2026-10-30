---
bump: patch
---

### Changed
- Publish through GitHub Actions trusted publishing, retaining registry token secrets as a fallback.

### Fixed
- Commit the refreshed Cargo.lock with release version bumps and enforce locked CI builds to prevent package checks failing on a dirty lockfile.
