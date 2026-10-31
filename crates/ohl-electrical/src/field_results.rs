// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The fields' results as JSON (2026-10-31: the owner approved it as a
//! core extension point for a building plugin): what `ohl electrical
//! magnetic|electric|fields --json` writes and a field-presets plugin's
//! `report` receives. Nothing is computed here: the results as they are,
//! serialised.
//!
//! A run's results are a **set** (format `ohl-field-results-set` version
//! 1; owner 2026-10-31, "Ok for 1 but keep it clean": one run of both
//! fields, one report): the case and one document per field computed, in
//! the order magnetic, electric.
//!
//! ```json
//! {"format": "ohl-field-results-set", "version": 1,
//!  "case": {"name": "still air", "temperature_c": 40.0},
//!  "fields": [{…ohl-field-results…}, …]}
//! ```
//!
//! Each field's document is format `ohl-field-results` version 2 (version
//! 1 had no alignment on a cut):
//!
//! ```json
//! {"format": "ohl-field-results", "version": 2,
//!  "quantity": "B", "unit": "µT", "model": "three_d",
//!  "case": {"name": "still air", "temperature_c": 40.0},
//!  "settings": {…}, "limit": null,
//!  "cuts": [{"label", "alignment", "station", "angle_point",
//!            "from", "to", "at", "quantity", "unit",
//!            "plane": {"origin": [x, y], "right": [dx, dy]},
//!            "offsets", "elevations", "ground", "values",
//!            "isolines": [{"level", "lines": [[[offset, elevation], …], …]}]}],
//!  "points": [{"name", "x", "y", "z", "offset", "height", "value",
//!              "major", "margin"}],
//!  "notes": […]}
//! ```
//!
//! `quantity` is `"B"` (µT; a point's `major` the largest value over the
//! cycle) or `"E"` (kV/m; no `major`). A cut is placed at `station` m of
//! `alignment`, its plane perpendicular to it (at an angle point,
//! `angle_point`, along the bisector); `from`, `to` are the span it
//! crosses at the axis and `at` where it crosses its chord (m from
//! `from`). Its `values` run row by row (`values[row × offsets +
//! column]`), every point computed, also under the ground (`ground`: its
//! elevation per column, null where none is known); its isolines are
//! those of the settings' levels, cut at the ground. `settings` are the
//! run's settings without the lists of places (profiles, grids, points,
//! cuts, volumes), which the results carry.

use serde_json::{Value, json};

use crate::electric::{self, ElectricField};
use crate::magnetic::{self, FieldCut, MagneticField};

/// A field's document: the format's name.
pub const FORMAT: &str = "ohl-field-results";

/// A field's document: the format's version.
pub const VERSION: u32 = 2;

/// A run's set of documents: the format's name.
pub const SET_FORMAT: &str = "ohl-field-results-set";

/// A run's set of documents: the format's version.
pub const SET_VERSION: u32 = 1;

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
        "alignment": c.alignment,
        "station": c.station,
        "angle_point": c.angle_point,
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
    magnetic_value(m, settings, case, temperature_c).to_string()
}

/// [`magnetic_json`] as a JSON value.
pub fn magnetic_value(
    m: &MagneticField,
    settings: &magnetic::Settings,
    case: &str,
    temperature_c: f64,
) -> Value {
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
}

/// The electric field's results (`e`, run with `settings` in weather case
/// `case` at `temperature_c` °C) as `ohl-field-results` JSON.
pub fn electric_json(
    e: &ElectricField,
    settings: &electric::Settings,
    case: &str,
    temperature_c: f64,
) -> String {
    electric_value(e, settings, case, temperature_c).to_string()
}

/// [`electric_json`] as a JSON value.
pub fn electric_value(
    e: &ElectricField,
    settings: &electric::Settings,
    case: &str,
    temperature_c: f64,
) -> Value {
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
}

/// A run's results as an `ohl-field-results-set`: the case and each
/// field's document (of [`magnetic_value`], [`electric_value`]; magnetic
/// first).
pub fn set_json(fields: Vec<Value>, case: &str, temperature_c: f64) -> String {
    json!({
        "format": SET_FORMAT,
        "version": SET_VERSION,
        "case": {"name": case, "temperature_c": temperature_c},
        "fields": fields,
    })
    .to_string()
}
