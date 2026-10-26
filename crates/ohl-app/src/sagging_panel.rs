// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The section window's **Sagging** panel: what the stringing crew
//! measures per span ([`ohl_line::sagging::sagging_table`]) — sag, the
//! inclined-span correction, the sag per kelvin, the dynamometer at both
//! clamps, the stopwatch, a transit at the back support and the clipping
//! offsets. Post-processing of the section as stored (the ruling-span
//! model, initial state, bare, still air).

use eframe::egui;
use ohl_line::sagging::{SaggingOptions, SaggingTable, sagging_table};

use crate::model::Model;

/// The panel's inputs and its last result, kept per section in egui's
/// memory (the section form is copied every frame).
#[derive(Debug, Clone, Default)]
struct Panel {
    /// Conductor temperature at sagging, °C, when not the stringing one.
    temperature: Option<f64>,
    /// Temperature at clipping in, °C, when not the sagging one.
    clip_temperature: Option<f64>,
    /// Waves timed by the stopwatch (0: the default, 3).
    waves: u32,
    /// Transit this far below each span's back support, m (none: off).
    transit: Option<f64>,
    /// The last result, or why it failed.
    result: Option<Result<SaggingTable, String>>,
}

/// The panel for the stored section `id` (nothing for a new section: it
/// is not in the line yet). `stringing_temperature` is the form's.
pub fn ui(ui: &mut egui::Ui, model: &Model, id: &str, stringing_temperature: f64) {
    let Some(section) = model.line.sections.iter().find(|s| s.id == id) else {
        return;
    };
    let key = egui::Id::new(("sagging-panel", id));
    let mut panel: Panel = ui.ctx().data(|d| d.get_temp(key)).unwrap_or_default();
    if panel.waves == 0 {
        panel.waves = 3;
    }
    egui::CollapsingHeader::new("Sagging")
        .id_salt(("sagging", id))
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(
                    "In the blocks, initial state, bare, still air (the section as stored, ruling \
                     span). For the crew: sags, the inclined-span correction, the sag per kelvin, \
                     the dynamometer at both clamps, the stopwatch, a transit at each back \
                     support, the clipping offsets.",
                )
                .small(),
            );
            optional(
                ui,
                "conductor temperature",
                &mut panel.temperature,
                stringing_temperature,
                "the stringing temperature",
            );
            let sag_t = panel.temperature.unwrap_or(stringing_temperature);
            optional(
                ui,
                "clipped in at",
                &mut panel.clip_temperature,
                sag_t,
                "the sagging temperature: no offsets",
            );
            ui.horizontal(|ui| {
                ui.label("stopwatch");
                crate::style::field(
                    ui,
                    egui::DragValue::new(&mut panel.waves)
                        .range(1..=10)
                        .suffix(" waves"),
                );
            });
            ui.horizontal(|ui| {
                let mut on = panel.transit.is_some();
                ui.checkbox(&mut on, "transit at the back support,");
                if on != panel.transit.is_some() {
                    panel.transit = on.then_some(3.0);
                }
                if let Some(below) = &mut panel.transit {
                    crate::style::field(
                        ui,
                        egui::DragValue::new(below)
                            .speed(0.05)
                            .range(0.0..=100.0)
                            .suffix(" m below it"),
                    );
                }
            });
            if ui
                .button("Calculate")
                .on_hover_text("The sagging table of the section as stored")
                .clicked()
            {
                let options = SaggingOptions {
                    temperature: panel.temperature,
                    clip_temperature: panel.clip_temperature,
                    waves: panel.waves,
                    transit_below_back: panel.transit,
                };
                panel.result = Some(match model.library.get(&section.cable) {
                    None => Err(format!(
                        "wire type {} not in the wire library",
                        section.cable
                    )),
                    Some(cable) => sagging_table(
                        &model.line,
                        section,
                        cable,
                        &options,
                        &ohl_core::budget::Budget::new()
                            .max_iterations(200)
                            .time_limit(std::time::Duration::from_secs(60)),
                    )
                    .map_err(|e| e.to_string()),
                });
            }
            match &panel.result {
                None => {}
                Some(Err(e)) => {
                    egui::ScrollArea::vertical()
                        .id_salt(("sagging-error", id))
                        .max_height(80.0)
                        .show(ui, |ui| {
                            ui.colored_label(ui.visuals().error_fg_color, e);
                        });
                }
                Some(Ok(table)) => table_ui(ui, table, id),
            }
        });
    ui.ctx().data_mut(|d| d.insert_temp(key, panel));
}

/// A temperature that follows `default` unless given.
fn optional(ui: &mut egui::Ui, label: &str, value: &mut Option<f64>, default: f64, what: &str) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut own = value.is_some();
        if ui
            .checkbox(&mut own, "")
            .on_hover_text(format!("Off: {what}"))
            .changed()
        {
            *value = own.then_some(default);
        }
        match value {
            Some(t) => {
                crate::style::field(ui, egui::DragValue::new(t).speed(0.5).suffix(" °C"));
            }
            None => {
                ui.label(egui::RichText::new(format!("{default:.1} °C ({what})")).weak());
            }
        }
    });
}

fn table_ui(ui: &mut egui::Ui, t: &SaggingTable, id: &str) {
    ui.label(match t.blocks {
        ohl_line::InTheBlocks::EqualH => format!(
            "At {:.1} °C: H = {:.2} kN in every span (simplified: equal H); clipped in at {:.1} °C",
            t.temperature,
            t.horizontal_tension / 1000.0,
            t.clip_temperature
        ),
        ohl_line::InTheBlocks::Sheaves => format!(
            "At {:.1} °C over sheaves: each span its own H; clipped in at {:.1} °C",
            t.temperature, t.clip_temperature
        ),
    });
    egui::ScrollArea::both()
        .id_salt(("sagging-table", id))
        .max_height(240.0)
        .show(ui, |ui| {
            egui::Grid::new(("sagging-grid", id))
                .striped(true)
                .show(ui, |ui| {
                    for h in [
                        "span",
                        "length m",
                        "rise m",
                        "H kN",
                        "sag m",
                        "level sag m",
                        "incl. corr. m",
                        "sag per K m",
                        "dyno back kN",
                        "dyno ahead kN",
                        "stopwatch s",
                        "transit °",
                        "target below m",
                        "clip offset m",
                    ] {
                        ui.label(egui::RichText::new(h).small().strong());
                    }
                    ui.end_row();
                    for (k, s) in t.spans.iter().enumerate() {
                        ui.label(format!("{}", k + 1));
                        ui.label(format!("{:.2}", s.span));
                        ui.label(format!("{:.2}", s.rise));
                        ui.label(format!("{:.3}", s.horizontal_tension / 1000.0));
                        ui.label(format!("{:.3}", s.sag));
                        ui.label(format!("{:.3}", s.level_sag));
                        ui.label(format!("{:.3}", s.inclined_correction));
                        ui.label(format!("{:.4}", s.sag_per_kelvin));
                        ui.label(format!("{:.2}", s.dyno_back / 1000.0));
                        ui.label(format!("{:.2}", s.dyno_ahead / 1000.0));
                        ui.label(format!("{:.2}", s.stopwatch));
                        match &s.transit {
                            None => {
                                ui.label("—");
                                ui.label("—");
                            }
                            Some(Ok((angle, below))) => {
                                ui.label(format!("{angle:+.3}"));
                                ui.label(format!("{below:.3}"));
                            }
                            Some(Err(e)) => {
                                ui.label("—").on_hover_text(e);
                                ui.label("—").on_hover_text(e);
                            }
                        }
                        ui.label(format!("{:+.3}", s.clip_offset));
                        ui.end_row();
                    }
                });
        });
    ui.label(
        egui::RichText::new(
            "Sag: vertical, where the wire is parallel to the chord. Level sag: a level span's \
             of the same length, tension and load (a level-span chart). Clip offset: at the \
             structure ahead, along the unstressed wire, positive into the next span (zero \
             when clipped at the sagging temperature: the wire is clipped in as it lies in the \
             blocks). In the blocks as the calculation options say: over frictionless sheaves \
             (the wire's tension the same on both sides of each) or simplified (equal H).",
        )
        .small()
        .weak(),
    );
    for w in &t.warnings {
        ui.colored_label(ui.visuals().warn_fg_color, format!("Warning: {w}"));
    }
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    /// The panel opens folded; Calculate gives the table of the section
    /// (one row per span, the clipping offsets zero at the sagging
    /// temperature).
    #[test]
    fn the_sagging_panel_calculates_the_table() {
        let model = crate::tests::synthetic();
        let section = model.line.sections[0].clone();
        let t = section.stringing.temperature();
        let mut h = Harness::new_ui(move |ui| {
            super::ui(ui, &model, &section.id, t);
        });
        h.run();
        assert!(h.query_by_label("Calculate").is_none(), "folded");
        h.get_by_label("Sagging").click();
        h.run();
        h.get_by_label("Calculate").click();
        h.run();
        assert!(
            h.query_all_by_label_contains("each span its own H")
                .next()
                .is_some(),
            "the table's heading"
        );
        assert!(h.query_by_label("stopwatch s").is_some());
        assert!(
            h.query_all_by_label("+0.000").next().is_some(),
            "no offsets"
        );
    }
}
