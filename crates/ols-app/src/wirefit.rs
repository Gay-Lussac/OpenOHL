// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Checks › As-built wires: catenaries fitted to the points of the wire
//! classes (roles `conductor`, `shield_wire`), span by span, compared with
//! the model solved in still air at a chosen temperature; and the
//! conductor temperature at the survey from the model's sag–temperature
//! relation ([`ols_clearance::wirefit`]). On a background thread.

use eframe::egui;
use ols_clearance::wirefit::{FitParams, SpanFit, WireCollector};
use ols_clearance::{Wire, wires_of};

use crate::job::{Job, or_dash, save_csv};
use crate::model::Model;
use crate::selection::Selection;

/// One span's result.
#[derive(Debug, Clone)]
pub struct Row {
    /// The modelled wire.
    pub wire: Wire,
    /// The fit, or why there is none (`None`: too few points).
    pub fit: Option<Result<SpanFit, String>>,
    /// Temperature at the survey, °C.
    pub temperature: Option<f64>,
}

/// The Wire fit window.
pub struct WireFitWindow {
    /// Shown.
    pub open: bool,
    /// Model temperature, °C.
    pub temperature: f64,
    /// Which points belong to a span.
    pub params: FitParams,
    /// Estimate the temperature at the survey.
    pub estimate: bool,
    job: Job<Vec<Row>>,
}

impl Default for WireFitWindow {
    fn default() -> Self {
        Self {
            open: false,
            temperature: 15.0,
            params: FitParams::default(),
            estimate: true,
            job: Job::default(),
        }
    }
}

impl WireFitWindow {
    /// The last run's answer, and whether one runs.
    #[cfg(test)]
    pub fn result(&self) -> (Option<&Result<Vec<Row>, String>>, bool) {
        (self.job.result.as_ref(), self.job.running())
    }

    fn start(&mut self, model: &Model, ctx: &egui::Context) {
        let Some(spec) = model.cloud.clone() else {
            self.job.result = Some(Err("no point cloud in this model".into()));
            return;
        };
        let mut classes: Vec<u8> = ["conductor", "shield_wire"]
            .iter()
            .flat_map(|r| model.scheme.codes_with_role(r))
            .collect();
        classes.sort_unstable();
        classes.dedup();
        if classes.is_empty() {
            self.job.result = Some(Err(format!(
                "no class of \"{}\" has the role conductor or shield_wire",
                model.scheme.name
            )));
            return;
        }
        let line = model.line.clone();
        let library = model.library.clone();
        let params = self.params.clone();
        let (t, estimate) = (self.temperature, self.estimate);
        self.job.start(ctx, move || {
            let case = ols_weather::WeatherCase::still(
                "model",
                ols_core::units::Temperature::from_celsius(t),
            );
            let shapes: Vec<_> = ols_line::solve::solve_line(&line, &library, &case)
                .into_iter()
                .filter_map(Result::ok)
                .collect();
            let mut c = WireCollector::new(wires_of(&line, &shapes, &[]), &classes, params.clone());
            let (file, name) = ols_lidar::cloud_spec(&spec);
            let source =
                ols_pointcloud::CloudSource::open(file, name).map_err(|e| e.to_string())?;
            let corridor = ols_lidar::Corridor::along_spans(&line, params.lateral + 2.0);
            // Exit clause: ~1 s per 10⁷ points decoded; 10 min is far beyond.
            let budget =
                ols_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(600));
            ols_pointcloud::PointSource::visit(
                &source,
                Some(&corridor),
                Some(&classes),
                &budget,
                &mut |x, y, z, k| c.visit(x, y, z, k),
            )?;
            let fits = c.fit(&ols_core::budget::Budget::new().max_iterations(500));
            let mut rows = Vec::new();
            for (w, fit) in c.wires().iter().zip(fits) {
                let mut temperature = None;
                if estimate
                    && let Some(Ok(f)) = &fit
                    && let Some(section) = line.sections.iter().find(|s| s.id == w.section)
                    && let Some(cable) = library.get(&section.cable)
                {
                    // Exit clause: about 6 section solves are usual.
                    let budget = ols_core::budget::Budget::new().max_iterations(60);
                    temperature = ols_clearance::wirefit::estimate_temperature(
                        &line,
                        section,
                        cable,
                        w.span,
                        f.sag,
                        (-30.0, 120.0),
                        &budget,
                    )
                    .unwrap_or_else(|e| {
                        log::warn!("{} span {}: {e}", w.section, w.span);
                        None
                    });
                }
                rows.push(Row {
                    wire: w.clone(),
                    fit,
                    temperature,
                });
            }
            Ok(rows)
        });
    }

    /// The window; returns a span to go to.
    pub fn ui(&mut self, ctx: &egui::Context, model: &Model) -> Option<Selection> {
        self.job.poll();
        let mut go = None;
        let mut open = self.open;
        egui::Window::new("As-built wires")
            .open(&mut open)
            .default_width(800.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("model at °C");
                    ui.add(egui::DragValue::new(&mut self.temperature).speed(0.5));
                    ui.label("points within m: across");
                    ui.add(
                        egui::DragValue::new(&mut self.params.lateral)
                            .speed(0.05)
                            .range(0.05..=10.0),
                    );
                    ui.label("up/down");
                    ui.add(
                        egui::DragValue::new(&mut self.params.vertical)
                            .speed(0.1)
                            .range(0.1..=30.0),
                    );
                    ui.checkbox(&mut self.estimate, "temperature at the survey");
                    if ui
                        .add_enabled(!self.job.running(), egui::Button::new("Fit"))
                        .on_hover_text("Read the wire points and fit a catenary in every span")
                        .clicked()
                    {
                        self.start(model, ui.ctx());
                    }
                    self.job.status_ui(ui);
                });
                let rows = match &self.job.result {
                    None => {
                        ui.label(
                            "Catenaries fitted to the LiDAR points of the wire classes, span by \
                             span, against the model in still air; the conductor temperature \
                             at the survey from the model's sags.",
                        );
                        return;
                    }
                    Some(Err(e)) => {
                        ui.colored_label(egui::Color32::LIGHT_RED, e);
                        return;
                    }
                    Some(Ok(r)) => r,
                };
                ui.horizontal(|ui| {
                    let fitted = rows.iter().filter(|r| matches!(r.fit, Some(Ok(_)))).count();
                    let temps: Vec<f64> = rows.iter().filter_map(|r| r.temperature).collect();
                    ui.label(format!(
                        "fitted {fitted} of {} spans{}",
                        rows.len(),
                        if temps.is_empty() {
                            String::new()
                        } else {
                            format!(
                                "; survey temperature: mean {:.1} °C",
                                temps.iter().sum::<f64>() / temps.len() as f64
                            )
                        }
                    ));
                    if ui.button("Export CSV…").clicked() {
                        save_csv("as-built-wires.csv", &csv(rows));
                    }
                });
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Grid::new("wirefit-rows")
                            .striped(true)
                            .spacing([12.0, 3.0])
                            .show(ui, |ui| {
                                for h in [
                                    "span",
                                    "points",
                                    "rms m",
                                    "sag m",
                                    "model sag m",
                                    "Δsag m",
                                    "Δmid m",
                                    "°C",
                                ] {
                                    ui.strong(h);
                                }
                                ui.end_row();
                                for r in rows {
                                    if ui
                                        .link(format!("{} span {}", r.wire.section, r.wire.span))
                                        .on_hover_text("Go to this span")
                                        .clicked()
                                    {
                                        go = Some(Selection::Span {
                                            section: r.wire.section.clone(),
                                            span: r.wire.span,
                                        });
                                    }
                                    match &r.fit {
                                        None => {
                                            ui.label("too few points");
                                        }
                                        Some(Err(e)) => {
                                            ui.colored_label(egui::Color32::LIGHT_RED, e);
                                        }
                                        Some(Ok(f)) => {
                                            ui.label(f.fit.points.to_string());
                                            for v in [
                                                f.fit.rms,
                                                f.sag,
                                                f.model_sag,
                                                f.sag - f.model_sag,
                                                f.mid_difference,
                                            ] {
                                                ui.label(
                                                    egui::RichText::new(format!("{v:.3}"))
                                                        .monospace(),
                                                );
                                            }
                                            ui.label(or_dash(r.temperature, 1));
                                        }
                                    }
                                    ui.end_row();
                                }
                            });
                    });
            });
        self.open = open;
        go
    }
}

fn csv(rows: &[Row]) -> String {
    use std::fmt::Write as _;
    let mut out = String::from(
        "section,span,points,rms_m,sag_m,model_sag_m,mid_difference_m,temperature_c\n",
    );
    for r in rows {
        if let Some(Ok(f)) = &r.fit {
            let _ = writeln!(
                out,
                "{},{},{},{:.4},{:.3},{:.3},{:.3},{}",
                ols_core::text::csv_field(&r.wire.section),
                r.wire.span,
                f.fit.points,
                f.fit.rms,
                f.sag,
                f.model_sag,
                f.mid_difference,
                r.temperature.map_or(String::new(), |t| format!("{t:.2}"))
            );
        }
    }
    out
}
