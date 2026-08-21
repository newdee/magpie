//! Diagnostic: open a real magpie DB and print what the folder queries return.
//! Usage: cargo run -p magpie-core --example dbcheck -- <path-to-stars.db>

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).expect("usage: dbcheck <stars.db>");
    let conn = magpie_core::db::open(std::path::Path::new(&path))?;
    let folders = magpie_core::files::list_folders(&conn)?;
    let folder_count = magpie_core::files::folder_count(&conn)?;
    let file_count = magpie_core::files::file_count(&conn)?;
    println!("folder_count={folder_count} file_count={file_count}");
    for f in &folders {
        println!("  id={} files={} path={}", f.id, f.file_count, f.path);
    }
    let json = serde_json::to_string(&folders)?;
    println!("serialized: {json}");
    Ok(())
}
