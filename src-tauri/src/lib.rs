//! Thin Tauri shell over magpie-core: window/tray/shortcut plumbing + commands.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Result;
use serde_json::json;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex as AsyncMutex;

use magpie_core::bookmarks;
use magpie_core::clips::{self, ClipHit};
use magpie_core::db;
use magpie_core::embed::Embedder;
use magpie_core::files::{self, FileHit, FolderInfo};
use magpie_core::github::GithubClient;
use magpie_core::history;
use magpie_core::search::{self, SearchResult, VectorStore};
use magpie_core::siglip::Siglip;
use magpie_core::sync;

struct AppState {
    db_path: PathBuf,
    model_dir: PathBuf,
    db: AsyncMutex<magpie_core::rusqlite::Connection>,
    /// Read-only twin of `db` for interactive searches. Separate so that
    /// cancelling a stale search can never touch a write: the interrupt
    /// handle below reaches only this connection, and WAL keeps its reads
    /// out of the writer's way.
    search_db: AsyncMutex<magpie_core::rusqlite::Connection>,
    search_interrupt: magpie_core::rusqlite::InterruptHandle,
    /// Bumped by every incoming search; a search that finds a newer ticket
    /// than its own when it reaches the head of the queue skips its work.
    search_gen: AtomicU64,
    embedder: Arc<StdMutex<Option<Embedder>>>,
    model_status: Arc<StdMutex<String>>,
    siglip: Arc<StdMutex<Option<Siglip>>>,
    siglip_status: Arc<StdMutex<String>>,
    /// All embedding vectors, resident; reloaded after every embed/sync pass.
    store: Arc<StdMutex<VectorStore>>,
    sync_running: Arc<AtomicBool>,
    local_indexing: Arc<AtomicBool>,
    video_indexing: Arc<AtomicBool>,
    /// Human-readable video-index problem ("" = fine), e.g. missing ffmpeg.
    video_note: Arc<StdMutex<String>>,
    /// ffmpeg resolution state: "" (unchecked) / "system" / "bundled" /
    /// "downloading ffmpeg… N%" / "missing: <why>".
    ffmpeg_status: Arc<StdMutex<String>>,
    /// One model init at a time: concurrent inits would race on the same
    /// .part download files. The reinit flag queues one follow-up attempt
    /// (used when the mirror changes while an init is already running).
    model_initing: Arc<AtomicBool>,
    model_reinit: Arc<AtomicBool>,
    siglip_initing: Arc<AtomicBool>,
    siglip_reinit: Arc<AtomicBool>,
    /// Installed-app list, enumerated once at startup, refreshable on demand.
    apps: Arc<StdMutex<Vec<magpie_core::apps::AppEntry>>>,
    /// Version string of a pending update ("" = none) — drives the tray
    /// badge and the extra tray menu item.
    update_badge: Arc<StdMutex<String>>,
    /// OCR engine, present only while the setting is on and init succeeded.
    ocr: Arc<StdMutex<Option<magpie_core::ocr::Ocr>>>,
    /// "" (off/unchecked) / download progress / "ready" / "failed: <why>".
    ocr_status: Arc<StdMutex<String>>,
    ocr_initing: Arc<AtomicBool>,
    ocr_reinit: Arc<AtomicBool>,
    ocr_indexing: Arc<AtomicBool>,
    /// Desired state of the clipboard watcher; the thread exits when false.
    clip_watch: Arc<AtomicBool>,
    /// Liveness guard so toggling can't stack watcher threads.
    clip_thread_alive: Arc<AtomicBool>,
}

/// Reload the resident vector store from the database.
fn reload_store(db_path: &std::path::Path, store: &Arc<StdMutex<VectorStore>>) {
    if let Ok(conn) = db::open(db_path) {
        if let Ok(fresh) = VectorStore::load(&conn) {
            *store.lock().unwrap() = fresh;
        }
    }
}

fn err_str<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Take the search connection, cancelling whatever search still runs on it.
///
/// A new keystroke makes the previous query's answer worthless, but before
/// this the query kept running anyway and held the connection: retyping
/// "vscode" could queue every later search behind an expensive single-"v"
/// scan. Interrupting makes the stale query error out and release the lock
/// within a statement step.
///
/// Returns None when a newer search overtook this one while it waited for the
/// lock: only the newest ticket does any work, the rest hand the lock straight
/// on. Interrupting with no query mid-flight is harmless — SQLite only stops
/// statements that were already running when the flag was raised.
async fn take_search_conn<'a>(
    db: &'a AsyncMutex<magpie_core::rusqlite::Connection>,
    interrupt: &magpie_core::rusqlite::InterruptHandle,
    gen: &AtomicU64,
) -> Option<tokio::sync::MutexGuard<'a, magpie_core::rusqlite::Connection>> {
    let ticket = gen.fetch_add(1, Ordering::SeqCst) + 1;
    interrupt.interrupt();
    let conn = db.lock().await;
    if gen.load(Ordering::SeqCst) != ticket {
        return None;
    }
    Some(conn)
}

/// [`take_search_conn`] with the pieces pulled out of the app state.
async fn take_state_search_conn(
    state: &AppState,
) -> Option<tokio::sync::MutexGuard<'_, magpie_core::rusqlite::Connection>> {
    take_search_conn(&state.search_db, &state.search_interrupt, &state.search_gen).await
}

// ---------- commands ----------

#[tauri::command]
async fn search_stars(
    state: State<'_, AppState>,
    query: String,
    sort: Option<search::RepoSort>,
    limit: Option<usize>,
) -> Result<Vec<SearchResult>, String> {
    let sort = sort.unwrap_or(search::RepoSort::Relevance);
    let limit = limit.unwrap_or(30).min(100);
    let qvec = if query.trim().is_empty() {
        None
    } else {
        let emb = state.embedder.clone();
        let q = query.clone();
        // try_lock: while a bulk embed pass holds the model, degrade to
        // keyword-only instead of stalling every keystroke behind the lock
        tokio::task::spawn_blocking(move || {
            emb.try_lock()
                .ok()
                .and_then(|mut guard| guard.as_mut().map(|e| e.embed_query(&q)))
        })
        .await
        .map_err(err_str)?
        .transpose()
        .map_err(err_str)?
    };
    let Some(conn) = take_state_search_conn(&state).await else {
        return Ok(Vec::new()); // superseded; the frontend drops stale answers
    };
    let store = state.store.lock().unwrap();
    let mut hits =
        search::search(&conn, &store, &query, qvec.as_deref(), sort, limit).map_err(err_str)?;
    // frecency: repos the user actually opens edge ahead at similar relevance
    if matches!(sort, search::RepoSort::Relevance) {
        let f = magpie_core::frecency::factors(&conn, "repo", unix_now()).map_err(err_str)?;
        magpie_core::frecency::boost(
            &mut hits,
            &f,
            0.01,
            |h| h.repo.id.to_string(),
            |h| h.score,
            |h, s| h.score = s,
        );
    }
    Ok(hits)
}

/// Backend halves of a settings snapshot: exportable meta keys. The GitHub
/// token is deliberately NEVER exported.
const EXPORTABLE_META: &[&str] = &[
    "app_aliases",
    "video_indexing",
    "video_decode_threads",
    "video_hwaccel",
    "ui_lang",
    "clipboard_enabled",
    "clip_retention_days",
    "clip_max_entries",
    "max_file_mb",
    "hf_endpoint",
    "ocr_enabled",
    "ocr_model",
    "ocr_pdf",
    "skip_worktrees",
    "index_threads",
];

/// Write a settings snapshot (backend meta + the frontend's localStorage
/// half, passed in) to a JSON file the user picked.
#[tauri::command]
async fn export_settings(
    state: State<'_, AppState>,
    path: String,
    frontend: serde_json::Value,
) -> Result<(), String> {
    let conn = state.db.lock().await;
    let mut meta = serde_json::Map::new();
    for key in EXPORTABLE_META {
        if let Some(v) = db::meta_get(&conn, key).map_err(err_str)? {
            meta.insert((*key).into(), serde_json::Value::String(v));
        }
    }
    let doc = json!({ "magpie_settings": 1, "meta": meta, "frontend": frontend });
    std::fs::write(&path, serde_json::to_string_pretty(&doc).map_err(err_str)?).map_err(err_str)?;
    Ok(())
}

/// Read a snapshot back: apply the meta half here, hand the frontend half
/// back for localStorage. Unknown keys are ignored; the token can't sneak in.
#[tauri::command]
async fn import_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<serde_json::Value, String> {
    let raw = std::fs::read_to_string(&path).map_err(err_str)?;
    let doc: serde_json::Value = serde_json::from_str(&raw).map_err(err_str)?;
    if doc.get("magpie_settings").and_then(|v| v.as_i64()) != Some(1) {
        return Err("not a magpie settings file".into());
    }
    let mut imported: Vec<&str> = Vec::new();
    {
        let conn = state.db.lock().await;
        if let Some(meta) = doc.get("meta").and_then(|m| m.as_object()) {
            for key in EXPORTABLE_META {
                if let Some(v) = meta.get(*key).and_then(|v| v.as_str()) {
                    db::meta_set(&conn, key, v).map_err(err_str)?;
                    imported.push(key);
                }
            }
        }
    }
    // side effects that read meta live: aliases re-attach, tray language,
    // OCR spin-up if the imported settings enable it
    spawn_app_scan(app.clone());
    if let Ok(conn) = db::open(&state.db_path) {
        if let Ok(Some(lang)) = db::meta_get(&conn, "ui_lang") {
            let _ = refresh_tray_menu(&app, &lang);
        }
        if let Ok(Some(v)) = db::meta_get(&conn, "ocr_enabled") {
            if v == "1" {
                spawn_ocr_init(app.clone());
            }
        }
    }
    // an imported thread cap applies to the loaded models now, as the
    // settings row does; an imported worktree rule needs a rescan
    if imported.contains(&"index_threads") {
        apply_index_threads(&app, state.inner());
    }
    if imported.contains(&"skip_worktrees") {
        spawn_local_index(app.clone());
    }
    Ok(doc.get("frontend").cloned().unwrap_or(json!({})))
}

/// The user opened a hit — feed the frecency stats (stable identity per
/// kind: path / url / target / repo id).
#[tauri::command]
fn record_hit_use(state: State<'_, AppState>, kind: String, key: String) -> Result<(), String> {
    let conn = db::open(&state.db_path).map_err(err_str)?;
    magpie_core::frecency::record_use(&conn, &kind, &key, unix_now()).map_err(err_str)
}

#[tauri::command]
async fn get_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let conn = state.db.lock().await;
    let count = db::repo_count(&conn).map_err(err_str)?;
    let file_count = files::file_count(&conn).map_err(err_str)?;
    let folder_count = files::folder_count(&conn).map_err(err_str)?;
    let bookmark_count = bookmarks::bookmark_count(&conn).map_err(err_str)?;
    let history_count = history::history_count(&conn).map_err(err_str)?;
    let clip_count = clips::clip_count(&conn).map_err(err_str)?;
    let clip_retention_days = db::meta_get(&conn, "clip_retention_days")
        .map_err(err_str)?
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(30);
    let clip_max_entries = db::meta_get(&conn, "clip_max_entries")
        .map_err(err_str)?
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    let last_sync = db::meta_get(&conn, "last_sync").map_err(err_str)?;
    let username = db::meta_get(&conn, "username").map_err(err_str)?;
    let has_token = db::meta_get(&conn, "token").map_err(err_str)?.is_some();
    let embedded = db::repo_chunk_hashes(&conn).map_err(err_str)?.len();
    let max_file_mb = db::meta_get(&conn, "max_file_mb")
        .map_err(err_str)?
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(files::DEFAULT_MAX_FILE_MB);
    let hotkey = db::meta_get(&conn, "hotkey")
        .map_err(err_str)?
        .unwrap_or_else(|| DEFAULT_HOTKEY.to_string());
    let hf_endpoint = db::meta_get(&conn, "hf_endpoint")
        .map_err(err_str)?
        .unwrap_or_else(|| "https://huggingface.co".to_string());
    Ok(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "repo_count": count,
        "file_count": file_count,
        "folder_count": folder_count,
        "bookmark_count": bookmark_count,
        "history_count": history_count,
        "clip_count": clip_count,
        "clipboard_enabled": state.clip_watch.load(Ordering::SeqCst),
        "clip_retention_days": clip_retention_days,
        "clip_max_entries": clip_max_entries,
        "app_aliases": db::meta_get(&conn, "app_aliases").map_err(err_str)?.unwrap_or_default(),
        "video_count": magpie_core::videos::video_count(&conn).map_err(err_str)?,
        "video_shot_count": magpie_core::videos::shot_count(&conn).map_err(err_str)?,
        "video_indexing_enabled": db::meta_get(&conn, "video_indexing")
            .map_err(err_str)?
            .map(|v| v != "0")
            .unwrap_or(true),
        "video_indexing": state.video_indexing.load(Ordering::SeqCst),
        "video_note": state.video_note.lock().unwrap().clone(),
        "ffmpeg_status": state.ffmpeg_status.lock().unwrap().clone(),
        "video_decode_threads": db::meta_get(&conn, "video_decode_threads")
            .map_err(err_str)?
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(2),
        "video_hwaccel": db::meta_get(&conn, "video_hwaccel")
            .map_err(err_str)?
            .map(|v| v == "1")
            .unwrap_or(false),
        "max_file_mb": max_file_mb,
        "hotkey": hotkey,
        "hotkey_selection": resolve_selection_hotkey(db::meta_get(&conn, "hotkey_selection").ok().flatten().as_deref()).unwrap_or_default(),
        "hotkey_selection_default": DEFAULT_SELECTION_HOTKEY,
        "skip_worktrees": db::meta_get(&conn, "skip_worktrees").ok().flatten().map(|v| v != "0").unwrap_or(true),
        "index_threads": magpie_core::threads::displayed(magpie_core::threads::setting(&conn), magpie_core::threads::cores()),
        "cpu_cores": magpie_core::threads::cores(),
        "note_path": note_path_from(&conn, &state.db_path).to_string_lossy().into_owned(),
        "hf_endpoint": hf_endpoint,
        "embedded_count": embedded,
        "last_sync": last_sync,
        "username": username,
        "has_token": has_token,
        "model": state.model_status.lock().unwrap().clone(),
        "image_model": state.siglip_status.lock().unwrap().clone(),
        "ocr_enabled": db::meta_get(&conn, "ocr_enabled")
            .map_err(err_str)?
            .map(|v| v == "1")
            .unwrap_or(false),
        "ocr_model": db::meta_get(&conn, "ocr_model")
            .map_err(err_str)?
            .unwrap_or_else(|| magpie_core::ocr::OCR_MODEL_ID.into()),
        "ocr_status": state.ocr_status.lock().unwrap().clone(),
        "ocr_pdf": db::meta_get(&conn, "ocr_pdf")
            .map_err(err_str)?
            .map(|v| v == "1")
            .unwrap_or(false),
        "syncing": state.sync_running.load(Ordering::SeqCst),
        "local_indexing": state.local_indexing.load(Ordering::SeqCst),
    }))
}

#[tauri::command]
async fn set_token(
    app: AppHandle,
    state: State<'_, AppState>,
    token: String,
) -> Result<String, String> {
    let client = GithubClient::new(&token).map_err(err_str)?;
    let login = client.viewer_login().await.map_err(err_str)?;
    {
        let conn = state.db.lock().await;
        db::meta_set(&conn, "token", token.trim()).map_err(err_str)?;
        db::meta_set(&conn, "username", &login).map_err(err_str)?;
    }
    // first token → kick off initial sync immediately
    spawn_sync(app);
    Ok(login)
}

#[tauri::command]
fn start_sync(app: AppHandle) -> Result<(), String> {
    spawn_sync(app);
    Ok(())
}

#[tauri::command]
fn open_repo(app: AppHandle, url: String) -> Result<(), String> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("only http(s) urls".into());
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_url(url, None::<&str>).map_err(err_str)
}

// ---------- local files ----------

#[tauri::command]
async fn search_local(
    state: State<'_, AppState>,
    query: String,
    scope: Option<search::LocalScope>,
    limit: Option<usize>,
) -> Result<Vec<serde_json::Value>, String> {
    let scope = scope.unwrap_or(search::LocalScope::All);
    let limit = limit.unwrap_or(30).min(100);
    // embed the words, not the filter tokens: "ext:pdf invoice" should land
    // near "invoice" in vector space, and search_files strips the same
    // tokens again for the keyword side
    let (_, text) = magpie_core::filters::parse(&query);
    let (qvec, image_qvec) = if text.trim().is_empty() {
        (None, None)
    } else {
        let emb = state.embedder.clone();
        let sig = state.siglip.clone();
        let q = text.clone();
        // try_lock: while a bulk embed pass holds a model, degrade that vector
        // list instead of stalling every keystroke behind the lock
        tokio::task::spawn_blocking(move || -> Result<_> {
            let text = emb
                .try_lock()
                .ok()
                .and_then(|mut g| g.as_mut().map(|e| e.embed_query(&q)))
                .transpose()?;
            let image = sig
                .try_lock()
                .ok()
                .and_then(|mut g| g.as_mut().map(|s| s.embed_query(&q)))
                .transpose()?;
            Ok((text, image))
        })
        .await
        .map_err(err_str)?
        .map_err(err_str)?
    };
    let Some(conn) = take_state_search_conn(&state).await else {
        return Ok(Vec::new()); // superseded; the frontend drops stale answers
    };
    let store = state.store.lock().unwrap();
    // the videos scope is its own pipeline: filename + semantic shots, fused
    if matches!(scope, search::LocalScope::Videos) {
        let vids = search::search_videos_scope(&conn, &store, &query, image_qvec.as_deref(), limit)
            .map_err(err_str)?;
        return Ok(tag_hits(Vec::new(), vids, false));
    }
    let mut files = search::search_files(
        &conn,
        &store,
        &query,
        qvec.as_deref(),
        image_qvec.as_deref(),
        scope,
        limit,
    )
    .map_err(err_str)?;
    if !query.trim().is_empty() {
        let f = magpie_core::frecency::factors(&conn, "file", unix_now()).map_err(err_str)?;
        magpie_core::frecency::boost(
            &mut files,
            &f,
            0.01,
            |h| h.path.clone(),
            |h| h.score,
            |h, s| h.score = s,
        );
    }
    Ok(tag_hits(files, Vec::new(), false))
}

/// Tag file/video hits with their kind for the frontend's mixed result list.
fn tag_hits(
    files: Vec<FileHit>,
    videos: Vec<magpie_core::videos::VideoHit>,
    interleave_by_score: bool,
) -> Vec<serde_json::Value> {
    let mut tagged: Vec<(f32, serde_json::Value)> = Vec::new();
    for f in files {
        let s = f.score;
        let mut v = serde_json::to_value(&f).unwrap_or_default();
        v["kind"] = serde_json::Value::from("file");
        tagged.push((s, v));
    }
    let file_count = tagged.len();
    for h in videos {
        let s = h.score;
        let mut v = serde_json::to_value(&h).unwrap_or_default();
        v["kind"] = serde_json::Value::from("video");
        tagged.push((s, v));
    }
    if interleave_by_score {
        // image queries: both lists are cosine similarities — comparable
        tagged.sort_by(|a, b| b.0.total_cmp(&a.0));
    } else {
        // text queries: hybrid file scores and cosine shot scores live on
        // different scales — videos append after files instead
        let _ = file_count;
    }
    tagged.into_iter().map(|(_, v)| v).collect()
}

/// Search indexed images with a query image: a dropped file (`path`) or
/// pasted clipboard bytes (`bytes_b64`). The query image itself does not need
/// to be inside an indexed folder — it is only embedded, never stored.
/// Video shots share the SigLIP space, so matching videos rank in the same
/// list with the exact time range of the best-matching shot.
#[tauri::command]
async fn search_by_image(
    state: State<'_, AppState>,
    path: Option<String>,
    bytes_b64: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<serde_json::Value>, String> {
    let limit = limit.unwrap_or(30).min(100);
    let sig = state.siglip.clone();
    let qvec = tokio::task::spawn_blocking(move || -> Result<Vec<f32>> {
        let mut guard = sig
            .try_lock()
            .map_err(|_| anyhow::anyhow!("image model is busy indexing, try again shortly"))?;
        let s = guard
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("image model not ready yet"))?;
        match (path, bytes_b64) {
            (Some(p), _) => s.embed_image(std::path::Path::new(&p)),
            (None, Some(b64)) => {
                use base64::Engine;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(b64.as_bytes())
                    .map_err(|e| anyhow::anyhow!("bad image data: {e}"))?;
                s.embed_image_bytes(&bytes)
            }
            (None, None) => Err(anyhow::anyhow!("no image given")),
        }
    })
    .await
    .map_err(err_str)?
    .map_err(err_str)?;

    let conn = state.db.lock().await;
    let store = state.store.lock().unwrap();
    let files = search::search_images(&conn, &store, &qvec, limit).map_err(err_str)?;
    let vids = search::search_video_shots(&conn, &store, &qvec, 8).map_err(err_str)?;
    Ok(tag_hits(files, vids, true))
}

/// Thumbnail of a query image for the input-row chip. Read-only, never stored.
#[tauri::command]
fn preview_thumb(path: String) -> Result<Option<String>, String> {
    Ok(files::thumb_b64_for(std::path::Path::new(&path)))
}

/// Content for the preview pane. Everything comes from the local index (file
/// text, repo metadata, video shots) or the file on disk (large image) — no
/// network. `query` centres a text preview on its first match.
#[tauri::command]
async fn get_preview(
    state: State<'_, AppState>,
    kind: String,
    id: i64,
    query: Option<String>,
) -> Result<serde_json::Value, String> {
    let conn = state.db.lock().await;
    match kind.as_str() {
        "file" => {
            let (path, ext, content): (String, Option<String>, Option<String>) = conn
                .query_row(
                    "SELECT path, ext, content FROM files WHERE id = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .map_err(err_str)?;
            if files::is_image_ext(ext.as_deref()) {
                // large preview rendered fresh from disk (index only keeps 96px)
                let b64 = tokio::task::spawn_blocking(move || {
                    files::preview_b64_for(std::path::Path::new(&path), 560)
                })
                .await
                .map_err(err_str)?;
                return Ok(json!({ "kind": "image", "image": b64 }));
            }
            let text = content.unwrap_or_default();
            if text.is_empty() {
                return Ok(json!({ "kind": "none" }));
            }
            // centre the window on the first query match when there is one
            let lower = text.to_lowercase();
            let q = query.unwrap_or_default().trim().to_lowercase();
            let hit = if q.is_empty() { None } else { lower.find(&q) };
            let (start, clipped_head) = match hit {
                Some(pos) if pos > 400 => {
                    // walk back to a char boundary near pos-400
                    let mut s = pos - 400;
                    while s > 0 && !text.is_char_boundary(s) {
                        s -= 1;
                    }
                    (s, true)
                }
                _ => (0, false),
            };
            let mut end = (start + 2400).min(text.len());
            while end < text.len() && !text.is_char_boundary(end) {
                end += 1;
            }
            Ok(json!({
                "kind": "text",
                "text": &text[start..end],
                "clipped_head": clipped_head,
                "clipped_tail": end < text.len(),
            }))
        }
        "clip" => {
            // image clips: the stored full-size JPEG; text clips carry their
            // content in the hit already
            match clips::image_clip_jpeg(&conn, id).map_err(err_str)? {
                Some(jpeg) => {
                    use base64::Engine;
                    Ok(json!({
                        "kind": "image",
                        "image": base64::engine::general_purpose::STANDARD.encode(jpeg),
                    }))
                }
                None => Ok(json!({ "kind": "none" })),
            }
        }
        "video" => {
            let mut stmt = conn
                .prepare(
                    "SELECT start_ms, end_ms, ts_ms, thumb FROM video_shots
                     WHERE file_id = ?1 ORDER BY start_ms LIMIT 60",
                )
                .map_err(err_str)?;
            let shots = stmt
                .query_map([id], |r| {
                    Ok(json!({
                        "start_ms": r.get::<_, i64>(0)?,
                        "end_ms": r.get::<_, i64>(1)?,
                        "ts_ms": r.get::<_, i64>(2)?,
                        "thumb": r.get::<_, Option<String>>(3)?,
                    }))
                })
                .map_err(err_str)?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(err_str)?;
            Ok(json!({ "kind": "shots", "shots": shots }))
        }
        "repo" => {
            let row = conn
                .query_row(
                    "SELECT description, topics, homepage, starred_at,
                            substr(COALESCE(readme, ''), 1, 2400), length(COALESCE(readme, ''))
                     FROM repos WHERE id = ?1",
                    [id],
                    |r| {
                        Ok(json!({
                            "kind": "repo",
                            "description": r.get::<_, Option<String>>(0)?,
                            "topics": r.get::<_, Option<String>>(1)?,
                            "homepage": r.get::<_, Option<String>>(2)?,
                            "starred_at": r.get::<_, Option<String>>(3)?,
                            "readme": r.get::<_, String>(4)?,
                            "readme_clipped": r.get::<_, i64>(5)? > 2400,
                        }))
                    },
                )
                .map_err(err_str)?;
            Ok(row)
        }
        _ => Ok(json!({ "kind": "none" })),
    }
}

// ---------- settings ----------

const DEFAULT_HOTKEY: &str = "Alt+Space";

/// Change the file-size cap (MB, 0 = unlimited) and rebuild the local index:
/// mtime-based increments would miss files the old cap truncated or skipped.
#[tauri::command]
async fn set_max_file_mb(
    app: AppHandle,
    state: State<'_, AppState>,
    mb: u64,
) -> Result<(), String> {
    if state.local_indexing.load(Ordering::SeqCst) {
        return Err("indexing is running; try again when it finishes".into());
    }
    {
        let conn = state.db.lock().await;
        db::meta_set(&conn, "max_file_mb", &mb.to_string()).map_err(err_str)?;
        conn.execute("DELETE FROM files", []).map_err(err_str)?;
    }
    spawn_local_index(app);
    Ok(())
}

/// The selection-search chord out of the box. A sibling of Alt+Space that no
/// OS claims: Ctrl+Alt+Space is free on Windows and Linux (JetBrains binds it
/// inside the IDE; rebindable here), while on macOS Ctrl+Option+Space is the
/// system's input-source switch, so Option+Shift+Space instead.
#[cfg(target_os = "macos")]
const DEFAULT_SELECTION_HOTKEY: &str = "Alt+Shift+Space";
#[cfg(not(target_os = "macos"))]
const DEFAULT_SELECTION_HOTKEY: &str = "Ctrl+Alt+Space";

/// Stored preference → chord to register. Nothing stored means the default;
/// an empty string is the user having removed it; anything else is theirs.
fn resolve_selection_hotkey(stored: Option<&str>) -> Option<String> {
    match stored {
        None => Some(DEFAULT_SELECTION_HOTKEY.to_string()),
        Some(s) if s.trim().is_empty() => None,
        Some(s) => Some(s.trim().to_string()),
    }
}

/// (Re)register every global chord: the summon toggle, and optionally the
/// selection search. One function because the plugin's `unregister_all` is
/// the only reliable way to drop a stale chord, so both have to be re-added
/// together whenever either changes.
fn register_hotkeys(app: &AppHandle, summon: &str, selection: Option<&str>) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(err_str)?;
    gs.on_shortcut(summon, |app, _sc, event| {
        if event.state() == ShortcutState::Pressed {
            toggle_window(app);
        }
    })
    .map_err(err_str)?;
    if let Some(sel) = selection.map(str::trim).filter(|s| !s.is_empty()) {
        gs.on_shortcut(sel, |app, _sc, event| {
            if event.state() == ShortcutState::Pressed {
                search_selection(app.clone());
            }
        })
        .map_err(err_str)?;
    }
    Ok(())
}

/// Look up whatever is selected in the frontmost app: synthesize the copy
/// chord, read the clipboard back, summon the palette with that as the
/// query. If the clipboard did not change, nothing was selected — summon
/// with an empty box rather than searching a stale clip.
fn search_selection(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let before = clips::clipboard_text().ok();
        let copied = tokio::task::spawn_blocking(|| -> Result<()> {
            use enigo::{Direction, Enigo, Key, Keyboard, Settings};
            let mut e = Enigo::new(&Settings::default()).map_err(|e| anyhow::anyhow!("input: {e}"))?;
            #[cfg(target_os = "macos")]
            let modifier = Key::Meta;
            #[cfg(not(target_os = "macos"))]
            let modifier = Key::Control;
            e.key(modifier, Direction::Press).map_err(|e| anyhow::anyhow!("{e}"))?;
            e.key(Key::Unicode('c'), Direction::Click).map_err(|e| anyhow::anyhow!("{e}"))?;
            e.key(modifier, Direction::Release).map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(())
        })
        .await;
        if !matches!(copied, Ok(Ok(()))) {
            log::warn!("selection search: could not synthesize the copy chord");
        }
        // give the frontmost app a moment to service the copy
        tokio::time::sleep(std::time::Duration::from_millis(160)).await;
        let after = clips::clipboard_text().ok();
        let query = match (after, before) {
            (Some(a), Some(b)) if a == b => String::new(),
            (Some(a), _) => a.trim().chars().take(500).collect(),
            _ => String::new(),
        };
        let _ = app.emit("search-selection", query);
        show_window(&app);
    });
}

/// Switch the model download endpoint (e.g. hf-mirror.com for regions where
/// huggingface.co is unreachable). Retries any failed model init immediately.
#[tauri::command]
async fn set_hf_endpoint(
    app: AppHandle,
    state: State<'_, AppState>,
    endpoint: String,
) -> Result<(), String> {
    let endpoint = endpoint.trim().trim_end_matches('/').to_string();
    if !endpoint.starts_with("https://") {
        return Err("endpoint must be an https:// URL".into());
    }
    {
        let conn = state.db.lock().await;
        db::meta_set(&conn, "hf_endpoint", &endpoint).map_err(err_str)?;
    }
    std::env::set_var("HF_ENDPOINT", &endpoint);
    // retry model init unless the model is already usable. If an init is
    // still running (e.g. wedged against the old endpoint), the spawn guard
    // skips and the reinit flag queues one fresh attempt for when it ends.
    if *state.model_status.lock().unwrap() != "ready" {
        state.model_reinit.store(true, Ordering::SeqCst);
        spawn_model_init(app.clone());
    }
    if *state.siglip_status.lock().unwrap() != "ready" {
        state.siglip_reinit.store(true, Ordering::SeqCst);
        spawn_siglip_init(app);
    }
    Ok(())
}

#[tauri::command]
async fn set_hotkey(
    app: AppHandle,
    state: State<'_, AppState>,
    hotkey: String,
) -> Result<(), String> {
    let hotkey = hotkey.trim().to_string();
    if hotkey.is_empty() {
        return Err("empty shortcut".into());
    }
    let (previous, selection) = {
        let conn = state.db.lock().await;
        (
            db::meta_get(&conn, "hotkey")
                .map_err(err_str)?
                .unwrap_or_else(|| DEFAULT_HOTKEY.to_string()),
            resolve_selection_hotkey(db::meta_get(&conn, "hotkey_selection").map_err(err_str)?.as_deref()),
        )
    };
    if let Err(e) = register_hotkeys(&app, &hotkey, selection.as_deref()) {
        // keep the old chord working instead of leaving nothing registered
        let _ = register_hotkeys(&app, &previous, selection.as_deref());
        return Err(format!("cannot register {hotkey:?}: {e}"));
    }
    let conn = state.db.lock().await;
    db::meta_set(&conn, "hotkey", &hotkey).map_err(err_str)?;
    Ok(())
}

/// Set (or clear, with an empty string) the chord that searches the current
/// selection. Registered alongside the summon chord.
#[tauri::command]
async fn set_selection_hotkey(
    app: AppHandle,
    state: State<'_, AppState>,
    hotkey: String,
) -> Result<(), String> {
    let hotkey = hotkey.trim().to_string();
    let (summon, previous) = {
        let conn = state.db.lock().await;
        (
            db::meta_get(&conn, "hotkey")
                .map_err(err_str)?
                .unwrap_or_else(|| DEFAULT_HOTKEY.to_string()),
            resolve_selection_hotkey(db::meta_get(&conn, "hotkey_selection").map_err(err_str)?.as_deref()),
        )
    };
    if hotkey == summon {
        return Err("that chord already summons the palette".into());
    }
    let wanted = (!hotkey.is_empty()).then_some(hotkey.as_str());
    if let Err(e) = register_hotkeys(&app, &summon, wanted) {
        let _ = register_hotkeys(&app, &summon, previous.as_deref());
        return Err(format!("cannot register {hotkey:?}: {e}"));
    }
    let conn = state.db.lock().await;
    db::meta_set(&conn, "hotkey_selection", &hotkey).map_err(err_str)?;
    Ok(())
}

/// The notes file: the user's choice, or notes.md next to the database.
fn note_path_from(conn: &magpie_core::rusqlite::Connection, db_path: &std::path::Path) -> PathBuf {
    db::meta_get(conn, "note_path")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            magpie_core::notes::default_path(db_path.parent().unwrap_or(std::path::Path::new(".")))
        })
}

/// `note buy milk` → one timestamped line appended to the notes file.
/// Returns the file it wrote to, for the confirmation row.
#[tauri::command]
async fn append_note(state: State<'_, AppState>, text: String) -> Result<String, String> {
    let path = {
        let conn = state.db.lock().await;
        note_path_from(&conn, &state.db_path)
    };
    magpie_core::notes::append(&path, &text).map_err(err_str)?;
    Ok(path.to_string_lossy().into_owned())
}

/// Choose the notes file; an empty path goes back to the default. Returns the
/// path now in effect.
#[tauri::command]
async fn set_note_path(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let conn = state.db.lock().await;
    db::meta_set(&conn, "note_path", path.trim()).map_err(err_str)?;
    Ok(note_path_from(&conn, &state.db_path).to_string_lossy().into_owned())
}

/// Open the notes file in whatever handles markdown; created empty if it does
/// not exist yet, so the click always lands somewhere.
#[tauri::command]
async fn open_note_file(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let path = {
        let conn = state.db.lock().await;
        note_path_from(&conn, &state.db_path)
    };
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(err_str)?;
        }
        std::fs::write(&path, "").map_err(err_str)?;
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(path.to_string_lossy().into_owned(), None::<&str>)
        .map_err(err_str)
}

/// What the user opened most recently from one source tab, newest first.
/// Backs the empty-query list when that setting is on. Identities come from
/// hit_stats and are resolved back into full rows; anything since deleted or
/// uninstalled simply drops out.
#[tauri::command]
async fn recent_hits(
    state: State<'_, AppState>,
    source: String,
    limit: Option<usize>,
) -> Result<Vec<serde_json::Value>, String> {
    let limit = limit.unwrap_or(12).min(50);
    let kinds: &[&str] = match source.as_str() {
        "local" => &["file", "video", "app"],
        "github-stars" => &["repo"],
        "web" => &["bookmark", "history"],
        _ => return Ok(Vec::new()),
    };
    let Some(conn) = take_state_search_conn(&state).await else {
        return Ok(Vec::new());
    };
    let keys = magpie_core::frecency::recent(&conn, kinds, limit * 2).map_err(err_str)?;
    let tag = |mut v: serde_json::Value, kind: &str| {
        v["kind"] = serde_json::Value::from(kind);
        if v.get("score").is_none() {
            v["score"] = serde_json::Value::from(0.0);
        }
        v
    };
    let mut out = Vec::with_capacity(limit);
    for (kind, key) in keys {
        let row = match kind.as_str() {
            // a video opened from a shot comes back as its file row: the shot
            // itself is not a stable identity, the path is
            "file" | "video" => files::file_by_path(&conn, &key)
                .map_err(err_str)?
                .and_then(|f| serde_json::to_value(&f).ok())
                .map(|v| tag(v, "file")),
            "app" => state
                .apps
                .lock()
                .unwrap()
                .iter()
                .find(|a| a.target == key)
                .and_then(|a| serde_json::to_value(a).ok())
                .map(|v| tag(v, "app")),
            "repo" => key
                .parse::<i64>()
                .ok()
                .and_then(|id| db::repos_by_ids(&conn, &[id]).ok())
                .and_then(|mut r| r.pop())
                .and_then(|r| serde_json::to_value(&r).ok())
                .map(|v| tag(v, "repo")),
            "bookmark" => bookmarks::bookmark_by_url(&conn, &key)
                .map_err(err_str)?
                .and_then(|b| serde_json::to_value(&b).ok())
                .map(|v| tag(v, "bookmark")),
            "history" => history::history_by_url(&conn, &key)
                .map_err(err_str)?
                .and_then(|h| serde_json::to_value(&h).ok())
                .map(|v| tag(v, "history")),
            _ => None,
        };
        if let Some(v) = row {
            out.push(v);
            if out.len() >= limit {
                break;
            }
        }
    }
    Ok(out)
}

#[tauri::command]
async fn list_folders(state: State<'_, AppState>) -> Result<Vec<FolderInfo>, String> {
    let conn = state.db.lock().await;
    files::list_folders(&conn).map_err(err_str)
}

#[tauri::command]
async fn add_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<Vec<FolderInfo>, String> {
    let out = {
        let conn = state.db.lock().await;
        files::add_folder(&conn, &path).map_err(err_str)?;
        files::list_folders(&conn).map_err(err_str)?
    };
    spawn_local_index(app);
    Ok(out)
}

#[tauri::command]
async fn remove_folder(
    state: State<'_, AppState>,
    folder_id: i64,
) -> Result<Vec<FolderInfo>, String> {
    let conn = state.db.lock().await;
    files::remove_folder(&conn, folder_id).map_err(err_str)?;
    files::list_folders(&conn).map_err(err_str)
}

#[tauri::command]
fn index_local(app: AppHandle) -> Result<(), String> {
    spawn_local_index(app);
    Ok(())
}

/// Wipe one folder's index (files, FTS, vectors, thumbnails) and re-scan it.
#[tauri::command]
async fn rebuild_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    folder_id: i64,
) -> Result<(), String> {
    if state.local_indexing.load(Ordering::SeqCst) {
        return Err("indexing is running; try again when it finishes".into());
    }
    {
        let conn = state.db.lock().await;
        files::clear_folder_files(&conn, folder_id).map_err(err_str)?;
    }
    spawn_local_index(app);
    Ok(())
}

// ---------- application launcher ----------

#[tauri::command]
fn search_apps(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
    pinyin: Option<bool>,
) -> Result<Vec<magpie_core::apps::AppEntry>, String> {
    let apps = state.apps.lock().unwrap();
    let mut hits =
        magpie_core::apps::match_apps(&apps, &query, limit.unwrap_or(4), pinyin.unwrap_or(true));
    // frecency: the app you launch daily wins ties within its match tier
    // (cap 0.08 < the 0.1 tier gaps, so exact matches stay on top)
    if let Ok(conn) = db::open(&state.db_path) {
        if let Ok(f) = magpie_core::frecency::factors(&conn, "app", unix_now()) {
            magpie_core::frecency::boost(
                &mut hits,
                &f,
                0.08,
                |h| h.target.clone(),
                |h| h.score,
                |h, s| h.score = s,
            );
        }
    }
    Ok(hits)
}

#[tauri::command]
fn launch_app(app: AppHandle, target: String) -> Result<(), String> {
    magpie_core::apps::launch_app(&target).map_err(err_str)?;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    Ok(())
}

/// Re-enumerate installed apps (e.g. after installing something new).
fn spawn_app_scan(app: AppHandle) {
    let state = app.state::<AppState>();
    let apps = state.apps.clone();
    let db_path = state.db_path.clone();
    std::thread::spawn(move || {
        let mut list = magpie_core::apps::list_apps();
        // user alias rules ("proxy = clash") ride on top of the built-ins
        if let Ok(conn) = db::open(&db_path) {
            if let Ok(Some(text)) = db::meta_get(&conn, "app_aliases") {
                let rules = magpie_core::apps::parse_alias_rules(&text);
                magpie_core::apps::apply_user_aliases(&mut list, &rules);
            }
        }
        *apps.lock().unwrap() = list;
    });
}

/// Persist the "alias = app name" rule text and re-attach aliases in place.
#[tauri::command]
fn set_app_aliases(app: AppHandle, state: State<'_, AppState>, text: String) -> Result<(), String> {
    let conn = db::open(&state.db_path).map_err(err_str)?;
    db::meta_set(&conn, "app_aliases", &text).map_err(err_str)?;
    drop(conn);
    spawn_app_scan(app);
    Ok(())
}

// ---------- video shot indexing ----------

/// Background pass: shot-detect + embed every new/changed video in the
/// indexed folders. Needs the image model; holds its lock per video (search
/// degrades to "busy indexing" exactly like bulk image embedding does).
fn spawn_video_index(app: AppHandle) {
    let state = app.state::<AppState>();
    if state.video_indexing.swap(true, Ordering::SeqCst) {
        return;
    }
    let db_path = state.db_path.clone();
    let model_dir = state.model_dir.clone();
    let siglip = state.siglip.clone();
    let store = state.store.clone();
    let running = state.video_indexing.clone();
    let note = state.video_note.clone();
    let ffmpeg_status = state.ffmpeg_status.clone();
    std::thread::spawn(move || {
        let result = (|| -> Result<usize> {
            let conn = db::open(&db_path)?;
            let enabled = db::meta_get(&conn, "video_indexing")
                .ok()
                .flatten()
                .map(|v| v != "0")
                .unwrap_or(true);
            if !enabled {
                return Ok(0);
            }
            let pending = magpie_core::videos::pending_videos(&conn)?;
            magpie_core::videos::prune_orphan_shots(&conn)?;
            if pending.is_empty() {
                return Ok(0);
            }
            // resolve ffmpeg once per pass (may download a static build);
            // the settings row mirrors progress via ffmpeg_status
            let fs2 = ffmpeg_status.clone();
            let (_, label) = magpie_core::videos::ensure_ffmpeg_with(&model_dir, &mut move |m| {
                *fs2.lock().unwrap() = m;
            })?;
            log::info!("ffmpeg resolved: {label}");
            *ffmpeg_status.lock().unwrap() = label.to_string();
            let total = pending.len();
            let mut done = 0usize;
            for (file_id, path, mtime) in pending {
                let _ = app.emit(
                    "local-progress",
                    json!({ "stage": "videos", "done": done, "total": total }),
                );
                // hold the model only per video; skip (retry next pass) if a
                // bulk image embed owns it right now
                let opts = decode_opts_from_meta(&conn);
                let indexed = match siglip.try_lock() {
                    Ok(mut guard) => match guard.as_mut() {
                        Some(s) => {
                            magpie_core::videos::index_video(&conn, s, file_id, &path, mtime, opts)
                        }
                        None => return Ok(done), // model gone (reinit) — stop quietly
                    },
                    Err(_) => continue,
                };
                match indexed {
                    Ok(_) => done += 1,
                    Err(e) => {
                        // a broken file must not wedge the queue: record the
                        // attempt at this mtime and move on
                        log::warn!("video index {path}: {e}");
                        let _ = conn.execute(
                            "INSERT INTO video_index (file_id, mtime, duration_ms, shot_count, indexed_at)
                             VALUES (?1, ?2, 0, 0, strftime('%s','now'))
                             ON CONFLICT(file_id) DO UPDATE SET mtime = excluded.mtime",
                            magpie_core::rusqlite::params![file_id, mtime],
                        );
                    }
                }
            }
            Ok(done)
        })();
        match result {
            Ok(n) => {
                *note.lock().unwrap() = String::new();
                if n > 0 {
                    reload_store(&db_path, &store);
                    let _ = app.emit("local-done", json!({ "videos": n }));
                    // fresh shots may need frame OCR (no-op when OCR is off)
                    spawn_ocr_index(app.clone());
                }
            }
            Err(e) => {
                *note.lock().unwrap() = e.to_string();
                log::warn!("video indexing failed: {e:#}");
                let _ = app.emit("local-error", format!("video indexing: {e}"));
            }
        }
        running.store(false, Ordering::SeqCst);
    });
}

/// Resolve ffmpeg early (system → magpie release asset → upstream) when the
/// feature is on and the folders actually contain videos, so the binary is
/// ready before the first index pass. Progress lands in ffmpeg_status.
fn spawn_ffmpeg_check(app: AppHandle) {
    let state = app.state::<AppState>();
    let db_path = state.db_path.clone();
    let model_dir = state.model_dir.clone();
    let status = state.ffmpeg_status.clone();
    std::thread::spawn(move || {
        let has_videos = (|| -> Result<bool> {
            let conn = db::open(&db_path)?;
            let enabled = db::meta_get(&conn, "video_indexing")?
                .map(|v| v != "0")
                .unwrap_or(true);
            if !enabled {
                return Ok(false);
            }
            let exts = magpie_core::videos::VIDEO_EXTS
                .iter()
                .map(|e| format!("'{e}'"))
                .collect::<Vec<_>>()
                .join(",");
            Ok(conn.query_row(
                &format!("SELECT EXISTS(SELECT 1 FROM files WHERE lower(ext) IN ({exts}))"),
                [],
                |r| r.get(0),
            )?)
        })()
        .unwrap_or(false);
        if !has_videos {
            return; // nothing to decode — never download 80 MB for nothing
        }
        let status2 = status.clone();
        match magpie_core::videos::ensure_ffmpeg_with(&model_dir, &mut move |msg| {
            *status2.lock().unwrap() = msg;
        }) {
            Ok((_, label)) => *status.lock().unwrap() = label.to_string(),
            Err(e) => *status.lock().unwrap() = format!("missing: {e}"),
        }
    });
}

/// Decode limits from meta (threads default 2, hwaccel default off).
fn decode_opts_from_meta(conn: &magpie_core::rusqlite::Connection) -> magpie_core::videos::DecodeOpts {
    let threads = db::meta_get(conn, "video_decode_threads")
        .ok()
        .flatten()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(2);
    let hwaccel = db::meta_get(conn, "video_hwaccel")
        .ok()
        .flatten()
        .map(|v| v == "1")
        .unwrap_or(false);
    magpie_core::videos::DecodeOpts { threads, hwaccel }
}

/// Persist decode limits; they apply from the next decode onwards.
#[tauri::command]
fn set_video_decode(state: State<'_, AppState>, threads: u32, hwaccel: bool) -> Result<(), String> {
    let conn = db::open(&state.db_path).map_err(err_str)?;
    db::meta_set(&conn, "video_decode_threads", &threads.to_string()).map_err(err_str)?;
    db::meta_set(&conn, "video_hwaccel", if hwaccel { "1" } else { "0" }).map_err(err_str)?;
    Ok(())
}

/// Toggle video shot indexing; enabling kicks a pass immediately.
#[tauri::command]
fn set_video_indexing(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let conn = db::open(&state.db_path).map_err(err_str)?;
    db::meta_set(&conn, "video_indexing", if enabled { "1" } else { "0" }).map_err(err_str)?;
    drop(conn);
    if enabled {
        spawn_ffmpeg_check(app.clone());
        spawn_video_index(app);
    }
    Ok(())
}

/// Prune linked git worktrees whose main checkout is indexed (default on).
/// Either way the next scan applies it: switching on drops the worktrees'
/// rows as unseen files, switching off walks them again.
#[tauri::command]
fn set_skip_worktrees(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let conn = db::open(&state.db_path).map_err(err_str)?;
    db::meta_set(&conn, "skip_worktrees", if enabled { "1" } else { "0" }).map_err(err_str)?;
    drop(conn);
    spawn_local_index(app);
    Ok(())
}

/// The thread cap the model sessions load with (see `magpie_core::threads`).
/// A database that will not open falls back to the default cap rather than
/// to every core: the polite choice is the safe one.
fn index_threads(db_path: &std::path::Path) -> usize {
    db::open(db_path)
        .map(|c| magpie_core::threads::from_meta(&c))
        .unwrap_or(magpie_core::threads::DEFAULT)
}

/// Re-create a loaded model so a load-time setting (the thread cap) applies
/// now rather than after a restart. A load in flight gets one rerun queued
/// through its reinit flag; a model that is off or failed is left alone and
/// picks the setting up whenever it next loads.
///
/// `stop` runs first on both paths, before anything is spawned or queued,
/// so that a load is always in flight once the passes have been told to
/// stop: every load ends in a resume, and a stop can never be left behind
/// with nobody to lift it. Returns whether a reload was started or queued.
fn reload_loaded(
    initing: &AtomicBool,
    reinit: &AtomicBool,
    status: &StdMutex<String>,
    stop: impl FnOnce(),
    spawn: impl FnOnce(),
) -> bool {
    if initing.load(Ordering::SeqCst) {
        stop();
        reinit.store(true, Ordering::SeqCst);
        // the load may have ended between the check and the flag, in which
        // case nobody is left to consume it: consume it here
        if !initing.load(Ordering::SeqCst) && reinit.swap(false, Ordering::SeqCst) {
            spawn();
        }
        return true;
    }
    if *status.lock().unwrap() == "ready" {
        stop();
        spawn();
        return true;
    }
    false
}

/// Cap the CPU threads each model session (text, image, OCR) may use while
/// indexing; 0 = every core. Persisted, then the loaded models are reloaded
/// so the cap applies to the next embed pass, not to the next launch.
#[tauri::command]
async fn set_index_threads(
    app: AppHandle,
    state: State<'_, AppState>,
    threads: u32,
) -> Result<(), String> {
    {
        let conn = state.db.lock().await;
        db::meta_set(&conn, magpie_core::threads::META_KEY, &threads.to_string())
            .map_err(err_str)?;
    }
    apply_index_threads(&app, state.inner());
    Ok(())
}

/// Reload whichever models are loaded so the stored thread cap applies now.
/// The text and image passes hold their model for a whole run, so a reload
/// on its way also tells those passes to end at their next item: the new
/// session swaps in within seconds instead of after the run, and the
/// reload's catch-up pass carries on from where the run stopped. OCR takes
/// its engine per item, so a swap there needs no such help.
fn apply_index_threads(app: &AppHandle, state: &AppState) {
    use magpie_core::threads::{self as th, Model};
    reload_loaded(
        &state.model_initing,
        &state.model_reinit,
        &state.model_status,
        || th::stop(Model::Text),
        || spawn_model_init(app.clone()),
    );
    reload_loaded(
        &state.siglip_initing,
        &state.siglip_reinit,
        &state.siglip_status,
        || th::stop(Model::Image),
        || spawn_siglip_init(app.clone()),
    );
    reload_loaded(
        &state.ocr_initing,
        &state.ocr_reinit,
        &state.ocr_status,
        || {},
        || spawn_ocr_init(app.clone()),
    );
}

// ---------- clipboard history ----------

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[tauri::command]
async fn search_clips(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<ClipHit>, String> {
    let limit = limit.unwrap_or(50);
    let qvec = if query.trim().is_empty() {
        None
    } else {
        match state.embedder.try_lock() {
            Ok(mut guard) => match guard.as_mut() {
                Some(e) => e.embed_query(&query).ok(),
                None => None,
            },
            Err(_) => None, // bulk embed in progress; keyword-only is fine
        }
    };
    // image clips answer to descriptions via SigLIP's text encoder
    let image_qvec = if query.trim().is_empty() {
        None
    } else {
        match state.siglip.try_lock() {
            Ok(mut guard) => guard.as_mut().and_then(|s| s.embed_query(&query).ok()),
            Err(_) => None,
        }
    };
    let Some(conn) = take_state_search_conn(&state).await else {
        return Ok(Vec::new()); // superseded; the frontend drops stale answers
    };
    let store = state.store.lock().unwrap();
    search::search_clips(&conn, &store, &query, qvec.as_deref(), image_qvec.as_deref(), limit)
        .map_err(err_str)
}

/// Copy an image clip back to the clipboard.
#[tauri::command]
async fn copy_image_clip(state: State<'_, AppState>, clip_id: i64) -> Result<(), String> {
    let jpeg = {
        let conn = state.db.lock().await;
        clips::image_clip_jpeg(&conn, clip_id)
            .map_err(err_str)?
            .ok_or("image clip not found")?
    };
    tokio::task::spawn_blocking(move || clips::set_clipboard_image(&jpeg))
        .await
        .map_err(err_str)?
        .map_err(err_str)
}

#[tauri::command]
async fn set_clipboard_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    {
        let conn = state.db.lock().await;
        db::meta_set(&conn, "clipboard_enabled", if enabled { "1" } else { "0" })
            .map_err(err_str)?;
    }
    state.clip_watch.store(enabled, Ordering::SeqCst);
    if enabled {
        spawn_clip_watcher(app);
    }
    Ok(())
}

#[tauri::command]
async fn set_clip_retention(state: State<'_, AppState>, days: u32) -> Result<(), String> {
    let conn = state.db.lock().await;
    db::meta_set(&conn, "clip_retention_days", &days.to_string()).map_err(err_str)?;
    let now = unix_now();
    clips::prune_clips(&conn, days, now).map_err(err_str)?;
    Ok(())
}

#[tauri::command]
async fn set_clip_max_entries(state: State<'_, AppState>, entries: u32) -> Result<(), String> {
    let conn = state.db.lock().await;
    db::meta_set(&conn, "clip_max_entries", &entries.to_string()).map_err(err_str)?;
    clips::prune_clips_to_count(&conn, entries).map_err(err_str)?;
    Ok(())
}

#[tauri::command]
async fn delete_clip(state: State<'_, AppState>, clip_id: i64) -> Result<(), String> {
    let conn = state.db.lock().await;
    clips::delete_clip(&conn, clip_id).map_err(err_str)?;
    Ok(())
}

#[tauri::command]
async fn clear_clips_now(state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().await;
    clips::clear_clips(&conn).map_err(err_str)?;
    Ok(())
}

#[tauri::command]
fn copy_clip(text: String) -> Result<(), String> {
    clips::set_clipboard_text(&text).map_err(err_str)
}

/// Copy a clip AND paste it into the app the user came from: hide the
/// palette, wait for focus to return to the previous window, then synthesize
/// the platform paste chord.
#[tauri::command]
async fn paste_clip(app: AppHandle, text: String) -> Result<(), String> {
    clips::set_clipboard_text(&text).map_err(err_str)?;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    tokio::time::sleep(std::time::Duration::from_millis(220)).await;
    tokio::task::spawn_blocking(|| -> Result<()> {
        use enigo::{Direction, Enigo, Key, Keyboard, Settings};
        let mut e = Enigo::new(&Settings::default()).map_err(|e| anyhow::anyhow!("input: {e}"))?;
        #[cfg(target_os = "macos")]
        let modifier = Key::Meta;
        #[cfg(not(target_os = "macos"))]
        let modifier = Key::Control;
        e.key(modifier, Direction::Press).map_err(|e| anyhow::anyhow!("{e}"))?;
        e.key(Key::Unicode('v'), Direction::Click).map_err(|e| anyhow::anyhow!("{e}"))?;
        e.key(modifier, Direction::Release).map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(())
    })
    .await
    .map_err(err_str)?
    .map_err(err_str)
}

/// Seek arguments for known players, by executable stem. Pure — unit tested.
fn player_seek_args(exe_stem: &str, path: &str, ts_ms: i64) -> Option<Vec<String>> {
    let secs = ts_ms as f64 / 1000.0;
    let (h, rem) = (ts_ms / 3_600_000, ts_ms % 3_600_000);
    let (m, s) = (rem / 60_000, (rem % 60_000) / 1000);
    match exe_stem.to_lowercase().as_str() {
        "vlc" => Some(vec![format!("--start-time={secs:.1}"), path.into()]),
        "mpv" | "iina" => Some(vec![format!("--start={secs:.1}"), path.into()]),
        st if st.starts_with("potplayer") => {
            Some(vec![path.into(), format!("/seek={h}:{m:02}:{s:02}")])
        }
        st if st.starts_with("mpc-hc") || st.starts_with("mpc-be") => {
            Some(vec![path.into(), "/start".into(), ts_ms.to_string()])
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::player_seek_args;

    #[test]
    fn seek_args_per_player() {
        let p = "C:\\v\\a.mp4";
        assert_eq!(
            player_seek_args("vlc", p, 204_500).unwrap(),
            vec!["--start-time=204.5".to_string(), p.to_string()]
        );
        assert_eq!(
            player_seek_args("mpv", p, 5_000).unwrap()[0],
            "--start=5.0"
        );
        assert_eq!(
            player_seek_args("PotPlayerMini64", p, 3_725_000).unwrap()[1],
            "/seek=1:02:05"
        );
        assert_eq!(
            player_seek_args("mpc-hc64", p, 1500).unwrap()[2],
            "1500"
        );
        assert!(player_seek_args("wmplayer", p, 0).is_none(), "no seek interface");
    }
}

/// Play a video at a timestamp with the SYSTEM DEFAULT player when it has a
/// seek interface (VLC / mpv / PotPlayer / MPC — resolved from the user's own
/// file association, never overriding their choice); otherwise a plain open.
#[tauri::command]
fn play_video(path: String, ts_ms: i64) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let assoc = (|| -> Option<(String, Vec<String>)> {
            use winreg::enums::{HKEY_CLASSES_ROOT, HKEY_CURRENT_USER};
            use winreg::RegKey;
            let ext = std::path::Path::new(&path).extension()?.to_str()?.to_lowercase();
            let prog_id: String = RegKey::predef(HKEY_CURRENT_USER)
                .open_subkey(format!(
                    "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.{ext}\\UserChoice"
                ))
                .ok()?
                .get_value("ProgId")
                .ok()?;
            let cmd: String = RegKey::predef(HKEY_CLASSES_ROOT)
                .open_subkey(format!("{prog_id}\\shell\\open\\command"))
                .ok()?
                .get_value("")
                .ok()?;
            // first token: quoted or bare exe path
            let exe = if let Some(rest) = cmd.strip_prefix('"') {
                rest.split('"').next()?.to_string()
            } else {
                cmd.split_whitespace().next()?.to_string()
            };
            let stem = std::path::Path::new(&exe).file_stem()?.to_str()?.to_string();
            let args = player_seek_args(&stem, &path, ts_ms)?;
            Some((exe, args))
        })();
        if let Some((exe, args)) = assoc {
            return std::process::Command::new(exe)
                .args(args)
                .spawn()
                .map(|_| ())
                .map_err(err_str);
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let desktop = std::process::Command::new("xdg-mime")
            .args(["query", "default", "video/mp4"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase())
            .unwrap_or_default();
        for player in ["mpv", "vlc"] {
            if desktop.contains(player) {
                if let Some(args) = player_seek_args(player, &path, ts_ms) {
                    if std::process::Command::new(player).args(&args).spawn().is_ok() {
                        return Ok(());
                    }
                }
            }
        }
    }
    // macOS and unknown players: plain open with the default app (no seek —
    // stock players expose no public jump interface)
    let _ = ts_ms;
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(err_str)
}

/// Poll the clipboard once a second while enabled; embed new clips inline
/// when the model is free. Owns its DB connection.
fn spawn_clip_watcher(app: AppHandle) {
    let state = app.state::<AppState>();
    if state.clip_thread_alive.swap(true, Ordering::SeqCst) {
        return; // already running; it reads clip_watch every tick
    }
    let run = state.clip_watch.clone();
    let alive = state.clip_thread_alive.clone();
    let db_path = state.db_path.clone();
    let embedder = state.embedder.clone();
    let siglip = state.siglip.clone();
    let store = state.store.clone();
    std::thread::spawn(move || {
        let result = (|| -> Result<()> {
            let conn = db::open(&db_path)?;
            let mut watcher = clips::ClipboardWatcher::new()?;
            let mut ticks: u64 = 0;
            while run.load(Ordering::SeqCst) {
                let mut recorded = false;
                if let Some(text) = watcher.poll() {
                    let now = unix_now();
                    if clips::record_clip(&conn, &text, now, clips::DEFAULT_MAX_LEN)
                        .unwrap_or(false)
                    {
                        recorded = true;
                        // embed immediately when the model isn't busy
                        if let Ok(mut guard) = embedder.try_lock() {
                            if let Some(e) = guard.as_mut() {
                                let _ = clips::embed_pending_clips(&conn, e, |_, _| {});
                            }
                        }
                    }
                } else if let Some((w, h, rgba)) = watcher.poll_image() {
                    // screenshots and copied images join the history too,
                    // searchable by describing what's in them
                    let hash = clips::sample_hash(w, h, &rgba);
                    if let Some((jpeg, thumb, iw, ih)) = clips::encode_clipboard_image(w, h, &rgba)
                    {
                        if clips::record_image_clip(&conn, &hash, &jpeg, &thumb, iw, ih, unix_now())
                            .unwrap_or(false)
                        {
                            recorded = true;
                            if let Ok(mut guard) = siglip.try_lock() {
                                if let Some(s) = guard.as_mut() {
                                    let _ = clips::embed_pending_image_clips(&conn, s);
                                }
                            }
                        }
                    }
                }
                if recorded {
                    let cap = db::meta_get(&conn, "clip_max_entries")
                        .ok()
                        .flatten()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0u32);
                    let _ = clips::prune_clips_to_count(&conn, cap);
                    reload_store(&db_path, &store);
                }
                ticks += 1;
                if ticks.is_multiple_of(3600) {
                    let days = db::meta_get(&conn, "clip_retention_days")
                        .ok()
                        .flatten()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(30u32);
                    let _ = clips::prune_clips(&conn, days, unix_now());
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            Ok(())
        })();
        if let Err(e) = result {
            log::error!("clipboard watcher stopped: {e}");
        }
        alive.store(false, Ordering::SeqCst);
    });
}

// ---------- bookmarks ----------

/// Unified web search over bookmarks and/or history. `scope` is one of
/// "all" | "bookmarks" | "history"; results are tagged and merged by score.
#[tauri::command]
async fn search_web(
    state: State<'_, AppState>,
    query: String,
    scope: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<serde_json::Value>, String> {
    let scope = scope.unwrap_or_else(|| "all".into());
    let limit = limit.unwrap_or(40);
    let qvec = if query.trim().is_empty() {
        None
    } else {
        match state.embedder.try_lock() {
            Ok(mut g) => g.as_mut().and_then(|e| e.embed_query(&query).ok()),
            Err(_) => None,
        }
    };
    let Some(conn) = take_state_search_conn(&state).await else {
        return Ok(Vec::new()); // superseded; the frontend drops stale answers
    };
    let store = state.store.lock().unwrap();
    let fb = magpie_core::frecency::factors(&conn, "bookmark", unix_now()).unwrap_or_default();
    let fh = magpie_core::frecency::factors(&conn, "history", unix_now()).unwrap_or_default();
    let mut out: Vec<(f32, serde_json::Value)> = Vec::new();
    if scope != "history" {
        for b in search::search_bookmarks(&conn, &store, &query, qvec.as_deref(), limit)
            .map_err(err_str)?
        {
            // curated bookmarks get a small edge over raw history at a tie;
            // frecency nudges the ones the user actually reopens
            let bonus = 0.01 * fb.get(&b.url).copied().unwrap_or(0.0);
            let mut v = serde_json::to_value(&b).map_err(err_str)?;
            v["kind"] = json!("bookmark");
            out.push((b.score + 0.05 + bonus, v));
        }
    }
    if scope != "bookmarks" {
        for h in search::search_history(&conn, &store, &query, qvec.as_deref(), limit)
            .map_err(err_str)?
        {
            let bonus = 0.01 * fh.get(&h.url).copied().unwrap_or(0.0);
            let mut v = serde_json::to_value(&h).map_err(err_str)?;
            v["kind"] = json!("history");
            out.push((h.score + bonus, v));
        }
    }
    out.sort_by(|a, b| b.0.total_cmp(&a.0));
    Ok(out.into_iter().take(limit).map(|(_, v)| v).collect())
}

/// Re-read every browser's bookmark and history store and refresh vectors.
fn spawn_bookmark_sync(app: AppHandle) {
    let state = app.state::<AppState>();
    let embedder = state.embedder.clone();
    let db_path = state.db_path.clone();
    tauri::async_runtime::spawn(async move {
        let outcome = tokio::task::spawn_blocking(move || -> Result<serde_json::Value> {
            let conn = db::open(&db_path)?;
            let report = bookmarks::sync_bookmarks(&conn)?;
            let hist_report = history::sync_history(&conn)?;
            let embedded = match embedder.try_lock() {
                Ok(mut guard) => match guard.as_mut() {
                    Some(e) => {
                        let b = bookmarks::embed_pending_bookmarks(&conn, e, |_, _| {})?;
                        let h = history::embed_pending_history(&conn, e, |_, _| {})?;
                        b + h
                    }
                    None => 0, // model not ready; catch-up covers it
                },
                Err(_) => 0, // busy embedding elsewhere; next sync catches up
            };
            Ok(json!({ "report": report, "history": hist_report, "embedded": embedded }))
        })
        .await;
        match outcome {
            Ok(Ok(v)) => {
                {
                    let state = app.state::<AppState>();
                    reload_store(&state.db_path, &state.store);
                }
                let _ = app.emit("bookmarks-done", v);
            }
            Ok(Err(e)) => {
                log::warn!("bookmark sync failed: {e:#}");
                let _ = app.emit("bookmarks-error", e.to_string());
            }
            Err(e) => {
                log::warn!("bookmark sync task panicked: {e}");
                let _ = app.emit("bookmarks-error", e.to_string());
            }
        }
    });
}

#[tauri::command]
fn sync_bookmarks_now(app: AppHandle) -> Result<(), String> {
    spawn_bookmark_sync(app);
    Ok(())
}

/// Wipe the whole star index (repos, READMEs, vectors) and sync from zero.
#[tauri::command]
async fn rebuild_stars(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if state.sync_running.load(Ordering::SeqCst) {
        return Err("sync is running; try again when it finishes".into());
    }
    {
        let conn = state.db.lock().await;
        conn.execute("DELETE FROM repos", []).map_err(err_str)?;
    }
    spawn_sync(app);
    Ok(())
}

#[tauri::command]
async fn open_file(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let allowed = {
        let conn = state.db.lock().await;
        files::path_is_allowed(&conn, &path).map_err(err_str)?
    };
    if !allowed {
        return Err("path is outside indexed folders".into());
    }
    // reveal in Explorer / Finder rather than executing the file
    tauri_plugin_opener::reveal_item_in_dir(&path).map_err(err_str)
}

fn spawn_local_index(app: AppHandle) {
    let state = app.state::<AppState>();
    if state.local_indexing.swap(true, Ordering::SeqCst) {
        return;
    }
    let running = state.local_indexing.clone();
    let embedder = state.embedder.clone();
    let siglip = state.siglip.clone();
    let db_path = state.db_path.clone();
    let store = state.store.clone();
    tauri::async_runtime::spawn(async move {
        let app2 = app.clone();
        let store_path = db_path.clone();
        let outcome = tokio::task::spawn_blocking(move || -> Result<serde_json::Value> {
            let conn = db::open(&db_path)?;
            let scan_app = app2.clone();
            let report = files::index_folders(&conn, move |scanned| {
                let _ = scan_app.emit("local-progress", json!({ "stage": "scan", "done": scanned }));
            })?;
            let embedded = match embedder.lock().unwrap().as_mut() {
                Some(e) => files::embed_pending_files(&conn, e, |done, total| {
                    let _ = app2.emit(
                        "local-progress",
                        json!({ "stage": "embed", "done": done, "total": total }),
                    );
                })?,
                None => 0,
            };
            let images = match siglip.lock().unwrap().as_mut() {
                Some(s) => files::embed_pending_images(&conn, s, |done, total| {
                    let _ = app2.emit(
                        "local-progress",
                        json!({ "stage": "embed-images", "done": done, "total": total }),
                    );
                })?,
                None => 0, // siglip not ready; catch-up runs after its init
            };
            Ok(json!({ "report": report, "embedded": embedded, "images": images }))
        })
        .await;
        reload_store(&store_path, &store);
        running.store(false, Ordering::SeqCst);
        match outcome {
            Ok(Ok(v)) => {
                let _ = app.emit("local-done", v);
            }
            Ok(Err(e)) => {
                log::warn!("local index failed: {e:#}");
                let _ = app.emit("local-error", e.to_string());
            }
            Err(e) => {
                log::warn!("local index task panicked: {e}");
                let _ = app.emit("local-error", e.to_string());
            }
        }
    });
}

// ---------- sync orchestration ----------

fn spawn_sync(app: AppHandle) {
    let state = app.state::<AppState>();
    if state.sync_running.swap(true, Ordering::SeqCst) {
        return; // already running
    }
    let running = state.sync_running.clone();
    let embedder = state.embedder.clone();
    let db_path = state.db_path.clone();
    let store = state.store.clone();
    tauri::async_runtime::spawn(async move {
        let outcome = run_sync(app.clone(), db_path.clone(), embedder).await;
        reload_store(&db_path, &store);
        running.store(false, Ordering::SeqCst);
        match outcome {
            Ok(v) => {
                let _ = app.emit("sync-done", v);
            }
            Err(e) => {
                log::warn!("star sync failed: {e:#}");
                let _ = app.emit("sync-error", e.to_string());
            }
        }
    });
}

async fn run_sync(
    app: AppHandle,
    db_path: PathBuf,
    embedder: Arc<StdMutex<Option<Embedder>>>,
) -> Result<serde_json::Value> {
    // own connection: WAL lets the search connection keep reading meanwhile
    let mut conn = db::open(&db_path)?;
    let Some(token) = db::meta_get(&conn, "token")? else {
        // fresh install, nothing to sync yet — not an error
        return Ok(json!({ "skipped": "no_token" }));
    };
    let client = GithubClient::new(&token)?;

    let progress_app = app.clone();
    let report = sync::sync(&mut conn, &client, move |p| {
        let _ = progress_app.emit("sync-progress", &p);
    })
    .await?;
    drop(conn);

    let embedded = {
        let progress_app = app.clone();
        tokio::task::spawn_blocking(move || -> Result<usize> {
            let conn = db::open(&db_path)?;
            let mut guard = embedder.lock().unwrap();
            match guard.as_mut() {
                Some(e) => sync::embed_pending(&conn, e, |p| {
                    let _ = progress_app.emit("sync-progress", &p);
                }),
                None => Ok(0), // model not ready; embed_pending reruns after init
            }
        })
        .await??
    };

    Ok(json!({ "report": report, "embedded": embedded }))
}

/// Load the embedding model in the background, then embed anything pending.
fn spawn_model_init(app: AppHandle) {
    let state = app.state::<AppState>();
    if state.model_initing.swap(true, Ordering::SeqCst) {
        return; // an init is already running; model_reinit may queue a rerun
    }
    let embedder = state.embedder.clone();
    let status = state.model_status.clone();
    let model_dir = state.model_dir.clone();
    let db_path = state.db_path.clone();
    let store = state.store.clone();
    let initing = state.model_initing.clone();
    let reinit = state.model_reinit.clone();
    let threads = index_threads(&db_path);
    tauri::async_runtime::spawn(async move {
        *status.lock().unwrap() = "loading".into();
        let _ = app.emit("model-status", "loading");
        let init = tokio::task::spawn_blocking({
            let model_dir = model_dir.clone();
            let status = status.clone();
            let app = app.clone();
            move || {
                let mut last = String::new();
                Embedder::new_with_fallback(&model_dir, threads, &mut |msg| {
                    // percent strings repeat per read; only surface changes
                    if msg != last {
                        last = msg.clone();
                        *status.lock().unwrap() = msg.clone();
                        let _ = app.emit("model-status", msg);
                    }
                })
            }
        })
        .await;
        match init {
            Ok(Ok(e)) => {
                *embedder.lock().unwrap() = Some(e);
                *status.lock().unwrap() = "ready".into();
                log::info!("semantic model ready ({threads} threads)");
                let _ = app.emit("model-status", "ready");
                initing.store(false, Ordering::SeqCst);
                // a load-time setting (thread cap, mirror) changed while this
                // load ran: load once more before catching up, so the
                // catch-up runs on the final session. The passes stay
                // stopped until that load has swapped its session in.
                if reinit.swap(false, Ordering::SeqCst) {
                    spawn_model_init(app.clone());
                    return;
                }
                magpie_core::threads::resume(magpie_core::threads::Model::Text);
                // catch up on vectors for repos/files ingested while the model was absent
                let app2 = app.clone();
                let catchup_path = db_path.clone();
                let done = tokio::task::spawn_blocking(move || -> Result<usize> {
                    let conn = db::open(&catchup_path)?;
                    let mut guard = embedder.lock().unwrap();
                    match guard.as_mut() {
                        Some(e) => {
                            let repos = sync::embed_pending(&conn, e, |p| {
                                let _ = app2.emit("sync-progress", &p);
                            })?;
                            let files_n = files::embed_pending_files(&conn, e, |done, total| {
                                if total > 0 {
                                    let _ = app2.emit(
                                        "local-progress",
                                        json!({ "stage": "embed", "done": done, "total": total }),
                                    );
                                }
                            })?;
                            let bm = bookmarks::embed_pending_bookmarks(&conn, e, |_, _| {})?;
                            let hi = history::embed_pending_history(&conn, e, |_, _| {})?;
                            let cl = clips::embed_pending_clips(&conn, e, |_, _| {})?;
                            Ok(repos + files_n + bm + hi + cl)
                        }
                        None => Ok(0),
                    }
                })
                .await;
                reload_store(&db_path, &store);
                // close out any progress state the catch-up opened; without
                // these the UI spinner never stops
                let _ = app.emit("sync-done", json!({ "catchup": true }));
                let _ = app.emit("local-done", json!({ "catchup": true }));
                if let Ok(Ok(n)) = done {
                    if n > 0 {
                        let _ = app.emit("embed-caught-up", n);
                    }
                }
            }
            Ok(Err(e)) => {
                log::error!("semantic model init failed: {e}");
                let msg = format!("failed: {e}");
                *status.lock().unwrap() = msg.clone();
                let _ = app.emit("model-status", msg);
                initing.store(false, Ordering::SeqCst);
                // whatever model is loaded stays in use; let its passes run
                magpie_core::threads::resume(magpie_core::threads::Model::Text);
                if reinit.swap(false, Ordering::SeqCst) {
                    // a setting changed while this attempt ran; try once more
                    spawn_model_init(app.clone());
                }
            }
            Err(e) => {
                log::error!("semantic model init task panicked: {e}");
                let msg = format!("failed: {e}");
                *status.lock().unwrap() = msg.clone();
                let _ = app.emit("model-status", msg);
                initing.store(false, Ordering::SeqCst);
                magpie_core::threads::resume(magpie_core::threads::Model::Text);
            }
        }
    });
}

/// Load the OCR models in the background (downloading on first run), then
/// sweep any images that are waiting for text extraction.
fn spawn_ocr_init(app: AppHandle) {
    let state = app.state::<AppState>();
    if state.ocr_initing.swap(true, Ordering::SeqCst) {
        return;
    }
    let ocr = state.ocr.clone();
    let status = state.ocr_status.clone();
    let model_dir = state.model_dir.clone();
    let initing = state.ocr_initing.clone();
    let reinit = state.ocr_reinit.clone();
    let model_id = db::open(&state.db_path)
        .ok()
        .and_then(|c| db::meta_get(&c, "ocr_model").ok().flatten())
        .filter(|m| magpie_core::ocr::is_known_model(m))
        .unwrap_or_else(|| magpie_core::ocr::OCR_MODEL_ID.into());
    let threads = index_threads(&state.db_path);
    tauri::async_runtime::spawn(async move {
        *status.lock().unwrap() = "loading".into();
        let init = tokio::task::spawn_blocking({
            let model_dir = model_dir.clone();
            let status = status.clone();
            let app = app.clone();
            move || {
                let mut last = String::new();
                magpie_core::ocr::Ocr::new_with_model(&model_dir, &model_id, threads, &mut |msg| {
                    if msg != last {
                        last = msg.clone();
                        *status.lock().unwrap() = msg.clone();
                        let _ = app.emit("model-status", msg);
                    }
                })
            }
        })
        .await;
        match init {
            Ok(Ok(engine)) => {
                // the user may have toggled OCR off while the download ran;
                // installing the engine anyway would keep extracting forever
                let still_on = {
                    let state = app.state::<AppState>();
                    db::open(&state.db_path)
                        .ok()
                        .and_then(|c| db::meta_get(&c, "ocr_enabled").ok().flatten())
                        .map(|v| v == "1")
                        .unwrap_or(false)
                };
                if still_on {
                    *ocr.lock().unwrap() = Some(engine);
                    *status.lock().unwrap() = "ready".into();
                    log::info!("ocr engine ready");
                    spawn_ocr_index(app.clone());
                } else {
                    *status.lock().unwrap() = String::new();
                }
            }
            Ok(Err(e)) => {
                log::error!("ocr init failed: {e}");
                *status.lock().unwrap() = format!("failed: {e}");
            }
            Err(e) => {
                log::error!("ocr init task panicked: {e}");
                *status.lock().unwrap() = format!("failed: {e}");
            }
        }
        initing.store(false, Ordering::SeqCst);
        // a load-time setting changed while this load ran: once more
        if reinit.swap(false, Ordering::SeqCst) {
            spawn_ocr_init(app.clone());
        }
    });
}

/// Extract text from indexed images that OCR hasn't seen yet (or whose file
/// changed since). Updates `files.content`, which flows into FTS via the
/// existing triggers and into the e5 space on the next embed pass.
fn spawn_ocr_index(app: AppHandle) {
    let state = app.state::<AppState>();
    if state.ocr_indexing.swap(true, Ordering::SeqCst) {
        return;
    }
    let ocr = state.ocr.clone();
    let db_path = state.db_path.clone();
    let indexing = state.ocr_indexing.clone();
    tauri::async_runtime::spawn(async move {
        let worked = tokio::task::spawn_blocking(move || -> Result<usize> {
            let conn = db::open(&db_path)?;
            let exts = "'jpg','jpeg','png','webp','bmp','gif'";
            let pending: Vec<(i64, String, i64)> = {
                let mut stmt = conn.prepare(&format!(
                    "SELECT id, path, mtime FROM files
                     WHERE lower(ext) IN ({exts})
                       AND (ocr_mtime IS NULL OR ocr_mtime != mtime)"
                ))?;
                let rows = stmt.query_map([], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
                })?;
                rows.collect::<magpie_core::rusqlite::Result<Vec<_>>>()?
            };
            let mut done = 0usize;
            for (id, path, mtime) in pending {
                // engine gone (setting turned off) — stop quietly
                let text = match ocr.lock().unwrap().as_mut() {
                    Some(engine) => {
                        match engine.extract_text_from_path(std::path::Path::new(&path)) {
                            Ok(t) => t,
                            Err(e) => {
                                // unreadable file or inference error: log,
                                // mark attempted, move on
                                log::warn!("ocr {path}: {e}");
                                String::new()
                            }
                        }
                    }
                    None => return Ok(done),
                };
                if text.trim().is_empty() {
                    // nothing readable: only record the attempt
                    conn.execute(
                        "UPDATE files SET ocr_mtime = ?2 WHERE id = ?1",
                        magpie_core::rusqlite::params![id, mtime],
                    )?;
                } else {
                    conn.execute(
                        "UPDATE files SET content = ?2, ocr_mtime = ?3 WHERE id = ?1",
                        magpie_core::rusqlite::params![id, text, mtime],
                    )?;
                }
                done += 1;
            }
            // scanned PDFs, only when the user opted in (large scans are
            // slow): pdf-inspector routes which pages need OCR, we pull each
            // such page's embedded scan image and read it. Native text-layer
            // markdown and OCR text land in `content` together.
            let pdf_on = db::meta_get(&conn, "ocr_pdf")
                .ok()
                .flatten()
                .map(|v| v == "1")
                .unwrap_or(false);
            if pdf_on {
                let pdfs: Vec<(i64, String, i64)> = {
                    let mut stmt = conn.prepare(
                        "SELECT id, path, mtime FROM files
                         WHERE lower(ext) = 'pdf'
                           AND (ocr_mtime IS NULL OR ocr_mtime != mtime)",
                    )?;
                    let rows = stmt.query_map([], |r| {
                        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
                    })?;
                    rows.collect::<magpie_core::rusqlite::Result<Vec<_>>>()?
                };
                for (id, path, mtime) in pdfs {
                    let text = match ocr.lock().unwrap().as_mut() {
                        Some(engine) => {
                            let p = std::path::Path::new(&path);
                            match files::pdf_ocr_plan(p) {
                                Some((pages, native)) if !pages.is_empty() => {
                                    let mut parts = vec![native];
                                    for img in files::pdf_page_images(p, &pages, 50) {
                                        match engine.extract_text(&img) {
                                            Ok(t) if !t.trim().is_empty() => parts.push(t),
                                            Ok(_) => {}
                                            Err(e) => log::warn!("pdf ocr {path}: {e}"),
                                        }
                                    }
                                    parts.join("\n").chars().take(2_000_000).collect()
                                }
                                // text-layer PDF or unreadable: nothing to add
                                _ => String::new(),
                            }
                        }
                        None => return Ok(done),
                    };
                    if text.trim().is_empty() {
                        conn.execute(
                            "UPDATE files SET ocr_mtime = ?2 WHERE id = ?1",
                            magpie_core::rusqlite::params![id, mtime],
                        )?;
                    } else {
                        conn.execute(
                            "UPDATE files SET content = ?2, ocr_mtime = ?3 WHERE id = ?1",
                            magpie_core::rusqlite::params![id, text, mtime],
                        )?;
                    }
                    done += 1;
                }
            }
            // then video shots: re-grab each unprocessed shot's frame at OCR
            // resolution (960px — the stored thumbs are 96px) and read it
            let decode = decode_opts_from_meta(&conn);
            loop {
                let batch = magpie_core::videos::pending_ocr_shots(&conn, 64)?;
                if batch.is_empty() {
                    break;
                }
                for (shot_id, path, ts_ms) in batch {
                    let text = match ocr.lock().unwrap().as_mut() {
                        Some(engine) => {
                            match magpie_core::videos::frame_at_sized(&path, ts_ms, decode, 960)
                                .and_then(|img| engine.extract_text(&img))
                            {
                                Ok(t) => t,
                                Err(e) => {
                                    log::warn!("shot ocr {path}@{ts_ms}ms: {e}");
                                    String::new() // mark attempted, move on
                                }
                            }
                        }
                        None => return Ok(done),
                    };
                    magpie_core::videos::set_shot_ocr(&conn, shot_id, &text)?;
                    done += 1;
                }
            }
            Ok(done)
        })
        .await;
        if let Ok(Ok(n)) = worked {
            if n > 0 {
                log::info!("ocr pass extracted text for {n} image(s)");
            }
        }
        indexing.store(false, Ordering::SeqCst);
    });
}

/// Load SigLIP in the background, then embed any images that are waiting.
fn spawn_siglip_init(app: AppHandle) {
    let state = app.state::<AppState>();
    if state.siglip_initing.swap(true, Ordering::SeqCst) {
        return; // an init is already running; siglip_reinit may queue a rerun
    }
    let siglip = state.siglip.clone();
    let status = state.siglip_status.clone();
    let model_dir = state.model_dir.clone();
    let db_path = state.db_path.clone();
    let store = state.store.clone();
    let initing = state.siglip_initing.clone();
    let reinit = state.siglip_reinit.clone();
    let threads = index_threads(&db_path);
    tauri::async_runtime::spawn(async move {
        *status.lock().unwrap() = "loading".into();
        let init = tokio::task::spawn_blocking({
            let model_dir = model_dir.clone();
            let status = status.clone();
            let app = app.clone();
            move || {
                let mut last = String::new();
                Siglip::new_with_progress(&model_dir, threads, &mut |msg| {
                    if msg != last {
                        last = msg.clone();
                        *status.lock().unwrap() = msg.clone();
                        let _ = app.emit("model-status", msg);
                    }
                })
            }
        })
        .await;
        match init {
            Ok(Ok(s)) => {
                *siglip.lock().unwrap() = Some(s);
                *status.lock().unwrap() = "ready".into();
                let _ = app.emit("model-status", "image-ready");
                initing.store(false, Ordering::SeqCst);
                // a load-time setting changed while this load ran: load once
                // more before catching up (see spawn_model_init)
                if reinit.swap(false, Ordering::SeqCst) {
                    spawn_siglip_init(app.clone());
                    return;
                }
                magpie_core::threads::resume(magpie_core::threads::Model::Image);
                let app2 = app.clone();
                let catchup_path = db_path.clone();
                let done = tokio::task::spawn_blocking(move || -> Result<usize> {
                    let conn = db::open(&catchup_path)?;
                    match siglip.lock().unwrap().as_mut() {
                        Some(s) => files::embed_pending_images(&conn, s, |done, total| {
                            if total > 0 {
                                let _ = app2.emit(
                                    "local-progress",
                                    json!({ "stage": "embed-images", "done": done, "total": total }),
                                );
                            }
                        }),
                        None => Ok(0),
                    }
                })
                .await;
                reload_store(&db_path, &store);
                let _ = app.emit("local-done", json!({ "catchup": true }));
                if let Ok(Ok(n)) = done {
                    if n > 0 {
                        let _ = app.emit("embed-caught-up", n);
                    }
                }
                // image model is up → catch up image clips, then sweep videos
                {
                    let siglip2 = app.state::<AppState>().siglip.clone();
                    let path2 = db_path.clone();
                    let store2 = store.clone();
                    tokio::task::spawn_blocking(move || {
                        if let (Ok(conn), Ok(mut guard)) = (db::open(&path2), siglip2.lock()) {
                            if let Some(s) = guard.as_mut() {
                                while matches!(clips::embed_pending_image_clips(&conn, s), Ok(n) if n > 0) {}
                            }
                        }
                        reload_store(&path2, &store2);
                    });
                }
                spawn_video_index(app.clone());
            }
            Ok(Err(e)) => {
                let msg = format!("failed: {e}");
                *status.lock().unwrap() = msg;
                let _ = app.emit("model-status", "image-failed");
                initing.store(false, Ordering::SeqCst);
                // whatever model is loaded stays in use; let its passes run
                magpie_core::threads::resume(magpie_core::threads::Model::Image);
                if reinit.swap(false, Ordering::SeqCst) {
                    spawn_siglip_init(app.clone());
                }
            }
            Err(e) => {
                let msg = format!("failed: {e}");
                *status.lock().unwrap() = msg;
                let _ = app.emit("model-status", "image-failed");
                initing.store(false, Ordering::SeqCst);
                magpie_core::threads::resume(magpie_core::threads::Model::Image);
            }
        }
    });
}

// ---------- window / tray / shortcut ----------

/// Tray labels for a UI language ("zh" or anything else = English).
fn tray_labels(lang: &str) -> [&'static str; 4] {
    if lang == "zh" {
        ["显示", "立即同步", "设置…", "退出"]
    } else {
        ["Show", "Sync now", "Settings…", "Quit"]
    }
}

/// `update_version`: non-empty adds a "new version available" item on top.
fn build_tray_menu(
    app: &AppHandle,
    lang: &str,
    update_version: &str,
) -> tauri::Result<Menu<tauri::Wry>> {
    let l = tray_labels(lang);
    let show = MenuItem::with_id(app, "show", l[0], true, None::<&str>)?;
    let sync_item = MenuItem::with_id(app, "sync", l[1], true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", l[2], true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", l[3], true, None::<&str>)?;
    if update_version.is_empty() {
        Menu::with_items(app, &[&show, &sync_item, &settings_item, &quit])
    } else {
        let label = if lang == "zh" {
            format!("有新版本 v{update_version}…")
        } else {
            format!("Update available: v{update_version}…")
        };
        let upd = MenuItem::with_id(app, "update", &label, true, None::<&str>)?;
        Menu::with_items(app, &[&upd, &show, &sync_item, &settings_item, &quit])
    }
}

/// Sub-switch: also OCR scanned PDFs (pages pdf-inspector routes to OCR).
/// Separate from the main toggle because large scans are slow — the user
/// decides. Off by default.
#[tauri::command]
fn set_ocr_pdf(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let conn = db::open(&state.db_path).map_err(err_str)?;
    db::meta_set(&conn, "ocr_pdf", if enabled { "1" } else { "0" }).map_err(err_str)?;
    if enabled {
        spawn_ocr_index(app); // no-op until the engine is up
    }
    Ok(())
}

/// Toggle OCR text extraction for indexed images. Turning it on downloads
/// the models (first run) and sweeps pending images; turning it off drops
/// the engine — extracted text stays searchable until a folder rebuild.
#[tauri::command]
fn set_ocr(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
    model: String,
) -> Result<(), String> {
    if !magpie_core::ocr::is_known_model(&model) {
        return Err(format!("unknown OCR model {model:?}"));
    }
    let conn = db::open(&state.db_path).map_err(err_str)?;
    db::meta_set(&conn, "ocr_enabled", if enabled { "1" } else { "0" }).map_err(err_str)?;
    db::meta_set(&conn, "ocr_model", &model).map_err(err_str)?;
    if enabled {
        // drop any loaded engine first: the model may have changed, and the
        // index worker must not keep extracting with the old one meanwhile
        *state.ocr.lock().unwrap() = None;
        spawn_ocr_init(app);
    } else {
        *state.ocr.lock().unwrap() = None; // the index worker stops on its own
        *state.ocr_status.lock().unwrap() = String::new();
    }
    Ok(())
}

/// Inline calculator / unit conversion / text transforms for the query box.
/// None = the query is neither (the frontend shows plain search results).
#[tauri::command]
fn calc_query(query: String) -> Option<serde_json::Value> {
    if let Some(r) = magpie_core::calc::eval(&query) {
        return Some(json!({ "value": r.value, "alt": r.alt }));
    }
    magpie_core::transform::transform(&query)
        .map(|t| json!({ "value": t.value, "alt": t.label, "swatch": t.swatch }))
}

/// Pin/unpin a clipboard entry; pinned clips sort first and are exempt from
/// count/age pruning. Returns the new state.
#[tauri::command]
async fn toggle_pin_clip(state: State<'_, AppState>, clip_id: i64) -> Result<bool, String> {
    let conn = state.db.lock().await;
    clips::toggle_pin(&conn, clip_id).map_err(err_str)
}

/// Put the FILE itself (not its path) on the clipboard, so pasting into a
/// chat app or mail client attaches it. Uses the OS-native tooling per
/// platform; Linux clipboards have no portable file flavor, so it degrades
/// to the path as text.
#[tauri::command]
async fn copy_file_clip(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let allowed = {
        let conn = state.db.lock().await;
        files::path_is_allowed(&conn, &path).map_err(err_str)?
    };
    if !allowed {
        return Err("path is outside indexed folders".into());
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let escaped = path.replace('\'', "''");
        let status = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("Set-Clipboard -LiteralPath '{escaped}'"),
            ])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .status()
            .map_err(err_str)?;
        if !status.success() {
            return Err("Set-Clipboard failed".into());
        }
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        let escaped = path.replace('\\', "\\\\").replace('"', "\\\"");
        let status = std::process::Command::new("osascript")
            .args(["-e", &format!("set the clipboard to POSIX file \"{escaped}\"")])
            .status()
            .map_err(err_str)?;
        if !status.success() {
            return Err("osascript clipboard failed".into());
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        copy_clip(path) // best effort: the path as text
    }
}

/// Open the OS log directory in the file manager — one click to grab the
/// log file for a bug report.
#[tauri::command]
fn open_log_dir(app: AppHandle) -> Result<(), String> {
    let dir = app.path().app_log_dir().map_err(err_str)?;
    std::fs::create_dir_all(&dir).map_err(err_str)?;
    tauri_plugin_opener::open_path(&dir, None::<&str>).map_err(err_str)
}

/// Retitle the tray menu with the stored language + current badge state.
fn refresh_tray_menu(app: &AppHandle, lang: &str) -> tauri::Result<()> {
    let version = app
        .state::<AppState>()
        .update_badge
        .lock()
        .map(|v| v.clone())
        .unwrap_or_default();
    let menu = build_tray_menu(app, lang, &version)?;
    if let Some(tray) = app.tray_by_id("main") {
        tray.set_menu(Some(menu))?;
    }
    Ok(())
}

/// Frontend found a pending update (or cleared it): remember the version,
/// re-badge the tray icon, and add/remove the update menu item.
#[tauri::command]
fn set_update_badge(
    app: AppHandle,
    state: State<'_, AppState>,
    version: Option<String>,
) -> Result<(), String> {
    let version = version.unwrap_or_default();
    if let Ok(mut v) = state.update_badge.lock() {
        if *v == version {
            return Ok(()); // periodic re-checks re-report the same version
        }
        *v = version.clone();
    }
    if !version.is_empty() {
        log::info!("update available: v{version}");
    }
    let lang = db::open(&state.db_path)
        .ok()
        .and_then(|c| db::meta_get(&c, "ui_lang").ok().flatten())
        .unwrap_or_else(|| "en".into());
    refresh_tray_menu(&app, &lang).map_err(err_str)?;
    if let (Some(tray), Some(base)) = (app.tray_by_id("main"), app.default_window_icon()) {
        let icon = if version.is_empty() {
            base.clone()
        } else {
            let (w, h) = (base.width(), base.height());
            let mut rgba = base.rgba().to_vec();
            magpie_core::badge::overlay_badge(&mut rgba, w, h);
            tauri::image::Image::new_owned(rgba, w, h)
        };
        tray.set_icon(Some(icon)).map_err(err_str)?;
    }
    Ok(())
}

/// Persist the resolved UI language ("en"/"zh") and retitle the tray menu.
/// The frontend resolves "auto" against the OS locale before calling this.
#[tauri::command]
fn set_ui_lang(app: AppHandle, state: State<'_, AppState>, lang: String) -> Result<(), String> {
    let lang = if lang == "zh" { "zh" } else { "en" };
    let conn = db::open(&state.db_path).map_err(err_str)?;
    db::meta_set(&conn, "ui_lang", lang).map_err(err_str)?;
    refresh_tray_menu(&app, lang).map_err(err_str)?;
    Ok(())
}

/// The palette's footprint with the preview pane open, in logical pixels.
/// Mirrors WINDOW_WIDTH + PREVIEW_PANE_WIDTH in src/App.tsx; `show_window`
/// centres this box so the pane can open with a pure rightward resize.
const PREVIEW_TOTAL_WIDTH: f64 = 1092.0;

/// Where a palette should sit once it has been resized.
///
/// The left edge stays pinned: `show_window` already reserved room for the
/// pane, so growing rightwards into it is the normal case and nothing should
/// move. The clamp is the safety net for a display too small for the reserved
/// box, or a palette dragged against an edge before opening the pane. Sizes
/// are the REQUESTED ones, not re-read from the window: macOS applies
/// `set_size` asynchronously, and geometry read straight after it is stale.
fn placed_after_resize(
    pos: (i32, i32),
    after: (i32, i32),
    monitor: Option<((i32, i32), (i32, i32))>,
) -> (i32, i32) {
    let (mut x, mut y) = pos;
    if let Some(((mx, my), (mw, mh))) = monitor {
        // a window larger than the monitor leaves no range to clamp into; pin
        // it to the corner rather than panicking on an inverted range
        let fit = |v: i32, lo: i32, span: i32, size: i32| {
            let hi = lo + span - size;
            if hi < lo {
                lo
            } else {
                v.clamp(lo, hi)
            }
        };
        x = fit(x, mx, mw, after.0);
        y = fit(y, my, mh, after.1);
    }
    (x, y)
}

/// Resize the palette and keep it on screen. See [`placed_after_resize`].
#[tauri::command]
fn resize_palette(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
    let Some(w) = app.get_webview_window("main") else {
        return Ok(());
    };
    let before = w.outer_size().map_err(err_str)?;
    // React runs the effect behind this on every render, so most calls ask for
    // the size the window already has. Typing would otherwise churn through a
    // resize per keystroke.
    let scale = w.scale_factor().map_err(err_str)?;
    let want = tauri::LogicalSize::new(width, height).to_physical::<u32>(scale);
    if before.width == want.width && before.height == want.height {
        return Ok(());
    }
    let pos = w.outer_position().map_err(err_str)?;
    w.set_size(tauri::LogicalSize::new(width, height))
        .map_err(err_str)?;
    let monitor = w.current_monitor().ok().flatten().map(|m| {
        let (mp, ms) = (m.position(), m.size());
        ((mp.x, mp.y), (ms.width as i32, ms.height as i32))
    });
    let (x, y) = placed_after_resize(
        (pos.x, pos.y),
        (want.width as i32, want.height as i32),
        monitor,
    );
    if (x, y) != (pos.x, pos.y) {
        w.set_position(tauri::PhysicalPosition::new(x, y))
            .map_err(err_str)?;
    }
    Ok(())
}

#[cfg(test)]
mod hotkey_tests {
    use super::{resolve_selection_hotkey, DEFAULT_SELECTION_HOTKEY};

    #[test]
    fn selection_chord_is_on_by_default_off_when_removed_custom_when_set() {
        assert_eq!(resolve_selection_hotkey(None).as_deref(), Some(DEFAULT_SELECTION_HOTKEY));
        assert_eq!(resolve_selection_hotkey(Some("")), None, "an explicit removal stays removed");
        assert_eq!(resolve_selection_hotkey(Some("   ")), None);
        assert_eq!(resolve_selection_hotkey(Some(" Ctrl+Alt+F9 ")).as_deref(), Some("Ctrl+Alt+F9"));
    }

    #[test]
    fn the_default_never_collides_with_the_summon_chord() {
        assert_ne!(DEFAULT_SELECTION_HOTKEY, super::DEFAULT_HOTKEY);
    }
}

#[cfg(test)]
mod search_cancel_tests {
    use super::take_search_conn;
    use magpie_core::db;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tokio::sync::Mutex as AsyncMutex;

    const ENDLESS: &str =
        "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c) SELECT count(*) FROM c";

    /// The scenario from the report: an expensive stale query (the lone "v"
    /// of a retyped "vscode") is holding the connection when the next search
    /// arrives. The new search must cancel it and take over promptly instead
    /// of queueing behind it.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_new_search_cancels_the_running_one_and_takes_over() {
        let db = Arc::new(AsyncMutex::new(db::open_in_memory().expect("db")));
        let interrupt = Arc::new(db.try_lock().unwrap().get_interrupt_handle());
        let gen = Arc::new(AtomicU64::new(0));

        let (db2, int2, gen2) = (db.clone(), interrupt.clone(), gen.clone());
        let stale = tokio::spawn(async move {
            let conn = take_search_conn(&db2, &int2, &gen2)
                .await
                .expect("first ticket runs");
            let r: Result<i64, _> = conn.query_row(ENDLESS, [], |row| row.get(0));
            r.is_err() // the endless query can only end by being interrupted
        });
        // let the stale query actually start spinning on the connection
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        let started = std::time::Instant::now();
        let conn = take_search_conn(&db, &interrupt, &gen)
            .await
            .expect("the latest ticket wins");
        let waited = started.elapsed();
        let v: i64 = conn.query_row("SELECT 5", [], |row| row.get(0)).expect("query");
        drop(conn);

        assert_eq!(v, 5, "the new search must run normally after the takeover");
        assert!(
            waited < std::time::Duration::from_secs(2),
            "the new search waited {waited:?}; it must not queue behind the stale scan"
        );
        assert!(stale.await.unwrap(), "the stale query must have been interrupted");
    }

    /// A burst of a hundred keystrokes: no deadlock, no panic, every ticket
    /// settles, and the newest one is the one that gets to run last.
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn a_burst_of_searches_settles_with_the_newest_running_last() {
        let db = Arc::new(AsyncMutex::new(db::open_in_memory().expect("db")));
        let interrupt = Arc::new(db.try_lock().unwrap().get_interrupt_handle());
        let gen = Arc::new(AtomicU64::new(0));
        let ran = Arc::new(AtomicU64::new(0));
        let mut tasks = Vec::new();
        for i in 0..100u64 {
            let (db, int, gen, ran) = (db.clone(), interrupt.clone(), gen.clone(), ran.clone());
            tasks.push(tokio::spawn(async move {
                if let Some(conn) = take_search_conn(&db, &int, &gen).await {
                    // a short real query, so interrupts have something to hit
                    let _: Result<i64, _> = conn.query_row("SELECT 1", [], |r| r.get(0));
                    ran.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    return Some(i);
                }
                None
            }));
        }
        let mut winners = Vec::new();
        for t in tasks {
            if let Some(i) = tokio::time::timeout(std::time::Duration::from_secs(10), t)
                .await
                .expect("no deadlock")
                .expect("no panic")
            {
                winners.push(i);
            }
        }
        assert_eq!(gen.load(std::sync::atomic::Ordering::SeqCst), 100, "every ticket was issued");
        assert!(!winners.is_empty(), "at least one search ran");
        assert_eq!(winners.len() as u64, ran.load(std::sync::atomic::Ordering::SeqCst));
        // the connection is still healthy afterwards
        let v: i64 = db.lock().await.query_row("SELECT 2", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 2);
    }

    /// Three keystrokes race for the lock: whoever is no longer the newest by
    /// the time it reaches the head of the queue must bail without running.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_superseded_ticket_does_no_work() {
        let db = Arc::new(AsyncMutex::new(db::open_in_memory().expect("db")));
        let interrupt = Arc::new(db.try_lock().unwrap().get_interrupt_handle());
        let gen = Arc::new(AtomicU64::new(0));

        // hold the connection so both tickets have to queue
        let guard = db.lock().await;
        let (db2, int2, gen2) = (db.clone(), interrupt.clone(), gen.clone());
        let older = tokio::spawn(async move {
            take_search_conn(&db2, &int2, &gen2).await.is_none()
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (db3, int3, gen3) = (db.clone(), interrupt.clone(), gen.clone());
        let newer = tokio::spawn(async move {
            take_search_conn(&db3, &int3, &gen3).await.is_some()
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        drop(guard); // tokio's mutex is fair: the older ticket wakes first

        assert!(older.await.unwrap(), "the overtaken ticket must bail");
        assert!(newer.await.unwrap(), "the newest ticket must get the connection");
    }

    /// The mechanism take_search_conn leans on: an interrupt raised from
    /// another thread stops a running statement with an error, and the
    /// connection stays usable for the next query.
    #[test]
    fn interrupt_stops_a_running_query_and_the_connection_survives() {
        let conn = db::open_in_memory().expect("in-memory db");
        let handle = conn.get_interrupt_handle();
        let t = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(60));
            handle.interrupt();
        });
        // unbounded recursive CTE: runs until something stops it
        let started = std::time::Instant::now();
        let r: Result<i64, _> = conn.query_row(
            "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c) SELECT count(*) FROM c",
            [],
            |row| row.get(0),
        );
        t.join().unwrap();
        assert!(r.is_err(), "the interrupt must surface as an error");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "the query must stop near the interrupt, not run on"
        );
        let ok: i64 = conn
            .query_row("SELECT 41 + 1", [], |row| row.get(0))
            .expect("the connection must survive an interrupt");
        assert_eq!(ok, 42);
    }

    /// Raising the flag while nothing runs must not poison later queries:
    /// take_search_conn interrupts before it knows whether a search is
    /// actually in flight.
    #[test]
    fn interrupt_with_nothing_running_is_harmless() {
        let conn = db::open_in_memory().expect("in-memory db");
        conn.get_interrupt_handle().interrupt();
        let ok: i64 = conn
            .query_row("SELECT 7", [], |row| row.get(0))
            .expect("a statement started after the flag must run normally");
        assert_eq!(ok, 7);
    }
}

#[cfg(test)]
mod reload_tests {
    use super::{reload_loaded, StdMutex};
    use std::sync::atomic::{AtomicBool, Ordering};

    /// The order of stop/spawn calls, the reinit flag afterwards, and "a
    /// reload is on its way" as reported to the caller.
    fn run(initing: bool, reinit: bool, status: &str) -> (Vec<&'static str>, bool, bool) {
        let initing = AtomicBool::new(initing);
        let reinit = AtomicBool::new(reinit);
        let status = StdMutex::new(status.to_string());
        let calls = StdMutex::new(Vec::new());
        let reported = reload_loaded(
            &initing,
            &reinit,
            &status,
            || calls.lock().unwrap().push("stop"),
            || calls.lock().unwrap().push("spawn"),
        );
        let calls = calls.into_inner().unwrap();
        (calls, reinit.load(Ordering::SeqCst), reported)
    }

    #[test]
    fn a_ready_model_is_stopped_then_reloaded_once() {
        // stop before spawn: the load that follows is what lifts the stop
        assert_eq!(run(false, false, "ready"), (vec!["stop", "spawn"], false, true));
    }

    #[test]
    fn a_model_that_is_off_or_failed_is_left_alone() {
        // nothing spawned and, above all, nothing stopped: with no load in
        // flight there would be nobody to lift the stop
        assert_eq!(run(false, false, ""), (vec![], false, false));
        assert_eq!(run(false, false, "failed: no network"), (vec![], false, false));
    }

    #[test]
    fn a_load_in_flight_gets_one_rerun_queued_instead() {
        assert_eq!(run(true, false, "loading"), (vec!["stop"], true, true));
        // queueing twice is still one rerun
        assert_eq!(run(true, true, "downloading model.onnx 40%"), (vec!["stop"], true, true));
    }

    #[test]
    fn every_stop_has_a_load_in_flight_to_lift_it() {
        // the invariant behind the ordering: on any path that stops, either
        // a load was already running (queued) or one is spawned right after
        for (initing, status) in [(false, "ready"), (true, "loading"), (true, "ready")] {
            let (calls, _, _) = run(initing, false, status);
            if calls.contains(&"stop") {
                assert!(initing || calls.contains(&"spawn"), "{initing} {status}: {calls:?}");
            }
        }
    }
}

#[cfg(test)]
mod placement_tests {
    use super::{placed_after_resize, PREVIEW_TOTAL_WIDTH};

    const HD: Option<((i32, i32), (i32, i32))> = Some(((0, 0), (1366, 768)));

    #[test]
    fn growing_into_the_reserved_box_never_moves_the_window() {
        // show_window seats a 720-wide palette at the left edge of the centred
        // 1092 box: x = (2560 - 1092) / 2 = 734. Opening the pane grows into
        // room that is already reserved, so the position must not change.
        let wide = Some(((0, 0), (2560, 1440)));
        assert_eq!(
            placed_after_resize((734, 456), (1092, 480), wide),
            (734, 456)
        );
        // and the whole box sits centred: 734 + 1092 = 1826, 2560 - 1826 = 734
        assert_eq!(2560 - (734 + 1092), 734);
    }

    #[test]
    fn the_reserved_box_fits_a_1366_screen() {
        // x = (1366 - 1092) / 2 = 137, and 137 + 1092 = 1229 <= 1366, so
        // opening the pane must stay put
        assert_eq!(placed_after_resize((137, 200), (1092, 480), HD), (137, 200));
    }

    #[test]
    fn a_palette_dragged_against_the_right_edge_is_pulled_back() {
        let (x, _) = placed_after_resize((640, 200), (1092, 480), HD);
        assert_eq!(x, 274, "clamped so the right edge lands exactly on 1366");
        assert_eq!(x + 1092, 1366);
    }

    #[test]
    fn height_only_changes_leave_the_window_alone() {
        // 100 + 620 = 720, inside the 768-tall screen, so nothing should move
        assert_eq!(placed_after_resize((323, 100), (720, 620), HD), (323, 100));
    }

    #[test]
    fn a_tall_palette_is_pulled_up_onto_the_screen() {
        let (_, y) = placed_after_resize((323, 600), (720, 620), HD);
        assert_eq!(y, 148);
        assert_eq!(y + 620, 768);
    }

    #[test]
    fn a_window_larger_than_the_monitor_pins_to_the_corner() {
        assert_eq!(placed_after_resize((100, 100), (2000, 900), HD), (0, 0));
    }

    #[test]
    fn a_second_monitor_clamps_to_its_own_offset() {
        // a 1366-wide display sitting to the right of a 2560-wide primary
        let right = Some(((2560, 0), (1366, 768)));
        let (x, y) = placed_after_resize((2883, 200), (1092, 480), right);
        assert_eq!((x, y), (2834, 200));
        assert!(x >= 2560 && x + 1092 <= 2560 + 1366);
    }

    #[test]
    fn no_monitor_information_leaves_the_window_alone() {
        assert_eq!(placed_after_resize((323, 200), (1092, 480), None), (323, 200));
    }

    #[test]
    fn shrinking_back_keeps_the_left_edge() {
        // close the pane: the palette stays where its left edge always was
        let wide = Some(((0, 0), (2560, 1440)));
        assert_eq!(placed_after_resize((734, 456), (720, 480), wide), (734, 456));
    }

    #[test]
    fn the_reserved_width_matches_the_frontend_constants() {
        // WINDOW_WIDTH (720) + PREVIEW_PANE_WIDTH (372) in src/App.tsx; the
        // static sweep checks the TS side, this pins the Rust side to it
        assert_eq!(PREVIEW_TOTAL_WIDTH, 1092.0);
    }
}

/// Restart after installing an update.
///
/// The single-instance lock has to be released first. `restart()` spawns the
/// replacement process and only then exits this one, so a lock still held here
/// makes the replacement see a running instance and quit on startup, leaving
/// the user with nothing after an update. Windows never reaches this path (the
/// updater runs the NSIS installer and exits through `process::exit`), but
/// macOS and Linux do.
#[tauri::command]
fn restart_for_update(app: AppHandle) {
    log::info!("update installed; releasing the single-instance lock and restarting");
    tauri_plugin_single_instance::destroy(&app);
    app.restart();
}

fn toggle_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            show_window(app);
        }
    }
}

fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        // launcher position: horizontally centered, upper fifth (Spotlight
        // convention) — on the monitor the CURSOR is on, so multi-display
        // users summon the palette where they're looking. Monitor offsets
        // are global coordinates; the old math ignored them and always
        // landed on the primary display.
        let monitor = app
            .cursor_position()
            .ok()
            .and_then(|cur| {
                w.available_monitors().ok().and_then(|mons| {
                    mons.into_iter().find(|m| {
                        let p = m.position();
                        let s = m.size();
                        cur.x >= p.x as f64
                            && cur.x < (p.x + s.width as i32) as f64
                            && cur.y >= p.y as f64
                            && cur.y < (p.y + s.height as i32) as f64
                    })
                })
            })
            .or_else(|| w.current_monitor().ok().flatten());
        if let (Some(monitor), Ok(size)) = (monitor, w.outer_size()) {
            let mp = monitor.position();
            let ms = monitor.size();
            // Centre the box the palette can GROW into, not the palette
            // itself. The preview pane extends the window to the right, and
            // moving the window at that moment does not work everywhere
            // (macOS applies set_size asynchronously, so a reposition
            // computed right after it reads stale geometry). Reserving the
            // expanded footprint up front means opening and closing the pane
            // is a pure resize with the left edge pinned: nothing has to
            // move, on any platform. The palette sits slightly left of
            // centre as a result; the pane opens into the reserved half.
            let reserved = (PREVIEW_TOTAL_WIDTH * monitor.scale_factor()) as i32;
            let span = reserved.max(size.width as i32);
            let x = mp.x + ((ms.width as i32 - span) / 2).max(0);
            let y = mp.y + (ms.height as f64 * 0.22) as i32;
            let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
        }
        // above everything, on every summon: re-assert topmost (another app
        // may have claimed it) and follow the user across macOS Spaces
        let _ = w.set_always_on_top(true);
        let _ = w.set_visible_on_all_workspaces(true);
        let _ = w.show();
        let _ = w.set_focus();
        let _ = app.emit("palette-shown", ());
    }
}

pub fn run() {
    tauri::Builder::default()
        // Registered before everything else so a second launch exits before it
        // opens the database or claims a tray icon. magpie lives in the tray
        // and answers to a hotkey, so clicking the icon again means "show me
        // the palette", never "start another copy".
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            log::info!("second launch requested; summoning the running instance");
            show_window(app);
        }))
        // logging first, so every later plugin/setup line can log. Info level,
        // one rotated file in the OS log dir — enough forensics for bug
        // reports, small enough to attach to an issue. Queries are never
        // logged (privacy).
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .max_file_size(2_000_000)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepOne)
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("magpie".into()),
                    }),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                ])
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            log::info!("magpie v{} starting", app.package_info().version);
            let data_dir = app.path().app_data_dir()?;
            let db_path = data_dir.join("stars.db");
            let model_dir = data_dir.join("models");
            let conn = db::open(&db_path)?;
            sync::ensure_embed_model(&conn)?;
            // must be set before model init spawns: hf-hub reads HF_ENDPOINT
            if let Ok(Some(ep)) = db::meta_get(&conn, "hf_endpoint") {
                if !ep.is_empty() {
                    std::env::set_var("HF_ENDPOINT", ep);
                }
            }
            let clipboard_enabled = db::meta_get(&conn, "clipboard_enabled")
                .ok()
                .flatten()
                .as_deref()
                == Some("1");
            // clip housekeeping on every launch: retention window + count cap
            {
                let days = db::meta_get(&conn, "clip_retention_days")
                    .ok()
                    .flatten()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(30u32);
                let _ = clips::prune_clips(&conn, days, unix_now());
                let cap = db::meta_get(&conn, "clip_max_entries")
                    .ok()
                    .flatten()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0u32);
                let _ = clips::prune_clips_to_count(&conn, cap);
            }
            let store = VectorStore::load(&conn).unwrap_or_else(|_| VectorStore::empty());

            // opened after `conn` so migrations have already run once; this
            // twin only ever reads (interactive searches), so a cancelled
            // search can never take a write down with it
            let search_conn = db::open(&db_path)?;
            let search_interrupt = search_conn.get_interrupt_handle();

            app.manage(AppState {
                db_path,
                model_dir,
                db: AsyncMutex::new(conn),
                search_db: AsyncMutex::new(search_conn),
                search_interrupt,
                search_gen: AtomicU64::new(0),
                store: Arc::new(StdMutex::new(store)),
                embedder: Arc::new(StdMutex::new(None)),
                model_status: Arc::new(StdMutex::new("loading".into())),
                siglip: Arc::new(StdMutex::new(None)),
                siglip_status: Arc::new(StdMutex::new("loading".into())),
                sync_running: Arc::new(AtomicBool::new(false)),
                local_indexing: Arc::new(AtomicBool::new(false)),
                video_indexing: Arc::new(AtomicBool::new(false)),
                video_note: Arc::new(StdMutex::new(String::new())),
                ffmpeg_status: Arc::new(StdMutex::new(String::new())),
                model_initing: Arc::new(AtomicBool::new(false)),
                model_reinit: Arc::new(AtomicBool::new(false)),
                siglip_initing: Arc::new(AtomicBool::new(false)),
                siglip_reinit: Arc::new(AtomicBool::new(false)),
                apps: Arc::new(StdMutex::new(Vec::new())),
                update_badge: Arc::new(StdMutex::new(String::new())),
                ocr: Arc::new(StdMutex::new(None)),
                ocr_status: Arc::new(StdMutex::new(String::new())),
                ocr_initing: Arc::new(AtomicBool::new(false)),
                ocr_reinit: Arc::new(AtomicBool::new(false)),
                ocr_indexing: Arc::new(AtomicBool::new(false)),
                clip_watch: Arc::new(AtomicBool::new(clipboard_enabled)),
                clip_thread_alive: Arc::new(AtomicBool::new(false)),
            });
            if clipboard_enabled {
                spawn_clip_watcher(app.handle().clone());
            }
            spawn_app_scan(app.handle().clone());
            spawn_ffmpeg_check(app.handle().clone());

            // tray: Show / Sync / Settings / Quit (labels follow the UI language)
            let ui_lang = {
                let state = app.state::<AppState>();
                db::open(&state.db_path)
                    .ok()
                    .and_then(|c| db::meta_get(&c, "ui_lang").ok().flatten())
                    .unwrap_or_else(|| "en".into())
            };
            let menu = build_tray_menu(app.handle(), &ui_lang, "")?;
            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_window(app),
                    "sync" => spawn_sync(app.clone()),
                    "settings" | "update" => {
                        show_window(app);
                        let _ = app.emit("open-settings", ());
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            // register the stored (or default) summon hotkey. Registration can
            // fail when another launcher owns the chord — degrade to tray-only
            // instead of crashing the app.
            let (hotkey, selection) = {
                let state = app.state::<AppState>();
                let db_path = state.db_path.clone();
                let conn = db::open(&db_path).ok();
                let get = |k: &str| conn.as_ref().and_then(|c| db::meta_get(c, k).ok().flatten());
                (
                    get("hotkey").unwrap_or_else(|| DEFAULT_HOTKEY.to_string()),
                    resolve_selection_hotkey(get("hotkey_selection").as_deref()),
                )
            };
            if let Err(e) = register_hotkeys(app.handle(), &hotkey, selection.as_deref()) {
                log::warn!("global shortcut {hotkey} unavailable: {e}");
                // the selection chord may be the one that failed; keep the
                // summon chord alive on its own rather than losing both
                if selection.is_some() {
                    let _ = register_hotkeys(app.handle(), &hotkey, None);
                }
            }

            spawn_model_init(app.handle().clone());
            spawn_siglip_init(app.handle().clone());
            // OCR is opt-in; only spin it up when the user enabled it
            {
                let state = app.state::<AppState>();
                let on = db::open(&state.db_path)
                    .ok()
                    .and_then(|c| db::meta_get(&c, "ocr_enabled").ok().flatten())
                    .map(|v| v == "1")
                    .unwrap_or(false);
                if on {
                    spawn_ocr_init(app.handle().clone());
                }
            }
            // refresh quietly at launch: stars (if token configured) + local folders
            spawn_sync(app.handle().clone());
            spawn_local_index(app.handle().clone());
            spawn_bookmark_sync(app.handle().clone());
            // periodic incremental re-scan: picks up deleted/changed files and
            // bookmarks while the app stays resident (unchanged scans are fast)
            let periodic = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut tick =
                    tokio::time::interval(std::time::Duration::from_secs(30 * 60));
                tick.tick().await; // first tick fires immediately; startup already indexed
                loop {
                    tick.tick().await;
                    spawn_local_index(periodic.clone());
                    spawn_bookmark_sync(periodic.clone());
                    spawn_video_index(periodic.clone());
                    // no-op unless the engine is loaded (OCR setting on)
                    spawn_ocr_index(periodic.clone());
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            search_stars,
            get_status,
            set_token,
            start_sync,
            open_repo,
            search_local,
            search_by_image,
            preview_thumb,
            set_max_file_mb,
            set_hotkey,
            set_hf_endpoint,
            list_folders,
            add_folder,
            remove_folder,
            index_local,
            rebuild_folder,
            rebuild_stars,
            search_web,
            search_apps,
            launch_app,
            sync_bookmarks_now,
            search_clips,
            copy_image_clip,
            set_clipboard_enabled,
            set_clip_retention,
            set_clip_max_entries,
            delete_clip,
            clear_clips_now,
            copy_clip,
            set_ui_lang,
            set_app_aliases,
            set_video_indexing,
            set_video_decode,
            get_preview,
            record_hit_use,
            paste_clip,
            play_video,
            export_settings,
            import_settings,
            set_update_badge,
            set_ocr,
            set_ocr_pdf,
            calc_query,
            toggle_pin_clip,
            copy_file_clip,
            open_log_dir,
            open_file,
            restart_for_update,
            resize_palette,
            recent_hits,
            append_note,
            set_note_path,
            open_note_file,
            set_selection_hotkey,
            set_skip_worktrees,
            set_index_threads
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
