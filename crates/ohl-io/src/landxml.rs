// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! LandXML 1.2 (the public LandXML schema) for the line: routes as
//! `Alignments` (`CoordGeom` lines between the alignment points, a
//! `Profile` of their heights) and structures as `CgPoints` (northing,
//! easting, elevation). Terrain surfaces (TINs) are read and written by
//! [`ohl_terrain::formats`].
//!
//! Import (through the `xml` importer, which recognises the root
//! `LandXML`): `CgPoint`s become structures ordered along the first
//! alignment (option `alignment`: by name), as other neutral files
//! ([`crate::neutral`]); a file with an alignment but no points updates
//! the route's alignment of the line it is imported into. Curves and
//! spirals are read by their start and end points.

use std::fmt::Write as _;

use ohl_line::{Line, Point};

use crate::neutral::{Neutral, NeutralPoint};
use crate::xml::{Element, attribute as attr};
use crate::{ExportModel, Exporter, Options};

/// The UTC date and time now, `YYYY-MM-DD` and `HH:MM:SS`.
pub fn utc_now() -> (String, String) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Civil date from days since 1970-01-01 (proleptic Gregorian).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (
        format!("{y:04}-{m:02}-{d:02}"),
        format!("{:02}:{:02}:{:02}", rem / 3600, rem / 60 % 60, rem % 60),
    )
}

fn plan(a: Point, b: Point) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

/// The LandXML document of a line: structures and routes.
pub fn document(line: &Line) -> String {
    let (date, time) = utc_now();
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<LandXML xmlns=\"http://www.landxml.org/schema/LandXML-1.2\" \
         version=\"1.2\" date=\"{date}\" time=\"{time}\">\n\
         <Units><Metric linearUnit=\"meter\" areaUnit=\"squareMeter\" volumeUnit=\"cubicMeter\" \
         angularUnit=\"decimal degrees\" directionUnit=\"decimal degrees\"/></Units>\n"
    );
    if let Some(code) = crate::gis::epsg_code(&line.crs) {
        let _ = writeln!(
            out,
            "<CoordinateSystem name=\"EPSG:{code}\" epsgCode=\"{code}\"/>"
        );
    }
    out.push_str("<Project");
    attr(&mut out, "name", &line.name);
    out.push_str("/>\n");
    let _ = writeln!(
        out,
        "<Application name=\"OpenOHL\" version=\"{}\"/>",
        env!("CARGO_PKG_VERSION")
    );
    out.push_str("<CgPoints name=\"structures\">\n");
    for s in &line.structures {
        out.push_str("<CgPoint");
        attr(&mut out, "name", &s.id);
        if let Some(t) = &s.structure_type {
            attr(&mut out, "code", t);
        }
        attr(&mut out, "desc", "structure");
        let p = s.position;
        let _ = writeln!(out, ">{:.4} {:.4} {:.4}</CgPoint>", p[1], p[0], p[2]);
    }
    out.push_str("</CgPoints>\n<Alignments name=\"routes\">\n");
    for r in line.alignment_views() {
        let c = r.centreline(line);
        if c.len() < 2 {
            continue;
        }
        let length: f64 = c.windows(2).map(|w| plan(w[0], w[1])).sum();
        out.push_str("<Alignment");
        attr(&mut out, "name", &r.id);
        if !r.name.is_empty() {
            attr(&mut out, "desc", &r.name);
        }
        let _ = writeln!(out, " length=\"{length:.4}\" staStart=\"0\">\n<CoordGeom>");
        let mut station = 0.0;
        for w in c.windows(2) {
            let l = plan(w[0], w[1]);
            if l == 0.0 {
                continue;
            }
            let _ = writeln!(
                out,
                "<Line staStart=\"{station:.4}\" length=\"{l:.4}\"><Start>{:.4} {:.4}</Start><End>{:.4} {:.4}</End></Line>",
                w[0][1], w[0][0], w[1][1], w[1][0]
            );
            station += l;
        }
        out.push_str("</CoordGeom>\n<Profile");
        attr(&mut out, "name", &r.id);
        out.push_str("><ProfAlign");
        attr(&mut out, "name", &r.id);
        out.push('>');
        let mut station = 0.0;
        for (k, p) in c.iter().enumerate() {
            if k > 0 {
                station += plan(c[k - 1], *p);
            }
            let _ = write!(out, "<PVI>{station:.4} {:.4}</PVI>", p[2]);
        }
        out.push_str("</ProfAlign></Profile>\n</Alignment>\n");
    }
    out.push_str("</Alignments>\n</LandXML>\n");
    out
}

/// LandXML: structures as CgPoints, routes as alignments.
pub struct LandXml;

impl Exporter for LandXml {
    fn id(&self) -> &str {
        "landxml"
    }
    fn description(&self) -> &str {
        "Routes (alignments with profile) and structures (CgPoints) as LandXML 1.2"
    }
    fn extension(&self) -> &str {
        "xml"
    }
    fn export(&mut self, model: &ExportModel, _: &Options) -> Result<Vec<u8>, String> {
        Ok(document(&model.line).into_bytes())
    }
}

/// Metres per unit of the file (`Units`).
fn scale(root: &Element) -> Result<f64, String> {
    let Some(units) = root.child("Units") else {
        return Ok(1.0);
    };
    let (unit, metric) = match (units.child("Metric"), units.child("Imperial")) {
        (Some(m), _) => (m.attr("linearUnit").unwrap_or("meter"), true),
        (None, Some(i)) => (i.attr("linearUnit").unwrap_or("foot"), false),
        _ => return Ok(1.0),
    };
    Ok(match unit {
        "meter" => 1.0,
        "millimeter" => 0.001,
        "centimeter" => 0.01,
        "kilometer" => 1000.0,
        "foot" | "internationalFoot" => 0.3048,
        "USSurveyFoot" => 1200.0 / 3937.0,
        other => {
            return Err(format!(
                "LandXML: linear unit \"{other}\" not supported ({})",
                if metric { "Metric" } else { "Imperial" }
            ));
        }
    })
}

/// "northing easting [elevation]" → [x, y, z] (z NaN when absent).
fn ne(e: &Element, scale: f64) -> Result<Point, String> {
    let v: Vec<f64> = e
        .text
        .split_whitespace()
        .map(|t| t.parse::<f64>())
        .collect::<Result<_, _>>()
        .map_err(|_| {
            format!(
                "LandXML <{}>: \"{}\" is not coordinates",
                e.name,
                e.text.trim()
            )
        })?;
    if v.len() < 2 {
        return Err(format!("LandXML <{}>: northing and easting needed", e.name));
    }
    Ok([
        v[1] * scale,
        v[0] * scale,
        v.get(2).map_or(f64::NAN, |z| z * scale),
    ])
}

/// The polyline of an alignment (start and end points of its elements).
fn polyline(a: &Element, scale: f64) -> Result<Vec<Point>, String> {
    let mut out: Vec<Point> = Vec::new();
    let Some(geom) = a.child("CoordGeom") else {
        return Ok(out);
    };
    for el in &geom.children {
        for end in ["Start", "End"] {
            if let Some(p) = el.child(end) {
                let p = ne(p, scale)?;
                if out.last().is_none_or(|q| plan(*q, p) > 1e-6) {
                    out.push(p);
                }
            }
        }
    }
    Ok(out)
}

/// A line from (or into, with `base`) a parsed LandXML document.
pub fn import(root: &Element, base: Option<&Line>, options: &Options) -> Result<Line, String> {
    let scale = scale(root)?;
    let mut alignments = Vec::new();
    root.descendants("Alignment", &mut alignments);
    let alignment = match options.get("alignment") {
        Some(n) => Some(
            alignments
                .iter()
                .find(|a| a.attr("name") == Some(n.as_str()))
                .ok_or_else(|| format!("LandXML: no alignment named \"{n}\""))?,
        ),
        None => alignments.first(),
    };
    let route = match alignment {
        Some(a) => polyline(a, scale)?,
        None => Vec::new(),
    };
    let mut cg = Vec::new();
    root.descendants("CgPoint", &mut cg);
    let crs = root
        .child("CoordinateSystem")
        .and_then(|c| c.attr("epsgCode"))
        .map(|c| format!("EPSG:{}", c.trim()));
    if cg.is_empty() {
        // An alignment only: the route of the line imported into.
        let Some(line) = base else {
            return Err(
                "LandXML: no CgPoints (structures); an alignment alone updates a line \
                        (import it into one)"
                    .into(),
            );
        };
        if route.len() < 2 {
            return Err("LandXML: no CgPoints and no alignment".into());
        }
        let mut line = line.clone();
        let points: Vec<(ohl_line::Point, Option<String>)> = route
            .into_iter()
            .map(|mut p| {
                if !p[2].is_finite() {
                    p[2] = 0.0;
                }
                (p, None)
            })
            .collect();
        // The alignment named (default: the first), or a new route and
        // alignment "main" for a line without any.
        if line.alignments.is_empty() {
            let id = options
                .get("route")
                .cloned()
                .unwrap_or_else(|| "main".into());
            if !line.routes.iter().any(|r| r.id == id) {
                line.routes
                    .push(ohl_line::Route::new(&id, &line.name.clone()));
            }
            let mut a = ohl_line::Alignment::new(&id, &id, Vec::new());
            a.name = line.name.clone();
            line.alignments.push(a);
            line.set_alignment_points(&id, points);
            return Ok(line);
        }
        let id = options
            .get("route")
            .cloned()
            .unwrap_or_else(|| line.alignments[0].id.clone());
        if !line.set_alignment_points(&id, points) {
            return Err(format!("no alignment {id} in the line"));
        }
        return Ok(line);
    }
    let points = cg
        .iter()
        .map(|p| {
            Ok(NeutralPoint {
                id: p.attr("name").or(p.attr("oID")).map(str::to_owned),
                position: ne(p, scale)?,
                ..Default::default()
            })
        })
        .collect::<Result<_, String>>()?;
    crate::neutral::to_line(
        Neutral {
            points,
            route,
            crs,
            name: root
                .child("Project")
                .and_then(|p| p.attr("name"))
                .map(str::to_owned),
        },
        base,
        options,
    )
}
