//! Installed-application launcher: enumerates apps from OS-standard locations
//! and matches them by name. No indexing/DB — the list is small and rebuilt
//! on demand, then filtered per keystroke.

use anyhow::{anyhow, Result};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize)]
pub struct AppEntry {
    pub name: String,
    /// Path launched when chosen (a .lnk, .app bundle, or executable/desktop).
    pub target: String,
    /// Alternate names this app also answers to: the built-in zh↔en table,
    /// user-defined aliases, the app's own localized names (macOS bundles,
    /// Linux .desktop Name[xx]), and .desktop Keywords/GenericName.
    /// Matched like second names (pinyin included), scored slightly below.
    pub aliases: Vec<String>,
    pub score: f32,
    /// The app's names by language tag ("zh_CN", "en", …), for picking the
    /// one the interface language shows (see [`localize`]).
    #[serde(skip)]
    pub names: Vec<(String, String)>,
}

/// Language tags whose names are worth keeping: the interface speaks
/// English and Chinese, and names in other scripts only add noise to
/// matching.
fn wanted_lang(tag: &str) -> bool {
    let t = tag.to_ascii_lowercase();
    t.is_empty() || t.starts_with("en") || t.starts_with("zh")
}

/// Show each app under its name in the interface language when it has one
/// ("zh": 活动监视器 for Activity Monitor); the name it had joins the
/// aliases, so it still matches.
pub fn localize(apps: &mut [AppEntry], ui_lang: &str) {
    if ui_lang != "zh" {
        return;
    }
    const ORDER: [&str; 8] = ["zh_CN", "zh-Hans", "zh_Hans", "zh-CN", "zh", "zh_TW", "zh-Hant", "zh_HK"];
    for a in apps.iter_mut() {
        let pick = ORDER
            .iter()
            .find_map(|want| a.names.iter().find(|(tag, _)| tag.eq_ignore_ascii_case(want)))
            .map(|(_, n)| n.clone());
        if let Some(local) = pick {
            if local != a.name {
                let old = std::mem::replace(&mut a.name, local);
                a.aliases.retain(|x| x != &a.name);
                if !a.aliases.contains(&old) {
                    a.aliases.push(old);
                }
            }
        }
    }
}

/// Built-in bilingual name groups for apps whose Start-Menu/bundle name is in
/// one language while users type the other. An app whose name equals any
/// member (case-insensitive) gains every other member as an alias. Only
/// stable, well-known pairs live here — everything else is a user alias.
const NAME_GROUPS: &[&[&str]] = &[
    &["微信", "WeChat"],
    &["飞书", "Lark", "Feishu"],
    &["钉钉", "DingTalk"],
    &["企业微信", "WeCom", "WeChat Work"],
    &["腾讯会议", "Tencent Meeting", "VooV Meeting"],
    &["腾讯文档", "Tencent Docs"],
    &["网易云音乐", "NetEase Cloud Music"],
    &["QQ音乐", "QQ Music"],
    &["酷狗音乐", "KuGou"],
    &["百度网盘", "Baidu Netdisk"],
    &["阿里云盘", "Aliyun Drive"],
    &["夸克", "Quark"],
    &["迅雷", "Thunder", "Xunlei"],
    &["爱奇艺", "iQIYI"],
    &["哔哩哔哩", "bilibili", "B站"],
    &["优酷", "Youku"],
    &["腾讯视频", "Tencent Video"],
    &["抖音", "Douyin"],
    &["剪映", "CapCut", "JianYing"],
    &["小红书", "RedNote", "Xiaohongshu"],
    &["有道词典", "Youdao Dictionary"],
    &["搜狗输入法", "Sogou Input"],
    &["美图秀秀", "Meitu"],
    &["金山文档", "KDocs"],
    &["向日葵", "Sunlogin"],
    &["石墨文档", "Shimo Docs"],
    &["语雀", "Yuque"],
    &["Visual Studio Code", "VS Code", "VSCode"],
    &["Google Chrome", "Chrome", "谷歌浏览器"],
    &["Microsoft Edge", "Edge"],
];

/// Aliases the built-in table grants a given app name.
fn builtin_aliases(name: &str) -> Vec<String> {
    let ln = name.trim().to_lowercase();
    for group in NAME_GROUPS {
        if group.iter().any(|m| m.to_lowercase() == ln) {
            return group
                .iter()
                .filter(|m| m.to_lowercase() != ln)
                .map(|m| m.to_string())
                .collect();
        }
    }
    Vec::new()
}

/// Apply user alias rules ("proxy = Clash for Windows": alias → app-name
/// substring) on top of whatever aliases the entries already carry.
pub fn apply_user_aliases(apps: &mut [AppEntry], rules: &[(String, String)]) {
    for (alias, target) in rules {
        let (alias, tgt) = (alias.trim(), target.trim().to_lowercase());
        if alias.is_empty() || tgt.is_empty() {
            continue;
        }
        for a in apps.iter_mut() {
            if a.name.to_lowercase().contains(&tgt)
                && !a.aliases.iter().any(|x| x.eq_ignore_ascii_case(alias))
            {
                a.aliases.push(alias.to_string());
            }
        }
    }
}

/// Parse "alias = app name" lines (one per line; '#' comments allowed).
pub fn parse_alias_rules(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() || l.starts_with('#') {
                return None;
            }
            let (a, t) = l.split_once('=')?;
            let (a, t) = (a.trim(), t.trim());
            (!a.is_empty() && !t.is_empty()).then(|| (a.to_string(), t.to_string()))
        })
        .collect()
}

/// Enumerate installed applications from platform-standard locations.
pub fn list_apps() -> Vec<AppEntry> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut push = |name: String, target: PathBuf, extra: Vec<String>, names: Vec<(String, String)>| {
        let key = name.to_lowercase();
        if !name.is_empty() && seen.insert(key) {
            let mut aliases = builtin_aliases(&name);
            let localized = names.iter().map(|(_, n)| n.clone());
            for k in extra.into_iter().chain(localized) {
                if !k.is_empty() && !k.eq_ignore_ascii_case(&name) && !aliases.iter().any(|x| x.eq_ignore_ascii_case(&k)) {
                    aliases.push(k);
                }
            }
            out.push(AppEntry {
                name,
                target: target.to_string_lossy().into_owned(),
                aliases,
                score: 0.0,
                names,
            });
        }
    };

    #[cfg(target_os = "windows")]
    {
        let roots = [std::env::var("ProgramData").ok(), std::env::var("APPDATA").ok()];
        for root in roots.into_iter().flatten() {
            let start = PathBuf::from(root).join("Microsoft/Windows/Start Menu/Programs");
            for entry in walk(&start, "lnk") {
                if let Some(stem) = entry.file_stem().and_then(|s| s.to_str()) {
                    push(stem.to_string(), entry.clone(), Vec::new(), Vec::new());
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").map(PathBuf::from).unwrap_or_default();
        let dirs = [
            PathBuf::from("/Applications"),
            PathBuf::from("/System/Applications"),
            home.join("Applications"),
        ];
        for dir in dirs {
            for p in find_bundles(&dir, APP_DIR_DEPTH) {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    let names = bundle_names(&p);
                    push(stem.to_string(), p.clone(), Vec::new(), names);
                }
            }
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let home = std::env::var("HOME").map(PathBuf::from).unwrap_or_default();
        let dirs = [
            PathBuf::from("/usr/share/applications"),
            PathBuf::from("/usr/local/share/applications"),
            home.join(".local/share/applications"),
        ];
        for dir in dirs {
            for entry in walk(&dir, "desktop") {
                if let Some(d) = parse_desktop(&entry) {
                    push(d.name, entry.clone(), d.keywords, d.names);
                }
            }
        }
    }
    out
}

/// How deep below an applications folder bundles are looked for. Two covers
/// `/System/Applications/Utilities/Activity Monitor.app` and a vendor folder
/// such as `/Applications/Adobe Photoshop/…`; three leaves room for a folder
/// the user made inside one of those.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
const APP_DIR_DEPTH: usize = 3;

/// Every `.app` bundle under `dir`, looking into plain folders up to `depth`
/// levels down but never into a bundle (apps nest helpers inside themselves).
/// issue #4: only the top level was read, so everything in Utilities was
/// missing, Activity Monitor included.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn find_bundles(dir: &std::path::Path, depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort(); // stable order, so the first of two same-named apps wins every time
    for p in entries {
        if p.extension().and_then(|x| x.to_str()) == Some("app") {
            out.push(p);
        } else if depth > 1 && p.is_dir() && !is_symlink(&p) {
            out.extend(find_bundles(&p, depth - 1));
        }
    }
    out
}

#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn is_symlink(p: &std::path::Path) -> bool {
    std::fs::symlink_metadata(p).map(|m| m.file_type().is_symlink()).unwrap_or(false)
}

/// A macOS bundle's own names by language: `CFBundleDisplayName` (else
/// `CFBundleName`) from `Info.plist` (tag ""), from `InfoPlist.loctable`
/// (one file with every language, what current system apps ship) and from
/// each `<lang>.lproj/InfoPlist.strings`. Only English and Chinese are kept.
/// This is how "活动监视器" finds `Activity Monitor.app`.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn bundle_names(bundle: &std::path::Path) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut add = |tag: &str, dict: &plist::Dictionary| {
        let name = ["CFBundleDisplayName", "CFBundleName"]
            .iter()
            .find_map(|k| dict.get(k).and_then(|v| v.as_string()))
            .map(str::trim)
            .filter(|n| !n.is_empty());
        if let Some(n) = name {
            if wanted_lang(tag) && !out.iter().any(|(t, x)| t == tag && x == n) {
                out.push((tag.to_string(), n.to_string()));
            }
        }
    };
    let contents = bundle.join("Contents");
    if let Ok(v) = plist::Value::from_file(contents.join("Info.plist")) {
        if let Some(d) = v.as_dictionary() {
            add("", d);
        }
    }
    let resources = contents.join("Resources");
    if let Ok(v) = plist::Value::from_file(resources.join("InfoPlist.loctable")) {
        if let Some(langs) = v.as_dictionary() {
            for (tag, v) in langs {
                if let Some(d) = v.as_dictionary() {
                    add(tag, d);
                }
            }
        }
    }
    if let Ok(dirs) = std::fs::read_dir(&resources) {
        for e in dirs.flatten() {
            let p = e.path();
            let Some(tag) = p.file_name().and_then(|n| n.to_str()).and_then(|n| n.strip_suffix(".lproj")) else {
                continue;
            };
            if !wanted_lang(tag) {
                continue;
            }
            if let Some(d) = read_strings(&p.join("InfoPlist.strings")) {
                add(tag, &d);
            }
        }
    }
    out
}

/// A `.strings` file as a dictionary: compiled (binary plist) or the text
/// form (`"key" = "value";`, UTF-8 or UTF-16 with a byte-order mark).
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn read_strings(path: &std::path::Path) -> Option<plist::Dictionary> {
    let bytes = std::fs::read(path).ok()?;
    if let Ok(plist::Value::Dictionary(d)) = plist::Value::from_reader(std::io::Cursor::new(&bytes)) {
        return Some(d);
    }
    let text = match bytes.as_slice() {
        [0xFF, 0xFE, rest @ ..] => String::from_utf16_lossy(
            &rest.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect::<Vec<_>>(),
        ),
        [0xFE, 0xFF, rest @ ..] => String::from_utf16_lossy(
            &rest.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect::<Vec<_>>(),
        ),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        all => String::from_utf8_lossy(all).into_owned(),
    };
    let mut d = plist::Dictionary::new();
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        let key = k.trim().trim_matches('"');
        let v = v.trim();
        let (Some(start), Some(end)) = (v.find('"'), v.rfind('"')) else { continue };
        if end <= start || key.is_empty() || key.starts_with("//") || key.starts_with("/*") {
            continue;
        }
        let value = v[start + 1..end].replace("\\\"", "\"").replace("\\\\", "\\");
        d.insert(key.to_string(), plist::Value::String(value));
    }
    (!d.is_empty()).then_some(d)
}

/// Rank apps against a query. Prefix match beats substring beats subsequence.
/// With `use_pinyin`, a latin query also matches Chinese names by full pinyin
/// or initials ("wx" / "weixin" -> 微信), ranked below same-script matches.
pub fn match_apps(apps: &[AppEntry], query: &str, limit: usize, use_pinyin: bool) -> Vec<AppEntry> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    // one name, one score — reused for the app's real name and every alias
    let score_name = |raw: &str| -> Option<f32> {
        let name = raw.to_lowercase();
        if name == q {
            Some(1.0)
        } else if name.starts_with(&q) {
            Some(0.9 - 0.001 * name.len() as f32) // shorter prefix match ranks higher
        } else if matches_word_start(&q, raw) {
            Some(0.75) // "mac" -> "MyMacCleaner", "code" -> "Visual Studio Code"
        } else if matches_initials(&q, raw) {
            // "vsc" -> "Visual Studio Code", "mt" -> "MacTap". A whole acronym
            // says more than letters that happen to sit inside a word: below
            // it, "mpt" lost its app to six "Command Prompt" shortcuts
            Some(0.7)
        } else if name.contains(&q) {
            Some(0.6)
        } else if use_pinyin {
            match_pinyin(&q, raw)
        } else {
            None
        }
    };
    let mut scored: Vec<AppEntry> = apps
        .iter()
        .filter_map(|a| {
            let own = score_name(&a.name);
            // aliases are second names, ranked a notch below the real one
            let via_alias = a.aliases.iter().filter_map(|al| score_name(al)).fold(None::<f32>, |m, s| {
                Some(m.map_or(s, |m| m.max(s)))
            });
            let score = match (own, via_alias) {
                (Some(o), Some(al)) => o.max(al * 0.95),
                (Some(o), None) => o,
                (None, Some(al)) => al * 0.95,
                (None, None) => return None,
            };
            let mut e = a.clone();
            e.score = score;
            Some(e)
        })
        .collect();
    scored.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.name.len().cmp(&b.name.len())));
    scored.truncate(limit);
    scored
}

/// Pinyin match for names containing Han characters. Each Han char may be
/// spelled in the query as any of its readings (heteronyms included) or their
/// first letter, so one walk covers full pinyin ("weixin"), initials ("wx"),
/// and mixes ("weix"). ASCII chars must match themselves; separators may be
/// skipped. Returns a score below same-script prefix/substring matches, or
/// None when the name has no Han chars / nothing lines up.
fn match_pinyin(q: &str, name: &str) -> Option<f32> {
    use pinyin::ToPinyinMulti;
    // guard: query must be latin (a Han query is matched directly upstream)
    // and 1-letter queries would light up every app sharing one initial
    if q.len() < 2 || !q.is_ascii() {
        return None;
    }
    let mut opts: Vec<Vec<String>> = Vec::new();
    let mut has_han = false;
    for c in name.chars() {
        if let Some(multi) = c.to_pinyin_multi() {
            has_han = true;
            let mut v: Vec<String> = Vec::new();
            for p in multi {
                let plain = p.plain().to_string();
                let first = p.first_letter().to_string();
                if !v.contains(&first) {
                    v.push(first);
                }
                if !v.contains(&plain) {
                    v.push(plain);
                }
            }
            opts.push(v);
        } else if c.is_ascii_alphanumeric() {
            opts.push(vec![c.to_ascii_lowercase().to_string()]);
        } else {
            opts.push(vec![String::new()]); // separator/punctuation: skippable
        }
    }
    if !has_han {
        return None;
    }
    let qb = q.as_bytes();
    for start in 0..opts.len() {
        if pinyin_walk(qb, 0, &opts, start) {
            // start-of-name pinyin beats mid-name, both stay below native hits
            return Some(if start == 0 { 0.8 - 0.001 * name.chars().count() as f32 } else { 0.55 });
        }
    }
    None
}

/// Can query bytes from `qi` be consumed by per-char spellings from `ci` on?
/// Chars are consumed in order; "" options (separators) consume nothing.
fn pinyin_walk(q: &[u8], qi: usize, opts: &[Vec<String>], ci: usize) -> bool {
    if qi == q.len() {
        return true;
    }
    if ci == opts.len() {
        return false;
    }
    for o in &opts[ci] {
        let ob = o.as_bytes();
        if q[qi..].starts_with(ob) && pinyin_walk(q, qi + ob.len(), opts, ci + 1) {
            return true;
        }
    }
    false
}

/// Where the words of a name start, char by char: the first letter or digit,
/// any one after a separator, and a capital that follows a lowercase letter
/// (camelCase: My|Mac|Cleaner, Mac|Tap). A run of capitals stays one word,
/// so "OBS" or "VLC" never splits into letters. Works on the name as
/// written: lowercasing first would erase the humps.
fn word_starts(raw: &str) -> Vec<bool> {
    let chars: Vec<char> = raw.chars().collect();
    (0..chars.len())
        .map(|i| {
            let c = chars[i];
            c.is_alphanumeric()
                && match i.checked_sub(1).map(|j| chars[j]) {
                    None => true,
                    Some(p) if !p.is_alphanumeric() => true,
                    Some(p) => p.is_lowercase() && c.is_uppercase(),
                }
        })
        .collect()
}

/// One lowercase char per char of `raw`, so indices line up with
/// [`word_starts`] (a char whose lowercase is several chars keeps its first).
fn lower_chars(raw: &str) -> Vec<char> {
    raw.chars().map(|c| c.to_lowercase().next().unwrap_or(c)).collect()
}

/// Does `q` (lowercase) start at a word inside the name, past its first
/// word? "mac" in "MyMacCleaner", "code" in "Visual Studio Code". The
/// start of the name itself is the plain prefix match, scored above this.
fn matches_word_start(q: &str, raw: &str) -> bool {
    let qc: Vec<char> = q.chars().collect();
    if qc.is_empty() {
        return false; // an empty query starts every word; it means nothing
    }
    let lc = lower_chars(raw);
    let starts = word_starts(raw);
    (1..lc.len()).any(|i| starts[i] && lc[i..].starts_with(&qc))
}

/// Acronym match: does `q` spell out the initials of the words in `name`?
/// "vsc" matches "Visual Studio Code", "mt" matches "MacTap"; "code" does
/// NOT match "RecoveryDrive". Words as [`word_starts`] sees them.
fn matches_initials(q: &str, raw: &str) -> bool {
    let lc = lower_chars(raw);
    let initials: String = word_starts(raw)
        .into_iter()
        .zip(lc)
        .filter_map(|(start, c)| start.then_some(c))
        .collect();
    q.chars().count() >= 2 && initials.starts_with(q)
}

/// Launch an application by the target recorded in [`AppEntry`].
pub fn launch_app(target: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        // ShellExecute via `cmd start` resolves .lnk targets and arguments.
        // cmd is a console program and the app a GUI one, so without
        // CREATE_NO_WINDOW every launch would flash a console window; `start`
        // still gives a console target its own window.
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .args(["/c", "start", "", target])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .spawn()
            .map_err(|e| anyhow!("launch: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(target)
            .spawn()
            .map_err(|e| anyhow!("launch: {e}"))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let exec = parse_desktop(std::path::Path::new(target))
            .map(|d| d.exec)
            .unwrap_or_else(|| target.to_string());
        // strip .desktop field codes (%u, %F, ...) and run the first token
        let cleaned: Vec<String> = exec
            .split_whitespace()
            .filter(|t| !t.starts_with('%'))
            .map(String::from)
            .collect();
        if let Some((cmd, args)) = cleaned.split_first() {
            std::process::Command::new(cmd)
                .args(args)
                .spawn()
                .map_err(|e| anyhow!("launch: {e}"))?;
        } else {
            return Err(anyhow!("no exec in {target}"));
        }
    }
    Ok(())
}

/// An application's icon, ready to hand to the webview as a data URL.
#[derive(Debug, Clone)]
pub struct Icon {
    /// `image/png`, or `image/svg+xml` for a Linux theme icon shipped as SVG
    pub mime: &'static str,
    pub bytes: Vec<u8>,
}

/// The icon the OS shows for an app target from [`list_apps`], `px` pixels
/// square (raster icons; an SVG is returned as is). None when the OS has no
/// icon for it or it cannot be read; the palette then shows no icon.
///
/// Threading: any worker thread will do. On macOS the AppKit calls used here
/// are safe off the main thread (CI checks that under the Main Thread
/// Checker, see tests/app_icons_main.rs). On Windows it initializes COM for
/// the calling thread and releases it again.
pub fn icon(target: &str, px: u32) -> Option<Icon> {
    #[cfg(target_os = "windows")]
    {
        let img = windows_icon::rgba(target, px)?;
        png_icon(&img)
    }
    #[cfg(target_os = "macos")]
    {
        macos_icon::png(target, px).map(|bytes| Icon { mime: "image/png", bytes })
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        linux_icon(target, px)
    }
}

/// What a cached icon's validity hangs on: the app's modification time, in
/// seconds. For a macOS bundle that is `Contents/Info.plist`, which every
/// update rewrites (the bundle directory's own time does not move);
/// elsewhere the Start Menu shortcut or the .desktop file. 0 when it cannot
/// be read, and a 0 stamp never counts as a cache hit.
pub fn icon_stamp(target: &str) -> i64 {
    let path = std::path::Path::new(target);
    let plist = path.join("Contents").join("Info.plist");
    let file = if plist.is_file() { plist } else { path.to_path_buf() };
    std::fs::metadata(file)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// One remembered icon: the stamp it was read at, and the icon (None when
/// the OS had none, which is worth remembering too).
pub type CachedIcon = (i64, Option<Icon>);

/// Every icon remembered from earlier launches, by launch target. Reading an
/// icon costs 10 to 60 ms on Windows and up to a second on macOS, so a
/// launch only reads the apps that are new or changed since.
pub fn cached_icons(conn: &rusqlite::Connection) -> Result<std::collections::HashMap<String, CachedIcon>> {
    let mut stmt = conn.prepare("SELECT target, stamp, mime, image FROM app_icons")?;
    let rows = stmt.query_map([], |r| {
        let mime: Option<String> = r.get(2)?;
        let image: Option<Vec<u8>> = r.get(3)?;
        // only the two types `icon` produces; anything else reads as none
        let mime: Option<&'static str> = match mime.as_deref() {
            Some("image/png") => Some("image/png"),
            Some("image/svg+xml") => Some("image/svg+xml"),
            _ => None,
        };
        let icon = match (mime, image) {
            (Some(mime), Some(bytes)) => Some(Icon { mime, bytes }),
            _ => None,
        };
        Ok((r.get::<_, String>(0)?, (r.get::<_, i64>(1)?, icon)))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn store_icon(conn: &rusqlite::Connection, target: &str, stamp: i64, icon: Option<&Icon>) -> Result<()> {
    conn.execute(
        "INSERT INTO app_icons (target, stamp, mime, image) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(target) DO UPDATE SET stamp = ?2, mime = ?3, image = ?4",
        rusqlite::params![target, stamp, icon.map(|i| i.mime), icon.map(|i| i.bytes.as_slice())],
    )?;
    Ok(())
}

/// Forget icons of apps that are no longer installed. Returns how many.
pub fn prune_icons(conn: &rusqlite::Connection, installed: &std::collections::HashSet<String>) -> Result<usize> {
    let known: Vec<String> = conn
        .prepare("SELECT target FROM app_icons")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut gone = 0;
    for t in known.iter().filter(|t| !installed.contains(*t)) {
        gone += conn.execute("DELETE FROM app_icons WHERE target = ?1", [t])?;
    }
    Ok(gone)
}

#[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
fn png_icon(img: &image::RgbaImage) -> Option<Icon> {
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(Icon { mime: "image/png", bytes: out.into_inner() })
}

/// Undo premultiplied alpha, in place. Shell bitmaps come premultiplied;
/// a PNG wants straight alpha, or every soft edge renders too dark. A pixel
/// whose colour exceeds its alpha proves the data was straight all along,
/// and then nothing is touched.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
fn unpremultiply(rgba: &mut [u8]) {
    let premultiplied = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]);
    if !premultiplied {
        return;
    }
    for p in rgba.as_chunks_mut::<4>().0 {
        let a = p[3] as u32;
        if a > 0 && a < 255 {
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}

#[cfg(target_os = "windows")]
mod windows_icon {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
    };
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY};

    /// The shell's own icon for a path (a Start Menu .lnk resolves to the
    /// program's icon, without the shortcut arrow), as straight RGBA.
    pub fn rgba(path: &str, px: u32) -> Option<image::RgbaImage> {
        // SAFETY: plain Win32 calls; every handle obtained here is released
        // before returning, and the COM init is paired with its uninit.
        unsafe {
            let inited = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
            let out = read(path, px);
            if inited {
                CoUninitialize();
            }
            out
        }
    }

    unsafe fn read(path: &str, px: u32) -> Option<image::RgbaImage> {
        // the shell's parser rejects forward slashes, and list_apps builds
        // Start Menu paths with them
        let path = path.replace('/', "\\");
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(&HSTRING::from(path.as_str()), None).ok()?;
        let side = px as i32;
        let hbm = factory.GetImage(SIZE { cx: side, cy: side }, SIIGBF_ICONONLY).ok()?;
        let obj = HGDIOBJ(hbm.0);
        let pixels = (|| {
            let mut bm = BITMAP::default();
            let n = GetObjectW(
                obj,
                std::mem::size_of::<BITMAP>() as i32,
                Some(&mut bm as *mut BITMAP as *mut _),
            );
            if n == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 {
                return None;
            }
            let (w, h) = (bm.bmWidth, bm.bmHeight);
            let mut info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // negative: rows top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut buf = vec![0u8; (w * h * 4) as usize];
            let dc = GetDC(None);
            let lines = GetDIBits(
                dc,
                hbm,
                0,
                h as u32,
                Some(buf.as_mut_ptr() as *mut _),
                &mut info,
                DIB_RGB_COLORS,
            );
            ReleaseDC(None, dc);
            if lines == 0 {
                return None;
            }
            // BGRA → RGBA
            for p in buf.as_chunks_mut::<4>().0 {
                p.swap(0, 2);
            }
            // a bitmap without an alpha channel reads back all-zero alpha
            if buf.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
                for p in buf.as_chunks_mut::<4>().0 {
                    p[3] = 255;
                }
            } else {
                super::unpremultiply(&mut buf);
            }
            image::RgbaImage::from_raw(w as u32, h as u32, buf)
        })();
        let _ = DeleteObject(obj);
        pixels
    }
}

#[cfg(target_os = "macos")]
mod macos_icon {
    use objc2::AllocAnyThread;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSBitmapImageRepPropertyKey, NSWorkspace};
    use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

    /// The Finder's icon for a bundle, rendered to a `px`-point PNG. AppKit
    /// picks the best representation, Assets.car icons included, which
    /// reading the bundle's .icns by hand would miss. Any thread.
    pub fn png(path: &str, px: u32) -> Option<Vec<u8>> {
        objc2::rc::autoreleasepool(|_| {
            let icon = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(path));
            let side = px as f64;
            let mut rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(side, side));
            // SAFETY: `rect` is a live NSRect; no context or hints are passed
            let cg = unsafe { icon.CGImageForProposedRect_context_hints(&mut rect, None, None) }?;
            let rep = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &cg);
            let props = NSDictionary::<NSBitmapImageRepPropertyKey, objc2::runtime::AnyObject>::new();
            // SAFETY: an empty properties dictionary is always valid
            let data = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &props) }?;
            Some(data.to_vec())
        })
    }
}

/// A .desktop entry's `Icon=`: an absolute file, or a name looked up the
/// way desktops do (the hicolor theme at common sizes, then pixmaps).
/// The icon a .desktop file names: an absolute path, or a theme icon name.
/// Only the main entry's `Icon=` counts (the first one; localized `Icon[xx]=`
/// keys do not match). The spec wants a bare theme name, but `Icon=foo.png`
/// is common in the wild, so a trailing image extension is dropped.
#[cfg_attr(not(any(all(unix, not(target_os = "macos")), test)), allow(dead_code))]
fn desktop_icon_name(desktop: &str) -> Option<String> {
    let name = desktop
        .lines()
        .find_map(|l| l.strip_prefix("Icon="))
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    if name.starts_with('/') {
        return Some(name.to_string());
    }
    let bare = ["png", "svg", "xpm"]
        .iter()
        .find_map(|ext| name.strip_suffix(&format!(".{ext}")))
        .unwrap_or(name);
    Some(bare.to_string())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn linux_icon(target: &str, px: u32) -> Option<Icon> {
    let text = std::fs::read_to_string(target).ok()?;
    let name = desktop_icon_name(&text)?;
    let file = if name.starts_with('/') {
        Some(PathBuf::from(name))
    } else {
        icon_theme_file(&name)
    }?;
    let bytes = std::fs::read(&file).ok()?;
    if file.extension().and_then(|e| e.to_str()) == Some("svg") {
        return Some(Icon { mime: "image/svg+xml", bytes });
    }
    let img = image::load_from_memory(&bytes).ok()?;
    png_icon(&img.thumbnail(px, px).to_rgba8())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn icon_theme_file(name: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").map(PathBuf::from).unwrap_or_default();
    let mut bases = vec![home.join(".local/share/icons")];
    let data_dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    bases.extend(data_dirs.split(':').filter(|d| !d.is_empty()).map(|d| PathBuf::from(d).join("icons")));
    for base in &bases {
        for size in ["64x64", "48x48", "128x128", "256x256", "96x96", "32x32", "scalable"] {
            for ext in ["png", "svg"] {
                let p = base.join("hicolor").join(size).join("apps").join(format!("{name}.{ext}"));
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    // xpm is left out: the webview cannot render it and `image` cannot read it
    ["png", "svg"]
        .iter()
        .map(|ext| PathBuf::from(format!("/usr/share/pixmaps/{name}.{ext}")))
        .find(|p| p.is_file())
}

#[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
fn walk(dir: &std::path::Path, ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|x| x.to_str()) == Some(ext) {
                out.push(p);
            }
        }
    }
    out
}

#[cfg(all(unix, not(target_os = "macos")))]
struct DesktopEntry {
    name: String,
    exec: String,
    /// Keywords= and GenericName= — free aliases the desktop file ships with.
    keywords: Vec<String>,
    /// Name[xx]= by language tag (English and Chinese only).
    names: Vec<(String, String)>,
}

#[cfg(all(unix, not(target_os = "macos")))]
fn parse_desktop(path: &std::path::Path) -> Option<DesktopEntry> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut name = None;
    let mut exec = None;
    let mut keywords: Vec<String> = Vec::new();
    let mut names: Vec<(String, String)> = Vec::new();
    let mut no_display = false;
    for line in text.lines() {
        // the main entry ends where the first action section begins
        if line.starts_with('[') && line.trim() != "[Desktop Entry]" {
            break;
        }
        if let Some((tag, v)) = line
            .strip_prefix("Name[")
            .and_then(|r| r.split_once("]="))
        {
            let v = v.trim();
            if wanted_lang(tag) && !v.is_empty() {
                names.push((tag.to_string(), v.to_string()));
            }
        } else if let Some(v) = line.strip_prefix("Name=") {
            name.get_or_insert_with(|| v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("Exec=") {
            exec.get_or_insert_with(|| v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("Keywords=") {
            keywords.extend(v.split(';').map(|k| k.trim().to_string()).filter(|k| !k.is_empty()));
        } else if let Some(v) = line.strip_prefix("GenericName=") {
            let v = v.trim();
            if !v.is_empty() {
                keywords.push(v.to_string());
            }
        } else if line.strip_prefix("NoDisplay=").map(|v| v.trim() == "true").unwrap_or(false) {
            no_display = true;
        }
    }
    if no_display {
        return None;
    }
    Some(DesktopEntry { name: name?, exec: exec?, keywords, names })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str) -> AppEntry {
        AppEntry {
            name: name.into(),
            target: format!("/x/{name}"),
            aliases: builtin_aliases(name),
            ..Default::default()
        }
    }

    #[test]
    fn ranks_exact_then_word_start_then_substring() {
        let apps = vec![app("Visual Studio Code"), app("Code"), app("QR Code Reader"), app("Xcode")];
        let hits = match_apps(&apps, "code", 10, true);
        let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
        // exact, then "code" starting a word (shorter name first), then
        // "code" merely inside a word
        assert_eq!(names, ["Code", "QR Code Reader", "Visual Studio Code", "Xcode"]);
    }

    /// issue #4: MacTap by "mt", MyMacCleaner by "mac", and a run of
    /// capitals kept whole.
    #[test]
    fn camel_case_words_count_for_initials_and_word_starts() {
        assert_eq!(match_apps(&[app("MacTap")], "mt", 10, true)[0].name, "MacTap");
        assert_eq!(match_apps(&[app("OmniDiskSweeper")], "ods", 10, true)[0].name, "OmniDiskSweeper");
        assert_eq!(match_apps(&[app("MyMacCleaner")], "mmc", 10, true)[0].name, "MyMacCleaner");
        // an acronym outranks letters that merely sit inside a word (the
        // "mpt" vs "Command Prompt" case found on a real Start Menu)
        let apps = vec![app("Developer Command Prompt"), app("MagpieProbeTool"), app("x64 Native Tools Command Prompt")];
        assert_eq!(match_apps(&apps, "mpt", 10, true)[0].name, "MagpieProbeTool");
        // the report's case: four apps start with "mac" and the cap used to
        // cut MyMacCleaner; a word-start match now ranks it above substrings
        let apps = vec![app("Mac Sai"), app("MacTap"), app("MacEverything"), app("MyMacCleaner"), app("Emacs")];
        let hits = match_apps(&apps, "mac", 10, true);
        let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names[3], "MyMacCleaner", "word start ranks after the prefixes: {names:?}");
        assert_eq!(names[4], "Emacs", "a plain substring comes last");
        assert!(hits[3].score > hits[4].score);
        // capitals in a run stay one word: VLC is not V-L-C
        assert!(!matches_initials("vl", "VLC"));
        assert!(matches_initials("os", "OBS Studio"));
        // no new word inside a lowercase run
        assert!(!matches_word_start("ap", "Xcode Tapper"), "ap starts no word");
        assert!(matches_word_start("tap", "MacTap"));
        assert!(!matches_word_start("mac", "MacTap"), "the name start is the prefix tier, not this one");
        assert_eq!(word_starts("iTerm"), vec![true, true, false, false, false]);
    }

    #[test]
    fn degenerate_names_and_files_never_panic_or_invent_matches() {
        // empty, symbols only, emoji, digits
        assert!(word_starts("").is_empty());
        assert_eq!(word_starts("--"), vec![false, false]);
        assert_eq!(word_starts("🍎Music"), vec![false, true, false, false, false, false]);
        assert_eq!(word_starts("Office365"), vec![true, false, false, false, false, false, false, false, false]);
        assert!(!matches_initials("m", "MacTap"), "one letter is never an acronym");
        assert!(!matches_word_start("", "MacTap"));
        assert!(match_apps(&[app("")], "a", 10, true).is_empty());
        // a capital straight after a digit or an accent does not start a word
        assert!(!matches_initials("oo", "Office365Online"));
        assert!(matches_initials("éd", "Éclair Draw"));
        // unreadable strings files and bare bundles give no names
        let dir = std::env::temp_dir().join(format!("magpie-degenerate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Bare.app/Contents/Resources/zh_CN.lproj")).unwrap();
        std::fs::write(dir.join("Bare.app/Contents/Resources/zh_CN.lproj/InfoPlist.strings"), [0xFF, 0xFE, 0x00]).unwrap();
        std::fs::write(dir.join("Bare.app/Contents/Resources/InfoPlist.loctable"), b"not a plist").unwrap();
        assert!(bundle_names(&dir.join("Bare.app")).is_empty());
        assert!(bundle_names(&dir.join("Missing.app")).is_empty());
        assert!(read_strings(&dir.join("nope.strings")).is_none());
        // a Chinese interface keeps the installed name when there is no Chinese one
        let mut plain = vec![app("Terminal")];
        localize(&mut plain, "zh");
        assert_eq!(plain[0].name, "Terminal");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_bundles_descends_folders_but_never_into_a_bundle() {
        let root = std::env::temp_dir().join(format!("magpie-bundles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for d in [
            "A.app/Contents",
            "Utilities/Activity Monitor.app/Contents",
            "Utilities/Terminal.app/Contents/Library/Helper.app",
            "Vendor/Suite/Tool.app",
            "d1/d2/d3/TooDeep.app",
            "Not An App/readme",
        ] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        let found: Vec<String> = find_bundles(&root, APP_DIR_DEPTH)
            .iter()
            .map(|p| p.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(
            found,
            ["A.app", "Utilities/Activity Monitor.app", "Utilities/Terminal.app", "Vendor/Suite/Tool.app"],
            "top level, Utilities and a vendor folder; not the helper inside a bundle, not 4 levels down"
        );
        assert!(find_bundles(&root.join("missing"), 3).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A bundle laid out like current macOS system apps: every language in
    /// one InfoPlist.loctable, plus an older .lproj strings file in UTF-16.
    #[test]
    fn bundle_names_read_loctable_and_lproj_strings() {
        let b = std::env::temp_dir().join(format!("magpie-names-{}", std::process::id())).join("Activity Monitor.app");
        let _ = std::fs::remove_dir_all(&b);
        let res = b.join("Contents/Resources");
        std::fs::create_dir_all(res.join("zh_TW.lproj")).unwrap();
        std::fs::create_dir_all(res.join("fr.lproj")).unwrap();
        let dict = |pairs: &[(&str, &str)]| {
            let mut d = plist::Dictionary::new();
            for (k, v) in pairs {
                d.insert(k.to_string(), plist::Value::String(v.to_string()));
            }
            plist::Value::Dictionary(d)
        };
        dict(&[("CFBundleName", "Activity Monitor"), ("CFBundleIdentifier", "com.apple.ActivityMonitor")])
            .to_file_xml(b.join("Contents/Info.plist"))
            .unwrap();
        let mut table = plist::Dictionary::new();
        table.insert("zh_CN".into(), dict(&[("CFBundleDisplayName", "活动监视器"), ("CFBundleName", "活动监视器")]));
        table.insert("en".into(), dict(&[("CFBundleDisplayName", "Activity Monitor")]));
        table.insert("de".into(), dict(&[("CFBundleDisplayName", "Aktivitätsanzeige")]));
        plist::Value::Dictionary(table).to_file_binary(res.join("InfoPlist.loctable")).unwrap();
        let text = "/* Localized */\n\"CFBundleDisplayName\" = \"活動監視器\";\n\"NSHumanReadableCopyright\" = \"x\";\n";
        let mut utf16 = vec![0xFF, 0xFE];
        for u in text.encode_utf16() {
            utf16.extend_from_slice(&u.to_le_bytes());
        }
        std::fs::write(res.join("zh_TW.lproj/InfoPlist.strings"), utf16).unwrap();
        std::fs::write(res.join("fr.lproj/InfoPlist.strings"), "\"CFBundleDisplayName\" = \"Moniteur\";").unwrap();

        let names = bundle_names(&b);
        let has = |tag: &str, n: &str| names.iter().any(|(t, x)| t == tag && x == n);
        assert!(has("", "Activity Monitor"), "{names:?}");
        assert!(has("zh_CN", "活动监视器"), "{names:?}");
        assert!(has("zh_TW", "活動監視器"), "{names:?}");
        assert!(!names.iter().any(|(t, _)| t == "de" || t == "fr"), "only English and Chinese: {names:?}");

        // listed as the app would be, Chinese finds it, so does pinyin
        let mut e = app("Activity Monitor");
        e.aliases.extend(names.iter().map(|(_, n)| n.clone()).filter(|n| n != "Activity Monitor"));
        e.names = names;
        let apps = vec![e.clone(), app("Mac优化大师")];
        assert_eq!(match_apps(&apps, "活动", 10, true)[0].name, "Activity Monitor");
        assert_eq!(match_apps(&apps, "huodong", 10, true)[0].name, "Activity Monitor");
        // a Chinese interface shows the Chinese name; the English one still matches
        let mut shown = vec![e];
        localize(&mut shown, "zh");
        assert_eq!(shown[0].name, "活动监视器");
        assert_eq!(match_apps(&shown, "activity", 10, true)[0].name, "活动监视器");
        let mut english = shown.clone();
        localize(&mut english, "en");
        assert_eq!(english[0].name, "活动监视器", "en leaves names as they are");
        let _ = std::fs::remove_dir_all(b.parent().unwrap());
    }

    #[test]
    fn prefix_beats_substring() {
        let apps = vec![app("Google Chrome"), app("Chrome")];
        let hits = match_apps(&apps, "chrome", 10, true);
        assert_eq!(hits[0].name, "Chrome", "exact/prefix beats mid-string");
    }

    #[test]
    fn acronym_matches_word_initials_only() {
        let apps = vec![app("Visual Studio Code"), app("RecoveryDrive")];
        let hits = match_apps(&apps, "vsc", 10, true);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "Visual Studio Code", "initials v-s-c");
        // 'code' must NOT match RecoveryDrive as a loose subsequence
        // (it legitimately substring-matches "Visual Studio Code", so test in isolation)
        assert!(match_apps(&[app("RecoveryDrive")], "code", 10, true).is_empty());
        assert!(match_apps(&apps, "zzz", 10, true).is_empty());
    }

    #[test]
    fn empty_query_matches_nothing() {
        assert!(match_apps(&[app("Safari")], "  ", 10, true).is_empty());
    }

    #[test]
    fn pinyin_initials_and_full_match_chinese_names() {
        let apps = vec![app("微信"), app("腾讯会议"), app("网易云音乐"), app("Visual Studio Code")];
        for q in ["wx", "weixin", "weix"] {
            let hits = match_apps(&apps, q, 10, true);
            assert_eq!(hits.len(), 1, "query {q}");
            assert_eq!(hits[0].name, "微信", "query {q}");
        }
        assert_eq!(match_apps(&apps, "txhy", 10, true)[0].name, "腾讯会议");
        assert_eq!(match_apps(&apps, "wangyiyun", 10, true)[0].name, "网易云音乐");
        assert_eq!(match_apps(&apps, "wyyyy", 10, true)[0].name, "网易云音乐");
    }

    #[test]
    fn pinyin_handles_heteronyms_and_mixed_names() {
        // 重 reads chong2 (in 重庆) and zhong4 — both spellings must match
        let apps = vec![app("重庆生活"), app("QQ音乐")];
        assert_eq!(match_apps(&apps, "cqsh", 10, true)[0].name, "重庆生活");
        assert_eq!(match_apps(&apps, "zqsh", 10, true)[0].name, "重庆生活");
        assert_eq!(match_apps(&apps, "chongqing", 10, true)[0].name, "重庆生活");
        // ascii chars inside a Han name must match themselves
        assert_eq!(match_apps(&apps, "qqyinyue", 10, true)[0].name, "QQ音乐");
        assert_eq!(match_apps(&apps, "qqyy", 10, true)[0].name, "QQ音乐");
    }

    #[test]
    fn builtin_aliases_bridge_zh_and_en_names() {
        // installed as 飞书 → findable as "lark" (and "feishu" via pinyin)
        let apps = vec![app("飞书"), app("腾讯会议")];
        assert_eq!(match_apps(&apps, "lark", 10, true)[0].name, "飞书");
        assert_eq!(match_apps(&apps, "feishu", 10, true)[0].name, "飞书");
        // installed as Lark → findable as 飞书 / "feishu" (pinyin OF the alias)
        let apps = vec![app("Lark")];
        assert_eq!(match_apps(&apps, "飞书", 10, true)[0].name, "Lark");
        assert_eq!(match_apps(&apps, "feishu", 10, true)[0].name, "Lark");
        // alias match ranks below an exact own-name match
        let apps = vec![app("WeChat"), app("微信")];
        let hits = match_apps(&apps, "wechat", 10, true);
        assert_eq!(hits[0].name, "WeChat");
        assert_eq!(hits[1].name, "微信");
        assert!(hits[0].score > hits[1].score);
    }

    #[test]
    fn user_aliases_attach_by_substring_and_match() {
        let mut apps = vec![app("Clash for Windows"), app("Chrome")];
        apply_user_aliases(&mut apps, &parse_alias_rules("proxy = clash\n# comment\nbrowser=chrome"));
        assert_eq!(match_apps(&apps, "proxy", 10, true)[0].name, "Clash for Windows");
        assert_eq!(match_apps(&apps, "browser", 10, true)[0].name, "Chrome");
        assert!(match_apps(&apps, "proxy", 10, true).len() == 1);
    }

    #[test]
    fn unpremultiply_restores_straight_alpha_and_leaves_straight_data_alone() {
        // half-transparent white, premultiplied: (128,128,128,128)
        let mut px = vec![128, 128, 128, 128, 0, 0, 0, 0, 10, 20, 30, 255];
        unpremultiply(&mut px);
        assert_eq!(&px[..4], &[255, 255, 255, 128]);
        assert_eq!(&px[4..8], &[0, 0, 0, 0], "fully transparent stays as is");
        assert_eq!(&px[8..], &[10, 20, 30, 255], "opaque pixels are unchanged");
        // a colour above its alpha means straight data: nothing may change
        let mut straight = vec![200, 10, 10, 100, 128, 128, 128, 128];
        let before = straight.clone();
        unpremultiply(&mut straight);
        assert_eq!(straight, before);
    }

    #[test]
    fn icon_cache_round_trips_updates_and_prunes() {
        let conn = crate::db::open_in_memory().unwrap();
        assert!(cached_icons(&conn).unwrap().is_empty());
        let png = Icon { mime: "image/png", bytes: vec![1, 2, 3] };
        store_icon(&conn, "/Apps/A.app", 100, Some(&png)).unwrap();
        store_icon(&conn, "/Apps/B.app", 200, None).unwrap();
        let got = cached_icons(&conn).unwrap();
        assert_eq!(got.len(), 2);
        let (stamp, icon) = &got["/Apps/A.app"];
        assert_eq!(*stamp, 100);
        assert_eq!(icon.as_ref().map(|i| (i.mime, i.bytes.clone())), Some(("image/png", vec![1, 2, 3])));
        assert!(got["/Apps/B.app"].1.is_none(), "no icon is remembered as none");
        // a re-read replaces the row in place
        let svg = Icon { mime: "image/svg+xml", bytes: b"<svg/>".to_vec() };
        store_icon(&conn, "/Apps/A.app", 150, Some(&svg)).unwrap();
        let got = cached_icons(&conn).unwrap();
        assert_eq!(got["/Apps/A.app"].0, 150);
        assert_eq!(got["/Apps/A.app"].1.as_ref().unwrap().mime, "image/svg+xml");
        // uninstalled apps are forgotten
        let installed: std::collections::HashSet<String> = ["/Apps/A.app".to_string()].into();
        assert_eq!(prune_icons(&conn, &installed).unwrap(), 1);
        assert_eq!(cached_icons(&conn).unwrap().len(), 1);
    }

    #[test]
    fn damaged_icon_rows_read_as_no_icon_instead_of_failing() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO app_icons VALUES ('/a', 1, 'image/gif', x'00');
             INSERT INTO app_icons VALUES ('/b', 2, 'image/png', NULL);
             INSERT INTO app_icons VALUES ('/c', 3, NULL, x'0102');",
        )
        .unwrap();
        let got = cached_icons(&conn).unwrap();
        assert_eq!(got.len(), 3, "every row still loads");
        assert!(got.values().all(|(_, icon)| icon.is_none()), "an unknown type or a missing half is no icon");
        // a stamp still counts, so these are re-read only when the app changes
        assert_eq!(got["/b"].0, 2);
    }

    #[test]
    fn icon_stamp_follows_the_bundle_plist_and_misses_as_zero() {
        let dir = std::env::temp_dir().join(format!("magpie-stamp-{}", std::process::id()));
        let bundle = dir.join("X.app");
        std::fs::create_dir_all(bundle.join("Contents")).unwrap();
        let plist = bundle.join("Contents").join("Info.plist");
        std::fs::write(&plist, "<plist/>").unwrap();
        let t = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        std::fs::File::options().write(true).open(&plist).unwrap().set_modified(t).unwrap();
        assert_eq!(icon_stamp(bundle.to_str().unwrap()), 1_700_000_000, "a bundle reads its Info.plist");
        let lnk = dir.join("App.lnk");
        std::fs::write(&lnk, "x").unwrap();
        assert!(icon_stamp(lnk.to_str().unwrap()) > 1_700_000_000, "a plain file reads itself");
        assert_eq!(icon_stamp(dir.join("gone.lnk").to_str().unwrap()), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn desktop_icon_names_as_found_in_the_wild() {
        let entry = |icon: &str| format!("[Desktop Entry]\nName=App\n{icon}\nExec=app\n");
        assert_eq!(desktop_icon_name(&entry("Icon=firefox")).as_deref(), Some("firefox"));
        assert_eq!(desktop_icon_name(&entry("Icon=code.png")).as_deref(), Some("code"), "extension dropped");
        assert_eq!(desktop_icon_name(&entry("Icon=gimp.svg")).as_deref(), Some("gimp"));
        assert_eq!(
            desktop_icon_name(&entry("Icon=/opt/app/icon.png")).as_deref(),
            Some("/opt/app/icon.png"),
            "absolute paths are kept whole"
        );
        assert_eq!(desktop_icon_name(&entry("Icon=  ")), None, "blank");
        assert_eq!(desktop_icon_name(&entry("Comment=no icon")), None);
        // localized keys never match; the main entry's key comes first
        let text = "[Desktop Entry]\nIcon[de]=de-icon\nIcon=main\n[Desktop Action new]\nIcon=action\n";
        assert_eq!(desktop_icon_name(text).as_deref(), Some("main"));
        // a dotted name that is not an image extension stays as is
        assert_eq!(desktop_icon_name(&entry("Icon=org.gnome.Nautilus")).as_deref(), Some("org.gnome.Nautilus"));
    }

    /// Every Windows install has Start Menu shortcuts; the first few must
    /// come back as real, visible 64px icons.
    #[cfg(target_os = "windows")]
    #[test]
    fn windows_start_menu_apps_have_icons() {
        let apps = list_apps();
        assert!(!apps.is_empty(), "no Start Menu shortcuts found");
        let mut ok = 0;
        for a in apps.iter().take(10) {
            let Some(icon) = icon(&a.target, 64) else { continue };
            assert_eq!(icon.mime, "image/png");
            let img = image::load_from_memory(&icon.bytes).expect("decodes").to_rgba8();
            assert_eq!(img.dimensions(), (64, 64), "{}", a.name);
            let visible = img.pixels().filter(|p| p[3] > 0).count();
            assert!(visible > 64 * 64 / 10, "{} icon is nearly empty", a.name);
            ok += 1;
        }
        assert!(ok >= 5, "only {ok} of the first 10 shortcuts produced an icon");
        assert!(icon(r"C:\definitely\not\here.lnk", 64).is_none());
    }

    #[test]
    fn pinyin_respects_toggle_and_guards() {
        // bare entry: no builtin aliases, so only the pinyin path is in play
        let apps = vec![AppEntry {
            name: "微信".into(),
            target: "/x/wx".into(),
            ..Default::default()
        }];
        assert!(match_apps(&apps, "wx", 10, false).is_empty(), "toggle off");
        assert!(match_apps(&apps, "w", 10, true).is_empty(), "1-letter query too broad");
        // Han query matches the name directly, with or without pinyin
        assert_eq!(match_apps(&apps, "微信", 10, false)[0].name, "微信");
        // pure-latin names (outside the alias table) never gain pinyin matches
        assert!(match_apps(&[app("Telegram")], "dianbao", 10, true).is_empty());
    }
}
