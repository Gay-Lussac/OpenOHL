// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A joint's bolt groups under the members' forces together by ASCE 10-97
//! (stage 4.5 of the joint details, 2026-11-19; built, in active
//! development, not approved yet): OpenOHL sends each physical bolt with
//! the force it carries in every case (the vector sum of every member end's
//! and plate's share, the group's moment shared out elastically) and its
//! distances to the checked plies' edges along and square to that force;
//! here each bolt is checked by the standard's rules (theory page
//! `docs/theory/joint-groups.md`):
//!
//! - bolt shear (4.3.2) on the planes sent;
//! - bearing on each ply (4.4): 1.5 F_u d t, F_u the lower of the ply's
//!   and the bolt's;
//! - the end distance in each ply in the force's direction (4.5.1, with
//!   the bolt's own force), the spacing to the next hole along it (4.5.2),
//!   the edge distance square to it (4.5.3: an angle's rolled toe, a
//!   plate's sheared edge).
//!
//! Where a ply's distances are not known (where its steel stops not
//! given), the end distance is not checked — said NOT ON THE SAFE SIDE
//! (the standard's bearing takes no distance).

use crate::joints::{Checked, PlateCheck};

/// A ply the bolts bear on.
#[derive(Debug, Clone, PartialEq)]
pub struct Ply {
    /// Its id (a member's or a plate's).
    pub part: String,
    /// A member (an angle's leg, its free edge rolled); false: a plate.
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

/// The checks of bolt `b` bearing on `plies` under load `l`.
pub fn bolt_checks(b: &Bolt, plies: &[Ply], l: &Load) -> Checked {
    let mut out = Checked::default();
    let d = b.diameter;
    let f = l.force;
    match crate::asce10::bolt_shear(
        d,
        b.planes.max(1),
        b.shear_capacity,
        b.shank,
        b.threads_in,
        b.ultimate,
    ) {
        Some(cap) => out.checks.push(PlateCheck {
            what: format!(
                "bolt shear, {} plane{} (ASCE 10-97 4.3.2)",
                b.planes.max(1),
                if b.planes.max(1) == 1 { "" } else { "s" }
            ),
            demand: f,
            resistance: cap,
            distance: false,
        }),
        None => out.not_safe.push(
            "bolt shear not checked (no shear capacity nor ultimate strength of the bolt)".into(),
        ),
    }
    for (i, p) in plies.iter().enumerate() {
        let t = p.thickness;
        let Some(fu_p) = p.fu else {
            out.not_safe.push(format!(
                "bearing on {} not checked (no ultimate strength)",
                p.part
            ));
            continue;
        };
        let fu = match b.ultimate {
            Some(fb) => fu_p.min(fb),
            None => {
                let w = format!("bearing on {}: its F_u (the bolt's not given)", p.part);
                if !out.notes.contains(&w) {
                    out.notes.push(w);
                }
                fu_p
            }
        };
        out.checks.push(PlateCheck {
            what: format!("bearing on {} (ASCE 10-97 4.4)", p.part),
            demand: f,
            resistance: 1.5 * fu * d * t,
            distance: false,
        });
        let [end_min, space_min, edge_min] =
            crate::asce10::bolt_minimums(d, t, fu_p, Some(f), p.redundant, !p.member);
        if let Some(e) = l.ends.get(i).copied().flatten() {
            if p.known {
                out.checks.push(PlateCheck {
                    what: format!(
                        "end distance in {} along the bolt's force {:.0} mm, ≥ {:.0} mm (ASCE \
                         10-97 4.5.1)",
                        p.part,
                        mm(e),
                        mm(end_min)
                    ),
                    demand: end_min,
                    resistance: e,
                    distance: true,
                });
            } else {
                out.not_safe.push(format!(
                    "end distance in {} not checked (where its steel stops at the joint is not \
                     given)",
                    p.part
                ));
            }
        }
        if let Some(a) = l.ahead.get(i).copied().flatten() {
            out.checks.push(PlateCheck {
                what: format!(
                    "spacing in {} to the next hole along the bolt's force {:.0} mm, ≥ {:.0} mm \
                     (ASCE 10-97 4.5.2)",
                    p.part,
                    mm(a),
                    mm(space_min)
                ),
                demand: space_min,
                resistance: a,
                distance: true,
            });
        }
        if let Some(s) = l.sides.get(i).copied().flatten() {
            out.checks.push(PlateCheck {
                what: format!(
                    "edge distance in {} square to the bolt's force {:.0} mm, ≥ {:.0} mm (ASCE \
                     10-97 4.5.3, {})",
                    p.part,
                    mm(s),
                    mm(edge_min),
                    if p.member {
                        "Eq. 4.5-6: rolled edge"
                    } else {
                        "Eq. 4.5-7: sheared edge"
                    }
                ),
                demand: edge_min,
                resistance: s,
                distance: true,
            });
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
                if !out.notes.contains(w) {
                    out.notes.push(w.clone());
                }
            }
            for w in &c.not_safe {
                if !out.not_safe.contains(w) {
                    out.not_safe.push(w.clone());
                }
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

    fn leg() -> Ply {
        Ply {
            part: "L1".into(),
            member: true,
            thickness: 0.008,
            fy: Some(355e6),
            fu: Some(510e6),
            redundant: false,
            known: true,
        }
    }

    fn m16(carries: &[&str]) -> Bolt {
        Bolt {
            diameter: 0.016,
            hole: Some(0.0175),
            ultimate: Some(800e6),
            shear_capacity: None,
            shank: None,
            threads_in: None,
            planes: 1,
            carries: carries.iter().map(|s| (*s).to_owned()).collect(),
            at: [0.0, 0.0],
        }
    }

    /// One bolt shared by a diagonal and a redundant on the leg (invented,
    /// by hand): 60 kN together. Bolt shear 0.62 × 800 × 0.6 × π 16²/4 =
    /// 59.84 kN (one plane: 100.3 %); bearing on the 8 mm leg 1.5 × 510 ×
    /// 16 × 8 = 97.92 kN (61.3 %); end distance 1.2 × 60 000 / (510 × 8) =
    /// 17.6 mm ≥ 1.3 d = 20.8 mm → 20.8 mm needed, 30 there; edge 0.85 ×
    /// 20.8 = 17.7 mm, 25 there.
    #[test]
    fn a_shared_bolt_on_the_leg() {
        let b = m16(&["D1", "R1"]);
        let l = Load {
            force: 60e3,
            ends: vec![Some(0.030)],
            sides: vec![Some(0.025)],
            ahead: vec![None],
        };
        let c = bolt_checks(&b, &[leg()], &l);
        let shear = c
            .checks
            .iter()
            .find(|x| x.what.starts_with("bolt shear"))
            .unwrap();
        let cap = 0.62 * 800e6 * 0.6 * std::f64::consts::PI * 0.016f64.powi(2) / 4.0;
        assert!((shear.resistance - cap).abs() < 1e-6);
        let bearing = c
            .checks
            .iter()
            .find(|x| x.what.starts_with("bearing"))
            .unwrap();
        assert!((bearing.resistance - 97_920.0).abs() < 1e-6);
        let end = c.checks.iter().find(|x| x.what.starts_with("end")).unwrap();
        assert!((end.demand - 0.0208).abs() < 1e-12 && end.resistance == 0.030);
        let (u, g) = c.governing(1.0).unwrap();
        assert!(g.what.starts_with("bolt shear") && (u - 60e3 / cap).abs() < 1e-12);
        assert!(c.not_safe.is_empty());
    }

    /// Where the leg's end is not known the end distance is not checked,
    /// said; the group takes its most loaded bolt per case.
    #[test]
    fn the_most_loaded_bolt_governs() {
        let g = Group {
            plies: vec![Ply {
                known: false,
                ..leg()
            }],
            bolts: vec![m16(&["D1"]), m16(&["D1", "R1"])],
            loads: vec![vec![
                Load {
                    force: 20e3,
                    ends: vec![Some(0.03)],
                    sides: vec![None],
                    ahead: vec![None],
                },
                Load {
                    force: 40e3,
                    ends: vec![None],
                    sides: vec![None],
                    ahead: vec![Some(0.05)],
                },
            ]],
        };
        let r = check_group(&g, &[1.0]);
        let (u, w) = r.usages[0].clone().unwrap();
        assert!(w.starts_with("bolt 2 of 2 (carries D1, R1"), "{w}");
        assert!(u > 0.6 && u < 0.7, "{u}");
        assert!(
            r.not_safe
                .iter()
                .any(|x| x.contains("end distance in L1 not checked"))
        );
    }
}
