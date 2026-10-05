//! Workspaces: a named set of things opened together (the files, pages and
//! apps of one job, say "报销" or "project A"). Typing the name and pressing
//! Enter opens them all. Kept in meta as JSON, like the editing-apps table,
//! so a settings export carries them.

use anyhow::{bail, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

pub const META_KEY: &str = "workspaces";
/// Longest name kept, in characters.
pub const NAME_MAX: usize = 60;

/// One thing in a workspace. `kind` says how it opens: "file" (a path, in
/// its default app), "app" (an app target), "url" (in the browser).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub kind: String,
    pub target: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub name: String,
    pub items: Vec<Item>,
    /// last change, unix seconds: the most recent comes first
    pub updated: i64,
}

/// All workspaces, the most recently changed first. A missing or damaged
/// record reads as none.
pub fn list(conn: &Connection) -> Result<Vec<Workspace>> {
    let mut all: Vec<Workspace> = crate::db::meta_get(conn, META_KEY)?
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    all.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.name.cmp(&b.name)));
    Ok(all)
}

fn store(conn: &Connection, all: &[Workspace]) -> Result<()> {
    crate::db::meta_set(conn, META_KEY, &serde_json::to_string(all)?)
}

/// A name as kept: trimmed, inner whitespace collapsed, at most NAME_MAX
/// characters. Empty means no name.
pub fn clean_name(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(NAME_MAX).collect()
}

fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// Add `item` to the workspace `name`, creating it if there is none (names
/// compare without case). An item already in it stays where it is.
pub fn add(conn: &Connection, name: &str, item: Item, now: i64) -> Result<Workspace> {
    let name = clean_name(name);
    if name.is_empty() {
        bail!("a workspace needs a name");
    }
    if !matches!(item.kind.as_str(), "file" | "app" | "url") || item.target.is_empty() {
        bail!("not something a workspace can open");
    }
    let mut all = list(conn)?;
    let ws = match all.iter_mut().find(|w| same_name(&w.name, &name)) {
        Some(w) => w,
        None => {
            all.push(Workspace { name: name.clone(), items: Vec::new(), updated: now });
            all.last_mut().unwrap()
        }
    };
    if !ws.items.iter().any(|i| i.kind == item.kind && i.target == item.target) {
        ws.items.push(item);
    }
    ws.updated = now;
    let out = ws.clone();
    store(conn, &all)?;
    Ok(out)
}

/// Take one thing out; the workspace goes when nothing is left in it.
pub fn remove_item(conn: &Connection, name: &str, kind: &str, target: &str, now: i64) -> Result<()> {
    let mut all = list(conn)?;
    if let Some(w) = all.iter_mut().find(|w| same_name(&w.name, name)) {
        w.items.retain(|i| !(i.kind == kind && i.target == target));
        w.updated = now;
    }
    all.retain(|w| !w.items.is_empty());
    store(conn, &all)
}

pub fn delete(conn: &Connection, name: &str) -> Result<()> {
    let mut all = list(conn)?;
    all.retain(|w| !same_name(&w.name, name));
    store(conn, &all)
}

/// Words that list every workspace when typed alone (#19: `gzxc`, the
/// initials of 工作现场, so a forgotten one shows up without its name).
const LIST_ALL: &[&str] = &["ws", "workspace", "workspaces", "现场", "工作现场", "gzxc", "gongzuoxianchang"];

/// Workspaces whose name matches the query, best first, scored and matched
/// as app names are (prefix, word start, initials, pinyin: `bx` finds 报销).
/// One of the LIST_ALL words alone lists them all, most recent first.
pub fn search(conn: &Connection, query: &str) -> Result<Vec<(Workspace, f32)>> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let all = list(conn)?;
    if LIST_ALL.contains(&q.as_str()) {
        return Ok(all.into_iter().map(|w| (w, 0.95)).collect());
    }
    let entries: Vec<crate::apps::AppEntry> = all
        .iter()
        .map(|w| crate::apps::AppEntry { name: w.name.clone(), target: w.name.to_lowercase(), ..Default::default() })
        .collect();
    let ascii = query.trim().is_ascii();
    Ok(crate::apps::match_apps(&entries, query, entries.len(), true)
        .into_iter()
        // the floor the system commands and scripts use: no stray letters
        .filter(|e| e.score >= 0.65 || (e.score >= 0.5 && !ascii))
        .filter_map(|e| all.iter().find(|w| w.name.to_lowercase() == e.target).map(|w| (w.clone(), e.score)))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: &str, target: &str) -> Item {
        Item { kind: kind.into(), target: target.into(), title: target.into() }
    }

    #[test]
    fn add_creates_then_appends_without_duplicates() {
        let conn = crate::db::open_in_memory().unwrap();
        assert!(list(&conn).unwrap().is_empty());
        add(&conn, "  报销   材料 ", item("file", "C:/a.xlsx"), 10).unwrap();
        add(&conn, "报销 材料", item("url", "https://x.example"), 20).unwrap();
        let w = add(&conn, "报销 材料", item("file", "C:/a.xlsx"), 30).unwrap();
        assert_eq!(w.name, "报销 材料", "whitespace tidied");
        assert_eq!(w.items, vec![item("file", "C:/a.xlsx"), item("url", "https://x.example")], "once each, in order");
        assert_eq!(w.updated, 30);
        // names compare without case
        add(&conn, "Project A", item("app", "C:/code.lnk"), 40).unwrap();
        add(&conn, "project a", item("file", "C:/b.md"), 50).unwrap();
        let all = list(&conn).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!((all[0].name.as_str(), all[0].items.len()), ("Project A", 2), "most recent first, first spelling kept");
    }

    #[test]
    fn bad_input_is_refused() {
        let conn = crate::db::open_in_memory().unwrap();
        assert!(add(&conn, "   ", item("file", "C:/a"), 1).is_err());
        assert!(add(&conn, "x", item("process", "123"), 1).is_err());
        assert!(add(&conn, "x", item("url", ""), 1).is_err());
        assert!(list(&conn).unwrap().is_empty());
        let long = "长".repeat(100);
        assert_eq!(add(&conn, &long, item("file", "C:/a"), 1).unwrap().name.chars().count(), NAME_MAX);
    }

    #[test]
    fn removing_the_last_item_removes_the_workspace() {
        let conn = crate::db::open_in_memory().unwrap();
        add(&conn, "w", item("file", "C:/a"), 1).unwrap();
        add(&conn, "w", item("file", "C:/b"), 2).unwrap();
        remove_item(&conn, "W", "file", "C:/a", 3).unwrap();
        assert_eq!(list(&conn).unwrap()[0].items, vec![item("file", "C:/b")]);
        remove_item(&conn, "w", "file", "C:/b", 4).unwrap();
        assert!(list(&conn).unwrap().is_empty());
        // unknown names and items change nothing
        add(&conn, "v", item("file", "C:/c"), 5).unwrap();
        remove_item(&conn, "nope", "file", "C:/c", 6).unwrap();
        remove_item(&conn, "v", "file", "C:/zzz", 6).unwrap();
        assert_eq!(list(&conn).unwrap()[0].items.len(), 1);
        delete(&conn, "V").unwrap();
        assert!(list(&conn).unwrap().is_empty());
    }

    #[test]
    fn search_by_name() {
        let conn = crate::db::open_in_memory().unwrap();
        add(&conn, "报销", item("file", "C:/a"), 1).unwrap();
        add(&conn, "报销 2026", item("file", "C:/b"), 2).unwrap();
        add(&conn, "年度报销", item("file", "C:/c"), 3).unwrap();
        add(&conn, "Project A", item("file", "C:/d"), 4).unwrap();
        let names = |q: &str| search(&conn, q).unwrap().into_iter().map(|(w, _)| w.name).collect::<Vec<_>>();
        assert_eq!(names("报销"), vec!["报销", "报销 2026", "年度报销"], "exact, prefix, inside");
        let scores: Vec<f32> = search(&conn, "报销").unwrap().into_iter().map(|(_, s)| s).collect();
        assert!(scores[0] == 1.0 && scores[0] > scores[1] && scores[1] > scores[2], "{scores:?}");
        assert_eq!(names("project"), vec!["Project A"]);
        assert_eq!(names("pa"), vec!["Project A"], "initials, as for apps");
        assert_eq!(names("bx")[..2], ["报销", "报销 2026"], "pinyin initials (#19)");
        assert_eq!(names("baoxiao")[0], "报销", "full pinyin");
        assert!(names("").is_empty() && names("  ").is_empty() && names("zzz").is_empty());
        // a listing word alone lists them all, most recent first
        let all = names("工作现场");
        assert_eq!(all.len(), 4);
        assert_eq!(all[0], "Project A");
        assert_eq!(names("WS").len(), 4);
        assert_eq!(names("gzxc").len(), 4, "the initials of 工作现场 (#19)");
    }

    #[test]
    fn a_damaged_record_reads_as_none() {
        let conn = crate::db::open_in_memory().unwrap();
        crate::db::meta_set(&conn, META_KEY, "not json").unwrap();
        assert!(list(&conn).unwrap().is_empty());
        // and the next add starts over cleanly
        add(&conn, "w", item("file", "C:/a"), 1).unwrap();
        assert_eq!(list(&conn).unwrap().len(), 1);
    }
}
