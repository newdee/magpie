//! File changes without waiting for the next full walk. The watcher (notify,
//! wired up in lib.rs) pushes paths here; a ticker takes them once the disk
//! has been quiet for a moment and hands them to a scoped re-index
//! (`files::index_changed`). A burst too big to handle path by path, or an
//! overflow the OS reports, turns into one full walk instead. The periodic
//! full walk stays as the reconciliation for whatever a watcher can miss.
//!
//! Pure: no watcher, no clock of its own, so every rule here is testable.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Events the index can never care about: anything under a dotfile
/// component below its folder root (`.git/objects/...` most of all). The
/// full walk skips those, so they would be dropped later anyway; dropping
/// them here keeps a `git checkout` from filling the queue past
/// [`MAX_PATHS`] and forcing a full walk for nothing. A dot component in
/// the root itself does not count: the user chose that folder.
pub fn is_noise(path: &Path, roots: &[PathBuf]) -> bool {
    let Some(root) = roots.iter().filter(|r| path.starts_with(r)).max_by_key(|r| r.as_os_str().len()) else {
        return true; // outside every folder: nothing to index
    };
    path.strip_prefix(root)
        .map(|rel| rel.components().any(|c| c.as_os_str().to_string_lossy().starts_with('.')))
        .unwrap_or(true)
}

/// How long the disk has to stay quiet before a batch is taken: an editor
/// saves a file in several steps, a `git checkout` touches hundreds.
pub const QUIET: Duration = Duration::from_millis(2000);
/// Above this many distinct paths one full walk is cheaper than scopes.
pub const MAX_PATHS: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Batch {
    /// Walk every folder (an overflow, or a burst past `MAX_PATHS`).
    All,
    /// Re-index just these, sorted and deduplicated.
    Paths(Vec<PathBuf>),
}

/// What has changed since the last batch was taken.
#[derive(Debug, Default)]
pub struct Pending {
    paths: HashSet<PathBuf>,
    everything: bool,
    last_event: Option<Instant>,
}

impl Pending {
    pub fn push(&mut self, path: PathBuf, now: Instant) {
        if !self.everything {
            self.paths.insert(path);
            if self.paths.len() > MAX_PATHS {
                self.everything = true;
                self.paths.clear();
            }
        }
        self.last_event = Some(now);
    }

    /// The OS lost events (or a caller wants the full walk): forget the
    /// paths, the next batch walks everything.
    pub fn rescan_all(&mut self, now: Instant) {
        self.everything = true;
        self.paths.clear();
        self.last_event = Some(now);
    }

    pub fn is_empty(&self) -> bool {
        !self.everything && self.paths.is_empty()
    }

    /// The batch, once the disk has been quiet for [`QUIET`]; `None` while
    /// events keep arriving or nothing is pending.
    pub fn take_if_quiet(&mut self, now: Instant) -> Option<Batch> {
        if self.is_empty() {
            return None;
        }
        let last = self.last_event?;
        if now.saturating_duration_since(last) < QUIET {
            return None;
        }
        let batch = if self.everything {
            Batch::All
        } else {
            let mut v: Vec<PathBuf> = self.paths.drain().collect();
            v.sort();
            Batch::Paths(v)
        };
        self.everything = false;
        self.paths.clear();
        self.last_event = None;
        Some(batch)
    }

    /// The indexer was busy: queue the batch again, behind whatever arrived
    /// meanwhile, for the next quiet moment.
    pub fn put_back(&mut self, batch: Batch, now: Instant) {
        match batch {
            Batch::All => self.rescan_all(now),
            Batch::Paths(paths) => {
                for p in paths {
                    self.push(p, now);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn a_batch_waits_for_the_disk_to_go_quiet() {
        let t0 = Instant::now();
        let mut pending = Pending::default();
        pending.push(p("C:/x/a.txt"), t0);
        assert_eq!(pending.take_if_quiet(t0), None, "just arrived");
        assert_eq!(pending.take_if_quiet(t0 + Duration::from_millis(1500)), None, "still writing");
        // another event resets the quiet timer
        pending.push(p("C:/x/b.txt"), t0 + Duration::from_millis(1500));
        assert_eq!(pending.take_if_quiet(t0 + Duration::from_millis(2500)), None);
        let batch = pending.take_if_quiet(t0 + Duration::from_millis(3600));
        assert_eq!(batch, Some(Batch::Paths(vec![p("C:/x/a.txt"), p("C:/x/b.txt")])));
        assert!(pending.is_empty(), "taken means gone");
        assert_eq!(pending.take_if_quiet(t0 + Duration::from_secs(10)), None);
    }

    #[test]
    fn paths_are_deduplicated_and_sorted() {
        let t0 = Instant::now();
        let mut pending = Pending::default();
        for s in ["C:/x/b.txt", "C:/x/a.txt", "C:/x/b.txt", "C:/x/a.txt"] {
            pending.push(p(s), t0);
        }
        let batch = pending.take_if_quiet(t0 + QUIET).unwrap();
        assert_eq!(batch, Batch::Paths(vec![p("C:/x/a.txt"), p("C:/x/b.txt")]));
    }

    #[test]
    fn a_burst_past_the_cap_becomes_one_full_walk() {
        let t0 = Instant::now();
        let mut pending = Pending::default();
        for i in 0..=MAX_PATHS {
            pending.push(p(&format!("C:/x/{i}.txt")), t0);
        }
        assert_eq!(pending.take_if_quiet(t0 + QUIET), Some(Batch::All));
        // and a path arriving after the overflow is covered by that walk
        let mut pending = Pending::default();
        pending.rescan_all(t0);
        pending.push(p("C:/x/late.txt"), t0);
        assert_eq!(pending.take_if_quiet(t0 + QUIET), Some(Batch::All));
    }

    #[test]
    fn a_batch_put_back_queues_behind_new_events() {
        let t0 = Instant::now();
        let mut pending = Pending::default();
        pending.push(p("C:/x/a.txt"), t0);
        let batch = pending.take_if_quiet(t0 + QUIET).unwrap();
        // the indexer was busy; meanwhile b changed
        pending.push(p("C:/x/b.txt"), t0 + QUIET);
        pending.put_back(batch, t0 + QUIET);
        assert_eq!(pending.take_if_quiet(t0 + QUIET), None, "quiet timer restarted");
        assert_eq!(
            pending.take_if_quiet(t0 + QUIET + QUIET),
            Some(Batch::Paths(vec![p("C:/x/a.txt"), p("C:/x/b.txt")]))
        );
        // a full walk put back stays a full walk
        let mut pending = Pending::default();
        pending.put_back(Batch::All, t0);
        assert_eq!(pending.take_if_quiet(t0 + QUIET), Some(Batch::All));
    }

    #[test]
    fn noise_is_dotfiles_below_a_root_and_anything_outside() {
        let roots = vec![p("C:/work/proj"), p("C:/Users/me/.config/notes")];
        assert!(is_noise(&p("C:/work/proj/.git/objects/ab/cdef"), &roots), ".git churn");
        assert!(is_noise(&p("C:/work/proj/src/.hidden.txt"), &roots), "a dotfile itself");
        assert!(!is_noise(&p("C:/work/proj/src/main.rs"), &roots));
        assert!(!is_noise(&p("C:/work/proj"), &roots), "the root itself is a directory event");
        assert!(is_noise(&p("C:/elsewhere/x.txt"), &roots), "outside every folder");
        // a dot component in the root is the user's choice, not noise
        assert!(!is_noise(&p("C:/Users/me/.config/notes/today.md"), &roots));
        assert!(is_noise(&p("C:/Users/me/.config/notes/.cache/x"), &roots));
        assert!(is_noise(&p("C:/work/proj/x"), &[]), "no folders: nothing to index");
    }

    #[test]
    fn nothing_pending_yields_nothing() {
        let mut pending = Pending::default();
        assert!(pending.is_empty());
        assert_eq!(pending.take_if_quiet(Instant::now() + Duration::from_secs(60)), None);
    }
}
