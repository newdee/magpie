//! E2E check for the OCR engine: downloads the models (once) into a local
//! cache and extracts text from the image given on the command line.
//! Run: cargo run -p magpie-core --example verify_ocr -- <image> [cache_dir]

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let img_path = args.next().expect("usage: verify_ocr <image> [cache_dir]");
    let cache = args
        .next()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("magpie-ocr-test"));
    let mut ocr = magpie_core::ocr::Ocr::new(&cache, &mut |s| eprintln!("[{s}]"))?;
    let img = image::open(&img_path)?;
    let text = ocr.extract_text(&img)?;
    println!("---\n{text}\n---");
    Ok(())
}
