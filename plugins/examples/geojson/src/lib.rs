// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Example OLS CAD file-format plugin (`io-plugin` world).
//!
//! - Export `geojson`: a FeatureCollection with a Point per structure and a
//!   LineString per wire of every weather case, in the line's coordinate
//!   system (named in the legacy `crs` member; RFC 7946 readers assume
//!   WGS84 — option `crs-member=false` leaves it out).
//! - Import `geojson-structures`: Point features become structures
//!   (`properties.id`, optional `type` and `orientation_deg`); with a line
//!   to update (option `into`, given by the host), existing structures
//!   move and new ones are added.
//!
//! Build with:
//!
//! ```bash
//! cargo build --release --target wasm32-wasip2
//! ```

use serde_json::{Value, json};

wit_bindgen::generate!({
    world: "io-plugin",
    path: "../../../crates/ols-plugins/wit",
});

use ols::plugin::io_types::Direction;

struct GeoJson;

fn option<'a>(options: &'a [(String, String)], key: &str) -> Option<&'a str> {
    options
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn export(model: &str, options: &[(String, String)]) -> Result<Vec<u8>, String> {
    let model: Value = serde_json::from_str(model).map_err(|e| format!("model: {e}"))?;
    if model["format"] != "ols-export-model" || model["version"] != 1 {
        return Err("expects ols-export-model version 1".into());
    }
    let mut features = Vec::new();
    for s in model["line"]["structures"].as_array().into_iter().flatten() {
        features.push(json!({
            "type": "Feature",
            "geometry": {"type": "Point", "coordinates": s["position"]},
            "properties": {
                "kind": "structure",
                "id": s["id"],
                "type": s.get("type").cloned().unwrap_or(Value::Null),
                "orientation_deg": s["orientation_deg"],
            },
        }));
    }
    for case in model["cases"].as_array().into_iter().flatten() {
        for w in case["wires"].as_array().into_iter().flatten() {
            features.push(json!({
                "type": "Feature",
                "geometry": {"type": "LineString", "coordinates": w["points"]},
                "properties": {
                    "kind": "wire",
                    "case": case["name"],
                    "section": w["section"],
                    "span": w["span"],
                    "circuit": w["circuit"],
                    "phase": w["phase"],
                    "stroke": w["color"],
                },
            }));
        }
    }
    let mut collection = json!({
        "type": "FeatureCollection",
        "name": model["line"]["name"],
        "features": features,
    });
    let crs = model["crs"].as_str().unwrap_or("");
    if !crs.is_empty() && option(options, "crs-member") != Some("false") {
        collection["crs"] = json!({"type": "name", "properties": {"name": crs}});
    }
    serde_json::to_vec_pretty(&collection).map_err(|e| e.to_string())
}

fn import(data: &[u8], options: &[(String, String)]) -> Result<String, String> {
    let doc: Value = serde_json::from_slice(data).map_err(|e| format!("not JSON: {e}"))?;
    let mut line = match option(options, "into") {
        Some(text) => serde_json::from_str(text).map_err(|e| format!("into: {e}"))?,
        None => json!({
            "format": "ols-line",
            "version": 1,
            "name": doc["name"].as_str().unwrap_or("imported from GeoJSON"),
            "crs": doc["crs"]["properties"]["name"].as_str().unwrap_or(""),
            "structures": [],
            "sections": [],
        }),
    };
    let structures = line["structures"]
        .as_array_mut()
        .ok_or("the line has no structures list")?;
    let mut count = 0;
    for (k, f) in doc["features"].as_array().into_iter().flatten().enumerate() {
        if f["geometry"]["type"] != "Point" {
            continue;
        }
        let id = f["properties"]["id"]
            .as_str()
            .ok_or_else(|| format!("feature {k}: no properties.id"))?;
        let c = f["geometry"]["coordinates"]
            .as_array()
            .ok_or_else(|| format!("feature {k}: no coordinates"))?;
        let n = |i: usize| c.get(i).and_then(Value::as_f64);
        let (Some(x), Some(y)) = (n(0), n(1)) else {
            return Err(format!("feature {k}: coordinates are not numbers"));
        };
        let position = json!([x, y, n(2).unwrap_or(0.0)]);
        match structures.iter_mut().find(|s| s["id"] == id) {
            // Only the position moves here; attachments of an existing
            // structure are left to the built-in structures-csv importer.
            Some(s) => s["position"] = position,
            None => {
                let mut s = json!({"id": id, "position": position, "attachments": []});
                if let Some(t) = f["properties"]["type"].as_str() {
                    s["type"] = json!(t);
                }
                if let Some(o) = f["properties"]["orientation_deg"].as_f64() {
                    s["orientation_deg"] = json!(o);
                }
                structures.push(s);
            }
        }
        count += 1;
    }
    if count == 0 {
        return Err("no Point features".into());
    }
    serde_json::to_string(&line).map_err(|e| e.to_string())
}

impl Guest for GeoJson {
    fn describe() -> PluginInfo {
        PluginInfo {
            name: "geojson".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: "GeoJSON export (structures, wires) and import (structures)".into(),
        }
    }

    fn formats() -> Vec<Format> {
        vec![
            Format {
                id: "geojson".into(),
                description: "Structures and wires as GeoJSON features (line CRS)".into(),
                extension: "geojson".into(),
                direction: Direction::Exporter,
            },
            Format {
                id: "geojson-structures".into(),
                description: "Structures from GeoJSON Point features".into(),
                extension: "geojson".into(),
                direction: Direction::Importer,
            },
        ]
    }

    fn export_model(
        format: String,
        model: String,
        options: Vec<(String, String)>,
    ) -> Result<Vec<u8>, String> {
        match format.as_str() {
            "geojson" => export(&model, &options),
            other => Err(format!("unknown format {other}")),
        }
    }

    fn import_file(
        format: String,
        data: Vec<u8>,
        options: Vec<(String, String)>,
    ) -> Result<String, String> {
        match format.as_str() {
            "geojson-structures" => import(&data, &options),
            other => Err(format!("unknown format {other}")),
        }
    }
}

export!(GeoJson);
