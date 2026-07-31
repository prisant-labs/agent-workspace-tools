mod exit;

use awt_core::apply::{apply_verified, ApplyOpts};
use awt_core::associate::{associate, AssociateOpts};
use awt_core::backup::Manifest;
use awt_core::doctor::{doctor, scan};
use awt_core::error::AwtError;
use awt_core::fs::FileSystem;
use awt_core::fs::RealFileSystem;
use awt_core::model::{Move, Scope};
use awt_core::plan::{build_plan, render_plan, Collision, PlanOpts};
use awt_core::rollback::{rollback, verify_rollback};
use awt_core::verify::verify;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "awt",
    version,
    about = "Move a project folder without orphaning its Claude state"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
    /// Home directory holding .claude and .claude.json
    #[arg(long, global = true)]
    home: Option<PathBuf>,
    /// Emit machine-readable JSON instead of text
    #[arg(long, global = true)]
    json: bool,
    /// Backup root directory (default: system temp dir)
    #[arg(long, global = true)]
    backup_root: Option<PathBuf>,
    /// Allow overwriting a destination that already exists
    #[arg(long, global = true)]
    force: bool,
    /// Also move nested projects under src
    #[arg(long, global = true)]
    recursive: bool,
    /// Disable automatic rollback on apply failure
    #[arg(long, global = true)]
    no_auto_rollback: bool,
    /// Collision strategy: refuse (default), keep-dest, keep-src
    #[arg(long, global = true)]
    on_collision: Option<String>,
    /// Rewrite scope: minimal, standard (default), full
    #[arg(long, global = true)]
    scope: Option<String>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Report path-keyed state that references folders that no longer exist
    Doctor,
    /// Enumerate every project Claude has state for, with session counts and health
    List {
        /// Also write a self-contained HTML inventory to this path (the table is still printed)
        #[arg(long)]
        html: Option<PathBuf>,
    },
    /// List all state that references a project's absolute path
    Scan {
        /// Absolute path of the project to scan
        #[arg(long)]
        src: String,
    },
    /// Dry-run: print the changes that apply would make
    Plan {
        /// Absolute path of the project in its current location
        #[arg(long)]
        src: String,
        /// Absolute path of the intended new location
        #[arg(long)]
        dst: String,
    },
    /// Move src to dst and rewrite all Claude state
    Apply {
        /// Absolute path of the project in its current location
        #[arg(long)]
        src: String,
        /// Absolute path of the intended new location
        #[arg(long)]
        dst: String,
    },
    /// Check that Claude state correctly references dst after a move
    Verify {
        /// Original (old) absolute path
        #[arg(long)]
        src: String,
        /// New absolute path
        #[arg(long)]
        dst: String,
    },
    /// Restore pre-move state from a backup manifest
    Rollback {
        /// Path to the manifest.json written by a previous apply run
        #[arg(long)]
        report: PathBuf,
    },
    /// Repair damaged Claude state. Dry run by default; pass --apply to write
    Repair {
        /// Repair history entries whose drive letter was corrupted (values starting with "::")
        #[arg(long)]
        drive_letter: bool,
        /// Perform the repair. Without this the command writes nothing
        #[arg(long)]
        apply: bool,
    },
    /// Re-associate session history from one project path to another, and/or export a portable copy
    Associate {
        /// Source project absolute path (may no longer exist on disk)
        #[arg(long)]
        from: String,
        /// Destination project absolute path
        #[arg(long)]
        to: String,
        /// Subdirectory under --to for the exported archive (default: .claude-sessions)
        #[arg(long)]
        export_subdir: Option<String>,
        /// Disable the re-association step (export only)
        #[arg(long)]
        no_reassociate: bool,
        /// Disable the export step (reassociate only)
        #[arg(long)]
        no_export: bool,
    },
    /// Archive Claude session state before the 30-day auto-delete window
    Archive {
        /// Destination directory for archived data
        #[arg(long)]
        archive_dir: Option<PathBuf>,
        /// Archive a single session transcript (path to the .jsonl file)
        #[arg(long)]
        session: Option<PathBuf>,
        /// Install the awt SessionEnd hook in ~/.claude/settings.json (requires --archive-dir)
        #[arg(long)]
        install_hook: bool,
        /// Remove the awt SessionEnd hook from ~/.claude/settings.json
        #[arg(long)]
        uninstall_hook: bool,
        /// Set cleanupPeriodDays in ~/.claude/settings.json
        #[arg(long)]
        set_retention: Option<u32>,
        /// Allow setting cleanupPeriodDays to 0 (read issues #23710 and #62272 first)
        #[arg(long)]
        force_zero: bool,
        /// Request HTML rendering of archived sessions (reserved for future use)
        #[arg(long)]
        render: bool,
        /// Read SessionEnd hook context from stdin as JSON and archive the transcript_path field
        #[arg(long)]
        hook_stdin: bool,
    },
}

fn home_of(cli: &Cli) -> Option<PathBuf> {
    cli.home.clone().or_else(|| {
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
    })
}

fn print_doctor(rep: &awt_core::doctor::DoctorReport, json: bool) {
    let render = |s: &awt_core::model::Stale| serde_json::json!({ "store": s.store, "reference": s.reference, "location": s.location });
    if json {
        println!(
            "{}",
            serde_json::json!({
                "stale": rep.stale.iter().map(render).collect::<Vec<_>>(),
                "report_only": rep.report_only.iter().map(render).collect::<Vec<_>>(),
                "unresolved": rep.unresolved.iter()
                    .map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
                "warnings": rep.warnings,
            })
        );
        return;
    }
    println!("Stale references: {}", rep.stale.len());
    for s in &rep.stale {
        println!("  [{}] {} @ {}", s.store, s.reference, s.location);
    }
    // Report-only findings live in regions no adapter owns, so they are surfaced but never
    // rewritten. Kept under their own heading so the "Stale references" count above stays a
    // count of what the tool can actually fix, not a mix of fixable state and mere mentions.
    println!("Report only (never rewritten): {}", rep.report_only.len());
    for s in &rep.report_only {
        println!("  [{}] {} @ {}", s.store, s.reference, s.location);
    }
    // Unresolvable dirs are not a fault to fix - they are dirs whose transcripts never
    // recorded a cwd, so there is nothing to resolve them against. Reported, never guessed.
    println!("Unresolvable project dirs: {}", rep.unresolved.len());
    for p in &rep.unresolved {
        println!("  {}", p.display());
    }
    // Shapes an adapter recognized and deliberately declined to act on. Distinct from stale
    // (a reference that is dead) and from report-only (a region no adapter owns): these are
    // inside an owned region and were skipped on purpose. Printed only when non-empty so the
    // channel keeps its signal.
    if !rep.warnings.is_empty() {
        println!("Warnings: {}", rep.warnings.len());
        for w in &rep.warnings {
            println!("  {w}");
        }
    }
}

fn print_scan(rep: &awt_core::doctor::ScanReport, src: &str, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::json!({
                "src": src,
                "hits": rep.hits.iter().map(|h| serde_json::json!({
                    "store": h.store, "detail": h.detail,
                    "target": h.target.to_string_lossy()
                })).collect::<Vec<_>>(),
            })
        );
        return;
    }
    println!("Hits for {src}: {}", rep.hits.len());
    for h in &rep.hits {
        println!("  [{}] {} -> {}", h.store, h.detail, h.target.display());
    }
}

fn plan_opts(cli: &Cli) -> PlanOpts {
    let scope = match cli.scope.as_deref() {
        Some("minimal") => Scope::Minimal,
        Some("full") => Scope::Full,
        _ => Scope::Standard,
    };
    let on_collision = match cli.on_collision.as_deref() {
        Some("keep-dest") => Collision::KeepDest,
        Some("keep-src") => Collision::KeepSrc,
        _ => Collision::Refuse,
    };
    PlanOpts {
        recursive: cli.recursive,
        on_collision,
        force: cli.force,
        move_folder: true,
        scope,
    }
}

fn pick_run_id() -> String {
    format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    )
}

fn health_to_str(h: &awt_core::list::Health) -> &'static str {
    match h {
        awt_core::list::Health::Ok => "ok",
        awt_core::list::Health::Stale => "stale",
        awt_core::list::Health::Unresolved => "unresolved",
    }
}

/// Warn on stderr (non-fatal) if the chosen archive directory appears to live
/// under a known cloud-sync root. Uses path-segment-boundary matching so that
/// a folder named "OneDriveFoo" does not trigger the warning for "OneDrive".
fn warn_if_cloud_synced(dir: &std::path::Path) {
    let dir_norm = dir.to_string_lossy().replace('\\', "/").to_lowercase();
    let vars = [
        "OneDrive",
        "OneDriveConsumer",
        "OneDriveCommercial",
        "Dropbox",
    ];
    for var in &vars {
        if let Ok(val) = std::env::var(var) {
            let root = val.replace('\\', "/").to_lowercase();
            if dir_norm == root || dir_norm.starts_with(&format!("{root}/")) {
                eprintln!(
                    "warning: --archive-dir appears to be under a cloud-sync root (${var}); \
                     cloud-synced archives may upload sensitive session data"
                );
                return;
            }
        }
    }
}

fn run(cli: &Cli, fs: &RealFileSystem, home: &std::path::Path) -> awt_core::error::Result<()> {
    match &cli.cmd {
        Cmd::Doctor => {
            let r = doctor(fs, home)?;
            print_doctor(&r, cli.json);
            Ok(())
        }
        Cmd::List { html } => {
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let recs = awt_core::list::list(fs, home, now_secs)?;
            if cli.json {
                let arr: Vec<_> = recs
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "cwd": r.cwd,
                            "encoded_dir": r.encoded_dir,
                            "sessions": r.sessions,
                            "bytes": r.bytes,
                            "oldest_days": r.oldest_days,
                            "newest_days": r.newest_days,
                            "json_keys": r.json_keys,
                            "github_paths": r.github_paths,
                            "history_lines": r.history_lines,
                            "plugin_dirs": r.plugin_dirs,
                            "todos": r.footprint.todos,
                            "file_history": r.footprint.file_history,
                            "health": health_to_str(&r.health),
                        })
                    })
                    .collect();
                println!("{}", serde_json::json!(arr));
            } else {
                print!("{}", awt_core::list::render_table(&recs));
            }
            if let Some(path) = html {
                let content = awt_core::list::render_html(&recs);
                fs.write(path, content.as_bytes())?;
            }
            Ok(())
        }
        Cmd::Scan { src } => {
            let r = scan(fs, home, src)?;
            print_scan(&r, src, cli.json);
            Ok(())
        }
        Cmd::Plan { src, dst } => {
            let mv = Move {
                src_abs: src.clone(),
                dst_abs: dst.clone(),
            };
            let plan = build_plan(fs, home, &mv, &plan_opts(cli))?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&plan.to_json()).unwrap());
            } else {
                print!("{}", render_plan(&plan));
            }
            Ok(())
        }
        Cmd::Apply { src, dst } => {
            let mv = Move {
                src_abs: src.clone(),
                dst_abs: dst.clone(),
            };
            let plan = build_plan(fs, home, &mv, &plan_opts(cli))?;
            let backup_root = cli.backup_root.clone().unwrap_or_else(std::env::temp_dir);
            let run_id = pick_run_id();
            let opts = ApplyOpts {
                run_id,
                auto_rollback: !cli.no_auto_rollback,
                force: cli.force,
            };
            let r = apply_verified(&plan, fs, &backup_root, &opts)?;
            let doc = r.to_json();
            let doc_str = serde_json::to_string_pretty(&doc).unwrap();
            let report_path = backup_root.join(&r.backup_dir).join("report.json");
            fs.write(&report_path, doc_str.as_bytes())?;
            if cli.json {
                println!("{}", doc_str);
            } else {
                println!(
                    "applied {} changes; backup {}",
                    r.applied.len(),
                    r.backup_dir
                );
                println!("report: {}", report_path.display());
            }
            Ok(())
        }
        Cmd::Verify { src, dst } => {
            let mv = Move {
                src_abs: src.clone(),
                dst_abs: dst.clone(),
            };
            // Standalone verify has neither a manifest nor the original plan, so it runs the
            // store-level postconditions only; the plan-derived splice checks and folder
            // postconditions run in apply_verified, which has both.
            let results = verify(fs, home, &mv, None, None)?;
            let failed = results.iter().filter(|r| !r.ok).count();
            if cli.json {
                let checks: Vec<_> = results
                    .iter()
                    .map(
                        |r| serde_json::json!({ "check": r.check, "ok": r.ok, "detail": r.detail }),
                    )
                    .collect();
                let doc = serde_json::json!({
                    "src": src,
                    "dst": dst,
                    "checks": checks,
                    "failed": failed,
                    "ok": failed == 0,
                });
                println!("{}", serde_json::to_string_pretty(&doc).unwrap());
            } else {
                for r in &results {
                    println!(
                        "  [{}] {}: {}",
                        if r.ok { "ok" } else { "FAIL" },
                        r.check,
                        r.detail
                    );
                }
            }
            // The exit-code contract is independent of the output format: a failed
            // verification is exit 3 whether it was reported as prose or as JSON.
            if failed > 0 {
                return Err(AwtError::VerifyFailed(format!("{failed} failed")));
            }
            Ok(())
        }
        Cmd::Repair {
            drive_letter,
            apply,
        } => {
            // Each repair is a separately named and separately guarded transformation (D8), so
            // one has to be selected. Defaulting to "repair everything you can find" is exactly
            // the unbounded inference this tool exists not to do.
            if !*drive_letter {
                return Err(AwtError::Locked(
                    "nothing to do: select a repair, e.g. --drive-letter".into(),
                ));
            }
            let plan = awt_core::repair::build_repair_plan(fs, home)?;
            if !*apply {
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&plan.to_json()).unwrap());
                } else {
                    print!("{}", awt_core::repair::render_repair(&plan));
                    if !plan.repairs.is_empty() {
                        println!("\nDry run: nothing was written. Re-run with --apply to repair.");
                    }
                }
                return Ok(());
            }
            let backup_root = cli.backup_root.clone().unwrap_or_else(std::env::temp_dir);
            let run_id = pick_run_id();
            let r = awt_core::repair::apply_repair(&plan, fs, &backup_root, &run_id)?;
            if r.backup_dir.is_empty() {
                // Idempotent no-op: a repaired file has nothing left to repair.
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&plan.to_json()).unwrap());
                } else {
                    print!("{}", awt_core::repair::render_repair(&plan));
                }
                return Ok(());
            }
            let doc = r.to_json();
            let doc_str = serde_json::to_string_pretty(&doc).unwrap();
            let report_path = backup_root.join(&r.backup_dir).join("report.json");
            fs.write(&report_path, doc_str.as_bytes())?;
            if cli.json {
                println!("{}", doc_str);
            } else {
                println!(
                    "repaired {} history line(s) across {} value(s); backup {}",
                    plan.total_lines(),
                    plan.repairs.len(),
                    r.backup_dir
                );
                println!("report: {}", report_path.display());
                if !plan.unrepairable.is_empty() || !plan.ambiguous.is_empty() {
                    println!(
                        "declined: {} unrepairable, {} ambiguous (re-run without --apply for detail)",
                        plan.unrepairable.len(),
                        plan.ambiguous.len()
                    );
                }
            }
            Ok(())
        }
        Cmd::Rollback { report } => {
            rollback(report, fs)?;
            let results = verify_rollback(report, fs)?;
            // Load the manifest to get run_id for the report (manifest still present after rollback).
            let run_id = Manifest::load(fs, report)
                .map(|m| m.run_id)
                .unwrap_or_default();

            let failed = results.iter().filter(|r| !r.ok).count();
            let total = results.len();
            for r in &results {
                println!(
                    "  [{}] {}: {}",
                    if r.ok { "ok" } else { "FAIL" },
                    r.check,
                    r.detail
                );
            }
            let summary = if failed == 0 {
                format!("revert verified: {total}/{total} checks passed")
            } else {
                format!("FAILED: {failed}/{total} checks failed")
            };
            println!("{summary}");

            // Build the rollback report (serde_json::json! - no serde-derive).
            let report_json = serde_json::json!({
                "run_id": run_id,
                "checks": results.iter().map(|r| serde_json::json!({
                    "check": r.check,
                    "ok": r.ok,
                    "detail": r.detail,
                })).collect::<Vec<_>>(),
            });
            let report_str = serde_json::to_string_pretty(&report_json).unwrap();

            // Always write rollback-report.json next to the manifest.
            let rollback_report_path = report.with_file_name("rollback-report.json");
            fs.write(&rollback_report_path, report_str.as_bytes())?;

            if cli.json {
                println!("{report_str}");
            }

            if failed > 0 {
                return Err(AwtError::VerifyFailed(format!("{failed} checks failed")));
            }
            Ok(())
        }
        Cmd::Associate {
            from,
            to,
            export_subdir,
            no_reassociate,
            no_export,
        } => {
            let reassociate = !no_reassociate;
            let export = !no_export;
            if !reassociate && !export {
                return Err(AwtError::Locked(
                    "--no-reassociate and --no-export together leave nothing to do".into(),
                ));
            }
            let on_collision = match cli.on_collision.as_deref() {
                Some("keep-dest") => Collision::KeepDest,
                Some("keep-src") => Collision::KeepSrc,
                _ => Collision::Refuse,
            };
            let aopts = AssociateOpts {
                reassociate,
                export,
                export_subdir: export_subdir
                    .clone()
                    .unwrap_or_else(|| ".claude-sessions".into()),
                run_id: pick_run_id(),
                on_collision,
            };
            let r = associate(fs, home, from, to, &aopts)?;
            let export_loc = format!("{}/{}", to.replace('\\', "/"), aopts.export_subdir);
            for line in associate_result_lines(&r, aopts.export, &export_loc) {
                println!("{line}");
            }
            Ok(())
        }
        Cmd::Archive {
            archive_dir,
            session,
            install_hook,
            uninstall_hook,
            set_retention: days,
            force_zero,
            render,
            hook_stdin,
        } => {
            if *install_hook {
                let dir = archive_dir.clone().ok_or_else(|| {
                    AwtError::Locked("--archive-dir is required with --install-hook".into())
                })?;
                let exe = std::env::current_exe().map_err(awt_core::error::AwtError::Io)?;
                awt_core::settings::install_session_end_hook(fs, home, &exe, &dir)?;
                println!("awt SessionEnd hook installed in ~/.claude/settings.json");
                return Ok(());
            }
            if *uninstall_hook {
                awt_core::settings::uninstall_session_end_hook(fs, home)?;
                println!("awt SessionEnd hook removed from ~/.claude/settings.json");
                return Ok(());
            }
            if let Some(d) = days {
                awt_core::settings::set_retention(fs, home, *d, *force_zero)?;
                println!("cleanupPeriodDays set to {d}");
                eprintln!(
                    "note: issues #23710 and #62272 warn against setting this to 0; \
                     minimum safe value is 1"
                );
                return Ok(());
            }
            if *hook_stdin {
                let dir = archive_dir.clone().ok_or_else(|| {
                    AwtError::Locked("--archive-dir is required with --hook-stdin".into())
                })?;
                let mut input = String::new();
                std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)
                    .map_err(awt_core::error::AwtError::Io)?;
                let json: serde_json::Value = serde_json::from_str(&input).map_err(|e| {
                    AwtError::UnrecognizedFormat(format!("hook stdin is not valid JSON: {e}"))
                })?;
                let transcript_path = json["transcript_path"].as_str().ok_or_else(|| {
                    AwtError::UnrecognizedFormat(
                        "hook stdin JSON missing 'transcript_path' field".into(),
                    )
                })?;
                check_transcript_confinement(transcript_path, home)?;
                warn_if_cloud_synced(&dir);
                let opts = awt_core::archive::ArchiveOpts {
                    archive_dir: dir,
                    render: *render,
                    run_token: pick_run_id(),
                };
                let r = awt_core::archive::archive_session(
                    fs,
                    home,
                    std::path::Path::new(transcript_path),
                    &opts,
                )?;
                println!("archived: {} copied, {} skipped", r.copied, r.skipped);
                return Ok(());
            }
            if let Some(transcript) = session.as_ref() {
                let dir = archive_dir.clone().ok_or_else(|| {
                    AwtError::Locked("--archive-dir is required with --session".into())
                })?;
                warn_if_cloud_synced(&dir);
                let opts = awt_core::archive::ArchiveOpts {
                    archive_dir: dir,
                    render: *render,
                    run_token: pick_run_id(),
                };
                let r = awt_core::archive::archive_session(fs, home, transcript, &opts)?;
                println!("archived: {} copied, {} skipped", r.copied, r.skipped);
                return Ok(());
            }
            let dir = archive_dir
                .clone()
                .ok_or_else(|| AwtError::Locked("--archive-dir is required for archive".into()))?;
            warn_if_cloud_synced(&dir);
            let opts = awt_core::archive::ArchiveOpts {
                archive_dir: dir,
                render: *render,
                run_token: pick_run_id(),
            };
            let r = awt_core::archive::archive_all(fs, home, &opts)?;
            println!("archived: {} copied, {} skipped", r.copied, r.skipped);
            Ok(())
        }
    }
}

/// Build the output lines for a completed `associate` command.
/// Prints the backup path when a reassociation ran; otherwise prints the export location.
fn associate_result_lines(
    r: &awt_core::report::Report,
    export: bool,
    export_loc: &str,
) -> Vec<String> {
    let mut lines = vec![format!(
        "associate complete: {} changes applied",
        r.applied.len()
    )];
    if !r.backup_dir.is_empty() {
        lines.push(format!("backup {}", r.backup_dir));
    } else if export {
        lines.push(format!("export {export_loc}"));
    }
    lines
}

/// Verify that `transcript_path` is under `home/.claude/projects`.
/// Returns an error if not, so the hook-stdin dispatch fails loudly on unexpected input.
fn check_transcript_confinement(
    transcript_path: &str,
    home: &std::path::Path,
) -> awt_core::error::Result<()> {
    let projects_root = home.join(".claude").join("projects");
    let tp_norm = transcript_path.replace('\\', "/").to_lowercase();
    let pr_norm = projects_root
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase();
    if tp_norm != pr_norm && !tp_norm.starts_with(&format!("{pr_norm}/")) {
        return Err(AwtError::UnrecognizedFormat(format!(
            "transcript_path '{transcript_path}' is outside ~/.claude/projects; \
             hook-stdin input may be malformed"
        )));
    }
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let fs = RealFileSystem;

    let Some(home) = home_of(&cli) else {
        eprintln!("error: no home directory found; pass --home explicitly");
        return ExitCode::from(1);
    };

    match run(&cli, &fs, &home) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(exit::code_for(&e) as u8)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // --- Finding 8 tests: hook-stdin transcript path confinement ---

    #[test]
    fn transcript_confinement_rejects_path_outside_projects() {
        assert!(
            check_transcript_confinement("/etc/passwd", Path::new("/home/user")).is_err(),
            "path outside home must be rejected"
        );
    }

    #[test]
    fn transcript_confinement_rejects_sibling_of_projects() {
        // A path that is a sibling of the projects dir, not inside it.
        assert!(
            check_transcript_confinement(
                "/home/user/.claude/settings.json",
                Path::new("/home/user"),
            )
            .is_err(),
            "path outside projects dir must be rejected"
        );
    }

    #[test]
    fn transcript_confinement_accepts_path_under_projects() {
        assert!(
            check_transcript_confinement(
                "/home/user/.claude/projects/myproj/session.jsonl",
                Path::new("/home/user"),
            )
            .is_ok(),
            "path under home/.claude/projects must be accepted"
        );
    }

    // --- Finding 7 tests: associate result lines ---

    #[test]
    fn associate_result_shows_backup_dir_when_set() {
        let r = awt_core::report::Report {
            run_id: "x".into(),
            applied: vec![],
            backup_dir: "/tmp/backup/x".into(),
            verify: None,
        };
        let lines = associate_result_lines(&r, false, "E:/B/.claude-sessions");
        assert!(
            lines.iter().any(|l| l.contains("/tmp/backup/x")),
            "backup dir must appear in output lines"
        );
    }

    #[test]
    fn associate_result_shows_export_loc_when_no_backup() {
        let r = awt_core::report::Report {
            run_id: "x".into(),
            applied: vec![],
            backup_dir: String::new(),
            verify: None,
        };
        let lines = associate_result_lines(&r, true, "E:/B/.claude-sessions");
        assert!(
            lines.iter().any(|l| l.contains("E:/B/.claude-sessions")),
            "export location must appear in output lines when backup_dir is empty"
        );
    }

    #[test]
    fn associate_result_omits_location_when_export_false_and_no_backup() {
        let r = awt_core::report::Report {
            run_id: "x".into(),
            applied: vec![],
            backup_dir: String::new(),
            verify: None,
        };
        let lines = associate_result_lines(&r, false, "E:/B/.claude-sessions");
        assert_eq!(
            lines.len(),
            1,
            "no location line when neither backup nor export"
        );
    }
}
