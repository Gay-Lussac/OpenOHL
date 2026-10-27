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

use crate::LoadCase;
use crate::capacity::{Check, Method};

/// Factored loads at one point of the structure: its label, (transverse,
/// longitudinal, vertical down), N.
pub(crate) type Factored = (String, [f64; 3]);

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
        let mut nodal = Vec::new();
        let refused = |why: String| {
            Some(Check {
                method: Method::Fe,
                usage: f64::INFINITY,
                governs: format!("model: {why}"),
                incomplete: false,
                approximate: false,
            })
        };
        for (label, [tr, lo, ve]) in loads {
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
                Some(at) => {
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
        if let Some(w) = weather
            && model.has_profiles()
        {
            // The wind blows across the line: from the right, to −x. The
            // pressure and factor are the structures' of the load case
            // (LoadCase::structure_wind); drag and shielding factors from
            // the section library's [wind] when given (else the model's
            // generic drag, no shielding).
            let body = case.structure_wind(w);
            let setup = ohl_structures_fe::wind::MemberWindSetup::with_faces(
                model,
                self.wind.as_ref(),
                self.faces.get(&key).cloned().unwrap_or_default(),
            );
            let wind = setup.wind(body.factored(), [body.towards, 0.0, 0.0]);
            if !wind.loads.is_empty() {
                let total = wind.total()[0];
                notes.push(format!(
                    "model: member wind from the section profiles, {:.1} kN across the line{}{}",
                    total.abs() / 1000.0,
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
                nodal.extend(wind.loads);
            }
        }
        let loads = Loads {
            nodal,
            self_weight: if model.analysis.self_weight {
                case.vertical
            } else {
                0.0
            },
        };
        match ohl_structures_fe::analyse(
            model,
            &loads,
            case.strength,
            &ohl_structures_fe::default_budget(),
        ) {
            Ok((solution, usage)) => {
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
                // The simple check is said (unsafe); ASCE 10-97, the
                // default, says its assumptions in what governs.
                if model.analysis.capacity == ohl_structures_fe::CapacityMethod::Simple {
                    notes.push(format!(
                        "model: {}",
                        ohl_structures_fe::check::capacity_note(model, &usage)
                    ));
                }
                match worst(&usage) {
                    Some(w) => Some(Check {
                        method: Method::Fe,
                        usage: w.usage.unwrap_or(0.0),
                        governs: format!("member {} {}", model.members[w.member].id, w.governs),
                        incomplete: false,
                        approximate: !solution.kept_active.is_empty(),
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
            }),
        }
    }

    /// The pole check of a structure of type `t`: the loads at the type's
    /// attachment points, the structures' wind of the case on the pole
    /// ([`LoadCase::structure_wind`], times the pole's drag).
    pub fn pole(
        &self,
        t: &StructureType,
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
        for (label, [tr, lo, ve]) in loads {
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
        // The wind blows across the line: from the right, to −x.
        let wind = weather.map_or([0.0; 2], |w| {
            let body = case.structure_wind(w);
            if body.is_own() && body.factored() > 0.0 {
                notes.push(format!(
                    "pole: wind {} Pa × {} ({})",
                    body.pressure,
                    body.factor,
                    body.origin()
                ));
            }
            [body.towards * body.factored(), 0.0]
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
                },
            },
        )
    }
}
