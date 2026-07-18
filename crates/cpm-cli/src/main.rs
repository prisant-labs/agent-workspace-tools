mod exit;

use clap::{Parser, Subcommand};
use cpm_core::apply::{apply_verified, ApplyOpts};
use cpm_core::doctor::{doctor, scan};
use cpm_core::error::CpmError;
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

fn run(cli: &Cli, fs: &RealFileSystem, home: &std::path::Path) -> cpm_core::error::Result<()> {
    match &cli.cmd {
        Cmd::Doctor => {
            let r = doctor(fs, home)?;
            print_doctor(&r, cli.json);
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
