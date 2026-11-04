// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Welded plates and members lying flat on what they bolt onto (owner
//! 2026-11-04):
//!
//! - a **welded plate** is several plates welded together so that every
//!   member bolting onto the joint reaches one of them
//!   (`[[plate]] welded_to`, [`ohl_structures::StructureType::welded_assemblies`]):
//!   [`seam`] finds where two plates of an assembly meet (the weld drawn
//!   there), [`welded_plan`] which plates a joint needs — one per plane of
//!   the members arriving (their direction and the joint's leg), each
//!   welded to the others;
//! - a member's **roll** turns it about its axis: [`flat_roll`] is the
//!   turn that lays its leg bolted flat on the plate or the leg it bolts
//!   onto (members brought in from other programs sometimes lie a quarter
//!   turn off).

use crate::joints::{Arrival, JointDetail};

/// Where two plates of a welded assembly meet: the seam's two ends
/// (structure's axes, m) and, from the seam, the directions into each
/// plate (in its plane; the weld's fillet lies between them); with the
/// weld's data when the file gives them (`[[plate.weld]]`, 2026-11-04).
#[derive(Debug, Clone, PartialEq)]
pub struct Weld {
    /// The other plate.
    pub with: String,
    /// The seam's ends.
    pub ends: [[f64; 3]; 2],
    /// From the seam into this plate and into the other one, unit.
    pub sides: [[f64; 3]; 2],
    /// Fillet (the default) or full penetration.
    pub kind: ohl_structures::connections::WeldKind,
    /// The fillet's throat a, m (none: not given — not checked).
    pub size: Option<f64>,
    /// Its length given, m (none: the seam's, [`Weld::seam_length`]).
    pub length: Option<f64>,
}

impl Weld {
    /// The seam's length, m.
    pub fn seam_length(&self) -> f64 {
        norm(sub(self.ends[1], self.ends[0]))
    }

    /// Its length: given, else the seam's, m.
    pub fn length(&self) -> f64 {
        self.length.unwrap_or_else(|| self.seam_length())
    }

    /// The angle between the two plates at the seam, degrees.
    pub fn angle(&self) -> f64 {
        dot(self.sides[0], self.sides[1])
            .clamp(-1.0, 1.0)
            .acos()
            .to_degrees()
    }

    /// Its label on the drawings: "a6 × 220" (the throat and the length,
    /// mm; "a6" with the seam's length), "full pen.", none without a size.
    pub fn label(&self) -> Option<String> {
        use ohl_structures::connections::WeldKind;
        if self.kind == WeldKind::FullPenetration {
            return Some("full pen.".into());
        }
        let a = self.size?;
        Some(match self.length {
            Some(l) => format!("a{:.0} × {:.0}", a * 1000.0, l * 1000.0),
            None => format!("a{:.0}", a * 1000.0),
        })
    }
}

/// The unit normal of a flat polygon (none: degenerate).
pub(crate) fn polygon_normal(poly: &[[f64; 3]]) -> Option<[f64; 3]> {
    if poly.len() < 3 {
        return None;
    }
    // Newell's method: robust for any simple polygon.
    let mut n = [0.0; 3];
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let l = norm(n);
    (l > 1e-12).then(|| scale(n, 1.0 / l))
}

/// The stretch along the line `l0 + t·u` that polygon `poly` (in its own
/// plane) reaches within `tol` of the plane `q0`, `nq`: the parameters
/// where its edges cross that plane and of its corners within `tol` of it.
fn reach_on(
    poly: &[[f64; 3]],
    q0: [f64; 3],
    nq: [f64; 3],
    l0: [f64; 3],
    u: [f64; 3],
    tol: f64,
) -> Option<(f64, f64)> {
    let d = |p: [f64; 3]| dot(sub(p, q0), nq);
    let t = |p: [f64; 3]| dot(sub(p, l0), u);
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let mut take = |x: f64| {
        lo = lo.min(x);
        hi = hi.max(x);
    };
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (da, db) = (d(a), d(b));
        if da.abs() <= tol {
            take(t(a));
        }
        if (da > tol && db < -tol) || (da < -tol && db > tol) {
            let s = da / (da - db);
            take(t(add(a, scale(sub(b, a), s))));
        }
    }
    (lo <= hi).then_some((lo, hi))
}

/// A seam's two ends and, from it, the directions into each plate.
pub type Seam = ([[f64; 3]; 2], [[f64; 3]; 2]);

/// Where plates `a` and `b` (outlines, structure's axes) meet: the
/// stretch of the line their planes share that both reach (within
/// `tol`, m: the plates' thickness). None: parallel planes, or they do
/// not reach the same stretch.
pub fn seam(a: &[[f64; 3]], b: &[[f64; 3]], tol: f64) -> Option<Seam> {
    let (na, nb) = (polygon_normal(a)?, polygon_normal(b)?);
    let c = cross(na, nb);
    let s2 = dot(c, c);
    // Within 5° of parallel: no seam (they lie on each other).
    if s2 < 5f64.to_radians().sin().powi(2) {
        return None;
    }
    let u = scale(c, 1.0 / s2.sqrt());
    let (ha, hb) = (dot(na, a[0]), dot(nb, b[0]));
    let k = dot(na, nb);
    // The point of both planes nearest the origin.
    let det = 1.0 - k * k;
    let l0 = add(
        scale(na, (ha - hb * k) / det),
        scale(nb, (hb - ha * k) / det),
    );
    let (a0, a1) = reach_on(a, b[0], nb, l0, u, tol)?;
    let (b0, b1) = reach_on(b, a[0], na, l0, u, tol)?;
    let (lo, hi) = (a0.max(b0), a1.min(b1));
    if hi - lo < 1e-3 {
        return None;
    }
    let ends = [add(l0, scale(u, lo)), add(l0, scale(u, hi))];
    // Into each plate: square to the seam in its plane, towards its middle.
    let mid = |p: &[[f64; 3]]| {
        let n = p.len() as f64;
        p.iter().fold([0.0; 3], |s, q| add(s, scale(*q, 1.0 / n)))
    };
    let side = |n: [f64; 3], p: &[[f64; 3]]| {
        let d = cross(n, u);
        if dot(sub(mid(p), ends[0]), d) >= 0.0 {
            d
        } else {
            scale(d, -1.0)
        }
    };
    Some((ends, [side(na, a), side(nb, b)]))
}

/// One plate of a welded plate a joint needs: the members bolted onto it
/// (they lie in its plane) and its plane's normal.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedPlate {
    /// The members onto it, in the joint's order.
    pub members: Vec<String>,
    /// Its plane's unit normal.
    pub normal: [f64; 3],
}

/// The joint's legs: the members arriving of class `leg`, else those
/// within 30° of vertical (as the joint's leg is found for the members
/// bolting onto it).
pub fn joint_legs(j: &JointDetail) -> Vec<&Arrival> {
    let class: Vec<&Arrival> = j
        .arrivals
        .iter()
        .filter(|a| a.class.as_deref() == Some("leg"))
        .collect();
    if !class.is_empty() {
        return class;
    }
    j.arrivals
        .iter()
        .filter(|a| a.direction[2].abs() >= 30f64.to_radians().cos())
        .collect()
}

/// The plates a welded plate at joint `j` needs so that every member
/// bolting there reaches one (owner 2026-11-04): the members arriving
/// that are not the joint's legs ([`joint_legs`]), not modelling members
/// and not held otherwise than bolted (given welded, pinned or fixed),
/// grouped by the plane they lie in with the joint's leg (their direction
/// and the leg's; within 10°) — one plate per plane, the plates welded
/// together. A member in line with the leg (a splice) is left out. With
/// what was left out and why.
pub fn welded_plan(j: &JointDetail) -> (Vec<PlannedPlate>, Vec<String>) {
    let legs = joint_legs(j);
    let leg = legs.first().map_or([0.0, 0.0, 1.0], |a| a.direction);
    let mut out: Vec<PlannedPlate> = Vec::new();
    let mut notes = Vec::new();
    for a in &j.arrivals {
        if legs.iter().any(|l| l.member == a.member) {
            continue;
        }
        if a.modelling {
            notes.push(format!("{}: a modelling member, left out", a.member));
            continue;
        }
        if a.kind
            .value
            .is_some_and(|k| k != ohl_structures::connections::ConnectionKind::Bolted)
            && a.kind.known()
        {
            notes.push(format!(
                "{}: {} (not bolted), left out",
                a.member,
                a.kind.value.map_or("", |k| k.as_str())
            ));
            continue;
        }
        let n = cross(a.direction, leg);
        if norm(n) < 20f64.to_radians().sin() {
            notes.push(format!(
                "{}: in line with the leg (a splice), left out",
                a.member
            ));
            continue;
        }
        let n = scale(n, 1.0 / norm(n));
        match out
            .iter_mut()
            .find(|p| dot(p.normal, n).abs() >= 10f64.to_radians().cos())
        {
            Some(p) => p.members.push(a.member.clone()),
            None => out.push(PlannedPlate {
                members: vec![a.member.clone()],
                normal: n,
            }),
        }
    }
    (out, notes)
}

/// How far a member's leg bolted is from lying flat on what it bolts onto
/// and the turn about its axis that lays it flat (owner 2026-11-04: "a
/// correction angle for the members to rotate them on their axis as one
/// face need to come in contact with the plates").
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flat {
    /// The angle between the leg bolted's face and the face it bolts onto,
    /// degrees (0 to 90).
    pub off: f64,
    /// The turn to add to the member's roll, radians (−90° to 90°: the
    /// smallest that lays it flat; the outstanding leg may then point the
    /// other way, a further 180°).
    pub turn: f64,
}

/// The turn about the member's axis `axes[0]` that lays its leg bolted
/// (along its y when `leg_y`, so its face's normal is z; else along z,
/// normal y) flat on a face of normal `target`: the smallest such turn,
/// and how far off it is now. None: the target's normal runs along the
/// member (no turn lays it flat).
pub fn flat_roll(axes: [[f64; 3]; 3], leg_y: bool, target: [f64; 3]) -> Option<Flat> {
    let [x, y, z] = axes;
    let along = dot(target, x);
    let np = sub(target, scale(x, along));
    if norm(np) < 1e-6 {
        return None;
    }
    let np = scale(np, 1.0 / norm(np));
    let (a, b) = (dot(np, y), dot(np, z));
    // Turned by δ: y' = cos δ·y + sin δ·z, z' = −sin δ·y + cos δ·z.
    let turn = if leg_y {
        // z' along ±np.
        (-a).atan2(b)
    } else {
        // y' along ±np.
        b.atan2(a)
    };
    // The smallest of δ and δ ± 180°.
    let half = std::f64::consts::FRAC_PI_2;
    let mut t = turn;
    while t > half {
        t -= std::f64::consts::PI;
    }
    while t <= -half {
        t += std::f64::consts::PI;
    }
    let normal = if leg_y { z } else { y };
    let off = dot(normal, scale(target, 1.0 / norm(target)))
        .abs()
        .min(1.0)
        .acos()
        .to_degrees();
    Some(Flat { off, turn: t })
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

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An angle along x with its legs along y and z: bolted by the y leg
    /// (normal z) onto a face of normal y it lies a quarter turn off; the
    /// turn found lays it flat (its z then along ±y).
    #[test]
    fn the_flat_roll_lays_the_leg_on_the_face() {
        let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let f = flat_roll(axes, true, [0.0, 1.0, 0.0]).unwrap();
        assert!((f.off - 90.0).abs() < 1e-9, "{f:?}");
        assert!((f.turn.abs() - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        // Already flat: no turn.
        let f = flat_roll(axes, true, [0.0, 0.0, -1.0]).unwrap();
        assert!(f.off < 1e-9 && f.turn.abs() < 1e-9, "{f:?}");
        // 30° off, the leg along z (normal y).
        let t = 30f64.to_radians();
        let n = [0.0, t.cos(), t.sin()];
        let f = flat_roll(axes, false, n).unwrap();
        assert!((f.off - 30.0).abs() < 1e-9, "{f:?}");
        // Turned by it, the normal (y') lies along n.
        let (s, c) = f.turn.sin_cos();
        let y2 = [0.0, c, s];
        assert!((dot(y2, n).abs() - 1.0).abs() < 1e-9);
        // The target along the member: nothing turns it flat.
        assert!(flat_roll(axes, true, [1.0, 0.0, 0.0]).is_none());
    }

    /// Two squares meeting along the z axis (one in the x–z plane, one in
    /// the y–z plane): the seam is the stretch both reach.
    #[test]
    fn two_plates_meet_along_their_shared_edge() {
        let a = [
            [0.0, 0.0, 0.0],
            [0.2, 0.0, 0.0],
            [0.2, 0.0, 0.3],
            [0.0, 0.0, 0.3],
        ];
        let b = [
            [0.0, 0.0, 0.1],
            [0.0, 0.2, 0.1],
            [0.0, 0.2, 0.5],
            [0.0, 0.0, 0.5],
        ];
        let (ends, sides) = seam(&a, &b, 0.01).unwrap();
        let mut z = [ends[0][2], ends[1][2]];
        z.sort_by(f64::total_cmp);
        assert!(
            (z[0] - 0.1).abs() < 1e-9 && (z[1] - 0.3).abs() < 1e-9,
            "{ends:?}"
        );
        assert!(sides[0][0] > 0.99 && sides[1][1] > 0.99, "{sides:?}");
        // Parallel plates: no seam.
        let c: Vec<[f64; 3]> = a.iter().map(|p| [p[0], 0.05, p[2]]).collect();
        assert!(seam(&a, &c, 0.01).is_none());
    }
}
