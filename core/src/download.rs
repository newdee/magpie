//! Plain-HTTPS model file fetcher.
//!
//! hf-hub's protocol depends on ETag headers that mirrors and interfering
//! middleboxes often strip ("header etag is missing"), and its metadata
//! round-trips add more ways to fail on hostile networks. Files here come
//! from the `{endpoint}/{repo}/resolve/main/{path}` URL scheme that
//! huggingface.co and every mirror serve as plain static downloads: one GET
//! per file, HTTP Range resume across retries, atomic rename on completion.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};

/// Mirror-aware endpoint, e.g. `https://hf-mirror.com` when the user picked it.
pub fn hf_endpoint() -> String {
    std::env::var("HF_ENDPOINT")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://huggingface.co".to_string())
}

pub fn file_url(endpoint: &str, repo: &str, path: &str) -> String {
    format!("{}/{repo}/resolve/main/{path}", endpoint.trim_end_matches('/'))
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        // only the native-tls provider is compiled in (matching hf-hub's tree)
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .provider(ureq::tls::TlsProvider::NativeTls)
                .build(),
        )
        .timeout_connect(Some(Duration::from_secs(20)))
        // headers must arrive promptly; the body itself may stream for minutes
        .timeout_recv_response(Some(Duration::from_secs(60)))
        .build()
        .into()
}

/// Download `url` into `dest` atomically (`<dest>.part` + rename), resuming a
/// partial file across up to 4 attempts. `progress(done_bytes, total_bytes)`.
/// An existing complete `dest` is reused without touching the network.
pub fn fetch_file(
    url: &str,
    dest: &Path,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<()> {
    if dest.is_file() {
        return Ok(());
    }
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let part = PathBuf::from(format!("{}.part", dest.display()));
    let mut attempt = 0;
    loop {
        attempt += 1;
        match fetch_into_part(url, &part, progress) {
            Ok(()) => break,
            Err(_) if attempt < 4 => {
                // the .part file survives; the next attempt resumes from it
                std::thread::sleep(Duration::from_secs(2));
            }
            Err(e) => return Err(e).with_context(|| format!("download {url}")),
        }
    }
    std::fs::rename(&part, dest)?;
    Ok(())
}

fn fetch_into_part(
    url: &str,
    part: &Path,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<()> {
    let start = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    let mut req = agent().get(url);
    if start > 0 {
        req = req.header("Range", &format!("bytes={start}-"));
    }
    let response = req.call().map_err(|e| anyhow::anyhow!("GET failed: {e}"))?;
    let status = response.status().as_u16();
    if status != 200 && status != 206 {
        bail!("HTTP {status}");
    }
    let header = |name: &str| -> Option<String> {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let resumed = status == 206;
    if resumed {
        // never append blindly: the advertised range must start where our
        // partial file ends, or the result would be silently corrupt
        let range_start = header("content-range").and_then(|v| {
            v.strip_prefix("bytes ")
                .and_then(|r| r.split('-').next())
                .and_then(|s| s.trim().parse::<u64>().ok())
        });
        if range_start != Some(start) {
            let _ = std::fs::remove_file(part);
            bail!("server range mismatch (asked {start}, got {range_start:?})");
        }
    }
    // total size: from Content-Range when resuming, Content-Length otherwise
    let total = if resumed {
        header("content-range")
            .and_then(|v| v.rsplit('/').next().and_then(|t| t.parse::<u64>().ok()))
    } else {
        header("content-length").and_then(|v| v.parse::<u64>().ok())
    };
    let mut file = if resumed {
        std::fs::OpenOptions::new().append(true).open(part)?
    } else {
        // server ignored the Range header (or fresh start): write from scratch
        std::fs::File::create(part)?
    };
    let mut done = if resumed { start } else { 0 };
    let (_, body) = response.into_parts();
    let mut reader = body.into_reader();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        done += n as u64;
        progress(done, total);
    }
    file.flush()?;
    if let Some(t) = total {
        if done < t {
            // connection dropped mid-body; keep .part so the retry resumes
            bail!("short read: {done}/{t} bytes");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    #[test]
    fn builds_resolve_urls() {
        assert_eq!(
            file_url("https://hf-mirror.com/", "intfloat/multilingual-e5-small", "onnx/model.onnx"),
            "https://hf-mirror.com/intfloat/multilingual-e5-small/resolve/main/onnx/model.onnx"
        );
    }

    /// Minimal HTTP server: first request is cut off mid-body, the resumed
    /// Range request serves the rest. Proves resume + atomic rename.
    #[test]
    fn resumes_after_truncated_transfer() {
        let payload: Vec<u8> = (0u32..50_000).flat_map(|i| i.to_le_bytes()).collect();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = payload.clone();
        let handle = std::thread::spawn(move || {
            for i in 0..2 {
                let (mut sock, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(sock.try_clone().unwrap());
                let mut from = 0usize;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    let line = line.trim().to_ascii_lowercase();
                    if let Some(r) = line.strip_prefix("range: bytes=") {
                        from = r.trim_end_matches('-').parse().unwrap();
                    }
                    if line.is_empty() {
                        break;
                    }
                }
                if i == 0 {
                    // full-length header, then drop the socket halfway through
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        served.len()
                    );
                    sock.write_all(head.as_bytes()).unwrap();
                    sock.write_all(&served[..served.len() / 2]).unwrap();
                    drop(sock); // truncation
                } else {
                    let rest = &served[from..];
                    let head = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n",
                        rest.len(),
                        from,
                        served.len() - 1,
                        served.len()
                    );
                    sock.write_all(head.as_bytes()).unwrap();
                    sock.write_all(rest).unwrap();
                }
            }
        });

        let dir = std::env::temp_dir().join(format!("magpie-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let dest = dir.join("blob.bin");
        let mut last = (0u64, None);
        fetch_file(
            &format!("http://127.0.0.1:{port}/x/resolve/main/blob.bin"),
            &dest,
            &mut |d, t| last = (d, t),
        )
        .unwrap();
        handle.join().unwrap();
        let got = std::fs::read(&dest).unwrap();
        assert_eq!(got.len(), payload.len(), "resumed to full length");
        assert_eq!(got, payload, "resumed bytes are correct, not repeated");
        assert_eq!(last.0, payload.len() as u64);
        assert!(!dir.join("blob.bin.part").exists(), "part file renamed away");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
