// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A tree falling about its root ball (theory: LiDAR analyses › Danger
//! trees › Fall-in).
//!
//! A vegetation point at height h above the ground is the top of a tree
//! standing straight below it, base B on the ground; grown by g its
//! height is h' = h + g. It falls towards a wire point q by turning about
//! a hinge H on the ground at the root-ball radius b_r = k·h' from the
//! trunk, on the side it falls to. In the vertical plane through B and q
//! (x from B towards q, z up from B): H = (b_r, 0), the top T = (0, h'),
//! the trunk's length from the hinge L = √(h'² + b_r²), its starting angle
//! θ₀ = atan2(h', −b_r). It sweeps the sector of radius L about H from θ₀
//! down to θ_min, where it rests on the ground ([`Tree::theta_min`]).

use ohl_terrain::Terrain;

use crate::geometry::P3;

/// A tree standing below a vegetation point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tree {
    /// Its base B, on the ground below the point.
    pub base: P3,
    /// Its height grown until the next cut, h' = h + g, m.
    pub height: f64,
    /// Root-ball radius b_r = k·h', m.
    pub root: f64,
    /// Trunk length from the hinge, L = √(h'² + b_r²), m.
    pub length: f64,
    /// Starting angle about the hinge, θ₀ = atan2(h', −b_r), rad.
    pub theta0: f64,
}

impl Tree {
    /// The tree of base `base`, grown height `height` (h + g) and
    /// root-ball fraction `k`.
    pub fn new(base: P3, height: f64, k: f64) -> Self {
        let root = k.max(0.0) * height;
        Self {
            base,
            height,
            root,
            length: if root == 0.0 {
                height
            } else {
                height.hypot(root)
            },
            theta0: height.atan2(-root),
        }
    }

    /// The plane coordinates of `q`: (ρ, ζ) — its horizontal distance from
    /// B and its height above B — and the unit plan direction from B to it
    /// (any direction when q is straight above B).
    pub fn plane(&self, q: P3) -> (f64, f64, [f64; 2]) {
        let (dx, dy) = (q[0] - self.base[0], q[1] - self.base[1]);
        let rho = dx.hypot(dy);
        let dir = if rho > 0.0 {
            [dx / rho, dy / rho]
        } else {
            [1.0, 0.0]
        };
        (rho, q[2] - self.base[2], dir)
    }

    /// The hinge on the side of plan direction `dir`.
    pub fn hinge(&self, dir: [f64; 2]) -> P3 {
        [
            self.base[0] + self.root * dir[0],
            self.base[1] + self.root * dir[1],
            self.base[2],
        ]
    }

    /// The angle about the hinge where the trunk falling along plan
    /// direction `dir` comes to rest on the ground: the largest angle,
    /// seen from H, of the ground along the direction within reach,
    /// \\( \max_{0 < u \le L} \operatorname{atan2}(z_g(b_r + u) - z_B, u) \\),
    /// at every edge crossing of the ground's TIN along the direction
    /// ([`Terrain::profile`]: between them the ground is linear and the
    /// angle seen from H changes monotonically, so the largest is at one
    /// of them).
    ///
    /// **Conservative:** where the ground is above the base's height (the
    /// tree falls uphill), its angle is seen from the base B instead of
    /// from the hinge (\\( u + b_r \\) for \\( u \\)): from H, at the
    /// hinge's own height, a rising ground close to it would stand at
    /// nearly 90° and stop the trunk at once; from B the ground's rise is
    /// its slope. A smaller resting angle is a larger sector. With
    /// \\( k = 0 \\) both are the same.
    ///
    /// Where the profile leaves the ground (a hole in the TIN), the parts
    /// missing do not stop the trunk; with no ground at all along the
    /// direction, −90°.
    pub fn theta_min(&self, ground: &Terrain, dir: [f64; 2]) -> f64 {
        let h = self.hinge(dir);
        let end = [h[0] + self.length * dir[0], h[1] + self.length * dir[1]];
        let mut best = -std::f64::consts::FRAC_PI_2;
        for run in ground.profile([h[0], h[1]], end) {
            for g in run {
                let u = g.t * self.length;
                if u <= 0.0 {
                    continue;
                }
                let dz = g.point[2] - self.base[2];
                let a = if dz > 0.0 {
                    dz.atan2(u + self.root)
                } else {
                    dz.atan2(u)
                };
                best = best.max(a);
            }
        }
        best
    }

    /// Whether wire point (ρ, ζ) lies within the sector's angles about the
    /// hinge, θ_min ≤ θ_q ≤ θ₀ (θ_q = atan2(ζ, ρ − b_r)).
    pub fn in_sector(&self, rho: f64, zeta: f64, theta_min: f64) -> bool {
        let t = zeta.atan2(rho - self.root);
        theta_min <= t && t <= self.theta0
    }

    /// The distance from wire point (ρ, ζ) of the plane to the trunk
    /// swept down to `theta_min`: within the sector's angles
    /// \\( |q - H| - L \\) (negative inside the sector: how far the
    /// trunk passes beyond the point), else the nearest of the trunk at
    /// rest \\( [H, H + L e^{i\theta_{\min}}] \\) and the trunk standing
    /// \\( [H, T] \\).
    pub fn fall_distance(&self, rho: f64, zeta: f64, theta_min: f64) -> f64 {
        let (x, z) = (rho - self.root, zeta);
        if self.in_sector(rho, zeta, theta_min) {
            return x.hypot(z) - self.length;
        }
        let seg = |ex: f64, ez: f64| {
            let len2 = ex * ex + ez * ez;
            let t = if len2 > 0.0 {
                ((x * ex + z * ez) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (x - t * ex).hypot(z - t * ez)
        };
        let rest = seg(self.length * theta_min.cos(), self.length * theta_min.sin());
        let standing = seg(-self.root, self.height);
        rest.min(standing)
    }
}

/// Tolerance of the smallest distance from the hinge circle, m: the bound
/// found is at most this below the smallest value.
pub const CIRCLE_TOLERANCE: f64 = 1e-3;

/// Most evaluations of the search over the hinge circle (exit clause; a
/// search typically takes 50–200).
const CIRCLE_EVALUATIONS: usize = 4000;

/// The smallest of `f` over the horizontal circle of radius `radius` about
/// `centre` (the hinge circle: every place the hinge can take), with the
/// point where it is found: a lower bound within [`CIRCLE_TOLERANCE`] of
/// the smallest value (Piyavskii–Shubert: `f` is a distance, so it changes
/// by at most `radius` per radian along the circle). Values from `cutoff`
/// up are not refined: a result at or above it only says so.
///
/// With `radius` 0 it is `f(centre)` exactly.
pub fn min_over_circle(
    centre: P3,
    radius: f64,
    cutoff: f64,
    mut f: impl FnMut(P3) -> (f64, P3),
) -> (f64, P3) {
    if radius <= 0.0 {
        return f(centre);
    }
    use std::f64::consts::TAU;
    let at = |psi: f64| {
        [
            centre[0] + radius * psi.cos(),
            centre[1] + radius * psi.sin(),
            centre[2],
        ]
    };
    const START: usize = 16;
    let step = TAU / START as f64;
    let values: Vec<(f64, P3)> = (0..START).map(|k| f(at(k as f64 * step))).collect();
    let mut best = values
        .iter()
        .copied()
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .expect("16 values");
    // Intervals (lower bound, ψ₀, width, f(ψ₀), f(ψ₀ + width)), lowest
    // bound first; each split where the bound's saw-tooth is lowest.
    let lower = |f0: f64, f1: f64, w: f64| 0.5 * (f0 + f1) - 0.5 * radius * w;
    let mut open: std::collections::BinaryHeap<Interval> = (0..START)
        .map(|k| {
            let (f0, f1) = (values[k].0, values[(k + 1) % START].0);
            Interval(lower(f0, f1, step), k as f64 * step, step, f0, f1)
        })
        .collect();
    let mut evaluations = START;
    // The lowest bound of the intervals dropped.
    let mut dropped = f64::INFINITY;
    loop {
        let Some(Interval(lb, psi, w, f0, f1)) = open.pop() else {
            return (best.0.min(dropped), best.1);
        };
        if lb >= cutoff || best.0 - lb <= CIRCLE_TOLERANCE || evaluations >= CIRCLE_EVALUATIONS {
            return (lb.min(best.0).min(dropped), best.1);
        }
        let split = (0.5 * w + (f0 - f1) / (2.0 * radius)).clamp(0.05 * w, 0.95 * w);
        let m = f(at(psi + split));
        evaluations += 1;
        if m.0 < best.0 {
            best = m;
        }
        let limit = best.0 - CIRCLE_TOLERANCE;
        for i in [
            Interval(lower(f0, m.0, split), psi, split, f0, m.0),
            Interval(lower(m.0, f1, w - split), psi + split, w - split, m.0, f1),
        ] {
            // An interval that cannot hold a better value is dropped.
            if i.0 < limit {
                open.push(i);
            } else {
                dropped = dropped.min(i.0);
            }
        }
    }
}

/// An interval of the circle search, ordered lowest bound first.
#[derive(Clone, Copy)]
struct Interval(f64, f64, f64, f64, f64);

impl PartialEq for Interval {
    fn eq(&self, other: &Self) -> bool {
        self.0.total_cmp(&other.0).is_eq()
    }
}

impl Eq for Interval {}

impl PartialOrd for Interval {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Interval {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // The heap pops its largest: the lowest bound is the largest.
        other.0.total_cmp(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ohl_terrain::SurveyPoint;

    fn plane(points: &[[f64; 3]]) -> Terrain {
        let p = |q: &[f64; 3]| SurveyPoint {
            x: q[0],
            y: q[1],
            z: q[2],
            code: String::new(),
            line: String::new(),
        };
        Terrain::from_points(&points.iter().map(p).collect::<Vec<_>>(), None).unwrap()
    }

    /// Ground z = s·x over a large square.
    fn slope(s: f64) -> Terrain {
        plane(&[
            [-200.0, -200.0, -200.0 * s],
            [200.0, -200.0, 200.0 * s],
            [200.0, 200.0, 200.0 * s],
            [-200.0, 200.0, -200.0 * s],
        ])
    }

    #[test]
    fn a_tree_without_root_ball_is_a_sphere_on_level_ground() {
        let t = Tree::new([0.0, 0.0, 0.0], 20.0, 0.0);
        assert_eq!((t.root, t.length), (0.0, 20.0));
        assert!((t.theta0 - std::f64::consts::FRAC_PI_2).abs() < 1e-15);
        let th = t.theta_min(&slope(0.0), [1.0, 0.0]);
        assert!(th.abs() < 1e-12, "{th}");
        // A wire point 15 m away at 12 m: √(15² + 12²) − 20 = −0.79.
        let d = t.fall_distance(15.0, 12.0, th);
        assert!((d - (15f64.hypot(12.0) - 20.0)).abs() < 1e-12);
    }

    /// Hand calculation, k = 0.1, h' = 20 m on level ground: b_r = 2,
    /// L = √404 = 20.0998, θ₀ = atan2(20, −2) = 95.71°. A wire point at
    /// ρ = 19, ζ = 10: |q − H| = √(17² + 10²) = 19.7231, d = −0.3767 (the
    /// trunk passes it); without the root ball √(19² + 10²) − 20 = 1.4709.
    #[test]
    fn the_root_ball_moves_the_hinge_out() {
        let t = Tree::new([0.0, 0.0, 0.0], 20.0, 0.1);
        assert!((t.root - 2.0).abs() < 1e-12);
        assert!((t.length - 404f64.sqrt()).abs() < 1e-12);
        assert!((t.theta0.to_degrees() - 95.710_593_137_499_64).abs() < 1e-9);
        let th = t.theta_min(&slope(0.0), [1.0, 0.0]);
        assert!(th.abs() < 1e-12);
        let d = t.fall_distance(19.0, 10.0, th);
        assert!(
            (d - (17f64.hypot(10.0) - 404f64.sqrt())).abs() < 1e-12,
            "{d}"
        );
        assert!((d + 0.376_668).abs() < 1e-6);
    }

    /// Downhill: ground falling 30 % (z = −0.3 x), the tree at the origin
    /// falling towards +x. Seen from H = (b_r, 0), the ground at u is
    /// −0.3 (b_r + u): its angle −atan(0.3 (1 + b_r/u)) is largest at u = L.
    /// k = 0: −atan 0.3 = −16.70°. k = 0.1, h' = 20: −atan(0.3 · (1 +
    /// 2/20.0998)) = −18.26°.
    #[test]
    fn falling_downhill_rests_below_the_horizontal() {
        let g = slope(-0.3);
        let t0 = Tree::new([0.0, 0.0, 0.0], 20.0, 0.0);
        let th0 = t0.theta_min(&g, [1.0, 0.0]);
        assert!(
            (th0 - (-0.3f64).atan()).abs() < 1e-9,
            "{}",
            th0.to_degrees()
        );
        let t = Tree::new([0.0, 0.0, 0.0], 20.0, 0.1);
        let th = t.theta_min(&g, [1.0, 0.0]);
        let expect = (-0.3 * (1.0 + 2.0 / 404f64.sqrt())).atan();
        assert!(
            (th - expect).abs() < 1e-9,
            "{} {}",
            th.to_degrees(),
            expect.to_degrees()
        );
        // A wire point below the base's height but above the resting
        // trunk is inside the sector: ρ = 15, ζ = −3 → θ_q = atan2(−3, 13)
        // = −12.99° > −18.26°.
        assert!(t.in_sector(15.0, -3.0, th));
        let d = t.fall_distance(15.0, -3.0, th);
        assert!((d - (13f64.hypot(3.0) - 404f64.sqrt())).abs() < 1e-12);
        // Below the resting trunk (ρ = 15, ζ = −6: −24.8°): to the trunk
        // at rest.
        assert!(!t.in_sector(15.0, -6.0, th));
        let d = t.fall_distance(15.0, -6.0, th);
        let (ex, ez) = (404f64.sqrt() * th.cos(), 404f64.sqrt() * th.sin());
        let s = (13.0 * ex - 6.0 * ez) / (ex * ex + ez * ez);
        assert!((d - (13.0 - s * ex).hypot(-6.0 - s * ez)).abs() < 1e-12);
    }

    /// Uphill (z = 0.3 x): from the base the ground rises at its slope,
    /// atan 0.3 = 16.70°, also with a root ball.
    #[test]
    fn falling_uphill_rests_at_the_slope() {
        let g = slope(0.3);
        let t = Tree::new([0.0, 0.0, 0.0], 20.0, 0.1);
        let th = t.theta_min(&g, [1.0, 0.0]);
        assert!((th - 0.3f64.atan()).abs() < 1e-9, "{}", th.to_degrees());
    }

    #[test]
    fn a_ridge_shelters_what_lies_below_the_resting_trunk() {
        // Level ground with a ridge 5 m high at x = 10, from x = 8 to 12
        // (a tent of TIN triangles).
        let g = plane(&[
            [-50.0, -50.0, 0.0],
            [-50.0, 50.0, 0.0],
            [8.0, -50.0, 0.0],
            [8.0, 50.0, 0.0],
            [10.0, -50.0, 5.0],
            [10.0, 50.0, 5.0],
            [12.0, -50.0, 0.0],
            [12.0, 50.0, 0.0],
            [80.0, -50.0, 0.0],
            [80.0, 50.0, 0.0],
        ]);
        let t = Tree::new([0.0, 0.0, 0.0], 20.0, 0.0);
        let th = t.theta_min(&g, [1.0, 0.0]);
        assert!((th - 0.5f64.atan()).abs() < 1e-9, "{}", th.to_degrees());
        // A wire point 18 m out, 2 m up: below the resting trunk
        // (atan(2/18) < atan 0.5): its distance to the trunk lying on the
        // ridge, not the sphere's −17.9 m.
        let d = t.fall_distance(18.0, 2.0, th);
        let e = [20.0 * th.cos(), 20.0 * th.sin()];
        let s = (18.0 * e[0] + 2.0 * e[1]) / 400.0;
        assert!((d - (18.0 - s * e[0]).hypot(2.0 - s * e[1])).abs() < 1e-12);
        assert!(d > 0.0);
    }

    #[test]
    fn the_circle_search_finds_the_nearest_hinge() {
        // Distance from the circle of radius 2 about the origin to the
        // point (10, 3, 4): the nearest circle point is towards it, the
        // distance √((√109 − 2)² + 16).
        let q = [10.0, 3.0, 4.0];
        let (d, _) = min_over_circle([0.0, 0.0, 0.0], 2.0, f64::INFINITY, |c| {
            (crate::geometry::distance(c, q), q)
        });
        let exact = (109f64.sqrt() - 2.0).hypot(4.0);
        assert!(d <= exact && exact - d <= CIRCLE_TOLERANCE, "{d} {exact}");
        // Radius 0: the value at the centre.
        let (d0, _) = min_over_circle([0.0; 3], 0.0, 0.0, |c| (crate::geometry::distance(c, q), q));
        assert_eq!(d0, crate::geometry::distance([0.0; 3], q));
    }
}
