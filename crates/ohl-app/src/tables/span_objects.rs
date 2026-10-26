// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Spacers and span objects (point and spread loads, markers, jumpers'
//! places): their rows and commands.

use super::*;

/// The spacers, their forces in weather case `case`.
pub(super) fn spacer_rows(model: &Model, case: usize) -> Vec<Row> {
    let end = |e: &ohl_line::SpanPointRef| format!("{} {}", e.section, e.point);
    model
        .line
        .spacers
        .iter()
        .map(|s| {
            let solved = crate::selection::spacer_state(model, case, &s.id);
            Row {
                defaulted: Vec::new(),
                key: s.id.clone(),
                select: Selection::Spacer(s.id.clone()),
                cells: vec![
                    s.id.clone(),
                    end(&s.a),
                    end(&s.b),
                    s.insulator.clone().unwrap_or_default(),
                    s.length.map_or(String::new(), |v| f(v, 3)),
                    f(s.weight, 1),
                    f(s.wind_area, 3),
                    s.stiffness.map_or(String::new(), |v| f(v, 0)),
                    s.compression.map_or(String::new(), |v| f(v, 0)),
                    solved.map_or(String::new(), |x| f(x.force, 0)),
                    solved.map_or(String::new(), |x| {
                        if x.over {
                            "over its compression limit".into()
                        } else if x.slack {
                            "slack".into()
                        } else {
                            "ok".into()
                        }
                    }),
                    s.description.clone(),
                ],
            }
        })
        .collect()
}

/// The spacers table's changes.
pub(super) fn spacer_commands<'a>(
    stored: &ohl_line::Line,
    page: &TablePage,
    existing: &impl Fn(&String) -> Result<Cells<'a>, String>,
    out: &mut Changes,
) -> Result<(), String> {
    for key in &page.deleted {
        out.commands.push((
            format!("deleted spacer {key}"),
            Command::Delete {
                kind: Kind::Spacer,
                id: key.clone(),
                confirm: true,
            },
        ));
    }
    for key in page.edits.keys().filter(|k| !page.deleted.contains(*k)) {
        let c = existing(key)?;
        let mut s = stored
            .spacers
            .iter()
            .find(|s| &s.id == key)
            .ok_or_else(|| format!("no spacer {key}"))?
            .clone();
        let name = format!("spacer {key}");
        s.insulator = opt(&c.get(3));
        s.length = opt_num(&c.get(4), &format!("{name} length"))?;
        s.weight = opt_num(&c.get(5), &format!("{name} weight"))?.unwrap_or(0.0);
        s.wind_area = opt_num(&c.get(6), &format!("{name} wind area"))?.unwrap_or(0.0);
        s.stiffness = opt_num(&c.get(7), &format!("{name} EA"))?;
        s.compression = opt_num(&c.get(8), &format!("{name} compression"))?;
        s.description = c.get(11).trim().to_owned();
        out.commands.push((
            format!("changed {name}"),
            Command::Put {
                kind: Kind::Spacer,
                id: key.clone(),
                value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
                ord: None,
            },
        ));
    }
    Ok(())
}

/// A spread's "load on" cell: on the whole bundle or each subconductor.
pub(super) fn load_on_text(per_subconductor: bool) -> String {
    if per_subconductor { "each" } else { "bundle" }.to_owned()
}

/// A "load on" cell read: `bundle` (or empty) on the whole bundle, `each`
/// on each subconductor.
pub(super) fn parse_load_on(text: &str) -> Result<bool, String> {
    match text.trim().to_lowercase().as_str() {
        "" | "bundle" | "whole bundle" => Ok(false),
        "each" | "each subconductor" | "subconductor" => Ok(true),
        other => Err(format!(
            "load on \"{other}\": bundle (the whole bundle) or each (each subconductor)"
        )),
    }
}

/// Key of a span object: section (or jumper), kind (p: a span point, s:
/// a spread, j: a jumper point) and index.
pub(super) fn span_object_key(on: &str, kind: char, k: usize) -> String {
    format!("{on}\t{kind}\t{k}")
}

/// An object's cells: load (weight), shape, sizes, axis, drag.
pub(super) fn object_cells(o: Option<&ohl_line::LineObject>) -> [String; 7] {
    let Some(o) = o else {
        return Default::default();
    };
    let n = |x: f64| f(x, 3);
    let (s1, s2, s3, axis) = match o.shape {
        ohl_line::Shape::Point => (String::new(), String::new(), String::new(), String::new()),
        ohl_line::Shape::Sphere { diameter } => {
            (n(diameter), String::new(), String::new(), String::new())
        }
        ohl_line::Shape::Cylinder {
            diameter,
            length,
            axis,
        } => (
            n(diameter),
            n(length),
            String::new(),
            match axis {
                ohl_line::Axis::Along => "along",
                ohl_line::Axis::Across => "across",
                ohl_line::Axis::Vertical => "vertical",
            }
            .to_owned(),
        ),
        ohl_line::Shape::Box {
            along,
            across,
            height,
        } => (n(along), n(across), n(height), String::new()),
        ohl_line::Shape::Areas {
            wind_area,
            ice_area,
        } => (n(wind_area), n(ice_area), String::new(), String::new()),
    };
    [
        f(o.weight, 1),
        o.shape.as_str().to_owned(),
        s1,
        s2,
        s3,
        axis,
        o.drag.map_or(String::new(), |d| f(d, 2)),
    ]
}

/// The kind cell of a point.
pub(super) fn point_kind(o: Option<&ohl_line::LineObject>) -> String {
    match o {
        None => "point".into(),
        Some(o) => format!(
            "{} ({})",
            o.kind.as_str(),
            o.type_id.clone().unwrap_or_else(|| "own".into())
        ),
    }
}

/// The points and spreads of the sections, and the jumpers' points.
pub(super) fn span_object_rows(model: &Model) -> Vec<Row> {
    let mut rows = Vec::new();
    let row = |on: &str,
               kind: char,
               k: usize,
               span: String,
               select: Selection,
               head: [String; 4],
               o: [String; 7],
               load_on: String| {
        let mut cells = vec![on.to_owned(), span];
        cells.extend(head);
        cells.extend(o);
        cells.push(load_on);
        Row {
            defaulted: Vec::new(),
            key: span_object_key(on, kind, k),
            select,
            cells,
        }
    };
    for s in &model.line.sections {
        for (k, p) in s.span_points.iter().enumerate() {
            rows.push(row(
                &s.id,
                'p',
                k,
                (p.span + 1).to_string(),
                Selection::SpanPoint {
                    section: s.id.clone(),
                    label: p.label.clone(),
                },
                [
                    point_kind(p.object.as_ref()),
                    p.label.clone(),
                    p.at.text(),
                    String::new(),
                ],
                object_cells(p.object.as_ref()),
                String::new(),
            ));
        }
        for (k, l) in s.spreads.iter().enumerate() {
            let span = s
                .span_points
                .iter()
                .find(|p| p.label == l.from)
                .map_or(0, |p| p.span);
            let mut o: [String; 7] = Default::default();
            o[0] = f(l.load, 2);
            rows.push(row(
                &s.id,
                's',
                k,
                (span + 1).to_string(),
                Selection::Spread {
                    section: s.id.clone(),
                    index: k,
                },
                [
                    format!(
                        "spread ({})",
                        l.type_id.clone().unwrap_or_else(|| "own".into())
                    ),
                    l.label.clone(),
                    l.from.clone(),
                    l.to.clone(),
                ],
                o,
                load_on_text(l.per_subconductor),
            ));
        }
    }
    for j in &model.line.jumpers {
        for (k, p) in j.points.iter().enumerate() {
            rows.push(row(
                &j.id,
                'j',
                k,
                "—".into(),
                Selection::JumperPoint {
                    jumper: j.id.clone(),
                    label: p.label.clone(),
                },
                [
                    point_kind(p.object.as_ref()),
                    p.label.clone(),
                    p.at.text(),
                    String::new(),
                ],
                object_cells(p.object.as_ref()),
                String::new(),
            ));
        }
    }
    rows
}

/// An object as its cells say (columns 6 to 12): the weight, the shape and
/// its sizes, the axis, the drag; changed from its type: its own values.
pub(super) fn object_from_cells(
    o: &mut ohl_line::LineObject,
    c: &Cells,
    name: &str,
) -> Result<(), String> {
    let before = o.clone();
    if let Some(w) = opt_num(&c.get(6), &format!("{name} weight"))? {
        o.weight = w;
    }
    let size = |k: usize, d: f64| -> Result<f64, String> {
        Ok(opt_num(&c.get(k), &format!("{name} size"))?.unwrap_or(d))
    };
    let shape = c.get(7).trim().to_lowercase();
    o.shape = match shape.as_str() {
        "" | "point" => ohl_line::Shape::Point,
        "sphere" => ohl_line::Shape::Sphere {
            diameter: size(8, 0.6)?,
        },
        "cylinder" => ohl_line::Shape::Cylinder {
            diameter: size(8, 0.25)?,
            length: size(9, 0.6)?,
            axis: match c.get(11).trim().to_lowercase().as_str() {
                "" | "along" => ohl_line::Axis::Along,
                "across" => ohl_line::Axis::Across,
                "vertical" => ohl_line::Axis::Vertical,
                other => {
                    return Err(format!(
                        "{name}: axis \"{other}\": along, across or vertical"
                    ));
                }
            },
        },
        "box" => ohl_line::Shape::Box {
            along: size(8, 0.3)?,
            across: size(9, 0.2)?,
            height: size(10, 0.4)?,
        },
        "areas" => ohl_line::Shape::Areas {
            wind_area: size(8, 0.1)?,
            ice_area: size(9, 0.2)?,
        },
        other => {
            return Err(format!(
                "{name}: shape \"{other}\": point, sphere, cylinder, box or areas"
            ));
        }
    };
    o.drag = opt_num(&c.get(12), &format!("{name} drag"))?;
    o.validate().map_err(|e| format!("{name}: {e}"))?;
    if *o != before {
        o.type_id = None;
    }
    Ok(())
}

/// A place on a jumper as written in the table ([`ohl_line::JumperPlace::text`]):
/// `0.25`, `3 m along` or `2.5 m` (from its `from` clamp).
pub(super) fn parse_jumper_place(text: &str) -> Result<ohl_line::JumperPlace, String> {
    let t = text.trim();
    let bad = || {
        format!(
            "\"{text}\": write a fraction (0.25), a length along it (3 m along) or a distance from its from clamp (2.5 m)"
        )
    };
    let number = |s: &str| ohl_core::units::parse_finite(s).map_err(|_| bad());
    if let Some(l) = t.strip_suffix("m along") {
        return Ok(ohl_line::JumperPlace::Length(number(l)?));
    }
    let d = t
        .strip_suffix("m from the clamp")
        .or_else(|| t.strip_suffix('m'));
    if let Some(d) = d {
        return Ok(ohl_line::JumperPlace::Distance(number(d)?));
    }
    let x = number(t)?;
    if !(0.0..=1.0).contains(&x) {
        return Err(bad());
    }
    Ok(ohl_line::JumperPlace::Fraction(x))
}

/// The span objects table's changes: each section (jumper) changed put
/// once, after deleting the spacers clamped and the jumpers hanging on
/// the points deleted.
pub(super) fn span_object_commands<'a>(
    original: &ohl_line::Line,
    page: &TablePage,
    existing: &impl Fn(&String) -> Result<Cells<'a>, String>,
    out: &mut Changes,
) -> Result<(), String> {
    let parse_key = |key: &str| -> Result<(String, char, usize), String> {
        let mut it = key.split('\t');
        match (
            it.next(),
            it.next().and_then(|k| k.chars().next()),
            it.next().and_then(|k| k.parse().ok()),
        ) {
            (Some(s), Some(kind), Some(k)) => Ok((s.to_owned(), kind, k)),
            _ => Err(format!("{key}: not a span object")),
        }
    };
    // Points relabelled first: their spreads, spacers and taps follow
    // ([`ohl_line::Line::relabel_point`]); the other edits on that.
    let mut work = original.clone();
    for key in page.edits.keys().filter(|k| !page.deleted.contains(*k)) {
        let (id, kind, k) = parse_key(key)?;
        let typed = existing(key)?.get(3).trim().to_owned();
        let old = match kind {
            'p' => work
                .sections
                .iter()
                .find(|s| s.id == id)
                .and_then(|s| s.span_points.get(k))
                .map(|p| p.label.clone()),
            'j' => work
                .jumpers
                .iter()
                .find(|j| j.id == id)
                .and_then(|j| j.points.get(k))
                .map(|p| p.label.clone()),
            _ => None,
        };
        if let Some(old) = old.filter(|l| *l != typed) {
            work.relabel_point(kind == 'j', &id, &old, &typed)
                .map_err(|e| format!("{id} {old}: {e}"))?;
        }
    }
    let stored = &work;
    let mut sections: BTreeMap<String, ohl_line::Section> = BTreeMap::new();
    let mut jumpers: BTreeMap<String, ohl_line::Jumper> = BTreeMap::new();
    let mut gone: BTreeMap<String, BTreeSet<(char, usize)>> = BTreeMap::new();
    let take_section =
        |m: &mut BTreeMap<String, ohl_line::Section>, id: &str| -> Result<(), String> {
            if !m.contains_key(id) {
                let s = stored
                    .sections
                    .iter()
                    .find(|s| s.id == id)
                    .ok_or_else(|| format!("no section {id}"))?;
                m.insert(id.to_owned(), s.clone());
            }
            Ok(())
        };
    let take_jumper =
        |m: &mut BTreeMap<String, ohl_line::Jumper>, id: &str| -> Result<(), String> {
            if !m.contains_key(id) {
                let j = stored
                    .jumpers
                    .iter()
                    .find(|j| j.id == id)
                    .ok_or_else(|| format!("no jumper {id}"))?;
                m.insert(id.to_owned(), j.clone());
            }
            Ok(())
        };
    for key in page.edits.keys().filter(|k| !page.deleted.contains(*k)) {
        let (id, kind, k) = parse_key(key)?;
        let c = existing(key)?;
        let name = format!("{id} {}", c.get(3).trim());
        match kind {
            'p' => {
                take_section(&mut sections, &id)?;
                let s = sections.get_mut(&id).expect("taken");
                let p = s
                    .span_points
                    .get_mut(k)
                    .ok_or_else(|| format!("{name}: gone"))?;
                p.at = parse_place(&c.get(4)).map_err(|e| format!("{name}: {e}"))?;
                if let Some(o) = &mut p.object {
                    object_from_cells(o, &c, &name)?;
                }
            }
            'j' => {
                take_jumper(&mut jumpers, &id)?;
                let j = jumpers.get_mut(&id).expect("taken");
                let p = j.points.get_mut(k).ok_or_else(|| format!("{name}: gone"))?;
                p.at = parse_jumper_place(&c.get(4)).map_err(|e| format!("{name}: {e}"))?;
                if let Some(o) = &mut p.object {
                    object_from_cells(o, &c, &name)?;
                }
            }
            _ => {
                take_section(&mut sections, &id)?;
                let s = sections.get_mut(&id).expect("taken");
                let l = s
                    .spreads
                    .get_mut(k)
                    .ok_or_else(|| format!("{name}: gone"))?;
                l.label = c.get(3).trim().to_owned();
                l.from = c.get(4).trim().to_owned();
                l.to = c.get(5).trim().to_owned();
                let load = num(&c.get(6), &format!("{name} load"))?;
                let each = parse_load_on(&c.get(13)).map_err(|e| format!("{name}: {e}"))?;
                if load != l.load || each != l.per_subconductor {
                    l.type_id = None;
                }
                l.load = load;
                l.per_subconductor = each;
            }
        }
    }
    for key in &page.deleted {
        let (id, kind, k) = parse_key(key)?;
        if kind == 'j' {
            take_jumper(&mut jumpers, &id)?;
        } else {
            take_section(&mut sections, &id)?;
        }
        gone.entry(format!("{kind}{id}"))
            .or_default()
            .insert((kind, k));
    }
    let mut hanging: BTreeSet<String> = BTreeSet::new();
    for (tagged, set) in &gone {
        let id = &tagged[1..];
        let keep = |kind: char, k: usize| !set.contains(&(kind, k));
        if tagged.starts_with('j') {
            let j = jumpers.get_mut(id).expect("taken");
            let labels: Vec<String> = j
                .points
                .iter()
                .enumerate()
                .filter(|(k, _)| !keep('j', *k))
                .map(|(_, p)| p.label.clone())
                .collect();
            for x in stored.jumpers.iter().filter(|x| {
                x.taps().any(|t| {
                    matches!(t, ohl_line::Tap::Jumper { jumper, point } if jumper == id && labels.contains(point))
                })
            }) {
                hanging.insert(x.id.clone());
            }
            let mut k = 0;
            j.points.retain(|_| {
                k += 1;
                keep('j', k - 1)
            });
            continue;
        }
        let s = sections.get_mut(id).expect("taken");
        let labels: Vec<String> = s
            .span_points
            .iter()
            .enumerate()
            .filter(|(k, _)| !keep('p', *k))
            .map(|(_, p)| p.label.clone())
            .collect();
        for x in stored.spacers.iter().filter(|x| {
            [&x.a, &x.b]
                .iter()
                .any(|e| e.section == *id && labels.contains(&e.point))
        }) {
            out.takes.push(format!("spacer {}", x.id));
            out.commands.push((
                format!("deleted spacer {}", x.id),
                Command::Delete {
                    kind: Kind::Spacer,
                    id: x.id.clone(),
                    confirm: true,
                },
            ));
        }
        for x in stored.jumpers.iter().filter(|x| {
            x.taps().any(|t| {
                matches!(t, ohl_line::Tap::Span(r) if r.section == *id && labels.contains(&r.point))
            })
        }) {
            hanging.insert(x.id.clone());
        }
        let mut k = 0;
        s.span_points.retain(|_| {
            k += 1;
            keep('p', k - 1)
        });
        // Spreads go with their points.
        k = 0;
        s.spreads.retain(|x| {
            k += 1;
            keep('s', k - 1) && !labels.contains(&x.from) && !labels.contains(&x.to)
        });
    }
    for id in hanging {
        jumpers.remove(&id);
        out.takes.push(format!("jumper {id} (on a tap)"));
        out.commands.push((
            format!("deleted jumper {id}"),
            Command::Delete {
                kind: Kind::Jumper,
                id,
                confirm: true,
            },
        ));
    }
    // What the relabelling changed besides.
    for (a, b) in original.sections.iter().zip(&stored.sections) {
        if a != b && !sections.contains_key(&b.id) {
            sections.insert(b.id.clone(), b.clone());
        }
    }
    for (a, b) in original.jumpers.iter().zip(&stored.jumpers) {
        if a != b && !jumpers.contains_key(&b.id) {
            jumpers.insert(b.id.clone(), b.clone());
        }
    }
    for (a, b) in original.spacers.iter().zip(&stored.spacers) {
        if a != b {
            out.commands.push((
                format!("spacer {} on the relabelled point", b.id),
                Command::Put {
                    kind: Kind::Spacer,
                    id: b.id.clone(),
                    value: serde_json::to_value(b).map_err(|e| e.to_string())?,
                    ord: None,
                },
            ));
        }
    }
    for (id, s) in sections {
        out.commands.push((
            format!("changed the points and spreads of section {id}"),
            Command::Put {
                kind: Kind::Section,
                id,
                value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
                ord: None,
            },
        ));
    }
    for (id, j) in jumpers {
        out.commands.push((
            format!("changed the points of jumper {id}"),
            Command::Put {
                kind: Kind::Jumper,
                id,
                value: serde_json::to_value(&j).map_err(|e| e.to_string())?,
                ord: None,
            },
        ));
    }
    Ok(())
}

/// A place on a span as written in the table ([`ohl_line::SpanPlace::text`]):
/// `0.25`, `120 m` or `st. 1520 m`.
pub(super) fn parse_place(text: &str) -> Result<ohl_line::SpanPlace, String> {
    let t = text.trim();
    let number = |s: &str| {
        ohl_core::units::parse_finite(s).map_err(|_| {
            format!(
                "\"{text}\": write a fraction (0.25), a distance (120 m) or a station (st. 1520 m)"
            )
        })
    };
    if let Some(s) = t.strip_prefix("st.") {
        return Ok(ohl_line::SpanPlace::at_station(number(
            s.trim().trim_end_matches('m'),
        )?));
    }
    if let Some(d) = t.strip_suffix('m') {
        let d = number(d)?;
        if d < 0.0 {
            return Err(format!(
                "\"{text}\": a distance from the back structure is not negative"
            ));
        }
        return Ok(ohl_line::SpanPlace::at_distance(d));
    }
    let x = number(t)?;
    if !(0.0..=1.0).contains(&x) {
        return Err(format!(
            "\"{text}\": a fraction is from 0 to 1 (a distance: add m)"
        ));
    }
    Ok(ohl_line::SpanPlace::at_fraction(x))
}
