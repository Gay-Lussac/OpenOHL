// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Checks › Bill of materials: what the line is built from (structures by
//! type, insulator assemblies, wire lengths, hardware) priced with a cost
//! file (`.olscost`), computed in the background from the model as it is;
//! the model is not changed. CSV export.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use ols_bom::{CostData, CostEstimate};

use crate::model::Model;

/// The Bill of materials window.
#[derive(Default)]
pub struct BomWindow {
    /// Shown.
    pub open: bool,
    /// The cost file used (none: quantities only).
    pub cost_path: Option<PathBuf>,
    /// The last answer (or why there is none).
    pub result: Option<Result<CostEstimate, String>>,
    running: Option<Receiver<Result<CostEstimate, String>>>,
    /// What the last export did.
    status: String,
}

/// The first cost file next to the model's file.
fn cost_file_near(file: &Path) -> Option<PathBuf> {
    let dir = file.parent()?;
    ols_core::files::files_with_extension(dir, ols_bom::cost::COST_FILE_EXTENSION)
        .into_iter()
        .next()
}

impl BomWindow {
    /// Opens the window and computes (a cost file next to the model is
    /// used until another is chosen).
    pub fn open(&mut self, model: &Model, ctx: &egui::Context) {
        self.open = true;
        if self.cost_path.is_none() {
            self.cost_path = cost_file_near(&model.sources.file);
        }
        self.start(model, ctx);
    }

    /// Whether a computation runs.
    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    /// Computes the bill of the model as it is now, in the background.
    pub fn start(&mut self, model: &Model, ctx: &egui::Context) {
        let line = model.line.clone();
        let library = model.library.clone();
        let path = self.cost_path.clone();
        let (tx, rx) = channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let r = path.as_deref().map(CostData::load).transpose().map(|cost| {
                let hardware = cost
                    .as_ref()
                    .map(|c| c.hardware.clone())
                    .unwrap_or_default();
                let bom = ols_bom::bill_of_materials(&line, &library, &hardware);
                CostEstimate::new(bom, cost.as_ref())
            });
            let _ = tx.send(r);
            c.request_repaint();
        });
        self.running = Some(rx);
    }

    fn poll(&mut self) {
        let Some(rx) = &self.running else {
            return;
        };
        let r = match rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("the computation stopped".into())
            }
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
        egui::Window::new("Bill of materials")
            .open(&mut open)
            .default_width(760.0)
            .default_height(520.0)
            .show(ctx, |ui| self.ui(ui, model));
        self.open = open;
    }

    fn ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        ui.horizontal(|ui| {
            ui.label("Cost file:");
            let name =
                self.cost_path
                    .as_ref()
                    .map_or("— none: quantities only —".into(), |p| {
                        p.file_name()
                            .map_or(String::new(), |n| n.to_string_lossy().into_owned())
                    });
            ui.strong(name);
            if ui
                .button("Choose…")
                .on_hover_text("Unit costs (.olscost)")
                .clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter("Unit costs", &[ols_bom::cost::COST_FILE_EXTENSION])
                    .pick_file()
            {
                self.cost_path = Some(p);
                self.start(model, ui.ctx());
            }
            if self.cost_path.is_some() && ui.button("No prices").clicked() {
                self.cost_path = None;
                self.start(model, ui.ctx());
            }
            ui.separator();
            if ui
                .add_enabled(!self.running(), egui::Button::new("Compute again"))
                .on_hover_text("From the line as it is now")
                .clicked()
            {
                self.start(model, ui.ctx());
            }
            if self.running() {
                ui.spinner();
            }
            let estimate = self.result.as_ref().and_then(|r| r.as_ref().ok());
            if ui
                .add_enabled(estimate.is_some(), egui::Button::new("Export CSV…"))
                .clicked()
                && let Some(e) = estimate
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("CSV", &["csv"])
                    .set_file_name("bill-of-materials.csv")
                    .save_file()
            {
                self.status = match std::fs::write(&path, e.csv()) {
                    Ok(()) => format!("wrote {}", path.display()),
                    Err(err) => format!("{}: {err}", path.display()),
                };
            }
        });
        if !self.status.is_empty() {
            ui.label(egui::RichText::new(&self.status).small().weak());
        }
        ui.separator();
        match &self.result {
            None => {
                ui.label("Computing…");
            }
            Some(Err(e)) => {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            Some(Ok(e)) => table(ui, e),
        }
    }
}

fn table(ui: &mut egui::Ui, e: &CostEstimate) {
    let currency = if e.currency.is_empty() {
        String::new()
    } else {
        format!(" {}", e.currency)
    };
    egui::ScrollArea::vertical()
        .max_height(ui.available_height() - 80.0)
        .show(ui, |ui| {
            egui::Grid::new("bom-table")
                .striped(true)
                .num_columns(6)
                .show(ui, |ui| {
                    for h in ["category", "item", "quantity", "unit"] {
                        ui.strong(h);
                    }
                    if e.priced {
                        ui.strong(format!("unit price{currency}"));
                        ui.strong(format!("amount{currency}"));
                    }
                    ui.end_row();
                    for l in &e.lines {
                        ui.label(&l.category);
                        ui.label(&l.item);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(if l.unit == "m" {
                                format!("{:.1}", l.quantity)
                            } else {
                                format!("{}", l.quantity)
                            })
                        });
                        ui.label(&l.unit);
                        if e.priced {
                            match l.unit_price {
                                Some(p) => ui.label(format!("{p:.2}")),
                                None => ui.colored_label(ui.visuals().warn_fg_color, "no price"),
                            };
                            ui.label(l.amount.map_or(String::new(), |a| format!("{a:.2}")));
                        }
                        ui.end_row();
                    }
                });
        });
    ui.separator();
    ui.label(format!("Spans between structures: {}", e.bom.spans));
    if e.priced {
        ui.strong(format!("Total: {:.2}{currency}", e.total));
        if !e.missing.is_empty() {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!(
                    "{} item(s) without a price in the cost file",
                    e.missing.len()
                ),
            )
            .on_hover_text(e.missing.join("\n"));
        }
    }
    for n in &e.bom.notes {
        ui.label(egui::RichText::new(n).small().weak());
    }
}
