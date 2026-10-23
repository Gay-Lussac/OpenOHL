// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Thermal rating: per section and ambient case (`.ohlta`), the
//! ampacity at a maximum conductor temperature (computed as the window is
//! drawn: microseconds each); on demand, the clearance-limited temperature
//! of every section ([`ohl_thermal::limit`]) — solved again at other
//! temperatures in a weather case, checked by the ground and point rules
//! of the chosen rule file — in the background with progress, and the
//! ampacity at it. The model is not changed.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use eframe::egui;
use ohl_clearance::{Rule, Target};
use ohl_core::budget::CancelToken;
use ohl_thermal::limit::{Limit, LimitOptions, SectionLimit, Targets};
use ohl_thermal::{AmbientCases, Conductor, Method};

use crate::check_help as help;
use crate::model::Model;

/// What its results are computed from ([`crate::revision`]).
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::ALL;

type Outcome = Result<Vec<SectionLimit>, String>;

/// A clearance-limit search running.
struct Job {
    rx: Receiver<Outcome>,
    /// Sections done.
    done: Arc<AtomicUsize>,
    sections: usize,
    cancel: CancelToken,
    started: Instant,
}

/// The Thermal rating window's state.
pub struct ThermalWindow {
    /// The window is open.
    pub open: bool,
    /// The ambient case file.
    pub file: Option<PathBuf>,
    /// Its cases, or why they could not be read.
    pub ambient: Option<Result<AmbientCases, String>>,
    /// Rating method.
    pub method: Method,
    /// Maximum conductor temperature, °C.
    pub max_temperature: f64,
    /// The weather case solved at other temperatures (index in the
    /// model's cases).
    pub base_case: usize,
    /// Temperature range and resolution of the search.
    pub options: LimitOptions,
    /// The last search's results (or why there are none).
    pub limits: Option<Outcome>,
    /// How long it took.
    pub took: Option<Duration>,
    job: Option<Job>,
    /// What the last search was computed from ([`crate::revision`]).
    stamp: crate::revision::Stamp,
}

impl Default for ThermalWindow {
    fn default() -> Self {
        Self {
            open: false,
            file: None,
            ambient: None,
            method: Method::default(),
            max_temperature: 80.0,
            base_case: 0,
            options: LimitOptions::default(),
            limits: None,
            took: None,
            job: None,
            stamp: Default::default(),
        }
    }
}

impl ThermalWindow {
    /// Opens the window; the first time with the project's `.ohlta` file
    /// (next to a line file), if there is one.
    pub fn show(&mut self, model: &Model) {
        self.open = true;
        if self.file.is_none()
            && let Some(p) = model.project_files("ambient").into_iter().next()
        {
            self.choose(p);
        }
    }

    /// Reads ambient case file `path`.
    pub fn choose(&mut self, path: PathBuf) {
        self.ambient = Some(AmbientCases::load(&path));
        self.file = Some(path);
    }

    /// Whether a search runs.
    pub fn running(&self) -> bool {
        self.job.is_some()
    }

    /// Takes a finished search.
    pub fn poll(&mut self) {
        let Some(job) = &self.job else {
            return;
        };
        let r = match job.rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("the search stopped".into()),
        };
        self.took = Some(job.started.elapsed());
        match &r {
            Ok(v) => log::info!(
                "thermal: clearance limits of {} sections in {:.1?}",
                v.len(),
                job.started.elapsed()
            ),
            Err(e) => log::warn!("thermal: {e}"),
        }
        self.limits = Some(r);
        self.stamp.arrived();
        self.job = None;
    }

    /// What the clearance limits were computed from that changed since,
    /// in words (`None`: nothing, or none computed): the line, the LiDAR
    /// points, the ground ([`crate::revision`]).
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        matches!(self.limits, Some(Ok(_)))
            .then(|| self.stamp.out_of_date(model, USES))
            .flatten()
    }

    /// Starts the clearance-limit search on the model as it is now.
    pub fn start(&mut self, model: &Model, rules: Vec<Rule>, ctx: &egui::Context) {
        let Some(base) = model.cases.get(self.base_case).map(|c| c.case.clone()) else {
            return;
        };
        let line = model.line.clone();
        let library = model.library.clone();
        let ground = model.ground_for_checks();
        // The surfaces the ground rules name (read on the search's thread).
        let named =
            crate::model::NamedSurfaces::of(model, rules.iter().filter_map(|r| r.surface.clone()));
        let cloud = model.cloud.clone();
        let scheme = model.scheme.clone();
        let corridor = model.sources.corridor;
        let options = self.options;
        let done = Arc::new(AtomicUsize::new(0));
        let cancel = CancelToken::new();
        let (tx, rx) = channel();
        let (c, d, token) = (ctx.clone(), done.clone(), cancel.clone());
        let sections = line.sections.len();
        self.stamp.started(model.revision());
        std::thread::spawn(move || {
            let r = (|| -> Outcome {
                let classes: Vec<Vec<u8>> = rules
                    .iter()
                    .map(|r| r.point_classes(|role| scheme.codes_with_role(role)))
                    .collect();
                // The points of the point rules' classes, read once.
                let wanted: Vec<u8> = rules
                    .iter()
                    .zip(&classes)
                    .filter(|(r, _)| {
                        matches!(r.target, Target::Points { .. })
                            && ohl_clearance::applies(r, &base.name)
                    })
                    .flat_map(|(_, c)| c.iter().copied())
                    .collect();
                let mut points: Vec<(f64, f64, f64, u8)> = Vec::new();
                if let (Some(spec), false) = (&cloud, wanted.is_empty()) {
                    let (file, name) = ohl_lidar::cloud_spec(spec);
                    let source =
                        ohl_pointcloud::CloudSource::open(file, name).map_err(|e| e.to_string())?;
                    let corridor = ohl_lidar::Corridor::along_spans(&line, corridor);
                    // Exit clause: ~1 s per 10⁷ points decoded.
                    let budget = ohl_core::budget::Budget::new()
                        .time_limit(Duration::from_secs(600))
                        .cancel_token(token.clone());
                    ohl_pointcloud::PointSource::visit(
                        &source,
                        Some(&corridor),
                        Some(&wanted),
                        &budget,
                        &mut |x, y, z, k| points.push((x, y, z, k)),
                    )?;
                }
                let mut feed = |_: &[u8], visit: &mut dyn FnMut(f64, f64, f64, u8)| {
                    for &(x, y, z, k) in &points {
                        visit(x, y, z, k);
                    }
                    Ok(())
                };
                let named = named.read();
                let surfaces: Vec<(String, &ohl_terrain::Terrain)> =
                    named.iter().map(|(n, t)| (n.clone(), t.as_ref())).collect();
                let mut targets = Targets {
                    ground: ground.as_deref(),
                    surfaces: &surfaces,
                    point_classes: &classes,
                    points: (!points.is_empty()).then_some(&mut feed as _),
                };
                let budget = ohl_thermal::limit::default_budget(&options, line.sections.len())
                    .cancel_token(token);
                ohl_thermal::limit::clearance_limits(
                    &line,
                    &library,
                    &base,
                    &rules,
                    &mut targets,
                    &options,
                    &budget,
                    &mut |k, _| {
                        d.store(k, Ordering::Relaxed);
                        c.request_repaint();
                    },
                )
            })();
            let _ = tx.send(r);
            c.request_repaint();
        });
        self.limits = None;
        self.job = Some(Job {
            rx,
            done,
            sections,
            cancel,
            started: Instant::now(),
        });
    }
}

/// "812 A", or why there is no rating.
fn rating(
    method: Method,
    conductor: &Result<Conductor, String>,
    ambient: &ohl_thermal::Ambient,
    t: f64,
) -> (String, String) {
    match conductor {
        Ok(c) => match ohl_thermal::ampacity(method, c, ambient, t) {
            Ok(b) => (format!("{:.0} A", b.current), terms(&b)),
            Err(e) => ("—".into(), e.to_string()),
        },
        Err(e) => ("—".into(), e.clone()),
    }
}

fn terms(b: &ohl_thermal::HeatBalance) -> String {
    format!(
        "Joule {:.1}, solar {:.1}, convection {:.1} ({}), radiation {:.1} W/m",
        b.joule,
        b.solar.heat,
        b.convection,
        b.convection_mode.as_str(),
        b.radiation
    )
}

/// The Thermal rating window.
pub fn window(
    ctx: &egui::Context,
    w: &mut ThermalWindow,
    model: &Model,
    rules: Option<&Result<Vec<Rule>, String>>,
) {
    w.poll();
    if !w.open {
        return;
    }
    let mut open = true;
    egui::Window::new("Thermal rating")
        .open(&mut open)
        .resizable(true)
        .default_width(640.0)
        .default_height(600.0)
        .show(ctx, |ui| {
            // Run in the footer at the bottom left, always visible (owner,
            // 2026-10-21); the rest scrolls above it.
            help::body(ui, "thermal-window", |ui| body(ui, w, model, rules));
            help::footer(ui, "thermal-window", |ui| run_ui(ui, w, model, rules));
        });
    w.open = open;
}

/// The window's body (above the Run footer): the ambient cases, the
/// method, the clearance-limit settings, the table.
fn body(
    ui: &mut egui::Ui,
    w: &mut ThermalWindow,
    model: &Model,
    rules: Option<&Result<Vec<Rule>, String>>,
) {
    ui.horizontal(|ui| {
        ui.label("Ambient cases");
        let name = w.file.as_ref().map_or("none".to_owned(), |p| {
            p.file_name()
                .map_or(String::new(), |n| n.to_string_lossy().into_owned())
        });
        ui.label(egui::RichText::new(name).strong());
        if ui
            .button("Choose…")
            .on_hover_text("An ambient case file (.ohlta): air, wind, sun")
            .clicked()
            && let Some(p) = rfd::FileDialog::new()
                .add_filter(
                    "Thermal ambient cases",
                    &ohl_core::files::open_extensions(ohl_thermal::ambient::AMBIENT_FILE_EXTENSION),
                )
                .pick_file()
        {
            w.choose(p);
        }
    });
    ui.horizontal(|ui| {
        ui.label("Method");
        egui::ComboBox::from_id_salt("thermal-method")
            .selected_text(w.method.title())
            .show_ui(ui, |ui| {
                for m in Method::ALL {
                    ui.selectable_value(&mut w.method, m, m.title());
                }
            });
        ui.label("Maximum temperature");
        ui.add(
            egui::DragValue::new(&mut w.max_temperature)
                .range(-20.0..=300.0)
                .suffix(" °C"),
        );
    });
    let cases = match &w.ambient {
        Some(Ok(set)) => set.cases.clone(),
        Some(Err(e)) => {
            ui.colored_label(ui.visuals().error_fg_color, e);
            Vec::new()
        }
        None => {
            ui.label(
                egui::RichText::new("Choose an ambient case file (.ohlta).")
                    .small()
                    .weak(),
            );
            Vec::new()
        }
    };
    for c in &cases {
        ui.label(
            egui::RichText::new(format!("{}: {}", c.name, c.summary()))
                .small()
                .weak(),
        );
    }
    ui.separator();

    // Clearance limit controls (fixed while a search runs).
    let running = w.running();
    ui.add_enabled_ui(!running, |ui| {
        ui.horizontal(|ui| {
            ui.label("Weather case heated");
            let name = |k: usize| model.cases.get(k).map_or("", |c| c.case.name.as_str());
            egui::ComboBox::from_id_salt("thermal-base")
                .selected_text(name(w.base_case))
                .show_ui(ui, |ui| {
                    for k in 0..model.cases.len() {
                        ui.selectable_value(&mut w.base_case, k, name(k));
                    }
                });
            ui.label("from");
            ui.add(
                egui::DragValue::new(&mut w.options.min_temperature)
                    .range(-50.0..=w.options.max_temperature - 1.0)
                    .suffix(" °C"),
            );
            ui.label("to");
            ui.add(
                egui::DragValue::new(&mut w.options.max_temperature)
                    .range(w.options.min_temperature + 1.0..=400.0)
                    .suffix(" °C"),
            );
        })
    });
    if let Some(Err(e)) = &w.limits {
        ui.colored_label(ui.visuals().error_fg_color, e);
    }
    if let Some(what) = w.out_of_date(model)
        && crate::revision::banner(ui, &what, !w.running())
        && let Some(Ok(r)) = rules
    {
        w.start(model, r.clone(), ui.ctx());
    }
    ui.separator();
    table(ui, w, model, &cases);
}

/// The Run footer: the clearance-limited temperature of every section
/// (Stop and the progress while it runs).
fn run_ui(
    ui: &mut egui::Ui,
    w: &mut ThermalWindow,
    model: &Model,
    rules: Option<&Result<Vec<Rule>, String>>,
) {
    let usable = match rules {
        Some(Ok(r)) => Some(r.clone()),
        _ => None,
    };
    if let Some(job) = &w.job {
        let done = job.done.load(Ordering::Relaxed);
        ui.add(
            egui::ProgressBar::new(done as f32 / job.sections.max(1) as f32)
                .text(format!("section {done} of {}", job.sections))
                .desired_width(240.0),
        );
        if help::stop_button(ui, "Stop the search: the previous results stay") {
            job.cancel.cancel();
        }
    } else if help::run_button(
        ui,
        true,
        "The clearance-limited temperature: per section, the highest conductor temperature at \
         which the ground and point rules of the rule file are met (to 0.5 °C)",
        usable
            .is_none()
            .then_some("Choose a rule file: Checks › Wire clearances › Rule file"),
    ) && let Some(r) = usable
    {
        w.start(model, r, ui.ctx());
    }
    if let Some(t) = w.took {
        ui.label(egui::RichText::new(format!("{t:.1?}")).small().weak());
    }
}

/// Per section: cable, ampacity at the maximum temperature per case, the
/// clearance limit and the ampacity at it per case.
fn table(ui: &mut egui::Ui, w: &ThermalWindow, model: &Model, cases: &[ohl_thermal::Ambient]) {
    let limits: &[SectionLimit] = match &w.limits {
        Some(Ok(v)) => v,
        _ => &[],
    };
    egui::ScrollArea::both().max_height(420.0).show(ui, |ui| {
        egui::Grid::new("thermal-table")
            .striped(true)
            .show(ui, |ui| {
                ui.strong("section");
                ui.strong("wire type");
                for c in cases {
                    ui.strong(format!("{} at {} °C", c.name, w.max_temperature));
                }
                if !limits.is_empty() {
                    ui.strong("clearance limit");
                    ui.strong("governing");
                    for c in cases {
                        ui.strong(format!("{} at limit", c.name));
                    }
                }
                ui.end_row();
                for s in &model.line.sections {
                    ui.label(&s.id);
                    ui.label(&s.cable);
                    let conductor = model
                        .library
                        .get(&s.cable)
                        .ok_or_else(|| format!("wire type {} not in the library", s.cable))
                        .and_then(|c| {
                            Conductor::from_cable(c)
                                .map(|c| c.with_defaults(&model.line.defaults))
                                .map_err(|e| e.to_string())
                        });
                    for c in cases {
                        let (text, hover) = rating(w.method, &conductor, c, w.max_temperature);
                        ui.label(text).on_hover_text(hover);
                    }
                    if let Some(l) = limits.iter().find(|l| l.section == s.id) {
                        let (t, text) = match &l.result {
                            Ok(Limit::At(t)) => (Some(*t), format!("{t:.1} °C")),
                            Ok(Limit::AboveRange) => (
                                Some(w.options.max_temperature),
                                format!("above {} °C", w.options.max_temperature),
                            ),
                            Ok(Limit::BelowRange) => {
                                (None, format!("below {} °C", w.options.min_temperature))
                            }
                            Err(e) => (None, format!("error: {e}")),
                        };
                        ui.label(text);
                        ui.label(l.governing.as_ref().map_or(String::new(), |g| {
                            format!(
                                "{}: span {} {} {:.2} < {:.2} m",
                                g.rule,
                                g.span + 1,
                                g.measure,
                                g.clearance,
                                g.required
                            )
                        }));
                        for c in cases {
                            match t {
                                Some(t) => {
                                    let (text, hover) = rating(w.method, &conductor, c, t);
                                    ui.label(text).on_hover_text(hover);
                                }
                                None => {
                                    ui.label("—");
                                }
                            }
                        }
                    }
                    ui.end_row();
                }
            });
    });
}
