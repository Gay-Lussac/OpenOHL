// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Small synthetic TIFFs written here byte by byte as GDAL writes them
//! (`COMPRESS=JPEG PHOTOMETRIC=YCBCR`: each strip or tile a JPEG of
//! YCbCr with its tables in the `JPEGTables` tag), a plain RGB TIFF
//! (deflate), a palette TIFF; georeferenced by GeoTIFF tags or a world
//! file; imported, their places and colours checked.

use std::path::{Path, PathBuf};

use image::RgbImage;
use ohl_core::budget::Budget;
use tiff::tags::Tag;

use super::{WORKAROUND, variant_of, ycbcr_to_rgb};
use crate::{Store, decode_image, decode_tile, georef_of};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ohl-tiff-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The colours of the test image: blocks of 16 × 16 pixels, each its own
/// colour (saturated and mixed, so a wrong YCbCr → RGB shows).
const COLOURS: [[u8; 3]; 12] = [
    [200, 30, 30],
    [30, 180, 40],
    [40, 60, 210],
    [230, 220, 60],
    [20, 200, 200],
    [190, 40, 190],
    [250, 250, 250],
    [10, 10, 10],
    [128, 128, 128],
    [240, 140, 20],
    [90, 50, 20],
    [150, 200, 120],
];

/// 64 × 48 pixels: 4 × 3 blocks of [`COLOURS`].
fn picture() -> RgbImage {
    RgbImage::from_fn(64, 48, |x, y| {
        image::Rgb(COLOURS[(y / 16 * 4 + x / 16) as usize])
    })
}

/// Every block's middle within `tol` of its colour.
fn assert_colours(img: &RgbImage, tol: i32) {
    assert_eq!(img.dimensions(), (64, 48));
    for by in 0..3u32 {
        for bx in 0..4u32 {
            let want = COLOURS[(by * 4 + bx) as usize];
            let got = img.get_pixel(bx * 16 + 8, by * 16 + 8).0;
            for c in 0..3 {
                assert!(
                    (i32::from(got[c]) - i32::from(want[c])).abs() <= tol,
                    "block ({bx}, {by}): {got:?} for {want:?}"
                );
            }
        }
    }
}

/// A JPEG of `img` as the image crate writes it (JFIF, YCbCr): split into
/// its tables (SOI, the DQT and DHT segments, EOI — `JPEGTables`) and the
/// abbreviated stream without them nor the JFIF marker (as libtiff
/// writes them); with the luma's sampling (h, v), the
/// chroma's being 1 × 1: `YCbCrSubSampling`.
fn jpeg_parts(img: &RgbImage) -> (Vec<u8>, Vec<u8>, (u16, u16)) {
    let mut full = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut full, 95)
        .encode_image(img)
        .unwrap();
    assert_eq!(&full[..2], &[0xFF, 0xD8]);
    let mut tables = vec![0xFF, 0xD8];
    let mut stream = vec![0xFF, 0xD8];
    let mut sampling = (1, 1);
    let mut i = 2;
    loop {
        assert_eq!(full[i], 0xFF);
        let marker = full[i + 1];
        let l = usize::from(u16::from_be_bytes([full[i + 2], full[i + 3]]));
        let seg = &full[i..i + 2 + l];
        match marker {
            0xDB | 0xC4 => tables.extend_from_slice(seg),
            // No JFIF (APPn) marker in the strips, as libtiff writes them.
            0xE0..=0xEF => {}
            0xDA => {
                stream.extend_from_slice(&full[i..]);
                break;
            }
            _ => {
                if marker == 0xC0 {
                    // SOF0: marker, length, precision, height, width, the
                    // number of components, then the first's id and
                    // sampling.
                    let s = seg[2 + 2 + 1 + 2 + 2 + 1 + 1];
                    sampling = (u16::from(s >> 4), u16::from(s & 15));
                }
                stream.extend_from_slice(seg);
            }
        }
        i += 2 + l;
    }
    tables.extend_from_slice(&[0xFF, 0xD9]);
    (tables, stream, sampling)
}

/// A TIFF tag's value: (type, count, bytes).
type Value = (u16, u32, Vec<u8>);

fn shorts(v: &[u16]) -> Value {
    (
        3,
        v.len() as u32,
        v.iter().flat_map(|x| x.to_le_bytes()).collect(),
    )
}

fn longs(v: &[u32]) -> Value {
    (
        4,
        v.len() as u32,
        v.iter().flat_map(|x| x.to_le_bytes()).collect(),
    )
}

fn doubles(v: &[f64]) -> Value {
    (
        12,
        v.len() as u32,
        v.iter().flat_map(|x| x.to_le_bytes()).collect(),
    )
}

/// A little-endian TIFF of one image: `chunks` (strips or tiles), the tags
/// `tags`, and the chunks' offsets and byte counts under the tags
/// `offsets` and `counts`.
fn write_tiff(
    path: &Path,
    mut tags: Vec<(u16, Value)>,
    chunks: &[Vec<u8>],
    (offsets, counts): (u16, u16),
) {
    let mut out = vec![b'I', b'I', 42, 0, 0, 0, 0, 0];
    let mut at = Vec::new();
    for c in chunks {
        at.push(out.len() as u32);
        out.extend_from_slice(c);
    }
    tags.push((offsets, longs(&at)));
    let sizes: Vec<u32> = chunks.iter().map(|c| c.len() as u32).collect();
    tags.push((counts, longs(&sizes)));
    tags.sort_by_key(|t| t.0);
    // Values longer than 4 bytes, after the chunks.
    let mut place = Vec::new();
    for (_, (_, _, bytes)) in &tags {
        if bytes.len() > 4 {
            if out.len() % 2 == 1 {
                out.push(0);
            }
            place.push(Some(out.len() as u32));
            out.extend_from_slice(bytes);
        } else {
            place.push(None);
        }
    }
    if out.len() % 2 == 1 {
        out.push(0);
    }
    let ifd = out.len() as u32;
    out[4..8].copy_from_slice(&ifd.to_le_bytes());
    out.extend_from_slice(&(tags.len() as u16).to_le_bytes());
    for ((tag, (ty, count, bytes)), off) in tags.iter().zip(place) {
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&ty.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        match off {
            Some(o) => out.extend_from_slice(&o.to_le_bytes()),
            None => {
                let mut v = bytes.clone();
                v.resize(4, 0);
                out.extend_from_slice(&v);
            }
        }
    }
    out.extend_from_slice(&[0; 4]);
    std::fs::write(path, out).unwrap();
}

/// The GeoTIFF tags: 0.5 m pixels, the top left corner at (1000, 2000), a
/// projected system (EPSG 32632, a generic test code).
fn geotiff_tags() -> Vec<(u16, Value)> {
    vec![
        (33550, doubles(&[0.5, 0.5, 0.0])),
        (33922, doubles(&[0.0, 0.0, 0.0, 1000.0, 2000.0, 0.0])),
        (
            34735,
            shorts(&[1, 1, 0, 2, 1025, 0, 1, 1, 3072, 0, 1, 32632]),
        ),
    ]
}

/// [`picture`] as GDAL writes it with `COMPRESS=JPEG PHOTOMETRIC=YCBCR`:
/// tiles of 32 × 32 (the last ones padded) or strips of 16 rows, each an
/// abbreviated JPEG, the tables in `JPEGTables`; with the GeoTIFF tags
/// when `geo`.
fn ycbcr_jpeg_tiff(path: &Path, tiled: bool, geo: bool) {
    let img = picture();
    let (w, h) = img.dimensions();
    let (cw, ch) = if tiled { (32, 32) } else { (w, 16) };
    let mut chunks = Vec::new();
    let mut tables = Vec::new();
    let mut sampling = (1, 1);
    for ty in 0..h.div_ceil(ch) {
        for tx in 0..w.div_ceil(cw) {
            // Padded with the edge's pixels, as encoders do.
            let tile = RgbImage::from_fn(cw, ch, |x, y| {
                *img.get_pixel((tx * cw + x).min(w - 1), (ty * ch + y).min(h - 1))
            });
            let (t, s, smp) = jpeg_parts(&tile);
            chunks.push(s);
            tables = t;
            sampling = smp;
        }
    }
    let mut tags = vec![
        (256, longs(&[w])),
        (257, longs(&[h])),
        (258, shorts(&[8, 8, 8])),
        (259, shorts(&[7])),
        (262, shorts(&[6])),
        (277, shorts(&[3])),
        (284, shorts(&[1])),
        (347, (7, tables.len() as u32, tables)),
        (530, shorts(&[sampling.0, sampling.1])),
    ];
    if tiled {
        tags.push((322, longs(&[cw])));
        tags.push((323, longs(&[ch])));
    } else {
        tags.push((278, longs(&[ch])));
    }
    if geo {
        tags.extend(geotiff_tags());
    }
    let chunk_tags = if tiled { (324, 325) } else { (273, 279) };
    write_tiff(path, tags, &chunks, chunk_tags);
}

/// The image crate alone does not read them (the message the owner saw);
/// read here, tiled and in strips: every block's colour within 6 levels
/// (JPEG at quality 95; the blocks' middles 8 pixels from any edge, where
/// the subsampled chroma is the block's own); imported with the GeoTIFF
/// tags: placed by them (0.5 m pixels, the corner at (1000, 2000): 32 m
/// by 24 m), the first tile's colours kept (JPEG twice: 12 levels).
#[test]
fn jpeg_compressed_ycbcr_geotiffs_are_read_tiled_and_in_strips() {
    let dir = scratch("ycbcr");
    for tiled in [true, false] {
        let name = if tiled { "tiled" } else { "strips" };
        let path = dir.join(format!("{name}.tif"));
        ycbcr_jpeg_tiff(&path, tiled, true);
        let by_image = image::ImageReader::open(&path)
            .unwrap()
            .with_guessed_format()
            .unwrap()
            .decode();
        assert!(by_image.is_err(), "the image crate reads it now");
        let img = decode_image(&path).unwrap();
        assert_colours(&img, 6);
        let (g, epsg) = georef_of(&path).unwrap();
        assert_eq!((g.x0, g.y0, g.px, g.py), (1000.0, 2000.0, 0.5, 0.5));
        assert_eq!(epsg, Some(32632));
        let mut store = Store::open(&dir.join("p.ohl")).unwrap();
        let info = store
            .import_image("O", name, &path, "", "", None, &Budget::new(), |_| {})
            .unwrap();
        assert_eq!((info.width, info.height), (64, 48));
        assert_eq!(info.bounds(), ([1000.0, 1976.0], [1032.0, 2000.0]));
        let tile = decode_tile(&store.tile(info.id, 0, 0, 0).unwrap().unwrap()).unwrap();
        let tile = image::imageops::crop_imm(&tile, 0, 0, 64, 48).to_image();
        assert_colours(&tile, 12);
    }
}

/// Without GeoTIFF tags, a world file (.tfw) places it: pixel (0, 0)'s
/// centre at (500.25, 899.75), 0.5 m pixels — its corner at (500, 900).
/// Neither: not georeferenced, said.
#[test]
fn without_geotiff_tags_the_world_file_places_it() {
    let dir = scratch("tfw");
    let path = dir.join("w.tif");
    ycbcr_jpeg_tiff(&path, true, false);
    std::fs::write(dir.join("w.tfw"), "0.5\n0\n0\n-0.5\n500.25\n899.75\n").unwrap();
    let (g, epsg) = georef_of(&path).unwrap();
    assert_eq!(epsg, None);
    assert_eq!((g.x0, g.y0, g.px, g.py), (500.0, 900.0, 0.5, 0.5));
    assert_colours(&decode_image(&path).unwrap(), 6);
    std::fs::remove_file(dir.join("w.tfw")).unwrap();
    assert!(georef_of(&path).unwrap_err().contains("not georeferenced"));
}

/// A plain RGB TIFF, deflate (the image crate reads it): the colours
/// exact; placed by its GeoTIFF tags; its variant in words.
#[test]
fn a_plain_rgb_deflate_tiff_is_read_exactly() {
    use tiff::encoder::{Compression, TiffEncoder, colortype::RGB8};
    let dir = scratch("deflate");
    let path = dir.join("rgb.tif");
    let img = picture();
    {
        let file = std::fs::File::create(&path).unwrap();
        let mut enc = TiffEncoder::new(file)
            .unwrap()
            .with_compression(Compression::Deflate(Default::default()));
        let mut out = enc.new_image::<RGB8>(64, 48).unwrap();
        out.encoder()
            .write_tag(Tag::ModelPixelScaleTag, &[0.5f64, 0.5, 0.0][..])
            .unwrap();
        out.encoder()
            .write_tag(
                Tag::ModelTiepointTag,
                &[0.0f64, 0.0, 0.0, 1000.0, 2000.0, 0.0][..],
            )
            .unwrap();
        out.encoder()
            .write_tag(
                Tag::GeoKeyDirectoryTag,
                &[1u16, 1, 0, 2, 1025, 0, 1, 1, 3072, 0, 1, 32632][..],
            )
            .unwrap();
        out.write_data(img.as_raw()).unwrap();
    }
    assert_eq!(decode_image(&path).unwrap(), img);
    let (g, epsg) = georef_of(&path).unwrap();
    assert_eq!((g.x0, g.y0, g.px, g.py), (1000.0, 2000.0, 0.5, 0.5));
    assert_eq!(epsg, Some(32632));
    assert_eq!(
        variant_of(&path).unwrap(),
        "RGB, 8 bits, deflate, in strips"
    );
}

/// A variant neither reads (a palette): refused, the message naming it
/// and the workaround.
#[test]
fn an_unsupported_variant_is_named_with_the_workaround() {
    let dir = scratch("palette");
    let path = dir.join("pal.tif");
    let colormap: Vec<u16> = (0..3 * 256).map(|k| (k % 256) as u16 * 257).collect();
    let tags = vec![
        (256, longs(&[4])),
        (257, longs(&[2])),
        (258, shorts(&[8])),
        (259, shorts(&[1])),
        (262, shorts(&[3])),
        (277, shorts(&[1])),
        (278, longs(&[2])),
        (320, shorts(&colormap)),
    ];
    write_tiff(&path, tags, &[vec![0, 1, 2, 3, 4, 5, 6, 7]], (273, 279));
    let e = decode_image(&path).unwrap_err();
    assert!(e.contains("pal.tif"), "{e}");
    assert!(
        e.contains("palette, 8 bits, uncompressed, in strips"),
        "{e}"
    );
    assert!(e.contains(WORKAROUND), "{e}");
}

/// JPEG's YCbCr to RGB (JFIF: Y = 0.299 R + 0.587 G + 0.114 B,
/// Cb = 128 + (B − Y)/1.772, Cr = 128 + (R − Y)/1.402): grey stays grey;
/// pure red is (76, 85, 255) and pure blue (29, 255, 107), back within
/// two levels of rounding.
#[test]
fn ycbcr_to_rgb_is_jfif() {
    assert_eq!(ycbcr_to_rgb(128, 128, 128), [128, 128, 128]);
    assert_eq!(ycbcr_to_rgb(0, 128, 128), [0, 0, 0]);
    assert_eq!(ycbcr_to_rgb(255, 128, 128), [255, 255, 255]);
    let red = ycbcr_to_rgb(76, 85, 255);
    assert!(red[0] >= 253 && red[1] <= 2 && red[2] <= 2, "{red:?}");
    let blue = ycbcr_to_rgb(29, 255, 107);
    assert!(blue[2] >= 253 && blue[0] <= 2 && blue[1] <= 2, "{blue:?}");
}
