//! Exit codes and path handling of `awt archive` (AR-09 review follow-up).
//!
//! Every run uses a temp `--home` and a temp archive folder. Nothing here touches a real
//! Claude home.
//!
//! - An archive folder that contains the Claude data is a refusal (exit 2), on a sweep, on
//!   `--session`, and on `--install-hook`, so a useless path is never recorded.
//! - A failure to WRITE into the archive folder exits 1 on a sweep or `--session`, but not in
//!   hook mode. A failure to READ a file stays at exit 0 (see archive_locked_file.rs).
//! - A relative or `..` spelling of `--archive-dir` must not defeat the self-copy guard.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn awt() -> Command {
    Command::new(env!("CARGO_BIN_EXE_awt"))
}

/// A temp home with one session `s1` that has a transcript and a subagent file, plus a task.
/// Returns (home guard, the project folder, the transcript path).
fn seed_home() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let home = tempfile::tempdir().unwrap();
    let claude = home.path().join(".claude");
    let proj = claude.join("projects").join("E--tmp-exit-proj");
    std::fs::create_dir_all(proj.join("s1").join("subagents")).unwrap();
    let transcript = proj.join("s1.jsonl");
    std::fs::write(&transcript, b"{\"cwd\":\"E:\\\\tmp\\\\exit-proj\"}\n").unwrap();
    std::fs::write(proj.join("s1").join("subagents").join("a.jsonl"), b"sub").unwrap();
    std::fs::create_dir_all(claude.join("tasks").join("s1")).unwrap();
    std::fs::write(claude.join("tasks").join("s1").join("1.json"), b"task").unwrap();
    (home, proj, transcript)
}

fn code(out: &Output) -> i32 {
    out.status.code().expect("process exited with a code")
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn run_archive(home: &Path, extra: &[&str], cwd: Option<&Path>) -> Output {
    let mut c = awt();
    c.arg("archive").arg("--home").arg(home).args(extra);
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    c.output().unwrap()
}

#[test]
fn archive_folder_containing_the_claude_data_exits_2_and_writes_nothing() {
    let (home, _proj, transcript) = seed_home();
    let h = home.path();
    let candidates = [
        h.to_path_buf(),
        h.join(".claude"),
        h.join(".claude").join("projects"),
        h.join(".claude").join("tasks"),
    ];
    for dir in &candidates {
        let d = dir.to_str().unwrap();
        for extra in [
            vec!["--archive-dir", d],
            vec![
                "--archive-dir",
                d,
                "--session",
                transcript.to_str().unwrap(),
            ],
        ] {
            let out = run_archive(h, &extra, None);
            assert_eq!(code(&out), 2, "{extra:?}: {}", text(&out.stderr));
        }
        assert!(
            !dir.join("manifest.json").exists(),
            "{d}: a refused run must write no manifest"
        );
    }
}

#[test]
fn a_drive_root_archive_folder_exits_2() {
    let (home, _proj, _t) = seed_home();
    // The root of the drive that holds the temp home contains the Claude data.
    let root: PathBuf = home
        .path()
        .ancestors()
        .last()
        .expect("a temp path has a root")
        .to_path_buf();
    let out = run_archive(
        home.path(),
        &["--archive-dir", root.to_str().unwrap()],
        None,
    );
    assert_eq!(code(&out), 2, "{}", text(&out.stderr));
    assert!(!root.join("manifest.json").exists());
}

#[test]
fn install_hook_refuses_an_archive_folder_that_contains_the_claude_data() {
    let (home, _proj, _t) = seed_home();
    let bad = home.path().join(".claude");
    let out = run_archive(
        home.path(),
        &["--install-hook", "--archive-dir", bad.to_str().unwrap()],
        None,
    );
    assert_eq!(code(&out), 2, "{}", text(&out.stderr));
    assert!(
        !home.path().join(".claude").join("settings.json").exists(),
        "a useless path must never be recorded in settings.json"
    );
}

#[test]
fn an_archive_folder_deeper_inside_the_home_is_allowed() {
    let (home, _proj, _t) = seed_home();
    let ok = home.path().join(".claude").join("backups").join("x");
    let out = run_archive(home.path(), &["--archive-dir", ok.to_str().unwrap()], None);
    assert_eq!(code(&out), 0, "{}", text(&out.stderr));
    assert!(ok.join("manifest.json").is_file());
}

/// An archive folder whose `projects` entry is a FILE makes every write into it fail, which
/// models a destination that cannot be written.
fn unwritable_archive(root: &Path) -> PathBuf {
    let arch = root.join("arch");
    std::fs::create_dir_all(&arch).unwrap();
    std::fs::write(arch.join("projects"), b"in the way").unwrap();
    arch
}

#[test]
fn write_failure_exits_1_after_printing_the_result_on_a_sweep() {
    let (home, _proj, _t) = seed_home();
    let root = tempfile::tempdir().unwrap();
    let arch = unwritable_archive(root.path());
    let out = run_archive(
        home.path(),
        &["--archive-dir", arch.to_str().unwrap()],
        None,
    );
    assert_eq!(code(&out), 1, "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(
        stdout.contains("archived:") && stdout.contains("failed"),
        "{stdout}"
    );
    assert!(
        text(&out.stderr).contains("archive failed:"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn write_failure_keeps_json_stdout_to_one_object_and_exits_1() {
    let (home, _proj, _t) = seed_home();
    let root = tempfile::tempdir().unwrap();
    let arch = unwritable_archive(root.path());
    let out = awt()
        .args(["archive", "--json", "--home"])
        .arg(home.path())
        .arg("--archive-dir")
        .arg(&arch)
        .output()
        .unwrap();
    assert_eq!(code(&out), 1, "{}", text(&out.stderr));
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout must be exactly one JSON object");
    assert!(v["failed"].as_u64().unwrap() >= 1, "{v}");
}

#[test]
fn write_failure_exits_1_on_a_session_run() {
    let (home, _proj, transcript) = seed_home();
    let root = tempfile::tempdir().unwrap();
    let arch = unwritable_archive(root.path());
    let out = run_archive(
        home.path(),
        &[
            "--archive-dir",
            arch.to_str().unwrap(),
            "--session",
            transcript.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(code(&out), 1, "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("archived:"));
}

#[test]
fn write_failure_stays_at_exit_0_in_hook_mode() {
    let (home, _proj, transcript) = seed_home();
    let root = tempfile::tempdir().unwrap();
    let arch = unwritable_archive(root.path());
    let payload = serde_json::json!({
        "session_id": "s1",
        "transcript_path": transcript.to_str().unwrap(),
        "hook_event_name": "SessionEnd",
    })
    .to_string();
    let mut child = awt()
        .args(["archive", "--hook-stdin", "--home"])
        .arg(home.path())
        .arg("--archive-dir")
        .arg(&arch)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(code(&out), 0, "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("archive failed:"));
}

#[test]
fn a_relative_archive_dir_inside_a_session_folder_does_not_copy_itself() {
    let (home, proj, _t) = seed_home();
    let session_dir = proj.join("s1");
    let json = |out: &Output| -> serde_json::Value {
        assert_eq!(code(out), 0, "{}", text(&out.stderr));
        serde_json::from_slice(&out.stdout).unwrap()
    };
    // The folder is spelled three ways; all of them name the same place.
    let first = json(&run_archive(
        home.path(),
        &["--json", "--archive-dir", "arch"],
        Some(&session_dir),
    ));
    assert!(first["copied"].as_u64().unwrap() >= 3, "{first}");
    for spelling in ["arch", "x/../arch", "./arch"] {
        let again = json(&run_archive(
            home.path(),
            &["--json", "--archive-dir", spelling],
            Some(&session_dir),
        ));
        assert_eq!(again["copied"], 0, "{spelling}: {again}");
        assert_eq!(again["failed"], 0, "{spelling}: {again}");
    }
    let arch = session_dir.join("arch");
    assert!(arch.join("manifest.json").is_file());
    for entry in walkdir_names(&arch) {
        assert!(
            !entry
                .replace('\\', "/")
                .contains("/arch/projects/E--tmp-exit-proj/s1/arch"),
            "the archive copied itself: {entry}"
        );
    }
    assert!(!arch
        .join("projects")
        .join("E--tmp-exit-proj")
        .join("s1")
        .join("arch")
        .exists());
}

fn walkdir_names(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            out.push(p.to_string_lossy().into_owned());
            if p.is_dir() {
                stack.push(p);
            }
        }
    }
    out
}

#[test]
fn install_hook_records_an_absolute_normalized_archive_dir() {
    let (home, _proj, _t) = seed_home();
    let work = tempfile::tempdir().unwrap();
    let out = run_archive(
        home.path(),
        &["--install-hook", "--archive-dir", "sub/../arch"],
        Some(work.path()),
    );
    assert_eq!(code(&out), 0, "{}", text(&out.stderr));
    let settings =
        std::fs::read_to_string(home.path().join(".claude").join("settings.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&settings).unwrap();
    let cmd = v["hooks"]["SessionEnd"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();
    let recorded = cmd
        .split("--archive-dir \"")
        .nth(1)
        .and_then(|r| r.strip_suffix('"'))
        .unwrap_or_else(|| panic!("no archive dir in: {cmd}"));
    assert!(Path::new(recorded).is_absolute(), "{recorded}");
    assert!(!recorded.contains(".."), "{recorded}");
    assert!(Path::new(recorded).ends_with("arch"), "{recorded}");
    assert_eq!(v["hooks"]["SessionEnd"][0]["hooks"][0]["timeout"], 60);
}
