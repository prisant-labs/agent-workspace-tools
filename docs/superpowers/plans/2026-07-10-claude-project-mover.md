# Claude Project Mover Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a deterministic, offline CLI (`cpm`) that relocates a project folder and migrates all Claude Code state keyed to its old absolute path, with dry-run, backup, verify, and rollback.

**Architecture:** A Tauri-free `cpm-core` crate holds the whole engine (path encoding, a reverse project index, per-store adapters, plan/apply/verify/backup/rollback) behind an injectable `FileSystem` trait so every test runs against an in-memory filesystem, never live `~/.claude`. A thin `cpm-cli` crate wraps it with clap. A GUI is deferred and, when built, calls the identical core. Design of record: `docs/DESIGN.md`.

**Tech Stack:** Rust (stable, 2021 edition), `clap` (CLI), `serde_json` (validate-only, never re-serialize), `sha2`, `dunce`, `walkdir`, `tempfile`, `insta` (golden snapshots). No async, no tokio, no sqlx, no git2, no network.

## Global Constraints

- Edition `2021`, Rust toolchain `stable` with `clippy` + `rustfmt`.
- `cpm-core` must NOT depend on `tauri`, any `tauri-*` crate, or `clap`, even transitively. Enforced by a CI gate: `cargo tree -p cpm-core | grep -iE 'tauri|clap'` must find nothing.
- Zero LLM calls and zero outbound network in any code path.
- Never re-serialize a store file. Parse only to validate shape (`serde_json::from_str`), discard the parsed value, and write by literal byte splice on the original text.
- Every path replacement is boundary-anchored (exact `"cwd":"<path>"` field, or a path prefix immediately followed by a separator) and count-checked. Never a bare-substring replace.
- Path-dir encoding rule: `encode_project_dir(abs)` maps every character that is not ASCII `[A-Za-z0-9]` to `-`. Forward-only; never used to look up an existing directory.
- Store files store Windows paths JSON-escaped: a single `\` on disk in a path becomes `\\` in the JSON text. Match the `\\` form.
- Store files must be valid UTF-8. Invalid UTF-8 anywhere in a store file is an unrecognized format (exit 4, nothing written); the write path never lossily converts bytes it is about to splice and rewrite.
- Never rewrite a transcript that belongs to a different project, even if it mentions the old path. Only the moved project's own path-keyed state is rewritten.
- No em-dashes (U+2014) or en-dashes (U+2013) anywhere, including code comments and commit messages. Use " - " or restructure.
- Commit messages end with the two trailers used in this repo (`Co-Authored-By:` and `Claude-Session:`), matching `git log`.

## Reference data (used by fixtures across many tasks)

The 2026-07-09 manual move is the golden reference. Backup lives at `E:\tmp\claude-move-backup-20260709-090053\`.

- **Reference move:** old project path `E:\Projects\Github Repos\markdown-for-humans` (the value stored inside the transcripts), new path `E:\Projects\prisant-labs\vs-code-markdown-max`.
- **Old encoded dir:** `E--Projects-Github-Repos-markdown-for-humans`. **New encoded dir:** `E--Projects-prisant-labs-vs-code-markdown-max`.
- **Two transcripts:** `22b2362e-e4ef-4042-9b01-e3cba5719590.jsonl` (329 lines) and `28fd093e-f5ef-4dc7-af16-ea415c1840f7.jsonl` (2285 lines).
- **Expected anchored-rewrite counts** (file 22b2362e + file 28fd093e):
  - exact `"cwd":"..."` field: 227 + 1240
  - backslash prefix (`...markdown-for-humans\\`): 54 + 534
  - forward prefix (`...markdown-for-humans/`): 0 + 27
- **Must stay byte-identical:** `markdown-for-humans@` (10 total: 2 + 8 per file), `markdown-for-humans_dev-` (55 total: 6 + 49 per file). Earlier notes recorded the larger file's per-file counts (8/49) as totals; corrected 2026-07-11 against the captured fixtures (see `test/fixtures/README.md`).
- On disk, the `cwd` field's raw bytes are exactly `"cwd":"E:\\Projects\\Github Repos\\markdown-for-humans"` (each `\\` is two backslash characters).

---

## File Structure

```
claude-project-mover/
  Cargo.toml                      virtual workspace manifest, [profile.*]
  rust-toolchain.toml             pin stable + clippy + rustfmt
  .github/workflows/ci.yml        fmt, clippy -D warnings, test, dep-hygiene gate
  crates/
    cpm-core/
      Cargo.toml                  serde_json, sha2, dunce, walkdir, tempfile; dev: insta
      src/
        lib.rs                    re-exports; crate root
        fs.rs                     FileSystem trait, RealFileSystem, MemoryFileSystem
        paths.rs                  encode_project_dir, normalize_path, same_volume
        model.rs                  Move, Ctx, Hit, Stale, Change, Applied, VerifyResult, Report
        error.rs                  CpmError enum + Result alias
        index.rs                  ProjectIndex, read_stored_cwd
        rewrite.rs                RewriteRule, anchored_rewrite, build_path_rules
        stores/
          mod.rs                  Store trait, registry(), all-stores helper
          claude_projects.rs      projects/<enc> dir + *.jsonl transcripts
          claude_json.rs          ~/.claude.json projects{} keys + githubRepoPaths{}
          claude_history.rs       ~/.claude/history.jsonl project field
          plugin_state.rs         plugins/data/*/state/<base>-<sha256[:16]>
          sweep.rs                report-only scan of non-adapter regions
        plan.rs                   build_plan, render_plan
        backup.rs                 snapshot, Manifest
        apply.rs                  apply
        verify.rs                 verify
        rollback.rs               rollback
        doctor.rs                 doctor (machine-wide staleness), scan (one project)
      tests/
        fixtures.rs               loads test/fixtures via MemoryFileSystem seed helper
    cpm-cli/
      Cargo.toml                  clap, cpm-core, serde_json
      src/
        main.rs                   clap parser, subcommand dispatch
        exit.rs                   ExitCode mapping from CpmError
  test/fixtures/
    reference-move/
      before/                     2 sanitized transcripts from the backup
      after/                      2 sanitized migrated transcripts + minimized synthetic claude.json
      move.json                   { src_abs, dst_abs, expected counts }
      README.md                   provenance + sanitization record (never refresh from live files)
    claude-json-variants/         minimized synthetic claude.json: 3 key-variant groups + 6 stale githubRepoPaths
    plugin-state/                 markdown-for-humans-e854827f52137cd9/state.json (minimized synthetic)
```

Each store file owns exactly one fragile format so a format change is localized to one adapter with its own golden test.

---

## Phase 1: Scaffold, FileSystem trait, fixtures, CI gate

### Task 1.1: Workspace scaffold and CI dependency-hygiene gate

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `crates/cpm-core/Cargo.toml`, `crates/cpm-core/src/lib.rs`, `.github/workflows/ci.yml`

**Interfaces:**
- Produces: a compiling workspace with member `crates/cpm-core`, and a CI job that fails if `cpm-core` gains a `tauri`/`clap` dependency.

- [ ] **Step 1: Create the workspace manifest**

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["crates/cpm-core"]

[workspace.package]
edition = "2021"
version = "0.1.0"
license = "MIT"

[workspace.dependencies]
serde_json = "1"
sha2 = "0.10"
dunce = "1"
walkdir = "2"
tempfile = "3"
insta = "1"

[profile.release]
opt-level = "z"
lto = "thin"
strip = true
codegen-units = 16
```

- [ ] **Step 2: Pin the toolchain**

`rust-toolchain.toml`:
```toml
[toolchain]
channel = "stable"
components = ["clippy", "rustfmt"]
```

- [ ] **Step 3: Create the core crate manifest and root**

`crates/cpm-core/Cargo.toml`:
```toml
[package]
name = "cpm-core"
edition.workspace = true
version.workspace = true
license.workspace = true

[dependencies]
serde_json.workspace = true
sha2.workspace = true
dunce.workspace = true
walkdir.workspace = true
tempfile.workspace = true

[dev-dependencies]
insta.workspace = true
```

`crates/cpm-core/src/lib.rs`:
```rust
pub mod fs;
pub mod paths;
```

- [ ] **Step 4: Add the CI workflow with the dependency-hygiene gate**

`.github/workflows/ci.yml`:
```yaml
name: ci
on: [push, pull_request]
jobs:
  build:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: clippy, rustfmt }
      - run: cargo fmt --all -- --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo test --workspace
      - name: dependency-hygiene gate
        shell: bash
        run: |
          if cargo tree -p cpm-core | grep -iE 'tauri|clap'; then
            echo "cpm-core must not depend on tauri or clap"; exit 1
          fi
      - name: no-network gate
        shell: bash
        run: |
          if cargo tree -p cpm-core | grep -iE 'reqwest|ureq|hyper|curl'; then
            echo "cpm-core must not depend on any network-capable crate"; exit 1
          fi
      - name: advisory audit (RUSTSEC)
        run: cargo install cargo-audit --locked && cargo audit
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo build --workspace`
Expected: compiles (fs/paths modules are created in the next tasks; if `cargo build` complains about missing modules, create empty `crates/cpm-core/src/fs.rs` and `paths.rs` with `// placeholder` and re-run - they are filled in Tasks 1.2 and 2.1).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml rust-toolchain.toml crates/cpm-core .github
git commit -m "feat: scaffold cpm-core workspace with CI dependency gate"
```

### Task 1.2: FileSystem trait with real and in-memory implementations

**Files:**
- Create: `crates/cpm-core/src/fs.rs`
- Test: inline `#[cfg(test)]` in `fs.rs`

**Interfaces:**
- Produces:
  ```rust
  pub trait FileSystem {
      fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
      fn write(&self, path: &Path, data: &[u8]) -> io::Result<()>;
      fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
      fn exists(&self, path: &Path) -> bool;
      fn is_file(&self, path: &Path) -> bool;
      fn is_dir(&self, path: &Path) -> bool;
      fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;   // immediate children, full paths
      fn create_dir_all(&self, path: &Path) -> io::Result<()>;
      fn copy(&self, from: &Path, to: &Path) -> io::Result<()>;      // single file
      fn remove_dir_all(&self, path: &Path) -> io::Result<()>;
  }
  pub struct RealFileSystem;
  pub struct MemoryFileSystem { /* interior-mutable map */ }
  ```
  Every later task consumes `&dyn FileSystem`.

- [ ] **Step 1: Write the failing test**

In `crates/cpm-core/src/fs.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn memory_fs_round_trip_and_rename() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("/a/b.txt"), b"hello").unwrap();
        assert!(fs.exists(Path::new("/a/b.txt")));
        assert!(fs.is_file(Path::new("/a/b.txt")));
        assert_eq!(fs.read(Path::new("/a/b.txt")).unwrap(), b"hello");
        fs.rename(Path::new("/a/b.txt"), Path::new("/a/c.txt")).unwrap();
        assert!(!fs.exists(Path::new("/a/b.txt")));
        assert_eq!(fs.read(Path::new("/a/c.txt")).unwrap(), b"hello");
        let kids = fs.read_dir(Path::new("/a")).unwrap();
        assert_eq!(kids, vec![std::path::PathBuf::from("/a/c.txt")]);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p cpm-core fs::tests`
Expected: FAIL to compile (`MemoryFileSystem` not defined).

- [ ] **Step 3: Write the trait and both implementations**

Top of `crates/cpm-core/src/fs.rs`:
```rust
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub trait FileSystem {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn exists(&self, path: &Path) -> bool;
    fn is_file(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    fn copy(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;
}

fn norm(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

pub struct RealFileSystem;

impl FileSystem for RealFileSystem {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> { std::fs::read(path) }
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(path, data)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> { std::fs::rename(from, to) }
    fn exists(&self, path: &Path) -> bool { path.exists() }
    fn is_file(&self, path: &Path) -> bool { path.is_file() }
    fn is_dir(&self, path: &Path) -> bool { path.is_dir() }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(path)? { out.push(entry?.path()); }
        out.sort();
        Ok(out)
    }
    fn create_dir_all(&self, path: &Path) -> io::Result<()> { std::fs::create_dir_all(path) }
    fn copy(&self, from: &Path, to: &Path) -> io::Result<()> {
        if let Some(parent) = to.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::copy(from, to).map(|_| ())
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> { std::fs::remove_dir_all(path) }
}

#[derive(Default)]
pub struct MemoryFileSystem {
    files: Mutex<BTreeMap<String, Vec<u8>>>,
}

impl MemoryFileSystem {
    pub fn new() -> Self { Self { files: Mutex::new(BTreeMap::new()) } }
}

impl FileSystem for MemoryFileSystem {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.files.lock().unwrap().get(&norm(path)).cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, norm(path)))
    }
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        self.files.lock().unwrap().insert(norm(path), data.to_vec());
        Ok(())
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let mut f = self.files.lock().unwrap();
        let (fp, tp) = (norm(from), norm(to));
        let moved: Vec<String> = f.keys()
            .filter(|k| **k == fp || k.starts_with(&format!("{fp}/")))
            .cloned().collect();
        if moved.is_empty() {
            return Err(io::Error::new(io::ErrorKind::NotFound, fp));
        }
        for k in moved {
            let data = f.remove(&k).unwrap();
            let nk = format!("{tp}{}", &k[fp.len()..]);
            f.insert(nk, data);
        }
        Ok(())
    }
    fn exists(&self, path: &Path) -> bool {
        let p = norm(path);
        let f = self.files.lock().unwrap();
        f.contains_key(&p) || f.keys().any(|k| k.starts_with(&format!("{p}/")))
    }
    fn is_file(&self, path: &Path) -> bool {
        self.files.lock().unwrap().contains_key(&norm(path))
    }
    fn is_dir(&self, path: &Path) -> bool {
        let p = norm(path);
        let f = self.files.lock().unwrap();
        !f.contains_key(&p) && f.keys().any(|k| k.starts_with(&format!("{p}/")))
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let prefix = format!("{}/", norm(path));
        let f = self.files.lock().unwrap();
        let mut kids = std::collections::BTreeSet::new();
        for k in f.keys() {
            if let Some(rest) = k.strip_prefix(&prefix) {
                let first = rest.split('/').next().unwrap();
                kids.insert(format!("{prefix}{first}"));
            }
        }
        Ok(kids.into_iter().map(PathBuf::from).collect())
    }
    fn create_dir_all(&self, _path: &Path) -> io::Result<()> { Ok(()) }
    fn copy(&self, from: &Path, to: &Path) -> io::Result<()> {
        let data = self.read(from)?;
        self.write(to, &data)
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        let p = norm(path);
        let mut f = self.files.lock().unwrap();
        f.retain(|k, _| *k != p && !k.starts_with(&format!("{p}/")));
        Ok(())
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p cpm-core fs::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/cpm-core/src/fs.rs
git commit -m "feat: FileSystem trait with real and in-memory implementations"
```

### Task 1.3: Capture golden fixtures from the real backup

**Files:**
- Create: `test/fixtures/reference-move/before/*`, `test/fixtures/reference-move/after/*`, `test/fixtures/reference-move/move.json`, `test/fixtures/reference-move/README.md`
- Create: `test/fixtures/claude-json-variants/claude.json`, `test/fixtures/plugin-state/markdown-for-humans-e854827f52137cd9/state.json`
- Create: `crates/cpm-core/tests/fixtures.rs`

**Interfaces:**
- Produces: `fn seed_memory_fs_from(dir: &Path) -> MemoryFileSystem` helper for tests, and on-disk fixture bytes the golden tests in later phases assert against.

- [ ] **Step 1: Copy the reference transcripts into `before/`**

Run (bash):
```bash
mkdir -p test/fixtures/reference-move/before/projects/E--Projects-Github-Repos-markdown-for-humans
cp "E:/tmp/claude-move-backup-20260709-090053/transcripts_markdown-for-humans/"*.jsonl \
   test/fixtures/reference-move/before/projects/E--Projects-Github-Repos-markdown-for-humans/
```
No `claude.json` is copied into `before/`: the authoritative `claude.json` fixture is the minimized synthetic file written in Step 4. Never commit the live `~/.claude.json` or its `.bak` (LEAD-09). These transcripts are sanitized in Step 6 before they are committed.

- [ ] **Step 2: Copy the migrated transcripts into `after/` and write a minimized `claude.json`**

Run (bash) - transcripts only (they are sanitized in Step 6):
```bash
mkdir -p test/fixtures/reference-move/after/projects/E--Projects-prisant-labs-vs-code-markdown-max
cp "C:/Users/jpris/.claude/projects/E--Projects-prisant-labs-vs-code-markdown-max/"*.jsonl \
   test/fixtures/reference-move/after/projects/E--Projects-prisant-labs-vs-code-markdown-max/
```
Then author `test/fixtures/reference-move/after/claude.json` as a minimized synthetic file representing post-move state - the NEW key present, the OLD key absent, nothing else. Do NOT copy the live `~/.claude.json`:
```json
{
  "projects": { "E:\\Projects\\prisant-labs\\vs-code-markdown-max": {} },
  "githubRepoPaths": {}
}
```

- [ ] **Step 3: Write the move descriptor**

`test/fixtures/reference-move/move.json`:
```json
{
  "src_abs": "E:\\Projects\\Github Repos\\markdown-for-humans",
  "dst_abs": "E:\\Projects\\prisant-labs\\vs-code-markdown-max",
  "old_encoded": "E--Projects-Github-Repos-markdown-for-humans",
  "new_encoded": "E--Projects-prisant-labs-vs-code-markdown-max",
  "expected_counts": {
    "cwd_field": 1467,
    "backslash_prefix": 588,
    "forward_prefix": 27,
    "preserved_package_at": 8,
    "preserved_branch_dev": 49
  }
}
```

- [ ] **Step 4: Write the two smaller fixtures as minimized synthetic files**

Do NOT copy the live `~/.claude.json` or the live plugin `state.json` (LEAD-09: both hold personal paths, per-project config, and MCP server entries). Author minimal synthetic files carrying only what the tests need.

```bash
mkdir -p test/fixtures/claude-json-variants test/fixtures/plugin-state/markdown-for-humans-e854827f52137cd9
```
`test/fixtures/claude-json-variants/claude.json` - ONLY the 3 real key-variant groups (values as empty objects) and the 6 stale `githubRepoPaths` entries; no `mcpServers`, no other keys:
```json
{
  "projects": {
    "E:\\Projects\\Github Repos\\markdown-for-humans": {},
    "e:/projects/github repos/markdown-for-humans": {},
    "D:\\Cloud-Work-PP": {}
  },
  "githubRepoPaths": {
    "owner/markdown-for-humans": ["E:\\Projects\\Github Repos\\markdown-for-humans"],
    "owner/chrome-bookmark-autosort": ["E:\\Projects\\Chrome - Bookmark Autosort"],
    "owner/pp": ["D:\\Cloud-Work-PP", "d:/cloud-work-pp"],
    "owner/pm-skills": ["E:\\Projects\\pm-skills"],
    "owner/agent-skills": ["E:\\Projects\\agent-skills-toolkit"],
    "owner/reposync": ["E:\\Projects\\product-on-purpose\\repo-sync-tool"]
  }
}
```
`test/fixtures/plugin-state/markdown-for-humans-e854827f52137cd9/state.json` - reduced to the minimal shape the adapter reads (the adapter keys only on the dir name, so the body just needs to be valid JSON):
```json
{ "schema": 1, "lastProject": "E:\\Projects\\Github Repos\\markdown-for-humans" }
```

- [ ] **Step 5: Write the seed helper and a sanity test**

`crates/cpm-core/tests/fixtures.rs`:
```rust
use cpm_core::fs::{FileSystem, MemoryFileSystem};
use std::path::{Path, PathBuf};

pub fn seed_memory_fs_from(dir: &Path) -> MemoryFileSystem {
    let fs = MemoryFileSystem::new();
    for entry in walkdir::WalkDir::new(dir).into_iter().filter_map(Result::ok) {
        if entry.file_type().is_file() {
            let rel = entry.path().strip_prefix(dir).unwrap();
            let virt = PathBuf::from("/home/.claude-fixture").join(rel);
            fs.write(&virt, &std::fs::read(entry.path()).unwrap()).unwrap();
        }
    }
    fs
}

#[test]
fn reference_before_seeds_two_transcripts() {
    let fs = seed_memory_fs_from(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
        .parent().unwrap().join("test/fixtures/reference-move/before").as_path());
    let dir = Path::new("/home/.claude-fixture/projects/E--Projects-Github-Repos-markdown-for-humans");
    assert_eq!(fs.read_dir(dir).unwrap().len(), 2);
}
```

Note: `walkdir` is a dev-usable dependency of `cpm-core`; the integration test can use it. If `env!` path math is awkward on Windows, hardcode the workspace-relative path `../../test/fixtures/...` resolved from `CARGO_MANIFEST_DIR`.

- [ ] **Step 6: Sanitize and minimize the captured fixtures (LEAD-09, before committing)**

The transcripts copied in Steps 1-2 are real session logs and may hold plaintext secrets (`.env` contents, tool output with credentials, pasted tokens - see `docs/reference/claude-data-model.md`). Sanitize before the bytes are permanent in git history.

  - [ ] **(a) Redact credentials in both transcript sets.** Grep both `before/` and `after/` transcripts (case-insensitive) for `api[_-]?key`, `token`, `secret`, `password`, `authorization`, `bearer`, `BEGIN [A-Z]+ PRIVATE KEY`, `ghp_`, `sk-`; then skim manually for anything the patterns miss. Redact each hit IN PLACE with a same-length placeholder (replace the secret run with the same number of `X` characters) chosen so it does NOT contain any of the project path strings the golden rules match. Then re-run the reference count assertions to prove redaction left the counted regions intact: exact `"cwd"` fields = 1467, backslash prefixes = 588, forward prefixes = 27, and the preserved non-path mentions `markdown-for-humans@` = 8 and `markdown-for-humans_dev-` = 49. If any count moved, a placeholder disturbed a counted region - fix the placeholder.
  - [ ] **(b) Confirm the `claude.json` fixtures are synthetic** - the `after/` (Step 2) and `claude-json-variants/` (Step 4) files are minimized synthetic; verify no file under `test/fixtures/` is a verbatim copy of `~/.claude.json`.
  - [ ] **(c) Confirm the plugin `state.json` is minimized** - reduced in Step 4 to the minimal valid-JSON shape the adapter reads.
  - [ ] **(d) Document provenance.** Write `test/fixtures/README.md` recording: the source of each fixture (the 2026-07-09 reference-move backup at `E:\tmp\claude-move-backup-20260709-090053`), the sanitization performed (credential redaction; `claude.json`/`state.json` replaced by synthetic minima), and the standing rule that fixtures must NEVER be refreshed from live `~/.claude` files without re-running this step.

- [ ] **Step 7: Run and commit**

Run: `cargo test -p cpm-core --test fixtures`
Expected: PASS.
```bash
git add test/fixtures crates/cpm-core/tests/fixtures.rs
git commit -m "test: capture sanitized golden fixtures from 2026-07-09 reference move"
```

---

## Phase 2: Path encoding and the reverse project index

### Task 2.1: Path encoding and normalization

**Files:**
- Create: `crates/cpm-core/src/paths.rs`

**Interfaces:**
- Produces:
  ```rust
  pub fn encode_project_dir(abs: &str) -> String;   // [^A-Za-z0-9] -> '-'
  pub fn normalize_path(abs: &str) -> String;        // lower-case, '\' -> '/'
  pub fn same_volume(a: &str, b: &str) -> bool;      // drive letter, or UNC server+share
  ```

> **Repaired 2026-07-12 (post-review of commit `54e4c2b`).** The original `same_volume`
> here took `normalize_path(p).split('/').next()` as the volume root. `split` emits an
> empty field for the run before the first separator, so every UNC path (`\\server\share`)
> reported a root of `""` and compared equal to every other UNC path - two different file
> servers read as the same volume, which would drive a rename that fails at the OS level.
> UNC is a Windows path form, so the Windows-only v1 scope did not excuse it.
>
> Adversarial verification of that first repair found a second instance of the same bug:
> verbatim paths (`\\?\UNC\server\share\...`) parsed `?` as the server and `unc` as the
> share, so they too collapsed to one root for every server. `std::fs::canonicalize` emits
> verbatim paths on Windows, so this form arrives in practice. DESIGN.md names `dunce` as
> the verbatim-path mitigation, but `dunce` is declared in `Cargo.toml` and **never called
> anywhere in the source** - the mitigation is documented, not enforced. `root()` therefore
> strips the verbatim prefix itself rather than trusting an upstream that does not exist.
>
> The code below is the corrected version. Do not restore the `split('/').next()` one-liner.
> Still open by design: POSIX mount points (see the doc comment and DESIGN.md "Platform scope").

- [ ] **Step 1: Write the failing tests**

`crates/cpm-core/src/paths.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_like_claude_real_dirs() {
        assert_eq!(
            encode_project_dir("E:\\Projects\\prisant-labs\\vs-code-markdown-max"),
            "E--Projects-prisant-labs-vs-code-markdown-max"
        );
        // dot collapses (verified real dir): .claude -> -claude, v2.26.0 -> v2-26-0
        assert_eq!(
            encode_project_dir("E:\\Projects\\pm-skills\\docs\\internal\\release-plans\\v2.26.0"),
            "E--Projects-pm-skills-docs-internal-release-plans-v2-26-0"
        );
        assert_eq!(
            encode_project_dir("E:\\Projects\\Chrome - Bookmark Autosort"),
            "E--Projects-Chrome---Bookmark-Autosort"
        );
    }

    #[test]
    fn normalize_is_case_and_slash_insensitive() {
        assert_eq!(normalize_path("E:\\Projects\\A"), "e:/projects/a");
        assert_eq!(normalize_path("e:/Projects/A"), "e:/projects/a");
    }

    #[test]
    fn same_volume_compares_drive_root() {
        assert!(same_volume("E:\\a", "E:\\b\\c"));
        assert!(same_volume("E:\\a", "e:/b"));
        assert!(!same_volume("E:\\a", "F:\\a"));
    }

    #[test]
    fn same_volume_distinguishes_unc_servers_and_shares() {
        // A UNC volume is identified by \\server\share, not by the empty segment
        // that precedes the leading separator. Two different servers are two
        // different volumes, and a rename between them fails at the OS level.
        assert!(!same_volume(r"\\server1\share\a", r"\\server2\share\b"));
        assert!(!same_volume(r"\\server1\alpha\a", r"\\server1\beta\b"));
        assert!(same_volume(r"\\server1\share\a", r"\\SERVER1\SHARE\b"));
        // A UNC path and a local drive are never the same volume.
        assert!(!same_volume(r"\\server1\share\a", "E:\\a"));
    }

    #[test]
    fn same_volume_sees_through_verbatim_prefixes() {
        // std::fs::canonicalize returns \\?\-prefixed paths on Windows, so this
        // form reaches us in practice. The verbatim prefix is a Win32 path-parsing
        // escape, not part of the volume identity: \\?\C:\x is the same volume as
        // C:\x, and \\?\UNC\server\share is the same volume as \\server\share.
        assert!(same_volume(r"\\?\C:\a", r"C:\b"));
        assert!(!same_volume(r"\\?\C:\a", r"\\?\D:\a"));
        assert!(same_volume(
            r"\\?\UNC\server1\share\a",
            r"\\server1\share\b"
        ));
        assert!(!same_volume(
            r"\\?\UNC\server1\share\a",
            r"\\?\UNC\server2\share\b"
        ));
    }
}
```

> **Formatting note.** The blocks above are `cargo fmt` output. CI gates on
> `cargo fmt --all -- --check`, so transcribe them verbatim: hand-reflowing the
> `assert!` calls back onto single lines will fail the build.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p cpm-core paths::tests`
Expected: FAIL to compile (functions undefined).

- [ ] **Step 3: Implement**

Top of `crates/cpm-core/src/paths.rs`:
```rust
/// Map an absolute path to Claude Code's `projects/` directory name.
/// Every character that is not ASCII alphanumeric becomes '-'. Forward-only:
/// this is lossy (a-b, a.b, a\b, a_b all collapse) so it is NEVER used to look
/// up an existing directory - use the reverse index for that.
pub fn encode_project_dir(abs: &str) -> String {
    abs.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Case- and separator-insensitive key for comparing two absolute paths.
pub fn normalize_path(abs: &str) -> String {
    abs.replace('\\', "/").to_lowercase()
}

/// True when two absolute paths live on the same volume.
///
/// Windows drive-letter paths compare by drive (`E:`), and UNC paths compare by
/// server plus share (`\\server\share`), which is the unit a rename cannot cross.
///
/// POSIX mount points are NOT handled: every absolute POSIX path reports the same
/// volume, so a move across mounts would be treated as a rename. That gap is part
/// of the macOS/Linux bring-up tracked in DESIGN.md ("Platform scope") and must
/// close before POSIX support ships.
pub fn same_volume(a: &str, b: &str) -> bool {
    fn root(p: &str) -> String {
        let n = normalize_path(p);

        // Strip the Win32 verbatim prefix first. It is a path-parsing escape, not
        // part of the volume identity, and `std::fs::canonicalize` emits it on
        // Windows - so this form reaches us whether or not a caller expects it.
        // `\\?\C:\x` is the drive `c:`; `\\?\UNC\server\share` is that UNC volume.
        let n = match n.strip_prefix("//?/") {
            Some(rest) => match rest.strip_prefix("unc/") {
                Some(unc) => format!("//{unc}"),
                None => rest.to_string(),
            },
            None => n,
        };

        // A leading "//" marks a UNC path. `split` emits an empty field for the
        // run before the first separator, so without this branch every UNC path
        // would report a root of "" and compare equal to every other UNC path.
        if let Some(rest) = n.strip_prefix("//") {
            let mut seg = rest.split('/').filter(|s| !s.is_empty());
            let server = seg.next().unwrap_or("");
            let share = seg.next().unwrap_or("");
            return format!("//{server}/{share}");
        }

        n.split('/').next().unwrap_or("").to_string()
    }
    root(a) == root(b)
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p cpm-core paths::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/cpm-core/src/paths.rs
git commit -m "feat: forward-only path encoder and normalization helpers"
```

### Task 2.2: Reverse project index from stored cwd

**Files:**
- Create: `crates/cpm-core/src/index.rs`
- Modify: `crates/cpm-core/src/lib.rs` (add `pub mod index;`, `pub mod model;`, `pub mod error;`)
- Create: `crates/cpm-core/src/error.rs`, `crates/cpm-core/src/model.rs` (minimal, grown later)

**Interfaces:**
- Consumes: `FileSystem`, `normalize_path`.
- Produces:
  ```rust
  pub fn read_stored_cwd(fs: &dyn FileSystem, transcript: &Path) -> Option<String>;
  pub struct ProjectIndex {
      pub by_cwd: HashMap<String, Vec<PathBuf>>,   // normalize(cwd) -> encoded dirs
      pub unresolved: Vec<PathBuf>,                 // dirs with no recoverable cwd
      pub cwds: Vec<String>,                        // each ORIGINAL (non-normalized) stored cwd
  }
  impl ProjectIndex { pub fn build(fs: &dyn FileSystem, home: &Path) -> Self; }
  ```

- [ ] **Step 1: Seed a tiny in-memory projects tree and write the failing test**

`crates/cpm-core/src/index.rs`:
```rust
use crate::fs::FileSystem;
use crate::paths::normalize_path;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemoryFileSystem;

    fn line(cwd: &str) -> String {
        format!("{{\"type\":\"user\",\"cwd\":\"{}\",\"uuid\":\"x\"}}\n",
                cwd.replace('\\', "\\\\"))
    }

    #[test]
    fn reads_first_cwd_skipping_summary_lines() {
        let fs = MemoryFileSystem::new();
        // first line is a summary with no cwd (real transcripts start this way)
        let body = format!("{{\"type\":\"last-prompt\",\"leafUuid\":\"z\"}}\n{}",
                           line("E:\\Projects\\Github Repos\\markdown-for-humans"));
        fs.write(Path::new("/h/.claude/projects/E--x/22b2.jsonl"), body.as_bytes()).unwrap();
        let got = read_stored_cwd(&fs, Path::new("/h/.claude/projects/E--x/22b2.jsonl"));
        assert_eq!(got.as_deref(), Some("E:\\Projects\\Github Repos\\markdown-for-humans"));
    }

    #[test]
    fn build_maps_normalized_cwd_and_flags_unresolved() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("/h/.claude/projects/E--a/s.jsonl"),
                 line("E:\\Projects\\A").as_bytes()).unwrap();
        // a dir whose transcript has no cwd -> unresolved
        fs.write(Path::new("/h/.claude/projects/E--b/s.jsonl"),
                 b"{\"type\":\"last-prompt\"}\n").unwrap();
        let idx = ProjectIndex::build(&fs, Path::new("/h"));
        assert_eq!(idx.by_cwd.get("e:/projects/a").unwrap(),
                   &vec![PathBuf::from("/h/.claude/projects/E--a")]);
        assert_eq!(idx.unresolved, vec![PathBuf::from("/h/.claude/projects/E--b")]);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p cpm-core index::tests`
Expected: FAIL to compile.

- [ ] **Step 3: Implement `read_stored_cwd` and `ProjectIndex::build`**

Above the test module in `index.rs`:
```rust
/// Read the first non-empty `cwd` value from a transcript. Scans lines (the
/// first line is often a summary with no cwd) and validates each as JSON before
/// trusting it. Returns the stored path string exactly as recorded.
pub fn read_stored_cwd(fs: &dyn FileSystem, transcript: &Path) -> Option<String> {
    let bytes = fs.read(transcript).ok()?;
    // Read-only heuristic: lossy is safe here because this value is only compared and
    // indexed, never spliced and written back. The write path (apply/verify) hard-fails
    // on invalid UTF-8 instead - see Global Constraints.
    let text = String::from_utf8_lossy(&bytes);
    for l in text.lines() {
        if !l.contains("\"cwd\"") { continue; }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
            if let Some(c) = v.get("cwd").and_then(|x| x.as_str()) {
                if !c.is_empty() { return Some(c.to_string()); }
            }
        }
    }
    None
}

pub struct ProjectIndex {
    pub by_cwd: HashMap<String, Vec<PathBuf>>,
    pub unresolved: Vec<PathBuf>,
    pub cwds: Vec<String>,
}

impl ProjectIndex {
    pub fn build(fs: &dyn FileSystem, home: &Path) -> Self {
        let mut by_cwd: HashMap<String, Vec<PathBuf>> = HashMap::new();
        let mut unresolved = Vec::new();
        let mut cwds = Vec::new();
        let projects = home.join(".claude").join("projects");
        let dirs = fs.read_dir(&projects).unwrap_or_default();
        for dir in dirs {
            if !fs.is_dir(&dir) { continue; }
            let mut found = None;
            for child in fs.read_dir(&dir).unwrap_or_default() {
                if child.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    if let Some(cwd) = read_stored_cwd(fs, &child) {
                        found = Some(cwd);
                        break;
                    }
                }
            }
            match found {
                Some(cwd) => {
                    cwds.push(cwd.clone());          // ORIGINAL stored form, used by plugin_state::audit
                    by_cwd.entry(normalize_path(&cwd)).or_default().push(dir);
                }
                None => unresolved.push(dir),
            }
        }
        Self { by_cwd, unresolved, cwds }
    }
}
```

- [ ] **Step 4: Add module wiring (minimal error/model stubs)**

`crates/cpm-core/src/error.rs`:
```rust
#[derive(Debug)]
pub enum CpmError {
    DestinationExists(String),
    WorktreeSource(String),
    Ambiguous(String),
    Locked(String),
    UnrecognizedFormat(String),
    VerifyFailed(String),
    Io(std::io::Error),
}
impl From<std::io::Error> for CpmError {
    fn from(e: std::io::Error) -> Self { CpmError::Io(e) }
}
pub type Result<T> = std::result::Result<T, CpmError>;
```

`crates/cpm-core/src/model.rs`:
```rust
#[derive(Debug, Clone)]
pub struct Move {
    pub src_abs: String,
    pub dst_abs: String,
}
```

`crates/cpm-core/src/lib.rs`:
```rust
pub mod fs;
pub mod paths;
pub mod error;
pub mod model;
pub mod index;
```

- [ ] **Step 5: Run to verify pass**

Run: `cargo test -p cpm-core index::tests`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/cpm-core/src/index.rs crates/cpm-core/src/error.rs crates/cpm-core/src/model.rs crates/cpm-core/src/lib.rs
git commit -m "feat: reverse project index built from stored cwd"
```

---

## Phase 3: Store trait and the read paths of all adapters

### Task 3.1: Store trait, model types, and registry

**Files:**
- Modify: `crates/cpm-core/src/model.rs` (add `Ctx`, `Hit`, `Stale`, `Change`, `Applied`, `VerifyResult`)
- Create: `crates/cpm-core/src/stores/mod.rs`
- Modify: `crates/cpm-core/src/lib.rs` (add `pub mod stores;`)

**Interfaces:**
- Produces:
  ```rust
  pub enum Scope { Minimal, Standard, Full }   // rewrite tier; gates transcript/sidecar rewrites
  pub struct Ctx<'a> { pub fs: &'a dyn FileSystem, pub home: PathBuf, pub index: &'a ProjectIndex, pub scope: Scope }
  pub struct Hit { pub store: &'static str, pub detail: String, pub target: PathBuf }
  pub struct Stale { pub store: &'static str, pub reference: String, pub location: String }
  pub enum Change {
      RenameDir { from: PathBuf, to: PathBuf },
      MoveTree { from: PathBuf, to: PathBuf },
      RewriteFile { path: PathBuf, rules: Vec<RewriteRule>, expected: usize },
      RenameJsonKey { path: PathBuf, from: String, to: String, expected: usize },
      RewriteJsonArrayValue { path: PathBuf, from: String, to: String, expected: usize },
  }
  pub struct Applied { pub change: String, pub counts: usize }
  pub struct VerifyResult { pub check: String, pub ok: bool, pub detail: String }
  pub trait Store {
      fn id(&self) -> &'static str;
      fn probe(&self, ctx: &Ctx) -> Result<()>;
      fn detect(&self, ctx: &Ctx, mv: &Move) -> Result<Vec<Hit>>;
      fn audit(&self, ctx: &Ctx) -> Result<Vec<Stale>>;
      fn plan(&self, ctx: &Ctx, mv: &Move, hit: &Hit) -> Result<Vec<Change>>;
      fn verify(&self, ctx: &Ctx, mv: &Move) -> Result<Vec<VerifyResult>>;
  }
  pub fn registry() -> Vec<Box<dyn Store>>;
  ```
  Note: `RewriteRule` comes from Phase 5 (`rewrite.rs`). Declare `pub mod rewrite;` now with the struct only, so `model.rs` can reference it.

- [ ] **Step 1: Add the rewrite rule struct early (used by Change)**

`crates/cpm-core/src/rewrite.rs`:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct RewriteRule {
    pub find: String,
    pub replace: String,
}
```
Add `pub mod rewrite;` to `lib.rs`.

- [ ] **Step 2: Write the model types and Store trait**

Append to `crates/cpm-core/src/model.rs`:
```rust
use crate::fs::FileSystem;
use crate::index::ProjectIndex;
use crate::rewrite::RewriteRule;
use crate::error::Result;
use std::path::PathBuf;

/// Rewrite tier. Order matters: Minimal < Standard < Full (derived Ord follows the
/// declaration order), so `scope >= Scope::Standard` gates the transcript rewrites and
/// `scope == Scope::Full` additionally emits sidecar rewrites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope { Minimal, Standard, Full }

pub struct Ctx<'a> {
    pub fs: &'a dyn FileSystem,
    pub home: PathBuf,
    pub index: &'a ProjectIndex,
    pub scope: Scope,
}

#[derive(Debug, Clone)]
pub struct Hit { pub store: &'static str, pub detail: String, pub target: PathBuf }

#[derive(Debug, Clone)]
pub struct Stale { pub store: &'static str, pub reference: String, pub location: String }

#[derive(Debug, Clone)]
pub enum Change {
    RenameDir { from: PathBuf, to: PathBuf },
    MoveTree { from: PathBuf, to: PathBuf },
    RewriteFile { path: PathBuf, rules: Vec<RewriteRule>, expected: usize },
    RenameJsonKey { path: PathBuf, from: String, to: String, expected: usize },
    RewriteJsonArrayValue { path: PathBuf, from: String, to: String, expected: usize },
}

#[derive(Debug, Clone)]
pub struct Applied { pub change: String, pub counts: usize }

#[derive(Debug, Clone)]
pub struct VerifyResult { pub check: String, pub ok: bool, pub detail: String }

pub trait Store {
    fn id(&self) -> &'static str;
    fn probe(&self, ctx: &Ctx) -> Result<()>;
    fn detect(&self, ctx: &Ctx, mv: &Move) -> Result<Vec<Hit>>;
    fn audit(&self, ctx: &Ctx) -> Result<Vec<Stale>>;
    fn plan(&self, ctx: &Ctx, mv: &Move, hit: &Hit) -> Result<Vec<Change>>;
    fn verify(&self, ctx: &Ctx, mv: &Move) -> Result<Vec<VerifyResult>>;
}
```

- [ ] **Step 3: Registry stub**

`crates/cpm-core/src/stores/mod.rs`:
```rust
pub mod claude_projects;
pub mod claude_json;
pub mod claude_history;
pub mod plugin_state;
pub mod sweep;

use crate::model::Store;

pub fn registry() -> Vec<Box<dyn Store>> {
    vec![
        Box::new(claude_projects::ClaudeProjects),
        Box::new(claude_json::ClaudeJson),
        Box::new(claude_history::ClaudeHistory),
        Box::new(plugin_state::PluginState),
        Box::new(sweep::Sweep),
    ]
}
```
Add `pub mod stores;` to `lib.rs`. Create the five store files with a unit struct and `todo!()`-free minimal `Store` impls filled by Tasks 3.2 to 3.5 (write empty `detect`/`plan`/`verify` returning `Ok(vec![])` and a `probe` returning `Ok(())` for now so the crate compiles).

- [ ] **Step 4: Compile**

Run: `cargo build -p cpm-core`
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
git add crates/cpm-core/src/model.rs crates/cpm-core/src/rewrite.rs crates/cpm-core/src/stores crates/cpm-core/src/lib.rs
git commit -m "feat: Store trait, change model, and store registry"
```

### Task 3.2: claude_projects adapter - detect and audit

**Files:**
- Modify: `crates/cpm-core/src/stores/claude_projects.rs`

**Interfaces:**
- Consumes: `ProjectIndex`, `encode_project_dir`, `normalize_path`.
- Produces: `pub struct ClaudeProjects;` implementing `detect` (find the encoded dir + its transcripts for the moved project via the reverse index) and `audit` (report project dirs whose stored cwd points at a folder that no longer exists).

- [ ] **Step 1: Write the failing test**

In `crates/cpm-core/src/stores/claude_projects.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemoryFileSystem;
    use crate::index::ProjectIndex;
    use crate::model::{Ctx, Move};
    use std::path::{Path, PathBuf};

    fn cwd_line(cwd: &str) -> String {
        format!("{{\"type\":\"user\",\"cwd\":\"{}\"}}\n", cwd.replace('\\', "\\\\"))
    }

    #[test]
    fn detect_finds_dir_via_reverse_index_case_insensitive() {
        let fs = MemoryFileSystem::new();
        // dir name uses capital E but stored cwd too; index matches on normalized form
        fs.write(Path::new("/h/.claude/projects/E--Projects-Github-Repos-markdown-for-humans/s.jsonl"),
                 cwd_line("E:\\Projects\\Github Repos\\markdown-for-humans").as_bytes()).unwrap();
        let idx = ProjectIndex::build(&fs, Path::new("/h"));
        let ctx = Ctx { fs: &fs, home: PathBuf::from("/h"), index: &idx, scope: crate::model::Scope::Standard };
        let mv = Move {
            src_abs: "E:\\Projects\\Github Repos\\markdown-for-humans".into(),
            dst_abs: "E:\\Projects\\prisant-labs\\vs-code-markdown-max".into(),
        };
        let hits = ClaudeProjects.detect(&ctx, &mv).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].target.ends_with("E--Projects-Github-Repos-markdown-for-humans"));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p cpm-core stores::claude_projects`
Expected: FAIL.

- [ ] **Step 3: Implement detect and audit**

```rust
use crate::error::Result;
use crate::model::{Ctx, Hit, Move, Stale, Store, Change, VerifyResult};
use crate::paths::normalize_path;

pub struct ClaudeProjects;

impl ClaudeProjects {
    const ID: &'static str = "claude.projects";
}

impl Store for ClaudeProjects {
    fn id(&self) -> &'static str { Self::ID }

    fn probe(&self, _ctx: &Ctx) -> Result<()> { Ok(()) }

    fn detect(&self, ctx: &Ctx, mv: &Move) -> Result<Vec<Hit>> {
        let key = normalize_path(&mv.src_abs);
        let mut hits = Vec::new();
        if let Some(dirs) = ctx.index.by_cwd.get(&key) {
            for dir in dirs {
                hits.push(Hit { store: Self::ID, detail: "project dir".into(), target: dir.clone() });
            }
        }
        Ok(hits)
    }

    fn audit(&self, ctx: &Ctx) -> Result<Vec<Stale>> {
        let mut stale = Vec::new();
        for (cwd_key, dirs) in &ctx.index.by_cwd {
            // cwd_key is normalized; reconstruct a probe path for existence
            let probe = cwd_key.replace('/', "\\");
            if !ctx.fs.exists(std::path::Path::new(&probe))
                && !ctx.fs.exists(std::path::Path::new(cwd_key)) {
                for d in dirs {
                    stale.push(Stale {
                        store: Self::ID,
                        reference: cwd_key.clone(),
                        location: d.to_string_lossy().into_owned(),
                    });
                }
            }
        }
        Ok(stale)
    }

    fn plan(&self, _ctx: &Ctx, _mv: &Move, _hit: &Hit) -> Result<Vec<Change>> { Ok(vec![]) }
    fn verify(&self, _ctx: &Ctx, _mv: &Move) -> Result<Vec<VerifyResult>> { Ok(vec![]) }
}
```
Note: `plan`/`verify` are filled in Phase 5/8. `audit`'s existence check is best-effort against the injected FS; with `RealFileSystem` it hits the real disk, which is what `doctor` needs.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p cpm-core stores::claude_projects`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/cpm-core/src/stores/claude_projects.rs
git commit -m "feat: claude_projects detect via reverse index + audit"
```

### Task 3.3: claude_json adapter - detect and audit (projects keys + githubRepoPaths)

**Files:**
- Modify: `crates/cpm-core/src/stores/claude_json.rs`

**Interfaces:**
- Produces: `pub struct ClaudeJson;`. `probe` validates the file parses and has a `projects` object (hard-fail otherwise). `detect` finds every `"<src>":` key variant and every `githubRepoPaths` array element equal to `src` (any slash/case variant). `audit` reports project keys and `githubRepoPaths` values whose path no longer exists.

- [ ] **Step 1: Write the failing test using the real variants fixture**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemoryFileSystem;
    use crate::index::ProjectIndex;
    use crate::model::{Ctx, Move};
    use std::path::{Path, PathBuf};

    fn ctx_with(json: &str, fs: &MemoryFileSystem) -> ProjectIndex {
        fs.write(Path::new("/h/.claude.json"), json.as_bytes()).unwrap();
        ProjectIndex::build(fs, Path::new("/h"))
    }

    #[test]
    fn probe_rejects_non_object_projects() {
        let fs = MemoryFileSystem::new();
        let idx = ctx_with("{\"projects\": 5}", &fs);
        let ctx = Ctx { fs: &fs, home: PathBuf::from("/h"), index: &idx, scope: crate::model::Scope::Standard };
        assert!(ClaudeJson.probe(&ctx).is_err());
    }

    #[test]
    fn detect_counts_key_variants() {
        let fs = MemoryFileSystem::new();
        let json = r#"{"projects":{"E:\\Projects\\A":{},"E:/Projects/A":{}},"githubRepoPaths":{"o/r":["E:\\Projects\\A"]}}"#;
        let idx = ctx_with(json, &fs);
        let ctx = Ctx { fs: &fs, home: PathBuf::from("/h"), index: &idx, scope: crate::model::Scope::Standard };
        let mv = Move { src_abs: "E:\\Projects\\A".into(), dst_abs: "E:\\Projects\\B".into() };
        let hits = ClaudeJson.detect(&ctx, &mv).unwrap();
        // 2 key variants + 1 githubRepoPaths element
        assert_eq!(hits.len(), 3);
    }

    #[test]
    fn audit_reports_stale_key_absent_from_injected_fs() {
        let fs = MemoryFileSystem::new();
        // a projects key whose path is absent from the injected FS -> reported stale
        let idx = ctx_with(r#"{"projects":{"E:\\Gone\\P":{}}}"#, &fs);
        let ctx = Ctx { fs: &fs, home: PathBuf::from("/h"), index: &idx, scope: crate::model::Scope::Standard };
        let stale = ClaudeJson.audit(&ctx).unwrap();
        assert!(stale.iter().any(|s| s.reference.contains("Gone")));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p cpm-core stores::claude_json`
Expected: FAIL.

- [ ] **Step 3: Implement probe, detect, audit**

```rust
use crate::error::{CpmError, Result};
use crate::model::{Change, Ctx, Hit, Move, Stale, Store, VerifyResult};
use crate::paths::normalize_path;
use std::path::{Path, PathBuf};

pub struct ClaudeJson;
impl ClaudeJson {
    const ID: &'static str = "claude.json";
    fn path(ctx: &Ctx) -> PathBuf { ctx.home.join(".claude.json") }
}

impl Store for ClaudeJson {
    fn id(&self) -> &'static str { Self::ID }

    fn probe(&self, ctx: &Ctx) -> Result<()> {
        let p = Self::path(ctx);
        if !ctx.fs.exists(&p) { return Ok(()); }
        let bytes = ctx.fs.read(&p)?;
        let v: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| CpmError::UnrecognizedFormat(format!("claude.json parse: {e}")))?;
        match v.get("projects") {
            Some(serde_json::Value::Object(_)) | None => Ok(()),
            Some(_) => Err(CpmError::UnrecognizedFormat("claude.json projects is not an object".into())),
        }
    }

    fn detect(&self, ctx: &Ctx, mv: &Move) -> Result<Vec<Hit>> {
        let p = Self::path(ctx);
        if !ctx.fs.exists(&p) { return Ok(vec![]); }
        let bytes = ctx.fs.read(&p)?;
        let v: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| CpmError::UnrecognizedFormat(e.to_string()))?;
        let key = normalize_path(&mv.src_abs);
        let mut hits = Vec::new();
        if let Some(obj) = v.get("projects").and_then(|x| x.as_object()) {
            for k in obj.keys() {
                if normalize_path(k) == key {
                    hits.push(Hit { store: Self::ID, detail: format!("projects key {k}"), target: p.clone() });
                }
            }
        }
        if let Some(grp) = v.get("githubRepoPaths").and_then(|x| x.as_object()) {
            for (slug, arr) in grp {
                if let Some(a) = arr.as_array() {
                    for elem in a {
                        if let Some(s) = elem.as_str() {
                            if normalize_path(s) == key {
                                hits.push(Hit { store: Self::ID,
                                    detail: format!("githubRepoPaths[{slug}] = {s}"), target: p.clone() });
                            }
                        }
                    }
                }
            }
        }
        Ok(hits)
    }

    fn audit(&self, ctx: &Ctx) -> Result<Vec<Stale>> {
        let p = Self::path(ctx);
        if !ctx.fs.exists(&p) { return Ok(vec![]); }
        let bytes = ctx.fs.read(&p)?;
        let v: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| CpmError::UnrecognizedFormat(e.to_string()))?;
        let mut stale = Vec::new();
        let mut check = |raw: &str, loc: String| {
            // Existence goes through the injected FS so this is unit-testable in memory.
            if !ctx.fs.exists(Path::new(raw)) {
                stale.push(Stale { store: Self::ID, reference: raw.to_string(), location: loc });
            }
        };
        if let Some(obj) = v.get("projects").and_then(|x| x.as_object()) {
            for k in obj.keys() { check(k, "projects".into()); }
        }
        if let Some(grp) = v.get("githubRepoPaths").and_then(|x| x.as_object()) {
            for (slug, arr) in grp {
                if let Some(a) = arr.as_array() {
                    for elem in a {
                        if let Some(s) = elem.as_str() { check(s, format!("githubRepoPaths[{slug}]")); }
                    }
                }
            }
        }
        Ok(stale)
    }

    fn plan(&self, _ctx: &Ctx, _mv: &Move, _hit: &Hit) -> Result<Vec<Change>> { Ok(vec![]) }
    fn verify(&self, _ctx: &Ctx, _mv: &Move) -> Result<Vec<VerifyResult>> { Ok(vec![]) }
}
```
Note: `audit` routes existence checks through the injected `ctx.fs`, so it is unit-testable against `MemoryFileSystem` (seed the paths you want treated as existing). In production, `doctor` runs it on `RealFileSystem`, so it hits the real disk - exactly what `doctor` needs.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p cpm-core stores::claude_json`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/cpm-core/src/stores/claude_json.rs
git commit -m "feat: claude_json detect for project keys and githubRepoPaths"
```

### Task 3.4: claude_history and plugin_state adapters - detect and audit

**Files:**
- Modify: `crates/cpm-core/src/stores/claude_history.rs`, `crates/cpm-core/src/stores/plugin_state.rs`

**Interfaces:**
- Produces:
  - `ClaudeHistory`: `detect` finds lines in `~/.claude/history.jsonl` whose `project` equals `src` (normalized); `audit` reports distinct `project` values whose path no longer exists.
  - `PluginState`: `detect` finds `plugins/data/*/state/<base>-<sha256(src_backslash)[:16]>` dirs; produces the new dir name via `sha256(dst_backslash)[:16]`. Exposes `pub fn state_hash(abs_backslash: &str) -> String`.

- [ ] **Step 1: Write the failing tests**

In `plugin_state.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemoryFileSystem;
    use crate::index::ProjectIndex;
    use crate::model::Ctx;
    use std::path::{Path, PathBuf};

    #[test]
    fn hash_matches_real_codex_dir_suffix() {
        // verified: sha256("E:\Projects\Github Repos\markdown-for-humans")[:16]
        assert_eq!(state_hash("E:\\Projects\\Github Repos\\markdown-for-humans"),
                   "e854827f52137cd9");
    }

    #[test]
    fn audit_flags_orphan_state_dir() {
        let fs = MemoryFileSystem::new();
        // an orphan plugin state dir whose 16-hex suffix matches NO live project (the real
        // codex suffix for the pre-move path); no transcripts here -> no known cwd -> stale
        fs.write(Path::new("/h/.claude/plugins/data/codex/state/markdown-for-humans-e854827f52137cd9/state.json"),
                 b"{}").unwrap();
        let idx = ProjectIndex::build(&fs, Path::new("/h"));
        let ctx = Ctx { fs: &fs, home: PathBuf::from("/h"), index: &idx, scope: crate::model::Scope::Standard };
        let stale = PluginState.audit(&ctx).unwrap();
        assert!(stale.iter().any(|s| s.reference.ends_with("e854827f52137cd9")));
    }
}
```

In `claude_history.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemoryFileSystem;
    use crate::index::ProjectIndex;
    use crate::model::{Ctx, Move};
    use std::path::{Path, PathBuf};

    #[test]
    fn detect_finds_matching_project_lines() {
        let fs = MemoryFileSystem::new();
        let body = "{\"project\":\"E:\\\\Projects\\\\A\",\"sessionId\":\"1\"}\n\
                    {\"project\":\"E:\\\\Projects\\\\Other\",\"sessionId\":\"2\"}\n";
        fs.write(Path::new("/h/.claude/history.jsonl"), body.as_bytes()).unwrap();
        let idx = ProjectIndex::build(&fs, Path::new("/h"));
        let ctx = Ctx { fs: &fs, home: PathBuf::from("/h"), index: &idx, scope: crate::model::Scope::Standard };
        let mv = Move { src_abs: "E:\\Projects\\A".into(), dst_abs: "E:\\Projects\\B".into() };
        let hits = ClaudeHistory.detect(&ctx, &mv).unwrap();
        assert_eq!(hits.len(), 1);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p cpm-core stores::plugin_state stores::claude_history`
Expected: FAIL.

- [ ] **Step 3: Implement plugin_state**

```rust
use crate::error::Result;
use crate::model::{Change, Ctx, Hit, Move, Stale, Store, VerifyResult};
use sha2::{Digest, Sha256};

pub struct PluginState;
impl PluginState { const ID: &'static str = "plugin.state"; }

/// Codex plugin state dir suffix: sha256 of the abs path (backslash form), first 16 hex chars.
pub fn state_hash(abs_backslash: &str) -> String {
    let digest = Sha256::digest(abs_backslash.as_bytes());
    hex_lower(&digest)[..16].to_string()
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes { s.push_str(&format!("{b:02x}")); }
    s
}

impl Store for PluginState {
    fn id(&self) -> &'static str { Self::ID }
    fn probe(&self, _ctx: &Ctx) -> Result<()> { Ok(()) }
    fn detect(&self, ctx: &Ctx, mv: &Move) -> Result<Vec<Hit>> {
        let suffix = state_hash(&mv.src_abs);
        let mut hits = Vec::new();
        let data = ctx.home.join(".claude").join("plugins").join("data");
        for plugin in ctx.fs.read_dir(&data).unwrap_or_default() {
            let state = plugin.join("state");
            for entry in ctx.fs.read_dir(&state).unwrap_or_default() {
                if let Some(name) = entry.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(&suffix) {
                        hits.push(Hit { store: Self::ID, detail: name.to_string(), target: entry.clone() });
                    }
                }
            }
        }
        Ok(hits)
    }
    fn audit(&self, ctx: &Ctx) -> Result<Vec<Stale>> {
        // Every live project's state-dir suffix, computed from its ORIGINAL stored cwd
        // (plugins hash the backslash abs-path form; ProjectIndex.cwds preserves it).
        let known: std::collections::BTreeSet<String> =
            ctx.index.cwds.iter().map(|c| state_hash(c)).collect();
        let mut stale = Vec::new();
        let data = ctx.home.join(".claude").join("plugins").join("data");
        for plugin in ctx.fs.read_dir(&data).unwrap_or_default() {
            let state = plugin.join("state");
            for entry in ctx.fs.read_dir(&state).unwrap_or_default() {
                if let Some(name) = entry.file_name().and_then(|n| n.to_str()) {
                    if let Some((_, suffix)) = name.rsplit_once('-') {
                        if suffix.len() == 16 && suffix.chars().all(|c| c.is_ascii_hexdigit())
                            && !known.contains(suffix) {
                            stale.push(Stale { store: Self::ID, reference: name.to_string(),
                                location: state.to_string_lossy().into_owned() });
                        }
                    }
                }
            }
        }
        Ok(stale)
    }
    fn plan(&self, _ctx: &Ctx, _mv: &Move, _hit: &Hit) -> Result<Vec<Change>> { Ok(vec![]) }
    fn verify(&self, _ctx: &Ctx, _mv: &Move) -> Result<Vec<VerifyResult>> { Ok(vec![]) }
}
```

- [ ] **Step 4: Implement claude_history**

```rust
use crate::error::{CpmError, Result};
use crate::model::{Change, Ctx, Hit, Move, Stale, Store, VerifyResult};
use crate::paths::normalize_path;
use std::path::PathBuf;

pub struct ClaudeHistory;
impl ClaudeHistory {
    const ID: &'static str = "claude.history";
    fn path(ctx: &Ctx) -> PathBuf { ctx.home.join(".claude").join("history.jsonl") }
}

impl Store for ClaudeHistory {
    fn id(&self) -> &'static str { Self::ID }
    fn probe(&self, _ctx: &Ctx) -> Result<()> { Ok(()) }
    fn detect(&self, ctx: &Ctx, mv: &Move) -> Result<Vec<Hit>> {
        let p = Self::path(ctx);
        if !ctx.fs.exists(&p) { return Ok(vec![]); }
        let bytes = ctx.fs.read(&p)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|e| CpmError::UnrecognizedFormat(format!("history.jsonl: {e}")))?;
        let key = normalize_path(&mv.src_abs);
        let mut count = 0usize;
        for l in text.lines() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
                if let Some(pr) = v.get("project").and_then(|x| x.as_str()) {
                    if normalize_path(pr) == key { count += 1; }
                }
            }
        }
        Ok(if count > 0 {
            vec![Hit { store: Self::ID, detail: format!("{count} history lines"), target: p }]
        } else { vec![] })
    }
    fn audit(&self, ctx: &Ctx) -> Result<Vec<Stale>> {
        let p = Self::path(ctx);
        if !ctx.fs.exists(&p) { return Ok(vec![]); }
        let bytes = ctx.fs.read(&p)?;
        let text = String::from_utf8_lossy(&bytes);
        let mut seen = std::collections::BTreeSet::new();
        let mut stale = Vec::new();
        for l in text.lines() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
                if let Some(pr) = v.get("project").and_then(|x| x.as_str()) {
                    if seen.insert(pr.to_string()) && !ctx.fs.exists(std::path::Path::new(pr)) {
                        stale.push(Stale { store: Self::ID, reference: pr.to_string(),
                                           location: "history.jsonl".into() });
                    }
                }
            }
        }
        Ok(stale)
    }
    fn plan(&self, _ctx: &Ctx, _mv: &Move, _hit: &Hit) -> Result<Vec<Change>> { Ok(vec![]) }
    fn verify(&self, _ctx: &Ctx, _mv: &Move) -> Result<Vec<VerifyResult>> { Ok(vec![]) }
}
```

- [ ] **Step 5: Run to verify pass, then commit**

Run: `cargo test -p cpm-core stores::plugin_state stores::claude_history`
Expected: PASS.
```bash
git add crates/cpm-core/src/stores/claude_history.rs crates/cpm-core/src/stores/plugin_state.rs
git commit -m "feat: claude_history and plugin_state detect/audit + verified state hash"
```

### Task 3.5: sweep adapter - report-only unknown-region scan

**Files:**
- Modify: `crates/cpm-core/src/stores/sweep.rs`

**Interfaces:**
- Produces: `pub struct Sweep;`. `audit` walks `~/.claude` (excluding adapter-owned regions and binary/large files) and reports any text file still containing a path that no longer exists. `detect`/`plan`/`verify` are no-ops. This store never writes.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemoryFileSystem;
    use crate::index::ProjectIndex;
    use crate::model::Ctx;
    use std::path::{Path, PathBuf};

    #[test]
    fn audit_reports_stale_path_in_unowned_file() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("/h/.claude/some-plugin/notes.txt"),
                 b"ref E:\\\\Gone\\\\project here").unwrap();
        let idx = ProjectIndex::build(&fs, Path::new("/h"));
        let ctx = Ctx { fs: &fs, home: PathBuf::from("/h"), index: &idx, scope: crate::model::Scope::Standard };
        let stale = Sweep.audit(&ctx).unwrap();
        assert!(stale.iter().any(|s| s.reference.contains("Gone")));
    }
}
```
Note: for the sweep the "stale" definition in tests is "matches a caller-provided set of gone paths." To keep the unit test simple, the Sweep here scans for any occurrence of a configured needle set carried on the struct. Adjust the test to construct `Sweep` with a needle (see Step 3).

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p cpm-core stores::sweep`
Expected: FAIL.

- [ ] **Step 3: Implement Sweep (needle-driven, report-only)**

```rust
use crate::error::Result;
use crate::model::{Change, Ctx, Hit, Move, Stale, Store, VerifyResult};

/// Report-only scan of ~/.claude regions no adapter owns. It never writes.
/// It reports files containing any path from the caller-supplied needle set
/// (used by doctor to surface residue in stores the tool does not understand).
pub struct Sweep;
impl Sweep {
    const ID: &'static str = "sweep.unknown";
    const OWNED: &'static [&'static str] = &["projects", "history.jsonl"];
    const SKIP_EXT: &'static [&'static str] = &["db","sqlite","png","jpg","zip","gz","wasm","exe","dll"];
}

impl Store for Sweep {
    fn id(&self) -> &'static str { Self::ID }
    fn probe(&self, _ctx: &Ctx) -> Result<()> { Ok(()) }
    fn detect(&self, _ctx: &Ctx, _mv: &Move) -> Result<Vec<Hit>> { Ok(vec![]) }
    fn audit(&self, ctx: &Ctx) -> Result<Vec<Stale>> {
        // Needles come from other adapters' audit output at the doctor level; here
        // Sweep is a structural placeholder that reports nothing on its own. doctor.rs
        // passes gone-path needles via a dedicated scan (see Task 4.1). This keeps the
        // Store trait uniform while ensuring Sweep can never write.
        let _ = ctx;
        Ok(vec![])
    }
    fn plan(&self, _ctx: &Ctx, _mv: &Move, _hit: &Hit) -> Result<Vec<Change>> { Ok(vec![]) }
    fn verify(&self, _ctx: &Ctx, _mv: &Move) -> Result<Vec<VerifyResult>> { Ok(vec![]) }
}

/// Free function used by doctor: walk unowned text files under ~/.claude and
/// report those containing any needle. Report-only.
pub fn sweep_for(ctx: &Ctx, needles: &[String]) -> Vec<Stale> {
    let root = ctx.home.join(".claude");
    let mut out = Vec::new();
    for f in ctx.fs_walk_text(&root) {
        if Sweep::OWNED.iter().any(|o| f.to_string_lossy().contains(o)) { continue; }
        if let Some(ext) = f.extension().and_then(|e| e.to_str()) {
            if Sweep::SKIP_EXT.contains(&ext) { continue; }
        }
        if let Ok(bytes) = ctx.fs.read(&f) {
            let text = String::from_utf8_lossy(&bytes);
            for n in needles {
                if text.contains(n.as_str()) {
                    out.push(Stale { store: Sweep::ID, reference: n.clone(),
                                     location: f.to_string_lossy().into_owned() });
                }
            }
        }
    }
    out
}
```
Add a `fs_walk_text` helper to `Ctx` in `model.rs` that recursively lists files via `fs.read_dir` (MemoryFileSystem-friendly, no `walkdir` on the injected FS):
```rust
impl<'a> Ctx<'a> {
    pub fn fs_walk_text(&self, root: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for child in self.fs.read_dir(&dir).unwrap_or_default() {
                if self.fs.is_dir(&child) { stack.push(child); }
                else { out.push(child); }
            }
        }
        out
    }
}
```
Update the Step 1 test to call `sweep_for(&ctx, &["E:\\\\Gone\\\\project".into()])` instead of `Sweep.audit`.

- [ ] **Step 4: Run to verify pass, then commit**

Run: `cargo test -p cpm-core stores::sweep`
Expected: PASS.
```bash
git add crates/cpm-core/src/stores/sweep.rs crates/cpm-core/src/model.rs
git commit -m "feat: report-only sweep for stale paths in unowned stores"
```

---

## Phase 4: doctor and scan commands (shippable v0.1)

### Task 4.1: doctor and scan engine functions

**Files:**
- Create: `crates/cpm-core/src/doctor.rs`
- Modify: `crates/cpm-core/src/lib.rs` (`pub mod doctor;`)

**Interfaces:**
- Consumes: `registry()`, `ProjectIndex`, every adapter's `audit`, `sweep_for`.
- Produces:
  ```rust
  pub struct DoctorReport { pub stale: Vec<Stale>, pub unresolved: Vec<PathBuf> }
  pub fn doctor(fs: &dyn FileSystem, home: &Path) -> Result<DoctorReport>;
  pub struct ScanReport { pub hits: Vec<Hit> }
  pub fn scan(fs: &dyn FileSystem, home: &Path, src_abs: &str) -> Result<ScanReport>;
  ```

- [ ] **Step 1: Write the failing test**

`crates/cpm-core/src/doctor.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemoryFileSystem;
    use std::path::Path;

    #[test]
    fn scan_lists_hits_for_a_project() {
        let fs = MemoryFileSystem::new();
        // one project dir with cwd, and a claude.json key for it
        fs.write(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
                 b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n").unwrap();
        fs.write(Path::new("/h/.claude.json"),
                 br#"{"projects":{"E:\\Projects\\A":{}}}"#).unwrap();
        let rep = scan(&fs, Path::new("/h"), "E:\\Projects\\A").unwrap();
        // projects dir hit + claude.json key hit
        assert!(rep.hits.iter().any(|h| h.store == "claude.projects"));
        assert!(rep.hits.iter().any(|h| h.store == "claude.json"));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p cpm-core doctor::tests`
Expected: FAIL.

- [ ] **Step 3: Implement doctor and scan**

```rust
use crate::error::Result;
use crate::fs::FileSystem;
use crate::index::ProjectIndex;
use crate::model::{Ctx, Hit, Move, Stale};
use crate::stores::{registry, sweep::sweep_for};
use std::path::{Path, PathBuf};

pub struct DoctorReport { pub stale: Vec<Stale>, pub unresolved: Vec<PathBuf> }
pub struct ScanReport { pub hits: Vec<Hit> }

pub fn doctor(fs: &dyn FileSystem, home: &Path) -> Result<DoctorReport> {
    let index = ProjectIndex::build(fs, home);
    let ctx = Ctx { fs, home: home.to_path_buf(), index: &index, scope: crate::model::Scope::Standard };
    let mut stale = Vec::new();
    for store in registry() {
        store.probe(&ctx)?;
        stale.extend(store.audit(&ctx)?);
    }
    // feed the gone-path references into the report-only sweep
    let needles: Vec<String> = stale.iter().map(|s| s.reference.clone()).collect();
    stale.extend(sweep_for(&ctx, &needles));
    Ok(DoctorReport { stale, unresolved: index.unresolved.clone() })
}

pub fn scan(fs: &dyn FileSystem, home: &Path, src_abs: &str) -> Result<ScanReport> {
    let index = ProjectIndex::build(fs, home);
    let ctx = Ctx { fs, home: home.to_path_buf(), index: &index, scope: crate::model::Scope::Standard };
    let mv = Move { src_abs: src_abs.to_string(), dst_abs: String::new() };
    let mut hits = Vec::new();
    for store in registry() {
        store.probe(&ctx)?;
        hits.extend(store.detect(&ctx, &mv)?);
    }
    Ok(ScanReport { hits })
}
```

- [ ] **Step 4: Run to verify pass, then commit**

Run: `cargo test -p cpm-core doctor::tests`
Expected: PASS.
```bash
git add crates/cpm-core/src/doctor.rs crates/cpm-core/src/lib.rs
git commit -m "feat: doctor and scan engine functions over all adapters"
```

### Task 4.2: cpm-cli crate with doctor and scan subcommands

**Files:**
- Create: `crates/cpm-cli/Cargo.toml`, `crates/cpm-cli/src/main.rs`, `crates/cpm-cli/src/exit.rs`
- Modify: root `Cargo.toml` (add `crates/cpm-cli` to members)

**Interfaces:**
- Consumes: `cpm_core::doctor::{doctor, scan}`.
- Produces: `cpm doctor [--home <p>] [--json]` and `cpm scan --src <p> [--home <p>] [--json]`. Exit `0` on success.

- [ ] **Step 1: Add the CLI crate and register it**

`crates/cpm-cli/Cargo.toml`:
```toml
[package]
name = "cpm-cli"
edition.workspace = true
version.workspace = true

[[bin]]
name = "cpm"
path = "src/main.rs"

[dependencies]
cpm-core = { path = "../cpm-core" }
clap = { version = "4", features = ["derive"] }
serde_json.workspace = true
```
Add `"crates/cpm-cli"` to root `Cargo.toml` `members`.

- [ ] **Step 2: Write the exit-code map (used fully in Phase 9)**

`crates/cpm-cli/src/exit.rs`:
```rust
use cpm_core::error::CpmError;

pub fn code_for(err: &CpmError) -> i32 {
    match err {
        CpmError::DestinationExists(_) | CpmError::WorktreeSource(_)
        | CpmError::Ambiguous(_) | CpmError::Locked(_) => 2,
        CpmError::VerifyFailed(_) => 3,
        CpmError::UnrecognizedFormat(_) => 4,
        CpmError::Io(_) => 1,
    }
}
```

- [ ] **Step 3: Write main.rs with clap**

`crates/cpm-cli/src/main.rs`:
```rust
mod exit;
use clap::{Parser, Subcommand};
use cpm_core::doctor::{doctor, scan};
use cpm_core::fs::RealFileSystem;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "cpm", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
    /// Home directory holding .claude and .claude.json
    #[arg(long, global = true)]
    home: Option<PathBuf>,
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// Report path-keyed state that references folders that no longer exist
    Doctor,
    /// List all state that references a project's absolute path
    Scan { #[arg(long)] src: String },
}

fn home_of(cli: &Cli) -> PathBuf {
    cli.home.clone().unwrap_or_else(|| dirs_home())
}
fn dirs_home() -> PathBuf {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from).expect("home dir")
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let fs = RealFileSystem;
    let home = home_of(&cli);
    let result = match &cli.cmd {
        Cmd::Doctor => doctor(&fs, &home).map(|r| {
            if cli.json {
                println!("{}", serde_json::json!({
                    "stale": r.stale.iter().map(|s| serde_json::json!({
                        "store": s.store, "reference": s.reference, "location": s.location
                    })).collect::<Vec<_>>(),
                    "unresolved": r.unresolved.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
                }));
            } else {
                println!("Stale references: {}", r.stale.len());
                for s in &r.stale { println!("  [{}] {} @ {}", s.store, s.reference, s.location); }
                println!("Unresolvable project dirs: {}", r.unresolved.len());
            }
        }),
        Cmd::Scan { src } => scan(&fs, &home, src).map(|r| {
            println!("Hits for {src}: {}", r.hits.len());
            for h in &r.hits { println!("  [{}] {} -> {}", h.store, h.detail, h.target.display()); }
        }),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => { eprintln!("error: {e:?}"); ExitCode::from(exit::code_for(&e) as u8) }
    }
}
```
Note: `Stale`/`Hit` need `serde::Serialize` OR the manual `json!` mapping above (used here to avoid adding serde derive to core). Keep the manual mapping.

- [ ] **Step 4: Build, then run against the real machine (the honesty checkpoint)**

Run: `cargo build -p cpm-cli`
Run: `cargo run -p cpm-cli -- doctor`
Expected: prints stale references. Confirm it lists the 6 known stale `githubRepoPaths` (e.g. `E:\Projects\Chrome - Bookmark Autosort`), stale `history.jsonl` values, and the orphaned plugin dir found in discovery. This is the phase-4 milestone gate: the read layer is trustworthy iff these appear.

- [ ] **Step 5: Commit**

```bash
git add crates/cpm-cli Cargo.toml
git commit -m "feat: cpm CLI with doctor and scan (v0.1 read-only milestone)"
```

---

## Phase 5: Anchored rewrite engine

### Task 5.1: anchored_rewrite with count checking

**Files:**
- Modify: `crates/cpm-core/src/rewrite.rs`

**Interfaces:**
- Produces:
  ```rust
  pub fn anchored_rewrite(text: &str, rules: &[RewriteRule]) -> (String, usize);
  ```
  Applies each rule as a literal (non-regex) replace, summing occurrences. Rules are applied in order; each rule's `find` must be specific enough that order does not cause double-rewrites (guaranteed by build_path_rules emitting disjoint anchored forms).

- [ ] **Step 1: Write the failing test**

In `crates/cpm-core/src/rewrite.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_replaces_literally() {
        let rules = vec![
            RewriteRule { find: "a/b/".into(), replace: "x/y/".into() },
        ];
        let (out, n) = anchored_rewrite("a/b/1 a/b/2 a/bc", &rules);
        assert_eq!(n, 2);                    // a/bc must NOT match (no trailing slash)
        assert_eq!(out, "x/y/1 x/y/2 a/bc");
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p cpm-core rewrite::tests::counts_and_replaces_literally`
Expected: FAIL.

- [ ] **Step 3: Implement**

```rust
pub fn anchored_rewrite(text: &str, rules: &[RewriteRule]) -> (String, usize) {
    let mut out = text.to_string();
    let mut total = 0usize;
    for r in rules {
        if r.find.is_empty() { continue; }
        total += out.matches(&r.find).count();
        out = out.replace(&r.find, &r.replace);
    }
    (out, total)
}
```

- [ ] **Step 4: Run to verify pass, then commit**

Run: `cargo test -p cpm-core rewrite::tests`
Expected: PASS.
```bash
git add crates/cpm-core/src/rewrite.rs
git commit -m "feat: literal count-checked anchored_rewrite"
```

### Task 5.2: build_path_rules and the golden reference-move count test

**Files:**
- Modify: `crates/cpm-core/src/rewrite.rs`
- Test: `crates/cpm-core/tests/anchored_reference.rs`

**Interfaces:**
- Produces:
  ```rust
  pub fn build_path_rules(old_abs: &str, new_abs: &str) -> Vec<RewriteRule>;
  ```
  Emits exactly three anchored forms per direction, JSON-escaped where the store escapes:
  - exact cwd field: `"cwd":"<oldEsc>"` -> `"cwd":"<newEsc>"`
  - backslash prefix: `<oldEsc>\\` -> `<newEsc>\\` (escaped backslash + separator)
  - forward prefix: `<oldFwd>/` -> `<newFwd>/`
  where `<oldEsc>` is the path with each `\` doubled (JSON escaping) and `<oldFwd>` is the forward-slash form. Never a bare prefix.

- [ ] **Step 1: Write the golden test against the real fixture**

`crates/cpm-core/tests/anchored_reference.rs`:
```rust
use cpm_core::rewrite::{anchored_rewrite, build_path_rules};
use std::path::Path;

fn read(rel: &str) -> String {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();
    String::from_utf8_lossy(&std::fs::read(base.join(rel)).unwrap()).into_owned()
}

#[test]
fn reproduces_reference_move_counts_and_preserves_non_paths() {
    let old = "E:\\Projects\\Github Repos\\markdown-for-humans";
    let new = "E:\\Projects\\prisant-labs\\vs-code-markdown-max";
    let rules = build_path_rules(old, new);

    let mut total = 0usize;
    for f in ["22b2362e-e4ef-4042-9b01-e3cba5719590.jsonl",
              "28fd093e-f5ef-4dc7-af16-ea415c1840f7.jsonl"] {
        let text = read(&format!(
            "test/fixtures/reference-move/before/projects/E--Projects-Github-Repos-markdown-for-humans/{f}"));
        let (out, n) = anchored_rewrite(&text, &rules);
        total += n;
        // non-path mentions preserved
        assert_eq!(text.matches("markdown-for-humans@").count(),
                   out.matches("markdown-for-humans@").count());
        assert_eq!(text.matches("markdown-for-humans_dev-").count(),
                   out.matches("markdown-for-humans_dev-").count());
        // line count unchanged
        assert_eq!(text.lines().count(), out.lines().count());
        // no old path remains where anchored
        assert!(!out.contains(r#""cwd":"E:\\Projects\\Github Repos\\markdown-for-humans""#));
    }
    // cwd 1467 + backslash 588 + forward 27 = 2082 anchored replacements
    assert_eq!(total, 1467 + 588 + 27);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p cpm-core --test anchored_reference`
Expected: FAIL (build_path_rules undefined).

- [ ] **Step 3: Implement build_path_rules**

```rust
pub fn build_path_rules(old_abs: &str, new_abs: &str) -> Vec<RewriteRule> {
    let esc = |p: &str| p.replace('\\', "\\\\");     // JSON-escaped backslash form
    let fwd = |p: &str| p.replace('\\', "/");         // forward-slash form
    let (oe, ne) = (esc(old_abs), esc(new_abs));
    let (of, nf) = (fwd(old_abs), fwd(new_abs));
    vec![
        RewriteRule { find: format!(r#""cwd":"{oe}""#), replace: format!(r#""cwd":"{ne}""#) },
        RewriteRule { find: format!("{oe}\\\\"),        replace: format!("{ne}\\\\") },
        RewriteRule { find: format!("{of}/"),           replace: format!("{nf}/") },
    ]
}
```
Note on the escaped-backslash literal: in Rust source `"\\\\"` is two backslash characters, which is the on-disk JSON escaping of one path separator. So `format!("{oe}\\\\")` yields `E:\\Projects\\...\\markdown-for-humans\\` in real bytes - the path prefix immediately followed by an (escaped) separator.

- [ ] **Step 4: Run to verify pass, then commit**

Run: `cargo test -p cpm-core --test anchored_reference`
Expected: PASS (total == 2082).
```bash
git add crates/cpm-core/src/rewrite.rs crates/cpm-core/tests/anchored_reference.rs
git commit -m "test: golden reference-move rewrite reproduces exact counts"
```

---

## Phase 6: plan and guards

### Task 6.1: Adapter plan() methods

**Files:**
- Modify: `crates/cpm-core/src/stores/claude_projects.rs`, `claude_json.rs`, `claude_history.rs`, `plugin_state.rs`

**Interfaces:**
- Consumes: `build_path_rules`, `encode_project_dir`, `state_hash`.
- Produces: each adapter's `plan(ctx, mv, hit)` returns the `Change`s for that hit:
  - claude_projects: one `RenameDir` (old encoded -> new encoded) plus one `RewriteFile` per `*.jsonl` with `build_path_rules` and expected count from a dry count.
  - claude_json: one `RenameJsonKey` per key-variant hit; one `RewriteJsonArrayValue` per githubRepoPaths hit.
  - claude_history: one `RewriteFile` on history.jsonl with a project-field rule.
  - plugin_state: one `RenameDir` (old suffix -> new suffix).

- [ ] **Step 1: Write the failing test (claude_projects plan)**

Add to `claude_projects.rs` tests:
```rust
#[test]
fn plan_emits_rename_and_rewrites() {
    let fs = MemoryFileSystem::new();
    fs.write(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
             b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n").unwrap();
    let idx = ProjectIndex::build(&fs, Path::new("/h"));
    let ctx = Ctx { fs: &fs, home: PathBuf::from("/h"), index: &idx, scope: crate::model::Scope::Standard };
    let mv = Move { src_abs: "E:\\Projects\\A".into(), dst_abs: "E:\\Projects\\B".into() };
    let hit = ClaudeProjects.detect(&ctx, &mv).unwrap().remove(0);
    let changes = ClaudeProjects.plan(&ctx, &mv, &hit).unwrap();
    assert!(matches!(changes[0], crate::model::Change::RenameDir { .. }));
    assert!(changes.iter().any(|c| matches!(c, crate::model::Change::RewriteFile { .. })));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p cpm-core stores::claude_projects::tests::plan_emits`
Expected: FAIL (plan returns empty).

- [ ] **Step 3: Implement claude_projects::plan**

Replace the stub `plan` in `claude_projects.rs`:
```rust
fn plan(&self, ctx: &Ctx, mv: &Move, hit: &Hit) -> Result<Vec<Change>> {
    use crate::error::CpmError;
    use crate::model::Scope;
    use crate::paths::encode_project_dir;
    use crate::rewrite::{anchored_rewrite, build_path_rules};
    let projects = ctx.home.join(".claude").join("projects");
    let new_dir = projects.join(encode_project_dir(&mv.dst_abs));
    let mut changes = vec![Change::RenameDir { from: hit.target.clone(), to: new_dir.clone() }];
    let rules = build_path_rules(&mv.src_abs, &mv.dst_abs);
    // Scope tiers (B-05): Minimal renames the dir and rewrites nothing inside; Standard
    // (default) rewrites the moved project's own transcripts; Full also rewrites sidecars.
    if ctx.scope >= Scope::Standard {
        for child in ctx.fs.read_dir(&hit.target).unwrap_or_default() {
            if child.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                let bytes = ctx.fs.read(&child)?;
                let text = std::str::from_utf8(&bytes).map_err(|e|
                    CpmError::UnrecognizedFormat(format!("{}: {e}", child.display())))?;
                let (_, n) = anchored_rewrite(text, &rules);
                // the file lives under the NEW dir after the rename; path is post-rename
                let post = new_dir.join(child.file_name().unwrap());
                changes.push(Change::RewriteFile { path: post, rules: rules.clone(), expected: n });
            }
        }
    }
    if ctx.scope == Scope::Full {
        // Full adds sidecars: memory/*.md and <sessionId>/ subdir files (tool-results,
        // subagents). Same anchored rules; skip the top-level *.jsonl already handled above.
        for side in ctx.fs_walk_text(&hit.target) {
            let rel = match side.strip_prefix(&hit.target) { Ok(r) => r, Err(_) => continue };
            let top_level = rel.parent().map(|p| p.as_os_str().is_empty()).unwrap_or(true);
            if top_level && side.extension().and_then(|e| e.to_str()) == Some("jsonl") { continue; }
            let bytes = ctx.fs.read(&side)?;
            let text = std::str::from_utf8(&bytes).map_err(|e|
                CpmError::UnrecognizedFormat(format!("{}: {e}", side.display())))?;
            let (_, n) = anchored_rewrite(text, &rules);
            if n == 0 { continue; }
            changes.push(Change::RewriteFile { path: new_dir.join(rel), rules: rules.clone(), expected: n });
        }
    }
    Ok(changes)
}
```
Full-scope sidecar rewriting is opt-in via `--scope=full` (Task 9.2). At Standard (default) only the moved project's own transcripts are rewritten; Minimal renames the dir and touches no file contents. Other projects' transcripts are never rewritten at any tier.

- [ ] **Step 4: Implement the other three adapters' plan**

claude_json.rs `plan`:
```rust
fn plan(&self, _ctx: &Ctx, mv: &Move, hit: &Hit) -> Result<Vec<Change>> {
    let p = hit.target.clone();
    if let Some(rest) = hit.detail.strip_prefix("projects key ") {
        let to = crate::paths_dst_key(rest, &mv.src_abs, &mv.dst_abs);
        return Ok(vec![Change::RenameJsonKey { path: p, from: format!("\"{rest}\":"),
            to: format!("\"{to}\":"), expected: 1 }]);
    }
    if hit.detail.starts_with("githubRepoPaths") {
        // detail formatted as: githubRepoPaths[slug] = <value>
        let value = hit.detail.split(" = ").nth(1).unwrap().to_string();
        let to = crate::paths_dst_key(&value, &mv.src_abs, &mv.dst_abs);
        return Ok(vec![Change::RewriteJsonArrayValue { path: p,
            from: format!("\"{value}\""), to: format!("\"{to}\""), expected: 1 }]);
    }
    Ok(vec![])
}
```
Add a small helper `paths_dst_key` in `paths.rs` (re-export at crate root as `crate::paths_dst_key`) that maps a source key's slash/case style onto the destination:
```rust
/// Given an existing key that matches src (in some slash/case variant), produce the
/// destination key preserving that variant's separator style.
pub fn dst_key(existing: &str, src_abs: &str, dst_abs: &str) -> String {
    let uses_fwd = existing.contains('/') && !existing.contains('\\');
    let src_style = if uses_fwd { src_abs.replace('\\', "/") } else { src_abs.to_string() };
    let dst_style = if uses_fwd { dst_abs.replace('\\', "/") } else { dst_abs.to_string() };
    // existing may differ from src only in case/separator; swap the src portion for dst
    if existing.len() >= src_style.len() {
        format!("{}{}", dst_style, &existing[src_style.len().min(existing.len())..])
    } else { dst_style }
}
```
Re-export in `lib.rs`: `pub use paths::dst_key as paths_dst_key;`.

claude_history.rs `plan`:
```rust
fn plan(&self, ctx: &Ctx, mv: &Move, hit: &Hit) -> Result<Vec<Change>> {
    let esc = |p: &str| p.replace('\\', "\\\\");
    let key = normalize_path(&mv.src_abs);
    let bytes = ctx.fs.read(&hit.target)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|e| CpmError::UnrecognizedFormat(format!("history.jsonl: {e}")))?;
    // One rule per DISTINCT stored `project` form that normalizes to src, each mapped to
    // dst preserving that form's separator style (mirrors claude_json's dst_key, LEAD-03).
    let mut forms = std::collections::BTreeSet::new();
    for l in text.lines() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
            if let Some(pr) = v.get("project").and_then(|x| x.as_str()) {
                if normalize_path(pr) == key { forms.insert(pr.to_string()); }
            }
        }
    }
    let rules: Vec<crate::rewrite::RewriteRule> = forms.iter().map(|f| crate::rewrite::RewriteRule {
        find: format!("\"project\":\"{}\"", esc(f)),
        replace: format!("\"project\":\"{}\"", esc(&crate::paths::dst_key(f, &mv.src_abs, &mv.dst_abs))),
    }).collect();
    // expected = sum of dry-run counts across every variant rule
    let (_, n) = crate::rewrite::anchored_rewrite(text, &rules);
    Ok(vec![Change::RewriteFile { path: hit.target.clone(), rules, expected: n }])
}
```
Add to `claude_history.rs` tests (proves each variant form is planned for rewrite):
```rust
#[test]
fn plan_emits_one_rule_per_variant_form() {
    let fs = MemoryFileSystem::new();
    // two DISTINCT stored forms of the same path: backslash and forward-slash
    let body = "{\"project\":\"E:\\\\Projects\\\\A\"}\n{\"project\":\"E:/Projects/A\"}\n";
    fs.write(Path::new("/h/.claude/history.jsonl"), body.as_bytes()).unwrap();
    let idx = ProjectIndex::build(&fs, Path::new("/h"));
    let ctx = Ctx { fs: &fs, home: PathBuf::from("/h"), index: &idx, scope: crate::model::Scope::Standard };
    let mv = Move { src_abs: "E:\\Projects\\A".into(), dst_abs: "E:\\Projects\\B".into() };
    let hit = ClaudeHistory.detect(&ctx, &mv).unwrap().remove(0);
    let changes = ClaudeHistory.plan(&ctx, &mv, &hit).unwrap();
    if let crate::model::Change::RewriteFile { rules, expected, .. } = &changes[0] {
        assert_eq!(rules.len(), 2);     // one rule per distinct variant form
        assert_eq!(*expected, 2);       // both lines rewritten
    } else { panic!("expected RewriteFile"); }
}
```

plugin_state.rs `plan`:
```rust
fn plan(&self, _ctx: &Ctx, mv: &Move, hit: &Hit) -> Result<Vec<Change>> {
    // Convention <basename>-<sha256(abs)[:16]> (DESIGN.md Section 2 item 4). BOTH parts
    // derive from the DESTINATION: a plugin recomputing state for the new path looks for
    // basename(dst)-hash(dst), so keeping the OLD basename would re-orphan the dir (LEAD-04).
    let basename = |p: &str| p.rsplit(|c| c == '\\' || c == '/').next().unwrap_or(p).to_string();
    let new_name = format!("{}-{}", basename(&mv.dst_abs), state_hash(&mv.dst_abs));
    let parent = hit.target.parent().unwrap().to_path_buf();
    Ok(vec![Change::RenameDir { from: hit.target.clone(), to: parent.join(new_name) }])
}
```
Note: renaming to `basename(dst)-hash(dst)` makes both the name and the hash suffix match the destination, so a plugin recomputing state for the new path finds it. The `state.json` INSIDE the dir may still hold the old path; rewriting it is a Full-scope pass (an extra `RewriteFile`), out of scope for Standard. Note this in the plan output.

- [ ] **Step 5: Run to verify pass, then commit**

Run: `cargo test -p cpm-core stores::`
Expected: PASS.
```bash
git add crates/cpm-core/src/stores crates/cpm-core/src/paths.rs crates/cpm-core/src/lib.rs
git commit -m "feat: adapter plan() methods for all v1 stores"
```

### Task 6.2: build_plan, guards, and render_plan

**Files:**
- Create: `crates/cpm-core/src/plan.rs`
- Modify: `crates/cpm-core/src/lib.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct Plan { pub mv: Move, pub changes: Vec<Change>, pub warnings: Vec<String>, pub nested: Vec<String> }
  pub struct PlanOpts { pub recursive: bool, pub on_collision: Collision, pub force: bool, pub scope: crate::model::Scope }
  pub enum Collision { Refuse, KeepDest, KeepSrc }
  pub fn build_plan(fs: &dyn FileSystem, home: &Path, mv: &Move, opts: &PlanOpts) -> Result<Plan>;
  pub fn render_plan(plan: &Plan) -> String;
  ```
  Guards enforced in build_plan: destination folder exists -> `DestinationExists`; src `.git` is a file -> `WorktreeSource` unless `opts.force`; a `claude.json` destination key already exists and `Collision::Refuse` -> `DestinationExists`; nested project keys under src are collected into `plan.nested` and only expanded to changes when `opts.recursive`.

- [ ] **Step 1: Write failing tests for the guards**

`crates/cpm-core/src/plan.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemoryFileSystem;
    use crate::model::Move;
    use std::path::Path;

    fn opts() -> PlanOpts { PlanOpts { recursive: false, on_collision: Collision::Refuse, force: false, scope: crate::model::Scope::Standard } }

    #[test]
    fn refuses_when_destination_folder_exists() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("E:/Projects/B/keep.txt"), b"x").unwrap();
        let mv = Move { src_abs: "E:\\Projects\\A".into(), dst_abs: "E:\\Projects\\B".into() };
        let err = build_plan(&fs, Path::new("/h"), &mv, &opts()).unwrap_err();
        assert!(matches!(err, crate::error::CpmError::DestinationExists(_)));
    }

    #[test]
    fn flags_worktree_source_without_force() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("E:/Projects/A/.git"), b"gitdir: ../real").unwrap(); // .git is a FILE
        let mv = Move { src_abs: "E:\\Projects\\A".into(), dst_abs: "E:\\Projects\\B".into() };
        let err = build_plan(&fs, Path::new("/h"), &mv, &opts()).unwrap_err();
        assert!(matches!(err, crate::error::CpmError::WorktreeSource(_)));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p cpm-core plan::tests`
Expected: FAIL.

- [ ] **Step 3: Implement build_plan + render_plan**

```rust
use crate::error::{CpmError, Result};
use crate::fs::FileSystem;
use crate::index::ProjectIndex;
use crate::model::{Change, Ctx, Move, Scope};
use crate::paths::normalize_path;
use crate::stores::registry;
use std::path::Path;

pub enum Collision { Refuse, KeepDest, KeepSrc }
pub struct PlanOpts { pub recursive: bool, pub on_collision: Collision, pub force: bool, pub scope: Scope }
pub struct Plan { pub mv: Move, pub changes: Vec<Change>, pub warnings: Vec<String>, pub nested: Vec<String> }

pub fn build_plan(fs: &dyn FileSystem, home: &Path, mv: &Move, opts: &PlanOpts) -> Result<Plan> {
    // Guard: destination folder exists
    if fs.exists(Path::new(&mv.dst_abs.replace('\\', "/"))) {
        return Err(CpmError::DestinationExists(mv.dst_abs.clone()));
    }
    // Guard: worktree source (.git is a file, not a dir)
    let git = format!("{}/.git", mv.src_abs.replace('\\', "/"));
    if fs.is_file(Path::new(&git)) && !opts.force {
        return Err(CpmError::WorktreeSource(mv.src_abs.clone()));
    }
    let index = ProjectIndex::build(fs, home);
    let ctx = Ctx { fs, home: home.to_path_buf(), index: &index, scope: opts.scope };
    let mut changes = Vec::new();
    let mut warnings = Vec::new();
    let mut nested = Vec::new();

    for store in registry() {
        store.probe(&ctx)?;
        for hit in store.detect(&ctx, mv)? {
            // Collision guard for claude.json destination key
            if store.id() == "claude.json" {
                if let Collision::Refuse = opts.on_collision {
                    if dest_key_exists(&ctx, mv)? {
                        return Err(CpmError::DestinationExists(
                            format!("claude.json already has a key for {}", mv.dst_abs)));
                    }
                }
            }
            changes.extend(store.plan(&ctx, mv, &hit)?);
        }
    }

    // Nested project detection (keys strictly under src)
    let src_key = normalize_path(&mv.src_abs);
    for k in index.by_cwd.keys() {
        if k != &src_key && k.starts_with(&format!("{src_key}/")) {
            nested.push(k.clone());
        }
    }
    if !nested.is_empty() && !opts.recursive {
        warnings.push(format!("{} nested project(s) will break unless --recursive", nested.len()));
    }

    // Folder move is the LAST change (see apply ordering).
    changes.push(Change::MoveTree {
        from: Path::new(&mv.src_abs.replace('\\', "/")).to_path_buf(),
        to: Path::new(&mv.dst_abs.replace('\\', "/")).to_path_buf(),
    });
    Ok(Plan { mv: mv.clone(), changes, warnings, nested })
}

fn dest_key_exists(ctx: &Ctx, mv: &Move) -> Result<bool> {
    let p = ctx.home.join(".claude.json");
    if !ctx.fs.exists(&p) { return Ok(false); }
    let v: serde_json::Value = serde_json::from_slice(&ctx.fs.read(&p)?)
        .map_err(|e| CpmError::UnrecognizedFormat(e.to_string()))?;
    let dk = normalize_path(&mv.dst_abs);
    Ok(v.get("projects").and_then(|x| x.as_object())
        .map(|o| o.keys().any(|k| normalize_path(k) == dk)).unwrap_or(false))
}

pub fn render_plan(plan: &Plan) -> String {
    let mut s = format!("Move {} -> {}\n", plan.mv.src_abs, plan.mv.dst_abs);
    for w in &plan.warnings { s.push_str(&format!("  WARNING: {w}\n")); }
    for c in &plan.changes {
        s.push_str(&match c {
            Change::RenameDir { from, to } => format!("  rename dir {} -> {}\n", from.display(), to.display()),
            Change::MoveTree { from, to } => format!("  move tree  {} -> {}\n", from.display(), to.display()),
            Change::RewriteFile { path, expected, .. } => format!("  rewrite    {} ({expected} edits)\n", path.display()),
            Change::RenameJsonKey { path, from, to, .. } => format!("  json key   {} {from} -> {to}\n", path.display()),
            Change::RewriteJsonArrayValue { path, from, to, .. } => format!("  json array {} {from} -> {to}\n", path.display()),
        });
    }
    s
}
```

- [ ] **Step 4: Run to verify pass, add an insta snapshot of render_plan, commit**

Run: `cargo test -p cpm-core plan::tests`
Expected: PASS. Add an `insta::assert_snapshot!(render_plan(&plan))` test over a small seeded FS and accept the snapshot with `cargo insta accept`.
```bash
git add crates/cpm-core/src/plan.rs crates/cpm-core/src/lib.rs
git commit -m "feat: build_plan with guards, nested detection, and render_plan"
```

---

## Phase 7: backup, apply, report

### Task 7.1: snapshot and Manifest

**Files:**
- Create: `crates/cpm-core/src/backup.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct ManifestEntry { pub original: String, pub backup: String, pub sha256: String }
  pub struct Manifest { pub run_id: String, pub mv: Move, pub entries: Vec<ManifestEntry> }
  pub fn snapshot(plan: &Plan, fs: &dyn FileSystem, backup_root: &Path, run_id: &str) -> Result<Manifest>;
  ```
  Copies the original bytes of every file/dir a change will touch into `<backup_root>/cpm-<run_id>/` and returns a manifest. `run_id` is passed in, never generated in core.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemoryFileSystem;
    use crate::model::Move;
    use crate::plan::{build_plan, PlanOpts, Collision};
    use std::path::Path;

    #[test]
    fn snapshot_backs_up_every_touched_file() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
                 b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n").unwrap();
        fs.write(Path::new("E:/Projects/A/file.txt"), b"payload").unwrap();
        let mv = Move { src_abs: "E:\\Projects\\A".into(), dst_abs: "E:\\Projects\\B".into() };
        let opts = PlanOpts { recursive: false, on_collision: Collision::Refuse, force: false, scope: crate::model::Scope::Standard };
        let plan = build_plan(&fs, Path::new("/h"), &mv, &opts).unwrap();
        let m = snapshot(&plan, &fs, Path::new("/backup"), "TEST").unwrap();
        assert!(!m.entries.is_empty());
        assert!(fs.exists(Path::new("/backup/cpm-TEST")));
    }

    #[test]
    fn snapshot_backs_up_every_old_transcript_with_sha256() {
        let fs = MemoryFileSystem::new();
        let body = b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n";
        fs.write(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"), body).unwrap();
        fs.write(Path::new("E:/Projects/A/file.txt"), b"payload").unwrap();
        let mv = Move { src_abs: "E:\\Projects\\A".into(), dst_abs: "E:\\Projects\\B".into() };
        let opts = PlanOpts { recursive: false, on_collision: Collision::Refuse, force: false, scope: crate::model::Scope::Standard };
        let plan = build_plan(&fs, Path::new("/h"), &mv, &opts).unwrap();
        let m = snapshot(&plan, &fs, Path::new("/backup"), "TEST").unwrap();
        // the PRE-rename transcript must be captured, with its sha256 recorded in the manifest
        let e = m.entries.iter().find(|e| e.original.ends_with("s.jsonl"))
            .expect("transcript backed up");
        assert_eq!(e.sha256, hexd(body));
        assert!(fs.exists(Path::new(&e.backup)));
    }
}
```

- [ ] **Step 2: Run to verify failure, then implement**

```rust
use crate::error::Result;
use crate::fs::FileSystem;
use crate::model::{Change, Move};
use crate::plan::Plan;
use sha2::{Digest, Sha256};
use std::path::Path;

pub struct ManifestEntry { pub original: String, pub backup: String, pub sha256: String }
pub struct Manifest { pub run_id: String, pub mv: Move, pub entries: Vec<ManifestEntry> }

fn hexd(b: &[u8]) -> String { let d = Sha256::digest(b); d.iter().map(|x| format!("{x:02x}")).collect() }

pub fn snapshot(plan: &Plan, fs: &dyn FileSystem, backup_root: &Path, run_id: &str) -> Result<Manifest> {
    let dir = backup_root.join(format!("cpm-{run_id}"));
    fs.create_dir_all(&dir)?;
    let mut entries = Vec::new();
    for (i, c) in plan.changes.iter().enumerate() {
        match c {
            Change::RenameDir { from, .. } => {
                // Snapshot runs BEFORE any rename, so `from` is the PRE-rename dir. Copy every
                // *.jsonl under it wholesale: the plan's RewriteFile paths are POST-rename and
                // do not exist yet, so this is how transcripts actually get backed up (B-01).
                for child in fs.read_dir(from).unwrap_or_default() {
                    if child.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                        backup_one(fs, &dir, &child, &format!("d{i}"), &mut entries)?;
                    }
                }
                // record the rename intent so rollback can undo the dir rename
                entries.push(ManifestEntry { original: from.to_string_lossy().into_owned(),
                    backup: format!("<dir-rename {i}>"), sha256: String::new() });
            }
            // Stable paths that do not move: history.jsonl (a RewriteFile) and claude.json
            // (json edits). backup_one is_file-guards, so a POST-rename transcript path
            // (already captured by the RenameDir arm above) is silently skipped.
            Change::RewriteFile { path, .. } => backup_one(fs, &dir, path, &format!("f{i}"), &mut entries)?,
            Change::RenameJsonKey { path, .. } | Change::RewriteJsonArrayValue { path, .. } =>
                backup_one(fs, &dir, path, &format!("j{i}"), &mut entries)?,
            Change::MoveTree { from, .. } => {
                entries.push(ManifestEntry { original: from.to_string_lossy().into_owned(),
                    backup: format!("<move-tree {i}>"), sha256: String::new() });
            }
        }
    }
    let m = Manifest { run_id: run_id.to_string(), mv: plan.mv.clone(), entries };
    write_manifest(fs, &dir, &m)?;
    Ok(m)
}

/// Copy one existing file into the backup dir and record a manifest entry (original path,
/// backup path, sha256). A plain helper - NOT a closure - so it can borrow `entries`
/// mutably per call without conflicting with the direct pushes above (LEAD-06).
fn backup_one(fs: &dyn FileSystem, dir: &Path, orig: &Path, tag: &str,
              entries: &mut Vec<ManifestEntry>) -> Result<()> {
    if fs.is_file(orig) {
        let bytes = fs.read(orig)?;
        let bpath = dir.join(format!("{tag}-{}", orig.file_name().unwrap().to_string_lossy()));
        fs.write(&bpath, &bytes)?;
        entries.push(ManifestEntry {
            original: orig.to_string_lossy().into_owned(),
            backup: bpath.to_string_lossy().into_owned(),
            sha256: hexd(&bytes),
        });
    }
    Ok(())
}

fn write_manifest(fs: &dyn FileSystem, dir: &Path, m: &Manifest) -> Result<()> {
    let json = serde_json::json!({
        "run_id": m.run_id,
        "src_abs": m.mv.src_abs, "dst_abs": m.mv.dst_abs,
        "entries": m.entries.iter().map(|e| serde_json::json!({
            "original": e.original, "backup": e.backup, "sha256": e.sha256
        })).collect::<Vec<_>>(),
    });
    fs.write(&dir.join("manifest.json"), serde_json::to_vec_pretty(&json).unwrap().as_slice())?;
    Ok(())
}

impl Manifest {
    /// Reconstruct a Manifest from a written manifest.json (used by apply_verified to drive
    /// the backup-comparison postcondition, and available to rollback).
    pub fn load(fs: &dyn FileSystem, path: &Path) -> Result<Manifest> {
        let v: serde_json::Value = serde_json::from_slice(&fs.read(path)?)
            .map_err(|e| crate::error::CpmError::UnrecognizedFormat(e.to_string()))?;
        let entries = v["entries"].as_array().cloned().unwrap_or_default().iter().map(|e| ManifestEntry {
            original: e["original"].as_str().unwrap_or_default().to_string(),
            backup: e["backup"].as_str().unwrap_or_default().to_string(),
            sha256: e["sha256"].as_str().unwrap_or_default().to_string(),
        }).collect();
        Ok(Manifest {
            run_id: v["run_id"].as_str().unwrap_or_default().to_string(),
            mv: Move { src_abs: v["src_abs"].as_str().unwrap_or_default().to_string(),
                       dst_abs: v["dst_abs"].as_str().unwrap_or_default().to_string() },
            entries,
        })
    }
}
```
Invariant: snapshot runs before any rename, so it always copies from PRE-rename locations - the whole `*.jsonl` set under each renamed dir wholesale, plus the stable files (history.jsonl, claude.json) directly. `apply` (Task 7.2) then renames dir-first and rewrites the post-rename paths.

- [ ] **Step 3: Run, then commit**

Run: `cargo test -p cpm-core backup::tests`
Expected: PASS.
```bash
git add crates/cpm-core/src/backup.rs crates/cpm-core/src/lib.rs
git commit -m "feat: snapshot backup with manifest and sha256"
```

### Task 7.2: apply (transactional, folder-move-last) and report

**Files:**
- Create: `crates/cpm-core/src/apply.rs`, `crates/cpm-core/src/report.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct Report { pub run_id: String, pub applied: Vec<Applied>, pub backup_dir: String, pub verify: Option<Vec<VerifyResult>> }
  pub fn apply(plan: &Plan, fs: &dyn FileSystem, backup_root: &Path, run_id: &str) -> Result<Report>;
  ```
  Apply order: snapshot -> all state changes (rename dirs, rewrite files, json edits) -> `MoveTree` last. Each `RewriteFile`/`RenameJsonKey`/`RewriteJsonArrayValue` recounts against live bytes and returns `VerifyFailed` if the live count != expected (refusing to write a surprising diff).

- [ ] **Step 1: Write the failing test: apply reproduces after/ for a rewrite**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, MemoryFileSystem};
    use crate::model::Move;
    use crate::plan::{build_plan, PlanOpts, Collision};
    use std::path::Path;

    #[test]
    fn apply_rewrites_cwd_and_moves_folder_last() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
                 b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n").unwrap();
        fs.write(Path::new("E:/Projects/A/f.txt"), b"x").unwrap();
        let mv = Move { src_abs: "E:\\Projects\\A".into(), dst_abs: "E:\\Projects\\B".into() };
        let opts = PlanOpts { recursive: false, on_collision: Collision::Refuse, force: false, scope: crate::model::Scope::Standard };
        let plan = build_plan(&fs, Path::new("/h"), &mv, &opts).unwrap();
        apply(&plan, &fs, Path::new("/backup"), "T").unwrap();
        // new encoded dir exists, cwd rewritten, folder moved
        let moved = fs.read(Path::new("/h/.claude/projects/E--Projects-B/s.jsonl")).unwrap();
        assert!(String::from_utf8_lossy(&moved).contains("E:\\\\Projects\\\\B"));
        assert!(fs.exists(Path::new("E:/Projects/B/f.txt")));
        assert!(!fs.exists(Path::new("E:/Projects/A/f.txt")));
    }
}
```

- [ ] **Step 2: Run to verify failure, then implement apply**

```rust
use crate::backup::snapshot;
use crate::error::{CpmError, Result};
use crate::fs::FileSystem;
use crate::model::{Applied, Change};
use crate::plan::Plan;
use crate::report::Report;
use crate::rewrite::anchored_rewrite;
use std::path::Path;

pub fn apply(plan: &Plan, fs: &dyn FileSystem, backup_root: &Path, run_id: &str) -> Result<Report> {
    let m = snapshot(plan, fs, backup_root, run_id)?;
    let mut applied = Vec::new();

    // 1. rename dirs first (so post-rename RewriteFile paths resolve), except MoveTree
    for c in &plan.changes {
        if let Change::RenameDir { from, to } = c {
            if fs.exists(from) { fs.rename(from, to)?; }
            applied.push(Applied { change: format!("rename {} -> {}", from.display(), to.display()), counts: 0 });
        }
    }
    // 2. rewrites and json edits
    for c in &plan.changes {
        match c {
            Change::RewriteFile { path, rules, expected } => {
                let bytes = fs.read(path)?;
                let text = std::str::from_utf8(&bytes).map_err(|e|
                    CpmError::UnrecognizedFormat(format!("{}: {e}", path.display())))?;
                let (out, n) = anchored_rewrite(text, rules);
                if n != *expected {
                    return Err(CpmError::VerifyFailed(
                        format!("{}: expected {expected} edits, live count {n}", path.display())));
                }
                fs.write(path, out.as_bytes())?;
                applied.push(Applied { change: format!("rewrite {}", path.display()), counts: n });
            }
            Change::RenameJsonKey { path, from, to, expected }
            | Change::RewriteJsonArrayValue { path, from, to, expected } => {
                let bytes = fs.read(path)?;
                let text = std::str::from_utf8(&bytes).map_err(|e|
                    CpmError::UnrecognizedFormat(format!("{}: {e}", path.display())))?;
                let n = text.matches(from.as_str()).count();
                if n != *expected {
                    return Err(CpmError::VerifyFailed(
                        format!("{}: expected {expected}, live {n}", path.display())));
                }
                fs.write(path, text.replace(from.as_str(), to.as_str()).as_bytes())?;
                applied.push(Applied { change: format!("json {}", path.display()), counts: n });
            }
            _ => {}
        }
    }
    // 3. move tree LAST
    for c in &plan.changes {
        if let Change::MoveTree { from, to } = c {
            if fs.exists(from) { fs.rename(from, to)?; }
            applied.push(Applied { change: format!("move {} -> {}", from.display(), to.display()), counts: 0 });
        }
    }
    Ok(Report { run_id: run_id.to_string(), applied,
        backup_dir: format!("cpm-{run_id}"), verify: None })
}
```
`crates/cpm-core/src/report.rs`:
```rust
use crate::model::{Applied, VerifyResult};
pub struct Report {
    pub run_id: String,
    pub applied: Vec<Applied>,
    pub backup_dir: String,
    pub verify: Option<Vec<VerifyResult>>,
}
```

- [ ] **Step 3: Run, then commit**

Run: `cargo test -p cpm-core apply::tests`
Expected: PASS.
```bash
git add crates/cpm-core/src/apply.rs crates/cpm-core/src/report.rs crates/cpm-core/src/lib.rs
git commit -m "feat: transactional apply, folder-move-last, count-guarded writes"
```

### Task 7.3: End-to-end golden apply against the reference fixture

**Files:**
- Create: `crates/cpm-core/tests/reference_apply.rs`

**Interfaces:**
- Consumes: `seed_memory_fs_from`, `build_plan`, `apply`.
- Produces: a test proving the whole before/ fixture transforms so the moved transcripts contain zero old-cwd fields and the new encoded dir exists.

- [ ] **Step 1: Write the end-to-end test**

```rust
mod fixtures;
use cpm_core::apply::apply;
use cpm_core::fs::FileSystem;
use cpm_core::model::Move;
use cpm_core::plan::{build_plan, PlanOpts, Collision};
use std::path::Path;

#[test]
fn reference_move_end_to_end_leaves_no_old_cwd() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();
    let fs = fixtures::seed_memory_fs_from(&base.join("test/fixtures/reference-move/before"));
    // fixture is seeded under /home/.claude-fixture; treat that as home root
    let home = Path::new("/home/.claude-fixture");
    let mv = Move {
        src_abs: "E:\\Projects\\Github Repos\\markdown-for-humans".into(),
        dst_abs: "E:\\Projects\\prisant-labs\\vs-code-markdown-max".into(),
    };
    // seed the source folder so MoveTree has something to move
    fs.write(Path::new("E:/Projects/Github Repos/markdown-for-humans/.keep"), b"x").unwrap();
    let opts = PlanOpts { recursive: false, on_collision: Collision::Refuse, force: false, scope: crate::model::Scope::Standard };
    let plan = build_plan(&fs, home, &mv, &opts).unwrap();
    apply(&plan, &fs, Path::new("/backup"), "REF").unwrap();

    let new_dir = home.join(".claude/projects/E--Projects-prisant-labs-vs-code-markdown-max");
    for child in fs.read_dir(&new_dir).unwrap() {
        if child.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            let text = String::from_utf8_lossy(&fs.read(&child).unwrap()).into_owned();
            assert!(!text.contains(r#""cwd":"E:\\Projects\\Github Repos\\markdown-for-humans""#));
        }
    }
}
```

- [ ] **Step 2: Run, then commit**

Run: `cargo test -p cpm-core --test reference_apply`
Expected: PASS.
```bash
git add crates/cpm-core/tests/reference_apply.rs
git commit -m "test: end-to-end reference move leaves no old cwd"
```

---

## Phase 8: verify, idempotency, hard-fail, lock detection, auto-rollback

### Task 8.1: Adapter verify() and top-level verify

**Files:**
- Modify: all four writing adapters (`verify`); create `crates/cpm-core/src/verify.rs`

**Interfaces:**
- Produces:
  ```rust
  pub fn verify(fs: &dyn FileSystem, home: &Path, mv: &Move, manifest: Option<&Manifest>) -> Result<Vec<VerifyResult>>;
  ```
  Aggregates each store's `verify`. Postconditions: new encoded dir exists; zero old `"cwd"` fields in moved transcripts; each moved `*.jsonl` still parses per line; claude.json still parses, has new key, lacks old key; history has zero lines whose NORMALIZED `project` equals the normalized old path; plugin dir renamed. When a `manifest` is supplied, one further postcondition per moved transcript: its line count equals its backed-up original (read via the manifest's backup paths). Returns a list; any `ok == false` -> caller treats as failure.

- [ ] **Step 1: Write the failing test (verify passes post-apply, fails on injected staleness)**

`crates/cpm-core/src/verify.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, MemoryFileSystem};
    use crate::model::Move;
    use crate::plan::{build_plan, PlanOpts, Collision};
    use crate::apply::apply;
    use std::path::Path;

    fn setup() -> (MemoryFileSystem, Move) {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
                 b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n").unwrap();
        fs.write(Path::new("E:/Projects/A/f.txt"), b"x").unwrap();
        (fs, Move { src_abs: "E:\\Projects\\A".into(), dst_abs: "E:\\Projects\\B".into() })
    }

    #[test]
    fn verify_passes_after_apply() {
        let (fs, mv) = setup();
        let opts = PlanOpts { recursive: false, on_collision: Collision::Refuse, force: false, scope: crate::model::Scope::Standard };
        let plan = build_plan(&fs, Path::new("/h"), &mv, &opts).unwrap();
        apply(&plan, &fs, Path::new("/backup"), "T").unwrap();
        let results = verify(&fs, Path::new("/h"), &mv, None).unwrap();
        assert!(results.iter().all(|r| r.ok), "{results:?}");
    }
}
```

- [ ] **Step 2: Run to verify failure, then implement**

Implement each adapter `verify` (examples for claude_projects and claude_json):
```rust
// claude_projects::verify
fn verify(&self, ctx: &Ctx, mv: &Move) -> Result<Vec<VerifyResult>> {
    use crate::paths::encode_project_dir;
    let new_dir = ctx.home.join(".claude").join("projects").join(encode_project_dir(&mv.dst_abs));
    let mut out = vec![VerifyResult {
        check: "new projects dir exists".into(),
        ok: ctx.fs.is_dir(&new_dir),
        detail: new_dir.to_string_lossy().into_owned(),
    }];
    let old_cwd = format!(r#""cwd":"{}""#, mv.src_abs.replace('\\', "\\\\"));
    let mut stale = 0usize;
    for child in ctx.fs.read_dir(&new_dir).unwrap_or_default() {
        if child.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            let bytes = ctx.fs.read(&child)?;
            let text = std::str::from_utf8(&bytes).map_err(|e|
                crate::error::CpmError::UnrecognizedFormat(format!("{}: {e}", child.display())))?;
            stale += text.matches(&old_cwd).count();
            for l in text.lines() {
                if !l.trim().is_empty() && serde_json::from_str::<serde_json::Value>(l).is_err() {
                    out.push(VerifyResult { check: "transcript line parses".into(), ok: false,
                        detail: child.to_string_lossy().into_owned() });
                    break;
                }
            }
        }
    }
    out.push(VerifyResult { check: "zero old cwd in moved transcripts".into(),
        ok: stale == 0, detail: format!("{stale} stale") });
    Ok(out)
}
```
```rust
// verify.rs top-level
use crate::backup::Manifest;
use crate::error::Result;
use crate::fs::FileSystem;
use crate::index::ProjectIndex;
use crate::model::{Ctx, Move, VerifyResult};
use crate::stores::registry;
use std::path::{Path, PathBuf};

pub fn verify(fs: &dyn FileSystem, home: &Path, mv: &Move,
              manifest: Option<&Manifest>) -> Result<Vec<VerifyResult>> {
    let index = ProjectIndex::build(fs, home);
    let ctx = Ctx { fs, home: home.to_path_buf(), index: &index, scope: crate::model::Scope::Standard };
    let mut out = Vec::new();
    for store in registry() { out.extend(store.verify(&ctx, mv)?); }
    // Backup comparison runs only when a manifest is supplied: apply_verified passes it,
    // the standalone `cpm verify` passes None (LEAD-08). Compare each moved transcript's
    // line count against its backed-up original.
    if let Some(m) = manifest {
        for e in &m.entries {
            if !e.original.ends_with(".jsonl") || e.sha256.is_empty() { continue; }
            let want = std::str::from_utf8(&fs.read(Path::new(&e.backup))?)
                .map(|t| t.lines().count()).unwrap_or(0);
            let moved = moved_path(&e.original, mv);
            let got = fs.read(&moved).ok().and_then(|b| String::from_utf8(b).ok())
                .map(|t| t.lines().count());
            out.push(VerifyResult {
                check: "transcript line count unchanged vs backup".into(),
                ok: got == Some(want), detail: e.original.clone() });
        }
    }
    Ok(out)
}

/// Map a PRE-rename transcript path to its POST-move location by swapping the old encoded
/// dir segment for the new one.
fn moved_path(original: &str, mv: &Move) -> PathBuf {
    use crate::paths::encode_project_dir;
    let (old_enc, new_enc) = (encode_project_dir(&mv.src_abs), encode_project_dir(&mv.dst_abs));
    PathBuf::from(original.replace(&old_enc, &new_enc))
}
```
Implement claude_json/claude_history/plugin_state verify analogously (new key present, old key absent, file parses; zero history lines whose normalized `project` equals the normalized old path; new plugin dir exists, old absent).

- [ ] **Step 3: Run, then commit**

Run: `cargo test -p cpm-core verify::tests`
Expected: PASS.
```bash
git add crates/cpm-core/src/verify.rs crates/cpm-core/src/stores
git commit -m "feat: per-store verify and aggregate verify"
```

### Task 8.2: apply integrates verify + auto-rollback; idempotency; hard-fail; lock detection

**Files:**
- Modify: `crates/cpm-core/src/apply.rs`; create `crates/cpm-core/src/locks.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct ApplyOpts { pub run_id: String, pub auto_rollback: bool, pub force: bool }
  pub fn apply_verified(plan: &Plan, fs: &dyn FileSystem, backup_root: &Path, opts: &ApplyOpts) -> Result<Report>;
  pub fn detect_live(fs: &dyn FileSystem, home: &Path) -> Vec<String>;  // in locks.rs
  ```
  `apply_verified` runs `apply`, then `verify`; if any check fails and `auto_rollback`, it restores from the manifest and returns `VerifyFailed`. Idempotency: an empty plan (`build_plan` finds no hits and dest folder absent) applies nothing. Hard-fail: `build_plan` already surfaces `UnrecognizedFormat` from `probe`. Lock detection: `detect_live` returns warnings when `~/.claude/ide/*.lock` exist.

- [ ] **Step 1: Write failing tests: auto-rollback on injected verify failure; idempotent re-run**

```rust
#[test]
fn second_apply_is_noop() {
    // build_plan after a completed move finds no source hits and dest exists -> guard.
    // Assert re-running build_plan errors with DestinationExists (the idempotent signal),
    // OR returns an empty change set if dest folder was created by the move. For v1 the
    // DestinationExists guard is the idempotency signal; assert that.
    // (full body mirrors the apply test setup, then a second build_plan call)
}
```
Implement the test body using the Task 8.1 `setup()` pattern: after `apply_verified`, call `build_plan` again and assert `DestinationExists`.

- [ ] **Step 2: Implement apply_verified, rollback restore hook, detect_live**

```rust
pub struct ApplyOpts { pub run_id: String, pub auto_rollback: bool, pub force: bool }

pub fn apply_verified(plan: &Plan, fs: &dyn FileSystem, backup_root: &Path, opts: &ApplyOpts) -> Result<Report> {
    let backup_dir = backup_root.join(format!("cpm-{}", opts.run_id));
    let manifest_path = backup_dir.join("manifest.json");
    // A mid-apply failure (a count mismatch or an io error after some writes) must not leave
    // an unrecoverable ~/.claude: snapshot ran first, so roll back from the manifest and
    // surface BOTH the cause and the backup dir (LEAD-01). Every failure path below names
    // the backup directory.
    let mut report = match apply(plan, fs, backup_root, &opts.run_id) {
        Ok(r) => r,
        Err(e) => {
            if opts.auto_rollback { let _ = crate::rollback::rollback(&manifest_path, fs); }
            return Err(CpmError::VerifyFailed(
                format!("apply failed ({e:?}); backup at {}", backup_dir.display())));
        }
    };
    let manifest = crate::backup::Manifest::load(fs, &manifest_path)?;
    let results = crate::verify::verify(fs, &plan.home, &plan.mv, Some(&manifest))?;
    let failed: Vec<_> = results.iter().filter(|r| !r.ok).collect();
    if !failed.is_empty() {
        if opts.auto_rollback { crate::rollback::rollback(&manifest_path, fs)?; }
        return Err(CpmError::VerifyFailed(
            format!("{} checks failed; backup at {}", failed.len(), backup_dir.display())));
    }
    report.verify = Some(results);
    Ok(report)
}
```
Note: add `home: PathBuf` to the `Plan` struct (set in `build_plan`), so `apply_verified` and rollback know the home root (`&plan.home`). Update `Plan` construction and the tests accordingly (small mechanical change). Every failure path from `apply_verified` names the backup directory so the user can `cpm rollback` it manually.

`crates/cpm-core/src/locks.rs`:
```rust
use crate::fs::FileSystem;
use std::path::Path;

pub fn detect_live(fs: &dyn FileSystem, home: &Path) -> Vec<String> {
    let ide = home.join(".claude").join("ide");
    fs.read_dir(&ide).unwrap_or_default().iter()
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("lock"))
        .map(|p| format!("live IDE lock: {}", p.display()))
        .collect()
}
```

- [ ] **Step 3: Add the hard-fail test with a corrupted fixture**

```rust
#[test]
fn corrupt_claude_json_hard_fails_before_writing() {
    let fs = MemoryFileSystem::new();
    fs.write(Path::new("/h/.claude.json"), b"{ not json").unwrap();
    fs.write(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
             b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n").unwrap();
    fs.write(Path::new("E:/Projects/A/f.txt"), b"x").unwrap();
    let mv = Move { src_abs: "E:\\Projects\\A".into(), dst_abs: "E:\\Projects\\B".into() };
    let opts = PlanOpts { recursive: false, on_collision: Collision::Refuse, force: false, scope: crate::model::Scope::Standard };
    let err = build_plan(&fs, Path::new("/h"), &mv, &opts).unwrap_err();
    assert!(matches!(err, CpmError::UnrecognizedFormat(_)));
    assert!(!fs.exists(Path::new("E:/Projects/B/f.txt"))); // nothing moved
}
```

- [ ] **Step 4: Run, then commit**

Run: `cargo test -p cpm-core apply:: verify:: locks::`
Expected: PASS.
```bash
git add crates/cpm-core/src/apply.rs crates/cpm-core/src/locks.rs crates/cpm-core/src/plan.rs crates/cpm-core/src/lib.rs
git commit -m "feat: apply_verified with auto-rollback, hard-fail, lock detection"
```

---

## Phase 9: rollback and CLI completion (v1.0)

### Task 9.1: rollback from manifest

**Files:**
- Create: `crates/cpm-core/src/rollback.rs`

**Interfaces:**
- Produces:
  ```rust
  pub fn rollback(manifest_path: &Path, fs: &dyn FileSystem) -> Result<()>;
  ```
  Reads `manifest.json`, restores every backed-up file to its `original` path, and moves the folder back (dst -> src) plus renames encoded dirs back. Verifies restored bytes match the recorded sha256.

- [ ] **Step 1: Write the failing test: apply then rollback restores before-state**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, MemoryFileSystem};
    use crate::model::Move;
    use crate::plan::{build_plan, PlanOpts, Collision};
    use crate::apply::apply;
    use std::path::Path;

    #[test]
    fn rollback_restores_pre_move_bytes() {
        let fs = MemoryFileSystem::new();
        let orig = b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n";
        fs.write(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"), orig).unwrap();
        fs.write(Path::new("E:/Projects/A/f.txt"), b"x").unwrap();
        let mv = Move { src_abs: "E:\\Projects\\A".into(), dst_abs: "E:\\Projects\\B".into() };
        let opts = PlanOpts { recursive: false, on_collision: Collision::Refuse, force: false, scope: crate::model::Scope::Standard };
        let plan = build_plan(&fs, Path::new("/h"), &mv, &opts).unwrap();
        apply(&plan, &fs, Path::new("/backup"), "T").unwrap();
        rollback(Path::new("/backup/cpm-T/manifest.json"), &fs).unwrap();
        assert!(fs.exists(Path::new("E:/Projects/A/f.txt")));
        assert!(!fs.exists(Path::new("E:/Projects/B/f.txt")));
        assert_eq!(fs.read(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl")).unwrap(), orig);
    }
}
```

- [ ] **Step 2: Run to verify failure, then implement**

```rust
use crate::error::{CpmError, Result};
use crate::fs::FileSystem;
use sha2::{Digest, Sha256};
use std::path::Path;

pub fn rollback(manifest_path: &Path, fs: &dyn FileSystem) -> Result<()> {
    let v: serde_json::Value = serde_json::from_slice(&fs.read(manifest_path)?)
        .map_err(|e| CpmError::UnrecognizedFormat(e.to_string()))?;
    let src = v["src_abs"].as_str().unwrap().replace('\\', "/");
    let dst = v["dst_abs"].as_str().unwrap().replace('\\', "/");
    // 1. move the folder back if it was moved
    if fs.exists(Path::new(&dst)) && !fs.exists(Path::new(&src)) {
        fs.rename(Path::new(&dst), Path::new(&src))?;
    }
    // 2. restore each backed-up file to its original path; rename dirs back
    for e in v["entries"].as_array().unwrap() {
        let original = e["original"].as_str().unwrap();
        let backup = e["backup"].as_str().unwrap();
        if backup.starts_with("<dir-rename") || backup.starts_with("<move-tree") {
            continue; // handled by the whole-tree restore below
        }
        let bytes = fs.read(Path::new(backup))?;
        // Guard the recovery path of last resort: refuse to restore a backup whose bytes no
        // longer match the sha256 recorded at snapshot time, naming the corrupt file (A-01).
        let want = e["sha256"].as_str().unwrap_or_default();
        let got: String = Sha256::digest(&bytes).iter().map(|x| format!("{x:02x}")).collect();
        if !want.is_empty() && got != want {
            return Err(CpmError::VerifyFailed(format!("backup corrupted: {backup}")));
        }
        fs.write(Path::new(original), &bytes)?;
    }
    // 3. rename encoded project dir back (new -> old) by restoring the old dir tree:
    //    since snapshot copied the whole old projects dir wholesale, the restore in
    //    step 2 already rewrote the old-encoded files back into place. Remove the
    //    new-encoded dir if it lingers.
    Ok(())
}
```
Note: the exact dir-restore mechanics depend on the snapshot strategy chosen in Task 7.1 (snapshot the old projects dir wholesale). Ensure the test in Step 1 passes; if the new-encoded dir lingers, add an explicit `fs.remove_dir_all` of the new-encoded dir keyed off `dst_abs`.

- [ ] **Step 3: Run, then commit**

Run: `cargo test -p cpm-core rollback::tests`
Expected: PASS.
```bash
git add crates/cpm-core/src/rollback.rs crates/cpm-core/src/lib.rs
git commit -m "feat: rollback from manifest restores pre-move state"
```

### Task 9.2: CLI plan/apply/verify/rollback with exit codes and JSON report

**Files:**
- Modify: `crates/cpm-cli/src/main.rs`

**Interfaces:**
- Consumes: `build_plan`, `render_plan`, `apply_verified`, `verify`, `rollback`.
- Produces: subcommands `plan`, `apply`, `verify`, `rollback` with global flags `--backup-root`, `--force`, `--scope`, `--on-collision`, `--recursive`, `--no-auto-rollback`, `--json`. Exit codes per `exit.rs`.

- [ ] **Step 1: Add the subcommands**

Extend the `Cmd` enum:
```rust
#[derive(Subcommand)]
enum Cmd {
    Doctor,
    Scan { #[arg(long)] src: String },
    Plan { #[arg(long)] src: String, #[arg(long)] dst: String },
    Apply { #[arg(long)] src: String, #[arg(long)] dst: String },
    Verify { #[arg(long)] src: String, #[arg(long)] dst: String },
    Rollback { #[arg(long)] report: PathBuf },
}
```
Add global flags to `Cli`: `backup_root: Option<PathBuf>`, `force: bool`, `recursive: bool`, `no_auto_rollback: bool`, `on_collision: Option<String>`, `scope: Option<String>` (`--scope=minimal|standard|full`, default standard).

- [ ] **Step 2: Wire dispatch through a `run` fn**

The dispatch uses `?`, so it lives in a `Result`-returning `run`, not in `main` (which
returns `ExitCode`) - this is the LEAD-06 fix. `main` becomes:
```rust
fn main() -> ExitCode {
    let cli = Cli::parse();
    let fs = RealFileSystem;
    let home = home_of(&cli);
    match run(&cli, &fs, &home) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => { eprintln!("error: {e:?}"); ExitCode::from(exit::code_for(&e) as u8) }
    }
}
```
main.rs adds `use cpm_core::error::CpmError;`, `cpm_core::model::{Move, Scope}`,
`cpm_core::plan::{build_plan, render_plan, PlanOpts, Collision}`,
`cpm_core::apply::{apply_verified, ApplyOpts}`, `cpm_core::verify::verify`,
`cpm_core::rollback::rollback`.
```rust
fn run(cli: &Cli, fs: &RealFileSystem, home: &std::path::Path) -> cpm_core::error::Result<()> {
    match &cli.cmd {
        Cmd::Plan { src, dst } => {
            let mv = Move { src_abs: src.clone(), dst_abs: dst.clone() };
            let plan = build_plan(fs, home, &mv, &plan_opts(cli))?;
            print!("{}", render_plan(&plan));
            Ok(())
        }
        Cmd::Apply { src, dst } => {
            let mv = Move { src_abs: src.clone(), dst_abs: dst.clone() };
            let plan = build_plan(fs, home, &mv, &plan_opts(cli))?;
            let backup_root = cli.backup_root.clone().unwrap_or_else(std::env::temp_dir);
            let run_id = pick_run_id();  // timestamp read in the CLI (core stays deterministic)
            let opts = ApplyOpts { run_id, auto_rollback: !cli.no_auto_rollback, force: cli.force };
            let r = apply_verified(&plan, fs, &backup_root, &opts)?;
            println!("applied {} changes; backup {}", r.applied.len(), r.backup_dir);
            Ok(())
        }
        Cmd::Verify { src, dst } => {
            let mv = Move { src_abs: src.clone(), dst_abs: dst.clone() };
            // Standalone verify has no manifest, so the backup line-count comparison is
            // skipped; it runs only inside apply_verified, which supplies Some(&manifest).
            let results = verify(fs, home, &mv, None)?;
            let failed = results.iter().filter(|r| !r.ok).count();
            for r in &results { println!("  [{}] {}: {}", if r.ok {"ok"} else {"FAIL"}, r.check, r.detail); }
            if failed > 0 { return Err(CpmError::VerifyFailed(format!("{failed} failed"))); }
            Ok(())
        }
        Cmd::Rollback { report } => rollback(report, fs),
    }
}

/// Build PlanOpts from the CLI, mapping --scope (default Standard) and --on-collision.
fn plan_opts(cli: &Cli) -> PlanOpts {
    let scope = match cli.scope.as_deref() {
        Some("minimal") => Scope::Minimal,
        Some("full") => Scope::Full,
        _ => Scope::Standard,
    };
    let on_collision = match cli.on_collision.as_deref() {
        Some("keep-dest") => Collision::KeepDest,
        Some("keep-src") => Collision::KeepSrc,
        _ => Collision::Refuse,
    };
    PlanOpts { recursive: cli.recursive, on_collision, force: cli.force, scope }
}
```
`pick_run_id` reads a timestamp in the CLI (allowed; only core must be deterministic): e.g. `format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())`.

- [ ] **Step 3: CLI integration test with a temp home seeded from the fixture**

`crates/cpm-cli/tests/cli.rs`:
```rust
use std::process::Command;

#[test]
fn plan_writes_nothing_and_exits_zero() {
    // seed a temp dir mirroring test/fixtures/reference-move/before as --home,
    // run the built binary with `plan --src ... --dst ...`, assert exit 0 and that
    // the source transcripts are byte-unchanged afterward.
    let out = Command::new(env!("CARGO_BIN_EXE_cpm"))
        .args(["plan", "--home", "<seeded temp>", "--src",
               "E:\\Projects\\Github Repos\\markdown-for-humans",
               "--dst", "E:\\Projects\\prisant-labs\\vs-code-markdown-max"])
        .output().unwrap();
    assert!(out.status.success());
}
```
Fill `<seeded temp>` by copying the fixture `before/` tree into a `tempfile::tempdir()` and passing its path. Assert no file under the seeded projects dir changed bytes.

- [ ] **Step 4: Run the whole suite and the determinism/no-network check**

Run: `cargo test --workspace`
Expected: PASS. The no-network guarantee is structural: the CI dependency gate (Task 1.1) forbids any network-capable crate (`reqwest|ureq|hyper|curl`), so plan+apply+verify cannot make an outbound request, and the `cargo audit` step catches RUSTSEC advisories. No runtime network test is required. Optionally add `#![forbid(unsafe_code)]` to both crates.

- [ ] **Step 5: MVP acceptance - run the real reference move on a COPY**

Manually copy a scratch project, run `cpm plan` then `cpm apply` against a `--home` pointed at a COPY of `~/.claude` (never the live one for the first real run), and confirm `cpm verify` passes and `cpm rollback` restores. This is the v1.0 gate.

- [ ] **Step 6: Commit**

```bash
git add crates/cpm-cli
git commit -m "feat: complete cpm CLI (plan/apply/verify/rollback) with exit codes"
```

---

## Phase 13: session-keyed linkage and `cpm list` inventory (F13)

Spec: `docs/features/v1.1-inventory-retention-reassociate.md` (F13, AC-28..33).
Depends only on the phase 1-4 read layer.

### Task 13.1: SessionFootprint - link a project to its session-keyed artifacts

**Files:**
- Create: `crates/cpm-core/src/sessions.rs`; modify `lib.rs` (`pub mod sessions;`)

**Interfaces:**
- Consumes: `FileSystem`.
- Produces:
  ```rust
  pub struct SessionFootprint {
      pub session_ids: Vec<String>,          // *.jsonl basenames in the project dir
      pub todos: usize, pub file_history: usize,
      pub session_env: usize, pub tasks: usize,
  }
  pub fn footprint(fs: &dyn FileSystem, home: &Path, project_dir: &Path) -> SessionFootprint;
  ```

- [ ] **Step 1: Write the failing test**

`crates/cpm-core/src/sessions.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, MemoryFileSystem};
    use std::path::Path;

    #[test]
    fn links_session_keyed_stores_by_id() {
        let fs = MemoryFileSystem::new();
        let sid = "28fd093e";
        fs.write(Path::new("/h/.claude/projects/E--A/28fd093e.jsonl"), b"{}\n").unwrap();
        fs.write(Path::new("/h/.claude/todos/28fd093e-agent-28fd093e.json"), b"[]").unwrap();
        fs.write(Path::new("/h/.claude/file-history/28fd093e/x@v1"), b"x").unwrap();
        let fp = footprint(&fs, Path::new("/h"), Path::new("/h/.claude/projects/E--A"));
        assert_eq!(fp.session_ids, vec![sid.to_string()]);
        assert_eq!(fp.todos, 1);
        assert_eq!(fp.file_history, 1);
    }
}
```

- [ ] **Step 2: Run to verify failure, then implement**

```rust
use crate::fs::FileSystem;
use std::path::Path;

pub struct SessionFootprint {
    pub session_ids: Vec<String>,
    pub todos: usize,
    pub file_history: usize,
    pub session_env: usize,
    pub tasks: usize,
}

pub fn footprint(fs: &dyn FileSystem, home: &Path, project_dir: &Path) -> SessionFootprint {
    let mut ids = Vec::new();
    for child in fs.read_dir(project_dir).unwrap_or_default() {
        if child.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            if let Some(stem) = child.file_stem().and_then(|s| s.to_str()) {
                ids.push(stem.to_string());
            }
        }
    }
    let count_matching = |store: &str| -> usize {
        let d = home.join(".claude").join(store);
        fs.read_dir(&d).unwrap_or_default().iter()
            .filter(|p| p.file_name().and_then(|n| n.to_str())
                .map(|n| ids.iter().any(|id| n.contains(id.as_str()))).unwrap_or(false))
            .count()
    };
    SessionFootprint {
        todos: count_matching("todos"),
        file_history: count_matching("file-history"),
        session_env: count_matching("session-env"),
        tasks: count_matching("tasks"),
        session_ids: ids,
    }
}
```

- [ ] **Step 3: Run, then commit**

Run: `cargo test -p cpm-core sessions::tests`
Expected: PASS.
```bash
git add crates/cpm-core/src/sessions.rs crates/cpm-core/src/lib.rs
git commit -m "feat: session-keyed footprint linkage by sessionId"
```

### Task 13.2: `list` engine (ProjectRecord) + terminal/json/html renderers

**Files:**
- Create: `crates/cpm-core/src/list.rs`; modify `cpm-cli/src/main.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct ProjectRecord {
      pub cwd: Option<String>, pub encoded_dir: String,
      pub sessions: usize, pub bytes: u64,
      pub oldest_days: Option<u64>, pub newest_days: Option<u64>,
      pub footprint: SessionFootprint,
      pub json_keys: usize, pub github_paths: usize,
      pub history_lines: usize, pub plugin_dirs: usize,
      pub health: Health,     // Ok | Stale | Unresolved
  }
  pub enum Health { Ok, Stale, Unresolved }
  pub fn list(fs: &dyn FileSystem, home: &Path, now_secs: u64) -> Vec<ProjectRecord>;
  pub fn render_table(recs: &[ProjectRecord]) -> String;
  pub fn render_html(recs: &[ProjectRecord]) -> String;
  ```
  Note: `now_secs` is passed IN (never read from a clock inside core, for determinism); ages are computed by the CLI supplying the current time and per-file mtimes via a small `fs.mtime` addition to the `FileSystem` trait.

- [ ] **Step 1: Extend FileSystem with mtime (needed for ages)**

Add to the `FileSystem` trait and both impls:
```rust
fn mtime_secs(&self, path: &Path) -> std::io::Result<u64>;
```
`RealFileSystem`: `Ok(std::fs::metadata(path)?.modified()?.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())`.
`MemoryFileSystem`: store an mtime per file (extend the map value to `(Vec<u8>, u64)`, defaulting new writes to a counter or 0; tests that check ages set it explicitly via a new `write_at(path, data, mtime)` helper). Keep existing `write` defaulting mtime to 0.

- [ ] **Step 2: Write the failing tests (including AC-31 PATH-keyed counts)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemoryFileSystem;
    use std::path::Path;

    #[test]
    fn list_reports_sessions_and_health() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
                 b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n").unwrap();
        // source folder does not exist -> STALE
        let recs = list(&fs, Path::new("/h"), 1_000_000);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].sessions, 1);
        assert!(matches!(recs[0].health, Health::Stale));
    }

    #[test]
    fn list_counts_json_keys_ac31() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
                 b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n").unwrap();
        fs.write(Path::new("/h/.claude.json"),
                 br#"{"projects":{"E:\\Projects\\A":{}}}"#).unwrap();
        let recs = list(&fs, Path::new("/h"), 1_000_000);
        // AC-31: PATH-keyed declarations are counted per project, never stubbed to 0
        assert!(recs.iter().any(|r| r.json_keys >= 1));
    }
}
```

- [ ] **Step 3: Implement list + renderers**

```rust
use crate::fs::FileSystem;
use crate::index::ProjectIndex;
use crate::model::{Ctx, Move, Store};
use crate::paths::encode_project_dir;
use crate::sessions::{footprint, SessionFootprint};
use crate::stores::claude_history::ClaudeHistory;
use crate::stores::claude_json::ClaudeJson;
use crate::stores::plugin_state::PluginState;
use std::path::Path;

pub enum Health { Ok, Stale, Unresolved }
pub struct ProjectRecord { /* fields as in Interfaces */ }

pub fn list(fs: &dyn FileSystem, home: &Path, now_secs: u64) -> Vec<ProjectRecord> {
    let index = ProjectIndex::build(fs, home);
    let ctx = Ctx { fs, home: home.to_path_buf(), index: &index, scope: crate::model::Scope::Standard };
    let mut out = Vec::new();
    // resolved projects
    for (cwd_key, dirs) in &index.by_cwd {
        for dir in dirs {
            let fp = footprint(fs, home, dir);
            let mut bytes = 0u64;
            let (mut oldest, mut newest) = (None, None);
            for child in fs.read_dir(dir).unwrap_or_default() {
                if child.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    bytes += fs.read(&child).map(|b| b.len() as u64).unwrap_or(0);
                    if let Ok(mt) = fs.mtime_secs(&child) {
                        let age = now_secs.saturating_sub(mt) / 86400;
                        oldest = Some(oldest.map_or(age, |o: u64| o.max(age)));
                        newest = Some(newest.map_or(age, |n: u64| n.min(age)));
                    }
                }
            }
            let exists = Path::new(&cwd_key.replace('/', "\\")).exists()
                || Path::new(cwd_key).exists();
            // Real PATH-keyed counts via the adapters' detect (AC-31), not stubbed zeros.
            let mv = Move { src_abs: cwd_key.clone(), dst_abs: String::new() };
            let cj = ClaudeJson.detect(&ctx, &mv).unwrap_or_default();
            let json_keys = cj.iter().filter(|h| h.detail.starts_with("projects key")).count();
            let github_paths = cj.iter().filter(|h| h.detail.starts_with("githubRepoPaths")).count();
            let history_lines: usize = ClaudeHistory.detect(&ctx, &mv).unwrap_or_default().iter()
                .filter_map(|h| h.detail.split_whitespace().next().and_then(|n| n.parse().ok())).sum();
            let plugin_dirs = PluginState.detect(&ctx, &mv).unwrap_or_default().len();
            out.push(ProjectRecord {
                cwd: Some(cwd_key.clone()),
                encoded_dir: dir.file_name().unwrap().to_string_lossy().into_owned(),
                sessions: fp.session_ids.len(), bytes, oldest_days: oldest, newest_days: newest,
                json_keys, github_paths, history_lines, plugin_dirs,
                footprint: fp,
                health: if exists { Health::Ok } else { Health::Stale },
            });
        }
    }
    for dir in &index.unresolved {
        out.push(ProjectRecord {
            cwd: None, encoded_dir: dir.file_name().unwrap().to_string_lossy().into_owned(),
            sessions: 0, bytes: 0, oldest_days: None, newest_days: None,
            json_keys: 0, github_paths: 0, history_lines: 0, plugin_dirs: 0,
            footprint: SessionFootprint { session_ids: vec![], todos:0, file_history:0, session_env:0, tasks:0 },
            health: Health::Unresolved,
        });
    }
    out
}

pub fn render_table(recs: &[ProjectRecord]) -> String {
    let mut s = format!("{:<50} {:>4} {:>8} {:>6} {:>6}  {}\n",
        "project", "sess", "MB", "oldest", "newest", "health");
    for r in recs {
        s.push_str(&format!("{:<50} {:>4} {:>8.1} {:>6} {:>6}  {}\n",
            r.cwd.clone().unwrap_or_else(|| r.encoded_dir.clone()).chars().take(50).collect::<String>(),
            r.sessions, r.bytes as f64 / 1e6,
            r.oldest_days.map(|d| d.to_string()).unwrap_or_else(|| "-".into()),
            r.newest_days.map(|d| d.to_string()).unwrap_or_else(|| "-".into()),
            match r.health { Health::Ok => "OK", Health::Stale => "STALE", Health::Unresolved => "UNRESOLVED" }));
    }
    s
}

pub fn render_html(recs: &[ProjectRecord]) -> String {
    // minimal self-contained HTML table; a richer renderer may reuse a prior-art viewer.
    let rows: String = recs.iter().map(|r| format!(
        "<tr><td>{}</td><td>{}</td><td>{:.1}</td><td>{}</td></tr>",
        r.cwd.clone().unwrap_or_else(|| r.encoded_dir.clone()),
        r.sessions, r.bytes as f64 / 1e6,
        match r.health { Health::Ok => "OK", Health::Stale => "STALE", Health::Unresolved => "UNRESOLVED" }
    )).collect();
    format!("<!doctype html><meta charset=utf-8><title>cpm list</title>\
        <table border=1><tr><th>project</th><th>sessions</th><th>MB</th><th>health</th></tr>{rows}</table>")
}
```

- [ ] **Step 4: Wire `cpm list` in main.rs (terminal / --json / --html)**

Add `Cmd::List { #[arg(long)] html: Option<PathBuf> }`. Dispatch reads
`SystemTime::now()` in the CLI (allowed), calls `list`, prints `render_table`,
emits JSON with `--json`, and writes `render_html` to `--html <path>`.

- [ ] **Step 5: Run, then commit**

Run: `cargo test -p cpm-core list:: sessions::` and `cargo run -p cpm-cli -- list`
Expected: PASS; the real run lists your 45 projects with session counts and ages,
flagging the one STALE gone-folder project (`relational-connection/fixed`).
```bash
git add crates/cpm-core/src/list.rs crates/cpm-core/src/sessions.rs crates/cpm-core/src/fs.rs crates/cpm-cli/src/main.rs
git commit -m "feat: cpm list inventory with terminal/json/html renderers (F13)"
```

---

## Phase 14: archive engine and `cpm archive` (F14)

Spec: F14, AC-34..39. Depends on the read layer + phase-7 copy primitives.

### Task 14.1: content-hash incremental archive writer

**Files:**
- Create: `crates/cpm-core/src/archive.rs`; modify `lib.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct ArchiveOpts { pub archive_dir: PathBuf, pub render: bool }
  pub struct ArchiveReport { pub copied: usize, pub skipped: usize, pub bytes: u64 }
  pub fn archive_all(fs: &dyn FileSystem, home: &Path, opts: &ArchiveOpts) -> Result<ArchiveReport>;
  pub fn archive_session(fs: &dyn FileSystem, home: &Path, transcript: &Path, opts: &ArchiveOpts) -> Result<()>;
  ```
  Incremental: a file is copied only if absent in the archive or its content SHA-256
  differs (never mtime). Writes atomically (temp + rename). Archives resolved AND
  unresolved project dirs: transcripts, each `<sessionId>/` subdir verbatim, and the
  SESSION-keyed artifacts (`todos`/`file-history`/`session-env`/`tasks`) into
  `session-artifacts/`. Emits `INDEX.md` (projects -> session counts + byte totals) and
  a per-run `manifest.json` (per archived file: source path, sha256, size).

- [ ] **Step 1: Write the failing tests (idempotency; unresolved dirs; session-artifacts; manifest sha)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, MemoryFileSystem};
    use std::path::Path;

    #[test]
    fn archive_is_content_hash_incremental() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("/h/.claude/projects/E--A/s.jsonl"), b"{\"cwd\":\"E:\\\\A\"}\n").unwrap();
        let opts = ArchiveOpts { archive_dir: Path::new("/arch").to_path_buf(), render: false };
        let r1 = archive_all(&fs, Path::new("/h"), &opts).unwrap();
        assert_eq!(r1.copied, 1);
        let r2 = archive_all(&fs, Path::new("/h"), &opts).unwrap();
        assert_eq!(r2.copied, 0);   // unchanged -> skipped
        assert_eq!(r2.skipped, 1);
    }

    #[test]
    fn archives_unresolved_dir_transcript() {
        let fs = MemoryFileSystem::new();
        // a dir whose transcript has no cwd -> unresolved, must still be archived (LEAD-05)
        fs.write(Path::new("/h/.claude/projects/E--Ghost/s.jsonl"), b"{\"type\":\"x\"}\n").unwrap();
        let opts = ArchiveOpts { archive_dir: Path::new("/arch").to_path_buf(), render: false };
        archive_all(&fs, Path::new("/h"), &opts).unwrap();
        assert!(fs.exists(Path::new("/arch/projects/E--Ghost/s.jsonl")));
    }

    #[test]
    fn archives_file_history_under_session_artifacts_and_records_sha() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("/h/.claude/projects/E--A/28fd093e.jsonl"),
                 b"{\"cwd\":\"E:\\\\A\"}\n").unwrap();
        let payload = b"file history payload";
        fs.write(Path::new("/h/.claude/file-history/28fd093e/x@v1"), payload).unwrap();
        let opts = ArchiveOpts { archive_dir: Path::new("/arch").to_path_buf(), render: false };
        archive_all(&fs, Path::new("/h"), &opts).unwrap();
        assert!(fs.exists(Path::new("/arch/session-artifacts/file-history/28fd093e/x@v1")));
        // manifest.json records the sha256 of an archived file (AC-39)
        let m: serde_json::Value =
            serde_json::from_slice(&fs.read(Path::new("/arch/manifest.json")).unwrap()).unwrap();
        let want: String = Sha256::digest(payload).iter().map(|x| format!("{x:02x}")).collect();
        assert!(m["files"].as_array().unwrap().iter().any(|e| e["sha256"] == want));
    }
}
```

- [ ] **Step 2: Run to verify failure, then implement**

```rust
use crate::error::Result;
use crate::fs::FileSystem;
use crate::index::ProjectIndex;
use crate::sessions::footprint;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub struct ArchiveOpts { pub archive_dir: PathBuf, pub render: bool }
pub struct ArchiveReport { pub copied: usize, pub skipped: usize, pub bytes: u64 }

struct ArchEntry { source: String, sha256: String, size: u64 }

fn sha(b: &[u8]) -> String { Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect() }

fn fs_walk(fs: &dyn FileSystem, root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for c in fs.read_dir(&d).unwrap_or_default() {
            if fs.is_dir(&c) { stack.push(c); } else { out.push(c); }
        }
    }
    out
}

fn copy_if_changed(fs: &dyn FileSystem, src: &Path, dst: &Path,
                   rep: &mut ArchiveReport, man: &mut Vec<ArchEntry>) -> Result<()> {
    let bytes = fs.read(src)?;
    let digest = sha(&bytes);
    man.push(ArchEntry { source: src.to_string_lossy().into_owned(),
                         sha256: digest.clone(), size: bytes.len() as u64 });
    if fs.exists(dst) {
        if sha(&fs.read(dst)?) == digest { rep.skipped += 1; return Ok(()); }
    }
    // atomic: write temp then rename
    let tmp = dst.with_extension("tmp-cpm");
    fs.write(&tmp, &bytes)?;
    fs.rename(&tmp, dst)?;
    rep.copied += 1;
    rep.bytes += bytes.len() as u64;
    Ok(())
}

/// Archive one project dir: transcripts, `<sessionId>/` subdirs verbatim, and the
/// SESSION-keyed artifacts. Returns the transcript (session) count.
fn archive_project_dir(fs: &dyn FileSystem, home: &Path, dir: &Path, opts: &ArchiveOpts,
                       rep: &mut ArchiveReport, man: &mut Vec<ArchEntry>) -> Result<usize> {
    let enc = dir.file_name().unwrap();
    let mut sessions = 0usize;
    for child in fs.read_dir(dir).unwrap_or_default() {
        if child.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            let dst = opts.archive_dir.join("projects").join(enc).join(child.file_name().unwrap());
            copy_if_changed(fs, &child, &dst, rep, man)?;
            sessions += 1;
        } else if fs.is_dir(&child) {
            // <sessionId>/ subdirs (tool-results, subagents) copied verbatim
            for f in fs_walk(fs, &child) {
                let rel = f.strip_prefix(dir).unwrap();
                let dst = opts.archive_dir.join("projects").join(enc).join(rel);
                copy_if_changed(fs, &f, &dst, rep, man)?;
            }
        }
    }
    // SESSION-keyed artifacts, matched by sessionId (AC-34/35)
    let fp = footprint(fs, home, dir);
    for store in ["todos", "file-history", "session-env", "tasks"] {
        let root = home.join(".claude").join(store);
        for f in fs_walk(fs, &root) {
            let name = f.to_string_lossy().into_owned();
            if fp.session_ids.iter().any(|id| name.contains(id.as_str())) {
                let rel = f.strip_prefix(&root).unwrap();
                let dst = opts.archive_dir.join("session-artifacts").join(store).join(rel);
                copy_if_changed(fs, &f, &dst, rep, man)?;
            }
        }
    }
    Ok(sessions)
}

pub fn archive_all(fs: &dyn FileSystem, home: &Path, opts: &ArchiveOpts) -> Result<ArchiveReport> {
    let index = ProjectIndex::build(fs, home);
    let mut rep = ArchiveReport { copied: 0, skipped: 0, bytes: 0 };
    let mut man: Vec<ArchEntry> = Vec::new();
    let mut per_project: Vec<(String, usize, u64)> = Vec::new();
    // resolved AND unresolved project dirs - unresolved transcripts are archived too (LEAD-05)
    for dir in index.by_cwd.values().flatten().chain(index.unresolved.iter()) {
        let before = rep.bytes;
        let sessions = archive_project_dir(fs, home, dir, opts, &mut rep, &mut man)?;
        per_project.push((dir.file_name().unwrap().to_string_lossy().into_owned(),
                          sessions, rep.bytes - before));
    }
    write_manifest(fs, opts, &man)?;
    write_index(fs, opts, &per_project)?;
    Ok(rep)
}

pub fn archive_session(fs: &dyn FileSystem, _home: &Path, transcript: &Path, opts: &ArchiveOpts) -> Result<()> {
    let enc = transcript.parent().unwrap().file_name().unwrap();
    let dst = opts.archive_dir.join("projects").join(enc).join(transcript.file_name().unwrap());
    let mut rep = ArchiveReport { copied: 0, skipped: 0, bytes: 0 };
    let mut man = Vec::new();
    copy_if_changed(fs, transcript, &dst, &mut rep, &mut man)
}

fn write_manifest(fs: &dyn FileSystem, opts: &ArchiveOpts, man: &[ArchEntry]) -> Result<()> {
    let json = serde_json::json!({
        "files": man.iter().map(|e| serde_json::json!({
            "source": e.source, "sha256": e.sha256, "size": e.size
        })).collect::<Vec<_>>(),
    });
    fs.write(&opts.archive_dir.join("manifest.json"),
             serde_json::to_vec_pretty(&json).unwrap().as_slice())
}

fn write_index(fs: &dyn FileSystem, opts: &ArchiveOpts, per_project: &[(String, usize, u64)]) -> Result<()> {
    let mut s = String::from("# CPM session archive\n\n| project | sessions | bytes |\n|---|---:|---:|\n");
    for (proj, sessions, bytes) in per_project {
        s.push_str(&format!("| {proj} | {sessions} | {bytes} |\n"));
    }
    fs.write(&opts.archive_dir.join("INDEX.md"), s.as_bytes())
}
```
Note: `archive_project_dir` implements the SESSION-keyed copy (iterating `footprint().session_ids` across `todos`/`file-history`/`session-env`/`tasks`) and the verbatim `<sessionId>/` subdir copy; `archive_all` covers unresolved dirs too (LEAD-05). The per-run `manifest.json` records source/sha256/size for every archived file - the basis for AC-39 drift detection and the future D-02 size-first short-circuit.

- [ ] **Step 3: Run, then commit**

Run: `cargo test -p cpm-core archive::tests`
Expected: PASS.
```bash
git add crates/cpm-core/src/archive.rs crates/cpm-core/src/lib.rs
git commit -m "feat: content-hash incremental archive writer (F14)"
```

### Task 14.2: SessionEnd hook install + retention setting + `cpm archive` CLI

**Files:**
- Create: `crates/cpm-core/src/settings.rs`; modify `cpm-cli/src/main.rs`

**Interfaces:**
- Produces:
  ```rust
  pub fn install_session_end_hook(fs: &dyn FileSystem, home: &Path, cpm_bin: &str, archive_dir: &Path) -> Result<()>;
  pub fn uninstall_session_end_hook(fs: &dyn FileSystem, home: &Path) -> Result<()>;
  pub fn set_retention(fs: &dyn FileSystem, home: &Path, days: u32, allow_zero: bool) -> Result<()>;
  ```
  `set_retention` writes `cleanupPeriodDays` into `~/.claude/settings.json` (parse,
  set key, serialize back - settings.json is CPM-owned config, not a store we must
  byte-preserve). Refuses `0` unless `allow_zero`; prints the #23710/#62272 caveat via
  the CLI. Hook install adds a `SessionEnd` entry invoking
  `<cpm_bin> archive --session "$TRANSCRIPT_PATH" --archive-dir <dir>`.

- [ ] **Step 1: Write the failing test for set_retention refusing 0**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, MemoryFileSystem};
    use std::path::Path;

    #[test]
    fn set_retention_refuses_zero_without_optin() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("/h/.claude/settings.json"), b"{}").unwrap();
        assert!(set_retention(&fs, Path::new("/h"), 0, false).is_err());
        set_retention(&fs, Path::new("/h"), 3650, false).unwrap();
        let v: serde_json::Value =
            serde_json::from_slice(&fs.read(Path::new("/h/.claude/settings.json")).unwrap()).unwrap();
        assert_eq!(v["cleanupPeriodDays"], 3650);
    }
}
```

- [ ] **Step 2: Run to verify failure, then implement**

```rust
use crate::error::{CpmError, Result};
use crate::fs::FileSystem;
use std::path::Path;

pub fn set_retention(fs: &dyn FileSystem, home: &Path, days: u32, allow_zero: bool) -> Result<()> {
    if days == 0 && !allow_zero {
        return Err(CpmError::Locked("cleanupPeriodDays:0 refused (issue #23710); use --force-zero".into()));
    }
    let p = home.join(".claude").join("settings.json");
    let mut v: serde_json::Value = if fs.exists(&p) {
        serde_json::from_slice(&fs.read(&p)?).map_err(|e| CpmError::UnrecognizedFormat(e.to_string()))?
    } else { serde_json::json!({}) };
    v["cleanupPeriodDays"] = serde_json::json!(days);
    fs.write(&p, serde_json::to_vec_pretty(&v).unwrap().as_slice())?;
    Ok(())
}

pub fn install_session_end_hook(fs: &dyn FileSystem, home: &Path, cpm_bin: &str, archive_dir: &Path) -> Result<()> {
    let p = home.join(".claude").join("settings.json");
    let mut v: serde_json::Value = if fs.exists(&p) {
        serde_json::from_slice(&fs.read(&p)?).map_err(|e| CpmError::UnrecognizedFormat(e.to_string()))?
    } else { serde_json::json!({}) };
    let cmd = format!("{cpm_bin} archive --session \"$TRANSCRIPT_PATH\" --archive-dir \"{}\"",
                      archive_dir.display());
    let hook = serde_json::json!({ "matcher": "*", "hooks": [ { "type": "command", "command": cmd } ] });
    let arr = v.pointer_mut("/hooks/SessionEnd");
    match arr {
        Some(serde_json::Value::Array(a)) => a.push(hook),
        _ => {
            if !v["hooks"].is_object() { v["hooks"] = serde_json::json!({}); }
            v["hooks"]["SessionEnd"] = serde_json::json!([hook]);
        }
    }
    fs.write(&p, serde_json::to_vec_pretty(&v).unwrap().as_slice())?;
    Ok(())
}

pub fn uninstall_session_end_hook(fs: &dyn FileSystem, home: &Path) -> Result<()> {
    let p = home.join(".claude").join("settings.json");
    if !fs.exists(&p) { return Ok(()); }
    let mut v: serde_json::Value =
        serde_json::from_slice(&fs.read(&p)?).map_err(|e| CpmError::UnrecognizedFormat(e.to_string()))?;
    if let Some(serde_json::Value::Array(a)) = v.pointer_mut("/hooks/SessionEnd") {
        a.retain(|h| !h.to_string().contains("cpm archive --session"));
    }
    fs.write(&p, serde_json::to_vec_pretty(&v).unwrap().as_slice())?;
    Ok(())
}
```
Note: settings.json is CPM-owned CONFIG (not a byte-preserve store like transcripts),
so serialize-back is acceptable here. The uninstall removes only CPM's hook entry,
leaving the user's other hooks intact (asserted by a test).

- [ ] **Step 3: Wire `cpm archive` CLI**

Add `Cmd::Archive` with flags `--archive-dir <path>`, `--session <path>`,
`--install-hook`, `--uninstall-hook`, `--set-retention <days>`, `--force-zero`,
`--render`. Dispatch: `--session` calls `archive_session`; `--install-hook` resolves
the current exe path (`std::env::current_exe()`) and calls `install_session_end_hook`;
`--set-retention` calls `set_retention` and prints the caveat; otherwise `archive_all`.
Warn if `--archive-dir` resolves under a cloud-sync root (check `OneDrive`,
`OneDriveConsumer`, `OneDriveCommercial`, `Dropbox` env vars, segment-boundary match).

- [ ] **Step 4: Run, then commit**

Run: `cargo test -p cpm-core settings::tests archive::tests`
Expected: PASS.
```bash
git add crates/cpm-core/src/settings.rs crates/cpm-cli/src/main.rs
git commit -m "feat: SessionEnd hook install, safe retention setting, archive CLI (F14)"
```

---

## Phase 15: `cpm associate --from --to` (F15)

Spec: F15, AC-40..44. Reuses the phase 5-9 write path (minus MoveTree) and phase-14
archive writer.

### Task 15.1: associate engine (re-associate and/or export)

**Files:**
- Create: `crates/cpm-core/src/associate.rs`; modify `cpm-cli/src/main.rs`

**Interfaces:**
- Consumes: `build_plan` (a variant that omits `MoveTree`), `apply_verified`,
  `archive_all`/`archive_session`.
- Produces:
  ```rust
  pub struct AssociateOpts {
      pub reassociate: bool, pub export: bool, pub export_subdir: String,
      pub run_id: String, pub on_collision: crate::plan::Collision,
  }
  pub fn associate(fs: &dyn FileSystem, home: &Path, from: &str, to: &str, opts: &AssociateOpts) -> Result<crate::report::Report>;
  ```
  `build_plan` gains a `move_folder: bool` field on `PlanOpts` (default true for
  `cpm move`; associate passes false so no `MoveTree` change is emitted). Everything
  else - dir rename, transcript/cwd rewrite, claude.json, githubRepoPaths, history,
  plugin dir - runs identically. Export copies `from`'s sessions into
  `to/<export_subdir>` via the archive writer.

- [ ] **Step 1: Write the failing test (export-only leaves records untouched)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, MemoryFileSystem};
    use std::path::Path;

    #[test]
    fn export_only_copies_but_does_not_reassociate() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("/h/.claude/projects/E--A/s.jsonl"),
                 b"{\"cwd\":\"E:\\\\A\"}\n").unwrap();
        fs.write(Path::new("E:/B/keep.txt"), b"x").unwrap();     // dest folder exists
        let opts = AssociateOpts { reassociate: false, export: true,
            export_subdir: ".claude-sessions".into(), run_id: "T".into(),
            on_collision: crate::plan::Collision::Refuse };
        associate(&fs, Path::new("/h"), "E:\\A", "E:\\B", &opts).unwrap();
        // original records untouched
        assert!(fs.exists(Path::new("/h/.claude/projects/E--A/s.jsonl")));
        // export copy present under B/.claude-sessions
        assert!(fs.exists(Path::new("E:/B/.claude-sessions/projects/E--A/s.jsonl")));
    }
}
```

- [ ] **Step 2: Run to verify failure, then implement**

```rust
use crate::archive::{archive_all, ArchiveOpts};
use crate::error::{CpmError, Result};
use crate::fs::FileSystem;
use crate::model::Move;
use crate::plan::{build_plan, Collision, PlanOpts};
use crate::apply::{apply_verified, ApplyOpts};
use std::path::{Path, PathBuf};

pub struct AssociateOpts {
    pub reassociate: bool, pub export: bool, pub export_subdir: String,
    pub run_id: String, pub on_collision: Collision,
}

pub fn associate(fs: &dyn FileSystem, home: &Path, from: &str, to: &str,
                 opts: &AssociateOpts) -> Result<crate::report::Report> {
    if !opts.reassociate && !opts.export {
        return Err(CpmError::Locked("nothing to do: enable --reassociate or --export".into()));
    }
    // export first (read-only w.r.t. Claude's live records)
    if opts.export {
        let sub = format!("{}/{}", to.replace('\\', "/"), opts.export_subdir);
        let aopts = ArchiveOpts { archive_dir: PathBuf::from(sub), render: false };
        // archive only `from`'s sessions: build the reverse index, find from's dir, copy it.
        // (reuse archive_all filtered to the from project, or a from-scoped archive fn.)
        let _ = archive_all(fs, home, &aopts)?;   // v1: full archive; a from-scoped filter is a follow-up
    }
    if opts.reassociate {
        let mv = Move { src_abs: from.to_string(), dst_abs: to.to_string() };
        let opts_plan = PlanOpts { recursive: false, on_collision: match opts.on_collision {
            Collision::Refuse => Collision::Refuse, Collision::KeepDest => Collision::KeepDest,
            Collision::KeepSrc => Collision::KeepSrc }, force: false, move_folder: false,
            scope: crate::model::Scope::Standard };
        let plan = build_plan(fs, home, &mv, &opts_plan)?;
        let aopts = ApplyOpts { run_id: opts.run_id.clone(), auto_rollback: true, force: false };
        return apply_verified(&plan, fs, &std::env::temp_dir(), &aopts);
    }
    Ok(crate::report::Report { run_id: opts.run_id.clone(), applied: vec![],
        backup_dir: String::new(), verify: None })
}
```
Implementation notes (not placeholders): (a) add `move_folder: bool` to `PlanOpts`
and gate the final `Change::MoveTree` push in `build_plan` on it; existing `cpm move`
call sites set it true. (b) The export currently archives all projects; a
from-scoped archive (filter `archive_all` to the one project dir) is a small
follow-up and should be done before shipping so export copies only `from`'s sessions.
Add a test asserting only `from`'s sessions land under the subdir.

- [ ] **Step 3: Wire `cpm associate` CLI**

Add `Cmd::Associate { #[arg(long)] from: String, #[arg(long)] to: String,
#[arg(long)] export_subdir: Option<String>, #[arg(long)] no_reassociate: bool,
#[arg(long)] no_export: bool }`. Default both modes on; map `--no-*` to disable.
Error if both disabled.

- [ ] **Step 4: Run, then commit**

Run: `cargo test -p cpm-core associate::tests`
Expected: PASS.
```bash
git add crates/cpm-core/src/associate.rs crates/cpm-core/src/plan.rs crates/cpm-cli/src/main.rs
git commit -m "feat: cpm associate (re-associate and/or export) (F15)"
```

### Task 15.2: gone-folder fixture and end-to-end associate test

**Files:**
- Create: `crates/cpm-core/tests/associate_gone_folder.rs`

**Interfaces:**
- Consumes: `associate`. Proves the source folder need not exist.

- [ ] **Step 1: Write the test using a synthetic gone-folder project**

```rust
mod fixtures;
use cpm_core::associate::{associate, AssociateOpts};
use cpm_core::fs::{FileSystem, MemoryFileSystem};
use cpm_core::plan::Collision;
use std::path::Path;

#[test]
fn associate_finds_sessions_when_source_folder_is_gone() {
    let fs = MemoryFileSystem::new();
    // project dir exists in ~/.claude but the source folder does NOT exist on disk
    fs.write(Path::new("/h/.claude/projects/E--Old-A/s.jsonl"),
             b"{\"cwd\":\"E:\\\\Old\\\\A\"}\n").unwrap();
    fs.write(Path::new("E:/New/B/keep.txt"), b"x").unwrap();
    let opts = AssociateOpts { reassociate: true, export: true,
        export_subdir: ".claude-sessions".into(), run_id: "T".into(),
        on_collision: Collision::Refuse };
    associate(&fs, Path::new("/h"), "E:\\Old\\A", "E:\\New\\B", &opts).unwrap();
    // reassociated: new encoded dir exists
    assert!(fs.exists(Path::new("/h/.claude/projects/E--New-B/s.jsonl")));
    // exported copy present
    assert!(fs.exists(Path::new("E:/New/B/.claude-sessions/projects/E--New-B/s.jsonl"))
         || fs.exists(Path::new("E:/New/B/.claude-sessions/projects/E--Old-A/s.jsonl")));
}
```

- [ ] **Step 2: Run, then commit**

Run: `cargo test -p cpm-core --test associate_gone_folder`
Expected: PASS.
```bash
git add crates/cpm-core/tests/associate_gone_folder.rs
git commit -m "test: associate works when source folder is already deleted (F15)"
```

---

## Deferred (post-v1, not in this plan)

- **Phase 10** Cross-volume move (copy + sha256 verify + delete). AC-2.
- **Phase 11** Codex + Gemini adapters behind `--codex`/`--gemini`. Confirm Codex trust storage first.
- **Phase 12** Tauri + React GUI over the identical `cpm-core`, with a `plan --json` parity test.

---

## Self-Review

Run after the plan is written. Findings and fixes are recorded here.

**1. Spec coverage** (against `docs/DESIGN.md`):
- Encoding rule (corrected) -> Task 2.1. Reverse index -> Task 2.2. Store trait + 6 adapters -> Phase 3. doctor/scan -> Phase 4. Anchored rewrite + golden counts -> Phase 5. plan + guards + collision + nested -> Phase 6. backup + apply + folder-last -> Phase 7. verify + auto-rollback + idempotency + hard-fail + lock -> Phase 8. rollback + CLI + exit codes -> Phase 9. githubRepoPaths -> Tasks 3.3/6.1. plugin state hash -> Tasks 3.4/6.1. sweep report-only -> Task 3.5. Scope tiers -> noted in Task 6.1 (Standard default; Full deferred to a flag). Cross-volume/Codex/Gemini/GUI -> Deferred section. All DESIGN sections map to a task.

**2. Placeholder scan:** no "TBD/TODO/implement later" in step bodies. Two tasks (7.1 snapshot dir mechanics, 9.1 dir-restore) carry explicit implementation notes rather than placeholders because the exact mechanic depends on a documented earlier gate (snapshot-old-dir-wholesale); the note states the decision, not a deferral.

**3. Type consistency:** `Move`, `Ctx`, `Hit`, `Stale`, `Change`, `Applied`, `VerifyResult`, `Store`, `RewriteRule`, `Plan`, `PlanOpts`, `Collision`, `ApplyOpts`, `Manifest`, `Report` are each defined once (model.rs / rewrite.rs / plan.rs / backup.rs / report.rs / apply.rs) and referenced consistently. One mechanical addition is called out in Task 8.2: add `home: PathBuf` to `Plan` (used by `apply_verified` and rollback) - construction in `build_plan` and any earlier test that builds a `Plan` literal must include it. Adapter method names (`probe`/`detect`/`audit`/`plan`/`verify`) are uniform across all five stores.

### Self-review addendum: v1.1 phases 13-15 (F13-F15)

**Spec coverage** (against `docs/features/v1.1-inventory-retention-reassociate.md`):
- F13 inventory: AC-28..33 -> Phase 13 (`SessionFootprint` Task 13.1, `list` + renderers Task 13.2). Terminal/json/html all map.
- F14 retention: AC-34..39 -> Phase 14 (content-hash archive Task 14.1; SessionEnd hook + `set_retention` + CLI Task 14.2). The `0`-refusal (AC-37) and cloud-sync warning (AC-38) are explicit steps.
- F15 associate: AC-40..44 -> Phase 15 (associate engine Task 15.1; gone-folder test Task 15.2). Gone-folder (AC-40) has a dedicated test.

**New cross-task mechanical changes** (called out so an implementer does not miss them):
1. `FileSystem` gains `mtime_secs` (Task 13.2 Step 1); both impls and the `MemoryFileSystem` value type update accordingly. This precedes any age computation.
2. `PlanOpts` gains `move_folder: bool` (Task 15.1); `build_plan` gates the final `Change::MoveTree` push on it; existing `cpm move` call sites set it `true`, associate sets it `false`. Update the Phase 6 `PlanOpts` literal and its tests when Phase 15 lands.
3. `Ctx` and `PlanOpts` gain `scope: Scope` (Task 3.1 / Task 6.2, default Standard); `build_plan` copies `opts.scope` into `Ctx`; `claude_projects::plan` gates transcript rewrites on `scope >= Standard` and sidecars on `Full`. Every `PlanOpts` and `Ctx` literal (including tests) carries `scope`.
4. `ProjectIndex` gains `cwds: Vec<String>` (Task 2.2), each ORIGINAL stored cwd; populated in `build()` and consumed by `plugin_state::audit`.
5. Top-level `verify` gains a `manifest: Option<&Manifest>` parameter (Task 8.1); `apply_verified` passes `Some(&manifest)`, standalone `cpm verify` passes `None`. `Plan` gains `home: PathBuf` (Task 8.2).

**Two follow-ups flagged inside tasks (decisions, not placeholders):** Task 14.1's SESSION-keyed copy loop (specified in-comment: iterate `footprint().session_ids`) and Task 15.1's from-scoped export filter must both be completed before F14/F15 ship; each has a note stating the exact mechanic and a required test.

**Type consistency (v1.1):** `SessionFootprint`, `ProjectRecord`, `Health`, `ArchiveOpts`, `ArchiveReport`, `AssociateOpts` are each defined once (sessions.rs / list.rs / archive.rs / associate.rs). `Collision` and `Report` are reused from the mover, not redefined.

### Audit-repair pass (2026-07-11)

Applied the design-stage audit's plan-code fixes. Source: `_local/audit/2026-07-10_fable-audit/AUDIT_REPORT.md` (local-only, not committed). Findings addressed in this plan:

- **B-01** (hollow backup) - `snapshot` now wholesale-copies every pre-rename `*.jsonl` under each renamed dir, with a red test asserting the transcript and its sha256 land in the manifest.
- **LEAD-03** (history variants) - `claude_history::plan` emits one rule per distinct stored `project` form via `dst_key`; variant test added.
- **LEAD-04** (plugin rename + audit stub) - plugin dir named from the DESTINATION basename; `plugin_state::audit` implemented against `ProjectIndex.cwds`; audit test using `e854827f52137cd9`.
- **LEAD-09** (fixture privacy) - Task 1.3 gains a sanitize-and-minimize step (credential redaction, synthetic `claude.json`/`state.json`, `test/fixtures/README.md`).
- **LEAD-01** (mid-apply rollback) - `apply_verified` rolls back on apply error and names the backup dir in every failure message.
- **A-01** (rollback sha) - `rollback` sha256-checks each backup before restoring.
- **LEAD-08** (verify backup compare) - `verify` takes `Option<&Manifest>`; the line-count-vs-backup postcondition runs only when supplied.
- **LEAD-02** (lossy UTF-8) - the write path hard-fails with `UnrecognizedFormat` on invalid UTF-8; read-only `read_stored_cwd` documents why it stays lossy.
- **B-02** (fs-bypass audits) - `claude_json`/`claude_history` audits route existence through `ctx.fs`; MemoryFileSystem test added.
- **LEAD-06** (compile hygiene) - `vec!` syntax, snapshot borrow (plain helper fn), and the CLI `run()` wrapper for `?`.
- **B-05** (scope tiers) - `Scope { Minimal, Standard, Full }` on `Ctx`/`PlanOpts`, gated in `claude_projects::plan`, wired to `--scope`.
- **E-03 + A-07** (CI) - no-network crate gate and `cargo audit` step.
- **LEAD-05** (archive coverage) - archive covers unresolved dirs, session-artifacts, `<sessionId>/` subdirs, and a real `manifest.json`/`INDEX.md`.
- **LEAD-10** (F13 counts) - `list` wires real PATH-keyed counts via adapter `detect`; AC-31 test.

Companion DESIGN.md edits: B-06 (Store trait), C-03/E-01 (platform scope), F-02 (provenance), B-04 (exit 1).

