# crates.io trusted publishing

The caller workflow is `.github/workflows/release.yml` in
`link-foundation/glab-pull-all`. Both `auto-release` and `manual-release` request
`id-token: write` and use the official, commit-pinned
[rust-lang/crates-io-auth-action](https://github.com/rust-lang/crates-io-auth-action).
Authentication runs after the release build and immediately before publishing so
that the short-lived credential is available for the upload. The action revokes
its temporary token in its post step.

## Registry registration

A crates.io owner of `glab-pull-all` must configure the GitHub trusted publisher
in the crate's settings, following the
[crates.io instructions](https://crates.io/docs/trusted-publishing):

| Field | Value |
| --- | --- |
| GitHub owner | `link-foundation` |
| Repository | `glab-pull-all` |
| Workflow filename | `release.yml` |
| Environment | Leave empty; neither release job uses an environment |

The original PR mentions re-running `setup --migrate`. That setup tool is not
part of this repository or its dependencies. If using the external migration
system that created the PR, run its setup against this caller workflow; otherwise
register the same values in crates.io directly. GitHub repository access alone
does not provide a crates.io owner session.

## Triggers and validation

Pull requests run checks and build the package; neither release job runs on a
pull request. Pushes to `main` can run Auto Release, which checks crates.io and
uses changelog fragments to choose the next version. The patch fragment included
with this migration is the release trigger; do not manually bump `Cargo.toml` in
the PR. Instant Release runs for `workflow_dispatch` with `release_mode=instant`
after lint, tests and build succeed. The changelog PR mode does not publish.

Version preparation updates and commits both `Cargo.toml` and `Cargo.lock`
before creating a release tag. CI builds and tests use `--locked` to catch drift.
The regression test `python3 experiments/test-release-version.py` exercises real
version commits, tags and pushes against disposable local remotes, including
single-language and `rust/` layouts and resolution failure before a release.

After registration and merge, inspect the next release run:

1. The trusted publishing authentication step must succeed and return a token.
2. The publish step must report `publish_result=success` for a new version, and
   the version must be visible on crates.io. `already_exists` does not verify a
   new OIDC upload.
3. The action's post step must revoke the temporary token.
4. Record the run URL and published version in PR #9 as verification evidence.

Authentication uses `continue-on-error: true`. Publishing selects the OIDC token
first, then `secrets.CARGO_REGISTRY_TOKEN`, then `secrets.CARGO_TOKEN`. A successful
release with a failed authentication step proves only the fallback path. Keep
legacy secrets as requested by PR #9; this change does not delete or rotate them.

## Investigation evidence for PR #9

Both failed runs were after their respective commits:

| Run | Created (UTC) | Head commit | Result |
| --- | --- | --- | --- |
| [38417127813](https://github.com/link-foundation/glab-pull-all/actions/runs/38417127813) | 2026-10-13 14:52:30 | `4b918cb` | Package check failed |
| [38428758445](https://github.com/link-foundation/glab-pull-all/actions/runs/38428758445) | 2026-10-13 16:13:44 | `a89f6b1` | Package check failed |

The full logs were downloaded to `ci-logs/pipeline-{run-id}.log` during the
investigation (local logs are ignored by git). In both logs, lines 5255–5261 show
that `cargo package --list` rejected the modified `Cargo.lock`. Release builds
and all three platform tests passed. `Cargo.toml` was at `0.5.0` but the root
package in `Cargo.lock` was still at `0.4.0`: the previous release script committed
only the manifest and changelog. `cargo metadata --locked --format-version 1`
reproduced the stale lockfile error locally before the fix.

Registry registration and a real OIDC upload cannot be claimed from local tests
or PR CI. At investigation time no authenticated crates.io owner session or
external migration executable was available. The crate settings page returned
"This page requires authentication", and no Cargo registry credentials were
present in the environment or Cargo credentials files. These criteria remain
unverified until the registry setup and release evidence above are available.
