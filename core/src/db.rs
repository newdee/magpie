//! SQLite storage: repos, FTS5 index, embeddings, metadata.

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repo {
    pub id: i64,
    pub full_name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub topics: Vec<String>,
    pub stars: i64,
    pub html_url: String,
    pub homepage: Option<String>,
    pub archived: bool,
    pub fork: bool,
    pub starred_at: Option<String>,
    pub pushed_at: Option<String>,
}

/// Sync-relevant state of a stored repo.
pub struct RepoSyncState {
    pub id: i64,
    pub pushed_at: Option<String>,
    pub readme_pushed_at: Option<String>,
    pub readme_etag: Option<String>,
    pub has_readme: bool,
}

pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path).context("open sqlite db")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&conn)?;
    Ok(conn)
}

pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
    // additive migration for dbs created before thumbnails existed
    let _ = conn.execute("ALTER TABLE files ADD COLUMN thumb BLOB", []);
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS repos (
            id               INTEGER PRIMARY KEY,
            full_name        TEXT NOT NULL,
            description      TEXT,
            language         TEXT,
            topics           TEXT NOT NULL DEFAULT '[]',
            stars            INTEGER NOT NULL DEFAULT 0,
            html_url         TEXT NOT NULL,
            homepage         TEXT,
            archived         INTEGER NOT NULL DEFAULT 0,
            fork             INTEGER NOT NULL DEFAULT 0,
            starred_at       TEXT,
            pushed_at        TEXT,
            readme           TEXT,
            readme_etag      TEXT,
            readme_pushed_at TEXT
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS repo_fts USING fts5(
            full_name, description, topics, readme,
            content='repos', content_rowid='id', tokenize='unicode61'
        );

        CREATE TRIGGER IF NOT EXISTS repos_ai AFTER INSERT ON repos BEGIN
            INSERT INTO repo_fts(rowid, full_name, description, topics, readme)
            VALUES (new.id, new.full_name, new.description, new.topics, new.readme);
        END;
        CREATE TRIGGER IF NOT EXISTS repos_ad AFTER DELETE ON repos BEGIN
            INSERT INTO repo_fts(repo_fts, rowid, full_name, description, topics, readme)
            VALUES ('delete', old.id, old.full_name, old.description, old.topics, old.readme);
        END;
        CREATE TRIGGER IF NOT EXISTS repos_au AFTER UPDATE ON repos BEGIN
            INSERT INTO repo_fts(repo_fts, rowid, full_name, description, topics, readme)
            VALUES ('delete', old.id, old.full_name, old.description, old.topics, old.readme);
            INSERT INTO repo_fts(rowid, full_name, description, topics, readme)
            VALUES (new.id, new.full_name, new.description, new.topics, new.readme);
        END;

        -- pre-1.0 schema step: repo single-vector embeddings replaced by
        -- chunked ones; dropped vectors rebuild on the next embed pass
        DROP TABLE IF EXISTS embeddings;

        CREATE TABLE IF NOT EXISTS repo_chunks (
            repo_id   INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
            chunk_idx INTEGER NOT NULL,
            doc_hash  TEXT NOT NULL,
            dim       INTEGER NOT NULL,
            vec       BLOB NOT NULL,
            PRIMARY KEY (repo_id, chunk_idx)
        );

        CREATE INDEX IF NOT EXISTS idx_repos_starred_at ON repos(starred_at DESC);

        -- local file search: only explicitly added folders are ever scanned
        CREATE TABLE IF NOT EXISTS folders (
            id   INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT NOT NULL UNIQUE
        );

        CREATE TABLE IF NOT EXISTS files (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            folder_id INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
            path      TEXT NOT NULL UNIQUE,
            name      TEXT NOT NULL,
            ext       TEXT,
            size      INTEGER NOT NULL,
            mtime     INTEGER NOT NULL,
            content   TEXT,
            thumb     BLOB
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS files_fts USING fts5(
            name, path, content,
            content='files', content_rowid='id', tokenize='unicode61'
        );

        CREATE TRIGGER IF NOT EXISTS files_ai AFTER INSERT ON files BEGIN
            INSERT INTO files_fts(rowid, name, path, content)
            VALUES (new.id, new.name, new.path, new.content);
        END;
        CREATE TRIGGER IF NOT EXISTS files_ad AFTER DELETE ON files BEGIN
            INSERT INTO files_fts(files_fts, rowid, name, path, content)
            VALUES ('delete', old.id, old.name, old.path, old.content);
        END;
        CREATE TRIGGER IF NOT EXISTS files_au AFTER UPDATE ON files BEGIN
            INSERT INTO files_fts(files_fts, rowid, name, path, content)
            VALUES ('delete', old.id, old.name, old.path, old.content);
            INSERT INTO files_fts(rowid, name, path, content)
            VALUES (new.id, new.name, new.path, new.content);
        END;

        -- pre-1.0 schema step: single-vector-per-file replaced by chunks;
        -- dropped vectors rebuild automatically on the next index pass
        DROP TABLE IF EXISTS file_embeddings;

        CREATE TABLE IF NOT EXISTS file_chunks (
            file_id   INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            chunk_idx INTEGER NOT NULL,
            doc_hash  TEXT NOT NULL,
            dim       INTEGER NOT NULL,
            vec       BLOB NOT NULL,
            PRIMARY KEY (file_id, chunk_idx)
        );

        -- browser bookmarks, mirrored from local browser stores
        CREATE TABLE IF NOT EXISTS bookmarks (
            id       INTEGER PRIMARY KEY AUTOINCREMENT,
            url      TEXT NOT NULL,
            title    TEXT NOT NULL,
            folder   TEXT NOT NULL DEFAULT '',
            browser  TEXT NOT NULL,
            added_at INTEGER,
            UNIQUE(browser, url, folder, title)
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS bookmarks_fts USING fts5(
            title, url, folder,
            content='bookmarks', content_rowid='id', tokenize='unicode61'
        );

        CREATE TRIGGER IF NOT EXISTS bookmarks_ai AFTER INSERT ON bookmarks BEGIN
            INSERT INTO bookmarks_fts(rowid, title, url, folder)
            VALUES (new.id, new.title, new.url, new.folder);
        END;
        CREATE TRIGGER IF NOT EXISTS bookmarks_ad AFTER DELETE ON bookmarks BEGIN
            INSERT INTO bookmarks_fts(bookmarks_fts, rowid, title, url, folder)
            VALUES ('delete', old.id, old.title, old.url, old.folder);
        END;
        CREATE TRIGGER IF NOT EXISTS bookmarks_au AFTER UPDATE ON bookmarks BEGIN
            INSERT INTO bookmarks_fts(bookmarks_fts, rowid, title, url, folder)
            VALUES ('delete', old.id, old.title, old.url, old.folder);
            INSERT INTO bookmarks_fts(rowid, title, url, folder)
            VALUES (new.id, new.title, new.url, new.folder);
        END;

        CREATE TABLE IF NOT EXISTS bookmark_vecs (
            bookmark_id INTEGER PRIMARY KEY REFERENCES bookmarks(id) ON DELETE CASCADE,
            doc_hash    TEXT NOT NULL,
            dim         INTEGER NOT NULL,
            vec         BLOB NOT NULL
        );

        -- SigLIP space for images; separate from the e5 text space by design.
        -- dim = 0 marks "tried and failed" (corrupt file), so it is not retried.
        CREATE TABLE IF NOT EXISTS image_embeddings (
            file_id  INTEGER PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE,
            doc_hash TEXT NOT NULL,
            dim      INTEGER NOT NULL,
            vec      BLOB NOT NULL
        );
        "#,
    )
    .context("run migrations")?;
    Ok(())
}

// ---------- meta ----------

pub fn meta_get(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?)
}

pub fn meta_set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO meta(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

// ---------- repos ----------

pub fn upsert_repo(conn: &Connection, repo: &Repo) -> Result<()> {
    let topics = serde_json::to_string(&repo.topics)?;
    conn.execute(
        r#"
        INSERT INTO repos (id, full_name, description, language, topics, stars,
                           html_url, homepage, archived, fork, starred_at, pushed_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        ON CONFLICT(id) DO UPDATE SET
            full_name = excluded.full_name,
            description = excluded.description,
            language = excluded.language,
            topics = excluded.topics,
            stars = excluded.stars,
            html_url = excluded.html_url,
            homepage = excluded.homepage,
            archived = excluded.archived,
            fork = excluded.fork,
            starred_at = excluded.starred_at,
            pushed_at = excluded.pushed_at
        "#,
        params![
            repo.id,
            repo.full_name,
            repo.description,
            repo.language,
            topics,
            repo.stars,
            repo.html_url,
            repo.homepage,
            repo.archived as i64,
            repo.fork as i64,
            repo.starred_at,
            repo.pushed_at,
        ],
    )?;
    Ok(())
}

/// Delete repos whose id is NOT in `keep` (unstarred). Returns number removed.
pub fn delete_repos_not_in(conn: &mut Connection, keep: &[i64]) -> Result<usize> {
    let tx = conn.transaction()?;
    tx.execute("CREATE TEMP TABLE IF NOT EXISTS keep_ids (id INTEGER PRIMARY KEY)", [])?;
    tx.execute("DELETE FROM keep_ids", [])?;
    {
        let mut stmt = tx.prepare("INSERT OR IGNORE INTO keep_ids(id) VALUES (?1)")?;
        for id in keep {
            stmt.execute([id])?;
        }
    }
    let removed = tx.execute(
        "DELETE FROM repos WHERE id NOT IN (SELECT id FROM keep_ids)",
        [],
    )?;
    tx.execute("DELETE FROM keep_ids", [])?;
    tx.commit()?;
    Ok(removed)
}

pub fn repo_sync_states(conn: &Connection) -> Result<Vec<RepoSyncState>> {
    let mut stmt = conn.prepare(
        "SELECT id, pushed_at, readme_pushed_at, readme_etag, readme IS NOT NULL FROM repos",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(RepoSyncState {
                id: r.get(0)?,
                pushed_at: r.get(1)?,
                readme_pushed_at: r.get(2)?,
                readme_etag: r.get(3)?,
                has_readme: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn set_readme(
    conn: &Connection,
    repo_id: i64,
    readme: Option<&str>,
    etag: Option<&str>,
    pushed_at: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE repos SET readme = ?2, readme_etag = ?3, readme_pushed_at = ?4 WHERE id = ?1",
        params![repo_id, readme, etag, pushed_at],
    )?;
    Ok(())
}

/// Mark readme as still-fresh (etag 304) without touching content.
pub fn touch_readme(conn: &Connection, repo_id: i64, pushed_at: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE repos SET readme_pushed_at = ?2 WHERE id = ?1",
        params![repo_id, pushed_at],
    )?;
    Ok(())
}

fn row_to_repo(r: &rusqlite::Row<'_>) -> rusqlite::Result<Repo> {
    let topics_json: String = r.get(4)?;
    Ok(Repo {
        id: r.get(0)?,
        full_name: r.get(1)?,
        description: r.get(2)?,
        language: r.get(3)?,
        topics: serde_json::from_str(&topics_json).unwrap_or_default(),
        stars: r.get(5)?,
        html_url: r.get(6)?,
        homepage: r.get(7)?,
        archived: r.get::<_, i64>(8)? != 0,
        fork: r.get::<_, i64>(9)? != 0,
        starred_at: r.get(10)?,
        pushed_at: r.get(11)?,
    })
}

const REPO_COLS: &str = "id, full_name, description, language, topics, stars, \
                         html_url, homepage, archived, fork, starred_at, pushed_at";

/// Fetch repos by id, preserving the order of `ids`.
pub fn repos_by_ids(conn: &Connection, ids: &[i64]) -> Result<Vec<Repo>> {
    let mut out = Vec::with_capacity(ids.len());
    let mut stmt = conn.prepare(&format!("SELECT {REPO_COLS} FROM repos WHERE id = ?1"))?;
    for id in ids {
        if let Some(repo) = stmt.query_row([id], row_to_repo).optional()? {
            out.push(repo);
        }
    }
    Ok(out)
}

pub fn recent_repos(conn: &Connection, limit: usize) -> Result<Vec<Repo>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {REPO_COLS} FROM repos ORDER BY starred_at DESC, id DESC LIMIT ?1"
    ))?;
    let rows = stmt
        .query_map([limit as i64], row_to_repo)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn repo_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM repos", [], |r| r.get(0))?)
}

/// (id, composed-doc source fields) for docs whose embedding is missing or stale.
pub struct EmbedCandidate {
    pub id: i64,
    pub full_name: String,
    pub description: Option<String>,
    pub topics: Vec<String>,
    pub language: Option<String>,
    pub readme: Option<String>,
}

pub fn embed_candidates(conn: &Connection) -> Result<Vec<EmbedCandidate>> {
    let mut stmt = conn.prepare(
        "SELECT r.id, r.full_name, r.description, r.topics, r.language, r.readme
         FROM repos r",
    )?;
    let rows = stmt
        .query_map([], |r| {
            let topics_json: String = r.get(3)?;
            Ok(EmbedCandidate {
                id: r.get(0)?,
                full_name: r.get(1)?,
                description: r.get(2)?,
                topics: serde_json::from_str(&topics_json).unwrap_or_default(),
                language: r.get(4)?,
                readme: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// repo_id -> per-repo doc_hash (identical across a repo's chunks).
pub fn repo_chunk_hashes(conn: &Connection) -> Result<std::collections::HashMap<i64, String>> {
    let mut stmt = conn.prepare("SELECT DISTINCT repo_id, doc_hash FROM repo_chunks")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<std::collections::HashMap<_, _>>>()?;
    Ok(rows)
}

pub fn clear_repo_chunks(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM repo_chunks", [])?;
    Ok(())
}

/// Replace a repo's chunk vectors atomically (a crash must never leave the
/// new hash with a partial chunk set).
pub fn put_repo_chunks(
    conn: &Connection,
    repo_id: i64,
    doc_hash: &str,
    vecs: &[Vec<f32>],
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM repo_chunks WHERE repo_id = ?1", [repo_id])?;
    for (idx, vec) in vecs.iter().enumerate() {
        let bytes: Vec<u8> = vec.iter().flat_map(|f| f.to_le_bytes()).collect();
        tx.execute(
            "INSERT INTO repo_chunks(repo_id, chunk_idx, doc_hash, dim, vec)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![repo_id, idx as i64, doc_hash, vec.len() as i64, bytes],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// All repo chunk vectors as (repo_id, vec); repo_id repeats across chunks.
pub fn all_repo_chunk_embeddings(conn: &Connection) -> Result<Vec<(i64, Vec<f32>)>> {
    let mut stmt = conn.prepare("SELECT repo_id, dim, vec FROM repo_chunks")?;
    let rows = stmt
        .query_map([], |r| {
            let id: i64 = r.get(0)?;
            let dim: i64 = r.get(1)?;
            let bytes: Vec<u8> = r.get(2)?;
            Ok((id, dim as usize, bytes))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut out = Vec::with_capacity(rows.len());
    for (id, dim, bytes) in rows {
        if bytes.len() != dim * 4 {
            continue; // corrupt row; skip rather than crash
        }
        let (chunks, _) = bytes.as_chunks::<4>();
        let vec: Vec<f32> = chunks.iter().map(|c| f32::from_le_bytes(*c)).collect();
        out.push((id, vec));
    }
    Ok(out)
}

/// FTS5 BM25 search. Returns repo ids, best match first.
pub fn fts_search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<i64>> {
    let fts_query = build_fts_query(query);
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT rowid FROM repo_fts WHERE repo_fts MATCH ?1
         ORDER BY bm25(repo_fts, 8.0, 4.0, 4.0, 1.0) LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![fts_query, limit as i64], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Sanitize free text into an FTS5 query: each token quoted, prefix-matched, AND-joined.
pub fn build_fts_query(input: &str) -> String {
    input
        .split_whitespace()
        .map(|tok| tok.replace('"', ""))
        .filter(|tok| !tok.is_empty())
        .map(|tok| format!("\"{tok}\"*"))
        .collect::<Vec<_>>()
        .join(" AND ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: i64, name: &str, desc: &str) -> Repo {
        Repo {
            id,
            full_name: name.into(),
            description: Some(desc.into()),
            language: Some("Rust".into()),
            topics: vec!["cli".into()],
            stars: 42,
            html_url: format!("https://github.com/{name}"),
            homepage: None,
            archived: false,
            fork: false,
            starred_at: Some(format!("2026-01-{:02}T00:00:00Z", id)),
            pushed_at: Some("2026-02-01T00:00:00Z".into()),
        }
    }

    #[test]
    fn upsert_fts_and_delete_roundtrip() {
        let mut conn = open_in_memory().unwrap();
        upsert_repo(&conn, &sample(1, "alice/scraper", "web scraping framework")).unwrap();
        upsert_repo(&conn, &sample(2, "bob/parser", "json parser")).unwrap();
        assert_eq!(repo_count(&conn).unwrap(), 2);

        let hits = fts_search(&conn, "scraping", 10).unwrap();
        assert_eq!(hits, vec![1]);

        // update flows into FTS via trigger
        upsert_repo(&conn, &sample(2, "bob/parser", "yaml parser")).unwrap();
        assert_eq!(fts_search(&conn, "yaml", 10).unwrap(), vec![2]);
        assert!(fts_search(&conn, "json", 10).unwrap().is_empty());

        // unstar removes from repos + FTS + embeddings
        put_repo_chunks(&conn, 1, "h", &[vec![0.1, 0.2]]).unwrap();
        let removed = delete_repos_not_in(&mut conn, &[2]).unwrap();
        assert_eq!(removed, 1);
        assert!(fts_search(&conn, "scraping", 10).unwrap().is_empty());
        assert!(all_repo_chunk_embeddings(&conn).unwrap().is_empty());
    }

    #[test]
    fn embedding_blob_roundtrip() {
        let conn = open_in_memory().unwrap();
        upsert_repo(&conn, &sample(7, "x/y", "d")).unwrap();
        let v = vec![0.25f32, -1.5, 3.75];
        let v2 = vec![9.0f32, 8.0, 7.0];
        put_repo_chunks(&conn, 7, "abc", &[v.clone(), v2.clone()]).unwrap();
        let all = all_repo_chunk_embeddings(&conn).unwrap();
        assert_eq!(all.len(), 2, "two chunks stored");
        assert_eq!(all[0], (7, v.clone())); // byte-exact roundtrip
        assert_eq!(all[1], (7, v2));
        assert_eq!(repo_chunk_hashes(&conn).unwrap().get(&7).map(String::as_str), Some("abc"));
        // replacing swaps the whole chunk set atomically
        put_repo_chunks(&conn, 7, "def", std::slice::from_ref(&v)).unwrap();
        assert_eq!(all_repo_chunk_embeddings(&conn).unwrap().len(), 1);
    }

    #[test]
    fn fts_query_sanitized() {
        assert_eq!(build_fts_query("hello world"), "\"hello\"* AND \"world\"*");
        assert_eq!(build_fts_query("a \"b\" NOT"), "\"a\"* AND \"b\"* AND \"NOT\"*");
        assert_eq!(build_fts_query("  "), "");
        // FTS operators are neutralized inside quotes
        let conn = open_in_memory().unwrap();
        upsert_repo(&conn, &sample(1, "a/b", "c")).unwrap();
        assert!(fts_search(&conn, "AND OR ( ) *", 5).is_ok());
    }

    #[test]
    fn recent_ordering() {
        let conn = open_in_memory().unwrap();
        upsert_repo(&conn, &sample(1, "a/a", "x")).unwrap();
        upsert_repo(&conn, &sample(3, "c/c", "x")).unwrap();
        upsert_repo(&conn, &sample(2, "b/b", "x")).unwrap();
        let recents = recent_repos(&conn, 10).unwrap();
        let ids: Vec<i64> = recents.iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![3, 2, 1]);
    }
}
