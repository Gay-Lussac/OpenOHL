// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Raster images of drawings (PNG), rendered from their SVG with `resvg`.
//! Texts use the computer's fonts (Helvetica, Arial, or another sans-serif
//! one found); without any font, texts are left out.
//!
//! Cost: roughly proportional to the pixels — an A3 sheet at 150 dpi
//! (2.7 Mpx) takes well under a second; images above [`MAX_PIXELS`] are
//! drawn at a lower resolution (the exit clause on memory and time).

use std::sync::{Arc, OnceLock};

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg;

use crate::model::Drawing;

/// The most pixels an image gets (≈ 160 MB while drawing).
pub const MAX_PIXELS: f64 = 40.0e6;

/// The computer's fonts, loaded once (≈ 0.1–0.5 s).
fn fonts() -> Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_system_fonts();
            let has = |name: &str| {
                db.faces()
                    .any(|f| f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)))
            };
            let family = [
                "Helvetica",
                "Arial",
                "Liberation Sans",
                "DejaVu Sans",
                "Noto Sans",
            ]
            .into_iter()
            .find(|n| has(n))
            .map(str::to_owned)
            .or_else(|| {
                db.faces()
                    .next()
                    .and_then(|f| f.families.first().map(|(n, _)| n.clone()))
            });
            if let Some(family) = family {
                db.set_sans_serif_family(family);
            }
            Arc::new(db)
        })
        .clone()
}

/// `svg` drawn at `px_per_mm` (lowered to stay within `max_pixels`), with
/// the scale used.
pub(crate) fn rasterize(
    svg: &str,
    px_per_mm: f64,
    max_pixels: f64,
) -> Result<(Pixmap, f64), String> {
    let options = usvg::Options {
        fontdb: fonts(),
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_str(svg, &options).map_err(|e| e.to_string())?;
    // usvg sizes in CSS pixels: 96 per inch.
    let size = tree.size();
    let (w_mm, h_mm) = (
        f64::from(size.width()) * 25.4 / 96.0,
        f64::from(size.height()) * 25.4 / 96.0,
    );
    let mut k = px_per_mm;
    if w_mm * h_mm * k * k > max_pixels {
        k = (max_pixels / (w_mm * h_mm)).sqrt();
    }
    let (w, h) = (
        (w_mm * k).ceil().max(1.0) as u32,
        (h_mm * k).ceil().max(1.0) as u32,
    );
    let mut pixmap =
        Pixmap::new(w, h).ok_or_else(|| format!("cannot make an image of {w} × {h} pixels"))?;
    let s = (k * 25.4 / 96.0) as f32;
    resvg::render(&tree, Transform::from_scale(s, s), &mut pixmap.as_mut());
    Ok((pixmap, k))
}

/// The drawing (a sheet) as PNG at `dpi` dots per inch, with the
/// resolution used (lower than asked when the image would be larger than
/// [`MAX_PIXELS`]).
pub fn render(d: &Drawing, dpi: f64) -> Result<(Vec<u8>, f64), String> {
    let (pixmap, k) = rasterize(&d.svg(), dpi.clamp(10.0, 2400.0) / 25.4, MAX_PIXELS)?;
    let png = pixmap.encode_png().map_err(|e| e.to_string())?;
    Ok((png, k * 25.4))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Color, Layer, Pen};

    #[test]
    fn sheets_render_as_png() {
        let mut d = Drawing::sheet(100.0, 50.0);
        d.path(
            Layer::Ground,
            vec![[0.0, 25.0], [100.0, 25.0]],
            Pen::new(Color(255, 0, 0), 2.0),
        );
        let (png, dpi) = render(&d, 100.0).unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!((dpi - 100.0).abs() < 1e-9);
        // 100 mm at 100 dpi: 394 pixels wide (IHDR width, big-endian).
        let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
        assert_eq!(width, 394);
        // The red line across the middle.
        let (pixmap, _) = rasterize(&d.svg(), 100.0 / 25.4, MAX_PIXELS).unwrap();
        let p = pixmap.pixel(200, 98).unwrap();
        assert!(p.red() > 200 && p.green() < 60, "{p:?}");
        // Too many pixels: a lower resolution.
        let (small, k) = rasterize(&d.svg(), 100.0 / 25.4, 5000.0).unwrap();
        assert!(f64::from(small.width() * small.height()) < 5200.0);
        assert!((k - 1.0).abs() < 0.01, "{k}");
    }
}
