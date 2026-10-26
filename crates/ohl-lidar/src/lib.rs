// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! LiDAR point clouds (LAS / LAZ) for terrain.
//!
//! - [`tile_info`]: header of a tile (point count, bounds) without reading
//!   points.
//! - [`read_ground`]: points of chosen classes (ASPRS class 2, ground, by
//!   default) from many tiles, kept only inside a [`Corridor`] around the
//!   line's routes, optionally thinned on a grid. Tiles that do not touch
//!   the corridor are not decompressed.
//! - [`build_terrain`]: the ground surface (TIN) from those points — the
//!   first-party terrain builder; other builders (plugins) produce the same
//!   output, a mesh for [`Terrain::from_mesh`].
//!
//! - [`classes`]: what class numbers mean ([`ClassScheme`], `*.ohlc`
//!   files): ground is chosen by role, so any classification works.
//! - [`classify`]: ground and height-above-ground classes for points not
//!   classified yet, and LAS/LAZ writing of the result.
//!
//! Cost: LAZ decompression dominates, roughly 5–10 million points per
//! second; triangulation about 1 s per million points kept. Exit clauses: a
//! [`Budget`] checked after every batch of points.

pub mod classes;
pub mod classify;

pub use classes::ClassScheme;

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use las::{PointDataBuilder, Reader};
use ohl_core::budget::Budget;
use ohl_terrain::{Terrain, TerrainError};

/// ASPRS class of ground points.
pub const GROUND: u8 = 2;

/// Errors reading LiDAR.
#[derive(Debug)]
pub enum LidarError {
    /// A file could not be read or decoded.
    File(PathBuf, String),
    /// The work was stopped by the budget (time limit or cancellation).
    Stopped(String),
    /// Building the surface failed.
    Terrain(TerrainError),
    /// Parameters that cannot be used.
    Invalid(String),
}

impl fmt::Display for LidarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(path, m) => write!(f, "{}: {m}", path.display()),
            Self::Stopped(m) => write!(f, "LiDAR reading stopped: {m}"),
            Self::Terrain(e) => write!(f, "{e}"),
            Self::Invalid(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for LidarError {}

/// Header information of a tile.
#[derive(Debug, Clone, PartialEq)]
pub struct TileInfo {
    /// File.
    pub path: PathBuf,
    /// Number of points.
    pub points: u64,
    /// Bounds: minimum and maximum (x, y, z).
    pub bounds: [[f64; 3]; 2],
}

/// Reads a tile's header.
pub fn tile_info(path: &Path) -> Result<TileInfo, LidarError> {
    let reader =
        Reader::from_path(path).map_err(|e| LidarError::File(path.into(), e.to_string()))?;
    let header = reader.header();
    let b = header.bounds();
    Ok(TileInfo {
        path: path.into(),
        points: header.number_of_points(),
        bounds: [[b.min.x, b.min.y, b.min.z], [b.max.x, b.max.y, b.max.z]],
    })
}

/// LAS / LAZ files in a directory (sorted), or the file itself.
pub fn tiles_in(path: &Path) -> Result<Vec<PathBuf>, LidarError> {
    if path.is_file() {
        return Ok(vec![path.into()]);
    }
    let entries =
        std::fs::read_dir(path).map_err(|e| LidarError::File(path.into(), e.to_string()))?;
    let mut tiles: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("las") || e.eq_ignore_ascii_case("laz"))
        })
        .collect();
    tiles.sort();
    Ok(tiles)
}

/// The area within `half_width` (horizontally) of polylines, e.g. a line's
/// route alignments or its spans. Segments are indexed on a plan grid, so
/// queries look only at the segments near them (a line's spans as dense
/// wire polylines are tens of thousands of segments).
#[derive(Debug, Clone, PartialEq)]
pub struct Corridor {
    segments: Vec<[[f64; 2]; 2]>,
    half_width: f64,
    /// Grid cell size, m.
    cell: f64,
    /// Segments within `half_width` of each cell (by bounding boxes).
    cells: HashMap<(i64, i64), Vec<u32>>,
}

impl Corridor {
    /// Corridor around the chords of a line's spans (attachment to
    /// attachment, in plan).
    pub fn along_spans(line: &ohl_line::Line, half_width: f64) -> Self {
        let polylines: Vec<Vec<[f64; 2]>> = line
            .sections
            .iter()
            .map(|s| {
                s.points
                    .iter()
                    .filter_map(|p| line.wire_point(p))
                    .map(|w| [w.position[0], w.position[1]])
                    .collect()
            })
            .collect();
        Self::new(&polylines, half_width)
    }

    /// Corridor around polylines (x, y).
    pub fn new(polylines: &[Vec<[f64; 2]>], half_width: f64) -> Self {
        let segments: Vec<[[f64; 2]; 2]> = polylines
            .iter()
            .flat_map(|l| l.windows(2).map(|w| [w[0], w[1]]))
            .collect();
        Self::from_segments(segments, half_width)
    }

    fn from_segments(segments: Vec<[[f64; 2]; 2]>, half_width: f64) -> Self {
        let cell = (2.0 * half_width).max(25.0);
        let mut cells: HashMap<(i64, i64), Vec<u32>> = HashMap::new();
        for (k, s) in segments.iter().enumerate() {
            let key = |v: f64| (v / cell).floor() as i64;
            let (x0, x1) = (
                s[0][0].min(s[1][0]) - half_width,
                s[0][0].max(s[1][0]) + half_width,
            );
            let (y0, y1) = (
                s[0][1].min(s[1][1]) - half_width,
                s[0][1].max(s[1][1]) + half_width,
            );
            for i in key(x0)..=key(x1) {
                for j in key(y0)..=key(y1) {
                    cells.entry((i, j)).or_default().push(k as u32);
                }
            }
        }
        Self {
            segments,
            half_width,
            cell,
            cells,
        }
    }

    /// Whether (x, y) is inside.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        let key = (
            (x / self.cell).floor() as i64,
            (y / self.cell).floor() as i64,
        );
        self.cells.get(&key).is_some_and(|ids| {
            ids.iter().any(|&k| {
                let [a, b] = self.segments[k as usize];
                ohl_core::geometry::distance_to_segment([x, y], a, b) <= self.half_width
            })
        })
    }

    /// Plan bounds of the corridor (`min`, `max`); `None` without
    /// segments.
    pub fn bounds(&self) -> Option<([f64; 2], [f64; 2])> {
        let r = self.half_width;
        self.segments.iter().flatten().fold(None, |b, p| {
            let (lo, hi) = b.unwrap_or(([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]));
            Some((
                [lo[0].min(p[0] - r), lo[1].min(p[1] - r)],
                [hi[0].max(p[0] + r), hi[1].max(p[1] + r)],
            ))
        })
    }

    /// Whether the corridor may reach into the plan box `min`–`max`
    /// (conservative: compares bounding boxes).
    pub fn touches(&self, min: [f64; 2], max: [f64; 2]) -> bool {
        !self.near_box(min, max).is_empty()
    }

    /// Ids of the segments that may reach into the box (sorted, unique).
    fn near_box(&self, min: [f64; 2], max: [f64; 2]) -> Vec<u32> {
        let key = |v: f64| (v / self.cell).floor() as i64;
        let r = self.half_width;
        let mut ids: Vec<u32> = Vec::new();
        for i in key(min[0])..=key(max[0]) {
            for j in key(min[1])..=key(max[1]) {
                if let Some(cell) = self.cells.get(&(i, j)) {
                    ids.extend(cell.iter().copied().filter(|&k| {
                        let s = &self.segments[k as usize];
                        s[0][0].min(s[1][0]) - r <= max[0]
                            && s[0][0].max(s[1][0]) + r >= min[0]
                            && s[0][1].min(s[1][1]) - r <= max[1]
                            && s[0][1].max(s[1][1]) + r >= min[1]
                    }));
                }
            }
        }
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// The part of the corridor that may reach into the plan box
    /// `min`–`max`: same width, only the segments near the box (empty if
    /// none). Queries on it are cheaper.
    pub fn local(&self, min: [f64; 2], max: [f64; 2]) -> Self {
        let segments = self
            .near_box(min, max)
            .into_iter()
            .map(|k| self.segments[k as usize])
            .collect();
        Self::from_segments(segments, self.half_width)
    }

    /// Whether there is no segment.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }
}

/// A point cloud named as `FILE` or `FILE#cloud-name`: the project file
/// and the cloud's name.
pub fn cloud_spec(spec: &str) -> (&Path, Option<&str>) {
    match spec.split_once('#') {
        Some((file, name)) => (Path::new(file), Some(name)),
        None => (Path::new(spec), None),
    }
}

/// Ground points: the lowest in each square plan cell, or every point.
#[derive(Debug, Clone, Default)]
pub struct LowestPerCell {
    cell: Option<f64>,
    cells: HashMap<(i64, i64), usize>,
    points: Vec<[f64; 3]>,
}

impl LowestPerCell {
    /// Keeps the lowest point per `cell` m square (`None`: every point).
    pub fn new(cell: Option<f64>) -> Self {
        Self {
            cell,
            ..Self::default()
        }
    }

    /// Takes one point into account.
    pub fn add(&mut self, x: f64, y: f64, z: f64) {
        let Some(cell) = self.cell else {
            self.points.push([x, y, z]);
            return;
        };
        let key = ((x / cell).floor() as i64, (y / cell).floor() as i64);
        match self.cells.get(&key) {
            Some(&k) if self.points[k][2] <= z => {}
            Some(&k) => self.points[k] = [x, y, z],
            None => {
                self.cells.insert(key, self.points.len());
                self.points.push([x, y, z]);
            }
        }
    }

    /// The points kept.
    pub fn points(&self) -> &[[f64; 3]] {
        &self.points
    }

    /// Their Delaunay TIN, without triangle sides longer than `max_edge`
    /// (no ground invented across gaps).
    pub fn terrain(&self, max_edge: Option<f64>) -> Result<Terrain, TerrainError> {
        Terrain::from_xyz(&self.points, max_edge)
    }
}

/// What to keep from the point clouds.
#[derive(Debug, Clone, PartialEq)]
pub struct GroundFilter {
    /// ASPRS classes to keep (default: ground).
    pub classes: Vec<u8>,
    /// Keep only points inside this corridor.
    pub corridor: Option<Corridor>,
    /// Thinning: keep the lowest point in each square cell of this size, m.
    pub thin: Option<f64>,
}

impl Default for GroundFilter {
    fn default() -> Self {
        Self {
            classes: vec![GROUND],
            corridor: None,
            thin: None,
        }
    }
}

/// What [`read_ground`] did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReadStats {
    /// Tiles decompressed.
    pub tiles_read: usize,
    /// Tiles skipped (outside the corridor).
    pub tiles_skipped: usize,
    /// Points decompressed.
    pub points_read: u64,
    /// Points of each class (index = class number) inside the corridor.
    pub classes: Vec<u64>,
    /// Points kept (after corridor and thinning).
    pub points_kept: usize,
}

/// One point with the attributes kept by OpenOHL.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LidarPoint {
    /// Easting, m.
    pub x: f64,
    /// Northing, m.
    pub y: f64,
    /// Elevation, m.
    pub z: f64,
    /// Class number.
    pub class: u8,
    /// Intensity.
    pub intensity: u16,
    /// Return number (1-based).
    pub return_number: u8,
    /// Number of returns of the pulse.
    pub number_of_returns: u8,
}

/// How far, m, a tile may lie from the nearest structure before reading it
/// warns (it is read anyway: a substation at a line's end, another
/// structure worth having).
pub const FAR_FROM_LINE: f64 = 2000.0;

/// The tiles lying more than `limit` (plan, m) from every point of `near`
/// (e.g. the line's structures), with that distance, by their headers'
/// bounds; none without points. A warning only: such tiles are read too.
pub fn far_tiles(
    tiles: &[PathBuf],
    near: &[[f64; 2]],
    limit: f64,
) -> Result<Vec<(PathBuf, f64)>, LidarError> {
    if near.is_empty() {
        return Ok(Vec::new());
    }
    let mut far = Vec::new();
    for path in tiles {
        let reader =
            Reader::from_path(path).map_err(|e| LidarError::File(path.clone(), e.to_string()))?;
        let b = reader.header().bounds();
        let d = near
            .iter()
            .map(|p| {
                let dx = (b.min.x - p[0]).max(p[0] - b.max.x).max(0.0);
                let dy = (b.min.y - p[1]).max(p[1] - b.max.y).max(0.0);
                dx.hypot(dy)
            })
            .fold(f64::INFINITY, f64::min);
        if d > limit {
            far.push((path.clone(), d));
        }
    }
    Ok(far)
}

/// One line per tile of [`far_tiles`] from a line's structures.
pub fn far_tile_warnings(tiles: &[PathBuf], line: &ohl_line::Line) -> Vec<String> {
    let near: Vec<[f64; 2]> = line
        .structures
        .iter()
        .map(|s| [s.position[0], s.position[1]])
        .collect();
    match far_tiles(tiles, &near, FAR_FROM_LINE) {
        Ok(far) => far
            .into_iter()
            .map(|(p, d)| {
                format!(
                    "{}: {:.1} km from the nearest structure of the line — read anyway \
                     (check it belongs to this project)",
                    p.file_name()
                        .map_or(String::new(), |f| f.to_string_lossy().into_owned()),
                    d / 1000.0
                )
            })
            .collect(),
        Err(e) => vec![e.to_string()],
    }
}

/// Like [`for_each_point`], with every attribute kept by OpenOHL.
pub fn for_each_record(
    tiles: &[PathBuf],
    corridor: Option<&Corridor>,
    budget: &Budget,
    mut visit: impl FnMut(&LidarPoint),
) -> Result<(usize, usize, u64), LidarError> {
    let run = budget.start();
    let (mut read, skipped, mut points) = (0, 0, 0u64);
    // The tiles decoded on other threads (2026-10-26), each tile's points
    // handed over in batches through a channel of its own, visited here
    // tile after tile in their order: the visitor sees the points in the
    // order of a single reader. The decoding overlaps the visiting (and
    // the next tiles' first batches are ready); a tile is not decoded far
    // ahead (the memory stays bounded: [`TILE_QUEUE`] batches a tile, at
    // most [`TILE_THREADS`] tiles at once).
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(TILE_THREADS)
        .min(tiles.len())
        .max(1);
    let stop = std::sync::atomic::AtomicBool::new(false);
    let next = std::sync::atomic::AtomicUsize::new(0);
    // Each tile's sender is taken by the thread reading it: dropped when
    // it is done, which closes the tile's channel.
    let (senders, receivers): (Vec<_>, Vec<_>) = tiles
        .iter()
        .map(|_| {
            let (tx, rx) = std::sync::mpsc::sync_channel::<TileBatch>(TILE_QUEUE);
            (std::sync::Mutex::new(Some(tx)), rx)
        })
        .unzip();
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(path) = tiles.get(k) else {
                        return;
                    };
                    let tx = senders[k].lock().ok().and_then(|mut s| s.take());
                    if let Some(tx) = tx {
                        decode_tile(path, corridor, &stop, &tx);
                    }
                }
            });
        }
        let result = (|| {
            for (path, rx) in tiles.iter().zip(&receivers) {
                let mut opened = false;
                // Ends when the tile's reader is done (its channel closed).
                while let Ok(batch) = rx.recv() {
                    run.check()
                        .map_err(|stop| LidarError::Stopped(stop.to_string()))?;
                    match batch {
                        TileBatch::Opened => {
                            // Every tile is read: its header's bounds are not
                            // trusted (a program may leave them wrong), so a
                            // tile is never skipped on them.
                            opened = true;
                            read += 1;
                        }
                        TileBatch::Read(n) => points += n,
                        TileBatch::Points(batch) => batch.iter().for_each(&mut visit),
                        TileBatch::Failed(e) => return Err(LidarError::File(path.clone(), e)),
                    }
                }
                if !opened {
                    // Its reader stopped before opening it (another failed).
                    return Err(LidarError::Stopped("reading stopped".into()));
                }
            }
            Ok(())
        })();
        // Stops the readers (a failure, a stop): their sends fail too.
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        drop(receivers);
        result
    })?;
    Ok((read, skipped, points))
}

/// What a tile's reader hands over, in order.
enum TileBatch {
    /// The tile opened.
    Opened,
    /// Points read (with those passed over).
    Read(u64),
    /// Points kept, in the file's order.
    Points(Vec<LidarPoint>),
    /// It failed: why (the points before were handed over).
    Failed(String),
}

/// Batches waiting per tile (each [`TILE_BATCH`] points): bounds the
/// memory while tiles are decoded ahead of the visitor.
const TILE_QUEUE: usize = 4;

/// Points per batch of a tile decoded on its own thread.
const TILE_BATCH: u64 = 1 << 16;

/// Tiles decoded at once ([`for_each_record`]).
const TILE_THREADS: usize = 4;

/// Decodes the tile at `path` into `tx` ([`for_each_record`]) until done,
/// failed, `stop` or the channel is closed.
fn decode_tile(
    path: &Path,
    corridor: Option<&Corridor>,
    stop: &std::sync::atomic::AtomicBool,
    tx: &std::sync::mpsc::SyncSender<TileBatch>,
) {
    let failed = |e: las::Error| {
        let _ = tx.send(TileBatch::Failed(e.to_string()));
    };
    if stop.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let mut reader = match Reader::from_path(path) {
        Ok(r) => r,
        Err(e) => return failed(e),
    };
    if tx.send(TileBatch::Opened).is_err() {
        return;
    }
    let header = reader.header().clone();
    let mut data = PointDataBuilder::new().for_header(&header).build();
    loop {
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let n = match reader.fill_points(TILE_BATCH, &mut data) {
            Ok(0) => return,
            Ok(n) => n,
            Err(e) => return failed(e),
        };
        if tx.send(TileBatch::Read(n)).is_err() {
            return;
        }
        let mut kept = Vec::with_capacity(n as usize);
        for point in data.points() {
            let point = match point {
                Ok(p) => p,
                Err(e) => {
                    // The points before it first, as one reader visits them.
                    let _ = tx.send(TileBatch::Points(kept));
                    return failed(e);
                }
            };
            // Withheld: deleted by whoever made the survey.
            if point.is_withheld || corridor.is_some_and(|c| !c.contains(point.x, point.y)) {
                continue;
            }
            kept.push(LidarPoint {
                x: point.x,
                y: point.y,
                z: point.z,
                class: u8::from(point.classification),
                intensity: point.intensity,
                return_number: point.return_number,
                number_of_returns: point.number_of_returns,
            });
        }
        if tx.send(TileBatch::Points(kept)).is_err() {
            return;
        }
    }
}

/// Calls `visit(x, y, z, class)` for every point of `tiles` inside the
/// corridor (all points without one), withheld points left out. Every tile
/// is read whole (its header's bounds are not trusted). Returns the tiles
/// read and skipped (none), and the points read.
pub fn for_each_point(
    tiles: &[PathBuf],
    corridor: Option<&Corridor>,
    budget: &Budget,
    mut visit: impl FnMut(f64, f64, f64, u8),
) -> Result<(usize, usize, u64), LidarError> {
    for_each_record(tiles, corridor, budget, |p| visit(p.x, p.y, p.z, p.class))
}

/// Reads the points of `filter.classes` from `tiles`, inside the corridor,
/// thinned if asked. `stats.classes` counts the points of every class
/// inside the corridor.
pub fn read_ground(
    tiles: &[PathBuf],
    filter: &GroundFilter,
    budget: &Budget,
) -> Result<(Vec<[f64; 3]>, ReadStats), LidarError> {
    let mut keep_class = [false; 256];
    for &c in &filter.classes {
        keep_class[c as usize] = true;
    }
    let mut classes = vec![0u64; 256];
    let mut kept = LowestPerCell::new(filter.thin);
    let (tiles_read, tiles_skipped, points_read) =
        for_each_point(tiles, filter.corridor.as_ref(), budget, |x, y, z, class| {
            classes[class as usize] += 1;
            if keep_class[class as usize] {
                kept.add(x, y, z);
            }
        })?;
    let points = kept.points;
    let stats = ReadStats {
        tiles_read,
        tiles_skipped,
        points_read,
        classes,
        points_kept: points.len(),
    };
    Ok((points, stats))
}

/// Builds the ground surface from LiDAR: [`read_ground`], then a Delaunay
/// TIN without triangles longer than `max_edge` (no ground is invented
/// across gaps in the data).
pub fn build_terrain(
    tiles: &[PathBuf],
    filter: &GroundFilter,
    max_edge: Option<f64>,
    budget: &Budget,
) -> Result<(Terrain, ReadStats), LidarError> {
    let (points, stats) = read_ground(tiles, filter, budget)?;
    let terrain = Terrain::from_xyz(&points, max_edge).map_err(LidarError::Terrain)?;
    Ok((terrain, stats))
}

#[cfg(test)]
mod tests {
    use super::*;
    use las::point::Classification;
    use las::{Builder, Point, Writer};

    #[test]
    fn lowest_point_per_cell_and_cloud_names() {
        let mut k = LowestPerCell::new(Some(2.0));
        for (x, y, z) in [
            (0.5, 0.5, 3.0),
            (1.5, 1.0, 2.0),
            (1.9, 1.9, 2.5),
            (2.5, 0.0, 9.0),
        ] {
            k.add(x, y, z);
        }
        assert_eq!(k.points(), [[1.5, 1.0, 2.0], [2.5, 0.0, 9.0]]);
        let mut all = LowestPerCell::new(None);
        all.add(0.0, 0.0, 1.0);
        all.add(0.0, 0.0, 0.0);
        assert_eq!(all.points().len(), 2);
        assert_eq!(cloud_spec("p.ohl"), (Path::new("p.ohl"), None));
        assert_eq!(
            cloud_spec("p.ohl#scan 2"),
            (Path::new("p.ohl"), Some("scan 2"))
        );
    }

    /// Writes a small LAZ tile: a 1 m grid of ground points on a plane and,
    /// every 5th point, a vegetation point 10 m above it.
    fn write_tile(path: &Path, x0: f64, y0: f64, n: usize) {
        let mut builder = Builder::from((1, 4));
        builder.point_format.is_compressed = true;
        let header = builder.into_header().unwrap();
        let mut writer = Writer::from_path(path, header).unwrap();
        for i in 0..n {
            for j in 0..n {
                let (x, y) = (x0 + i as f64, y0 + j as f64);
                let z = 400.0 + 0.1 * x;
                writer
                    .write_point(Point {
                        x,
                        y,
                        z,
                        classification: Classification::Ground,
                        ..Default::default()
                    })
                    .unwrap();
                if (i + j) % 5 == 0 {
                    writer
                        .write_point(Point {
                            x: x + 0.5,
                            y: y + 0.5,
                            z: z + 10.0,
                            classification: Classification::HighVegetation,
                            ..Default::default()
                        })
                        .unwrap();
                }
            }
        }
        writer.close().unwrap();
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ohl-lidar-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Tiles decoded side by side (2026-10-26) are visited as one reader
    /// visits them: tile after tile, each in its file's order (more points
    /// than a batch); a tile that does not read stops the visit after the
    /// tiles before it.
    #[test]
    fn tiles_read_side_by_side_are_visited_in_order() {
        let dir = scratch("order");
        let mut tiles = Vec::new();
        for k in 0..6 {
            let path = dir.join(format!("t{k}.laz"));
            // 300 × 300 ground points and a fifth of them again: more than
            // one batch of a tile.
            write_tile(&path, 1000.0 * k as f64, 0.0, if k == 2 { 300 } else { 40 });
            tiles.push(path);
        }
        let mut seen: Vec<(f64, f64, f64)> = Vec::new();
        let (read, skipped, points) =
            for_each_record(&tiles, None, &Budget::new(), |p| seen.push((p.x, p.y, p.z))).unwrap();
        assert_eq!((read, skipped), (6, 0));
        assert_eq!(points, seen.len() as u64);
        // The order of the files, one after another.
        let mut expected = Vec::new();
        for path in &tiles {
            let mut reader = Reader::from_path(path).unwrap();
            let all = reader.read_points(u64::MAX).unwrap();
            for p in all.points() {
                let p = p.unwrap();
                expected.push((p.x, p.y, p.z));
            }
        }
        assert!(expected.len() as u64 > TILE_BATCH);
        assert_eq!(seen, expected);
        // A tile that is no LAS file: the tiles before it visited, then the
        // error naming it.
        let bad = dir.join("t3.laz");
        std::fs::write(&bad, b"not a point cloud").unwrap();
        let mut n = 0usize;
        let e = for_each_record(&tiles, None, &Budget::new(), |_| n += 1).unwrap_err();
        assert!(matches!(&e, LidarError::File(p, _) if *p == bad), "{e}");
        let before: usize = tiles[..3]
            .iter()
            .map(|p| tile_info(p).unwrap().points as usize)
            .sum();
        assert_eq!(n, before);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn ground_only_inside_the_corridor_every_tile_read() {
        let dir = scratch("corridor");
        write_tile(&dir.join("a.laz"), 0.0, 0.0, 50);
        write_tile(&dir.join("b.laz"), 1000.0, 0.0, 50);
        let tiles = tiles_in(&dir).unwrap();
        assert_eq!(tiles.len(), 2);
        assert_eq!(tile_info(&tiles[0]).unwrap().points, 2500 + 500);
        // A corridor 5 m either side of y = 25 across tile a only.
        let filter = GroundFilter {
            corridor: Some(Corridor::new(&[vec![[-10.0, 25.0], [60.0, 25.0]]], 5.0)),
            ..GroundFilter::default()
        };
        let (points, stats) = read_ground(&tiles, &filter, &Budget::new()).unwrap();
        // Every tile read (headers' bounds are not trusted), only the
        // points inside kept.
        assert_eq!((stats.tiles_read, stats.tiles_skipped), (2, 0));
        // Counts inside the corridor: 11 ground rows; vegetation (offset
        // +0.5 m) on rows y = 20..=29, every 5th point: 10 × 10.
        assert_eq!(stats.classes[GROUND as usize], 550);
        assert_eq!(stats.classes[5], 100);
        // Rows y = 20..=30: 11 rows of 50 ground points.
        assert_eq!(points.len(), 11 * 50);
        assert!(points.iter().all(|p| (p[1] - 25.0).abs() <= 5.0));
        // Vegetation is not ground: the surface is the plane.
        let (terrain, _) = build_terrain(&tiles, &filter, Some(3.0), &Budget::new()).unwrap();
        assert!((terrain.height(30.3, 24.6).unwrap() - (400.0 + 3.03)).abs() < 1e-6);
        assert_eq!(terrain.height(30.0, 40.0), None);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn withheld_points_are_left_out_and_far_tiles_said() {
        let dir = scratch("withheld");
        let path = dir.join("w.laz");
        let mut builder = Builder::from((1, 4));
        builder.point_format.is_compressed = true;
        let mut writer = Writer::from_path(&path, builder.into_header().unwrap()).unwrap();
        for (k, withheld) in [false, true, false].into_iter().enumerate() {
            writer
                .write_point(Point {
                    x: k as f64,
                    y: 0.0,
                    z: 400.0,
                    is_withheld: withheld,
                    classification: Classification::Ground,
                    ..Default::default()
                })
                .unwrap();
        }
        writer.close().unwrap();
        let tiles = vec![path];
        let mut xs = Vec::new();
        for_each_point(&tiles, None, &Budget::new(), |x, _, _, _| xs.push(x)).unwrap();
        assert_eq!(xs, [0.0, 2.0]);
        // 5 km from the only structure: said so (it is read anyway).
        let far = far_tiles(&tiles, &[[5000.0, 0.0]], FAR_FROM_LINE).unwrap();
        assert_eq!(far.len(), 1);
        assert!((far[0].1 - 4998.0).abs() < 1e-9, "{}", far[0].1);
        assert!(
            far_tiles(&tiles, &[[100.0, 50.0]], FAR_FROM_LINE)
                .unwrap()
                .is_empty()
        );
        assert!(far_tiles(&tiles, &[], FAR_FROM_LINE).unwrap().is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn thinning_keeps_the_lowest_point_per_cell() {
        let dir = scratch("thin");
        write_tile(&dir.join("a.laz"), 0.0, 0.0, 20);
        let filter = GroundFilter {
            thin: Some(4.0),
            classes: vec![GROUND, 5],
            ..GroundFilter::default()
        };
        let (points, _) = read_ground(&tiles_in(&dir).unwrap(), &filter, &Budget::new()).unwrap();
        assert_eq!(points.len(), 25);
        // Lowest per 4 m cell: the ground point at the cell's low-x edge.
        for p in &points {
            assert!(
                (p[2] - (400.0 + 0.1 * p[0])).abs() < 1e-6 && p[0] % 4.0 == 0.0,
                "{p:?}"
            );
        }
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn time_limit_stops_reading() {
        let dir = scratch("budget");
        write_tile(&dir.join("a.laz"), 0.0, 0.0, 10);
        let budget = Budget::new().time_limit(std::time::Duration::ZERO);
        let err =
            read_ground(&tiles_in(&dir).unwrap(), &GroundFilter::default(), &budget).unwrap_err();
        assert!(matches!(err, LidarError::Stopped(_)), "{err}");
        std::fs::remove_dir_all(dir).ok();
    }
}
