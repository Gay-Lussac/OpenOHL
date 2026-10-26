// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Weather case groups** (owner, 2026-10-25): a name, a description and
//! a few weather cases, kept in the weather case file beside the cases
//! (`[[group]]`, [`ohl_weather::CaseGroup`]); made in Line › Criteria › Weather
//! case groups…, picked as one in Line › Criteria › Loads for structures and in
//! the checks' case lists (a group's button adds its cases).

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

use eframe::egui;
use ohl_weather::CaseGroup;

use crate::model::Model;

/// The groups read, by file and its time (read again when it changed).
type Cache = Option<(PathBuf, Option<SystemTime>, Vec<CaseGroup>)>;
static CACHE: Mutex<Cache> = Mutex::new(None);

/// The groups of the model's weather case file (none without one, or
/// when it does not read).
pub fn groups(model: &Model) -> Vec<CaseGroup> {
    let Some(path) = model.sources.weather.clone() else {
        return Vec::new();
    };
    let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    let mut cache = CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((p, m, g)) = cache.as_ref()
        && *p == path
        && *m == modified
    {
        return g.clone();
    }
    let g = ohl_weather::WeatherCases::load(&path)
        .map(|s| s.groups)
        .unwrap_or_default();
    *cache = Some((path, modified, g.clone()));
    g
}

/// A check's two ticks (owner, 2026-10-25): the **structure override**
/// and the **section override** — results at a structure or on a section
/// with weather case groups (Structures and Sections tables) take only
/// the check's cases in those groups.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Overrides {
    /// The structures' groups.
    pub structures: bool,
    /// The sections' groups.
    pub sections: bool,
}

impl Overrides {
    /// The two ticks (in a check's footer, beside Run); whether one
    /// changed.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> bool {
        let a = ui
            .checkbox(&mut self.structures, "structure override")
            .on_hover_text(
                "At a structure with weather case groups (Structures table): only this check's \
                 cases in them",
            )
            .changed();
        let b = ui
            .checkbox(&mut self.sections, "section override")
            .on_hover_text(
                "On a section with weather case groups (Sections table): only this check's \
                 cases in them",
            )
            .changed();
        a || b
    }

    /// The filter of the ticks for `model`'s structures and sections.
    pub fn filter(self, model: &Model) -> ohl_weather::CaseFilter {
        let groups = groups(model);
        ohl_weather::CaseFilter {
            structures: self.structures.then(|| {
                ohl_weather::OwnCases::new(
                    model
                        .line
                        .structures
                        .iter()
                        .map(|s| (s.id.as_str(), &s.weather_groups[..])),
                    &groups,
                )
            }),
            sections: self.sections.then(|| {
                ohl_weather::OwnCases::new(
                    model
                        .line
                        .sections
                        .iter()
                        .map(|s| (s.id.as_str(), &s.weather_groups[..])),
                    &groups,
                )
            }),
        }
    }
}

/// Whether the overrides let span `span` of section `section` take
/// weather case `case` (its section's groups and both its ends'; a
/// section not in the line: yes).
pub fn span_takes(
    line: &ohl_line::Line,
    filter: &ohl_weather::CaseFilter,
    section: &str,
    span: usize,
    case: &str,
) -> bool {
    match line.sections.iter().find(|s| s.id == section) {
        Some(sec) => {
            let end = |k: usize| sec.points.get(k).map_or("", |p| p.structure.as_str());
            filter.takes_span(&sec.id, [end(span), end(span + 1)], case)
        }
        None => true,
    }
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
        let edit = self.edit.get_or_insert_with(|| groups(model));
        let mut open = true;
        let mut save = false;
        let mut remove = None;
        egui::Window::new("Weather case groups")
            .open(&mut open)
            .default_width(460.0)
            .show(ctx, |ui| {
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
                egui::ScrollArea::vertical()
                    .max_height(480.0)
                    .show(ui, |ui| {
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
            structure_wind: "lev-ch.wind_study".into(),
            structure_wind_min: Some(ohl_core::units::Stress::from_si(300.0)),
            ..g("short", &["b"])
        };
        let three = with_groups(text, std::slice::from_ref(&study)).unwrap();
        let set = ohl_weather::WeatherCases::parse(&three).unwrap();
        assert_eq!(set.groups, vec![study]);
    }
}
