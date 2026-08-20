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

/// Brute-force top-k over a set of embeddings. Returns (id, similarity), best first.
pub fn top_similar(all: Vec<(i64, Vec<f32>)>, qvec: &[f32], limit: usize) -> Vec<(i64, f32)> {
    let mut scored: Vec<(i64, f32)> = all
        .into_iter()
        .filter(|(_, v)| v.len() == qvec.len())
        .map(|(id, v)| (id, dot(qvec, &v)))
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    scored.truncate(limit);
    scored
}

/// Vector candidates over repo embeddings.
pub fn vector_search(conn: &Connection, qvec: &[f32], limit: usize) -> Result<Vec<(i64, f32)>> {
    Ok(top_similar(db::all_embeddings(conn)?, qvec, limit))
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

/// Hybrid search. `qvec` is None when the embedding model is not ready — FTS-only then.
pub fn search(
    conn: &Connection,
    query: &str,
    qvec: Option<&[f32]>,
    limit: usize,
) -> Result<Vec<SearchResult>> {
    if query.trim().is_empty() {
        return Ok(db::recent_repos(conn, limit)?
            .into_iter()
            .map(|repo| SearchResult { repo, score: 0.0 })
            .collect());
    }

    let fts = db::fts_search(conn, query, CANDIDATES_PER_LIST)?;
    let vecs = match qvec {
        Some(qvec) => vec![vector_search(conn, qvec, CANDIDATES_PER_LIST)?],
        None => vec![],
    };
    let fused = rank_hybrid(fts, vecs);
    let scores: std::collections::HashMap<i64, f32> = fused.iter().copied().collect();
    let ids: Vec<i64> = fused.iter().take(limit).map(|(id, _)| *id).collect();
    let repos = db::repos_by_ids(conn, &ids)?;
    Ok(repos
        .into_iter()
        .map(|repo| {
            let score = scores.get(&repo.id).copied().unwrap_or(0.0);
            SearchResult { repo, score }
        })
        .collect())
}

/// Hybrid search over local files: filename/content FTS + e5 text vectors +
/// SigLIP image vectors, all fused by rank. The two vector spaces cover
/// disjoint id sets (text files vs images), so their similarities never mix.
pub fn search_files(
    conn: &Connection,
    query: &str,
    qvec: Option<&[f32]>,
    image_qvec: Option<&[f32]>,
    limit: usize,
) -> Result<Vec<crate::files::FileHit>> {
    use crate::files;
    if query.trim().is_empty() {
        return files::recent_files(conn, limit);
    }
    let fts = files::files_fts_search(conn, query, CANDIDATES_PER_LIST)?;
    let mut vecs = Vec::new();
    if let Some(qvec) = qvec {
        vecs.push(top_similar(
            files::all_file_embeddings(conn)?,
            qvec,
            CANDIDATES_PER_LIST,
        ));
    }
    if let Some(image_qvec) = image_qvec {
        vecs.push(top_similar(
            files::all_image_embeddings(conn)?,
            image_qvec,
            CANDIDATES_PER_LIST,
        ));
    }
    let fused = rank_hybrid(fts, vecs);
    let scores: std::collections::HashMap<i64, f32> = fused.iter().copied().collect();
    let ids: Vec<i64> = fused.iter().take(limit).map(|(id, _)| *id).collect();
    files::files_by_ids(conn, &ids, &scores)
}

/// Image-to-image search: a query image's SigLIP vector against all indexed
/// image vectors. Scores are raw cosine similarities.
pub fn search_images(
    conn: &Connection,
    image_qvec: &[f32],
    limit: usize,
) -> Result<Vec<crate::files::FileHit>> {
    use crate::files;
    let scored = top_similar(files::all_image_embeddings(conn)?, image_qvec, limit);
    let scores: std::collections::HashMap<i64, f32> = scored.iter().copied().collect();
    let ids: Vec<i64> = scored.into_iter().map(|(id, _)| id).collect();
    files::files_by_ids(conn, &ids, &scores)
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
        crate::db::put_embedding(&conn, 1, "h", &[1.0, 0.0]).unwrap();
        crate::db::put_embedding(&conn, 2, "h", &[0.0, 1.0]).unwrap();
        crate::db::put_embedding(&conn, 3, "h", &[0.7, 0.7]).unwrap();
        let hits: Vec<i64> = vector_search(&conn, &[1.0, 0.0], 3)
            .unwrap()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(hits, vec![1, 3, 2]);
        // dimension-mismatched query vectors match nothing rather than panicking
        assert!(vector_search(&conn, &[1.0, 0.0, 0.0], 3).unwrap().is_empty());
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
        crate::db::put_embedding(&conn, 1, "h", &[1.0, 0.0]).unwrap();
        crate::db::put_embedding(&conn, 2, "h", &[0.0, 1.0]).unwrap();

        // both match FTS equally on "tool"; the query vector must break the tie
        let towards_1 = search(&conn, "tool", Some(&[1.0, 0.0]), 10).unwrap();
        let towards_2 = search(&conn, "tool", Some(&[0.0, 1.0]), 10).unwrap();
        assert_eq!(towards_1.len(), 2);
        assert_eq!(towards_1[0].repo.id, 1, "qvec [1,0] must rank repo 1 first");
        assert_eq!(towards_2[0].repo.id, 2, "qvec [0,1] must rank repo 2 first");
        // scores align with the repo they belong to (regression: zip misalignment)
        assert!(towards_1[0].score > towards_1[1].score);

        // FTS-only fallback still returns both
        let fts_only = search(&conn, "tool", None, 10).unwrap();
        assert_eq!(fts_only.len(), 2);
    }

    #[test]
    fn empty_query_returns_recent() {
        let conn = crate::db::open_in_memory().unwrap();
        let results = search(&conn, "   ", None, 10).unwrap();
        assert!(results.is_empty());
    }
}
