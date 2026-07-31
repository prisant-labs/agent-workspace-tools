use crate::error::{AwtError, Result};
use crate::fs::FileSystem;
use crate::index::ProjectIndex;
use crate::sessions::footprint;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub struct ArchiveOpts {
    pub archive_dir: PathBuf,
    pub render: bool,
    /// Per-run token used to name the sibling temp file during manifest writes.
    /// Must be unique per concurrent archive process to reduce collision risk.
    pub run_token: String,
}

#[derive(Debug)]
pub struct ArchiveReport {
    pub copied: usize,
    pub skipped: usize,
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

fn fs_walk(fs: &dyn FileSystem, dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(children) = fs.read_dir(dir) {
        for child in children {
            // AC-61: never follow a junction or symlink while archiving - a link inside a
            // project-state tree could pull arbitrary outside content into the archive (or
            // recurse forever). Unlike the mutation walk this one SKIPS rather than refuses,
            // because archive is a best-effort protective sweep across every project and one
            // link should not abort protecting the rest; the skip is the documented policy.
            if fs.is_reparse_point(&child) {
                continue;
            }
            if fs.is_dir(&child) {
                out.extend(fs_walk(fs, &child));
            } else {
                out.push(child);
            }
        }
    }
    out
}

fn copy_if_changed(
    fs: &dyn FileSystem,
    src: &Path,
    dst: &Path,
    entries: &mut Vec<ArchEntry>,
    report: &mut ArchiveReport,
) -> Result<()> {
    let bytes = fs.read(src)?;
    let src_hash = sha(&bytes);
    // Record every examined file in the manifest regardless of copy-vs-skip,
    // so a no-change rerun never produces an empty or incomplete manifest.
    entries.push(ArchEntry {
        src: src.to_path_buf(),
        dst: dst.to_path_buf(),
        sha256: src_hash.clone(),
    });
    if fs.exists(dst) {
        if let Ok(dst_bytes) = fs.read(dst) {
            if sha(&dst_bytes) == src_hash {
                report.skipped += 1;
                return Ok(());
            }
        }
    }
    fs.write(dst, &bytes)?;
    report.copied += 1;
    Ok(())
}

fn archive_project_dir(
    fs: &dyn FileSystem,
    home: &Path,
    project_dir: &Path,
    opts: &ArchiveOpts,
    entries: &mut Vec<ArchEntry>,
    report: &mut ArchiveReport,
) -> Result<()> {
    let projects_root = home.join(".claude").join("projects");
    let dir_rel: PathBuf = project_dir
        .strip_prefix(&projects_root)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|_| PathBuf::from(project_dir.file_name().unwrap_or_default()));

    // Copy all transcript files from this project dir.
    for child in fs.read_dir(project_dir).unwrap_or_default() {
        if child.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            let fname = match child.file_name() {
                Some(n) => n.to_os_string(),
                None => continue,
            };
            let dst = opts
                .archive_dir
                .join("projects")
                .join(&dir_rel)
                .join(&fname);
            copy_if_changed(fs, &child, &dst, entries, report)?;
        }
    }

    // Copy file-history entries for every session in this project dir.
    let fp = footprint(fs, home, project_dir);
    let claude_root = home.join(".claude");
    let file_history_root = claude_root.join("file-history");
    for session_id in &fp.session_ids {
        let fh_dir = file_history_root.join(session_id);
        if fs.is_dir(&fh_dir) {
            for src in fs_walk(fs, &fh_dir) {
                let rel = src.strip_prefix(&claude_root).unwrap_or(&src);
                let dst = opts.archive_dir.join("session-artifacts").join(rel);
                copy_if_changed(fs, &src, &dst, entries, report)?;
            }
        }
    }

    Ok(())
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
    let mut rep = ArchiveReport {
        copied: 0,
        skipped: 0,
    };
    let mut man = Vec::new();
    for dir in dirs {
        archive_project_dir(fs, home, dir, opts, &mut man, &mut rep)?;
    }
    let manifest_path = opts.archive_dir.join("manifest.json");
    write_manifest(fs, &manifest_path, &man, &opts.run_token)?;
    write_index(fs, &opts.archive_dir.join("index.json"))?;
    Ok(rep)
}

/// Archive a single session transcript and its associated file-history entries.
pub fn archive_session(
    fs: &dyn FileSystem,
    home: &Path,
    transcript: &Path,
    opts: &ArchiveOpts,
) -> Result<ArchiveReport> {
    let projects_root = home.join(".claude").join("projects");
    let project_dir = transcript.parent().unwrap_or(Path::new(""));
    let dir_rel: PathBuf = project_dir
        .strip_prefix(&projects_root)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|_| PathBuf::from(project_dir.file_name().unwrap_or_default()));

    let mut entries = Vec::new();
    let mut report = ArchiveReport {
        copied: 0,
        skipped: 0,
    };

    if let Some(fname) = transcript.file_name() {
        let dst = opts.archive_dir.join("projects").join(&dir_rel).join(fname);
        copy_if_changed(fs, transcript, &dst, &mut entries, &mut report)?;
    }

    if let Some(session_id) = transcript.file_stem().and_then(|s| s.to_str()) {
        let claude_root = home.join(".claude");
        let fh_dir = claude_root.join("file-history").join(session_id);
        if fs.is_dir(&fh_dir) {
            for src in fs_walk(fs, &fh_dir) {
                let rel = src.strip_prefix(&claude_root).unwrap_or(&src);
                let dst = opts.archive_dir.join("session-artifacts").join(rel);
                copy_if_changed(fs, &src, &dst, &mut entries, &mut report)?;
            }
        }
    }

    let manifest_path = opts.archive_dir.join("manifest.json");
    write_manifest(fs, &manifest_path, &entries, &opts.run_token)?;

    Ok(report)
}

/// Archive all Claude project session state to the archive directory.
/// Covers resolved, unresolved, and ambiguous project dirs so nothing is silently dropped.
pub fn archive_all(fs: &dyn FileSystem, home: &Path, opts: &ArchiveOpts) -> Result<ArchiveReport> {
    let index = ProjectIndex::build(fs, home)?;

    let mut entries = Vec::new();
    let mut report = ArchiveReport {
        copied: 0,
        skipped: 0,
    };

    let all_dirs: Vec<&PathBuf> = index
        .by_cwd
        .values()
        .flatten()
        .chain(index.unresolved.iter())
        .chain(index.ambiguous.iter())
        .collect();

    for dir in all_dirs {
        archive_project_dir(fs, home, dir, opts, &mut entries, &mut report)?;
    }

    let manifest_path = opts.archive_dir.join("manifest.json");
    write_manifest(fs, &manifest_path, &entries, &opts.run_token)?;

    write_index(fs, &opts.archive_dir.join("index.json"))?;

    Ok(report)
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
}
