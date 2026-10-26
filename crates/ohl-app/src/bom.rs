// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Plans & sheets › Bill of materials › Cables: the wire lengths of each cable and
//! their stringing, priced with a cost file (`.ohlcost`), computed in the
//! background from the model as it is; the model is not changed. CSV
//! export. (The bill also counts structures, insulators and hardware;
//! those parts are not offered yet.)

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use ohl_bom::{CostData, CostEstimate};

use crate::check_help as help;
use crate::model::Model;

/// What its results are computed from ([`crate::revision`]).
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::LINE;

/// The categories of the bill shown: the cables'.
pub const CABLES: [&str; 2] = ["wire", "stringing"];

/// `e` with only the lines of `categories` (its total and the items
/// without a price of those only).
fn only(mut e: CostEstimate, categories: &[&str]) -> CostEstimate {
    e.lines
        .retain(|l| categories.contains(&l.category.as_str()));
    e.total = e.lines.iter().filter_map(|l| l.amount).sum();
    e.missing
        .retain(|m| categories.iter().any(|c| m.starts_with(&format!("{c}:"))));
    e
}

/// The Bill of materials window (the cables).
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
    /// What the last answer was computed from ([`crate::revision`]).
    stamp: crate::revision::Stamp,
}

impl BomWindow {
    /// Opens the window and computes (the project's cost file, next to a
    /// line file, is used until another is chosen).
    pub fn open(&mut self, model: &Model, ctx: &egui::Context) {
        self.open = true;
        if self.cost_path.is_none() {
            self.cost_path = model.project_files("costs").into_iter().next();
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
        self.stamp.started(model.revision());
        std::thread::spawn(move || {
            let r = path.as_deref().map(CostData::load).transpose().map(|cost| {
                let hardware = cost
                    .as_ref()
                    .map(|c| c.hardware.clone())
                    .unwrap_or_default();
                let bom = ohl_bom::bill_of_materials(&line, &library, &hardware);
                only(CostEstimate::new(bom, cost.as_ref()), &CABLES)
            });
            let _ = tx.send(r);
            c.request_repaint();
        });
        self.running = Some(rx);
    }

    /// Takes the answer, when it came.
    pub(crate) fn poll(&mut self) {
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
        self.stamp.arrived();
        self.running = None;
    }

    /// What the last answer was computed from that changed since, in
    /// words (`None`: nothing, or no answer): the line and its files
    /// ([`crate::revision`]).
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        matches!(self.result, Some(Ok(_)))
            .then(|| self.stamp.out_of_date(model, USES))
            .flatten()
    }

    /// The window, when open.
    pub fn show(&mut self, ctx: &egui::Context, model: &Model) {
        if !self.open {
            return;
        }
        self.poll();
        let mut open = true;
        egui::Window::new("Bill of materials: wires")
            .open(&mut open)
            .default_width(760.0)
            .default_height(520.0)
            .show(ctx, |ui| {
                // Run in the footer at the bottom left, always visible
                // (owner, 2026-10-21); the rest scrolls above it.
                help::body(ui, "bom-window", |ui| self.ui(ui, model));
                help::footer(ui, "bom-window", |ui| {
                    if help::run_button(ui, !self.running(), "From the line as it is now", None) {
                        self.start(model, ui.ctx());
                    }
                    if self.running() {
                        ui.spinner();
                    }
                });
            });
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
                .on_hover_text("Unit costs (.ohlcost)")
                .clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter("Unit costs", &[ohl_bom::cost::COST_FILE_EXTENSION])
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
            let estimate = self.result.as_ref().and_then(|r| r.as_ref().ok());
            if ui
                .add_enabled(estimate.is_some(), egui::Button::new("Export CSV…"))
                .clicked()
                && let Some(e) = estimate
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("CSV", &["csv"])
                    .set_file_name("bill-of-materials-wires.csv")
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
        if let Some(what) = self.out_of_date(model)
            && crate::revision::banner(ui, &what, !self.running())
        {
            self.start(model, ui.ctx());
        }
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
            // Every line, only those in view drawn (2026-10-26).
            let unit_price = format!("unit price{currency}");
            let amount = format!("amount{currency}");
            let mut columns = vec![
                ("category", ""),
                ("item", ""),
                ("quantity", ""),
                ("unit", ""),
            ];
            if e.priced {
                columns.push((unit_price.as_str(), ""));
                columns.push((amount.as_str(), ""));
            }
            let mut table = crate::rows_in_view::Table::new(ui, "bom-table", columns.len());
            table.header(ui, &columns);
            table.rows(ui, e.lines.len(), None, |c, i| {
                let l = &e.lines[i];
                c.cell(|ui| ui.label(&l.category));
                c.cell(|ui| ui.label(&l.item));
                c.cell(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(if l.unit == "m" {
                            format!("{:.1}", l.quantity)
                        } else {
                            format!("{}", l.quantity)
                        })
                    })
                });
                c.cell(|ui| ui.label(&l.unit));
                if e.priced {
                    c.cell(|ui| match l.unit_price {
                        Some(p) => ui.label(format!("{p:.2}")),
                        None => ui.colored_label(ui.visuals().warn_fg_color, "no price"),
                    });
                    c.cell(|ui| ui.label(l.amount.map_or(String::new(), |a| format!("{a:.2}"))));
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
