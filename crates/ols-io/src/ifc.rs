// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! IFC4 (buildingSMART, ISO 16739-1) as a STEP physical file (ISO
//! 10303-21): an `IfcProject` with metre/radian/newton/volt units, a
//! 3D model context, one `IfcSite` placed at the first structure (so
//! coordinates in the file stay small), and in it
//!
//! - every structure as an `IfcBuildingElementProxy` placed and turned
//!   like the structure, drawn as 3D polylines (its stick drawing, else a
//!   mast with arms to the insulator points) and the insulator strings,
//!   with property set `OHL_Structure`;
//! - every span (and jumper) of the chosen weather cases (option `cases`:
//!   `first`, the default, or `all`) as an `IfcCableSegment`
//!   (`CONDUCTORSEGMENT`) drawn as a 3D polyline, with property set
//!   `OHL_Wire` (case, section, span, cable, tensions, sag, voltage).
//!
//! Heights and plan coordinates are the line's (no map conversion is
//! written). Linear in the number of points.

use std::fmt::Write as _;

use ols_line::{Line, Point};

use crate::{ExportModel, Exporter, Options};

const GUID_CHARS: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";

/// A STEP real (always with a decimal point).
pub fn real(x: f64) -> String {
    let s = format!("{x:?}");
    match s.split_once('e') {
        Some((m, e)) => format!("{m}{}E{e}", if m.contains('.') { "" } else { "." }),
        None if s.contains('.') => s,
        None => format!("{s}."),
    }
}

/// A STEP string (quotes doubled, backslashes escaped, other than ASCII
/// as `\X2\…\X0\`).
pub fn string(t: &str) -> String {
    let mut out = String::from("'");
    for c in t.chars() {
        match c {
            '\'' => out.push_str("''"),
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(c),
            c => {
                out.push_str("\\X2\\");
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    let _ = write!(out, "{u:04X}");
                }
                out.push_str("\\X0\\");
            }
        }
    }
    out.push('\'');
    out
}

/// An IFC GlobalId (22 characters) for a 128-bit number.
pub fn guid(v: u128) -> String {
    let mut s = String::with_capacity(22);
    s.push(GUID_CHARS[((v >> 126) & 3) as usize] as char);
    for k in 1..22 {
        s.push(GUID_CHARS[((v >> (126 - 6 * k)) & 63) as usize] as char);
    }
    s
}

/// The entities of a file being written.
struct Step {
    lines: Vec<String>,
    seed: u64,
}

impl Step {
    /// Adds `TYPE(args)`; returns its reference `#n`.
    fn add(&mut self, entity: &str, args: &str) -> String {
        self.lines.push(format!("{entity}({args})"));
        format!("#{}", self.lines.len())
    }

    /// A GlobalId, the same for the same line and entity number.
    fn guid(&mut self) -> String {
        let mut next = || {
            // splitmix64
            self.seed = self.seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = self.seed;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        };
        let v = (u128::from(next()) << 64) | u128::from(next());
        string(&guid(v))
    }

    fn point(&mut self, p: Point) -> String {
        self.add(
            "IFCCARTESIANPOINT",
            &format!("({},{},{})", real(p[0]), real(p[1]), real(p[2])),
        )
    }

    fn polyline(&mut self, points: &[Point]) -> String {
        let refs: Vec<String> = points.iter().map(|p| self.point(*p)).collect();
        self.add("IFCPOLYLINE", &format!("({})", refs.join(",")))
    }

    /// A property set on an element.
    fn properties(&mut self, element: &str, name: &str, values: &[(&str, String)]) {
        let props: Vec<String> = values
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(n, v)| self.add("IFCPROPERTYSINGLEVALUE", &format!("{},$,{v},$", string(n))))
            .collect();
        let g = self.guid();
        let set = self.add(
            "IFCPROPERTYSET",
            &format!("{g},$,{},$,({})", string(name), props.join(",")),
        );
        let g = self.guid();
        self.add(
            "IFCRELDEFINESBYPROPERTIES",
            &format!("{g},$,$,$,({element}),{set}"),
        );
    }
}

fn label(t: &str) -> String {
    format!("IFCLABEL({})", string(t))
}

fn opt<T>(v: Option<T>, f: impl Fn(T) -> String) -> String {
    v.map(f).unwrap_or_default()
}

fn hash(text: &str) -> u64 {
    // FNV-1a: GlobalIds stay the same from one export to the next.
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A structure's polylines in its own frame (x along its orientation).
fn structure_sticks(s: &ols_line::Structure) -> Vec<Vec<Point>> {
    let (sin, cos) = s.orientation_deg.to_radians().sin_cos();
    let local = |q: Point| {
        let (dx, dy) = (q[0] - s.position[0], q[1] - s.position[1]);
        [
            dx * cos + dy * sin,
            -dx * sin + dy * cos,
            q[2] - s.position[2],
        ]
    };
    let mut lines: Vec<Vec<Point>> = s
        .members
        .iter()
        .filter(|m| m.len() >= 2)
        .map(|m| m.iter().map(|q| local(*q)).collect())
        .collect();
    if lines.is_empty() {
        let top = s
            .points
            .iter()
            .map(|p| local(p.position)[2])
            .fold(0.0, f64::max);
        let top = if top > 0.0 { top } else { 10.0 };
        lines.push(vec![[0.0, 0.0, 0.0], [0.0, 0.0, top]]);
        for q in &s.points {
            let p = local(q.position);
            if p[0].hypot(p[1]) > 0.01 {
                lines.push(vec![[0.0, 0.0, p[2]], p]);
            }
        }
    }
    // The strings, as given.
    for x in &s.strings {
        if let (Some(top), Some(wire)) = (s.node(&x.top), s.wire(&x.wire)) {
            lines.push(vec![local(top.position()), local(wire.position)]);
        }
    }
    lines
}

/// The IFC file of a model.
pub fn document(model: &ExportModel, options: &Options) -> Result<String, String> {
    let line: &Line = &model.line;
    let cases = crate::gis::chosen_cases(model, options, false)?;
    let mut f = Step {
        lines: Vec::new(),
        seed: hash(&line.name),
    };
    let origin = line.structures.first().map_or([0.0; 3], |s| s.position);
    let zero = f.point([0.0; 3]);
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
    let units: Vec<String> = [
        ".LENGTHUNIT.,$,.METRE.",
        ".PLANEANGLEUNIT.,$,.RADIAN.",
        ".FORCEUNIT.,$,.NEWTON.",
        ".ELECTRICVOLTAGEUNIT.,$,.VOLT.",
    ]
    .iter()
    .map(|u| f.add("IFCSIUNIT", &format!("*,{u}")))
    .collect();
    let units = f.add("IFCUNITASSIGNMENT", &format!("({})", units.join(",")));
    let g = f.guid();
    let project = f.add(
        "IFCPROJECT",
        &format!(
            "{g},$,{},{},$,$,$,({context}),{units}",
            string(&line.name),
            string(&format!("Overhead line, coordinates {}", line.crs))
        ),
    );
    let site_origin = f.point(origin);
    let site_axes = f.add("IFCAXIS2PLACEMENT3D", &format!("{site_origin},$,$"));
    let site_place = f.add("IFCLOCALPLACEMENT", &format!("$,{site_axes}"));
    let g = f.guid();
    let site = f.add(
        "IFCSITE",
        &format!(
            "{g},$,{},$,$,{site_place},$,$,.ELEMENT.,$,$,$,$,$",
            string(&line.name)
        ),
    );
    let g = f.guid();
    f.add("IFCRELAGGREGATES", &format!("{g},$,$,$,{project},({site})"));
    let mut elements = Vec::new();
    for s in &line.structures {
        let at = f.point([
            s.position[0] - origin[0],
            s.position[1] - origin[1],
            s.position[2] - origin[2],
        ]);
        let (sin, cos) = s.orientation_deg.to_radians().sin_cos();
        let x = f.add("IFCDIRECTION", &format!("({},{},0.)", real(cos), real(sin)));
        let axes = f.add("IFCAXIS2PLACEMENT3D", &format!("{at},{up},{x}"));
        let place = f.add("IFCLOCALPLACEMENT", &format!("{site_place},{axes}"));
        let items: Vec<String> = structure_sticks(s).iter().map(|l| f.polyline(l)).collect();
        let rep = f.add(
            "IFCSHAPEREPRESENTATION",
            &format!("{body},'Body','Curve3D',({})", items.join(",")),
        );
        let shape = f.add("IFCPRODUCTDEFINITIONSHAPE", &format!("$,$,({rep})"));
        let g = f.guid();
        let e = f.add(
            "IFCBUILDINGELEMENTPROXY",
            &format!(
                "{g},$,{},'Structure',{},{place},{shape},{},.NOTDEFINED.",
                string(&s.id),
                s.structure_type.as_deref().map_or("$".to_owned(), string),
                string(&s.id)
            ),
        );
        f.properties(
            &e,
            "OHL_Structure",
            &[
                ("Id", format!("IFCIDENTIFIER({})", string(&s.id))),
                ("Type", opt(s.structure_type.as_deref(), label)),
                ("Extension", opt(s.extension.as_deref(), label)),
                (
                    "OrientationDeg",
                    format!("IFCREAL({})", real(s.orientation_deg)),
                ),
                ("Routes", label(&line.routes_of(&s.id).join(" "))),
            ],
        );
        elements.push(e);
    }
    let identity = f.add("IFCLOCALPLACEMENT", &format!("{site_place},{world}"));
    for case in cases {
        for w in &case.wires {
            if w.points.len() < 2 {
                continue;
            }
            let local: Vec<Point> = w
                .points
                .iter()
                .map(|p| [p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]])
                .collect();
            let poly = f.polyline(&local);
            let rep = f.add(
                "IFCSHAPEREPRESENTATION",
                &format!("{body},'Body','Curve3D',({poly})"),
            );
            let shape = f.add("IFCPRODUCTDEFINITIONSHAPE", &format!("$,$,({rep})"));
            let name = if w.section.starts_with("jumper:") {
                format!("{} ({})", w.section, case.name)
            } else {
                format!("{} span {} ({})", w.section, w.span + 1, case.name)
            };
            let g = f.guid();
            let e = f.add(
                "IFCCABLESEGMENT",
                &format!(
                    "{g},$,{},{},$,{identity},{shape},$,.CONDUCTORSEGMENT.",
                    string(&name),
                    string(&format!("{} {}", w.circuit, w.phase))
                ),
            );
            let cable = line
                .sections
                .iter()
                .find(|s| s.id == w.section)
                .map(|s| s.cable.as_str());
            f.properties(
                &e,
                "OHL_Wire",
                &[
                    ("Case", label(&case.name)),
                    ("Section", label(&w.section)),
                    ("Span", format!("IFCINTEGER({})", w.span + 1)),
                    ("Cable", opt(cable, label)),
                    ("Circuit", label(&w.circuit)),
                    ("Phase", label(&w.phase)),
                    ("System", opt(w.system.as_deref(), label)),
                    (
                        "NominalVoltage",
                        opt(w.voltage, |v| {
                            format!("IFCELECTRICVOLTAGEMEASURE({})", real(v * 1000.0))
                        }),
                    ),
                    (
                        "HorizontalTension",
                        opt(w.horizontal_tension, |v| {
                            format!("IFCFORCEMEASURE({})", real(v))
                        }),
                    ),
                    (
                        "MaxTension",
                        opt(w.max_tension, |v| format!("IFCFORCEMEASURE({})", real(v))),
                    ),
                    (
                        "Sag",
                        opt(w.sag, |v| format!("IFCLENGTHMEASURE({})", real(v))),
                    ),
                ],
            );
            elements.push(e);
        }
    }
    if !elements.is_empty() {
        let g = f.guid();
        f.add(
            "IFCRELCONTAINEDINSPATIALSTRUCTURE",
            &format!("{g},$,$,$,({}),{site}", elements.join(",")),
        );
    }
    let (date, time) = crate::landxml::utc_now();
    let mut out = format!(
        "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('ViewDefinition [ReferenceView_V1.2]'),'2;1');\n\
         FILE_NAME({},'{date}T{time}',(''),(''),'OpenOHL {}','OpenOHL','');\n\
         FILE_SCHEMA(('IFC4'));\nENDSEC;\nDATA;\n",
        string(&line.name),
        env!("CARGO_PKG_VERSION")
    );
    for (k, l) in f.lines.iter().enumerate() {
        let _ = writeln!(out, "#{}={l};", k + 1);
    }
    out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
    Ok(out)
}

/// IFC4: structures and wires for BIM programs.
pub struct Ifc;

impl Exporter for Ifc {
    fn id(&self) -> &str {
        "ifc"
    }
    fn description(&self) -> &str {
        "Structures and wires (with tensions, sag) as IFC4 for BIM programs"
    }
    fn extension(&self) -> &str {
        "ifc"
    }
    fn export(&mut self, model: &ExportModel, options: &Options) -> Result<Vec<u8>, String> {
        document(model, options).map(String::into_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_values() {
        assert_eq!(real(1.0), "1.0");
        assert_eq!(real(1e-7), "1.E-7");
        assert_eq!(real(2.5e20), "2.5E20");
        assert_eq!(string("it's ä\\"), "'it''s \\X2\\00E4\\X0\\\\\\'");
        assert_eq!(guid(0).len(), 22);
        assert_eq!(guid(u128::MAX), "3$$$$$$$$$$$$$$$$$$$$$");
    }
}
