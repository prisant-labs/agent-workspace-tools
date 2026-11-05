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

    // Build the plan before anything is written. A plan refusal (exit 2) then leaves `--to`
    // untouched, which is what "nothing was written" promises. The plan is applied last.
    let plan = if opts.reassociate {
        let mv = Move {
            src_abs: from.to_string(),
            dst_abs: to.to_string(),
        };
        let plan_opts = PlanOpts {
            force: false,
            move_folder: false,
        };
        Some(build_plan(fs, home, &mv, &plan_opts)?)
    } else {
        None
    };

    if opts.export {
        // Export copies transcripts. When they have expired there is nothing to copy. With
        // re-association on, that is a no-op and the re-association half still runs. An
        // export-only run has nothing else to do, so it refuses instead of reporting an
        // export path over an empty folder.
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
            let rep = archive_project(fs, home, from, &aopts)?;
            // The export is the safety copy taken before the re-association rewrites state.
            // `archive` tolerates unreadable files so a hook never fails; here that tolerance
            // would let the rewrite proceed behind a partial copy, so refuse instead.
            if rep.failed > 0 {
                let shown: Vec<String> = rep
                    .failures
                    .iter()
                    .take(5)
                    .map(|f| format!("{} ({})", f.path.display(), f.reason))
                    .collect();
                let advice = if opts.reassociate {
                    "Close the sessions that hold them and re-run, or pass --no-export."
                } else {
                    "Close the sessions that hold them and re-run."
                };
                let outcome = if opts.reassociate {
                    "so nothing was re-associated"
                } else {
                    "so the export is incomplete"
                };
                return Err(AwtError::Io(std::io::Error::other(format!(
                    "export could not copy {} file(s), {outcome}: {}. {advice}",
                    rep.failed,
                    shown.join("; ")
                ))));
            }
        } else if !opts.reassociate {
            return Err(AwtError::Locked(format!(
                "no readable transcript resolved project '{from}', so nothing was exported. \
                 The transcripts may have expired or be unreadable."
            )));
        }
    }
    if let Some(plan) = plan {
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

    /// A plan refusal (exit 2) must happen before the export, so nothing lands in `--to`.
    #[test]
    fn plan_refusal_writes_nothing_into_the_export_folder() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--A/s.jsonl"),
            b"{\"cwd\":\"E:\\\\A\"}\n",
        )
        .unwrap();
        // The destination already has a claude.json key, which the plan refuses.
        fs.write(
            Path::new("/h/.claude.json"),
            br#"{"projects":{"E:/A":{},"E:/B":{}}}"#,
        )
        .unwrap();
        fs.write(Path::new("E:/B/keep.txt"), b"x").unwrap();
        let opts = AssociateOpts {
            reassociate: true,
            export: true,
            export_subdir: ".claude-sessions".into(),
            run_id: "T".into(),
        };
        let err = associate(&fs, Path::new("/h"), "E:\\A", "E:\\B", &opts).unwrap_err();
        assert!(matches!(err, AwtError::DestinationExists(_)), "{err:?}");
        assert!(
            !fs.exists(Path::new("E:/B/.claude-sessions")),
            "a refused plan must not leave an export behind"
        );
    }

    /// Export-only where no readable transcript resolves the project must refuse, not print
    /// an export path over an empty folder.
    #[test]
    fn export_only_without_a_resolving_transcript_refuses() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude.json"),
            br#"{"projects":{"E:/gone/proj":{}}}"#,
        )
        .unwrap();
        fs.write(Path::new("E:/new/proj/keep.txt"), b"x").unwrap();
        let opts = AssociateOpts {
            reassociate: false,
            export: true,
            export_subdir: ".claude-sessions".into(),
            run_id: "T".into(),
        };
        let err = associate(
            &fs,
            Path::new("/h"),
            "E:\\gone\\proj",
            "E:\\new\\proj",
            &opts,
        )
        .unwrap_err();
        match err {
            AwtError::Locked(m) => assert!(
                m.contains("no readable transcript") && m.contains("nothing was exported"),
                "{m}"
            ),
            other => panic!("expected Locked, got {other:?}"),
        }
        assert!(!fs.exists(Path::new("E:/new/proj/.claude-sessions")));
    }

    /// `--no-export` is only useful advice when re-association is on. With
    /// `--no-reassociate`, passing it leaves nothing to do and exits 2.
    #[test]
    fn export_failure_suggests_no_export_only_when_reassociating() {
        for (reassociate, advises) in [(true, true), (false, false)] {
            let fs = crate::fs::FaultyFileSystem::new(&["tasks/s/.lock"], &[]);
            fs.write(
                Path::new("/h/.claude/projects/E--A/s.jsonl"),
                b"{\"cwd\":\"E:\\\\A\"}\n",
            )
            .unwrap();
            fs.write(Path::new("/h/.claude/tasks/s/.lock"), b"")
                .unwrap();
            fs.write(Path::new("E:/B/keep.txt"), b"x").unwrap();
            let opts = AssociateOpts {
                reassociate,
                export: true,
                export_subdir: ".claude-sessions".into(),
                run_id: "T".into(),
            };
            let err = associate(&fs, Path::new("/h"), "E:\\A", "E:\\B", &opts).unwrap_err();
            let m = err.to_string();
            assert_eq!(m.contains("--no-export"), advises, "{m}");
            assert!(m.contains("export could not copy"), "{m}");
        }
    }

    /// AR-09: archive tolerates unreadable files, but an export that failed to copy a file
    /// must stop the re-association, because the export is the safety copy taken before it.
    #[test]
    fn export_with_a_failed_file_refuses_before_reassociating() {
        let fs = crate::fs::FaultyFileSystem::new(&["tasks/s/.lock"], &[]);
        fs.write(
            Path::new("/h/.claude/projects/E--A/s.jsonl"),
            b"{\"cwd\":\"E:\\\\A\"}\n",
        )
        .unwrap();
        fs.write(Path::new("/h/.claude/tasks/s/.lock"), b"")
            .unwrap();
        fs.write(Path::new("E:/B/keep.txt"), b"x").unwrap();
        let opts = AssociateOpts {
            reassociate: true,
            export: true,
            export_subdir: ".claude-sessions".into(),
            run_id: "T".into(),
        };
        let err = associate(&fs, Path::new("/h"), "E:\\A", "E:\\B", &opts).unwrap_err();
        assert!(matches!(err, AwtError::Io(_)), "{err:?}");
        assert!(
            fs.exists(Path::new("/h/.claude/projects/E--A/s.jsonl")),
            "records must be untouched when the export failed"
        );
        assert!(!fs.exists(Path::new("/h/.claude/projects/E--B")));
    }
}
