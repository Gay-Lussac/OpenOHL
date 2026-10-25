// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The tables kept in files: insulators (the library), weather cases (the
//! case file), point and spread load types (the project's library).

use super::*;

/// The strings of the model (resolved: the structures' own and their
/// types') labelled `id`, as "structure string".
pub(super) fn labelled(model: &Model, id: &str) -> Vec<String> {
    model
        .line
        .structures
        .iter()
        .flat_map(|s| {
            s.strings
                .iter()
                .filter(|x| x.insulator.as_deref() == Some(id))
                .map(|x| format!("{} {}", s.id, x.label))
        })
        .collect()
}

/// The string library's rows: one per insulator assembly.
pub(super) fn insulator_rows(model: &Model) -> Vec<Row> {
    let Some((_, library)) = model.types.as_ref() else {
        return Vec::new();
    };
    library
        .iter()
        .map(|a| Row {
            key: a.id.clone(),
            select: Selection::None,
            cells: vec![
                a.id.clone(),
                a.name.clone(),
                match a.kind {
                    ohl_structures::AssemblyKind::Suspension => "suspension",
                    ohl_structures::AssemblyKind::Strain => "strain",
                    ohl_structures::AssemblyKind::VString => "two strings",
                }
                .into(),
                f(a.length.si(), 3),
                f(a.weight.si(), 1),
                f(a.wind_area.si(), 4),
                f(a.axial_stiffness.si(), 0),
                a.half_width.map_or(String::new(), |h| f(h.si(), 3)),
                a.max_load.map_or(String::new(), |v| f(v.si(), 0)),
                a.max_swing
                    .map_or(String::new(), |v| f(v.si().to_degrees(), 2)),
                a.max_swing_across
                    .map_or(String::new(), |v| f(v.si().to_degrees(), 2)),
                a.max_swing_along
                    .map_or(String::new(), |v| f(v.si().to_degrees(), 2)),
                a.source.clone().unwrap_or_default(),
                labelled(model, &a.id).len().to_string(),
                a.volumes.swing_across.as_str().into(),
                a.volumes.swing_along.as_str().into(),
                a.volumes.twist.as_str().into(),
                a.volumes.strings.to_string(),
                a.volumes.arrangement.as_str().into(),
                if a.volumes.strings > 1 {
                    f(a.volumes.spacing, 3)
                } else {
                    String::new()
                },
                a.volumes.shapes.len().to_string(),
            ],
            defaulted: Vec::new(),
        })
        .collect()
}

/// The string library's changes: its files written or deleted; strings
/// whose label's length changed take it (their wire points move); strings
/// whose label is deleted get its values as their own (and a matching or
/// new label when applied).
pub(super) fn insulator_changes(
    model: &Model,
    rows: &[Row],
    page: &TablePage,
) -> Result<Changes, String> {
    use ohl_core::units::{Angle, Area, Force, Length};
    use ohl_structures::{AssemblyKind, InsulatorAssembly};
    let stored = model
        .stored_line()
        .ok_or_else(|| "read-only: open a project to edit".to_owned())?;
    let empty = ohl_structures::InsulatorLibrary::default();
    let library = model.types.as_ref().map_or(&empty, |(_, i)| i);
    let dir = crate::library::LibraryKind::Insulators.new_file_dir(&model.sources);
    let file = |id: &str| {
        dir.join(format!(
            "{id}.{}",
            ohl_structures::insulators::INSULATOR_FILE_EXTENSION
        ))
    };
    let read = |a: &mut InsulatorAssembly, c: &Cells| -> Result<(), String> {
        let what = |name: &str| format!("{}: {name}", a.id);
        a.name = c.get(1).trim().to_owned();
        a.kind = match c.get(2).trim() {
            "suspension" => AssemblyKind::Suspension,
            "strain" => AssemblyKind::Strain,
            "two strings" | "v_string" => AssemblyKind::VString,
            other => {
                return Err(format!(
                    "{}: kind \"{other}\": suspension, strain or two strings",
                    a.id
                ));
            }
        };
        a.length = Length::from_meters(num(&c.get(3), &what("length"))?);
        a.weight = Force::from_newtons(num(&c.get(4), &what("weight"))?);
        a.wind_area = Area::from_square_meters(num(&c.get(5), &what("wind area"))?);
        a.axial_stiffness = Force::from_newtons(num(&c.get(6), &what("stiffness"))?);
        a.half_width = opt_num(&c.get(7), &what("half width"))?.map(Length::from_meters);
        let col = |name: &str| TableKind::Insulators.column(name);
        let angle = |name: &str| -> Result<Option<Angle>, String> {
            Ok(opt_num(&c.get(col(name)), &what(name))?.map(Angle::from_degrees))
        };
        a.max_load = opt_num(&c.get(col("max load")), &what("max load"))?.map(Force::from_newtons);
        a.max_swing = angle("max swing")?;
        a.max_swing_across = angle("max across")?;
        a.max_swing_along = angle("max along")?;
        a.source = opt(&c.get(col("source")));
        // The volumes' freedoms and strings (its shapes: String shapes).
        let freedom = |name: &str| -> Result<ohl_structures::Freedom, String> {
            let t = c.get(col(name));
            if t.trim().is_empty() {
                return Ok(ohl_structures::Freedom::Free);
            }
            ohl_structures::Freedom::parse(&t)
                .ok_or_else(|| format!("{}: {name} \"{}\": free or blocked", a.id, t.trim()))
        };
        let v = &mut a.volumes;
        v.swing_across = freedom("swing across")?;
        v.swing_along = freedom("swing along")?;
        v.twist = freedom("twist")?;
        v.strings = match c.get(col("strings")).trim() {
            "" => 1,
            t => t
                .parse()
                .map_err(|_| format!("{}: strings \"{t}\": 1 to 4", a.id))?,
        };
        v.arrangement = match c.get(col("arrangement")).trim() {
            "" => ohl_structures::Arrangement::Across,
            t => ohl_structures::Arrangement::parse(t).ok_or_else(|| {
                format!("{}: arrangement \"{t}\": across, along or triangle", a.id)
            })?,
        };
        v.spacing = opt_num(&c.get(col("spacing")), &what("spacing"))?.unwrap_or(0.0);
        if v.strings <= 1 {
            v.spacing = 0.0;
        }
        a.validate().map_err(|e| format!("{}: {e}", a.id))
    };
    let mut out = Changes::default();
    let mut lengths: BTreeMap<String, f64> = BTreeMap::new();
    let mut unlabel: BTreeSet<String> = BTreeSet::new();
    for row in rows {
        let id = row.key.as_str();
        let Some(was) = library.get(id) else {
            continue;
        };
        let path = library
            .path(id)
            .map_or_else(|| file(id), std::path::Path::to_path_buf);
        if page.deleted.contains(id) {
            let users = labelled(model, id);
            let own: usize = stored
                .structures
                .iter()
                .flat_map(|s| &s.strings)
                .filter(|x| x.insulator.as_deref() == Some(id))
                .count();
            if users.len() > own {
                return Err(format!(
                    "{id} is used by structure types (their strings): change them first (Structures › Structure library)"
                ));
            }
            if !users.is_empty() {
                out.takes.push(format!(
                    "{id}: {} string(s) ({}{}) keep its values as their own and get a matching or new label",
                    users.len(),
                    users.iter().take(6).cloned().collect::<Vec<_>>().join(", "),
                    if users.len() > 6 { ", …" } else { "" }
                ));
            }
            out.library.push((path, None));
            unlabel.insert(id.to_owned());
            continue;
        }
        let Some(typed) = page.edits.get(id) else {
            continue;
        };
        let c = Cells {
            row: &row.cells,
            typed: Some(typed),
            defaulted: &row.defaulted,
        };
        let mut a = was.clone();
        read(&mut a, &c)?;
        if (a.length.si() - was.length.si()).abs() > 1e-9 {
            lengths.insert(id.to_owned(), a.length.si());
        }
        out.library.push((path, Some(a.to_toml())));
    }
    let mut new_ids: Vec<String> = Vec::new();
    for added in &page.added {
        let c = Cells {
            row: added,
            typed: None,
            defaulted: &[],
        };
        let id = c.get(0).trim().to_owned();
        if id.is_empty() {
            return Err("a new library entry needs a label".into());
        }
        if library.get(&id).is_some() || new_ids.contains(&id) {
            return Err(format!("label {id} is taken"));
        }
        let mut a = InsulatorAssembly {
            format: None,
            version: None,
            id: id.clone(),
            name: id.clone(),
            kind: AssemblyKind::Suspension,
            source: None,
            length: Length::from_meters(3.0),
            weight: Force::from_newtons(0.0),
            axial_stiffness: Force::from_newtons(5e7),
            wind_area: Area::from_square_meters(0.0),
            half_width: None,
            max_load: None,
            max_swing: None,
            max_swing_across: None,
            max_swing_along: None,
            volumes: Default::default(),
        };
        read(&mut a, &c)?;
        out.library.push((file(&id), Some(a.to_toml())));
        new_ids.push(id);
    }
    // The structures' own strings: a label's new length (from where each
    // points now), a deleted label's values as their own.
    for s in &stored.structures {
        if !s.strings.iter().any(|x| {
            x.insulator
                .as_ref()
                .is_some_and(|i| lengths.contains_key(i) || unlabel.contains(i))
        }) {
            continue;
        }
        let mut s = s.clone();
        s.take_string_geometry();
        for x in &mut s.strings {
            let Some(id) = x.insulator.clone() else {
                continue;
            };
            if let Some(l) = lengths.get(&id) {
                x.length = *l;
            }
            if unlabel.contains(&id)
                && let Some(a) = library.get(&id)
            {
                x.insulator = None;
                if x.weight == 0.0 {
                    x.weight = a.weight.si();
                }
                if x.wind_area == 0.0 {
                    x.wind_area = a.wind_area.si();
                }
                x.stiffness.get_or_insert(a.axial_stiffness.si());
                // Its limits too, where the string gives none.
                let l = a.limits();
                x.set_limits(x.limits().or(ohl_line::StringLimits {
                    max_load: l.max_load,
                    max_swing_deg: l.max_swing_deg,
                    max_swing_across_deg: l.max_swing_across_deg,
                    max_swing_along_deg: l.max_swing_along_deg,
                }));
            }
        }
        out.commands.push((
            format!("strings of {} from the string library", s.id),
            Command::Put {
                kind: Kind::Structure,
                id: s.id.clone(),
                value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
                ord: None,
            },
        ));
    }
    Ok(out)
}

/// The key of a shape's row: its assembly and its name.
fn shape_key(assembly: &str, shape: &str) -> String {
    format!("{assembly}\t{shape}")
}

/// "0.5" (a fraction) or "0.3 m" (metres) as a shape's position.
fn along_text(a: ohl_structures::Along) -> String {
    match a {
        ohl_structures::Along::Fraction(x) => f(x, 4),
        ohl_structures::Along::Metres(m) => format!("{} m", f(m, 3)),
    }
}

/// A shape's position typed: a fraction, or metres with "m".
fn parse_along(text: &str, what: &str) -> Result<ohl_structures::Along, String> {
    let t = text.trim();
    match t.strip_suffix('m') {
        Some(m) => Ok(ohl_structures::Along::Metres(num(m, what)?)),
        None => Ok(ohl_structures::Along::Fraction(num(t, what)?)),
    }
}

/// The String shapes table's rows: one per shape of each insulator
/// assembly, in the library's order.
pub(super) fn string_shape_rows(model: &Model) -> Vec<Row> {
    let Some((_, library)) = model.types.as_ref() else {
        return Vec::new();
    };
    let size = |v: f64, used: bool| if used { f(v, 3) } else { String::new() };
    library
        .iter()
        .flat_map(|a| {
            a.volumes.shapes.iter().map(move |s| {
                use ohl_structures::ShapeKind;
                let k = s.kind;
                Row {
                    key: shape_key(&a.id, &s.name),
                    select: Selection::None,
                    cells: vec![
                        a.id.clone(),
                        s.name.clone(),
                        k.as_str().into(),
                        s.state.as_str().into(),
                        along_text(s.at),
                        size(s.diameter, k != ShapeKind::Box),
                        size(s.end_diameter, k == ShapeKind::Cone),
                        size(s.length, k != ShapeKind::Sphere),
                        size(s.width, k == ShapeKind::Box),
                        size(s.depth, k == ShapeKind::Box),
                        f(s.offset[0], 3),
                        f(s.offset[1], 3),
                        if s.shared { "yes" } else { "" }.into(),
                    ],
                    defaulted: Vec::new(),
                }
            })
        })
        .collect()
}

/// A shape from a row's cells (the sizes its kind does not take dropped).
fn read_shape(c: &Cells, assembly: &str) -> Result<ohl_structures::StringShape, String> {
    use ohl_structures::{ElectricalState, ShapeKind, StringShape};
    let name = c.get(1).trim().to_owned();
    let what = |col: &str| format!("{assembly} {name}: {col}");
    let kind = ShapeKind::parse(&c.get(2)).ok_or_else(|| {
        format!(
            "{assembly} {name}: kind \"{}\": sphere, cylinder, box or cone",
            c.get(2).trim()
        )
    })?;
    let state = ElectricalState::parse(&c.get(3)).ok_or_else(|| {
        format!(
            "{assembly} {name}: state \"{}\": grounded, electrified or insulated",
            c.get(3).trim()
        )
    })?;
    let size = |k: usize, col: &str| -> Result<f64, String> {
        Ok(opt_num(&c.get(k), &what(col))?.unwrap_or(0.0))
    };
    let shape = StringShape {
        at: parse_along(&c.get(4), &what("at"))?,
        diameter: size(5, "diameter")?,
        end_diameter: size(6, "end diameter")?,
        length: size(7, "length")?,
        width: size(8, "width")?,
        depth: size(9, "depth")?,
        offset: [size(10, "offset across")?, size(11, "offset along")?],
        shared: matches!(c.get(12).trim(), "yes" | "true" | "1"),
        ..StringShape::new(&name, kind, state)
    }
    .sizes_of_its_kind();
    shape
        .check()
        .map_err(|e| format!("{assembly} {name}: {e}"))?;
    Ok(shape)
}

/// The String shapes table's changes: each assembly with a shape edited,
/// added or deleted written again (its file), in one change.
pub(super) fn string_shape_changes(
    model: &Model,
    rows: &[Row],
    page: &TablePage,
) -> Result<Changes, String> {
    model
        .stored_line()
        .ok_or_else(|| "read-only: open a project to edit".to_owned())?;
    let empty = ohl_structures::InsulatorLibrary::default();
    let library = model.types.as_ref().map_or(&empty, |(_, i)| i);
    let dir = crate::library::LibraryKind::Insulators.new_file_dir(&model.sources);
    // Each assembly's shapes as the rows now give them.
    let mut shapes: BTreeMap<String, Vec<ohl_structures::StringShape>> = BTreeMap::new();
    let mut touched: BTreeSet<String> = BTreeSet::new();
    for row in rows {
        let id = &row.cells[0];
        let list = shapes.entry(id.clone()).or_default();
        if page.deleted.contains(&row.key) {
            touched.insert(id.clone());
            continue;
        }
        let typed = page.edits.get(&row.key);
        if typed.is_some() {
            touched.insert(id.clone());
        }
        let c = Cells {
            row: &row.cells,
            typed,
            defaulted: &row.defaulted,
        };
        list.push(read_shape(&c, id)?);
    }
    for added in &page.added {
        let c = Cells {
            row: added,
            typed: None,
            defaulted: &[],
        };
        let id = c.get(0).trim().to_owned();
        if library.get(&id).is_none() {
            return Err(format!(
                "\"{id}\": no such library label (String library): a shape belongs to one"
            ));
        }
        let s = read_shape(&c, &id)?;
        shapes.entry(id.clone()).or_default().push(s);
        touched.insert(id);
    }
    let mut out = Changes::default();
    for id in touched {
        let Some(was) = library.get(&id) else {
            continue;
        };
        let mut a = was.clone();
        a.volumes.shapes = shapes.remove(&id).unwrap_or_default();
        a.validate().map_err(|e| format!("{id}: {e}"))?;
        let path = library.path(&id).map_or_else(
            || {
                dir.join(format!(
                    "{id}.{}",
                    ohl_structures::insulators::INSULATOR_FILE_EXTENSION
                ))
            },
            std::path::Path::to_path_buf,
        );
        out.library.push((path, Some(a.to_toml())));
    }
    Ok(out)
}

/// Weather cases: the model's case file with the changes written in
/// (comments and layout kept), checked; its path and new text.
/// The weather case file the table writes: the model's, else (a project
/// still on the default case) a new one the project keeps.
pub fn weather_path(model: &Model) -> Option<PathBuf> {
    model.sources.weather.clone().or_else(|| {
        model.can_edit().ok()?;
        Some(
            model
                .library_dir
                .as_ref()?
                .join("weather")
                .join(NEW_WEATHER_FILE),
        )
    })
}

/// The name of a project's first weather case file.
pub const NEW_WEATHER_FILE: &str = "weather-cases.ohlwc";

pub(super) fn weather_file(
    model: &Model,
    rows: &[Row],
    page: &TablePage,
) -> Result<(PathBuf, String), String> {
    let path =
        weather_path(model).ok_or_else(|| "the model has no weather case file".to_owned())?;
    // No file yet: the default case's row is written whole into a new one.
    let new = model.sources.weather.is_none();
    let text = if new {
        format!(
            "format = \"{}\"\nversion = {}\nname = \"Weather cases\"\n",
            ohl_weather::WEATHER_FILE_FORMAT,
            ohl_weather::WEATHER_FILE_VERSION
        )
    } else {
        std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?
    };
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let old = doc
        .get("case")
        .and_then(toml_edit::Item::as_array_of_tables)
        .cloned()
        .unwrap_or_default();
    let mut cases: Vec<(f64, toml_edit::Table)> = Vec::new();
    let mut renamed: Vec<(String, String)> = Vec::new();
    let write = |t: &mut toml_edit::Table, c: &Cells, all: bool| -> Result<(), String> {
        let name = c.get(W_NAME);
        let label = |col: usize| WEATHER[col].0.replace('\n', " ").trim().to_owned();
        let number = |col: usize, unit: &str| -> Result<Option<toml_edit::Item>, String> {
            let v = c.get(col);
            let v = v.trim();
            if v.is_empty() {
                return Ok(None);
            }
            let x = num(v, &format!("{name} {}", label(col)))?;
            Ok(Some(toml_edit::value(format!("{x} {unit}"))))
        };
        let mut set = |field: &str, item: Option<toml_edit::Item>| match item {
            Some(i) => {
                t.insert(field, i);
            }
            None => {
                t.remove(field);
            }
        };
        let touched = |col: usize| all || c.changed(col);
        if touched(W_NAME) {
            let v = c.get(W_NAME).trim().to_owned();
            if v.is_empty() {
                return Err("a case needs a name".into());
            }
            set("name", Some(toml_edit::value(v)));
        }
        if touched(W_DESCRIPTION) {
            let v = c.get(W_DESCRIPTION).trim().to_owned();
            set("description", (!v.is_empty()).then(|| toml_edit::value(v)));
        }
        if touched(W_TAGS) {
            let mut a = toml_edit::Array::new();
            for tag in c
                .get(W_TAGS)
                .split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
            {
                a.push(tag);
            }
            set("tags", (!a.is_empty()).then(|| toml_edit::value(a)));
        }
        if touched(W_TEMP) {
            if c.get(W_TEMP).trim().is_empty() {
                return Err(format!("{name}: a temperature is needed"));
            }
            set("temperature", number(W_TEMP, "°C")?);
        }
        if touched(W_AMBIENT) {
            set("ambient_temperature", number(W_AMBIENT, "°C")?);
        }
        if touched(W_ICE) {
            set("ice_thickness", number(W_ICE, "mm")?);
        }
        if touched(W_ICE_LOAD) {
            set("ice_load", number(W_ICE_LOAD, "N/m")?);
        }
        if touched(W_ICE_DENSITY) {
            set("ice_density", number(W_ICE_DENSITY, "kg/m³")?);
        }
        // Wind: pressure P = Q·V². The pressure typed wins; else a velocity
        // (or another Q) typed gives the pressure of the velocity shown.
        if touched(W_Q) {
            let q = opt_num(&c.get(W_Q), &format!("{name} air density factor"))?;
            if q.is_some_and(|q| q <= 0.0) {
                return Err(format!("{name}: the air density factor must be > 0"));
            }
            set("air_density_factor", q.map(toml_edit::value));
        }
        let q = opt_num(&c.get(W_Q), &format!("{name} air density factor"))?
            .unwrap_or(model.line.defaults.air_density_factor);
        if touched(W_PRESSURE) && (all || !c.changed(W_VELOCITY)) {
            set("wind_pressure", number(W_PRESSURE, "Pa")?);
        } else if c.changed(W_VELOCITY) || c.changed(W_Q) {
            let v = opt_num(&c.get(W_VELOCITY), &format!("{name} wind velocity"))?.unwrap_or(0.0);
            if v < 0.0 {
                return Err(format!("{name}: the wind velocity must be ≥ 0"));
            }
            let p = q * v * v;
            set(
                "wind_pressure",
                (p > 0.0)
                    .then(|| toml_edit::value(format!("{} Pa", (p * 1000.0).round() / 1000.0))),
            );
        }
        Ok(())
    };
    for row in rows {
        if page.deleted.contains(&row.key) {
            continue;
        }
        let k: usize = row.key.parse().map_err(|_| "bad row".to_owned())?;
        let mut t = if new {
            toml_edit::Table::new()
        } else {
            old.get(k)
                .cloned()
                .ok_or_else(|| format!("case {}: not in {}", row.cells[1], path.display()))?
        };
        let c = Cells {
            row: &row.cells,
            typed: page.edits.get(&row.key),
            defaulted: &row.defaulted,
        };
        write(&mut t, &c, new)?;
        // Renamed: its groups follow.
        let after = c.get(W_NAME).trim().to_owned();
        if after != row.cells[W_NAME] && !after.is_empty() {
            renamed.push((row.cells[W_NAME].clone(), after));
        }
        cases.push((
            num(&c.get(W_ORDER), &format!("{} #", row.cells[W_NAME]))?,
            t,
        ));
    }
    for cells in &page.added {
        let c = Cells {
            row: cells,
            typed: None,
            defaulted: &[],
        };
        let mut t = toml_edit::Table::new();
        write(&mut t, &c, true)?;
        let order = opt_num(&c.get(W_ORDER), "new case #")?.unwrap_or(f64::MAX);
        cases.push((order + 1e-9, t));
    }
    cases.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut array = toml_edit::ArrayOfTables::new();
    for (_, t) in cases {
        array.push(t);
    }
    doc.insert("case", toml_edit::Item::ArrayOfTables(array));
    // The weather case groups name the cases renamed by their new names.
    if let Some(groups) = doc
        .get_mut("group")
        .and_then(toml_edit::Item::as_array_of_tables_mut)
    {
        for g in groups.iter_mut() {
            if let Some(a) = g.get_mut("cases").and_then(toml_edit::Item::as_array_mut) {
                for v in a.iter_mut() {
                    if let Some((_, to)) = renamed
                        .iter()
                        .find(|(from, _)| Some(from.as_str()) == v.as_str())
                    {
                        *v = to.as_str().into();
                    }
                }
            }
        }
    }
    let text = doc.to_string();
    ohl_weather::WeatherCases::parse(&text)?;
    Ok((path, text))
}

/// The project's own point and spread load types (its library file).
pub(super) fn project_load_types(model: &Model) -> ohl_line::ObjectLibrary {
    model
        .editing
        .as_ref()
        .ok()
        .and_then(|e| {
            e.session
                .library_files(crate::model::Model::OBJECT_LIBRARY_KIND)
                .ok()
        })
        .into_iter()
        .flatten()
        .filter_map(|(_, text)| ohl_line::ObjectLibrary::parse(&text).ok())
        .reduce(|mut a, b| {
            a.types.extend(b.types);
            a
        })
        .unwrap_or_default()
}

/// The types to choose from, the project's marked.
pub(super) fn load_type_rows(model: &Model) -> Vec<Row> {
    let own = project_load_types(model);
    model
        .object_library()
        .types
        .iter()
        .map(|t| {
            let o = t.object();
            let mut c = object_cells(o.as_ref());
            if t.kind == ohl_line::TypeKind::Spread {
                c = Default::default();
            }
            let mut cells = vec![
                t.id.clone(),
                if own.get(&t.id).is_some() {
                    "project"
                } else {
                    "template"
                }
                .to_owned(),
                t.name.clone(),
                match t.kind {
                    ohl_line::TypeKind::Body => "body",
                    ohl_line::TypeKind::Clamp => "clamp",
                    ohl_line::TypeKind::Spread => "spread",
                }
                .to_owned(),
            ];
            cells.extend(c);
            cells.push(t.clamp_length.map_or(String::new(), |l| f(l, 3)));
            cells.push(t.load.map_or(String::new(), |l| f(l, 2)));
            cells.push(t.description.clone());
            cells.push(if t.kind == ohl_line::TypeKind::Spread {
                load_on_text(t.per_subconductor)
            } else {
                String::new()
            });
            Row {
                defaulted: Vec::new(),
                key: t.id.clone(),
                select: Selection::None,
                cells,
            }
        })
        .collect()
}

/// A type as its cells say (columns as [`LOAD_TYPES`]).
pub(super) fn load_type_from_cells(c: &Cells, id: &str) -> Result<ohl_line::ObjectType, String> {
    let kind = match c.get(3).trim().to_lowercase().as_str() {
        "body" => ohl_line::TypeKind::Body,
        "clamp" => ohl_line::TypeKind::Clamp,
        "spread" => ohl_line::TypeKind::Spread,
        other => {
            return Err(format!(
                "type {id}: kind \"{other}\": body, clamp or spread"
            ));
        }
    };
    // The object's columns, shifted to where object_from_cells reads them.
    let shifted: Vec<String> = (0..13)
        .map(|k| match k {
            6..=12 => c.get(k - 2),
            _ => String::new(),
        })
        .collect();
    let mut o = ohl_line::LineObject::default();
    if kind != ohl_line::TypeKind::Spread {
        object_from_cells(
            &mut o,
            &Cells {
                row: &shifted,
                typed: None,
                defaulted: &[],
            },
            &format!("type {id}"),
        )?;
    }
    Ok(ohl_line::ObjectType {
        id: id.to_owned(),
        name: c.get(2).trim().to_owned(),
        description: c.get(13).trim().to_owned(),
        kind,
        weight: o.weight,
        shape: (kind != ohl_line::TypeKind::Spread && o.shape != ohl_line::Shape::Point)
            .then_some(o.shape),
        drag: o.drag,
        clamp_length: opt_num(&c.get(11), &format!("type {id} clamp length"))?,
        load: if kind == ohl_line::TypeKind::Spread {
            Some(num(&c.get(12), &format!("type {id} load"))?)
        } else {
            None
        },
        per_subconductor: kind == ohl_line::TypeKind::Spread
            && parse_load_on(&c.get(14)).map_err(|e| format!("type {id}: {e}"))?,
    })
}

/// The name of the project's library file of its own load types: the one
/// it holds (`project.olsload` in projects of before 2026-10-18), else
/// `project.ohlload`.
pub(super) fn project_load_types_file(model: &Model) -> String {
    model
        .editing
        .as_ref()
        .ok()
        .and_then(|e| {
            e.session
                .library_files(crate::model::Model::OBJECT_LIBRARY_KIND)
                .ok()
        })
        .and_then(|files| files.into_iter().next().map(|(name, _)| name))
        .unwrap_or_else(|| format!("project.{}", ohl_line::objects::OBJECT_FILE_EXTENSION))
}

/// The load types table's changes: the project's own types written to
/// its library file (a template changed becomes the project's of its id;
/// a project's type deleted brings its template back, if any).
pub(super) fn load_type_changes(
    model: &Model,
    rows: &[Row],
    page: &TablePage,
) -> Result<Changes, String> {
    model.can_edit()?;
    let mut own = project_load_types(model);
    for key in &page.deleted {
        if own.get(key).is_none() {
            return Err(format!(
                "type {key} is a shipped template: it stays (change it to make the project's own)"
            ));
        }
        own.types.retain(|t| &t.id != key);
    }
    for (key, typed) in page
        .edits
        .iter()
        .filter(|(k, _)| !page.deleted.contains(*k))
    {
        let row = rows
            .iter()
            .find(|r| &r.key == key)
            .ok_or_else(|| format!("type {key} is gone"))?;
        let c = Cells {
            row: &row.cells,
            typed: Some(typed),
            defaulted: &row.defaulted,
        };
        let t = load_type_from_cells(&c, key)?;
        own.types.retain(|x| x.id != t.id);
        own.types.push(t);
    }
    for cells in &page.added {
        let c = Cells {
            row: cells,
            typed: None,
            defaulted: &[],
        };
        let id = c.get(0).trim().to_owned();
        if id.is_empty() || model.object_library().get(&id).is_some() && own.get(&id).is_some() {
            return Err(format!("a new type needs an id of its own ({id:?})"));
        }
        let t = load_type_from_cells(&c, &id)?;
        own.types.retain(|x| x.id != t.id);
        own.types.push(t);
    }
    if own.name.is_empty() {
        own.name = "Project types".into();
    }
    own.validate()?;
    Ok(Changes {
        project_library: Some((
            crate::model::Model::OBJECT_LIBRARY_KIND.to_owned(),
            project_load_types_file(model),
            own.to_text(),
        )),
        ..Changes::default()
    })
}
