// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A gusset plate's own axes (2026-11-04: "the user should be able to
//! access the gusset plate details if needed to correct some of the joint
//! in case the automatic sizing fails"): its corners as u, v in its plane,
//! edited by hand and written back as the 3D `outline` in the same plane.
//!
//! - **n**: the plate's normal (its outline's, Newell's sum), its largest
//!   component made positive so that it stays the same however the
//!   corners are edited;
//! - **u**: along the joint's leg the plate lies on (its direction laid
//!   into the plane), else along the first member bolted onto it, else
//!   along the outline's first side;
//! - **v**: n × u;
//! - **origin**: the joint's work point laid square onto the plate's
//!   plane.

use crate::{JointDetail, PlateDetail};
use ohl_core::geometry::cross;

/// A plate's axes in its plane, structure's axes, m.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlateFrame {
    /// The work point laid onto the plate's plane.
    pub origin: [f64; 3],
    /// Along the leg (or the first member) in the plane, unit.
    pub u: [f64; 3],
    /// n × u, unit.
    pub v: [f64; 3],
    /// The plate's normal, unit.
    pub n: [f64; 3],
    /// How far the work point lies from the plate's plane along n, m
    /// (positive: on n's side).
    pub work_point: f64,
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

/// The normal of a polygon (Newell's sum), unit, its largest component
/// positive; none for fewer than three points or no area.
pub fn plane_normal(outline: &[[f64; 3]]) -> Option<[f64; 3]> {
    if outline.len() < 3 {
        return None;
    }
    let mut n = [0.0; 3];
    for (k, p) in outline.iter().enumerate() {
        let q = outline[(k + 1) % outline.len()];
        n[0] += (p[1] - q[1]) * (p[2] + q[2]);
        n[1] += (p[2] - q[2]) * (p[0] + q[0]);
        n[2] += (p[0] - q[0]) * (p[1] + q[1]);
    }
    let l = norm(n);
    if l < 1e-12 {
        return None;
    }
    let mut n = scale(n, 1.0 / l);
    let big = (0..3)
        .max_by(|a, b| n[*a].abs().total_cmp(&n[*b].abs()))
        .unwrap_or(2);
    if n[big] < 0.0 {
        n = scale(n, -1.0);
    }
    Some(n)
}

impl PlateFrame {
    /// The axes of a plate lying on `outline` (3 points or more), u along
    /// the first of `along` that does not lie (nearly) square to the plane,
    /// else along the outline's first side; the origin `work_point` laid
    /// onto the plane.
    pub fn new(outline: &[[f64; 3]], along: &[[f64; 3]], work_point: [f64; 3]) -> Option<Self> {
        let n = plane_normal(outline)?;
        let c = outline.iter().fold([0.0; 3], |s, p| add(s, *p));
        let c = scale(c, 1.0 / outline.len() as f64);
        let laid = |d: [f64; 3]| {
            let d = sub(d, scale(n, dot(d, n)));
            let l = norm(d);
            (l > 1e-9).then(|| scale(d, 1.0 / l))
        };
        let first_side = outline
            .iter()
            .zip(outline.iter().cycle().skip(1))
            .map(|(a, b)| sub(*b, *a))
            .find(|d| norm(*d) > 1e-9)?;
        // A direction square to the plane (within ~6°) gives no u.
        let u = along
            .iter()
            .filter(|d| norm(**d) > 1e-12)
            .filter(|d| dot(**d, n).abs() / norm(**d) < 0.995)
            .find_map(|d| laid(*d))
            .or_else(|| laid(first_side))?;
        let v = cross(n, u);
        let h = dot(sub(work_point, c), n);
        Some(Self {
            origin: sub(work_point, scale(n, h)),
            u,
            v,
            n,
            work_point: h,
        })
    }

    /// The axes of plate `p` at joint `j`: u along the leg it lies on,
    /// else its first member bolted onto it (the members' directions from
    /// the joint).
    pub fn of(j: &JointDetail, p: &PlateDetail) -> Option<Self> {
        let dir = |m: &str| {
            j.arrivals
                .iter()
                .find(|a| a.member == m)
                .map(|a| a.direction)
        };
        let along: Vec<[f64; 3]> = p
            .leg
            .iter()
            .chain(p.members.iter())
            .filter_map(|m| dir(m))
            .collect();
        Self::new(&p.outline, &along, j.position)
    }

    /// A point (structure's axes, m) as u, v in the plate's plane, m (its
    /// distance from the plane dropped).
    pub fn to_uv(&self, p: [f64; 3]) -> [f64; 2] {
        let d = sub(p, self.origin);
        [dot(d, self.u), dot(d, self.v)]
    }

    /// u, v in the plate's plane (m) as a point, structure's axes, m.
    pub fn to_3d(&self, uv: [f64; 2]) -> [f64; 3] {
        add(self.origin, add(scale(self.u, uv[0]), scale(self.v, uv[1])))
    }

    /// An outline as u, v.
    pub fn corners(&self, outline: &[[f64; 3]]) -> Vec<[f64; 2]> {
        outline.iter().map(|p| self.to_uv(*p)).collect()
    }

    /// Corners u, v as an outline in the plate's plane.
    pub fn outline(&self, corners: &[[f64; 2]]) -> Vec<[f64; 3]> {
        corners.iter().map(|c| self.to_3d(*c)).collect()
    }

    /// Where a line through `at` along `dir` meets the plate's plane (a
    /// point seen in a view looking along `dir`); none when it runs
    /// nearly in the plane (the plate seen edge-on).
    pub fn meet(&self, at: [f64; 3], dir: [f64; 3]) -> Option<[f64; 3]> {
        let k = dot(dir, self.n);
        if k.abs() < 0.2 * norm(dir) {
            return None;
        }
        let t = dot(sub(self.origin, at), self.n) / k;
        Some(add(at, scale(dir, t)))
    }
}

/// An outline moved `by` metres along `n` (the plate's plane shifted
/// square to itself: e.g. onto the other side of a flange).
pub fn offset_outline(outline: &[[f64; 3]], n: [f64; 3], by: f64) -> Vec<[f64; 3]> {
    outline.iter().map(|p| add(*p, scale(n, by))).collect()
}

/// Corner `after` and the next one (cyclic) with a new corner midway
/// between them inserted after `after` (none: after the last one).
pub fn insert_corner(corners: &[[f64; 2]], after: Option<usize>) -> Vec<[f64; 2]> {
    let mut out = corners.to_vec();
    if corners.is_empty() {
        return out;
    }
    let k = after
        .filter(|k| *k < corners.len())
        .unwrap_or(corners.len() - 1);
    let (a, b) = (corners[k], corners[(k + 1) % corners.len()]);
    out.insert(k + 1, [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        norm(sub(a, b)) < 1e-9
    }

    /// A sloped plate (a battered face) and a leg along it.
    fn sloped() -> (Vec<[f64; 3]>, [f64; 3]) {
        let leg = [0.1, 0.0, 1.0];
        let across = [0.0, 1.0, 0.05];
        let o = [2.0, 1.0, 10.0];
        let at = |a: f64, b: f64| add(o, add(scale(leg, a), scale(across, b)));
        (
            vec![at(-0.2, 0.0), at(0.2, 0.0), at(0.25, 0.3), at(-0.1, 0.35)],
            leg,
        )
    }

    #[test]
    fn corners_go_round_and_back() {
        let (outline, leg) = sloped();
        let wp = [2.05, 0.98, 10.02];
        let f = PlateFrame::new(&outline, &[leg], wp).unwrap();
        // Unit axes, square to each other.
        for a in [f.u, f.v, f.n] {
            assert!((norm(a) - 1.0).abs() < 1e-12);
        }
        assert!(dot(f.u, f.v).abs() < 1e-12 && dot(f.u, f.n).abs() < 1e-12);
        // u along the leg.
        assert!(dot(f.u, leg) / norm(leg) > 0.999);
        // The round trip.
        let back = f.outline(&f.corners(&outline));
        for (a, b) in outline.iter().zip(&back) {
            assert!(close(*a, *b), "{a:?} {b:?}");
        }
        // The origin: the work point laid onto the plane.
        assert!(dot(sub(f.origin, outline[0]), f.n).abs() < 1e-12);
        assert!(close(add(f.origin, scale(f.n, f.work_point)), wp));
    }

    #[test]
    fn an_edited_corner_stays_in_the_plane() {
        let (outline, leg) = sloped();
        let f = PlateFrame::new(&outline, &[leg], [2.0, 1.0, 10.0]).unwrap();
        let mut c = f.corners(&outline);
        c[2] = [0.4, 0.5];
        let c = insert_corner(&c, Some(3));
        assert_eq!(c.len(), 5);
        let moved = f.outline(&c);
        for p in &moved {
            assert!(dot(sub(*p, outline[0]), f.n).abs() < 1e-12);
        }
        // The same axes from the edited outline (the normal's sign kept).
        let g = PlateFrame::new(&moved, &[leg], [2.0, 1.0, 10.0]).unwrap();
        assert!(close(g.n, f.n) && close(g.u, f.u) && close(g.origin, f.origin));
        // Reversed corners: the same normal still.
        let mut rev = moved.clone();
        rev.reverse();
        assert!(close(plane_normal(&rev).unwrap(), f.n));
    }

    #[test]
    fn the_offset_moves_the_plane_only() {
        let (outline, leg) = sloped();
        let f = PlateFrame::new(&outline, &[leg], [2.0, 1.0, 10.0]).unwrap();
        let moved = offset_outline(&outline, f.n, 0.012);
        let g = PlateFrame::new(&moved, &[leg], [2.0, 1.0, 10.0]).unwrap();
        assert!(close(g.n, f.n));
        assert!((f.work_point - g.work_point - 0.012).abs() < 1e-12);
        // The corners in the plane unchanged.
        for (a, b) in f.corners(&outline).iter().zip(g.corners(&moved)) {
            assert!((a[0] - b[0]).abs() < 1e-12 && (a[1] - b[1]).abs() < 1e-12);
        }
    }

    #[test]
    fn a_direction_square_to_the_plate_gives_way() {
        let outline = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
        // Along z (square to the plate): the next direction, then the
        // first side.
        let f = PlateFrame::new(&outline, &[[0.0, 0.0, 1.0], [0.0, 2.0, 0.0]], [0.0; 3]).unwrap();
        assert!(close(f.u, [0.0, 1.0, 0.0]));
        let g = PlateFrame::new(&outline, &[[0.0, 0.0, 1.0]], [0.0; 3]).unwrap();
        assert!(close(g.u, [1.0, 0.0, 0.0]));
        assert!(close(g.v, [0.0, 1.0, 0.0]));
        // A view along z meets it; one along x does not.
        assert!(close(
            g.meet([0.3, 0.4, 5.0], [0.0, 0.0, 1.0]).unwrap(),
            [0.3, 0.4, 0.0]
        ));
        assert!(g.meet([0.3, 0.4, 5.0], [1.0, 0.0, 0.0]).is_none());
    }

    #[test]
    fn a_corner_goes_in_midway() {
        let c = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]];
        assert_eq!(insert_corner(&c, Some(0))[1], [0.5, 0.0]);
        // At the end: midway back to the first.
        assert_eq!(insert_corner(&c, None)[3], [0.5, 0.5]);
    }
}
