//! Exit codes of `awt associate --export` (AR-09 review follow-up).
//!
//! Both runs use a temp `--home` and a temp destination folder.
//!
//! - Export-only (`--no-reassociate`) where no readable transcript resolves the project
//!   refuses with exit 2, and prints no export path.
//! - With re-association on, a plan refusal (exit 2) writes nothing into the destination.

use std::process::{Command, Output};

fn awt() -> Command {
    Command::new(env!("CARGO_BIN_EXE_awt"))
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn run(home: &std::path::Path, to: &std::path::Path, extra: &[&str]) -> Output {
    awt()
        .args(["associate", "--home"])
        .arg(home)
        .args(["--from", "E:\\gone\\proj", "--to"])
        .arg(to)
        .args(extra)
        .output()
        .unwrap()
}

#[test]
fn export_only_with_no_resolving_transcript_exits_2_and_exports_nothing() {
    let home = tempfile::tempdir().unwrap();
    let to = tempfile::tempdir().unwrap();
    // State exists for the project, but no transcript does.
    std::fs::write(
        home.path().join(".claude.json"),
        br#"{"projects":{"E:/gone/proj":{}}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(home.path().join(".claude").join("projects")).unwrap();

    let out = run(home.path(), to.path(), &["--no-reassociate"]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("nothing was exported"),
        "{}",
        text(&out.stderr)
    );
    assert!(
        !text(&out.stdout).contains(".claude-sessions"),
        "no export path may be printed: {}",
        text(&out.stdout)
    );
    assert!(!to.path().join(".claude-sessions").exists());
}

#[test]
fn plan_refusal_with_export_exits_2_and_writes_nothing_into_the_destination() {
    let home = tempfile::tempdir().unwrap();
    let to = tempfile::tempdir().unwrap();
    let proj = home
        .path()
        .join(".claude")
        .join("projects")
        .join("E--gone-proj");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(proj.join("s1.jsonl"), b"{\"cwd\":\"E:\\\\gone\\\\proj\"}\n").unwrap();
    // The destination already has a claude.json key, so the plan refuses.
    let to_key = to.path().to_str().unwrap().replace('\\', "/");
    let json = format!(r#"{{"projects":{{"E:/gone/proj":{{}},"{to_key}":{{}}}}}}"#);
    std::fs::write(home.path().join(".claude.json"), json).unwrap();

    let out = run(home.path(), to.path(), &[]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("already has a key"),
        "refused for the wrong reason: {}",
        text(&out.stderr)
    );
    assert!(!to.path().join(".claude-sessions").exists());
}
