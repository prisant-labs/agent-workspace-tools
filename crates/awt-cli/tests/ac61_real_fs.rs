//! AC-61 on the REAL filesystem, through the real binary: Windows junctions and the
//! hook-stdin traversal escape. The MemoryFileSystem tests prove the policy; these prove
//! the detection, because `is_reparse_point` reads real NTFS attributes and an in-memory
//! model cannot exercise that.

use std::io::Write as _;
use std::process::{Command, Stdio};

fn encode(abs: &str) -> String {
    abs.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// mklink /J needs no elevation, unlike symlinks, which is why junctions are the test
/// vehicle (and the realistic attack shape on Windows).
#[cfg(windows)]
fn make_junction(link: &std::path::Path, target: &std::path::Path) {
    let out = Command::new("cmd")
        .args([
            "/C",
            "mklink",
            "/J",
            link.to_str().unwrap(),
            target.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "mklink /J failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[cfg(windows)]
#[test]
fn apply_refuses_a_real_junction_inside_project_state() {
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();

    // Real source folder.
    let src_dir = root.path().join("src").join("proj");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(src_dir.join("f.txt"), b"x").unwrap();
    let src_abs = src_dir.to_str().unwrap().to_string();
    let dst_abs = root.path().join("dst-proj").to_str().unwrap().to_string();

    // Project-state dir with a transcript and a REAL junction pointing outside.
    let outside = root.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("secret.txt"), b"outside").unwrap();
    let proj_state = home
        .path()
        .join(".claude")
        .join("projects")
        .join(encode(&src_abs));
    std::fs::create_dir_all(&proj_state).unwrap();
    let cwd_json = format!("{{\"cwd\":\"{}\"}}\n", src_abs.replace('\\', "\\\\"));
    std::fs::write(proj_state.join("s.jsonl"), cwd_json.as_bytes()).unwrap();
    make_junction(&proj_state.join("linked"), &outside);

    let out = Command::new(env!("CARGO_BIN_EXE_awt"))
        .args([
            "apply",
            "--home",
            home.path().to_str().unwrap(),
            "--backup-root",
            root.path().join("bk").to_str().unwrap(),
            "--src",
            &src_abs,
            "--dst",
            &dst_abs,
        ])
        .output()
        .unwrap();

    assert_eq!(
        out.status.code(),
        Some(2),
        "a junction in the tree must refuse with the guard exit code\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("reparse"),
        "the refusal must name the reparse point: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Nothing moved, nothing renamed.
    assert!(src_dir.exists(), "source folder untouched");
    assert!(proj_state.join("s.jsonl").exists(), "state dir untouched");
    // And the junction's target was never disturbed.
    assert!(outside.join("secret.txt").exists());
}

/// The hook confinement escape: a transcript_path that starts with the accepted prefix as
/// BYTES but resolves outside it. The old lexical check accepted this; the canonicalizing
/// check must refuse it and archive nothing.
#[test]
fn hook_stdin_rejects_a_dotdot_escape() {
    let home = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let projects = home.path().join(".claude").join("projects").join("E--p");
    std::fs::create_dir_all(&projects).unwrap();

    // The victim file lives OUTSIDE projects; the hook path reaches it via `..`.
    let outside = home.path().join("private.jsonl");
    std::fs::write(&outside, b"{\"secret\":true}\n").unwrap();
    let sneaky = format!("{}\\..\\..\\..\\private.jsonl", projects.to_str().unwrap());

    let mut child = Command::new(env!("CARGO_BIN_EXE_awt"))
        .args([
            "archive",
            "--hook-stdin",
            "--archive-dir",
            archive.path().to_str().unwrap(),
            "--home",
            home.path().to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(
            format!(
                "{{\"transcript_path\":\"{}\"}}",
                sneaky.replace('\\', "\\\\")
            )
            .as_bytes(),
        )
        .unwrap();
    let out = child.wait_with_output().unwrap();

    assert_eq!(
        out.status.code(),
        Some(4),
        "the escape must be refused as unrecognized input\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Nothing was archived.
    let archived = std::fs::read_dir(archive.path()).unwrap().count();
    assert_eq!(archived, 0, "nothing may be archived from an escaping path");
}
