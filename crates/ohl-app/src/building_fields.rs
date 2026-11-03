// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Electrical › **Buildings (fields)…** (owner, 2026-11-02, and
//! 2026-11-03: "it should also only offer to find the buildings and then
//! setup the presets for the other field tools. not run them itself"): a
//! plugin with field presets (e.g. the example `building-cuts`) finds the
//! buildings near the line from the point cloud's classes ticked here
//! (and the survey file's codes ticked here) and gives the cuts across the
//! line and the places of sensitive use; **Set up the field tools** hands
//! them to the Magnetic field and Electric field windows
//! ([`crate::field_preset`]) with the field plan template's limits and
//! weather case, where they are run. Every input is kept in the project
//! (`app.building_fields.<line>`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use eframe::egui;
use ohl_electrical::field_plan::{self, FieldPlan};
use serde::{Deserialize, Serialize};

use crate::check_help as help;
use crate::check_ui;
use crate::field_preset::Preset;
use crate::job::Job;
use crate::model::Model;

/// What its results are computed from ([`crate::revision`]).
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::ALL;

/// The project setting the window's setup is kept in, per project line.
pub const SETTING: &str = "app.building_fields";

/// The id of the window's body, tabs and footer.
const WINDOW: &str = "building-fields-window";

/// The setup kept in the project.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    /// The plugin, by name.
    plugin: String,
    /// Its preset ("": its first).
    preset: String,
    /// Its parameters (TOML; "": its template).
    parameters: String,
    /// The field plan template of the project, by file name ("": none).
    plan: String,
    /// The point cloud's classes of the buildings (none: the plugin's
    /// roles, e.g. the scheme's building classes).
    building_classes: Option<Vec<u8>>,
    /// The ground's classes (none: the plugin's ground roles).
    ground_classes: Option<Vec<u8>>,
    /// The survey file's codes of the buildings.
    building_codes: Vec<String>,
    /// The survey file's codes of the ground.
    ground_codes: Vec<String>,
}

/// A building found.
#[derive(Debug, Clone, PartialEq)]
pub struct Building {
    /// Its id.
    pub id: String,
    /// Its ground and top, m.
    pub ground: f64,
    /// Its top, m.
    pub top: f64,
    /// Its area in plan, m².
    pub area: f64,
    /// Its cuts.
    pub cuts: usize,
    /// Its places of sensitive use.
    pub places: usize,
}

/// What a search found.
#[derive(Debug, Clone, Default)]
pub struct Found {
    /// The plugin.
    pub plugin: String,
    /// Every preset it gave.
    pub presets: Vec<ohl_plugins::FieldPreset>,
    /// The weather case the line was solved in.
    pub case: String,
    /// What was read, assumed and left out.
    pub notes: Vec<String>,
}

/// What the window asks of the application: the preset for both field
/// windows, the plan's limits (µT, kV/m) and weather case (index).
#[derive(Debug, Clone)]
pub struct HandOver {
    /// The preset.
    pub preset: Preset,
    /// The limits of the plan.
    pub limits: (Option<f64>, Option<f64>),
    /// The weather case to open the windows on.
    pub case: usize,
}

/// The Buildings (fields) window.
#[derive(Default)]
pub struct BuildingFieldsWindow {
    /// Shown.
    pub open: bool,
    /// The weather case when the plan names none (index in the model's).
    pub case: usize,
    st: Stored,
    kept: check_ui::Kept,
    /// A plugin's parameters template, by plugin name.
    template: Option<(String, Result<String, String>)>,
    /// The plan being edited: its file name and text.
    plan_edit: Option<(String, String)>,
    /// A plan to write into the project (file name, text).
    pub(crate) save_plan: Option<(String, String)>,
    /// The model has no point cloud (said in the setup).
    cloud_missing: bool,
    /// The survey file's codes, by its path.
    codes: Option<(PathBuf, Result<Vec<String>, String>)>,
    /// The preset handed to the field windows (taken by the app).
    pub hand_over: Option<HandOver>,
    pub(crate) job: Job<Found>,
}

impl BuildingFieldsWindow {
    /// Opens the window on the case shown.
    pub fn show(&mut self, case: usize) {
        self.open = true;
        self.case = case;
    }

    /// The last search, and whether one runs.
    #[cfg(test)]
    pub fn found(&self) -> (Option<&Result<Found, String>>, bool) {
        (self.job.result.as_ref(), self.job.running())
    }

    /// Reads the setup from the project when not read yet for the line
    /// worked on, writes it when it changed, and writes a plan saved in
    /// the window into the project's field plans.
    pub fn keep(&mut self, model: &mut Model, ctx: &egui::Context) {
        if let Some((name, text)) = self.save_plan.take() {
            match write_plan(model, &name, &text) {
                Ok(()) => self.st.plan = name,
                Err(e) => self.job.result = Some(Err(format!("field plan {name}: {e}"))),
            }
        }
        let json = serde_json::to_string(&self.st).unwrap_or_default();
        let mut kept = std::mem::take(&mut self.kept);
        if let check_ui::Step::Load(text) =
            kept.step(model, ctx, SETTING, "buildings (fields) setup", &json)
        {
            self.st = text
                .as_deref()
                .and_then(|t| serde_json::from_str(t).ok())
                .unwrap_or_default();
            kept.loaded(serde_json::to_string(&self.st).unwrap_or_default());
        }
        self.kept = kept;
    }

    /// What the last search was computed from that changed since, in
    /// words.
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        matches!(self.job.result, Some(Ok(_)))
            .then(|| self.job.stamp.out_of_date(model, USES))
            .flatten()
    }

    /// The plugin chosen among `plugins` (name, folder): by name, else
    /// the first.
    fn plugin<'a>(&self, plugins: &'a [(String, PathBuf)]) -> Option<&'a (String, PathBuf)> {
        plugins
            .iter()
            .find(|(n, _)| *n == self.st.plugin)
            .or_else(|| plugins.first())
    }

    /// The parameters run with: the kept ones, else the plugin's template.
    fn parameters(&mut self, plugin: &(String, PathBuf)) -> String {
        if !self.st.parameters.is_empty() {
            return self.st.parameters.clone();
        }
        self.template_of(plugin).unwrap_or_default()
    }

    /// The plugin's parameters template, asked once per plugin.
    fn template_of(&mut self, plugin: &(String, PathBuf)) -> Result<String, String> {
        if self.template.as_ref().is_none_or(|(n, _)| *n != plugin.0) {
            let t = ohl_plugins::WasmHost::new()
                .map_err(|e| e.to_string())
                .and_then(|h| {
                    h.field_parameters_template(&plugin.1)
                        .map_err(|e| e.to_string())
                });
            self.template = Some((plugin.0.clone(), t));
        }
        self.template
            .as_ref()
            .map_or(Ok(String::new()), |t| t.1.clone())
    }

    /// The weather case: the plan's when the model has it, else the one
    /// chosen.
    fn case_of(&self, model: &Model, plan: Option<&FieldPlan>) -> usize {
        plan.and_then(|p| {
            let n = &p.weather.case;
            model
                .cases
                .iter()
                .position(|c| !n.is_empty() && &c.case.name == n)
        })
        .unwrap_or(self.case)
    }

    /// Starts a search: the plugin's presets only (no field computed).
    fn start(&mut self, model: &Model, ctx: &egui::Context, plugins: &[(String, PathBuf)]) {
        let Some(plugin) = self.plugin(plugins).cloned() else {
            self.job.result = Some(Err("no plugin with field presets in use".into()));
            return;
        };
        self.st.plugin = plugin.0.clone();
        let mut parameters = self.parameters(&plugin);
        // The survey codes ticked: the plugin's code parameters.
        parameters = set_list(&parameters, "building_codes", &self.st.building_codes);
        parameters = set_list(&parameters, "ground_codes", &self.st.ground_codes);
        let plan = match read_plan(model, &self.st.plan) {
            Ok(p) => p,
            Err(e) => {
                self.job.result = Some(Err(e));
                return;
            }
        };
        let case = self.case_of(model, plan.as_ref().map(|p| &p.0));
        let Some(solved) = model.cases.get(case) else {
            self.job.result = Some(Err("no weather case in the model".into()));
            return;
        };
        let line = model.line.clone();
        let library = model.library.clone();
        let wind = model.wind;
        let unsolved = !solved.solved_for(wind);
        let (mut shapes, mut jumpers) = (solved.shapes.clone(), solved.jumpers.clone());
        let weather = solved.case.clone();
        let cloud = model.cloud.clone();
        let survey = model.sources.terrain.clone();
        let st = self.st.clone();
        self.job.start_at(ctx, model.revision(), move || {
            if unsolved {
                let s = crate::model::Solved::with_wind(&line, &library, weather.clone(), wind);
                shapes = s.shapes;
                jumpers = s.jumpers;
            }
            find(
                &plugin,
                &parameters,
                &st,
                &line,
                &weather,
                &shapes,
                &jumpers,
                cloud.as_deref(),
                survey.as_deref(),
            )
        });
    }

    /// The window. `plugins`: those in use with field presets (name,
    /// folder).
    pub fn ui(&mut self, ctx: &egui::Context, model: &Model, plugins: &[(String, PathBuf)]) {
        self.job.poll();
        self.cloud_missing = model.cloud.is_none();
        let mut again = false;
        let stale = self.out_of_date(model);
        let mut open = self.open;
        let why = if model.cases.is_empty() {
            Some("the model has no weather case".to_owned())
        } else if plugins.is_empty() {
            Some(
                "no plugin with field presets in use (File › Plugins › Plugin manager…)".to_owned(),
            )
        } else {
            None
        };
        let findings = match &self.job.result {
            Some(Ok(f)) => Some(self.chosen(f).map_or(0, |p| p.outlines.len())),
            _ => None,
        };
        help::window(
            ctx,
            crate::dockable::Panel::BuildingFields,
            "Buildings (fields)",
            [900.0, 780.0],
            &mut open,
            |ui| {
                if let Some(what) = &stale {
                    again = crate::revision::banner(ui, what, !self.job.running());
                }
                let tab = check_ui::tabs(
                    ui,
                    WINDOW,
                    findings,
                    self.job.generation,
                    why.is_some() && !self.job.running(),
                );
                match tab {
                    check_ui::Tab::Setup => help::body(ui, WINDOW, |ui| {
                        self.setup_ui(ui, model, plugins);
                    }),
                    check_ui::Tab::Results => check_ui::results_area(ui, WINDOW, |ui| {
                        self.results_ui(ui, model);
                    }),
                }
                help::footer(ui, WINDOW, |ui| {
                    again |= check_ui::job_footer(
                        ui,
                        &mut self.job,
                        why.clone(),
                        "Find the buildings by the plugin: their cuts and places of sensitive \
                         use, to set up the field windows with (no field is computed here)",
                        Some("the whole line".into()),
                        None,
                        |_| {},
                    );
                });
            },
        );
        self.open = open;
        if again && why.is_none() {
            self.start(model, ctx, plugins);
        }
    }

    /// The preset of `found` chosen ("" its first).
    fn chosen<'a>(&self, found: &'a Found) -> Option<&'a ohl_plugins::FieldPreset> {
        if self.st.preset.is_empty() {
            found.presets.first()
        } else {
            found.presets.iter().find(|p| p.name == self.st.preset)
        }
    }

    fn setup_ui(&mut self, ui: &mut egui::Ui, model: &Model, plugins: &[(String, PathBuf)]) {
        check_ui::section(ui, 1, "What it does", None);
        help::explain(
            ui,
            "Finds the buildings near the line by a plugin and sets up the Magnetic field and \
             Electric field windows with their cuts and places of sensitive use; the fields \
             are run there.",
            "The plugin with field presets reads the point cloud's classes ticked below (and \
             the survey file's codes ticked) near the line and the line solved in the weather \
             case, and gives cuts across the line at each building and points at its places of \
             sensitive use. Set up the field tools (in the results) hands them to both field \
             windows, with the field plan template's limits and weather case; each window runs \
             them with its own cuts and points.",
        );
        ui.separator();
        check_ui::section(ui, 2, "Scope", None);
        let names: Vec<String> = model.cases.iter().map(|c| c.case.name.clone()).collect();
        ui.horizontal(|ui| {
            ui.label("The whole line; weather case");
            crate::job::case_combo(ui, "building-fields-case", &names, &mut self.case);
        })
        .response
        .on_hover_text("The case the wires hang in when the field plan names none");
        ui.separator();
        check_ui::section(ui, 3, "Survey data", None);
        self.classes_ui(ui, model, plugins);
        ui.add_space(4.0);
        self.codes_ui(ui, model);
        ui.separator();
        check_ui::section(ui, 4, "Presets", None);
        self.plugin_ui(ui, plugins);
        ui.add_space(4.0);
        self.plan_ui(ui, model);
    }

    /// The point cloud's classes, ticked for the buildings and for the
    /// ground (owner, 2026-11-03: "a tickable list from the survey data
    /// classes. same for the ground").
    fn classes_ui(&mut self, ui: &mut egui::Ui, model: &Model, plugins: &[(String, PathBuf)]) {
        if self.cloud_missing {
            ui.label(
                egui::RichText::new(
                    "No point cloud in the project: the buildings come from the survey codes \
                     only",
                )
                .color(ui.visuals().warn_fg_color),
            );
            return;
        }
        let table = self
            .plugin(plugins)
            .and_then(|p| ohl_plugins::Manifest::load(&p.1).ok())
            .and_then(|m| m.field_input);
        let roles = |r: &[String]| -> Vec<u8> {
            let mut v: Vec<u8> = r
                .iter()
                .flat_map(|role| model.scheme.codes_with_role(role))
                .collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        let (role_b, role_g) = table.as_ref().map_or((Vec::new(), Vec::new()), |t| {
            (roles(&t.roles), roles(&t.ground_roles))
        });
        let classes = cloud_classes(model);
        let mut b: BTreeSet<u8> = self
            .st
            .building_classes
            .clone()
            .unwrap_or(role_b.clone())
            .into_iter()
            .collect();
        let mut g: BTreeSet<u8> = self
            .st
            .ground_classes
            .clone()
            .unwrap_or(role_g.clone())
            .into_iter()
            .collect();
        let (mut cb, mut cg) = (false, false);
        egui::Grid::new("building-fields-classes")
            .striped(true)
            .spacing(help::GRID_SPACING)
            .show(ui, |ui| {
                ui.strong("class");
                ui.strong("buildings");
                ui.strong("ground");
                ui.end_row();
                for (code, name, count) in &classes {
                    ui.label(format!("{code} {name}"))
                        .on_hover_text(match count {
                            Some(n) => format!("{n} points in the cloud"),
                            None => "in the class scheme".to_owned(),
                        });
                    let mut on = b.contains(code);
                    if ui.checkbox(&mut on, "").changed() {
                        if on {
                            b.insert(*code);
                        } else {
                            b.remove(code);
                        }
                        cb = true;
                    }
                    let mut on = g.contains(code);
                    if ui.checkbox(&mut on, "").changed() {
                        if on {
                            g.insert(*code);
                        } else {
                            g.remove(code);
                        }
                        cg = true;
                    }
                    ui.end_row();
                }
            });
        if cb {
            self.st.building_classes = Some(b.into_iter().collect());
        }
        if cg {
            self.st.ground_classes = Some(g.into_iter().collect());
        }
        if (self.st.building_classes.is_some() || self.st.ground_classes.is_some())
            && ui
                .small_button("The plugin's classes")
                .on_hover_text("Back to the classes of the plugin's roles (its manifest)")
                .clicked()
        {
            self.st.building_classes = None;
            self.st.ground_classes = None;
        }
    }

    /// The survey file's codes, ticked for the buildings and the ground.
    fn codes_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        let Some(path) = model.sources.terrain.clone() else {
            ui.label(egui::RichText::new("No survey file: no survey codes").weak());
            return;
        };
        if self.codes.as_ref().is_none_or(|(p, _)| *p != path) {
            let codes = std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| ohl_terrain::read_points(&t).map_err(|e| e.to_string()))
                .map(|pts| {
                    let set: BTreeSet<String> = pts
                        .into_iter()
                        .map(|p| p.code)
                        .filter(|c| !c.is_empty())
                        .collect();
                    set.into_iter().collect()
                });
            self.codes = Some((path.clone(), codes));
        }
        let Some((_, codes)) = &self.codes else {
            return;
        };
        match codes {
            Err(e) => {
                crate::menu::error_label(ui, format!("survey file codes: {e}"));
            }
            Ok(c) if c.is_empty() => {
                ui.label(egui::RichText::new("The survey file has no codes").weak());
            }
            Ok(codes) => {
                let codes = codes.clone();
                egui::Grid::new("building-fields-codes")
                    .striped(true)
                    .spacing(help::GRID_SPACING)
                    .show(ui, |ui| {
                        ui.strong("survey code");
                        ui.strong("buildings");
                        ui.strong("ground");
                        ui.end_row();
                        for code in &codes {
                            ui.label(code);
                            for list in [&mut self.st.building_codes, &mut self.st.ground_codes] {
                                let mut on = list.contains(code);
                                if ui.checkbox(&mut on, "").changed() {
                                    if on {
                                        list.push(code.clone());
                                        list.sort();
                                    } else {
                                        list.retain(|c| c != code);
                                    }
                                }
                            }
                            ui.end_row();
                        }
                    });
            }
        }
    }

    fn plugin_ui(&mut self, ui: &mut egui::Ui, plugins: &[(String, PathBuf)]) {
        // Not blocking: the plugin says what it found without one.
        if self.cloud_missing {
            ui.label(
                egui::RichText::new(
                    "No point cloud in the project: a plugin finding the buildings from its \
                     classes finds none",
                )
                .color(ui.visuals().warn_fg_color),
            );
        }
        let Some(plugin) = self.plugin(plugins).cloned() else {
            ui.label(
                egui::RichText::new(
                    "No plugin with field presets in use: install one (e.g. building-cuts) in \
                     File › Plugins › Plugin manager…",
                )
                .color(ui.visuals().warn_fg_color),
            );
            return;
        };
        ui.horizontal(|ui| {
            ui.label("Plugin");
            egui::ComboBox::from_id_salt("building-fields-plugin")
                .selected_text(&plugin.0)
                .show_ui(ui, |ui| {
                    for (n, _) in plugins {
                        if ui.selectable_label(*n == plugin.0, n).clicked() && *n != plugin.0 {
                            self.st.plugin = n.clone();
                            self.st.parameters.clear();
                        }
                    }
                });
            ui.label("preset");
            ui.add(
                egui::TextEdit::singleline(&mut self.st.preset)
                    .hint_text("its first")
                    .desired_width(120.0),
            );
        });
        let mut text = self.parameters(&plugin);
        egui::CollapsingHeader::new("Parameters")
            .id_salt("building-fields-parameters")
            .default_open(true)
            .show(ui, |ui| {
                if let Err(e) = self.template_of(&plugin) {
                    crate::menu::error_label(ui, &e);
                }
                if params_form(ui, &mut text) {
                    self.st.parameters = text.clone();
                }
                if !self.st.parameters.is_empty() && ui.button("Reset to the plugin's").clicked() {
                    self.st.parameters.clear();
                }
            });
    }

    fn plan_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        let files = model.project_files(field_plan::KIND);
        let names: Vec<String> = files
            .iter()
            .filter_map(|f| f.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        ui.horizontal(|ui| {
            ui.label("Field plan");
            let shown = if self.st.plan.is_empty() {
                "none (the plugin's own drawing)".to_owned()
            } else {
                self.st.plan.clone()
            };
            egui::ComboBox::from_id_salt("building-fields-plan")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.st.plan, String::new(), "none");
                    for n in &names {
                        ui.selectable_value(&mut self.st.plan, n.clone(), n);
                    }
                });
            if ui
                .button("New")
                .on_hover_text("A field plan template with every key and its default")
                .clicked()
            {
                let name = new_name(&names);
                self.plan_edit = Some((name, field_plan::TEMPLATE.to_owned()));
            }
            if !self.st.plan.is_empty()
                && self.plan_edit.is_none()
                && ui.button("Edit").clicked()
                && let Some(f) = files.iter().find(|f| {
                    f.file_name()
                        .is_some_and(|n| n.to_string_lossy() == self.st.plan)
                })
            {
                let text = std::fs::read_to_string(f).unwrap_or_default();
                self.plan_edit = Some((self.st.plan.clone(), text));
            }
        });
        let mut close = false;
        if let Some((name, text)) = &mut self.plan_edit {
            let parsed = FieldPlan::parse(text);
            ui.horizontal(|ui| {
                ui.label("file");
                ui.add(egui::TextEdit::singleline(name).desired_width(200.0));
                let ok = parsed.is_ok() && name.ends_with(&format!(".{}", field_plan::EXTENSION));
                if ui
                    .add_enabled(ok, egui::Button::new("Save into the project"))
                    .clicked()
                {
                    self.save_plan = Some((name.clone(), text.clone()));
                    close = true;
                }
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
            if let Err(e) = &parsed {
                crate::menu::error_label(ui, e);
            }
            egui::ScrollArea::vertical()
                .id_salt("building-fields-plan-text")
                .max_height(260.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(text)
                            .code_editor()
                            .desired_width(f32::INFINITY),
                    );
                });
        }
        if close {
            self.plan_edit = None;
        }
        match read_plan(model, &self.st.plan) {
            Ok(Some((p, _))) => {
                ui.label(egui::RichText::new(plan_words(&p)).small().weak());
            }
            Ok(None) => {}
            Err(e) => {
                crate::menu::error_label(ui, &e);
            }
        }
    }

    fn results_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        let found = match &self.job.result {
            None => {
                ui.label(egui::RichText::new("Not searched yet: Run at the bottom.").weak());
                return;
            }
            Some(Err(e)) => {
                crate::menu::error_label(ui, e);
                return;
            }
            Some(Ok(f)) => f.clone(),
        };
        let Some(preset) = self.chosen(&found).cloned() else {
            crate::menu::error_label(
                ui,
                format!(
                    "no preset {} (the plugin gives: {})",
                    self.st.preset,
                    found
                        .presets
                        .iter()
                        .map(|p| p.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
            return;
        };
        let buildings = buildings_of(&preset);
        check_ui::verdict(
            ui,
            false,
            &format!(
                "{} building{} near the line in \"{}\": {} cuts, {} places of sensitive use \
                 (preset {})",
                buildings.len(),
                if buildings.len() == 1 { "" } else { "s" },
                found.case,
                preset.cuts.len(),
                preset.points.len(),
                preset.name
            ),
            false,
        );
        let plan = read_plan(model, &self.st.plan).ok().flatten();
        ui.horizontal(|ui| {
            if ui
                .button("Set up the field tools")
                .on_hover_text(
                    "Hand the cuts and places to the Magnetic field and Electric field windows \
                     (with the field plan's limits and weather case) and open them; run the \
                     fields there",
                )
                .clicked()
            {
                self.hand_over = Some(HandOver {
                    preset: Preset::from_plugin(&found.plugin, &preset),
                    limits: (
                        plan.as_ref().and_then(|p| p.0.magnetic.limit()),
                        plan.as_ref().and_then(|p| p.0.electric.limit()),
                    ),
                    case: self.case_of(model, plan.as_ref().map(|p| &p.0)),
                });
            }
            if found.presets.len() > 1 {
                ui.label("preset");
                egui::ComboBox::from_id_salt("building-fields-found-preset")
                    .selected_text(&preset.name)
                    .show_ui(ui, |ui| {
                        for p in &found.presets {
                            ui.selectable_value(&mut self.st.preset, p.name.clone(), &p.name);
                        }
                    });
            }
        });
        check_ui::notes_fold(ui, WINDOW, &found.notes);
        use check_ui::{Cell, Col};
        let rows: Vec<check_ui::Row> = buildings
            .iter()
            .map(|b| {
                check_ui::Row::new(
                    vec![
                        Cell::text(&b.id),
                        Cell::num(Some(b.area), 0),
                        Cell::num(Some(b.ground), 2),
                        Cell::num(Some(b.top - b.ground), 1),
                        Cell::num(Some(b.cuts as f64), 0),
                        Cell::num(Some(b.places as f64), 0),
                    ],
                    false,
                )
            })
            .collect();
        check_ui::ResultTable::new(
            "building-fields-buildings",
            vec![
                Col::text("Building"),
                Col::num("Area [m²]"),
                Col::num("Ground [m]"),
                Col::num("Height [m]"),
                Col::num("Cuts"),
                Col::num("Places"),
            ],
        )
        .rows(rows)
        .csv("building-fields-buildings.csv")
        .ui(ui);
    }
}

/// The buildings of a preset: its outlines, with the cuts and places
/// named after them (a cut's label, a place's name before its first `-`).
fn buildings_of(p: &ohl_plugins::FieldPreset) -> Vec<Building> {
    let named = |s: &str, id: &str| s == id || s.split('-').next() == Some(id);
    let mut out: Vec<Building> = p
        .outlines
        .iter()
        .map(|o| {
            let ring: Vec<[f64; 2]> = o.ring.as_chunks::<2>().0.to_vec();
            let area = ring
                .iter()
                .zip(ring.iter().cycle().skip(1))
                .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
                .sum::<f64>()
                .abs()
                / 2.0;
            Building {
                id: o.id.clone(),
                ground: o.ground,
                top: o.top,
                area,
                cuts: p.cuts.iter().filter(|c| named(&c.label, &o.id)).count(),
                places: p.points.iter().filter(|q| named(&q.name, &o.id)).count(),
            }
        })
        .collect();
    // A preset without outlines: its cuts by label.
    if out.is_empty() {
        let labels: BTreeSet<String> = p.cuts.iter().map(|c| c.label.clone()).collect();
        out = labels
            .into_iter()
            .map(|l| Building {
                cuts: p.cuts.iter().filter(|c| c.label == l).count(),
                places: p.points.iter().filter(|q| named(&q.name, &l)).count(),
                id: l,
                ground: f64::NAN,
                top: f64::NAN,
                area: f64::NAN,
            })
            .collect();
    }
    out
}

/// The point cloud's classes to tick: (code, name, points) of the cloud's
/// classes, else the class scheme's.
fn cloud_classes(model: &Model) -> Vec<(u8, String, Option<u64>)> {
    let mut codes: BTreeSet<u8> = model.class_counts.iter().map(|(c, _)| *c).collect();
    codes.extend(model.points.iter().map(|p| p.1));
    if codes.is_empty() {
        codes.extend(model.scheme.classes.iter().map(|c| c.code));
    }
    codes
        .into_iter()
        .map(|c| {
            (
                c,
                model
                    .scheme
                    .class(c)
                    .map_or_else(|| "unknown".to_owned(), |d| d.name.clone()),
                model
                    .class_counts
                    .iter()
                    .find(|(k, _)| *k == c)
                    .map(|(_, n)| *n),
            )
        })
        .collect()
}

/// `text` (the plugin's parameters, TOML) with `key` set to `list` (a
/// TOML array of strings) at its top level: its line replaced (also a
/// line commented out), else added at the top; an empty list leaves the
/// text as it is.
fn set_list(text: &str, key: &str, list: &[String]) -> String {
    if list.is_empty() {
        return text.to_owned();
    }
    let value = format!(
        "{key} = [{}]",
        list.iter()
            .map(|c| format!("{c:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut done = false;
    let mut in_table = false;
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') && !t.starts_with("[[") && !t.contains('=') {
            in_table = true;
        }
        let body = t.strip_prefix('#').map_or(t, str::trim);
        let is_key = body.split_once('=').is_some_and(|(k, _)| k.trim() == key);
        if !in_table && !done && is_key {
            lines.push(value.clone());
            done = true;
        } else {
            lines.push(line.to_owned());
        }
    }
    if !done {
        lines.insert(0, value);
    }
    lines.join("\n") + "\n"
}

/// A new plan's file name not among `names`.
fn new_name(names: &[String]) -> String {
    let ext = field_plan::EXTENSION;
    (1..)
        .map(|k| {
            if k == 1 {
                format!("field-plans.{ext}")
            } else {
                format!("field-plans-{k}.{ext}")
            }
        })
        .find(|n| !names.contains(n))
        .unwrap_or_default()
}

/// The plan of `model` named `name` ("": none), with its JSON for a
/// plugin (its logos read and embedded: a relative path from the plan's
/// folder).
fn read_plan(model: &Model, name: &str) -> Result<Option<(FieldPlan, String)>, String> {
    if name.is_empty() {
        return Ok(None);
    }
    let file = model
        .project_files(field_plan::KIND)
        .into_iter()
        .find(|f| f.file_name().is_some_and(|n| n.to_string_lossy() == name))
        .ok_or_else(|| format!("no field plan {name} in the project"))?;
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{name}: {e}"))?;
    let plan = FieldPlan::parse(&text).map_err(|e| format!("{name}: {e}"))?;
    let mut logos = Vec::new();
    for l in &plan.title.logo {
        let path = file.parent().unwrap_or(Path::new(".")).join(&l.file);
        let bytes = std::fs::read(&path).map_err(|e| format!("logo {}: {e}", path.display()))?;
        logos.push((field_plan::data_uri(&l.file, &bytes)?, l.width));
    }
    let json = plan.plugin_json(&logos);
    Ok(Some((plan, json)))
}

/// A plan in one line: its classes, background, limits and weather.
fn plan_words(p: &FieldPlan) -> String {
    let classes: Vec<String> = p.cut.classes.iter().map(|c| c.class.to_string()).collect();
    let limit = |f: &field_plan::FieldStyle, unit: &str| {
        f.limit()
            .map_or("no limit".to_owned(), |l| format!("limit {l} {unit}"))
    };
    format!(
        "{}: classes {} within {} m of each cut; {}; B {}, E {}; weather {}",
        if p.name.is_empty() { "plan" } else { &p.name },
        if classes.is_empty() {
            "none".to_owned()
        } else {
            classes.join(", ")
        },
        p.cut.band,
        match p.cut.background {
            field_plan::Background::None => "no surface".to_owned(),
            field_plan::Background::Tin if p.cut.surface.is_empty() => {
                "the field's ground along the cut".to_owned()
            }
            field_plan::Background::Tin => format!("surface {} along the cut", p.cut.surface),
            field_plan::Background::Dxf => format!("drawing {} crossing the cut", p.cut.drawing),
        },
        limit(&p.magnetic, "µT"),
        limit(&p.electric, "kV/m"),
        if p.weather.case.is_empty() {
            "the case chosen".to_owned()
        } else {
            p.weather.case.clone()
        } + &if p.weather.systems.is_empty() && p.weather.earth_wires.is_empty() {
            String::new()
        } else {
            format!(
                " ({} system(s) and the earth wires their own)",
                p.weather.systems.len()
            )
        },
    )
}

/// Writes plan `name` into the project's field plans (the working copy,
/// then stored), or beside a line file.
fn write_plan(model: &mut Model, name: &str, text: &str) -> Result<(), String> {
    FieldPlan::parse(text)?;
    if name.contains(['/', '\\']) {
        return Err("a file name without a folder".into());
    }
    let dir = match &model.library_dir {
        Some(w) => w.join(crate::model::library_folder(field_plan::KIND)),
        None => model
            .sources
            .file
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf(),
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(name), text).map_err(|e| e.to_string())?;
    model.store_libraries().map(|_| ())
}

/// The plugin's parameters as a form (owner, 2026-11-02: "all relevant
/// presets"): every `key = value` of the TOML at its top level, with the
/// comment above it as its help; a key commented out (`# key = value`)
/// with a box to set it. Returns whether `text` changed.
fn params_form(ui: &mut egui::Ui, text: &mut String) -> bool {
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let mut changed = false;
    let mut help = String::new();
    egui::Grid::new("building-fields-params")
        .num_columns(3)
        .spacing(help::GRID_SPACING)
        .show(ui, |ui| {
            let mut in_table = false;
            for line in lines.iter_mut() {
                let current = line.clone();
                let t = current.trim();
                if t.starts_with('[') {
                    in_table = true;
                }
                if in_table {
                    continue;
                }
                let (off, body) = match t.strip_prefix('#') {
                    Some(rest) => (true, rest.trim()),
                    None => (false, t),
                };
                let Some((key, value)) = body.split_once('=') else {
                    if off {
                        if !help.is_empty() {
                            help.push(' ');
                        }
                        help.push_str(body);
                    } else {
                        help.clear();
                    }
                    continue;
                };
                let key = key.trim();
                if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    continue;
                }
                let mut set = !off;
                let mut value = value.trim().to_owned();
                let label = ui.label(key);
                if !help.is_empty() {
                    label.on_hover_text(&help);
                }
                let boxed = ui
                    .checkbox(&mut set, "")
                    .on_hover_text("Set (else its default)");
                let edit = ui.add_enabled(
                    set,
                    egui::TextEdit::singleline(&mut value).desired_width(260.0),
                );
                ui.end_row();
                if boxed.changed() || edit.changed() {
                    *line = if set {
                        format!("{key} = {value}")
                    } else {
                        format!("# {key} = {value}")
                    };
                    changed = true;
                }
            }
        });
    if changed {
        *text = lines.join("\n") + "\n";
    }
    changed
}

/// Finds the buildings: gathers the plugin's points (the classes and
/// codes chosen) and takes its presets.
#[allow(clippy::too_many_arguments)]
fn find(
    plugin: &(String, PathBuf),
    parameters: &str,
    st: &Stored,
    line: &ohl_line::Line,
    weather: &ohl_weather::WeatherCase,
    shapes: &[ohl_line::solve::SectionShape],
    jumpers: &[ohl_line::jumper::JumperShape],
    cloud: Option<&str>,
    survey: Option<&Path>,
) -> Result<Found, String> {
    let (name, dir) = plugin;
    let mut notes = Vec::new();
    let host = ohl_plugins::WasmHost::new().map_err(|e| e.to_string())?;
    let manifest = ohl_plugins::Manifest::load(dir).map_err(|e| e.to_string())?;
    let at = |id: &str| {
        line.structure(id)
            .map(|s| [s.position[0], s.position[1]])
            .ok_or_else(|| format!("no structure {id}"))
    };
    let mut spans = Vec::new();
    for s in ohl_electrical::cross_section::line_spans(line) {
        spans.push(vec![at(&s.from)?, at(&s.to)?]);
    }
    let alignments: Vec<(String, f64)> = line
        .alignment_views()
        .iter()
        .map(|v| {
            (
                v.id.clone(),
                ohl_line::route::plan_length(&v.centreline(line)),
            )
        })
        .collect();
    let cloud = match cloud {
        Some(c) => Some(ohl_pointcloud::open_source(c)?),
        None => None,
    };
    let cloud_ref = cloud
        .as_deref()
        .map(|c| c as &dyn ohl_pointcloud::PointSource);
    // Exit clause: decoding is about 1 s per 10⁷ points.
    let budget = ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(600));
    let mut table = manifest.field_input.clone().unwrap_or_default();
    // The survey codes ticked, from the survey file.
    let mut codes: Vec<String> = st.building_codes.clone();
    codes.extend(st.ground_codes.iter().cloned());
    codes.sort();
    codes.dedup();
    table.survey_codes = codes;
    let survey_points = match survey {
        Some(p) if !table.survey_codes.is_empty() => {
            let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
            ohl_terrain::read_points(&text).map_err(|e| format!("{}: {e}", p.display()))?
        }
        _ => Vec::new(),
    };
    let chosen = match (&st.building_classes, &st.ground_classes) {
        (None, None) => None,
        (b, g) => {
            let role = |r: &[String]| -> Vec<u8> {
                let scheme = cloud_ref
                    .and_then(|c| c.class_scheme())
                    .and_then(|t| ohl_lidar::ClassScheme::parse(&t).ok())
                    .unwrap_or_else(ohl_lidar::ClassScheme::asprs);
                r.iter().flat_map(|x| scheme.codes_with_role(x)).collect()
            };
            Some((
                b.clone().unwrap_or_else(|| role(&table.roles)),
                g.clone().unwrap_or_else(|| role(&table.ground_roles)),
            ))
        }
    };
    let gathered = ohl_io::field_input::gather_classes(
        &table,
        chosen.as_ref().map(|(b, g)| (b.as_slice(), g.as_slice())),
        cloud_ref,
        &survey_points,
        &spans,
        &budget,
    )?;
    notes.extend(gathered.notes.iter().map(|n| format!("plugin input: {n}")));
    let model_json = ohl_io::ExportModel::new(
        line,
        &[(
            weather.name.clone(),
            weather.summary(),
            shapes.to_vec(),
            jumpers.to_vec(),
        )],
    )
    .to_json();
    let input = ohl_plugins::FieldInput {
        parameters: parameters.to_owned(),
        model: model_json,
        points: gathered.points,
        ground: gathered.ground,
    };
    let presets = host
        .field_presets(dir, &input, &alignments)
        .map_err(|e| e.to_string())?;
    for p in &presets {
        notes.extend(p.notes.iter().map(|n| format!("plugin ({}): {n}", p.name)));
    }
    Ok(Found {
        plugin: name.clone(),
        presets,
        case: weather.name.clone(),
        notes,
    })
}
