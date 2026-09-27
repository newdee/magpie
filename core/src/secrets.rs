//! Clipboard text that looks like a password or an access key, and the form
//! it is shown in: head and tail kept, the middle starred (issue #6). Copying
//! a clip still gives the full text; only what is drawn on screen, and what
//! the MCP server hands to other programs, is masked.
//!
//! Three signals, all local:
//! - keys with a published shape (GitHub, OpenAI, AWS, Slack, …), found
//!   anywhere in the text, so a pasted `.env` masks just the key;
//! - the value in `password = …` / `API_KEY: …` / `密码：…`;
//! - a whole clip that is one word mixing three kinds of characters, the way
//!   generated passwords do. A few identifiers look like that too; the cost
//!   of such a miss is one click to show it.

use regex::Regex;
use std::ops::Range;
use std::sync::OnceLock;

/// What stands in for the hidden middle. Fixed, so it does not tell the length.
const STARS: &str = "****";

fn known_keys() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(concat!(
            r"\b(?:",
            r"gh[pousr]_[A-Za-z0-9]{30,}",
            r"|github_pat_[A-Za-z0-9_]{30,}",
            r"|glpat-[A-Za-z0-9_\-]{20,}",
            r"|sk-[A-Za-z0-9_\-]{20,}",
            r"|(?:sk|rk)_(?:live|test)_[A-Za-z0-9]{16,}",
            r"|A(?:KIA|SIA)[0-9A-Z]{16}",
            r"|AIza[0-9A-Za-z_\-]{35}",
            r"|xox[abprs]-[A-Za-z0-9\-]{10,}",
            r"|hf_[A-Za-z0-9]{30,}",
            r"|npm_[A-Za-z0-9]{36}",
            r"|eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}",
            r")"
        ))
        .expect("key patterns")
    })
}

/// `name = value` where the name says it is a secret; group 1 is the value.
fn assignments() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(concat!(
            r#"(?i)(?:\b[\w\-]*?(?:password|passwd|pwd|secret|token|api_?key|apikey|access_?key)|密码|口令)"#,
            r#"["']?[ \t]*[:=：][ \t]*["']?([^\s"',;]+)"#
        ))
        .expect("assignment pattern")
    })
}

fn private_keys() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----([\s\S]*?)-----END [A-Z ]*PRIVATE KEY-----")
            .expect("pem pattern")
    })
}

/// A value after `password =` worth hiding: long enough, and not a plain
/// lowercase word (`token: string` in code is a type, not a token).
fn secret_value(v: &str) -> bool {
    let n = v.chars().count();
    n >= 6 && (n >= 12 || v.chars().any(|c| !c.is_ascii_lowercase()))
}

/// A whole clip that is one generated-looking word: 8–64 printable ASCII
/// characters, no spaces, at least three of upper / lower / digit / symbol.
/// `. - _ / : @ \ =` do not count as symbols: they make versions, file
/// names, e-mail addresses, paths and `key=value`, not passwords.
pub fn looks_like_password(s: &str) -> bool {
    let n = s.chars().count();
    if !(8..=64).contains(&n) || !s.chars().all(|c| c.is_ascii_graphic()) {
        return false;
    }
    let upper = s.chars().any(|c| c.is_ascii_uppercase());
    let lower = s.chars().any(|c| c.is_ascii_lowercase());
    let digit = s.chars().any(|c| c.is_ascii_digit());
    let symbol = s.chars().any(|c| c.is_ascii_punctuation() && !"._-/:@\\=".contains(c));
    [upper, lower, digit, symbol].iter().filter(|&&b| b).count() >= 3
        && !s.contains("://")
        && !looks_like_email(s)
}

fn looks_like_email(s: &str) -> bool {
    match s.split_once('@') {
        Some((user, host)) => {
            !user.is_empty()
                && host.contains('.')
                && !host.contains('@')
                && s.chars().all(|c| c.is_ascii_alphanumeric() || "._%+-@".contains(c))
        }
        None => false,
    }
}

/// Head and tail kept, the middle starred. How much shows grows with the
/// length but stays a small share: a sixth at each end, one to six
/// characters. An 8-character password shows one at each end; showing two
/// would leave only four to guess.
pub fn mask_token(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let keep = (chars.len() / 6).clamp(1, 6);
    if chars.len() <= keep * 2 + 1 {
        return STARS.to_string();
    }
    let head: String = chars[..keep].iter().collect();
    let tail: String = chars[chars.len() - keep..].iter().collect();
    format!("{head}{STARS}{tail}")
}

/// The spans of `text` to hide, sorted and merged.
fn secret_spans(text: &str) -> Vec<Range<usize>> {
    let mut spans: Vec<Range<usize>> = Vec::new();
    for m in known_keys().find_iter(text) {
        spans.push(m.range());
    }
    for c in assignments().captures_iter(text) {
        if let Some(v) = c.get(1) {
            if secret_value(v.as_str()) {
                spans.push(v.range());
            }
        }
    }
    for c in private_keys().captures_iter(text) {
        if let Some(body) = c.get(1) {
            if !body.as_str().trim().is_empty() {
                spans.push(body.range());
            }
        }
    }
    if spans.is_empty() {
        let t = text.trim();
        if looks_like_password(t) {
            let start = text.len() - text.trim_start().len();
            spans.push(start..start + t.len());
        }
    }
    spans.sort_by_key(|r| (r.start, std::cmp::Reverse(r.end)));
    let mut merged: Vec<Range<usize>> = Vec::new();
    for r in spans {
        match merged.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => merged.push(r),
        }
    }
    merged
}

/// `text` with every secret in it masked, or `None` when it holds none.
pub fn masked(text: &str) -> Option<String> {
    let spans = secret_spans(text);
    if spans.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for r in spans {
        out.push_str(&text[at..r.start]);
        let part = &text[r.clone()];
        if part.contains('\n') {
            // a private key's body: nothing of it is worth showing
            out.push('\n');
            out.push_str(STARS);
            out.push('\n');
        } else {
            out.push_str(&mask_token(part));
        }
        at = r.end;
    }
    out.push_str(&text[at..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test keys are assembled at run time: a literal that looks like a live
    // key would trip secret scanners on push, and none of these is real.
    fn fake(prefix: &str, body: &str, times: usize) -> String {
        format!("{prefix}{}", body.repeat(times))
    }

    #[test]
    fn known_keys_are_masked_wherever_they_sit() {
        let gh = fake("ghp_", "a1B2", 9);
        assert_eq!(masked(&gh).unwrap(), "ghp_a1****B2a1B2");
        // inside other text only the key goes
        let env = format!("GITHUB_TOKEN={gh}\nDEBUG=true\n");
        let m = masked(&env).unwrap();
        assert!(m.starts_with("GITHUB_TOKEN=ghp_a1****") && m.ends_with("\nDEBUG=true\n"), "{m}");
        assert!(!m.contains(&gh));
        // the demo's clip (src/demo-mock.ts) carries this masked form by hand
        let demo = format!("OPENAI_API_KEY={}", fake("sk-proj-", "Xy9zQw3e", 4));
        assert_eq!(masked(&demo).unwrap(), "OPENAI_API_KEY=sk-pro****9zQw3e");
        for key in [
            fake("github_pat_", "Ab1_", 10),
            fake("sk-proj-", "Xy9z", 8),
            fake("AKIA", "ABCD2345", 2),
            fake("xoxb-", "12345-", 4),
            format!("{}.{}.{}", fake("eyJ", "hbGciOi", 2), "eyJzdWIiOiIxMjM0", "SflKxwRJSMeKKF2QT4"),
        ] {
            let m = masked(&format!("key: {key} end")).unwrap_or_default();
            assert!(!m.contains(&key) && m.contains(STARS), "{key} -> {m}");
        }
    }

    #[test]
    fn named_values_are_masked_but_types_and_prose_are_not() {
        assert_eq!(masked("password = hunter2!").unwrap(), "password = h****!");
        assert_eq!(masked("\"api_key\": \"Zx81-kk20-PPq9\"").unwrap(), "\"api_key\": \"Zx****q9\"");
        assert_eq!(masked("wifi 密码：Tp8#link2026").unwrap(), "wifi 密码：Tp****26");
        assert_eq!(masked("token: string;"), None, "a type annotation");
        assert_eq!(masked("max_tokens=4096"), None, "tokens is not token");
        assert_eq!(masked("Forgot your password? Reset it here."), None);
    }

    #[test]
    fn a_whole_clip_that_looks_generated_is_masked() {
        assert_eq!(masked("Tr0ub4dor&3").unwrap(), "T****3");
        assert_eq!(masked("  q8#Lm2!vZr9pK4x  ").unwrap(), "  q8****4x  ");
        for plain in [
            "hello world",
            "v0.4.6-rc1",
            "report_2024.pdf",
            "John.Doe99@example.com",
            "https://Example.com/a1",
            "3f786850e387550fdab836ed7e6dc881de23001b",
            "550e8400-e29b-41d4-a716-446655440000",
            "short1A",
            "日本語のパスワード1A!",
        ] {
            assert_eq!(masked(plain), None, "{plain}");
        }
    }

    #[test]
    fn a_private_key_body_is_hidden_whole() {
        let pem = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAA\nQyNTUxOQAAACD\n-----END OPENSSH PRIVATE KEY-----\n";
        let m = masked(pem).unwrap();
        assert_eq!(m, "-----BEGIN OPENSSH PRIVATE KEY-----\n****\n-----END OPENSSH PRIVATE KEY-----\n");
    }

    #[test]
    fn degenerate_clips_never_panic_and_stay_fast() {
        for s in ["", " ", "\n\n", "****", "=", "password=", "密码：", "🔑🔑🔑🔑🔑🔑🔑🔑", "-----BEGIN RSA PRIVATE KEY-----\nabc"] {
            let _ = masked(s);
        }
        assert_eq!(masked(""), None);
        assert_eq!(masked("-----BEGIN RSA PRIVATE KEY-----\nabc"), None, "no END line: not a key block");
        // the largest clip history keeps (100 000 chars), prose and one long word
        let prose = "the quick brown fox jumps over the lazy dog ".repeat(2300);
        let word = "aB3".repeat(33_000);
        let t = std::time::Instant::now();
        assert_eq!(masked(&prose), None);
        assert_eq!(masked(&word), None, "one word longer than 64 chars is not a password");
        // a file of a thousand keys: all hidden, none left over
        let many: String = (0..1000).map(|i| format!("k{i}={}\n", fake("ghp_", "Zz9y", 9))).collect();
        let m = masked(&many).unwrap();
        assert!(!m.contains(&fake("ghp_", "Zz9y", 9)) && m.matches(STARS).count() == 1000);
        assert!(t.elapsed().as_millis() < 500, "took {:?}", t.elapsed());
    }

    #[test]
    fn masking_shows_a_small_share_and_never_the_length() {
        assert_eq!(mask_token("abcd1234"), "a****4");
        assert_eq!(mask_token("abcdefghijkl1234567890!@#$%"), "abcd****@#$%");
        let long = "x".repeat(200);
        assert_eq!(mask_token(&long), format!("xxxxxx{STARS}xxxxxx"));
        assert_eq!(mask_token("abc"), STARS);
        // multibyte text is cut on characters, not bytes
        assert_eq!(mask_token("密码密码密码密码"), "密****码");
    }
}
