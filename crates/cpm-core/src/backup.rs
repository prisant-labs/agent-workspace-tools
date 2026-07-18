use crate::error::Result;
use crate::fs::FileSystem;
use crate::model::{Change, Move};
use crate::plan::Plan;
use sha2::{Digest, Sha256};
use std::path::Path;

pub struct ManifestEntry {
    pub original: String,
    pub backup: String,
    pub sha256: String,
}
pub struct Manifest {
    pub run_id: String,
    pub mv: Move,
    pub entries: Vec<ManifestEntry>,
}

fn hexd(b: &[u8]) -> String {
    let d = Sha256::digest(b);
    d.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn snapshot(
    plan: &Plan,
    fs: &dyn FileSystem,
    backup_root: &Path,
    run_id: &str,
) -> Result<Manifest> {
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
                entries.push(ManifestEntry {
                    original: from.to_string_lossy().into_owned(),
                    backup: format!("<dir-rename {i}>"),
                    sha256: String::new(),
                });
            }
            Change::RewriteFile { path, .. } => {
                backup_one(fs, &dir, path, &format!("f{i}"), &mut entries)?
            }
            Change::RenameJsonKey { path, .. } | Change::RewriteJsonArrayValue { path, .. } => {
                backup_one(fs, &dir, path, &format!("j{i}"), &mut entries)?
            }
            Change::MoveTree { from, .. } => {
                entries.push(ManifestEntry {
                    original: from.to_string_lossy().into_owned(),
                    backup: format!("<move-tree {i}>"),
                    sha256: String::new(),
                });
            }
        }
    }
    let m = Manifest {
        run_id: run_id.to_string(),
        mv: plan.mv.clone(),
        entries,
    };
    write_manifest(fs, &dir, &m)?;
    Ok(m)
}

/// Copy one existing file into the backup dir and record a manifest entry. A plain helper -
/// NOT a closure - so it can borrow `entries` mutably per call (LEAD-06).
fn backup_one(
    fs: &dyn FileSystem,
    dir: &Path,
    orig: &Path,
    tag: &str,
    entries: &mut Vec<ManifestEntry>,
) -> Result<()> {
    if fs.is_file(orig) {
        let bytes = fs.read(orig)?;
        let bpath = dir.join(format!(
            "{tag}-{}",
            orig.file_name().unwrap().to_string_lossy()
        ));
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
    fs.write(
        &dir.join("manifest.json"),
        serde_json::to_vec_pretty(&json).unwrap().as_slice(),
    )?;
    Ok(())
}

impl Manifest {
    /// Reconstruct a Manifest from a written manifest.json (used by rollback / verify later).
    pub fn load(fs: &dyn FileSystem, path: &Path) -> Result<Manifest> {
        let v: serde_json::Value = serde_json::from_slice(&fs.read(path)?)
            .map_err(|e| crate::error::CpmError::UnrecognizedFormat(e.to_string()))?;
        let entries = v["entries"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|e| ManifestEntry {
                original: e["original"].as_str().unwrap_or_default().to_string(),
                backup: e["backup"].as_str().unwrap_or_default().to_string(),
                sha256: e["sha256"].as_str().unwrap_or_default().to_string(),
            })
            .collect();
        Ok(Manifest {
            run_id: v["run_id"].as_str().unwrap_or_default().to_string(),
            mv: Move {
                src_abs: v["src_abs"].as_str().unwrap_or_default().to_string(),
                dst_abs: v["dst_abs"].as_str().unwrap_or_default().to_string(),
            },
            entries,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemoryFileSystem;
    use crate::model::Move;
    use crate::plan::{build_plan, Collision, PlanOpts};
    use std::path::Path;

    #[test]
    fn snapshot_backs_up_every_touched_file() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
            b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n",
        )
        .unwrap();
        fs.write(Path::new("E:/Projects/A/file.txt"), b"payload")
            .unwrap();
        let mv = Move {
            src_abs: "E:\\Projects\\A".into(),
            dst_abs: "E:\\Projects\\B".into(),
        };
        let opts = PlanOpts {
            recursive: false,
            on_collision: Collision::Refuse,
            force: false,
            scope: crate::model::Scope::Standard,
        };
        let plan = build_plan(&fs, Path::new("/h"), &mv, &opts).unwrap();
        let m = snapshot(&plan, &fs, Path::new("/backup"), "TEST").unwrap();
        assert!(!m.entries.is_empty());
        assert!(fs.exists(Path::new("/backup/cpm-TEST")));
    }

    #[test]
    fn snapshot_backs_up_every_old_transcript_with_sha256() {
        let fs = MemoryFileSystem::new();
        let body = b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n";
        fs.write(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"), body)
            .unwrap();
        fs.write(Path::new("E:/Projects/A/file.txt"), b"payload")
            .unwrap();
        let mv = Move {
            src_abs: "E:\\Projects\\A".into(),
            dst_abs: "E:\\Projects\\B".into(),
        };
        let opts = PlanOpts {
            recursive: false,
            on_collision: Collision::Refuse,
            force: false,
            scope: crate::model::Scope::Standard,
        };
        let plan = build_plan(&fs, Path::new("/h"), &mv, &opts).unwrap();
        let m = snapshot(&plan, &fs, Path::new("/backup"), "TEST").unwrap();
        let e = m
            .entries
            .iter()
            .find(|e| e.original.ends_with("s.jsonl"))
            .expect("transcript backed up");
        assert_eq!(e.sha256, hexd(body));
        assert!(fs.exists(Path::new(&e.backup)));
    }
}
