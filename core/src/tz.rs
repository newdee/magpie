//! Time zones in the query box, offline: the IANA database ships inside the
//! binary (chrono-tz), so nothing is looked up on the network.
//!
//! - `time in tokyo`, `tokyo time`, `now in london`, `东京时间`: the time
//!   there now.
//! - `3pm pst to beijing`, `15:30 tokyo in london`, `9am to new york` (from
//!   local time): a wall-clock time moved between zones, DST included.
//!
//! Anything else returns None and the query stays a search. A bare place
//! name never triggers, so "tokyo" still finds files about Tokyo.

use chrono::{DateTime, FixedOffset, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;

use crate::calc::CalcResult;

pub fn eval(q: &str) -> Option<CalcResult> {
    let now = Utc::now();
    let local = *chrono::Local::now().offset();
    eval_at(q, now, local)
}

/// [`eval`] at a given moment and local offset, so tests are fixed.
pub fn eval_at(q: &str, now: DateTime<Utc>, local: FixedOffset) -> Option<CalcResult> {
    let q = q.trim();
    if q.chars().count() < 4 {
        return None;
    }
    // Chinese: 东京时间 / 东京现在几点
    for suffix in ["现在几点", "时间"] {
        if let Some(place) = q.strip_suffix(suffix) {
            let tz = place_zone(place.trim())?;
            return Some(now_in(now, tz, local));
        }
    }
    let lower = q.to_lowercase();
    // "time in X" / "now in X" / "X time" / "X now"
    for prefix in ["time in ", "now in ", "time at "] {
        if let Some(place) = lower.strip_prefix(prefix) {
            return Some(now_in(now, place_zone(place.trim())?, local));
        }
    }
    for suffix in [" time", " now"] {
        if let Some(place) = lower.strip_suffix(suffix) {
            // "3pm pst time" is not a place; only a whole place name counts
            if let Some(tz) = place_zone(place.trim()) {
                return Some(now_in(now, tz, local));
            }
        }
    }
    // "<time> [<from>] to|in <to>"
    let (left, to) = lower
        .rsplit_once(" to ")
        .or_else(|| lower.rsplit_once(" in "))?;
    let to = place_zone(to.trim())?;
    let (time, from) = split_time(left.trim())?;
    let from = match from {
        Some(p) => Zone::Named(place_zone(p)?),
        None => Zone::Local(local),
    };
    Some(convert(now, time, from, to))
}

enum Zone {
    Named(Tz),
    Local(FixedOffset),
}

/// "3pm", "3 pm", "3:30pm", "15:30", "15" with am/pm, then an optional place:
/// the time and whatever follows it.
fn split_time(s: &str) -> Option<(NaiveTime, Option<&str>)> {
    let toks: Vec<&str> = s.split_whitespace().collect();
    let (time_str, rest_at) = match toks.as_slice() {
        [t, ap, ..] if matches!(*ap, "am" | "pm") => (format!("{t}{ap}"), 2),
        [t, ..] => (t.to_string(), 1),
        [] => return None,
    };
    let time = parse_time(&time_str)?;
    let rest = toks[rest_at..].join(" ");
    let from = if rest.is_empty() {
        None
    } else {
        // hand back a slice of the input, so the caller borrows from `s`
        let start = s.find(toks[rest_at])?;
        Some(s[start..].trim())
    };
    Some((time, from))
}

fn parse_time(t: &str) -> Option<NaiveTime> {
    let (body, pm) = if let Some(b) = t.strip_suffix("pm") {
        (b, Some(true))
    } else if let Some(b) = t.strip_suffix("am") {
        (b, Some(false))
    } else {
        (t, None)
    };
    let (h, m) = match body.split_once(':') {
        Some((h, m)) => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        // a bare number is a time only with am/pm: "15 to tokyo" is not
        None if pm.is_some() => (body.parse::<u32>().ok()?, 0),
        None => return None,
    };
    let h = match pm {
        Some(true) if (1..=12).contains(&h) => h % 12 + 12,
        Some(false) if (1..=12).contains(&h) => h % 12,
        Some(_) => return None,
        None => h,
    };
    NaiveTime::from_hms_opt(h, m, 0)
}

/// The zone a place name or abbreviation means, or None.
pub fn place_zone(place: &str) -> Option<Tz> {
    let p = place.trim().to_lowercase();
    if p.is_empty() {
        return None;
    }
    if let Some((_, z)) = ALIASES.iter().find(|(names, _)| names.split('|').any(|n| n == p)) {
        return z.parse().ok();
    }
    // any IANA city by its own name: "new york", "los angeles", "tokyo"
    chrono_tz::TZ_VARIANTS.iter().copied().find(|tz| {
        let name = tz.name();
        name.contains('/')
            && !name.starts_with("Etc/")
            && name.rsplit('/').next().is_some_and(|city| city.replace('_', " ").eq_ignore_ascii_case(&p))
    })
}

/// Names people use that are not IANA city names: countries, big cities
/// that share a zone, Chinese names, and the unambiguous abbreviations.
/// "cst" (China or US Central) and "ist" (India, Israel, Ireland) are left
/// out on purpose: a wrong guess is worse than no answer.
const ALIASES: &[(&str, &str)] = &[
    ("utc|gmt|z", "UTC"),
    ("pst|pdt|pt|pacific|la|san francisco|sf|seattle|silicon valley|旧金山|洛杉矶|西雅图", "America/Los_Angeles"),
    ("mst|mdt|mt|mountain", "America/Denver"),
    ("cdt|central", "America/Chicago"),
    ("est|edt|et|eastern|nyc|washington|boston|纽约|华盛顿|波士顿", "America/New_York"),
    ("chicago|芝加哥", "America/Chicago"),
    ("toronto|多伦多", "America/Toronto"),
    ("vancouver|温哥华", "America/Vancouver"),
    ("hawaii|夏威夷", "Pacific/Honolulu"),
    ("bst|uk|britain|england|伦敦|英国", "Europe/London"),
    ("cet|cest|germany|德国|柏林", "Europe/Berlin"),
    ("france|法国|巴黎", "Europe/Paris"),
    ("moscow|msk|莫斯科|俄罗斯", "Europe/Moscow"),
    ("dubai|迪拜", "Asia/Dubai"),
    ("india|delhi|new delhi|mumbai|bangalore|印度|新德里|孟买", "Asia/Kolkata"),
    ("china|beijing|shenzhen|guangzhou|hangzhou|chengdu|中国|北京|上海|深圳|广州|杭州|成都|南京|武汉|西安", "Asia/Shanghai"),
    ("hkt|香港", "Asia/Hong_Kong"),
    ("台北|台湾|taiwan", "Asia/Taipei"),
    ("sgt|新加坡", "Asia/Singapore"),
    ("jst|japan|日本|东京|大阪", "Asia/Tokyo"),
    ("kst|korea|south korea|韩国|首尔", "Asia/Seoul"),
    ("aest|aedt|悉尼|澳大利亚", "Australia/Sydney"),
];

fn offset_label(secs: i32) -> String {
    let (sign, a) = if secs < 0 { ('-', -secs) } else { ('+', secs) };
    let (h, m) = (a / 3600, (a % 3600) / 60);
    if m == 0 {
        format!("UTC{sign}{h}")
    } else {
        format!("UTC{sign}{h}:{m:02}")
    }
}

/// "1 h ahead" / "2.5 h behind" / "same time" relative to local.
fn relation(there: i32, here: i32) -> String {
    let d = there - here;
    if d == 0 {
        return "same time as here".into();
    }
    let hours = d.abs() as f64 / 3600.0;
    let h = if hours.fract() == 0.0 { format!("{}", hours as i64) } else { format!("{hours}") };
    if d > 0 {
        format!("{h} h ahead of here")
    } else {
        format!("{h} h behind here")
    }
}

fn now_in(now: DateTime<Utc>, tz: Tz, local: FixedOffset) -> CalcResult {
    let t = now.with_timezone(&tz);
    let off = t.offset().fix().local_minus_utc();
    CalcResult {
        value: format!("{}  ·  {}", t.format("%H:%M"), t.format("%a %-d %b")),
        alt: Some(format!("{} {}, {}", tz.name(), offset_label(off), relation(off, local.local_minus_utc()))),
    }
}

use chrono::Offset;

fn convert(now: DateTime<Utc>, time: NaiveTime, from: Zone, to: Tz) -> CalcResult {
    // the time is taken on today's date in the source zone, so DST is the
    // one in force today
    let (instant, from_label) = match from {
        Zone::Named(tz) => {
            let day = now.with_timezone(&tz).date_naive();
            let dt = tz
                .from_local_datetime(&day.and_time(time))
                .earliest()
                .unwrap_or_else(|| tz.from_utc_datetime(&day.and_time(time)));
            let abbr = dt.format("%Z").to_string();
            (dt.with_timezone(&Utc), format!("{} {}", time.format("%H:%M"), abbr))
        }
        Zone::Local(off) => {
            let day = now.with_timezone(&off).date_naive();
            let dt = off.from_local_datetime(&day.and_time(time)).single().expect("fixed offsets are unambiguous");
            (dt.with_timezone(&Utc), format!("{} here", time.format("%H:%M")))
        }
    };
    let there = instant.with_timezone(&to);
    let src_day = match from {
        Zone::Named(tz) => instant.with_timezone(&tz).date_naive(),
        Zone::Local(off) => instant.with_timezone(&off).date_naive(),
    };
    let shift = (there.date_naive() - src_day).num_days();
    let day_note = match shift {
        0 => String::new(),
        1 => ", next day".into(),
        -1 => ", previous day".into(),
        n => format!(", {n:+} days"),
    };
    CalcResult {
        value: format!("{}  ·  {}", there.format("%H:%M"), there.format("%a %-d %b")),
        alt: Some(format!("{from_label} → {} {}{day_note}", to.name(), there.format("%Z"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-09-25 06:00 UTC; the machine at UTC+8
    fn at(q: &str) -> Option<CalcResult> {
        let now = Utc.with_ymd_and_hms(2026, 9, 25, 6, 0, 0).unwrap();
        eval_at(q, now, FixedOffset::east_opt(8 * 3600).unwrap())
    }
    fn v(q: &str) -> String {
        at(q).unwrap_or_else(|| panic!("{q:?} must evaluate")).value
    }

    #[test]
    fn the_time_somewhere_now() {
        assert_eq!(v("time in tokyo"), "15:00  ·  Fri 25 Sep");
        assert_eq!(v("tokyo time"), "15:00  ·  Fri 25 Sep");
        assert_eq!(v("Now in New York"), "02:00  ·  Fri 25 Sep", "EDT in September");
        assert_eq!(v("东京时间"), "15:00  ·  Fri 25 Sep");
        assert_eq!(v("纽约现在几点"), "02:00  ·  Fri 25 Sep");
        let alt = at("time in tokyo").unwrap().alt.unwrap();
        assert_eq!(alt, "Asia/Tokyo UTC+9, 1 h ahead of here");
        let india = at("time in india").unwrap().alt.unwrap();
        assert_eq!(india, "Asia/Kolkata UTC+5:30, 2.5 h behind here");
        assert_eq!(at("beijing time").unwrap().alt.unwrap(), "Asia/Shanghai UTC+8, same time as here");
    }

    #[test]
    fn a_time_moved_between_zones() {
        // at the test moment it is still 24 Sep in Los Angeles: 3pm PDT that
        // day = 22:00 UTC = 06:00 on the 25th in Beijing, the next day there
        assert_eq!(v("3pm pst to beijing"), "06:00  ·  Fri 25 Sep");
        assert_eq!(
            at("3pm pst to beijing").unwrap().alt.unwrap(),
            "15:00 PDT → Asia/Shanghai CST, next day"
        );
        assert_eq!(v("15:30 tokyo in london"), "07:30  ·  Fri 25 Sep");
        assert_eq!(v("3 pm la to 纽约"), "18:00  ·  Thu 24 Sep", "Chinese target, same LA day");
        // from local (UTC+8): 9am here = 01:00 UTC = 21:00 previous day in New York
        assert_eq!(v("9am to new york"), "21:00  ·  Thu 24 Sep");
        assert!(at("9am to new york").unwrap().alt.unwrap().ends_with("previous day"));
    }

    #[test]
    fn dst_follows_the_date() {
        // in January New York is on EST, UTC-5
        let jan = Utc.with_ymd_and_hms(2026, 1, 15, 12, 0, 0).unwrap();
        let r = eval_at("time in new york", jan, FixedOffset::east_opt(0).unwrap()).unwrap();
        assert_eq!(r.value, "07:00  ·  Thu 15 Jan");
        assert!(r.alt.unwrap().contains("UTC-5"));
    }

    #[test]
    fn searches_stay_searches() {
        for q in [
            "tokyo", "time", "tokyo trip notes", "time in narnia", "3pm to narnia",
            "15 to tokyo", "25:00 tokyo to london", "13pm to tokyo", "cst time", "ist time",
            "meeting at 3pm", "move to tokyo", "时间", "东京",
        ] {
            assert!(at(q).is_none(), "{q:?} must stay a search");
        }
    }

    #[test]
    fn iana_city_names_resolve_by_their_last_part() {
        assert_eq!(place_zone("Los Angeles").map(|t| t.name()), Some("America/Los_Angeles"));
        assert_eq!(place_zone("sao paulo").map(|t| t.name()), Some("America/Sao_Paulo"));
        assert_eq!(place_zone("berlin").map(|t| t.name()), Some("Europe/Berlin"));
        assert!(place_zone("").is_none());
    }
}
