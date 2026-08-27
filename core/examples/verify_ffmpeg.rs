//! E2E check of the managed-ffmpeg chain: with MAGPIE_FORCE_MANAGED_FFMPEG=1
//! this downloads magpie's release asset, unpacks the single binary, and runs
//! `-version` on it. Usage: cargo run -p magpie-core --example verify_ffmpeg
fn main() -> anyhow::Result<()> {
    let dir = std::env::temp_dir().join("magpie-ffmpeg-e2e");
    let _ = std::fs::remove_dir_all(&dir);
    let (path, label) = magpie_core::videos::ensure_ffmpeg_with(&dir, &mut |m| println!("  {m}"))?;
    println!("resolved: {} ({label})", path.display());
    let out = std::process::Command::new(&path).arg("-version").output()?;
    anyhow::ensure!(out.status.success(), "ffmpeg -version failed");
    let first = String::from_utf8_lossy(&out.stdout);
    println!("{}", first.lines().next().unwrap_or(""));
    Ok(())
}
