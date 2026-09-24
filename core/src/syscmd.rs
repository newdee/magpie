//! System commands from the query box: lock, sleep, restart, shut down,
//! empty the trash, toggle dark mode.
//!
//! Matching reuses the app matcher (prefix, word start, initials, pinyin of
//! the Chinese name), so `lock`, `锁屏` and `sp` all find the lock screen.
//! Destructive commands carry a flag; the palette asks for a second Enter
//! before running them.

use anyhow::{anyhow, Result};
use serde::Serialize;

use crate::apps::{match_apps, AppEntry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    Mac,
    Linux,
}

pub const CURRENT: Os = if cfg!(target_os = "windows") {
    Os::Windows
} else if cfg!(target_os = "macos") {
    Os::Mac
} else {
    Os::Linux
};

/// One command: stable id, the names it answers to, and whether running it
/// loses something (unsaved work, the trash) and so needs a confirmation.
struct Command {
    id: &'static str,
    names: &'static [&'static str],
    destructive: bool,
}

const COMMANDS: &[Command] = &[
    Command { id: "lock", names: &["Lock Screen", "锁屏", "lock"], destructive: false },
    Command { id: "sleep", names: &["Sleep", "睡眠", "suspend"], destructive: false },
    Command { id: "restart", names: &["Restart", "重启", "reboot"], destructive: true },
    Command { id: "shutdown", names: &["Shut Down", "关机", "shutdown", "power off"], destructive: true },
    Command {
        id: "empty_trash",
        names: &["Empty Trash", "Empty Recycle Bin", "清空废纸篓", "清空回收站", "trash", "recycle bin"],
        destructive: true,
    },
    Command {
        id: "dark_mode",
        names: &["Toggle Dark Mode", "切换深色模式", "dark mode", "light mode", "深色模式", "浅色模式"],
        destructive: false,
    },
];

#[derive(Debug, Clone, Serialize)]
pub struct CommandHit {
    pub id: &'static str,
    pub destructive: bool,
    pub score: f32,
}

/// Commands matching a query. Only strong matches count (a prefix, a word
/// start, initials, pinyin, an exact name): a stray letter inside a word
/// must not offer to shut the machine down.
pub fn match_commands(query: &str) -> Vec<CommandHit> {
    if query.trim().chars().count() < 2 {
        return Vec::new();
    }
    let entries: Vec<AppEntry> = COMMANDS
        .iter()
        .map(|c| AppEntry {
            name: c.names[0].to_string(),
            target: c.id.to_string(),
            aliases: c.names[1..].iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        })
        .collect();
    match_apps(&entries, query, COMMANDS.len(), true)
        .into_iter()
        .filter(|e| e.score >= 0.65 || (e.score >= 0.5 && !query.trim().is_ascii()))
        .filter_map(|e| {
            COMMANDS.iter().find(|c| c.id == e.target).map(|c| CommandHit {
                id: c.id,
                destructive: c.destructive,
                score: e.score,
            })
        })
        .collect()
}

pub fn is_destructive(id: &str) -> Option<bool> {
    COMMANDS.iter().find(|c| c.id == id).map(|c| c.destructive)
}

/// What running `id` does on `os`, as one readable line. Pure: the tests pin
/// every platform's mechanism from any machine, and a dry run reports it
/// instead of acting.
pub fn describe(id: &str, os: Os) -> Option<String> {
    let s = match (id, os) {
        ("lock", Os::Windows) => "user32 LockWorkStation",
        ("lock", Os::Mac) => "login.framework SACLockScreenImmediate",
        ("lock", Os::Linux) => "loginctl lock-session",
        ("sleep", Os::Windows) => "powrprof SetSuspendState(sleep)",
        ("sleep", Os::Mac) => "pmset sleepnow",
        ("sleep", Os::Linux) => "systemctl suspend",
        ("restart", Os::Windows) => "shutdown /r /t 0",
        ("restart", Os::Mac) => "osascript: loginwindow «event aevtrrst»",
        ("restart", Os::Linux) => "systemctl reboot",
        ("shutdown", Os::Windows) => "shutdown /s /t 0",
        ("shutdown", Os::Mac) => "osascript: loginwindow «event aevtrsdn»",
        ("shutdown", Os::Linux) => "systemctl poweroff",
        ("empty_trash", Os::Windows) => "shell32 SHEmptyRecycleBinW",
        ("empty_trash", Os::Mac) => "osascript: Finder empty trash",
        ("empty_trash", Os::Linux) => "gio trash --empty",
        ("dark_mode", Os::Windows) => "HKCU Themes\\Personalize AppsUseLightTheme/SystemUsesLightTheme + WM_SETTINGCHANGE",
        ("dark_mode", Os::Mac) => "osascript: System Events appearance preferences dark mode",
        ("dark_mode", Os::Linux) => "gsettings org.gnome.desktop.interface color-scheme",
        _ => return None,
    };
    Some(s.to_string())
}

/// Run `id` on this machine. With `MAGPIE_SYSCMD_DRYRUN` set, only report
/// what would run (tests use this; nothing here is safe to try for real).
pub fn run(id: &str) -> Result<String> {
    let what = describe(id, CURRENT).ok_or_else(|| anyhow!("unknown command {id:?}"))?;
    if std::env::var_os("MAGPIE_SYSCMD_DRYRUN").is_some() {
        return Ok(format!("dry run: {what}"));
    }
    imp::run(id)?;
    Ok(what)
}

/// A helper process without a console window flashing up on Windows.
#[allow(dead_code)] // not every platform spawns one
fn spawn(program: &str, args: &[&str]) -> Result<std::process::Output> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().map_err(|e| anyhow!("{program}: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow!("{program} failed: {}", err.trim()));
    }
    Ok(out)
}

#[cfg(target_os = "windows")]
mod imp {
    use super::spawn;
    use anyhow::{anyhow, Result};

    pub fn run(id: &str) -> Result<()> {
        match id {
            "lock" => {
                // SAFETY: no arguments; fails only without an interactive desktop
                unsafe { windows::Win32::System::Shutdown::LockWorkStation() }.map_err(|e| anyhow!("lock: {e}"))
            }
            "sleep" => {
                // SAFETY: plain call; sleep, not hibernate, wake events allowed
                let ok = unsafe { windows::Win32::System::Power::SetSuspendState(false, false, false) };
                if ok { Ok(()) } else { Err(anyhow!("sleep was refused")) }
            }
            "restart" => spawn("shutdown", &["/r", "/t", "0"]).map(|_| ()),
            "shutdown" => spawn("shutdown", &["/s", "/t", "0"]).map(|_| ()),
            "empty_trash" => {
                use windows::Win32::UI::Shell::{SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOSOUND};
                // SAFETY: null window and root mean every drive's bin. The
                // palette already asked; an empty bin reports an error code
                // that is not a failure for us
                let _ = unsafe { SHEmptyRecycleBinW(None, None, SHERB_NOCONFIRMATION | SHERB_NOSOUND) };
                Ok(())
            }
            "dark_mode" => toggle_dark_mode(),
            _ => Err(anyhow!("unknown command {id:?}")),
        }
    }

    fn toggle_dark_mode() -> Result<()> {
        use windows::core::w;
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::System::Registry::{
            RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
        };
        let key = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
        let mut light: u32 = 1;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: a DWORD read into a local of the right size
        let read = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key,
                w!("AppsUseLightTheme"),
                RRF_RT_REG_DWORD,
                None,
                Some(&mut light as *mut u32 as *mut _),
                Some(&mut size),
            )
        };
        if read.is_err() {
            light = 1; // never set: Windows defaults to light
        }
        let next: u32 = if light == 0 { 1 } else { 0 };
        for name in [w!("AppsUseLightTheme"), w!("SystemUsesLightTheme")] {
            // SAFETY: a DWORD written from a local
            let res = unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key,
                    name,
                    REG_DWORD.0,
                    Some(&next as *const u32 as *const _),
                    std::mem::size_of::<u32>() as u32,
                )
            };
            if res.is_err() {
                return Err(anyhow!("dark mode: registry write failed ({res:?})"));
            }
        }
        // tell running apps and the shell to re-read the theme
        // SAFETY: a broadcast with a static string; hung windows are skipped
        unsafe {
            let _ = SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(w!("ImmersiveColorSet").as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                200,
                None,
            );
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::spawn;
    use anyhow::{anyhow, Result};

    pub fn run(id: &str) -> Result<()> {
        match id {
            "lock" => lock(),
            "sleep" => spawn("pmset", &["sleepnow"]).map(|_| ()),
            "restart" => osascript("tell application \"loginwindow\" to «event aevtrrst»"),
            "shutdown" => osascript("tell application \"loginwindow\" to «event aevtrsdn»"),
            // the first run asks for permission to control Finder / System
            // Events; macOS remembers the answer
            "empty_trash" => osascript("tell application \"Finder\" to empty trash"),
            "dark_mode" => osascript(
                "tell application \"System Events\" to tell appearance preferences to set dark mode to not dark mode",
            ),
            _ => Err(anyhow!("unknown command {id:?}")),
        }
    }

    fn osascript(script: &str) -> Result<()> {
        spawn("osascript", &["-e", script]).map(|_| ())
    }

    /// The lock that Control-Command-Q triggers. It lives in a private
    /// framework, so it is looked up at run time; if Apple moves it, the
    /// display is put to sleep instead, which locks when a password is
    /// required after sleep (the default).
    fn lock() -> Result<()> {
        const LOGIN: &str = "/System/Library/PrivateFrameworks/login.framework/Versions/Current/login";
        // SAFETY: the symbol takes no arguments and returns nothing
        let locked = unsafe {
            libloading::Library::new(LOGIN).ok().and_then(|lib| {
                let f: libloading::Symbol<unsafe extern "C" fn()> = lib.get(b"SACLockScreenImmediate\0").ok()?;
                f();
                Some(())
            })
        };
        match locked {
            Some(()) => Ok(()),
            None => spawn("pmset", &["displaysleepnow"]).map(|_| ()),
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::spawn;
    use anyhow::{anyhow, Result};

    pub fn run(id: &str) -> Result<()> {
        match id {
            "lock" => spawn("loginctl", &["lock-session"]).map(|_| ()),
            "sleep" => spawn("systemctl", &["suspend"]).map(|_| ()),
            "restart" => spawn("systemctl", &["reboot"]).map(|_| ()),
            "shutdown" => spawn("systemctl", &["poweroff"]).map(|_| ()),
            "empty_trash" => spawn("gio", &["trash", "--empty"]).map(|_| ()),
            "dark_mode" => {
                let key = "org.gnome.desktop.interface";
                let now = spawn("gsettings", &["get", key, "color-scheme"])?;
                let dark = String::from_utf8_lossy(&now.stdout).contains("dark");
                let next = if dark { "default" } else { "prefer-dark" };
                spawn("gsettings", &["set", key, "color-scheme", next]).map(|_| ())
            }
            _ => Err(anyhow!("unknown command {id:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(q: &str) -> Vec<&'static str> {
        match_commands(q).into_iter().map(|h| h.id).collect()
    }

    #[test]
    fn commands_answer_to_english_chinese_and_pinyin() {
        assert_eq!(ids("lock")[0], "lock");
        assert_eq!(ids("锁屏")[0], "lock");
        assert_eq!(ids("suoping")[0], "lock");
        assert_eq!(ids("sp")[0], "lock", "pinyin initials of 锁屏");
        assert_eq!(ids("shut")[0], "shutdown");
        assert_eq!(ids("关机")[0], "shutdown");
        assert_eq!(ids("reboot")[0], "restart");
        assert_eq!(ids("dark")[0], "dark_mode");
        assert_eq!(ids("trash")[0], "empty_trash");
        assert_eq!(ids("回收站")[0], "empty_trash");
    }

    #[test]
    fn stray_letters_never_offer_a_command() {
        // one letter, and letters that sit inside a word without starting it
        for q in ["s", "l", "ee", "ock", "tart", "ash", "vscode", "photoshop"] {
            assert!(ids(q).is_empty(), "{q:?} offered {:?}", ids(q));
        }
    }

    #[test]
    fn destructive_ones_are_flagged() {
        for (id, d) in [("lock", false), ("sleep", false), ("dark_mode", false), ("restart", true), ("shutdown", true), ("empty_trash", true)] {
            assert_eq!(is_destructive(id), Some(d), "{id}");
        }
        assert_eq!(is_destructive("format_c"), None);
    }

    #[test]
    fn every_command_has_a_mechanism_on_every_platform() {
        for c in COMMANDS {
            for os in [Os::Windows, Os::Mac, Os::Linux] {
                assert!(describe(c.id, os).is_some(), "{} on {os:?}", c.id);
            }
        }
        assert_eq!(describe("lock", Os::Windows).as_deref(), Some("user32 LockWorkStation"));
        assert_eq!(describe("shutdown", Os::Linux).as_deref(), Some("systemctl poweroff"));
        assert!(describe("nope", Os::Mac).is_none());
    }

    #[test]
    fn a_dry_run_reports_and_never_acts() {
        std::env::set_var("MAGPIE_SYSCMD_DRYRUN", "1");
        for c in COMMANDS {
            let out = run(c.id).unwrap();
            assert!(out.starts_with("dry run: "), "{out}");
        }
        assert!(run("nope").is_err());
    }
}
