// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Where a span's wire can be before it is solved: the bound that picks
//! the candidates a scoped check solves (ADR-0013, phase 10).
//!
//! A wire of length ℓ between its ends E_A and E_B has every point P with
//! |P − E_A| + |P − E_B| ≤ ℓ, so |P − M| ≤ ℓ / 2 for the ends' midpoint M
//! (also in plan, a projection shortening every distance). The ends lie
//! within each structure's radius R (its points and wire points from its
//! position, plus its longest string: a wire point swings at most that
//! far) and at most its height H above its base. **Assumed
//! (conservative):** no wire is longer than [`WIRE_LENGTH_FACTOR`] times
//! the straight line between its ends (a sag of about a half of the span:
//! no overhead line comes near it); a typed structure's points lie within
//! [`TYPED_RADIUS`] of its position and [`TYPED_HEIGHT`] above it (before
//! its type is read). Every candidate solved is checked against the bound
//! ([`SpanChord::holds`]); a wire outside it makes the result say NOT ON
//! THE SAFE SIDE.

use ohl_line::{Line, Point, Section, Structure};
use serde::{Deserialize, Serialize};

/// Assumed (conservative): a wire is at most this many times as long as
/// the straight line between its ends.
pub const WIRE_LENGTH_FACTOR: f64 = 1.5;

/// Assumed (conservative): a typed structure's points within this plan
/// distance of its position, m (its type not read when candidates are
/// picked).
pub const TYPED_RADIUS: f64 = 50.0;

/// Assumed (conservative): a typed structure's points at most this high
/// above its position, m.
pub const TYPED_HEIGHT: f64 = 150.0;

/// A structure's extent: how far its wire ends can be from its position.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Extent {
    /// Its position (base).
    pub position: Point,
    /// In plan: its points and wire points from the position plus its
    /// longest string, m.
    pub radius: f64,
    /// Its highest point above the position, m.
    pub height: f64,
}

/// The extent of `s` (typed: the assumed [`TYPED_RADIUS`] and
/// [`TYPED_HEIGHT`] unless its own points reach further).
pub fn extent_of(s: &Structure) -> Extent {
    let p = s.position;
    let mut radius: f64 = 0.0;
    let mut height: f64 = 0.0;
    let mut take = |q: &Point| {
        if q.iter().all(|v| v.is_finite()) {
            radius = radius.max((q[0] - p[0]).hypot(q[1] - p[1]));
            height = height.max(q[2] - p[2]);
        }
    };
    for q in &s.points {
        take(&q.position);
    }
    for w in &s.wires {
        take(&w.position);
    }
    let string = s
        .strings
        .iter()
        .map(|x| x.length)
        .filter(|l| l.is_finite())
        .fold(0.0, f64::max);
    radius += string;
    if s.structure_type.is_some() {
        radius = radius.max(TYPED_RADIUS);
        height = height.max(TYPED_HEIGHT);
    }
    Extent {
        position: p,
        radius,
        height,
    }
}

/// One span before it is solved: its structures' chord and how far from
/// the chord's midpoint (in plan) any point of its wire can be.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpanChord {
    /// The span's index in its section.
    pub span: usize,
    /// The back structure's position, in plan.
    pub a: [f64; 2],
    /// The ahead structure's position, in plan.
    pub b: [f64; 2],
    /// Every point of the wire within this of the midpoint, in plan, m.
    pub margin: f64,
    /// No point of the wire above this, m: the higher structure's top
    /// (a wire hangs below its higher end) plus [`BUNDLE_ALLOWANCE`].
    pub top: f64,
}

/// Above a structure's highest point, a bundle's subconductors may reach
/// this much, m (assumed, conservative: bundles are a metre across at
/// most).
pub const BUNDLE_ALLOWANCE: f64 = 2.0;

impl SpanChord {
    /// The span between structures of extents `a` and `b`.
    pub fn new(span: usize, a: &Extent, b: &Extent) -> Self {
        let plan = (a.position[0] - b.position[0]).hypot(a.position[1] - b.position[1]);
        let rise = (a.position[2] - b.position[2]).abs() + a.height + b.height;
        let ends = (plan + a.radius + b.radius).hypot(rise);
        Self {
            span,
            a: [a.position[0], a.position[1]],
            b: [b.position[0], b.position[1]],
            margin: 0.5 * WIRE_LENGTH_FACTOR * ends + 0.5 * (a.radius + b.radius),
            top: (a.position[2] + a.height).max(b.position[2] + b.height) + BUNDLE_ALLOWANCE,
        }
    }

    /// The chord's midpoint.
    pub fn mid(&self) -> [f64; 2] {
        [0.5 * (self.a[0] + self.b[0]), 0.5 * (self.a[1] + self.b[1])]
    }

    /// Whether every point of a solved wire lies within the bound (the
    /// assumption checked on what was solved).
    pub fn holds(&self, points: &[Point]) -> bool {
        let m = self.mid();
        points
            .iter()
            .all(|p| (p[0] - m[0]).hypot(p[1] - m[1]) <= self.margin + 1e-6)
    }
}

/// The span chords of `section` in `line` (its structures looked up by
/// `structure`); a span whose structure is missing: none for it.
pub fn chords_of<'a>(
    section: &Section,
    structure: impl Fn(&str) -> Option<&'a Structure>,
) -> Vec<SpanChord> {
    let extents: Vec<Option<Extent>> = section
        .points
        .iter()
        .map(|p| structure(&p.structure).map(extent_of))
        .collect();
    extents
        .windows(2)
        .enumerate()
        .filter_map(|(k, w)| match (w[0], w[1]) {
            (Some(a), Some(b)) => Some(SpanChord::new(k, &a, &b)),
            _ => None,
        })
        .collect()
}

/// The largest margin of every span of `line` (an upper bound for the
/// candidate search), m.
pub fn max_margin(line: &Line) -> f64 {
    let by_id: std::collections::HashMap<&str, &Structure> =
        line.structures.iter().map(|s| (s.id.as_str(), s)).collect();
    line.sections
        .iter()
        .flat_map(|s| chords_of(s, |id| by_id.get(id).copied()))
        .map(|c| c.margin)
        .fold(0.0, f64::max)
}

/// The plan distance between segments `p`–`q` and `c`–`d`.
pub fn segment_distance(p: [f64; 2], q: [f64; 2], c: [f64; 2], d: [f64; 2]) -> f64 {
    if segments_cross(p, q, c, d) {
        return 0.0;
    }
    point_segment(p, c, d)
        .min(point_segment(q, c, d))
        .min(point_segment(c, p, q))
        .min(point_segment(d, p, q))
}

/// The plan distance from point `p` to segment `a`–`b`.
pub fn point_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

fn segments_cross(p: [f64; 2], q: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let orient = |a: [f64; 2], b: [f64; 2], x: [f64; 2]| {
        (b[0] - a[0]) * (x[1] - a[1]) - (b[1] - a[1]) * (x[0] - a[0])
    };
    let (o1, o2) = (orient(p, q, c), orient(p, q, d));
    let (o3, o4) = (orient(c, d, p), orient(c, d, q));
    (o1 * o2 < 0.0) && (o3 * o4 < 0.0)
}

/// Plan segments in a grid, to ask whether anything lies within a
/// distance of a point or a segment.
#[derive(Debug, Clone, Default)]
pub struct SegmentGrid {
    cell: f64,
    cells: std::collections::HashMap<(i64, i64), Vec<usize>>,
    segments: Vec<([f64; 2], [f64; 2])>,
}

impl SegmentGrid {
    /// An empty grid of cells `cell` m wide.
    pub fn new(cell: f64) -> Self {
        Self {
            cell: cell.max(1.0),
            ..Self::default()
        }
    }

    /// Adds the plan segments of polyline `points`.
    pub fn add_polyline(&mut self, points: &[Point]) {
        for w in points.windows(2) {
            self.add([w[0][0], w[0][1]], [w[1][0], w[1][1]]);
        }
        if points.len() == 1 {
            let p = [points[0][0], points[0][1]];
            self.add(p, p);
        }
    }

    /// Adds segment `a`–`b`.
    pub fn add(&mut self, a: [f64; 2], b: [f64; 2]) {
        let k = self.segments.len();
        self.segments.push((a, b));
        let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
        let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
        for i in self.at(x0)..=self.at(x1) {
            for j in self.at(y0)..=self.at(y1) {
                self.cells.entry((i, j)).or_default().push(k);
            }
        }
    }

    fn at(&self, v: f64) -> i64 {
        (v / self.cell).floor() as i64
    }

    /// Whether the grid holds nothing.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// The plan bounds of every segment.
    pub fn bounds(&self) -> Option<ohl_core::grid::Bounds> {
        let mut b: Option<ohl_core::grid::Bounds> = None;
        for (p, q) in &self.segments {
            let s = ohl_core::grid::Bounds::new(*p, *q);
            b = Some(b.map_or(s, |b| b.union(&s)));
        }
        b
    }

    /// The nearest plan distance from segment `a`–`b` to the grid's
    /// segments, looked for up to `within` (beyond: `None`).
    pub fn nearest(&self, a: [f64; 2], b: [f64; 2], within: f64) -> Option<f64> {
        let (x0, x1) = (a[0].min(b[0]) - within, a[0].max(b[0]) + within);
        let (y0, y1) = (a[1].min(b[1]) - within, a[1].max(b[1]) + within);
        let mut best: Option<f64> = None;
        let mut seen = std::collections::HashSet::new();
        for i in self.at(x0)..=self.at(x1) {
            for j in self.at(y0)..=self.at(y1) {
                for &k in self.cells.get(&(i, j)).into_iter().flatten() {
                    if !seen.insert(k) {
                        continue;
                    }
                    let (c, d) = self.segments[k];
                    let dist = segment_distance(a, b, c, d);
                    if dist <= within && best.is_none_or(|x| dist < x) {
                        best = Some(dist);
                    }
                }
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hanging_wire_stays_within_its_bound() {
        let a = Extent {
            position: [0.0, 0.0, 400.0],
            radius: 8.0,
            height: 40.0,
        };
        let b = Extent {
            position: [400.0, 0.0, 420.0],
            radius: 8.0,
            height: 40.0,
        };
        let c = SpanChord::new(0, &a, &b);
        // A catenary sagging 20 m and swung 20 m sideways.
        let wire: Vec<Point> = (0..=40)
            .map(|k| {
                let t = k as f64 / 40.0;
                let sag = 4.0 * 20.0 * t * (1.0 - t);
                [400.0 * t, 8.0 + sag * 0.7, 440.0 + 20.0 * t - sag * 0.7]
            })
            .collect();
        assert!(c.holds(&wire));
        assert!(c.margin > 200.0 && c.margin < 400.0, "{}", c.margin);
        assert!(!c.holds(&[[200.0, 1000.0, 400.0]]));
    }

    #[test]
    fn segments_and_the_grid() {
        assert_eq!(
            segment_distance([0.0, 0.0], [10.0, 0.0], [5.0, -1.0], [5.0, 1.0]),
            0.0
        );
        assert!(
            (segment_distance([0.0, 0.0], [10.0, 0.0], [12.0, 3.0], [12.0, 9.0]) - 13f64.sqrt())
                .abs()
                < 1e-12
        );
        let mut g = SegmentGrid::new(50.0);
        g.add_polyline(&[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [200.0, 50.0, 0.0]]);
        assert_eq!(g.nearest([50.0, 30.0], [50.0, 40.0], 29.0), None);
        assert!((g.nearest([50.0, 30.0], [50.0, 40.0], 31.0).unwrap() - 30.0).abs() < 1e-12);
        assert!(g.nearest([0.0, -500.0], [0.0, 500.0], 1.0).is_some());
    }
}
