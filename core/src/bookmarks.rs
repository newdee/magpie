//! Browser bookmark indexing: reads local bookmark stores directly (Chromium
//! JSON files, Firefox places.sqlite) — no browser APIs, no network, nothing
//! leaves the machine.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::embed::{self, Embedder};

const EMBED_BATCH: usize = 16;

#[derive(Debug, Clone, Serialize)]
pub struct BookmarkHit {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub folder: String,
    pub browser: String,
    pub added_at: Option<i64>,
    pub score: f32,
}

#[derive(Debug, Default, Serialize)]
pub struct BookmarkReport {
    pub browsers: Vec<String>,
    pub total: usize,
    pub removed: usize,
}

struct RawBookmark {
    url: String,
    title: String,
    folder: String,
    browser: String,
    added_at: Option<i64>,
}

// ---------- discovery ----------

/// Candidate bookmark stores for installed browsers, per platform.
fn discover() -> Vec<(String, PathBuf, bool)> {
    // (browser name, path, is_firefox)
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let home = dirs_home();

    // Known browsers first so they keep stable names; then a generic sweep of
    // the platform data root so any Chromium fork (Ego, Arc, Vivaldi, ...) is
    // picked up without a hardcoded path.
    #[cfg(target_os = "windows")]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let local = PathBuf::from(local);
            push_chromium_profiles("chrome", &local.join("Google/Chrome/User Data"), &mut seen, &mut out);
            push_chromium_profiles("edge", &local.join("Microsoft/Edge/User Data"), &mut seen, &mut out);
            push_chromium_profiles("brave", &local.join("BraveSoftware/Brave-Browser/User Data"), &mut seen, &mut out);
            scan_chromium_forks(&local, &mut seen, &mut out);
        }
        if let Ok(roaming) = std::env::var("APPDATA") {
            push_firefox(&mut out, PathBuf::from(roaming).join("Mozilla/Firefox/Profiles"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        let sup = home.join("Library/Application Support");
        push_chromium_profiles("chrome", &sup.join("Google/Chrome"), &mut seen, &mut out);
        push_chromium_profiles("edge", &sup.join("Microsoft Edge"), &mut seen, &mut out);
        push_chromium_profiles("brave", &sup.join("BraveSoftware/Brave-Browser"), &mut seen, &mut out);
        scan_chromium_forks(&sup, &mut seen, &mut out);
        push_firefox(&mut out, sup.join("Firefox/Profiles"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let cfg = home.join(".config");
        push_chromium_profiles("chrome", &cfg.join("google-chrome"), &mut seen, &mut out);
        push_chromium_profiles("chromium", &cfg.join("chromium"), &mut seen, &mut out);
        push_chromium_profiles("edge", &cfg.join("microsoft-edge"), &mut seen, &mut out);
        push_chromium_profiles("brave", &cfg.join("BraveSoftware/Brave-Browser"), &mut seen, &mut out);
        scan_chromium_forks(&cfg, &mut seen, &mut out);
        push_firefox(&mut out, home.join(".mozilla/firefox"));
    }
    let _ = (&home, &mut seen);
    out
}

/// Collect `Default` / `Profile *` bookmark files directly under `base`.
fn push_chromium_profiles(
    browser: &str,
    base: &Path,
    seen: &mut std::collections::HashSet<PathBuf>,
    out: &mut Vec<(String, PathBuf, bool)>,
) {
    if let Ok(entries) = std::fs::read_dir(base) {
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() && (name == "Default" || name.starts_with("Profile")) {
                let f = p.join("Bookmarks");
                if f.is_file() && seen.insert(f.clone()) {
                    out.push((browser.to_string(), f, false));
                }
            }
        }
    }
}

/// Sweep a platform data root for Chromium forks. Every fork keeps profiles
/// either directly in its data dir, under a `User Data` subdir, or one vendor
/// level deeper (`Google/Chrome`, `BraveSoftware/Brave-Browser`). Only actual
/// `Default`/`Profile *`/`Bookmarks` layouts match, and parsing later requires
/// the Chromium JSON shape, so unrelated app dirs contribute nothing.
fn scan_chromium_forks(
    root: &Path,
    seen: &mut std::collections::HashSet<PathBuf>,
    out: &mut Vec<(String, PathBuf, bool)>,
) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for e in entries.flatten() {
        let dir = e.path();
        if !dir.is_dir() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_lowercase();
        push_chromium_profiles(&name, &dir, seen, out);
        push_chromium_profiles(&name, &dir.join("User Data"), seen, out);
        if let Ok(subs) = std::fs::read_dir(&dir) {
            for s in subs.flatten() {
                let sub = s.path();
                if !sub.is_dir() {
                    continue;
                }
                let sub_name = s.file_name().to_string_lossy().to_lowercase();
                push_chromium_profiles(&sub_name, &sub, seen, out);
                push_chromium_profiles(&sub_name, &sub.join("User Data"), seen, out);
            }
        }
    }
}

/// Same profile discovery as [`discover`], exposed for the history module so
/// it can locate profile dirs (Chromium `History` lives beside `Bookmarks`;
/// Firefox history shares `places.sqlite`).
pub(crate) fn discover_history_sources() -> Vec<(String, PathBuf, bool)> {
    discover()
}

fn push_firefox(out: &mut Vec<(String, PathBuf, bool)>, profiles: PathBuf) {
    if let Ok(entries) = std::fs::read_dir(&profiles) {
        for e in entries.flatten() {
            let f = e.path().join("places.sqlite");
            if f.is_file() {
                out.push(("firefox".to_string(), f, true));
            }
        }
    }
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

// ---------- parsing ----------

/// Chromium epoch (1601-01-01) microseconds -> unix seconds.
fn webkit_to_unix(micros: i64) -> Option<i64> {
    if micros <= 0 {
        return None;
    }
    Some(micros / 1_000_000 - 11_644_473_600)
}

fn parse_chromium(browser: &str, path: &Path, out: &mut Vec<RawBookmark>) -> Result<()> {
    let raw = std::fs::read_to_string(path)?;
    let json: serde_json::Value = serde_json::from_str(&raw)?;
    let Some(roots) = json.get("roots").and_then(|r| r.as_object()) else {
        return Ok(());
    };
    for (_, root) in roots {
        walk_chromium(browser, root, "", out);
    }
    Ok(())
}

fn walk_chromium(browser: &str, node: &serde_json::Value, folder: &str, out: &mut Vec<RawBookmark>) {
    match node.get("type").and_then(|t| t.as_str()) {
        Some("url") => {
            let url = node.get("url").and_then(|u| u.as_str()).unwrap_or_default();
            if url.is_empty() {
                return;
            }
            out.push(RawBookmark {
                url: url.to_string(),
                title: node
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or(url)
                    .to_string(),
                folder: folder.to_string(),
                browser: browser.to_string(),
                added_at: node
                    .get("date_added")
                    .and_then(|d| d.as_str())
                    .and_then(|d| d.parse::<i64>().ok())
                    .and_then(webkit_to_unix),
            });
        }
        Some("folder") | None => {
            let name = node.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let sub = if folder.is_empty() {
                name.to_string()
            } else if name.is_empty() {
                folder.to_string()
            } else {
                format!("{folder}/{name}")
            };
            if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
                for child in children {
                    walk_chromium(browser, child, &sub, out);
                }
            }
        }
        _ => {}
    }
}

/// Firefox keeps places.sqlite locked while running; read a temp copy.
fn parse_firefox(path: &Path, out: &mut Vec<RawBookmark>) -> Result<()> {
    let tmp = std::env::temp_dir().join(format!("magpie-places-{}.sqlite", std::process::id()));
    std::fs::copy(path, &tmp)?;
    let result = (|| -> Result<()> {
        let conn = Connection::open_with_flags(
            &tmp,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        // folder tree for path strings
        let mut folders: std::collections::HashMap<i64, (i64, String)> =
            std::collections::HashMap::new();
        {
            let mut stmt =
                conn.prepare("SELECT id, parent, IFNULL(title,'') FROM moz_bookmarks WHERE type = 2")?;
            let rows = stmt.query_map([], |r| {
                Ok((r.get::<_, i64>(0)?, (r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))
            })?;
            for row in rows {
                let (id, v) = row?;
                folders.insert(id, v);
            }
        }
        let folder_path = |mut id: i64| -> String {
            let mut parts = Vec::new();
            for _ in 0..32 {
                let Some((parent, title)) = folders.get(&id) else { break };
                if !title.is_empty() {
                    parts.push(title.clone());
                }
                id = *parent;
            }
            parts.reverse();
            parts.join("/")
        };
        let mut stmt = conn.prepare(
            "SELECT IFNULL(b.title, ''), p.url, b.parent, b.dateAdded
             FROM moz_bookmarks b JOIN moz_places p ON b.fk = p.id
             WHERE b.type = 1 AND p.url LIKE 'http%'",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<i64>>(3)?,
            ))
        })?;
        for row in rows {
            let (title, url, parent, added) = row?;
            out.push(RawBookmark {
                title: if title.is_empty() { url.clone() } else { title },
                url,
                folder: folder_path(parent),
                browser: "firefox".to_string(),
                added_at: added.map(|a| a / 1_000_000), // micros -> secs
            });
        }
        Ok(())
    })();
    let _ = std::fs::remove_file(&tmp);
    result
}

// ---------- sync ----------

/// Read every discovered bookmark store and mirror it into the index.
pub fn sync_bookmarks(conn: &Connection) -> Result<BookmarkReport> {
    let mut raw = Vec::new();
    let mut report = BookmarkReport::default();
    for (browser, path, is_firefox) in discover() {
        let before = raw.len();
        let res = if is_firefox {
            parse_firefox(&path, &mut raw)
        } else {
            parse_chromium(&browser, &path, &mut raw)
        };
        if res.is_ok() && raw.len() > before && !report.browsers.contains(&browser) {
            report.browsers.push(browser);
        }
    }

    let tx = conn.unchecked_transaction()?;
    let mut seen = Vec::new();
    for b in &raw {
        tx.execute(
            "INSERT INTO bookmarks(url, title, folder, browser, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(browser, url, folder, title) DO UPDATE SET
                added_at = excluded.added_at",
            params![b.url, b.title, b.folder, b.browser, b.added_at],
        )?;
        let id: i64 = tx.query_row(
            "SELECT id FROM bookmarks WHERE browser=?1 AND url=?2 AND folder=?3 AND title=?4",
            params![b.browser, b.url, b.folder, b.title],
            |r| r.get(0),
        )?;
        seen.push(id);
    }
    // prune deleted bookmarks
    tx.execute("CREATE TEMP TABLE IF NOT EXISTS keep_bm (id INTEGER PRIMARY KEY)", [])?;
    tx.execute("DELETE FROM keep_bm", [])?;
    {
        let mut stmt = tx.prepare("INSERT OR IGNORE INTO keep_bm(id) VALUES (?1)")?;
        for id in &seen {
            stmt.execute([id])?;
        }
    }
    report.removed = tx.execute("DELETE FROM bookmarks WHERE id NOT IN (SELECT id FROM keep_bm)", [])?;
    tx.execute("DELETE FROM keep_bm", [])?;
    tx.commit()?;
    report.total = seen.len();
    Ok(report)
}

// ---------- embeddings ----------

fn bookmark_doc(title: &str, url: &str, folder: &str) -> String {
    format!("{title}\n{folder}\n{url}")
}

/// One vector per bookmark (they have no body text). Blocking.
pub fn embed_pending_bookmarks(
    conn: &Connection,
    embedder: &mut Embedder,
    mut progress: impl FnMut(usize, usize),
) -> Result<usize> {
    let hashes: std::collections::HashMap<i64, String> = {
        let mut stmt = conn.prepare("SELECT bookmark_id, doc_hash FROM bookmark_vecs")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        rows
    };
    let pending: Vec<(i64, String, String)> = {
        let mut stmt = conn.prepare("SELECT id, title, url, folder FROM bookmarks")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .filter_map(|(id, title, url, folder)| {
                let doc = bookmark_doc(&title, &url, &folder);
                let hash = embed::doc_hash(&doc);
                if hashes.get(&id) == Some(&hash) {
                    None
                } else {
                    Some((id, doc, hash))
                }
            })
            .collect()
    };

    let total = pending.len();
    let mut done = 0usize;
    progress(0, total);
    for chunk in pending.chunks(EMBED_BATCH) {
        let docs: Vec<String> = chunk.iter().map(|(_, d, _)| d.clone()).collect();
        let vecs = embedder.embed_passages(&docs)?;
        for ((id, _, hash), vec) in chunk.iter().zip(vecs) {
            let bytes: Vec<u8> = vec.iter().flat_map(|f| f.to_le_bytes()).collect();
            conn.execute(
                "INSERT INTO bookmark_vecs(bookmark_id, doc_hash, dim, vec)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(bookmark_id) DO UPDATE SET doc_hash = excluded.doc_hash,
                                                        dim = excluded.dim, vec = excluded.vec",
                params![id, hash, vec.len() as i64, bytes],
            )?;
        }
        done += chunk.len();
        progress(done, total);
    }
    conn.execute(
        "DELETE FROM bookmark_vecs WHERE bookmark_id NOT IN (SELECT id FROM bookmarks)",
        [],
    )?;
    Ok(total)
}

pub fn all_bookmark_embeddings(conn: &Connection) -> Result<Vec<(i64, Vec<f32>)>> {
    let mut stmt = conn.prepare("SELECT bookmark_id, dim, vec FROM bookmark_vecs")?;
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)? as usize, r.get::<_, Vec<u8>>(2)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut out = Vec::with_capacity(rows.len());
    for (id, dim, bytes) in rows {
        if bytes.len() != dim * 4 {
            continue;
        }
        let (chunks, _) = bytes.as_chunks::<4>();
        out.push((id, chunks.iter().map(|c| f32::from_le_bytes(*c)).collect()));
    }
    Ok(out)
}

// ---------- retrieval ----------

pub fn bookmarks_fts_search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<i64>> {
    let fts_query = crate::db::build_fts_query(query);
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT rowid FROM bookmarks_fts WHERE bookmarks_fts MATCH ?1
         ORDER BY bm25(bookmarks_fts, 8.0, 2.0, 3.0) LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![fts_query, limit as i64], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn bookmarks_by_ids(
    conn: &Connection,
    ids: &[i64],
    scores: &std::collections::HashMap<i64, f32>,
) -> Result<Vec<BookmarkHit>> {
    let mut out = Vec::with_capacity(ids.len());
    let mut stmt =
        conn.prepare("SELECT id, url, title, folder, browser, added_at FROM bookmarks WHERE id = ?1")?;
    for id in ids {
        let hit = stmt
            .query_row([id], |r| {
                Ok(BookmarkHit {
                    id: r.get(0)?,
                    url: r.get(1)?,
                    title: r.get(2)?,
                    folder: r.get(3)?,
                    browser: r.get(4)?,
                    added_at: r.get(5)?,
                    score: 0.0,
                })
            })
            .optional()?;
        if let Some(mut h) = hit {
            h.score = scores.get(&h.id).copied().unwrap_or(0.0);
            out.push(h);
        }
    }
    Ok(out)
}

pub fn bookmark_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM bookmarks", [], |r| r.get(0))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chromium_parse_walks_folders() {
        let json = r#"{
          "roots": {
            "bookmark_bar": {
              "type": "folder", "name": "Bookmarks bar",
              "children": [
                { "type": "url", "name": "Rust Book", "url": "https://doc.rust-lang.org/book/", "date_added": "13300000000000000" },
                { "type": "folder", "name": "Dev",
                  "children": [
                    { "type": "url", "name": "crates.io", "url": "https://crates.io" }
                  ]
                }
              ]
            }
          }
        }"#;
        let tmp = std::env::temp_dir().join(format!("magpie-bm-{}.json", std::process::id()));
        std::fs::write(&tmp, json).unwrap();
        let mut out = Vec::new();
        parse_chromium("chrome", &tmp, &mut out).unwrap();
        let _ = std::fs::remove_file(&tmp);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].title, "Rust Book");
        assert_eq!(out[0].folder, "Bookmarks bar");
        assert!(out[0].added_at.unwrap() > 1_500_000_000, "webkit time converts");
        assert_eq!(out[1].folder, "Bookmarks bar/Dev");
    }

    #[test]
    #[ignore = "diagnostic: prints real browser stores found on this machine"]
    fn print_discovered_stores() {
        let t = std::time::Instant::now();
        let found = discover();
        for (b, p, ff) in &found {
            println!("{b}\tfirefox={ff}\t{}", p.display());
        }
        println!("{} stores in {:?}", found.len(), t.elapsed());
    }

    #[test]
    fn discovers_chromium_forks() {
        let root = std::env::temp_dir().join(format!("magpie-forks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mk = |p: &str| {
            let f = root.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(&f, "{}").unwrap();
        };
        mk("Ego/Default/Bookmarks"); // mac/linux-style: profiles in the data dir
        mk("Vendor/Fork/User Data/Profile 1/Bookmarks"); // windows-style vendor nesting
        mk("NotABrowser/settings.json"); // no profile layout -> ignored
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        scan_chromium_forks(&root, &mut seen, &mut out);
        let mut names: Vec<&str> = out.iter().map(|(b, _, _)| b.as_str()).collect();
        names.sort();
        assert_eq!(names, vec!["ego", "fork"]);
        // rescanning adds nothing: dedup is by bookmark-file path
        scan_chromium_forks(&root, &mut seen, &mut out);
        assert_eq!(out.len(), 2);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn bookmark_roundtrip_and_fts() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO bookmarks(url, title, folder, browser, added_at)
             VALUES ('https://example.com/scraper', 'Web Scraping Guide', 'Dev/Python', 'chrome', 1700000000)",
            [],
        )
        .unwrap();
        let hits = bookmarks_fts_search(&conn, "scraping", 10).unwrap();
        assert_eq!(hits.len(), 1);
        // folder path is searchable too
        assert_eq!(bookmarks_fts_search(&conn, "python", 10).unwrap().len(), 1);
        let got = bookmarks_by_ids(&conn, &hits, &Default::default()).unwrap();
        assert_eq!(got[0].title, "Web Scraping Guide");
        assert_eq!(bookmark_count(&conn).unwrap(), 1);
    }
}
