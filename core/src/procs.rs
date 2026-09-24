//! `kill <name>` in the query box: running processes by name, and ending one.
//!
//! Ending is guarded: the process must still carry the name the palette
//! showed (a PID can be reused between listing and Enter), and magpie itself
//! and the operating system's own processes are refused.

use anyhow::{anyhow, Result};
use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

#[derive(Debug, Clone, Serialize)]
pub struct ProcHit {
    pub pid: u32,
    pub name: String,
    /// resident memory in bytes
    pub memory: u64,
    /// full path of the executable, when the OS lets us read it
    pub exe: Option<String>,
}

fn snapshot() -> System {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_memory().with_exe(UpdateKind::OnlyIfNotSet),
    );
    sys
}

/// Processes whose name contains `query` (case-insensitive), heaviest first.
/// A name that starts with the query ranks ahead of one that merely
/// contains it. Protected processes are left out, so they are never offered.
pub fn find(query: &str, limit: usize) -> Vec<ProcHit> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let sys = snapshot();
    let me = std::process::id();
    let mut hits: Vec<(bool, ProcHit)> = sys
        .processes()
        .values()
        .filter_map(|p| {
            let name = p.name().to_string_lossy().into_owned();
            let lower = name.to_lowercase();
            if !lower.contains(&q) || protected(p.pid().as_u32(), &lower, me) {
                return None;
            }
            Some((
                lower.starts_with(&q),
                ProcHit {
                    pid: p.pid().as_u32(),
                    name,
                    memory: p.memory(),
                    exe: p.exe().map(|e| e.to_string_lossy().into_owned()),
                },
            ))
        })
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.memory.cmp(&a.1.memory)).then(a.1.pid.cmp(&b.1.pid)));
    hits.into_iter().take(limit).map(|(_, h)| h).collect()
}

/// Processes that are never offered or ended: magpie itself, the first
/// PIDs (the kernel, init/launchd, Windows' System), and the handful whose
/// death takes the session down.
fn protected(pid: u32, lower_name: &str, me: u32) -> bool {
    const SESSION: &[&str] = &[
        "csrss.exe", "wininit.exe", "winlogon.exe", "smss.exe", "lsass.exe", "services.exe",
        "system", "registry", "dwm.exe", "launchd", "kernel_task", "windowserver", "loginwindow",
        "systemd", "init",
    ];
    pid == me || pid <= 4 || lower_name.starts_with("magpie") || SESSION.contains(&lower_name)
}

/// End `pid`, provided it is still the process called `name`. Asks it to
/// terminate first where the OS distinguishes (SIGTERM), and waits a moment
/// for it to go.
pub fn end(pid: u32, name: &str) -> Result<()> {
    let sys = snapshot();
    let p = sys
        .process(Pid::from_u32(pid))
        .ok_or_else(|| anyhow!("that process has already ended"))?;
    let current = p.name().to_string_lossy().into_owned();
    if !current.eq_ignore_ascii_case(name) {
        return Err(anyhow!("process {pid} is now {current:?}, not {name:?}; nothing was ended"));
    }
    if protected(pid, &current.to_lowercase(), std::process::id()) {
        return Err(anyhow!("{current} is protected and was not ended"));
    }
    let sent = p.kill_with(sysinfo::Signal::Term).unwrap_or_else(|| p.kill());
    if !sent {
        return Err(anyhow!("could not end {current} ({pid}); it may need administrator rights"));
    }
    // give it a moment, so the palette's refreshed list no longer shows it
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let mut s = System::new();
        s.refresh_processes(ProcessesToUpdate::Some(&[Pid::from_u32(pid)]), true);
        if s.process(Pid::from_u32(pid)).is_none() {
            return Ok(());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A long-running child of this test, under a name no one else uses.
    fn probe() -> (std::process::Child, String) {
        let dir = std::env::temp_dir().join(format!("magpie-procs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        #[cfg(windows)]
        let (src, name, args) = (r"C:\Windows\System32\PING.EXE", "mgprobeping.exe", vec!["-n", "60", "127.0.0.1"]);
        #[cfg(not(windows))]
        let (src, name, args) = ("/bin/sleep", "mgprobesleep", vec!["60"]);
        let exe = dir.join(name);
        std::fs::copy(src, &exe).unwrap();
        let mut cmd = std::process::Command::new(&exe);
        cmd.args(args).stdout(std::process::Stdio::null());
        let child = cmd.spawn().unwrap();
        (child, name.to_string())
    }

    #[test]
    fn find_and_end_a_process_by_name() {
        let (mut child, name) = probe();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let hits = find("mgprobe", 10);
        let hit = hits.iter().find(|h| h.pid == child.id()).unwrap_or_else(|| panic!("not listed: {hits:?}"));
        assert_eq!(hit.name.to_lowercase(), name);
        assert!(hit.memory > 0);
        // a wrong name is refused: the PID might have been reused
        let refused = end(hit.pid, "notepad.exe").unwrap_err().to_string();
        assert!(refused.contains("nothing was ended"), "{refused}");
        assert!(child.try_wait().unwrap().is_none(), "still running after the refusal");
        end(hit.pid, &hit.name).unwrap();
        let status = child.wait().unwrap();
        assert!(!status.success(), "ended, not finished on its own");
        assert!(end(hit.pid, &hit.name).is_err(), "already gone");
    }

    #[test]
    fn magpie_and_the_system_are_never_offered_or_ended() {
        assert!(protected(std::process::id(), "whatever", std::process::id()));
        assert!(protected(4, "system", 1));
        assert!(protected(1234, "winlogon.exe", 1));
        assert!(protected(1234, "magpie.exe", 1));
        assert!(!protected(1234, "chrome.exe", 1));
        // this test binary is not "magpie", but it is itself
        let me = std::process::id();
        assert!(end(me, "anything").is_err());
        assert!(find("", 10).is_empty());
    }
}
