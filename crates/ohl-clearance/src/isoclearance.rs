// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Isoclearance lines (theory: LiDAR analyses › Danger trees ›
//! Isoclearance lines): the clearance of a ground point G = (x, y,
//! z_g(x, y)), its distance to the nearest wire's surface over the
//! positions checked,
//! \\( c(x, y) = \min (d(G, \text{wire}) - r) \\), contoured over the
//! corridor at a chosen interval.
//!
//! c is computed exactly at the nodes of a grid (spacing δ) and contoured
//! by marching squares with linear interpolation between nodes. First on
//! a coarse grid: as c changes by at most the distance moved along the
//! ground (it is a distance), a coarse cell whose corner values all exceed
//! the highest line drawn by more than its diagonal times √(1 + m²) (m:
//! the ground's steepest slope in the cell) cannot hold a drawn line and
//! is not refined. The contours are a drawing; the values at the nodes are
//! exact.

use std::collections::HashMap;

use ohl_core::budget::Budget;
use ohl_terrain::Terrain;

use crate::geometry::P3;
use crate::positions::{Positions, parallel};
use crate::vegetation::SpanIndex;

/// The steepest ground slope taken for the bounds (rise over run, 79°):
/// steeper TIN triangles are taken as this.
///
/// **Simplification:** on ground steeper than 5:1 a coarse cell whose
/// corners are just above the top line could hold a line and not be
/// refined. Such slopes are rare outside TIN artefacts and cliffs.
pub const MAX_SLOPE: f64 = 5.0;

/// What isoclearance lines are drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IsoParams {
    /// Interval between lines, m (e.g. 1 or 5).
    pub interval: f64,
    /// Lines above this clearance are not drawn ("hide above"), m.
    pub max_level: f64,
    /// Grid spacing δ where lines are drawn, m.
    pub spacing: f64,
    /// Spacing of the first, coarse grid, m (a whole number of δ).
    pub coarse: f64,
}

impl Default for IsoParams {
    fn default() -> Self {
        Self {
            interval: 5.0,
            max_level: 40.0,
            spacing: 1.0,
            coarse: 8.0,
        }
    }
}

impl IsoParams {
    /// The levels drawn: 0, interval, 2·interval … up to `max_level`.
    pub fn levels(&self) -> Vec<f64> {
        if !(self.interval > 0.0 && self.max_level >= 0.0) {
            return Vec::new();
        }
        let n = (self.max_level / self.interval + 1e-9).floor() as usize;
        (0..=n.min(10_000))
            .map(|k| k as f64 * self.interval)
            .collect()
    }
}

/// One isoclearance line.
#[derive(Debug, Clone, PartialEq)]
pub struct Contour {
    /// Its clearance, m.
    pub level: f64,
    /// Its points on the ground.
    pub points: Vec<P3>,
    /// Joined back to its start.
    pub closed: bool,
}

/// The lines and the work done.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Isoclearance {
    /// The lines.
    pub contours: Vec<Contour>,
    /// Coarse cells looked at.
    pub coarse_cells: u64,
    /// Coarse cells refined to δ.
    pub refined_cells: u64,
    /// Clearances computed (coarse and fine nodes).
    pub nodes: u64,
    /// The smallest clearance found at a node, m.
    pub lowest: Option<(f64, P3)>,
}

/// The clearance of ground point `g`: the smallest distance to the
/// surface of the positions of the spans `index` gives near it, up to
/// `reach` (infinite beyond).
pub fn clearance_at(positions: &Positions, index: &SpanIndex, g: P3, reach: f64) -> f64 {
    let mut best = f64::INFINITY;
    for &sp in index.near(g[0], g[1]) {
        let s = &positions.spans[sp as usize];
        let ad = s.axis.distance(g);
        if ad - s.capsule > reach.min(best) {
            continue;
        }
        let pg = s.axis.polar(g);
        let t = pg.along;
        for &k in &s.positions {
            let pos = &positions.positions[k];
            let outer = pos.shape.outer();
            let lower = pos.bands.lower(&pg, reach.min(best) + outer) - outer;
            if lower >= best || lower > reach {
                continue;
            }
            let (d, _) = pos.shape.nearest_surface(g, t, reach.min(best));
            best = best.min(d);
        }
    }
    best
}

/// The steepest slope of the ground's triangles within `radius` of
/// (x, y) (rise over run; none: no ground there).
fn steepest(ground: &Terrain, x: f64, y: f64, radius: f64) -> Option<f64> {
    let mut m: Option<f64> = None;
    ground.triangles_near(x, y, radius, |[a, b, c]| {
        if let Some(s) = triangle_slope(a, b, c) {
            m = Some(m.map_or(s, |m: f64| m.max(s)));
        }
    });
    m
}

/// The slope of a triangle's plane (rise over run; none: vertical).
fn triangle_slope(a: P3, b: P3, c: P3) -> Option<f64> {
    let (ux, uy, uz) = (b[0] - a[0], b[1] - a[1], b[2] - a[2]);
    let (vx, vy, vz) = (c[0] - a[0], c[1] - a[1], c[2] - a[2]);
    let (nx, ny, nz) = (uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx);
    (nz.abs() > 0.0).then(|| nx.hypot(ny) / nz.abs())
}

/// Values of a cell's nodes: `n + 1` by `n + 1`, (clearance, ground z),
/// NaN outside the corridor or the ground.
type Nodes = Vec<(f64, f64)>;

/// The isoclearance lines of `positions` over the ground within the
/// corridor (`inside` (x, y); `bounds` its extent in plan), as `params`
/// say. Cells are worked out in parallel. Exit clause: `budget` (its time
/// limit, between rows of coarse cells): an error, never a partial
/// drawing.
pub fn isoclearance(
    positions: &Positions,
    ground: &Terrain,
    bounds: ([f64; 2], [f64; 2]),
    inside: &(dyn Fn(f64, f64) -> bool + Sync),
    params: &IsoParams,
    budget: &Budget,
) -> Result<Isoclearance, String> {
    let levels = params.levels();
    if levels.is_empty() || params.spacing.is_nan() || params.spacing <= 0.0 {
        return Ok(Isoclearance::default());
    }
    let top = *levels.last().expect("levels");
    let n = (params.coarse / params.spacing).round().max(1.0) as i64;
    let delta = params.spacing;
    let coarse = n as f64 * delta;
    // Whole coarse cells over the bounds.
    let (lo, hi) = bounds;
    let (i0, j0) = (
        (lo[0] / coarse).floor() as i64,
        (lo[1] / coarse).floor() as i64,
    );
    let (i1, j1) = (
        (hi[0] / coarse).ceil() as i64,
        (hi[1] / coarse).ceil() as i64,
    );
    if i1 <= i0 || j1 <= j0 {
        return Ok(Isoclearance::default());
    }
    let (cols, rows) = ((i1 - i0) as usize, (j1 - j0) as usize);
    if (cols as u64) * (rows as u64) > 50_000_000 {
        return Err(format!(
            "isoclearance: {cols} × {rows} coarse cells — a corridor this large needs a larger coarse spacing"
        ));
    }
    // Clearances are worked out up to the top line and a coarse cell's
    // largest step along the ground: a node beyond is above the top line,
    // and so is every point of its cells.
    let diag = coarse * std::f64::consts::SQRT_2;
    let mut slope = 0.0f64;
    for t in ground.triangles() {
        let [a, b, c] = t.map(|v| ground.points()[v as usize]);
        if let Some(m) = triangle_slope(a, b, c) {
            slope = slope.max(m);
        }
    }
    // A TIN's near-vertical triangles (artefacts, cliffs) would make the
    // reach huge: slopes above MAX_SLOPE are taken as MAX_SLOPE.
    let slope = slope.min(MAX_SLOPE);
    let reach = top + diag * (1.0 + slope * slope).sqrt() + delta;
    let index = SpanIndex::new(positions, reach, 25f64.max(reach / 4.0));
    let value = |x: f64, y: f64| -> (f64, f64) {
        if !inside(x, y) {
            return (f64::NAN, f64::NAN);
        }
        match ground.height(x, y) {
            Some(z) => (clearance_at(positions, &index, [x, y, z], reach), z),
            None => (f64::NAN, f64::NAN),
        }
    };
    // Coarse nodes, row by row.
    let coarse_rows = parallel(rows + 1, budget, |j| {
        let y = (j0 + j as i64) as f64 * coarse;
        Ok((0..=cols)
            .map(|i| value((i0 + i as i64) as f64 * coarse, y))
            .collect::<Vec<_>>())
    })?;
    // Each row of cells: the cells to refine, refined.
    let cells = parallel(rows, budget, |j| {
        let mut out: Vec<(i64, i64, Nodes)> = Vec::new();
        let mut looked = 0u64;
        for i in 0..cols {
            let corners = [
                coarse_rows[j][i].0,
                coarse_rows[j][i + 1].0,
                coarse_rows[j + 1][i].0,
                coarse_rows[j + 1][i + 1].0,
            ];
            // NaN: outside the corridor or the ground; infinite: beyond
            // the reach (above the top line).
            let known: Vec<f64> = corners.into_iter().filter(|v| !v.is_nan()).collect();
            if known.is_empty() {
                continue;
            }
            looked += 1;
            let (cx, cy) = (
                (i0 + i as i64) as f64 * coarse + 0.5 * coarse,
                (j0 + j as i64) as f64 * coarse + 0.5 * coarse,
            );
            let low = known.iter().copied().fold(f64::INFINITY, f64::min);
            let m = steepest(ground, cx, cy, 0.5 * diag)
                .unwrap_or(0.0)
                .min(MAX_SLOPE);
            if low - diag * (1.0 + m * m).sqrt() > top {
                continue;
            }
            // Refined: the cell's (n + 1)² nodes.
            let (gx, gy) = ((i0 + i as i64) * n, (j0 + j as i64) * n);
            let mut nodes = Vec::with_capacity(((n + 1) * (n + 1)) as usize);
            for b in 0..=n {
                for a in 0..=n {
                    nodes.push(value((gx + a) as f64 * delta, (gy + b) as f64 * delta));
                }
            }
            out.push((gx, gy, nodes));
        }
        Ok((looked, out))
    })?;
    let mut result = Isoclearance::default();
    for row in &coarse_rows {
        result.nodes += row.len() as u64;
    }
    // Segments per level, keyed by the grid edges they cross.
    let mut segments: Vec<Vec<Segment>> = vec![Vec::new(); levels.len()];
    let lowest = |r: &mut Isoclearance, v: f64, p: P3| {
        if v.is_finite() && r.lowest.is_none_or(|(l, _)| v < l) {
            r.lowest = Some((v, p));
        }
    };
    for (looked, refined) in &cells {
        result.coarse_cells += looked;
        for (gx, gy, nodes) in refined {
            result.refined_cells += 1;
            result.nodes += nodes.len() as u64;
            let w = (n + 1) as usize;
            let at = |a: i64, b: i64| nodes[b as usize * w + a as usize];
            let xy = |a: i64, b: i64| [(gx + a) as f64 * delta, (gy + b) as f64 * delta];
            for b in 0..=n {
                for a in 0..=n {
                    let (v, z) = at(a, b);
                    let [x, y] = xy(a, b);
                    lowest(&mut result, v, [x, y, z]);
                }
            }
            for b in 0..n {
                for a in 0..n {
                    let c = [at(a, b), at(a + 1, b), at(a + 1, b + 1), at(a, b + 1)];
                    if c.iter().any(|v| !v.0.is_finite() || !v.1.is_finite()) {
                        continue;
                    }
                    let p = [xy(a, b), xy(a + 1, b), xy(a + 1, b + 1), xy(a, b + 1)];
                    let (ga, gb) = (gx + a, gy + b);
                    // Edges: bottom, right, top, left, with their keys.
                    let edges: [(usize, usize, Key); 4] = [
                        (0, 1, (ga, gb, 0)),
                        (1, 2, (ga + 1, gb, 1)),
                        (3, 2, (ga, gb + 1, 0)),
                        (0, 3, (ga, gb, 1)),
                    ];
                    for (l, &level) in levels.iter().enumerate() {
                        let above: Vec<bool> = c.iter().map(|v| v.0 >= level).collect();
                        let crossing = |e: usize| {
                            let (s, t, key) = edges[e];
                            let (vs, vt) = (c[s].0, c[t].0);
                            let f = (level - vs) / (vt - vs);
                            let point = [
                                p[s][0] + f * (p[t][0] - p[s][0]),
                                p[s][1] + f * (p[t][1] - p[s][1]),
                                c[s].1 + f * (c[t].1 - c[s].1),
                            ];
                            (key, point)
                        };
                        let cut: Vec<usize> = (0..4)
                            .filter(|&e| above[edges[e].0] != above[edges[e].1])
                            .collect();
                        let pairs: Vec<(usize, usize)> = match cut.as_slice() {
                            [a, b] => vec![(*a, *b)],
                            [_, _, _, _] => {
                                // A saddle: the centre's value decides.
                                let centre = 0.25 * (c[0].0 + c[1].0 + c[2].0 + c[3].0);
                                if (centre >= level) == above[0] {
                                    // Corner 0's side joins the centre:
                                    // cut off corners 1 and 3.
                                    vec![(0, 1), (2, 3)]
                                } else {
                                    vec![(0, 3), (1, 2)]
                                }
                            }
                            _ => Vec::new(),
                        };
                        for (e1, e2) in pairs {
                            let (k1, p1) = crossing(e1);
                            let (k2, p2) = crossing(e2);
                            segments[l].push((k1, k2, p1, p2));
                        }
                    }
                }
            }
        }
    }
    for (l, segs) in segments.into_iter().enumerate() {
        result.contours.extend(join(levels[l], segs));
    }
    Ok(result)
}

/// A grid edge: (column, row, 0 along x or 1 along y) of its first node.
type Key = (i64, i64, u8);

/// A piece of a line in one grid cell: the edges it crosses, its ends.
type Segment = (Key, Key, P3, P3);

/// Joins segments sharing grid edges into lines.
fn join(level: f64, segs: Vec<Segment>) -> Vec<Contour> {
    let mut by_key: HashMap<Key, Vec<usize>> = HashMap::new();
    for (k, s) in segs.iter().enumerate() {
        by_key.entry(s.0).or_default().push(k);
        by_key.entry(s.1).or_default().push(k);
    }
    let mut used = vec![false; segs.len()];
    let mut out = Vec::new();
    // Ends first (open lines), then what is left (closed loops).
    let mut starts: Vec<usize> = (0..segs.len())
        .filter(|&k| by_key[&segs[k].0].len() == 1 || by_key[&segs[k].1].len() == 1)
        .collect();
    starts.extend(0..segs.len());
    for start in starts {
        if used[start] {
            continue;
        }
        used[start] = true;
        let s = &segs[start];
        // Walk from the end that is free (or either).
        let (mut key, mut points) = if by_key[&s.0].len() == 1 {
            (s.1, vec![s.2, s.3])
        } else {
            (s.0, vec![s.3, s.2])
        };
        let first_key = if by_key[&s.0].len() == 1 { s.0 } else { s.1 };
        let closed;
        loop {
            let next = by_key[&key].iter().copied().find(|&k| !used[k]);
            let Some(k) = next else {
                closed = key == first_key && points.len() > 2;
                break;
            };
            used[k] = true;
            let t = &segs[k];
            if t.0 == key {
                points.push(t.3);
                key = t.1;
            } else {
                points.push(t.2);
                key = t.0;
            }
        }
        out.push(Contour {
            level,
            points,
            closed,
        });
    }
    out
}

/// The lines as GeoJSON (a FeatureCollection of LineStrings with their
/// `level`, project coordinates, `crs` when known).
pub fn to_geojson(contours: &[Contour], name: &str, crs: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = write!(
        out,
        "{{\"type\":\"FeatureCollection\",\"name\":{}",
        json_text(name)
    );
    if !crs.is_empty() {
        let _ = write!(
            out,
            ",\"crs\":{{\"type\":\"name\",\"properties\":{{\"name\":{}}}}}",
            json_text(crs)
        );
    }
    out.push_str(",\"features\":[");
    for (k, c) in contours.iter().enumerate() {
        if k > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"type\":\"Feature\",\"properties\":{{\"layer\":\"OHL_ISOCLEARANCE\",\"level_m\":{}}},\"geometry\":{{\"type\":\"LineString\",\"coordinates\":[",
            c.level
        );
        for (i, p) in c.points.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = write!(out, "[{:.3},{:.3},{:.3}]", p[0], p[1], p[2]);
        }
        out.push_str("]}}");
    }
    out.push_str("]}\n");
    out
}

fn json_text(s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Header of [`to_csv`].
pub const CSV_HEADER: &str = "level_m,line,vertex,x,y,z\n";

/// The lines as CSV: one row per vertex.
pub fn to_csv(contours: &[Contour]) -> String {
    use std::fmt::Write as _;
    let mut out = String::from(CSV_HEADER);
    for (k, c) in contours.iter().enumerate() {
        for (i, p) in c.points.iter().enumerate() {
            let _ = writeln!(
                out,
                "{},{k},{i},{:.3},{:.3},{:.3}",
                c.level, p[0], p[1], p[2]
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::Wire;
    use ohl_terrain::SurveyPoint;

    fn flat() -> Terrain {
        let p = |x: f64, y: f64| SurveyPoint {
            x,
            y,
            z: 0.0,
            code: String::new(),
            line: String::new(),
        };
        Terrain::from_points(
            &[
                p(-200.0, -200.0),
                p(500.0, -200.0),
                p(500.0, 200.0),
                p(-200.0, 200.0),
            ],
            None,
        )
        .unwrap()
    }

    fn straight(z: f64) -> Wire {
        Wire {
            section: "S".into(),
            span: 0,
            circuit: String::new(),
            phase: String::new(),
            potential: None,
            bundle: vec![],
            points: (0..=30).map(|k| [k as f64 * 10.0, 0.0, z]).collect(),
            ends: vec![],
            surface_radius: 0.0,
            polyline_error: 0.0,
        }
    }

    /// A straight wire 20 m above level ground along x from 0 to 300:
    /// over its middle, c(x, y) = √(y² + 20²); the 25 m line runs at
    /// |y| = 15, the 30 m line at √(30² − 20²) = 22.36. Lines below 20 m
    /// are not there. Placed by interpolation between 1 m nodes: within a
    /// few mm.
    #[test]
    fn contours_of_a_straight_wire() {
        let positions = Positions::of_wires("hot", vec![straight(20.0)]);
        let params = IsoParams {
            interval: 5.0,
            max_level: 30.0,
            spacing: 1.0,
            coarse: 8.0,
        };
        let inside = |x: f64, y: f64| (0.0..=300.0).contains(&x) && y.abs() <= 80.0;
        let r = isoclearance(
            &positions,
            &flat(),
            ([0.0, -80.0], [300.0, 80.0]),
            &inside,
            &params,
            &Budget::new(),
        )
        .unwrap();
        let levels: Vec<f64> = r.contours.iter().map(|c| c.level).collect();
        assert!(
            levels.iter().all(|l| *l == 25.0 || *l == 30.0),
            "{levels:?}"
        );
        for c in &r.contours {
            let y = (c.level * c.level - 400.0).sqrt();
            for p in c.points.iter().filter(|p| p[0] > 5.0 && p[0] < 295.0) {
                assert!((p[1].abs() - y).abs() < 0.01, "{} {p:?}", c.level);
            }
        }
        // Two lines (either side) of each level along the whole span.
        let len = |l: f64| -> f64 {
            r.contours
                .iter()
                .filter(|c| c.level == l)
                .map(|c| {
                    c.points
                        .windows(2)
                        .map(|w| (w[1][0] - w[0][0]).abs())
                        .sum::<f64>()
                })
                .sum()
        };
        assert!(len(25.0) > 2.0 * 290.0, "{}", len(25.0));
        let (low, _) = r.lowest.unwrap();
        assert!((low - 20.0).abs() < 1e-9);
        // Cells far from the line are not refined: c > 30 + 8√2.
        assert!(r.refined_cells < r.coarse_cells, "{r:?}");
        let csv = to_csv(&r.contours);
        assert!(csv.starts_with(CSV_HEADER));
        let gj = to_geojson(&r.contours, "test", "EPSG:2056");
        assert!(gj.contains("\"level_m\":25"));
    }

    /// Refined or not, every node value is the exact clearance: a cell
    /// skipped by the bound holds no value at or below the top line.
    #[test]
    fn skipped_cells_are_above_the_top_line() {
        let positions = Positions::of_wires("hot", vec![straight(20.0)]);
        let index = SpanIndex::new(&positions, 100.0, 25.0);
        for y in [0.0, 10.0, 25.0, 39.0] {
            let c = clearance_at(&positions, &index, [150.0, y, 0.0], 100.0);
            assert!((c - (y * y + 400.0f64).sqrt()).abs() < 1e-12);
        }
    }
}
