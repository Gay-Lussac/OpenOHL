// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The `member-check-full` records of a structure (2026-11-18): every
//! checked member with what OpenOHL's own checks read — the section's
//! values and the profile's properties, the capacities given on the
//! member, its ends (the joints, the conditions with why, the joint
//! details and the leg they bolt onto), and per load case its forces with
//! the loads along it, whether it bowed or was kept active, and the
//! member a redundant member braces — and the plugin's results mapped
//! back to the members' usage.

use ohl_plugins::member_check_bracing as bwit;
use ohl_plugins::member_check_full as wit;
use ohl_structures_fe::check::MemberUsage;
use ohl_structures_fe::design::Onto;
use ohl_structures_fe::model::PluginMethod;
use ohl_structures_fe::{Model, Solution};

use crate::{NOT_CHECKED, word};

/// A section's values given in the section library, by the library's
/// names.
fn given(g: &ohl_sections::Given) -> Vec<(String, f64)> {
    [
        ("area", g.area),
        ("iy", g.iy),
        ("iz", g.iz),
        ("iu", g.iu),
        ("iv", g.iv),
        ("it", g.it),
        ("wel_y", g.wel_y),
        ("wel_z", g.wel_z),
        ("wpl_y", g.wpl_y),
        ("wpl_z", g.wpl_z),
        ("gyration_y", g.gyration_y),
        ("gyration_z", g.gyration_z),
        ("gyration_u", g.gyration_u),
        ("gyration_v", g.gyration_v),
        ("shear_area_y", g.shear_area_y),
        ("shear_area_z", g.shear_area_z),
    ]
    .into_iter()
    .filter_map(|(k, v)| v.map(|v| (k.to_owned(), v)))
    .collect()
}

/// The properties of a profile as the core's checks take them.
fn properties(p: &ohl_sections::Section) -> wit::ProfileProperties {
    // Worked out once per profile and thread (the core's store, 2026-11-18:
    // most of a large tower's call went into working them out again).
    let q = ohl_structures_fe::check::properties(p);
    wit::ProfileProperties {
        area: q.area,
        centroid: (q.centroid[0], q.centroid[1]),
        iy: q.iy,
        iz: q.iz,
        iyz: q.iyz,
        iu: q.iu,
        iv: q.iv,
        principal_angle: q.principal_angle,
        it: q.it,
        shear_centre: q.shear_centre.map(|c| (c[0], c[1])),
        warping: q.warping,
        wel_y: q.wel_y,
        wel_z: q.wel_z,
        wel_u: q.wel_u,
        wel_v: q.wel_v,
        wpl_y: q.wpl_y,
        wpl_z: q.wpl_z,
    }
}

/// One end's joint detail.
fn connection(model: &Model, k: usize, end: usize) -> Option<wit::EndConnection> {
    let c = model.members[k].design.ends[end].as_ref()?;
    let (onto, onto_thickness) = match &c.onto {
        None => ("", None),
        Some(Onto::Member(_)) => ("member", None),
        Some(Onto::Plate { thickness, .. }) => ("plate", Some(*thickness)),
        Some(Onto::Splice { covers, .. }) => ("splice", Some(*covers)),
        Some(Onto::Other(_)) => ("other", None),
    };
    Some(wit::EndConnection {
        kind: word(c.kind, |k| k.as_str()),
        onto: onto.to_owned(),
        onto_thickness,
        leg: word(c.leg, |l| l.as_str()),
        bolts: c.bolts.as_ref().map(|b| wit::EndBolts {
            bolt: c.bolt.clone().unwrap_or_default(),
            count: b.count,
            diameter: b.diameter,
            hole: b.hole,
            shear_planes: b.shear_planes,
            shear_capacity: b.shear_capacity,
            shear_capacity_shank: b.shear_capacity_shank,
            ultimate: b.ultimate,
            threads_in_shear: b.threads_in_shear,
            grade: b.grade.map_or(String::new(), |g| g.name()),
        }),
        holes: c.holes,
        gauges: c.gauges.clone(),
        edge_distance: c.edge_distance,
        rolled_edge: c.rolled_edge,
        end_distance: c.end_distance,
        pitch: c.pitch,
        shift: c.shift,
        planes: c.planes.as_ref().map(|p| p.0),
        planes_why: c.planes.as_ref().map_or(String::new(), |p| p.1.clone()),
        continuous: c.continuous.is_some(),
        onto_leg: ohl_structures_fe::member_ends::onto_leg(model, k, end).map(|o| wit::OntoLeg {
            member: o.id,
            thickness: o.thickness,
            fu: o.fu,
        }),
        notes: c.notes.clone(),
    })
}

/// One solved load case as the full records take it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Case<'a> {
    pub name: &'a str,
    pub strength: f64,
    pub self_weight: f64,
    pub solution: &'a Solution,
}

/// The `member-check-full` records of `model` and its solved `cases`
/// (none: the capacities only), with the plugin's `parameters` (TOML; ""
/// none).
pub(crate) fn input(model: &Model, cases: &[Case], parameters: &str) -> wit::FullInput {
    let taken = model.joint_use.taken();
    let redundant: Vec<Vec<Option<Result<ohl_structures_fe::check::Supported, String>>>> =
        match model.analysis.redundant_force {
            Some(share) => cases
                .iter()
                .map(|c| ohl_structures_fe::check::redundant_supports(model, c.solution, share))
                .collect(),
            None => Vec::new(),
        };
    let mut members = Vec::new();
    for (k, m) in model.members.iter().enumerate() {
        if !m.check {
            continue;
        }
        let s = &model.sections[m.section];
        let d = &m.design;
        let conditions = ohl_structures_fe::member_ends::end_conditions(model, k);
        let condition = |c: &ohl_structures_fe::member_ends::Condition| wit::Condition {
            value: c.value,
            source: c.source.words().to_owned(),
            why: c.why.clone().unwrap_or_default(),
        };
        let ends = (0..2)
            .map(|e| wit::MemberEnd {
                joint: model.nodes[if e == 0 { m.i } else { m.j }].id.clone(),
                eccentric: condition(&conditions.eccentric[e]),
                restrained: condition(&conditions.restrained[e]),
                connection: connection(model, k, e),
            })
            .collect();
        let forces = cases
            .iter()
            .enumerate()
            .map(|(n, c)| {
                let f = &c.solution.members[k];
                let (braced, braced_why) = match redundant.get(n).and_then(|r| r[k].as_ref()) {
                    Some(Ok(s)) => (
                        Some(wit::Braced {
                            member: model.members[s.member].id.clone(),
                            joint: model.nodes[s.joint].id.clone(),
                            force: s.force,
                            via_redundant: s.via_redundant,
                        }),
                        String::new(),
                    ),
                    Some(Err(why)) => (None, why.clone()),
                    None => (None, String::new()),
                };
                wit::CaseForces {
                    load_case: n as u32,
                    axial: f.axial,
                    // The moments of eccentric ends the solve could not carry
                    // (only bars meet the joint), in its check (2026-11-18).
                    end_forces: ohl_structures_fe::offsets::with_eccentric_moments(model, k, f)
                        .end_forces
                        .to_vec(),
                    partner_axial: d
                        .crossing
                        .and_then(|p| c.solution.members.get(p))
                        .map(|p| p.axial),
                    slack: f.slack,
                    bowed: f.bowed,
                    kept_active: c.solution.kept_active.contains(&k),
                    length: f.length,
                    span_load: f.span_load.to_vec(),
                    lumped_load: f.lumped_load.to_vec(),
                    braced,
                    braced_why,
                }
            })
            .collect();
        members.push(wit::Member {
            id: m.id.clone(),
            kind: m.kind.as_str().to_owned(),
            frame: m.is_frame(),
            class: word(d.class, |c| c.as_str()),
            section: wit::Section {
                id: s.id.clone(),
                shape: s
                    .profile
                    .as_ref()
                    .map_or(String::new(), |p| p.shape.as_str().to_owned()),
                dimensions: s.profile.as_ref().map_or(Vec::new(), |p| {
                    ohl_structures_fe::capacity::dimensions(&p.shape)
                }),
                given: s.profile.as_ref().map_or(Vec::new(), |p| given(&p.given)),
                properties: s.profile.as_ref().map(properties),
                area: s.area,
                iy: s.iy,
                iz: s.iz,
                iv: s.i_min,
                j: s.j,
                wy: s.wy,
                wz: s.wz,
                e: s.e,
                g: s.g,
                fy: s.yield_strength,
                fu: s.ultimate,
                tension: s.tension,
                compression: s.compression,
                moment_y: s.moment_y,
                moment_z: s.moment_z,
            },
            length: model.length(m),
            k: m.k,
            lk: m.lk,
            length_ratios: d.length_ratios.map_or(Vec::new(), |r| r.to_vec()),
            crossing: d
                .crossing
                .and_then(|p| model.members.get(p))
                .map_or(String::new(), |p| p.id.clone()),
            end_eccentricity: word(d.end_eccentricity, |e| e.as_str()),
            end_restraint: word(d.end_restraint, |e| e.as_str()),
            width_thickness: d.width_thickness,
            thickness: d.thickness,
            plies: d.plies,
            connected_leg: word(d.connected_leg, |l| l.as_str()),
            bolts: d.bolts.map(|b| wit::MemberBolts {
                diameter: b.diameter,
                hole: b.hole,
                count: b.count,
                shear_planes: b.shear_planes,
                shear_capacity: b.shear_capacity,
                ultimate: b.ultimate,
                grade: b.grade.map_or(String::new(), |g| g.name()),
                holes: b.holes,
            }),
            edge_distance: d.edge_distance,
            end_distance: d.end_distance,
            spacing: d.spacing,
            capacity_tension: m.capacity_tension,
            capacity_compression: m.capacity_compression,
            ends,
            forces,
        });
    }
    wit::FullInput {
        structure: model.name.clone(),
        parameters: parameters.to_owned(),
        joint_details: taken,
        single_angle_moments: model.analysis.single_angle_moments,
        tension_compression: model.analysis.tension_compression.as_str().to_owned(),
        redundant_share: model.analysis.redundant_force,
        cases: cases
            .iter()
            .map(|c| wit::LoadCase {
                name: c.name.to_owned(),
                strength: c.strength,
                self_weight: c.self_weight,
            })
            .collect(),
        members,
    }
}

/// What braces each checked member of `model`
/// ([`ohl_structures_fe::bracing`]), as the `member-check-bracing`
/// records: one per checked member with a run.
pub(crate) fn bracing(model: &Model) -> Vec<bwit::MemberBracing> {
    let b = ohl_structures_fe::bracing::bracing(model);
    let id = |k: usize| model.members[k].id.clone();
    (0..model.members.len())
        .filter(|&k| model.members[k].check)
        .filter_map(|k| {
            let (run, _) = b.of(k)?;
            Some(bwit::MemberBracing {
                member: id(k),
                run: run
                    .members
                    .iter()
                    .zip(&run.spans)
                    .zip(&run.forward)
                    .map(|((&m, &(start, end)), &forward)| bwit::RunPiece {
                        member: id(m),
                        start,
                        end,
                        forward,
                    })
                    .collect(),
                length: run.length,
                points: run
                    .points
                    .iter()
                    .map(|p| bwit::BracingPoint {
                        at: p.at,
                        joint: p.joint.map_or(String::new(), |j| model.nodes[j].id.clone()),
                        support: p.support,
                        restraints: p
                            .restraints
                            .iter()
                            .map(|r| {
                                let d = ohl_structures_fe::bracing::local(model, k, r.direction);
                                bwit::Restraint {
                                    member: r.member.map_or(String::new(), id),
                                    direction: (d[0], d[1]),
                                    through: r.through,
                                }
                            })
                            .collect(),
                    })
                    .collect(),
            })
        })
        .collect()
}

/// The check of `model` in `cases` by method `method` of the plugin in
/// `dir`: through its `member-check-bracing` export with what braces each
/// member (true), else its `member-check-full` one (false). None: the
/// plugin has neither.
pub(crate) fn check_full(
    host: &ohl_plugins::WasmHost,
    dir: &std::path::Path,
    method: &str,
    model: &Model,
    cases: &[Case],
    params: &str,
) -> Result<Option<(wit::FullOutput, bool)>, ohl_plugins::PluginError> {
    let input = input(model, cases, params);
    if let Some(out) = host.check_members_bracing(dir, method, &input, &bracing(model))? {
        return Ok(Some((out, true)));
    }
    Ok(host
        .check_members_full(dir, method, &input)?
        .map(|out| (out, false)))
}

/// The members' usage in each of `cases` from what the plugin gave
/// (`results`), one list per case in the model's member order: as
/// [`crate::usages`], a usage left empty by the plugin kept empty (no
/// capacity, its words what governs), one not a number or infinite kept
/// (failing). `braced`: the plugin was sent what braces each member — a
/// half of a diagonal through a crossing then needs no remark of the
/// core's (its plugin worked its length out over the crossing) unless it
/// gives its own lengths.
pub(crate) fn usages(
    model: &Model,
    p: &PluginMethod,
    cases: &[Case],
    results: &[wit::MemberResult],
    braced: bool,
) -> Vec<Vec<MemberUsage>> {
    let mut crossings = ohl_structures_fe::check::unbraced_crossings(model);
    if braced {
        for (k, c) in crossings.iter_mut().enumerate() {
            if !ohl_structures_fe::bracing::lengths_given(&model.members[k]) {
                *c = None;
            }
        }
    }
    let by_id: std::collections::HashMap<&str, &wit::MemberResult> =
        results.iter().map(|r| (r.id.as_str(), r)).collect();
    cases
        .iter()
        .enumerate()
        .map(|(n, case)| {
            model
                .members
                .iter()
                .enumerate()
                .map(|(k, m)| {
                    if !m.check {
                        return MemberUsage {
                            member: k,
                            usage: None,
                            governs: ohl_structures_fe::check::NOT_CHECKED.to_owned(),
                        };
                    }
                    let none = |why: &str| MemberUsage {
                        member: k,
                        usage: None,
                        governs: format!(
                            "{NOT_CHECKED}: plugin {} (method {}) {why}",
                            p.plugin, p.method
                        ),
                    };
                    let Some(r) = by_id.get(m.id.as_str()) else {
                        return none("returned nothing for this member");
                    };
                    let Some(u) = r.cases.iter().find(|u| u.load_case as usize == n) else {
                        return none("returned no result for this load case");
                    };
                    let mut governs = if u.check.is_empty() {
                        r.governs.clone()
                    } else {
                        u.check.clone()
                    };
                    if let Some(w) = &r.not_safe
                        && !ohl_structures_fe::check::not_safe(&governs)
                    {
                        governs.push_str(&format!(" {} {w}]", ohl_structures_fe::check::NOT_SAFE));
                    }
                    ohl_structures_fe::check::with_remarks(
                        model,
                        case.solution,
                        &crossings,
                        MemberUsage {
                            member: k,
                            usage: u.usage,
                            governs,
                        },
                    )
                })
                .collect()
        })
        .collect()
}
