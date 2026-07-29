# Security Policy

## Supported versions

`agent-workspace-tools` is pre-release. Until v1.0.0 is tagged, only `main` is supported. After
the tag, security fixes land on `main` and are released as a patch version.

## Reporting a vulnerability

Please report privately, not as a public issue.

Use GitHub's private vulnerability reporting on this repository: **Security** tab -> **Report a
vulnerability**. That opens a private advisory visible only to the maintainers.

Please include the `awt --version` output, your OS version, the command you ran (with paths
redacted as needed), and what you expected versus what happened. If the report involves a store
file with an unexpected shape, a minimized and sanitized excerpt is far more useful than a
description - see the sanitization guidance in `test/fixtures/README.md`.

Expect an initial response within a week. There is no bounty program.

## What is in scope

This tool reads and rewrites files under a user's Claude home (`~/.claude/` and `~/.claude.json`),
which contain conversation transcripts, prompt history, per-project configuration, and plugin
state. That data is frequently sensitive. Reports of the following are in scope:

- Any path that writes outside the intended Claude home or the destination project, including
  path-traversal through a crafted `cwd`, project key, or transcript filename.
- Any path that destroys or corrupts state without a recoverable backup, or where `rollback`
  cannot restore byte-identical content.
- Any path that causes the tool to make a network request or invoke an LLM. The tool guarantees
  neither happens; a counterexample is a security bug, not just a design bug.
- Symlink, junction, or hardlink handling that lets a move or archive escape its intended tree.
- Leakage of transcript content into a location the user did not choose, including the archive
  directory, backup snapshots, HTML output, or `--json` output.
- Privilege or ACL mishandling that widens access to copied data. Backups and archives inherit
  the destination's permissions; a copy landing somewhere more permissive than the source is
  worth reporting.

## What is not a vulnerability

- **A guard refusing to run.** Exit 2 (guard), 3 (verify failed), and 4 (unrecognized format) are
  the tool working as designed. Refusing is the safe behavior, not a denial of service.
- **`doctor` reporting stale references it will not rewrite.** Report-only regions
  (`plugins/`, `file-history/`, `backups/`) are excluded deliberately; an old path there is a
  correct historical record.
- **Requiring a copy for first use.** Documented, intentional.
- **Anything requiring an attacker to already have write access to your Claude home.** At that
  point the transcripts are already readable and the threat model has been lost upstream of this
  tool.

## Security posture

**No network, structurally enforced.** `crates/awt-core/tests/no_network_deps.rs` fails the build
if any network-capable crate appears in `Cargo.lock`, and CI independently checks the dependency
tree of every workspace package. The migration path makes zero network and zero LLM calls.

**Minimal dependency surface.** `awt-core` may not depend on `tauri` or `clap`; CI enforces it.
`cargo audit` runs on every CI build and an unresolved RUSTSEC advisory fails the build.

**Backup before write, verify after.** Every write run snapshots the files it will touch with
sha256 first, verifies its postconditions by re-reading from disk, and auto-rolls-back on
failure. `rollback` proves byte-identity of every restored file.

**Distribution.** v1.0.0 ships source-first: install by cloning and running
`cargo install --path crates/awt-cli`. No pre-built binaries are distributed, so there is no
unsigned-binary supply-chain surface and nothing to spoof. A signed binary channel (checksums,
`minisign`, Authenticode, notarization) is tracked as CI-3 and is not part of v1.0.0. **If you
encounter a pre-built `awt` binary attributed to this project, it did not come from us.**
