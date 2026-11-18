// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A member in detail (owner 2026-11-02: "the same view of the members
//! as for the joints where we could define where the holes and bolts are
//! and that it would directly be shown on the joint side … see live the
//! impacts on the bar's resistance"): its two ends as the joints see them
//! ([`crate::joint_details`]: the same `[[connection]]` data, so what is
//! set from the member is what the joint shows), its leg bolted laid flat
//! with the holes and bolts of both ends, and its capacities worked out
//! from them by the structure's design code — ASCE 10-97 (the default) or
//! another, each a plugin's since 2026-11-18 (the bundled `asce10` and
//! `sia263`; asked without a load case, with the structure's parameter
//! file for the plugin, as the check takes them).

use ohl_sections::SectionLibrary;
use ohl_structures::bolts::BoltLibrary;
use ohl_structures_fe::model::Model;

use crate::joints::{Arrival, JointDetails, Source};
use crate::plugin_check;
use ohl_structures_fe::model::PluginMethod;

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

/// A member's capacities as its design code works them out: the core's
/// ASCE 10-97, or the plugin's method the structure names (a bundled
/// one: SIA 263:2013; 2026-11-18).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MemberCapacity {
    /// The code, in words ("ASCE 10-97", "SIA 263:2013 (method sia263-2013
    /// of the plugin sia263)").
    pub code: String,
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

/// The capacities of member `id` of the structure `text` by its design
/// code, with the connections and the bolt library as the check reads them
/// when the joint details are taken (always read here, an aid to the
/// joints' design: a run takes them only with `joint_details = true` at
/// level 4, 2026-11-11); a crossing diagonal's partner taken as not in
/// tension (no load case: as the solve's cap takes it). The structure's
/// folder not known: the plugin's own parameters ([`member_capacity_in`]
/// takes the structure's). Err: the model is not built (below level 3),
/// no such member, or no design code's resistance.
pub fn member_capacity(
    text: &str,
    sections: &SectionLibrary,
    bolts: &BoltLibrary,
    id: &str,
) -> Result<MemberCapacity, String> {
    member_capacity_in(text, sections, bolts, id, None)
}

/// [`member_capacity`] of a structure whose file is in `dir`: the plugin
/// asked with the structure's parameter file for it (`Plugins/<plugin>.toml`
/// beside the structure, 2026-11-18), as a run asks it.
pub fn member_capacity_in(
    text: &str,
    sections: &SectionLibrary,
    bolts: &BoltLibrary,
    id: &str,
    dir: Option<&std::path::Path>,
) -> Result<MemberCapacity, String> {
    let mut model = Model::from_structure_type_with_joint_details(text, sections, bolts, None)?;
    if !model.members.iter().any(|m| m.id == id) {
        return Err(format!("no member {id} in the model"));
    }
    // The design code's own capacities (a plugin's), asked now (no load
    // case); no other code's are shown in their place. The simple check
    // (no design code): ASCE 10-97's, the default's, as an aid to the
    // joints' design (as before the move), said.
    let simple = model.capacity_plugin.is_none();
    if simple {
        model.capacity_plugin =
            PluginMethod::parse(ohl_structures_fe::model::DEFAULT_METHOD).and_then(Result::ok);
    }
    let Some(p) = model.capacity_plugin.clone() else {
        return Err(format!("{RESISTANCE_FROM_PLUGIN} no design code"));
    };
    plugin_check::with_parameters(&mut model, dir)?;
    let mut c = plugin_capacity(&model, &p, id)?;
    if simple {
        c.assumed.insert(
            0,
            "the structure's check is the simple one (not a design code): ASCE 10-97's \
             capacities shown as an aid"
                .into(),
        );
    }
    Ok(c)
}

/// Member `id`'s capacities by the plugin's method `p` of `model`
/// (2026-11-18: Member details shows them as the core's): the plugin
/// asked without a load case, with the model's parameter file for it
/// ([`plugin_check::with_parameters`]; none: the method's own values,
/// said). Err ([`RESISTANCE_FROM_PLUGIN`]): the
/// plugin is not found, failed, or gave nothing for the member — never
/// another code's resistance in its place.
fn plugin_capacity(
    model: &Model,
    p: &ohl_structures_fe::model::PluginMethod,
    id: &str,
) -> Result<MemberCapacity, String> {
    let from = |why: String| {
        format!(
            "{RESISTANCE_FROM_PLUGIN} {} (method {}): {why}; no other code's resistance is \
             shown in its place",
            p.plugin, p.method
        )
    };
    let given = ohl_member_check::capacities(model).map_err(from)?;
    let g = given
        .iter()
        .find(|g| g.id == id)
        .ok_or_else(|| from("nothing for this member: NOT CHECKED, NOT ON THE SAFE SIDE".into()))?;
    let capacity = |name: &str| {
        g.capacities
            .iter()
            .find(|c| c.0 == name)
            .map(|c| (c.1, c.2.clone()))
    };
    let mut assumed = Vec::new();
    let mut joints = Vec::new();
    let mut limit = Vec::new();
    for n in &g.notes {
        if let Some(j) = n.strip_prefix("joints: ") {
            joints.push(j.to_owned());
        } else if let Some(l) = n.strip_prefix("limit: ") {
            limit.push(l.to_owned());
        } else {
            assumed.push(n.clone());
        }
    }
    if let Some(w) = &g.not_safe {
        assumed.push(format!("NOT ON THE SAFE SIDE: {w}"));
    }
    match model
        .capacity_plugin
        .as_ref()
        .and_then(|m| m.parameters.as_ref())
    {
        Some((path, _)) => assumed.push(format!("parameters: {}", path.display())),
        None => assumed.push(format!(
            "the method's own parameters (no parameter file Plugins/{}.toml beside the structure)",
            p.plugin
        )),
    }
    Ok(MemberCapacity {
        code: match p.alias() {
            Some(a) => format!("{a} (the bundled plugin {})", p.plugin),
            None => format!("method {} of the plugin {}", p.method, p.plugin),
        },
        tension: capacity("tension"),
        compression: capacity("compression"),
        kl_r: capacity("slenderness").map(|c| c.0),
        assumed,
        joints,
        limit: (!limit.is_empty()).then(|| limit.join("; ")),
    })
}

/// How [`member_capacity`]'s message starts when the structure's method
/// is a plugin's and gives no resistance: "resistance: from the plugin
/// …".
pub const RESISTANCE_FROM_PLUGIN: &str = "resistance: from";

/// The minimum distances of member `idx`'s end `end` in `model`
/// (arguments as [`ohl_structures_fe::bolt_rules::rules_in`]): the
/// capacity method's own, or — the method a plugin's — that plugin's
/// (its `bolt-distances` export; none when it gives none: never another
/// code's).
pub(crate) fn rules_in(
    model: &Model,
    idx: usize,
    end: usize,
    force: Option<f64>,
    part: Option<(f64, Option<f64>)>,
) -> Option<ohl_structures_fe::bolt_rules::BoltRules> {
    match ohl_member_check::joints::bolt_rules(model, idx, end, force, part) {
        Some(by_plugin) => by_plugin.ok(),
        None => ohl_structures_fe::bolt_rules::rules_in(model, idx, end, force, part),
    }
}

/// Why the structure file `text` has no bolt rules of a design code: its
/// member check is a plugin's method and that plugin gives no minimum
/// distances ("no minimum distances from the plugin: …"); none: the method
/// is the core's, or the plugin gives them.
pub fn no_bolt_rules(text: &str) -> Option<String> {
    use ohl_structures::LibraryItem as _;
    let t =
        ohl_structures::StructureType::parse(std::path::Path::new("structure.ohls"), text).ok()?;
    let name = t.analysis.as_ref()?.get("capacity")?.as_str()?;
    let p = ohl_structures_fe::model::PluginMethod::parse(name)?.ok()?;
    // Asked for an ordinary bolt: whether the plugin gives any at all.
    ohl_member_check::joints::minimum_distances(
        &p,
        &ohl_member_check::joints::DistanceInput {
            diameter: 0.016,
            thickness: 0.0,
            ultimate: None,
            bolt_force: None,
            sheared_edge: true,
            redundant: false,
        },
    )
    .err()
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
    bolt_rules_in(text, sections, bolts, id, joint, force, None)
}

/// [`bolt_rules`] of a structure whose file is in `dir` (its plugins
/// looked for beside it first, the folder remembered for the session).
pub fn bolt_rules_in(
    text: &str,
    sections: &SectionLibrary,
    bolts: &BoltLibrary,
    id: &str,
    joint: &str,
    force: Option<f64>,
    dir: Option<&std::path::Path>,
) -> Option<ohl_structures_fe::bolt_rules::BoltRules> {
    let mut model =
        Model::from_structure_type_with_joint_details(text, sections, bolts, None).ok()?;
    plugin_check::with_parameters(&mut model, dir).ok()?;
    let idx = model.members.iter().position(|m| m.id == id)?;
    let m = &model.members[idx];
    let end = if model.nodes[m.i].id == joint {
        0
    } else if model.nodes[m.j].id == joint {
        1
    } else {
        return None;
    };
    rules_in(&model, idx, end, force, None)
}
