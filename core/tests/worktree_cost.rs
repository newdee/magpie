//! How much does the worktree check add to a scan? One extra `stat` per
//! directory (is `<dir>/.git` a file?). Run explicitly:
//!   cargo test -p magpie-core --test worktree_cost -- --ignored --nocapture
use magpie_core::{db, files};
use std::path::PathBuf;
use std::time::Instant;

fn tree(dirs: usize) -> PathBuf {
    let root = std::env::temp_dir().join(format!("magpie-wtcost-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for i in 0..dirs {
        let d = root.join(format!("d{}", i / 50)).join(format!("s{i}"));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("f.txt"), "x").unwrap();
    }
    root
}

#[test]
#[ignore]
fn scan_cost_with_and_without_the_worktree_check() {
    let dirs = 5_000;
    let root = tree(dirs);
    let conn = db::open_in_memory().unwrap();
    files::add_folder(&conn, &root.to_string_lossy()).unwrap();
    // first pass populates the index; the timed passes below are incremental
    // rescans (unchanged files are skipped), which is what the timer runs
    files::index_folders(&conn, |_| {}).unwrap();

    let time = |label: &str| {
        let t = Instant::now();
        for _ in 0..3 {
            files::index_folders(&conn, |_| {}).unwrap();
        }
        let per = t.elapsed() / 3;
        println!("{label:>14}: {per:?} per rescan of {dirs} dirs");
        per
    };
    db::meta_set(&conn, "skip_worktrees", "1").unwrap();
    let on = time("check on");
    db::meta_set(&conn, "skip_worktrees", "0").unwrap();
    let off = time("check off");
    println!(
        "overhead: {:?} total, {:.1} µs per directory",
        on.saturating_sub(off),
        on.saturating_sub(off).as_secs_f64() * 1e6 / dirs as f64
    );
    let _ = std::fs::remove_dir_all(&root);
}
