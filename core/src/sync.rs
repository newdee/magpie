//! Sync orchestration: starred list -> upsert/delete -> READMEs -> embeddings.
//!
//! `sync()` is async (network + db). `embed_pending()` is a blocking CPU-bound
//! pass, meant for `spawn_blocking`; run it after sync and after model init.

use anyhow::Result;
use futures::stream::{self, StreamExt};
use rusqlite::Connection;
use serde::Serialize;

use crate::db::{self};
use crate::embed::{self, Embedder};
use crate::github::{GithubClient, ReadmeResult};

/// Identity of the embedding space. Changing the model MUST change this string;
/// stored vectors are invalidated automatically when it differs.
pub const EMBED_MODEL_ID: &str = "multilingual-e5-small";

const README_CONCURRENCY: usize = 8;
const EMBED_BATCH: usize = 16;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum Progress {
    Listing { page: usize, total: usize },
    Readmes { done: usize, total: usize },
    Embedding { done: usize, total: usize },
}

#[derive(Debug, Default, Serialize)]
pub struct SyncReport {
    pub total: usize,
    pub removed: usize,
    pub readmes_fetched: usize,
    pub readme_errors: usize,
}

/// Wipe stored vectors if they came from a different model.
pub fn ensure_embed_model(conn: &Connection) -> Result<()> {
    let stored = db::meta_get(conn, "embed_model")?;
    if stored.as_deref() != Some(EMBED_MODEL_ID) {
        db::clear_embeddings(conn)?;
        db::meta_set(conn, "embed_model", EMBED_MODEL_ID)?;
    }
    Ok(())
}

/// Network + db stages. Does NOT embed; call `embed_pending` afterwards.
pub async fn sync(
    conn: &mut Connection,
    client: &GithubClient,
    progress: impl Fn(Progress) + Send,
) -> Result<SyncReport> {
    ensure_embed_model(conn)?;
    let mut report = SyncReport::default();

    // 1. full starred listing (10 pages per 1k stars — cheap, and the only
    //    reliable way to detect unstars)
    let starred = client
        .list_starred(|page, total| progress(Progress::Listing { page, total }))
        .await?;
    report.total = starred.len();

    // 2. upsert + prune
    for (_, repo) in &starred {
        db::upsert_repo(conn, repo)?;
    }
    let keep: Vec<i64> = starred.iter().map(|(_, r)| r.id).collect();
    report.removed = db::delete_repos_not_in(conn, &keep)?;

    // 3. READMEs for new or pushed-since-last-fetch repos
    let stale = {
        let states = db::repo_sync_states(conn)?;
        let repos: Vec<&crate::db::Repo> = starred.iter().map(|(_, r)| r).collect();
        stale_readme_targets(&states, &repos)
    };

    let total_stale = stale.len();
    progress(Progress::Readmes { done: 0, total: total_stale });
    let mut fetched = stream::iter(stale.into_iter().map(|(id, name, etag, marker)| async move {
        let res = client.fetch_readme(&name, etag.as_deref()).await;
        (id, marker, res)
    }))
    .buffer_unordered(README_CONCURRENCY);

    let mut done = 0usize;
    while let Some((id, marker, res)) = fetched.next().await {
        match res {
            Ok(ReadmeResult::Fresh(text, etag)) => {
                // cap stored size: FTS + embedding only ever use the head
                let capped: String = text.chars().take(64 * 1024).collect();
                db::set_readme(conn, id, Some(&capped), etag.as_deref(), Some(&marker))?;
                report.readmes_fetched += 1;
            }
            Ok(ReadmeResult::NotModified) => {
                db::touch_readme(conn, id, Some(&marker))?;
            }
            Ok(ReadmeResult::Missing) => {
                db::set_readme(conn, id, None, None, Some(&marker))?;
            }
            Err(_) => {
                // transient failure: leave state untouched so the next sync retries
                report.readme_errors += 1;
            }
        }
        done += 1;
        if done.is_multiple_of(10) || done == total_stale {
            progress(Progress::Readmes { done, total: total_stale });
        }
    }

    db::meta_set(conn, "last_sync", &now_unix())?;
    Ok(report)
}

/// Decide which repos need a README request this sync.
///
/// `readme_pushed_at` doubles as a "fetch attempted" marker: it stores the
/// repo's pushed_at at fetch time (or "" when pushed_at is null), so a Missing
/// (404) result also counts as fresh and is not re-requested every sync.
/// Returns (repo_id, full_name, cached_etag, marker_to_store).
fn stale_readme_targets(
    states: &[db::RepoSyncState],
    starred: &[&db::Repo],
) -> Vec<(i64, String, Option<String>, String)> {
    let by_id: std::collections::HashMap<i64, &db::RepoSyncState> =
        states.iter().map(|s| (s.id, s)).collect();
    starred
        .iter()
        .filter_map(|r| {
            let s = by_id.get(&r.id)?;
            let marker = r.pushed_at.clone().unwrap_or_default();
            if s.readme_pushed_at.as_deref() == Some(marker.as_str()) {
                None
            } else {
                Some((r.id, r.full_name.clone(), s.readme_etag.clone(), marker))
            }
        })
        .collect()
}

/// Embed every repo whose composed doc changed. Blocking; run via spawn_blocking.
/// Returns the number of repos (re-)embedded.
pub fn embed_pending(
    conn: &Connection,
    embedder: &mut Embedder,
    progress: impl Fn(Progress),
) -> Result<usize> {
    ensure_embed_model(conn)?;
    let hashes = db::all_embedding_hashes(conn)?;
    let pending: Vec<(i64, String, String)> = db::embed_candidates(conn)?
        .iter()
        .filter_map(|c| {
            let doc = embed::compose_doc(c);
            let hash = embed::doc_hash(&doc);
            if hashes.get(&c.id) == Some(&hash) {
                None
            } else {
                Some((c.id, doc, hash))
            }
        })
        .collect();

    let total = pending.len();
    progress(Progress::Embedding { done: 0, total });
    let mut done = 0usize;
    for chunk in pending.chunks(EMBED_BATCH) {
        let docs: Vec<String> = chunk.iter().map(|(_, d, _)| d.clone()).collect();
        let vecs = embedder.embed_passages(&docs)?;
        for ((id, _, hash), vec) in chunk.iter().zip(vecs) {
            db::put_embedding(conn, *id, hash, &vec)?;
        }
        done += chunk.len();
        progress(Progress::Embedding { done, total });
    }
    Ok(total)
}

fn now_unix() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(id: i64, readme_pushed_at: Option<&str>, has_readme: bool) -> db::RepoSyncState {
        db::RepoSyncState {
            id,
            pushed_at: None,
            readme_pushed_at: readme_pushed_at.map(str::to_string),
            readme_etag: Some(format!("etag-{id}")),
            has_readme,
        }
    }

    fn repo(id: i64, pushed_at: Option<&str>) -> db::Repo {
        db::Repo {
            id,
            full_name: format!("o/r{id}"),
            description: None,
            language: None,
            topics: vec![],
            stars: 0,
            html_url: String::new(),
            homepage: None,
            archived: false,
            fork: false,
            starred_at: None,
            pushed_at: pushed_at.map(str::to_string),
        }
    }

    #[test]
    fn stale_selection() {
        let states = vec![
            state(1, None, false),            // never fetched -> stale
            state(2, Some("t1"), true),       // fetched at t1, repo still t1 -> fresh
            state(3, Some("t1"), true),       // fetched at t1, repo pushed t2 -> stale
            state(4, Some(""), false),        // 404'd empty repo, pushed_at null -> fresh
            state(5, Some("t1"), false),      // 404'd at t1, repo pushed t2 -> retry
        ];
        let repos = [
            repo(1, Some("t1")),
            repo(2, Some("t1")),
            repo(3, Some("t2")),
            repo(4, None),
            repo(5, Some("t2")),
            repo(6, Some("t9")), // no state row (freshly inserted this call) -> skipped defensively
        ];
        let refs: Vec<&db::Repo> = repos.iter().collect();
        let stale = stale_readme_targets(&states, &refs);
        let ids: Vec<i64> = stale.iter().map(|t| t.0).collect();
        assert_eq!(ids, vec![1, 3, 5]);
        // marker carries the new pushed_at, etag carried over for conditional GET
        let t3 = stale.iter().find(|t| t.0 == 3).unwrap();
        assert_eq!(t3.3, "t2");
        assert_eq!(t3.2.as_deref(), Some("etag-3"));
    }

    #[test]
    fn model_change_invalidates_vectors() {
        let conn = db::open_in_memory().unwrap();
        db::upsert_repo(
            &conn,
            &db::Repo {
                id: 1,
                full_name: "a/b".into(),
                description: None,
                language: None,
                topics: vec![],
                stars: 0,
                html_url: String::new(),
                homepage: None,
                archived: false,
                fork: false,
                starred_at: None,
                pushed_at: None,
            },
        )
        .unwrap();
        db::put_embedding(&conn, 1, "h", &[1.0]).unwrap();
        db::meta_set(&conn, "embed_model", "old-model").unwrap();

        ensure_embed_model(&conn).unwrap();
        assert!(db::all_embeddings(&conn).unwrap().is_empty());
        assert_eq!(
            db::meta_get(&conn, "embed_model").unwrap().as_deref(),
            Some(EMBED_MODEL_ID)
        );

        // second call is a no-op (vectors survive)
        db::put_embedding(&conn, 1, "h", &[1.0]).unwrap();
        ensure_embed_model(&conn).unwrap();
        assert_eq!(db::all_embeddings(&conn).unwrap().len(), 1);
    }
}
