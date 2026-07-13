use crate::fs::FileSystem;
use crate::paths::normalize_path;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

pub struct ProjectIndex {
    pub by_cwd: HashMap<String, Vec<PathBuf>>,
    pub unresolved: Vec<PathBuf>,
    pub cwds: Vec<String>,
}

impl ProjectIndex {
    pub fn build(fs: &dyn FileSystem, home: &Path) -> Self {
        let mut by_cwd: HashMap<String, Vec<PathBuf>> = HashMap::new();
        let mut unresolved = Vec::new();
        let mut cwds = Vec::new();
        let projects = home.join(".claude").join("projects");
        let dirs = fs.read_dir(&projects).unwrap_or_default();
        for dir in dirs {
            if !fs.is_dir(&dir) {
                continue;
            }
            let mut found = None;
            for child in fs.read_dir(&dir).unwrap_or_default() {
                if child.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    if let Some(cwd) = read_stored_cwd(fs, &child) {
                        found = Some(cwd);
                        break;
                    }
                }
            }
            match found {
                Some(cwd) => {
                    cwds.push(cwd.clone()); // ORIGINAL stored form, used by plugin_state::audit
                    by_cwd.entry(normalize_path(&cwd)).or_default().push(dir);
                }
                None => unresolved.push(dir),
            }
        }
        Self {
            by_cwd,
            unresolved,
            cwds,
        }
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
            line("E:\\Projects\\Github Repos\\markdown-for-humans")
        );
        fs.write(
            Path::new("/h/.claude/projects/E--x/22b2.jsonl"),
            body.as_bytes(),
        )
        .unwrap();
        let got = read_stored_cwd(&fs, Path::new("/h/.claude/projects/E--x/22b2.jsonl"));
        assert_eq!(
            got.as_deref(),
            Some("E:\\Projects\\Github Repos\\markdown-for-humans")
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
        let idx = ProjectIndex::build(&fs, Path::new("/h"));
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
        let idx = ProjectIndex::build(&fs, Path::new("/h"));
        assert_eq!(
            idx.by_cwd.get("e:/projects/a").unwrap(),
            &vec![PathBuf::from("/h/.claude/projects/E--a")]
        );
        assert!(idx.unresolved.is_empty());
        assert_eq!(idx.cwds, vec!["E:\\Projects\\A".to_string()]);
    }
}
