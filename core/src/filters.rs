//! Query filters for local file search: `ext:pdf`, `.md`, `>10mb`, `<500kb`,
//! `7d`, `in:projects`. Filter tokens are stripped from the text before it
//! reaches FTS and the embedder; what they describe narrows the candidates.

#[derive(Debug, Default, PartialEq)]
pub struct Filters {
    /// Lower-case extensions without the dot; any of them matches.
    pub exts: Vec<String>,
    pub min_size: Option<i64>,
    pub max_size: Option<i64>,
    /// Modified within this many days.
    pub within_days: Option<i64>,
    /// Case-insensitive substrings the path must contain (all of them).
    pub in_paths: Vec<String>,
}

impl Filters {
    pub fn is_empty(&self) -> bool {
        *self == Filters::default()
    }

    pub fn matches(&self, ext: Option<&str>, size: i64, mtime: i64, path: &str, now: i64) -> bool {
        if !self.exts.is_empty() {
            let e = ext.map(|e| e.to_lowercase()).unwrap_or_default();
            if !self.exts.contains(&e) {
                return false;
            }
        }
        if let Some(min) = self.min_size {
            if size < min {
                return false;
            }
        }
        if let Some(max) = self.max_size {
            if size > max {
                return false;
            }
        }
        if let Some(days) = self.within_days {
            if mtime < now - days * 86_400 {
                return false;
            }
        }
        if !self.in_paths.is_empty() {
            let p = path.to_lowercase();
            if !self.in_paths.iter().all(|s| p.contains(s.as_str())) {
                return false;
            }
        }
        true
    }
}

/// Split a query into its filters and the text that is left to search.
pub fn parse(query: &str) -> (Filters, String) {
    let mut f = Filters::default();
    let mut text = Vec::new();
    for tok in query.split_whitespace() {
        let lower = tok.to_lowercase();
        if let Some(rest) = lower.strip_prefix("ext:") {
            f.exts.extend(rest.split(',').map(|e| e.trim_start_matches('.').to_string()).filter(|e| !e.is_empty()));
        } else if let Some(rest) = lower.strip_prefix("in:") {
            if !rest.is_empty() {
                f.in_paths.push(rest.to_string());
            }
        } else if lower.len() > 1
            && lower.len() <= 7
            && lower.starts_with('.')
            && lower[1..].chars().all(|c| c.is_ascii_alphanumeric())
        {
            f.exts.push(lower[1..].to_string());
        } else if let Some(bytes) = lower.strip_prefix('>').and_then(parse_size) {
            f.min_size = Some(bytes);
        } else if let Some(bytes) = lower.strip_prefix('<').and_then(parse_size) {
            f.max_size = Some(bytes);
        } else if let Some(days) = parse_days(&lower) {
            f.within_days = Some(days);
        } else {
            text.push(tok);
        }
    }
    (f, text.join(" "))
}

/// `10mb`, `1.5gb`, `500kb`, `200b`, `3m` (bytes, 1024-based).
fn parse_size(s: &str) -> Option<i64> {
    let s = s.trim_start_matches('=');
    let split = s.find(|c: char| c.is_ascii_alphabetic())?;
    let n: f64 = s[..split].parse().ok()?;
    let mult = match &s[split..] {
        "b" => 1.0,
        "k" | "kb" => 1024.0,
        "m" | "mb" => 1024.0 * 1024.0,
        "g" | "gb" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((n * mult) as i64)
}

/// `7d`, `2w`, `3m`, `1y`: modified within that many days. The token has to
/// be exactly digits plus one unit letter, so "7d" in a filename would need
/// quoting, and "d" alone is still text.
fn parse_days(s: &str) -> Option<i64> {
    // split on the last CHAR, not the last byte: a CJK token would otherwise
    // be cut inside a code point and panic
    let unit = s.chars().next_back()?;
    let num = &s[..s.len() - unit.len_utf8()];
    if num.is_empty() || !num.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n: i64 = num.parse().ok()?;
    match unit {
        'd' => Some(n),
        'w' => Some(n * 7),
        'm' => Some(n * 30),
        'y' => Some(n * 365),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_filters_and_keeps_the_text() {
        let (f, text) = parse("invoice ext:pdf >10mb 7d in:finance");
        assert_eq!(text, "invoice");
        assert_eq!(f.exts, vec!["pdf"]);
        assert_eq!(f.min_size, Some(10 * 1024 * 1024));
        assert_eq!(f.within_days, Some(7));
        assert_eq!(f.in_paths, vec!["finance"]);
    }

    #[test]
    fn dot_extension_and_lists() {
        let (f, text) = parse(".md notes ext:txt,rst");
        assert_eq!(text, "notes");
        assert_eq!(f.exts, vec!["md", "txt", "rst"]);
    }

    #[test]
    fn size_units_and_bounds() {
        let (f, _) = parse("<1.5gb >500kb");
        assert_eq!(f.max_size, Some((1.5 * 1024.0 * 1024.0 * 1024.0) as i64));
        assert_eq!(f.min_size, Some(500 * 1024));
    }

    #[test]
    fn day_units() {
        assert_eq!(parse("2w").0.within_days, Some(14));
        assert_eq!(parse("3m").0.within_days, Some(90));
        assert_eq!(parse("1y").0.within_days, Some(365));
        // not filters: a bare unit, a word, a version-like token
        assert_eq!(parse("d").1, "d");
        assert_eq!(parse("7days").1, "7days");
        assert_eq!(parse("v2").1, "v2");
    }

    #[test]
    fn plain_queries_pass_through_untouched() {
        let (f, text) = parse("vector search notes");
        assert!(f.is_empty());
        assert_eq!(text, "vector search notes");
    }

    #[test]
    fn multibyte_tokens_are_text_not_a_crash() {
        // every token classifier slices the string; none may cut a code point
        let (f, text) = parse("发票 报销单 7d ext:pdf 年度總結");
        assert_eq!(text, "发票 报销单 年度總結");
        assert_eq!(f.within_days, Some(7));
        assert_eq!(f.exts, vec!["pdf"]);
        assert!(parse("é").0.is_empty());
        assert_eq!(parse(".图片").1, ".图片", "a non-ascii 'extension' stays text");
    }

    #[test]
    fn matching_applies_every_constraint() {
        let (f, _) = parse("ext:pdf >1kb 7d in:docs");
        let now = 1_000_000_000;
        let fresh = now - 3 * 86_400;
        let old = now - 30 * 86_400;
        assert!(f.matches(Some("pdf"), 5000, fresh, "C:/docs/a.pdf", now));
        assert!(!f.matches(Some("md"), 5000, fresh, "C:/docs/a.md", now), "ext");
        assert!(!f.matches(Some("pdf"), 100, fresh, "C:/docs/a.pdf", now), "size");
        assert!(!f.matches(Some("pdf"), 5000, old, "C:/docs/a.pdf", now), "age");
        assert!(!f.matches(Some("pdf"), 5000, fresh, "C:/other/a.pdf", now), "path");
        assert!(f.matches(Some("PDF"), 5000, fresh, "C:/DOCS/a.pdf", now), "case-insensitive");
    }

    #[test]
    fn empty_filters_match_everything() {
        assert!(Filters::default().matches(None, 0, 0, "", 0));
    }
}
