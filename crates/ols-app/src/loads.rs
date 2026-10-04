// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Checks › Structure loads: the loads the wires and strings put on the
//! structures in each load case (the load case file used: the project's,
//! Libraries › Load cases), computed on demand in the background — the
//! model is not changed. One load case shown at a time; factored or not;
//! a click goes to the structure; CSV export.

use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use eframe::egui;
use ols_loads::CaseLoads;

use crate::model::Model;

type Outcome = Result<Vec<CaseLoads>, String>;

/// The Structure loads page.
#[derive(Default)]
pub struct LoadsPage {
    /// The last results (or why there are none).
    pub results: Option<Outcome>,
    /// A computation running: since, its answer.
    running: Option<(Instant, Receiver<Outcome>)>,
    /// How long the last one took.
    pub took: Option<std::time::Duration>,
    /// The load case shown.
    pub case: usize,
    /// Structures shown (id contains this text).
    pub filter: String,
    /// Factored loads shown (else unfactored).
    pub unfactored: bool,
}

impl LoadsPage {
    /// Whether a computation runs.
    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    /// Computes the loads of the model as it is now (in the background).
    pub fn start(&mut self, model: &Model, ctx: &egui::Context) {
        let Some(path) = model.sources.load_cases.clone() else {
            self.results = Some(Err(
                "no load case file: Libraries › Load cases (a .olslc file next to the line, \
                 or the project's own)"
                    .into(),
            ));
            return;
        };
        let line = model.line.clone();
        let library = model.library.clone();
        let weather: Vec<_> = model.cases.iter().map(|c| c.case.clone()).collect();
        let (tx, rx) = channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let r = ols_loads::LoadCases::load(&path).and_then(|set| {
                ols_loads::structure_loads(
                    &line,
                    &library,
                    &weather,
                    &set.cases,
                    &ols_loads::default_budget(),
                )
            });
            let _ = tx.send(r);
            c.request_repaint();
        });
        self.running = Some((Instant::now(), rx));
    }

    /// Takes a finished computation.
    pub fn poll(&mut self) {
        let Some((t, rx)) = &self.running else {
            return;
        };
        let r = match rx.try_recv() {
            Ok(r) => Some(r),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Some(Err("the computation stopped".into()))
            }
        };
        if let Some(r) = r {
            self.took = Some(t.elapsed());
            if let Ok(v) = &r
                && self.case >= v.len()
            {
                self.case = 0;
            }
            self.results = Some(r);
            self.running = None;
        }
    }

    /// The page; returns a structure to go to.
    pub fn ui(&mut self, ui: &mut egui::Ui, model: &Model) -> Option<String> {
        let mut go = None;
        ui.horizontal(|ui| {
            let label = if self.results.is_some() {
                "Compute again"
            } else {
                "Compute"
            };
            if ui
                .add_enabled(!self.running(), egui::Button::new(label))
                .on_hover_text("Solve every section in every load case (finite elements)")
                .clicked()
            {
                self.start(model, ui.ctx());
            }
            match &model.sources.load_cases {
                Some(p) => ui.label(
                    egui::RichText::new(format!(
                        "load cases: {}",
                        p.file_name()
                            .map_or(String::new(), |n| n.to_string_lossy().into_owned())
                    ))
                    .small(),
                ),
                None => ui.label(egui::RichText::new("no load case file").small().weak()),
            };
            if let Some((t, _)) = &self.running {
                ui.spinner();
                ui.label(format!("{:.1} s", t.elapsed().as_secs_f64()));
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(200));
            } else if let Some(t) = self.took {
                ui.label(
                    egui::RichText::new(format!("computed in {t:.1?}"))
                        .small()
                        .weak(),
                );
            }
        });
        let results = match &self.results {
            None => {
                ui.label(
                    "The loads the wires and strings put on the structures, at every \
                     attachment, in each load case. Compute to see them.",
                );
                return None;
            }
            Some(Err(e)) => {
                ui.colored_label(egui::Color32::LIGHT_RED, e);
                return None;
            }
            Some(Ok(r)) => r,
        };
        if results.is_empty() {
            ui.label("The load case file has no case.");
            return None;
        }
        ui.horizontal(|ui| {
            let names: Vec<&str> = results.iter().map(|r| r.case.name.as_str()).collect();
            egui::ComboBox::from_label("load case")
                .selected_text(names[self.case.min(names.len() - 1)])
                .width(220.0)
                .show_ui(ui, |ui| {
                    for (k, n) in names.iter().enumerate() {
                        ui.selectable_value(&mut self.case, k, *n);
                    }
                });
            ui.label("structure");
            ui.add(egui::TextEdit::singleline(&mut self.filter).desired_width(100.0));
            ui.checkbox(&mut self.unfactored, "unfactored");
            if ui
                .button("Export CSV…")
                .on_hover_text("Every load case, every attachment")
                .clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("CSV", &["csv"])
                    .set_file_name("structure-loads.csv")
                    .save_file()
                && let Err(e) = std::fs::write(&path, ols_loads::to_csv(results))
            {
                log::warn!("{}: {e}", path.display());
            }
        });
        let r = &results[self.case.min(results.len() - 1)];
        let c = &r.case;
        ui.label(
            egui::RichText::new(format!(
                "{} — weather {}, wires {}, factors V {} T {} L {}, strength {}{}",
                if c.description.is_empty() {
                    &c.name
                } else {
                    &c.description
                },
                c.weather,
                c.condition.as_str(),
                c.vertical,
                c.transverse,
                c.longitudinal,
                c.strength,
                if c.tags.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", c.tags.join(", "))
                }
            ))
            .small(),
        );
        for f in &r.failed {
            ui.colored_label(egui::Color32::LIGHT_RED, format!("not solved: {f}"));
        }
        let filter = self.filter.trim().to_lowercase();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("structure-loads")
                    .striped(true)
                    .spacing([14.0, 3.0])
                    .show(ui, |ui| {
                        for h in [
                            "structure",
                            "set:phase",
                            "label",
                            "support",
                            "transverse kN",
                            "longitudinal kN",
                            "vertical kN",
                            "sections",
                        ] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for l in r
                            .loads
                            .iter()
                            .filter(|l| {
                                filter.is_empty() || l.structure.to_lowercase().contains(&filter)
                            })
                            .take(5000)
                        {
                            if ui
                                .link(&l.structure)
                                .on_hover_text("Go to this structure")
                                .clicked()
                            {
                                go = Some(l.structure.clone());
                            }
                            ui.label(format!("{}:{}", l.set, l.phase));
                            ui.label(&l.label);
                            ui.label(if l.support == 0 && l.label.is_empty() {
                                String::new()
                            } else {
                                format!("{}", l.support + 1)
                            });
                            let v = if self.unfactored { l.local } else { l.factored };
                            for x in v {
                                ui.label(
                                    egui::RichText::new(format!("{:.2}", x / 1000.0)).monospace(),
                                );
                            }
                            ui.label(l.sections.join(", "));
                            ui.end_row();
                        }
                    });
            });
        go
    }
}
