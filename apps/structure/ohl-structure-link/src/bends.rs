// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **A bent plate's bend in bending** (2026-11-19, the owner's decision of
//! that day; built, in active development, not approved yet): the bend of a
//! plate folded along a line (the joint layout's bent plate, J2b) checked
//! by the structure's code plugin through its additive export
//! `joint-check-bends`; a plugin without it: the bend's own bending not
//! checked, NOT ON THE SAFE SIDE, said on the row as before.
//!
//! What OpenOHL sends per bend and load case — the forces on the plate's
//! section along the bend line (its width the bend's length, its depth the
//! plate's thickness), no code's rule in them, each taken on the safe side:
//!
//! - **across the bend** N = Σ Nᵢ (dᵢ · a): each member on the flat in its
//!   plane pulling (tension positive) along its direction dᵢ from the
//!   joint, a the flat's direction square to the bend line, away from it;
//! - **along the bend** V = |Σ Nᵢ (dᵢ · l)|, l along the bend line;
//! - **about the bend line** M = Σ |Nᵢ (dᵢ · a)| eᵢ + |N| sin θ · d — each
//!   member's force across the bend at its line's distance eᵢ out of the
//!   flat's middle plane (half the plate's thickness and the member's
//!   centroid from the back of its leg bolted; *by size*, the worst of the
//!   signs), plus the force across the bend turned by the bend's angle θ
//!   into the flat on the leg, held there by its bolts at the lever d from
//!   the bend line to the farthest of them (else to the flat's farthest
//!   edge) — the most that flat's cantilever can give, *taken at the bend*;
//! - **in the plate's plane** M_in = |Σ Nᵢ ((cᵢ − o) × dᵢ) · n|, about the
//!   bend's middle o, cᵢ the centroid of member i's bolts, n the flat's
//!   normal.
//!
//! The rows are the joint checks' (`PlatePart::Bend`): a usage per case
//! with the plugin's check and clause, or "NOT CHECKED by (the plugin), NOT ON
//! THE SAFE SIDE: …" without a usage.

use ohl_sections::SectionLibrary;
use ohl_structures_fe::model::Model;

use crate::joints::{JointDetails, PlateDetail};
use crate::plate_checks::{PlateCase, PlatePart, PlateUsage};

type V3 = [f64; 3];

/// A member on a bend's flat, as its forces reach the bend.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OnBend {
    member: String,
    /// Its direction from the joint across the bend (d · a).
    across: f64,
    /// Its direction along the bend line (d · l).
    along: f64,
    /// Its force line's distance out of the flat's middle plane, m.
    lever: f64,
    /// The in-plane lever of its force about the bend's middle, m
    /// (((c − o) × d) · n).
    in_plane: f64,
}

/// One bend, planned once per structure.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BendPlan {
    joint: String,
    /// The two flats, "P1/P2" (the flat in the members' plane first).
    part: String,
    thickness: f64,
    fy: Option<f64>,
    fu: Option<f64>,
    /// The bend's length along its line, m.
    width: f64,
    inner_radius: f64,
    /// The angle the plate turns through, rad.
    angle: f64,
    welded: bool,
    members: Vec<OnBend>,
    /// From the bend line to the farthest bolt holding the other flat on
    /// the leg (else that flat's farthest edge), m.
    hold: f64,
    notes: Vec<String>,
    /// Why it cannot be worked out (none: it can).
    unknown: Option<String>,
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}

fn unit(a: V3) -> Option<V3> {
    let l = norm(a);
    (l > 1e-12).then(|| a.map(|c| c / l))
}

/// Distance of `q` from the line through `o` along unit `l`.
fn off_line(q: V3, o: V3, l: V3) -> f64 {
    let d = sub(q, o);
    let s = dot(d, l);
    norm(sub(d, l.map(|c| c * s)))
}

/// The bends of every joint of `details` (a plate with `[plate.bend]`, the
/// flat in its members' plane, bent from the flat on the leg it names).
pub(crate) fn bend_plans(
    details: &JointDetails,
    model: &Model,
    library: &SectionLibrary,
) -> Vec<BendPlan> {
    let mut out = Vec::new();
    for j in &details.joints {
        for p in j.plates.iter().filter(|p| p.bend.is_some()) {
            out.push(bend_plan(j, p, model, library));
        }
    }
    out
}

fn bend_plan(
    j: &crate::joints::JointDetail,
    p: &PlateDetail,
    model: &Model,
    library: &SectionLibrary,
) -> BendPlan {
    let b = p.bend.as_ref().expect("a bent flat");
    let other = j.plates.iter().find(|q| q.id == b.to);
    let (steel, steel_note) = crate::plate_checks::plate_steel_of(p, j, model, library);
    let mut plan = BendPlan {
        joint: j.id.clone(),
        part: format!("{}/{}", p.id, b.to),
        thickness: steel.thickness,
        fy: steel.fy,
        fu: steel.fu,
        width: 0.0,
        inner_radius: b.inner_radius,
        angle: b.angle.to_radians(),
        welded: !p.welds.is_empty()
            || !p.welded_to.is_empty()
            || other.is_some_and(|q| !q.welds.is_empty() || !q.welded_to.is_empty()),
        members: Vec::new(),
        hold: 0.0,
        notes: steel_note.into_iter().collect(),
        unknown: None,
    };
    let Some([o0, o1]) = b.line else {
        plan.unknown = Some(format!(
            "the bend line of plate {} is not given: its section not known",
            p.id
        ));
        return plan;
    };
    let Some(l) = unit(sub(o1, o0)) else {
        plan.unknown = Some(format!("plate {}'s bend line has no length", p.id));
        return plan;
    };
    plan.width = norm(sub(o1, o0));
    let o: V3 = std::array::from_fn(|k| 0.5 * (o0[k] + o1[k]));
    let on: Vec<std::borrow::Cow<crate::joints::Arrival>> =
        crate::joints::bolted_onto(&j.arrivals, &p.id)
            .into_iter()
            .filter(|a| !a.modelling)
            .collect();
    let normal = crate::welds::polygon_normal(&p.outline).or_else(|| {
        on.iter()
            .find(|a| !a.bolt_points.is_empty())
            .map(|a| a.bolt_axis)
    });
    let Some(n) = normal.and_then(unit) else {
        plan.unknown = Some(format!("plate {}'s plane is not known", p.id));
        return plan;
    };
    let Some(mut a) = unit(cross(l, n)) else {
        plan.unknown = Some(format!("plate {}'s bend line lies across its plane", p.id));
        return plan;
    };
    // a: away from the bend, towards the members' bolts (else the flat's
    // outline).
    let pts: Vec<V3> = on
        .iter()
        .flat_map(|x| x.bolt_points.iter().copied())
        .chain(p.outline.iter().copied())
        .collect();
    let toward: f64 = pts.iter().map(|q| dot(sub(*q, o), a)).sum();
    if toward < 0.0 {
        a = a.map(|c| -c);
    }
    for x in &on {
        let c = if x.bolt_points.is_empty() {
            plan.notes.push(format!(
                "{}: its bolts on plate {} not placed — its force taken through the bend's \
                 middle in the plate's plane",
                x.member, p.id
            ));
            o
        } else {
            let k = x.bolt_points.len() as f64;
            std::array::from_fn(|i| x.bolt_points.iter().map(|q| q[i]).sum::<f64>() / k)
        };
        // Its force line out of the flat's middle plane: half the plate
        // and its centroid from the back of its leg bolted (an angle);
        // else its steel's farthest point from its axis (on the safe side).
        let own = x.profile.as_ref().and_then(|pr| match pr.angle {
            Some(g) => Some(if x.leg_y { g.heel[1] } else { g.heel[0] }.abs()),
            None => pr
                .outline
                .iter()
                .map(|q| q[0].abs().max(q[1].abs()))
                .reduce(f64::max),
        });
        let lever = 0.5 * plan.thickness
            + own.unwrap_or_else(|| {
                plan.notes.push(format!(
                    "{}: its section's shape not known — its force line taken in the plate's \
                     middle plane out of it, NOT ON THE SAFE SIDE",
                    x.member
                ));
                0.0
            });
        plan.members.push(OnBend {
            member: x.member.clone(),
            across: dot(x.direction, a),
            along: dot(x.direction, l),
            lever,
            in_plane: dot(cross(sub(c, o), x.direction), n),
        });
    }
    // The other flat's hold: its bolts onto the leg, else its outline.
    let held: Vec<V3> = other
        .and_then(|q| q.leg_bolts.as_deref())
        .map(|lb| lb.bolt_points.clone())
        .unwrap_or_default();
    let hold = held
        .iter()
        .map(|q| off_line(*q, o, l))
        .reduce(f64::max)
        .or_else(|| {
            let far = other?
                .outline
                .iter()
                .map(|q| off_line(*q, o, l))
                .reduce(f64::max)?;
            plan.notes.push(format!(
                "plate {}'s own bolts onto the leg not known: the lever of the force across the \
                 bend to its farthest edge (on the safe side)",
                b.to
            ));
            Some(far)
        });
    match hold {
        Some(h) => plan.hold = h,
        None => {
            plan.unknown = Some(format!(
                "how plate {} holds the bend (its bolts onto the leg, its outline) is not known",
                b.to
            ));
        }
    }
    if plan.members.is_empty() {
        plan.unknown = Some(format!("no member bolted onto plate {}", p.id));
    }
    plan
}

/// What acts on `plan`'s section in a case of members' axial forces
/// `axial` (N, tension positive): (across, along, about the bend line, in
/// the plate's plane), N and N m ([module](self)).
pub(crate) fn bend_load(
    plan: &BendPlan,
    axial: &std::collections::BTreeMap<String, f64>,
) -> [f64; 4] {
    let (mut n, mut v, mut m, mut m_in) = (0.0, 0.0, 0.0, 0.0);
    for x in &plan.members {
        let f = axial.get(&x.member).copied().unwrap_or(0.0);
        n += f * x.across;
        v += f * x.along;
        m += (f * x.across).abs() * x.lever;
        m_in += f * x.in_plane;
    }
    m += n.abs() * plan.angle.sin().abs() * plan.hold;
    [n, v.abs(), m, m_in.abs()]
}

fn row(plan: &BendPlan, axial: &std::collections::BTreeMap<String, f64>) -> PlateUsage {
    PlateUsage {
        joint: plan.joint.clone(),
        plate: plan.part.clone(),
        part: PlatePart::Bend,
        force: bend_load(plan, axial)[0],
        usage: None,
        governs: String::new(),
        checks: Vec::new(),
        notes: plan.notes.clone(),
        does_not_fit: None,
        misfit_assumed: false,
    }
}

/// The rows of the bends of `plans` in every case of `cases` (one list per
/// case): checked by `plugin` through its `joint-check-bends` export, one
/// call with every bend and case; without a plugin (a method that is not a
/// design code's), without the export, or for a bend that cannot be worked
/// out: not checked, NOT ON THE SAFE SIDE, said, without a usage.
pub(crate) fn bend_rows(
    plans: &[BendPlan],
    structure: &str,
    plugin: Option<&ohl_structures_fe::model::PluginMethod>,
    cases: &[PlateCase],
) -> Vec<Vec<PlateUsage>> {
    use by_plugin::{bends as w, groups as g};
    use ohl_member_check::joints as by_plugin;
    if plans.is_empty() {
        return vec![Vec::new(); cases.len()];
    }
    let Some(p) = plugin else {
        return cases
            .iter()
            .map(|c| {
                plans
                    .iter()
                    .map(|b| {
                        let mut r = row(b, c.2);
                        r.governs = "NOT CHECKED, NOT ON THE SAFE SIDE: the bend's own bending \
                                     (no design code's check)"
                            .into();
                        r
                    })
                    .collect()
            })
            .collect();
    };
    let not_checked = by_plugin::not_checked_by(p);
    let sent: Vec<w::Bend> = plans
        .iter()
        .enumerate()
        .filter(|(_, b)| b.unknown.is_none())
        .map(|(k, b)| w::Bend {
            id: k as u32,
            joint: b.joint.clone(),
            part: b.part.clone(),
            thickness: b.thickness,
            fy: b.fy,
            fu: b.fu,
            width: b.width,
            inner_radius: b.inner_radius,
            angle: b.angle,
            welded: b.welded,
            loads: cases
                .iter()
                .map(|c| {
                    let [axial, shear, moment, moment_in_plane] = bend_load(b, c.2);
                    w::BendLoad {
                        axial,
                        shear,
                        moment,
                        moment_in_plane,
                    }
                })
                .collect(),
            notes: b.notes.clone(),
        })
        .collect();
    let checked = if sent.is_empty() {
        by_plugin::GroupsChecked::Results(Vec::new())
    } else {
        by_plugin::check_bends(
            p,
            &w::BendInput {
                structure: structure.to_owned(),
                parameters: String::new(),
                cases: cases
                    .iter()
                    .map(|c| g::LoadCase {
                        name: c.0.to_owned(),
                        strength: c.1,
                    })
                    .collect(),
                bends: sent,
            },
        )
    };
    cases
        .iter()
        .enumerate()
        .map(|(n, c)| {
            plans
                .iter()
                .enumerate()
                .map(|(k, b)| {
                    let mut r = row(b, c.2);
                    if let Some(why) = &b.unknown {
                        r.governs = format!(
                            "NOT CHECKED, NOT ON THE SAFE SIDE: the bend's own bending — {why}"
                        );
                        return r;
                    }
                    let results = match &checked {
                        by_plugin::GroupsChecked::NotChecked(why) => {
                            r.governs = format!("{not_checked}: the bend's own bending — {why}");
                            return r;
                        }
                        by_plugin::GroupsChecked::Results(x) => x,
                    };
                    let Some(x) = results.iter().find(|x| x.id as usize == k) else {
                        r.governs =
                            format!("{not_checked}: the plugin returned nothing for this bend");
                        return r;
                    };
                    r.notes.extend(x.notes.iter().cloned());
                    let Some(u) = x.usages.iter().find(|u| u.load_case as usize == n) else {
                        r.governs = format!(
                            "{not_checked}: the plugin returned no usage for this load case"
                        );
                        return r;
                    };
                    r.usage = Some(u.usage);
                    r.governs = if u.check.is_empty() {
                        x.governs.clone()
                    } else {
                        u.check.clone()
                    };
                    r.checks = vec![(r.governs.clone(), u.usage)];
                    if let Some(ws) = &x.not_safe {
                        r.governs
                            .push_str(&format!(" {} {ws}]", ohl_structures_fe::check::NOT_SAFE));
                    }
                    r
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> BendPlan {
        BendPlan {
            joint: "N2".into(),
            part: "P2/P1".into(),
            thickness: 0.008,
            fy: Some(355e6),
            fu: Some(510e6),
            width: 0.2,
            inner_radius: 0.016,
            angle: std::f64::consts::FRAC_PI_2,
            welded: false,
            members: vec![OnBend {
                member: "D1".into(),
                across: 0.8,
                along: 0.6,
                lever: 0.02,
                in_plane: 0.05,
            }],
            hold: 0.04,
            notes: Vec::new(),
            unknown: None,
        }
    }

    /// By hand: D1 pulls 10 kN at 0.8 across the bend and 0.6 along it —
    /// across 8 kN, along 6 kN; about the bend line 8 kN × 20 mm (its line
    /// out of the flat) + 8 kN × sin 90° × 40 mm (the hold's lever) =
    /// 0.48 kN m; in the plate's plane 10 kN × 50 mm = 0.5 kN m. In
    /// compression the same sizes, the force across negative.
    #[test]
    fn the_forces_on_the_bend_by_hand() {
        let p = plan();
        let mut f = std::collections::BTreeMap::new();
        f.insert("D1".to_owned(), 10e3);
        let [n, v, m, m_in] = bend_load(&p, &f);
        assert!((n - 8e3).abs() < 1e-9 && (v - 6e3).abs() < 1e-9);
        assert!((m - (8e3 * 0.02 + 8e3 * 0.04)).abs() < 1e-9, "{m}");
        assert!((m_in - 500.0).abs() < 1e-9, "{m_in}");
        f.insert("D1".to_owned(), -10e3);
        let [n2, v2, m2, m_in2] = bend_load(&p, &f);
        assert!((n2 + 8e3).abs() < 1e-9);
        assert_eq!((v2, m2, m_in2), (v, m, m_in));
    }

    /// Without a design code's plugin, and a bend that cannot be worked
    /// out: not checked, NOT ON THE SAFE SIDE, without a usage.
    #[test]
    fn without_a_plugin_the_bend_is_said_not_checked() {
        let f = std::collections::BTreeMap::new();
        let rows = bend_rows(&[plan()], "S", None, &[("one", 1.0, &f)]);
        assert_eq!(rows.len(), 1);
        let r = &rows[0][0];
        assert!(r.usage.is_none());
        assert!(r.governs.contains("NOT ON THE SAFE SIDE"), "{}", r.governs);
        assert_eq!(r.what(), "plate P2/P1, its bend");
    }
}
