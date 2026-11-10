// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Per-pylon load case sets** (owner 2026-11-03: "on each pylon you
//! should have a load case for structure that can be different per
//! structure and composed of cases from the library"): each structure
//! takes the cases of its own set ([`ohl_line::Structure::load_cases`]),
//! else its type's ([`ohl_line::StructureLoads::type_load_cases`]), else
//! every case of the load case library ([`ohl_line::Line::load_cases_for`])
//! — or none at all ([`ohl_line::Structure::no_load_cases`], owner
//! 2026-11-10: no loads, no checks, said so).
//!
//! - the Structures table's **load cases** column ([`cell`]);
//! - a structure's Properties: **Load cases** — "as its type" or its own
//!   ticks, applied to the other structures selected in one command
//!   ([`structure_ui`]);
//! - Line › Criteria › **Load cases per structure type…**: a tick list per
//!   structure type ([`types_window`]).
//!
//! Every change is one command of the session (one undo).

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use eframe::egui;
use ohl_session::{Command, Kind};

use crate::model::Model;

/// The library file's case names as last read: (file, time, names or why
/// not).
type Names = (PathBuf, Option<SystemTime>, Result<Vec<String>, String>);
static NAMES: Mutex<Option<Names>> = Mutex::new(None);

/// The names of the cases of the load case library file `path` (read
/// again when the file changed).
pub fn names_in(path: &Path) -> Result<Vec<String>, String> {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let mut held = NAMES.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((p, t, names)) = held.as_ref()
        && p == path
        && *t == modified
        && modified.is_some()
    {
        return names.clone();
    }
    let names =
        ohl_loads::LoadCases::load(path).map(|s| s.cases.into_iter().map(|c| c.name).collect());
    *held = Some((path.to_path_buf(), modified, names.clone()));
    names
}

/// The project's load case library's case names (none: no library).
pub fn library_names(model: &Model) -> Result<Vec<String>, String> {
    match &model.sources.load_cases {
        Some(p) => names_in(p),
        None => Err("no load case library (Line › Criteria › Load cases library)".into()),
    }
}

/// Structure `s`'s set in words for the Structures table: the cell (a
/// count and the names, `2: wind, ice`; the type's `type: 2: …`; none:
/// `every case`; a structure taking no load case: [`NO_CASE`]) — its hover
/// shows the whole text.
pub fn cell(line: &ohl_line::Line, s: &ohl_line::Structure) -> String {
    if s.no_load_cases {
        return NO_CASE.to_owned();
    }
    let set = line.load_cases_for(s);
    let names = set.join(", ");
    match (s.load_cases.is_empty(), set.is_empty()) {
        (false, _) => format!("{}: {names}", set.len()),
        (true, false) => format!("type's {}: {names}", set.len()),
        (true, true) => "every case".into(),
    }
}

/// The Structures table's cell of a structure that takes no load case.
pub const NO_CASE: &str = "none: takes no load case";

/// What "none" does, for its hover.
const NO_CASE_HOVER: &str = "This structure takes no load case: no loads are computed for it and      no structure check is made (Checks › Structure loads lists it as \"takes no load case\",      never as passing; OHL-Structure gets no load case of the library for it). The wires are      solved as before: nothing else changes, no solve time.";

/// The command giving each of `ids` the load cases `set` (empty: as its
/// type; `none`: no load case at all, `set` then left out), on the stored
/// line; one command for all (one undo).
pub fn structures_command(
    model: &Model,
    ids: &[String],
    set: &[String],
    none: bool,
) -> Result<(String, Command), String> {
    let set: &[String] = if none { &[] } else { set };
    let stored = model
        .stored_line()
        .ok_or("read-only: open a project to edit")?;
    let mut commands = Vec::new();
    for id in ids {
        let mut s = stored
            .structure(id)
            .ok_or_else(|| format!("no structure {id}"))?
            .clone();
        if s.load_cases == set && s.no_load_cases == none {
            continue;
        }
        s.load_cases = set.to_vec();
        s.no_load_cases = none;
        commands.push(Command::Put {
            kind: Kind::Structure,
            id: id.clone(),
            value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
            ord: None,
        });
    }
    let what = format!(
        "load cases of {}: {}",
        ids.join(", "),
        if none {
            "none".to_owned()
        } else if set.is_empty() {
            "as its type".to_owned()
        } else {
            set.join(", ")
        }
    );
    Ok((
        what,
        match commands.len() {
            1 => commands.remove(0),
            _ => Command::Batch { commands },
        },
    ))
}

/// The structures selected besides `id` (Shift/Cmd+click).
fn others_selected(model: &Model, id: &str) -> Vec<String> {
    model
        .selected_also
        .iter()
        .filter_map(|s| match s {
            crate::selection::Selection::Structure(x) if x != id => Some(x.clone()),
            _ => None,
        })
        .collect()
}

/// A structure's **Load cases** in its Properties: "as its type" or its
/// own ticks of the library's cases, each change one command; with other
/// structures selected, **Apply to the N other selected** gives them the
/// same set (one command).
pub fn structure_ui(ui: &mut egui::Ui, model: &Model, id: &str) -> Option<crate::edit::Action> {
    let line = &model.line;
    let s = line.structure(id)?;
    let mut out = None;
    ui.separator();
    ui.label(egui::RichText::new("Load cases").strong())
        .on_hover_text(
            "The load cases of the library this structure takes (Checks › Structure loads, \
             and OHL-Structure for it): its own, else its type's (Line › Criteria › Load cases \
             per structure type), else every case — or none: no loads and no checks for it, \
             said so in the results. No other cost: a case is solved once for the whole line.",
        );
    let names = match library_names(model) {
        Ok(n) => n,
        Err(e) => {
            ui.label(egui::RichText::new(e).small().weak());
            return None;
        }
    };
    let editable = model.can_edit().is_ok();
    let type_set: Vec<String> = s
        .structure_type
        .as_deref()
        .and_then(|t| line.structure_loads.type_load_cases.get(t))
        .cloned()
        .unwrap_or_default();
    let none = s.no_load_cases;
    let own = !none && !s.load_cases.is_empty();
    let as_type = if type_set.is_empty() {
        "as its type (every case)".to_owned()
    } else {
        format!("as its type ({})", type_set.join(", "))
    };
    // The set chosen, and whether it is "none".
    let mut set: Option<(Vec<String>, bool)> = None;
    ui.add_enabled_ui(editable, |ui| {
        ui.horizontal_wrapped(|ui| {
            if ui
                .radio(!own && !none, as_type)
                .on_hover_text("No set of its own: its type's, else every case")
                .clicked()
                && (own || none)
            {
                set = Some((Vec::new(), false));
            }
            if ui
                .radio(own, "its own")
                .on_hover_text("Tick the cases it takes")
                .clicked()
                && !own
            {
                let cases = if type_set.is_empty() {
                    names.clone()
                } else {
                    type_set.clone()
                };
                set = Some((cases, false));
            }
            if ui
                .radio(none, "none")
                .on_hover_text(NO_CASE_HOVER)
                .clicked()
                && !none
            {
                set = Some((Vec::new(), true));
            }
        });
        if none {
            ui.label(
                egui::RichText::new("Takes no load case: no loads, no structure check.")
                    .small()
                    .weak(),
            )
            .on_hover_text(NO_CASE_HOVER);
        }
        if own {
            egui::ScrollArea::vertical()
                .id_salt(("structure-load-cases", id))
                .max_height(160.0)
                .show(ui, |ui| {
                    let gone: Vec<&String> =
                        s.load_cases.iter().filter(|n| !names.contains(n)).collect();
                    for n in names.iter().chain(gone.iter().copied()) {
                        let mut on = s.load_cases.contains(n);
                        let text = if names.contains(n) {
                            n.clone()
                        } else {
                            format!("{n} (not in the library)")
                        };
                        if ui.checkbox(&mut on, text).changed() {
                            let mut next: Vec<String> = names
                                .iter()
                                .chain(gone.iter().copied())
                                .filter(|x| {
                                    if *x == n {
                                        on
                                    } else {
                                        s.load_cases.contains(x)
                                    }
                                })
                                .cloned()
                                .collect();
                            next.dedup();
                            set = Some((next, false));
                        }
                    }
                });
            if s.load_cases.len() == 1 {
                ui.label(
                    egui::RichText::new("Untick the last one: back to its type's.")
                        .small()
                        .weak(),
                );
            }
        }
        let others = others_selected(model, id);
        if !others.is_empty()
            && ui
                .button(format!("Apply to the {} other selected", others.len()))
                .on_hover_text(format!(
                    "{} take the load cases of {id} ({}), in one command",
                    others.join(", "),
                    if none {
                        "none".to_owned()
                    } else if own {
                        s.load_cases.join(", ")
                    } else {
                        "as their type".to_owned()
                    }
                ))
                .clicked()
        {
            match structures_command(model, &others, &s.load_cases, none) {
                Ok((what, command)) => {
                    out = Some(crate::edit::Action::Command { what, command });
                }
                Err(e) => log::warn!("{e}"),
            }
        }
    })
    .response
    .on_disabled_hover_text(model.can_edit().err().unwrap_or_default());
    if let Some((set, none)) = set {
        match structures_command(model, &[id.to_owned()], &set, none) {
            Ok((what, command)) => out = Some(crate::edit::Action::Command { what, command }),
            Err(e) => log::warn!("{e}"),
        }
    }
    out
}

/// The command setting structure type `t`'s default load cases to `set`
/// (empty: none, every case).
pub fn type_command(model: &Model, t: &str, set: Vec<String>) -> Option<(String, Command)> {
    let e = model.editing.as_ref().ok()?;
    let mut choice = model.line.structure_loads.clone();
    if set.is_empty() {
        choice.type_load_cases.remove(t);
    } else {
        choice.type_load_cases.insert(t.to_owned(), set.clone());
    }
    Some((
        format!(
            "load cases of type {t}: {}",
            if set.is_empty() {
                "every case".to_owned()
            } else {
                set.join(", ")
            }
        ),
        e.session.structure_loads_command(choice),
    ))
}

/// Line › Criteria › **Load cases per structure type…**: per structure
/// type of the line, the library's cases its structures take (none ticked:
/// every case); each tick one command. A structure with its own set
/// keeps it.
pub fn types_window(ctx: &egui::Context, r: &mut crate::app::Ready) {
    if !r.load_case_types_open {
        return;
    }
    let mut open = true;
    let mut action = None;
    egui::Window::new("Load cases per structure type")
        .open(&mut open)
        .default_width(460.0)
        .show(ctx, |ui| {
            action = types_ui(ui, &r.model);
        });
    r.load_case_types_open = open;
    if let Some((what, command)) = action {
        r.perform(crate::edit::Action::Command { what, command });
    }
}

/// The window's body; the command of a tick.
pub fn types_ui(ui: &mut egui::Ui, model: &Model) -> Option<(String, Command)> {
    ui.label(
        egui::RichText::new(
            "Each structure takes the load cases of its own set (Properties › Load cases), else \
             those ticked here for its type, else every case of the library. Structures with a \
             set of their own keep it.",
        )
        .small()
        .weak(),
    );
    let names = match library_names(model) {
        Ok(n) => n,
        Err(e) => {
            crate::menu::error_label(ui, e);
            return None;
        }
    };
    if let Err(why) = model.can_edit() {
        crate::menu::error_label(ui, format!("read-only: {why}"));
    }
    let line = &model.line;
    let mut types: Vec<String> = line
        .structures
        .iter()
        .filter_map(|s| s.structure_type.clone())
        .chain(line.structure_loads.type_load_cases.keys().cloned())
        .collect();
    types.sort();
    types.dedup();
    if types.is_empty() {
        ui.label("No structure of the line has a type.");
        return None;
    }
    let mut out = None;
    ui.add_enabled_ui(model.can_edit().is_ok(), |ui| {
        egui::ScrollArea::vertical()
            .id_salt("load-case-types")
            .max_height(420.0)
            .show(ui, |ui| {
                for t in &types {
                    let set = line
                        .structure_loads
                        .type_load_cases
                        .get(t)
                        .cloned()
                        .unwrap_or_default();
                    let using = line
                        .structures
                        .iter()
                        .filter(|s| s.structure_type.as_deref() == Some(t))
                        .count();
                    let own = line
                        .structures
                        .iter()
                        .filter(|s| {
                            s.structure_type.as_deref() == Some(t)
                                && (!s.load_cases.is_empty() || s.no_load_cases)
                        })
                        .count();
                    let title = format!(
                        "{t} — {} ({using} structure(s){})",
                        if set.is_empty() {
                            "every case".to_owned()
                        } else {
                            format!("{} case(s)", set.len())
                        },
                        if own > 0 {
                            format!(", {own} with their own set")
                        } else {
                            String::new()
                        }
                    );
                    egui::CollapsingHeader::new(title)
                        .id_salt(("load-case-type", t))
                        .default_open(types.len() <= 4)
                        .show(ui, |ui| {
                            let gone: Vec<&String> =
                                set.iter().filter(|n| !names.contains(n)).collect();
                            for n in names.iter().chain(gone.iter().copied()) {
                                let mut on = set.contains(n);
                                let text = if names.contains(n) {
                                    format!("{t}: {n}")
                                } else {
                                    format!("{t}: {n} (not in the library)")
                                };
                                if ui.checkbox(&mut on, text).changed() {
                                    let next: Vec<String> = names
                                        .iter()
                                        .chain(gone.iter().copied())
                                        .filter(|x| if *x == n { on } else { set.contains(x) })
                                        .cloned()
                                        .collect();
                                    out = type_command(model, t, next);
                                }
                            }
                            if !set.is_empty() && ui.small_button("every case").clicked() {
                                out = type_command(model, t, Vec::new());
                            }
                        });
                }
            });
    });
    out
}
