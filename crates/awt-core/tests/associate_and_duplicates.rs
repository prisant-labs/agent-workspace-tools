//! Regression guards for two defects found by the 2026-07-28 acceptance run and its
//! follow-up, both of which made `awt associate` unusable on real data.
//!
//! AR-02: `associate` resolved its target through the transcript-keyed reverse index, so a
//! project whose transcripts had expired but whose `claude.json` and `history.jsonl` state
//! survived was reported as having "no Claude state found". Transcripts expire after 30
//! days and `history.jsonl` never does, so this refused precisely the long-dead projects
//! the command exists to rescue.
//!
//! AR-04: `.claude.json` can hold the SAME path value under two different
//! `githubRepoPaths` slugs. Each hit planned its own change with `expected: 1`, but each
//! change counts occurrences across the whole file and finds 2, so the count check refused
//! with `expected 1, live 2`. Surfaced only after the AR-01 escaping fix let the anchor
//! match at all - the two defects were stacked.

use awt_core::associate::{associate, AssociateOpts};
use awt_core::error::AwtError;
use awt_core::fs::{FileSystem, MemoryFileSystem};
use awt_core::model::{Move, Scope};
use awt_core::plan::{build_plan, Collision, PlanOpts};
use std::path::Path;

const HOME: &str = "/h";

fn opts() -> AssociateOpts {
    AssociateOpts {
        reassociate: true,
        export: true,
        export_subdir: ".claude-sessions".into(),
        run_id: "T".into(),
        on_collision: Collision::Refuse,
    }
}

fn plan_opts() -> PlanOpts {
    PlanOpts {
        recursive: false,
        on_collision: Collision::Refuse,
        force: false,
        move_folder: true,
        scope: Scope::Standard,
    }
}

// ---------------------------------------------------------------------------
// AR-04: the same path value under two githubRepoPaths slugs
// ---------------------------------------------------------------------------

/// Mirrors the real shape found in the maintainer's `.claude.json`, where
/// `prisant-labs/agent-workspace-tools` and `prisant-labs/claude-project-mover` both
/// pointed at the same old folder.
fn seed_duplicate_value_home() -> MemoryFileSystem {
    let fs = MemoryFileSystem::new();
    let json = r#"{"projects":{"E:/tmp/probe":{}},"githubRepoPaths":{"owner/a":["E:\\tmp\\probe"],"owner/b":["E:\\tmp\\probe"]}}"#;
    fs.write(Path::new("/h/.claude.json"), json.as_bytes())
        .unwrap();
    fs.write(Path::new("E:/tmp/probe/.keep"), b"x").unwrap();
    fs
}

#[test]
fn duplicate_github_repo_path_values_are_coalesced_into_one_change() {
    let fs = seed_duplicate_value_home();
    let mv = Move {
        src_abs: r"E:\tmp\probe".into(),
        dst_abs: r"E:\tmp\probe-moved".into(),
    };
    let plan = build_plan(&fs, Path::new(HOME), &mv, &plan_opts()).unwrap();

    // Two hits share one literal, so exactly one change should carry expected: 2.
    // Two changes of expected: 1 each is the bug: each counts the whole file and sees 2.
    let array_changes: Vec<_> = plan
        .changes
        .iter()
        .filter_map(|c| match c {
            awt_core::model::Change::RewriteJsonArrayValue { from, expected, .. } => {
                Some((from.clone(), *expected))
            }
            _ => None,
        })
        .collect();

    assert_eq!(
        array_changes.len(),
        1,
        "duplicate literals must coalesce into a single change, got {array_changes:?}"
    );
    assert_eq!(
        array_changes[0].1, 2,
        "the coalesced change must expect both occurrences, got {array_changes:?}"
    );
}

#[test]
fn apply_succeeds_when_two_slugs_share_a_path_value() {
    let fs = seed_duplicate_value_home();
    let mv = Move {
        src_abs: r"E:\tmp\probe".into(),
        dst_abs: r"E:\tmp\probe-moved".into(),
    };
    let plan = build_plan(&fs, Path::new(HOME), &mv, &plan_opts()).unwrap();
    awt_core::apply::apply(&plan, &fs, Path::new("/backup"), "AR04")
        .expect("apply must succeed; 'expected 1, live 2' here is AR-04 reproducing");

    let text = String::from_utf8(fs.read(Path::new("/h/.claude.json")).unwrap()).unwrap();
    assert!(
        !text.contains(r#""E:\\tmp\\probe""#),
        "an old value survived: {text}"
    );
    assert_eq!(
        text.matches(r#""E:\\tmp\\probe-moved""#).count(),
        2,
        "both slugs must be rewritten: {text}"
    );
}

// ---------------------------------------------------------------------------
// AR-02: associate on a project whose transcripts have expired
// ---------------------------------------------------------------------------

/// A project with `claude.json` and `history.jsonl` state but NO transcript directory,
/// which is the state every project reaches 30 days after its last session. A second,
/// unrelated project supplies transcripts so the reverse index is non-empty - the bug
/// must not be masked by an entirely empty index.
fn seed_expired_transcripts_home() -> MemoryFileSystem {
    let fs = MemoryFileSystem::new();
    let json = r#"{"projects":{"E:/gone/proj":{},"E:/other":{}},"githubRepoPaths":{"owner/gone":["E:\\gone\\proj"]}}"#;
    fs.write(Path::new("/h/.claude.json"), json.as_bytes())
        .unwrap();
    fs.write(
        Path::new("/h/.claude/history.jsonl"),
        b"{\"display\":\"/mcp\",\"project\":\"E:\\\\gone\\\\proj\",\"sessionId\":\"a\"}\n\
          {\"display\":\"hi\",\"project\":\"E:\\\\gone\\\\proj\",\"sessionId\":\"b\"}\n\
          {\"display\":\"x\",\"project\":\"E:\\\\other\",\"sessionId\":\"c\"}\n",
    )
    .unwrap();
    // Another project's transcripts exist; the target project's do not.
    fs.write(
        Path::new("/h/.claude/projects/E--other/o.jsonl"),
        b"{\"cwd\":\"E:\\\\other\"}\n",
    )
    .unwrap();
    fs.write(Path::new("E:/new/proj/.keep"), b"x").unwrap();
    fs
}

#[test]
fn associate_works_when_transcripts_have_expired() {
    let fs = seed_expired_transcripts_home();
    associate(
        &fs,
        Path::new(HOME),
        "E:\\gone\\proj",
        "E:\\new\\proj",
        &opts(),
    )
    .expect(
        "associate must handle a project with no transcripts; \
         'no Claude state found' here is AR-02 reproducing",
    );

    let text = String::from_utf8(fs.read(Path::new("/h/.claude.json")).unwrap()).unwrap();
    assert!(
        text.contains(r#""E:/new/proj""#) || text.contains(r#""E:\\new\\proj""#),
        "claude.json projects key was not re-associated: {text}"
    );
    assert!(
        !text.contains(r#""E:/gone/proj""#),
        "old projects key survived: {text}"
    );

    let hist = String::from_utf8(fs.read(Path::new("/h/.claude/history.jsonl")).unwrap()).unwrap();
    assert!(
        !hist.contains(r"E:\\gone\\proj"),
        "history lines were not re-associated: {hist}"
    );
    assert!(
        hist.contains(r"E:\\other"),
        "an unrelated project's history line was damaged: {hist}"
    );
}

/// The export half has nothing to copy when there are no transcripts. That must be a
/// no-op, not a failure that aborts the re-association half.
#[test]
fn export_is_a_noop_rather_than_a_failure_when_there_are_no_transcripts() {
    let fs = seed_expired_transcripts_home();
    let rep = associate(
        &fs,
        Path::new(HOME),
        "E:\\gone\\proj",
        "E:\\new\\proj",
        &opts(),
    )
    .unwrap();
    assert!(
        !rep.applied.is_empty(),
        "re-association should still have applied changes"
    );
    // No transcripts existed, so nothing should have been exported.
    assert!(
        !fs.exists(Path::new(
            "E:/new/proj/.claude-sessions/projects/E--gone-proj"
        )),
        "export created a directory for a project that has no transcripts"
    );
}

/// A path with genuinely no state anywhere must still be refused. The AR-02 fix widens
/// what counts as "has state"; it must not turn the check off.
#[test]
fn associate_still_refuses_a_project_with_no_state_at_all() {
    let fs = seed_expired_transcripts_home();
    let err = associate(
        &fs,
        Path::new(HOME),
        "E:\\never\\existed",
        "E:\\new\\proj",
        &opts(),
    )
    .unwrap_err();
    assert!(
        matches!(err, AwtError::UnrecognizedFormat(_)),
        "expected UnrecognizedFormat for a project with no state, got {err:?}"
    );
}
