//! Opening a result with another program: its folder in a terminal, the
//! file or folder in an editor. Every opening is planned first ([`Launch`]:
//! program, arguments, working folder), so a test or a dry run can check
//! exactly what would run without anything appearing on screen.

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

/// What to run, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
    /// working folder of the new process
    pub cwd: Option<PathBuf>,
    /// Windows: hand `program` (a shortcut or an app) to the shell, which
    /// resolves `.lnk` targets, instead of starting it as a process
    pub shell_open: bool,
    /// Windows: give a console program its own window
    pub new_console: bool,
}

impl std::fmt::Display for Launch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.program)?;
        for a in &self.args {
            write!(f, " {a:?}")?;
        }
        if let Some(c) = &self.cwd {
            write!(f, " (in {})", c.display())?;
        }
        Ok(())
    }
}

impl Launch {
    fn new(program: impl Into<String>, args: &[&str]) -> Self {
        Launch {
            program: program.into(),
            args: args.iter().map(|a| a.to_string()).collect(),
            cwd: None,
            shell_open: false,
            new_console: false,
        }
    }
}

/// The folder a terminal should open in: the path itself for a folder, its
/// parent for a file.
pub fn folder_of(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().map(Path::to_path_buf).unwrap_or_else(|| path.to_path_buf())
    }
}

/// A terminal in `dir`: Windows Terminal when installed, else PowerShell in
/// a window of its own; Terminal.app on a Mac; on Linux the first terminal
/// found on PATH, starting with the distribution's default.
pub fn terminal_at(dir: &Path) -> Result<Launch> {
    let d = dir.to_string_lossy().to_string();
    #[cfg(target_os = "windows")]
    {
        let mut l = if which("wt.exe").is_some() {
            Launch::new("wt.exe", &["-d", &d])
        } else {
            let mut p = Launch::new("powershell.exe", &["-NoExit"]);
            p.new_console = true;
            p
        };
        l.cwd = Some(dir.to_path_buf());
        Ok(l)
    }
    #[cfg(target_os = "macos")]
    {
        Ok(Launch::new("open", &["-a", "Terminal", &d]))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let wd = format!("--working-directory={d}");
        let candidates: [(&str, Vec<&str>); 7] = [
            ("x-terminal-emulator", vec![]),
            ("gnome-terminal", vec![wd.as_str()]),
            ("konsole", vec!["--workdir", d.as_str()]),
            ("xfce4-terminal", vec![wd.as_str()]),
            ("kitty", vec!["--directory", d.as_str()]),
            ("alacritty", vec!["--working-directory", d.as_str()]),
            ("xterm", vec![]),
        ];
        let (prog, args) = candidates
            .into_iter()
            .find(|(p, _)| which(p).is_some())
            .ok_or_else(|| anyhow!("no terminal found on PATH"))?;
        let mut l = Launch::new(prog, &args);
        l.cwd = Some(dir.to_path_buf());
        Ok(l)
    }
}

/// An installed editor (an app target from the app list: a Start Menu
/// shortcut, an `.app` bundle, a `.desktop` file) opening `path`.
pub fn editor_with(app_target: &str, path: &Path) -> Result<Launch> {
    let p = path.to_string_lossy().to_string();
    #[cfg(target_os = "windows")]
    {
        let mut l = Launch::new(app_target, &[&p]);
        l.shell_open = true;
        Ok(l)
    }
    #[cfg(target_os = "macos")]
    {
        Ok(Launch::new("open", &["-a", app_target, &p]))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let exec = crate::apps::desktop_exec(Path::new(app_target))
            .ok_or_else(|| anyhow!("{app_target} has no Exec line"))?;
        // the file goes where the entry asks for one (%f %F %u %U), else last
        let mut placed = false;
        let mut tokens: Vec<String> = Vec::new();
        for t in exec.split_whitespace() {
            match t {
                "%f" | "%F" | "%u" | "%U" if !placed => {
                    tokens.push(p.clone());
                    placed = true;
                }
                t if t.starts_with('%') => {}
                t => tokens.push(t.trim_matches('"').to_string()),
            }
        }
        if !placed {
            tokens.push(p);
        }
        let (prog, args) = tokens.split_first().ok_or_else(|| anyhow!("empty Exec line"))?;
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        Ok(Launch::new(prog.as_str(), &args))
    }
}

/// Start what [`terminal_at`] or [`editor_with`] planned.
pub fn run(l: &Launch) -> Result<()> {
    #[cfg(target_os = "windows")]
    if l.shell_open {
        use windows::core::HSTRING;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        // one path per argument; Windows paths cannot contain a quote
        let params = l.args.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(" ");
        let r = unsafe {
            ShellExecuteW(None, &HSTRING::from("open"), &HSTRING::from(l.program.as_str()), &HSTRING::from(params), None, SW_SHOWNORMAL)
        };
        // ShellExecute returns a value above 32 on success
        return if r.0 as isize > 32 { Ok(()) } else { Err(anyhow!("could not open {} (code {})", l.program, r.0 as isize)) };
    }
    let mut cmd = std::process::Command::new(&l.program);
    cmd.args(&l.args);
    if let Some(c) = &l.cwd {
        cmd.current_dir(c);
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        if l.new_console {
            cmd.creation_flags(0x0000_0010); // CREATE_NEW_CONSOLE
        }
    }
    cmd.spawn().map(|_| ()).map_err(|e| anyhow!("could not start {}: {e}", l.program))
}

/// A program on PATH (on Windows, with PATHEXT extensions tried).
pub fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    #[cfg(target_os = "windows")]
    let exts: Vec<String> = if Path::new(program).extension().is_some() {
        vec![String::new()]
    } else {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .map(|e| e.to_string())
            .collect()
    };
    #[cfg(not(target_os = "windows"))]
    let exts = vec![String::new()];
    for dir in std::env::split_paths(&path) {
        for e in &exts {
            let candidate = dir.join(format!("{program}{e}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_opens_its_folder_a_folder_itself() {
        let dir = std::env::temp_dir().join(format!("magpie-launch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a b & c.txt");
        std::fs::write(&file, "x").unwrap();
        assert_eq!(folder_of(&file), dir);
        assert_eq!(folder_of(&dir), dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn terminals_open_in_the_folder() {
        let dir = std::env::temp_dir();
        match terminal_at(&dir) {
            Ok(l) => {
                let shown = l.to_string();
                let d = dir.to_string_lossy().to_string();
                assert!(l.cwd.as_deref() == Some(dir.as_path()) || l.args.iter().any(|a| a.contains(&d)), "{shown}");
                #[cfg(target_os = "windows")]
                assert!(l.program == "wt.exe" || (l.program == "powershell.exe" && l.new_console), "{shown}");
                #[cfg(target_os = "macos")]
                assert_eq!(l.args[..2], ["-a".to_string(), "Terminal".to_string()]);
            }
            // a headless Linux box may have no terminal at all
            Err(e) => {
                let headless_linux_ok = cfg!(all(unix, not(target_os = "macos")));
                assert!(headless_linux_ok, "{e}");
            }
        }
    }

    #[test]
    fn editors_get_the_path_as_one_argument() {
        let path = Path::new(if cfg!(windows) { r"C:\work\a b & c.txt" } else { "/work/a b & c.txt" });
        #[cfg(target_os = "windows")]
        {
            let l = editor_with(r"C:\Start Menu\Visual Studio Code.lnk", path).unwrap();
            assert!(l.shell_open);
            assert_eq!(l.args, vec![path.to_string_lossy().to_string()]);
        }
        #[cfg(target_os = "macos")]
        {
            let l = editor_with("/Applications/Cursor.app", path).unwrap();
            assert_eq!(l.args, vec!["-a", "/Applications/Cursor.app", "/work/a b & c.txt"]);
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let dir = std::env::temp_dir().join(format!("magpie-desktop-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let entry = dir.join("code.desktop");
            std::fs::write(&entry, "[Desktop Entry]\nName=Visual Studio Code\nExec=/usr/share/code/code --unity-launch %F\n").unwrap();
            let l = editor_with(entry.to_str().unwrap(), path).unwrap();
            assert_eq!(l.program, "/usr/share/code/code");
            assert_eq!(l.args, vec!["--unity-launch", "/work/a b & c.txt"]);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
