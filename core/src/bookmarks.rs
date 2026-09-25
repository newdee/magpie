//! Browser bookmark indexing: reads local bookmark stores directly (Chromium
//! JSON files, Firefox places.sqlite) — no browser APIs, no network, nothing
//! leaves the machine.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;

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

/// Gecko keeps places.sqlite locked while running; read a snapshot.
fn parse_firefox(browser: &str, path: &Path, out: &mut Vec<RawBookmark>) -> Result<()> {
    let snap = crate::browsers::Snapshot::of(path)?;
    let result = (|| -> Result<()> {
        let conn = snap.open()?;
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
                browser: browser.to_string(),
                added_at: added.map(|a| a / 1_000_000), // micros -> secs
            });
        }
        Ok(())
    })();
    result
}

// ---------- sync ----------

/// Read every discovered bookmark store and mirror it into the index.
pub fn sync_bookmarks(conn: &Connection) -> Result<BookmarkReport> {
    let mut raw = Vec::new();
    let mut report = BookmarkReport::default();
    use crate::browsers::Engine;
    for profile in crate::browsers::profiles() {
        let (browser, path) = (profile.browser.clone(), profile.bookmarks_file());
        if !path.is_file() {
            continue; // a Chromium profile with history but no bookmarks yet
        }
        let before = raw.len();
        let res = match profile.engine {
            Engine::Gecko => parse_firefox(&browser, &path, &mut raw),
            Engine::Chromium => parse_chromium(&browser, &path, &mut raw),
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
        if crate::threads::stopping(crate::threads::Model::Text) {
            break; // a reload wants the model; its catch-up pass resumes here
        }
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
    Ok(done)
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
    let mut rows = stmt
        .query_map(params![fts_query, limit as i64], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    // FTS matches token *prefixes* only — "sms" can never reach "longsms",
    // and domains concatenate words all the time. A LIKE scan over the same
    // columns supplements mid-token matches, appended below the FTS ranks.
    append_substring_matches(
        conn,
        query,
        "SELECT id FROM bookmarks
         WHERE title LIKE ?1 ESCAPE '\\' OR url LIKE ?1 ESCAPE '\\' OR folder LIKE ?1 ESCAPE '\\'
         LIMIT ?2",
        limit,
        &mut rows,
    )?;
    Ok(rows)
}

/// Append ids whose text contains `query` as a raw substring (case-insensitive
/// for ASCII) and are not already in `ids`. Shared by bookmark/history search.
pub(crate) fn append_substring_matches(
    conn: &Connection,
    query: &str,
    sql: &str,
    limit: usize,
    ids: &mut Vec<i64>,
) -> Result<()> {
    let q = query.trim();
    if q.len() < 2 {
        return Ok(());
    }
    let pat = format!(
        "%{}%",
        q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
    );
    let mut stmt = conn.prepare(sql)?;
    let extra = stmt
        .query_map(params![pat, limit as i64], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for id in extra {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(())
}

/// One bookmark by URL (the frecency identity for web hits). Several browser
/// profiles may hold the same URL; the first is as good as any.
pub fn bookmark_by_url(conn: &Connection, url: &str) -> Result<Option<BookmarkHit>> {
    let mut stmt = conn.prepare(
        "SELECT id, url, title, folder, browser, added_at FROM bookmarks WHERE url = ?1 LIMIT 1",
    )?;
    Ok(stmt
        .query_row([url], |r| {
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
        .optional()?)
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
