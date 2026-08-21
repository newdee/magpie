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

use magpie_core::db;
use magpie_core::embed::Embedder;
use magpie_core::files::{self, FileHit, FolderInfo};
use magpie_core::github::GithubClient;
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
    search::search(&conn, &store, &query, qvec.as_deref(), sort, limit).map_err(err_str)
}

#[tauri::command]
async fn get_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let conn = state.db.lock().await;
    let count = db::repo_count(&conn).map_err(err_str)?;
    let file_count = files::file_count(&conn).map_err(err_str)?;
    let last_sync = db::meta_get(&conn, "last_sync").map_err(err_str)?;
    let username = db::meta_get(&conn, "username").map_err(err_str)?;
    let has_token = db::meta_get(&conn, "token").map_err(err_str)?.is_some();
    let embedded = db::all_embedding_hashes(&conn).map_err(err_str)?.len();
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
        "repo_count": count,
        "file_count": file_count,
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
    if !url.starts_with("https://") {
        return Err("only https urls".into());
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_url(url, None::<&str>).map_err(err_str)
}

// ---------- local files ----------

#[tauri::command]
async fn search_local(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<FileHit>, String> {
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
    search::search_files(
        &conn,
        &store,
        &query,
        qvec.as_deref(),
        image_qvec.as_deref(),
        limit,
    )
    .map_err(err_str)
}

/// Search indexed images with a query image: a dropped file (`path`) or
/// pasted clipboard bytes (`bytes_b64`). The query image itself does not need
/// to be inside an indexed folder — it is only embedded, never stored.
#[tauri::command]
async fn search_by_image(
    state: State<'_, AppState>,
    path: Option<String>,
    bytes_b64: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<FileHit>, String> {
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
    search::search_images(&conn, &store, &qvec, limit).map_err(err_str)
}

/// Thumbnail of a query image for the input-row chip. Read-only, never stored.
#[tauri::command]
fn preview_thumb(path: String) -> Result<Option<String>, String> {
    Ok(files::thumb_b64_for(std::path::Path::new(&path)))
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
    // a failed first download can now succeed: retry model init
    let retry_text = state.model_status.lock().unwrap().starts_with("failed");
    let retry_image = state.siglip_status.lock().unwrap().starts_with("failed");
    if retry_text {
        spawn_model_init(app.clone());
    }
    if retry_image {
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
    let embedder = state.embedder.clone();
    let status = state.model_status.clone();
    let model_dir = state.model_dir.clone();
    let db_path = state.db_path.clone();
    let store = state.store.clone();
    tauri::async_runtime::spawn(async move {
        *status.lock().unwrap() = "loading".into();
        let _ = app.emit("model-status", "loading");
        let init = tokio::task::spawn_blocking({
            let model_dir = model_dir.clone();
            move || Embedder::new(&model_dir)
        })
        .await;
        match init {
            Ok(Ok(e)) => {
                *embedder.lock().unwrap() = Some(e);
                *status.lock().unwrap() = "ready".into();
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
                            Ok(repos + files_n)
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
                let msg = format!("failed: {e}");
                *status.lock().unwrap() = msg.clone();
                let _ = app.emit("model-status", msg);
            }
            Err(e) => {
                let msg = format!("failed: {e}");
                *status.lock().unwrap() = msg.clone();
                let _ = app.emit("model-status", msg);
            }
        }
    });
}

/// Load SigLIP in the background, then embed any images that are waiting.
fn spawn_siglip_init(app: AppHandle) {
    let state = app.state::<AppState>();
    let siglip = state.siglip.clone();
    let status = state.siglip_status.clone();
    let model_dir = state.model_dir.clone();
    let db_path = state.db_path.clone();
    let store = state.store.clone();
    tauri::async_runtime::spawn(async move {
        *status.lock().unwrap() = "loading".into();
        let init = tokio::task::spawn_blocking({
            let model_dir = model_dir.clone();
            move || Siglip::new(&model_dir)
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
            }
            Ok(Err(e)) => {
                let msg = format!("failed: {e}");
                *status.lock().unwrap() = msg;
                let _ = app.emit("model-status", "image-failed");
            }
            Err(e) => {
                let msg = format!("failed: {e}");
                *status.lock().unwrap() = msg;
                let _ = app.emit("model-status", "image-failed");
            }
        }
    });
}

// ---------- window / tray / shortcut ----------

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
        // convention). Re-applied on every summon, so a dragged-away window
        // always comes back to a predictable spot.
        if let (Ok(Some(monitor)), Ok(size)) = (w.current_monitor(), w.outer_size()) {
            let m = monitor.size();
            let x = ((m.width as i32 - size.width as i32) / 2).max(0);
            let y = (m.height as f64 * 0.22) as i32;
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
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
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
            });

            // tray: Show / Sync / Settings / Quit
            let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
            let sync_item = MenuItem::with_id(app, "sync", "Sync now", true, None::<&str>)?;
            let settings_item =
                MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &sync_item, &settings_item, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_window(app),
                    "sync" => spawn_sync(app.clone()),
                    "settings" => {
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
                eprintln!("global shortcut {hotkey} unavailable: {e}");
            }

            spawn_model_init(app.handle().clone());
            spawn_siglip_init(app.handle().clone());
            // refresh quietly at launch: stars (if token configured) + local folders
            spawn_sync(app.handle().clone());
            spawn_local_index(app.handle().clone());
            // periodic incremental re-scan: picks up deleted/changed files while
            // the app stays resident (an unchanged scan is sub-second)
            let periodic = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut tick =
                    tokio::time::interval(std::time::Duration::from_secs(30 * 60));
                tick.tick().await; // first tick fires immediately; startup already indexed
                loop {
                    tick.tick().await;
                    spawn_local_index(periodic.clone());
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
            open_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
