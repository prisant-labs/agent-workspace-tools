use crate::error::{AwtError, Result};
use crate::model::{Change, Ctx, Hit, Move, Stale, Store, VerifyResult};
use crate::paths::{encode_project_dir, normalize_path};
use std::path::{Path, PathBuf};

pub struct ClaudeProjects;

/// What the folder at a project's encoded name means for that project.
enum NameMatch {
    /// No folder has that name.
    NoFolder,
    /// The folder is this project's: nothing ties it to another path.
    Ours(PathBuf),
    /// The folder has this project's encoded name, but it belongs to another path.
    Others { dir: PathBuf, owner: String },
}

impl ClaudeProjects {
    const ID: &'static str = "claude.projects";
    const NAME_MATCH_DETAIL: &'static str = "project dir, matched by folder name";

    /// Decide whether the folder at `src`'s encoded name is `src`'s (AR-10).
    ///
    /// The encoding is lossy, so a name alone never proves ownership. `detect` and `verify` both
    /// ask this one question, because the defect was the two of them disagreeing. Anything that
    /// ties the folder to another path makes it that path's: taking another project's memory is
    /// a worse failure than leaving a folder behind.
    fn name_match(ctx: &Ctx, src: &str) -> Result<NameMatch> {
        let encoded = encode_project_dir(src);
        let projects = ctx.home.join(".claude").join("projects");
        let children = match ctx.fs.read_dir(&projects) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(NameMatch::NoFolder),
            Err(e) => return Err(e.into()),
        };
        // Case-insensitive: the name comes from the launch-time spelling, so the drive letter
        // can differ from the path the person typed.
        let has_name = |p: &Path| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(&encoded))
        };
        let Some(dir) = children
            .into_iter()
            .find(|c| has_name(c) && ctx.fs.is_dir(c))
        else {
            return Ok(NameMatch::NoFolder);
        };
        let key = normalize_path(src);
        let others = |owner: &str| NameMatch::Others {
            dir: dir.clone(),
            owner: owner.to_string(),
        };

        // What the folder's own transcripts say comes first.
        for (cwd, dirs) in &ctx.index.by_cwd {
            if dirs.iter().any(|d| has_name(d)) {
                return Ok(if *cwd == key {
                    NameMatch::Ours(dir)
                } else {
                    others(cwd)
                });
            }
        }
        let recorded = ctx
            .index
            .ambiguous_candidates
            .iter()
            .chain(ctx.index.stale.iter())
            .find(|(d, _)| has_name(d));
        if let Some((_, paths)) = recorded {
            return Ok(if paths.iter().any(|p| normalize_path(p) == key) {
                NameMatch::Ours(dir)
            } else {
                others(paths.first().map_or("another path", |p| p.as_str()))
            });
        }

        // The folder says nothing, so any other known path with the same name could own it.
        let lookalike = |p: &String| {
            normalize_path(p) != key && encode_project_dir(p).eq_ignore_ascii_case(&encoded)
        };
        if let Some(p) = ctx.index.cwds.iter().find(|p| lookalike(p)) {
            return Ok(others(p));
        }
        if let Some(p) = Self::claude_json_paths(ctx)?.iter().find(|p| lookalike(p)) {
            return Ok(others(p));
        }
        Ok(NameMatch::Ours(dir))
    }

    /// Every path `~/.claude.json` records, as `projects` keys or `githubRepoPaths` values.
    fn claude_json_paths(ctx: &Ctx) -> Result<Vec<String>> {
        let path = ctx.home.join(".claude.json");
        if !ctx.fs.exists(&path) {
            return Ok(Vec::new());
        }
        let bytes = ctx.fs.read(&path)?;
        let v: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| AwtError::UnrecognizedFormat(format!("claude.json parse: {e}")))?;
        let mut out = Vec::new();
        if let Some(projects) = v.get("projects").and_then(|p| p.as_object()) {
            out.extend(projects.keys().cloned());
        }
        if let Some(repos) = v.get("githubRepoPaths").and_then(|r| r.as_object()) {
            for paths in repos.values().filter_map(|p| p.as_array()) {
                out.extend(paths.iter().filter_map(|p| p.as_str()).map(str::to_string));
            }
        }
        Ok(out)
    }

    /// A warning for the plan when the folder name alone decided what happens to a folder.
    pub fn name_match_warning(ctx: &Ctx, mv: &Move) -> Result<Option<String>> {
        if ctx.index.by_cwd.contains_key(&normalize_path(&mv.src_abs)) {
            return Ok(None);
        }
        Ok(match Self::name_match(ctx, &mv.src_abs)? {
            NameMatch::NoFolder => None,
            NameMatch::Ours(dir) => Some(format!(
                "moving {} by its folder name: no readable transcript names {}, so the name is \
                 the only link. Check that this folder is this project's before you apply.",
                dir.display(),
                mv.src_abs
            )),
            NameMatch::Others { dir, owner } => Some(format!(
                "left {} where it is: its name matches {}, but it belongs to {owner}.",
                dir.display(),
                mv.src_abs
            )),
        })
    }
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
        } else if let NameMatch::Ours(dir) = Self::name_match(ctx, &mv.src_abs)? {
            // AR-10: no readable transcript names this path, so the folder name decides.
            hits.push(Hit {
                store: Self::ID,
                detail: Self::NAME_MATCH_DETAIL.into(),
                target: dir,
            });
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
        use crate::rewrite::{anchored_rewrite, build_path_rules};
        let projects = ctx.home.join(".claude").join("projects");
        let encoded_dst = encode_project_dir(&mv.dst_abs);
        // A rename such as foo_bar to foo-bar keeps the encoded name, so the folder is
        // already in place. Only its transcripts need rewriting.
        let same_folder = hit
            .target
            .file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(&encoded_dst));
        let new_dir = if same_folder {
            hit.target.clone()
        } else {
            projects.join(&encoded_dst)
        };
        // Choose between a plain rename and a merge. If the destination projects dir already
        // exists (B is a live project with its own history), a rename would try to move A's dir
        // ONTO B's - which the OS refuses. Merge A's files into B instead. If B's dir does not
        // exist, the historical rename is correct. Either way the transcripts end up at
        // new_dir/<name>, exactly where the RewriteFile changes below expect them.
        let mut changes = Vec::new();
        if !same_folder {
            if ctx.fs.is_dir(&new_dir) {
                // A merge refuses to overwrite a file, so a file on both sides would fail
                // at apply. MEMORY.md has a fixed name, which makes that the normal case
                // for two projects with memory. Say so here, where nothing has happened.
                for file in crate::fs::walk_files_strict(ctx.fs, &hit.target)? {
                    let dest = file
                        .strip_prefix(&hit.target)
                        .map(|rel| new_dir.join(rel))
                        .unwrap_or_default();
                    if ctx.fs.exists(&dest) {
                        return Err(AwtError::DestinationExists(format!(
                            "{} (both project folders hold this file, and a merge never \
                             overwrites one. Move or combine it by hand, then re-run)",
                            dest.display()
                        )));
                    }
                }
                changes.push(Change::MergeDir {
                    from: hit.target.clone(),
                    to: new_dir.clone(),
                });
            } else {
                changes.push(Change::RenameDir {
                    from: hit.target.clone(),
                    to: new_dir.clone(),
                });
            }
        }
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
        // this check still catches it. AR-10: "the source" means a folder this project owns.
        // One that only shares its encoded name was never planned to move, so it stays.
        let new_exists = ctx.fs.is_dir(&new_dir);
        let had_transcripts =
            new_exists || matches!(Self::name_match(ctx, &mv.src_abs)?, NameMatch::Ours(_));
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
                    AwtError::UnrecognizedFormat(format!("{}: {e}", child.display()))
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
            Path::new("/h/.claude/projects/E--Projects-Sample-Repos-demo-notes-editor/s.jsonl"),
            cwd_line("E:\\Projects\\Sample Repos\\demo-notes-editor").as_bytes(),
        )
        .unwrap();
        let idx = ProjectIndex::build(&fs, Path::new("/h")).unwrap();
        let ctx = Ctx {
            fs: &fs,
            home: PathBuf::from("/h"),
            index: &idx,
        };
        let mv = Move {
            src_abs: "E:\\Projects\\Sample Repos\\demo-notes-editor".into(),
            dst_abs: "E:\\Projects\\demo-labs\\demo-notes-editor-pro".into(),
        };
        let hits = ClaudeProjects.detect(&ctx, &mv).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0]
            .target
            .ends_with("E--Projects-Sample-Repos-demo-notes-editor"));
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
            src_abs: "E:\\Projects\\Sample Repos\\demo-notes-editor".into(),
            dst_abs: "E:\\Projects\\demo-labs\\demo-notes-editor-pro".into(),
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
