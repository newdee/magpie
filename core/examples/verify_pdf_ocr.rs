//! E2E for scanned-PDF OCR: build a one-page PDF whose page is a single
//! embedded JPEG (the shape every scanner produces), confirm pdf-inspector
//! routes that page to OCR, pull the image back out, and read its text.
//! Run: cargo run -p magpie-core --example verify_pdf_ocr -- <text.png> [ocr_cache]

use lopdf::{dictionary, Document, Object, Stream};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let png = args.next().expect("usage: verify_pdf_ocr <text.png> [ocr_cache]");
    let cache = args
        .next()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("magpie-ocr-test"));

    // re-encode the known-text image as JPEG (what DCTDecode carries)
    let img = image::open(&png)?.to_rgb8();
    let (w, h) = img.dimensions();
    let mut jpeg = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img).write_to(&mut jpeg, image::ImageFormat::Jpeg)?;

    // minimal scanned-PDF shape: one page, one full-page image XObject
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let image_stream = Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image",
            "Width" => w as i64, "Height" => h as i64,
            "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
            "Filter" => "DCTDecode",
        },
        jpeg.into_inner(),
    )
    .with_compression(false);
    let image_id = doc.add_object(image_stream);
    let content = format!("q {w} 0 0 {h} 0 0 cm /Im0 Do Q");
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), (w as i64).into(), (h as i64).into()],
        "Contents" => content_id,
        "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => image_id } },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let dir = std::env::temp_dir().join("magpie-pdf-ocr-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    let pdf_path = dir.join("scan.pdf");
    doc.save(&pdf_path)?;

    // the worker's exact pipeline: route -> extract page image -> OCR
    let (pages, native) =
        magpie_core::files::pdf_ocr_plan(&pdf_path).expect("plan must parse the pdf");
    anyhow::ensure!(!pages.is_empty(), "image-only page must be routed to OCR, got none");
    anyhow::ensure!(native.trim().is_empty(), "no text layer expected, got {native:?}");
    let imgs = magpie_core::files::pdf_page_images(&pdf_path, &pages, 50);
    anyhow::ensure!(imgs.len() == 1, "embedded page image must be recovered");
    let mut ocr = magpie_core::ocr::Ocr::new(&cache, &mut |_| {})?;
    let text = ocr.extract_text(&imgs[0])?;
    anyhow::ensure!(text.contains("Hello"), "latin read back, got {text:?}");
    anyhow::ensure!(text.contains("本地搜索"), "CJK read back, got {text:?}");
    println!("ok: pages_needing_ocr={pages:?}, text={text:?}");
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
