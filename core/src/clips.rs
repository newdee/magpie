//! Clipboard history: opt-in, text-only, local.
//!
//! The shell polls the clipboard and hands changed text to [`record_clip`];
//! everything else mirrors the bookmark pipeline (FTS + one vector per clip).

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::embed::{self, Embedder};

/// Ignore trivially short clips; the upper bound is user-configurable.
const MIN_LEN: usize = 3;
pub const DEFAULT_MAX_LEN: usize = 100_000;
const EMBED_BATCH: usize = 32;
/// Only the head of a long clip goes into the vector; FTS still gets it all.
const EMBED_HEAD: usize = 2_000;

#[derive(Debug, Clone, Serialize)]
pub struct ClipHit {
    pub id: i64,
    pub content: String,
    pub first_copied: i64,
    pub last_copied: i64,
    pub copy_count: i64,
    /// "text" or "image".
    pub clip_kind: String,
    /// 96px preview for image clips (base64 JPEG).
    pub thumb: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub score: f32,
}

const CLIP_COLS: &str =
    "id, content, first_copied, last_copied, copy_count, kind, thumb, width, height";

fn row_to_hit(r: &rusqlite::Row) -> rusqlite::Result<ClipHit> {
    Ok(ClipHit {
        id: r.get(0)?,
        content: r.get(1)?,
        first_copied: r.get(2)?,
        last_copied: r.get(3)?,
        copy_count: r.get(4)?,
        clip_kind: r.get(5)?,
        thumb: r.get(6)?,
        width: r.get(7)?,
        height: r.get(8)?,
        score: 0.0,
    })
}

/// Store one clipboard capture. Repeats bump `last_copied`/`copy_count`
/// instead of duplicating. Oversized clips are skipped whole rather than
/// truncated — a clip that pastes back different from what was copied is
/// worse than one that was never recorded. Returns whether anything was written.
pub fn record_clip(conn: &Connection, text: &str, now: i64, max_len: usize) -> Result<bool> {
    let trimmed = text.trim();
    if trimmed.len() < MIN_LEN || (max_len > 0 && trimmed.len() > max_len) {
        return Ok(false);
    }
    let hash = embed::doc_hash(trimmed);
    let updated = conn.execute(
        "UPDATE clips SET last_copied = ?2, copy_count = copy_count + 1
         WHERE content_hash = ?1",
        params![hash, now],
    )?;
    if updated == 0 {
        conn.execute(
            "INSERT INTO clips(content, content_hash, first_copied, last_copied)
             VALUES (?1, ?2, ?3, ?3)",
            params![trimmed, hash, now],
        )?;
    }
    Ok(true)
}

/// Drop clips older than the retention window (by last copy). 0 = keep forever.
pub fn prune_clips(conn: &Connection, retention_days: u32, now: i64) -> Result<usize> {
    if retention_days == 0 {
        return Ok(0);
    }
    let cutoff = now - i64::from(retention_days) * 86_400;
    let n = conn.execute("DELETE FROM clips WHERE last_copied < ?1", params![cutoff])?;
    conn.execute(
        "DELETE FROM clip_vecs WHERE clip_id NOT IN (SELECT id FROM clips)",
        [],
    )?;
    Ok(n)
}

/// Keep at most `max_entries` clips, dropping the least recently copied.
/// 0 = unlimited.
pub fn prune_clips_to_count(conn: &Connection, max_entries: u32) -> Result<usize> {
    if max_entries == 0 {
        return Ok(0);
    }
    let n = conn.execute(
        "DELETE FROM clips WHERE id IN (
            SELECT id FROM clips ORDER BY last_copied DESC, id DESC
            LIMIT -1 OFFSET ?1
        )",
        params![i64::from(max_entries)],
    )?;
    if n > 0 {
        conn.execute(
            "DELETE FROM clip_vecs WHERE clip_id NOT IN (SELECT id FROM clips)",
            [],
        )?;
    }
    Ok(n)
}

pub fn delete_clip(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM clips WHERE id = ?1", params![id])?;
    conn.execute("DELETE FROM clip_vecs WHERE clip_id = ?1", params![id])?;
    Ok(())
}

pub fn clear_clips(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM clips", [])?;
    conn.execute("DELETE FROM clip_vecs", [])?;
    Ok(())
}

pub fn clip_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM clips", [], |r| r.get(0))?)
}

/// One vector per clip (head only for long clips). Blocking.
pub fn embed_pending_clips(
    conn: &Connection,
    embedder: &mut Embedder,
    mut progress: impl FnMut(usize, usize),
) -> Result<usize> {
    let hashes: std::collections::HashMap<i64, String> = {
        let mut stmt = conn.prepare("SELECT clip_id, doc_hash FROM clip_vecs")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        rows
    };
    let pending: Vec<(i64, String, String)> = {
        let mut stmt = conn.prepare("SELECT id, content FROM clips")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .filter_map(|(id, content)| {
                let doc: String = content.chars().take(EMBED_HEAD).collect();
                let hash = embed::doc_hash(&doc);
                if hashes.get(&id) == Some(&hash) {
                    None
                } else {
                    Some((id, doc, hash))
                }
            })
            .collect()
    };

    let total = pending.len();
    let mut done = 0usize;
    progress(0, total);
    for chunk in pending.chunks(EMBED_BATCH) {
        let docs: Vec<String> = chunk.iter().map(|(_, d, _)| d.clone()).collect();
        let vecs = embedder.embed_passages(&docs)?;
        for ((id, _, hash), vec) in chunk.iter().zip(vecs) {
            let bytes: Vec<u8> = vec.iter().flat_map(|f| f.to_le_bytes()).collect();
            conn.execute(
                "INSERT INTO clip_vecs(clip_id, doc_hash, dim, vec)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(clip_id) DO UPDATE SET doc_hash = excluded.doc_hash,
                                                    dim = excluded.dim, vec = excluded.vec",
                params![id, hash, vec.len() as i64, bytes],
            )?;
        }
        done += chunk.len();
        progress(done, total);
    }
    conn.execute(
        "DELETE FROM clip_vecs WHERE clip_id NOT IN (SELECT id FROM clips)",
        [],
    )?;
    Ok(total)
}

pub fn all_clip_embeddings(conn: &Connection) -> Result<Vec<(i64, Vec<f32>)>> {
    let mut stmt = conn.prepare("SELECT clip_id, dim, vec FROM clip_vecs")?;
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)? as usize, r.get::<_, Vec<u8>>(2)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut out = Vec::with_capacity(rows.len());
    for (id, dim, bytes) in rows {
        if bytes.len() != dim * 4 {
            continue;
        }
        let (chunks, _) = bytes.as_chunks::<4>();
        out.push((id, chunks.iter().map(|c| f32::from_le_bytes(*c)).collect()));
    }
    Ok(out)
}

pub fn clips_fts_search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<i64>> {
    let fts_query = crate::db::build_fts_query(query);
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT rowid FROM clips_fts WHERE clips_fts MATCH ?1
         ORDER BY bm25(clips_fts) LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![fts_query, limit as i64], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn clips_by_ids(
    conn: &Connection,
    ids: &[i64],
    scores: &std::collections::HashMap<i64, f32>,
) -> Result<Vec<ClipHit>> {
    let mut out = Vec::with_capacity(ids.len());
    let mut stmt =
        conn.prepare(&format!("SELECT {CLIP_COLS} FROM clips WHERE id = ?1"))?;
    for id in ids {
        let hit = stmt.query_row([id], row_to_hit).optional()?;
        if let Some(mut h) = hit {
            h.score = scores.get(&h.id).copied().unwrap_or(0.0);
            out.push(h);
        }
    }
    Ok(out)
}

/// Most recently copied first — what an empty query shows.
pub fn recent_clips(conn: &Connection, limit: usize) -> Result<Vec<ClipHit>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {CLIP_COLS} FROM clips ORDER BY last_copied DESC, id DESC LIMIT ?1"
    ))?;
    let rows = stmt
        .query_map([limit as i64], row_to_hit)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ---------- image clips ----------

/// Longest edge stored for an image clip; larger captures downscale.
const IMAGE_MAX_EDGE: u32 = 1600;

/// Encode a raw RGBA clipboard capture: bounded JPEG for storage + paste-back
/// fidelity, 96px thumb for rows. Returns (jpeg, thumb_b64, w, h).
pub fn encode_clipboard_image(
    width: usize,
    height: usize,
    rgba: &[u8],
) -> Option<(Vec<u8>, String, u32, u32)> {
    use base64::Engine;
    let buf = image::RgbaImage::from_raw(width as u32, height as u32, rgba.to_vec())?;
    let img = image::DynamicImage::ImageRgba8(buf);
    let img = if img.width().max(img.height()) > IMAGE_MAX_EDGE {
        img.thumbnail(IMAGE_MAX_EDGE, IMAGE_MAX_EDGE)
    } else {
        img
    };
    let (w, h) = (img.width(), img.height());
    let rgb = img.to_rgb8();
    let mut jpeg = std::io::Cursor::new(Vec::new());
    rgb.write_to(&mut jpeg, image::ImageFormat::Jpeg).ok()?;
    let thumb = image::DynamicImage::ImageRgb8(rgb).thumbnail(96, 96).to_rgb8();
    let mut tout = std::io::Cursor::new(Vec::new());
    thumb.write_to(&mut tout, image::ImageFormat::Jpeg).ok()?;
    let thumb_b64 = base64::engine::general_purpose::STANDARD.encode(tout.into_inner());
    Some((jpeg.into_inner(), thumb_b64, w, h))
}

/// Store one image capture; repeats (same content hash) bump the counters.
pub fn record_image_clip(
    conn: &Connection,
    hash: &str,
    jpeg: &[u8],
    thumb_b64: &str,
    width: u32,
    height: u32,
    now: i64,
) -> Result<bool> {
    let updated = conn.execute(
        "UPDATE clips SET last_copied = ?2, copy_count = copy_count + 1
         WHERE content_hash = ?1",
        params![hash, now],
    )?;
    if updated == 0 {
        conn.execute(
            "INSERT INTO clips(content, content_hash, first_copied, last_copied,
                               kind, image, thumb, width, height)
             VALUES ('', ?1, ?2, ?2, 'image', ?3, ?4, ?5, ?6)",
            params![hash, now, jpeg, thumb_b64, width, height],
        )?;
    }
    Ok(true)
}

/// The stored JPEG of an image clip (copy-back and the preview pane).
pub fn image_clip_jpeg(conn: &Connection, id: i64) -> Result<Option<Vec<u8>>> {
    Ok(conn
        .query_row("SELECT image FROM clips WHERE id = ?1 AND kind = 'image'", [id], |r| {
            r.get(0)
        })
        .optional()?)
}

/// Put an image clip back on the clipboard.
pub fn set_clipboard_image(jpeg: &[u8]) -> Result<()> {
    let img = image::load_from_memory(jpeg)?.to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    let data = arboard::ImageData {
        width: w,
        height: h,
        bytes: std::borrow::Cow::Owned(img.into_raw()),
    };
    arboard::Clipboard::new()
        .and_then(|mut b| b.set_image(data))
        .map_err(|e| anyhow::anyhow!("clipboard: {e}"))
}

/// SigLIP-embed image clips that don't have a vector yet.
pub fn embed_pending_image_clips(
    conn: &Connection,
    siglip: &mut crate::siglip::Siglip,
) -> Result<usize> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.image FROM clips c
         LEFT JOIN clip_image_vecs v ON v.clip_id = c.id
         WHERE c.kind = 'image' AND c.image IS NOT NULL AND v.clip_id IS NULL
         LIMIT 64",
    )?;
    let pending: Vec<(i64, Vec<u8>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut done = 0;
    for (id, jpeg) in pending {
        let Ok(img) = image::load_from_memory(&jpeg) else { continue };
        let mut vec = siglip.embed_dynamic(img)?;
        let norm = vec.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
        vec.iter_mut().for_each(|x| *x /= norm);
        let blob: Vec<u8> = vec.iter().flat_map(|f| f.to_le_bytes()).collect();
        conn.execute(
            "INSERT OR REPLACE INTO clip_image_vecs (clip_id, dim, vec) VALUES (?1, ?2, ?3)",
            params![id, vec.len() as i64, blob],
        )?;
        done += 1;
    }
    Ok(done)
}

/// (clip_id, SigLIP vector) for the resident store.
pub fn all_clip_image_embeddings(conn: &Connection) -> Result<Vec<(i64, Vec<f32>)>> {
    let mut stmt = conn.prepare("SELECT clip_id, vec FROM clip_image_vecs")?;
    let rows = stmt.query_map([], |r| {
        let id: i64 = r.get(0)?;
        let blob: Vec<u8> = r.get(1)?;
        Ok((id, blob))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, blob) = row?;
        let (words, _) = blob.as_chunks::<4>();
        out.push((id, words.iter().map(|b| f32::from_le_bytes(*b)).collect()));
    }
    Ok(out)
}

// ---------- clipboard access ----------

/// Polls the OS clipboard; yields text only when it changes. The caller owns
/// the cadence (a 1s tick costs nothing measurable).
pub struct ClipboardWatcher {
    board: arboard::Clipboard,
    last_hash: Option<String>,
}

impl ClipboardWatcher {
    pub fn new() -> Result<Self> {
        Ok(Self {
            board: arboard::Clipboard::new().map_err(|e| anyhow::anyhow!("clipboard: {e}"))?,
            last_hash: None,
        })
    }

    /// Returns new clipboard text once per change; None while unchanged,
    /// non-text, or marked sensitive by the source app.
    pub fn poll(&mut self) -> Option<String> {
        if sensitive_clip_present() {
            return None;
        }
        let text = self.board.get_text().ok()?;
        let hash = embed::doc_hash(text.trim());
        if self.last_hash.as_deref() == Some(hash.as_str()) {
            return None;
        }
        self.last_hash = Some(hash);
        Some(text)
    }

    /// Returns a new clipboard IMAGE once per change (when no text is
    /// present). The hash samples the raw pixels — a 4K screenshot hashes in
    /// well under a millisecond, so the 1s tick stays cheap.
    pub fn poll_image(&mut self) -> Option<(usize, usize, Vec<u8>)> {
        if sensitive_clip_present() {
            return None;
        }
        let img = self.board.get_image().ok()?;
        let hash = sample_hash(img.width, img.height, &img.bytes);
        if self.last_hash.as_deref() == Some(hash.as_str()) {
            return None;
        }
        self.last_hash = Some(hash);
        Some((img.width, img.height, img.bytes.into_owned()))
    }
}

/// Cheap content hash for big pixel buffers: dimensions + strided samples.
pub fn sample_hash(width: usize, height: usize, bytes: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    width.hash(&mut h);
    height.hash(&mut h);
    bytes.len().hash(&mut h);
    let step = (bytes.len() / 4096).max(1);
    let mut i = 0;
    while i < bytes.len() {
        bytes[i].hash(&mut h);
        i += step;
    }
    format!("img:{:016x}", h.finish())
}

/// Put text back on the clipboard (Enter on a clip hit).
pub fn set_clipboard_text(text: &str) -> Result<()> {
    arboard::Clipboard::new()
        .and_then(|mut b| b.set_text(text.to_string()))
        .map_err(|e| anyhow::anyhow!("clipboard: {e}"))
}

/// Password managers mark clipboard entries that monitors must ignore.
#[cfg(windows)]
fn sensitive_clip_present() -> bool {
    use std::sync::OnceLock;
    static FORMATS: OnceLock<Vec<u32>> = OnceLock::new();
    let formats = FORMATS.get_or_init(|| {
        ["ExcludeClipboardContentFromMonitorProcessing", "CF_CLIPBOARD_VIEWER_IGNORE"]
            .iter()
            .filter_map(|n| clipboard_win::register_format(n).map(|f| f.get()))
            .collect()
    });
    formats.iter().any(|&f| clipboard_win::is_format_avail(f))
}

/// macOS password managers mark the pasteboard with nspasteboard.org types
/// (`ConcealedType` for passwords, plus transient/auto-generated) that history
/// tools are expected to honor by not recording the content.
#[cfg(target_os = "macos")]
fn sensitive_clip_present() -> bool {
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::{NSArray, NSString};
    objc2::rc::autoreleasepool(|_| {
        let pb = NSPasteboard::generalPasteboard();
        let markers = [
            NSString::from_str("org.nspasteboard.ConcealedType"),
            NSString::from_str("org.nspasteboard.TransientType"),
            NSString::from_str("org.nspasteboard.AutoGeneratedType"),
        ];
        let arr = NSArray::from_retained_slice(&markers);
        pb.availableTypeFromArray(&arr).is_some()
    })
}

#[cfg(not(any(windows, target_os = "macos")))]
fn sensitive_clip_present() -> bool {
    false // Linux: no cross-DE convention for this yet
}

#[cfg(test)]
mod tests {
    #[test]
    fn image_clip_roundtrip_and_dedupe() {
        let conn = crate::db::open_in_memory().unwrap();
        // synthetic 64x40 RGBA capture
        let (w, h) = (64usize, 40usize);
        let rgba: Vec<u8> = (0..w * h * 4).map(|i| (i % 251) as u8).collect();
        let hash = super::sample_hash(w, h, &rgba);
        let (jpeg, thumb, iw, ih) = super::encode_clipboard_image(w, h, &rgba).unwrap();
        assert_eq!((iw, ih), (64, 40), "small captures keep their size");
        assert!(!jpeg.is_empty() && !thumb.is_empty());
        assert!(super::record_image_clip(&conn, &hash, &jpeg, &thumb, iw, ih, 100).unwrap());
        // the same capture again bumps counters instead of duplicating
        assert!(super::record_image_clip(&conn, &hash, &jpeg, &thumb, iw, ih, 200).unwrap());
        let hits = super::recent_clips(&conn, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].clip_kind, "image");
        assert_eq!(hits[0].copy_count, 2);
        assert_eq!((hits[0].width, hits[0].height), (Some(64), Some(40)));
        // stored JPEG round-trips back out (copy-back / preview path)
        let out = super::image_clip_jpeg(&conn, hits[0].id).unwrap().unwrap();
        assert_eq!(out, jpeg);
        // different pixels → different hash → second row
        let rgba2: Vec<u8> = (0..w * h * 4).map(|i| (i % 97) as u8).collect();
        assert_ne!(super::sample_hash(w, h, &rgba2), hash);
    }

    use super::*;

    #[test]
    fn record_dedupes_and_counts() {
        let conn = crate::db::open_in_memory().unwrap();
        assert!(record_clip(&conn, "  cargo build --release  ", 100, DEFAULT_MAX_LEN).unwrap());
        assert!(record_clip(&conn, "cargo build --release", 200, DEFAULT_MAX_LEN).unwrap());
        assert!(!record_clip(&conn, "ab", 300, DEFAULT_MAX_LEN).unwrap(), "too short is skipped");
        assert_eq!(clip_count(&conn).unwrap(), 1, "same text dedupes");
        let hits = recent_clips(&conn, 10).unwrap();
        assert_eq!(hits[0].copy_count, 2);
        assert_eq!(hits[0].last_copied, 200);
        assert_eq!(hits[0].first_copied, 100);
    }

    #[test]
    fn fts_and_retention() {
        let conn = crate::db::open_in_memory().unwrap();
        record_clip(&conn, "ffmpeg -i input.mp4 -c:v libx264 out.mp4", 1000, DEFAULT_MAX_LEN).unwrap();
        record_clip(&conn, "SELECT * FROM users WHERE id = 1", 2000, DEFAULT_MAX_LEN).unwrap();
        let hits = clips_fts_search(&conn, "ffmpeg", 10).unwrap();
        assert_eq!(hits.len(), 1);
        let got = clips_by_ids(&conn, &hits, &Default::default()).unwrap();
        assert!(got[0].content.contains("libx264"));

        // retention: day-old cutoff drops the older clip only
        let now = 1000 + 86_400 * 2;
        let dropped = prune_clips(&conn, 1, now).unwrap();
        assert_eq!(dropped, 2, "both clips fall outside one day");
        assert_eq!(clip_count(&conn).unwrap(), 0);

        record_clip(&conn, "keep me around please", now, DEFAULT_MAX_LEN).unwrap();
        assert_eq!(prune_clips(&conn, 0, now).unwrap(), 0, "0 = keep forever");
        assert_eq!(clip_count(&conn).unwrap(), 1);
        clear_clips(&conn).unwrap();
        assert_eq!(clip_count(&conn).unwrap(), 0);
    }

    #[test]
    fn count_cap_drops_oldest() {
        let conn = crate::db::open_in_memory().unwrap();
        for i in 0..5 {
            record_clip(&conn, &format!("clip number {i}"), 1000 + i, DEFAULT_MAX_LEN).unwrap();
        }
        assert_eq!(prune_clips_to_count(&conn, 0).unwrap(), 0, "0 = unlimited");
        assert_eq!(prune_clips_to_count(&conn, 3).unwrap(), 2);
        let kept = recent_clips(&conn, 10).unwrap();
        assert_eq!(kept.len(), 3);
        assert!(kept.iter().all(|c| c.last_copied >= 1002), "newest three kept");
    }
}
