//! Where a downloaded file came from, as the browser recorded it when it
//! saved the file: Windows keeps a `Zone.Identifier` stream beside the file
//! (`ReferrerUrl` = the page, `HostUrl` = the file itself), macOS the
//! `kMDItemWhereFroms` attribute, and Chromium on Linux `user.xdg.*` ones.
//!
//! Only the page is ever handed on as a link. The file address is often a
//! CDN URL carrying a signed token: opening it would download the file
//! again, and the token is nobody's business, so it only lends its host
//! when there is no page. Read live when a file is previewed; never stored,
//! never logged.

use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Origin {
    /// what to show: the page's host, else the file's (`www.` dropped)
    pub host: String,
    /// the page the download started from, when it was recorded
    pub page: Option<String>,
}

/// The origin of `path`, if a browser recorded one.
pub fn of(path: &Path) -> Option<Origin> {
    let (page, file) = read(path);
    pick(page, file)
}

fn pick(page: Option<String>, file: Option<String>) -> Option<Origin> {
    let page = page.and_then(|u| web(&u).map(|h| (u, h)));
    let file_host = file.as_deref().and_then(web);
    match (page, file_host) {
        (Some((url, host)), _) => Some(Origin { host, page: Some(url) }),
        (None, Some(host)) => Some(Origin { host, page: None }),
        (None, None) => None,
    }
}

/// The host of an http(s) URL, `www.` dropped; None for anything else
/// (`about:internet`, `file:`, blobs, garbage).
fn web(u: &str) -> Option<String> {
    let url = url::Url::parse(u.trim()).ok()?;
    if url.scheme() != "https" && url.scheme() != "http" {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    Some(host.strip_prefix("www.").map(str::to_string).unwrap_or(host))
}

/// (page, file) from a `Zone.Identifier` stream.
#[cfg(any(windows, test))]
fn parse_zone_identifier(text: &str) -> (Option<String>, Option<String>) {
    let mut page = None;
    let mut file = None;
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "ReferrerUrl" => page = Some(v.trim().to_string()),
                "HostUrl" => file = Some(v.trim().to_string()),
                _ => {}
            }
        }
    }
    (page, file)
}

#[cfg(windows)]
fn read(path: &Path) -> (Option<String>, Option<String>) {
    let mut stream = path.as_os_str().to_os_string();
    stream.push(":Zone.Identifier");
    match std::fs::read(&stream) {
        Ok(bytes) => parse_zone_identifier(&String::from_utf8_lossy(&bytes)),
        Err(_) => (None, None),
    }
}

/// macOS: a binary plist array, the file's URL first and the page second.
#[cfg(target_os = "macos")]
fn read(path: &Path) -> (Option<String>, Option<String>) {
    let Ok(Some(bytes)) = xattr::get(path, "com.apple.metadata:kMDItemWhereFroms") else {
        return (None, None);
    };
    let urls: Vec<String> = plist::from_bytes(&bytes).unwrap_or_default();
    (urls.get(1).cloned(), urls.first().cloned())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn read(path: &Path) -> (Option<String>, Option<String>) {
    let get = |name: &str| {
        xattr::get(path, name)
            .ok()
            .flatten()
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    };
    (get("user.xdg.referrer.url"), get("user.xdg.origin.url"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_identifier_fields() {
        let text = "[ZoneTransfer]\r\nZoneId=3\r\nReferrerUrl=https://github.com/a/b/releases\r\nHostUrl=https://release-assets.githubusercontent.com/x?sig=SECRET\r\n";
        assert_eq!(
            parse_zone_identifier(text),
            (
                Some("https://github.com/a/b/releases".into()),
                Some("https://release-assets.githubusercontent.com/x?sig=SECRET".into())
            )
        );
        assert_eq!(parse_zone_identifier("[ZoneTransfer]\nZoneId=3\n"), (None, None));
        assert_eq!(parse_zone_identifier(""), (None, None));
    }

    #[test]
    fn the_page_is_the_link_the_file_only_lends_its_host() {
        let o = pick(Some("https://www.github.com/a/b".into()), Some("https://cdn.example/x?token=t".into())).unwrap();
        assert_eq!(o, Origin { host: "github.com".into(), page: Some("https://www.github.com/a/b".into()) });
        // no page: the file's host, and no link (a signed CDN URL)
        let o = pick(None, Some("https://File.WX.qq.com/cgi?x=1".into())).unwrap();
        assert_eq!(o, Origin { host: "file.wx.qq.com".into(), page: None });
        // a page that is not the web falls back to the file
        let o = pick(Some("about:internet".into()), Some("http://example.org/f".into())).unwrap();
        assert_eq!(o, Origin { host: "example.org".into(), page: None });
        assert_eq!(pick(None, None), None);
        assert_eq!(pick(Some("file:///C:/x".into()), Some("not a url".into())), None);
        // a host is not enough: only the web is shown or opened
        assert_eq!(pick(Some("ftp://files.example.org/x".into()), Some("chrome-extension://abc/x".into())), None);
    }

    #[test]
    fn nothing_recorded_is_none() {
        let f = std::env::temp_dir().join(format!("magpie-origin-none-{}", std::process::id()));
        std::fs::write(&f, "x").unwrap();
        assert_eq!(of(&f), None);
        assert_eq!(of(&f.with_extension("missing")), None);
        std::fs::remove_file(&f).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn reads_the_stream_a_browser_writes() {
        let f = std::env::temp_dir().join(format!("magpie-origin-{}.zip", std::process::id()));
        std::fs::write(&f, "x").unwrap();
        let mut stream = f.as_os_str().to_os_string();
        stream.push(":Zone.Identifier");
        std::fs::write(&stream, "[ZoneTransfer]\r\nZoneId=3\r\nReferrerUrl=https://filehelper.weixin.qq.com/\r\nHostUrl=https://file.wx.qq.com/cgi-bin/x?token=1\r\n").unwrap();
        assert_eq!(of(&f), Some(Origin { host: "filehelper.weixin.qq.com".into(), page: Some("https://filehelper.weixin.qq.com/".into()) }));
        std::fs::remove_file(&f).unwrap();
    }
}
