// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The orthophotos of a project read colour by colour at any point: what
//! a sheet's plan needs to resample them along the line. Tiles are read
//! from the project file as they are needed (at the pyramid level suited
//! to the pixel size asked for) and kept in a small cache.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use image::RgbImage;

use crate::{BackgroundError, ImageInfo, Store, TILE, decode_tile};

/// Tiles kept decoded at most (≈ 200 KB each: ≈ 100 MB); past it the
/// cache is emptied (the sheets go along the line, so what is dropped is
/// mostly behind).
const CACHED_TILES: usize = 512;

type TileKey = (i64, u8, u32, u32);

/// A tile read (none: not kept, a blank tile).
type Tile = Option<Arc<RgbImage>>;

/// Orthophotos (images of the project), sampled at points.
pub struct Mosaic {
    /// The images, the last one drawn over the others.
    images: Vec<ImageInfo>,
    /// The project file's backgrounds.
    store: Mutex<Store>,
    /// Tiles read (none: not kept, a blank tile).
    cache: Mutex<HashMap<TileKey, Option<Arc<RgbImage>>>>,
}

impl std::fmt::Debug for Mosaic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mosaic")
            .field("images", &self.images.len())
            .finish_non_exhaustive()
    }
}

impl Mosaic {
    /// The images of project `file` in the image layers `layers` (none:
    /// every layer), in layer and name order (the last drawn over the
    /// others). None when there is no such image.
    pub fn open(file: &Path, layers: Option<&[String]>) -> Result<Option<Self>, BackgroundError> {
        let store = Store::open(file)?;
        let images: Vec<ImageInfo> = store
            .images()?
            .into_iter()
            .filter(|i| layers.is_none_or(|l| l.contains(&i.layer)))
            .collect();
        Ok((!images.is_empty()).then(|| Self::new(store, images)))
    }

    /// The images `images` of `store`.
    pub fn new(store: Store, images: Vec<ImageInfo>) -> Self {
        Self {
            images,
            store: Mutex::new(store),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// The images.
    pub fn images(&self) -> &[ImageInfo] {
        &self.images
    }

    /// The image layers, in order, each once.
    pub fn layers(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for i in &self.images {
            if !out.contains(&i.layer) {
                out.push(i.layer.clone());
            }
        }
        out
    }

    /// The colour at each of `points` (x, y in the images' coordinate
    /// system) for pixels of `metres_per_pixel` on the ground: bilinear
    /// between the pixels of the coarsest pyramid level not coarser than
    /// that; the image drawn last where several cover a point. `out`
    /// (as long as `points`) gets none where no image holds data (outside
    /// every image, or on a blank tile, which is not kept).
    ///
    /// Cost: four cached pixel reads per point, a tile read and decoded
    /// (≈ 2 ms) the first time one is needed.
    pub fn sample(&self, points: &[[f64; 2]], metres_per_pixel: f64, out: &mut [Option<[u8; 3]>]) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        for info in self.images.iter().rev() {
            let level = info.level_for(metres_per_pixel.max(0.0));
            let (w, h) = info.level_size(level);
            let f = f64::from(1u32 << level.min(31));
            let (px, py) = (info.georef.px * f, info.georef.py * f);
            let (lo, hi) = info.bounds();
            let mut tile = |tx: u32, ty: u32| -> Option<Arc<RgbImage>> {
                let key = (info.id, level, tx, ty);
                if let Some(t) = cache.get(&key) {
                    return t.clone();
                }
                if cache.len() >= CACHED_TILES {
                    cache.clear();
                }
                let t = store
                    .tile(info.id, level, tx, ty)
                    .ok()
                    .flatten()
                    .and_then(|data| decode_tile(&data).ok())
                    .map(Arc::new);
                cache.insert(key, t.clone());
                t
            };
            // The last tile used (neighbouring points share it).
            let mut last: Option<((u32, u32), Tile)> = None;
            let mut pixel = |c: u32, r: u32| -> Option<[u8; 3]> {
                let key = (c / TILE, r / TILE);
                let t = match &last {
                    Some((k, t)) if *k == key => t.clone(),
                    _ => {
                        let t = tile(key.0, key.1);
                        last = Some((key, t.clone()));
                        t
                    }
                }?;
                t.get_pixel_checked(c % TILE, r % TILE).map(|p| p.0)
            };
            for (p, o) in points.iter().zip(out.iter_mut()) {
                if o.is_some() || p[0] < lo[0] || p[0] > hi[0] || p[1] < lo[1] || p[1] > hi[1] {
                    continue;
                }
                // Pixel centres at whole numbers.
                let fx = ((p[0] - info.georef.x0) / px - 0.5).clamp(0.0, f64::from(w - 1));
                let fy = ((info.georef.y0 - p[1]) / py - 0.5).clamp(0.0, f64::from(h - 1));
                let (c0, r0) = (fx.floor() as u32, fy.floor() as u32);
                let (c1, r1) = ((c0 + 1).min(w - 1), (r0 + 1).min(h - 1));
                let (tx, ty) = (fx - f64::from(c0), fy - f64::from(r0));
                let corners = [
                    (pixel(c0, r0), (1.0 - tx) * (1.0 - ty)),
                    (pixel(c1, r0), tx * (1.0 - ty)),
                    (pixel(c0, r1), (1.0 - tx) * ty),
                    (pixel(c1, r1), tx * ty),
                ];
                let (mut sum, mut weight) = ([0.0f64; 3], 0.0);
                for (c, k) in corners {
                    if let Some(c) = c {
                        for (s, v) in sum.iter_mut().zip(c) {
                            *s += k * f64::from(v);
                        }
                        weight += k;
                    }
                }
                if weight > 1e-9 {
                    *o = Some(sum.map(|s| (s / weight).round().clamp(0.0, 255.0) as u8));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ohl_core::budget::Budget;

    /// A gradient of 400 × 200 pixels: red along x, green along y (both
    /// 0–255 over the image), blue 100.
    pub(crate) fn gradient() -> RgbImage {
        RgbImage::from_fn(400, 200, |x, y| {
            image::Rgb([
                (f64::from(x) / 399.0 * 255.0).round() as u8,
                (f64::from(y) / 199.0 * 255.0).round() as u8,
                100,
            ])
        })
    }

    #[test]
    fn a_mosaic_samples_its_images() {
        let dir = std::env::temp_dir().join(format!("ohl-mosaic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("g.png");
        gradient().save(&png).unwrap();
        // 1 m pixels, the upper-left corner at (500, 1200).
        std::fs::write(dir.join("g.pgw"), "1\n0\n0\n-1\n500.5\n1199.5\n").unwrap();
        let file = dir.join("p.ohl");
        let _ = std::fs::remove_file(&file);
        let mut store = Store::open(&file).unwrap();
        store
            .import_image("Ortho", "g", &png, "", "", None, &Budget::new(), |_| {})
            .unwrap();
        drop(store);
        assert!(
            Mosaic::open(&file, Some(&["other".into()]))
                .unwrap()
                .is_none()
        );
        let m = Mosaic::open(&file, None).unwrap().unwrap();
        assert_eq!(m.layers(), ["Ortho"]);
        let points = [
            [500.5, 1199.5],  // the first pixel's centre
            [899.5, 1000.5],  // the last one's
            [700.0, 1100.0],  // the middle
            [499.0, 1100.0],  // outside
            [1000.0, 1100.0], // outside
        ];
        let mut out = vec![None; points.len()];
        m.sample(&points, 1.0, &mut out);
        let near = |a: [u8; 3], b: [u8; 3]| (0..3).all(|k| a[k].abs_diff(b[k]) <= 6);
        assert!(near(out[0].unwrap(), [0, 0, 100]), "{:?}", out[0]);
        assert!(near(out[1].unwrap(), [255, 255, 100]), "{:?}", out[1]);
        assert!(near(out[2].unwrap(), [128, 128, 100]), "{:?}", out[2]);
        assert_eq!((out[3], out[4]), (None, None));
        // Coarser pixels: a coarser level, the same colours on average.
        let mut coarse = vec![None; points.len()];
        m.sample(&points, 4.0, &mut coarse);
        assert!(near(coarse[2].unwrap(), [128, 128, 100]), "{:?}", coarse[2]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
