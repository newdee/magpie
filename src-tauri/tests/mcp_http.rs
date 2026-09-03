//! The MCP server end to end over real HTTP, with a fake backend: bearer
//! auth, loopback-only Host, the handshake, tool discovery and the three
//! tools' shapes. What Tauri adds on top (the real backend) is covered on a
//! real machine.
use std::sync::Arc;

use magpie_lib::mcp::{self, Backend, ReadOutcome, Source};
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ClientInfo},
    transport::{
        StreamableHttpClientTransport,
        streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};

struct Fake;

impl Backend for Fake {
    fn search(&self, source: Source, query: &str, limit: usize) -> anyhow::Result<Vec<Value>> {
        Ok(vec![json!({
            "kind": "file",
            "path": "C:/x/a.md",
            "name": "a.md",
            "score": 1.0,
            "thumb": "zzz",
            "snippet": format!("\u{1}{query}\u{2} store"),
            "source": format!("{source:?}"),
            "limit": limit,
        })])
    }

    fn read_file(&self, path: &str, max_chars: usize) -> anyhow::Result<ReadOutcome> {
        Ok(match path {
            "C:/x/a.md" => ReadOutcome::Text(json!({ "path": path, "text": "hello", "max": max_chars })),
            "C:/x/b.bin" => ReadOutcome::NotIndexed,
            _ => ReadOutcome::Outside,
        })
    }

    fn recent(&self, source: Source, limit: usize) -> anyhow::Result<Vec<Value>> {
        Ok(vec![json!({ "kind": "repo", "full_name": "a/b", "source": format!("{source:?}"), "limit": limit })])
    }
}

const TOKEN: &str = "0123456789abcdef0123456789abcdef";

/// A client that talks to the loopback port itself. reqwest honours the
/// system proxy by default, and a machine with one configured would send
/// even 127.0.0.1 through it (a 502 from the proxy, not our server).
fn direct() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().expect("http client")
}

async fn start() -> mcp::Server {
    mcp::serve(Arc::new(Fake), TOKEN.into(), 0).await.expect("bind a loopback port")
}

#[tokio::test]
async fn no_token_or_a_wrong_token_is_turned_away_before_any_session() {
    let server = start().await;
    let url = mcp::url(server.port);
    let http = direct();
    let init = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": { "name": "t", "version": "0" } } });

    let none = http.post(&url).json(&init).send().await.unwrap();
    assert_eq!(none.status(), 401);
    assert_eq!(none.headers().get("www-authenticate").unwrap(), "Bearer");
    assert!(none.headers().get("mcp-session-id").is_none(), "no session for a rejected caller");

    let wrong = http
        .post(&url)
        .bearer_auth("0123456789abcdef0123456789abcdeX")
        .json(&init)
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), 401);

    let basic = http.post(&url).header("authorization", "Basic abc").json(&init).send().await.unwrap();
    assert_eq!(basic.status(), 401);
    server.stop();
}

#[tokio::test]
async fn a_foreign_host_header_is_rejected_even_with_the_token() {
    // DNS rebinding: a page at evil.example resolving to 127.0.0.1 would send
    // its own Host; rmcp's loopback-only rule refuses it
    let server = start().await;
    let http = direct();
    let res = http
        .post(mcp::url(server.port))
        .bearer_auth(TOKEN)
        .header("host", "evil.example")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403, "{}", res.text().await.unwrap_or_default());
    server.stop();
}

#[tokio::test]
async fn handshake_lists_three_read_only_tools_and_they_answer() {
    let server = start().await;
    let transport = StreamableHttpClientTransport::with_client(
        direct(),
        StreamableHttpClientTransportConfig::with_uri(mcp::url(server.port)).auth_header(TOKEN),
    );
    let client = ClientInfo::default().serve(transport).await.expect("initialize");
    let info = client.peer_info().expect("server info");
    assert_eq!(info.server_info.as_ref().map(|s| s.name.as_str()), Some("magpie"));
    assert!(info.instructions.as_deref().unwrap_or("").contains("read-only"));

    let tools = client.list_tools(Default::default()).await.expect("tools/list");
    let mut names: Vec<&str> = tools.tools.iter().map(|t| t.name.as_ref()).collect();
    names.sort_unstable();
    assert_eq!(names, ["read_file", "recent", "search"]);
    let search = tools.tools.iter().find(|t| t.name == "search").unwrap();
    let props = &search.input_schema["properties"];
    assert!(props.get("source").is_some() && props.get("query").is_some() && props.get("limit").is_some());
    let sources = search.input_schema["properties"]["source"]["enum"]
        .as_array()
        .or_else(|| search.input_schema["$defs"]["Source"]["enum"].as_array())
        .expect("source is an enum in the schema");
    assert_eq!(sources.len(), 5);

    // search: hits come back slimmed (no thumbnail, no highlight marks)
    let r = client
        .call_tool(CallToolRequestParams::new("search").with_arguments(
            json!({ "source": "local", "query": "vector", "limit": 3 }).as_object().unwrap().clone(),
        ))
        .await
        .expect("tools/call search");
    let s = r.structured_content.expect("structured search result");
    assert_eq!(s["source"], "local");
    assert_eq!(s["count"], 1);
    assert_eq!(s["hits"][0]["snippet"], "vector store");
    assert!(s["hits"][0].get("thumb").is_none());
    assert_eq!(s["hits"][0]["limit"], 3);
    assert_eq!(r.is_error, Some(false));

    // a limit beyond the cap is clamped, not rejected
    let r = client
        .call_tool(CallToolRequestParams::new("search").with_arguments(
            json!({ "source": "clips", "query": "x", "limit": 999 }).as_object().unwrap().clone(),
        ))
        .await
        .unwrap();
    assert_eq!(r.structured_content.unwrap()["hits"][0]["limit"], mcp::MAX_LIMIT);

    // read_file: text, outside, not indexed
    let r = client
        .call_tool(CallToolRequestParams::new("read_file").with_arguments(
            json!({ "path": "C:/x/a.md" }).as_object().unwrap().clone(),
        ))
        .await
        .unwrap();
    let v = r.structured_content.unwrap();
    assert_eq!(v["text"], "hello");
    assert_eq!(v["max"], mcp::DEFAULT_CHARS);
    let r = client
        .call_tool(CallToolRequestParams::new("read_file").with_arguments(
            json!({ "path": "C:/elsewhere/secret.txt" }).as_object().unwrap().clone(),
        ))
        .await
        .unwrap();
    assert_eq!(r.is_error, Some(true));
    assert!(r.structured_content.unwrap()["error"].as_str().unwrap().contains("not inside"));
    let r = client
        .call_tool(CallToolRequestParams::new("read_file").with_arguments(
            json!({ "path": "C:/x/b.bin" }).as_object().unwrap().clone(),
        ))
        .await
        .unwrap();
    assert_eq!(r.is_error, Some(true));

    // recent
    let r = client
        .call_tool(CallToolRequestParams::new("recent").with_arguments(
            json!({ "source": "stars" }).as_object().unwrap().clone(),
        ))
        .await
        .unwrap();
    let v = r.structured_content.unwrap();
    assert_eq!(v["items"][0]["full_name"], "a/b");
    assert_eq!(v["items"][0]["limit"], mcp::DEFAULT_LIMIT);

    // a bad source is a protocol-level invalid-params error, not a crash
    let bad = client
        .call_tool(CallToolRequestParams::new("search").with_arguments(
            json!({ "source": "email", "query": "x" }).as_object().unwrap().clone(),
        ))
        .await;
    assert!(bad.is_err() || bad.as_ref().unwrap().is_error == Some(true));

    client.cancel().await.expect("close client");
    server.stop();
}

#[tokio::test]
async fn stopping_the_server_closes_the_port() {
    let server = start().await;
    let url = mcp::url(server.port);
    let port = server.port;
    server.stop();
    // give the graceful shutdown a moment, then the port must refuse
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let res = direct().post(&url).bearer_auth(TOKEN).body("{}").send().await;
    assert!(res.is_err(), "port {port} still answers after stop");
}
