//! Logo mode: the logo image drawn centred over the style's background, never enlarged
//! (OpenSong `PresentWindow:2599-2618`).

use std::sync::{Arc, Mutex};

use serviceflow_core::output::Logo;

use crate::draw::Image;

/// A decoded logo: straight RGBA8.
pub(crate) struct Decoded {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// Caches the last decoded logo, so redrawing it for another size does not decode it again.
#[derive(Default)]
pub(crate) struct LogoCache(Mutex<Option<Cached>>);

/// The logo text a decode was made from, and its result (`None`: undecodable).
type Cached = (Arc<str>, Option<Arc<Decoded>>);

impl LogoCache {
    pub(crate) fn get(&self, logo: &Logo) -> Option<Arc<Decoded>> {
        let mut cache = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((key, decoded)) = &*cache
            && Arc::ptr_eq(key, &logo.image)
        {
            return decoded.clone();
        }
        let decoded = decode(&logo.image).map(Arc::new);
        *cache = Some((Arc::clone(&logo.image), decoded.clone()));
        decoded
    }
}

fn decode(base64: &str) -> Option<Decoded> {
    let bytes = base64_decode(base64)?;
    let image = image::load_from_memory(&bytes).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    (width > 0 && height > 0).then(|| Decoded { width, height, rgba: image.into_raw() })
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            c if c.is_ascii_whitespace() => continue,
            _ => return None,
        };
        acc = acc << 6 | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Draw `logo` centred over `frame` (a premultiplied, opaque background), shrinking it to fit by
/// averaging the source pixels under each target pixel.
pub(crate) fn composite(frame: &mut Image, logo: &Decoded) {
    let (fw, fh) = (frame.width as f64, frame.height as f64);
    let scale = (fw / f64::from(logo.width)).min(fh / f64::from(logo.height)).min(1.0);
    let (tw, th) = (((f64::from(logo.width) * scale).round() as u32).max(1), ((f64::from(logo.height) * scale).round() as u32).max(1));
    let (x0, y0) = ((frame.width.saturating_sub(tw)) / 2, (frame.height.saturating_sub(th)) / 2);
    for ty in 0..th.min(frame.height) {
        let (sy0, sy1) = span(ty, th, logo.height);
        for tx in 0..tw.min(frame.width) {
            let (sx0, sx1) = span(tx, tw, logo.width);
            // Premultiplied sums of the source pixels in the footprint.
            let mut sum = [0u64; 4];
            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let i = ((sy * logo.width + sx) * 4) as usize;
                    let a = u64::from(logo.rgba[i + 3]);
                    for (c, total) in sum.iter_mut().take(3).enumerate() {
                        *total += u64::from(logo.rgba[i + c]) * a / 255;
                    }
                    sum[3] += a;
                }
            }
            let n = u64::from((sy1 - sy0) * (sx1 - sx0));
            let (src, alpha) = ([sum[0] / n, sum[1] / n, sum[2] / n], sum[3] / n);
            let i = (((y0 + ty) * frame.width + x0 + tx) * 4) as usize;
            for (c, value) in src.iter().enumerate() {
                let under = u64::from(frame.pixels[i + c]);
                frame.pixels[i + c] = (value + under * (255 - alpha) / 255).min(255) as u8;
            }
            let under_a = u64::from(frame.pixels[i + 3]);
            frame.pixels[i + 3] = (alpha + under_a * (255 - alpha) / 255).min(255) as u8;
        }
    }
}

/// The source pixels `[start, end)` along one axis that target pixel `t` of `target` covers.
fn span(t: u32, target: u32, source: u32) -> (u32, u32) {
    let start = (u64::from(t) * u64::from(source) / u64::from(target)) as u32;
    let end = ((u64::from(t) + 1) * u64::from(source) / u64::from(target)) as u32;
    (start.min(source - 1), end.max(start + 1).min(source))
}
