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

/// What a chunk holds.
#[derive(Debug)]
enum Content {
    /// Primitives.
    Prims(Vec<Prim>),
    /// TIN triangles (indices into the index's mesh).
    Triangles(Vec<u32>),
    /// LiDAR points (indices into the model's points).
    Points(Vec<u32>),
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
