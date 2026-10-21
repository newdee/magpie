//! Folders by name (#24): "I remember the folder, not the files in it".
//! Folders are not indexed themselves; every folder that holds an indexed
//! file, from a registered root down, is worked out from the files table.
//! The list is kept in memory until `files_rev` moves (the triggers in db.rs
//! count every file that comes, goes or moves, whichever code path did it).

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// A folder under a registered root, the root itself included.
#[derive(Debug, Clone, PartialEq)]
pub struct Dir {
    pub path: String,
    pub name: String,
    /// Indexed files anywhere under it.
    pub files: u32,
}

/// A folder whose name matches the query.
#[derive(Debug, Clone, Serialize)]
pub struct DirHit {
    pub path: String,
    pub name: String,
    pub files: u32,
    pub score: f32,
}

/// One of the newest indexed files under a folder, for its preview.
#[derive(Debug, Clone, Serialize)]
pub struct DirFile {
    pub path: String,
    pub name: String,
    pub changed: i64,
}

/// How many times a file came, went or moved; 0 before the first file.
pub fn rev(conn: &Connection) -> Result<i64> {
    Ok(conn
        .query_row("SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'files_rev'", [], |r| r.get(0))
        .optional()?
        .unwrap_or(0))
}

/// Every folder that holds an indexed file, with how many it holds, in path
/// order. Each file counts for its folder and every folder above it up to
/// its root.
pub fn collect(conn: &Connection) -> Result<Vec<Dir>> {
    let roots: HashMap<i64, String> = conn
        .prepare("SELECT id, path FROM folders")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut stmt = conn.prepare("SELECT folder_id, path FROM files")?;
    let mut rows = stmt.query([])?;
    // each path is read in place, and a folder's name is copied only the
    // first time it is seen: 200k files hold a few thousand folders
    let mut counts: HashMap<String, u32> = HashMap::new();
    while let Some(r) = rows.next()? {
        let Some(root) = roots.get(&r.get::<_, i64>(0)?) else { continue };
        let file = r.get_ref(1)?.as_str()?;
        for dir in folders_up_to(file, root) {
            match counts.get_mut(dir) {
                Some(n) => *n += 1,
                None => {
                    counts.insert(dir.to_string(), 1);
                }
            }
        }
    }
    let mut out: Vec<Dir> = counts
        .into_iter()
        .map(|(path, files)| Dir {
            // a drive root (D:\) has no name of its own
            name: Path::new(&path).file_name().map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned()),
            path,
            files,
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// The folders a file sits in, its own first, up to and including `root`;
/// none when the file is not under `root`. Cut from the path at its
/// separators, so a drive root (`D:\`) keeps its own.
fn folders_up_to<'a>(file: &'a str, root: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    let under = file.len() > root.len()
        && file.starts_with(root)
        && (root.ends_with(std::path::is_separator) || file[root.len()..].starts_with(std::path::is_separator));
    let mut end = if under { file.len() } else { 0 };
    std::iter::from_fn(move || {
        if end <= root.len() {
            return None;
        }
        let cut = file[..end].rfind(std::path::is_separator)?;
        // the separator right after the root, or the root's own trailing one
        end = if cut < root.len() { root.len() } else { cut };
        Some(&file[..end])
    })
    .take_while(move |d| d.len() >= root.len())
}

/// The folder list, worked out again only after a file came, went or moved.
#[derive(Default)]
pub struct Cache(Mutex<Option<(i64, Arc<Vec<Dir>>)>>);

impl Cache {
    pub fn get(&self, conn: &Connection) -> Result<Arc<Vec<Dir>>> {
        // the counter is read first: a change while the list is worked out
        // leaves it stamped with the older count, so the next call redoes it
        let now = rev(conn)?;
        if let Some((at, dirs)) = &*self.0.lock().unwrap() {
            if *at == now {
                return Ok(dirs.clone());
            }
        }
        let dirs = Arc::new(collect(conn)?);
        *self.0.lock().unwrap() = Some((now, dirs.clone()));
        Ok(dirs)
    }
}

/// Folders whose name matches the query, best first: matched as app names
/// are (whole name, prefix, word start, initials, anywhere, pinyin: `bx`
/// finds 报销), or with several words, each somewhere in the name. Among
/// equal matches the folder holding more files leads, then the shorter path.
pub fn search(dirs: &[Dir], query: &str, limit: usize) -> Vec<DirHit> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let words: Vec<&str> = q.split_whitespace().collect();
    let mut hits: Vec<DirHit> = dirs
        .iter()
        .filter_map(|d| {
            let score = crate::apps::name_score(&q, &d.name, true).or_else(|| {
                let name = d.name.to_lowercase();
                (words.len() > 1 && words.iter().all(|w| name.contains(w))).then_some(0.5)
            })?;
            Some(DirHit { path: d.path.clone(), name: d.name.clone(), files: d.files, score })
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(b.files.cmp(&a.files))
            .then(a.path.len().cmp(&b.path.len()))
            .then(a.path.cmp(&b.path))
    });
    hits.truncate(limit);
    hits
}

/// The newest indexed files anywhere under `dir`, newest first.
pub fn newest_files(conn: &Connection, dir: &str, limit: usize) -> Result<Vec<DirFile>> {
    // every path under `dir` sorts between "dir\" and "dir]" (the next
    // character after the separator), so the path index answers it, and a
    // sibling that only starts with the same name ("docs-old") stays out
    let sep = std::path::MAIN_SEPARATOR;
    let base = dir.strip_suffix(sep).unwrap_or(dir);
    let lo = format!("{base}{sep}");
    let hi = format!("{base}{}", char::from(sep as u8 + 1));
    let mut stmt = conn.prepare(
        "SELECT path, name, COALESCE(changed, mtime) AS c FROM files
         WHERE path >= ?1 AND path < ?2 ORDER BY c DESC, path LIMIT ?3",
    )?;
    let rows = stmt
        .query_map(params![lo, hi, limit as i64], |r| {
            Ok(DirFile { path: r.get(0)?, name: r.get(1)?, changed: r.get(2)? })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(parts: &[&str]) -> String {
        let mut b = PathBuf::from(if cfg!(windows) { r"C:\w" } else { "/w" });
        for part in parts {
            b.push(part);
        }
        b.to_string_lossy().into_owned()
    }

    fn with_files(files: &[(&[&str], i64)]) -> Connection {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute("INSERT INTO folders(id, path) VALUES (1, ?1)", [p(&[])]).unwrap();
        for (parts, changed) in files {
            add(&conn, &p(parts), *changed);
        }
        conn
    }

    fn add(conn: &Connection, path: &str, changed: i64) {
        let name = Path::new(path).file_name().unwrap().to_string_lossy().into_owned();
        conn.execute(
            "INSERT INTO files(folder_id, path, name, ext, size, mtime, changed) VALUES (1, ?1, ?2, 'txt', 1, ?3, ?3)",
            params![path, name, changed],
        )
        .unwrap();
    }

    fn counts(dirs: &[Dir]) -> Vec<(String, u32)> {
        dirs.iter().map(|d| (d.path.clone(), d.files)).collect()
    }

    #[test]
    fn every_folder_up_to_the_root_counts_the_files_under_it() {
        let conn = with_files(&[
            (&["top.txt"], 1),
            (&["a", "x.txt"], 1),
            (&["a", "b", "y.txt"], 1),
            (&["a", "b", "z.txt"], 1),
            (&["c", "z.txt"], 1),
        ]);
        let dirs = collect(&conn).unwrap();
        assert_eq!(
            counts(&dirs),
            vec![(p(&[]), 5), (p(&["a"]), 3), (p(&["a", "b"]), 2), (p(&["c"]), 1)],
            "the root and each folder once; nothing above the root"
        );
        assert_eq!(dirs[2].name, "b");
        assert_eq!(dirs[0].name, "w", "the root is found by its own name too");
    }

    #[test]
    fn the_folders_of_a_path_stop_at_the_root() {
        let up = |file: &str, root: &str| folders_up_to(file, root).map(str::to_string).collect::<Vec<String>>();
        let s = std::path::MAIN_SEPARATOR;
        let (root, drive) = if cfg!(windows) { (r"C:\w", r"D:\") } else { ("/w", "/") };
        assert_eq!(up(&format!("{root}{s}a{s}b{s}y.txt"), root), [format!("{root}{s}a{s}b"), format!("{root}{s}a"), root.to_string()]);
        assert_eq!(up(&format!("{root}{s}top.txt"), root), [root]);
        assert!(up(&format!("{root}x{s}f.txt"), root).is_empty(), "a sibling that starts with the root's name is not under it");
        assert!(up(root, root).is_empty(), "the root itself is no file under it");
        assert_eq!(up(&format!("{drive}a{s}x.txt"), drive), [format!("{drive}a"), drive.to_string()], "a drive root keeps its separator");
        assert_eq!(up(&format!("{drive}x.txt"), drive), [drive]);
    }

    #[test]
    fn no_files_no_folders() {
        let conn = with_files(&[]);
        assert!(collect(&conn).unwrap().is_empty());
        assert_eq!(rev(&conn).unwrap(), 0);
        assert!(search(&collect(&conn).unwrap(), "w", 10).is_empty());
    }

    #[test]
    fn the_counter_moves_when_a_file_comes_goes_or_moves_and_only_then() {
        let conn = with_files(&[(&["a", "x.txt"], 1)]);
        let r1 = rev(&conn).unwrap();
        assert_eq!(r1, 1, "one insert");
        conn.execute("UPDATE files SET content = 'hello', size = 9, mtime = 5, thumb = x'00'", []).unwrap();
        assert_eq!(rev(&conn).unwrap(), r1, "a file that only changed inside stays where it was");
        conn.execute("UPDATE files SET path = ?1", [p(&["b", "x.txt"])]).unwrap();
        assert_eq!(rev(&conn).unwrap(), r1 + 1, "a move");
        conn.execute("DELETE FROM files", []).unwrap();
        assert_eq!(rev(&conn).unwrap(), r1 + 2, "a delete");
        add(&conn, &p(&["c", "y.txt"]), 1);
        assert_eq!(rev(&conn).unwrap(), r1 + 3);
    }

    #[test]
    fn the_cache_is_kept_until_a_file_comes_goes_or_moves() {
        let conn = with_files(&[(&["a", "x.txt"], 1), (&["b", "y.txt"], 1)]);
        let cache = Cache::default();
        let first = cache.get(&conn).unwrap();
        conn.execute("UPDATE files SET content = 'new text'", []).unwrap();
        assert!(Arc::ptr_eq(&first, &cache.get(&conn).unwrap()), "nothing moved: the same list, not worked out again");
        conn.execute("DELETE FROM files WHERE path = ?1", [p(&["b", "y.txt"])]).unwrap();
        let after = cache.get(&conn).unwrap();
        assert_eq!(counts(&after), vec![(p(&[]), 1), (p(&["a"]), 1)], "b had its last file deleted: gone");
        // a folder removed from the index takes its files, and so its folders
        conn.execute("DELETE FROM files WHERE folder_id = 1", []).unwrap();
        conn.execute("DELETE FROM folders WHERE id = 1", []).unwrap();
        assert!(cache.get(&conn).unwrap().is_empty());
    }

    #[test]
    fn whole_name_then_prefix_then_word_then_anywhere() {
        let conn = with_files(&[
            (&["docs", "1.txt"], 1),
            (&["docs-old", "1.txt"], 1),
            (&["project docs", "1.txt"], 1),
            (&["mydocsx", "1.txt"], 1),
            (&["other", "1.txt"], 1),
        ]);
        let dirs = collect(&conn).unwrap();
        let names: Vec<String> = search(&dirs, "Docs", 10).into_iter().map(|h| h.name).collect();
        assert_eq!(names, vec!["docs", "docs-old", "project docs", "mydocsx"], "case does not matter");
        let names: Vec<String> = search(&dirs, "project docs", 10).into_iter().map(|h| h.name).collect();
        assert_eq!(names, vec!["project docs"]);
        let names: Vec<String> = search(&dirs, "docs project", 10).into_iter().map(|h| h.name).collect();
        assert_eq!(names, vec!["project docs"], "several words, each somewhere in the name");
        assert!(search(&dirs, "   ", 10).is_empty(), "nothing typed, nothing listed");
        assert_eq!(search(&dirs, "docs", 2).len(), 2, "the limit holds");
    }

    #[test]
    fn same_name_folders_are_told_apart_by_path_and_ranked_by_size() {
        let conn = with_files(&[
            (&["web", "components", "a.tsx"], 1),
            (&["app", "src", "components", "a.tsx"], 1),
            (&["app", "src", "components", "b.tsx"], 1),
            (&["lib", "components", "a.tsx"], 1),
        ]);
        let hits = search(&collect(&conn).unwrap(), "components", 10);
        assert_eq!(
            hits.iter().map(|h| (h.path.clone(), h.files)).collect::<Vec<_>>(),
            vec![
                (p(&["app", "src", "components"]), 2),
                (p(&["lib", "components"]), 1),
                (p(&["web", "components"]), 1),
            ],
            "the one holding more files first; then the shorter path, then by path"
        );
    }

    #[test]
    fn chinese_names_match_directly_and_by_pinyin() {
        let conn = with_files(&[(&["报销 2026", "发票.pdf"], 1), (&["合同", "a.docx"], 1)]);
        let dirs = collect(&conn).unwrap();
        let names = |q: &str| search(&dirs, q, 10).into_iter().map(|h| h.name).collect::<Vec<_>>();
        assert_eq!(names("报销"), vec!["报销 2026"]);
        assert_eq!(names("bx"), vec!["报销 2026"], "pinyin initials, as for apps and workspaces");
        assert_eq!(names("hetong"), vec!["合同"], "full pinyin");
    }

    /// The cost on a big index: `cargo test -p magpie-core --release --lib
    /// dir_search::tests::timing -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn timing_on_200k_files() {
        let conn = with_files(&[]);
        let tx = conn.unchecked_transaction().unwrap();
        for i in 0..200_000u32 {
            let parts = [format!("proj{}", i % 50), format!("src{}", i % 7), format!("mod{}", i % 300), format!("f{i}.txt")];
            add(&tx, &p(&parts.iter().map(String::as_str).collect::<Vec<_>>()), i as i64);
        }
        tx.commit().unwrap();
        let t = std::time::Instant::now();
        let dirs = collect(&conn).unwrap();
        let collect_ms = t.elapsed().as_millis();
        let t = std::time::Instant::now();
        let hits = search(&dirs, "mod12", 30);
        let search_ms = t.elapsed().as_millis();
        let t = std::time::Instant::now();
        let none = search(&dirs, "zzzz", 30);
        let miss_ms = t.elapsed().as_millis();
        let cache = Cache::default();
        cache.get(&conn).unwrap();
        let t = std::time::Instant::now();
        cache.get(&conn).unwrap();
        let cached_us = t.elapsed().as_micros();
        println!(
            "200k files, {} folders: collect {collect_ms} ms, search {search_ms} ms ({} hits), miss {miss_ms} ms, cached get {cached_us} us",
            dirs.len(),
            hits.len(),
        );
        assert!(none.is_empty());
    }

    #[test]
    fn newest_files_stay_under_the_folder() {
        let conn = with_files(&[
            (&["docs", "old.txt"], 10),
            (&["docs", "deep", "new.txt"], 30),
            (&["docs-old", "newest.txt"], 99),
            (&["docsx.txt"], 98),
            (&["docs", "mid.txt"], 20),
        ]);
        let names = |dir: &str| newest_files(&conn, dir, 10).unwrap().into_iter().map(|f| f.name).collect::<Vec<_>>();
        assert_eq!(names(&p(&["docs"])), vec!["new.txt", "mid.txt", "old.txt"], "subfolders too, newest first; not docs-old");
        assert_eq!(newest_files(&conn, &p(&["docs"]), 2).unwrap().len(), 2);
        assert_eq!(names(&p(&[])).len(), 5, "the root holds them all");
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(names(&format!("{}{sep}", p(&["docs"]))).len(), 3, "a trailing separator is the same folder");
        assert!(names(&p(&["nowhere"])).is_empty());
    }
}
