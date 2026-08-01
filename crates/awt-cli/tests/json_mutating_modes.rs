//! Codex adversarial-review finding (2026-07-31, medium) plus one adjacent gap: the
//! global `--json` promise ("emit machine-readable JSON instead of text") was broken by
//! four SUCCESSFUL mutating paths that printed prose - `archive --install-hook`,
//! `archive --uninstall-hook`, `archive --set-retention`, and `associate`. Automation
//! parsing stdout saw a parse failure AFTER the mutation had already landed.
//!
//! Contract: every successful mutating mode emits exactly one parseable JSON object on
//! stdout under `--json` (warnings may still go to stderr).

use std::process::Command;

fn parse_stdout_json(out: &std::process::Output, label: &str) -> serde_json::Value {
    assert!(
        out.status.success(),
        "{label} failed\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!("{label}: stdout must be one JSON object ({e}); got: {stdout:?}")
    })
}

#[test]
fn archive_install_and_uninstall_hook_json() {
    let home = tempfile::tempdir().unwrap();
    let arch = tempfile::tempdir().unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_awt"))
        .args([
            "archive",
            "--install-hook",
            "--archive-dir",
            arch.path().to_str().unwrap(),
            "--home",
            home.path().to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    let v = parse_stdout_json(&out, "install-hook --json");
    assert_eq!(v["hook_installed"], serde_json::Value::Bool(true), "{v}");

    let out = Command::new(env!("CARGO_BIN_EXE_awt"))
        .args([
            "archive",
            "--uninstall-hook",
            "--home",
            home.path().to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    let v = parse_stdout_json(&out, "uninstall-hook --json");
    assert_eq!(v["hook_removed"], serde_json::Value::Bool(true), "{v}");
}

#[test]
fn archive_set_retention_json() {
    let home = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_awt"))
        .args([
            "archive",
            "--set-retention",
            "90",
            "--home",
            home.path().to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    let v = parse_stdout_json(&out, "set-retention --json");
    assert_eq!(v["cleanup_period_days"], serde_json::json!(90), "{v}");
}

#[test]
fn associate_json() {
    let home = tempfile::tempdir().unwrap();
    let to = tempfile::tempdir().unwrap();
    // Minimal recorded state for the source: one history line.
    let claude = home.path().join(".claude");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(
        claude.join("history.jsonl"),
        b"{\"display\":\"a\",\"project\":\"E:\\\\gone\\\\proj\",\"sessionId\":\"1\"}\n",
    )
    .unwrap();
    std::fs::write(home.path().join(".claude.json"), b"{\"projects\":{}}").unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_awt"))
        .args([
            "associate",
            "--from",
            "E:\\gone\\proj",
            "--to",
            to.path().to_str().unwrap(),
            "--home",
            home.path().to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    let v = parse_stdout_json(&out, "associate --json");
    assert!(
        v["applied"].is_u64(),
        "summary must carry the applied count: {v}"
    );
}
