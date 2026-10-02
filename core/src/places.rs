//! Common places: where saved things usually land on this computer, for the
//! "Add common places" list in settings. Downloads, the desktop and the
//! screenshots folder everywhere; on Windows also the folders WeChat and QQ
//! keep received files in, which most people never find on their own. Only
//! places that exist are listed, and nothing is added without the user
//! ticking it: some of these hold tens of gigabytes.
//!
//! Chat apps on macOS keep their files inside their sandbox containers, and
//! merely looking there can raise a system prompt about another app's data,
//! so they are not probed there.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// The directories detection starts from. Filled from the OS by
/// [`Roots::from_os`]; tests pass their own.
#[derive(Debug, Default, Clone)]
pub struct Roots {
    pub downloads: Option<PathBuf>,
    pub desktop: Option<PathBuf>,
    pub screenshots: Option<PathBuf>,
    /// where WeChat and QQ put their data by default ("My Documents")
    pub documents: Option<PathBuf>,
    /// %APPDATA%, where WeChat records a storage folder the user moved
    pub appdata: Option<PathBuf>,
}

impl Roots {
    pub fn from_os() -> Self {
        // macOS saves screenshots to the desktop unless told otherwise; Windows
        // (Win+PrintScreen, the Snipping Tool) and GNOME use Pictures\Screenshots
        #[cfg(target_os = "macos")]
        let screenshots = macos_screenshot_dir().or_else(dirs::desktop_dir);
        #[cfg(not(target_os = "macos"))]
        let screenshots = dirs::picture_dir().map(|p| p.join("Screenshots"));
        Self {
            downloads: dirs::download_dir(),
            desktop: dirs::desktop_dir(),
            screenshots,
            documents: if cfg!(windows) { dirs::document_dir() } else { None },
            appdata: if cfg!(windows) { dirs::config_dir() } else { None },
        }
    }
}

/// Where `screencapture` saves, when the user changed it.
#[cfg(target_os = "macos")]
fn macos_screenshot_dir() -> Option<PathBuf> {
    let out = std::process::Command::new("defaults")
        .args(["read", "com.apple.screencapture", "location"])
        .output()
        .ok()?;
    let s = String::from_utf8(out.stdout).ok()?;
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let p = match s.strip_prefix("~/") {
        Some(rest) => dirs::home_dir()?.join(rest),
        None => PathBuf::from(s),
    };
    Some(p)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Place {
    /// downloads | desktop | screenshots | wechat | qq
    pub kind: &'static str,
    pub path: String,
}

/// The common places that exist, in a fixed order, each path once.
pub fn detect(r: &Roots) -> Vec<Place> {
    let mut out: Vec<Place> = Vec::new();
    let mut push = |kind: &'static str, p: &Path| {
        if !p.is_dir() {
            return;
        }
        let path = p.to_string_lossy().into_owned();
        if !out.iter().any(|x| same_path(&x.path, &path)) {
            out.push(Place { kind, path });
        }
    };
    for (kind, p) in [("downloads", &r.downloads), ("desktop", &r.desktop), ("screenshots", &r.screenshots)] {
        if let Some(p) = p {
            push(kind, p);
        }
    }
    for p in wechat_dirs(r) {
        push("wechat", &p);
    }
    for p in qq_dirs(r) {
        push("qq", &p);
    }
    out
}

fn same_path(a: &str, b: &str) -> bool {
    if cfg!(windows) || cfg!(target_os = "macos") {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

/// Subdirectories of `dir`, sorted by name; empty when it cannot be read.
fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// The folders WeChat stores data under: "My Documents" unless the user
/// moved it, which WeChat records in a small ini under %APPDATA% (WeChat 4:
/// `Tencent\xwechat\config\*.ini`; WeChat 3: `Tencent\WeChat\All
/// Users\config\3ebffe94.ini`). The ini holds either `MyDocument:` or the
/// chosen folder.
fn wechat_roots(r: &Roots) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = r.documents.iter().cloned().collect();
    if let Some(appdata) = &r.appdata {
        let mut inis: Vec<PathBuf> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(appdata.join("Tencent").join("xwechat").join("config")) {
            inis.extend(rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "ini")));
        }
        inis.push(appdata.join("Tencent").join("WeChat").join("All Users").join("config").join("3ebffe94.ini"));
        for ini in inis {
            let Ok(text) = std::fs::read_to_string(&ini) else { continue };
            let v = text.trim().trim_matches('\0');
            if v.is_empty() || v.starts_with("MyDocument") {
                continue;
            }
            let p = PathBuf::from(v);
            if p.is_dir() && !roots.contains(&p) {
                roots.push(p);
            }
        }
    }
    roots
}

/// Received files, one folder per signed-in account: WeChat 4 keeps them in
/// `xwechat_files\<account>\msg\file`, WeChat 3 in `WeChat
/// Files\<account>\FileStorage\File`.
fn wechat_dirs(r: &Roots) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in wechat_roots(r) {
        // the chosen folder may be the parent of xwechat_files or that folder
        let bases = [root.join("xwechat_files"), root.clone()];
        for base in bases.iter() {
            for acct in subdirs(base) {
                out.push(acct.join("msg").join("file"));
            }
        }
        for acct in subdirs(&root.join("WeChat Files")) {
            out.push(acct.join("FileStorage").join("File"));
        }
    }
    out.retain(|p| p.is_dir());
    out
}

/// Received files per QQ number: QQ NT keeps them in `Tencent
/// Files\<number>\nt_qq\nt_data\File`, classic QQ in `Tencent
/// Files\<number>\FileRecv`.
fn qq_dirs(r: &Roots) -> Vec<PathBuf> {
    let Some(docs) = &r.documents else { return Vec::new() };
    let mut out = Vec::new();
    for acct in subdirs(&docs.join("Tencent Files")) {
        out.push(acct.join("nt_qq").join("nt_data").join("File"));
        out.push(acct.join("FileRecv"));
    }
    out.retain(|p| p.is_dir());
    out
}

/// How a place relates to the folders already indexed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaceState {
    /// not indexed yet: can be added
    New,
    /// indexed as it is
    Added,
    /// inside an indexed folder, so already searched
    Covered,
    /// holds an indexed folder; adding it would index that twice
    Contains,
}

pub fn place_state(path: &str, folders: &[String]) -> PlaceState {
    let p = Path::new(path);
    let mut state = PlaceState::New;
    for f in folders {
        let f = Path::new(f);
        if p == f {
            return PlaceState::Added;
        }
        if p.starts_with(f) {
            state = PlaceState::Covered;
        } else if f.starts_with(p) && state == PlaceState::New {
            state = PlaceState::Contains;
        }
    }
    state
}

/// Files the index would walk under `dir` (same rules: hidden and gitignored
/// paths skipped), counting up to `cap`. True in the second slot when the
/// count stopped at the cap or ran out of `budget`.
pub fn count_files(dir: &Path, cap: usize, budget: std::time::Duration) -> (usize, bool) {
    let start = std::time::Instant::now();
    let walker = ignore::WalkBuilder::new(dir)
        .follow_links(false)
        .hidden(true)
        .git_ignore(true)
        .require_git(false)
        .git_global(false)
        .git_exclude(true)
        .build();
    let mut n = 0usize;
    for entry in walker.flatten() {
        if entry.file_type().is_some_and(|t| t.is_file()) {
            n += 1;
            if n >= cap || (n.is_multiple_of(256) && start.elapsed() > budget) {
                return (n, true);
            }
        }
    }
    (n, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("magpie-places-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn mk(p: &Path) {
        std::fs::create_dir_all(p).unwrap();
    }

    #[test]
    fn detects_what_exists_in_order() {
        let t = tmp("detect");
        let (dl, desk, shots, docs, appdata) =
            (t.join("Downloads"), t.join("Desktop"), t.join("Pictures/Screenshots"), t.join("Documents"), t.join("AppData"));
        mk(&dl);
        mk(&desk);
        // no screenshots folder: not listed
        mk(&docs.join("xwechat_files/wxid_a_1234/msg/file"));
        mk(&docs.join("xwechat_files/all_users/config")); // no msg/file: skipped
        mk(&docs.join("WeChat Files/wxid_old/FileStorage/File"));
        mk(&docs.join("Tencent Files/12345/nt_qq/nt_data/File"));
        mk(&docs.join("Tencent Files/67890/FileRecv"));
        mk(&docs.join("Tencent Files/nt_qq/global")); // not an account: no File
        let r = Roots { downloads: Some(dl.clone()), desktop: Some(desk.clone()), screenshots: Some(shots), documents: Some(docs.clone()), appdata: Some(appdata) };
        let got: Vec<(String, String)> = detect(&r)
            .into_iter()
            .map(|p| (p.kind.to_string(), p.path.replace('\\', "/").rsplit(&format!("magpie-places-detect-{}", std::process::id())).next().unwrap().to_string()))
            .collect();
        let want: Vec<(String, String)> = [
            ("downloads", "/Downloads"),
            ("desktop", "/Desktop"),
            ("wechat", "/Documents/xwechat_files/wxid_a_1234/msg/file"),
            ("wechat", "/Documents/WeChat Files/wxid_old/FileStorage/File"),
            ("qq", "/Documents/Tencent Files/12345/nt_qq/nt_data/File"),
            ("qq", "/Documents/Tencent Files/67890/FileRecv"),
        ]
        .iter()
        .map(|(k, p)| (k.to_string(), p.to_string()))
        .collect();
        assert_eq!(got, want);
        std::fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn a_moved_wechat_folder_is_read_from_its_ini() {
        let t = tmp("ini");
        let (docs, appdata, moved) = (t.join("Documents"), t.join("AppData"), t.join("D/WeChatData"));
        mk(&docs);
        mk(&moved.join("xwechat_files/wxid_b_99/msg/file"));
        let conf = appdata.join("Tencent").join("xwechat").join("config");
        mk(&conf);
        std::fs::write(conf.join("abc.ini"), moved.to_string_lossy().as_bytes()).unwrap();
        // a default one alongside: ignored
        std::fs::write(conf.join("def.ini"), "MyDocument:").unwrap();
        let r = Roots { documents: Some(docs), appdata: Some(appdata), ..Default::default() };
        let got = detect(&r);
        assert_eq!(got.len(), 1, "{got:?}");
        assert_eq!(got[0].kind, "wechat");
        assert!(got[0].path.replace('\\', "/").ends_with("D/WeChatData/xwechat_files/wxid_b_99/msg/file"), "{}", got[0].path);
        std::fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn nothing_there_lists_nothing() {
        assert!(detect(&Roots::default()).is_empty());
        let r = Roots { downloads: Some(PathBuf::from("/no/such/dir")), documents: Some(PathBuf::from("/no/such/docs")), ..Default::default() };
        assert!(detect(&r).is_empty());
    }

    #[test]
    fn the_same_folder_twice_is_listed_once() {
        let t = tmp("dup");
        let d = t.join("Desktop");
        mk(&d);
        // macOS saves screenshots to the desktop by default
        let r = Roots { desktop: Some(d.clone()), screenshots: Some(d), ..Default::default() };
        let got = detect(&r);
        assert_eq!(got.iter().map(|p| p.kind).collect::<Vec<_>>(), vec!["desktop"]);
        std::fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn state_against_the_indexed_folders() {
        let sep = std::path::MAIN_SEPARATOR;
        let j = |parts: &[&str]| parts.join(&sep.to_string());
        let folders = vec![j(&["", "home", "u", "Downloads"]), j(&["", "home", "u", "Documents", "proj"])];
        assert_eq!(place_state(&j(&["", "home", "u", "Downloads"]), &folders), PlaceState::Added);
        assert_eq!(place_state(&j(&["", "home", "u", "Downloads", "x"]), &folders), PlaceState::Covered);
        assert_eq!(place_state(&j(&["", "home", "u", "Documents"]), &folders), PlaceState::Contains);
        assert_eq!(place_state(&j(&["", "home", "u", "Desktop"]), &folders), PlaceState::New);
        // a name that merely starts the same is not inside
        assert_eq!(place_state(&j(&["", "home", "u", "Downloads2"]), &folders), PlaceState::New);
        assert_eq!(place_state("anything", &[]), PlaceState::New);
    }

    #[test]
    fn counts_like_the_index_and_stops_at_the_cap() {
        let t = tmp("count");
        for i in 0..5 {
            std::fs::write(t.join(format!("f{i}.txt")), "x").unwrap();
        }
        std::fs::write(t.join(".hidden"), "x").unwrap();
        mk(&t.join("sub"));
        std::fs::write(t.join("sub/g.txt"), "x").unwrap();
        let long = std::time::Duration::from_secs(10);
        assert_eq!(count_files(&t, 100, long), (6, false), "hidden file skipped");
        assert_eq!(count_files(&t, 3, long), (3, true));
        assert_eq!(count_files(&t.join("missing"), 100, long), (0, false));
        std::fs::remove_dir_all(&t).unwrap();
    }
}
