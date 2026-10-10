//! The treemap view's sizing and coordinate conversions, kept apart from the widget so
//! they are unit-tested on their own.
//!
//! The raster is the widget's size in physical pixels (logical size × DPI scale), but never
//! more than [`MAX_RASTER_PIXELS`] in all: a huge window (or a bogus scale) is rasterised
//! at a lower resolution, keeping its aspect ratio, and drawn stretched to the widget.
//! The layout lives in raster pixels (Y down); the widget works in logical points (Y up);
//! [`to_raster`] and [`to_local`] convert using the raster's actual size, so hit testing
//! and outlines stay right when the raster was scaled down.

use agg_gui::{Point, Rect};

use crate::layout::PxRect;

/// Most pixels a map raster may have: 8192 × 8192 (256 MB of RGBA). Larger maps are
/// rasterised smaller and stretched.
pub const MAX_RASTER_PIXELS: u64 = 8192 * 8192;

/// The raster size for a widget of `width` × `height` logical points at `scale` physical
/// pixels per point: whole pixels, `(0, 0)` for an empty or invalid size, scaled down
/// (aspect kept) to at most [`MAX_RASTER_PIXELS`].
pub fn raster_size(width: f64, height: f64, scale: f64) -> (u32, u32) {
    let w = whole_pixels(width * scale);
    let h = whole_pixels(height * scale);
    if w == 0.0 || h == 0.0 {
        return (0, 0);
    }
    let pixels = w * h;
    let (w, h) = if pixels > MAX_RASTER_PIXELS as f64 {
        let shrink = (MAX_RASTER_PIXELS as f64 / pixels).sqrt();
        ((w * shrink).floor().max(1.0), (h * shrink).floor().max(1.0))
    } else {
        (w, h)
    };
    // Each side is at most MAX_RASTER_PIXELS (one pixel by that many), within u32.
    (
        w.min(MAX_RASTER_PIXELS as f64) as u32,
        h.min(MAX_RASTER_PIXELS as f64) as u32,
    )
}

/// A length in pixels rounded to whole pixels; 0 for anything not finite and positive.
fn whole_pixels(px: f64) -> f64 {
    let px = px.round();
    if px.is_finite() && px > 0.0 {
        px
    } else {
        0.0
    }
}

/// Raster pixels per logical point along each axis, for a raster of `raster` pixels shown
/// over `bounds`.
fn factors(raster: (u32, u32), bounds: Rect) -> Option<(f64, f64)> {
    if bounds.width <= 0.0 || bounds.height <= 0.0 || raster.0 == 0 || raster.1 == 0 {
        return None;
    }
    Some((
        f64::from(raster.0) / bounds.width,
        f64::from(raster.1) / bounds.height,
    ))
}

/// A local (Y-up, logical) point as raster pixels (Y-down).
pub fn to_raster(pos: Point, raster: (u32, u32), bounds: Rect) -> Option<(f64, f64)> {
    let (fx, fy) = factors(raster, bounds)?;
    Some((pos.x * fx, f64::from(raster.1) - pos.y * fy))
}

/// A raster rectangle (Y-down pixels) as a local (Y-up, logical) rectangle.
pub fn to_local(r: PxRect, raster: (u32, u32), bounds: Rect) -> Option<Rect> {
    let (fx, fy) = factors(raster, bounds)?;
    let top = f64::from(raster.1) - r.bottom();
    Some(Rect::new(r.x / fx, top / fy, r.w / fx, r.h / fy))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_windows_get_their_physical_size() {
        assert_eq!(raster_size(1200.0, 800.0, 2.0), (2400, 1600));
        assert_eq!(raster_size(100.4, 50.6, 1.0), (100, 51));
        assert_eq!(raster_size(0.0, 800.0, 2.0), (0, 0));
        assert_eq!(raster_size(f64::NAN, 800.0, 2.0), (0, 0));
        assert_eq!(raster_size(100.0, -5.0, 1.0), (0, 0));
    }

    #[test]
    fn huge_maps_are_capped_in_total_pixels_keeping_the_aspect() {
        // Each side under the old per-side cap of 65535, yet 17 GB of RGBA together.
        let (w, h) = raster_size(32_000.0, 32_000.0, 2.0);
        assert!(u64::from(w) * u64::from(h) <= MAX_RASTER_PIXELS, "{w}x{h}");
        assert_eq!(w, h);
        assert!(w >= 8000);
        let (w, h) = raster_size(40_000.0, 10_000.0, 1.0);
        assert!(u64::from(w) * u64::from(h) <= MAX_RASTER_PIXELS);
        assert!((f64::from(w) / f64::from(h) - 4.0).abs() < 0.01, "{w}x{h}");
        let (w, h) = raster_size(1e12, 1.0, 1.0);
        assert!(u64::from(w) * u64::from(h) <= MAX_RASTER_PIXELS);
        assert!(w >= 1 && h >= 1);
    }

    #[test]
    fn conversions_follow_a_scaled_down_raster() {
        let bounds = Rect::new(0.0, 0.0, 1000.0, 500.0);
        let raster = (500, 250); // half resolution
        assert_eq!(
            to_raster(Point::new(100.0, 400.0), raster, bounds),
            Some((50.0, 50.0))
        );
        let local = to_local(PxRect::new(50.0, 0.0, 100.0, 50.0), raster, bounds).expect("rect");
        assert_eq!(local, Rect::new(100.0, 400.0, 200.0, 100.0));
        assert_eq!(to_raster(Point::new(1.0, 1.0), (0, 0), bounds), None);
    }
}
