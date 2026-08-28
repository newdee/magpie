//! E2E for the OCR indexing path: a fresh DB gets one image file row, the
//! worker's exact SQL picks it up, extracted text lands in files.content via
//! UPDATE, and the FTS triggers make it searchable — including CJK.
//! Run: cargo run -p magpie-core --example verify_ocr_index -- <image> [cache_dir]

use magpie_core::rusqlite::params;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let img = args.next().expect("usage: verify_ocr_index <image> [cache_dir]");
    let cache = args
        .next()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("magpie-ocr-test"));

    let dir = std::env::temp_dir().join("magpie-ocr-index-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    let conn = magpie_core::db::open(&dir.join("test.db"))?;
    conn.execute("INSERT INTO folders (path) VALUES ('/t')", [])?;
    conn.execute(
        "INSERT INTO files (folder_id, path, name, ext, size, mtime) VALUES (1, ?1, 'shot.png', 'png', 1, 42)",
        params![img],
    )?;

    // the worker's selection query, verbatim shape
    let exts = "'jpg','jpeg','png','webp','bmp','gif'";
    let pending: Vec<(i64, String, i64)> = {
        let mut stmt = conn.prepare(&format!(
            "SELECT id, path, mtime FROM files
             WHERE lower(ext) IN ({exts})
               AND (ocr_mtime IS NULL OR ocr_mtime != mtime)"
        ))?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<magpie_core::rusqlite::Result<Vec<_>>>()?
    };
    assert_eq!(pending.len(), 1, "fresh image row must be pending");

    let mut ocr = magpie_core::ocr::Ocr::new(&cache, &mut |_| {})?;
    let (id, path, mtime) = &pending[0];
    let text = ocr.extract_text_from_path(std::path::Path::new(path))?;
    conn.execute(
        "UPDATE files SET content = ?2, ocr_mtime = ?3 WHERE id = ?1",
        params![id, text, mtime],
    )?;

    // re-select: must be empty now (mtime recorded)
    let left: i64 = conn.query_row(
        &format!(
            "SELECT count(*) FROM files WHERE lower(ext) IN ({exts})
             AND (ocr_mtime IS NULL OR ocr_mtime != mtime)"
        ),
        [],
        |r| r.get(0),
    )?;
    assert_eq!(left, 0, "processed image must not be re-picked");

    // the real retrieval path must find it — "Hello" via FTS, and the CJK
    // substring via the LIKE supplement (unicode61 lumps a CJK run into one
    // token, so plain MATCH can't)
    for term in ["Hello", "本地搜索"] {
        let hits = magpie_core::files::files_fts_search(&conn, term, 10)?;
        assert!(!hits.is_empty(), "search must find {term:?}");
    }
    println!("ok: pending->extract->update->fts all verified; text = {:?}", text);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
