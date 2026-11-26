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
//! - **Everything the core's checks read** (2026-11-18): a plugin with
//!   the `member-check-full` export is sent the full records ([`full`]):
//!   the sections' and profiles' values, the ends, the loads along each
//!   member, bowed and kept-active members, the redundant-force check's
//!   braced members, and the structure's **parameter file** for the
//!   plugin (`Plugins/<plugin>.toml` beside the structure). Its usage may
//!   be empty (no capacity), infinite or not a number (failing), as the
//!   core's own methods report.
//! - **Foot movements** (2026-11-18): a case solved with and without the
//!   structure's measured foot movements ([`Solved::moved`]) is sent as
//!   two cases in the same call, and each member keeps the larger usage
//!   of the two ([`ohl_structures_fe::feet::larger_usage`]) — as the
//!   core's own methods take them.
//! - **The solve's cap** (2026-11-18): a tension-only member's compression
//!   is capped by the plugin's own compression capacity, asked once when
//!   the plugin is found ([`find`], [`prepare`]), never another code's.
//! - **Bundled plugins** (2026-11-18): the design codes that were the
//!   core's own (`[analysis] capacity = "asce10-97"`, the default also when
//!   none is given, and `"sia263-2013"`,
//!   [`ohl_structures_fe::model::BUNDLED_ALIASES`]) are plugins shipped
//!   with OpenOHL, found in its own folder ([`bundled_root`]) before any
//!   other and only there; one that does not load is an error at start
//!   ([`check_bundled`]), never a fallback to another code; they are given
//!   a longer time and more memory a call than user plugins ([`host`]).
//! - **Their settings everywhere** (2026-11-18): the structure's parameter
//!   file reaches the joint check (`joint-check-moments`, else
//!   `joint-check-full`, [`joints`]) and
//!   Member details ([`with_parameters`]) as it reaches the member check.
//!
//! This crate is the one place that knows both the finite-element
//! results and the plugin host: `ohl-structures-fe` runs no plugins, and
//! `ohl-plugins` knows no model.

use std::path::{Path, PathBuf};

pub mod default_bolts;
pub mod full;
pub mod joints;
pub mod limits;

use ohl_plugins::member_check as wit;
use ohl_sections::Shape;
use ohl_structures_fe::check::MemberUsage;
use ohl_structures_fe::design::Onto;
use ohl_structures_fe::model::PluginMethod;
use ohl_structures_fe::{Model, Solution};

/// The plugin host of the member checks: one for the process, so that a
/// plugin is compiled once and not at every call (2026-11-18: 0.7 s each
/// for a plugin of 400 kB; a plugin's file changed is compiled again).
pub fn host() -> Result<&'static ohl_plugins::WasmHost, String> {
    static HOST: std::sync::OnceLock<Result<ohl_plugins::WasmHost, String>> =
        std::sync::OnceLock::new();
    // The bundled plugins (OpenOHL's own design codes) get their longer
    // time a call ([`ohl_plugins::Limits::BUNDLED_CALL_TIMEOUT`]); every
    // other plugin keeps its own limits.
    HOST.get_or_init(|| {
        ohl_plugins::WasmHost::new()
            .map(|h| h.with_bundled_root(bundled_root()))
            .map_err(|e| e.to_string())
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// Said for a member the plugin gave nothing for.
pub const NOT_CHECKED: &str = "NOT CHECKED, NOT ON THE SAFE SIDE";

/// The folder beside a structure file where its plugins are looked for.
pub const BESIDE: &str = "Plugins";

/// Where OpenOHL's bundled plugins are (`plugins/codes`, holding one
/// folder per plugin as built: its manifest and its WebAssembly file):
/// beside the program or above it (a build in the source tree's or a
/// package's folder), else above the current folder (the tests). None:
/// not found.
pub fn bundled_root() -> Option<PathBuf> {
    let first = ohl_structures_fe::model::BUNDLED_ALIASES.first()?.1;
    let within = |start: PathBuf| {
        start
            .ancestors()
            .map(|a| a.join("plugins").join("codes"))
            .find(|d| ohl_plugins::manifest_path(&d.join(first)).is_file())
    };
    std::env::current_exe()
        .ok()
        .and_then(within)
        .or_else(|| std::env::current_dir().ok().and_then(within))
}

/// Whether `plugin` is the name of a plugin bundled with OpenOHL: looked
/// for in [`bundled_root`] only (never in a user's folder: another
/// plugin of that name cannot take its place).
pub fn is_bundled(plugin: &str) -> bool {
    ohl_structures_fe::model::BUNDLED_ALIASES
        .iter()
        .any(|a| a.1 == plugin)
}

/// The bundled plugins checked as the applications start: each found,
/// loading, giving its method through `member-check-full`. Err: which does
/// not, and what that means — an error at start (owner 2026-11-17), never a
/// fallback to another code.
pub fn check_bundled() -> Result<(), String> {
    let refused = |what: &str, why: String| {
        format!(
            "the bundled plugin {what} does not load: {why}. Structures naming its method \
             cannot be checked — nothing else is used in its place; reinstall OpenOHL (or, in \
             the source tree, build plugins/codes/{what}: cargo build --release --target \
             wasm32-wasip2)"
        )
    };
    let root = bundled_root();
    for (_, plugin, method) in ohl_structures_fe::model::BUNDLED_ALIASES {
        let Some(root) = &root else {
            return Err(refused(
                plugin,
                "OpenOHL's plugins/codes folder is not found beside the program".into(),
            ));
        };
        let dir = root.join(plugin);
        let host = crate::host().map_err(|e| refused(plugin, e.to_string()))?;
        let methods = host
            .member_check_methods(&dir)
            .map_err(|e| refused(plugin, e.to_string()))?;
        if !methods.iter().any(|m| m.name == *method) {
            return Err(refused(
                plugin,
                format!(
                    "{} gives no method {method} (not built, or built without the member-check \
                     export)",
                    dir.display()
                ),
            ));
        }
        if !host
            .has_member_check_full(&dir)
            .map_err(|e| refused(plugin, e.to_string()))?
        {
            return Err(refused(plugin, "built without member-check-full".into()));
        }
    }
    Ok(())
}

/// The folders plugins are looked for in: `Plugins/` beside the
/// structure, then the user's plugin folder (a bundled plugin is looked
/// for before them, in [`bundled_root`] only: [`resolve`]).
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
    let Ok(host) = crate::host() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    // The bundled plugins first, from OpenOHL's own folder only.
    if let Some(root) = bundled_root() {
        let mut seen = Vec::new();
        for (_, plugin, _) in ohl_structures_fe::model::BUNDLED_ALIASES {
            if seen.contains(plugin) {
                continue;
            }
            seen.push(*plugin);
            for method in host
                .member_check_methods(&root.join(plugin))
                .unwrap_or_default()
            {
                out.push(((*plugin).to_owned(), method.name, method.description));
            }
        }
    }
    for root in roots {
        for f in ohl_plugins::discover(root) {
            let Ok(m) = &f.manifest else { continue };
            if is_bundled(&m.plugin.name) {
                continue;
            }
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
    // A bundled plugin: OpenOHL's own, from its own folder only.
    let bundled;
    let roots = if is_bundled(&p.plugin) {
        bundled = bundled_root().into_iter().collect::<Vec<_>>();
        &bundled[..]
    } else {
        roots
    };
    let found = roots
        .iter()
        .flat_map(|r| {
            if is_bundled(&p.plugin) {
                ohl_plugins::discover(&r.join(&p.plugin))
            } else {
                ohl_plugins::discover(r)
            }
        })
        .find(|f| f.manifest.as_ref().is_ok_and(|m| m.plugin.name == p.plugin));
    let Some(found) = found else {
        return Err(refused(if is_bundled(&p.plugin) {
            "is bundled with OpenOHL but not found beside the program (plugins/codes): \
             reinstall OpenOHL, or in the source tree build it"
                .to_owned()
        } else {
            format!(
                "is not installed (looked in: {})",
                roots
                    .iter()
                    .map(|r| r.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }));
    };
    let host = crate::host().map_err(|e| refused(format!("cannot run: {e}")))?;
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
        p.parameters = parameters(&p.plugin, dir)?;
    }
    prepare(&mut model)?;
    Ok(model)
}

/// The plugin `model`'s file names given the structure's parameter file
/// for it (`Plugins/<plugin>.toml` beside the structure in `dir`,
/// [`parameters`]) without looking for the plugin or running it
/// (2026-11-18: the paths that ask the plugin later from a file's text —
/// Member details, the joints' checks — take the file's parameters as a
/// run does); `dir` is remembered as a folder plugins are looked for in
/// ([`roots`]). A model naming no plugin is left as it is. Err: the
/// parameter file does not read.
pub fn with_parameters(model: &mut Model, dir: Option<&Path>) -> Result<(), String> {
    if let Some(p) = &mut model.capacity_plugin {
        roots(dir);
        p.parameters = parameters(&p.plugin, dir)?;
    }
    Ok(())
}

/// The structure's parameter file for `plugin`: `Plugins/<plugin>.toml`
/// beside the structure (`dir`), its path and text; none: no such file
/// (the method's own values). Err: it does not read.
pub fn parameters(plugin: &str, dir: Option<&Path>) -> Result<Option<(PathBuf, String)>, String> {
    let Some(d) = dir else { return Ok(None) };
    let path = d.join(BESIDE).join(format!("{plugin}.toml"));
    if !path.is_file() {
        return Ok(None);
    }
    std::fs::read_to_string(&path)
        .map(|t| Some((path.clone(), t)))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Asks the plugin `model`'s file names (found: [`resolve`]) for each
/// member's compression capacity, for the solve's cap on the tension-only
/// members ([`Model::method_compression`]); a model without tension-only
/// members or without a plugin is left as it is. Err: the plugin failed —
/// the structure is then not solved by another code's cap.
pub fn prepare(model: &mut Model) -> Result<(), String> {
    let Some(p) = &model.capacity_plugin else {
        return Ok(());
    };
    if p.dir.is_none()
        || !model
            .members
            .iter()
            .any(|m| m.kind == ohl_structures_fe::MemberKind::Tension)
    {
        return Ok(());
    }
    let given = capacities(model)?;
    let by_id: std::collections::HashMap<&str, &Given> =
        given.iter().map(|g| (g.id.as_str(), g)).collect();
    model.method_compression = model
        .members
        .iter()
        .map(|m| {
            by_id.get(m.id.as_str()).and_then(|g| {
                g.capacities
                    .iter()
                    .find(|c| c.0 == "compression")
                    .map(|c| c.1)
            })
        })
        .collect();
    Ok(())
}

/// What a plugin's method gives for one member: its capacities by name
/// (value, N or the unit the name says; what governs it), the governing
/// check, notes, and what is not on the safe side.
#[derive(Debug, Clone, PartialEq)]
pub struct Given {
    /// The member's id.
    pub id: String,
    /// Its capacities: (name, value, what governs).
    pub capacities: Vec<(String, f64, String)>,
    /// The governing check, in words.
    pub governs: String,
    /// Notes (assumptions, limits).
    pub notes: Vec<String>,
    /// What is not on the safe side (none: nothing).
    pub not_safe: Option<String>,
}

impl From<&wit::MemberResult> for Given {
    fn from(r: &wit::MemberResult) -> Self {
        Self {
            id: r.id.clone(),
            capacities: r
                .capacities
                .iter()
                .map(|c| (c.name.clone(), c.value, c.governs.clone()))
                .collect(),
            governs: r.governs.clone(),
            notes: r.notes.clone(),
            not_safe: r.not_safe.clone(),
        }
    }
}

impl From<&ohl_plugins::member_check_full::MemberResult> for Given {
    fn from(r: &ohl_plugins::member_check_full::MemberResult) -> Self {
        Self {
            id: r.id.clone(),
            capacities: r
                .capacities
                .iter()
                .map(|c| (c.name.clone(), c.value, c.governs.clone()))
                .collect(),
            governs: r.governs.clone(),
            notes: r.notes.clone(),
            not_safe: r.not_safe.clone(),
        }
    }
}

/// The capacities of every checked member of `model` by the plugin its
/// file names (found: [`resolve`]), without a load case — for the solve's
/// cap ([`prepare`]) and Member details. Err: the plugin is not found or
/// failed.
pub fn capacities(model: &Model) -> Result<Vec<Given>, String> {
    let Some(p) = &model.capacity_plugin else {
        return Ok(Vec::new());
    };
    let dir = match &p.dir {
        Some(d) => d.clone(),
        None => joints::installed(p)?,
    };
    let host = crate::host().map_err(|e| format!("member check: {e}"))?;
    let fail = |e: ohl_plugins::PluginError| {
        format!(
            "member check: plugin {} (method {}): {e}",
            p.plugin, p.method
        )
    };
    let params = p.parameters.as_ref().map_or("", |x| x.1.as_str());
    match full::check_full(host, &dir, &p.method, model, &[], params).map_err(fail)? {
        Some((out, _)) => Ok(out.results.iter().map(Given::from).collect()),
        None => Ok(host
            .check_members(&dir, &p.method, &input(model, &[]))
            .map_err(fail)?
            .iter()
            .map(Given::from)
            .collect()),
    }
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
    /// Its solution (without the structure's foot movements).
    pub solution: &'a Solution,
    /// Its solution on the structure's moved feet (its measured foot
    /// movements, 2026-11-16), when it has some: each member then keeps
    /// the larger usage of the two ([`check`]).
    pub moved: Option<&'a Solution>,
}

/// The interface's records for `model` and its solved `cases`: every
/// checked member, its forces in each case.
pub fn input(model: &Model, cases: &[Solved]) -> wit::CheckInput {
    let taken = model.joint_use.taken();
    let mut members = Vec::new();
    for (k, m) in model.members.iter().enumerate() {
        if !m.check {
            continue;
        }
        let s = &model.sections[m.section];
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
                    // The moments of eccentric ends the solve could not carry
                    // (only bars meet the joint), in its check (2026-11-18).
                    end_forces: ohl_structures_fe::offsets::with_eccentric_moments(model, k, f)
                        .end_forces
                        .to_vec(),
                    partner_axial: m
                        .design
                        .crossing
                        .and_then(|p| c.solution.members.get(p))
                        .map(|p| p.axial),
                    slack: f.slack,
                }
            })
            .collect();
        members.push(wit::Member {
            id: m.id.clone(),
            kind: m.kind.as_str().to_owned(),
            class: word(d.class, |c| c.as_str()),
            shape: s
                .profile
                .as_ref()
                .map_or(String::new(), |p| p.shape.as_str().to_owned()),
            dimensions: s.profile.as_ref().map_or(Vec::new(), |p| {
                ohl_structures_fe::capacity::dimensions(&p.shape)
            }),
            parts: parts(model, k),
            area: s.area,
            iy: s.iy,
            iz: s.iz,
            iv: s.i_min,
            wy: s.wy,
            wz: s.wz,
            e: s.e,
            fy: s.yield_strength,
            fu: s.ultimate,
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
                            "{NOT_CHECKED}: plugin {} (method {}) {why} — an overload of this \
                             member would not be found",
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
    /// The members' usage, one list per case in the model's member order
    /// (a case with foot movements: each member's larger usage of with and
    /// without them).
    pub usage: Vec<Vec<MemberUsage>>,
    /// The note for the results: the method, by whom, how many members
    /// are not checked (in the worst case).
    pub note: String,
    /// The note of each case: [`Checked::note`] with what the marks in
    /// what governs say, counted as for the core's own methods.
    pub notes: Vec<String>,
    /// What the plugin returned for each member (capacities, notes).
    pub results: Vec<Given>,
}

static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// How many times [`check`] called a plugin's member check in this
/// process (the tests of "one call per structure").
pub fn calls() -> usize {
    CALLS.load(std::sync::atomic::Ordering::SeqCst)
}

/// The member check of `model` by the plugin its file names, **one call
/// for all of `cases`** (a case with foot movements sent twice, with and
/// without them). None: the file names no plugin, or the plugin was not
/// looked for ([`find`]; the core's own results then say every member is
/// not checked). Err: the plugin failed — no result, never another method
/// in its place.
pub fn check(model: &Model, cases: &[Solved]) -> Option<Result<Checked, String>> {
    let p = model.capacity_plugin.as_ref()?;
    let dir = p.dir.as_deref()?;
    Some(check_in(model, p, dir, cases))
}

/// The name of a case's solve on the structure's moved feet, as sent.
fn moved_name(name: &str) -> String {
    format!("{name} (on its moved feet)")
}

fn check_in(
    model: &Model,
    p: &PluginMethod,
    dir: &Path,
    cases: &[Solved],
) -> Result<Checked, String> {
    let host = crate::host().map_err(|e| format!("member check: {e}"))?;
    let fail = |e: ohl_plugins::PluginError| {
        format!(
            "member check: plugin {} (method {}): {e}",
            p.plugin, p.method
        )
    };
    // Every solve sent: each case, and after it its solve on the moved
    // feet when it has one.
    let names: Vec<String> = cases
        .iter()
        .flat_map(|c| std::iter::once(c.name.to_owned()).chain(c.moved.map(|_| moved_name(c.name))))
        .collect();
    let mut sent: Vec<Solved> = Vec::new();
    let mut k = 0;
    for c in cases {
        sent.push(Solved {
            name: &names[k],
            moved: None,
            ..*c
        });
        k += 1;
        if let Some(m) = c.moved {
            sent.push(Solved {
                name: &names[k],
                solution: m,
                moved: None,
                ..*c
            });
            k += 1;
        }
    }
    CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let full_cases: Vec<full::Case> = sent
        .iter()
        .map(|c| full::Case {
            name: c.name,
            strength: c.strength,
            self_weight: c.self_weight,
            solution: c.solution,
        })
        .collect();
    let params = p.parameters.as_ref().map_or("", |x| x.1.as_str());
    let (each, results, method_note) =
        match full::check_full(host, dir, &p.method, model, &full_cases, params).map_err(fail)? {
            Some((out, braced)) => (
                full::usages(model, p, &full_cases, &out.results, braced),
                out.results.iter().map(Given::from).collect::<Vec<_>>(),
                Some(out.method_note),
            ),
            None => {
                let results = host
                    .check_members(dir, &p.method, &input(model, &sent))
                    .map_err(fail)?;
                (
                    usages(model, p, &sent, &results),
                    results.iter().map(Given::from).collect(),
                    None,
                )
            }
        };
    // Back to one list per case: with foot movements, each member's
    // larger usage of the two.
    let mut each = each.into_iter();
    let mut usage = Vec::with_capacity(cases.len());
    for c in cases {
        let plain = each.next().unwrap_or_default();
        usage.push(match c.moved {
            Some(_) => {
                ohl_structures_fe::feet::larger_usage(&plain, each.next().unwrap_or_default())
            }
            None => plain,
        });
    }
    let bare = usage
        .iter()
        .map(|u| {
            u.iter()
                .filter(|u| u.governs.starts_with(NOT_CHECKED))
                .count()
        })
        .max()
        .unwrap_or(0);
    let mut note = match &method_note {
        Some(m) => format!(
            "member capacities: {m} — method {} of the plugin {}{}",
            p.method,
            p.plugin,
            match &p.parameters {
                Some((path, _)) => format!(" (parameters: {})", path.display()),
                None => String::new(),
            }
        ),
        None => format!(
            "member capacities: method {} of the plugin {} (not checked by OpenOHL)",
            p.method, p.plugin
        ),
    };
    if bare > 0 {
        note.push_str(&format!(
            "; {bare} member(s) {NOT_CHECKED}: the plugin returned nothing for them, so an \
             overload there would not be found (each member's line says what is missing)"
        ));
    }
    let notes = usage
        .iter()
        .map(|u| match &method_note {
            // The marks counted as the core's own methods count them.
            Some(_) => format!(
                "{note}{}{}",
                ohl_structures_fe::check::marks_note(u),
                ohl_structures_fe::check::shifted_note(u)
            ),
            None => note.clone(),
        })
        .collect();
    Ok(Checked {
        usage,
        note,
        notes,
        results,
    })
}
