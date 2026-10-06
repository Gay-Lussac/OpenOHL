// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The 3D view's layers in chunks (ADR-0006, ADR-0008): what of a layer
//! lies in each 256 m tile of the project grid, and its geometry at a
//! level of detail ([`crate::lod`]).
//!
//! - **Terrain**: level 0 the TIN's triangles (by centroid); levels 1 and
//!   2 a regular grid of the surface's heights at the row's spacing.
//! - **LiDAR** held in memory: level 0 every point shown; levels 1 and 2
//!   a share of them (every n-th, the same points every time). A point
//!   cloud of the project or a COPC file is streamed by node instead
//!   ([`crate::octree`]).
//! - **Drawings** (DXF): level 0 every vertex and point; levels 1 and 2
//!   the lines simplified to the row's tolerance (Douglas–Peucker), lines
//!   and points smaller left out. Lines are cut where they cross tiles.
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
    /// Drawing lines (pieces within the tile) and points, with colours.
    Lines(Vec<Line>, Vec<Dot>),
}

/// A polyline of a drawing and its colour.
type Line = (Vec<[f64; 3]>, [u8; 4]);
/// A point of a drawing and its colour.
type Dot = ([f64; 3], [u8; 4]);

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

/// Lowest and highest (x, y, z) of points.
fn extent(pts: &[[f64; 3]]) -> ([f64; 3], [f64; 3]) {
    pts.iter().fold(
        ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]),
        |(lo, hi), p| {
            (
                [0, 1, 2].map(|k| lo[k].min(p[k])),
                [0, 1, 2].map(|k| hi[k].max(p[k])),
            )
        },
    )
}

/// A polyline simplified (Douglas–Peucker): no point of it farther than
/// `tol` from the result; the ends kept.
pub fn simplify(pts: &[[f64; 3]], tol: f64) -> Vec<[f64; 3]> {
    if pts.len() <= 2 {
        return pts.to_vec();
    }
    let dist = |p: [f64; 3], a: [f64; 3], b: [f64; 3]| {
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ap = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
        let len2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
        let t = if len2 > 0.0 {
            ((ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let d = [ap[0] - t * ab[0], ap[1] - t * ab[1], ap[2] - t * ab[2]];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    };
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    // Ranges to look at (no recursion: long lines).
    let mut stack = vec![(0, pts.len() - 1)];
    while let Some((i, j)) = stack.pop() {
        let (mut far, mut at) = (0.0, 0);
        for k in i + 1..j {
            let d = dist(pts[k], pts[i], pts[j]);
            if d > far {
                (far, at) = (d, k);
            }
        }
        if far > tol {
            keep[at] = true;
            stack.push((i, at));
            stack.push((at, j));
        }
    }
    pts.iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(*p))
        .collect()
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
            Layer::Drawings => {
                let mut by: HashMap<Tile, (Vec<Line>, Vec<Dot>)> = HashMap::new();
                for d in &model.backgrounds.drawings {
                    if model
                        .hidden_backgrounds
                        .contains(&crate::backgrounds::drawing_key(&d.info.name))
                    {
                        continue;
                    }
                    let off: Vec<bool> = d.drawing.layers.iter().map(|l| l.off).collect();
                    let hidden = |layer: u32| off.get(layer as usize).copied().unwrap_or(false);
                    for l in &d.drawing.lines {
                        if hidden(l.layer) || l.points.len() < 2 {
                            continue;
                        }
                        let c = [l.color[0], l.color[1], l.color[2], 255];
                        let mut pts = l.points.clone();
                        if l.closed {
                            pts.push(pts[0]);
                        }
                        // Cut where it crosses into another tile (the
                        // crossing segment in both pieces' tiles: the
                        // first).
                        let mut piece = vec![pts[0]];
                        let mut at = tile(pts[0]);
                        for &p in &pts[1..] {
                            piece.push(p);
                            let t = tile(p);
                            if t != at {
                                let done = std::mem::replace(&mut piece, vec![p]);
                                for q in &done {
                                    grow(bounds.entry(at).or_default(), *q);
                                }
                                by.entry(at).or_default().0.push((done, c));
                                at = t;
                            }
                        }
                        if piece.len() >= 2 {
                            for q in &piece {
                                grow(bounds.entry(at).or_default(), *q);
                            }
                            by.entry(at).or_default().0.push((piece, c));
                        }
                    }
                    for m in &d.drawing.points {
                        if !hidden(m.layer) {
                            let t = tile(m.at);
                            grow(bounds.entry(t).or_default(), m.at);
                            by.entry(t)
                                .or_default()
                                .1
                                .push((m.at, [m.color[0], m.color[1], m.color[2], 255]));
                        }
                    }
                }
                by.into_iter()
                    .map(|(k, (l, p))| (k, Content::Lines(l, p)))
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
            Content::Lines(lines, points) => {
                for (pts, c) in lines {
                    if level == 0 {
                        s.polyline(pts, *c, 1.0);
                        continue;
                    }
                    let tol = detail(level);
                    let (lo, hi) = extent(pts);
                    if (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]) < tol {
                        continue;
                    }
                    s.polyline(&simplify(pts, tol), *c, 1.0);
                }
                if level == 0 {
                    for (p, c) in points {
                        s.point(*p, *c, 3.0);
                    }
                }
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
                    grid_surface(&mut s, g, area, step, scene::ground_color);
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
    let (file, name) = ols_lidar::cloud_spec(model.cloud.as_ref()?);
    Some((file.to_path_buf(), name.map(str::to_owned)))
}
