// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Routes, alignments, alignment points, stationing, offsets and line
//! angles.
//!
//! A line is laid out in three steps: a [`Route`] (a path of the line),
//! its [`Alignment`]s (each on exactly one route, in their order along
//! it), and the [`AlignmentPoint`]s (PIs) of each alignment, in order. A
//! point is an object of its own: it may be on no alignment yet (placed,
//! then picked one after another to make an alignment), on one, or on
//! several, with its own place in each; where alignments of different
//! routes meet they share the point, so the point (and a structure tied
//! to it) is on all their routes.
//!
//! Everything else is put on routes from the alignments
//! ([`Line::structure_routes`]): a structure tied to an alignment point,
//! or within [`NEAR_ALIGNMENT`] of an alignment, is on it (and on its
//! route); a structure away from every alignment gives its routes itself
//! ([`crate::Structure::routes`]). Sections, jumpers and what hangs on a
//! structure follow their structures.
//!
//! Stations and offsets are horizontal: a structure's position is
//! projected onto its alignment (the closest segment); the offset is
//! positive to the right looking ahead. [`Line::alignment_views`] gives
//! each alignment with its structures in station order.
//!
//! Older files are read into this form by [`Line::upgrade_routes`]:
//! version 2 and before held routes that were their own alignment
//! (listing their structures, or following them, without points) and the
//! line's own alignment; version 3 held alignments on one or more routes
//! with their points inside them.

use serde::{Deserialize, Serialize};

use crate::{Line, LineError, Point};

/// A structure within this plan distance of an alignment (beside it, not
/// beyond its ends) is on it, m.
pub const NEAR_ALIGNMENT: f64 = 5.0;

/// Points of older alignments this close in plan are one point, m.
const SAME_POINT: f64 = 1e-3;

/// An alignment point (PI): an object of its own, on any number of
/// alignments (each gives it its place in its own order).
///
/// A point tied to a structure must lie under it (in plan, within 1 mm)
/// and follows it when it moves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlignmentPoint {
    /// Unique id, e.g. `"P12"`.
    pub id: String,
    /// Position.
    pub position: Point,
    /// Structure it is tied to (it follows the structure when it moves).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structure: Option<String>,
    /// What it is, in words (free text).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

impl AlignmentPoint {
    /// A free point `id` at `position`.
    pub fn free(id: &str, position: Point) -> Self {
        Self {
            id: id.to_owned(),
            position,
            structure: None,
            description: String::new(),
        }
    }
}

/// A point of an older file (version 3 and before), inside its alignment
/// or route, without an id: `[x, y, z]`, or `{"position": [x, y, z],
/// "structure": "T12", "routes": [...], "description": ...}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "OldPointRepr", into = "OldPointRepr")]
pub struct OldPoint {
    /// Position.
    pub position: Point,
    /// Structure it was tied to.
    pub structure: Option<String>,
    /// What it is, in words.
    pub description: String,
    /// Its own routes (version 3; given to a structure tied to it).
    pub routes: Option<Vec<String>>,
}

impl OldPoint {
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
enum OldPointRepr {
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

impl From<OldPointRepr> for OldPoint {
    fn from(r: OldPointRepr) -> Self {
        match r {
            OldPointRepr::Free(position) => Self::free(position),
            OldPointRepr::Tied {
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

impl From<OldPoint> for OldPointRepr {
    fn from(a: OldPoint) -> Self {
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
    pub legacy_alignment: Vec<OldPoint>,
    /// …with its structures in order ([`Line::upgrade_routes`]).
    #[serde(default, rename = "structures", skip_serializing)]
    pub legacy_structures: Option<Vec<String>>,
    /// Values of columns added by plugins, by `plugin.column`
    /// ([`crate::Fields`]).
    #[serde(default, skip_serializing_if = "crate::Fields::is_empty")]
    pub fields: crate::Fields,
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
            fields: crate::Fields::new(),
        }
    }

    /// Read from a file of version 2 (not upgraded yet).
    pub fn is_legacy(&self) -> bool {
        self.legacy_structures.is_some() || !self.legacy_alignment.is_empty()
    }
}

/// An alignment: a polyline through alignment points, on one route.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "AlignmentRepr")]
pub struct Alignment {
    /// Unique id, e.g. `"alignment 2"`.
    pub id: String,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// What it is, in words (free text).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The route it belongs to.
    pub route: String,
    /// The ids of its points, in order (a profile needs two; none while
    /// it is being laid out).
    pub points: Vec<String>,
    /// Read from a file of version 3 (not upgraded yet): its routes and
    /// its own points ([`Line::upgrade_routes`]).
    #[serde(skip)]
    pub legacy: Option<(Vec<String>, Vec<OldPoint>)>,
    /// Values of columns added by plugins, by `plugin.column`
    /// ([`crate::Fields`]).
    #[serde(default, skip_serializing_if = "crate::Fields::is_empty")]
    pub fields: crate::Fields,
}

impl Alignment {
    /// Alignment `id` on `route`, through the points `points`.
    pub fn new(id: &str, route: &str, points: Vec<String>) -> Self {
        Self {
            id: id.to_owned(),
            name: String::new(),
            description: String::new(),
            route: route.to_owned(),
            points,
            legacy: None,
            fields: Default::default(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AlignmentRepr {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    route: Option<String>,
    /// Version 3.
    #[serde(default)]
    routes: Option<Vec<String>>,
    points: Vec<PointRef>,
    #[serde(default)]
    fields: crate::Fields,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PointRef {
    Id(String),
    Old(OldPoint),
}

impl From<AlignmentRepr> for Alignment {
    fn from(r: AlignmentRepr) -> Self {
        let (mut ids, mut old) = (Vec::new(), Vec::new());
        for p in r.points {
            match p {
                PointRef::Id(id) => ids.push(id),
                PointRef::Old(p) => old.push(p),
            }
        }
        let legacy =
            (r.routes.is_some() || !old.is_empty()).then(|| (r.routes.unwrap_or_default(), old));
        Self {
            id: r.id,
            name: r.name,
            description: r.description,
            route: r.route.unwrap_or_default(),
            points: ids,
            legacy,
            fields: r.fields,
        }
    }
}

/// An alignment with the structures on it, in station order: what the
/// profile, the drawings and the stationing work on.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignmentView {
    /// The alignment's id.
    pub id: String,
    /// Its name.
    pub name: String,
    /// The route it belongs to.
    pub route: String,
    /// Its points, in order.
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
    /// Alignment point `id`.
    pub fn alignment_point(&self, id: &str) -> Option<&AlignmentPoint> {
        self.alignment_points.iter().find(|p| p.id == id)
    }

    /// Alignment `id`.
    pub fn alignment_by_id(&self, id: &str) -> Option<&Alignment> {
        self.alignments.iter().find(|a| a.id == id)
    }

    /// The points of `alignment`, in order (unknown ids left out).
    pub fn points_of(&self, alignment: &Alignment) -> Vec<AlignmentPoint> {
        alignment
            .points
            .iter()
            .filter_map(|id| self.alignment_point(id).cloned())
            .collect()
    }

    /// The alignments point `id` is on, in the line's order.
    pub fn point_alignments(&self, id: &str) -> Vec<String> {
        self.alignments
            .iter()
            .filter(|a| a.points.iter().any(|p| p == id))
            .map(|a| a.id.clone())
            .collect()
    }

    /// The routes of point `id`: those of its alignments.
    pub fn point_routes(&self, id: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .alignments
            .iter()
            .filter(|a| a.points.iter().any(|p| p == id))
            .map(|a| a.route.clone())
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// A free id for a new alignment point: `P1`, `P2`… past the highest.
    pub fn new_point_id(&self) -> String {
        let n = self
            .alignment_points
            .iter()
            .filter_map(|p| p.id.strip_prefix('P')?.parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        format!("P{}", n + 1)
    }

    /// New alignment points at `points` (position, structure tied to),
    /// with fresh ids: their ids, in order.
    pub fn add_points(
        &mut self,
        points: impl IntoIterator<Item = (Point, Option<String>)>,
    ) -> Vec<String> {
        let first = self
            .new_point_id()
            .trim_start_matches('P')
            .parse::<u64>()
            .unwrap_or(1);
        let mut out = Vec::new();
        for (n, (position, structure)) in (first..).zip(points) {
            let id = format!("P{n}");
            self.alignment_points.push(AlignmentPoint {
                structure,
                ..AlignmentPoint::free(&id, position)
            });
            out.push(id);
        }
        out
    }

    /// Alignment `id` made to go through new points at `points` (position,
    /// structure tied to); its old points on no other alignment deleted
    /// (replaced). Whether there is such an alignment.
    pub fn set_alignment_points(
        &mut self,
        id: &str,
        points: impl IntoIterator<Item = (Point, Option<String>)>,
    ) -> bool {
        if self.alignment_by_id(id).is_none() {
            return false;
        }
        let ids = self.add_points(points);
        let old = match self.alignments.iter_mut().find(|a| a.id == id) {
            Some(a) => std::mem::replace(&mut a.points, ids),
            None => Vec::new(),
        };
        for p in old {
            if self.point_alignments(&p).is_empty() {
                self.alignment_points.retain(|q| q.id != p);
            }
        }
        true
    }

    /// Deletes alignment point `id`: off every alignment it is on (the
    /// others keep their order) and out of the line. Whether it was there.
    pub fn delete_alignment_point(&mut self, id: &str) -> bool {
        let before = self.alignment_points.len();
        self.alignment_points.retain(|p| p.id != id);
        for a in &mut self.alignments {
            a.points.retain(|p| p != id);
        }
        self.alignment_points.len() != before
    }

    /// The stations of structures `a` and `b` (a span's ends), along the
    /// first alignment both stand on; none when no alignment has both.
    pub fn span_stations(&self, a: &str, b: &str) -> Option<(f64, f64)> {
        let (pa, pb) = (self.structure(a)?.position, self.structure(b)?.position);
        self.alignment_views()
            .into_iter()
            .find(|v| v.structures.iter().any(|s| s == a) && v.structures.iter().any(|s| s == b))
            .map(|v| {
                let pis = positions(&v.alignment);
                (project(&pis, pa).0, project(&pis, pb).0)
            })
    }

    /// Section `section` with its places given by station turned into
    /// fractions of their spans ([`Line::span_stations`]); unchanged when
    /// it has none. A station whose span has no stations stays as given
    /// (its fraction).
    pub fn section_with_fractions<'s>(
        &self,
        section: &'s crate::Section,
    ) -> std::borrow::Cow<'s, crate::Section> {
        let any = section.loads.iter().any(|l| l.station.is_some())
            || section.span_points.iter().any(|p| p.at.station.is_some())
            || section
                .spread_loads
                .iter()
                .any(|l| l.start.station.is_some() || l.end.station.is_some());
        if !any {
            return std::borrow::Cow::Borrowed(section);
        }
        let mut s = section.clone();
        let stations = |span: usize| {
            let w = s.points.get(span..=span + 1)?;
            self.span_stations(&w[0].structure, &w[1].structure)
        };
        let resolve = |span: usize, p: &mut crate::SpanPlace| {
            if let (Some(st), Some((a, b))) = (p.station, stations(span))
                && (b - a).abs() > 1e-9
            {
                *p = crate::SpanPlace::at_fraction(((st - a) / (b - a)).clamp(0.0, 1.0));
            }
        };
        let mut loads = s.loads.clone();
        for l in &mut loads {
            let mut p = l.place();
            resolve(l.span, &mut p);
            (l.fraction, l.distance, l.station) = (p.fraction, p.distance, p.station);
        }
        let mut points = s.span_points.clone();
        for p in &mut points {
            resolve(p.span, &mut p.at);
        }
        let mut spread = s.spread_loads.clone();
        for l in &mut spread {
            resolve(l.span, &mut l.start);
            resolve(l.span, &mut l.end);
        }
        (s.loads, s.span_points, s.spread_loads) = (loads, points, spread);
        std::borrow::Cow::Owned(s)
    }

    /// Every alignment with the structures on it, in the line's order.
    pub fn alignment_views(&self) -> Vec<AlignmentView> {
        let near = self.near_alignments();
        self.alignments
            .iter()
            .enumerate()
            .map(|(k, a)| {
                let points = self.points_of(a);
                let pts = positions(&points);
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
                    route: a.route.clone(),
                    alignment: points,
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
        let tied: std::collections::BTreeMap<&str, &str> = self
            .alignment_points
            .iter()
            .filter_map(|p| Some((p.id.as_str(), p.structure.as_deref()?)))
            .collect();
        let lines: Vec<Vec<Point>> = self
            .alignments
            .iter()
            .map(|a| positions(&self.points_of(a)))
            .collect();
        for s in &self.structures {
            let mut ks: Vec<usize> = (0..self.alignments.len())
                .filter(|&k| {
                    self.alignments[k]
                        .points
                        .iter()
                        .any(|p| tied.get(p.as_str()) == Some(&s.id.as_str()))
                })
                .collect();
            if !s.routes.is_empty() {
                for r in &s.routes {
                    let nearest = (0..self.alignments.len())
                        .filter(|&k| self.alignments[k].route == *r)
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
    /// the alignments it is on. A structure on no alignment has none.
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
                    .map(|&k| self.alignments[k].route.clone())
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
            .filter(|a| a.route == id)
            .collect()
    }

    /// Reads older files into the current form.
    ///
    /// Version 2 and before: their routes (each its own alignment, with
    /// its structures, or following them) and the line's own alignment
    /// become routes and alignments of the same ids. A route without
    /// points gets points tied to its structures. A line of version 2 or
    /// before with no routes (`version` < 3) gets the route and alignment
    /// `main` through its own alignment, or all its structures; a line of
    /// a later version never does.
    ///
    /// Version 3 (alignments holding their points, on one or more
    /// routes): the points become alignment points of their own; points
    /// of different alignments at the same place (within 1 mm, tied to
    /// the same structure or one of them free) become one point, shared;
    /// an alignment on several routes becomes one alignment per route
    /// (`"a"`, `"a (route)"`…) through the same points. A point's own
    /// routes go to the structure tied to it.
    ///
    /// Every structure keeps the routes it was on: one no longer found on
    /// them from the alignments gets them as its own
    /// [`crate::Structure::routes`].
    pub fn upgrade_routes(&mut self) {
        let legacy = self.routes.iter().any(Route::is_legacy);
        let own = self.version < 3
            && self.routes.is_empty()
            && self.alignments.is_empty()
            && (!self.alignment.is_empty() || !self.structures.is_empty());
        let old_alignments = self.alignments.iter().any(|a| a.legacy.is_some());
        if !legacy && !own && !old_alignments {
            self.alignment.clear();
            return;
        }
        // Before: each structure's routes (version 3 meaning).
        let mut wanted: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        // Version 2 and before, into version-3 alignments.
        if legacy || own {
            let old: Vec<(String, String, Vec<OldPoint>, Vec<String>)> = if legacy {
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
            for (id, name, points, structures) in old {
                let points = if points.len() >= 2 {
                    points
                } else {
                    structures
                        .iter()
                        .filter_map(|s| self.structure(s))
                        .map(|s| OldPoint {
                            structure: Some(s.id.clone()),
                            ..OldPoint::free(s.position)
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
                    let mut a = Alignment::new(&id, "", Vec::new());
                    a.name = name;
                    a.legacy = Some((vec![id], points));
                    self.alignments.push(a);
                }
            }
        } else {
            // Version 3: the routes structures were on then.
            for (s, r) in self.structure_routes_v3() {
                wanted.insert(s, r);
            }
        }
        self.alignment.clear();
        // Version 3 alignments: points of their own, one alignment per route.
        let mut out: Vec<Alignment> = Vec::new();
        for a in std::mem::take(&mut self.alignments) {
            let Some((routes, points)) = a.legacy.clone() else {
                out.push(a);
                continue;
            };
            let mut ids: Vec<String> = Vec::new();
            for p in points {
                let id = self.old_point_id(&p, &ids);
                if let (Some(s), Some(own)) = (&p.structure, &p.routes) {
                    let w = wanted.entry(s.clone()).or_default();
                    w.extend(own.iter().cloned());
                }
                ids.push(id);
            }
            let routes = if routes.is_empty() {
                vec![a.route.clone()]
            } else {
                routes
            };
            for (k, r) in routes.iter().enumerate() {
                let mut b = a.clone();
                b.legacy = None;
                b.route = r.clone();
                b.points = ids.clone();
                if k > 0 {
                    b.id = format!("{} ({r})", a.id);
                    if !b.name.is_empty() {
                        b.name = format!("{} ({r})", a.name);
                    }
                }
                out.push(b);
            }
        }
        self.alignments = out;
        // Each structure stays on the routes it was on.
        let found = self.structure_routes();
        for s in &mut self.structures {
            let Some(mut want) = wanted.remove(&s.id) else {
                continue;
            };
            want.sort();
            want.dedup();
            if !s.routes.is_empty() {
                continue;
            }
            if found.get(&s.id) != Some(&want) {
                s.routes = want;
            }
        }
    }

    /// The id of the alignment point for old point `p`: a point already
    /// made at the same place (not one of `own`, the alignment's points so
    /// far) and compatible structure, its structure filled in; else a new
    /// point.
    fn old_point_id(&mut self, p: &OldPoint, own: &[String]) -> String {
        let same = self.alignment_points.iter_mut().find(|q| {
            !own.contains(&q.id)
                && (q.position[0] - p.position[0]).hypot(q.position[1] - p.position[1])
                    <= SAME_POINT
                && (q.structure.is_none() || p.structure.is_none() || q.structure == p.structure)
        });
        if let Some(q) = same {
            if q.structure.is_none() {
                q.structure = p.structure.clone();
            }
            if q.description.is_empty() {
                q.description = p.description.clone();
            }
            return q.id.clone();
        }
        let id = self.new_point_id();
        self.alignment_points.push(AlignmentPoint {
            id: id.clone(),
            position: p.position,
            structure: p.structure.clone(),
            description: p.description.clone(),
        });
        id
    }

    /// Version 3: the routes of each structure, from the alignments as
    /// read (several routes each, points with routes of their own).
    fn structure_routes_v3(&self) -> std::collections::BTreeMap<String, Vec<String>> {
        let mut out = std::collections::BTreeMap::new();
        let lines: Vec<(Vec<String>, Vec<OldPoint>)> = self
            .alignments
            .iter()
            .map(|a| {
                a.legacy
                    .clone()
                    .unwrap_or_else(|| (vec![a.route.clone()], Vec::new()))
            })
            .collect();
        for s in &self.structures {
            if !s.routes.is_empty() {
                continue;
            }
            let mut routes: Vec<String> = Vec::new();
            for (rs, pts) in &lines {
                let polyline: Vec<Point> = pts.iter().map(|p| p.position).collect();
                if let Some(p) = pts
                    .iter()
                    .find(|p| p.structure.as_deref() == Some(s.id.as_str()))
                {
                    routes.extend(p.routes.clone().unwrap_or_else(|| rs.clone()));
                } else if beside(&polyline, s.position) <= NEAR_ALIGNMENT {
                    routes.extend(rs.iter().cloned());
                }
            }
            routes.sort();
            routes.dedup();
            if !routes.is_empty() {
                out.insert(s.id.clone(), routes);
            }
        }
        out
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

    /// Points tied under each structure of `view`, in order: the point
    /// already tied to it when there is one, else a new point (a free id,
    /// not in the line yet).
    pub fn alignment_under_structures(&self, view: &AlignmentView) -> Vec<AlignmentPoint> {
        let mut out: Vec<AlignmentPoint> = Vec::new();
        let mut n = self
            .new_point_id()
            .trim_start_matches('P')
            .parse::<u64>()
            .unwrap_or(1);
        for s in view.structures.iter().filter_map(|id| self.structure(id)) {
            let p = match self
                .alignment_points
                .iter()
                .find(|p| p.structure.as_deref() == Some(s.id.as_str()))
            {
                Some(p) => AlignmentPoint {
                    position: s.position,
                    ..p.clone()
                },
                None => {
                    let id = format!("P{n}");
                    n += 1;
                    AlignmentPoint {
                        structure: Some(s.id.clone()),
                        ..AlignmentPoint::free(&id, s.position)
                    }
                }
            };
            out.push(p);
        }
        out
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
        let main = line.points_of(line.alignment_by_id("main").unwrap());
        let [a, b] = [main[0].position, main[1].position];
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
    fn a_line_of_version_3_gets_points_of_their_own() {
        // Two alignments meeting at T3 (each with its own copy of the
        // point), the second on two routes; a point with its own route.
        let mut line = crate::tests::example();
        let t3 = line.structure("T3").unwrap().position;
        let t7 = line.structure("T7").unwrap().position;
        let old = |p: Point, s: Option<&str>| OldPoint {
            structure: s.map(str::to_owned),
            ..OldPoint::free(p)
        };
        let main_pts = line.points_of(line.alignment_by_id("main").unwrap());
        line.routes.push(Route::new("other", "Other"));
        line.routes.push(Route::new("third", "Third"));
        line.alignment_points.clear();
        let mut main = Alignment::new("main", "", Vec::new());
        main.legacy = Some((
            vec!["main".into()],
            main_pts.iter().map(|p| old(p.position, None)).collect(),
        ));
        let mut branch = Alignment::new("branch", "", Vec::new());
        let mut end = old(t7, Some("T7"));
        end.routes = Some(vec!["branch".into(), "third".into()]);
        branch.legacy = Some((
            vec!["branch".into(), "other".into()],
            vec![old(t3, Some("T3")), end],
        ));
        line.alignments = vec![main, branch];
        line.version = 3;
        line.upgrade_routes();
        line.validate().unwrap();
        let ids: Vec<&str> = line.alignments.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["main", "branch", "branch (other)"]);
        assert_eq!(line.alignments[2].route, "other");
        // T3's point is shared: one point, on all three alignments.
        let p3 = line
            .alignment_points
            .iter()
            .find(|p| p.structure.as_deref() == Some("T3"))
            .unwrap();
        assert_eq!(line.point_alignments(&p3.id).len(), 3);
        assert_eq!(line.point_routes(&p3.id), ["branch", "main", "other"]);
        assert_eq!(line.routes_of("T3"), ["branch", "main", "other"]);
        // T7's own routes went to the structure.
        assert_eq!(line.routes_of("T7"), ["branch", "third"]);
    }

    #[test]
    fn deleting_a_point_takes_it_off_every_alignment() {
        let mut line = crate::tests::example();
        let shared = line
            .alignment_points
            .iter()
            .find(|p| line.point_alignments(&p.id).len() > 1)
            .unwrap()
            .id
            .clone();
        let before: Vec<Vec<String>> = line.alignments.iter().map(|a| a.points.clone()).collect();
        assert!(line.delete_alignment_point(&shared));
        for (a, b) in line.alignments.iter().zip(&before) {
            let kept: Vec<String> = b.iter().filter(|p| **p != shared).cloned().collect();
            assert_eq!(a.points, kept);
        }
        assert!(line.alignment_point(&shared).is_none());
        line.validate().unwrap();
    }

    #[test]
    fn an_alignment_needs_its_route_and_a_point_may_wait_alone() {
        let mut line = crate::tests::example();
        line.alignments
            .push(Alignment::new("alone", "main", Vec::new()));
        line.validate().unwrap();
        // A point may wait on no alignment.
        line.alignment_points
            .push(AlignmentPoint::free("Q", [0.0, 0.0, 0.0]));
        line.validate().unwrap();
        line.alignments.last_mut().unwrap().points.push("Q".into());
        line.validate().unwrap();
        let v = line.alignment_views();
        let alone = v.iter().find(|a| a.id == "alone").unwrap();
        assert_eq!(alone.centreline(&line).len(), 1);
        line.alignments.last_mut().unwrap().route = "nowhere".into();
        assert!(
            line.validate()
                .unwrap_err()
                .to_string()
                .contains("unknown route")
        );
    }
}
