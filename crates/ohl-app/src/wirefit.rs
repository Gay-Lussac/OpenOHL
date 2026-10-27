// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › As-built wires: catenaries fitted to the points of the wire
//! classes (roles `conductor`, `shield_wire`), span by span, compared with
//! the model solved in still air at a chosen temperature; and the
//! conductor temperature at the survey from the model's sag–temperature
//! relation ([`ohl_clearance::wirefit`]). On a background thread.

use eframe::egui;
use ohl_clearance::wirefit::{FitParams, SpanFit, WireCollector};
use ohl_clearance::{Wire, wires_of};

use crate::check_help as help;
use crate::job::{Job, or_dash, save_csv};
use crate::model::Model;
use crate::selection::Selection;

/// What its results are computed from ([`crate::revision`]).
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::LINE_POINTS;

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
    pub(crate) job: Job<Vec<Row>>,
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
        self.job.start_at(ctx, model.revision(), move || {
            let case = ohl_weather::WeatherCase::still(
                "model",
                ohl_core::units::Temperature::from_celsius(t),
            );
            let shapes: Vec<_> = ohl_line::solve::solve_line(&line, &library, &case)
                .into_iter()
                .filter_map(Result::ok)
                .collect();
            let mut c = WireCollector::new(wires_of(&line, &shapes, &[]), &classes, params.clone());
            let (file, name) = ohl_lidar::cloud_spec(&spec);
            let source =
                ohl_pointcloud::CloudSource::open(file, name).map_err(|e| e.to_string())?;
            let corridor = ohl_lidar::Corridor::along_spans(&line, params.lateral + 2.0);
            // Exit clause: ~1 s per 10⁷ points decoded; 10 min is far beyond.
            let budget =
                ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(600));
            ohl_pointcloud::PointSource::visit(
                &source,
                Some(&corridor),
                Some(&classes),
                &budget,
                &mut |x, y, z, k| c.visit(x, y, z, k),
            )?;
            let fits = c.fit(&ohl_core::budget::Budget::new().max_iterations(500));
            let mut rows = Vec::new();
            for (w, fit) in c.wires().iter().zip(fits) {
                let mut temperature = None;
                if estimate
                    && let Some(Ok(f)) = &fit
                    && let Some(section) = line.sections.iter().find(|s| s.id == w.section)
                    && let Some(cable) = library.get(&section.cable)
                {
                    // Exit clause: about 6 section solves are usual.
                    let budget = ohl_core::budget::Budget::new().max_iterations(60);
                    temperature = ohl_clearance::wirefit::estimate_temperature(
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

    /// What the last fit was computed from that changed since, in words
    /// (`None`: nothing, or no fit): the line, the LiDAR points
    /// ([`crate::revision`]).
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        matches!(self.job.result, Some(Ok(_)))
            .then(|| self.job.stamp.out_of_date(model, USES))
            .flatten()
    }

    /// The window; returns a span to go to.
    pub fn ui(&mut self, ctx: &egui::Context, model: &Model) -> Option<Selection> {
        self.job.poll();
        let mut go = None;
        let mut again = false;
        let stale = self.out_of_date(model);
        let mut open = self.open;
        help::window(
            ctx,
            crate::dockable::Panel::AsBuiltWires,
            "As-built wires",
            [800.0, 600.0],
            &mut open,
            |ui| {
                // Run in the footer at the bottom left, always visible
                // (owner, 2026-10-21); the rest scrolls above it.
                help::body(ui, "wirefit-window", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("model at °C");
                        crate::style::field(
                            ui,
                            egui::DragValue::new(&mut self.temperature).speed(0.5),
                        );
                        ui.label("points within m: across");
                        crate::style::field(
                            ui,
                            egui::DragValue::new(&mut self.params.lateral)
                                .speed(0.05)
                                .range(0.05..=10.0),
                        );
                        ui.label("up/down");
                        crate::style::field(
                            ui,
                            egui::DragValue::new(&mut self.params.vertical)
                                .speed(0.1)
                                .range(0.1..=30.0),
                        );
                        ui.checkbox(&mut self.estimate, "temperature at the survey");
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
                            crate::menu::error_label(ui, e);
                            return;
                        }
                        Some(Ok(r)) => r,
                    };
                    if let Some(what) = &stale {
                        again = crate::revision::banner(ui, what, !self.job.running());
                    }
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
                                .spacing(help::GRID_SPACING)
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
                                            .link(format!(
                                                "{} span {}",
                                                r.wire.section, r.wire.span
                                            ))
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
                                                crate::menu::error_label(ui, e);
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
                help::footer(ui, "wirefit-window", |ui| {
                    again |= help::run_button(
                        ui,
                        !self.job.running(),
                        "Read the wire points and fit a catenary in every span",
                        None,
                    );
                    self.job.status_ui(ui);
                });
            },
        );
        self.open = open;
        if again {
            self.start(model, ctx);
        }
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
                ohl_core::text::csv_field(&r.wire.section),
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
