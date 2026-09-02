//! Browser history indexing: reads local history stores directly (Chromium
//! `History` SQLite, Firefox `places.sqlite`) — no browser APIs, no network.
//!
//! History is large, so only the most-visited pages per profile are kept
//! (`TOP_PER_PROFILE`); ranking later favors visit count and recency.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::embed::{self, Embedder};

const EMBED_BATCH: usize = 16;
/// Most-visited pages kept per browser profile. Bounds DB size and embedding.
const TOP_PER_PROFILE: usize = 3_000;

#[derive(Debug, Clone, Serialize)]
pub struct HistoryHit {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub browser: String,
    pub visit_count: i64,
    pub last_visit: Option<i64>,
    pub score: f32,
}

#[derive(Debug, Default, Serialize)]
pub struct HistoryReport {
    pub browsers: Vec<String>,
    pub total: usize,
    pub removed: usize,
}

struct RawHistory {
    url: String,
    title: String,
    browser: String,
    visit_count: i64,
    last_visit: Option<i64>,
}

// ---------- discovery ----------

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// (browser, history-file path, is_firefox) for every discovered profile.
fn discover() -> Vec<(String, PathBuf, bool)> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Chromium: each profile keeps a `History` SQLite file. Firefox reuses
    // `places.sqlite` (history + bookmarks in one DB). We reuse the browser
    // bookmark discovery to locate profile directories, then swap filenames.
    for (browser, bmark_path, is_firefox) in crate::bookmarks::discover_history_sources() {
        let hist = if is_firefox {
            bmark_path // places.sqlite already
        } else {
            match bmark_path.parent() {
                Some(dir) => dir.join("History"),
                None => continue,
            }
        };
        if hist.is_file() && seen.insert(hist.clone()) {
            out.push((browser, hist, is_firefox));
        }
    }
    let _ = &dirs_home; // reserved for future direct scans
    out
}

// ---------- parsing ----------

/// Chromium epoch (1601-01-01) microseconds -> unix seconds.
fn webkit_to_unix(micros: i64) -> Option<i64> {
    if micros <= 0 {
        return None;
    }
    Some(micros / 1_000_000 - 11_644_473_600)
}

/// History DBs are locked while the browser runs; read a temp copy.
fn read_copy<T>(path: &Path, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
    let tmp = std::env::temp_dir().join(format!(
        "magpie-hist-{}-{}.sqlite",
        std::process::id(),
        path.file_name().and_then(|n| n.to_str()).unwrap_or("db")
    ));
    std::fs::copy(path, &tmp)?;
    let result = (|| {
        let conn = Connection::open_with_flags(&tmp, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        f(&conn)
    })();
    let _ = std::fs::remove_file(&tmp);
    result
}

fn parse_chromium(browser: &str, path: &Path, out: &mut Vec<RawHistory>) -> Result<()> {
    read_copy(path, |conn| {
        let mut stmt = conn.prepare(
            "SELECT url, IFNULL(title,''), visit_count, last_visit_time
             FROM urls WHERE url LIKE 'http%' AND visit_count > 0
             ORDER BY visit_count DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([TOP_PER_PROFILE as i64], |r| {
            Ok(RawHistory {
                url: r.get(0)?,
                title: r.get(1)?,
                browser: browser.to_string(),
                visit_count: r.get(2)?,
                last_visit: webkit_to_unix(r.get::<_, i64>(3)?),
            })
        })?;
        for row in rows {
            out.push(row?);
        }
        Ok(())
    })
}

fn parse_firefox(path: &Path, out: &mut Vec<RawHistory>) -> Result<()> {
    read_copy(path, |conn| {
        let mut stmt = conn.prepare(
            "SELECT url, IFNULL(title,''), visit_count, last_visit_date
             FROM moz_places WHERE url LIKE 'http%' AND visit_count > 0
             ORDER BY visit_count DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([TOP_PER_PROFILE as i64], |r| {
            Ok(RawHistory {
                url: r.get(0)?,
                title: r.get(1)?,
                browser: "firefox".to_string(),
                visit_count: r.get(2)?,
                last_visit: r.get::<_, Option<i64>>(3)?.map(|m| m / 1_000_000),
            })
        })?;
        for row in rows {
            out.push(row?);
        }
        Ok(())
    })
}

// ---------- sync ----------

/// Read every discovered history store and mirror the top pages into the index.
pub fn sync_history(conn: &Connection) -> Result<HistoryReport> {
    let mut raw = Vec::new();
    let mut report = HistoryReport::default();
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
    for h in &raw {
        tx.execute(
            "INSERT INTO history(url, title, browser, visit_count, last_visit)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(browser, url) DO UPDATE SET
                title = excluded.title,
                visit_count = excluded.visit_count,
                last_visit = excluded.last_visit",
            params![h.url, h.title, h.browser, h.visit_count, h.last_visit],
        )?;
        let id: i64 = tx.query_row(
            "SELECT id FROM history WHERE browser=?1 AND url=?2",
            params![h.browser, h.url],
            |r| r.get(0),
        )?;
        seen.push(id);
    }
    tx.execute("CREATE TEMP TABLE IF NOT EXISTS keep_hist (id INTEGER PRIMARY KEY)", [])?;
    tx.execute("DELETE FROM keep_hist", [])?;
    {
        let mut stmt = tx.prepare("INSERT OR IGNORE INTO keep_hist(id) VALUES (?1)")?;
        for id in &seen {
            stmt.execute([id])?;
        }
    }
    report.removed =
        tx.execute("DELETE FROM history WHERE id NOT IN (SELECT id FROM keep_hist)", [])?;
    tx.execute("DELETE FROM keep_hist", [])?;
    tx.commit()?;
    report.total = seen.len();
    Ok(report)
}

// ---------- embeddings ----------

fn history_doc(title: &str, url: &str) -> String {
    format!("{title}\n{url}")
}

pub fn embed_pending_history(
    conn: &Connection,
    embedder: &mut Embedder,
    mut progress: impl FnMut(usize, usize),
) -> Result<usize> {
    let hashes: std::collections::HashMap<i64, String> = {
        let mut stmt = conn.prepare("SELECT history_id, doc_hash FROM history_vecs")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        rows
    };
    let pending: Vec<(i64, String, String)> = {
        let mut stmt = conn.prepare("SELECT id, title, url FROM history")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .filter_map(|(id, title, url)| {
                let doc = history_doc(&title, &url);
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
                "INSERT INTO history_vecs(history_id, doc_hash, dim, vec)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(history_id) DO UPDATE SET doc_hash = excluded.doc_hash,
                                                       dim = excluded.dim, vec = excluded.vec",
                params![id, hash, vec.len() as i64, bytes],
            )?;
        }
        done += chunk.len();
        progress(done, total);
    }
    conn.execute(
        "DELETE FROM history_vecs WHERE history_id NOT IN (SELECT id FROM history)",
        [],
    )?;
    Ok(total)
}

pub fn all_history_embeddings(conn: &Connection) -> Result<Vec<(i64, Vec<f32>)>> {
    let mut stmt = conn.prepare("SELECT history_id, dim, vec FROM history_vecs")?;
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

pub fn history_fts_search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<i64>> {
    let fts_query = crate::db::build_fts_query(query);
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT rowid FROM history_fts WHERE history_fts MATCH ?1
         ORDER BY bm25(history_fts, 5.0, 1.0) LIMIT ?2",
    )?;
    let mut rows = stmt
        .query_map(params![fts_query, limit as i64], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    // mid-token substring supplement (FTS only matches token prefixes;
    // "sms" must still find "longsms.net") — see bookmarks_fts_search
    crate::bookmarks::append_substring_matches(
        conn,
        query,
        "SELECT id FROM history
         WHERE title LIKE ?1 ESCAPE '\\' OR url LIKE ?1 ESCAPE '\\'
         LIMIT ?2",
        limit,
        &mut rows,
    )?;
    Ok(rows)
}

/// One history entry by URL (the frecency identity for web hits).
pub fn history_by_url(conn: &Connection, url: &str) -> Result<Option<HistoryHit>> {
    let mut stmt = conn.prepare(
        "SELECT id, url, title, browser, visit_count, last_visit FROM history WHERE url = ?1 LIMIT 1",
    )?;
    Ok(stmt
        .query_row([url], |r| {
            Ok(HistoryHit {
                id: r.get(0)?,
                url: r.get(1)?,
                title: r.get(2)?,
                browser: r.get(3)?,
                visit_count: r.get(4)?,
                last_visit: r.get(5)?,
                score: 0.0,
            })
        })
        .optional()?)
}

pub fn history_by_ids(
    conn: &Connection,
    ids: &[i64],
    scores: &std::collections::HashMap<i64, f32>,
) -> Result<Vec<HistoryHit>> {
    let mut out = Vec::with_capacity(ids.len());
    let mut stmt = conn.prepare(
        "SELECT id, url, title, browser, visit_count, last_visit FROM history WHERE id = ?1",
    )?;
    for id in ids {
        let hit = stmt
            .query_row([id], |r| {
                Ok(HistoryHit {
                    id: r.get(0)?,
                    url: r.get(1)?,
                    title: r.get(2)?,
                    browser: r.get(3)?,
                    visit_count: r.get(4)?,
                    last_visit: r.get(5)?,
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

pub fn history_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM history", [], |r| r.get(0))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_roundtrip_and_fts() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO history(url, title, browser, visit_count, last_visit)
             VALUES ('https://doc.rust-lang.org/book/', 'The Rust Programming Language', 'chrome', 42, 1700000000)",
            [],
        )
        .unwrap();
        let hits = history_fts_search(&conn, "rust", 10).unwrap();
        assert_eq!(hits.len(), 1);
        let got = history_by_ids(&conn, &hits, &Default::default()).unwrap();
        assert_eq!(got[0].visit_count, 42);
        assert_eq!(history_count(&conn).unwrap(), 1);
    }
}
