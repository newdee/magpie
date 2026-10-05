//! The daily recall card: with nothing typed, one thing you kept long ago and
//! may have forgotten. A search finds only what you remember having; this
//! brings back what you do not. Only what you chose to keep qualifies (a
//! bookmark, a starred repo), never plain history, so the card is something
//! you once cared about rather than a page you passed through.
//!
//! One card a day. On a date you kept something in an earlier year, that
//! comes first ("2 years ago today"); otherwise it is something kept at least
//! half a year ago that you have not opened from magpie. The day's card stays
//! the same all day, opened or not, until the user says not to show it again.

use anyhow::Result;
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Kept at least this long ago to count as possibly forgotten.
pub const IDLE_DAYS: i64 = 180;
const META_KEY: &str = "recall_today";

/// Why this card today. Only what the index knows for sure: when it was
/// kept. magpie cannot tell whether it was opened outside magpie, so the
/// card never claims it was not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "why", rename_all = "snake_case")]
pub enum Why {
    /// kept on this date, this many years ago
    Anniversary { years: i64 },
    /// kept this many months ago
    LongAgo { months: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pick {
    /// "bookmark" (key: the url) or "repo" (key: the repo id)
    pub kind: String,
    pub key: String,
    #[serde(flatten)]
    pub why: Why,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    /// YYYY-MM-DD, local
    day: String,
    pick: Pick,
}

/// Everything of one kind ("bookmark" or "repo") that may come up, with
/// when it was kept (unix seconds).
fn candidates(conn: &Connection, kind: &str) -> Result<Vec<(String, String, i64)>> {
    let skipped: HashSet<(String, String)> = conn
        .prepare("SELECT kind, key FROM recall_skip")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut out = Vec::new();
    // a page bookmarked in two browsers or folders counts once, from the
    // first time it was kept
    let mut stmt = conn.prepare(
        "SELECT url, MIN(added_at) FROM bookmarks WHERE added_at IS NOT NULL GROUP BY url",
    )?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
        let (url, at) = row?;
        out.push(("bookmark".to_string(), url, at));
    }
    let mut stmt = conn.prepare("SELECT id, starred_at FROM repos WHERE starred_at IS NOT NULL")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        let (id, starred) = row?;
        if let Ok(t) = DateTime::parse_from_rfc3339(&starred) {
            out.push(("repo".to_string(), id.to_string(), t.timestamp()));
        }
    }
    out.retain(|(k, key, _)| k == kind && !skipped.contains(&(k.clone(), key.clone())));
    // a stable order, so the day's choice does not depend on row order
    out.sort();
    Ok(out)
}

fn local_day(t: i64, tz: FixedOffset) -> Option<NaiveDate> {
    DateTime::from_timestamp(t, 0).map(|d| d.with_timezone(&tz).date_naive())
}

/// Whole months between two instants (a month is a twelfth of a year).
fn months_between(from: i64, to: i64, tz: FixedOffset) -> i64 {
    let (Some(a), Some(b)) = (local_day(from, tz), local_day(to, tz)) else { return 0 };
    let m = i64::from(b.year() - a.year()) * 12 + i64::from(b.month()) - i64::from(a.month())
        - i64::from(b.day() < a.day());
    m.max(0)
}

/// The same day picks the same index; the next day usually another one far
/// from it, so a run of days does not walk through one corner of the list.
fn index_for(day: NaiveDate, len: usize) -> usize {
    let seed = (day.num_days_from_ce() as u64).wrapping_mul(2_654_435_761) % (1 << 32);
    (seed % len as u64) as usize
}

/// Today's choice of one kind from scratch (no memory of earlier picks).
fn choose(conn: &Connection, kind: &str, today: NaiveDate, now: i64, tz: FixedOffset) -> Result<Option<Pick>> {
    let all = candidates(conn, kind)?;
    let anniversaries: Vec<(&(String, String, i64), i64)> = all
        .iter()
        .filter_map(|c| {
            let d = local_day(c.2, tz)?;
            (d.month() == today.month() && d.day() == today.day() && d.year() < today.year())
                .then(|| (c, i64::from(today.year() - d.year())))
        })
        .collect();
    if !anniversaries.is_empty() {
        let (c, years) = anniversaries[index_for(today, anniversaries.len())];
        return Ok(Some(Pick { kind: c.0.clone(), key: c.1.clone(), why: Why::Anniversary { years } }));
    }
    let opened: HashSet<(String, String)> = conn
        .prepare("SELECT kind, key FROM hit_stats WHERE kind IN ('bookmark', 'repo')")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let idle: Vec<&(String, String, i64)> = all
        .iter()
        .filter(|c| c.2 <= now - IDLE_DAYS * 86_400 && !opened.contains(&(c.0.clone(), c.1.clone())))
        .collect();
    if idle.is_empty() {
        return Ok(None);
    }
    let c = idle[index_for(today, idle.len())];
    Ok(Some(Pick { kind: c.0.clone(), key: c.1.clone(), why: Why::LongAgo { months: months_between(c.2, now, tz) } }))
}

/// Where the day's card of a kind is remembered.
fn meta_key(kind: &str) -> String {
    format!("{META_KEY}:{kind}")
}

/// The day's card of one kind ("bookmark" or "repo"; each tab shows its
/// own, the local tab both, #13): the one already chosen today while it is
/// still there and not skipped, else a new choice, remembered for the rest
/// of the day. None when nothing of that kind qualifies.
pub fn today(conn: &Connection, kind: &str, today: NaiveDate, now: i64, tz: FixedOffset) -> Result<Option<Pick>> {
    let key = meta_key(kind);
    let stored = crate::db::meta_get(conn, &key)?
        .and_then(|s| serde_json::from_str::<Stored>(&s).ok())
        .filter(|s| s.day == today.to_string());
    if let Some(s) = stored {
        let still_there = candidates(conn, kind)?.iter().any(|c| c.0 == s.pick.kind && c.1 == s.pick.key);
        if still_there {
            return Ok(Some(s.pick));
        }
    }
    let pick = choose(conn, kind, today, now, tz)?;
    match &pick {
        Some(p) => crate::db::meta_set(conn, &key, &serde_json::to_string(&Stored { day: today.to_string(), pick: p.clone() })?)?,
        None => {
            conn.execute("DELETE FROM meta WHERE key = ?1", [&key])?;
        }
    }
    Ok(pick)
}

/// Today's local date, the instant, and the local offset; the clock a test
/// sets in `MAGPIE_TEST_NOW` (`2026-10-04T09:30`, local) stands in for now.
pub fn local_clock() -> (NaiveDate, i64, FixedOffset) {
    let tz = *chrono::Local::now().offset();
    let local = crate::chinese_calendar::local_now();
    let now = local.and_local_timezone(tz).single().map_or_else(|| chrono::Utc::now().timestamp(), |t| t.timestamp());
    (local.date(), now, tz)
}

/// "Don't show this again": it never comes up again, and today gets another.
pub fn skip(conn: &Connection, kind: &str, key: &str) -> Result<()> {
    conn.execute("INSERT OR IGNORE INTO recall_skip (kind, key) VALUES (?1, ?2)", params![kind, key])?;
    conn.execute("DELETE FROM meta WHERE key = ?1", [meta_key(kind)])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;

    fn utc() -> FixedOffset {
        FixedOffset::east_opt(0).unwrap()
    }

    fn ts(y: i32, m: u32, d: u32) -> i64 {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(12, 0, 0).unwrap().and_utc().timestamp()
    }

    fn bookmark(conn: &Connection, url: &str, at: i64) {
        conn.execute(
            "INSERT INTO bookmarks (url, title, folder, browser, added_at) VALUES (?1, ?1, '', 'chrome', ?2)",
            params![url, at],
        )
        .unwrap();
    }

    fn repo(conn: &Connection, id: i64, starred: &str) {
        conn.execute(
            "INSERT INTO repos (id, full_name, html_url, starred_at) VALUES (?1, 'o/r', 'https://github.com/o/r', ?2)",
            params![id, starred],
        )
        .unwrap();
    }

    #[test]
    fn nothing_kept_nothing_shown() {
        let conn = crate::db::open_in_memory().unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 10, 4).unwrap();
        assert_eq!(today(&conn, "bookmark", day, ts(2026, 10, 4), utc()).unwrap(), None);
        // kept recently only: not yet forgotten
        bookmark(&conn, "https://new.example", ts(2026, 9, 1));
        assert_eq!(today(&conn, "bookmark", day, ts(2026, 10, 4), utc()).unwrap(), None);
    }

    #[test]
    fn a_date_kept_in_an_earlier_year_comes_first() {
        let conn = crate::db::open_in_memory().unwrap();
        let now = ts(2026, 10, 4);
        let day = NaiveDate::from_ymd_opt(2026, 10, 4).unwrap();
        bookmark(&conn, "https://old.example", ts(2020, 1, 1));
        bookmark(&conn, "https://anniversary.example", ts(2023, 10, 4));
        repo(&conn, 7, "2024-10-04T08:00:00Z");
        repo(&conn, 8, "2020-01-01T08:00:00Z");
        // the same date this year is not an anniversary
        bookmark(&conn, "https://today.example", ts(2026, 10, 4));
        let p = today(&conn, "repo", day, now, utc()).unwrap().unwrap();
        assert_eq!((p.kind.as_str(), p.key.as_str(), &p.why), ("repo", "7", &Why::Anniversary { years: 2 }));
        let p = today(&conn, "bookmark", day, now, utc()).unwrap().unwrap();
        assert_eq!((p.key.as_str(), &p.why), ("https://anniversary.example", &Why::Anniversary { years: 3 }));
    }

    #[test]
    fn each_kind_has_its_own_card() {
        let conn = crate::db::open_in_memory().unwrap();
        let now = ts(2026, 10, 4);
        let day = NaiveDate::from_ymd_opt(2026, 10, 4).unwrap();
        bookmark(&conn, "https://a.example", ts(2024, 1, 1));
        repo(&conn, 3, "2024-02-01T00:00:00Z");
        let b = today(&conn, "bookmark", day, now, utc()).unwrap().unwrap();
        let r = today(&conn, "repo", day, now, utc()).unwrap().unwrap();
        assert_eq!((b.kind.as_str(), r.kind.as_str()), ("bookmark", "repo"), "a tab never gets the other kind (#13)");
        // retiring the repo leaves the bookmark's card alone
        skip(&conn, "repo", "3").unwrap();
        assert_eq!(today(&conn, "repo", day, now, utc()).unwrap(), None);
        assert_eq!(today(&conn, "bookmark", day, now, utc()).unwrap(), Some(b));
        assert_eq!(today(&conn, "history", day, now, utc()).unwrap(), None, "no other kinds");
    }

    #[test]
    fn the_local_date_decides_the_anniversary() {
        let conn = crate::db::open_in_memory().unwrap();
        // 2024-10-03 20:00 UTC is already the 4th in Beijing
        repo(&conn, 1, "2024-10-03T20:00:00Z");
        let day = NaiveDate::from_ymd_opt(2026, 10, 4).unwrap();
        let bj = FixedOffset::east_opt(8 * 3600).unwrap();
        let p = choose(&conn, "repo", day, ts(2026, 10, 4), bj).unwrap().unwrap();
        assert_eq!(p.why, Why::Anniversary { years: 2 });
        // in UTC it was the 3rd, so not today's; and it is old enough to come up anyway
        let p = choose(&conn, "repo", day, ts(2026, 10, 4), utc()).unwrap().unwrap();
        assert_eq!(p.why, Why::LongAgo { months: 24 });
    }

    #[test]
    fn otherwise_something_kept_long_ago_and_not_opened_here() {
        let conn = crate::db::open_in_memory().unwrap();
        let now = ts(2026, 10, 4);
        let day = NaiveDate::from_ymd_opt(2026, 10, 4).unwrap();
        bookmark(&conn, "https://a.example", ts(2026, 3, 1)); // 7 months
        bookmark(&conn, "https://recent.example", now - 30 * DAY);
        repo(&conn, 3, "2025-12-01T00:00:00Z"); // 10 months, but opened
        crate::frecency::record_use(&conn, "repo", "3", now - DAY).unwrap();
        let p = today(&conn, "bookmark", day, now, utc()).unwrap().unwrap();
        assert_eq!((p.kind.as_str(), p.key.as_str(), &p.why), ("bookmark", "https://a.example", &Why::LongAgo { months: 7 }));
        assert_eq!(today(&conn, "repo", day, now, utc()).unwrap(), None, "the only repo was opened here");
    }

    #[test]
    fn one_card_all_day_even_after_it_is_opened() {
        let conn = crate::db::open_in_memory().unwrap();
        let now = ts(2026, 10, 4);
        let day = NaiveDate::from_ymd_opt(2026, 10, 4).unwrap();
        for i in 0..50 {
            bookmark(&conn, &format!("https://b{i:02}.example"), ts(2024, 1, 1) + i * DAY);
        }
        let first = today(&conn, "bookmark", day, now, utc()).unwrap().unwrap();
        crate::frecency::record_use(&conn, "bookmark", &first.key, now).unwrap();
        assert_eq!(today(&conn, "bookmark", day, now + 3600, utc()).unwrap().unwrap(), first, "opened, still today's card");
        // the next day brings another
        let next = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        let second = today(&conn, "bookmark", next, now + DAY, utc()).unwrap().unwrap();
        assert_ne!(second.key, first.key);
        // a deleted bookmark is replaced the same day
        conn.execute("DELETE FROM bookmarks WHERE url = ?1", [&second.key]).unwrap();
        let third = today(&conn, "bookmark", next, now + DAY, utc()).unwrap().unwrap();
        assert_ne!(third.key, second.key);
    }

    #[test]
    fn skipped_never_comes_back() {
        let conn = crate::db::open_in_memory().unwrap();
        let now = ts(2026, 10, 4);
        let day = NaiveDate::from_ymd_opt(2026, 10, 4).unwrap();
        bookmark(&conn, "https://x.example", ts(2023, 1, 1));
        bookmark(&conn, "https://y.example", ts(2023, 2, 1));
        let a = today(&conn, "bookmark", day, now, utc()).unwrap().unwrap();
        skip(&conn, &a.kind, &a.key).unwrap();
        let b = today(&conn, "bookmark", day, now, utc()).unwrap().unwrap();
        assert_ne!(a.key, b.key, "today gets another");
        skip(&conn, &b.kind, &b.key).unwrap();
        skip(&conn, &b.kind, &b.key).unwrap(); // twice is harmless
        assert_eq!(today(&conn, "bookmark", day, now, utc()).unwrap(), None, "all skipped");
        // on later days too
        for d in 5..20 {
            let day = NaiveDate::from_ymd_opt(2026, 10, d).unwrap();
            assert_eq!(today(&conn, "bookmark", day, now + i64::from(d) * DAY, utc()).unwrap(), None);
        }
    }

    #[test]
    fn days_spread_over_the_whole_list() {
        let conn = crate::db::open_in_memory().unwrap();
        // kept June to September; the days below are January and February,
        // so no anniversary decides for them
        for i in 0..100 {
            bookmark(&conn, &format!("https://c{i:03}.example"), ts(2023, 6, 1) + i * DAY);
        }
        let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let seen: HashSet<String> = (0..60)
            .map(|d| {
                let day = start + chrono::Days::new(d);
                choose(&conn, "bookmark", day, ts(2026, 6, 1), utc()).unwrap().unwrap().key
            })
            .collect();
        assert!(seen.len() >= 35, "60 days showed only {} different cards", seen.len());
        // same day, same answer
        let day = NaiveDate::from_ymd_opt(2026, 3, 3).unwrap();
        assert_eq!(
            choose(&conn, "bookmark", day, ts(2026, 6, 1), utc()).unwrap(),
            choose(&conn, "bookmark", day, ts(2026, 6, 1), utc()).unwrap()
        );
    }

    #[test]
    fn a_page_kept_twice_counts_from_the_first_time_and_bad_dates_are_ignored() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO bookmarks (url, title, folder, browser, added_at) VALUES
               ('https://dup.example', 'a', 'x', 'chrome', 1700000000),
               ('https://dup.example', 'a', 'y', 'firefox', 1500000000);",
        )
        .unwrap();
        repo(&conn, 9, "not a date");
        assert_eq!(candidates(&conn, "bookmark").unwrap(), vec![("bookmark".to_string(), "https://dup.example".to_string(), 1500000000)]);
        assert!(candidates(&conn, "repo").unwrap().is_empty());
    }

    #[test]
    fn months_are_whole_months() {
        assert_eq!(months_between(ts(2026, 3, 1), ts(2026, 10, 4), utc()), 7);
        assert_eq!(months_between(ts(2026, 3, 5), ts(2026, 10, 4), utc()), 6, "not yet the 5th");
        assert_eq!(months_between(ts(2026, 10, 4), ts(2026, 3, 1), utc()), 0, "never negative");
        assert_eq!(months_between(ts(2016, 10, 4), ts(2026, 10, 4), utc()), 120);
    }
}