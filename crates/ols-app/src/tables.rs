// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Table editing of the line: structures, sections, strings, jumpers and
//! electrical systems, one row per object. Cells are typed like a
//! spreadsheet; changed cells stay pending (yellow) until **Apply**, which
//! makes one session command for all of them (one undo) through the same
//! code as the editor windows. Rows can be filtered and sorted; clicking a
//! row's first cell selects the object in the views.

use std::collections::{BTreeMap, HashMap};

use eframe::egui;
use ols_line::{SectionPoint, Stringing};
use ols_session::{Command, Kind};

use crate::editors::{SectionForm, StringForm, StructureForm};
use crate::model::Model;
use crate::selection::Selection;

/// A table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TableKind {
    /// Structures.
    Structures,
    /// Sections.
    Sections,
    /// Strings (attachment points).
    Strings,
    /// Jumpers.
    Jumpers,
    /// Electrical systems.
    Systems,
    /// Weather cases (of the model's weather case file).
    WeatherCases,
}

/// A column: name, width (points), editable, help.
type Column = (&'static str, f32, bool, &'static str);

const STRUCTURES: &[Column] = &[
    ("id", 120.0, false, "click: select it"),
    (
        "type",
        110.0,
        true,
        "structure type id (empty: strings given)",
    ),
    (
        "extension",
        80.0,
        true,
        "height extension of the type (empty: none)",
    ),
    (
        "orientation °",
        90.0,
        true,
        "cross-arm direction, degrees counter-clockwise from east",
    ),
    ("E", 110.0, true, "base easting, m"),
    ("N", 110.0, true, "base northing, m"),
    ("z", 80.0, true, "base height, m"),
    ("strings", 60.0, false, "attachment points"),
    ("sections", 60.0, false, "sections through it"),
];

const SECTIONS: &[Column] = &[
    ("id", 90.0, false, "click: select it"),
    ("cable", 170.0, true, "cable id from the wire library"),
    ("circuit", 150.0, true, "circuit"),
    ("phase", 50.0, true, "phase label"),
    (
        "system",
        140.0,
        true,
        "electrical system id (empty: from the jumpers, or earthed)",
    ),
    ("bundle", 55.0, true, "subconductors"),
    ("spacing m", 70.0, true, "subconductor spacing"),
    (
        "status",
        95.0,
        false,
        "in the blocks (rolling) or clipped in — change it in the section's window",
    ),
    ("condition", 120.0, false, "what it is sagged to"),
    (
        "value",
        70.0,
        true,
        "tension, kN, or sag, m (not for clipped sections)",
    ),
    (
        "temp °C",
        65.0,
        true,
        "temperature of the condition (or of the lengths)",
    ),
    ("points", 230.0, false, "first → last (number of points)"),
    (
        "H kN",
        65.0,
        false,
        "horizontal tension of the first span, shown weather case",
    ),
    (
        "max sag m",
        75.0,
        false,
        "largest sag of its spans, shown weather case",
    ),
];

const STRINGS: &[Column] = &[
    ("structure", 110.0, false, "click: select the string"),
    ("string", 55.0, false, "set:phase"),
    ("label", 100.0, true, "label"),
    (
        "top across",
        75.0,
        true,
        "insulator point: across the line (right looking ahead), m from the base",
    ),
    (
        "top along",
        75.0,
        true,
        "insulator point: along the line (ahead), m",
    ),
    ("top up", 75.0, true, "insulator point: up from the base, m"),
    ("wire across", 75.0, true, "wire point: across, m"),
    ("wire along", 75.0, true, "wire point: along, m"),
    ("wire up", 75.0, true, "wire point: up, m"),
    ("length m", 65.0, false, "string length"),
    ("weight N", 75.0, true, "insulator weight"),
    (
        "insulator",
        110.0,
        true,
        "insulator assembly id (empty: none)",
    ),
    (
        "strain",
        55.0,
        true,
        "yes: a strain string (free along its conductor)",
    ),
    ("sections", 60.0, false, "sections through it"),
];

const JUMPERS: &[Column] = &[
    ("id", 110.0, false, "click: select it"),
    ("from", 150.0, false, "structure set:phase"),
    ("to", 150.0, false, "structure set:phase"),
    ("cable", 170.0, true, "cable id"),
    ("length m", 75.0, true, "length (or give a sag)"),
    ("sag m", 65.0, true, "sag (or give a length)"),
    ("temp °C", 65.0, true, "temperature of the length or sag"),
    ("bundle", 60.0, true, "subconductors (empty: its section's)"),
    (
        "spacing m",
        70.0,
        true,
        "subconductor spacing (empty: its section's)",
    ),
];

const SYSTEMS: &[Column] = &[
    ("id", 160.0, false, "referenced by the sections"),
    ("name", 200.0, true, "name"),
    ("kind", 50.0, true, "AC or DC"),
    ("kV", 70.0, true, "nominal voltage"),
    ("Hz", 60.0, true, "frequency (AC; empty: none)"),
    ("phases", 160.0, false, "phase labels"),
    ("sections", 70.0, false, "sections given this system"),
];

const WEATHER: &[Column] = &[
    ("#", 35.0, false, "order in the file; click: show this case"),
    ("name", 170.0, true, "unique name"),
    ("description", 200.0, true, "free text"),
    (
        "tags",
        150.0,
        true,
        "comma separated, e.g. clearance, loads",
    ),
    ("temp °C", 70.0, true, "cable temperature"),
    ("ice mm", 65.0, true, "radial ice thickness (0: none)"),
    (
        "ice N/m",
        65.0,
        true,
        "or: ice load per length (empty: none)",
    ),
    (
        "ice kg/m³",
        75.0,
        true,
        "ice density (empty: glaze ice with a thickness, bare diameter with a load)",
    ),
    (
        "wind Pa",
        70.0,
        true,
        "wind pressure on the projected diameter",
    ),
];

/// One row: the object's key, what selecting it selects, the cells.
pub struct Row {
    /// Key of the object (id; structure and set:phase for strings).
    pub key: String,
    /// The object, for the views.
    pub select: Selection,
    /// Cell texts.
    pub cells: Vec<String>,
}

fn f(v: f64, decimals: usize) -> String {
    let t = format!("{v:.decimals$}");
    // No "-0.000".
    if t.starts_with('-') && t[1..].chars().all(|c| c == '0' || c == '.') {
        t[1..].to_owned()
    } else {
        t
    }
}

fn point(p: &SectionPoint) -> String {
    format!("{} {}:{}", p.structure, p.set, p.phase)
}

fn num(text: &str, what: &str) -> Result<f64, String> {
    text.trim()
        .parse::<f64>()
        .map_err(|_| format!("{what}: \"{text}\" is not a number"))
}

fn opt_num(text: &str, what: &str) -> Result<Option<f64>, String> {
    if text.trim().is_empty() {
        Ok(None)
    } else {
        num(text, what).map(Some)
    }
}

fn opt(text: &str) -> Option<String> {
    let t = text.trim();
    (!t.is_empty()).then(|| t.to_owned())
}

impl TableKind {
    /// Every table (the Tables menu).
    pub const ALL: [TableKind; 6] = [
        TableKind::Structures,
        TableKind::Sections,
        TableKind::Strings,
        TableKind::Jumpers,
        TableKind::Systems,
        TableKind::WeatherCases,
    ];

    /// Title of its page.
    pub fn title(self) -> &'static str {
        match self {
            TableKind::Structures => "Structures",
            TableKind::Sections => "Sections",
            TableKind::Strings => "Strings",
            TableKind::Jumpers => "Jumpers",
            TableKind::Systems => "Electrical systems",
            TableKind::WeatherCases => "Weather cases",
        }
    }

    /// Key for saving.
    pub fn key(self) -> &'static str {
        match self {
            TableKind::Structures => "structures",
            TableKind::Sections => "sections",
            TableKind::Strings => "strings",
            TableKind::Jumpers => "jumpers",
            TableKind::Systems => "systems",
            TableKind::WeatherCases => "weather",
        }
    }

    /// Its columns.
    pub fn columns(self) -> &'static [Column] {
        match self {
            TableKind::Structures => STRUCTURES,
            TableKind::Sections => SECTIONS,
            TableKind::Strings => STRINGS,
            TableKind::Jumpers => JUMPERS,
            TableKind::Systems => SYSTEMS,
            TableKind::WeatherCases => WEATHER,
        }
    }

    /// The rows of the model (weather case `case` for results).
    pub fn rows(self, model: &Model, case: usize) -> Vec<Row> {
        let line = &model.line;
        let mut through: HashMap<(&str, u32, u32), usize> = HashMap::new();
        let mut at_structure: HashMap<&str, usize> = HashMap::new();
        for s in &line.sections {
            for p in &s.points {
                *through
                    .entry((p.structure.as_str(), p.set, p.phase))
                    .or_default() += 1;
            }
            let mut ids: Vec<&str> = s.points.iter().map(|p| p.structure.as_str()).collect();
            ids.dedup();
            for id in ids {
                *at_structure.entry(id).or_default() += 1;
            }
        }
        match self {
            TableKind::Structures => line
                .structures
                .iter()
                .map(|s| Row {
                    key: s.id.clone(),
                    select: Selection::Structure(s.id.clone()),
                    cells: vec![
                        s.id.clone(),
                        s.structure_type.clone().unwrap_or_default(),
                        s.extension.clone().unwrap_or_default(),
                        f(s.orientation_deg, 3),
                        f(s.position[0], 3),
                        f(s.position[1], 3),
                        f(s.position[2], 3),
                        s.attachments.len().to_string(),
                        at_structure
                            .get(s.id.as_str())
                            .copied()
                            .unwrap_or(0)
                            .to_string(),
                    ],
                })
                .collect(),
            TableKind::Sections => {
                let shapes: HashMap<&str, &ols_line::solve::SectionShape> = model
                    .cases
                    .get(case)
                    .map(|c| c.shapes.iter().map(|s| (s.section.as_str(), s)).collect())
                    .unwrap_or_default();
                line.sections
                    .iter()
                    .map(|s| {
                        let (condition, value) = match &s.stringing {
                            Stringing::Tension {
                                horizontal_tension, ..
                            } => ("tension".to_owned(), f(horizontal_tension / 1000.0, 2)),
                            Stringing::Sag { sag, span, .. } => (
                                span.map_or("sag, ruling span".into(), |k| {
                                    format!("sag, span {}", k + 1)
                                }),
                                f(*sag, 3),
                            ),
                            Stringing::UnstressedLengths { lengths, .. } => {
                                (format!("{} lengths", lengths.len()), String::new())
                            }
                        };
                        let shape = shapes.get(s.id.as_str());
                        Row {
                            key: s.id.clone(),
                            select: Selection::Span {
                                section: s.id.clone(),
                                span: 0,
                            },
                            cells: vec![
                                s.id.clone(),
                                s.cable.clone(),
                                s.circuit.clone(),
                                s.phase_label.clone(),
                                s.system.clone().unwrap_or_default(),
                                s.bundle.to_string(),
                                s.bundle_spacing.map_or(String::new(), |v| f(v, 3)),
                                if s.stringing.clipped() {
                                    "clipped in".into()
                                } else {
                                    "in the blocks".into()
                                },
                                condition,
                                value,
                                f(s.stringing.temperature(), 1),
                                match (s.points.first(), s.points.last()) {
                                    (Some(a), Some(b)) => {
                                        format!("{} → {} ({})", point(a), point(b), s.points.len())
                                    }
                                    _ => String::new(),
                                },
                                shape
                                    .and_then(|x| x.spans.first())
                                    .map_or(String::new(), |sp| {
                                        f(sp.horizontal_tension / 1000.0, 2)
                                    }),
                                shape.map_or(String::new(), |x| {
                                    f(x.spans.iter().map(|sp| sp.sag).fold(0.0, f64::max), 2)
                                }),
                            ],
                        }
                    })
                    .collect()
            }
            TableKind::Strings => line
                .structures
                .iter()
                .flat_map(|s| {
                    let through = &through;
                    s.attachments.iter().filter_map(move |a| {
                        let form = StringForm::of(model, &s.id, a.set, a.phase)?;
                        Some(Row {
                            key: format!("{}\t{}:{}", s.id, a.set, a.phase),
                            select: Selection::Attachment {
                                structure: s.id.clone(),
                                set: a.set,
                                phase: a.phase,
                            },
                            cells: vec![
                                s.id.clone(),
                                format!("{}:{}", a.set, a.phase),
                                form.label.clone(),
                                f(form.top[0], 3),
                                f(form.top[1], 3),
                                f(form.top[2], 3),
                                f(form.bottom[0], 3),
                                f(form.bottom[1], 3),
                                f(form.bottom[2], 3),
                                f(form.length(), 3),
                                f(form.weight, 1),
                                form.insulator.clone().unwrap_or_default(),
                                if form.strain { "yes" } else { "no" }.into(),
                                through
                                    .get(&(s.id.as_str(), a.set, a.phase))
                                    .copied()
                                    .unwrap_or(0)
                                    .to_string(),
                            ],
                        })
                    })
                })
                .collect(),
            TableKind::Jumpers => line
                .jumpers
                .iter()
                .map(|j| Row {
                    key: j.id.clone(),
                    select: Selection::Jumper(j.id.clone()),
                    cells: vec![
                        j.id.clone(),
                        point(&j.from),
                        point(&j.to),
                        j.cable.clone(),
                        j.length.map_or(String::new(), |v| f(v, 3)),
                        j.sag.map_or(String::new(), |v| f(v, 3)),
                        f(j.temperature, 1),
                        j.bundle.map_or(String::new(), |v| v.to_string()),
                        j.bundle_spacing.map_or(String::new(), |v| f(v, 3)),
                    ],
                })
                .collect(),
            TableKind::Systems => line
                .systems
                .iter()
                .map(|s| Row {
                    key: s.id.clone(),
                    select: Selection::None,
                    cells: vec![
                        s.id.clone(),
                        s.name.clone(),
                        match s.kind {
                            ols_line::electrical::SystemKind::Ac => "AC".into(),
                            ols_line::electrical::SystemKind::Dc => "DC".into(),
                        },
                        f(s.voltage_kv, 1),
                        s.frequency_hz.map_or(String::new(), |v| f(v, 1)),
                        s.phases
                            .iter()
                            .map(|p| p.label.clone())
                            .collect::<Vec<_>>()
                            .join(" "),
                        line.sections
                            .iter()
                            .filter(|x| x.system.as_deref() == Some(s.id.as_str()))
                            .count()
                            .to_string(),
                    ],
                })
                .collect(),
            TableKind::WeatherCases => model
                .cases
                .iter()
                .enumerate()
                .map(|(k, c)| {
                    let w = &c.case;
                    Row {
                        key: k.to_string(),
                        select: Selection::None,
                        cells: vec![
                            (k + 1).to_string(),
                            w.name.clone(),
                            w.description.clone(),
                            w.tags.join(", "),
                            f(w.temperature.celsius(), 1),
                            f(w.ice_thickness.si() * 1000.0, 1),
                            w.ice_load.map_or(String::new(), |v| f(v.si(), 2)),
                            w.ice_density.map_or(String::new(), |v| f(v.si(), 0)),
                            f(w.wind_pressure.si(), 0),
                        ],
                    }
                })
                .collect(),
        }
    }

    /// Weather cases: the model's case file with the `edits` written in
    /// (comments and layout kept), checked; its path and new text.
    pub fn weather_file(
        model: &Model,
        rows: &[Row],
        edits: &BTreeMap<String, BTreeMap<usize, String>>,
    ) -> Result<(std::path::PathBuf, String), String> {
        let path = model
            .sources
            .weather
            .clone()
            .ok_or_else(|| "the model has no weather case file".to_owned())?;
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut doc: toml_edit::DocumentMut = text
            .parse()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let cases = doc
            .get_mut("case")
            .and_then(toml_edit::Item::as_array_of_tables_mut)
            .ok_or_else(|| format!("{}: no [[case]]", path.display()))?;
        for (key, cells) in edits {
            let row = rows
                .iter()
                .find(|r| &r.key == key)
                .ok_or_else(|| format!("case {key} is no longer in the file"))?;
            let k: usize = key.parse().map_err(|_| "bad row".to_owned())?;
            let name = &row.cells[1];
            let t = cases
                .get_mut(k)
                .ok_or_else(|| format!("case {name}: not in {}", path.display()))?;
            for (&c, text) in cells {
                let v = text.trim();
                let number = |unit: &str| -> Result<Option<toml_edit::Item>, String> {
                    if v.is_empty() {
                        return Ok(None);
                    }
                    let x = num(v, &format!("{name} {}", WEATHER[c].0))?;
                    Ok(Some(toml_edit::value(format!("{x} {unit}"))))
                };
                let (field, item) = match c {
                    1 if v.is_empty() => return Err("a case needs a name".into()),
                    1 => ("name", Some(toml_edit::value(v))),
                    2 => ("description", (!v.is_empty()).then(|| toml_edit::value(v))),
                    3 => {
                        let mut a = toml_edit::Array::new();
                        for tag in v.split(',').map(str::trim).filter(|t| !t.is_empty()) {
                            a.push(tag);
                        }
                        ("tags", (!a.is_empty()).then(|| toml_edit::value(a)))
                    }
                    4 if v.is_empty() => return Err(format!("{name}: a temperature is needed")),
                    4 => ("temperature", number("°C")?),
                    5 => ("ice_thickness", number("mm")?),
                    6 => ("ice_load", number("N/m")?),
                    7 => ("ice_density", number("kg/m³")?),
                    8 => ("wind_pressure", number("Pa")?),
                    _ => continue,
                };
                match item {
                    Some(i) => {
                        t.insert(field, i);
                    }
                    None => {
                        t.remove(field);
                    }
                }
            }
        }
        let text = doc.to_string();
        ols_weather::WeatherCases::parse(&text)?;
        Ok((path, text))
    }

    /// The commands making the `edits` (row key → column → text), with
    /// what they do in words; `rows` gives the other cells.
    pub fn commands(
        self,
        model: &Model,
        rows: &[Row],
        edits: &BTreeMap<String, BTreeMap<usize, String>>,
    ) -> Result<Vec<(String, Command)>, String> {
        let stored = model
            .stored_line()
            .ok_or_else(|| "read-only: open a project to edit".to_owned())?;
        let cell = |key: &str, row: &Row, c: usize| -> String {
            edits
                .get(key)
                .and_then(|e| e.get(&c))
                .cloned()
                .unwrap_or_else(|| row.cells[c].clone())
        };
        let row_of = |key: &str| {
            rows.iter()
                .find(|r| r.key == key)
                .ok_or_else(|| format!("{key} is no longer in the line"))
        };
        let mut out = Vec::new();
        match self {
            TableKind::Structures => {
                for key in edits.keys() {
                    let row = row_of(key)?;
                    let c = |i| cell(key, row, i);
                    let mut form = StructureForm::of(model, key)
                        .ok_or_else(|| format!("no structure {key}"))?;
                    form.structure_type = opt(&c(1));
                    form.extension = opt(&c(2));
                    form.orientation_deg = num(&c(3), &format!("{key} orientation"))?;
                    for k in 0..3 {
                        form.position[k] = num(&c(4 + k), &format!("{key} position"))?;
                    }
                    out.push(form.command(Some(stored))?);
                }
            }
            TableKind::Sections => {
                for key in edits.keys() {
                    let row = row_of(key)?;
                    let c = |i| cell(key, row, i);
                    let mut form =
                        SectionForm::of(model, key).ok_or_else(|| format!("no section {key}"))?;
                    form.cable = c(1).trim().to_owned();
                    form.circuit = c(2);
                    form.phase_label = c(3);
                    form.system = opt(&c(4));
                    form.bundle = c(5)
                        .trim()
                        .parse()
                        .map_err(|_| format!("{key} bundle: \"{}\" is not a number", c(5)))?;
                    if let Some(s) = opt_num(&c(6), &format!("{key} spacing"))? {
                        form.spacing = s;
                    }
                    let temperature = num(&c(10), &format!("{key} temperature"))?;
                    let value = c(9);
                    match &mut form.stringing {
                        Stringing::Tension {
                            horizontal_tension,
                            temperature: t,
                        } => {
                            *horizontal_tension = num(&value, &format!("{key} tension"))? * 1000.0;
                            *t = temperature;
                        }
                        Stringing::Sag {
                            sag,
                            temperature: t,
                            ..
                        } => {
                            *sag = num(&value, &format!("{key} sag"))?;
                            *t = temperature;
                        }
                        Stringing::UnstressedLengths { temperature: t, .. } => {
                            if !value.trim().is_empty() {
                                return Err(format!(
                                    "{key} is clipped in: no tension or sag (change its status in its window)"
                                ));
                            }
                            *t = temperature;
                        }
                    }
                    if model.library.get(&form.cable).is_none() {
                        return Err(format!(
                            "{key}: cable {} is not in the wire library",
                            form.cable
                        ));
                    }
                    out.push(form.command(Some(stored))?);
                }
            }
            TableKind::Strings => {
                // One put per structure, with all its strings changed.
                let mut by_structure: BTreeMap<&str, Vec<&String>> = BTreeMap::new();
                for key in edits.keys() {
                    let (s, _) = key.split_once('\t').unwrap_or((key, ""));
                    by_structure.entry(s).or_default().push(key);
                }
                for (structure, keys) in by_structure {
                    let mut s = stored
                        .structure(structure)
                        .ok_or_else(|| format!("no structure {structure}"))?
                        .clone();
                    for key in keys {
                        let row = row_of(key)?;
                        let c = |i| cell(key, row, i);
                        let (set, phase) = row.cells[1]
                            .split_once(':')
                            .and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))
                            .ok_or_else(|| format!("{key}: bad string number"))?;
                        let name = format!("{structure} {set}:{phase}");
                        let mut form = StringForm::of(model, structure, set, phase)
                            .ok_or_else(|| format!("no string {name}"))?;
                        form.label = c(2);
                        for k in 0..3 {
                            form.top[k] = num(&c(3 + k), &format!("{name} insulator point"))?;
                            form.bottom[k] = num(&c(6 + k), &format!("{name} wire point"))?;
                        }
                        form.weight = num(&c(10), &format!("{name} weight"))?;
                        form.insulator = opt(&c(11));
                        form.strain = match c(12).trim().to_lowercase().as_str() {
                            "yes" | "true" | "1" => true,
                            "no" | "false" | "0" | "" => false,
                            other => {
                                return Err(format!("{name} strain: \"{other}\" is not yes or no"));
                            }
                        };
                        form.apply_to(&mut s);
                    }
                    out.push((
                        format!("changed strings of {structure}"),
                        Command::Put {
                            kind: Kind::Structure,
                            id: structure.to_owned(),
                            value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
                            ord: None,
                        },
                    ));
                }
            }
            TableKind::Jumpers => {
                for key in edits.keys() {
                    let row = row_of(key)?;
                    let c = |i| cell(key, row, i);
                    let mut j = stored
                        .jumpers
                        .iter()
                        .find(|j| &j.id == key)
                        .ok_or_else(|| format!("no jumper {key}"))?
                        .clone();
                    j.cable = c(3).trim().to_owned();
                    j.length = opt_num(&c(4), &format!("{key} length"))?;
                    j.sag = opt_num(&c(5), &format!("{key} sag"))?;
                    j.temperature = num(&c(6), &format!("{key} temperature"))?;
                    j.bundle = opt_num(&c(7), &format!("{key} bundle"))?.map(|v| v as u32);
                    j.bundle_spacing = opt_num(&c(8), &format!("{key} spacing"))?;
                    out.push((
                        format!("changed jumper {key}"),
                        Command::Put {
                            kind: Kind::Jumper,
                            id: key.clone(),
                            value: serde_json::to_value(&j).map_err(|e| e.to_string())?,
                            ord: None,
                        },
                    ));
                }
            }
            TableKind::WeatherCases => {
                return Err("weather cases are written to their file".into());
            }
            TableKind::Systems => {
                let mut systems = stored.systems.clone();
                for key in edits.keys() {
                    let row = row_of(key)?;
                    let c = |i| cell(key, row, i);
                    let s = systems
                        .iter_mut()
                        .find(|s| &s.id == key)
                        .ok_or_else(|| format!("no system {key}"))?;
                    s.name = c(1);
                    s.kind = match c(2).trim().to_uppercase().as_str() {
                        "AC" => ols_line::electrical::SystemKind::Ac,
                        "DC" => ols_line::electrical::SystemKind::Dc,
                        other => return Err(format!("{key} kind: \"{other}\" is not AC or DC")),
                    };
                    s.voltage_kv = num(&c(3), &format!("{key} voltage"))?;
                    s.frequency_hz = opt_num(&c(4), &format!("{key} frequency"))?;
                }
                // The systems are part of the line settings.
                out.push((
                    "changed electrical systems".into(),
                    Command::Put {
                        kind: Kind::Meta,
                        id: "line".into(),
                        value: serde_json::json!({
                            "name": stored.name,
                            "crs": stored.crs,
                            "alignment": stored.alignment,
                            "display": stored.display,
                            "systems": systems,
                        }),
                        ord: None,
                    },
                ));
            }
        }
        Ok(out)
    }
}

/// An open table page: pending edits, filter, sort.
#[derive(Default)]
pub struct TablePage {
    /// Pending cell texts: row key → column → text.
    pub edits: BTreeMap<String, BTreeMap<usize, String>>,
    /// Rows shown contain this (any cell, any case).
    pub filter: String,
    /// Sorted by this column (ascending when true).
    pub sort: Option<(usize, bool)>,
    /// Why Apply failed, or what it did.
    pub message: Option<(String, bool)>,
}

/// What a table asks of the application.
pub enum TableAction {
    /// Apply these commands as one.
    Apply(Vec<(String, Command)>),
    /// Write this file (weather cases) and read the libraries again.
    Write(std::path::PathBuf, String),
    /// Show weather case `k`.
    ShowCase(usize),
    /// Select this object.
    Select(Selection),
}

impl TablePage {
    /// Number of cells changed.
    pub fn pending(&self) -> usize {
        self.edits.values().map(BTreeMap::len).sum()
    }

    /// The page.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        kind: TableKind,
        model: &Model,
        case: usize,
        selection: &Selection,
    ) -> Option<TableAction> {
        // Weather cases are a file of the model, not part of the project.
        let editable = match kind {
            TableKind::WeatherCases => model.sources.weather.is_some(),
            _ => model.can_edit().is_ok(),
        };
        let columns = kind.columns();
        let rows = kind.rows(model, case);
        let mut action = None;
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("filter")
                    .desired_width(160.0),
            );
            ui.separator();
            let n = self.pending();
            if ui
                .add_enabled(
                    editable && n > 0,
                    egui::Button::new(format!("Apply {n} changes")),
                )
                .on_hover_text("One command for all of them (one undo)")
                .clicked()
            {
                let done = match kind {
                    TableKind::WeatherCases => TableKind::weather_file(model, &rows, &self.edits)
                        .map(|(p, t)| TableAction::Write(p, t)),
                    _ => kind
                        .commands(model, &rows, &self.edits)
                        .map(TableAction::Apply),
                };
                match done {
                    Ok(a) => action = Some(a),
                    Err(e) => self.message = Some((e, true)),
                }
            }
            if ui.add_enabled(n > 0, egui::Button::new("Revert")).clicked() {
                self.edits.clear();
                self.message = None;
            }
            ui.label(
                egui::RichText::new(format!("{} rows", rows.len()))
                    .small()
                    .weak(),
            );
            if !editable {
                ui.label(egui::RichText::new("read-only").small().weak());
            }
        });
        if kind == TableKind::WeatherCases {
            ui.label(
                egui::RichText::new(match &model.sources.weather {
                    Some(p) => format!(
                        "{} — Apply writes the file (comments kept) and solves the model again; add or remove cases in Libraries › Weather cases",
                        p.display()
                    ),
                    None => "The model uses a default case (15 °C): make a case file in Libraries › Weather cases and choose Use for the model".into(),
                })
                .small()
                .weak(),
            );
        }
        if let Some((m, error)) = &self.message {
            let t = egui::RichText::new(m).small();
            if *error {
                ui.colored_label(egui::Color32::LIGHT_RED, t);
            } else {
                ui.colored_label(egui::Color32::LIGHT_GREEN, t);
            }
        }
        // Filter and sort (on the cells as shown, edits included).
        let shown_cell = |r: &Row, c: usize| -> String {
            self.edits
                .get(&r.key)
                .and_then(|e| e.get(&c))
                .cloned()
                .unwrap_or_else(|| r.cells[c].clone())
        };
        let needle = self.filter.trim().to_lowercase();
        let mut order: Vec<usize> = (0..rows.len())
            .filter(|&i| {
                needle.is_empty()
                    || rows[i]
                        .cells
                        .iter()
                        .any(|c| c.to_lowercase().contains(&needle))
            })
            .collect();
        if let Some((c, up)) = self.sort {
            order.sort_by(|&a, &b| {
                let (x, y) = (shown_cell(&rows[a], c), shown_cell(&rows[b], c));
                let o = match (x.trim().parse::<f64>(), y.trim().parse::<f64>()) {
                    (Ok(p), Ok(q)) => p.total_cmp(&q),
                    _ => x.cmp(&y),
                };
                if up { o } else { o.reverse() }
            });
        }
        let row_height = 20.0;
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (c, col) in columns.iter().enumerate() {
                    let arrow = match self.sort {
                        Some((s, true)) if s == c => " ▲",
                        Some((s, false)) if s == c => " ▼",
                        _ => "",
                    };
                    let text = egui::RichText::new(format!("{}{arrow}", col.0)).strong();
                    if ui
                        .add_sized([col.1, row_height], egui::Button::new(text).frame(false))
                        .on_hover_text(format!("{} — click to sort", col.3))
                        .clicked()
                    {
                        self.sort = match self.sort {
                            Some((s, true)) if s == c => Some((c, false)),
                            Some((s, false)) if s == c => None,
                            _ => Some((c, true)),
                        };
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_rows(ui, row_height, order.len(), |ui, range| {
                    for &i in &order[range] {
                        let row = &rows[i];
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            for (c, col) in columns.iter().enumerate() {
                                let original = &row.cells[c];
                                if c == 0 {
                                    let selected = match kind {
                                        TableKind::WeatherCases => row.key == case.to_string(),
                                        _ => {
                                            *selection == row.select
                                                && row.select != Selection::None
                                        }
                                    };
                                    if ui
                                        .add_sized(
                                            [col.1, row_height],
                                            egui::Button::selectable(selected, original),
                                        )
                                        .clicked()
                                    {
                                        action = Some(match kind {
                                            TableKind::WeatherCases => {
                                                TableAction::ShowCase(row.key.parse().unwrap_or(0))
                                            }
                                            _ => TableAction::Select(row.select.clone()),
                                        });
                                    }
                                    continue;
                                }
                                if !(col.2 && editable) {
                                    ui.add_sized(
                                        [col.1, row_height],
                                        egui::Label::new(egui::RichText::new(original).weak())
                                            .truncate(),
                                    )
                                    .on_hover_text(original);
                                    continue;
                                }
                                let edited = self.edits.get(&row.key).and_then(|e| e.get(&c));
                                let mut text = edited.cloned().unwrap_or_else(|| original.clone());
                                let mut edit = egui::TextEdit::singleline(&mut text);
                                if edited.is_some() {
                                    edit = edit.text_color(egui::Color32::YELLOW);
                                }
                                if ui.add_sized([col.1, row_height], edit).changed() {
                                    if text == *original {
                                        if let Some(e) = self.edits.get_mut(&row.key) {
                                            e.remove(&c);
                                            if e.is_empty() {
                                                self.edits.remove(&row.key);
                                            }
                                        }
                                    } else {
                                        self.edits
                                            .entry(row.key.clone())
                                            .or_default()
                                            .insert(c, text);
                                    }
                                }
                            }
                        });
                    }
                });
        });
        action
    }
}
