use crate::apply::{apply_verified, ApplyOpts};
use crate::archive::{archive_project, ArchiveOpts};
use crate::error::{AwtError, Result};
use crate::fs::FileSystem;
use crate::model::Move;
use crate::paths::normalize_path;
use crate::plan::{build_plan, PlanOpts};
use std::path::{Path, PathBuf};

pub struct AssociateOpts {
    pub reassociate: bool,
    pub export: bool,
    pub export_subdir: String,
    pub run_id: String,
}

pub fn associate(
    fs: &dyn FileSystem,
    home: &Path,
    from: &str,
    to: &str,
    opts: &AssociateOpts,
) -> Result<crate::report::Report> {
    if normalize_path(from) == normalize_path(to) {
        return Err(AwtError::Locked(
            "source and destination are the same project".into(),
        ));
    }
    if !opts.reassociate && !opts.export {
        return Err(AwtError::Locked(
            "nothing to do: enable --reassociate or --export".into(),
        ));
    }

    // AR-02: resolve the target across EVERY store, not just the transcript-keyed reverse
    // index. `history.jsonl` never expires while transcripts are auto-deleted after 30
    // days, so a long-dead project routinely has `claude.json` and history state with no
    // surviving transcripts - which is precisely the case this command exists to rescue.
    // Resolving through transcripts alone meant the longer a project had been dead, the
    // more certain `associate` was to refuse it.
    if crate::doctor::scan(fs, home, from)?.hits.is_empty() {
        // AR-08: a guard refusal (exit 2), not UnrecognizedFormat (exit 4). The input was
        // understood; there is simply nothing recorded to act on. Exit 4 is reserved for
        // store bytes the tool cannot parse.
        return Err(AwtError::Locked(format!(
            "no Claude state found for project '{from}'; run 'awt list' to see known projects"
        )));
    }

    if opts.export {
        // Export copies transcripts. When they have expired there is nothing to copy, so
        // this degrades to a no-op rather than aborting a run whose re-association half is
        // perfectly viable.
        let has_transcripts = crate::index::ProjectIndex::build(fs, home)?
            .by_cwd
            .contains_key(&normalize_path(from));
        if has_transcripts {
            let sub = format!("{}/{}", to.replace('\\', "/"), opts.export_subdir);
            let aopts = ArchiveOpts {
                archive_dir: PathBuf::from(sub),
                render: false,
                run_token: opts.run_id.clone(),
            };
            archive_project(fs, home, from, &aopts)?;
        }
    }
    if opts.reassociate {
        let mv = Move {
            src_abs: from.to_string(),
            dst_abs: to.to_string(),
        };
        let plan_opts = PlanOpts {
            force: false,
            move_folder: false,
        };
        let plan = build_plan(fs, home, &mv, &plan_opts)?;
        let aopts = ApplyOpts {
            run_id: opts.run_id.clone(),
            auto_rollback: true,
            force: false,
        };
        return apply_verified(&plan, fs, &std::env::temp_dir(), &aopts);
    }
    Ok(crate::report::Report {
        run_id: opts.run_id.clone(),
        applied: vec![],
        backup_dir: String::new(),
        verify: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{FileSystem, MemoryFileSystem};
    use std::path::Path;

    // --- Finding 5 test: from == to is rejected ---

    #[test]
    fn associate_rejects_from_equal_to() {
        let fs = MemoryFileSystem::new();
        let opts = AssociateOpts {
            reassociate: true,
            export: false,
            export_subdir: ".claude-sessions".into(),
            run_id: "T".into(),
        };
        let err = associate(&fs, Path::new("/h"), "E:\\A", "E:\\A", &opts).unwrap_err();
        assert!(
            matches!(err, AwtError::Locked(_)),
            "expected Locked error for from==to, got {err:?}"
        );
    }

    /// Case and separator variants of the same path are also equal.
    #[test]
    fn associate_rejects_from_equal_to_case_insensitive() {
        let fs = MemoryFileSystem::new();
        let opts = AssociateOpts {
            reassociate: true,
            export: false,
            export_subdir: ".claude-sessions".into(),
            run_id: "T".into(),
        };
        let err = associate(
            &fs,
            Path::new("/h"),
            "E:\\Projects\\A",
            "E:/projects/a",
            &opts,
        )
        .unwrap_err();
        assert!(
            matches!(err, AwtError::Locked(_)),
            "expected Locked error for case-variant from==to, got {err:?}"
        );
    }

    #[test]
    fn export_only_copies_but_does_not_reassociate() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--A/s.jsonl"),
            b"{\"cwd\":\"E:\\\\A\"}\n",
        )
        .unwrap();
        fs.write(Path::new("E:/B/keep.txt"), b"x").unwrap();
        let opts = AssociateOpts {
            reassociate: false,
            export: true,
            export_subdir: ".claude-sessions".into(),
            run_id: "T".into(),
        };
        associate(&fs, Path::new("/h"), "E:\\A", "E:\\B", &opts).unwrap();
        assert!(fs.exists(Path::new("/h/.claude/projects/E--A/s.jsonl"))); // records untouched
        assert!(fs.exists(Path::new("E:/B/.claude-sessions/projects/E--A/s.jsonl")));
        // export copy
    }

    #[test]
    fn export_is_scoped_to_the_from_project_only() {
        // Two projects; export FROM A must NOT copy B (the plan's archive_all would - regression guard).
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--A/s.jsonl"),
            b"{\"cwd\":\"E:\\\\A\"}\n",
        )
        .unwrap();
        fs.write(
            Path::new("/h/.claude/projects/E--Other/o.jsonl"),
            b"{\"cwd\":\"E:\\\\Other\"}\n",
        )
        .unwrap();
        fs.write(Path::new("E:/B/keep.txt"), b"x").unwrap();
        let opts = AssociateOpts {
            reassociate: false,
            export: true,
            export_subdir: ".claude-sessions".into(),
            run_id: "T".into(),
        };
        associate(&fs, Path::new("/h"), "E:\\A", "E:\\B", &opts).unwrap();
        assert!(fs.exists(Path::new("E:/B/.claude-sessions/projects/E--A/s.jsonl")));
        assert!(
            !fs.exists(Path::new("E:/B/.claude-sessions/projects/E--Other/o.jsonl")),
            "export must be scoped to the from project only"
        );
    }
}
