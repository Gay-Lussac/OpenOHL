// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Chunks, levels of detail and culling (ADR-0006, ADR-0008).
//!
//! A layer is split into **chunks** (tiles of the project's grid), each
//! with its bounds and up to [`LODS`] versions of its geometry, from full
//! detail (0) to coarsest. Every frame [`ChunkSet::plan`] picks, for each
//! chunk in the view frustum and within the layer's render distance, the
//! level its [`LodRule`] gives for the distance from the eye; the caller
//! builds what is missing (nearest first, within a time budget) and draws
//! [`ChunkSet::drawn`]. A chunk whose level is not built yet is drawn at
//! another level already built, so the view never waits. Chunks far
//! beyond the render distance are dropped from the GPU.
//!
//! Display only: the engineering model is never simplified.

use std::collections::HashMap;
use std::hash::Hash;

use crate::{Bounds, Camera, Clip, GpuScene};

/// Levels of detail per chunk: 0 full detail, 1, 2 coarsest.
pub const LODS: usize = 3;

/// When a layer is drawn and at which level of detail.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LodRule {
    /// Chunks farther from the eye than this are not drawn, m.
    pub render_distance: f64,
    /// Distance from which level 1, then level 2, is drawn, m.
    pub from: [f64; LODS - 1],
}

impl LodRule {
    /// The level for a chunk `distance` m from the eye; `None` beyond the
    /// render distance.
    pub fn level(&self, distance: f64) -> Option<usize> {
        if distance > self.render_distance {
            return None;
        }
        Some(self.from.iter().filter(|&&d| distance >= d).count())
    }
}

/// The view frustum of a camera, to cull chunks.
#[derive(Debug, Clone)]
pub struct Frustum {
    m: nalgebra::Matrix4<f64>,
    target: [f64; 3],
    eye: [f64; 3],
    clip: Option<Clip>,
}

impl Frustum {
    /// The frustum of `camera` in a view of `aspect` (width / height).
    pub fn new(camera: &Camera, aspect: f64) -> Self {
        Self {
            m: camera.view_proj_f64(camera.target, aspect),
            target: camera.target,
            eye: camera.eye(),
            clip: None,
        }
    }

    /// The frustum seeing only what meets `clip` (`None`: all it holds):
    /// chunks and nodes outside the box are neither built nor drawn.
    pub fn clipped(mut self, clip: Option<Clip>) -> Self {
        self.clip = clip;
        self
    }

    /// Whether any of `bounds` may be in view: false only when its eight
    /// corners all lie outside the same side of the frustum (conservative:
    /// a box across a frustum corner is kept).
    pub fn sees(&self, bounds: &Bounds) -> bool {
        if self.clip.is_some_and(|c| !c.meets(bounds)) {
            return false;
        }
        let mut out = [true; 6];
        for k in 0..8 {
            let p = [0, 1, 2].map(|a| {
                if k & (1 << a) == 0 {
                    bounds.min[a]
                } else {
                    bounds.max[a]
                }
            });
            let v = self.m
                * nalgebra::Vector4::new(
                    p[0] - self.target[0],
                    p[1] - self.target[1],
                    p[2] - self.target[2],
                    1.0,
                );
            let inside = [
                v.x >= -v.w,
                v.x <= v.w,
                v.y >= -v.w,
                v.y <= v.w,
                v.z >= 0.0,
                v.z <= v.w,
            ];
            for (o, i) in out.iter_mut().zip(inside) {
                *o &= !i;
            }
        }
        !out.iter().any(|&o| o)
    }

    /// Distance from the eye to the nearest point of `bounds` (0 inside).
    pub fn distance(&self, bounds: &Bounds) -> f64 {
        let d = [0, 1, 2].map(|a| {
            let e = self.eye[a];
            (bounds.min[a] - e).max(e - bounds.max[a]).max(0.0)
        });
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    }
}

/// One chunk: its bounds and the levels built.
#[derive(Debug)]
struct Chunk {
    bounds: Bounds,
    lods: [Option<GpuScene>; LODS],
    /// Level drawn in the last plan.
    shown: Option<usize>,
}

/// A chunk to build: key, level, distance from the eye.
#[derive(Debug, Clone, PartialEq)]
pub struct Wanted<K> {
    /// Chunk.
    pub key: K,
    /// Level of detail.
    pub level: usize,
    /// Distance from the eye, m (build the nearest first).
    pub distance: f64,
}

/// What a plan found: per level, the chunks drawn at it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlanStats {
    /// Chunks of the layer.
    pub chunks: usize,
    /// Chunks drawn at each level.
    pub drawn: [usize; LODS],
    /// Chunks in view whose level is not built yet.
    pub pending: usize,
}

/// The chunks of one layer and the levels built on the GPU.
#[derive(Debug)]
pub struct ChunkSet<K> {
    chunks: HashMap<K, Chunk>,
    /// Statistics of the last plan.
    pub stats: PlanStats,
}

impl<K> Default for ChunkSet<K> {
    fn default() -> Self {
        Self {
            chunks: HashMap::new(),
            stats: PlanStats::default(),
        }
    }
}

impl<K: Eq + Hash + Clone> ChunkSet<K> {
    /// Chunks `keys` with their bounds; levels built before are dropped.
    pub fn reset(&mut self, keys: impl IntoIterator<Item = (K, Bounds)>) {
        self.chunks = keys
            .into_iter()
            .map(|(k, bounds)| {
                (
                    k,
                    Chunk {
                        bounds,
                        lods: [None, None, None],
                        shown: None,
                    },
                )
            })
            .collect();
        self.stats = PlanStats::default();
    }

    /// Whether it has no chunks.
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// Picks the level of every chunk for this frame; returns the levels
    /// to build, nearest first. Levels of chunks beyond 1.5 × the render
    /// distance are dropped (streamed out).
    pub fn plan(&mut self, frustum: &Frustum, rule: &LodRule) -> Vec<Wanted<K>> {
        let mut wanted = Vec::new();
        let mut stats = PlanStats {
            chunks: self.chunks.len(),
            ..PlanStats::default()
        };
        for (key, chunk) in &mut self.chunks {
            let distance = frustum.distance(&chunk.bounds);
            if distance > 1.5 * rule.render_distance {
                chunk.lods = [None, None, None];
            }
            chunk.shown = None;
            let Some(level) = rule.level(distance) else {
                continue;
            };
            if !frustum.sees(&chunk.bounds) {
                continue;
            }
            if chunk.lods[level].is_some() {
                chunk.shown = Some(level);
            } else {
                wanted.push(Wanted {
                    key: key.clone(),
                    level,
                    distance,
                });
                stats.pending += 1;
                // Meanwhile, the nearest level built (coarser first).
                chunk.shown = (level..LODS)
                    .chain((0..level).rev())
                    .find(|&l| chunk.lods[l].is_some());
            }
            if let Some(l) = chunk.shown {
                stats.drawn[l] += 1;
            }
        }
        wanted.sort_by(|a, b| a.distance.total_cmp(&b.distance));
        self.stats = stats;
        wanted
    }

    /// Keeps the built `level` of chunk `key` (drawn from the next plan).
    pub fn insert(&mut self, key: &K, level: usize, scene: GpuScene) {
        if let Some(c) = self.chunks.get_mut(key) {
            c.lods[level] = Some(scene);
        }
    }

    /// The chunks to draw, at the levels of the last plan.
    pub fn drawn(&self) -> impl Iterator<Item = &GpuScene> {
        self.chunks
            .values()
            .filter_map(|c| c.shown.and_then(|l| c.lods[l].as_ref()))
    }

    /// The chunks drawn at the last plan, with the level each is drawn
    /// at (what a click can pick).
    pub fn shown(&self) -> impl Iterator<Item = (&K, usize)> {
        self.chunks
            .iter()
            .filter_map(|(k, c)| c.shown.filter(|l| c.lods[*l].is_some()).map(|l| (k, l)))
    }

    /// Segments, points and triangles on the GPU, every level of every
    /// chunk.
    pub fn held(&self) -> [u64; 3] {
        let mut n = [0u64; 3];
        for g in self.chunks.values().flat_map(|c| c.lods.iter().flatten()) {
            let c = g.counts();
            n[0] += u64::from(c.0);
            n[1] += u64::from(c.1);
            n[2] += u64::from(c.2);
        }
        n
    }

    /// Levels on the GPU, all chunks.
    pub fn built(&self) -> usize {
        self.chunks
            .values()
            .map(|c| c.lods.iter().filter(|l| l.is_some()).count())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const O: [f64; 3] = [2_600_000.0, 1_200_000.0, 500.0];

    fn cube(x: f64, y: f64, half: f64) -> Bounds {
        Bounds {
            min: [O[0] + x - half, O[1] + y - half, O[2] - half],
            max: [O[0] + x + half, O[1] + y + half, O[2] + half],
        }
    }

    /// Looking north along y from 1 km south, level.
    fn camera() -> Camera {
        Camera {
            target: O,
            distance: 1000.0,
            yaw: (-90f64).to_radians(),
            pitch: 0.0,
            fov_y: 45f64.to_radians(),
            exaggeration: 1.0,
            ortho: false,
        }
    }

    #[test]
    fn levels_by_distance_and_render_distance() {
        let rule = LodRule {
            render_distance: 5000.0,
            from: [500.0, 2000.0],
        };
        assert_eq!(rule.level(0.0), Some(0));
        assert_eq!(rule.level(499.0), Some(0));
        assert_eq!(rule.level(500.0), Some(1));
        assert_eq!(rule.level(4999.0), Some(2));
        assert_eq!(rule.level(5001.0), None);
    }

    #[test]
    fn the_frustum_keeps_what_is_in_view_and_measures_distance() {
        let f = Frustum::new(&camera(), 1.5);
        // At the target, far ahead: in view.
        assert!(f.sees(&cube(0.0, 0.0, 10.0)));
        assert!(f.sees(&cube(0.0, 5000.0, 10.0)));
        // Behind the eye, far to the side: not.
        assert!(!f.sees(&cube(0.0, -3000.0, 10.0)));
        assert!(!f.sees(&cube(5000.0, 0.0, 10.0)));
        // Across the edge of the view: kept.
        let half_width = 1000.0 * (22.5f64).to_radians().tan() * 1.5;
        assert!(f.sees(&cube(half_width + 5.0, 0.0, 10.0)));
        // Distance from the eye (1 km south of the target) to a box.
        let d = f.distance(&cube(0.0, 0.0, 10.0));
        assert!((d - 990.0).abs() < 1e-6, "{d}");
        assert_eq!(f.distance(&cube(0.0, -1000.0, 10.0)), 0.0);
    }

    #[test]
    fn a_plan_picks_levels_and_builds_the_nearest_first() {
        let mut set: ChunkSet<u32> = ChunkSet::default();
        // Chunks 0..6 every 500 m ahead of the eye; one behind.
        set.reset(
            (0..6)
                .map(|k| (k, cube(0.0, f64::from(k) * 500.0 - 900.0, 50.0)))
                .chain([(99, cube(0.0, -3000.0, 50.0))]),
        );
        let rule = LodRule {
            render_distance: 2000.0,
            from: [300.0, 1000.0],
        };
        let f = Frustum::new(&camera(), 1.5);
        let wanted = set.plan(&f, &rule);
        // Near edges 50, 550, 1050, 1550, 2050, 2550 m from the eye: behind
        // (99) and beyond 2 km (4, 5) are left out.
        let keys: Vec<u32> = wanted.iter().map(|w| w.key).collect();
        assert_eq!(keys, vec![0, 1, 2, 3]);
        let levels: Vec<usize> = wanted.iter().map(|w| w.level).collect();
        assert_eq!(levels, vec![0, 1, 2, 2]);
        assert!(wanted.windows(2).all(|w| w[0].distance <= w[1].distance));
        assert_eq!(set.stats.pending, 4);
        assert_eq!(set.stats.chunks, 7);
        assert_eq!(set.drawn().count(), 0, "nothing built yet");
    }

    /// A level not built yet is drawn at a level built (coarser first);
    /// levels far beyond the render distance are dropped.
    #[test]
    fn missing_levels_fall_back_and_far_chunks_stream_out() {
        let device = match crate::headless_device() {
            Ok((d, _)) => d,
            Err(e) => {
                assert!(
                    ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
                    "{e}"
                );
                return;
            }
        };
        let scene = || {
            let mut s = crate::Scene::new(O);
            s.point(O, [255; 4], 2.0);
            GpuScene::new(&device, &s)
        };
        let mut set: ChunkSet<u32> = ChunkSet::default();
        set.reset([(0, cube(0.0, 0.0, 50.0))]);
        let rule = LodRule {
            render_distance: 2000.0,
            from: [300.0, 1000.0],
        };
        let f = Frustum::new(&camera(), 1.5);
        // 950 m away: level 1 wanted; only 2 built: drawn at 2 meanwhile.
        set.insert(&0, 2, scene());
        let wanted = set.plan(&f, &rule);
        assert_eq!(wanted[0].level, 1);
        assert_eq!(set.stats.drawn, [0, 0, 1]);
        set.insert(&0, 1, scene());
        assert!(set.plan(&f, &rule).is_empty());
        assert_eq!(set.stats.drawn, [0, 1, 0]);
        assert_eq!(set.built(), 2);
        // The camera 5 km away: beyond 1.5 × 2 km, streamed out.
        let mut far = camera();
        far.distance = 5000.0;
        set.plan(&Frustum::new(&far, 1.5), &rule);
        assert_eq!(set.built(), 0);
        assert_eq!(set.drawn().count(), 0);
    }
}
