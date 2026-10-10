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
    AlignmentPoint, AttachmentV1, Display, InsulatorString, Jumper, JumperSupport, Line, Point,
    Route, Section, SectionPoint, SpanLoad, Stringing, Structure, StructurePoint, WirePoint,
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
    let SectionPoint { structure, set } = p;
    let _ = write!(out, "{indent}<{tag}");
    attr(out, "structure", structure);
    let _ = writeln!(out, " set=\"{set}\"/>");
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
        points,
        strings,
        wires,
        omit,
        omit_sets,
        members,
    } = s;
    out.push_str("  <structure");
    attr(out, "id", id);
    point_attrs(out, *position);
    attr(out, "orientation_deg", &num(*orientation_deg));
    opt_text(out, "type", structure_type.as_deref());
    opt_text(out, "extension", extension.as_deref());
    if points.is_empty()
        && strings.is_empty()
        && wires.is_empty()
        && omit.is_empty()
        && omit_sets.is_empty()
        && members.is_empty()
    {
        out.push_str("/>\n");
        return;
    }
    out.push_str(">\n");
    for p in points {
        let StructurePoint { label, position } = p;
        out.push_str("    <point");
        attr(out, "label", label);
        point_attrs(out, *position);
        out.push_str("/>\n");
    }
    for w in wires {
        let WirePoint {
            label,
            position,
            set,
            weight,
        } = w;
        out.push_str("    <wire");
        attr(out, "label", label);
        point_attrs(out, *position);
        if let Some(set) = set {
            let _ = write!(out, " set=\"{set}\"");
        }
        if *weight != 0.0 {
            attr(out, "weight", &num(*weight));
        }
        out.push_str("/>\n");
    }
    for x in strings {
        let InsulatorString {
            label,
            top,
            wire,
            insulator,
            weight,
            wind_area,
            stiffness,
            compression,
            strain,
        } = x;
        out.push_str("    <string");
        attr(out, "label", label);
        attr(out, "top", top);
        attr(out, "wire", wire);
        opt_text(out, "insulator", insulator.as_deref());
        for (name, v) in [("weight", weight), ("wind_area", wind_area)] {
            if *v != 0.0 {
                attr(out, name, &num(*v));
            }
        }
        for (name, v) in [("stiffness", stiffness), ("compression", compression)] {
            if let Some(v) = v {
                attr(out, name, &num(*v));
            }
        }
        if *strain {
            attr(out, "strain", "true");
        }
        out.push_str("/>\n");
    }
    for label in omit {
        out.push_str("    <omit");
        attr(out, "label", label);
        out.push_str("/>\n");
    }
    for set in omit_sets {
        let _ = writeln!(out, "    <omit set=\"{set}\"/>");
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
            diameter,
            drag,
            wind_area,
            ice_area,
        } = l;
        let _ = write!(
            out,
            "    <load span=\"{span}\" fraction=\"{}\" vertical=\"{}\"",
            num(*fraction),
            num(*vertical)
        );
        nonempty(out, "label", label);
        if *diameter != 0.0 {
            let _ = write!(out, " diameter=\"{}\"", num(*diameter));
        }
        for (name, value) in [
            ("drag", drag),
            ("wind_area", wind_area),
            ("ice_area", ice_area),
        ] {
            if let Some(v) = value {
                let _ = write!(out, " {name}=\"{}\"", num(*v));
            }
        }
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
        rigid,
        support,
        bending_stiffness,
        stiffness_factor,
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
    if *rigid {
        attr(out, "rigid", "true");
    }
    opt_num(out, "bending_stiffness", *bending_stiffness);
    opt_num(out, "stiffness_factor", *stiffness_factor);
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
        circuit_starts,
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
    for p in circuit_starts {
        section_point(&mut out, "circuit_start", p, "  ");
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
    let set = number(e, "set")?;
    // Files written before give a phase too.
    Ok(SectionPoint::new(
        text(e, "structure")?,
        match optional(e, "phase")? {
            Some(phase) => ols_line::assembly::set_from_v1(set, phase),
            None => set,
        },
    ))
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
        &["point", "wire", "string", "attachment", "omit", "member"],
    )?;
    let id = text(e, "id")?.to_owned();
    let flag = |a: &Element, name: &str| -> Result<bool, String> {
        match a.attr(name) {
            None | Some("false") | Some("0") => Ok(false),
            Some("true") | Some("1") => Ok(true),
            Some(o) => Err(format!("{name}=\"{o}\": true or false")),
        }
    };
    let mut points: Vec<StructurePoint> = e
        .children_named("point")
        .map(|p| {
            check(p, &["label", "x", "y", "z"], &[])?;
            Ok(StructurePoint {
                label: text(p, "label")?.to_owned(),
                position: point(p)?,
            })
        })
        .collect::<Result<_, String>>()?;
    let mut wires: Vec<WirePoint> = e
        .children_named("wire")
        .map(|w| {
            check(w, &["label", "x", "y", "z", "set", "weight"], &[])?;
            Ok(WirePoint {
                label: text(w, "label")?.to_owned(),
                position: point(w)?,
                set: optional(w, "set")?,
                weight: optional(w, "weight")?.unwrap_or(0.0),
            })
        })
        .collect::<Result<_, String>>()?;
    let mut strings: Vec<InsulatorString> = e
        .children_named("string")
        .map(|x| {
            check(
                x,
                &[
                    "label",
                    "top",
                    "wire",
                    "insulator",
                    "weight",
                    "wind_area",
                    "stiffness",
                    "compression",
                    "strain",
                ],
                &[],
            )?;
            Ok(InsulatorString {
                label: text(x, "label")?.to_owned(),
                top: text(x, "top")?.to_owned(),
                wire: text(x, "wire")?.to_owned(),
                insulator: owned(x, "insulator"),
                weight: optional(x, "weight")?.unwrap_or(0.0),
                wind_area: optional(x, "wind_area")?.unwrap_or(0.0),
                stiffness: optional(x, "stiffness")?,
                compression: optional(x, "compression")?,
                strain: flag(x, "strain")?,
            })
        })
        .collect::<Result<_, String>>()?;
    // Files written before: attachments.
    let attachments: Vec<AttachmentV1> = e
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
            Ok(AttachmentV1 {
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
                strain_member: flag(a, "strain_member")?,
            })
        })
        .collect::<Result<_, String>>()?;
    if !attachments.is_empty() {
        let (p, x, w) = ols_line::assembly::from_v1(&attachments);
        points.extend(p);
        strings.extend(x);
        wires.extend(w);
    }
    let (mut omit, mut omit_sets) = (Vec::new(), Vec::new());
    for o in e.children_named("omit") {
        check(o, &["label", "set", "phase"], &[])?;
        match (
            owned(o, "label"),
            optional::<u32>(o, "set")?,
            optional(o, "phase")?,
        ) {
            (Some(l), None, None) => omit.push(l),
            (None, Some(set), None) => omit_sets.push(set),
            (None, Some(set), Some(phase)) => {
                omit_sets.push(ols_line::assembly::set_from_v1(set, phase));
            }
            _ => return Err(format!("structure {id}: <omit> takes a label or a set")),
        }
    }
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
        points,
        strings,
        wires,
        omit,
        omit_sets,
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
                check(
                    l,
                    &[
                        "span",
                        "fraction",
                        "vertical",
                        "label",
                        "diameter",
                        "drag",
                        "wind_area",
                        "ice_area",
                    ],
                    &[],
                )?;
                Ok(SpanLoad {
                    span: number(l, "span")?,
                    fraction: number(l, "fraction")?,
                    vertical: number(l, "vertical")?,
                    label: owned(l, "label").unwrap_or_default(),
                    diameter: optional(l, "diameter")?.unwrap_or(0.0),
                    drag: optional(l, "drag")?,
                    wind_area: optional(l, "wind_area")?,
                    ice_area: optional(l, "ice_area")?,
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
            "rigid",
            "bending_stiffness",
            "stiffness_factor",
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
        rigid: match e.attr("rigid") {
            None | Some("false") => false,
            Some("true") => true,
            Some(v) => return Err(format!("jumper rigid=\"{v}\": true or false")),
        },
        support,
        bending_stiffness: optional(e, "bending_stiffness")?,
        stiffness_factor: optional(e, "stiffness_factor")?,
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
            "circuit_start",
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
    // Older versions are read into the current one; newer ones refused.
    let version: u32 = optional(root, "version")?.unwrap_or(ols_line::LINE_FILE_VERSION);
    if version > ols_line::LINE_FILE_VERSION {
        return Err(format!(
            "version {version} is newer than supported ({})",
            ols_line::LINE_FILE_VERSION
        ));
    }
    Ok(Line {
        format: owned(root, "format").unwrap_or_else(|| ols_line::LINE_FILE_FORMAT.into()),
        version: ols_line::LINE_FILE_VERSION,
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
        circuit_starts: root
            .children_named("circuit_start")
            .map(read_section_point)
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
        let text = crate::xml::decode(data)?;
        let root = crate::xml::parse(&text)?;
        if root.name == "LandXML" {
            return crate::landxml::import(&root, base, options);
        }
        if let Some(app) = root.child("creator").and_then(|c| c.attr("application")) {
            return Err(format!(
                "an XML report of {app}, not an OLS XML line: open it with File › New project \
                 from another program (a project plugin reading {app} reports)"
            ));
        }
        from_element(&root)
    }
}
