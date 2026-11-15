// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Which load cases each structure takes**, shown where a structure is
//! listed: since 2026-11-15 (owner: "simply defining load cases and then
//! have them attributed in groups of pylons") the cases of its structure
//! groups ([`ohl_line::Line::case_sets`]: the union of its groups'; in no
//! group, every case of the library; in groups ticking none only, none),
//! made in Line › Structures › Load cases per group…
//! ([`crate::structure_groups`]).
//!
//! - the Structures table's **load cases** column ([`cell`]): its groups
//!   and its cases;
//! - a structure's Properties: **Load cases** — shown only, with a button
//!   to the groups window ([`structure_ui`]);
//! - a case renamed in the Load cases library is renamed in every group
//!   ticking it in the same command ([`rename_command`], 2026-11-14).

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use eframe::egui;
use ohl_session::Command;

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
        None => Err("no load case library (Line › Structures › Load cases…)".into()),
    }
}

/// Structure `s`'s groups and cases in words for the Structures table
/// (`sets`: the line's, resolved once): `angle, dead ends: 3: wind, ice,
/// broken`; in no group `no group: every case`; a structure taking no
/// load case [`NO_CASE`] (with its groups) — its hover shows the whole
/// text. With `known` (the library's case names), cases not there begin
/// with [`UNKNOWN_MARK`] and are named (2026-11-14: drawn in the error
/// colour; not checked in them).
pub fn cell(
    sets: &ohl_line::CaseSets,
    s: &ohl_line::Structure,
    known: Option<&[String]>,
) -> String {
    let groups = sets.groups_of(&s.id).join(", ");
    let set = sets.of(&s.id);
    let base = match set {
        ohl_line::CaseSet::Every => return NO_GROUP.to_owned(),
        ohl_line::CaseSet::NoCase => return format!("{NO_CASE} ({groups})"),
        ohl_line::CaseSet::These(c) => format!("{groups}: {}: {}", c.len(), c.join(", ")),
    };
    let names = set.names();
    let unknown: Vec<&str> = known.map_or_else(Vec::new, |k| {
        names
            .iter()
            .filter(|n| !k.contains(n))
            .map(String::as_str)
            .collect()
    });
    if unknown.is_empty() {
        base
    } else if unknown.len() == names.len() {
        format!("{UNKNOWN_MARK}NOT CHECKED: {base} — not in the library")
    } else {
        format!(
            "{UNKNOWN_MARK}{base} — {} not in the library (not checked in it)",
            unknown.join(", ")
        )
    }
}

/// How a cell of the Structures table whose cases name one not in the
/// library begins (drawn in the error colour).
pub const UNKNOWN_MARK: &str = "⚠ ";

/// The Structures table's cell of a structure that takes no load case.
pub const NO_CASE: &str = "none: takes no load case";

/// The Structures table's cell of a structure in no group.
pub const NO_GROUP: &str = "no group: every case";

/// What "takes no load case" does, for its hover.
pub const NO_CASE_HOVER: &str = "A structure whose groups tick no load case takes none: no \
    loads are computed for it and no structure check is made (Checks › Structure loads lists it \
    as \"takes no load case\", never as passing; OHL-Structure gets no load case of the library \
    for it). The wires are solved as before: nothing else changes, no solve time.";

/// A structure's **Load cases** in its Properties (2026-11-15: shown
/// only): its groups and the cases they give it, and a button opening
/// Line › Structures › Load cases per group…, where they are changed.
pub fn structure_ui(ui: &mut egui::Ui, model: &Model, id: &str) -> Option<crate::edit::Action> {
    let line = &model.line;
    line.structure(id)?;
    ui.separator();
    ui.label(egui::RichText::new("Load cases").strong())
        .on_hover_text(
            "The load cases of the library this structure takes (Checks › Structure loads, \
             and OHL-Structure for it): those of its structure groups (both groups' when it is \
             in two); in no group, every case — or none when its groups tick none. Changed in \
             Line › Structures › Load cases per group….",
        );
    let sets = line.case_sets();
    let groups = sets.groups_of(id);
    let text = match sets.of(id) {
        ohl_line::CaseSet::Every => "In no group: it takes every case of the library.".to_owned(),
        ohl_line::CaseSet::NoCase => format!(
            "Groups {}: they tick no case — it takes none (no loads, no structure check).",
            groups.join(", ")
        ),
        ohl_line::CaseSet::These(c) => format!(
            "Group{} {}: it takes {} case(s): {}.",
            if groups.len() > 1 { "s" } else { "" },
            groups.join(", "),
            c.len(),
            c.join(", ")
        ),
    };
    let r = ui.label(egui::RichText::new(text).small());
    if sets.takes_none(id) {
        r.on_hover_text(NO_CASE_HOVER);
    }
    if let Ok(known) = library_names(model) {
        let gone: Vec<&String> = sets
            .of(id)
            .names()
            .iter()
            .filter(|n| !known.contains(n))
            .collect();
        if !gone.is_empty() {
            crate::menu::error_label(
                ui,
                format!(
                    "{} not in the load case library: not checked in {}",
                    gone.iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    if gone.len() == 1 { "it" } else { "them" }
                ),
            );
        }
    }
    if ui
        .small_button("Change its groups…")
        .on_hover_text("Line › Structures › Load cases per group: the groups and their cases")
        .clicked()
    {
        crate::structure_groups::request_open(ui.ctx(), Some(id));
    }
    None
}

/// The structure groups of `line` ticking load case `case`, in their
/// order: their structures lose it when it leaves the library.
pub fn takers(line: &ohl_line::Line, case: &str) -> Vec<String> {
    line.structure_groups
        .iter()
        .filter(|g| g.load_cases.iter().any(|c| c == case))
        .map(|g| g.name.clone())
        .collect()
}

/// `names` with `old` renamed `new` (each name once).
fn renamed(names: &[String], old: &str, new: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for n in names {
        let n = if n == old { new.to_owned() } else { n.clone() };
        if !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

/// The load case file `text` with its case `old` named `new` (its
/// comments and layout kept). Err: no such case, or `new` taken.
pub fn renamed_case(text: &str, old: &str, new: &str) -> Result<String, String> {
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("{e}"))?;
    let cases = doc
        .get_mut("case")
        .and_then(toml_edit::Item::as_array_of_tables_mut)
        .ok_or("the file has no [[case]]")?;
    if cases
        .iter()
        .any(|c| c.get("name").and_then(toml_edit::Item::as_str) == Some(new))
    {
        return Err(format!("another case is called \"{new}\""));
    }
    let case = cases
        .iter_mut()
        .find(|c| c.get("name").and_then(toml_edit::Item::as_str) == Some(old))
        .ok_or_else(|| format!("no case \"{old}\" in the file"))?;
    case["name"] = toml_edit::value(new);
    Ok(doc.to_string())
}

/// Load case `old` of the project's load case library renamed `new`, as
/// one command (2026-11-14: a rename must not drop structures out of the
/// checks): the library file with the case renamed (comments kept), and
/// every structure group ticking it — undone as one. What it does in
/// words, and the command.
pub fn rename_command(model: &Model, old: &str, new: &str) -> Result<(String, Command), String> {
    let e = model
        .editing
        .as_ref()
        .map_err(|e| format!("read-only: {e}"))?;
    let session = &e.session;
    let path = model
        .sources
        .load_cases
        .clone()
        .ok_or("the project has no load case library")?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or("the load case library has no name")?;
    let stored = session
        .library_files("loadcases")
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .ok_or_else(|| {
            format!(
                "the load cases come from {}, not one of the project's files: copy it into the \
                 project first (its Load cases library)",
                path.display()
            )
        })?;
    let text = renamed_case(&stored, old, new)?;
    let mut commands = session
        .library_puts(&e.user, &[("loadcases".to_owned(), name, text)])
        .map_err(|e| e.to_string())?;
    let before = session.line();
    let mut groups = before.structure_groups.clone();
    let mut named = Vec::new();
    for g in &mut groups {
        if g.load_cases.iter().any(|c| c == old) {
            g.load_cases = renamed(&g.load_cases, old, new);
            named.push(g.name.clone());
        }
    }
    if !named.is_empty() {
        commands.push(session.structure_groups_command(groups));
    }
    let what = format!(
        "load case \"{old}\" renamed \"{new}\"{}",
        if named.is_empty() {
            String::new()
        } else {
            format!(", also in the group(s) {}", named.join(", "))
        }
    );
    Ok((what, Command::Batch { commands }))
}
