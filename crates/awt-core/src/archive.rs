use crate::error::{AwtError, Result};
use crate::fs::FileSystem;
use crate::index::ProjectIndex;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub struct ArchiveOpts {
    pub archive_dir: PathBuf,
    pub render: bool,
    /// Per-run token used to name the sibling temp file during manifest writes.
    /// Must be unique per concurrent archive process to reduce collision risk.
    pub run_token: String,
}

/// One file the sweep could not archive, with the reason the filesystem gave.
#[derive(Debug, Clone)]
pub struct ArchiveFailure {
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct ArchiveReport {
    pub copied: usize,
    pub skipped: usize,
    /// Files that could not be read or written. They are not in the manifest. The sweep
    /// continues past them (AR-09): one locked file of a live session must not stop the
    /// protection of every other session.
    pub failed: usize,
    pub failures: Vec<ArchiveFailure>,
}

struct ArchEntry {
    src: PathBuf,
    dst: PathBuf,
    sha256: String,
}

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

/// Lowercase, forward-slash form used for case-insensitive path-prefix comparison.
fn path_key(p: &Path) -> String {
    p.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase()
}

/// True when `child` is `root` or lies under it (case-insensitive, separator-insensitive).
fn is_under(child: &Path, root_key: &str) -> bool {
    if root_key.is_empty() {
        return false;
    }
    let c = path_key(child);
    c == root_key || c.starts_with(&format!("{root_key}/"))
}

/// Store folders that hold one subfolder per session id, copied under `session-artifacts/`.
const SESSION_DIR_STORES: [&str; 3] = ["file-history", "session-env", "tasks"];

/// Mutable state of one archive run: the manifest entries, the report, and the filesystem.
struct Sweep<'a> {
    fs: &'a dyn FileSystem,
    opts: &'a ArchiveOpts,
    archive_key: String,
    entries: Vec<ArchEntry>,
    report: ArchiveReport,
}

impl<'a> Sweep<'a> {
    fn new(fs: &'a dyn FileSystem, opts: &'a ArchiveOpts) -> Self {
        Sweep {
            fs,
            opts,
            archive_key: path_key(&opts.archive_dir),
            entries: Vec::new(),
            report: ArchiveReport::default(),
        }
    }

    fn fail(&mut self, path: &Path, reason: String) {
        self.report.failed += 1;
        self.report.failures.push(ArchiveFailure {
            path: path.to_path_buf(),
            reason,
        });
    }

    /// Every FILE under `dir`. Never follows a junction or symlink (AC-61), and never
    /// descends into the archive folder itself. A directory that cannot be listed is
    /// recorded as a failure instead of silently shrinking the sweep.
    fn walk(&mut self, dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let children = match self.fs.read_dir(dir) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return out,
            Err(e) => {
                self.fail(dir, format!("cannot list folder: {e}"));
                return out;
            }
        };
        for child in children {
            // AC-61: archive is a best-effort protective sweep across every project, so a
            // link is SKIPPED rather than refused. A link inside a project-state tree could
            // pull arbitrary outside content into the archive, or recurse forever.
            if self.fs.is_reparse_point(&child) || is_under(&child, &self.archive_key) {
                continue;
            }
            if self.fs.is_dir(&child) {
                out.extend(self.walk(&child));
            } else {
                out.push(child);
            }
        }
        out
    }

    /// Copy `src` to `dst` unless `dst` already holds the same bytes. A read or write failure
    /// is recorded and the file is left out of the manifest. The destination is written to a
    /// sibling temp file and renamed into place, so a failed write never leaves a torn file
    /// that a later run or reader would take for a good copy.
    fn copy(&mut self, src: &Path, dst: &Path) {
        if is_under(src, &self.archive_key) {
            return;
        }
        let bytes = match self.fs.read(src) {
            Ok(b) => b,
            Err(e) => {
                self.fail(src, format!("cannot read: {e}"));
                return;
            }
        };
        let src_hash = sha(&bytes);
        let entry = ArchEntry {
            src: src.to_path_buf(),
            dst: dst.to_path_buf(),
            sha256: src_hash.clone(),
        };
        if self.fs.exists(dst) {
            if let Ok(dst_bytes) = self.fs.read(dst) {
                if sha(&dst_bytes) == src_hash {
                    // Recorded regardless of copy-vs-skip, so a no-change rerun never
                    // produces an empty or incomplete manifest.
                    self.entries.push(entry);
                    self.report.skipped += 1;
                    return;
                }
            }
        }
        let mut tmp_name = dst.file_name().unwrap_or_default().to_os_string();
        tmp_name.push(format!(".{}.awt-tmp", self.opts.run_token));
        let tmp = dst.with_file_name(tmp_name);
        let written = self
            .fs
            .write(&tmp, &bytes)
            .and_then(|_| self.fs.rename(&tmp, dst));
        if let Err(e) = written {
            let _ = self.fs.remove_file(&tmp);
            self.fail(src, format!("cannot write {}: {e}", dst.display()));
            return;
        }
        self.entries.push(entry);
        self.report.copied += 1;
    }

    /// Copy every file under `tree` to `dst_base/<path relative to rel_root>`.
    fn copy_tree(&mut self, rel_root: &Path, tree: &Path, dst_base: &Path) {
        for src in self.walk(tree) {
            let rel = src.strip_prefix(rel_root).unwrap_or(&src);
            let dst = dst_base.join(rel);
            self.copy(&src, &dst);
        }
    }

    /// Copy the session-keyed stores for each session id: `file-history/<sid>/`,
    /// `session-env/<sid>/`, `tasks/<sid>/`, and the `todos/` files whose name contains the
    /// id (the same `contains` match `sessions::footprint` uses, so `awt list` and the
    /// archive agree).
    fn copy_stores(&mut self, claude_root: &Path, session_ids: &[String]) {
        let base = self.opts.archive_dir.join("session-artifacts");
        let todos_dir = claude_root.join("todos");
        let todos = self.fs.read_dir(&todos_dir).unwrap_or_default();
        for sid in session_ids {
            if sid.is_empty() {
                continue;
            }
            for store in SESSION_DIR_STORES {
                let dir = claude_root.join(store).join(sid);
                if self.fs.is_dir(&dir) && !self.fs.is_reparse_point(&dir) {
                    self.copy_tree(claude_root, &dir, &base);
                }
            }
            for entry in &todos {
                let matches = entry
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.contains(sid.as_str()))
                    .unwrap_or(false);
                if !matches || self.fs.is_reparse_point(entry) {
                    continue;
                }
                if self.fs.is_dir(entry) {
                    self.copy_tree(claude_root, entry, &base);
                } else {
                    let rel = entry.strip_prefix(claude_root).unwrap_or(entry);
                    self.copy(entry, &base.join(rel));
                }
            }
        }
    }

    /// Archive one project folder: top-level transcripts, every session subfolder except
    /// `memory`, and the session-keyed stores of every session id found by either route.
    fn project_dir(&mut self, home: &Path, project_dir: &Path) {
        let dir_rel = project_rel(home, project_dir);
        let dst_base = self.opts.archive_dir.join("projects").join(&dir_rel);
        let children = match self.fs.read_dir(project_dir) {
            Ok(c) => c,
            Err(e) => {
                self.fail(project_dir, format!("cannot list folder: {e}"));
                return;
            }
        };
        let mut session_ids: Vec<String> = Vec::new();
        for child in children {
            if self.fs.is_reparse_point(&child) || is_under(&child, &self.archive_key) {
                continue;
            }
            let Some(name) = child.file_name().map(|n| n.to_os_string()) else {
                continue;
            };
            if self.fs.is_dir(&child) {
                // `memory/` is project notes, not session history. Every other subfolder is
                // a session folder (subagents, tool-results, workflows), archived even when
                // no transcript sits beside it.
                if name.to_string_lossy().eq_ignore_ascii_case("memory") {
                    continue;
                }
                session_ids.push(name.to_string_lossy().into_owned());
                self.copy_tree(project_dir, &child, &dst_base);
            } else if child.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                if let Some(stem) = child.file_stem().and_then(|s| s.to_str()) {
                    session_ids.push(stem.to_string());
                }
                self.copy(&child, &dst_base.join(&name));
            }
        }
        session_ids.sort();
        session_ids.dedup();
        self.copy_stores(&home.join(".claude"), &session_ids);
    }

    /// Write the manifest (and optionally the index) and hand back the report.
    fn finish(self, write_index_file: bool) -> Result<ArchiveReport> {
        let manifest_path = self.opts.archive_dir.join("manifest.json");
        write_manifest(self.fs, &manifest_path, &self.entries, &self.opts.run_token)?;
        if write_index_file {
            write_index(self.fs, &self.opts.archive_dir.join("index.json"))?;
        }
        Ok(self.report)
    }
}

fn project_rel(home: &Path, project_dir: &Path) -> PathBuf {
    let projects_root = home.join(".claude").join("projects");
    project_dir
        .strip_prefix(&projects_root)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|_| PathBuf::from(project_dir.file_name().unwrap_or_default()))
}

/// Write (merge) manifest entries to `path`. Uses `run_token` to name the sibling
/// temp file (`manifest.json.<run_token>.tmp`) to reduce same-archive collision risk
/// when two archive processes run concurrently against the same archive directory.
/// Note: the residual read-merge-write lost-update race is a documented v1.x follow-up (needs a lock).
fn write_manifest(
    fs: &dyn FileSystem,
    path: &Path,
    entries: &[ArchEntry],
    run_token: &str,
) -> Result<()> {
    // Read any existing manifest and build a keyed map so this run merges rather
    // than overwrites. A no-change rerun (all files skipped) still records every
    // file; a single-session archive_session call preserves other sessions' entries.
    let mut file_map: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();
    if let Ok(existing_bytes) = fs.read(path) {
        if let Ok(existing) = serde_json::from_slice::<serde_json::Value>(&existing_bytes) {
            if let Some(arr) = existing["files"].as_array() {
                for entry in arr {
                    if let Some(src) = entry["src"].as_str() {
                        file_map.insert(src.to_string(), entry.clone());
                    }
                }
            }
        }
    }
    // Upsert this run's entries; newer data wins for any given source path.
    for e in entries {
        let src_str = e.src.to_string_lossy().into_owned();
        file_map.insert(
            src_str.clone(),
            serde_json::json!({
                "src": src_str,
                "dst": e.dst.to_string_lossy(),
                "sha256": e.sha256,
            }),
        );
    }
    let files: Vec<_> = file_map.into_values().collect();
    let json = serde_json::json!({ "files": files });
    // Atomic write: write to a token-named sibling temp file, then rename over the final path.
    let tmp_path = path.with_file_name(format!("manifest.json.{run_token}.tmp"));
    fs.write(
        &tmp_path,
        serde_json::to_vec_pretty(&json).unwrap().as_slice(),
    )?;
    fs.rename(&tmp_path, path)?;
    Ok(())
}

fn write_index(fs: &dyn FileSystem, path: &Path) -> Result<()> {
    let json = serde_json::json!({ "version": 1 });
    fs.write(path, serde_json::to_vec_pretty(&json).unwrap().as_slice())?;
    Ok(())
}

/// Archive the project(s) whose recorded cwd resolves to `from_abs` via the
/// `ProjectIndex` reverse lookup. Returns an error rather than silently exporting
/// nothing when no Claude state is found for that path.
///
/// Callers: `associate --export` (through `associate::associate`) and tests. The CLI's own
/// `archive` command does not call it.
pub fn archive_project(
    fs: &dyn FileSystem,
    home: &Path,
    from_abs: &str,
    opts: &ArchiveOpts,
) -> Result<ArchiveReport> {
    use crate::paths::normalize_path;
    let index = ProjectIndex::build(fs, home)?;
    let key = normalize_path(from_abs);
    let dirs = index.by_cwd.get(&key).ok_or_else(|| {
        // AR-08: guard refusal (exit 2), matching associate; see associate.rs.
        AwtError::Locked(format!(
            "no Claude state found for project '{from_abs}'; run 'awt list' to see known projects"
        ))
    })?;
    fs.create_dir_all(&opts.archive_dir)?;
    let mut sweep = Sweep::new(fs, opts);
    for dir in dirs {
        sweep.project_dir(home, dir);
    }
    sweep.finish(true)
}

/// Archive a single session: its transcript, its session folder
/// (`projects/<enc>/<sid>/`), and its session-keyed stores. A transcript that does not
/// exist is an error; a transcript that exists but cannot be read is a recorded failure.
pub fn archive_session(
    fs: &dyn FileSystem,
    home: &Path,
    transcript: &Path,
    opts: &ArchiveOpts,
) -> Result<ArchiveReport> {
    if !fs.exists(transcript) {
        return Err(AwtError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("transcript not found: {}", transcript.display()),
        )));
    }
    fs.create_dir_all(&opts.archive_dir)?;
    let project_dir = transcript.parent().unwrap_or(Path::new(""));
    let dst_base = opts
        .archive_dir
        .join("projects")
        .join(project_rel(home, project_dir));
    let mut sweep = Sweep::new(fs, opts);

    if let Some(fname) = transcript.file_name() {
        if !fs.is_reparse_point(transcript) {
            sweep.copy(transcript, &dst_base.join(fname));
        }
    }

    if let Some(session_id) = transcript.file_stem().and_then(|s| s.to_str()) {
        if !session_id.is_empty() {
            let session_dir = project_dir.join(session_id);
            if fs.is_dir(&session_dir) && !fs.is_reparse_point(&session_dir) {
                sweep.copy_tree(project_dir, &session_dir, &dst_base);
            }
            sweep.copy_stores(&home.join(".claude"), &[session_id.to_string()]);
        }
    }

    sweep.finish(false)
}

/// Archive all Claude project session state to the archive directory.
/// Covers resolved, unresolved, and ambiguous project dirs so nothing is silently dropped.
pub fn archive_all(fs: &dyn FileSystem, home: &Path, opts: &ArchiveOpts) -> Result<ArchiveReport> {
    let index = ProjectIndex::build(fs, home)?;
    fs.create_dir_all(&opts.archive_dir)?;
    let mut sweep = Sweep::new(fs, opts);

    let all_dirs: Vec<&PathBuf> = index
        .by_cwd
        .values()
        .flatten()
        .chain(index.unresolved.iter())
        .chain(index.ambiguous.iter())
        .collect();

    for dir in all_dirs {
        sweep.project_dir(home, dir);
    }
    sweep.finish(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, MemoryFileSystem};
    use sha2::{Digest, Sha256};
    use std::path::Path;

    // --- Finding 2 regression tests ---

    /// A no-change rerun (all files skipped) must leave the manifest populated, not empty.
    #[test]
    fn archive_all_rerun_preserves_manifest() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--A/s.jsonl"),
            b"{\"cwd\":\"E:\\\\A\"}\n",
        )
        .unwrap();
        let opts = ArchiveOpts {
            archive_dir: Path::new("/arch").to_path_buf(),
            render: false,
            run_token: "test".into(),
        };
        // First run: file is copied; manifest must record it.
        let r1 = archive_all(&fs, Path::new("/h"), &opts).unwrap();
        assert_eq!(r1.copied, 1);
        // Second run: file is unchanged, so it is skipped; manifest must still list it.
        let r2 = archive_all(&fs, Path::new("/h"), &opts).unwrap();
        assert_eq!(r2.copied, 0, "second run should skip unchanged file");
        assert_eq!(r2.skipped, 1);
        let m: serde_json::Value =
            serde_json::from_slice(&fs.read(Path::new("/arch/manifest.json")).unwrap()).unwrap();
        let files = m["files"].as_array().unwrap();
        assert!(
            !files.is_empty(),
            "manifest must not be empty after a no-change rerun"
        );
        assert!(
            files
                .iter()
                .any(|e| e["src"].as_str().unwrap_or("").contains("s.jsonl")),
            "manifest must still list the transcript after a no-change rerun"
        );
    }

    /// archive_session for a new transcript must not discard entries from earlier runs.
    #[test]
    fn archive_session_merges_not_overwrites() {
        let fs = MemoryFileSystem::new();
        let opts = ArchiveOpts {
            archive_dir: Path::new("/arch").to_path_buf(),
            render: false,
            run_token: "test".into(),
        };

        // Seed only project A and run archive_all so the manifest has A's entry.
        fs.write(
            Path::new("/h/.claude/projects/E--A/a.jsonl"),
            b"{\"cwd\":\"E:\\\\A\"}\n",
        )
        .unwrap();
        archive_all(&fs, Path::new("/h"), &opts).unwrap();

        let m1: serde_json::Value =
            serde_json::from_slice(&fs.read(Path::new("/arch/manifest.json")).unwrap()).unwrap();
        assert_eq!(
            m1["files"].as_array().unwrap().len(),
            1,
            "manifest should have 1 entry after archiving A"
        );

        // Now add project B and archive just that session into the same archive_dir.
        fs.write(
            Path::new("/h/.claude/projects/E--B/b.jsonl"),
            b"{\"cwd\":\"E:\\\\B\"}\n",
        )
        .unwrap();
        archive_session(
            &fs,
            Path::new("/h"),
            Path::new("/h/.claude/projects/E--B/b.jsonl"),
            &opts,
        )
        .unwrap();

        // Both A and B must appear in the manifest.
        let m2: serde_json::Value =
            serde_json::from_slice(&fs.read(Path::new("/arch/manifest.json")).unwrap()).unwrap();
        let files = m2["files"].as_array().unwrap();
        assert_eq!(
            files.len(),
            2,
            "manifest should have 2 entries after archiving B"
        );
        assert!(
            files
                .iter()
                .any(|e| e["src"].as_str().unwrap_or("").contains("a.jsonl")),
            "manifest must still contain A's entry after archiving B"
        );
        assert!(
            files
                .iter()
                .any(|e| e["src"].as_str().unwrap_or("").contains("b.jsonl")),
            "manifest must contain B's entry"
        );
    }

    #[test]
    fn archive_is_content_hash_incremental() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--A/s.jsonl"),
            b"{\"cwd\":\"E:\\\\A\"}\n",
        )
        .unwrap();
        let opts = ArchiveOpts {
            archive_dir: Path::new("/arch").to_path_buf(),
            render: false,
            run_token: "test".into(),
        };
        let r1 = archive_all(&fs, Path::new("/h"), &opts).unwrap();
        assert_eq!(r1.copied, 1);
        let r2 = archive_all(&fs, Path::new("/h"), &opts).unwrap();
        assert_eq!(r2.copied, 0);
        assert_eq!(r2.skipped, 1);
    }

    #[test]
    fn archives_unresolved_dir_transcript() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--Ghost/s.jsonl"),
            b"{\"type\":\"x\"}\n",
        )
        .unwrap();
        let opts = ArchiveOpts {
            archive_dir: Path::new("/arch").to_path_buf(),
            render: false,
            run_token: "test".into(),
        };
        archive_all(&fs, Path::new("/h"), &opts).unwrap();
        assert!(fs.exists(Path::new("/arch/projects/E--Ghost/s.jsonl")));
    }

    // --- Finding 2 tests: archive_project uses ProjectIndex ---

    /// archive_project resolves the project dir via the reverse index (not encode_project_dir),
    /// so it finds the correct dir even when the encoding would differ.
    #[test]
    fn archive_project_resolves_via_index() {
        let fs = MemoryFileSystem::new();
        // Transcript has a cwd that normalizes to "e:/a"
        fs.write(
            Path::new("/h/.claude/projects/E--A/s.jsonl"),
            b"{\"cwd\":\"E:\\\\A\"}\n",
        )
        .unwrap();
        let opts = ArchiveOpts {
            archive_dir: Path::new("/arch").to_path_buf(),
            render: false,
            run_token: "test".into(),
        };
        let r = archive_project(&fs, Path::new("/h"), "E:\\A", &opts).unwrap();
        assert!(r.copied >= 1, "archive_project must copy at least one file");
        assert!(
            fs.exists(Path::new("/arch/projects/E--A/s.jsonl")),
            "archived transcript must be present"
        );
    }

    /// archive_project returns an error when no Claude state resolves to the given path,
    /// rather than silently writing an empty archive.
    #[test]
    fn archive_project_errors_for_unknown_project() {
        let fs = MemoryFileSystem::new();
        // No transcripts in the projects dir
        let opts = ArchiveOpts {
            archive_dir: Path::new("/arch").to_path_buf(),
            render: false,
            run_token: "test".into(),
        };
        let err = archive_project(&fs, Path::new("/h"), "E:\\NoSuchProject", &opts).unwrap_err();
        // AR-08: a missing project is a guard refusal (exit-2 class), not a format error.
        assert!(
            matches!(err, crate::error::AwtError::Locked(_)),
            "expected Locked, got {err:?}"
        );
    }

    // --- Finding 4 test: run_token is used for the temp file name ---

    /// After archive_all completes, the final manifest.json exists and the
    /// old fixed-name temp file ("manifest.json.tmp") does NOT exist, confirming
    /// the token-based temp name is used instead.
    #[test]
    fn archive_uses_run_token_in_temp_name() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--A/s.jsonl"),
            b"{\"cwd\":\"E:\\\\A\"}\n",
        )
        .unwrap();
        let opts = ArchiveOpts {
            archive_dir: Path::new("/arch").to_path_buf(),
            render: false,
            run_token: "tok42".into(),
        };
        archive_all(&fs, Path::new("/h"), &opts).unwrap();
        assert!(
            fs.exists(Path::new("/arch/manifest.json")),
            "final manifest must exist"
        );
        assert!(
            !fs.exists(Path::new("/arch/manifest.json.tmp")),
            "old fixed-name temp must not exist"
        );
        assert!(
            !fs.exists(Path::new("/arch/manifest.json.tok42.tmp")),
            "token-named temp must be cleaned up by rename"
        );
    }

    #[test]
    fn archives_file_history_under_session_artifacts_and_records_sha() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--A/28fd093e.jsonl"),
            b"{\"cwd\":\"E:\\\\A\"}\n",
        )
        .unwrap();
        let payload = b"file history payload";
        fs.write(Path::new("/h/.claude/file-history/28fd093e/x@v1"), payload)
            .unwrap();
        let opts = ArchiveOpts {
            archive_dir: Path::new("/arch").to_path_buf(),
            render: false,
            run_token: "test".into(),
        };
        archive_all(&fs, Path::new("/h"), &opts).unwrap();
        assert!(fs.exists(Path::new(
            "/arch/session-artifacts/file-history/28fd093e/x@v1"
        )));
        let m: serde_json::Value =
            serde_json::from_slice(&fs.read(Path::new("/arch/manifest.json")).unwrap()).unwrap();
        let want: String = Sha256::digest(payload)
            .iter()
            .map(|x| format!("{x:02x}"))
            .collect();
        assert!(m["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["sha256"] == want));
    }

    // --- AR-09 / AC-35: archive scope, read-failure policy, self-copy guard ---

    use crate::fs::FaultyFileSystem;

    fn opts_at(dir: &str) -> ArchiveOpts {
        ArchiveOpts {
            archive_dir: Path::new(dir).to_path_buf(),
            render: false,
            run_token: "t".into(),
        }
    }

    fn seed_session(fs: &dyn FileSystem, sid: &str) {
        let p = format!("/h/.claude/projects/E--A/{sid}.jsonl");
        fs.write(Path::new(&p), b"{\"cwd\":\"E:\\\\A\"}\n").unwrap();
    }

    /// AC-35: the session folder beside a transcript is archived verbatim.
    #[test]
    fn ac35_archives_session_folder_subagents_and_tool_results() {
        let fs = MemoryFileSystem::new();
        seed_session(&fs, "s1");
        fs.write(
            Path::new("/h/.claude/projects/E--A/s1/subagents/x.jsonl"),
            b"sub",
        )
        .unwrap();
        fs.write(
            Path::new("/h/.claude/projects/E--A/s1/tool-results/y"),
            b"tool",
        )
        .unwrap();
        fs.write(
            Path::new("/h/.claude/projects/E--A/s1/workflows/w/z.json"),
            b"wf",
        )
        .unwrap();
        let r = archive_all(&fs, Path::new("/h"), &opts_at("/arch")).unwrap();
        assert_eq!(r.failed, 0);
        assert_eq!(r.copied, 4);
        assert_eq!(
            fs.read(Path::new("/arch/projects/E--A/s1/subagents/x.jsonl"))
                .unwrap(),
            b"sub"
        );
        assert!(fs.exists(Path::new("/arch/projects/E--A/s1/tool-results/y")));
        assert!(fs.exists(Path::new("/arch/projects/E--A/s1/workflows/w/z.json")));
    }

    /// AC-35: a session folder with no transcript beside it is still archived, with its stores.
    #[test]
    fn ac35_archives_orphan_session_folder_and_its_stores() {
        let fs = MemoryFileSystem::new();
        // A transcript is needed only so the project resolves; the orphan has none.
        seed_session(&fs, "s1");
        fs.write(
            Path::new("/h/.claude/projects/E--A/orphan/subagents/a.jsonl"),
            b"o",
        )
        .unwrap();
        fs.write(Path::new("/h/.claude/tasks/orphan/1.json"), b"t")
            .unwrap();
        archive_all(&fs, Path::new("/h"), &opts_at("/arch")).unwrap();
        assert!(fs.exists(Path::new("/arch/projects/E--A/orphan/subagents/a.jsonl")));
        assert!(fs.exists(Path::new("/arch/session-artifacts/tasks/orphan/1.json")));
    }

    /// AC-35: a project `memory/` folder is not session history and is not copied.
    #[test]
    fn ac35_does_not_copy_project_memory_folder() {
        let fs = MemoryFileSystem::new();
        seed_session(&fs, "s1");
        fs.write(Path::new("/h/.claude/projects/E--A/memory/MEMORY.md"), b"m")
            .unwrap();
        fs.write(Path::new("/h/.claude/projects/E--A/notes.txt"), b"n")
            .unwrap();
        archive_all(&fs, Path::new("/h"), &opts_at("/arch")).unwrap();
        assert!(!fs.exists(Path::new("/arch/projects/E--A/memory")));
        assert!(
            !fs.exists(Path::new("/arch/projects/E--A/notes.txt")),
            "top-level non-transcript files are not session history"
        );
        assert!(fs.exists(Path::new("/arch/projects/E--A/s1.jsonl")));
    }

    /// AC-35: session-env, tasks and todos are archived under session-artifacts.
    #[test]
    fn ac35_archives_session_env_tasks_and_todos() {
        let fs = MemoryFileSystem::new();
        seed_session(&fs, "s1");
        fs.write(Path::new("/h/.claude/session-env/s1/hook.sh"), b"e")
            .unwrap();
        fs.write(Path::new("/h/.claude/tasks/s1/.lock"), b"")
            .unwrap();
        fs.write(Path::new("/h/.claude/tasks/s1/1.json"), b"t")
            .unwrap();
        fs.write(Path::new("/h/.claude/todos/s1-agent-s1.json"), b"[]")
            .unwrap();
        fs.write(Path::new("/h/.claude/todos/other-agent.json"), b"[]")
            .unwrap();
        fs.write(Path::new("/h/.claude/session-env/other/hook.sh"), b"x")
            .unwrap();
        let r = archive_all(&fs, Path::new("/h"), &opts_at("/arch")).unwrap();
        assert_eq!(r.failed, 0);
        assert!(fs.exists(Path::new("/arch/session-artifacts/session-env/s1/hook.sh")));
        assert!(fs.exists(Path::new("/arch/session-artifacts/tasks/s1/.lock")));
        assert!(fs.exists(Path::new("/arch/session-artifacts/tasks/s1/1.json")));
        assert!(fs.exists(Path::new("/arch/session-artifacts/todos/s1-agent-s1.json")));
        assert!(!fs.exists(Path::new("/arch/session-artifacts/todos/other-agent.json")));
        assert!(!fs.exists(Path::new("/arch/session-artifacts/session-env/other")));
    }

    /// AC-35: an unchanged re-run copies nothing and keeps the manifest complete.
    #[test]
    fn ac35_incremental_rerun_copies_zero() {
        let fs = MemoryFileSystem::new();
        seed_session(&fs, "s1");
        fs.write(Path::new("/h/.claude/projects/E--A/s1/subagents/x"), b"a")
            .unwrap();
        fs.write(Path::new("/h/.claude/tasks/s1/1.json"), b"t")
            .unwrap();
        fs.write(Path::new("/h/.claude/todos/s1-a.json"), b"[]")
            .unwrap();
        let r1 = archive_all(&fs, Path::new("/h"), &opts_at("/arch")).unwrap();
        assert_eq!(r1.copied, 4);
        let r2 = archive_all(&fs, Path::new("/h"), &opts_at("/arch")).unwrap();
        assert_eq!(r2.copied, 0);
        assert_eq!(r2.skipped, 4);
        assert_eq!(r2.failed, 0);
        let m: serde_json::Value =
            serde_json::from_slice(&fs.read(Path::new("/arch/manifest.json")).unwrap()).unwrap();
        assert_eq!(m["files"].as_array().unwrap().len(), 4);
    }

    /// AC-35: `archive_session` archives the same set for ONE session and nothing of another.
    #[test]
    fn ac35_archive_session_covers_one_sessions_full_set_only() {
        let fs = MemoryFileSystem::new();
        seed_session(&fs, "s1");
        seed_session(&fs, "s2");
        for sid in ["s1", "s2"] {
            fs.write(
                Path::new(&format!("/h/.claude/projects/E--A/{sid}/subagents/x")),
                sid.as_bytes(),
            )
            .unwrap();
            fs.write(
                Path::new(&format!("/h/.claude/file-history/{sid}/f@v1")),
                b"f",
            )
            .unwrap();
            fs.write(
                Path::new(&format!("/h/.claude/session-env/{sid}/h.sh")),
                b"e",
            )
            .unwrap();
            fs.write(Path::new(&format!("/h/.claude/tasks/{sid}/1.json")), b"t")
                .unwrap();
            fs.write(Path::new(&format!("/h/.claude/todos/{sid}-a.json")), b"[]")
                .unwrap();
        }
        let r = archive_session(
            &fs,
            Path::new("/h"),
            Path::new("/h/.claude/projects/E--A/s1.jsonl"),
            &opts_at("/arch"),
        )
        .unwrap();
        assert_eq!(r.failed, 0);
        assert_eq!(r.copied, 6);
        for p in [
            "/arch/projects/E--A/s1.jsonl",
            "/arch/projects/E--A/s1/subagents/x",
            "/arch/session-artifacts/file-history/s1/f@v1",
            "/arch/session-artifacts/session-env/s1/h.sh",
            "/arch/session-artifacts/tasks/s1/1.json",
            "/arch/session-artifacts/todos/s1-a.json",
        ] {
            assert!(fs.exists(Path::new(p)), "missing {p}");
        }
        assert!(!fs.exists(Path::new("/arch/projects/E--A/s2.jsonl")));
        assert!(!fs.exists(Path::new("/arch/projects/E--A/s2")));
        assert!(!fs.exists(Path::new("/arch/session-artifacts/tasks/s2")));
        assert!(!fs.exists(Path::new("/arch/session-artifacts/todos/s2-a.json")));
    }

    /// An unreadable file is recorded and the sweep continues; it is not in the manifest.
    #[test]
    fn ac35_unreadable_file_is_recorded_and_sweep_continues() {
        let fs = FaultyFileSystem::new(&["tasks/s1/.lock"], &[]);
        seed_session(&fs, "s1");
        fs.write(Path::new("/h/.claude/tasks/s1/.lock"), b"")
            .unwrap();
        fs.write(Path::new("/h/.claude/tasks/s1/1.json"), b"t")
            .unwrap();
        fs.write(Path::new("/h/.claude/projects/E--A/s1/subagents/x"), b"a")
            .unwrap();
        let r = archive_all(&fs, Path::new("/h"), &opts_at("/arch")).unwrap();
        assert_eq!(r.failed, 1);
        assert_eq!(r.failures.len(), 1);
        assert!(r.failures[0].path.to_string_lossy().contains(".lock"));
        assert!(r.failures[0].reason.contains("sharing violation"));
        assert_eq!(r.copied, 3);
        assert!(fs.exists(Path::new("/arch/session-artifacts/tasks/s1/1.json")));
        assert!(!fs.exists(Path::new("/arch/session-artifacts/tasks/s1/.lock")));
        let m: serde_json::Value =
            serde_json::from_slice(&fs.read(Path::new("/arch/manifest.json")).unwrap()).unwrap();
        let files = m["files"].as_array().unwrap();
        assert_eq!(files.len(), 3);
        assert!(!files
            .iter()
            .any(|e| e["src"].as_str().unwrap_or("").contains(".lock")));
        assert!(fs.exists(Path::new("/arch/index.json")));
    }

    /// A destination write failure is recorded, leaves no destination file and is not in the manifest.
    #[test]
    fn ac35_destination_write_failure_leaves_no_partial_file() {
        let fs = FaultyFileSystem::new(&[], &["session-artifacts/tasks/s1/bad"]);
        seed_session(&fs, "s1");
        fs.write(Path::new("/h/.claude/tasks/s1/bad"), b"t")
            .unwrap();
        fs.write(Path::new("/h/.claude/tasks/s1/good"), b"t")
            .unwrap();
        let r = archive_all(&fs, Path::new("/h"), &opts_at("/arch")).unwrap();
        assert_eq!(r.failed, 1);
        assert!(!fs.exists(Path::new("/arch/session-artifacts/tasks/s1/bad")));
        assert!(fs.exists(Path::new("/arch/session-artifacts/tasks/s1/good")));
        let m: serde_json::Value =
            serde_json::from_slice(&fs.read(Path::new("/arch/manifest.json")).unwrap()).unwrap();
        assert!(!m["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["src"].as_str().unwrap_or("").contains("/bad")));
    }

    /// An archive folder placed inside the walked tree must not copy itself.
    #[test]
    fn ac35_archive_folder_inside_walked_tree_is_not_copied_into_itself() {
        let fs = MemoryFileSystem::new();
        seed_session(&fs, "s1");
        fs.write(Path::new("/h/.claude/projects/E--A/s1/subagents/x"), b"a")
            .unwrap();
        // Different casing on purpose: path comparison is case-insensitive.
        let opts = opts_at("/h/.claude/projects/E--A/S1/ARCH");
        archive_all(&fs, Path::new("/h"), &opts).unwrap();
        let r2 = archive_all(&fs, Path::new("/h"), &opts).unwrap();
        assert_eq!(r2.copied, 0, "second run must not see its own output");
        assert_eq!(r2.failed, 0);
        assert!(!fs.exists(Path::new(
            "/h/.claude/projects/E--A/S1/ARCH/projects/E--A/s1/ARCH"
        )));
    }

    /// A top-level transcript that is a link is skipped, like a linked subfolder (AC-61).
    #[test]
    fn ac35_linked_top_level_transcript_is_skipped() {
        let fs = MemoryFileSystem::new();
        seed_session(&fs, "s1");
        fs.write(Path::new("/h/.claude/projects/E--A/link.jsonl"), b"x")
            .unwrap();
        fs.mark_reparse(Path::new("/h/.claude/projects/E--A/link.jsonl"));
        archive_all(&fs, Path::new("/h"), &opts_at("/arch")).unwrap();
        assert!(!fs.exists(Path::new("/arch/projects/E--A/link.jsonl")));
        assert!(fs.exists(Path::new("/arch/projects/E--A/s1.jsonl")));
    }

    /// `archive_session` on a transcript that does not exist stays an error (exit 1).
    #[test]
    fn archive_session_missing_transcript_is_an_error() {
        let fs = MemoryFileSystem::new();
        let err = archive_session(
            &fs,
            Path::new("/h"),
            Path::new("/h/.claude/projects/E--A/none.jsonl"),
            &opts_at("/arch"),
        )
        .unwrap_err();
        assert!(matches!(err, crate::error::AwtError::Io(_)), "{err:?}");
    }
}
