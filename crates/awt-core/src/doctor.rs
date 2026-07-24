//! The read-only engine behind `awt doctor` and `awt scan`.
//!
//! Neither function writes. `doctor` answers "what state points at folders that are
//! gone?", `scan` answers "what state points at THIS folder?".

use crate::error::Result;
use crate::fs::FileSystem;
use crate::index::ProjectIndex;
use crate::model::{Ctx, Hit, Move, Scope, Stale};
use crate::stores::{registry, sweep::sweep_for};
use std::path::{Path, PathBuf};

pub struct DoctorReport {
    /// Path-keyed references an adapter owns and Phase 5 can rewrite in place.
    pub stale: Vec<Stale>,
    /// Findings from the sweep of unowned regions. Surfaced for the user's awareness but
    /// never rewritten - kept in a separate vector so the rewrite path (which consumes
    /// `stale`) is structurally incapable of touching a region no adapter understands.
    pub report_only: Vec<Stale>,
    pub unresolved: Vec<PathBuf>,
}

pub struct ScanReport {
    pub hits: Vec<Hit>,
}

fn ctx_for<'a>(fs: &'a dyn FileSystem, home: &Path, index: &'a ProjectIndex) -> Ctx<'a> {
    Ctx {
        fs,
        home: home.to_path_buf(),
        index,
        scope: Scope::Standard,
    }
}

/// Report every path-keyed reference to a folder that no longer exists.
pub fn doctor(fs: &dyn FileSystem, home: &Path) -> Result<DoctorReport> {
    let index = ProjectIndex::build(fs, home)?;
    let ctx = ctx_for(fs, home, &index);

    let mut stale = Vec::new();
    for store in registry() {
        // probe first: an unrecognized shape must abort the whole run rather than
        // produce a half-report that reads as authoritative.
        store.probe(&ctx)?;
        stale.extend(store.audit(&ctx)?);
    }

    // Every adapter has now named what it knows to be dead. Only now can the sweep run:
    // it searches the regions no adapter owns for those same references, and it needs the
    // full set to do it. This is why the sweep is a free function and not the Store::audit
    // above - that signature cannot carry the needles. Deduped because several dirs can
    // name one dead path, and a needle repeated is a finding repeated.
    let mut needles: Vec<String> = stale.iter().map(|s| s.reference.clone()).collect();
    needles.sort();
    needles.dedup();
    // Sweep results are kept apart from `stale` on purpose: they name regions no adapter
    // owns, so they are reported but must never be rewritten. See DoctorReport::report_only.
    let report_only = sweep_for(&ctx, &needles);

    Ok(DoctorReport {
        stale,
        report_only,
        unresolved: index.unresolved.clone(),
    })
}

/// List every reference to `src_abs`, across every store.
pub fn scan(fs: &dyn FileSystem, home: &Path, src_abs: &str) -> Result<ScanReport> {
    let index = ProjectIndex::build(fs, home)?;
    let ctx = ctx_for(fs, home, &index);

    // detect() reads only src_abs; the empty dst is never consulted on this path.
    let mv = Move {
        src_abs: src_abs.to_string(),
        dst_abs: String::new(),
    };
    let mut hits = Vec::new();
    for store in registry() {
        store.probe(&ctx)?;
        hits.extend(store.detect(&ctx, &mv)?);
    }
    Ok(ScanReport { hits })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemoryFileSystem;
    use std::path::Path;

    fn cwd_line(cwd: &str) -> String {
        format!(
            "{{\"type\":\"user\",\"cwd\":\"{}\"}}\n",
            cwd.replace('\\', "\\\\")
        )
    }

    #[test]
    fn scan_lists_hits_for_a_project() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
            cwd_line("E:\\Projects\\A").as_bytes(),
        )
        .unwrap();
        fs.write(
            Path::new("/h/.claude.json"),
            br#"{"projects":{"E:\\Projects\\A":{}}}"#,
        )
        .unwrap();

        let rep = scan(&fs, Path::new("/h"), "E:\\Projects\\A").unwrap();

        assert!(
            rep.hits.iter().any(|h| h.store == "claude.projects"),
            "expected a projects-dir hit: {:?}",
            rep.hits
        );
        assert!(
            rep.hits.iter().any(|h| h.store == "claude.json"),
            "expected a claude.json key hit: {:?}",
            rep.hits
        );
    }

    /// Both halves live in one test on purpose. "Unrelated path yields nothing" is a claim
    /// a scan that finds nothing at all also satisfies, so on its own it proves nothing
    /// (the mistake behind audit finding C-2). Pairing it with the positive case over the
    /// same fixture is what makes it a test of discrimination.
    #[test]
    fn scan_distinguishes_the_named_project_from_an_unrelated_path() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
            cwd_line("E:\\Projects\\A").as_bytes(),
        )
        .unwrap();
        fs.write(
            Path::new("/h/.claude.json"),
            br#"{"projects":{"E:\\Projects\\A":{}}}"#,
        )
        .unwrap();

        let named = scan(&fs, Path::new("/h"), "E:\\Projects\\A").unwrap();
        let other = scan(&fs, Path::new("/h"), "E:\\Projects\\Unrelated").unwrap();

        assert!(
            !named.hits.is_empty(),
            "the project every store knows must produce hits"
        );
        assert!(
            other.hits.is_empty(),
            "a path no store knows must produce no hits: {:?}",
            other.hits
        );
    }

    /// doctor aggregates across adapters AND across the unowned regions the sweep covers.
    /// The live project is the discriminator: a doctor that reports everything is as
    /// useless as one that reports nothing, and only the live row proves the difference.
    /// The sweep's finding must land in `report_only`, never in `stale`: `stale` is the
    /// vector Phase 5 will rewrite, and an unowned region must be structurally unreachable
    /// from that path, not merely trusted to be left alone.
    #[test]
    fn doctor_reports_dead_references_across_stores_and_leaves_the_live_one_alone() {
        let fs = MemoryFileSystem::new();

        // A live project: on disk, recorded in a transcript, keyed in claude.json.
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-Live/s.jsonl"),
            cwd_line("E:\\Projects\\Live").as_bytes(),
        )
        .unwrap();
        fs.write(Path::new("E:\\Projects\\Live\\README.md"), b"live")
            .unwrap();

        // A dead project: transcripts and claude.json still name a path that is gone.
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-Gone/s.jsonl"),
            cwd_line("E:\\Projects\\Gone").as_bytes(),
        )
        .unwrap();
        fs.write(
            Path::new("/h/.claude.json"),
            br#"{"projects":{"E:\\Projects\\Live":{},"E:\\Projects\\Gone":{}}}"#,
        )
        .unwrap();
        // ...and so does a file in a region no adapter owns, which only the sweep sees.
        fs.write(
            Path::new("/h/.claude/some-plugin/notes.txt"),
            b"last opened E:\\Projects\\Gone ok",
        )
        .unwrap();

        let rep = doctor(&fs, Path::new("/h")).unwrap();

        let stores: Vec<&str> = rep.stale.iter().map(|s| s.store).collect();
        assert!(
            stores.contains(&"claude.projects"),
            "the projects dir still names a gone path: {stores:?}"
        );
        assert!(
            stores.contains(&"claude.json"),
            "the claude.json key still names a gone path: {stores:?}"
        );
        // The sweep finding is report-only: it must be carried out of the unowned region,
        // but in `report_only`, not `stale` - the rewrite path must never see it.
        assert!(
            !stores.contains(&"sweep.unknown"),
            "a sweep finding must not sit in the rewritable `stale` vector: {stores:?}"
        );
        assert!(
            rep.report_only.iter().any(|s| s.store == "sweep.unknown"),
            "the sweep must carry the gone path out of the unowned region, into report_only: {:?}",
            rep.report_only
        );
        assert!(
            rep.stale
                .iter()
                .chain(rep.report_only.iter())
                .all(|s| !s.reference.contains("Live")),
            "the live project must never be reported: stale={:?} report_only={:?}",
            rep.stale,
            rep.report_only
        );
    }
}
