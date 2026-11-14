// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The structure checks by analysis ([`ohl_structures_fe`]): a structure
//! type's finite-element model (its members, when they are a full design,
//! or a separate `model = "….ohlfe"`) and its pole (`pole = "id"`, a
//! `.ohlp` file next to the type, or the type's own `[pole]` table, read
//! by [`ohl_structures_fe::pole::structure_pole`]), loaded once per usage
//! run and solved with each load case's factored attachment loads. A
//! placed structure standing on an extension with a body or leg
//! extensions is solved on the model with that extension
//! ([`Model::from_structure_type_extended`]). A member's section is
//! looked up in the project's section library first, then in the type's
//! `[[section]]` tables; when the model has profiles from the library,
//! the weather case's wind on them is added (member wind).

use std::collections::BTreeMap;
use std::path::Path;

use ohl_sections::SectionLibrary;
use ohl_structures::{StructureLibrary, StructureType};
use ohl_structures_fe::check::worst;
use ohl_structures_fe::{Loads, Model, Pole, PoleLoad};
use ohl_weather::WeatherCase;

use crate::capacity::{Check, Method};
use crate::{IceOn, LoadCase};

/// The weight of ice and snow on a model's members (version 4,
/// `[case.structure] ice`, `snow`), unfactored, as loads at their joints
/// (half of each member's at each end), structure axes.
#[derive(Debug, Clone, Default)]
pub(crate) struct BodyWeights {
    /// The joint loads (down), N.
    pub nodal: Vec<ohl_structures_fe::model::NodalLoad>,
    /// The ice's weight, N.
    pub ice: f64,
    /// The snow's weight, N.
    pub snow: f64,
    /// Members iced (with a profile).
    pub iced: usize,
    /// Members without a profile (no outline: no ice, no snow on them).
    pub bare: usize,
    /// The height of the weights' centroid in the model's axes, m.
    pub height: f64,
    /// Per member, how much wider the ice makes it for the wind (2·t), m.
    pub widening: Vec<f64>,
    /// Per member its ice and snow as one force (down), N: for
    /// [`Loads::member`] (the bending between its joints).
    pub member: Vec<(usize, [f64; 3])>,
}

/// The perimeter of a profile's outline (its outer edge, an angle's
/// inside faces too), m.
pub(crate) fn outline_perimeter(profile: &ohl_sections::Section) -> f64 {
    let o = ohl_structures_fe::drawn::outline(&profile.shape);
    (0..o.len())
        .map(|k| {
            let (a, b) = (o[k], o[(k + 1) % o.len()]);
            (b[0] - a[0]).hypot(b[1] - a[1])
        })
        .sum()
}

/// The ice and snow on the members of `model` that have a profile: ice
/// all round each member's outline, ρ·g·(P·t + π·t²) per metre
/// ([`IceOn::weight_per_metre`]; conservative: an angle's inside faces
/// iced too, the iced outline not rounded); snow of `snow` (N/m², q_k)
/// on each member's plan — its width seen from above times its length
/// projected on the ground (a vertical member none; theory: Structure
/// wind, Ice and snow on the structure).
pub(crate) fn body_weights(model: &Model, ice: Option<IceOn>, snow: Option<f64>) -> BodyWeights {
    let mut out = BodyWeights {
        widening: vec![0.0; model.members.len()],
        ..BodyWeights::default()
    };
    if ice.is_none() && snow.is_none_or(|q| q <= 0.0) {
        return out;
    }
    let mut moment = 0.0;
    for (k, m) in model.members.iter().enumerate() {
        let Some(profile) = model.sections[m.section].profile.as_ref() else {
            out.bare += 1;
            continue;
        };
        let length = model.length(m);
        let mut weight = 0.0;
        if let Some(i) = ice {
            let w = i.weight_per_metre(outline_perimeter(profile)) * length;
            out.ice += w;
            out.iced += 1;
            weight += w;
            out.widening[k] = 2.0 * i.thickness;
        }
        if let Some(q) = snow.filter(|q| *q > 0.0)
            && let Some((_, sin, width)) = model.across(m, [0.0, 0.0, -1.0])
        {
            let w = q * width * length * sin;
            out.snow += w;
            weight += w;
        }
        if weight > 0.0 {
            let (a, b) = (model.nodes[m.i].point, model.nodes[m.j].point);
            moment += weight * 0.5 * (a[2] + b[2]);
            for n in [m.i, m.j] {
                out.nodal.push(ohl_structures_fe::model::NodalLoad {
                    node: n,
                    force: [0.0, 0.0, -weight / 2.0],
                    moment: [0.0; 3],
                });
            }
            out.member.push((k, [0.0, 0.0, -weight]));
        }
    }
    let total = out.ice + out.snow;
    if total > 0.0 {
        out.height = moment / total;
    }
    out
}

/// The ice on a pole's shaft above the ground (version 4): ρ·g·(P·t +
/// π·t²) per metre, P its perimeter at that height (a polygon's own,
/// corners included, 2026-11-08: before π·D, not on the safe side), as
/// vertical loads on its axis at the middle of
/// [`POLE_ICE_PIECES`] pieces of equal height; with the total weight, N.
/// `foot`: the height of the ground in the type's axes, m.
pub(crate) fn pole_ice(pole: &Pole, ice: IceOn, foot: f64) -> (Vec<PoleLoad>, f64) {
    let h = pole.height();
    let piece = h / POLE_ICE_PIECES as f64;
    let mut total = 0.0;
    let loads = (0..POLE_ICE_PIECES)
        .map(|k| {
            let z = (k as f64 + 0.5) * piece;
            let w = ice.weight_per_metre(pole.perimeter_at(z)) * piece;
            total += w;
            PoleLoad {
                point: [0.0, 0.0, foot + z],
                force: [0.0, 0.0, -w],
            }
        })
        .collect();
    (loads, total)
}

/// The pieces a pole's ice is put on.
pub(crate) const POLE_ICE_PIECES: usize = 8;

/// Factored loads at one point of the structure: its label, (transverse,
/// longitudinal, vertical down), N.
pub(crate) type Factored = (String, [f64; 3], [f64; 3]);

/// A model's key: the type id and the extension it stands on (none: as
/// written, also for an extension without a body or legs).
type ModelKey = (String, Option<String>);

/// The models and poles of the structure types, by type id (or why they
/// could not be read).
#[derive(Default)]
pub(crate) struct Analyses {
    models: BTreeMap<ModelKey, Result<Model, String>>,
    poles: BTreeMap<String, Result<Pole, String>>,
    /// The section library's wind factors (`[wind]`), when given.
    wind: Option<ohl_sections::StructureWind>,
    /// The lattice faces of each model, when the wind factors need them.
    faces: BTreeMap<ModelKey, Vec<ohl_structures_fe::wind::Face>>,
    /// Why a model's plugin (its file's member check) was not found: its
    /// check is refused with this, never made by another method.
    plugin_errors: BTreeMap<ModelKey, String>,
    /// The members checked by plugins ([`Analyses::check_plugins`]).
    plugin_runs: std::sync::Mutex<PluginRuns>,
}

/// The member checks by plugins of a usage run (2026-11-11): a plugin is
/// called **once per placed structure with all its load cases**, so the
/// run is made in two passes. The first ([`PluginRuns::collecting`])
/// only solves the structures whose model names a plugin and keeps their
/// solutions, in the order they are asked for; then each structure's
/// plugin is called once ([`Analyses::check_plugins`]); the second pass
/// — the run itself — takes the kept solutions and the plugin's usage in
/// the same order instead of solving again.
#[derive(Default)]
struct PluginRuns {
    /// The first pass: solve and keep, check nothing.
    collecting: bool,
    /// Per placed structure (its id; a type on its own: "" and the type
    /// id), its solves in the order asked for.
    kept: BTreeMap<(String, String), Vec<Kept>>,
    /// Per structure, how many of `kept` the second pass has taken.
    taken: BTreeMap<(String, String), usize>,
}

/// One solve of a structure whose members a plugin checks.
struct Kept {
    key: ModelKey,
    case: String,
    strength: f64,
    self_weight: f64,
    solution: Result<ohl_structures_fe::Solution, String>,
    /// The plugin's usage and its note, once it was called.
    checked: Option<Result<(Vec<ohl_structures_fe::MemberUsage>, String), String>>,
}

impl Analyses {
    /// Reads the models and poles the types name (each file once), the
    /// members' sections from `sections` first; a type with members
    /// designed also with each extension that changes its model.
    pub fn load(types: &StructureLibrary, sections: &SectionLibrary) -> Self {
        let mut out = Self::default();
        for t in types.iter() {
            let dir = types
                .path(&t.id)
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .unwrap_or_default();
            if let Some(file) = &t.model {
                out.models.insert(
                    (t.id.clone(), None),
                    Model::load_with(&dir.join(file), sections),
                );
            } else if t.members_designed()
                && let Some(path) = types.path(&t.id)
            {
                // A full design in the type's own file.
                out.models
                    .insert((t.id.clone(), None), Model::load_with(path, sections));
                for e in t.extensions.iter().filter(|e| e.changes_model()) {
                    out.models.insert(
                        (t.id.clone(), Some(e.name.clone())),
                        Model::load_extended(path, sections, Some(&e.name)),
                    );
                }
            }
            if let Some(pole) = ohl_structures_fe::pole::structure_pole(t, &dir, sections) {
                out.poles.insert(t.id.clone(), pole);
            }
        }
        out.wind = sections.wind.clone();
        if ohl_structures_fe::wind::needs_faces(out.wind.as_ref()) {
            // Once per model: the faces do not change with the case.
            for (key, model) in &out.models {
                if let Ok(m) = model
                    && m.has_profiles()
                {
                    out.faces
                        .insert(key.clone(), ohl_structures_fe::wind::faces(m));
                }
            }
        }
        out
    }

    /// Looks for the plugins the models' files name for their member
    /// check (`Plugins/` beside each type's file, then the user's plugin
    /// folder); true: at least one model names a plugin. One not found
    /// refuses that model's check, the plugin named.
    pub fn find_plugins(&mut self, types: &StructureLibrary) -> bool {
        let mut any = false;
        for (key, model) in &mut self.models {
            let Ok(m) = model else { continue };
            let Some(p) = &mut m.capacity_plugin else {
                continue;
            };
            any = true;
            let dir = types.path(&key.0).and_then(Path::parent);
            match ohl_member_check::resolve(p, &ohl_member_check::roots(dir)) {
                Ok(found) => p.dir = Some(found),
                Err(e) => {
                    self.plugin_errors.insert(key.clone(), e);
                }
            }
        }
        any
    }

    /// The first pass of a run with plugins on (solve and keep) or off.
    pub fn collect_for_plugins(&self, on: bool) {
        self.plugin_runs.lock().expect("not poisoned").collecting = on;
    }

    /// Calls each structure's plugin once with every solve kept for it
    /// in the first pass. A plugin that fails: every check of that
    /// structure is refused with its message.
    pub fn check_plugins(&self) {
        let mut runs = self.plugin_runs.lock().expect("not poisoned");
        for kept in runs.kept.values_mut() {
            let Some(key) = kept.first().map(|k| k.key.clone()) else {
                continue;
            };
            let Some(Ok(model)) = self.models.get(&key) else {
                continue;
            };
            let places: Vec<usize> = (0..kept.len())
                .filter(|k| kept[*k].solution.is_ok())
                .collect();
            let checked = {
                let solved: Vec<ohl_member_check::Solved> = places
                    .iter()
                    .filter_map(|k| {
                        let k = &kept[*k];
                        Some(ohl_member_check::Solved {
                            name: &k.case,
                            strength: k.strength,
                            self_weight: k.self_weight,
                            solution: k.solution.as_ref().ok()?,
                        })
                    })
                    .collect();
                if solved.is_empty() {
                    continue;
                }
                ohl_member_check::check(model, &solved)
            };
            match checked {
                Some(Ok(mut c)) => {
                    for (n, k) in places.iter().enumerate().rev() {
                        kept[*k].checked = Some(Ok((c.usage.remove(n), c.note.clone())));
                    }
                }
                Some(Err(e)) => {
                    for k in places {
                        kept[k].checked = Some(Err(e.clone()));
                    }
                }
                None => {}
            }
        }
    }

    /// The model a structure of type `t` standing on `extension` is
    /// checked on (the extension's own when it changes the model), with
    /// its key; none: the type has no model.
    pub(crate) fn model_of(
        &self,
        t: &StructureType,
        extension: Option<&str>,
    ) -> Option<(&Result<Model, String>, bool)> {
        let own = extension
            .filter(|e| {
                self.models
                    .contains_key(&(t.id.clone(), Some((*e).to_owned())))
            })
            .map(str::to_owned);
        let extended = own.is_some();
        self.models.get(&(t.id.clone(), own)).map(|m| (m, extended))
    }

    /// The pole of type `id`, when it has one.
    pub(crate) fn pole_of(&self, id: &str) -> Option<&Result<Pole, String>> {
        self.poles.get(id)
    }

    /// The model of type `id` standing on `extension` (none: as written),
    /// when one was built for it.
    #[cfg(test)]
    pub fn model(&self, id: &str, extension: Option<&str>) -> Option<&Model> {
        self.models
            .get(&(id.to_owned(), extension.map(str::to_owned)))?
            .as_ref()
            .ok()
    }

    /// The finite-element check of a structure of type `t` (none: the
    /// type has no model, or no member a capacity); `placed`: the
    /// structure as resolved — the extension it stands on, and its
    /// attachment points' links (an unlinked one refuses the check). Members with a
    /// profile from the section library take the structures' wind of the
    /// case ([`LoadCase::structure_wind`]: across the line, its pressure
    /// times its factor — by default the weather case's pressure and the
    /// transverse factor); the type's wind bands are never in the model,
    /// so the body wind is not counted twice.
    pub fn fe(
        &self,
        t: &StructureType,
        placed: Option<&ohl_line::Structure>,
        loads: &[Factored],
        case: &LoadCase,
        weather: Option<&WeatherCase>,
        notes: &mut Vec<String>,
    ) -> Option<Check> {
        let extension = placed.and_then(|s| s.extension.as_deref());
        // Its own model when the extension changes it.
        let key = (
            t.id.clone(),
            extension
                .filter(|e| {
                    self.models
                        .contains_key(&(t.id.clone(), Some((*e).to_owned())))
                })
                .map(str::to_owned),
        );
        let model = match self.models.get(&key)? {
            Ok(m) => m,
            Err(e) => {
                notes.push(format!("model not read: {e}"));
                return None;
            }
        };
        // The members are a plugin's to check: in the first pass of the
        // run only these structures are solved (and kept for the
        // plugin's one call).
        let by_plugin = model.capacity_plugin.is_some();
        let collecting = self.plugin_runs.lock().expect("not poisoned").collecting;
        if collecting && !by_plugin {
            return None;
        }
        let run_key = (placed.map_or(String::new(), |s| s.id.clone()), t.id.clone());
        // Which form the member ends were taken in, as OHL-Structure says
        // it (joint details asked for below level 4: not applied, said).
        if let Some(n) = model.joint_use.note() {
            let n = format!("model: {n}");
            if !notes.contains(&n) {
                notes.push(n);
            }
        }
        let mut nodal = Vec::new();
        // The loads along members put on their joints as halves (ice,
        // snow, member wind): for the bending between joints.
        let mut spread: Vec<(usize, [f64; 3])> = Vec::new();
        let refused = |why: String| {
            Some(Check {
                method: Method::Fe,
                usage: f64::INFINITY,
                governs: format!("model: {why}"),
                incomplete: false,
                approximate: false,
                not_safe: 0,
            })
        };
        for (label, [tr, lo, ve], extra) in loads {
            let force = [*tr, *lo, -ve];
            // The placed structure's attachment point: its own link (a
            // relink, an offset) over the type's (2026-10-21).
            let point = placed.and_then(|s| s.point(label));
            if point.is_some_and(|p| p.unlinked) {
                return refused(format!(
                    "attachment point {label} is not linked to a joint of the type: its \
                     loads are refused until it is linked"
                ));
            }
            let at = match point.and_then(|p| Some((p.joint.as_deref()?, p.offset))) {
                Some((joint, offset)) => model.linked_load(label, joint, offset, force),
                None => model.point_load(label, force),
            };
            match at {
                Some(mut at) => {
                    // The moment of the loads acting away from the point (a
                    // post insulator's top and middle, owner 2026-11-14):
                    // at the joint where a frame member meets it, else left
                    // out and said (as an offset's).
                    if extra.iter().any(|m| m.abs() > 1e-9) {
                        match at.loads.first_mut() {
                            Some(n) if model.frame_at(n.node) => {
                                for (sum, m) in n.moment.iter_mut().zip(extra) {
                                    *sum += m;
                                }
                            }
                            _ => notes.push(format!(
                                "model: attachment point {label}: the moment of the post \
                                 insulator on it ({:.2} kN m) left out — NOT ON THE SAFE \
                                 SIDE: only trusses meet its joint (make it a frame joint)",
                                extra.iter().map(|m| m * m).sum::<f64>().sqrt() / 1e3
                            )),
                        }
                    }
                    nodal.extend(at.loads);
                    notes.extend(at.note.map(|n| format!("model: {n}")));
                }
                // An attachment point no joint of the model carries:
                // refused, not left out.
                None if point.is_some() => {
                    return refused(format!(
                        "attachment point {label}: no joint of the model carries it"
                    ));
                }
                None => notes.push(format!(
                    "model: no node for point {label} (its load left out)"
                )),
            }
        }
        // Ice and snow on the members (version 4), times the vertical
        // factor; the ice widens the members for the wind.
        let mut widening = Vec::new();
        if let Some(w) = weather
            && case.ices_or_snows(w)
        {
            let ice = match case.structure_ice_at(w, placed) {
                Ok(i) => i,
                Err(e) => {
                    notes.push(format!("model: {e}: no ice"));
                    None
                }
            };
            let snow = case.structure_snow_at(placed);
            if case.on_structure.snow && snow.is_none() {
                notes.push(
                    "model: snow: no snow on the ground given at this structure (local \
                     snow_ground or snow_altitude): none"
                        .into(),
                );
            }
            let weights = body_weights(model, ice, snow.map(|s| s.load()));
            if let Some(i) = ice {
                notes.push(format!(
                    "model: {}ice {:.0} mm of {:.0} kg/m³ on {} member(s), {:.2} kN{}",
                    if case.takes_local_ice(w) {
                        "the structure's local "
                    } else {
                        ""
                    },
                    i.thickness * 1000.0,
                    i.density,
                    weights.iced,
                    weights.ice / 1000.0,
                    if weights.bare > 0 {
                        format!(
                            " ({} member(s) without a profile from the cross-section library: no ice on them, not on the safe side)",
                            weights.bare
                        )
                    } else {
                        String::new()
                    }
                ));
            }
            if let Some(s) = snow {
                notes.push(format!(
                    "model: snow q = μ·C_e·C_T·s_k = {:.2} × {:.2} kN/m² = {:.2} kN/m² on the \
                     members' plan, {:.2} kN",
                    s.factor,
                    s.ground / 1000.0,
                    s.load() / 1000.0,
                    weights.snow / 1000.0
                ));
                notes.push(format!(
                    "model: NOT ON THE SAFE SIDE: {}",
                    ohl_structures::SNOW_BRIDGING
                ));
            }
            nodal.extend(weights.nodal.iter().map(|l| {
                let mut l = *l;
                l.force[2] *= case.vertical;
                l
            }));
            spread.extend(
                weights
                    .member
                    .iter()
                    .map(|(k, f)| (*k, f.map(|x| x * case.vertical))),
            );
            if ice.is_some() {
                widening = weights.widening;
            }
        }
        if let Some(w) = weather
            && model.has_profiles()
        {
            // The wind blows from the case's direction (by default across
            // the line, from the right: to −x). The pressure and factor
            // are the structures' of the load case
            // (LoadCase::structure_wind_at); drag and shielding factors
            // from the section library's [wind] when given (else the
            // model's generic drag, no shielding).
            let body = case.structure_wind_at(w, placed);
            let setup = ohl_structures_fe::wind::MemberWindSetup::with_faces(
                model,
                self.wind.as_ref(),
                self.faces.get(&key).cloned().unwrap_or_default(),
            );
            let [tx, ty] = body.towards_xy();
            let wind = setup.wind_iced(body.factored(), [tx, ty, 0.0], &widening);
            if !wind.loads.is_empty() {
                let total = wind.total();
                notes.push(format!(
                    "model: member wind from the section profiles, {:.1} kN {}{}{}",
                    total[0].hypot(total[1]) / 1000.0,
                    if body.has_longitudinal() {
                        format!(
                            "{} ({:.1} kN along the line)",
                            body.direction.words(),
                            total[1].abs() / 1000.0
                        )
                    } else {
                        "across the line".to_owned()
                    },
                    if t.wind.is_some() {
                        " (the type's wind bands not in the model)"
                    } else {
                        ""
                    },
                    if body.is_own() {
                        format!(
                            " ({}: {} Pa × {})",
                            body.origin(),
                            body.pressure,
                            body.factor
                        )
                    } else {
                        String::new()
                    }
                ));
                notes.extend(wind.notes.iter().map(|n| format!("model: {n}")));
                spread.extend(wind.member_forces());
                nodal.extend(wind.loads);
            }
        }
        let loads = Loads {
            nodal,
            member: spread,
            self_weight: if model.analysis.self_weight {
                case.vertical
            } else {
                0.0
            },
        };
        if let Some(e) = self.plugin_errors.get(&key) {
            // The plugin is not installed (or has no such method): the
            // check is refused, the plugin named.
            return refused(e.clone());
        }
        let solve = || {
            ohl_structures_fe::analyse(
                model,
                &loads,
                case.strength,
                &ohl_structures_fe::default_budget(),
            )
            .map_err(|e| e.to_string())
        };
        let mut plugin_note = None;
        let solved = if !by_plugin {
            solve()
        } else if collecting {
            let solution = solve().map(|(s, _)| s);
            self.plugin_runs
                .lock()
                .expect("not poisoned")
                .kept
                .entry(run_key)
                .or_default()
                .push(Kept {
                    key: key.clone(),
                    case: case.name.clone(),
                    strength: case.strength,
                    self_weight: loads.self_weight,
                    solution,
                    checked: None,
                });
            return None;
        } else {
            // The second pass: the solve kept, with the plugin's usage.
            let mut runs = self.plugin_runs.lock().expect("not poisoned");
            let n = {
                let n = runs.taken.entry(run_key.clone()).or_default();
                *n += 1;
                *n - 1
            };
            match runs.kept.get_mut(&run_key).and_then(|k| k.get_mut(n)) {
                Some(k) => match (
                    std::mem::replace(&mut k.solution, Err("taken".into())),
                    k.checked.take(),
                ) {
                    (Err(e), _) => Err(e),
                    (Ok(_), Some(Err(e))) => Err(e),
                    (Ok(s), Some(Ok((usage, note)))) => {
                        plugin_note = Some(note);
                        Ok((s, usage))
                    }
                    // No plugin was called: the core's own words (every
                    // member not checked).
                    (Ok(s), None) => {
                        let usage = ohl_structures_fe::member_usage(model, &s, case.strength);
                        Ok((s, usage))
                    }
                },
                // Not kept in the first pass (it never asked for this
                // solve): solved now, every member not checked, said.
                None => solve(),
            }
        };
        match solved {
            Ok((solution, usage)) => {
                if let Some(note) = plugin_note {
                    let n = format!("model: {note}");
                    if !notes.contains(&n) {
                        notes.push(n);
                    }
                }
                if let Some(note) = ohl_structures_fe::dead_load_note(model) {
                    notes.push(format!("model: {note}"));
                }
                if let Some(note) = ohl_structures_fe::stabilisation_note(model, &solution) {
                    notes.push(format!("model: {note}"));
                }
                if let Some(note) = ohl_structures_fe::tension_only_note(model, &solution) {
                    notes.push(format!("model: {note}"));
                }
                // Tension-only members kept active (their set did not
                // settle, the solver's last resort): the check is
                // approximate. Bowed and slack ones are in the note above.
                if let Some(note) = ohl_structures_fe::kept_active_note(model, &solution) {
                    notes.push(format!("model: {note}"));
                }
                if let Some(note) = ohl_structures_fe::convergence_note(&solution) {
                    notes.push(format!("model: {note}"));
                }
                // Eccentric member ends (connection shifts): those whose
                // moment was left out at a joint only bars meet.
                if let Some(note) = ohl_structures_fe::eccentricity_note(model, &solution) {
                    notes.push(format!("model: {note}"));
                }
                // The simple check is said (unsafe); ASCE 10-97, the
                // default, says its assumptions in what governs.
                if model.analysis.capacity == ohl_structures_fe::CapacityMethod::Simple {
                    notes.push(format!(
                        "model: {}",
                        ohl_structures_fe::check::capacity_note(model, &usage)
                    ));
                }
                // Members whose check is not on the safe side: said, with
                // which (owner 2026-11-08).
                let unsafe_members: Vec<String> = usage
                    .iter()
                    .filter(|u| ohl_structures_fe::check::not_safe(&u.governs))
                    .map(|u| model.members[u.member].id.clone())
                    .collect();
                if !unsafe_members.is_empty() {
                    const NAMED: usize = 10;
                    notes.push(format!(
                        "model: NOT ON THE SAFE SIDE: {} member(s) with part of their check not \
                         done (their usage may be too low; what governs each says which): {}{}",
                        unsafe_members.len(),
                        unsafe_members[..unsafe_members.len().min(NAMED)].join(", "),
                        if unsafe_members.len() > NAMED {
                            ", …"
                        } else {
                            ""
                        }
                    ));
                }
                // Members a plugin was to check and gave no result for
                // (or no plugin ran): never passing, said with which.
                let unchecked: Vec<String> = usage
                    .iter()
                    .filter(|u| {
                        u.usage.is_none() && u.governs.starts_with(ohl_member_check::NOT_CHECKED)
                    })
                    .map(|u| model.members[u.member].id.clone())
                    .collect();
                if !unchecked.is_empty() {
                    const NAMED: usize = 10;
                    notes.push(format!(
                        "model: {}: {} member(s) without a result of the member check{}: {}{}",
                        ohl_member_check::NOT_CHECKED,
                        unchecked.len(),
                        model
                            .capacity_plugin
                            .as_ref()
                            .map_or(String::new(), |p| format!(" ({})", p.name())),
                        unchecked[..unchecked.len().min(NAMED)].join(", "),
                        if unchecked.len() > NAMED { ", …" } else { "" }
                    ));
                }
                match worst(&usage) {
                    Some(w) => Some(Check {
                        method: Method::Fe,
                        usage: w.usage.unwrap_or(0.0),
                        governs: format!("member {} {}", model.members[w.member].id, w.governs),
                        incomplete: false,
                        approximate: !solution.kept_active.is_empty(),
                        not_safe: usage
                            .iter()
                            .filter(|u| ohl_structures_fe::check::not_safe(&u.governs))
                            .count()
                            + unchecked.len(),
                    }),
                    // No member checked at all: not a pass.
                    None if !unchecked.is_empty() => Some(Check {
                        method: Method::Fe,
                        usage: f64::INFINITY,
                        governs: format!(
                            "model: {}: no member has a result of the member check",
                            ohl_member_check::NOT_CHECKED
                        ),
                        incomplete: false,
                        approximate: false,
                        not_safe: unchecked.len(),
                    }),
                    None => {
                        notes.push("model: no member has a capacity".into());
                        None
                    }
                }
            }
            Err(e) => Some(Check {
                method: Method::Fe,
                usage: f64::INFINITY,
                governs: format!("model: {e}"),
                incomplete: false,
                approximate: false,
                not_safe: 0,
            }),
        }
    }

    /// The pole check of a structure of type `t`: the loads at the type's
    /// attachment points, the structures' wind of the case on the pole
    /// ([`LoadCase::structure_wind`], times the pole's drag).
    pub fn pole(
        &self,
        t: &StructureType,
        placed: Option<&ohl_line::Structure>,
        loads: &[Factored],
        case: &LoadCase,
        weather: Option<&WeatherCase>,
        notes: &mut Vec<String>,
    ) -> Option<Check> {
        let pole = match self.poles.get(&t.id)? {
            Ok(p) => p,
            Err(e) => {
                notes.push(e.clone());
                return None;
            }
        };
        let mut on_pole = Vec::new();
        for (label, [tr, lo, ve], extra) in loads {
            // A post insulator's moment (owner, 2026-11-14): the pole
            // takes forces at points only — said.
            if extra.iter().any(|m| m.abs() > 1e-9) {
                notes.push(format!(
                    "pole: point {label}: the moment of the post insulator on it ({:.2} kN m) \
                     left out — NOT ON THE SAFE SIDE",
                    extra.iter().map(|m| m * m).sum::<f64>().sqrt() / 1e3
                ));
            }
            match t.point_local(label, None) {
                Some(point) => on_pole.push(PoleLoad {
                    point,
                    force: [*tr, *lo, -ve],
                }),
                None => notes.push(format!(
                    "pole: point {label} not in the type (its load left out)"
                )),
            }
        }
        // Ice on the shaft (version 4), times the vertical factor; the
        // wind on the iced pole: its pressure times (D + 2t)/D at the top,
        // the largest ratio (conservative).
        let mut iced = 1.0;
        if let Some(w) = weather {
            match case.structure_ice_at(w, placed) {
                Ok(Some(ice)) => {
                    let foot = t.foot.map_or(0.0, |f| f.si());
                    let (ice_loads, total) = pole_ice(pole, ice, foot);
                    on_pole.extend(ice_loads.into_iter().map(|mut l| {
                        l.force[2] *= case.vertical;
                        l
                    }));
                    let top = pole.diameter_at(pole.height());
                    if top > 0.0 {
                        iced = (top + 2.0 * ice.thickness) / top;
                    }
                    notes.push(format!(
                        "pole: {}ice {:.0} mm of {:.0} kg/m³ on the shaft, {:.2} kN; its wind \
                         times {iced:.3} ((D + 2t)/D at the top, conservative)",
                        if case.takes_local_ice(w) {
                            "the structure's local "
                        } else {
                            ""
                        },
                        ice.thickness * 1000.0,
                        ice.density,
                        total / 1000.0
                    ));
                }
                Ok(None) => {}
                Err(e) => notes.push(format!("pole: {e}: no ice")),
            }
            if case.on_structure.snow {
                notes.push("pole: no snow on a pole (no horizontal surface)".into());
            }
        }
        // The wind blows from the case's direction (by default across the
        // line, from the right: to −x).
        let wind = weather.map_or([0.0; 2], |w| {
            let body = case.structure_wind_at(w, placed);
            if body.is_own() && body.factored() > 0.0 {
                notes.push(format!(
                    "pole: wind {} Pa × {} ({})",
                    body.pressure,
                    body.factor,
                    body.origin()
                ));
            }
            let [tx, ty] = body.towards_xy();
            [tx * body.factored() * iced, ty * body.factored() * iced]
        });

        Some(
            // The pole's own weight in the P-Delta model times the
            // vertical factor, as the members' own weight in the fe check
            // (physics review 2026-10-26).
            match ohl_structures_fe::check_pole_factored(
                pole,
                &on_pole,
                wind,
                case.strength,
                case.vertical,
                true,
                &ohl_structures_fe::default_budget(),
            ) {
                Ok(c) => Check {
                    method: Method::Pole,
                    usage: c.usage,
                    // Without P-Delta (no elasticity): not all on the
                    // safe side, never shown as passing.
                    not_safe: usize::from(ohl_structures_fe::check::not_safe(&c.governs)),
                    governs: format!("pole {}: {}", pole.id, c.governs),
                    incomplete: false,
                    approximate: false,
                },
                Err(e) => Check {
                    method: Method::Pole,
                    usage: f64::INFINITY,
                    governs: e,
                    incomplete: false,
                    approximate: false,
                    not_safe: 0,
                },
            },
        )
    }
}
