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
    let hits = magpie_core::search::search_files(&conn, "embedding", None, None, 10)?;
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
        magpie_core::search::search_files(&conn, query, None, None, 5)?.is_empty(),
        "sanity: keyword-only must find nothing for a Chinese query"
    );
    let hits = magpie_core::search::search_files(&conn, query, Some(&qvec), None, 5)?;
    println!("hybrid zh query '{query}': {} hits", hits.len());
    for h in &hits {
        println!("  {}  score={:.4}", h.name, h.score);
    }
    assert!(!hits.is_empty(), "vector side must recall for Chinese query");

    // 4. determinism: identical input, byte-identical vector, identical ranking
    let qvec2 = embedder.embed_query(query)?;
    assert_eq!(qvec, qvec2, "query embedding must be byte-identical across runs");
    let hits2 = magpie_core::search::search_files(&conn, query, Some(&qvec2), None, 5)?;
    let a: Vec<i64> = hits.iter().map(|h| h.id).collect();
    let b: Vec<i64> = hits2.iter().map(|h| h.id).collect();
    assert_eq!(a, b, "ranking must be deterministic");
    println!("determinism: OK (byte-identical vector, identical ranking)");

    // 5. image path with SigLIP (downloads its model on first run)
    let img_dir = tmp.join("imgs");
    std::fs::create_dir_all(&img_dir)?;
    // the repo icon (a white bird mark on dark) + a synthetic gradient
    let icon = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src-tauri/icon-1024.png");
    std::fs::copy(&icon, img_dir.join("bird.png"))?;
    let mut grad = image::RgbImage::new(256, 256);
    for (x, y, p) in grad.enumerate_pixels_mut() {
        *p = image::Rgb([x as u8, y as u8, 128]);
    }
    grad.save(img_dir.join("gradient.png"))?;
    magpie_core::files::add_folder(&conn, img_dir.to_str().unwrap())?;
    magpie_core::files::index_folders(&conn, |_| {})?;

    println!("loading siglip ...");
    let t = std::time::Instant::now();
    let mut siglip = magpie_core::siglip::Siglip::new(&model_dir)?;
    println!("siglip ready in {:?}", t.elapsed());
    let t = std::time::Instant::now();
    let n = magpie_core::files::embed_pending_images(&conn, &mut siglip, |_, _| {})?;
    println!("embedded {n} images in {:?}", t.elapsed());
    assert_eq!(n, 2);

    // Chinese query, English filenames: FTS finds nothing, SigLIP must carry it
    let iq_text = "一只白色的鸟的标志";
    let iq = siglip.embed_query(iq_text)?;
    assert!(
        magpie_core::search::search_files(&conn, iq_text, None, None, 5)?.is_empty(),
        "sanity: keyword-only must find nothing for the Chinese image query"
    );
    let hits = magpie_core::search::search_files(&conn, iq_text, None, Some(&iq), 5)?;
    println!("image query '{iq_text}': {} hits", hits.len());
    for h in &hits {
        println!("  {}  score={:.4}", h.name, h.score);
    }
    assert!(!hits.is_empty(), "image vectors must recall for the Chinese query");
    let iq2 = siglip.embed_query(iq_text)?;
    assert_eq!(iq, iq2, "siglip query embedding must be byte-identical");
    // quality signal, printed not asserted (two synthetic images only)
    println!("top image hit: {} (bird.png expected)", hits[0].name);

    // 6. image-to-image: querying with bird.png itself must rank it first
    let self_vec = siglip.embed_image(&img_dir.join("bird.png"))?;
    let hits = magpie_core::search::search_images(&conn, &self_vec, 5)?;
    println!("image-to-image (bird.png as query):");
    for h in &hits {
        println!("  {}  score={:.4}  thumb={}", h.name, h.score, h.thumb.is_some());
    }
    assert_eq!(hits[0].name, "bird.png", "self-similarity must win");
    assert!(hits[0].score > 0.99, "self cosine ~1.0, got {}", hits[0].score);
    assert!(hits[0].thumb.is_some(), "image hits must carry a thumbnail");

    std::fs::remove_dir_all(&tmp).ok();
    println!("E2E OK");
    Ok(())
}
