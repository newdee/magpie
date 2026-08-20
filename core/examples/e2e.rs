//! End-to-end smoke over the real pipeline: index this crate's own src/,
//! FTS-search it, load the real ONNX model, embed, run a Chinese semantic
//! query against English source files, and check determinism.
//!
//! Run: cargo run -p magpie-core --example e2e --release
//! Set MAGPIE_MODEL_DIR to reuse an existing model cache (e.g. the app's).

use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let tmp = std::env::temp_dir().join(format!("magpie-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&tmp)?;
    let db_path = tmp.join("e2e.db");
    let conn = magpie_core::db::open(&db_path)?;

    // 1. index this crate's src/ via the real folder pipeline
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    magpie_core::files::add_folder(&conn, src.to_str().unwrap())?;
    let t = std::time::Instant::now();
    let report = magpie_core::files::index_folders(&conn, |_| {})?;
    println!(
        "index: scanned={} indexed={} removed={} in {:?}",
        report.scanned, report.indexed, report.removed, t.elapsed()
    );
    assert!(report.indexed >= 6, "core/src has at least 6 rs files");

    // 2. keyword path
    let hits = magpie_core::search::search_files(&conn, "embedding", None, 10)?;
    println!("fts 'embedding': {} hits", hits.len());
    for h in hits.iter().take(3) {
        println!("  {}  score={:.4}", h.name, h.score);
    }
    assert!(!hits.is_empty(), "FTS must find 'embedding' in core/src");

    // 3. semantic path with the real model (downloads on first run)
    let model_dir = std::env::var("MAGPIE_MODEL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| tmp.join("models"));
    println!("loading model (cache: {}) ...", model_dir.display());
    let t = std::time::Instant::now();
    let mut embedder = magpie_core::embed::Embedder::new(&model_dir)?;
    println!("model ready in {:?}", t.elapsed());

    let t = std::time::Instant::now();
    let n = magpie_core::files::embed_pending_files(&conn, &mut embedder, |_, _| {})?;
    println!("embedded {} files in {:?}", n, t.elapsed());
    assert!(n >= 6);

    // Chinese query against English source files: FTS finds nothing, the
    // vector side must carry the result alone.
    let query = "向量检索 余弦相似度";
    let t = std::time::Instant::now();
    let qvec = embedder.embed_query(query)?;
    println!("query embed in {:?}", t.elapsed());
    assert!(
        magpie_core::search::search_files(&conn, query, None, 5)?.is_empty(),
        "sanity: keyword-only must find nothing for a Chinese query"
    );
    let hits = magpie_core::search::search_files(&conn, query, Some(&qvec), 5)?;
    println!("hybrid zh query '{query}': {} hits", hits.len());
    for h in &hits {
        println!("  {}  score={:.4}", h.name, h.score);
    }
    assert!(!hits.is_empty(), "vector side must recall for Chinese query");

    // 4. determinism: identical input, byte-identical vector, identical ranking
    let qvec2 = embedder.embed_query(query)?;
    assert_eq!(qvec, qvec2, "query embedding must be byte-identical across runs");
    let hits2 = magpie_core::search::search_files(&conn, query, Some(&qvec2), 5)?;
    let a: Vec<i64> = hits.iter().map(|h| h.id).collect();
    let b: Vec<i64> = hits2.iter().map(|h| h.id).collect();
    assert_eq!(a, b, "ranking must be deterministic");
    println!("determinism: OK (byte-identical vector, identical ranking)");

    std::fs::remove_dir_all(&tmp).ok();
    println!("E2E OK");
    Ok(())
}
