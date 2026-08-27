//! Video search: shot-boundary detection + representative-frame SigLIP
//! embeddings, so an image (or text) query can land inside a video and the
//! result names the exact time range.
//!
//! Pipeline per video (files already indexed by name in `files`):
//!   1. ffmpeg decodes 2fps @ 160px RGB frames (cheap pass)
//!   2. RGB histogram (4x4x4 bins) chi-square distance between neighbours →
//!      shot boundaries (pure Rust — no OpenCV; ffmpeg is the only native dep)
//!   3. each shot contributes representative frames (midpoint, plus one per
//!      20s for long shots, ≤3/shot, ≤200/video)
//!   4. a second ffmpeg seek per rep frame grabs a 480px still → 96px thumb
//!      + SigLIP embedding, stored in `video_shots`
//!
//! ffmpeg resolution: a system `ffmpeg` on PATH wins; otherwise a static
//! build is auto-downloaded once (ffmpeg-sidecar). Both failures surface as
//! a status string, never a crash.

use anyhow::{anyhow, Result};
use base64::Engine;
use rusqlite::{params, Connection};
use serde::Serialize;

pub const VIDEO_EXTS: &[&str] = &["mp4", "mkv", "mov", "avi", "webm", "m4v"];

pub fn is_video_ext(ext: Option<&str>) -> bool {
    ext.map(|e| VIDEO_EXTS.contains(&e.to_lowercase().as_str())).unwrap_or(false)
}

/// A search hit inside a video: the best-matching shot of a file.
#[derive(Debug, Clone, Serialize)]
pub struct VideoHit {
    pub id: i64, // file id
    pub shot_id: i64,
    pub path: String,
    pub name: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub ts_ms: i64,
    pub thumb: Option<String>,
    pub duration_ms: i64,
    pub score: f32,
}

// ---------- ffmpeg resolution ----------

/// Locate a usable ffmpeg: system PATH first, else sidecar auto-download.
pub fn ensure_ffmpeg() -> Result<std::path::PathBuf> {
    if let Ok(out) = std::process::Command::new("ffmpeg").arg("-version").output() {
        if out.status.success() {
            return Ok(std::path::PathBuf::from("ffmpeg"));
        }
    }
    ffmpeg_sidecar::download::auto_download()
        .map_err(|e| anyhow!("ffmpeg unavailable (install it or allow the download): {e}"))?;
    Ok(ffmpeg_sidecar::paths::ffmpeg_path())
}

// ---------- shot detection ----------

const SAMPLE_FPS: f32 = 2.0;
const BINS: usize = 64; // 4x4x4 RGB
const BOUNDARY_THRESHOLD: f32 = 0.4;
const MIN_SHOT_SAMPLES: usize = 2; // ≥1s at 2fps
const LONG_SHOT_STEP_MS: i64 = 20_000;
const MAX_REPS_PER_SHOT: usize = 3;
const MAX_REPS_PER_VIDEO: usize = 200;

#[derive(Debug, Clone, PartialEq)]
pub struct Shot {
    pub start_ms: i64,
    pub end_ms: i64,
}

fn histogram(rgb: &[u8]) -> [f32; BINS] {
    let mut h = [0f32; BINS];
    let (pixels, _) = rgb.as_chunks::<3>();
    for px in pixels {
        let idx = (px[0] as usize >> 6) * 16 + (px[1] as usize >> 6) * 4 + (px[2] as usize >> 6);
        h[idx] += 1.0;
    }
    let total: f32 = h.iter().sum();
    if total > 0.0 {
        for v in h.iter_mut() {
            *v /= total;
        }
    }
    h
}

fn chi_square(a: &[f32; BINS], b: &[f32; BINS]) -> f32 {
    let mut d = 0.0;
    for i in 0..BINS {
        let s = a[i] + b[i];
        if s > 1e-9 {
            let diff = a[i] - b[i];
            d += diff * diff / s;
        }
    }
    d * 0.5
}

/// Split a sampled frame sequence (timestamps in ms, one histogram each)
/// into shots. Pure function — unit-testable without ffmpeg.
pub fn detect_shots_from_histograms(ts_ms: &[i64], hists: &[[f32; BINS]]) -> Vec<Shot> {
    let n = ts_ms.len().min(hists.len());
    if n == 0 {
        return Vec::new();
    }
    let mut shots = Vec::new();
    let mut start_i = 0usize;
    for i in 1..n {
        let boundary = chi_square(&hists[i - 1], &hists[i]) > BOUNDARY_THRESHOLD;
        if boundary && i - start_i >= MIN_SHOT_SAMPLES {
            shots.push(Shot { start_ms: ts_ms[start_i], end_ms: ts_ms[i] });
            start_i = i;
        }
    }
    let tail_end = ts_ms[n - 1] + (1000.0 / SAMPLE_FPS) as i64;
    shots.push(Shot { start_ms: ts_ms[start_i], end_ms: tail_end });
    shots
}

/// Representative timestamps for a shot: midpoint, plus one per 20s of a
/// long shot, capped.
pub fn rep_timestamps(shot: &Shot) -> Vec<i64> {
    let len = shot.end_ms - shot.start_ms;
    if len <= 0 {
        return vec![shot.start_ms];
    }
    let mut reps = vec![shot.start_ms + len / 2];
    if len > LONG_SHOT_STEP_MS {
        let extra = ((len / LONG_SHOT_STEP_MS) as usize).min(MAX_REPS_PER_SHOT - 1);
        for k in 1..=extra {
            let t = shot.start_ms + (len * k as i64) / (extra as i64 + 1);
            reps.push(t);
        }
        reps.sort_unstable();
        reps.dedup();
        reps.truncate(MAX_REPS_PER_SHOT);
    }
    reps
}

/// Decode the sampling pass and detect shots. Returns (shots, duration_ms).
pub fn detect_shots(path: &str) -> Result<(Vec<Shot>, i64)> {
    use ffmpeg_sidecar::command::FfmpegCommand;
    use ffmpeg_sidecar::event::FfmpegEvent;

    let mut ts: Vec<i64> = Vec::new();
    let mut hists: Vec<[f32; BINS]> = Vec::new();
    let iter = FfmpegCommand::new()
        .input(path)
        .args(["-vf", &format!("fps={SAMPLE_FPS},scale=160:-2"), "-an", "-sn"])
        .rawvideo()
        .spawn()?
        .iter()?;
    for ev in iter {
        if let FfmpegEvent::OutputFrame(f) = ev {
            ts.push((f.timestamp * 1000.0) as i64);
            hists.push(histogram(&f.data));
        }
    }
    if ts.is_empty() {
        return Err(anyhow!("no decodable video frames"));
    }
    let duration = ts.last().copied().unwrap_or(0) + (1000.0 / SAMPLE_FPS) as i64;
    Ok((detect_shots_from_histograms(&ts, &hists), duration))
}

/// Grab one frame at `ts_ms` as a decoded image (480px wide).
pub fn frame_at(path: &str, ts_ms: i64) -> Result<image::DynamicImage> {
    use ffmpeg_sidecar::command::FfmpegCommand;
    use ffmpeg_sidecar::event::FfmpegEvent;

    let seek = format!("{}.{:03}", ts_ms / 1000, ts_ms % 1000);
    let iter = FfmpegCommand::new()
        .args(["-ss", &seek])
        .input(path)
        .args(["-frames:v", "1", "-vf", "scale=480:-2", "-an", "-sn"])
        .rawvideo()
        .spawn()?
        .iter()?;
    for ev in iter {
        if let FfmpegEvent::OutputFrame(f) = ev {
            let buf = image::RgbImage::from_raw(f.width, f.height, f.data)
                .ok_or_else(|| anyhow!("bad frame buffer"))?;
            return Ok(image::DynamicImage::ImageRgb8(buf));
        }
    }
    Err(anyhow!("no frame at {ts_ms}ms"))
}

fn thumb_b64(img: &image::DynamicImage) -> Option<String> {
    let thumb = img.thumbnail(96, 96).to_rgb8();
    let mut out = std::io::Cursor::new(Vec::new());
    thumb
        .write_to(&mut out, image::ImageFormat::Jpeg)
        .ok()
        .map(|_| base64::engine::general_purpose::STANDARD.encode(out.into_inner()))
}

// ---------- indexing ----------

/// Candidate videos: files rows with a video extension whose mtime is newer
/// than (or missing from) the video index.
pub fn pending_videos(conn: &Connection) -> Result<Vec<(i64, String, i64)>> {
    let exts = VIDEO_EXTS.iter().map(|e| format!("'{e}'")).collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT f.id, f.path, f.mtime FROM files f
         LEFT JOIN video_index vi ON vi.file_id = f.id
         WHERE lower(f.ext) IN ({exts})
           AND (vi.file_id IS NULL OR vi.mtime != f.mtime)
         ORDER BY f.mtime DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Drop shots whose file rows vanished (folder removal, deletions).
pub fn prune_orphan_shots(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM video_shots WHERE file_id NOT IN (SELECT id FROM files)", [])?;
    conn.execute("DELETE FROM video_index WHERE file_id NOT IN (SELECT id FROM files)", [])?;
    Ok(())
}

pub fn video_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM video_index", [], |r| r.get(0))?)
}

pub fn shot_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM video_shots", [], |r| r.get(0))?)
}

fn l2_normalize(v: &mut [f32]) {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 1e-12 {
        for x in v.iter_mut() {
            *x /= n;
        }
    }
}

/// Index one video: detect shots, embed representative frames, replace any
/// previous rows. Returns the number of embedded shots.
pub fn index_video(
    conn: &Connection,
    siglip: &mut crate::siglip::Siglip,
    file_id: i64,
    path: &str,
    mtime: i64,
) -> Result<usize> {
    let (shots, duration_ms) = detect_shots(path)?;
    let mut reps: Vec<(usize, i64)> = Vec::new(); // (shot idx, ts)
    for (si, s) in shots.iter().enumerate() {
        for t in rep_timestamps(s) {
            reps.push((si, t));
        }
    }
    reps.truncate(MAX_REPS_PER_VIDEO);

    conn.execute("DELETE FROM video_shots WHERE file_id = ?1", params![file_id])?;
    let mut stored = 0usize;
    for (si, ts) in reps {
        let s = &shots[si];
        let Ok(img) = frame_at(path, ts) else { continue };
        let thumb = thumb_b64(&img);
        let mut emb = siglip.embed_dynamic(img)?;
        l2_normalize(&mut emb);
        let blob: Vec<u8> = emb.iter().flat_map(|f| f.to_le_bytes()).collect();
        conn.execute(
            "INSERT INTO video_shots (file_id, start_ms, end_ms, ts_ms, thumb, embedding, model)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                file_id,
                s.start_ms,
                s.end_ms,
                ts,
                thumb,
                blob,
                crate::siglip::IMAGE_EMBED_MODEL_ID
            ],
        )?;
        stored += 1;
    }
    conn.execute(
        "INSERT INTO video_index (file_id, mtime, duration_ms, shot_count, indexed_at)
         VALUES (?1, ?2, ?3, ?4, strftime('%s','now'))
         ON CONFLICT(file_id) DO UPDATE SET
           mtime = excluded.mtime, duration_ms = excluded.duration_ms,
           shot_count = excluded.shot_count, indexed_at = excluded.indexed_at",
        params![file_id, mtime, duration_ms, shots.len() as i64],
    )?;
    Ok(stored)
}

/// All shot embeddings for the in-memory vector store: (shot_id, vector).
pub fn all_shot_embeddings(conn: &Connection) -> Result<Vec<(i64, Vec<f32>)>> {
    let mut stmt = conn.prepare("SELECT id, embedding FROM video_shots WHERE embedding IS NOT NULL")?;
    let rows = stmt.query_map([], |r| {
        let id: i64 = r.get(0)?;
        let blob: Vec<u8> = r.get(1)?;
        Ok((id, blob))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, blob) = row?;
        let (words, _) = blob.as_chunks::<4>();
        let vec: Vec<f32> = words.iter().map(|b| f32::from_le_bytes(*b)).collect();
        out.push((id, vec));
    }
    Ok(out)
}

/// Shot-id → owning file-id map, for grouping search results per video.
pub fn shot_owners(conn: &Connection) -> Result<std::collections::HashMap<i64, i64>> {
    let mut stmt = conn.prepare("SELECT id, file_id FROM video_shots")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Hydrate shot hits (already scored, one best shot per video).
pub fn hits_by_shot_ids(
    conn: &Connection,
    shot_ids: &[i64],
    scores: &std::collections::HashMap<i64, f32>,
) -> Result<Vec<VideoHit>> {
    let mut out = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT vs.id, vs.file_id, f.path, f.name, vs.start_ms, vs.end_ms, vs.ts_ms, vs.thumb,
                COALESCE(vi.duration_ms, 0)
         FROM video_shots vs
         JOIN files f ON f.id = vs.file_id
         LEFT JOIN video_index vi ON vi.file_id = vs.file_id
         WHERE vs.id = ?1",
    )?;
    for sid in shot_ids {
        let row = stmt.query_row(params![sid], |r| {
            Ok(VideoHit {
                shot_id: r.get(0)?,
                id: r.get(1)?,
                path: r.get(2)?,
                name: r.get(3)?,
                start_ms: r.get(4)?,
                end_ms: r.get(5)?,
                ts_ms: r.get(6)?,
                thumb: r.get(7)?,
                duration_ms: r.get(8)?,
                score: 0.0,
            })
        });
        if let Ok(mut h) = row {
            h.score = scores.get(sid).copied().unwrap_or(0.0);
            out.push(h);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hist_of(color: [u8; 3]) -> [f32; BINS] {
        let px: Vec<u8> = color.iter().copied().cycle().take(3 * 100).collect();
        histogram(&px)
    }

    #[test]
    fn detects_boundary_on_hard_color_change() {
        // 4 red samples, then 4 blue samples → two shots split at the change
        let red = hist_of([220, 30, 30]);
        let blue = hist_of([20, 40, 220]);
        let hists = vec![red, red, red, red, blue, blue, blue, blue];
        let ts: Vec<i64> = (0..8).map(|i| i * 500).collect();
        let shots = detect_shots_from_histograms(&ts, &hists);
        assert_eq!(shots.len(), 2);
        assert_eq!(shots[0].start_ms, 0);
        assert_eq!(shots[0].end_ms, 2000);
        assert_eq!(shots[1].start_ms, 2000);
    }

    #[test]
    fn similar_frames_stay_one_shot() {
        let a = hist_of([100, 100, 100]);
        let hists = vec![a; 10];
        let ts: Vec<i64> = (0..10).map(|i| i * 500).collect();
        let shots = detect_shots_from_histograms(&ts, &hists);
        assert_eq!(shots.len(), 1, "no false boundaries on a static scene");
    }

    #[test]
    fn min_shot_length_suppresses_flicker() {
        // boundary right after a boundary (1 sample apart) must not split
        let red = hist_of([220, 30, 30]);
        let blue = hist_of([20, 40, 220]);
        let green = hist_of([20, 220, 40]);
        let hists = vec![red, red, blue, green, green, green];
        let ts: Vec<i64> = (0..6).map(|i| i * 500).collect();
        let shots = detect_shots_from_histograms(&ts, &hists);
        // split at red→blue; blue→green suppressed (shot would be 1 sample)
        assert_eq!(shots.len(), 2);
    }

    #[test]
    fn rep_timestamps_midpoint_and_long_shot_extras() {
        assert_eq!(rep_timestamps(&Shot { start_ms: 0, end_ms: 4000 }), vec![2000]);
        let reps = rep_timestamps(&Shot { start_ms: 0, end_ms: 60_000 });
        assert!(reps.len() > 1 && reps.len() <= MAX_REPS_PER_SHOT);
        assert!(reps.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn video_ext_detection() {
        assert!(is_video_ext(Some("mp4")));
        assert!(is_video_ext(Some("MKV")));
        assert!(!is_video_ext(Some("png")));
        assert!(!is_video_ext(None));
    }
}
