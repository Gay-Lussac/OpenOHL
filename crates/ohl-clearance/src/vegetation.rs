// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Vegetation and danger trees: points near the wires of weather cases,
//! now and after they have grown (**grow-in**), and trees that could
//! reach a wire when they fall (**fall-in**), against every position of
//! every span checked ([`crate::positions`]: a case as it is, blown left
//! and right, the swing arc between, unbalanced ice).
//!
//! Distances are to the conductor's surface (iced in a case with ice),
//! as every clearance ([`crate::checks`]).
//!
//! - **Grow-in**: a vegetation point closer to a wire position (radially,
//!   to the nearest subconductor) than the required clearance plus the
//!   growth until the next cut (`growth_per_year × years`, grown in every
//!   direction) plus a buffer: d − r < c_req + g + b.
//! - **Fall-in**: a vegetation point of height h above the ground (the
//!   TIN below it) is taken as a tree top on a trunk straight below it,
//!   grown to h' = h + g; falling towards a wire point it turns about the
//!   edge of its root ball, a hinge at b_r = k·h' from the trunk, and
//!   sweeps the sector of radius L = √(h'² + b_r²) about the hinge down to
//!   where it rests on the ground ([`crate::fall::Tree`]). The margin is
//!   d_fall − r − A (A: the clearance allowance); negative: a danger
//!   tree. With k = 0 the hinge is the base and the margin the distance
//!   from the base to the wire's surface less h', as before.
//!
//! Results are per span (a span of a section): counts, the nearest point,
//! the worst fall-in, and the worst points by margin, each with the case
//! and position it was found in.
//!
//! How it is made fast (theory: How it is made fast): spans are indexed on
//! a plan grid with their reach; a point meets the spans of its cell, is
//! rejected by each span's capsule (every position within it), and is
//! compared with each position whose own bound can still beat the best
//! found; the root-ball search, the ground's resting angle and the
//! ground's shelter are worked out only for trees whose bound can count or
//! be reported. Points are checked in parallel blocks
//! ([`VegetationCheck::visit_parallel`]); results are merged per span.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use ohl_core::budget::Budget;
use ohl_terrain::Terrain;

use crate::checks::Wire;
use crate::fall::{CIRCLE_TOLERANCE, Tree};
use crate::geometry::{P3, closest_on_segment, distance};
use crate::positions::{Positions, Shape};

/// Search margin beyond the clearance, m: the nearest point is reported
/// when within clearance + growth + buffer + this.
pub const SEARCH_MARGIN: f64 = 5.0;

/// Points per block of [`VegetationCheck::visit_parallel`] (about one
/// stored chunk of a point cloud).
pub const BLOCK: usize = 65_536;

/// What the vegetation check looks for.
#[derive(Debug, Clone, PartialEq)]
pub struct VegetationParams {
    /// Required radial clearance from the wire c_req, m.
    pub clearance: f64,
    /// Growth per year, m.
    pub growth_per_year: f64,
    /// Years until the next cut.
    pub years: f64,
    /// Buffer b added to the clearance for grow-in (e.g. the survey's
    /// accuracy), m.
    pub buffer: f64,
    /// Look for trees that could fall onto the wires (needs the ground).
    pub fall_in: bool,
    /// Root-ball radius as a fraction k of the grown tree's height (0:
    /// the tree pivots at its base, as before 2026-10-20; default
    /// [`DEFAULT_ROOT_BALL`]).
    pub root_ball: f64,
    /// Clearance allowance A of a falling tree (flashover clearance,
    /// buffers), m (default [`DEFAULT_FALL_ALLOWANCE`]).
    pub fall_allowance: f64,
    /// Points lower than this above the ground are not trees, m.
    pub min_tree_height: f64,
    /// The tallest tree looked for, m (how far from the wires bases are
    /// searched).
    pub max_tree_height: f64,
    /// Largest horizontal distance from a vegetation point to the nearest
    /// ground point the ground surface is built from (a TIN vertex: a
    /// LiDAR ground point or a survey point) for its ground height — its
    /// tree base — to be taken, m (none: no limit). Beyond it the point
    /// has no base and no fall-in check (default
    /// [`DEFAULT_MAX_BASE_GROUND_DISTANCE`]).
    pub max_base_ground_distance: Option<f64>,
    /// Worst points kept per span.
    pub keep: usize,
}

// The danger-tree defaults (the owner, 2026-10-20): the utility
// practice, the same in the application and the command line; a
// design-code plugin's preset may give others, and a user may set any
// (k = 0, A = 0 and no limit give the check as before 2026-10-20).

/// Default root-ball fraction k: a hinge at 5 % of the tree's height from
/// the trunk.
pub const DEFAULT_ROOT_BALL: f64 = 0.05;

/// Default clearance allowance A of a falling tree, m.
pub const DEFAULT_FALL_ALLOWANCE: f64 = 2.0;

/// Default largest distance from a tree base to a ground point, m
/// ([`VegetationParams::max_base_ground_distance`]).
pub const DEFAULT_MAX_BASE_GROUND_DISTANCE: f64 = 5.0;

/// Default search distance, m: the half width of the corridor around the
/// spans' chords in which vegetation points are read (and isoclearance
/// lines drawn).
pub const DEFAULT_SEARCH_DISTANCE: f64 = 50.0;

impl Default for VegetationParams {
    fn default() -> Self {
        Self {
            clearance: 5.0,
            growth_per_year: 0.0,
            years: 0.0,
            buffer: 0.0,
            fall_in: false,
            root_ball: DEFAULT_ROOT_BALL,
            fall_allowance: DEFAULT_FALL_ALLOWANCE,
            min_tree_height: 2.0,
            max_tree_height: 40.0,
            max_base_ground_distance: Some(DEFAULT_MAX_BASE_GROUND_DISTANCE),
            keep: 10,
        }
    }
}

impl VegetationParams {
    /// Growth until the next cut, m.
    pub fn growth(&self) -> f64 {
        (self.growth_per_year * self.years).max(0.0)
    }

    /// How far from a wire's surface points are looked at for grow-in, m.
    pub fn near_reach(&self) -> f64 {
        self.clearance + self.growth() + self.buffer + SEARCH_MARGIN
    }

    /// How far from a wire's surface tree bases are looked at for
    /// fall-in, m: the tallest tree grown, its root ball and the
    /// allowance.
    pub fn fall_reach(&self) -> f64 {
        let h = self.max_tree_height + self.growth();
        let root = self.root_ball.max(0.0) * h;
        root + h.hypot(root) + self.fall_allowance.max(0.0)
    }
}

/// One vegetation point near a span.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VegPoint {
    /// The point.
    pub point: P3,
    /// Its class.
    pub class: u8,
    /// Radial distance to the nearest position's surface, m (`INFINITY`:
    /// beyond the search).
    pub distance: f64,
    /// Nearest point of that position (its axis).
    pub wire_point: P3,
    /// The position it is nearest to ([`Positions::positions`]).
    pub position: Option<usize>,
    /// Height above the ground, m (none: no ground below it).
    pub height: Option<f64>,
    /// Fall-in margin d_fall − r − A, m (negative: it can reach a wire).
    pub fall_margin: Option<f64>,
    /// The position the tree comes nearest to when it falls.
    pub fall_position: Option<usize>,
    /// The wire point (axis) it falls towards.
    pub fall_point: Option<P3>,
    /// Its fall was found without the ground's resting angle: the margin
    /// is a lower bound (the tree's whole circle of radius L), worked out
    /// fully only for trees that count or are reported.
    bound_only: bool,
}

impl VegPoint {
    /// The smaller of the grow-in margin (distance − growth − buffer −
    /// required) and the fall-in margin, m.
    pub fn margin(&self, p: &VegetationParams) -> f64 {
        let near = self.distance - p.growth() - p.buffer - p.clearance;
        self.fall_margin.map_or(near, |f| near.min(f))
    }

    /// Order of the worst list: by margin, then by place (the same order
    /// however the points were shared between threads).
    fn order(&self, other: &Self, p: &VegetationParams) -> std::cmp::Ordering {
        self.margin(p)
            .total_cmp(&other.margin(p))
            .then_with(|| self.point[0].total_cmp(&other.point[0]))
            .then_with(|| self.point[1].total_cmp(&other.point[1]))
            .then_with(|| self.point[2].total_cmp(&other.point[2]))
    }
}

fn before(a: &VegPoint, b: &VegPoint) -> bool {
    a.point[0]
        .total_cmp(&b.point[0])
        .then_with(|| a.point[1].total_cmp(&b.point[1]))
        .then_with(|| a.point[2].total_cmp(&b.point[2]))
        .is_lt()
}

/// The vegetation found near one span.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WireVegetation {
    /// Points closer than the clearance plus growth and buffer (grow-in).
    pub encroaching: u64,
    /// Points of trees that could fall onto the span (fall-in).
    pub fall_in: u64,
    /// The nearest point within the search (the worst grow-in).
    pub nearest: Option<VegPoint>,
    /// The tree with the smallest fall-in margin.
    pub worst_fall: Option<VegPoint>,
    /// The worst points by margin (grow-in or fall-in), worst first.
    pub worst: Vec<VegPoint>,
}

impl WireVegetation {
    fn merge(&mut self, other: WireVegetation, p: &VegetationParams) {
        self.encroaching += other.encroaching;
        self.fall_in += other.fall_in;
        if let Some(n) = other.nearest
            && self.nearest.is_none_or(|m| {
                n.distance < m.distance || (n.distance == m.distance && before(&n, &m))
            })
        {
            self.nearest = Some(n);
        }
        if let Some(f) = other.worst_fall {
            let fm = f.fall_margin.unwrap_or(f64::INFINITY);
            if self.worst_fall.is_none_or(|m| {
                let mm = m.fall_margin.unwrap_or(f64::INFINITY);
                fm < mm || (fm == mm && before(&f, &m))
            }) {
                self.worst_fall = Some(f);
            }
        }
        self.worst.extend(other.worst);
        if self.worst.len() >= 4 * p.keep.max(1) {
            self.worst.sort_by(|a, b| a.order(b, p));
            self.worst.truncate(p.keep);
        }
    }
}

/// Spans on a plan grid: each in the cells its capsule and the reach
/// cover.
#[derive(Debug, Clone)]
pub struct SpanIndex {
    cell: f64,
    cells: HashMap<(i64, i64), Vec<u32>>,
}

impl SpanIndex {
    /// Indexes the spans of `positions` for points up to `reach` (m) from
    /// their capsules, on cells of `cell` m.
    pub fn new(positions: &Positions, reach: f64, cell: f64) -> Self {
        let mut cells: HashMap<(i64, i64), Vec<u32>> = HashMap::new();
        let at = |v: f64| (v / cell).floor() as i64;
        for (k, s) in positions.spans.iter().enumerate() {
            let r = s.capsule + reach;
            let (a, b) = (s.axis.a1, s.axis.a2);
            let (x0, x1) = (a[0].min(b[0]) - r, a[0].max(b[0]) + r);
            let (y0, y1) = (a[1].min(b[1]) - r, a[1].max(b[1]) + r);
            // Cells within r of the axis in plan.
            for i in at(x0)..=at(x1) {
                for j in at(y0)..=at(y1) {
                    let (cx, cy) = ((i as f64 + 0.5) * cell, (j as f64 + 0.5) * cell);
                    let (q, _) =
                        closest_on_segment([cx, cy, 0.0], [a[0], a[1], 0.0], [b[0], b[1], 0.0]);
                    if (q[0] - cx).hypot(q[1] - cy) <= r + cell * std::f64::consts::FRAC_1_SQRT_2 {
                        cells.entry((i, j)).or_default().push(k as u32);
                    }
                }
            }
        }
        Self { cell, cells }
    }

    /// The spans that may be within reach of plan position (x, y).
    pub fn near(&self, x: f64, y: f64) -> &[u32] {
        let at = |v: f64| (v / self.cell).floor() as i64;
        self.cells.get(&(at(x), at(y))).map_or(&[], Vec::as_slice)
    }
}

/// The spans with one of `points` (plan) within `reach` (m) of their
/// capsule in plan: the spans with vegetation within reach, where the
/// unbalanced ice is solved (most spans over fields have none).
pub fn spans_near(
    positions: &Positions,
    reach: f64,
    points: impl IntoIterator<Item = [f64; 2]>,
) -> Vec<usize> {
    let index = SpanIndex::new(positions, reach, 10f64.max(reach / 4.0));
    let mut near = vec![false; positions.spans.len()];
    for p in points {
        for &s in index.near(p[0], p[1]) {
            let s = s as usize;
            if near[s] {
                continue;
            }
            let sp = &positions.spans[s];
            let (a, b) = (sp.axis.a1, sp.axis.a2);
            let (q, _) =
                closest_on_segment([p[0], p[1], 0.0], [a[0], a[1], 0.0], [b[0], b[1], 0.0]);
            if (q[0] - p[0]).hypot(q[1] - p[1]) - sp.capsule <= reach {
                near[s] = true;
            }
        }
    }
    (0..near.len()).filter(|&s| near[s]).collect()
}

/// Whether a point of `ground` (a vertex of its triangles: a ground point
/// it is built from) lies within `limit` m of (x, y) in plan.
pub fn ground_point_within(ground: &Terrain, x: f64, y: f64, limit: f64) -> bool {
    let mut found = false;
    ground.triangles_near(x, y, limit, |corners| {
        found |= corners.iter().any(|c| (c[0] - x).hypot(c[1] - y) <= limit);
    });
    found
}

/// A fall found for a tree and a span.
#[derive(Debug, Clone, Copy)]
struct Fall {
    margin: f64,
    position: usize,
    point: P3,
    /// A lower bound only (the circle of radius L; not refined beyond
    /// the cutoff).
    bound_only: bool,
}

/// A span met by a point: its index, the grow-in found (distance, wire
/// point, position) and the fall.
type Touched = (usize, (f64, P3, Option<usize>), Option<Fall>);

/// Counts of the work done (for the speed report).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Work {
    /// Point–span pairs met in the index.
    pub pairs: u64,
    /// Exact distances to positions worked out.
    pub distances: u64,
    /// Trees whose fall was worked out with the ground's resting angle.
    pub sectors: u64,
    /// Trees found sheltered by the ground (their nearest wire point
    /// below the resting trunk).
    pub sheltered: u64,
}

/// The vegetation check of the positions of a set of spans; feed points
/// with [`VegetationCheck::visit`] (or [`VegetationCheck::visit_parallel`]),
/// then take the results with [`VegetationCheck::finish`].
///
/// Cost: per point, the spans of its plan cell (a few), a capsule test
/// each, then the closest pieces of the positions that can still beat the
/// best: about 1 µs a point for one case; see the speed test
/// (`cargo test -p ohl-clearance --release -- --ignored speed`).
#[derive(Debug, Clone)]
pub struct VegetationCheck {
    params: VegetationParams,
    classes: [bool; 256],
    geometry: Arc<Positions>,
    index: Arc<SpanIndex>,
    results: Vec<WireVegetation>,
    /// Points looked at.
    pub points: u64,
    /// Points with no ground below them (no fall-in check).
    pub without_ground: u64,
    /// Of those, the points with a ground surface below them but no ground
    /// point within [`VegetationParams::max_base_ground_distance`].
    pub ground_too_far: u64,
    /// The work done.
    pub work: Work,
    touched: Vec<Touched>,
    scratch: Vec<(f64, usize)>,
}

impl VegetationCheck {
    /// A check of `wires` (one case, each as it is) against points of
    /// `classes`.
    pub fn new(wires: Vec<Wire>, classes: &[u8], params: VegetationParams) -> Self {
        Self::with_positions(Arc::new(Positions::of_wires("", wires)), classes, params)
    }

    /// A check of every position of `positions` against points of
    /// `classes`.
    pub fn with_positions(
        positions: Arc<Positions>,
        classes: &[u8],
        params: VegetationParams,
    ) -> Self {
        let mut mask = [false; 256];
        for &c in classes {
            mask[c as usize] = true;
        }
        let mut reach = params.near_reach();
        if params.fall_in {
            reach = reach.max(params.fall_reach());
        }
        let n = positions.spans.len();
        let index = SpanIndex::new(&positions, reach, 10f64.max(reach / 4.0));
        Self {
            params,
            classes: mask,
            geometry: positions,
            index: Arc::new(index),
            results: vec![WireVegetation::default(); n],
            points: 0,
            without_ground: 0,
            ground_too_far: 0,
            work: Work::default(),
            touched: Vec::new(),
            scratch: Vec::new(),
        }
    }

    /// The positions checked.
    pub fn positions(&self) -> &Arc<Positions> {
        &self.geometry
    }

    /// The span wires checked (one per span, as first solved).
    pub fn wires(&self) -> Vec<Wire> {
        self.geometry.spans.iter().map(|s| s.wire.clone()).collect()
    }

    /// The parameters.
    pub fn params(&self) -> &VegetationParams {
        &self.params
    }

    /// The same check with no point seen yet (for another thread).
    fn fresh(&self) -> Self {
        Self {
            results: vec![WireVegetation::default(); self.results.len()],
            points: 0,
            without_ground: 0,
            ground_too_far: 0,
            work: Work::default(),
            touched: Vec::new(),
            scratch: Vec::new(),
            ..self.clone()
        }
    }

    /// Takes in what `other` (the same check) found.
    pub fn merge(&mut self, other: VegetationCheck) {
        self.points += other.points;
        self.without_ground += other.without_ground;
        self.ground_too_far += other.ground_too_far;
        self.work.pairs += other.work.pairs;
        self.work.distances += other.work.distances;
        self.work.sectors += other.work.sectors;
        self.work.sheltered += other.work.sheltered;
        for (r, o) in self.results.iter_mut().zip(other.results) {
            r.merge(o, &self.params);
        }
    }
}

/// What the fall-in search reads: the positions and the parameters.
#[derive(Clone, Copy)]
struct Fixed<'a> {
    geo: &'a Positions,
    params: &'a VegetationParams,
}

impl Fixed<'_> {
    /// The fall-in margin of `tree` against position `k` from the
    /// smallest centre-line distance `d` of its hinge circle.
    fn margin_from(&self, tree: &Tree, h: f64, k: usize, d: f64) -> f64 {
        let p = self.params;
        let db = self.geo.positions[k].shape.off_surface(d);
        if tree.root == 0.0 {
            (db - h) - p.growth() - p.fall_allowance
        } else {
            (db - tree.length) - p.fall_allowance
        }
    }

    /// The fall of `tree` (top height `h` above the ground) onto span
    /// `span`: the smallest margin over its positions. A margin from
    /// `cutoff` up is a lower bound (not refined). With `ground`, the
    /// resting angle is applied to a margin below `cutoff`.
    #[allow(clippy::too_many_arguments)]
    fn fall(
        &self,
        work: &mut Work,
        scratch: &mut Vec<(f64, usize)>,
        ground: Option<&Terrain>,
        tree: &Tree,
        h: f64,
        span: usize,
        cutoff: f64,
    ) -> Option<Fall> {
        let geo = self.geo;
        let s = &geo.spans[span];
        let pb = s.axis.polar(tree.base);
        let tb = pb.along;
        // The span's bound first (all its positions' bands together): the
        // hinge circle is within b_r of the base.
        if s.positions.is_empty() {
            return None;
        }
        let near = s.bands.lower(&pb, f64::INFINITY) - tree.root - s.outer;
        let span_bound = if tree.root == 0.0 {
            (near - h) - self.params.growth() - self.params.fall_allowance
        } else {
            (near - tree.length) - self.params.fall_allowance
        };
        if span_bound >= cutoff {
            return Some(Fall {
                margin: span_bound,
                position: s.positions[0],
                point: tree.base,
                bound_only: true,
            });
        }
        // Each position's bound. Only those below the cutoff are looked at.
        scratch.clear();
        let mut lowest = (f64::INFINITY, usize::MAX);
        let mut outside = f64::INFINITY;
        for &k in &s.positions {
            let pos = &geo.positions[k];
            let lb = self.margin_from(tree, h, k, pos.bands.lower(&pb, f64::INFINITY) - tree.root);
            if lb < lowest.0 {
                lowest = (lb, k);
            }
            if lb < cutoff {
                scratch.push((lb, k));
            } else {
                outside = outside.min(lb);
            }
        }
        if lowest.1 == usize::MAX {
            return None;
        }
        let bound = Fall {
            margin: lowest.0,
            position: lowest.1,
            point: tree.base,
            bound_only: true,
        };
        if scratch.is_empty() {
            return Some(bound);
        }
        scratch.sort_by(|a, b| a.0.total_cmp(&b.0));
        // Pass 1: the circle of radius L (no ground), smallest margin.
        // Entries: (margin or its lower bound, position, wire point,
        // centre-line distance, worked out: found rather than stopped at
        // the limit).
        let mut best: Option<Fall> = None;
        let mut entries: Vec<(f64, usize, P3, f64, bool)> = Vec::new();
        let order = std::mem::take(scratch);
        for (n, &(lb, k)) in order.iter().enumerate() {
            let limit = best.map_or(cutoff, |b| b.margin.min(cutoff));
            if lb >= limit {
                // The rest: their bounds only.
                entries.extend(
                    order[n..]
                        .iter()
                        .map(|&(lb, k)| (lb, k, tree.base, f64::NAN, false)),
                );
                break;
            }
            let shape = &geo.positions[k].shape;
            // The limit in centre-line distance.
            let to_d = limit - self.margin_from(tree, h, k, 0.0);
            if tree.root > 0.0 {
                // A tighter bound first: the hinge circle is within b_r of
                // the base.
                let (d0, _) = shape.nearest_axis(tree.base, tb);
                if d0 - tree.root >= to_d {
                    let m = self.margin_from(tree, h, k, d0 - tree.root);
                    entries.push((m, k, tree.base, f64::NAN, false));
                    continue;
                }
            }
            work.distances += 1;
            let (d, q) = shape.nearest_circle(tree.base, tree.root, tb, to_d);
            let m = self.margin_from(tree, h, k, d);
            let found = tree.root == 0.0 || m < limit;
            entries.push((m, k, q, d, found));
            if best.is_none_or(|b| m < b.margin) {
                best = Some(Fall {
                    margin: m,
                    position: k,
                    point: q,
                    bound_only: true,
                });
            }
        }
        *scratch = order;
        let floor = entries.iter().map(|e| e.0).fold(outside, f64::min);
        let Some(mut best) = best else {
            return Some(Fall {
                margin: floor,
                ..bound
            });
        };
        if best.margin >= cutoff {
            best.margin = best.margin.min(floor);
            return Some(best);
        }
        let Some(ground) = ground else {
            best.bound_only = false;
            return Some(best);
        };
        // Pass 2: the sector, resting on the ground. The circle's margin
        // bounds it from below; positions in that order until the bound
        // passes the best found (those only bounded in pass 1 as well: a
        // sheltered position can leave another one the nearest).
        entries.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut exact: Option<Fall> = None;
        for (m, k, q, d, found) in entries {
            if exact.is_some_and(|e| m >= e.margin) {
                break;
            }
            let shape = &geo.positions[k].shape;
            let (d, q) = if found {
                (d, q)
            } else {
                work.distances += 1;
                let (d, q) = shape.nearest_circle(tree.base, tree.root, tb, f64::INFINITY);
                if exact.is_some_and(|e| self.margin_from(tree, h, k, d) >= e.margin) {
                    continue;
                }
                (d, q)
            };
            work.sectors += 1;
            let (rho, zeta, dir) = tree.plane(q);
            let theta = tree.theta_min(ground, dir);
            let value = if tree.in_sector(rho, zeta, theta) {
                Fall {
                    margin: self.margin_from(tree, h, k, d),
                    position: k,
                    point: q,
                    bound_only: false,
                }
            } else {
                work.sheltered += 1;
                self.sheltered_fall(ground, tree, k)
            };
            if exact.is_none_or(|e| value.margin < e.margin) {
                exact = Some(value);
            }
        }
        exact
    }

    /// The fall of `tree` onto position `k` whose nearest point lies
    /// outside the sector's angles: the smallest of the sector distance
    /// along its wire, by the bound of a distance (it changes no faster
    /// than the wire point moves), each piece cut until its bound is
    /// within [`CIRCLE_TOLERANCE`] of the best (the bound reported). A
    /// swing arc is taken by its circle value (**Conservative**: the
    /// ground's shelter is not credited on an arc).
    fn sheltered_fall(&self, ground: &Terrain, tree: &Tree, k: usize) -> Fall {
        let p = self.params;
        let shape = &self.geo.positions[k].shape;
        let margin = |d: f64| shape.off_surface(d) - p.fall_allowance;
        let Shape::Wire(w) = shape else {
            let span = &self.geo.spans[self.geo.positions[k].span];
            let (d, q) = shape.nearest_circle(
                tree.base,
                tree.root,
                span.axis.along(tree.base),
                f64::INFINITY,
            );
            return Fall {
                margin: margin(d - tree.length),
                position: k,
                point: q,
                bound_only: false,
            };
        };
        let f = |q: P3| {
            let (rho, zeta, dir) = tree.plane(q);
            let theta = tree.theta_min(ground, dir);
            tree.fall_distance(rho, zeta, theta)
        };
        let pts = &w.wire.points;
        // Pieces by their bound: base distance less the root and L.
        let mut pieces: Vec<(f64, usize)> = pts
            .windows(2)
            .enumerate()
            .map(|(i, s)| {
                let (q, _) = closest_on_segment(tree.base, s[0], s[1]);
                (distance(tree.base, q) - tree.root - tree.length, i)
            })
            .collect();
        pieces.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut best = (f64::INFINITY, pts[0]);
        let mut bound = f64::INFINITY;
        let mut evaluations = 0usize;
        for (lb, i) in pieces {
            if lb >= best.0 {
                break;
            }
            let (a, b) = (pts[i], pts[i + 1]);
            let len = distance(a, b);
            let at = |t: f64| {
                [
                    a[0] + t * (b[0] - a[0]),
                    a[1] + t * (b[1] - a[1]),
                    a[2] + t * (b[2] - a[2]),
                ]
            };
            let (fa, fb) = (f(a), f(b));
            for (v, q) in [(fa, a), (fb, b)] {
                if v < best.0 {
                    best = (v, q);
                }
            }
            let mut open = vec![(0.0, 1.0, fa, fb)];
            while let Some((t0, t1, f0, f1)) = open.pop() {
                let w = (t1 - t0) * len;
                let low = (0.5 * (f0 + f1) - 0.5 * w).max(lb);
                if low >= best.0 - CIRCLE_TOLERANCE || evaluations > 20_000 {
                    bound = bound.min(low);
                    continue;
                }
                let tm = 0.5 * (t0 + t1);
                let qm = at(tm);
                let fm = f(qm);
                evaluations += 1;
                if fm < best.0 {
                    best = (fm, qm);
                }
                open.push((t0, tm, f0, fm));
                open.push((tm, t1, fm, f1));
            }
        }
        let d = best.0.min(bound);
        Fall {
            margin: margin(d),
            position: k,
            point: best.1,
            bound_only: false,
        }
    }
}

impl VegetationCheck {
    /// Takes one point into account, with the ground below it for the
    /// fall-in check.
    pub fn visit(&mut self, ground: Option<&Terrain>, x: f64, y: f64, z: f64, class: u8) {
        if !self.classes[class as usize] {
            return;
        }
        self.points += 1;
        let p = [x, y, z];
        let geo: &Positions = &self.geometry;
        let index: &SpanIndex = &self.index;
        let params = &self.params;
        let fixed = Fixed { geo, params };
        let growth = params.growth();
        let near_reach = params.near_reach();
        let mut g = ground.and_then(|t| t.height(x, y));
        if params.fall_in
            && let (Some(t), Some(_), Some(limit)) = (ground, g, params.max_base_ground_distance)
            && !ground_point_within(t, x, y, limit)
        {
            g = None;
            self.ground_too_far += 1;
        }
        if params.fall_in && g.is_none() {
            self.without_ground += 1;
        }
        let height = g.map(|g| z - g);
        let is_tree = params.fall_in
            && height.is_some_and(|h| h >= params.min_tree_height && h <= params.max_tree_height);
        let base = [x, y, g.unwrap_or(z)];
        let tree =
            is_tree.then(|| Tree::new(base, height.unwrap_or(0.0) + growth, params.root_ball));
        let fall_reach = params.fall_reach();
        let mut touched = std::mem::take(&mut self.touched);
        touched.clear();
        for &sp in index.near(x, y) {
            let sp = sp as usize;
            self.work.pairs += 1;
            let s = &geo.spans[sp];
            // Grow-in: the capsule, then each position whose bound can
            // still beat the best.
            let ad = s.axis.distance(p);
            let mut grow = (f64::INFINITY, p, None);
            let pp = s.axis.polar(p);
            if ad - s.capsule <= near_reach
                && s.bands.lower(&pp, near_reach + s.outer) - s.outer <= near_reach
            {
                let t = pp.along;
                for &k in &s.positions {
                    let pos = &geo.positions[k];
                    let outer = pos.shape.outer();
                    let lower = pos.bands.lower(&pp, near_reach.min(grow.0) + outer) - outer;
                    if lower > near_reach || lower >= grow.0 {
                        continue;
                    }
                    self.work.distances += 1;
                    let (d, q) = pos.shape.nearest_surface(p, t, near_reach);
                    if d < grow.0 {
                        grow = (d, q, Some(k));
                    }
                }
            }
            // Fall-in: up to the fall reach of the capsule.
            let mut fall = None;
            if let Some(tree) = &tree
                && s.axis.distance(base) - s.capsule <= fall_reach
            {
                let worst = self.results[sp]
                    .worst_fall
                    .and_then(|v| v.fall_margin)
                    .unwrap_or(f64::INFINITY);
                // Exactly worked out when it may count (< 0) or be the
                // worst fall; the nearest and the worst list are made
                // exact below.
                let cutoff = worst.max(0.0);
                fall = fixed.fall(
                    &mut self.work,
                    &mut self.scratch,
                    ground,
                    tree,
                    height.unwrap_or(0.0),
                    sp,
                    cutoff,
                );
            }
            if grow.0.is_finite() || fall.is_some() {
                touched.push((sp, grow, fall));
            }
        }
        for &mut (sp, (d, q, position), ref mut fall) in &mut touched {
            let encroaching = d - growth - params.buffer < params.clearance;
            let r = &self.results[sp];
            let becomes_nearest = d.is_finite() && r.nearest.is_none_or(|n| d < n.distance);
            let reported = becomes_nearest || (encroaching && params.keep > 0);
            if reported
                && let (Some(f), Some(tree)) = (*fall, &tree)
                && f.bound_only
            {
                *fall = fixed.fall(
                    &mut self.work,
                    &mut self.scratch,
                    ground,
                    tree,
                    height.unwrap_or(0.0),
                    sp,
                    f64::INFINITY,
                );
            }
            let fall_margin = fall.map(|f| f.margin);
            let vp = VegPoint {
                point: p,
                class,
                distance: d,
                wire_point: q,
                position,
                height,
                fall_margin,
                fall_position: fall.map(|f| f.position),
                fall_point: fall.map(|f| f.point),
                bound_only: fall.is_some_and(|f| f.bound_only),
            };
            let r = &mut self.results[sp];
            let falls = fall_margin.is_some_and(|f| f < 0.0);
            r.encroaching += u64::from(encroaching);
            r.fall_in += u64::from(falls);
            if becomes_nearest {
                r.nearest = Some(vp);
            }
            if let Some(f) = fall_margin
                && !vp.bound_only
                && r.worst_fall
                    .is_none_or(|n| n.fall_margin.is_none_or(|m| f < m))
            {
                r.worst_fall = Some(vp);
            }
            if (encroaching || falls) && params.keep > 0 {
                r.worst.push(vp);
                if r.worst.len() >= 4 * params.keep {
                    r.worst.sort_by(|a, b| a.order(b, params));
                    r.worst.truncate(params.keep);
                }
            }
        }
        self.touched = touched;
    }

    /// Takes `items` into account in parallel blocks of [`BLOCK`] points
    /// (each item a point by `point`), on every core; results merged per
    /// span. Exit clause: `budget` (its time limit and cancel token,
    /// checked between blocks): an error, never a partial result.
    pub fn visit_parallel<T: Sync>(
        &mut self,
        ground: Option<&Terrain>,
        items: &[T],
        point: impl Fn(&T) -> (f64, f64, f64, u8) + Sync,
        budget: &Budget,
    ) -> Result<(), String> {
        let blocks = items.len().div_ceil(BLOCK);
        if blocks == 0 {
            return Ok(());
        }
        let threads = std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(blocks)
            .max(1);
        let next = AtomicUsize::new(0);
        let stop = AtomicBool::new(false);
        let clock = budget.start();
        let template = self.fresh();
        let mut parts: Vec<Result<VegetationCheck, String>> = Vec::new();
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..threads)
                .map(|_| {
                    let mut check = template.fresh();
                    let (next, stop, clock, point) = (&next, &stop, &clock, &point);
                    scope.spawn(move || {
                        loop {
                            if stop.load(Ordering::Relaxed) {
                                return Ok(check);
                            }
                            if let Err(e) = clock.check() {
                                stop.store(true, Ordering::Relaxed);
                                return Err(format!("vegetation check stopped: {e}"));
                            }
                            let b = next.fetch_add(1, Ordering::Relaxed);
                            if b >= blocks {
                                return Ok(check);
                            }
                            for item in &items[b * BLOCK..((b + 1) * BLOCK).min(items.len())] {
                                let (x, y, z, c) = point(item);
                                check.visit(ground, x, y, z, c);
                            }
                        }
                    })
                })
                .collect();
            for h in handles {
                parts.push(h.join().unwrap_or_else(|_| Err("a thread failed".into())));
            }
        });
        let mut merged = Vec::new();
        for p in parts {
            merged.push(p?);
        }
        for p in merged {
            self.merge(p);
        }
        Ok(())
    }

    /// The results, one per span (in the order of the positions' spans).
    pub fn finish(mut self) -> Vec<WireVegetation> {
        let p = &self.params;
        for r in &mut self.results {
            r.worst.sort_by(|a, b| a.order(b, p));
            r.worst.truncate(p.keep);
        }
        self.results
    }
}

/// Header of [`to_csv`].
pub const CSV_HEADER: &str = "case,section,span,kind,encroaching,fall_in,class,x,y,z,distance_m,height_m,fall_margin_m,growth_m,margin_m\n";

/// One row per span with vegetation within the search, and its worst
/// points, as CSV (the form before the danger-tree positions: one case).
pub fn to_csv(
    case: &str,
    wires: &[Wire],
    results: &[WireVegetation],
    p: &VegetationParams,
) -> String {
    use std::fmt::Write as _;
    let mut out = String::from(CSV_HEADER);
    let f = |v: Option<f64>| v.map_or(String::new(), |v| format!("{v:.3}"));
    for (w, r) in wires.iter().zip(results) {
        for (kind, v) in rows(r) {
            let _ = writeln!(
                out,
                "{},{},{},{kind},{},{},{},{:.3},{:.3},{:.3},{},{},{},{},{:.3}",
                ohl_core::text::csv_field(case),
                ohl_core::text::csv_field(&w.section),
                w.span,
                r.encroaching,
                r.fall_in,
                v.class,
                v.point[0],
                v.point[1],
                v.point[2],
                f(v.distance.is_finite().then_some(v.distance)),
                f(v.height),
                f(v.fall_margin),
                f(Some(p.growth())),
                v.margin(p),
            );
        }
    }
    out
}

fn rows(r: &WireVegetation) -> Vec<(&'static str, &VegPoint)> {
    r.nearest
        .iter()
        .map(|v| ("nearest", v))
        .chain(r.worst_fall.iter().map(|v| ("worst_fall", v)))
        .chain(r.worst.iter().map(|v| ("worst", v)))
        .collect()
}

/// Header of [`danger_csv`].
pub const DANGER_CSV_HEADER: &str = "section,span,circuit,phase,kind,grow_in,fall_in,class,x,y,z,distance_m,grow_case,grow_position,height_m,fall_margin_m,fall_case,fall_position,margin_m\n";

/// The danger-tree results as CSV: per span its counts, and its nearest
/// point (the worst grow-in), its worst fall-in and its worst points,
/// each with the case and position it was found in.
pub fn danger_csv(
    positions: &Positions,
    results: &[WireVegetation],
    p: &VegetationParams,
) -> String {
    use std::fmt::Write as _;
    let mut out = String::from(DANGER_CSV_HEADER);
    let f = |v: Option<f64>| v.map_or(String::new(), |v| format!("{v:.3}"));
    let place = |k: Option<usize>| {
        k.map_or((String::new(), String::new()), |k| {
            let pos = &positions.positions[k];
            (
                ohl_core::text::csv_field(&positions.cases[pos.case]),
                pos.kind.name().to_owned(),
            )
        })
    };
    for (s, r) in positions.spans.iter().zip(results) {
        for (kind, v) in rows(r) {
            let (gc, gp) = place(v.position.filter(|_| v.distance.is_finite()));
            let (fc, fp) = place(v.fall_position);
            let _ = writeln!(
                out,
                "{},{},{},{},{kind},{},{},{},{:.3},{:.3},{:.3},{},{gc},{gp},{},{},{fc},{fp},{:.3}",
                ohl_core::text::csv_field(&s.wire.section),
                s.wire.span,
                ohl_core::text::csv_field(&s.wire.circuit),
                ohl_core::text::csv_field(&s.wire.phase),
                r.encroaching,
                r.fall_in,
                v.class,
                v.point[0],
                v.point[1],
                v.point[2],
                f(v.distance.is_finite().then_some(v.distance)),
                f(v.height),
                f(v.fall_margin),
                v.margin(p),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ohl_terrain::SurveyPoint;

    /// The check as before the danger trees' defaults (2026-10-20): no
    /// root ball, no allowance, no limit to a base's ground point — what
    /// the tests below were written for.
    fn plain() -> VegetationParams {
        VegetationParams {
            root_ball: 0.0,
            fall_allowance: 0.0,
            max_base_ground_distance: None,
            ..VegetationParams::default()
        }
    }

    /// The defaults are the utility practice: k = 0.05, A = 2 m, a ground
    /// point within 5 m of a tree base, a 50 m search distance.
    #[test]
    fn the_defaults_are_the_utility_practice() {
        let p = VegetationParams::default();
        assert_eq!(p.root_ball, 0.05);
        assert_eq!(p.fall_allowance, 2.0);
        assert_eq!(p.max_base_ground_distance, Some(5.0));
        assert_eq!(DEFAULT_SEARCH_DISTANCE, 50.0);
    }

    fn level_wire() -> Wire {
        Wire {
            section: "S".into(),
            span: 0,
            circuit: "c".into(),
            phase: "L1".into(),
            potential: None,
            bundle: vec![],
            points: (0..=50).map(|k| [k as f64 * 6.0, 0.0, 20.0]).collect(),
            ends: vec![],
            surface_radius: 0.0,
            polyline_error: 0.0,
        }
    }

    fn flat(z: f64) -> Terrain {
        let p = |x: f64, y: f64| SurveyPoint {
            x,
            y,
            z,
            code: String::new(),
            line: String::new(),
        };
        Terrain::from_points(
            &[
                p(-100.0, -100.0),
                p(400.0, -100.0),
                p(400.0, 100.0),
                p(-100.0, 100.0),
            ],
            None,
        )
        .unwrap()
    }

    #[test]
    fn encroaching_growing_and_falling_trees() {
        let ground = flat(0.0);
        let params = VegetationParams {
            clearance: 4.0,
            growth_per_year: 0.5,
            years: 2.0,
            fall_in: true,
            ..plain()
        };
        let mut check = VegetationCheck::new(vec![level_wire()], &[5], params.clone());
        // A tree top 3 m below the wire: encroaching (3 < 4 + 1).
        check.visit(Some(&ground), 50.0, 0.0, 17.0, 5);
        // 4.5 m to the side at wire height: encroaching only after growth;
        // a 20 m tree whose base is √(4.5² + 20²) = 20.5 m from the wire
        // falls in once grown (margin −0.5).
        check.visit(Some(&ground), 100.0, 4.5, 20.0, 5);
        // 7 m to the side: clear (7 − 1 > 4); 7 m from the wire's foot
        // line but a 15 m tree whose base is √(7² + 20²) = 21.2 m from
        // the wire: no fall-in.
        check.visit(Some(&ground), 150.0, 7.0, 15.0, 5);
        // A 25 m tree 12 m to the side: base √(12² + 20²) = 23.3 m from
        // the wire, < 25 + 1: falls in, margin −2.68.
        check.visit(Some(&ground), 200.0, 12.0, 25.0, 5);
        // Another class: ignored.
        check.visit(Some(&ground), 50.0, 0.0, 19.0, 2);
        assert_eq!(check.points, 4);
        let r = &check.finish()[0];
        assert_eq!((r.encroaching, r.fall_in), (2, 2));
        assert!((r.nearest.unwrap().distance - 3.0).abs() < 1e-9);
        let f = r.worst_fall.unwrap();
        assert!((f.fall_margin.unwrap() - (12f64.hypot(20.0) - 26.0)).abs() < 1e-9);
        assert_eq!(f.height, Some(25.0));
        // Worst first: the falling tree (−2.68), then the tree below the
        // wire (3 − 1 − 4 = −2), then the 20 m tree (−0.5).
        let margins: Vec<f64> = r.worst.iter().map(|v| v.margin(&params)).collect();
        assert_eq!(margins.len(), 3);
        assert!(margins.windows(2).all(|w| w[0] <= w[1]), "{margins:?}");
        assert!((margins[0] - (12f64.hypot(20.0) - 26.0)).abs() < 1e-9);
        let csv = to_csv("hot", &[level_wire()], std::slice::from_ref(r), &params);
        assert_eq!(csv.lines().count(), 1 + 2 + 3);
    }

    /// The same trees with a root ball k = 0.1: the 25 m tree (grown 26,
    /// b_r = 2.6, L = √(26² + 2.6²) = 26.1297) turns about a hinge 2.6 m
    /// nearer the wire: the hinge circle comes within √((12 − 2.6)² +
    /// 20²) = 22.0989 of the wire, margin 22.0989 − 26.1297 = −4.0308
    /// (the hinge on the wire's side, the wire point straight across).
    /// The 15 m tree 7 m out (grown 16, b_r 1.6, L 16.0798):
    /// √(5.4² + 20²) − 16.0798 = 4.6364, still clear.
    #[test]
    fn a_root_ball_brings_trees_nearer() {
        let ground = flat(0.0);
        let params = VegetationParams {
            clearance: 4.0,
            growth_per_year: 0.5,
            years: 2.0,
            fall_in: true,
            root_ball: 0.1,
            ..plain()
        };
        let mut check = VegetationCheck::new(vec![level_wire()], &[5], params.clone());
        check.visit(Some(&ground), 150.0, 7.0, 15.0, 5);
        check.visit(Some(&ground), 200.0, 12.0, 25.0, 5);
        let r = &check.finish()[0];
        assert_eq!(r.fall_in, 1);
        let f = r.worst_fall.unwrap();
        let expect = 9.4f64.hypot(20.0) - 26f64.hypot(2.6);
        let got = f.fall_margin.unwrap();
        assert!(
            got <= expect + 1e-9 && expect - got <= CIRCLE_TOLERANCE,
            "{got} {expect}"
        );
        assert!((expect + 4.0308).abs() < 1e-4);
    }

    /// The fall allowance A and the buffer b move the margins by their
    /// size: the falling tree's margin by −A, the nearest's by −b.
    #[test]
    fn allowance_and_buffer() {
        let ground = flat(0.0);
        let base = VegetationParams {
            clearance: 4.0,
            fall_in: true,
            ..plain()
        };
        let run = |p: &VegetationParams| {
            let mut c = VegetationCheck::new(vec![level_wire()], &[5], p.clone());
            c.visit(Some(&ground), 200.0, 12.0, 25.0, 5);
            c.visit(Some(&ground), 50.0, 0.0, 15.5, 5);
            c.finish().remove(0)
        };
        let a = run(&base);
        let b = run(&VegetationParams {
            fall_allowance: 1.5,
            buffer: 0.7,
            ..base.clone()
        });
        let fa = a.worst_fall.unwrap().fall_margin.unwrap();
        let fb = b.worst_fall.unwrap().fall_margin.unwrap();
        assert!((fa - fb - 1.5).abs() < 1e-12);
        // 4.5 m below the wire: clear of 4 m, encroaching on 4 + 0.7.
        assert_eq!((a.encroaching, b.encroaching), (0, 1));
    }

    /// With k = 0, no allowance and no buffer the results are bit for bit
    /// those of the check before the danger trees (2026-10-20), written
    /// out here as it was (every piece of the wire looked at): a forest
    /// beside a bundle span on level ground and on a gentle slope.
    #[test]
    fn root_ball_zero_keeps_the_results_bit_for_bit() {
        let mut wire = level_wire();
        wire.bundle = vec![[-0.2, 0.0], [0.2, 0.0]];
        wire.surface_radius = 0.0141;
        wire.polyline_error = 0.003;
        let params = VegetationParams {
            clearance: 4.0,
            growth_per_year: 0.4,
            years: 3.0,
            fall_in: true,
            keep: 25,
            ..plain()
        };
        let sloped = {
            let p = |x: f64, y: f64| SurveyPoint {
                x,
                y,
                z: 0.05 * x,
                code: String::new(),
                line: String::new(),
            };
            Terrain::from_points(
                &[
                    p(-100.0, -100.0),
                    p(400.0, -100.0),
                    p(400.0, 100.0),
                    p(-100.0, 100.0),
                ],
                None,
            )
            .unwrap()
        };
        for ground in [flat(0.0), sloped] {
            let mut s = 99u64;
            let mut rnd = || {
                s = s
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                (s >> 11) as f64 / (1u64 << 53) as f64
            };
            let pts: Vec<P3> = (0..20_000)
                .map(|_| {
                    let (x, y) = (rnd() * 300.0, rnd() * 70.0 - 35.0);
                    [x, y, ground.height(x, y).unwrap() + rnd() * 24.0]
                })
                .collect();
            let mut check = VegetationCheck::new(vec![wire.clone()], &[5], params.clone());
            for p in &pts {
                check.visit(Some(&ground), p[0], p[1], p[2], 5);
            }
            let new = check.finish().remove(0);
            let old = before(&wire, &ground, &pts, &params);
            assert_eq!(
                (new.encroaching, new.fall_in),
                (old.encroaching, old.fall_in)
            );
            let key = |v: &VegPoint| (v.point, v.distance, v.wire_point, v.height, v.fall_margin);
            assert_eq!(new.nearest.map(|v| key(&v)), old.nearest.map(|v| key(&v)));
            assert_eq!(
                new.worst_fall.map(|v| key(&v)),
                old.worst_fall.map(|v| key(&v))
            );
            let list = |r: &WireVegetation| -> Vec<_> { r.worst.iter().map(key).collect() };
            assert_eq!(list(&new), list(&old));
            assert!(
                new.fall_in > 100 && new.encroaching > 10,
                "{} {}",
                new.fall_in,
                new.encroaching
            );
        }
    }

    /// The vegetation check of one wire as it was before 2026-10-20.
    fn before(
        wire: &Wire,
        ground: &Terrain,
        pts: &[P3],
        params: &VegetationParams,
    ) -> WireVegetation {
        let growth = params.growth();
        let near_reach = params.clearance + growth + SEARCH_MARGIN;
        let (radius, off) = (wire.bundle_radius(), wire.allowance());
        let mut r = WireVegetation::default();
        for &p in pts {
            let g = ground.height(p[0], p[1]);
            let height = g.map(|g| p[2] - g);
            let tree = params.fall_in
                && height
                    .is_some_and(|h| h >= params.min_tree_height && h <= params.max_tree_height);
            let base = [p[0], p[1], g.unwrap_or(p[2])];
            let (mut d, mut q, mut db) = (f64::INFINITY, p, f64::INFINITY);
            for s in wire.points.windows(2) {
                let (a, b) = (s[0], s[1]);
                let (qq, _) = closest_on_segment(p, a, b);
                let mut e = (f64::INFINITY, qq);
                if distance(p, qq) - radius - off <= near_reach {
                    e = if radius > 0.0 {
                        wire.nearest(p, a, b)
                    } else {
                        (distance(p, qq), qq)
                    };
                    e.0 -= off;
                }
                if e.0 < d {
                    (d, q) = e;
                }
                if tree {
                    let (qb, _) = closest_on_segment(base, a, b);
                    db = db.min(distance(base, qb) - radius - off);
                }
            }
            let fall_margin = (tree && db.is_finite()).then(|| db - height.unwrap() - growth);
            let vp = VegPoint {
                point: p,
                class: 5,
                distance: d,
                wire_point: q,
                position: None,
                height,
                fall_margin,
                fall_position: None,
                fall_point: None,
                bound_only: false,
            };
            let encroaching = d - growth < params.clearance;
            let falls = fall_margin.is_some_and(|f| f < 0.0);
            r.encroaching += u64::from(encroaching);
            r.fall_in += u64::from(falls);
            if d.is_finite() && r.nearest.is_none_or(|n| d < n.distance) {
                r.nearest = Some(vp);
            }
            if let Some(f) = fall_margin
                && r.worst_fall
                    .is_none_or(|n| n.fall_margin.is_none_or(|m| f < m))
            {
                r.worst_fall = Some(vp);
            }
            if encroaching || falls {
                r.worst.push(vp);
            }
        }
        r.worst.sort_by(|a, b| a.order(b, params));
        r.worst.truncate(params.keep);
        r
    }

    /// The ground under a tree is taken only with a ground point within
    /// the distance: the ground's points are the corners at ±100 m and one
    /// at (150, 4); a 25 m tree at (150, 0) has it 4 m away (base found
    /// with a 5 m limit), at (150, −2) 6 m away (no base, counted).
    #[test]
    fn a_tree_base_needs_a_ground_point_near() {
        let p = |x: f64, y: f64| SurveyPoint {
            x,
            y,
            z: 0.0,
            code: String::new(),
            line: String::new(),
        };
        let ground = Terrain::from_points(
            &[
                p(-100.0, -100.0),
                p(400.0, -100.0),
                p(400.0, 100.0),
                p(-100.0, 100.0),
                p(150.0, 4.0),
            ],
            None,
        )
        .unwrap();
        let params = VegetationParams {
            clearance: 4.0,
            fall_in: true,
            max_base_ground_distance: Some(5.0),
            ..plain()
        };
        let mut found = VegetationCheck::new(vec![level_wire()], &[5], params.clone());
        found.visit(Some(&ground), 150.0, 0.0, 25.0, 5);
        assert_eq!((found.without_ground, found.ground_too_far), (0, 0));
        let r = found.finish().remove(0);
        assert_eq!(r.fall_in, 1);
        let mut far = VegetationCheck::new(vec![level_wire()], &[5], params.clone());
        far.visit(Some(&ground), 150.0, -2.0, 25.0, 5);
        assert_eq!((far.without_ground, far.ground_too_far), (1, 1));
        let r = far.finish().remove(0);
        assert_eq!(r.fall_in, 0);
        assert!(r.nearest.is_some_and(|v| v.height.is_none()));
        // No limit (the default): the base is found.
        let mut any = VegetationCheck::new(
            vec![level_wire()],
            &[5],
            VegetationParams {
                max_base_ground_distance: None,
                ..params
            },
        );
        any.visit(Some(&ground), 150.0, -2.0, 25.0, 5);
        assert_eq!(any.finish().remove(0).fall_in, 1);
    }

    /// Parallel blocks find what one pass finds.
    #[test]
    fn parallel_blocks_find_the_same() {
        let ground = flat(0.0);
        let params = VegetationParams {
            clearance: 4.0,
            growth_per_year: 0.3,
            years: 3.0,
            fall_in: true,
            root_ball: 0.1,
            ..plain()
        };
        // A pseudo-random forest beside the span.
        let mut pts = Vec::new();
        let mut s = 12345u64;
        let mut rnd = || {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (s >> 11) as f64 / (1u64 << 53) as f64
        };
        for _ in 0..(BLOCK + 5000) {
            pts.push([rnd() * 300.0, rnd() * 80.0 - 40.0, rnd() * 30.0]);
        }
        let mut one = VegetationCheck::new(vec![level_wire()], &[5], params.clone());
        for p in &pts {
            one.visit(Some(&ground), p[0], p[1], p[2], 5);
        }
        let mut par = VegetationCheck::new(vec![level_wire()], &[5], params.clone());
        par.visit_parallel(
            Some(&ground),
            &pts,
            |p| (p[0], p[1], p[2], 5),
            &Budget::new(),
        )
        .unwrap();
        assert_eq!(one.points, par.points);
        let (a, b) = (one.finish().remove(0), par.finish().remove(0));
        assert_eq!((a.encroaching, a.fall_in), (b.encroaching, b.fall_in));
        assert_eq!(a.nearest.map(|v| v.distance), b.nearest.map(|v| v.distance));
        assert_eq!(
            a.worst_fall.and_then(|v| v.fall_margin),
            b.worst_fall.and_then(|v| v.fall_margin)
        );
        let m = |r: &WireVegetation| -> Vec<f64> {
            r.worst.iter().map(|v| v.margin(&params)).collect()
        };
        assert_eq!(m(&a), m(&b));
    }
}
