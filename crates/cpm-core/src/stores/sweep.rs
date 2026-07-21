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
    /// Top-level regions under `~/.claude` the sweep must not read. A needle match inside
    /// any of these is never an actionable stale reference, so reading them only adds cost
    /// and noise - and they are also most of the files under `~/.claude`, so skipping them
    /// is what keeps `doctor` fast (the content-read of ~34k vendored + archival files was
    /// the whole of doctor's multi-minute cold run). Matched against the FIRST path
    /// component relative to `~/.claude`, never as a substring: a home under a directory
    /// called `projects` must still be swept (LEAD-07). Each region is skipped for one of
    /// three reasons:
    ///   - owned by a dedicated adapter that already reports it, so scanning here would
    ///     double-report: `projects` (claude.projects), `history.jsonl` (claude.history),
    ///     and `plugins/data` under `plugins` (plugin.state).
    ///   - archival - an old path is CORRECT by design and must never be surfaced as rot:
    ///     `backups` (rotated .claude.json restore points) and `file-history` (Claude's own
    ///     pre-edit file snapshots).
    ///   - vendored - third-party checkouts that are not our state and are reinstallable:
    ///     the rest of `plugins` (marketplaces/repos/cache).
    const SKIP_REGIONS: &'static [&'static str] = &[
        "projects",
        "history.jsonl",
        "plugins",
        "file-history",
        "backups",
    ];
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
        // SKIP_REGIONS names top-level regions under ~/.claude, so match the FIRST component
        // of the path relative to that root. A substring test over the absolute path reads
        // the user's home as if it were ours: a home under `/data/projects/` would make
        // every file look adapter-owned and silently disable the sweep. Lowercased because
        // the skipped regions are matched case-insensitively on NTFS (LEAD-07).
        let Ok(rel) = f.strip_prefix(&root) else {
            continue;
        };
        let first = rel
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().to_lowercase());
        if first.is_some_and(|c| Sweep::SKIP_REGIONS.contains(&c.as_str())) {
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
        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
            scope: crate::model::Scope::Standard,
        };
        let stale = sweep_for(&ctx, &["E:\\Gone\\project".into()]);
        assert!(stale.iter().any(|s| s.reference.contains("Gone")));
    }

    /// The sweep must NOT read `plugins/`, `file-history/`, or `backups/`. A needle match
    /// inside any of them is never actionable rot: backups and file-history keep old paths
    /// BY DESIGN (a rotated config or a pre-edit snapshot is supposed to name where things
    /// were), plugins/data has its own adapter, and the rest of plugins/ is vendored. Only a
    /// file in a region no adapter owns and no archive protects is genuine residue. Positive
    /// and negative live in one fixture on purpose: a sweep that skipped EVERYTHING would
    /// pass a test that only checked the three archives were quiet, so the lone genuine hit
    /// is the discriminator (the C-2 lesson).
    #[test]
    fn archival_and_vendored_regions_are_skipped_reporting_only_genuine_residue() {
        let fs = MemoryFileSystem::new();
        let body = b"last opened E:\\Gone\\project ok";
        // Three regions that must be skipped, each the shape it takes on a real machine:
        fs.write(
            Path::new("/h/.claude/plugins/repos/some-plugin/notes.txt"),
            body,
        )
        .unwrap();
        fs.write(Path::new("/h/.claude/file-history/sess-1/abc123@v2"), body)
            .unwrap();
        fs.write(
            Path::new("/h/.claude/backups/.claude.json.backup.1784334143067"),
            body,
        )
        .unwrap();
        // One region that is genuinely unowned residue and MUST still be reported.
        fs.write(Path::new("/h/.claude/stray-rename.log"), body)
            .unwrap();

        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
            scope: crate::model::Scope::Standard,
        };
        let stale = sweep_for(&ctx, &["E:\\Gone\\project".into()]);

        let locations: Vec<&str> = stale.iter().map(|s| s.location.as_str()).collect();
        assert_eq!(
            stale.len(),
            1,
            "only the genuine unowned residue may be reported: {locations:?}"
        );
        assert!(
            stale[0].location.contains("stray-rename.log"),
            "the single report must be the unowned log, not an archive: {locations:?}"
        );
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
        let idx = ProjectIndex::build(&fs, Path::new("/data/projects/home")).unwrap();
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
        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
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
        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
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
