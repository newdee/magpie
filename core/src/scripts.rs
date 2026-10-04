//! Script commands: scripts the user drops into one folder become commands
//! in the box. A comment block at the top names each one, in the format of
//! Raycast's script commands (`@raycast.title`, `@raycast.mode`,
//! `@raycast.argument1`, …), so scripts written for Raycast work as they
//! are; `@magpie.` works the same and adds `@magpie.keyword`.
//!
//! A script runs only when the user picks it and presses Enter. It runs with
//! the interpreter its shebang names or its extension implies, in its own
//! folder (or `currentDirectoryPath`), for at most a minute; what it prints
//! comes back to the palette.

use anyhow::{anyhow, Result};
use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const META_KEY: &str = "scripts_dir";
/// Only the start of a file is read for its header.
const HEADER_BYTES: usize = 8 * 1024;
/// Output kept from one run, per stream.
const OUTPUT_CAP: usize = 256 * 1024;
pub const RUN_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    /// the output, whole, in the palette
    FullOutput,
    /// the last line of the output, in the palette's status line
    Compact,
    /// nothing shown; the palette goes away at once
    Silent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Arg {
    pub placeholder: String,
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Script {
    pub path: String,
    pub title: String,
    pub mode: Mode,
    pub package: Option<String>,
    pub description: Option<String>,
    /// an emoji, when the header gives one (image icons are not shown)
    pub icon: Option<String>,
    /// a short word that runs it, with what follows as its arguments
    pub keyword: Option<String>,
    pub args: Vec<Arg>,
    /// Enter twice, like the destructive system commands
    pub confirm: bool,
    pub cwd: Option<String>,
    /// how it runs ("bash", "powershell", …); None when nothing on this
    /// machine can run it, which the row says
    pub runner: Option<String>,
}

/// One header line's key and value, from any common comment style.
fn header_field(line: &str) -> Option<(&str, &str)> {
    let mut l = line.trim_start();
    for marker in ["#", "//", "--", "::", ";", "'", "REM ", "rem ", "%"] {
        if let Some(rest) = l.strip_prefix(marker) {
            l = rest.trim_start();
            break;
        }
    }
    let l = l.strip_prefix("@raycast.").or_else(|| l.strip_prefix("@magpie."))?;
    let (key, value) = l.split_once(char::is_whitespace).unwrap_or((l, ""));
    Some((key, value.trim()))
}

/// An icon worth showing: a short run of non-ASCII symbols (an emoji), not
/// a file name or a URL.
fn emoji_icon(v: &str) -> Option<String> {
    let v = v.trim();
    let n = v.chars().count();
    (n > 0 && n <= 8 && !v.chars().any(|c| c.is_ascii_alphanumeric() || c == '/' || c == '.')).then(|| v.to_string())
}

/// The header of one script; None without a title (not a script command).
pub fn parse(path: &Path, text: &str) -> Option<Script> {
    let mut s = Script {
        path: path.to_string_lossy().into_owned(),
        title: String::new(),
        mode: Mode::FullOutput,
        package: None,
        description: None,
        icon: None,
        keyword: None,
        args: Vec::new(),
        confirm: false,
        cwd: None,
        runner: None,
    };
    let mut args: Vec<(u32, Arg)> = Vec::new();
    for line in text.lines().take(120) {
        let Some((key, value)) = header_field(line) else { continue };
        let some = |v: &str| (!v.is_empty()).then(|| v.to_string());
        match key {
            "title" => s.title = value.to_string(),
            "mode" => {
                s.mode = match value {
                    "silent" => Mode::Silent,
                    // inline refreshes on a timer in Raycast; here it runs once
                    "compact" | "inline" => Mode::Compact,
                    _ => Mode::FullOutput,
                }
            }
            "packageName" => s.package = some(value),
            "description" => s.description = some(value),
            "icon" => s.icon = emoji_icon(value),
            "keyword" => s.keyword = some(&value.to_lowercase()).filter(|k| !k.contains(char::is_whitespace)),
            "needsConfirmation" => s.confirm = value == "true",
            "currentDirectoryPath" => s.cwd = some(value),
            k if k.starts_with("argument") => {
                let Ok(n) = k["argument".len()..].parse::<u32>() else { continue };
                let Ok(v) = serde_json::from_str::<serde_json::Value>(value) else { continue };
                let placeholder = v.get("placeholder").and_then(|p| p.as_str()).unwrap_or("").to_string();
                let optional = v.get("optional").and_then(|o| o.as_bool()).unwrap_or(false);
                if (1..=3).contains(&n) {
                    args.push((n, Arg { placeholder, optional }));
                }
            }
            _ => {}
        }
    }
    if s.title.trim().is_empty() {
        return None;
    }
    args.sort_by_key(|(n, _)| *n);
    s.args = args.into_iter().map(|(_, a)| a).collect();
    s.runner = runner_for(path, text).map(|(p, _)| program_label(&p));
    Some(s)
}

fn program_label(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// What runs a script: the program and the arguments before the script's
/// path. The shebang wins; else the extension.
fn runner_for(path: &Path, text: &str) -> Option<(PathBuf, Vec<String>)> {
    let first = text.lines().next().unwrap_or("");
    if let Some(sb) = first.strip_prefix("#!") {
        let mut words = sb.split_whitespace();
        let head = words.next()?;
        let rest: Vec<&str> = words.collect();
        let (prog, extra): (&str, Vec<&str>) = if head.ends_with("/env") {
            // `#!/usr/bin/env -S python3 -u` and `#!/usr/bin/env python3`
            let rest: Vec<&str> = rest.into_iter().filter(|w| *w != "-S").collect();
            let (p, e) = rest.split_first()?;
            (*p, e.to_vec())
        } else {
            (head, rest)
        };
        let direct = Path::new(prog);
        let found = if cfg!(not(windows)) && direct.is_absolute() && direct.is_file() {
            Some(direct.to_path_buf())
        } else {
            // on Windows a Unix path means nothing: look the name up instead
            let name = direct.file_name()?.to_string_lossy().into_owned();
            crate::launch::which(&name).or_else(|| {
                // python3 is "python" on most Windows installs
                name.strip_suffix('3').and_then(crate::launch::which)
            })
        };
        if let Some(p) = found {
            return Some((p, extra.iter().map(|s| s.to_string()).collect()));
        }
    }
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    let pick = |names: &[&str], pre: &[&str]| {
        names.iter().find_map(|n| crate::launch::which(n)).map(|p| (p, pre.iter().map(|s| s.to_string()).collect()))
    };
    match ext.as_str() {
        "ps1" => pick(&["pwsh", "powershell"], &["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]),
        "bat" | "cmd" if cfg!(windows) => pick(&["cmd"], &["/D", "/C"]),
        "py" => pick(&["python3", "python", "py"], &[]),
        "js" | "mjs" | "cjs" => pick(&["node"], &[]),
        "ts" => pick(&["deno", "bun"], &["run"]),
        "rb" => pick(&["ruby"], &[]),
        "sh" | "bash" => pick(&["bash", "sh"], &[]),
        "zsh" => pick(&["zsh"], &[]),
        "applescript" | "scpt" => pick(&["osascript"], &[]),
        _ => None,
    }
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.') || name.starts_with('~')
}

/// Every script command in `dir` (not its subfolders), by title.
pub fn list(dir: &Path) -> Vec<Script> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<Script> = rd
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter(|e| !is_hidden(&e.file_name().to_string_lossy()))
        .filter_map(|e| {
            let mut buf = vec![0u8; HEADER_BYTES];
            let n = std::fs::File::open(e.path()).ok()?.read(&mut buf).ok()?;
            buf.truncate(n);
            parse(&e.path(), &String::from_utf8_lossy(&buf))
        })
        .collect();
    out.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()).then_with(|| a.path.cmp(&b.path)));
    out
}

/// A script found for the query: by its name, or by its keyword (or title)
/// with what follows taken as its arguments.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Match {
    #[serde(flatten)]
    pub script: Script,
    pub score: f32,
    /// the arguments typed after the keyword, in order
    pub given: Vec<String>,
}

/// `rest` cut into at most `n` arguments: words, the last one taking the
/// remainder.
fn split_args(rest: &str, n: usize) -> Vec<String> {
    let rest = rest.trim();
    if rest.is_empty() || n == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut tail = rest;
    while out.len() + 1 < n {
        match tail.split_once(char::is_whitespace) {
            Some((w, r)) => {
                out.push(w.to_string());
                tail = r.trim_start();
            }
            None => break,
        }
    }
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
}

pub fn matches(scripts: &[Script], query: &str) -> Vec<Match> {
    let q = query.trim_start();
    let ql = q.to_lowercase();
    let mut out: Vec<Match> = Vec::new();
    // "keyword rest" (or "title rest"): this script, these arguments
    for s in scripts.iter().filter(|s| !s.args.is_empty()) {
        let triggers = s.keyword.iter().cloned().chain(std::iter::once(s.title.to_lowercase()));
        for t in triggers {
            if let Some(rest) = ql.strip_prefix(&t).filter(|r| r.starts_with(char::is_whitespace)) {
                // the arguments keep the case they were typed in
                let given = split_args(&q[q.len() - rest.len()..], s.args.len());
                out.push(Match { script: s.clone(), score: 0.99, given });
                break;
            }
        }
    }
    // by name, as apps are matched (prefix, word start, initials, pinyin)
    if q.trim().chars().count() >= 2 {
        let entries: Vec<crate::apps::AppEntry> = scripts
            .iter()
            .map(|s| crate::apps::AppEntry {
                name: s.title.clone(),
                target: s.path.clone(),
                aliases: s.keyword.iter().cloned().collect(),
                ..Default::default()
            })
            .collect();
        for e in crate::apps::match_apps(&entries, q, entries.len(), true) {
            if !(e.score >= 0.65 || (e.score >= 0.5 && !q.trim().is_ascii())) {
                continue;
            }
            if out.iter().any(|m| m.script.path == e.target) {
                continue;
            }
            if let Some(s) = scripts.iter().find(|s| s.path == e.target) {
                out.push(Match { script: s.clone(), score: e.score, given: Vec::new() });
            }
        }
    }
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Output {
    pub ok: bool,
    /// None when it was stopped for running too long
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

/// Colours and cursor moves a terminal would act on; the palette shows text.
fn strip_ansi(s: &str) -> String {
    let re = regex::Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07]*\x07").unwrap();
    re.replace_all(s, "").into_owned()
}

/// Drain a pipe into `out`, keeping at most OUTPUT_CAP bytes. Past the cap
/// the pipe is still read, so the script never blocks on a full pipe.
fn read_capped(mut r: impl Read, out: std::sync::Arc<std::sync::Mutex<Vec<u8>>>) {
    let mut buf = [0u8; 8192];
    while let Ok(n) = r.read(&mut buf) {
        if n == 0 {
            break;
        }
        let mut o = out.lock().unwrap();
        if o.len() < OUTPUT_CAP {
            let take = n.min(OUTPUT_CAP - o.len());
            o.extend_from_slice(&buf[..take]);
        }
    }
}

/// Stop the script and everything it started: a child it left running
/// would keep the output pipes open.
fn kill_tree(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .status();
    }
    #[cfg(unix)]
    {
        // the script leads its own process group (see `run`)
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Run a script with these arguments (trailing empty optional ones left
/// out), wait at most `timeout`, and hand back what it printed.
pub fn run(script: &Script, given: &[String], timeout: Duration) -> Result<Output> {
    let path = Path::new(&script.path);
    let mut head = vec![0u8; HEADER_BYTES];
    let n = std::fs::File::open(path)?.read(&mut head)?;
    head.truncate(n);
    let (program, pre) = runner_for(path, &String::from_utf8_lossy(&head))
        .ok_or_else(|| anyhow!("nothing on this computer runs {}", path.display()))?;
    for (i, a) in script.args.iter().enumerate() {
        if !a.optional && given.get(i).is_none_or(|g| g.trim().is_empty()) {
            return Err(anyhow!("“{}” is needed", if a.placeholder.is_empty() { "an argument" } else { &a.placeholder }));
        }
    }
    let mut args: Vec<String> = given.iter().take(script.args.len()).cloned().collect();
    while args.last().is_some_and(|a| a.is_empty()) {
        args.pop();
    }
    let dir = script
        .cwd
        .as_deref()
        .map(|c| match c.strip_prefix("~") {
            Some(rest) => dirs_home().map(|h| h.join(rest.trim_start_matches(['/', '\\']))).unwrap_or_else(|| PathBuf::from(c)),
            None => PathBuf::from(c),
        })
        .or_else(|| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    let mut cmd = Command::new(&program);
    cmd.args(&pre).arg(path).args(&args).current_dir(&dir).env("MAGPIE", "1");
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0); // so a timeout can stop what it started too
    }
    let mut child = cmd.spawn().map_err(|e| anyhow!("could not start {}: {e}", program.display()))?;
    let out_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let err_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let readers = [
        {
            let (p, b) = (child.stdout.take().unwrap(), out_buf.clone());
            std::thread::spawn(move || read_capped(p, b))
        },
        {
            let (p, b) = (child.stderr.take().unwrap(), err_buf.clone());
            std::thread::spawn(move || read_capped(p, b))
        },
    ];
    let started = Instant::now();
    let (code, timed_out) = loop {
        if let Some(status) = child.try_wait()? {
            break (status.code(), false);
        }
        if started.elapsed() >= timeout {
            kill_tree(&mut child);
            break (None, true);
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    // the pipes close when the last process holding them ends; one the
    // script left running in the background must not hold this answer
    let grace = Instant::now();
    while readers.iter().any(|r| !r.is_finished()) && grace.elapsed() < Duration::from_secs(1) {
        std::thread::sleep(Duration::from_millis(10));
    }
    let decode = |b: &std::sync::Mutex<Vec<u8>>| strip_ansi(&String::from_utf8_lossy(&b.lock().unwrap())).replace("\r\n", "\n");
    let stdout = decode(&out_buf);
    let stderr = decode(&err_buf);
    Ok(Output { ok: code == Some(0) && !timed_out, code, stdout, stderr, timed_out })
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// The example a new scripts folder starts with, for this system.
pub fn example() -> (&'static str, &'static str) {
    if cfg!(windows) {
        (
            "hello.ps1",
            "# @raycast.schemaVersion 1\r\n\
             # @raycast.title Hello\r\n\
             # @raycast.mode fullOutput\r\n\
             # @raycast.argument1 { \"type\": \"text\", \"placeholder\": \"name\", \"optional\": true }\r\n\
             # @raycast.description An example script command. Change it or delete it.\r\n\
             # @magpie.keyword hello\r\n\
             param([string]$name = \"world\")\r\n\
             \"Hello, $name!\"\r\n\
             \"Today is $(Get-Date -Format 'yyyy-MM-dd').\"\r\n",
        )
    } else {
        (
            "hello.sh",
            "#!/bin/bash\n\
             # @raycast.schemaVersion 1\n\
             # @raycast.title Hello\n\
             # @raycast.mode fullOutput\n\
             # @raycast.argument1 { \"type\": \"text\", \"placeholder\": \"name\", \"optional\": true }\n\
             # @raycast.description An example script command. Change it or delete it.\n\
             # @magpie.keyword hello\n\
             echo \"Hello, ${1:-world}!\"\n\
             date +\"Today is %Y-%m-%d.\"\n",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("magpie-scripts-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_raycast_header_parses() {
        let text = "#!/bin/bash\n\
            # Required parameters:\n\
            # @raycast.schemaVersion 1\n\
            # @raycast.title Search Issues\n\
            # @raycast.mode compact\n\
            # @raycast.icon 🐛\n\
            # @raycast.packageName GitHub\n\
            # @raycast.argument2 { \"type\": \"text\", \"placeholder\": \"label\", \"optional\": true }\n\
            # @raycast.argument1 { \"type\": \"text\", \"placeholder\": \"repo\" }\n\
            # @raycast.needsConfirmation true\n\
            # @magpie.keyword gi\n\
            echo hi\n";
        let s = parse(Path::new("/x/issues.sh"), text).unwrap();
        assert_eq!(s.title, "Search Issues");
        assert_eq!(s.mode, Mode::Compact);
        assert_eq!((s.icon.as_deref(), s.package.as_deref(), s.keyword.as_deref()), (Some("🐛"), Some("GitHub"), Some("gi")));
        assert_eq!(
            s.args,
            vec![Arg { placeholder: "repo".into(), optional: false }, Arg { placeholder: "label".into(), optional: true }],
            "in argument order"
        );
        assert!(s.confirm);
    }

    #[test]
    fn other_comment_styles_and_defaults() {
        let s = parse(Path::new("a.js"), "// @raycast.title Hi\n// @raycast.icon images/x.png\nconsole.log(1)").unwrap();
        assert_eq!((s.title.as_str(), s.mode, s.icon), ("Hi", Mode::FullOutput, None), "an image icon is not shown");
        let s = parse(Path::new("a.bat"), "@echo off\r\nREM @raycast.title Bat One\r\n:: @raycast.mode silent\r\n").unwrap();
        assert_eq!((s.title.as_str(), s.mode), ("Bat One", Mode::Silent));
        let s = parse(Path::new("a.lua"), "-- @magpie.title Lua\n-- @magpie.mode inline").unwrap();
        assert_eq!(s.mode, Mode::Compact, "inline runs once");
        assert_eq!(parse(Path::new("a.sh"), "#!/bin/sh\necho no header\n"), None);
        assert_eq!(parse(Path::new("a.sh"), "# @raycast.title    \n"), None, "a blank title is none");
        // a bad argument line is skipped, not fatal
        let s = parse(Path::new("a.sh"), "# @raycast.title T\n# @raycast.argument1 {not json\n# @raycast.argument9 {\"placeholder\":\"x\"}").unwrap();
        assert!(s.args.is_empty());
    }

    #[test]
    fn the_folder_lists_its_scripts_by_title() {
        let d = tmp("list");
        std::fs::write(d.join("b.sh"), "# @raycast.title beta\n").unwrap();
        std::fs::write(d.join("a.sh"), "# @raycast.title Alpha\n").unwrap();
        std::fs::write(d.join("notes.txt"), "not a script").unwrap();
        std::fs::write(d.join(".hidden.sh"), "# @raycast.title Hidden\n").unwrap();
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("sub").join("c.sh"), "# @raycast.title Deep\n").unwrap();
        let titles: Vec<String> = list(&d).into_iter().map(|s| s.title).collect();
        assert_eq!(titles, vec!["Alpha", "beta"]);
        assert!(list(&d.join("missing")).is_empty());
        std::fs::remove_dir_all(&d).unwrap();
    }

    fn script(title: &str, keyword: Option<&str>, args: usize) -> Script {
        Script {
            path: format!("/s/{title}"),
            title: title.into(),
            mode: Mode::FullOutput,
            package: None,
            description: None,
            icon: None,
            keyword: keyword.map(str::to_string),
            args: (0..args).map(|i| Arg { placeholder: format!("a{i}"), optional: false }).collect(),
            confirm: false,
            cwd: None,
            runner: Some("bash".into()),
        }
    }

    #[test]
    fn found_by_name_or_by_keyword_with_arguments() {
        let all = vec![script("Translate", Some("tr"), 1), script("Git Pull All", None, 0), script("Copy Date", None, 2)];
        let m = matches(&all, "tr Hello World");
        assert_eq!((m[0].script.title.as_str(), m[0].given.clone()), ("Translate", vec!["Hello World".to_string()]), "one argument takes the rest, case kept");
        let m = matches(&all, "copy date  2026-10-04  long rest here");
        assert_eq!(m[0].given, vec!["2026-10-04".to_string(), "long rest here".to_string()]);
        assert_eq!(matches(&all, "gpa")[0].script.title, "Git Pull All", "initials, as for apps");
        assert_eq!(matches(&all, "trans")[0].given, Vec::<String>::new(), "by name: no arguments yet");
        assert!(matches(&all, "trx something").is_empty());
        assert!(matches(&all, "z").is_empty(), "one letter is too little");
        // a keyword with nothing after it is a name match, not an argument run
        assert!(matches(&all, "tr").iter().all(|m| m.given.is_empty()));
        assert_eq!(split_args("a  b c", 2), vec!["a".to_string(), "b c".to_string()]);
        assert_eq!(split_args("one", 3), vec!["one".to_string()]);
        assert!(split_args("   ", 2).is_empty());
    }

    #[test]
    fn ansi_colours_are_dropped() {
        assert_eq!(strip_ansi("\x1b[1;32mok\x1b[0m done\x1b]0;title\x07"), "ok done");
    }

    #[test]
    fn a_script_runs_with_its_arguments_and_reports_failure() {
        let d = tmp("run");
        let (name, body) = if cfg!(windows) {
            ("echo.cmd", "@echo off\r\nREM @raycast.title Echo\r\nREM @raycast.argument1 {\"placeholder\":\"x\"}\r\nREM @raycast.argument2 {\"placeholder\":\"y\",\"optional\":true}\r\necho got %1 %2\r\necho oops 1>&2\r\nexit /b 3\r\n")
        } else {
            ("echo.sh", "#!/bin/sh\n# @raycast.title Echo\n# @raycast.argument1 {\"placeholder\":\"x\"}\n# @raycast.argument2 {\"placeholder\":\"y\",\"optional\":true}\necho got $1 $2\necho oops 1>&2\nexit 3\n")
        };
        std::fs::write(d.join(name), body).unwrap();
        let s = list(&d).pop().unwrap();
        assert!(s.runner.is_some(), "{s:?}");
        let out = run(&s, &["first".into(), "".into()], RUN_TIMEOUT).unwrap();
        assert_eq!(out.stdout.trim(), "got first", "{out:?}");
        assert!(out.stderr.contains("oops"));
        assert_eq!((out.ok, out.code, out.timed_out), (false, Some(3), false));
        // a needed argument missing: refused before anything runs
        let e = run(&s, &[], RUN_TIMEOUT).unwrap_err().to_string();
        assert!(e.contains("x"), "{e}");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_script_that_runs_too_long_is_stopped() {
        let d = tmp("slow");
        let (name, body) = if cfg!(windows) {
            ("slow.cmd", "@echo off\r\nREM @raycast.title Slow\r\necho started\r\nping -n 30 127.0.0.1 >nul\r\n")
        } else {
            ("slow.sh", "#!/bin/sh\n# @raycast.title Slow\necho started\nsleep 30\n")
        };
        std::fs::write(d.join(name), body).unwrap();
        let s = list(&d).pop().unwrap();
        let t = Instant::now();
        let out = run(&s, &[], Duration::from_millis(1500)).unwrap();
        assert!(out.timed_out && !out.ok && out.code.is_none(), "{out:?}");
        assert!(t.elapsed() < Duration::from_secs(10), "stopped, not waited out: {:?}", t.elapsed());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_child_left_running_does_not_hold_the_answer() {
        let d = tmp("bg");
        let (name, body) = if cfg!(windows) {
            ("bg.cmd", "@echo off\r\nREM @raycast.title Bg\r\nstart /b \"\" ping -n 20 127.0.0.1\r\necho done\r\n")
        } else {
            ("bg.sh", "#!/bin/sh\n# @raycast.title Bg\nsleep 20 &\necho done\n")
        };
        std::fs::write(d.join(name), body).unwrap();
        let s = list(&d).pop().unwrap();
        let t = Instant::now();
        let out = run(&s, &[], RUN_TIMEOUT).unwrap();
        assert!(out.ok && out.stdout.contains("done"), "{out:?}");
        assert!(t.elapsed() < Duration::from_secs(5), "waited {:?}", t.elapsed());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_example_is_a_script_command() {
        let (name, body) = example();
        let s = parse(Path::new(name), body).unwrap();
        assert_eq!((s.title.as_str(), s.keyword.as_deref(), s.args.len()), ("Hello", Some("hello"), 1));
        assert!(s.args[0].optional);
    }
}
