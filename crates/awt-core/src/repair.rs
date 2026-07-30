//! S-03: repair `history.jsonl` entries whose drive letter has been corrupted.
//!
//! A real machine was found with 46 distinct `project` values, across 3,121 lines, of the form
//! `::\Projects\X` where `E:\Projects\X` belongs - every capital `E` had become `:`. Those lines
//! are unreachable, because Claude Code cannot match them to any project, so that prompt history
//! is lost while still occupying the file.
//!
//! `doctor` reports these as stale and refuses them, which is correct: it will not guess. This
//! module closes the gap between "will not guess" and "cannot help", under the same rule the rest
//! of the tool uses - **act when there is exactly one answer, refuse when there is not**.
//!
//! Deliberately narrow (see decision D8). The only transformation is the leading drive prefix,
//! and the only accepted repair is one where exactly one present drive makes the path resolve.
//! A repair command is the most dangerous surface this tool could grow, because it writes based on
//! inference rather than on a path the user supplied, so each transformation stays named,
//! separately guarded, and auditable.

use crate::error::Result;
use crate::fs::FileSystem;
use crate::model::Change;
use crate::rewrite::RewriteRule;
use std::path::{Path, PathBuf};

/// The two-character prefix that marks a corrupted drive letter.
const MALFORMED_PREFIX: &str = "::";

/// One distinct malformed `project` value and how many lines carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Malformed {
    pub value: String,
    pub lines: usize,
}

/// The verdict for one malformed value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Candidate {
    /// Not corrupted at all; nothing to do.
    NotMalformed,
    /// Exactly one present drive makes the repaired path resolve. Safe to repair.
    Repairable(char),
    /// No present drive makes it resolve. The folder is gone too, so there is nothing to
    /// repair *to*. Reported, never written.
    NoCandidate,
    /// More than one drive would resolve. Refused: choosing would be guessing.
    Ambiguous(Vec<char>),
}

/// A repair the plan will perform.
#[derive(Debug, Clone)]
pub struct Repair {
    pub from: String,
    pub to: String,
    pub drive: char,
    pub lines: usize,
}

/// The full outcome of planning, including what was declined and why. Callers need the declined
/// sets as much as the accepted ones: "I did not touch these, for this reason" is the part that
/// makes a repair command trustworthy.
#[derive(Debug)]
pub struct RepairPlan {
    pub path: PathBuf,
    pub repairs: Vec<Repair>,
    pub unrepairable: Vec<Malformed>,
    pub ambiguous: Vec<(Malformed, Vec<char>)>,
    pub change: Option<Change>,
}

impl RepairPlan {
    /// Total history lines that would be repaired.
    pub fn total_lines(&self) -> usize {
        self.repairs.iter().map(|r| r.lines).sum()
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "path": self.path.to_string_lossy(),
            "repairs": self.repairs.iter().map(|r| serde_json::json!({
                "from": r.from, "to": r.to, "drive": r.drive.to_string(), "lines": r.lines,
            })).collect::<Vec<_>>(),
            "unrepairable": self.unrepairable.iter().map(|m| serde_json::json!({
                "value": m.value, "lines": m.lines,
                "reason": "no present drive makes this path resolve",
            })).collect::<Vec<_>>(),
            "ambiguous": self.ambiguous.iter().map(|(m, c)| serde_json::json!({
                "value": m.value, "lines": m.lines,
                "candidates": c.iter().map(|d| d.to_string()).collect::<Vec<_>>(),
                "reason": "more than one drive would resolve; refusing rather than guessing",
            })).collect::<Vec<_>>(),
            "totals": {
                "repairable_values": self.repairs.len(),
                "repairable_lines": self.total_lines(),
                "unrepairable_values": self.unrepairable.len(),
                "ambiguous_values": self.ambiguous.len(),
            },
        })
    }
}

/// Every distinct `project` value with a malformed drive prefix, with its line count.
///
/// Parses each line only to read the field, then discards the parse - the rewrite is a byte
/// splice, exactly as elsewhere. Unparseable lines are skipped rather than failing the run: a
/// damaged file is the premise here, so refusing to look at it would defeat the purpose.
pub fn scan_malformed(text: &str) -> Vec<Malformed> {
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(p) = v.get("project").and_then(|x| x.as_str()) {
            if p.starts_with(MALFORMED_PREFIX) {
                *counts.entry(p.to_string()).or_insert(0) += 1;
            }
        }
    }
    counts
        .into_iter()
        .map(|(value, lines)| Malformed { value, lines })
        .collect()
}

/// Drive letters that currently exist, probed through the injected filesystem rather than any OS
/// API so the set is deterministic and controllable in tests.
///
/// Both `X:/` and `X:` are probed because the two `FileSystem` implementations disagree about how
/// a drive root is spelled. `RealFileSystem` needs the trailing slash - bare `X:` is a
/// drive-relative path on Windows, not the root. `MemoryFileSystem` keys on the normalized string
/// and treats a directory as "some key starts with `<path>/`", so `X:/` becomes a lookup for
/// `x://` and never matches. Accepting either form keeps one code path correct on both.
///
/// Restricting to present drives first also avoids stat-ing all 26 letters per value on a real
/// machine, which historically could poke removable-drive hardware.
pub fn present_drives(fs: &dyn FileSystem) -> Vec<char> {
    ('A'..='Z')
        .filter(|d| {
            fs.is_dir(Path::new(&format!("{d}:/"))) || fs.is_dir(Path::new(&format!("{d}:")))
        })
        .collect()
}

/// Substitute `drive` for the malformed prefix. `::\Projects\X` becomes `E:\Projects\X`.
fn repaired_with(value: &str, drive: char) -> String {
    format!("{drive}:{}", &value[MALFORMED_PREFIX.len()..])
}

/// Decide what, if anything, may be done with one value.
pub fn classify(fs: &dyn FileSystem, value: &str, drives: &[char]) -> Candidate {
    if !value.starts_with(MALFORMED_PREFIX) {
        return Candidate::NotMalformed;
    }
    let hits: Vec<char> = drives
        .iter()
        .copied()
        .filter(|d| {
            let candidate = repaired_with(value, *d).replace('\\', "/");
            fs.is_dir(Path::new(&candidate))
        })
        .collect();
    match hits.len() {
        0 => Candidate::NoCandidate,
        1 => Candidate::Repairable(hits[0]),
        _ => Candidate::Ambiguous(hits),
    }
}

/// Build the repair plan for a home. Writes nothing.
pub fn build_repair_plan(fs: &dyn FileSystem, home: &Path) -> Result<RepairPlan> {
    let path = home.join(".claude").join("history.jsonl");
    let mut plan = RepairPlan {
        path: path.clone(),
        repairs: Vec::new(),
        unrepairable: Vec::new(),
        ambiguous: Vec::new(),
        change: None,
    };
    if !fs.exists(&path) {
        return Ok(plan);
    }
    let bytes = fs.read(&path)?;
    // Lossy is right here: the file is damaged by premise, and refusing to read it because of a
    // stray byte would block the very repair that is being asked for. Nothing is re-serialized.
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let drives = present_drives(fs);

    for m in scan_malformed(&text) {
        match classify(fs, &m.value, &drives) {
            Candidate::Repairable(drive) => plan.repairs.push(Repair {
                to: repaired_with(&m.value, drive),
                from: m.value.clone(),
                drive,
                lines: m.lines,
            }),
            Candidate::NoCandidate => plan.unrepairable.push(m),
            Candidate::Ambiguous(c) => plan.ambiguous.push((m, c)),
            Candidate::NotMalformed => {}
        }
    }

    if !plan.repairs.is_empty() {
        // Anchor on the whole `"project":"<value>"` field so a path that merely appears inside
        // some other field, or inside prompt text, is never touched.
        let esc = |s: &str| s.replace('\\', "\\\\");
        let rules: Vec<RewriteRule> = plan
            .repairs
            .iter()
            .map(|r| RewriteRule {
                find: format!("\"project\":\"{}\"", esc(&r.from)),
                replace: format!("\"project\":\"{}\"", esc(&r.to)),
            })
            .collect();
        let (_, expected) = crate::rewrite::anchored_rewrite(&text, &rules);
        plan.change = Some(Change::RewriteFile {
            path,
            rules,
            expected,
        });
    }
    Ok(plan)
}

/// Apply a repair plan through the standard write path: snapshot first, count-checked splice,
/// and a manifest that `awt rollback` consumes like any other run.
pub fn apply_repair(
    plan: &RepairPlan,
    fs: &dyn FileSystem,
    backup_root: &Path,
    run_id: &str,
) -> Result<crate::report::Report> {
    let Some(change) = plan.change.clone() else {
        // Nothing to do is a success, not an error. This is what makes repair idempotent.
        return Ok(crate::report::Report {
            run_id: run_id.to_string(),
            applied: vec![],
            backup_dir: String::new(),
            verify: None,
        });
    };
    let inner = crate::plan::Plan {
        mv: crate::model::Move {
            src_abs: String::new(),
            dst_abs: String::new(),
        },
        changes: vec![change],
        warnings: Vec::new(),
        nested: Vec::new(),
        home: plan
            .path
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or(Path::new(""))
            .to_path_buf(),
    };
    crate::apply::apply(&inner, fs, backup_root, run_id)
}

/// Human-readable rendering of a repair plan.
pub fn render_repair(plan: &RepairPlan) -> String {
    let mut out = String::new();
    if plan.repairs.is_empty() {
        out.push_str("No repairable history entries found.\n");
    } else {
        out.push_str(&format!(
            "Repairable: {} value(s), {} history line(s)\n",
            plan.repairs.len(),
            plan.total_lines()
        ));
        for r in &plan.repairs {
            out.push_str(&format!(
                "  {} -> {}  ({} line(s), drive {} is the only match)\n",
                r.from, r.to, r.lines, r.drive
            ));
        }
    }
    if !plan.unrepairable.is_empty() {
        out.push_str(&format!(
            "\nNot repairable ({}): no existing drive makes these resolve, so there is nothing to repair to.\n",
            plan.unrepairable.len()
        ));
        for m in &plan.unrepairable {
            out.push_str(&format!("  {} ({} line(s))\n", m.value, m.lines));
        }
    }
    if !plan.ambiguous.is_empty() {
        out.push_str(&format!(
            "\nRefused as ambiguous ({}): more than one drive would resolve. Choosing would be guessing.\n",
            plan.ambiguous.len()
        ));
        for (m, c) in &plan.ambiguous {
            let cands: Vec<String> = c.iter().map(|d| format!("{d}:")).collect();
            out.push_str(&format!(
                "  {} ({} line(s), candidates: {})\n",
                m.value,
                m.lines,
                cands.join(", ")
            ));
        }
    }
    out
}
