//! A path typed into the box: `D:\Projects`, `~/Downloads`, `%APPDATA%`,
//! `$HOME/.config`, `\\nas\share`. When it names something that exists, the
//! top row offers to open the folder, or to show the file in its folder
//! (a file is never run from here).

use std::path::PathBuf;

/// The existing file or folder `query` names, and whether it is a folder.
/// None when the query does not look like a path or nothing is there, so a
/// half-typed path never takes over the box.
pub fn resolve(query: &str) -> Option<(PathBuf, bool)> {
    let q = query.trim().trim_matches(|c| c == '"' || c == '\'');
    if q.is_empty() || !looks_like_path(q) {
        return None;
    }
    let expanded = expand(q)?;
    let meta = std::fs::metadata(&expanded).ok()?;
    Some((expanded, meta.is_dir()))
}

fn looks_like_path(q: &str) -> bool {
    let b = q.as_bytes();
    let drive = b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b.len() == 2 || b[2] == b'\\' || b[2] == b'/');
    let unc = q.starts_with("\\\\");
    let home = q == "~" || q.starts_with("~/") || q.starts_with("~\\");
    let env = q.starts_with('%') || q.starts_with('$');
    let root = !cfg!(windows) && q.starts_with('/');
    drive || unc || home || env || root
}

/// `~` and environment variables (`%NAME%` everywhere Windows-style, `$NAME`
/// and `${NAME}` unix-style). An unknown variable means no path.
fn expand(q: &str) -> Option<PathBuf> {
    let mut s = if q == "~" || q.starts_with("~/") || q.starts_with("~\\") {
        let home = dirs::home_dir()?;
        format!("{}{}", home.display(), &q[1..])
    } else {
        q.to_string()
    };
    // one pass, left to right: a value that itself holds % or $ is taken as
    // it is, never expanded again (re-scanning could loop forever)
    let mut out = String::with_capacity(s.len());
    let mut rest = s.as_str();
    while let Some(i) = rest.find(['%', '$']) {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let (name, used) = if rest.as_bytes()[i] == b'%' {
            let close = after.find('%')?; // %NAME%
            (&after[..close], close + 1)
        } else if let Some(inner) = after.strip_prefix('{') {
            let close = inner.find('}')?; // ${NAME}
            (&inner[..close], close + 2)
        } else {
            // $NAME
            let n = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(after.len());
            (&after[..n], n)
        };
        if name.is_empty() {
            return None;
        }
        // $HOME is unset on Windows unless something set it: the home folder
        let value = match std::env::var(name) {
            Ok(v) => v,
            Err(_) if name == "HOME" => dirs::home_dir()?.to_string_lossy().into_owned(),
            Err(_) => return None,
        };
        out.push_str(&value);
        rest = &after[used..];
    }
    out.push_str(rest);
    s = out;
    #[cfg(windows)]
    {
        s = s.replace('/', "\\");
        if s.len() == 2 && s.ends_with(':') {
            s.push('\\'); // "D:" alone means the drive's root, not its current folder
        }
    }
    Some(PathBuf::from(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_paths_resolve_and_the_rest_do_not() {
        let dir = std::env::temp_dir().join(format!("magpie-typed-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a file.txt"), "x").unwrap();
        let d = dir.to_string_lossy().to_string();

        assert_eq!(resolve(&d), Some((dir.clone(), true)));
        assert_eq!(resolve(&format!("\"{d}\"")), Some((dir.clone(), true)), "pasted with quotes");
        let (f, is_dir) = resolve(&format!("{d}{}a file.txt", std::path::MAIN_SEPARATOR)).unwrap();
        assert!(!is_dir && f.ends_with("a file.txt"));
        assert_eq!(resolve(&format!("{d}{}missing", std::path::MAIN_SEPARATOR)), None, "nothing there");

        let home = dirs::home_dir().unwrap();
        assert_eq!(resolve("~"), Some((home.clone(), true)));
        // $HOME works whether or not the variable is set (Windows rarely has it)
        assert_eq!(resolve("$HOME").map(|r| r.0.canonicalize().unwrap()), Some(home.canonicalize().unwrap()));

        std::env::set_var("MAGPIE_TYPED_TEST", &d);
        assert_eq!(resolve("%MAGPIE_TYPED_TEST%").map(|r| r.0), Some(dir.clone()));
        assert_eq!(resolve("$MAGPIE_TYPED_TEST").map(|r| r.0), Some(dir.clone()));
        assert_eq!(resolve("${MAGPIE_TYPED_TEST}").map(|r| r.0.canonicalize().unwrap()), Some(dir.canonicalize().unwrap()));
        assert_eq!(resolve("%MAGPIE_NO_SUCH_VAR%"), None);
        // a value holding % or $ is used as is, not expanded again
        std::env::set_var("MAGPIE_TYPED_LOOP", "%MAGPIE_TYPED_LOOP%$MAGPIE_TYPED_LOOP");
        assert_eq!(expand("%MAGPIE_TYPED_LOOP%").map(|p| p.to_string_lossy().contains("%MAGPIE_TYPED_LOOP%")), Some(true));
        assert!(expand("$MAGPIE_TYPED_LOOP/x").is_some());

        // not paths at all
        for q in ["", "chrome", "3+4", "vol 40", "~user", "%", "$", "hello world", "C", "sha256 abc"] {
            assert_eq!(resolve(q), None, "{q:?}");
        }
        #[cfg(windows)]
        {
            assert_eq!(resolve("C:").map(|r| r.1), Some(true), "a drive alone is its root");
            assert_eq!(resolve("C:/Windows").map(|r| r.0), Some(PathBuf::from("C:\\Windows")));
            assert_eq!(resolve("/Windows"), None, "no unix roots on Windows");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
