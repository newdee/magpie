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

/// Words that list the notes when typed alone (#26: after `note …` the user
/// wants to see that it was kept, and what else is there). Only the command
/// word itself: `notes`, `笔记` and the like also name real folders and
/// files, and the list would bury them.
const LIST_ALL: &[&str] = &["note"];

/// Whether the query asks for the list of notes.
pub fn is_list_query(query: &str) -> bool {
    let q = query.trim().to_lowercase();
    LIST_ALL.contains(&q.as_str())
}

/// One note as [`append`] wrote it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Entry {
    /// `2026-09-01 14:03`, local time
    pub stamp: String,
    pub text: String,
    /// 1-based line in what was read (the whole file unless it is over
    /// [`READ_CAP`]): tells two same-text notes apart
    pub line: usize,
}

/// The newest `limit` notes, newest first, and how many the file holds.
/// Only lines in [`append`]'s shape count: a chosen file may be a whole
/// notebook, and its other lines are not notes this list should claim.
/// A missing file is no notes. Only the last [`READ_CAP`] bytes are read.
pub fn recent(path: &Path, limit: usize) -> Result<(Vec<Entry>, usize)> {
    let bytes = match read_tail(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((Vec::new(), 0)),
        Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
    };
    let body = String::from_utf8_lossy(&bytes);
    let all: Vec<Entry> = body
        .lines()
        .enumerate()
        .filter_map(|(i, l)| parse_line(l).map(|(stamp, text)| Entry { stamp, text, line: i + 1 }))
        .collect();
    let total = all.len();
    Ok((all.into_iter().rev().take(limit).collect(), total))
}

/// How much of the end of the file [`recent`] reads: thousands of notes, and
/// a bounded read when the chosen file is a large notebook.
pub const READ_CAP: u64 = 1 << 20;

fn read_tail(path: &Path) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    let mut buf = Vec::new();
    if len > READ_CAP {
        f.seek(SeekFrom::Start(len - READ_CAP))?;
        f.read_to_end(&mut buf)?;
        // the cut lands inside a line: drop that partial first line
        let first = buf.iter().position(|&b| b == b'\n').map_or(buf.len(), |i| i + 1);
        buf.drain(..first);
    } else {
        f.read_to_end(&mut buf)?;
    }
    Ok(buf)
}

/// `- 2026-09-01 14:03  text` → (stamp, text); anything else → None.
fn parse_line(line: &str) -> Option<(String, String)> {
    let rest = line.trim_end_matches('\r').strip_prefix("- ")?;
    let stamp = rest.get(..16)?;
    let b = stamp.as_bytes();
    let shape = b.iter().enumerate().all(|(i, &c)| match i {
        4 | 7 => c == b'-',
        10 => c == b' ',
        13 => c == b':',
        _ => c.is_ascii_digit(),
    });
    let text = rest.get(16..)?.strip_prefix("  ")?.trim();
    (shape && !text.is_empty()).then(|| (stamp.to_string(), text.to_string()))
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
    fn parse_reads_only_appended_lines() {
        assert_eq!(parse_line("- 2026-09-01 14:03  buy milk"), Some(("2026-09-01 14:03".into(), "buy milk".into())));
        assert_eq!(parse_line("- 2026-09-01 14:03  买牛奶\r"), Some(("2026-09-01 14:03".into(), "买牛奶".into())));
        for other in ["# Notes", "- buy milk", "- 2026-09-01 14:03 one space", "- 2026-09-01 14:03  ", "-2026-09-01 14:03  x", "- 2026/09/01 14:03  x", "- 中文中文中文中文  x", ""] {
            assert_eq!(parse_line(other), None, "{other:?}");
        }
    }

    #[test]
    fn recent_is_newest_first_and_counts_all() {
        let dir = std::env::temp_dir().join(format!("magpie-notes-recent-{}", std::process::id()));
        let path = dir.join("notes.md");
        assert_eq!(recent(&path, 5).unwrap(), (Vec::new(), 0), "no file, no notes");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            &path,
            "# my notebook\n- 2026-09-01 10:00  one\nsome prose\n- 2026-09-02 10:00  two\r\n- 2026-09-03 10:00  three\n",
        )
        .unwrap();
        let (list, total) = recent(&path, 2).unwrap();
        assert_eq!(total, 3);
        assert_eq!(list.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(), ["three", "two"]);
        assert_eq!((list[0].line, list[1].line), (5, 4));
        // an appended note is the first one listed
        append(&path, "four").unwrap();
        assert_eq!(recent(&path, 1).unwrap().0[0].text, "four");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recent_reads_a_bounded_tail() {
        let dir = std::env::temp_dir().join(format!("magpie-notes-tail-{}", std::process::id()));
        let path = dir.join("big.md");
        std::fs::create_dir_all(&dir).unwrap();
        let line = "- 2026-09-01 10:00  a note of some length to fill the file\n";
        let n = (READ_CAP as usize / line.len()) * 2;
        std::fs::write(&path, line.repeat(n) + "- 2026-09-02 10:00  last\n").unwrap();
        let (list, total) = recent(&path, 3).unwrap();
        assert_eq!(list[0].text, "last");
        assert!(total < n && total > n / 3, "only the tail is read: {total} of {n}");
        assert!(list.iter().all(|e| e.stamp.len() == 16), "no torn first line");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_words() {
        for q in ["note", " Note "] {
            assert!(is_list_query(q), "{q:?}");
        }
        for q in ["note x", "notes", "笔记", "notebook", "", "no"] {
            assert!(!is_list_query(q), "{q:?}");
        }
    }

    #[test]
    fn default_path_sits_next_to_the_database() {
        assert_eq!(default_path(Path::new("/data")), PathBuf::from("/data/notes.md"));
    }
}
