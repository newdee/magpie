//! Hybrid retrieval: FTS5 BM25 + vector cosine, fused with Reciprocal Rank Fusion.

use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;

use crate::db::{self, Repo};
use crate::embed::dot;

const RRF_K: f32 = 60.0;
const CANDIDATES_PER_LIST: usize = 50;
/// Cosine similarity contributes a small additive term on top of RRF: rank
/// structure dominates, similarity magnitude breaks ties and nudges close calls.
const VEC_SIM_WEIGHT: f32 = 0.005;

#[derive(Debug, Serialize)]
pub struct SearchResult {
    #[serde(flatten)]
    pub repo: Repo,
    pub score: f32,
}

/// Reciprocal Rank Fusion over ranked id lists. Input lists are best-first.
pub fn rrf_fuse(lists: &[Vec<i64>]) -> Vec<(i64, f32)> {
    use std::collections::HashMap;
    let mut scores: HashMap<i64, f32> = HashMap::new();
    for list in lists {
        for (rank, id) in list.iter().enumerate() {
            *scores.entry(*id).or_default() += 1.0 / (RRF_K + rank as f32 + 1.0);
        }
    }
    let mut out: Vec<(i64, f32)> = scores.into_iter().collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    out
}

/// All embedding vectors held resident in memory. Reload after any embed or
/// sync pass; per-query loading from SQLite does not survive chunk-scale
/// corpora (tens of MB per keystroke).
pub struct VectorStore {
    /// (repo_id, chunk vector); repo_id repeats across a repo's README chunks.
    pub repos: Vec<(i64, Vec<f32>)>,
    /// (file_id, chunk vector); file_id repeats across a file's chunks.
    pub file_chunks: Vec<(i64, Vec<f32>)>,
    pub images: Vec<(i64, Vec<f32>)>,
}

impl VectorStore {
    pub fn load(conn: &Connection) -> Result<Self> {
        Ok(Self {
            repos: db::all_repo_chunk_embeddings(conn)?,
            file_chunks: crate::files::all_file_chunk_embeddings(conn)?,
            images: crate::files::all_image_embeddings(conn)?,
        })
    }

    pub fn empty() -> Self {
        Self {
            repos: Vec::new(),
            file_chunks: Vec::new(),
            images: Vec::new(),
        }
    }
}

/// Brute-force top-k over embeddings. Returns (id, similarity), best first.
pub fn top_similar(all: &[(i64, Vec<f32>)], qvec: &[f32], limit: usize) -> Vec<(i64, f32)> {
    let mut scored: Vec<(i64, f32)> = all
        .iter()
        .filter(|(_, v)| v.len() == qvec.len())
        .map(|(id, v)| (*id, dot(qvec, v)))
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    scored.truncate(limit);
    scored
}

/// Like top_similar, but ids repeat (chunks): a file scores as its best chunk.
pub fn top_similar_grouped(
    all: &[(i64, Vec<f32>)],
    qvec: &[f32],
    limit: usize,
) -> Vec<(i64, f32)> {
    let mut best: std::collections::HashMap<i64, f32> = std::collections::HashMap::new();
    for (id, v) in all {
        if v.len() != qvec.len() {
            continue;
        }
        let sim = dot(qvec, v);
        let entry = best.entry(*id).or_insert(f32::NEG_INFINITY);
        if sim > *entry {
            *entry = sim;
        }
    }
    let mut scored: Vec<(i64, f32)> = best.into_iter().collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    scored.truncate(limit);
    scored
}

/// Fuse an FTS ranking with any number of vector candidate lists: RRF over
/// ranks plus a small summed similarity term. Returns (id, score), best first.
pub fn rank_hybrid(fts: Vec<i64>, vecs: Vec<Vec<(i64, f32)>>) -> Vec<(i64, f32)> {
    let mut lists = vec![fts];
    let mut sims: std::collections::HashMap<i64, f32> = std::collections::HashMap::new();
    for vs in vecs {
        for (id, sim) in &vs {
            *sims.entry(*id).or_default() += sim;
        }
        lists.push(vs.into_iter().map(|(id, _)| id).collect());
    }
    let mut fused = rrf_fuse(&lists);
    for (id, score) in fused.iter_mut() {
        *score += VEC_SIM_WEIGHT * sims.get(id).copied().unwrap_or(0.0);
    }
    fused.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    fused
}

/// Ordering applied to matched repos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepoSort {
    /// Hybrid retrieval score (default).
    Relevance,
    /// Most recently starred first.
    Starred,
    /// Highest star count first.
    Stars,
}

/// Hybrid search. `qvec` is None when the embedding model is not ready — FTS-only
/// then. Non-relevance sorts reorder the matched candidate set.
pub fn search(
    conn: &Connection,
    store: &VectorStore,
    query: &str,
    qvec: Option<&[f32]>,
    sort: RepoSort,
    limit: usize,
) -> Result<Vec<SearchResult>> {
    // sorting only applies to search matches; the empty-query browse view is
    // always the recently-starred list
    if query.trim().is_empty() {
        return Ok(db::recent_repos(conn, limit)?
            .into_iter()
            .map(|repo| SearchResult { repo, score: 0.0 })
            .collect());
    }

    let fts = db::fts_search(conn, query, CANDIDATES_PER_LIST)?;
    let vecs = match qvec {
        // a repo ranks by its best README chunk
        Some(qvec) => vec![top_similar_grouped(&store.repos, qvec, CANDIDATES_PER_LIST)],
        None => vec![],
    };
    let fused = rank_hybrid(fts, vecs);
    let scores: std::collections::HashMap<i64, f32> = fused.iter().copied().collect();
    // alternate sorts reorder exactly the top-`limit` relevant matches — never
    // a wider pool, so a famous-but-barely-related repo cannot crowd in
    let ids: Vec<i64> = fused.iter().take(limit).map(|(id, _)| *id).collect();
    let mut results: Vec<SearchResult> = db::repos_by_ids(conn, &ids)?
        .into_iter()
        .map(|repo| {
            let score = scores.get(&repo.id).copied().unwrap_or(0.0);
            SearchResult { repo, score }
        })
        .collect();
    results.truncate(limit);
    match sort {
        RepoSort::Relevance => {}
        RepoSort::Starred => results.sort_by(|a, b| {
            b.repo
                .starred_at
                .cmp(&a.repo.starred_at)
                .then(a.repo.id.cmp(&b.repo.id))
        }),
        RepoSort::Stars => results.sort_by(|a, b| {
            b.repo
                .stars
                .cmp(&a.repo.stars)
                .then(a.repo.id.cmp(&b.repo.id))
        }),
    }
    Ok(results)
}

/// What a local search may return.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LocalScope {
    /// Text files and images mixed (default).
    All,
    /// Text files only.
    Text,
    /// Images only.
    Images,
}

/// Hybrid search over local files: filename/content FTS + e5 text vectors +
/// SigLIP image vectors, all fused by rank. The two vector spaces cover
/// disjoint id sets (text files vs images), so their similarities never mix.
/// `scope` restricts which kinds of files come back.
pub fn search_files(
    conn: &Connection,
    store: &VectorStore,
    query: &str,
    qvec: Option<&[f32]>,
    image_qvec: Option<&[f32]>,
    scope: LocalScope,
    limit: usize,
) -> Result<Vec<crate::files::FileHit>> {
    use crate::files;
    if query.trim().is_empty() {
        return files::recent_files(conn, limit);
    }
    let fts_hits = files::files_fts_search(conn, query, CANDIDATES_PER_LIST)?;
    let snippets: std::collections::HashMap<i64, String> = fts_hits
        .iter()
        .filter(|(_, s)| s.contains('\u{1}'))
        .map(|(id, s)| (*id, s.clone()))
        .collect();
    let fts: Vec<i64> = fts_hits.into_iter().map(|(id, _)| id).collect();
    let mut vecs = Vec::new();
    if scope != LocalScope::Images {
        if let Some(qvec) = qvec {
            // a file ranks by its best chunk
            vecs.push(top_similar_grouped(
                &store.file_chunks,
                qvec,
                CANDIDATES_PER_LIST,
            ));
        }
    }
    if scope != LocalScope::Text {
        if let Some(image_qvec) = image_qvec {
            vecs.push(top_similar(&store.images, image_qvec, CANDIDATES_PER_LIST));
        }
    }
    let fused = rank_hybrid(fts, vecs);
    let scores: std::collections::HashMap<i64, f32> = fused.iter().copied().collect();
    // fetch the full candidate set, filter by scope, then truncate
    let ids: Vec<i64> = fused.iter().map(|(id, _)| *id).collect();
    let mut hits = files::files_by_ids(conn, &ids, &scores)?;
    match scope {
        LocalScope::All => {}
        LocalScope::Text => hits.retain(|h| !files::is_image_ext(h.ext.as_deref())),
        LocalScope::Images => hits.retain(|h| files::is_image_ext(h.ext.as_deref())),
    }
    hits.truncate(limit);
    for h in &mut hits {
        h.snippet = snippets.get(&h.id).cloned();
    }
    Ok(hits)
}

/// Image-to-image search: a query image's SigLIP vector against all indexed
/// image vectors. Scores are raw cosine similarities.
pub fn search_images(
    conn: &Connection,
    store: &VectorStore,
    image_qvec: &[f32],
    limit: usize,
) -> Result<Vec<crate::files::FileHit>> {
    let scored = top_similar(&store.images, image_qvec, limit);
    let scores: std::collections::HashMap<i64, f32> = scored.iter().copied().collect();
    let ids: Vec<i64> = scored.into_iter().map(|(id, _)| id).collect();
    crate::files::files_by_ids(conn, &ids, &scores)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_prefers_items_in_both_lists() {
        // 2 appears in both lists (ranks 1 and 0) → must beat 1 and 3.
        let fused = rrf_fuse(&[vec![1, 2], vec![2, 3]]);
        assert_eq!(fused[0].0, 2);
        let ids: Vec<i64> = fused.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids.len(), 3);
        // deterministic tie-break: 1 (rank 0) vs 3 (rank 1) → 1 scores higher
        assert_eq!(ids, vec![2, 1, 3]);
    }

    #[test]
    fn rrf_empty() {
        assert!(rrf_fuse(&[]).is_empty());
        assert!(rrf_fuse(&[vec![], vec![]]).is_empty());
    }

    #[test]
    fn vector_search_orders_by_similarity() {
        let conn = crate::db::open_in_memory().unwrap();
        for (id, name) in [(1, "a/a"), (2, "b/b"), (3, "c/c")] {
            crate::db::upsert_repo(
                &conn,
                &crate::db::Repo {
                    id,
                    full_name: name.into(),
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
        }
        crate::db::put_repo_chunks(&conn, 1, "h", &[vec![1.0, 0.0]]).unwrap();
        crate::db::put_repo_chunks(&conn, 2, "h", &[vec![0.0, 1.0]]).unwrap();
        crate::db::put_repo_chunks(&conn, 3, "h", &[vec![0.7, 0.7]]).unwrap();
        let store = VectorStore::load(&conn).unwrap();
        let hits: Vec<i64> = top_similar(&store.repos, &[1.0, 0.0], 3)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(hits, vec![1, 3, 2]);
        // dimension-mismatched query vectors match nothing rather than panicking
        assert!(top_similar(&store.repos, &[1.0, 0.0, 0.0], 3).is_empty());

        // grouped: a file scores as its best chunk
        let chunks = vec![
            (10i64, vec![0.2, 0.0]),
            (10i64, vec![0.9, 0.0]),
            (11i64, vec![0.5, 0.0]),
        ];
        let grouped = top_similar_grouped(&chunks, &[1.0, 0.0], 5);
        assert_eq!(grouped[0], (10, 0.9));
        assert_eq!(grouped[1], (11, 0.5));
    }

    #[test]
    fn vector_input_changes_hybrid_ranking() {
        let conn = crate::db::open_in_memory().unwrap();
        for (id, name, desc) in [(1, "a/cli", "terminal tool"), (2, "b/gui", "desktop tool")] {
            crate::db::upsert_repo(
                &conn,
                &crate::db::Repo {
                    id,
                    full_name: name.into(),
                    description: Some(desc.into()),
                    language: None,
                    topics: vec![],
                    stars: 0,
                    html_url: String::new(),
                    homepage: None,
                    archived: false,
                    fork: false,
                    starred_at: Some(format!("2026-01-0{id}T00:00:00Z")),
                    pushed_at: None,
                },
            )
            .unwrap();
        }
        crate::db::put_repo_chunks(&conn, 1, "h", &[vec![1.0, 0.0]]).unwrap();
        crate::db::put_repo_chunks(&conn, 2, "h", &[vec![0.0, 1.0]]).unwrap();

        let store = VectorStore::load(&conn).unwrap();
        // both match FTS equally on "tool"; the query vector must break the tie
        let towards_1 =
            search(&conn, &store, "tool", Some(&[1.0, 0.0]), RepoSort::Relevance, 10).unwrap();
        let towards_2 =
            search(&conn, &store, "tool", Some(&[0.0, 1.0]), RepoSort::Relevance, 10).unwrap();
        assert_eq!(towards_1.len(), 2);
        assert_eq!(towards_1[0].repo.id, 1, "qvec [1,0] must rank repo 1 first");
        assert_eq!(towards_2[0].repo.id, 2, "qvec [0,1] must rank repo 2 first");
        // scores align with the repo they belong to (regression: zip misalignment)
        assert!(towards_1[0].score > towards_1[1].score);

        // FTS-only fallback still returns both
        let fts_only = search(&conn, &store, "tool", None, RepoSort::Relevance, 10).unwrap();
        assert_eq!(fts_only.len(), 2);

        // alternate sorts reorder matches: repo 2 starred later, repo 1 has more stars
        conn.execute("UPDATE repos SET stars = 500 WHERE id = 1", []).unwrap();
        let by_starred = search(&conn, &store, "tool", None, RepoSort::Starred, 10).unwrap();
        assert_eq!(by_starred[0].repo.id, 2, "starred_at 2026-01-02 wins");
        let by_stars = search(&conn, &store, "tool", None, RepoSort::Stars, 10).unwrap();
        assert_eq!(by_stars[0].repo.id, 1, "500 stars wins");
    }

    #[test]
    fn empty_query_returns_recent() {
        let conn = crate::db::open_in_memory().unwrap();
        let store = VectorStore::empty();
        let results = search(&conn, &store, "   ", None, RepoSort::Relevance, 10).unwrap();
        assert!(results.is_empty());
    }
}
