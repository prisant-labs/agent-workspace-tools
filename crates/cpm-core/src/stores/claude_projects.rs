use crate::error::Result;
use crate::model::{Change, Ctx, Hit, Move, Stale, Store, VerifyResult};
use crate::paths::normalize_path;

pub struct ClaudeProjects;

impl ClaudeProjects {
    const ID: &'static str = "claude.projects";
}

impl Store for ClaudeProjects {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn probe(&self, _ctx: &Ctx) -> Result<()> {
        Ok(())
    }

    fn detect(&self, ctx: &Ctx, mv: &Move) -> Result<Vec<Hit>> {
        let key = normalize_path(&mv.src_abs);
        let mut hits = Vec::new();
        if let Some(dirs) = ctx.index.by_cwd.get(&key) {
            for dir in dirs {
                hits.push(Hit {
                    store: Self::ID,
                    detail: "project dir".into(),
                    target: dir.clone(),
                });
            }
        }
        Ok(hits)
    }

    fn audit(&self, ctx: &Ctx) -> Result<Vec<Stale>> {
        let mut stale = Vec::new();
        for (cwd_key, dirs) in &ctx.index.by_cwd {
            // cwd_key is normalized; reconstruct a probe path for existence
            let probe = cwd_key.replace('/', "\\");
            if !ctx.fs.exists(std::path::Path::new(&probe))
                && !ctx.fs.exists(std::path::Path::new(cwd_key))
            {
                for d in dirs {
                    stale.push(Stale {
                        store: Self::ID,
                        reference: cwd_key.clone(),
                        location: d.to_string_lossy().into_owned(),
                    });
                }
            }
        }
        Ok(stale)
    }

    fn plan(&self, _ctx: &Ctx, _mv: &Move, _hit: &Hit) -> Result<Vec<Change>> {
        Ok(vec![])
    }

    fn verify(&self, _ctx: &Ctx, _mv: &Move) -> Result<Vec<VerifyResult>> {
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, MemoryFileSystem};
    use crate::index::ProjectIndex;
    use std::path::{Path, PathBuf};

    fn cwd_line(cwd: &str) -> String {
        format!(
            "{{\"type\":\"user\",\"cwd\":\"{}\"}}\n",
            cwd.replace('\\', "\\\\")
        )
    }

    #[test]
    fn detect_finds_dir_via_reverse_index_case_insensitive() {
        let fs = MemoryFileSystem::new();
        // dir name uses capital E but stored cwd too; index matches on normalized form
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-Github-Repos-markdown-for-humans/s.jsonl"),
            cwd_line("E:\\Projects\\Github Repos\\markdown-for-humans").as_bytes(),
        )
        .unwrap();
        let idx = ProjectIndex::build(&fs, Path::new("/h"));
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
            scope: crate::model::Scope::Standard,
        };
        let mv = Move {
            src_abs: "E:\\Projects\\Github Repos\\markdown-for-humans".into(),
            dst_abs: "E:\\Projects\\prisant-labs\\vs-code-markdown-max".into(),
        };
        let hits = ClaudeProjects.detect(&ctx, &mv).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0]
            .target
            .ends_with("E--Projects-Github-Repos-markdown-for-humans"));
    }

    #[test]
    fn detect_returns_empty_when_no_matching_cwd() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-Other/s.jsonl"),
            cwd_line("E:\\Projects\\Other").as_bytes(),
        )
        .unwrap();
        let idx = ProjectIndex::build(&fs, Path::new("/h"));
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
            scope: crate::model::Scope::Standard,
        };
        let mv = Move {
            src_abs: "E:\\Projects\\Github Repos\\markdown-for-humans".into(),
            dst_abs: "E:\\Projects\\prisant-labs\\vs-code-markdown-max".into(),
        };
        let hits = ClaudeProjects.detect(&ctx, &mv).unwrap();
        assert_eq!(hits.len(), 0);
    }

    #[test]
    fn audit_reports_stale_directories() {
        let fs = MemoryFileSystem::new();
        // Write a transcript in a project dir, but DON'T write the cwd path itself
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-Deleted/s.jsonl"),
            cwd_line("E:\\Projects\\Deleted").as_bytes(),
        )
        .unwrap();
        let idx = ProjectIndex::build(&fs, Path::new("/h"));
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
            scope: crate::model::Scope::Standard,
        };
        let stale = ClaudeProjects.audit(&ctx).unwrap();
        assert_eq!(stale.len(), 1);
        // reference is in normalized form (forward slashes, lowercase)
        assert_eq!(stale[0].reference, "e:/projects/deleted");
        assert!(stale[0].location.contains("E--Projects-Deleted"));
    }

    /// Both audit tests below seed a genuinely dead directory ALONGSIDE the one under
    /// test, and assert that exactly the dead one is reported. Asserting only
    /// `stale.len() == 0` would pass against a stub that returns nothing at all, so it
    /// could not tell working code from an empty function body - the vacuous-green shape
    /// of audit finding B-01. The contrast is what makes the assertion mean something.
    #[test]
    fn audit_ignores_live_directories() {
        let fs = MemoryFileSystem::new();
        // A live project: transcript AND the cwd path itself exist.
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-Live/s.jsonl"),
            cwd_line("E:\\Projects\\Live").as_bytes(),
        )
        .unwrap();
        fs.write(Path::new("E:\\Projects\\Live\\dummy.txt"), b"data")
            .unwrap();
        // A dead project, to prove audit reports SOMETHING and is not simply inert.
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-Dead/s.jsonl"),
            cwd_line("E:\\Projects\\Dead").as_bytes(),
        )
        .unwrap();
        let idx = ProjectIndex::build(&fs, Path::new("/h"));
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
            scope: crate::model::Scope::Standard,
        };
        let stale = ClaudeProjects.audit(&ctx).unwrap();
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].reference, "e:/projects/dead");
    }

    #[test]
    fn audit_case_insensitive_for_path_existence() {
        let fs = MemoryFileSystem::new();
        // The transcript records a LOWERCASE path while the directory on disk is
        // capitalized. NTFS matches case-insensitively, so this project is LIVE and
        // must not be reported stale (audit finding LEAD-07).
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-Case/s.jsonl"),
            cwd_line("e:\\projects\\case").as_bytes(),
        )
        .unwrap();
        fs.write(Path::new("E:\\Projects\\Case\\dummy.txt"), b"data")
            .unwrap();
        // A genuinely dead project, so "one stale, and it is the dead one" is a claim a
        // stub cannot satisfy.
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-Dead/s.jsonl"),
            cwd_line("E:\\Projects\\Dead").as_bytes(),
        )
        .unwrap();
        let idx = ProjectIndex::build(&fs, Path::new("/h"));
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
            scope: crate::model::Scope::Standard,
        };
        let stale = ClaudeProjects.audit(&ctx).unwrap();
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].reference, "e:/projects/dead");
    }
}
