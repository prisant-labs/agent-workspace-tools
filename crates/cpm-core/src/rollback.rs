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
            continue; // handled by the whole-tree restore above
        }
        let bytes = fs.read(Path::new(backup))?;
        let want = e["sha256"].as_str().unwrap_or_default();
        let got: String = Sha256::digest(&bytes)
            .iter()
            .map(|x| format!("{x:02x}"))
            .collect();
        if !want.is_empty() && got != want {
            return Err(CpmError::VerifyFailed(format!(
                "backup corrupted: {backup}"
            )));
        }
        fs.write(Path::new(original), &bytes)?;
    }
    // 3. Remove the lingering new-encoded projects dir. apply renamed the old-encoded dir to
    //    the new one; step 2 restored the transcripts back into the OLD-encoded dir, so the
    //    new-encoded dir now holds only a stale duplicate. Leaving it turns an "undo" into an
    //    orphan - the exact residue this tool detects. Derive it from a dir-rename entry (whose
    //    `original` is the old-encoded dir) by swapping the old project encoding for the new.
    let (old_enc, new_enc) = (
        crate::paths::encode_project_dir(v["src_abs"].as_str().unwrap()),
        crate::paths::encode_project_dir(v["dst_abs"].as_str().unwrap()),
    );
    for e in v["entries"].as_array().unwrap() {
        let backup = e["backup"].as_str().unwrap();
        if !backup.starts_with("<dir-rename") {
            continue;
        }
        let old_dir = e["original"].as_str().unwrap();
        if old_dir.contains(&old_enc) {
            let new_dir = old_dir.replace(&old_enc, &new_enc);
            if fs.exists(Path::new(&new_dir)) {
                fs.remove_dir_all(Path::new(&new_dir))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::apply;
    use crate::fs::{FileSystem, MemoryFileSystem};
    use crate::model::Move;
    use crate::plan::{build_plan, Collision, PlanOpts};
    use std::path::Path;

    #[test]
    fn rollback_restores_pre_move_bytes() {
        let fs = MemoryFileSystem::new();
        let orig = b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n";
        fs.write(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"), orig)
            .unwrap();
        fs.write(Path::new("E:/Projects/A/f.txt"), b"x").unwrap();
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
        apply(&plan, &fs, Path::new("/backup"), "T").unwrap();
        rollback(Path::new("/backup/cpm-T/manifest.json"), &fs).unwrap();
        assert!(fs.exists(Path::new("E:/Projects/A/f.txt")));
        assert!(!fs.exists(Path::new("E:/Projects/B/f.txt")));
        assert_eq!(
            fs.read(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"))
                .unwrap(),
            orig
        );
        // A rollback must leave no trace of the move: the new-encoded transcript dir created
        // by apply's rename must be gone, or rollback leaves exactly the orphan residue this
        // tool exists to detect (the old dir is restored above, so a lingering new dir is a
        // duplicate, not a recovery).
        assert!(
            !fs.exists(Path::new("/h/.claude/projects/E--Projects-B")),
            "new-encoded projects dir must be removed on rollback"
        );
    }
}
