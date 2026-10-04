// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The 3D view's layers in chunks (ADR-0006, ADR-0008): what of a layer
//! lies in each 256 m tile of the project grid, and its geometry at a
//! level of detail ([`crate::lod`]).
//!
//! - **Terrain**: level 0 the TIN's triangles (by centroid); levels 1 and
//!   2 a regular grid of the surface's heights at the row's spacing.
//! - **LiDAR**: level 0 every point shown; levels 1 and 2 a share of them
//!   (every n-th, the same points every time).
//! - **Structures, wires, other cases, project lines**: the layers'
//!   primitives ([`crate::scene::Prim`]) by anchor, each drawn at the
//!   levels it belongs to; wires thinned at levels 1 and 2.
//!
//! An index is made when its layer changes (an edit, another weather
//! case); chunks are built from it when the view needs them.

use std::collections::HashMap;
use std::sync::Arc;

use ols_core::grid::{Grid, Tile};
use ols_render::{Bounds, Scene};

use crate::lod::{DetailKind, LodRow};
use crate::model::Model;
use crate::scene::{self, Layer, Prim};

/// How far above its block's base height a LiDAR point may lie, m (a
/// chunk's slope and the tallest pylons and trees; a box too tall only
/// culls a little less).
const POINTS_ABOVE_BASE: f64 = 300.0;

/// What a chunk holds.
#[derive(Debug)]
enum Content {
    /// Primitives.
    Prims(Vec<Prim>),
    /// TIN triangles (indices into the index's mesh).
    Triangles(Vec<u32>),
    /// Triangles of other surfaces: (surface, triangle indices).
    Others(Vec<(usize, Vec<u32>)>),
    /// LiDAR points (indices into the model's points).
    Points(Vec<u32>),
    /// LiDAR points read from the project's point cloud when needed
    /// ([`PointStream`]).
    Stored,
}

/// A TIN: points and triangles (indices into the points).
type Mesh = (Vec<[f64; 3]>, Vec<[usize; 3]>);

/// A layer split into the tiles of the grid.
#[derive(Debug)]
pub struct ChunkIndex {
    grid: Grid,
    chunks: HashMap<Tile, (Bounds, Content)>,
    /// The TIN, for terrain chunks.
    mesh: Option<Arc<Mesh>>,
}

fn grow(b: &mut Option<Bounds>, p: [f64; 3]) {
    match b {
        Some(b) => {
            for (k, v) in p.into_iter().enumerate() {
                b.min[k] = b.min[k].min(v);
                b.max[k] = b.max[k].max(v);
            }
        }
        None => *b = Some(Bounds { min: p, max: p }),
    }
}

impl ChunkIndex {
    /// The chunks of `layer` (one of [`crate::lod::CHUNKED`]) for weather
    /// case `case`.
    ///
    /// Expected cost: one pass over the layer's content (5 million TIN
    /// triangles: ~0.5 s in release; wires: as drawing them).
    pub fn new(model: &Model, layer: Layer, case: usize) -> Self {
        let grid = Grid::default();
        let tile = |p: [f64; 3]| grid.tile_at(0, [p[0], p[1]]);
        let mut bounds: HashMap<Tile, Option<Bounds>> = HashMap::new();
        let mut mesh = None;
        let chunks: HashMap<Tile, Content> = match layer {
            Layer::Surfaces => {
                let mut by: HashMap<Tile, Vec<(usize, Vec<u32>)>> = HashMap::new();
                for (s, o) in model.other_surfaces.iter().enumerate() {
                    let pts = o.terrain.points();
                    for (k, t) in o.terrain.triangles().iter().enumerate() {
                        let [a, b, c] = t.map(|i| pts[i as usize]);
                        let centre = [0, 1, 2].map(|i| (a[i] + b[i] + c[i]) / 3.0);
                        let key = tile(centre);
                        let list = by.entry(key).or_default();
                        match list.last_mut() {
                            Some((j, v)) if *j == s => v.push(k as u32),
                            _ => list.push((s, vec![k as u32])),
                        }
                        let bb = bounds.entry(key).or_default();
                        for p in [a, b, c] {
                            grow(bb, p);
                        }
                    }
                }
                by.into_iter()
                    .map(|(k, v)| (k, Content::Others(v)))
                    .collect()
            }
            Layer::Ground => {
                let Some(g) = &model.ground else {
                    return Self::empty(grid);
                };
                let m = Arc::new(g.mesh());
                let mut by: HashMap<Tile, Vec<u32>> = HashMap::new();
                for (k, t) in m.1.iter().enumerate() {
                    let [a, b, c] = t.map(|i| m.0[i]);
                    let centre = [0, 1, 2].map(|i| (a[i] + b[i] + c[i]) / 3.0);
                    let key = tile(centre);
                    by.entry(key).or_default().push(k as u32);
                    let bb = bounds.entry(key).or_default();
                    for p in [a, b, c] {
                        grow(bb, p);
                    }
                }
                mesh = Some(m);
                by.into_iter()
                    .map(|(k, v)| (k, Content::Triangles(v)))
                    .collect()
            }
            Layer::Points if cloud_of(model).is_some() => {
                // The tiles of the stored cloud: read when needed.
                let (file, name) = cloud_of(model).expect("checked");
                // Every tile a chunk overlaps (chunks of any size), its
                // heights from its blocks: the cloud's stored bounds lack
                // the top on older files, and boxes reaching to 0 m culled
                // and mis-levelled the tiles of a line climbing to 1 000 m
                // (flickering, vanishing points).
                let tiles = ols_pointcloud::Store::open(&file).and_then(|s| {
                    let cloud = s.cloud(name.as_deref())?;
                    s.tile_extents(&cloud, &grid, 0)
                });
                let Ok(tiles) = tiles else {
                    return Self::empty(grid);
                };
                let mut out = HashMap::new();
                for (tile, z) in tiles {
                    let area = grid.bounds(&tile);
                    bounds.insert(
                        tile,
                        Some(Bounds {
                            min: [area.min[0], area.min[1], z[0]],
                            max: [area.max[0], area.max[1], z[1] + POINTS_ABOVE_BASE],
                        }),
                    );
                    out.insert(tile, Content::Stored);
                }
                out
            }
            Layer::Points => {
                let mut by: HashMap<Tile, Vec<u32>> = HashMap::new();
                for (k, (q, c)) in model.points.iter().enumerate() {
                    if model.hidden_classes.contains(c)
                        || scene::class_color(&model.scheme, *c).is_none()
                    {
                        continue;
                    }
                    let p = crate::selection::lidar_xyz(model, q);
                    let key = tile(p);
                    by.entry(key).or_default().push(k as u32);
                    grow(bounds.entry(key).or_default(), p);
                }
                by.into_iter()
                    .map(|(k, v)| (k, Content::Points(v)))
                    .collect()
            }
            _ => {
                let mut by: HashMap<Tile, Vec<Prim>> = HashMap::new();
                for p in scene::prims(model, layer, case) {
                    let key = grid.tile_at(0, p.anchor);
                    let bb = bounds.entry(key).or_default();
                    for q in &p.points {
                        grow(bb, *q);
                    }
                    by.entry(key).or_default().push(p);
                }
                by.into_iter()
                    .map(|(k, v)| (k, Content::Prims(v)))
                    .collect()
            }
        };
        Self {
            grid,
            chunks: chunks
                .into_iter()
                .filter_map(|(k, c)| Some((k, (bounds.remove(&k).flatten()?, c))))
                .collect(),
            mesh,
        }
    }

    fn empty(grid: Grid) -> Self {
        Self {
            grid,
            chunks: HashMap::new(),
            mesh: None,
        }
    }

    /// Whether its points are read from the project's point cloud
    /// ([`PointStream`]) rather than built here.
    pub fn streamed(&self) -> bool {
        self.chunks
            .values()
            .any(|(_, c)| matches!(c, Content::Stored))
    }

    /// The area of a tile.
    pub fn area(&self, tile: &Tile) -> ([f64; 2], [f64; 2]) {
        let b = self.grid.bounds(tile);
        (b.min, b.max)
    }

    /// The chunks and their bounds.
    pub fn keys(&self) -> impl Iterator<Item = (Tile, Bounds)> + '_ {
        self.chunks.iter().map(|(k, (b, _))| (*k, *b))
    }

    /// Number of chunks.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    /// The geometry of chunk `tile` at `level`, as `row` says.
    pub fn build(
        &self,
        model: &Model,
        layer: Layer,
        tile: &Tile,
        level: usize,
        row: &LodRow,
    ) -> Scene {
        let mut s = Scene::new(model.origin);
        let Some((_, content)) = self.chunks.get(tile) else {
            return s;
        };
        let detail = |level: usize| row.detail[level.clamp(1, 2) - 1];
        match content {
            Content::Prims(prims) => {
                let per_span = if level == 0 {
                    usize::MAX
                } else {
                    detail(level).round() as usize
                };
                scene::draw_prims(&mut s, prims, level, per_span);
            }
            Content::Points(points) => {
                let stride = if level == 0 {
                    1
                } else {
                    (100.0 / detail(level)).round().max(1.0) as usize
                };
                for &k in points {
                    if !(k as usize).is_multiple_of(stride) {
                        continue;
                    }
                    let (q, c) = &model.points[k as usize];
                    if let Some(col) = scene::class_color(&model.scheme, *c) {
                        let p = crate::selection::lidar_xyz(model, q);
                        s.point(p, [col[0], col[1], col[2], 255], 2.0);
                    }
                }
            }
            Content::Stored => {}
            Content::Others(lists) => {
                for (k, tris) in lists {
                    let Some(o) = model.other_surfaces.get(*k) else {
                        continue;
                    };
                    if level == 0 {
                        let pts = o.terrain.points();
                        for &t in tris {
                            let [a, b, c] =
                                o.terrain.triangles()[t as usize].map(|i| pts[i as usize]);
                            s.triangle([a, b, c], [scene::tinted_color(a, b, c, o.color); 3]);
                        }
                    } else {
                        let area = self.grid.bounds(tile);
                        grid_surface(
                            &mut s,
                            &o.terrain,
                            area,
                            detail(level).max(0.5),
                            |a, b, c| scene::tinted_color(a, b, c, o.color),
                        );
                    }
                }
            }
            Content::Triangles(tris) if level == 0 => {
                if let Some(m) = &self.mesh {
                    for &k in tris {
                        let [a, b, c] = m.1[k as usize].map(|i| m.0[i]);
                        s.triangle([a, b, c], [scene::ground_color(a, b, c); 3]);
                    }
                }
            }
            Content::Triangles(_) => {
                debug_assert_eq!(DetailKind::of(layer), DetailKind::Spacing);
                if let Some(g) = &model.ground {
                    let area = self.grid.bounds(tile);
                    let step = detail(level).max(0.5);
                    let n = ((area.max[0] - area.min[0]) / step).ceil() as usize;
                    let at = |i: usize, j: usize| {
                        let (x, y) = (
                            (area.min[0] + i as f64 * step).min(area.max[0]),
                            (area.min[1] + j as f64 * step).min(area.max[1]),
                        );
                        g.height(x, y).map(|z| [x, y, z])
                    };
                    let heights: Vec<Vec<Option<[f64; 3]>>> = (0..=n)
                        .map(|i| (0..=n).map(|j| at(i, j)).collect())
                        .collect();
                    for i in 0..n {
                        for j in 0..n {
                            let (Some(a), Some(b), Some(c), Some(d)) = (
                                heights[i][j],
                                heights[i + 1][j],
                                heights[i + 1][j + 1],
                                heights[i][j + 1],
                            ) else {
                                continue;
                            };
                            s.triangle([a, b, c], [scene::ground_color(a, b, c); 3]);
                            s.triangle([a, c, d], [scene::ground_color(a, c, d); 3]);
                        }
                    }
                }
            }
        }
        s
    }
}

/// A surface sampled on a regular grid of `step` over `area` (the coarse
/// levels of detail), each triangle shaded by `color`.
fn grid_surface(
    s: &mut Scene,
    g: &ols_terrain::Terrain,
    area: ols_core::grid::Bounds,
    step: f64,
    color: impl Fn([f64; 3], [f64; 3], [f64; 3]) -> [u8; 4],
) {
    let n = ((area.max[0] - area.min[0]) / step).ceil() as usize;
    let at = |i: usize, j: usize| {
        let (x, y) = (
            (area.min[0] + i as f64 * step).min(area.max[0]),
            (area.min[1] + j as f64 * step).min(area.max[1]),
        );
        g.height(x, y).map(|z| [x, y, z])
    };
    let heights: Vec<Vec<Option<[f64; 3]>>> = (0..=n)
        .map(|i| (0..=n).map(|j| at(i, j)).collect())
        .collect();
    for i in 0..n {
        for j in 0..n {
            let (Some(a), Some(b), Some(c), Some(d)) = (
                heights[i][j],
                heights[i + 1][j],
                heights[i + 1][j + 1],
                heights[i][j + 1],
            ) else {
                continue;
            };
            s.triangle([a, b, c], [color(a, b, c); 3]);
            s.triangle([a, c, d], [color(a, c, d); 3]);
        }
    }
}

/// The point cloud of a model: project file and cloud name.
pub fn cloud_of(model: &Model) -> Option<(std::path::PathBuf, Option<String>)> {
    let spec = model.cloud.as_ref()?;
    Some(match spec.split_once('#') {
        Some((f, n)) => (f.into(), Some(n.to_owned())),
        None => (spec.into(), None),
    })
}

/// A tile of LiDAR points to read.
struct Request {
    tile: Tile,
    level: usize,
    area: ([f64; 2], [f64; 2]),
    /// Every n-th point of a class kept (1: all).
    every: usize,
    /// Classes kept whole (small ones).
    whole: [bool; 256],
    /// Classes not read.
    hidden: [bool; 256],
    generation: u64,
}

/// A tile read.
pub struct Loaded {
    /// The tile.
    pub tile: Tile,
    /// Its level of detail.
    pub level: usize,
    /// Settings it was read with ([`PointStream::generation`]).
    pub generation: u64,
    /// Its points; `None`: not read, no longer wanted.
    pub scene: Option<Result<Scene, String>>,
}

/// LiDAR points streamed from the project's point cloud: tiles read on a
/// background thread (its own connection to the project file), so the
/// view never waits for decoding.
pub struct PointStream {
    tx: std::sync::mpsc::Sender<Request>,
    rx: std::sync::mpsc::Receiver<Loaded>,
    /// Settings now (hidden classes, shares): results of other ones are
    /// dropped.
    pub generation: u64,
    /// Tiles asked for and not back yet.
    in_flight: std::collections::HashSet<(Tile, usize)>,
    /// What the view wants now: the reader skips requests no longer in
    /// it (the camera moved on), so the queue never lags behind.
    wanted: Arc<std::sync::Mutex<std::collections::HashSet<(Tile, usize)>>>,
}

impl std::fmt::Debug for PointStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PointStream")
            .field("generation", &self.generation)
            .field("in_flight", &self.in_flight.len())
            .finish()
    }
}

impl PointStream {
    /// Starts the reader of `model`'s point cloud (`None` without one).
    pub fn new(model: &Model, ctx: Option<eframe::egui::Context>) -> Option<Self> {
        let (file, name) = cloud_of(model)?;
        let colors: Vec<Option<[u8; 3]>> = (0..=255u8)
            .map(|c| scene::class_color(&model.scheme, c))
            .collect();
        let origin = model.origin;
        let (tx, requests) = std::sync::mpsc::channel::<Request>();
        let (done, rx) = std::sync::mpsc::channel();
        let wanted: Arc<std::sync::Mutex<std::collections::HashSet<(Tile, usize)>>> =
            Default::default();
        let still = wanted.clone();
        std::thread::spawn(move || {
            let store = ols_pointcloud::Store::open(&file)
                .and_then(|s| s.cloud(name.as_deref()).map(|c| (s, c)));
            let budget =
                ols_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(60));
            for r in requests {
                let skip = still
                    .lock()
                    .map(|w| !w.contains(&(r.tile, r.level)))
                    .unwrap_or(false);
                let scene = if skip {
                    None
                } else {
                    Some(match &store {
                        Err(e) => Err(e.to_string()),
                        Ok((store, cloud)) => {
                            let mut s = Scene::new(origin);
                            let mut seen = [0usize; 256];
                            let classes: Vec<u8> =
                                (0..=255u8).filter(|c| !r.hidden[*c as usize]).collect();
                            store
                                .visit_rect(
                                    cloud,
                                    r.area.0,
                                    r.area.1,
                                    Some(&classes),
                                    &budget,
                                    &mut |x, y, z, c| {
                                        let n = &mut seen[c as usize];
                                        *n += 1;
                                        if (r.whole[c as usize] || (*n).is_multiple_of(r.every))
                                            && let Some(col) = colors[c as usize]
                                        {
                                            s.point([x, y, z], [col[0], col[1], col[2], 255], 2.0);
                                        }
                                    },
                                )
                                .map(|_| s)
                                .map_err(|e| e.to_string())
                        }
                    })
                };
                let sent = done.send(Loaded {
                    tile: r.tile,
                    level: r.level,
                    generation: r.generation,
                    scene,
                });
                if sent.is_err() {
                    break;
                }
                if let Some(c) = &ctx {
                    c.request_repaint();
                }
            }
        });
        Some(Self {
            tx,
            rx,
            generation: 0,
            in_flight: Default::default(),
            wanted,
        })
    }

    /// The tiles and levels the view wants now (the reader skips the
    /// others still queued).
    pub fn want(&self, now: impl IntoIterator<Item = (Tile, usize)>) {
        if let Ok(mut w) = self.wanted.lock() {
            w.clear();
            w.extend(now);
        }
    }

    /// Other settings: what is being read is no longer wanted.
    pub fn restart(&mut self) {
        self.generation += 1;
        self.in_flight.clear();
    }

    /// Asks for tile `tile` at `level`, keeping `share` (0–1] of its
    /// points (classes of up to [`crate::model::SMALL_CLASS`] points
    /// whole), without the `hidden` classes; nothing if already asked.
    pub fn request(
        &mut self,
        index: &ChunkIndex,
        tile: Tile,
        level: usize,
        share: f64,
        model: &Model,
    ) {
        if !self.in_flight.insert((tile, level)) {
            return;
        }
        let mut whole = [false; 256];
        for (c, n) in &model.class_counts {
            whole[*c as usize] = *n <= crate::model::SMALL_CLASS;
        }
        let mut hidden = [false; 256];
        for c in &model.hidden_classes {
            hidden[*c as usize] = true;
        }
        let every = (1.0 / share.clamp(1e-6, 1.0)).round().max(1.0) as usize;
        let _ = self.tx.send(Request {
            tile,
            level,
            area: index.area(&tile),
            every,
            whole,
            hidden,
            generation: self.generation,
        });
    }

    /// The tiles read since the last call (of the current settings).
    pub fn take(&mut self) -> Vec<Loaded> {
        let mut out = Vec::new();
        while let Ok(l) = self.rx.try_recv() {
            if l.generation == self.generation {
                self.in_flight.remove(&(l.tile, l.level));
                out.push(l);
            }
        }
        out
    }

    /// Tiles being read.
    pub fn pending(&self) -> usize {
        self.in_flight.len()
    }
}
