// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Design › Weight & quantities (joint details stage 4.6, 2026-11-19): a
//! tower's members, gusset plates, splice covers, bolts and welds counted
//! and weighed (`ohl_structure_link::structure_quantities`), its bill of
//! materials, and whether a run's self-weight is the counted mass (every
//! joint defined, the option on) or the members' steel × the dead-load
//! factor. Counted in the background whenever the file or the libraries
//! change while the panel is shown (a few tenths of a second on a real
//! tower).

use std::sync::mpsc;
use std::time::{Duration, Instant};

use eframe::egui;
use ohl_bom::structure::{PartKind, StructureBom, row_item};

use crate::app::{StructureApp, error_label};

/// What a count is made for: the file's revision and the libraries'
/// stamp.
type Key = (u64, u64);

/// A count's answer.
type Counted = Result<StructureBom, String>;

/// What the panel keeps.
#[derive(Default)]
pub struct QuantitiesTab {
    /// The last count: what it was made for, its answer, how long it took.
    pub counted: Option<(Key, Counted, Duration)>,
    /// A count going on: when it started, for what, its answer to come.
    running: Option<(Instant, Key, mpsc::Receiver<Counted>)>,
}

impl QuantitiesTab {
    /// Takes a finished count.
    fn poll(&mut self) {
        if let Some((started, key, rx)) = &self.running {
            match rx.try_recv() {
                Ok(r) => {
                    self.counted = Some((*key, r, started.elapsed()));
                    self.running = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.counted = Some((
                        *key,
                        Err("the count stopped unexpectedly".into()),
                        started.elapsed(),
                    ));
                    self.running = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }
}

/// The panel's sentence and its (?) (`ohl_ui::tool_window::intro`).
const INTRO: &str = "What the tower is built from, counted and weighed from its members and \
                     its joint details.";
const INTRO_MORE: &str = "Members: each its cross-section × its length (the model's \
     self-weight). At each joint, from its connections only: the bolts (their bolt library's \
     mass, else weighed from their sizes), the gusset plates and splice covers (outline × \
     thickness, holes not deducted), the welds between plates. A joint with a member end \
     without a connection, or one left for a hand detail, is not defined: its parts are not \
     counted. The heavy count takes every doubt on the heavy side (for a weight that loads \
     the tower), the light count on the light side (for a weight that holds it: uplift, \
     overturning). Where every joint is defined a run's self-weight is the counted mass \
     instead of the members' steel × the dead-load factor, unless the tick below is off.";
const COUNTED_TIP: &str = "On (the default): where every joint is defined, a run's self-weight \
     is the counted mass (the heavy count; the light count for the cases whose self-weight \
     factor is below 1) instead of the members' steel × the dead-load factor. Off: the factor \
     is kept anyway (counted_self_weight = false in the file). Effect: the dead load changes \
     by the counted mass over the members × the factor (shown below); no change in time.";

impl StructureApp {
    /// The Weight & quantities panel.
    pub fn quantities_ui(&mut self, ui: &mut egui::Ui) {
        self.quantities.poll();
        ohl_ui::tool_window::intro(ui, INTRO, INTRO_MORE, "weight--quantities", || {
            crate::help::open_section("user-guide/structure-app", "weight--quantities")
        });
        let Some(f) = &self.file else {
            return;
        };
        let t = f.structure().clone();
        if t.model.is_some() || !t.detail().is_design() {
            ui.label(
                "The weight and quantities need the tower's own members at level 3: every \
                 member with a cross-section and a supported joint (not a separate model file).",
            );
            return;
        }
        let key = (f.revision(), self.libs_stamp);
        // Counted again in the background when the file or the libraries
        // changed (one count at a time).
        let fresh = self.quantities.counted.as_ref().map(|c| c.0) == Some(key);
        if !fresh && self.quantities.running.is_none() {
            let text = f.text();
            let sections = self.loads.sections.clone();
            let bolts = self.bolt_library();
            let (tx, rx) = mpsc::channel();
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let r = ohl_structure_link::structure_quantities(&text, &sections, &bolts.0);
                let _ = tx.send(r);
                ctx.request_repaint();
            });
            self.quantities.running = Some((Instant::now(), key, rx));
        }
        // The option, and what a run takes.
        let mut on = t.counts_self_weight();
        let undefined = ohl_structure_link::undefined_joints(&t);
        ui.horizontal_wrapped(|ui| {
            if ui
                .checkbox(&mut on, "Self weight from the counted mass")
                .on_hover_text(COUNTED_TIP)
                .changed()
                && let Some(f) = self.file.as_mut()
            {
                let what = if on {
                    "Self weight from the counted mass"
                } else {
                    "Keep the dead-load factor"
                };
                f.edit(what, |doc, _| {
                    ohl_structure_edit::connection_edits::set_counted_self_weight(doc, on);
                });
            }
            // The count's thread asks for a frame when it is done.
            if self.quantities.running.is_some() {
                ui.label(egui::RichText::new("counting…").weak());
            } else if let Some((_, _, took)) = &self.quantities.counted {
                ui.label(
                    egui::RichText::new(format!("counted in {:.0} ms", took.as_secs_f64() * 1e3))
                        .weak(),
                );
            }
            let can = matches!(&self.quantities.counted, Some((k, Ok(_), _)) if *k == key);
            if ui
                .add_enabled(can, egui::Button::new("Bill of materials (xlsx)…"))
                .on_hover_text(
                    "The bill, every part, the joints not defined and the notes as a workbook \
                     (also in File › Export › CSV tables and in the structure report)",
                )
                .on_disabled_hover_text("Wait for the count")
                .clicked()
            {
                self.export_to(ohl_structure_export::Format::Quantities);
            }
        });
        let run_takes = if !on {
            "A run keeps the members' steel × the dead-load factor (the tick is off).".to_owned()
        } else if undefined.is_empty() {
            "Every joint is defined: a run takes the counted mass as the self-weight.".to_owned()
        } else {
            format!(
                "A run keeps the members' steel × the dead-load factor: {} joint(s) not \
                 defined (listed below).",
                undefined.len()
            )
        };
        ui.label(egui::RichText::new(run_takes).strong());
        let Some((counted_for, result, _)) = &self.quantities.counted else {
            return;
        };
        if *counted_for != key {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "The file changed since: counting again.",
            );
        }
        let b = match result {
            Ok(b) => b,
            Err(e) => {
                error_label(ui, format!("Not counted: {e}"));
                return;
            }
        };
        let factor = t
            .analysis
            .as_ref()
            .and_then(|a| a.get("dead_load_factor"))
            .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)));
        totals_ui(ui, b, factor);
        egui::ScrollArea::vertical()
            .id_salt("quantities-body")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::CollapsingHeader::new(format!("Bill of materials ({} lines)", b.rows.len()))
                    .id_salt("quantities-bill")
                    .default_open(true)
                    .show(ui, |ui| bill_ui(ui, b));
                if !b.undefined.is_empty() {
                    egui::CollapsingHeader::new(format!(
                        "Joints not defined ({}), their parts not counted",
                        b.undefined.len()
                    ))
                    .id_salt("quantities-undefined")
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("quantities-undefined-list")
                            .max_height(160.0)
                            .show(ui, |ui| {
                                for (j, why) in &b.undefined {
                                    ui.label(format!("{j}: {why}"));
                                }
                            });
                    });
                }
                egui::CollapsingHeader::new(format!("Notes ({})", b.notes.len()))
                    .id_salt("quantities-notes")
                    .default_open(true)
                    .show(ui, |ui| {
                        for n in &b.notes {
                            if n.contains("NOT ON THE SAFE SIDE") {
                                error_label(ui, n.as_str());
                            } else {
                                ui.label(n.as_str());
                            }
                        }
                    });
            });
    }
}

/// The totals per kind, the whole and the dead-load factor's equivalent.
fn totals_ui(ui: &mut egui::Ui, b: &StructureBom, factor: Option<f64>) {
    egui::Grid::new("quantities-totals")
        .striped(true)
        .num_columns(4)
        .show(ui, |ui| {
            ui.strong("Parts");
            ui.strong("count");
            ui.strong("mass [kg]");
            ui.strong("light count [kg]");
            ui.end_row();
            for t in &b.totals {
                ui.label(t.kind.plural());
                ui.label(t.count.to_string());
                ui.label(format!("{:.1}", t.mass));
                ui.label(format!("{:.1}", t.light));
                ui.end_row();
            }
            ui.strong("total");
            ui.label("");
            ui.strong(format!("{:.1}", b.mass));
            ui.strong(format!("{:.1}", b.light));
            ui.end_row();
        });
    if let Some(k) = b.factor() {
        let f = factor.unwrap_or(1.0);
        ui.label(format!(
            "The counted mass is {k:.3} × the members' steel; the members × the dead-load factor \
             {f} give {:.1} kg ({:+.1} %).",
            b.members_mass * f,
            (b.mass / (b.members_mass * f) - 1.0) * 100.0
        ));
    }
}

/// The bill: one row per kind and item.
fn bill_ui(ui: &mut egui::Ui, b: &StructureBom) {
    egui::Grid::new("quantities-bill-grid")
        .striped(true)
        .num_columns(5)
        .show(ui, |ui| {
            for h in ["kind", "item", "count", "quantity", "mass [kg]"] {
                ui.strong(h);
            }
            ui.end_row();
            for r in &b.rows {
                ui.label(r.kind.as_str());
                ui.label(row_item(r));
                ui.label(r.count.to_string());
                ui.label(match r.kind {
                    PartKind::Bolt => format!("{:.0} pcs", r.quantity),
                    PartKind::Plate | PartKind::Cover => format!("{:.4} m²", r.quantity),
                    PartKind::Member | PartKind::Weld => format!("{:.3} m", r.quantity),
                });
                ui.label(format!("{:.1}", r.mass));
                ui.end_row();
            }
        });
}
