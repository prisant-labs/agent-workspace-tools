mod exit;

use clap::{Parser, Subcommand};
use cpm_core::doctor::{doctor, scan};
use cpm_core::fs::RealFileSystem;
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

fn main() -> ExitCode {
    let cli = Cli::parse();
    let fs = RealFileSystem;

    let Some(home) = home_of(&cli) else {
        eprintln!("error: no home directory found; pass --home explicitly");
        return ExitCode::from(1);
    };

    let result = match &cli.cmd {
        Cmd::Doctor => doctor(&fs, &home).map(|r| print_doctor(&r, cli.json)),
        Cmd::Scan { src } => scan(&fs, &home, src).map(|r| print_scan(&r, src, cli.json)),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:?}");
            ExitCode::from(exit::code_for(&e) as u8)
        }
    }
}
