//! PP-OCRv4 text extraction (detection + recognition) run directly on ort.
//!
//! Two small ONNX models (det ~4.5MB DBNet, rec ~10MB CRNN) from the RapidOCR
//! distribution; the recognition charset ships inside the rec model's ONNX
//! metadata ("character" key), so there is no separate dictionary file. The
//! detector's polygon post-processing is simplified to connected components +
//! expanded bounding rects — right for screenshots and documents, which is
//! what a local file index sees.

use anyhow::{anyhow, bail, Result};
use image::{DynamicImage, GenericImageView};
use ndarray::Array4;
use ort::session::Session;
use ort::value::Value;
use std::path::Path;

/// Default OCR model (the settings dropdown stores one of [`OCR_MODELS`]).
pub const OCR_MODEL_ID: &str = "pp-ocr-v4";

/// The selectable engines: (id, human label with download size). Both are
/// PaddleOCR-family DBNet + CTC models, so one inference path serves both.
pub const OCR_MODELS: &[(&str, &str)] = &[
    ("pp-ocr-v4", "PP-OCRv4 (15 MB)"),
    ("pp-ocr-v6-small", "PP-OCRv6 small (30 MB)"),
];

const RAPIDOCR_REPO: &str = "SWHL/RapidOCR";
const OAR_BASE: &str = "https://github.com/GreatV/oar-ocr/releases/download/v0.7.0";

/// Everything model-specific: where each file comes from (sources tried in
/// order — the last is always magpie's own `models-1` mirror) and how the
/// charset is obtained.
struct ModelSpec {
    /// Cache subdirectory, one per model so switching never mixes files.
    dir: &'static str,
    /// (primary url template kind, local file name)
    det: FileSource,
    rec: FileSource,
    /// v4 embeds the charset in the rec model's ONNX metadata; v6 ships a
    /// dictionary file.
    dict: Option<FileSource>,
}

struct FileSource {
    /// Primary download location. `HfPath` goes through the user-selected
    /// endpoint (huggingface.co or the mirror); `Url` is absolute.
    primary: Primary,
    local: &'static str,
}

enum Primary {
    HfPath(&'static str),
    Url(&'static str),
}

fn model_spec(model_id: &str) -> Result<ModelSpec> {
    match model_id {
        "pp-ocr-v4" => Ok(ModelSpec {
            dir: "manual-ocr",
            det: FileSource {
                primary: Primary::HfPath("PP-OCRv4/ch_PP-OCRv4_det_infer.onnx"),
                local: "ocr-det.onnx",
            },
            rec: FileSource {
                primary: Primary::HfPath("PP-OCRv4/ch_PP-OCRv4_rec_infer.onnx"),
                local: "ocr-rec.onnx",
            },
            dict: None,
        }),
        "pp-ocr-v6-small" => Ok(ModelSpec {
            dir: "manual-ocr-v6",
            det: FileSource {
                primary: Primary::Url("pp-ocrv6_small_det.onnx"),
                local: "ocr-v6-det.onnx",
            },
            rec: FileSource {
                primary: Primary::Url("pp-ocrv6_small_rec.onnx"),
                local: "ocr-v6-rec.onnx",
            },
            dict: Some(FileSource {
                primary: Primary::Url("ppocrv6_dict.txt"),
                local: "ocr-v6-dict.txt",
            }),
        }),
        other => anyhow::bail!("unknown OCR model {other:?}"),
    }
}

/// True when `model_id` names a selectable OCR model.
pub fn is_known_model(model_id: &str) -> bool {
    OCR_MODELS.iter().any(|(id, _)| *id == model_id)
}

/// Detection input is capped at this edge (multiple of 32), like RapidOCR.
const DET_MAX_EDGE: u32 = 960;
const DET_BIN_THRESH: f32 = 0.3;
const DET_BOX_THRESH: f32 = 0.5;
const DET_UNCLIP_RATIO: f32 = 1.6;
/// Recognition line geometry (fixed height, width follows aspect, capped).
const REC_H: u32 = 48;
const REC_MAX_W: u32 = 320;
const REC_MIN_CONF: f32 = 0.5;

pub struct Ocr {
    det: Session,
    rec: Session,
    charset: Vec<String>,
}

impl Ocr {
    /// [`Self::new_with_model`] with the default model.
    pub fn new(cache_dir: &Path, progress: &mut dyn FnMut(String)) -> Result<Self> {
        Self::new_with_model(cache_dir, OCR_MODEL_ID, progress)
    }

    /// Download (or reuse) the selected model's files and load both
    /// sessions. Blocking; call off the UI thread. Sources per file: the
    /// primary location (user-selected HF endpoint for v4, the oar-ocr
    /// release for v6), then magpie's own `models-1` release assets.
    pub fn new_with_model(
        cache_dir: &Path,
        model_id: &str,
        progress: &mut dyn FnMut(String),
    ) -> Result<Self> {
        let spec = model_spec(model_id)?;
        let manual = cache_dir.join(spec.dir);
        let endpoint = crate::download::hf_endpoint();
        let fetch = |src: &FileSource, progress: &mut dyn FnMut(String)| -> Result<()> {
            let primary = match src.primary {
                Primary::HfPath(p) => crate::download::file_url(&endpoint, RAPIDOCR_REPO, p),
                Primary::Url(name) => format!("{OAR_BASE}/{name}"),
            };
            let urls = [primary, format!("{}/{}", crate::download::MODELS_BASE, src.local)];
            let local = src.local;
            crate::download::fetch_file_any(&urls, &manual.join(local), &mut |done, total| {
                progress(match total {
                    Some(t) if t > 0 => format!("downloading {local} {}%", done * 100 / t),
                    _ => format!("downloading {local}"),
                });
            })
        };
        fetch(&spec.det, progress)?;
        fetch(&spec.rec, progress)?;
        if let Some(dict) = &spec.dict {
            fetch(dict, progress)?;
        }
        progress("loading".into());
        let det = Session::builder()?.commit_from_file(manual.join(spec.det.local))?;
        let rec = Session::builder()?.commit_from_file(manual.join(spec.rec.local))?;
        let charset = match &spec.dict {
            Some(dict) => file_charset(&manual.join(dict.local))?,
            None => rec_charset(&rec)?,
        };
        Ok(Self { det, rec, charset })
    }

    /// [`Self::extract_text`] for an image file on disk.
    pub fn extract_text_from_path(&mut self, path: &Path) -> Result<String> {
        let img = image::open(path)?;
        self.extract_text(&img)
    }

    /// Extract the readable text of an image, lines joined with newlines,
    /// reading order top-to-bottom then left-to-right. Empty string when the
    /// image contains no confident text.
    pub fn extract_text(&mut self, img: &DynamicImage) -> Result<String> {
        let boxes = self.detect(img)?;
        let mut lines: Vec<String> = Vec::new();
        for b in boxes {
            let crop = img.crop_imm(b.x0, b.y0, b.x1 - b.x0, b.y1 - b.y0);
            if let Some(text) = self.recognize(&crop)? {
                if !text.trim().is_empty() {
                    lines.push(text);
                }
            }
        }
        Ok(lines.join("\n"))
    }

    /// DBNet forward + simplified post-processing. Returns boxes in original
    /// image coordinates, sorted into reading order.
    fn detect(&mut self, img: &DynamicImage) -> Result<Vec<Box2>> {
        let (ow, oh) = img.dimensions();
        if ow < 8 || oh < 8 {
            return Ok(Vec::new());
        }
        // resize so the long edge fits DET_MAX_EDGE, snapped to 32-multiples
        let scale = (DET_MAX_EDGE as f32 / ow.max(oh) as f32).min(1.0);
        let rw = (((ow as f32 * scale) / 32.0).round().max(1.0) as u32) * 32;
        let rh = (((oh as f32 * scale) / 32.0).round().max(1.0) as u32) * 32;
        let resized = img.resize_exact(rw, rh, image::imageops::FilterType::Triangle);
        let rgb = resized.to_rgb8();
        let mean = [0.485f32, 0.456, 0.406];
        let std = [0.229f32, 0.224, 0.225];
        let mut input = Array4::<f32>::zeros((1, 3, rh as usize, rw as usize));
        for (x, y, p) in rgb.enumerate_pixels() {
            for c in 0..3 {
                input[[0, c, y as usize, x as usize]] =
                    (p.0[c] as f32 / 255.0 - mean[c]) / std[c];
            }
        }
        let input_name = self.det.inputs()[0].name().to_string();
        let outputs = self
            .det
            .run(ort::inputs![input_name => Value::from_array(input)?])?;
        let (shape, probs) = outputs[0].try_extract_tensor::<f32>()?;
        let (mh, mw) = (shape[2] as usize, shape[3] as usize);
        // connected components over the binarized probability map
        let mut label = vec![0u32; mh * mw];
        let mut comps: Vec<Comp> = Vec::new();
        let mut stack: Vec<(usize, usize)> = Vec::new();
        for sy in 0..mh {
            for sx in 0..mw {
                let i = sy * mw + sx;
                if label[i] != 0 || probs[i] < DET_BIN_THRESH {
                    continue;
                }
                let id = comps.len() as u32 + 1;
                let mut c = Comp::new(sx, sy);
                label[i] = id;
                stack.push((sx, sy));
                while let Some((x, y)) = stack.pop() {
                    c.absorb(x, y, probs[y * mw + x]);
                    let mut visit = |nx: usize, ny: usize| {
                        let j = ny * mw + nx;
                        if label[j] == 0 && probs[j] >= DET_BIN_THRESH {
                            label[j] = id;
                            stack.push((nx, ny));
                        }
                    };
                    if x > 0 {
                        visit(x - 1, y);
                    }
                    if x + 1 < mw {
                        visit(x + 1, y);
                    }
                    if y > 0 {
                        visit(x, y - 1);
                    }
                    if y + 1 < mh {
                        visit(x, y + 1);
                    }
                }
                comps.push(c);
            }
        }
        // score + unclip each component, map back to original coordinates
        let sx = ow as f32 / mw as f32;
        let sy = oh as f32 / mh as f32;
        let mut boxes: Vec<Box2> = Vec::new();
        for c in comps {
            let (w, h) = (c.x1 - c.x0 + 1, c.y1 - c.y0 + 1);
            if w.min(h) < 3 || c.score() < DET_BOX_THRESH {
                continue;
            }
            // DB predicts shrunk kernels; grow the rect back (area/perimeter
            // heuristic — the rectangular cousin of the official unclip)
            let expand = (w * h) as f32 * DET_UNCLIP_RATIO / (2.0 * (w + h) as f32);
            let x0 = ((c.x0 as f32 - expand) * sx).max(0.0) as u32;
            let y0 = ((c.y0 as f32 - expand) * sy).max(0.0) as u32;
            let x1 = (((c.x1 + 1) as f32 + expand) * sx).min(ow as f32) as u32;
            let y1 = (((c.y1 + 1) as f32 + expand) * sy).min(oh as f32) as u32;
            if x1 > x0 && y1 > y0 {
                boxes.push(Box2 { x0, y0, x1, y1 });
            }
        }
        // reading order: coarse rows first, then left-to-right
        boxes.sort_by_key(|b| ((b.y0 / 16), b.x0));
        Ok(boxes)
    }

    /// CRNN forward on one detected line; CTC greedy decode. None when the
    /// mean per-character confidence is too low to trust.
    fn recognize(&mut self, line: &DynamicImage) -> Result<Option<String>> {
        let (lw, lh) = line.dimensions();
        if lw < 4 || lh < 4 {
            return Ok(None);
        }
        let w = ((lw * REC_H) as f32 / lh as f32).round().clamp(8.0, REC_MAX_W as f32) as u32;
        let rgb = line
            .resize_exact(w, REC_H, image::imageops::FilterType::Triangle)
            .to_rgb8();
        let mut input = Array4::<f32>::zeros((1, 3, REC_H as usize, w as usize));
        for (x, y, p) in rgb.enumerate_pixels() {
            for c in 0..3 {
                input[[0, c, y as usize, x as usize]] = (p.0[c] as f32 / 255.0 - 0.5) / 0.5;
            }
        }
        let input_name = self.rec.inputs()[0].name().to_string();
        let outputs = self
            .rec
            .run(ort::inputs![input_name => Value::from_array(input)?])?;
        let (shape, logits) = outputs[0].try_extract_tensor::<f32>()?;
        let (steps, classes) = (shape[1] as usize, shape[2] as usize);
        // greedy CTC first (while `outputs` still borrows the session), then
        // map class indices to characters once the borrow is released
        let mut picks: Vec<(usize, f32)> = Vec::new();
        let mut prev = 0usize;
        for t in 0..steps {
            let row = &logits[t * classes..(t + 1) * classes];
            let (best, conf) = row
                .iter()
                .enumerate()
                .fold((0usize, f32::MIN), |acc, (i, &v)| if v > acc.1 { (i, v) } else { acc });
            if best != 0 && best != prev {
                picks.push((best, conf));
            }
            prev = best;
        }
        drop(outputs);
        let mut out = String::new();
        let mut confs: Vec<f32> = Vec::new();
        for (idx, conf) in picks {
            if let Some(ch) = self.class_char(idx, classes) {
                out.push_str(ch);
                confs.push(conf);
            }
        }
        if confs.is_empty() {
            return Ok(None);
        }
        let mean = confs.iter().sum::<f32>() / confs.len() as f32;
        Ok((mean >= REC_MIN_CONF).then_some(out))
    }

    /// Class index -> charset entry: 0 is CTC blank, 1..=N the metadata
    /// characters, and the one trailing extra class (if present) is space.
    fn class_char(&self, idx: usize, classes: usize) -> Option<&str> {
        if idx == 0 {
            return None;
        }
        if let Some(c) = self.charset.get(idx - 1) {
            return Some(c);
        }
        (idx == classes - 1 && classes > self.charset.len() + 1).then_some(" ")
    }
}

/// Charset from a PaddleOCR dictionary file: one entry per line (the v6
/// distribution ships it separately instead of embedding it).
fn file_charset(path: &Path) -> Result<Vec<String>> {
    let raw = std::fs::read_to_string(path)?;
    let charset: Vec<String> = raw.lines().map(str::to_string).collect();
    if charset.len() < 100 {
        bail!("dictionary suspiciously small ({} entries)", charset.len());
    }
    Ok(charset)
}

/// The recognition charset lives in the rec model's ONNX metadata under
/// "character", one entry per line — the RapidOCR convention.
fn rec_charset(rec: &Session) -> Result<Vec<String>> {
    let meta = rec.metadata()?;
    let raw = meta
        .custom("character")
        .ok_or_else(|| anyhow!("rec model has no embedded charset"))?;
    let charset: Vec<String> = raw.lines().map(str::to_string).collect();
    if charset.len() < 100 {
        bail!("embedded charset suspiciously small ({} entries)", charset.len());
    }
    Ok(charset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_model_has_a_resolvable_spec() {
        for (id, label) in OCR_MODELS {
            assert!(is_known_model(id));
            let spec = model_spec(id).expect("listed model must resolve");
            // release-asset names must be flat (no path separators)
            for src in [Some(&spec.det), Some(&spec.rec), spec.dict.as_ref()].into_iter().flatten()
            {
                assert!(!src.local.contains('/'), "{id}: {}", src.local);
            }
            assert!(label.contains("MB"), "label should show the download size");
        }
        assert!(is_known_model(OCR_MODEL_ID), "default must be listed");
        assert!(model_spec("nope").is_err());
        assert!(!is_known_model("nope"));
    }

    #[test]
    fn model_cache_dirs_are_distinct() {
        // switching models must never mix files in one cache directory
        let dirs: Vec<&str> = OCR_MODELS
            .iter()
            .map(|(id, _)| model_spec(id).unwrap().dir)
            .collect();
        let unique: std::collections::HashSet<&&str> = dirs.iter().collect();
        assert_eq!(unique.len(), dirs.len());
    }
}

#[derive(Debug)]
struct Box2 {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

struct Comp {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    count: u32,
    sum: f32,
}

impl Comp {
    fn new(x: usize, y: usize) -> Self {
        Self { x0: x, y0: y, x1: x, y1: y, count: 0, sum: 0.0 }
    }
    fn absorb(&mut self, x: usize, y: usize, p: f32) {
        self.x0 = self.x0.min(x);
        self.y0 = self.y0.min(y);
        self.x1 = self.x1.max(x);
        self.y1 = self.y1.max(y);
        self.count += 1;
        self.sum += p;
    }
    fn score(&self) -> f32 {
        if self.count == 0 {
            0.0
        } else {
            self.sum / self.count as f32
        }
    }
}
