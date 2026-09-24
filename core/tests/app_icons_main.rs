//! App icons, read on the process's main thread the way the app does it.
//!
//! A plain `#[test]` runs on a worker thread, and on macOS AppKit belongs to
//! the main thread, so this file opts out of the test harness (see
//! Cargo.toml) and runs its checks straight from `main`. CI runs it on
//! macOS; Windows and Linux are covered by the unit tests in `apps.rs` and
//! this binary only confirms the call is harmless there.

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
    let mut ok = 0;
    for t in &targets {
        let started = std::time::Instant::now();
        let icon = magpie_core::apps::icon(t, 64).unwrap_or_else(|| panic!("no icon for {t}"));
        assert_eq!(icon.mime, "image/png");
        let img = image::load_from_memory(&icon.bytes).expect("decodes").to_rgba8();
        let (w, h) = img.dimensions();
        assert!(w >= 32 && h >= 32 && w <= 256 && h <= 256, "{t}: {w}x{h}");
        let visible = img.pixels().filter(|p| p[3] > 0).count();
        assert!(visible > (w * h / 10) as usize, "{t}: icon is nearly empty");
        println!("{:>4}ms {w}x{h} {:>6}B {t}", started.elapsed().as_millis(), icon.bytes.len());
        ok += 1;
    }
    println!("app_icons_main: {ok} icons ok");
}
