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
//!
//! **The bolts in tension** (2026-11-19, `joint-check-groups-tension`):
//! OpenOHL sends each bolt's pull per case and the flanges it bends; here
//! the prying force (4.3.3 asks for "any tensile stress resulting from
//! prying action" and gives no method: the T-stub model of Struik and de
//! Back, [`prying`], on the safe side), the bolt's tension (4.3.3: F_t =
//! 0.6 F_u, its case (b) — the proof-load stress is not in the data), its
//! shear and tension together (4.3.4, Eq. 4.3-2) and each flange's bending
//! (the model's mechanism).

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
    /// Per bolt the total thickness of the packings in its grip, m (0:
    /// none; none: not known); none: not sent (an export before
    /// `joint-check-groups-packing`, O49 stage 1c).
    pub packings: Option<Vec<Option<f64>>>,
}

/// What ASCE 10-97 says of packings in a group's bolts' grips (O49 stage
/// 1c, 2026-11-27): nothing — no rule for packings or fillers was found in
/// ASCE 10-97 (4.3.2, 4.4, C4.3.2 read) nor in ASCE/SEI 48-19 (6.2.2,
/// C6.2.2); its bolt shear is not reduced. A packing thicker than a third
/// of the bolt's diameter bends the bolt across it and lowers its shear
/// resistance (other codes reduce it from there): said NOT ON THE SAFE
/// SIDE, with what makes it safe. Returns (notes, not safe).
pub fn packing_words(g: &Group) -> (Vec<String>, Vec<String>) {
    let mut notes = Vec::new();
    let mut not_safe = Vec::new();
    let Some(p) = &g.packings else {
        return (notes, not_safe);
    };
    let mut thick: Option<(f64, f64)> = None;
    let mut unknown = false;
    let mut any = false;
    for (k, b) in g.bolts.iter().enumerate() {
        match p.get(k).copied().flatten() {
            Some(tp) if tp > 0.0 => {
                any = true;
                if tp > b.diameter / 3.0 && thick.is_none_or(|x| tp > x.0) {
                    thick = Some((tp, b.diameter));
                }
            }
            Some(_) => {}
            None => unknown = true,
        }
    }
    if any {
        notes.push(
            "packings in the bolts' grips: ASCE 10-97 gives no reduction for them (none found), \
             the bolt shear not reduced"
                .into(),
        );
    }
    if let Some((tp, d)) = thick {
        not_safe.push(format!(
            "a packing {:.0} mm thick in a bolt's grip, above a third of its {:.0} mm diameter: \
             the bolt bends across it and resists less shear, which ASCE 10-97 does not cover \
             (no rule found) — keep the packing at d/3 or less, or check the bolt by hand",
            mm(tp),
            mm(d)
        ));
    }
    if unknown {
        not_safe.push(
            "the packings in a bolt's grip not known: a thick packing lowers the bolt's shear \
             resistance — give the packings"
                .into(),
        );
    }
    (notes, not_safe)
}

/// A flange a bolt bends as it pulls (a T-stub's flange; theory page).
#[derive(Debug, Clone, PartialEq)]
pub struct Tee {
    /// The member or plate.
    pub part: String,
    /// Its thickness, m.
    pub thickness: f64,
    /// Its yield strength, Pa.
    pub fy: Option<f64>,
    /// From the bolt to its free edge, m (`a`).
    pub edge: f64,
    /// From the bolt to the web's face, m (`b`).
    pub web: f64,
    /// Its length along the bolts the bolt takes, m (`w`).
    pub width: f64,
}

/// A bolt's data in tension.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BoltTension {
    /// Its library's tension capacity (its ultimate tensile load), N.
    pub capacity: Option<f64>,
    /// The flanges it bends.
    pub tees: Vec<Tee>,
    /// The plates it bends (2026-11-19): flanges without a web — the
    /// whole moment of the pull at the bolt line ([`plate_prying`]).
    pub plates: Vec<Tee>,
    /// The plies it pulls whose bending is not known, in words.
    pub unknown: Vec<String>,
}

/// A group's bolts in tension: per bolt its data, per case per bolt its
/// pull, N.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GroupTension {
    /// Per bolt (the group's order).
    pub bolts: Vec<BoltTension>,
    /// Per case, per bolt.
    pub loads: Vec<Vec<f64>>,
}

/// The prying force on a bolt (diameter `d`, hole `d0`) pulled by `t`
/// through flanges `tees`: the T-stub model of Struik and de Back as
/// Kulak, Fisher & Struik give it (*Guide to Design Criteria for Bolted and
/// Riveted Joints*, 2nd ed., 1987, Eqs. 17.8–17.12, with the modified
/// distances a′ = a + d/2, b′ = b − d/2 and a ≤ 1.25 b of 17.5 and
/// 17.6.1): B = T (1 + δα b′ / ((1 + δα) a′)), taken with α = δ = 1 — the
/// largest the model gives, B rising with both (on the safe side) — so Q
/// = T b′ / (2 a′); the flange giving the most governs (the least stiff
/// part, Guide 17.6.1). With each flange's bending (the model's mechanism,
/// Eq. 17.12 with α = 1: T b′ ≤ (1 + δ) w t² f_y / 4, δ = 1 − d₀/w; f_y
/// divided by `fy_factor`, `clause` its words) and what is not covered.
/// Returns (Q, the flange that gives it, the flanges' checks, not safe).
pub fn prying(
    t: f64,
    d: f64,
    d0: f64,
    tees: &[Tee],
    fy_factor: f64,
    clause: &str,
) -> (f64, Option<String>, Vec<PlateCheck>, Vec<String>) {
    let mut q = 0.0;
    let mut by = None;
    let mut checks = Vec::new();
    let mut not_safe = Vec::new();
    for x in tees {
        let a = x.edge.min(1.25 * x.web.max(0.0));
        let a1 = a + d / 2.0;
        let b1 = x.web - d / 2.0;
        if b1 <= 0.0 {
            not_safe.push(format!(
                "the bolt within half a diameter of {}'s web: the local prying of its head \
                 (Kulak et al. Fig. 17.19) not covered",
                x.part
            ));
            continue;
        }
        let qi = t * b1 / (2.0 * a1);
        if qi > q {
            q = qi;
            by = Some(format!(
                "{}: a′ {:.0} mm, b′ {:.0} mm",
                x.part,
                mm(a1),
                mm(b1)
            ));
        }
        let Some(fy) = x.fy else {
            not_safe.push(format!(
                "the bending of {} under the bolt's pull not checked (no yield strength)",
                x.part
            ));
            continue;
        };
        let delta = (1.0 - d0 / x.width.max(1e-9)).max(0.0);
        checks.push(PlateCheck {
            what: format!(
                "bending of {} under the bolt's pull {:.1} kN (T-stub of Struik and de Back, \
                 Kulak et al. Eq. 17.12, α = 1: T b′ ≤ (1 + δ) w t² f_y / 4{clause}; b′ {:.0} \
                 mm, w {:.0} mm, δ {:.2})",
                x.part,
                t / 1000.0,
                mm(b1),
                mm(x.width),
                delta
            ),
            demand: t * b1,
            resistance: (1.0 + delta) * x.width * x.thickness.powi(2) * fy / 4.0 / fy_factor,
            distance: false,
        });
    }
    (q, by, checks, not_safe)
}

/// The prying force on a bolt (diameter `d`, hole `d0`) pulled by `t`
/// through plates `plates` (2026-11-19, the owner's decision of that day:
/// a gusset plate's own prying and bending by the same T-stub model as an
/// angle's flange): a plate is a flange **without a web** — the pull
/// reaches it through the bolts of the members on it, which hold it
/// against no turning — so the model's moment at the web is taken as none
/// and the whole moment of the pull is at the bolt line (Kulak, Fisher &
/// Struik's Eq. 17.8 with no moment at the web; their Eq. 17.11 with
/// δα/(1 + δα) at its largest, 1): Q = T b′/a′, twice a flange's with α = δ = 1
/// (on the safe side), with the same a′ = a + d/2, b′ = b − d/2, a ≤ 1.25
/// b. Its bending: the moment at the bolt line on the net section, T b′ ≤
/// δ w t² f_y / 4 (δ = 1 − d₀/w). Returns as [`prying`].
pub fn plate_prying(
    t: f64,
    d: f64,
    d0: f64,
    plates: &[Tee],
    fy_factor: f64,
    clause: &str,
) -> (f64, Option<String>, Vec<PlateCheck>, Vec<String>) {
    let mut q = 0.0;
    let mut by = None;
    let mut checks = Vec::new();
    let mut not_safe = Vec::new();
    for x in plates {
        let a = x.edge.min(1.25 * x.web.max(0.0));
        let a1 = a + d / 2.0;
        let b1 = x.web - d / 2.0;
        if b1 <= 0.0 {
            not_safe.push(format!(
                "the bolt within half a diameter of where the pull reaches plate {}: the local \
                 prying of its head (Kulak et al. Fig. 17.19) not covered",
                x.part
            ));
            continue;
        }
        let qi = t * b1 / a1;
        if qi > q {
            q = qi;
            by = Some(format!(
                "plate {} (no web: Q = T b′/a′): a′ {:.0} mm, b′ {:.0} mm",
                x.part,
                mm(a1),
                mm(b1)
            ));
        }
        let Some(fy) = x.fy else {
            not_safe.push(format!(
                "the bending of plate {} under the bolt's pull not checked (no yield strength)",
                x.part
            ));
            continue;
        };
        let delta = (1.0 - d0 / x.width.max(1e-9)).max(0.0);
        checks.push(PlateCheck {
            what: format!(
                "bending of plate {} under the bolt's pull {:.1} kN (T-stub of Struik and de \
                 Back without a web, Kulak et al. Eq. 17.8 with no moment at the web: T b′ ≤ δ w \
                 t² f_y / 4{clause}; b′ {:.0} mm, w {:.0} mm, δ {:.2})",
                x.part,
                t / 1000.0,
                mm(b1),
                mm(x.width),
                delta
            ),
            demand: t * b1,
            resistance: delta * x.width * x.thickness.powi(2) * fy / 4.0 / fy_factor,
            distance: false,
        });
    }
    (q, by, checks, not_safe)
}

/// The tension checks of bolt `b` pulled by `t` (N) with its shear force
/// `v` (N) by ASCE 10-97: the prying force from its flanges ([`prying`]),
/// its tension (4.3.3) and its shear and tension together (4.3.4).
pub fn tension_checks(b: &Bolt, bt: &BoltTension, t: f64, v: f64) -> Checked {
    let mut out = Checked::default();
    let d = b.diameter;
    // The hole: given, else a standard hole (4.1: 1/16 in. larger).
    let d0 = b.hole.unwrap_or(d + 0.0016);
    let (qf, byf, flanges, said) = prying(t, d, d0, &bt.tees, 1.0, "");
    out.checks.extend(flanges);
    out.not_safe.extend(said);
    // The plates' prying (2026-11-19): the largest of all governs.
    let (qp, byp, plates, said) = plate_prying(t, d, d0, &bt.plates, 1.0, "");
    out.checks.extend(plates);
    out.not_safe.extend(said);
    let (q, by) = if qp > qf { (qp, byp) } else { (qf, byf) };
    for u in &bt.unknown {
        out.not_safe.push(format!(
            "the prying of {u} under the bolt's pull not worked out (its bending not known)"
        ));
    }
    let pull = t + q;
    let gross = std::f64::consts::PI * d * d / 4.0;
    // 4.3.3: F_t on the stress area A_s.
    let (rt, words) = match (bt.capacity, b.ultimate) {
        (Some(c), _) => (
            0.6 * c,
            "F_t A_s = 0.6 × the bolt's given tension capacity (its F_u A_s), 4.3.3 (b)".to_owned(),
        ),
        (None, Some(fu)) => (
            0.6 * fu * 0.6 * gross,
            "F_t = 0.6 F_u, 4.3.3 (b), on A_s taken as 0.6 of the gross area (Eq. 4.3-1 gives \
             more for a coarse thread: on the safe side)"
                .to_owned(),
        ),
        (None, None) => {
            out.not_safe.push(
                "bolt tension not checked (no tension capacity nor ultimate strength of the bolt)"
                    .into(),
            );
            return out;
        }
    };
    note_once(
        &mut out.notes,
        "bolts in tension: F_t = 0.6 F_u (ASCE 10-97 4.3.3 (b)); the ASTM proof-load stress of \
         4.3.3 (a) is not in the data (C4.3.3: 0.6 F_u a conservative estimate)"
            .into(),
    );
    let prying_words = match &by {
        Some(w) => format!(", prying {:.1} kN ({w})", q / 1000.0),
        None => String::new(),
    };
    out.checks.push(PlateCheck {
        what: format!(
            "bolt tension: pull {:.1} kN{prying_words}, {:.1} kN together (ASCE 10-97 4.3.3: \
             {words})",
            t / 1000.0,
            pull / 1000.0
        ),
        demand: pull,
        resistance: rt,
        distance: false,
    });
    if let Some(rv) = crate::asce10::bolt_shear(
        d,
        b.planes.max(1),
        b.shear_capacity,
        b.shank,
        b.threads_in,
        b.ultimate,
    ) && rv > 0.0
        && v > 0.0
    {
        let x = ((pull / rt).powi(2) + (v / rv).powi(2)).sqrt();
        out.checks.push(PlateCheck {
            what: format!(
                "bolt shear and tension together {:.1} and {:.1} kN (ASCE 10-97 4.3.4, Eq. \
                 4.3-2: (f_t/F_t)² + (f_v/F_v)² ≤ 1, its square root shown)",
                v / 1000.0,
                pull / 1000.0
            ),
            demand: x,
            resistance: 1.0,
            distance: false,
        });
    }
    out
}

fn note_once(v: &mut Vec<String>, w: String) {
    if !v.contains(&w) {
        v.push(w);
    }
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
/// strength factor), in tension too where `tension` gives its pull: per
/// case the most loaded bolt's usage and check.
pub fn check_group(g: &Group, strengths: &[f64], tension: Option<&GroupTension>) -> GroupResult {
    let mut out = GroupResult::default();
    let mut worst: Option<f64> = None;
    let (said, unsafe_words) = packing_words(g);
    out.notes.extend(said);
    out.not_safe.extend(unsafe_words);
    for (n, strength) in strengths.iter().enumerate() {
        let mut best: Option<(f64, String)> = None;
        for (k, b) in g.bolts.iter().enumerate() {
            let Some(l) = g.loads.get(n).and_then(|x| x.get(k)) else {
                continue;
            };
            let mut c = bolt_checks(b, &g.plies, l);
            if let Some(gt) = tension
                && let Some(t) = gt.loads.get(n).and_then(|x| x.get(k)).copied()
                && t > 0.0
                && let Some(bt) = gt.bolts.get(k)
            {
                let mut x = tension_checks(b, bt, t, l.force);
                // What is not worked out matters where the pull is 1 %
                // or more of the shear (the core's rule).
                if t < 0.01 * l.force || t < 1.0 {
                    x.not_safe.retain(|w| !w.contains("not worked out"));
                }
                c.checks.extend(x.checks);
                c.notes.extend(x.notes);
                c.not_safe.extend(x.not_safe);
            }
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

    /// Packings (O49 stage 1c): ASCE 10-97 has no rule — a 10 mm packing
    /// under an M16 (above 16/3 = 5.3 mm) said not on the safe side, a 4
    /// mm one only noted; the bolt shear not reduced either way.
    #[test]
    fn a_packing_is_said_not_reduced() {
        let g = |tp: f64| Group {
            plies: vec![leg()],
            bolts: vec![m16(&["G1b"])],
            loads: vec![vec![Load {
                force: 20e3,
                ends: vec![None],
                sides: vec![None],
                ahead: vec![None],
            }]],
            packings: Some(vec![Some(tp)]),
        };
        let thick = check_group(&g(0.010), &[1.0], None);
        let thin = check_group(&g(0.004), &[1.0], None);
        assert!(
            thick.not_safe.iter().any(|w| w.contains("10 mm")),
            "{thick:?}"
        );
        assert!(thin.not_safe.is_empty() && thin.notes.iter().any(|w| w.contains("none found")));
        assert_eq!(thick.usages, thin.usages);
    }

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

    /// A bolt pulled through an angle's flange (invented, by hand): an M16
    /// (F_u 800 MPa, no tension capacity given) pulled by 20 kN through a
    /// 60 × 6 mm leg (S355), the bolt 30 mm from the heel, w 32 mm, hole
    /// 17.5 mm. a = 30, b = 24 mm; a ≤ 1.25 b = 30: a′ = 38, b′ = 16 mm; Q
    /// = 20 × 16 / (2 × 38) = 4.211 kN, B = 24.211 kN. F_t A_s = 0.6 × 800
    /// × 0.6 × π 16²/4 = 57.906 kN: 41.8 %. The flange: δ = 1 − 17.5/32 =
    /// 0.453; (1 + δ) w t² f_y / 4 = 1.453 × 32 × 36 × 355 / 4 = 148.6 N m
    /// against T b′ = 320 N m: 215 %. With a shear of 30 kN (one plane,
    /// 59.84 kN): √(0.418² + 0.501²) = 0.653.
    #[test]
    fn a_bolt_in_tension_with_prying() {
        let b = m16(&["D1"]);
        let bt = BoltTension {
            capacity: None,
            tees: vec![Tee {
                part: "L1".into(),
                thickness: 0.006,
                fy: Some(355e6),
                edge: 0.03,
                web: 0.024,
                width: 0.032,
            }],
            plates: Vec::new(),
            unknown: Vec::new(),
        };
        let c = tension_checks(&b, &bt, 20e3, 30e3);
        let pull = c
            .checks
            .iter()
            .find(|x| x.what.starts_with("bolt tension"))
            .unwrap();
        let q = 20e3 * 0.016 / (2.0 * 0.038);
        assert!((pull.demand - (20e3 + q)).abs() < 1e-6, "{}", pull.demand);
        let rt = 0.6 * 800e6 * 0.6 * std::f64::consts::PI * 0.016f64.powi(2) / 4.0;
        assert!((pull.resistance - rt).abs() < 1e-6);
        assert!(
            (pull.usage(1.0) - 0.418).abs() < 1e-3,
            "{}",
            pull.usage(1.0)
        );
        let flange = c
            .checks
            .iter()
            .find(|x| x.what.starts_with("bending of L1"))
            .unwrap();
        let delta = 1.0 - 0.0175 / 0.032;
        let mp = (1.0 + delta) * 0.032 * 0.006f64.powi(2) * 355e6 / 4.0;
        assert!((flange.demand - 320.0).abs() < 1e-9 && (flange.resistance - mp).abs() < 1e-9);
        assert!(
            (flange.usage(1.0) - 2.15).abs() < 0.01,
            "{}",
            flange.usage(1.0)
        );
        let both = c
            .checks
            .iter()
            .find(|x| x.what.starts_with("bolt shear and tension"))
            .unwrap();
        let rv = 0.62 * 800e6 * 0.6 * std::f64::consts::PI * 0.016f64.powi(2) / 4.0;
        let want = (((20e3 + q) / rt).powi(2) + (30e3 / rv).powi(2)).sqrt();
        assert!((both.demand - want).abs() < 1e-12, "{} {want}", both.demand);
        assert!((want - 0.653).abs() < 1e-3, "{want}");
        assert!(c.not_safe.is_empty(), "{:?}", c.not_safe);
        // A plate whose bending is not known: said.
        let bt = BoltTension {
            unknown: vec!["plate G1".into()],
            ..bt
        };
        let c = tension_checks(&b, &bt, 20e3, 30e3);
        assert!(c.not_safe.iter().any(|w| w.contains("plate G1")));
    }

    /// A plate's prying without a web (2026-11-19; invented, by hand): the
    /// same M16 pulled by 20 kN through a 10 mm plate (S355), the bolt 40
    /// mm from the plate's edge where it bears and 60 mm from the farthest
    /// bolt pulling it, w 50 mm, hole 17.5 mm: a = 40 ≤ 1.25 b = 75, a′ =
    /// 48, b′ = 52 mm; Q = 20 × 52 / 48 = 21.667 kN (twice a flange's with
    /// α = δ = 1), B = 41.667 kN: 72.0 % of 57.906 kN. The plate: δ = 1 −
    /// 17.5/50 = 0.65; δ w t² f_y / 4 = 0.65 × 50 × 100 × 355 / 4 = 288.4
    /// N m against T b′ = 1040 N m: 360.6 %. The plate's prying governs
    /// over a flange's smaller one; no "not worked out" said.
    #[test]
    fn a_plates_prying_without_a_web() {
        let b = m16(&["D1"]);
        let plate = Tee {
            part: "G1".into(),
            thickness: 0.010,
            fy: Some(355e6),
            edge: 0.040,
            web: 0.060,
            width: 0.050,
        };
        let bt = BoltTension {
            capacity: None,
            tees: vec![Tee {
                part: "L1".into(),
                thickness: 0.006,
                fy: Some(355e6),
                edge: 0.03,
                web: 0.024,
                width: 0.032,
            }],
            plates: vec![plate],
            unknown: Vec::new(),
        };
        let c = tension_checks(&b, &bt, 20e3, 0.0);
        let pull = c
            .checks
            .iter()
            .find(|x| x.what.starts_with("bolt tension"))
            .unwrap();
        let q: f64 = 20e3 * 0.052 / 0.048;
        assert!((q - 21_666.67).abs() < 0.01, "{q}");
        assert!((pull.demand - (20e3 + q)).abs() < 1e-6, "{}", pull.demand);
        assert!(pull.what.contains("plate G1 (no web"), "{}", pull.what);
        assert!(
            (pull.usage(1.0) - 0.7196).abs() < 1e-3,
            "{}",
            pull.usage(1.0)
        );
        let bend = c
            .checks
            .iter()
            .find(|x| x.what.starts_with("bending of plate G1"))
            .unwrap();
        let r = 0.65 * 0.050 * 0.010f64.powi(2) * 355e6 / 4.0;
        assert!((bend.demand - 1040.0).abs() < 1e-9 && (bend.resistance - r).abs() < 1e-9);
        assert!(
            (bend.usage(1.0) - 3.606).abs() < 1e-3,
            "{}",
            bend.usage(1.0)
        );
        assert!(c.not_safe.is_empty(), "{:?}", c.not_safe);
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
            packings: None,
        };
        let r = check_group(&g, &[1.0], None);
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
