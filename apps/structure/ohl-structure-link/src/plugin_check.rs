// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The member check of a structure by a plugin (the `member-check`
//! interface, owner 2026-11-11; built, in active development, not
//! approved yet): a structure file that names a plugin's method
//! (`[analysis] capacity = "plugin:<plugin>/<method>"`,
//! [`ohl_structures_fe::model::PluginMethod`]) has its members checked by
//! that plugin instead of the core's own methods.
//!
//! - **Found or refused.** The plugin is looked for by its name in
//!   `Plugins/` beside the structure file, then in the user's plugin
//!   folder. Not installed, not built, or without that method: the run
//!   fails with a message naming the plugin and the method — never
//!   another code in its place.
//! - **What it is sent.** Every checked member in one call
//!   ([`input`]): the records of the interface, from the model and the
//!   solved cases; the ends' connections only when the joint details are
//!   taken (level 4, the file's option on).
//! - **What comes back** ([`usages`]): per case, each member's usage and
//!   what governs. A member the plugin returns nothing for, or no usage
//!   for a case, is [`NOT_CHECKED`] — without a usage, never passing —
//!   and what the plugin flags as not on the safe side is said with the
//!   member.
//!
//! The run of the structure application calls the plugin once per load
//! case (the cases are solved in parallel and their solutions not kept
//! together); the interface itself takes every case in one call.

use std::path::{Path, PathBuf};

use ohl_plugins::member_check as wit;
use ohl_structures_fe::check::MemberUsage;
use ohl_structures_fe::design::Onto;
use ohl_structures_fe::model::PluginMethod;
use ohl_structures_fe::{Model, Solution};

/// Said for a member the plugin gave nothing for.
pub const NOT_CHECKED: &str = "NOT CHECKED, NOT ON THE SAFE SIDE";

/// The folder beside a structure file where its plugins are looked for.
pub const BESIDE: &str = "Plugins";

/// The folders plugins are looked for in: `Plugins/` beside the
/// structure, then the user's plugin folder.
pub fn roots(dir: Option<&Path>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(d) = dir {
        out.push(d.join(BESIDE));
    }
    if let Some(s) = ohl_plugins::settings::shared_storage_dir() {
        out.push(ohl_plugins::settings::user_dir(&s));
    }
    out
}

/// The installed plugins' member-check methods in `roots`, as (plugin,
/// method, description): what the method choice offers.
pub fn methods(roots: &[PathBuf]) -> Vec<(String, String, String)> {
    let Ok(host) = ohl_plugins::WasmHost::new() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for root in roots {
        for f in ohl_plugins::discover(root) {
            let Ok(m) = &f.manifest else { continue };
            for method in host.member_check_methods(&f.dir).unwrap_or_default() {
                out.push((m.plugin.name.clone(), method.name, method.description));
            }
        }
    }
    out
}

/// Where the plugin `p` names is installed, with its method there. Err:
/// the message a run fails with — the plugin and the method named, where
/// it was looked for, and that no other method is used in its place.
pub fn resolve(p: &PluginMethod, roots: &[PathBuf]) -> Result<PathBuf, String> {
    let refused = |why: String| {
        format!(
            "member check: plugin \"{}\" (method \"{}\") {why}; the structure is not checked by \
             another method in its place — install the plugin or choose another method \
             ([analysis] capacity)",
            p.plugin, p.method
        )
    };
    let found = roots
        .iter()
        .flat_map(|r| ohl_plugins::discover(r))
        .find(|f| f.manifest.as_ref().is_ok_and(|m| m.plugin.name == p.plugin));
    let Some(found) = found else {
        return Err(refused(format!(
            "is not installed (looked in: {})",
            roots
                .iter()
                .map(|r| r.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    };
    let host = ohl_plugins::WasmHost::new().map_err(|e| refused(format!("cannot run: {e}")))?;
    let methods = host
        .member_check_methods(&found.dir)
        .map_err(|e| refused(format!("does not load: {e}")))?;
    if methods.is_empty() {
        return Err(refused(format!(
            "in {} gives no member check (not built, or built without the member-check export)",
            found.dir.display()
        )));
    }
    if !methods.iter().any(|m| m.name == p.method) {
        return Err(refused(format!(
            "has no such method (it gives: {})",
            methods
                .iter()
                .map(|m| m.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    Ok(found.dir)
}

/// The plugin a model's file names, found for its run (`dir`: the
/// structure file's folder) and kept in the model; a model that names
/// none is left as it is. Err: [`resolve`]'s message.
pub fn find(mut model: Model, dir: Option<&Path>) -> Result<Model, String> {
    if let Some(p) = &mut model.capacity_plugin {
        p.dir = Some(resolve(p, &roots(dir))?);
    }
    Ok(model)
}

fn word<T>(v: Option<T>, f: impl Fn(T) -> &'static str) -> String {
    v.map_or(String::new(), |v| f(v).to_owned())
}

/// The interface's records for `model` and its solved `cases` (name,
/// strength factor, self-weight factor, solution): every checked member,
/// its forces in each case.
pub fn input(model: &Model, cases: &[(&str, f64, f64, &Solution)]) -> wit::CheckInput {
    let taken = model.joint_use.taken();
    let basics: Vec<Vec<ohl_structures_fe::capacity::CapacityInput>> = cases
        .iter()
        .map(|c| ohl_structures_fe::capacity::capacity_inputs(model, c.3))
        .collect();
    let mut members = Vec::new();
    for (k, m) in model.members.iter().enumerate() {
        if !m.check {
            continue;
        }
        let Some(b) = basics.first().and_then(|b| b.get(k)) else {
            continue;
        };
        let d = &m.design;
        let conditions = ohl_structures_fe::member_ends::end_conditions(model, k);
        let ends = (0..2)
            .map(|e| wit::EndCondition {
                eccentric: conditions.eccentric[e].value,
                eccentric_source: conditions.eccentric[e].source.words().to_owned(),
                restrained: conditions.restrained[e].value,
                restrained_source: conditions.restrained[e].source.words().to_owned(),
            })
            .collect();
        let mut connections = Vec::new();
        if taken {
            for (e, c) in d.ends.iter().enumerate() {
                let Some(c) = c else { continue };
                let (onto, onto_thickness) = match &c.onto {
                    None => ("", None),
                    Some(Onto::Member(_)) => ("member", None),
                    Some(Onto::Plate { thickness, .. }) => ("plate", Some(*thickness)),
                    Some(Onto::Splice { covers, .. }) => ("splice", Some(*covers)),
                    Some(Onto::Other(_)) => ("other", None),
                };
                connections.push(wit::EndConnection {
                    end: e as u32,
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
                    continuous: c.continuous.is_some(),
                    notes: c.notes.clone(),
                });
            }
        }
        let forces = cases
            .iter()
            .enumerate()
            .map(|(n, c)| {
                let f = &c.3.members[k];
                wit::CaseForces {
                    load_case: n as u32,
                    axial: f.axial,
                    end_forces: f.end_forces.to_vec(),
                    partner_axial: basics[n][k].partner_axial,
                    slack: f.slack,
                }
            })
            .collect();
        members.push(wit::Member {
            id: b.id.clone(),
            kind: b.kind.clone(),
            class: word(d.class, |c| c.as_str()),
            shape: b.shape.clone(),
            dimensions: b.dimensions.clone(),
            parts: Vec::new(),
            area: b.area,
            iy: b.iy,
            iz: b.iz,
            iv: b.iv,
            wy: b.wy,
            wz: b.wz,
            e: b.e,
            fy: b.fy,
            fu: b.fu,
            length: b.length,
            k: b.k,
            lk: b.lk,
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
            ends,
            connections,
            forces,
        });
    }
    wit::CheckInput {
        structure: model.name.clone(),
        joint_details: taken,
        cases: cases
            .iter()
            .map(|c| wit::LoadCase {
                name: c.0.to_owned(),
                strength: c.1,
                self_weight: c.2,
            })
            .collect(),
        members,
    }
}

/// The members' usage in each of `cases` cases from what the plugin gave
/// (`results`), one list per case in the model's member order: a checked
/// member without a result, or without a usage for the case, is
/// [`NOT_CHECKED`] (no usage: never passing); a modelling member as the
/// core says it; what the plugin flags as not safe is said with the
/// member.
pub fn usages(
    model: &Model,
    p: &PluginMethod,
    cases: usize,
    results: &[wit::MemberResult],
) -> Vec<Vec<MemberUsage>> {
    let by_id: std::collections::HashMap<&str, &wit::MemberResult> =
        results.iter().map(|r| (r.id.as_str(), r)).collect();
    (0..cases)
        .map(|n| {
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
                    let Some(u) = r.usages.iter().find(|u| u.load_case as usize == n) else {
                        return none("returned no usage for this load case");
                    };
                    let mut governs = if u.check.is_empty() {
                        r.governs.clone()
                    } else {
                        u.check.clone()
                    };
                    if let Some(w) = &r.not_safe {
                        governs.push_str(&format!(" [NOT ON THE SAFE SIDE: {w}]"));
                    }
                    MemberUsage {
                        member: k,
                        usage: Some(u.usage),
                        governs,
                    }
                })
                .collect()
        })
        .collect()
}

/// One case's member usage by the plugin `p` names, installed in `dir`
/// ([`resolve`]): the plugin called with every checked member and this
/// case, and the note for the results (the method, by whom, how many
/// members are not checked).
pub fn usage_of_case(
    model: &Model,
    p: &PluginMethod,
    dir: &Path,
    case: (&str, f64, f64),
    solution: &Solution,
) -> Result<(Vec<MemberUsage>, String), String> {
    let host = ohl_plugins::WasmHost::new().map_err(|e| e.to_string())?;
    let sent = input(model, &[(case.0, case.1, case.2, solution)]);
    let results = host
        .check_members(dir, &p.method, &sent)
        .map_err(|e| format!("member check: {e}"))?;
    let usage = usages(model, p, 1, &results).remove(0);
    let bare = usage
        .iter()
        .filter(|u| u.governs.starts_with(NOT_CHECKED))
        .count();
    let mut note = format!(
        "member capacities: method {} of the plugin {} (not checked by OpenOHL)",
        p.method, p.plugin
    );
    if bare > 0 {
        note.push_str(&format!(
            "; {bare} member(s) {NOT_CHECKED}: the plugin returned nothing for them"
        ));
    }
    Ok((usage, note))
}
