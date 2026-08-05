use crate::error::Result;
use crate::fs::FileSystem;
use crate::paths::normalize_path;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn read_dir_or_empty(fs: &dyn FileSystem, path: &Path) -> Result<Vec<PathBuf>> {
    match fs.read_dir(path) {
        Ok(d) => Ok(d),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}

/// Read the first non-empty `cwd` value from a transcript. Scans lines (the
/// first line is often a summary with no cwd) and validates each as JSON before
/// trusting it. Returns the stored path string exactly as recorded.
pub fn read_stored_cwd(fs: &dyn FileSystem, transcript: &Path) -> Option<String> {
    let bytes = fs.read(transcript).ok()?;
    // Read-only heuristic: lossy is safe here because this value is only compared and
    // indexed, never spliced and written back. The write path (apply/verify) hard-fails
    // on invalid UTF-8 instead - see Global Constraints.
    let text = String::from_utf8_lossy(&bytes);
    for l in text.lines() {
        if !l.contains("\"cwd\"") {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
            if let Some(c) = v.get("cwd").and_then(|x| x.as_str()) {
                if !c.is_empty() {
                    return Some(c.to_string());
                }
            }
        }
    }
    None
}

#[derive(Debug)]
pub struct ProjectIndex {
    /// normalize(cwd) -> the `projects/` dirs that resolve to it.
    pub by_cwd: HashMap<String, Vec<PathBuf>>,
    /// Dirs with no recoverable cwd, and dirs whose every recorded cwd is gone.
    pub unresolved: Vec<PathBuf>,
    /// Every distinct ORIGINAL (non-normalized) cwd seen, across all dirs.
    /// `plugin_state::audit` hashes these to find orphaned plugin dirs, so stale
    /// ones are as valuable as live ones - an orphan is keyed by the OLD path.
    pub cwds: Vec<String>,
    /// Dirs whose transcripts name more than one path that still exists. There is
    /// no honest way to pick one, so the tool refuses rather than guesses.
    pub ambiguous: Vec<PathBuf>,
    /// dir -> the live candidate cwds for each ambiguous dir. Additive companion to
    /// `ambiguous`: carries the actual candidate paths so `build_plan` can check
    /// whether `src` is one of them and refuse rather than silently skip (AC-7).
    /// Consumers of `ambiguous` (list.rs, archive.rs) are not affected.
    pub ambiguous_candidates: HashMap<PathBuf, Vec<String>>,
    /// dir -> recorded cwds that no longer exist on disk. This is the move residue
    /// the doctor reports: transcripts that were relocated without being rewritten.
    pub stale: HashMap<PathBuf, Vec<String>>,
}

impl ProjectIndex {
    pub fn build(fs: &dyn FileSystem, home: &Path) -> Result<Self> {
        let mut by_cwd: HashMap<String, Vec<PathBuf>> = HashMap::new();
        let mut unresolved = Vec::new();
        let mut cwds = Vec::new();
        let mut ambiguous = Vec::new();
        let mut ambiguous_candidates: HashMap<PathBuf, Vec<String>> = HashMap::new();
        let mut stale: HashMap<PathBuf, Vec<String>> = HashMap::new();
        let projects = home.join(".claude").join("projects");
        let dirs = read_dir_or_empty(fs, &projects)?;
        for dir in dirs {
            if !fs.is_dir(&dir) {
                continue;
            }

            // Collect EVERY distinct cwd this dir's transcripts record - do not stop
            // at the first. A project that was moved keeps transcripts pointing at
            // its old locations, and that residue is exactly what the doctor exists
            // to report. Stopping early both discards it and makes the answer depend
            // on which filename happens to sort first.
            let mut found: Vec<String> = Vec::new();
            for child in read_dir_or_empty(fs, &dir)? {
                if child.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                if let Some(cwd) = read_stored_cwd(fs, &child) {
                    let key = normalize_path(&cwd);
                    if !found.iter().any(|f| normalize_path(f) == key) {
                        found.push(cwd);
                    }
                }
            }
            cwds.extend(found.iter().cloned());

            match found.len() {
                0 => unresolved.push(dir),
                1 => {
                    by_cwd
                        .entry(normalize_path(&found[0]))
                        .or_default()
                        .push(dir);
                }
                _ => {
                    // More than one recorded path. The live one is the project; the
                    // rest are residue. Two live paths is genuine ambiguity.
                    let live: Vec<String> = found
                        .iter()
                        .filter(|c| fs.is_dir(Path::new(c.as_str())))
                        .cloned()
                        .collect();
                    match live.len() {
                        1 => {
                            let win = &live[0];
                            let dead: Vec<String> =
                                found.iter().filter(|c| *c != win).cloned().collect();
                            stale.insert(dir.clone(), dead);
                            by_cwd.entry(normalize_path(win)).or_default().push(dir);
                        }
                        0 => {
                            // Every recorded path is gone. We cannot say which one the
                            // project was, but the dead paths are still residue and the
                            // doctor needs them to explain WHY the dir is unresolved.
                            stale.insert(dir.clone(), found.clone());
                            unresolved.push(dir);
                        }
                        _ => {
                            // Two or more live paths: genuine ambiguity. Record the
                            // candidates so build_plan can detect src matches (AC-7).
                            ambiguous_candidates.insert(dir.clone(), live);
                            ambiguous.push(dir);
                        }
                    }
                }
            }
        }
        Ok(Self {
            by_cwd,
            unresolved,
            cwds,
            ambiguous,
            ambiguous_candidates,
            stale,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemoryFileSystem;

    fn line(cwd: &str) -> String {
        format!(
            "{{\"type\":\"user\",\"cwd\":\"{}\",\"uuid\":\"x\"}}\n",
            cwd.replace('\\', "\\\\")
        )
    }

    #[test]
    fn reads_first_cwd_skipping_summary_lines() {
        let fs = MemoryFileSystem::new();
        // first line is a summary with no cwd (real transcripts start this way)
        let body = format!(
            "{{\"type\":\"last-prompt\",\"leafUuid\":\"z\"}}\n{}",
            line("E:\\Projects\\Sample Repos\\demo-notes-editor")
        );
        fs.write(
            Path::new("/h/.claude/projects/E--x/22b2.jsonl"),
            body.as_bytes(),
        )
        .unwrap();
        let got = read_stored_cwd(&fs, Path::new("/h/.claude/projects/E--x/22b2.jsonl"));
        assert_eq!(
            got.as_deref(),
            Some("E:\\Projects\\Sample Repos\\demo-notes-editor")
        );
    }

    #[test]
    fn build_maps_normalized_cwd_and_flags_unresolved() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--a/s.jsonl"),
            line("E:\\Projects\\A").as_bytes(),
        )
        .unwrap();
        // a dir whose transcript has no cwd -> unresolved
        fs.write(
            Path::new("/h/.claude/projects/E--b/s.jsonl"),
            b"{\"type\":\"last-prompt\"}\n",
        )
        .unwrap();
        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
        assert_eq!(
            idx.by_cwd.get("e:/projects/a").unwrap(),
            &vec![PathBuf::from("/h/.claude/projects/E--a")]
        );
        assert_eq!(
            idx.unresolved,
            vec![PathBuf::from("/h/.claude/projects/E--b")]
        );
        // cwds holds the ORIGINAL, non-normalized string. plugin_state::audit hashes
        // it with sha256 to locate a plugin dir, so a lowercased or slash-flipped
        // value here would produce a different digest and silently miss the dir.
        assert_eq!(idx.cwds, vec!["E:\\Projects\\A".to_string()]);
    }

    #[test]
    fn build_scans_past_a_transcript_that_has_no_cwd() {
        let fs = MemoryFileSystem::new();
        // a.jsonl sorts first and carries no cwd; b.jsonl carries it. "No cwd in the
        // first file" is not "no cwd in the directory" - the dir must still resolve.
        fs.write(
            Path::new("/h/.claude/projects/E--a/a.jsonl"),
            b"{\"type\":\"last-prompt\"}\n",
        )
        .unwrap();
        fs.write(
            Path::new("/h/.claude/projects/E--a/b.jsonl"),
            line("E:\\Projects\\A").as_bytes(),
        )
        .unwrap();
        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
        assert_eq!(
            idx.by_cwd.get("e:/projects/a").unwrap(),
            &vec![PathBuf::from("/h/.claude/projects/E--a")]
        );
        assert!(idx.unresolved.is_empty());
        assert_eq!(idx.cwds, vec!["E:\\Projects\\A".to_string()]);
    }

    /// Modeled on a real directory from the 2026-07-13 machine scan:
    /// E--Projects-prisant-labs-obsidian-tag-visibility held 17 transcripts naming
    /// THREE paths - the current one plus two dead ones left behind by earlier moves.
    /// Resolving to whichever transcript sorts first is a coin flip; resolving to the
    /// path that still exists is the answer.
    #[test]
    fn build_resolves_move_residue_to_the_path_that_still_exists() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--proj/a.jsonl"),
            line("E:\\Projects\\old\\proj").as_bytes(),
        )
        .unwrap();
        fs.write(
            Path::new("/h/.claude/projects/E--proj/b.jsonl"),
            line("E:\\Projects\\new\\proj").as_bytes(),
        )
        .unwrap();
        // Only the new location exists on disk. Note a.jsonl sorts FIRST and holds the
        // dead path, so first-wins would have resolved this dir to a folder that is gone.
        fs.write(Path::new("E:\\Projects\\new\\proj\\.keep"), b"x")
            .unwrap();

        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
        let dir = PathBuf::from("/h/.claude/projects/E--proj");

        assert_eq!(
            idx.by_cwd.get("e:/projects/new/proj").unwrap(),
            &vec![dir.clone()]
        );
        assert!(!idx.by_cwd.contains_key("e:/projects/old/proj"));
        // The dead reference is REPORTED, not silently dropped - it is the residue.
        assert_eq!(
            idx.stale.get(&dir).unwrap(),
            &vec!["E:\\Projects\\old\\proj".to_string()]
        );
        assert!(idx.ambiguous.is_empty());
        assert!(idx.unresolved.is_empty());
        // Both originals survive for plugin_state::audit - an orphaned plugin dir is
        // keyed by the OLD path, so the stale cwd is the one that finds it.
        assert!(idx.cwds.contains(&"E:\\Projects\\old\\proj".to_string()));
        assert!(idx.cwds.contains(&"E:\\Projects\\new\\proj".to_string()));
    }

    /// The real directory names THREE paths, not two. An implementation that only
    /// handles the two-path case passes every other test here and still misclassifies
    /// the actual machine, so the three-path shape gets its own test.
    #[test]
    fn build_resolves_three_recorded_paths_with_one_survivor() {
        let fs = MemoryFileSystem::new();
        let dir = "/h/.claude/projects/E--Projects-prisant-labs-obsidian-tag-visibility";
        // Two dead paths sort BEFORE the live one, so first-wins would pick a dead path.
        fs.write(
            Path::new(&format!("{dir}/a.jsonl")),
            line("E:\\Projects\\github-jprisant\\obsidian-tag-curator").as_bytes(),
        )
        .unwrap();
        fs.write(
            Path::new(&format!("{dir}/b.jsonl")),
            line("E:\\Projects\\prisant-labs\\obsidian-tag-curator").as_bytes(),
        )
        .unwrap();
        fs.write(
            Path::new(&format!("{dir}/c.jsonl")),
            line("E:\\Projects\\prisant-labs\\obsidian-tag-visibility").as_bytes(),
        )
        .unwrap();
        // a transcript with no cwd at all, as the real dir has
        fs.write(
            Path::new(&format!("{dir}/d.jsonl")),
            b"{\"type\":\"last-prompt\"}\n",
        )
        .unwrap();
        fs.write(
            Path::new("E:\\Projects\\prisant-labs\\obsidian-tag-visibility\\.keep"),
            b"x",
        )
        .unwrap();

        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
        let d = PathBuf::from(dir);

        assert_eq!(
            idx.by_cwd
                .get("e:/projects/prisant-labs/obsidian-tag-visibility")
                .unwrap(),
            &vec![d.clone()]
        );
        let stale = idx.stale.get(&d).unwrap();
        assert_eq!(stale.len(), 2);
        assert!(stale.contains(&"E:\\Projects\\prisant-labs\\obsidian-tag-curator".to_string()));
        assert!(stale.contains(&"E:\\Projects\\github-jprisant\\obsidian-tag-curator".to_string()));
        assert!(idx.ambiguous.is_empty());
        assert!(idx.unresolved.is_empty());
    }

    /// Regression test for LEAD-07. Resolution asks the filesystem "does this recorded
    /// path still exist", and NTFS answers case-insensitively. A transcript that records
    /// `e:\projects\live` while the folder on disk is `E:\Projects\Live` describes a path
    /// that DOES exist. Before MemoryFileSystem modeled NTFS casing, this test would have
    /// said the path was dead and resolved the dir to the wrong place - a wrong answer
    /// that only appeared on the real machine, never in a test.
    #[test]
    fn build_matches_a_recorded_path_case_insensitively() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--proj/a.jsonl"),
            line("E:\\Projects\\Dead").as_bytes(),
        )
        .unwrap();
        fs.write(
            Path::new("/h/.claude/projects/E--proj/b.jsonl"),
            line("e:\\projects\\live").as_bytes(),
        )
        .unwrap();
        // On disk with DIFFERENT casing than the transcript recorded.
        fs.write(Path::new("E:\\Projects\\Live\\.keep"), b"x")
            .unwrap();

        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
        let d = PathBuf::from("/h/.claude/projects/E--proj");
        assert_eq!(
            idx.by_cwd.get("e:/projects/live").unwrap(),
            &vec![d.clone()]
        );
        assert_eq!(
            idx.stale.get(&d).unwrap(),
            &vec!["E:\\Projects\\Dead".to_string()]
        );
        assert!(idx.unresolved.is_empty());
    }

    #[test]
    fn build_records_the_dead_paths_even_when_none_survive() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--proj/a.jsonl"),
            line("E:\\Projects\\gone-one").as_bytes(),
        )
        .unwrap();
        fs.write(
            Path::new("/h/.claude/projects/E--proj/b.jsonl"),
            line("E:\\Projects\\gone-two").as_bytes(),
        )
        .unwrap();
        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
        let d = PathBuf::from("/h/.claude/projects/E--proj");
        // Unresolvable, but the doctor still needs to say WHICH dead paths it saw.
        assert_eq!(idx.unresolved, vec![d.clone()]);
        assert_eq!(idx.stale.get(&d).unwrap().len(), 2);
    }

    #[test]
    fn build_refuses_when_two_recorded_paths_both_still_exist() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--proj/a.jsonl"),
            line("E:\\Projects\\one").as_bytes(),
        )
        .unwrap();
        fs.write(
            Path::new("/h/.claude/projects/E--proj/b.jsonl"),
            line("E:\\Projects\\two").as_bytes(),
        )
        .unwrap();
        fs.write(Path::new("E:\\Projects\\one\\.keep"), b"x")
            .unwrap();
        fs.write(Path::new("E:\\Projects\\two\\.keep"), b"x")
            .unwrap();

        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
        // Two live candidates. There is no honest winner, so refuse rather than guess.
        assert_eq!(
            idx.ambiguous,
            vec![PathBuf::from("/h/.claude/projects/E--proj")]
        );
        assert!(idx.by_cwd.is_empty());
    }

    #[test]
    fn build_treats_a_dir_whose_every_path_is_gone_as_unresolved() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--proj/a.jsonl"),
            line("E:\\Projects\\gone-one").as_bytes(),
        )
        .unwrap();
        fs.write(
            Path::new("/h/.claude/projects/E--proj/b.jsonl"),
            line("E:\\Projects\\gone-two").as_bytes(),
        )
        .unwrap();
        // Neither path exists. We cannot tell which one the project was.
        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
        assert_eq!(
            idx.unresolved,
            vec![PathBuf::from("/h/.claude/projects/E--proj")]
        );
        assert!(idx.by_cwd.is_empty());
        assert!(idx.ambiguous.is_empty());
    }

    struct FailingReadDir {
        inner: MemoryFileSystem,
        fail_on: String,
    }

    impl crate::fs::FileSystem for FailingReadDir {
        fn read_dir(&self, path: &std::path::Path) -> std::io::Result<Vec<std::path::PathBuf>> {
            if path.to_string_lossy().replace('\\', "/") == self.fail_on {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "denied",
                ));
            }
            self.inner.read_dir(path)
        }
        fn read(&self, path: &std::path::Path) -> std::io::Result<Vec<u8>> {
            self.inner.read(path)
        }
        fn write(&self, path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
            self.inner.write(path, data)
        }
        fn rename(&self, from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
            self.inner.rename(from, to)
        }
        fn exists(&self, path: &std::path::Path) -> bool {
            self.inner.exists(path)
        }
        fn is_file(&self, path: &std::path::Path) -> bool {
            self.inner.is_file(path)
        }
        fn is_dir(&self, path: &std::path::Path) -> bool {
            self.inner.is_dir(path)
        }
        fn create_dir_all(&self, path: &std::path::Path) -> std::io::Result<()> {
            self.inner.create_dir_all(path)
        }
        fn copy(&self, from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
            self.inner.copy(from, to)
        }
        fn remove_file(&self, path: &std::path::Path) -> std::io::Result<()> {
            self.inner.remove_file(path)
        }
        fn remove_dir_all(&self, path: &std::path::Path) -> std::io::Result<()> {
            self.inner.remove_dir_all(path)
        }
        fn mtime_secs(&self, path: &std::path::Path) -> std::io::Result<u64> {
            self.inner.mtime_secs(path)
        }
    }

    #[test]
    fn build_propagates_a_real_projects_read_error() {
        let inner = MemoryFileSystem::new();
        let fs = FailingReadDir {
            inner,
            fail_on: "/h/.claude/projects".into(),
        };
        let err = ProjectIndex::build(&fs, std::path::Path::new("/h")).unwrap_err();
        assert!(matches!(err, crate::error::AwtError::Io(_)), "{err:?}");
    }

    #[test]
    fn build_treats_a_missing_projects_dir_as_empty_not_an_error() {
        let fs = MemoryFileSystem::new(); // nothing under /h/.claude/projects
        let idx = ProjectIndex::build(&fs, std::path::Path::new("/h")).unwrap();
        assert!(idx.by_cwd.is_empty() && idx.unresolved.is_empty());
    }
}
