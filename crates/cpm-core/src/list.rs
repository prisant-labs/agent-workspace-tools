use crate::fs::FileSystem;
use crate::index::ProjectIndex;
use crate::model::{Ctx, Move, Scope, Store};
use crate::paths::normalize_path;
use crate::sessions::{footprint, SessionFootprint};
use crate::stores::claude_history::ClaudeHistory;
use crate::stores::claude_json::ClaudeJson;
use crate::stores::plugin_state::PluginState;
use std::path::Path;

#[derive(Debug)]
pub enum Health {
    Ok,
    Stale,
    Unresolved,
}

pub struct ProjectRecord {
    pub cwd: Option<String>,
    pub encoded_dir: String,
    pub sessions: usize,
    pub bytes: u64,
    pub oldest_days: Option<f64>,
    pub newest_days: Option<f64>,
    pub footprint: SessionFootprint,
    pub json_keys: usize,
    pub github_paths: usize,
    pub history_lines: usize,
    pub plugin_dirs: usize,
    pub health: Health,
}

/// Scan a project dir for .jsonl files; return (count, total_bytes, oldest_days, newest_days).
/// Ages are computed relative to now_secs. Returns None for age fields when mtime is unavailable.
fn scan_dir(
    fs: &dyn FileSystem,
    dir: &Path,
    now_secs: u64,
) -> (usize, u64, Option<f64>, Option<f64>) {
    let mut count = 0usize;
    let mut total_bytes = 0u64;
    let mut oldest: Option<u64> = None; // smallest mtime = largest age
    let mut newest: Option<u64> = None; // largest mtime = smallest age

    for child in fs.read_dir(dir).unwrap_or_default() {
        if child.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        count += 1;
        if let Ok(data) = fs.read(&child) {
            total_bytes += data.len() as u64;
        }
        if let Ok(mtime) = fs.mtime_secs(&child) {
            oldest = Some(match oldest {
                None => mtime,
                Some(m) => m.min(mtime),
            });
            newest = Some(match newest {
                None => mtime,
                Some(m) => m.max(mtime),
            });
        }
    }

    let to_days = |t: u64| -> f64 { now_secs.saturating_sub(t) as f64 / 86_400.0 };

    (count, total_bytes, oldest.map(to_days), newest.map(to_days))
}

/// Gather PATH-keyed reference counts via the store adapters.
/// cwd_key is the normalized (lowercase, forward-slash) form.
/// original_cwd is the original backslash form; used for PluginState which hashes the exact bytes.
fn ac31_counts(
    fs: &dyn FileSystem,
    home: &Path,
    index: &ProjectIndex,
    cwd_key: &str,
    original_cwd: Option<&str>,
) -> (usize, usize, usize, usize) {
    let mv_norm = Move {
        src_abs: cwd_key.to_string(),
        dst_abs: String::new(),
    };
    let ctx = Ctx {
        fs,
        home: home.to_path_buf(),
        index,
        scope: Scope::Standard,
    };

    let json_hits = ClaudeJson.detect(&ctx, &mv_norm).unwrap_or_default();
    let json_keys = json_hits
        .iter()
        .filter(|h| h.detail.starts_with("projects key"))
        .count();
    let github_paths = json_hits
        .iter()
        .filter(|h| h.detail.starts_with("githubRepoPaths"))
        .count();

    let hist_hits = ClaudeHistory.detect(&ctx, &mv_norm).unwrap_or_default();
    let history_lines: usize = hist_hits
        .iter()
        .map(|h| {
            h.detail
                .split_whitespace()
                .next()
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(0)
        })
        .sum();

    // PluginState hashes the EXACT backslash bytes, so pass the original cwd if available.
    let plugin_src = original_cwd.unwrap_or(cwd_key);
    let mv_plugin = Move {
        src_abs: plugin_src.to_string(),
        dst_abs: String::new(),
    };
    let plugin_dirs = PluginState
        .detect(&ctx, &mv_plugin)
        .unwrap_or_default()
        .len();

    (json_keys, github_paths, history_lines, plugin_dirs)
}

/// Build a ProjectRecord for one project dir.
#[allow(clippy::too_many_arguments)]
fn build_record(
    fs: &dyn FileSystem,
    home: &Path,
    index: &ProjectIndex,
    dir: &Path,
    cwd: Option<String>,
    cwd_key: Option<&str>,
    health: Health,
    now_secs: u64,
) -> ProjectRecord {
    let (sessions, bytes, oldest_days, newest_days) = scan_dir(fs, dir, now_secs);
    let fp = footprint(fs, home, dir);
    let encoded = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let (json_keys, github_paths, history_lines, plugin_dirs) = match cwd_key {
        Some(key) => ac31_counts(fs, home, index, key, cwd.as_deref()),
        None => (0, 0, 0, 0),
    };

    ProjectRecord {
        cwd,
        encoded_dir: encoded,
        sessions,
        bytes,
        oldest_days,
        newest_days,
        footprint: fp,
        json_keys,
        github_paths,
        history_lines,
        plugin_dirs,
        health,
    }
}

/// Enumerate every project Claude has state for, with session counts, size,
/// transcript ages, PATH-keyed reference counts, and a health flag.
///
/// Health is determined through the INJECTED fs, not the real disk, keeping
/// the function fully testable and consistent with the rest of the codebase.
/// Ambiguous dirs (multiple live cwds) are included as Unresolved rather than
/// silently dropped, preserving the inventory's completeness guarantee.
pub fn list(fs: &dyn FileSystem, home: &Path, now_secs: u64) -> Vec<ProjectRecord> {
    let index = ProjectIndex::build(fs, home);
    let mut records = Vec::new();

    // Resolved projects: one canonical cwd per entry.
    for (cwd_key, dirs) in &index.by_cwd {
        // Health is the same for every dir under this cwd_key.
        let is_live = fs.is_dir(Path::new(cwd_key.as_str()));
        // Look up the original (backslash) form from the recorded cwds list.
        let original_cwd = index
            .cwds
            .iter()
            .find(|c| normalize_path(c) == *cwd_key)
            .cloned();
        for dir in dirs {
            let health = if is_live { Health::Ok } else { Health::Stale };
            let rec = build_record(
                fs,
                home,
                &index,
                dir,
                original_cwd.clone(),
                Some(cwd_key.as_str()),
                health,
                now_secs,
            );
            records.push(rec);
        }
    }

    // Unresolved: no recoverable cwd, or every recorded cwd is gone.
    for dir in &index.unresolved {
        let rec = build_record(
            fs,
            home,
            &index,
            dir,
            None,
            None,
            Health::Unresolved,
            now_secs,
        );
        records.push(rec);
    }

    // Ambiguous: multiple cwds still exist; cannot pick one without guessing.
    // Emit a record so the inventory stays complete - never silently drop a project.
    for dir in &index.ambiguous {
        let rec = build_record(
            fs,
            home,
            &index,
            dir,
            None,
            None,
            Health::Unresolved,
            now_secs,
        );
        records.push(rec);
    }

    records
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn health_str(h: &Health) -> &'static str {
    match h {
        Health::Ok => "ok",
        Health::Stale => "stale",
        Health::Unresolved => "unresolved",
    }
}

/// Render a plain-text table of the inventory.
pub fn render_table(records: &[ProjectRecord]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:<52} {:>8} {:>11} {:>6} {:>10}  {}\n",
        "CWD / ENCODED DIR", "SESSIONS", "BYTES", "KEYS", "AGE(days)", "HEALTH"
    ));
    out.push_str(&"-".repeat(100));
    out.push('\n');
    for r in records {
        let label = r.cwd.as_deref().unwrap_or(r.encoded_dir.as_str());
        let age = r
            .newest_days
            .map(|d| format!("{:.1}", d))
            .unwrap_or_else(|| "-".into());
        out.push_str(&format!(
            "{:<52} {:>8} {:>11} {:>6} {:>10}  {}\n",
            label,
            r.sessions,
            r.bytes,
            r.json_keys,
            age,
            health_str(&r.health),
        ));
    }
    out
}

/// Render a self-contained HTML inventory page (no external assets).
pub fn render_html(records: &[ProjectRecord]) -> String {
    let rows: String = records
        .iter()
        .map(|r| {
            let label = r.cwd.as_deref().unwrap_or(r.encoded_dir.as_str());
            let age = r
                .newest_days
                .map(|d| format!("{:.1}", d))
                .unwrap_or_else(|| "-".into());
            let health = health_str(&r.health);
            let cls = match r.health {
                Health::Ok => "ok",
                Health::Stale => "stale",
                Health::Unresolved => "unresolved",
            };
            format!(
                "<tr class=\"{cls}\"><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                escape_html(label),
                r.sessions,
                r.bytes,
                r.json_keys,
                age,
                health
            )
        })
        .collect();

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>CPM Project Inventory</title>
<style>
body {{ font-family: monospace; padding: 1rem; background: #fff; color: #222; }}
@media (prefers-color-scheme: dark) {{
  body {{ background: #1e1e1e; color: #d4d4d4; }}
  th {{ background: #333; }}
  td {{ border-color: #555; }}
}}
:root[data-theme="dark"] body {{ background: #1e1e1e; color: #d4d4d4; }}
:root[data-theme="light"] body {{ background: #fff; color: #222; }}
table {{ border-collapse: collapse; width: 100%; }}
th, td {{ border: 1px solid #ccc; padding: 4px 8px; text-align: left; }}
th {{ background: #eee; }}
tr.stale td {{ color: #a00; }}
tr.unresolved td {{ color: #808; }}
</style>
</head>
<body>
<h1>CPM Project Inventory</h1>
<p>{total} projects</p>
<table>
<thead><tr>
<th>CWD / Encoded Dir</th>
<th>Sessions</th>
<th>Bytes</th>
<th>JSON Keys</th>
<th>Newest (days)</th>
<th>Health</th>
</tr></thead>
<tbody>
{rows}</tbody>
</table>
</body>
</html>
"#,
        total = records.len(),
        rows = rows,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemoryFileSystem;
    use std::path::Path;

    #[test]
    fn list_reports_sessions_and_health() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
            b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n",
        )
        .unwrap();
        // The source folder E:/Projects/A is NOT written to the fs -> fs.is_dir is false -> STALE
        let recs = list(&fs, Path::new("/h"), 1_000_000);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].sessions, 1);
        assert!(matches!(recs[0].health, Health::Stale));
    }

    #[test]
    fn list_ok_when_source_folder_exists() {
        // Positive counterpart so "STALE" is a real discrimination, not the only outcome.
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
            b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n",
        )
        .unwrap();
        fs.write(Path::new("E:/Projects/A/.keep"), b"x").unwrap(); // folder exists -> OK
        let recs = list(&fs, Path::new("/h"), 1_000_000);
        assert!(matches!(recs[0].health, Health::Ok), "{:?}", recs[0].health);
    }

    #[test]
    fn list_counts_json_keys_ac31() {
        let fs = MemoryFileSystem::new();
        fs.write(
            Path::new("/h/.claude/projects/E--Projects-A/s.jsonl"),
            b"{\"cwd\":\"E:\\\\Projects\\\\A\"}\n",
        )
        .unwrap();
        fs.write(
            Path::new("/h/.claude.json"),
            br#"{"projects":{"E:\\Projects\\A":{}}}"#,
        )
        .unwrap();
        let recs = list(&fs, Path::new("/h"), 1_000_000);
        assert!(recs.iter().any(|r| r.json_keys >= 1));
    }
}
