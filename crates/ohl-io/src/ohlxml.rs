// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! OHL XML: the whole line model as XML (namespace [`NAMESPACE`], schema
//! `docs/schema/ohl-line.xsd`), the same content as a line file
//! (`.ohl.json`) — written and read back without loss.
//!
//! Names follow the line file: an object is an element, its plain values
//! are attributes with the JSON key's name, points are `x`, `y`, `z`
//! attributes, lists are repeated child elements. Absent optional values
//! are absent attributes. See `docs/schema/ohl-line.md`.

use std::fmt::Write as _;

use ohl_line::condition::{Calculation, CreepStretch};
use ohl_line::electrical::{System, SystemKind, SystemPhase};
use ohl_line::{
    Alignment, AlignmentPoint, AttachmentV1, Display, InsulatorString, Jumper, JumperClamp,
    JumperSupport, Line, OldPoint, Point, Route, Section, SectionPoint, Stringing, Structure,
    StructurePoint, WirePoint,
};

use crate::xml::{Element, attribute as attr};
use crate::{ExportModel, Exporter, Importer, Options};

/// The XML namespace of OHL XML line documents.
pub const NAMESPACE: &str = "urn:openohl:ohl-line:1";

/// The namespace of OLS XML documents of before 2026-10-18, when OLS XML
/// became OHL XML: read as the current one (as are documents in no
/// namespace).
pub const LEGACY_NAMESPACE: &str = "urn:ols-cad:ols-line:1";

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

/// `<on_route id="…"/>` children: the routes something is on.
fn write_on_routes(out: &mut String, routes: &[String], indent: &str) {
    for r in routes {
        let _ = write!(out, "{indent}<on_route");
        attr(out, "id", r);
        out.push_str("/>\n");
    }
}

/// The routes of `<on_route>` children.
fn read_on_routes(e: &Element) -> Result<Vec<String>, String> {
    e.children_named("on_route")
        .map(|r| {
            check(r, &["id"], &[])?;
            Ok(text(r, "id")?.to_owned())
        })
        .collect()
}

/// A place along a span: `{prefix}fraction`, `{prefix}distance` or
/// `{prefix}station`.
fn write_place(out: &mut String, prefix: &str, p: &ohl_line::SpanPlace) {
    let (key, v) = match *p {
        ohl_line::SpanPlace::Fraction(f) => ("fraction", f),
        ohl_line::SpanPlace::Distance(d) => ("distance", d),
        ohl_line::SpanPlace::Station(s) => ("station", s),
    };
    let _ = write!(out, " {prefix}{key}=\"{}\"", num(v));
}

/// Reads a place along a span (older files may give several: a station
/// over a distance over the fraction).
fn read_place(e: &Element, prefix: &str) -> Result<ohl_line::SpanPlace, String> {
    Ok(ohl_line::SpanPlace::from_parts(
        optional(e, &format!("{prefix}fraction"))?.unwrap_or(0.0),
        optional(e, &format!("{prefix}distance"))?,
        optional(e, &format!("{prefix}station"))?,
    ))
}

fn write_spacer(out: &mut String, s: &ohl_line::Spacer) {
    let ohl_line::Spacer {
        id,
        description,
        a,
        b,
        insulator,
        length,
        weight,
        wind_area,
        stiffness,
        compression,
        fields,
    } = s;
    out.push_str("  <spacer");
    attr(out, "id", id);
    nonempty(out, "description", description);
    opt_text(out, "insulator", insulator.as_deref());
    opt_num(out, "length", *length);
    let _ = write!(
        out,
        " weight=\"{}\" wind_area=\"{}\"",
        num(*weight),
        num(*wind_area)
    );
    opt_num(out, "stiffness", *stiffness);
    opt_num(out, "compression", *compression);
    out.push_str(">\n");
    for end in [a, b] {
        out.push_str("    <end");
        attr(out, "section", &end.section);
        attr(out, "point", &end.point);
        out.push_str("/>\n");
    }
    write_fields(out, fields);
    out.push_str("  </spacer>\n");
}

fn read_spacer(e: &Element) -> Result<ohl_line::Spacer, String> {
    check(
        e,
        &[
            "id",
            "description",
            "insulator",
            "length",
            "weight",
            "wind_area",
            "stiffness",
            "compression",
        ],
        &["end", "field"],
    )?;
    let ends: Vec<ohl_line::SpanPointRef> = e
        .children_named("end")
        .map(|x| {
            check(x, &["section", "point"], &[])?;
            Ok(ohl_line::SpanPointRef {
                section: text(x, "section")?.to_owned(),
                point: text(x, "point")?.to_owned(),
            })
        })
        .collect::<Result<_, String>>()?;
    let [a, b] = <[ohl_line::SpanPointRef; 2]>::try_from(ends)
        .map_err(|_| "<spacer>: two <end>s".to_owned())?;
    Ok(ohl_line::Spacer {
        id: text(e, "id")?.to_owned(),
        description: owned(e, "description").unwrap_or_default(),
        a,
        b,
        insulator: owned(e, "insulator"),
        length: optional(e, "length")?,
        weight: optional(e, "weight")?.unwrap_or(0.0),
        wind_area: optional(e, "wind_area")?.unwrap_or(0.0),
        stiffness: optional(e, "stiffness")?,
        compression: optional(e, "compression")?,
        fields: read_fields(e)?,
    })
}

/// Values of plugin columns: `<field key="plugin.column" value="…"/>`.
fn write_fields(out: &mut String, fields: &ohl_line::Fields) {
    for (key, value) in fields {
        out.push_str("    <field");
        attr(out, "key", key);
        attr(out, "value", value);
        out.push_str("/>\n");
    }
}

/// The `<field>` children.
fn read_fields(e: &Element) -> Result<ohl_line::Fields, String> {
    e.children_named("field")
        .map(|f| {
            check(f, &["key", "value"], &[])?;
            Ok((text(f, "key")?.to_owned(), text(f, "value")?.to_owned()))
        })
        .collect()
}

fn write_alignment(out: &mut String, a: &Alignment) {
    let Alignment {
        id,
        name,
        description,
        route,
        points,
        legacy: _,
        fields,
    } = a;
    out.push_str("  <alignment");
    attr(out, "id", id);
    nonempty(out, "name", name);
    nonempty(out, "description", description);
    attr(out, "route", route);
    if points.is_empty() && fields.is_empty() {
        out.push_str("/>\n");
        return;
    }
    out.push_str(">\n");
    for p in points {
        out.push_str("    <point");
        attr(out, "ref", p);
        out.push_str("/>\n");
    }
    write_fields(out, fields);
    out.push_str("  </alignment>\n");
}

fn write_alignment_point(out: &mut String, p: &AlignmentPoint) {
    let AlignmentPoint {
        id,
        position,
        structure,
        description,
    } = p;
    out.push_str("  <alignment_point");
    attr(out, "id", id);
    point_attrs(out, *position);
    opt_text(out, "structure", structure.as_deref());
    nonempty(out, "description", description);
    out.push_str("/>\n");
}

/// A line's own alignment of an older file, not upgraded (no id).
fn write_old_alignment(out: &mut String, points: &[OldPoint]) {
    out.push_str("  <alignment>\n");
    for p in points {
        out.push_str("    <point");
        nonempty(out, "description", &p.description);
        point_attrs(out, p.position);
        opt_text(out, "structure", p.structure.as_deref());
        out.push_str("/>\n");
    }
    out.push_str("  </alignment>\n");
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
        description,
        height_adjust,
        points,
        strings,
        wires,
        omit,
        omit_sets,
        members,
        routes,
        fields,
    } = s;
    out.push_str("  <structure");
    attr(out, "id", id);
    point_attrs(out, *position);
    attr(out, "orientation_deg", &num(*orientation_deg));
    opt_text(out, "type", structure_type.as_deref());
    opt_text(out, "extension", extension.as_deref());
    if *height_adjust != 0.0 {
        attr(out, "height_adjust", &num(*height_adjust));
    }
    nonempty(out, "description", description);
    if points.is_empty()
        && strings.is_empty()
        && wires.is_empty()
        && omit.is_empty()
        && omit_sets.is_empty()
        && members.is_empty()
        && routes.is_empty()
        && fields.is_empty()
    {
        out.push_str("/>\n");
        return;
    }
    out.push_str(">\n");
    write_on_routes(out, routes, "    ");
    for p in points {
        let StructurePoint {
            description,
            label,
            position,
        } = p;
        out.push_str("    <point");
        attr(out, "label", label);
        nonempty(out, "description", description);
        point_attrs(out, *position);
        out.push_str("/>\n");
    }
    for w in wires {
        let WirePoint {
            description,
            label,
            position,
            set,
            weight,
        } = w;
        out.push_str("    <wire");
        attr(out, "label", label);
        nonempty(out, "description", description);
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
            description,
            label,
            top,
            wire,
            length,
            angle_deg,
            direction_deg,
            insulator,
            weight,
            wind_area,
            stiffness,
            compression,
            strain,
            max_load,
            max_swing_deg,
            max_swing_across_deg,
            max_swing_along_deg,
        } = x;
        out.push_str("    <string");
        attr(out, "label", label);
        nonempty(out, "description", description);
        attr(out, "top", top);
        attr(out, "wire", wire);
        opt_text(out, "insulator", insulator.as_deref());
        for (name, v) in [
            ("length", length),
            ("angle_deg", angle_deg),
            ("direction_deg", direction_deg),
            ("weight", weight),
            ("wind_area", wind_area),
        ] {
            if *v != 0.0 {
                attr(out, name, &num(*v));
            }
        }
        for (name, v) in [
            ("stiffness", stiffness),
            ("compression", compression),
            ("max_load", max_load),
            ("max_swing_deg", max_swing_deg),
            ("max_swing_across_deg", max_swing_across_deg),
            ("max_swing_along_deg", max_swing_along_deg),
        ] {
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
    write_fields(out, fields);
    out.push_str("  </structure>\n");
}

fn write_section(out: &mut String, s: &Section) {
    let Section {
        description,
        id,
        cable,
        circuit_name,
        phase_label,
        system,
        bundle,
        bundle_spacing,
        bundle_rotation_deg,
        points,
        stringing,
        span_points,
        spreads,
        color,
        calculation,
        fields,
    } = s;
    out.push_str("  <section");
    attr(out, "id", id);
    nonempty(out, "description", description);
    attr(out, "cable", cable);
    nonempty(out, "circuit_name", circuit_name);
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
    for p in span_points {
        let ohl_line::SpanPoint {
            label,
            span,
            at,
            description,
            object,
        } = p;
        out.push_str("    <span_point");
        attr(out, "label", label);
        let _ = write!(out, " span=\"{span}\"");
        write_place(out, "", at);
        nonempty(out, "description", description);
        match object {
            None => out.push_str("/>\n"),
            Some(o) => {
                out.push_str(">\n");
                write_object(out, o, "      ");
                out.push_str("    </span_point>\n");
            }
        }
    }
    for s in spreads {
        let ohl_line::SpreadObject {
            label,
            from,
            to,
            type_id,
            load,
            per_subconductor,
        } = s;
        out.push_str("    <spread");
        nonempty(out, "label", label);
        attr(out, "from", from);
        attr(out, "to", to);
        opt_text(out, "type", type_id.as_deref());
        attr(out, "load", &num(*load));
        if *per_subconductor {
            attr(out, "per_subconductor", "true");
        }
        out.push_str("/>\n");
    }
    write_fields(out, fields);
    out.push_str("  </section>\n");
}

/// An object on a point: `<object kind type weight shape …sizes drag
/// clamp_length/>`; a jumper's support clamp holds `<hold structure
/// wire/>` (on a structure's strings) or `<hold x y z length weight/>`
/// (a string from a fixed point).
fn write_object(out: &mut String, o: &ohl_line::LineObject, indent: &str) {
    let ohl_line::LineObject {
        kind,
        type_id,
        weight,
        shape,
        drag,
        clamp_length,
        hold,
    } = o;
    let _ = write!(out, "{indent}<object");
    attr(out, "kind", kind.as_str());
    opt_text(out, "type", type_id.as_deref());
    attr(out, "weight", &num(*weight));
    attr(out, "shape", shape.as_str());
    match *shape {
        ohl_line::Shape::Point => {}
        ohl_line::Shape::Sphere { diameter } => attr(out, "diameter", &num(diameter)),
        ohl_line::Shape::Cylinder {
            diameter,
            length,
            axis,
        } => {
            attr(out, "diameter", &num(diameter));
            attr(out, "length", &num(length));
            attr(out, "axis", &enum_name(&axis));
        }
        ohl_line::Shape::Box {
            along,
            across,
            height,
        } => {
            attr(out, "along", &num(along));
            attr(out, "across", &num(across));
            attr(out, "height", &num(height));
        }
        ohl_line::Shape::Areas {
            wind_area,
            ice_area,
        } => {
            attr(out, "wind_area", &num(wind_area));
            attr(out, "ice_area", &num(ice_area));
        }
    }
    opt_num(out, "drag", *drag);
    opt_num(out, "clamp_length", *clamp_length);
    match hold {
        None => out.push_str("/>\n"),
        Some(h) => {
            out.push_str(">\n");
            let _ = write!(out, "{indent}  <hold");
            match h {
                ohl_line::objects::Hold::Strings { structure, wire } => {
                    opt_text(out, "structure", structure.as_deref());
                    attr(out, "wire", wire);
                }
                ohl_line::objects::Hold::String {
                    point,
                    length,
                    weight,
                } => {
                    point_attrs(out, *point);
                    attr(out, "length", &num(*length));
                    attr(out, "weight", &num(*weight));
                }
            }
            let _ = writeln!(out, "/>\n{indent}</object>");
        }
    }
}

fn read_object(e: &Element) -> Result<ohl_line::LineObject, String> {
    check(
        e,
        &[
            "kind",
            "type",
            "weight",
            "shape",
            "diameter",
            "length",
            "axis",
            "along",
            "across",
            "height",
            "wind_area",
            "ice_area",
            "drag",
            "clamp_length",
        ],
        &["hold"],
    )?;
    let hold = e
        .child("hold")
        .map(|h| {
            check(
                h,
                &["structure", "wire", "x", "y", "z", "length", "weight"],
                &[],
            )?;
            Ok::<_, String>(match owned(h, "wire") {
                Some(wire) => ohl_line::objects::Hold::Strings {
                    structure: owned(h, "structure"),
                    wire,
                },
                None => ohl_line::objects::Hold::String {
                    point: point(h)?,
                    length: number(h, "length")?,
                    weight: optional(h, "weight")?.unwrap_or(0.0),
                },
            })
        })
        .transpose()?;
    fn named<T: serde::de::DeserializeOwned>(name: &str, v: &str) -> Result<T, String> {
        serde_json::from_value(serde_json::Value::String(v.to_owned()))
            .map_err(|_| format!("<object>: {name} \"{v}\" unknown"))
    }
    let shape = match e.attr("shape").unwrap_or("point") {
        "point" => ohl_line::Shape::Point,
        "sphere" => ohl_line::Shape::Sphere {
            diameter: number(e, "diameter")?,
        },
        "cylinder" => ohl_line::Shape::Cylinder {
            diameter: number(e, "diameter")?,
            length: number(e, "length")?,
            axis: match e.attr("axis") {
                None => ohl_line::Axis::Along,
                Some(a) => named("axis", a)?,
            },
        },
        "box" => ohl_line::Shape::Box {
            along: number(e, "along")?,
            across: number(e, "across")?,
            height: number(e, "height")?,
        },
        "areas" => ohl_line::Shape::Areas {
            wind_area: number(e, "wind_area")?,
            ice_area: number(e, "ice_area")?,
        },
        other => return Err(format!("<object>: shape \"{other}\" unknown")),
    };
    Ok(ohl_line::LineObject {
        kind: match e.attr("kind") {
            None => ohl_line::ObjectKind::Body,
            Some(k) => named("kind", k)?,
        },
        type_id: owned(e, "type"),
        weight: optional(e, "weight")?.unwrap_or(0.0),
        shape,
        drag: optional(e, "drag")?,
        clamp_length: optional(e, "clamp_length")?,
        hold,
    })
}

fn write_jumper(out: &mut String, j: &Jumper) {
    let Jumper {
        description,
        id,
        cable,
        from,
        to,
        length,
        sag,
        temperature,
        rigid,
        bending_stiffness,
        stiffness_factor,
        angle_from_deg,
        angle_to_deg,
        points,
        color,
        bundle,
        bundle_spacing,
        bundle_rotation_deg,
        fields,
    } = j;
    out.push_str("  <jumper");
    attr(out, "id", id);
    nonempty(out, "description", description);
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
    // Each end: `<from>` at a wire point, or `<from_tap>` on a tap.
    for (name, end) in [("from", from), ("to", to)] {
        match end {
            ohl_line::JumperEnd::Wire(p) => section_point(out, name, p, "    "),
            ohl_line::JumperEnd::Tap {
                tap: ohl_line::Tap::Span(r),
            } => {
                let _ = write!(out, "    <{name}_tap");
                attr(out, "section", &r.section);
                attr(out, "point", &r.point);
                out.push_str("/>\n");
            }
            ohl_line::JumperEnd::Tap {
                tap: ohl_line::Tap::Jumper { jumper, point },
            } => {
                let _ = write!(out, "    <{name}_tap");
                attr(out, "jumper", jumper);
                attr(out, "point", point);
                out.push_str("/>\n");
            }
        }
    }
    for p in points {
        let ohl_line::JumperPoint {
            label,
            at,
            description,
            object,
        } = p;
        out.push_str("    <jumper_point");
        attr(out, "label", label);
        match *at {
            ohl_line::JumperPlace::Fraction(f) => attr(out, "fraction", &num(f)),
            ohl_line::JumperPlace::Length(l) => attr(out, "length", &num(l)),
            ohl_line::JumperPlace::Distance(d) => attr(out, "distance", &num(d)),
        }
        nonempty(out, "description", description);
        match object {
            None => out.push_str("/>\n"),
            Some(o) => {
                out.push_str(">\n");
                write_object(out, o, "      ");
                out.push_str("    </jumper_point>\n");
            }
        }
    }
    write_fields(out, fields);
    out.push_str("  </jumper>\n");
}

fn write_system(out: &mut String, s: &System) {
    let System {
        description,
        id,
        name,
        kind,
        voltage_kv,
        frequency_hz,
        phases,
        color,
        fields,
    } = s;
    out.push_str("  <system");
    attr(out, "id", id);
    nonempty(out, "description", description);
    nonempty(out, "name", name);
    attr(out, "kind", &enum_name(kind));
    attr(out, "voltage_kv", &num(*voltage_kv));
    opt_num(out, "frequency_hz", *frequency_hz);
    if let Some(c) = color {
        attr(out, "color", c);
    }
    out.push_str(">\n");
    for p in phases {
        let SystemPhase {
            label,
            angle_deg,
            polarity,
            code,
        } = p;
        out.push_str("    <phase");
        attr(out, "label", label);
        opt_num(out, "angle_deg", *angle_deg);
        if let Some(p) = polarity {
            let _ = write!(out, " polarity=\"{p}\"");
        }
        if let Some(c) = code {
            attr(out, "code", c);
        }
        out.push_str("/>\n");
    }
    write_fields(out, fields);
    out.push_str("  </system>\n");
}

/// The line as an OHL XML document.
pub fn to_xml(line: &Line) -> String {
    let Line {
        format,
        version,
        name,
        crs,
        height_datum,
        alignment,
        routes,
        alignments,
        alignment_points,
        structures,
        sections,
        jumpers,
        systems,
        display,
        creep_stretch,
        bimetallic,
        solver,
        spacers,
        defaults,
        circuit_starts,
        structure_loads,
        condition_cases: _,
    } = line;
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<line");
    attr(&mut out, "xmlns", NAMESPACE);
    attr(&mut out, "format", format);
    let _ = write!(out, " version=\"{version}\"");
    attr(&mut out, "name", name);
    nonempty(&mut out, "crs", crs);
    if let Some(h) = height_datum {
        nonempty(&mut out, "height_datum", &h.name);
        if let Some(c) = h.epsg {
            attr(&mut out, "height_datum_epsg", &c.to_string());
        }
    }
    if display.ahead_deg != 0.0 {
        attr(&mut out, "ahead_deg", &num(display.ahead_deg));
    }
    out.push_str(">\n");
    // A line read from an older file and not upgraded: its own alignment
    // as before (no id).
    if !alignment.is_empty() {
        write_old_alignment(&mut out, alignment);
    }
    for r in routes {
        let Route {
            id,
            name,
            description,
            legacy_alignment: _,
            legacy_structures: _,
            fields,
        } = r;
        out.push_str("  <route");
        attr(&mut out, "id", id);
        nonempty(&mut out, "name", name);
        nonempty(&mut out, "description", description);
        if fields.is_empty() {
            out.push_str("/>\n");
        } else {
            out.push_str(">\n");
            write_fields(&mut out, fields);
            out.push_str("  </route>\n");
        }
    }
    for a in alignments {
        write_alignment(&mut out, a);
    }
    for p in alignment_points {
        write_alignment_point(&mut out, p);
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
    for s in spacers {
        write_spacer(&mut out, s);
    }
    for p in circuit_starts {
        section_point(&mut out, "circuit_start", p, "  ");
    }
    let Display {
        phase_colors,
        ahead_deg: _,
    } = display;
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
    let ohl_cables::BimetallicModel {
        behavior,
        max_virtual_compression,
    } = bimetallic;
    if !defaults.is_default() {
        // The values not the program's, `section_key="value"` (SI).
        let program = ohl_line::Defaults::default();
        out.push_str("  <defaults");
        for f in ohl_line::DEFAULT_FIELDS {
            let v = (f.get)(defaults);
            if v != (f.get)(&program) {
                attr(&mut out, &format!("{}_{}", f.section, f.key), &num(v));
            }
        }
        out.push_str("/>\n");
    }
    if !solver.is_default() {
        let ohl_line::SolverOptions { jumper_loads, taps } = solver;
        out.push_str("  <solver");
        attr(&mut out, "jumper_loads", jumper_loads.as_str());
        if !taps.is_default() {
            attr(&mut out, "taps", taps.as_str());
        }
        out.push_str("/>\n");
    }
    if !bimetallic.is_default() {
        out.push_str("  <bimetallic");
        attr(&mut out, "behavior", behavior.as_str());
        if let Some(s) = max_virtual_compression {
            let _ = write!(out, " max_virtual_compression_pa=\"{}\"", s.si());
        }
        out.push_str("/>\n");
    }
    if !structure_loads.is_default() {
        let ohl_line::StructureLoads {
            load_cases,
            weather_cases,
        } = structure_loads;
        out.push_str("  <structure_loads");
        attr(
            &mut out,
            "load_cases",
            if load_cases.is_some() {
                "listed"
            } else {
                "all"
            },
        );
        out.push_str(">\n");
        for c in load_cases.iter().flatten() {
            out.push_str("    <load_case");
            attr(&mut out, "name", c);
            out.push_str("/>\n");
        }
        for c in weather_cases {
            out.push_str("    <weather_case");
            attr(&mut out, "name", c);
            out.push_str("/>\n");
        }
        out.push_str("  </structure_loads>\n");
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

/// The points of an older file's alignment (version 3 and before), in it.
fn read_alignment(e: Option<&Element>) -> Result<Vec<OldPoint>, String> {
    let Some(e) = e else {
        return Ok(Vec::new());
    };
    e.children_named("point")
        .map(|p| {
            check(
                p,
                &["x", "y", "z", "structure", "description", "own_routes"],
                &["on_route"],
            )?;
            let own = matches!(p.attr("own_routes"), Some("true") | Some("1"));
            Ok(OldPoint {
                description: owned(p, "description").unwrap_or_default(),
                position: point(p)?,
                structure: owned(p, "structure"),
                routes: own.then(|| read_on_routes(p)).transpose()?,
            })
        })
        .collect()
}

/// An `<alignment>` with an id: its route and points by `ref`; of version
/// 3, its `<on_route>`s and points with their coordinates.
fn read_alignment_of(a: &Element, id: String) -> Result<Alignment, String> {
    check(
        a,
        &["id", "name", "description", "route"],
        &["point", "on_route", "field"],
    )?;
    let mut out = Alignment::new(&id, &owned(a, "route").unwrap_or_default(), Vec::new());
    out.name = owned(a, "name").unwrap_or_default();
    out.description = owned(a, "description").unwrap_or_default();
    out.fields = read_fields(a)?;
    let refs: Vec<&Element> = a
        .children_named("point")
        .filter(|p| p.attr("ref").is_some())
        .collect();
    if refs.is_empty() && (a.child("point").is_some() || a.child("on_route").is_some()) {
        out.legacy = Some((read_on_routes(a)?, read_alignment(Some(a))?));
        return Ok(out);
    }
    for p in a.children_named("point") {
        check(p, &["ref"], &[])?;
        out.points.push(text(p, "ref")?.to_owned());
    }
    Ok(out)
}

fn read_alignment_point(e: &Element) -> Result<AlignmentPoint, String> {
    check(e, &["id", "x", "y", "z", "structure", "description"], &[])?;
    Ok(AlignmentPoint {
        id: text(e, "id")?.to_owned(),
        position: point(e)?,
        structure: owned(e, "structure"),
        description: owned(e, "description").unwrap_or_default(),
    })
}

/// `<structure_loads load_cases="all|listed">` with its `<load_case
/// name>` and `<weather_case name>`.
fn read_structure_loads(e: &Element) -> Result<ohl_line::StructureLoads, String> {
    check(e, &["load_cases"], &["load_case", "weather_case"])?;
    let names = |tag: &str| {
        e.children_named(tag)
            .map(|c| {
                check(c, &["name"], &[])?;
                Ok(text(c, "name")?.to_owned())
            })
            .collect::<Result<Vec<String>, String>>()
    };
    let load_cases = match owned(e, "load_cases").as_deref() {
        None | Some("all") => None,
        Some("listed") => Some(names("load_case")?),
        Some(other) => {
            return Err(format!(
                "structure_loads load_cases \"{other}\": all or listed"
            ));
        }
    };
    Ok(ohl_line::StructureLoads {
        load_cases,
        weather_cases: names("weather_case")?,
    })
}

fn read_section_point(e: &Element) -> Result<SectionPoint, String> {
    check(e, &["structure", "set", "phase"], &[])?;
    let set = number(e, "set")?;
    // Files written before give a phase too.
    Ok(SectionPoint::new(
        text(e, "structure")?,
        match optional(e, "phase")? {
            Some(phase) => ohl_line::assembly::set_from_v1(set, phase),
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
        &[
            "id",
            "x",
            "y",
            "z",
            "orientation_deg",
            "type",
            "extension",
            "height_adjust",
            "description",
        ],
        &[
            "point",
            "wire",
            "string",
            "attachment",
            "omit",
            "member",
            "on_route",
            "field",
        ],
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
            check(p, &["label", "x", "y", "z", "description"], &[])?;
            Ok(StructurePoint {
                description: owned(p, "description").unwrap_or_default(),
                label: text(p, "label")?.to_owned(),
                position: point(p)?,
            })
        })
        .collect::<Result<_, String>>()?;
    let mut wires: Vec<WirePoint> = e
        .children_named("wire")
        .map(|w| {
            check(
                w,
                &["label", "x", "y", "z", "set", "weight", "description"],
                &[],
            )?;
            Ok(WirePoint {
                description: owned(w, "description").unwrap_or_default(),
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
                    "length",
                    "angle_deg",
                    "direction_deg",
                    "insulator",
                    "weight",
                    "wind_area",
                    "stiffness",
                    "compression",
                    "strain",
                    "description",
                    "max_load",
                    "max_swing_deg",
                    "max_swing_across_deg",
                    "max_swing_along_deg",
                ],
                &[],
            )?;
            Ok(InsulatorString {
                description: owned(x, "description").unwrap_or_default(),
                label: text(x, "label")?.to_owned(),
                top: text(x, "top")?.to_owned(),
                wire: text(x, "wire")?.to_owned(),
                length: optional(x, "length")?.unwrap_or(0.0),
                angle_deg: optional(x, "angle_deg")?.unwrap_or(0.0),
                direction_deg: optional(x, "direction_deg")?.unwrap_or(0.0),
                insulator: owned(x, "insulator"),
                weight: optional(x, "weight")?.unwrap_or(0.0),
                wind_area: optional(x, "wind_area")?.unwrap_or(0.0),
                stiffness: optional(x, "stiffness")?,
                compression: optional(x, "compression")?,
                strain: flag(x, "strain")?,
                max_load: optional(x, "max_load")?,
                max_swing_deg: optional(x, "max_swing_deg")?,
                max_swing_across_deg: optional(x, "max_swing_across_deg")?,
                max_swing_along_deg: optional(x, "max_swing_along_deg")?,
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
        let (p, x, w) = ohl_line::assembly::from_v1(&attachments);
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
                omit_sets.push(ohl_line::assembly::set_from_v1(set, phase));
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
        height_adjust: optional(e, "height_adjust")?.unwrap_or(0.0),
        description: owned(e, "description").unwrap_or_default(),
        points,
        strings,
        wires,
        omit,
        omit_sets,
        members,
        routes: read_on_routes(e)?,
        id,
        fields: read_fields(e)?,
    })
}

fn read_section(e: &Element) -> Result<Section, String> {
    check(
        e,
        &[
            "id",
            "cable",
            "circuit_name",
            "circuit",
            "phase_label",
            "system",
            "bundle",
            "bundle_spacing",
            "bundle_rotation_deg",
            "color",
            "calculation",
            "description",
        ],
        &[
            "point",
            "stringing",
            "load",
            "spread_load",
            "span_point",
            "spread",
            "field",
        ],
    )?;
    let calculation = match e.attr("calculation") {
        None => Calculation::default(),
        Some(c) => serde_json::from_value(serde_json::Value::String(c.to_owned()))
            .map_err(|_| format!("<section>: calculation \"{c}\" unknown"))?,
    };
    let mut section = Section {
        description: owned(e, "description").unwrap_or_default(),
        id: text(e, "id")?.to_owned(),
        cable: text(e, "cable")?.to_owned(),
        // (Files before version 5: `circuit`.)
        circuit_name: owned(e, "circuit_name")
            .or_else(|| owned(e, "circuit"))
            .unwrap_or_default(),
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
        color: owned(e, "color"),
        calculation,
        fields: read_fields(e)?,
        span_points: e
            .children_named("span_point")
            .map(|p| {
                check(
                    p,
                    &[
                        "label",
                        "span",
                        "fraction",
                        "distance",
                        "station",
                        "description",
                    ],
                    &["object"],
                )?;
                Ok(ohl_line::SpanPoint {
                    label: text(p, "label")?.to_owned(),
                    span: number(p, "span")?,
                    at: read_place(p, "")?,
                    description: owned(p, "description").unwrap_or_default(),
                    object: p.child("object").map(read_object).transpose()?,
                })
            })
            .collect::<Result<_, String>>()?,
        spreads: e
            .children_named("spread")
            .map(|s| {
                check(
                    s,
                    &["label", "from", "to", "type", "load", "per_subconductor"],
                    &[],
                )?;
                Ok(ohl_line::SpreadObject {
                    label: owned(s, "label").unwrap_or_default(),
                    from: text(s, "from")?.to_owned(),
                    to: text(s, "to")?.to_owned(),
                    type_id: owned(s, "type"),
                    load: number(s, "load")?,
                    per_subconductor: match s.attr("per_subconductor") {
                        None | Some("false") | Some("0") => false,
                        Some("true") | Some("1") => true,
                        Some(o) => {
                            return Err(format!("per_subconductor=\"{o}\": true or false"));
                        }
                    },
                })
            })
            .collect::<Result<_, String>>()?,
    };
    // Older documents: point and spread loads of their own, read into
    // points with objects.
    let loads: Vec<ohl_line::legacy::SpanLoad> = e
        .children_named("load")
        .map(|l| {
            check(
                l,
                &[
                    "span",
                    "fraction",
                    "distance",
                    "station",
                    "vertical",
                    "label",
                    "diameter",
                    "drag",
                    "wind_area",
                    "ice_area",
                ],
                &[],
            )?;
            Ok(ohl_line::legacy::SpanLoad {
                span: number(l, "span")?,
                fraction: optional(l, "fraction")?.unwrap_or(0.0),
                vertical: number(l, "vertical")?,
                label: owned(l, "label").unwrap_or_default(),
                diameter: optional(l, "diameter")?.unwrap_or(0.0),
                drag: optional(l, "drag")?,
                wind_area: optional(l, "wind_area")?,
                ice_area: optional(l, "ice_area")?,
                distance: optional(l, "distance")?,
                station: optional(l, "station")?,
            })
        })
        .collect::<Result<_, String>>()?;
    for l in &loads {
        section.add_point_load(l);
    }
    let spread: Vec<ohl_line::legacy::SpreadLoad> = e
        .children_named("spread_load")
        .map(|l| {
            check(
                l,
                &[
                    "span",
                    "start_fraction",
                    "start_distance",
                    "start_station",
                    "end_fraction",
                    "end_distance",
                    "end_station",
                    "load",
                    "label",
                ],
                &[],
            )?;
            Ok(ohl_line::legacy::SpreadLoad {
                span: number(l, "span")?,
                start: read_place(l, "start_")?,
                end: read_place(l, "end_")?,
                load: number(l, "load")?,
                label: owned(l, "label").unwrap_or_default(),
            })
        })
        .collect::<Result<_, String>>()?;
    for l in &spread {
        section.add_spread_load(l);
    }
    Ok(section)
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
            "description",
        ],
        &[
            "from",
            "to",
            "from_tap",
            "to_tap",
            "support",
            "clamp",
            "jumper_point",
            "field",
        ],
    )?;
    let tap = |name: &str| -> Result<Option<ohl_line::Tap>, String> {
        let Some(t) = e.child(name) else {
            return Ok(None);
        };
        check(t, &["section", "jumper", "point"], &[])?;
        let point = text(t, "point")?.to_owned();
        Ok(Some(match (owned(t, "section"), owned(t, "jumper")) {
            (Some(section), None) => ohl_line::Tap::Span(ohl_line::SpanPointRef { section, point }),
            (None, Some(jumper)) => ohl_line::Tap::Jumper { jumper, point },
            _ => return Err(format!("<{name}>: give a section or a jumper")),
        }))
    };
    let points = e
        .children_named("jumper_point")
        .map(|p| {
            check(
                p,
                &["label", "fraction", "length", "distance", "description"],
                &["object"],
            )?;
            Ok(ohl_line::JumperPoint {
                label: text(p, "label")?.to_owned(),
                at: ohl_line::JumperPlace::from_parts(
                    optional(p, "fraction")?.unwrap_or(0.0),
                    optional(p, "length")?,
                    optional(p, "distance")?,
                ),
                description: owned(p, "description").unwrap_or_default(),
                object: p.child("object").map(read_object).transpose()?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let supports = e
        .children_named("clamp")
        .map(|c| {
            check(c, &["structure", "wire", "at", "clamp_length"], &[])?;
            Ok(JumperClamp {
                label: String::new(),
                structure: owned(c, "structure"),
                wire: text(c, "wire")?.to_owned(),
                at: number(c, "at")?,
                clamp_length: optional(c, "clamp_length")?
                    .unwrap_or(ohl_line::DEFAULT_CLAMP_LENGTH),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
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
    // A tap, else the wire point (files before version 5 gave a
    // placeholder `<from>` beside a tap).
    let end = |name: &str| -> Result<ohl_line::JumperEnd, String> {
        Ok(match tap(&format!("{name}_tap"))? {
            Some(tap) => ohl_line::JumperEnd::Tap { tap },
            None => ohl_line::JumperEnd::Wire(read_section_point(one(e, name)?)?),
        })
    };
    let mut j = Jumper {
        description: owned(e, "description").unwrap_or_default(),
        id: text(e, "id")?.to_owned(),
        cable: text(e, "cable")?.to_owned(),
        from: end("from")?,
        to: end("to")?,
        length: optional(e, "length")?,
        sag: optional(e, "sag")?,
        temperature: optional(e, "temperature")?.unwrap_or(15.0),
        rigid: match e.attr("rigid") {
            None | Some("false") => false,
            Some("true") => true,
            Some(v) => return Err(format!("jumper rigid=\"{v}\": true or false")),
        },
        bending_stiffness: optional(e, "bending_stiffness")?,
        stiffness_factor: optional(e, "stiffness_factor")?,
        angle_from_deg: optional(e, "angle_from_deg")?,
        angle_to_deg: optional(e, "angle_to_deg")?,
        points,
        color: owned(e, "color"),
        bundle: optional(e, "bundle")?,
        bundle_spacing: optional(e, "bundle_spacing")?,
        bundle_rotation_deg: optional(e, "bundle_rotation_deg")?,
        fields: read_fields(e)?,
    };
    // Supports given on their own before version 5: points holding a
    // held clamp.
    if !supports.is_empty() {
        j.set_supports(supports);
    }
    if support.is_some() {
        j.set_support(support);
    }
    Ok(j)
}

fn read_system(e: &Element) -> Result<System, String> {
    check(
        e,
        &[
            "id",
            "name",
            "kind",
            "voltage_kv",
            "frequency_hz",
            "color",
            "description",
        ],
        &["phase", "field"],
    )?;
    let kind = match e.attr("kind") {
        None => SystemKind::default(),
        Some(k) => serde_json::from_value(serde_json::Value::String(k.to_owned()))
            .map_err(|_| format!("<system>: kind \"{k}\" unknown (ac or dc)"))?,
    };
    Ok(System {
        description: owned(e, "description").unwrap_or_default(),
        id: text(e, "id")?.to_owned(),
        name: owned(e, "name").unwrap_or_default(),
        kind,
        voltage_kv: number(e, "voltage_kv")?,
        frequency_hz: optional(e, "frequency_hz")?,
        color: owned(e, "color"),
        phases: e
            .children_named("phase")
            .map(|p| {
                check(p, &["label", "angle_deg", "polarity", "code"], &[])?;
                Ok(SystemPhase {
                    label: text(p, "label")?.to_owned(),
                    angle_deg: optional(p, "angle_deg")?,
                    polarity: optional(p, "polarity")?,
                    code: owned(p, "code"),
                })
            })
            .collect::<Result<_, String>>()?,
        fields: read_fields(e)?,
    })
}

/// Reads an OHL XML document (the line is not validated here).
pub fn from_xml(text_: &str) -> Result<Line, String> {
    let root = crate::xml::parse(text_)?;
    from_element(&root)
}

/// Reads the line of a parsed OHL XML document.
pub fn from_element(root: &Element) -> Result<Line, String> {
    if root.name != "line" {
        return Err(format!(
            "not an OHL XML line: root element <{}> (expected <line>)",
            root.name
        ));
    }
    check(
        root,
        &[
            "format",
            "version",
            "name",
            "crs",
            "height_datum",
            "height_datum_epsg",
            "ahead_deg",
        ],
        &[
            "alignment",
            "alignment_point",
            "route",
            "structure",
            "system",
            "section",
            "jumper",
            "spacer",
            "defaults",
            "phase_color",
            "creep_stretch",
            "bimetallic",
            "solver",
            "circuit_start",
            "structure_loads",
        ],
    )?;
    // A route; files of version 2: its own alignment and structures
    // inside it (read into alignments with the line).
    let routes = root
        .children_named("route")
        .map(|r| {
            check(
                r,
                &["id", "name", "description"],
                &["alignment", "structure", "field"],
            )?;
            let structures: Vec<String> = r
                .children_named("structure")
                .map(|s| {
                    check(s, &["ref"], &[])?;
                    Ok(text(s, "ref")?.to_owned())
                })
                .collect::<Result<_, String>>()?;
            let legacy = r.child("alignment").is_some() || !structures.is_empty();
            Ok(Route {
                id: text(r, "id")?.to_owned(),
                name: owned(r, "name").unwrap_or_default(),
                description: owned(r, "description").unwrap_or_default(),
                legacy_alignment: read_alignment(r.child("alignment"))?,
                legacy_structures: legacy.then_some(structures),
                fields: read_fields(r)?,
            })
        })
        .collect::<Result<_, String>>()?;
    // Alignments (with an id); one without: the line's own (version 2).
    let mut own = Vec::new();
    let mut alignments = Vec::new();
    for a in root.children_named("alignment") {
        match owned(a, "id").filter(|id| !id.is_empty()) {
            None => {
                check(a, &[], &["point"])?;
                own = read_alignment(Some(a))?;
            }
            Some(id) => alignments.push(read_alignment_of(a, id)?),
        }
    }
    let mut display = Display {
        ahead_deg: optional(root, "ahead_deg")?.unwrap_or(0.0),
        ..Display::default()
    };
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
    let solver = match root.child("solver") {
        None => ohl_line::SolverOptions::default(),
        Some(s) => {
            check(s, &["jumper_loads", "taps"], &[])?;
            let jumper_loads = match owned(s, "jumper_loads") {
                None => ohl_line::JumperLoads::default(),
                Some(t) => ohl_line::JumperLoads::parse(&t)
                    .ok_or_else(|| format!("solver jumper_loads \"{t}\": estimate or pull"))?,
            };
            let taps = match owned(s, "taps") {
                None => ohl_line::TapLoads::default(),
                Some(t) => ohl_line::TapLoads::parse(&t)
                    .ok_or_else(|| format!("solver taps \"{t}\": loop or once"))?,
            };
            ohl_line::SolverOptions { jumper_loads, taps }
        }
    };
    let bimetallic = match root.child("bimetallic") {
        None => ohl_cables::BimetallicModel::default(),
        Some(b) => {
            check(b, &["behavior", "max_virtual_compression_pa"], &[])?;
            let behavior = match owned(b, "behavior").as_deref() {
                None | Some("compression") => ohl_cables::BimetallicBehavior::Compression,
                Some("bird_cage") => ohl_cables::BimetallicBehavior::BirdCage,
                Some(other) => {
                    return Err(format!(
                        "bimetallic behavior \"{other}\": compression or bird_cage"
                    ));
                }
            };
            ohl_cables::BimetallicModel {
                behavior,
                max_virtual_compression: optional::<f64>(b, "max_virtual_compression_pa")?
                    .map(ohl_core::units::Stress::from_pascals),
            }
        }
    };
    // Older versions are read into the current one; newer ones refused.
    // A document without a version is of the first one.
    let version: u32 = optional(root, "version")?.unwrap_or(1);
    ohl_core::files::check_header(
        root.attr("format"),
        Some(version),
        ohl_line::LINE_FILE_FORMAT,
        ohl_line::LINE_FILE_VERSION,
        "OHL XML line",
    )?;
    let mut line = Line {
        // Checked above (the id of before 2026-10-18 is read too); the
        // current one is kept.
        format: ohl_line::LINE_FILE_FORMAT.into(),
        version,
        bimetallic,
        solver,
        name: text(root, "name")?.to_owned(),
        crs: owned(root, "crs").unwrap_or_default(),
        height_datum: {
            let name = owned(root, "height_datum").unwrap_or_default();
            let epsg: Option<u32> = optional(root, "height_datum_epsg")?;
            (!name.is_empty() || epsg.is_some()).then_some(ohl_line::HeightDatum { name, epsg })
        },
        alignment: own,
        routes,
        alignments,
        alignment_points: root
            .children_named("alignment_point")
            .map(read_alignment_point)
            .collect::<Result<_, _>>()?,
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
        structure_loads: match root.child("structure_loads") {
            None => ohl_line::StructureLoads::default(),
            Some(e) => read_structure_loads(e)?,
        },
        systems: root
            .children_named("system")
            .map(read_system)
            .collect::<Result<_, _>>()?,
        display,
        creep_stretch,
        condition_cases: Default::default(),
        spacers: root
            .children_named("spacer")
            .map(read_spacer)
            .collect::<Result<_, _>>()?,
        defaults: match root.child("defaults") {
            None => ohl_line::Defaults::default(),
            Some(d) => {
                let names: Vec<String> = ohl_line::DEFAULT_FIELDS
                    .iter()
                    .map(|f| format!("{}_{}", f.section, f.key))
                    .collect();
                let refs: Vec<&str> = names.iter().map(String::as_str).collect();
                check(d, &refs, &[])?;
                let mut defaults = ohl_line::Defaults::default();
                for (f, name) in ohl_line::DEFAULT_FIELDS.iter().zip(&names) {
                    if let Some(v) = optional(d, name)? {
                        (f.set)(&mut defaults, v);
                    }
                }
                defaults.validate()?;
                defaults
            }
        },
    };
    // Older documents: read into routes, alignments and alignment points.
    line.upgrade_routes();
    line.place_strings().map_err(|e| e.to_string())?;
    line.version = ohl_line::LINE_FILE_VERSION;
    line.read_older();
    Ok(line)
}

/// The OHL XML format (export and import; an import replaces the line).
pub struct OhlXml;

impl Exporter for OhlXml {
    fn id(&self) -> &str {
        "ohl-xml"
    }
    fn description(&self) -> &str {
        "The whole line model as OHL XML (docs/schema/ohl-line.xsd)"
    }
    fn extension(&self) -> &str {
        "xml"
    }
    fn export(&mut self, model: &ExportModel, _: &Options) -> Result<Vec<u8>, String> {
        Ok(to_xml(&model.line).into_bytes())
    }
}

impl Importer for OhlXml {
    fn id(&self) -> &str {
        "ohl-xml"
    }
    fn description(&self) -> &str {
        "A line from OHL XML; LandXML (alignments, CgPoints) is recognised too"
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
                "an XML report of {app}, not an OHL XML line: open it with File › New project \
                 from another program (a project plugin reading {app} reports)"
            ));
        }
        from_element(&root)
    }
}
