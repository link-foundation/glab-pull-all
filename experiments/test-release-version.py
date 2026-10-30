#!/usr/bin/env python3
"""Exercise the release script in disposable repositories with local remotes.

Run: python3 experiments/test-release-version.py
No registry credentials or network access are used by the fixture crates.
"""
import os
import shlex
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest

REPOSITORY = Path(__file__).resolve().parents[1]
SCRIPT = REPOSITORY / "scripts/version-and-commit.rs"


def run(*args, cwd, env=None, check=True):
    return subprocess.run(args, cwd=cwd, env=env, check=check,
                          text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)


class ReleaseVersionTests(unittest.TestCase):
    def fixture(self, directory, subfolder):
        root = Path(directory) / "work"
        root.mkdir()
        remote = Path(directory) / "remote.git"
        run("git", "init", "--bare", str(remote), cwd=root)
        run("git", "init", "-b", "release-test", cwd=root)
        run("git", "config", "user.name", "Release Test", cwd=root)
        run("git", "config", "user.email", "test@example.invalid", cwd=root)
        run("git", "remote", "add", "origin", str(remote), cwd=root)
        package = root / "rust" if subfolder else root
        (package / "src").mkdir(parents=True)
        (package / "src/lib.rs").write_text("pub fn example() {}\n")
        (package / "Cargo.toml").write_text(
            '[package]\nname = "release-fixture"\nversion = "0.1.0"\n'
            'edition = "2021"\ndescription = "Release test"\nlicense = "Unlicense"\n')
        (package / "CHANGELOG.md").write_text("# Changelog\n")
        (root / ".gitignore").write_text("target/\n")
        run("cargo", "generate-lockfile", "--offline", cwd=package)
        run("git", "add", ".", cwd=root)
        run("git", "commit", "-m", "Initial fixture", cwd=root)
        run("git", "push", "-u", "origin", "release-test", cwd=root)
        environment = os.environ.copy()
        for key in ("RUST_ROOT", "GITHUB_OUTPUT", "BUMP_TYPE", "DESCRIPTION"):
            environment.pop(key, None)
        return root, package, environment

    def test_release_commits_matching_lockfile(self):
        for subfolder in (False, True):
            with self.subTest(subfolder=subfolder), tempfile.TemporaryDirectory() as directory:
                root, package, environment = self.fixture(directory, subfolder)
                result = run("rust-script", str(SCRIPT), "--bump-type", "patch",
                             cwd=root, env=environment)
                self.assertIn("version_committed=true", result.stdout)
                lock = tomllib.loads((package / "Cargo.lock").read_text())
                self.assertEqual(lock["package"][0]["version"], "0.1.1")
                self.assertEqual(run("git", "status", "--porcelain", cwd=root).stdout, "")
                run("cargo", "package", "--locked", "--list", cwd=package)
                relative = (package / "Cargo.lock").relative_to(root)
                tagged_lock = run("git", "show", f"v0.1.1:{relative}", cwd=root).stdout
                self.assertEqual(tomllib.loads(tagged_lock)["package"][0]["version"], "0.1.1")
                self.assertEqual(run("git", "rev-parse", "HEAD", cwd=root).stdout,
                                 run("git", "rev-parse", "origin/release-test", cwd=root).stdout)

    def test_lockfile_failure_prevents_commit_tag_and_push(self):
        with tempfile.TemporaryDirectory() as directory:
            root, _, environment = self.fixture(directory, False)
            original = run("git", "rev-parse", "HEAD", cwd=root).stdout
            tools = Path(directory) / "bin"
            tools.mkdir()
            cargo = tools / "cargo"
            cargo.write_text('#!/bin/sh\necho "simulated resolution failure" >&2\nexit 23\n')
            cargo.chmod(0o755)
            # The wrapper injects the failing Cargo after rust-script has compiled.
            wrapper = Path(directory) / "wrapper"
            wrapper.write_text('#!/bin/sh\nexport PATH=' + shlex.quote(str(tools))
                               + ':"$PATH"\nexec "$@"\n')
            wrapper.chmod(0o755)
            result = run("rust-script", "--wrapper", str(wrapper), str(SCRIPT),
                         "--bump-type", "patch", cwd=root, env=environment, check=False)
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn("simulated resolution failure", result.stderr)
            self.assertEqual(run("git", "rev-parse", "HEAD", cwd=root).stdout, original)
            self.assertEqual(run("git", "tag", cwd=root).stdout, "")
            self.assertEqual(run("git", "rev-parse", "origin/release-test", cwd=root).stdout, original)


if __name__ == "__main__":
    unittest.main(verbosity=2)
