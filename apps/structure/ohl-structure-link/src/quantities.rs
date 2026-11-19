// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Weight and quantities of a lattice structure (joint details stage 4.6,
//! 2026-11-19): its parts counted from its members and its joint details
//! ([`structure_quantities`]: the rules are `ohl_bom::structure`'s), and
//! the self-weight a run takes from them ([`counted_self_weight`]).
//!
//! Counted at each joint, from the connection records only (a member end
//! without a `[[connection]]` is a joint not defined, its parts not
//! counted): the bolts of each end where the joint details place them
//! (their library mass, else weighed from their sizes), the gusset plates
//! and the splices' covers (their outlines, given or worked out round the
//! bolts), the welds between plates. What the automatic joint layout laid
//! out is marked automatic.

use std::collections::{BTreeMap, BTreeSet};

use ohl_bom::structure::{
    FILLET_THROAT_MAX, Part, PartKind, StructureBom, bolt_mass_estimate, fillet_section, hole_area,
    plate_mass, polygon_area, steel_density,
};
use ohl_sections::SectionLibrary;
use ohl_structures::bolts::BoltLibrary;
use ohl_structures::connections::{BoltedLeg, ConnectionKind, CoverLeg, WeldKind};
use ohl_structures::structures::SelfWeightFrom;
use ohl_structures::{LibraryItem as _, StructureType};
use ohl_structures_fe::Model;

use crate::joints::{Arrival, BOLT_DRAWN, JointDetail, bolted_onto};

fn mm(x: f64) -> String {
    let v = x * 1000.0;
    if (v - v.round()).abs() < 0.05 {
        format!("{:.0}", v.round())
    } else {
        format!("{v:.1}")
    }
}

/// How much shorter a plate `t` thick bent through `angle` degrees at an
/// inside radius `r` is developed than its two flats taken to the line
/// where their middle planes meet, per metre of bend, m: the two straight
/// lengths to that line less the arc, both at the middle of the thickness
/// (`(r + t/2)(2 tan(a/2) − a)`; J2b, 2026-11-19). The middle of the
/// thickness is taken as the line that keeps its length — of the usual
/// assumptions the longest arc, so the heavier count.
pub fn bend_shortening(r: f64, t: f64, angle: f64) -> f64 {
    let a = angle.clamp(0.0, 179.0).to_radians();
    (r.max(0.0) + t / 2.0) * (2.0 * (a / 2.0).tan() - a)
}

/// The joints whose member ends are not all defined, with why: an end
/// without a `[[connection]]` (the members named), a joint the automatic
/// layout left for a hand detail. Cheap (no joint details worked out).
pub fn undefined_joints(t: &StructureType) -> BTreeMap<String, String> {
    let mut without: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (m, j) in t.ends_without_joint_details() {
        without.entry(j.to_owned()).or_default().push(m.to_owned());
    }
    let mut out: BTreeMap<String, String> = without
        .into_iter()
        .map(|(j, ms)| {
            let n = ms.len();
            let named = if n > 3 {
                format!("{} … ({n} ends)", ms[..3].join(", "))
            } else {
                ms.join(", ")
            };
            (j, format!("{named}: no connection"))
        })
        .collect();
    for (j, why) in crate::LayoutRecord::of(t).hand {
        let words = format!("left for a hand detail ({why})");
        out.entry(j)
            .and_modify(|w| {
                w.push_str("; ");
                w.push_str(&words);
            })
            .or_insert(words);
    }
    out
}

/// One library bolt or typed bolt kind's name in the bill: the library
/// bolt's id (`M16-8.8`), else `M16` and its grade.
fn bolt_name(b: &crate::BoltSet) -> String {
    match &b.bolt {
        Some(id) => id.clone(),
        None => {
            let mut n = format!("M{}", mm(b.diameter));
            if let Some(g) = &b.grade {
                n.push(' ');
                n.push_str(g);
            }
            n
        }
    }
}

/// What the counting found, for the notes.
#[derive(Default)]
struct Tally {
    estimated_bolts: usize,
    library_bolts_longer: usize,
    library_no_grip: usize,
    outlines_worked_out: usize,
    welds_without_size: usize,
    welded_ends: Vec<String>,
    held_ends: Vec<String>,
    modelling: usize,
}

/// The bolts of end `a` at joint `joint` as parts: one per grip (rounded
/// up to the millimetre).
fn bolt_parts(
    a: &Arrival,
    joint: &str,
    b: &crate::BoltSet,
    library: &BoltLibrary,
    automatic: bool,
    tally: &mut Tally,
) -> Vec<Part> {
    let mut grips: BTreeMap<i64, usize> = BTreeMap::new();
    let mut placed = 0;
    for (_, _, grip) in a.bolts_with_axes() {
        let g = ((grip[0] + grip[1]) * 1000.0 - 1e-6).ceil() as i64;
        *grips.entry(g).or_default() += 1;
        placed += 1;
    }
    if placed == 0 && b.count > 0 {
        let g = ((a.grip[0] + a.grip[1]) * 1000.0 - 1e-6).ceil() as i64;
        grips.insert(g, b.count as usize);
    }
    let rho = steel_density();
    let lib = b.bolt.as_deref().and_then(|id| library.get(id));
    let [flats, head, nut, beyond] = BOLT_DRAWN;
    let d = b.diameter;
    let mut out = Vec::new();
    for (g, n) in grips {
        let grip = g as f64 / 1000.0;
        let (each, estimated) = match lib.and_then(|l| l.mass_kg.map(|m| (l, m))) {
            Some((l, m)) => match l.grip.map(|x| x.si()) {
                Some(sold) if grip > sold + 1e-9 => {
                    tally.library_bolts_longer += n;
                    (m + hole_area(d) * (grip - sold) * rho, false)
                }
                Some(_) => (m, false),
                None => {
                    tally.library_no_grip += n;
                    (m, false)
                }
            },
            None => {
                tally.estimated_bolts += n;
                let shank = grip + b.drawn[2].unwrap_or(nut * d) + beyond * d;
                (
                    bolt_mass_estimate(
                        d,
                        shank,
                        b.drawn[0].unwrap_or(flats * d),
                        b.drawn[1].unwrap_or(head * d),
                        b.drawn[2].unwrap_or(nut * d),
                        rho,
                    ),
                    true,
                )
            }
        };
        out.push(Part {
            kind: PartKind::Bolt,
            item: format!("{} grip {g} mm", bolt_name(b)),
            id: format!("{} at {joint}", a.member),
            joint: Some(joint.to_owned()),
            count: n,
            quantity: n as f64,
            mass: each * n as f64,
            light: each * n as f64,
            automatic,
            estimated,
        });
    }
    out
}

/// The holes through part `id` at joint `j` (a plate, or a splice's
/// cover on `leg`: none, every leg), m²: each bolt of the ends bolted
/// onto it, its hole (else its diameter).
fn holes_through(j: &JointDetail, id: &str, leg: Option<Option<CoverLeg>>) -> f64 {
    let own = j
        .plates
        .iter()
        .filter(|p| p.id == id)
        .filter_map(|p| p.leg_bolts.as_deref())
        .map(std::borrow::Cow::Borrowed);
    bolted_onto(&j.arrivals, id)
        .into_iter()
        .chain(own)
        .filter(|a| match leg {
            Some(Some(l)) => match a.leg.value {
                Some(BoltedLeg::Long) => l == CoverLeg::Long,
                Some(BoltedLeg::Short) => l == CoverLeg::Short,
                _ => true,
            },
            _ => true,
        })
        .filter_map(|a| {
            let b = a.bolts.value.as_ref()?;
            let n = a.bolt_points.len().max(if a.bolt_points.is_empty() {
                b.count as usize
            } else {
                0
            });
            Some(n as f64 * hole_area(b.hole.unwrap_or(b.diameter)))
        })
        .sum()
}

/// The structure file `text`'s weight and quantities: its members (each
/// its section × its length, as the finite-element model weighs them)
/// and the parts its joint details define (sections from `library`, bolts
/// from `bolts`). Err: no members of its own (a pole, a separate model
/// file) or the model does not build.
///
/// Expected cost: the joint details of every joint (about 0.1–1 s on a
/// real tower of 1 000 joints, more under load).
pub fn structure_quantities(
    text: &str,
    library: &SectionLibrary,
    bolts: &BoltLibrary,
) -> Result<StructureBom, String> {
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    if t.model.is_some() {
        return Err("the structure names a separate model file: no members of its own".into());
    }
    let (details, model) = crate::joints::joint_details_and_model(text, library, bolts, None)?;
    let model = model.ok_or_else(|| {
        details
            .notes
            .first()
            .cloned()
            .unwrap_or_else(|| "the model is not built (level 3 needed)".into())
    })?;
    Ok(quantities_of(&t, &model, &details, bolts))
}

/// [`structure_quantities`] of the structure, its model and its joint
/// details worked out already.
pub(crate) fn quantities_of(
    t: &StructureType,
    model: &Model,
    details: &crate::JointDetails,
    bolts: &BoltLibrary,
) -> StructureBom {
    let mut parts = Vec::new();
    let mut tally = Tally::default();
    let density: BTreeMap<&str, f64> = model
        .members
        .iter()
        .map(|m| (m.id.as_str(), model.sections[m.section].density))
        .collect();
    // Where each member's steel stops at its ends, from the work points
    // (the joint details' cut: given, else worked out as drawn): beyond
    // the work point (below 0) added to the heavy count, short of it
    // deducted in the light count.
    let mut beyond: BTreeMap<&str, f64> = BTreeMap::new();
    let mut short: BTreeMap<&str, f64> = BTreeMap::new();
    for j in &details.joints {
        for a in &j.arrivals {
            if let Some(c) = a.cut.value {
                if c < 0.0 {
                    *beyond.entry(a.member.as_str()).or_default() -= c;
                } else {
                    *short.entry(a.member.as_str()).or_default() += c;
                }
            }
        }
    }
    // The holes through each member's legs at the ends counted (m³): the
    // light count deducts them.
    let mut member_holes: BTreeMap<String, f64> = BTreeMap::new();
    let zero_density: Vec<&str> = model
        .sections
        .iter()
        .filter(|s| s.density <= 0.0)
        .map(|s| s.id.as_str())
        .collect();
    // The steel of a joint's parts: the heaviest of the members on it,
    // else SIA 261's.
    let steel_of = |members: &[String]| -> f64 {
        let d = members
            .iter()
            .filter_map(|m| density.get(m.as_str()).copied())
            .fold(0.0, f64::max);
        if d > 0.0 { d } else { steel_density() }
    };
    // The welds' filler: SIA 261's steel (heavier than most libraries').
    let weld_rho = steel_density();
    let record: BTreeSet<(&str, &str)> = t
        .connections
        .iter()
        .map(|c| (c.member.as_str(), c.joint.as_str()))
        .collect();
    let layout = crate::LayoutRecord::of(t);
    let provisional: BTreeSet<(String, String)> = t.provisional_bolts().into_iter().collect();
    let mut undefined = undefined_joints(t);
    let mut add_undefined = |j: &str, why: String| {
        undefined
            .entry(j.to_owned())
            .and_modify(|w| {
                w.push_str("; ");
                w.push_str(&why);
            })
            .or_insert(why);
    };
    for j in &details.joints {
        // A plate's own bolts onto the leg (`[plate.leg_bolts]`,
        // 2026-11-19): bolts, and holes through the leg.
        for p in &j.plates {
            if let Some(a) = p.leg_bolts.as_deref()
                && let Some(b) = &a.bolts.value
            {
                let hole = hole_area(b.hole.unwrap_or(b.diameter));
                let through: f64 = a.bolts_with_axes().map(|(_, _, g)| hole * g[0]).sum();
                *member_holes.entry(a.member.clone()).or_default() += through;
                parts.extend(bolt_parts(a, &j.id, b, bolts, false, &mut tally));
            }
        }
        for a in &j.arrivals {
            if a.modelling || !record.contains(&(a.member.as_str(), j.id.as_str())) {
                continue;
            }
            if a.continuous.is_some() {
                continue;
            }
            let key = (a.member.clone(), j.id.clone());
            let automatic = provisional.contains(&key) || layout.ends.contains_key(&key);
            match a.kind.value.unwrap_or_default() {
                ConnectionKind::Continuous => {}
                ConnectionKind::Bolted => match &a.bolts.value {
                    Some(b) => {
                        let hole = hole_area(b.hole.unwrap_or(b.diameter));
                        let through: f64 = a.bolts_with_axes().map(|(_, _, g)| hole * g[0]).sum();
                        *member_holes.entry(a.member.clone()).or_default() += through;
                        parts.extend(bolt_parts(a, &j.id, b, bolts, automatic, &mut tally));
                    }
                    None => {
                        add_undefined(&j.id, format!("{}: bolted, no bolts given", a.member));
                    }
                },
                ConnectionKind::Welded => {
                    tally.welded_ends.push(format!("{} at {}", a.member, j.id))
                }
                ConnectionKind::Pinned | ConnectionKind::Fixed => {
                    tally.held_ends.push(format!("{} at {}", a.member, j.id));
                }
            }
        }
        let thickness: BTreeMap<&str, f64> = j
            .plates
            .iter()
            .map(|p| (p.id.as_str(), p.thickness))
            .collect();
        for p in &j.plates {
            let area = polygon_area(&p.outline);
            if area <= 0.0 {
                add_undefined(&j.id, format!("plate {}: no outline", p.id));
                continue;
            }
            // A bent plate (J2b): its two flats' outlines meet at the bend
            // line (their middle planes); the developed area is less by the
            // bend's straight lengths there against its arc, both at the
            // middle of the thickness (the flats taken to the bend line
            // would weigh a little more).
            let (area, bent_from) = match &p.bend {
                Some(b) => {
                    let len = b.line.map_or(0.0, |l| {
                        ((l[1][0] - l[0][0]).powi(2)
                            + (l[1][1] - l[0][1]).powi(2)
                            + (l[1][2] - l[0][2]).powi(2))
                        .sqrt()
                    });
                    (
                        (area - len * bend_shortening(b.inner_radius, p.thickness, b.angle))
                            .max(0.0),
                        format!(", bent from {} (developed)", b.to),
                    )
                }
                None => (area, String::new()),
            };
            if !p.outline_given {
                tally.outlines_worked_out += 1;
            }
            let rho = steel_of(&p.members);
            let holes = holes_through(j, &p.id, None).min(area);
            let material = p
                .material
                .as_deref()
                .map_or(String::new(), |m| format!(" {m}"));
            parts.push(Part {
                kind: PartKind::Plate,
                item: format!("plate {} mm{material}{bent_from}", mm(p.thickness)),
                id: p.id.clone(),
                joint: Some(j.id.clone()),
                count: 1,
                quantity: area,
                mass: plate_mass(area, p.thickness, rho),
                light: plate_mass(area - holes, p.thickness, rho),
                automatic: layout.made.contains_key(&p.id),
                estimated: !p.outline_given,
            });
            for w in &p.welds {
                let thinner = thickness
                    .get(w.with.as_str())
                    .map_or(p.thickness, |o| o.min(p.thickness));
                // A length given is the whole weld (both sides together
                // when welded on both, SIA 263 6.3.2.5); none: the seam's,
                // both sides in the heavy count, one in the light.
                let (length, sides_heavy, sides_light) = match w.length {
                    Some(l) => (l, 1.0, 1.0),
                    None => (w.seam_length(), 2.0, 1.0),
                };
                let (section, light_section, item, estimated) = match (w.kind, w.size) {
                    (WeldKind::Fillet, Some(a)) => {
                        let (h, l) = fillet_section(a, w.angle());
                        (h, l, format!("fillet a {} mm", mm(a)), false)
                    }
                    (WeldKind::Fillet, None) => {
                        tally.welds_without_size += 1;
                        let a = FILLET_THROAT_MAX * thinner;
                        (
                            fillet_section(a, w.angle()).0,
                            0.0,
                            "fillet, size not given".to_owned(),
                            true,
                        )
                    }
                    (WeldKind::FullPenetration, _) => (
                        thinner * thinner,
                        0.0,
                        format!("full penetration t {} mm", mm(thinner)),
                        true,
                    ),
                };
                parts.push(Part {
                    kind: PartKind::Weld,
                    item,
                    id: format!("{}–{}", p.id, w.with),
                    joint: Some(j.id.clone()),
                    count: 1,
                    quantity: length * sides_heavy,
                    mass: section * length * sides_heavy * weld_rho,
                    light: light_section * length * sides_light * weld_rho,
                    automatic: layout.made.contains_key(&p.id),
                    estimated,
                });
            }
        }
        for sp in &j.splices {
            for c in &sp.covers {
                let area = polygon_area(&c.outline);
                if area <= 0.0 {
                    add_undefined(
                        &j.id,
                        format!("splice {}: its {} cover has no outline", sp.id, c.words()),
                    );
                    continue;
                }
                if !c.outline_given {
                    tally.outlines_worked_out += 1;
                }
                let rho = steel_of(&sp.members);
                let holes = holes_through(j, &sp.id, Some(c.leg)).min(area);
                parts.push(Part {
                    kind: PartKind::Cover,
                    item: format!("cover {} mm", mm(c.thickness)),
                    id: format!("{} {}", sp.id, c.words()),
                    joint: Some(j.id.clone()),
                    count: 1,
                    quantity: area,
                    mass: plate_mass(area, c.thickness, rho),
                    light: plate_mass(area - holes, c.thickness, rho),
                    automatic: false,
                    estimated: !c.outline_given,
                });
            }
        }
    }
    // The members: the heavy count over their length between the joints
    // (the model's) and their steel beyond the work points, the light
    // count over their steel only, their holes deducted.
    let mut steel = 0.0;
    for m in &model.members {
        let s = &model.sections[m.section];
        let length = model.length(m);
        let per_m = s.density * s.area;
        steel += per_m * length;
        if !m.check {
            tally.modelling += 1;
        }
        let id = m.id.as_str();
        let heavy = length + beyond.get(id).copied().unwrap_or(0.0);
        let light = (length - short.get(id).copied().unwrap_or(0.0)).max(0.0);
        let holes = member_holes.get(id).copied().unwrap_or(0.0) * s.density;
        parts.push(Part {
            kind: PartKind::Member,
            item: s.id.clone(),
            id: m.id.clone(),
            joint: None,
            count: 1,
            quantity: heavy,
            mass: per_m * heavy,
            light: (per_m * light - holes).max(0.0),
            automatic: false,
            estimated: false,
        });
    }
    let mut notes = vec![
        "members: the heavy count each its section × its length between its joints (the \
         finite-element model's self-weight) and its steel beyond the work points, holes not \
         deducted; the light count its steel only (short of the work points as the joint \
         details cut it), the holes of the ends counted deducted"
            .to_owned(),
        "plates and covers: the outline's area × the thickness × the steel of the members on \
         them (none: SIA 261 Table 28, 78.5 kN/m³); holes not deducted in the heavy count, \
         deducted in the light count"
            .to_owned(),
    ];
    if !zero_density.is_empty() {
        notes.push(format!(
            "NOT ON THE SAFE SIDE: section(s) {} without a density — weighed 0 kg/m",
            zero_density.join(", ")
        ));
    }
    if tally.modelling > 0 {
        notes.push(format!(
            "{} modelling member(s) (not checked) counted as the model weighs them",
            tally.modelling
        ));
    }
    if tally.outlines_worked_out > 0 {
        notes.push(format!(
            "NOT ON THE SAFE SIDE: {} plate or cover outline(s) worked out round their bolts \
             (the least plate keeping the edge distances; a drawn plate is often larger)",
            tally.outlines_worked_out
        ));
    }
    if tally.estimated_bolts > 0 {
        notes.push(format!(
            "{} bolt(s) without a mass in the bolt library: weighed from their sizes (shank \
             over the grip, the nut and what stands beyond, head and nut as hexagonal prisms; \
             the drawing proportions where the library gives no sizes) — washers not counted, \
             NOT ON THE SAFE SIDE by their mass",
            tally.estimated_bolts
        ));
    }
    if tally.library_bolts_longer > 0 {
        notes.push(format!(
            "{} bolt(s) clamping more than their library grip: the library mass and the shank \
             beyond it",
            tally.library_bolts_longer
        ));
    }
    if tally.library_no_grip > 0 {
        notes.push(format!(
            "{} bolt(s): the library mass as sold, its grip not given (NOT ON THE SAFE SIDE \
             where they clamp more)",
            tally.library_no_grip
        ));
    }
    if tally.welds_without_size > 0 {
        notes.push(format!(
            "{} weld(s) without a size: heavy count with the throat 0.7 × the thinner plate \
             (the most SIA 263 6.3.6.3 allows), light count without them",
            tally.welds_without_size
        ));
    }
    if !tally.welded_ends.is_empty() {
        notes.push(format!(
            "NOT ON THE SAFE SIDE: {} welded member end(s) — their welds have no size in the \
             data, not counted ({})",
            tally.welded_ends.len(),
            first_names(&tally.welded_ends)
        ));
    }
    if !tally.held_ends.is_empty() {
        notes.push(format!(
            "NOT ON THE SAFE SIDE: {} pinned or fixed member end(s) — what holds them is not in \
             the data, not counted ({})",
            tally.held_ends.len(),
            first_names(&tally.held_ends)
        ));
    }
    notes.push(
        "NOT ON THE SAFE SIDE: not counted — galvanising, fillers and packs (no record in the \
         data), step bolts, ladders and the other fittings not modelled as parts"
            .to_owned(),
    );
    let mut bom = StructureBom::new(&t.id, parts, undefined.into_iter().collect(), notes);
    // What the dead-load factor multiplies: the model's members' steel.
    bom.members_mass = steel;
    bom
}

fn first_names(v: &[String]) -> String {
    if v.len() > 5 {
        format!("{} …", v[..5].join(", "))
    } else {
        v.join(", ")
    }
}

/// How a run takes the structure's self-weight.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SelfWeight {
    /// The counted mass entered the run (where it governs, or alone when
    /// asked for).
    pub counted: bool,
    /// The factor on every member's steel the run takes instead of the
    /// model's dead-load factors (none: the model as it is).
    pub factor: Option<f64>,
    /// The factor on the self-weight factor of the cases whose factor is
    /// below 1 (the weight holding the tower): their mass over the mass the
    /// model then carries (1: as the others).
    pub light_ratio: f64,
    /// The weight and quantities (counted only).
    pub quantities: Option<StructureBom>,
    /// What the run says: which self-weight, and why.
    pub note: Option<String>,
}

/// The self-weight of a run of structure `t` (its file `text`) on its
/// extension `extension`, its model `model`. Counted only where every
/// joint is defined (no end without a `[[connection]]`, none left for a
/// hand detail, every bolted end with its bolts) and the run stands on the
/// structure as written (an extension with its own members keeps the
/// factor: its joints are not counted); else the members' steel × the
/// dead-load factor, said why. Then by `self_weight_from`
/// ([`SelfWeightFrom`]):
///
/// - the envelope (the default, until the owner validates the counting):
///   where the weight loads the tower (a case's self-weight factor of 1 or
///   more) the larger of the heavy count and the members × the dead-load
///   factor; where it holds the tower (below 1) the lesser of the light
///   count and the members × the factor — the counted mass only ever
///   raises a load;
/// - the factor: as before (said NOT ON THE SAFE SIDE where the heavy count
///   is above it);
/// - the counted mass alone: heavy and light counts, said NOT ON THE SAFE
///   SIDE until the counting is validated.
///
/// Only a tower's own members (no separate model file) are counted; others
/// get nothing.
pub fn counted_self_weight(
    t: &StructureType,
    text: &str,
    library: &SectionLibrary,
    bolts: &BoltLibrary,
    extension: Option<&str>,
    model: &Model,
) -> SelfWeight {
    if t.model.is_some() || !ohl_structure_edit::is_tower(t.kind) || model.members.is_empty() {
        return SelfWeight::default();
    }
    let kept = |why: String| SelfWeight {
        light_ratio: 1.0,
        note: Some(format!(
            "self weight: the members' steel × {} (factor kept: {why})",
            factor_words(model)
        )),
        ..SelfWeight::default()
    };
    let from = t.self_weight_from();
    let undefined = undefined_joints(t);
    if !undefined.is_empty() {
        return kept(format!("{} joint(s) not defined", undefined.len()));
    }
    if let Some(e) = extension.and_then(|e| t.extension(e))
        && e.changes_model()
    {
        return kept(format!("extension {}: its joints are not counted", e.name));
    }
    let bom = match structure_quantities(text, library, bolts) {
        Ok(b) => b,
        Err(e) => return kept(format!("the parts not counted: {e}")),
    };
    if !bom.every_joint_defined() {
        return kept(format!("{} joint(s) not defined", bom.undefined.len()));
    }
    let (Some(heavy), Some(light)) = (bom.factor(), bom.light_factor()) else {
        return kept("the members weigh nothing".into());
    };
    let steel = model.steel_weight();
    let typed = if steel > 0.0 {
        model.weight() / steel
    } else {
        1.0
    };
    let words = factor_words(model);
    let not_counted = "not counted: galvanising, fillers and packs, fittings (Weight & quantities)";
    let (factor, held, counted, note) = match from {
        SelfWeightFrom::Factor => {
            let mut note = format!(
                "self weight: the members' steel × {words} (self_weight_from = \"factor\"; the \
                 counted mass is {heavy:.3} × the members' steel)"
            );
            if heavy > typed {
                note.push_str(&format!(
                    "; NOT ON THE SAFE SIDE: the counted mass, {:.0} kg, is above the factor's \
                     ({typed:.3} ×)",
                    bom.mass
                ));
            }
            (None, typed, false, note)
        }
        SelfWeightFrom::Counted => (
            Some(heavy),
            light,
            true,
            format!(
                "self weight: the counted mass alone (self_weight_from = \"counted\"), {:.0} kg \
                 ({heavy:.3} × the members' steel), instead of {words} ({typed:.3} ×); the cases \
                 whose self-weight factor is below 1 take the light count, {:.0} kg \
                 ({light:.3} ×). NOT ON THE SAFE SIDE until the counting is validated \
                 ({not_counted})",
                bom.mass, bom.light
            ),
        ),
        SelfWeightFrom::Envelope => {
            let loads = if heavy > typed {
                format!(
                    "counted mass {heavy:.3} × the members' steel ({:.0} kg), above {words} \
                     ({typed:.3} ×): counted mass taken where the weight loads the tower",
                    bom.mass
                )
            } else {
                format!(
                    "counted mass {heavy:.3} × the members' steel, below {words} ({typed:.3} ×): \
                     factor kept (counting not validated)"
                )
            };
            let holds = if light < typed {
                format!(
                    "the cases whose self-weight factor is below 1 (the weight holding the \
                     tower) take the light count, {:.0} kg ({light:.3} ×), below the factor's",
                    bom.light
                )
            } else {
                format!(
                    "the cases whose self-weight factor is below 1 keep the factor's mass \
                     ({typed:.3} ×), below the light count ({light:.3} ×)"
                )
            };
            (
                (heavy > typed).then_some(heavy),
                light.min(typed),
                heavy > typed || light < typed,
                format!(
                    "self weight: {loads}; {holds} — the larger where the weight loads the \
                     tower, the lesser where it holds it, until the counting is validated; \
                     {not_counted}"
                ),
            )
        }
    };
    let carried = factor.unwrap_or(typed);
    SelfWeight {
        counted,
        factor,
        light_ratio: if carried > 0.0 { held / carried } else { 1.0 },
        quantities: Some(bom),
        note: Some(note),
    }
}

/// The model's dead-load factor in words: "the dead-load factor 1.3",
/// "no dead-load factor", "the members' own dead-load factors".
fn factor_words(model: &Model) -> String {
    if model.members.iter().any(|m| m.dead_load_factor.is_some()) {
        "the members' own dead-load factors".into()
    } else if model.analysis.dead_load_given {
        format!("the dead-load factor {}", model.analysis.dead_load_factor)
    } else {
        "no dead-load factor (1)".into()
    }
}

/// Takes the counted self-weight into `model`: every member's self-weight
/// × `factor` (the members' own dead-load factors dropped: the counted
/// parts replace what they stood for).
pub fn apply_counted(model: &mut Model, factor: f64) {
    model.analysis.dead_load_factor = factor;
    model.analysis.dead_load_given = true;
    for m in &mut model.members {
        m.dead_load_factor = None;
    }
}
