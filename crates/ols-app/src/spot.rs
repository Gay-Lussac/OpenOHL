// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Structures › Spot structures: the least-cost structures along a route of the
//! model ([`ols_spotting`]) on the model's ground, from its structure types
//! and a cost file, computed in the background (cancellable). The proposal
//! is listed and saved as a new line file; the model is not changed.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use ols_core::budget::CancelToken;
use ols_spotting::Spotting;
use ols_spotting::route::{RouteOptions, RouteProblem};

use crate::model::Model;

type Answer = Result<(RouteProblem, Spotting, ols_line::Line), String>;

/// The Spot structures window.
pub struct SpotWindow {
    /// Shown.
    pub open: bool,
    route: String,
    cable: String,
    tension_kn: f64,
    temperature: f64,
    clearance: f64,
    step: f64,
    /// Longest span, m (0: from the types).
    max_span: f64,
    /// The cost file.
    pub cost_path: Option<PathBuf>,
    running: Option<(CancelToken, Receiver<Answer>)>,
    /// The last answer.
    pub result: Option<Answer>,
    status: String,
}

impl Default for SpotWindow {
    fn default() -> Self {
        Self {
            open: false,
            route: String::new(),
            cable: String::new(),
            tension_kn: 20.0,
            temperature: 80.0,
            clearance: 7.0,
            step: 10.0,
            max_span: 0.0,
            cost_path: None,
            running: None,
            result: None,
            status: String::new(),
        }
    }
}

impl SpotWindow {
    /// Opens the window with the model's first route and cable.
    pub fn open(&mut self, model: &Model) {
        self.open = true;
        let routes = model.line.alignment_views();
        if !routes.iter().any(|r| r.id == self.route) {
            self.route = routes.first().map(|r| r.id.clone()).unwrap_or_default();
        }
        if self.cable.is_empty() {
            self.cable = model
                .line
                .sections
                .first()
                .map(|s| s.cable.clone())
                .unwrap_or_default();
        }
        if self.cost_path.is_none() {
            self.cost_path = model.project_files("costs").into_iter().next();
        }
    }

    /// Whether a run is going.
    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    fn options(&self) -> RouteOptions {
        RouteOptions {
            route: self.route.clone(),
            cable: self.cable.clone(),
            horizontal_tension: self.tension_kn * 1000.0,
            temperature: self.temperature,
            clearance: self.clearance,
            step: self.step,
            max_span: (self.max_span > 0.0).then_some(self.max_span),
            min_span: 0.0,
            forbidden: Vec::new(),
            conductors: None,
        }
    }

    /// Runs the optimiser on the model as it is now, in the background.
    pub fn start(&mut self, model: &Model, ctx: &egui::Context) {
        let fail = |m: &str| Some(Err(m.to_owned()));
        let Some(ground) = model.ground.clone() else {
            self.result = fail("no ground: Terrain › open survey data or a surface first");
            return;
        };
        let Some((types, insulators)) = model.types.clone() else {
            self.result = fail("no structure types: the line uses none (Structures › Libraries)");
            return;
        };
        let Some(cost_path) = self.cost_path.clone() else {
            self.result = fail("choose a cost file (.ohlcost) first");
            return;
        };
        let line = model.line.clone();
        let cables = model.library.clone();
        let options = self.options();
        let token = CancelToken::new();
        let budget = ols_spotting::default_budget().cancel_token(token.clone());
        let (tx, rx) = channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let r = (|| {
                let cost = ols_bom::CostData::load(&cost_path)?;
                let rp = ols_spotting::route::route_problem(
                    &line,
                    &ground,
                    &types,
                    &insulators,
                    &cables,
                    &cost,
                    &options,
                )?;
                let s = ols_spotting::spot(&rp.problem, &budget).map_err(|e| e.to_string())?;
                let spotted = ols_spotting::route::spotted_line(&line, &rp, &s, &types, &options);
                Ok((rp, s, spotted))
            })();
            let _ = tx.send(r);
            c.request_repaint();
        });
        self.running = Some((token, rx));
        self.result = None;
    }

    fn poll(&mut self) {
        let Some((_, rx)) = &self.running else {
            return;
        };
        let r = match rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err("the run stopped".into()),
        };
        self.result = Some(r);
        self.running = None;
    }

    /// The window, when open.
    pub fn show(&mut self, ctx: &egui::Context, model: &Model) {
        if !self.open {
            return;
        }
        self.poll();
        let mut open = true;
        egui::Window::new("Spot structures")
            .open(&mut open)
            .default_width(680.0)
            .show(ctx, |ui| self.ui(ui, model));
        self.open = open;
        if !open && let Some((token, _)) = &self.running {
            token.cancel();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        ui.label(
            egui::RichText::new(
                "The least-cost structures along a route: types and extensions priced in the \
                 cost file, the wire's ground clearance kept at the design tension, the types' \
                 allowable spans. The model is not changed: the proposal is saved as a new line.",
            )
            .small()
            .weak(),
        );
        egui::Grid::new("spot-inputs")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Route");
                egui::ComboBox::from_id_salt("spot-route")
                    .selected_text(&self.route)
                    .show_ui(ui, |ui| {
                        for r in model.line.alignment_views() {
                            ui.selectable_value(&mut self.route, r.id.clone(), &r.id);
                        }
                    });
                ui.end_row();
                ui.label("Cable");
                egui::ComboBox::from_id_salt("spot-cable")
                    .selected_text(&self.cable)
                    .show_ui(ui, |ui| {
                        for c in model.library.iter() {
                            ui.selectable_value(&mut self.cable, c.id.clone(), &c.id);
                        }
                    });
                ui.end_row();
                let number =
                    |ui: &mut egui::Ui, label: &str, v: &mut f64, unit: &str, hint: &str| {
                        ui.label(label).on_hover_text(hint);
                        ui.add(
                            egui::DragValue::new(v)
                                .speed(0.5)
                                .range(0.0..=1e6)
                                .suffix(unit),
                        );
                        ui.end_row();
                    };
                number(
                    ui,
                    "Design tension",
                    &mut self.tension_kn,
                    " kN",
                    "Horizontal tension in the maximum sag condition",
                );
                number(
                    ui,
                    "Design temperature",
                    &mut self.temperature,
                    " °C",
                    "The spotted sections are strung to the tension at it (still air)",
                );
                number(
                    ui,
                    "Ground clearance",
                    &mut self.clearance,
                    " m",
                    "Smallest vertical distance",
                );
                number(
                    ui,
                    "Step",
                    &mut self.step,
                    " m",
                    "Distance between candidate stations",
                );
                number(
                    ui,
                    "Longest span",
                    &mut self.max_span,
                    " m",
                    "0: twice the largest allowable wind span of the types",
                );
                ui.label("Cost file");
                ui.horizontal(|ui| {
                    let name = self.cost_path.as_ref().map_or("— none —".into(), |p| {
                        p.file_name()
                            .map_or(String::new(), |n| n.to_string_lossy().into_owned())
                    });
                    ui.strong(name);
                    if ui.button("Choose…").clicked()
                        && let Some(p) = rfd::FileDialog::new()
                            .add_filter(
                                "Unit costs",
                                &ols_core::files::open_extensions(
                                    ols_bom::cost::COST_FILE_EXTENSION,
                                ),
                            )
                            .pick_file()
                    {
                        self.cost_path = Some(p);
                    }
                });
                ui.end_row();
            });
        ui.horizontal(|ui| {
            if self.running() {
                ui.spinner();
                if ui.button("Cancel").clicked()
                    && let Some((token, _)) = &self.running
                {
                    token.cancel();
                }
            } else if ui.button("Spot").clicked() {
                self.start(model, ui.ctx());
            }
            let ready = matches!(&self.result, Some(Ok(_)));
            if ui
                .add_enabled(ready, egui::Button::new("Save as line file…"))
                .on_hover_text("The proposal as a new line (.ohl.json), to open or import")
                .clicked()
                && let Some(Ok((_, _, line))) = &self.result
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("Line", &["json"])
                    .set_file_name("spotted.ohl.json")
                    .save_file()
            {
                self.status = match std::fs::write(&path, line.to_json()) {
                    Ok(()) => format!("wrote {}", path.display()),
                    Err(e) => format!("{}: {e}", path.display()),
                };
            }
        });
        if !self.status.is_empty() {
            ui.label(egui::RichText::new(&self.status).small().weak());
        }
        ui.separator();
        match &self.result {
            None => {}
            Some(Err(e)) => {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            Some(Ok((rp, s, _))) => {
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        egui::Grid::new("spot-result").striped(true).show(ui, |ui| {
                            for h in [
                                "#",
                                "station m",
                                "type",
                                "ext",
                                "angle °",
                                "span m",
                                "clearance m",
                            ] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for (k, p) in s.structures.iter().enumerate() {
                                let c = &rp.problem.candidates[p.candidate];
                                let span = s.spans.get(k);
                                ui.label(format!("{}", k + 1));
                                ui.label(format!("{:.1}", p.station));
                                ui.label(&c.structure_type);
                                ui.label(c.extension.as_deref().unwrap_or(""));
                                ui.label(format!("{:.1}", p.angle_deg));
                                ui.label(
                                    span.map_or(String::new(), |x| format!("{:.1}", x.length)),
                                );
                                ui.label(
                                    span.map_or(String::new(), |x| format!("{:.2}", x.clearance)),
                                );
                                ui.end_row();
                            }
                        });
                    });
                ui.strong(format!(
                    "{} structures: {:.2} (structures {:.2}, spans {:.2}, wire {:.2})",
                    s.structures.len(),
                    s.total,
                    s.structure_cost,
                    s.span_cost,
                    s.wire_cost
                ));
                ui.label(
                    egui::RichText::new(format!(
                        "{} candidate stations, {} states, {:.2?}",
                        s.stations, s.states, s.elapsed
                    ))
                    .small()
                    .weak(),
                );
                for n in &rp.notes {
                    ui.label(egui::RichText::new(n).small().weak());
                }
            }
        }
    }
}
