use crate::backup::snapshot;
use crate::error::{CpmError, Result};
use crate::fs::FileSystem;
use crate::model::{Applied, Change};
use crate::plan::Plan;
use crate::report::Report;
use crate::rewrite::anchored_rewrite;
use std::path::Path;

pub fn apply(plan: &Plan, fs: &dyn FileSystem, backup_root: &Path, run_id: &str) -> Result<Report> {
    let _m = snapshot(plan, fs, backup_root, run_id)?;
    let mut applied = Vec::new();

    // 1. rename dirs first (so post-rename RewriteFile paths resolve), except MoveTree
    for c in &plan.changes {
        if let Change::RenameDir { from, to } = c {
            if fs.exists(from) {
                fs.rename(from, to)?;
            }
            applied.push(Applied {
                change: format!("rename {} -> {}", from.display(), to.display()),
                counts: 0,
            });
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
                    CpmError::UnrecognizedFormat(format!("{}: {e}", path.display()))
                })?;
                let (out, n) = anchored_rewrite(text, rules);
                if n != *expected {
                    return Err(CpmError::VerifyFailed(format!(
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
                    CpmError::UnrecognizedFormat(format!("{}: {e}", path.display()))
                })?;
                let n = text.matches(from.as_str()).count();
                if n != *expected {
                    return Err(CpmError::VerifyFailed(format!(
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
            if fs.exists(from) {
                fs.rename(from, to)?;
            }
            applied.push(Applied {
                change: format!("move {} -> {}", from.display(), to.display()),
                counts: 0,
            });
        }
    }
    Ok(Report {
        run_id: run_id.to_string(),
        applied,
        backup_dir: format!("cpm-{run_id}"),
        verify: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, MemoryFileSystem};
    use crate::model::Move;
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
}
