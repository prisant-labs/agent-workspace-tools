use crate::error::{AwtError, Result};
use crate::model::{Change, Ctx, Hit, Move, Stale, Store, VerifyResult};
use crate::paths::{encode_project_dir, normalize_path, trim_separators};
use std::path::{Path, PathBuf};

pub struct ClaudeProjects;

/// What the folder at a project's encoded name means for that project.
enum NameMatch {
    /// No folder has that name.
    NoFolder,
    /// Nothing ties the folder to another path. `named` says its own transcripts name this
    /// path. Without that, the folder name is the only link.
    Ours { dir: PathBuf, named: bool },
    /// The folder has this project's encoded name, but something ties it to another path.
    Others { dir: PathBuf, owner: String },
    /// The folder's transcripts name this path and a look-alike, so it could be either's.
    Contested { dir: PathBuf, paths: Vec<String> },
}

impl ClaudeProjects {
    const ID: &'static str = "claude.projects";
    const NAME_MATCH_DETAIL: &'static str = "project dir, matched by folder name";

    /// Decide what the folder at `src`'s encoded name is to `src` (AR-10).
    ///
    /// The encoding is lossy, so a name alone never proves ownership. `detect` and `verify` both
    /// ask this one question, because the defect was the two of them disagreeing. Anything that
    /// ties the folder to another path makes it that path's: taking another project's memory is
    /// a worse failure than leaving a folder behind.
    ///
    /// The evidence, strongest first: the folder's top-level transcripts through the index, then
    /// its nested transcripts, then any other known path with the same encoded name.
    fn name_match(ctx: &Ctx, src: &str) -> Result<NameMatch> {
        let src = trim_separators(src);
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
        let is_src = |p: &str| normalize_path(trim_separators(p)) == key;
        let is_lookalike = |p: &str| {
            !is_src(p) && encode_project_dir(trim_separators(p)).eq_ignore_ascii_case(&encoded)
        };
        // Paths the folder itself records. A second path that encodes differently is an older
        // home of the same project. One that encodes alike is a genuine rival.
        let decide = |paths: &[String]| {
            if !paths.iter().any(|p| is_src(p)) {
                NameMatch::Others {
                    dir: dir.clone(),
                    owner: paths.first().cloned().unwrap_or_default(),
                }
            } else if paths.iter().any(|p| is_lookalike(p)) {
                NameMatch::Contested {
                    dir: dir.clone(),
                    paths: paths.to_vec(),
                }
            } else {
                NameMatch::Ours {
                    dir: dir.clone(),
                    named: true,
                }
            }
        };

        for (cwd, dirs) in &ctx.index.by_cwd {
            if dirs.iter().any(|d| has_name(d)) {
                return Ok(decide(std::slice::from_ref(cwd)));
            }
        }
        let recorded = ctx
            .index
            .ambiguous_candidates
            .iter()
            .chain(ctx.index.stale.iter())
            .find(|(d, _)| has_name(d));
        if let Some((_, paths)) = recorded {
            return Ok(decide(paths));
        }

        // The index reads only top-level transcripts. Session folders can name a path too.
        let mut nested: Vec<String> = Vec::new();
        for file in crate::fs::walk_files(ctx.fs, &dir) {
            if file.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            if let Some(cwd) = crate::index::read_stored_cwd(ctx.fs, &file) {
                if !nested
                    .iter()
                    .any(|n| normalize_path(n) == normalize_path(&cwd))
                {
                    nested.push(cwd);
                }
            }
        }
        if !nested.is_empty() {
            return Ok(decide(&nested));
        }

        // The folder says nothing, so any other known path with the same name could own it.
        let known = Self::known_paths(ctx)?;
        let rival = ctx
            .index
            .cwds
            .iter()
            .chain(known.iter())
            .find(|p| is_lookalike(p));
        Ok(match rival {
            Some(p) => NameMatch::Others {
                dir,
                owner: p.clone(),
            },
            None => NameMatch::Ours { dir, named: false },
        })
    }

    /// Every path `~/.claude.json` and `history.jsonl` record.
    fn known_paths(ctx: &Ctx) -> Result<Vec<String>> {
        let mut out = Self::claude_json_paths(ctx)?;
        let history = ctx.home.join(".claude").join("history.jsonl");
        if ctx.fs.exists(&history) {
            // Read-only comparison, so lossy is safe. The history store refuses invalid UTF-8
            // on its own path when a write is planned.
            let bytes = ctx.fs.read(&history)?;
            let mut seen = std::collections::HashSet::new();
            for line in String::from_utf8_lossy(&bytes).lines() {
                let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                    continue;
                };
                if let Some(p) = v.get("project").and_then(|x| x.as_str()) {
                    if seen.insert(p.to_string()) {
                        out.push(p.to_string());
                    }
                }
            }
        }
        Ok(out)
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

    /// Whether anything besides a folder name says this path is a real project. Without this, a
    /// typo such as foo.bar for foo-bar would take foo-bar's folder.
    fn path_is_known(ctx: &Ctx, src: &str) -> Result<bool> {
        let src = trim_separators(src);
        if ctx.fs.is_dir(Path::new(&src.replace('\\', "/"))) {
            return Ok(true);
        }
        let key = normalize_path(src);
        Ok(Self::known_paths(ctx)?
            .iter()
            .any(|p| normalize_path(trim_separators(p)) == key))
    }

    /// What the plan must say, or refuse, when a folder name decided what happens to a folder.
    pub fn name_match_warning(ctx: &Ctx, mv: &Move) -> Result<Option<String>> {
        let indexed = ctx.index.by_cwd.contains_key(&normalize_path(&mv.src_abs));
        Ok(match Self::name_match(ctx, &mv.src_abs)? {
            NameMatch::NoFolder => None,
            NameMatch::Contested { dir, paths } => {
                return Err(AwtError::Ambiguous(format!(
                    "the project dir {} could belong to more than one path with the same encoded \
                     name ({}). The tool will not guess which is correct - resolve this manually.",
                    dir.display(),
                    paths.join(", ")
                )));
            }
            NameMatch::Ours { named: true, .. } => None,
            NameMatch::Ours { .. } if indexed || !Self::path_is_known(ctx, &mv.src_abs)? => None,
            NameMatch::Ours { dir, .. } => Some(format!(
                "treating {} as this project's by its folder name: no readable transcript names \
                 {}, so the name is the only link. Another project with the same encoded name \
                 could own it, and awt cannot see one that nothing records.",
                dir.display(),
                mv.src_abs
            )),
            NameMatch::Others { .. } if indexed => None,
            NameMatch::Others { dir, owner } => Some(format!(
                "left {} where it is: its name matches {}, but it belongs to {owner}.",
                dir.display(),
                mv.src_abs
            )),
        })
    }

    /// Recorded paths under `src` whose state folder exists. The index knows only projects with a
    /// readable transcript, so a memory-only child would otherwise be stranded by a parent move.
    pub fn nested_state_paths(ctx: &Ctx, src: &str) -> Result<Vec<String>> {
        let prefix = format!("{}/", normalize_path(trim_separators(src)));
        let projects = ctx.home.join(".claude").join("projects");
        let mut out: Vec<String> = Vec::new();
        for path in Self::claude_json_paths(ctx)? {
            let path = trim_separators(&path).to_string();
            let key = normalize_path(&path);
            if key.starts_with(&prefix)
                && ctx.fs.is_dir(&projects.join(encode_project_dir(&path)))
                && !out.contains(&key)
            {
                out.push(key);
            }
        }
        Ok(out)
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
        }
        // AR-10: a folder no transcript resolves, which the folder name alone points at. A
        // differently named transcript folder can sit beside it, and both are this project's.
        let by_name = match Self::name_match(ctx, &mv.src_abs)? {
            NameMatch::Ours { dir, named }
                if named || !hits.is_empty() || Self::path_is_known(ctx, &mv.src_abs)? =>
            {
                Some(dir)
            }
            NameMatch::Contested { dir, .. } => Some(dir),
            _ => None,
        };
        if let Some(dir) = by_name {
            let same = |h: &Hit| {
                h.target
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&dir.to_string_lossy())
            };
            if !hits.iter().any(same) {
                hits.push(Hit {
                    store: Self::ID,
                    detail: Self::NAME_MATCH_DETAIL.into(),
                    target: dir,
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
        use crate::rewrite::{anchored_rewrite, build_path_rules};
        let projects = ctx.home.join(".claude").join("projects");
        let encoded_dst = encode_project_dir(trim_separators(&mv.dst_abs));
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
                // The folder at the destination's name can belong to a look-alike of the
                // destination. Merging would hand this project's state to that project.
                if let NameMatch::Others { dir, owner } = Self::name_match(ctx, &mv.dst_abs)? {
                    let rival = encode_project_dir(trim_separators(&owner));
                    if rival.eq_ignore_ascii_case(&encoded_dst) {
                        return Err(AwtError::DestinationExists(format!(
                            "{} (this folder belongs to {owner}, whose encoded name matches {}. \
                             Choose another destination)",
                            dir.display(),
                            mv.dst_abs
                        )));
                    }
                }
                // A merge refuses to overwrite a file, so a file on both sides would fail
                // at apply. MEMORY.md has a fixed name, which makes that the normal case
                // for two projects with memory. Say so here, where nothing has happened.
                if let Some(conflict) = crate::plan::merge_conflict(ctx.fs, &hit.target, &new_dir)?
                {
                    return Err(AwtError::DestinationExists(conflict));
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
                // Verification requires every line to parse, so apply could only fail and roll
                // back. Keep the promise that plan shows what apply will do.
                let broken = text.lines().position(|l| {
                    !l.trim().is_empty() && serde_json::from_str::<serde_json::Value>(l).is_err()
                });
                if let Some(at) = broken {
                    return Err(AwtError::UnrecognizedFormat(format!(
                        "{}: line {} is not valid JSON, so this transcript cannot be verified \
                         after a move. Repair or remove that line, then re-run",
                        child.display(),
                        at + 1
                    )));
                }
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
        let old_is_ours = matches!(
            Self::name_match(ctx, &mv.src_abs)?,
            NameMatch::Ours { .. } | NameMatch::Contested { .. }
        );
        let had_transcripts = new_exists || old_is_ours;
        let mut out = vec![VerifyResult {
            check: "new projects dir exists".into(),
            ok: new_exists || !had_transcripts,
            detail: if had_transcripts {
                new_dir.to_string_lossy().into_owned()
            } else {
                format!("{} (no transcripts for this project)", new_dir.display())
            },
        }];
        // A folder at each name means a rename or merge did not happen. The check above cannot
        // see that, because the new name can already exist. A rename that keeps the encoded
        // name has one folder, which is both.
        let same_name = encode_project_dir(trim_separators(&mv.src_abs))
            .eq_ignore_ascii_case(&encode_project_dir(trim_separators(&mv.dst_abs)));
        if new_exists && old_is_ours && !same_name {
            out.push(VerifyResult {
                check: "old projects dir gone".into(),
                ok: false,
                detail: format!("the folder for {} is still there", mv.src_abs),
            });
        }
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
