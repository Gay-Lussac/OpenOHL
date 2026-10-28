// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The rule table every check window with rules shares
//! ([`crate::check_ui::RuleTable`]; owner, 2026-10-28: "keep it standard
//! between all tests"): one compact row per rule — its name, target,
//! voltages, weather cases, distance in words and wires — in a list that
//! can be made taller, and the selected rule's full editor below the
//! table (the window's own fields, the distance editor of
//! [`crate::requirement_ui`] with its preview at the line's voltages).
//! Above it, in this order: **Preset ▾** (one dropdown: the presets of
//! the design-code plugins turned on, grouped by plugin, then the
//! project's generated presets), the preset chosen previewed with
//! **Merge** (a rule of the same name replaced, the others added) and
//! **Replace**; **Import…** and **Export…** (.ohlr); **Add rule**,
//! **Duplicate** and **Remove** (asked first).
//!
//! The table keeps what the user looks at (the row selected, the preset
//! chosen) for the session; the rules are the window's, kept where the
//! window keeps them.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use ohl_clearance::RuleSet;
use ohl_clearance::rules::Requirement;
use ohl_io::codes::RulePresetChecked;

use crate::check_help as help;
use crate::preset_cases::Part;
use crate::rule_editor::{Load, PluginPresets};

/// A rule as a row of a [`RuleTable`].
pub trait RuleRow: Clone {
    /// Its name.
    fn name(&self) -> &str;
    /// Its name, to edit.
    fn name_mut(&mut self) -> &mut String;
    /// The row in words (`voltages`: the line's, for the distance).
    fn summary(&self, voltages: &[f64]) -> Summary;
}

/// A rule's compact row, in words.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Summary {
    /// What it checks against ("ground", "classes 2, 5", "wires of this
    /// line", "every wire").
    pub target: String,
    /// The voltages it applies to ("every voltage", "220 kV, 380 kV").
    pub voltages: String,
    /// Its weather cases ("every case", "ice, hot").
    pub cases: String,
    /// What it requires, in words ("7.5 m", "by voltage: …").
    pub distance: String,
    /// The wires (or parts) it checks.
    pub wires: String,
}

/// "every case", or the cases named.
pub fn cases_words(cases: &[String]) -> String {
    if cases.is_empty() {
        "every case".to_owned()
    } else {
        cases.join(", ")
    }
}

/// "every voltage", or the voltages named ("0 kV (earthed), 220 kV").
pub fn voltages_words(voltages: &[f64]) -> String {
    if voltages.is_empty() {
        "every voltage".to_owned()
    } else {
        voltages
            .iter()
            .map(|v| crate::requirement_ui::voltage_name(*v))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The voltages of a rule ticked among the line's (`all`, kV; none
/// ticked: every voltage), in a menu that stays open; whether they
/// changed.
pub fn voltages_menu(ui: &mut egui::Ui, all: &[f64], chosen: &mut Vec<f64>, hint: &str) -> bool {
    let mut changed = false;
    crate::menu::options_menu(ui, &voltages_words(chosen), |ui| {
        ui.label(egui::RichText::new(hint).small().weak());
        for v in all {
            let mut on = chosen.iter().any(|x| (x - v).abs() < 1e-6);
            if ui
                .checkbox(&mut on, crate::requirement_ui::voltage_name(*v))
                .changed()
            {
                if on {
                    chosen.push(*v);
                    chosen.sort_by(f64::total_cmp);
                } else {
                    chosen.retain(|x| (x - v).abs() >= 1e-6);
                }
                changed = true;
            }
        }
    });
    changed
}

/// A distance in words: "7.5 m", "by voltage: up to 110 kV 5 m, up to
/// 220 kV 7 m", "5 m + 0.01 m/kV above 0 kV"; "not set" for a distance
/// still to fill in.
pub fn distance_words(r: &Requirement) -> String {
    if r.problem().is_some() {
        return "not set".to_owned();
    }
    match r {
        Requirement::Fixed(d) => format!("{d} m"),
        Requirement::ByVoltage(rows) => format!(
            "by voltage: {}",
            rows.iter()
                .map(|(kv, d)| format!("up to {kv} kV {d} m"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Requirement::Formula {
            base,
            per_kv,
            above_kv,
        } => crate::requirement_ui::formula_text(*base, *per_kv, *above_kv),
    }
}

/// Merges rules `from` into `into` (a rule of the same name replaced in
/// its place, the others added at the end) or replaces them; what was
/// done, in words.
pub fn merge<R: RuleRow>(into: &mut Vec<R>, from: Vec<R>, how: Load) -> String {
    match how {
        Load::Replace => {
            let n = from.len();
            *into = from;
            format!("the rules replaced by the preset's {n}")
        }
        Load::Merge => {
            let (mut replaced, mut added) = (0, 0);
            for r in from {
                match into.iter_mut().find(|x| x.name() == r.name()) {
                    Some(x) => {
                        *x = r;
                        replaced += 1;
                    }
                    None => {
                        into.push(r);
                        added += 1;
                    }
                }
            }
            format!("{replaced} rule(s) replaced, {added} added")
        }
    }
}

// ------------------------------------------------------------- presets

/// The name of the project's own group of presets in the dropdown.
pub const PROJECT_GROUP: &str = "This project";

/// The presets of the project (owner, 2026-10-21: a design code writes
/// them when it generates): its rule files — but the check windows' own —
/// with a weather case file of the same name, that `part` lists; (name
/// shown, rule file, weather case file), sorted by file.
pub fn project_preset_files(
    model: &crate::model::Model,
    part: Part,
) -> Vec<(String, PathBuf, PathBuf)> {
    let Some(work) = model.library_dir.as_ref().filter(|_| model.editing.is_ok()) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(work.join("rules")) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| ohl_core::files::has_extension(p, ohl_clearance::rules::RULE_FILE_EXTENSION))
        .collect();
    files.sort();
    let mut out = Vec::new();
    for r in files {
        let name = r
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        if name.ends_with("-clearances.ohlr") {
            continue;
        }
        let stem = r
            .file_stem()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        let w = work
            .join("weather")
            .join(format!("{stem}.{}", ohl_weather::WEATHER_FILE_EXTENSION));
        if !w.is_file() {
            continue;
        }
        let Ok(set) = RuleSet::load(&r) else {
            continue;
        };
        if part.lists(&set) {
            let shown = if set.name.is_empty() {
                name
            } else {
                set.name.clone()
            };
            out.push((shown, r, w));
        }
    }
    out
}

/// A project preset as a plugin's ([`RulePresetChecked`]): its rule file
/// and weather case file read, so it goes into a window the same way.
fn project_preset(
    model: &crate::model::Model,
    name: &str,
    rules: &std::path::Path,
    weather: &std::path::Path,
) -> Result<RulePresetChecked, String> {
    let read = |p: &std::path::Path| {
        std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))
    };
    let rules_text = read(rules)?;
    let weather_text = read(weather)?;
    let set = RuleSet::parse_with(&rules_text, &(&model.line.defaults).into())
        .map_err(|e| format!("{}: {e}", rules.display()))?;
    let wc = ohl_weather::WeatherCases::parse(&weather_text)
        .map_err(|e| format!("{}: {e}", weather.display()))?;
    Ok(RulePresetChecked {
        name: name.to_owned(),
        description: format!(
            "Kept in this project ({})",
            rules
                .file_name()
                .map_or(String::new(), |n| n.to_string_lossy().into_owned())
        ),
        rules_text,
        weather_text,
        rules: set,
        weather: Some(wc),
    })
}

/// The project's presets for `part`, read from its files.
pub fn project_presets_read(model: &crate::model::Model, part: Part) -> Vec<RulePresetChecked> {
    project_preset_files(model, part)
        .into_iter()
        .filter_map(|(name, r, w)| project_preset(model, &name, &r, &w).ok())
        .collect()
}

/// The project's presets for `part`, read once per change of the
/// project's libraries (kept in the session's memory).
fn project_presets(
    ctx: &egui::Context,
    model: &crate::model::Model,
    part: Part,
) -> Arc<Vec<RulePresetChecked>> {
    let key = egui::Id::new((
        "project-presets",
        format!("{part:?}"),
        model.library_dir.clone(),
        model.revisions.libraries,
    ));
    if let Some(list) = ctx.data(|d| d.get_temp::<Arc<Vec<RulePresetChecked>>>(key)) {
        return list;
    }
    let list = Arc::new(project_presets_read(model, part));
    ctx.data_mut(|d| d.insert_temp(key, list.clone()));
    list
}

/// A preset chosen and asked for: its group (a plugin's name, or
/// [`PROJECT_GROUP`]), the preset as its plugin or file gives it (the
/// window takes its part, [`crate::preset_cases::for_project`]), and how.
#[derive(Debug, Clone)]
pub struct PresetPick {
    /// The plugin's name, or [`PROJECT_GROUP`].
    pub group: String,
    /// The preset.
    pub preset: RulePresetChecked,
    /// Merged or replacing the rules.
    pub how: Load,
}

impl PresetPick {
    /// The preset as this check takes it ([`crate::preset_cases::for_project`]:
    /// its `part`, the weather cases its rules name matched to the
    /// project's) and what that does to the cases, line by line.
    pub fn taken(
        &self,
        model: &crate::model::Model,
        part: Part,
    ) -> Result<(RulePresetChecked, Vec<String>), String> {
        crate::preset_cases::for_project(model, &self.preset, part)
            .map(|(p, plan)| (p, plan.lines()))
            .map_err(|e| format!("preset {} {}: {e}", self.group, self.preset.name))
    }

    /// What was done, in words: "preset code roads: 2 rule(s) replaced,
    /// 1 added", then what happened to its weather cases.
    pub fn said(&self, done: &str, lines: &[String]) -> String {
        let mut text = format!("preset {} {}: {done}", self.group, self.preset.name);
        for l in lines {
            text = format!("{text}\n{l}");
        }
        text
    }
}

/// *Import…*: a rule file chosen on the computer.
pub fn import_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(
            "Clearance rules",
            &[ohl_clearance::rules::RULE_FILE_EXTENSION],
        )
        .pick_file()
}

/// *Export…*: where to write the rules (`name` offered).
pub fn export_file(name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(
            "Clearance rules",
            &[ohl_clearance::rules::RULE_FILE_EXTENSION],
        )
        .set_file_name(name)
        .save_file()
}

/// An adapted preset's rules in words, one line each.
type Preview<'a> = dyn Fn(&RuleSet) -> Vec<String> + 'a;

/// What the presets of a [`RuleTable`] offer.
struct Presets<'a> {
    model: &'a crate::model::Model,
    plugins: &'a [PluginPresets],
    part: Part,
    /// The rules of an adapted preset in words, one line each.
    preview: Box<Preview<'a>>,
}

// ----------------------------------------------------------- the table

/// What a [`RuleTable`] keeps from frame to frame (the session only).
#[derive(Debug, Clone, Default)]
struct State {
    /// The rule whose editor is shown.
    selected: Option<usize>,
    /// Remove asked for this rule: waiting for its confirmation.
    confirm: Option<usize>,
    /// The preset chosen in the dropdown (group, preset).
    preset: Option<(usize, usize)>,
}

fn state_key(id: &str) -> egui::Id {
    egui::Id::new(("rule-table", id))
}

/// Chooses preset `k` of group `g` in table `id`'s *Preset ▾* (tests).
#[cfg(test)]
pub fn choose_preset(ctx: &egui::Context, id: &str, g: usize, k: usize) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<State>(state_key(id)).preset = Some((g, k)));
}

/// What the user asked of a [`RuleTable`].
#[derive(Debug, Default)]
pub struct RuleTableOut {
    /// A rule changed, was added, duplicated or removed (keep the rules).
    pub changed: bool,
    /// *Import…* clicked: the window reads a rule file.
    pub import: bool,
    /// *Export…* clicked: the window writes its rules as a rule file.
    pub export: bool,
    /// *Merge* or *Replace* clicked on the preset chosen.
    pub preset: Option<PresetPick>,
}

/// How a rule is added: a button's (or menu entry's) label and hover, and
/// the rule it adds (given the number of rules).
type Adder<'a, R> = (String, String, Box<dyn Fn(usize) -> R + 'a>);

/// The shared rule table (owner, 2026-10-28; see the module's text).
pub struct RuleTable<'a, R: RuleRow> {
    id: &'a str,
    rules: &'a mut Vec<R>,
    voltages: &'a [f64],
    adders: Vec<Adder<'a, R>>,
    keep_one: bool,
    editable: bool,
    presets: Option<Presets<'a>>,
}

/// The longest text of a cell of the table, characters (the whole text
/// on hover).
const CELL_CHARS: usize = 34;

/// `text` cut to [`CELL_CHARS`].
fn cut(text: &str) -> String {
    if text.chars().count() <= CELL_CHARS {
        text.to_owned()
    } else {
        let mut s: String = text.chars().take(CELL_CHARS - 1).collect();
        s.push('…');
        s
    }
}

impl<'a, R: RuleRow> RuleTable<'a, R> {
    /// The table `id` of `rules` (`voltages`: the line's, for the
    /// distances in words).
    pub fn new(id: &'a str, rules: &'a mut Vec<R>, voltages: &'a [f64]) -> Self {
        Self {
            id,
            rules,
            voltages,
            adders: Vec::new(),
            keep_one: false,
            editable: true,
            presets: None,
        }
    }

    /// A way to add a rule: *Add rule* with one, a menu *Add rule ▾* with
    /// several (`label` names each, by target).
    pub fn add(
        mut self,
        label: impl Into<String>,
        hover: impl Into<String>,
        make: impl Fn(usize) -> R + 'a,
    ) -> Self {
        self.adders
            .push((label.into(), hover.into(), Box::new(make)));
        self
    }

    /// The window always keeps a rule (Remove greyed on the last one).
    pub fn keep_one(mut self) -> Self {
        self.keep_one = true;
        self
    }

    /// Read only (a project this user cannot change).
    pub fn editable(mut self, editable: bool) -> Self {
        self.editable = editable;
        self
    }

    /// *Preset ▾*: the presets of `plugins` and of the project that
    /// `part` lists; `preview` gives an adapted preset's rules in words.
    pub fn presets(
        mut self,
        model: &'a crate::model::Model,
        plugins: &'a [PluginPresets],
        part: Part,
        preview: impl Fn(&RuleSet) -> Vec<String> + 'a,
    ) -> Self {
        self.presets = Some(Presets {
            model,
            plugins,
            part,
            preview: Box::new(preview),
        });
        self
    }

    /// The toolbar, the table and, below it, the selected rule's editor:
    /// `editor` draws its fields (the name is the table's), true when one
    /// changed.
    pub fn ui(
        self,
        ui: &mut egui::Ui,
        editor: impl FnOnce(&mut egui::Ui, usize, &mut R) -> bool,
    ) -> RuleTableOut {
        let key = state_key(self.id);
        let mut st: State = ui.data(|d| d.get_temp(key)).unwrap_or_default();
        let mut out = RuleTableOut::default();
        let Self {
            id,
            rules,
            voltages,
            adders,
            keep_one,
            editable,
            presets,
        } = self;
        if rules.is_empty() {
            st.selected = None;
        } else if st.selected.is_none_or(|k| k >= rules.len()) {
            st.selected = Some(0);
        }
        // Presets, then the rule files.
        ui.horizontal_wrapped(|ui| {
            if let Some(p) = &presets {
                ui.add_enabled_ui(editable, |ui| preset_menu(ui, id, p, &mut st));
            }
            out.import = ui
                .add_enabled(editable, egui::Button::new("Import…"))
                .on_hover_text("Rules from a rule file (.ohlr): they replace those listed")
                .clicked();
            out.export = ui
                .add_enabled(!rules.is_empty(), egui::Button::new("Export…"))
                .on_hover_text("The rules as a rule file (.ohlr), for ohl line --rules too")
                .clicked();
        });
        if let Some(p) = &presets
            && let Some(pick) = preset_preview(ui, id, p, &mut st)
        {
            out.preset = Some(pick);
        }
        // Add, duplicate, remove.
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(editable, |ui| {
                let mut made = None;
                match adders.len() {
                    0 => {}
                    1 => {
                        let (label, hover, make) = &adders[0];
                        if ui
                            .button("Add rule")
                            .on_hover_text(format!("{label}: {hover}"))
                            .clicked()
                        {
                            made = Some(make(rules.len()));
                        }
                    }
                    _ => {
                        ui.menu_button("Add rule…", |ui| {
                            for (label, hover, make) in &adders {
                                if ui.button(label).on_hover_text(hover).clicked() {
                                    made = Some(make(rules.len()));
                                    ui.close();
                                }
                            }
                        });
                    }
                }
                if let Some(r) = made {
                    rules.push(r);
                    st.selected = Some(rules.len() - 1);
                    out.changed = true;
                }
                let sel = st.selected.filter(|k| *k < rules.len());
                if ui
                    .add_enabled(sel.is_some(), egui::Button::new("Duplicate"))
                    .on_hover_text("A copy of the rule selected, after it")
                    .clicked()
                    && let Some(k) = sel
                {
                    let mut r = rules[k].clone();
                    r.name_mut().push_str(" (copy)");
                    rules.insert(k + 1, r);
                    st.selected = Some(k + 1);
                    out.changed = true;
                }
                let can_remove = sel.is_some() && !(keep_one && rules.len() <= 1);
                if ui
                    .add_enabled(can_remove, egui::Button::new("Remove"))
                    .on_hover_text("The rule selected (asked first)")
                    .on_disabled_hover_text(if keep_one {
                        "The window always has a rule: change this one"
                    } else {
                        "Select a rule first"
                    })
                    .clicked()
                {
                    st.confirm = sel;
                }
            });
            ui.label(
                egui::RichText::new(format!("{} rule(s)", rules.len()))
                    .small()
                    .weak(),
            );
        });
        if let Some(k) = st.confirm.filter(|k| *k < rules.len()) {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("Remove the rule \"{}\"?", rules[k].name()),
                );
                if ui.button("Remove it").clicked() {
                    rules.remove(k);
                    st.confirm = None;
                    st.selected = (!rules.is_empty()).then(|| k.min(rules.len() - 1));
                    out.changed = true;
                }
                if ui.button("Keep it").clicked() {
                    st.confirm = None;
                }
            });
        } else {
            st.confirm = None;
        }
        if rules.is_empty() {
            ui.label(egui::RichText::new("No rule yet: add one, or take a preset.").weak());
        } else {
            table(
                ui,
                id,
                rules,
                voltages,
                &mut st,
                &mut out,
                editable && !(keep_one && rules.len() <= 1),
            );
        }
        // The rule selected: its editor, below the table.
        if let Some(k) = st.selected.filter(|k| *k < rules.len()) {
            ui.separator();
            let rule = &mut rules[k];
            ui.add_enabled_ui(editable, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(format!("Rule {}", k + 1));
                    let label = ui.label("Rule name");
                    // Kept once typed (not at every key).
                    out.changed |= ui
                        .add(egui::TextEdit::singleline(rule.name_mut()).desired_width(260.0))
                        .labelled_by(label.id)
                        .on_hover_text("Its name, in the results and the reports")
                        .lost_focus();
                });
                ui.push_id((id, "editor", k), |ui| {
                    out.changed |= editor(ui, k, rule);
                });
            });
        }
        ui.data_mut(|d| d.insert_temp(key, st));
        out
    }
}

/// The compact rows: a click on a row selects it (its editor shown below
/// the table); … duplicates or removes it.
fn table<R: RuleRow>(
    ui: &mut egui::Ui,
    id: &str,
    rules: &mut Vec<R>,
    voltages: &[f64],
    st: &mut State,
    out: &mut RuleTableOut,
    can_remove: bool,
) {
    let mut duplicate = None;
    help::list(
        ui,
        (id, "rule-rows"),
        help::LIST_HEIGHT,
        egui::ScrollArea::both()
            .id_salt((id, "rule-rows"))
            .auto_shrink([false, true]),
        |ui| {
            egui::Grid::new((id, "rule-grid"))
                .striped(true)
                .spacing(help::GRID_SPACING)
                .show(ui, |ui| {
                    for h in [
                        "Name",
                        "Target",
                        "Voltages",
                        "Weather cases",
                        "Distance",
                        "Wires",
                        "",
                    ] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for (k, r) in rules.iter().enumerate() {
                        let s = r.summary(voltages);
                        let selected = st.selected == Some(k);
                        let mut hit = ui
                            .selectable_label(selected, cut(r.name()))
                            .on_hover_text(format!("{}: click to edit it below", r.name()))
                            .clicked();
                        for text in [&s.target, &s.voltages, &s.cases, &s.distance, &s.wires] {
                            hit |= ui
                                .add(egui::Label::new(cut(text)).sense(egui::Sense::click()))
                                .on_hover_text(text.as_str())
                                .clicked();
                        }
                        ui.push_id((id, "row-menu", k), |ui| {
                            ui.menu_button("…", |ui| {
                                if ui.button("Duplicate this rule").clicked() {
                                    duplicate = Some(k);
                                    ui.close();
                                }
                                if ui
                                    .add_enabled(can_remove, egui::Button::new("Remove this rule…"))
                                    .clicked()
                                {
                                    st.confirm = Some(k);
                                    ui.close();
                                }
                            })
                            .response
                            .on_hover_text("Duplicate or remove this rule");
                        });
                        if hit {
                            st.selected = Some(k);
                        }
                        ui.end_row();
                    }
                });
        },
    );
    if let Some(k) = duplicate {
        let mut r = rules[k].clone();
        r.name_mut().push_str(" (copy)");
        rules.insert(k + 1, r);
        st.selected = Some(k + 1);
        out.changed = true;
    }
}

/// The groups of the dropdown: each plugin's presets that the part lists,
/// then the project's.
fn groups(ui: &egui::Ui, p: &Presets<'_>) -> Vec<(String, Result<Vec<RulePresetChecked>, String>)> {
    let part = p.part;
    let mut out: Vec<(String, Result<Vec<RulePresetChecked>, String>)> = p
        .plugins
        .iter()
        .map(|pp| {
            (
                pp.plugin.clone(),
                pp.presets.as_ref().map_err(Clone::clone).map(|list| {
                    list.iter()
                        .filter(|x| part.lists(&x.rules))
                        .cloned()
                        .collect()
                }),
            )
        })
        .collect();
    let own = project_presets(ui.ctx(), p.model, part);
    if !own.is_empty() {
        out.push((PROJECT_GROUP.to_owned(), Ok(own.as_ref().clone())));
    }
    out
}

/// *Preset ▾*: one dropdown, the plugins' presets grouped by plugin, then
/// the project's.
fn preset_menu(ui: &mut egui::Ui, id: &str, p: &Presets<'_>, st: &mut State) {
    let groups = groups(ui, p);
    let shown = st
        .preset
        .and_then(|(g, k)| {
            let (name, list) = groups.get(g)?;
            Some(format!("{name}: {}", list.as_ref().ok()?.get(k)?.name))
        })
        .unwrap_or_else(|| "Preset".to_owned());
    egui::ComboBox::from_id_salt((id, "preset"))
        .selected_text(shown)
        .width(260.0)
        .height(420.0)
        .show_ui(ui, |ui| {
            if groups.is_empty() {
                ui.label(
                    egui::RichText::new(
                        "No preset: turn on a design-code plugin (File › Plugins › Plugin \
                         manager), or let a design code generate the project's (Line › Criteria \
                         › Design code)",
                    )
                    .weak(),
                );
            }
            for (g, (name, list)) in groups.iter().enumerate() {
                ui.label(egui::RichText::new(name).strong());
                match list {
                    Err(e) => {
                        crate::menu::error_label(ui, e.as_str());
                    }
                    Ok(list) if list.is_empty() => {
                        ui.label(
                            egui::RichText::new("   no preset for this check")
                                .small()
                                .weak(),
                        );
                    }
                    Ok(list) => {
                        for (k, x) in list.iter().enumerate() {
                            let r = ui
                                .selectable_label(
                                    st.preset == Some((g, k)),
                                    format!("   {}", x.name),
                                )
                                .on_hover_text(if x.description.is_empty() {
                                    x.name.as_str()
                                } else {
                                    x.description.as_str()
                                });
                            if r.clicked() {
                                st.preset = Some((g, k));
                            }
                        }
                    }
                }
            }
        })
        .response
        .on_hover_text(
            "A design code's rules for this check: the plugins' presets, then the project's; \
             previewed below before they are taken",
        );
}

/// The preset chosen, previewed: its rules in words (as they would go in:
/// this check's part, the weather cases matched to the project's), then
/// *Merge*, *Replace* or *Close*.
fn preset_preview(
    ui: &mut egui::Ui,
    id: &str,
    p: &Presets<'_>,
    st: &mut State,
) -> Option<PresetPick> {
    let (g, k) = st.preset?;
    let groups = groups(ui, p);
    let Some((group, Ok(list))) = groups.get(g) else {
        st.preset = None;
        return None;
    };
    let Some(preset) = list.get(k).cloned() else {
        st.preset = None;
        return None;
    };
    let mut pick = None;
    let mut close = false;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.strong(format!("{group}: {}", preset.name));
        if !preset.description.is_empty() {
            crate::menu::message(ui, egui::RichText::new(&preset.description).small(), None);
        }
        let (lines, plan) = match crate::preset_cases::for_project(p.model, &preset, p.part) {
            Ok((adapted, plan)) => ((p.preview)(&adapted.rules), plan.lines()),
            Err(e) => (Vec::new(), vec![format!("its weather cases: {e}")]),
        };
        help::list(
            ui,
            (id, "preset-preview"),
            160.0,
            egui::ScrollArea::vertical()
                .id_salt((id, "preset-preview"))
                .auto_shrink([false, true]),
            |ui| {
                if lines.is_empty() {
                    ui.label(egui::RichText::new("no rule for this check").weak());
                }
                for l in &lines {
                    ui.label(egui::RichText::new(l).small());
                }
            },
        );
        for l in &plan {
            ui.label(egui::RichText::new(l).small().weak());
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Merge")
                .on_hover_text("A rule of the same name replaced, the others added")
                .clicked()
            {
                pick = Some(Load::Merge);
            }
            if ui
                .button("Replace")
                .on_hover_text("The rules listed replaced by the preset's (and its settings)")
                .clicked()
            {
                pick = Some(Load::Replace);
            }
            if ui
                .button("Close")
                .on_hover_text("Leave the rules as they are")
                .clicked()
            {
                close = true;
            }
            ui.label(
                egui::RichText::new(
                    "Check every value against the regulation you work to; your weather cases \
                     of the same weather are used, the others created.",
                )
                .small()
                .weak(),
            );
        });
    });
    if close || pick.is_some() {
        st.preset = None;
    }
    pick.map(|how| PresetPick {
        group: group.clone(),
        preset,
        how,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    #[derive(Debug, Clone, PartialEq)]
    struct R {
        name: String,
        d: f64,
    }

    impl RuleRow for R {
        fn name(&self) -> &str {
            &self.name
        }
        fn name_mut(&mut self) -> &mut String {
            &mut self.name
        }
        fn summary(&self, _: &[f64]) -> Summary {
            Summary {
                target: "ground".into(),
                distance: distance_words(&Requirement::Fixed(self.d)),
                ..Summary::default()
            }
        }
    }

    fn rule(name: &str, d: f64) -> R {
        R {
            name: name.into(),
            d,
        }
    }

    fn draw(ui: &mut egui::Ui, rules: &mut Vec<R>) {
        RuleTable::new("t", rules, &[])
            .add("A rule", "to the ground", |n| {
                rule(&format!("rule {}", n + 1), 1.0)
            })
            .ui(ui, |ui, _, r| {
                ui.label(format!("editing {}", r.name));
                false
            });
    }

    /// Add rule adds a row and selects it (its editor below); Duplicate
    /// copies the rule selected after it; Remove asks first, Keep it
    /// keeps it, Remove it removes it.
    #[test]
    fn a_rule_table_adds_duplicates_and_removes() {
        let mut h = Harness::builder()
            .with_size(egui::vec2(800.0, 600.0))
            .build_ui_state(draw, vec![rule("Ground rule", 6.0)]);
        h.run();
        h.get_by_label("editing Ground rule");
        h.get_by_label("Add rule").click();
        h.run();
        assert_eq!(h.state().len(), 2);
        h.get_by_label("editing rule 2");
        h.get_by_label("Duplicate").click();
        h.run();
        assert_eq!(
            h.state()
                .iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>(),
            ["Ground rule", "rule 2", "rule 2 (copy)"]
        );
        h.get_by_label("editing rule 2 (copy)");
        // The first row clicked: its editor.
        h.get_by_label("Ground rule").click();
        h.run();
        h.get_by_label("editing Ground rule");
        h.get_by_label("Remove").click();
        h.run();
        h.get_by_label("Remove the rule \"Ground rule\"?");
        h.get_by_label("Keep it").click();
        h.run();
        assert_eq!(h.state().len(), 3);
        h.get_by_label("Remove").click();
        h.run();
        h.get_by_label("Remove it").click();
        h.run();
        assert_eq!(
            h.state()
                .iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>(),
            ["rule 2", "rule 2 (copy)"]
        );
    }

    /// Merge replaces a rule of the same name in its place and adds the
    /// others; Replace takes the preset's rules only.
    #[test]
    fn presets_merge_or_replace_the_rules() {
        let mut rules = vec![rule("a", 1.0), rule("b", 2.0)];
        let said = merge(
            &mut rules,
            vec![rule("b", 5.0), rule("c", 3.0)],
            Load::Merge,
        );
        assert_eq!(said, "1 rule(s) replaced, 1 added");
        assert_eq!(rules, vec![rule("a", 1.0), rule("b", 5.0), rule("c", 3.0)]);
        merge(&mut rules, vec![rule("x", 9.0)], Load::Replace);
        assert_eq!(rules, vec![rule("x", 9.0)]);
    }

    #[test]
    fn distances_in_words() {
        assert_eq!(distance_words(&Requirement::Fixed(7.5)), "7.5 m");
        assert_eq!(distance_words(&Requirement::Fixed(0.0)), "not set");
        assert_eq!(
            distance_words(&Requirement::ByVoltage(vec![(110.0, 5.0), (220.0, 7.0)])),
            "by voltage: up to 110 kV 5 m, up to 220 kV 7 m"
        );
        assert_eq!(voltages_words(&[]), "every voltage");
        assert_eq!(cases_words(&["ice".into(), "hot".into()]), "ice, hot");
    }
}
