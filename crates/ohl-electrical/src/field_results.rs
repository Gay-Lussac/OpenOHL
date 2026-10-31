// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The fields' results as JSON, format `ohl-field-results` version 1
//! (2026-10-31: the owner approved it as a core extension point for a
//! building plugin): what `ohl electrical magnetic|electric --json` writes
//! and a field-presets plugin's `report` receives. Nothing is computed
//! here: the results as they are, serialised.
//!
//! ```json
//! {"format": "ohl-field-results", "version": 1,
//!  "quantity": "B", "unit": "µT", "model": "three_d",
//!  "case": {"name": "still air", "temperature_c": 40.0},
//!  "settings": {…}, "limit": null,
//!  "cuts": [{"label", "from", "to", "at", "quantity", "unit",
//!            "plane": {"origin": [x, y], "right": [dx, dy]},
//!            "offsets", "elevations", "ground", "values",
//!            "isolines": [{"level", "lines": [[[offset, elevation], …], …]}]}],
//!  "points": [{"name", "x", "y", "z", "offset", "height", "value",
//!              "major", "margin"}],
//!  "notes": […]}
//! ```
//!
//! `quantity` is `"B"` (µT; a point's `major` the largest value over the
//! cycle) or `"E"` (kV/m; no `major`). A cut's `values` run row by row
//! (`values[row × offsets + column]`), every point computed, also under
//! the ground (`ground`: its elevation per column, null where none is
//! known); its isolines are those of the settings' levels, cut at the
//! ground. `settings` are the run's settings without the lists of places
//! (profiles, grids, points, cuts, volumes), which the results carry.

use serde_json::{Value, json};

use crate::electric::{self, ElectricField};
use crate::magnetic::{self, FieldCut, MagneticField};

/// The format's name.
pub const FORMAT: &str = "ohl-field-results";

/// The format's version.
pub const VERSION: u32 = 1;

/// The run's settings as JSON without the lists of places.
fn settings_summary(settings: Value) -> Value {
    let mut v = settings;
    if let Some(o) = v.as_object_mut() {
        for k in ["stations", "grids", "points", "cuts", "volumes"] {
            o.remove(k);
        }
    }
    v
}

fn cut_json(c: &FieldCut, quantity: &str, unit: &str, levels: &[f64]) -> Value {
    let v = &c.values;
    json!({
        "label": c.label,
        "from": c.from,
        "to": c.to,
        "at": c.at,
        "quantity": quantity,
        "unit": unit,
        "plane": {"origin": v.cut.origin, "right": v.cut.right},
        "offsets": v.offsets,
        "elevations": v.elevations,
        "ground": v.ground,
        "values": v.values,
        "isolines": levels
            .iter()
            .map(|&l| json!({"level": l, "lines": v.isolines(l)}))
            .collect::<Vec<_>>(),
    })
}

/// The magnetic field's results (`m`, run with `settings` in weather case
/// `case` at `temperature_c` °C) as `ohl-field-results` JSON.
pub fn magnetic_json(
    m: &MagneticField,
    settings: &magnetic::Settings,
    case: &str,
    temperature_c: f64,
) -> String {
    let settings_value = serde_json::to_value(settings).unwrap_or(Value::Null);
    json!({
        "format": FORMAT,
        "version": VERSION,
        "quantity": "B",
        "unit": "µT",
        "model": m.model,
        "case": {"name": case, "temperature_c": temperature_c},
        "settings": settings_summary(settings_value),
        "limit": m.limit_ut,
        "cuts": m
            .cuts
            .iter()
            .map(|c| cut_json(c, "B", "µT", &settings.levels))
            .collect::<Vec<_>>(),
        "points": m
            .points
            .iter()
            .map(|p| json!({
                "name": p.name,
                "x": p.x,
                "y": p.y,
                "z": p.z,
                "offset": p.offset,
                "height": p.height,
                "value": p.b_ut,
                "major": p.major_ut,
                "margin": p.margin_ut,
            }))
            .collect::<Vec<_>>(),
        "notes": m.notes,
    })
    .to_string()
}

/// The electric field's results (`e`, run with `settings` in weather case
/// `case` at `temperature_c` °C) as `ohl-field-results` JSON.
pub fn electric_json(
    e: &ElectricField,
    settings: &electric::Settings,
    case: &str,
    temperature_c: f64,
) -> String {
    let settings_value = serde_json::to_value(settings).unwrap_or(Value::Null);
    json!({
        "format": FORMAT,
        "version": VERSION,
        "quantity": "E",
        "unit": "kV/m",
        "model": e.model,
        "case": {"name": case, "temperature_c": temperature_c},
        "settings": settings_summary(settings_value),
        "limit": e.limit_kv_m,
        "cuts": e
            .cuts
            .iter()
            .map(|c| cut_json(c, "E", "kV/m", &settings.levels))
            .collect::<Vec<_>>(),
        "points": e
            .points
            .iter()
            .map(|p| json!({
                "name": p.name,
                "x": p.x,
                "y": p.y,
                "z": p.z,
                "offset": p.offset,
                "height": p.height,
                "value": p.e_kv_m,
                "margin": p.margin_kv_m,
            }))
            .collect::<Vec<_>>(),
        "notes": e.notes,
    })
    .to_string()
}
