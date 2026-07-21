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
    if let Some(m) = manifest {
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
            scope: crate::model::Scope::Standard,
        };
        let plan = build_plan(&fs, Path::new("/h"), &mv, &opts).unwrap();
        apply(&plan, &fs, Path::new("/backup"), "T").unwrap();
        let results = verify(&fs, Path::new("/h"), &mv, None).unwrap();
        assert!(results.iter().all(|r| r.ok), "{results:?}");
    }
}
