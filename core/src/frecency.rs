//! Frecency: results the user actually picks float up over time. Purely
//! local statistics (`hit_stats`), applied as a small score bonus that can
//! break ties and lift habitual picks — but is capped per source so it can
//! never override a strictly better match tier (an exact app-name match
//! stays above a boosted prefix match).

use anyhow::Result;
use rusqlite::{params, Connection};
use std::collections::HashMap;

/// Record that the user opened a hit. `key` is the stable identity for the
/// kind: file/video → path, app → target, bookmark/history → url, repo → id.
pub fn record_use(conn: &Connection, kind: &str, key: &str, now: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO hit_stats (kind, key, uses, last_used) VALUES (?1, ?2, 1, ?3)
         ON CONFLICT(kind, key) DO UPDATE SET uses = uses + 1, last_used = excluded.last_used",
        params![kind, key, now],
    )?;
    Ok(())
}

/// Normalized frecency factor in [0, 1]: saturating with use count (ln,
/// full at ~10 uses), decaying with a 30-day half-life-ish curve.
fn factor(uses: i64, last_used: i64, now: i64) -> f32 {
    let freq = ((1.0 + uses as f32).ln() / (11.0f32).ln()).min(1.0);
    let age_days = ((now - last_used).max(0) as f32) / 86_400.0;
    let recency = (-age_days / 30.0).exp();
    freq * recency
}

/// Factors for one kind, keyed by identity. One query per search call —
/// hit_stats stays tiny (only things the user actually opened).
pub fn factors(conn: &Connection, kind: &str, now: i64) -> Result<HashMap<String, f32>> {
    let mut stmt =
        conn.prepare("SELECT key, uses, last_used FROM hit_stats WHERE kind = ?1")?;
    let rows = stmt.query_map([kind], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))
    })?;
    let mut out = HashMap::new();
    for row in rows {
        let (key, uses, last) = row?;
        out.insert(key, factor(uses, last, now));
    }
    Ok(out)
}

/// The most recently opened identities among `kinds`, newest first. Feeds
/// the empty-query "recent opens" list; the caller resolves each identity
/// back into a full hit and drops the ones that no longer exist.
pub fn recent(conn: &Connection, kinds: &[&str], limit: usize) -> Result<Vec<(String, String)>> {
    if kinds.is_empty() {
        return Ok(Vec::new());
    }
    let marks = kinds.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT kind, key FROM hit_stats WHERE kind IN ({marks}) ORDER BY last_used DESC LIMIT {limit}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(kinds.iter()), |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

/// Apply a capped bonus and re-sort. `cap` is chosen per source relative to
/// its score scale (apps ≈ 0.08 against 0.1 tier gaps; RRF lists ≈ 0.01).
pub fn boost<T>(
    hits: &mut [T],
    factors: &HashMap<String, f32>,
    cap: f32,
    key_of: impl Fn(&T) -> String,
    score_of: impl Fn(&T) -> f32,
    set_score: impl Fn(&mut T, f32),
) {
    for h in hits.iter_mut() {
        if let Some(f) = factors.get(&key_of(h)) {
            let s = score_of(h) + cap * f;
            set_score(h, s);
        }
    }
    hits.sort_by(|a, b| score_of(b).total_cmp(&score_of(a)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_lists_newest_first_within_the_asked_kinds() {
        let conn = crate::db::open_in_memory().unwrap();
        record_use(&conn, "file", "C:/a.md", 100).unwrap();
        record_use(&conn, "app", "/apps/code", 200).unwrap();
        record_use(&conn, "repo", "42", 300).unwrap();
        record_use(&conn, "file", "C:/b.md", 400).unwrap();
        // re-opening a.md moves it to the front
        record_use(&conn, "file", "C:/a.md", 500).unwrap();
        let r = recent(&conn, &["file", "app"], 10).unwrap();
        let keys: Vec<&str> = r.iter().map(|(_, k)| k.as_str()).collect();
        assert_eq!(keys, vec!["C:/a.md", "C:/b.md", "/apps/code"], "newest first, repo excluded");
        assert_eq!(recent(&conn, &["file"], 1).unwrap().len(), 1, "limit applies");
        assert!(recent(&conn, &[], 10).unwrap().is_empty(), "no kinds, no rows");
        assert!(recent(&conn, &["video"], 10).unwrap().is_empty(), "unknown kind, no rows");
    }

    #[test]
    fn record_and_factor_shape() {
        let conn = crate::db::open_in_memory().unwrap();
        for _ in 0..5 {
            record_use(&conn, "app", "/x/wechat", 1_000_000).unwrap();
        }
        record_use(&conn, "app", "/x/rare", 1_000_000).unwrap();
        let f = factors(&conn, "app", 1_000_000).unwrap();
        assert!(f["/x/wechat"] > f["/x/rare"], "more uses → bigger factor");
        // recency decay: same stats read 90 days later shrink hard
        let f_old = factors(&conn, "app", 1_000_000 + 90 * 86_400).unwrap();
        assert!(f_old["/x/wechat"] < f["/x/wechat"] * 0.2);
        // unknown kind is empty
        assert!(factors(&conn, "file", 1_000_000).unwrap().is_empty());
    }

    #[test]
    fn boost_lifts_habit_but_not_over_a_tier() {
        #[derive(Debug)]
        struct H {
            key: &'static str,
            score: f32,
        }
        let conn = crate::db::open_in_memory().unwrap();
        for _ in 0..20 {
            record_use(&conn, "app", "b", 1_000_000).unwrap();
        }
        let f = factors(&conn, "app", 1_000_000).unwrap();
        // same tier (two prefix matches at 0.9-ish): habit wins
        let mut hits = vec![H { key: "a", score: 0.90 }, H { key: "b", score: 0.895 }];
        boost(&mut hits, &f, 0.08, |h| h.key.to_string(), |h| h.score, |h, s| h.score = s);
        assert_eq!(hits[0].key, "b");
        // across tiers (exact 1.0 vs boosted prefix): exact still wins
        let mut hits = vec![H { key: "a", score: 1.0 }, H { key: "b", score: 0.9 }];
        boost(&mut hits, &f, 0.08, |h| h.key.to_string(), |h| h.score, |h, s| h.score = s);
        assert_eq!(hits[0].key, "a");
    }
}
