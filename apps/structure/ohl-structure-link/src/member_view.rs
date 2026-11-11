// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A member in detail (owner 2026-11-02: "the same view of the members
//! as for the joints where we could define where the holes and bolts are
//! and that it would directly be shown on the joint side … see live the
//! impacts on the bar's resistance"): its two ends as the joints see them
//! ([`crate::joint_details`]: the same `[[connection]]` data, so what is
//! set from the member is what the joint shows), its leg bolted laid flat
//! with the holes and bolts of both ends, and its ASCE 10-97 capacities
//! worked out from them ([`ohl_structures_fe::asce10::capacity`], as the
//! check takes them).

use ohl_sections::SectionLibrary;
use ohl_structures::bolts::BoltLibrary;
use ohl_structures_fe::model::Model;

use crate::joints::{Arrival, JointDetails, Source};

/// A member's two ends as the joints see them.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberEnds {
    /// The member.
    pub member: String,
    /// Its length, m.
    pub length: f64,
    /// Its section's name (none: no section).
    pub section: Option<String>,
    /// At its first and second joint: the joint and the end there.
    pub ends: [Option<(String, Arrival)>; 2],
}

/// The member `id`'s ends in `details`.
pub fn member_ends(details: &JointDetails, id: &str) -> Option<MemberEnds> {
    let mut ends: [Option<(String, Arrival)>; 2] = [None, None];
    for j in &details.joints {
        for a in j.arrivals.iter().filter(|a| a.member == id) {
            ends[a.end] = Some((j.id.clone(), a.clone()));
        }
    }
    let any = ends.iter().flatten().next()?;
    Some(MemberEnds {
        member: id.to_owned(),
        length: any.1.length,
        section: any.1.profile.as_ref().map(|p| p.section.clone()),
        ends,
    })
}

/// A hole of the member's leg bolted, laid flat.
#[derive(Debug, Clone, PartialEq)]
pub struct Hole {
    /// Along the member from its first joint, m.
    pub along: f64,
    /// Across the leg from the heel, m.
    pub across: f64,
    /// The hole's diameter, m (the bolt's when not given).
    pub hole: f64,
    /// The bolt's diameter, m.
    pub bolt: f64,
    /// The end it belongs to (0: the first joint's, 1: the second's).
    pub end: usize,
    /// Its place partly assumed (for the drawing).
    pub assumed: bool,
    /// On the other leg (both legs bolted; `across` from the heel on
    /// that leg), drawn folded out below the heel.
    pub other: bool,
    /// Its place in its end's bolts of its leg (`Arrival::bolt_flat`, or
    /// `other_flat` when on the other leg).
    pub index: usize,
}

/// The member's leg bolted laid flat: its length, its width and its
/// holes at both ends.
#[derive(Debug, Clone, PartialEq)]
pub struct FlatLeg {
    /// Length, m.
    pub length: f64,
    /// The leg's width, m.
    pub width: f64,
    /// The other leg's width, m (2026-11-08: drawn folded out below the
    /// heel, so bolts can be moved to it).
    pub other_width: f64,
    /// The holes.
    pub holes: Vec<Hole>,
    /// Per end (first, second): the end distance, the pitch and the gauge
    /// drawn, m, where it has bolts.
    pub layout: [Option<[f64; 3]>; 2],
    /// Per end: where its steel stops from the work point, m.
    pub cuts: [f64; 2],
}

/// The leg bolted of `m` laid flat (an angle's; none: no angle at
/// either end): its holes where the joints put them (`Arrival::bolt_flat`,
/// measured from each joint), where its steel stops at each end.
pub fn flat_leg(m: &MemberEnds) -> Option<FlatLeg> {
    let mut width = None;
    let mut other_width = 0.0;
    let mut holes = Vec::new();
    let mut layout = [None, None];
    let mut cuts = [0.0; 2];
    for (k, e) in m.ends.iter().enumerate() {
        let Some((_, a)) = e else { continue };
        let Some(g) = a.profile.as_ref().and_then(|p| p.angle) else {
            continue;
        };
        if width.is_none() {
            width = Some(if a.leg_y { g.b } else { g.h });
            other_width = if a.leg_y { g.h } else { g.b };
        }
        cuts[k] = a.cut.value.unwrap_or(0.0);
        let Some(b) = &a.bolts.value else { continue };
        let assumed =
            [a.gauges.source, a.pitch.source, a.end_distance.source].contains(&Source::Assumed);
        let legs = [(&a.bolt_flat, false), (&a.other_flat, true)];
        for (flat, other) in legs {
            for (index, q) in flat.iter().enumerate() {
                holes.push(Hole {
                    // From the member's first joint: the second end's
                    // measured back from its far end.
                    along: if k == 0 { q[0] } else { m.length - q[0] },
                    across: q[1],
                    hole: b.hole.unwrap_or(b.diameter),
                    bolt: b.diameter,
                    end: k,
                    assumed,
                    other,
                    index,
                });
            }
        }
        if let (Some(e), Some(p), Some(g0)) = (
            a.end_distance.value,
            a.pitch.value,
            a.gauges.value.as_ref().and_then(|g| g.first().copied()),
        ) {
            layout[k] = Some([e, p, g0]);
        }
    }
    Some(FlatLeg {
        length: m.length,
        width: width?,
        other_width,
        holes,
        layout,
        cuts,
    })
}

/// A member seen from the side (2026-11-10): what the member view draws
/// of a member whose leg bolted is not laid flat ([`flat_leg`] none: a
/// built-up section, an I or H, a tube, a section of values only).
#[derive(Debug, Clone, PartialEq)]
pub struct Elevation {
    /// Length between its work points, m.
    pub length: f64,
    /// Per end: where its steel stops from the work point, m (as the 3D
    /// view's solid: [`crate::steel_start`], the second within what the
    /// first leaves).
    pub cuts: [f64; 2],
    /// Each piece of its steel seen along its section's z: from where to
    /// where it reaches across (its section's y, from the member's axis),
    /// m. A built-up section: one per part, the gap between them open.
    pub bands: Vec<[f64; 2]>,
    /// The pieces' outlines in the section's plane (y, z), about the
    /// member's axis, m: its cross-section, drawn beside.
    pub section: Vec<Vec<[f64; 2]>>,
    /// The outlines are the section's shape (false: a section of values
    /// only, a square of its area).
    pub shaped: bool,
}

/// `m` seen from the side; none: no cross-section at either end (below
/// level 3).
pub fn elevation(m: &MemberEnds) -> Option<Elevation> {
    let p = m
        .ends
        .iter()
        .flatten()
        .find_map(|(_, a)| a.profile.as_ref())?;
    let cut = |k: usize| {
        m.ends[k]
            .as_ref()
            .map_or(0.0, |(_, a)| crate::steel_start(a))
    };
    let c0 = cut(0).min(m.length);
    let c1 = cut(1).min(m.length - c0);
    let section: Vec<Vec<[f64; 2]>> = p.pieces().map(<[_]>::to_vec).collect();
    let bands = section
        .iter()
        .map(|o| {
            o.iter().fold([f64::INFINITY, f64::NEG_INFINITY], |b, q| {
                [b[0].min(q[0]), b[1].max(q[0])]
            })
        })
        .collect();
    Some(Elevation {
        length: m.length,
        cuts: [c0, c1],
        bands,
        section,
        shaped: p.shaped,
    })
}

/// A member's capacities as the ASCE 10-97 check works them out.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MemberCapacity {
    /// Tension capacity, N, and what governs it.
    pub tension: Option<(f64, String)>,
    /// Compression capacity, N, and what governs it.
    pub compression: Option<(f64, String)>,
    /// KL/r.
    pub kl_r: Option<f64>,
    /// What was assumed, in words.
    pub assumed: Vec<String>,
    /// Where each end's conditions come from, in words.
    pub joints: Vec<String>,
    /// Slenderness beyond its class's limit, in words.
    pub limit: Option<String>,
}

/// The ASCE 10-97 capacities of member `id` of the structure `text`, with
/// the connections and the bolt library as the check reads them when the
/// joint details are taken (always read here, an aid to the joints'
/// design: a run takes them only with `joint_details = true` at level 4,
/// 2026-11-11); a
/// crossing diagonal's partner not known (no load case: as the check
/// without one). Err: the model is not built (below level 3), or no such
/// member.
pub fn member_capacity(
    text: &str,
    sections: &SectionLibrary,
    bolts: &BoltLibrary,
    id: &str,
) -> Result<MemberCapacity, String> {
    let model = Model::from_structure_type_with_joint_details(text, sections, bolts, None)?;
    let idx = model
        .members
        .iter()
        .position(|m| m.id == id)
        .ok_or_else(|| format!("no member {id} in the model"))?;
    let c = ohl_structures_fe::asce10::capacity(&model, idx, None);
    let mut assumed = c.compression_assumed.clone();
    for a in &c.tension_assumed {
        if !assumed.contains(a) {
            assumed.push(a.clone());
        }
    }
    Ok(MemberCapacity {
        tension: c.tension,
        compression: c.compression,
        kl_r: c.kl_r,
        assumed,
        joints: c.joints,
        limit: c.limit,
    })
}

/// The bolt rules of member `id`'s end at joint `joint` by the design code
/// of the structure's capacity method
/// ([`ohl_structures_fe::bolt_rules::rules`]): `force` the member's
/// largest axial force over the last analysis, N (none: no analysis — the
/// bolt's shear capacity). None: no model, no such member end, or the
/// method has no bolt rules.
pub fn bolt_rules(
    text: &str,
    sections: &SectionLibrary,
    bolts: &BoltLibrary,
    id: &str,
    joint: &str,
    force: Option<f64>,
) -> Option<ohl_structures_fe::bolt_rules::BoltRules> {
    let model = Model::from_structure_type_with_joint_details(text, sections, bolts, None).ok()?;
    let idx = model.members.iter().position(|m| m.id == id)?;
    let m = &model.members[idx];
    let end = if model.nodes[m.i].id == joint {
        0
    } else if model.nodes[m.j].id == joint {
        1
    } else {
        return None;
    };
    ohl_structures_fe::bolt_rules::rules(&model, idx, end, force)
}
