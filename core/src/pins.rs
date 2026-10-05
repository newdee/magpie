//! Pinned folders (#13): up to four folders the user goes to often, at the
//! top of the local tab's empty box, in the order the user set. They need
//! not be indexed: a pin only opens the folder. Kept in meta as JSON, like
//! the editing-apps table, so a settings export carries them.

use anyhow::{bail, Result};
use rusqlite::Connection;
use std::path::Path;

pub const META_KEY: &str = "pinned_folders";
/// Four, so with the date line and the two cards from long ago the empty
/// box still fits on one screen (#13).
pub const MAX: usize = 4;

/// The pinned folders in the user's order; a missing or damaged record
/// reads as none.
pub fn list(conn: &Connection) -> Result<Vec<String>> {
    Ok(crate::db::meta_get(conn, META_KEY)?
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default())
}

fn same_path(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim_end_matches(['/', '\\']), b.trim_end_matches(['/', '\\']));
    if cfg!(windows) {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

/// Replace the list (the settings page sends it whole after an add, a
/// removal or a move). Every entry must be a folder that is there; the same
/// folder twice is kept once; more than MAX is refused.
pub fn set(conn: &Connection, paths: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for p in paths {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        if !Path::new(p).is_dir() {
            bail!("not a folder: {p}");
        }
        if !out.iter().any(|q| same_path(q, p)) {
            out.push(p.to_string());
        }
    }
    if out.len() > MAX {
        bail!("at most {MAX} pinned folders");
    }
    crate::db::meta_set(conn, META_KEY, &serde_json::to_string(&out)?)?;
    Ok(out)
}

/// Whether `path` is one of the pinned folders: only those are opened from
/// a pin's row.
pub fn is_pinned(conn: &Connection, path: &str) -> Result<bool> {
    Ok(list(conn)?.iter().any(|p| same_path(p, path)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(n: usize) -> (std::path::PathBuf, Vec<String>) {
        let root = std::env::temp_dir().join(format!("magpie-pins-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let v = (0..n)
            .map(|i| {
                let d = root.join(format!("d{i}"));
                std::fs::create_dir_all(&d).unwrap();
                d.to_string_lossy().into_owned()
            })
            .collect();
        (root, v)
    }

    #[test]
    fn pins_keep_their_order_once_each() {
        let conn = crate::db::open_in_memory().unwrap();
        assert!(list(&conn).unwrap().is_empty());
        let (root, d) = dirs(3);
        let set1 = set(&conn, &[d[2].clone(), d[0].clone(), format!("{}{}", d[2], std::path::MAIN_SEPARATOR)]).unwrap();
        assert_eq!(set1, vec![d[2].clone(), d[0].clone()], "the user's order, the same folder once");
        assert_eq!(list(&conn).unwrap(), set1);
        assert!(is_pinned(&conn, &d[0]).unwrap() && !is_pinned(&conn, &d[1]).unwrap());
        // moved: the new order is kept
        assert_eq!(set(&conn, &[d[0].clone(), d[2].clone()]).unwrap(), vec![d[0].clone(), d[2].clone()]);
        // removed down to none
        assert!(set(&conn, &[]).unwrap().is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn only_folders_and_at_most_four() {
        let conn = crate::db::open_in_memory().unwrap();
        let (root, d) = dirs(5);
        assert!(set(&conn, &d).is_err(), "five is too many");
        assert!(set(&conn, &[root.join("missing").to_string_lossy().into_owned()]).is_err());
        let file = root.join("f.txt");
        std::fs::write(&file, "x").unwrap();
        assert!(set(&conn, &[file.to_string_lossy().into_owned()]).is_err(), "a file is no folder");
        assert!(list(&conn).unwrap().is_empty(), "a refused list changes nothing");
        assert_eq!(set(&conn, &d[..4]).unwrap().len(), 4);
        // a damaged record reads as none
        crate::db::meta_set(&conn, META_KEY, "not json").unwrap();
        assert!(list(&conn).unwrap().is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
