// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The force each bolt of a group carries (stage 4.5 of the joint details,
//! 2026-11-19: the checks of the joints themselves; built, in active
//! development, not approved yet). No code's rule here — the mechanics the
//! joint checks share; the design codes' plugins check each bolt
//! (`joint-check-groups`), the structure application gathers the groups
//! and their forces (`ohl_structure_link::joint_groups`).
//!
//! **The elastic method** (Kulak, G. L., Fisher, J. W. and Struik, J. H. A.,
//! *Guide to Design Criteria for Bolted and Riveted Joints*, 2nd ed.,
//! Wiley, New York, 1987, its chapter on eccentrically loaded joints — the
//! guide ASCE 10-97's commentary cites as Kulak et al. 1988): the part the
//! bolts hold is rigid and every bolt equally stiff, so a force through the
//! group's centroid is shared equally and a moment about it in proportion
//! to each bolt's distance from the centroid, square to its radius:
//!
//! f_k = F / n + M_c (ẑ × r_k) / Σ_j |r_j|²,  M_c = M + (a − c) × F
//!
//! with c the centroid, r_k = p_k − c, a the point the force acts at. It is
//! the simpler and the more conservative of the guide's two analyses (the
//! ultimate-strength one lets ductile bolts share more): an assumption **on
//! the safe side** for bolts that bear. One bolt (or bolts all at the
//! centroid) cannot carry a moment: [`Elastic::shared`] false — the caller
//! says so (a pin), never a share.

/// A point or vector in a plane, m or N.
pub type V2 = [f64; 2];
/// A point or vector in the structure's axes.
pub type V3 = [f64; 3];

/// A plane: its origin, two axes in it and its normal (unit, right-handed:
/// u × v = n).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    /// The origin, m.
    pub origin: V3,
    /// First axis in the plane (unit).
    pub u: V3,
    /// Second axis in the plane (unit), n × u.
    pub v: V3,
    /// The normal (unit).
    pub n: V3,
}

impl Plane {
    /// The plane through `origin` square to `normal` (none: no normal);
    /// `u` along `towards` laid into the plane, else the axis of x, y, z
    /// least along the normal laid into it.
    pub fn new(origin: V3, normal: V3, towards: Option<V3>) -> Option<Plane> {
        let l = norm(normal);
        if l < 1e-12 {
            return None;
        }
        let n = scale(normal, 1.0 / l);
        let lay = |d: V3| {
            let x = sub(d, scale(n, dot(d, n)));
            let lx = norm(x);
            (lx > 1e-9).then(|| scale(x, 1.0 / lx))
        };
        let u = towards.and_then(lay).or_else(|| {
            let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
            let least = axes
                .iter()
                .copied()
                .min_by(|a, b| dot(*a, n).abs().total_cmp(&dot(*b, n).abs()))?;
            lay(least)
        })?;
        Some(Plane {
            origin,
            u,
            v: cross(n, u),
            n,
        })
    }

    /// Where point `p` is in the plane.
    pub fn point(&self, p: V3) -> V2 {
        let d = sub(p, self.origin);
        [dot(d, self.u), dot(d, self.v)]
    }

    /// The part of vector `f` in the plane.
    pub fn vector(&self, f: V3) -> V2 {
        [dot(f, self.u), dot(f, self.v)]
    }

    /// The part of vector `f` along the normal.
    pub fn normal_part(&self, f: V3) -> f64 {
        dot(f, self.n)
    }

    /// The plane's vector `f` in the structure's axes.
    pub fn to_3d(&self, f: V2) -> V3 {
        add(scale(self.u, f[0]), scale(self.v, f[1]))
    }
}

/// A force system in a plane: a force acting at a point and a moment about
/// the plane's normal.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PlaneLoad {
    /// The force, N.
    pub force: V2,
    /// Where it acts, m.
    pub at: V2,
    /// A moment about the normal, N·m (counter-clockwise positive).
    pub moment: f64,
}

/// What [`elastic`] gives.
#[derive(Debug, Clone, PartialEq)]
pub struct Elastic {
    /// The force on each bolt, N, in the order of the points.
    pub forces: Vec<V2>,
    /// The group's centroid, m.
    pub centroid: V2,
    /// The moment about the centroid, N·m.
    pub moment: f64,
    /// Σ |r|², m².
    pub polar: f64,
    /// The moment is shared out (false: one bolt, or every bolt at the
    /// centroid — the moment not carried, left out of `forces`).
    pub shared: bool,
}

impl Elastic {
    /// The most loaded bolt and its force's size (none: no bolt).
    pub fn largest(&self) -> Option<(usize, f64)> {
        self.forces
            .iter()
            .enumerate()
            .map(|(k, f)| (k, f[0].hypot(f[1])))
            .max_by(|a, b| a.1.total_cmp(&b.1))
    }
}

/// The centroid of `points` (the origin for none).
pub fn centroid(points: &[V2]) -> V2 {
    if points.is_empty() {
        return [0.0, 0.0];
    }
    let n = points.len() as f64;
    let s = points
        .iter()
        .fold([0.0, 0.0], |s, p| [s[0] + p[0], s[1] + p[1]]);
    [s[0] / n, s[1] / n]
}

/// The bolts at `points` (in the plane, m) sharing `load` by the elastic
/// method (module docs): the force equally, the moment about their
/// centroid by each bolt's distance from it.
pub fn elastic(points: &[V2], load: &PlaneLoad) -> Elastic {
    let c = centroid(points);
    let n = points.len();
    let lever = [load.at[0] - c[0], load.at[1] - c[1]];
    let moment = load.moment + cross2(lever, load.force);
    let polar: f64 = points
        .iter()
        .map(|p| (p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2))
        .sum();
    // Bolts within a tenth of a millimetre of their centroid: no lever.
    let shared = n > 1 && polar > 1e-8 * n as f64;
    let forces = points
        .iter()
        .map(|p| {
            let r = [p[0] - c[0], p[1] - c[1]];
            let direct = [load.force[0] / n as f64, load.force[1] / n as f64];
            if shared {
                [
                    direct[0] - moment * r[1] / polar,
                    direct[1] + moment * r[0] / polar,
                ]
            } else {
                direct
            }
        })
        .collect();
    Elastic {
        forces,
        centroid: c,
        moment,
        polar,
        shared,
    }
}

/// a × b of two plane vectors (the normal's component), N·m for a lever
/// and a force.
pub fn cross2(a: V2, b: V2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An eccentric group with a moment, worked by hand (invented data):
    /// four bolts on an 80 × 60 mm rectangle round their centroid, 100 kN
    /// down acting 150 mm beside it. M = 0.15 × −100 = −15 kN m, Σr² =
    /// 4 × (0.04² + 0.03²) = 0.01 m². Bolt (40, 30): F/n = (0, −25), the
    /// moment's share −15 / 0.01 × (−0.03, 0.04) = (45, −60) kN, together
    /// (45, −85), 96.18 kN — the most loaded, with (40, −30); those at x =
    /// −40 mm (±45, 35), 57.01 kN.
    #[test]
    fn an_eccentric_group_shares_its_moment() {
        let pts = [[0.04, 0.03], [0.04, -0.03], [-0.04, 0.03], [-0.04, -0.03]];
        let e = elastic(
            &pts,
            &PlaneLoad {
                force: [0.0, -100e3],
                at: [0.15, 0.0],
                moment: 0.0,
            },
        );
        assert!(e.shared);
        assert!((e.moment + 15e3).abs() < 1e-9, "{}", e.moment);
        assert!((e.polar - 0.01).abs() < 1e-15);
        let f0 = e.forces[0];
        assert!(
            (f0[0] - 45e3).abs() < 1e-6 && (f0[1] + 85e3).abs() < 1e-6,
            "{f0:?}"
        );
        let (k, f) = e.largest().unwrap();
        assert!(k < 2 && (f - 9250f64.sqrt() * 1e3).abs() < 1e-6, "{f}");
        let f2 = e.forces[2];
        assert!((f2[0].hypot(f2[1]) - 3250f64.sqrt() * 1e3).abs() < 1e-6);
        // The bolts' forces sum to the load and their moments to M.
        let s = e
            .forces
            .iter()
            .fold([0.0, 0.0], |s, f| [s[0] + f[0], s[1] + f[1]]);
        assert!(s[0].abs() < 1e-6 && (s[1] + 100e3).abs() < 1e-6);
        let m: f64 = pts.iter().zip(&e.forces).map(|(p, f)| cross2(*p, *f)).sum();
        assert!((m + 15e3).abs() < 1e-6);
    }

    /// A force through the centroid: equal shares (ASCE 10-97 4.1's
    /// assumption); one bolt cannot share a moment.
    #[test]
    fn equal_shares_and_a_pin() {
        let pts = [[0.0, 0.0], [0.05, 0.0], [0.1, 0.0]];
        let e = elastic(
            &pts,
            &PlaneLoad {
                force: [90e3, 0.0],
                at: [0.05, 0.0],
                moment: 0.0,
            },
        );
        for f in &e.forces {
            assert!((f[0] - 30e3).abs() < 1e-9 && f[1].abs() < 1e-9);
        }
        let one = elastic(
            &[[0.0, 0.0]],
            &PlaneLoad {
                force: [10e3, 0.0],
                at: [0.0, 0.0],
                moment: 500.0,
            },
        );
        assert!(!one.shared);
        assert!((one.forces[0][0] - 10e3).abs() < 1e-9);
    }

    /// A plane's axes: right-handed, u along the direction given.
    #[test]
    fn a_plane_and_its_axes() {
        let p = Plane::new([1.0, 2.0, 3.0], [0.0, 0.0, 2.0], Some([1.0, 0.0, 0.5])).unwrap();
        assert_eq!(p.u, [1.0, 0.0, 0.0]);
        assert_eq!(p.v, [0.0, 1.0, 0.0]);
        assert_eq!(p.point([2.0, 4.0, 9.0]), [1.0, 2.0]);
        assert_eq!(p.normal_part([0.0, 0.0, -3.0]), -3.0);
        assert!(Plane::new([0.0; 3], [0.0; 3], None).is_none());
        let q = Plane::new([0.0; 3], [1.0, 1.0, 0.0], None).unwrap();
        assert!(dot(q.u, q.n).abs() < 1e-12 && dot(q.v, q.n).abs() < 1e-12);
        assert!((dot(cross(q.u, q.v), q.n) - 1.0).abs() < 1e-12);
    }
}
