//! F-1 (rollback leaves a plugin folder behind) and F-2 (`list --html` replaces an existing
//! file), through the real binary on the real filesystem.
//!
//! F-1's symptom, OS error 145, is a property of a real directory rename. An in-memory model
//! can only approximate it, so the retry is proven here as well as in `awt-core`.

use awt_core::stores::plugin_state::state_hash;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn awt(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_awt"))
        .args(args)
        .output()
        .unwrap()
}

fn show(out: &Output) -> String {
    format!(
        "status {:?}\nstderr: {}\nstdout: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    )
}

struct Fixture {
    _root: tempfile::TempDir,
    _home: tempfile::TempDir,
    home: PathBuf,
    backup_root: PathBuf,
    src: String,
    dst: String,
    old_state: PathBuf,
    new_state: PathBuf,
}

fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();

    let src_dir = root.path().join("src").join("proj");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(src_dir.join("f.txt"), b"x").unwrap();
    let src = src_dir.to_str().unwrap().to_string();
    let dst = root.path().join("moved").to_str().unwrap().to_string();

    let encoded: String = src
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let transcripts = home.path().join(".claude").join("projects").join(&encoded);
    std::fs::create_dir_all(&transcripts).unwrap();
    std::fs::write(
        transcripts.join("s.jsonl"),
        format!("{{\"cwd\":\"{}\"}}\n", src.replace('\\', "\\\\")),
    )
    .unwrap();

    let state = home
        .path()
        .join(".claude")
        .join("plugins")
        .join("data")
        .join("codex")
        .join("state");
    let old_state = state.join(format!("proj-{}", state_hash(&src)));
    let new_state = state.join(format!("moved-{}", state_hash(&dst)));
    std::fs::create_dir_all(&old_state).unwrap();
    std::fs::write(old_state.join("state.json"), br#"{"n":1}"#).unwrap();

    let backup_root = root.path().join("backup");
    std::fs::create_dir_all(&backup_root).unwrap();

    Fixture {
        home: home.path().to_path_buf(),
        backup_root,
        src,
        dst,
        old_state,
        new_state,
        _root: root,
        _home: home,
    }
}

impl Fixture {
    fn run(&self, verb: &str) -> Output {
        let mut args = vec![verb, "--home", self.home.to_str().unwrap()];
        if verb == "apply" {
            args.extend(["--backup-root", self.backup_root.to_str().unwrap()]);
        }
        args.extend(["--src", &self.src, "--dst", &self.dst]);
        awt(&args)
    }

    fn only_manifest(&self) -> PathBuf {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&self.backup_root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(dirs.len(), 1, "exactly one apply has run: {dirs:?}");
        dirs.pop().unwrap().join("manifest.json")
    }

    fn rollback(&self, manifest: &Path) -> Output {
        awt(&[
            "rollback",
            "--home",
            self.home.to_str().unwrap(),
            "--report",
            manifest.to_str().unwrap(),
        ])
    }
}

#[test]
fn a_rolled_back_move_can_be_repeated_on_the_real_filesystem() {
    let fx = fixture();

    let out = fx.run("apply");
    assert!(out.status.success(), "first apply: {}", show(&out));
    assert!(fx.new_state.is_dir(), "precondition: state folder renamed");
    assert!(!fx.old_state.exists(), "precondition: old name gone");

    let out = fx.rollback(&fx.only_manifest());
    assert!(out.status.success(), "rollback: {}", show(&out));
    assert!(
        !fx.new_state.exists(),
        "F-1: the renamed folder must not survive rollback"
    );
    assert_eq!(
        std::fs::read(fx.old_state.join("state.json")).unwrap(),
        br#"{"n":1}"#
    );

    let out = fx.run("plan");
    assert!(out.status.success(), "second plan: {}", show(&out));
    let out = fx.run("apply");
    assert!(
        out.status.success(),
        "F-1: the same move must succeed again: {}",
        show(&out)
    );
    assert!(fx.new_state.is_dir());
    assert!(Path::new(&fx.dst).is_dir());
}

/// A leftover from a rollback by an older binary must stop `plan`, not `apply`. Exit 2 means
/// refused with nothing written, and the message must name the folder.
#[test]
fn plan_refuses_a_leftover_state_folder_with_exit_2() {
    let fx = fixture();
    std::fs::create_dir_all(&fx.new_state).unwrap();
    std::fs::write(fx.new_state.join("leftover.json"), b"old").unwrap();

    let out = fx.run("plan");

    assert_eq!(out.status.code(), Some(2), "{}", show(&out));
    let name = fx.new_state.file_name().unwrap().to_str().unwrap();
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(name),
        "the refusal must name the folder: {}",
        show(&out)
    );
    assert!(fx.old_state.is_dir(), "nothing may have moved");
    assert_eq!(
        std::fs::read(fx.new_state.join("leftover.json")).unwrap(),
        b"old"
    );
}

#[test]
fn list_html_refuses_to_replace_an_existing_file() {
    let home = tempfile::tempdir().unwrap();
    let out_dir = tempfile::tempdir().unwrap();
    let target = out_dir.path().join("precious.txt");
    std::fs::write(&target, b"irreplaceable").unwrap();

    let out = awt(&[
        "list",
        "--home",
        home.path().to_str().unwrap(),
        "--html",
        target.to_str().unwrap(),
    ]);

    assert_eq!(out.status.code(), Some(2), "{}", show(&out));
    assert_eq!(
        std::fs::read(&target).unwrap(),
        b"irreplaceable",
        "F-2: the existing file must be untouched"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("precious.txt"),
        "the refusal must name the file: {}",
        show(&out)
    );
}

#[test]
fn list_html_still_writes_a_new_file() {
    let home = tempfile::tempdir().unwrap();
    let out_dir = tempfile::tempdir().unwrap();
    let target = out_dir.path().join("inventory.html");

    let out = awt(&[
        "list",
        "--home",
        home.path().to_str().unwrap(),
        "--html",
        target.to_str().unwrap(),
    ]);

    assert!(out.status.success(), "{}", show(&out));
    let page = std::fs::read_to_string(&target).unwrap();
    assert!(page.contains("<html"), "must be a page: {page}");
}
