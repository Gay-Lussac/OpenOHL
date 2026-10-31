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
use crate::check_ui;
use crate::job::Job;
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
    /// The circuits, routes and sections fitted (owner, 2026-10-28), kept
    /// in the project ([`SETTING`]).
    pub scope: check_ui::Scope,
    kept: check_ui::Kept,
    pub(crate) job: Job<Vec<Row>>,
}

/// The project setting the window's scope is kept in, per project line.
pub const SETTING: &str = "app.wirefit";

impl Default for WireFitWindow {
    fn default() -> Self {
        Self {
            open: false,
            temperature: 15.0,
            params: FitParams::default(),
            estimate: true,
            scope: check_ui::Scope::default(),
            kept: check_ui::Kept::default(),
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

    /// Reads the scope from the project when not read yet for the line
    /// worked on, and writes it into it when it changed (project setting
    /// `app.wirefit.<line>`, owner, 2026-10-28: the setups kept in the
    /// project).
    pub fn keep(&mut self, model: &mut Model, ctx: &egui::Context) {
        let json = serde_json::to_string(&self.scope).unwrap_or_default();
        let mut kept = std::mem::take(&mut self.kept);
        if let check_ui::Step::Load(text) =
            kept.step(model, ctx, SETTING, "as-built wires scope", &json)
        {
            self.scope = text
                .as_deref()
                .and_then(|t| serde_json::from_str(t).ok())
                .unwrap_or_default();
            kept.loaded(serde_json::to_string(&self.scope).unwrap_or_default());
        }
        self.kept = kept;
    }

    fn start(&mut self, model: &Model, ctx: &egui::Context) {
        let Some(spec) = model.cloud.clone() else {
            self.job.result = Some(Err(check_ui::NO_CLOUD.into()));
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
        // The scope's sections only (none: the whole line).
        let keep = self.scope.keeps(&model.line);
        self.job.start_at(ctx, model.revision(), move || {
            let case = ohl_weather::WeatherCase::still(
                "model",
                ohl_core::units::Temperature::from_celsius(t),
            );
            let shapes: Vec<_> = ohl_line::solve::solve_line(&line, &library, &case)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|s| keep.as_ref().is_none_or(|k| k.contains(&s.section)))
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
                        log::warn!("{} span {}: {e}", w.section, w.span + 1);
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
        let why = model
            .cloud
            .is_none()
            .then(|| check_ui::NO_CLOUD.to_owned())
            .or_else(|| self.scope.why_not(&model.line));
        let scope_words = check_ui::scope_words(&model.line, &self.scope, true);
        let findings = match &self.job.result {
            Some(Ok(r)) => Some(r.iter().filter(|r| matches!(r.fit, Some(Ok(_)))).count()),
            _ => None,
        };
        help::window(
            ctx,
            crate::dockable::Panel::AsBuiltWires,
            "As-built wires",
            [860.0, 760.0],
            &mut open,
            |ui| {
                if let Some(what) = &stale {
                    again = crate::revision::banner(ui, what, !self.job.running());
                }
                // Two tabs (owner, 2026-10-27): the setup scrolls, the
                // results fill theirs; Run in the footer, always visible.
                let tab = check_ui::tabs(
                    ui,
                    WINDOW,
                    findings,
                    self.job.generation,
                    why.is_some() && !self.job.running(),
                );
                match tab {
                    check_ui::Tab::Setup => help::body(ui, WINDOW, |ui| {
                        check_ui::section(ui, 1, "What it checks", None);
                        help::explain(
                            ui,
                            "Catenaries fitted to the LiDAR points of the wire classes, span by \
                             span, against the model in still air.",
                            "The conductor temperature at the survey from the model's sags; \
                             the difference of the sag and of the mid-span height between the \
                             fitted wire and the model.",
                        );
                        ui.separator();
                        check_ui::section(ui, 2, "Scope", None);
                        check_ui::ScopePicker::new(WINDOW, &model.line)
                            .wires(&mut self.scope, true)
                            .ui(ui);
                        ui.separator();
                        // The modelling settings, folded (check pages'
                        // guidelines), a labelled row each.
                        check_ui::model_section(ui, WINDOW, 3, |ui| self.settings_ui(ui));
                    }),
                    check_ui::Tab::Results => check_ui::results_area(ui, WINDOW, |ui| {
                        let rows = match &self.job.result {
                            None => {
                                ui.label(
                                    egui::RichText::new("No run yet: Run at the bottom.").weak(),
                                );
                                return;
                            }
                            Some(Err(e)) => {
                                crate::menu::error_label(ui, e);
                                return;
                            }
                            Some(Ok(r)) => r,
                        };
                        let fitted = rows.iter().filter(|r| matches!(r.fit, Some(Ok(_)))).count();
                        let temps: Vec<f64> = rows.iter().filter_map(|r| r.temperature).collect();
                        ui.horizontal_wrapped(|ui| {
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
                        });
                        use check_ui::{Cell, Col};
                        let table: Vec<check_ui::Row> = rows
                            .iter()
                            .map(|r| {
                                let span = Cell::span(&r.wire.section, r.wire.span);
                                let cells = match &r.fit {
                                    None => vec![span, Cell::text("too few points")],
                                    Some(Err(e)) => vec![span, Cell::text(e).bad(true)],
                                    Some(Ok(f)) => vec![
                                        span,
                                        Cell::sorted(f.fit.points.to_string(), f.fit.points as f64),
                                        Cell::num(Some(f.fit.rms), 3),
                                        Cell::num(Some(f.sag), 3),
                                        Cell::num(Some(f.model_sag), 3),
                                        Cell::num(Some(f.sag - f.model_sag), 3),
                                        Cell::num(Some(f.mid_difference), 3),
                                        Cell::num(r.temperature, 1),
                                    ],
                                };
                                check_ui::Row::new(cells, false)
                            })
                            .collect();
                        if let Some(k) = check_ui::ResultTable::new(
                            "wirefit-rows",
                            vec![
                                Col::text("Span"),
                                Col::num("Points"),
                                Col::num("rms [m]"),
                                Col::num("Sag [m]"),
                                Col::num("Model sag [m]"),
                                Col::num("Δsag [m]"),
                                Col::num("Δmid [m]"),
                                Col::num("Temperature [°C]"),
                            ],
                        )
                        .rows(table)
                        .no_filter()
                        .csv("as-built-table.csv")
                        .ui(ui)
                        {
                            let r = &rows[k];
                            go = Some(Selection::Span {
                                section: r.wire.section.clone(),
                                span: r.wire.span,
                            });
                        }
                    }),
                }
                help::footer(ui, WINDOW, |ui| {
                    again |= check_ui::job_footer(
                        ui,
                        &mut self.job,
                        why.clone(),
                        "Read the wire points and fit a catenary in every span",
                        Some(scope_words),
                        |_| {},
                    );
                });
            },
        );
        self.open = open;
        if again && why.is_none() {
            self.start(model, ctx);
        }
        go
    }
}

/// The id of the window's body, tabs and footer.
const WINDOW: &str = "wirefit-window";

impl WireFitWindow {
    /// *3 Model*: the model's temperature, which points belong to a wire,
    /// the temperature at the survey — a labelled row each, what it
    /// changes under it.
    fn settings_ui(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("wirefit-settings")
            .num_columns(2)
            .spacing(help::GRID_SPACING)
            .show(ui, |ui| {
                ui.label("Model temperature");
                crate::style::field(
                    ui,
                    egui::DragValue::new(&mut self.temperature)
                        .speed(0.5)
                        .suffix(" °C"),
                );
                ui.end_row();
                ui.label("Points across");
                crate::style::field(
                    ui,
                    egui::DragValue::new(&mut self.params.lateral)
                        .speed(0.05)
                        .range(0.05..=10.0)
                        .suffix(" m"),
                );
                ui.end_row();
                ui.label("Points up or down");
                crate::style::field(
                    ui,
                    egui::DragValue::new(&mut self.params.vertical)
                        .speed(0.1)
                        .range(0.1..=30.0)
                        .suffix(" m"),
                );
                ui.end_row();
                ui.label("Survey temperature");
                ui.checkbox(&mut self.estimate, "estimate it");
                ui.end_row();
            });
        help::explain(
            ui,
            "Model temperature: the model is solved in still air at it, for the sags and \
             heights compared.",
            "The fitted wires are compared with the model solved in still air at this conductor \
             temperature: the difference of the sag and of the mid-span height. It does not \
             change the fit itself.",
        );
        help::explain(
            ui,
            "Points across, up or down: how far from the modelled wire a LiDAR point of the wire \
             classes may lie to be fitted to it.",
            "Across: the largest distance in plan from the span's vertical plane; up or down: the \
             largest height difference from the modelled wire. Larger takes in points of a \
             neighbouring wire or of vegetation; smaller leaves out a wire that hangs far from \
             the model.",
        );
        help::explain(
            ui,
            "Survey temperature: the conductor temperature at the survey, from the model's sag \
             at each temperature.",
            "Per span fitted, the temperature at which the model's sag equals the fitted sag \
             (searched from -30 to 120 °C, about six section solves a span): longer runs when \
             ticked.",
        );
    }
}
