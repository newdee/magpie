//! Thin Tauri shell over magpie-core: window/tray/shortcut plumbing + commands.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Result;
use serde_json::json;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex as AsyncMutex;

use magpie_core::bookmarks::{self, BookmarkHit};
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
    let conn = state.db.lock().await;
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
    {
        let conn = state.db.lock().await;
        if let Some(meta) = doc.get("meta").and_then(|m| m.as_object()) {
            for key in EXPORTABLE_META {
                if let Some(v) = meta.get(*key).and_then(|v| v.as_str()) {
                    db::meta_set(&conn, key, v).map_err(err_str)?;
                }
            }
        }
    }
    // side effects that read meta live: aliases re-attach, tray language
    spawn_app_scan(app.clone());
    if let Ok(conn) = db::open(&state.db_path) {
        if let Ok(Some(lang)) = db::meta_get(&conn, "ui_lang") {
            let _ = refresh_tray_menu(&app, &lang);
        }
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
        "hf_endpoint": hf_endpoint,
        "embedded_count": embedded,
        "last_sync": last_sync,
        "username": username,
        "has_token": has_token,
        "model": state.model_status.lock().unwrap().clone(),
        "image_model": state.siglip_status.lock().unwrap().clone(),
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
    let (qvec, image_qvec) = if query.trim().is_empty() {
        (None, None)
    } else {
        let emb = state.embedder.clone();
        let sig = state.siglip.clone();
        let q = query.clone();
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
    let conn = state.db.lock().await;
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

fn register_hotkey(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(err_str)?;
    gs.on_shortcut(hotkey, |app, _sc, event| {
        if event.state() == ShortcutState::Pressed {
            toggle_window(app);
        }
    })
    .map_err(err_str)
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
    let previous = {
        let conn = state.db.lock().await;
        db::meta_get(&conn, "hotkey")
            .map_err(err_str)?
            .unwrap_or_else(|| DEFAULT_HOTKEY.to_string())
    };
    if let Err(e) = register_hotkey(&app, &hotkey) {
        // keep the old chord working instead of leaving nothing registered
        let _ = register_hotkey(&app, &previous);
        return Err(format!("cannot register {hotkey:?}: {e}"));
    }
    let conn = state.db.lock().await;
    db::meta_set(&conn, "hotkey", &hotkey).map_err(err_str)?;
    Ok(())
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
                }
            }
            Err(e) => {
                *note.lock().unwrap() = e.to_string();
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
    let conn = state.db.lock().await;
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

#[tauri::command]
async fn search_bookmarks(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<BookmarkHit>, String> {
    let limit = limit.unwrap_or(30).min(100);
    let qvec = if query.trim().is_empty() {
        None
    } else {
        let emb = state.embedder.clone();
        let q = query.clone();
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
    let conn = state.db.lock().await;
    let store = state.store.lock().unwrap();
    search::search_bookmarks(&conn, &store, &query, qvec.as_deref(), limit).map_err(err_str)
}

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
    let conn = state.db.lock().await;
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
                let _ = app.emit("bookmarks-error", e.to_string());
            }
            Err(e) => {
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
                let _ = app.emit("local-error", e.to_string());
            }
            Err(e) => {
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
    tauri::async_runtime::spawn(async move {
        *status.lock().unwrap() = "loading".into();
        let _ = app.emit("model-status", "loading");
        let init = tokio::task::spawn_blocking({
            let model_dir = model_dir.clone();
            let status = status.clone();
            let app = app.clone();
            move || {
                let mut last = String::new();
                Embedder::new_with_fallback(&model_dir, &mut |msg| {
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
                log::info!("semantic model ready");
                let _ = app.emit("model-status", "ready");
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
                initing.store(false, Ordering::SeqCst);
                reinit.store(false, Ordering::SeqCst); // succeeded: nothing queued matters
            }
            Ok(Err(e)) => {
                log::error!("semantic model init failed: {e}");
                let msg = format!("failed: {e}");
                *status.lock().unwrap() = msg.clone();
                let _ = app.emit("model-status", msg);
                initing.store(false, Ordering::SeqCst);
                if reinit.swap(false, Ordering::SeqCst) {
                    // the mirror changed while this attempt ran; try once more
                    spawn_model_init(app.clone());
                }
            }
            Err(e) => {
                log::error!("semantic model init task panicked: {e}");
                let msg = format!("failed: {e}");
                *status.lock().unwrap() = msg.clone();
                let _ = app.emit("model-status", msg);
                initing.store(false, Ordering::SeqCst);
            }
        }
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
    tauri::async_runtime::spawn(async move {
        *status.lock().unwrap() = "loading".into();
        let init = tokio::task::spawn_blocking({
            let model_dir = model_dir.clone();
            let status = status.clone();
            let app = app.clone();
            move || {
                let mut last = String::new();
                Siglip::new_with_progress(&model_dir, &mut |msg| {
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
                initing.store(false, Ordering::SeqCst);
                reinit.store(false, Ordering::SeqCst);
            }
            Ok(Err(e)) => {
                let msg = format!("failed: {e}");
                *status.lock().unwrap() = msg;
                let _ = app.emit("model-status", "image-failed");
                initing.store(false, Ordering::SeqCst);
                if reinit.swap(false, Ordering::SeqCst) {
                    spawn_siglip_init(app.clone());
                }
            }
            Err(e) => {
                let msg = format!("failed: {e}");
                *status.lock().unwrap() = msg;
                let _ = app.emit("model-status", "image-failed");
                initing.store(false, Ordering::SeqCst);
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
            let x = mp.x + ((ms.width as i32 - size.width as i32) / 2).max(0);
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
        .plugin(tauri_plugin_process::init())
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

            app.manage(AppState {
                db_path,
                model_dir,
                db: AsyncMutex::new(conn),
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
            let hotkey = {
                let state = app.state::<AppState>();
                let db_path = state.db_path.clone();
                db::open(&db_path)
                    .ok()
                    .and_then(|c| db::meta_get(&c, "hotkey").ok().flatten())
                    .unwrap_or_else(|| DEFAULT_HOTKEY.to_string())
            };
            if let Err(e) = register_hotkey(app.handle(), &hotkey) {
                log::warn!("global shortcut {hotkey} unavailable: {e}");
            }

            spawn_model_init(app.handle().clone());
            spawn_siglip_init(app.handle().clone());
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
            search_bookmarks,
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
            open_log_dir,
            open_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
