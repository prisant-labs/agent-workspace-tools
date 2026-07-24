# awt v1.0 Release Runbook

Tag ceremony checklist for v1.0.0. Run this in order. Every gate must pass before the tag is pushed.

---

## 1. Pre-tag gates: CI (automated)

These run on every push via the CI-0 workflow. Confirm they are green on the commit you intend to tag.

- [ ] `cargo fmt --check` - no unformatted files
- [ ] `cargo clippy -- -D warnings` - no warnings
- [ ] `cargo test --workspace` - all tests pass
- [ ] Dependency-hygiene gate: `awt-core` has no dependency on `tauri`, `clap`, `reqwest`, `ureq`, `hyper`, or `curl`
- [ ] No-network gate: none of the above network crates appear anywhere in the tree
- [ ] `cargo audit` - no unresolved vulnerability advisories

---

## 2. Pre-tag gates: manual acceptance test

This test must be run on the real machine against a COPY of `~/.claude`. Never run acceptance tests against the live installation.

**Prepare:**

1. Copy `~/.claude` and `~/.claude.json` to a scratch location (e.g., `C:\Temp\claude-acceptance-copy\`).
2. Set `--home` to the scratch location for all commands below. No command in this test should touch the live `~/.claude`.

**Acceptance sequence:**

- [ ] `awt doctor --home <scratch>` completes without error and reports the expected stale-reference categories (stale `githubRepoPaths`, stale `history.jsonl` values, orphaned plugin dir). Compare counts to the last known-good baseline in the session log.
- [ ] `awt scan --home <scratch> --src <a project path you know has state>` returns hits for that project.
- [ ] `awt list --home <scratch>` prints a table with at least one row; no panic or I/O error.
- [ ] `awt plan --home <scratch> --src <test-src> --dst <test-dst>` prints a change list; exit 0. (`--src` and `--dst` do not need to exist on disk for plan to run.)
- [ ] `awt apply --home <scratch> --src <test-src> --dst <test-dst>` applies the move in the scratch copy; reports applied count and backup path; exit 0.
- [ ] `awt verify --home <scratch> --src <test-src> --dst <test-dst>` reports all checks `[ok]`; exit 0.
- [ ] `awt rollback --home <scratch> --report <backup-manifest-from-apply>` restores the scratch copy; exit 0. Confirm the scratch copy is back to its pre-apply state by re-running `awt verify` (expect it to fail, confirming rollback succeeded).
- [ ] `awt archive --home <scratch> --archive-dir <scratch-archive>` completes; exit 0.
- [ ] `awt associate --home <scratch> --from <old-path> --to <new-path>` completes; exit 0.

**Clean up:** delete the scratch copy and scratch archive when done.

---

## 3. Tag steps

Once all pre-tag gates are green:

1. Confirm the working tree is clean: `git status` shows nothing uncommitted.
2. Confirm you are on `main` and it is up to date with the remote.
3. Create the annotated tag:
   ```
   git tag -a v1.0.0 -m "v1.0.0"
   ```
4. Push the tag:
   ```
   git push origin v1.0.0
   ```
5. Verify the tag appears on the remote: `git ls-remote --tags origin`.
6. Update `docs/ROADMAP.md` Section 7 (Current status) to record that v1.0.0 was tagged and the date.
7. Add a `docs/CHANGELOG.md` entry for the release.

---

## 4. Signing posture

### v1.0.0: source-first distribution

v1.0.0 ships via source distribution only. Users install by cloning and running:

```
cargo build --release
```

or:

```
cargo install --path crates/awt-cli
```

Source distribution requires no binary signing, no certificate, and no submission to an OS package manager. There is no SmartScreen or Gatekeeper surface because no pre-built binary is distributed.

### Future: binary channel (CI-3)

A signed binary channel is a planned future step, tracked as CI-3 in `docs/ROADMAP.md`. CI-3 is not a v1.0.0 blocker. When it is scheduled, the following work is required:

- Release workflow: matrix build for each target platform.
- Checksum file: `sha256sums.txt` signed with `minisign`.
- Windows SmartScreen posture: either an Extended Validation (EV) code-signing certificate, or a documented reputation-building plan (SmartScreen clears after sufficient download volume for Authenticode-signed builds).
- winget manifest: `winget-pkgs` pull request with the manifest for the new version.
- macOS Gatekeeper posture: notarization via `xcrun notarytool` plus `staple`; requires an Apple Developer ID certificate.
- Release runbook update: add CI-3 steps to this file before the first binary release.

Until CI-3 gates pass, do not distribute pre-built binaries. Distributing an unsigned binary on Windows produces a SmartScreen block for every user; distributing an unsigned binary on macOS produces a Gatekeeper quarantine. Source-first avoids both.

---

## 5. Post-tag checklist

- [ ] Tag visible on remote (`git ls-remote --tags origin` shows `v1.0.0`).
- [ ] `docs/ROADMAP.md` Section 7 updated with tag date.
- [ ] `docs/CHANGELOG.md` entry added.
- [ ] Scratch copy from acceptance test deleted.
- [ ] Any open issues or PRs that are closed by v1.0.0 updated accordingly.
