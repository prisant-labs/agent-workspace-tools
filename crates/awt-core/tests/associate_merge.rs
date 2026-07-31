use awt_core::apply::{apply_verified, ApplyOpts};
use awt_core::associate::{associate, AssociateOpts};
use awt_core::fs::{FileSystem, MemoryFileSystem};
use awt_core::model::{Change, Move};
use awt_core::plan::{build_plan, PlanOpts};
use std::path::Path;

// The two projects dirs and B's own transcript bytes, reused by both tests.
const A1: &[u8] = b"{\"type\":\"user\",\"cwd\":\"E:\\\\A\",\"uuid\":\"a\"}\n";
const B1: &[u8] = b"{\"type\":\"user\",\"cwd\":\"E:\\\\B\",\"uuid\":\"b\"}\n";

fn seed_merge_scenario() -> MemoryFileSystem {
    let fs = MemoryFileSystem::new();
    // A's history: one transcript whose cwd is E:\A.
    fs.write(Path::new("/h/.claude/projects/E--A/a1.jsonl"), A1)
        .unwrap();
    // B's EXISTING history: one transcript whose cwd is E:\B. This must survive untouched.
    fs.write(Path::new("/h/.claude/projects/E--B/b1.jsonl"), B1)
        .unwrap();
    // B exists on disk (its projects dir is live, not gone).
    fs.write(Path::new("E:/B/keep.txt"), b"x").unwrap();
    fs
}

/// The core capability: associating A -> B when B already has its own history must MERGE
/// A's transcripts into B's dir (rewriting cwd A -> B) and keep B's own transcripts intact.
#[test]
fn associate_merges_into_existing_destination() {
    let fs = seed_merge_scenario();
    let opts = AssociateOpts {
        reassociate: true,
        export: false,
        export_subdir: ".claude-sessions".into(),
        run_id: "MERGE".into(),
    };
    associate(&fs, Path::new("/h"), "E:\\A", "E:\\B", &opts).unwrap();

    // A's transcript now lives under B, with its cwd rewritten to E:\B and no trace of E:\A.
    let a_moved = fs
        .read(Path::new("/h/.claude/projects/E--B/a1.jsonl"))
        .expect("A's transcript must now live under B");
    let a_text = String::from_utf8_lossy(&a_moved);
    assert!(
        a_text.contains("E:\\\\B") && !a_text.contains("E:\\\\A"),
        "merged transcript must be rewritten to B, got: {a_text}"
    );

    // B's OWN transcript is present and byte-identical (never rewritten, never lost).
    assert_eq!(
        fs.read(Path::new("/h/.claude/projects/E--B/b1.jsonl"))
            .unwrap(),
        B1,
        "B's own transcript must be byte-identical after the merge"
    );

    // A's projects dir is gone (its files were moved, not left behind).
    assert!(
        !fs.exists(Path::new("/h/.claude/projects/E--A")),
        "A's projects dir must no longer exist after the merge"
    );
}

/// The core SAFETY property: a merge that fails mid-flight must roll back so that A is fully
/// restored and B is left EXACTLY as it was - B's own transcript intact and A's copy removed.
#[test]
fn associate_merge_rolls_back_cleanly() {
    let fs = seed_merge_scenario();
    let mv = Move {
        src_abs: "E:\\A".into(),
        dst_abs: "E:\\B".into(),
    };
    // Mirror exactly what `associate` plans: no folder move, Standard scope.
    let plan_opts = PlanOpts {
        force: false,
        move_folder: false,
    };
    let mut plan = build_plan(&fs, Path::new("/h"), &mv, &plan_opts).unwrap();
    // Confirm we are actually testing the MERGE path, not a plain rename.
    assert!(
        plan.changes
            .iter()
            .any(|c| matches!(c, Change::MergeDir { .. })),
        "expected a MergeDir change when B already exists; got {:?}",
        plan.changes
    );
    // Force the apply to fail AFTER the merge move, before the rewrite completes, by demanding
    // an impossible edit count (same technique as apply's own rollback test).
    let mut injected = false;
    for c in &mut plan.changes {
        if let Change::RewriteFile { expected, .. } = c {
            *expected += 999;
            injected = true;
        }
    }
    assert!(injected, "fixture must produce a RewriteFile to corrupt");

    let aopts = ApplyOpts {
        run_id: "MERGE".into(),
        auto_rollback: true,
        force: false,
    };
    let err = apply_verified(&plan, &fs, Path::new("/backup"), &aopts).unwrap_err();
    assert!(
        matches!(err, awt_core::error::AwtError::VerifyFailed(_)),
        "{err:?}"
    );

    // A is fully restored: its transcript is back under E--A with the ORIGINAL bytes.
    assert_eq!(
        fs.read(Path::new("/h/.claude/projects/E--A/a1.jsonl"))
            .expect("A's transcript must be restored on rollback"),
        A1,
        "A's transcript must be byte-restored on rollback"
    );

    // B is uncorrupted: its own transcript is present and byte-identical...
    assert_eq!(
        fs.read(Path::new("/h/.claude/projects/E--B/b1.jsonl"))
            .expect("B's own transcript must survive rollback"),
        B1,
        "B's own transcript must be byte-identical after rollback"
    );
    // ...and A's copy has been removed from B (B holds ONLY its own transcript again).
    assert!(
        !fs.exists(Path::new("/h/.claude/projects/E--B/a1.jsonl")),
        "A's merged copy must be removed from B on rollback"
    );
    // B's real project folder must NOT have been touched by the rollback.
    assert!(
        fs.exists(Path::new("E:/B/keep.txt")),
        "B's real project folder must be untouched by rollback"
    );
}
