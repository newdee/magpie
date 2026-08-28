//! E2E for video-frame OCR: synthesize a video from a known-text image, cut
//! shots, OCR the representative frame at 960px, store the text, and find
//! the video back by a CJK substring of that text.
//! Run: cargo run -p magpie-core --example verify_video_ocr -- <text.png> [ocr_cache]

use magpie_core::videos::DecodeOpts;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let png = args.next().expect("usage: verify_video_ocr <text.png> [ocr_cache]");
    let cache = args
        .next()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("magpie-ocr-test"));

    let dir = std::env::temp_dir().join("magpie-video-ocr-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    let video = dir.join("text.mp4");
    let status = std::process::Command::new("ffmpeg")
        .args(["-y", "-loop", "1", "-i", &png, "-t", "3", "-r", "10"])
        .args(["-pix_fmt", "yuv420p", "-vf", "scale=trunc(iw/2)*2:trunc(ih/2)*2"])
        .arg(video.to_str().unwrap())
        .output()?;
    anyhow::ensure!(status.status.success(), "ffmpeg synth failed");
    let vpath = video.to_str().unwrap().to_string();

    // shots + representative-frame OCR at 960px
    let (shots, duration) = magpie_core::videos::detect_shots(&vpath, DecodeOpts::default())?;
    anyhow::ensure!(!shots.is_empty(), "expected at least one shot");
    let mid = (shots[0].start_ms + shots[0].end_ms) / 2;
    let frame = magpie_core::videos::frame_at_sized(&vpath, mid, DecodeOpts::default(), 960)?;
    let mut ocr = magpie_core::ocr::Ocr::new(&cache, &mut |_| {})?;
    let text = ocr.extract_text(&frame)?;
    anyhow::ensure!(text.contains("Hello"), "latin text read back, got {text:?}");
    anyhow::ensure!(text.contains("本地搜索"), "CJK text read back, got {text:?}");

    // persistence + retrieval path
    let conn = magpie_core::db::open(&dir.join("t.db"))?;
    conn.execute("INSERT INTO folders (path) VALUES ('/t')", [])?;
    conn.execute(
        "INSERT INTO files (folder_id, path, name, ext, size, mtime) VALUES (1, ?1, 'text.mp4', 'mp4', 1, 1)",
        magpie_core::rusqlite::params![vpath],
    )?;
    conn.execute(
        "INSERT INTO video_index (file_id, mtime, duration_ms, shot_count) VALUES (1, 1, ?1, 1)",
        magpie_core::rusqlite::params![duration],
    )?;
    conn.execute(
        "INSERT INTO video_shots (file_id, start_ms, end_ms, ts_ms, thumb) VALUES (1, ?1, ?2, ?3, 'x')",
        magpie_core::rusqlite::params![shots[0].start_ms, shots[0].end_ms, mid],
    )?;
    let pending = magpie_core::videos::pending_ocr_shots(&conn, 10)?;
    anyhow::ensure!(pending.len() == 1, "fresh shot must be pending");
    magpie_core::videos::set_shot_ocr(&conn, pending[0].0, &text)?;
    anyhow::ensure!(
        magpie_core::videos::pending_ocr_shots(&conn, 10)?.is_empty(),
        "processed shot must not re-pend"
    );
    let hits = magpie_core::videos::video_ocr_search(&conn, "本地搜索", 10)?;
    anyhow::ensure!(hits.len() == 1, "CJK substring must find the video");
    println!("ok: shots={} text={:?} search hit shot {}", shots.len(), text, hits[0].1);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
