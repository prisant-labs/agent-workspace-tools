use crate::fs::FileSystem;
use std::path::Path;

pub struct SessionFootprint {
    pub session_ids: Vec<String>,
    pub todos: usize,
    pub file_history: usize,
    pub session_env: usize,
    pub tasks: usize,
}

pub fn footprint(fs: &dyn FileSystem, home: &Path, project_dir: &Path) -> SessionFootprint {
    let mut ids = Vec::new();
    for child in fs.read_dir(project_dir).unwrap_or_default() {
        if child.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            if let Some(stem) = child.file_stem().and_then(|s| s.to_str()) {
                ids.push(stem.to_string());
            }
        }
    }
    let count_matching = |store: &str| -> usize {
        let d = home.join(".claude").join(store);
        fs.read_dir(&d)
            .unwrap_or_default()
            .iter()
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| ids.iter().any(|id| n.contains(id.as_str())))
                    .unwrap_or(false)
            })
            .count()
    };
    SessionFootprint {
        todos: count_matching("todos"),
        file_history: count_matching("file-history"),
        session_env: count_matching("session-env"),
        tasks: count_matching("tasks"),
        session_ids: ids,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, MemoryFileSystem};
    use std::path::Path;

    #[test]
    fn links_session_keyed_stores_by_id() {
        let fs = MemoryFileSystem::new();
        let sid = "28fd093e";
        fs.write(
            Path::new("/h/.claude/projects/E--A/28fd093e.jsonl"),
            b"{}\n",
        )
        .unwrap();
        fs.write(
            Path::new("/h/.claude/todos/28fd093e-agent-28fd093e.json"),
            b"[]",
        )
        .unwrap();
        fs.write(Path::new("/h/.claude/file-history/28fd093e/x@v1"), b"x")
            .unwrap();
        let fp = footprint(&fs, Path::new("/h"), Path::new("/h/.claude/projects/E--A"));
        assert_eq!(fp.session_ids, vec![sid.to_string()]);
        assert_eq!(fp.todos, 1);
        assert_eq!(fp.file_history, 1);
    }
}
