// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A joint's bolt groups under the members' forces together by SIA
//! 263:2013 (stage 4.5 of the joint details, 2026-11-19; built, in active
//! development, not approved yet): OpenOHL sends each physical bolt with
//! the force it carries in every case (the vector sum of every member end's
//! and plate's share, the group's moment shared out elastically) and its
//! distances to the checked plies' edges along and square to that force;
//! here each bolt is checked by the standard's rules (theory page
//! `docs/theory/joint-groups.md`):
//!
//! - bolt shear per plane, Table 16 (73), (74) or (75), on the planes sent;
//! - bearing on each ply, Table 16 (76): F_b,Rd = min(0.85 e_1/d_0, 2.4)
//!   f_u d t / γ_M2 with e_1 the distance along the bolt's force to the
//!   ply's edge (none: 2.4), replaced by p_1 − d_0/2 where the next hole
//!   along the force is nearer than e_1 + d_0/2;
//! - the end, spacing and edge distances of Table 17 along and square to
//!   the force (Figure 31: e_2 and p_2 also oblique to the force).
//!
//! Where a ply's distances are not known (where its steel stops not
//! given), e_1 is taken as the smaller of the one found and Table 17's
//! minimum (the ply may end as near the bolt as the code allows: on the
//! safe side, said), as the member check does for the leg a member bolts
//! onto.

use crate::joints::{Checked, PlateCheck};
use crate::params::gamma_m2;
use crate::sia263;

/// A ply the bolts bear on.
#[derive(Debug, Clone, PartialEq)]
pub struct Ply {
    /// Its id (a member's or a plate's).
    pub part: String,
    /// A member (an angle's leg); false: a plate.
    pub member: bool,
    /// Thickness, m.
    pub thickness: f64,
    /// Yield strength, Pa.
    pub fy: Option<f64>,
    /// Ultimate strength, Pa.
    pub fu: Option<f64>,
    /// A redundant member.
    pub redundant: bool,
    /// Its edges' distances are known.
    pub known: bool,
}

/// One physical bolt.
#[derive(Debug, Clone, PartialEq)]
pub struct Bolt {
    /// Diameter, m.
    pub diameter: f64,
    /// Hole, m.
    pub hole: Option<f64>,
    /// The bolt's ultimate strength, Pa.
    pub ultimate: Option<f64>,
    /// Shear capacity per plane through the threads, N.
    pub shear_capacity: Option<f64>,
    /// The same through the shank, N.
    pub shank: Option<f64>,
    /// The threads are in the shear plane.
    pub threads_in: Option<bool>,
    /// Its grade as the files name it ("" not given).
    pub grade: String,
    /// The planes its force is taken on.
    pub planes: u32,
    /// What it carries (member ends, plates).
    pub carries: Vec<String>,
    /// Where it is from the group's centroid, m.
    pub at: [f64; 2],
}

/// One bolt in one case: its force and, per ply, the distances along the
/// direction it pushes that ply (to the edge, to the next hole) and square
/// to it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Load {
    /// The force's size, N.
    pub force: f64,
    /// Per ply: to the edge along it, m (none: no edge).
    pub ends: Vec<Option<f64>>,
    /// Per ply: to the nearer edge square to it, m (none: none).
    pub sides: Vec<Option<f64>>,
    /// Per ply: to the next hole's centre along it, m (none: none).
    pub ahead: Vec<Option<f64>>,
}

/// One group.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// The plies checked.
    pub plies: Vec<Ply>,
    /// The bolts.
    pub bolts: Vec<Bolt>,
    /// Per case, per bolt.
    pub loads: Vec<Vec<Load>>,
}

/// What a group gives: per case its usage and the check (none: nothing
/// checked in that case), the governing check, notes and what is not on
/// the safe side.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GroupResult {
    /// Per case.
    pub usages: Vec<Option<(f64, String)>>,
    /// The governing check over the cases.
    pub governs: String,
    /// Notes.
    pub notes: Vec<String>,
    /// Not on the safe side or not checked.
    pub not_safe: Vec<String>,
}

fn mm(x: f64) -> f64 {
    x * 1000.0
}

fn note(out: &mut Vec<String>, w: String) {
    if !out.contains(&w) {
        out.push(w);
    }
}

/// The checks of bolt `b` bearing on `plies` under load `l`.
pub fn bolt_checks(b: &Bolt, plies: &[Ply], l: &Load) -> Checked {
    let mut out = Checked::default();
    let d = b.diameter;
    let f = l.force;
    let grade = crate::model::Grade::parse(&b.grade);
    match sia263::bolt_shear(
        d,
        b.planes.max(1),
        b.shear_capacity,
        b.shank,
        b.threads_in,
        b.ultimate,
        grade,
    ) {
        Some((cap, words)) => {
            out.checks.push(PlateCheck {
                what: format!(
                    "bolt shear, {} plane{} ({words})",
                    b.planes.max(1),
                    if b.planes.max(1) == 1 { "" } else { "s" }
                ),
                demand: f,
                resistance: cap,
                distance: false,
            });
            if b.shear_capacity.is_none() && !sia263::shear_factor(grade).1 {
                note(
                    &mut out.notes,
                    "bolt grade not known: the lower shear factor 0.5 (SIA 263 Table 16, (74))"
                        .into(),
                );
            }
        }
        None => out.not_safe.push(
            "bolt shear not checked (no shear capacity nor ultimate strength of the bolt)".into(),
        ),
    }
    let d0 = match b.hole.or_else(|| sia263::standard_hole(d)) {
        Some(h) => {
            if b.hole.is_none() {
                note(&mut out.notes, "standard hole (SIA 263 Table 17)".into());
            }
            h
        }
        None => {
            note(&mut out.notes, "hole: the bolt + 3 mm".into());
            d + 0.003
        }
    };
    let minimums = sia263::bolt_minimums(d);
    for (i, p) in plies.iter().enumerate() {
        let t = p.thickness;
        let end = l.ends.get(i).copied().flatten();
        let side = l.sides.get(i).copied().flatten();
        let ahead = l.ahead.get(i).copied().flatten();
        // e_1: the distance to the edge along the force; not known: at
        // most Table 17's minimum (on the safe side).
        let mut e1 = end.unwrap_or(f64::INFINITY);
        let mut why = String::new();
        if !p.known && end.is_some() {
            match minimums {
                Some([e1_min, _, _]) => {
                    if e1_min < e1 {
                        e1 = e1_min;
                        why = format!(
                            ", e_1 at most Table 17's {:.0} mm (where {}'s steel stops is not \
                             given: on the safe side)",
                            mm(e1_min),
                            p.part
                        );
                    }
                }
                None => out.not_safe.push(format!(
                    "bearing on {}: where its steel stops is not given and SIA 263 Table 17 has \
                     no M{:.0} — e_1 as found, not checked whether it ends nearer",
                    p.part,
                    mm(d)
                )),
            }
        }
        if let Some(p1) = ahead
            && p1 < e1 + d0 / 2.0
        {
            e1 = p1 - d0 / 2.0;
            why = format!(", e_1 = p_1 − d_0/2 = {:.0} mm (the next hole)", mm(e1));
        }
        match p.fu {
            Some(fu) => {
                let k = (0.85 * e1 / d0).clamp(0.0, 2.4);
                out.checks.push(PlateCheck {
                    what: format!(
                        "bearing on {} (SIA 263 Table 16, (76): {}{why})",
                        p.part,
                        if e1.is_finite() {
                            format!("e_1 {:.0} mm along the force, k {:.2}", mm(e1), k)
                        } else {
                            "no edge along the force, k 2.4".to_owned()
                        }
                    ),
                    demand: f,
                    resistance: k * fu * d * t / gamma_m2(),
                    distance: false,
                });
            }
            None => out.not_safe.push(format!(
                "bearing on {} not checked (no ultimate strength)",
                p.part
            )),
        }
        if side.is_some_and(|e2| e2 < d0) {
            note(
                &mut out.notes,
                format!(
                    "(76) asks e_2 ≥ d_0 ({:.0} mm): not met in {} for a bolt",
                    mm(d0),
                    p.part
                ),
            );
        }
        match minimums {
            Some([e1_min, p1_min, e2_min]) => {
                if let Some(e) = end
                    && p.known
                {
                    out.checks.push(PlateCheck {
                        what: format!(
                            "end distance in {} along the bolt's force {:.0} mm, ≥ {:.0} mm (SIA \
                             263 Table 17, e_1)",
                            p.part,
                            mm(e),
                            mm(e1_min)
                        ),
                        demand: e1_min,
                        resistance: e,
                        distance: true,
                    });
                }
                if let Some(a) = ahead {
                    out.checks.push(PlateCheck {
                        what: format!(
                            "spacing in {} to the next hole along the bolt's force {:.0} mm, ≥ \
                             {:.0} mm (SIA 263 Table 17, p_1)",
                            p.part,
                            mm(a),
                            mm(p1_min)
                        ),
                        demand: p1_min,
                        resistance: a,
                        distance: true,
                    });
                }
                if let Some(s) = side {
                    out.checks.push(PlateCheck {
                        what: format!(
                            "edge distance in {} square to the bolt's force {:.0} mm, ≥ {:.0} mm \
                             (SIA 263 Table 17, e_2)",
                            p.part,
                            mm(s),
                            mm(e2_min)
                        ),
                        demand: e2_min,
                        resistance: s,
                        distance: true,
                    });
                }
            }
            None => note(
                &mut out.notes,
                format!(
                    "distances not checked: SIA 263 Table 17 has no M{:.0}",
                    mm(d)
                ),
            ),
        }
    }
    out
}

/// Every bolt of `g` checked in every case (`strengths`: each case's
/// strength factor): per case the most loaded bolt's usage and check.
pub fn check_group(g: &Group, strengths: &[f64]) -> GroupResult {
    let mut out = GroupResult::default();
    let mut worst: Option<f64> = None;
    for (n, strength) in strengths.iter().enumerate() {
        let mut best: Option<(f64, String)> = None;
        for (k, b) in g.bolts.iter().enumerate() {
            let Some(l) = g.loads.get(n).and_then(|x| x.get(k)) else {
                continue;
            };
            let c = bolt_checks(b, &g.plies, l);
            for w in &c.notes {
                note(&mut out.notes, w.clone());
            }
            for w in &c.not_safe {
                note(&mut out.not_safe, w.clone());
            }
            if let Some((u, chk)) = c.governing(*strength)
                && best
                    .as_ref()
                    .is_none_or(|x| crate::check::usage_order(u, x.0).is_gt())
            {
                best = Some((
                    u,
                    format!(
                        "bolt {} of {} (carries {}; at {:+.0}, {:+.0} mm), {:.1} kN: {}",
                        k + 1,
                        g.bolts.len(),
                        b.carries.join(", "),
                        mm(b.at[0]),
                        mm(b.at[1]),
                        l.force / 1000.0,
                        chk.what
                    ),
                ));
            }
        }
        if let Some((u, w)) = &best
            && worst.is_none_or(|x| crate::check::usage_order(*u, x).is_gt())
        {
            worst = Some(*u);
            out.governs = w.clone();
        }
        out.usages.push(best);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leg(known: bool) -> Ply {
        Ply {
            part: "L1".into(),
            member: true,
            thickness: 0.008,
            fy: Some(355e6),
            fu: Some(510e6),
            redundant: false,
            known,
        }
    }

    fn m16() -> Bolt {
        Bolt {
            diameter: 0.016,
            hole: None,
            ultimate: Some(800e6),
            shear_capacity: None,
            shank: None,
            threads_in: None,
            grade: "8.8".into(),
            planes: 1,
            carries: vec!["D1".into(), "R1".into()],
            at: [0.0, 0.0],
        }
    }

    /// A bolt on the leg pushed towards its toe (invented, by hand): e_1 =
    /// 30 mm along the force, d_0 18 mm (Table 17): k = 0.85 × 30 / 18 =
    /// 1.417, F_b,Rd = 1.417 × 510 × 16 × 8 / 1.25 = 73.98 kN; pushed along
    /// the leg (no edge): k 2.4, 125.34 kN; the leg's end not known:
    /// e_1 = min(30, 25) mm → k 1.181, 61.65 kN. Bolt shear (73), 8.8:
    /// 0.6 × 800 × 0.6 × 201.06 / 1.25 = 46.32 kN per plane.
    #[test]
    fn bearing_along_the_bolts_force() {
        let l = |end: Option<f64>| Load {
            force: 40e3,
            ends: vec![end],
            sides: vec![Some(0.040)],
            ahead: vec![None],
        };
        let bearing = |c: &Checked| {
            c.checks
                .iter()
                .find(|x| x.what.starts_with("bearing"))
                .unwrap()
                .resistance
        };
        let c = bolt_checks(&m16(), &[leg(true)], &l(Some(0.030)));
        let want = 0.85 * 30.0 / 18.0 * 510e6 * 0.016 * 0.008 / 1.25;
        assert!((bearing(&c) - want).abs() < 1e-6, "{}", bearing(&c));
        let c = bolt_checks(&m16(), &[leg(true)], &l(None));
        assert!((bearing(&c) - 2.4 * 510e6 * 0.016 * 0.008 / 1.25).abs() < 1e-6);
        let c = bolt_checks(&m16(), &[leg(false)], &l(Some(0.030)));
        let want = 0.85 * 25.0 / 18.0 * 510e6 * 0.016 * 0.008 / 1.25;
        assert!((bearing(&c) - want).abs() < 1e-6);
        assert!(!c.checks.iter().any(|x| x.what.starts_with("end distance")));
        let shear = c
            .checks
            .iter()
            .find(|x| x.what.starts_with("bolt shear"))
            .unwrap();
        let area = std::f64::consts::PI * 0.016f64.powi(2) / 4.0;
        assert!((shear.resistance - 0.6 * 800e6 * 0.6 * area / 1.25).abs() < 1e-6);
    }

    /// The next hole along the force nearer than e_1 + d_0/2: e_1 = p_1 −
    /// d_0/2 (Table 16).
    #[test]
    fn a_hole_ahead_shortens_e1() {
        let l = Load {
            force: 40e3,
            ends: vec![None],
            sides: vec![None],
            ahead: vec![Some(0.045)],
        };
        let g = Group {
            plies: vec![leg(true)],
            bolts: vec![m16()],
            loads: vec![vec![l]],
        };
        let r = check_group(&g, &[1.0]);
        let (u, w) = r.usages[0].clone().unwrap();
        // Bearing: e_1 = 45 − 9 = 36 mm, k 1.7, 88.74 kN; shear 46.32 kN
        // governs (86.4 %).
        let area = std::f64::consts::PI * 0.016f64.powi(2) / 4.0;
        assert!(
            (u - 40e3 / (0.6 * 800e6 * 0.6 * area / 1.25)).abs() < 1e-9,
            "{u}"
        );
        assert!(w.contains("bolt shear"), "{w}");
    }
}
