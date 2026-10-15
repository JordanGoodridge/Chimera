//! Comparing two pictures of the same view (a reference and ours):
//! per-image brightness, saturation and sharpness (`Stats`), the mean
//! absolute difference, how well fine detail lines up
//! (`detail_correlation`) and a side-by-side sheet (reference | ours |
//! difference x4). Game-neutral: `refcmp` (CS:S's reference driver,
//! games/cs_source/tools) captures the views and calls `compare`.

use image::{Rgb, RgbImage, imageops};

/// One image's look, averaged over its pixels.
#[derive(Default, serde::Serialize)]
pub struct Stats {
    /// Mean sRGB value per channel, 0..1.
    pub rgb: [f32; 3],
    pub luma: f32,
    /// Mean HSV saturation.
    pub saturation: f32,
    /// Mean absolute Laplacian of luma (0..255 scale): higher = crisper.
    pub sharpness: f32,
}

pub fn stats(img: &RgbImage) -> Stats {
    let (w, h) = img.dimensions();
    let n = (w * h) as f32;
    let mut s = Stats::default();
    let luma: Vec<f32> = img
        .pixels()
        .map(|Rgb([r, g, b])| {
            let (r, g, b) = (*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0);
            s.rgb[0] += r / n;
            s.rgb[1] += g / n;
            s.rgb[2] += b / n;
            let (max, min) = (r.max(g).max(b), r.min(g).min(b));
            s.saturation += if max > 0.0 { (max - min) / max } else { 0.0 } / n;
            0.2126 * r + 0.7152 * g + 0.0722 * b
        })
        .collect();
    s.luma = luma.iter().sum::<f32>() / n;
    let at = |x: u32, y: u32| luma[(y * w + x) as usize] * 255.0;
    let mut lap = 0.0;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            lap += (4.0 * at(x, y) - at(x - 1, y) - at(x + 1, y) - at(x, y - 1) - at(x, y + 1)).abs();
        }
    }
    s.sharpness = lap / ((w - 2) * (h - 2)) as f32;
    s
}

/// Correlation of the two images' fine detail (Laplacian of luma): near 1
/// when texture relief lines up, near 0 when unrelated, negative when
/// inverted (e.g. normal maps lit from the wrong side).
pub fn detail_correlation(a: &RgbImage, b: &RgbImage) -> f32 {
    // Half resolution first: tolerant of sub-pixel differences between
    // renderers, still sensitive to texture relief.
    let half =
        |img: &RgbImage| imageops::resize(img, img.width() / 2, img.height() / 2, imageops::FilterType::Triangle);
    let (a, b) = (&half(a), &half(b));
    let lap = |img: &RgbImage| {
        let (w, h) = img.dimensions();
        let l = |x: u32, y: u32| {
            let p = img.get_pixel(x, y);
            0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32
        };
        let mut out = Vec::with_capacity((w * h) as usize);
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                out.push(4.0 * l(x, y) - l(x - 1, y) - l(x + 1, y) - l(x, y - 1) - l(x, y + 1));
            }
        }
        out
    };
    let (la, lb) = (lap(a), lap(b));
    let n = la.len() as f32;
    let (ma, mb) = (la.iter().sum::<f32>() / n, lb.iter().sum::<f32>() / n);
    let (mut sab, mut saa, mut sbb) = (0.0f64, 0.0f64, 0.0f64);
    for (x, y) in la.iter().zip(&lb) {
        let (dx, dy) = ((x - ma) as f64, (y - mb) as f64);
        sab += dx * dy;
        saa += dx * dx;
        sbb += dy * dy;
    }
    (sab / (saa.sqrt() * sbb.sqrt()).max(1e-9)) as f32
}

/// A reference and ours compared (`compare`).
pub struct Comparison {
    pub reference: Stats,
    pub ours: Stats,
    /// Mean absolute difference per channel, 0..1.
    pub abs_diff: f32,
    pub detail: f32,
    /// Reference | ours | difference x4, at half size.
    pub sheet: RgbImage,
}

/// Compare `ours` with `reference` (ours resized to the reference's size
/// first when they differ).
pub fn compare(reference: &image::DynamicImage, ours: &image::DynamicImage) -> Comparison {
    let o = if ours.width() != reference.width() || ours.height() != reference.height() {
        ours.resize_exact(reference.width(), reference.height(), imageops::FilterType::Triangle)
    } else {
        ours.clone()
    };
    let (r, o) = (reference.to_rgb8(), o.to_rgb8());
    let mut diff = RgbImage::new(r.width(), r.height());
    let mut total = 0.0;
    for (d, (a, b)) in diff.pixels_mut().zip(r.pixels().zip(o.pixels())) {
        let px: [u8; 3] = std::array::from_fn(|c| (a[c] as i32 - b[c] as i32).unsigned_abs() as u8);
        total += px.iter().map(|v| *v as f32).sum::<f32>() / (3.0 * 255.0);
        *d = Rgb(px.map(|v| v.saturating_mul(4)));
    }
    let abs_diff = total / (r.width() * r.height()) as f32;
    let detail = detail_correlation(&r, &o);
    let (hw, hh) = (r.width() / 2, r.height() / 2);
    let mut sheet = RgbImage::new(hw * 3, hh);
    for (i, img) in [&r, &o, &diff].into_iter().enumerate() {
        let small = imageops::resize(img, hw, hh, imageops::FilterType::Triangle);
        imageops::replace(&mut sheet, &small, (i as u32 * hw) as i64, 0);
    }
    Comparison {
        reference: stats(&r),
        ours: stats(&o),
        abs_diff,
        detail,
        sheet,
    }
}

pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker(w: u32, h: u32, shift: u32) -> image::DynamicImage {
        image::DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
            if ((x + shift) / 4 + y / 4) % 2 == 0 {
                Rgb([200, 120, 40])
            } else {
                Rgb([20, 30, 60])
            }
        }))
    }

    #[test]
    fn same_picture_matches() {
        let a = checker(64, 48, 0);
        let c = compare(&a, &a);
        assert_eq!(c.abs_diff, 0.0);
        assert!(c.detail > 0.99, "detail {}", c.detail);
        assert_eq!(c.reference.luma, c.ours.luma);
        assert_eq!(c.sheet.dimensions(), (32 * 3, 24));
    }

    #[test]
    fn different_size_and_shift_differ() {
        let c = compare(&checker(64, 48, 0), &checker(128, 96, 4));
        assert!(c.abs_diff > 0.1, "abs diff {}", c.abs_diff);
        assert!(c.detail < 0.5, "detail {}", c.detail);
    }

    #[test]
    fn srgb_round_trips() {
        for v in [0.0, 0.02, 0.5, 1.0] {
            assert!((linear_to_srgb(srgb_to_linear(v)) - v).abs() < 1e-5);
        }
    }
}
