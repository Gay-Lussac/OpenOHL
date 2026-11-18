// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The joints of a structure whose member check is a plugin's method
//! (2026-11-11; built, in active development, not approved yet): the
//! plates', covers', welds' and bolt groups' checks by the plugin's
//! optional `joint-check` export, and the bolt tools' minimum distances
//! by its optional `bolt-distances` export.
//!
//! - **Never another code in its place.** A plugin without the
//!   `joint-check` export, or one that fails: every joint part is
//!   [`not_checked_by`] the plugin, without a usage. A plugin without
//!   `bolt-distances`: no minimum distances, said ([`bolt_rules`]'s
//!   error) — not ASCE 10-97's, not SIA 263's (the bolt tools then keep
//!   the structure's default minimums, said as not a code's).
//! - **The force per bolt** without an analysis is the plugin's own
//!   (third round, 2026-11-18): its `bolt-distances-full` export gets the
//!   end's bolts and the structure's parameter file and works out the
//!   bolt's design shear capacity by its code. A plugin built without it
//!   gets the bolt's shear capacity as ASCE 10-97 4.3.2 gives it, as
//!   before ([`ohl_structures_fe::bolt_rules::shear_without_plugin`]).
//! - **One call per structure** for the joint parts, with every load
//!   case ([`check_joints`]), with the structure's parameter file for the
//!   plugin when it has the `joint-check-full` export (2026-11-18), else by
//!   `joint-check` (the method's own values).
//! - **Where the plugin is.** A model built from a file's text alone does
//!   not know its folder: the plugin is looked for where structures were
//!   opened or run in this session ([`crate::roots`] remembers the
//!   folders), then in the user's plugin folder ([`installed`]).
//! - The minimum distances are asked once per different bolt and part in
//!   a session (kept: the bolt tools ask at every redraw).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use ohl_plugins::bolt_distances as dist;
pub use ohl_plugins::bolt_distances::DistanceInput;
use ohl_plugins::bolt_distances_full as dist_full;
use ohl_plugins::joint_check as wit;
pub use ohl_plugins::joint_check::{JointInput, LoadCase, Part, PartResult};
use ohl_plugins::joint_check_full as full;
use ohl_plugins::joint_check_moments as mom;
use ohl_structures::connections::WeldKind;
use ohl_structures_fe::Model;
use ohl_structures_fe::bolt_rules::{Basis, BoltRules, Force};
use ohl_structures_fe::model::PluginMethod;
use ohl_structures_fe::plates::{BoltGroup, PlateSteel, WeldInput};

/// Said for a joint part the plugin `p` names did not check: "NOT CHECKED
/// by the plugin, NOT ON THE SAFE SIDE".
pub fn not_checked_by(p: &PluginMethod) -> String {
    format!("NOT CHECKED by {}, NOT ON THE SAFE SIDE", p.plugin)
}

type Found = HashMap<(String, String, usize), Result<PathBuf, String>>;

static FOUND: Mutex<Option<Found>> = Mutex::new(None);

/// Where the plugin `p` names is installed: where its runner found it
/// ([`crate::find`]), else looked for in `Plugins/` beside the structures
/// opened or run in this session and in the user's plugin folder (once
/// per plugin and set of folders). Err: [`crate::resolve`]'s message.
pub fn installed(p: &PluginMethod) -> Result<PathBuf, String> {
    if let Some(dir) = &p.dir {
        return Ok(dir.clone());
    }
    let roots = crate::session_roots();
    let key = (p.plugin.clone(), p.method.clone(), roots.len());
    let mut found = FOUND
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    found
        .get_or_insert_with(HashMap::new)
        .entry(key)
        .or_insert_with(|| crate::resolve(p, &roots))
        .clone()
}

/// A bolt group as the interface takes it.
pub fn bolt_group(g: &BoltGroup) -> wit::BoltGroup {
    wit::BoltGroup {
        count: g.count,
        diameter: g.diameter,
        hole: g.hole,
        bolt_ultimate: g.bolt_ultimate,
        lines: g.lines,
        per_line: g.per_line,
        pitch: g.pitch,
        span: g.span,
        end_tension: g.end[0],
        end_compression: g.end[1],
        edge: g.edge,
        width: g.width,
        width_note: g.width_words.clone(),
    }
}

/// A plate's steel as the interface takes it.
pub fn plate_steel(s: &PlateSteel) -> wit::PlateSteel {
    wit::PlateSteel {
        thickness: s.thickness,
        fy: s.fy,
        fu: s.fu,
    }
}

/// A weld as the interface takes it.
pub fn weld(w: &WeldInput) -> wit::Weld {
    wit::Weld {
        kind: match w.kind {
            WeldKind::Fillet => "fillet",
            WeldKind::FullPenetration => "full_penetration",
        }
        .to_owned(),
        throat: w.size,
        length: w.length,
        length_given: w.length_given,
        angle: w.angle,
        fy: w.fy,
        thicknesses: w.thickness.to_vec(),
    }
}

/// What a plugin's joint check gave.
#[derive(Debug, Clone)]
pub enum JointsChecked {
    /// The plugin is not found, has no `joint-check` export, or failed:
    /// why, in words — every part is then [`not_checked_by`] it.
    NotChecked(String),
    /// Its results, by part number.
    Results(Vec<wit::PartResult>),
}

static JOINT_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// How many times [`check_joints`] called a plugin's `check-joints` in
/// this process (the tests of "one call per structure").
pub fn joint_calls() -> usize {
    JOINT_CALLS.load(std::sync::atomic::Ordering::SeqCst)
}

/// The joint parts of `input` checked by the plugin `p` names: one call
/// with every part and load case (nothing beside the parts' forces sent:
/// [`check_joints_with`] sends the rest).
pub fn check_joints(p: &PluginMethod, input: &wit::JointInput) -> JointsChecked {
    check_joints_with(p, input, &[]).0
}

/// What acts on one part beside its force (2026-11-18), for the plugin's
/// `joint-check-moments` export: per load case (as the part's `forces`)
/// `[shear, in-plane, out-of-plane, normal]` (sizes, N and N·m; the
/// interface's `part-moments`), and a bolt group's plate's free length
/// in compression, m.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Extra {
    /// Per load case: shear, in-plane moment, out-of-plane moment, normal
    /// force (empty: nothing else acts).
    pub moments: Vec<[f64; 4]>,
    /// The plate's free length in compression (a bolt group), m.
    pub free_length: Option<f64>,
}

impl Extra {
    /// Something acts on the part beside its force in some case.
    pub fn acts(&self) -> bool {
        self.moments.iter().any(|m| m.iter().any(|x| *x > 0.0))
    }
}

/// [`check_joints`] with what acts on each part beside its force
/// (`extra`, one per part of `input` in its order, or empty): through the
/// plugin's `joint-check-moments` when it has it (true with the results),
/// else `joint-check-full` or `joint-check` without it (false: the caller
/// says, on each part something else acts on, that it was not sent).
pub fn check_joints_with(
    p: &PluginMethod,
    input: &wit::JointInput,
    extra: &[Extra],
) -> (JointsChecked, bool) {
    let dir = match installed(p) {
        Ok(d) => d,
        Err(e) => return (JointsChecked::NotChecked(e), false),
    };
    let host = match crate::host() {
        Ok(h) => h,
        Err(e) => {
            return (
                JointsChecked::NotChecked(format!("the plugin host does not start: {e}")),
                false,
            );
        }
    };
    JOINT_CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let params = p.parameters.as_ref().map_or("", |x| x.1.as_str());
    // Every force on a part (`joint-check-moments`, 2026-11-18), else
    // with the structure's parameter file when the plugin takes it
    // (`joint-check-full`, 2026-11-18), else `joint-check` (the method's
    // own values).
    let moments = host
        .check_joints_moments(&dir, &p.method, &moments_input(input, params, extra))
        .map(|r| r.map(|r| r.into_iter().map(moments_result).collect::<Vec<_>>()));
    let (given, sent) = match moments {
        Ok(Some(r)) => (Ok(Some(r)), true),
        Err(e) => (Err(e), false),
        Ok(None) => {
            let full = host
                .check_joints_full(&dir, &p.method, &full_input(input, params))
                .map(|r| r.map(|r| r.into_iter().map(plain_result).collect::<Vec<_>>()));
            let given = match full {
                Ok(Some(r)) => Ok(Some(r)),
                Ok(None) => host.check_joints(&dir, &p.method, input),
                Err(e) => Err(e),
            };
            (given, false)
        }
    };
    let checked = match given {
        Ok(Some(r)) => JointsChecked::Results(r),
        Ok(None) => JointsChecked::NotChecked(format!(
            "the plugin {} has no joint check (no joint-check export)",
            p.plugin
        )),
        Err(e) => JointsChecked::NotChecked(format!("the joint check failed: {e}")),
    };
    (checked, sent)
}

/// `input` as `joint-check-moments` takes it, with the plugin's
/// `parameters` (TOML; "" none) and what else acts on each part (`extra`,
/// by place; a part without one: nothing).
fn moments_input(input: &wit::JointInput, parameters: &str, extra: &[Extra]) -> mom::JointInput {
    let f = full_input(input, parameters);
    mom::JointInput {
        structure: f.structure,
        parameters: f.parameters,
        cases: f
            .cases
            .into_iter()
            .map(|c| mom::LoadCase {
                name: c.name,
                strength: c.strength,
            })
            .collect(),
        parts: f
            .parts
            .into_iter()
            .enumerate()
            .map(|(k, p)| {
                let x = extra.get(k);
                mom::Part {
                    id: p.id,
                    joint: p.joint,
                    plate: p.plate,
                    kind: p.kind,
                    other: p.other,
                    bolts: p.bolts.map(|b| mom::BoltGroup {
                        count: b.count,
                        diameter: b.diameter,
                        hole: b.hole,
                        bolt_ultimate: b.bolt_ultimate,
                        lines: b.lines,
                        per_line: b.per_line,
                        pitch: b.pitch,
                        span: b.span,
                        end_tension: b.end_tension,
                        end_compression: b.end_compression,
                        edge: b.edge,
                        width: b.width,
                        width_note: b.width_note,
                    }),
                    steel: p.steel.map(|s| mom::PlateSteel {
                        thickness: s.thickness,
                        fy: s.fy,
                        fu: s.fu,
                    }),
                    weld: p.weld.map(|w| mom::Weld {
                        kind: w.kind,
                        throat: w.throat,
                        length: w.length,
                        length_given: w.length_given,
                        angle: w.angle,
                        fy: w.fy,
                        thicknesses: w.thicknesses,
                    }),
                    forces: p.forces,
                    notes: p.notes,
                    moments: x.map_or(Vec::new(), |x| {
                        x.moments
                            .iter()
                            .map(|m| mom::PartMoments {
                                shear: m[0],
                                in_plane: m[1],
                                out_of_plane: m[2],
                                normal: m[3],
                            })
                            .collect()
                    }),
                    free_length: x.and_then(|x| x.free_length),
                }
            })
            .collect(),
    }
}

/// A `joint-check-moments` result as `joint-check`'s (the same record).
fn moments_result(r: mom::PartResult) -> wit::PartResult {
    wit::PartResult {
        id: r.id,
        usages: r
            .usages
            .into_iter()
            .map(|u| wit::CaseUsage {
                load_case: u.load_case,
                usage: u.usage,
                check: u.check,
            })
            .collect(),
        governs: r.governs,
        notes: r.notes,
        not_safe: r.not_safe,
    }
}

/// `input` as `joint-check-full` takes it, with the plugin's `parameters`
/// (TOML; "" none).
fn full_input(input: &wit::JointInput, parameters: &str) -> full::JointInput {
    full::JointInput {
        structure: input.structure.clone(),
        parameters: parameters.to_owned(),
        cases: input
            .cases
            .iter()
            .map(|c| full::LoadCase {
                name: c.name.clone(),
                strength: c.strength,
            })
            .collect(),
        parts: input
            .parts
            .iter()
            .map(|p| full::Part {
                id: p.id,
                joint: p.joint.clone(),
                plate: p.plate.clone(),
                kind: p.kind.clone(),
                other: p.other.clone(),
                bolts: p.bolts.as_ref().map(|b| full::BoltGroup {
                    count: b.count,
                    diameter: b.diameter,
                    hole: b.hole,
                    bolt_ultimate: b.bolt_ultimate,
                    lines: b.lines,
                    per_line: b.per_line,
                    pitch: b.pitch,
                    span: b.span,
                    end_tension: b.end_tension,
                    end_compression: b.end_compression,
                    edge: b.edge,
                    width: b.width,
                    width_note: b.width_note.clone(),
                }),
                steel: p.steel.map(|s| full::PlateSteel {
                    thickness: s.thickness,
                    fy: s.fy,
                    fu: s.fu,
                }),
                weld: p.weld.as_ref().map(|w| full::Weld {
                    kind: w.kind.clone(),
                    throat: w.throat,
                    length: w.length,
                    length_given: w.length_given,
                    angle: w.angle,
                    fy: w.fy,
                    thicknesses: w.thicknesses.clone(),
                }),
                forces: p.forces.clone(),
                notes: p.notes.clone(),
            })
            .collect(),
    }
}

/// A `joint-check-full` result as `joint-check`'s (the same record).
fn plain_result(r: full::PartResult) -> wit::PartResult {
    wit::PartResult {
        id: r.id,
        usages: r
            .usages
            .into_iter()
            .map(|u| wit::CaseUsage {
                load_case: u.load_case,
                usage: u.usage,
                check: u.check,
            })
            .collect(),
        governs: r.governs,
        notes: r.notes,
        not_safe: r.not_safe,
    }
}

/// The minimum distances of one bolt, by bits of the input (kept for the
/// session).
type Distances =
    HashMap<(PathBuf, String, [u64; 4], bool, bool), Result<Option<dist::Distances>, String>>;

static DISTANCES: Mutex<Option<Distances>> = Mutex::new(None);

/// The plugin's minimum distances for `input`. Err: why there are none —
/// "no minimum distances from the plugin: …".
pub fn minimum_distances(
    p: &PluginMethod,
    input: &dist::DistanceInput,
) -> Result<dist::Distances, String> {
    let none = |why: String| format!("no minimum distances from {}: {why}", p.plugin);
    let dir = installed(p).map_err(none)?;
    let key = (
        dir.clone(),
        p.method.clone(),
        [
            input.diameter.to_bits(),
            input.thickness.to_bits(),
            input.ultimate.map_or(u64::MAX, f64::to_bits),
            input.bolt_force.map_or(u64::MAX, f64::to_bits),
        ],
        input.sheared_edge,
        input.redundant,
    );
    let mut kept = DISTANCES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let given = kept
        .get_or_insert_with(HashMap::new)
        .entry(key)
        .or_insert_with(|| {
            let host = crate::host().map_err(|e| format!("the plugin host does not start: {e}"))?;
            match host.minimum_distances(&dir, &p.method, input) {
                Ok(Some(d)) => Ok(d),
                Ok(None) => Err("the plugin has no bolt-distances export".to_owned()),
                Err(e) => Err(e.to_string()),
            }
        })
        .clone();
    given
        .map_err(none)?
        .ok_or_else(|| none("its method gives none for this bolt".to_owned()))
}

/// The minimum distances of one bolt with its bolts and the parameter
/// file, by the text of the input (kept for the session).
type FullDistances =
    HashMap<(PathBuf, String, String), Result<Option<dist_full::Distances>, String>>;

static FULL_DISTANCES: Mutex<Option<FullDistances>> = Mutex::new(None);

/// The input's every value, exactly (the cache's key).
fn full_key(i: &dist_full::DistanceInput) -> String {
    let bits = |x: Option<f64>| x.map_or("-".to_owned(), |v| format!("{:x}", v.to_bits()));
    let bolts = i.bolts.as_ref().map_or("-".to_owned(), |b| {
        format!(
            "{}|{}|{}|{}|{}|{:?}|{}",
            b.count,
            b.shear_planes,
            bits(b.shear_capacity),
            bits(b.shear_capacity_shank),
            bits(b.ultimate),
            b.threads_in_shear,
            b.grade
        )
    });
    format!(
        "{}|{}|{}|{}|{bolts}|{}|{}|{}",
        bits(Some(i.diameter)),
        bits(Some(i.thickness)),
        bits(i.ultimate),
        bits(i.analysis_force),
        i.sheared_edge,
        i.redundant,
        i.parameters
    )
}

/// The plugin's minimum distances for `input` by its `bolt-distances-full`
/// export (third round, 2026-11-18). Ok(None): the plugin has no such
/// export (ask `bolt-distances`). Err: why there are none — "no minimum
/// distances from the plugin: …".
pub fn minimum_distances_full(
    p: &PluginMethod,
    input: &dist_full::DistanceInput,
) -> Result<Option<dist_full::Distances>, String> {
    let none = |why: String| format!("no minimum distances from {}: {why}", p.plugin);
    let dir = installed(p).map_err(none)?;
    let key = (dir.clone(), p.method.clone(), full_key(input));
    let mut kept = FULL_DISTANCES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let given = kept
        .get_or_insert_with(HashMap::new)
        .entry(key)
        .or_insert_with(|| {
            let host = crate::host().map_err(|e| format!("the plugin host does not start: {e}"))?;
            match host.minimum_distances_full(&dir, &p.method, input) {
                Ok(Some(Some(d))) => Ok(Some(d)),
                Ok(Some(None)) => Err("its method gives none for this bolt".to_owned()),
                Ok(None) => Ok(None),
                Err(e) => Err(e.to_string()),
            }
        })
        .clone();
    given.map_err(none)
}

/// The minimum distances at member `idx`'s end `end` of `model` (whose
/// file names a plugin's method) from that plugin, as the bolt tools take
/// them (arguments as [`ohl_structures_fe::bolt_rules::rules_in`]). Err:
/// the words to show instead — "no minimum distances from the plugin: …" —
/// never another code's values. None: the model names no plugin, or the
/// end has no bolts of known diameter.
///
/// The plugin's `bolt-distances-full` first (third round, 2026-11-18): the
/// end's bolts and the structure's parameter file sent, the force per bolt
/// the analysis' or else the plugin's own (the bolt's design shear
/// capacity by its code); a plugin without it by `bolt-distances` with
/// the analysis' force, else the bolt's shear capacity as ASCE 10-97 4.3.2
/// gives it (as before; said).
pub fn bolt_rules(
    model: &Model,
    idx: usize,
    end: usize,
    force: Option<f64>,
    part: Option<(f64, Option<f64>)>,
) -> Option<Result<BoltRules, String>> {
    let p = model.capacity_plugin.as_ref()?;
    let Basis {
        diameter,
        thickness,
        ultimate,
        analysis,
        bolts,
        leg,
        root,
        sheared,
        redundant,
    } = ohl_structures_fe::bolt_rules::basis(model, idx, end, force, part)?;
    let input = dist_full::DistanceInput {
        diameter,
        thickness,
        ultimate,
        analysis_force: analysis,
        bolts: bolts.map(|b| dist_full::Bolts {
            count: b.count,
            shear_planes: b.shear_planes,
            shear_capacity: b.shear_capacity,
            shear_capacity_shank: b.shear_capacity_shank,
            ultimate: b.ultimate,
            threads_in_shear: b.threads_in_shear,
            grade: b.grade.unwrap_or_default(),
        }),
        sheared_edge: sheared,
        redundant,
        parameters: p
            .parameters
            .as_ref()
            .map(|x| x.1.clone())
            .unwrap_or_default(),
    };
    // A bundled code by its name in files ("asce10-97"), another plugin's
    // method with its plugin.
    let code = p.alias().map_or_else(
        || format!("{} of the plugin {}", p.method, p.plugin),
        str::to_owned,
    );
    let rules = |end_d: f64,
                 spacing: f64,
                 edge: f64,
                 clauses: Vec<String>,
                 force: Force,
                 force_basis: String| {
        let mut clauses = clauses.into_iter();
        let mut next = || clauses.next().unwrap_or_default();
        BoltRules {
            code: code.clone(),
            end: end_d,
            spacing,
            edge,
            clauses: [next(), next(), next()],
            force,
            force_basis,
            diameter,
            leg,
            root,
            sheared,
            assumed: None,
        }
    };
    let full = match minimum_distances_full(p, &input) {
        Err(e) => return Some(Err(e)),
        Ok(full) => full,
    };
    if let Some(d) = full {
        let force = match (analysis, d.bolt_force) {
            (Some(a), _) => Force::Analysis(a),
            (None, Some(v)) => Force::Capacity(v),
            (None, None) => Force::Unknown,
        };
        return Some(Ok(rules(
            d.end,
            d.spacing,
            d.edge,
            d.clauses,
            force,
            d.force_basis,
        )));
    }
    // A plugin built against `bolt-distances` alone: the force as before.
    let force = match (
        analysis,
        ohl_structures_fe::bolt_rules::shear_without_plugin(model, idx, end),
    ) {
        (Some(a), _) => Force::Analysis(a),
        (None, Some(v)) => Force::Capacity(v),
        (None, None) => Force::Unknown,
    };
    let input = dist::DistanceInput {
        diameter,
        thickness,
        ultimate,
        bolt_force: force.value(),
        sheared_edge: sheared,
        redundant,
    };
    Some(minimum_distances(p, &input).map(|d| {
        rules(
            d.end,
            d.spacing,
            d.edge,
            d.clauses,
            force,
            if matches!(force, Force::Capacity(_)) {
                "the bolt's shear capacity as ASCE 10-97 4.3.2 gives it (the plugin works out \
                 none of its own)"
                    .to_owned()
            } else {
                String::new()
            },
        )
    }))
}
