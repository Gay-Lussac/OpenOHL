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
//! side). With it, since 2026-11-18, the **moments of those forces about
//! the weld** ([`weld_loads`]): each member's force acting along its line
//! through its bolts, about the weld's middle — the part bending the weld
//! along its length (in its plane) and the part about the seam's own line
//! — the larger of the two sides' (a ring: the sums of their sizes); a
//! weld shorter than the seam is taken anywhere along it (the moment's
//! largest). The plugin's joint check takes them (`joint-check-moments`;
//! SIA 263 6.3.1.7, 6.3.2.1); a plugin without that export: said NOT ON
//! THE SAFE SIDE on the weld.
//!
//! A member's axial force goes into the plate checks and, since
//! 2026-11-18, a frame member's **end shear and moments** at its bolt
//! group ([`bolt_loads`]): the shear in the plate's plane, the moment
//! about the plate's normal and the rest bending the plate out of its
//! plane, and the force square to it — sent to the plugin's joint check
//! with the part (`joint-check-moments`), which takes them on the safe
//! side.

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
    /// The plate's bolts onto the member(s) holding it, in words
    /// (2026-11-19, stage 4.5: `crate::joint_groups`).
    Holding(String),
    /// Every bolt onto the member at the joint: its bearing (2026-11-19);
    /// the row's `plate` is the member.
    Bearing,
    /// Something at the joint not checked, in words (2026-11-19).
    NotChecked(String),
}

impl PlatePart {
    /// In words: "D1's bolts", "weld to G2".
    pub fn words(&self) -> String {
        match self {
            PlatePart::Member(m) => format!("{m}'s bolts"),
            PlatePart::Weld(o) => format!("weld to {o}"),
            PlatePart::Holding(h) => format!("its bolts onto {h}"),
            PlatePart::Bearing => "the bolts onto it (its bearing)".to_owned(),
            PlatePart::NotChecked(_) => "not checked".to_owned(),
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
    /// It does not fit as given (owner 2026-11-19: a bolt off the flange
    /// or plate it bolts onto, holes in one ply overlapping), why in
    /// words: no usage, failing, counted apart (none: it fits).
    pub does_not_fit: Option<String>,
}

impl PlateUsage {
    /// The part with its plate or member, in words: "plate G1, D1's
    /// bolts", "plate G1, its bolts onto L1", "the bolts onto L1 (its
    /// bearing)".
    pub fn what(&self) -> String {
        match &self.part {
            PlatePart::Bearing => format!("the bolts onto {} (its bearing)", self.plate),
            PlatePart::NotChecked(_) => format!("{}, not checked", self.plate),
            p => format!("plate {}, {}", self.plate, p.words()),
        }
    }
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
        /// The plate's free length in compression ([`free_length`]), m.
        free: Option<f64>,
        /// Where the member's end and bolts are.
        at: BoltPlace,
    },
    Weld {
        with: String,
        input: WeldInput,
        /// The members on each side of the weld; one list (`ring`) when
        /// the welds close a ring.
        sides: [Vec<SideMember>; 2],
        ring: bool,
        /// Where the weld is.
        at: WeldPlace,
    },
    /// Two plates welded to each other that do not meet: no seam, not
    /// checked.
    NoSeam { with: String },
    /// A member's bolts not placed in a cover: not checked (said).
    Unplaced { member: String },
}

/// Where a member's end bolts onto a plate: which end, the plate's normal
/// (the bolts' axis), the member's direction in the plate from the joint
/// and how far its bolts' centroid lies along it from the joint (m).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BoltPlace {
    /// The member's end at the joint: 0 its first, 1 its second.
    pub end: usize,
    /// The plate's normal, unit.
    pub normal: [f64; 3],
    /// Along the member in the plate's plane, from the joint, unit.
    pub along: [f64; 3],
    /// From the joint to the bolts' centroid along it, m.
    pub lever: f64,
}

impl BoltPlace {
    fn of(a: &Arrival, j: &JointDetail) -> BoltPlace {
        let (along, _) = crate::joints::keep_axes(a);
        let n = a.bolt_points.len().max(1) as f64;
        let c = a
            .bolt_points
            .iter()
            .fold([0.0; 3], |s, q| add(s, scale(sub(*q, j.position), 1.0 / n)));
        BoltPlace {
            end: a.end,
            normal: a.bolt_axis,
            along,
            lever: dot(c, along).max(0.0),
        }
    }
}

/// Each member's end forces in one case, by id: at its first and second
/// end, on it, structure axes, `[F_x, F_y, F_z, M_x, M_y, M_z]` (N, N·m;
/// `MemberResult::end_forces`).
pub type EndForces = BTreeMap<String, [[f64; 6]; 2]>;

/// What a member's end forces `ef` put on its bolt group in a plate
/// beside its axial force (2026-11-18), `[shear, in-plane, out-of-plane,
/// normal]` (sizes, N and N·m), times the plate's `share`: the shear in
/// the plate's plane square to the member; the moment about the plate's
/// normal at the bolts' centroid, |M·n| + lever × shear (the end's moment
/// carried to the centroid, the sizes added: on the safe side); the
/// rest of the moment (bending the plate out of its plane, with the
/// member's torsion), |M − (M·n) n| + lever × the normal force; the force
/// square to the plate.
pub(crate) fn bolt_loads(at: &BoltPlace, ef: &[f64; 6], share: f64) -> [f64; 4] {
    let f = [ef[0], ef[1], ef[2]];
    let m = [ef[3], ef[4], ef[5]];
    let n = at.normal;
    let c = cross(n, at.along);
    let c = scale(c, 1.0 / norm(c).max(1e-12));
    let v = dot(f, c).abs();
    let vn = dot(f, n).abs();
    let mn = dot(m, n);
    let rest = norm(sub(m, scale(n, mn)));
    [
        v * share,
        (mn.abs() + at.lever * v) * share,
        (rest + at.lever * vn) * share,
        vn * share,
    ]
}

/// A member on one side of a weld: its id, its direction from the joint
/// (unit) and a point of its force's line (its bolts' centroid, else the
/// joint), structure axes.
pub(crate) type SideMember = (String, [f64; 3], [f64; 3]);

/// Where a weld is (structure axes): the seam's middle, its direction
/// (unit), its length and the weld's (m).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WeldPlace {
    /// The seam's middle.
    pub middle: [f64; 3],
    /// Along the seam, unit.
    pub along: [f64; 3],
    /// The seam's length.
    pub seam: f64,
    /// The weld's length (given, else the seam's).
    pub length: f64,
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
    /// The structure's member check is this plugin's method: the joints
    /// are checked by its `joint-check` export ([`plate_usage_cases`]),
    /// never by `method`.
    pub plugin: Option<ohl_structures_fe::model::PluginMethod>,
    /// The structure's name (for the plugin).
    structure: String,
    parts: Vec<Planned>,
    /// The joints' bolt groups and the members' bearing at the joints
    /// (2026-11-19, stage 4.5: `crate::joint_groups`).
    pub(crate) groups: crate::joint_groups::GroupPlans,
    /// What applies to every case.
    pub notes: Vec<String>,
}

impl PlatePlans {
    /// No plate and no joint bolt group to check.
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty() && self.groups.is_empty()
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
    plate_plans_in(text, library, bolts, None)
}

/// [`plate_plans`] of a structure whose file is in `dir`: its plugin's
/// joint check is then given the structure's parameter file for it
/// (`Plugins/<plugin>.toml` beside the structure, 2026-11-18), as the
/// member check is.
pub fn plate_plans_in(
    text: &str,
    library: &SectionLibrary,
    bolts: &BoltLibrary,
    dir: Option<&std::path::Path>,
) -> Result<PlatePlans, String> {
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    // The joints' own checks (2026-11-19): a tower's, at level 3 or more.
    let groups = joint_checks_apply(&t);
    if t.plates.is_empty() && t.splices.is_empty() && !groups {
        return Ok(PlatePlans::default());
    }
    let details = crate::joint_details(text, library, bolts)?;
    let Ok(mut model) = Model::from_structure_type_with_joint_details(text, library, bolts, None)
    else {
        return Ok(PlatePlans::default());
    };
    crate::plugin_check::with_parameters(&mut model, dir)?;
    let mut out = PlatePlans {
        method: model.analysis.capacity,
        plugin: model.capacity_plugin.clone(),
        structure: model.name.clone(),
        ..PlatePlans::default()
    };
    if out.method == CapacityMethod::Simple {
        out.notes.push(plates::SIMPLE_NOT_CHECKED.into());
    }
    if groups {
        if out.method == CapacityMethod::Simple {
            out.notes.push(
                "simple check: the joints' bolt groups and the members' bearing at the joints \
                 not checked (not a design code)"
                    .into(),
            );
        } else {
            out.groups = crate::joint_groups::group_plans(&details, &model, library, bolts);
        }
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
                // 2026-11-08): each plate checked for the member's whole
                // force (on the safe side: how the force splits between an
                // angle's two legs is not known; owner 2026-11-08, "stay on
                // the safe side").
                let whole = j
                    .arrivals
                    .iter()
                    .find(|x| x.member == a.member && x.end == a.end);
                if let Some(w) = whole.filter(|w| w.other_onto.is_some()) {
                    notes.push(format!(
                        "both legs bolted, onto {} and {}: each plate checked for the whole \
                         force (on the safe side: the split between the legs is not known)",
                        w.onto.value.as_deref().unwrap_or("?"),
                        w.other_onto.as_deref().unwrap_or("?"),
                    ));
                }
                let share = 1.0;
                let Some(group) = bolt_group(a, &p.id, &p.outline, j, &model, steel, &mut notes)
                else {
                    continue;
                };
                let free = free_length(a, &p.id, j, &group);
                if model
                    .members
                    .iter()
                    .find(|m| m.id == a.member)
                    .is_some_and(|m| m.is_frame())
                {
                    notes.push(
                        "a frame member: its end shear and moments at its bolts taken with its \
                         axial force (sizes added, on the safe side)"
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
                        free,
                        at: BoltPlace::of(a, j),
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
                     the welded plate, and the sum of their moments about it (on the safe side)"
                        .to_owned()
                } else {
                    "the weld's force: the larger resultant of the member forces on either side \
                     of it, with the larger moments of those forces about the weld (on the safe \
                     side)"
                        .to_owned()
                });
                if w.length.is_some_and(|l| l < w.seam_length() - 1e-6) {
                    notes.push(
                        "the weld shorter than the seam: taken where along it its moment is \
                         largest (on the safe side; where it is is not given)"
                            .to_owned(),
                    );
                }
                let at = WeldPlace {
                    middle: scale(add(w.ends[0], w.ends[1]), 0.5),
                    along: {
                        let d = sub(w.ends[1], w.ends[0]);
                        scale(d, 1.0 / norm(d).max(1e-12))
                    },
                    seam: w.seam_length(),
                    length: w.length(),
                };
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
                        at,
                    },
                    notes,
                });
            }
        }
    }
    Ok(out)
}

/// The joints' own checks apply to structure `t` (2026-11-19, stage 4.5):
/// a tower (lattice or portal) at level 3 or more with members.
pub(crate) fn joint_checks_apply(t: &StructureType) -> bool {
    use ohl_structures::structures::StructureKind;
    matches!(t.kind, StructureKind::Lattice | StructureKind::Portal)
        && t.detail().is_design()
        && !t.members.is_empty()
        && t.model.is_none()
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
                let free = free_length(a, &sp.id, j, &group);
                planned.push((name, group, steel, notes, width * c.thickness, free));
            }
            let total: f64 = planned.iter().map(|x| x.4).sum();
            for (name, group, steel, mut notes, area, free) in planned {
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
                        free,
                        at: BoltPlace::of(a, j),
                    },
                    notes,
                });
            }
        }
    }
}

/// A plate's free length in compression where member `a` bolts onto it
/// (`plate`: the plate's or splice's id; 2026-11-18): from the bolts
/// nearest the joint, along the member towards the joint, to the nearest
/// bolt of one of the joint's legs ([`crate::welds::joint_legs`]) bolted
/// onto the same plate that lies within the bolt group's spread (Whitmore's
/// width at 30°, about its gauge lines' middle) — what holds the plate
/// there —, else to the plate's edge that way (`g.end[1]`). On the safe
/// side: another member's bolts are not taken as holding it, the plate's
/// whole length to its edge where no leg's bolt is in the way. None: the
/// plate's edge not reached.
fn free_length(a: &Arrival, plate: &str, j: &JointDetail, g: &BoltGroup) -> Option<f64> {
    let to_edge = g.end[1]?;
    let (along, across) = crate::joints::keep_axes(a);
    let rel = |q: &[f64; 3]| sub(*q, j.position);
    let inner = a
        .bolt_points
        .iter()
        .map(|q| dot(rel(q), along))
        .fold(f64::INFINITY, f64::min);
    let xs: Vec<f64> = a.bolt_points.iter().map(|q| dot(rel(q), across)).collect();
    let mid = (xs.iter().copied().fold(f64::INFINITY, f64::min)
        + xs.iter().copied().fold(f64::NEG_INFINITY, f64::max))
        / 2.0;
    let rows = if g.per_line > 1 {
        f64::from(g.per_line - 1) * g.pitch.unwrap_or(0.0)
    } else {
        0.0
    };
    let half = g.span / 2.0 + rows * 30f64.to_radians().tan();
    let legs = crate::welds::joint_legs(j);
    let mut best = to_edge;
    for l in legs.iter().filter(|l| {
        l.member != a.member
            && (l.onto.value.as_deref() == Some(plate) || l.other_onto.as_deref() == Some(plate))
    }) {
        for q in &l.bolt_points {
            let ahead = inner - dot(rel(q), along);
            let off = (dot(rel(q), across) - mid).abs();
            if ahead > 1e-6 && off <= half + 1e-9 {
                best = best.min(ahead);
            }
        }
    }
    Some(best)
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

/// [`plate_steel`] for the joints' bolt groups (2026-11-19).
pub(crate) fn plate_steel_of(
    p: &PlateDetail,
    j: &JointDetail,
    model: &Model,
    library: &SectionLibrary,
) -> (PlateSteel, Option<String>) {
    plate_steel(p, j, model, library)
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
            crate::member_view::rules_in(model, k, a.end, None, Some((steel.thickness, steel.fu)))
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
/// the joint and a point of their force's line (their bolts' centroid,
/// else the joint); a ring (b reached from a without that weld): every
/// member of the welded plate on one side.
fn weld_sides(j: &JointDetail, a: &str, b: &str) -> ([Vec<SideMember>; 2], bool) {
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
    let members = |plates: &[String]| -> Vec<SideMember> {
        j.arrivals
            .iter()
            .filter(|x| {
                (x.onto.value.as_ref().is_some_and(|o| plates.contains(o))
                    || x.other_onto.as_ref().is_some_and(|o| plates.contains(o)))
                    && !x.modelling
            })
            .map(|x| {
                let point = if x.bolt_points.is_empty() {
                    j.position
                } else {
                    let n = x.bolt_points.len() as f64;
                    x.bolt_points
                        .iter()
                        .fold([0.0; 3], |s, q| add(s, scale(*q, 1.0 / n)))
                };
                (x.member.clone(), x.direction, point)
            })
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
    let mut rows: Vec<PlateUsage> = plans
        .parts
        .iter()
        .map(|x| {
            let (part, force, checked) = match &x.plan {
                Plan::Bolted {
                    member,
                    group,
                    steel,
                    share,
                    ..
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
                    ..
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
                does_not_fit: None,
            }
        })
        .collect();
    // The joints' bolt groups (2026-11-19): a design code's, its plugin
    // not run here — not checked, said.
    if !plans.groups.is_empty() {
        rows.extend(
            crate::joint_groups::group_rows(
                &plans.groups,
                &plans.structure,
                None,
                None,
                &[("", strength, axial)],
                &[],
            )
            .into_iter()
            .flatten(),
        );
    }
    rows
}

/// One load case for [`plate_usage_cases`]: its name, its strength
/// factor, each member's axial force (N, tension positive).
pub type PlateCase<'a> = (&'a str, f64, &'a BTreeMap<String, f64>);

/// The plate checks of every case of `cases`, one list per case: the
/// capacity method's own ([`plate_usage`]), or — the structure's member
/// check a plugin's method — that plugin's joint check, **one call with
/// every part and case** (`ohl_member_check::joints::check_joints`). A
/// plugin without a joint check, or that fails: every part is "NOT
/// CHECKED by the plugin, NOT ON THE SAFE SIDE", without a usage — never
/// ASCE 10-97 in its place; so is a part or a load case it returns
/// nothing for. What it flags as not safe is said with the part.
pub fn plate_usage_cases(plans: &PlatePlans, cases: &[PlateCase]) -> Vec<Vec<PlateUsage>> {
    plate_usage_cases_with(plans, cases, &[])
}

/// [`plate_usage_cases`] with each case's members' end forces (`ends`,
/// one per case of `cases`, or empty: none known): a frame member's end
/// shear and moments at its bolts go to the plugin with the part
/// (`bolt_loads`, 2026-11-18).
pub fn plate_usage_cases_with(
    plans: &PlatePlans,
    cases: &[PlateCase],
    ends: &[&EndForces],
) -> Vec<Vec<PlateUsage>> {
    use ohl_member_check::joints as by_plugin;
    let Some(p) = &plans.plugin else {
        return cases.iter().map(|c| plate_usage(plans, c.2, c.1)).collect();
    };
    let force = |x: &Planned, axial: &BTreeMap<String, f64>| match &x.plan {
        Plan::Bolted { member, share, .. } => axial.get(member).copied().unwrap_or(0.0) * share,
        Plan::Unplaced { member } => axial.get(member).copied().unwrap_or(0.0),
        Plan::NoSeam { .. } => 0.0,
        Plan::Weld { sides, ring, .. } => weld_force(sides, *ring, axial),
    };
    // What acts on a part beside its force, per case (2026-11-18): a
    // weld's moments about it.
    let extra_of = |x: &Planned| -> by_plugin::Extra {
        match &x.plan {
            Plan::Weld {
                sides, ring, at, ..
            } => by_plugin::Extra {
                moments: cases
                    .iter()
                    .map(|c| {
                        let l = weld_loads(sides, *ring, at, c.2);
                        [0.0, l[1], l[2], 0.0]
                    })
                    .collect(),
                free_length: None,
            },
            // A bolt group: its plate's free length in compression and
            // the member's end shear and moments at its bolts.
            Plan::Bolted {
                member,
                free,
                at,
                share,
                ..
            } => by_plugin::Extra {
                moments: if ends.len() == cases.len() {
                    ends.iter()
                        .map(|e| {
                            e.get(member)
                                .map_or([0.0; 4], |f| bolt_loads(at, &f[at.end], *share))
                        })
                        .collect()
                } else {
                    Vec::new()
                },
                free_length: *free,
            },
            _ => by_plugin::Extra::default(),
        }
    };
    let mut extra: Vec<by_plugin::Extra> = Vec::new();
    let sent: Vec<by_plugin::Part> = plans
        .parts
        .iter()
        .enumerate()
        .filter_map(|(k, x)| {
            let (kind, other, bolts, steel, weld) = match &x.plan {
                Plan::Bolted {
                    member,
                    group,
                    steel,
                    ..
                } => (
                    "bolts",
                    member,
                    Some(by_plugin::bolt_group(group)),
                    Some(by_plugin::plate_steel(steel)),
                    None,
                ),
                Plan::Weld { with, input, .. } => {
                    ("weld", with, None, None, Some(by_plugin::weld(input)))
                }
                Plan::NoSeam { .. } | Plan::Unplaced { .. } => return None,
            };
            extra.push(extra_of(x));
            Some(by_plugin::Part {
                id: k as u32,
                joint: x.joint.clone(),
                plate: x.plate.clone(),
                kind: kind.to_owned(),
                other: other.clone(),
                bolts,
                steel,
                weld,
                forces: cases.iter().map(|c| force(x, c.2)).collect(),
                notes: x.notes.clone(),
            })
        })
        .collect();
    // Which parts something acts on beside their force (by part number).
    let acted: Vec<u32> = sent
        .iter()
        .zip(&extra)
        .filter(|(p, x)| x.acts() || (x.free_length.is_some() && p.forces.iter().any(|f| *f < 0.0)))
        .map(|(p, _)| p.id)
        .collect();
    let (checked, moments_sent) = if sent.is_empty() {
        (by_plugin::JointsChecked::Results(Vec::new()), true)
    } else {
        by_plugin::check_joints_with(
            p,
            &by_plugin::JointInput {
                structure: plans.structure.clone(),
                cases: cases
                    .iter()
                    .map(|c| by_plugin::LoadCase {
                        name: c.0.to_owned(),
                        strength: c.1,
                    })
                    .collect(),
                parts: sent,
            },
            &extra,
        )
    };
    let not_checked = by_plugin::not_checked_by(p);
    // The joints' bolt groups (2026-11-19): one more call with every
    // group and case.
    let mut groups = if plans.groups.is_empty() {
        vec![Vec::new(); cases.len()]
    } else {
        crate::joint_groups::group_rows(&plans.groups, &plans.structure, Some(p), None, cases, ends)
    };
    cases
        .iter()
        .enumerate()
        .map(|(n, c)| {
            let mut rows: Vec<PlateUsage> = plans
                .parts
                .iter()
                .enumerate()
                .map(|(k, x)| {
                    let part = match &x.plan {
                        Plan::Bolted { member, .. } | Plan::Unplaced { member } => {
                            PlatePart::Member(member.clone())
                        }
                        Plan::Weld { with, .. } | Plan::NoSeam { with } => {
                            PlatePart::Weld(with.clone())
                        }
                    };
                    let mut row = PlateUsage {
                        joint: x.joint.clone(),
                        plate: x.plate.clone(),
                        part,
                        force: force(x, c.2),
                        usage: None,
                        governs: String::new(),
                        checks: Vec::new(),
                        notes: x.notes.clone(),
                        does_not_fit: None,
                    };
                    if matches!(x.plan, Plan::NoSeam { .. } | Plan::Unplaced { .. }) {
                        // Nothing to send: not checked, as the core says it.
                        row.governs = x
                            .notes
                            .last()
                            .cloned()
                            .unwrap_or_else(|| "not checked".into());
                        return row;
                    }
                    let results = match &checked {
                        by_plugin::JointsChecked::NotChecked(why) => {
                            row.governs = format!("{not_checked}: {why}");
                            return row;
                        }
                        by_plugin::JointsChecked::Results(r) => r,
                    };
                    let Some(r) = results.iter().find(|r| r.id as usize == k) else {
                        row.governs =
                            format!("{not_checked}: the plugin returned nothing for this part");
                        return row;
                    };
                    row.notes.extend(r.notes.iter().cloned());
                    let Some(u) = r.usages.iter().find(|u| u.load_case as usize == n) else {
                        row.governs = format!(
                            "{not_checked}: the plugin returned no usage for this load case"
                        );
                        return row;
                    };
                    row.usage = Some(u.usage);
                    row.governs = if u.check.is_empty() {
                        r.governs.clone()
                    } else {
                        u.check.clone()
                    };
                    row.checks = vec![(row.governs.clone(), u.usage)];
                    if let Some(w) = &r.not_safe {
                        row.governs
                            .push_str(&format!(" {} {w}]", ohl_structures_fe::check::NOT_SAFE));
                    }
                    // Something acts on it beside its force that the
                    // plugin was not sent (it has no joint-check-moments).
                    if !moments_sent && acted.contains(&(k as u32)) {
                        row.governs.push_str(&format!(
                            " {} {}]",
                            ohl_structures_fe::check::NOT_SAFE,
                            not_sent_words(&p.plugin, &x.plan)
                        ));
                    }
                    row
                })
                .collect();
            rows.append(&mut groups[n]);
            rows
        })
        .collect()
}

/// What a plugin without `joint-check-moments` was not sent of a part.
fn not_sent_words(plugin: &str, plan: &Plan) -> String {
    let what = match plan {
        Plan::Weld { .. } => "the moments of its force about the weld",
        _ => {
            "the plate's free length in compression (its buckling), the member's end shear and moments"
        }
    };
    format!("{what} not sent: the plugin {plugin} has no joint-check-moments")
}

/// The force a weld carries: the larger resultant of the member forces on
/// its two sides (a ring: the sum of every member's force), N.
fn weld_force(sides: &[Vec<SideMember>; 2], ring: bool, axial: &BTreeMap<String, f64>) -> f64 {
    weld_loads_with(sides, ring, None, axial)[0]
}

/// What a weld carries (2026-11-18): `[F, M, T]` — the force (as
/// [`weld_force`]), the moment of the member forces about the weld's
/// middle bending it along its length (the moment's part square to the
/// seam, N·m) and the moment about the seam's own line (N·m) — each the
/// larger of the two sides' (a ring: the sums of every member's sizes).
/// Each member's force acts along its direction through its point
/// ([`SideMember`]). A weld shorter than the seam lies anywhere along it
/// (where it is is not given): its middle up to (seam − weld) / 2 from
/// the seam's, the in-plane moment taken at its largest, |M| + that
/// distance × |u × F| (on the safe side).
pub(crate) fn weld_loads(
    sides: &[Vec<SideMember>; 2],
    ring: bool,
    at: &WeldPlace,
    axial: &BTreeMap<String, f64>,
) -> [f64; 3] {
    weld_loads_with(sides, ring, Some(at), axial)
}

fn weld_loads_with(
    sides: &[Vec<SideMember>; 2],
    ring: bool,
    at: Option<&WeldPlace>,
    axial: &BTreeMap<String, f64>,
) -> [f64; 3] {
    let n = |m: &str| axial.get(m).copied().unwrap_or(0.0);
    // The moment of forces about the weld's middle split into its part
    // about the seam (T) and the rest (M), M at its largest along the
    // seam for a shorter weld.
    let split = |m: [f64; 3], f: [f64; 3]| -> [f64; 2] {
        let Some(at) = at else {
            return [0.0, 0.0];
        };
        let t = dot(m, at.along);
        let rest = sub(m, scale(at.along, t));
        let slack = ((at.seam - at.length) / 2.0).max(0.0);
        [norm(rest) + slack * norm(cross(at.along, f)), t.abs()]
    };
    let middle = at.map_or([0.0; 3], |a| a.middle);
    if ring {
        let mut out = [0.0; 3];
        for (m, d, q) in &sides[0] {
            let f = scale(*d, n(m));
            let [mm, t] = split(cross(sub(*q, middle), f), f);
            out[0] += n(m).abs();
            out[1] += mm;
            out[2] += t;
        }
        return out;
    }
    let mut out = [0.0f64; 3];
    for s in sides {
        let mut r = [0.0; 3];
        let mut mo = [0.0; 3];
        for (m, d, q) in s {
            let f = scale(*d, n(m));
            r = add(r, f);
            mo = add(mo, cross(sub(*q, middle), f));
        }
        let [mm, t] = split(mo, r);
        out[0] = out[0].max(norm(r));
        out[1] = out[1].max(mm);
        out[2] = out[2].max(t);
    }
    out
}

/// The joints of a case's joint checks that do not fit as given
/// (2026-11-19): each with why, its parts' words joined — failing,
/// counted apart from the usages, never an infinite usage.
pub fn joints_not_fitting(plates: &[PlateUsage]) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for p in plates {
        if let Some(w) = &p.does_not_fit {
            let e = out.entry(p.joint.clone()).or_default();
            let words = format!("{}: {w}", p.what());
            if !e.contains(&words) {
                if !e.is_empty() {
                    e.push_str("; ");
                }
                e.push_str(&words);
            }
        }
    }
    out
}

/// The worst usage per joint of a case's joint checks: its plates', welds',
/// bolt groups' and bearing (2026-11-19) — the joint's usage. A joint that
/// does not fit is in [`joints_not_fitting`] (its parts that fit keep
/// their usage here).
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
pub(crate) fn ray_to_outline(outline: &[[f64; 3]], origin: [f64; 3], dir: [f64; 3]) -> Option<f64> {
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
        let o = [0.0; 3];
        let sides = [
            vec![("A".to_owned(), [0.0, 0.0, 1.0], o)],
            vec![
                ("B".to_owned(), [1.0, 0.0, 0.0], o),
                ("C".to_owned(), [0.0, 1.0, 0.0], o),
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

    /// A frame member's end forces at its bolt group (invented, by hand):
    /// the plate's normal z, the member along x, its bolts' centroid 0.1 m
    /// from the joint; F = (100, 5, 2) kN, M = (0.3, 0.4, 1.0) kN m. The
    /// shear in the plane (along y = z × x) 5 kN, square to it 2 kN; about
    /// the normal 1.0 + 0.1 × 5 = 1.5 kN m; the rest |(0.3, 0.4, 0)| plus
    /// 0.1 × 2, 0.5 + 0.2 = 0.7 kN m; a cover's share 0.5 halves them.
    #[test]
    fn a_frame_members_end_forces_at_its_bolts() {
        let at = BoltPlace {
            end: 0,
            normal: [0.0, 0.0, 1.0],
            along: [1.0, 0.0, 0.0],
            lever: 0.1,
        };
        let ef = [100e3, 5e3, 2e3, 300.0, 400.0, 1000.0];
        let l = bolt_loads(&at, &ef, 1.0);
        let want = [5e3, 1500.0, 700.0, 2e3];
        for k in 0..4 {
            assert!((l[k] - want[k]).abs() < 1e-9, "{l:?}");
        }
        let h = bolt_loads(&at, &ef, 0.5);
        assert!((h[1] - 750.0).abs() < 1e-9, "{h:?}");
    }

    /// The moments of a weld's force (invented data, worked by hand): a
    /// seam along z from (0, 0, 0) to (0, 0, 0.2), its middle (0, 0,
    /// 0.1). One side: member B pulling 100 kN along x through its bolts
    /// at (0.15, 0, 0.18) — r = (0.15, 0, 0.08), F = (100, 0, 0) kN, r × F
    /// = (0, 0.08·100, 0) = (0, 8, 0) kN m: square to the seam, so M =
    /// 8 kN m bending the weld along its length, T = 0 about the seam.
    /// Member C on the same side pushing 50 kN along y at (0.15, 0, 0.1):
    /// r = (0.15, 0, 0), F = (0, −50, 0), r × F = (0, 0, −7.5) kN m —
    /// about the seam: T = 7.5 kN m (its side's resultant √(100² + 50²)
    /// = 111.8 kN). The other side: member A 30 kN along z at the middle,
    /// no moment. A weld of 0.12 m on the 0.2 m seam lies anywhere along
    /// it: its middle up to 0.04 m off, M up to 8 + 0.04 × |u × F| = 8 +
    /// 0.04 × 111.8 = 12.47 kN m.
    #[test]
    fn a_weld_carries_the_moments_of_its_force() {
        let sides = [
            vec![("A".to_owned(), [0.0, 0.0, 1.0], [0.0, 0.0, 0.1])],
            vec![
                ("B".to_owned(), [1.0, 0.0, 0.0], [0.15, 0.0, 0.18]),
                ("C".to_owned(), [0.0, 1.0, 0.0], [0.15, 0.0, 0.1]),
            ],
        ];
        let axial: BTreeMap<String, f64> = [("A", 30e3), ("B", 100e3), ("C", -50e3)]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect();
        let mut at = WeldPlace {
            middle: [0.0, 0.0, 0.1],
            along: [0.0, 0.0, 1.0],
            seam: 0.2,
            length: 0.2,
        };
        let [f, m, t] = weld_loads(&sides, false, &at, &axial);
        assert!((f - 100e3f64.hypot(50e3)).abs() < 1e-6, "{f}");
        assert!((m - 8e3).abs() < 1e-6, "{m}");
        assert!((t - 7.5e3).abs() < 1e-6, "{t}");
        at.length = 0.12;
        let [_, m, _] = weld_loads(&sides, false, &at, &axial);
        assert!(
            (m - (8e3 + 0.04 * 100e3f64.hypot(50e3))).abs() < 1e-6,
            "{m}"
        );
        // A ring: the sums of the sizes (A's none, B's 8 + 0.04 × 100,
        // C's 0 + 0.04 × 50 in plane; T: C's 7.5).
        let all = [
            sides[0].iter().chain(&sides[1]).cloned().collect(),
            Vec::new(),
        ];
        let [f, m, t] = weld_loads(&all, true, &at, &axial);
        assert!((f - 180e3).abs() < 1e-6, "{f}");
        assert!((m - (8e3 + 0.04 * 100e3 + 0.04 * 50e3)).abs() < 1e-6, "{m}");
        assert!((t - 7.5e3).abs() < 1e-6, "{t}");
    }
}
