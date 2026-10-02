//! Requests on the command line: `magpie --toggle`, `magpie --source clips`,
//! `magpie --query "invoice"`. A second launch hands its arguments to the
//! running copy (the single-instance plugin), so a key bound in a window
//! manager or desktop settings can open the palette straight on a tab or a
//! query. On Wayland that is how the summon key works at all: the desktop
//! owns the keyboard, and magpie asks it to run `magpie --toggle` (see
//! wayland_hotkey.rs).

use serde::Serialize;

/// What one launch asked for. Empty means a plain launch: show the palette,
/// as a second launch always did.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct Request {
    /// Hide the palette if it is showing, show it otherwise.
    pub toggle: bool,
    /// The tab to open on, as its id (`local`, `github-stars`, `web`, `clips`).
    pub source: Option<String>,
    /// Text to put in the search box.
    pub query: Option<String>,
}

impl Request {
    pub fn is_empty(&self) -> bool {
        *self == Request::default()
    }

    /// Asks for a tab or a query: the palette must show and the page apply it.
    pub fn opens_something(&self) -> bool {
        self.source.is_some() || self.query.is_some()
    }
}

/// The arguments of a launch handed over by the single-instance plugin,
/// without the program name. A key bound on Wayland sends just the flags
/// (a comma-separated list would be cut apart by sway's command parser, see
/// wayland_hotkey.rs), so the first entry is a program name only when it is
/// not a flag.
pub fn after_program(argv: Vec<String>) -> impl Iterator<Item = String> {
    let skip = usize::from(argv.first().is_some_and(|a| !a.starts_with("--")));
    argv.into_iter().skip(skip)
}

/// A tab id from what a person would type. Unknown names give None, and the
/// palette then stays on its current tab.
pub fn source_id(name: &str) -> Option<&'static str> {
    match name.trim().to_ascii_lowercase().as_str() {
        "local" | "files" | "file" => Some("local"),
        "github-stars" | "stars" | "github" => Some("github-stars"),
        "web" | "bookmarks" | "history" => Some("web"),
        "clips" | "clipboard" | "clip" => Some("clips"),
        _ => None,
    }
}

/// Parse the arguments after the program name. `--name value` and
/// `--name=value` both work; anything else is ignored, so a launcher that
/// appends its own flags does not stop the palette from opening.
pub fn parse<I, S>(args: I) -> Request
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut req = Request::default();
    let mut it = args.into_iter().map(Into::into).peekable();
    while let Some(arg) = it.next() {
        let (name, inline) = match arg.split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n.to_string(), Some(v.to_string())),
            _ => (arg.clone(), None),
        };
        match name.as_str() {
            "--toggle" => req.toggle = true,
            "--source" | "--tab" => {
                if let Some(v) = take_value(inline, &mut it) {
                    req.source = source_id(&v).map(str::to_string);
                }
            }
            "--query" | "-q" => {
                if let Some(v) = take_value(inline, &mut it) {
                    req.query = Some(v);
                }
            }
            _ => {}
        }
    }
    req
}

/// A flag's value: the one after `=`, or the next argument unless that is
/// itself a flag.
fn take_value<I: Iterator<Item = String>>(
    inline: Option<String>,
    it: &mut std::iter::Peekable<I>,
) -> Option<String> {
    inline.or_else(|| match it.peek() {
        Some(next) if !next.starts_with("--") => it.next(),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_launch_is_empty() {
        assert!(parse(Vec::<String>::new()).is_empty());
        // flags magpie does not know are ignored, not fatal
        assert!(parse(["--minimized", "--no-sandbox"]).is_empty());
    }

    #[test]
    fn toggle_source_and_query() {
        let r = parse(["--toggle"]);
        assert!(r.toggle && !r.opens_something());
        let r = parse(["--source", "clipboard", "--query", "invoice 2026"]);
        assert_eq!(r.source.as_deref(), Some("clips"));
        assert_eq!(r.query.as_deref(), Some("invoice 2026"));
        assert!(r.opens_something());
        let r = parse(["--source=stars", "-q", "tauri"]);
        assert_eq!(r.source.as_deref(), Some("github-stars"));
        assert_eq!(r.query.as_deref(), Some("tauri"));
        // `--query=` gives an empty box, which is a request too
        assert_eq!(parse(["--query="]).query.as_deref(), Some(""));
    }

    #[test]
    fn the_program_name_is_dropped_only_when_present() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(parse(after_program(v(&["/opt/magpie/magpie", "--toggle"]))).toggle);
        // a key bound on Wayland sends the flags alone
        assert!(parse(after_program(v(&["--toggle"]))).toggle);
        assert!(parse(after_program(v(&["magpie"]))).is_empty());
        assert!(parse(after_program(Vec::new())).is_empty());
    }

    #[test]
    fn a_flag_is_never_taken_as_a_value() {
        let r = parse(["--query", "--toggle"]);
        assert_eq!(r.query, None);
        assert!(r.toggle);
        // an unknown tab leaves the tab alone
        assert_eq!(parse(["--source", "notes"]).source, None);
    }
}
