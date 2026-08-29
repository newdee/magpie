//! Query-box text transforms: generators (uuid / timestamps / passwords),
//! encoders (base64, URL percent-encoding), and color conversion. Siblings
//! of the calculator — same top-row UI, Enter copies the value.

use base64::Engine;

/// The display payload for a transform hit. `swatch` carries a `#rrggbb`
/// for color queries so the UI can paint a preview chip.
#[derive(Debug, PartialEq)]
pub struct TransformResult {
    pub label: String,
    pub value: String,
    pub swatch: Option<String>,
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
        }),
        "now" | "time" if rest.is_empty() => {
            let now = chrono::Local::now();
            Some(TransformResult {
                label: "unix".into(),
                value: format!("{}  ·  {}", now.timestamp(), now.format("%Y-%m-%d %H:%M:%S %z")),
                swatch: None,
            })
        }
        "ts" | "timestamp" => {
            if rest.is_empty() {
                Some(TransformResult {
                    label: "unix timestamp".into(),
                    value: chrono::Local::now().timestamp().to_string(),
                    swatch: None,
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
            })
        }
        "b64" if !rest.is_empty() => Some(TransformResult {
            label: "base64".into(),
            value: base64::engine::general_purpose::STANDARD.encode(rest.as_bytes()),
            swatch: None,
        }),
        "unb64" if !rest.is_empty() => {
            let bytes = base64::engine::general_purpose::STANDARD.decode(rest.trim()).ok()?;
            let text = String::from_utf8(bytes).ok()?;
            Some(TransformResult { label: "decoded".into(), value: text, swatch: None })
        }
        "url" if !rest.is_empty() => Some(TransformResult {
            label: "url-encoded".into(),
            value: percent_encode(rest),
            swatch: None,
        }),
        "unurl" if !rest.is_empty() => Some(TransformResult {
            label: "url-decoded".into(),
            value: percent_decode(rest)?,
            swatch: None,
        }),
        _ => color(q),
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
}
