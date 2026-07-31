use crate::backup::snapshot;
use crate::error::{AwtError, Result};
use crate::fs::FileSystem;
use crate::model::{Applied, Change};
use crate::plan::Plan;
use crate::report::Report;
use crate::rewrite::anchored_rewrite;
use std::path::Path;

pub struct ApplyOpts {
    pub run_id: String,
    pub auto_rollback: bool,
    pub force: bool,
}

pub fn apply(plan: &Plan, fs: &dyn FileSystem, backup_root: &Path, run_id: &str) -> Result<Report> {
    let _m = snapshot(plan, fs, backup_root, run_id)?;
    let mut applied = Vec::new();

    // 1. rename dirs first (so post-rename RewriteFile paths resolve), except MoveTree.
    //    MergeDir is handled here too (before pass-2 rewrites the moved *.jsonl in place).
    for c in &plan.changes {
        match c {
            Change::RenameDir { from, to } => {
                if fs.exists(from) {
                    fs.rename(from, to)?;
                }
                applied.push(Applied {
                    change: format!("rename {} -> {}", from.display(), to.display()),
                    counts: 0,
                });
            }
            Change::MergeDir { from, to } => {
                // Move every file under `from` into the existing `to`, preserving relative
                // sub-paths. Refuse to overwrite any file B already has (never clobber B's
                // history) - session filenames are UUIDs so this should never fire, but if it
                // does we stop loudly rather than lose data. Then remove the now-empty `from`.
                let mut moved = 0usize;
                for file in crate::fs::walk_files_strict(fs, from)? {
                    let rel = file.strip_prefix(from).map_err(|e| {
                        AwtError::VerifyFailed(format!(
                            "merge: {} not under {}: {e}",
                            file.display(),
                            from.display()
                        ))
                    })?;
                    let dest = to.join(rel);
                    if fs.exists(&dest) {
                        return Err(AwtError::VerifyFailed(format!(
                            "merge collision: {} exists",
                            dest.display()
                        )));
                    }
                    if let Some(parent) = dest.parent() {
                        fs.create_dir_all(parent)?;
                    }
                    fs.rename(&file, &dest)?;
                    moved += 1;
                }
                if fs.exists(from) {
                    fs.remove_dir_all(from)?;
                }
                applied.push(Applied {
                    change: format!("merge {} -> {}", from.display(), to.display()),
                    counts: moved,
                });
            }
            _ => {}
        }
    }
    // 2. rewrites and json edits
    for c in &plan.changes {
        match c {
            Change::RewriteFile {
                path,
                rules,
                expected,
            } => {
                let bytes = fs.read(path)?;
                let text = std::str::from_utf8(&bytes).map_err(|e| {
                    AwtError::UnrecognizedFormat(format!("{}: {e}", path.display()))
                })?;
                let (out, n) = anchored_rewrite(text, rules);
                if n != *expected {
                    return Err(AwtError::VerifyFailed(format!(
                        "{}: expected {expected} edits, live count {n}",
                        path.display()
                    )));
                }
                fs.write(path, out.as_bytes())?;
                applied.push(Applied {
                    change: format!("rewrite {}", path.display()),
                    counts: n,
                });
            }
            Change::RenameJsonKey {
                path,
                from,
                to,
                expected,
            }
            | Change::RewriteJsonArrayValue {
                path,
                from,
                to,
                expected,
            } => {
                let bytes = fs.read(path)?;
                let text = std::str::from_utf8(&bytes).map_err(|e| {
                    AwtError::UnrecognizedFormat(format!("{}: {e}", path.display()))
                })?;
                let n = text.matches(from.as_str()).count();
                if n != *expected {
                    return Err(AwtError::VerifyFailed(format!(
                        "{}: expected {expected}, live {n}",
                        path.display()
                    )));
                }
                fs.write(path, text.replace(from.as_str(), to.as_str()).as_bytes())?;
                applied.push(Applied {
                    change: format!("json {}", path.display()),
                    counts: n,
                });
            }
            _ => {}
        }
    }
    // 3. move tree LAST
    for c in &plan.changes {
        if let Change::MoveTree { from, to } = c {
            // A missing source here is fatal, never a skip (AC-55). The plan guaranteed the
            // source existed at plan time; if it vanished since (concurrent delete, ejected
            // volume), recording the move as applied would be a false success on the user's
            // most trust-sensitive claim. Failing returns Err, which apply_verified turns
            // into an auto-rollback of the store rewrites already made above.
            if !fs.exists(from) {
                return Err(AwtError::SourceMissing(format!(
                    "{} vanished between plan and apply",
                    from.display()
                )));
            }
            fs.rename(from, to)?;
            applied.push(Applied {
                change: format!("move {} -> {}", from.display(), to.display()),
                counts: 0,
            });
        }
    }
    Ok(Report {
        run_id: run_id.to_string(),
        applied,
        backup_dir: format!("awt-{run_id}"),
        verify: None,
    })
}

pub fn apply_verified(
    plan: &Plan,
    fs: &dyn FileSystem,
    backup_root: &Path,
    opts: &ApplyOpts,
) -> Result<Report> {
    let backup_dir = backup_root.join(format!("awt-{}", opts.run_id));
    let manifest_path = backup_dir.join("manifest.json");
    let mut report = match apply(plan, fs, backup_root, &opts.run_id) {
        Ok(r) => r,
        Err(e) => {
            if opts.auto_rollback {
                let _ = crate::rollback::rollback(&manifest_path, fs);
            }
            return Err(AwtError::VerifyFailed(format!(
                "apply failed ({e:?}); backup at {}",
                backup_dir.display()
            )));
        }
    };
    let manifest = crate::backup::Manifest::load(fs, &manifest_path)?;
    // A verify that ERRORS is treated exactly like a verify that fails (AC-59): the apply's
    // postconditions are unproven either way, and an unproven migration must not stand. The
    // previous `?` here bubbled the error up PAST the rollback branch, stranding the move in
    // a "done but unverifiable" state.
    let results = match crate::verify::verify(fs, &plan.home, &plan.mv, Some(&manifest), Some(plan))
    {
        Ok(r) => r,
        Err(e) => {
            if opts.auto_rollback {
                crate::rollback::rollback(&manifest_path, fs)?;
            }
            return Err(AwtError::VerifyFailed(format!(
                "verification could not run ({e}); the apply was rolled back; backup at {}",
                backup_dir.display()
            )));
        }
    };
    let failed: Vec<_> = results.iter().filter(|r| !r.ok).collect();
    if !failed.is_empty() {
        if opts.auto_rollback {
            crate::rollback::rollback(&manifest_path, fs)?;
        }
        return Err(AwtError::VerifyFailed(format!(
            "{} checks failed; backup at {}",
            failed.len(),
            backup_dir.display()
        )));
    }
    report.verify = Some(results);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AwtError;
    use crate::fs::{FileSystem, MemoryFileSystem};
    use crate::model::{Change, Move};
    use crate::plan::{build_plan, Collision, PlanOpts};
    use std::path::Path;

    #[test]
    fn apply_rewrites_cwd_and_moves_folder_last() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
            b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n",
        )
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
        let moved = fs
            .read(Path::new("/h/.claude/projects/E--Projects-B/s.jsonl"))
            .unwrap();
        assert!(String::from_utf8_lossy(&moved).contains("E:\\\\Projects\\\\B"));
        assert!(fs.exists(Path::new("E:/Projects/B/f.txt")));
        assert!(!fs.exists(Path::new("E:/Projects/A/f.txt")));
    }

    #[test]
    fn apply_verified_rolls_back_on_failure() {
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
        let mut plan = build_plan(&fs, Path::new("/h"), &mv, &opts).unwrap();
        // Inject an impossible expected count so apply's count-guard trips mid-apply (after the
        // dir rename, before the folder move), forcing apply_verified onto its rollback path.
        let mut injected = false;
        for c in &mut plan.changes {
            if let Change::RewriteFile { expected, .. } = c {
                *expected += 999;
                injected = true;
            }
        }
        assert!(injected, "fixture must produce a RewriteFile to corrupt");
        let aopts = ApplyOpts {
            run_id: "T".into(),
            auto_rollback: true,
            force: false,
        };
        let err = apply_verified(&plan, &fs, Path::new("/backup"), &aopts).unwrap_err();
        assert!(matches!(err, AwtError::VerifyFailed(_)), "{err:?}");
        // pre-move state restored: source folder intact, dest absent, old transcript byte-restored
        assert!(fs.exists(Path::new("E:/Projects/A/f.txt")));
        assert!(!fs.exists(Path::new("E:/Projects/B/f.txt")));
        assert_eq!(
            fs.read(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"))
                .unwrap(),
            orig
        );
    }

    #[test]
    fn second_apply_is_noop() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
            b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n",
        )
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
        let aopts = ApplyOpts {
            run_id: "T".into(),
            auto_rollback: true,
            force: false,
        };
        apply_verified(&plan, &fs, Path::new("/backup"), &aopts).unwrap();
        // After a completed move the dest folder exists, so re-planning trips the guard:
        // that DestinationExists is the v1 idempotency signal.
        let err = build_plan(&fs, Path::new("/h"), &mv, &opts).unwrap_err();
        assert!(
            matches!(err, crate::error::AwtError::DestinationExists(_)),
            "{err:?}"
        );
    }

    #[test]
    fn corrupt_claude_json_hard_fails_before_writing() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("/h/.claude.json"), b"{ not json")
            .unwrap();
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
            b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n",
        )
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
        let err = build_plan(&fs, Path::new("/h"), &mv, &opts).unwrap_err();
        assert!(matches!(err, AwtError::UnrecognizedFormat(_)));
        assert!(!fs.exists(Path::new("E:/Projects/B/f.txt"))); // nothing moved
    }
}
