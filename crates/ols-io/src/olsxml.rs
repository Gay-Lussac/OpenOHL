// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! OLS XML: the whole line model as XML (namespace [`NAMESPACE`], schema
//! `docs/schema/ols-line.xsd`), the same content as a line file
//! (`.ols.json`) — written and read back without loss.
//!
//! Names follow the line file: an object is an element, its plain values
//! are attributes with the JSON key's name, points are `x`, `y`, `z`
//! attributes, lists are repeated child elements. Absent optional values
//! are absent attributes. See `docs/schema/ols-line.md`.

use std::fmt::Write as _;

use ols_line::condition::{Calculation, CreepStretch};
use ols_line::electrical::{System, SystemKind, SystemPhase};
use ols_line::{
    AlignmentPoint, Attachment, Display, Jumper, JumperSupport, Line, Point, Route, Section,
    SectionPoint, SpanLoad, Stringing, Structure,
};

use crate::xml::{Element, attribute as attr, escape};
use crate::{ExportModel, Exporter, Importer, Options};

/// The XML namespace of OLS XML line documents.
pub const NAMESPACE: &str = "urn:ols-cad:ols-line:1";

/// A number as written (shortest text that reads back to the same value).
fn num(x: f64) -> String {
    format!("{x}")
}

fn point_attrs(out: &mut String, p: Point) {
    let _ = write!(
        out,
        " x=\"{}\" y=\"{}\" z=\"{}\"",
        num(p[0]),
        num(p[1]),
        num(p[2])
    );
}

fn opt_num(out: &mut String, name: &str, v: Option<f64>) {
    if let Some(v) = v {
        attr(out, name, &num(v));
    }
}

fn opt_text(out: &mut String, name: &str, v: Option<&str>) {
    if let Some(v) = v {
        attr(out, name, v);
    }
}

fn nonempty(out: &mut String, name: &str, v: &str) {
    if !v.is_empty() {
        attr(out, name, v);
    }
}

/// The serde name of a unit enum value (e.g. `initial-fe`).
fn enum_name<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn write_alignment(out: &mut String, points: &[AlignmentPoint], indent: &str) {
    if points.is_empty() {
        return;
    }
    let _ = writeln!(out, "{indent}<alignment>");
    for p in points {
        let AlignmentPoint {
            position,
            structure,
        } = p;
        let _ = write!(out, "{indent}  <point");
        point_attrs(out, *position);
        opt_text(out, "structure", structure.as_deref());
        out.push_str("/>\n");
    }
    let _ = writeln!(out, "{indent}</alignment>");
}

fn section_point(out: &mut String, tag: &str, p: &SectionPoint, indent: &str) {
    let SectionPoint {
        structure,
        set,
        phase,
    } = p;
    let _ = write!(out, "{indent}<{tag}");
    attr(out, "structure", structure);
    let _ = writeln!(out, " set=\"{set}\" phase=\"{phase}\"/>");
}

fn write_stringing(out: &mut String, s: &Stringing, indent: &str) {
    let _ = write!(out, "{indent}<stringing");
    match s {
        Stringing::Tension {
            horizontal_tension,
            temperature,
        } => {
            let _ = writeln!(
                out,
                " kind=\"tension\" horizontal_tension=\"{}\" temperature=\"{}\"/>",
                num(*horizontal_tension),
                num(*temperature)
            );
        }
        Stringing::Sag {
            sag,
            temperature,
            span,
        } => {
            let _ = write!(
                out,
                " kind=\"sag\" sag=\"{}\" temperature=\"{}\"",
                num(*sag),
                num(*temperature)
            );
            if let Some(k) = span {
                let _ = write!(out, " span=\"{k}\"");
            }
            out.push_str("/>\n");
        }
        Stringing::UnstressedLengths {
            lengths,
            temperature,
            sagged,
        } => {
            let _ = writeln!(
                out,
                " kind=\"unstressed_lengths\" temperature=\"{}\">",
                num(*temperature)
            );
            let list: Vec<String> = lengths.iter().map(|l| num(*l)).collect();
            let _ = writeln!(out, "{indent}  <lengths>{}</lengths>", list.join(" "));
            if let Some(s) = sagged {
                write_stringing(out, s, &format!("{indent}  "));
            }
            let _ = writeln!(out, "{indent}</stringing>");
        }
    }
}

fn write_structure(out: &mut String, s: &Structure) {
    let Structure {
        id,
        position,
        orientation_deg,
        structure_type,
        extension,
        attachments,
        omit,
        members,
    } = s;
    out.push_str("  <structure");
    attr(out, "id", id);
    point_attrs(out, *position);
    attr(out, "orientation_deg", &num(*orientation_deg));
    opt_text(out, "type", structure_type.as_deref());
    opt_text(out, "extension", extension.as_deref());
    if attachments.is_empty() && omit.is_empty() && members.is_empty() {
        out.push_str("/>\n");
        return;
    }
    out.push_str(">\n");
    for a in attachments {
        let Attachment {
            set,
            phase,
            label,
            insulator_point,
            wire_point,
            insulator_weight,
            insulator,
            insulator_wind_area,
            legs,
            top_label,
            bottom_label,
            strain_member,
        } = a;
        let _ = write!(out, "    <attachment set=\"{set}\" phase=\"{phase}\"");
        nonempty(out, "label", label);
        if *insulator_weight != 0.0 {
            attr(out, "insulator_weight", &num(*insulator_weight));
        }
        opt_text(out, "insulator", insulator.as_deref());
        if *insulator_wind_area != 0.0 {
            attr(out, "insulator_wind_area", &num(*insulator_wind_area));
        }
        nonempty(out, "top_label", top_label);
        nonempty(out, "bottom_label", bottom_label);
        if *strain_member {
            attr(out, "strain_member", "true");
        }
        out.push_str(">\n      <insulator_point");
        point_attrs(out, *insulator_point);
        out.push_str("/>\n      <wire_point");
        point_attrs(out, *wire_point);
        out.push_str("/>\n");
        for leg in legs.iter().flatten() {
            out.push_str("      <leg");
            point_attrs(out, *leg);
            out.push_str("/>\n");
        }
        out.push_str("    </attachment>\n");
    }
    for [set, phase] in omit {
        let _ = writeln!(out, "    <omit set=\"{set}\" phase=\"{phase}\"/>");
    }
    for m in members {
        let list: Vec<String> = m.iter().flat_map(|p| p.iter().map(|v| num(*v))).collect();
        let _ = writeln!(out, "    <member>{}</member>", list.join(" "));
    }
    out.push_str("  </structure>\n");
}

fn write_section(out: &mut String, s: &Section) {
    let Section {
        id,
        cable,
        circuit,
        phase_label,
        system,
        bundle,
        bundle_spacing,
        bundle_rotation_deg,
        points,
        stringing,
        loads,
        color,
        calculation,
    } = s;
    out.push_str("  <section");
    attr(out, "id", id);
    attr(out, "cable", cable);
    nonempty(out, "circuit", circuit);
    nonempty(out, "phase_label", phase_label);
    opt_text(out, "system", system.as_deref());
    let _ = write!(out, " bundle=\"{bundle}\"");
    opt_num(out, "bundle_spacing", *bundle_spacing);
    opt_num(out, "bundle_rotation_deg", *bundle_rotation_deg);
    opt_text(out, "color", color.as_deref());
    if !calculation.is_default() {
        attr(out, "calculation", &enum_name(calculation));
    }
    out.push_str(">\n");
    for p in points {
        section_point(out, "point", p, "    ");
    }
    write_stringing(out, stringing, "    ");
    for l in loads {
        let SpanLoad {
            span,
            fraction,
            vertical,
            label,
        } = l;
        let _ = write!(
            out,
            "    <load span=\"{span}\" fraction=\"{}\" vertical=\"{}\"",
            num(*fraction),
            num(*vertical)
        );
        nonempty(out, "label", label);
        out.push_str("/>\n");
    }
    out.push_str("  </section>\n");
}

fn write_jumper(out: &mut String, j: &Jumper) {
    let Jumper {
        id,
        cable,
        from,
        to,
        length,
        sag,
        temperature,
        support,
        bending_stiffness,
        angle_from_deg,
        angle_to_deg,
        color,
        bundle,
        bundle_spacing,
        bundle_rotation_deg,
    } = j;
    out.push_str("  <jumper");
    attr(out, "id", id);
    attr(out, "cable", cable);
    opt_num(out, "length", *length);
    opt_num(out, "sag", *sag);
    attr(out, "temperature", &num(*temperature));
    opt_num(out, "bending_stiffness", *bending_stiffness);
    opt_num(out, "angle_from_deg", *angle_from_deg);
    opt_num(out, "angle_to_deg", *angle_to_deg);
    opt_text(out, "color", color.as_deref());
    if let Some(b) = bundle {
        let _ = write!(out, " bundle=\"{b}\"");
    }
    opt_num(out, "bundle_spacing", *bundle_spacing);
    opt_num(out, "bundle_rotation_deg", *bundle_rotation_deg);
    out.push_str(">\n");
    section_point(out, "from", from, "    ");
    section_point(out, "to", to, "    ");
    if let Some(JumperSupport {
        point,
        length,
        weight,
        fraction,
    }) = support
    {
        let _ = write!(
            out,
            "    <support length=\"{}\" weight=\"{}\" fraction=\"{}\"",
            num(*length),
            num(*weight),
            num(*fraction)
        );
        point_attrs(out, *point);
        out.push_str("/>\n");
    }
    out.push_str("  </jumper>\n");
}

fn write_system(out: &mut String, s: &System) {
    let System {
        id,
        name,
        kind,
        voltage_kv,
        frequency_hz,
        phases,
    } = s;
    out.push_str("  <system");
    attr(out, "id", id);
    nonempty(out, "name", name);
    attr(out, "kind", &enum_name(kind));
    attr(out, "voltage_kv", &num(*voltage_kv));
    opt_num(out, "frequency_hz", *frequency_hz);
    out.push_str(">\n");
    for p in phases {
        let SystemPhase {
            label,
            angle_deg,
            polarity,
        } = p;
        out.push_str("    <phase");
        attr(out, "label", label);
        opt_num(out, "angle_deg", *angle_deg);
        if let Some(p) = polarity {
            let _ = write!(out, " polarity=\"{p}\"");
        }
        out.push_str("/>\n");
    }
    out.push_str("  </system>\n");
}

/// The line as an OLS XML document.
pub fn to_xml(line: &Line) -> String {
    let Line {
        format,
        version,
        name,
        crs,
        alignment,
        routes,
        structures,
        sections,
        jumpers,
        systems,
        display,
        creep_stretch,
        bimetallic,
        condition_cases: _,
    } = line;
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<line");
    attr(&mut out, "xmlns", NAMESPACE);
    attr(&mut out, "format", format);
    let _ = write!(out, " version=\"{version}\"");
    attr(&mut out, "name", name);
    nonempty(&mut out, "crs", crs);
    out.push_str(">\n");
    write_alignment(&mut out, alignment, "  ");
    for r in routes {
        let Route {
            id,
            name,
            alignment,
            structures,
        } = r;
        out.push_str("  <route");
        attr(&mut out, "id", id);
        nonempty(&mut out, "name", name);
        out.push_str(">\n");
        write_alignment(&mut out, alignment, "    ");
        for s in structures {
            let _ = writeln!(out, "    <structure ref=\"{}\"/>", escape(s));
        }
        out.push_str("  </route>\n");
    }
    for s in structures {
        write_structure(&mut out, s);
    }
    for s in systems {
        write_system(&mut out, s);
    }
    for s in sections {
        write_section(&mut out, s);
    }
    for j in jumpers {
        write_jumper(&mut out, j);
    }
    let Display { phase_colors } = display;
    for (label, color) in phase_colors {
        out.push_str("  <phase_color");
        attr(&mut out, "label", label);
        attr(&mut out, "color", color);
        out.push_str("/>\n");
    }
    let CreepStretch {
        creep_case,
        load_cases,
    } = creep_stretch;
    if !creep_stretch.is_empty() {
        out.push_str("  <creep_stretch");
        opt_text(&mut out, "creep_case", creep_case.as_deref());
        out.push_str(">\n");
        for c in load_cases {
            out.push_str("    <load_case");
            attr(&mut out, "name", c);
            out.push_str("/>\n");
        }
        out.push_str("  </creep_stretch>\n");
    }
    let ols_cables::BimetallicModel {
        behavior,
        max_virtual_compression,
    } = bimetallic;
    if !bimetallic.is_default() {
        out.push_str("  <bimetallic");
        attr(&mut out, "behavior", behavior.as_str());
        if let Some(s) = max_virtual_compression {
            let _ = write!(out, " max_virtual_compression_pa=\"{}\"", s.si());
        }
        out.push_str("/>\n");
    }
    out.push_str("</line>\n");
    out
}

// Reading.

/// Refuses attributes and child elements not in the lists.
fn check(e: &Element, attrs: &[&str], children: &[&str]) -> Result<(), String> {
    if let Some((k, _)) = e
        .attributes
        .iter()
        .find(|(k, _)| !attrs.contains(&k.as_str()))
    {
        return Err(format!("<{}>: unknown attribute \"{k}\"", e.name));
    }
    if let Some(c) = e
        .children
        .iter()
        .find(|c| !children.contains(&c.name.as_str()))
    {
        return Err(format!("<{}>: unknown element <{}>", e.name, c.name));
    }
    Ok(())
}

fn text<'a>(e: &'a Element, name: &str) -> Result<&'a str, String> {
    e.attr(name)
        .ok_or_else(|| format!("<{}>: attribute \"{name}\" missing", e.name))
}

fn parse_num<T: std::str::FromStr>(e: &Element, name: &str, v: &str) -> Result<T, String> {
    v.trim()
        .parse()
        .map_err(|_| format!("<{}>: {name}=\"{v}\" is not a number", e.name))
}

fn number<T: std::str::FromStr>(e: &Element, name: &str) -> Result<T, String> {
    parse_num(e, name, text(e, name)?)
}

fn optional<T: std::str::FromStr>(e: &Element, name: &str) -> Result<Option<T>, String> {
    e.attr(name).map(|v| parse_num(e, name, v)).transpose()
}

fn owned(e: &Element, name: &str) -> Option<String> {
    e.attr(name).map(str::to_owned)
}

fn point(e: &Element) -> Result<Point, String> {
    Ok([number(e, "x")?, number(e, "y")?, number(e, "z")?])
}

fn numbers(e: &Element) -> Result<Vec<f64>, String> {
    e.text
        .split_whitespace()
        .map(|v| parse_num(e, "value", v))
        .collect()
}

fn one<'a>(e: &'a Element, name: &str) -> Result<&'a Element, String> {
    e.child(name)
        .ok_or_else(|| format!("<{}>: element <{name}> missing", e.name))
}

fn read_alignment(e: Option<&Element>) -> Result<Vec<AlignmentPoint>, String> {
    let Some(e) = e else {
        return Ok(Vec::new());
    };
    check(e, &[], &["point"])?;
    e.children
        .iter()
        .map(|p| {
            check(p, &["x", "y", "z", "structure"], &[])?;
            Ok(AlignmentPoint {
                position: point(p)?,
                structure: owned(p, "structure"),
            })
        })
        .collect()
}

fn read_section_point(e: &Element) -> Result<SectionPoint, String> {
    check(e, &["structure", "set", "phase"], &[])?;
    Ok(SectionPoint {
        structure: text(e, "structure")?.to_owned(),
        set: number(e, "set")?,
        phase: number(e, "phase")?,
    })
}

fn read_stringing(e: &Element) -> Result<Stringing, String> {
    Ok(match text(e, "kind")? {
        "tension" => {
            check(e, &["kind", "horizontal_tension", "temperature"], &[])?;
            Stringing::Tension {
                horizontal_tension: number(e, "horizontal_tension")?,
                temperature: number(e, "temperature")?,
            }
        }
        "sag" => {
            check(e, &["kind", "sag", "temperature", "span"], &[])?;
            Stringing::Sag {
                sag: number(e, "sag")?,
                temperature: number(e, "temperature")?,
                span: optional(e, "span")?,
            }
        }
        "unstressed_lengths" => {
            check(e, &["kind", "temperature"], &["lengths", "stringing"])?;
            Stringing::UnstressedLengths {
                lengths: numbers(one(e, "lengths")?)?,
                temperature: number(e, "temperature")?,
                sagged: e
                    .child("stringing")
                    .map(read_stringing)
                    .transpose()?
                    .map(Box::new),
            }
        }
        other => return Err(format!("<stringing>: kind \"{other}\" unknown")),
    })
}

fn read_structure(e: &Element) -> Result<Structure, String> {
    check(
        e,
        &["id", "x", "y", "z", "orientation_deg", "type", "extension"],
        &["attachment", "omit", "member"],
    )?;
    let id = text(e, "id")?.to_owned();
    let attachments = e
        .children_named("attachment")
        .map(|a| {
            check(
                a,
                &[
                    "set",
                    "phase",
                    "label",
                    "insulator_weight",
                    "insulator",
                    "insulator_wind_area",
                    "top_label",
                    "bottom_label",
                    "strain_member",
                ],
                &["insulator_point", "wire_point", "leg"],
            )?;
            let legs: Vec<Point> = a
                .children_named("leg")
                .map(point)
                .collect::<Result<_, _>>()?;
            Ok(Attachment {
                set: number(a, "set")?,
                phase: number(a, "phase")?,
                label: owned(a, "label").unwrap_or_default(),
                insulator_point: point(one(a, "insulator_point")?)?,
                wire_point: point(one(a, "wire_point")?)?,
                insulator_weight: optional(a, "insulator_weight")?.unwrap_or(0.0),
                insulator: owned(a, "insulator"),
                insulator_wind_area: optional(a, "insulator_wind_area")?.unwrap_or(0.0),
                legs: match legs.len() {
                    0 => None,
                    2 => Some([legs[0], legs[1]]),
                    n => return Err(format!("structure {id}: {n} legs (a V-string has 2)")),
                },
                top_label: owned(a, "top_label").unwrap_or_default(),
                bottom_label: owned(a, "bottom_label").unwrap_or_default(),
                strain_member: match a.attr("strain_member") {
                    None | Some("false") | Some("0") => false,
                    Some("true") | Some("1") => true,
                    Some(o) => return Err(format!("strain_member=\"{o}\": true or false")),
                },
            })
        })
        .collect::<Result<_, String>>()?;
    let omit = e
        .children_named("omit")
        .map(|o| {
            check(o, &["set", "phase"], &[])?;
            Ok([number(o, "set")?, number(o, "phase")?])
        })
        .collect::<Result<_, String>>()?;
    let members = e
        .children_named("member")
        .map(|m| {
            let v = numbers(m)?;
            if v.len() % 3 != 0 {
                return Err(format!("structure {id}: a member needs x y z triples"));
            }
            Ok(v.chunks(3).map(|c| [c[0], c[1], c[2]]).collect())
        })
        .collect::<Result<_, String>>()?;
    Ok(Structure {
        position: point(e)?,
        orientation_deg: optional(e, "orientation_deg")?.unwrap_or(0.0),
        structure_type: owned(e, "type"),
        extension: owned(e, "extension"),
        attachments,
        omit,
        members,
        id,
    })
}

fn read_section(e: &Element) -> Result<Section, String> {
    check(
        e,
        &[
            "id",
            "cable",
            "circuit",
            "phase_label",
            "system",
            "bundle",
            "bundle_spacing",
            "bundle_rotation_deg",
            "color",
            "calculation",
        ],
        &["point", "stringing", "load"],
    )?;
    let calculation = match e.attr("calculation") {
        None => Calculation::default(),
        Some(c) => serde_json::from_value(serde_json::Value::String(c.to_owned()))
            .map_err(|_| format!("<section>: calculation \"{c}\" unknown"))?,
    };
    Ok(Section {
        id: text(e, "id")?.to_owned(),
        cable: text(e, "cable")?.to_owned(),
        circuit: owned(e, "circuit").unwrap_or_default(),
        phase_label: owned(e, "phase_label").unwrap_or_default(),
        system: owned(e, "system"),
        bundle: optional(e, "bundle")?.unwrap_or(1),
        bundle_spacing: optional(e, "bundle_spacing")?,
        bundle_rotation_deg: optional(e, "bundle_rotation_deg")?,
        points: e
            .children_named("point")
            .map(read_section_point)
            .collect::<Result<_, _>>()?,
        stringing: read_stringing(one(e, "stringing")?)?,
        loads: e
            .children_named("load")
            .map(|l| {
                check(l, &["span", "fraction", "vertical", "label"], &[])?;
                Ok(SpanLoad {
                    span: number(l, "span")?,
                    fraction: number(l, "fraction")?,
                    vertical: number(l, "vertical")?,
                    label: owned(l, "label").unwrap_or_default(),
                })
            })
            .collect::<Result<_, String>>()?,
        color: owned(e, "color"),
        calculation,
    })
}

fn read_jumper(e: &Element) -> Result<Jumper, String> {
    check(
        e,
        &[
            "id",
            "cable",
            "length",
            "sag",
            "temperature",
            "bending_stiffness",
            "angle_from_deg",
            "angle_to_deg",
            "color",
            "bundle",
            "bundle_spacing",
            "bundle_rotation_deg",
        ],
        &["from", "to", "support"],
    )?;
    let support = e
        .child("support")
        .map(|s| {
            check(s, &["length", "weight", "fraction", "x", "y", "z"], &[])?;
            Ok::<_, String>(JumperSupport {
                point: point(s)?,
                length: number(s, "length")?,
                weight: optional(s, "weight")?.unwrap_or(0.0),
                fraction: optional(s, "fraction")?.unwrap_or(0.5),
            })
        })
        .transpose()?;
    Ok(Jumper {
        id: text(e, "id")?.to_owned(),
        cable: text(e, "cable")?.to_owned(),
        from: read_section_point(one(e, "from")?)?,
        to: read_section_point(one(e, "to")?)?,
        length: optional(e, "length")?,
        sag: optional(e, "sag")?,
        temperature: optional(e, "temperature")?.unwrap_or(15.0),
        support,
        bending_stiffness: optional(e, "bending_stiffness")?,
        angle_from_deg: optional(e, "angle_from_deg")?,
        angle_to_deg: optional(e, "angle_to_deg")?,
        color: owned(e, "color"),
        bundle: optional(e, "bundle")?,
        bundle_spacing: optional(e, "bundle_spacing")?,
        bundle_rotation_deg: optional(e, "bundle_rotation_deg")?,
    })
}

fn read_system(e: &Element) -> Result<System, String> {
    check(
        e,
        &["id", "name", "kind", "voltage_kv", "frequency_hz"],
        &["phase"],
    )?;
    let kind = match e.attr("kind") {
        None => SystemKind::default(),
        Some(k) => serde_json::from_value(serde_json::Value::String(k.to_owned()))
            .map_err(|_| format!("<system>: kind \"{k}\" unknown (ac or dc)"))?,
    };
    Ok(System {
        id: text(e, "id")?.to_owned(),
        name: owned(e, "name").unwrap_or_default(),
        kind,
        voltage_kv: number(e, "voltage_kv")?,
        frequency_hz: optional(e, "frequency_hz")?,
        phases: e
            .children_named("phase")
            .map(|p| {
                check(p, &["label", "angle_deg", "polarity"], &[])?;
                Ok(SystemPhase {
                    label: text(p, "label")?.to_owned(),
                    angle_deg: optional(p, "angle_deg")?,
                    polarity: optional(p, "polarity")?,
                })
            })
            .collect::<Result<_, String>>()?,
    })
}

/// Reads an OLS XML document (the line is not validated here).
pub fn from_xml(text_: &str) -> Result<Line, String> {
    let root = crate::xml::parse(text_)?;
    from_element(&root)
}

/// Reads the line of a parsed OLS XML document.
pub fn from_element(root: &Element) -> Result<Line, String> {
    if root.name != "line" {
        return Err(format!(
            "not an OLS XML line: root element <{}> (expected <line>)",
            root.name
        ));
    }
    check(
        root,
        &["format", "version", "name", "crs"],
        &[
            "alignment",
            "route",
            "structure",
            "system",
            "section",
            "jumper",
            "phase_color",
            "creep_stretch",
            "bimetallic",
        ],
    )?;
    let routes = root
        .children_named("route")
        .map(|r| {
            check(r, &["id", "name"], &["alignment", "structure"])?;
            Ok(Route {
                id: text(r, "id")?.to_owned(),
                name: owned(r, "name").unwrap_or_default(),
                alignment: read_alignment(r.child("alignment"))?,
                structures: r
                    .children_named("structure")
                    .map(|s| {
                        check(s, &["ref"], &[])?;
                        Ok(text(s, "ref")?.to_owned())
                    })
                    .collect::<Result<_, String>>()?,
            })
        })
        .collect::<Result<_, String>>()?;
    let mut display = Display::default();
    for c in root.children_named("phase_color") {
        check(c, &["label", "color"], &[])?;
        display
            .phase_colors
            .insert(text(c, "label")?.to_owned(), text(c, "color")?.to_owned());
    }
    let creep_stretch = match root.child("creep_stretch") {
        None => CreepStretch::default(),
        Some(c) => {
            check(c, &["creep_case"], &["load_case"])?;
            CreepStretch {
                creep_case: owned(c, "creep_case"),
                load_cases: c
                    .children_named("load_case")
                    .map(|l| {
                        check(l, &["name"], &[])?;
                        Ok(text(l, "name")?.to_owned())
                    })
                    .collect::<Result<_, String>>()?,
            }
        }
    };
    let bimetallic = match root.child("bimetallic") {
        None => ols_cables::BimetallicModel::default(),
        Some(b) => {
            check(b, &["behavior", "max_virtual_compression_pa"], &[])?;
            let behavior = match owned(b, "behavior").as_deref() {
                None | Some("compression") => ols_cables::BimetallicBehavior::Compression,
                Some("bird_cage") => ols_cables::BimetallicBehavior::BirdCage,
                Some(other) => {
                    return Err(format!(
                        "bimetallic behavior \"{other}\": compression or bird_cage"
                    ));
                }
            };
            ols_cables::BimetallicModel {
                behavior,
                max_virtual_compression: optional::<f64>(b, "max_virtual_compression_pa")?
                    .map(ols_core::units::Stress::from_pascals),
            }
        }
    };
    Ok(Line {
        format: owned(root, "format").unwrap_or_else(|| ols_line::LINE_FILE_FORMAT.into()),
        version: optional(root, "version")?.unwrap_or(ols_line::LINE_FILE_VERSION),
        bimetallic,
        name: text(root, "name")?.to_owned(),
        crs: owned(root, "crs").unwrap_or_default(),
        alignment: read_alignment(root.child("alignment"))?,
        routes,
        structures: root
            .children_named("structure")
            .map(read_structure)
            .collect::<Result<_, _>>()?,
        sections: root
            .children_named("section")
            .map(read_section)
            .collect::<Result<_, _>>()?,
        jumpers: root
            .children_named("jumper")
            .map(read_jumper)
            .collect::<Result<_, _>>()?,
        systems: root
            .children_named("system")
            .map(read_system)
            .collect::<Result<_, _>>()?,
        display,
        creep_stretch,
        condition_cases: Default::default(),
    })
}

/// The OLS XML format (export and import; an import replaces the line).
pub struct OlsXml;

impl Exporter for OlsXml {
    fn id(&self) -> &str {
        "ols-xml"
    }
    fn description(&self) -> &str {
        "The whole line model as OLS XML (docs/schema/ols-line.xsd)"
    }
    fn extension(&self) -> &str {
        "xml"
    }
    fn export(&mut self, model: &ExportModel, _: &Options) -> Result<Vec<u8>, String> {
        Ok(to_xml(&model.line).into_bytes())
    }
}

impl Importer for OlsXml {
    fn id(&self) -> &str {
        "ols-xml"
    }
    fn description(&self) -> &str {
        "A line from OLS XML; LandXML (alignments, CgPoints) is recognised too"
    }
    fn extension(&self) -> &str {
        "xml"
    }
    fn import(
        &mut self,
        data: &[u8],
        base: Option<&Line>,
        options: &Options,
    ) -> Result<Line, String> {
        let text = std::str::from_utf8(data).map_err(|e| format!("not UTF-8 text: {e}"))?;
        let root = crate::xml::parse(text)?;
        if root.name == "LandXML" {
            return crate::landxml::import(&root, base, options);
        }
        from_element(&root)
    }
}
