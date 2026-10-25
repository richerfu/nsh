//! Raster admission keeps long paragraphs visible at a reduced resolution.
const MAX_PIXELS: f64 = 12_000_000.0;
const MAX_SIDE: f64 = 32_768.0;

pub(crate) fn dimensions(width: f32, height: f32, scale: f32) -> (u32, u32, f32) {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return (0, 0, scale);
    }
    let (w, h) = (f64::from(width), f64::from(height));
    let scale = f64::from(scale)
        .min((MAX_PIXELS / (w * h)).sqrt())
        .min(MAX_SIDE / w.max(h));
    // Floor the capped dimensions so rounding cannot exceed the allocation budget.
    (
        (w * scale).floor().max(1.0) as u32,
        (h * scale).floor().max(1.0) as u32,
        scale as f32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn long_articles_keep_their_entire_extent() {
        let (w, h, scale) = dimensions(390.0, 10_000.0, 3.0);
        assert!(w > 0 && h > 0);
        assert!(u64::from(w) * u64::from(h) > 4_194_304);
        assert!(u64::from(w) * u64::from(h) <= MAX_PIXELS as u64);
        assert!((h as f32 / scale - 10_000.0).abs() < 1.0);
    }
    #[test]
    fn hostile_dimensions_cannot_request_unbounded_bitmaps() {
        for (w, h) in [(f32::MAX, f32::MAX), (1.0, 1e20), (1e20, 1.0)] {
            let (w, h, _) = dimensions(w, h, 3.0);
            assert!(u64::from(w) * u64::from(h) <= MAX_PIXELS as u64);
            assert!(w <= MAX_SIDE as u32 && h <= MAX_SIDE as u32);
        }
        assert_eq!(dimensions(f32::NAN, 10.0, 1.0).0, 0);
    }
}
