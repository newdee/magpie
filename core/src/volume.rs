//! `vol`, `vol 40`, `vol +10`, `mute`: the system output volume.
//!
//! Windows talks to the default render endpoint through Core Audio, macOS
//! goes through `osascript`, Linux through `pactl` (PulseAudio or PipeWire's
//! pulse server). Setting honours `MAGPIE_SYSCMD_DRYRUN` like the system
//! commands: tests never change what the user hears.

use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Volume {
    /// 0–100
    pub level: u8,
    pub muted: bool,
}

pub fn get() -> Result<Volume> {
    imp::get()
}

fn dry_run() -> bool {
    std::env::var_os("MAGPIE_SYSCMD_DRYRUN").is_some()
}

/// Set the output level (clamped to 0–100). Returns what was done.
pub fn set_level(level: u8) -> Result<String> {
    let level = level.min(100);
    if dry_run() {
        return Ok(format!("dry run: volume {level}%"));
    }
    imp::set_level(level)?;
    Ok(format!("volume {level}%"))
}

pub fn set_muted(muted: bool) -> Result<String> {
    if dry_run() {
        return Ok(format!("dry run: {}", if muted { "mute" } else { "unmute" }));
    }
    imp::set_muted(muted)?;
    Ok(if muted { "muted" } else { "unmuted" }.to_string())
}

/// `40` → 40, `+10` / `-10` → relative to `now`, clamped to 0–100.
pub fn parse_target(arg: &str, now: u8) -> Option<u8> {
    let a = arg.trim().trim_end_matches('%');
    let signed = a.starts_with('+') || a.starts_with('-');
    let n: i32 = a.parse().ok()?;
    let v = if signed { now as i32 + n } else { n };
    if !signed && !(0..=100).contains(&n) {
        return None;
    }
    Some(v.clamp(0, 100) as u8)
}

#[cfg(windows)]
mod imp {
    use super::Volume;
    use anyhow::{anyhow, Result};
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};

    /// Core Audio on a thread of its own, so COM is initialised the way we
    /// want whatever the caller's thread already did.
    fn with_endpoint<T: Send + 'static>(f: impl FnOnce(&IAudioEndpointVolume) -> Result<T> + Send + 'static) -> Result<T> {
        std::thread::spawn(move || {
            // SAFETY: plain COM calls on a thread we own; uninitialised on exit
            unsafe {
                CoInitializeEx(None, COINIT_MULTITHREADED).ok().map_err(|e| anyhow!("COM: {e}"))?;
                let r = (|| {
                    let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
                    let dev = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
                    let vol: IAudioEndpointVolume = dev.Activate(CLSCTX_ALL, None)?;
                    Ok::<_, windows::core::Error>(vol)
                })()
                .map_err(|e| anyhow!("no audio output device: {e}"))
                .and_then(|v| f(&v));
                CoUninitialize();
                r
            }
        })
        .join()
        .map_err(|_| anyhow!("audio thread panicked"))?
    }

    pub fn get() -> Result<Volume> {
        with_endpoint(|v| {
            // SAFETY: reads on a live endpoint
            let level = unsafe { v.GetMasterVolumeLevelScalar()? };
            let muted = unsafe { v.GetMute()? }.as_bool();
            Ok(Volume { level: (level * 100.0).round() as u8, muted })
        })
    }

    pub fn set_level(level: u8) -> Result<()> {
        with_endpoint(move |v| {
            // SAFETY: a level in 0.0–1.0, no event context
            unsafe { v.SetMasterVolumeLevelScalar(level as f32 / 100.0, std::ptr::null())? };
            Ok(())
        })
    }

    pub fn set_muted(muted: bool) -> Result<()> {
        with_endpoint(move |v| {
            // SAFETY: as above
            unsafe { v.SetMute(muted, std::ptr::null())? };
            Ok(())
        })
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::Volume;
    use anyhow::{anyhow, Result};

    fn osascript(script: &str) -> Result<String> {
        let out = std::process::Command::new("osascript").args(["-e", script]).output()?;
        if !out.status.success() {
            return Err(anyhow!("osascript: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    pub fn get() -> Result<Volume> {
        // "40, true": output volume and output muted
        let s = osascript("set v to get volume settings\nreturn ((output volume of v) as string) & \",\" & ((output muted of v) as string)")?;
        let (level, muted) = s.split_once(',').ok_or_else(|| anyhow!("unexpected volume settings {s:?}"))?;
        Ok(Volume { level: level.trim().parse().map_err(|_| anyhow!("no output volume ({s})"))?, muted: muted.trim() == "true" })
    }

    pub fn set_level(level: u8) -> Result<()> {
        osascript(&format!("set volume output volume {level}")).map(|_| ())
    }

    pub fn set_muted(muted: bool) -> Result<()> {
        osascript(&format!("set volume output muted {muted}")).map(|_| ())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::Volume;
    use anyhow::{anyhow, Result};

    fn pactl(args: &[&str]) -> Result<String> {
        let out = std::process::Command::new("pactl")
            .args(args)
            // untranslated output: "Mute: yes", not "静音：是"
            .env("LC_ALL", "C")
            .output()
            .map_err(|e| anyhow!("pactl (PulseAudio / PipeWire) not available: {e}"))?;
        if !out.status.success() {
            return Err(anyhow!("pactl: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    pub fn get() -> Result<Volume> {
        let v = pactl(&["get-sink-volume", "@DEFAULT_SINK@"])?;
        let level = v
            .split_whitespace()
            .find_map(|t| t.strip_suffix('%').and_then(|n| n.parse::<u32>().ok()))
            .ok_or_else(|| anyhow!("no volume in {v:?}"))?;
        let m = pactl(&["get-sink-mute", "@DEFAULT_SINK@"])?;
        Ok(Volume { level: level.min(100) as u8, muted: m.contains("yes") })
    }

    pub fn set_level(level: u8) -> Result<()> {
        pactl(&["set-sink-volume", "@DEFAULT_SINK@", &format!("{level}%")]).map(|_| ())
    }

    pub fn set_muted(muted: bool) -> Result<()> {
        pactl(&["set-sink-mute", "@DEFAULT_SINK@", if muted { "1" } else { "0" }]).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_absolute_relative_and_clamped() {
        assert_eq!(parse_target("40", 10), Some(40));
        assert_eq!(parse_target("40%", 10), Some(40));
        assert_eq!(parse_target("+10", 35), Some(45));
        assert_eq!(parse_target("-50", 35), Some(0));
        assert_eq!(parse_target("+90", 35), Some(100));
        assert_eq!(parse_target("0", 35), Some(0));
        assert_eq!(parse_target("100", 35), Some(100));
        for bad in ["101", "abc", "", "4 0", "-"] {
            assert_eq!(parse_target(bad, 35), None, "{bad:?}");
        }
    }

    /// Needs a real output device (CI has none): `--ignored` to run. Reads,
    /// then sets the level and mute to what they already are, so nothing
    /// the user hears changes, and reads back.
    #[test]
    #[ignore]
    fn reads_and_sets_the_real_volume_without_changing_it() {
        let v = get().unwrap();
        assert!(v.level <= 100);
        std::env::remove_var("MAGPIE_SYSCMD_DRYRUN");
        imp::set_level(v.level).unwrap();
        imp::set_muted(v.muted).unwrap();
        assert_eq!(get().unwrap(), v);
        eprintln!("VOLUME {v:?}");
    }

    #[test]
    fn setting_in_a_dry_run_changes_nothing() {
        std::env::set_var("MAGPIE_SYSCMD_DRYRUN", "1");
        assert_eq!(set_level(40).unwrap(), "dry run: volume 40%");
        assert_eq!(set_level(250).unwrap(), "dry run: volume 100%");
        assert_eq!(set_muted(true).unwrap(), "dry run: mute");
    }
}
