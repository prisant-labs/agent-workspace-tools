//! S-03 AC-47 / AC-48: `awt repair --drive-letter` end to end through the binary.
//!
//! Builds its malformed value from the test's own temp directory, so the repaired path is
//! guaranteed to exist on exactly one drive without the test needing to know which drive that
//! is. That keeps it portable across a dev machine and the CI runner.

use std::process::Command;

/// Returns (home, malformed_value, real_value).
fn seed(home: &std::path::Path) -> (String, String) {
    let real = home.to_string_lossy().replace('/', "\\");
    // Strip the "X:" prefix and put the corruption in its place, reproducing the observed damage.
    let malformed = format!("::{}", &real[2..]);

    let claude = home.join(".claude");
    std::fs::create_dir_all(&claude).unwrap();
    let body = format!(
        "{{\"display\":\"a\",\"project\":\"{}\",\"sessionId\":\"1\"}}\n\
         {{\"display\":\"b\",\"project\":\"{}\",\"sessionId\":\"2\"}}\n\
         {{\"display\":\"keep\",\"project\":\"{}\",\"sessionId\":\"3\"}}\n",
        malformed.replace('\\', "\\\\"),
        malformed.replace('\\', "\\\\"),
        real.replace('\\', "\\\\"),
    );
    std::fs::write(claude.join("history.jsonl"), body.as_bytes()).unwrap();
    (malformed, real)
}

fn history(home: &std::path::Path) -> String {
    std::fs::read_to_string(home.join(".claude").join("history.jsonl")).unwrap()
}

#[test]
fn repair_dry_run_writes_nothing_then_apply_repairs() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let (malformed, real) = seed(home);
    let before = history(home);

    // --- dry run: reports the repair, writes nothing (AC-47) ---
    let out = Command::new(env!("CARGO_BIN_EXE_awt"))
        .args(["repair", "--drive-letter", "--home", home.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Repairable"),
        "dry run did not report the repair: {stdout}"
    );
    assert!(
        stdout.contains("Dry run"),
        "dry run did not say it wrote nothing: {stdout}"
    );
    assert_eq!(history(home), before, "dry run modified the file");

    // --- apply: repairs, leaves the healthy line alone (AC-48) ---
    let out = Command::new(env!("CARGO_BIN_EXE_awt"))
        .args([
            "repair",
            "--drive-letter",
            "--apply",
            "--home",
            home.to_str().unwrap(),
            "--backup-root",
            tmp.path().join("bk").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let after = history(home);
    assert!(
        !after.contains(&malformed.replace('\\', "\\\\")),
        "a malformed value survived: {after}"
    );
    assert_eq!(
        after.matches(&real.replace('\\', "\\\\")).count(),
        3,
        "expected all three lines to carry the real path: {after}"
    );
    assert_eq!(
        after.lines().count(),
        before.lines().count(),
        "line count changed"
    );

    // --- idempotent: a second apply finds nothing to do and still exits 0 (AC-51) ---
    let out = Command::new(env!("CARGO_BIN_EXE_awt"))
        .args([
            "repair",
            "--drive-letter",
            "--apply",
            "--home",
            home.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "second apply should exit 0");
    assert_eq!(history(home), after, "second apply mutated the file");
}

/// Selecting no repair is a refusal, not a default-to-everything (D8).
#[test]
fn repair_without_a_selected_transformation_refuses() {
    let tmp = tempfile::tempdir().unwrap();
    seed(tmp.path());
    let out = Command::new(env!("CARGO_BIN_EXE_awt"))
        .args(["repair", "--home", tmp.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected the guard exit code; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
