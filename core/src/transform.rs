//! Query-box text transforms: generators (uuid / timestamps / passwords),
//! encoders (base64, URL percent-encoding), and color conversion. Siblings
//! of the calculator — same top-row UI, Enter copies the value.

use base64::Engine;

/// The display payload for a transform hit. `swatch` carries a `#rrggbb`
/// for color queries so the UI can paint a preview chip.
#[derive(Debug, PartialEq, Default)]
pub struct TransformResult {
    pub label: String,
    pub value: String,
    pub swatch: Option<String>,
    /// The value explains why the input did not work (JSON that does not
    /// parse, a JWT that is not one): shown, but Enter copies nothing.
    pub error: bool,
    /// A PNG (base64) to show instead of text, e.g. a QR code; Enter copies
    /// the image.
    pub image: Option<String>,
    /// `timer 25m`: seconds to count down; Enter starts the timer instead of
    /// copying, and `value` is what the reminder will say.
    pub timer: Option<u64>,
}

pub fn transform(query: &str) -> Option<TransformResult> {
    let q = query.trim();
    let lower = q.to_lowercase();
    let (cmd, rest) = match lower.find(char::is_whitespace) {
        Some(i) => (&lower[..i], q[i..].trim()),
        None => (lower.as_str(), ""),
    };
    match cmd {
        "uuid" if rest.is_empty() => Some(TransformResult {
            label: "UUID v4".into(),
            value: uuid_v4()?,
            swatch: None,
            ..Default::default()
        }),
        "now" | "time" if rest.is_empty() => {
            let now = chrono::Local::now();
            Some(TransformResult {
                label: "unix".into(),
                value: format!("{}  ·  {}", now.timestamp(), now.format("%Y-%m-%d %H:%M:%S %z")),
                swatch: None,
                ..Default::default()
            })
        }
        "ts" | "timestamp" => {
            if rest.is_empty() {
                Some(TransformResult {
                    label: "unix timestamp".into(),
                    value: chrono::Local::now().timestamp().to_string(),
                    swatch: None,
                    ..Default::default()
                })
            } else {
                // reverse: a numeric timestamp becomes readable local time
                let n: i64 = rest.parse().ok()?;
                let secs = if n > 100_000_000_000 { n / 1000 } else { n }; // ms input
                let dt = chrono::DateTime::from_timestamp(secs, 0)?;
                Some(TransformResult {
                    label: "local time".into(),
                    value: dt.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S %z").to_string(),
                    swatch: None,
                    ..Default::default()
                })
            }
        }
        "pwd" | "password" => {
            // a non-numeric argument means this is a search, not a request
            let len: usize = if rest.is_empty() { 20 } else { rest.parse().ok()? };
            if !(4..=128).contains(&len) {
                return None;
            }
            Some(TransformResult {
                label: format!("random password ({len})"),
                value: random_password(len)?,
                swatch: None,
                ..Default::default()
            })
        }
        "b64" if !rest.is_empty() => Some(TransformResult {
            label: "base64".into(),
            value: base64::engine::general_purpose::STANDARD.encode(rest.as_bytes()),
            swatch: None,
            ..Default::default()
        }),
        "unb64" if !rest.is_empty() => {
            let bytes = base64::engine::general_purpose::STANDARD.decode(rest.trim()).ok()?;
            let text = String::from_utf8(bytes).ok()?;
            Some(TransformResult { label: "decoded".into(), value: text, ..Default::default() })
        }
        "url" if !rest.is_empty() => Some(TransformResult {
            label: "url-encoded".into(),
            value: percent_encode(rest),
            swatch: None,
            ..Default::default()
        }),
        "unurl" if !rest.is_empty() => Some(TransformResult {
            label: "url-decoded".into(),
            value: percent_decode(rest)?,
            swatch: None,
            ..Default::default()
        }),
        "ip" | "本机ip" if rest.is_empty() => ip_verb(),
        "大写" | "dx" => {
            let (src, from_clip) = source(rest)?;
            let value = crate::handy::rmb_upper(src.trim())?;
            Some(TransformResult { label: labeled("Chinese capitals".into(), from_clip), value, ..Default::default() })
        }
        "py" | "拼音" => {
            let (src, from_clip) = source(rest)?;
            // only when there is Chinese in it: `py` alone on English text is a search
            src.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)).then(|| TransformResult {
                label: labeled("pinyin".into(), from_clip),
                value: crate::handy::pinyin_of(src.trim()),
                ..Default::default()
            })
        }
        "半角" | "全角" => {
            let (src, from_clip) = source(rest)?;
            let (value, label) = if cmd == "半角" {
                (crate::handy::to_halfwidth(&src), "half-width")
            } else {
                (crate::handy::to_fullwidth(&src), "full-width")
            };
            Some(TransformResult { label: labeled(label.into(), from_clip), value, ..Default::default() })
        }
        "unicode" => {
            let (src, from_clip) = source(rest)?;
            let (value, decoded) = crate::handy::unicode_toggle(&src);
            let label = if decoded { "unicode decoded" } else { "unicode escaped" };
            Some(TransformResult { label: labeled(label.into(), from_clip), value, ..Default::default() })
        }
        "html" => {
            let (src, from_clip) = source(rest)?;
            let (value, decoded) = crate::handy::html_toggle(&src);
            let label = if decoded { "html decoded" } else { "html escaped" };
            Some(TransformResult { label: labeled(label.into(), from_clip), value, ..Default::default() })
        }
        "random" | "随机" => random_verb(rest),
        "pick" | "抽签" => {
            let items = crate::handy::pick_items(rest);
            (items.len() >= 2).then(|| {
                let i = crate::handy::random_between(0, items.len() as i64 - 1).unwrap_or(0) as usize;
                TransformResult { label: format!("picked from {}", items.len()), value: items[i].clone(), ..Default::default() }
            })
        }
        "coin" | "抛硬币" if rest.is_empty() => {
            let heads = crate::handy::random_between(0, 1)? == 0;
            let value = match (cmd == "抛硬币", heads) {
                (true, true) => "正面",
                (true, false) => "反面",
                (false, true) => "heads",
                (false, false) => "tails",
            };
            Some(TransformResult { label: "coin".into(), value: value.into(), ..Default::default() })
        }
        "dice" | "骰子" if rest.is_empty() => Some(TransformResult {
            label: "dice".into(),
            value: crate::handy::random_between(1, 6)?.to_string(),
            ..Default::default()
        }),
        "sys" | "系统" if rest.is_empty() => Some(TransformResult {
            label: "system".into(),
            value: crate::handy::system_summary(),
            ..Default::default()
        }),
        "timer" | "倒计时" if !rest.is_empty() => timer_verb(rest),
        "json" => json_verb(rest),
        "md5" | "sha1" | "sha256" => hash_verb(cmd, rest),
        "jwt" => jwt_verb(rest),
        "qr" => qr_verb(rest),
        "upper" | "lower" | "trim" | "slug" | "lines" | "count" | "camel" | "pascal" | "snake"
        | "kebab" | "title" => text_verb(cmd, rest),
        _ => color(q),
    }
}

/// `random` (1–100), `random 6`, `random 10 20`: a whole number, inclusive.
fn random_verb(rest: &str) -> Option<TransformResult> {
    let nums: Vec<i64> = rest.split_whitespace().map(|t| t.parse().ok()).collect::<Option<_>>()?;
    let (lo, hi) = match nums.as_slice() {
        [] => (1, 100),
        [n] if *n >= 1 => (1, *n),
        [a, b] => (*a, *b),
        _ => return None,
    };
    let v = crate::handy::random_between(lo, hi)?;
    Some(TransformResult { label: format!("random {}–{}", lo.min(hi), lo.max(hi)), value: v.to_string(), ..Default::default() })
}

/// `timer 25m 开会`: a countdown; the first word is the length, the rest
/// what the reminder says. Enter starts it (the app keeps the clock).
fn timer_verb(rest: &str) -> Option<TransformResult> {
    let (len, what) = match rest.split_once(char::is_whitespace) {
        Some((l, w)) => (l, w.trim()),
        None => (rest, ""),
    };
    let secs = crate::handy::parse_duration(len)?;
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    let clock = if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") };
    Some(TransformResult {
        label: format!("timer {clock}"),
        value: if what.is_empty() { "Time's up".into() } else { what.to_string() },
        timer: Some(secs),
        ..Default::default()
    })
}

/// `ip`: this machine's IPv4 addresses on the local network. Enter copies
/// the first; the label names its adapter and lists the others. Read from
/// the adapters themselves, so nothing is sent anywhere and it works offline.
fn ip_verb() -> Option<TransformResult> {
    let addrs = local_ipv4s();
    let ((name, ip), others) = addrs.split_first()?;
    let mut label = format!("local IP · {name}");
    if !others.is_empty() {
        let rest: Vec<String> = others.iter().map(|(n, a)| format!("{a} ({n})")).collect();
        label.push_str(&format!(" · also {}", rest.join(", ")));
    }
    Some(TransformResult { label, value: ip.to_string(), ..Default::default() })
}

/// IPv4 addresses of adapters that are up, without loopback and link-local
/// ones: private LAN addresses first, virtual adapters (WSL, Docker, VMs)
/// last, so the first is the one other devices on your network can reach.
fn local_ipv4s() -> Vec<(String, std::net::Ipv4Addr)> {
    const VIRTUAL: &[&str] = &["vethernet", "wsl", "docker", "vmware", "virtualbox", "vbox", "hyper-v", "veth", "bridge", "utun"];
    let mut v: Vec<(String, std::net::Ipv4Addr)> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| i.is_oper_up() && !i.is_loopback())
        .filter_map(|i| match i.addr {
            if_addrs::IfAddr::V4(a) if !a.ip.is_link_local() => Some((i.name, a.ip)),
            _ => None,
        })
        .collect();
    v.sort_by_key(|(name, ip)| {
        let n = name.to_lowercase();
        (VIRTUAL.iter().any(|x| n.contains(x)), !ip.is_private(), ip.octets())
    });
    v.dedup_by(|a, b| a.1 == b.1);
    v
}

/// Text verbs work on the argument when there is one, otherwise on whatever
/// the clipboard holds: `json` alone pretty-prints what you just copied, and
/// Enter puts the result back.
fn text_verb(cmd: &str, rest: &str) -> Option<TransformResult> {
    let (src, from_clip) = if rest.is_empty() {
        (crate::clips::clipboard_text().ok()?, true)
    } else {
        (rest.to_string(), false)
    };
    if src.trim().is_empty() {
        return None;
    }
    let (label, value): (String, String) = match cmd {
        "camel" | "pascal" | "snake" | "kebab" | "title" => {
            let words = split_words(&src);
            if words.is_empty() {
                return None;
            }
            let cap = |w: &str| {
                let mut c = w.chars();
                c.next().map(|f| f.to_uppercase().chain(c.flat_map(char::to_lowercase)).collect::<String>()).unwrap_or_default()
            };
            let lower: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
            match cmd {
                "camel" => ("camelCase".into(), lower[0].clone() + &words[1..].iter().map(|w| cap(w)).collect::<String>()),
                "pascal" => ("PascalCase".into(), words.iter().map(|w| cap(w)).collect()),
                "snake" => ("snake_case".into(), lower.join("_")),
                "kebab" => ("kebab-case".into(), lower.join("-")),
                _ => ("Title Case".into(), words.iter().map(|w| cap(w)).collect::<Vec<_>>().join(" ")),
            }
        }
        "upper" => ("UPPER CASE".into(), src.to_uppercase()),
        "lower" => ("lower case".into(), src.to_lowercase()),
        "trim" => (
            "trimmed".into(),
            src.lines().map(str::trim_end).collect::<Vec<_>>().join("\n").trim().to_string(),
        ),
        "slug" => ("slug".into(), slugify(&src)),
        "lines" => {
            let mut v: Vec<&str> = src.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
            let total = v.len();
            v.sort_unstable();
            v.dedup();
            (format!("{} unique of {total} lines, sorted", v.len()), v.join("\n"))
        }
        "count" => {
            let chars = src.chars().count();
            let no_space = src.chars().filter(|c| !c.is_whitespace()).count();
            let words = src.split_whitespace().count();
            let lines = src.lines().count();
            (
                format!("count ({no_space} without spaces)"),
                format!("{chars} chars  ·  {words} words  ·  {lines} lines"),
            )
        }
        _ => return None,
    };
    Some(TransformResult {
        label: if from_clip { format!("clipboard → {label}") } else { label },
        value,
        swatch: None,
        ..Default::default()
    })
}

/// The text to work on: what follows the verb, or else the clipboard.
/// Returns it with whether it came from the clipboard; None when there is
/// nothing (an empty clipboard, or one holding an image).
fn source(rest: &str) -> Option<(String, bool)> {
    let (src, from_clip) = if rest.is_empty() {
        (crate::clips::clipboard_text().ok()?, true)
    } else {
        (rest.to_string(), false)
    };
    (!src.trim().is_empty()).then_some((src, from_clip))
}

fn labeled(label: String, from_clip: bool) -> String {
    if from_clip {
        format!("clipboard → {label}")
    } else {
        label
    }
}

/// Words of a name or phrase in any case style: split at anything that is
/// not a letter or digit, at a lowercase→uppercase step (myMac), and before
/// the last capital of a run followed by lowercase (HTTPServer → HTTP,
/// Server). `snake MyMacCleaner` → my_mac_cleaner.
fn split_words(s: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    for chunk in s.split(|c: char| !c.is_alphanumeric()).filter(|c| !c.is_empty()) {
        let chars: Vec<char> = chunk.chars().collect();
        let mut cur = String::new();
        for (i, &c) in chars.iter().enumerate() {
            let prev = i.checked_sub(1).map(|j| chars[j]);
            let next = chars.get(i + 1).copied();
            let boundary = match prev {
                None => false,
                Some(p) => {
                    (p.is_lowercase() && c.is_uppercase())
                        || (p.is_uppercase() && c.is_uppercase() && next.is_some_and(|n| n.is_lowercase()))
                        || (p.is_alphabetic() != c.is_alphabetic() && (p.is_numeric() || c.is_numeric()) && p.is_ascii() && c.is_ascii())
                }
            };
            if boundary && !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            cur.push(c);
        }
        if !cur.is_empty() {
            words.push(cur);
        }
    }
    words
}

/// `json`: pretty-print (keys in their original order), `json min`: one
/// line, `json sort`: keys sorted. JSON escaped into a string, as logs
/// print it (`"{\"a\":1}"` or `{\"a\":1}`), is unwrapped first. Input that
/// does not parse gets its error and position instead of silence.
fn json_verb(rest: &str) -> Option<TransformResult> {
    let (mode, arg) = match rest.split_once(char::is_whitespace) {
        Some((w, tail)) if matches!(w, "min" | "minify" | "sort") => (w, tail.trim()),
        _ if matches!(rest, "min" | "minify" | "sort") => (rest, ""),
        _ => ("pretty", rest),
    };
    let (src, from_clip) = source(arg)?;
    let text = match unwrap_escaped_json(src.trim()) {
        Ok(t) => t,
        Err(e) => {
            let msg = e.to_string();
            let what = msg.split(" at line ").next().unwrap_or(&msg);
            return Some(TransformResult {
                label: labeled("not valid JSON".into(), from_clip),
                value: format!("line {} column {}: {what}", e.line(), e.column()),
                error: true,
                ..Default::default()
            });
        }
    };
    let (label, value) = match mode {
        "sort" => {
            // serde_json's map is ordered by key, so a round trip sorts
            let v: serde_json::Value = serde_json::from_str(&text).ok()?;
            ("JSON, keys sorted".to_string(), serde_json::to_string_pretty(&v).ok()?)
        }
        "min" | "minify" => ("JSON, minified".to_string(), transcode_json(&text, false)?),
        _ => ("JSON, pretty-printed".to_string(), transcode_json(&text, true)?),
    };
    Some(TransformResult { label: labeled(label, from_clip), value, ..Default::default() })
}

/// Re-serialize JSON straight from the parser, so keys keep their order
/// (serde_json's own map would sort them).
fn transcode_json(text: &str, pretty: bool) -> Option<String> {
    let mut de = serde_json::Deserializer::from_str(text);
    let mut out = Vec::new();
    if pretty {
        let mut ser = serde_json::Serializer::pretty(&mut out);
        serde_transcode::transcode(&mut de, &mut ser).ok()?;
    } else {
        let mut ser = serde_json::Serializer::new(&mut out);
        serde_transcode::transcode(&mut de, &mut ser).ok()?;
    }
    de.end().ok()?;
    String::from_utf8(out).ok()
}

/// The JSON text to format: as given, or unwrapped once when it is a JSON
/// string holding an object or array. The error is the original input's.
fn unwrap_escaped_json(t: &str) -> Result<String, serde_json::Error> {
    let inner_json = |s: &str| {
        let s = s.trim();
        (s.starts_with('{') || s.starts_with('['))
            && serde_json::from_str::<serde::de::IgnoredAny>(s).is_ok()
    };
    match serde_json::from_str::<serde_json::Value>(t) {
        Ok(serde_json::Value::String(s)) if inner_json(&s) => Ok(s),
        Ok(_) => Ok(t.to_string()),
        Err(e) => {
            // escaped but without its outer quotes: {\"a\":1}
            if t.contains("\\\"") {
                if let Ok(s) = serde_json::from_str::<String>(&format!("\"{t}\"")) {
                    if inner_json(&s) {
                        return Ok(s);
                    }
                }
            }
            Err(e)
        }
    }
}

/// `md5` / `sha1` / `sha256` of the text after the verb, or the clipboard,
/// as lowercase hex over its UTF-8 bytes.
fn hash_verb(cmd: &str, rest: &str) -> Option<TransformResult> {
    use sha2::Digest;
    let (src, from_clip) = source(rest)?;
    let bytes = src.as_bytes();
    let hex = |d: &[u8]| d.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let (name, value) = match cmd {
        "md5" => ("MD5", hex(&md5::Md5::digest(bytes))),
        "sha1" => ("SHA-1", hex(&sha1::Sha1::digest(bytes))),
        _ => ("SHA-256", hex(&sha2::Sha256::digest(bytes))),
    };
    Some(TransformResult {
        label: labeled(format!("{name} of {} bytes", bytes.len()), from_clip),
        value,
        ..Default::default()
    })
}

/// `jwt`: a token's header and payload, decoded locally (nothing leaves the
/// machine; the signature is not checked, there is no key to check it
/// with). Accepts a bare token or `Bearer …`. The label tells when it
/// expires; the value is the payload, pretty.
fn jwt_verb(rest: &str) -> Option<TransformResult> {
    use base64::Engine;
    let (src, from_clip) = source(rest)?;
    let token = src.trim();
    let token = token.strip_prefix("Bearer ").or_else(|| token.strip_prefix("bearer ")).unwrap_or(token).trim();
    let bad = |why: &str| {
        Some(TransformResult {
            label: labeled("not a JWT".into(), from_clip),
            value: why.to_string(),
            error: true,
            ..Default::default()
        })
    };
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return bad("a JWT has three parts separated by dots");
    }
    // the JSON text as sent (to print in its own key order) and parsed
    let decode = |p: &str| -> Option<(String, serde_json::Value)> {
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(p.trim_end_matches('=')).ok()?;
        let text = String::from_utf8(bytes).ok()?;
        let v = serde_json::from_str(&text).ok()?;
        Some((text, v))
    };
    let (Some((_, header)), Some((payload_text, payload))) = (decode(parts[0]), decode(parts[1])) else {
        return bad("the header or payload is not base64url JSON");
    };
    let alg = header.get("alg").and_then(|a| a.as_str()).unwrap_or("?").to_string();
    let now = chrono::Utc::now().timestamp();
    let when = |secs: i64| {
        chrono::DateTime::from_timestamp(secs, 0)
            .map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| secs.to_string())
    };
    let exp = match payload.get("exp").and_then(|e| e.as_i64()) {
        Some(e) if e < now => format!("expired {}", when(e)),
        Some(e) => format!("expires {}", when(e)),
        None => "no expiry".into(),
    };
    let value = transcode_json(&payload_text, true)?;
    Some(TransformResult {
        label: labeled(format!("JWT {alg} · {exp}"), from_clip),
        value,
        ..Default::default()
    })
}

/// `qr`: a QR code of the text after the verb, or the clipboard, as a PNG.
/// Enter copies the image, ready to paste into a chat or to scan from the
/// screen with a phone.
fn qr_verb(rest: &str) -> Option<TransformResult> {
    use base64::Engine;
    // `qr` alone with an image on the clipboard: read the code in it
    if rest.is_empty() {
        if let Ok(img) = crate::clips::clipboard_image() {
            if let Some(text) = crate::handy::decode_qr(&img) {
                return Some(TransformResult { label: "QR code in the copied image".into(), value: text, ..Default::default() });
            }
            if crate::clips::clipboard_text().map(|t| t.trim().is_empty()).unwrap_or(true) {
                return Some(TransformResult {
                    label: "QR code".into(),
                    value: "No QR code found in the copied image".into(),
                    error: true,
                    ..Default::default()
                });
            }
        }
    }
    let (src, from_clip) = source(rest)?;
    let text = src.trim();
    let code = match qrcode::QrCode::new(text.as_bytes()) {
        Ok(c) => c,
        Err(_) => {
            return Some(TransformResult {
                label: labeled("QR code".into(), from_clip),
                value: format!("too long for a QR code ({} bytes; about 2,900 fit)", text.len()),
                error: true,
                ..Default::default()
            })
        }
    };
    let img = code.render::<image::Luma<u8>>().min_dimensions(240, 240).quiet_zone(true).build();
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageLuma8(img).write_to(&mut png, image::ImageFormat::Png).ok()?;
    let preview: String = text.chars().take(60).collect();
    Some(TransformResult {
        label: labeled(format!("QR code · {} chars", text.chars().count()), from_clip),
        value: if text.chars().count() > 60 { format!("{preview}…") } else { preview },
        image: Some(base64::engine::general_purpose::STANDARD.encode(png.into_inner())),
        ..Default::default()
    })
}

/// `Hello, World! 2026` → `hello-world-2026`. Alphanumerics of any script
/// survive; everything else becomes one dash.
fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut dash = true; // suppress a leading dash
    for c in s.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            out.push(c);
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod verb_tests {
    use super::transform;

    fn value(q: &str) -> String {
        transform(q).unwrap_or_else(|| panic!("{q:?} must transform")).value
    }

    #[test]
    fn json_pretty_prints_and_explains_non_json() {
        assert_eq!(value("json {\"a\":1,\"b\":[1,2]}"), "{\n  \"a\": 1,\n  \"b\": [\n    1,\n    2\n  ]\n}");
        // not JSON: an error with its position, never a silent nothing
        let e = transform("json not json at all").unwrap();
        assert!(e.error && e.value.starts_with("line 1 column "), "{e:?}");
    }

    #[test]
    fn case_and_trim() {
        assert_eq!(value("upper abc Déf"), "ABC DÉF");
        assert_eq!(value("lower ABC"), "abc");
        assert_eq!(value("trim   x  \n y  "), "x\n y");
    }

    #[test]
    fn slug_keeps_letters_of_any_script() {
        assert_eq!(value("slug Hello, World! 2026"), "hello-world-2026");
        assert_eq!(value("slug  --a__b--  "), "a-b");
        assert_eq!(value("slug 你好 世界"), "你好-世界");
    }

    #[test]
    fn lines_dedupes_and_sorts_with_counts_in_the_label() {
        let r = transform("lines b\na\nb\n\n a ").unwrap();
        assert_eq!(r.value, "a\nb");
        assert_eq!(r.label, "2 unique of 4 lines, sorted");
    }

    #[test]
    fn count_reports_chars_words_lines() {
        let r = transform("count hello world\nagain").unwrap();
        assert_eq!(r.value, "17 chars  ·  3 words  ·  2 lines");
        assert_eq!(r.label, "count (15 without spaces)");
    }

    #[test]
    fn a_hash_followed_by_non_hex_text_is_a_search_not_a_panic() {
        // 6 bytes of CJK after '#': the old byte-index slice cut a character
        assert!(transform("#报销").is_none());
        assert!(transform("#报").is_none());
        assert!(transform("#ffz600").is_none());
        assert!(transform("#ff660").is_none(), "5 hex digits is not a color");
        assert!(transform("#ff6600").is_some());
        assert!(transform("#FFF").is_some());
    }

    #[test]
    fn a_verb_with_an_argument_never_touches_the_clipboard() {
        // explicit arguments are labelled plainly; only the clipboard path
        // carries the "clipboard →" prefix
        assert_eq!(transform("upper x").unwrap().label, "UPPER CASE");
    }
}

fn uuid_v4() -> Option<String> {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).ok()?;
    b[6] = (b[6] & 0x0f) | 0x40; // version 4
    b[8] = (b[8] & 0x3f) | 0x80; // RFC 4122 variant
    Some(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    ))
}

/// Rejection-sampled from the OS CSPRNG — no modulo bias, no weak seeds.
fn random_password(len: usize) -> Option<String> {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!#$%&*+-=?@_";
    let mut out = String::with_capacity(len);
    let mut buf = [0u8; 64];
    while out.len() < len {
        getrandom::fill(&mut buf).ok()?;
        for &byte in buf.iter() {
            // 74 * 3 = 222: accept only the unbiased range
            if byte < 222 {
                out.push(CHARSET[(byte % 74) as usize] as char);
                if out.len() == len {
                    break;
                }
            }
        }
    }
    Some(out)
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = s.get(i + 1..i + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// `#rrggbb` / `#rgb` -> rgb()+hsl(); `rgb(r, g, b)` -> hex+hsl.
fn color(q: &str) -> Option<TransformResult> {
    let (r, g, b) = if let Some(hex) = q.strip_prefix('#') {
        // byte slicing below is only safe once every byte is an ASCII hex
        // digit; "#报销" is 6 bytes and used to be cut inside a character
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        match hex.len() {
            3 => {
                let v: Vec<u8> = hex
                    .chars()
                    .map(|c| u8::from_str_radix(&format!("{c}{c}"), 16))
                    .collect::<Result<_, _>>()
                    .ok()?;
                (v[0], v[1], v[2])
            }
            6 => (
                u8::from_str_radix(&hex[0..2], 16).ok()?,
                u8::from_str_radix(&hex[2..4], 16).ok()?,
                u8::from_str_radix(&hex[4..6], 16).ok()?,
            ),
            _ => return None,
        }
    } else {
        let lower = q.to_lowercase();
        let inner = lower.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')'))?;
        let parts: Vec<u8> = inner
            .split(',')
            .map(|p| p.trim().parse::<u8>())
            .collect::<Result<Vec<u8>, _>>()
            .ok()?;
        if parts.len() != 3 {
            return None;
        }
        (parts[0], parts[1], parts[2])
    };
    let hex = format!("#{r:02x}{g:02x}{b:02x}");
    let (h, s, l) = rgb_to_hsl(r, g, b);
    Some(TransformResult {
        label: "color".into(),
        value: format!("{hex}  ·  rgb({r}, {g}, {b})  ·  hsl({h:.0}, {:.0}%, {:.0}%)", s * 100.0, l * 100.0),
        swatch: Some(hex),
        ..Default::default()
    })
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if (max - r).abs() < f32::EPSILON {
        ((g - b) / d).rem_euclid(6.0)
    } else if (max - g).abs() < f32::EPSILON {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } * 60.0;
    (h, s, l)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The local helpers reach the box: each verb answers, and the ones that
    /// would shadow a search (py on English words, a bare timer) stay quiet.
    #[test]
    fn handy_verbs_answer_from_the_box() {
        assert_eq!(transform("大写 1234.56").unwrap().value, "壹仟贰佰叁拾肆元伍角陆分");
        assert_eq!(transform("dx 100").unwrap().value, "壹佰元整");
        assert!(transform("大写 abc").is_none());
        assert_eq!(transform("py 重庆").unwrap().value, "chóng qìng");
        assert!(transform("py thon").is_none(), "no Chinese: a search, not pinyin");
        assert_eq!(transform("半角 ＡＢＣ１２３").unwrap().value, "ABC123");
        assert_eq!(transform("全角 AB").unwrap().value, "ＡＢ");
        assert_eq!(transform("html <b>").unwrap().value, "&lt;b&gt;");
        let r = transform("random 10 20").unwrap();
        let n: i64 = r.value.parse().unwrap();
        assert!((10..=20).contains(&n) && r.label == "random 10–20");
        assert!((1..=6).contains(&transform("dice").unwrap().value.parse::<i64>().unwrap()));
        assert!(["正面", "反面"].contains(&transform("抛硬币").unwrap().value.as_str()));
        let p = transform("pick 火锅 烧烤 麻辣烫").unwrap();
        assert!(["火锅", "烧烤", "麻辣烫"].contains(&p.value.as_str()) && p.label == "picked from 3");
        assert!(transform("pick onlyone").is_none());
        let t = transform("timer 25m 开会").unwrap();
        assert_eq!((t.timer, t.value.as_str(), t.label.as_str()), (Some(1500), "开会", "timer 25:00"));
        assert_eq!(transform("倒计时 1h").unwrap().timer, Some(3600));
        assert!(transform("timer").is_none() && transform("timer soon").is_none());
        assert!(transform("sys").unwrap().value.starts_with("CPU "));
    }
    /// `ip`: a LAN address that is not loopback, the first one ranked; the
    /// ranking puts real adapters before virtual ones and private before public.
    #[test]
    fn ip_lists_this_machines_lan_addresses() {
        if let Some(r) = transform("ip") {
            let ip: std::net::Ipv4Addr = r.value.parse().unwrap();
            assert!(!ip.is_loopback() && !ip.is_link_local(), "{ip}");
            assert!(r.label.starts_with("local IP · "), "{}", r.label);
            assert_eq!(transform("本机IP").map(|r| r.value), Some(ip.to_string()));
        }
        let all = local_ipv4s();
        let keys: Vec<(bool, bool)> = all
            .iter()
            .map(|(n, ip)| (["vethernet", "wsl", "docker", "vmware", "virtualbox", "vbox", "hyper-v", "veth", "bridge", "utun"].iter().any(|x| n.to_lowercase().contains(x)), !ip.is_private()))
            .collect();
        assert!(keys.windows(2).all(|w| w[0] <= w[1]), "ranked: {all:?}");
        assert!(transform("ip address lookup").is_none(), "ip with words is a search");
    }
    #[test]
    fn generators_have_expected_shape() {
        let u = transform("uuid").unwrap().value;
        assert_eq!(u.len(), 36);
        assert_eq!(u.as_bytes()[14], b'4', "version nibble");
        assert_ne!(transform("uuid").unwrap().value, u, "uuids must differ");

        let p = transform("pwd 24").unwrap().value;
        assert_eq!(p.len(), 24);
        assert_ne!(transform("pwd 24").unwrap().value, p, "passwords must differ");
        assert!(transform("pwd 2").is_none(), "too short rejected");

        assert!(transform("now").unwrap().value.contains("·"));
        // known instant round-trips through ts decode (label says local time)
        let t = transform("ts 1700000000").unwrap();
        assert_eq!(t.label, "local time");
        assert!(t.value.starts_with("2023-11-1"), "{}", t.value);
    }

    #[test]
    fn encoders_round_trip() {
        assert_eq!(transform("b64 hello 世界").unwrap().value, "aGVsbG8g5LiW55WM");
        assert_eq!(transform("unb64 aGVsbG8g5LiW55WM").unwrap().value, "hello 世界");
        assert_eq!(transform("url a b/中").unwrap().value, "a%20b%2F%E4%B8%AD");
        assert_eq!(transform("unurl a%20b%2F%E4%B8%AD").unwrap().value, "a b/中");
        assert!(transform("unb64 not-base64!!").is_none());
    }

    #[test]
    fn colors_convert_both_ways_with_swatch() {
        let c = transform("#ff6600").unwrap();
        assert!(c.value.contains("rgb(255, 102, 0)"));
        assert_eq!(c.swatch.as_deref(), Some("#ff6600"));
        let c2 = transform("rgb(255, 102, 0)").unwrap();
        assert!(c2.value.starts_with("#ff6600"));
        let c3 = transform("#f60").unwrap();
        assert!(c3.value.contains("rgb(255, 102, 0)"), "short hex expands");
        assert!(transform("#zzz").is_none());
    }

    #[test]
    fn plain_queries_pass_through() {
        for q in ["magpie", "url", "b64", "pwd abc", "rgb()", "#toolong7"] {
            assert!(transform(q).is_none(), "{q:?} must not transform");
        }
    }

    #[test]
    fn json_keeps_key_order_minifies_sorts_and_explains_errors() {
        let t = transform(r#"json {"b":1,"a":[1,2],"c":{"z":0,"y":1}}"#).unwrap();
        assert!(!t.error);
        assert_eq!(t.value, "{\n  \"b\": 1,\n  \"a\": [\n    1,\n    2\n  ],\n  \"c\": {\n    \"z\": 0,\n    \"y\": 1\n  }\n}", "original key order");
        assert_eq!(transform(r#"json min {"b": 1, "a": [1, 2]}"#).unwrap().value, r#"{"b":1,"a":[1,2]}"#);
        let sorted = transform(r#"json sort {"b":1,"a":2}"#).unwrap().value;
        assert!(sorted.find("\"a\"").unwrap() < sorted.find("\"b\"").unwrap(), "{sorted}");
        // escaped, with and without the outer quotes (as logs print it)
        assert_eq!(transform(r#"json min "{\"a\":1}""#).unwrap().value, r#"{"a":1}"#);
        assert_eq!(transform(r#"json min {\"a\":{\"b\":2}}"#).unwrap().value, r#"{"a":{"b":2}}"#);
        // a plain JSON string stays a string
        assert_eq!(transform(r#"json "hello""#).unwrap().value, r#""hello""#);
        // errors say where
        let e = transform("json {\"a\":1,\n\"b\" 2}").unwrap();
        assert!(e.error, "{e:?}");
        assert!(e.value.starts_with("line 2 column 5:"), "{}", e.value);
        assert!(transform("json {\"a\":1} trailing").unwrap().error);
        // the verbs alone are not treated as JSON text
        assert!(transform("json min").is_none() || !transform("json min").unwrap().value.contains("min"));
    }

    #[test]
    fn hashes_match_known_digests() {
        assert_eq!(transform("md5 abc").unwrap().value, "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(transform("sha1 abc").unwrap().value, "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(
            transform("sha256 abc").unwrap().value,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(transform("sha256 abc").unwrap().label, "SHA-256 of 3 bytes");
        // UTF-8 bytes, not chars
        assert_eq!(transform("md5 中").unwrap().label, "MD5 of 3 bytes");
    }

    #[test]
    fn jwt_decodes_header_payload_and_expiry() {
        use base64::Engine;
        let enc = |s: &str| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(s);
        let tok = |payload: &str| format!("{}.{}.sig", enc(r#"{"alg":"HS256","typ":"JWT"}"#), enc(payload));
        let t = transform(&format!("jwt {}", tok(r#"{"sub":"42","name":"Ann","exp":4102444800}"#))).unwrap();
        assert!(!t.error);
        assert!(t.label.starts_with("JWT HS256 · expires 2100-01-01") || t.label.starts_with("JWT HS256 · expires 2099-12-31"), "{}", t.label);
        assert!(t.value.find("\"sub\"").unwrap() < t.value.find("\"name\"").unwrap(), "payload order kept");
        let old = transform(&format!("jwt Bearer {}", tok(r#"{"exp":1000000000}"#))).unwrap();
        assert!(old.label.contains("expired 2001-09-"), "{}", old.label);
        assert!(transform(&format!("jwt {}", tok(r#"{"a":1}"#))).unwrap().label.ends_with("no expiry"));
        assert!(transform("jwt not.a.jwt").unwrap().error);
        assert!(transform("jwt abc").unwrap().error);
    }

    #[test]
    fn case_styles_split_camel_humps_and_acronyms() {
        let v = |q: &str| transform(q).unwrap().value;
        assert_eq!(v("snake MyMacCleaner"), "my_mac_cleaner");
        assert_eq!(v("kebab HTTPServer config"), "http-server-config");
        assert_eq!(v("camel user_id from-db"), "userIdFromDb");
        assert_eq!(v("pascal user id"), "UserId");
        assert_eq!(v("title the quick BROWN fox"), "The Quick Brown Fox");
        assert_eq!(v("snake version2Beta"), "version_2_beta");
        assert_eq!(v("camel 中文 name"), "中文Name");
        assert!(transform("snake ---").is_none());
    }

    #[test]
    fn qr_renders_a_scannable_size_and_refuses_what_does_not_fit() {
        use base64::Engine;
        let t = transform("qr https://github.com/newdee/magpie").unwrap();
        assert!(!t.error);
        let png = base64::engine::general_purpose::STANDARD.decode(t.image.unwrap()).unwrap();
        let img = image::load_from_memory(&png).unwrap();
        assert!(img.width() >= 240 && img.width() == img.height(), "{}x{}", img.width(), img.height());
        assert_eq!(t.value, "https://github.com/newdee/magpie");
        let long = format!("qr {}", "x".repeat(4000));
        let e = transform(&long).unwrap();
        assert!(e.error && e.image.is_none(), "{}", e.value);
    }
}
