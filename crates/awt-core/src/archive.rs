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
    /// The part of `failed` that is a failure to WRITE to the archive folder (a full disk,
    /// a locked destination). A read failure is a live session holding its own file, which is
    /// expected. A write failure means the archive folder itself is not working, so the CLI
    /// exits 1 for it on a full sweep or a `--session` run.
    pub write_failed: usize,
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

/// Every session-keyed store under `.claude`, copied whole by a full sweep.
const WHOLE_STORES: [&str; 4] = ["file-history", "session-env", "tasks", "todos"];

/// Folders under `.claude` that the archive reads. An archive folder that contains any of
/// them would exclude its own sources from the copy. The empty name is `.claude` itself.
const SOURCE_FOLDERS: [&str; 6] = [
    "",
    "projects",
    "file-history",
    "session-env",
    "tasks",
    "todos",
];

/// Refuse an archive folder that contains the folders the archive reads from. The sweep
/// skips everything under the archive folder so it cannot copy itself, so such a folder
/// would make the run copy nothing, report success, and leave the data unprotected.
/// An archive folder deeper inside the home, such as `.claude/backups/x`, stays allowed.
pub fn check_archive_dir(home: &Path, archive_dir: &Path) -> Result<()> {
    let key = path_key(archive_dir);
    if key.is_empty() {
        return Err(AwtError::Locked(format!(
            "archive folder '{}' is a filesystem root, which contains the Claude folders \
             that are archived; choose a folder that holds only the archive",
            archive_dir.display()
        )));
    }
    let claude = home.join(".claude");
    for rel in SOURCE_FOLDERS {
        let source = if rel.is_empty() {
            claude.clone()
        } else {
            claude.join(rel)
        };
        if is_under(&source, &key) {
            return Err(AwtError::Locked(format!(
                "archive folder '{}' contains '{}', which is archived; the run would copy \
                 nothing. Choose a folder that is not above or equal to the Claude data",
                archive_dir.display(),
                source.display()
            )));
        }
    }
    Ok(())
}

/// A session id becomes a path component under `.claude/<store>/` and beside the
/// transcript. `.` and `..` would resolve to a folder above the store, so they are refused,
/// along with the empty id. A transcript named `...jsonl` has the stem `..`.
fn is_safe_session_id(id: &str) -> bool {
    !id.is_empty() && id != "." && id != ".."
}

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

    /// Record a failure to write into the archive folder. Counted apart from read failures.
    fn fail_write(&mut self, path: &Path, reason: String) {
        self.report.write_failed += 1;
        self.fail(path, reason);
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
            self.fail_write(src, format!("cannot write {}: {e}", dst.display()));
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
        let todos = match self.fs.read_dir(&todos_dir) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => {
                self.fail(&todos_dir, format!("cannot list folder: {e}"));
                Vec::new()
            }
        };
        for sid in session_ids {
            if !is_safe_session_id(sid) {
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

    /// Copy each session-keyed store whole, under `session-artifacts/`. A full sweep uses
    /// this instead of the per-id copy so that files whose session has no transcript or
    /// session folder left are archived too. A store that is a link is skipped (AC-61). A
    /// store that does not exist is skipped; one that cannot be listed is a failure.
    fn copy_whole_stores(&mut self, claude_root: &Path) {
        let base = self.opts.archive_dir.join("session-artifacts");
        for store in WHOLE_STORES {
            let dir = claude_root.join(store);
            if self.fs.is_reparse_point(&dir) {
                continue;
            }
            self.copy_tree(claude_root, &dir, &base);
        }
    }

    /// Archive one project folder: top-level transcripts and every session subfolder except
    /// `memory`. Returns the session ids found by either route, so a caller that archives a
    /// single project can copy the session-keyed stores for those ids.
    fn project_dir(&mut self, home: &Path, project_dir: &Path) -> Vec<String> {
        let dir_rel = project_rel(home, project_dir);
        let dst_base = self.opts.archive_dir.join("projects").join(&dir_rel);
        let children = match self.fs.read_dir(project_dir) {
            Ok(c) => c,
            Err(e) => {
                self.fail(project_dir, format!("cannot list folder: {e}"));
                return Vec::new();
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
                let id = name.to_string_lossy().into_owned();
                if is_safe_session_id(&id) {
                    session_ids.push(id);
                }
                self.copy_tree(project_dir, &child, &dst_base);
            } else if child.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                if let Some(stem) = child.file_stem().and_then(|s| s.to_str()) {
                    if is_safe_session_id(stem) {
                        session_ids.push(stem.to_string());
                    }
                }
                self.copy(&child, &dst_base.join(&name));
            }
        }
        session_ids.sort();
        session_ids.dedup();
        session_ids
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
    check_archive_dir(home, &opts.archive_dir)?;
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
    let mut session_ids: Vec<String> = Vec::new();
    for dir in dirs {
        session_ids.extend(sweep.project_dir(home, dir));
    }
    session_ids.sort();
    session_ids.dedup();
    sweep.copy_stores(&home.join(".claude"), &session_ids);
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
    check_archive_dir(home, &opts.archive_dir)?;
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
        if is_safe_session_id(session_id) {
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
/// Covers resolved, unresolved, and ambiguous project dirs so nothing is silently dropped,
/// and copies `file-history/`, `session-env/`, `tasks/` and `todos/` whole.
pub fn archive_all(fs: &dyn FileSystem, home: &Path, opts: &ArchiveOpts) -> Result<ArchiveReport> {
    check_archive_dir(home, &opts.archive_dir)?;
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
    // The stores are copied whole, not per session id, so files whose session has no
    // transcript or session folder left are archived too.
    sweep.copy_whole_stores(&home.join(".claude"));
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

    /// AC-35: a full sweep copies session-env, tasks, todos and file-history WHOLE, so files
    /// whose session has no transcript or session folder left are archived too. Nothing is
    /// copied twice (`skipped` stays 0), which shows the per-id copy did not also run.
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
        // No transcript and no session folder exist for these ids.
        fs.write(Path::new("/h/.claude/todos/other-agent.json"), b"[]")
            .unwrap();
        fs.write(Path::new("/h/.claude/session-env/other/hook.sh"), b"x")
            .unwrap();
        fs.write(Path::new("/h/.claude/file-history/ghost/f@v1"), b"f")
            .unwrap();
        let r = archive_all(&fs, Path::new("/h"), &opts_at("/arch")).unwrap();
        assert_eq!(r.failed, 0);
        assert_eq!(r.copied, 8, "the transcript plus seven store files");
        assert_eq!(r.skipped, 0, "no file may be copied twice");
        for p in [
            "/arch/session-artifacts/session-env/s1/hook.sh",
            "/arch/session-artifacts/tasks/s1/.lock",
            "/arch/session-artifacts/tasks/s1/1.json",
            "/arch/session-artifacts/todos/s1-agent-s1.json",
            "/arch/session-artifacts/todos/other-agent.json",
            "/arch/session-artifacts/session-env/other/hook.sh",
            "/arch/session-artifacts/file-history/ghost/f@v1",
        ] {
            assert!(fs.exists(Path::new(p)), "missing {p}");
        }
        let m: serde_json::Value =
            serde_json::from_slice(&fs.read(Path::new("/arch/manifest.json")).unwrap()).unwrap();
        assert_eq!(m["files"].as_array().unwrap().len(), 8);
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
        // The archive folder is spelled with different casing than the source folder. The
        // in-memory filesystem ignores case too, so this checks the run end to end, but it is
        // `path_keys_ignore_case_and_separator` that pins the case folding of the guard.
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

    // --- Review follow-up: destination faults, guards and path keys ---

    fn seed_task(fs: &dyn FileSystem, bytes: &[u8]) {
        fs.write(Path::new("/h/.claude/tasks/s1/1.json"), bytes)
            .unwrap();
    }

    fn manifest_srcs(fs: &dyn FileSystem) -> Vec<String> {
        let m: serde_json::Value =
            serde_json::from_slice(&fs.read(Path::new("/arch/manifest.json")).unwrap()).unwrap();
        m["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["src"].as_str().unwrap().to_string())
            .collect()
    }

    /// A read failure is counted in `failed` only; a write failure is also in `write_failed`.
    #[test]
    fn write_failures_are_counted_apart_from_read_failures() {
        let fs = FaultyFileSystem::new(&["tasks/s1/.lock"], &["session-artifacts/tasks/s1/bad"]);
        seed_session(&fs, "s1");
        fs.write(Path::new("/h/.claude/tasks/s1/.lock"), b"")
            .unwrap();
        fs.write(Path::new("/h/.claude/tasks/s1/bad"), b"t")
            .unwrap();
        let r = archive_all(&fs, Path::new("/h"), &opts_at("/arch")).unwrap();
        assert_eq!(r.failed, 2);
        assert_eq!(r.write_failed, 1);
    }

    /// A failed rename leaves the older destination bytes untouched and removes the temp file.
    #[test]
    fn failed_rename_keeps_the_old_destination_and_removes_the_temp_file() {
        let fs = FaultyFileSystem::new(&[], &[])
            .with_fail_rename(&["session-artifacts/tasks/s1/1.json"]);
        seed_session(&fs, "s1");
        seed_task(&fs, b"new bytes");
        fs.write(
            Path::new("/arch/session-artifacts/tasks/s1/1.json"),
            b"old bytes",
        )
        .unwrap();
        let r = archive_all(&fs, Path::new("/h"), &opts_at("/arch")).unwrap();
        assert_eq!(r.write_failed, 1);
        assert_eq!(
            fs.read(Path::new("/arch/session-artifacts/tasks/s1/1.json"))
                .unwrap(),
            b"old bytes"
        );
        assert!(
            !fs.exists(Path::new(
                "/arch/session-artifacts/tasks/s1/1.json.t.awt-tmp"
            )),
            "the temp file must be removed"
        );
        assert!(!manifest_srcs(&fs).iter().any(|s| s.ends_with("1.json")));
    }

    /// A write that tears (stores part of the bytes, then fails) must not reach the old copy.
    /// Writing straight to the destination, instead of a temp file, would fail this test.
    #[test]
    fn torn_write_never_reaches_the_old_destination() {
        let fs =
            FaultyFileSystem::new(&[], &[]).with_torn_write(&["session-artifacts/tasks/s1/1.json"]);
        seed_session(&fs, "s1");
        seed_task(&fs, b"new bytes!");
        fs.inner
            .write(
                Path::new("/arch/session-artifacts/tasks/s1/1.json"),
                b"old bytes",
            )
            .unwrap();
        let r = archive_all(&fs, Path::new("/h"), &opts_at("/arch")).unwrap();
        assert_eq!(r.write_failed, 1);
        assert_eq!(
            fs.read(Path::new("/arch/session-artifacts/tasks/s1/1.json"))
                .unwrap(),
            b"old bytes"
        );
        assert!(
            !fs.exists(Path::new(
                "/arch/session-artifacts/tasks/s1/1.json.t.awt-tmp"
            )),
            "the torn temp file must be removed"
        );
    }

    /// Case and separator differences must not defeat the self-copy guard.
    #[test]
    fn path_keys_ignore_case_and_separator() {
        assert_eq!(path_key(Path::new("C:\\Users\\X\\")), "c:/users/x");
        assert_eq!(path_key(Path::new("/")), "");
        assert_eq!(path_key(Path::new("C:\\")), "c:");
        let root = path_key(Path::new("c:/USERS/x/Arch"));
        assert!(is_under(Path::new("C:\\Users\\X\\arch"), &root));
        assert!(is_under(Path::new("C:\\users\\x\\ARCH\\a\\b.txt"), &root));
        assert!(!is_under(Path::new("C:\\Users\\X\\archive"), &root));
        assert!(!is_under(Path::new("C:\\Users\\X"), &root));
        assert!(!is_under(Path::new("/anything"), ""));
    }

    /// The folder name `memory` is matched without regard to case.
    #[test]
    fn memory_folder_is_excluded_whatever_its_case() {
        let fs = MemoryFileSystem::new();
        seed_session(&fs, "s1");
        fs.write(Path::new("/h/.claude/projects/E--A/MeMoRy/MEMORY.md"), b"m")
            .unwrap();
        let r = archive_all(&fs, Path::new("/h"), &opts_at("/arch")).unwrap();
        assert_eq!(r.copied, 1);
        assert!(!fs.exists(Path::new("/arch/projects/E--A/MeMoRy")));
    }

    /// A folder that cannot be listed is recorded in `failed` and the rest is still archived.
    #[test]
    fn unlistable_folder_is_recorded_in_failed() {
        let fs = FaultyFileSystem::new(&[], &[]).with_fail_list(&["s1/subagents"]);
        seed_session(&fs, "s1");
        fs.write(Path::new("/h/.claude/projects/E--A/s1/subagents/x"), b"a")
            .unwrap();
        fs.write(Path::new("/h/.claude/projects/E--A/s1/other"), b"o")
            .unwrap();
        let r = archive_all(&fs, Path::new("/h"), &opts_at("/arch")).unwrap();
        assert_eq!(r.failed, 1);
        assert_eq!(r.write_failed, 0);
        assert!(r.failures[0].reason.contains("cannot list folder"));
        assert!(fs.exists(Path::new("/arch/projects/E--A/s1/other")));
    }

    /// A store folder that cannot be listed is a failure, in a sweep and in the per-id copy.
    /// A store folder that does not exist is not.
    #[test]
    fn unlistable_store_is_a_failure_and_a_missing_store_is_not() {
        let fs = FaultyFileSystem::new(&[], &[]).with_fail_list(&["/h/.claude/tasks"]);
        seed_session(&fs, "s1");
        let r = archive_all(&fs, Path::new("/h"), &opts_at("/arch")).unwrap();
        assert_eq!(r.failed, 1, "{:?}", r.failures);
        assert!(r.failures[0].path.ends_with("tasks"));

        let fs = FaultyFileSystem::new(&[], &[]).with_fail_list(&["/h/.claude/todos"]);
        seed_session(&fs, "s1");
        let r = archive_session(
            &fs,
            Path::new("/h"),
            Path::new("/h/.claude/projects/E--A/s1.jsonl"),
            &opts_at("/arch"),
        )
        .unwrap();
        assert_eq!(r.failed, 1, "{:?}", r.failures);
        assert!(r.failures[0].path.ends_with("todos"));

        let fs = MemoryFileSystem::new();
        seed_session(&fs, "s1");
        let r = archive_session(
            &fs,
            Path::new("/h"),
            Path::new("/h/.claude/projects/E--A/s1.jsonl"),
            &opts_at("/arch"),
        )
        .unwrap();
        assert_eq!(r.failed, 0, "no stores at all is not a failure");
    }

    /// A store folder that is a link is skipped by the whole-store copy (AC-61).
    #[test]
    fn linked_store_folder_is_skipped_by_the_sweep() {
        let fs = MemoryFileSystem::new();
        seed_session(&fs, "s1");
        fs.write(Path::new("/h/.claude/tasks/s1/1.json"), b"t")
            .unwrap();
        fs.write(Path::new("/h/.claude/todos/s1-a.json"), b"[]")
            .unwrap();
        fs.mark_reparse(Path::new("/h/.claude/tasks"));
        archive_all(&fs, Path::new("/h"), &opts_at("/arch")).unwrap();
        assert!(!fs.exists(Path::new("/arch/session-artifacts/tasks")));
        assert!(fs.exists(Path::new("/arch/session-artifacts/todos/s1-a.json")));
    }

    /// `archive_session` matches a todos file that holds the session id in the MIDDLE of its
    /// name. Changing `contains` to `starts_with` would fail this test.
    #[test]
    fn archive_session_matches_a_todos_file_with_the_id_in_the_middle() {
        let fs = MemoryFileSystem::new();
        seed_session(&fs, "s1");
        fs.write(Path::new("/h/.claude/todos/agent-s1-x.json"), b"[]")
            .unwrap();
        fs.write(Path::new("/h/.claude/todos/agent-s2-x.json"), b"[]")
            .unwrap();
        archive_session(
            &fs,
            Path::new("/h"),
            Path::new("/h/.claude/projects/E--A/s1.jsonl"),
            &opts_at("/arch"),
        )
        .unwrap();
        assert!(fs.exists(Path::new("/arch/session-artifacts/todos/agent-s1-x.json")));
        assert!(!fs.exists(Path::new("/arch/session-artifacts/todos/agent-s2-x.json")));
    }

    /// `archive_project` stays per id: it copies the stores of its own sessions only.
    #[test]
    fn archive_project_copies_stores_per_session_id_only() {
        let fs = MemoryFileSystem::new();
        seed_session(&fs, "s1");
        fs.write(Path::new("/h/.claude/tasks/s1/1.json"), b"t")
            .unwrap();
        fs.write(Path::new("/h/.claude/tasks/other/1.json"), b"t")
            .unwrap();
        let r = archive_project(&fs, Path::new("/h"), "E:\\A", &opts_at("/arch")).unwrap();
        assert_eq!(r.failed, 0);
        assert!(fs.exists(Path::new("/arch/session-artifacts/tasks/s1/1.json")));
        assert!(!fs.exists(Path::new("/arch/session-artifacts/tasks/other")));
    }

    fn assert_refused_by_all_three(dir: &str) {
        let fs = MemoryFileSystem::new();
        seed_session(&fs, "s1");
        fs.write(Path::new("/h/.claude/tasks/s1/1.json"), b"t")
            .unwrap();
        let opts = opts_at(dir);
        let transcript = Path::new("/h/.claude/projects/E--A/s1.jsonl");
        let errs = [
            archive_all(&fs, Path::new("/h"), &opts).unwrap_err(),
            archive_session(&fs, Path::new("/h"), transcript, &opts).unwrap_err(),
            archive_project(&fs, Path::new("/h"), "E:\\A", &opts).unwrap_err(),
        ];
        for e in errs {
            assert!(matches!(e, AwtError::Locked(_)), "{dir}: {e:?}");
        }
        assert!(
            !fs.exists(&Path::new(dir).join("manifest.json")),
            "{dir}: nothing may be written"
        );
    }

    /// An archive folder at or above the Claude data would exclude its own sources.
    #[test]
    fn archive_folder_containing_the_sources_is_refused() {
        for dir in [
            "/",
            "/h",
            "/H/.CLAUDE",
            "/h/.claude",
            "/h/.claude/projects",
            "/h/.claude/file-history",
            "/h/.claude/session-env",
            "/h/.claude/tasks",
            "/h/.claude/todos",
        ] {
            assert_refused_by_all_three(dir);
        }
    }

    /// An archive folder that does not contain the Claude data stays allowed, even when it
    /// sits inside the home or inside a project folder.
    #[test]
    fn archive_folder_deeper_inside_the_home_is_allowed() {
        for dir in [
            "/h/.claude/backups/x",
            "/h/.claude/projects/E--A/s1/arch",
            "/h/.claude-sessions",
            "/h2",
        ] {
            let fs = MemoryFileSystem::new();
            seed_session(&fs, "s1");
            let opts = opts_at(dir);
            archive_all(&fs, Path::new("/h"), &opts).unwrap_or_else(|e| panic!("{dir}: {e:?}"));
            check_archive_dir(Path::new("/h"), &opts.archive_dir).unwrap();
        }
    }

    // --- Dot-named transcripts must not become path traversal ---

    fn real_home_with_secret() -> (tempfile::TempDir, tempfile::TempDir) {
        let home = tempfile::tempdir().unwrap();
        let arch = tempfile::tempdir().unwrap();
        let claude = home.path().join(".claude");
        let proj = claude.join("projects").join("E--A");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join("s1.jsonl"), b"{\"cwd\":\"E:\\\\A\"}\n").unwrap();
        std::fs::create_dir_all(claude.join("tasks")).unwrap();
        std::fs::create_dir_all(claude.join("todos")).unwrap();
        std::fs::write(claude.join(".credentials.json"), b"SECRET").unwrap();
        (home, arch)
    }

    fn assert_no_secret_copied(arch: &Path) {
        for entry in walkdir::WalkDir::new(arch) {
            let entry = entry.unwrap();
            assert_ne!(
                entry.file_name().to_string_lossy(),
                ".credentials.json",
                "a credential file was copied to {}",
                entry.path().display()
            );
        }
    }

    /// `...jsonl` has the stem `..`, and `..jsonl` has the stem `.`. Joining the stem onto a
    /// store folder would resolve to the folder above it, which holds the credentials. The
    /// transcript file itself is still copied.
    #[test]
    fn dot_named_transcripts_do_not_escape_the_stores() {
        for name in ["...jsonl", "..jsonl"] {
            let (home, arch) = real_home_with_secret();
            let proj = home.path().join(".claude/projects/E--A");
            std::fs::write(proj.join(name), b"{\"cwd\":\"E:\\\\A\"}\n").unwrap();
            let opts = ArchiveOpts {
                archive_dir: arch.path().to_path_buf(),
                render: false,
                run_token: "t".into(),
            };
            let real = crate::fs::RealFileSystem;
            let r = archive_session(&real, home.path(), &proj.join(name), &opts).unwrap();
            assert_eq!(r.failed, 0, "{name}");
            assert_eq!(r.copied, 1, "{name}: only the transcript is copied");
            assert!(arch.path().join("projects/E--A").join(name).is_file());
            assert_no_secret_copied(arch.path());

            let (home, arch) = real_home_with_secret();
            let proj = home.path().join(".claude/projects/E--A");
            std::fs::write(proj.join(name), b"{\"cwd\":\"E:\\\\A\"}\n").unwrap();
            let opts = ArchiveOpts {
                archive_dir: arch.path().to_path_buf(),
                render: false,
                run_token: "t".into(),
            };
            archive_project(&real, home.path(), "E:\\A", &opts).unwrap();
            assert!(arch.path().join("projects/E--A").join(name).is_file());
            assert_no_secret_copied(arch.path());
        }
    }

    /// The per-id store copy itself refuses the empty id, `.` and `..`.
    #[test]
    fn copy_stores_refuses_empty_dot_and_dotdot_ids() {
        let (home, arch) = real_home_with_secret();
        let claude = home.path().join(".claude");
        std::fs::write(claude.join("todos").join("a.json"), b"[]").unwrap();
        std::fs::write(claude.join("tasks").join("t.json"), b"t").unwrap();
        let opts = ArchiveOpts {
            archive_dir: arch.path().to_path_buf(),
            render: false,
            run_token: "t".into(),
        };
        let real = crate::fs::RealFileSystem;
        let mut sweep = Sweep::new(&real, &opts);
        sweep.copy_stores(&claude, &["..".to_string(), ".".to_string(), String::new()]);
        assert_eq!(sweep.report.copied, 0);
        assert_eq!(sweep.report.failed, 0);
        assert_no_secret_copied(arch.path());
    }
}
