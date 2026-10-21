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

/// The folders scope's list is worked out again only when a file comes,
/// goes or moves (dir_search.rs): a file rewritten in place, through the
/// indexer's own upsert, leaves its counter alone.
#[test]
fn a_rewritten_file_leaves_the_folder_list_alone() {
    let (conn, root) = setup("rev");
    let before = magpie_core::dir_search::rev(&conn).unwrap();
    fs::write(root.join("a.txt"), "alpha one, longer now").unwrap();
    let report = files::index_changed(&conn, &[root.join("a.txt")], |_| {}).unwrap();
    assert_eq!(report.indexed, 1, "the new text went in");
    assert_eq!(magpie_core::dir_search::rev(&conn).unwrap(), before);
    fs::write(root.join("c.txt"), "gamma").unwrap();
    files::index_changed(&conn, &[root.join("c.txt")], |_| {}).unwrap();
    assert_eq!(magpie_core::dir_search::rev(&conn).unwrap(), before + 1, "a new file moves it");
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

/// A gitignored folder (`target/`, `node_modules/`) keeps out what is under
/// it too: an event inside it (a build writing there) indexes nothing, as
/// the full walk, which never goes in, indexes nothing there.
#[test]
fn nothing_under_a_gitignored_folder_comes_in_by_an_event() {
    let (conn, root) = setup("ignored-dir");
    fs::write(root.join(".gitignore"), "ignored/\n").unwrap();
    fs::create_dir_all(root.join("ignored").join("components")).unwrap();
    fs::write(root.join("ignored").join("components").join("q.tsx"), "built output").unwrap();
    fs::write(root.join("ignored").join("top.txt"), "built output").unwrap();
    assert_eq!(files::index_folders(&conn, |_| {}).unwrap().indexed, 0, "the full walk stays out");
    for event in [
        root.join("ignored").join("components").join("q.tsx"),
        root.join("ignored").join("components"),
        root.join("ignored").join("top.txt"),
        root.join("ignored"),
    ] {
        let report = files::index_changed(&conn, std::slice::from_ref(&event), |_| {}).unwrap();
        assert_eq!(report.indexed, 0, "{event:?} brought in {:?}", rows(&conn));
    }
    assert_eq!(rows(&conn).len(), 2);
    let _ = fs::remove_dir_all(&root);
}

/// What events bring in is what the full walk takes: a `!` rule brings a
/// folder back, a deeper .gitignore or an .ignore file leaves one out.
#[test]
fn events_take_in_what_the_full_walk_takes() {
    let (conn, root) = setup("ignore-rules");
    fs::write(root.join(".gitignore"), "gen/*\n!gen/keep/\n").unwrap();
    fs::create_dir_all(root.join("sub").join("out")).unwrap();
    fs::write(root.join("sub").join(".gitignore"), "out/\n").unwrap();
    fs::create_dir_all(root.join("skipme")).unwrap();
    fs::write(root.join(".ignore"), "skipme/\n").unwrap();
    let tree = [
        root.join("gen").join("keep").join("k.txt"),
        root.join("gen").join("drop").join("d.txt"),
        root.join("sub").join("out").join("o.txt"),
        root.join("sub").join("in").join("i.txt"),
        root.join("skipme").join("s.txt"),
    ];
    for f in &tree {
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, "text").unwrap();
    }
    for f in &tree {
        files::index_changed(&conn, std::slice::from_ref(f), |_| {}).unwrap();
        files::index_changed(&conn, &[f.parent().unwrap().to_path_buf()], |_| {}).unwrap();
    }
    let by_events: Vec<String> = rows(&conn).into_iter().map(|(p, _)| p).collect();
    let full = db::open_in_memory().unwrap();
    files::add_folder(&full, root.to_str().unwrap()).unwrap();
    files::index_folders(&full, |_| {}).unwrap();
    let by_walk: Vec<String> = rows(&full).into_iter().map(|(p, _)| p).collect();
    assert_eq!(by_events, by_walk);
    assert!(by_walk.iter().any(|p| p.ends_with("k.txt")) && by_walk.iter().any(|p| p.ends_with("i.txt")), "{by_walk:?}");
    assert_eq!(by_walk.len(), 4, "a.txt, b.txt, k.txt, i.txt: {by_walk:?}");
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
