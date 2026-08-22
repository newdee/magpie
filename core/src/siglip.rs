//! SigLIP 2 dual encoder (text <-> image) run directly on ort.
//!
//! fastembed's ImageEmbedding assumes CLS-token pooling for 3D outputs, which
//! SigLIP breaks (MAP-head pooling, no CLS token) — so this module owns the
//! ONNX sessions, preprocessing, and output selection itself.

use anyhow::{anyhow, bail, Context, Result};
use image::imageops::FilterType;
use ndarray::{Array2, Array4};
use ort::session::Session;
use ort::value::Value;
use std::path::Path;
use tokenizers::Tokenizer;

/// Identity of the image embedding space; changing model/files MUST change it
/// (stored vectors are invalidated automatically when it differs).
pub const IMAGE_EMBED_MODEL_ID: &str = "siglip2-base-patch16-224-int8";

const REPO: &str = "onnx-community/siglip2-base-patch16-224-ONNX";
const TEXT_ONNX: &str = "onnx/text_model_quantized.onnx";
const VISION_ONNX: &str = "onnx/vision_model_quantized.onnx";
/// SigLIP is trained on 64-token inputs padded with full attention.
const TEXT_SEQ_LEN: usize = 64;
/// Pooled output keys, in preference order, for optimum/transformers.js exports.
const POOLED_KEYS: &[&str] = &["pooler_output", "text_embeds", "image_embeds"];

pub struct Siglip {
    text: Session,
    vision: Session,
    tokenizer: Tokenizer,
    pad_id: i64,
    img_size: u32,
    mean: [f32; 3],
    std: [f32; 3],
    resample: FilterType,
}

/// Everything Siglip needs on disk, however it was downloaded.
struct ModelPaths {
    text: std::path::PathBuf,
    vision: std::path::PathBuf,
    tokenizer: std::path::PathBuf,
    preprocessor: std::path::PathBuf,
    tokenizer_config: Option<std::path::PathBuf>,
}

impl Siglip {
    /// Downloads (or reuses) the model files, then loads both sessions.
    /// Blocking; call off the UI thread.
    pub fn new(cache_dir: &Path) -> Result<Self> {
        Self::new_with_progress(cache_dir, &mut |_| {})
    }

    /// hf-hub first (reuses existing caches); on failure the files are pulled
    /// as plain static downloads, which survive mirrors and middleboxes that
    /// strip the ETag headers hf-hub's protocol requires.
    pub fn new_with_progress(
        cache_dir: &Path,
        progress: &mut dyn FnMut(String),
    ) -> Result<Self> {
        let paths = match Self::hf_hub_paths(cache_dir) {
            Ok(p) => p,
            Err(primary) => Self::direct_paths(cache_dir, progress)
                .with_context(|| format!("direct download (hf-hub failed first: {primary})"))?,
        };
        progress("loading".to_string());
        Self::from_paths(paths)
    }

    /// The fallback path on its own: plain static downloads, no hf-hub.
    pub fn new_direct(cache_dir: &Path, progress: &mut dyn FnMut(String)) -> Result<Self> {
        let paths = Self::direct_paths(cache_dir, progress)?;
        progress("loading".to_string());
        Self::from_paths(paths)
    }

    fn hf_hub_paths(cache_dir: &Path) -> Result<ModelPaths> {
        let api = hf_hub::api::sync::ApiBuilder::new()
            .with_cache_dir(cache_dir.to_path_buf())
            .build()?;
        let repo = api.model(REPO.to_string());
        Ok(ModelPaths {
            text: repo.get(TEXT_ONNX)?,
            vision: repo.get(VISION_ONNX)?,
            tokenizer: repo.get("tokenizer.json")?,
            preprocessor: repo.get("preprocessor_config.json")?,
            tokenizer_config: repo.get("tokenizer_config.json").ok(),
        })
    }

    fn direct_paths(
        cache_dir: &Path,
        progress: &mut dyn FnMut(String),
    ) -> Result<ModelPaths> {
        let manual = cache_dir.join("manual-siglip");
        let endpoint = crate::download::hf_endpoint();
        // (remote repo path, local name, required)
        let files = [
            (TEXT_ONNX, "text_model.onnx", true),
            (VISION_ONNX, "vision_model.onnx", true),
            ("tokenizer.json", "tokenizer.json", true),
            ("preprocessor_config.json", "preprocessor_config.json", true),
            ("tokenizer_config.json", "tokenizer_config.json", false),
        ];
        for (remote, local, required) in files {
            let url = crate::download::file_url(&endpoint, REPO, remote);
            let res = crate::download::fetch_file(&url, &manual.join(local), &mut |done, total| {
                progress(match total {
                    Some(t) if t > 0 => format!("downloading {local} {}%", done * 100 / t),
                    _ => format!("downloading {local}"),
                });
            });
            if required {
                res?;
            }
        }
        let tok_cfg = manual.join("tokenizer_config.json");
        Ok(ModelPaths {
            text: manual.join("text_model.onnx"),
            vision: manual.join("vision_model.onnx"),
            tokenizer: manual.join("tokenizer.json"),
            preprocessor: manual.join("preprocessor_config.json"),
            tokenizer_config: tok_cfg.is_file().then_some(tok_cfg),
        })
    }

    fn from_paths(paths: ModelPaths) -> Result<Self> {
        let tokenizer = Tokenizer::from_file(&paths.tokenizer)
            .map_err(|e| anyhow!("load tokenizer: {e}"))?;
        let pad_token = paths
            .tokenizer_config
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| pad_token_name(&v))
            .unwrap_or_else(|| "</s>".to_string());
        let pad_id = tokenizer
            .token_to_id(&pad_token)
            .ok_or_else(|| anyhow!("pad token {pad_token:?} not in vocab"))?
            as i64;

        let pre: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&paths.preprocessor)?)?;
        let img_size = pre["size"]["height"]
            .as_u64()
            .or_else(|| pre["size"]["shortest_edge"].as_u64())
            .unwrap_or(224) as u32;
        let mean = rgb_triple(&pre["image_mean"], 0.5);
        let std = rgb_triple(&pre["image_std"], 0.5);
        let resample = match pre["resample"].as_u64().unwrap_or(3) {
            0 => FilterType::Nearest,
            2 => FilterType::Triangle,   // bilinear
            _ => FilterType::CatmullRom, // bicubic
        };

        let text = Session::builder()?.commit_from_file(&paths.text)?;
        let vision = Session::builder()?.commit_from_file(&paths.vision)?;
        Ok(Self {
            text,
            vision,
            tokenizer,
            pad_id,
            img_size,
            mean,
            std,
            resample,
        })
    }

    /// Embed a text query into the shared text-image space. L2-normalized.
    pub fn embed_query(&mut self, query: &str) -> Result<Vec<f32>> {
        let enc = self
            .tokenizer
            .encode(query, true)
            .map_err(|e| anyhow!("tokenize: {e}"))?;
        let mut ids: Vec<i64> = enc.get_ids().iter().map(|&i| i as i64).collect();
        ids.truncate(TEXT_SEQ_LEN);
        while ids.len() < TEXT_SEQ_LEN {
            ids.push(self.pad_id);
        }
        let ids = Array2::from_shape_vec((1, TEXT_SEQ_LEN), ids)?;

        let input_names: Vec<String> =
            self.text.inputs().iter().map(|i| i.name().to_string()).collect();
        let ids_name = input_names
            .iter()
            .find(|n| n.contains("input_ids"))
            .cloned()
            .unwrap_or_else(|| input_names[0].clone());
        let wants_mask = input_names.iter().any(|n| n.contains("attention_mask"));

        let outputs = if wants_mask {
            // padding tokens are attended to by design in SigLIP
            let mask = Array2::<i64>::ones((1, TEXT_SEQ_LEN));
            self.text.run(ort::inputs![
                ids_name => Value::from_array(ids)?,
                "attention_mask" => Value::from_array(mask)?,
            ])?
        } else {
            self.text.run(ort::inputs![ids_name => Value::from_array(ids)?])?
        };
        let mut rows = pooled_rows(&outputs)?;
        let mut v = rows
            .pop()
            .ok_or_else(|| anyhow!("empty text output"))?;
        crate::embed::normalize(&mut v);
        Ok(v)
    }

    /// Embed one image file. L2-normalized. Errors on unreadable/corrupt files.
    pub fn embed_image(&mut self, path: &Path) -> Result<Vec<f32>> {
        let img = image::open(path)?;
        self.embed_dynamic(img)
    }

    /// Embed an in-memory image (e.g. pasted from the clipboard).
    pub fn embed_image_bytes(&mut self, bytes: &[u8]) -> Result<Vec<f32>> {
        let img = image::load_from_memory(bytes)?;
        self.embed_dynamic(img)
    }

    /// Embed an already-decoded image (lets callers reuse one decode for
    /// both thumbnailing and embedding).
    pub fn embed_dynamic(&mut self, img: image::DynamicImage) -> Result<Vec<f32>> {
        let s = self.img_size;
        let img = img
            .resize_exact(s, s, self.resample) // SigLIP squashes to square, no crop
            .to_rgb8();
        let mut pixels = Array4::<f32>::zeros((1, 3, s as usize, s as usize));
        for (x, y, p) in img.enumerate_pixels() {
            for c in 0..3 {
                pixels[[0, c, y as usize, x as usize]] =
                    (p.0[c] as f32 / 255.0 - self.mean[c]) / self.std[c];
            }
        }
        let input_name = self.vision.inputs()[0].name().to_string();
        let outputs = self
            .vision
            .run(ort::inputs![input_name => Value::from_array(pixels)?])?;
        let mut rows = pooled_rows(&outputs)?;
        let mut v = rows
            .pop()
            .ok_or_else(|| anyhow!("empty vision output"))?;
        crate::embed::normalize(&mut v);
        Ok(v)
    }
}

/// Extract the pooled [batch, dim] tensor by preferred key, as row vectors.
fn pooled_rows(outputs: &ort::session::SessionOutputs) -> Result<Vec<Vec<f32>>> {
    for &key in POOLED_KEYS {
        let Some(value) = outputs.get(key) else { continue };
        let Ok((shape, data)) = value.try_extract_tensor::<f32>() else {
            continue;
        };
        let dims: Vec<usize> = shape.iter().map(|&d| d as usize).collect();
        if dims.len() == 2 {
            let (batch, dim) = (dims[0], dims[1]);
            return Ok((0..batch)
                .map(|b| data[b * dim..(b + 1) * dim].to_vec())
                .collect());
        }
    }
    let available: Vec<String> = outputs.keys().map(|k| k.to_string()).collect();
    bail!("no pooled 2D output among ONNX outputs {available:?} (expected one of {POOLED_KEYS:?})")
}

fn pad_token_name(config: &serde_json::Value) -> Option<String> {
    match &config["pad_token"] {
        serde_json::Value::String(s) => Some(s.clone()),
        obj @ serde_json::Value::Object(_) => {
            obj["content"].as_str().map(str::to_string)
        }
        _ => None,
    }
}

fn rgb_triple(v: &serde_json::Value, default: f32) -> [f32; 3] {
    let get = |i: usize| v[i].as_f64().map(|f| f as f32).unwrap_or(default);
    [get(0), get(1), get(2)]
}
