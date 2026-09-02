//! Linked git worktrees (`git worktree add`) are full checkouts, usually of
//! another branch of a repository whose main checkout sits in the same
//! indexed folder. Indexing them multiplies every cost by the number of
//! worktrees: file rows, full text, and above all the embeddings, which are
//! keyed by path and never shared. The walker prunes a worktree when the
//! checkout it links to is already covered by an indexed folder; a worktree
//! that is the only copy (a bare repository's, or one living outside every
//! indexed folder) is indexed as before.

use std::path::{Path, PathBuf};

/// The `gitdir:` target of `dir/.git`, when `.git` is a pointer FILE (the
/// mark of a linked worktree; the main checkout has a `.git` directory).
/// Relative targets resolve against `dir`.
pub fn gitdir_pointer(dir: &Path) -> Option<PathBuf> {
    let dotgit = dir.join(".git");
    if !dotgit.is_file() {
        return None;
    }
    let text = std::fs::read_to_string(&dotgit).ok()?;
    let target = text.trim().strip_prefix("gitdir:")?.trim();
    if target.is_empty() {
        return None;
    }
    let p = PathBuf::from(target);
    Some(if p.is_absolute() { p } else { dir.join(p) })
}

/// The main checkout a linked worktree belongs to, from its gitdir target
/// `<common>/worktrees/<name>`: the parent of `<common>` when `<common>` is a
/// `.git` directory. A bare repository (`repo.git/worktrees/<name>`) has no
/// checkout of its own, so None.
pub fn main_checkout(gitdir: &Path) -> Option<PathBuf> {
    let name = gitdir.parent()?;
    if name.file_name()? != "worktrees" {
        return None;
    }
    let common = name.parent()?;
    if common.file_name()? != ".git" {
        return None;
    }
    common.parent().map(Path::to_path_buf)
}

/// Is `dir` a linked worktree whose main checkout lies inside one of the
/// indexed folders? Only then is it a copy of something already indexed.
pub fn is_shadowed(dir: &Path, indexed: &[PathBuf]) -> bool {
    let Some(gitdir) = gitdir_pointer(dir) else {
        return false;
    };
    let Some(main) = main_checkout(&gitdir) else {
        return false;
    };
    let main = normalize(&main);
    indexed.iter().any(|root| main.starts_with(normalize(root)))
}

/// Canonical where the path exists (so `..`, symlinks and Windows verbatim
/// prefixes compare equal), lexical otherwise.
fn normalize(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("magpie-wt-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// main/.git/ (dir) with worktrees/feat/, and wt/.git (file) pointing at it
    fn layout(root: &Path, pointer: &str) -> (PathBuf, PathBuf) {
        let main = root.join("proj");
        std::fs::create_dir_all(main.join(".git").join("worktrees").join("feat")).unwrap();
        std::fs::write(main.join("a.txt"), "main copy").unwrap();
        let wt = root.join("proj-feat");
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(wt.join(".git"), pointer).unwrap();
        std::fs::write(wt.join("a.txt"), "feature copy").unwrap();
        (main, wt)
    }

    #[test]
    fn main_checkout_is_the_parent_of_the_dot_git_dir() {
        let g = Path::new("/home/u/proj/.git/worktrees/feat");
        assert_eq!(main_checkout(g), Some(PathBuf::from("/home/u/proj")));
        // bare repositories have no checkout to be shadowed by
        assert_eq!(main_checkout(Path::new("/srv/proj.git/worktrees/feat")), None);
        // not a worktree gitdir at all
        assert_eq!(main_checkout(Path::new("/home/u/proj/.git")), None);
    }

    #[test]
    fn pointer_file_is_read_and_relative_targets_resolve() {
        let root = scratch("ptr");
        let (main, wt) = layout(&root, "gitdir: ../proj/.git/worktrees/feat\n");
        let got = gitdir_pointer(&wt).unwrap();
        assert_eq!(normalize(&got), normalize(&main.join(".git").join("worktrees").join("feat")));
        assert_eq!(gitdir_pointer(&main), None, "a .git DIRECTORY is not a pointer");
        assert_eq!(gitdir_pointer(&root), None, "no .git at all");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pointer_file_tolerates_crlf_spaces_and_odd_contents() {
        let root = scratch("crlf");
        let wt = root.join("wt");
        std::fs::create_dir_all(&wt).unwrap();
        let cases: [(&str, Option<&str>); 5] = [
            ("gitdir: C:/x/.git/worktrees/a\r\n", Some("C:/x/.git/worktrees/a")),
            ("gitdir:   C:/x/.git/worktrees/a   ", Some("C:/x/.git/worktrees/a")),
            ("gitdir:", None),
            ("not a pointer at all", None),
            ("", None),
        ];
        for (content, want) in cases {
            std::fs::write(wt.join(".git"), content).unwrap();
            let got = gitdir_pointer(&wt).map(|p| p.to_string_lossy().replace('\\', "/"));
            assert_eq!(got.as_deref(), want, "content {content:?}");
            // whatever the pointer says, a dangling target never panics
            let _ = is_shadowed(&wt, std::slice::from_ref(&root));
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn shadowed_only_when_the_main_checkout_is_indexed() {
        let root = scratch("shadow");
        let abs = root.join("proj").join(".git").join("worktrees").join("feat");
        let (main, wt) = layout(&root, &format!("gitdir: {}\n", abs.display()));
        let only = std::slice::from_ref;
        assert!(is_shadowed(&wt, only(&root)), "root covers the main checkout");
        assert!(is_shadowed(&wt, only(&main)), "the main checkout itself is indexed");
        assert!(!is_shadowed(&wt, only(&wt)), "only the worktree is indexed: it is the sole copy");
        assert!(!is_shadowed(&wt, &[root.join("elsewhere")]), "unrelated folder");
        assert!(!is_shadowed(&main, only(&root)), "the main checkout is never pruned");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_bare_repositorys_worktree_is_never_shadowed() {
        let root = scratch("bare");
        let bare = root.join("proj.git").join("worktrees").join("feat");
        std::fs::create_dir_all(&bare).unwrap();
        let wt = root.join("feat");
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(wt.join(".git"), format!("gitdir: {}", bare.display())).unwrap();
        assert!(!is_shadowed(&wt, std::slice::from_ref(&root)));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_indexer_prunes_shadowed_worktrees_unless_told_not_to() {
        let root = scratch("index");
        let abs = root.join("proj").join(".git").join("worktrees").join("feat");
        let (_main, _wt) = layout(&root, &format!("gitdir: {}\n", abs.display()));
        let conn = crate::db::open_in_memory().unwrap();
        crate::files::add_folder(&conn, &root.to_string_lossy()).unwrap();
        let paths = |conn: &rusqlite::Connection| -> Vec<String> {
            let mut s = conn.prepare("SELECT path FROM files ORDER BY path").unwrap();
            s.query_map([], |r| r.get::<_, String>(0)).unwrap().map(|r| r.unwrap()).collect()
        };

        crate::files::index_folders(&conn, |_| {}).unwrap();
        let p = paths(&conn);
        assert_eq!(p.len(), 1, "default: only the main checkout's file, got {p:?}");
        assert!(p[0].contains("proj") && !p[0].contains("proj-feat"));

        crate::db::meta_set(&conn, "skip_worktrees", "0").unwrap();
        crate::files::index_folders(&conn, |_| {}).unwrap();
        assert_eq!(paths(&conn).len(), 2, "switched off: both copies");

        crate::db::meta_set(&conn, "skip_worktrees", "1").unwrap();
        crate::files::index_folders(&conn, |_| {}).unwrap();
        assert_eq!(paths(&conn).len(), 1, "switched back on: the worktree's rows are pruned again");
        let _ = std::fs::remove_dir_all(&root);
    }
}
