//! S-04 phases 17.5-17.7: AC-59 (I/O failure is never absence), AC-57 (verify derives from
//! the plan), AC-60 (plugin hash uses the recorded spelling). Red-first: each behavioral test
//! failed against the pre-fix code.

use awt_core::apply::{apply, apply_verified, ApplyOpts};
use awt_core::fs::{FileSystem, MemoryFileSystem};
use awt_core::model::{Move, Scope};
use awt_core::plan::{build_plan, Collision, PlanOpts};
use std::io;
use std::path::{Path, PathBuf};

const HOME: &str = "/h";

fn opts() -> PlanOpts {
    PlanOpts {
        recursive: false,
        on_collision: Collision::Refuse,
        force: false,
        move_folder: true,
        scope: Scope::Standard,
    }
}

fn mv() -> Move {
    Move {
        src_abs: r"E:\Projects\A".into(),
        dst_abs: r"E:\Projects\B".into(),
    }
}

/// A FileSystem that behaves exactly like MemoryFileSystem until a path containing the
/// configured needle is read (or read_dir'd), then fails with PermissionDenied. This is the
/// injection half of AC-59: without it, "a read failed" and "there was nothing there" are
/// indistinguishable in tests, exactly as they were in the shipped code.
struct FailingFs {
    inner: MemoryFileSystem,
    fail_read_containing: Option<String>,
    fail_read_dir_containing: Option<String>,
}

impl FailingFs {
    fn denied(p: &Path) -> io::Error {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("injected: {}", p.display()),
        )
    }
    fn read_hits(&self, p: &Path) -> bool {
        self.fail_read_containing
            .as_deref()
            .is_some_and(|n| p.to_string_lossy().replace('\\', "/").contains(n))
    }
    fn read_dir_hits(&self, p: &Path) -> bool {
        self.fail_read_dir_containing
            .as_deref()
            .is_some_and(|n| p.to_string_lossy().replace('\\', "/").contains(n))
    }
}

impl FileSystem for FailingFs {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        if self.read_hits(path) {
            return Err(Self::denied(path));
        }
        self.inner.read(path)
    }
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        self.inner.write(path, data)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.inner.rename(from, to)
    }
    fn exists(&self, path: &Path) -> bool {
        self.inner.exists(path)
    }
    fn is_file(&self, path: &Path) -> bool {
        self.inner.is_file(path)
    }
    fn is_dir(&self, path: &Path) -> bool {
        self.inner.is_dir(path)
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        if self.read_dir_hits(path) {
            return Err(Self::denied(path));
        }
        self.inner.read_dir(path)
    }
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        self.inner.create_dir_all(path)
    }
    fn copy(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.inner.copy(from, to)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.inner.remove_file(path)
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        self.inner.remove_dir_all(path)
    }
    fn mtime_secs(&self, path: &Path) -> io::Result<u64> {
        self.inner.mtime_secs(path)
    }
}

fn seed_basic(fs: &MemoryFileSystem) {
    fs.write(
        Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
        b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n",
    )
    .unwrap();
    fs.write(
        Path::new("/h/.claude/projects/E--Projects-A/memory/notes.md"),
        b"# notes\n",
    )
    .unwrap();
    fs.write(Path::new("E:/Projects/A/f.txt"), b"x").unwrap();
}

// =========================================================================================
// AC-59: I/O failure is never absence
// =========================================================================================

/// A read_dir failure inside the snapshot walk must abort the apply BEFORE anything is
/// written. The old lenient walk silently skipped the unreadable subtree, producing a
/// snapshot that looked complete while missing every file under it.
#[test]
fn backup_walk_read_failure_aborts_before_any_write() {
    let inner = MemoryFileSystem::new();
    seed_basic(&inner);
    let plan = build_plan(&inner, Path::new(HOME), &mv(), &opts()).unwrap();
    let before_transcript = inner
        .read(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"))
        .unwrap();

    let fs = FailingFs {
        inner,
        fail_read_containing: None,
        fail_read_dir_containing: Some("E--Projects-A/memory".into()),
    };
    apply(&plan, &fs, Path::new("/backup"), "AC59A")
        .expect_err("an unreadable subtree during snapshot must abort the apply");

    assert!(
        fs.exists(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl")),
        "the project dir must not have been renamed"
    );
    assert_eq!(
        fs.read(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"))
            .unwrap(),
        before_transcript,
        "nothing may be written after a snapshot failure"
    );
    assert!(
        fs.exists(Path::new("E:/Projects/A/f.txt")),
        "the real folder must not have moved"
    );
}

/// A verify that cannot READ what it must check may never report green - and after a
/// successful apply, a verify hard-error must roll the apply back, not strand the migration
/// in a "done but unverifiable" state that still returns an error.
#[test]
fn apply_verified_rolls_back_when_verify_itself_errors() {
    let inner = MemoryFileSystem::new();
    seed_basic(&inner);
    // claude.json exists with content the plan does not touch, so apply never reads it and
    // the injected failure fires only inside verify.
    inner
        .write(Path::new("/h/.claude.json"), br#"{"projects":{}}"#)
        .unwrap();
    let plan = build_plan(&inner, Path::new(HOME), &mv(), &opts()).unwrap();

    let fs = FailingFs {
        inner,
        fail_read_containing: Some(".claude.json".into()),
        fail_read_dir_containing: None,
    };
    apply_verified(
        &plan,
        &fs,
        Path::new("/backup"),
        &ApplyOpts {
            run_id: "AC59B".into(),
            auto_rollback: true,
            force: false,
        },
    )
    .expect_err("a verify that cannot read its evidence must not succeed");

    // The apply must have been rolled back: folder back at src, transcripts back at the
    // old-encoded dir.
    assert!(
        fs.exists(Path::new("E:/Projects/A/f.txt")),
        "auto-rollback must restore the folder when verify errors"
    );
    assert!(
        !fs.exists(Path::new("E:/Projects/B")),
        "the destination must be gone after rollback"
    );
    assert!(
        fs.exists(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl")),
        "the project-state dir must be back at the old encoding"
    );
}

// =========================================================================================
// AC-57: verify derives from the plan
// =========================================================================================

/// A malformed history.jsonl line is a verification FAILURE, not something to skip. The old
/// filter_map silently dropped unparseable lines, so a rewrite that corrupted a line would
/// verify green.
#[test]
fn verify_reports_malformed_history_lines() {
    let fs = MemoryFileSystem::new();
    seed_basic(&fs);
    fs.write(
        Path::new("/h/.claude/history.jsonl"),
        b"{\"display\":\"ok\",\"project\":\"E:\\\\Other\"}\nTHIS IS NOT JSON\n",
    )
    .unwrap();
    let results = awt_core::verify::verify(&fs, Path::new(HOME), &mv(), None, None).unwrap();
    assert!(
        results
            .iter()
            .any(|r| !r.ok && r.check.contains("history") && r.detail.contains("parse")),
        "a malformed history line must fail verification: {results:?}"
    );
}

/// Plan-derived splice checks: verify must confirm each planned claude.json edit actually
/// landed (the destination anchor is present, the source anchor is gone), not merely that
/// the old projects key is absent. Sabotage that removes BOTH old and new keys passed the
/// old checks.
#[test]
fn verify_with_plan_catches_a_missing_destination_anchor() {
    let fs = MemoryFileSystem::new();
    seed_basic(&fs);
    fs.write(
        Path::new("/h/.claude.json"),
        br#"{"projects":{"E:/Projects/A":{}}}"#,
    )
    .unwrap();
    let plan = build_plan(&fs, Path::new(HOME), &mv(), &opts()).unwrap();
    apply(&plan, &fs, Path::new("/backup"), "AC57").unwrap();

    // Sabotage: wipe the migrated key entirely. Old-key-absent still holds; only a
    // plan-derived presence check can see the loss.
    fs.write(Path::new("/h/.claude.json"), br#"{"projects":{}}"#)
        .unwrap();

    let manifest =
        awt_core::backup::Manifest::load(&fs, Path::new("/backup/awt-AC57/manifest.json")).unwrap();
    let results =
        awt_core::verify::verify(&fs, Path::new(HOME), &mv(), Some(&manifest), Some(&plan))
            .unwrap();
    assert!(
        results
            .iter()
            .any(|r| !r.ok && r.detail.contains("E:/Projects/B")),
        "verify must notice the planned destination anchor is missing: {results:?}"
    );
}

// =========================================================================================
// AC-60: plugin hash uses the recorded spelling
// =========================================================================================

/// The transcript records `E:\Projects\A`; the plugin state dir is keyed to that exact
/// spelling. The caller types a case/separator variant, which every other store matches
/// case-insensitively. The plugin store must find the dir via the RECORDED spelling, not
/// hash the caller's input and find nothing.
#[test]
fn plugin_state_is_found_when_src_is_spelled_differently() {
    let fs = MemoryFileSystem::new();
    seed_basic(&fs);
    let recorded_hash = awt_core::stores::plugin_state::state_hash(r"E:\Projects\A");
    fs.write(
        Path::new(&format!(
            "/h/.claude/plugins/data/codex/state/A-{recorded_hash}/state.json"
        )),
        b"{}",
    )
    .unwrap();

    // Same project, different spelling: lowercase drive, forward slashes.
    let variant = Move {
        src_abs: "e:/projects/a".into(),
        dst_abs: r"E:\Projects\B".into(),
    };
    let plan = build_plan(&fs, Path::new(HOME), &variant, &opts()).unwrap();
    let has_plugin_rename = plan.changes.iter().any(|c| {
        matches!(c, awt_core::model::Change::RenameDir { from, .. }
            if from.to_string_lossy().contains("plugins"))
    });
    assert!(
        has_plugin_rename,
        "the plan must include the plugin-state rename even though the caller's spelling \
         differs from the recorded one: {:?}",
        plan.changes
    );
}

/// Verify has the same blind spot: it hashed the caller's spelling, so a leftover plugin dir
/// keyed to the recorded spelling was invisible and verify passed.
#[test]
fn plugin_verify_catches_a_leftover_dir_under_the_recorded_spelling() {
    let fs = MemoryFileSystem::new();
    seed_basic(&fs);
    let recorded_hash = awt_core::stores::plugin_state::state_hash(r"E:\Projects\A");
    fs.write(
        Path::new(&format!(
            "/h/.claude/plugins/data/codex/state/A-{recorded_hash}/state.json"
        )),
        b"{}",
    )
    .unwrap();

    let variant = Move {
        src_abs: "e:/projects/a".into(),
        dst_abs: r"E:\Projects\B".into(),
    };
    let results = awt_core::verify::verify(&fs, Path::new(HOME), &variant, None, None).unwrap();
    assert!(
        results.iter().any(|r| !r.ok && r.check.contains("plugin")),
        "the leftover plugin dir must fail verification despite the spelling variant: {results:?}"
    );
}
