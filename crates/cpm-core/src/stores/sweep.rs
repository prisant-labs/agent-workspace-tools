use crate::error::Result;
use crate::model::{Change, Ctx, Hit, Move, Stale, Store, VerifyResult};

pub struct Sweep;
impl Sweep {
    const ID: &'static str = "sweep.unknown";
    const OWNED: &'static [&'static str] = &["projects", "history.jsonl"];
    const SKIP_EXT: &'static [&'static str] = &[
        "db", "sqlite", "png", "jpg", "zip", "gz", "wasm", "exe", "dll",
    ];
}

impl Store for Sweep {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn probe(&self, _ctx: &Ctx) -> Result<()> {
        Ok(())
    }

    fn detect(&self, _ctx: &Ctx, _mv: &Move) -> Result<Vec<Hit>> {
        Ok(vec![])
    }

    fn audit(&self, _ctx: &Ctx) -> Result<Vec<Stale>> {
        Ok(vec![])
    }

    fn plan(&self, _ctx: &Ctx, _mv: &Move, _hit: &Hit) -> Result<Vec<Change>> {
        Ok(vec![])
    }

    fn verify(&self, _ctx: &Ctx, _mv: &Move) -> Result<Vec<VerifyResult>> {
        Ok(vec![])
    }
}

/// Free function used by doctor: walk unowned text files under ~/.claude and
/// report those containing any needle. Report-only.
pub fn sweep_for(ctx: &Ctx, needles: &[String]) -> Vec<Stale> {
    let root = ctx.home.join(".claude");
    let mut out = Vec::new();
    for f in ctx.fs_walk_text(&root) {
        if Sweep::OWNED.iter().any(|o| f.to_string_lossy().contains(o)) {
            continue;
        }
        if let Some(ext) = f.extension().and_then(|e| e.to_str()) {
            if Sweep::SKIP_EXT.contains(&ext) {
                continue;
            }
        }
        if let Ok(bytes) = ctx.fs.read(&f) {
            let text = String::from_utf8_lossy(&bytes);
            for n in needles {
                if text.contains(n.as_str()) {
                    out.push(Stale {
                        store: Sweep::ID,
                        reference: n.clone(),
                        location: f.to_string_lossy().into_owned(),
                    });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, MemoryFileSystem};
    use crate::index::ProjectIndex;
    use std::path::{Path, PathBuf};

    #[test]
    fn audit_reports_stale_path_in_unowned_file() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/some-plugin/notes.txt"),
            b"ref E:\\Gone\\project here",
        )
        .unwrap();
        let idx = ProjectIndex::build(&fs, Path::new("/h"));
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
            scope: crate::model::Scope::Standard,
        };
        let stale = sweep_for(&ctx, &["E:\\Gone\\project".into()]);
        assert!(stale.iter().any(|s| s.reference.contains("Gone")));
    }

    #[test]
    fn plan_returns_no_changes() {
        let fs = MemoryFileSystem::new();
        let idx = ProjectIndex::build(&fs, Path::new("/h"));
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
            scope: crate::model::Scope::Standard,
        };
        let mv = Move {
            src_abs: "E:\\Old".into(),
            dst_abs: "E:\\New".into(),
        };
        let sweep = Sweep;
        let changes = sweep
            .plan(
                &ctx,
                &mv,
                &Hit {
                    store: "test",
                    detail: "test".into(),
                    target: PathBuf::new(),
                },
            )
            .unwrap();
        assert!(changes.is_empty());
    }
}
