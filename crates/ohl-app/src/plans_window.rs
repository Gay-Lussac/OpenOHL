// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Export › Plans (owner, 2026-10-24: a menu of their own, apart from the
//! plan-and-profile sheets): the **route plan** and the **situation
//! plans** (north up, over the project's drawings and orthophotos) and the
//! **phase plan** (the structures where the phase arrangement changes),
//! written for every route of the Drawings window with its sheet template
//! and title fields ([`crate::drawings::run_plans`]).

use std::path::PathBuf;

use eframe::egui;

use crate::drawings::{Input, PlanKind, PlanRequest, run_plans};
use crate::model::Model;
use crate::survey_io::Job;

/// The papers offered.
const PAPERS: [&str; 5] = ["A0", "A1", "A2", "A3", "A4"];

/// The Plans window.
#[derive(Debug)]
pub struct PlansWindow {
    /// Shown.
    pub open: bool,
    /// The plan.
    pub kind: PlanKind,
    /// Paper.
    pub page: String,
    /// The route plan's scale (1:…).
    pub route_scale: f64,
    /// The situation plans' scale (1:…).
    pub situation_scale: f64,
    /// The project's drawings under the route and situation plans.
    pub drawings: bool,
    /// The orthophotos under them.
    pub photo: bool,
    /// The corridor drawn (at the drawings' line width).
    pub corridor: bool,
    /// Formats: `pdf`, `svg`, `png`, `dxf`.
    pub formats: Vec<String>,
    /// Output folder.
    pub out: Option<PathBuf>,
    /// Why the last start failed.
    pub error: Option<String>,
    /// The export under way, or done.
    pub job: Option<Job>,
}

impl Default for PlansWindow {
    fn default() -> Self {
        Self {
            open: false,
            kind: PlanKind::Situation,
            page: "A3".into(),
            route_scale: 10_000.0,
            situation_scale: 1000.0,
            drawings: true,
            photo: true,
            corridor: true,
            formats: vec!["pdf".into()],
            out: None,
            error: None,
            job: None,
        }
    }
}

impl PlansWindow {
    /// Opens the window on plan `kind`.
    pub fn show(&mut self, kind: PlanKind) {
        self.kind = kind;
        self.open = true;
    }

    /// The request of the window's settings.
    pub fn request(&self) -> PlanRequest {
        PlanRequest {
            kind: self.kind,
            page: self.page.clone(),
            scale: match self.kind {
                PlanKind::Route => self.route_scale,
                _ => self.situation_scale,
            },
            drawings: self.drawings,
            photo: self.photo,
            corridor: self.corridor,
        }
    }

    /// The window; `drawings` the Drawings window's settings (routes,
    /// template, title fields, cases) the plans are made with.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        model: &Model,
        case: usize,
        drawings: &crate::drawings::DrawingsWindow,
    ) {
        if let Some(job) = &mut self.job
            && crate::survey_io::job_window(ctx, job)
        {
            self.job = None;
        }
        if !self.open {
            return;
        }
        let mut open = true;
        let mut start = false;
        egui::Window::new("Plans")
            .open(&mut open)
            .collapsible(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(
                        "Plans beside the plan-and-profile sheets, for every route, with the \
                         sheet template, title fields and routes of Export › Drawings….",
                    )
                    .small()
                    .weak(),
                );
                ui.horizontal(|ui| {
                    for k in [PlanKind::Route, PlanKind::Situation, PlanKind::Phase] {
                        ui.selectable_value(&mut self.kind, k, k.title());
                    }
                });
                ui.label(
                    egui::RichText::new(match self.kind {
                        PlanKind::Route => {
                            "The whole route north up at a small scale, over the project's \
                             drawings (e.g. municipal boundaries) and orthophotos: the axis, \
                             the wires, the structures numbered, the corridor, a north arrow, \
                             a scale bar and a legend."
                        }
                        PlanKind::Situation => {
                            "The route in parts north up at a large scale, the sheets \
                             overlapping, over the project's drawings (the cadastral survey: \
                             parcels and their numbers) and orthophotos."
                        }
                        PlanKind::Phase => {
                            "Each structure's figure seen along the line, every wire labelled \
                             with its circuit (numbered, a key) and phase — only where the \
                             arrangement changes; between, \"arrangement unchanged\"."
                        }
                    })
                    .small(),
                );
                egui::Grid::new("plans-settings")
                    .num_columns(2)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Paper");
                        egui::ComboBox::from_id_salt("plans-paper")
                            .selected_text(self.page.as_str())
                            .show_ui(ui, |ui| {
                                for p in PAPERS {
                                    ui.selectable_value(&mut self.page, p.to_owned(), p);
                                }
                            });
                        ui.end_row();
                        if self.kind != PlanKind::Phase {
                            ui.label("Scale 1:");
                            let (scale, range) = if self.kind == PlanKind::Route {
                                (&mut self.route_scale, 5000.0..=25_000.0)
                            } else {
                                (&mut self.situation_scale, 500.0..=2000.0)
                            };
                            ui.add(egui::DragValue::new(scale).range(range).speed(50.0));
                            ui.end_row();
                            ui.label("Under it");
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut self.drawings, "the project's drawings");
                                ui.checkbox(&mut self.photo, "orthophotos");
                                ui.checkbox(&mut self.corridor, "the corridor");
                            });
                            ui.end_row();
                        }
                        ui.label("Formats");
                        ui.horizontal(|ui| {
                            for f in ["pdf", "svg", "png", "dxf"] {
                                let mut on = self.formats.iter().any(|x| x == f);
                                if ui.checkbox(&mut on, f.to_uppercase()).changed() {
                                    if on {
                                        self.formats.push(f.into());
                                    } else {
                                        self.formats.retain(|x| x != f);
                                    }
                                }
                            }
                        });
                        ui.end_row();
                        ui.label("Folder");
                        ui.horizontal(|ui| {
                            if ui.button("Choose…").clicked()
                                && let Some(p) = rfd::FileDialog::new().pick_folder()
                            {
                                self.out = Some(p);
                            }
                            let shown = self
                                .out
                                .as_ref()
                                .or(drawings.out.as_ref())
                                .map_or("—".to_owned(), |p| p.display().to_string());
                            ui.label(egui::RichText::new(shown).small());
                        });
                        ui.end_row();
                    });
                if let Some(e) = &self.error {
                    crate::menu::error_label(ui, e);
                }
                start = ui
                    .add_enabled(self.job.is_none(), egui::Button::new("Export"))
                    .clicked();
            });
        if start {
            let out = self.out.clone().or_else(|| drawings.out.clone());
            match out {
                None => self.error = Some("choose a folder".into()),
                Some(out) if self.formats.is_empty() => {
                    let _ = out;
                    self.error = Some("tick a format".into());
                }
                Some(out) => {
                    let mut w = drawings.clone();
                    w.formats = self
                        .formats
                        .iter()
                        .filter_map(|f| ohl_drawing::export::Format::parse(f))
                        .collect();
                    let req = self.request();
                    match Input::for_plans(model, case, &w, out, &req) {
                        Ok(input) => {
                            self.error = None;
                            self.job = Some(Job::start(
                                format!("Exporting the {}", req.kind.title().to_lowercase()),
                                false,
                                ctx,
                                move |report| run_plans(&input, &req, report),
                            ));
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
            }
        }
        self.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Export › Plans on the example line: the route plan, the situation
    /// plans and the phase plan written as PDF for every route, with the
    /// Drawings window's settings.
    #[test]
    fn the_plans_are_written() {
        let model = crate::tests::synthetic();
        let w = crate::drawings::DrawingsWindow::default();
        let dir = std::env::temp_dir().join(format!("ohl-app-{}-plans", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for kind in [PlanKind::Route, PlanKind::Situation, PlanKind::Phase] {
            let mut win = PlansWindow::default();
            win.show(kind);
            let req = win.request();
            let mut ww = w.clone();
            ww.formats = vec![ohl_drawing::export::Format::Pdf];
            let input = Input::for_plans(&model, 0, &ww, dir.clone(), &req).unwrap();
            let done = run_plans(&input, &req, &|_, _| {}).unwrap();
            assert!(done.contains("files written"), "{done}");
        }
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        for what in ["route-plan.pdf", "situation-plan.pdf", "phase-plan.pdf"] {
            assert!(names.iter().any(|n| n.ends_with(what)), "{what}: {names:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
