//! `kill <name>` in the query box: running processes by name, and ending one.
//! `port 3000` (or `kill :3000`): the processes listening on a port.
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
    /// for a port search: the sockets it listens on ("TCP 0.0.0.0:3000")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listen: Option<String>,
    /// `cpu`: share of the whole machine's CPU, 0–100
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu: Option<f32>,
    /// `top` / `cpu`: the processes sharing this name (a browser runs dozens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<Group>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Group {
    pub count: usize,
    pub memory: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopBy {
    Memory,
    Cpu,
}

/// `top` / `内存` / `cpu`: what weighs on the machine right now, heaviest
/// first. One row per process (Enter ends that one), each carrying the total
/// of every process of the same name, so twenty browser helpers of 200 MB
/// are not mistaken for something small. CPU is sampled over a short
/// interval (sysinfo needs two readings), then shown as a share of all cores.
pub fn top(by: TopBy, limit: usize) -> Vec<ProcHit> {
    let mut sys = System::new();
    let kind = ProcessRefreshKind::nothing().with_memory().with_cpu().with_exe(UpdateKind::OnlyIfNotSet);
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
    if by == TopBy::Cpu {
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(std::time::Duration::from_millis(250)));
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
    }
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f32;
    let me = std::process::id();
    let ours = ours(&sys);
    let procs: Vec<&sysinfo::Process> = sys
        .processes()
        .values()
        .filter(|p| {
            !ours.contains(&p.pid().as_u32()) && !protected(p.pid().as_u32(), &p.name().to_string_lossy().to_lowercase(), me)
        })
        .collect();
    let mut groups: std::collections::HashMap<String, (usize, u64)> = Default::default();
    for p in &procs {
        let g = groups.entry(p.name().to_string_lossy().to_lowercase()).or_default();
        g.0 += 1;
        g.1 += p.memory();
    }
    let mut hits: Vec<ProcHit> = procs
        .iter()
        .map(|p| {
            let name = p.name().to_string_lossy().into_owned();
            let g = groups[&name.to_lowercase()];
            ProcHit {
                pid: p.pid().as_u32(),
                memory: p.memory(),
                exe: p.exe().map(|e| e.to_string_lossy().into_owned()),
                listen: None,
                cpu: (by == TopBy::Cpu).then(|| (p.cpu_usage() / cores).clamp(0.0, 100.0)),
                group: (g.0 > 1).then_some(Group { count: g.0, memory: g.1 }),
                name,
            }
        })
        .collect();
    match by {
        TopBy::Memory => hits.sort_by(|a, b| b.memory.cmp(&a.memory).then(a.pid.cmp(&b.pid))),
        TopBy::Cpu => hits.sort_by(|a, b| {
            b.cpu.unwrap_or(0.0).total_cmp(&a.cpu.unwrap_or(0.0)).then(b.memory.cmp(&a.memory)).then(a.pid.cmp(&b.pid))
        }),
    }
    hits.truncate(limit);
    hits
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
    let ours = ours(&sys);
    let mut hits: Vec<(bool, ProcHit)> = sys
        .processes()
        .values()
        .filter_map(|p| {
            let name = p.name().to_string_lossy().into_owned();
            let lower = name.to_lowercase();
            if !lower.contains(&q) || ours.contains(&p.pid().as_u32()) || protected(p.pid().as_u32(), &lower, me) {
                return None;
            }
            Some((
                lower.starts_with(&q),
                ProcHit {
                    pid: p.pid().as_u32(),
                    name,
                    memory: p.memory(),
                    exe: p.exe().map(|e| e.to_string_lossy().into_owned()),
                    listen: None,
                    cpu: None,
                    group: None,
                },
            ))
        })
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.memory.cmp(&a.1.memory)).then(a.1.pid.cmp(&b.1.pid)));
    hits.into_iter().take(limit).map(|(_, h)| h).collect()
}

/// The processes listening on `port`: TCP sockets in the listening state and
/// UDP sockets bound to it, one row per process however many sockets it
/// holds (IPv4 and IPv6, several addresses). Protected processes are left
/// out, the same as for `find`.
pub fn on_port(port: u16) -> Result<Vec<ProcHit>> {
    let me = std::process::id();
    let sys = snapshot();
    let ours = ours(&sys);
    let mut hits: Vec<ProcHit> = listening(port)?
        .into_iter()
        .filter_map(|(pid, fallback_name, sockets)| {
            let p = sys.process(Pid::from_u32(pid));
            let name = p.map(|p| p.name().to_string_lossy().into_owned()).unwrap_or(fallback_name);
            if ours.contains(&pid) || protected(pid, &name.to_lowercase(), me) {
                return None;
            }
            Some(ProcHit {
                pid,
                name,
                memory: p.map(|p| p.memory()).unwrap_or(0),
                exe: p.and_then(|p| p.exe()).map(|e| e.to_string_lossy().into_owned()),
                listen: Some(sockets.join(", ")),
                cpu: None,
                group: None,
            })
        })
        .collect();
    hits.sort_by(|a, b| b.memory.cmp(&a.memory).then(a.pid.cmp(&b.pid)));
    Ok(hits)
}

/// Every process with a socket listening on `port`, unfiltered: (pid, name
/// the socket table gave, sockets as "TCP 0.0.0.0:3000"), in PID order.
fn listening(port: u16) -> Result<Vec<(u32, String, Vec<String>)>> {
    use listeners::{Protocol, SocketState};
    let mut by_pid: std::collections::BTreeMap<u32, (String, Vec<String>)> = Default::default();
    for l in listeners::get_all().map_err(|e| anyhow!("could not read the socket table: {e}"))? {
        if l.socket.port() != port {
            continue;
        }
        let proto = match l.protocol {
            Protocol::TCP if l.state == SocketState::Listen => "TCP",
            Protocol::UDP => "UDP",
            _ => continue, // an open connection, not a listener
        };
        let entry = by_pid.entry(l.process.pid).or_insert_with(|| (l.process.name.clone(), Vec::new()));
        let socket = format!("{proto} {}", l.socket);
        if !entry.1.contains(&socket) {
            entry.1.push(socket);
        }
    }
    Ok(by_pid.into_iter().map(|(pid, (name, mut sockets))| {
        sockets.sort();
        (pid, name, sockets)
    }).collect())
}

#[cfg(test)]
thread_local! {
    /// Tests run their probes as children of the test process: the
    /// descendant rule below applies only where a test turns it on.
    static GUARD_CHILDREN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// magpie's own processes: itself and everything it started, down the tree
/// (its WebView2 or WebKit processes). Ending one takes the window down, so
/// they are protected like magpie. A "child" that started before its parent
/// holds a reused PID and is not ours.
fn ours(sys: &System) -> std::collections::HashSet<u32> {
    let me = std::process::id();
    let mut ours = std::collections::HashSet::from([me]);
    #[cfg(test)]
    if !GUARD_CHILDREN.with(|g| g.get()) {
        return ours;
    }
    loop {
        let before = ours.len();
        for p in sys.processes().values() {
            let pid = p.pid().as_u32();
            if ours.contains(&pid) {
                continue;
            }
            if let Some(parent) = p.parent().and_then(|pp| sys.process(pp)) {
                if ours.contains(&parent.pid().as_u32()) && p.start_time() >= parent.start_time() {
                    ours.insert(pid);
                }
            }
        }
        if ours.len() == before {
            return ours;
        }
    }
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
    if ours(&sys).contains(&pid) || protected(pid, &current.to_lowercase(), std::process::id()) {
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
        probe_in("")
    }

    /// The same probe from its own folder: a second one runs under the same
    /// name while the first still holds its file.
    fn probe_in(tag: &str) -> (std::process::Child, String) {
        let dir = std::env::temp_dir().join(format!("magpie-procs-{}{tag}", std::process::id()));
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

    /// What magpie started (its WebView2 processes, and theirs) is magpie:
    /// never listed by top / find, never ended. Here the test process plays
    /// magpie, a probe is its child and another its grandchild via a shell.
    #[test]
    fn our_own_children_and_grandchildren_are_never_offered_or_ended() {
        let (mut child, name) = probe_in("-ours-a");
        let (mut seed, _) = probe_in("-ours-b");
        let exe = std::env::temp_dir().join(format!("magpie-procs-{}-ours-b", std::process::id())).join(&name);
        let _ = (seed.kill(), seed.wait()); // only its copied file is wanted
        #[cfg(windows)]
        let mut shell = std::process::Command::new("cmd")
            .args(["/c", &exe.to_string_lossy(), "-n", "60", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        #[cfg(not(windows))]
        let mut shell = std::process::Command::new("sh").args(["-c", &format!("'{}' 60; true", exe.display())]).spawn().unwrap(); // "; true": no exec in place
        std::thread::sleep(std::time::Duration::from_millis(500));
        let listed = |on: bool| {
            GUARD_CHILDREN.with(|g| g.set(on));
            let pids: Vec<u32> = find(&name, 50).iter().map(|h| h.pid).collect();
            let top_pids: Vec<u32> = top(TopBy::Memory, 10_000).iter().filter(|h| h.name.to_lowercase() == name).map(|h| h.pid).collect();
            GUARD_CHILDREN.with(|g| g.set(false));
            (pids, top_pids)
        };
        // off: both probes are visible, so the rule below is what hides them
        let (all, all_top) = listed(false);
        // the probe the shell started (other tests run probes of this name too)
        let grandchild = snapshot()
            .processes()
            .values()
            .find(|p| p.parent().map(|pp| pp.as_u32()) == Some(shell.id()) && p.name().to_string_lossy().to_lowercase() == name)
            .map(|p| p.pid().as_u32())
            .expect("the grandchild runs");
        assert!(all.contains(&grandchild));
        assert!(all.contains(&child.id()) && all_top.contains(&child.id()) && all_top.contains(&grandchild));
        let (pids, top_pids) = listed(true);
        assert!(!pids.contains(&child.id()) && !pids.contains(&grandchild), "find: {pids:?}");
        assert!(!top_pids.contains(&child.id()) && !top_pids.contains(&grandchild), "top: {top_pids:?}");
        GUARD_CHILDREN.with(|g| g.set(true));
        let refused = end(child.id(), &name).unwrap_err().to_string();
        GUARD_CHILDREN.with(|g| g.set(false));
        assert!(refused.contains("protected"), "{refused}");
        assert!(child.try_wait().unwrap().is_none(), "still running after the refusal");
        let _ = end(grandchild, &name);
        let _ = (child.kill(), shell.kill());
        let _ = (child.wait(), shell.wait());
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
    fn top_lists_the_heaviest_first_with_name_totals_and_never_itself() {
        let (mut a, name) = probe_in("-top-a");
        let (mut b, _) = probe_in("-top-b"); // two of the same name: a group
        std::thread::sleep(std::time::Duration::from_millis(300));
        let mem = top(TopBy::Memory, 1000);
        assert!(mem.windows(2).all(|w| w[0].memory >= w[1].memory), "by memory, heaviest first");
        assert!(mem.iter().all(|h| h.pid != std::process::id() && h.cpu.is_none()), "never itself; no CPU column");
        // by PID: a parallel test may run a probe of the same name too
        let mine: Vec<&ProcHit> = mem.iter().filter(|h| h.pid == a.id() || h.pid == b.id()).collect();
        assert_eq!(mine.len(), 2, "both probes listed");
        assert_eq!(mine[0].name.to_lowercase(), name);
        let g = mine[0].group.as_ref().expect("a group of at least two");
        let same: Vec<&ProcHit> = mem.iter().filter(|h| h.name.to_lowercase() == name).collect();
        assert_eq!(g.count, same.len());
        assert_eq!(g.memory, same.iter().map(|h| h.memory).sum::<u64>(), "the total of every process of that name");
        let cpu = top(TopBy::Cpu, 5);
        assert!(cpu.len() <= 5 && cpu.iter().all(|h| h.cpu.is_some_and(|c| (0.0..=100.0).contains(&c))));
        assert!(cpu.windows(2).all(|w| w[0].cpu >= w[1].cpu), "by CPU");
        let _ = a.kill();
        let _ = b.kill();
        let _ = (a.wait(), b.wait());
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

    /// A port this test listens on is found, with this process behind it;
    /// `on_port` then leaves it out, since a process never offers itself
    /// (the same guard that keeps magpie off the list). Once closed, it is gone.
    #[test]
    fn finds_who_listens_on_a_port() {
        let tcp = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = tcp.local_addr().unwrap().port();
        let udp = std::net::UdpSocket::bind(("127.0.0.1", port)).ok(); // same number, UDP too
        let found = listening(port).unwrap();
        let mine = found.iter().find(|(pid, _, _)| *pid == std::process::id());
        let (_, _, sockets) = mine.unwrap_or_else(|| panic!("this process not found on {port}: {found:?}"));
        assert!(sockets.contains(&format!("TCP 127.0.0.1:{port}")), "{sockets:?}");
        if udp.is_some() {
            assert!(sockets.contains(&format!("UDP 127.0.0.1:{port}")), "{sockets:?}");
        }
        assert!(on_port(port).unwrap().iter().all(|h| h.pid != std::process::id()), "itself is protected");
        drop(tcp);
        drop(udp);
        assert!(listening(port).unwrap().iter().all(|(pid, _, _)| *pid != std::process::id()), "closed");
    }
}
