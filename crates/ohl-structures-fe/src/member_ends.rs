// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The conditions of a member's ends at its joints (owner 2026-11-02,
//! stage 4.1 of the [joint details]; built, in active development, not
//! approved yet): whether each end is **eccentric** and **restrained**
//! against turning, and where that comes from, in this order:
//!
//! 1. **given** at the end (`[[connection]] eccentric`, `restrained`);
//! 2. **the member's** design data (`end_eccentricity`, `end_restraint`:
//!    none or both ends; one end, which one not said, is the end the
//!    other end's given value leaves);
//! 3. **worked out** at an end with a `[[connection]]`, from what it (or
//!    the member) gives: one leg bolted → eccentric, both legs → not; two
//!    bolts or more onto a member whose stiffness I/L is at least the sum
//!    of those it restrains at the joint ([`restrains`], ASCE 10-97
//!    C3.7.4 (b)) → restrained; one bolt, or bolts onto a gusset plate only
//!    (3.7.4.5, C3.7.4 (c)) → not;
//! 4. **assumed** as the check assumes without data: eccentric, not
//!    restrained (on the safe side).
//!
//! An end without a `[[connection]]` is never worked out: a tower without
//! connection data keeps its members' flags and the assumptions (owner,
//! 2026-11-02: "yes and yes"). The two ends together give the [`Ends`]
//! a capacity method's curves read. The rules live here only: the
//! structure application shows what this module says
//! (`ohl_structure_link::joints`), so what is shown is what is checked.
//!
//! [joint details]: https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/joint-details.md

use ohl_structures::bolts::BoltLibrary;
use ohl_structures::connections::{BoltedLeg, TypeConnection, TypePlate};

use crate::design::{ConnectedLeg, EndBolts, EndDesign, Ends, MemberClass, Onto};
use crate::model::Model;

/// Where an end's condition comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Given at the member end (`[[connection]]`).
    Given,
    /// The member's design data (`[member.design]`).
    Member,
    /// Worked out from the end's connection.
    WorkedOut,
    /// Assumed on the safe side.
    Assumed,
}

impl Source {
    /// In words, for the results.
    pub fn words(self) -> &'static str {
        match self {
            Source::Given => "given",
            Source::Member => "design data",
            Source::WorkedOut => "worked out",
            Source::Assumed => "assumed",
        }
    }
}

/// One condition at one end.
#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    /// Its value (none: the member's "one end", which one not said).
    pub value: Option<bool>,
    /// Where it comes from.
    pub source: Source,
    /// Why, when worked out ("one leg bolted", "2 bolts onto L1").
    pub why: Option<String>,
}

/// The conditions of both ends of a member.
#[derive(Debug, Clone, PartialEq)]
pub struct EndConditions {
    /// Eccentric, at the first joint and at the second.
    pub eccentric: [Condition; 2],
    /// Held against turning, at each end.
    pub restrained: [Condition; 2],
    /// The two ends together, for the curves.
    pub eccentricity: Ends,
    /// The two ends together, for the curves.
    pub restraint: Ends,
    /// Each end has a `[[connection]]`.
    pub connected: [bool; 2],
}

/// Eccentric from the leg bolted: one leg yes, both no; none: no rule.
pub fn eccentric_from_leg(leg: ConnectedLeg) -> Option<bool> {
    match leg {
        ConnectedLeg::Long | ConnectedLeg::Short => Some(true),
        ConnectedLeg::Both => Some(false),
        ConnectedLeg::None => None,
    }
}

/// Restrained from the bolts: two or more onto something stiff enough
/// ([`restrains`]) yes, one (or none) no; none: no rule (two or more onto
/// nothing stiff enough). Bolts onto a gusset plate only never restrain
/// (ASCE 10-97 3.7.4.5): pass `false`.
pub fn restrained_from_bolts(count: u32, onto_stiffer: bool) -> Option<bool> {
    if count <= 1 {
        Some(false)
    } else if onto_stiffer {
        Some(true)
    } else {
        None
    }
}

/// A member arriving at a joint, for [`joint_leg`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arriving {
    /// From the joint along it, unit vector.
    pub direction: [f64; 3],
    /// Its class.
    pub class: Option<MemberClass>,
    /// Its section's area, m² (0: not known).
    pub area: f64,
}

/// The member that is the joint's leg for `all[own]` (what it bolts onto
/// when not given): the others of class leg, else those within 30° of
/// vertical; for a leg itself the one most nearly in line (a splice), for
/// any other the largest section (the last of equals).
pub fn joint_leg(own: usize, all: &[Arriving]) -> Option<usize> {
    let vertical = |b: &Arriving| b.direction[2].abs() > 30f64.to_radians().cos();
    let others: Vec<usize> = (0..all.len()).filter(|&k| k != own).collect();
    let mut legs: Vec<usize> = others
        .iter()
        .copied()
        .filter(|&k| all[k].class == Some(MemberClass::Leg))
        .collect();
    if legs.is_empty() {
        legs = others.into_iter().filter(|&k| vertical(&all[k])).collect();
    }
    let a = &all[own];
    let a_is_leg = a.class == Some(MemberClass::Leg) || (a.class.is_none() && vertical(a));
    if a_is_leg {
        let dot = |b: &Arriving| -(0..3).map(|i| a.direction[i] * b.direction[i]).sum::<f64>();
        return legs
            .into_iter()
            .max_by(|&x, &y| dot(&all[x]).total_cmp(&dot(&all[y])));
    }
    legs.into_iter()
        .max_by(|&x, &y| all[x].area.total_cmp(&all[y].area))
}

/// The members of `model` meeting at node `node`, in order, as
/// [`Arriving`]s.
pub(crate) fn arriving(model: &Model, node: usize) -> Vec<(usize, Arriving)> {
    let at = model.nodes[node].point;
    model
        .members
        .iter()
        .enumerate()
        .filter_map(|(k, m)| {
            let other = if m.i == node {
                m.j
            } else if m.j == node {
                m.i
            } else {
                return None;
            };
            let p = model.nodes[other].point;
            let d = [p[0] - at[0], p[1] - at[1], p[2] - at[2]];
            let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            (l > 0.0).then(|| {
                (
                    k,
                    Arriving {
                        direction: d.map(|v| v / l),
                        class: m.design.class,
                        area: model.sections[m.section].area,
                    },
                )
            })
        })
        .collect()
}

/// Whether member `k` restrains the members bolted onto it at node `node`
/// (ASCE 10-97 C3.7.4 (b)): its stiffness I/L at least the sum of theirs —
/// every member at the joint whose end bolts onto `k` (given, else
/// worked out: [`onto`]), but not `k`'s continuation in line (a leg's
/// splice). I is each member's minor principal second moment: the
/// restraining member's least, on the safe side; L its length. With the
/// stiffness ratio, for the words.
pub fn restrains(model: &Model, k: usize, node: usize) -> (bool, f64) {
    let stiffness = |j: usize| {
        let m = &model.members[j];
        let l = model.length(m);
        if l > 0.0 {
            model.sections[m.section].i_min / l
        } else {
            0.0
        }
    };
    let all = arriving(model, node);
    let Some(dk) = all.iter().find(|(j, _)| *j == k).map(|(_, a)| a.direction) else {
        return (false, 0.0);
    };
    let restrained: f64 = all
        .iter()
        .filter(|(j, a)| {
            let in_line =
                (0..3).map(|i| a.direction[i] * dk[i]).sum::<f64>() < -30f64.to_radians().cos();
            let m = &model.members[*j];
            let end = if m.i == node { 0 } else { 1 };
            *j != k
                && !in_line
                && matches!(onto(model, *j, end), Some((Onto::Member(o), _)) if o == k)
        })
        .map(|(j, _)| stiffness(*j))
        .sum();
    if restrained <= 0.0 {
        return (false, 0.0);
    }
    let ratio = stiffness(k) / restrained;
    (ratio >= 1.0, ratio)
}

/// What end `end` (0 at its first joint, 1 at its second) of member
/// `idx` bolts onto: given, else the joint's leg ([`joint_leg`]), worked
/// out (true: given).
pub fn onto(model: &Model, idx: usize, end: usize) -> Option<(Onto, bool)> {
    if let Some(o) = model.members[idx]
        .design
        .ends
        .get(end)
        .and_then(Option::as_ref)
        .and_then(|e| e.onto.clone())
    {
        return Some((o, true));
    }
    let m = &model.members[idx];
    let node = if end == 0 { m.i } else { m.j };
    let all = arriving(model, node);
    let own = all.iter().position(|(k, _)| *k == idx)?;
    let list: Vec<Arriving> = all.iter().map(|(_, a)| *a).collect();
    joint_leg(own, &list).map(|k| (Onto::Member(all[k].0), false))
}

/// The conditions of both ends of member `idx`.
pub fn end_conditions(model: &Model, idx: usize) -> EndConditions {
    let m = &model.members[idx];
    let d = &m.design;
    let mut given_ecc = [None, None];
    let mut given_res = [None, None];
    let mut worked_ecc: [Option<(bool, String)>; 2] = [None, None];
    let mut worked_res: [Option<(bool, String)>; 2] = [None, None];
    for end in 0..2 {
        let Some(c) = &d.ends[end] else {
            continue;
        };
        given_ecc[end] = c.eccentric;
        given_res[end] = c.restrained;
        if let Some(leg) = c.leg.or(d.connected_leg) {
            worked_ecc[end] = eccentric_from_leg(leg).map(|v| {
                let why = if v {
                    "one leg bolted"
                } else {
                    "both legs bolted"
                };
                (v, why.to_owned())
            });
        }
        let count = c.bolts.map(|b| b.count).or(d.bolts.map(|b| b.count));
        if let Some(n) = count {
            let onto = onto(model, idx, end).map(|o| o.0);
            let node = if end == 0 { m.i } else { m.j };
            let (stiffer, what) = match &onto {
                // A gusset plate alone never restrains (3.7.4.5).
                Some(Onto::Plate { id, .. }) => (false, format!("plate {id} only")),
                Some(Onto::Member(k)) => {
                    let (yes, ratio) = restrains(model, *k, node);
                    (
                        yes,
                        format!(
                            "{} (I/L {ratio:.2} of those it holds)",
                            model.members[*k].id
                        ),
                    )
                }
                Some(Onto::Other(o)) => (false, o.clone()),
                None => (false, String::new()),
            };
            worked_res[end] =
                if n >= 2 && matches!(onto, Some(Onto::Plate { .. }) | Some(Onto::Member(_))) {
                    Some((stiffer, format!("{n} bolts onto {what}")))
                } else {
                    restrained_from_bolts(n, stiffer).map(|v| {
                        let why = match (n, v) {
                            (0, _) => "no bolts".to_owned(),
                            (1, _) => "one bolt".to_owned(),
                            _ => format!("{n} bolts onto {what}"),
                        };
                        (v, why)
                    })
                };
        }
    }
    let (eccentric, eccentricity) = resolve(given_ecc, d.end_eccentricity, worked_ecc, true);
    let (restrained, restraint) = resolve(given_res, d.end_restraint, worked_res, false);
    EndConditions {
        eccentric,
        restrained,
        eccentricity,
        restraint,
        connected: [d.ends[0].is_some(), d.ends[1].is_some()],
    }
}

/// One condition at both ends from what is given at each, the member's
/// flag and what is worked out; `assumed` when nothing says.
fn resolve(
    given: [Option<bool>; 2],
    member: Option<Ends>,
    worked: [Option<(bool, String)>; 2],
    assumed: bool,
) -> ([Condition; 2], Ends) {
    let one =
        |value: Option<bool>, source: Source, why: Option<String>| Condition { value, source, why };
    let mut out: [Condition; 2] = [0, 1].map(|end| match (given[end], member) {
        (Some(v), _) => one(Some(v), Source::Given, None),
        (None, Some(Ends::None)) => one(Some(false), Source::Member, None),
        (None, Some(Ends::BothEnds)) => one(Some(true), Source::Member, None),
        // Which end: below.
        (None, Some(Ends::OneEnd)) => one(None, Source::Member, None),
        (None, None) => match &worked[end] {
            Some((v, why)) => one(Some(*v), Source::WorkedOut, Some(why.clone())),
            None => one(Some(assumed), Source::Assumed, None),
        },
    });
    // The member's "one end": the end the other end's value leaves.
    for end in 0..2 {
        if out[end].value.is_none()
            && let Some(v) = out[1 - end].value
        {
            out[end].value = Some(!v);
        }
    }
    let ends = match out.iter().map(|c| c.value).collect::<Vec<_>>()[..] {
        [None, _] | [_, None] => Ends::OneEnd,
        [Some(a), Some(b)] => match (a, b) {
            (true, true) => Ends::BothEnds,
            (false, false) => Ends::None,
            _ => Ends::OneEnd,
        },
        _ => unreachable!("two ends"),
    };
    (out, ends)
}

/// The leg bolted as the design data name it.
pub fn connected_leg(l: BoltedLeg) -> ConnectedLeg {
    match l {
        BoltedLeg::Both => ConnectedLeg::Both,
        BoltedLeg::Long => ConnectedLeg::Long,
        BoltedLeg::Short => ConnectedLeg::Short,
        BoltedLeg::None => ConnectedLeg::None,
    }
}

/// Holes in one cross-section from a connection's layout: its gauge lines
/// (all in one section: staggering not counted, on the safe side), else
/// the bolt points across the leg; never more than its bolts (none: no
/// layout).
fn holes_of(c: &TypeConnection) -> Option<u32> {
    let count = c.bolts.as_ref().map(|b| b.count);
    let lines = match c.gauge_lines {
        Some(n) => Some(n),
        None if !c.bolt_points.is_empty() => {
            // Across the leg, to 0.1 mm.
            let mut across: Vec<i64> = c
                .bolt_points
                .iter()
                .map(|p| (p[1].si() * 1e4).round() as i64)
                .collect();
            across.sort_unstable();
            across.dedup();
            Some(across.len() as u32)
        }
        None => None,
    }?;
    Some(match count {
        Some(n) if n > 0 => lines.min(n),
        _ => lines,
    })
}

/// The `[[connection]]`s and `[[plate]]`s of a structure file's tables
/// into the members' [`EndDesign`]s of `model`, the bolts a connection
/// names from `bolts`. A connection leading nowhere is left out (the
/// structure file says so: `StructureType::connection_notes`).
pub(crate) fn read_connections(
    model: &mut Model,
    file: &toml::Table,
    bolts: &BoltLibrary,
) -> Result<(), String> {
    let list = |key: &str| {
        file.get(key)
            .and_then(toml::Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let plates: Vec<TypePlate> = list("plate")
        .into_iter()
        .map(|v| {
            v.try_into()
                .map_err(|e: toml::de::Error| format!("plate: {e}"))
        })
        .collect::<Result<_, _>>()?;
    for v in list("connection") {
        let c: TypeConnection = v
            .try_into()
            .map_err(|e: toml::de::Error| format!("connection: {e}"))?;
        let Some(idx) = model.members.iter().position(|m| m.id == c.member) else {
            continue;
        };
        let Some(node) = model.nodes.iter().position(|n| n.id == c.joint) else {
            continue;
        };
        let m = &model.members[idx];
        let end = if m.i == node {
            0
        } else if m.j == node {
            1
        } else {
            continue;
        };
        let mut e = EndDesign {
            kind: c.kind,
            leg: c.leg.map(connected_leg),
            holes: holes_of(&c),
            gauges: c.gauges.iter().map(|g| g.si()).collect(),
            edge_distance: c.edge_distance.map(|l| l.si()),
            rolled_edge: c.rolled_edge,
            end_distance: c.end_distance.map(|l| l.si()),
            pitch: c.pitch.map(|l| l.si()),
            eccentric: c.eccentric,
            restrained: c.restrained,
            shift: c.shift.map(|l| l.si()).filter(|s| *s != 0.0),
            ..EndDesign::default()
        };
        e.onto = c.onto.as_ref().map(|o| {
            let member = model.members.iter().position(|x| {
                &x.id == o && (model.nodes[x.i].id == c.joint || model.nodes[x.j].id == c.joint)
            });
            match member {
                Some(k) => Onto::Member(k),
                None => match plates.iter().find(|p| &p.id == o && p.joint == c.joint) {
                    Some(p) => Onto::Plate {
                        id: p.id.clone(),
                        thickness: p.thickness.si(),
                    },
                    None => Onto::Other(o.clone()),
                },
            }
        });
        if let Some(b) = &c.bolts {
            let lib = b.bolt.as_deref().and_then(|id| bolts.get(id));
            if let (Some(id), None) = (&b.bolt, lib) {
                e.notes.push(format!("bolt {id}: in no bolt library"));
            }
            e.bolt = b.bolt.clone();
            e.bolts = Some(EndBolts {
                count: b.count,
                diameter: b.diameter.or(lib.map(|l| l.diameter)).map(|l| l.si()),
                hole: b.hole.or(lib.and_then(|l| l.hole)).map(|l| l.si()),
                shear_planes: b.shear_planes,
                shear_capacity: lib.and_then(|l| l.shear_capacity).map(|f| f.si()),
                shear_capacity_shank: lib.and_then(|l| l.shear_capacity_shank).map(|f| f.si()),
                ultimate: lib.and_then(|l| l.ultimate).map(|s| s.si()),
                threads_in_shear: b.threads_in_shear,
                grade: lib
                    .and_then(|l| l.grade.as_deref())
                    .and_then(crate::design::Grade::parse),
            });
        }
        model.members[idx].design.ends[end] = Some(e);
    }
    crate::offsets::from_shifts(model);
    Ok(())
}
