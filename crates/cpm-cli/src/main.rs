mod exit;

use clap::{Parser, Subcommand};
use cpm_core::apply::{apply_verified, ApplyOpts};
use cpm_core::doctor::{doctor, scan};
use cpm_core::error::CpmError;
use cpm_core::fs::FileSystem;
use cpm_core::fs::RealFileSystem;
use cpm_core::model::{Move, Scope};
use cpm_core::plan::{build_plan, render_plan, Collision, PlanOpts};
use cpm_core::rollback::rollback;
use cpm_core::verify::verify;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "cpm",
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
        /// Write an HTML inventory to this path instead of printing a table
        #[arg(long)]
        html: Option<PathBuf>,
    },
    /// List all state that references a project's absolute path
    Scan {
        #[arg(long)]
        src: String,
    },
    /// Dry-run: print the changes that apply would make
    Plan {
        #[arg(long)]
        src: String,
        #[arg(long)]
        dst: String,
    },
    /// Move src to dst and rewrite all Claude state
    Apply {
        #[arg(long)]
        src: String,
        #[arg(long)]
        dst: String,
    },
    /// Check that Claude state correctly references dst after a move
    Verify {
        #[arg(long)]
        src: String,
        #[arg(long)]
        dst: String,
    },
    /// Restore pre-move state from a backup manifest
    Rollback {
        #[arg(long)]
        report: PathBuf,
    },
    /// Archive Claude session state before the 30-day auto-delete window
    Archive {
        /// Destination directory for archived data
        #[arg(long)]
        archive_dir: Option<PathBuf>,
        /// Archive a single session transcript (path to the .jsonl file)
        #[arg(long)]
        session: Option<PathBuf>,
        /// Install the cpm SessionEnd hook in ~/.claude/settings.json
        #[arg(long)]
        install_hook: bool,
        /// Remove the cpm SessionEnd hook from ~/.claude/settings.json
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
    },
}

fn home_of(cli: &Cli) -> Option<PathBuf> {
    cli.home.clone().or_else(|| {
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
    })
}

fn print_doctor(rep: &cpm_core::doctor::DoctorReport, json: bool) {
    let render = |s: &cpm_core::model::Stale| serde_json::json!({ "store": s.store, "reference": s.reference, "location": s.location });
    if json {
        println!(
            "{}",
            serde_json::json!({
                "stale": rep.stale.iter().map(render).collect::<Vec<_>>(),
                "report_only": rep.report_only.iter().map(render).collect::<Vec<_>>(),
                "unresolved": rep.unresolved.iter()
                    .map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
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
}

fn print_scan(rep: &cpm_core::doctor::ScanReport, src: &str, json: bool) {
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

fn health_to_str(h: &cpm_core::list::Health) -> &'static str {
    match h {
        cpm_core::list::Health::Ok => "ok",
        cpm_core::list::Health::Stale => "stale",
        cpm_core::list::Health::Unresolved => "unresolved",
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

fn run(cli: &Cli, fs: &RealFileSystem, home: &std::path::Path) -> cpm_core::error::Result<()> {
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
            let recs = cpm_core::list::list(fs, home, now_secs)?;
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
                print!("{}", cpm_core::list::render_table(&recs));
            }
            if let Some(path) = html {
                let content = cpm_core::list::render_html(&recs);
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
            print!("{}", render_plan(&plan));
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
            println!(
                "applied {} changes; backup {}",
                r.applied.len(),
                r.backup_dir
            );
            Ok(())
        }
        Cmd::Verify { src, dst } => {
            let mv = Move {
                src_abs: src.clone(),
                dst_abs: dst.clone(),
            };
            let results = verify(fs, home, &mv, None)?;
            let failed = results.iter().filter(|r| !r.ok).count();
            for r in &results {
                println!(
                    "  [{}] {}: {}",
                    if r.ok { "ok" } else { "FAIL" },
                    r.check,
                    r.detail
                );
            }
            if failed > 0 {
                return Err(CpmError::VerifyFailed(format!("{failed} failed")));
            }
            Ok(())
        }
        Cmd::Rollback { report } => rollback(report, fs),
        Cmd::Archive {
            archive_dir,
            session,
            install_hook,
            uninstall_hook,
            set_retention: days,
            force_zero,
            render,
        } => {
            if *install_hook {
                let exe = std::env::current_exe().map_err(cpm_core::error::CpmError::Io)?;
                cpm_core::settings::install_session_end_hook(fs, home, &exe)?;
                println!("cpm SessionEnd hook installed in ~/.claude/settings.json");
                return Ok(());
            }
            if *uninstall_hook {
                cpm_core::settings::uninstall_session_end_hook(fs, home)?;
                println!("cpm SessionEnd hook removed from ~/.claude/settings.json");
                return Ok(());
            }
            if let Some(d) = days {
                cpm_core::settings::set_retention(fs, home, *d, *force_zero)?;
                println!("cleanupPeriodDays set to {d}");
                eprintln!(
                    "note: issues #23710 and #62272 warn against setting this to 0; \
                     minimum safe value is 1"
                );
                return Ok(());
            }
            if let Some(transcript) = session.as_ref() {
                let dir = archive_dir.clone().ok_or_else(|| {
                    CpmError::Locked("--archive-dir is required with --session".into())
                })?;
                warn_if_cloud_synced(&dir);
                let opts = cpm_core::archive::ArchiveOpts {
                    archive_dir: dir,
                    render: *render,
                };
                let r = cpm_core::archive::archive_session(fs, home, transcript, &opts)?;
                println!("archived: {} copied, {} skipped", r.copied, r.skipped);
                return Ok(());
            }
            let dir = archive_dir
                .clone()
                .ok_or_else(|| CpmError::Locked("--archive-dir is required for archive".into()))?;
            warn_if_cloud_synced(&dir);
            let opts = cpm_core::archive::ArchiveOpts {
                archive_dir: dir,
                render: *render,
            };
            let r = cpm_core::archive::archive_all(fs, home, &opts)?;
            println!("archived: {} copied, {} skipped", r.copied, r.skipped);
            Ok(())
        }
    }
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
            eprintln!("error: {e:?}");
            ExitCode::from(exit::code_for(&e) as u8)
        }
    }
}
