// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Built-in formats.
//!
//! | id               | direction | contents                                   |
//! |------------------|-----------|--------------------------------------------|
//! | `structures-csv` | both      | structure positions, orientations, types  |
//! | `wires-csv`      | export    | wire points of every case                  |
//! | `kml`            | export    | structures and wires in WGS84 (Google Earth) |
//! | `kmz`            | export    | the same, zipped                           |
//! | `spans-csv`      | export    | spans of every case: tensions, sag ([`crate::tables`]) |
//! | `ols-xml`        | both      | the whole line as OLS XML ([`crate::olsxml`]; import also reads LandXML) |
//! | `landxml`        | export    | routes as alignments, structures as CgPoints ([`crate::landxml`]) |
//! | `geojson`        | export    | structures, routes, wires per case ([`crate::gis`]) |
//! | `structures-geojson` | import | structures from points along a route   |
//! | `shapefile`      | export    | the same layers as zipped shapefiles ([`crate::shapefile`]) |
//! | `structures-shp` | import    | structures from shapefile points       |
//! | `gpkg`           | export    | the same layers as a GeoPackage ([`crate::geopackage`]) |
//! | `ifc`            | export    | structures and wires as IFC4 ([`crate::ifc`]) |
//! | `xlsx`           | export    | report tables as a workbook ([`crate::xlsx`]) |

use std::fmt::Write as _;
use std::io::Write as _;

use ols_core::text::{csv_field, parse_rgb};
use ols_crs::{Crs, Heights, Transformation};
use ols_line::{Line, Structure};

use crate::neutral::{Neutral, NeutralPoint};

use crate::{ExportModel, Exporter, Importer, Options, Registry};

/// Adds the built-in formats to a registry.
pub fn register(r: &mut Registry) {
    r.add_exporter(Box::new(StructuresCsv));
    r.add_importer(Box::new(StructuresCsv));
    r.add_exporter(Box::new(WiresCsv));
    r.add_exporter(Box::new(Kml { zipped: false }));
    r.add_exporter(Box::new(Kml { zipped: true }));
    r.add_exporter(Box::new(crate::tables::SpansCsv));
    r.add_exporter(Box::new(crate::olsxml::OlsXml));
    r.add_importer(Box::new(crate::olsxml::OlsXml));
    r.add_exporter(Box::new(crate::landxml::LandXml));
    r.add_exporter(Box::new(crate::gis::GeoJson));
    r.add_importer(Box::new(crate::gis::GeoJson));
    r.add_exporter(Box::new(crate::shapefile::Shapefile));
    r.add_importer(Box::new(crate::shapefile::Shapefile));
    r.add_exporter(Box::new(crate::geopackage::GeoPackage));
    r.add_exporter(Box::new(crate::ifc::Ifc));
    r.add_exporter(Box::new(crate::xlsx::Xlsx));
}

const STRUCTURES_HEADER: &str = "structure,x,y,z,orientation_deg,type,extension";

/// Structure positions as CSV: `structure,x,y,z,orientation_deg,type,extension`.
///
/// Import into an existing line (`--into`) moves and turns the structures
/// it lists, with their attachments, legs and stick drawings, and adds the
/// new ones; without a line, it makes a line of structures along a route
/// in file order. Only `x` and `y` are needed; other column names are
/// given with options `<column>_column` (e.g. `x_column=Easting`), the
/// separator with `delimiter` (`,`, `;` or `tab`); the options of
/// [`crate::neutral`] apply (default type, heights, coordinate systems).
pub struct StructuresCsv;

/// Splits one CSV record (fields may be quoted).
pub(crate) fn csv_fields(line: &str) -> Vec<String> {
    csv_split(line, ',')
}

/// Splits one record at `sep` (fields may be quoted).
fn csv_split(line: &str, sep: char) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                chars.next();
                fields.last_mut().expect("one field").push('"');
            }
            ('"', _) => quoted = !quoted,
            (c, false) if c == sep => fields.push(String::new()),
            (c, _) => fields.last_mut().expect("one field").push(c),
        }
    }
    fields.into_iter().map(|f| f.trim().to_owned()).collect()
}

impl Exporter for StructuresCsv {
    fn id(&self) -> &str {
        "structures-csv"
    }
    fn description(&self) -> &str {
        "Structure positions, orientations and types (CSV)"
    }
    fn extension(&self) -> &str {
        "csv"
    }
    fn export(&mut self, model: &ExportModel, _: &Options) -> Result<Vec<u8>, String> {
        let mut out = format!("{STRUCTURES_HEADER}\n");
        for s in &model.line.structures {
            let _ = writeln!(
                out,
                "{},{:.4},{:.4},{:.4},{},{},{}",
                csv_field(&s.id),
                s.position[0],
                s.position[1],
                s.position[2],
                s.orientation_deg,
                csv_field(s.structure_type.as_deref().unwrap_or("")),
                csv_field(s.extension.as_deref().unwrap_or("")),
            );
        }
        Ok(out.into_bytes())
    }
}

/// Moves a structure to `position` and turns it to `orientation_deg`,
/// with everything attached to it.
pub(crate) fn place(s: &mut Structure, position: [f64; 3], orientation_deg: f64) {
    let turn = (orientation_deg - s.orientation_deg).to_radians();
    let (sin, cos) = turn.sin_cos();
    let from = s.position;
    let map = |p: &mut [f64; 3]| {
        let (dx, dy) = (p[0] - from[0], p[1] - from[1]);
        *p = [
            position[0] + cos * dx - sin * dy,
            position[1] + sin * dx + cos * dy,
            position[2] + p[2] - from[2],
        ];
    };
    for a in &mut s.attachments {
        map(&mut a.insulator_point);
        map(&mut a.wire_point);
        a.legs.iter_mut().flatten().for_each(map);
    }
    s.members.iter_mut().flatten().for_each(map);
    s.position = position;
    s.orientation_deg = orientation_deg;
}

impl Importer for StructuresCsv {
    fn id(&self) -> &str {
        "structures-csv"
    }
    fn description(&self) -> &str {
        "Structure positions, orientations and types (CSV)"
    }
    fn extension(&self) -> &str {
        "csv"
    }
    fn import(
        &mut self,
        data: &[u8],
        base: Option<&Line>,
        options: &Options,
    ) -> Result<Line, String> {
        let text = std::str::from_utf8(data).map_err(|e| format!("not UTF-8 text: {e}"))?;
        let sep = match options.get("delimiter").map(String::as_str) {
            None | Some(",") => ',',
            Some(";") => ';',
            Some("tab") | Some("\t") => '\t',
            Some(o) => return Err(format!("option delimiter: \"{o}\" (, ; or tab)")),
        };
        let mut rows = text
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty());
        let (_, header) = rows.next().ok_or("empty file")?;
        let header = csv_split(header, sep);
        // A column: named by option `<name>_column`, else by its usual name.
        let column = |name: &str| -> Result<Option<usize>, String> {
            match options.get(&format!("{name}_column")) {
                Some(c) => header
                    .iter()
                    .position(|h| h.eq_ignore_ascii_case(c))
                    .map(Some)
                    .ok_or_else(|| format!("no column \"{c}\" (option {name}_column)")),
                None => Ok(header.iter().position(|h| h.eq_ignore_ascii_case(name))),
            }
        };
        let need = |name: &str| {
            column(name)?.ok_or_else(|| {
                format!("no column \"{name}\" (expected {STRUCTURES_HEADER}, or name it with option {name}_column)")
            })
        };
        let (x, y) = (need("x")?, need("y")?);
        let (id, z) = (column("structure")?, column("z")?);
        let (orientation, kind, extension) = (
            column("orientation_deg")?,
            column("type")?,
            column("extension")?,
        );
        let mut points = Vec::new();
        for (n, row) in rows {
            let f = csv_split(row, sep);
            let at = |k: usize| f.get(k).map_or("", String::as_str);
            let number = |k: usize, what: &str| -> Result<f64, String> {
                at(k)
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| format!("line {}: {what} \"{}\" is not a number", n + 1, at(k)))
            };
            let optional =
                |k: Option<usize>| k.map(at).filter(|t| !t.is_empty()).map(str::to_owned);
            let given = |k: Option<usize>, what: &str| match k.filter(|&k| !at(k).is_empty()) {
                Some(k) => number(k, what).map(Some),
                None => Ok(None),
            };
            if id.is_some() && optional(id).is_none() {
                return Err(format!("line {}: no structure id", n + 1));
            }
            points.push(NeutralPoint {
                id: optional(id),
                position: [
                    number(x, "x")?,
                    number(y, "y")?,
                    given(z, "z")?.unwrap_or(f64::NAN),
                ],
                structure_type: optional(kind),
                extension: optional(extension),
                orientation_deg: given(orientation, "orientation_deg")?,
            });
        }
        crate::neutral::to_line(
            Neutral {
                points,
                ..Default::default()
            },
            base,
            options,
        )
    }
}

/// Wire points of every case as CSV:
/// `case,section,span,circuit,phase,index,x,y,z` (jumpers: section
/// `jumper:<id>`).
pub struct WiresCsv;

impl Exporter for WiresCsv {
    fn id(&self) -> &str {
        "wires-csv"
    }
    fn description(&self) -> &str {
        "Wire points of every weather case (CSV)"
    }
    fn extension(&self) -> &str {
        "csv"
    }
    fn export(&mut self, model: &ExportModel, _: &Options) -> Result<Vec<u8>, String> {
        let mut out = String::from("case,section,span,circuit,phase,index,x,y,z\n");
        for case in &model.cases {
            for w in &case.wires {
                for (k, p) in w.points.iter().enumerate() {
                    let _ = writeln!(
                        out,
                        "{},{},{},{},{},{k},{:.4},{:.4},{:.4}",
                        csv_field(&case.name),
                        csv_field(&w.section),
                        w.span,
                        csv_field(&w.circuit),
                        csv_field(&w.phase),
                        p[0],
                        p[1],
                        p[2]
                    );
                }
            }
        }
        Ok(out.into_bytes())
    }
}

/// Structures and wires for Google Earth and GIS: KML in WGS84 (from the
/// line's `crs`, or option `crs`), heights kept (`altitudeMode` absolute:
/// above sea level, like the line's heights). One folder per weather case;
/// wire colours from the sections. Option `cases`: `first` (default) or
/// `all`.
pub struct Kml {
    zipped: bool,
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `#rrggbb` → KML `ffbbggrr`.
fn kml_color(color: &str) -> String {
    parse_rgb(color).map_or("ffc0b8b0".into(), |[r, g, b]| {
        format!("ff{b:02x}{g:02x}{r:02x}")
    })
}

/// The KML document of a model.
pub fn kml_document(model: &ExportModel, options: &Options) -> Result<String, String> {
    let spec = options
        .get("crs")
        .map(String::as_str)
        .unwrap_or(model.line.crs.as_str());
    if spec.trim().is_empty() {
        return Err("KML needs the line's coordinate system: set \"crs\" in the line file or give --option crs=EPSG:<code>".into());
    }
    let from = Crs::parse(spec)
        .map_err(|e| format!("{e} (KML needs a real coordinate system: option crs=EPSG:<code>)"))?;
    let to_wgs84 = Transformation::new(&from, &Crs::parse("EPSG:4326")?, Heights::Keep);
    let all_cases = match options.get("cases").map(String::as_str) {
        None | Some("first") => false,
        Some("all") => true,
        Some(other) => return Err(format!("option cases: \"{other}\" (use first or all)")),
    };
    let coords = |points: &[[f64; 3]]| -> Result<String, String> {
        let mut p = points.to_vec();
        to_wgs84.points(&mut p)?;
        Ok(p.iter()
            .map(|q| format!("{:.8},{:.8},{:.3}", q[0], q[1], q[2]))
            .collect::<Vec<_>>()
            .join(" "))
    };
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<kml xmlns=\"http://www.opengis.net/kml/2.2\">\n<Document>\n",
    );
    let _ = writeln!(out, "<name>{}</name>", xml(&model.line.name));
    let mut colors: Vec<&str> = model
        .cases
        .iter()
        .flat_map(|c| c.wires.iter().map(|w| w.color.as_str()))
        .collect();
    colors.sort_unstable();
    colors.dedup();
    for c in &colors {
        let _ = writeln!(
            out,
            "<Style id=\"w{}\"><LineStyle><color>{}</color><width>2</width></LineStyle></Style>",
            c.trim_start_matches('#'),
            kml_color(c)
        );
    }
    out.push_str("<Folder>\n<name>Structures</name>\n");
    for s in &model.line.structures {
        let _ = writeln!(
            out,
            "<Placemark><name>{}</name><description>{}</description><Point><altitudeMode>absolute</altitudeMode><coordinates>{}</coordinates></Point></Placemark>",
            xml(&s.id),
            xml(s.structure_type.as_deref().unwrap_or("")),
            coords(&[s.position])?
        );
    }
    out.push_str("</Folder>\n");
    let cases = if all_cases {
        &model.cases[..]
    } else {
        &model.cases[..model.cases.len().min(1)]
    };
    for case in cases {
        let _ = writeln!(
            out,
            "<Folder>\n<name>{}</name>\n<description>{}</description>",
            xml(&case.name),
            xml(&case.summary)
        );
        for w in &case.wires {
            let name = if w.section.starts_with("jumper:") {
                w.section.clone()
            } else {
                format!("{} span {}", w.section, w.span + 1)
            };
            let _ = writeln!(
                out,
                "<Placemark><name>{}</name><description>{} {}</description><styleUrl>#w{}</styleUrl><LineString><altitudeMode>absolute</altitudeMode><coordinates>{}</coordinates></LineString></Placemark>",
                xml(&name),
                xml(&w.circuit),
                xml(&w.phase),
                w.color.trim_start_matches('#'),
                coords(&w.points)?
            );
        }
        out.push_str("</Folder>\n");
    }
    out.push_str("</Document>\n</kml>\n");
    Ok(out)
}

impl Exporter for Kml {
    fn id(&self) -> &str {
        if self.zipped { "kmz" } else { "kml" }
    }
    fn description(&self) -> &str {
        if self.zipped {
            "Structures and wires in WGS84 for Google Earth (zipped KML)"
        } else {
            "Structures and wires in WGS84 for Google Earth and GIS (KML)"
        }
    }
    fn extension(&self) -> &str {
        self.id()
    }
    fn export(&mut self, model: &ExportModel, options: &Options) -> Result<Vec<u8>, String> {
        let doc = kml_document(model, options)?;
        if !self.zipped {
            return Ok(doc.into_bytes());
        }
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("doc.kml", opts).map_err(|e| e.to_string())?;
        zip.write_all(doc.as_bytes()).map_err(|e| e.to_string())?;
        Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
    }
}
