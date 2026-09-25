//! Where installed browsers keep their profiles, and how to read their
//! databases while the browser runs. Shared by bookmark and history indexing.
//!
//! No browser is hardcoded beyond a few stable names: each platform's data
//! roots are swept for the two layouts every browser uses. A Chromium profile
//! is a `Default` / `Profile N` dir holding `Bookmarks` or `History` (Helium,
//! Arc, Vivaldi, ...), or a data dir holding `Bookmarks` itself (Opera). A
//! Gecko browser is a dir with a `profiles.ini` (Firefox, LibreWolf, Zen,
//! Floorp, Waterfox, ...), whose profiles hold `places.sqlite`.

use anyhow::Result;
use rusqlite::Connection;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Engine {
    Chromium,
    Gecko,
}

/// One browser profile directory.
#[derive(Debug, Clone)]
pub(crate) struct Profile {
    /// Shown on each hit: "chrome", "helium", "librewolf", ...
    pub browser: String,
    pub dir: PathBuf,
    pub engine: Engine,
}

impl Profile {
    pub fn bookmarks_file(&self) -> PathBuf {
        match self.engine {
            Engine::Chromium => self.dir.join("Bookmarks"),
            Engine::Gecko => self.dir.join("places.sqlite"),
        }
    }

    pub fn history_file(&self) -> PathBuf {
        match self.engine {
            Engine::Chromium => self.dir.join("History"),
            Engine::Gecko => self.dir.join("places.sqlite"),
        }
    }
}

/// A data root to sweep two levels deep (`<root>/<browser>` and
/// `<root>/<vendor>/<browser>`). `hidden_only` limits the first level to
/// dot-dirs, for sweeping a Linux home without walking the user's files.
struct Sweep {
    root: PathBuf,
    hidden_only: bool,
}

impl Sweep {
    fn all(root: PathBuf) -> Self {
        Sweep { root, hidden_only: false }
    }
}

#[derive(Default)]
struct Roots {
    /// Stable names for the big three, checked first so the sweep, which
    /// would name them after their folder, finds them already taken.
    named: Vec<(&'static str, PathBuf)>,
    chromium: Vec<Sweep>,
    gecko: Vec<Sweep>,
}

/// Every browser profile on this machine.
pub(crate) fn profiles() -> Vec<Profile> {
    profiles_in(&platform_roots())
}

fn platform_roots() -> Roots {
    let mut r = Roots::default();
    let home = home_dir();
    #[cfg(target_os = "windows")]
    {
        let _ = &home;
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let local = PathBuf::from(local);
            r.named.push(("chrome", local.join("Google/Chrome/User Data")));
            r.named.push(("edge", local.join("Microsoft/Edge/User Data")));
            r.named.push(("brave", local.join("BraveSoftware/Brave-Browser/User Data")));
            r.chromium.push(Sweep::all(local));
        }
        if let Ok(roaming) = std::env::var("APPDATA") {
            let roaming = PathBuf::from(roaming);
            // Opera keeps its profile in Roaming, not Local
            r.chromium.push(Sweep::all(roaming.clone()));
            r.gecko.push(Sweep::all(roaming));
        }
    }
    #[cfg(target_os = "macos")]
    {
        let sup = home.join("Library/Application Support");
        r.named.push(("chrome", sup.join("Google/Chrome")));
        r.named.push(("edge", sup.join("Microsoft Edge")));
        r.named.push(("brave", sup.join("BraveSoftware/Brave-Browser")));
        r.chromium.push(Sweep::all(sup.clone()));
        r.gecko.push(Sweep::all(sup));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let cfg = home.join(".config");
        r.named.push(("chrome", cfg.join("google-chrome")));
        r.named.push(("chromium", cfg.join("chromium")));
        r.named.push(("edge", cfg.join("microsoft-edge")));
        r.named.push(("brave", cfg.join("BraveSoftware/Brave-Browser")));
        r.chromium.push(Sweep::all(cfg));
        // ~/.mozilla/firefox, ~/.librewolf, ~/.zen, ...
        r.gecko.push(Sweep { root: home.clone(), hidden_only: true });
        // Flatpak and Snap keep a home of their own per app
        for base in [home.join(".var/app"), home.join("snap")] {
            for app in subdirs(&base) {
                let app_home = if base.ends_with("snap") { app.join("common") } else { app };
                r.chromium.push(Sweep::all(app_home.join(".config")));
                r.chromium.push(Sweep::all(app_home.join("config")));
                r.gecko.push(Sweep { root: app_home, hidden_only: true });
            }
        }
    }
    r
}

fn profiles_in(roots: &Roots) -> Vec<Profile> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for (name, base) in &roots.named {
        push_chromium(name, base, true, &mut seen, &mut out);
    }
    for sweep in &roots.chromium {
        for dir in candidates(sweep) {
            let name = display_name(&dir);
            push_chromium(&name, &dir, false, &mut seen, &mut out);
            push_chromium(&name, &dir.join("User Data"), false, &mut seen, &mut out);
        }
    }
    for sweep in &roots.gecko {
        for dir in candidates(sweep) {
            push_gecko(&dir, &mut seen, &mut out);
        }
    }
    out
}

/// `<root>/<a>` and `<root>/<a>/<b>` directories, `<a>` filtered by the
/// sweep's `hidden_only`.
fn candidates(sweep: &Sweep) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for a in subdirs(&sweep.root) {
        let hidden = a.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.'));
        if sweep.hidden_only && !hidden {
            continue;
        }
        // the browser's own dir before its subdirs, so a profile is named
        // after the browser ("vivaldi"), not the folder inside ("user data")
        let inner = subdirs(&a);
        out.push(a);
        out.extend(inner);
    }
    out
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    v.sort(); // a stable order, so the same machine names browsers the same way
    v
}

/// Chromium profiles in a data dir: `Default` / `Profile N` subdirs holding
/// bookmarks or history, or the data dir itself when it holds `Bookmarks`
/// (Opera).
///
/// A swept dir (`known` false) counts only if some profile has `Bookmarks`:
/// apps that embed Chromium (WebView2, Electron) and throwaway automation
/// profiles keep a `History` too, but never bookmarks. Once it counts, a
/// profile with history and no bookmarks yet is read as well.
fn push_chromium(browser: &str, base: &Path, known: bool, seen: &mut HashSet<PathBuf>, out: &mut Vec<Profile>) {
    if !base.is_dir() {
        return;
    }
    let mut found: Vec<(PathBuf, bool)> = Vec::new(); // (dir, has bookmarks)
    if base.join("Bookmarks").is_file() {
        found.push((base.to_path_buf(), true));
    }
    for dir in subdirs(base) {
        let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name != "Default" && !name.starts_with("Profile ") {
            continue;
        }
        let bookmarks = dir.join("Bookmarks").is_file();
        if bookmarks || dir.join("History").is_file() {
            found.push((dir, bookmarks));
        }
    }
    if !known && !found.iter().any(|(_, b)| *b) {
        return;
    }
    for (dir, _) in found {
        if seen.insert(normalize(&dir)) {
            out.push(Profile { browser: browser.to_string(), dir, engine: Engine::Chromium });
        }
    }
}

/// A Gecko browser's profiles: those `profiles.ini` lists (relative or
/// absolute). Only when it lists none that exist are `Profiles/*` and `*`
/// scanned instead, so a profile removed from the ini (its files left
/// behind) is not indexed. Thunderbird shares the layout and is skipped.
fn push_gecko(base: &Path, seen: &mut HashSet<PathBuf>, out: &mut Vec<Profile>) {
    let Ok(bytes) = std::fs::read(base.join("profiles.ini")) else { return };
    // not always clean UTF-8 (a hand edit, an old ANSI path): read what
    // parses, and the directory scan below covers the rest
    let ini = String::from_utf8_lossy(&bytes);
    let browser = display_name(base);
    if browser.contains("thunderbird") {
        return;
    }
    let has_places = |d: &PathBuf| d.join("places.sqlite").is_file();
    let mut dirs: Vec<PathBuf> = ini_profile_paths(&ini)
        .into_iter()
        .map(|(path, relative)| if relative { base.join(path) } else { PathBuf::from(path) })
        .filter(has_places)
        .collect();
    if dirs.is_empty() {
        dirs.extend(subdirs(&base.join("Profiles")).into_iter().filter(has_places));
        dirs.extend(subdirs(base).into_iter().filter(has_places));
    }
    for dir in dirs {
        if seen.insert(normalize(&dir)) {
            out.push(Profile { browser: browser.clone(), dir, engine: Engine::Gecko });
        }
    }
}

/// `(Path, IsRelative)` of each `[Profile*]` section in a `profiles.ini`.
fn ini_profile_paths(ini: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut in_profile = false;
    let mut path: Option<String> = None;
    let mut relative = true;
    let mut flush = |path: &mut Option<String>, relative: &mut bool| {
        if let Some(p) = path.take() {
            out.push((p, *relative));
        }
        *relative = true;
    };
    for line in ini.trim_start_matches('\u{feff}').lines().map(str::trim) {
        if line.starts_with('[') {
            if in_profile {
                flush(&mut path, &mut relative);
            }
            in_profile = line.starts_with("[Profile");
        } else if in_profile {
            if let Some(v) = line.strip_prefix("Path=") {
                path = Some(v.trim().to_string());
            } else if let Some(v) = line.strip_prefix("IsRelative=") {
                relative = v.trim() != "0";
            }
        }
    }
    if in_profile {
        flush(&mut path, &mut relative);
    }
    out
}

/// Same dir however it was reached ("Profiles/x" from the ini, or found by
/// the scan): compare canonical paths where the OS can give one.
fn normalize(dir: &Path) -> PathBuf {
    std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf())
}

/// A browser's name from its data dir: lowercased, without a leading dot, the
/// last part of a reverse-DNS id, and without a release channel suffix.
/// `net.imput.helium` -> helium, `.librewolf` -> librewolf,
/// `Opera Stable` -> opera, `com.operasoftware.Opera` -> opera,
/// `Brave-Browser` -> brave.
fn display_name(dir: &Path) -> String {
    let raw = dir.file_name().and_then(|n| n.to_str()).unwrap_or("browser");
    let mut name = raw.trim_start_matches('.').to_lowercase();
    if name.matches('.').count() >= 2 {
        name = name.rsplit('.').next().unwrap_or(&name).to_string();
    }
    for suffix in [" stable", "-stable", "-browser"] {
        if let Some(s) = name.strip_suffix(suffix) {
            name = s.to_string();
        }
    }
    if name.is_empty() {
        "browser".into()
    } else {
        name
    }
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// A private copy of a browser's SQLite database, taken while the browser
/// runs (it keeps the file locked). The write-ahead log is copied too:
/// Firefox keeps its latest bookmarks and visits there until it checkpoints,
/// so a copy of the main file alone misses them. Removed on drop.
pub(crate) struct Snapshot {
    path: PathBuf,
}

impl Snapshot {
    pub fn of(src: &Path) -> Result<Self> {
        static N: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "magpie-{}-{}-{}.sqlite",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed),
            src.file_name().and_then(|n| n.to_str()).unwrap_or("db")
        ));
        let snap = Snapshot { path };
        std::fs::copy(src, &snap.path)?;
        let wal = sidecar(src, "-wal");
        if wal.is_file() {
            // best effort: without it the copy is older, not broken
            let _ = std::fs::copy(&wal, sidecar(&snap.path, "-wal"));
        }
        Ok(snap)
    }

    /// Opened read-write: it is our copy, and applying the log needs it.
    pub fn open(&self) -> Result<Connection> {
        Ok(Connection::open_with_flags(
            &self.path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?)
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        for p in [self.path.clone(), sidecar(&self.path, "-wal"), sidecar(&self.path, "-shm")] {
            let _ = std::fs::remove_file(p);
        }
    }
}

fn sidecar(db: &Path, suffix: &str) -> PathBuf {
    let mut s = db.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tree(PathBuf);
    impl Tree {
        fn new(tag: &str) -> Self {
            let root = std::env::temp_dir().join(format!("magpie-browsers-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            Tree(root)
        }
        fn file(&self, rel: &str, body: &str) -> &Self {
            let f = self.0.join(rel);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, body).unwrap();
            self
        }
        fn p(&self, rel: &str) -> PathBuf {
            self.0.join(rel)
        }
    }
    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn found(ps: &[Profile]) -> Vec<String> {
        let mut v: Vec<String> = ps
            .iter()
            .map(|p| format!("{}:{:?}:{}", p.browser, p.engine, p.dir.file_name().unwrap().to_string_lossy()))
            .collect();
        v.sort();
        v
    }

    const INI: &str = "[General]\nStartWithLastProfile=1\n\n[Profile1]\nName=work\nIsRelative=1\nPath=Profiles/abcd.work\n\n[Profile0]\nName=default\nIsRelative=1\nPath=Profiles/wxyz.default-release\nDefault=1\n\n[Install4F96D1932A9F858E]\nDefault=Profiles/wxyz.default-release\n";

    #[test]
    fn ini_lists_relative_and_absolute_profiles() {
        let got = ini_profile_paths("[Profile0]\nPath=Profiles/a.default\nIsRelative=1\n[Profile1]\nIsRelative=0\nPath=D:\\ff\\b\n[Install1]\nDefault=Profiles/a.default\n");
        assert_eq!(got, vec![("Profiles/a.default".to_string(), true), ("D:\\ff\\b".to_string(), false)]);
        assert!(ini_profile_paths("").is_empty());
        assert!(ini_profile_paths("[General]\nPath=x\n").is_empty(), "only [Profile*] sections count");
    }

    #[test]
    fn names_from_data_dirs() {
        for (dir, want) in [
            ("net.imput.helium", "helium"),
            (".librewolf", "librewolf"),
            ("Opera Stable", "opera"),
            ("Opera GX Stable", "opera gx"),
            ("com.operasoftware.Opera", "opera"),
            ("Floorp", "floorp"),
            ("zen", "zen"),
            ("Firefox", "firefox"),
            ("Brave-Browser", "brave"),
            ("YandexBrowser", "yandexbrowser"),
        ] {
            assert_eq!(display_name(Path::new(dir)), want, "{dir}");
        }
    }

    /// macOS: Application Support holds everything one or two levels down.
    #[test]
    fn macos_layout() {
        let t = Tree::new("mac");
        t.file("Google/Chrome/Default/Bookmarks", "{}")
            .file("Google/Chrome/Profile 2/History", "") // history but no bookmarks yet
            .file("Google/Chrome/Guest Profile/History", "") // not a user profile
            .file("net.imput.helium/Default/Bookmarks", "{}")
            .file("Arc/User Data/Default/Bookmarks", "{}")
            .file("Arc/User Data/Profile 1/History", "") // a browser's profile without bookmarks yet
            .file("com.example.app/EBWebView/Default/History", "") // an app embedding Chromium
            .file("Temp/puppeteer_dev_chrome_profile-x/Default/History", "") // an automation profile
            .file("com.operasoftware.Opera/Bookmarks", "{}") // profile is the data dir
            .file("librewolf/profiles.ini", INI)
            .file("librewolf/Profiles/wxyz.default-release/places.sqlite", "")
            .file("librewolf/Profiles/abcd.work/places.sqlite", "")
            .file("zen/profiles.ini", "[Profile0]\nPath=Profiles/z.Default (release)\nIsRelative=1\n")
            .file("zen/Profiles/z.Default (release)/places.sqlite", "")
            .file("Firefox/profiles.ini", "[Profile0]\nPath=Profiles/f.default\nIsRelative=1\n")
            .file("Firefox/Profiles/f.default/places.sqlite", "")
            .file("Thunderbird/profiles.ini", "[Profile0]\nPath=Profiles/t.default\nIsRelative=1\n")
            .file("Thunderbird/Profiles/t.default/places.sqlite", "")
            .file("Code/Preferences", "{}") // an Electron app: not a browser
            .file("Code/History/x", "")
            .file("Slack/Default/Preferences", "{}"); // no bookmarks or history
        let roots = Roots {
            named: vec![("chrome", t.p("Google/Chrome"))],
            chromium: vec![Sweep::all(t.0.clone())],
            gecko: vec![Sweep::all(t.0.clone())],
        };
        assert_eq!(
            found(&profiles_in(&roots)),
            vec![
                "arc:Chromium:Default",
                "arc:Chromium:Profile 1",
                "chrome:Chromium:Default",
                "chrome:Chromium:Profile 2",
                "firefox:Gecko:f.default",
                "helium:Chromium:Default",
                "librewolf:Gecko:abcd.work",
                "librewolf:Gecko:wxyz.default-release",
                "opera:Chromium:com.operasoftware.Opera",
                "zen:Gecko:z.Default (release)",
            ]
        );
    }

    /// Windows: Chromium in Local (Opera in Roaming), Gecko in Roaming, with
    /// Firefox one vendor level down (Mozilla/Firefox).
    #[test]
    fn windows_layout() {
        let t = Tree::new("win");
        t.file("Local/Google/Chrome/User Data/Default/Bookmarks", "{}")
            .file("Local/Vivaldi/User Data/Default/Bookmarks", "{}")
            .file("Local/com.dfine.magpie/EBWebView/Default/History", "") // WebView2 app data
            .file("Local/Mozilla/Firefox/Profiles/f.default/cache2/x", "") // cache only, no ini
            .file("Roaming/Opera Software/Opera Stable/Bookmarks", "{}")
            .file("Roaming/Mozilla/Firefox/profiles.ini", INI)
            .file("Roaming/Mozilla/Firefox/Profiles/wxyz.default-release/places.sqlite", "")
            .file("Roaming/Mozilla/Firefox/Profiles/stale.old/places.sqlite", "") // removed from the ini
            .file("Roaming/Tor Browser/profiles.ini", "[General]\nVersion=2\n") // lists none: scanned
            .file("Roaming/Tor Browser/Profiles/tor.default/places.sqlite", "")
            .file("Roaming/Floorp/profiles.ini", "[Profile0]\nPath=Profiles/p.default\nIsRelative=1\n")
            .file("Roaming/Floorp/Profiles/p.default/places.sqlite", "")
            .file("Roaming/Waterfox/profiles.ini", &format!("[Profile0]\nPath={}\nIsRelative=0\n", t.p("elsewhere/wf").display()))
            .file("elsewhere/wf/places.sqlite", "");
        let roots = Roots {
            named: vec![("chrome", t.p("Local/Google/Chrome/User Data"))],
            chromium: vec![Sweep::all(t.p("Local")), Sweep::all(t.p("Roaming"))],
            gecko: vec![Sweep::all(t.p("Roaming"))],
        };
        assert_eq!(
            found(&profiles_in(&roots)),
            vec![
                "chrome:Chromium:Default",
                "firefox:Gecko:wxyz.default-release",
                "floorp:Gecko:p.default",
                "opera:Chromium:Opera Stable",
                "tor browser:Gecko:tor.default",
                "vivaldi:Chromium:Default",
                "waterfox:Gecko:wf",
            ]
        );
    }

    /// Linux: dot-dirs in home (~/.mozilla/firefox is two levels down), and
    /// the home's other folders are never walked.
    #[test]
    fn linux_layout() {
        let t = Tree::new("linux");
        t.file(".mozilla/firefox/profiles.ini", "[Profile0]\nPath=x.default\nIsRelative=1\n")
            .file(".mozilla/firefox/x.default/places.sqlite", "")
            .file(".librewolf/profiles.ini", "[Profile0]\nPath=y.default\nIsRelative=1\n")
            .file(".librewolf/y.default/places.sqlite", "")
            .file("projects/fake/profiles.ini", "[Profile0]\nPath=p\nIsRelative=1\n")
            .file("projects/fake/p/places.sqlite", "")
            .file(".config/chromium/Default/Bookmarks", "{}")
            .file(".config/thorium/Default/Bookmarks", "{}")
            .file(".config/Code/Default/History", ""); // Electron: no bookmarks
        let roots = Roots {
            named: vec![("chromium", t.p(".config/chromium"))],
            chromium: vec![Sweep::all(t.p(".config"))],
            gecko: vec![Sweep { root: t.0.clone(), hidden_only: true }],
        };
        assert_eq!(
            found(&profiles_in(&roots)),
            vec![
                "chromium:Chromium:Default",
                "firefox:Gecko:x.default",
                "librewolf:Gecko:y.default",
                "thorium:Chromium:Default",
            ]
        );
    }

    #[test]
    fn missing_roots_and_broken_files_find_nothing() {
        let t = Tree::new("empty");
        t.file("Broken/profiles.ini", "\u{0}\u{1}garbage") // no profiles listed, none on disk
            .file("Half/Default/Bookmarks.bak", "");
        let roots = Roots {
            named: vec![("chrome", t.p("nope"))],
            chromium: vec![Sweep::all(t.p("nope")), Sweep::all(t.0.clone())],
            gecko: vec![Sweep::all(t.0.clone())],
        };
        assert!(profiles_in(&roots).is_empty());
    }

    /// profiles.ini that is not valid UTF-8 (or has a BOM and CRLFs) still
    /// yields its profiles, by the lossy parse or the directory scan.
    #[test]
    fn odd_ini_encodings_still_find_profiles() {
        let t = Tree::new("ini");
        std::fs::create_dir_all(t.p("waterfox/Profiles/w.default")).unwrap();
        std::fs::write(t.p("waterfox/profiles.ini"), [0xFFu8, 0xFE, b'[', 0xC3]).unwrap();
        t.file("waterfox/Profiles/w.default/places.sqlite", "")
            .file("zen/profiles.ini", "\u{FEFF}[Profile0]\r\nPath=Profiles/z.default\r\nIsRelative=1\r\n")
            .file("zen/Profiles/z.default/places.sqlite", "")
            .file("zen/Profiles/stale.old/places.sqlite", ""); // not in the ini: the ini was parsed
        let roots = Roots { gecko: vec![Sweep::all(t.0.clone())], ..Default::default() };
        assert_eq!(found(&profiles_in(&roots)), vec!["waterfox:Gecko:w.default", "zen:Gecko:z.default"]);
    }

    /// The copy includes rows that only exist in the write-ahead log, which
    /// is where a running Firefox keeps its latest bookmarks and visits.
    #[test]
    fn snapshot_reads_the_write_ahead_log() {
        let t = Tree::new("wal");
        let db = t.p("places.sqlite");
        let live = Connection::open(&db).unwrap();
        live.pragma_update(None, "journal_mode", "WAL").unwrap();
        live.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
        live.execute_batch("CREATE TABLE t(x); INSERT INTO t VALUES (1); PRAGMA wal_checkpoint(TRUNCATE); INSERT INTO t VALUES (2);").unwrap();
        assert!(sidecar(&db, "-wal").metadata().unwrap().len() > 0, "row 2 is only in the log");
        let path;
        {
            let snap = Snapshot::of(&db).unwrap();
            path = snap.path.clone();
            let n: i64 = snap.open().unwrap().query_row("SELECT count(*) FROM t", [], |r| r.get(0)).unwrap();
            assert_eq!(n, 2);
        }
        assert!(!path.exists() && !sidecar(&path, "-wal").exists() && !sidecar(&path, "-shm").exists(), "cleaned up");
        drop(live);
    }

    #[test]
    #[ignore = "diagnostic: prints the browser profiles found on this machine"]
    fn print_profiles() {
        let t = std::time::Instant::now();
        for p in profiles() {
            println!("{}\t{:?}\t{}", p.browser, p.engine, p.dir.display());
        }
        println!("in {:?}", t.elapsed());
    }
}
