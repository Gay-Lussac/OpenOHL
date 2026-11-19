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
//!
//! **The bolts in tension** (2026-11-19, `joint-check-groups-tension`):
//! OpenOHL sends each bolt's pull per case and the flanges it bends; here
//! the prying force (6.2.2.4 asks for the added tension of prying, Figure
//! 26, without a method: the T-stub model of Struik and de Back,
//! [`prying`], on the safe side), the bolt's tension (Table 16, (77)), its
//! shear and tension together (6.2.2.6, (78)) and each flange's bending
//! (the model's mechanism, f_y / γ_M1).

use crate::joints::{Checked, PlateCheck};
use crate::params::{gamma_m1, gamma_m2};
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
    /// Its library's tension capacity (its ultimate tensile load f_ub
    /// A_s), N.
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
/// Eq. 17.12 with α = 1: T b′ ≤ (1 + δ) w t² f_y / (4 γ_M1), δ = 1 −
/// d₀/w) and what is not covered. Returns (Q, the flange that gives it,
/// the flanges' checks, not safe).
pub fn prying(
    t: f64,
    d: f64,
    d0: f64,
    tees: &[Tee],
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
                 Kulak et al. Eq. 17.12, α = 1: T b′ ≤ (1 + δ) w t² f_y / (4 γ_M1 {}); b′ {:.0} \
                 mm, w {:.0} mm, δ {:.2})",
                x.part,
                t / 1000.0,
                gamma_m1(),
                mm(b1),
                mm(x.width),
                delta
            ),
            demand: t * b1,
            resistance: (1.0 + delta) * x.width * x.thickness.powi(2) * fy / 4.0 / gamma_m1(),
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
/// δ w t² f_y / 4 γ_M1 (δ = 1 − d₀/w). Returns as [`prying`].
pub fn plate_prying(
    t: f64,
    d: f64,
    d0: f64,
    plates: &[Tee],
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
                 t² f_y / (4 γ_M1 {}); b′ {:.0} mm, w {:.0} mm, δ {:.2})",
                x.part,
                t / 1000.0,
                gamma_m1(),
                mm(b1),
                mm(x.width),
                delta
            ),
            demand: t * b1,
            resistance: delta * x.width * x.thickness.powi(2) * fy / 4.0 / gamma_m1(),
            distance: false,
        });
    }
    (q, by, checks, not_safe)
}

/// The tension checks of bolt `b` pulled by `t` (N) with its shear force
/// `v` (N) by SIA 263: the prying force from its flanges ([`prying`];
/// 6.2.2.4), its tension (Table 16, (77)) and its shear and tension
/// together (6.2.2.6, (78)).
pub fn tension_checks(b: &Bolt, bt: &BoltTension, t: f64, v: f64) -> Checked {
    let mut out = Checked::default();
    let d = b.diameter;
    let d0 = b
        .hole
        .or_else(|| sia263::standard_hole(d))
        .unwrap_or(d + 0.003);
    let (qf, byf, flanges, said) = prying(t, d, d0, &bt.tees);
    out.checks.extend(flanges);
    out.not_safe.extend(said);
    // The plates' prying (2026-11-19): the largest of all governs.
    let (qp, byp, plates, said) = plate_prying(t, d, d0, &bt.plates);
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
    // (77): F_t,Rd = 0.9 f_ub A_s / γ_M2.
    let (rt, words) = match (bt.capacity, b.ultimate) {
        (Some(c), _) => (
            0.9 * c / gamma_m2(),
            format!(
                "0.9 × the bolt's given tension capacity (its f_ub A_s) / γ_M2 {}",
                gamma_m2()
            ),
        ),
        (None, Some(fub)) => (
            0.9 * fub * 0.6 * gross / gamma_m2(),
            format!(
                "0.9 f_ub A_s / γ_M2 {}, A_s taken as 0.6 of the gross area (on the safe side)",
                gamma_m2()
            ),
        ),
        (None, None) => {
            out.not_safe.push(
                "bolt tension not checked (no tension capacity nor ultimate strength of the bolt)"
                    .into(),
            );
            return out;
        }
    };
    note(
        &mut out.notes,
        "bolts in tension: SIA 263 6.2.2.3 (loads not essentially static: preloaded high-strength \
         bolts) and 6.2.1.4 (countersunk bolts, thin nuts: 25 % less) not checked — the bolts' \
         kind and the loads' character are the engineer's"
            .into(),
    );
    let prying_words = match &by {
        Some(w) => format!(", prying {:.1} kN ({w})", q / 1000.0),
        None => String::new(),
    };
    out.checks.push(PlateCheck {
        what: format!(
            "bolt tension: pull {:.1} kN{prying_words}, {:.1} kN together (SIA 263 Table 16, (77): \
             {words})",
            t / 1000.0,
            pull / 1000.0
        ),
        demand: pull,
        resistance: rt,
        distance: false,
    });
    let grade = crate::model::Grade::parse(&b.grade);
    if let Some((rv, _)) = sia263::bolt_shear(
        d,
        b.planes.max(1),
        b.shear_capacity,
        b.shank,
        b.threads_in,
        b.ultimate,
        grade,
    ) && rv > 0.0
        && v > 0.0
    {
        let x = ((pull / rt).powi(2) + (v / rv).powi(2)).sqrt();
        out.checks.push(PlateCheck {
            what: format!(
                "bolt shear and tension together {:.1} and {:.1} kN (SIA 263 6.2.2.6, (78): \
                 (F_v,Ed/F_v,Rd)² + (F_t,Ed/F_t,Rd)² ≤ 1, its square root shown)",
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
/// strength factor), in tension too where `tension` gives its pull: per
/// case the most loaded bolt's usage and check.
pub fn check_group(g: &Group, strengths: &[f64], tension: Option<&GroupTension>) -> GroupResult {
    let mut out = GroupResult::default();
    let mut worst: Option<f64> = None;
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

    /// A plate's prying without a web (2026-11-19; invented, by hand, as the
    /// ASCE 10-97 plugin's test): an M16 8.8 pulled by 20 kN through a 10 mm
    /// plate (S355), a = 40, b = 60 mm, w 50 mm: a′ = 48, b′ = 52 mm, Q = 20
    /// × 52 / 48 = 21.667 kN, B = 41.667 kN against (77)'s 69.49 kN. The
    /// plate with d₀ 18 mm (Table 17): δ = 1 − 18/50 = 0.64, δ w t² f_y /
    /// (4 γ_M1) against T b′ = 1040 N m.
    #[test]
    fn a_plates_prying_without_a_web() {
        let bt = BoltTension {
            capacity: None,
            tees: Vec::new(),
            plates: vec![Tee {
                part: "G1".into(),
                thickness: 0.010,
                fy: Some(355e6),
                edge: 0.040,
                web: 0.060,
                width: 0.050,
            }],
            unknown: Vec::new(),
        };
        let c = tension_checks(&m16(), &bt, 20e3, 0.0);
        let find = |w: &str| c.checks.iter().find(|x| x.what.starts_with(w)).unwrap();
        let q = 20e3 * 0.052 / 0.048;
        let area = std::f64::consts::PI * 0.016f64.powi(2) / 4.0;
        let rt = 0.9 * 800e6 * 0.6 * area / 1.25;
        let pull = find("bolt tension");
        assert!((pull.demand - (20e3 + q)).abs() < 1e-6 && (pull.resistance - rt).abs() < 1e-6);
        let bend = find("bending of plate G1");
        let r = 0.64 * 0.050 * 0.010f64.powi(2) * 355e6 / 4.0 / gamma_m1();
        assert!((bend.resistance - r).abs() < 1e-9 && (bend.demand - 1040.0).abs() < 1e-9);
        assert!(c.not_safe.is_empty(), "{:?}", c.not_safe);
    }

    /// A bolt pulled through an angle's flange (invented, by hand; as the
    /// ASCE 10-97 plugin's test): an M16 8.8 pulled by 20 kN through a 60 ×
    /// 6 mm leg (S355), a = 30, b = 24 mm, w 32 mm: Q = 20 × 16 / (2 × 38)
    /// = 4.211 kN, together 24.211 kN. (77): 0.9 × 800 × 0.6 × 201.06 /
    /// 1.25 = 69.49 kN: 34.8 %. The flange with d_0 18 mm (Table 17): δ =
    /// 1 − 18/32 = 0.4375, (1 + δ) w t² f_y / (4 γ_M1) against T b′ = 320
    /// N m. (78) with a shear of 30 kN (46.32 kN): √(0.348² + 0.648²).
    #[test]
    fn a_bolt_in_tension_with_prying() {
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
        let c = tension_checks(&m16(), &bt, 20e3, 30e3);
        let find = |w: &str| c.checks.iter().find(|x| x.what.starts_with(w)).unwrap();
        let q = 20e3 * 0.016 / (2.0 * 0.038);
        let area = std::f64::consts::PI * 0.016f64.powi(2) / 4.0;
        let rt = 0.9 * 800e6 * 0.6 * area / 1.25;
        let pull = find("bolt tension");
        assert!((pull.demand - (20e3 + q)).abs() < 1e-6 && (pull.resistance - rt).abs() < 1e-6);
        assert!(pull.what.contains("(77)"), "{}", pull.what);
        let flange = find("bending of L1");
        let mp =
            (1.0 + (1.0 - 0.018 / 0.032)) * 0.032 * 0.006f64.powi(2) * 355e6 / 4.0 / gamma_m1();
        assert!((flange.resistance - mp).abs() < 1e-9 && (flange.demand - 320.0).abs() < 1e-9);
        let rv = 0.6 * 800e6 * 0.6 * area / 1.25;
        let both = find("bolt shear and tension");
        let want = (((20e3 + q) / rt).powi(2) + (30e3 / rv).powi(2)).sqrt();
        assert!((both.demand - want).abs() < 1e-12, "{}", both.demand);
        assert!(both.what.contains("(78)"));
        // Checked with the group: tension governs where it is larger.
        let g = Group {
            plies: vec![leg(true)],
            bolts: vec![m16()],
            loads: vec![vec![Load {
                force: 1e3,
                ends: vec![None],
                sides: vec![None],
                ahead: vec![None],
            }]],
        };
        let gt = GroupTension {
            bolts: vec![bt],
            loads: vec![vec![60e3]],
        };
        let r = check_group(&g, &[1.0], Some(&gt));
        let (u, w) = r.usages[0].clone().unwrap();
        assert!(u > 1.0 && w.contains("bending of L1"), "{u} {w}");
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
        let r = check_group(&g, &[1.0], None);
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
