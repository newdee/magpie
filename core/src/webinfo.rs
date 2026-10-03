//! What the index knows about one web address, for the preview of a
//! bookmark or history row (#13): which site it is, where it is bookmarked
//! and since when, how often and over what span it was visited, and the
//! page's own summary when Firefox kept one. Everything comes from the
//! browsers' local files already mirrored here; nothing is fetched from the
//! web, so a page you forgot is described without contacting it.

use anyhow::Result;
use rusqlite::{params, Connection};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Saved {
    pub browser: String,
    /// the bookmark folder path, empty for the top level
    pub folder: String,
    pub added_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WebInfo {
    /// the site, `www.` dropped (empty for an address that has no host)
    pub host: String,
    /// every bookmark of this address, oldest first
    pub bookmarks: Vec<Saved>,
    /// visits summed over the browsers that have the address in history
    pub visits: i64,
    pub first_visit: Option<i64>,
    pub last_visit: Option<i64>,
    pub description: Option<String>,
}

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
        .map(|h| h.strip_prefix("www.").map(str::to_string).unwrap_or(h))
        .unwrap_or_default()
}

pub fn of(conn: &Connection, url: &str) -> Result<WebInfo> {
    let bookmarks = {
        let mut stmt = conn.prepare(
            "SELECT browser, folder, added_at FROM bookmarks WHERE url = ?1
             ORDER BY added_at IS NULL, added_at, browser, folder",
        )?;
        let rows = stmt
            .query_map([url], |r| Ok(Saved { browser: r.get(0)?, folder: r.get(1)?, added_at: r.get(2)? }))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let (visits, first_visit, last_visit, description): (i64, Option<i64>, Option<i64>, Option<String>) = conn.query_row(
        "SELECT IFNULL(SUM(visit_count), 0), MIN(first_visit), MAX(last_visit),
                (SELECT description FROM history WHERE url = ?1 AND description IS NOT NULL LIMIT 1)
         FROM history WHERE url = ?1",
        params![url],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    Ok(WebInfo { host: host_of(url), bookmarks, visits, first_visit, last_visit, description })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bookmarks_and_history_of_one_address_together() {
        let conn = crate::db::open_in_memory().unwrap();
        let u = "https://www.example.org/guide";
        conn.execute_batch(&format!(
            "INSERT INTO bookmarks(url, title, folder, browser, added_at) VALUES
               ('{u}', 'Guide', 'Reading/Later', 'firefox', 1700000000),
               ('{u}', 'Guide', '', 'chrome', 1600000000),
               ('https://other.example/', 'Other', 'x', 'chrome', 1);
             INSERT INTO history(url, title, browser, visit_count, last_visit, first_visit, description) VALUES
               ('{u}', 'Guide', 'chrome', 3, 1750000000, 1690000000, NULL),
               ('{u}', 'Guide', 'firefox', 4, 1740000000, 1500000000, 'A guide to the example.'),
               ('https://other.example/', 'Other', 'chrome', 99, 1800000000, 1, 'not this one');"
        ))
        .unwrap();
        let w = of(&conn, u).unwrap();
        assert_eq!(w.host, "example.org");
        assert_eq!(
            w.bookmarks,
            vec![
                Saved { browser: "chrome".into(), folder: "".into(), added_at: Some(1600000000) },
                Saved { browser: "firefox".into(), folder: "Reading/Later".into(), added_at: Some(1700000000) },
            ],
            "oldest first"
        );
        assert_eq!((w.visits, w.first_visit, w.last_visit), (7, Some(1500000000), Some(1750000000)));
        assert_eq!(w.description.as_deref(), Some("A guide to the example."));
    }

    #[test]
    fn an_address_the_index_does_not_know() {
        let conn = crate::db::open_in_memory().unwrap();
        let w = of(&conn, "https://nowhere.example/x").unwrap();
        assert_eq!(w, WebInfo { host: "nowhere.example".into(), bookmarks: vec![], visits: 0, first_visit: None, last_visit: None, description: None });
        assert_eq!(of(&conn, "not a url").unwrap().host, "");
    }
}
