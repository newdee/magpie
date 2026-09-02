//! Quick capture: `note buy milk` appends one timestamped line to a markdown
//! file. Plain text on disk, no format of our own — the file is meant to be
//! opened in whatever the user already keeps notes in.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Where notes go when the user has not chosen a file: next to the database.
pub fn default_path(data_dir: &Path) -> PathBuf {
    data_dir.join("notes.md")
}

/// One line, `- 2026-09-01 14:03  text`, newlines folded to spaces so a note
/// never breaks the list. Creates the file and its folder on first use.
pub fn append(path: &Path, text: &str) -> Result<()> {
    let text = text.trim();
    if text.is_empty() {
        anyhow::bail!("empty note");
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let line = format_line(chrono::Local::now().format("%Y-%m-%d %H:%M").to_string().as_str(), text);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open {}", path.display()))?;
    f.write_all(line.as_bytes())?;
    Ok(())
}

fn format_line(stamp: &str, text: &str) -> String {
    let folded = text.split('\n').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
    format!("- {stamp}  {folded}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_shape_and_newline_folding() {
        assert_eq!(format_line("2026-09-01 14:03", "buy milk"), "- 2026-09-01 14:03  buy milk\n");
        assert_eq!(format_line("s", "a\n  b \n\nc"), "- s  a b c\n");
    }

    #[test]
    fn appends_and_creates_the_folder() {
        let dir = std::env::temp_dir().join(format!("magpie-notes-{}", std::process::id()));
        let path = dir.join("deep").join("notes.md");
        append(&path, "first").unwrap();
        append(&path, "  second  ").unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("- ") && lines[0].ends_with("  first"));
        assert!(lines[1].ends_with("  second"));
        assert!(append(&path, "   ").is_err(), "blank notes are refused");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_path_sits_next_to_the_database() {
        assert_eq!(default_path(Path::new("/data")), PathBuf::from("/data/notes.md"));
    }
}
