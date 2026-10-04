// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Point clouds stored in the project file (ADR-0007).
//!
//! A [`Store`] is the project's SQLite file (`.ols`, ADR-0004); each named
//! point cloud in it is held as **blocks** — one per square chunk of the
//! plan and per class — so a query for a few classes in a corridor reads
//! only those blocks. Coordinates are integers of the cloud's scale (1 mm
//! by default) relative to the chunk, compressed ([`codec`]).
//!
//! - [`Store::import_tiles`]: LAS/LAZ tiles → blocks (once).
//! - [`Store::visit`]: points by area and classes, blocks decompressed in
//!   parallel.
//! - [`PointSource`]: what every point reader offers — a stored cloud
//!   ([`CloudSource`]), LAS/LAZ tiles read directly ([`TileSource`]), and
//!   later plugins (e.g. a link to a point database).
//!
//! Schema (version 1):
//!
//! ```sql
//! ols_meta(key PRIMARY KEY, value)            -- 'pointcloud_schema' = '1'
//! pointcloud(id, name UNIQUE, crs, scale, chunk_size, class_scheme,
//!            points, xmin, ymin, zmin, xmax, ymax, zmax, created)
//! pointcloud_source(cloud, path, points, imported)
//! pointcloud_block(id, cloud, cx, cy, class, count, z0,
//!                  xyz, intensity, returns,  UNIQUE(cloud, cx, cy, class))
//! ```

pub mod codec;

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use ols_core::budget::Budget;
use ols_lidar::{Corridor, LidarPoint};
use rusqlite::{Connection, OptionalExtension, params};

use crate::codec::Columns;

/// Schema version of the point-cloud tables.
pub const SCHEMA_VERSION: u32 = 1;
/// Default chunk size, m: 64 m, a quarter of the default grid tile
/// (ADR-0008), so blocks nest in the tiles of the project's chunk grid.
pub const DEFAULT_CHUNK: f64 = 64.0;
/// Default coordinate scale, m (1 mm).
pub const DEFAULT_SCALE: f64 = 0.001;

/// Errors of the store.
#[derive(Debug, Clone, PartialEq)]
pub struct StoreError(pub String);

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "point store: {}", self.0)
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        Self(e.to_string())
    }
}

fn err(m: impl Into<String>) -> StoreError {
    StoreError(m.into())
}

/// A point cloud in the store.
#[derive(Debug, Clone, PartialEq)]
pub struct CloudInfo {
    /// Row id.
    pub id: i64,
    /// Name (unique in the project).
    pub name: String,
    /// Coordinate system (informative).
    pub crs: String,
    /// Coordinate scale, m.
    pub scale: f64,
    /// Chunk size, m.
    pub chunk_size: f64,
    /// Class scheme (`.olsc` text), if given at import.
    pub class_scheme: Option<String>,
    /// Number of points.
    pub points: u64,
    /// Bounds: minimum and maximum (x, y, z).
    pub bounds: [[f64; 3]; 2],
}

/// What an import or a query did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stats {
    /// Blocks (store) or tiles (LAS/LAZ) read.
    pub read: usize,
    /// Blocks or tiles skipped (outside the area or of other classes).
    pub skipped: usize,
    /// Points decoded.
    pub points_read: u64,
    /// Points passed to the visitor.
    pub points_visited: u64,
}

/// Points per grid tile ([`Store::tiles`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileCounts {
    /// Tiles holding points, with their point counts.
    pub tiles: Vec<(ols_core::grid::Tile, u64)>,
    /// Whether the cloud's chunks nest in the tiles (else each chunk
    /// counts in the tile of its centre).
    pub exact: bool,
}

/// Anything that delivers points by area and class: a stored cloud, LAS/LAZ
/// tiles, or a plugin (e.g. a point database).
pub trait PointSource {
    /// Description for reports.
    fn describe(&self) -> String;
    /// The class scheme of the points (`.olsc` text), if known.
    fn class_scheme(&self) -> Option<String>;
    /// Calls `visit(x, y, z, class)` for the points inside `area` (all
    /// without one) of the `classes` (all without them).
    fn visit(
        &self,
        area: Option<&Corridor>,
        classes: Option<&[u8]>,
        budget: &Budget,
        visit: &mut dyn FnMut(f64, f64, f64, u8),
    ) -> Result<Stats, String>;
}

/// The project file's point clouds.
pub struct Store {
    conn: Connection,
}

impl fmt::Debug for Store {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Store").finish_non_exhaustive()
    }
}

/// Takes a decoded point (with its class and its block's corridor) and
/// says whether it took it.
type BlockVisitor<'a> = dyn FnMut([f64; 3], u8, Option<&Corridor>) -> bool + 'a;

/// A chunk size made a whole number (at least one) of `scale` units: the
/// blocks' points are integers relative to their chunk, so every chunk
/// edge must fall on the scale's grid.
fn whole_chunk(chunk_size: f64, scale: f64) -> f64 {
    (chunk_size / scale).round().max(1.0) * scale
}

/// A block selected for reading: id, chunk x and y, class, count, z0, the
/// corridor near its chunk (none: every point).
type Selected = (
    i64,
    i64,
    i64,
    u8,
    usize,
    i64,
    Option<std::sync::Arc<Corridor>>,
);

/// A stored block: id, count, z0, coordinates, intensities, returns.
type StoredBlock = (i64, i64, i64, Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>);

/// Points of one block being imported (absolute integer coordinates).
#[derive(Default)]
struct Pending {
    x: Vec<i64>,
    y: Vec<i64>,
    z: Vec<i64>,
    intensity: Vec<u16>,
    returns: Vec<u8>,
}

impl Store {
    /// Opens (or creates) the project file and its point-cloud tables.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS ols_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS pointcloud (
                 id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, crs TEXT NOT NULL DEFAULT '',
                 scale REAL NOT NULL, chunk_size REAL NOT NULL, class_scheme TEXT,
                 points INTEGER NOT NULL DEFAULT 0,
                 xmin REAL, ymin REAL, zmin REAL, xmax REAL, ymax REAL, zmax REAL,
                 created TEXT NOT NULL DEFAULT (datetime('now')));
             CREATE TABLE IF NOT EXISTS pointcloud_source (
                 cloud INTEGER NOT NULL REFERENCES pointcloud(id) ON DELETE CASCADE,
                 path TEXT NOT NULL, points INTEGER NOT NULL,
                 imported TEXT NOT NULL DEFAULT (datetime('now')));
             CREATE TABLE IF NOT EXISTS pointcloud_block (
                 id INTEGER PRIMARY KEY,
                 cloud INTEGER NOT NULL REFERENCES pointcloud(id) ON DELETE CASCADE,
                 cx INTEGER NOT NULL, cy INTEGER NOT NULL, class INTEGER NOT NULL,
                 count INTEGER NOT NULL, z0 INTEGER NOT NULL,
                 xyz BLOB NOT NULL, intensity BLOB, returns BLOB,
                 UNIQUE (cloud, cx, cy, class));
             PRAGMA foreign_keys = ON;",
        )?;
        let version: Option<String> = conn
            .query_row(
                "SELECT value FROM ols_meta WHERE key = 'pointcloud_schema'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        match version.as_deref().map(str::parse::<u32>) {
            None => {
                conn.execute(
                    "INSERT INTO ols_meta (key, value) VALUES ('pointcloud_schema', ?1)",
                    params![SCHEMA_VERSION.to_string()],
                )?;
            }
            Some(Ok(v)) if v <= SCHEMA_VERSION => {}
            Some(_) => {
                return Err(err(format!(
                    "{}: point-cloud schema newer than supported ({SCHEMA_VERSION})",
                    path.display()
                )));
            }
        }
        Ok(Self { conn })
    }

    /// The point clouds in the project.
    pub fn clouds(&self) -> Result<Vec<CloudInfo>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, crs, scale, chunk_size, class_scheme, points,
                    xmin, ymin, zmin, xmax, ymax, zmax FROM pointcloud ORDER BY name",
        )?;
        let rows = stmt.query_map([], |r| {
            let opt = |k: usize| -> rusqlite::Result<f64> {
                Ok(r.get::<_, Option<f64>>(k)?.unwrap_or(f64::NAN))
            };
            let scale: f64 = r.get(3)?;
            Ok(CloudInfo {
                id: r.get(0)?,
                name: r.get(1)?,
                crs: r.get(2)?,
                scale,
                // The chunks are whole numbers of scale units (as the
                // blocks were binned), whatever was asked for.
                chunk_size: whole_chunk(r.get(4)?, scale),
                class_scheme: r.get(5)?,
                points: r.get::<_, i64>(6)? as u64,
                bounds: [[opt(7)?, opt(8)?, opt(9)?], [opt(10)?, opt(11)?, opt(12)?]],
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The cloud named `name`, or the only cloud when `name` is `None`.
    pub fn cloud(&self, name: Option<&str>) -> Result<CloudInfo, StoreError> {
        let clouds = self.clouds()?;
        match name {
            Some(n) => clouds
                .into_iter()
                .find(|c| c.name == n)
                .ok_or_else(|| err(format!("no point cloud \"{n}\""))),
            None if clouds.len() == 1 => Ok(clouds.into_iter().next().expect("one")),
            None if clouds.is_empty() => Err(err("the project has no point cloud")),
            None => Err(err(format!(
                "several point clouds ({}): choose one by name",
                clouds
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
    }

    /// Removes a point cloud and its blocks.
    pub fn delete_cloud(&self, name: &str) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM pointcloud WHERE name = ?1", params![name])?;
        Ok(())
    }

    /// Points per class of a cloud.
    pub fn class_counts(&self, cloud: &CloudInfo) -> Result<Vec<(u8, u64)>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT class, SUM(count) FROM pointcloud_block WHERE cloud = ?1 GROUP BY class ORDER BY class",
        )?;
        let rows = stmt.query_map(params![cloud.id], |r| {
            Ok((r.get::<_, i64>(0)? as u8, r.get::<_, i64>(1)? as u64))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Points per tile of `level` of the project's chunk grid — what a
    /// viewer or a server streams by (ADR-0008). Each block counts in the
    /// tile holding its chunk's centre; `exact` tells whether the chunks
    /// nest in the tiles (the tile width a whole multiple of the chunk
    /// size, origins on the chunk grid — true for the default 64 m chunks
    /// and tiles from 64 m up).
    pub fn tiles(
        &self,
        cloud: &CloudInfo,
        grid: &ols_core::grid::Grid,
        level: i8,
    ) -> Result<TileCounts, StoreError> {
        let c = cloud.chunk_size;
        let whole = |v: f64| (v - v.round()).abs() < 1e-9;
        let ratio = grid.tile_size(level) / c;
        let exact =
            ratio >= 1.0 && whole(ratio) && whole(grid.origin[0] / c) && whole(grid.origin[1] / c);
        let mut stmt = self.conn.prepare(
            "SELECT cx, cy, SUM(count) FROM pointcloud_block WHERE cloud = ?1 GROUP BY cx, cy",
        )?;
        let rows = stmt.query_map(params![cloud.id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        let mut counts: std::collections::BTreeMap<ols_core::grid::Tile, u64> =
            std::collections::BTreeMap::new();
        for row in rows {
            let (cx, cy, n) = row?;
            let centre = [(cx as f64 + 0.5) * c, (cy as f64 + 0.5) * c];
            *counts.entry(grid.tile_at(level, centre)).or_default() += n as u64;
        }
        Ok(TileCounts {
            tiles: counts.into_iter().collect(),
            exact,
        })
    }

    /// The tiles of `level` of the project's grid that the cloud's chunks
    /// overlap (a chunk across tile borders counts in each), with the
    /// lowest and highest block base height in each, m. What a viewer
    /// streams by: chunks of any size, nesting in the tiles or not.
    /// Heights are the blocks' bases: points lie at or above them.
    pub fn tile_extents(
        &self,
        cloud: &CloudInfo,
        grid: &ols_core::grid::Grid,
        level: i8,
    ) -> Result<Vec<(ols_core::grid::Tile, [f64; 2])>, StoreError> {
        let c = cloud.chunk_size;
        let mut stmt = self.conn.prepare(
            "SELECT cx, cy, MIN(z0), MAX(z0) FROM pointcloud_block WHERE cloud = ?1 GROUP BY cx, cy",
        )?;
        let rows = stmt.query_map(params![cloud.id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        let mut out: std::collections::BTreeMap<ols_core::grid::Tile, [f64; 2]> =
            std::collections::BTreeMap::new();
        for row in rows {
            let (cx, cy, z0, z1) = row?;
            let (z0, z1) = (z0 as f64 * cloud.scale, z1 as f64 * cloud.scale);
            // The chunk's square (its upper edges out).
            let area = ols_core::grid::Bounds::new(
                [cx as f64 * c, cy as f64 * c],
                [(cx + 1) as f64 * c - 1e-6, (cy + 1) as f64 * c - 1e-6],
            );
            for tile in grid.covering(&area, level).tiles() {
                let z = out.entry(tile).or_insert([z0, z1]);
                z[0] = z[0].min(z0);
                z[1] = z[1].max(z1);
            }
        }
        Ok(out.into_iter().collect())
    }

    /// Writes the points of `cloud` to a LAS file, or LAZ when `path` ends
    /// in `.laz`: LAS 1.4, point format 6 (classes 0–255), the cloud's
    /// coordinate scale, intensities and return numbers as imported.
    /// `classes`: only these (all without them); `area`: only points in this
    /// rectangle (min, max; x, y). `progress(points written, points to
    /// read)` after every block.
    ///
    /// Cost: decoding (~50 ns per point) and, for LAZ, compressing (~1 µs
    /// per point: 100 million points in about 2 min). Exit clause: `budget`,
    /// checked per block; the file is then incomplete and removed.
    pub fn export_las(
        &self,
        cloud: &CloudInfo,
        path: &Path,
        classes: Option<&[u8]>,
        area: Option<([f64; 2], [f64; 2])>,
        budget: &Budget,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<Stats, StoreError> {
        let run = budget.start();
        let mut wanted = [true; 256];
        if let Some(cs) = classes {
            wanted = [false; 256];
            for &c in cs {
                wanted[c as usize] = true;
            }
        }
        let c = cloud.chunk_size;
        let (cx, cy) = match area {
            Some((lo, hi)) => (
                [(lo[0] / c).floor() as i64, (hi[0] / c).floor() as i64],
                [(lo[1] / c).floor() as i64, (hi[1] / c).floor() as i64],
            ),
            None => ([i64::MIN, i64::MAX], [i64::MIN, i64::MAX]),
        };
        let mut stmt = self.conn.prepare(
            "SELECT id, cx, cy, class, count, z0 FROM pointcloud_block
             WHERE cloud = ?1 AND cx BETWEEN ?2 AND ?3 AND cy BETWEEN ?4 AND ?5
             ORDER BY cx, cy, class",
        )?;
        let blocks: Vec<(i64, i64, i64, u8, usize, i64)> = stmt
            .query_map(params![cloud.id, cx[0], cx[1], cy[0], cy[1]], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get::<_, i64>(3)? as u8,
                    r.get::<_, i64>(4)? as usize,
                    r.get(5)?,
                ))
            })?
            .collect::<Result<_, _>>()?;
        let blocks: Vec<_> = blocks
            .into_iter()
            .filter(|b| wanted[b.3 as usize])
            .collect();
        let total: u64 = blocks.iter().map(|b| b.4 as u64).sum();
        // Offsets: the lower corner of what is written, on the scale.
        let s = cloud.scale;
        let low = |k: usize| -> f64 {
            let v = match (area, k) {
                (Some((lo, _)), 0 | 1) => lo[k],
                _ => match k {
                    0 => blocks.iter().map(|b| b.1).min().unwrap_or(0) as f64 * c,
                    1 => blocks.iter().map(|b| b.2).min().unwrap_or(0) as f64 * c,
                    _ => blocks.iter().map(|b| b.5).min().unwrap_or(0) as f64 * s,
                },
            };
            (v / s).floor() * s
        };
        let mut builder = las::Builder::from((1, 4));
        builder.point_format = las::point::Format::new(6).map_err(|e| err(e.to_string()))?;
        builder.point_format.is_compressed = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("laz"));
        builder.generating_software = format!("OLS CAD {}", env!("CARGO_PKG_VERSION"));
        for (k, t) in [
            &mut builder.transforms.x,
            &mut builder.transforms.y,
            &mut builder.transforms.z,
        ]
        .into_iter()
        .enumerate()
        {
            t.scale = s;
            t.offset = low(k);
        }
        let header = builder.into_header().map_err(|e| err(e.to_string()))?;
        let mut writer = las::Writer::from_path(path, header)
            .map_err(|e| err(format!("{}: {e}", path.display())))?;
        let mut stats = Stats::default();
        let units = (c / s).round() as i64;
        let mut blob = self
            .conn
            .prepare("SELECT xyz, intensity, returns FROM pointcloud_block WHERE id = ?1")?;
        let result = (|| -> Result<(), StoreError> {
            for (id, cx, cy, class, n, z0) in &blocks {
                run.check()
                    .map_err(|stop| err(format!("stopped: {stop}")))?;
                let (xyz, intensity, returns): (Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>) =
                    blob.query_row(params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
                let (x, y, z) = Columns::decode_xyz(&xyz, *n).map_err(err)?;
                let intensity = match &intensity {
                    Some(b) => Columns::decode_intensity(b, *n).map_err(err)?,
                    None => vec![0; *n],
                };
                let returns = match &returns {
                    Some(b) => Columns::decode_returns(b, *n).map_err(err)?,
                    None => vec![0x11; *n],
                };
                // Class 12 is "overlap" to the las crate: written as code 12.
                let classification = las::point::Classification::new(*class)
                    .unwrap_or(las::point::Classification::Reserved(*class));
                stats.read += 1;
                for i in 0..*n {
                    let p = [
                        (cx * units + x[i] as i64) as f64 * s,
                        (cy * units + y[i] as i64) as f64 * s,
                        (z0 + z[i] as i64) as f64 * s,
                    ];
                    if let Some((lo, hi)) = area
                        && !(p[0] >= lo[0] && p[0] < hi[0] && p[1] >= lo[1] && p[1] < hi[1])
                    {
                        continue;
                    }
                    let r = returns[i];
                    writer
                        .write_point(las::Point {
                            x: p[0],
                            y: p[1],
                            z: p[2],
                            intensity: intensity[i],
                            return_number: (r >> 4).max(1),
                            number_of_returns: (r & 0x0f).max(1),
                            classification,
                            gps_time: Some(0.0),
                            ..Default::default()
                        })
                        .map_err(|e| err(e.to_string()))?;
                    stats.points_visited += 1;
                }
                stats.points_read += *n as u64;
                progress(stats.points_visited, total);
            }
            writer.close().map_err(|e| err(e.to_string()))?;
            Ok(())
        })();
        if let Err(e) = result {
            drop(writer);
            let _ = std::fs::remove_file(path);
            return Err(e);
        }
        Ok(stats)
    }

    /// Imports LAS/LAZ tiles into the cloud `name` (created if missing,
    /// with `crs`, `class_scheme` text, `chunk_size` and `scale`; an
    /// existing cloud keeps its settings and gets the points added).
    /// Points of a block already in the store are merged with it.
    ///
    /// Cost: dominated by LAZ decompression (~1 min per 10⁸ points); one
    /// transaction per tile. Exit clause: `budget`, checked per batch.
    #[allow(clippy::too_many_arguments)]
    pub fn import_tiles(
        &mut self,
        name: &str,
        tiles: &[PathBuf],
        crs: &str,
        class_scheme: Option<&str>,
        chunk_size: f64,
        scale: f64,
        budget: &Budget,
        mut progress: impl FnMut(usize, &Path, u64),
    ) -> Result<(CloudInfo, Stats), StoreError> {
        if !(chunk_size > 0.0 && scale > 0.0 && chunk_size / scale < u32::MAX as f64 / 2.0) {
            return Err(err(
                "chunk size and scale must be positive, chunk/scale < 2³¹",
            ));
        }
        let chunk_size = whole_chunk(chunk_size, scale);
        let existing = self.clouds()?.into_iter().find(|c| c.name == name);
        let cloud = match existing {
            Some(c) => c,
            None => {
                self.conn.execute(
                    "INSERT INTO pointcloud (name, crs, scale, chunk_size, class_scheme) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![name, crs, scale, chunk_size, class_scheme],
                )?;
                self.cloud(Some(name))?
            }
        };
        let units = (cloud.chunk_size / cloud.scale).round() as i64;
        let mut stats = Stats::default();
        for (k, tile) in tiles.iter().enumerate() {
            let mut pending: HashMap<(i64, i64, u8), Pending> = HashMap::new();
            let (_, _, n) = ols_lidar::for_each_record(
                std::slice::from_ref(tile),
                None,
                budget,
                |p: &LidarPoint| {
                    let q = |v: f64| (v / cloud.scale).round() as i64;
                    let (x, y, z) = (q(p.x), q(p.y), q(p.z));
                    let b = pending
                        .entry((x.div_euclid(units), y.div_euclid(units), p.class))
                        .or_default();
                    b.x.push(x);
                    b.y.push(y);
                    b.z.push(z);
                    b.intensity.push(p.intensity);
                    b.returns
                        .push((p.return_number.min(15) << 4) | p.number_of_returns.min(15));
                },
            )
            .map_err(|e| err(e.to_string()))?;
            let tx = self.conn.transaction()?;
            for ((cx, cy, class), mut b) in pending {
                // Merge with a block from an earlier tile (chunks across
                // tile edges).
                let old: Option<StoredBlock> = tx
                    .query_row(
                        "SELECT id, count, z0, xyz, intensity, returns FROM pointcloud_block
                         WHERE cloud = ?1 AND cx = ?2 AND cy = ?3 AND class = ?4",
                        params![cloud.id, cx, cy, class],
                        |r| {
                            Ok((
                                r.get(0)?,
                                r.get(1)?,
                                r.get(2)?,
                                r.get(3)?,
                                r.get(4)?,
                                r.get(5)?,
                            ))
                        },
                    )
                    .optional()?;
                if let Some((id, count, z0, xyz, intensity, returns)) = &old {
                    let n = *count as usize;
                    let (x, y, z) = Columns::decode_xyz(xyz, n).map_err(err)?;
                    let inten = match intensity {
                        Some(i) => Columns::decode_intensity(i, n).map_err(err)?,
                        None => vec![0; n],
                    };
                    let ret = match returns {
                        Some(r) => Columns::decode_returns(r, n).map_err(err)?,
                        None => vec![0; n],
                    };
                    for i in 0..n {
                        b.x.push(cx * units + x[i] as i64);
                        b.y.push(cy * units + y[i] as i64);
                        b.z.push(z0 + z[i] as i64);
                    }
                    b.intensity.extend(inten);
                    b.returns.extend(ret);
                    tx.execute("DELETE FROM pointcloud_block WHERE id = ?1", params![id])?;
                }
                let z0 = b.z.iter().copied().min().unwrap_or(0);
                let zmax = b.z.iter().copied().max().unwrap_or(0);
                if zmax - z0 > u32::MAX as i64 {
                    return Err(err("height range of a block exceeds the integer range"));
                }
                let mut columns = Columns {
                    x: b.x.iter().map(|&v| (v - cx * units) as u32).collect(),
                    y: b.y.iter().map(|&v| (v - cy * units) as u32).collect(),
                    z: b.z.iter().map(|&v| (v - z0) as u32).collect(),
                    intensity: b.intensity,
                    returns: b.returns,
                };
                let count = columns.len() as i64;
                let (xyz, intensity, returns) = columns.encode().map_err(err)?;
                tx.execute(
                    "INSERT INTO pointcloud_block (cloud, cx, cy, class, count, z0, xyz, intensity, returns)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![cloud.id, cx, cy, class, count, z0, xyz, intensity, returns],
                )?;
            }
            tx.execute(
                "INSERT INTO pointcloud_source (cloud, path, points) VALUES (?1, ?2, ?3)",
                params![cloud.id, tile.display().to_string(), n as i64],
            )?;
            tx.commit()?;
            stats.read += 1;
            stats.points_read += n;
            progress(k, tile, n);
        }
        self.update_summary(&cloud)?;
        Ok((self.cloud(Some(name))?, stats))
    }

    /// Recomputes a cloud's point count and bounds from its blocks.
    fn update_summary(&self, cloud: &CloudInfo) -> Result<(), StoreError> {
        let (s, c) = (cloud.scale, cloud.chunk_size);
        // Plan bounds from the chunks; heights from the block minima (the
        // maxima need decoding — taken while visiting is enough for now).
        self.conn.execute(
            "UPDATE pointcloud SET
                points = (SELECT COALESCE(SUM(count), 0) FROM pointcloud_block WHERE cloud = ?1),
                xmin = (SELECT MIN(cx) * ?2 FROM pointcloud_block WHERE cloud = ?1),
                ymin = (SELECT MIN(cy) * ?2 FROM pointcloud_block WHERE cloud = ?1),
                xmax = (SELECT (MAX(cx) + 1) * ?2 FROM pointcloud_block WHERE cloud = ?1),
                ymax = (SELECT (MAX(cy) + 1) * ?2 FROM pointcloud_block WHERE cloud = ?1),
                zmin = (SELECT MIN(z0) * ?3 FROM pointcloud_block WHERE cloud = ?1)
             WHERE id = ?1",
            params![cloud.id, c, s],
        )?;
        Ok(())
    }

    /// Calls `visit(x, y, z, class)` for the points of `cloud` inside
    /// `area` (whole cloud without one) of `classes` (all without them).
    /// Blocks outside the area or of other classes are not read; the
    /// others are decompressed in parallel, in batches.
    pub fn visit(
        &self,
        cloud: &CloudInfo,
        area: Option<&Corridor>,
        classes: Option<&[u8]>,
        budget: &Budget,
        visit: &mut dyn FnMut(f64, f64, f64, u8),
    ) -> Result<Stats, StoreError> {
        let run = budget.start();
        let mut wanted = [true; 256];
        if let Some(cs) = classes {
            wanted = [false; 256];
            for &c in cs {
                wanted[c as usize] = true;
            }
        }
        let c = cloud.chunk_size;
        // Block list without the data, only the chunks of the area's
        // bounds (a range on the block index: large clouds are not
        // scanned).
        let (cx, cy) = match area.and_then(Corridor::bounds) {
            Some((lo, hi)) => (
                [(lo[0] / c).floor() as i64, (hi[0] / c).floor() as i64],
                [(lo[1] / c).floor() as i64, (hi[1] / c).floor() as i64],
            ),
            None if area.is_some() => ([1, 0], [1, 0]), // empty corridor
            None => ([i64::MIN, i64::MAX], [i64::MIN, i64::MAX]),
        };
        let total: usize = self.conn.query_row(
            "SELECT COUNT(*) FROM pointcloud_block WHERE cloud = ?1",
            params![cloud.id],
            |r| r.get::<_, i64>(0),
        )? as usize;
        let mut stmt = self.conn.prepare(
            "SELECT id, cx, cy, class, count, z0 FROM pointcloud_block
             WHERE cloud = ?1 AND cx BETWEEN ?2 AND ?3 AND cy BETWEEN ?4 AND ?5",
        )?;
        let rows = stmt.query_map(params![cloud.id, cx[0], cx[1], cy[0], cy[1]], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)? as u8,
                r.get::<_, i64>(4)? as usize,
                r.get::<_, i64>(5)?,
            ))
        })?;
        let mut stats = Stats::default();
        let mut selected = Vec::new();
        // The corridor near each chunk (shared by its class blocks): the
        // per-point test then looks at a few segments only.
        let mut near: HashMap<(i64, i64), Option<std::sync::Arc<Corridor>>> = HashMap::new();
        for row in rows {
            let (id, cx, cy, class, count, z0) = row?;
            if !wanted[class as usize] {
                stats.skipped += 1;
                continue;
            }
            let local = match area {
                None => None,
                Some(a) => {
                    let entry = near.entry((cx, cy)).or_insert_with(|| {
                        let l = a.local(
                            [cx as f64 * c, cy as f64 * c],
                            [(cx + 1) as f64 * c, (cy + 1) as f64 * c],
                        );
                        (!l.is_empty()).then(|| std::sync::Arc::new(l))
                    });
                    match entry {
                        Some(l) => Some(l.clone()),
                        None => {
                            stats.skipped += 1;
                            continue;
                        }
                    }
                }
            };
            selected.push((id, cx, cy, class, count, z0, local));
        }
        // Blocks outside the area's chunk range were not even listed.
        stats.skipped = total - selected.len();
        self.decode_selected(
            cloud,
            &selected,
            &run,
            &mut stats,
            &mut |p, class, local| {
                local.is_none_or(|a| a.contains(p[0], p[1])) && {
                    visit(p[0], p[1], p[2], class);
                    true
                }
            },
        )?;
        Ok(stats)
    }

    /// Calls `visit(x, y, z, class)` for every point of `cloud` in the
    /// rectangle `min`–`max` (x, y; lower edges in, upper edges out) of
    /// `classes` (all without them) — what a viewer streams by tile: only
    /// the chunks of the rectangle are listed and read.
    ///
    /// Cost: one indexed query, then decoding the rectangle's blocks in
    /// parallel (~50 ns per point on 10 cores).
    pub fn visit_rect(
        &self,
        cloud: &CloudInfo,
        min: [f64; 2],
        max: [f64; 2],
        classes: Option<&[u8]>,
        budget: &Budget,
        visit: &mut dyn FnMut(f64, f64, f64, u8),
    ) -> Result<Stats, StoreError> {
        let run = budget.start();
        let mut wanted = [true; 256];
        if let Some(cs) = classes {
            wanted = [false; 256];
            for &c in cs {
                wanted[c as usize] = true;
            }
        }
        let c = cloud.chunk_size;
        let chunk = |v: f64| (v / c).floor() as i64;
        // The chunk holding a point just below the upper edge.
        let below = |v: f64| chunk(v - 1e-9 * c.max(1.0));
        let mut stmt = self.conn.prepare(
            "SELECT id, cx, cy, class, count, z0 FROM pointcloud_block
             WHERE cloud = ?1 AND cx BETWEEN ?2 AND ?3 AND cy BETWEEN ?4 AND ?5",
        )?;
        let rows = stmt.query_map(
            params![
                cloud.id,
                chunk(min[0]),
                below(max[0]),
                chunk(min[1]),
                below(max[1])
            ],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)? as u8,
                    r.get::<_, i64>(4)? as usize,
                    r.get::<_, i64>(5)?,
                ))
            },
        )?;
        let mut stats = Stats::default();
        let mut selected = Vec::new();
        for row in rows {
            let (id, cx, cy, class, count, z0) = row?;
            if wanted[class as usize] {
                selected.push((id, cx, cy, class, count, z0, None));
            } else {
                stats.skipped += 1;
            }
        }
        self.decode_selected(cloud, &selected, &run, &mut stats, &mut |p, class, _| {
            let inside = p[0] >= min[0] && p[0] < max[0] && p[1] >= min[1] && p[1] < max[1];
            if inside {
                visit(p[0], p[1], p[2], class);
            }
            inside
        })?;
        Ok(stats)
    }

    /// Decodes the blocks `selected` in parallel batches and hands their
    /// points to `visit(point, class, block's corridor)`, which says
    /// whether it took the point.
    fn decode_selected(
        &self,
        cloud: &CloudInfo,
        selected: &[Selected],
        run: &ols_core::budget::Run<'_>,
        stats: &mut Stats,
        visit: &mut BlockVisitor<'_>,
    ) -> Result<(), StoreError> {
        let units = (cloud.chunk_size / cloud.scale).round() as i64;
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        let mut blob_stmt = self
            .conn
            .prepare("SELECT xyz FROM pointcloud_block WHERE id = ?1")?;
        // Batches of about 4 million points: bounded memory, threads busy.
        let mut start = 0;
        while start < selected.len() {
            run.check()
                .map_err(|stop| err(format!("stopped: {stop}")))?;
            let mut end = start;
            let mut points = 0;
            while end < selected.len() && (points < 4_000_000 || end == start) {
                points += selected[end].4;
                end += 1;
            }
            let batch = &selected[start..end];
            let blobs: Vec<Vec<u8>> = batch
                .iter()
                .map(|b| blob_stmt.query_row(params![b.0], |r| r.get(0)))
                .collect::<Result<_, _>>()?;
            let per = batch.len().div_ceil(threads);
            let decoded: Vec<Result<Vec<[f64; 3]>, String>> = std::thread::scope(|scope| {
                let handles: Vec<_> = batch
                    .chunks(per)
                    .zip(blobs.chunks(per))
                    .map(|(metas, blobs)| {
                        scope.spawn(move || {
                            metas
                                .iter()
                                .zip(blobs)
                                .map(|((_, cx, cy, _, n, z0, _), blob)| {
                                    let (cx, cy, n, z0) = (*cx, *cy, *n, *z0);
                                    let (x, y, z) = Columns::decode_xyz(blob, n)?;
                                    Ok((0..n)
                                        .map(|i| {
                                            [
                                                (cx * units + x[i] as i64) as f64 * cloud.scale,
                                                (cy * units + y[i] as i64) as f64 * cloud.scale,
                                                (z0 + z[i] as i64) as f64 * cloud.scale,
                                            ]
                                        })
                                        .collect())
                                })
                                .collect::<Vec<_>>()
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .flat_map(|h| h.join().expect("decoder thread"))
                    .collect()
            });
            for (meta, points) in batch.iter().zip(decoded) {
                let points = points.map_err(err)?;
                stats.read += 1;
                stats.points_read += points.len() as u64;
                for p in points {
                    if visit(p, meta.3, meta.6.as_deref()) {
                        stats.points_visited += 1;
                    }
                }
            }
            start = end;
        }
        Ok(())
    }
}

/// A point cloud of a project file as a [`PointSource`].
pub struct CloudSource {
    store: Store,
    cloud: CloudInfo,
}

impl CloudSource {
    /// Opens the cloud `name` (or the only one) of a project file.
    pub fn open(path: &Path, name: Option<&str>) -> Result<Self, StoreError> {
        let store = Store::open(path)?;
        let cloud = store.cloud(name)?;
        Ok(Self { store, cloud })
    }

    /// The cloud.
    pub fn cloud(&self) -> &CloudInfo {
        &self.cloud
    }
}

impl PointSource for CloudSource {
    fn describe(&self) -> String {
        format!(
            "point cloud \"{}\" ({} points)",
            self.cloud.name, self.cloud.points
        )
    }

    fn class_scheme(&self) -> Option<String> {
        self.cloud.class_scheme.clone()
    }

    fn visit(
        &self,
        area: Option<&Corridor>,
        classes: Option<&[u8]>,
        budget: &Budget,
        visit: &mut dyn FnMut(f64, f64, f64, u8),
    ) -> Result<Stats, String> {
        self.store
            .visit(&self.cloud, area, classes, budget, visit)
            .map_err(|e| e.to_string())
    }
}

/// LAS/LAZ tiles read directly (every point is decompressed).
pub struct TileSource {
    /// Tiles.
    pub tiles: Vec<PathBuf>,
    /// Class scheme text, if known.
    pub scheme: Option<String>,
}

impl PointSource for TileSource {
    fn describe(&self) -> String {
        format!("{} LAS/LAZ tiles", self.tiles.len())
    }

    fn class_scheme(&self) -> Option<String> {
        self.scheme.clone()
    }

    fn visit(
        &self,
        area: Option<&Corridor>,
        classes: Option<&[u8]>,
        budget: &Budget,
        visit: &mut dyn FnMut(f64, f64, f64, u8),
    ) -> Result<Stats, String> {
        let mut wanted = [true; 256];
        if let Some(cs) = classes {
            wanted = [false; 256];
            for &c in cs {
                wanted[c as usize] = true;
            }
        }
        let mut visited = 0u64;
        let (read, skipped, n) =
            ols_lidar::for_each_point(&self.tiles, area, budget, |x, y, z, c| {
                if wanted[c as usize] {
                    visited += 1;
                    visit(x, y, z, c);
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Stats {
            read,
            skipped,
            points_read: n,
            points_visited: visited,
        })
    }
}

#[cfg(test)]
mod tests;
