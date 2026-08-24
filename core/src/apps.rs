//! Installed-application launcher: enumerates apps from OS-standard locations
//! and matches them by name. No indexing/DB — the list is small and rebuilt
//! on demand, then filtered per keystroke.

use anyhow::{anyhow, Result};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct AppEntry {
    pub name: String,
    /// Path launched when chosen (a .lnk, .app bundle, or executable/desktop).
    pub target: String,
    pub score: f32,
}

/// Enumerate installed applications from platform-standard locations.
pub fn list_apps() -> Vec<AppEntry> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut push = |name: String, target: PathBuf| {
        let key = name.to_lowercase();
        if !name.is_empty() && seen.insert(key) {
            out.push(AppEntry { name, target: target.to_string_lossy().into_owned(), score: 0.0 });
        }
    };

    #[cfg(target_os = "windows")]
    {
        let roots = [std::env::var("ProgramData").ok(), std::env::var("APPDATA").ok()];
        for root in roots.into_iter().flatten() {
            let start = PathBuf::from(root).join("Microsoft/Windows/Start Menu/Programs");
            for entry in walk(&start, "lnk") {
                if let Some(stem) = entry.file_stem().and_then(|s| s.to_str()) {
                    push(stem.to_string(), entry.clone());
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").map(PathBuf::from).unwrap_or_default();
        let dirs = [
            PathBuf::from("/Applications"),
            PathBuf::from("/System/Applications"),
            home.join("Applications"),
        ];
        for dir in dirs {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for e in entries.flatten() {
                    let p = e.path();
                    if p.extension().and_then(|x| x.to_str()) == Some("app") {
                        if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                            push(stem.to_string(), p.clone());
                        }
                    }
                }
            }
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let home = std::env::var("HOME").map(PathBuf::from).unwrap_or_default();
        let dirs = [
            PathBuf::from("/usr/share/applications"),
            PathBuf::from("/usr/local/share/applications"),
            home.join(".local/share/applications"),
        ];
        for dir in dirs {
            for entry in walk(&dir, "desktop") {
                if let Some((name, _)) = parse_desktop(&entry) {
                    push(name, entry.clone());
                }
            }
        }
    }
    out
}

/// Rank apps against a query. Prefix match beats substring beats subsequence.
pub fn match_apps(apps: &[AppEntry], query: &str, limit: usize) -> Vec<AppEntry> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<AppEntry> = apps
        .iter()
        .filter_map(|a| {
            let name = a.name.to_lowercase();
            let score = if name == q {
                1.0
            } else if name.starts_with(&q) {
                0.9 - 0.001 * name.len() as f32 // shorter prefix match ranks higher
            } else if name.contains(&q) {
                0.6
            } else if matches_initials(&q, &name) {
                0.5 // "vsc" -> "Visual Studio Code"
            } else {
                return None;
            };
            let mut e = a.clone();
            e.score = score;
            Some(e)
        })
        .collect();
    scored.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.name.len().cmp(&b.name.len())));
    scored.truncate(limit);
    scored
}

/// Acronym match: does `q` spell out the initials of the words in `name`?
/// "vsc" matches "Visual Studio Code"; "code" does NOT match "RecoveryDrive".
/// Word initials are letters starting a word or following a space/-/_/. .
fn matches_initials(q: &str, name: &str) -> bool {
    let initials: String = {
        let mut prev_boundary = true;
        let mut acc = String::new();
        for c in name.chars() {
            if prev_boundary && c.is_alphanumeric() {
                acc.push(c);
            }
            prev_boundary = matches!(c, ' ' | '-' | '_' | '.' | '/');
        }
        acc
    };
    initials.starts_with(q) && q.len() >= 2
}

/// Launch an application by the target recorded in [`AppEntry`].
pub fn launch_app(target: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        // ShellExecute via `cmd start` resolves .lnk targets and arguments
        std::process::Command::new("cmd")
            .args(["/c", "start", "", target])
            .spawn()
            .map_err(|e| anyhow!("launch: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(target)
            .spawn()
            .map_err(|e| anyhow!("launch: {e}"))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let exec = parse_desktop(std::path::Path::new(target))
            .map(|(_, e)| e)
            .unwrap_or_else(|| target.to_string());
        // strip .desktop field codes (%u, %F, ...) and run the first token
        let cleaned: Vec<String> = exec
            .split_whitespace()
            .filter(|t| !t.starts_with('%'))
            .map(String::from)
            .collect();
        if let Some((cmd, args)) = cleaned.split_first() {
            std::process::Command::new(cmd)
                .args(args)
                .spawn()
                .map_err(|e| anyhow!("launch: {e}"))?;
        } else {
            return Err(anyhow!("no exec in {target}"));
        }
    }
    Ok(())
}

#[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
fn walk(dir: &std::path::Path, ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|x| x.to_str()) == Some(ext) {
                out.push(p);
            }
        }
    }
    out
}

#[cfg(all(unix, not(target_os = "macos")))]
fn parse_desktop(path: &std::path::Path) -> Option<(String, String)> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut name = None;
    let mut exec = None;
    let mut no_display = false;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("Name=") {
            name.get_or_insert_with(|| v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("Exec=") {
            exec.get_or_insert_with(|| v.trim().to_string());
        } else if line.strip_prefix("NoDisplay=").map(|v| v.trim() == "true").unwrap_or(false) {
            no_display = true;
        }
    }
    if no_display {
        return None;
    }
    Some((name?, exec?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str) -> AppEntry {
        AppEntry { name: name.into(), target: format!("/x/{name}"), score: 0.0 }
    }

    #[test]
    fn ranks_exact_then_shortest_substring() {
        let apps = vec![app("Visual Studio Code"), app("Code"), app("QR Code Reader"), app("Xcode")];
        let hits = match_apps(&apps, "code", 10);
        assert_eq!(hits[0].name, "Code", "exact match wins");
        // remaining are substring matches, shortest name first
        assert_eq!(hits[1].name, "Xcode");
        assert_eq!(hits.len(), 4);
    }

    #[test]
    fn prefix_beats_substring() {
        let apps = vec![app("Google Chrome"), app("Chrome")];
        let hits = match_apps(&apps, "chrome", 10);
        assert_eq!(hits[0].name, "Chrome", "exact/prefix beats mid-string");
    }

    #[test]
    fn acronym_matches_word_initials_only() {
        let apps = vec![app("Visual Studio Code"), app("RecoveryDrive")];
        let hits = match_apps(&apps, "vsc", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "Visual Studio Code", "initials v-s-c");
        // 'code' must NOT match RecoveryDrive as a loose subsequence
        // (it legitimately substring-matches "Visual Studio Code", so test in isolation)
        assert!(match_apps(&[app("RecoveryDrive")], "code", 10).is_empty());
        assert!(match_apps(&apps, "zzz", 10).is_empty());
    }

    #[test]
    fn empty_query_matches_nothing() {
        assert!(match_apps(&[app("Safari")], "  ", 10).is_empty());
    }
}
