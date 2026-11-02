// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Electrical › **Buildings (fields)…** (owner, 2026-11-02: "add
//! a field in the check window with all relevant presets required"): a
//! plugin with field presets (e.g. the example `building-cuts`) finds the
//! buildings near the line from the point cloud's classes and gives the
//! cuts and the places of sensitive use; both fields are run there in one
//! go; a field plan template of the project (`field-plans`,
//! [`ohl_electrical::field_plan`]) gives the limits and isolines, the
//! weather case per system and how the cuts are drawn (title, logos,
//! point classes, a TIN or a drawing along them); the plugin's report is
//! written into a folder one cut at a time. Every input is kept in the
//! project (`app.building_fields.<line>`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use eframe::egui;
use ohl_electrical::field_plan::{self, FieldPlan};
use ohl_electrical::{electric, field_results, magnetic};
use serde::{Deserialize, Serialize};

use crate::check_help as help;
use crate::check_ui;
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
    /// The current per circuit, A.
    currents: BTreeMap<String, f64>,
    /// The earth wires' induced currents.
    earth_wires: bool,
    /// The towers lead the earth wires' currents into the ground.
    towers: bool,
    /// Where the ground comes from.
    ground: crate::field_ground::FieldGround,
    /// The folder the report goes into (none: beside the project).
    out: Option<PathBuf>,
}

/// A place of sensitive use and its values.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    /// Its name (`B007-F2-c1-h4.5`).
    pub name: String,
    /// Height above the ground, m.
    pub height: f64,
    /// B, µT, and the cycle's largest.
    pub b: Option<f64>,
    /// The cycle's largest B, µT.
    pub b_major: Option<f64>,
    /// E, kV/m.
    pub e: Option<f64>,
}

impl Place {
    /// The building of the name (its part before the first `-`).
    pub fn building(&self) -> &str {
        self.name.split('-').next().unwrap_or("")
    }
}

/// What a run found.
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// The weather case the wires hang in (the others per system in the
    /// notes).
    pub case: String,
    /// The plugin's preset run.
    pub preset: String,
    /// Every place.
    pub places: Vec<Place>,
    /// The limits, µT and kV/m.
    pub limits: (Option<f64>, Option<f64>),
    /// The report's files, and the folder.
    pub files: Vec<String>,
    /// Where they are.
    pub out: PathBuf,
    /// What was read, assumed and left out.
    pub notes: Vec<String>,
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
    pub(crate) job: Job<Report>,
}

impl BuildingFieldsWindow {
    /// Opens the window on the case shown; the circuits without a current
    /// take the Magnetic field window's.
    pub fn show(&mut self, case: usize, currents: impl Fn(&str) -> Option<f64>, model: &Model) {
        self.open = true;
        self.case = case;
        for c in magnetic::circuits(&model.line) {
            let label = c.label();
            if !self.st.currents.contains_key(&label)
                && let Some(a) = currents(&label)
            {
                self.st.currents.insert(label, a);
            }
        }
    }

    /// The last run's report, and whether one runs.
    #[cfg(test)]
    pub fn report(&self) -> (Option<&Result<Report, String>>, bool) {
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

    /// What the last run was computed from that changed since, in words.
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

    /// Starts a run.
    fn start(&mut self, model: &Model, ctx: &egui::Context, plugins: &[(String, PathBuf)]) {
        let Some(plugin) = self.plugin(plugins).cloned() else {
            self.job.result = Some(Err("no plugin with field presets in use".into()));
            return;
        };
        self.st.plugin = plugin.0.clone();
        let parameters = self.parameters(&plugin);
        let plan = match read_plan(model, &self.st.plan) {
            Ok(p) => p,
            Err(e) => {
                self.job.result = Some(Err(e));
                return;
            }
        };
        // The weather: the plan's case when the model has it, else the one
        // chosen; each system its own (the plan's).
        let case = plan
            .as_ref()
            .and_then(|p| {
                let n = &p.0.weather.case;
                model
                    .cases
                    .iter()
                    .position(|c| !n.is_empty() && &c.case.name == n)
            })
            .unwrap_or(self.case);
        let Some(solved) = model.cases.get(case) else {
            self.job.result = Some(Err("no weather case in the model".into()));
            return;
        };
        let mut per = crate::field_systems::PerSystem::new();
        if let Some((p, _)) = &plan {
            for (k, v) in &p.weather.systems {
                per.insert(k.clone(), (Some(v.clone()), None));
            }
            if !p.weather.earth_wires.is_empty() {
                per.insert(
                    crate::field_systems::EARTHED.to_owned(),
                    (Some(p.weather.earth_wires.clone()), None),
                );
            }
        }
        let missing: Vec<String> = per
            .values()
            .filter_map(|(n, _)| n.clone())
            .filter(|n| !model.cases.iter().any(|c| &c.case.name == n))
            .collect();
        if !missing.is_empty() {
            self.job.result = Some(Err(format!(
                "the field plan's weather case(s) {} are not among the model's",
                missing.join(", ")
            )));
            return;
        }
        let systems = crate::field_systems::plan(model, &per, &solved.case, None);
        let line = model.line.clone();
        let library = model.library.clone();
        let wind = model.wind;
        let unsolved = !solved.solved_for(wind);
        let (mut shapes, mut jumpers) = (solved.shapes.clone(), solved.jumpers.clone());
        let weather = solved.case.clone();
        let ground_source = crate::field_ground::source(model, &self.st.ground);
        let cloud = model.cloud.clone();
        let file = model.sources.file.clone();
        let out = self.out_folder(model);
        let st = self.st.clone();
        let line_name = model
            .session()
            .map_or_else(|| model.line.name.clone(), |_| model.project_line());
        self.job.start_at(ctx, model.revision(), move || {
            let mut notes = Vec::new();
            if unsolved {
                let s = crate::model::Solved::with_wind(&line, &library, weather.clone(), wind);
                shapes = s.shapes;
                jumpers = s.jumpers;
            }
            let (shapes, per_notes) =
                crate::field_systems::compose(&line, &library, wind, shapes, &systems);
            notes.extend(per_notes);
            let (ground, ground_note) = ground_source.read()?;
            notes.extend(ground_note);
            let run = Run {
                line: &line,
                library: &library,
                shapes: &shapes,
                jumpers: &jumpers,
                weather: &weather,
                ground: ground.as_deref(),
                cloud: cloud.as_deref(),
                file: &file,
                line_name: &line_name,
            };
            let mut report = run.go(&plugin, &parameters, &st, plan.as_ref(), &out)?;
            report.notes.splice(0..0, notes);
            Ok(report)
        });
    }

    /// The report's folder: the one chosen, else `buildings-fields` beside
    /// the project.
    fn out_folder(&self, model: &Model) -> PathBuf {
        self.st.out.clone().unwrap_or_else(|| {
            model
                .sources
                .file
                .parent()
                .unwrap_or(Path::new("."))
                .join("buildings-fields")
        })
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
        } else if magnetic::circuits(&model.line).is_empty() {
            Some("no circuit: assign the electrical systems (Line › Electrical systems)".to_owned())
        } else {
            None
        };
        let findings = match &self.job.result {
            Some(Ok(r)) => Some(r.places.iter().filter(|p| over(p, r.limits)).count()),
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
                        self.results_ui(ui);
                    }),
                }
                help::footer(ui, WINDOW, |ui| {
                    again |= check_ui::job_footer(
                        ui,
                        &mut self.job,
                        why.clone(),
                        "Find the buildings by the plugin, run both fields at their cuts and \
                         places, and write the plugin's report",
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

    fn setup_ui(&mut self, ui: &mut egui::Ui, model: &Model, plugins: &[(String, PathBuf)]) {
        check_ui::section(ui, 1, "What it checks", None);
        help::explain(
            ui,
            "The magnetic and electric fields at the buildings near the line: a plugin finds \
             them from the point cloud's classes and gives the cuts across the line and the \
             places of sensitive use; its report goes into a folder.",
            "The plugin with field presets reads the points its manifest asks for (the point \
             cloud's building and ground classes near the line) and the line solved in the \
             weather case, and gives cuts across the line at each building and points at its \
             places of sensitive use. Both fields are computed there with the wires as solved \
             (each system in the field plan's weather case), the levels and limits of the field \
             plan template; the plugin then writes its report one cut at a time, each drawing \
             with the plan's title, logos, point classes and TIN or drawing.",
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
        check_ui::section(ui, 3, "Presets", None);
        self.plugin_ui(ui, plugins);
        ui.add_space(4.0);
        self.plan_ui(ui, model);
        ui.add_space(4.0);
        self.currents_ui(ui, model);
        ui.separator();
        check_ui::section(ui, 4, "Model", None);
        crate::field_ground::ui(ui, WINDOW, model, &mut self.st.ground);
        ui.horizontal(|ui| {
            ui.label("Report into");
            let out = self.out_folder(model);
            ui.label(egui::RichText::new(out.display().to_string()).monospace());
            if ui.button("Choose…").clicked()
                && let Some(p) = rfd::FileDialog::new().set_directory(&out).pick_folder()
            {
                self.st.out = Some(p);
            }
        });
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

    fn currents_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        let circuits = magnetic::circuits(&model.line);
        egui::Grid::new("building-fields-currents")
            .striped(true)
            .spacing(help::GRID_SPACING)
            .show(ui, |ui| {
                ui.strong("circuit");
                ui.strong("current");
                ui.end_row();
                for c in &circuits {
                    let label = c.label();
                    ui.label(&label);
                    let a = self.st.currents.entry(label).or_insert(0.0);
                    crate::style::field(
                        ui,
                        egui::DragValue::new(a)
                            .range(0.0..=20_000.0)
                            .speed(10.0)
                            .suffix(" A"),
                    );
                    ui.end_row();
                }
            });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.st.earth_wires, "Earth wire currents")
                .on_hover_text("The earth wires' currents induced by the phases");
            ui.add_enabled(
                self.st.earth_wires,
                egui::Checkbox::new(&mut self.st.towers, "through the towers"),
            )
            .on_hover_text(
                "The earth wires as a network over the line, each structure leading current \
                 into the ground through its earthing resistance",
            );
        });
    }

    fn results_ui(&mut self, ui: &mut egui::Ui) {
        let report = match &self.job.result {
            None => {
                ui.label(egui::RichText::new("No run yet: Run at the bottom.").weak());
                return;
            }
            Some(Err(e)) => {
                crate::menu::error_label(ui, e);
                return;
            }
            Some(Ok(r)) => r,
        };
        let n = report
            .places
            .iter()
            .filter(|p| over(p, report.limits))
            .count();
        let buildings: std::collections::BTreeSet<&str> =
            report.places.iter().map(Place::building).collect();
        check_ui::verdict(
            ui,
            n > 0,
            &if report.limits == (None, None) {
                format!(
                    "{} places at {} buildings in \"{}\" (no limit: the field plan gives none)",
                    report.places.len(),
                    buildings.len(),
                    report.case
                )
            } else if n > 0 {
                format!(
                    "✖ {n} place{} over a limit in \"{}\"",
                    if n == 1 { "" } else { "s" },
                    report.case
                )
            } else {
                format!(
                    "✔ Every place within the limits in \"{}\" ({} places at {} buildings)",
                    report.case,
                    report.places.len(),
                    buildings.len()
                )
            },
            false,
        );
        ui.horizontal(|ui| {
            ui.label(format!(
                "{} file(s) of preset {} in {}",
                report.files.len(),
                report.preset,
                report.out.display()
            ));
            if ui.button("Open the folder").clicked() {
                let _ = crate::help::open(&report.out);
            }
        });
        check_ui::notes_fold(ui, WINDOW, &report.notes);
        use check_ui::{Cell, Col};
        let (lb, le) = report.limits;
        let rows: Vec<check_ui::Row> = report
            .places
            .iter()
            .map(|p| {
                let worst_b = larger(p.b, p.b_major);
                let bad_b = lb.is_some_and(|l| worst_b.is_some_and(|b| b > l));
                let bad_e = le.is_some_and(|l| p.e.is_some_and(|e| e > l));
                check_ui::Row::new(
                    vec![
                        Cell::text(p.building()),
                        Cell::text(&p.name),
                        Cell::num(Some(p.height), 1),
                        Cell::num(p.b, 3).bad(bad_b),
                        Cell::num(p.b_major, 3).bad(bad_b),
                        Cell::num(lb.zip(worst_b).map(|(l, b)| l - b), 3).bad(bad_b),
                        Cell::num(p.e, 4).bad(bad_e),
                        Cell::num(le.zip(p.e).map(|(l, e)| l - e), 4).bad(bad_e),
                    ],
                    bad_b || bad_e,
                )
            })
            .collect();
        check_ui::ResultTable::new(
            "building-fields-places",
            vec![
                Col::text("Building"),
                Col::text("Place"),
                Col::num("Height [m]"),
                Col::num("B [µT]"),
                Col::num("B cycle [µT]").tip("The largest over the cycle"),
                Col::num("Margin B [µT]").tip("The limit less the larger of B and its cycle's"),
                Col::num("E [kV/m]"),
                Col::num("Margin E [kV/m]"),
            ],
        )
        .rows(rows)
        .group_by(&[0])
        .csv("building-fields-places.csv")
        .ui(ui);
    }
}

/// A place over a limit.
fn over(p: &Place, (lb, le): (Option<f64>, Option<f64>)) -> bool {
    lb.is_some_and(|l| larger(p.b, p.b_major).is_some_and(|b| b > l))
        || le.is_some_and(|l| p.e.is_some_and(|e| e > l))
}

/// The larger of two values, either missing.
fn larger(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
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

/// What a run reads, taken on the UI thread.
struct Run<'a> {
    line: &'a ohl_line::Line,
    library: &'a ohl_cables::Library,
    shapes: &'a [ohl_line::solve::SectionShape],
    jumpers: &'a [ohl_line::jumper::JumperShape],
    weather: &'a ohl_weather::WeatherCase,
    ground: Option<&'a ohl_terrain::Terrain>,
    cloud: Option<&'a str>,
    file: &'a Path,
    line_name: &'a str,
}

impl Run<'_> {
    /// Gathers the plugin's points, takes its preset, runs both fields
    /// there and has the plugin write its report into `out`.
    fn go(
        &self,
        plugin: &(String, PathBuf),
        parameters: &str,
        st: &Stored,
        plan: Option<&(FieldPlan, String)>,
        out: &Path,
    ) -> Result<Report, String> {
        let (name, dir) = plugin;
        let mut notes = Vec::new();
        let host = ohl_plugins::WasmHost::new().map_err(|e| e.to_string())?;
        let manifest = ohl_plugins::Manifest::load(dir).map_err(|e| e.to_string())?;
        // The spans in plan and the alignments the host checks against.
        let at = |id: &str| {
            self.line
                .structure(id)
                .map(|s| [s.position[0], s.position[1]])
                .ok_or_else(|| format!("no structure {id}"))
        };
        let mut spans = Vec::new();
        for s in ohl_electrical::cross_section::line_spans(self.line) {
            spans.push(vec![at(&s.from)?, at(&s.to)?]);
        }
        let alignments: Vec<(String, f64)> = self
            .line
            .alignment_views()
            .iter()
            .map(|v| {
                (
                    v.id.clone(),
                    ohl_line::route::plan_length(&v.centreline(self.line)),
                )
            })
            .collect();
        let cloud = match self.cloud {
            Some(c) => Some(ohl_pointcloud::open_source(c)?),
            None => None,
        };
        let cloud_ref = cloud
            .as_deref()
            .map(|c| c as &dyn ohl_pointcloud::PointSource);
        // Exit clause: decoding is about 1 s per 10⁷ points.
        let budget =
            ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(600));
        let table = manifest.field_input.clone().unwrap_or_default();
        // The buildings from the point cloud's classes alone (owner,
        // 2026-11-02: "There are classes for buildings only use those").
        let gathered = ohl_io::field_input::gather(&table, cloud_ref, &[], &spans, &budget)?;
        notes.extend(gathered.notes.iter().map(|n| format!("plugin input: {n}")));
        let model_json = ohl_io::ExportModel::new(
            self.line,
            &[(
                self.weather.name.clone(),
                self.weather.summary(),
                self.shapes.to_vec(),
                self.jumpers.to_vec(),
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
        let preset = if st.preset.is_empty() {
            presets.first()
        } else {
            presets.iter().find(|p| p.name == st.preset)
        }
        .ok_or_else(|| {
            format!(
                "plugin {name}: no preset {} (it gives: {})",
                st.preset,
                presets
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?
        .clone();
        notes.extend(preset.notes.iter().map(|n| format!("plugin: {n}")));
        // Both fields at the preset's cuts and places, the plan's levels
        // and limits added.
        let plan_levels = |m: bool| {
            plan.map_or_else(Vec::new, |(p, _)| {
                if m {
                    p.magnetic.values()
                } else {
                    p.electric.values()
                }
            })
        };
        let levels = |own: &[f64], m: bool| {
            let mut l: Vec<f64> = own.iter().copied().chain(plan_levels(m)).collect();
            l.sort_by(f64::total_cmp);
            l.dedup();
            l
        };
        let limits = (
            plan.and_then(|(p, _)| p.magnetic.limit()),
            plan.and_then(|(p, _)| p.electric.limit()),
        );
        let cuts = ohl_io::field_input::preset_cuts(&preset);
        let points = ohl_io::field_input::preset_points(&preset);
        let ms = magnetic::Settings {
            currents: st
                .currents
                .iter()
                .map(|(c, a)| magnetic::CircuitCurrent {
                    circuit: c.clone(),
                    amps: *a,
                    reverse: false,
                })
                .collect(),
            earth_wires: st.earth_wires,
            towers: st.earth_wires && st.towers,
            profiles: false,
            cuts: cuts.clone(),
            points: points.clone(),
            levels: levels(&preset.levels_ut, true),
            limit_ut: limits.0,
            ..magnetic::Settings::default()
        };
        let es = electric::Settings {
            profiles: false,
            cuts,
            points,
            levels: levels(&preset.levels_kv_m, false),
            limit_kv_m: limits.1,
            ..electric::Settings::default()
        };
        let height = |x: f64, y: f64| self.ground.and_then(|t| t.height(x, y));
        let g: Option<&(dyn Fn(f64, f64) -> Option<f64> + Sync)> = self
            .ground
            .map(|_| &height as &(dyn Fn(f64, f64) -> Option<f64> + Sync));
        let src = ohl_electrical::Sources {
            line: self.line,
            library: self.library,
            shapes: self.shapes,
            temperature: self.weather.temperature,
            ground: g,
            jumpers: self.jumpers,
        };
        let m = magnetic::run(&src, &ms, None)?;
        let e = electric::run(&src, &es, None)?;
        notes.extend(m.notes.iter().map(|n| format!("magnetic: {n}")));
        notes.extend(e.notes.iter().map(|n| format!("electric: {n}")));
        let (case, t) = (&self.weather.name, self.weather.temperature.celsius());
        let places = m
            .points
            .iter()
            .map(|p| Place {
                name: p.name.clone(),
                height: p.height,
                b: Some(p.b_ut),
                b_major: Some(p.major_ut),
                e: e.points.iter().find(|q| q.name == p.name).map(|q| q.e_kv_m),
            })
            .collect();
        let set = serde_json::json!({
            "format": field_results::SET_FORMAT,
            "version": field_results::SET_VERSION,
            "case": {"name": case, "temperature_c": t},
            "fields": [
                field_results::magnetic_value(&m, &ms, case, t),
                field_results::electric_value(&e, &es, case, t),
            ],
        });
        drop((m, e));
        let (summary, mut docs) = field_results::split_set(set);
        for d in &mut docs {
            d["line"] = serde_json::Value::from(self.line_name);
        }
        if let Some((p, _)) = plan {
            notes.extend(self.extras(p, &mut docs, cloud_ref, g, &budget)?);
        }
        let cut_texts: Vec<String> = docs.into_iter().map(|d| d.to_string()).collect();
        let results = summary.to_string();
        let files = host
            .field_report(
                dir,
                &ohl_plugins::ReportInput {
                    parameters,
                    plan: plan.map_or("", |(_, j)| j.as_str()),
                    preset: &preset,
                    results: &results,
                    cuts: &cut_texts,
                },
                &mut |_, _| {},
            )
            .map_err(|e| e.to_string())?;
        std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
        for f in &files {
            let path = out.join(&f.name);
            std::fs::write(&path, &f.bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        Ok(Report {
            case: case.clone(),
            preset: preset.name.clone(),
            places,
            limits,
            files: files.iter().map(|f| f.name.clone()).collect(),
            out: out.to_path_buf(),
            notes,
        })
    }

    /// The plan's points and its TIN or drawing along each cut.
    fn extras(
        &self,
        plan: &FieldPlan,
        docs: &mut [serde_json::Value],
        cloud: Option<&dyn ohl_pointcloud::PointSource>,
        ground: Option<&(dyn Fn(f64, f64) -> Option<f64> + Sync)>,
        budget: &ohl_core::budget::Budget,
    ) -> Result<Vec<String>, String> {
        use field_plan::Background;
        use ohl_io::field_cuts::{self, Along};
        let surface = match plan.cut.background {
            Background::Tin if !plan.cut.surface.is_empty() => {
                Some(crate::surfaces::load(self.file, &plan.cut.surface)?)
            }
            _ => None,
        };
        let drawing = match plan.cut.background {
            Background::Dxf => Some(
                ohl_background::Store::open(self.file)
                    .and_then(|s| s.drawing(&plan.cut.drawing))
                    .map_err(|e| format!("field plan: drawing {}: {}", plan.cut.drawing, e.0))?,
            ),
            _ => None,
        };
        let tin = |x: f64, y: f64| match (&surface, ground) {
            (Some(t), _) => t.height(x, y),
            (None, Some(g)) => g(x, y),
            (None, None) => None,
        };
        let crossings = |p: &field_cuts::CutPlane| {
            drawing.as_ref().map_or_else(Vec::new, |d| {
                d.plane_crossings(p.origin, p.right, p.span, &plan.cut.layers)
            })
        };
        let along = match plan.cut.background {
            Background::None => Along::Nothing,
            Background::Tin => Along::Tin(
                if plan.cut.surface.is_empty() {
                    "the field's ground"
                } else {
                    &plan.cut.surface
                },
                &tin,
            ),
            Background::Dxf => Along::Dxf(&plan.cut.drawing, &crossings),
        };
        let classes: Vec<u8> = plan.cut.classes.iter().map(|c| c.class).collect();
        let notes = field_cuts::add_extras(docs, cloud, &classes, plan.cut.band, &along, budget)?;
        Ok(notes.into_iter().map(|n| format!("plan: {n}")).collect())
    }
}
