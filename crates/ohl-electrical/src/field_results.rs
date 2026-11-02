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

/// A cut's document: the format's name (2026-11-02, a plugin's report
/// one cut at a time).
pub const CUT_FORMAT: &str = "ohl-field-cut";

/// A cut's document: the format's version.
pub const CUT_VERSION: u32 = 1;

/// The grids of a cut, moved to its own document by [`split_set`].
const GRID_KEYS: [&str; 5] = ["offsets", "elevations", "ground", "values", "isolines"];

/// A run's set (taken, not copied: it may be large) split for a
/// plugin's report (owner 2026-11-02: one cut at
/// a time, so that a long line's results need not be held at once): the
/// set with each cut's grids left out (`span`: its first and last offset
/// in their place), and per cut, in the order of the first field's cuts,
/// its document (format `ohl-field-cut` 1):
///
/// ```json
/// {"format": "ohl-field-cut", "version": 1, "case": {…},
///  "label", "alignment", "station", "angle_point", "from", "to", "at",
///  "plane": {"origin", "right"},
///  "fields": [{"quantity", "unit", "limit", "offsets", "elevations",
///              "ground", "values", "isolines"}, …],
///  "points": [], "sections": []}
/// ```
///
/// A field's cut is matched by its label and place (each field computes
/// the same cuts). `points` and `sections` are the host's to fill (the
/// plan's point classes and its TIN or DXF along the cut), as is `line`
/// (the line's name, for the plan's title).
pub fn split_set(set: Value) -> (Value, Vec<Value>) {
    let mut summary = set;
    let case = summary["case"].clone();
    let mut cuts: Vec<Value> = Vec::new();
    let fields = summary["fields"]
        .as_array_mut()
        .map(std::mem::take)
        .unwrap_or_default();
    let mut kept = Vec::new();
    for mut field in fields {
        let (quantity, unit, limit) = (
            field["quantity"].clone(),
            field["unit"].clone(),
            field["limit"].clone(),
        );
        if let Some(list) = field["cuts"].as_array_mut() {
            for c in list.iter_mut() {
                let mut grid = json!({"quantity": quantity, "unit": unit, "limit": limit});
                let offsets = c["offsets"].as_array().cloned().unwrap_or_default();
                for k in GRID_KEYS {
                    if let Some(v) = c.as_object_mut().and_then(|o| o.remove(k)) {
                        grid[k] = v;
                    }
                }
                c["span"] = json!([offsets.first(), offsets.last()]);
                let same = |d: &Value| {
                    d["label"] == c["label"]
                        && d["alignment"] == c["alignment"]
                        && d["station"] == c["station"]
                };
                match cuts.iter_mut().find(|d| same(d)) {
                    Some(d) => {
                        if let Some(f) = d["fields"].as_array_mut() {
                            f.push(grid);
                        }
                    }
                    None => {
                        let mut d = json!({
                            "format": CUT_FORMAT,
                            "version": CUT_VERSION,
                            "case": case,
                            "fields": [grid],
                            "points": [],
                            "sections": [],
                        });
                        for k in [
                            "label",
                            "alignment",
                            "station",
                            "angle_point",
                            "from",
                            "to",
                            "at",
                            "plane",
                        ] {
                            d[k] = c[k].clone();
                        }
                        cuts.push(d);
                    }
                }
            }
        }
        kept.push(field);
    }
    summary["fields"] = Value::Array(kept);
    (summary, cuts)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Two fields of two cuts: one document per cut holding both fields'
    /// grids; the summary without the grids, each cut's span in their
    /// place.
    #[test]
    fn a_set_splits_per_cut() {
        let cut = |label: &str, v: f64| {
            json!({"label": label, "alignment": "A", "station": 10.0, "angle_point": false,
                   "from": "1", "to": "2", "at": 5.0, "quantity": "B", "unit": "µT",
                   "plane": {"origin": [0.0, 0.0], "right": [1.0, 0.0]},
                   "offsets": [-2.0, 0.0, 2.0], "elevations": [1.0], "ground": [0.0, 0.0, 0.0],
                   "values": [v, v, v], "isolines": []})
        };
        let field = |q: &str| {
            json!({"format": FORMAT, "version": VERSION, "quantity": q, "unit": "u",
                   "limit": null, "cuts": [cut("c1", 1.0), cut("c2", 2.0)], "points": [],
                   "notes": []})
        };
        let set: Value =
            serde_json::from_str(&set_json(vec![field("B"), field("E")], "hot", 40.0)).unwrap();
        let (summary, cuts) = split_set(set);
        assert_eq!(cuts.len(), 2);
        assert_eq!(cuts[0]["format"], CUT_FORMAT);
        assert_eq!(cuts[1]["label"], "c2");
        assert_eq!(cuts[1]["fields"].as_array().unwrap().len(), 2);
        assert_eq!(cuts[1]["fields"][1]["quantity"], "E");
        assert_eq!(cuts[1]["fields"][0]["values"][0], 2.0);
        assert_eq!(cuts[0]["case"]["name"], "hot");
        let c = &summary["fields"][1]["cuts"][0];
        assert!(c.get("values").is_none() && c.get("offsets").is_none());
        assert_eq!(c["span"], json!([-2.0, 2.0]));
        assert_eq!(summary["format"], SET_FORMAT);
    }
}
