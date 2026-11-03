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
//! Plain data for the structure application. Eccentric and restrained are
//! the check's own ([`ohl_structures_fe::member_ends`], 2026-11-02,
//! stage 4.1): what is shown is what is checked; what a member end bolts
//! onto is worked out by the same rule. A structure below level 3 (no cross-sections, no model)
//! gets the arrivals without profiles, bolts or design data.
//!
//! [joint details]: https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/joint-details.md

use std::collections::BTreeMap;

use ohl_sections::{SectionLibrary, Shape};
use ohl_structures::connections::{BoltedLeg, ConnectionKind, TypeConnection};
use ohl_structures::{LibraryItem, StructureType};
use ohl_structures_fe::design::{ConnectedLeg, MemberClass, MemberDesign};
use ohl_structures_fe::member_ends::{self, connected_leg};
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
    /// The bolt of a library it is (none: typed only).
    pub bolt: Option<String>,
    /// Drawn with: across the flats, the head's and the nut's heights, m
    /// (each the library bolt's when it gives them; else the
    /// [`BOLT_DRAWN`] proportions).
    pub drawn: [Option<f64>; 3],
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
    /// The bolts' axis: from the face the leg bolted lies on towards its
    /// own side (where the heads are).
    pub bolt_axis: [f64; 3],
    /// What the bolts clamp, m: the leg bolted's thickness and what it
    /// bolts onto's (a member's leg, a plate; unknown: the same again).
    pub grip: [f64; 2],
    /// The leg bolted lies along the section's y (else z): the long or
    /// short leg as given, else the one that lies flat against what it
    /// bolts onto.
    pub leg_y: bool,
    /// Where the member is drawn from its axis, m: shifted across so its
    /// leg bolted lies flat on the leg of the member it bolts onto (the
    /// model's axes meet at the joint; a real joint's legs lie back to
    /// back), and by its end's sideways `shift` (which the analysis takes
    /// as an eccentricity; the seat along the bolts is for the drawings
    /// only).
    pub offset: [f64; 3],
    /// Where its steel stops from the work point along it, m: given, else
    /// worked out so it fits (before the outstanding leg of the angle it
    /// bolts onto), else 0 (assumed). Its axis still runs to the work
    /// point (owner 2026-11-02).
    pub cut: Valued<f64>,
    /// Its end moved sideways to fit, m (given; none: 0), and the
    /// direction it is measured in ([`ohl_structures::connections::shift_direction`]).
    pub shift: (f64, Option<[f64; 3]>),
    /// Its bolts flat: along it from the joint and across its leg bolted
    /// from the heel, m (the same order as `bolt_points`).
    pub bolt_flat: Vec<[f64; 2]>,
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
pub fn joint_details(
    text: &str,
    library: &SectionLibrary,
    bolts: &ohl_structures::bolts::BoltLibrary,
) -> Result<JointDetails, String> {
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    let mut notes = t.connection_notes();
    // Level 3: the model (design data, sections) and the members' axes and
    // profiles as the exports take them.
    let (model, export) = match Model::from_structure_type_with_bolts(text, library, bolts, None) {
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
    // Each member's end conditions as the check takes them.
    let conditions: BTreeMap<&str, member_ends::EndConditions> = model
        .as_ref()
        .map(|m| {
            (0..m.members.len())
                .map(|k| (m.members[k].id.as_str(), member_ends::end_conditions(m, k)))
                .collect()
        })
        .unwrap_or_default();
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
                grip: [0.0, 0.0],
                leg_y: true,
                offset: [0.0; 3],
                cut: Valued::none(),
                shift: (0.0, None),
                bolt_flat: Vec::new(),
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
        let seats = plate_seats(&t, &n.id, &plates_here, &snapshot, &areas);
        for a in &mut arrivals {
            let c = t.connection(&a.member, &n.id);
            let design = designs.get(a.member.as_str()).map(|d| d.0);
            let ends = conditions.get(a.member.as_str());
            condition(
                a,
                c,
                design,
                &snapshot,
                &areas,
                &plates_here,
                level3,
                bolts,
                ends,
                &seats,
            );
            if level3 {
                place_bolts(a, at, c, &snapshot, &seats);
                a.grip = grip(a, &snapshot, &plates_here);
            }
        }
        let plates = plates_here
            .iter()
            .map(|p| plate(p, at, &arrivals, seats.get(&p.id)))
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
#[allow(clippy::too_many_arguments)]
fn condition(
    a: &mut Arrival,
    c: Option<&TypeConnection>,
    design: Option<&MemberDesign>,
    all: &[Arrival],
    areas: &BTreeMap<String, f64>,
    plates: &[&ohl_structures::connections::TypePlate],
    level3: bool,
    bolts: &ohl_structures::bolts::BoltLibrary,
    ends: Option<&member_ends::EndConditions>,
    seats: &PlateSeats,
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
    a.leg_y = leg_along_y(a, all, seats);
    // An assumed leg is the one that lies flat (the drawing's).
    if a.leg.source == Assumed
        && let Some(g) = a.profile.as_ref().and_then(|p| p.angle)
    {
        let (mine, other) = if a.leg_y { (g.b, g.h) } else { (g.h, g.b) };
        let leg = if mine >= other {
            BoltedLeg::Long
        } else {
            BoltedLeg::Short
        };
        a.leg = Valued::of(leg, Assumed);
    }
    // The bolts.
    a.bolts = match (
        c.and_then(|c| c.bolts.as_ref()),
        design.and_then(|d| d.bolts),
    ) {
        (Some(b), _) => {
            // The library's bolt, what is typed here overriding it.
            let lib = b.bolt.as_deref().and_then(|id| bolts.get(id));
            if let (Some(id), None) = (&b.bolt, lib) {
                a.notes.push(format!("bolt {id}: in no bolt library"));
            }
            match b.diameter.map(|d| d.si()).or(lib.map(|l| l.diameter.si())) {
                Some(diameter) => Valued::of(
                    BoltSet {
                        count: b.count,
                        diameter,
                        hole: b
                            .hole
                            .map(|h| h.si())
                            .or(lib.and_then(|l| l.hole.map(|h| h.si()))),
                        grade: b.grade.clone().or(lib.and_then(|l| l.grade.clone())),
                        bolt: b.bolt.clone(),
                        drawn: lib.map_or([None; 3], |l| l.drawn()),
                    },
                    Given,
                ),
                // No diameter (a bolt in no library): the member's bolts,
                // as the check takes them.
                None => member_bolts(design),
            }
        }
        (None, _) => member_bolts(design),
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
        .map(|g| if a.leg_y { g.b } else { g.h });
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
    // Eccentric and restrained: at level 3 as the check takes them
    // (`ohl_structures_fe::member_ends`: given; the member's; worked out at
    // an end with a connection; else assumed); below it, what is given here
    // and what the same rules work out from it.
    if let Some(e) = ends {
        a.eccentric = valued(&e.eccentric[a.end]);
        a.restrained = valued(&e.restrained[a.end]);
        for (c, what) in [
            (&e.eccentric[a.end], "eccentric"),
            (&e.restrained[a.end], "restrained"),
        ] {
            if c.value.is_none() {
                a.notes
                    .push(format!("the member: one end {what}, which not said"));
            }
        }
    } else {
        a.eccentric = match c.and_then(|c| c.eccentric) {
            Some(e) => Valued::of(e, Given),
            None => match a.leg.value.filter(|_| a.leg.known()) {
                Some(l) => member_ends::eccentric_from_leg(connected_leg(l))
                    .map_or(Valued::none(), |e| Valued::of(e, WorkedOut)),
                None => Valued::none(),
            },
        };
        let onto_plate = a
            .onto
            .value
            .as_ref()
            .is_some_and(|o| plates.iter().any(|p| &p.id == o));
        a.restrained = match c.and_then(|c| c.restrained) {
            Some(r) => Valued::of(r, Given),
            None => match a.bolts.value.as_ref().filter(|_| a.bolts.known()) {
                // Onto a gusset plate only: never restrained (ASCE 10-97
                // 3.7.4.5); onto a member: the check's stiffness rule.
                Some(b) if onto_plate && b.count >= 2 => Valued::of(false, WorkedOut),
                Some(b) => member_ends::restrained_from_bolts(b.count, false)
                    .map_or(Valued::none(), |r| Valued::of(r, WorkedOut)),
                None => Valued::none(),
            },
        };
    }
    // The eccentricity: given, else from the gauge and the section.
    a.eccentricity = match c.and_then(|c| c.eccentricity) {
        Some(e) => Valued::of(e.si(), Given),
        None => match (a.profile.as_ref().and_then(|p| p.angle), &a.gauges.value) {
            (Some(g), Some(gauges)) if a.eccentric.value == Some(true) => {
                let (heel, along) = (g.heel, if a.leg_y { [1.0, 0.0] } else { [0.0, 1.0] });
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

/// The member's own bolts (`[member.design] bolts`), if any.
fn member_bolts(design: Option<&MemberDesign>) -> Valued<BoltSet> {
    match design.and_then(|d| d.bolts) {
        Some(b) => Valued::of(
            BoltSet {
                count: b.count,
                diameter: b.diameter,
                hole: b.hole,
                grade: None,
                bolt: None,
                drawn: [None; 3],
            },
            Source::Member,
        ),
        None => Valued::none(),
    }
}

/// A condition of the check ([`member_ends::Condition`]) as shown.
fn valued(c: &member_ends::Condition) -> Valued<bool> {
    Valued {
        value: c.value,
        source: match c.source {
            member_ends::Source::Given => Source::Given,
            member_ends::Source::Member => Source::Member,
            member_ends::Source::WorkedOut => Source::WorkedOut,
            member_ends::Source::Assumed => Source::Assumed,
        },
    }
}

/// The member that is the joint's leg for `a`: the other arrivals of
/// class leg, else those within 30° of vertical; the largest section,
/// else the most nearly in line with `a` when `a` is one of them (a
/// splice), else the first.
fn leg_of(a: &Arrival, all: &[Arrival], areas: &BTreeMap<String, f64>) -> Option<String> {
    let list: Vec<member_ends::Arriving> = all
        .iter()
        .map(|b| member_ends::Arriving {
            direction: b.direction,
            class: b.class.as_deref().and_then(class_of),
            area: areas.get(&b.member).copied().unwrap_or(0.0),
        })
        .collect();
    let own = all.iter().position(|b| b.member == a.member)?;
    member_ends::joint_leg(own, &list).map(|k| all[k].member.clone())
}

/// A member class from its name.
fn class_of(name: &str) -> Option<MemberClass> {
    [
        MemberClass::Leg,
        MemberClass::Bracing,
        MemberClass::Redundant,
        MemberClass::Crossing,
    ]
    .into_iter()
    .find(|c| c.as_str() == name)
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

/// Whether `a`'s leg bolted lies along its section's y: the long or short
/// leg when given (the member's or here) and the legs differ; else the leg
/// that lies flat against a leg of the member it bolts onto (its normal
/// most nearly that leg's); else the long leg.
fn leg_along_y(a: &Arrival, all: &[Arrival], seats: &PlateSeats) -> bool {
    let Some(p) = a.profile.as_ref() else {
        return true;
    };
    let Some(g) = p.angle else {
        return true;
    };
    let sized = matches!(a.leg.value, Some(BoltedLeg::Long | BoltedLeg::Short))
        && a.leg.known()
        && (g.b - g.h).abs() > 1e-9;
    if sized {
        return along_y(g, a.leg.value);
    }
    let onto = a
        .onto
        .value
        .as_ref()
        .and_then(|o| all.iter().find(|b| &b.member == o))
        .and_then(|b| b.profile.as_ref());
    // Onto a plate: the leg lying in the plate's plane (its normal along
    // the plate's).
    if let Some(s) = a.onto.value.as_ref().and_then(|o| seats.get(o)) {
        return dot(p.axes[2], s.normal).abs() >= dot(p.axes[1], s.normal).abs();
    }
    match onto {
        Some(q) => {
            let flat = |n: [f64; 3]| dot(n, q.axes[1]).abs().max(dot(n, q.axes[2]).abs());
            // Along y: its normal is z; along z: y.
            flat(p.axes[2]) >= flat(p.axes[1])
        }
        None => along_y(g, a.leg.value),
    }
}

/// How a gusset plate lies at its joint: square to `normal` (the plane of
/// the members bolted onto it), on the flange of the joint's `leg` (an
/// angle) when there is one, `thickness` thick.
#[derive(Debug, Clone)]
struct PlateSeat {
    normal: [f64; 3],
    leg: Option<String>,
    thickness: f64,
}

/// The plates of a joint by id.
type PlateSeats = BTreeMap<String, PlateSeat>;

/// How each plate at joint `joint` lies: in the plane of the members
/// bolted onto it (their directions; one member: with the joint's leg; an
/// outline given: its own plane), on the joint's leg.
fn plate_seats(
    t: &StructureType,
    joint: &str,
    plates: &[&ohl_structures::connections::TypePlate],
    all: &[Arrival],
    areas: &BTreeMap<String, f64>,
) -> PlateSeats {
    let mut out = PlateSeats::new();
    for p in plates {
        let on: Vec<&Arrival> = t
            .connections
            .iter()
            .filter(|c| c.joint == joint && c.onto.as_deref() == Some(p.id.as_str()))
            .filter_map(|c| all.iter().find(|a| a.member == c.member))
            .collect();
        let leg = on.first().and_then(|a| leg_of(a, all, areas));
        let leg_dir = leg
            .as_ref()
            .and_then(|l| all.iter().find(|a| &a.member == l))
            .map(|a| a.direction);
        let given = (p.outline.len() >= 3).then(|| {
            let q: Vec<[f64; 3]> = p.outline.iter().map(|x| x.map(|v| v.si())).collect();
            cross(sub(q[1], q[0]), sub(q[2], q[0]))
        });
        let mut dirs: Vec<[f64; 3]> = on.iter().map(|a| a.direction).collect();
        dirs.extend(leg_dir);
        let spanned = dirs.iter().enumerate().find_map(|(k, d)| {
            dirs[k + 1..]
                .iter()
                .map(|e| cross(*d, *e))
                .find(|c| norm(*c) > 1e-3)
        });
        let Some(n) = given.filter(|c| norm(*c) > 1e-9).or(spanned) else {
            continue;
        };
        out.insert(
            p.id.clone(),
            PlateSeat {
                normal: scale(n, 1.0 / norm(n)),
                leg,
                thickness: p.thickness.si(),
            },
        );
    }
    out
}

/// The shift that seats `a`'s leg bolted on the face of the member it
/// bolts onto ([`Arrival::offset`]): along the bolts' axis, so the leg's
/// outer face (the heel's) lies on the onto leg's face turned towards it;
/// none when it bolts onto no angle or the legs are not near parallel
/// (within 25°).
fn seat(a: &Arrival, joint: [f64; 3], all: &[Arrival], seats: &PlateSeats) -> [f64; 3] {
    // Onto a plate: on the plate, which lies on the joint's leg (seated
    // onto the leg, then the plate's thickness out); no leg: the plate
    // through the work point.
    if let Some(s) = a.onto.value.as_ref().and_then(|o| seats.get(o)) {
        let Some(p) = a.profile.as_ref() else {
            return [0.0; 3];
        };
        let Some(g) = p.angle else {
            return [0.0; 3];
        };
        let nb = if a.leg_y { p.axes[2] } else { p.axes[1] };
        let on_leg = s.leg.as_ref().map(|l| {
            let mut b = a.clone();
            b.onto = Valued::of(l.clone(), Source::WorkedOut);
            seat(&b, joint, all, &PlateSeats::new())
        });
        return match on_leg {
            Some(o) if norm(o) > 0.0 => add(o, scale(nb, s.thickness)),
            _ => {
                let heel = add(scale(p.axes[1], g.heel[0]), scale(p.axes[2], g.heel[1]));
                scale(nb, -dot(heel, nb))
            }
        };
    }
    let (Some(p), Some(o)) = (
        a.profile.as_ref(),
        a.onto
            .value
            .as_ref()
            .and_then(|o| all.iter().find(|b| &b.member == o))
            .and_then(|b| b.profile.as_ref()),
    ) else {
        return [0.0; 3];
    };
    let (Some(g), Some(h)) = (p.angle, o.angle) else {
        return [0.0; 3];
    };
    // The leg bolted's normal, towards its own side (its outstanding leg).
    let nb = if a.leg_y { p.axes[2] } else { p.axes[1] };
    let heel = |q: &Profile, g: Angle| {
        add(
            joint,
            add(scale(q.axes[1], g.heel[0]), scale(q.axes[2], g.heel[1])),
        )
    };
    let own = dot(sub(heel(p, g), joint), nb);
    // The onto leg most nearly parallel: its normal from the heel into
    // its thickness (a leg along y: z).
    let (nl, cos) = [o.axes[2], o.axes[1]]
        .into_iter()
        .map(|n| (n, dot(n, nb)))
        .max_by(|x, y| x.1.abs().total_cmp(&y.1.abs()))
        .expect("two legs");
    if cos.abs() < 25f64.to_radians().cos() {
        return [0.0; 3];
    }
    // Its face turned towards `a`'s outer face (whose outward normal is
    // −nb): the inner one when the leg's normal goes along nb.
    let face = if cos > 0.0 {
        add(heel(o, h), scale(nl, h.t))
    } else {
        heel(o, h)
    };
    scale(nb, dot(sub(face, joint), nb) - own)
}

/// The bolts of `a` where they are: given points, else the layout (the
/// bolts shared among the gauge lines, from the end distance at the pitch).
fn place_bolts(
    a: &mut Arrival,
    joint: [f64; 3],
    c: Option<&TypeConnection>,
    all: &[Arrival],
    seats: &PlateSeats,
) {
    a.cut = match c.and_then(|c| c.cut) {
        Some(x) => Valued::of(x.si(), Source::Given),
        None => match fit_cut(a, all) {
            Some(x) => Valued::of(x, Source::WorkedOut),
            None => Valued::of(0.0, Source::Assumed),
        },
    };
    // The sideways shift, towards what it bolts onto (else the joint's
    // leg), as the analysis takes it.
    let towards = a
        .onto
        .value
        .as_ref()
        .and_then(|o| all.iter().find(|b| &b.member == o))
        .or_else(|| {
            leg_of(a, all, &Default::default()).and_then(|l| all.iter().find(|b| b.member == l))
        })
        .map(|b| b.direction);
    a.shift = (
        c.and_then(|c| c.shift).map_or(0.0, |s| s.si()),
        towards.and_then(|t| ohl_structures::connections::shift_direction(a.direction, t)),
    );
    if a.shift.0 != 0.0 && a.shift.1.is_none() {
        a.notes
            .push("shift: no direction (it runs in line with what it bolts onto)".into());
    }
    let cut = a.cut.value.unwrap_or(0.0);
    let (Some(p), Some(b)) = (a.profile.as_ref(), a.bolts.value.as_ref()) else {
        // No bolts: still drawn shifted.
        a.offset = scale(a.shift.1.unwrap_or([0.0; 3]), a.shift.0);
        return;
    };
    let Some(g) = p.angle else {
        return;
    };
    let along_y = a.leg_y;
    a.offset = add(
        seat(a, joint, all, seats),
        scale(a.shift.1.unwrap_or([0.0; 3]), a.shift.0),
    );
    let joint = add(joint, a.offset);
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
    let flat: Vec<[f64; 2]> = if !given.is_empty() {
        given
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
                // Every second line staggered by half a pitch (two lines:
                // the second; three: the middle one).
                let stagger = if line % 2 == 1 { pitch / 2.0 } else { 0.0 };
                // From the steel's end, where the member stops.
                [cut + end + row * pitch + stagger, gauges[line]]
            })
            .collect()
    };
    a.bolt_points = flat.iter().map(|q| at(q[0], q[1])).collect();
    a.bolt_flat = flat;
}

/// Where `a`'s steel must stop so it fits, m from the work point: its leg
/// bolted lies on the flange of the angle it bolts onto, so its end stays
/// on that flange's side of the other angle's heel (where its outstanding
/// leg stands) — in the face they bolt in. None: it bolts onto no angle,
/// runs in line with it, or is no angle.
fn fit_cut(a: &Arrival, all: &[Arrival]) -> Option<f64> {
    let p = a.profile.as_ref()?;
    let g = p.angle?;
    let o = a
        .onto
        .value
        .as_ref()
        .and_then(|o| all.iter().find(|b| &b.member == o))?;
    let q = o.profile.as_ref()?;
    let h = q.angle?;
    // A splice (in line): no cut.
    if angle_between(a.direction, o.direction) < 20.0 {
        return None;
    }
    let nb = if a.leg_y { p.axes[2] } else { p.axes[1] };
    let across = if a.leg_y { p.axes[1] } else { p.axes[2] };
    let w = if a.leg_y { g.b } else { g.h };
    // The onto angle's leg flat against ours (its normal most nearly
    // ours) and the direction across it, from its heel to its toe.
    let flange = if dot(q.axes[2], nb).abs() >= dot(q.axes[1], nb).abs() {
        q.axes[1]
    } else {
        q.axes[2]
    };
    let heel_o = add(scale(q.axes[1], h.heel[0]), scale(q.axes[2], h.heel[1]));
    let heel = add(
        add(scale(p.axes[1], g.heel[0]), scale(p.axes[2], g.heel[1])),
        scale(a.shift.1.unwrap_or([0.0; 3]), a.shift.0),
    );
    let rate = dot(a.direction, flange);
    if rate <= 1e-6 {
        return None;
    }
    // Each edge of our leg bolted (heel, toe): how far along it before it
    // is on the flange's side of the other's heel.
    let need = [heel, add(heel, scale(across, w))]
        .iter()
        .map(|e| -dot(sub(*e, heel_o), flange) / rate)
        .fold(0.0, f64::max);
    Some(need.min(a.length / 2.0))
}

/// A plate's detail: its outline given, else the bolts onto it and the
/// joint, in the plane of the members onto it, 30 mm around (assumed).
fn plate(
    p: &ohl_structures::connections::TypePlate,
    joint: [f64; 3],
    arrivals: &[Arrival],
    seat: Option<&PlateSeat>,
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
    // Its shape (owner 2026-11-02: "use the corner width it's bolted onto
    // and try to keep hidden in the shape of the corner … fitting"): each
    // member's leg bolted over its bolts (from its first bolt less its end
    // distance to its last plus it), and the joint's leg — the corner —
    // over its own width as far along it as those reach; the plate the
    // hull of them, in the face of the bolts (the exports lay it there too).
    let mut pts = vec![joint];
    for a in &onto {
        pts.extend(leg_strip(a, joint));
        if a.bolt_points.is_empty() {
            pts.push(add(joint, scale(a.direction, 0.15)));
        }
    }
    let leg = seat
        .and_then(|s| s.leg.as_ref())
        .and_then(|l| arrivals.iter().find(|a| &a.member == l));
    if let Some(l) = leg
        && let Some(g) = l.profile.as_ref().and_then(|p| p.angle)
    {
        // The corner's flange lying under the plate (its normal along the
        // bolts'), from its heel across its width, over the stretch along
        // the leg the members' strips reach (both ways from the joint).
        let n = onto.first().map_or([0.0, 0.0, 1.0], |a| a.bolt_axis);
        let p = l.profile.as_ref().expect("an angle");
        let across = if dot(p.axes[2], n).abs() >= dot(p.axes[1], n).abs() {
            (p.axes[1], g.b)
        } else {
            (p.axes[2], g.h)
        };
        let heel = add(
            add(joint, l.offset),
            add(scale(p.axes[1], g.heel[0]), scale(p.axes[2], g.heel[1])),
        );
        let reach = pts
            .iter()
            .map(|q| dot(sub(*q, joint), l.direction))
            .fold((0.0f64, 0.0f64), |(lo, hi), s| (lo.min(s), hi.max(s)));
        for s in [reach.0, reach.1] {
            let base = add(heel, scale(l.direction, s));
            pts.push(base);
            pts.push(add(base, scale(across.0, across.1)));
        }
    }
    let outline = match onto
        .iter()
        .find_map(|a| a.bolt_points.first().map(|q| (*q, a)))
    {
        Some((at, a)) => face_hull(&pts, at, a.bolt_axis, onto[0].direction, 0.0),
        None => hull_around(&pts, joint, &onto, 0.03),
    };
    PlateDetail {
        id: p.id.clone(),
        thickness: p.thickness.si(),
        material: p.material.clone(),
        outline,
        outline_given: false,
        members,
    }
}

/// The corners of `a`'s leg bolted over its bolts: from its first bolt
/// less its end distance to its last plus it (1.5 bolt diameters when not
/// given, for the drawing), across the leg's width (structure's axes).
fn leg_strip(a: &Arrival, joint: [f64; 3]) -> Vec<[f64; 3]> {
    let (Some(p), Some(b)) = (a.profile.as_ref(), a.bolts.value.as_ref()) else {
        return a.bolt_points.clone();
    };
    let Some(g) = p.angle else {
        return a.bolt_points.clone();
    };
    if a.bolt_flat.is_empty() {
        return Vec::new();
    }
    let margin = a.end_distance.value.unwrap_or(1.5 * b.diameter);
    let lo = a
        .bolt_flat
        .iter()
        .map(|q| q[0])
        .fold(f64::INFINITY, f64::min)
        - margin;
    let hi = a
        .bolt_flat
        .iter()
        .map(|q| q[0])
        .fold(f64::NEG_INFINITY, f64::max)
        + margin;
    let (across, w) = if a.leg_y {
        (p.axes[1], g.b)
    } else {
        (p.axes[2], g.h)
    };
    let heel = add(
        add(joint, a.offset),
        add(scale(p.axes[1], g.heel[0]), scale(p.axes[2], g.heel[1])),
    );
    let mut out = Vec::new();
    for s in [lo.max(0.0), hi] {
        let base = add(heel, scale(a.direction, s));
        out.push(base);
        out.push(add(base, scale(across, w)));
    }
    out
}

/// The convex hull of `pts` in the plane through `at` square to `normal`
/// (each point moved along `normal` into it), grown by `margin`; its x
/// along `along` as far as it lies in the plane.
pub(crate) fn face_hull(
    pts: &[[f64; 3]],
    at: [f64; 3],
    normal: [f64; 3],
    along: [f64; 3],
    margin: f64,
) -> Vec<[f64; 3]> {
    let n = normal.map(|v| v / dot(normal, normal).sqrt());
    let mut u = add(along, scale(n, -dot(along, n)));
    if dot(u, u) < 1e-12 {
        u = if n[2].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        u = add(u, scale(n, -dot(u, n)));
    }
    let u = u.map(|v| v / dot(u, u).sqrt());
    let v = cross(n, u);
    let mut grown = Vec::new();
    for p in pts {
        let d = sub(*p, at);
        let (x, y) = (dot(d, u), dot(d, v));
        for k in 0..8 {
            let t = f64::from(k) * std::f64::consts::FRAC_PI_4;
            grown.push([x + margin * t.cos(), y + margin * t.sin()]);
        }
    }
    crate::joint_sheet::hull(grown)
        .into_iter()
        .map(|q| add(add(at, scale(u, q[0])), scale(v, q[1])))
        .collect()
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
        // Built-up: the outer edge, the space between the angles filled
        // (each angle drawn: stage B3 of the built-up members).
        Some(ref sh) if sh.is_built_up() => {
            (shift(sh.built_up_outline().unwrap_or_default()), None)
        }
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

/// A joint like another, its member ends matched to the other's.
#[derive(Debug, Clone, PartialEq)]
pub struct Similar {
    /// The joint.
    pub joint: String,
    /// Each member end of the joint compared with, matched: (its member,
    /// this joint's member).
    pub pairs: Vec<(String, String)>,
}

/// The joints like `id` (stage 3 of the joint details: its details
/// copied to them): as many member ends, matched one to one by
/// cross-section, class and angle to what they bolt onto (within 1°),
/// so mirror joints count; `id` itself left out.
pub fn similar_joints(d: &JointDetails, id: &str) -> Vec<Similar> {
    let Some(from) = d.joint(id) else {
        return Vec::new();
    };
    let key = |a: &Arrival| {
        (
            a.profile.as_ref().map(|p| p.section_id.clone()),
            a.class.clone(),
        )
    };
    let angle = |a: &Arrival| a.angle.as_ref().map(|x| x.1);
    let mut out = Vec::new();
    for j in &d.joints {
        if j.id == id || j.arrivals.len() != from.arrivals.len() || from.arrivals.is_empty() {
            continue;
        }
        let mut used = vec![false; j.arrivals.len()];
        let mut pairs = Vec::new();
        for a in &from.arrivals {
            let found = j.arrivals.iter().enumerate().position(|(k, b)| {
                !used[k]
                    && key(a) == key(b)
                    && match (angle(a), angle(b)) {
                        (Some(x), Some(y)) => (x - y).abs() <= 1.0,
                        (None, None) => true,
                        _ => false,
                    }
            });
            match found {
                Some(k) => {
                    used[k] = true;
                    pairs.push((a.member.clone(), j.arrivals[k].member.clone()));
                }
                None => break,
            }
        }
        if pairs.len() == from.arrivals.len() {
            out.push(Similar {
                joint: j.id.clone(),
                pairs,
            });
        }
    }
    out
}

/// What the bolts of `a` clamp: its leg's thickness and what it bolts
/// onto's.
fn grip(
    a: &Arrival,
    all: &[Arrival],
    plates: &[&ohl_structures::connections::TypePlate],
) -> [f64; 2] {
    let own = a
        .profile
        .as_ref()
        .and_then(|p| p.angle)
        .map_or(0.008, |g| g.t);
    let onto = a.onto.value.as_ref().and_then(|o| {
        plates
            .iter()
            .find(|p| &p.id == o)
            .map(|p| p.thickness.si())
            .or_else(|| {
                all.iter()
                    .find(|b| &b.member == o)
                    .and_then(|b| b.profile.as_ref())
                    .and_then(|p| p.angle)
                    .map(|g| g.t)
            })
    });
    [own, onto.unwrap_or(own)]
}

/// The part of a bolt a face belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoltPart {
    /// The hexagonal head, on the leg bolted's own side.
    Head,
    /// The round shank through both thicknesses.
    Shank,
    /// The hexagonal nut, beyond what the leg bolts onto.
    Nut,
}

/// The proportions a bolt is drawn with, in bolt diameters (for the
/// drawings only — no standard's dimensions): across the head's and the
/// nut's flats, the head's height, the nut's, the shank beyond the nut.
pub const BOLT_DRAWN: [f64; 4] = [1.5, 0.65, 0.8, 0.3];

/// The faces of a bolt of diameter `d` at `p` (on the face between the
/// two thicknesses `grip`), its axis `axis` towards the head, its sizes
/// `drawn` (the library bolt's, else [`BOLT_DRAWN`]): polygons in the
/// structure's axes, each with its part.
pub fn bolt_faces(
    p: [f64; 3],
    axis: [f64; 3],
    d: f64,
    grip: [f64; 2],
    drawn: [Option<f64>; 3],
) -> Vec<(Vec<[f64; 3]>, BoltPart)> {
    let [flats, head, nut, beyond] = BOLT_DRAWN;
    // The library bolt's sizes, else the proportions (in diameters).
    let flats = drawn[0].map_or(flats, |x| x / d);
    let head = drawn[1].map_or(head, |x| x / d);
    let nut = drawn[2].map_or(nut, |x| x / d);
    let n = scale(axis, 1.0 / norm(axis).max(1e-12));
    let u = {
        let t = if n[2].abs() < 0.9 {
            [0.0, 0.0, 1.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let c = cross(n, t);
        scale(c, 1.0 / norm(c))
    };
    let v = cross(n, u);
    // A prism of `sides` round the axis from `a0` to `a1` along it,
    // `r` its corners' radius.
    let prism = |sides: usize, r: f64, a0: f64, a1: f64, part: BoltPart| {
        let ring = |a: f64| -> Vec<[f64; 3]> {
            (0..sides)
                .map(|k| {
                    let t = k as f64 * std::f64::consts::TAU / sides as f64;
                    add(
                        add(p, scale(n, a)),
                        add(scale(u, r * t.cos()), scale(v, r * t.sin())),
                    )
                })
                .collect()
        };
        let (lo, hi) = (ring(a0), ring(a1));
        let mut out = vec![(lo.clone(), part), (hi.clone(), part)];
        for k in 0..sides {
            let k2 = (k + 1) % sides;
            out.push((vec![lo[k], lo[k2], hi[k2], hi[k]], part));
        }
        out
    };
    // Across the flats to the corners' radius.
    let hex = flats * d / 3f64.sqrt();
    let mut out = prism(6, hex, grip[0], grip[0] + head * d, BoltPart::Head);
    out.extend(prism(
        12,
        d / 2.0,
        -(grip[1] + nut * d + beyond * d),
        grip[0],
        BoltPart::Shank,
    ));
    out.extend(prism(6, hex, -(grip[1] + nut * d), -grip[1], BoltPart::Nut));
    out
}
