// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Routes, alignments, stationing, offsets and line angles.
//!
//! A [`Route`] is a path of the line made of one or more [`Alignment`]s,
//! in their order in the line. An alignment is a polyline of points
//! ([`AlignmentPoint`], PIs) and belongs to one or more routes; where
//! alignments of different routes meet, the shared point (and a structure
//! on it) belongs to all of them.
//!
//! Everything else is put on routes from the alignments
//! ([`Line::structure_routes`]): a structure tied to an alignment point,
//! or within [`NEAR_ALIGNMENT`] of an alignment, is on it (and on its
//! routes); a structure away from every alignment gives its routes itself
//! ([`crate::Structure::routes`]). Sections, jumpers and what hangs on a
//! structure follow their structures.
//!
//! Stations and offsets are horizontal: a structure's position is
//! projected onto its alignment (the closest segment); the offset is
//! positive to the right looking ahead. [`Line::alignment_views`] gives
//! each alignment with its structures in station order.
//!
//! Files of version 2 and before held routes that were their own
//! alignment, listing their structures (or following them, without
//! points), and the line's own alignment: [`Line::upgrade_routes`] turns
//! them into routes and alignments, keeping every structure on its routes.

use serde::{Deserialize, Serialize};

use crate::{Line, LineError, Point};

/// A structure within this plan distance of an alignment (beside it, not
/// beyond its ends) is on it, m.
pub const NEAR_ALIGNMENT: f64 = 5.0;

/// A point of an alignment (PI).
///
/// In files a free point is `[x, y, z]`; otherwise `{"position": [x, y,
/// z], "structure": "T12", "routes": [...], "description": ...}`. A point
/// tied to a structure must lie under it (in plan, within 1 mm).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "AlignmentRepr", into = "AlignmentRepr")]
pub struct AlignmentPoint {
    /// Position.
    pub position: Point,
    /// Structure it is tied to (it follows the structure when it moves).
    pub structure: Option<String>,
    /// What it is, in words (free text).
    pub description: String,
    /// The routes of this point alone (and of a structure tied to it);
    /// none: its alignment's.
    pub routes: Option<Vec<String>>,
}

impl AlignmentPoint {
    /// A free point.
    pub fn free(position: Point) -> Self {
        Self {
            position,
            structure: None,
            description: String::new(),
            routes: None,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum AlignmentRepr {
    Free(Point),
    Tied {
        position: Point,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        structure: Option<String>,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        description: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        routes: Option<Vec<String>>,
    },
}

impl From<AlignmentRepr> for AlignmentPoint {
    fn from(r: AlignmentRepr) -> Self {
        match r {
            AlignmentRepr::Free(position) => Self::free(position),
            AlignmentRepr::Tied {
                position,
                structure,
                description,
                routes,
            } => Self {
                position,
                structure,
                description,
                routes,
            },
        }
    }
}

impl From<AlignmentPoint> for AlignmentRepr {
    fn from(a: AlignmentPoint) -> Self {
        if a.structure.is_none() && a.description.is_empty() && a.routes.is_none() {
            return Self::Free(a.position);
        }
        Self::Tied {
            position: a.position,
            structure: a.structure,
            description: a.description,
            routes: a.routes,
        }
    }
}

/// Positions of alignment points.
pub fn positions(alignment: &[AlignmentPoint]) -> Vec<Point> {
    alignment.iter().map(|a| a.position).collect()
}

/// A route: a path of the line, made of the alignments that belong to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    /// Unique id, e.g. `"route 1"`, `"north"`.
    pub id: String,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// What it is, in words (free text).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Files of version 2: the route was its own alignment (these points)…
    #[serde(default, rename = "alignment", skip_serializing)]
    pub legacy_alignment: Vec<AlignmentPoint>,
    /// …with its structures in order ([`Line::upgrade_routes`]).
    #[serde(default, rename = "structures", skip_serializing)]
    pub legacy_structures: Option<Vec<String>>,
}

impl Route {
    /// A route `id` named `name`.
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            description: String::new(),
            legacy_alignment: Vec::new(),
            legacy_structures: None,
        }
    }

    /// Read from a file of version 2 (not upgraded yet).
    pub fn is_legacy(&self) -> bool {
        self.legacy_structures.is_some() || !self.legacy_alignment.is_empty()
    }
}

/// An alignment: a polyline of points, on one or more routes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Alignment {
    /// Unique id, e.g. `"alignment 2"`.
    pub id: String,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// What it is, in words (free text).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The routes it belongs to (one or more).
    pub routes: Vec<String>,
    /// Its points, in order (one or more; a profile needs two).
    pub points: Vec<AlignmentPoint>,
}

/// An alignment with the structures on it, in station order: what the
/// profile, the drawings and the stationing work on.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignmentView {
    /// The alignment's id.
    pub id: String,
    /// Its name.
    pub name: String,
    /// The routes it belongs to.
    pub routes: Vec<String>,
    /// Its points.
    pub alignment: Vec<AlignmentPoint>,
    /// The structures on it, in station order.
    pub structures: Vec<String>,
}

/// Stationing of one structure on one route.
#[derive(Debug, Clone, PartialEq)]
pub struct Station {
    /// Structure id.
    pub structure: String,
    /// Horizontal distance along the alignment, m.
    pub station: f64,
    /// Horizontal distance from the alignment, m (positive to the right).
    pub offset: f64,
    /// Line angle at the structure, degrees (positive = turning right;
    /// 0 at the ends of the route).
    pub line_angle_deg: f64,
    /// Horizontal distance to the next structure on the route, m (0 at the end).
    pub ahead_span: f64,
}

fn horizontal(a: Point, b: Point) -> (f64, f64) {
    (b[0] - a[0], b[1] - a[1])
}

/// Projects `p` onto a polyline in plan: (station, signed offset, right
/// of the direction positive), to the closest segment.
pub fn project(polyline: &[Point], p: Point) -> (f64, f64) {
    let mut best = (f64::INFINITY, 0.0, 0.0); // (distance², station, offset)
    let mut start = 0.0;
    for w in polyline.windows(2) {
        let (dx, dy) = horizontal(w[0], w[1]);
        let len = dx.hypot(dy);
        if len == 0.0 {
            continue;
        }
        let (ux, uy) = (dx / len, dy / len);
        let (px, py) = horizontal(w[0], p);
        let along = (px * ux + py * uy).clamp(0.0, len);
        let (cx, cy) = (px - along * ux, py - along * uy);
        let d2 = cx * cx + cy * cy;
        if d2 < best.0 {
            // Right of the direction (ux, uy) is (uy, -ux).
            best = (d2, start + along, px * uy - py * ux);
        }
        start += len;
    }
    (best.1, best.2)
}

/// The plan point at `station` along a polyline, `offset` to its right.
pub fn point_at(polyline: &[Point], station: f64, offset: f64) -> [f64; 2] {
    let mut start = 0.0;
    let n = polyline.len();
    for (k, w) in polyline.windows(2).enumerate() {
        let (dx, dy) = horizontal(w[0], w[1]);
        let len = dx.hypot(dy);
        if len == 0.0 {
            continue;
        }
        if station <= start + len || k + 2 == n {
            let (ux, uy) = (dx / len, dy / len);
            let t = station - start;
            return [
                w[0][0] + ux * t + uy * offset,
                w[0][1] + uy * t - ux * offset,
            ];
        }
        start += len;
    }
    polyline.first().map_or([0.0, 0.0], |p| [p[0], p[1]])
}

impl AlignmentView {
    /// The alignment's centreline: its points (`line` is not needed any
    /// more; kept for the callers).
    pub fn centreline(&self, _line: &Line) -> Vec<Point> {
        positions(&self.alignment)
    }

    /// Stationing of every structure of the route.
    pub fn stations(&self, line: &Line) -> Result<Vec<Station>, LineError> {
        let positions: Vec<Point> = self
            .structures
            .iter()
            .map(|id| {
                line.structure(id).map(|s| s.position).ok_or_else(|| {
                    LineError::Invalid(format!("alignment {}: unknown structure {id}", self.id))
                })
            })
            .collect::<Result<_, _>>()?;
        let pis = self::positions(&self.alignment);
        let alignment = &pis;
        let n = positions.len();
        Ok((0..n)
            .map(|k| {
                let (station, offset) = project(alignment, positions[k]);
                let line_angle_deg = if k > 0 && k + 1 < n {
                    let (ax, ay) = horizontal(positions[k - 1], positions[k]);
                    let (bx, by) = horizontal(positions[k], positions[k + 1]);
                    // Signed angle from back to ahead direction; right turn positive.
                    -(ax * by - ay * bx).atan2(ax * bx + ay * by).to_degrees()
                } else {
                    0.0
                };
                let ahead_span = if k + 1 < n {
                    let (dx, dy) = horizontal(positions[k], positions[k + 1]);
                    dx.hypot(dy)
                } else {
                    0.0
                };
                Station {
                    structure: self.structures[k].clone(),
                    station,
                    offset,
                    line_angle_deg,
                    ahead_span,
                }
            })
            .collect())
    }
}

/// A place on a route: its index, the distance along it, its unit
/// direction there.
type OnRoute = (usize, f64, [f64; 2]);

impl Line {
    /// Every alignment with the structures on it, in the line's order.
    pub fn alignment_views(&self) -> Vec<AlignmentView> {
        let near = self.near_alignments();
        self.alignments
            .iter()
            .enumerate()
            .map(|(k, a)| {
                let pts = positions(&a.points);
                let mut on: Vec<(f64, String)> = self
                    .structures
                    .iter()
                    .filter(|s| near.get(&s.id).is_some_and(|ks| ks.contains(&k)))
                    .map(|s| (project_or_point(&pts, s.position), s.id.clone()))
                    .collect();
                on.sort_by(|x, y| x.0.total_cmp(&y.0));
                AlignmentView {
                    id: a.id.clone(),
                    name: a.name.clone(),
                    routes: a.routes.clone(),
                    alignment: a.points.clone(),
                    structures: on.into_iter().map(|x| x.1).collect(),
                }
            })
            .collect()
    }

    /// The alignments each structure is on (indices in `alignments`): those
    /// with a point tied to it; else, for a structure giving its routes,
    /// the nearest alignment of each of them; else those it stands within
    /// [`NEAR_ALIGNMENT`] of.
    fn near_alignments(&self) -> std::collections::BTreeMap<String, Vec<usize>> {
        let mut out = std::collections::BTreeMap::new();
        let lines: Vec<Vec<Point>> = self
            .alignments
            .iter()
            .map(|a| positions(&a.points))
            .collect();
        for s in &self.structures {
            let mut ks: Vec<usize> = (0..self.alignments.len())
                .filter(|&k| {
                    self.alignments[k]
                        .points
                        .iter()
                        .any(|p| p.structure.as_deref() == Some(s.id.as_str()))
                })
                .collect();
            if !s.routes.is_empty() {
                for r in &s.routes {
                    let nearest = (0..self.alignments.len())
                        .filter(|&k| self.alignments[k].routes.contains(r))
                        .min_by(|&a, &b| {
                            distance_to(&lines[a], s.position)
                                .total_cmp(&distance_to(&lines[b], s.position))
                        });
                    if let Some(k) = nearest
                        && !ks.contains(&k)
                    {
                        ks.push(k);
                    }
                }
            } else {
                for (k, l) in lines.iter().enumerate() {
                    if !ks.contains(&k) && beside(l, s.position) <= NEAR_ALIGNMENT {
                        ks.push(k);
                    }
                }
            }
            ks.sort_unstable();
            if !ks.is_empty() {
                out.insert(s.id.clone(), ks);
            }
        }
        out
    }

    /// The routes of every structure: those it gives itself, else those of
    /// the alignments it is on (of the point it is tied to, when that point
    /// has routes of its own). A structure on no alignment has none.
    pub fn structure_routes(&self) -> std::collections::BTreeMap<String, Vec<String>> {
        let near = self.near_alignments();
        let mut out = std::collections::BTreeMap::new();
        for s in &self.structures {
            let mut routes: Vec<String> = if !s.routes.is_empty() {
                s.routes.clone()
            } else {
                near.get(&s.id)
                    .into_iter()
                    .flatten()
                    .flat_map(|&k| {
                        let a = &self.alignments[k];
                        a.points
                            .iter()
                            .find(|p| p.structure.as_deref() == Some(s.id.as_str()))
                            .and_then(|p| p.routes.clone())
                            .unwrap_or_else(|| a.routes.clone())
                    })
                    .collect()
            };
            routes.sort();
            routes.dedup();
            out.insert(s.id.clone(), routes);
        }
        out
    }

    /// The routes of structure `id` (see [`Line::structure_routes`]).
    pub fn routes_of(&self, id: &str) -> Vec<String> {
        self.structure_routes().remove(id).unwrap_or_default()
    }

    /// The routes of every section and jumper: those of their structures
    /// (a section from one route into another is on both).
    pub fn wire_routes(
        &self,
    ) -> (
        std::collections::BTreeMap<String, Vec<String>>,
        std::collections::BTreeMap<String, Vec<String>>,
    ) {
        let of = self.structure_routes();
        let union = |ids: &mut dyn Iterator<Item = &str>| {
            let mut r: Vec<String> = ids
                .flat_map(|id| of.get(id).cloned().unwrap_or_default())
                .collect();
            r.sort();
            r.dedup();
            r
        };
        let sections = self
            .sections
            .iter()
            .map(|s| {
                (
                    s.id.clone(),
                    union(&mut s.points.iter().map(|p| p.structure.as_str())),
                )
            })
            .collect();
        let jumpers = self
            .jumpers
            .iter()
            .map(|j| {
                (
                    j.id.clone(),
                    union(&mut [j.from.structure.as_str(), j.to.structure.as_str()].into_iter()),
                )
            })
            .collect();
        (sections, jumpers)
    }

    /// The alignments of route `id`, in the line's order.
    pub fn route_alignments(&self, id: &str) -> Vec<AlignmentView> {
        self.alignment_views()
            .into_iter()
            .filter(|a| a.routes.iter().any(|r| r == id))
            .collect()
    }

    /// Files of version 2 and before: their routes (each its own alignment,
    /// with its structures, or following them) and the line's own
    /// alignment become routes and alignments of the same ids. A route
    /// without points gets points tied to its structures; a structure not
    /// found on its routes from the alignments keeps them as its own
    /// [`crate::Structure::routes`]. Routes already of the current form are
    /// kept. A line of version 2 or before with no routes (`version` < 3)
    /// gets the route and alignment `main` through its own alignment, or
    /// all its structures; a line of the current version never does.
    pub fn upgrade_routes(&mut self) {
        let legacy = self.routes.iter().any(Route::is_legacy);
        let own = self.version < 3
            && self.routes.is_empty()
            && self.alignments.is_empty()
            && (!self.alignment.is_empty() || !self.structures.is_empty());
        if !legacy && !own {
            self.alignment.clear();
            return;
        }
        let old: Vec<(String, String, Vec<AlignmentPoint>, Vec<String>)> = if legacy {
            let (old, keep): (Vec<Route>, Vec<Route>) = std::mem::take(&mut self.routes)
                .into_iter()
                .partition(Route::is_legacy);
            self.routes = keep;
            old.into_iter()
                .map(|r| {
                    let name = r.name.clone();
                    (
                        r.id,
                        name,
                        r.legacy_alignment,
                        r.legacy_structures.unwrap_or_default(),
                    )
                })
                .collect()
        } else {
            vec![(
                "main".to_owned(),
                self.name.clone(),
                std::mem::take(&mut self.alignment),
                self.structures.iter().map(|s| s.id.clone()).collect(),
            )]
        };
        self.alignment.clear();
        let mut wanted: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        for (id, name, points, structures) in old {
            let points = if points.len() >= 2 {
                points
            } else {
                structures
                    .iter()
                    .filter_map(|s| self.structure(s))
                    .map(|s| AlignmentPoint {
                        structure: Some(s.id.clone()),
                        ..AlignmentPoint::free(s.position)
                    })
                    .collect()
            };
            for s in &structures {
                wanted.entry(s.clone()).or_default().push(id.clone());
            }
            if !self.routes.iter().any(|r| r.id == id) {
                self.routes.push(Route::new(&id, &name));
            }
            if !points.is_empty() && !self.alignments.iter().any(|a| a.id == id) {
                self.alignments.push(Alignment {
                    id: id.clone(),
                    name,
                    description: String::new(),
                    routes: vec![id],
                    points,
                });
            }
        }
        // Each structure stays on the routes it was listed on.
        let found = self.structure_routes();
        for s in &mut self.structures {
            let Some(mut want) = wanted.remove(&s.id) else {
                continue;
            };
            want.sort();
            want.dedup();
            if found.get(&s.id) != Some(&want) {
                s.routes = want;
            }
        }
    }

    /// The line's direction near `at` (plan): the unit direction, ahead
    /// along its route, of the nearest segment of its routes' centrelines;
    /// `None` for a line without two distinct points. What the wind's
    /// side ("from the right of the line") is taken against.
    pub fn direction_at(&self, at: [f64; 2]) -> Option<[f64; 2]> {
        self.nearest_on_routes(at).map(|n| n.2)
    }

    /// Where `at` (plan) is along the line: the route nearest to it (its
    /// index in [`Line::alignment_views`]) and the distance along that
    /// route's centreline (from its start) of the point nearest to it.
    pub fn place_along(&self, at: [f64; 2]) -> Option<(usize, f64)> {
        self.nearest_on_routes(at).map(|n| (n.0, n.1))
    }

    /// The nearest segment of the routes' centrelines: route index,
    /// distance along it, unit direction.
    fn nearest_on_routes(&self, at: [f64; 2]) -> Option<OnRoute> {
        let mut best: Option<(f64, OnRoute)> = None;
        for (k, r) in self.alignment_views().iter().enumerate() {
            let mut run = 0.0;
            for w in r.centreline(self).windows(2) {
                let (dx, dy) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
                let len2 = dx * dx + dy * dy;
                if len2 <= 0.0 {
                    continue;
                }
                let len = len2.sqrt();
                let t = (((at[0] - w[0][0]) * dx + (at[1] - w[0][1]) * dy) / len2).clamp(0.0, 1.0);
                let dist = (at[0] - w[0][0] - t * dx).hypot(at[1] - w[0][1] - t * dy);
                if best.is_none_or(|b| dist < b.0) {
                    best = Some((dist, (k, run + t * len, [dx / len, dy / len])));
                }
                run += len;
            }
        }
        best.map(|b| b.1)
    }

    /// Alignment points tied to each structure of `view`, under it.
    pub fn alignment_under_structures(&self, view: &AlignmentView) -> Vec<AlignmentPoint> {
        view.structures
            .iter()
            .filter_map(|id| self.structure(id))
            .map(|s| AlignmentPoint {
                structure: Some(s.id.clone()),
                ..AlignmentPoint::free(s.position)
            })
            .collect()
    }
}

/// The station of `p` along `line`, or 0 for an alignment of one point.
fn project_or_point(line: &[Point], p: Point) -> f64 {
    if line.len() < 2 {
        0.0
    } else {
        project(line, p).0
    }
}

/// Plan distance from `p` to the nearest point of `line` (one point:
/// to it).
fn distance_to(line: &[Point], p: Point) -> f64 {
    if let [a] = line {
        return (a[0] - p[0]).hypot(a[1] - p[1]);
    }
    line.windows(2)
        .map(|w| {
            let (dx, dy) = horizontal(w[0], w[1]);
            let len2 = dx * dx + dy * dy;
            let t = if len2 > 0.0 {
                (((p[0] - w[0][0]) * dx + (p[1] - w[0][1]) * dy) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (w[0][0] + t * dx - p[0]).hypot(w[0][1] + t * dy - p[1])
        })
        .fold(f64::INFINITY, f64::min)
}

/// Plan distance from `p` to `line` when `p` lies beside it (its
/// projection falls on it, not beyond its ends); infinite otherwise.
fn beside(line: &[Point], p: Point) -> f64 {
    match line {
        [] => f64::INFINITY,
        [a] => (a[0] - p[0]).hypot(a[1] - p[1]),
        _ => {
            let (st, off) = project(line, p);
            let back = point_at(line, st, off);
            if (back[0] - p[0]).hypot(back[1] - p[1]) > 1e-6 {
                // Beyond an end: the end itself counts, as a junction.
                let ends = [line[0], line[line.len() - 1]];
                ends.iter()
                    .map(|e| (e[0] - p[0]).hypot(e[1] - p[1]))
                    .fold(f64::INFINITY, f64::min)
            } else {
                off.abs()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_at_inverts_project() {
        let poly = [[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 50.0, 0.0]];
        for (station, offset) in [(30.0, 5.0), (30.0, -4.0), (120.0, 3.0), (150.0, -2.0)] {
            let q = point_at(&poly, station, offset);
            let (s, o) = project(&poly, [q[0], q[1], 0.0]);
            assert!(
                (s - station).abs() < 1e-9 && (o - offset).abs() < 1e-9,
                "{station} {offset}: {s} {o}"
            );
        }
        // Right of the direction is positive: heading east, right is south.
        assert_eq!(point_at(&poly, 10.0, 2.0), [10.0, -2.0]);
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} vs {b}");
    }

    #[test]
    fn stationing_offsets_and_angles() {
        let line = crate::tests::example();
        let routes = line.alignment_views();
        let main = routes.iter().find(|r| r.id == "main").unwrap();
        let st = main.stations(&line).unwrap();
        close(st[0].station, 0.0);
        close(st[1].station, 310.0);
        // T3 is the 10° angle point (turning left: y grows after T3). The
        // example's coordinates are rounded to 1 mm: angle within 0.001°.
        assert!(
            (st[2].line_angle_deg + 10.0).abs() < 1e-3,
            "{}",
            st[2].line_angle_deg
        );
        close(st[0].line_angle_deg, 0.0);
        for s in &st {
            assert!(s.offset.abs() < 1e-3, "{}", s.offset); // mm-rounded example
        }
    }

    #[test]
    fn y_pylon_belongs_to_both_routes() {
        let line = crate::tests::example();
        let mut routes = line.routes_of("T3");
        routes.sort();
        assert_eq!(routes, ["branch", "main"]);
        let views = line.alignment_views();
        let branch = views.iter().find(|r| r.id == "branch").unwrap();
        let st = branch.stations(&line).unwrap();
        // The branch starts at the Y pylon.
        assert_eq!(st[0].structure, "T3");
        close(st[0].station, 0.0);
    }

    #[test]
    fn offset_sign_is_right_positive() {
        // Alignment along +x; a point at y = -5 is to the right.
        let (station, offset) = project(&[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0]], [40.0, -5.0, 0.0]);
        close(station, 40.0);
        close(offset, 5.0);
    }

    #[test]
    fn a_line_of_version_2_becomes_routes_and_alignments() {
        // Its own alignment, no routes: route and alignment "main" through
        // every structure (tied under them when it had no points).
        let mut line = crate::tests::example();
        let ids: Vec<String> = line.structures.iter().map(|s| s.id.clone()).collect();
        line.routes.clear();
        line.alignments.clear();
        // A line of the current version is left as it is…
        line.upgrade_routes();
        assert!(line.routes.is_empty() && line.alignments.is_empty());
        // …one of version 2 gets "main".
        line.version = 2;
        line.upgrade_routes();
        assert_eq!(line.routes.len(), 1);
        assert_eq!(line.alignments[0].id, "main");
        assert_eq!(line.alignment_views()[0].structures, ids);
        assert!(line.structures.iter().all(|s| s.routes.is_empty()));
        line.validate().unwrap();
    }

    #[test]
    fn structures_go_on_the_routes_of_their_alignments() {
        let mut line = crate::tests::example();
        // A structure 4 m beside main's alignment is on it; 6 m away it is
        // on no route, until it gives one.
        let main = line
            .alignments
            .iter()
            .find(|a| a.id == "main")
            .unwrap()
            .clone();
        let [a, b] = [main.points[0].position, main.points[1].position];
        let n = (b[0] - a[0]).hypot(b[1] - a[1]);
        let side = [-(b[1] - a[1]) / n, (b[0] - a[0]) / n];
        let at = |d: f64| {
            [
                0.5 * (a[0] + b[0]) + side[0] * d,
                0.5 * (a[1] + b[1]) + side[1] * d,
                a[2],
            ]
        };
        let mut s = line.structures[0].clone();
        s.id = "X".into();
        s.position = at(4.0);
        s.routes.clear();
        line.structures.push(s);
        assert_eq!(line.routes_of("X"), ["main"]);
        line.structures.last_mut().unwrap().position = at(6.0);
        assert!(line.routes_of("X").is_empty());
        line.structures.last_mut().unwrap().routes = vec!["branch".into()];
        assert_eq!(line.routes_of("X"), ["branch"]);
        assert!(
            line.alignment_views()
                .iter()
                .any(|v| v.id == "branch" && v.structures.iter().any(|s| s == "X"))
        );
        // Sections follow their structures: one from main into the branch
        // is on both.
        let (sections, _) = line.wire_routes();
        assert!(sections.values().any(|r| r == &["branch", "main"]));
    }

    #[test]
    fn a_point_of_its_own_routes_puts_its_structure_there() {
        let mut line = crate::tests::example();
        line.routes.push(Route::new("other", "Other"));
        // The branch (version 2: it followed its structures) has points
        // tied to them.
        let branch = line
            .alignments
            .iter_mut()
            .find(|a| a.id == "branch")
            .unwrap();
        let p = branch.points.last_mut().unwrap();
        let id = p.structure.clone().unwrap();
        assert_eq!(line.routes_of(&id), ["branch"]);
        let branch = line
            .alignments
            .iter_mut()
            .find(|a| a.id == "branch")
            .unwrap();
        branch.points.last_mut().unwrap().routes = Some(vec!["branch".into(), "other".into()]);
        line.validate().unwrap();
        assert_eq!(line.routes_of(&id), ["branch", "other"]);
    }

    #[test]
    fn an_alignment_of_one_point_is_kept() {
        let mut line = crate::tests::example();
        line.alignments.push(Alignment {
            id: "alone".into(),
            name: String::new(),
            description: String::new(),
            routes: vec!["main".into()],
            points: vec![AlignmentPoint::free([0.0, 0.0, 0.0])],
        });
        line.validate().unwrap();
        let v = line.alignment_views();
        let alone = v.iter().find(|a| a.id == "alone").unwrap();
        assert_eq!(alone.centreline(&line).len(), 1);
        line.alignments.last_mut().unwrap().points.clear();
        assert!(line.validate().is_err());
    }
}
