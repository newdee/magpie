//! App icons, read the way the app reads them.
//!
//! A plain `#[test]` runs on a worker thread, and on macOS AppKit cares about
//! threads, so this file opts out of the test harness (see Cargo.toml) and
//! runs from `main`. CI runs it on macOS three ways.
//!
//! - Plain: icons read on the main thread and on background threads.
//! - Under Apple's Main Thread Checker with MTC_CRASH_ON_REPORT=1: the
//!   background reads must not trip it (magpie reads icons off the main
//!   thread, so a report here would be a real bug).
//! - Control, MAGPIE_MTC_CONTROL=1: a call the checker must catch, run off
//!   the main thread, so CI proves the checker was actually loaded.
//!
//! Windows and Linux are covered by the unit tests in `apps.rs`.

fn main() {
    #[cfg(target_os = "macos")]
    macos();
    #[cfg(not(target_os = "macos"))]
    {
        // a path that is not an app must come back empty, not panic
        assert!(magpie_core::apps::icon("/definitely/not/an/app", 64).is_none());
        println!("app_icons_main: ok (macOS checks run on macOS only)");
    }
}

#[cfg(target_os = "macos")]
fn macos() {
    if std::env::var("MAGPIE_MTC_CONTROL").is_ok() {
        std::thread::spawn(|| {
            use objc2::MainThreadMarker;
            // SAFETY: deliberately wrong. An NSView belongs to the main
            // thread; the Main Thread Checker must stop the process here.
            let _view = objc2_app_kit::NSView::new(unsafe { MainThreadMarker::new_unchecked() });
        })
        .join()
        .unwrap();
        println!("control: an NSView was created off the main thread and nothing stopped it");
        return;
    }

    // issue #4: apps inside folders (Utilities) are listed, under their
    // localized names too, whatever language the runner speaks
    let t = std::time::Instant::now();
    let apps = magpie_core::apps::list_apps();
    let scan_ms = t.elapsed().as_millis();
    let am = apps
        .iter()
        .find(|a| a.target.ends_with("/Utilities/Activity Monitor.app"))
        .expect("Activity Monitor in Utilities is listed");
    assert!(am.aliases.iter().any(|a| a == "活动监视器"), "aliases: {:?}", am.aliases);
    let hits = magpie_core::apps::match_apps(&apps, "活动", 6, true);
    assert!(hits.iter().any(|h| h.target == am.target), "活动 finds it");
    let hits = magpie_core::apps::match_apps(&apps, "huodong", 6, true);
    assert!(hits.iter().any(|h| h.target == am.target), "huodong finds it");
    let nested = apps.iter().filter(|a| a.target.matches(".app").count() == 1 && a.target.contains("/Utilities/")).count();
    println!("list_apps: {} apps ({nested} in Utilities) in {scan_ms} ms; Activity Monitor aliases: {:?}", apps.len(), am.aliases);

    // bundles every macOS install ships, plus whatever list_apps finds
    let mut targets: Vec<String> = [
        "/System/Applications/Calculator.app",
        "/System/Applications/System Settings.app",
        "/System/Applications/Utilities/Terminal.app",
    ]
    .iter()
    .filter(|p| std::path::Path::new(p).exists())
    .map(|p| p.to_string())
    .collect();
    assert!(!targets.is_empty(), "no system app bundles found");
    targets.extend(magpie_core::apps::list_apps().into_iter().take(8).map(|a| a.target));

    println!("-- main thread");
    let main_ms = read_all(&targets);
    // what the app does: every read on a background thread, several at once
    println!("-- background threads");
    let bg: Vec<std::thread::JoinHandle<u128>> = targets
        .chunks(4)
        .map(|chunk| {
            let chunk = chunk.to_vec();
            std::thread::spawn(move || read_all(&chunk))
        })
        .collect();
    let bg_ms: u128 = bg.into_iter().map(|h| h.join().expect("background read panicked")).sum();
    println!("app_icons_main: {} icons ok; main thread {main_ms} ms, background {bg_ms} ms", targets.len());
}

#[cfg(target_os = "macos")]
fn read_all(targets: &[String]) -> u128 {
    let mut total = 0;
    for t in targets {
        let started = std::time::Instant::now();
        let icon = magpie_core::apps::icon(t, 64).unwrap_or_else(|| panic!("no icon for {t}"));
        assert_eq!(icon.mime, "image/png");
        let img = image::load_from_memory(&icon.bytes).expect("decodes").to_rgba8();
        let (w, h) = img.dimensions();
        assert!(w >= 32 && h >= 32 && w <= 256 && h <= 256, "{t}: {w}x{h}");
        let visible = img.pixels().filter(|p| p[3] > 0).count();
        assert!(visible > (w * h / 10) as usize, "{t}: icon is nearly empty");
        let ms = started.elapsed().as_millis();
        total += ms;
        println!("{ms:>5}ms {w}x{h} {:>6}B {t}", icon.bytes.len());
    }
    total
}
