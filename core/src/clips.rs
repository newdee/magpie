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
    pub score: f32,
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
    let mut stmt = conn.prepare(
        "SELECT id, content, first_copied, last_copied, copy_count FROM clips WHERE id = ?1",
    )?;
    for id in ids {
        let hit = stmt
            .query_row([id], |r| {
                Ok(ClipHit {
                    id: r.get(0)?,
                    content: r.get(1)?,
                    first_copied: r.get(2)?,
                    last_copied: r.get(3)?,
                    copy_count: r.get(4)?,
                    score: 0.0,
                })
            })
            .optional()?;
        if let Some(mut h) = hit {
            h.score = scores.get(&h.id).copied().unwrap_or(0.0);
            out.push(h);
        }
    }
    Ok(out)
}

/// Most recently copied first — what an empty query shows.
pub fn recent_clips(conn: &Connection, limit: usize) -> Result<Vec<ClipHit>> {
    let mut stmt = conn.prepare(
        "SELECT id, content, first_copied, last_copied, copy_count FROM clips
         ORDER BY last_copied DESC, id DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map([limit as i64], |r| {
            Ok(ClipHit {
                id: r.get(0)?,
                content: r.get(1)?,
                first_copied: r.get(2)?,
                last_copied: r.get(3)?,
                copy_count: r.get(4)?,
                score: 0.0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
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
