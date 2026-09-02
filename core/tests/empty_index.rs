//! Removing the last indexed folder must leave every later pass — the scan,
//! the embed catch-up, the vector reload — succeeding on an empty index.
use magpie_core::{db, files, search::VectorStore};
use std::path::PathBuf;

fn scratch() -> PathBuf {
    let d = std::env::temp_dir().join(format!("magpie-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("a.txt"), "hello index").unwrap();
    std::fs::write(d.join("b.md"), "# notes\nmore text").unwrap();
    d
}

#[test]
fn every_pass_survives_an_index_with_no_folders_left() {
    let dir = scratch();
    let conn = db::open_in_memory().unwrap();

    // a populated index first, so the removal has something to tear down
    let id = files::add_folder(&conn, &dir.to_string_lossy()).unwrap();
    let report = files::index_folders(&conn, |_| {}).unwrap();
    assert_eq!(report.scanned, 2);
    assert_eq!(files::file_count(&conn).unwrap(), 2);

    files::remove_folder(&conn, id).unwrap();
    assert_eq!(files::folder_count(&conn).unwrap(), 0);
    assert_eq!(files::file_count(&conn).unwrap(), 0, "the folder's rows go with it");

    // what the timer, the manual refresh and the embed catch-up run next
    let report = files::index_folders(&conn, |_| {}).unwrap();
    assert_eq!(report.scanned, 0);
    assert_eq!(files::recent_files(&conn, 10).unwrap().len(), 0);
    let store = VectorStore::load(&conn).unwrap();
    assert!(store.file_chunks.is_empty() && store.images.is_empty());
    assert!(files::list_folders(&conn).unwrap().is_empty());
    assert!(files::file_chunk_hashes(&conn).unwrap().is_empty());
    assert!(files::image_embedding_hashes(&conn).unwrap().is_empty());

    // and adding a folder back works as on day one
    files::add_folder(&conn, &dir.to_string_lossy()).unwrap();
    assert_eq!(files::index_folders(&conn, |_| {}).unwrap().scanned, 2);
    let _ = std::fs::remove_dir_all(&dir);
}
