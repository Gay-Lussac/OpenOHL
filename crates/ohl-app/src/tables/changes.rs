// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The rows as edited made into one session command: the objects' cells
//! parsed, the plugins' fields, targets reached.

use super::*;

impl TableKind {
    /// What the page's changes do.
    pub fn changes(
        self,
        model: &Model,
        case: usize,
        rows: &[Row],
        page: &TablePage,
    ) -> Result<Changes, String> {
        if self == TableKind::WeatherCases {
            return weather_file(model, rows, page).map(|file| Changes {
                file: Some(file),
                ..Changes::default()
            });
        }
        if self == TableKind::Insulators {
            return insulator_changes(model, rows, page);
        }
        if self == TableKind::StringShapes {
            return string_shape_changes(model, rows, page);
        }
        if self == TableKind::LoadTypes {
            return load_type_changes(model, rows, page);
        }
        let stored = model
            .stored_line()
            .ok_or_else(|| "read-only: open a project to edit".to_owned())?;
        let session = &model
            .editing
            .as_ref()
            .map_err(|e| format!("read-only: {e}"))?
            .session;
        let row_of = |key: &str| {
            rows.iter()
                .find(|r| r.key == key)
                .ok_or_else(|| format!("{key} is no longer in the line"))
        };
        let existing = |key: &String| -> Result<Cells<'_>, String> {
            let row = row_of(key)?;
            Ok(Cells {
                row: &row.cells,
                typed: page.edits.get(key),
                defaulted: &row.defaulted,
            })
        };
        let mut out = Changes::default();
        match self {
            TableKind::Structures => {
                for key in &page.deleted {
                    let i = session.structure_deletion(key).map_err(|e| e.to_string())?;
                    if i.has_dependents() {
                        out.takes.push(i.summary());
                    }
                    out.commands.push((
                        format!("deleted structure {key}"),
                        Command::Delete {
                            kind: Kind::Structure,
                            id: key.clone(),
                            confirm: true,
                        },
                    ));
                }
                for key in page.edits.keys().filter(|k| !page.deleted.contains(*k)) {
                    let c = existing(key)?;
                    let mut form = StructureForm::of(model, key)
                        .ok_or_else(|| format!("no structure {key}"))?;
                    // The table changes the structure only (its alignment
                    // point stays tied as it is).
                    form.on_alignment = None;
                    structure_cells(&mut form, &c, model, key)?;
                    let (what, cmd) = form.command(Some(stored))?;
                    let new_id = c.get(0).trim().to_owned();
                    if new_id != *key {
                        out.commands.push((
                            format!("renamed structure {key} to {new_id}"),
                            Command::Rename {
                                kind: Kind::Structure,
                                id: key.clone(),
                                new_id: new_id.clone(),
                            },
                        ));
                        out.commands.push((what, retarget(cmd, &new_id)));
                    } else {
                        out.commands.push((what, cmd));
                    }
                }
                for cells in &page.added {
                    let c = Cells {
                        row: cells,
                        typed: None,
                        defaulted: &[],
                    };
                    let id = c.get(0).trim().to_owned();
                    let mut form = StructureForm::new_at(model, [0.0; 3]);
                    form.id = id.clone();
                    form.structure_type = None;
                    form.on_ground = false;
                    structure_cells(&mut form, &c, model, &id)?;
                    out.commands.push(form.command(Some(stored))?);
                }
            }
            TableKind::Sections => {
                for key in &page.deleted {
                    let i = session
                        .deletion_impact(Kind::Section, key)
                        .map_err(|e| e.to_string())?;
                    if i.has_dependents() {
                        out.takes.push(i.summary());
                    }
                    out.commands.push((
                        format!("deleted section {key}"),
                        Command::Delete {
                            kind: Kind::Section,
                            id: key.clone(),
                            confirm: true,
                        },
                    ));
                }
                for key in page.edits.keys().filter(|k| !page.deleted.contains(*k)) {
                    let c = existing(key)?;
                    let display = TableKind::Sections.column("display cases");
                    let only_display = page
                        .edits
                        .get(key)
                        .is_some_and(|e| e.keys().all(|k| *k == display));
                    let new_id = c.get(0).trim().to_owned();
                    out.display
                        .push((new_id.clone(), display_case(model, &c.get(display))?));
                    if only_display {
                        continue;
                    }
                    let mut form =
                        SectionForm::of(model, key).ok_or_else(|| format!("no section {key}"))?;
                    section_cells(&mut form, &c, model, case, key)?;
                    let (what, cmd) = form.command(Some(stored))?;
                    // Its circuit's other sections take its name, phase and
                    // system (but those changed here by hand): listed to
                    // confirm.
                    let by_hand: Vec<String> = page.edits.keys().cloned().collect();
                    let mates: Vec<_> = form
                        .circuit_mates(stored, key)
                        .into_iter()
                        .filter(|(id, _)| !by_hand.contains(id))
                        .collect();
                    if !mates.is_empty() {
                        out.takes.push(format!(
                            "section {key}: its conductor path's {} change too",
                            mates
                                .iter()
                                .map(|(id, what)| format!("{id} ({})", what.join(", ")))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                    let mut mate_puts = form.circuit_puts(stored, key, &by_hand)?;
                    // A colour typed: every section of its circuit (by its
                    // name) takes it, listed to confirm.
                    if c.changed(TableKind::Sections.column("colour"))
                        && !form.circuit.trim().is_empty()
                    {
                        let mut taking = Vec::new();
                        for s in stored.sections.iter().filter(|s| {
                            s.id != *key
                                && s.circuit_name == form.circuit
                                && !by_hand.contains(&s.id)
                                && s.color != form.color
                        }) {
                            let put = mate_puts.iter_mut().find(|(_, cmd)| {
                                matches!(cmd, Command::Put { kind: Kind::Section, id, .. } if *id == s.id)
                            });
                            match put {
                                Some((_, Command::Put { value, .. })) => {
                                    value["color"] = serde_json::to_value(&form.color)
                                        .map_err(|e| e.to_string())?;
                                }
                                _ => {
                                    let mut m = s.clone();
                                    m.color = form.color.clone();
                                    mate_puts.push((
                                        format!(
                                            "section {}: the colour of its conductor path",
                                            s.id
                                        ),
                                        Command::Put {
                                            kind: Kind::Section,
                                            id: s.id.clone(),
                                            value: serde_json::to_value(&m)
                                                .map_err(|e| e.to_string())?,
                                            ord: None,
                                        },
                                    ));
                                }
                            }
                            taking.push(s.id.clone());
                        }
                        if !taking.is_empty() {
                            out.takes.push(format!(
                                "section {key}: its conductor path {}'s sections take its colour: {}",
                                form.circuit,
                                taking.join(", ")
                            ));
                        }
                    }
                    if new_id != *key {
                        out.commands.push((
                            format!("renamed section {key} to {new_id}"),
                            Command::Rename {
                                kind: Kind::Section,
                                id: key.clone(),
                                new_id: new_id.clone(),
                            },
                        ));
                        out.commands.push((what, retarget(cmd, &new_id)));
                    } else {
                        out.commands.push((what, cmd));
                    }
                    out.commands.extend(mate_puts);
                }
                for cells in &page.added {
                    let c = Cells {
                        row: cells,
                        typed: None,
                        defaulted: &[],
                    };
                    let id = c.get(0).trim().to_owned();
                    let mut form = SectionForm::new_through(model, case, Vec::new(), None);
                    form.id = id.clone();
                    section_cells(&mut form, &c, model, case, &id)?;
                    out.display.push((
                        id,
                        display_case(model, &c.get(TableKind::Sections.column("display cases")))?,
                    ));
                    out.commands.push(form.command(Some(stored))?);
                }
            }
            TableKind::Strings => {
                // One put per structure with all its strings changed.
                let mut structures: BTreeMap<String, Structure> = BTreeMap::new();
                let take = |structures: &mut BTreeMap<String, Structure>,
                            id: &str|
                 -> Result<(), String> {
                    if !structures.contains_key(id) {
                        let s = stored
                            .structure(id)
                            .ok_or_else(|| format!("no structure {id}"))?
                            .clone();
                        structures.insert(id.to_owned(), s);
                    }
                    Ok(())
                };
                // A string of a typed structure's type becomes its own with
                // the strings and wire points that move with it.
                let own = |s: &mut Structure, label: &str| -> Result<(), String> {
                    if s.string(label).is_some() {
                        return Ok(());
                    }
                    let form = StringForm::of_string(model, &s.id, label)
                        .ok_or_else(|| format!("no string {} {label}", s.id))?;
                    form.apply_to(s, model.line.structure(&s.id));
                    Ok(())
                };
                for key in &page.deleted {
                    let (id, label) = key.split_once('\t').unwrap_or((key, ""));
                    take(&mut structures, id)?;
                    let s = structures.get_mut(id).expect("taken");
                    own(s, label)?;
                    s.strings.retain(|x| x.label != label);
                }
                for key in page.edits.keys().filter(|k| !page.deleted.contains(*k)) {
                    let (id, label) = key.split_once('\t').unwrap_or((key, ""));
                    take(&mut structures, id)?;
                    let c = existing(key)?;
                    let s = structures.get_mut(id).expect("taken");
                    own(s, label)?;
                    let x = s
                        .strings
                        .iter_mut()
                        .find(|x| x.label == label)
                        .ok_or_else(|| format!("no string {id} {label}"))?;
                    string_cells(x, &c, &format!("{id} {label}"))?;
                }
                for cells in &page.added {
                    let c = Cells {
                        row: cells,
                        typed: None,
                        defaulted: &[],
                    };
                    let id = c.get(0).trim().to_owned();
                    if id.is_empty() {
                        return Err("a new string needs its structure".into());
                    }
                    take(&mut structures, &id)?;
                    let s = structures.get_mut(&id).expect("taken");
                    let mut x = ohl_line::InsulatorString::default();
                    string_cells(&mut x, &c, &format!("{id} new string"))?;
                    if x.label.is_empty() {
                        x.label = format!("S{}", s.strings.len() + 1);
                    }
                    s.strings.push(x);
                }
                for (id, s) in structures {
                    out.commands.push((
                        format!("changed strings of {id}"),
                        Command::Put {
                            kind: Kind::Structure,
                            id: id.clone(),
                            value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
                            ord: None,
                        },
                    ));
                }
            }
            TableKind::Jumpers => {
                for key in &page.deleted {
                    out.commands.push((
                        format!("deleted jumper {key}"),
                        Command::Delete {
                            kind: Kind::Jumper,
                            id: key.clone(),
                            confirm: true,
                        },
                    ));
                }
                for key in page.edits.keys().filter(|k| !page.deleted.contains(*k)) {
                    let c = existing(key)?;
                    let mut j = stored
                        .jumpers
                        .iter()
                        .find(|j| &j.id == key)
                        .ok_or_else(|| format!("no jumper {key}"))?
                        .clone();
                    jumper_cells(&mut j, &c, model, key)?;
                    let new_id = c.get(0).trim().to_owned();
                    let put = Command::Put {
                        kind: Kind::Jumper,
                        id: key.clone(),
                        value: serde_json::to_value(&j).map_err(|e| e.to_string())?,
                        ord: None,
                    };
                    if new_id != *key {
                        out.commands.push((
                            format!("renamed jumper {key} to {new_id}"),
                            Command::Rename {
                                kind: Kind::Jumper,
                                id: key.clone(),
                                new_id: new_id.clone(),
                            },
                        ));
                        out.commands
                            .push((format!("changed jumper {new_id}"), retarget(put, &new_id)));
                    } else {
                        out.commands.push((format!("changed jumper {key}"), put));
                    }
                }
                for cells in &page.added {
                    let c = Cells {
                        row: cells,
                        typed: None,
                        defaulted: &[],
                    };
                    let id = c.get(0).trim().to_owned();
                    let mut j: ohl_line::Jumper = serde_json::from_value(serde_json::json!({
                        "id": id,
                        "cable": "",
                        "from": {"structure": "", "set": 0, "phase": 0},
                        "to": {"structure": "", "set": 0, "phase": 0},
                    }))
                    .map_err(|e| e.to_string())?;
                    jumper_cells(&mut j, &c, model, &id)?;
                    out.commands.push((
                        format!("added jumper {id}"),
                        Command::Put {
                            kind: Kind::Jumper,
                            id,
                            value: serde_json::to_value(&j).map_err(|e| e.to_string())?,
                            ord: None,
                        },
                    ));
                }
            }
            TableKind::Systems => {
                let mut systems = stored.systems.clone();
                for key in &page.deleted {
                    let n = stored
                        .sections
                        .iter()
                        .filter(|s| s.system.as_deref() == Some(key.as_str()))
                        .count();
                    if n > 0 {
                        out.takes
                            .push(format!("system {key}: {n} sections lose their system"));
                    }
                    systems.retain(|s| &s.id != key);
                    for sec in stored
                        .sections
                        .iter()
                        .filter(|s| s.system.as_deref() == Some(key.as_str()))
                    {
                        let mut sec = sec.clone();
                        sec.system = None;
                        out.commands.push((
                            format!("section {} without a system", sec.id),
                            Command::Put {
                                kind: Kind::Section,
                                id: sec.id.clone(),
                                value: serde_json::to_value(&sec).map_err(|e| e.to_string())?,
                                ord: None,
                            },
                        ));
                    }
                }
                for key in page.edits.keys().filter(|k| !page.deleted.contains(*k)) {
                    let c = existing(key)?;
                    let new_id = c.get(0).trim().to_owned();
                    if new_id != *key {
                        out.commands.push((
                            format!("renamed system {key} to {new_id}"),
                            Command::RenameSystem {
                                id: key.clone(),
                                new_id: new_id.clone(),
                            },
                        ));
                    }
                    let s = systems
                        .iter_mut()
                        .find(|s| &s.id == key)
                        .ok_or_else(|| format!("no system {key}"))?;
                    s.id = new_id;
                    system_cells(s, &c, key)?;
                }
                for cells in &page.added {
                    let c = Cells {
                        row: cells,
                        typed: None,
                        defaulted: &[],
                    };
                    let id = c.get(0).trim().to_owned();
                    let mut s: ohl_line::electrical::System =
                        serde_json::from_value(serde_json::json!({
                            "id": id, "voltage_kv": 0.0, "phases": []
                        }))
                        .map_err(|e| e.to_string())?;
                    system_cells(&mut s, &c, &id)?;
                    systems.push(s);
                }
                // The systems are part of the line settings.
                out.commands.push(("changed electrical systems".into(), {
                    let mut line = stored.clone();
                    line.systems = systems;
                    // A circuit of a system deleted: of none said.
                    for c in &mut line.circuits {
                        if c.system.as_ref().is_some_and(|s| page.deleted.contains(s)) {
                            c.system = None;
                        }
                    }
                    ohl_session::line_settings_change(stored, &line)
                }));
            }
            TableKind::Alignments => alignment_list_commands(stored, rows, page, &mut out)?,
            TableKind::AlignmentPoints => {
                point_commands(model, stored, rows, page, &mut out)?;
            }
            TableKind::ConductorPaths => circuit_commands(stored, rows, page, &mut out)?,
            TableKind::Circuits => circuit_list_commands(stored, page, &mut out)?,
            TableKind::Routes => {
                for key in &page.deleted {
                    let i = session
                        .deletion_impact(Kind::Route, key)
                        .map_err(|e| e.to_string())?;
                    if i.has_dependents() {
                        out.takes.push(i.summary());
                    }
                    out.commands.push((
                        format!("deleted route {key}"),
                        Command::Delete {
                            kind: Kind::Route,
                            id: key.clone(),
                            confirm: true,
                        },
                    ));
                }
                for key in page.edits.keys().filter(|k| !page.deleted.contains(*k)) {
                    let c = existing(key)?;
                    let mut r = stored
                        .routes
                        .iter()
                        .find(|r| &r.id == key)
                        .ok_or_else(|| format!("no route {key}"))?
                        .clone();
                    let new_id = c.get(0).trim().to_owned();
                    if new_id.is_empty() {
                        return Err(format!("route {key}: give it an id"));
                    }
                    r.name = c.get(1).trim().to_owned();
                    r.description = c.get(5).trim().to_owned();
                    if new_id != *key {
                        out.commands.push((
                            format!("renamed route {key} to {new_id}"),
                            Command::Rename {
                                kind: Kind::Route,
                                id: key.clone(),
                                new_id: new_id.clone(),
                            },
                        ));
                        r.id = new_id.clone();
                    }
                    out.commands.push((
                        format!("changed route {}", r.id),
                        Command::Put {
                            kind: Kind::Route,
                            id: r.id.clone(),
                            value: serde_json::to_value(&r).map_err(|e| e.to_string())?,
                            ord: None,
                        },
                    ));
                }
                for cells in &page.added {
                    let c = Cells {
                        row: cells,
                        typed: None,
                        defaulted: &[],
                    };
                    let (what, cmd) = crate::editors::new_route(stored, &c.get(0), &c.get(1))?;
                    let mut cmd = cmd;
                    if let Command::Put { value, .. } = &mut cmd {
                        value["description"] = c.get(5).trim().into();
                    }
                    out.commands.push((what, cmd));
                }
            }
            TableKind::Spacers => spacer_commands(stored, page, &existing, &mut out)?,
            TableKind::SpanObjects => span_object_commands(stored, page, &existing, &mut out)?,
            TableKind::WeatherCases
            | TableKind::Insulators
            | TableKind::StringShapes
            | TableKind::LoadTypes => {
                unreachable!("written to its files")
            }
        }
        self.plugin_changes(model, stored, page, &mut out)?;
        Ok(out)
    }

    /// Plugin column edits: each object's value set (empty: unset) in its
    /// put among `out`'s commands, else in a put of its own.
    pub(super) fn plugin_changes(
        self,
        model: &Model,
        stored: &ohl_line::Line,
        page: &TablePage,
        out: &mut Changes,
    ) -> Result<(), String> {
        let columns = self.plugin_columns(model);
        if columns.is_empty() {
            return Ok(());
        }
        let n = self.columns().len();
        let circuits =
            (self == TableKind::ConductorPaths).then(|| ohl_line::electrical::circuits(stored));
        let value_of = |c: &PluginColumn, text: &str| -> Result<String, String> {
            let v = text.trim();
            if v.is_empty() || c.values.is_empty() {
                return Ok(v.to_owned());
            }
            c.values
                .iter()
                .find(|x| x.eq_ignore_ascii_case(v))
                .cloned()
                .ok_or_else(|| {
                    format!("{}: \"{v}\" is not one of {}", c.title, c.values.join(", "))
                })
        };
        // A circuit's values go to its sections.
        let ids_of = |key: &str, id: String| -> Vec<String> {
            match &circuits {
                Some(cs) => cs
                    .iter()
                    .find(|c| circuit_key(c) == key)
                    .map_or_else(Vec::new, |c| c.sections.clone()),
                None => vec![id],
            }
        };
        let mut sets: Vec<(Vec<String>, &PluginColumn, String)> = Vec::new();
        for (key, edits) in page
            .edits
            .iter()
            .filter(|(k, _)| !page.deleted.contains(*k))
        {
            // Renamed: its new id.
            let id = edits
                .get(&0)
                .map_or_else(|| key.clone(), |t| t.trim().to_owned());
            for (&c, text) in edits.range(n..) {
                if let Some(col) = columns.get(c - n) {
                    sets.push((ids_of(key, id.clone()), col, value_of(col, text)?));
                }
            }
        }
        for cells in &page.added {
            let id = cells
                .first()
                .map_or_else(String::new, |t| t.trim().to_owned());
            for (k, col) in columns.iter().enumerate() {
                let text = cells.get(n + k).map_or("", String::as_str);
                if !text.trim().is_empty() {
                    sets.push((vec![id.clone()], col, value_of(col, text)?));
                }
            }
        }
        for (ids, col, value) in sets {
            for id in ids {
                self.set_field(stored, out, &id, col, &value)?;
            }
        }
        Ok(())
    }

    /// Sets plugin value `col` of object `id` to `value` (empty: unset).
    pub(super) fn set_field(
        self,
        stored: &ohl_line::Line,
        out: &mut Changes,
        id: &str,
        col: &PluginColumn,
        value: &str,
    ) -> Result<(), String> {
        let kind = match self {
            TableKind::Structures => Kind::Structure,
            TableKind::Sections | TableKind::ConductorPaths => Kind::Section,
            TableKind::Jumpers => Kind::Jumper,
            TableKind::Alignments => Kind::Alignment,
            TableKind::Routes => Kind::Route,
            TableKind::Systems => Kind::Meta,
            _ => return Ok(()),
        };
        // The systems are a row of the line settings.
        let put_id = if kind == Kind::Meta { "systems" } else { id };
        let found = out
            .commands
            .iter_mut()
            .rev()
            .find_map(|(_, c)| put_in(c, kind, put_id));
        let json = |e: serde_json::Error| e.to_string();
        let mut own = None;
        let value_json = match found {
            Some(v) => v,
            None => {
                let missing = || format!("no {} {id}", kind.as_str());
                let v = match self {
                    TableKind::Structures => {
                        serde_json::to_value(stored.structure(id).ok_or_else(missing)?)
                    }
                    TableKind::Sections | TableKind::ConductorPaths => serde_json::to_value(
                        stored
                            .sections
                            .iter()
                            .find(|s| s.id == id)
                            .ok_or_else(missing)?,
                    ),
                    TableKind::Jumpers => serde_json::to_value(
                        stored
                            .jumpers
                            .iter()
                            .find(|j| j.id == id)
                            .ok_or_else(missing)?,
                    ),
                    TableKind::Alignments => serde_json::to_value(
                        stored
                            .alignments
                            .iter()
                            .find(|a| a.id == id)
                            .ok_or_else(missing)?,
                    ),
                    TableKind::Routes => serde_json::to_value(
                        stored
                            .routes
                            .iter()
                            .find(|r| r.id == id)
                            .ok_or_else(missing)?,
                    ),
                    _ => Ok(serde_json::json!({ "systems": stored.systems })),
                }
                .map_err(json)?;
                own.insert(v)
            }
        };
        let target = if kind == Kind::Meta {
            value_json["systems"]
                .as_array_mut()
                .and_then(|a| a.iter_mut().find(|s| s["id"] == id))
                .ok_or_else(|| format!("no system {id}"))?
        } else {
            value_json
        };
        patch_field(target, &col.key, value);
        if let Some(value) = own {
            out.commands.push((
                format!("set {} of {} {id}", col.title, kind.as_str()),
                Command::Put {
                    kind,
                    id: put_id.to_owned(),
                    value,
                    ord: None,
                },
            ));
        }
        Ok(())
    }
}

/// The value of the put of object (`kind`, `id`) in `c` (a batch
/// searched from its end).
pub(super) fn put_in<'a>(
    c: &'a mut Command,
    kind: Kind,
    id: &str,
) -> Option<&'a mut serde_json::Value> {
    match c {
        Command::Put {
            kind: k,
            id: i,
            value,
            ..
        } if *k == kind && i == id => Some(value),
        Command::Batch { commands } => commands.iter_mut().rev().find_map(|c| put_in(c, kind, id)),
        _ => None,
    }
}

/// Sets `key` in an object's `fields` (empty `value`: removes it).
pub(super) fn patch_field(object: &mut serde_json::Value, key: &str, value: &str) {
    let Some(o) = object.as_object_mut() else {
        return;
    };
    let fields = o
        .entry("fields")
        .or_insert_with(|| serde_json::Value::Object(Default::default()));
    if let Some(f) = fields.as_object_mut() {
        if value.is_empty() {
            f.remove(key);
        } else {
            f.insert(key.to_owned(), value.into());
        }
    }
    if o["fields"].as_object().is_some_and(|f| f.is_empty()) {
        o.remove("fields");
    }
}

/// "max temperature; glaze ice" → the cases' indices.
pub(super) fn display_case(model: &Model, names: &str) -> Result<Vec<usize>, String> {
    names
        .split(';')
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(|n| {
            model
                .cases
                .iter()
                .position(|c| c.case.name == n)
                .ok_or_else(|| format!("no weather case \"{n}\" (separate cases with ;)"))
        })
        .collect()
}

pub(super) fn structure_cells(
    form: &mut StructureForm,
    c: &Cells,
    model: &Model,
    name: &str,
) -> Result<(), String> {
    form.structure_type = opt(&c.get(1));
    form.extension = opt(&c.get(2));
    form.orientation_deg = opt_num(&c.get(3), &format!("{name} orientation"))?.unwrap_or(0.0);
    for k in 0..2 {
        form.position[k] = num(&c.get(4 + k), &format!("{name} position"))?;
    }
    form.position[2] = match opt_num(&c.get(6), &format!("{name} z"))? {
        Some(z) => z,
        None => model
            .ground_at(form.position[0], form.position[1])
            .ok_or_else(|| format!("{name}: give z (no ground there)"))?,
    };
    form.description = c
        .get(TableKind::Structures.column("description"))
        .trim()
        .to_owned();
    form.weather_groups = group_list(c, TableKind::Structures, model, name)?;
    // Routes typed: its own, unless they are those of the alignments it
    // stands on (then left to them).
    let col = TableKind::Structures.column("routes");
    if c.changed(col) || c.typed.is_none() {
        let list = route_list(&c.get(col));
        if let Some(r) = list
            .iter()
            .find(|r| !model.line.routes.iter().any(|x| &x.id == *r))
        {
            return Err(format!(
                "{name}: no route {r} (make it in the Routes table)"
            ));
        }
        let mut found = model.line.routes_of(name);
        let mut sorted = list.clone();
        sorted.sort();
        found.sort();
        let own = model
            .line
            .structure(name)
            .is_some_and(|s| !s.routes.is_empty());
        form.routes = if sorted == found && !own {
            Vec::new()
        } else {
            list
        };
    }
    Ok(())
}

/// The weather case groups ticked in a row's "weather groups" cell
/// (joined with `; `); each one of the case file's groups (checked when
/// changed: one renamed later stays as it is).
fn group_list(
    c: &Cells,
    kind: TableKind,
    model: &Model,
    name: &str,
) -> Result<Vec<String>, String> {
    let col = kind.column("weather groups");
    let list: Vec<String> = c
        .get(col)
        .split(';')
        .map(str::trim)
        .filter(|g| !g.is_empty())
        .map(str::to_owned)
        .collect();
    if c.changed(col) || c.typed.is_none() {
        let groups = crate::case_groups::groups(model);
        if let Some(g) = list.iter().find(|g| !groups.iter().any(|x| &x.name == *g)) {
            let names: Vec<&str> = groups.iter().map(|x| x.name.as_str()).collect();
            return Err(format!(
                "{name}: no weather case group \"{g}\" ({}; Criteria › Weather case groups…)",
                if names.is_empty() {
                    "none made yet".to_owned()
                } else {
                    format!("the groups: {}", names.join(", "))
                }
            ));
        }
    }
    Ok(list)
}

pub(super) fn section_cells(
    form: &mut SectionForm,
    c: &Cells,
    model: &Model,
    case: usize,
    name: &str,
) -> Result<(), String> {
    let is_new = c.typed.is_none();
    form.weather_groups = group_list(c, TableKind::Sections, model, name)?;
    form.cable = c.get(1).trim().to_owned();
    let calc = c.get(TableKind::Sections.column("calculation"));
    form.calculation = if calc.trim().is_empty() {
        Default::default()
    } else {
        ohl_line::condition::Calculation::parse(&calc).ok_or_else(|| {
            format!("{name} calculation: \"{calc}\" — e.g. Initial FE, Creep RS, Max Sag FE")
        })?
    };
    if model.library.get(&form.cable).is_none() {
        return Err(format!(
            "{name}: wire type \"{}\" is not in the wire library",
            form.cable
        ));
    }
    form.circuit = c.get(2);
    form.phase_label = c.get(3);
    form.description = c
        .get(TableKind::Sections.column("description"))
        .trim()
        .to_owned();
    let color = c
        .get(TableKind::Sections.column("colour"))
        .trim()
        .to_lowercase();
    form.color = if color.is_empty() {
        None
    } else {
        let hex = color.trim_start_matches('#');
        if hex.len() != 6 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(format!(
                "{name} colour: \"{color}\" — write #rrggbb, e.g. #e4572e"
            ));
        }
        Some(format!("#{hex}"))
    };
    form.system = opt(&c.get(4));
    form.bundle = c
        .get(5)
        .trim()
        .parse()
        .map_err(|_| format!("{name} bundle: \"{}\" is not a number", c.get(5)))?;
    if let Some(s) = opt_num(&c.get(6), &format!("{name} spacing"))? {
        form.spacing = s;
    }
    let col = |n: &str| TableKind::Sections.column(n);
    form.rotation = opt_num(
        &c.get(col("bundle angle °")),
        &format!("{name} bundle angle"),
    )?
    .unwrap_or(0.0);
    let clipped = match c.get(col("status")).trim().to_lowercase().as_str() {
        "in the blocks" | "blocks" | "unclipped" | "rolling" => false,
        "clipped in" | "clipped" => true,
        other => {
            return Err(format!(
                "{name} status: \"{other}\" — write in the blocks or clipped in"
            ));
        }
    };
    if form.stringing.clipped() && (!clipped || c.changed(col("condition"))) {
        form.stringing = form.unclipped(model, case);
    }
    // The points first: a sag in a span needs the spans.
    if is_new || c.changed(col("points")) {
        let points: Vec<SectionPoint> = c
            .get(col("points"))
            .split([',', '➡'])
            .filter(|t| !t.trim().is_empty())
            .map(parse_point)
            .collect::<Result<_, _>>()?;
        if points.len() < 2 {
            return Err(format!("{name}: two points or more"));
        }
        form.set_points(model, case, points);
    }
    // Local winds (roadmap O15), on the spans as they are.
    if is_new || c.changed(col("local winds")) {
        if !is_new && c.changed(col("points")) {
            return Err(format!(
                "{name}: change the points and the local winds one after the other"
            ));
        }
        let spans = form.points.len().saturating_sub(1);
        form.wind_factors = ohl_line::parse_wind_factors(&c.get(col("local winds")), spans)
            .map_err(|e| format!("{name} local winds: {e}"))?;
    }
    let temperature = num(&c.get(col("temp °C")), &format!("{name} temperature"))?;
    if !form.stringing.clipped() {
        let condition = c.get(col("condition")).trim().to_lowercase();
        if c.changed(col("condition")) && !c.changed(col("value")) && !is_new {
            return Err(format!(
                "{name}: give the value (kN or m) for the new condition"
            ));
        }
        let value = num(&c.get(col("value")), &format!("{name} value"))?;
        form.stringing = if condition == "tension" {
            Stringing::Tension {
                horizontal_tension: value * 1000.0,
                temperature,
            }
        } else if let Some(rest) = condition.strip_prefix("sag") {
            let rest = rest.trim_start_matches([',', ' ']);
            let span = if rest.is_empty() || rest.starts_with("ruling") {
                None
            } else {
                let k: usize = rest
                    .trim_start_matches("span")
                    .trim()
                    .parse()
                    .map_err(|_| format!("{name} condition: \"{condition}\""))?;
                Some(k.max(1) - 1)
            };
            Stringing::Sag {
                sag: value,
                temperature,
                span,
            }
        } else {
            return Err(format!(
                "{name} condition: \"{condition}\" — write tension, sag, ruling span or sag, span N"
            ));
        };
    } else if let Stringing::UnstressedLengths { temperature: t, .. } = &mut form.stringing {
        *t = temperature;
    }
    if clipped && !form.stringing.clipped() {
        form.clip_in(model)?;
    }
    for (col, measure) in [
        (col("H kN"), Measure::Tension),
        (col("max sag m"), Measure::Sag),
    ] {
        if c.changed(col) {
            let target = num(&c.get(col), &format!("{name} {}", SECTIONS[col].0))?;
            let shown = model.section_case(&form.id, case);
            let weather = model
                .cases
                .get(shown)
                .map(|x| x.case.clone())
                .ok_or("no weather case")?;
            reach_target(form, model, &weather, measure, target, name)?;
        }
    }
    Ok(())
}

/// What a target is.
#[derive(Clone, Copy)]
pub(super) enum Measure {
    /// Horizontal tension of the first span, kN.
    Tension,
    /// Largest sag, m.
    Sag,
}

/// Sets the section's condition (tension or sag) so that `measure` is
/// `target` in `weather`: bracketed bisection on the condition's value
/// (each step one solve of the section); exit clause 60 steps.
pub(super) fn reach_target(
    form: &mut SectionForm,
    model: &Model,
    weather: &ohl_weather::WeatherCase,
    measure: Measure,
    target: f64,
    name: &str,
) -> Result<(), String> {
    if form.stringing.clipped() {
        return Err(format!(
            "{name} is clipped in: a target tension or sag needs it in the blocks"
        ));
    }
    if target.is_nan() || target <= 0.0 {
        return Err(format!("{name}: the target must be above 0"));
    }
    let by_tension = matches!(form.stringing, Stringing::Tension { .. });
    let start = match &form.stringing {
        Stringing::Tension {
            horizontal_tension, ..
        } => *horizontal_tension,
        Stringing::Sag { sag, .. } => *sag,
        Stringing::UnstressedLengths { .. } => unreachable!("checked"),
    };
    let set = |form: &mut SectionForm, v: f64| match &mut form.stringing {
        Stringing::Tension {
            horizontal_tension, ..
        } => *horizontal_tension = v,
        Stringing::Sag { sag, .. } => *sag = v,
        Stringing::UnstressedLengths { .. } => {}
    };
    // Result for a condition value; increasing in it when `rising`.
    // With its loads (marker balls…), as it will be solved.
    let old = model.line.sections.iter().find(|s| s.id == form.id);
    let mut eval = |v: f64| -> Result<f64, String> {
        set(form, v);
        let shape =
            ohl_line::solve::solve_one(&model.line, &form.section(old)?, &model.library, weather)
                .map_err(|e| e.to_string())?;
        Ok(match measure {
            Measure::Tension => shape.spans.first().map_or(0.0, |s| s.horizontal_tension) / 1000.0,
            Measure::Sag => shape.spans.iter().map(|s| s.sag).fold(0.0, f64::max),
        })
    };
    // Tension condition: tension rises, sag falls with it; sag condition:
    // the other way round.
    let rising = matches!(measure, Measure::Tension) == by_tension;
    let (mut lo, mut hi) = (start / 1.5, start * 1.5);
    let below = |r: f64| if rising { r < target } else { r > target };
    let mut steps = 0;
    while !below(eval(lo)?) {
        steps += 1;
        lo /= 2.0;
        if steps > 30 {
            return Err(format!("{name}: no condition gives {target}"));
        }
    }
    while below(eval(hi)?) {
        steps += 1;
        hi *= 2.0;
        if steps > 30 {
            return Err(format!("{name}: no condition gives {target}"));
        }
    }
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if below(eval(mid)?) {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo < 1e-6 * hi {
            break;
        }
    }
    let v = 0.5 * (lo + hi);
    // Rounded: 1 N, 1 mm.
    set(
        form,
        if by_tension {
            v.round()
        } else {
            (v * 1000.0).round() / 1000.0
        },
    );
    Ok(())
}

pub(super) fn string_cells(
    x: &mut ohl_line::InsulatorString,
    c: &Cells,
    name: &str,
) -> Result<(), String> {
    x.description = c
        .get(TableKind::Strings.column("description"))
        .trim()
        .to_owned();
    x.label = c.get(1).trim().to_owned();
    x.top = c.get(2).trim().to_owned();
    x.wire = c.get(3).trim().to_owned();
    if x.top.is_empty() || x.wire.is_empty() {
        return Err(format!(
            "{name}: give its top (from) and its wire point (to)"
        ));
    }
    x.weight = opt_num(&c.get(6), &format!("{name} weight"))?.unwrap_or(0.0);
    x.insulator = opt(&c.get(7));
    x.compression = opt_num(&c.get(8), &format!("{name} compression"))?;
    x.strain = match c.get(9).trim().to_lowercase().as_str() {
        "strain" | "yes" | "true" | "1" => true,
        "suspension" | "no" | "false" | "0" | "" => false,
        other => {
            return Err(format!(
                "{name} clamp: \"{other}\" is not suspension or strain"
            ));
        }
    };
    x.wind_area = opt_num(&c.get(10), &format!("{name} wind area"))?.unwrap_or(0.0);
    if x.wind_area.is_nan() || x.wind_area < 0.0 {
        return Err(format!("{name} wind area: must be ≥ 0"));
    }
    // Its limits (kept, not checked); a dimmed one (its insulator's)
    // reads empty: not its own.
    let limit = |col: &str| {
        opt_num(
            &c.get(TableKind::Strings.column(col)),
            &format!("{name} {col}"),
        )
    };
    x.set_limits(ohl_line::StringLimits {
        max_load: limit("max load N")?,
        max_swing_deg: limit("max swing °")?,
        max_swing_across_deg: limit("max across °")?,
        max_swing_along_deg: limit("max along °")?,
    });
    x.limits().check().map_err(|e| format!("{name}: {e}"))?;
    Ok(())
}

/// A jumper's support clamps, in order: "T3 JW at 0.50 (0.10 m); …".
pub(super) fn clamps_text(j: &ohl_line::Jumper) -> String {
    j.supports()
        .iter()
        .map(|c| {
            format!(
                "{} {} at {} ({} m)",
                c.structure_id(j),
                c.wire,
                f(c.at, 3),
                f(c.clamp_length, 3)
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

pub(super) fn jumper_cells(
    j: &mut ohl_line::Jumper,
    c: &Cells,
    model: &Model,
    name: &str,
) -> Result<(), String> {
    j.description = c
        .get(TableKind::Jumpers.column("description"))
        .trim()
        .to_owned();
    j.from = parse_end(&c.get(1), &model.line).map_err(|e| format!("{name} from: {e}"))?;
    j.to = parse_end(&c.get(2), &model.line).map_err(|e| format!("{name} to: {e}"))?;
    j.cable = match opt(&c.get(3)) {
        Some(cable) => cable,
        None => model
            .line
            .jumper_section(j)
            .map(|s| s.cable.clone())
            .ok_or_else(|| format!("{name}: give its wire type"))?,
    };
    j.length = opt_num(&c.get(4), &format!("{name} length"))?;
    j.sag = opt_num(&c.get(5), &format!("{name} sag"))?;
    // A length or sag typed in replaces a riser's tension.
    if j.length.is_some() || j.sag.is_some() {
        j.tension = None;
    }
    j.temperature = opt_num(&c.get(6), &format!("{name} temperature"))?
        .unwrap_or(ohl_line::DEFAULT_STRINGING_TEMPERATURE);
    j.bundle = opt_num(&c.get(7), &format!("{name} bundle"))?.map(|v| v as u32);
    j.bundle_spacing = opt_num(&c.get(8), &format!("{name} spacing"))?;
    j.bending_stiffness = opt_num(&c.get(9), &format!("{name} EI"))?;
    j.stiffness_factor = opt_num(&c.get(10), &format!("{name} EI factor"))?;
    j.angle_from_deg = opt_num(&c.get(11), &format!("{name} angle from"))?;
    j.angle_to_deg = opt_num(&c.get(12), &format!("{name} angle to"))?;
    j.rigid = match c.get(13).trim() {
        "" | "no" => false,
        "yes" => true,
        v => return Err(format!("{name} rigid: yes or no, not \"{v}\"")),
    };
    Ok(())
}

/// A colour typed in a cell: `#rrggbb` (lower case), or none when empty.
pub(super) fn parse_color(text: &str, name: &str) -> Result<Option<String>, String> {
    let color = text.trim().to_lowercase();
    if color.is_empty() {
        return Ok(None);
    }
    let hex = color.trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(format!(
            "{name} colour: \"{color}\" — write #rrggbb, e.g. #4c9be8"
        ));
    }
    Ok(Some(format!("#{hex}")))
}

pub(super) fn system_cells(
    s: &mut ohl_line::electrical::System,
    c: &Cells,
    name: &str,
) -> Result<(), String> {
    s.description = c
        .get(TableKind::Systems.column("description"))
        .trim()
        .to_owned();
    s.name = c.get(1);
    s.owner = c.get(TableKind::Systems.column("owner")).trim().to_owned();
    s.number = c.get(TableKind::Systems.column("number")).trim().to_owned();
    s.kind = match c.get(2).trim().to_uppercase().as_str() {
        "AC" => ohl_line::electrical::SystemKind::Ac,
        "DC" => ohl_line::electrical::SystemKind::Dc,
        other => return Err(format!("{name} kind: \"{other}\" is not AC or DC")),
    };
    s.voltage_kv = num(&c.get(3), &format!("{name} voltage"))?;
    s.frequency_hz = opt_num(&c.get(4), &format!("{name} frequency"))?;
    let labels: Vec<String> = c
        .get(5)
        .split([' ', ','])
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .collect();
    // A default shown reads empty: the colour by its place.
    s.color = parse_color(&c.get(7), name)?;
    let old = std::mem::take(&mut s.phases);
    s.phases = labels
        .into_iter()
        .enumerate()
        .map(|(k, label)| {
            let mut p = old.get(k).cloned().unwrap_or_else(|| {
                serde_json::from_value(serde_json::json!({ "label": "" })).expect("a phase")
            });
            p.label = label;
            p
        })
        .collect();
    // Line codes, one per phase in order (fewer: the rest by place; a
    // default shown reads empty).
    let codes: Vec<String> = c.get(6).split_whitespace().map(str::to_owned).collect();
    if codes.len() > s.phases.len() {
        return Err(format!(
            "{name} line codes: {} for {} phases",
            codes.len(),
            s.phases.len()
        ));
    }
    for (k, p) in s.phases.iter_mut().enumerate() {
        p.code = match codes.get(k) {
            None => None,
            Some(code) if ohl_line::electrical::is_line_code(code) => Some(code.clone()),
            Some(code) => {
                return Err(format!(
                    "{name} line code \"{code}\": dots and dashes, e.g. - .- .--"
                ));
            }
        };
    }
    Ok(())
}
