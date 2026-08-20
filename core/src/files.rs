//! Local file indexing: only folders the user explicitly added are scanned.
//! `.gitignore` files are respected, hidden files skipped, symlinks not
//! followed — the index never leaves the allow-listed roots.

use anyhow::{bail, Result};
use ignore::WalkBuilder;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

use crate::embed::{self, Embedder};

/// Bytes read per file (binary sniff + excerpt source).
const READ_CAP: usize = 64 * 1024;
/// Chars of content stored / indexed per file.
const CONTENT_CHAR_CAP: usize = 16 * 1024;
const MAX_FILE_SIZE: u64 = 4 * 1024 * 1024;
const EMBED_BATCH: usize = 16;

/// Image formats indexed for SigLIP visual search (no text content stored).
const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp", "gif"];

const TEXT_EXTS: &[&str] = &[
    "md", "markdown", "txt", "rst", "org", "adoc", "tex", "csv", "tsv", "json", "yaml", "yml",
    "toml", "ini", "cfg", "conf", "xml", "html", "htm", "css", "scss", "less", "js", "mjs", "cjs",
    "ts", "jsx", "tsx", "rs", "py", "go", "java", "c", "cpp", "cc", "h", "hpp", "cs", "rb", "php",
    "sh", "bash", "zsh", "ps1", "psm1", "bat", "cmd", "sql", "ipynb", "vue", "svelte", "kt",
    "swift", "lua", "r", "pl", "pm", "ex", "exs", "erl", "hs", "ml", "mli", "scala", "clj",
    "cljs", "zig", "nim", "dart", "proto", "graphql", "gql", "dockerfile", "makefile", "gradle",
    "properties", "env", "log",
];

#[derive(Debug, Clone, Serialize)]
pub struct FolderInfo {
    pub id: i64,
    pub path: String,
    pub file_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileHit {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub ext: Option<String>,
    pub size: i64,
    pub mtime: i64,
    pub score: f32,
    /// Base64 JPEG thumbnail for image results (None for text files).
    pub thumb: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct IndexReport {
    pub scanned: usize,
    pub indexed: usize,
    pub removed: usize,
}

// ---------- folder management ----------

pub fn add_folder(conn: &Connection, path: &str) -> Result<i64> {
    let p = Path::new(path);
    if !p.is_dir() {
        bail!("not a directory: {path}");
    }
    let canonical = dunce_canonicalize(p)?;
    conn.execute(
        "INSERT OR IGNORE INTO folders(path) VALUES (?1)",
        [canonical.as_str()],
    )?;
    Ok(conn.query_row(
        "SELECT id FROM folders WHERE path = ?1",
        [canonical.as_str()],
        |r| r.get(0),
    )?)
}

/// Canonicalize without Windows `\\?\` prefix noise.
fn dunce_canonicalize(p: &Path) -> Result<String> {
    let c = p.canonicalize()?;
    let s = c.to_string_lossy().to_string();
    Ok(s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s))
}

pub fn remove_folder(conn: &Connection, folder_id: i64) -> Result<()> {
    // explicit deletes so the FTS sync triggers fire (FK cascade is not relied on)
    conn.execute("DELETE FROM files WHERE folder_id = ?1", [folder_id])?;
    conn.execute("DELETE FROM folders WHERE id = ?1", [folder_id])?;
    Ok(())
}

pub fn list_folders(conn: &Connection) -> Result<Vec<FolderInfo>> {
    let mut stmt = conn.prepare(
        "SELECT f.id, f.path, COUNT(fi.id) FROM folders f
         LEFT JOIN files fi ON fi.folder_id = f.id
         GROUP BY f.id ORDER BY f.path",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(FolderInfo {
                id: r.get(0)?,
                path: r.get(1)?,
                file_count: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn file_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?)
}

// ---------- indexing ----------

/// Incremental scan of all registered folders. Unchanged files (same mtime and
/// size) are skipped; files gone from disk are removed from the index.
pub fn index_folders(
    conn: &Connection,
    mut progress: impl FnMut(usize),
) -> Result<IndexReport> {
    let mut report = IndexReport::default();

    // current index state: path -> (id, mtime, size)
    let mut known: HashMap<String, (i64, i64, i64)> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT path, id, mtime, size FROM files")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?),
            ))
        })?;
        for row in rows {
            let (path, v) = row?;
            known.insert(path, v);
        }
    }

    let folders = list_folders(conn)?;
    let mut seen: Vec<String> = Vec::new();

    for folder in &folders {
        let walker = WalkBuilder::new(&folder.path)
            .follow_links(false)
            .hidden(true) // skip dotfiles
            .git_ignore(true)
            .require_git(false) // honor .gitignore even outside git repos
            .git_global(false)
            .git_exclude(true)
            .build();
        for entry in walker.flatten() {
            let Some(ft) = entry.file_type() else { continue };
            if !ft.is_file() {
                continue;
            }
            let path = entry.path();
            if !is_indexable(path) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.len() > MAX_FILE_SIZE {
                continue;
            }
            let path_str = path.to_string_lossy().to_string();
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let size = meta.len() as i64;

            report.scanned += 1;
            if report.scanned % 200 == 0 {
                progress(report.scanned);
            }
            seen.push(path_str.clone());

            if let Some((_, m, s)) = known.get(&path_str) {
                if *m == mtime && *s == size {
                    continue; // unchanged
                }
            }
            let content = if is_image(path) {
                None // pixels are indexed via SigLIP, not as text
            } else {
                match read_text_excerpt(path) {
                    Some(text) => Some(text),
                    None => continue, // binary or unreadable
                }
            };
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase());
            conn.execute(
                "INSERT INTO files(folder_id, path, name, ext, size, mtime, content)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(path) DO UPDATE SET
                    folder_id = excluded.folder_id,
                    name = excluded.name, ext = excluded.ext,
                    size = excluded.size, mtime = excluded.mtime,
                    content = excluded.content",
                params![folder.id, path_str, name, ext, size, mtime, content],
            )?;
            report.indexed += 1;
        }
    }

    // prune files that vanished from disk (or whose folder was removed)
    let seen_set: std::collections::HashSet<&str> = seen.iter().map(String::as_str).collect();
    let mut stale_ids = Vec::new();
    for (path, (id, _, _)) in &known {
        if !seen_set.contains(path.as_str()) {
            stale_ids.push(*id);
        }
    }
    for id in &stale_ids {
        conn.execute("DELETE FROM files WHERE id = ?1", [id])?;
    }
    report.removed = stale_ids.len();
    progress(report.scanned);
    Ok(report)
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .map(|e| IMAGE_EXTS.contains(&e.to_string_lossy().to_lowercase().as_str()))
        .unwrap_or(false)
}

fn is_indexable(path: &Path) -> bool {
    match path.extension() {
        Some(ext) => {
            let ext = ext.to_string_lossy().to_lowercase();
            TEXT_EXTS.contains(&ext.as_str()) || IMAGE_EXTS.contains(&ext.as_str())
        }
        // extensionless: allow well-known text files by name
        None => {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            matches!(
                name.as_str(),
                "makefile" | "dockerfile" | "readme" | "license" | "justfile" | "rakefile"
            )
        }
    }
}

/// Read up to READ_CAP bytes; reject binary (NUL byte); cap at CONTENT_CHAR_CAP chars.
fn read_text_excerpt(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = Vec::with_capacity(READ_CAP.min(8192));
    f.by_ref().take(READ_CAP as u64).read_to_end(&mut buf).ok()?;
    if buf.contains(&0) {
        return None; // binary
    }
    let text = String::from_utf8_lossy(&buf);
    Some(text.chars().take(CONTENT_CHAR_CAP).collect())
}

// ---------- embeddings ----------

pub fn file_embedding_hashes(conn: &Connection) -> Result<HashMap<i64, String>> {
    let mut stmt = conn.prepare("SELECT file_id, doc_hash FROM file_embeddings")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<HashMap<_, _>>>()?;
    Ok(rows)
}

fn compose_file_doc(name: &str, path: &str, content: &str) -> String {
    let mut doc = String::with_capacity(2200);
    doc.push_str(name);
    doc.push('\n');
    doc.push_str(path);
    doc.push('\n');
    doc.extend(content.chars().take(2000));
    doc
}

/// Embed files whose composed doc changed. Blocking; run via spawn_blocking.
pub fn embed_pending_files(
    conn: &Connection,
    embedder: &mut Embedder,
    mut progress: impl FnMut(usize, usize),
) -> Result<usize> {
    let hashes = file_embedding_hashes(conn)?;
    let pending: Vec<(i64, String, String)> = {
        let mut stmt = conn.prepare("SELECT id, name, path, content FROM files")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .filter_map(|(id, name, path, content)| {
                let doc = compose_file_doc(&name, &path, content.as_deref().unwrap_or(""));
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
                "INSERT INTO file_embeddings(file_id, doc_hash, dim, vec) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(file_id) DO UPDATE SET doc_hash = excluded.doc_hash,
                                                    dim = excluded.dim, vec = excluded.vec",
                params![id, hash, vec.len() as i64, bytes],
            )?;
        }
        done += chunk.len();
        progress(done, total);
    }
    // drop embeddings for deleted files
    conn.execute(
        "DELETE FROM file_embeddings WHERE file_id NOT IN (SELECT id FROM files)",
        [],
    )?;
    Ok(total)
}

pub fn all_file_embeddings(conn: &Connection) -> Result<Vec<(i64, Vec<f32>)>> {
    let mut stmt = conn.prepare("SELECT file_id, dim, vec FROM file_embeddings")?;
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
        let vec: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        out.push((id, vec));
    }
    Ok(out)
}

// ---------- image embeddings (SigLIP space) ----------

/// Wipe stored image vectors if they came from a different model.
pub fn ensure_image_embed_model(conn: &Connection) -> Result<()> {
    let stored = crate::db::meta_get(conn, "image_embed_model")?;
    if stored.as_deref() != Some(crate::siglip::IMAGE_EMBED_MODEL_ID) {
        conn.execute("DELETE FROM image_embeddings", [])?;
        crate::db::meta_set(conn, "image_embed_model", crate::siglip::IMAGE_EMBED_MODEL_ID)?;
    }
    Ok(())
}

fn image_doc_hash(path: &str, mtime: i64, size: i64) -> String {
    embed::doc_hash(&format!("{path}|{mtime}|{size}"))
}

/// 96px JPEG thumbnail bytes; None when encoding fails.
fn make_thumb(img: &image::DynamicImage) -> Option<Vec<u8>> {
    let thumb = img.thumbnail(96, 96).to_rgb8();
    let mut out = std::io::Cursor::new(Vec::new());
    thumb
        .write_to(&mut out, image::ImageFormat::Jpeg)
        .ok()
        .map(|_| out.into_inner())
}

pub fn image_embedding_hashes(conn: &Connection) -> Result<HashMap<i64, String>> {
    let mut stmt = conn.prepare("SELECT file_id, doc_hash FROM image_embeddings")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<HashMap<_, _>>>()?;
    Ok(rows)
}

fn put_image_embedding(conn: &Connection, file_id: i64, doc_hash: &str, vec: &[f32]) -> Result<()> {
    let bytes: Vec<u8> = vec.iter().flat_map(|f| f.to_le_bytes()).collect();
    conn.execute(
        "INSERT INTO image_embeddings(file_id, doc_hash, dim, vec) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(file_id) DO UPDATE SET doc_hash = excluded.doc_hash,
                                            dim = excluded.dim, vec = excluded.vec",
        params![file_id, doc_hash, vec.len() as i64, bytes],
    )?;
    Ok(())
}

/// All usable image vectors (dim = 0 "failed" markers are skipped).
pub fn all_image_embeddings(conn: &Connection) -> Result<Vec<(i64, Vec<f32>)>> {
    let mut stmt =
        conn.prepare("SELECT file_id, dim, vec FROM image_embeddings WHERE dim > 0")?;
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
        let vec: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        out.push((id, vec));
    }
    Ok(out)
}

/// Embed every image whose (path, mtime, size) changed. Blocking.
/// Corrupt/unreadable images get a dim-0 marker so they are not retried.
pub fn embed_pending_images(
    conn: &Connection,
    siglip: &mut crate::siglip::Siglip,
    mut progress: impl FnMut(usize, usize),
) -> Result<usize> {
    ensure_image_embed_model(conn)?;
    let hashes = image_embedding_hashes(conn)?;
    let exts = IMAGE_EXTS
        .iter()
        .map(|e| format!("'{e}'"))
        .collect::<Vec<_>>()
        .join(",");
    let pending: Vec<(i64, String, String)> = {
        let mut stmt = conn.prepare(&format!(
            "SELECT id, path, mtime, size FROM files WHERE ext IN ({exts})"
        ))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .filter_map(|(id, path, mtime, size)| {
                let hash = image_doc_hash(&path, mtime, size);
                if hashes.get(&id) == Some(&hash) {
                    None
                } else {
                    Some((id, path, hash))
                }
            })
            .collect()
    };

    let total = pending.len();
    let mut done = 0usize;
    progress(0, total);
    for (id, path, hash) in &pending {
        // decode once; the same decode feeds both the thumbnail and the model
        match image::open(Path::new(path)) {
            Ok(img) => {
                let thumb = make_thumb(&img);
                conn.execute(
                    "UPDATE files SET thumb = ?2 WHERE id = ?1",
                    params![id, thumb],
                )?;
                match siglip.embed_dynamic(img) {
                    Ok(vec) => put_image_embedding(conn, *id, hash, &vec)?,
                    Err(_) => put_image_embedding(conn, *id, hash, &[])?,
                }
            }
            Err(_) => put_image_embedding(conn, *id, hash, &[])?, // failed marker
        }
        done += 1;
        if done % 5 == 0 || done == total {
            progress(done, total);
        }
    }
    // drop vectors for deleted files
    conn.execute(
        "DELETE FROM image_embeddings WHERE file_id NOT IN (SELECT id FROM files)",
        [],
    )?;
    Ok(total)
}

// ---------- retrieval ----------

pub fn files_fts_search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<i64>> {
    let fts_query = crate::db::build_fts_query(query);
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT rowid FROM files_fts WHERE files_fts MATCH ?1
         ORDER BY bm25(files_fts, 8.0, 2.0, 1.0) LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![fts_query, limit as i64], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn row_to_hit(r: &rusqlite::Row<'_>) -> rusqlite::Result<FileHit> {
    use base64::Engine;
    let thumb: Option<Vec<u8>> = r.get(6)?;
    Ok(FileHit {
        id: r.get(0)?,
        path: r.get(1)?,
        name: r.get(2)?,
        ext: r.get(3)?,
        size: r.get(4)?,
        mtime: r.get(5)?,
        score: 0.0,
        thumb: thumb.map(|b| base64::engine::general_purpose::STANDARD.encode(b)),
    })
}

const HIT_COLS: &str = "id, path, name, ext, size, mtime, thumb";

pub fn files_by_ids(conn: &Connection, ids: &[i64], scores: &HashMap<i64, f32>) -> Result<Vec<FileHit>> {
    let mut out = Vec::with_capacity(ids.len());
    let mut stmt = conn.prepare(&format!("SELECT {HIT_COLS} FROM files WHERE id = ?1"))?;
    for id in ids {
        let hit = stmt.query_row([id], row_to_hit).optional()?;
        if let Some(mut h) = hit {
            h.score = scores.get(&h.id).copied().unwrap_or(0.0);
            out.push(h);
        }
    }
    Ok(out)
}

pub fn recent_files(conn: &Connection, limit: usize) -> Result<Vec<FileHit>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {HIT_COLS} FROM files ORDER BY mtime DESC, id DESC LIMIT ?1"
    ))?;
    let rows = stmt
        .query_map([limit as i64], row_to_hit)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Is `path` inside one of the registered folders? Guard for the open command.
pub fn path_is_allowed(conn: &Connection, path: &str) -> Result<bool> {
    let folders = list_folders(conn)?;
    let p = Path::new(path);
    Ok(folders.iter().any(|f| p.starts_with(&f.path)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn write(dir: &Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, content).unwrap();
    }

    #[test]
    fn index_respects_allowlist_and_gitignore() {
        let tmp = std::env::temp_dir().join(format!("sr-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        write(&tmp, "notes.md", "meeting notes about quarterly search index");
        write(&tmp, "code.rs", "fn main() { println!(\"scraper\"); }");
        write(&tmp, "image.png", "\u{0}\u{0}fakebinary");
        write(&tmp, ".gitignore", "secret/\n");
        write(&tmp, "secret/token.txt", "super secret credentials");

        let conn = open_in_memory().unwrap();
        add_folder(&conn, tmp.to_str().unwrap()).unwrap();
        let report = index_folders(&conn, |_| {}).unwrap();
        assert_eq!(report.indexed, 3, "md + rs + png (image, by name); secret/ gitignored");
        // the image is findable by filename but its bytes are not text-indexed
        assert_eq!(files_fts_search(&conn, "image", 10).unwrap().len(), 1);
        assert!(files_fts_search(&conn, "fakebinary", 10).unwrap().is_empty());

        let hits = files_fts_search(&conn, "quarterly", 10).unwrap();
        assert_eq!(hits.len(), 1);
        // gitignored content must NOT be findable
        assert!(files_fts_search(&conn, "credentials", 10).unwrap().is_empty());

        // corrupt image gets a dim-0 marker and is skipped by the loader
        let img_id: i64 = conn
            .query_row("SELECT id FROM files WHERE name = 'image.png'", [], |r| r.get(0))
            .unwrap();
        put_image_embedding(&conn, img_id, "h", &[]).unwrap();
        assert!(all_image_embeddings(&conn).unwrap().is_empty());
        put_image_embedding(&conn, img_id, "h2", &[0.5, 0.5]).unwrap();
        assert_eq!(all_image_embeddings(&conn).unwrap().len(), 1);

        // incremental: nothing re-indexed when unchanged
        let report2 = index_folders(&conn, |_| {}).unwrap();
        assert_eq!(report2.indexed, 0);
        assert_eq!(report2.removed, 0);

        // deletion pruned
        std::fs::remove_file(tmp.join("notes.md")).unwrap();
        let report3 = index_folders(&conn, |_| {}).unwrap();
        assert_eq!(report3.removed, 1);
        assert!(files_fts_search(&conn, "quarterly", 10).unwrap().is_empty());

        // folder removal wipes everything incl. fts
        let folders = list_folders(&conn).unwrap();
        remove_folder(&conn, folders[0].id).unwrap();
        assert_eq!(file_count(&conn).unwrap(), 0);
        assert!(files_fts_search(&conn, "scraper", 10).unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn path_allow_guard() {
        let tmp = std::env::temp_dir().join(format!("sr-guard-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let conn = open_in_memory().unwrap();
        add_folder(&conn, tmp.to_str().unwrap()).unwrap();
        let canonical = list_folders(&conn).unwrap()[0].path.clone();
        assert!(path_is_allowed(&conn, &format!("{canonical}\\a.txt")).unwrap());
        assert!(!path_is_allowed(&conn, "C:\\Windows\\system32\\config").unwrap());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn binary_rejected() {
        let tmp = std::env::temp_dir().join(format!("sr-bin-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let p = tmp.join("data.json");
        std::fs::write(&p, b"{\"a\": 1}\x00binary tail").unwrap();
        assert!(read_text_excerpt(&p).is_none());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
