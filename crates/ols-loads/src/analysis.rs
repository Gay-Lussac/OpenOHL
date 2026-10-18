// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The structure checks by analysis ([`ols_structures_fe`]): a structure
//! type's finite-element model (its members, when they are a full design,
//! or a separate `model = "….olsfe"`) and its pole (`pole = "id"`, a
//! `.olsp` file next to the type, or the type's own `[pole]` table, read
//! by [`ols_structures_fe::pole::structure_pole`]), loaded once per usage
//! run and solved with each load case's factored attachment loads. A
//! placed structure standing on an extension with a body or leg
//! extensions is solved on the model with that extension
//! ([`Model::from_structure_type_extended`]). A member's section is
//! looked up in the project's section library first, then in the type's
//! `[[section]]` tables; when the model has profiles from the library,
//! the weather case's wind on them is added (member wind).

use std::collections::BTreeMap;
use std::path::Path;

use ols_sections::SectionLibrary;
use ols_structures::{StructureLibrary, StructureType};
use ols_structures_fe::check::worst;
use ols_structures_fe::{Loads, Model, Pole, PoleLoad};
use ols_weather::WeatherCase;

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
            if let Some(pole) = ols_structures_fe::pole::structure_pole(t, &dir, sections) {
                out.poles.insert(t.id.clone(), pole);
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

    /// The finite-element check of a structure of type `t` standing on
    /// `extension` (none: the type has no model, or no member a
    /// capacity). Members with a
    /// profile from the section library take the wind of `weather`
    /// (across the line, times the transverse factor); the type's wind
    /// bands are never in the model, so the body wind is not counted
    /// twice.
    pub fn fe(
        &self,
        t: &StructureType,
        extension: Option<&str>,
        loads: &[Factored],
        case: &LoadCase,
        weather: Option<&WeatherCase>,
        notes: &mut Vec<String>,
    ) -> Option<Check> {
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
        for (label, [tr, lo, ve]) in loads {
            match model.point_loads(label, [*tr, *lo, -ve]) {
                Some(at) => nodal.extend(at),
                None => notes.push(format!(
                    "model: no node for point {label} (its load left out)"
                )),
            }
        }
        if let Some(w) = weather
            && model.has_profiles()
        {
            // The wind blows across the line: from the right, to −x.
            let wind = model.member_wind(
                w.wind_pressure.si() * case.transverse,
                [-w.wind_from.to_left(), 0.0, 0.0],
            );
            if !wind.is_empty() {
                let total: f64 = wind.iter().map(|l| l.force[0]).sum();
                notes.push(format!(
                    "model: member wind from the section profiles, {:.1} kN across the line{}",
                    total.abs() / 1000.0,
                    if t.wind.is_some() {
                        " (the type's wind bands not in the model)"
                    } else {
                        ""
                    }
                ));
                nodal.extend(wind);
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
        match ols_structures_fe::analyse(
            model,
            &loads,
            case.strength,
            &ols_structures_fe::default_budget(),
        ) {
            Ok((solution, usage)) => {
                if let Some(note) = ols_structures_fe::stabilisation_note(model, &solution) {
                    notes.push(format!("model: {note}"));
                }
                if let Some(note) = ols_structures_fe::tension_only_note(model, &solution) {
                    notes.push(format!("model: {note}"));
                }
                // Tension-only members kept active (their set did not
                // settle): the check is approximate.
                if let Some(note) = ols_structures_fe::kept_active_note(model, &solution) {
                    notes.push(format!("model: {note}"));
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
    /// attachment points, the wind of `weather` on the pole.
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
            [
                -w.wind_from.to_left() * w.wind_pressure.si() * case.transverse,
                0.0,
            ]
        });
        Some(
            match ols_structures_fe::check_pole(
                pole,
                &on_pole,
                wind,
                case.strength,
                true,
                &ols_structures_fe::default_budget(),
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
