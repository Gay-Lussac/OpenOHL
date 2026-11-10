// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Weather case groups** (owner, 2026-10-25): a name, a description and
//! a few weather cases, kept in the weather case file beside the cases
//! (`[[group]]`, [`ohl_weather::CaseGroup`]); made in Line › Criteria › Weather
//! case groups…, picked as one in Line › Criteria › Loads for structures and in
//! the checks' case lists (a group's button adds its cases). Since
//! 2026-11-03 groups are only that: named lists of cases for picking
//! several at once. The structures' and sections' groups (and the checks'
//! structure and section overrides that read them) gave way to the
//! structures' local conditions and the checks' **Use local conditions**
//! ([`crate::local_solve`]). A group's wind study column
//! (`structure_wind`, `structure_wind_min`) is read and kept, no longer
//! used (2026-11-04): a structure's wind study is its local `wind_study`;
//! Internal clearances offers to copy the columns' values into the
//! structures ([`crate::internal_clearances::GroupStudy`]).

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use ohl_weather::CaseGroup;

use crate::model::Model;

/// The groups read, by the file's path and the libraries' revision
/// ([`crate::revision::Revisions::libraries`]); kept by the model
/// ([`crate::model::ModelKept`]).
pub type KeptGroups = crate::kept::Kept<(PathBuf, u64), Arc<Vec<CaseGroup>>>;

#[cfg(test)]
thread_local! {
    /// How often this thread read a weather case file for its groups.
    static GROUPS_READ: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The groups of the model's weather case file (none without one, or
/// when it does not read).
pub fn groups(model: &Model) -> Vec<CaseGroup> {
    groups_shared(model).as_ref().clone()
}

/// As [`groups`], shared instead of copied (what asks every frame: the
/// checks' case lists, [`crate::check_ui::LineChoices`]). The file is
/// read when the model reads its libraries (owner, 2026-11-10: "refresh
/// on a library change, not every frame" — until then its time on disk
/// was asked on every call) or uses another file; a file changed outside
/// the program shows at the next library change or reload, and when the
/// Weather case groups window is opened ([`groups_now`]).
pub fn groups_shared(model: &Model) -> Arc<Vec<CaseGroup>> {
    let Some(path) = model.sources.weather.as_ref() else {
        return Arc::default();
    };
    model
        .kept
        .case_groups
        .get((path.clone(), model.revisions.libraries), || read(path))
}

/// The groups as the file has them now: read from disk and kept (the
/// user's own action: the Weather case groups window opened).
pub fn groups_now(model: &Model) -> Arc<Vec<CaseGroup>> {
    model.kept.case_groups.forget();
    groups_shared(model)
}

/// The groups of the weather case file `path` (none when it does not
/// read).
fn read(path: &std::path::Path) -> Arc<Vec<CaseGroup>> {
    #[cfg(test)]
    GROUPS_READ.with(|n| n.set(n.get() + 1));
    Arc::new(
        ohl_weather::WeatherCases::load(path)
            .map(|s| s.groups)
            .unwrap_or_default(),
    )
}

/// A group's buttons: the cases of the one clicked added to `chosen`
/// (names); whether it changed. Nothing shown without groups.
pub fn pick_names(ui: &mut egui::Ui, groups: &[CaseGroup], chosen: &mut Vec<String>) -> bool {
    let mut changed = false;
    if groups.is_empty() {
        return false;
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Groups:").small().weak());
        for g in groups {
            let hover = format!(
                "{}{}Adds: {}",
                g.description,
                if g.description.is_empty() { "" } else { "\n" },
                g.cases.join(", ")
            );
            if ui.small_button(&g.name).on_hover_text(hover).clicked() {
                for c in &g.cases {
                    if !chosen.contains(c) {
                        chosen.push(c.clone());
                        changed = true;
                    }
                }
            }
        }
    });
    changed
}

/// As [`pick_names`], the cases chosen by their place in the model.
pub fn pick_indices(
    ui: &mut egui::Ui,
    model: &Model,
    groups: &[CaseGroup],
    chosen: &mut Vec<usize>,
) -> bool {
    let mut names: Vec<String> = chosen
        .iter()
        .filter_map(|k| model.cases.get(*k).map(|c| c.case.name.clone()))
        .collect();
    if !pick_names(ui, groups, &mut names) {
        return false;
    }
    for n in names {
        if let Some(k) = model.cases.iter().position(|c| c.case.name == n)
            && !chosen.contains(&k)
        {
            chosen.push(k);
        }
    }
    chosen.sort_unstable();
    true
}

/// The weather case file's text with `groups` in place of its own.
pub fn with_groups(text: &str, groups: &[CaseGroup]) -> Result<String, String> {
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("{e}"))?;
    doc.remove("group");
    if !groups.is_empty() {
        let mut a = toml_edit::ArrayOfTables::new();
        for g in groups {
            let mut t = toml_edit::Table::new();
            t.insert("name", toml_edit::value(g.name.trim()));
            if !g.description.trim().is_empty() {
                t.insert("description", toml_edit::value(g.description.trim()));
            }
            let mut cases = toml_edit::Array::new();
            for c in &g.cases {
                cases.push(c.as_str());
            }
            t.insert("cases", toml_edit::value(cases));
            if !g.structure_wind.is_empty() {
                t.insert(
                    "structure_wind",
                    toml_edit::value(g.structure_wind.as_str()),
                );
            }
            if let Some(m) = g.structure_wind_min {
                t.insert(
                    "structure_wind_min",
                    toml_edit::value(format!("{} Pa", m.si())),
                );
            }
            a.push(t);
        }
        doc.insert("group", toml_edit::Item::ArrayOfTables(a));
    }
    let out = doc.to_string();
    ohl_weather::WeatherCases::parse(&out)?;
    Ok(out)
}

/// Line › Criteria › Weather case groups…: the groups as edited.
#[derive(Debug, Default)]
pub struct GroupsWindow {
    /// Shown.
    pub open: bool,
    /// The groups as edited (none: read from the file when shown).
    edit: Option<Vec<CaseGroup>>,
    /// Why the last save failed.
    error: Option<String>,
}

impl GroupsWindow {
    /// Opens it on the file's groups.
    pub fn show(&mut self) {
        self.open = true;
        self.edit = None;
        self.error = None;
    }

    /// The window; the file's new text to write (path, text) when saved.
    pub fn ui(&mut self, ctx: &egui::Context, model: &Model) -> Option<(PathBuf, String)> {
        if !self.open {
            return None;
        }
        let names: Vec<String> = model.cases.iter().map(|c| c.case.name.clone()).collect();
        let edit = self
            .edit
            .get_or_insert_with(|| groups_now(model).as_ref().clone());
        let mut open = true;
        let mut save = false;
        let mut remove = None;
        egui::Window::new("Weather case groups")
            .open(&mut open)
            .default_width(460.0)
            .show(ctx, |ui| {
                // The shared spacing (owner, 2026-10-27).
                ui.spacing_mut().item_spacing = crate::check_help::ITEM_SPACING;
                ui.label(
                    egui::RichText::new(
                        "A group: a name, a description and a few weather cases, kept in the \
                         weather case file beside the cases. Picked as one in Line › Criteria › Loads \
                         for structures and in the checks' case lists.",
                    )
                    .small()
                    .weak(),
                );
                if let Err(why) = model.can_edit() {
                    crate::menu::error_label(ui, format!("read-only: {why}"));
                }
                crate::check_help::list(ui, "case-groups", 480.0, egui::ScrollArea::vertical(), |ui| {
                        for (k, g) in edit.iter_mut().enumerate() {
                            ui.push_id(k, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label("Name");
                                    ui.text_edit_singleline(&mut g.name);
                                    if ui.small_button("Remove").clicked() {
                                        remove = Some(k);
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Description");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut g.description)
                                            .desired_width(320.0),
                                    );
                                });
                                egui::CollapsingHeader::new(format!("Cases ({})", g.cases.len()))
                                    .id_salt("cases")
                                    .show(ui, |ui| {
                                        crate::properties::cases_box(ui, "group-cases", |ui| {
                                            for n in &names {
                                                let mut on = g.cases.contains(n);
                                                if ui.checkbox(&mut on, n).changed() {
                                                    g.cases.retain(|x| x != n);
                                                    if on {
                                                        g.cases.push(n.clone());
                                                        g.cases.sort_by_key(|x| {
                                                            names.iter().position(|y| y == x)
                                                        });
                                                    }
                                                }
                                            }
                                            // Names no case has (renamed, deleted).
                                            for gone in g
                                                .cases
                                                .clone()
                                                .iter()
                                                .filter(|x| !names.contains(x))
                                            {
                                                let mut on = true;
                                                if ui
                                                    .checkbox(
                                                        &mut on,
                                                        format!(
                                                            "{gone} (not in the weather cases)"
                                                        ),
                                                    )
                                                    .changed()
                                                {
                                                    g.cases.retain(|x| x != gone);
                                                }
                                            }
                                        });
                                    });
                                ui.separator();
                            });
                        }
                    });
                ui.horizontal(|ui| {
                    if ui.button("New group").clicked() {
                        edit.push(CaseGroup {
                            name: format!("Group {}", edit.len() + 1),
                            description: String::new(),
                            cases: Vec::new(),
                            ..Default::default()
                        });
                    }
                    save = ui
                        .add_enabled(model.can_edit().is_ok(), egui::Button::new("Save"))
                        .on_hover_text("Written into the weather case file (one change)")
                        .clicked();
                });
                if let Some(e) = &self.error {
                    crate::menu::error_label(ui, e);
                }
            });
        if let Some(k) = remove {
            edit.remove(k);
        }
        self.open = open;
        if !save {
            return None;
        }
        let Some(path) = crate::tables::weather_path(model) else {
            self.error = Some("the model has no weather case file".into());
            return None;
        };
        let text = std::fs::read_to_string(&path).unwrap_or_else(|_| {
            format!(
                "format = \"{}\"\nversion = {}\nname = \"Weather cases\"\n",
                ohl_weather::WEATHER_FILE_FORMAT,
                ohl_weather::WEATHER_FILE_VERSION
            )
        });
        match with_groups(&text, edit) {
            Ok(t) => {
                self.error = None;
                Some((path, t))
            }
            Err(e) => {
                self.error = Some(e);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The groups are read once per file and library revision (owner,
    /// 2026-11-10), not on every call: a file changed outside shows when
    /// the libraries are read again, when the window asks, or when the
    /// model uses another file.
    #[test]
    fn groups_are_read_once_per_library_revision() {
        let mut model = crate::tests::synthetic_project("groups-kept");
        let dir = model.sources.file.parent().unwrap().to_path_buf();
        let write = |file: &str, group: &str| {
            let path = dir.join(file);
            std::fs::write(
                &path,
                format!(
                    "format = \"ohl-weather-cases\"\nversion = 1\n[[case]]\nname = \"a\"\n\
                     temperature = \"10 °C\"\n[[group]]\nname = \"{group}\"\ncases = [\"a\"]\n"
                ),
            )
            .unwrap();
            path
        };
        let read = || GROUPS_READ.with(std::cell::Cell::get);
        let name = |m: &Model| groups_shared(m)[0].name.clone();
        model.sources.weather = Some(write("cases.ohlwc", "one"));
        let before = read();
        for _ in 0..4 {
            assert_eq!(name(&model), "one");
        }
        assert_eq!(read(), before + 1);
        // Changed outside the program: not looked at on every call.
        write("cases.ohlwc", "two");
        assert_eq!(name(&model), "one");
        assert_eq!(read(), before + 1);
        // The window opened: read now, and kept.
        assert_eq!(groups_now(&model)[0].name, "two");
        assert_eq!(name(&model), "two");
        assert_eq!(read(), before + 2);
        // The libraries changed.
        write("cases.ohlwc", "three");
        model.revisions.libraries += 1;
        assert_eq!(name(&model), "three");
        assert_eq!(name(&model), "three");
        assert_eq!(read(), before + 3);
        // Read again by the model.
        write("cases.ohlwc", "four");
        model.reload_libraries().unwrap();
        assert_eq!(name(&model), "four");
        assert_eq!(read(), before + 4);
        // Another file used.
        model.sources.weather = Some(write("other.ohlwc", "five"));
        assert_eq!(name(&model), "five");
        assert_eq!(name(&model), "five");
        assert_eq!(read(), before + 5);
        // None: no groups, nothing read.
        model.sources.weather = None;
        assert!(groups_shared(&model).is_empty());
        assert_eq!(read(), before + 5);
    }

    /// Groups written into a case file keep its cases and comments, read
    /// back, and are replaced (not added) when saved again; two of one
    /// name refused.
    #[test]
    fn groups_are_written_into_the_case_file() {
        let text = "format = \"ohl-weather-cases\"\nversion = 1\n# kept\n[[case]]\nname = \"a\"\n\
                    temperature = \"10 °C\"\n[[case]]\nname = \"b\"\ntemperature = \"80 °C\"\n";
        let g = |name: &str, cases: &[&str]| CaseGroup {
            name: name.into(),
            description: "for the checks".into(),
            cases: cases.iter().map(|c| (*c).into()).collect(),
            ..Default::default()
        };
        let one = with_groups(text, &[g("hot", &["b"])]).unwrap();
        assert!(one.contains("# kept"));
        let set = ohl_weather::WeatherCases::parse(&one).unwrap();
        assert_eq!(set.cases.len(), 2);
        assert_eq!(set.groups, vec![g("hot", &["b"])]);
        let two = with_groups(&one, &[g("all", &["a", "b"])]).unwrap();
        let set = ohl_weather::WeatherCases::parse(&two).unwrap();
        assert_eq!(set.groups.len(), 1);
        assert_eq!(set.groups[0].name, "all");
        assert!(with_groups(text, &[g("x", &[]), g("x", &[])]).is_err());
        // A wind study's column and least pressure are kept.
        let study = CaseGroup {
            structure_wind: "my-code.wind_study".into(),
            structure_wind_min: Some(ohl_core::units::Stress::from_si(300.0)),
            ..g("short", &["b"])
        };
        let three = with_groups(text, std::slice::from_ref(&study)).unwrap();
        let set = ohl_weather::WeatherCases::parse(&three).unwrap();
        assert_eq!(set.groups, vec![study]);
    }
}
