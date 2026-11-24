// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Calculations for the validation of the connection capacities (stage 5
//! of the joint details, queue item C10; 2026-11-23): invented joint parts
//! and bolt groups sent to a design code's plugin as a structure's run
//! sends them (`ohl_member_check::joints`), its usages compared with hand
//! calculations (`benchmarks/structures-fe/joint-capacities.toml`).
//! Adapters only: the records of the plugin interface filled from the
//! case's inputs, SI values with units.

use ohl_core::units::{Force, Length, Quantity, Stress};
use ohl_member_check::joints::{self as mc, GroupsChecked, JointsChecked};
use ohl_plugins::bolt_distances_full as bd;
use ohl_plugins::joint_check as jc;
use ohl_plugins::joint_check_groups as gr;
use ohl_plugins::joint_check_groups_tension as gt;
use ohl_structures_fe::model::PluginMethod;

use crate::{Calculation, Inputs, Outputs, Registry};

pub(crate) fn register(registry: &mut Registry) {
    registry.register(Box::new(JointParts));
    registry.register(Box::new(JointGroups));
    registry.register(Box::new(BoltDistances));
}

/// A value with a unit from a table (`"16 mm"`), SI; none: not given.
fn q<Q: Quantity>(t: &toml::Table, key: &str) -> Result<Option<f64>, String> {
    match t.get(key) {
        None => Ok(None),
        Some(v) => v
            .as_str()
            .ok_or_else(|| format!("{key}: a value with its unit"))?
            .parse::<Q>()
            .map(|x| Some(x.si()))
            .map_err(|e| format!("{key}: {e}")),
    }
}

fn need<Q: Quantity>(t: &toml::Table, key: &str) -> Result<f64, String> {
    q::<Q>(t, key)?.ok_or_else(|| format!("missing {key}"))
}

fn number(t: &toml::Table, key: &str) -> Option<f64> {
    t.get(key)
        .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
}

fn count(t: &toml::Table, key: &str, default: u32) -> u32 {
    t.get(key)
        .and_then(toml::Value::as_integer)
        .and_then(|i| u32::try_from(i).ok())
        .unwrap_or(default)
}

fn tables<'a>(t: &'a toml::Table, key: &str) -> Vec<&'a toml::Table> {
    t.get(key)
        .and_then(toml::Value::as_array)
        .map(|a| a.iter().filter_map(toml::Value::as_table).collect())
        .unwrap_or_default()
}

/// Lengths of a list (`["30 mm", "none"]`), none where `"none"`.
fn lengths(t: &toml::Table, key: &str) -> Result<Vec<Option<f64>>, String> {
    let Some(a) = t.get(key).and_then(toml::Value::as_array) else {
        return Ok(Vec::new());
    };
    a.iter()
        .map(|v| {
            let s = v
                .as_str()
                .ok_or_else(|| format!("{key}: lengths with units"))?;
            if s.trim() == "none" {
                Ok(None)
            } else {
                s.parse::<Length>()
                    .map(|l| Some(l.si()))
                    .map_err(|e| format!("{key}: {e}"))
            }
        })
        .collect()
}

/// The design code's plugin method named by `method` (`asce10-97`,
/// `sia263-2013`, `plugin:<plugin>/<method>`), found as a run finds it.
fn method(inputs: &Inputs) -> Result<PluginMethod, String> {
    let name = inputs
        .get("method")
        .and_then(toml::Value::as_str)
        .ok_or("missing input \"method\"")?;
    let mut p = PluginMethod::parse(name)
        .ok_or_else(|| format!("method \"{name}\": not a plugin's method"))??;
    p.dir = Some(ohl_member_check::resolve(
        &p,
        &ohl_member_check::roots(None),
    )?);
    Ok(p)
}

fn case() -> Vec<jc::LoadCase> {
    vec![jc::LoadCase {
        name: "case".into(),
        strength: 1.0,
    }]
}

/// `structures.joint_parts`: a joint's plate parts — a member's bolt group
/// in a plate, a weld between two plates — checked by a design code's
/// plugin as a run checks them, with what else acts on each (a member's
/// end shear and moments, a plate's free length in compression).
struct JointParts;

impl Calculation for JointParts {
    fn id(&self) -> &'static str {
        "structures.joint_parts"
    }

    fn description(&self) -> &'static str {
        "A joint's plate parts checked by a design code's plugin (inputs: method, parts = \
         [{kind = bolts: count, diameter, hole, bolt_ultimate, lines, per_line, pitch, span, \
         end_tension, end_compression, edge, width, thickness, fy, fu, free_length; kind = \
         weld: weld = fillet|full_penetration, throat, length, angle (degrees), fy, \
         thicknesses; both: force (tension positive), shear, normal, and in_plane, \
         out_of_plane as numbers in N m}]); outputs usage_<i> (the part's usage in its one case, strength 1)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let p = method(inputs)?;
        let mut parts = Vec::new();
        let mut extra = Vec::new();
        for (i, t) in tables(inputs, "parts").into_iter().enumerate() {
            let kind = t
                .get("kind")
                .and_then(toml::Value::as_str)
                .unwrap_or("bolts");
            let force = need::<Force>(t, "force")?;
            let (bolts, steel, weld) = if kind == "weld" {
                (
                    None,
                    None,
                    Some(jc::Weld {
                        kind: t
                            .get("weld")
                            .and_then(toml::Value::as_str)
                            .unwrap_or("fillet")
                            .to_owned(),
                        throat: q::<Length>(t, "throat")?,
                        length: need::<Length>(t, "length")?,
                        length_given: true,
                        angle: number(t, "angle").unwrap_or(90.0),
                        fy: q::<Stress>(t, "fy")?,
                        thicknesses: lengths(t, "thicknesses")?
                            .into_iter()
                            .map(|x| x.unwrap_or(0.0))
                            .collect(),
                    }),
                )
            } else {
                (
                    Some(jc::BoltGroup {
                        count: count(t, "count", 1),
                        diameter: need::<Length>(t, "diameter")?,
                        hole: q::<Length>(t, "hole")?,
                        bolt_ultimate: q::<Stress>(t, "bolt_ultimate")?,
                        lines: count(t, "lines", 1),
                        per_line: count(t, "per_line", 1),
                        pitch: q::<Length>(t, "pitch")?,
                        span: q::<Length>(t, "span")?.unwrap_or(0.0),
                        end_tension: q::<Length>(t, "end_tension")?,
                        end_compression: q::<Length>(t, "end_compression")?,
                        edge: q::<Length>(t, "edge")?,
                        width: need::<Length>(t, "width")?,
                        width_note: "given".into(),
                    }),
                    Some(jc::PlateSteel {
                        thickness: need::<Length>(t, "thickness")?,
                        fy: q::<Stress>(t, "fy")?,
                        fu: q::<Stress>(t, "fu")?,
                    }),
                    None,
                )
            };
            parts.push(jc::Part {
                id: i as u32,
                joint: "J".into(),
                plate: "P".into(),
                kind: kind.to_owned(),
                other: "M".into(),
                bolts,
                steel,
                weld,
                forces: vec![force],
                notes: Vec::new(),
            });
            let moments = [
                q::<Force>(t, "shear")?.unwrap_or(0.0),
                number(t, "in_plane").unwrap_or(0.0),
                number(t, "out_of_plane").unwrap_or(0.0),
                q::<Force>(t, "normal")?.unwrap_or(0.0),
            ];
            extra.push(mc::Extra {
                moments: if moments.iter().any(|m| *m != 0.0) {
                    vec![moments]
                } else {
                    Vec::new()
                },
                free_length: q::<Length>(t, "free_length")?,
            });
        }
        let input = jc::JointInput {
            structure: "validation".into(),
            cases: case(),
            parts,
        };
        let results = match mc::check_joints_with(&p, &input, &extra).0 {
            JointsChecked::Results(r) => r,
            JointsChecked::NotChecked(e) => return Err(e),
        };
        let mut out = Outputs::new();
        for r in results {
            if let Some(u) = r.usages.first() {
                out.insert(format!("usage_{}", r.id), u.usage);
            }
        }
        Ok(out)
    }
}

/// `structures.joint_groups`: a joint's bolt groups — each bolt with its
/// force and its distances in the plies it bears on, its pull and the
/// flanges it bends when in tension — checked by a design code's plugin as
/// a run checks them.
struct JointGroups;

impl Calculation for JointGroups {
    fn id(&self) -> &'static str {
        "structures.joint_groups"
    }

    fn description(&self) -> &'static str {
        "A joint's bolt groups checked by a design code's plugin (inputs: method, groups = \
         [{plies = [{kind = member|plate, thickness, fy, fu, redundant, known}], bolts = \
         [{diameter, hole, bolt_ultimate, shear_capacity, grade, planes, force, ends, sides, \
         ahead (per ply, \"none\" for no edge), pull, tension_capacity, tees = [{thickness, \
         fy, edge, web, width}]}]}]); outputs usage_<i> (the group's usage in its one case, \
         strength 1)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let p = method(inputs)?;
        let mut groups = Vec::new();
        let mut tensions = Vec::new();
        for (i, g) in tables(inputs, "groups").into_iter().enumerate() {
            let plies = tables(g, "plies")
                .into_iter()
                .enumerate()
                .map(|(k, y)| {
                    Ok(gr::Ply {
                        part: format!("ply{k}"),
                        kind: y
                            .get("kind")
                            .and_then(toml::Value::as_str)
                            .unwrap_or("plate")
                            .to_owned(),
                        thickness: need::<Length>(y, "thickness")?,
                        fy: q::<Stress>(y, "fy")?,
                        fu: q::<Stress>(y, "fu")?,
                        redundant: y
                            .get("redundant")
                            .and_then(toml::Value::as_bool)
                            .unwrap_or(false),
                        known: y
                            .get("known")
                            .and_then(toml::Value::as_bool)
                            .unwrap_or(true),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let mut bolts = Vec::new();
            let mut loads = Vec::new();
            let mut tension = Vec::new();
            let mut pulls = Vec::new();
            for b in tables(g, "bolts") {
                bolts.push(gr::GroupBolt {
                    diameter: need::<Length>(b, "diameter")?,
                    hole: q::<Length>(b, "hole")?,
                    bolt_ultimate: q::<Stress>(b, "bolt_ultimate")?,
                    shear_capacity: q::<Force>(b, "shear_capacity")?,
                    shear_capacity_shank: None,
                    threads_in_shear: None,
                    grade: b
                        .get("grade")
                        .and_then(toml::Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    planes: count(b, "planes", 1),
                    carries: vec!["M".into()],
                    at: (0.0, 0.0),
                });
                loads.push(gr::BoltLoad {
                    force: q::<Force>(b, "force")?.unwrap_or(0.0),
                    ends: lengths(b, "ends")?,
                    sides: lengths(b, "sides")?,
                    ahead: lengths(b, "ahead")?,
                });
                let tees = tables(b, "tees")
                    .into_iter()
                    .enumerate()
                    .map(|(k, x)| {
                        Ok(gt::Tee {
                            part: format!("flange{k}"),
                            thickness: need::<Length>(x, "thickness")?,
                            fy: q::<Stress>(x, "fy")?,
                            edge: need::<Length>(x, "edge")?,
                            web: need::<Length>(x, "web")?,
                            width: need::<Length>(x, "width")?,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                tension.push(gt::BoltTension {
                    tension_capacity: q::<Force>(b, "tension_capacity")?,
                    tees,
                    unknown: Vec::new(),
                });
                pulls.push(q::<Force>(b, "pull")?.unwrap_or(0.0));
            }
            if pulls.iter().any(|x| *x > 0.0) {
                tensions.push(gt::GroupTension {
                    id: i as u32,
                    bolts: tension,
                    loads: vec![pulls],
                });
            }
            groups.push(gr::Group {
                id: i as u32,
                joint: "J".into(),
                kind: g
                    .get("kind")
                    .and_then(toml::Value::as_str)
                    .unwrap_or("bearing")
                    .to_owned(),
                part: "M".into(),
                plies,
                bolts,
                loads: vec![loads],
                notes: Vec::new(),
            });
        }
        let input = gr::GroupInput {
            structure: "validation".into(),
            parameters: String::new(),
            cases: vec![gr::LoadCase {
                name: "case".into(),
                strength: 1.0,
            }],
            groups,
        };
        let checked = if tensions.is_empty() {
            mc::check_joint_groups(&p, &input)
        } else {
            let (c, sent) = mc::check_joint_groups_tension(&p, &input, &tensions);
            if !sent {
                return Err("the plugin has no check of bolts in tension".into());
            }
            c
        };
        let results = match checked {
            GroupsChecked::Results(r) => r,
            GroupsChecked::NotChecked(e) => return Err(e),
        };
        let mut out = Outputs::new();
        for r in results {
            if let Some(u) = r.usages.first() {
                out.insert(format!("usage_{}", r.id), u.usage);
            }
        }
        Ok(out)
    }
}

/// `structures.bolt_distances`: one bolt's minimum end distance, spacing
/// and edge distance in a connected part by a design code's plugin, as the
/// bolt tools and the joint layout ask for them (2026-11-24: the force per
/// bolt without an analysis, O44 L16's question 4).
struct BoltDistances;

impl Calculation for BoltDistances {
    fn id(&self) -> &'static str {
        "structures.bolt_distances"
    }

    fn description(&self) -> &'static str {
        "One bolt's minimum distances in a connected part by a design code's plugin (inputs: \
         method, diameter, thickness, ultimate (the part's), sheared (true: a sheared edge, \
         false: rolled), redundant, optional analysis_force, optional bolts = {count, planes, \
         shear_capacity, bolt_ultimate}); outputs end_m, spacing_m, edge_m, bolt_force_n (the \
         force per bolt they took; absent: none)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let p = method(inputs)?;
        let flag = |k: &str| {
            inputs
                .get(k)
                .and_then(toml::Value::as_bool)
                .unwrap_or(false)
        };
        let bolts = match inputs.get("bolts").and_then(toml::Value::as_table) {
            None => None,
            Some(b) => Some(bd::Bolts {
                count: count(b, "count", 1),
                shear_planes: count(b, "planes", 1),
                shear_capacity: q::<Force>(b, "shear_capacity")?,
                shear_capacity_shank: None,
                ultimate: q::<Stress>(b, "bolt_ultimate")?,
                threads_in_shear: None,
                grade: String::new(),
            }),
        };
        let input = bd::DistanceInput {
            diameter: need::<Length>(inputs, "diameter")?,
            thickness: need::<Length>(inputs, "thickness")?,
            ultimate: q::<Stress>(inputs, "ultimate")?,
            analysis_force: q::<Force>(inputs, "analysis_force")?,
            bolts,
            sheared_edge: flag("sheared"),
            redundant: flag("redundant"),
            parameters: String::new(),
        };
        let d = mc::minimum_distances_full(&p, &input)?
            .ok_or("the plugin has no bolt-distances-full export")?;
        let mut out = Outputs::new();
        out.insert("end_m".into(), d.end);
        out.insert("spacing_m".into(), d.spacing);
        out.insert("edge_m".into(), d.edge);
        if let Some(f) = d.bolt_force {
            out.insert("bolt_force_n".into(), f);
        }
        Ok(out)
    }
}
