use crate::error::{CpmError, Result};
use crate::fs::FileSystem;
use crate::index::ProjectIndex;
use crate::locks::detect_live;
use crate::model::{Change, Ctx, Move, Scope};
use crate::paths::{normalize_path, same_volume};
use crate::stores::registry;
use std::path::Path;

#[derive(Clone)]
pub enum Collision {
    Refuse,
    KeepDest,
    KeepSrc,
}
pub struct PlanOpts {
    pub recursive: bool,
    pub on_collision: Collision,
    pub force: bool,
    pub move_folder: bool,
    pub scope: Scope,
}
#[derive(Debug)]
pub struct Plan {
    pub mv: Move,
    pub changes: Vec<Change>,
    pub warnings: Vec<String>,
    pub nested: Vec<String>,
    pub home: std::path::PathBuf,
}

pub fn build_plan(fs: &dyn FileSystem, home: &Path, mv: &Move, opts: &PlanOpts) -> Result<Plan> {
    // Guard: cross-volume moves are not supported in v1.0 (spec AC-2, deferred to v1.x).
    // Checked first so the refusal is immediate and unambiguous.
    if opts.move_folder && !same_volume(&mv.src_abs, &mv.dst_abs) {
        return Err(CpmError::CrossVolume(format!(
            "cross-volume move refused: source {} is on a different volume from \
             destination {}. Cross-volume moves are not supported in v1.0 \
             (spec AC-2, deferred). Move within the same volume instead.",
            mv.src_abs, mv.dst_abs
        )));
    }
    // Guard: destination folder exists (only relevant when we will move the folder there)
    if opts.move_folder && fs.exists(Path::new(&mv.dst_abs.replace('\\', "/"))) {
        return Err(CpmError::DestinationExists(mv.dst_abs.clone()));
    }
    // Guard: worktree source (.git is a file, not a dir)
    let git = format!("{}/.git", mv.src_abs.replace('\\', "/"));
    if fs.is_file(Path::new(&git)) && !opts.force {
        return Err(CpmError::WorktreeSource(mv.src_abs.clone()));
    }
    // Guard: live IDE lock files signal a running CLI that may be editing the project.
    // Without --force this is a hard refusal; with --force we warn and continue.
    let live_locks = detect_live(fs, home);
    let mut warnings = Vec::new();
    if !live_locks.is_empty() {
        if !opts.force {
            return Err(CpmError::Locked(format!(
                "live IDE lock detected - a running Claude Code CLI may be editing \
                 your Claude state. Lock(s): {}. Close the running CLI first, or \
                 pass --force to proceed anyway.",
                live_locks.join(", ")
            )));
        }
        warnings.push(format!(
            "proceeding despite {} live lock(s): {}. Editing Claude state while \
             a CLI instance is running could cause conflicts.",
            live_locks.len(),
            live_locks.join(", ")
        ));
    }

    let index = ProjectIndex::build(fs, home)?;
    let src_key = normalize_path(&mv.src_abs);

    // Guard: ambiguous history - src matches a project dir that records two or more
    // live paths. The tool refuses rather than guessing which path is correct (AC-7).
    for (dir, candidates) in &index.ambiguous_candidates {
        if candidates.iter().any(|c| normalize_path(c) == src_key) {
            let cands = candidates.join(", ");
            return Err(CpmError::Ambiguous(format!(
                "the project dir {} could belong to more than one live path ({}). \
                 The tool will not guess which path is correct - resolve this manually. \
                 The --attribute resolver is planned for v1.x.",
                dir.display(),
                cands
            )));
        }
    }

    let ctx = Ctx {
        fs,
        home: home.to_path_buf(),
        index: &index,
        scope: opts.scope,
    };
    let mut changes = Vec::new();
    let mut nested = Vec::new();

    for store in registry() {
        store.probe(&ctx)?;
        for hit in store.detect(&ctx, mv)? {
            // Collision guard for claude.json destination key
            if store.id() == "claude.json" {
                if let Collision::Refuse = opts.on_collision {
                    if dest_key_exists(&ctx, mv)? {
                        return Err(CpmError::DestinationExists(format!(
                            "claude.json already has a key for {}",
                            mv.dst_abs
                        )));
                    }
                }
            }
            changes.extend(store.plan(&ctx, mv, &hit)?);
        }
    }

    // Nested project detection (keys strictly under src)
    for k in index.by_cwd.keys() {
        if k != &src_key && k.starts_with(&format!("{src_key}/")) {
            nested.push(k.clone());
        }
    }
    if !nested.is_empty() && !opts.recursive {
        warnings.push(format!(
            "{} nested project(s) will break unless --recursive",
            nested.len()
        ));
    }

    // Folder move is the LAST change (see apply ordering). Skipped for associate mode.
    if opts.move_folder {
        changes.push(Change::MoveTree {
            from: Path::new(&mv.src_abs.replace('\\', "/")).to_path_buf(),
            to: Path::new(&mv.dst_abs.replace('\\', "/")).to_path_buf(),
        });
    }
    Ok(Plan {
        mv: mv.clone(),
        changes,
        warnings,
        nested,
        home: home.to_path_buf(),
    })
}

fn dest_key_exists(ctx: &Ctx, mv: &Move) -> Result<bool> {
    let p = ctx.home.join(".claude.json");
    if !ctx.fs.exists(&p) {
        return Ok(false);
    }
    let v: serde_json::Value = serde_json::from_slice(&ctx.fs.read(&p)?)
        .map_err(|e| CpmError::UnrecognizedFormat(e.to_string()))?;
    let dk = normalize_path(&mv.dst_abs);
    Ok(v.get("projects")
        .and_then(|x| x.as_object())
        .map(|o| o.keys().any(|k| normalize_path(k) == dk))
        .unwrap_or(false))
}

pub fn render_plan(plan: &Plan) -> String {
    let mut s = format!("Move {} -> {}\n", plan.mv.src_abs, plan.mv.dst_abs);
    for w in &plan.warnings {
        s.push_str(&format!("  WARNING: {w}\n"));
    }
    for c in &plan.changes {
        s.push_str(&match c {
            Change::RenameDir { from, to } => {
                format!("  rename dir {} -> {}\n", from.display(), to.display())
            }
            Change::MergeDir { from, to } => {
                format!("  merge dir  {} -> {}\n", from.display(), to.display())
            }
            Change::MoveTree { from, to } => {
                format!("  move tree  {} -> {}\n", from.display(), to.display())
            }
            Change::RewriteFile { path, expected, .. } => {
                format!("  rewrite    {} ({expected} edits)\n", path.display())
            }
            Change::RenameJsonKey { path, from, to, .. } => {
                format!("  json key   {} {from} -> {to}\n", path.display())
            }
            Change::RewriteJsonArrayValue { path, from, to, .. } => {
                format!("  json array {} {from} -> {to}\n", path.display())
            }
        });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemoryFileSystem;
    use crate::model::Move;
    use std::path::Path;

    fn opts() -> PlanOpts {
        PlanOpts {
            recursive: false,
            on_collision: Collision::Refuse,
            force: false,
            move_folder: true,
            scope: crate::model::Scope::Standard,
        }
    }

    #[test]
    fn refuses_when_destination_folder_exists() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("E:/Projects/B/keep.txt"), b"x").unwrap();
        let mv = Move {
            src_abs: "E:\\Projects\\A".into(),
            dst_abs: "E:\\Projects\\B".into(),
        };
        let err = build_plan(&fs, Path::new("/h"), &mv, &opts()).unwrap_err();
        assert!(matches!(err, crate::error::CpmError::DestinationExists(_)));
    }

    #[test]
    fn flags_worktree_source_without_force() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("E:/Projects/A/.git"), b"gitdir: ../real")
            .unwrap(); // .git is a FILE
        let mv = Move {
            src_abs: "E:\\Projects\\A".into(),
            dst_abs: "E:\\Projects\\B".into(),
        };
        let err = build_plan(&fs, Path::new("/h"), &mv, &opts()).unwrap_err();
        assert!(matches!(err, crate::error::CpmError::WorktreeSource(_)));
    }

    // ---------------------------------------------------------------------------
    // Sub-task 1a (AC-1): cross-volume guard
    // ---------------------------------------------------------------------------

    #[test]
    fn refuses_cross_volume_move_when_move_folder_true() {
        // src on E:, dst on F: - different volumes, no stores to hit
        let fs = MemoryFileSystem::new();
        let mv = Move {
            src_abs: "E:\\Projects\\A".into(),
            dst_abs: "F:\\Projects\\B".into(),
        };
        let err = build_plan(&fs, Path::new("/h"), &mv, &opts()).unwrap_err();
        assert!(
            matches!(err, crate::error::CpmError::CrossVolume(_)),
            "expected CrossVolume, got {err:?}"
        );
    }

    // ---------------------------------------------------------------------------
    // Sub-task 1b (AC-21): lock detection
    // ---------------------------------------------------------------------------

    #[test]
    fn refuses_when_lock_exists_and_force_false() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("/h/.claude/ide/session.lock"), b"")
            .unwrap();
        let mv = Move {
            src_abs: "E:\\Projects\\A".into(),
            dst_abs: "E:\\Projects\\B".into(),
        };
        let err = build_plan(&fs, Path::new("/h"), &mv, &opts()).unwrap_err();
        assert!(
            matches!(err, crate::error::CpmError::Locked(_)),
            "expected Locked, got {err:?}"
        );
    }

    #[test]
    fn proceeds_with_warning_when_lock_exists_and_force_true() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("/h/.claude/ide/session.lock"), b"")
            .unwrap();
        let mv = Move {
            src_abs: "E:\\Projects\\A".into(),
            dst_abs: "E:\\Projects\\B".into(),
        };
        let mut o = opts();
        o.force = true;
        let plan = build_plan(&fs, Path::new("/h"), &mv, &o).unwrap();
        assert!(
            plan.warnings.iter().any(|w| w.contains("lock")),
            "expected a lock warning in plan.warnings, got {:?}",
            plan.warnings
        );
    }

    // ---------------------------------------------------------------------------
    // Sub-task 1c (AC-7): ambiguous attribution, fail-closed surfacing
    // ---------------------------------------------------------------------------

    fn transcript_line(cwd: &str) -> Vec<u8> {
        format!(
            "{{\"type\":\"user\",\"cwd\":\"{}\",\"uuid\":\"x\"}}\n",
            cwd.replace('\\', "\\\\")
        )
        .into_bytes()
    }

    #[test]
    fn refuses_when_src_matches_ambiguous_history() {
        let fs = MemoryFileSystem::new();
        // One projects/ dir whose transcripts record two live cwds - genuine ambiguity
        fs.write(
            Path::new("/h/.claude/projects/E--proj/a.jsonl"),
            &transcript_line("E:\\Projects\\one"),
        )
        .unwrap();
        fs.write(
            Path::new("/h/.claude/projects/E--proj/b.jsonl"),
            &transcript_line("E:\\Projects\\two"),
        )
        .unwrap();
        // Both candidate paths still exist on disk
        fs.write(Path::new("E:\\Projects\\one\\.keep"), b"x")
            .unwrap();
        fs.write(Path::new("E:\\Projects\\two\\.keep"), b"x")
            .unwrap();
        // Attempting to move "one", which is one of the ambiguous candidates
        let mv = Move {
            src_abs: "E:\\Projects\\one".into(),
            dst_abs: "E:\\Projects\\three".into(),
        };
        let err = build_plan(&fs, Path::new("/h"), &mv, &opts()).unwrap_err();
        assert!(
            matches!(err, crate::error::CpmError::Ambiguous(_)),
            "expected Ambiguous, got {err:?}"
        );
    }

    #[test]
    fn render_plan_locks_format() {
        let fs = MemoryFileSystem::new();
        // Seed a claude.json with a source key so the plan emits a json key change.
        let json = r#"{"projects":{"E:\\Projects\\A":{}}}"#;
        fs.write(Path::new("/h/.claude.json"), json.as_bytes())
            .unwrap();
        let mv = Move {
            src_abs: "E:\\Projects\\A".into(),
            dst_abs: "E:\\Projects\\C".into(),
        };
        let plan = build_plan(&fs, Path::new("/h"), &mv, &opts()).unwrap();
        let got = render_plan(&plan);
        // Expected output captured from actual output on first run, then pasted back.
        // Uses assert_eq! instead of insta to avoid CRLF/acceptance tooling issues on Windows.
        // On Windows, Path::display() uses backslashes, so /h\.claude.json not /h/.claude.json.
        let expected = "Move E:\\Projects\\A -> E:\\Projects\\C\n  json key   /h\\.claude.json \"E:\\Projects\\A\": -> \"E:\\Projects\\C\":\n  move tree  E:/Projects/A -> E:/Projects/C\n";
        assert_eq!(got, expected);
    }
}
