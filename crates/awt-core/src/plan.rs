use crate::error::{AwtError, Result};
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

impl Plan {
    /// Serialize the plan to a JSON object. Pure function: no IO.
    ///
    /// This is the machine-readable contract behind `awt plan --json`, and it is what the
    /// v2 GUI is required to render: ROADMAP AC-25 states the parity rule as
    /// `GUI plan model == awt plan --json`, so both front ends consume this identical
    /// object rather than each computing its own view. Until this existed the parity test
    /// could not be written at all (AR-03).
    ///
    /// Every entry carries a `kind` discriminant so a consumer can switch on it without
    /// parsing prose, and `rules` exposes the literal find/replace pairs so a UI can offer
    /// a byte-level drill-down.
    ///
    /// Shape:
    /// ```json
    /// {
    ///   "src": "...", "dst": "...", "home": "...",
    ///   "changes": [{"kind": "rewrite_file", "path": "...", "expected": 37, "rules": [...]}],
    ///   "warnings": [], "nested": [],
    ///   "totals": {"changes": 5, "edits": 2082}
    /// }
    /// ```
    pub fn to_json(&self) -> serde_json::Value {
        let pathstr = |p: &std::path::Path| p.to_string_lossy().into_owned();

        let changes: Vec<serde_json::Value> = self
            .changes
            .iter()
            .map(|c| match c {
                Change::RenameDir { from, to } => serde_json::json!({
                    "kind": "rename_dir", "from": pathstr(from), "to": pathstr(to),
                }),
                Change::MergeDir { from, to } => serde_json::json!({
                    "kind": "merge_dir", "from": pathstr(from), "to": pathstr(to),
                }),
                Change::MoveTree { from, to } => serde_json::json!({
                    "kind": "move_tree", "from": pathstr(from), "to": pathstr(to),
                }),
                Change::RewriteFile {
                    path,
                    rules,
                    expected,
                } => serde_json::json!({
                    "kind": "rewrite_file",
                    "path": pathstr(path),
                    "expected": expected,
                    "rules": rules.iter().map(|r| serde_json::json!({
                        "find": r.find, "replace": r.replace,
                    })).collect::<Vec<_>>(),
                }),
                Change::RenameJsonKey {
                    path,
                    from,
                    to,
                    expected,
                } => serde_json::json!({
                    "kind": "rename_json_key",
                    "path": pathstr(path), "from": from, "to": to, "expected": expected,
                }),
                Change::RewriteJsonArrayValue {
                    path,
                    from,
                    to,
                    expected,
                } => serde_json::json!({
                    "kind": "rewrite_json_array_value",
                    "path": pathstr(path), "from": from, "to": to, "expected": expected,
                }),
            })
            .collect();

        // `edits` counts planned byte replacements, which is the number a human reasons
        // about ("2,082 changes"); `changes` counts plan entries, which is the number a
        // progress bar steps through. They are different and both are wanted.
        let edits: usize = self
            .changes
            .iter()
            .map(|c| match c {
                Change::RewriteFile { expected, .. }
                | Change::RenameJsonKey { expected, .. }
                | Change::RewriteJsonArrayValue { expected, .. } => *expected,
                _ => 0,
            })
            .sum();

        serde_json::json!({
            "src": self.mv.src_abs,
            "dst": self.mv.dst_abs,
            "home": pathstr(&self.home),
            "changes": changes,
            "warnings": self.warnings,
            "nested": self.nested,
            "totals": { "changes": self.changes.len(), "edits": edits },
        })
    }
}

pub fn build_plan(fs: &dyn FileSystem, home: &Path, mv: &Move, opts: &PlanOpts) -> Result<Plan> {
    // Guard: cross-volume moves are not supported in v1.0 (spec AC-2, deferred to v1.x).
    // Checked first so the refusal is immediate and unambiguous.
    if opts.move_folder && !same_volume(&mv.src_abs, &mv.dst_abs) {
        return Err(AwtError::CrossVolume(format!(
            "cross-volume move refused: source {} is on a different volume from \
             destination {}. Cross-volume moves are not supported in v1.0 \
             (spec AC-2, deferred). Move within the same volume instead.",
            mv.src_abs, mv.dst_abs
        )));
    }
    // Guard: destination folder exists (only relevant when we will move the folder there)
    if opts.move_folder && fs.exists(Path::new(&mv.dst_abs.replace('\\', "/"))) {
        return Err(AwtError::DestinationExists(mv.dst_abs.clone()));
    }
    // Guard: worktree source (.git is a file, not a dir)
    let git = format!("{}/.git", mv.src_abs.replace('\\', "/"));
    if fs.is_file(Path::new(&git)) && !opts.force {
        return Err(AwtError::WorktreeSource(mv.src_abs.clone()));
    }
    // Guard: live IDE lock files signal a running CLI that may be editing the project.
    // Without --force this is a hard refusal; with --force we warn and continue.
    let live_locks = detect_live(fs, home);
    let mut warnings = Vec::new();
    if !live_locks.is_empty() {
        if !opts.force {
            return Err(AwtError::Locked(format!(
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
            return Err(AwtError::Ambiguous(format!(
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
                        return Err(AwtError::DestinationExists(format!(
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

    // AR-04: the same literal can be reached by more than one hit. `.claude.json` may hold
    // one path under two `githubRepoPaths` slugs, and each hit plans its own change. But a
    // change's count check scans the WHOLE file, so N separate changes of `expected: 1`
    // each observe N live occurrences and the first one refuses with "expected 1, live N".
    // Coalescing identical splices into a single change with the true total is what makes
    // the count check agree with the file. Found on real data where
    // prisant-labs/agent-workspace-tools and prisant-labs/claude-project-mover both pointed
    // at the same folder.
    let mut changes = coalesce_text_splices(changes);

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

/// Merge text-splice changes that target the same file with the same `from` and `to`
/// literal, summing their expected counts.
///
/// These changes are applied by counting occurrences of `from` across the entire file and
/// refusing when the live count differs from `expected`. Two changes that share a literal
/// therefore each see the other's occurrence, so leaving them separate guarantees a
/// spurious refusal. Order is otherwise preserved: the merged change keeps the position of
/// the first occurrence, because apply ordering is load-bearing elsewhere.
///
/// Changes with different literals never merge, so a genuine count mismatch (the literal
/// also appearing somewhere no hit claimed) still refuses, which is the intended behavior.
fn coalesce_text_splices(changes: Vec<Change>) -> Vec<Change> {
    use std::collections::HashMap;
    use std::path::PathBuf;

    // The u8 discriminant keeps a key rename and an array-value rewrite distinct even if
    // they somehow share a literal, since they are different operations on apply.
    let mut seen: HashMap<(u8, PathBuf, String, String), usize> = HashMap::new();
    let mut out: Vec<Change> = Vec::with_capacity(changes.len());

    for change in changes {
        let key = match &change {
            Change::RenameJsonKey { path, from, to, .. } => {
                Some((0u8, path.clone(), from.clone(), to.clone()))
            }
            Change::RewriteJsonArrayValue { path, from, to, .. } => {
                Some((1u8, path.clone(), from.clone(), to.clone()))
            }
            _ => None,
        };
        let Some(key) = key else {
            out.push(change);
            continue;
        };
        let count = match &change {
            Change::RenameJsonKey { expected, .. }
            | Change::RewriteJsonArrayValue { expected, .. } => *expected,
            _ => 0,
        };
        match seen.get(&key) {
            Some(&i) => match &mut out[i] {
                Change::RenameJsonKey { expected, .. }
                | Change::RewriteJsonArrayValue { expected, .. } => *expected += count,
                _ => unreachable!("index recorded for a non-splice change"),
            },
            None => {
                seen.insert(key, out.len());
                out.push(change);
            }
        }
    }
    out
}

fn dest_key_exists(ctx: &Ctx, mv: &Move) -> Result<bool> {
    let p = ctx.home.join(".claude.json");
    if !ctx.fs.exists(&p) {
        return Ok(false);
    }
    let v: serde_json::Value = serde_json::from_slice(&ctx.fs.read(&p)?)
        .map_err(|e| AwtError::UnrecognizedFormat(e.to_string()))?;
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
        assert!(matches!(err, crate::error::AwtError::DestinationExists(_)));
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
        assert!(matches!(err, crate::error::AwtError::WorktreeSource(_)));
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
            matches!(err, crate::error::AwtError::CrossVolume(_)),
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
            matches!(err, crate::error::AwtError::Locked(_)),
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
            matches!(err, crate::error::AwtError::Ambiguous(_)),
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
        // Uses assert_eq! instead of insta to avoid CRLF/acceptance tooling issues on Windows.
        // On Windows, Path::display() uses backslashes, so /h\.claude.json not /h/.claude.json.
        //
        // The json key line shows the path DOUBLE-escaped (`E:\\Projects\\A`) because that is
        // the literal byte sequence the rewrite anchors on, and claude.json stores Windows
        // paths JSON-escaped. Until the AR-01 fix this golden expected the single-escaped form,
        // which read more naturally and was precisely the problem: the rendered plan disagreed
        // with the bytes the splice would actually search for, so the plan looked correct while
        // apply could not possibly succeed. Rendering the true anchor keeps the dry run a
        // faithful preview of the write. Do not "clean this up" back to single escaping.
        let expected = "Move E:\\Projects\\A -> E:\\Projects\\C\n  json key   /h\\.claude.json \"E:\\\\Projects\\\\A\": -> \"E:\\\\Projects\\\\C\":\n  move tree  E:/Projects/A -> E:/Projects/C\n";
        assert_eq!(got, expected);
    }
}
