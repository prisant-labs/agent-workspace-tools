use crate::error::Result;
use crate::fs::FileSystem;
use crate::index::ProjectIndex;
use crate::sessions::footprint;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub struct ArchiveOpts {
    pub archive_dir: PathBuf,
    pub render: bool,
}

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
    if fs.exists(dst) {
        if let Ok(dst_bytes) = fs.read(dst) {
            if sha(&dst_bytes) == src_hash {
                report.skipped += 1;
                return Ok(());
            }
        }
    }
    fs.write(dst, &bytes)?;
    entries.push(ArchEntry {
        src: src.to_path_buf(),
        dst: dst.to_path_buf(),
        sha256: src_hash,
    });
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

fn write_manifest(fs: &dyn FileSystem, path: &Path, entries: &[ArchEntry]) -> Result<()> {
    let files: Vec<_> = entries
        .iter()
        .map(|e| {
            serde_json::json!({
                "src": e.src.to_string_lossy(),
                "dst": e.dst.to_string_lossy(),
                "sha256": e.sha256,
            })
        })
        .collect();
    let json = serde_json::json!({ "files": files });
    fs.write(path, serde_json::to_vec_pretty(&json).unwrap().as_slice())?;
    Ok(())
}

fn write_index(fs: &dyn FileSystem, path: &Path) -> Result<()> {
    let json = serde_json::json!({ "version": 1 });
    fs.write(path, serde_json::to_vec_pretty(&json).unwrap().as_slice())?;
    Ok(())
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
    write_manifest(fs, &manifest_path, &entries)?;

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
    write_manifest(fs, &manifest_path, &entries)?;

    write_index(fs, &opts.archive_dir.join("index.json"))?;

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, MemoryFileSystem};
    use sha2::{Digest, Sha256};
    use std::path::Path;

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
        };
        archive_all(&fs, Path::new("/h"), &opts).unwrap();
        assert!(fs.exists(Path::new("/arch/projects/E--Ghost/s.jsonl")));
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
