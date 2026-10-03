//! "Edit with" (#14): Ctrl+E / ⌘E on a file opens it in the program the user
//! picked for its type, Photoshop for images say. The rules are a short
//! list of (extensions → app); a type nobody picked an app for goes to the
//! system's own edit action. Stored in meta as JSON; the backend checks
//! every app against the installed-app list before anything runs.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// One row of the table: these extensions open in this app (an app target
/// from the app list; empty while the user has not picked one).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditRule {
    pub exts: Vec<String>,
    pub app: String,
}

pub const META_KEY: &str = "edit_with";

/// What a fresh install shows: one row for images, waiting for an app.
pub fn default_rules() -> Vec<EditRule> {
    vec![EditRule {
        exts: ["png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "heic", "psd"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        app: String::new(),
    }]
}

/// Extensions as the user typed them ("PNG, .jpg jpeg") → `png`, `jpg`,
/// `jpeg`: lower case, no dots, each once, in the order given.
pub fn normalize_exts(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in raw.iter().flat_map(|s| s.split(|c: char| c == ',' || c == '，' || c.is_whitespace())) {
        let e = part.trim().trim_start_matches('.').to_lowercase();
        if !e.is_empty() && e.chars().all(|c| c.is_alphanumeric()) && !out.contains(&e) {
            out.push(e);
        }
    }
    out
}

/// The rules as stored, or the default when there are none or they do not
/// parse (a hand-edited or older settings file).
pub fn parse_rules(stored: Option<&str>) -> Vec<EditRule> {
    stored
        .and_then(|s| serde_json::from_str::<Vec<EditRule>>(s).ok())
        .unwrap_or_else(default_rules)
}

/// The app picked for `path`'s type: the first row listing its extension
/// that has an app. None means "the system's edit action".
pub fn app_for<'a>(rules: &'a [EditRule], path: &Path) -> Option<&'a str> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    rules
        .iter()
        .find(|r| !r.app.is_empty() && r.exts.contains(&ext))
        .map(|r| r.app.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn extensions_are_normalized() {
        assert_eq!(normalize_exts(&s(&["PNG, .jpg  jpeg", "png", "，webp"])), s(&["png", "jpg", "jpeg", "webp"]));
        assert_eq!(normalize_exts(&s(&["", " , ", "..", "a/b", "tar.gz"])), Vec::<String>::new(), "no paths, no dots inside");
    }

    #[test]
    fn the_first_row_with_an_app_wins() {
        let rules = vec![
            EditRule { exts: s(&["png", "psd"]), app: String::new() },
            EditRule { exts: s(&["png"]), app: "PS".into() },
            EditRule { exts: s(&["png", "svg"]), app: "Other".into() },
            EditRule { exts: s(&["md"]), app: "Typora".into() },
        ];
        assert_eq!(app_for(&rules, Path::new("C:/a/b.PNG")), Some("PS"), "case of the file's extension");
        assert_eq!(app_for(&rules, Path::new("/x/y.svg")), Some("Other"));
        assert_eq!(app_for(&rules, Path::new("/x/notes.md")), Some("Typora"));
        assert_eq!(app_for(&rules, Path::new("/x/a.psd")), None, "listed, but no app picked");
        assert_eq!(app_for(&rules, Path::new("/x/a.txt")), None);
        assert_eq!(app_for(&rules, Path::new("/x/Makefile")), None, "no extension");
        assert_eq!(app_for(&[], Path::new("/x/a.png")), None);
    }

    #[test]
    fn stored_rules_or_the_default() {
        assert_eq!(parse_rules(None), default_rules());
        assert_eq!(parse_rules(Some("not json")), default_rules());
        let rules = vec![EditRule { exts: s(&["png"]), app: "PS".into() }];
        let json = serde_json::to_string(&rules).unwrap();
        assert_eq!(parse_rules(Some(&json)), rules);
        // an empty list the user made stays empty
        assert_eq!(parse_rules(Some("[]")), Vec::<EditRule>::new());
        let d = default_rules();
        assert_eq!(d.len(), 1);
        assert!(d[0].app.is_empty() && d[0].exts.contains(&"png".to_string()) && d[0].exts.contains(&"psd".to_string()));
    }
}
