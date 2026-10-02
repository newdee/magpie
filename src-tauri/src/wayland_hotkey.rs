//! The summon key on Wayland (#15). A Wayland desktop owns the keyboard and
//! lets no app grab a key for itself, so the global-shortcut plugin (an X11
//! key grab underneath) never fires there. Instead the desktop is asked to
//! run magpie's toggle when the key is pressed:
//!
//! - sway and Hyprland take bindings at run time over their IPC
//!   (`swaymsg`, `hyprctl`); magpie adds the key and a floating rule for its
//!   window on every start, so nothing is written to the user's config;
//! - niri has no run-time bindings: Settings shows the lines to add;
//! - GNOME, KDE and the rest: Settings shows the command to bind.
//!
//! The toggle asks the running copy over D-Bus (the single-instance
//! plugin's own service, `ExecuteCallback`), which is instant; only when
//! magpie is not running does it start the program with `--toggle`.

// only Linux runs this; elsewhere the tests still check the spelling
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use serde::Serialize;

/// Which desktop holds the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Desktop {
    Sway,
    Hyprland,
    Niri,
    Gnome,
    Kde,
    Other,
}

/// The desktop of a Wayland session, from its environment; None when the
/// session is not Wayland, where the ordinary key grab works.
pub fn detect(env: impl Fn(&str) -> Option<String>) -> Option<Desktop> {
    let wayland = env("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty())
        || env("XDG_SESSION_TYPE").is_some_and(|v| v.eq_ignore_ascii_case("wayland"));
    if !wayland {
        return None;
    }
    let set = |k: &str| env(k).is_some_and(|v| !v.is_empty());
    if set("SWAYSOCK") {
        return Some(Desktop::Sway);
    }
    if set("HYPRLAND_INSTANCE_SIGNATURE") {
        return Some(Desktop::Hyprland);
    }
    if set("NIRI_SOCKET") {
        return Some(Desktop::Niri);
    }
    let current = env("XDG_CURRENT_DESKTOP").unwrap_or_default().to_ascii_uppercase();
    Some(if current.contains("GNOME") {
        Desktop::Gnome
    } else if current.contains("KDE") {
        Desktop::Kde
    } else {
        Desktop::Other
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Super,
}

/// A hotkey as the desktops spell it: modifiers plus an xkb key name.
#[derive(Debug, Clone, PartialEq)]
pub struct Chord {
    mods: Vec<Modifier>,
    key: String,
}

/// Read magpie's own hotkey notation (`Ctrl+Alt+Space`, `Super+K`, `F9`).
/// None for a key with no xkb name here; Settings then shows the command to
/// bind by hand.
pub fn chord(hotkey: &str) -> Option<Chord> {
    let parts: Vec<&str> = hotkey.split('+').map(str::trim).filter(|p| !p.is_empty()).collect();
    let (key, mods) = parts.split_last()?;
    let mut out = Vec::new();
    for m in mods {
        let m = match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "commandorcontrol" | "cmdorctrl" => Modifier::Ctrl,
            "alt" | "option" => Modifier::Alt,
            "shift" => Modifier::Shift,
            "super" | "meta" | "win" | "cmd" | "command" => Modifier::Super,
            _ => return None,
        };
        if !out.contains(&m) {
            out.push(m);
        }
    }
    Some(Chord { mods: out, key: xkb_name(key)? })
}

fn xkb_name(key: &str) -> Option<String> {
    let mut chars = key.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_alphanumeric() {
            return Some(c.to_ascii_lowercase().to_string());
        }
        let name = match c {
            '/' => "slash",
            '\\' => "backslash",
            ';' => "semicolon",
            '\'' => "apostrophe",
            ',' => "comma",
            '.' => "period",
            '-' => "minus",
            '=' => "equal",
            '`' => "grave",
            '[' => "bracketleft",
            ']' => "bracketright",
            _ => return None,
        };
        return Some(name.into());
    }
    if let Some(n) = key.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()) {
        return (1..=24).contains(&n).then(|| format!("F{n}"));
    }
    let name = match key {
        "Space" => "space",
        "Enter" | "Return" => "Return",
        "Tab" => "Tab",
        "Escape" => "Escape",
        "Backspace" => "BackSpace",
        "Delete" => "Delete",
        "Insert" => "Insert",
        "Home" => "Home",
        "End" => "End",
        "PageUp" => "Prior",
        "PageDown" => "Next",
        "ArrowUp" | "Up" => "Up",
        "ArrowDown" | "Down" => "Down",
        "ArrowLeft" | "Left" => "Left",
        "ArrowRight" | "Right" => "Right",
        _ => return None,
    };
    Some(name.into())
}

impl Chord {
    /// sway: `Mod1+Shift+space`.
    pub fn sway(&self) -> String {
        let mut parts: Vec<&str> = self
            .mods
            .iter()
            .map(|m| match m {
                Modifier::Ctrl => "Ctrl",
                Modifier::Alt => "Mod1",
                Modifier::Shift => "Shift",
                Modifier::Super => "Mod4",
            })
            .collect();
        parts.push(&self.key);
        parts.join("+")
    }

    fn hypr_mods(&self) -> Vec<&'static str> {
        self.mods
            .iter()
            .map(|m| match m {
                Modifier::Ctrl => "CTRL",
                Modifier::Alt => "ALT",
                Modifier::Shift => "SHIFT",
                Modifier::Super => "SUPER",
            })
            .collect()
    }

    /// Hyprland's `keyword bind` form: `CTRL ALT,space`.
    pub fn hypr_keyword(&self) -> String {
        format!("{},{}", self.hypr_mods().join(" "), self.key)
    }

    /// Hyprland's Lua form (0.55 and later): `CTRL + ALT + space`.
    pub fn hypr_lua(&self) -> String {
        let mut parts = self.hypr_mods();
        parts.push(&self.key);
        parts.join(" + ")
    }

    /// niri: `Ctrl+Alt+space`.
    pub fn niri(&self) -> String {
        let mut parts: Vec<&str> = self
            .mods
            .iter()
            .map(|m| match m {
                Modifier::Ctrl => "Ctrl",
                Modifier::Alt => "Alt",
                Modifier::Shift => "Shift",
                Modifier::Super => "Super",
            })
            .collect();
        parts.push(&self.key);
        parts.join("+")
    }
}

/// A path the desktops can be handed without any quoting: sway and Hyprland
/// strip or keep quotes by rules of their own, so a path that needs them is
/// left to a binding made by hand.
pub fn plain_path(path: &str) -> bool {
    !path.is_empty()
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._+-".contains(c))
}

/// The shell command a key runs: ask the running magpie over D-Bus to
/// toggle, or start magpie with `--toggle` when it is not running.
/// `dbus-send` ships with D-Bus itself, unlike `gdbus`. The argument list is
/// the flag alone: sway splits commands at commas, and `magpie,--toggle`
/// was cut to `magpie` (see cli::after_program). No quotes either, which
/// the desktops strip or keep by rules of their own.
pub fn toggle_shell(exe: &str) -> String {
    format!(
        "dbus-send --session --print-reply --dest=com.dfine.magpie.SingleInstance \
         /com/dfine/magpie/SingleInstance org.SingleInstance.DBus.ExecuteCallback \
         array:string:--toggle string: >/dev/null 2>&1 || exec {exe} --toggle"
    )
}

/// The command for a desktop's own shortcut settings, which run a program
/// rather than a shell line.
pub fn toggle_command(exe: &str) -> String {
    format!("sh -c \"{}\"", toggle_shell(exe))
}

/// sway commands, in order: float magpie's window and drop its border (by
/// X11 class and by Wayland app id), drop an earlier binding of the key,
/// bind it. One rule per command: a comma would end the `for_window` and
/// run the rest as a command of its own. `--inhibited` keeps the key
/// working over full-screen windows, which hold a keyboard-shortcuts
/// inhibitor.
pub fn sway_commands(chord: &Chord, exe: &str) -> Vec<String> {
    let key = chord.sway();
    let mut cmds = Vec::new();
    for criteria in [r#"[class="^(?i)magpie$"]"#, r#"[app_id="^(?i)(com\.dfine\.)?magpie$"]"#] {
        cmds.push(format!("for_window {criteria} floating enable"));
        cmds.push(format!("for_window {criteria} border none"));
    }
    cmds.push(format!("unbindsym --inhibited {key}"));
    cmds.push(format!("bindsym --inhibited {key} exec {}", toggle_shell(exe)));
    cmds
}

/// The Lua that Hyprland 0.55+ takes through `hyprctl eval`. The unbind is
/// guarded: it fails on a first start, when nothing is bound yet.
pub fn hypr_lua(chord: &Chord, exe: &str) -> String {
    let key = chord.hypr_lua();
    format!(
        r#"pcall(hl.unbind, "{key}")
hl.window_rule({{ name = "magpie-float", match = {{ class = "^(magpie|com\\.dfine\\.magpie)$" }}, float = true }})
hl.bind("{key}", hl.dsp.exec_cmd("{}"), {{ dont_inhibit = true }})"#,
        toggle_shell(exe)
    )
}

/// The `hyprctl keyword` calls for older Hyprland, as argument lists.
/// `bindp` passes the key through over full-screen windows.
pub fn hypr_keywords(chord: &Chord, exe: &str) -> Vec<[String; 3]> {
    let key = chord.hypr_keyword();
    let kw = |a: &str, b: String| ["keyword".to_string(), a.to_string(), b];
    vec![
        kw("unbind", key.clone()),
        kw("windowrulev2", "float, class:^(magpie|com\\.dfine\\.magpie)$".into()),
        kw("bindp", format!("{key},exec,{}", toggle_shell(exe))),
    ]
}

/// The lines for niri's config.kdl. `allow-inhibiting=false` keeps the key
/// working over full-screen windows.
pub fn niri_snippet(chord: &Chord, exe: &str) -> String {
    format!(
        "binds {{\n    {} allow-inhibiting=false {{ spawn \"sh\" \"-c\" \"{}\"; }}\n}}\n\nwindow-rule {{\n    match app-id=\"^(magpie|com\\\\.dfine\\\\.magpie)$\"\n    open-floating true\n}}\n",
        chord.niri(),
        toggle_shell(exe)
    )
}

/// What Settings shows about the summon key on a Wayland session.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub desktop: Desktop,
    /// The key as magpie shows it.
    pub hotkey: String,
    /// magpie bound the key itself (sway, Hyprland).
    pub registered: bool,
    /// The command to bind by hand.
    pub command: String,
    /// Config lines to paste (niri).
    pub snippet: Option<String>,
    /// Why registering did not work, when it was tried.
    pub error: Option<String>,
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::process::Command;
    use std::sync::Mutex;

    static STATUS: Mutex<Option<Status>> = Mutex::new(None);

    pub fn status() -> Option<Status> {
        STATUS.lock().ok()?.clone()
    }

    /// The program a key should start: the AppImage when running from one
    /// (the mounted path inside it changes every start), else this binary.
    fn exe() -> String {
        std::env::var("APPIMAGE")
            .ok()
            .filter(|p| !p.is_empty())
            .or_else(|| std::env::current_exe().ok().map(|p| p.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "magpie".into())
    }

    fn run(program: &str, args: &[&str]) -> Result<String, String> {
        let out = Command::new(program).args(args).output().map_err(|e| format!("{program}: {e}"))?;
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        if out.status.success() {
            Ok(text)
        } else {
            Err(format!("{program}: {}", text.trim()))
        }
    }

    /// hyprctl can exit 0 and still report a rejected command in its output.
    fn hypr_ok(text: &str) -> bool {
        let t = text.to_ascii_lowercase();
        !t.contains("error") && !t.contains("unknown") && !t.contains("invalid")
    }

    fn register_sway(chord: &Chord, exe: &str) -> Result<(), String> {
        let cmds = sway_commands(chord, exe);
        let (setup, bind) = cmds.split_at(cmds.len() - 1);
        for c in setup {
            // a missing earlier binding makes the unbind fail: not an error
            if let Err(e) = run("swaymsg", &[c]) {
                log::info!("wayland hotkey: {c}: {e}");
            }
        }
        run("swaymsg", &[&bind[0]]).map(|_| ())
    }

    fn register_hyprland(chord: &Chord, exe: &str) -> Result<(), String> {
        let lua = hypr_lua(chord, exe);
        match run("hyprctl", &["eval", &lua]) {
            Ok(t) if hypr_ok(&t) => return Ok(()),
            Ok(t) | Err(t) => log::info!("wayland hotkey: hyprctl eval: {}", t.trim()),
        }
        let calls = hypr_keywords(chord, exe);
        let (setup, bind) = calls.split_at(calls.len() - 1);
        for c in setup {
            let _ = run("hyprctl", &[&c[0], &c[1], &c[2]]);
        }
        let b = &bind[0];
        match run("hyprctl", &[&b[0], &b[1], &b[2]]) {
            Ok(t) if hypr_ok(&t) => Ok(()),
            Ok(t) | Err(t) => Err(t.trim().to_string()),
        }
    }

    /// Bind `hotkey` on a Wayland session, or work out what to show for
    /// binding it by hand. Does nothing off Wayland. Runs the desktop's
    /// tools, so call it off the main thread.
    pub fn apply(hotkey: &str) {
        let Some(desktop) = detect(|k| std::env::var(k).ok()) else {
            return;
        };
        let exe = exe();
        // a key magpie bound earlier in this session goes when it changes
        let previous = status().filter(|s| s.registered && s.hotkey != hotkey);
        if let Some(p) = previous.and_then(|p| chord(&p.hotkey)) {
            match desktop {
                Desktop::Sway => {
                    let _ = run("swaymsg", &[&format!("unbindsym --inhibited {}", p.sway())]);
                }
                Desktop::Hyprland => {
                    let lua = format!("pcall(hl.unbind, \"{}\")", p.hypr_lua());
                    if !run("hyprctl", &["eval", &lua]).is_ok_and(|t| hypr_ok(&t)) {
                        let _ = run("hyprctl", &["keyword", "unbind", &p.hypr_keyword()]);
                    }
                }
                _ => {}
            }
        }
        let chord = chord(hotkey);
        let mut status = Status {
            desktop,
            hotkey: hotkey.to_string(),
            registered: false,
            command: toggle_command(&exe),
            snippet: None,
            error: None,
        };
        match (&chord, desktop) {
            (Some(c), Desktop::Sway | Desktop::Hyprland) if plain_path(&exe) => {
                let result = if desktop == Desktop::Sway {
                    register_sway(c, &exe)
                } else {
                    register_hyprland(c, &exe)
                };
                match result {
                    Ok(()) => {
                        status.registered = true;
                        log::info!("wayland hotkey: {hotkey} bound through {desktop:?}");
                    }
                    Err(e) => {
                        log::warn!("wayland hotkey: binding {hotkey} through {desktop:?} failed: {e}");
                        status.error = Some(e);
                    }
                }
            }
            (Some(c), Desktop::Niri) => status.snippet = Some(niri_snippet(c, &exe)),
            _ => {}
        }
        if let Ok(mut s) = STATUS.lock() {
            *s = Some(status);
        }
    }
}

#[cfg(target_os = "linux")]
pub use imp::{apply, status};

#[cfg(not(target_os = "linux"))]
pub fn status() -> Option<Status> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
    }

    #[test]
    fn detects_the_desktop_only_on_wayland() {
        assert_eq!(detect(env_of(&[("DISPLAY", ":0")])), None);
        assert_eq!(detect(env_of(&[("WAYLAND_DISPLAY", "wayland-1"), ("SWAYSOCK", "/run/sway")])), Some(Desktop::Sway));
        assert_eq!(
            detect(env_of(&[("XDG_SESSION_TYPE", "wayland"), ("HYPRLAND_INSTANCE_SIGNATURE", "x")])),
            Some(Desktop::Hyprland)
        );
        assert_eq!(detect(env_of(&[("WAYLAND_DISPLAY", "w"), ("NIRI_SOCKET", "/run/niri")])), Some(Desktop::Niri));
        assert_eq!(
            detect(env_of(&[("WAYLAND_DISPLAY", "w"), ("XDG_CURRENT_DESKTOP", "ubuntu:GNOME")])),
            Some(Desktop::Gnome)
        );
        assert_eq!(detect(env_of(&[("WAYLAND_DISPLAY", "w"), ("XDG_CURRENT_DESKTOP", "KDE")])), Some(Desktop::Kde));
        assert_eq!(detect(env_of(&[("WAYLAND_DISPLAY", "w")])), Some(Desktop::Other));
        // an X11 session that happens to export an empty WAYLAND_DISPLAY
        assert_eq!(detect(env_of(&[("WAYLAND_DISPLAY", "")])), None);
    }

    #[test]
    fn spells_the_key_for_each_desktop() {
        let c = chord("Alt+Space").unwrap();
        assert_eq!(c.sway(), "Mod1+space");
        assert_eq!(c.hypr_keyword(), "ALT,space");
        assert_eq!(c.hypr_lua(), "ALT + space");
        assert_eq!(c.niri(), "Alt+space");
        let c = chord("Ctrl+Shift+Super+K").unwrap();
        assert_eq!(c.sway(), "Ctrl+Shift+Mod4+k");
        assert_eq!(c.hypr_keyword(), "CTRL SHIFT SUPER,k");
        assert_eq!(chord("Ctrl+/").unwrap().sway(), "Ctrl+slash");
        assert_eq!(chord("F9").unwrap().sway(), "F9");
        assert_eq!(chord("Alt+PageDown").unwrap().niri(), "Alt+Next");
        // nothing the desktops could take
        assert_eq!(chord("Alt+Ü"), None);
        assert_eq!(chord("Hyper+K"), None);
        assert_eq!(chord("F30"), None);
        assert_eq!(chord(""), None);
    }

    #[test]
    fn commands_reach_the_running_copy_first() {
        let exe = "/home/a/Apps/magpie_0.5.5_amd64.AppImage";
        let sh = toggle_shell(exe);
        assert!(sh.starts_with("dbus-send --session --print-reply --dest=com.dfine.magpie.SingleInstance"));
        assert!(sh.ends_with(&format!("|| exec {exe} --toggle")));
        assert!(!sh.contains('"') && !sh.contains('\''), "no quotes for the desktops to mangle");
        assert!(sh.contains("array:string:--toggle string:"), "the flag alone, no program name");
        let c = chord("Alt+Space").unwrap();
        let sway = sway_commands(&c, exe);
        assert_eq!(sway.last().unwrap(), &format!("bindsym --inhibited Mod1+space exec {sh}"));
        assert_eq!(sway[sway.len() - 2], "unbindsym --inhibited Mod1+space");
        // sway ends a command at a comma or a semicolon (found on a real sway
        // 1.7: `magpie,--toggle` was bound as `magpie`)
        for cmd in &sway {
            assert!(!cmd.contains(',') && !cmd.contains(';'), "{cmd}");
        }
        // Hyprland's bind splits its fields at commas: none past the dispatcher
        assert!(!hypr_keywords(&c, exe)[2][2].splitn(4, ',').nth(3).unwrap().contains(','));
        let lua = hypr_lua(&c, exe);
        assert!(lua.contains(r#"hl.bind("ALT + space", hl.dsp.exec_cmd(""#));
        assert_eq!(hypr_keywords(&c, exe)[2][1], "bindp");
        let niri = niri_snippet(&c, exe);
        assert!(niri.contains("Alt+space allow-inhibiting=false { spawn \"sh\" \"-c\" \"dbus-send"));
        assert_eq!(toggle_command(exe), format!("sh -c \"{sh}\""));
    }

    #[test]
    fn only_plain_paths_are_handed_over() {
        assert!(plain_path("/opt/magpie/magpie"));
        assert!(plain_path("/home/a/magpie_0.5.5_amd64.AppImage"));
        assert!(!plain_path("/home/a/My Apps/magpie.AppImage"));
        assert!(!plain_path("/home/a/it's/magpie"));
        assert!(!plain_path(""));
    }
}
