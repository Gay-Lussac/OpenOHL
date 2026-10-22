// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! TIFF images the `image` crate does not read, read with the `tiff`
//! crate itself: above all orthophotos written **JPEG-compressed in
//! YCbCr** (GDAL's `COMPRESS=JPEG PHOTOMETRIC=YCBCR`, tiled or in strips,
//! with their JPEG tables in the `JPEGTables` tag), which many agencies
//! publish. The `tiff` crate decodes the JPEG strips or tiles (its JPEG
//! tables put in front, the chroma upsampled by the JPEG decoder) and
//! gives the samples in YCbCr; they are turned into RGB here
//! ([`ycbcr_to_rgb`]). Grey, RGB and RGBA of 8 or 16 bits are read too.
//! Any other variant (a palette, CMYK, Lab, floating point, several
//! bands…) is refused with a message naming it and the workaround
//! ([`unsupported`]).

use std::io::BufReader;
use std::path::Path;

use image::RgbImage;
use tiff::ColorType;
use tiff::decoder::{Decoder, DecodingResult, Limits};
use tiff::tags::Tag;

/// What to do with a TIFF that cannot be read.
pub const WORKAROUND: &str = "convert it to a JPEG with a world file (.jgw), or to a plain RGB \
    TIFF (for example `gdal_translate -co COMPRESS=DEFLATE -co PHOTOMETRIC=RGB in.tif out.tif`; \
    `-expand rgb` for a palette)";

/// The TIFF `path` (its first image) as RGB, 8 bits: refused when wider
/// or taller than `max_side` pixels or needing more than `max_bytes` to
/// decode.
pub fn read(path: &Path, max_side: u32, max_bytes: u64) -> Result<RgbImage, String> {
    let at = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let file = std::fs::File::open(path).map_err(|e| at(&e))?;
    let mut limits = Limits::default();
    let bytes = usize::try_from(max_bytes).unwrap_or(usize::MAX);
    limits.decoding_buffer_size = bytes;
    limits.intermediate_buffer_size = bytes;
    let mut d = Decoder::new(BufReader::new(file))
        .map_err(|e| at(&e))?
        .with_limits(limits);
    let (w, h) = d.dimensions().map_err(|e| at(&e))?;
    if w > max_side || h > max_side {
        return Err(at(&format!(
            "{w} × {h} pixels: larger than {max_side} pixels a side"
        )));
    }
    let variant = describe(&mut d);
    let color = d
        .colortype()
        .map_err(|e| unsupported(path, &variant, &e.to_string()))?;
    let fail = |e: &dyn std::fmt::Display| unsupported(path, &variant, &e.to_string());
    let rgb: Vec<u8> = match (color, d.read_image().map_err(|e| fail(&e))?) {
        (ColorType::RGB(8), DecodingResult::U8(v)) => v,
        (ColorType::RGBA(8), DecodingResult::U8(v)) => v
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect(),
        (ColorType::Gray(8), DecodingResult::U8(v)) => v.iter().flat_map(|g| [*g; 3]).collect(),
        (ColorType::YCbCr(8), DecodingResult::U8(mut v)) => {
            for p in v.as_chunks_mut::<3>().0 {
                *p = ycbcr_to_rgb(p[0], p[1], p[2]);
            }
            v
        }
        (ColorType::RGB(16), DecodingResult::U16(v)) => v.iter().map(|x| (x >> 8) as u8).collect(),
        (ColorType::RGBA(16), DecodingResult::U16(v)) => v
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]].map(|x| (x >> 8) as u8))
            .collect(),
        (ColorType::Gray(16), DecodingResult::U16(v)) => {
            v.iter().flat_map(|g| [(g >> 8) as u8; 3]).collect()
        }
        _ => return Err(unsupported(path, &variant, "not read")),
    };
    let expected = u64::from(w) * u64::from(h) * 3;
    if rgb.len() as u64 != expected {
        return Err(unsupported(
            path,
            &variant,
            &format!("{} bytes decoded for {expected} expected", rgb.len()),
        ));
    }
    RgbImage::from_raw(w, h, rgb).ok_or_else(|| unsupported(path, &variant, "not read"))
}

/// JPEG's YCbCr (full range, ITU-R BT.601 as in JFIF; what the JPEG
/// strips and tiles of a TIFF hold) to RGB, rounded and clamped.
pub fn ycbcr_to_rgb(y: u8, cb: u8, cr: u8) -> [u8; 3] {
    let (y, cb, cr) = (f64::from(y), f64::from(cb) - 128.0, f64::from(cr) - 128.0);
    let c = |v: f64| v.round().clamp(0.0, 255.0) as u8;
    [
        c(y + 1.402 * cr),
        c(y - 0.344_136 * cb - 0.714_136 * cr),
        c(y + 1.772 * cb),
    ]
}

/// The TIFF variant, in words, from its tags: its colour (photometric
/// interpretation and bands), bits per sample, sample format,
/// compression and layout.
fn describe<R: std::io::Read + std::io::Seek>(d: &mut Decoder<R>) -> String {
    let compression = match d.find_tag_unsigned::<u16>(Tag::Compression).ok().flatten() {
        None | Some(1) => "uncompressed".to_owned(),
        Some(5) => "LZW".into(),
        Some(6) => "old-style JPEG".into(),
        Some(7) => "JPEG".into(),
        Some(8 | 32946) => "deflate".into(),
        Some(32773) => "PackBits".into(),
        Some(50000) => "ZSTD".into(),
        Some(50001) => "WebP".into(),
        Some(34887) => "LERC".into(),
        Some(c) => format!("compression {c}"),
    };
    let planar = match d
        .find_tag_unsigned::<u16>(Tag::PlanarConfiguration)
        .ok()
        .flatten()
    {
        Some(2) => ", planar (one band after the other)",
        _ => "",
    };
    let layout = if d.find_tag(Tag::TileWidth).ok().flatten().is_some() {
        "tiled"
    } else {
        "in strips"
    };
    let first = |d: &mut Decoder<R>, tag| {
        d.find_tag_unsigned_vec::<u16>(tag)
            .ok()
            .flatten()
            .and_then(|v| v.first().copied())
    };
    let bits = first(d, Tag::BitsPerSample).unwrap_or(1);
    let samples = first(d, Tag::SamplesPerPixel).unwrap_or(1);
    let colour = match first(d, Tag::PhotometricInterpretation) {
        Some(0 | 1) if samples > 1 => format!("grey with {} extra bands", samples - 1),
        Some(0 | 1) => "grey".to_owned(),
        Some(2) if samples > 3 => format!("RGB with {} extra bands", samples - 3),
        Some(2) => "RGB".into(),
        Some(3) => "palette".into(),
        Some(4) => "transparency mask".into(),
        Some(5) => "CMYK".into(),
        Some(6) => "YCbCr".into(),
        Some(8..=10) => "CIE Lab".into(),
        Some(p) => format!("photometric interpretation {p}"),
        None => format!("{samples} bands"),
    };
    let float = match first(d, Tag::SampleFormat) {
        Some(2) => ", signed integers",
        Some(3) => ", floating point",
        _ => "",
    };
    format!("{colour}, {bits} bits{float}, {compression}, {layout}{planar}")
}

/// The message for a TIFF `path` of `variant` that cannot be read
/// (`why`): the variant and the workaround.
pub fn unsupported(path: &Path, variant: &str, why: &str) -> String {
    format!(
        "{}: this TIFF ({variant}) cannot be read ({why}); {WORKAROUND}",
        path.display()
    )
}

/// The variant of TIFF `path`, in words (none: not a TIFF read).
pub fn variant_of(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut d = Decoder::new(BufReader::new(file)).ok()?;
    Some(describe(&mut d))
}

#[cfg(test)]
#[path = "tiff_image_tests.rs"]
mod tests;
