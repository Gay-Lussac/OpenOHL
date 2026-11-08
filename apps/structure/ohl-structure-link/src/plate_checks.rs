// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The plates' and welds' checks per load case (owner 2026-11-04: "add
//! weld size and length and the plate checks"). Built, in active
//! development, not approved yet.
//!
//! [`plate_plans`] works out, once per structure, what each check needs
//! from the joint details ([`crate::joint_details`]): per member bolting
//! onto a plate its bolt group in the plate (the bolts' gauge lines,
//! pitch, the distances to the plate's edge along and across the member,
//! the width the net section crosses) and the plate's steel; per weld
//! between two plates its size, length, the angle between the plates and
//! the members whose forces it carries. [`plate_usage`] then takes a
//! case's member forces and checks each by the structure's capacity
//! method ([`ohl_structures_fe::plates`]): ASCE 10-97, SIA 263:2013, or
//! the simple check (not checked, said).
//!
//! The force a weld carries is taken **on the safe side** as the resultant
//! of the member forces on the plates it holds: the welded plate split at
//! the weld, the larger of the two sides' resultants (each member's axial
//! force along its direction from the joint; a welded plate whose welds
//! close a ring: the sum of every member's force on it, more on the safe
//! side). The moment of that force about the seam is not taken (not on
//! the safe side, said).
//!
//! Only a member's axial force goes into the plate checks (its end shear
//! and moments, in a frame member, are not taken — said for frames).

use std::collections::BTreeMap;

use ohl_core::geometry::cross;
use ohl_sections::SectionLibrary;
use ohl_structures::bolts::BoltLibrary;
use ohl_structures::{LibraryItem, StructureType};
use ohl_structures_fe::model::{CapacityMethod, Model};
use ohl_structures_fe::plates::{self, BoltGroup, PlateSteel, WeldInput};

use crate::joints::{Arrival, JointDetail, PlateDetail};

/// What one check of a plate is about.
#[derive(Debug, Clone, PartialEq)]
pub enum PlatePart {
    /// A member's bolts in the plate.
    Member(String),
    /// The weld to another plate.
    Weld(String),
}

impl PlatePart {
    /// In words: "D1's bolts", "weld to G2".
    pub fn words(&self) -> String {
        match self {
            PlatePart::Member(m) => format!("{m}'s bolts"),
            PlatePart::Weld(o) => format!("weld to {o}"),
        }
    }
}

/// One plate check of one case.
#[derive(Debug, Clone, PartialEq)]
pub struct PlateUsage {
    /// The joint.
    pub joint: String,
    /// The plate.
    pub plate: String,
    /// What.
    pub part: PlatePart,
    /// The force, N (a member's axial force, tension positive; a weld's:
    /// the resultant it carries).
    pub force: f64,
    /// The largest share used (1 = 100 %); none: nothing checked.
    pub usage: Option<f64>,
    /// What governs, with its clause.
    pub governs: String,
    /// Every check and its usage.
    pub checks: Vec<(String, f64)>,
    /// What was assumed or not checked.
    pub notes: Vec<String>,
}

/// A part's check as planned (the force comes per case).
#[derive(Debug, Clone, PartialEq)]
enum Plan {
    Bolted {
        member: String,
        group: BoltGroup,
        steel: PlateSteel,
        /// The share of the member's force this plate takes (1; a
        /// splice's cover: its share by area).
        share: f64,
    },
    Weld {
        with: String,
        input: WeldInput,
        /// The members (and their direction from the joint) on each side
        /// of the weld; one list (`ring`) when the welds close a ring.
        sides: [Vec<(String, [f64; 3])>; 2],
        ring: bool,
    },
    /// Two plates welded to each other that do not meet: no seam, not
    /// checked.
    NoSeam { with: String },
    /// A member's bolts not placed in a cover: not checked (said).
    Unplaced { member: String },
}

/// One planned part.
#[derive(Debug, Clone, PartialEq)]
struct Planned {
    joint: String,
    plate: String,
    plan: Plan,
    notes: Vec<String>,
}

/// What the plate checks need from a structure, worked out once
/// ([`plate_plans`]).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlatePlans {
    /// The capacity method.
    pub method: CapacityMethod,
    parts: Vec<Planned>,
    /// What applies to every case.
    pub notes: Vec<String>,
}

impl PlatePlans {
    /// No plate to check.
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}

impl PlatePlans {
    /// The shares of `member`'s force its bolted plans take (tests).
    #[cfg(test)]
    pub(crate) fn bolted_shares(&self, member: &str) -> Vec<f64> {
        self.parts
            .iter()
            .filter_map(|p| match &p.plan {
                Plan::Bolted {
                    member: m, share, ..
                } if m == member => Some(*share),
                _ => None,
            })
            .collect()
    }
}

/// The plates' plans of the structure file `text` (its sections in
/// `library`, its bolts in `bolts`); empty without plates or below level
/// 3.
pub fn plate_plans(
    text: &str,
    library: &SectionLibrary,
    bolts: &BoltLibrary,
) -> Result<PlatePlans, String> {
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    if t.plates.is_empty() && t.splices.is_empty() {
        return Ok(PlatePlans::default());
    }
    let details = crate::joint_details(text, library, bolts)?;
    let Ok(model) = Model::from_structure_type_with_bolts(text, library, bolts, None) else {
        return Ok(PlatePlans::default());
    };
    let mut out = PlatePlans {
        method: model.analysis.capacity,
        ..PlatePlans::default()
    };
    if out.method == CapacityMethod::Simple {
        out.notes.push(plates::SIMPLE_NOT_CHECKED.into());
    }
    for j in &details.joints {
        cover_plans(j, &model, library, &mut out.parts);
        for p in &j.plates {
            let (steel, steel_note) = plate_steel(p, j, &model, library);
            for a in crate::joints::bolted_onto(&j.arrivals, &p.id)
                .iter()
                .filter(|a| !a.modelling)
            {
                let a = a.as_ref();
                let mut notes = Vec::new();
                notes.extend(steel_note.clone());
                // Both legs bolted onto two plates (`other_onto`,
                // 2026-11-08): each plate the member's force shared by
                // its bolts on that leg.
                let whole = j
                    .arrivals
                    .iter()
                    .find(|x| x.member == a.member && x.end == a.end);
                let share = match whole {
                    Some(w) if w.other_onto.is_some() => {
                        let all = (w.bolt_points.len() + w.other_points.len()).max(1);
                        let here = a.bolt_points.len() as f64 / all as f64;
                        notes.push(format!(
                            "both legs bolted, onto {} and {}: this plate takes {:.0} % of its \
                             force (its bolts here of all its bolts: an assumption, each bolt \
                             of the end carrying the same)",
                            w.onto.value.as_deref().unwrap_or("?"),
                            w.other_onto.as_deref().unwrap_or("?"),
                            here * 100.0
                        ));
                        here
                    }
                    _ => 1.0,
                };
                let Some(group) = bolt_group(a, &p.id, &p.outline, j, &model, steel, &mut notes)
                else {
                    continue;
                };
                if model
                    .members
                    .iter()
                    .find(|m| m.id == a.member)
                    .is_some_and(|m| m.is_frame())
                {
                    notes.push(
                        "a frame member: its axial force only (its end shear and moments not in \
                         the plate checks)"
                            .into(),
                    );
                }
                out.parts.push(Planned {
                    joint: j.id.clone(),
                    plate: p.id.clone(),
                    plan: Plan::Bolted {
                        member: a.member.clone(),
                        group,
                        steel,
                        share,
                    },
                    notes,
                });
            }
            // Welded to a plate after it that it does not meet: said.
            let at = j.plates.iter().position(|x| x.id == p.id).unwrap_or(0);
            for o in &p.welded_to {
                let later = j
                    .plates
                    .iter()
                    .position(|x| &x.id == o)
                    .is_some_and(|k| k > at);
                if later && !p.welds.iter().any(|w| &w.with == o) {
                    out.parts.push(Planned {
                        joint: j.id.clone(),
                        plate: p.id.clone(),
                        plan: Plan::NoSeam { with: o.clone() },
                        notes: vec![format!(
                            "plates {} and {o} do not meet (parallel, or apart): no seam, the \
                             weld not checked",
                            p.id
                        )],
                    });
                }
            }
            for w in &p.welds {
                let Some(q) = j.plates.iter().find(|q| q.id == w.with) else {
                    continue;
                };
                let (sq, _) = plate_steel(q, j, &model, library);
                let fy = match (steel.fy, sq.fy) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    _ => None,
                };
                let (sides, ring) = weld_sides(j, &p.id, &q.id);
                let mut notes = Vec::new();
                notes.push(if ring {
                    "the welds close a ring: the weld taken carrying every member's force on \
                     the welded plate (on the safe side)"
                        .to_owned()
                } else {
                    "the weld's force: the larger resultant of the member forces on either side \
                     of it (on the safe side)"
                        .to_owned()
                });
                out.parts.push(Planned {
                    joint: j.id.clone(),
                    plate: p.id.clone(),
                    plan: Plan::Weld {
                        with: q.id.clone(),
                        input: WeldInput {
                            kind: w.kind,
                            size: w.size,
                            length: w.length(),
                            length_given: w.length.is_some(),
                            angle: w.angle(),
                            fy,
                            thickness: [p.thickness, q.thickness],
                        },
                        sides,
                        ring,
                    },
                    notes,
                });
            }
        }
    }
    Ok(out)
}

/// The checks of a splice's covers at joint `j`: per member end bolting
/// onto the splice, each cover on the leg it is bolted by checked as a
/// plate is, for its share of the member's force by area — its thickness
/// times its width across the member, over the covers' together
/// (2026-11-05; an assumption, said: the covers taken as straining alike).
fn cover_plans(j: &JointDetail, model: &Model, library: &SectionLibrary, out: &mut Vec<Planned>) {
    for sp in &j.splices {
        for a in j
            .arrivals
            .iter()
            .filter(|a| a.onto.value.as_deref() == Some(sp.id.as_str()) && !a.modelling)
        {
            let leg = crate::joints::bolted_cover_leg(a);
            let on: Vec<&crate::joints::CoverDetail> = sp
                .covers
                .iter()
                .filter(|c| c.leg.is_none() || leg.is_none() || c.leg == leg)
                .collect();
            let mut planned = Vec::new();
            for c in &on {
                let name = format!("{} ({})", sp.id, c.words());
                let (steel, steel_note) = material_steel(
                    c.material.as_deref(),
                    c.thickness,
                    j,
                    &sp.id,
                    &name,
                    model,
                    library,
                );
                let mut notes: Vec<String> = steel_note.into_iter().collect();
                let Some(group) = bolt_group(a, &name, &c.outline, j, model, steel, &mut notes)
                else {
                    out.push(Planned {
                        joint: j.id.clone(),
                        plate: name,
                        plan: Plan::Unplaced {
                            member: a.member.clone(),
                        },
                        notes,
                    });
                    continue;
                };
                // Its width across the member (the outline's extent).
                let (_, across) = crate::joints::keep_axes(a);
                let x: Vec<f64> = c.outline.iter().map(|q| dot(*q, across)).collect();
                let width = x.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                    - x.iter().copied().fold(f64::INFINITY, f64::min);
                planned.push((name, group, steel, notes, width * c.thickness));
            }
            let total: f64 = planned.iter().map(|x| x.4).sum();
            for (name, group, steel, mut notes, area) in planned {
                let share = if total > 0.0 { area / total } else { 1.0 };
                if on.len() > 1 {
                    notes.push(format!(
                        "{}: {:.0} % of {}'s force — shared between the covers by area (thickness \
                         × width across the member; an assumption)",
                        name,
                        share * 100.0,
                        a.member
                    ));
                }
                out.push(Planned {
                    joint: j.id.clone(),
                    plate: name,
                    plan: Plan::Bolted {
                        member: a.member.clone(),
                        group,
                        steel,
                        share,
                    },
                    notes,
                });
            }
        }
    }
}

/// A plate's or cover's steel: its material's, else (assumed) the
/// weakest of the members bolting onto `onto`, said of `label`.
fn material_steel(
    material: Option<&str>,
    thickness: f64,
    j: &JointDetail,
    onto: &str,
    label: &str,
    model: &Model,
    library: &SectionLibrary,
) -> (PlateSteel, Option<String>) {
    if let Some(m) = material.and_then(|id| library.material(id)) {
        return (
            PlateSteel {
                thickness,
                fy: m.yield_strength,
                fu: m.ultimate,
            },
            None,
        );
    }
    let mut fy: Option<f64> = None;
    let mut fu: Option<f64> = None;
    for a in j
        .arrivals
        .iter()
        .filter(|a| a.onto.value.as_deref() == Some(onto))
    {
        if let Some(m) = model.members.iter().find(|m| m.id == a.member) {
            let s = &model.sections[m.section];
            if let Some(v) = s.yield_strength {
                fy = Some(fy.map_or(v, |x| x.min(v)));
            }
            if let Some(v) = s.ultimate {
                fu = Some(fu.map_or(v, |x| x.min(v)));
            }
        }
    }
    let note = match material {
        Some(m) => format!(
            "{label}: material {m} not in the library — the weakest steel of the members onto \
             it (assumed)"
        ),
        None => format!(
            "{label}: no material given — the weakest steel of the members onto it (assumed)"
        ),
    };
    (PlateSteel { thickness, fy, fu }, Some(note))
}

/// A plate's thickness and steel: its material's, else (assumed) the
/// weakest of the members bolted onto it, said.
fn plate_steel(
    p: &PlateDetail,
    j: &JointDetail,
    model: &Model,
    library: &SectionLibrary,
) -> (PlateSteel, Option<String>) {
    let label = format!("plate {}", p.id);
    material_steel(
        p.material.as_deref(),
        p.thickness,
        j,
        &p.id,
        &label,
        model,
        library,
    )
}

/// A member's bolt group in plate `p`, from its bolts where they are.
fn bolt_group(
    a: &Arrival,
    id: &str,
    outline: &[[f64; 3]],
    j: &JointDetail,
    model: &Model,
    steel: PlateSteel,
    notes: &mut Vec<String>,
) -> Option<BoltGroup> {
    let set = a.bolts.value.as_ref()?;
    if a.bolt_points.is_empty() || outline.len() < 3 {
        notes.push(format!(
            "{}: its bolts not placed in the plate: not checked",
            a.member
        ));
        return None;
    }
    let idx = model.members.iter().position(|m| m.id == a.member);
    let end_bolts = idx.and_then(|k| plates::end_bolts(model, k, a.end));
    let (along, across) = crate::joints::keep_axes(a);
    let rel = |q: &[f64; 3]| {
        [
            q[0] - j.position[0],
            q[1] - j.position[1],
            q[2] - j.position[2],
        ]
    };
    let pts: Vec<[f64; 2]> = a
        .bolt_points
        .iter()
        .map(|q| {
            let r = rel(q);
            [dot(r, along), dot(r, across)]
        })
        .collect();
    // Gauge lines: the bolts' places across (within 2 mm).
    let mut lines: Vec<(f64, Vec<f64>)> = Vec::new();
    for q in &pts {
        match lines.iter_mut().find(|l| (l.0 - q[1]).abs() < 0.002) {
            Some(l) => l.1.push(q[0]),
            None => lines.push((q[1], vec![q[0]])),
        }
    }
    let per_line = lines.iter().map(|l| l.1.len()).max().unwrap_or(1) as u32;
    let pitch = lines
        .iter()
        .filter(|l| l.1.len() > 1)
        .flat_map(|l| {
            let mut s = l.1.clone();
            s.sort_by(f64::total_cmp);
            s.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>()
        })
        .reduce(f64::min);
    let lo = lines.iter().map(|l| l.0).fold(f64::INFINITY, f64::min);
    let hi = lines.iter().map(|l| l.0).fold(f64::NEG_INFINITY, f64::max);
    let span = (hi - lo).max(0.0);
    let ray = |q: &[f64; 3], d: [f64; 3]| ray_to_outline(outline, *q, d);
    let least = |d: [f64; 3]| -> Option<f64> {
        let mut out: Option<f64> = None;
        for q in &a.bolt_points {
            let r = ray(q, d)?;
            out = Some(out.map_or(r, |x: f64| x.min(r)));
        }
        out
    };
    let minus = |v: [f64; 3]| [-v[0], -v[1], -v[2]];
    let end = [least(along), least(minus(along))];
    let edge = match (least(across), least(minus(across))) {
        (Some(x), Some(y)) => Some(x.min(y)),
        _ => None,
    };
    if end[0].is_none() || end[1].is_none() {
        notes.push(format!("{}: a bolt lies outside plate {id}", a.member));
    }
    // The width the net section crosses: the bolt group's width plus the
    // code's minimum edge distance each side (on the safe side: the
    // standards give a gusset plate no width), or the plate's own width
    // through the bolts when narrower.
    let edge_min = idx
        .and_then(|k| {
            ohl_structures_fe::bolt_rules::rules_in(
                model,
                k,
                a.end,
                None,
                Some((steel.thickness, steel.fu)),
            )
        })
        .map(|r| r.edge);
    let own = a
        .bolt_points
        .iter()
        .filter_map(|q| Some(ray(q, across)? + ray(q, minus(across))?))
        .reduce(f64::min);
    let (width, width_words) = match (edge_min, own) {
        (Some(e), Some(w)) if w < span + 2.0 * e => (
            w,
            format!("the plate's own {:.0} mm through the bolts", w * 1000.0),
        ),
        (Some(e), _) => (
            span + 2.0 * e,
            format!(
                "the bolts' {:.0} mm + 2 × the minimum edge distance {:.0} mm",
                span * 1000.0,
                e * 1000.0
            ),
        ),
        (None, Some(w)) => (
            w,
            format!("the plate's own {:.0} mm through the bolts", w * 1000.0),
        ),
        (None, None) => (span, "the bolts' width alone".to_owned()),
    };
    Some(BoltGroup {
        count: a.bolt_points.len() as u32,
        diameter: set.diameter,
        hole: set.hole.or(end_bolts.and_then(|b| b.hole)),
        bolt_ultimate: end_bolts.and_then(|b| b.ultimate),
        lines: lines.len() as u32,
        per_line,
        pitch,
        span,
        end,
        edge,
        width,
        width_words,
    })
}

/// The members on each side of the weld between plates `a` and `b` of
/// joint `j` (the welded plate split there), with their direction from
/// the joint; a ring (b reached from a without that weld): every member
/// of the welded plate on one side.
#[allow(clippy::type_complexity)]
fn weld_sides(j: &JointDetail, a: &str, b: &str) -> ([Vec<(String, [f64; 3])>; 2], bool) {
    let linked = |x: &str, y: &str| {
        j.plates
            .iter()
            .any(|p| p.id == x && p.welded_to.iter().any(|o| o == y))
    };
    let reach = |from: &str| -> Vec<String> {
        let mut seen = vec![from.to_owned()];
        let mut k = 0;
        while k < seen.len() {
            let x = seen[k].clone();
            for q in &j.plates {
                let cut = (x == a && q.id == b) || (x == b && q.id == a);
                if !cut && !seen.contains(&q.id) && (linked(&x, &q.id) || linked(&q.id, &x)) {
                    seen.push(q.id.clone());
                }
            }
            k += 1;
        }
        seen
    };
    let members = |plates: &[String]| -> Vec<(String, [f64; 3])> {
        j.arrivals
            .iter()
            .filter(|x| {
                (x.onto.value.as_ref().is_some_and(|o| plates.contains(o))
                    || x.other_onto.as_ref().is_some_and(|o| plates.contains(o)))
                    && !x.modelling
            })
            .map(|x| (x.member.clone(), x.direction))
            .collect()
    };
    let (sa, sb) = (reach(a), reach(b));
    if sa.iter().any(|x| x == b) {
        return ([members(&sa), Vec::new()], true);
    }
    ([members(&sa), members(&sb)], false)
}

/// The plate checks of one case: `axial` each member's axial force (N,
/// tension positive), `strength` the case's strength factor.
pub fn plate_usage(
    plans: &PlatePlans,
    axial: &BTreeMap<String, f64>,
    strength: f64,
) -> Vec<PlateUsage> {
    plans
        .parts
        .iter()
        .map(|x| {
            let (part, force, checked) = match &x.plan {
                Plan::Bolted {
                    member,
                    group,
                    steel,
                    share,
                } => {
                    let n = axial.get(member).copied().unwrap_or(0.0) * share;
                    (
                        PlatePart::Member(member.clone()),
                        n,
                        plates::bolted(plans.method, group, steel, n),
                    )
                }
                Plan::Unplaced { member } => (
                    PlatePart::Member(member.clone()),
                    axial.get(member).copied().unwrap_or(0.0),
                    plates::Checked::default(),
                ),
                Plan::NoSeam { with } => (
                    PlatePart::Weld(with.clone()),
                    0.0,
                    plates::Checked::default(),
                ),
                Plan::Weld {
                    with,
                    input,
                    sides,
                    ring,
                } => {
                    let f = weld_force(sides, *ring, axial);
                    (
                        PlatePart::Weld(with.clone()),
                        f,
                        plates::weld(plans.method, input, f),
                    )
                }
            };
            let gov = checked.governing(strength);
            PlateUsage {
                joint: x.joint.clone(),
                plate: x.plate.clone(),
                part,
                force,
                usage: gov.map(|g| g.0),
                governs: gov.map(|g| g.1.what.clone()).unwrap_or_else(|| {
                    checked
                        .notes
                        .last()
                        .or(x.notes.last())
                        .cloned()
                        .unwrap_or_else(|| "not checked".into())
                }),
                checks: checked
                    .checks
                    .iter()
                    .map(|c| (c.what.clone(), c.usage(strength)))
                    .collect(),
                notes: x.notes.iter().chain(&checked.notes).cloned().collect(),
            }
        })
        .collect()
}

/// The force a weld carries: the larger resultant of the member forces on
/// its two sides (a ring: the sum of every member's force), N.
fn weld_force(
    sides: &[Vec<(String, [f64; 3])>; 2],
    ring: bool,
    axial: &BTreeMap<String, f64>,
) -> f64 {
    let n = |m: &str| axial.get(m).copied().unwrap_or(0.0);
    if ring {
        return sides[0].iter().map(|(m, _)| n(m).abs()).sum();
    }
    sides
        .iter()
        .map(|s| {
            let r = s.iter().fold([0.0; 3], |r, (m, d)| {
                let f = n(m);
                [r[0] + f * d[0], r[1] + f * d[1], r[2] + f * d[2]]
            });
            dot(r, r).sqrt()
        })
        .fold(0.0, f64::max)
}

/// The worst plate usage per joint of a case's plate checks.
pub fn joint_plate_usage(plates: &[PlateUsage]) -> BTreeMap<String, f64> {
    let mut out: BTreeMap<String, f64> = BTreeMap::new();
    for p in plates {
        if let Some(u) = p.usage {
            let e = out.entry(p.joint.clone()).or_insert(u);
            if ohl_structures_fe::check::usage_order(u, *e).is_gt() {
                *e = u;
            }
        }
    }
    out
}

/// From `origin` (in plate `outline`'s plane) along `dir` (in its plane)
/// to the outline, m; none when `origin` lies outside it.
fn ray_to_outline(outline: &[[f64; 3]], origin: [f64; 3], dir: [f64; 3]) -> Option<f64> {
    let n = crate::welds::polygon_normal(outline)?;
    let u0 = sub(outline[1], outline[0]);
    let u = scale(u0, 1.0 / norm(u0).max(1e-12));
    let v = cross(n, u);
    let flat = |q: [f64; 3]| {
        let d = sub(q, outline[0]);
        [dot(d, u), dot(d, v)]
    };
    let o = flat(origin);
    let d = [dot(dir, u), dot(dir, v)];
    let l = d[0].hypot(d[1]);
    if l < 1e-9 {
        return None;
    }
    let d = [d[0] / l, d[1] / l];
    let poly: Vec<[f64; 2]> = outline.iter().map(|q| flat(*q)).collect();
    // Inside: the crossings of a ray to +x.
    let mut inside = false;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        if (a[1] > o[1]) != (b[1] > o[1]) {
            let x = a[0] + (o[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if x > o[0] {
                inside = !inside;
            }
        }
    }
    if !inside {
        return None;
    }
    let mut best: Option<f64> = None;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let e = [b[0] - a[0], b[1] - a[1]];
        let den = d[0] * e[1] - d[1] * e[0];
        if den.abs() < 1e-14 {
            continue;
        }
        let w = [a[0] - o[0], a[1] - o[1]];
        let t = (w[0] * e[1] - w[1] * e[0]) / den;
        let s = (w[0] * d[1] - w[1] * d[0]) / den;
        if t >= 0.0 && (-1e-12..=1.0 + 1e-12).contains(&s) {
            best = Some(best.map_or(t, |x: f64| x.min(t)));
        }
    }
    best
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The ray from inside a 200 × 100 mm rectangle to its edge, in each
    /// direction; none from outside.
    #[test]
    fn rays_reach_the_outline() {
        let r = [
            [0.0, 0.0, 0.0],
            [0.2, 0.0, 0.0],
            [0.2, 0.1, 0.0],
            [0.0, 0.1, 0.0],
        ];
        let o = [0.05, 0.03, 0.0];
        assert!((ray_to_outline(&r, o, [1.0, 0.0, 0.0]).unwrap() - 0.15).abs() < 1e-12);
        assert!((ray_to_outline(&r, o, [-1.0, 0.0, 0.0]).unwrap() - 0.05).abs() < 1e-12);
        assert!((ray_to_outline(&r, o, [0.0, 1.0, 0.0]).unwrap() - 0.07).abs() < 1e-12);
        assert!(ray_to_outline(&r, [0.3, 0.03, 0.0], [1.0, 0.0, 0.0]).is_none());
    }

    /// The conservative weld force (invented data): plate G2 with two
    /// members, 100 kN tension along x and 60 kN compression along y,
    /// welded to G1 with one member of 30 kN tension along z: the G2 side's
    /// resultant √(100² + 60²) = 116.6 kN governs over G1's 30 kN; a ring:
    /// the sum 190 kN.
    #[test]
    fn a_weld_carries_the_larger_resultant() {
        let sides = [
            vec![("A".to_owned(), [0.0, 0.0, 1.0])],
            vec![
                ("B".to_owned(), [1.0, 0.0, 0.0]),
                ("C".to_owned(), [0.0, 1.0, 0.0]),
            ],
        ];
        let axial: BTreeMap<String, f64> = [("A", 30e3), ("B", 100e3), ("C", -60e3)]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect();
        let f = weld_force(&sides, false, &axial);
        assert!((f - (100e3f64.hypot(60e3))).abs() < 1e-6, "{f}");
        let all = [
            sides[0].iter().chain(&sides[1]).cloned().collect(),
            Vec::new(),
        ];
        assert!((weld_force(&all, true, &axial) - 190e3).abs() < 1e-6);
    }
}
