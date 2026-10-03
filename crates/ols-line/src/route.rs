// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Routes: alignments, stationing, offsets and line angles.
//!
//! A [`Route`] is an alignment (PIs) and the ordered structures along it. A
//! structure can belong to several routes (Y pylons, parallel routes that
//! rejoin); each route gives it its own station. Stations and offsets are
//! horizontal: the structure position is projected onto the alignment
//! polyline (the closest segment); the offset is positive to the right
//! looking ahead. Without PIs, the alignment is the polyline through the
//! route's structures.
//!
//! The alignment is the base of the plan and profile drawings, kept apart
//! from the line design: its points ([`AlignmentPoint`]) are usually placed
//! under the structures and **tied** to them (they follow when a structure
//! moves), and each can be moved or detached on its own.

use serde::{Deserialize, Serialize};

use crate::{Line, LineError, Point};

/// A point of an alignment (PI).
///
/// In files a free point is `[x, y, z]`; a point tied to a structure is
/// `{"position": [x, y, z], "structure": "T12"}` and must lie under it (in
/// plan, within 1 mm).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "AlignmentRepr", into = "AlignmentRepr")]
pub struct AlignmentPoint {
    /// Position.
    pub position: Point,
    /// Structure it is tied to (it follows the structure when it moves).
    pub structure: Option<String>,
}

impl AlignmentPoint {
    /// A free point.
    pub fn free(position: Point) -> Self {
        Self {
            position,
            structure: None,
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
    },
}

impl From<AlignmentRepr> for AlignmentPoint {
    fn from(r: AlignmentRepr) -> Self {
        match r {
            AlignmentRepr::Free(position) => Self::free(position),
            AlignmentRepr::Tied {
                position,
                structure,
            } => Self {
                position,
                structure,
            },
        }
    }
}

impl From<AlignmentPoint> for AlignmentRepr {
    fn from(a: AlignmentPoint) -> Self {
        match a.structure {
            None => Self::Free(a.position),
            Some(s) => Self::Tied {
                position: a.position,
                structure: Some(s),
            },
        }
    }
}

/// Positions of alignment points.
pub fn positions(alignment: &[AlignmentPoint]) -> Vec<Point> {
    alignment.iter().map(|a| a.position).collect()
}

/// A route through the line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    /// Unique id, e.g. `"main"`, `"branch-north"`.
    pub id: String,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// Alignment points (PIs) in order; empty = through the structures.
    #[serde(default)]
    pub alignment: Vec<AlignmentPoint>,
    /// Structure ids in order along the route.
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

impl Route {
    /// The route's centreline: its alignment, or its structures' positions
    /// when it has none.
    pub fn centreline(&self, line: &Line) -> Vec<Point> {
        if self.alignment.len() >= 2 {
            positions(&self.alignment)
        } else {
            self.structures
                .iter()
                .filter_map(|id| line.structure(id).map(|s| s.position))
                .collect()
        }
    }

    /// Stationing of every structure of the route.
    pub fn stations(&self, line: &Line) -> Result<Vec<Station>, LineError> {
        let positions: Vec<Point> = self
            .structures
            .iter()
            .map(|id| {
                line.structure(id).map(|s| s.position).ok_or_else(|| {
                    LineError::Invalid(format!("route {}: unknown structure {id}", self.id))
                })
            })
            .collect::<Result<_, _>>()?;
        let pis = self::positions(&self.alignment);
        let alignment = if self.alignment.len() >= 2 {
            &pis
        } else {
            &positions
        };
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

impl Line {
    /// The line's routes; without explicit routes, one implicit route
    /// `"main"` through all structures in file order, along `alignment`.
    pub fn effective_routes(&self) -> Vec<Route> {
        if !self.routes.is_empty() {
            return self.routes.clone();
        }
        vec![Route {
            id: "main".into(),
            name: self.name.clone(),
            alignment: self.alignment.clone(),
            structures: self.structures.iter().map(|s| s.id.clone()).collect(),
        }]
    }

    /// Alignment points tied to each structure, under it: one per
    /// structure of the route (the usual starting alignment).
    pub fn alignment_under_structures(&self, route: &Route) -> Vec<AlignmentPoint> {
        route
            .structures
            .iter()
            .filter_map(|id| self.structure(id))
            .map(|s| AlignmentPoint {
                position: s.position,
                structure: Some(s.id.clone()),
            })
            .collect()
    }

    /// Routes a structure belongs to.
    pub fn routes_of(&self, structure: &str) -> Vec<String> {
        self.effective_routes()
            .into_iter()
            .filter(|r| r.structures.iter().any(|s| s == structure))
            .map(|r| r.id)
            .collect()
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
        let routes = line.effective_routes();
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
        let branch = line.routes.iter().find(|r| r.id == "branch").unwrap();
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
    fn implicit_route_when_none_given() {
        let mut line = crate::tests::example();
        line.routes.clear();
        let routes = line.effective_routes();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].structures.len(), line.structures.len());
    }
}
