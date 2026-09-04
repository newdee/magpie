//! `files::index_changed`: the watcher's scoped re-index applies the full
//! walk's rules and touches nothing outside the paths it was given.
use magpie_core::{db, files};
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};

/// A scratch folder (a.txt at the root, sub/b.txt below), registered and
/// fully indexed once. Returns the connection and the folder path as the
/// index stores it (canonical), which is what watcher events are built on.
fn setup(name: &str) -> (Connection, PathBuf) {
    let dir = std::env::temp_dir().join(format!("magpie-watch-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("sub")).unwrap();
    fs::write(dir.join("a.txt"), "alpha one").unwrap();
    fs::write(dir.join("sub").join("b.txt"), "beta two").unwrap();
    let conn = db::open_in_memory().unwrap();
    files::add_folder(&conn, dir.to_str().unwrap()).unwrap();
    files::index_folders(&conn, |_| {}).unwrap();
    let root = PathBuf::from(&files::list_folders(&conn).unwrap()[0].path);
    (conn, root)
}

fn rows(conn: &Connection) -> Vec<(String, String)> {
    let mut stmt = conn.prepare("SELECT path, COALESCE(content, '') FROM files ORDER BY path").unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn content_of(conn: &Connection, path: &Path) -> Option<String> {
    rows(conn)
        .into_iter()
        .find(|(p, _)| Path::new(p) == path)
        .map(|(_, c)| c)
}

#[test]
fn a_changed_file_updates_only_its_row() {
    let (conn, root) = setup("change");
    assert_eq!(rows(&conn).len(), 2, "the full walk indexed both files");
    // a different size is enough: the walk compares mtime and size
    fs::write(root.join("a.txt"), "alpha one rewritten").unwrap();
    let report = files::index_changed(&conn, &[root.join("a.txt")], |_| {}).unwrap();
    assert_eq!((report.indexed, report.removed), (1, 0));
    assert_eq!(content_of(&conn, &root.join("a.txt")).as_deref(), Some("alpha one rewritten"));
    assert_eq!(content_of(&conn, &root.join("sub").join("b.txt")).as_deref(), Some("beta two"));
    // the same event again, nothing changed: looked at, wrote nothing
    let again = files::index_changed(&conn, &[root.join("a.txt")], |_| {}).unwrap();
    assert_eq!((again.scanned, again.indexed, again.removed), (1, 0, 0));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_deleted_file_is_dropped_and_its_siblings_stay() {
    let (conn, root) = setup("delete");
    fs::write(root.join("c.txt"), "gamma").unwrap();
    files::index_changed(&conn, &[root.join("c.txt")], |_| {}).unwrap();
    assert_eq!(rows(&conn).len(), 3);
    fs::remove_file(root.join("a.txt")).unwrap();
    let report = files::index_changed(&conn, &[root.join("a.txt")], |_| {}).unwrap();
    assert_eq!(report.removed, 1);
    let paths: Vec<String> = rows(&conn).into_iter().map(|(p, _)| p).collect();
    assert!(!paths.iter().any(|p| p.ends_with("a.txt")), "{paths:?}");
    assert!(paths.iter().any(|p| p.ends_with("c.txt")), "the sibling stays: {paths:?}");
    assert!(paths.iter().any(|p| p.ends_with("b.txt")), "a subfolder file is out of scope: {paths:?}");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_new_directory_is_walked_whole() {
    let (conn, root) = setup("newdir");
    fs::create_dir_all(root.join("sub2").join("deep")).unwrap();
    fs::write(root.join("sub2").join("deep").join("c.txt"), "gamma three").unwrap();
    fs::write(root.join("sub2").join("d.txt"), "delta four").unwrap();
    // the OS reports the directory; its contents came with it
    let report = files::index_changed(&conn, &[root.join("sub2")], |_| {}).unwrap();
    assert_eq!(report.indexed, 2);
    assert_eq!(content_of(&conn, &root.join("sub2").join("deep").join("c.txt")).as_deref(), Some("gamma three"));
    assert_eq!(rows(&conn).len(), 4);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_renamed_directory_moves_its_rows() {
    let (conn, root) = setup("rename");
    fs::rename(root.join("sub"), root.join("moved")).unwrap();
    // a rename arrives as the old path (gone) and the new one (a directory)
    let report = files::index_changed(&conn, &[root.join("sub"), root.join("moved")], |_| {}).unwrap();
    assert_eq!((report.indexed, report.removed), (1, 1));
    let paths: Vec<String> = rows(&conn).into_iter().map(|(p, _)| p).collect();
    assert!(paths.iter().any(|p| Path::new(p) == root.join("moved").join("b.txt")), "{paths:?}");
    assert!(!paths.iter().any(|p| Path::new(p) == root.join("sub").join("b.txt")), "{paths:?}");
    assert_eq!(paths.len(), 2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_deleted_directory_takes_everything_under_it() {
    let (conn, root) = setup("rmdir");
    fs::remove_dir_all(root.join("sub")).unwrap();
    let report = files::index_changed(&conn, &[root.join("sub")], |_| {}).unwrap();
    assert_eq!(report.removed, 1);
    assert_eq!(rows(&conn).len(), 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn dotfiles_gitignored_and_outside_paths_are_ignored() {
    let (conn, root) = setup("ignored");
    fs::create_dir_all(root.join(".hidden")).unwrap();
    fs::write(root.join(".hidden").join("x.txt"), "secret").unwrap();
    fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(root.join("ignored.txt"), "not for the index").unwrap();
    let elsewhere = std::env::temp_dir().join(format!("magpie-watch-elsewhere-{}.txt", std::process::id()));
    fs::write(&elsewhere, "outside every folder").unwrap();
    let report = files::index_changed(
        &conn,
        &[root.join(".hidden").join("x.txt"), root.join("ignored.txt"), elsewhere.clone()],
        |_| {},
    )
    .unwrap();
    assert_eq!(report.indexed, 0, "{:?}", rows(&conn));
    assert_eq!(rows(&conn).len(), 2);
    let _ = fs::remove_file(&elsewhere);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_change_batch_never_prunes_outside_its_scopes() {
    let (conn, root) = setup("scope");
    // b.txt vanishes from disk, but the batch is only about a.txt: the
    // full walk owns that reconciliation, a scoped pass must not guess
    fs::remove_file(root.join("sub").join("b.txt")).unwrap();
    fs::write(root.join("a.txt"), "alpha one more").unwrap();
    let report = files::index_changed(&conn, &[root.join("a.txt")], |_| {}).unwrap();
    assert_eq!((report.indexed, report.removed), (1, 0));
    assert_eq!(rows(&conn).len(), 2, "b.txt's row waits for the full walk");
    let full = files::index_folders(&conn, |_| {}).unwrap();
    assert_eq!(full.removed, 1);
    assert_eq!(rows(&conn).len(), 1);
    let _ = fs::remove_dir_all(&root);
}
