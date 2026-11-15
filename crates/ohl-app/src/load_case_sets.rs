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
//!   ticking it and in its own list in the same command
//!   ([`rename_command`], 2026-11-14);
//! - the Load cases form's *8 Applies to* and the groups window's own-list
//!   view change which structures take a case through the case's own
//!   list, one command each ([`give_command`], 2026-11-15), said in words
//!   ([`why_words`], [`given_words`]).

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
    let groups = groups_words(sets, &s.id);
    let excluded = sets.excluded_from(&s.id);
    let ruled_out = sets.ruled_out_of(&s.id);
    let mut but = String::new();
    if !excluded.is_empty() {
        but.push_str(&format!(" — {} {EXCLUDED}", excluded.join(", ")));
    }
    if !ruled_out.is_empty() {
        but.push_str(&format!(" — {} {RULED_OUT}", ruled_out.join(", ")));
    }
    let set = sets.of(&s.id);
    let base = match set {
        ohl_line::CaseSet::Every => return NO_GROUP.to_owned(),
        ohl_line::CaseSet::EveryBut(c) => {
            let why = |case: &String| {
                if ruled_out.contains(case) {
                    RULED_OUT
                } else {
                    EXCLUDED
                }
            };
            let named: Vec<String> = c.iter().map(|x| format!("{x} ({})", why(x))).collect();
            return format!("{NO_GROUP_BUT}{}", named.join(", "));
        }
        ohl_line::CaseSet::NoCase => return format!("{NO_CASE} ({groups}){but}"),
        ohl_line::CaseSet::These(c) => format!("{groups}: {}: {}{but}", c.len(), c.join(", ")),
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

/// The Structures table's cell of a structure in no group excluded from
/// some cases begins so (the cases follow).
pub const NO_GROUP_BUT: &str = "no group: every case but ";

/// How an exclusion in a load case's own list is said.
pub const EXCLUDED: &str = "excluded in the own list";

/// How a case a structure does not take because its own list's rules do
/// not hold for it is said (2026-11-15).
pub const RULED_OUT: &str = "its own list's rules do not hold here";

/// Structure `id`'s groups and the load cases' own lists picking it, in
/// words: `suspension, wind (own list)`.
pub fn groups_words(sets: &ohl_line::CaseSets, id: &str) -> String {
    sets.groups_of(id)
        .iter()
        .map(|g| (*g).to_owned())
        .chain(
            sets.picked_for(id)
                .iter()
                .map(|c| format!("{c} (own list)")),
        )
        .collect::<Vec<_>>()
        .join(", ")
}

/// How a structure gets a load case, or why not, in words (the Load
/// cases form's *Applies to*): `picked here`, `in no group: every case`,
/// `group angle by rule (line angle 10–30°)`, `excluded here (else: group
/// angle by rule …)`, `not taken: its groups suspension do not tick it`.
pub fn why_words(line: &ohl_line::Line, why: &ohl_line::CaseWhy) -> String {
    let by = |why: &ohl_line::CaseWhy| by_words(line, why);
    if why.takes {
        match (why.by_groups(), why.picked_here, why.rule_here) {
            (true, true, _) => format!("{}; also picked here", by(why)),
            (_, true, _) => "picked here".to_owned(),
            (true, false, true) => format!("{}; also by this list's rules", by(why)),
            (false, false, true) => "by this list's rules".to_owned(),
            _ => by(why),
        }
    } else if why.ruled_out_here {
        if why.by_groups() {
            format!(
                "not taken: this list's rules do not hold here (else: {})",
                by(why)
            )
        } else {
            "not taken: this list's rules do not hold here".to_owned()
        }
    } else if why.excluded_here {
        if why.by_groups() {
            format!("excluded here (else: {})", by(why))
        } else {
            "excluded here".to_owned()
        }
    } else {
        format!(
            "not taken: its group{} {} {} not tick it",
            if why.groups.len() == 1 { "" } else { "s" },
            why.groups.join(", "),
            if why.groups.len() == 1 { "does" } else { "do" }
        )
    }
}

/// How a structure gets a load case without the case's own list, in
/// words: `in no group: every case`, or its groups ticking it — `group
/// angle by rule (line angle 10–30°)`, `group crossing (picked)`.
pub fn by_words(line: &ohl_line::Line, why: &ohl_line::CaseWhy) -> String {
    let alignment_name = |id: &str| {
        line.alignments.iter().find(|a| a.id == id).map_or_else(
            || id.to_owned(),
            |a| {
                if a.name.is_empty() {
                    a.id.clone()
                } else {
                    a.name.clone()
                }
            },
        )
    };
    if why.no_group {
        return "in no group: every case".to_owned();
    }
    why.by
        .iter()
        .map(|g| {
            let rules = line
                .structure_groups
                .iter()
                .find(|x| !x.is_own_list() && x.name == g.group)
                .map(|x| x.rules.words(alignment_name))
                .unwrap_or_default();
            match (g.picked, g.rules) {
                (true, true) => format!("group {} (picked; by rule: {rules})", g.group),
                (true, false) => format!("group {} (picked)", g.group),
                _ => format!("group {} by rule ({rules})", g.group),
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// What [`ohl_line::Line::give_case`] did, in words (the command's name
/// and the status line): a structure excluded although a group's rule
/// gives it the case says which rule and what to do.
pub fn given_words(line: &ohl_line::Line, case: &str, given: &ohl_line::Given) -> String {
    let mut out = Vec::new();
    if !given.picked.is_empty() {
        out.push(format!("{} picked", given.picked.join(", ")));
    }
    if !given.unexcluded.is_empty() {
        out.push(format!(
            "{} no longer excluded",
            given.unexcluded.join(", ")
        ));
    }
    if !given.unpicked.is_empty()
        && given
            .unpicked
            .iter()
            .any(|id| !given.excluded.iter().any(|(x, _)| x == id))
    {
        let only: Vec<&str> = given
            .unpicked
            .iter()
            .filter(|id| !given.excluded.iter().any(|(x, _)| x == *id))
            .map(String::as_str)
            .collect();
        out.push(format!("{} taken out of its own list", only.join(", ")));
    }
    for (id, why) in &given.excluded {
        let ruled = why.by.iter().any(|g| g.rules);
        out.push(format!(
            "{id} excluded here (it took it: {}){}",
            by_words(line, why),
            if ruled {
                " — change the rule in Load cases per group, or keep the exclusion"
            } else {
                ""
            }
        ));
    }
    format!("load case \"{case}\": {}", out.join("; "))
}

/// Structures `take` made to take load case `case` and `drop` not to
/// ([`ohl_line::Line::give_case`]: the case's own list, its groups
/// untouched), as one command of the session (one undo); none when
/// nothing changes. What it did in words, and the command.
pub fn give_command(
    model: &Model,
    case: &str,
    take: &[String],
    drop: &[String],
) -> Result<Option<(String, Command)>, String> {
    let e = model
        .editing
        .as_ref()
        .map_err(|e| format!("read-only: {e}"))?;
    let mut line = model.line.clone();
    if let Some(stored) = model.stored_line() {
        line.structure_groups = stored.structure_groups.clone();
    }
    let mut given = line.give_case(case, take, true);
    let dropped = line.give_case(case, drop, false);
    given.unpicked.extend(dropped.unpicked);
    given.excluded.extend(dropped.excluded);
    if given.is_empty() {
        return Ok(None);
    }
    if let Some(p) = line.groups_problem() {
        return Err(p);
    }
    let what = given_words(&model.line, case, &given);
    Ok(Some((
        what,
        e.session.structure_groups_command(line.structure_groups),
    )))
}

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
        ohl_line::CaseSet::EveryBut(c) => format!(
            "In no group: it takes every case of the library but {} (excluded in the case's own \
             list, or its rules do not hold here).",
            c.join(", ")
        ),
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
    // The load cases' own lists (2026-11-15): picked or excluded there.
    let picked = sets.picked_for(id);
    if !picked.is_empty() && !groups.is_empty() {
        ui.label(
            egui::RichText::new(format!(
                "Picked in the own list of {} (Load cases… › Applies to): taken on top of its \
                 groups' cases.",
                picked.join(", ")
            ))
            .small(),
        );
    }
    let excluded = sets.excluded_from(id);
    if !excluded.is_empty() && !groups.is_empty() {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            egui::RichText::new(format!(
                "Excluded from {} in the case's own list (Load cases… › Applies to): it does not \
                 take {} whatever its groups give.",
                excluded.join(", "),
                if excluded.len() == 1 { "it" } else { "them" }
            ))
            .small(),
        );
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
/// order, and its own list when it picks a structure: their structures
/// lose it when it leaves the library.
pub fn takers(line: &ohl_line::Line, case: &str) -> Vec<String> {
    line.structure_groups
        .iter()
        .filter(|g| {
            g.load_cases.iter().any(|c| c == case)
                || (g.load_case.as_deref() == Some(case) && !g.structures.is_empty())
        })
        .map(ohl_line::StructureGroup::label)
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
    // The case's own list follows (2026-11-15), joined to one the new
    // name had (a case once deleted): its picks kept (more cases, the safe
    // side), its exclusions where it is not picked.
    if let Some(k) = groups
        .iter()
        .position(|g| g.load_case.as_deref() == Some(old))
    {
        let mut own = groups.remove(k);
        own.load_case = Some(new.to_owned());
        named.push(format!("{new} (own list)"));
        match groups
            .iter_mut()
            .find(|g| g.load_case.as_deref() == Some(new))
        {
            Some(there) => {
                for id in own.structures {
                    if !there.structures.contains(&id) {
                        there.structures.push(id);
                    }
                }
                for id in own.excluded {
                    if !there.excluded.contains(&id) {
                        there.excluded.push(id);
                    }
                }
                let picked = there.structures.clone();
                there.excluded.retain(|id| !picked.contains(id));
            }
            None => groups.insert(k, own),
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
