//! E2E check for the model-download fallback chain: an unreachable primary
//! source must fall through to magpie's own `models-1` release assets.
//! Run: cargo run -p magpie-core --example verify_model_fallback

fn main() -> anyhow::Result<()> {
    let dir = std::env::temp_dir().join("magpie-model-fallback-test");
    let _ = std::fs::remove_dir_all(&dir);
    let dest = dir.join("e5-config.json");
    let urls = [
        // guaranteed-dead primary (nothing listens on port 9)
        "https://127.0.0.1:9/e5-config.json".to_string(),
        format!("{}/e5-config.json", magpie_core::download::MODELS_BASE),
    ];
    magpie_core::download::fetch_file_any(&urls, &dest, &mut |_, _| {})?;
    let text = std::fs::read_to_string(&dest)?;
    let v: serde_json::Value = serde_json::from_str(&text)?;
    println!(
        "ok: fell through to release asset, {} bytes, model_type={}",
        text.len(),
        v["model_type"]
    );
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
