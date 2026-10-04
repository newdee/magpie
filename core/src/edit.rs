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

/// The row that decides `ext`: the first one listing it that has an app.
fn winner(rules: &[EditRule], ext: &str) -> Option<usize> {
    rules.iter().position(|r| !r.app.is_empty() && r.exts.iter().any(|e| e == ext))
}

/// The app picked for `path`'s type: the first row listing its extension
/// that has an app. None means "the system's edit action".
pub fn app_for<'a>(rules: &'a [EditRule], path: &Path) -> Option<&'a str> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    winner(rules, &ext).map(|i| rules[i].app.as_str())
}

/// An extension a row lists but does not decide (#18): another row opens it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Shadowed {
    pub ext: String,
    /// the deciding row, counted from 0
    pub by: usize,
}

/// A row as the settings table shows it: the rule, and which of its
/// extensions another row decides (an app picked above, or this row has no
/// app and another row has one). Same rule as [`app_for`], so the table says
/// exactly what Ctrl+E will do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuleView {
    #[serde(flatten)]
    pub rule: EditRule,
    pub shadowed: Vec<Shadowed>,
}

pub fn view(rules: Vec<EditRule>) -> Vec<RuleView> {
    let decided: Vec<Vec<Shadowed>> = rules
        .iter()
        .enumerate()
        .map(|(i, r)| {
            r.exts
                .iter()
                .filter_map(|e| winner(&rules, e).filter(|&w| w != i).map(|by| Shadowed { ext: e.clone(), by }))
                .collect()
        })
        .collect();
    rules.into_iter().zip(decided).map(|(rule, shadowed)| RuleView { rule, shadowed }).collect()
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
    fn the_table_names_extensions_another_row_decides() {
        let rules = vec![
            EditRule { exts: s(&["png", "psd"]), app: "PS".into() },
            EditRule { exts: s(&["png", "svg"]), app: "Compositor".into() },
            EditRule { exts: s(&["svg", "md"]), app: String::new() },
            EditRule { exts: s(&["txt"]), app: String::new() },
        ];
        let v = view(rules.clone());
        let sh = |i: usize| v[i].shadowed.iter().map(|x| (x.ext.as_str(), x.by)).collect::<Vec<_>>();
        assert_eq!(sh(0), vec![], "the first row decides its own");
        assert_eq!(sh(1), vec![("png", 0)], "png goes to PS above (#18)");
        assert_eq!(sh(2), vec![("svg", 1)], "a row with no app loses svg to the row that has one");
        assert_eq!(sh(3), vec![], "nobody else lists txt: the system default holds");
        // the table and Ctrl+E agree on every listed extension
        for (i, r) in v.iter().enumerate() {
            for e in &r.rule.exts {
                let decided_here = !r.shadowed.iter().any(|x| &x.ext == e);
                let app = app_for(&rules, Path::new(&format!("/x/a.{e}")));
                if decided_here {
                    assert_eq!(app, Some(rules[i].app.as_str()).filter(|a| !a.is_empty()), "{e}");
                } else {
                    let by = r.shadowed.iter().find(|x| &x.ext == e).unwrap().by;
                    assert_eq!(app, Some(rules[by].app.as_str()), "{e}");
                }
            }
        }
        // flattened for the frontend: the rule's fields plus `shadowed`
        let json = serde_json::to_value(&v[1]).unwrap();
        assert_eq!(json["app"], "Compositor");
        assert_eq!(json["shadowed"][0]["by"], 0);
        assert!(view(vec![]).is_empty());
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
