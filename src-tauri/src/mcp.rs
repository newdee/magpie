//! MCP server for AI assistants (Claude Code, Cursor, ...): the index over
//! HTTP on the loopback interface, behind a bearer token, read-only. Off by
//! default; the settings row switches it on and hands out the client command.
//!
//! Transport: Streamable HTTP (rmcp). rmcp already rejects non-loopback
//! `Host` headers (DNS rebinding) and, configured here, any browser `Origin`
//! that is not this server's own; the bearer check in front of it turns
//! everything else away before a session exists. The tools never write:
//! they search, read indexed text, and list what the user opened.

use std::sync::Arc;

use axum::{
    Router,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ErrorData, Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

/// Where a search or a "recent" list looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Local,
    Stars,
    Bookmarks,
    History,
    Clips,
}

pub const DEFAULT_LIMIT: usize = 10;
pub const MAX_LIMIT: usize = 50;
pub const DEFAULT_CHARS: usize = 20_000;
pub const MAX_CHARS: usize = 200_000;

/// What `read_file` found.
pub enum ReadOutcome {
    /// The indexed text plus the file's row (path, name, ext, size, mtime).
    Text(Value),
    /// The path is not inside a folder the user chose to index.
    Outside,
    /// Inside an indexed folder, but not in the index (unsupported format,
    /// or not scanned yet).
    NotIndexed,
}

/// What the tools need from the app. Synchronous on purpose: every call is
/// database and model work and runs on the blocking pool. A trait so the
/// HTTP layer and the tools are testable without Tauri.
pub trait Backend: Send + Sync + 'static {
    /// The same ranked hits the palette shows for `query` on `source`.
    fn search(&self, source: Source, query: &str, limit: usize) -> anyhow::Result<Vec<Value>>;
    /// The indexed text of one file, capped at `max_chars` characters.
    fn read_file(&self, path: &str, max_chars: usize) -> anyhow::Result<ReadOutcome>;
    /// What the user opened lately through magpie, most recent first.
    fn recent(&self, source: Source, limit: usize) -> anyhow::Result<Vec<Value>>;
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchParams {
    /// Which index. "local": files, images and videos in the indexed folders
    /// (full text, OCR text, image and video content). "stars": the user's
    /// starred GitHub repositories, README-aware. "bookmarks" and "history":
    /// the user's browsers. "clips": clipboard history.
    pub source: Source,
    /// The query, in any language; keyword and semantic matches are fused.
    /// Local queries accept filters mixed into the text: `ext:pdf` or `.md`,
    /// `>10mb` / `<500kb`, `7d` / `2w` / `3m` / `1y` (modified within),
    /// `in:folder` (path contains).
    pub query: String,
    /// How many hits (default 10, at most 50).
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadFileParams {
    /// Absolute path of an indexed file, as returned by `search`.
    pub path: String,
    /// Cap on the text returned, in characters (default 20000, at most 200000).
    pub max_chars: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RecentParams {
    /// "local": files the user opened through magpie. "stars": repositories
    /// opened. "bookmarks" or "history": pages opened. ("clips" has no
    /// history of its own and returns nothing.)
    pub source: Source,
    /// How many (default 10, at most 50).
    pub limit: Option<usize>,
}

pub fn clamp_limit(limit: Option<usize>) -> usize {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

pub fn clamp_chars(chars: Option<usize>) -> usize {
    chars.unwrap_or(DEFAULT_CHARS).clamp(1, MAX_CHARS)
}

/// A palette hit shaped for a language model: no thumbnail bytes, and the
/// keyword snippet without its highlight marks.
pub fn slim(mut hit: Value) -> Value {
    if let Some(obj) = hit.as_object_mut() {
        obj.remove("thumb");
        if let Some(Value::String(s)) = obj.get_mut("snippet") {
            *s = s.replace(['\u{1}', '\u{2}'], "");
        }
    }
    hit
}

const INSTRUCTIONS: &str = "magpie indexes this user's own machine: chosen local folders (text with \
PDF, Office and OCR content, plus what images and video scenes show), starred GitHub repositories, \
browser bookmarks and history, and clipboard history. Start with `search`; `read_file` returns the \
indexed text of a local hit; `recent` lists what the user opened lately. Everything is read-only and \
limited to what the user chose to index.";

fn internal(e: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(e.to_string(), None)
}

/// The MCP handler: one per session, all sharing the backend.
#[derive(Clone)]
pub struct Magpie {
    backend: Arc<dyn Backend>,
    #[expect(dead_code, reason = "the tool_handler macro reads it")]
    tool_router: ToolRouter<Self>,
}

impl Magpie {
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        Self { backend, tool_router: Self::tool_router() }
    }
}

#[tool_router]
impl Magpie {
    #[tool(
        name = "search",
        description = "Search the user's machine through magpie's index: local files (full text, OCR'd images, video scenes), starred GitHub repos, browser bookmarks and history, or clipboard history. Returns ranked hits with paths or URLs; use read_file for a local hit's text."
    )]
    async fn search(
        &self,
        Parameters(p): Parameters<SearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let backend = self.backend.clone();
        let limit = clamp_limit(p.limit);
        let hits = tokio::task::spawn_blocking(move || backend.search(p.source, &p.query, limit))
            .await
            .map_err(internal)?
            .map_err(internal)?;
        let hits: Vec<Value> = hits.into_iter().map(slim).collect();
        Ok(CallToolResult::structured(json!({ "source": p.source, "count": hits.len(), "hits": hits })))
    }

    #[tool(
        name = "read_file",
        description = "The indexed text of one local file (PDF, Office and OCR text included), by the absolute path a search hit gave. Only files inside the user's indexed folders."
    )]
    async fn read_file(
        &self,
        Parameters(p): Parameters<ReadFileParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let backend = self.backend.clone();
        let max_chars = clamp_chars(p.max_chars);
        let outcome = tokio::task::spawn_blocking(move || backend.read_file(&p.path, max_chars))
            .await
            .map_err(internal)?
            .map_err(internal)?;
        Ok(match outcome {
            ReadOutcome::Text(v) => CallToolResult::structured(v),
            ReadOutcome::Outside => CallToolResult::structured_error(json!({
                "error": "the path is not inside a folder the user chose to index"
            })),
            ReadOutcome::NotIndexed => CallToolResult::structured_error(json!({
                "error": "the file is inside an indexed folder but not in the index: an unsupported format, or not scanned yet"
            })),
        })
    }

    #[tool(
        name = "recent",
        description = "What the user opened lately through magpie: local files, starred repos, or bookmarks and history pages. Most recent first."
    )]
    async fn recent(
        &self,
        Parameters(p): Parameters<RecentParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let backend = self.backend.clone();
        let limit = clamp_limit(p.limit);
        let rows = tokio::task::spawn_blocking(move || backend.recent(p.source, limit))
            .await
            .map_err(internal)?
            .map_err(internal)?;
        let rows: Vec<Value> = rows.into_iter().map(slim).collect();
        Ok(CallToolResult::structured(json!({ "source": p.source, "count": rows.len(), "items": rows })))
    }
}

#[tool_handler]
impl ServerHandler for Magpie {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("magpie", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

// ---------- HTTP: bearer token in front of the MCP service ----------

struct Auth {
    token: Vec<u8>,
}

/// Equal length and equal bytes, without an early exit on the first
/// mismatch: the comparison time does not tell an attacker how much of a
/// guessed token was right. (The length itself is not a secret.)
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn require_bearer(State(auth): State<Arc<Auth>>, req: Request, next: Next) -> Response {
    let presented = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim);
    if !presented.is_some_and(|t| ct_eq(t.as_bytes(), &auth.token)) {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            "unauthorized",
        )
            .into_response();
    }
    next.run(req).await
}

/// The service under `/mcp`, with the bearer check in front and rmcp's
/// loopback-only Host rule plus an Origin rule pinned to this server.
pub fn router(backend: Arc<dyn Backend>, token: String, port: u16, cancel: CancellationToken) -> Router {
    let config = StreamableHttpServerConfig::default()
        .with_cancellation_token(cancel)
        .with_json_response(true)
        .with_allowed_origins([
            format!("http://127.0.0.1:{port}"),
            format!("http://localhost:{port}"),
        ]);
    let service = StreamableHttpService::new(
        move || Ok(Magpie::new(backend.clone())),
        Arc::new(LocalSessionManager::default()),
        config,
    );
    let auth = Arc::new(Auth { token: token.into_bytes() });
    Router::new()
        .nest_service("/mcp", service)
        .layer(middleware::from_fn_with_state(auth, require_bearer))
}

/// A running server. Dropping it (or `stop`) shuts it down.
pub struct Server {
    pub port: u16,
    cancel: CancellationToken,
}

impl Server {
    pub fn stop(&self) {
        self.cancel.cancel();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// Bind `127.0.0.1:preferred_port` (0 = any free port; a remembered port
/// that is taken falls back to any free one, the caller persists the result)
/// and serve until stopped. Must run inside a tokio runtime.
pub async fn serve(
    backend: Arc<dyn Backend>,
    token: String,
    preferred_port: u16,
) -> anyhow::Result<Server> {
    let listener = bind_preferred(preferred_port).await?;
    let port = listener.local_addr()?.port();
    let cancel = CancellationToken::new();
    let router = router(backend, token, port, cancel.child_token());
    let shutdown = cancel.clone();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await;
    });
    Ok(Server { port, cancel })
}

/// The remembered port, retried for a second: a restart right after a stop
/// (a token rotation) can find the old listener still letting go. Only then
/// any free port, which the caller persists as the new remembered one.
async fn bind_preferred(port: u16) -> std::io::Result<tokio::net::TcpListener> {
    if port != 0 {
        for _ in 0..20 {
            if let Ok(l) = tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
                return Ok(l);
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
    tokio::net::TcpListener::bind(("127.0.0.1", 0)).await
}

/// 256 bits from the OS, as 64 hex characters.
pub fn new_token() -> anyhow::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("random token: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

pub fn url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/mcp")
}

/// What the user pastes into a terminal to register the server with Claude
/// Code. Other clients take the same URL and header.
pub fn client_command(port: u16, token: &str) -> String {
    format!(
        "claude mcp add --transport http magpie {} --header \"Authorization: Bearer {token}\"",
        url(port)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_equality() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"ab"));
        assert!(!ct_eq(b"", b"a"));
        assert!(ct_eq(b"", b""));
    }

    #[test]
    fn tokens_are_64_hex_and_unique() {
        let a = new_token().unwrap();
        let b = new_token().unwrap();
        assert_eq!(a.len(), 64);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_ne!(a, b);
    }

    #[test]
    fn limits_are_clamped() {
        assert_eq!(clamp_limit(None), DEFAULT_LIMIT);
        assert_eq!(clamp_limit(Some(0)), 1);
        assert_eq!(clamp_limit(Some(7)), 7);
        assert_eq!(clamp_limit(Some(usize::MAX)), MAX_LIMIT);
        assert_eq!(clamp_chars(None), DEFAULT_CHARS);
        assert_eq!(clamp_chars(Some(0)), 1);
        assert_eq!(clamp_chars(Some(usize::MAX)), MAX_CHARS);
    }

    #[test]
    fn slim_drops_thumbnails_and_highlight_marks() {
        let hit = json!({
            "path": "C:/x/a.md",
            "thumb": "base64...",
            "snippet": "a \u{1}vector\u{2} store",
        });
        let s = slim(hit);
        assert!(s.get("thumb").is_none());
        assert_eq!(s["snippet"], "a vector store");
        assert_eq!(s["path"], "C:/x/a.md");
        // non-objects and objects without those keys pass through untouched
        assert_eq!(slim(json!(3)), json!(3));
        assert_eq!(slim(json!({ "url": "u" })), json!({ "url": "u" }));
    }

    #[test]
    fn client_command_carries_url_and_bearer() {
        let c = client_command(5199, "deadbeef");
        assert_eq!(
            c,
            "claude mcp add --transport http magpie http://127.0.0.1:5199/mcp --header \"Authorization: Bearer deadbeef\""
        );
        assert_eq!(url(80), "http://127.0.0.1:80/mcp");
    }

    #[test]
    fn source_names_are_lowercase_on_the_wire() {
        assert_eq!(serde_json::to_string(&Source::Local).unwrap(), "\"local\"");
        assert_eq!(serde_json::from_str::<Source>("\"clips\"").unwrap(), Source::Clips);
        assert!(serde_json::from_str::<Source>("\"Local\"").is_err());
    }
}
