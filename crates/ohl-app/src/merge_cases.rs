// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Line › Criteria › Merge duplicate weather cases… (owner, 2026-10-24): the
//! project's weather cases of the same physics
//! ([`ohl_weather::WeatherCase::same_physics`]: what a solve reads, never
//! the name) in groups; each group ticked is merged into the case chosen
//! to keep (the first in the project's order by default). One command,
//! undone as one: the others taken out of the weather case file, and
//! every name of them made the kept case's name —
//!
//! - in the project's library files that name weather cases (rule files'
//!   `cases`, `case_pairs` and tension rules, load cases' `weather`,
//!   galloping files' `cases`, sheet templates' `[cases]`), each name
//!   kept once in a list; nothing else of a file changed (comments,
//!   order and the rest as written);
//! - in the line's settings: the creep-stretch cases and the weather cases
//!   sent to the structures;
//! - and the cases shown and each section's display cases follow.
//!
//! Not changed: the saved performed checks (records of a run as it was
//! made; reopened, one naming a case merged away says it is not in the
//! model) and the plugins' own files. Patterns (`*`, `?`) in a list are
//! left as written.

use std::collections::BTreeMap;

use eframe::egui;
use ohl_weather::WeatherCase;
use toml_edit::{DocumentMut, Item, Value};

use crate::app::Ready;
use crate::model::Model;

/// The project's library kinds whose files may name weather cases.
pub const NAMING_KINDS: [&str; 4] = ["rules", "loadcases", "galloping", "sheet-templates"];

/// Weather cases of the same physics.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// Their names, in the project's order.
    pub names: Vec<String>,
    /// The one kept (index in `names`).
    pub keep: usize,
    /// Merged when applied.
    pub on: bool,
}

/// The groups of two or more of `cases` with the same physics, in the
/// order of their first case; each keeps its first and is ticked.
pub fn groups(cases: &[WeatherCase]) -> Vec<Group> {
    let mut out: Vec<(WeatherCase, Group)> = Vec::new();
    for c in cases {
        match out.iter_mut().find(|(first, _)| first.same_physics(c)) {
            Some((_, g)) => g.names.push(c.name.clone()),
            None => out.push((
                c.clone(),
                Group {
                    names: vec![c.name.clone()],
                    keep: 0,
                    on: true,
                },
            )),
        }
    }
    out.into_iter()
        .map(|(_, g)| g)
        .filter(|g| g.names.len() > 1)
        .collect()
}

/// Each case merged away and the case it becomes, of the groups ticked.
pub fn renames(groups: &[Group]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for g in groups.iter().filter(|g| g.on) {
        let kept = &g.names[g.keep.min(g.names.len() - 1)];
        for n in g.names.iter().filter(|n| *n != kept) {
            out.insert(n.clone(), kept.clone());
        }
    }
    out
}

/// Whether a value under `key` (in a table under `parent`) names weather
/// cases.
fn names_cases(key: &str, parent: &str) -> bool {
    matches!(
        key,
        "cases" | "case_pairs" | "weather" | "creep_case" | "case" | "weather_cases"
    ) || parent == "cases"
}

/// A list's entry as a key to find it again (strings, pairs of strings).
fn entry_key(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.value().clone()),
        Value::Array(a) => a
            .iter()
            .map(|x| x.as_str().map(str::to_owned))
            .collect::<Option<Vec<String>>>()
            .map(|v| v.join("\u{1}")),
        _ => None,
    }
}

fn rename_value(v: &mut Value, key: &str, parent: &str, map: &BTreeMap<String, String>) -> bool {
    match v {
        Value::String(s) => {
            if !names_cases(key, parent) {
                return false;
            }
            let Some(to) = map.get(s.value()) else {
                return false;
            };
            let decor = s.decor().clone();
            let mut new = toml_edit::Formatted::new(to.clone());
            *new.decor_mut() = decor;
            *s = new;
            true
        }
        Value::Array(a) => {
            let mut changed = false;
            for x in a.iter_mut() {
                changed |= rename_value(x, key, parent, map);
            }
            if changed {
                // Each name (or pair) once.
                let mut seen: Vec<String> = Vec::new();
                a.retain(|x| match entry_key(x) {
                    Some(k) if seen.contains(&k) => false,
                    Some(k) => {
                        seen.push(k);
                        true
                    }
                    None => true,
                });
            }
            changed
        }
        Value::InlineTable(t) => {
            let mut changed = false;
            for (k, x) in t.iter_mut() {
                changed |= rename_value(x, k.get(), key, map);
            }
            changed
        }
        _ => false,
    }
}

fn rename_item(item: &mut Item, key: &str, parent: &str, map: &BTreeMap<String, String>) -> bool {
    match item {
        Item::Value(v) => rename_value(v, key, parent, map),
        Item::Table(t) => {
            let mut changed = false;
            for (k, x) in t.iter_mut() {
                changed |= rename_item(x, k.get(), key, map);
            }
            changed
        }
        Item::ArrayOfTables(a) => {
            let mut changed = false;
            for t in a.iter_mut() {
                for (k, x) in t.iter_mut() {
                    changed |= rename_item(x, k.get(), key, map);
                }
            }
            changed
        }
        Item::None => false,
    }
}

/// A library file's text with the weather case names of `map` renamed
/// where it names weather cases; `None`: nothing to change (or not a TOML
/// file).
pub fn rename_text(text: &str, map: &BTreeMap<String, String>) -> Option<String> {
    let mut doc: DocumentMut = text.parse().ok()?;
    let mut changed = false;
    for (k, x) in doc.as_table_mut().iter_mut() {
        changed |= rename_item(x, k.get(), "", map);
    }
    changed.then(|| doc.to_string())
}

/// A weather case file's text without the cases named in `map` (merged
/// away); the rest as written.
pub fn without_cases(text: &str, map: &BTreeMap<String, String>) -> Result<String, String> {
    let mut doc: DocumentMut = text.parse().map_err(|e| format!("{e}"))?;
    if let Some(list) = doc.get_mut("case").and_then(Item::as_array_of_tables_mut) {
        list.retain(|t| {
            t.get("name")
                .and_then(Item::as_str)
                .is_none_or(|n| !map.contains_key(n))
        });
    }
    Ok(doc.to_string())
}

/// `cases` (names) with the names of `map` renamed, each once.
fn renamed_list(cases: &[String], map: &BTreeMap<String, String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in cases {
        let n = map.get(c).unwrap_or(c);
        if !out.contains(n) {
            out.push(n.clone());
        }
    }
    out
}

/// A merge as one command: what it does in words, the command, and the
/// library files it changes (kind, name).
pub type Merge = (String, ohl_session::Command, Vec<(String, String)>);

/// The merge of `map` (each case merged away and the case it becomes) as
/// one command: what it does in words, the command, and the library files
/// it changes (kind, name).
pub fn command(model: &Model, map: &BTreeMap<String, String>) -> Result<Merge, String> {
    if map.is_empty() {
        return Err("nothing to merge".into());
    }
    let e = model
        .editing
        .as_ref()
        .map_err(|e| format!("read-only: {e}"))?;
    let session = &e.session;
    // The weather case file: the project's own.
    let path = model
        .sources
        .weather
        .clone()
        .ok_or("the project has no weather case file")?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or("the weather case file has no name")?;
    let own = session
        .library_files("weather")
        .map_err(|e| e.to_string())?
        .into_iter()
        .any(|(n, _)| n == name)
        && model
            .library_dir
            .as_ref()
            .is_some_and(|d| path.starts_with(d));
    if !own {
        return Err(format!(
            "the weather cases come from {}, not one of the project's files: copy it into \
             the project first (its weather cases library)",
            path.display()
        ));
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut changes = vec![("weather".to_owned(), name, without_cases(&text, map)?)];
    for kind in NAMING_KINDS {
        for (n, t) in session.library_files(kind).map_err(|e| e.to_string())? {
            if let Some(new) = rename_text(&t, map) {
                changes.push((kind.to_owned(), n, new));
            }
        }
    }
    let files: Vec<(String, String)> = changes
        .iter()
        .map(|(k, n, _)| (k.clone(), n.clone()))
        .collect();
    let mut commands = session
        .library_puts(&e.user, &changes)
        .map_err(|e| e.to_string())?;
    // The line's settings naming cases.
    let before = session.line();
    let mut after = before.clone();
    let cs = &mut after.creep_stretch;
    cs.creep_case = cs
        .creep_case
        .as_ref()
        .map(|c| map.get(c).unwrap_or(c).clone());
    cs.load_cases = renamed_list(&cs.load_cases, map);
    let sl = &mut after.structure_loads;
    sl.weather_cases = renamed_list(&sl.weather_cases, map);
    if after.creep_stretch != before.creep_stretch
        || after.structure_loads != before.structure_loads
    {
        commands.push(ohl_session::line_settings_change(before, &after));
    }
    let what = if map.len() == 1 {
        let (a, b) = map.iter().next().expect("one");
        format!("weather case \"{a}\" merged into \"{b}\"")
    } else {
        format!(
            "{} weather cases merged into those of the same weather",
            map.len()
        )
    };
    Ok((what, ohl_session::Command::Batch { commands }, files))
}

/// The window, when open.
#[derive(Debug, Clone, Default)]
pub struct MergeCasesWindow {
    /// Shown.
    pub open: bool,
    /// The groups found when opened (none: found again).
    pub groups: Option<Vec<Group>>,
    /// The last outcome, an error or not.
    pub status: Option<(String, bool)>,
}

/// The cases' indices of `before` (names) in the model's cases now, the
/// names of `map` followed: each once, those gone left out.
fn follow(
    indices: &[usize],
    before: &[String],
    now: &[String],
    map: &BTreeMap<String, String>,
) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    for k in indices {
        let Some(n) = before.get(*k) else { continue };
        let n = map.get(n).unwrap_or(n);
        if let Some(i) = now.iter().position(|x| x == n)
            && !out.contains(&i)
        {
            out.push(i);
        }
    }
    out
}

/// Merges `map` (one command), the cases shown and the sections' display
/// cases following. Returns what was done.
pub fn apply(r: &mut Ready, map: &BTreeMap<String, String>) -> Result<String, String> {
    let (what, command, files) = command(&r.model, map)?;
    let before: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
    let shown = r.model.shown(r.case);
    let display = r.model.display_cases.clone();
    if !r.perform(crate::edit::Action::Command {
        what: what.clone(),
        command,
    }) {
        return Err(r
            .status
            .clone()
            .map_or_else(|| "the merge was refused".into(), |s| s.0));
    }
    let now: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
    r.model.display_cases = display
        .into_iter()
        .map(|(id, ks)| (id, follow(&ks, &before, &now, map)))
        .filter(|(_, ks)| !ks.is_empty())
        .collect();
    let shown = follow(&shown, &before, &now, map);
    r.show_cases(if shown.is_empty() { vec![0] } else { shown });
    let others = files.len() - 1;
    Ok(if others == 0 {
        format!("{what}; no other file named them")
    } else {
        format!(
            "{what}; renamed in {others} other file{} ({})",
            if others == 1 { "" } else { "s" },
            files[1..]
                .iter()
                .map(|(_, n)| n.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

/// Line › Criteria › Merge duplicate weather cases….
pub fn window(ctx: &egui::Context, r: &mut Ready) {
    if !r.merge_cases.open {
        return;
    }
    let cases: Vec<WeatherCase> = r.model.cases.iter().map(|c| c.case.clone()).collect();
    let mut w = std::mem::take(&mut r.merge_cases);
    let groups = w.groups.get_or_insert_with(|| groups(&cases));
    let mut open = true;
    let mut merge = false;
    egui::Window::new("Merge duplicate weather cases")
        .open(&mut open)
        .collapsible(false)
        .default_width(560.0)
        .show(ctx, |ui| {
            // The shared spacing (owner, 2026-10-27).
            ui.spacing_mut().item_spacing = crate::check_help::ITEM_SPACING;
            ui.label(
                egui::RichText::new(
                    "Weather cases with the same weather (temperature, ice, wind and what \
                     else a calculation reads — not the name or description): each group \
                     ticked becomes the case kept, everything naming the others (rule \
                     files, load cases, galloping files, sheet templates, the creep-stretch \
                     cases, the cases sent to the structures) names it instead. One change, \
                     undone as one. Saved check reports stay as they were run.",
                )
                .small()
                .weak(),
            );
            ui.separator();
            if groups.is_empty() {
                ui.label("No two weather cases have the same weather.");
            }
            crate::check_help::list(
                ui,
                "merge-cases",
                360.0,
                egui::ScrollArea::vertical(),
                |ui| {
                    for (k, g) in groups.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut g.on, "");
                            ui.label("keep");
                            egui::ComboBox::from_id_salt(("merge-keep", k))
                                .selected_text(g.names[g.keep].as_str())
                                .show_ui(ui, |ui| {
                                    for (i, n) in g.names.iter().enumerate() {
                                        ui.selectable_value(&mut g.keep, i, n);
                                    }
                                });
                            let others: Vec<String> = g
                                .names
                                .iter()
                                .enumerate()
                                .filter(|(i, _)| *i != g.keep)
                                .map(|(_, n)| format!("\"{n}\""))
                                .collect();
                            ui.label(format!("merges {}", others.join(", ")));
                        });
                    }
                },
            );
            if let Some((m, error)) = &w.status {
                if *error {
                    crate::menu::error_label(ui, m);
                } else {
                    ui.label(egui::RichText::new(m).color(crate::menu::ok_color(ui)));
                }
            }
            ui.separator();
            if let Err(why) = r.model.can_edit() {
                crate::menu::error_label(ui, format!("read-only: {why}"));
            }
            let any = groups.iter().any(|g| g.on);
            merge = ui
                .add_enabled(
                    any && r.model.can_edit().is_ok(),
                    egui::Button::new("Merge"),
                )
                .clicked();
        });
    if merge {
        let map = renames(groups);
        w.status = Some(match apply(r, &map) {
            Ok(m) => (m, false),
            Err(e) => (e, true),
        });
        // Found again from the cases now.
        w.groups = None;
    }
    w.open = open;
    if !open {
        w = MergeCasesWindow::default();
    }
    r.merge_cases = w;
}

#[cfg(test)]
mod tests {
    use super::*;
    use ohl_core::units::Temperature;

    fn case(name: &str, t: f64) -> WeatherCase {
        WeatherCase::still(name, Temperature::from_celsius(t))
    }

    #[test]
    fn groups_by_physics_in_order() {
        let g = groups(&[
            case("a", 10.0),
            case("b", -20.0),
            case("c", 10.0),
            case("d", 80.0),
            case("e", -20.0),
            case("f", 10.0),
        ]);
        let names: Vec<Vec<String>> = g.iter().map(|g| g.names.clone()).collect();
        assert_eq!(names, vec![vec!["a", "c", "f"], vec!["b", "e"]]);
        let mut g = g;
        g[0].keep = 1;
        g[1].on = false;
        let m = renames(&g);
        assert_eq!(
            m.into_iter().collect::<Vec<_>>(),
            vec![("a".into(), "c".into()), ("f".into(), "c".into())]
        );
    }

    /// The names renamed only where cases are named, each once; comments
    /// and the rest kept; a pattern left as written.
    #[test]
    fn files_name_the_kept_case() {
        let map = BTreeMap::from([("10 °C bis".to_owned(), "10 °C".to_owned())]);
        let rules = "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"10 °C bis\"\n\
                     # a comment\n[[rule]]\nname = \"g\"\ntarget = \"same_line\"\nradial = \"3 m\"\ncases = [\"10 °C\", \"10 °C bis\", \"ice*\"]\n\
                     case_pairs = [[\"10 °C bis\", \"x\"], [\"10 °C\", \"x\"]]\n";
        let t = rename_text(rules, &map).unwrap();
        assert!(t.contains("# a comment"), "{t}");
        assert!(
            t.contains("name = \"10 °C bis\""),
            "the file's own name kept: {t}"
        );
        let set = ohl_clearance::RuleSet::parse(&t).unwrap();
        assert_eq!(set.rules[0].cases, ["10 °C", "ice*"]);
        assert_eq!(
            set.rules[0].case_pairs,
            [["10 °C".to_owned(), "x".to_owned()]]
        );
        let loads = "format = \"ohl-load-cases\"\nversion = 1\n[[case]]\nname = \"10 °C bis\"\n\
                     weather = \"10 °C bis\"\n";
        let t = rename_text(loads, &map).unwrap();
        assert!(
            t.contains("name = \"10 °C bis\"") && t.contains("weather = \"10 °C\""),
            "{t}"
        );
        let sheet = "[cases]\nconductors = \"10 °C bis\"\nstringing = \"80 °C\"\n";
        let t = rename_text(sheet, &map).unwrap();
        assert!(t.contains("conductors = \"10 °C\""), "{t}");
        assert!(rename_text("[cases]\nconductors = \"80 °C\"\n", &map).is_none());
        assert!(rename_text("a,b\n1,2\n", &map).is_none(), "not TOML");
        let w = "format = \"ohl-weather-cases\"\nversion = 1\n[[case]]\nname = \"10 °C\"\n\
                 temperature = \"10 °C\"\n[[case]]\nname = \"10 °C bis\"\ntemperature = \"10 °C\"\n";
        let t = without_cases(w, &map).unwrap();
        let names: Vec<String> = ohl_weather::WeatherCases::parse(&t)
            .unwrap()
            .cases
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names, ["10 °C"]);
    }
}

#[cfg(test)]
mod project_tests {
    use super::*;

    /// A project with "10 °C" the same weather as its "everyday": merged
    /// into "everyday" — gone from the weather cases, the load case and
    /// the creep case naming "everyday" — in one command, undone as one.
    #[test]
    fn a_duplicate_is_merged_and_undone_as_one() {
        let mut m = crate::tests::synthetic_project_with("merge-cases", |l| {
            l.creep_stretch.creep_case = Some("10 °C".into());
        });
        let weather = m.sources.weather.clone().unwrap();
        let loads = m.sources.load_cases.clone().unwrap();
        let mut w = std::fs::read_to_string(&weather).unwrap();
        w.push_str("\n[[case]]\nname = \"10 °C\"\ntemperature = \"10 °C\"\n");
        std::fs::write(&weather, &w).unwrap();
        let l = std::fs::read_to_string(&loads)
            .unwrap()
            .replace("weather = \"everyday\"", "weather = \"10 °C\"");
        std::fs::write(&loads, &l).unwrap();
        m.store_libraries().unwrap();
        m.refresh_project_libraries().unwrap();
        let names = |r: &Ready| -> Vec<String> {
            r.model.cases.iter().map(|c| c.case.name.clone()).collect()
        };
        let mut r = Ready::new(m);
        assert!(names(&r).contains(&"10 °C".to_owned()));
        let cases: Vec<WeatherCase> = r.model.cases.iter().map(|c| c.case.clone()).collect();
        let g = groups(&cases);
        assert_eq!(g.len(), 1, "{g:?}");
        assert_eq!(g[0].names, ["everyday", "10 °C"]);
        let done = apply(&mut r, &renames(&g)).unwrap();
        assert!(done.contains("merged into \"everyday\""), "{done}");
        assert!(!names(&r).contains(&"10 °C".to_owned()));
        assert_eq!(
            r.model.line.creep_stretch.creep_case.as_deref(),
            Some("everyday")
        );
        let lc = std::fs::read_to_string(r.model.sources.load_cases.clone().unwrap()).unwrap();
        assert!(!lc.contains("\"10 °C\""), "{lc}");
        // One undo: all of it back.
        r.model.undo().unwrap();
        assert!(names(&r).contains(&"10 °C".to_owned()));
        assert_eq!(
            r.model.line.creep_stretch.creep_case.as_deref(),
            Some("10 °C")
        );
        let lc = std::fs::read_to_string(r.model.sources.load_cases.clone().unwrap()).unwrap();
        assert!(lc.contains("weather = \"10 °C\""), "{lc}");
    }
}
