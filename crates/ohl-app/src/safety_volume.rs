// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Safety volumes: the space within a distance of the wires in
//! every position of the weather cases ticked (blowouts, wind steps,
//! swing arc), one closed mesh per wire or per circuit
//! ([`ohl_clearance::safety_volume`]), for work near the line. The
//! distance is a requirement by voltage (none is built in: the user's, or
//! a rule of a design-code plugin's preset). Run on a background thread;
//! drawn translucent in the 3D view (each in its circuit's colour) and
//! its outline in the plan while the window is open; out of date (dimmed,
//! **Run again**) once the line or its weather cases change; saved in
//! Checks › Performed checks ([`CHECK_ID`]) with its meshes, and exported
//! as georeferenced DXF (+ `.prj`), IFC4 and KML/KMZ
//! ([`ohl_io::safety_volume`]).

use std::sync::Arc;

use eframe::egui;
use ohl_clearance::rules::{Requirement, WindSides, WindSteps};
use ohl_clearance::safety_volume::{
    DEFAULT_TOLERANCE, Grouping, Mesh, MeshStats, SafetyParams, SafetyVolume, SafetyVolumes,
    WindMode,
};
use ohl_io::safety_volume::{Description, Format};
use ohl_session::packing::{Reader, Writer};
use ohl_session::reports::{self, CheckReport, ReportStatus};
use serde_json::{Value, json};

use crate::job::Job;
use crate::model::Model;

/// The id of a saved run in the library of performed checks.
pub const CHECK_ID: &str = "ohl.safety-volume";
/// Its title.
pub const CHECK_TITLE: &str = "Safety volumes";
/// The version of its saved parameters and results.
pub const SAVED_VERSION: u32 = 1;
/// A run with more triangles than this is saved without its meshes (run
/// again to see them).
pub const MAX_SAVED_TRIANGLES: usize = 2_000_000;

/// What a safety volume is computed from: the line (with its weather
/// cases).
const USES: crate::revision::Uses = crate::revision::Uses::LINE;

/// The window's settings: what a run uses, saved with it.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// The distance from the conductors' surface (none built in: 0 m
    /// until entered).
    pub distance: Requirement,
    /// Added to the distance, m.
    pub margin: f64,
    /// How far the mesh may lie outside the envelope, m.
    pub tolerance: f64,
    /// The wind.
    pub wind: WindMode,
    /// Its steps (step and sides) when stepped.
    pub steps: WindSteps,
    /// Per wire or per circuit.
    pub grouping: Grouping,
    /// The circuits (empty: all).
    pub circuits: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        let p = SafetyParams::default();
        Self {
            distance: p.distance,
            margin: p.margin,
            tolerance: DEFAULT_TOLERANCE,
            wind: WindMode::None,
            steps: WindSteps::default(),
            grouping: Grouping::Wire,
            circuits: Vec::new(),
        }
    }
}

impl Settings {
    /// The parameters of a run.
    pub fn params(&self) -> SafetyParams {
        SafetyParams {
            distance: self.distance.clone(),
            margin: self.margin,
            tolerance: self.tolerance,
            grouping: self.grouping,
            circuits: self.circuits.clone(),
        }
    }

    /// As saved (JSON), with the weather cases' names.
    pub fn to_json(&self, cases: &[String]) -> Value {
        json!({
            "cases": cases,
            "distance": self.distance.to_toml(),
            "margin": self.margin,
            "tolerance": self.tolerance,
            "wind": self.wind.as_str(),
            "wind_step": self.steps.step,
            "wind_sides": self.steps.sides.as_str(),
            "per": self.grouping.as_str(),
            "circuits": self.circuits,
        })
    }

    /// Read back from [`Settings::to_json`]: the settings and the cases.
    pub fn from_json(v: &Value) -> Result<(Self, Vec<String>), String> {
        let d = Self::default();
        let num = |k: &str, or: f64| v.get(k).and_then(Value::as_f64).unwrap_or(or);
        let text = |k: &str| v.get(k).and_then(Value::as_str);
        let list = |k: &str| -> Vec<String> {
            v.get(k)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|c| c.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        let distance = match text("distance") {
            Some(t) => Requirement::parse(t).map_err(|e| format!("distance: {e}"))?,
            None => d.distance.clone(),
        };
        Ok((
            Self {
                distance,
                margin: num("margin", d.margin),
                tolerance: num("tolerance", d.tolerance),
                wind: text("wind").map_or(Ok(d.wind), WindMode::parse)?,
                steps: WindSteps {
                    stepped: false,
                    arc: false,
                    step: num("wind_step", d.steps.step),
                    sides: text("wind_sides")
                        .and_then(|s| WindSides::parse(s).ok())
                        .unwrap_or(WindSides::Both),
                },
                grouping: text("per").map_or(Ok(d.grouping), Grouping::parse)?,
                circuits: list("circuits"),
            },
            list("cases"),
        ))
    }

    /// The distance and margin in words.
    pub fn distance_text(&self) -> String {
        if self.margin > 0.0 {
            format!("{} + {:.2} m", self.distance, self.margin)
        } else {
            self.distance.to_string()
        }
    }
}

/// What a run gave (or a saved run holds).
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// The volumes (without meshes when saved without them).
    pub volumes: Arc<SafetyVolumes>,
    /// Each volume's colour (its circuit's).
    pub colors: Vec<[u8; 3]>,
    /// The settings of the run.
    pub settings: Settings,
    /// The saved report it was read from, if any.
    pub saved: Option<String>,
    /// Whether it holds the meshes (a saved run may not).
    pub meshes: bool,
}

/// The colour of each volume: its first section's (the circuit's wire
/// colour, [`ohl_line::Line::section_color`]).
fn colors(model: &Model, v: &SafetyVolumes) -> Vec<[u8; 3]> {
    v.volumes
        .iter()
        .map(|s| {
            s.sections
                .first()
                .and_then(|id| model.line.sections.iter().find(|x| &x.id == id))
                .map_or([230, 120, 40], |x| {
                    crate::scene::rgb(&model.line.section_color(x))
                })
        })
        .collect()
}

/// What the views draw of a report.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overlay {
    /// The volumes (none: nothing drawn).
    pub volumes: Option<Arc<SafetyVolumes>>,
    /// Their colours.
    pub colors: Vec<[u8; 3]>,
    /// Each volume's outline seen from above (plan).
    pub outlines: Arc<Vec<Vec<[[f64; 3]; 2]>>>,
    /// The report is out of date: drawn dimmed.
    pub stale: bool,
}

impl Overlay {
    fn of(r: &Report) -> Self {
        Self {
            outlines: Arc::new(
                r.volumes
                    .volumes
                    .iter()
                    .map(|v| v.mesh.silhouette())
                    .collect(),
            ),
            volumes: Some(Arc::clone(&r.volumes)),
            colors: r.colors.clone(),
            stale: false,
        }
    }
}

/// What the window asks of the application.
#[derive(Default)]
pub struct Out {
    /// A command to apply (a run saved), with what it does.
    pub apply: Option<(String, ohl_session::Command)>,
    /// A message for the status bar (an error when true).
    pub status: Option<(String, bool)>,
}

/// The Safety volumes window.
pub struct SafetyVolumeWindow {
    /// Shown (closed: its layer is taken off the views).
    pub open: bool,
    /// Weather cases ticked (indices in the model's cases).
    pub cases: Vec<usize>,
    /// The settings.
    pub settings: Settings,
    job: Job<Report>,
    /// Counts the reports taken (a new one is drawn).
    generation: u64,
    /// What the views draw now: shown, report.
    drawn: Option<(bool, u64)>,
    /// A saved run shown: what changed since it was saved.
    saved_stale: Option<String>,
    /// The name to save the run as, and what the last save did.
    save_name: String,
    save_message: Option<(String, bool)>,
    overwrite: Option<String>,
    /// The preset and rule chosen to fill the distance.
    preset: Option<(usize, usize)>,
    rule: usize,
    /// The formats ticked for export.
    pub formats: Vec<Format>,
    /// What the last export did.
    export_message: Option<(String, bool)>,
}

impl Default for SafetyVolumeWindow {
    fn default() -> Self {
        Self {
            open: false,
            cases: Vec::new(),
            settings: Settings::default(),
            job: Job::default(),
            generation: 0,
            drawn: None,
            saved_stale: None,
            save_name: "safety volumes".into(),
            save_message: None,
            overwrite: None,
            preset: None,
            rule: 0,
            formats: vec![Format::Dxf, Format::Ifc, Format::Kmz],
            export_message: None,
        }
    }
}

impl SafetyVolumeWindow {
    /// Opens the window on the case shown (ticked, if nothing is).
    pub fn show(&mut self, case: usize) {
        self.open = true;
        if self.cases.is_empty() {
            self.cases.push(case);
        }
    }

    /// The report shown.
    pub fn report(&self) -> Option<&Report> {
        match &self.job.result {
            Some(Ok(r)) => Some(r),
            _ => None,
        }
    }

    /// The last run's answer, and whether one runs (tests).
    #[cfg(test)]
    pub fn result(&self) -> (Option<&Result<Report, String>>, bool) {
        (self.job.result.as_ref(), self.job.running())
    }

    /// Sets the name a run is saved as (tests).
    #[cfg(test)]
    pub fn set_save_name(&mut self, name: &str) {
        self.save_name = name.to_owned();
    }

    /// What the views should draw, when it changed since the last call.
    pub fn overlay_change(&mut self) -> Option<Overlay> {
        let shown = self.open && self.report().is_some_and(|r| r.meshes);
        let key = (shown, self.generation);
        if self.drawn == Some(key) {
            return None;
        }
        self.drawn = Some(key);
        Some(match (shown, self.report()) {
            (true, Some(r)) => Overlay::of(r),
            _ => Overlay::default(),
        })
    }

    /// What the report shown was computed from that changed since, in
    /// words (`None`: nothing, or no report).
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        self.report()?;
        if let Some(s) = &self.saved_stale {
            return Some(s.clone());
        }
        self.job.stamp.out_of_date(model, USES)
    }

    /// Starts a run on the model as it is.
    pub fn start(&mut self, model: &Model, ctx: &egui::Context) {
        let cases: Vec<ohl_weather::WeatherCase> = self
            .cases
            .iter()
            .filter_map(|&k| model.cases.get(k).map(|c| c.case.clone()))
            .collect();
        if cases.is_empty() {
            self.job.result = Some(Err("tick a weather case".into()));
            return;
        }
        let params = self.settings.params();
        if let Some(p) = params.problem() {
            self.job.result = Some(Err(p));
            return;
        }
        self.saved_stale = None;
        let line = model.line.clone();
        let library = model.library.clone();
        let settings = self.settings.clone();
        // The colours are the line's: taken now, kept with the report.
        let sections: Vec<(String, [u8; 3])> = model
            .line
            .sections
            .iter()
            .map(|s| {
                (
                    s.id.clone(),
                    crate::scene::rgb(&model.line.section_color(s)),
                )
            })
            .collect();
        self.job.start_at(ctx, model.revision(), move || {
            // Exit clause: an hour is far beyond what a line needs.
            let budget =
                ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(3600));
            let v = ohl_clearance::safety_volume::run(
                &line,
                &library,
                &cases,
                settings.wind,
                settings.steps,
                &params,
                &budget,
            )?;
            let colors = v
                .volumes
                .iter()
                .map(|s| {
                    s.sections
                        .first()
                        .and_then(|id| sections.iter().find(|x| &x.0 == id))
                        .map_or([230, 120, 40], |x| x.1)
                })
                .collect();
            Ok(Report {
                volumes: Arc::new(v),
                colors,
                settings,
                saved: None,
                meshes: true,
            })
        });
    }

    /// The saved run `r` shown (its settings, cases and volumes), with
    /// what changed since it was saved.
    pub fn load_saved(&mut self, r: &CheckReport, model: &Model) -> Result<(), String> {
        let (settings, cases) = Settings::from_json(&r.parameters)?;
        let mut volumes = saved::read(&r.results)?;
        volumes.cases = cases.clone();
        let meshes = volumes.volumes.iter().any(|v| !v.mesh.triangles.is_empty());
        self.saved_stale = None;
        if let Some(s) = model.session()
            && let Ok(now) = s.check_state()
            && !reports::changed_since(&r.state, &now).is_empty()
        {
            self.saved_stale = Some("the line".into());
        }
        self.job.stamp.set(model.revision());
        self.settings = settings.clone();
        self.cases = cases
            .iter()
            .filter_map(|n| model.cases.iter().position(|c| &c.case.name == n))
            .collect();
        self.save_name = r.id.clone();
        let colors = colors(model, &volumes);
        self.job.result = Some(Ok(Report {
            volumes: Arc::new(volumes),
            colors,
            settings,
            saved: Some(r.id.clone()),
            meshes,
        }));
        self.generation += 1;
        self.open = true;
        Ok(())
    }

    /// The run shown as a report named `name` of the performed checks.
    fn to_check_report(&self, model: &Model, name: &str) -> Result<CheckReport, String> {
        let report = self.report().ok_or("run it first")?;
        let editing = model
            .editing
            .as_ref()
            .map_err(|e| format!("read-only: {e}"))?;
        let s = &editing.session;
        let mut parameters = report.settings.to_json(&report.volumes.cases);
        parameters["version"] = json!(SAVED_VERSION);
        let triangles: usize = report
            .volumes
            .volumes
            .iter()
            .map(|v| v.mesh.triangles.len())
            .sum();
        Ok(CheckReport {
            format: reports::REPORT_FILE_FORMAT.into(),
            version: reports::REPORT_FILE_VERSION,
            id: name.to_owned(),
            check: CHECK_ID.into(),
            title: CHECK_TITLE.into(),
            time: s.now().map_err(|e| e.to_string())?,
            user: editing.user.clone(),
            app: format!("OHL-Design {}", env!("CARGO_PKG_VERSION")),
            plugin: None,
            status: ReportStatus::Draft,
            note: s.report(name).map(|o| o.note).unwrap_or_default(),
            state: s.check_state().map_err(|e| e.to_string())?,
            summary: format!(
                "{} volumes, {} from the conductors, {}; {} triangles",
                report.volumes.volumes.len(),
                report.settings.distance_text(),
                report.settings.wind.describe(),
                triangles
            ),
            parameters,
            results: saved::results(&report.volumes, triangles <= MAX_SAVED_TRIANGLES)?,
        })
    }

    /// Writes the volumes shown in the formats ticked into `dir`.
    pub fn export_to(&mut self, model: &Model, dir: &std::path::Path) -> Result<String, String> {
        let report = self.report().ok_or("run it first")?;
        if !report.meshes {
            return Err("this saved run holds no meshes: run it again".into());
        }
        if self.formats.is_empty() {
            return Err("tick a format".into());
        }
        let (date, _) = ohl_io::landxml::utc_now();
        let d = Description {
            line: model.line.name.clone(),
            crs: model.line.crs.clone(),
            distance: report.settings.distance_text(),
            wind: report.settings.wind.describe().into(),
            date,
        };
        let stem: String = format!("{} safety volume", model.line.name)
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let (files, notes) =
            ohl_io::safety_volume::write(dir, &stem, &report.volumes, &d, &self.formats)?;
        let mut text = format!(
            "written: {}",
            files
                .iter()
                .filter_map(|f| f.file_name().map(|n| n.to_string_lossy().into_owned()))
                .collect::<Vec<_>>()
                .join(", ")
        );
        for n in notes {
            text.push_str(&format!("; {n}"));
        }
        Ok(text)
    }

    /// The window; what it asks of the app.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        model: &Model,
        presets: &[crate::rule_editor::PluginPresets],
    ) -> Out {
        let was = self.job.running();
        self.job.poll();
        if was && !self.job.running() {
            self.generation += 1;
        }
        let mut out = Out::default();
        if !self.open {
            return out;
        }
        let mut open = true;
        let mut again = false;
        let stale = self.out_of_date(model);
        egui::Window::new(CHECK_TITLE)
            .open(&mut open)
            .default_width(620.0)
            .default_height(640.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("safety-volume-window")
                    .show(ui, |ui| {
                        if let Some(what) = &stale {
                            again = crate::revision::banner(ui, what, !self.job.running());
                        }
                        self.settings_ui(ui, model, presets);
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(!self.job.running(), egui::Button::new("Run"))
                                .on_hover_text(
                                    "Solve the cases ticked and build the volumes (in the \
                                     background)",
                                )
                                .clicked()
                            {
                                again = true;
                            }
                            self.job.status_ui(ui);
                        });
                        match &self.job.result {
                            None => {
                                ui.label(
                                    "The space within the distance of the wires in every \
                                     position of the cases ticked, for work near the line: \
                                     one closed volume per wire or per circuit, shown in the \
                                     3D view and the plan, exported as DXF, IFC or KML.",
                                );
                            }
                            Some(Err(e)) => {
                                crate::menu::error_label(ui, e);
                            }
                            Some(Ok(_)) => {
                                self.report_ui(ui);
                                ui.separator();
                                self.save_ui(ui, model, &mut out);
                                ui.separator();
                                self.export_ui(ui, model);
                            }
                        }
                    });
            });
        self.open = open;
        if again {
            self.start(model, ctx);
        }
        out
    }

    fn settings_ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        presets: &[crate::rule_editor::PluginPresets],
    ) {
        let voltages = ohl_clearance::zone::line_voltages(&model.line);
        let small = |ui: &mut egui::Ui, t: &str| {
            ui.label(egui::RichText::new(t).small().weak());
        };
        ui.strong("Wire positions");
        let cases = &mut self.cases;
        egui::CollapsingHeader::new(format!("Weather cases ({} ticked)", cases.len()))
            .id_salt("safety-volume-cases")
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(140.0)
                    .show(ui, |ui| {
                        for (k, c) in model.cases.iter().enumerate() {
                            let mut on = cases.contains(&k);
                            if ui.checkbox(&mut on, &c.case.name).changed() {
                                if on {
                                    cases.push(k);
                                    cases.sort_unstable();
                                } else {
                                    cases.retain(|x| *x != k);
                                }
                            }
                        }
                    });
            });
        let s = &mut self.settings;
        ui.horizontal(|ui| {
            ui.label("wind");
            egui::ComboBox::from_id_salt("safety-volume-wind")
                .selected_text(s.wind.describe())
                .show_ui(ui, |ui| {
                    for m in WindMode::ALL {
                        ui.selectable_value(&mut s.wind, m, m.describe());
                    }
                });
            ui.add_enabled_ui(s.wind.stepped(), |ui| {
                ui.add(
                    egui::DragValue::new(&mut s.steps.step)
                        .speed(10.0)
                        .range(1.0..=10_000.0)
                        .suffix(" Pa"),
                );
                egui::ComboBox::from_id_salt("safety-volume-sides")
                    .selected_text(s.steps.sides.as_str())
                    .width(70.0)
                    .show_ui(ui, |ui| {
                        for w in WindSides::ALL {
                            ui.selectable_value(&mut s.steps.sides, w, w.as_str());
                        }
                    });
            });
        });
        small(
            ui,
            "Each case: still air as it is, a windy case blown left and right at its full \
             pressure. Wind stepped: also still air and every pressure up to the case's. Swing \
             arc: also every position between the two blowouts (a rigid swing at the largest \
             radius, closed form).",
        );
        ui.separator();
        ui.strong("Distance");
        ui.horizontal(|ui| {
            ui.label("from the conductors' surface");
            crate::requirement_ui::requirement_ui_stacked(
                ui,
                "safety-volume-distance",
                &mut s.distance,
                &voltages,
            );
        });
        small(
            ui,
            "No safety distance is built in: enter the one of the rules you work to (one \
             distance, or by the voltage of each wire's system; earthed wires count as 0 kV), \
             or fill it from a rule of a design-code plugin's preset below.",
        );
        // A preset's rule fills the distance (its radial distance, else
        // its vertical one).
        ui.horizontal(|ui| {
            ui.label("preset");
            if crate::rule_editor::preset_dropdown(
                ui,
                "safety-volume-preset",
                presets,
                |p| {
                    p.rules
                        .rules
                        .iter()
                        .any(|r| r.radial.is_some() || r.vertical.is_some())
                },
                &mut self.preset,
            ) {
                self.rule = 0;
            }
        });
        if let Some(preset) = self
            .preset
            .and_then(|(i, k)| presets.get(i)?.presets.as_ref().ok()?.get(k).cloned())
        {
            let rules: Vec<(String, Requirement)> = preset
                .rules
                .rules
                .iter()
                .filter_map(|r| {
                    r.radial
                        .clone()
                        .or_else(|| r.vertical.clone())
                        .map(|q| (r.name.clone(), q))
                })
                .collect();
            ui.horizontal(|ui| {
                ui.label("rule");
                let shown = rules.get(self.rule).map_or("-", |r| r.0.as_str());
                egui::ComboBox::from_id_salt("safety-volume-preset-rule")
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        for (k, (name, q)) in rules.iter().enumerate() {
                            ui.selectable_value(&mut self.rule, k, format!("{name}: {q}"));
                        }
                    });
                if let Some((name, q)) = rules.get(self.rule)
                    && ui
                        .button("Fill the distance")
                        .on_hover_text(format!(
                            "The rule \"{name}\"'s radial distance (else its vertical one): {q}"
                        ))
                        .clicked()
                {
                    s.distance = q.clone();
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label("margin");
            ui.add(
                egui::DragValue::new(&mut s.margin)
                    .speed(0.05)
                    .range(0.0..=50.0)
                    .suffix(" m"),
            )
            .on_hover_text("Added to the distance (e.g. for the survey's accuracy or tools)");
            ui.label("tolerance");
            ui.add(
                egui::DragValue::new(&mut s.tolerance)
                    .speed(0.005)
                    .range(0.005..=1.0)
                    .suffix(" m"),
            )
            .on_hover_text(
                "How far the mesh may lie outside the exact volume where it is smooth; it is \
                 never inside it. Smaller: more triangles.",
            );
        });
        ui.separator();
        ui.strong("Volumes");
        ui.horizontal(|ui| {
            ui.radio_value(&mut s.grouping, Grouping::Wire, "one per wire");
            ui.radio_value(&mut s.grouping, Grouping::Circuit, "one per circuit");
            let mut names: Vec<String> = model
                .line
                .sections
                .iter()
                .map(|x| x.circuit_name.clone())
                .collect();
            names.sort();
            names.dedup();
            let shown = if s.circuits.is_empty() {
                "every circuit".to_owned()
            } else {
                s.circuits.join(", ")
            };
            // A tick menu: it stays open while ticking.
            crate::menu::options_menu(ui, &format!("{shown} ⏷"), |ui| {
                if ui.button("Every circuit").clicked() {
                    s.circuits.clear();
                }
                for n in &names {
                    let mut on = s.circuits.contains(n);
                    if ui.checkbox(&mut on, n).changed() {
                        if on {
                            s.circuits.push(n.clone());
                        } else {
                            s.circuits.retain(|x| x != n);
                        }
                    }
                }
            });
        });
        small(
            ui,
            "A wire: a circuit's phase through all its sections. Structures and jumpers are not \
             included.",
        );
    }

    fn report_ui(&self, ui: &mut egui::Ui) {
        let Some(r) = self.report() else { return };
        let v = &r.volumes;
        ui.label(format!(
            "{} volumes from {} wire positions; weather cases: {}; {} from the conductors, {}.",
            v.volumes.len(),
            v.positions,
            v.cases.join(", "),
            r.settings.distance_text(),
            r.settings.wind.describe()
        ));
        if !r.meshes {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "Saved without its meshes (too large): run it again to see and export them.",
            );
        }
        egui::Grid::new("safety-volume-table")
            .striped(true)
            .num_columns(7)
            .show(ui, |ui| {
                for h in [
                    "volume",
                    "voltage",
                    "distance, m",
                    "triangles",
                    "volume, m³",
                    "outside by ≤, m",
                    "time, s",
                ] {
                    ui.strong(h);
                }
                ui.end_row();
                for (s, c) in v.volumes.iter().zip(&r.colors) {
                    ui.horizontal(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter().rect_filled(
                            rect,
                            2.0,
                            egui::Color32::from_rgb(c[0], c[1], c[2]),
                        );
                        ui.label(&s.name);
                    });
                    ui.label(s.kv.map_or("earthed".into(), |k| format!("{k} kV")));
                    ui.label(format!("{:.2}", s.distance + s.margin));
                    ui.label(s.mesh.triangles.len().to_string());
                    ui.label(if r.meshes {
                        format!("{:.0}", s.mesh.volume())
                    } else {
                        "-".into()
                    });
                    ui.label(format!("{:.3}", s.allowance));
                    ui.label(format!("{:.2}", s.seconds));
                    ui.end_row();
                }
            });
        ui.label(
            egui::RichText::new(
                "Each mesh holds the whole exact volume (never inside it) and lies outside it by \
                 at most the value shown where the volume is smooth; where tubes meet, its \
                 triangles may bridge the groove by up to one grid cell.",
            )
            .small()
            .weak(),
        );
        if !v.notes.is_empty() {
            egui::ScrollArea::vertical()
                .id_salt("safety-volume-notes")
                .max_height(80.0)
                .show(ui, |ui| {
                    for n in &v.notes {
                        ui.label(egui::RichText::new(n).small());
                    }
                });
        }
    }

    fn save_ui(&mut self, ui: &mut egui::Ui, model: &Model, out: &mut Out) {
        ui.horizontal(|ui| {
            ui.label("Save in the library of performed checks as");
            ui.add(egui::TextEdit::singleline(&mut self.save_name).desired_width(160.0));
            let exists = model
                .session()
                .is_some_and(|s| s.report(self.save_name.trim()).is_some());
            let asked = self.overwrite.as_deref() == Some(self.save_name.trim());
            let label = if exists && asked {
                "Replace it"
            } else {
                "Save in Performed checks"
            };
            if ui
                .button(label)
                .on_hover_text(
                    "Kept with the project line; reopened from Checks › Performed checks",
                )
                .clicked()
            {
                let name = self.save_name.trim().to_owned();
                if name.is_empty() {
                    self.save_message = Some(("give it a name".into(), true));
                } else if exists && !asked {
                    self.overwrite = Some(name.clone());
                    self.save_message = Some((
                        format!("\"{name}\" exists: click again to replace it"),
                        true,
                    ));
                } else {
                    self.overwrite = None;
                    match self.to_check_report(model, &name) {
                        Ok(report) => {
                            out.apply =
                                Some((format!("saved the safety volumes {name}"), report.put()));
                            self.save_message = Some((format!("saved as \"{name}\""), false));
                            if let Some(Ok(rep)) = &mut self.job.result {
                                rep.saved = Some(name);
                            }
                        }
                        Err(e) => self.save_message = Some((e, true)),
                    }
                }
            }
        });
        if let Some((m, error)) = &self.save_message {
            crate::menu::message(
                ui,
                egui::RichText::new(m).small(),
                error.then_some(ui.visuals().warn_fg_color),
            );
        }
    }

    fn export_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        ui.horizontal(|ui| {
            ui.label("Export");
            for (f, name, hover) in [
                (
                    Format::Dxf,
                    "DXF + .prj",
                    "3D faces in the line's coordinates; the coordinate system as WKT beside it",
                ),
                (
                    Format::Ifc,
                    "IFC4",
                    "One element per volume with its properties; georeferenced by a map \
                     conversion to the projected system (EPSG code)",
                ),
                (
                    Format::Kml,
                    "KML",
                    "WGS84 with absolute heights (the line's, not corrected to a geoid)",
                ),
                (Format::Kmz, "KMZ", "The KML zipped"),
            ] {
                let mut on = self.formats.contains(&f);
                if ui.checkbox(&mut on, name).on_hover_text(hover).changed() {
                    if on {
                        self.formats.push(f);
                    } else {
                        self.formats.retain(|x| *x != f);
                    }
                }
            }
            if ui.button("Export to a folder…").clicked()
                && let Some(dir) = rfd::FileDialog::new().pick_folder()
            {
                self.export_message = Some(match self.export_to(model, &dir) {
                    Ok(m) => (m, false),
                    Err(e) => (e, true),
                });
            }
        });
        ui.label(
            egui::RichText::new(format!(
                "In the line's coordinate system: {}",
                if model.line.crs.is_empty() {
                    "none set (KML needs one)"
                } else {
                    model.line.crs.as_str()
                }
            ))
            .small()
            .weak(),
        );
        if let Some((m, error)) = &self.export_message {
            crate::menu::message(
                ui,
                egui::RichText::new(m).small(),
                error.then_some(ui.visuals().warn_fg_color),
            );
        }
    }
}

/// What the app does with the window's asks.
pub fn handle(r: &mut crate::app::Ready, out: Out) {
    if let Some((what, command)) = out.apply {
        r.perform(crate::edit::Action::Command { what, command });
    }
    if let Some(s) = out.status {
        r.status = Some(s);
    }
}

/// The 3D layer: each volume's triangles translucent in its colour (its
/// circuit's), dimmed while out of date.
pub fn scene(origin: [f64; 3], overlay: &Overlay) -> ohl_render::Scene {
    let mut s = ohl_render::Scene::new(origin);
    let Some(v) = &overlay.volumes else {
        return s;
    };
    for (vol, rgb) in v.volumes.iter().zip(&overlay.colors) {
        let c = crate::revision::dimmed([rgb[0], rgb[1], rgb[2], 70], overlay.stale);
        for t in &vol.mesh.triangles {
            s.triangle(vol.mesh.corners(t), [c; 3]);
        }
    }
    s
}

/// The plan: each volume's outline seen from above, in its colour.
pub fn paint_plan(
    painter: &egui::Painter,
    rect: egui::Rect,
    to_screen: &impl Fn([f64; 3]) -> egui::Pos2,
    overlay: &Overlay,
) {
    for (edges, rgb) in overlay.outlines.iter().zip(&overlay.colors) {
        let c = crate::revision::dimmed32(
            egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
            overlay.stale,
        );
        let stroke = egui::Stroke::new(1.0, c);
        for [a, b] in edges {
            let (p, q) = (to_screen(*a), to_screen(*b));
            if rect.contains(p) || rect.contains(q) {
                painter.line_segment([p, q], stroke);
            }
        }
    }
}

/// The saved form of a run: its volumes (name, circuit, voltage,
/// distances, statistics) and, up to [`MAX_SAVED_TRIANGLES`], their
/// meshes packed ([`ohl_session::packing`]: vertices in mm as differences,
/// triangle corners as differences).
pub mod saved {
    use super::*;

    fn pack_mesh(m: &Mesh) -> Result<String, String> {
        let mm = |v: f64| (v * 1000.0).round() as i64;
        let mut w = Writer::default();
        w.u(m.vertices.len() as u64);
        for k in 0..3 {
            w.deltas(m.vertices.iter().map(|p| mm(p[k])));
        }
        w.u(m.triangles.len() as u64);
        for k in 0..3 {
            w.deltas(m.triangles.iter().map(|t| i64::from(t[k])));
        }
        ohl_session::packing::pack(&w.0)
    }

    fn unpack_mesh(text: &str) -> Result<Mesh, String> {
        let raw = ohl_session::packing::unpack(text, 1 << 31)?;
        let mut r = Reader(&raw);
        let n = usize::try_from(r.u()?).map_err(|e| e.to_string())?;
        if n > raw.len() {
            return Err(format!("{n} vertices in {} bytes", raw.len()));
        }
        let cols: Vec<Vec<i64>> = (0..3).map(|_| r.deltas(n)).collect::<Result<_, _>>()?;
        let vertices = (0..n)
            .map(|i| [0, 1, 2].map(|k| cols[k][i] as f64 / 1000.0))
            .collect();
        let m = usize::try_from(r.u()?).map_err(|e| e.to_string())?;
        if m > raw.len() {
            return Err(format!("{m} triangles in {} bytes", raw.len()));
        }
        let cols: Vec<Vec<i64>> = (0..3).map(|_| r.deltas(m)).collect::<Result<_, _>>()?;
        let mut triangles = Vec::with_capacity(m);
        for ((&a, &b), &c) in cols[0].iter().zip(&cols[1]).zip(&cols[2]) {
            let t = [a, b, c];
            if t.iter().any(|&x| x < 0 || x as usize >= n) {
                return Err("a triangle's corner out of range".into());
            }
            triangles.push(t.map(|x| x as u32));
        }
        if !r.done() {
            return Err("bytes left over".into());
        }
        Ok(Mesh {
            vertices,
            triangles,
        })
    }

    /// The results of a run (`meshes`: with them).
    pub fn results(v: &SafetyVolumes, meshes: bool) -> Result<Value, String> {
        let list: Vec<Value> = v
            .volumes
            .iter()
            .map(|s| -> Result<Value, String> {
                Ok(json!({
                    "name": s.name,
                    "circuit": s.circuit,
                    "phases": s.phases,
                    "sections": s.sections,
                    "kv": s.kv,
                    "distance": s.distance,
                    "margin": s.margin,
                    "outer": s.outer,
                    "positions": s.positions,
                    "arc_error": s.arc_error,
                    "arc_error_median": s.arc_error_median,
                    "allowance": s.allowance,
                    "cell": s.stats.cell,
                    "level": s.stats.level,
                    "triangles": s.mesh.triangles.len(),
                    "volume_m3": s.mesh.volume(),
                    "seconds": s.seconds,
                    "mesh": if meshes { Some(pack_mesh(&s.mesh)?) } else { None },
                }))
            })
            .collect::<Result<_, _>>()?;
        Ok(json!({
            "volumes": list,
            "positions": v.positions,
            "notes": v.notes,
        }))
    }

    /// Read back from [`results`] (the cases come from the parameters).
    pub fn read(v: &Value) -> Result<SafetyVolumes, String> {
        let num = |o: &Value, k: &str| o.get(k).and_then(Value::as_f64).unwrap_or(0.0);
        let text = |o: &Value, k: &str| o.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
        let list = |o: &Value, k: &str| -> Vec<String> {
            o.get(k)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut volumes = Vec::new();
        for o in v
            .get("volumes")
            .and_then(Value::as_array)
            .ok_or("no volumes in the saved run")?
        {
            let mesh = match o.get("mesh").and_then(Value::as_str) {
                Some(t) => unpack_mesh(t)?,
                None => Mesh::default(),
            };
            volumes.push(SafetyVolume {
                name: text(o, "name"),
                circuit: text(o, "circuit"),
                phases: list(o, "phases"),
                sections: list(o, "sections"),
                kv: o.get("kv").and_then(Value::as_f64),
                distance: num(o, "distance"),
                margin: num(o, "margin"),
                outer: num(o, "outer"),
                positions: num(o, "positions") as usize,
                arc_error: num(o, "arc_error"),
                arc_error_median: num(o, "arc_error_median"),
                simplified: Default::default(),
                timings: [0.0; 2],
                allowance: num(o, "allowance"),
                stats: MeshStats {
                    cell: num(o, "cell"),
                    level: num(o, "level"),
                    ..MeshStats::default()
                },
                mesh,
                seconds: num(o, "seconds"),
            });
        }
        Ok(SafetyVolumes {
            volumes,
            cases: Vec::new(),
            positions: v.get("positions").and_then(Value::as_u64).unwrap_or(0) as usize,
            notes: list(v, "notes"),
        })
    }
}
