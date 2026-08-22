//! Local embedding via fastembed (multilingual-e5-small, 384-dim).
//!
//! E5 models require "query: " / "passage: " prefixes; vectors are L2-normalized
//! at creation so similarity is a plain dot product.

use anyhow::{Context, Result};
use fastembed::{
    EmbeddingModel, InitOptionsUserDefined, Pooling, TextEmbedding, TextInitOptions,
    TokenizerFiles, UserDefinedEmbeddingModel,
};
use std::path::Path;

use crate::db::EmbedCandidate;
use crate::download;

const E5_REPO: &str = "intfloat/multilingual-e5-small";
/// (remote path in the repo, local file name in the manual cache)
const E5_FILES: [(&str, &str); 5] = [
    ("onnx/model.onnx", "model.onnx"),
    ("tokenizer.json", "tokenizer.json"),
    ("config.json", "config.json"),
    ("special_tokens_map.json", "special_tokens_map.json"),
    ("tokenizer_config.json", "tokenizer_config.json"),
];

pub struct Embedder {
    model: TextEmbedding,
}

impl Embedder {
    /// Loads (and on first run downloads) the model. Blocking; call off the UI thread.
    pub fn new(cache_dir: &Path) -> Result<Self> {
        let model = TextEmbedding::try_new(
            TextInitOptions::new(EmbeddingModel::MultilingualE5Small)
                .with_cache_dir(cache_dir.to_path_buf())
                .with_show_download_progress(false),
        )?;
        Ok(Self { model })
    }

    /// Like [`Self::new`], but when the hf-hub download protocol fails (its
    /// ETag/metadata round-trips break behind mirrors and interfering
    /// middleboxes) the model files are fetched as plain static downloads and
    /// fed to fastembed from bytes. `progress` receives display strings.
    pub fn new_with_fallback(
        cache_dir: &Path,
        progress: &mut dyn FnMut(String),
    ) -> Result<Self> {
        let primary = match Self::new(cache_dir) {
            Ok(s) => return Ok(s),
            Err(e) => e,
        };
        Self::new_direct(cache_dir, progress)
            .with_context(|| format!("direct download (hf-hub failed first: {primary})"))
    }

    /// The fallback path on its own: plain static downloads, no hf-hub.
    pub fn new_direct(cache_dir: &Path, progress: &mut dyn FnMut(String)) -> Result<Self> {
        let manual = cache_dir.join("manual-e5");
        let endpoint = download::hf_endpoint();
        for (remote, local) in E5_FILES {
            let url = download::file_url(&endpoint, E5_REPO, remote);
            download::fetch_file(&url, &manual.join(local), &mut |done, total| {
                progress(match total {
                    Some(t) if t > 0 => {
                        format!("downloading {local} {}%", done * 100 / t)
                    }
                    _ => format!("downloading {local}"),
                });
            })?;
        }
        progress("loading".to_string());
        let read = |name: &str| {
            std::fs::read(manual.join(name)).with_context(|| format!("read {name}"))
        };
        let user_model = UserDefinedEmbeddingModel::new(
            read("model.onnx")?,
            TokenizerFiles {
                tokenizer_file: read("tokenizer.json")?,
                config_file: read("config.json")?,
                special_tokens_map_file: read("special_tokens_map.json")?,
                tokenizer_config_file: read("tokenizer_config.json")?,
            },
        )
        // must match what fastembed's built-in MultilingualE5Small uses
        .with_pooling(Pooling::Mean);
        let model = TextEmbedding::try_new_from_user_defined(
            user_model,
            InitOptionsUserDefined::new().with_max_length(512),
        )?;
        Ok(Self { model })
    }

    pub fn embed_passages(&mut self, docs: &[String]) -> Result<Vec<Vec<f32>>> {
        let prefixed: Vec<String> = docs.iter().map(|d| format!("passage: {d}")).collect();
        let mut vecs = self.model.embed(&prefixed, Some(16))?;
        for v in &mut vecs {
            normalize(v);
        }
        Ok(vecs)
    }

    pub fn embed_query(&mut self, query: &str) -> Result<Vec<f32>> {
        let mut vecs = self.model.embed(&[format!("query: {query}")], None)?;
        let mut v = vecs.pop().unwrap_or_default();
        normalize(&mut v);
        Ok(v)
    }
}

pub fn normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-12 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// Dot product; equals cosine for normalized vectors.
pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Repo identity header prepended to every README chunk before embedding.
pub fn compose_repo_header(c: &EmbedCandidate) -> String {
    let mut doc = String::with_capacity(256);
    doc.push_str(&c.full_name);
    doc.push('\n');
    if let Some(d) = &c.description {
        doc.push_str(d);
        doc.push('\n');
    }
    if !c.topics.is_empty() {
        doc.push_str("Topics: ");
        doc.push_str(&c.topics.join(", "));
        doc.push('\n');
    }
    if let Some(l) = &c.language {
        doc.push_str("Language: ");
        doc.push_str(l);
    }
    doc.trim_end().to_string()
}

/// Strip markdown noise: badges, images, HTML tags, link targets, code fences.
pub fn clean_markdown(md: &str) -> String {
    let mut out = String::with_capacity(md.len().min(8 * 1024));
    let mut in_fence = false;
    for line in md.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue; // code adds noise, not meaning
        }
        let mut s = strip_inline(trimmed);
        s = s.trim_matches(|ch: char| ch == '#' || ch == '>' || ch.is_whitespace())
            .to_string();
        if s.is_empty() {
            continue;
        }
        out.push_str(&s);
        out.push('\n');
    }
    out
}

/// Remove images/badges entirely, unwrap [text](url) to text, drop HTML tags.
fn strip_inline(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        // image / badge: ![alt](url) — drop wholesale
        if ch == '!' && i + 1 < chars.len() && chars[i + 1] == '[' {
            if let Some(end) = skip_link(&chars, i + 1) {
                i = end;
                continue;
            }
        }
        // link: [text](url) — keep text
        if ch == '[' {
            if let Some(close) = matching_bracket(&chars, i) {
                let text: String = chars[i + 1..close].iter().collect();
                let after = close + 1;
                if after < chars.len() && chars[after] == '(' {
                    if let Some(paren_end) = chars[after..].iter().position(|&c| c == ')') {
                        out.push_str(&strip_inline(&text));
                        i = after + paren_end + 1;
                        continue;
                    }
                }
            }
        }
        // HTML tag: <...> — drop
        if ch == '<' {
            if let Some(gt) = chars[i..].iter().position(|&c| c == '>') {
                i += gt + 1;
                continue;
            }
        }
        // inline code backticks: keep content, drop ticks
        if ch != '`' {
            out.push(ch);
        }
        i += 1;
    }
    out
}

/// Given chars[start] == '[', find the matching ']' then a trailing "(...)" and
/// return the index just past ')'. Used to skip whole image links.
fn skip_link(chars: &[char], start: usize) -> Option<usize> {
    let close = matching_bracket(chars, start)?;
    let after = close + 1;
    if after < chars.len() && chars[after] == '(' {
        let paren_end = chars[after..].iter().position(|&c| c == ')')?;
        Some(after + paren_end + 1)
    } else {
        Some(after)
    }
}

fn matching_bracket(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0;
    for (idx, &c) in chars.iter().enumerate().skip(open) {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(idx);
                }
            }
            _ => {}
        }
    }
    None
}

/// FNV-1a 64-bit hex hash — cheap change detector for embedding staleness.
pub fn doc_hash(doc: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in doc.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_cleanup() {
        let md = "# Title\n\
                  [![CI](https://img.shields.io/badge.svg)](https://ci.example.com)\n\
                  A **great** tool. See [docs](https://d.example.com).\n\
                  <img src=\"x.png\">\n\
                  ```rust\nfn main() {}\n```\n\
                  End.";
        let cleaned = clean_markdown(md);
        assert!(cleaned.contains("Title"));
        assert!(cleaned.contains("A **great** tool. See docs."));
        assert!(cleaned.contains("End."));
        assert!(!cleaned.contains("shields.io"), "badge must vanish");
        assert!(!cleaned.contains("fn main"), "code fence must vanish");
        assert!(!cleaned.contains("img src"), "html must vanish");
    }

    #[test]
    fn normalize_and_dot() {
        let mut v = vec![3.0, 4.0];
        normalize(&mut v);
        assert!((dot(&v, &v) - 1.0).abs() < 1e-6);
        let mut zero = vec![0.0, 0.0];
        normalize(&mut zero); // must not NaN
        assert_eq!(zero, vec![0.0, 0.0]);
    }

    #[test]
    fn doc_hash_stable_and_distinct() {
        assert_eq!(doc_hash("abc"), doc_hash("abc"));
        assert_ne!(doc_hash("abc"), doc_hash("abd"));
        assert_eq!(doc_hash("abc").len(), 16);
    }

    #[test]
    fn repo_header_shape() {
        let c = crate::db::EmbedCandidate {
            id: 1,
            full_name: "a/b".into(),
            description: Some("desc".into()),
            topics: vec!["t1".into(), "t2".into()],
            language: Some("Rust".into()),
            readme: Some("ignored here".into()),
        };
        assert_eq!(
            compose_repo_header(&c),
            "a/b\ndesc\nTopics: t1, t2\nLanguage: Rust"
        );
    }
}
