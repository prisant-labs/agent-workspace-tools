//! AR-05 and AR-07 from the 2026-07-31 adversarial acceptance run, end to end through the
//! real binary.
//!
//! AR-05: `apply` prints `report: ...report.json` and rollback's flag is named `--report`,
//! so the natural rollback invocation hands over report.json - which panicked (exit 101)
//! instead of working or refusing. Contract: rollback accepts EITHER the manifest.json or
//! the report.json (dereferencing to its sibling manifest), and refuses any other JSON
//! shape with the unrecognized-format exit code 4, never a panic.
//!
//! AR-07: `archive --json` advertised JSON output but emitted the text summary line
//! (the AR-03 defect class, missed on archive).

use std::path::Path;
use std::process::Command;

/// Seed a real temp home + source folder, run apply, and return
/// (home, root, src_abs, backup_dir).
fn seed_and_apply() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    String,
    std::path::PathBuf,
) {
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();

    let src_dir = root.path().join("src").join("proj");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(src_dir.join("f.txt"), b"x").unwrap();
    let src_abs = src_dir.to_str().unwrap().to_string();
    let dst_abs = root.path().join("dst-proj").to_str().unwrap().to_string();

    let encoded: String = src_abs
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let proj_state = home.path().join(".claude").join("projects").join(&encoded);
    std::fs::create_dir_all(&proj_state).unwrap();
    let cwd_json = format!("{{\"cwd\":\"{}\"}}\n", src_abs.replace('\\', "\\\\"));
    std::fs::write(proj_state.join("s.jsonl"), cwd_json.as_bytes()).unwrap();

    let backup_root = root.path().join("backup");
    std::fs::create_dir_all(&backup_root).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_awt"))
        .args([
            "apply",
            "--home",
            home.path().to_str().unwrap(),
            "--backup-root",
            backup_root.to_str().unwrap(),
            "--src",
            &src_abs,
            "--dst",
            &dst_abs,
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "apply failed\nstderr: {}\nstdout: {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );

    let backup_dir = std::fs::read_dir(&backup_root)
        .unwrap()
        .find_map(|e| {
            let e = e.unwrap();
            e.file_name()
                .to_string_lossy()
                .starts_with("awt-")
                .then(|| e.path())
        })
        .expect("backup dir created");
    (home, root, src_abs, backup_dir)
}

/// AR-05, the natural invocation: apply's own output points at report.json, so rollback
/// must accept it by dereferencing to the sibling manifest.
#[test]
fn rollback_accepts_the_report_json_apply_prints() {
    let (home, _root, src_abs, backup_dir) = seed_and_apply();
    let report = backup_dir.join("report.json");
    assert!(report.is_file(), "apply writes report.json");

    let out = Command::new(env!("CARGO_BIN_EXE_awt"))
        .args([
            "rollback",
            "--home",
            home.path().to_str().unwrap(),
            "--report",
            report.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "rollback via report.json must succeed\nexit: {:?}\nstderr: {}\nstdout: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        Path::new(&src_abs).is_dir(),
        "source folder restored via report.json rollback"
    );
}

/// AR-05, the refusal half: a JSON file that is neither a manifest nor an apply report
/// must exit 4 (unrecognized format), never panic (exit 101).
#[test]
fn rollback_refuses_wrong_shape_json_with_exit_4() {
    let (home, root, _src_abs, _backup_dir) = seed_and_apply();
    let bogus = root.path().join("bogus.json");
    std::fs::write(&bogus, b"{\"foo\": 1}").unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_awt"))
        .args([
            "rollback",
            "--home",
            home.path().to_str().unwrap(),
            "--report",
            bogus.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(4),
        "wrong-shape JSON must refuse with exit 4\nstderr: {}\nstdout: {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unrecognized"),
        "refusal must use the unrecognized-format channel: {stderr}"
    );
}

/// AR-07: archive --json emits a machine-readable summary, not the text line.
#[test]
fn archive_json_emits_machine_readable_summary() {
    let home = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();

    let proj_state = home
        .path()
        .join(".claude")
        .join("projects")
        .join("E--tmp-arch-proj");
    std::fs::create_dir_all(&proj_state).unwrap();
    std::fs::write(
        proj_state.join("s.jsonl"),
        b"{\"cwd\":\"E:\\\\tmp\\\\arch-proj\"}\n",
    )
    .unwrap();
    let arch = root.path().join("archive");

    let out = Command::new(env!("CARGO_BIN_EXE_awt"))
        .args([
            "archive",
            "--home",
            home.path().to_str().unwrap(),
            "--archive-dir",
            arch.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "archive --json failed\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!("archive --json stdout must parse as JSON ({e}); got: {stdout:?}")
    });
    assert!(
        v["copied"].is_u64() && v["skipped"].is_u64(),
        "summary must carry numeric copied/skipped: {v}"
    );
    assert_eq!(v["copied"].as_u64(), Some(1), "one transcript archived");
}
