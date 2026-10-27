// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Electrical › **Magnetic field…** (roadmap O18 step 4, owner
//! 2026-10-27; built, in active development, not approved yet): the
//! magnetic flux density of the line's currents ([`ohl_electrical::magnetic`])
//! in a weather case — the currents per circuit and their load-flow
//! directions, earth wire currents, the profiles' height and width, the
//! stations, an optional limit, 3D or 2D, points from a CSV — and on Run
//! the profiles plotted (B against the distance from the axis), a table
//! per profile and per point with the limit's margin, the notes; Export
//! CSV. Out of date like the other checks when the line or the ground
//! changes. Not yet: isolines drawn in the views, the thermal rating as
//! the currents (in `ohl electrical magnetic --thermal`).
//!
//! The window's inputs are kept in the project, per project line
//! ([`SETTING`]; owner, 2026-10-27): the weather case, the currents and
//! directions, the earth wires, the profiles, the stations, the limit, 2D,
//! and the points (the CSV's text, so they travel with the project).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use eframe::egui;
use ohl_electrical::magnetic::{
    self, CircuitCurrent, GivenPoint, Heights2d, MagneticField, Model as FieldModel, Progress,
    Settings, StationSpec,
};

use crate::check_help as help;
use crate::job::{Job, save_csv};
use crate::model::Model;

/// What its results are computed from ([`crate::revision`]).
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::LINE_GROUND;

/// The answer: the field and the weather case.
#[derive(Debug, Clone)]
pub struct Answer {
    /// The field.
    pub field: MagneticField,
    /// The weather case's name.
    pub case: String,
}

/// Where the profiles are taken.
#[derive(Debug, Clone, PartialEq, Default)]
enum Stations {
    /// Every span at its lowest point.
    #[default]
    EverySpan,
    /// One span (index into the line's spans), at its lowest point or a
    /// fraction of it.
    One { span: usize, fraction: Option<f64> },
}

/// The Magnetic field window.
pub struct MagneticFieldWindow {
    /// Shown.
    pub open: bool,
    /// The weather case (index into the model's cases).
    pub case: usize,
    /// Current (A) and reverse per circuit, by its label.
    currents: BTreeMap<String, (f64, bool)>,
    earth_wires: bool,
    height: f64,
    width: f64,
    step: f64,
    stations: Stations,
    limit: Option<f64>,
    two_d: bool,
    mean_heights: bool,
    /// Points read from a CSV file, and its name.
    points: Option<(String, Vec<GivenPoint>)>,
    /// The points file's text (kept in the project).
    points_text: Option<String>,
    /// The project setting's key and the inputs as last read or written
    /// (none: not read yet).
    saved: Option<(String, String)>,
    /// The weather case came from the project (opening keeps it).
    case_kept: bool,
    /// Why the last points file could not be read.
    points_error: Option<String>,
    /// The profile shown in the plot.
    shown: usize,
    /// The run's progress and Stop.
    progress: Option<Arc<Progress>>,
    /// The run.
    pub(crate) job: Job<Answer>,
}

impl Default for MagneticFieldWindow {
    fn default() -> Self {
        Self {
            open: false,
            case: 0,
            currents: BTreeMap::new(),
            earth_wires: false,
            height: 1.0,
            width: 50.0,
            step: 1.0,
            stations: Stations::EverySpan,
            limit: None,
            two_d: false,
            mean_heights: false,
            points: None,
            points_text: None,
            saved: None,
            case_kept: false,
            points_error: None,
            shown: 0,
            progress: None,
            job: Job::default(),
        }
    }
}

/// The project setting holding the window's inputs (per project line:
/// `app.magnetic_field.<line>`).
pub const SETTING: &str = "app.magnetic_field";

/// The window's inputs as kept in the project (JSON).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Stored {
    case: Option<String>,
    currents: BTreeMap<String, (f64, bool)>,
    earth_wires: bool,
    height: Option<f64>,
    width: Option<f64>,
    step: Option<f64>,
    /// One span's stations: its structures and the place in it (none:
    /// every span at its lowest point).
    span: Option<(String, String, Option<f64>)>,
    limit: Option<f64>,
    two_d: bool,
    mean_heights: bool,
    /// The points file's name and text.
    points: Option<(String, String)>,
}

impl MagneticFieldWindow {
    /// The inputs as kept in the project.
    fn stored(&self, model: &Model) -> Stored {
        let span = match &self.stations {
            Stations::EverySpan => None,
            Stations::One { span, fraction } => {
                ohl_electrical::cross_section::line_spans(&model.line)
                    .get(*span)
                    .map(|s| (s.from.clone(), s.to.clone(), *fraction))
            }
        };
        Stored {
            case: model.cases.get(self.case).map(|c| c.case.name.clone()),
            currents: self.currents.clone(),
            earth_wires: self.earth_wires,
            height: Some(self.height),
            width: Some(self.width),
            step: Some(self.step),
            span,
            limit: self.limit,
            two_d: self.two_d,
            mean_heights: self.mean_heights,
            points: match (&self.points, &self.points_text) {
                (Some((name, _)), Some(text)) => Some((name.clone(), text.clone())),
                _ => None,
            },
        }
    }

    /// Takes the inputs kept in the project for the line worked on (after
    /// opening it, switching lines, or another user changing them); what
    /// is no longer there (a case, a span) falls back to the default.
    fn load(&mut self, model: &Model) {
        let Some(session) = model.session() else {
            return;
        };
        let key = format!("{SETTING}.{}", session.current_line());
        let text = session.setting(&key).ok().flatten();
        let st: Stored = text
            .as_deref()
            .and_then(|t| serde_json::from_str(t).ok())
            .unwrap_or_default();
        let d = Self::default();
        self.case_kept = false;
        if let Some(k) = st
            .case
            .as_ref()
            .and_then(|n| model.cases.iter().position(|c| &c.case.name == n))
        {
            self.case = k;
            self.case_kept = true;
        }
        self.currents = st.currents;
        self.earth_wires = st.earth_wires;
        self.height = st.height.unwrap_or(d.height);
        self.width = st.width.unwrap_or(d.width);
        self.step = st.step.unwrap_or(d.step);
        self.stations = st
            .span
            .and_then(|(from, to, fraction)| {
                ohl_electrical::cross_section::line_spans(&model.line)
                    .iter()
                    .position(|s| s.from == from && s.to == to)
                    .map(|span| Stations::One { span, fraction })
            })
            .unwrap_or_default();
        self.limit = st.limit;
        self.two_d = st.two_d;
        self.mean_heights = st.mean_heights;
        (self.points, self.points_text, self.points_error) = match st.points {
            Some((name, text)) => match magnetic::parse_points(&text) {
                Ok(p) => (Some((name, p)), Some(text), None),
                Err(e) => (
                    None,
                    None,
                    Some(format!("{name} (kept in the project): {e}")),
                ),
            },
            None => (None, None, None),
        };
        self.saved = serde_json::to_string(&self.stored(model))
            .ok()
            .map(|json| (key, json));
    }

    /// Reads the inputs from the project when not read yet for the line
    /// worked on, and writes them into it when they changed (each frame;
    /// not while a value is dragged or typed). A project this user
    /// cannot change keeps them for the session.
    pub fn keep(&mut self, model: &mut Model, ctx: &egui::Context) {
        let Some(line) = model.session().map(|s| s.current_line().to_owned()) else {
            return;
        };
        let key = format!("{SETTING}.{line}");
        if self.saved.as_ref().is_none_or(|(k, _)| *k != key) {
            self.load(model);
            return;
        }
        let Ok(json) = serde_json::to_string(&self.stored(model)) else {
            return;
        };
        if self.saved.as_ref().is_some_and(|(_, j)| *j == json)
            || ctx.input(|i| i.pointer.any_down())
            || ctx.memory(|m| m.focused().is_some())
        {
            return;
        }
        let op = ohl_server::Op::SetSetting {
            key: key.clone(),
            value: Some(json.clone()),
        };
        // (Read-only for this user: kept for the session.)
        let _ = model.project_op("magnetic field inputs", op);
        self.saved = Some((key, json));
    }

    /// Opens the window on the weather case shown.
    pub fn show(&mut self, case: usize) {
        if !self.open && !self.case_kept {
            self.case = case;
        }
        self.open = true;
    }

    /// Sets a circuit's current, A (tests).
    #[cfg(test)]
    pub fn set_current(&mut self, circuit: &str, amps: f64) {
        self.currents.insert(circuit.to_owned(), (amps, false));
    }

    /// A circuit's current, A (tests).
    #[cfg(test)]
    pub fn current_of(&self, circuit: &str) -> Option<f64> {
        self.currents.get(circuit).map(|c| c.0)
    }

    /// The limit, µT (tests).
    #[cfg(test)]
    pub fn limit(&self) -> Option<f64> {
        self.limit
    }

    /// Sets the limit, µT (tests).
    #[cfg(test)]
    pub fn set_limit(&mut self, limit: Option<f64>) {
        self.limit = limit;
    }

    /// What the last run was computed from that changed since, in words
    /// (`None`: nothing, or no answer).
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        matches!(self.job.result, Some(Ok(_)))
            .then(|| self.job.stamp.out_of_date(model, USES))
            .flatten()
    }

    /// The settings as the engine takes them.
    fn settings(&self, model: &Model) -> Settings {
        let spans = ohl_electrical::cross_section::line_spans(&model.line);
        let stations = match &self.stations {
            Stations::EverySpan => Vec::new(),
            Stations::One { span, fraction } => spans
                .get(*span)
                .map(|s| StationSpec {
                    from: s.from.clone(),
                    to: s.to.clone(),
                    fraction: *fraction,
                })
                .into_iter()
                .collect(),
        };
        Settings {
            model: if self.two_d {
                FieldModel::TwoD
            } else {
                FieldModel::ThreeD
            },
            heights_2d: if self.mean_heights {
                Heights2d::Mean
            } else {
                Heights2d::Station
            },
            currents: self
                .currents
                .iter()
                .map(|(c, &(amps, reverse))| CircuitCurrent {
                    circuit: c.clone(),
                    amps,
                    reverse,
                })
                .collect(),
            earth_wires: self.earth_wires,
            height: self.height,
            width: self.width,
            step: self.step,
            stations,
            grids: Vec::new(),
            points: self
                .points
                .as_ref()
                .map(|p| p.1.clone())
                .unwrap_or_default(),
            limit_ut: self.limit,
        }
    }

    /// Starts a run on the case's solved sections.
    pub fn start(&mut self, model: &Model, ctx: &egui::Context) {
        let Some(solved) = model.cases.get(self.case) else {
            self.job.result = Some(Err("no weather case in the model".into()));
            return;
        };
        let line = model.line.clone();
        let library = model.library.clone();
        let shapes = solved.shapes.clone();
        let case = solved.case.clone();
        let ground = model.ground_for_checks();
        let settings = self.settings(model);
        let progress = Arc::new(Progress::default());
        self.progress = Some(progress.clone());
        self.job.start_at(ctx, model.revision(), move || {
            let height = |x: f64, y: f64| ground.as_ref().and_then(|t| t.height(x, y));
            let g: Option<&dyn Fn(f64, f64) -> Option<f64>> = ground
                .as_ref()
                .map(|_| &height as &dyn Fn(f64, f64) -> Option<f64>);
            let src = ohl_electrical::Sources {
                line: &line,
                library: &library,
                shapes: &shapes,
                temperature: case.temperature,
                ground: g,
            };
            let mut field = magnetic::run(&src, &settings, Some(&progress))?;
            for id in line
                .sections
                .iter()
                .map(|s| &s.id)
                .filter(|id| !shapes.iter().any(|s| &s.section == *id))
            {
                field
                    .notes
                    .push(format!("section {id}: not solved in this case, left out"));
            }
            if g.is_none() {
                field.notes.push(
                    "ground: straight between the structures' feet and level across (no terrain surface)"
                        .into(),
                );
            }
            Ok(Answer {
                field,
                case: case.name.clone(),
            })
        });
    }

    /// The window.
    pub fn ui(&mut self, ctx: &egui::Context, model: &Model) {
        self.job.poll();
        if !self.job.running() {
            self.progress = None;
        }
        if !self.open {
            return;
        }
        let mut open = true;
        let mut run = false;
        let stale = self.out_of_date(model);
        help::window(
            ctx,
            crate::dockable::Panel::MagneticField,
            "Magnetic field",
            [900.0, 680.0],
            &mut open,
            |ui| {
                help::body(ui, "magnetic-field-window", |ui| {
                    ui.label(
                        egui::RichText::new(
                            "The magnetic flux density (µT, the rms of the vector) of the line's \
                             currents, from the wires where they hang in a weather case: 3D from \
                             the solved wires (Biot–Savart for every straight piece, bundles \
                             subconductor by subconductor) or 2D (infinite straight conductors). \
                             A limit is yours to give; none is built in. Built, in active \
                             development, not approved yet.",
                        )
                        .small()
                        .weak(),
                    );
                    self.inputs_ui(ui, model);
                    ui.separator();
                    self.results_ui(ui, &stale, &mut run);
                });
                help::footer(ui, "magnetic-field-window", |ui| {
                    let names: Vec<String> =
                        model.cases.iter().map(|c| c.case.name.clone()).collect();
                    ui.label("Weather case");
                    crate::job::case_combo(ui, "magnetic-field-case", &names, &mut self.case);
                    let why = if names.is_empty() {
                        Some("no weather case in the model")
                    } else if !self.currents.values().any(|c| c.0 > 0.0) {
                        Some("give a circuit a current")
                    } else {
                        None
                    };
                    if let Some(p) = &self.progress {
                        let total = p.total.load(Ordering::Relaxed).max(1);
                        let done = p.done.load(Ordering::Relaxed);
                        ui.add(
                            egui::ProgressBar::new(done as f32 / total as f32)
                                .text(format!("{done} of {total} points"))
                                .desired_width(220.0),
                        );
                        if help::stop_button(ui, "Stop: the previous results stay") {
                            p.cancel.store(true, Ordering::Relaxed);
                        }
                    } else {
                        run |= help::run_button(
                            ui,
                            !self.job.running(),
                            "Compute the field from the wires as solved in this weather case",
                            why,
                        );
                    }
                    self.job.status_ui(ui);
                });
            },
        );
        self.open = open;
        if run {
            self.start(model, ctx);
        }
    }

    fn inputs_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        let circuits = magnetic::circuits(&model.line);
        if circuits.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No section carries a system's phase: assign the systems (Line › Electrical \
                     systems) to get circuits.",
                )
                .color(ui.visuals().warn_fg_color),
            );
        }
        egui::Grid::new("magnetic-field-currents")
            .striped(true)
            .spacing(help::GRID_SPACING)
            .show(ui, |ui| {
                for h in ["circuit", "system", "current", "load flow"] {
                    ui.strong(h);
                }
                ui.end_row();
                for c in &circuits {
                    let label = c.label();
                    let entry = self.currents.entry(label.clone()).or_insert((0.0, false));
                    ui.label(&label);
                    ui.label(&c.system);
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut entry.0)
                            .range(0.0..=100_000.0)
                            .speed(5.0)
                            .suffix(" A"),
                    )
                    .on_hover_text(
                        "rms, each phase; balanced phases at the system's phase angles \
                         (L1 0°, L2 −120°, L3 +120° unless the system gives others)",
                    );
                    let mut along = !entry.1;
                    egui::ComboBox::from_id_salt(("magnetic-field-dir", &label))
                        .selected_text(if along {
                            "along the line"
                        } else {
                            "against the line"
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut along, true, "along the line");
                            ui.selectable_value(&mut along, false, "against the line");
                        })
                        .response
                        .on_hover_text(
                            "The load flow's direction: along the line (from its first structure \
                             to its last) or against it — every phase turned by 180°. Which \
                             combination governs is the ordinance's question",
                        );
                    entry.1 = !along;
                    ui.end_row();
                }
            });
        ui.add_space(4.0);
        egui::Grid::new("magnetic-field-settings")
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label("Model");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.two_d, false, "3D")
                        .on_hover_text(FieldModel::ThreeD.describe());
                    ui.selectable_value(&mut self.two_d, true, "2D")
                        .on_hover_text(
                            "Infinite straight parallel conductors: not valid near a line angle \
                             over 5°, near crossings, or on spans between differently arranged \
                             structures (warned)",
                        );
                    if self.two_d {
                        ui.checkbox(&mut self.mean_heights, "wires at the span's mean height")
                            .on_hover_text(
                                "Off: the wires where they cross the profile (at the lowest point \
                                 by default, the larger field). On: the span's mean height, as \
                                 the line parameters take it — less than under the lowest point",
                            );
                    }
                });
                ui.end_row();
                ui.label("Earth wires");
                ui.checkbox(&mut self.earth_wires, "carry the currents the phases induce")
                    .on_hover_text(
                        "An infinitely long earth wire earthed everywhere: I_e = −inv(Z_ee) · Z_ep · I_p per \
                         span (an approximation). Off: no current in the earth wires",
                    );
                ui.end_row();
                ui.label("Profiles");
                ui.horizontal(|ui| {
                    let spans = ohl_electrical::cross_section::line_spans(&model.line);
                    let mut every = self.stations == Stations::EverySpan;
                    ui.radio_value(&mut every, true, "every span's lowest point");
                    ui.radio_value(&mut every, false, "one span");
                    if every {
                        self.stations = Stations::EverySpan;
                    } else {
                        if self.stations == Stations::EverySpan {
                            self.stations = Stations::One {
                                span: 0,
                                fraction: None,
                            };
                        }
                        if let Stations::One { span, fraction } = &mut self.stations {
                            *span = (*span).min(spans.len().saturating_sub(1));
                            let name = |k: usize| {
                                spans
                                    .get(k)
                                    .map_or(String::new(), |s| format!("{}–{}", s.from, s.to))
                            };
                            egui::ComboBox::from_id_salt("magnetic-field-span")
                                .selected_text(name(*span))
                                .show_ui(ui, |ui| {
                                    for k in 0..spans.len() {
                                        ui.selectable_value(span, k, name(k));
                                    }
                                });
                            let mut lowest = fraction.is_none();
                            ui.checkbox(&mut lowest, "at its lowest point");
                            if lowest {
                                *fraction = None;
                            } else {
                                let f = fraction.get_or_insert(0.5);
                                crate::style::field(
                                    ui,
                                    egui::DragValue::new(f).range(0.0..=1.0).speed(0.01),
                                )
                                .on_hover_text("The place in the span from its first structure along the line (0–1)");
                            }
                        }
                    }
                });
                ui.end_row();
                ui.label("Across");
                ui.horizontal(|ui| {
                    ui.label("±");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut self.width)
                            .range(1.0..=2000.0)
                            .speed(1.0)
                            .suffix(" m"),
                    );
                    ui.label("every");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut self.step)
                            .range(0.1..=50.0)
                            .speed(0.1)
                            .suffix(" m"),
                    );
                    ui.label("at");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut self.height)
                            .range(0.0..=200.0)
                            .speed(0.1)
                            .suffix(" m"),
                    );
                    ui.label("above the ground");
                });
                ui.end_row();
                ui.label("Limit");
                ui.horizontal(|ui| {
                    let mut given = self.limit.is_some();
                    ui.checkbox(&mut given, "given")
                        .on_hover_text(
                            "Your limit, µT: the margin at every point and the limit line's \
                             extent per profile. None is built in (an ordinance's values come \
                             from its parameter file)",
                        );
                    if !given {
                        self.limit = None;
                    } else {
                        let l = self.limit.get_or_insert(1.0);
                        crate::style::field(
                            ui,
                            egui::DragValue::new(l)
                                .range(0.001..=10_000.0)
                                .speed(0.01)
                                .suffix(" µT"),
                        );
                    }
                });
                ui.end_row();
                ui.label("Points");
                ui.horizontal(|ui| {
                    if ui
                        .button("Points from CSV…")
                        .on_hover_text(
                            "A CSV file with a header naming x, y and z (elevation) or height \
                             (above the ground), optionally name: e.g. places of sensitive use",
                        )
                        .clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("CSV", &["csv", "txt"])
                            .pick_file()
                    {
                        match std::fs::read_to_string(&path)
                            .map_err(|e| e.to_string())
                            .and_then(|t| magnetic::parse_points(&t))
                        {
                            Ok(p) => {
                                let name = path
                                    .file_name()
                                    .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                                self.points = Some((name, p));
                                self.points_text = std::fs::read_to_string(&path).ok();
                                self.points_error = None;
                            }
                            Err(e) => self.points_error = Some(format!("{}: {e}", path.display())),
                        }
                    }
                    if let Some((name, p)) = &self.points {
                        ui.label(format!("{} points from {name}", p.len()));
                        if ui.small_button("Clear").clicked() {
                            self.points = None;
                            self.points_text = None;
                        }
                    }
                });
                ui.end_row();
            });
        if let Some(e) = &self.points_error {
            crate::menu::error_label(ui, e.as_str());
        }
    }

    fn results_ui(&mut self, ui: &mut egui::Ui, stale: &Option<String>, run: &mut bool) {
        let answer = match &self.job.result {
            None => {
                ui.label("Run to compute the field.");
                return;
            }
            Some(Err(e)) => {
                crate::menu::error_label(ui, e.as_str());
                return;
            }
            Some(Ok(a)) => a,
        };
        if let Some(what) = stale {
            *run |= crate::revision::banner(ui, what, !self.job.running());
        }
        let m = &answer.field;
        ui.horizontal(|ui| {
            ui.label(format!(
                "In \"{}\"; {}; {} m above the ground{}",
                answer.case,
                m.model.describe(),
                m.height,
                m.limit_ut
                    .map_or(String::new(), |l| format!("; limit {l} µT"))
            ));
            if ui.button("Export CSV…").clicked() {
                save_csv("magnetic-field.csv", &magnetic::csv(m));
            }
        });
        for c in &m.currents {
            ui.label(
                egui::RichText::new(format!(
                    "circuit {}: {:.1} A, {} Hz, {}",
                    c.circuit,
                    c.amps,
                    c.frequency_hz,
                    if c.reverse {
                        "against the line"
                    } else {
                        "along the line"
                    }
                ))
                .small(),
            );
        }
        for e in &m.earth_wires {
            ui.label(
                egui::RichText::new(format!(
                    "earth wire {} {}–{}: {:.1} A at {:.1}°",
                    e.section, e.from, e.to, e.amps, e.angle_deg
                ))
                .small(),
            );
        }
        for n in &m.notes {
            ui.label(egui::RichText::new(n).small().weak());
        }
        ui.label(
            egui::RichText::new(
                "Not yet: the isolines drawn in the 3D view and the cross-section.",
            )
            .small()
            .weak(),
        );
        if !m.profiles.is_empty() {
            ui.separator();
            self.shown = self.shown.min(m.profiles.len() - 1);
            ui.horizontal(|ui| {
                ui.strong("Profile");
                let name = |p: &magnetic::Profile| format!("{}–{} at {:.1} m", p.from, p.to, p.at);
                egui::ComboBox::from_id_salt("magnetic-field-profile")
                    .selected_text(name(&m.profiles[self.shown]))
                    .show_ui(ui, |ui| {
                        for (k, p) in m.profiles.iter().enumerate() {
                            ui.selectable_value(&mut self.shown, k, name(p));
                        }
                    });
            });
            plot(ui, &m.profiles[self.shown], m.limit_ut);
            help::list(
                ui,
                "magnetic-field-profiles",
                help::LIST_HEIGHT,
                egui::ScrollArea::vertical().id_salt("magnetic-field-profiles-scroll"),
                |ui| {
                    egui::Grid::new("magnetic-field-profiles")
                        .striped(true)
                        .spacing(help::GRID_SPACING)
                        .show(ui, |ui| {
                            for h in [
                                "span",
                                "station m",
                                "along the line m",
                                "max µT",
                                "at m",
                                "limit line m",
                                "",
                            ] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for (k, p) in m.profiles.iter().enumerate() {
                                if ui
                                    .selectable_label(
                                        k == self.shown,
                                        format!("{}–{}", p.from, p.to),
                                    )
                                    .clicked()
                                {
                                    self.shown = k;
                                }
                                ui.label(format!("{:.1}", p.at));
                                ui.label(format!("{:.1}", p.along_line));
                                ui.label(format!("{:.3}", p.max_ut));
                                ui.label(format!("{:.1}", p.max_offset));
                                ui.label(match p.limit {
                                    Some(l) => format!(
                                        "{:.1} … {:.1}{}",
                                        l.left,
                                        l.right,
                                        if l.beyond { " (beyond)" } else { "" }
                                    ),
                                    None if m.limit_ut.is_some() => "below the limit".into(),
                                    None => String::new(),
                                });
                                if p.warnings.is_empty() {
                                    ui.label("");
                                } else {
                                    ui.label(
                                        egui::RichText::new(format!("⚠ {}", p.warnings.len()))
                                            .color(ui.visuals().warn_fg_color),
                                    )
                                    .on_hover_text(p.warnings.join("\n"));
                                }
                                ui.end_row();
                            }
                        });
                },
            );
            for w in &m.profiles[self.shown].warnings {
                ui.label(
                    egui::RichText::new(w)
                        .small()
                        .color(ui.visuals().warn_fg_color),
                );
            }
        }
        if !m.points.is_empty() {
            ui.separator();
            ui.strong("Points");
            help::list(
                ui,
                "magnetic-field-points",
                help::LIST_HEIGHT,
                egui::ScrollArea::vertical().id_salt("magnetic-field-points-scroll"),
                |ui| {
                    egui::Grid::new("magnetic-field-points")
                        .striped(true)
                        .spacing(help::GRID_SPACING)
                        .show(ui, |ui| {
                            for h in [
                                "name",
                                "x m",
                                "y m",
                                "z m",
                                "height m",
                                "B µT",
                                "largest µT",
                                "margin µT",
                            ] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for p in &m.points {
                                ui.label(&p.name);
                                ui.label(format!("{:.2}", p.x));
                                ui.label(format!("{:.2}", p.y));
                                ui.label(format!("{:.2}", p.z));
                                ui.label(format!("{:.2}", p.height));
                                ui.label(format!("{:.4}", p.b_ut));
                                ui.label(format!("{:.4}", p.major_ut));
                                match p.margin_ut {
                                    Some(x) if x < 0.0 => {
                                        ui.label(
                                            egui::RichText::new(format!("{x:.4}"))
                                                .color(ui.visuals().error_fg_color),
                                        );
                                    }
                                    Some(x) => {
                                        ui.label(format!("{x:.4}"));
                                    }
                                    None => {
                                        ui.label("—");
                                    }
                                }
                                ui.end_row();
                            }
                        });
                },
            );
        }
    }
}

/// A profile's plot: B (and the cycle's largest value, fainter) against
/// the distance from the axis, the limit as a dashed line, the value under
/// the pointer.
fn plot(ui: &mut egui::Ui, p: &magnetic::Profile, limit: Option<f64>) {
    let width = ui.available_width().clamp(300.0, 900.0);
    let (response, painter) = ui.allocate_painter(egui::vec2(width, 240.0), egui::Sense::hover());
    let r = response.rect;
    let v = ui.visuals();
    let text = v.text_color();
    let faint = v.weak_text_color();
    let line = v.hyperlink_color;
    let font = egui::FontId::proportional(11.0);
    let frame = egui::Rect::from_min_max(
        r.min + egui::vec2(48.0, 8.0),
        r.max - egui::vec2(10.0, 24.0),
    );
    painter.rect_stroke(
        frame,
        0.0,
        egui::Stroke::new(1.0, faint),
        egui::StrokeKind::Inside,
    );
    let (Some(first), Some(last)) = (p.points.first(), p.points.last()) else {
        return;
    };
    let (x0, x1) = (first.offset, last.offset.max(first.offset + 1e-9));
    let top = p
        .points
        .iter()
        .map(|q| q.b_ut)
        .fold(limit.unwrap_or(0.0), f64::max)
        .max(1e-6)
        * 1.1;
    let to_screen = |x: f64, y: f64| {
        egui::pos2(
            frame.left() + ((x - x0) / (x1 - x0)) as f32 * frame.width(),
            frame.bottom() - (y / top) as f32 * frame.height(),
        )
    };
    // Ticks: 5 across, 4 up.
    for k in 0..=4 {
        let x = x0 + (x1 - x0) * f64::from(k) / 4.0;
        let at = to_screen(x, 0.0);
        painter.line_segment(
            [at, egui::pos2(at.x, frame.top())],
            egui::Stroke::new(0.5, faint.gamma_multiply(0.4)),
        );
        painter.text(
            egui::pos2(at.x, frame.bottom() + 3.0),
            egui::Align2::CENTER_TOP,
            format!("{x:.0}"),
            font.clone(),
            faint,
        );
        let y = top * f64::from(k) / 4.0;
        let at = to_screen(x0, y);
        painter.line_segment(
            [at, egui::pos2(frame.right(), at.y)],
            egui::Stroke::new(0.5, faint.gamma_multiply(0.4)),
        );
        painter.text(
            egui::pos2(frame.left() - 4.0, at.y),
            egui::Align2::RIGHT_CENTER,
            format!("{y:.2}"),
            font.clone(),
            faint,
        );
    }
    painter.text(
        egui::pos2(frame.left() + 4.0, frame.top() + 2.0),
        egui::Align2::LEFT_TOP,
        "B µT",
        font.clone(),
        text,
    );
    painter.text(
        egui::pos2(frame.right(), r.bottom()),
        egui::Align2::RIGHT_BOTTOM,
        "m from the axis (right +)",
        font.clone(),
        faint,
    );
    let major: Vec<egui::Pos2> = p
        .points
        .iter()
        .map(|q| to_screen(q.offset, q.major_ut))
        .collect();
    painter.add(egui::Shape::line(
        major,
        egui::Stroke::new(1.0, line.gamma_multiply(0.45)),
    ));
    let b: Vec<egui::Pos2> = p
        .points
        .iter()
        .map(|q| to_screen(q.offset, q.b_ut))
        .collect();
    painter.add(egui::Shape::line(b, egui::Stroke::new(2.0, line)));
    if let Some(l) = limit {
        let y = to_screen(x0, l).y;
        painter.extend(egui::Shape::dashed_line(
            &[egui::pos2(frame.left(), y), egui::pos2(frame.right(), y)],
            egui::Stroke::new(1.0, v.warn_fg_color),
            6.0,
            4.0,
        ));
        painter.text(
            egui::pos2(frame.right() - 4.0, y - 2.0),
            egui::Align2::RIGHT_BOTTOM,
            format!("limit {l} µT"),
            font.clone(),
            v.warn_fg_color,
        );
    }
    if let Some(pos) = response.hover_pos()
        && frame.contains(pos)
    {
        let x = x0 + f64::from((pos.x - frame.left()) / frame.width()) * (x1 - x0);
        if let Some(q) = p
            .points
            .iter()
            .min_by(|a, b| (a.offset - x).abs().total_cmp(&(b.offset - x).abs()))
        {
            let at = to_screen(q.offset, q.b_ut);
            painter.circle_filled(at, 3.0, line);
            painter.text(
                at + egui::vec2(6.0, -6.0),
                egui::Align2::LEFT_BOTTOM,
                format!(
                    "{:.1} m: {:.4} µT (largest over the cycle {:.4})",
                    q.offset, q.b_ut, q.major_ut
                ),
                font,
                text,
            );
        }
    }
}
