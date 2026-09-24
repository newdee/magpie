//! Installed-application launcher: enumerates apps from OS-standard locations
//! and matches them by name. No indexing/DB — the list is small and rebuilt
//! on demand, then filtered per keystroke.

use anyhow::{anyhow, Result};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct AppEntry {
    pub name: String,
    /// Path launched when chosen (a .lnk, .app bundle, or executable/desktop).
    pub target: String,
    /// Alternate names this app also answers to: the built-in zh↔en table,
    /// user-defined aliases, and (Linux) .desktop Keywords/GenericName.
    /// Matched like second names (pinyin included), scored slightly below.
    pub aliases: Vec<String>,
    pub score: f32,
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
    let mut push = |name: String, target: PathBuf, extra: Vec<String>| {
        let key = name.to_lowercase();
        if !name.is_empty() && seen.insert(key) {
            let mut aliases = builtin_aliases(&name);
            for k in extra {
                if !k.is_empty() && !aliases.iter().any(|x| x.eq_ignore_ascii_case(&k)) {
                    aliases.push(k);
                }
            }
            out.push(AppEntry {
                name,
                target: target.to_string_lossy().into_owned(),
                aliases,
                score: 0.0,
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
                    push(stem.to_string(), entry.clone(), Vec::new());
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
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for e in entries.flatten() {
                    let p = e.path();
                    if p.extension().and_then(|x| x.to_str()) == Some("app") {
                        if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                            push(stem.to_string(), p.clone(), Vec::new());
                        }
                    }
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
                    push(d.name, entry.clone(), d.keywords);
                }
            }
        }
    }
    out
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
        } else if name.contains(&q) {
            Some(0.6)
        } else if matches_initials(&q, &name) {
            Some(0.5) // "vsc" -> "Visual Studio Code"
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

/// Acronym match: does `q` spell out the initials of the words in `name`?
/// "vsc" matches "Visual Studio Code"; "code" does NOT match "RecoveryDrive".
/// Word initials are letters starting a word or following a space/-/_/. .
fn matches_initials(q: &str, name: &str) -> bool {
    let initials: String = {
        let mut prev_boundary = true;
        let mut acc = String::new();
        for c in name.chars() {
            if prev_boundary && c.is_alphanumeric() {
                acc.push(c);
            }
            prev_boundary = matches!(c, ' ' | '-' | '_' | '.' | '/');
        }
        acc
    };
    initials.starts_with(q) && q.len() >= 2
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
/// Threading: on macOS this calls AppKit, so the caller runs it on the main
/// thread. On Windows it initializes COM for the calling thread and releases
/// it again, so any worker thread will do.
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
    /// reading the bundle's .icns by hand would miss. Main thread only.
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
}

#[cfg(all(unix, not(target_os = "macos")))]
fn parse_desktop(path: &std::path::Path) -> Option<DesktopEntry> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut name = None;
    let mut exec = None;
    let mut keywords: Vec<String> = Vec::new();
    let mut no_display = false;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("Name=") {
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
    Some(DesktopEntry { name: name?, exec: exec?, keywords })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str) -> AppEntry {
        AppEntry {
            name: name.into(),
            target: format!("/x/{name}"),
            aliases: builtin_aliases(name),
            score: 0.0,
        }
    }

    #[test]
    fn ranks_exact_then_shortest_substring() {
        let apps = vec![app("Visual Studio Code"), app("Code"), app("QR Code Reader"), app("Xcode")];
        let hits = match_apps(&apps, "code", 10, true);
        assert_eq!(hits[0].name, "Code", "exact match wins");
        // remaining are substring matches, shortest name first
        assert_eq!(hits[1].name, "Xcode");
        assert_eq!(hits.len(), 4);
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
            aliases: Vec::new(),
            score: 0.0,
        }];
        assert!(match_apps(&apps, "wx", 10, false).is_empty(), "toggle off");
        assert!(match_apps(&apps, "w", 10, true).is_empty(), "1-letter query too broad");
        // Han query matches the name directly, with or without pinyin
        assert_eq!(match_apps(&apps, "微信", 10, false)[0].name, "微信");
        // pure-latin names (outside the alias table) never gain pinyin matches
        assert!(match_apps(&[app("Telegram")], "dianbao", 10, true).is_empty());
    }
}
