// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The loads on the structures' own bodies listed per structure: the wind
//! on their wind bands ([`crate::add_structure_wind`]) and, version 4,
//! the ice and snow on them (owner 2026-11-03: "a load case should be
//! able to apply ice and snow on a pylon structure").

use ohl_line::Line;
use ohl_weather::WeatherCase;

use crate::analysis::{Analyses, body_weights, pole_ice};
use crate::{AttachmentLoad, CaseLoads};

/// [`crate::add_structure_wind`], and the ice and snow of each case on
/// the structures taking it ([`crate::LoadCase::structure_ice_at`]: with
/// the local conditions taken and a weather case with ice, each
/// structure's local ice too,
/// [`crate::LoadCase::structure_snow_at`]), each a load without a point,
/// down, at the weights' centroid on the structure's axis, labelled "ice
/// on the structure" and "snow on the structure", factored by the
/// vertical factor:
///
/// - a type with a structure model (its members, or `model = "….ohlfe"`):
///   on the members with a profile (the project's `sections` first, then
///   the type's), as the finite-element check takes them
///   (`analysis::body_weights`): ice all round each member's outline,
///   snow on its plan;
/// - a type with a pole: the ice on its shaft (no snow);
/// - else (wind bands or attachment points only): none — the type gives
///   no surface to put them on; a note says which structures.
///
/// These rows are data for the reports and the checks that take the
/// body's loads: the finite-element and pole checks compute the same
/// weights on their members and shaft themselves; the allowable loads at
/// the attachments (Method 2) do not take loads without a point (as the
/// wind bands).
///
/// Expected cost: reads each type's model once (as the usage check).
pub fn add_structure_loads(
    results: &mut [CaseLoads],
    line: &Line,
    types: &ohl_structures::StructureLibrary,
    sections: &ohl_sections::SectionLibrary,
    weather: &[WeatherCase],
) {
    crate::add_structure_wind(results, line, types, weather);
    let weather_of = |r: &CaseLoads| weather.iter().find(|w| w.name == r.case.weather);
    if !results
        .iter()
        .any(|r| weather_of(r).is_some_and(|w| r.case.ices_or_snows(w)))
    {
        return;
    }
    let analyses = Analyses::load(types, sections);
    for r in results {
        let Some(w) = weather_of(r) else {
            continue;
        };
        if !r.case.ices_or_snows(w) {
            continue;
        }
        let mut without: Vec<&str> = Vec::new();
        let mut no_snow: Vec<&str> = Vec::new();
        for s in &line.structures {
            if !line.takes_load_case(&s.id, &r.case.name) {
                continue;
            }
            let ice = match r.case.structure_ice_at(w, Some(s)) {
                Ok(i) => i,
                Err(e) => {
                    if !r.notes.contains(&e) {
                        r.notes.push(e);
                    }
                    None
                }
            };
            let snow = r.case.structure_snow_at(Some(s));
            if r.case.on_structure.snow && snow.is_none() {
                no_snow.push(&s.id);
            }
            if ice.is_none() && snow.is_none() {
                continue;
            }
            let Some(t) = s.structure_type.as_deref().and_then(|t| types.get(t)) else {
                without.push(&s.id);
                continue;
            };
            let dz = s
                .extension
                .as_deref()
                .and_then(|e| t.extension(e))
                .map_or(0.0, |e| e.dz.si());
            // (weight, label, height above the structure's base).
            let mut rows: Vec<(f64, &str, f64)> = Vec::new();
            if let Some((Ok(model), extended)) = analyses.model_of(t, s.extension.as_deref()) {
                let weights = body_weights(model, ice, snow.map(|x| x.load()));
                let up = if extended { 0.0 } else { dz };
                if weights.iced == 0 && weights.bare > 0 {
                    // No member with a profile: no outline, no plan.
                    without.push(&s.id);
                }
                if weights.ice > 0.0 {
                    rows.push((weights.ice, "ice on the structure", weights.height + up));
                }
                if weights.snow > 0.0 {
                    rows.push((weights.snow, "snow on the structure", weights.height + up));
                }
            } else if let Some(Ok(pole)) = analyses.pole_of(&t.id) {
                if let Some(i) = ice {
                    let (_, total) = pole_ice(pole, i, 0.0);
                    rows.push((total, "ice on the structure", pole.height() / 2.0 + dz));
                }
            } else {
                without.push(&s.id);
            }
            for (weight, label, z) in rows {
                let local = [0.0, 0.0, weight];
                r.loads.push(AttachmentLoad {
                    structure: s.id.clone(),
                    point: String::new(),
                    label: label.to_owned(),
                    sets: Vec::new(),
                    sections: Vec::new(),
                    at: [s.position[0], s.position[1], s.position[2] + z],
                    force: [0.0, 0.0, -weight],
                    local,
                    factored: [0.0, 0.0, weight * r.case.vertical],
                    broken_span: None,
                    pull_back: [0.0; 2],
                    pull_ahead: [0.0; 2],
                    pulls_by_set: Vec::new(),
                });
            }
        }
        if !no_snow.is_empty() {
            r.notes.push(format!(
                "snow on the structures: no snow on the ground given (local snow_ground or \
                 snow_altitude) at {}: none there",
                no_snow.join(", ")
            ));
        }
        if !without.is_empty() {
            r.notes.push(format!(
                "ice and snow on the structures not counted at {} (their type has no members \
                 with a profile from the cross-section library and no pole: no surface to put \
                 them on — not on the safe side)",
                without.join(", ")
            ));
        }
    }
}
