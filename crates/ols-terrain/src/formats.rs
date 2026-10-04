// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! TIN file formats: LandXML surfaces (read and write) and Wavefront OBJ
//! meshes (read and write). Survey points (CSV / XYZ) are triangulated
//! ([`crate::read_points`], [`crate::Terrain::from_points`]).
//!
//! - **LandXML 1.x** `<Surface name="…"><Definition surfType="TIN">` with
//!   `<Pnts><P id="1">northing easting elevation</P>…</Pnts>` and
//!   `<Faces><F>1 2 3</F>…</Faces>` (faces marked `i="1"` are invisible:
//!   left out). A file may hold several surfaces. Note the coordinate order
//!   of `P`: **northing first**.
//! - **OBJ**: `v x y z` and `f a b c …` (1-based, `a/…` forms accepted,
//!   polygons split in fans, negative indices relative to the end).
//!
//! Readers are linear in the file size (a 300 MB LandXML: a few seconds).

use crate::{Terrain, TerrainError};

/// A surface read from a file: its name and mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    /// Name (LandXML) or the file's.
    pub name: String,
    /// Points (x east, y north, z up).
    pub points: Vec<[f64; 3]>,
    /// Triangles (indices into the points).
    pub triangles: Vec<[u32; 3]>,
}

impl Mesh {
    /// The surface (heights, profiles).
    pub fn terrain(self) -> Result<Terrain, TerrainError> {
        Terrain::from_mesh(self.points, self.triangles)
    }
}

fn bad(message: String) -> TerrainError {
    TerrainError::Degenerate(message)
}

/// The value of attribute `name` in a tag's text (`<P id="3">`).
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = tag;
    while let Some(k) = rest.find(name) {
        let before_ok = k == 0
            || rest[..k]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
        let after = &rest[k + name.len()..];
        let after_trim = after.trim_start();
        if before_ok && after_trim.starts_with('=') {
            let v = after_trim[1..].trim_start();
            let quote = v.chars().next()?;
            if quote == '"' || quote == '\'' {
                let end = v[1..].find(quote)?;
                return Some(&v[1..1 + end]);
            }
        }
        rest = &rest[k + name.len()..];
    }
    None
}

/// Metres per unit of a LandXML file's `<Units>` (`<Metric>` or
/// `<Imperial>` with its `linearUnit`; metres when not given).
fn landxml_scale(text: &str) -> Result<f64, TerrainError> {
    let Some(k) = text.find("<Units") else {
        return Ok(1.0);
    };
    let units = &text[k..];
    let units = &units[..units.find("</Units>").unwrap_or(units.len())];
    for system in ["<Metric", "<Imperial"] {
        let Some(k) = units.find(system) else {
            continue;
        };
        let tag = &units[k..];
        let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
        let unit = attribute(tag, "linearUnit").unwrap_or(if system == "<Metric" {
            "meter"
        } else {
            "foot"
        });
        return match unit {
            "meter" => Ok(1.0),
            "millimeter" => Ok(0.001),
            "centimeter" => Ok(0.01),
            "kilometer" => Ok(1000.0),
            "foot" | "internationalFoot" => Ok(0.3048),
            "USSurveyFoot" => Ok(1200.0 / 3937.0),
            other => Err(bad(format!(
                "LandXML: linear unit \"{other}\" not supported"
            ))),
        };
    }
    Ok(1.0)
}

/// Reads the TIN surfaces of a LandXML file (surfaces without faces are
/// left out). Point ids may be any text; faces refer to them.
pub fn read_landxml(text: &str) -> Result<Vec<Mesh>, TerrainError> {
    let scale = landxml_scale(text)?;
    let mut out = Vec::new();
    let mut rest = text;
    let mut n = 0;
    while let Some(start) = rest.find("<Surface") {
        // `<Surfaces>` is the container.
        let after = &rest[start + 8..];
        if after.starts_with('s') {
            rest = after;
            continue;
        }
        let end = after
            .find("</Surface>")
            .ok_or_else(|| bad("LandXML: <Surface> not closed".into()))?;
        let body = &after[..end];
        rest = &after[end..];
        n += 1;
        let tag_end = body.find('>').unwrap_or(0);
        let name = attribute(&body[..tag_end], "name").map_or_else(
            || format!("surface {n}"),
            |v| {
                v.replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&quot;", "\"")
                    .replace("&apos;", "'")
                    .replace("&amp;", "&")
            },
        );
        let mut ids: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
        let mut points = Vec::new();
        let mut triangles = Vec::new();
        let mut b = body;
        // Points.
        while let Some(k) = b.find("<P") {
            let t = &b[k + 2..];
            if !t.starts_with([' ', '>', '\t', '\n', '\r']) {
                b = t;
                continue;
            }
            let close = t
                .find('>')
                .ok_or_else(|| bad("LandXML: <P> not closed".into()))?;
            let tag = &t[..close];
            let content_end = t[close..]
                .find("</P>")
                .map(|e| close + e)
                .ok_or_else(|| bad("LandXML: </P> missing".into()))?;
            let content = &t[close + 1..content_end];
            let v: Vec<f64> = content
                .split_whitespace()
                .map(|x| x.parse::<f64>())
                .collect::<Result<_, _>>()
                .map_err(|e| bad(format!("LandXML point \"{content}\": {e}")))?;
            if v.len() < 3 {
                return Err(bad(format!(
                    "LandXML point \"{content}\": three values needed"
                )));
            }
            let id = attribute(tag, "id").unwrap_or("");
            ids.insert(id, points.len() as u32);
            // Northing, easting, elevation.
            points.push([v[1] * scale, v[0] * scale, v[2] * scale]);
            b = &t[content_end + 4..];
        }
        let mut f = body;
        while let Some(k) = f.find("<F") {
            let t = &f[k + 2..];
            if !t.starts_with([' ', '>', '\t', '\n', '\r']) {
                f = t;
                continue;
            }
            let close = t
                .find('>')
                .ok_or_else(|| bad("LandXML: <F> not closed".into()))?;
            let tag = &t[..close];
            let content_end = t[close..]
                .find("</F>")
                .map(|e| close + e)
                .ok_or_else(|| bad("LandXML: </F> missing".into()))?;
            let content = &t[close + 1..content_end];
            f = &t[content_end + 4..];
            if attribute(tag, "i") == Some("1") {
                continue;
            }
            let mut tri = [0u32; 3];
            let mut it = content.split_whitespace();
            for v in &mut tri {
                let id = it.next().ok_or_else(|| {
                    bad(format!("LandXML face \"{content}\": three points needed"))
                })?;
                *v = *ids
                    .get(id)
                    .ok_or_else(|| bad(format!("LandXML face \"{content}\": no point {id}")))?;
            }
            triangles.push(tri);
        }
        if !triangles.is_empty() {
            out.push(Mesh {
                name,
                points,
                triangles,
            });
        }
    }
    if out.is_empty() {
        return Err(bad(
            "LandXML: no TIN surface (<Surface> with <Faces>)".into()
        ));
    }
    Ok(out)
}

/// Writes surfaces as LandXML 1.2 (points as northing, easting,
/// elevation, ids from 1), coordinates to the millimetre.
pub fn write_landxml(surfaces: &[(&str, &Terrain)]) -> String {
    use std::fmt::Write as _;
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <LandXML xmlns=\"http://www.landxml.org/schema/LandXML-1.2\" version=\"1.2\">\n\
         <Units><Metric linearUnit=\"meter\" areaUnit=\"squareMeter\" volumeUnit=\"cubicMeter\"/></Units>\n\
         <Surfaces>\n",
    );
    for (name, t) in surfaces {
        let esc = name
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('"', "&quot;");
        let _ = writeln!(
            out,
            "<Surface name=\"{esc}\">\n<Definition surfType=\"TIN\">\n<Pnts>"
        );
        for (k, p) in t.points().iter().enumerate() {
            let _ = writeln!(
                out,
                "<P id=\"{}\">{:.3} {:.3} {:.3}</P>",
                k + 1,
                p[1],
                p[0],
                p[2]
            );
        }
        out.push_str("</Pnts>\n<Faces>\n");
        for f in t.triangles() {
            let _ = writeln!(out, "<F>{} {} {}</F>", f[0] + 1, f[1] + 1, f[2] + 1);
        }
        out.push_str("</Faces>\n</Definition>\n</Surface>\n");
    }
    out.push_str("</Surfaces>\n</LandXML>\n");
    out
}

/// Reads an OBJ mesh (vertices and faces; the rest ignored).
pub fn read_obj(text: &str, name: &str) -> Result<Mesh, TerrainError> {
    let mut points = Vec::new();
    let mut triangles = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => {
                let v: Vec<f64> = it
                    .take(3)
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .map_err(|e| TerrainError::Parse {
                        line: n + 1,
                        message: format!("vertex: {e}"),
                    })?;
                if v.len() < 3 {
                    return Err(TerrainError::Parse {
                        line: n + 1,
                        message: "vertex: three values needed".into(),
                    });
                }
                points.push([v[0], v[1], v[2]]);
            }
            Some("f") => {
                let count = points.len() as i64;
                let idx: Vec<u32> = it
                    .map(|t| {
                        let first = t.split('/').next().unwrap_or("");
                        let i: i64 = first.parse().map_err(|_| TerrainError::Parse {
                            line: n + 1,
                            message: format!("face index \"{t}\""),
                        })?;
                        let k = if i < 0 { count + i } else { i - 1 };
                        if k < 0 || k >= count {
                            return Err(TerrainError::Parse {
                                line: n + 1,
                                message: format!("face index {i} out of range"),
                            });
                        }
                        Ok(k as u32)
                    })
                    .collect::<Result<_, _>>()?;
                for k in 1..idx.len().saturating_sub(1) {
                    triangles.push([idx[0], idx[k], idx[k + 1]]);
                }
            }
            _ => {}
        }
    }
    if triangles.is_empty() {
        return Err(bad("OBJ: no face".into()));
    }
    Ok(Mesh {
        name: name.to_owned(),
        points,
        triangles,
    })
}

/// Writes a surface as OBJ (millimetres).
pub fn write_obj(name: &str, t: &Terrain) -> String {
    use std::fmt::Write as _;
    let mut out = format!("# OLS CAD surface {name}\no {name}\n");
    for p in t.points() {
        let _ = writeln!(out, "v {:.3} {:.3} {:.3}", p[0], p[1], p[2]);
    }
    for f in t.triangles() {
        let _ = writeln!(out, "f {} {} {}", f[0] + 1, f[1] + 1, f[2] + 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Terrain {
        Terrain::from_mesh(
            vec![
                [0.0, 0.0, 10.0],
                [10.0, 0.0, 11.0],
                [10.0, 10.0, 12.0],
                [0.0, 10.0, 13.0],
            ],
            vec![[0, 1, 2], [0, 2, 3]],
        )
        .unwrap()
    }

    #[test]
    fn landxml_round_trip() {
        let t = square();
        let text = write_landxml(&[("ground & co", &t), ("copy", &t)]);
        let read = read_landxml(&text).unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].name, "ground & co");
        assert_eq!(read[0].points, t.points());
        assert_eq!(read[0].triangles, t.triangles());
        let back = read[1].clone().terrain().unwrap();
        assert!((back.height(5.0, 2.0).unwrap() - t.height(5.0, 2.0).unwrap()).abs() < 1e-9);
    }

    #[test]
    fn landxml_northing_first_ids_and_invisible_faces() {
        let text = r#"<LandXML><Surfaces><Surface name='S'><Definition surfType="TIN">
            <Pnts><P id="a">100 200 5</P><P id="b">100 210 6</P><P id='c'>110 210 7</P></Pnts>
            <Faces><F>a b c</F><F i="1">a c b</F></Faces></Definition></Surface></Surfaces></LandXML>"#;
        let m = &read_landxml(text).unwrap()[0];
        assert_eq!(m.name, "S");
        assert_eq!(m.points[0], [200.0, 100.0, 5.0]);
        assert_eq!(m.triangles, vec![[0, 1, 2]]);
        assert!(read_landxml("<LandXML/>").is_err());
        assert!(read_landxml("<Surface><Faces><F>1 2 9</F></Faces></Surface>").is_err());
    }

    #[test]
    fn landxml_units_and_malformed_tags() {
        let surface = r#"<Surfaces><Surface name="S"><Definition><Pnts><P id="a">0 0 10</P><P id="b">0 10 10</P><P id="c">10 0 10</P></Pnts><Faces><F>a b c</F></Faces></Definition></Surface></Surfaces>"#;
        let feet =
            format!(r#"<LandXML><Units><Imperial linearUnit="foot"/></Units>{surface}</LandXML>"#);
        let m = &read_landxml(&feet).unwrap()[0];
        assert!((m.points[0][2] - 3.048).abs() < 1e-12);
        let mm = format!(
            r#"<LandXML><Units><Metric linearUnit="millimeter"/></Units>{surface}</LandXML>"#
        );
        assert!((read_landxml(&mm).unwrap()[0].points[1][0] - 0.01).abs() < 1e-12);
        let odd =
            format!(r#"<LandXML><Units><Imperial linearUnit="chain"/></Units>{surface}</LandXML>"#);
        assert!(read_landxml(&odd).is_err());
        // A tag closed after its end tag: an error, not a panic.
        assert!(read_landxml("<Surface name='S'><P </P> 1 2 3></Surface>").is_err());
    }

    #[test]
    fn obj_round_trip_and_polygons() {
        let t = square();
        let m = read_obj(&write_obj("g", &t), "g").unwrap();
        assert_eq!(m.points, t.points());
        assert_eq!(m.triangles, t.triangles());
        let quad = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1/1 2/2 3/3 -1\n";
        assert_eq!(
            read_obj(quad, "q").unwrap().triangles,
            vec![[0, 1, 2], [0, 2, 3]]
        );
        assert!(read_obj("v 0 0 0\nf 1 2 3\n", "x").is_err());
    }
}
