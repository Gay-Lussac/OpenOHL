// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! GIS layers of a solved line, shared by GeoJSON, Shapefile and
//! GeoPackage: `structures` (points), `routes` (centrelines) and `wires`
//! (one line per span and weather case, with its tensions and sag).
//!
//! Options common to these formats:
//!
//! - `coordinates` — `line` (the line's coordinate system) or `wgs84`
//!   (longitude, latitude; heights kept). Default: `wgs84` for GeoJSON
//!   (RFC 7946), `line` for Shapefile and GeoPackage.
//! - `crs` — the line's coordinate system, when the line file has none.
//! - `cases` — `all` (default) or `first` weather case.
//!
//! Also the built-in GeoJSON format ([`GeoJson`]): export of the layers,
//! import of structures from Point features ordered along a LineString
//! ([`crate::neutral`]).

use ohl_crs::{Crs, Heights, Transformation};
use ohl_line::Point;
use serde_json::{Value, json};

use crate::neutral::{Neutral, NeutralPoint};
use crate::tables::Cell;
use crate::{ExportCase, ExportModel, Exporter, Importer, Options};

/// The coordinates written: the line's, or WGS84.
pub struct Output {
    transform: Option<Transformation>,
    /// The coordinate system written (`EPSG:4326`, the line's, or empty
    /// when unknown).
    pub crs: String,
    /// Its EPSG code, if it has one.
    pub epsg: Option<u32>,
}

impl Output {
    /// Points in the output coordinates.
    pub fn points(&self, points: &[Point]) -> Result<Vec<Point>, String> {
        let mut p = points.to_vec();
        if let Some(t) = &self.transform {
            t.points(&mut p)?;
        }
        Ok(p)
    }

    /// Whether the output is longitude, latitude.
    pub fn geographic(&self) -> bool {
        self.epsg == Some(4326)
    }
}

/// The EPSG code of `EPSG:<code>` text.
pub fn epsg_code(crs: &str) -> Option<u32> {
    let t = crs.trim().to_ascii_lowercase();
    t.strip_prefix("epsg:")
        .or_else(|| t.strip_prefix("urn:ogc:def:crs:epsg::"))
        .and_then(|c| c.trim().parse().ok())
}

/// The output coordinates chosen by the options (`coordinates`, `crs`).
pub fn output(model: &ExportModel, options: &Options, wgs84: bool) -> Result<Output, String> {
    let line_crs = options
        .get("crs")
        .map(String::as_str)
        .unwrap_or(model.line.crs.as_str())
        .trim()
        .to_owned();
    let wgs84 = match options.get("coordinates").map(String::as_str) {
        None => wgs84,
        Some("wgs84") => true,
        Some("line") => false,
        Some(o) => return Err(format!("option coordinates: \"{o}\" (line or wgs84)")),
    };
    if !wgs84 {
        return Ok(Output {
            transform: None,
            epsg: epsg_code(&line_crs),
            crs: line_crs,
        });
    }
    if line_crs.is_empty() {
        return Err(
            "WGS84 output needs the line's coordinate system: set \"crs\" in the line \
                    file, give --option crs=EPSG:<code>, or write coordinates=line"
                .into(),
        );
    }
    let from = Crs::parse(&line_crs).map_err(|e| {
        format!("{e} (WGS84 output needs a real coordinate system: option crs=EPSG:<code>, or coordinates=line)")
    })?;
    Ok(Output {
        transform: Some(Transformation::new(
            &from,
            &Crs::parse("EPSG:4326")?,
            Heights::Keep,
        )),
        crs: "EPSG:4326".into(),
        epsg: Some(4326),
    })
}

/// The weather cases chosen by option `cases` (`all` or `first`).
pub fn chosen_cases<'a>(
    model: &'a ExportModel,
    options: &Options,
    all: bool,
) -> Result<&'a [ExportCase], String> {
    let all = match options.get("cases").map(String::as_str) {
        None => all,
        Some("all") => true,
        Some("first") => false,
        Some(o) => return Err(format!("option cases: \"{o}\" (first or all)")),
    };
    Ok(if all {
        &model.cases
    } else {
        &model.cases[..model.cases.len().min(1)]
    })
}

/// A feature's geometry.
#[derive(Debug, Clone, PartialEq)]
pub enum Geometry {
    /// A point.
    Point(Point),
    /// A line string.
    Line(Vec<Point>),
}

/// The type of a layer's column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Text.
    Text,
    /// A number.
    Number,
}

/// A layer: its columns and features (geometry and one cell per column).
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// Name (`structures`, `routes`, `wires`).
    pub name: &'static str,
    /// Columns: name (at most 10 characters, for dBASE) and type.
    pub columns: Vec<(&'static str, Kind)>,
    /// Features.
    pub features: Vec<(Geometry, Vec<Cell>)>,
}

/// The layers of a model in the output coordinates.
pub fn layers(
    model: &ExportModel,
    options: &Options,
    out: &Output,
    all_cases: bool,
) -> Result<Vec<Layer>, String> {
    use Kind::{Number, Text};
    let line = &model.line;
    let mut structures = Layer {
        name: "structures",
        columns: vec![
            ("id", Text),
            ("type", Text),
            ("extension", Text),
            ("orient_deg", Number),
            ("routes", Text),
        ],
        features: Vec::new(),
    };
    let positions: Vec<Point> = line.structures.iter().map(|s| s.position).collect();
    for (s, p) in line.structures.iter().zip(out.points(&positions)?) {
        structures.features.push((
            Geometry::Point(p),
            vec![
                s.id.as_str().into(),
                s.structure_type.as_deref().into(),
                s.extension.as_deref().into(),
                s.orientation_deg.into(),
                line.routes_of(&s.id).join(" ").into(),
            ],
        ));
    }
    let mut routes = Layer {
        name: "routes",
        columns: vec![("id", Text), ("name", Text), ("structures", Text)],
        features: Vec::new(),
    };
    for r in line.alignment_views() {
        let c = r.centreline(line);
        if c.len() >= 2 {
            routes.features.push((
                Geometry::Line(out.points(&c)?),
                vec![
                    r.id.as_str().into(),
                    r.name.as_str().into(),
                    r.structures.join(" ").into(),
                ],
            ));
        }
    }
    let mut wires = Layer {
        name: "wires",
        columns: vec![
            ("case", Text),
            ("section", Text),
            ("span", Number),
            ("circuit", Text),
            ("phase", Text),
            ("system", Text),
            ("voltage_kv", Number),
            ("color", Text),
            ("h_tens_n", Number),
            ("max_tens_n", Number),
            ("sag_m", Number),
        ],
        features: Vec::new(),
    };
    for case in chosen_cases(model, options, all_cases)? {
        for w in &case.wires {
            if w.points.len() < 2 {
                continue;
            }
            wires.features.push((
                Geometry::Line(out.points(&w.points)?),
                vec![
                    case.name.as_str().into(),
                    w.section.as_str().into(),
                    (w.span + 1).into(),
                    w.circuit.as_str().into(),
                    w.phase.as_str().into(),
                    w.system.as_deref().into(),
                    w.voltage.into(),
                    w.color.as_str().into(),
                    w.horizontal_tension.into(),
                    w.max_tension.into(),
                    w.sag.into(),
                ],
            ));
        }
    }
    Ok(vec![structures, routes, wires])
}

/// Rounds coordinates: 1e-9° (0.1 mm) or 0.1 mm.
fn coords(p: Point, geographic: bool) -> Value {
    let r = |v: f64, f: f64| (v * f).round() / f;
    let f = if geographic { 1e9 } else { 1e4 };
    json!([r(p[0], f), r(p[1], f), r(p[2], 1e4)])
}

/// The GeoJSON document of the layers.
pub fn geojson(model: &ExportModel, layers: &[Layer], out: &Output) -> Value {
    let geo = out.geographic();
    let mut features = Vec::new();
    for layer in layers {
        for (g, cells) in &layer.features {
            let mut props = serde_json::Map::new();
            props.insert("layer".into(), layer.name.into());
            for ((name, _), c) in layer.columns.iter().zip(cells) {
                let v = match c {
                    Cell::Text(t) => Value::String(t.clone()),
                    Cell::Number(x) => json!(x),
                    Cell::Empty => Value::Null,
                };
                props.insert((*name).into(), v);
            }
            let geometry = match g {
                Geometry::Point(p) => json!({"type": "Point", "coordinates": coords(*p, geo)}),
                Geometry::Line(ps) => json!({
                    "type": "LineString",
                    "coordinates": ps.iter().map(|p| coords(*p, geo)).collect::<Vec<_>>(),
                }),
            };
            features.push(json!({"type": "Feature", "geometry": geometry, "properties": props}));
        }
    }
    let mut doc = json!({
        "type": "FeatureCollection",
        "name": model.line.name,
        "features": features,
    });
    if !geo && !out.crs.is_empty() {
        doc["crs"] = json!({"type": "name", "properties": {"name": out.crs}});
    }
    doc
}

/// GeoJSON: export of the layers (WGS84 by default), import of
/// structures from Point features along a LineString.
pub struct GeoJson;

impl Exporter for GeoJson {
    fn id(&self) -> &str {
        "geojson"
    }
    fn description(&self) -> &str {
        "Structures, routes and wires (tensions, sag per case) as GeoJSON (WGS84)"
    }
    fn extension(&self) -> &str {
        "geojson"
    }
    fn export(&mut self, model: &ExportModel, options: &Options) -> Result<Vec<u8>, String> {
        let out = output(model, options, true)?;
        let layers = layers(model, options, &out, true)?;
        serde_json::to_vec(&geojson(model, &layers, &out)).map_err(|e| e.to_string())
    }
}

/// A property as text (strings as they are, numbers written out).
fn property(props: &Value, key: &str) -> Option<String> {
    match props.get(key)? {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_owned()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn position(c: &Value) -> Option<Point> {
    let a = c.as_array()?;
    let v = |k: usize| a.get(k).and_then(Value::as_f64);
    Some([v(0)?, v(1)?, v(2).unwrap_or(f64::NAN)])
}

/// The points and polyline of a GeoJSON document (properties `id`,
/// `name` or option `id`; `type`, `extension`, `orientation_deg`).
pub fn read_geojson(data: &[u8], options: &Options) -> Result<Neutral, String> {
    let doc: Value = serde_json::from_slice(data).map_err(|e| format!("not JSON: {e}"))?;
    let features: Vec<&Value> = match doc["type"].as_str() {
        Some("FeatureCollection") => doc["features"].as_array().into_iter().flatten().collect(),
        Some("Feature") => vec![&doc],
        _ => return Err("not a GeoJSON FeatureCollection or Feature".into()),
    };
    let id_keys: Vec<&str> = match options.get("id") {
        Some(k) => vec![k.as_str()],
        None => vec!["id", "name", "structure"],
    };
    let mut data = Neutral {
        crs: doc["crs"]["properties"]["name"]
            .as_str()
            .map(|n| match epsg_code(n) {
                Some(c) => format!("EPSG:{c}"),
                None => n.to_owned(),
            })
            .or_else(|| Some("EPSG:4326".into())),
        name: doc["name"].as_str().map(str::to_owned),
        ..Default::default()
    };
    for f in features {
        let props = &f["properties"];
        let g = &f["geometry"];
        match g["type"].as_str() {
            Some("Point") => {
                let p = position(&g["coordinates"]).ok_or("a Point without coordinates")?;
                data.points.push(NeutralPoint {
                    id: id_keys.iter().find_map(|k| property(props, k)),
                    position: p,
                    structure_type: property(props, "type"),
                    extension: property(props, "extension"),
                    orientation_deg: props
                        .get("orientation_deg")
                        .or_else(|| props.get("orient_deg"))
                        .and_then(Value::as_f64),
                });
            }
            Some("LineString") if data.route.is_empty() => {
                // The route (our own exports: the routes layer, not wires).
                if props["layer"].as_str().is_some_and(|l| l != "routes") {
                    continue;
                }
                data.route = g["coordinates"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(position)
                    .collect();
            }
            _ => {}
        }
    }
    Ok(data)
}

impl Importer for GeoJson {
    fn id(&self) -> &str {
        "structures-geojson"
    }
    fn description(&self) -> &str {
        "Structures from GeoJSON points, ordered along a LineString route"
    }
    fn extension(&self) -> &str {
        "geojson"
    }
    fn import(
        &mut self,
        data: &[u8],
        base: Option<&ohl_line::Line>,
        options: &Options,
    ) -> Result<ohl_line::Line, String> {
        crate::neutral::to_line(read_geojson(data, options)?, base, options)
    }
}
