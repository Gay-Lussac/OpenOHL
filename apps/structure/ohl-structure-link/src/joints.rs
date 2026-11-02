// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A tower's joints in detail (owner 2026-11-02, stages 1 and 2 of the
//! [joint details]): at each joint, the member ends that arrive — from
//! where (direction, face, angle to the member they bolt onto) and their
//! condition (how held, onto what, the leg bolted, the bolts and their
//! layout, eccentric, restrained) — each value with where it comes from:
//!
//! - **given** at the member end (`[[connection]]`);
//! - **the member's**: its `[member.design]`, the same at both ends;
//! - **worked out** from the geometry, the bolts or the leg bolted;
//! - **assumed**: what the checks assume when nothing is given (ASCE
//!   10-97: one leg bolted, the ends eccentric), or a value for the
//!   drawing only (a pitch, an end distance);
//! - **not given**: nothing, and nothing assumed.
//!
//! Plain data for the structure application; the checks do not read it
//! (stage 4). A structure below level 3 (no cross-sections, no model)
//! gets the arrivals without profiles, bolts or design data.
//!
//! [joint details]: https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/joint-details.md

use std::collections::BTreeMap;

use ohl_sections::{SectionLibrary, Shape};
use ohl_structures::connections::{BoltedLeg, ConnectionKind, TypeConnection};
use ohl_structures::{LibraryItem, StructureType};
use ohl_structures_fe::design::{ConnectedLeg, Ends, MemberDesign};
use ohl_structures_fe::model::Model;

/// Where a value comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Given at the member end (`[[connection]]`).
    Given,
    /// The member's design data (`[member.design]`), both ends alike.
    Member,
    /// Worked out from the geometry, the bolts or the leg bolted.
    WorkedOut,
    /// Assumed (as the checks assume, or for the drawing only).
    Assumed,
    /// Not given, nothing assumed.
    Missing,
}

impl Source {
    /// In words, for the tables.
    pub fn words(self) -> &'static str {
        match self {
            Source::Given => "given",
            Source::Member => "the member's",
            Source::WorkedOut => "worked out",
            Source::Assumed => "assumed",
            Source::Missing => "not given",
        }
    }
}

/// A value and where it comes from.
#[derive(Debug, Clone, PartialEq)]
pub struct Valued<T> {
    /// The value (none: not given).
    pub value: Option<T>,
    /// Where it comes from.
    pub source: Source,
}

impl<T> Valued<T> {
    fn of(value: T, source: Source) -> Self {
        Self {
            value: Some(value),
            source,
        }
    }

    fn none() -> Self {
        Self {
            value: None,
            source: Source::Missing,
        }
    }

    /// Given, the member's or worked out (not assumed, not missing).
    pub fn known(&self) -> bool {
        matches!(
            self.source,
            Source::Given | Source::Member | Source::WorkedOut
        )
    }
}

/// The bolts of a member end, SI.
#[derive(Debug, Clone, PartialEq)]
pub struct BoltSet {
    /// How many.
    pub count: u32,
    /// Diameter, m.
    pub diameter: f64,
    /// Hole, m (none: not given).
    pub hole: Option<f64>,
    /// Grade (none: not given).
    pub grade: Option<String>,
}

/// A member's profile for the drawings: its axes and its outline.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    /// Local axes x (first joint to second), y, z: unit vectors in the
    /// structure's axes (the member's reference and roll applied).
    pub axes: [[f64; 3]; 3],
    /// The outline in the section's plane (y, z), m, around the member's
    /// axis (the centroid).
    pub outline: Vec<[f64; 2]>,
    /// For an angle: the heel (y, z) and the legs' lengths along y and z
    /// and its thickness, m.
    pub angle: Option<Angle>,
    /// The section's name.
    pub section: String,
    /// The section's id.
    pub section_id: String,
}

/// An angle's geometry around its centroid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Angle {
    /// The heel (y, z), m.
    pub heel: [f64; 2],
    /// The leg along y, m.
    pub b: f64,
    /// The leg along z, m.
    pub h: f64,
    /// Thickness, m.
    pub t: f64,
}

/// One member end at a joint.
#[derive(Debug, Clone, PartialEq)]
pub struct Arrival {
    /// The member.
    pub member: String,
    /// Its end at the joint: 0 its first joint, 1 its second.
    pub end: usize,
    /// The joint at its other end.
    pub other: String,
    /// From the joint along the member, unit vector.
    pub direction: [f64; 3],
    /// Its length, m.
    pub length: f64,
    /// Its class (`leg`, `bracing`, `redundant`, `crossing`; the
    /// member's design data) or its group.
    pub class: Option<String>,
    /// Its member group.
    pub group: Option<String>,
    /// The face it lies in (`x+`, `x-`, `y+`, `y-`), `plan` (level),
    /// `inside`: from its direction and where it is.
    pub face: String,
    /// The member its angle is measured against (the one it bolts onto,
    /// else the joint's leg) and the angle, degrees.
    pub angle: Option<(String, f64)>,
    /// How it is held.
    pub kind: Valued<ConnectionKind>,
    /// What it bolts onto.
    pub onto: Valued<String>,
    /// The angle's leg bolted.
    pub leg: Valued<BoltedLeg>,
    /// Its bolts.
    pub bolts: Valued<BoltSet>,
    /// Gauge lines.
    pub gauge_lines: Valued<u32>,
    /// The gauges, m.
    pub gauges: Valued<Vec<f64>>,
    /// Pitch, m.
    pub pitch: Valued<f64>,
    /// End distance, m.
    pub end_distance: Valued<f64>,
    /// Edge distance, m.
    pub edge_distance: Valued<f64>,
    /// Eccentric.
    pub eccentric: Valued<bool>,
    /// Held against turning.
    pub restrained: Valued<bool>,
    /// The eccentricity, m.
    pub eccentricity: Valued<f64>,
    /// Its profile (level 3).
    pub profile: Option<Profile>,
    /// The bolts where they are (structure's axes, m) and the bolts' axis
    /// (normal to the leg bolted): level 3 with bolts.
    pub bolt_points: Vec<[f64; 3]>,
    /// The bolts' axis.
    pub bolt_axis: [f64; 3],
    /// A modelling member (`check = false`).
    pub modelling: bool,
    /// Notes on it ("one end eccentric, which not said"…).
    pub notes: Vec<String>,
}

/// How complete a joint's details are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Completeness {
    /// Every member end's condition known (given, the member's, worked
    /// out).
    Complete,
    /// Some of it assumed.
    PartlyAssumed,
    /// No connection data at all (no connection, no member design data).
    Nothing,
}

impl Completeness {
    /// In words.
    pub fn words(self) -> &'static str {
        match self {
            Completeness::Complete => "complete",
            Completeness::PartlyAssumed => "partly assumed",
            Completeness::Nothing => "no connection data",
        }
    }
}

/// A gusset plate.
#[derive(Debug, Clone, PartialEq)]
pub struct PlateDetail {
    /// Id.
    pub id: String,
    /// Thickness, m.
    pub thickness: f64,
    /// Material.
    pub material: Option<String>,
    /// Its outline, structure's axes, m; given or around the bolts onto
    /// it (`outline_given` false).
    pub outline: Vec<[f64; 3]>,
    /// The outline given in the file.
    pub outline_given: bool,
    /// The members bolted onto it.
    pub members: Vec<String>,
}

/// A joint in detail.
#[derive(Debug, Clone, PartialEq)]
pub struct JointDetail {
    /// Id.
    pub id: String,
    /// Position, m.
    pub position: [f64; 3],
    /// The member ends arriving, in the file's member order.
    pub arrivals: Vec<Arrival>,
    /// Its plates.
    pub plates: Vec<PlateDetail>,
    /// How complete.
    pub completeness: Completeness,
}

/// Every joint of a structure in detail.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JointDetails {
    /// The joints, in the file's order.
    pub joints: Vec<JointDetail>,
    /// The structure is a full design (profiles, bolts, design data);
    /// false: the arrivals only.
    pub level3: bool,
    /// What could not be used (connections leading nowhere, the model
    /// not built…).
    pub notes: Vec<String>,
}

impl JointDetails {
    /// The joint `id`.
    pub fn joint(&self, id: &str) -> Option<&JointDetail> {
        self.joints.iter().find(|j| j.id == id)
    }
}

/// Every joint of the structure file `text` in detail; the profiles from
/// `library` (the structure's cross-section library).
pub fn joint_details(text: &str, library: &SectionLibrary) -> Result<JointDetails, String> {
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    let mut notes = t.connection_notes();
    // Level 3: the model (design data, sections) and the members' axes and
    // profiles as the exports take them.
    let (model, export) = match Model::from_structure_type_with(text, library) {
        Ok(m) if !m.members.is_empty() => {
            let e = crate::export_structure(text, library, None, &[]).ok();
            (Some(m), e)
        }
        Ok(_) => (None, None),
        Err(e) => {
            if t.members.iter().any(|m| m.section.is_some()) {
                notes.push(format!("the model is not built ({e}): the arrivals only"));
            }
            (None, None)
        }
    };
    let position: BTreeMap<&str, [f64; 3]> = t
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.point.map(|v| v.si())))
        .collect();
    let designs: BTreeMap<&str, (&MemberDesign, f64, bool)> = model
        .as_ref()
        .map(|m| {
            m.members
                .iter()
                .map(|x| {
                    (
                        x.id.as_str(),
                        (&x.design, m.sections[x.section].area, x.check),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let profiles: BTreeMap<&str, Profile> = export
        .as_ref()
        .map(|e| {
            e.members
                .iter()
                .map(|m| {
                    let s = &e.sections[m.section];
                    (m.id.as_str(), profile(m.axes, s))
                })
                .collect()
        })
        .unwrap_or_default();
    let mut joints = Vec::new();
    for n in &t.nodes {
        let at = position[n.id.as_str()];
        let mut arrivals = Vec::new();
        for m in &t.members {
            let Some(end) = m.nodes.iter().position(|x| *x == n.id) else {
                continue;
            };
            let other = &m.nodes[1 - end];
            let Some(&p) = position.get(other.as_str()) else {
                continue;
            };
            let d = sub(p, at);
            let length = norm(d);
            if length <= 0.0 {
                continue;
            }
            let design = designs.get(m.id.as_str());
            let class = design
                .and_then(|(d, _, _)| d.class)
                .map(|c| c.as_str().to_owned());
            arrivals.push(Arrival {
                member: m.id.clone(),
                end,
                other: other.clone(),
                direction: scale(d, 1.0 / length),
                length,
                class,
                group: m.group.clone(),
                face: face(at, scale(d, 1.0 / length), p),
                angle: None,
                kind: Valued::none(),
                onto: Valued::none(),
                leg: Valued::none(),
                bolts: Valued::none(),
                gauge_lines: Valued::none(),
                gauges: Valued::none(),
                pitch: Valued::none(),
                end_distance: Valued::none(),
                edge_distance: Valued::none(),
                eccentric: Valued::none(),
                restrained: Valued::none(),
                eccentricity: Valued::none(),
                profile: profiles.get(m.id.as_str()).cloned(),
                bolt_points: Vec::new(),
                bolt_axis: [0.0, 0.0, 1.0],
                modelling: design.is_some_and(|(_, _, check)| !check),
                notes: Vec::new(),
            });
        }
        let plates_here: Vec<&ohl_structures::connections::TypePlate> =
            t.plates.iter().filter(|p| p.joint == n.id).collect();
        let level3 = model.is_some();
        let areas: BTreeMap<String, f64> = arrivals
            .iter()
            .filter_map(|a| {
                designs
                    .get(a.member.as_str())
                    .map(|d| (a.member.clone(), d.1))
            })
            .collect();
        let snapshot = arrivals.clone();
        for a in &mut arrivals {
            let c = t.connection(&a.member, &n.id);
            let design = designs.get(a.member.as_str()).map(|d| d.0);
            condition(a, c, design, &snapshot, &areas, &plates_here, level3);
            if level3 {
                place_bolts(a, at, c, &snapshot);
            }
        }
        let plates = plates_here
            .iter()
            .map(|p| plate(p, at, &arrivals))
            .collect();
        let completeness = completeness(&arrivals, level3);
        joints.push(JointDetail {
            id: n.id.clone(),
            position: at,
            arrivals,
            plates,
            completeness,
        });
    }
    Ok(JointDetails {
        joints,
        level3: model.is_some(),
        notes,
    })
}

/// The condition of `a` from its connection `c`, its member's design data
/// and the other arrivals (for what it bolts onto).
fn condition(
    a: &mut Arrival,
    c: Option<&TypeConnection>,
    design: Option<&MemberDesign>,
    all: &[Arrival],
    areas: &BTreeMap<String, f64>,
    plates: &[&ohl_structures::connections::TypePlate],
    level3: bool,
) {
    use Source::*;
    let si = |l: &ohl_core::units::Length| l.si();
    // How held.
    a.kind = match c.and_then(|c| c.kind) {
        Some(k) => Valued::of(k, Given),
        None if c.and_then(|c| c.bolts.as_ref()).is_some()
            || design.and_then(|d| d.bolts).is_some() =>
        {
            Valued::of(ConnectionKind::Bolted, WorkedOut)
        }
        None if level3 => Valued::of(ConnectionKind::Bolted, Assumed),
        None => Valued::none(),
    };
    // Onto what: given, else the joint's leg.
    a.onto = match c.and_then(|c| c.onto.clone()) {
        Some(o) => Valued::of(o, Given),
        None => match leg_of(a, all, areas) {
            Some(l) => Valued::of(l, WorkedOut),
            None => Valued::none(),
        },
    };
    if let Some(o) = a.onto.value.clone() {
        if let Some(b) = all.iter().find(|b| b.member == o) {
            a.angle = Some((o, angle_between(a.direction, b.direction)));
        } else if plates.iter().any(|p| p.id == o) {
            // Onto a plate: the angle to the joint's leg.
            if let Some(l) = leg_of(a, all, areas)
                && let Some(b) = all.iter().find(|b| b.member == l)
            {
                a.angle = Some((l, angle_between(a.direction, b.direction)));
            }
        }
    }
    let is_angle = a.profile.as_ref().is_some_and(|p| p.angle.is_some());
    // The leg bolted.
    a.leg = match (c.and_then(|c| c.leg), design.and_then(|d| d.connected_leg)) {
        (Some(l), _) => Valued::of(l, Given),
        (None, Some(l)) => Valued::of(bolted_leg(l), Member),
        // ASCE 10-97's check assumes one leg bolted.
        (None, None) if is_angle => Valued::of(BoltedLeg::Long, Assumed),
        _ => Valued::none(),
    };
    // The bolts.
    a.bolts = match (
        c.and_then(|c| c.bolts.as_ref()),
        design.and_then(|d| d.bolts),
    ) {
        (Some(b), _) => Valued::of(
            BoltSet {
                count: b.count,
                diameter: b.diameter.si(),
                hole: b.hole.map(|h| h.si()),
                grade: b.grade.clone(),
            },
            Given,
        ),
        (None, Some(b)) => Valued::of(
            BoltSet {
                count: b.count,
                diameter: b.diameter,
                hole: b.hole,
                grade: None,
            },
            Member,
        ),
        _ => Valued::none(),
    };
    let diameter = a.bolts.value.as_ref().map(|b| b.diameter);
    let count = a.bolts.value.as_ref().map_or(0, |b| b.count);
    // The layout.
    let points_given = c.is_some_and(|c| !c.bolt_points.is_empty());
    a.gauge_lines = match c.and_then(|c| c.gauge_lines) {
        Some(n) => Valued::of(n, Given),
        None if points_given => Valued::of(1, WorkedOut),
        None if count > 0 => Valued::of(1, Assumed),
        None => Valued::none(),
    };
    let lines = a.gauge_lines.value.unwrap_or(1).max(1) as usize;
    let leg_width = a
        .profile
        .as_ref()
        .and_then(|p| p.angle)
        .map(|g| bolted_width(g, a.leg.value));
    a.gauges = match c.filter(|c| !c.gauges.is_empty()) {
        Some(c) => Valued::of(c.gauges.iter().map(si).collect(), Given),
        None => match leg_width {
            // For the drawing: the leg divided evenly.
            Some(w) if count > 0 => Valued::of(
                (1..=lines)
                    .map(|k| w * k as f64 / (lines as f64 + 1.0))
                    .collect(),
                Assumed,
            ),
            _ => Valued::none(),
        },
    };
    a.pitch = match (c.and_then(|c| c.pitch), design.and_then(|d| d.spacing)) {
        (Some(p), _) => Valued::of(p.si(), Given),
        (None, Some(p)) => Valued::of(p, Member),
        // For the drawing only: three diameters.
        (None, None) => diameter.map_or(Valued::none(), |d| Valued::of(3.0 * d, Assumed)),
    };
    a.end_distance = match (
        c.and_then(|c| c.end_distance),
        design.and_then(|d| d.end_distance),
    ) {
        (Some(p), _) => Valued::of(p.si(), Given),
        (None, Some(p)) => Valued::of(p, Member),
        // For the drawing only: one and a half diameters.
        (None, None) => diameter.map_or(Valued::none(), |d| Valued::of(1.5 * d, Assumed)),
    };
    a.edge_distance = match (
        c.and_then(|c| c.edge_distance),
        design.and_then(|d| d.edge_distance),
    ) {
        (Some(p), _) => Valued::of(p.si(), Given),
        (None, Some(p)) => Valued::of(p, Member),
        _ => Valued::none(),
    };
    // Eccentric: given; the member's (both or neither end); from the leg
    // bolted; else as the check assumes.
    let member_ends = |e: Option<Ends>| -> Option<Option<bool>> {
        match e? {
            Ends::None => Some(Some(false)),
            Ends::BothEnds => Some(Some(true)),
            Ends::OneEnd => Some(None),
        }
    };
    a.eccentric = match c.and_then(|c| c.eccentric) {
        Some(e) => Valued::of(e, Given),
        None => match member_ends(design.and_then(|d| d.end_eccentricity)) {
            Some(Some(e)) => Valued::of(e, Member),
            m => {
                if m.is_some() {
                    a.notes
                        .push("the member: one end eccentric, which not said".into());
                }
                match a.leg.value {
                    Some(BoltedLeg::Long | BoltedLeg::Short) if a.leg.known() => {
                        Valued::of(true, WorkedOut)
                    }
                    Some(BoltedLeg::Both) if a.leg.known() => Valued::of(false, WorkedOut),
                    _ if is_angle => Valued::of(true, Assumed),
                    _ => Valued::none(),
                }
            }
        },
    };
    // Restrained: given; the member's; two bolts or more into a stiffer
    // member or a plate; else not (as the check assumes).
    a.restrained = match c.and_then(|c| c.restrained) {
        Some(r) => Valued::of(r, Given),
        None => match member_ends(design.and_then(|d| d.end_restraint)) {
            Some(Some(r)) => Valued::of(r, Member),
            m => {
                if m.is_some() {
                    a.notes
                        .push("the member: one end restrained, which not said".into());
                }
                let onto_stiffer = a.onto.value.as_ref().is_some_and(|o| {
                    plates.iter().any(|p| &p.id == o)
                        || areas
                            .get(o)
                            .zip(areas.get(&a.member))
                            .is_some_and(|(onto, own)| onto > own)
                });
                match a.bolts.value.as_ref() {
                    Some(b) if a.bolts.known() && b.count >= 2 && onto_stiffer => {
                        Valued::of(true, WorkedOut)
                    }
                    Some(b) if a.bolts.known() && b.count <= 1 => Valued::of(false, WorkedOut),
                    _ if level3 => Valued::of(false, Assumed),
                    _ => Valued::none(),
                }
            }
        },
    };
    // The eccentricity: given, else from the gauge and the section.
    a.eccentricity = match c.and_then(|c| c.eccentricity) {
        Some(e) => Valued::of(e.si(), Given),
        None => match (a.profile.as_ref().and_then(|p| p.angle), &a.gauges.value) {
            (Some(g), Some(gauges)) if a.eccentric.value == Some(true) => {
                let (heel, along) = leg_frame(g, a.leg.value);
                let mean = gauges.iter().sum::<f64>() / gauges.len().max(1) as f64;
                let at = [heel[0] + along[0] * mean, heel[1] + along[1] * mean];
                let src = if a.gauges.known() && a.leg.known() {
                    WorkedOut
                } else {
                    Assumed
                };
                Valued::of((at[0] * at[0] + at[1] * at[1]).sqrt(), src)
            }
            _ => Valued::none(),
        },
    };
}

/// The member that is the joint's leg for `a`: the other arrivals of
/// class leg, else those within 30° of vertical; the largest section,
/// else the most nearly in line with `a` when `a` is one of them (a
/// splice), else the first.
fn leg_of(a: &Arrival, all: &[Arrival], areas: &BTreeMap<String, f64>) -> Option<String> {
    let others: Vec<&Arrival> = all.iter().filter(|b| b.member != a.member).collect();
    let mut legs: Vec<&Arrival> = others
        .iter()
        .copied()
        .filter(|b| b.class.as_deref() == Some("leg"))
        .collect();
    if legs.is_empty() {
        legs = others
            .iter()
            .copied()
            .filter(|b| b.direction[2].abs() > 30f64.to_radians().cos())
            .collect();
    }
    let a_is_leg = a.class.as_deref() == Some("leg")
        || (a.class.is_none() && a.direction[2].abs() > 30f64.to_radians().cos());
    if a_is_leg {
        // The leg continuing through the joint.
        return legs
            .iter()
            .max_by(|x, y| {
                let k = |b: &Arrival| -dot(a.direction, b.direction);
                k(x).total_cmp(&k(y))
            })
            .map(|b| b.member.clone());
    }
    legs.iter()
        .max_by(|x, y| {
            let k = |b: &Arrival| areas.get(&b.member).copied().unwrap_or(0.0);
            k(x).total_cmp(&k(y))
        })
        .map(|b| b.member.clone())
}

fn bolted_leg(l: ConnectedLeg) -> BoltedLeg {
    match l {
        ConnectedLeg::Both => BoltedLeg::Both,
        ConnectedLeg::Long => BoltedLeg::Long,
        ConnectedLeg::Short => BoltedLeg::Short,
        ConnectedLeg::None => BoltedLeg::None,
    }
}

/// Whether the leg bolted lies along the section's y (else z): the long
/// leg is the longer of b (y) and h (z); equal legs and "both": y.
fn along_y(g: Angle, leg: Option<BoltedLeg>) -> bool {
    match leg {
        Some(BoltedLeg::Short) => g.b < g.h,
        _ => g.b >= g.h,
    }
}

/// The width of the leg bolted, m.
fn bolted_width(g: Angle, leg: Option<BoltedLeg>) -> f64 {
    if along_y(g, leg) { g.b } else { g.h }
}

/// The heel and the direction along the leg bolted, in (y, z).
fn leg_frame(g: Angle, leg: Option<BoltedLeg>) -> ([f64; 2], [f64; 2]) {
    if along_y(g, leg) {
        (g.heel, [1.0, 0.0])
    } else {
        (g.heel, [0.0, 1.0])
    }
}

/// The bolts of `a` where they are: given points, else the layout (the
/// bolts shared among the gauge lines, from the end distance at the pitch).
fn place_bolts(a: &mut Arrival, joint: [f64; 3], c: Option<&TypeConnection>, _all: &[Arrival]) {
    let (Some(p), Some(b)) = (a.profile.as_ref(), a.bolts.value.as_ref()) else {
        return;
    };
    let Some(g) = p.angle else {
        return;
    };
    let along_y = along_y(g, a.leg.value);
    let (ey, ez) = (p.axes[1], p.axes[2]);
    // The bolts go through the leg bolted: across it, along the other axis.
    a.bolt_axis = if along_y { ez } else { ey };
    let at = |s: f64, across: f64| {
        let (y, z) = if along_y {
            (g.heel[0] + across, g.heel[1])
        } else {
            (g.heel[0], g.heel[1] + across)
        };
        let mut q = joint;
        for k in 0..3 {
            q[k] += a.direction[k] * s + ey[k] * y + ez[k] * z;
        }
        q
    };
    let given: Vec<[f64; 2]> = c
        .map(|c| {
            c.bolt_points
                .iter()
                .map(|p| [p[0].si(), p[1].si()])
                .collect()
        })
        .unwrap_or_default();
    a.bolt_points = if !given.is_empty() {
        given.iter().map(|q| at(q[0], q[1])).collect()
    } else {
        let gauges = a.gauges.value.clone().unwrap_or_default();
        let (Some(pitch), Some(end)) = (a.pitch.value, a.end_distance.value) else {
            return;
        };
        if gauges.is_empty() {
            return;
        }
        let lines = gauges.len() as u32;
        (0..b.count)
            .map(|k| {
                let line = (k % lines) as usize;
                let row = (k / lines) as f64;
                // Two lines staggered by half a pitch.
                let stagger = if lines == 2 && line == 1 {
                    pitch / 2.0
                } else {
                    0.0
                };
                at(end + row * pitch + stagger, gauges[line])
            })
            .collect()
    };
}

/// A plate's detail: its outline given, else the bolts onto it and the
/// joint, in the plane of the members onto it, 30 mm around (assumed).
fn plate(
    p: &ohl_structures::connections::TypePlate,
    joint: [f64; 3],
    arrivals: &[Arrival],
) -> PlateDetail {
    let onto: Vec<&Arrival> = arrivals
        .iter()
        .filter(|a| a.onto.value.as_deref() == Some(p.id.as_str()))
        .collect();
    let members = onto.iter().map(|a| a.member.clone()).collect();
    if !p.outline.is_empty() {
        return PlateDetail {
            id: p.id.clone(),
            thickness: p.thickness.si(),
            material: p.material.clone(),
            outline: p.outline.iter().map(|q| q.map(|v| v.si())).collect(),
            outline_given: true,
            members,
        };
    }
    let mut pts = vec![joint];
    for a in &onto {
        pts.extend(a.bolt_points.iter().copied());
        if a.bolt_points.is_empty() {
            pts.push(add(joint, scale(a.direction, 0.15)));
        }
    }
    let outline = hull_around(&pts, joint, &onto, 0.03);
    PlateDetail {
        id: p.id.clone(),
        thickness: p.thickness.si(),
        material: p.material.clone(),
        outline,
        outline_given: false,
        members,
    }
}

/// The convex hull of `pts` in the plane of the first two members' directions
/// through `joint`, grown by `margin`.
fn hull_around(pts: &[[f64; 3]], joint: [f64; 3], onto: &[&Arrival], margin: f64) -> Vec<[f64; 3]> {
    let u = onto.first().map_or([1.0, 0.0, 0.0], |a| a.direction);
    let mut n = onto
        .iter()
        .skip(1)
        .map(|b| cross(u, b.direction))
        .find(|c| norm(*c) > 1e-6)
        .unwrap_or_else(|| {
            cross(
                u,
                if u[2].abs() < 0.9 {
                    [0.0, 0.0, 1.0]
                } else {
                    [1.0, 0.0, 0.0]
                },
            )
        });
    n = scale(n, 1.0 / norm(n));
    let v = cross(n, u);
    let flat: Vec<[f64; 2]> = pts
        .iter()
        .map(|p| {
            let d = sub(*p, joint);
            [dot(d, u), dot(d, v)]
        })
        .collect();
    let mut grown = Vec::new();
    for p in &flat {
        for k in 0..8 {
            let t = k as f64 * std::f64::consts::FRAC_PI_4;
            grown.push([p[0] + margin * t.cos(), p[1] + margin * t.sin()]);
        }
    }
    crate::joint_sheet::hull(grown)
        .into_iter()
        .map(|q| add(joint, add(scale(u, q[0]), scale(v, q[1]))))
        .collect()
}

/// How complete the joint's details are: nothing when no end has any
/// connection data; partly assumed when any end's condition (held, leg,
/// bolts, eccentric, restrained) is assumed or missing at level 3.
fn completeness(arrivals: &[Arrival], level3: bool) -> Completeness {
    let any = arrivals.iter().any(|a| {
        [
            a.leg.source,
            a.bolts.source,
            a.eccentric.source,
            a.restrained.source,
        ]
        .iter()
        .any(|s| matches!(s, Source::Given | Source::Member))
            || a.kind.source == Source::Given
    });
    if !any {
        return Completeness::Nothing;
    }
    let complete = arrivals.iter().filter(|a| !a.modelling).all(|a| {
        let bolted = a.kind.value == Some(ConnectionKind::Bolted);
        a.kind.known()
            && a.eccentric.value.is_none_or(|_| a.eccentric.known())
            && a.restrained.value.is_none_or(|_| a.restrained.known())
            && (!bolted
                || !level3
                || (a.bolts.known() && a.leg.value.is_none_or(|_| a.leg.known())))
    });
    if complete {
        Completeness::Complete
    } else {
        Completeness::PartlyAssumed
    }
}

/// The face a member lies in: level → `plan`; else the axis its direction
/// barely runs along (under 0.35 of its length) with the side its far end
/// is on; else `inside`.
fn face(at: [f64; 3], d: [f64; 3], far: [f64; 3]) -> String {
    if d[2].abs() < 0.2 && (d[0].abs() > 0.2 && d[1].abs() > 0.2) {
        return "plan".into();
    }
    let mid = [(at[0] + far[0]) / 2.0, (at[1] + far[1]) / 2.0];
    let (k, name) = if d[0].abs() < d[1].abs() {
        (0, "x")
    } else {
        (1, "y")
    };
    if d[k].abs() < 0.35 && mid[k].abs() > 0.05 {
        format!("{name}{}", if mid[k] > 0.0 { "+" } else { "-" })
    } else if d[2].abs() < 0.2 {
        "plan".into()
    } else {
        "inside".into()
    }
}

/// A member's profile from its axes and section.
fn profile(axes: [[f64; 3]; 3], s: &crate::ExportSection) -> Profile {
    let [w, h] = s.size;
    // The lower left of the outline box from the centroid.
    let ll = [-(s.centroid[0] + w / 2.0), -(s.centroid[1] + h / 2.0)];
    let shift = |pts: Vec<[f64; 2]>| -> Vec<[f64; 2]> {
        pts.into_iter()
            .map(|p| [p[0] + ll[0], p[1] + ll[1]])
            .collect()
    };
    let (outline, angle) = match s.shape {
        Some(Shape::EqualAngle { h, t, .. }) | Some(Shape::UnequalAngle { h, t, .. }) => {
            let b = match s.shape {
                Some(Shape::UnequalAngle { b, .. }) => b,
                _ => h,
            };
            (
                shift(vec![[0.0, 0.0], [b, 0.0], [b, t], [t, t], [t, h], [0.0, h]]),
                Some(Angle { heel: ll, b, h, t }),
            )
        }
        Some(Shape::Channel { h, b, tw, tf, .. }) => (
            shift(vec![
                [0.0, 0.0],
                [b, 0.0],
                [b, tf],
                [tw, tf],
                [tw, h - tf],
                [b, h - tf],
                [b, h],
                [0.0, h],
            ]),
            None,
        ),
        Some(Shape::CircularHollow { d, .. }) | Some(Shape::RoundBar { d }) => (
            (0..16)
                .map(|k| {
                    let a = k as f64 * std::f64::consts::TAU / 16.0;
                    [d / 2.0 * a.cos(), d / 2.0 * a.sin()]
                })
                .collect(),
            None,
        ),
        _ if w > 0.0 && h > 0.0 => (shift(vec![[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]]), None),
        // Only values (a `[[section]]` table): a square of its area.
        _ => {
            let a = s.area.max(1e-6).sqrt() / 2.0;
            (vec![[-a, -a], [a, -a], [a, a], [-a, a]], None)
        }
    };
    Profile {
        axes,
        outline,
        angle,
        section: s.name.clone(),
        section_id: s.id.clone(),
    }
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

/// The angle between two members' lines, 0 to 90°.
fn angle_between(a: [f64; 3], b: [f64; 3]) -> f64 {
    dot(a, b).abs().clamp(0.0, 1.0).acos().to_degrees()
}
