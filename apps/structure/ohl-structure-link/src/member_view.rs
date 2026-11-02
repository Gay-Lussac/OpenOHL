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
    /// Its place partly assumed (for the drawing).
    pub assumed: bool,
}

/// The member's leg bolted laid flat: its length, its width and its
/// holes at both ends.
#[derive(Debug, Clone, PartialEq)]
pub struct FlatLeg {
    /// Length, m.
    pub length: f64,
    /// The leg's width, m.
    pub width: f64,
    /// The holes.
    pub holes: Vec<Hole>,
    /// Per end (first, second): the end distance, the pitch and the gauge
    /// drawn, m, where it has bolts.
    pub layout: [Option<[f64; 3]>; 2],
}

/// The leg bolted of `m` laid flat (an angle's; none: no angle or no
/// bolts at either end).
pub fn flat_leg(m: &MemberEnds) -> Option<FlatLeg> {
    let mut width = None;
    let mut holes = Vec::new();
    let mut layout = [None, None];
    for (k, e) in m.ends.iter().enumerate() {
        let Some((_, a)) = e else { continue };
        let Some(g) = a.profile.as_ref().and_then(|p| p.angle) else {
            continue;
        };
        width = width.or(Some(if a.leg_y { g.b } else { g.h }));
        let Some(b) = &a.bolts.value else { continue };
        let (Some(pitch), Some(end)) = (a.pitch.value, a.end_distance.value) else {
            continue;
        };
        let gauges = a.gauges.value.clone().unwrap_or_default();
        if gauges.is_empty() {
            continue;
        }
        let assumed =
            [a.gauges.source, a.pitch.source, a.end_distance.source].contains(&Source::Assumed);
        let lines = gauges.len() as u32;
        for n in 0..b.count {
            let line = (n % lines) as usize;
            let row = f64::from(n / lines);
            let stagger = if lines == 2 && line == 1 {
                pitch / 2.0
            } else {
                0.0
            };
            let s = end + row * pitch + stagger;
            holes.push(Hole {
                // From the member's first joint: the second end's measured
                // back from its far end.
                along: if k == 0 { s } else { m.length - s },
                across: gauges[line],
                hole: b.hole.unwrap_or(b.diameter),
                bolt: b.diameter,
                assumed,
            });
        }
        layout[k] = Some([end, pitch, gauges[0]]);
    }
    Some(FlatLeg {
        length: m.length,
        width: width?,
        holes,
        layout,
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
/// the connections and the bolt library as the check reads them; a
/// crossing diagonal's partner not known (no load case: as the check
/// without one). Err: the model is not built (below level 3), or no such
/// member.
pub fn member_capacity(
    text: &str,
    sections: &SectionLibrary,
    bolts: &BoltLibrary,
    id: &str,
) -> Result<MemberCapacity, String> {
    let model = Model::from_structure_type_with_bolts(text, sections, bolts, None)?;
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
