// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Safety volumes ([`ohl_clearance::safety_volume`]) written as
//! georeferenced files:
//!
//! - **DXF** (R12, ASCII): every triangle a `3DFACE` (its fourth corner
//!   the third), a layer per volume, in the line's coordinates; a `.prj`
//!   beside it with the coordinate system as WKT (EPSG systems);
//! - **IFC4** (STEP): each volume a spatial element, an `IfcSpatialZone`
//!   (predefined type `USERDEFINED`, object type `SafetyVolume`; since
//!   2026-10-21) referenced in the site by an
//!   `IfcRelReferencedInSpatialStructure` (the spatial zone's own relation
//!   to the spatial structure: non-hierarchical, overlapping), shaped by
//!   an `IfcTriangulatedFaceSet`, translucent, with property set
//!   `OHL_SafetyVolume`; coordinates
//!   relative to a whole-metre origin given by `IfcMapConversion`
//!   (eastings, northings, orthogonal height) to an `IfcProjectedCRS`
//!   named by its EPSG code;
//! - **KML / KMZ**: each volume a placemark of a `MultiGeometry` of
//!   triangles (polygons, altitude absolute) in WGS84 through the line's
//!   coordinate system; heights kept as they are (the line's heights above
//!   its height datum, which Google Earth takes as above its geoid; no
//!   geoid model is applied).

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use ohl_clearance::safety_volume::{SafetyVolume, SafetyVolumes};
use ohl_crs::{Crs, Heights, Transformation};

use crate::ifc::{Step, real, string};

/// What the files say about a run besides its volumes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Description {
    /// The line's name.
    pub line: String,
    /// The line's coordinate system (`EPSG:<code>` or proj4; may be
    /// informative only).
    pub crs: String,
    /// The distance as given (a requirement in words).
    pub distance: String,
    /// The wind, in words.
    pub wind: String,
    /// The date (`YYYY-MM-DD`).
    pub date: String,
}

/// A file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// DXF with a `.prj`.
    Dxf,
    /// IFC4.
    Ifc,
    /// KML.
    Kml,
    /// KMZ (zipped KML).
    Kmz,
}

impl Format {
    /// Every format.
    pub const ALL: [Format; 4] = [Self::Dxf, Self::Ifc, Self::Kml, Self::Kmz];

    /// Its extension and name: `dxf`, `ifc`, `kml`, `kmz`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dxf => "dxf",
            Self::Ifc => "ifc",
            Self::Kml => "kml",
            Self::Kmz => "kmz",
        }
    }

    /// Reads a list `dxf,ifc,kmz`.
    pub fn parse_list(text: &str) -> Result<Vec<Format>, String> {
        let mut out = Vec::new();
        for t in text.split(',').map(str::trim).filter(|t| !t.is_empty()) {
            let f = Self::ALL
                .into_iter()
                .find(|f| f.as_str() == t.to_ascii_lowercase())
                .ok_or_else(|| format!("format \"{t}\": use dxf, ifc, kml or kmz"))?;
            if !out.contains(&f) {
                out.push(f);
            }
        }
        if out.is_empty() {
            return Err("no format given (dxf, ifc, kml, kmz)".into());
        }
        Ok(out)
    }
}

/// The properties of a volume, as written to IFC and KML.
pub fn properties(
    v: &SafetyVolume,
    all: &SafetyVolumes,
    d: &Description,
) -> Vec<(&'static str, String)> {
    vec![
        ("Line", d.line.clone()),
        ("Circuit", v.circuit.clone()),
        ("Phases", v.phases.join(", ")),
        ("Sections", v.sections.join(", ")),
        (
            "NominalVoltage_kV",
            v.kv.map_or("earthed (0)".into(), |k| format!("{k}")),
        ),
        ("Distance_m", format!("{:.3}", v.distance)),
        ("Margin_m", format!("{:.3}", v.margin)),
        ("DistanceRule", d.distance.clone()),
        ("WeatherCases", all.cases.join(", ")),
        ("Wind", d.wind.clone()),
        ("Date", d.date.clone()),
        ("Tolerance_m", format!("{:.3}", v.allowance)),
        ("Volume_m3", format!("{:.1}", v.mesh.volume())),
        ("CoordinateSystem", d.crs.clone()),
    ]
}

/// A layer name R12 takes: letters, digits, `$ - _`, at most 31.
fn layer_name(name: &str) -> String {
    let s: String = format!("SAFETY_{name}")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '$' | '-' | '_') {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    s.chars().take(31).collect()
}

/// The ACI colour of volume `k` (red, yellow, green, cyan, blue,
/// magenta, in turn).
fn aci(k: usize) -> u8 {
    [1, 2, 3, 4, 5, 6][k % 6]
}

/// RGB of volume `k` (as [`aci`]).
fn rgb(k: usize) -> [u8; 3] {
    [
        [255, 0, 0],
        [255, 255, 0],
        [0, 255, 0],
        [0, 255, 255],
        [0, 0, 255],
        [255, 0, 255],
    ][k % 6]
}

/// The DXF (R12): a `3DFACE` per triangle, a layer per volume, the
/// line's coordinates to 0.1 mm.
pub fn dxf(all: &SafetyVolumes) -> String {
    let mut layers: Vec<String> = Vec::new();
    for v in &all.volumes {
        let mut n = layer_name(&v.name);
        let mut k = 2;
        while layers.contains(&n) {
            let suffix = format!("_{k}");
            n = format!("{}{suffix}", &layer_name(&v.name)[..31 - suffix.len()]);
            k += 1;
        }
        layers.push(n);
    }
    let mut out = String::new();
    let mut pair = |code: u32, value: &str| {
        let _ = write!(out, "{code:>3}\n{value}\n");
    };
    pair(0, "SECTION");
    pair(2, "HEADER");
    pair(9, "$ACADVER");
    pair(1, "AC1009");
    pair(0, "ENDSEC");
    pair(0, "SECTION");
    pair(2, "TABLES");
    pair(0, "TABLE");
    pair(2, "LAYER");
    pair(70, &layers.len().to_string());
    for (k, l) in layers.iter().enumerate() {
        pair(0, "LAYER");
        pair(2, l);
        pair(70, "0");
        pair(62, &aci(k).to_string());
        pair(6, "CONTINUOUS");
    }
    pair(0, "ENDTAB");
    pair(0, "ENDSEC");
    pair(0, "SECTION");
    pair(2, "ENTITIES");
    for (v, l) in all.volumes.iter().zip(&layers) {
        for t in &v.mesh.triangles {
            let c = v.mesh.corners(t);
            pair(0, "3DFACE");
            pair(8, l);
            for (k, p) in [c[0], c[1], c[2], c[2]].iter().enumerate() {
                let k = k as u32;
                pair(10 + k, &format!("{:.4}", p[0]));
                pair(20 + k, &format!("{:.4}", p[1]));
                pair(30 + k, &format!("{:.4}", p[2]));
            }
        }
    }
    pair(0, "ENDSEC");
    pair(0, "EOF");
    out
}

/// The `.prj` of a coordinate system: its WKT (EPSG systems only).
pub fn prj(crs: &str) -> Option<&'static str> {
    Crs::parse(crs).ok()?.wkt()
}

/// The IFC4 file.
pub fn ifc(all: &SafetyVolumes, d: &Description) -> Result<String, String> {
    let mut f = Step {
        lines: Vec::new(),
        seed: crate::ifc::hash(&format!("{}|safety|{}", d.line, d.date)),
    };
    // A whole-metre origin near the volumes: the map conversion's.
    let origin = all
        .volumes
        .iter()
        .find_map(|v| v.mesh.bounds())
        .map_or([0.0; 3], |(lo, _)| lo.map(f64::floor));
    let zero = f.add("IFCCARTESIANPOINT", "(0.,0.,0.)");
    let up = f.add("IFCDIRECTION", "(0.,0.,1.)");
    let east = f.add("IFCDIRECTION", "(1.,0.,0.)");
    let world = f.add("IFCAXIS2PLACEMENT3D", &format!("{zero},{up},{east}"));
    let context = f.add(
        "IFCGEOMETRICREPRESENTATIONCONTEXT",
        &format!("$,'Model',3,1.E-05,{world},$"),
    );
    let body = f.add(
        "IFCGEOMETRICREPRESENTATIONSUBCONTEXT",
        &format!("'Body','Model',*,*,*,*,{context},$,.MODEL_VIEW.,$"),
    );
    let metre = f.add("IFCSIUNIT", "*,.LENGTHUNIT.,$,.METRE.");
    let radian = f.add("IFCSIUNIT", "*,.PLANEANGLEUNIT.,$,.RADIAN.");
    let volt = f.add("IFCSIUNIT", "*,.ELECTRICVOLTAGEUNIT.,$,.VOLT.");
    let units = f.add("IFCUNITASSIGNMENT", &format!("({metre},{radian},{volt})"));
    let g = f.guid();
    let project = f.add(
        "IFCPROJECT",
        &format!(
            "{g},$,{},{},$,$,$,({context}),{units}",
            string(&format!("{} — safety volumes", d.line)),
            string(&format!(
                "Safety volumes of the line, coordinates {}",
                d.crs
            ))
        ),
    );
    // Georeferencing: the projected system, and the map conversion from
    // the model's coordinates (metres from the origin) to it.
    let crs = Crs::parse(&d.crs).ok();
    let (crs_name, crs_text) = match crs.as_ref().and_then(Crs::epsg) {
        Some(code) => (
            format!("EPSG:{code}"),
            ohl_crs::known_code(code).map_or(String::new(), |k| k.name.to_owned()),
        ),
        None => (d.crs.clone(), "not an EPSG system".to_owned()),
    };
    let projected = f.add(
        "IFCPROJECTEDCRS",
        &format!(
            "{},{},$,$,$,$,{metre}",
            string(&crs_name),
            string(&crs_text)
        ),
    );
    f.add(
        "IFCMAPCONVERSION",
        &format!(
            "{context},{projected},{},{},{},1.,0.,1.",
            real(origin[0]),
            real(origin[1]),
            real(origin[2])
        ),
    );
    let site_axes = f.add("IFCAXIS2PLACEMENT3D", &format!("{zero},$,$"));
    let site_place = f.add("IFCLOCALPLACEMENT", &format!("$,{site_axes}"));
    let g = f.guid();
    let site = f.add(
        "IFCSITE",
        &format!(
            "{g},$,{},$,$,{site_place},$,$,.ELEMENT.,$,$,$,$,$",
            string(&d.line)
        ),
    );
    let g = f.guid();
    f.add("IFCRELAGGREGATES", &format!("{g},$,$,$,{project},({site})"));
    let mut elements = Vec::new();
    for (k, v) in all.volumes.iter().enumerate() {
        let mut coords = String::new();
        for (i, p) in v.mesh.vertices.iter().enumerate() {
            if i > 0 {
                coords.push(',');
            }
            let _ = write!(
                coords,
                "({},{},{})",
                real(round4(p[0] - origin[0])),
                real(round4(p[1] - origin[1])),
                real(round4(p[2] - origin[2]))
            );
        }
        let list = f.add("IFCCARTESIANPOINTLIST3D", &format!("({coords})"));
        let mut index = String::new();
        for (i, t) in v.mesh.triangles.iter().enumerate() {
            if i > 0 {
                index.push(',');
            }
            let _ = write!(index, "({},{},{})", t[0] + 1, t[1] + 1, t[2] + 1);
        }
        let faces = f.add(
            "IFCTRIANGULATEDFACESET",
            &format!("{list},$,.T.,({index}),$"),
        );
        let [r, gr, b] = rgb(k).map(|c| real(f64::from(c) / 255.0));
        let colour = f.add("IFCCOLOURRGB", &format!("$,{r},{gr},{b}"));
        let shading = f.add("IFCSURFACESTYLESHADING", &format!("{colour},0.6"));
        let style = f.add(
            "IFCSURFACESTYLE",
            &format!("{},.BOTH.,({shading})", string(&v.name)),
        );
        f.add("IFCSTYLEDITEM", &format!("{faces},({style}),$"));
        let rep = f.add(
            "IFCSHAPEREPRESENTATION",
            &format!("{body},'Body','Tessellation',({faces})"),
        );
        let shape = f.add("IFCPRODUCTDEFINITIONSHAPE", &format!("$,$,({rep})"));
        let axes = f.add("IFCAXIS2PLACEMENT3D", &format!("{zero},$,$"));
        let place = f.add("IFCLOCALPLACEMENT", &format!("{site_place},{axes}"));
        let g = f.guid();
        // IfcSpatialZone (IFC4): GlobalId, OwnerHistory, Name,
        // Description, ObjectType, ObjectPlacement, Representation,
        // LongName, PredefinedType.
        let e = f.add(
            "IFCSPATIALZONE",
            &format!(
                "{g},$,{},{},'SafetyVolume',{place},{shape},{},.USERDEFINED.",
                string(&v.name),
                string(&format!(
                    "Safety volume: {} m from the conductors ({})",
                    v.distance + v.margin,
                    d.wind
                )),
                string(&format!("Safety volume {}", v.name))
            ),
        );
        let text = |t: &str| format!("IFCLABEL({})", string(t));
        let values: Vec<(&str, String)> = properties(v, all, d)
            .into_iter()
            .map(|(n, val)| (n, text(&val)))
            .collect();
        f.properties(&e, "OHL_SafetyVolume", &values);
        elements.push(e);
    }
    if !elements.is_empty() {
        let g = f.guid();
        f.add(
            "IFCRELREFERENCEDINSPATIALSTRUCTURE",
            &format!("{g},$,'Safety volumes',$,({}),{site}", elements.join(",")),
        );
    }
    let (date, time) = crate::landxml::utc_now();
    let mut out = format!(
        "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('ViewDefinition [ReferenceView_V1.2]'),'2;1');\n\
         FILE_NAME({},'{date}T{time}',(''),(''),'OpenOHL {}','OpenOHL','');\n\
         FILE_SCHEMA(('IFC4'));\nENDSEC;\nDATA;\n",
        string(&format!("{} safety volumes", d.line)),
        env!("CARGO_PKG_VERSION")
    );
    for (k, l) in f.lines.iter().enumerate() {
        let _ = writeln!(out, "#{}={l};", k + 1);
    }
    out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
    Ok(out)
}

/// To 0.1 mm.
fn round4(v: f64) -> f64 {
    (v * 1e4).round() / 1e4
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The KML document: WGS84 longitude, latitude (8 decimals: about 1 mm)
/// and the heights as they are (3 decimals).
pub fn kml(all: &SafetyVolumes, d: &Description) -> Result<String, String> {
    if d.crs.trim().is_empty() {
        return Err("KML needs the line's coordinate system: set it (EPSG:<code>) first".into());
    }
    let from = Crs::parse(&d.crs)
        .map_err(|e| format!("{e} (KML needs the line's real coordinate system: EPSG:<code>)"))?;
    let to = Transformation::new(&from, &Crs::parse("EPSG:4326")?, Heights::Keep);
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<kml xmlns=\"http://www.opengis.net/kml/2.2\">\n<Document>\n",
    );
    let _ = writeln!(out, "<name>{} — safety volumes</name>", xml(&d.line));
    let _ = writeln!(
        out,
        "<description>{} from the conductors; {}; weather cases {}; heights as in the line ({}), not corrected to a geoid</description>",
        xml(&d.distance),
        xml(&d.wind),
        xml(&all.cases.join(", ")),
        xml(&d.crs)
    );
    for k in 0..all.volumes.len() {
        let [r, g, b] = rgb(k);
        let _ = writeln!(
            out,
            "<Style id=\"v{k}\"><LineStyle><width>0</width></LineStyle><PolyStyle><color>80{b:02x}{g:02x}{r:02x}</color><outline>0</outline></PolyStyle></Style>"
        );
    }
    for (k, v) in all.volumes.iter().enumerate() {
        let mut points = v.mesh.vertices.clone();
        to.points(&mut points)?;
        let text: Vec<String> = points
            .iter()
            .map(|q| format!("{:.8},{:.8},{:.3}", q[0], q[1], q[2]))
            .collect();
        let _ = writeln!(
            out,
            "<Placemark><name>{}</name><styleUrl>#v{k}</styleUrl>",
            xml(&v.name)
        );
        out.push_str("<ExtendedData>");
        for (n, val) in properties(v, all, d) {
            let _ = write!(
                out,
                "<Data name=\"{n}\"><value>{}</value></Data>",
                xml(&val)
            );
        }
        out.push_str("</ExtendedData>\n<MultiGeometry>\n");
        for t in &v.mesh.triangles {
            let [a, b, c] = t.map(|i| &text[i as usize]);
            let _ = writeln!(
                out,
                "<Polygon><altitudeMode>absolute</altitudeMode><outerBoundaryIs><LinearRing><coordinates>{a} {b} {c} {a}</coordinates></LinearRing></outerBoundaryIs></Polygon>"
            );
        }
        out.push_str("</MultiGeometry></Placemark>\n");
    }
    out.push_str("</Document>\n</kml>\n");
    Ok(out)
}

/// The KMZ: [`kml`] zipped as `doc.kml`.
pub fn kmz(all: &SafetyVolumes, d: &Description) -> Result<Vec<u8>, String> {
    let doc = kml(all, d)?;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("doc.kml", opts).map_err(|e| e.to_string())?;
    zip.write_all(doc.as_bytes()).map_err(|e| e.to_string())?;
    Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
}

/// Writes the volumes in `formats` into `dir` as `<stem>.<ext>` (a DXF
/// with its `.prj` when the system is an EPSG one); returns the files
/// written and notes (a KML without the line's real coordinate system is
/// not written, said in a note). Each file is written whole or not at
/// all.
pub fn write(
    dir: &Path,
    stem: &str,
    all: &SafetyVolumes,
    d: &Description,
    formats: &[Format],
) -> Result<(Vec<PathBuf>, Vec<String>), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut files = Vec::new();
    let mut notes = Vec::new();
    let save = |name: String, bytes: &[u8], files: &mut Vec<PathBuf>| -> Result<(), String> {
        let path = dir.join(name);
        let tmp = path.with_extension("part");
        std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))?;
        files.push(path);
        Ok(())
    };
    for f in formats {
        match f {
            Format::Dxf => {
                save(format!("{stem}.dxf"), dxf(all).as_bytes(), &mut files)?;
                match prj(&d.crs) {
                    Some(w) => save(format!("{stem}.prj"), w.as_bytes(), &mut files)?,
                    None => notes.push(format!(
                        "no .prj written: the line's coordinate system \"{}\" is not an EPSG system",
                        d.crs
                    )),
                }
            }
            Format::Ifc => {
                if Crs::parse(&d.crs).ok().and_then(|c| c.epsg()).is_none() {
                    notes.push(format!(
                        "IFC: the coordinate system \"{}\" has no EPSG code (named as it is)",
                        d.crs
                    ));
                }
                save(format!("{stem}.ifc"), ifc(all, d)?.as_bytes(), &mut files)?;
            }
            // Without a real coordinate system: said, the others written.
            Format::Kml => match kml(all, d) {
                Ok(doc) => save(format!("{stem}.kml"), doc.as_bytes(), &mut files)?,
                Err(e) => notes.push(format!("no KML written: {e}")),
            },
            Format::Kmz => match kmz(all, d) {
                Ok(z) => save(format!("{stem}.kmz"), &z, &mut files)?,
                Err(e) => notes.push(format!("no KMZ written: {e}")),
            },
        }
    }
    Ok((files, notes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ohl_clearance::checks::Wire;
    use ohl_clearance::positions::Positions;
    use ohl_clearance::rules::Requirement;
    use ohl_clearance::safety_volume::{SafetyParams, build};
    use ohl_core::budget::Budget;

    /// A straight wire 40 m long, 3 m: one volume, in invented LV95
    /// coordinates (EPSG:2056, near the system's false origin).
    fn volumes() -> (SafetyVolumes, Description) {
        let w = Wire {
            section: "S".into(),
            span: 0,
            circuit: "c".into(),
            phase: "L1".into(),
            potential: None,
            bundle: vec![],
            points: (0..=4)
                .map(|k| [2_600_000.0 + 10.0 * f64::from(k), 1_200_000.0, 520.0])
                .collect(),
            ends: vec![],
            surface_radius: 0.01,
            polyline_error: 0.0,
        };
        let p = SafetyParams {
            distance: Requirement::Fixed(3.0),
            tolerance: 0.1,
            ..SafetyParams::default()
        };
        let v = build(&Positions::of_wires("still", vec![w]), &p, &Budget::new()).unwrap();
        let d = Description {
            line: "Test line".into(),
            crs: "EPSG:2056".into(),
            distance: "3.00 m".into(),
            wind: "blowouts at full wind".into(),
            date: "2026-10-20".into(),
        };
        (v, d)
    }

    /// DXF: read back, every face is a triangle of the mesh in the line's
    /// coordinates (to 0.1 mm); the drawing reader of the backgrounds
    /// reads it (faces counted, not drawn); the `.prj` names EPSG:2056.
    #[test]
    fn dxf_round_trip() {
        let (v, _) = volumes();
        let text = dxf(&v);
        let lines: Vec<&str> = text.lines().collect();
        let mut faces: Vec<[[f64; 3]; 4]> = Vec::new();
        let mut k = 0;
        while k + 1 < lines.len() {
            let (code, value) = (lines[k].trim(), lines[k + 1]);
            if code == "0" && value == "3DFACE" {
                faces.push([[0.0; 3]; 4]);
            } else if let (Ok(c), Some(f)) = (code.parse::<usize>(), faces.last_mut())
                && (10..=33).contains(&c)
                && c % 10 <= 3
            {
                f[c % 10][c / 10 - 1] = value.parse().unwrap();
            }
            k += 2;
        }
        let mesh = &v.volumes[0].mesh;
        assert_eq!(faces.len(), mesh.triangles.len());
        for (f, t) in faces.iter().zip(&mesh.triangles) {
            let c = mesh.corners(t);
            for i in 0..3 {
                for j in 0..3 {
                    assert!((f[i][j] - c[i][j]).abs() <= 5e-5);
                }
            }
            assert_eq!(f[3], f[2]);
            // In the line's coordinates.
            assert!((f[0][0] - 2_600_020.0).abs() < 30.0 && (f[0][1] - 1_200_000.0).abs() < 10.0);
        }
        let read = ohl_background::dxf::read_dxf(text.as_bytes()).unwrap();
        assert_eq!(read.skipped.get("3DFACE"), Some(&faces.len()));
        assert_eq!(
            read.layers
                .iter()
                .filter(|l| l.name == "SAFETY_C_L1")
                .count(),
            1
        );
        assert_eq!(
            ohl_crs::from_prj(prj("EPSG:2056").unwrap()).unwrap(),
            "EPSG:2056"
        );
        assert_eq!(prj("local (invented)"), None);
    }

    /// IFC4: a well-formed STEP file (every reference resolves, IFC4
    /// attribute counts); the map conversion's origin plus each point of
    /// the face set gives the mesh's vertex in the line's coordinates; the
    /// projected system is EPSG:2056; the properties are there.
    #[test]
    fn ifc_parses_and_is_georeferenced() {
        let (v, d) = volumes();
        let text = ifc(&v, &d).unwrap();
        assert!(text.contains("FILE_SCHEMA(('IFC4'));"));
        let data = text
            .split("DATA;\n")
            .nth(1)
            .unwrap()
            .split("ENDSEC;")
            .next()
            .unwrap();
        let mut entities: std::collections::BTreeMap<usize, (String, String)> = Default::default();
        for l in data.lines() {
            let (id, rest) = l.split_once('=').unwrap();
            let id: usize = id[1..].parse().unwrap();
            let rest = rest.strip_suffix(");").unwrap();
            let (name, args) = rest.split_once('(').unwrap();
            entities.insert(id, (name.to_owned(), args.to_owned()));
        }
        for (_, args) in entities.values() {
            for r in args.split('#').skip(1) {
                let n: usize = r
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse()
                    .unwrap();
                assert!(entities.contains_key(&n), "#{n}");
            }
        }
        let find = |name: &str| {
            entities
                .values()
                .find(|(n, _)| n == name)
                .map(|(_, a)| a.clone())
                .unwrap()
        };
        let top = |a: &str| {
            let (mut depth, mut quoted, mut out) = (0, false, vec![String::new()]);
            for c in a.chars() {
                match c {
                    '\'' => quoted = !quoted,
                    '(' if !quoted => depth += 1,
                    ')' if !quoted => depth -= 1,
                    ',' if !quoted && depth == 0 => {
                        out.push(String::new());
                        continue;
                    }
                    _ => {}
                }
                out.last_mut().unwrap().push(c);
            }
            out
        };
        // IFC4 attribute counts.
        for (name, n) in [
            ("IFCMAPCONVERSION", 8),
            ("IFCPROJECTEDCRS", 7),
            ("IFCTRIANGULATEDFACESET", 5),
            ("IFCCARTESIANPOINTLIST3D", 1),
            ("IFCSPATIALZONE", 9),
            ("IFCRELREFERENCEDINSPATIALSTRUCTURE", 6),
            ("IFCSURFACESTYLESHADING", 2),
            ("IFCSTYLEDITEM", 3),
            ("IFCSURFACESTYLE", 3),
            ("IFCCOLOURRGB", 4),
        ] {
            assert_eq!(top(&find(name)).len(), n, "{name}");
        }
        assert!(find("IFCPROJECTEDCRS").starts_with("'EPSG:2056'"));
        let mc = top(&find("IFCMAPCONVERSION"));
        let e: f64 = mc[2].parse().unwrap();
        let n: f64 = mc[3].parse().unwrap();
        let h: f64 = mc[4].parse().unwrap();
        let list = find("IFCCARTESIANPOINTLIST3D");
        let pts: Vec<[f64; 3]> = list
            .trim_matches(|c| c == '(' || c == ')')
            .split("),(")
            .map(|p| {
                let v: Vec<f64> = p.split(',').map(|x| x.parse().unwrap()).collect();
                [v[0] + e, v[1] + n, v[2] + h]
            })
            .collect();
        let mesh = &v.volumes[0].mesh;
        assert_eq!(pts.len(), mesh.vertices.len());
        for (p, q) in pts.iter().zip(&mesh.vertices) {
            for k in 0..3 {
                assert!((p[k] - q[k]).abs() <= 6e-5, "{p:?} {q:?}");
            }
        }
        // A spatial element: a spatial zone, user-defined as a safety
        // volume, referenced in the site; no building element proxy.
        let zone = top(&find("IFCSPATIALZONE"));
        assert_eq!(zone[4], "'SafetyVolume'");
        assert_eq!(zone[8], ".USERDEFINED.");
        assert!(!text.contains("IFCBUILDINGELEMENTPROXY"));
        let zone_id = entities
            .iter()
            .find(|(_, (n, _))| n == "IFCSPATIALZONE")
            .map(|(k, _)| *k)
            .unwrap();
        let site_id = entities
            .iter()
            .find(|(_, (n, _))| n == "IFCSITE")
            .map(|(k, _)| *k)
            .unwrap();
        let rel = top(&find("IFCRELREFERENCEDINSPATIALSTRUCTURE"));
        assert_eq!(rel[4], format!("(#{zone_id})"));
        assert_eq!(rel[5], format!("#{site_id}"));
        // The site is aggregated in the project; the zone placed relative
        // to the site.
        assert!(top(&find("IFCRELAGGREGATES"))[5] == format!("(#{site_id})"));
        // Its property set is attached to it.
        let defines = top(&find("IFCRELDEFINESBYPROPERTIES"));
        assert_eq!(defines[4], format!("(#{zone_id})"));
        assert!(text.contains("'OHL_SafetyVolume'"));
        for prop in [
            "'Distance_m'",
            "'WeatherCases'",
            "'Wind'",
            "'NominalVoltage_kV'",
            "'Date'",
        ] {
            assert!(text.contains(prop), "{prop}");
        }
    }

    /// KML: each triangle's corners transformed back from WGS84 to the
    /// line's system match the mesh within 1 cm (heights kept).
    #[test]
    fn kml_round_trip() {
        let (v, d) = volumes();
        let text = kml(&v, &d).unwrap();
        let back = Transformation::new(
            &Crs::parse("EPSG:4326").unwrap(),
            &Crs::parse("EPSG:2056").unwrap(),
            Heights::Keep,
        );
        let mesh = &v.volumes[0].mesh;
        let mut n = 0;
        let mut worst = 0.0f64;
        for (chunk, t) in text.split("<coordinates>").skip(1).zip(&mesh.triangles) {
            let coords = chunk.split("</coordinates>").next().unwrap();
            let mut pts: Vec<[f64; 3]> = coords
                .split_whitespace()
                .map(|c| {
                    let v: Vec<f64> = c.split(',').map(|x| x.parse().unwrap()).collect();
                    [v[0], v[1], v[2]]
                })
                .collect();
            back.points(&mut pts).unwrap();
            for (p, q) in pts.iter().zip(mesh.corners(t)) {
                for k in 0..3 {
                    worst = worst.max((p[k] - q[k]).abs());
                }
            }
            n += 1;
        }
        println!("KML: {n} triangles, worst {worst:.5} m after the round trip");
        assert_eq!(n, mesh.triangles.len());
        assert!(worst < 0.01, "{worst}");
        // Zipped: a KMZ with doc.kml.
        let z = kmz(&v, &d).unwrap();
        assert_eq!(&z[..2], b"PK");
        // A system that is not one: refused, saying why.
        let e = kml(
            &v,
            &Description {
                crs: "local (invented)".into(),
                ..d
            },
        )
        .unwrap_err();
        assert!(e.contains("EPSG"), "{e}");
    }

    #[test]
    fn formats_are_read() {
        assert_eq!(
            Format::parse_list("dxf, IFC,kmz").unwrap(),
            vec![Format::Dxf, Format::Ifc, Format::Kmz]
        );
        assert!(Format::parse_list("obj").is_err());
    }
}
