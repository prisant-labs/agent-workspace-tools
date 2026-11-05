//! AR-10 (a project folder with no readable transcript) through the real binary on real folders.
//!
//! The in-memory tests prove the decisions. These prove the renames, the exit codes and a
//! byte-identical rollback, which an in-memory model can only approximate.

use std::collections::BTreeMap;
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

fn encode(path: &str) -> String {
    path.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Every file under `root`, by relative path, with its bytes.
fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path.strip_prefix(root).unwrap().to_path_buf();
                out.insert(rel, std::fs::read(&path).unwrap());
            }
        }
    }
    out
}

struct Fixture {
    root: tempfile::TempDir,
    home: tempfile::TempDir,
    backup_root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let backup_root = root.path().join("backup");
        std::fs::create_dir_all(&backup_root).unwrap();
        std::fs::create_dir_all(home.path().join(".claude").join("projects")).unwrap();
        Self {
            root,
            home,
            backup_root,
        }
    }

    fn project(&self, name: &str) -> String {
        let dir = self.root.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("f.txt"), b"x").unwrap();
        dir.to_str().unwrap().to_string()
    }

    fn path_of(&self, name: &str) -> String {
        self.root.path().join(name).to_str().unwrap().to_string()
    }

    fn state_dir(&self, project_path: &str) -> PathBuf {
        self.home
            .path()
            .join(".claude")
            .join("projects")
            .join(encode(project_path))
    }

    fn seed(&self, project_path: &str, rel: &str, bytes: &[u8]) {
        let file = self.state_dir(project_path).join(rel);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, bytes).unwrap();
    }

    fn apply(&self, src: &str, dst: &str) -> Output {
        awt(&[
            "apply",
            "--home",
            self.home.path().to_str().unwrap(),
            "--backup-root",
            self.backup_root.to_str().unwrap(),
            "--src",
            src,
            "--dst",
            dst,
        ])
    }

    fn plan(&self, src: &str, dst: &str) -> Output {
        awt(&[
            "plan",
            "--home",
            self.home.path().to_str().unwrap(),
            "--src",
            src,
            "--dst",
            dst,
        ])
    }

    fn rollback(&self) -> Output {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&self.backup_root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(dirs.len(), 1, "exactly one apply has run: {dirs:?}");
        let manifest = dirs.pop().unwrap().join("manifest.json");
        awt(&[
            "rollback",
            "--home",
            self.home.path().to_str().unwrap(),
            "--report",
            manifest.to_str().unwrap(),
        ])
    }
}

#[test]
fn memory_follows_a_real_move_and_rollback_is_byte_identical() {
    let fx = Fixture::new();
    let src = fx.project("proj");
    let dst = fx.path_of("moved");
    fx.seed(&src, "memory/MEMORY.md", b"remember this");
    fx.seed(&src, "abc/subagents/agent-1.jsonl", b"{}\n");
    let home_before = tree(fx.home.path());

    let out = fx.apply(&src, &dst);
    assert!(out.status.success(), "apply: {}", show(&out));
    assert_eq!(
        std::fs::read(fx.state_dir(&dst).join("memory").join("MEMORY.md")).unwrap(),
        b"remember this"
    );
    assert!(!fx.state_dir(&src).exists(), "the old folder must be gone");

    let out = fx.rollback();
    assert!(out.status.success(), "rollback: {}", show(&out));
    assert_eq!(
        tree(fx.home.path()),
        home_before,
        "rollback must restore the home byte for byte"
    );
    assert!(!fx.state_dir(&dst).exists());

    // The same move works again after the rollback.
    let out = fx.plan(&src, &dst);
    assert!(out.status.success(), "plan after rollback: {}", show(&out));
}

#[test]
fn a_real_lookalike_folder_stays_with_its_owner() {
    let fx = Fixture::new();
    // These two paths encode alike: the separator and the hyphen both become '-'.
    let owner = fx.project("foo-bar");
    let src = fx.project(&format!("foo{}bar", std::path::MAIN_SEPARATOR));
    assert_eq!(encode(&owner), encode(&src), "fixture must collide");
    let dst = fx.path_of("moved");
    let transcript = format!("{{\"cwd\":\"{}\"}}\n", owner.replace('\\', "\\\\"));
    fx.seed(&owner, "s.jsonl", transcript.as_bytes());
    let folder_before = tree(&fx.state_dir(&owner));

    let out = fx.apply(&src, &dst);

    assert!(out.status.success(), "apply: {}", show(&out));
    assert_eq!(tree(&fx.state_dir(&owner)), folder_before);
    assert!(!fx.state_dir(&dst).exists());
    assert!(Path::new(&dst).join("f.txt").exists(), "the project moved");
}

#[test]
fn apply_prints_the_name_match_warning_and_keeps_json_clean() {
    let fx = Fixture::new();
    let src = fx.project("proj");
    let dst = fx.path_of("moved");
    fx.seed(&src, "memory/MEMORY.md", b"m");

    let out = awt(&[
        "--json",
        "apply",
        "--home",
        fx.home.path().to_str().unwrap(),
        "--backup-root",
        fx.backup_root.to_str().unwrap(),
        "--src",
        &src,
        "--dst",
        &dst,
    ]);

    assert!(out.status.success(), "{}", show(&out));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("folder name"),
        "{}",
        show(&out)
    );
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(doc["warnings"][0].as_str().unwrap().contains("folder name"));
}

/// Three spellings of one path, each of which used to verify clean with state left behind.
fn spelled_move(spell: impl Fn(&str) -> String) {
    let fx = Fixture::new();
    let src = fx.project("proj");
    let dst = fx.path_of("moved");
    let transcript = format!("{{\"cwd\":\"{}\"}}\n", src.replace('\\', "\\\\"));
    fx.seed(&src, "s.jsonl", transcript.as_bytes());
    let json = format!(
        "{{\"projects\":{{{}:{{}}}}}}",
        serde_json::Value::String(src.clone())
    );
    std::fs::write(fx.home.path().join(".claude.json"), json).unwrap();

    let out = fx.apply(&spell(&src), &spell(&dst));
    assert!(out.status.success(), "apply: {}", show(&out));

    let moved = std::fs::read_to_string(fx.state_dir(&dst).join("s.jsonl"))
        .unwrap_or_else(|e| panic!("state did not reach the clean name: {e}\n{}", show(&out)));
    assert!(
        moved.contains("moved"),
        "the cwd was not rewritten: {moved}"
    );
    assert!(!fx.state_dir(&src).exists(), "the old folder must be gone");
    let settings = std::fs::read_to_string(fx.home.path().join(".claude.json")).unwrap();
    assert!(
        settings.contains("moved"),
        "the key was not moved: {settings}"
    );
    assert!(Path::new(&dst).join("f.txt").exists());
}

#[test]
fn a_trailing_separator_still_migrates_state() {
    spelled_move(|p| format!("{p}{}", std::path::MAIN_SEPARATOR));
}

#[test]
fn a_trailing_dot_component_still_migrates_state() {
    spelled_move(|p| format!("{p}{}.", std::path::MAIN_SEPARATOR));
}

#[cfg(windows)]
#[test]
fn forward_slashes_still_migrate_state() {
    spelled_move(|p| p.replace('\\', "/"));
}

#[test]
fn associate_reports_no_export_when_nothing_was_exported() {
    let fx = Fixture::new();
    let src = fx.path_of("gone");
    let to = fx.project("to");
    fx.seed(&src, "memory/MEMORY.md", b"m");
    let json = format!(
        "{{\"projects\":{{{}:{{}}}}}}",
        serde_json::Value::String(src.clone())
    );
    std::fs::write(fx.home.path().join(".claude.json"), json).unwrap();

    let out = awt(&[
        "--json",
        "associate",
        "--home",
        fx.home.path().to_str().unwrap(),
        "--from",
        &src,
        "--to",
        &to,
    ]);

    assert!(out.status.success(), "{}", show(&out));
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(doc["export"].is_null(), "{}", show(&out));
    assert!(fx.state_dir(&to).join("memory").join("MEMORY.md").exists());
}

#[test]
fn a_file_on_both_sides_of_a_merge_exits_2_at_plan() {
    let fx = Fixture::new();
    let src = fx.project("proj");
    let dst = fx.path_of("moved");
    fx.seed(&src, "memory/MEMORY.md", b"a");
    fx.seed(&dst, "memory/MEMORY.md", b"b");
    let home_before = tree(fx.home.path());

    let out = fx.plan(&src, &dst);

    assert_eq!(out.status.code(), Some(2), "{}", show(&out));
    assert!(String::from_utf8_lossy(&out.stderr).contains("MEMORY.md"));
    assert_eq!(tree(fx.home.path()), home_before);
}

#[test]
fn a_real_rename_with_the_same_encoded_name_works_and_rolls_back() {
    let fx = Fixture::new();
    let src = fx.project("foo_bar");
    let dst = fx.path_of("foo-bar");
    assert_eq!(encode(&src), encode(&dst), "fixture must collide");
    let transcript = format!("{{\"cwd\":\"{}\"}}\n", src.replace('\\', "\\\\"));
    fx.seed(&src, "s.jsonl", transcript.as_bytes());
    fx.seed(&src, "memory/MEMORY.md", b"m");
    let home_before = tree(fx.home.path());

    let out = fx.apply(&src, &dst);
    assert!(out.status.success(), "apply: {}", show(&out));
    let moved = std::fs::read_to_string(fx.state_dir(&dst).join("s.jsonl")).unwrap();
    assert!(moved.contains("foo-bar"), "{moved}");
    assert!(Path::new(&dst).join("f.txt").exists());

    let out = fx.rollback();
    assert!(out.status.success(), "rollback: {}", show(&out));
    assert_eq!(tree(fx.home.path()), home_before);
    assert!(Path::new(&src).join("f.txt").exists());
}

#[cfg(windows)]
#[test]
fn a_state_folder_that_is_a_junction_refuses_with_exit_2() {
    let fx = Fixture::new();
    let src = fx.project("proj");
    let dst = fx.path_of("moved");
    let target = fx.root.path().join("elsewhere");
    std::fs::create_dir_all(target.join("memory")).unwrap();
    std::fs::write(target.join("memory").join("M.md"), b"m").unwrap();
    let link = fx.state_dir(&src);
    let status = Command::new("cmd")
        .args([
            "/C",
            "mklink",
            "/J",
            link.to_str().unwrap(),
            target.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(status.status.success(), "mklink /J failed");

    let out = fx.plan(&src, &dst);

    assert_eq!(out.status.code(), Some(2), "{}", show(&out));
    assert!(target.join("memory").join("M.md").exists());
}
