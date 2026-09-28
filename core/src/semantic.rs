//! Whether semantic search is on: the text and image models, downloaded on
//! first use (~500 MB and ~200 MB) and held in memory while running.
//!
//! Issue #8: a fresh install used to start downloading the text model the
//! moment it opened, and nobody could turn the models off. Keyword search,
//! app launching and every tool in the box work without them. Now a fresh
//! install asks first (the welcome screen), and the switch lives in
//! settings. Someone who used magpie before this existed keeps what they had.

use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

use crate::db;

pub const META_KEY: &str = "semantic";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    On,
    Off,
    /// A fresh install that has not answered the welcome screen yet: no
    /// model loads until it does.
    Undecided,
}

impl Choice {
    pub fn as_str(self) -> &'static str {
        match self {
            Choice::On => "on",
            Choice::Off => "off",
            Choice::Undecided => "undecided",
        }
    }
}

/// The stored choice. With none stored, an install that was used before
/// (a model on disk, any vectors, an indexed folder, a GitHub token) is
/// recorded as on, as it always was; anything else is undecided.
pub fn choice(conn: &Connection, model_dir: &Path) -> Result<Choice> {
    match db::meta_get(conn, META_KEY)?.as_deref() {
        Some("1") => return Ok(Choice::On),
        Some("0") => return Ok(Choice::Off),
        _ => {}
    }
    if used_before(conn, model_dir)? {
        db::meta_set(conn, META_KEY, "1")?;
        return Ok(Choice::On);
    }
    Ok(Choice::Undecided)
}

pub fn set(conn: &Connection, on: bool) -> Result<()> {
    db::meta_set(conn, META_KEY, if on { "1" } else { "0" })
}

fn used_before(conn: &Connection, model_dir: &Path) -> Result<bool> {
    let model_on_disk = ["models--intfloat--multilingual-e5-small", "manual-e5"]
        .iter()
        .any(|d| model_dir.join(d).is_dir());
    if model_on_disk {
        return Ok(true);
    }
    for table in [
        "repo_chunks",
        "file_chunks",
        "bookmark_vecs",
        "history_vecs",
        "clip_vecs",
        "image_embeddings",
        "folders",
    ] {
        let any: bool = conn.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table})"), [], |r| r.get(0))?;
        if any {
            return Ok(true);
        }
    }
    Ok(db::meta_get(conn, "token")?.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("magpie-semantic-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_fresh_install_is_undecided_until_it_answers() {
        let conn = db::open_in_memory().unwrap();
        let models = tmp("fresh");
        assert_eq!(choice(&conn, &models).unwrap(), Choice::Undecided);
        assert_eq!(db::meta_get(&conn, META_KEY).unwrap(), None, "nothing stored for it");
        set(&conn, false).unwrap();
        assert_eq!(choice(&conn, &models).unwrap(), Choice::Off);
        set(&conn, true).unwrap();
        assert_eq!(choice(&conn, &models).unwrap(), Choice::On);
    }

    #[test]
    fn an_install_used_before_stays_on() {
        // a downloaded model
        let conn = db::open_in_memory().unwrap();
        let models = tmp("model");
        std::fs::create_dir_all(models.join("models--intfloat--multilingual-e5-small")).unwrap();
        assert_eq!(choice(&conn, &models).unwrap(), Choice::On);
        assert_eq!(db::meta_get(&conn, META_KEY).unwrap().as_deref(), Some("1"), "recorded");
        // an indexed folder, no model yet (its download failed, say)
        let conn = db::open_in_memory().unwrap();
        let empty = tmp("folder");
        conn.execute("INSERT INTO folders(path) VALUES ('/x')", []).unwrap();
        assert_eq!(choice(&conn, &empty).unwrap(), Choice::On);
        // a GitHub token
        let conn = db::open_in_memory().unwrap();
        db::meta_set(&conn, "token", "t").unwrap();
        assert_eq!(choice(&conn, &empty).unwrap(), Choice::On);
        // a stored off wins over all of that
        let conn = db::open_in_memory().unwrap();
        conn.execute("INSERT INTO folders(path) VALUES ('/x')", []).unwrap();
        set(&conn, false).unwrap();
        assert_eq!(choice(&conn, &models).unwrap(), Choice::Off);
    }
}
