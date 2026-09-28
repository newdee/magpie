//! Where the ONNX Runtime library comes from.
//!
//! Windows and macOS link it into the binary (`ort`'s prebuilt). On Linux that
//! prebuilt needs glibc 2.38, which leaves out Ubuntu 22.04 and Debian 12, so
//! there `ort` is built with `load-dynamic` and the bundles ship Microsoft's
//! own `libonnxruntime.so` (glibc 2.27), fetched by
//! `scripts/fetch-onnxruntime.sh`. Every model constructor calls
//! [`ensure_runtime`] first, so a missing library is a plain error in the
//! model status instead of a panic inside `ort`.

/// Load the ONNX Runtime library once. A no-op where it is linked in.
pub fn ensure_runtime() -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        use std::sync::OnceLock;
        static LOADED: OnceLock<Result<(), String>> = OnceLock::new();
        LOADED
            .get_or_init(|| {
                let tried = candidates();
                let path = tried.iter().find(|p| p.is_file()).ok_or_else(|| {
                    let list = tried.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ");
                    format!("ONNX Runtime library not found (looked in {list})")
                })?;
                ort::init_from(path).map_err(|e| format!("load {}: {e}", path.display()))?.commit();
                log::info!("onnx runtime loaded from {}", path.display());
                Ok(())
            })
            .clone()
            .map_err(anyhow::Error::msg)
    }
    #[cfg(not(target_os = "linux"))]
    Ok(())
}

/// In order: `ORT_DYLIB_PATH` (tests, custom installs), then the bundles'
/// resource folder beside the binary (`/usr/bin/magpie` →
/// `/usr/lib/magpie/`, the same inside the AppImage), then the binary's own
/// folder.
#[cfg(any(target_os = "linux", test))]
fn candidates() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Some(p) = std::env::var_os("ORT_DYLIB_PATH").filter(|p| !p.is_empty()) {
        out.push(std::path::PathBuf::from(p));
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.to_path_buf())) {
        out.push(dir.join("../lib/magpie/libonnxruntime.so"));
        out.push(dir.join("libonnxruntime.so"));
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_bundle_folder_is_looked_in_after_the_override() {
        let c = super::candidates();
        let names: Vec<String> = c.iter().map(|p| p.to_string_lossy().replace('\\', "/")).collect();
        let bundle = names.iter().position(|p| p.ends_with("/../lib/magpie/libonnxruntime.so")).expect("bundle folder");
        let beside = names.iter().position(|p| p.ends_with("/libonnxruntime.so") && !p.contains("/lib/magpie/")).expect("beside the binary");
        assert!(bundle < beside);
        if std::env::var_os("ORT_DYLIB_PATH").is_some_and(|p| !p.is_empty()) {
            assert_eq!(bundle, 1, "the override comes first");
        }
    }
}
