//! Red update dot painted onto the tray icon, in place.
//!
//! Pure pixel math on an RGBA buffer — the shell hands us the decoded window
//! icon, we return nothing. Kept in core so it's unit-testable without a
//! running Tauri app.

/// Paint a red notification dot (white-ringed, antialiased) in the top-right
/// corner of an RGBA image. `rgba` must be exactly `w * h * 4` bytes; the
/// function is a no-op on mismatched or degenerate sizes.
pub fn overlay_badge(rgba: &mut [u8], w: u32, h: u32) {
    if w < 8 || h < 8 || rgba.len() != (w as usize) * (h as usize) * 4 {
        return;
    }
    let wf = w as f32;
    // dot geometry: radius ~22% of the icon edge, nudged in from the corner
    // so scaled-down tray renders don't clip it
    let r = wf * 0.22;
    let ring = (wf * 0.03).max(1.0); // white outline so the dot reads on any tray shade
    let margin = wf * 0.02;
    let cx = wf - r - ring - margin;
    let cy = r + ring + margin;

    let y0 = ((cy - r - ring - 1.0).floor().max(0.0)) as u32;
    let y1 = ((cy + r + ring + 1.0).ceil() as u32).min(h);
    let x0 = ((cx - r - ring - 1.0).floor().max(0.0)) as u32;
    let x1 = ((cx + r + ring + 1.0).ceil() as u32).min(w);

    const RED: [f32; 3] = [244.0, 63.0, 54.0];
    const WHITE: [f32; 3] = [255.0, 255.0, 255.0];

    for y in y0..y1 {
        for x in x0..x1 {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            // coverage of the full disc (red + ring), feathered over 1px
            let cover = (r + ring - d + 0.5).clamp(0.0, 1.0);
            if cover <= 0.0 {
                continue;
            }
            // how red vs white this pixel is (inner disc vs outline)
            let redness = (r - d + 0.5).clamp(0.0, 1.0);
            let i = ((y * w + x) * 4) as usize;
            for c in 0..3 {
                let dot = WHITE[c] + (RED[c] - WHITE[c]) * redness;
                let base = rgba[i + c] as f32;
                rgba[i + c] = (base + (dot - base) * cover).round() as u8;
            }
            let a = rgba[i + 3] as f32;
            rgba[i + 3] = (a + (255.0 - a) * cover).round() as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badge_paints_red_dot_deterministically() {
        let (w, h) = (64u32, 64u32);
        let mut img = vec![0u8; (w * h * 4) as usize];
        let mut again = img.clone();
        overlay_badge(&mut img, w, h);
        overlay_badge(&mut again, w, h);
        assert_eq!(img, again, "same input must give byte-identical output");

        // dot center is solid red and opaque
        let wf = w as f32;
        let r = wf * 0.22;
        let ring = (wf * 0.03).max(1.0);
        let cx = (wf - r - ring - wf * 0.02) as u32;
        let cy = (r + ring + wf * 0.02) as u32;
        let i = ((cy * w + cx) * 4) as usize;
        assert_eq!(&img[i..i + 4], &[244, 63, 54, 255]);

        // far corners untouched
        let bl = (((h - 1) * w) * 4) as usize;
        assert_eq!(&img[bl..bl + 4], &[0, 0, 0, 0]);
        assert_eq!(&img[0..4], &[0, 0, 0, 0]);
    }

    #[test]
    fn badge_ignores_bad_buffers() {
        let mut short = vec![0u8; 16];
        overlay_badge(&mut short, 64, 64); // wrong len: no-op, no panic
        assert!(short.iter().all(|&b| b == 0));
        let mut tiny = vec![0u8; 4 * 4 * 4];
        overlay_badge(&mut tiny, 4, 4); // degenerate size: no-op
        assert!(tiny.iter().all(|&b| b == 0));
    }
}
