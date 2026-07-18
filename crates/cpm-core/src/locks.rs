use crate::fs::FileSystem;
use std::path::Path;

pub fn detect_live(fs: &dyn FileSystem, home: &Path) -> Vec<String> {
    let ide = home.join(".claude").join("ide");
    fs.read_dir(&ide)
        .unwrap_or_default()
        .iter()
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("lock"))
        .map(|p| format!("live IDE lock: {}", p.display()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, MemoryFileSystem};
    use std::path::Path;

    #[test]
    fn detect_live_returns_one_entry_for_lock_file() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("/h/.claude/ide/session.lock"), b"")
            .unwrap();
        let warnings = detect_live(&fs, Path::new("/h"));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("session.lock"), "{warnings:?}");
    }

    #[test]
    fn detect_live_returns_empty_when_no_locks() {
        let fs = MemoryFileSystem::new();
        let warnings = detect_live(&fs, Path::new("/h"));
        assert!(warnings.is_empty(), "{warnings:?}");
    }
}
