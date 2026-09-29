//! 农历 in the box, computed on this machine (tyme4rs, after the 寿星天文历
//! algorithms): no network, no table to refresh.
//!
//! - `农历` / `阴历` / `nl` / `lunar`: today in the Chinese calendar, with the
//!   zodiac year, festivals and the solar term.
//! - `农历 2026-10-1`, `农历 10月1日`, `农历 明天`: the same for that day.
//! - `农历 八月十五`, `农历 腊月廿三`, `农历 2027年正月初一`, `农历 中秋`: when
//!   that is on the Gregorian calendar (the next one, unless a year is given).
//! - `节气`: the next solar term; `节气 清明` (or `农历 清明`): the next 清明.
//!
//! Arabic numerals are Gregorian dates, Chinese numerals lunar ones, so
//! `8月15日` and `八月十五` never mean the same day by accident. Statutory
//! holidays and swapped workdays are left out: they are announced each year
//! and cannot be computed.

use chrono::{Datelike, NaiveDate};
use tyme4rs::tyme::lunar::{LunarDay, LunarMonth};
use tyme4rs::tyme::solar::{SolarDay, SolarTerm};
use tyme4rs::tyme::{Culture, Tyme};

/// Years answered. The algorithms go further; dates this far out are
/// enough for a launcher and keep every answer checkable.
const YEARS: std::ops::RangeInclusive<i32> = 1900..=2100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub value: String,
    pub label: String,
    /// the input names a day that does not exist (腊月三十 in a short year)
    pub error: bool,
}

/// Festivals of the lunar calendar: names people type, and (month, day);
/// day 0 is the last day of the month (除夕 falls on 廿九 or 三十).
const FESTIVALS: &[(&str, &[&str], i32, usize)] = &[
    ("春节", &["春节", "过年", "大年初一", "新年"], 1, 1),
    ("元宵节", &["元宵", "元宵节", "上元节"], 1, 15),
    ("龙抬头", &["龙抬头", "二月二"], 2, 2),
    ("端午节", &["端午", "端午节"], 5, 5),
    ("七夕", &["七夕", "七夕节"], 7, 7),
    ("中元节", &["中元", "中元节", "鬼节"], 7, 15),
    ("中秋节", &["中秋", "中秋节"], 8, 15),
    ("重阳节", &["重阳", "重阳节"], 9, 9),
    ("腊八节", &["腊八", "腊八节"], 12, 8),
    ("小年（北方，南方多为腊月廿四）", &["小年"], 12, 23),
    ("除夕", &["除夕", "大年三十", "年三十"], 12, 0),
];

pub fn answer(query: &str, today: NaiveDate) -> Option<Answer> {
    let q = query.trim();
    if let Some(rest) = strip_verb(q, &["节气"], &[]) {
        return terms(rest, today);
    }
    let rest = strip_verb(q, &["农历", "阴历"], &["nl", "lunar"])?;
    if rest.is_empty() {
        return Some(solar_to_lunar(today, today));
    }
    if let Some(day) = parse_solar(rest, today) {
        return Some(solar_to_lunar(day, today));
    }
    if let Some(a) = festival(rest, today) {
        return Some(a);
    }
    if term_names(today).iter().any(|n| n == rest) {
        return terms(rest, today);
    }
    let (year, month, day) = parse_lunar(rest)?;
    Some(lunar_to_solar(year, month, day, None, today))
}

/// The text after a verb: Chinese verbs may run straight into it (`农历八月十五`),
/// Latin ones need a space (`nl 10-1`, not `nlp`).
fn strip_verb<'a>(q: &'a str, joined: &[&str], spaced: &[&str]) -> Option<&'a str> {
    for v in joined {
        if let Some(rest) = q.strip_prefix(v) {
            return Some(rest.trim());
        }
    }
    let lower = q.to_lowercase();
    for v in spaced {
        if lower == *v {
            return Some("");
        }
        if lower.starts_with(v) && q[v.len()..].starts_with(char::is_whitespace) {
            return Some(q[v.len()..].trim());
        }
    }
    None
}

fn solar(d: NaiveDate) -> Option<SolarDay> {
    YEARS.contains(&d.year()).then_some(())?;
    SolarDay::new(d.year() as isize, d.month() as usize, d.day() as usize).ok()
}

fn naive(s: &SolarDay) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(s.get_year() as i32, s.get_month() as u32, s.get_day() as u32)
}

/// "2026年9月25日 周五"
fn show(d: NaiveDate) -> String {
    const WEEK: [&str; 7] = ["一", "二", "三", "四", "五", "六", "日"];
    format!("{}年{}月{}日 周{}", d.year(), d.month(), d.day(), WEEK[d.weekday().num_days_from_monday() as usize])
}

fn countdown(d: NaiveDate, today: NaiveDate) -> String {
    match (d - today).num_days() {
        0 => "就是今天".into(),
        n if n > 0 => format!("还有 {n} 天"),
        n => format!("{} 天前", -n),
    }
}

/// `2026-10-1`, `2026/10/1`, `2026.10.1`, `10-1`, `2026年10月1日`, `10月1号`,
/// `今天` / `明天` / `后天` / `昨天` / `前天`.
fn parse_solar(s: &str, today: NaiveDate) -> Option<NaiveDate> {
    let shift = match s {
        "今天" => Some(0),
        "明天" => Some(1),
        "后天" => Some(2),
        "昨天" => Some(-1),
        "前天" => Some(-2),
        _ => None,
    };
    if let Some(n) = shift {
        return today.checked_add_signed(chrono::Duration::days(n));
    }
    let t = s.trim_end_matches(['日', '号']).replace(['年', '月', '/', '.'], "-");
    let parts: Vec<&str> = t.split('-').collect();
    if parts.iter().any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) || p.len() > 4) {
        return None;
    }
    let n: Vec<u32> = parts.iter().map(|p| p.parse().ok()).collect::<Option<_>>()?;
    let (y, m, d) = match n[..] {
        [m, d] => (today.year(), m, d),
        [y, m, d] if parts[0].len() == 4 => (y as i32, m, d),
        _ => return None,
    };
    // a year out of range still parses: the row then says which years work
    NaiveDate::from_ymd_opt(y, m, d)
}

/// `八月十五`, `闰六月初一`, `腊月廿三`, `2027年正月初一`, `2027 正月初一`:
/// (year if given, month, negative for a leap month as tyme4rs has it, day).
fn parse_lunar(s: &str) -> Option<(Option<i32>, i32, usize)> {
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    let (year, s) = if digits == 4 {
        let y: i32 = s[..4].parse().ok()?;
        (Some(y), s[4..].trim_start_matches('年').trim())
    } else if digits == 0 {
        (None, s)
    } else {
        return None;
    };
    let (leap, s) = match s.strip_prefix('闰') {
        Some(r) => (true, r),
        None => (false, s),
    };
    const MONTHS: &[(&str, i32)] = &[
        ("十一", 11), ("十二", 12), ("正", 1), ("一", 1), ("二", 2), ("三", 3), ("四", 4), ("五", 5),
        ("六", 6), ("七", 7), ("八", 8), ("九", 9), ("十", 10), ("冬", 11), ("腊", 12),
    ];
    let (m, s) = MONTHS.iter().find_map(|(name, m)| s.strip_prefix(name).and_then(|r| r.strip_prefix('月')).map(|r| (*m, r)))?;
    let d = lunar_day_number(s.trim_end_matches('日'))?;
    Some((year, if leap { -m } else { m }, d))
}

/// 初一 … 初十, 十一 … 十九, 二十, 廿一 … 廿九 (or 二十一), 三十.
fn lunar_day_number(s: &str) -> Option<usize> {
    const DIGITS: [&str; 10] = ["一", "二", "三", "四", "五", "六", "七", "八", "九", "十"];
    let digit = |t: &str| DIGITS.iter().position(|d| *d == t).map(|i| i + 1);
    let ones = |r: &str| digit(r).filter(|n| *n < 10);
    let d = match s {
        "十" => 10,
        "二十" => 20,
        "三十" => 30,
        _ => match (s.strip_prefix('初'), s.strip_prefix("二十").or_else(|| s.strip_prefix('廿')), s.strip_prefix('十')) {
            (Some(r), _, _) => digit(r)?,
            (_, Some(r), _) => 20 + ones(r)?,
            (_, _, Some(r)) => 10 + ones(r)?,
            _ => return None,
        },
    };
    (1..=30).contains(&d).then_some(d)
}

/// Today (or another day) in the Chinese calendar.
fn solar_to_lunar(day: NaiveDate, today: NaiveDate) -> Answer {
    let Some(s) = solar(day) else {
        return out_of_range();
    };
    let l = s.get_lunar_day();
    let year = l.get_lunar_month().get_lunar_year();
    let zodiac = year.get_sixty_cycle().get_earth_branch().get_zodiac().get_name();
    let mut label = Vec::new();
    if day != today {
        label.push(show(day));
    }
    label.push(format!("{zodiac}年"));
    for name in [l.get_festival().map(|f| f.get_name()), s.get_festival().map(|f| f.get_name())].into_iter().flatten() {
        label.push(name);
    }
    let td = s.get_term_day();
    let term = td.get_solar_term();
    label.push(if td.get_day_index() == 0 { format!("今天{}", term.get_name()) } else { format!("{}第 {} 天", term.get_name(), td.get_day_index() + 1) });
    let next = term.next(1);
    if let Some(nd) = naive(&next.get_solar_day()) {
        let when = format!("下个节气{} {}月{}日", next.get_name(), nd.month(), nd.day());
        label.push(if day == today { format!("{when}（{}）", countdown(nd, today)) } else { when });
    }
    Answer { value: l.to_string(), label: label.join(" · "), error: false }
}

/// When a lunar date falls: in `year` if given, else the next time from today.
fn lunar_to_solar(year: Option<i32>, month: i32, day: usize, festival: Option<&str>, today: NaiveDate) -> Answer {
    let this_year = match solar(today) {
        Some(s) => s.get_lunar_day().get_lunar_month().get_lunar_year().get_year() as i32,
        None => return out_of_range(),
    };
    let years: Vec<i32> = match year {
        Some(y) => vec![y],
        // a leap month comes back every few years, some (闰正月) far apart
        None if month < 0 => (this_year..this_year + 60).collect(),
        None => (this_year..this_year + 3).collect(),
    };
    for y in years {
        if !YEARS.contains(&y) {
            return out_of_range();
        }
        let Ok(m) = LunarMonth::new(y as isize, month as isize) else { continue };
        let d = if day == 0 { m.get_day_count() } else { day };
        let Ok(l) = LunarDay::new(y as isize, month as isize, d) else { continue };
        let Some(date) = naive(&l.get_solar_day()) else { continue };
        if year.is_none() && date < today {
            continue;
        }
        let mut label = Vec::new();
        if let Some(f) = festival {
            label.push(f.to_string());
        }
        label.push(l.to_string());
        label.push(countdown(date, today));
        return Answer { value: show(date), label: label.join(" · "), error: false };
    }
    let what = format!("{}{}月{}", if month < 0 { "闰" } else { "" }, month_name(month.abs()), day_name(day));
    Answer {
        value: match year {
            Some(y) => format!("农历 {y} 年没有{what}"),
            None => format!("近几年农历没有{what}"),
        },
        label: "农历".into(),
        error: true,
    }
}

fn month_name(m: i32) -> &'static str {
    ["正", "二", "三", "四", "五", "六", "七", "八", "九", "十", "冬", "腊"][(m.clamp(1, 12) - 1) as usize]
}

fn day_name(d: usize) -> String {
    const N: [&str; 10] = ["一", "二", "三", "四", "五", "六", "七", "八", "九", "十"];
    match d {
        1..=10 => format!("初{}", N[d - 1]),
        11..=19 => format!("十{}", N[d - 11]),
        20 => "二十".into(),
        21..=29 => format!("廿{}", N[d - 21]),
        30 => "三十".into(),
        _ => d.to_string(),
    }
}

fn festival(s: &str, today: NaiveDate) -> Option<Answer> {
    let (name, _, m, d) = FESTIVALS.iter().find(|(_, names, _, _)| names.contains(&s))?;
    Some(lunar_to_solar(None, *m, *d, Some(name), today))
}

fn term_names(today: NaiveDate) -> Vec<String> {
    (0..24).map(|i| SolarTerm::from_index(today.year() as isize, i).get_name()).collect()
}

/// `节气`: the next one; `节气 清明`: the next 清明 (today counts).
fn terms(rest: &str, today: NaiveDate) -> Option<Answer> {
    let Some(s) = solar(today) else {
        return Some(out_of_range());
    };
    let td = s.get_term_day();
    let now = td.get_solar_term();
    if rest.is_empty() {
        let next = now.next(1);
        let date = naive(&next.get_solar_day())?;
        let current = if td.get_day_index() == 0 { format!("今天{}", now.get_name()) } else { format!("现在是{}第 {} 天", now.get_name(), td.get_day_index() + 1) };
        return Some(Answer {
            value: format!("{} {}", next.get_name(), show(date)),
            label: format!("下个节气 · {} · {current}", countdown(date, today)),
            error: false,
        });
    }
    if !term_names(today).iter().any(|n| n == rest) {
        return None;
    }
    (0..=24).map(|k| now.next(k)).find_map(|t| {
        let date = naive(&t.get_solar_day())?;
        (t.get_name() == rest && date >= today).then(|| Answer {
            value: format!("{rest} {}", show(date)),
            label: format!("节气 · {}", countdown(date, today)),
            error: false,
        })
    })
}

fn out_of_range() -> Answer {
    Answer { value: format!("农历只算 {} 到 {} 年", YEARS.start(), YEARS.end()), label: "农历".into(), error: true }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// Dates checked against the published calendar (春节, 中秋, the leap
    /// months of 2023, 2025 and the much-discussed 2033, solar terms).
    #[test]
    fn known_dates_come_out_right() {
        let t = day(2026, 9, 29);
        let a = answer("农历", t).unwrap();
        assert_eq!(a.value, "农历丙午年八月十九");
        assert!(a.label.starts_with("马年 · 秋分第 7 天 · 下个节气寒露 10月8日（还有 9 天）"), "{}", a.label);
        for (q, v) in [
            ("农历 2026-2-17", "农历丙午年正月初一"),
            ("农历 2025/7/25", "农历乙巳年闰六月初一"),
            ("农历 2023年3月22日", "农历癸卯年闰二月初一"),
            ("农历2033-12-22", "农历癸丑年闰十一月初一"),
            ("农历 1900-01-31", "农历庚子年正月初一"),
            ("nl 10-1", "农历丙午年八月廿一"),
            ("lunar 明天", "农历丙午年八月二十"),
        ] {
            assert_eq!(answer(q, t).map(|a| a.value), Some(v.to_string()), "{q}");
        }
        let spring = answer("农历 2026-2-17", t).unwrap();
        assert!(spring.label.contains("春节") && spring.label.contains("2026年2月17日 周二"), "{}", spring.label);
    }

    #[test]
    fn lunar_dates_and_festivals_find_their_day() {
        let t = day(2026, 9, 29);
        // 中秋 2026 was on 9-25: the next one is in 2027
        let a = answer("农历 中秋", t).unwrap();
        assert_eq!(a.value, "2027年9月15日 周三");
        assert!(a.label.starts_with("中秋节 · 农历丁未年八月十五 · 还有 351 天"), "{}", a.label);
        assert_eq!(answer("农历八月十九", t).unwrap().label.split(" · ").last(), Some("就是今天"));
        assert_eq!(answer("农历 2027年正月初一", t).unwrap().value, "2027年2月6日 周六");
        assert_eq!(answer("农历 2026 正月初一", t).unwrap().label.split(" · ").last(), Some("224 天前"));
        assert_eq!(answer("农历 腊月廿三", t).unwrap().value, "2027年1月30日 周六");
        assert_eq!(answer("农历 二十三", t), None, "no month: not a lunar date");
        // 除夕: the last day of 腊月, 廿九 in some years
        assert_eq!(answer("农历 除夕", t).unwrap().value, "2027年2月5日 周五");
        assert_eq!(answer("农历 除夕", day(2025, 3, 1)).unwrap().value, "2026年2月16日 周一");
        // a leap month: the next year that has one (2036, per .NET too)
        assert_eq!(answer("农历 闰六月初一", t).unwrap().value, "2036年7月23日 周三");
    }

    #[test]
    fn every_day_name_reads_back() {
        for d in 1..=30 {
            assert_eq!(lunar_day_number(&day_name(d)), Some(d), "{}", day_name(d));
        }
        assert_eq!(lunar_day_number("二十一"), Some(21));
        for bad in ["", "初", "初十一", "十十", "廿十", "三十一", "四十", "卅", "15"] {
            assert_eq!(lunar_day_number(bad), None, "{bad:?}");
        }
        for m in 1..=12 {
            let (_, got, _) = parse_lunar(&format!("{}月初一", month_name(m))).unwrap();
            assert_eq!(got, m, "{}", month_name(m));
        }
    }

    #[test]
    fn a_day_that_does_not_exist_says_so() {
        let t = day(2026, 9, 29);
        let a = answer("农历 2026年腊月三十", t).unwrap();
        assert!(a.error && a.value.contains("没有腊月三十"), "{a:?}");
        assert!(answer("农历 2300-1-1", t).unwrap().error);
    }

    #[test]
    fn solar_terms() {
        let t = day(2026, 9, 29);
        let a = answer("节气", t).unwrap();
        assert_eq!(a.value, "寒露 2026年10月8日 周四");
        assert_eq!(a.label, "下个节气 · 还有 9 天 · 现在是秋分第 7 天");
        assert_eq!(answer("节气 清明", t).unwrap().value, "清明 2027年4月5日 周一");
        assert_eq!(answer("农历 冬至", t).unwrap().value, "冬至 2026年12月22日 周二");
        assert_eq!(answer("节气 秋分", day(2026, 9, 23)).unwrap().label, "节气 · 就是今天");
    }

    /// Anything else stays a search.
    #[test]
    fn other_input_is_not_taken() {
        let t = day(2026, 9, 29);
        for q in ["nlp", "lunar lander", "农历新年.pdf", "节气表", "农历 8月15", "农历 abc", "月饼", "", "nl 2026-2-30", "农历 13-1"] {
            let a = answer(q, t);
            // "农历 8月15" is a Gregorian date: it answers, as a Gregorian one
            if q == "农历 8月15" {
                assert_eq!(a.map(|a| a.value), Some("农历丙午年七月初三".to_string()));
                continue;
            }
            assert_eq!(a, None, "{q:?}");
        }
    }
}
