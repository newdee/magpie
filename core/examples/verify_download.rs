//! Real-network verification of the direct (non-hf-hub) model download path.
//!
//! Usage: cargo run -p magpie-core --example verify_download -- <primary-cache-dir>
//!
//! Downloads both models from the endpoint in HF_ENDPOINT into a fresh temp
//! dir via the plain-GET fallback, then compares its embeddings against the
//! primary (hf-hub-cached) models from <primary-cache-dir>. Byte-identical
//! configs must give near-identical vectors.

use magpie_core::embed::{dot, Embedder};
use magpie_core::siglip::Siglip;

fn main() -> anyhow::Result<()> {
    let primary_dir = std::env::args()
        .nth(1)
        .expect("usage: verify_download <primary-cache-dir>");
    let endpoint = std::env::var("HF_ENDPOINT").unwrap_or_else(|_| "https://huggingface.co".into());
    println!("endpoint: {endpoint}");

    let tmp = std::env::temp_dir().join(format!("magpie-verify-dl-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);

    let t = std::time::Instant::now();
    let mut last = String::new();
    let mut direct = Embedder::new_direct(&tmp, &mut |m| {
        if m != last && (m.ends_with("0%") || !m.contains('%')) {
            println!("  e5: {m}");
            last = m;
        }
    })?;
    println!("e5 direct download + load: {:?}", t.elapsed());

    let t = std::time::Instant::now();
    let mut last = String::new();
    let mut siglip_direct = Siglip::new_direct(&tmp, &mut |m| {
        if m != last && (m.ends_with("0%") || !m.contains('%')) {
            println!("  siglip: {m}");
            last = m;
        }
    })?;
    println!("siglip direct download + load: {:?}", t.elapsed());

    // primary models from the existing cache (no network)
    let mut primary = Embedder::new(std::path::Path::new(&primary_dir))?;
    let mut siglip_primary = Siglip::new(std::path::Path::new(&primary_dir))?;

    let q = "如何在本地做向量检索";
    let a = primary.embed_query(q)?;
    let b = direct.embed_query(q)?;
    let sim = dot(&a, &b);
    println!("e5 primary-vs-direct cosine: {sim:.6}");
    assert!(sim > 0.999, "e5 fallback diverges from primary: {sim}");

    let a = siglip_primary.embed_query("a photo of a bird")?;
    let b = siglip_direct.embed_query("a photo of a bird")?;
    let sim = dot(&a, &b);
    println!("siglip primary-vs-direct cosine: {sim:.6}");
    assert!(sim > 0.999, "siglip fallback diverges from primary: {sim}");

    let _ = std::fs::remove_dir_all(&tmp);
    println!("OK: direct download path verified against primary models");
    Ok(())
}
