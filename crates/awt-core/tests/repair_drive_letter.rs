//! S-03 (AC-45..AC-53): `awt repair --drive-letter`.
//!
//! `history.jsonl` on a real machine was found to contain 46 distinct `project` values, across
//! 3,121 lines, whose drive letter had been replaced by a colon: `::\Projects\X` where
//! `E:\Projects\X` belongs. Those lines are unreachable - Claude Code cannot match them to any
//! project - so that prompt history is lost while still occupying the file.
//!
//! The tool's rule is act when there is one answer, refuse when there is not. Measured on the
//! real file: 34 of 46 resolve on exactly one drive, 12 on none, and **zero on more than one**.
//! So repair is deterministic here, and fails closed by construction if that ever changes.
//!
//! Two of the 46 differ only in case. They are separate literals to a byte splice and so are
//! repaired separately; a case-insensitive count under-reports the work by one.
//!
//! Every assertion below is on the RAW BYTES of the file, per the convention AR-01 established:
//! the engine writes by literal byte splice, so a parsed-value assertion can pass while the
//! actual write is impossible.

use awt_core::fs::{FileSystem, MemoryFileSystem};
use awt_core::repair::{build_repair_plan, classify, present_drives, scan_malformed, Candidate};
use std::path::Path;

const HOME: &str = "/h";

/// A history file mixing: two repairable values (only E: exists for them), one that resolves
/// nowhere, one that would resolve on two drives, and two well-formed lines that must not move.
fn seed() -> MemoryFileSystem {
    let fs = MemoryFileSystem::new();
    let body = concat!(
        r#"{"display":"a","project":"::\\Projects\\alpha","sessionId":"1"}"#,
        "\n",
        r#"{"display":"b","project":"::\\Projects\\alpha","sessionId":"2"}"#,
        "\n",
        r#"{"display":"c","project":"::\\Projects\\beta","sessionId":"3"}"#,
        "\n",
        r#"{"display":"d","project":"::\\Projects\\vanished","sessionId":"4"}"#,
        "\n",
        r#"{"display":"e","project":"::\\Shared\\both","sessionId":"5"}"#,
        "\n",
        r#"{"display":"f","project":"E:\\Projects\\healthy","sessionId":"6"}"#,
        "\n",
        r#"{"display":"g","project":"F:/Projects/healthy-fwd","sessionId":"7"}"#,
        "\n",
    );
    fs.write(Path::new("/h/.claude/history.jsonl"), body.as_bytes())
        .unwrap();

    // Drives present. `alpha` and `beta` exist only on E:. `both` exists on E: AND F:, which is
    // the ambiguous case. `vanished` exists nowhere.
    for d in ["E:/", "F:/"] {
        fs.create_dir_all(Path::new(d)).unwrap();
    }
    fs.write(Path::new("E:/Projects/alpha/.keep"), b"x")
        .unwrap();
    fs.write(Path::new("E:/Projects/beta/.keep"), b"x").unwrap();
    fs.write(Path::new("E:/Shared/both/.keep"), b"x").unwrap();
    fs.write(Path::new("F:/Shared/both/.keep"), b"x").unwrap();
    fs
}

fn history_text(fs: &MemoryFileSystem) -> String {
    String::from_utf8(fs.read(Path::new("/h/.claude/history.jsonl")).unwrap()).unwrap()
}

// --- Phase 16.1: detection and classification -------------------------------------------

#[test]
fn scan_finds_malformed_values_with_line_counts() {
    let fs = seed();
    let found = scan_malformed(&history_text(&fs));

    // Four distinct malformed values; the two well-formed lines are not reported.
    assert_eq!(found.len(), 4, "got {found:?}");
    let alpha = found
        .iter()
        .find(|m| m.value == r"::\Projects\alpha")
        .expect("alpha missing");
    assert_eq!(alpha.lines, 2, "alpha appears on two lines");
    assert!(
        !found.iter().any(|m| m.value.starts_with("E:")),
        "a well-formed value was reported as malformed: {found:?}"
    );
}

#[test]
fn classify_repairable_when_exactly_one_drive_resolves() {
    let fs = seed();
    let drives = present_drives(&fs);
    assert!(drives.contains(&'E'), "seeded drives: {drives:?}");
    assert_eq!(
        classify(&fs, r"::\Projects\alpha", &drives),
        Candidate::Repairable('E')
    );
}

#[test]
fn classify_refuses_when_no_drive_resolves() {
    let fs = seed();
    let drives = present_drives(&fs);
    assert_eq!(
        classify(&fs, r"::\Projects\vanished", &drives),
        Candidate::NoCandidate,
        "a value that resolves nowhere must be reported, never repaired"
    );
}

/// The guard that makes this feature defensible. It has no instance in the real data, which is
/// exactly why it needs a test: an unexercised guard is an assumption, not a behavior.
#[test]
fn classify_refuses_when_two_drives_resolve() {
    let fs = seed();
    let drives = present_drives(&fs);
    match classify(&fs, r"::\Shared\both", &drives) {
        Candidate::Ambiguous(letters) => {
            assert!(
                letters.contains(&'E') && letters.contains(&'F'),
                "{letters:?}"
            );
        }
        other => panic!("expected Ambiguous, got {other:?}"),
    }
}

#[test]
fn classify_ignores_a_well_formed_path() {
    let fs = seed();
    let drives = present_drives(&fs);
    assert_eq!(
        classify(&fs, r"E:\Projects\healthy", &drives),
        Candidate::NotMalformed
    );
}

// --- Phase 16.2 / 16.3: plan and apply ---------------------------------------------------

#[test]
fn dry_run_writes_nothing() {
    let fs = seed();
    let before = history_text(&fs);
    let plan = build_repair_plan(&fs, Path::new(HOME)).unwrap();
    assert!(!plan.repairs.is_empty(), "vacuous plan");
    assert_eq!(
        history_text(&fs),
        before,
        "building a plan must not write; --apply does that"
    );
}

#[test]
fn plan_expected_count_matches_live_occurrences() {
    let fs = seed();
    let plan = build_repair_plan(&fs, Path::new(HOME)).unwrap();
    // alpha x2 + beta x1 = 3 lines repairable. `vanished` and `both` are declined.
    assert_eq!(plan.total_lines(), 3, "{:?}", plan.repairs);
    assert_eq!(plan.repairs.len(), 2, "two distinct repairable values");
    assert_eq!(plan.unrepairable.len(), 1, "{:?}", plan.unrepairable);
    assert_eq!(plan.ambiguous.len(), 1, "{:?}", plan.ambiguous);
}

#[test]
fn apply_repairs_only_the_unambiguous_values() {
    let fs = seed();
    let plan = build_repair_plan(&fs, Path::new(HOME)).unwrap();
    awt_core::repair::apply_repair(&plan, &fs, Path::new("/backup"), "S03").unwrap();
    let text = history_text(&fs);

    assert!(
        text.contains(r#""project":"E:\\Projects\\alpha""#),
        "alpha not repaired: {text}"
    );
    assert!(
        text.contains(r#""project":"E:\\Projects\\beta""#),
        "beta not repaired: {text}"
    );
    // The declined values must survive verbatim.
    assert!(
        text.contains(r#""project":"::\\Projects\\vanished""#),
        "a value with no candidate was altered: {text}"
    );
    assert!(
        text.contains(r#""project":"::\\Shared\\both""#),
        "an ambiguous value was altered: {text}"
    );
}

#[test]
fn apply_preserves_unrelated_lines_byte_for_byte() {
    let fs = seed();
    let plan = build_repair_plan(&fs, Path::new(HOME)).unwrap();
    awt_core::repair::apply_repair(&plan, &fs, Path::new("/backup"), "S03").unwrap();
    let text = history_text(&fs);
    for untouched in [
        r#"{"display":"f","project":"E:\\Projects\\healthy","sessionId":"6"}"#,
        r#"{"display":"g","project":"F:/Projects/healthy-fwd","sessionId":"7"}"#,
    ] {
        assert!(text.contains(untouched), "line changed: {untouched}");
    }
}

#[test]
fn apply_preserves_line_count() {
    let fs = seed();
    let before = history_text(&fs).lines().count();
    let plan = build_repair_plan(&fs, Path::new(HOME)).unwrap();
    awt_core::repair::apply_repair(&plan, &fs, Path::new("/backup"), "S03").unwrap();
    assert_eq!(history_text(&fs).lines().count(), before);
}

// --- Phase 16.4: idempotency and isolation ----------------------------------------------

#[test]
fn repair_is_idempotent() {
    let fs = seed();
    let plan = build_repair_plan(&fs, Path::new(HOME)).unwrap();
    awt_core::repair::apply_repair(&plan, &fs, Path::new("/backup"), "S03").unwrap();
    let after_first = history_text(&fs);

    let plan2 = build_repair_plan(&fs, Path::new(HOME)).unwrap();
    assert!(
        plan2.repairs.is_empty(),
        "a second run proposed repairs: {:?}",
        plan2.repairs
    );
    assert_eq!(
        history_text(&fs),
        after_first,
        "second run mutated the file"
    );
}

#[test]
fn repair_leaves_other_stores_untouched() {
    let fs = seed();
    let json = r#"{"projects":{"E:/Projects/alpha":{}}}"#;
    fs.write(Path::new("/h/.claude.json"), json.as_bytes())
        .unwrap();
    fs.write(
        Path::new("/h/.claude/projects/E--Projects-alpha/s.jsonl"),
        b"{\"cwd\":\"E:\\\\Projects\\\\alpha\"}\n",
    )
    .unwrap();

    let plan = build_repair_plan(&fs, Path::new(HOME)).unwrap();
    awt_core::repair::apply_repair(&plan, &fs, Path::new("/backup"), "S03").unwrap();

    assert_eq!(
        String::from_utf8(fs.read(Path::new("/h/.claude.json")).unwrap()).unwrap(),
        json,
        "repair must only write history.jsonl"
    );
    assert_eq!(
        fs.read(Path::new("/h/.claude/projects/E--Projects-alpha/s.jsonl"))
            .unwrap(),
        b"{\"cwd\":\"E:\\\\Projects\\\\alpha\"}\n".to_vec(),
        "repair must not touch transcripts"
    );
}

#[test]
fn json_output_reports_declined_sets() {
    let fs = seed();
    let plan = build_repair_plan(&fs, Path::new(HOME)).unwrap();
    let v = plan.to_json();

    assert_eq!(v["totals"]["repairable_values"], 2);
    assert_eq!(v["totals"]["repairable_lines"], 3);
    // A caller must be able to see what was declined and why, not just what was done.
    assert_eq!(v["unrepairable"].as_array().unwrap().len(), 1);
    assert_eq!(v["ambiguous"].as_array().unwrap().len(), 1);
    assert_eq!(v["ambiguous"][0]["value"], r"::\Shared\both");
    assert!(
        v["ambiguous"][0]["candidates"].as_array().unwrap().len() >= 2,
        "ambiguous entries must name the competing drives: {v}"
    );
}
