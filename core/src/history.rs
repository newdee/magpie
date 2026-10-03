//! Browser history indexing: reads local history stores directly (Chromium
//! `History` SQLite, Firefox `places.sqlite`) — no browser APIs, no network.
//!
//! History is large, so only the most-visited pages per profile are kept
//! (`TOP_PER_PROFILE`); ranking later favors visit count and recency.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;

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
    /// Every browser that visited this URL (search results only).
    pub browsers: Vec<String>,
    pub visit_count: i64,
    pub last_visit: Option<i64>,
    pub score: f32,
    /// Found by meaning alone, with no keyword in common (search results).
    pub fuzzy: bool,
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
    /// the earliest visit the browser still keeps (Chrome drops visits
    /// after about 90 days, so this is not always the very first)
    first_visit: Option<i64>,
    /// the page's own summary, which Firefox keeps (`og:description`)
    description: Option<String>,
}

/// A page summary as shown: trimmed, short, empty treated as none.
fn tidy_description(d: Option<String>) -> Option<String> {
    let d = d?;
    let d = d.trim();
    if d.is_empty() {
        return None;
    }
    Some(d.chars().take(500).collect())
}

// ---------- parsing ----------

/// Chromium epoch (1601-01-01) microseconds -> unix seconds.
fn webkit_to_unix(micros: i64) -> Option<i64> {
    if micros <= 0 {
        return None;
    }
    Some(micros / 1_000_000 - 11_644_473_600)
}

/// History DBs are locked while the browser runs; read a snapshot.
fn read_copy<T>(path: &Path, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
    let snap = crate::browsers::Snapshot::of(path)?;
    let conn = snap.open()?;
    f(&conn)
}

fn parse_chromium(browser: &str, path: &Path, out: &mut Vec<RawHistory>) -> Result<()> {
    read_copy(path, |conn| {
        // every Chromium keeps `visits`; a fork without it still reads
        let has_visits = conn.prepare("SELECT visit_time FROM visits LIMIT 0").is_ok();
        let sql = format!(
            "SELECT url, IFNULL(title,''), visit_count, last_visit_time, {}
             FROM urls WHERE url LIKE 'http%' AND visit_count > 0
             ORDER BY visit_count DESC LIMIT ?1",
            if has_visits { "(SELECT MIN(visit_time) FROM visits WHERE visits.url = urls.id)" } else { "NULL" }
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([TOP_PER_PROFILE as i64], |r| {
            Ok(RawHistory {
                url: r.get(0)?,
                title: r.get(1)?,
                browser: browser.to_string(),
                visit_count: r.get(2)?,
                last_visit: webkit_to_unix(r.get::<_, i64>(3)?),
                first_visit: r.get::<_, Option<i64>>(4)?.and_then(webkit_to_unix),
                description: None,
            })
        })?;
        for row in rows {
            out.push(row?);
        }
        Ok(())
    })
}

fn parse_firefox(browser: &str, path: &Path, out: &mut Vec<RawHistory>) -> Result<()> {
    read_copy(path, |conn| {
        // `description` arrived in Firefox 63; an older profile has none
        let has_description = conn
            .prepare("SELECT description FROM moz_places LIMIT 0")
            .is_ok();
        let sql = format!(
            "SELECT url, IFNULL(title,''), visit_count, last_visit_date,
                    (SELECT MIN(visit_date) FROM moz_historyvisits WHERE place_id = moz_places.id),
                    {}
             FROM moz_places WHERE url LIKE 'http%' AND visit_count > 0
             ORDER BY visit_count DESC LIMIT ?1",
            if has_description { "description" } else { "NULL" }
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([TOP_PER_PROFILE as i64], |r| {
            Ok(RawHistory {
                url: r.get(0)?,
                title: r.get(1)?,
                browser: browser.to_string(),
                visit_count: r.get(2)?,
                last_visit: r.get::<_, Option<i64>>(3)?.map(|m| m / 1_000_000),
                first_visit: r.get::<_, Option<i64>>(4)?.map(|m| m / 1_000_000),
                description: tidy_description(r.get(5)?),
            })
        })?;
        for row in rows {
            out.push(row?);
        }
        Ok(())
    })
}

// ---------- sync ----------

/// One row per (browser, url): two profiles of one browser (Chrome's
/// "Default" and "Profile 1") both visiting a page used to overwrite each
/// other, the last profile read winning. Visits add up, the latest visit
/// and its title win, the earliest visit is the earliest of all, and a
/// summary is kept from whichever profile has one.
fn merge_profiles(raw: Vec<RawHistory>) -> Vec<RawHistory> {
    let mut out: Vec<RawHistory> = Vec::with_capacity(raw.len());
    let mut at: std::collections::HashMap<(String, String), usize> = std::collections::HashMap::new();
    for h in raw {
        match at.get(&(h.browser.clone(), h.url.clone())) {
            None => {
                at.insert((h.browser.clone(), h.url.clone()), out.len());
                out.push(h);
            }
            Some(&i) => {
                let m = &mut out[i];
                m.visit_count += h.visit_count;
                if h.last_visit > m.last_visit {
                    m.last_visit = h.last_visit;
                    m.title = h.title;
                }
                m.first_visit = match (m.first_visit, h.first_visit) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
                if m.description.is_none() {
                    m.description = h.description;
                }
            }
        }
    }
    out
}

/// Read every discovered history store and mirror the top pages into the index.
pub fn sync_history(conn: &Connection) -> Result<HistoryReport> {
    let mut raw = Vec::new();
    let mut report = HistoryReport::default();
    use crate::browsers::Engine;
    for profile in crate::browsers::profiles() {
        let (browser, path) = (profile.browser.clone(), profile.history_file());
        if !path.is_file() {
            continue;
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

    let raw = merge_profiles(raw);
    let tx = crate::db::write_tx(conn)?;
    let mut seen = Vec::new();
    for h in &raw {
        tx.execute(
            "INSERT INTO history(url, title, browser, visit_count, last_visit, first_visit, description)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(browser, url) DO UPDATE SET
                title = excluded.title,
                visit_count = excluded.visit_count,
                last_visit = excluded.last_visit,
                first_visit = excluded.first_visit,
                description = excluded.description",
            params![h.url, h.title, h.browser, h.visit_count, h.last_visit, h.first_visit, h.description],
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
        if crate::threads::stopping(crate::threads::Model::Text) {
            break; // a reload wants the model; its catch-up pass resumes here
        }
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
    Ok(done)
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
                browsers: Vec::new(),
                visit_count: r.get(4)?,
                last_visit: r.get(5)?,
                score: 0.0,
                fuzzy: false,
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
                    browsers: Vec::new(),
                    visit_count: r.get(4)?,
                    last_visit: r.get(5)?,
                    score: 0.0,
                    fuzzy: false,
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

    fn fake_db(name: &str, sql: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("magpie-hist-{name}-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&p);
        let c = Connection::open(&p).unwrap();
        c.execute_batch(sql).unwrap();
        drop(c);
        p
    }

    #[test]
    fn two_profiles_of_one_browser_add_up() {
        let r = |browser: &str, url: &str, title: &str, n: i64, last: Option<i64>, first: Option<i64>, d: Option<&str>| RawHistory {
            url: url.into(),
            title: title.into(),
            browser: browser.into(),
            visit_count: n,
            last_visit: last,
            first_visit: first,
            description: d.map(str::to_string),
        };
        let merged = merge_profiles(vec![
            r("chrome", "https://a/", "Old title", 3, Some(200), Some(50), None),
            r("chrome", "https://b/", "B", 1, Some(10), None, None),
            r("chrome", "https://a/", "New title", 4, Some(300), Some(80), Some("about a")),
            r("edge", "https://a/", "Edge A", 7, Some(100), None, None),
            r("chrome", "https://a/", "Third", 1, None, None, Some("ignored")),
        ]);
        assert_eq!(merged.len(), 3, "chrome a, chrome b, edge a");
        let a = merged.iter().find(|h| h.browser == "chrome" && h.url == "https://a/").unwrap();
        assert_eq!((a.visit_count, a.last_visit, a.first_visit), (8, Some(300), Some(50)));
        assert_eq!((a.title.as_str(), a.description.as_deref()), ("New title", Some("about a")), "the latest visit's title; the first summary found");
        let e = merged.iter().find(|h| h.browser == "edge").unwrap();
        assert_eq!(e.visit_count, 7, "another browser stays apart");
        assert_eq!(merged[1].url, "https://b/", "order of first appearance kept");
    }

    #[test]
    fn chromium_earliest_kept_visit() {
        // webkit micros for unix 1_600_000_000 and 1_700_000_000
        let (w1, w2) = ((1_600_000_000i64 + 11_644_473_600) * 1_000_000, (1_700_000_000i64 + 11_644_473_600) * 1_000_000);
        let p = fake_db(
            "chrome",
            &format!(
                "CREATE TABLE urls(id INTEGER PRIMARY KEY, url TEXT, title TEXT, visit_count INTEGER, last_visit_time INTEGER);
                 CREATE TABLE visits(id INTEGER PRIMARY KEY, url INTEGER, visit_time INTEGER);
                 INSERT INTO urls VALUES (1, 'https://a.example/', 'A', 2, {w2}), (2, 'https://b.example/', 'B', 1, {w2});
                 INSERT INTO visits(url, visit_time) VALUES (1, {w2}), (1, {w1});"
            ),
        );
        let mut out = Vec::new();
        parse_chromium("chrome", &p, &mut out).unwrap();
        let a = out.iter().find(|h| h.url == "https://a.example/").unwrap();
        assert_eq!((a.first_visit, a.last_visit, a.description.as_deref()), (Some(1_600_000_000), Some(1_700_000_000), None));
        let b = out.iter().find(|h| h.url == "https://b.example/").unwrap();
        assert_eq!(b.first_visit, None, "no visits kept");
        // a fork without `visits` still reads
        let p2 = fake_db(
            "fork",
            &format!("CREATE TABLE urls(id INTEGER PRIMARY KEY, url TEXT, title TEXT, visit_count INTEGER, last_visit_time INTEGER);
                      INSERT INTO urls VALUES (1, 'https://a.example/', 'A', 2, {w2});"),
        );
        let mut out2 = Vec::new();
        parse_chromium("fork", &p2, &mut out2).unwrap();
        assert_eq!((out2.len(), out2[0].first_visit), (1, None));
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_file(&p2);
    }

    #[test]
    fn firefox_first_visit_and_description() {
        let schema = |desc: bool| {
            format!(
                "CREATE TABLE moz_places(id INTEGER PRIMARY KEY, url TEXT, title TEXT, visit_count INTEGER, last_visit_date INTEGER{});
                 CREATE TABLE moz_historyvisits(id INTEGER PRIMARY KEY, place_id INTEGER, visit_date INTEGER);
                 INSERT INTO moz_historyvisits(place_id, visit_date) VALUES (1, 1700000000000000), (1, 1500000000000000);",
                if desc { ", description TEXT" } else { "" }
            )
        };
        let p = fake_db(
            "ff",
            &(schema(true)
                + "INSERT INTO moz_places VALUES (1, 'https://a.example/', 'A', 2, 1700000000000000, '  A page about things.  '),
                                                 (2, 'https://b.example/', 'B', 1, 1700000000000000, '   ');"),
        );
        let mut out = Vec::new();
        parse_firefox("firefox", &p, &mut out).unwrap();
        let a = out.iter().find(|h| h.url == "https://a.example/").unwrap();
        assert_eq!((a.first_visit, a.last_visit), (Some(1_500_000_000), Some(1_700_000_000)));
        assert_eq!(a.description.as_deref(), Some("A page about things."), "trimmed");
        assert_eq!(out.iter().find(|h| h.url == "https://b.example/").unwrap().description, None, "blank is none");
        // a profile from before Firefox 63: no description column
        let p2 = fake_db("ff-old", &(schema(false) + "INSERT INTO moz_places VALUES (1, 'https://a.example/', 'A', 2, 1700000000000000);"));
        let mut out2 = Vec::new();
        parse_firefox("firefox", &p2, &mut out2).unwrap();
        assert_eq!((out2[0].first_visit, out2[0].description.clone()), (Some(1_500_000_000), None));
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_file(&p2);
    }

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
