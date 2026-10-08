---
bump: patch
---

### Fixed
- A directory without `.git` inside `--dir` is never treated as a clone, even when `--dir` itself is inside a git work tree: syncing no longer pulls the enclosing repository, and `--delete` no longer removes the directory with everything inside it
- Project paths that would resolve outside `--dir` (absolute paths or `..` components) are rejected
