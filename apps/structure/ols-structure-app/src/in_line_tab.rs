// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The Line values panel (In the line menu): the status of the line's
//! values, the values the line gives at the placed structure (from its
//! line-values file, `--line-values`), per point and case — a case whose
//! values lack a wire not solved marked "incomplete: S12 not solved" —
//! and the other places of the same structure file with their governing
//! values.

use eframe::egui;
use ols_structure_loads::{LineQuantity, LineValues};

use crate::app::{StructureApp, error_label};

/// The six values per point and case, in the table's order.
const QUANTITIES: [(LineQuantity, &str, &str); 6] = [
    (
        LineQuantity::Transverse,
        "transverse",
        "kN, to the right of the line, unfactored",
    ),
    (LineQuantity::Longitudinal, "longitudinal", "kN, ahead"),
    (LineQuantity::Vertical, "vertical", "kN, down"),
    (
        LineQuantity::TransverseFactored,
        "transverse ×",
        "kN, factored by the line's load case",
    ),
    (LineQuantity::LongitudinalFactored, "longitudinal ×", "kN"),
    (LineQuantity::VerticalFactored, "vertical ×", "kN"),
];

/// The line's cases in `values`, in the order of `cases` (then any other).
fn cases_of(values: &LineValues, cases: &[String]) -> Vec<String> {
    let mut out: Vec<String> = cases
        .iter()
        .filter(|c| values.values.iter().any(|v| &v.case == *c))
        .cloned()
        .collect();
    for v in &values.values {
        if !out.contains(&v.case) {
            out.push(v.case.clone());
        }
    }
    out
}

fn kn(v: Option<f64>) -> String {
    v.map_or("—".into(), |v| format!("{:.3}", v / 1000.0))
}

impl StructureApp {
    /// The Line values panel (In the line menu).
    pub fn in_the_line_ui(&mut self, ui: &mut egui::Ui) {
        if let Some(f) = &self.file {
            self.loads.refresh(f);
        }
        self.line_status_ui(ui);
        let Some(title) = self.loads.placed_title() else {
            ui.label(
                "Opened on its own: no line. From OHL-Design, Open in OHL-Structure… on a placed \
                 structure (its Properties) opens the structure with that place's loads: the \
                 line's values per point and load case are listed here, the recipes evaluated \
                 with them, and values typed over the table are kept with that placed \
                 structure.",
            );
            return;
        };
        ui.horizontal_wrapped(|ui| {
            ui.strong(title);
            if let Some(i) = &self.loads.info
                && !i.structure_type.is_empty()
            {
                ui.label(format!("(structure type {})", i.structure_type));
            }
            if let Some(p) = &self.line_values {
                ui.label(egui::RichText::new(p.display().to_string()).weak().small())
                    .on_hover_text(
                        "Written by OHL-Design; read again when the line is solved \
                         again",
                    );
            }
        });
        if let Some(e) = &self.loads.values_error {
            error_label(ui, e.as_str());
        }
        let Some(info) = &self.loads.info else {
            ui.label("Waiting for the line's values.");
            return;
        };
        let values = &info.values;
        let cases = cases_of(values, &info.cases);
        if values.points.is_empty() {
            ui.label("The line gives no values at this structure (no wire hangs on it).");
        }
        ui.label(format!(
            "{} point(s), {} load case(s) — forces in kN, transverse to the right of the line, \
             longitudinal ahead, vertical down; × factored by the line's load case",
            values.points.len(),
            cases.len()
        ));
        egui::ScrollArea::both()
            .id_salt("in-the-line")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new("line-values").striped(true).show(ui, |ui| {
                    for (h, tip) in [
                        ("point", "A point or wire point of the structure"),
                        ("sets", "The sets hanging there"),
                        ("phases", "The phases hanging there (the line's labels)"),
                        (
                            "case",
                            "The line's case: a load case, or a weather case sent on its own \
                             (all factors 1)",
                        ),
                        ("source", "Load case or weather case"),
                        (
                            "complete",
                            "incomplete: a section or jumper at this structure did not solve in \
                             that case of the line — its forces are missing, the values too low",
                        ),
                    ] {
                        ui.strong(h).on_hover_text(tip);
                    }
                    for (_, h, tip) in QUANTITIES {
                        ui.strong(h).on_hover_text(tip);
                    }
                    ui.end_row();
                    for p in &values.points {
                        let sets = p
                            .sets
                            .iter()
                            .map(u32::to_string)
                            .collect::<Vec<_>>()
                            .join(", ");
                        for c in &cases {
                            ui.label(&p.label);
                            ui.label(&sets);
                            ui.label(p.phases.join(", "));
                            let incomplete = info.incomplete(c);
                            if incomplete.is_some() {
                                ui.colored_label(crate::load_cases_tab::INCOMPLETE, c);
                            } else {
                                ui.label(c);
                            }
                            ui.label(info.source(c).label());
                            match &incomplete {
                                Some(text) => {
                                    ui.colored_label(crate::load_cases_tab::INCOMPLETE, text)
                                }
                                None => ui.label("yes"),
                            };
                            for (q, _, _) in QUANTITIES {
                                ui.label(kn(values.get(c, &p.label, &p.sets, q)));
                            }
                            ui.end_row();
                        }
                    }
                });
                if !info.others.is_empty() {
                    ui.add_space(8.0);
                    egui::CollapsingHeader::new(format!(
                        "Other places of this structure ({})",
                        info.others.len()
                    ))
                    .id_salt("other-places")
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(
                                "The governing value per point and direction over the line's \
                                 cases (factored, largest in size), kN — each place keeps its \
                                 own loads; this is for comparison only.",
                            )
                            .weak(),
                        );
                        egui::Grid::new("other-places-grid")
                            .striped(true)
                            .show(ui, |ui| {
                                for h in [
                                    "structure",
                                    "point",
                                    "transverse ×",
                                    "longitudinal ×",
                                    "vertical ×",
                                ] {
                                    ui.strong(h);
                                }
                                ui.end_row();
                                for o in &info.others {
                                    for g in o.values.governing() {
                                        ui.label(&o.structure);
                                        ui.label(&g.point);
                                        for l in &g.largest {
                                            ui.label(l.as_ref().map_or("—".into(), |(v, c)| {
                                                format!("{:.3} ({c})", v / 1000.0)
                                            }));
                                        }
                                        ui.end_row();
                                    }
                                }
                            });
                    });
                }
            });
    }
}
