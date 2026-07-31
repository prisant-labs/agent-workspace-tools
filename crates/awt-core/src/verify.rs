use crate::backup::Manifest;
use crate::error::Result;
use crate::fs::FileSystem;
use crate::index::ProjectIndex;
use crate::model::{Ctx, Move, VerifyResult};
use crate::stores::registry;
use std::path::{Path, PathBuf};

pub fn verify(
    fs: &dyn FileSystem,
    home: &Path,
    mv: &Move,
    manifest: Option<&Manifest>,
    plan: Option<&crate::plan::Plan>,
) -> Result<Vec<VerifyResult>> {
    let index = ProjectIndex::build(fs, home)?;
    let ctx = Ctx {
        fs,
        home: home.to_path_buf(),
        index: &index,
        scope: crate::model::Scope::Standard,
    };
    let mut out = Vec::new();
    for store in registry() {
        out.extend(store.verify(&ctx, mv)?);
    }
    // Plan-derived splice checks (AC-57): when the caller can supply the plan that was
    // applied, verify each planned json splice against the file's actual bytes - the
    // destination anchor must be present and the source anchor gone. The store-level checks
    // above only assert old-key ABSENCE, so sabotage (or a bug) that removes both old and
    // new keys verified green before this. Anchors are matched as raw bytes because the
    // splice wrote raw bytes; parsed-level checks reintroduce the AR-01 blind spot.
    if let Some(p) = plan {
        for c in &p.changes {
            if let crate::model::Change::RenameJsonKey { path, from, to, .. }
            | crate::model::Change::RewriteJsonArrayValue { path, from, to, .. } = c
            {
                match fs.read(path) {
                    Err(e) => out.push(VerifyResult {
                        check: "planned json edit verifiable".into(),
                        ok: false,
                        detail: format!("{}: read failed: {e}", path.display()),
                    }),
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        out.push(VerifyResult {
                            check: "planned json edit landed".into(),
                            ok: text.contains(to.as_str()) && !text.contains(from.as_str()),
                            detail: format!(
                                "{}: expected {to} present and {from} absent",
                                path.display()
                            ),
                        });
                    }
                }
            }
        }
    }
    if let Some(m) = manifest {
        // Folder postcondition (AC-55): when the plan actually moved the project folder (a
        // <move-tree> marker is in the manifest), the move's most basic claim must be checked
        // from disk: the destination is a directory and the source is gone. Before this check,
        // every store could verify green while no folder move had happened at all. Scoped to
        // manifest-carrying verifies because only the manifest knows whether this run was a
        // folder move or an associate (which legitimately has no source folder).
        if m.entries.iter().any(|e| e.backup.starts_with("<move-tree")) {
            let src = mv.src_abs.replace('\\', "/");
            let dst = mv.dst_abs.replace('\\', "/");
            let dst_ok = fs.is_dir(Path::new(&dst));
            out.push(VerifyResult {
                check: "project folder present at destination".into(),
                ok: dst_ok,
                detail: if dst_ok {
                    dst.clone()
                } else {
                    format!("{dst} is not a directory")
                },
            });
            let src_gone = !fs.exists(Path::new(&src));
            out.push(VerifyResult {
                check: "project folder absent at source".into(),
                ok: src_gone,
                detail: if src_gone {
                    src.clone()
                } else {
                    format!("{src} still exists")
                },
            });
        }
        for e in &m.entries {
            if !e.original.ends_with(".jsonl") || e.sha256.is_empty() {
                continue;
            }
            let want = std::str::from_utf8(&fs.read(Path::new(&e.backup))?)
                .map(|t| t.lines().count())
                .unwrap_or(0);
            let moved = moved_path(&e.original, mv);
            let got = fs
                .read(&moved)
                .ok()
                .and_then(|b| String::from_utf8(b).ok())
                .map(|t| t.lines().count());
            out.push(VerifyResult {
                check: "transcript line count unchanged vs backup".into(),
                ok: got == Some(want),
                detail: e.original.clone(),
            });
        }
    }
    Ok(out)
}

/// Map a PRE-rename transcript path to its POST-move location by swapping the old encoded
/// dir segment for the new one.
fn moved_path(original: &str, mv: &Move) -> PathBuf {
    use crate::paths::encode_project_dir;
    let (old_enc, new_enc) = (
        encode_project_dir(&mv.src_abs),
        encode_project_dir(&mv.dst_abs),
    );
    PathBuf::from(original.replace(&old_enc, &new_enc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::apply;
    use crate::fs::{FileSystem, MemoryFileSystem};
    use crate::model::Move;
    use crate::plan::{build_plan, Collision, PlanOpts};
    use std::path::Path;

    fn setup() -> (MemoryFileSystem, Move) {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
            b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n",
        )
        .unwrap();
        fs.write(Path::new("E:/Projects/A/f.txt"), b"x").unwrap();
        (
            fs,
            Move {
                src_abs: "E:\\Projects\\A".into(),
                dst_abs: "E:\\Projects\\B".into(),
            },
        )
    }

    #[test]
    fn verify_passes_after_apply() {
        let (fs, mv) = setup();
        let opts = PlanOpts {
            recursive: false,
            on_collision: Collision::Refuse,
            force: false,
            move_folder: true,
            scope: crate::model::Scope::Standard,
        };
        let plan = build_plan(&fs, Path::new("/h"), &mv, &opts).unwrap();
        apply(&plan, &fs, Path::new("/backup"), "T").unwrap();
        let results = verify(&fs, Path::new("/h"), &mv, None, None).unwrap();
        assert!(results.iter().all(|r| r.ok), "{results:?}");
    }
}
