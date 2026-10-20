// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Where an image lies: from a world file (`.tfw`, `.jgw`, `.pgw`, `.eww`,
//! `.wld`, …) or from GeoTIFF tags. North-up images only (no rotation).

use std::path::{Path, PathBuf};

/// Where an image lies: its upper-left corner (the outer corner of its
/// first pixel) and the size of a pixel, in the coordinate system's units.
/// Rows go south.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Georef {
    /// Easting of the upper-left corner.
    pub x0: f64,
    /// Northing of the upper-left corner.
    pub y0: f64,
    /// Pixel width (east).
    pub px: f64,
    /// Pixel height (south), positive.
    pub py: f64,
}

impl Georef {
    /// Lowest and highest (x, y) of an image of `width` × `height` pixels.
    pub fn bounds(&self, width: u32, height: u32) -> ([f64; 2], [f64; 2]) {
        (
            [self.x0, self.y0 - f64::from(height) * self.py],
            [self.x0 + f64::from(width) * self.px, self.y0],
        )
    }

    fn checked(self) -> Result<Self, String> {
        if [self.x0, self.y0, self.px, self.py]
            .iter()
            .all(|v| v.is_finite())
            && self.px > 0.0
            && self.py > 0.0
        {
            Ok(self)
        } else {
            Err(format!(
                "georeferencing: pixel size {} × {} at ({}, {}) — rows must go south, \
                 columns east",
                self.px, -self.py, self.x0, self.y0
            ))
        }
    }

    /// From a world file's six numbers: pixel width, two rotation terms,
    /// pixel height (negative), and the centre of the upper-left pixel.
    pub fn from_world_file(text: &str) -> Result<Self, String> {
        let v: Vec<f64> = text
            .split_whitespace()
            .map(|s| s.replace(',', ".").parse::<f64>())
            .collect::<Result<_, _>>()
            .map_err(|_| "a world file holds six numbers".to_owned())?;
        if v.len() != 6 {
            return Err(format!("a world file holds six numbers, not {}", v.len()));
        }
        let (a, d, b, e, c, f) = (v[0], v[1], v[2], v[3], v[4], v[5]);
        if d != 0.0 || b != 0.0 {
            return Err(
                "the image is rotated (world file rotation terms not 0): only north-up images \
                 are drawn; rectify it first"
                    .into(),
            );
        }
        Self {
            x0: c - a / 2.0,
            y0: f - e / 2.0,
            px: a,
            py: -e,
        }
        .checked()
    }
}

/// The world file of image `path`, if there is one next to it: the
/// extension's first and last letters with `w` (`.tif` → `.tfw`, `.jpg` →
/// `.jgw`, `.ecw` → `.eww`), the extension with `w` (`.tifw`), or `.wld`;
/// any case.
pub fn world_file_for(path: &Path) -> Option<PathBuf> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    let mut wanted = vec![format!("{ext}w"), "wld".to_owned()];
    if ext.len() >= 2 {
        let c: Vec<char> = ext.chars().collect();
        wanted.insert(0, format!("{}{}w", c[0], c[c.len() - 1]));
    }
    let stem = path.file_stem()?.to_string_lossy().to_lowercase();
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty());
    let dir = dir.unwrap_or(Path::new("."));
    let entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    wanted.iter().find_map(|w| {
        entries
            .iter()
            .find(|p| {
                p.file_stem()
                    .is_some_and(|s| s.to_string_lossy().to_lowercase() == stem)
                    && p.extension()
                        .is_some_and(|e| e.to_string_lossy().to_lowercase() == *w)
            })
            .cloned()
    })
}

/// GeoTIFF keys read.
const RASTER_TYPE: u16 = 1025;
const GEOGRAPHIC_TYPE: u16 = 2048;
const PROJECTED_TYPE: u16 = 3072;

/// The georeferencing of a GeoTIFF file and the EPSG code of its system
/// (when it names one); `None` without GeoTIFF tags.
pub fn geotiff(path: &Path) -> Result<Option<(Georef, Option<u16>)>, String> {
    use tiff::tags::Tag;
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut d = tiff::decoder::Decoder::new(std::io::BufReader::new(file))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let f64s = |d: &mut tiff::decoder::Decoder<_>, tag| -> Option<Vec<f64>> {
        d.find_tag(tag).ok().flatten()?.into_f64_vec().ok()
    };
    let keys: Vec<u16> = d
        .find_tag(Tag::GeoKeyDirectoryTag)
        .ok()
        .flatten()
        .and_then(|v| v.into_u16_vec().ok())
        .unwrap_or_default();
    // Header (version, revision, minor, count), then (key, location,
    // count, value) per key; only values held in the directory itself.
    let key = |id: u16| -> Option<u16> {
        keys.get(4..)?
            .as_chunks::<4>()
            .0
            .iter()
            .find(|k| k[0] == id && k[1] == 0)
            .map(|k| k[3])
    };
    let point = key(RASTER_TYPE) == Some(2);
    let epsg = key(PROJECTED_TYPE)
        .or_else(|| key(GEOGRAPHIC_TYPE))
        .filter(|c| (1024..32767).contains(c));
    let georef = if let Some(m) = f64s(&mut d, Tag::ModelTransformationTag) {
        if m.len() < 8 {
            return Err("GeoTIFF: model transformation of fewer than 16 values".into());
        }
        if m[1] != 0.0 || m[4] != 0.0 {
            return Err(
                "the image is rotated (GeoTIFF transformation): only north-up images are \
                 drawn; rectify it first"
                    .into(),
            );
        }
        Georef {
            x0: m[3],
            y0: m[7],
            px: m[0],
            py: -m[5],
        }
    } else {
        let (Some(scale), Some(tie)) = (
            f64s(&mut d, Tag::ModelPixelScaleTag),
            f64s(&mut d, Tag::ModelTiepointTag),
        ) else {
            return Ok(None);
        };
        if scale.len() < 2 || tie.len() < 6 {
            return Err("GeoTIFF: incomplete pixel scale or tie point".into());
        }
        // Tie point (i, j) ↔ (x, y); for "pixel is point" the corner is half
        // a pixel up and left of the first pixel's centre.
        let half = if point { 0.5 } else { 0.0 };
        Georef {
            x0: tie[3] - (tie[0] + half) * scale[0],
            y0: tie[4] + (tie[1] + half) * scale[1],
            px: scale[0],
            py: scale[1],
        }
    };
    Ok(Some((georef.checked()?, epsg)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_files_are_read() {
        // 0.1 m pixels, upper-left pixel centred at (1000.05, 2999.95).
        let g = Georef::from_world_file("0.1\n0\n0\n-0.1\n1000.05\n2999.95\n").unwrap();
        assert!((g.x0 - 1000.0).abs() < 1e-9 && (g.y0 - 3000.0).abs() < 1e-9);
        assert_eq!((g.px, g.py), (0.1, 0.1));
        let (lo, hi) = g.bounds(1000, 500);
        assert!((lo[0] - 1000.0).abs() < 1e-9 && (lo[1] - 2950.0).abs() < 1e-9);
        assert!((hi[0] - 1100.0).abs() < 1e-9 && (hi[1] - 3000.0).abs() < 1e-9);
        assert!(
            Georef::from_world_file("0.1 0.01 0 -0.1 1 2").is_err(),
            "rotated"
        );
        assert!(
            Georef::from_world_file("0.1 0 0 0.1 1 2").is_err(),
            "rows north"
        );
        assert!(Georef::from_world_file("1 2 3").is_err());
    }

    #[test]
    fn the_world_file_is_found_next_to_the_image() {
        let dir = std::env::temp_dir().join(format!("ohl-bg-wf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for f in [
            "a.tif", "a.TFW", "b.ecw", "b.eww", "c.png", "c.wld", "d.jpg",
        ] {
            std::fs::write(dir.join(f), "").unwrap();
        }
        assert_eq!(world_file_for(&dir.join("a.tif")), Some(dir.join("a.TFW")));
        assert_eq!(world_file_for(&dir.join("b.ecw")), Some(dir.join("b.eww")));
        assert_eq!(world_file_for(&dir.join("c.png")), Some(dir.join("c.wld")));
        assert_eq!(world_file_for(&dir.join("d.jpg")), None);
    }
}
