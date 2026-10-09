// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Backgrounds kept in the project file, drawn under the line:
//!
//! - **Orthophotos** (and any north-up raster): GeoTIFF, or TIFF / JPEG /
//!   PNG with a world file, imported as JPEG tiles of [`TILE`] pixels with
//!   a zoom pyramid (each level half the last, down to one tile), grouped
//!   in named layers (e.g. one per survey). ECW files are converted to
//!   GeoTIFF first by an external converter the user installs
//!   ([`convert_with`]); none is shipped (ECW decoding needs a licensed
//!   SDK).
//! - **Drawings** from DXF files ([`dxf`]): lines, polylines, circles,
//!   texts… by layer, in their colours, at their own heights.
//!
//! Like point clouds, they are part of the project file, not of the line:
//! not logged, not undone.

pub mod dxf;
pub mod georef;

use std::path::{Path, PathBuf};

use image::RgbImage;
use ols_core::budget::Budget;
use rusqlite::{Connection, OptionalExtension, params};

pub use dxf::Drawing;
pub use georef::Georef;

/// Moves points in place (e.g. an `ols_crs::Transformation`).
pub type Move<'a> = &'a dyn Fn(&mut [[f64; 3]]) -> Result<(), String>;

/// Tile size, pixels.
pub const TILE: u32 = 256;

/// JPEG quality of the tiles (0–100).
pub const QUALITY: u8 = 85;

/// Width and height of an imported image at most, pixels (a 5 km
/// orthophoto mosaic at 0.1 m).
pub const MAX_IMAGE_SIDE: u32 = 50_000;

/// Memory for decoding an imported image at most, bytes (8 GiB: a
/// 50 000 × 50 000 RGB image): past it, a clean error, not an abort.
pub const MAX_IMAGE_BYTES: u64 = 8 << 30;

/// An error of the store.
#[derive(Debug, Clone, PartialEq)]
pub struct BackgroundError(pub String);

impl std::fmt::Display for BackgroundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BackgroundError {}

impl From<rusqlite::Error> for BackgroundError {
    fn from(e: rusqlite::Error) -> Self {
        Self(e.to_string())
    }
}

fn err(m: impl Into<String>) -> BackgroundError {
    BackgroundError(m.into())
}

/// An image kept in the project.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageInfo {
    /// Row id.
    pub id: i64,
    /// Its layer (e.g. `Orthophoto 2024`).
    pub layer: String,
    /// Its name in the layer (the file's).
    pub name: String,
    /// Where it was imported from.
    pub source: String,
    /// Coordinate system (informative).
    pub crs: String,
    /// Where it lies.
    pub georef: Georef,
    /// Width, pixels.
    pub width: u32,
    /// Height, pixels.
    pub height: u32,
    /// Levels of the pyramid (level `k`: 1 / 2^k of the pixels each way).
    pub levels: u8,
}

impl ImageInfo {
    /// Lowest and highest (x, y).
    pub fn bounds(&self) -> ([f64; 2], [f64; 2]) {
        self.georef.bounds(self.width, self.height)
    }

    /// Size of level `level`, pixels.
    pub fn level_size(&self, level: u8) -> (u32, u32) {
        let d = 1u32 << level.min(31);
        (
            self.width.div_ceil(d).max(1),
            self.height.div_ceil(d).max(1),
        )
    }

    /// Tiles of level `level` across and down.
    pub fn tiles(&self, level: u8) -> (u32, u32) {
        let (w, h) = self.level_size(level);
        (w.div_ceil(TILE), h.div_ceil(TILE))
    }

    /// Where tile (`tx`, `ty`) of level `level` lies: lowest and highest
    /// (x, y), and its size in pixels.
    pub fn tile_bounds(&self, level: u8, tx: u32, ty: u32) -> ([f64; 2], [f64; 2], (u32, u32)) {
        let (w, h) = self.level_size(level);
        let f = f64::from(1u32 << level.min(31));
        let (px, py) = (self.georef.px * f, self.georef.py * f);
        let (c0, r0) = (tx * TILE, ty * TILE);
        let (tw, th) = ((w - c0).min(TILE), (h - r0).min(TILE));
        let x0 = self.georef.x0 + f64::from(c0) * px;
        let y1 = self.georef.y0 - f64::from(r0) * py;
        (
            [x0, y1 - f64::from(th) * py],
            [x0 + f64::from(tw) * px, y1],
            (tw, th),
        )
    }

    /// The pyramid level to draw at `metres_per_pixel` on screen: the
    /// coarsest whose pixels are not larger than the screen's.
    pub fn level_for(&self, metres_per_pixel: f64) -> u8 {
        let mut level = 0u8;
        while level + 1 < self.levels
            && self.georef.px * f64::from(1u32 << (level + 1)) <= metres_per_pixel
        {
            level += 1;
        }
        level
    }
}

/// A drawing kept in the project.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawingInfo {
    /// Unique name.
    pub name: String,
    /// Where it was imported from.
    pub source: String,
    /// Coordinate system (informative).
    pub crs: String,
    /// Lines, points and texts.
    pub items: u64,
    /// Lowest and highest (x, y, z).
    pub bounds: Option<([f64; 3], [f64; 3])>,
}

/// Tables of the backgrounds.
const SCHEMA: &str = "PRAGMA journal_mode = WAL;
    PRAGMA foreign_keys = ON;
    CREATE TABLE IF NOT EXISTS background_image (
        id INTEGER PRIMARY KEY, layer TEXT NOT NULL, name TEXT NOT NULL,
        source TEXT NOT NULL, crs TEXT NOT NULL DEFAULT '',
        x0 REAL NOT NULL, y0 REAL NOT NULL, px REAL NOT NULL, py REAL NOT NULL,
        width INTEGER NOT NULL, height INTEGER NOT NULL, levels INTEGER NOT NULL,
        created TEXT NOT NULL DEFAULT (datetime('now')),
        UNIQUE (layer, name));
    CREATE TABLE IF NOT EXISTS background_tile (
        image INTEGER NOT NULL REFERENCES background_image(id) ON DELETE CASCADE,
        level INTEGER NOT NULL, tx INTEGER NOT NULL, ty INTEGER NOT NULL,
        data BLOB NOT NULL,
        PRIMARY KEY (image, level, tx, ty));
    CREATE TABLE IF NOT EXISTS background_drawing (
        name TEXT PRIMARY KEY, source TEXT NOT NULL, crs TEXT NOT NULL DEFAULT '',
        items INTEGER NOT NULL, bounds TEXT NOT NULL, data BLOB NOT NULL,
        created TEXT NOT NULL DEFAULT (datetime('now')));";

/// The backgrounds of a project file.
pub struct Store {
    conn: Connection,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").finish_non_exhaustive()
    }
}

/// Halves an image (2 × 2 pixels averaged; an odd last row or column on
/// its own).
fn halve(img: &RgbImage) -> RgbImage {
    let (w, h) = img.dimensions();
    let (nw, nh) = (w.div_ceil(2), h.div_ceil(2));
    RgbImage::from_fn(nw, nh, |x, y| {
        let mut sum = [0u32; 3];
        let mut n = 0;
        for dy in 0..2 {
            for dx in 0..2 {
                let (sx, sy) = (2 * x + dx, 2 * y + dy);
                if sx < w && sy < h {
                    let p = img.get_pixel(sx, sy).0;
                    for k in 0..3 {
                        sum[k] += u32::from(p[k]);
                    }
                    n += 1;
                }
            }
        }
        image::Rgb(sum.map(|s| ((s + n / 2) / n) as u8))
    })
}

/// Whether every pixel is black or every pixel white (no data: not kept).
fn blank(img: &RgbImage) -> bool {
    let first = img.get_pixel(0, 0).0;
    (first == [0; 3] || first == [255; 3]) && img.pixels().all(|p| p.0 == first)
}

/// Encodes a tile as JPEG.
fn jpeg(img: &RgbImage) -> Result<Vec<u8>, BackgroundError> {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, QUALITY)
        .encode_image(img)
        .map_err(|e| err(e.to_string()))?;
    Ok(out)
}

/// Decodes a tile (RGB, 8 bits).
pub fn decode_tile(data: &[u8]) -> Result<RgbImage, BackgroundError> {
    image::load_from_memory_with_format(data, image::ImageFormat::Jpeg)
        .map(|i| i.to_rgb8())
        .map_err(|e| err(e.to_string()))
}

/// The georeferencing of image `path`: its GeoTIFF tags, else its world
/// file; and the EPSG code its GeoTIFF tags name.
pub fn georef_of(path: &Path) -> Result<(Georef, Option<u16>), String> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if (ext == "tif" || ext == "tiff")
        && let Some(found) = georef::geotiff(path)?
    {
        return Ok(found);
    }
    let wf = georef::world_file_for(path).ok_or_else(|| {
        format!(
            "{}: not georeferenced — no GeoTIFF tags and no world file (.tfw, .jgw, .pgw, .wld, …) \
             next to it",
            path.display()
        )
    })?;
    let text = std::fs::read_to_string(&wf).map_err(|e| format!("{}: {e}", wf.display()))?;
    Ok((
        Georef::from_world_file(&text).map_err(|e| format!("{}: {e}", wf.display()))?,
        None,
    ))
}

/// Converts `input` (e.g. an ECW file) to a GeoTIFF in `out_dir` with
/// `converter` (GDAL's `gdal_translate`, with the ECW driver for ECW
/// files): `converter -of GTiff input out.tif`. Returns the GeoTIFF.
///
/// Exit clause: the converter is stopped after `limit`.
pub fn convert_with(
    converter: &Path,
    input: &Path,
    out_dir: &Path,
    limit: std::time::Duration,
) -> Result<PathBuf, String> {
    let stem = input
        .file_stem()
        .map_or_else(|| "image".into(), |s| s.to_string_lossy().into_owned());
    let out = out_dir.join(format!("{stem}.tif"));
    let _ = std::fs::remove_file(&out);
    let mut child = std::process::Command::new(converter)
        .args(["-of", "GTiff", "-q"])
        .arg(input)
        .arg(&out)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("{}: {e}", converter.display()))?;
    let start = std::time::Instant::now();
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => {
                if status.success() && out.exists() {
                    return Ok(out);
                }
                let mut why = String::new();
                if let Some(mut e) = child.stderr.take() {
                    use std::io::Read;
                    let _ = e.read_to_string(&mut why);
                }
                return Err(format!(
                    "{} could not convert {}: {}",
                    converter.display(),
                    input.display(),
                    why.trim()
                ));
            }
            None if start.elapsed() > limit => {
                let _ = child.kill();
                return Err(format!(
                    "{}: stopped after {:?} converting {}",
                    converter.display(),
                    limit,
                    input.display()
                ));
            }
            None => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }
}

impl Store {
    /// Opens (or creates) the project file and its background tables.
    pub fn open(path: &Path) -> Result<Self, BackgroundError> {
        let conn = Connection::open(path).map_err(|e| err(format!("{}: {e}", path.display())))?;
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// The images kept, by layer and name.
    pub fn images(&self) -> Result<Vec<ImageInfo>, BackgroundError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, layer, name, source, crs, x0, y0, px, py, width, height, levels
             FROM background_image ORDER BY layer, name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ImageInfo {
                id: r.get(0)?,
                layer: r.get(1)?,
                name: r.get(2)?,
                source: r.get(3)?,
                crs: r.get(4)?,
                georef: Georef {
                    x0: r.get(5)?,
                    y0: r.get(6)?,
                    px: r.get(7)?,
                    py: r.get(8)?,
                },
                width: r.get(9)?,
                height: r.get(10)?,
                levels: r.get(11)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Tile (`tx`, `ty`) of level `level` of image `image` (JPEG), if kept
    /// (blank tiles are not).
    pub fn tile(
        &self,
        image: i64,
        level: u8,
        tx: u32,
        ty: u32,
    ) -> Result<Option<Vec<u8>>, BackgroundError> {
        Ok(self
            .conn
            .prepare_cached(
                "SELECT data FROM background_tile WHERE image = ?1 AND level = ?2 AND tx = ?3 AND ty = ?4",
            )?
            .query_row(params![image, level, tx, ty], |r| r.get(0))
            .optional()?)
    }

    /// Imports image `path` (GeoTIFF, or TIFF / JPEG / PNG with a world
    /// file) into layer `layer` as `name` (replacing an image of that
    /// name): its tiles and pyramid. `georef`: where it lies, when known
    /// otherwise (e.g. a converted ECW's world file); else read from the
    /// file ([`georef_of`]). `progress(fraction)` per row of tiles.
    ///
    /// Cost: decoding the image whole (3 bytes per pixel: a 10 000 ×
    /// 10 000 photo needs 300 MB), ~20 ms per tile written (JPEG); the
    /// pyramid adds a third. Exit clause: `budget`, checked per row of
    /// tiles; the image is then not kept. Images wider or taller than
    /// [`MAX_IMAGE_SIDE`], or needing more than [`MAX_IMAGE_BYTES`] to
    /// decode, are refused.
    #[allow(clippy::too_many_arguments)]
    pub fn import_image(
        &mut self,
        layer: &str,
        name: &str,
        path: &Path,
        source: &str,
        crs: &str,
        georef: Option<Georef>,
        budget: &Budget,
        mut progress: impl FnMut(f32),
    ) -> Result<ImageInfo, BackgroundError> {
        if layer.trim().is_empty() || name.trim().is_empty() {
            return Err(err("an image needs a layer and a name"));
        }
        let run = budget.start();
        let georef = match georef {
            Some(g) => g,
            None => georef_of(path).map_err(err)?.0,
        };
        let mut reader = image::ImageReader::open(path)
            .map_err(|e| err(format!("{}: {e}", path.display())))?
            .with_guessed_format()
            .map_err(|e| err(format!("{}: {e}", path.display())))?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_IMAGE_SIDE);
        limits.max_image_height = Some(MAX_IMAGE_SIDE);
        limits.max_alloc = Some(MAX_IMAGE_BYTES);
        reader.limits(limits);
        let mut img = reader
            .decode()
            .map_err(|e| err(format!("{}: {e}", path.display())))?
            .to_rgb8();
        let (width, height) = img.dimensions();
        let mut levels = 1u8;
        while (width.max(height) >> (levels - 1)) > TILE {
            levels += 1;
        }
        let total: u32 = (0..levels)
            .map(|l| {
                let d = 1u32 << l;
                width.div_ceil(d).div_ceil(TILE) * height.div_ceil(d).div_ceil(TILE)
            })
            .sum();
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM background_image WHERE layer = ?1 AND name = ?2",
            params![layer, name],
        )?;
        tx.execute(
            "INSERT INTO background_image (layer, name, source, crs, x0, y0, px, py, width, height, levels)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                layer, name, source, crs, georef.x0, georef.y0, georef.px, georef.py, width,
                height, levels
            ],
        )?;
        let id = tx.last_insert_rowid();
        let mut done = 0u32;
        for level in 0..levels {
            if level > 0 {
                img = halve(&img);
            }
            let (w, h) = img.dimensions();
            for ty in 0..h.div_ceil(TILE) {
                run.check()
                    .map_err(|stop| err(format!("stopped: {stop}")))?;
                for txi in 0..w.div_ceil(TILE) {
                    let (x, y) = (txi * TILE, ty * TILE);
                    let tile =
                        image::imageops::crop_imm(&img, x, y, TILE.min(w - x), TILE.min(h - y))
                            .to_image();
                    done += 1;
                    if blank(&tile) {
                        continue;
                    }
                    tx.execute(
                        "INSERT INTO background_tile (image, level, tx, ty, data) VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![id, level, txi, ty, jpeg(&tile)?],
                    )?;
                }
                progress(done as f32 / total as f32);
            }
        }
        tx.commit()?;
        self.images()?
            .into_iter()
            .find(|i| i.id == id)
            .ok_or_else(|| err("the image was not kept"))
    }

    /// Removes image `id`.
    pub fn delete_image(&self, id: i64) -> Result<(), BackgroundError> {
        self.conn
            .execute("DELETE FROM background_image WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Removes every image of layer `layer`.
    pub fn delete_image_layer(&self, layer: &str) -> Result<usize, BackgroundError> {
        Ok(self.conn.execute(
            "DELETE FROM background_image WHERE layer = ?1",
            params![layer],
        )?)
    }

    /// The drawings kept, by name.
    pub fn drawings(&self) -> Result<Vec<DrawingInfo>, BackgroundError> {
        let mut stmt = self.conn.prepare(
            "SELECT name, source, crs, items, bounds FROM background_drawing ORDER BY name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(DrawingInfo {
                name: r.get(0)?,
                source: r.get(1)?,
                crs: r.get(2)?,
                items: r.get::<_, i64>(3)? as u64,
                bounds: serde_json::from_str(&r.get::<_, String>(4)?).ok().flatten(),
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Drawing `name`.
    pub fn drawing(&self, name: &str) -> Result<Drawing, BackgroundError> {
        let data: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT data FROM background_drawing WHERE name = ?1",
                params![name],
                |r| r.get(0),
            )
            .optional()?;
        let data = data.ok_or_else(|| err(format!("no drawing \"{name}\"")))?;
        let raw = zstd::decode_all(data.as_slice()).map_err(|e| err(e.to_string()))?;
        serde_json::from_slice(&raw).map_err(|e| err(format!("drawing {name}: {e}")))
    }

    /// Keeps drawing `name` (replacing one of that name).
    pub fn put_drawing(
        &self,
        name: &str,
        source: &str,
        crs: &str,
        drawing: &Drawing,
    ) -> Result<DrawingInfo, BackgroundError> {
        if name.trim().is_empty() {
            return Err(err("a drawing needs a name"));
        }
        let json = serde_json::to_vec(drawing).map_err(|e| err(e.to_string()))?;
        let data = zstd::encode_all(json.as_slice(), 3).map_err(|e| err(e.to_string()))?;
        let bounds = drawing.bounds();
        self.conn.execute(
            "INSERT INTO background_drawing (name, source, crs, items, bounds, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(name) DO UPDATE SET source = excluded.source, crs = excluded.crs,
                 items = excluded.items, bounds = excluded.bounds, data = excluded.data,
                 created = datetime('now')",
            params![
                name,
                source,
                crs,
                drawing.len() as i64,
                serde_json::to_string(&bounds).unwrap_or_else(|_| "null".into()),
                data
            ],
        )?;
        self.drawings()?
            .into_iter()
            .find(|d| d.name == name)
            .ok_or_else(|| err("the drawing was not kept"))
    }

    /// Reads DXF file `path` and keeps it as drawing `name`.
    pub fn import_dxf(
        &self,
        name: &str,
        path: &Path,
        crs: &str,
    ) -> Result<(DrawingInfo, Drawing), BackgroundError> {
        let bytes = std::fs::read(path).map_err(|e| err(format!("{}: {e}", path.display())))?;
        let drawing = dxf::read_dxf(&bytes).map_err(|e| err(format!("{}: {e}", path.display())))?;
        if drawing.is_empty() {
            return Err(err(format!("{}: nothing to draw", path.display())));
        }
        let info = self.put_drawing(name, &path.display().to_string(), crs, &drawing)?;
        Ok((info, drawing))
    }

    /// Removes drawing `name`.
    pub fn delete_drawing(&self, name: &str) -> Result<(), BackgroundError> {
        self.conn.execute(
            "DELETE FROM background_drawing WHERE name = ?1",
            params![name],
        )?;
        Ok(())
    }

    /// Moves every drawing into coordinate system `crs` with `transform`,
    /// and removes the images (they would need resampling: imported again
    /// in the new system). Returns the number of images removed.
    pub fn reproject(&self, crs: &str, transform: Move<'_>) -> Result<usize, BackgroundError> {
        for info in self.drawings()? {
            let mut d = self.drawing(&info.name)?;
            d.transform(transform)
                .map_err(|e| err(format!("drawing {}: {e}", info.name)))?;
            self.put_drawing(&info.name, &info.source, crs, &d)?;
        }
        Ok(self.conn.execute("DELETE FROM background_image", [])?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ols-bg-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A 600 × 300 image: a gradient, its left 256 columns black (no
    /// data) in the top row of tiles.
    fn photo() -> RgbImage {
        RgbImage::from_fn(600, 300, |x, y| {
            if x < 256 && y < 256 {
                image::Rgb([0, 0, 0])
            } else {
                image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
            }
        })
    }

    #[test]
    fn an_orthophoto_is_tiled_with_its_pyramid() {
        let dir = scratch("tiles");
        let png = dir.join("photo.png");
        photo().save(&png).unwrap();
        // 0.5 m pixels, upper-left pixel centred at (1000.25, 2999.75).
        std::fs::write(dir.join("photo.pgw"), "0.5\n0\n0\n-0.5\n1000.25\n2999.75\n").unwrap();
        let mut store = Store::open(&dir.join("p.ols")).unwrap();
        let mut steps = 0;
        let info = store
            .import_image(
                "Ortho",
                "photo",
                &png,
                "photo.png",
                "EPSG:2056",
                None,
                &Budget::new(),
                |_| steps += 1,
            )
            .unwrap();
        assert!(steps > 0);
        assert_eq!((info.width, info.height, info.levels), (600, 300, 3));
        assert_eq!(info.bounds(), ([1000.0, 2850.0], [1300.0, 3000.0]));
        assert_eq!(info.tiles(0), (3, 2));
        assert_eq!(info.tiles(2), (1, 1));
        // The black tile is not kept; the others are.
        assert!(store.tile(info.id, 0, 0, 0).unwrap().is_none());
        let t = decode_tile(&store.tile(info.id, 0, 2, 1).unwrap().unwrap()).unwrap();
        assert_eq!(t.dimensions(), (600 - 512, 300 - 256));
        let (lo, hi, size) = info.tile_bounds(0, 2, 1);
        assert_eq!(size, (88, 44));
        assert_eq!((lo, hi), ([1256.0, 2850.0], [1300.0, 2872.0]));
        let top = decode_tile(&store.tile(info.id, 2, 0, 0).unwrap().unwrap()).unwrap();
        assert_eq!(top.dimensions(), (150, 75));
        // Levels for a zoom: 0.5 m/pixel on screen → level 0; 2 m → 2.
        assert_eq!(info.level_for(0.4), 0);
        assert_eq!(info.level_for(1.0), 1);
        assert_eq!(info.level_for(100.0), 2);
        // Again: replaced, not added.
        store
            .import_image(
                "Ortho",
                "photo",
                &png,
                "photo.png",
                "",
                None,
                &Budget::new(),
                |_| {},
            )
            .unwrap();
        assert_eq!(store.images().unwrap().len(), 1);
        // Without georeferencing: refused.
        let bare = dir.join("bare.png");
        photo().save(&bare).unwrap();
        let e = store
            .import_image("Ortho", "bare", &bare, "", "", None, &Budget::new(), |_| {})
            .unwrap_err();
        assert!(e.0.contains("not georeferenced"), "{e}");
        assert_eq!(store.delete_image_layer("Ortho").unwrap(), 1);
        assert!(store.images().unwrap().is_empty());
    }

    #[test]
    fn an_image_past_the_limits_is_refused() {
        let dir = scratch("limits");
        // Tiny on disk, wider than the limit once decoded.
        let png = dir.join("wide.png");
        RgbImage::new(MAX_IMAGE_SIDE + 1, 1).save(&png).unwrap();
        std::fs::write(dir.join("wide.pgw"), "0.5\n0\n0\n-0.5\n0\n0\n").unwrap();
        let mut store = Store::open(&dir.join("p.ols")).unwrap();
        let e = store
            .import_image("Ortho", "wide", &png, "", "", None, &Budget::new(), |_| {})
            .unwrap_err();
        assert!(e.0.contains("wide.png"), "{e}");
        assert!(store.images().unwrap().is_empty());
    }

    #[test]
    fn a_geotiff_places_itself() {
        let dir = scratch("geotiff");
        let path = dir.join("g.tif");
        {
            use tiff::encoder::{TiffEncoder, colortype::RGB8};
            use tiff::tags::Tag;
            let file = std::fs::File::create(&path).unwrap();
            let mut enc = TiffEncoder::new(file).unwrap();
            let mut img = enc.new_image::<RGB8>(4, 2).unwrap();
            img.encoder()
                .write_tag(Tag::ModelPixelScaleTag, &[0.25f64, 0.25, 0.0][..])
                .unwrap();
            img.encoder()
                .write_tag(
                    Tag::ModelTiepointTag,
                    &[0.0f64, 0.0, 0.0, 2_600_000.0, 1_200_000.0, 0.0][..],
                )
                .unwrap();
            // Version 1.1.0, 2 keys: area pixels, projected EPSG 2056.
            img.encoder()
                .write_tag(
                    Tag::GeoKeyDirectoryTag,
                    &[1u16, 1, 0, 2, 1025, 0, 1, 1, 3072, 0, 1, 2056][..],
                )
                .unwrap();
            img.write_data(&[200u8; 4 * 2 * 3]).unwrap();
        }
        let (g, epsg) = georef_of(&path).unwrap();
        assert_eq!(epsg, Some(2056));
        assert_eq!(
            (g.x0, g.y0, g.px, g.py),
            (2_600_000.0, 1_200_000.0, 0.25, 0.25)
        );
        let mut store = Store::open(&dir.join("p.ols")).unwrap();
        let info = store
            .import_image("O", "g", &path, "", "", None, &Budget::new(), |_| {})
            .unwrap();
        assert_eq!(
            info.bounds(),
            ([2_600_000.0, 1_199_999.5], [2_600_001.0, 1_200_000.0])
        );
    }

    #[test]
    fn drawings_are_kept_and_moved_with_the_project() {
        let dir = scratch("drawings");
        let dxf_path = dir.join("cadastre.dxf");
        std::fs::write(
            &dxf_path,
            "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\nParcels\n10\n0\n20\n0\n11\n10\n21\n0\n0\nENDSEC\n0\nEOF\n",
        )
        .unwrap();
        let store = Store::open(&dir.join("p.ols")).unwrap();
        let (info, d) = store
            .import_dxf("cadastre", &dxf_path, "EPSG:2056")
            .unwrap();
        assert_eq!(info.items, 1);
        assert_eq!(info.bounds, Some(([0.0; 3], [10.0, 0.0, 0.0])));
        assert_eq!(store.drawing("cadastre").unwrap(), d);
        let empty = dir.join("empty.dxf");
        std::fs::write(&empty, "0\nEOF\n").unwrap();
        assert!(store.import_dxf("e", &empty, "").is_err());
        let shift = |p: &mut [[f64; 3]]| -> Result<(), String> {
            for q in p {
                q[0] += 5.0;
            }
            Ok(())
        };
        assert_eq!(store.reproject("EPSG:32632", &shift).unwrap(), 0);
        let moved = store.drawing("cadastre").unwrap();
        assert_eq!(moved.lines[0].points[1], [15.0, 0.0, 0.0]);
        assert_eq!(store.drawings().unwrap()[0].crs, "EPSG:32632");
        store.delete_drawing("cadastre").unwrap();
        assert!(store.drawings().unwrap().is_empty());
    }

    #[test]
    fn a_converter_that_fails_says_so() {
        let dir = scratch("convert");
        let input = dir.join("x.ecw");
        std::fs::write(&input, "").unwrap();
        let e = convert_with(
            Path::new("/nonexistent/gdal_translate"),
            &input,
            &dir,
            std::time::Duration::from_secs(5),
        )
        .unwrap_err();
        assert!(e.contains("gdal_translate"), "{e}");
        // `false` runs and fails.
        if Path::new("/usr/bin/false").exists() {
            let e = convert_with(
                Path::new("/usr/bin/false"),
                &input,
                &dir,
                std::time::Duration::from_secs(5),
            )
            .unwrap_err();
            assert!(e.contains("could not convert"), "{e}");
        }
    }
}
