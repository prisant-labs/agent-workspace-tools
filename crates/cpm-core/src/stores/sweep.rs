use crate::error::Result;
use crate::model::{Change, Ctx, Hit, Move, Stale, Store, VerifyResult};

/// The report-only store for everything under `~/.claude` that no adapter owns.
///
/// Every `Store` method here is deliberately inert, and that inertness IS the safety
/// property: sweep is the one store aimed at regions CPM does not understand, so it
/// must be structurally incapable of writing rather than merely trusted not to. An
/// empty `plan` cannot emit a change, so no apply can act on a sweep finding. It stays
/// in `registry()` so `probe` covers it and the trait stays uniform.
///
/// The real reporting lives in the `sweep_for` free function below, which `doctor`
/// calls directly: a sweep needs the gone-path needles gathered from every OTHER
/// adapter's audit first, and the `Store::audit(&ctx)` signature cannot carry them.
/// This is why `audit` returns empty - not because it is unfinished.
pub struct Sweep;
impl Sweep {
    const ID: &'static str = "sweep.unknown";
    /// Top-level regions under `~/.claude` that have their own adapters. Matched against
    /// the first path component relative to `~/.claude`, never as a substring.
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

    /// Always empty. See the type doc: the real sweep is `sweep_for`, which `doctor`
    /// calls with needles this signature cannot carry.
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
        // OWNED names top-level regions under ~/.claude, so match the FIRST component of
        // the path relative to that root. A substring test over the absolute path reads
        // the user's home as if it were ours: a home under `/data/projects/` would make
        // every file look adapter-owned and silently disable the sweep. Lowercased because
        // the owned regions are matched case-insensitively on NTFS (LEAD-07).
        let Ok(rel) = f.strip_prefix(&root) else {
            continue;
        };
        let first = rel
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().to_lowercase());
        if first.is_some_and(|c| Sweep::OWNED.contains(&c.as_str())) {
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

    /// The adapter-owned regions are `~/.claude/projects/**` and `~/.claude/history.jsonl`,
    /// which is a fact about the path RELATIVE to `~/.claude` - not about the absolute path.
    /// A user whose home sits under a directory called `projects` must still get a sweep.
    #[test]
    fn owned_regions_are_matched_relative_to_claude_root_not_by_substring() {
        let fs = MemoryFileSystem::new();
        // home itself contains "projects", so every absolute path under it does too.
        fs.write(
            Path::new("/data/projects/home/.claude/some-plugin/notes.txt"),
            b"ref E:\\Gone\\project here",
        )
        .unwrap();
        let idx = ProjectIndex::build(&fs, Path::new("/data/projects/home"));
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/data/projects/home"),
            index: &idx,
            scope: crate::model::Scope::Standard,
        };
        let stale = sweep_for(&ctx, &["E:\\Gone\\project".into()]);
        assert_eq!(
            stale.len(),
            1,
            "a home under /data/projects must not read as adapter-owned"
        );
    }

    #[test]
    fn owned_regions_are_still_skipped() {
        let fs = MemoryFileSystem::new();
        // Both hold the needle; both are adapter-owned, so neither is the sweep's business.
        fs.write(
            Path::new("/h/.claude/projects/E--Gone-project/s.jsonl"),
            b"ref E:\\Gone\\project here",
        )
        .unwrap();
        fs.write(
            Path::new("/h/.claude/history.jsonl"),
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
        assert!(
            stale.is_empty(),
            "owned regions have their own adapters and must not be swept: {stale:?}"
        );
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
