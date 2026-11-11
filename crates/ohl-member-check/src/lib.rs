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
//! - **One call per structure** ([`check`]): every solved load case in
//!   one call, for every runner — OHL-Structure's run (through its link),
//!   `ohl structures fe` and the line's structure check (`ohl-loads`).
//!   [`calls`] counts them (for the tests).
//! - **The core's own remarks stay**: what the core says about a member
//!   whatever method checks it — kept active, a crossing only the other
//!   diagonal holds, eccentric ends
//!   ([`ohl_structures_fe::check::with_remarks`]) — is appended to the
//!   plugin's result.
//!
//! This crate is the one place that knows both the finite-element
//! results and the plugin host: `ohl-structures-fe` runs no plugins, and
//! `ohl-plugins` knows no model.

use std::path::{Path, PathBuf};

pub mod joints;

use ohl_plugins::member_check as wit;
use ohl_sections::Shape;
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
        // Remembered for the session ([`session_roots`]).
        let mut seen = SEEN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !seen.iter().any(|s| s == d) {
            seen.push(d.to_path_buf());
        }
    }
    if let Some(s) = ohl_plugins::settings::shared_storage_dir() {
        out.push(ohl_plugins::settings::user_dir(&s));
    }
    out
}

static SEEN: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

/// The folders plugins are looked for when a structure's folder is not
/// known (a model built from a file's text alone: the joint tools):
/// `Plugins/` beside every structure opened or run in this session
/// (the folders [`roots`] was asked for, the last first), then the
/// user's plugin folder.
pub fn session_roots() -> Vec<PathBuf> {
    let seen = SEEN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let mut out: Vec<PathBuf> = seen.iter().rev().map(|d| d.join(BESIDE)).collect();
    out.extend(roots(None));
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

/// The parts of a built-up member (two angles, two channels): each
/// part's shape and dimensions, half the section's area each; empty for
/// one profile or a section without a profile.
fn parts(model: &Model, member: usize) -> Vec<wit::Part> {
    let s = &model.sections[model.members[member].section];
    let Some(profile) = &s.profile else {
        return Vec::new();
    };
    let named = |v: &[(&str, f64)]| -> Vec<(String, f64)> {
        v.iter().map(|(k, x)| ((*k).to_owned(), *x)).collect()
    };
    let (shape, dimensions) = match profile.shape {
        Shape::DoubleAngle { h, b, t, r, .. } | Shape::StarAngle { h, b, t, r, .. } => {
            if (h - b).abs() < 1e-12 {
                ("equal_angle", named(&[("h", h), ("t", t), ("r", r)]))
            } else {
                (
                    "unequal_angle",
                    named(&[("h", h), ("b", b), ("t", t), ("r", r)]),
                )
            }
        }
        Shape::CrossAngle { h, t, r, .. } => {
            ("equal_angle", named(&[("h", h), ("t", t), ("r", r)]))
        }
        Shape::DoubleChannel {
            h, b, tw, tf, r, ..
        } => (
            "channel",
            named(&[("h", h), ("b", b), ("tw", tw), ("tf", tf), ("r", r)]),
        ),
        _ => return Vec::new(),
    };
    (0..2)
        .map(|_| wit::Part {
            shape: shape.to_owned(),
            dimensions: dimensions.clone(),
            area: s.area / 2.0,
        })
        .collect()
}

fn word<T>(v: Option<T>, f: impl Fn(T) -> &'static str) -> String {
    v.map_or(String::new(), |v| f(v).to_owned())
}

/// One solved load case of a structure, as [`check`] takes it.
#[derive(Debug, Clone, Copy)]
pub struct Solved<'a> {
    /// The case's name.
    pub name: &'a str,
    /// Its strength factor.
    pub strength: f64,
    /// Its factor on the members' own weight (already in the forces).
    pub self_weight: f64,
    /// Its solution.
    pub solution: &'a Solution,
}

/// The interface's records for `model` and its solved `cases`: every
/// checked member, its forces in each case.
pub fn input(model: &Model, cases: &[Solved]) -> wit::CheckInput {
    let taken = model.joint_use.taken();
    let basics: Vec<Vec<ohl_structures_fe::capacity::CapacityInput>> = cases
        .iter()
        .map(|c| ohl_structures_fe::capacity::capacity_inputs(model, c.solution))
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
                let f = &c.solution.members[k];
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
            parts: parts(model, k),
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
                name: c.name.to_owned(),
                strength: c.strength,
                self_weight: c.self_weight,
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
/// member, and what the core says about a member whatever method checks
/// it (kept active, an unbraced crossing, eccentric ends) is appended.
pub fn usages(
    model: &Model,
    p: &PluginMethod,
    cases: &[Solved],
    results: &[wit::MemberResult],
) -> Vec<Vec<MemberUsage>> {
    let crossings = ohl_structures_fe::check::unbraced_crossings(model);
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
                    let Some(u) = r.usages.iter().find(|u| u.load_case as usize == n) else {
                        return none("returned no usage for this load case");
                    };
                    let mut governs = if u.check.is_empty() {
                        r.governs.clone()
                    } else {
                        u.check.clone()
                    };
                    if let Some(w) = &r.not_safe {
                        // The core's own mark: counted wherever the
                        // members not on the safe side are.
                        governs.push_str(&format!(" {} {w}]", ohl_structures_fe::check::NOT_SAFE));
                    }
                    ohl_structures_fe::check::with_remarks(
                        model,
                        case.solution,
                        &crossings,
                        MemberUsage {
                            member: k,
                            usage: Some(u.usage),
                            governs,
                        },
                    )
                })
                .collect()
        })
        .collect()
}

/// What a plugin's member check gave for a structure ([`check`]).
#[derive(Debug, Clone)]
pub struct Checked {
    /// The members' usage, one list per case in the model's member order.
    pub usage: Vec<Vec<MemberUsage>>,
    /// The note for the results: the method, by whom, how many members
    /// are not checked (in the worst case).
    pub note: String,
    /// What the plugin returned, as it gave it (capacities, notes).
    pub results: Vec<wit::MemberResult>,
}

static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// How many times [`check`] called a plugin's `check-members` in this
/// process (the tests of "one call per structure").
pub fn calls() -> usize {
    CALLS.load(std::sync::atomic::Ordering::SeqCst)
}

/// The member check of `model` by the plugin its file names, **one call
/// for all of `cases`**. None: the file names no plugin, or the plugin
/// was not looked for ([`find`]; the core's own results then say every
/// member is not checked). Err: the plugin failed — no result, never
/// another method in its place.
pub fn check(model: &Model, cases: &[Solved]) -> Option<Result<Checked, String>> {
    let p = model.capacity_plugin.as_ref()?;
    let dir = p.dir.as_deref()?;
    Some(check_in(model, p, dir, cases))
}

fn check_in(
    model: &Model,
    p: &PluginMethod,
    dir: &Path,
    cases: &[Solved],
) -> Result<Checked, String> {
    let host = ohl_plugins::WasmHost::new().map_err(|e| format!("member check: {e}"))?;
    let sent = input(model, cases);
    CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let results = host.check_members(dir, &p.method, &sent).map_err(|e| {
        format!(
            "member check: plugin {} (method {}): {e}",
            p.plugin, p.method
        )
    })?;
    let usage = usages(model, p, cases, &results);
    let bare = usage
        .iter()
        .map(|u| {
            u.iter()
                .filter(|u| u.governs.starts_with(NOT_CHECKED))
                .count()
        })
        .max()
        .unwrap_or(0);
    let mut note = format!(
        "member capacities: method {} of the plugin {} (not checked by OpenOHL)",
        p.method, p.plugin
    );
    if bare > 0 {
        note.push_str(&format!(
            "; {bare} member(s) {NOT_CHECKED}: the plugin returned nothing for them"
        ));
    }
    Ok(Checked {
        usage,
        note,
        results,
    })
}
