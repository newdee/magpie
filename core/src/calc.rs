//! Inline calculator + unit conversion for the query box.
//!
//! A hand-rolled Pratt parser — no dependency, and the grammar stays exactly
//! as small as the feature: arithmetic (+ - * / % ^, parentheses, floats,
//! 0x/0b integer literals) and `<value> <unit> to <unit>` conversions for
//! data sizes (1024-based), length, mass, and temperature. Anything that
//! doesn't parse returns None — the caller treats that as "not a formula"
//! and shows normal search results.

/// A successful evaluation: the display string, plus an alternate rendering
/// (hex for integer results when the input used hex/binary literals).
#[derive(Debug, PartialEq)]
pub struct CalcResult {
    pub value: String,
    pub alt: Option<String>,
}

pub fn eval(query: &str) -> Option<CalcResult> {
    let q = query.trim();
    if q.len() < 2 || q.len() > 200 {
        return None;
    }
    if let Some(r) = eval_date(q) {
        return Some(r);
    }
    if let Some(r) = eval_conversion(q) {
        return Some(r);
    }
    // quick reject: expressions contain only this alphabet, and must have at
    // least one operator or be a base literal (plain "42" is a search query)
    let ok_chars = q
        .chars()
        .all(|c| c.is_ascii_hexdigit() || " .+-*/%^()xXbB_".contains(c));
    let has_op = q.chars().any(|c| "+*/%^".contains(c))
        || (q.contains('-') && !q.starts_with('-'))
        || q.starts_with("0x")
        || q.starts_with("0b");
    if !ok_chars || !has_op {
        return None;
    }
    let mut p = Parser { s: q.as_bytes(), i: 0, used_base_literal: false };
    let v = p.expr(0)?;
    p.skip_ws();
    if p.i != p.s.len() || !v.is_finite() {
        return None;
    }
    let alt = (p.used_base_literal && v.fract() == 0.0 && v.abs() < 9e15)
        .then(|| format!("0x{:X}", v as i64));
    Some(CalcResult { value: fmt_num(v), alt })
}

fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    used_base_literal: bool,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i] == b' ' {
            self.i += 1;
        }
    }

    fn expr(&mut self, min_bp: u8) -> Option<f64> {
        self.skip_ws();
        let mut lhs = match self.s.get(self.i)? {
            b'(' => {
                self.i += 1;
                let v = self.expr(0)?;
                self.skip_ws();
                if self.s.get(self.i) != Some(&b')') {
                    return None;
                }
                self.i += 1;
                v
            }
            b'-' => {
                self.i += 1;
                -self.expr(7)?
            }
            _ => self.number()?,
        };
        loop {
            self.skip_ws();
            let (op, l_bp, r_bp) = match self.s.get(self.i) {
                Some(b'+') => (b'+', 1, 2),
                Some(b'-') => (b'-', 1, 2),
                Some(b'*') => (b'*', 3, 4),
                Some(b'/') => (b'/', 3, 4),
                Some(b'%') => (b'%', 3, 4),
                Some(b'^') => (b'^', 6, 5), // right-assoc
                _ => break,
            };
            if l_bp < min_bp {
                break;
            }
            self.i += 1;
            let rhs = self.expr(r_bp)?;
            lhs = match op {
                b'+' => lhs + rhs,
                b'-' => lhs - rhs,
                b'*' => lhs * rhs,
                b'/' => lhs / rhs,
                b'%' => {
                    if rhs == 0.0 {
                        return None;
                    }
                    lhs % rhs
                }
                b'^' => lhs.powf(rhs),
                _ => unreachable!(),
            };
        }
        Some(lhs)
    }

    fn number(&mut self) -> Option<f64> {
        self.skip_ws();
        let start = self.i;
        let rest = &self.s[self.i..];
        if rest.starts_with(b"0x") || rest.starts_with(b"0X") {
            self.i += 2;
            let d0 = self.i;
            while self.i < self.s.len() && (self.s[self.i].is_ascii_hexdigit() || self.s[self.i] == b'_') {
                self.i += 1;
            }
            let txt: String = std::str::from_utf8(&self.s[d0..self.i]).ok()?.replace('_', "");
            let v = i64::from_str_radix(&txt, 16).ok()?;
            self.used_base_literal = true;
            return Some(v as f64);
        }
        if rest.starts_with(b"0b") || rest.starts_with(b"0B") {
            self.i += 2;
            let d0 = self.i;
            while self.i < self.s.len() && (self.s[self.i] == b'0' || self.s[self.i] == b'1' || self.s[self.i] == b'_') {
                self.i += 1;
            }
            let txt: String = std::str::from_utf8(&self.s[d0..self.i]).ok()?.replace('_', "");
            let v = i64::from_str_radix(&txt, 2).ok()?;
            self.used_base_literal = true;
            return Some(v as f64);
        }
        while self.i < self.s.len() && (self.s[self.i].is_ascii_digit() || self.s[self.i] == b'.') {
            self.i += 1;
        }
        if self.i == start {
            return None;
        }
        std::str::from_utf8(&self.s[start..self.i]).ok()?.parse().ok()
    }
}

// ---------- unit conversion ----------

/// (canonical name, aliases, factor to base unit). Base units: byte, meter,
/// gram. Temperature is special-cased.
const DATA: &[(&str, &[&str], f64)] = &[
    ("B", &["b", "byte", "bytes"], 1.0),
    ("KB", &["kb"], 1024.0),
    ("MB", &["mb"], 1048576.0),
    ("GB", &["gb"], 1073741824.0),
    ("TB", &["tb"], 1099511627776.0),
];
const LENGTH: &[(&str, &[&str], f64)] = &[
    ("mm", &["mm"], 0.001),
    ("cm", &["cm"], 0.01),
    ("m", &["m", "meter", "meters"], 1.0),
    ("km", &["km"], 1000.0),
    ("in", &["in", "inch", "inches"], 0.0254),
    ("ft", &["ft", "foot", "feet"], 0.3048),
    ("mi", &["mi", "mile", "miles"], 1609.344),
];
const MASS: &[(&str, &[&str], f64)] = &[
    ("g", &["g", "gram", "grams"], 1.0),
    ("kg", &["kg"], 1000.0),
    ("lb", &["lb", "lbs", "pound", "pounds"], 453.59237),
    ("oz", &["oz", "ounce", "ounces"], 28.349523),
];

fn find_unit(table: &'static [(&str, &[&str], f64)], u: &str) -> Option<(&'static str, f64)> {
    let lu = u.to_lowercase();
    table
        .iter()
        .find(|(_, aliases, _)| aliases.contains(&lu.as_str()))
        .map(|(name, _, f)| (*name, *f))
}

/// `<number> <unit> to <unit>` (also accepts "in" as the connector).
fn eval_conversion(q: &str) -> Option<CalcResult> {
    let lower = q.to_lowercase();
    let parts: Vec<&str> = lower.split_whitespace().collect();
    // formats: [num, unit, to, unit] or [num+unit, to, unit]
    let (num_txt, from_txt, to_txt) = match parts.as_slice() {
        [n, f, c, t] if *c == "to" || *c == "in" => (*n, *f, *t),
        [nf, c, t] if *c == "to" || *c == "in" => {
            let split = nf.find(|ch: char| ch.is_ascii_alphabetic())?;
            (&nf[..split], &nf[split..], *t)
        }
        _ => return None,
    };
    let n: f64 = num_txt.parse().ok()?;
    // temperature first (non-linear)
    let temp = |s: &str| matches!(s, "c" | "f" | "celsius" | "fahrenheit" | "°c" | "°f");
    if temp(from_txt) && temp(to_txt) {
        let from_c = from_txt.starts_with('c') || from_txt == "°c" || from_txt == "celsius";
        let to_c = to_txt.starts_with('c') || to_txt == "°c" || to_txt == "celsius";
        let v = match (from_c, to_c) {
            (true, false) => n * 9.0 / 5.0 + 32.0,
            (false, true) => (n - 32.0) * 5.0 / 9.0,
            _ => n,
        };
        let unit = if to_c { "°C" } else { "°F" };
        return Some(CalcResult { value: format!("{} {unit}", fmt_num(round6(v))), alt: None });
    }
    for table in [DATA, LENGTH, MASS] {
        if let (Some((_, ff)), Some((tn, tf))) = (find_unit(table, from_txt), find_unit(table, to_txt)) {
            let v = round6(n * ff / tf);
            return Some(CalcResult { value: format!("{} {tn}", fmt_num(v)), alt: None });
        }
    }
    None
}

fn round6(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

// ---------- date math ----------

/// Dates in, dates or day counts out:
/// `today + 30d`, `tomorrow - 2w`, `2026-10-01 + 3 months`,
/// `2026-10-01 - today`, `until 2026-10-01`, `2026-10-01` (weekday and
/// distance). A lone `today` stays a search word; only ISO dates stand alone.
fn eval_date(q: &str) -> Option<CalcResult> {
    use chrono::NaiveDate;
    let lower = q.to_lowercase();
    let toks: Vec<&str> = lower.split_whitespace().collect();
    let today = chrono::Local::now().date_naive();
    let date = |s: &str| -> Option<NaiveDate> {
        match s {
            "today" => Some(today),
            "tomorrow" => today.succ_opt(),
            "yesterday" => today.pred_opt(),
            _ => NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .or_else(|_| NaiveDate::parse_from_str(s, "%Y/%m/%d"))
                .ok(),
        }
    };
    let iso = |s: &str| s.len() >= 8 && (s.contains('-') || s.contains('/')) && date(s).is_some();
    // `30d`, `2w`, `3m`, `1y`, or a bare number of days
    let dur = |num: &str, unit: &str| -> Option<(i64, char)> {
        if num.is_empty() || !num.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let n: i64 = num.parse().ok()?;
        let u = match unit {
            "" | "d" | "day" | "days" => 'd',
            "w" | "wk" | "week" | "weeks" => 'w',
            "m" | "mo" | "month" | "months" => 'm',
            "y" | "yr" | "year" | "years" => 'y',
            _ => return None,
        };
        Some((n, u))
    };
    let dur1 = |s: &str| -> Option<(i64, char)> {
        let split = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
        dur(&s[..split], &s[split..])
    };
    let shift = |d: NaiveDate, (n, u): (i64, char), sign: i64| -> Option<NaiveDate> {
        let n = n * sign;
        let months = |m: i64| {
            if m >= 0 {
                d.checked_add_months(chrono::Months::new(m as u32))
            } else {
                d.checked_sub_months(chrono::Months::new((-m) as u32))
            }
        };
        match u {
            'd' => d.checked_add_signed(chrono::Duration::days(n)),
            'w' => d.checked_add_signed(chrono::Duration::days(n * 7)),
            'm' => months(n),
            'y' => months(n * 12),
            _ => None,
        }
    };
    let show = |d: NaiveDate| format!("{}  ·  {}", d.format("%Y-%m-%d"), d.format("%A"));
    let span = |n: i64| -> CalcResult {
        let a = n.abs();
        let alt = if a >= 7 {
            let (w, d) = (a / 7, a % 7);
            Some(if d == 0 { format!("{w} weeks") } else { format!("{w} weeks {d} days") })
        } else {
            None
        };
        CalcResult { value: format!("{n} days"), alt }
    };
    let distance = |d: NaiveDate| -> String {
        let n = (d - today).num_days();
        match n {
            0 => "today".into(),
            1 => "tomorrow".into(),
            -1 => "yesterday".into(),
            n if n > 0 => format!("in {n} days"),
            n => format!("{} days ago", -n),
        }
    };
    match toks.as_slice() {
        [d] if iso(d) => {
            let d = date(d)?;
            Some(CalcResult { value: show(d), alt: Some(distance(d)) })
        }
        ["until" | "till", d] | ["days", "until" | "till", d] => {
            Some(span((date(d)? - today).num_days()))
        }
        ["since", d] | ["days", "since", d] => Some(span((today - date(d)?).num_days())),
        [a, op @ ("+" | "-"), b] => {
            let a = date(a)?;
            let sign = if *op == "+" { 1 } else { -1 };
            if *op == "-" {
                if let Some(b) = date(b) {
                    return Some(span((a - b).num_days()));
                }
            }
            let d = shift(a, dur1(b)?, sign)?;
            Some(CalcResult { value: show(d), alt: Some(distance(d)) })
        }
        [a, op @ ("+" | "-"), n, unit] => {
            let a = date(a)?;
            let sign = if *op == "+" { 1 } else { -1 };
            let d = shift(a, dur(n, unit)?, sign)?;
            Some(CalcResult { value: show(d), alt: Some(distance(d)) })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(q: &str) -> String {
        eval(q).unwrap_or_else(|| panic!("{q:?} must evaluate")).value
    }

    #[test]
    fn date_minus_date_is_a_day_count() {
        assert_eq!(v("2026-10-01 - 2026-09-01"), "30 days");
        assert_eq!(eval("2026-10-01 - 2026-09-01").unwrap().alt.as_deref(), Some("4 weeks 2 days"));
        assert_eq!(v("2026-09-01 - 2026-10-01"), "-30 days");
        assert_eq!(v("2026-09-08 - 2026-09-01"), "7 days");
    }

    #[test]
    fn date_plus_duration_lands_on_a_date_with_its_weekday() {
        assert_eq!(v("2026-10-01 + 2w"), "2026-10-15  ·  Thursday");
        assert_eq!(v("2026-10-01 + 3 months"), "2027-01-01  ·  Friday");
        assert_eq!(v("2026-10-01 - 1y"), "2025-10-01  ·  Wednesday");
        assert_eq!(v("2026-10-01 + 10"), "2026-10-11  ·  Sunday");
        // month arithmetic clamps to the last day instead of overflowing
        assert_eq!(v("2026-01-31 + 1m"), "2026-02-28  ·  Saturday");
    }

    #[test]
    fn a_lone_iso_date_shows_its_weekday() {
        assert!(v("2026-10-01").starts_with("2026-10-01  ·  Thursday"));
        assert!(v("2026/10/01").starts_with("2026-10-01"));
    }

    #[test]
    fn relative_forms_parse_without_pinning_today() {
        assert!(eval("until 2026-10-01").is_some());
        assert!(eval("days since 2020-01-01").is_some());
        assert!(eval("today + 30d").is_some());
        assert!(eval("tomorrow - 1w").is_some());
    }

    #[test]
    fn date_words_alone_stay_search_text() {
        assert!(eval("today").is_none());
        assert!(eval("tomorrow").is_none());
        assert!(eval("today + x").is_none());
        // an impossible date is not a date; the arithmetic path still owns
        // the dashes, as it always has (2026 - 13 - 40)
        assert_eq!(v("2026-13-40"), "1973");
    }

    #[test]
    fn arithmetic_with_precedence_and_parens() {
        assert_eq!(v("3*(5+2)^2"), "147");
        assert_eq!(v("2^3^2"), "512"); // right-assoc
        assert_eq!(v("10 - 4 - 3"), "3"); // left-assoc
        assert_eq!(v("7 % 4 * 2"), "6");
        assert_eq!(v("1/8"), "0.125");
        assert_eq!(v("-3 + 5"), "2");
    }

    #[test]
    fn base_literals_show_hex_alt() {
        let r = eval("0xff + 1").unwrap();
        assert_eq!(r.value, "256");
        assert_eq!(r.alt.as_deref(), Some("0x100"));
        assert_eq!(v("0b1010 * 2"), "20");
    }

    #[test]
    fn conversions() {
        assert_eq!(v("100 mb to gb"), "0.097656 GB");
        assert_eq!(v("1.5gb to mb"), "1536 MB");
        assert_eq!(v("32 f to c"), "0 °C");
        assert_eq!(v("100 c to f"), "212 °F");
        assert_eq!(v("5 km to mi"), "3.106856 mi");
        assert_eq!(v("2 lb to kg"), "0.907185 kg");
    }

    #[test]
    fn non_formulas_return_none() {
        for q in ["magpie", "42", "hello world", "a+b", "3*", "(2+3", "1/0 x", "视频 教程"] {
            assert!(eval(q).is_none(), "{q:?} must not evaluate");
        }
        // division by zero yields inf -> rejected
        assert!(eval("1/0").is_none());
        assert!(eval("5 % 0").is_none());
    }
}
