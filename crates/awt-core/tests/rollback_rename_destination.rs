//! F-1 (rollback leaves a plugin folder behind), found 2026-11-03.
//!
//! Rollback used to work out where a renamed directory went by swapping the encoded source
//! name for the encoded destination name inside the old path, and it skipped any path that did
//! not contain the encoded source name. Two real shapes never contain it:
//!
//! - a plugin-state folder, which is named `<basename>-<hash>`;
//! - a transcript folder whose stored spelling differs in case from the source argument, which
//!   the AGENTS.md encoding invariant says is normal (`d--Cloud-Work-Demo` holds `D:\...`).
//!
//! Step 2 of rollback then restored the backed-up files into the old location, so every original
//! came back and the byte-identity proof passed. Nothing removed the new location, so the orphan
//! stayed, and a retry of the same move passed `plan` and failed at `apply` with OS error 145.
//!
//! The contract under test: the manifest records each rename's real destination, rollback uses
//! it, and `verify_rollback` proves the new location is gone. Manifests written before this fix
//! have no recorded destination, and they must still restore.

use awt_core::apply::{apply, apply_verified, ApplyOpts};
use awt_core::error::AwtError;
use awt_core::fs::{FileSystem, MemoryFileSystem};
use awt_core::model::{Change, Move};
use awt_core::plan::{build_plan, Plan, PlanOpts};
use awt_core::rollback::{rollback, verify_rollback};
use awt_core::stores::plugin_state::state_hash;
use std::io;
use std::path::{Path, PathBuf};

const HOME: &str = "/h";
const OLD: &str = r"E:\Projects\A";
const NEW: &str = r"E:\Projects\B";
const MANIFEST: &str = "/backup/awt-T/manifest.json";
const PLUGIN_BYTES: &[u8] = br#"{"n":1}"#;

fn old_plugin_dir() -> String {
    format!("/h/.claude/plugins/data/codex/state/A-{}", state_hash(OLD))
}

fn new_plugin_dir() -> String {
    format!("/h/.claude/plugins/data/codex/state/B-{}", state_hash(NEW))
}

fn mv() -> Move {
    Move {
        src_abs: OLD.into(),
        dst_abs: NEW.into(),
    }
}

fn plan_of(fs: &MemoryFileSystem) -> Plan {
    let opts = PlanOpts {
        force: false,
        move_folder: true,
    };
    build_plan(fs, Path::new(HOME), &mv(), &opts).expect("planning this home must succeed")
}

/// One project with a transcript, one plugin-state folder and a real source folder.
fn seed() -> MemoryFileSystem {
    let fs = MemoryFileSystem::new();
    fs.write(
        Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
        b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n",
    )
    .unwrap();
    fs.write(
        Path::new(&format!("{}/state.json", old_plugin_dir())),
        PLUGIN_BYTES,
    )
    .unwrap();
    fs.write(Path::new("E:/Projects/A/f.txt"), b"x").unwrap();
    fs
}

fn apply_move(fs: &MemoryFileSystem) {
    let plan = plan_of(fs);
    apply(&plan, fs, Path::new("/backup"), "T").expect("apply must succeed");
    assert!(
        fs.is_dir(Path::new(&new_plugin_dir())),
        "precondition: apply must have renamed the plugin-state folder"
    );
    assert!(
        !fs.is_dir(Path::new(&old_plugin_dir())),
        "precondition: the old plugin-state folder must be gone after apply"
    );
}

/// Rewrite the manifest as a binary from before this fix wrote it.
fn strip_recorded_destinations(fs: &MemoryFileSystem) {
    let mut v: serde_json::Value =
        serde_json::from_slice(&fs.read(Path::new(MANIFEST)).unwrap()).unwrap();
    for e in v["entries"].as_array_mut().unwrap() {
        e.as_object_mut().unwrap().remove("to");
    }
    fs.write(Path::new(MANIFEST), v.to_string().as_bytes())
        .unwrap();
}

#[test]
fn rollback_returns_the_plugin_state_folder_to_its_old_name() {
    let fs = seed();
    apply_move(&fs);
    // A file the plugin wrote after the move. No backup holds it, so it can only come home if
    // rollback renames the folder rather than recreating it from the backup.
    fs.write(
        Path::new(&format!("{}/written-after-move.json", new_plugin_dir())),
        b"later",
    )
    .unwrap();

    rollback(Path::new(MANIFEST), &fs).unwrap();

    assert!(
        !fs.exists(Path::new(&new_plugin_dir())),
        "F-1: the renamed plugin-state folder must not survive rollback"
    );
    assert_eq!(
        fs.read(Path::new(&format!("{}/state.json", old_plugin_dir())))
            .unwrap(),
        PLUGIN_BYTES,
        "the original plugin state must return byte-identical"
    );
    assert_eq!(
        fs.read(Path::new(&format!(
            "{}/written-after-move.json",
            old_plugin_dir()
        )))
        .unwrap(),
        b"later",
        "the whole folder must come home, not only the files the backup holds"
    );
}

#[test]
fn the_same_move_can_be_retried_after_a_rollback() {
    let fs = seed();
    apply_move(&fs);
    rollback(Path::new(MANIFEST), &fs).unwrap();

    let plan = plan_of(&fs);
    apply(&plan, &fs, Path::new("/backup"), "RETRY")
        .expect("F-1: a rolled-back move must be repeatable");

    assert!(fs.is_dir(Path::new(&new_plugin_dir())));
    assert!(!fs.is_dir(Path::new(&old_plugin_dir())));
}

#[test]
fn the_manifest_records_each_renames_real_destination() {
    let fs = seed();
    apply_move(&fs);
    let v: serde_json::Value =
        serde_json::from_slice(&fs.read(Path::new(MANIFEST)).unwrap()).unwrap();
    let entries = v["entries"].as_array().unwrap();

    let renames: Vec<_> = entries
        .iter()
        .filter(|e| e["backup"].as_str().unwrap().starts_with("<dir-rename"))
        .collect();
    assert_eq!(
        renames.len(),
        2,
        "precondition: the transcript and plugin-state folders are both renamed: {entries:?}"
    );
    for e in &renames {
        assert!(
            e["to"].as_str().is_some_and(|s| !s.is_empty()),
            "every rename marker must record its destination: {e}"
        );
    }
    assert!(
        renames.iter().any(|e| {
            let to = e["to"].as_str().unwrap().replace('\\', "/");
            to == new_plugin_dir()
        }),
        "the plugin-state rename must record its real destination: {renames:?}"
    );
    assert!(
        entries
            .iter()
            .filter(|e| !e["backup"].as_str().unwrap().starts_with("<dir-rename"))
            .all(|e| e.get("to").is_none()),
        "only rename markers carry a destination"
    );
}

/// Manifests already sitting in backup folders have no recorded destination. Rollback is the
/// recovery path for those moves, so it must work on them, and it must not leave F-1's orphan.
#[test]
fn a_manifest_from_before_this_fix_still_restores_everything() {
    let fs = seed();
    apply_move(&fs);
    strip_recorded_destinations(&fs);

    rollback(Path::new(MANIFEST), &fs).unwrap();

    assert!(
        !fs.exists(Path::new(&new_plugin_dir())),
        "the plugin-state destination must be derived when the manifest has none"
    );
    assert!(
        !fs.exists(Path::new("/h/.claude/projects/E--Projects-B")),
        "the transcript destination must be derived when the manifest has none"
    );
    assert_eq!(
        fs.read(Path::new(&format!("{}/state.json", old_plugin_dir())))
            .unwrap(),
        PLUGIN_BYTES
    );
    assert!(fs.exists(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl")));
    assert!(fs.exists(Path::new("E:/Projects/A/f.txt")));
}

/// The AGENTS.md encoding invariant: a stored folder name need not equal `encode(src)`. The old
/// case-sensitive `contains` skipped this folder exactly as it skipped plugin state.
#[test]
fn a_transcript_folder_spelled_in_another_case_also_comes_home() {
    let fs = MemoryFileSystem::new();
    let old_dir = "/h/.claude/projects/e--Projects-A";
    fs.write(
        Path::new(&format!("{old_dir}/s.jsonl")),
        b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n",
    )
    .unwrap();
    fs.write(Path::new("E:/Projects/A/f.txt"), b"x").unwrap();
    apply(&plan_of(&fs), &fs, Path::new("/backup"), "T").unwrap();
    assert!(
        fs.is_dir(Path::new("/h/.claude/projects/E--Projects-B")),
        "precondition: apply must have renamed the transcript folder"
    );

    rollback(Path::new(MANIFEST), &fs).unwrap();

    assert!(
        !fs.exists(Path::new("/h/.claude/projects/E--Projects-B")),
        "a folder whose stored name differs in case from encode(src) must not be orphaned"
    );
    assert!(fs.exists(Path::new(&format!("{old_dir}/s.jsonl"))));
}

/// The proof must be able to see the orphan. Before this fix it read only original files, so
/// it passed with the leftover in place, which is how F-1 survived a Proven rating.
#[test]
fn verify_rollback_fails_when_a_renamed_folder_is_still_there() {
    let fs = seed();
    apply_move(&fs);
    rollback(Path::new(MANIFEST), &fs).unwrap();
    let clean = verify_rollback(Path::new(MANIFEST), &fs).unwrap();
    assert!(
        clean.iter().all(|r| r.ok),
        "a clean rollback must verify: {clean:?}"
    );

    fs.write(
        Path::new(&format!("{}/state.json", new_plugin_dir())),
        PLUGIN_BYTES,
    )
    .unwrap();

    let results = verify_rollback(Path::new(MANIFEST), &fs).unwrap();
    assert!(
        results
            .iter()
            .any(|r| !r.ok && r.detail.contains(&format!("B-{}", state_hash(NEW)))),
        "the leftover folder must fail a check that names it: {results:?}"
    );
}

/// `plan` must tell the truth about what `apply` can do. A folder already sitting at the
/// destination made `plan` pass and `apply` fail, which also falsified "the dry run is exact".
#[test]
fn plan_refuses_when_a_state_folder_already_holds_the_destination() {
    let fs = seed();
    let leftover = format!("{}/state.json", new_plugin_dir());
    fs.write(Path::new(&leftover), b"someone else's").unwrap();

    let opts = PlanOpts {
        force: false,
        move_folder: true,
    };
    let err = match build_plan(&fs, Path::new(HOME), &mv(), &opts) {
        Err(e) => e,
        Ok(_) => panic!("plan must refuse an occupied state-folder destination"),
    };

    assert!(matches!(err, AwtError::DestinationExists(_)), "{err:?}");
    assert!(
        err.to_string().contains(&format!("B-{}", state_hash(NEW))),
        "the refusal must name the folder to deal with: {err}"
    );
    assert!(
        err.to_string().contains("live project"),
        "the folder may hold a live project's state, so the refusal must not read as safe to \
         delete: {err}"
    );
    assert_eq!(fs.read(Path::new(&leftover)).unwrap(), b"someone else's");
}

/// The window between `plan` and the first rename includes the snapshot, which can be slow.
#[test]
fn apply_refuses_before_any_rename_when_a_destination_appears_after_plan() {
    let fs = seed();
    let plan = plan_of(&fs);
    fs.write(
        Path::new(&format!("{}/state.json", new_plugin_dir())),
        b"appeared after plan",
    )
    .unwrap();

    let opts = ApplyOpts {
        run_id: "T".into(),
        auto_rollback: true,
        force: false,
    };
    let err = match apply_verified(&plan, &fs, Path::new("/backup"), &opts) {
        Err(e) => e,
        Ok(_) => panic!("apply must refuse a destination that appeared after plan"),
    };

    // A guard refusal, not a failed apply. Nothing changed, so no rollback may run: it would
    // rewrite every backed-up file, and a live session may have appended to one since the
    // snapshot.
    assert!(matches!(err, AwtError::DestinationExists(_)), "{err:?}");
    assert!(
        fs.exists(Path::new("/h/.claude/projects/E--Projects-A/s.jsonl")),
        "no store may have been renamed"
    );
    assert!(!fs.exists(Path::new("/h/.claude/projects/E--Projects-B")));
    assert_eq!(
        fs.read(Path::new(&format!("{}/state.json", new_plugin_dir())))
            .unwrap(),
        b"appeared after plan",
        "the folder that appeared must be left alone"
    );
}

/// F-1's fix makes this state reachable for plugin folders, because a live session can recreate
/// the old one. A refusal that fires halfway strands the move, so it belongs in the preflight.
#[test]
fn rollback_refuses_before_any_change_when_both_folders_exist() {
    let fs = seed();
    apply_move(&fs);
    fs.write(
        Path::new(&format!("{}/state.json", old_plugin_dir())),
        b"recreated by a live session",
    )
    .unwrap();

    let err = rollback(Path::new(MANIFEST), &fs).unwrap_err();

    assert!(matches!(err, AwtError::VerifyFailed(_)), "{err:?}");
    assert!(
        fs.exists(Path::new("E:/Projects/B/f.txt")),
        "the project folder must not have been moved back before the refusal"
    );
    assert!(
        fs.exists(Path::new("/h/.claude/projects/E--Projects-B/s.jsonl")),
        "the transcript folder must not have been renamed back before the refusal"
    );
    assert_eq!(
        fs.read(Path::new(&format!("{}/state.json", old_plugin_dir())))
            .unwrap(),
        b"recreated by a live session"
    );
}

/// Behaves like a memory filesystem, except that a live session recreates the old plugin-state
/// folder at the moment apply reads the renamed transcript.
struct LiveSessionFs {
    inner: MemoryFileSystem,
}

impl FileSystem for LiveSessionFs {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        if path.to_string_lossy().contains("E--Projects-B") {
            self.inner.write(
                Path::new(&format!("{}/state.json", old_plugin_dir())),
                b"recreated by a live session",
            )?;
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

/// A failed apply whose rollback refused is half-applied. The person must be told, because
/// "backup at ..." alone reads as if the failure had been cleaned up.
#[test]
fn a_refused_automatic_rollback_is_reported_not_swallowed() {
    let fs = LiveSessionFs { inner: seed() };
    let mut plan = plan_of(&fs.inner);
    // An impossible count makes apply fail after its renames, on the rewrite of the transcript.
    let mut injected = false;
    for c in &mut plan.changes {
        if let Change::RewriteFile { expected, .. } = c {
            *expected += 999;
            injected = true;
        }
    }
    assert!(
        injected,
        "the fixture must produce a RewriteFile to corrupt"
    );

    let opts = ApplyOpts {
        run_id: "T".into(),
        auto_rollback: true,
        force: false,
    };
    let err = match apply_verified(&plan, &fs, Path::new("/backup"), &opts) {
        Err(e) => e,
        Ok(_) => panic!("the corrupted plan must fail"),
    };

    let text = err.to_string();
    assert!(matches!(err, AwtError::VerifyFailed(_)), "{err:?}");
    assert!(
        text.contains("rollback did not complete"),
        "the refused rollback must be reported: {text}"
    );
    assert!(
        text.contains("both"),
        "the report must carry rollback's own reason: {text}"
    );
}

/// The plugin store looks for state under the recorded spelling of the path and under the
/// caller's spelling (AC-60). When a folder exists for each, both plan the same destination. The
/// second rename cannot succeed, so `plan` must say so. Left to `apply`, the failure reaches a
/// rollback that sees the first folder's destination and the second folder's old name, and
/// refuses, stranding a half-applied move.
#[test]
fn plan_refuses_when_two_state_folders_would_land_on_one_destination() {
    let fs = seed();
    let other_spelling = r"e:\projects\a";
    let second = format!(
        "/h/.claude/plugins/data/codex/state/a-{}/state.json",
        state_hash(other_spelling)
    );
    fs.write(Path::new(&second), b"second spelling").unwrap();

    let opts = PlanOpts {
        force: false,
        move_folder: true,
    };
    let typed_differently = Move {
        src_abs: other_spelling.into(),
        dst_abs: NEW.into(),
    };
    let err = match build_plan(&fs, Path::new(HOME), &typed_differently, &opts) {
        Err(e) => e,
        Ok(_) => panic!("plan must refuse two renames onto one destination"),
    };

    assert!(matches!(err, AwtError::DestinationExists(_)), "{err:?}");
    assert!(
        err.to_string().contains(&format!("B-{}", state_hash(NEW))),
        "the refusal must name the destination: {err}"
    );
    assert_eq!(fs.read(Path::new(&second)).unwrap(), b"second spelling");
}
