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

    fn plan(&self, ctx: &Ctx, mv: &Move, hit: &Hit) -> Result<Vec<Change>> {
        use crate::error::AwtError;
        use crate::paths::encode_project_dir;
        use crate::rewrite::{anchored_rewrite, build_path_rules};
        let projects = ctx.home.join(".claude").join("projects");
        let new_dir = projects.join(encode_project_dir(&mv.dst_abs));
        // Choose between a plain rename and a merge. If the destination projects dir already
        // exists (B is a live project with its own history), a rename would try to move A's dir
        // ONTO B's - which the OS refuses. Merge A's files into B instead. If B's dir does not
        // exist, the historical rename is correct. Either way the transcripts end up at
        // new_dir/<name>, exactly where the RewriteFile changes below expect them.
        let first = if ctx.fs.is_dir(&new_dir) {
            Change::MergeDir {
                from: hit.target.clone(),
                to: new_dir.clone(),
            }
        } else {
            Change::RenameDir {
                from: hit.target.clone(),
                to: new_dir.clone(),
            }
        };
        let mut changes = vec![first];
        let rules = build_path_rules(&mv.src_abs, &mv.dst_abs);
        // One behavior, no tiers (AC-58): the moved project's own transcripts are rewritten.
        // The old Minimal tier renamed the dir and rewrote nothing, so Standard verification
        // always failed it; Full rewrote nested sidecars that verification and (pre-AC-54)
        // backup did not cover. Both were removed rather than half-kept. Sidecars still move
        // with the dir rename and still survive rollback byte-identically (AC-54); they are
        // simply never content-rewritten, which mirrors the report-only principle: a path
        // inside a memory note is a record, not a live key.
        // The read_dir is strict (AC-59): hit.target came from detect, so an unreadable dir
        // here is a failure, not an empty project.
        for child in ctx.fs.read_dir(&hit.target)? {
            if child.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                let bytes = ctx.fs.read(&child)?;
                let text = std::str::from_utf8(&bytes).map_err(|e| {
                    AwtError::UnrecognizedFormat(format!("{}: {e}", child.display()))
                })?;
                let (_, n) = anchored_rewrite(text, &rules);
                // the file lives under the NEW dir after the rename; path is post-rename
                let post = new_dir.join(child.file_name().unwrap());
                changes.push(Change::RewriteFile {
                    path: post,
                    rules: rules.clone(),
                    expected: n,
                });
            }
        }
        Ok(changes)
    }

    fn verify(&self, ctx: &Ctx, mv: &Move) -> Result<Vec<VerifyResult>> {
        use crate::paths::encode_project_dir;
        let new_dir = ctx
            .home
            .join(".claude")
            .join("projects")
            .join(encode_project_dir(&mv.dst_abs));
        // AR-02: a project whose transcripts have expired has no directory to relocate, so
        // demanding that the destination directory exist would fail a migration that is in
        // fact complete. Tell "nothing to move" apart from "the move did not happen" by
        // also looking at the source: if neither side is a directory there were never any
        // transcripts, and their continued absence is the correct postcondition. If the
        // source is still there while the destination is not, the move genuinely failed and
        // this check still catches it.
        let old_dir = ctx
            .home
            .join(".claude")
            .join("projects")
            .join(encode_project_dir(&mv.src_abs));
        let new_exists = ctx.fs.is_dir(&new_dir);
        let had_transcripts = new_exists || ctx.fs.is_dir(&old_dir);
        let mut out = vec![VerifyResult {
            check: "new projects dir exists".into(),
            ok: new_exists || !had_transcripts,
            detail: if had_transcripts {
                new_dir.to_string_lossy().into_owned()
            } else {
                format!("{} (no transcripts for this project)", new_dir.display())
            },
        }];
        let old_cwd = format!(r#""cwd":"{}""#, mv.src_abs.replace('\\', "\\\\"));
        let mut stale = 0usize;
        // A read_dir failure here is a verification failure, not emptiness (AC-59): "the
        // transcripts could not be read" must never verify as "zero stale transcripts". A
        // genuinely absent dir stays a valid empty state (the no-transcripts case above).
        let children = if new_exists {
            match ctx.fs.read_dir(&new_dir) {
                Ok(c) => c,
                Err(e) => {
                    out.push(VerifyResult {
                        check: "moved transcripts readable".into(),
                        ok: false,
                        detail: format!("{}: {e}", new_dir.display()),
                    });
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };
        for child in children {
            if child.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                let bytes = ctx.fs.read(&child)?;
                let text = std::str::from_utf8(&bytes).map_err(|e| {
                    crate::error::AwtError::UnrecognizedFormat(format!("{}: {e}", child.display()))
                })?;
                stale += text.matches(&old_cwd).count();
                for l in text.lines() {
                    if !l.trim().is_empty() && serde_json::from_str::<serde_json::Value>(l).is_err()
                    {
                        out.push(VerifyResult {
                            check: "transcript line parses".into(),
                            ok: false,
                            detail: child.to_string_lossy().into_owned(),
                        });
                        break;
                    }
                }
            }
        }
        out.push(VerifyResult {
            check: "zero old cwd in moved transcripts".into(),
            ok: stale == 0,
            detail: format!("{stale} stale"),
        });
        Ok(out)
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
        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
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
        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
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
        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
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
        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
        };
        let stale = ClaudeProjects.audit(&ctx).unwrap();
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].reference, "e:/projects/dead");
    }

    #[test]
    fn plan_emits_rename_and_rewrites() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
            b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n",
        )
        .unwrap();
        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
        };
        let mv = Move {
            src_abs: "E:\\Projects\\A".into(),
            dst_abs: "E:\\Projects\\B".into(),
        };
        let hit = ClaudeProjects.detect(&ctx, &mv).unwrap().remove(0);
        let changes = ClaudeProjects.plan(&ctx, &mv, &hit).unwrap();
        assert!(matches!(changes[0], crate::model::Change::RenameDir { .. }));
        assert!(changes
            .iter()
            .any(|c| matches!(c, crate::model::Change::RewriteFile { .. })));
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
        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
        };
        let stale = ClaudeProjects.audit(&ctx).unwrap();
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].reference, "e:/projects/dead");
    }
}
