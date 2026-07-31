use crate::error::{AwtError, Result};
use crate::fs::FileSystem;
use crate::model::VerifyResult;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Re-read every file named in a manifest's content entries and confirm that each is
/// byte-identical to the pre-migration original recorded in the manifest. Call this AFTER
/// `rollback` to produce a proof that the restore succeeded.
///
/// Returns one `VerifyResult` per content entry (non-empty sha256) plus one result for the
/// project folder if a move-tree occurred. Never short-circuits: all checks run regardless of
/// individual failures so the caller gets a complete picture.
pub fn verify_rollback(manifest_path: &Path, fs: &dyn FileSystem) -> Result<Vec<VerifyResult>> {
    let m = crate::backup::Manifest::load(fs, manifest_path)?;
    let mut out = Vec::new();

    // Folder-restoration check: present iff the plan included a move-tree step.
    let had_move_tree = m.entries.iter().any(|e| e.backup.starts_with("<move-tree"));
    if had_move_tree {
        let src = m.mv.src_abs.replace('\\', "/");
        let dst = m.mv.dst_abs.replace('\\', "/");
        let src_present = fs.exists(Path::new(&src));
        let dst_absent = !fs.exists(Path::new(&dst));
        let ok = src_present && dst_absent;
        out.push(VerifyResult {
            check: format!("folder restored {src}"),
            ok,
            detail: if ok {
                format!("src {src} present, dst {dst} absent")
            } else {
                format!(
                    "expected src {src} present ({src_present}) and dst {dst} absent ({})",
                    !dst_absent
                )
            },
        });
    }

    // Per-file content checks: skip markers (empty sha256 = not a content file).
    for e in &m.entries {
        if e.sha256.is_empty() {
            continue;
        }
        let check = format!("restored {}", e.original);
        let orig_path = Path::new(&e.original);
        if !fs.exists(orig_path) {
            out.push(VerifyResult {
                check,
                ok: false,
                detail: format!("file missing at {}", e.original),
            });
            continue;
        }
        match fs.read(orig_path) {
            Err(err) => {
                out.push(VerifyResult {
                    check,
                    ok: false,
                    detail: format!("read error at {}: {err}", e.original),
                });
            }
            Ok(bytes) => {
                let got: String = Sha256::digest(&bytes)
                    .iter()
                    .map(|x| format!("{x:02x}"))
                    .collect();
                let ok = got == e.sha256;
                out.push(VerifyResult {
                    check,
                    ok,
                    detail: if ok {
                        "byte-identical to pre-migration".into()
                    } else {
                        format!("expected {} got {}", e.sha256, got)
                    },
                });
            }
        }
    }

    Ok(out)
}

pub fn rollback(manifest_path: &Path, fs: &dyn FileSystem) -> Result<()> {
    let v: serde_json::Value = serde_json::from_slice(&fs.read(manifest_path)?)
        .map_err(|e| AwtError::UnrecognizedFormat(e.to_string()))?;
    let src = v["src_abs"].as_str().unwrap().replace('\\', "/");
    let dst = v["dst_abs"].as_str().unwrap().replace('\\', "/");
    let entries = v["entries"].as_array().unwrap();
    // 1. Move the real project FOLDER back, but ONLY if this run actually moved it. A folder
    //    move is recorded as a <move-tree> marker; an `associate` (which re-homes HISTORY via
    //    a merge or a rename but never moves the folder) records none. Without this gate, an
    //    associate into an EXISTING B - where B's folder is present on disk and A's is gone -
    //    would satisfy `exists(dst) && !exists(src)` and spuriously rename B's real folder onto
    //    A's path, corrupting B. Scope the move-back strictly to genuine folder moves.
    let had_move_tree = entries.iter().any(|e| {
        e["backup"]
            .as_str()
            .unwrap_or_default()
            .starts_with("<move-tree")
    });
    if had_move_tree && fs.exists(Path::new(&dst)) && !fs.exists(Path::new(&src)) {
        fs.rename(Path::new(&dst), Path::new(&src))?;
    }
    // 1.5. Rename each renamed project-state DIRECTORY back, wholesale, BEFORE any file
    //      restore (AC-54). The rename that apply performed left every byte on disk under the
    //      new name - sidecars included - so renaming the directory back is the only rollback
    //      that provably returns the COMPLETE tree, not just the manifested files. The first
    //      shipped version instead restored manifested files into a recreated old dir and then
    //      remove_dir_all'd the new dir, which destroyed every unbacked sidecar during the
    //      undo. Order matters: the restore in step 2 writes into the old paths, so the old
    //      dir must exist again first, and creating it by restore-then-delete is exactly the
    //      bug this replaces.
    let (old_enc, new_enc) = (
        crate::paths::encode_project_dir(v["src_abs"].as_str().unwrap()),
        crate::paths::encode_project_dir(v["dst_abs"].as_str().unwrap()),
    );
    for e in entries {
        if !e["backup"]
            .as_str()
            .unwrap_or_default()
            .starts_with("<dir-rename")
        {
            continue;
        }
        let old_dir = e["original"].as_str().unwrap();
        if !old_dir.contains(&old_enc) {
            continue;
        }
        let new_dir = old_dir.replace(&old_enc, &new_enc);
        let new_exists = fs.exists(Path::new(&new_dir));
        let old_exists = fs.exists(Path::new(old_dir));
        match (new_exists, old_exists) {
            // The normal post-apply state: rename the whole tree back.
            (true, false) => fs.rename(Path::new(&new_dir), Path::new(old_dir))?,
            // Apply failed before this dir was renamed: nothing to undo here.
            (false, true) | (false, false) => {}
            // Both exist: someone recreated the old dir since apply. Merging blind could
            // clobber either side; refuse loudly rather than guess.
            (true, true) => {
                return Err(AwtError::VerifyFailed(format!(
                    "rollback: both {old_dir} and {new_dir} exist; refusing to merge them - \
                     resolve manually, then re-run rollback"
                )));
            }
        }
    }
    // 2. restore each backed-up file to its original path (over the renamed-back tree, so
    //    rewritten files get their pre-migration bytes while untouched sidecars are already
    //    home)
    for e in entries {
        let original = e["original"].as_str().unwrap();
        let backup = e["backup"].as_str().unwrap();
        if backup.starts_with("<dir-rename")
            || backup.starts_with("<move-tree")
            || backup.starts_with("<merge-dir")
        {
            continue; // markers: whole-tree restore (above) or un-merge (below) handle these
        }
        let bytes = fs.read(Path::new(backup))?;
        let want = e["sha256"].as_str().unwrap_or_default();
        let got: String = Sha256::digest(&bytes)
            .iter()
            .map(|x| format!("{x:02x}"))
            .collect();
        if !want.is_empty() && got != want {
            return Err(AwtError::VerifyFailed(format!(
                "backup corrupted: {backup}"
            )));
        }
        fs.write(Path::new(original), &bytes)?;
    }
    // (The old step 3 - remove_dir_all on the new-encoded dir - is gone. Step 1.5's
    //  rename-back both restores the tree and leaves nothing to remove: the same operation
    //  that guarantees completeness also guarantees no orphan.)
    // 4. Un-merge (the merge counterpart of the dir handling above). For a merge into an existing B, step 2
    //    already restored A's files to their pre-merge paths under E--A; here we remove EXACTLY
    //    the copies that were merged into B, addressed by RELATIVE path, and nothing else. B's
    //    own transcripts were never backed up, so they never appear in `entries` and are never
    //    removed. This is deliberately NOT the wholesale remove_dir_all used for <dir-rename>
    //    above - doing that to a merge target would destroy B's own history. Any now-empty
    //    subdirectory left behind in B is acceptable v1 residue; removing B's dirs is not worth
    //    the risk. (Session filenames are UUIDs, so a relative path from A cannot alias one of
    //    B's own files; apply additionally refuses on any collision before moving.)
    for e in entries {
        if !e["backup"]
            .as_str()
            .unwrap_or_default()
            .starts_with("<merge-dir")
        {
            continue;
        }
        let from = e["original"].as_str().unwrap().replace('\\', "/");
        // B's projects dir is A's sibling, named for the destination encoding.
        let to = match Path::new(&from).parent() {
            Some(parent) => parent.join(&new_enc),
            None => PathBuf::from(&new_enc),
        };
        let prefix = format!("{from}/");
        for e2 in entries {
            // Only real content entries (nonempty sha256) name files that came from A; markers
            // and the merge marker itself carry an empty sha256 and are skipped.
            if e2["sha256"].as_str().unwrap_or_default().is_empty() {
                continue;
            }
            let orig2 = e2["original"].as_str().unwrap().replace('\\', "/");
            if let Some(rel) = orig2.strip_prefix(&prefix) {
                let merged = to.join(rel);
                if fs.exists(&merged) {
                    fs.remove_file(&merged)?;
                }
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
            move_folder: true,
            scope: crate::model::Scope::Standard,
        };
        let plan = build_plan(&fs, Path::new("/h"), &mv, &opts).unwrap();
        apply(&plan, &fs, Path::new("/backup"), "T").unwrap();
        rollback(Path::new("/backup/awt-T/manifest.json"), &fs).unwrap();
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

    // --- verify_rollback: positive test (all checks ok, anti-vacuity) ---

    #[test]
    fn verify_rollback_positive_all_checks_pass() {
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
            move_folder: true,
            scope: crate::model::Scope::Standard,
        };
        let plan = build_plan(&fs, Path::new("/h"), &mv, &opts).unwrap();
        apply(&plan, &fs, Path::new("/backup"), "T").unwrap();
        rollback(Path::new("/backup/awt-T/manifest.json"), &fs).unwrap();
        let results = verify_rollback(Path::new("/backup/awt-T/manifest.json"), &fs).unwrap();
        // Anti-vacuity: must have produced at least one check.
        assert!(
            !results.is_empty(),
            "verify_rollback returned an empty result set - vacuous proof"
        );
        // Every check must pass after a clean rollback.
        for r in &results {
            assert!(
                r.ok,
                "check failed after clean rollback: check={:?} detail={:?}",
                r.check, r.detail
            );
        }
    }

    // --- verify_rollback: negative test (tampered file flagged as failed) ---

    #[test]
    fn verify_rollback_negative_tampered_file_flagged() {
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
            move_folder: true,
            scope: crate::model::Scope::Standard,
        };
        let plan = build_plan(&fs, Path::new("/h"), &mv, &opts).unwrap();
        apply(&plan, &fs, Path::new("/backup"), "T").unwrap();
        rollback(Path::new("/backup/awt-T/manifest.json"), &fs).unwrap();
        // Tamper: overwrite the restored transcript with different bytes post-rollback.
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
            b"tampered content that does not match pre-migration hash",
        )
        .unwrap();
        let results = verify_rollback(Path::new("/backup/awt-T/manifest.json"), &fs).unwrap();
        let failed: Vec<_> = results.iter().filter(|r| !r.ok).collect();
        assert!(
            !failed.is_empty(),
            "tampered file must produce at least one failed check; all ok: {results:?}"
        );
        // The failed check detail must name the expected and actual hash.
        assert!(
            failed
                .iter()
                .any(|r| r.detail.contains("expected") && r.detail.contains("got")),
            "failed check detail must describe the hash mismatch: {failed:?}"
        );
    }
}
