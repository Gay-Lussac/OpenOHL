// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Two plates on one member end** (O44 L33, 2026-11-27; the owner's
//! words of that day: "if you have two plates, one on each side it's two
//! sided and if only one it's just one", then "Also make sure the joint
//! layout can have 2 plates if needed and shows them properly"; built, in
//! active development, not approved yet).
//!
//! A member end bolted onto a gusset plate (the **first plate**, its
//! `onto`) may get a **second plate** on the other face of its bolted
//! leg (`[plate.paired]`, structure file version 31): the end's bolts pass
//! through the first plate, the leg and the second plate — two shear
//! planes (`ohl_structures_fe::member_ends::worked_planes`). The second
//! plate is held to the first by **bolts of its own** through a **packing**
//! as thick as the leg, nearer the joint than the end's steel; without
//! them it would carry no force (one plane).
//!
//! - **The joint details** ([`detail_pairs`]): the second plate drawn on
//!   the leg's other face (its outline given, else round the end's bolts
//!   and its own), the end's bolts made longer by its thickness (their
//!   grip through both), its packing and its own bolts drawn
//!   ([`PairedDetail`]); in the 3D view, the Detail tab, the close-ups and
//!   the sketches as any plate, counted in the bill of materials.
//! - **Lay out joints** ([`after_run`], setting
//!   [`crate::LayoutSettings::two_plates`]): where an end laid onto a
//!   plate fails the code's check by the bolt shear at that end (the
//!   loaded plugin's capacity, with the forces of the file's own load
//!   cases or the table given), a second plate is planned — the first's
//!   thickness, round the end's bolts and its own, its own bolts on the
//!   end's gauge line nearer the joint (as many as half the end's force
//!   needs, two at least, by the code's force one bolt passes in the
//!   plate), inside the first plate with the code's distances, clear of
//!   the end's other leg — and kept only where the end's usage falls and
//!   the joint has no more clashes. Every such end said in the run's
//!   notes.
//!
//! **The forces' split** (conservative, marked in the theory page): the
//! first plate is checked, as before, with the end's full force; the
//! second plate and its own bolts with half of it — the share each plane
//! takes in the codes' bolt shear rule that gives the two planes (ASCE
//! 10-97 4.1, 4.3.2; SIA 263 Table 16, per shear plane); no code read gives
//! another split. The second plate's own bolts through the packing are a
//! bolt group of their own in the joint checks (O49 stage 1c, 2026-11-27,
//! `joint_groups::paired_groups`), the packing's thickness sent to the
//! code's plugin (SIA 263 6.2.2.5's β_p).

use std::collections::BTreeSet;

use ohl_structure_loads::{LoadCaseRecipe, LoadTable};
use ohl_structures::bolts::BoltLibrary;
use ohl_structures::{LibraryItem as _, StructureType};
use toml_edit::{Array, DocumentMut, Item, Table, Value};

use crate::joints::{Arrival, PlateDetail};
use crate::layout_run::{LayoutRun, RunAsk};

/// The second plate of a member end lying between two plates, as drawn
/// (structure axes, m).
#[derive(Debug, Clone, PartialEq)]
pub struct PairedDetail {
    /// The first plate (the end's `onto`).
    pub first: String,
    /// The member whose end lies between the two.
    pub member: String,
    /// The packing's thickness, m.
    pub packing: f64,
    /// The packing's outline on the second plate's inner face (empty: no
    /// bolts of its own, no packing).
    pub packing_outline: Vec<[f64; 3]>,
    /// From that face through the packing (towards the first plate).
    pub packing_through: [f64; 3],
    /// Its own bolts, each on the first plate's face under the packing.
    pub bolts: Vec<[f64; 3]>,
    /// Their axis (towards their heads, on the second plate's side).
    pub bolt_axis: [f64; 3],
    /// What they clamp, m: the packing and the second plate, the first
    /// plate.
    pub grip: [f64; 2],
    /// Their diameter, m (none: not known).
    pub diameter: Option<f64>,
    /// The library bolt's drawn sizes (head across flats, head, nut).
    pub drawn: [Option<f64>; 3],
    /// Held by bolts of its own (only then two shear planes at the end).
    pub held: bool,
    /// Its own bolts are the first plate's leg bolts (O49 stage 1c, L33's
    /// room): one bolt group through the leg, the first plate, the packing
    /// and it — drawn once, their grip through all four.
    pub shared: bool,
}

/// Whether `q` is within a quarter of a diameter `d` of one of `pts`
/// across the bolts' `axis` (the same bolt seen from another face).
fn same_bolt(q: [f64; 3], pts: &[[f64; 3]], axis: [f64; 3], d: f64) -> bool {
    pts.iter().any(|x| {
        let v = sub(q, *x);
        let off = sub(v, scale(axis, dot(v, axis)));
        norm(off) <= (0.25 * d).max(0.002)
    })
}

/// Whether bolt point `q` (axis `axis`) at a joint is one of the bolts a
/// second plate there shares with its first plate's leg bolts (O49 stage
/// 1c): drawn once, as the second plate's own; it passes through that
/// plate.
pub fn shared_bolt(plates: &[PlateDetail], q: [f64; 3], axis: [f64; 3]) -> Option<&str> {
    plates.iter().find_map(|p| {
        let x = p.paired.as_ref()?;
        let d = x.diameter.unwrap_or(0.016);
        (x.shared && dot(axis, x.bolt_axis).abs() > 0.99 && same_bolt(q, &x.bolts, x.bolt_axis, d))
            .then_some(p.id.as_str())
    })
}

/// A point of a member end's bolted leg: along the member from the
/// joint's work point and across the leg from the heel, on the face the
/// leg lies on.
#[derive(Debug, Clone, Copy)]
struct Frame {
    origin: [f64; 3],
    along: [f64; 3],
    across: [f64; 3],
}

impl Frame {
    fn at(&self, s: f64, x: f64) -> [f64; 3] {
        add(
            add(self.origin, scale(self.along, s)),
            scale(self.across, x),
        )
    }
}

/// The frame of end `a`'s bolted leg from its first bolt (none: no bolt,
/// no angle profile, or a plate between a built-up member's angles).
fn frame(a: &Arrival) -> Option<Frame> {
    let p = a.profile.as_ref()?;
    p.angle?;
    let across = if a.leg_y { p.axes[1] } else { p.axes[2] };
    let (q, f) = (a.bolt_points.first()?, a.bolt_flat.first()?);
    let origin = sub(sub(*q, scale(a.direction, f[0])), scale(across, f[1]));
    Some(Frame {
        origin,
        along: a.direction,
        across,
    })
}

/// Point `q` (structure axes, m) in end `a`'s bolted leg's frame: along the
/// member from the joint's work point and across its leg from the heel —
/// as `[plate.paired] bolt_points` give a second plate's own bolts (none:
/// the end's bolts not placed).
pub fn leg_frame_point(a: &Arrival, q: [f64; 3]) -> Option<[f64; 2]> {
    let f = frame(a)?;
    let d = sub(q, f.origin);
    Some([dot(d, f.along), dot(d, f.across)])
}

/// A rectangle of the frame, `s` along and `x` across (each a range),
/// moved by `off`.
fn rect(f: &Frame, s: [f64; 2], x: [f64; 2], off: [f64; 3]) -> Vec<[f64; 3]> {
    [(s[0], x[0]), (s[1], x[0]), (s[1], x[1]), (s[0], x[1])]
        .into_iter()
        .map(|(a, b)| add(f.at(a, b), off))
        .collect()
}

/// The second plates of joint `joint` of `t` drawn (L33): each paired
/// plate's face on the other face of its end's leg (`through` away from
/// it), its outline (given, else round the end's bolts and its own, 1.5
/// diameters out), its member; the end's bolts' grip through it; its
/// packing and its own bolts. What cannot be drawn is said in its notes.
pub fn detail_pairs(
    t: &StructureType,
    joint: &str,
    arrivals: &mut [Arrival],
    plates: &mut [PlateDetail],
    bolts: &BoltLibrary,
) {
    for p in t.plates.iter().filter(|p| p.joint == joint) {
        let Some(q) = &p.paired else { continue };
        let Some(k2) = plates.iter().position(|x| x.id == p.id) else {
            continue;
        };
        let say = |plates: &mut [PlateDetail], w: String| plates[k2].notes.push(w);
        let Some(k1) = plates.iter().position(|x| x.id == q.plate) else {
            say(
                plates,
                format!("{}: its first plate {} is not here", p.id, q.plate),
            );
            continue;
        };
        let Some(ai) = arrivals.iter().position(|a| {
            a.member == q.member && a.onto.value.as_deref() == Some(q.plate.as_str())
        }) else {
            say(
                plates,
                format!(
                    "{}: {}'s end here does not bolt onto {}: not drawn as its second plate",
                    p.id, q.member, q.plate
                ),
            );
            continue;
        };
        let a = &arrivals[ai];
        let Some(f) = frame(a) else {
            say(
                plates,
                format!(
                    "{}: {}'s bolts are not placed: the second plate not drawn",
                    p.id, q.member
                ),
            );
            continue;
        };
        let axis = scale(a.bolt_axis, 1.0 / norm(a.bolt_axis).max(1e-12));
        let leg = a.grip[0];
        let packing = q.packing.map_or(leg, |l| l.si());
        // Its own bolts.
        let lib = q
            .bolts
            .as_ref()
            .and_then(|b| b.bolt.as_deref())
            .and_then(|id| bolts.get(id));
        let diameter = q
            .bolts
            .as_ref()
            .and_then(|b| b.diameter.map(|d| d.si()))
            .or(lib.map(|l| l.diameter.si()))
            .or(a.bolts.value.as_ref().map(|b| b.diameter));
        let own: Vec<[f64; 3]> = q
            .bolt_points
            .iter()
            .map(|b| f.at(b[0].si(), b[1].si()))
            .collect();
        let flat_of = |b: &[ohl_core::units::Length; 2]| [b[0].si(), b[1].si()];
        let own_flat: Vec<[f64; 2]> = q.bolt_points.iter().map(flat_of).collect();
        let off = scale(axis, leg);
        let margin = 1.5 * diameter.unwrap_or(0.016);
        let span = |pts: &[[f64; 2]], i: usize| {
            pts.iter().fold([f64::INFINITY, f64::NEG_INFINITY], |r, v| {
                [r[0].min(v[i]), r[1].max(v[i])]
            })
        };
        let all: Vec<[f64; 2]> = a
            .bolt_flat
            .iter()
            .copied()
            .chain(own_flat.clone())
            .collect();
        if plates[k2].outline.len() < 3 && !all.is_empty() {
            let (s, x) = (span(&all, 0), span(&all, 1));
            plates[k2].outline = rect(
                &f,
                [s[0] - margin, s[1] + margin],
                [x[0] - margin, x[1] + margin],
                off,
            );
        }
        plates[k2].through = Some(axis);
        if !plates[k2].members.contains(&q.member) {
            plates[k2].members.push(q.member.clone());
        }
        let t2 = plates[k2].thickness;
        let t1 = plates[k1].thickness;
        // (O49 stage 1c) Its own bolts the first plate's leg bolts: their
        // grip through the leg too (the holder's leg's thickness).
        let holders: Vec<(Vec<[f64; 3]>, f64)> = arrivals
            .iter()
            .filter(|h| h.member != q.member && h.onto.value.as_deref() == Some(q.plate.as_str()))
            .map(|h| (h.bolt_points.clone(), h.grip[0]))
            .chain(
                plates[k1]
                    .holding_ends()
                    .map(|h| (h.bolt_points.clone(), h.grip[0])),
            )
            .collect();
        let d_own = diameter.unwrap_or(0.016);
        let shared_leg = (!own.is_empty())
            .then(|| {
                own.iter()
                    .map(|b| {
                        holders
                            .iter()
                            .find(|(pts, _)| same_bolt(*b, pts, axis, d_own))
                            .map(|h| h.1)
                    })
                    .collect::<Option<Vec<f64>>>()
            })
            .flatten()
            .map(|v| v.into_iter().fold(0.0, f64::max));
        arrivals[ai].grip[0] += t2;
        // Its packing: the second plate's own outline where the end's leg
        // is not (O49 stage 1c) — towards the joint from the end's steel,
        // or (its own bolts beside the end's) past the leg's toe — so that
        // its edges keep the distances the plate's do.
        let packing_outline = if own_flat.is_empty() {
            Vec::new()
        } else {
            let (s, x) = (span(&own_flat, 0), span(&own_flat, 1));
            let start = crate::member_solids::steel_start(&arrivals[ai]) - 0.001;
            let width = arrivals[ai]
                .profile
                .as_ref()
                .and_then(|p| p.angle.map(|g| if arrivals[ai].leg_y { g.b } else { g.h }));
            let o2: Vec<[f64; 2]> = plates[k2]
                .outline
                .iter()
                .map(|q| {
                    let d = sub(sub(*q, off), f.origin);
                    [dot(d, f.along), dot(d, f.across)]
                })
                .collect();
            let (s2, x2) = (span(&o2, 0), span(&o2, 1));
            let known = o2.len() >= 3;
            match width {
                Some(w) if x[0] > w => {
                    let (sr, xr) = if known {
                        ([s2[0], s2[1]], [w + 0.001, x2[1]])
                    } else {
                        (
                            [s[0] - margin, s[1] + margin],
                            [(x[0] - margin).max(w + 0.001), x[1] + margin],
                        )
                    };
                    rect(&f, sr, xr, off)
                }
                _ if known && s2[0] < start => rect(&f, [s2[0], start], [x2[0], x2[1]], off),
                _ => rect(
                    &f,
                    [s[0] - margin, start.max(s[1] + margin)],
                    [x[0] - margin, x[1] + margin],
                    off,
                ),
            }
        };
        plates[k2].paired = Some(PairedDetail {
            first: q.plate.clone(),
            member: q.member.clone(),
            packing,
            packing_outline,
            packing_through: scale(axis, -1.0),
            bolts: own,
            bolt_axis: axis,
            grip: [packing + t2, t1 + shared_leg.unwrap_or(0.0)],
            diameter,
            drawn: lib.map_or([None; 3], |l| l.drawn()),
            held: q.held(),
            shared: shared_leg.is_some(),
        });
        if !q.held() {
            say(
                plates,
                format!(
                    "{}: no bolts of its own into {}: it carries no force, {}'s bolts in one \
                     shear plane",
                    p.id, q.plate, q.member
                ),
            );
        } else {
            say(
                plates,
                format!(
                    "{}: second plate of {} (L33), held by its {} bolt(s) through a {:.0} mm \
                     packing into {}: those bolts checked in the joint checks as a group of their \
                     own, for half of {}'s force, with the code's rule for the packing (O49 stage \
                     1c)",
                    p.id,
                    q.member,
                    q.bolt_points.len(),
                    packing * 1e3,
                    q.plate,
                    q.member
                ),
            );
        }
    }
}

/// The loads of a layout run's two-plate step (the table given on the
/// command line), instead of the file's own load cases.
pub struct Loads<'a> {
    /// The load table.
    pub table: &'a LoadTable,
    /// Its recipes.
    pub recipes: &'a [LoadCaseRecipe],
}

/// An end given a second plate by a run.
#[derive(Debug, Clone, PartialEq)]
pub struct PairedEnd {
    /// The member.
    pub member: String,
    /// The joint.
    pub joint: String,
    /// The first plate.
    pub first: String,
    /// The second plate.
    pub second: String,
    /// The member's force over its capacity at the end before and after.
    pub usage: [f64; 2],
    /// The second plate's own bolts.
    pub bolts: u32,
    /// Where they lie (O49 stage 1c).
    pub room: Room,
}

/// Forces over capacities of `model`'s members in the failing sense: per
/// member id (largest tension over the tension capacity, largest
/// compression over the compression capacity), each with what governs.
type Ratios = std::collections::HashMap<String, [(f64, String); 2]>;

/// The members' ratios with the joint details taken (the ends' planes in
/// them): forces `force` (per member id: largest tension, largest
/// compression over the case's strength), capacities by the code's plugin
/// without a load case.
fn ratios(
    text: &str,
    ask: &RunAsk<'_>,
    force: &std::collections::HashMap<String, [f64; 2]>,
) -> Result<Ratios, String> {
    let model = ohl_structures_fe::Model::from_structure_type_with_joint_details(
        text,
        ask.library,
        ask.bolts,
        None,
    )?;
    let model = ohl_member_check::find(model, None)?;
    let given = ohl_member_check::capacities(&model)?;
    let mut out = Ratios::new();
    for g in given {
        let Some(f) = force.get(&g.id) else { continue };
        let by = |name: &str, v: f64| {
            g.capacities
                .iter()
                .find(|c| c.0 == name)
                .filter(|c| c.1 > 0.0)
                .map_or((0.0, String::new()), |c| (v / c.1, c.2.clone()))
        };
        out.insert(g.id.clone(), [by("tension", f[0]), by("compression", f[1])]);
    }
    Ok(out)
}

/// The failing sense's ratio of `r` and whether the bolt shear at joint
/// `joint` governs it (a capacity named "bolt shear at <joint>", or
/// "bolt shear" with the ends alike).
fn failing(r: &[(f64, String); 2], joint: &str) -> (f64, bool) {
    let w = if r[0].0 >= r[1].0 { &r[0] } else { &r[1] };
    let here = w.1.contains(crate::sizing_bolts::BOLT_SHEAR)
        && (w.1.ends_with(&format!(" at {joint}")) || !w.1.contains(" at "));
    (w.0, here)
}

/// Lay out joints' two-plate step (L33) after a run: where an end laid
/// onto a plate fails the code's check by the bolt shear at that end, a
/// second plate held by its own bolts through a packing ([module
/// docs](self)), kept only where the end's usage falls and its joint has no
/// more clashes. `loads`: the table given, else the file's own load cases
/// (none: not weighed, said).
pub fn after_run(
    run: LayoutRun,
    ask: &RunAsk<'_>,
    loads: Option<&Loads<'_>>,
) -> Result<LayoutRun, String> {
    let mut run = pairs(run, ask, loads)?;
    // (O49 stage 1c) The joints whose bolts stay of several sizes: said.
    if let Some(w) =
        crate::bolt_sizes::mixed_note(&run.text, ask.library, ask.bolts, ask.joints.as_ref())
    {
        run.notes.push(w);
    }
    Ok(run)
}

/// [`after_run`]'s two-plate step.
fn pairs(
    mut run: LayoutRun,
    ask: &RunAsk<'_>,
    loads: Option<&Loads<'_>>,
) -> Result<LayoutRun, String> {
    if !ask.settings.two_plates || !run.text.contains("[[connection]]") {
        return Ok(run);
    }
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), &run.text)
        .map_err(|e| e.message)?;
    let asked = |j: &str| ask.joints.as_ref().is_none_or(|s| s.contains(j));
    let ends: Vec<(String, String, String)> = t
        .connections
        .iter()
        .filter(|c| asked(&c.joint) && c.leg != Some(ohl_structures::connections::BoltedLeg::Both))
        .filter_map(|c| {
            let o = c.onto.as_ref()?;
            let plate = t.plates.iter().find(|p| &p.id == o && p.joint == c.joint)?;
            let paired = t.plates.iter().any(|p| {
                p.paired
                    .as_ref()
                    .is_some_and(|q| q.member == c.member && q.plate == plate.id)
            });
            (!paired && c.bolts.is_some() && plate.paired.is_none())
                .then(|| (c.member.clone(), c.joint.clone(), plate.id.clone()))
        })
        .collect();
    if ends.is_empty() {
        return Ok(run);
    }
    // The forces: the table given, else the file's own load cases.
    let own;
    let (table, recipes) = match loads {
        Some(l) => (l.table, l.recipes),
        None => {
            let recipes = ohl_structure_loads::StructureLoadCases::from_structure(&t)?;
            let areas = crate::SectionAreas::new(&t, &run.text, None, ask.library);
            let table = ohl_structure_loads::evaluate_with_areas(
                &recipes,
                &t,
                &ohl_structure_loads::LineValues::default(),
                &areas,
            );
            own = (table, recipes);
            (&own.0, own.1.as_slice())
        }
    };
    let cases = crate::cases_to_run(table, recipes, None);
    if cases.is_empty() {
        run.notes.push(
            "two plates on one end (L33) not weighed: no load case (the file has none of its \
             own; the command line takes a table with --loads)"
                .into(),
        );
        return Ok(run);
    }
    let fe = crate::analyse_structure(
        &crate::RunInput {
            structure: &t,
            text: &run.text,
            dir: None,
            sections: ask.library,
            table,
            recipes,
            cases: &cases,
            extension: None,
            bolts: Some(ask.bolts),
            feet: None,
        },
        &ohl_structures_fe::default_budget(),
    )?;
    let mut force: std::collections::HashMap<String, [f64; 2]> = Default::default();
    for (c, k) in fe.cases.iter().zip(&cases) {
        let Some(Ok(r)) = &c.fe else { continue };
        let strength = if k.strength > 0.0 { k.strength } else { 1.0 };
        for m in &r.members {
            let e = force.entry(m.id.clone()).or_insert([0.0; 2]);
            if m.axial >= 0.0 {
                e[0] = e[0].max(m.axial / strength);
            } else {
                e[1] = e[1].max(-m.axial / strength);
            }
        }
    }
    let before = ratios(&run.text, ask, &force)?;
    let failing_here: Vec<&(String, String, String)> = ends
        .iter()
        .filter(|(m, j, _)| {
            before.get(m).is_some_and(|r| {
                let (u, bolts) = failing(r, j);
                u > 1.0 + 1e-12 && bolts
            })
        })
        .collect();
    if failing_here.is_empty() {
        run.notes.push(format!(
            "two plates on one end (L33): {} end(s) onto a plate weighed, none fails the code's \
             check by the bolt shear at it (one plate each)",
            ends.len()
        ));
        return Ok(run);
    }
    let model = crate::model_of(&t, &run.text, None, ask.library)?;
    let mut doc: DocumentMut = run.text.parse().map_err(|e| format!("not TOML: {e}"))?;
    let mut ids: BTreeSet<String> = t
        .plates
        .iter()
        .map(|p| p.id.clone())
        .chain(t.members.iter().map(|m| m.id.clone()))
        .collect();
    let mut not: Vec<String> = Vec::new();
    let mut keep: Vec<PairedEnd> = Vec::new();
    // (O49 stage 1c, the owner's answer of 2026-11-27) Where it may lie, in
    // order: its own bolts nearer the joint than the end's steel; the first
    // plate's leg bolts as its own where they fall within it; its own bolts
    // beside the end's, past the leg's toe. Each end the next where the one
    // before does not fit or does not measure better (one round each).
    let mut left: Vec<&(String, String, String)> = failing_here;
    let mut tried: std::collections::HashMap<(String, String), Vec<String>> = Default::default();
    for room in [Room::Toward, Room::Shared, Room::Beside] {
        if left.is_empty() {
            break;
        }
        let text0 = doc.to_string();
        let d0 = crate::joint_details(&text0, ask.library, ask.bolts)?;
        let mut planned: Vec<PairedEnd> = Vec::new();
        let mut next: Vec<&(String, String, String)> = Vec::new();
        for e in left {
            let (member, joint, first) = e;
            let Some(j) = d0.joint(joint) else { continue };
            let (Some(a), Some(g1)) = (
                j.arrivals.iter().find(|a| {
                    &a.member == member && a.onto.value.as_deref() == Some(first.as_str())
                }),
                j.plates.iter().find(|p| &p.id == first),
            ) else {
                continue;
            };
            match plan(a, g1, j, &model, &force, room) {
                Ok(p) => {
                    let mut id = format!("{first}b");
                    let mut k = 2;
                    while ids.contains(&id) {
                        id = format!("{first}b{k}");
                        k += 1;
                    }
                    ids.insert(id.clone());
                    write(&mut doc, &id, joint, g1, member, &p, a);
                    planned.push(PairedEnd {
                        member: member.clone(),
                        joint: joint.clone(),
                        first: first.clone(),
                        second: id,
                        usage: [failing(&before[member], joint).0, 0.0],
                        bolts: p.points.len() as u32,
                        room,
                    });
                }
                Err(w) => {
                    tried
                        .entry((member.clone(), joint.clone()))
                        .or_default()
                        .push(format!("{}: {w}", room.words()));
                    next.push(e);
                }
            }
        }
        if planned.is_empty() {
            left = next;
            continue;
        }
        ohl_structure_edit::edits::raise_version(
            &mut doc,
            i64::from(ohl_structures::connections::PLATE_PAIRED_VERSION),
        );
        let text = doc.to_string();
        let after = ratios(&text, ask, &force)?;
        let d1 = crate::joint_details(&text, ask.library, ask.bolts)?;
        let clashes = |d: &crate::JointDetails, j: &str| d.joint(j).map_or(0, |x| x.clashes.len());
        let mut back: Vec<String> = Vec::new();
        for mut p in planned {
            p.usage[1] = after
                .get(&p.member)
                .map_or(f64::INFINITY, |r| failing(r, &p.joint).0);
            if p.usage[1] < p.usage[0] && clashes(&d1, &p.joint) <= clashes(&d0, &p.joint) {
                keep.push(p);
            } else {
                back.push(p.second.clone());
                // The clashes it would bring, in words (the first three).
                let new: Vec<String> = d1
                    .joint(&p.joint)
                    .map(|x| {
                        let had: Vec<String> = d0
                            .joint(&p.joint)
                            .map(|y| y.clashes.iter().map(|c| c.words()).collect())
                            .unwrap_or_default();
                        x.clashes
                            .iter()
                            .map(|c| c.words())
                            .filter(|w| !had.contains(w))
                            .take(3)
                            .collect()
                    })
                    .unwrap_or_default();
                tried
                    .entry((p.member.clone(), p.joint.clone()))
                    .or_default()
                    .push(format!(
                        "{}: {:.0} % (before {:.0} %), clashes {} (before {}){}",
                        p.room.words(),
                        p.usage[1] * 100.0,
                        p.usage[0] * 100.0,
                        clashes(&d1, &p.joint),
                        clashes(&d0, &p.joint),
                        if new.is_empty() {
                            String::new()
                        } else {
                            format!(" — {}", new.join("; "))
                        }
                    ));
                if let Some(e) = left_of(&p, &ends) {
                    next.push(e);
                }
            }
        }
        if !back.is_empty()
            && let Some(arr) = doc.get_mut("plate").and_then(Item::as_array_of_tables_mut)
        {
            arr.retain(|t| {
                t.get("id")
                    .and_then(|v| v.as_str())
                    .is_none_or(|id| !back.iter().any(|b| b == id))
            });
        }
        left = next;
    }
    for e in &left {
        if let Some(w) = tried.get(&(e.0.clone(), e.1.clone())) {
            not.push(format!("{} at {}: {}", e.0, e.1, w.join("; ")));
        }
    }
    if keep.is_empty() {
        run.notes.push(format!(
            "two plates on one end (L33): {} end(s) failing by their bolt shear, none given a \
             second plate — {}",
            left.len(),
            not.join("; ")
        ));
        return Ok(run);
    }
    run.text = doc.to_string();
    run.notes.push(format!(
        "two plates on one end (L33): {} end(s) failing by their bolt shear given a second plate \
         on the other face of the leg (bolt shear in two planes): {}{}",
        keep.len(),
        keep.iter()
            .map(|p| format!(
                "{} at {} between {} and {} ({}, {} bolt(s)), {:.0} % to {:.0} %",
                p.member,
                p.joint,
                p.first,
                p.second,
                p.room.words(),
                p.bolts,
                p.usage[0] * 100.0,
                p.usage[1] * 100.0
            ))
            .collect::<Vec<_>>()
            .join("; "),
        if !not.is_empty() {
            format!("; not: {}", not.join("; "))
        } else {
            String::new()
        }
    ));
    Ok(run)
}

/// The end of `ends` a planned second plate `p` is for.
fn left_of<'a>(
    p: &PairedEnd,
    ends: &'a [(String, String, String)],
) -> Option<&'a (String, String, String)> {
    ends.iter()
        .find(|e| e.0 == p.member && e.1 == p.joint && e.2 == p.first)
}

/// Where a second plate's own bolts may lie (O49 stage 1c, the owner's
/// answer of 2026-11-27 to stage 1b's question 4), in the order tried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Room {
    /// Its own bolts on the end's gauge line nearer the joint than the
    /// end's steel (L33 as built first).
    Toward,
    /// The first plate's leg bolts falling within it as its own (one bolt
    /// group through the leg, the first plate, the packing and it).
    Shared,
    /// Its own bolts beside the end's, past the end's leg's toe (stopped
    /// short of the leg bolts).
    Beside,
}

impl Room {
    /// In words, for the run's notes.
    pub fn words(self) -> &'static str {
        match self {
            Room::Toward => "its own bolts nearer the joint",
            Room::Shared => "the first plate's leg bolts as its own",
            Room::Beside => "its own bolts beside the end's",
        }
    }
}

/// A second plate planned for end `a` on plate `g1`: its outline and its
/// own bolts (along, across on the end's leg).
struct Plan {
    outline: Vec<[f64; 3]>,
    points: Vec<[f64; 2]>,
    /// The bolts they are (the leg's, where it shares the first plate's leg
    /// bolts; none: the end's).
    bolts: Option<crate::joints::BoltSet>,
}

/// The second plate of end `a` on `g1` at joint `j` with its own bolts in
/// `room` ([module docs](self)); Err: why none fits there.
fn plan(
    a: &Arrival,
    g1: &PlateDetail,
    j: &crate::joints::JointDetail,
    model: &ohl_structures_fe::Model,
    force: &std::collections::HashMap<String, [f64; 2]>,
    room: Room,
) -> Result<Plan, String> {
    let f = frame(a).ok_or("its bolts are not placed")?;
    let p = a.profile.as_ref().ok_or("no profile")?;
    let g = p.angle.ok_or("not an angle")?;
    let k = model
        .members
        .iter()
        .position(|m| m.id == a.member)
        .ok_or("not in the model")?;
    let end = a.end.min(1);
    let rules = crate::member_view::rules_in(model, k, end, None, Some((g1.thickness, None)))
        .ok_or("no bolt rules for its bolts")?;
    let per_bolt = rules
        .force
        .value()
        .filter(|v| *v > 0.0)
        .ok_or("no code's force one bolt passes")?;
    let n = force.get(&a.member).map_or(0.0, |v| v[0].max(v[1]));
    let count = ((n / 2.0 / per_bolt) - 1e-9).ceil().max(2.0) as usize;
    let fold = |i: usize, lo: bool| {
        a.bolt_flat.iter().map(|q| q[i]).fold(
            if lo { f64::INFINITY } else { f64::NEG_INFINITY },
            |r, v| if lo { r.min(v) } else { r.max(v) },
        )
    };
    let (x, xs, s_lo_end, s_hi) = (fold(1, true), fold(1, false), fold(0, true), fold(0, false));
    let start = crate::member_solids::steel_start(a) - 0.001;
    let root = g.t + g.r + 0.001;
    let width = if a.leg_y { g.b } else { g.h };
    let axis = scale(a.bolt_axis, 1.0 / norm(a.bolt_axis).max(1e-12));
    let flat_of = |q: [f64; 3]| {
        let d = sub(q, f.origin);
        [dot(d, f.along), dot(d, f.across)]
    };
    // The first plate's own geometry, in its plane.
    let n1 = g1.through.ok_or("the first plate has no outline")?;
    if g1.outline.len() < 3 {
        return Err("the first plate has no outline".into());
    }
    let u0 = sub(g1.outline[1], g1.outline[0]);
    let u = scale(u0, 1.0 / norm(u0).max(1e-12));
    let v = cross(n1, u);
    let flat = |q: [f64; 3]| {
        let d = sub(q, g1.outline[0]);
        [dot(d, u), dot(d, v)]
    };
    let poly: Vec<[f64; 2]> = g1.outline.iter().map(|q| flat(*q)).collect();
    // Every other bolt through the first plate (where it lies on the end's
    // leg's plane, along and across), and whether it is one of the plate's
    // leg bolts on the plate's other face.
    let holders: Vec<String> = crate::joint_groups::plate_holders(j, &g1.id)
        .iter()
        .map(|h| h.member.clone())
        .collect();
    let mut others: Vec<([f64; 2], bool, String)> = Vec::new();
    for h in crate::joint_groups::onto_plate_ends(j, &g1.id) {
        if h.member == a.member {
            continue;
        }
        for q in &h.bolt_points {
            let behind = dot(sub(*q, f.origin), axis) < -0.5 * g1.thickness;
            others.push((
                flat_of(*q),
                behind && holders.contains(&h.member),
                h.member.clone(),
            ));
        }
    }
    let inside_rect = |q: [f64; 2], sr: [f64; 2], xr: [f64; 2], margin: f64| {
        q[0] > sr[0] - margin
            && q[0] < sr[1] + margin
            && q[1] > xr[0] - margin
            && q[1] < xr[1] + margin
    };
    let hole = a
        .bolts
        .value
        .as_ref()
        .map_or(0.018, |b| b.hole.unwrap_or(b.diameter + 0.002));
    let (points, sr, xr, bolts) = match room {
        Room::Toward => {
            let points: Vec<[f64; 2]> = (0..count)
                .map(|i| [start - rules.end - i as f64 * rules.spacing, x])
                .collect();
            let s_lo = points.last().map_or(start, |q| q[0]) - rules.end;
            if s_lo <= 0.0 {
                return Err(format!(
                    "its {count} bolt(s) of its own do not fit between the work point and its steel"
                ));
            }
            let (sr, xr) = ([s_lo, s_hi + rules.end], [x - rules.edge, xs + rules.edge]);
            if let Some(o) = others.iter().find(|o| inside_rect(o.0, sr, xr, hole / 2.0)) {
                return Err(format!(
                    "{}'s bolts{} fall within it",
                    o.2,
                    if o.1 {
                        " (the first plate's leg bolts)"
                    } else {
                        ""
                    }
                ));
            }
            (points, sr, xr, None)
        }
        Room::Shared => {
            // The leg bolts toward the joint, clear of the end's steel and
            // of its other leg, nearest the end first.
            let mut cand: Vec<&([f64; 2], bool, String)> = others
                .iter()
                .filter(|o| o.1 && o.0[0] <= start - rules.edge && o.0[1] >= root + rules.edge)
                .collect();
            cand.sort_by(|p, q| q.0[0].total_cmp(&p.0[0]));
            if cand.len() < count {
                return Err(format!(
                    "{} of the first plate's leg bolts within its reach, {count} needed",
                    cand.len()
                ));
            }
            let mut chosen: Vec<[f64; 2]> = cand[..count].iter().map(|o| o.0).collect();
            let rect_of = |c: &[[f64; 2]]| {
                let s0 = c.iter().map(|q| q[0]).fold(f64::INFINITY, f64::min);
                let x0 = c.iter().map(|q| q[1]).fold(x, f64::min);
                let x1 = c.iter().map(|q| q[1]).fold(xs, f64::max);
                (
                    [s0 - rules.end, s_hi + rules.end],
                    [x0 - rules.edge, x1 + rules.edge],
                )
            };
            let (mut sr, mut xr) = rect_of(&chosen);
            // Every other bolt within it: a leg bolt joins it (one bolt
            // group), another member's stops it.
            loop {
                let mut grew = false;
                for o in &others {
                    if chosen
                        .iter()
                        .any(|c| (c[0] - o.0[0]).hypot(c[1] - o.0[1]) < 1e-6)
                    {
                        continue;
                    }
                    if inside_rect(o.0, sr, xr, hole / 2.0) {
                        if !o.1 {
                            return Err(format!("{}'s bolts fall within it", o.2));
                        }
                        chosen.push(o.0);
                        grew = true;
                    }
                }
                if !grew {
                    break;
                }
                (sr, xr) = rect_of(&chosen);
            }
            let leg = crate::joint_groups::plate_holders(j, &g1.id)
                .iter()
                .find_map(|h| h.bolts.value.clone());
            (chosen, sr, xr, leg)
        }
        Room::Beside => {
            let x_own = width + 0.002 + rules.edge;
            let points: Vec<[f64; 2]> = (0..count)
                .map(|i| [s_hi - i as f64 * rules.spacing, x_own])
                .collect();
            let s_lo = points.iter().map(|q| q[0]).fold(s_lo_end, f64::min) - rules.end;
            if s_lo <= 0.0 {
                return Err(format!(
                    "its {count} bolt(s) beside the end's do not fit before the work point"
                ));
            }
            let (sr, xr) = (
                [s_lo, s_hi + rules.end],
                [x - rules.edge, x_own + rules.edge],
            );
            if let Some(o) = others.iter().find(|o| inside_rect(o.0, sr, xr, hole / 2.0)) {
                return Err(format!(
                    "{}'s bolts{} fall within it",
                    o.2,
                    if o.1 {
                        " (the first plate's leg bolts)"
                    } else {
                        ""
                    }
                ));
            }
            (points, sr, xr, None)
        }
    };
    // Clear of the end's other leg (its root) on that face.
    if xr[0] < root {
        return Err(format!(
            "a second plate would run into its other leg (from {:.0} mm across the leg, the leg \
             and root {:.0} mm)",
            xr[0] * 1e3,
            (g.t + g.r) * 1e3
        ));
    }
    // Its own bolts inside the first plate, the code's edge distance kept.
    for q in &points {
        let w = flat(f.at(q[0], q[1]));
        if !inside(&poly, w) || edge_distance(&poly, w) < rules.edge.min(rules.end) - 1e-9 {
            return Err(
                "its own bolts would not lie inside the first plate with the code's distances"
                    .into(),
            );
        }
    }
    // Not past the first plate's extent (along and across the end's leg):
    // cut back to it, every bolt through it keeping the code's distances
    // to its edges.
    let ext: Vec<[f64; 2]> = g1.outline.iter().map(|q| flat_of(*q)).collect();
    let lo = |i: usize| ext.iter().map(|q| q[i]).fold(f64::INFINITY, f64::min);
    let hi = |i: usize| ext.iter().map(|q| q[i]).fold(f64::NEG_INFINITY, f64::max);
    let sr = [sr[0].max(lo(0)), sr[1].min(hi(0))];
    let xr = [xr[0].max(lo(1)), xr[1].min(hi(1))];
    let keeps = |q: &[f64; 2]| {
        q[0] - sr[0] >= rules.end - 1e-9
            && sr[1] - q[0] >= rules.end - 1e-9
            && q[1] - xr[0] >= rules.edge - 1e-9
            && xr[1] - q[1] >= rules.edge - 1e-9
    };
    if !points.iter().chain(a.bolt_flat.iter()).all(keeps) {
        return Err(
            "it would reach past the first plate (the code's distances to its edges not kept \
             within it)"
                .into(),
        );
    }
    let off = scale(a.bolt_axis, a.grip[0] / norm(a.bolt_axis).max(1e-12));
    let outline = rect(&f, sr, xr, off);
    Ok(Plan {
        outline,
        points,
        bolts,
    })
}

/// The second plate `id` of `member`'s end at `joint` written into `doc`.
fn write(
    doc: &mut DocumentMut,
    id: &str,
    joint: &str,
    g1: &PlateDetail,
    member: &str,
    p: &Plan,
    a: &Arrival,
) {
    let mut x = Table::new();
    x.insert("id", toml_edit::value(id));
    x.insert("joint", toml_edit::value(joint));
    x.insert("thickness", toml_edit::value(crate::mm_text(g1.thickness)));
    if let Some(m) = &g1.material {
        x.insert("material", toml_edit::value(m.as_str()));
    }
    let mut q = Table::new();
    q.insert("plate", toml_edit::value(g1.id.as_str()));
    q.insert("member", toml_edit::value(member));
    q.insert("packing", toml_edit::value(crate::mm_text(a.grip[0])));
    let mut b = toml_edit::InlineTable::new();
    b.insert("count", (p.points.len() as i64).into());
    // Its own bolts: the joint's size (the owner, 2026-11-27: one bolt size
    // per joint) — the end's, or the leg's it shares.
    if let Some(set) = p.bolts.as_ref().or(a.bolts.value.as_ref()) {
        if let Some(id) = &set.bolt {
            b.insert("bolt", Value::from(id.as_str()));
        }
        b.insert("diameter", Value::from(crate::mm_text(set.diameter)));
        if let Some(h) = set.hole {
            b.insert("hole", Value::from(crate::mm_text(h)));
        }
        if let Some(g) = &set.grade {
            b.insert("grade", Value::from(g.as_str()));
        }
    }
    q.insert("bolts", Item::Value(Value::InlineTable(b)));
    let mut pts = Array::new();
    for v in &p.points {
        let mut e = Array::new();
        e.push(crate::mm_text(v[0]));
        e.push(crate::mm_text(v[1]));
        pts.push(Value::Array(e));
    }
    q.insert("bolt_points", Item::Value(Value::Array(pts)));
    x.insert("paired", Item::Table(q));
    if doc
        .get("plate")
        .and_then(Item::as_array_of_tables)
        .is_none()
    {
        doc.insert(
            "plate",
            Item::ArrayOfTables(toml_edit::ArrayOfTables::new()),
        );
    }
    doc["plate"]
        .as_array_of_tables_mut()
        .expect("made above")
        .push(x);
    let _ = ohl_structure_edit::connection_edits::set_plate_outline(doc, id, &p.outline);
}

/// Point `p` inside polygon `poly` (either turning).
fn inside(poly: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            c = !c;
        }
    }
    c
}

/// The distance from `p` to the nearest edge of `poly`.
fn edge_distance(poly: &[[f64; 2]], p: [f64; 2]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            let d = [b[0] - a[0], b[1] - a[1]];
            let l2 = (d[0] * d[0] + d[1] * d[1]).max(1e-18);
            let t = (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2).clamp(0.0, 1.0);
            let q = [a[0] + t * d[0] - p[0], a[1] + t * d[1] - p[1]];
            (q[0] * q[0] + q[1] * q[1]).sqrt()
        })
        .fold(f64::INFINITY, f64::min)
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
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

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
