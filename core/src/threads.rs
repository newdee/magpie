//! How many CPU threads each ONNX session (text embedding, image embedding,
//! OCR) may use. One knob in meta, applied to every session at load time;
//! the app reloads the models when it changes.
//!
//! Without a cap ONNX Runtime takes every core it can see, which turns an
//! embed pass on a 32-core desktop into a 32-thread burst that the user
//! notices. Measured there with e5-small (chunks per second at cores used):
//! 1 thread 4.5 at 1.0, 4 threads 14.7 at 3.8, all 32 threads 26 at 21.8.
//! Four threads keep over half the throughput for under a fifth of the CPU
//! and leave the machine responsive; "every core" stays one click away for
//! a first index of a big folder.

use crate::db;
use rusqlite::Connection;
use std::sync::atomic::{AtomicBool, Ordering};

pub const META_KEY: &str = "index_threads";

/// The model an embed pass drives. Each has its own stop flag, so reloading
/// one never interrupts a pass on the other. (OCR has none: its passes take
/// the engine per item, so a reload swaps it in between two items anyway.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    Text,
    Image,
}

static STOP: [AtomicBool; 2] = [const { AtomicBool::new(false) }; 2];

/// Ask every pass driving `model` to end at its next item. A pass holds the
/// model for its whole run, so without this a reload would wait for the run
/// to finish, at the old thread count, before it could swap the session in.
/// The reload's own catch-up pass then picks up where the run stopped.
pub fn stop(model: Model) {
    STOP[model as usize].store(true, Ordering::SeqCst);
}

/// Let passes on `model` run again: the reload swapped its session in, or
/// gave up. Every reload path must end here, or nothing embeds again.
pub fn resume(model: Model) {
    STOP[model as usize].store(false, Ordering::SeqCst);
}

/// Checked by the passes once per item.
pub fn stopping(model: Model) -> bool {
    STOP[model as usize].load(Ordering::SeqCst)
}
/// Applied when nothing is stored.
pub const DEFAULT: usize = 4;

/// Logical cores on this machine (1 when the OS will not say).
pub fn cores() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
}

/// Turn the stored setting into a thread count: `None` is the default cap,
/// `0` means every core, `n` means `n`. Always within `1..=cores`.
pub fn resolve(setting: Option<usize>, cores: usize) -> usize {
    let cores = cores.max(1);
    match setting {
        None => DEFAULT.min(cores),
        Some(0) => cores,
        Some(n) => n.min(cores),
    }
}

/// What the settings page shows: the stored choice clamped to the machine,
/// with `0` kept as "every core" so that choice stays selectable.
pub fn displayed(setting: Option<usize>, cores: usize) -> usize {
    match setting {
        Some(0) => 0,
        other => resolve(other, cores),
    }
}

/// The raw stored setting, if any (unparseable values count as unset).
pub fn setting(conn: &Connection) -> Option<usize> {
    db::meta_get(conn, META_KEY).ok().flatten().and_then(|v| v.parse().ok())
}

/// The thread count to load sessions with right now.
pub fn from_meta(conn: &Connection) -> usize {
    resolve(setting(conn), cores())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_caps_at_four_but_never_above_the_machine() {
        assert_eq!(resolve(None, 32), 4);
        assert_eq!(resolve(None, 4), 4);
        assert_eq!(resolve(None, 2), 2);
        assert_eq!(resolve(None, 1), 1);
    }

    #[test]
    fn zero_means_every_core() {
        assert_eq!(resolve(Some(0), 32), 32);
        assert_eq!(resolve(Some(0), 1), 1);
    }

    #[test]
    fn explicit_counts_are_clamped_to_the_machine() {
        assert_eq!(resolve(Some(2), 32), 2);
        assert_eq!(resolve(Some(8), 32), 8);
        assert_eq!(resolve(Some(8), 6), 6);
        assert_eq!(resolve(Some(1), 6), 1);
        assert_eq!(resolve(Some(usize::MAX), 6), 6);
    }

    #[test]
    fn a_machine_reporting_zero_cores_still_gets_one_thread() {
        assert_eq!(resolve(None, 0), 1);
        assert_eq!(resolve(Some(0), 0), 1);
        assert_eq!(resolve(Some(3), 0), 1);
    }

    #[test]
    fn the_settings_page_sees_the_clamped_choice_with_zero_kept() {
        assert_eq!(displayed(None, 32), 4);
        assert_eq!(displayed(None, 2), 2);
        assert_eq!(displayed(Some(0), 32), 0, "'every core' stays selectable");
        assert_eq!(displayed(Some(8), 32), 8);
        assert_eq!(displayed(Some(8), 6), 6, "imported from a bigger machine");
    }

    #[test]
    fn stop_flags_are_per_model_and_off_by_default() {
        // one test for both flags: they are process-global
        assert!(!stopping(Model::Text) && !stopping(Model::Image));
        stop(Model::Text);
        assert!(stopping(Model::Text), "text passes are told to stop");
        assert!(!stopping(Model::Image), "image passes are not");
        resume(Model::Text);
        stop(Model::Image);
        assert!(!stopping(Model::Text) && stopping(Model::Image));
        resume(Model::Image);
        assert!(!stopping(Model::Text) && !stopping(Model::Image));
    }

    #[test]
    fn stored_values_round_trip_and_junk_is_unset() {
        let conn = db::open_in_memory().unwrap();
        assert_eq!(setting(&conn), None);
        db::meta_set(&conn, META_KEY, "8").unwrap();
        assert_eq!(setting(&conn), Some(8));
        db::meta_set(&conn, META_KEY, "0").unwrap();
        assert_eq!(setting(&conn), Some(0));
        db::meta_set(&conn, META_KEY, "lots").unwrap();
        assert_eq!(setting(&conn), None, "junk falls back to the default cap");
        assert!(from_meta(&conn) >= 1);
    }
}
