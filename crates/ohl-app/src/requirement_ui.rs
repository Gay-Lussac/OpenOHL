// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A distance rule, as the user sets it wherever one is edited (the
//! Clearances tab's rule editor, the Vertical / Radial clearances windows'
//! rule tables, the vegetation panel's clearance
//! and allowance, the safety-volume distance): one shared component.
//!
//! - A **form selector** "One distance | Table by voltage | Base + per
//!   kV" ([`Requirement`]'s three forms). Switching keeps the values
//!   sensibly: a form left and come back to unchanged is restored exactly;
//!   otherwise the largest distance at the line's voltages is carried over
//!   (a table from a formula gets a row per voltage of the line, so its
//!   distances there stay the same).
//! - Every editable value is a **framed field with its unit** (m, kV,
//!   m/kV), shown in place — no value hides in a menu.
//! - A table has a **"voltage up to" / "distance"** header, a **Remove**
//!   button per row, **+ Add row** and **Sort by voltage**, and a hint on
//!   the order.
//! - A **live line** under it: the distance at each of the line's own
//!   system voltages ("At 220 kV: 7.00 m; at 380 kV: 9.00 m"), a formula
//!   written out, and what is missing.
//! - A rule's **voltage filter** ([`voltage_filter_ui`]), "only for
//!   these voltages" (none ticked: every voltage): optional with one
//!   distance; with a table or base + per kV, which already depend on the
//!   voltage, hidden behind **Only for some voltages…** (off unless a
//!   filter is set, as in an older file) — no voltage is ever needed
//!   (owner, 2026-10-21).
//! - A **read-only** form ([`requirement_view`]) for places that only show
//!   a rule (a preset before it is applied): greyed, on a faint
//!   background, marked [`PREVIEW_NOTE`].

use eframe::egui;
use ohl_clearance::rules::Requirement;

/// The three forms, as the selector names them.
pub const FORMS: [&str; 3] = ["One distance", "Table by voltage", "Base + per kV"];

/// What each form means (hover).
const FORM_TIPS: [&str; 3] = [
    "The same distance for every voltage",
    "Rows \"voltage up to: distance\": a wire takes the first row its system's nominal voltage \
     does not exceed (earthed wires: 0 kV)",
    "A base distance plus an amount per kV above a voltage: base + per kV × (U − above)",
];

/// A table row's remove button (narrow: the Clearances tab).
pub const REMOVE: &str = "×";

/// The toggle that shows a voltage filter beside a distance that
/// depends on the voltage.
pub const ONLY_SOME_VOLTAGES: &str = "Only for some voltages…";

/// A voltage in a filter: "earthed (0 kV)", "220 kV".
pub fn voltage_name(kv: f64) -> String {
    if kv == 0.0 {
        "earthed (0 kV)".into()
    } else {
        format!("{kv} kV")
    }
}

/// A rule's voltage filter (`filter`: the nominal voltages of the wires
/// checked, kV, 0 for earthed wires; empty: every wire), ticked among the
/// line's `voltages`. `depends`: the rule's distance is a table by voltage
/// or base + per kV — then no voltage is needed and the ticks hide behind
/// [`ONLY_SOME_VOLTAGES`], off by default and shown on while a filter is
/// set (an older file's: the user may clear it). Turning it off clears the
/// filter. True when the filter changed.
pub fn voltage_filter_ui(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    filter: &mut Vec<f64>,
    depends: bool,
    voltages: &[f64],
) -> bool {
    let key = ui.make_persistent_id(("voltage-filter", id));
    let mut changed = false;
    ui.vertical(|ui| {
        // One line each, also in a narrow column.
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        if depends {
            let asked = ui.data(|d| d.get_temp::<bool>(key)).unwrap_or(false);
            let mut on = asked || !filter.is_empty();
            if ui
                .checkbox(&mut on, ONLY_SOME_VOLTAGES)
                .on_hover_text(
                    "The distance already depends on each wire's voltage: no voltage is needed. \
                     Tick to check only the wires of some voltages (off: every wire).",
                )
                .changed()
            {
                ui.data_mut(|d| d.insert_temp(key, on));
                if !on && !filter.is_empty() {
                    filter.clear();
                    changed = true;
                }
            }
            if !on {
                return;
            }
        }
        let title = if filter.is_empty() {
            "every voltage".to_owned()
        } else {
            ohl_clearance::zone::voltages_text(filter)
        };
        let mut all: Vec<f64> = voltages.to_vec();
        for v in filter.iter() {
            if !all.iter().any(|x| (x - v).abs() < 1e-6) {
                all.push(*v);
            }
        }
        all.sort_by(f64::total_cmp);
        ui.push_id(key, |ui| {
            crate::menu::options_menu(ui, &title, |ui| {
                ui.label(
                    egui::RichText::new("Only for these voltages; none ticked: every voltage")
                        .small()
                        .weak(),
                );
                for v in all {
                    let mut on = filter.iter().any(|x| (x - v).abs() < 1e-6);
                    if ui.checkbox(&mut on, voltage_name(v)).changed() {
                        if on {
                            filter.push(v);
                            filter.sort_by(f64::total_cmp);
                        } else {
                            filter.retain(|x| (x - v).abs() >= 1e-6);
                        }
                        changed = true;
                    }
                }
            });
        });
    });
    changed
}

/// The mark of a rule shown read-only.
pub const PREVIEW_NOTE: &str = "(preview — Apply to edit)";

fn form(r: &Requirement) -> usize {
    match r {
        Requirement::Fixed(_) => 0,
        Requirement::ByVoltage(_) => 1,
        Requirement::Formula { .. } => 2,
    }
}

/// The largest distance `r` gives at the line's voltages (and, for a
/// table, in any of its rows): what another form starts with, on the safe
/// side.
fn largest(r: &Requirement, voltages: &[f64]) -> f64 {
    let mut d = preview_voltages(voltages)
        .into_iter()
        .filter_map(|kv| r.at(kv).ok())
        .fold(f64::NEG_INFINITY, f64::max);
    match r {
        Requirement::ByVoltage(rows) => {
            for row in rows {
                d = d.max(row.1);
            }
        }
        Requirement::Fixed(x) => d = d.max(*x),
        Requirement::Formula { base, .. } => d = d.max(*base),
    }
    if d.is_finite() { d } else { 0.0 }
}

/// `r` turned into form `k` (`0` one distance, `1` table, `2` base + per
/// kV): one distance and a formula's base take the largest distance at
/// the line's voltages (conservative: no wire of the line gets less); a
/// table from one distance has one row up to the line's highest voltage,
/// from a formula one row per voltage of the line with the formula's
/// distance there.
pub fn convert(r: &Requirement, k: usize, voltages: &[f64]) -> Requirement {
    if k == form(r) {
        return r.clone();
    }
    let d = largest(r, voltages);
    match k {
        0 => Requirement::Fixed(d),
        1 => match r {
            Requirement::Formula { .. } => {
                let mut kvs: Vec<f64> = voltages.iter().copied().filter(|v| *v > 0.0).collect();
                kvs.sort_by(f64::total_cmp);
                kvs.dedup();
                if kvs.is_empty() {
                    kvs.push(0.0);
                }
                Requirement::ByVoltage(
                    kvs.into_iter()
                        .map(|kv| (kv, r.at(kv).unwrap_or(d)))
                        .collect(),
                )
            }
            _ => Requirement::ByVoltage(vec![(voltages.iter().copied().fold(0.0, f64::max), d)]),
        },
        _ => Requirement::Formula {
            base: d,
            per_kv: 0.0,
            above_kv: 0.0,
        },
    }
}

/// What a selector remembers between frames: the value it switched to,
/// and the value each form had when it was left.
#[derive(Clone)]
struct Memory {
    shown: Requirement,
    kept: [Option<Requirement>; 3],
}

/// Switches `r` to form `k`: the value that form had when it was left,
/// when `r` is still what the switch gave (nothing typed since); else
/// [`convert`].
pub fn switch_form(
    ctx: &egui::Context,
    id: egui::Id,
    r: &mut Requirement,
    k: usize,
    voltages: &[f64],
) {
    if k == form(r) {
        return;
    }
    let key = id.with("requirement-memory");
    let memory: Option<Memory> = ctx.data(|d| d.get_temp(key));
    let mut kept = match memory {
        Some(m) if m.shown == *r => m.kept,
        _ => [None, None, None],
    };
    kept[form(r)] = Some(r.clone());
    let next = kept[k].clone().unwrap_or_else(|| convert(r, k, voltages));
    ctx.data_mut(|d| {
        d.insert_temp(
            key,
            Memory {
                shown: next.clone(),
                kept,
            },
        )
    });
    *r = next;
}

/// The voltages to preview: 0 kV (earthed wires) and the project's.
pub fn preview_voltages(voltages: &[f64]) -> Vec<f64> {
    let mut v = vec![0.0];
    v.extend(voltages.iter().copied());
    v.sort_by(f64::total_cmp);
    v.dedup();
    v
}

/// The preview: the distance at each voltage (`"0 kV: 5.00 m"`, or what
/// is missing).
pub fn preview(r: &Requirement, voltages: &[f64]) -> Vec<(f64, Result<f64, String>)> {
    preview_voltages(voltages)
        .into_iter()
        .map(|kv| (kv, r.at(kv)))
        .collect()
}

/// The project's voltages a table does not reach (above its last row).
pub fn uncovered(r: &Requirement, voltages: &[f64]) -> Vec<f64> {
    preview_voltages(voltages)
        .into_iter()
        .filter(|kv| r.at(*kv).is_err())
        .collect()
}

/// What to tell the user about `r`: its problem, or the project's
/// voltages above a table's last row.
pub fn warning(r: &Requirement, voltages: &[f64]) -> Option<String> {
    if let Some(p) = r.problem() {
        return Some(p);
    }
    let above = uncovered(r, voltages);
    if above.is_empty() {
        return None;
    }
    let last = match r {
        Requirement::ByVoltage(rows) => rows.last().map_or(0.0, |x| x.0),
        _ => 0.0,
    };
    Some(format!(
        "the project has {} above the last row ({last} kV): add a row",
        above
            .iter()
            .map(|v| format!("{v} kV"))
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// One line of the preview (0 kV and the project's voltages).
pub fn preview_text(r: &Requirement, voltages: &[f64]) -> String {
    preview(r, voltages)
        .into_iter()
        .map(|(kv, d)| match d {
            Ok(d) => format!("{kv} kV: {d:.2} m"),
            Err(_) => format!("{kv} kV: none"),
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A formula in words: `5 m + 0.01 m/kV above 0 kV`.
pub fn formula_text(base: f64, per_kv: f64, above_kv: f64) -> String {
    format!("{base} m + {per_kv} m/kV above {above_kv} kV")
}

/// The live line under the editor: the distance at each of the line's
/// own system voltages ("At 220 kV: 7.00 m; at 380 kV: 9.00 m", earthed
/// wires last), after the formula written out; without voltages, the
/// rule in words.
pub fn live_text(r: &Requirement, voltages: &[f64]) -> String {
    let mut kvs: Vec<f64> = voltages.to_vec();
    kvs.sort_by(f64::total_cmp);
    kvs.dedup();
    let earthed = kvs.first() == Some(&0.0);
    kvs.retain(|v| *v > 0.0);
    let formula = match r {
        Requirement::Formula {
            base,
            per_kv,
            above_kv,
        } => Some(formula_text(*base, *per_kv, *above_kv)),
        _ => None,
    };
    let at = |kv: f64| match r.at(kv) {
        Ok(d) => format!("{d:.2} m"),
        Err(_) => "no row reaches it".into(),
    };
    let mut parts: Vec<String> = kvs
        .iter()
        .enumerate()
        .map(|(i, kv)| format!("{} {kv} kV: {}", if i == 0 { "At" } else { "at" }, at(*kv)))
        .collect();
    if earthed {
        parts.push(format!(
            "{} (0 kV): {}",
            if parts.is_empty() {
                "Earthed wires"
            } else {
                "earthed wires"
            },
            at(0.0)
        ));
    }
    let at_line = parts.join("; ");
    match (formula, at_line.is_empty()) {
        (Some(f), true) => f,
        (Some(f), false) => format!("{f} — {at_line}"),
        (None, false) => at_line,
        (None, true) => match r {
            Requirement::Fixed(d) => format!("{d:.2} m at every voltage"),
            _ => format!("{r} (the line has no system voltage yet)"),
        },
    }
}

/// Sorts a table's rows by voltage.
pub fn sort_rows(rows: &mut [(f64, f64)]) {
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
}

/// The row "+ Add row" adds: up to the project's lowest voltage above
/// the last row (else 100 kV above it), the last row's distance.
pub fn new_row(rows: &[(f64, f64)], voltages: &[f64]) -> (f64, f64) {
    let last = rows.last().copied();
    let above = |kv: f64| last.is_none_or(|(u, _)| kv > u);
    let kv = preview_voltages(voltages)
        .into_iter()
        .find(|kv| above(*kv))
        .unwrap_or_else(|| last.map_or(0.0, |(u, _)| u + 100.0));
    (kv, last.map_or(0.0, |(_, d)| d))
}

/// The distance editor in a cell of a grid (a rule table's row): the
/// form selector, the fields of the form, the live line, what is missing.
/// True when it changed. `id` makes the widgets unique.
pub fn requirement_ui(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug + Copy,
    r: &mut Requirement,
    voltages: &[f64],
) -> bool {
    ui.vertical(|ui| parts(ui, id, r, voltages)).inner
}

/// [`requirement_ui`] in a panel (the same component).
pub fn requirement_ui_stacked(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug + Copy,
    r: &mut Requirement,
    voltages: &[f64],
) -> bool {
    ui.vertical(|ui| parts(ui, id, r, voltages)).inner
}

/// The same component read-only: greyed, on a faint background, marked
/// [`PREVIEW_NOTE`] (a preset's rule before it is applied).
pub fn requirement_view(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug + Copy,
    r: &Requirement,
    voltages: &[f64],
) {
    egui::Frame::group(ui.style())
        .fill(ui.visuals().faint_bg_color)
        .stroke(egui::Stroke::new(
            1.0,
            ui.visuals().widgets.noninteractive.bg_stroke.color,
        ))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(PREVIEW_NOTE).small().italics().weak())
                .on_hover_text(
                    "Shown, not editable here: apply the preset (or fill the distance from it) \
                     to change its values",
                );
            let mut copy = r.clone();
            ui.add_enabled_ui(false, |ui| {
                ui.vertical(|ui| parts(ui, ("requirement-view", id), &mut copy, voltages));
            });
        });
}

/// Gives the fields a visible frame (an outline in the text colour, faint)
/// so they read as inputs.
fn framed_fields(ui: &mut egui::Ui) {
    let v = ui.visuals_mut();
    let edge = v
        .widgets
        .noninteractive
        .fg_stroke
        .color
        .gamma_multiply(0.45);
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, edge);
    v.widgets.inactive.bg_fill = v.extreme_bg_color;
    v.widgets.inactive.weak_bg_fill = v.extreme_bg_color;
}

fn parts(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug + Copy,
    r: &mut Requirement,
    voltages: &[f64],
) -> bool {
    let mut changed = false;
    let wid = ui.make_persistent_id(("requirement", id));
    // The form selector: three buttons, the form in use lit.
    // Wrapped: the Clearances tab is narrow.
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        framed_fields(ui);
        let k = form(r);
        for (j, name) in FORMS.iter().enumerate() {
            // Buttons, each framed; the form in use lit.
            if ui
                .add(
                    egui::Button::new(*name)
                        .selected(j == k)
                        .frame_when_inactive(true),
                )
                .on_hover_text(FORM_TIPS[j])
                .clicked()
                && j != k
            {
                switch_form(ui.ctx(), wid, r, j, voltages);
                changed = true;
            }
        }
    });
    ui.scope(|ui| {
        framed_fields(ui);
        changed |= fields(ui, id, r, voltages);
    });
    let color = ui.visuals().weak_text_color();
    // Not wrapped: in a grid a wrapped label would resize its column every
    // frame.
    ui.add(
        egui::Label::new(
            egui::RichText::new(live_text(r, voltages))
                .small()
                .color(color),
        )
        .extend(),
    )
    .on_hover_text(format!(
        "The distance at the line's own system voltages (each wire takes its system's nominal \
         voltage; earthed wires 0 kV). At 0 kV and each project voltage: {}",
        preview_text(r, voltages)
    ));
    if let Some(w) = warning(r, voltages) {
        let color = ui.visuals().warn_fg_color;
        ui.add(egui::Label::new(egui::RichText::new(w).small().color(color)).extend());
    }
    changed
}

/// The fields of the form in use.
fn fields(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug + Copy,
    r: &mut Requirement,
    voltages: &[f64],
) -> bool {
    let mut changed = false;
    fn metres(v: &mut f64) -> egui::DragValue<'_> {
        egui::DragValue::new(v)
            .speed(0.05)
            .range(0.0..=200.0)
            .max_decimals(3)
            .suffix(" m")
    }
    match r {
        Requirement::Fixed(d) => {
            ui.horizontal(|ui| {
                ui.label("distance");
                changed |= ui
                    .add(metres(d))
                    .on_hover_text(
                        "The distance to keep: the value of the regulation you work to, for \
                         every voltage (drag or click to type)",
                    )
                    .changed();
            });
        }
        Requirement::ByVoltage(rows) => {
            let mut remove = None;
            egui::Grid::new(("requirement-rows", id))
                .num_columns(3)
                .spacing([8.0, 3.0])
                .show(ui, |ui| {
                    ui.strong("voltage up to").on_hover_text(
                        "A wire takes the first row whose voltage its system's nominal voltage \
                         does not exceed",
                    );
                    ui.strong("distance");
                    ui.label("");
                    ui.end_row();
                    for (k, row) in rows.iter_mut().enumerate() {
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut row.0)
                                    .speed(1.0)
                                    .range(0.0..=2000.0)
                                    .suffix(" kV"),
                            )
                            .on_hover_text("Up to this nominal voltage (kV)")
                            .changed();
                        changed |= ui
                            .add(metres(&mut row.1))
                            .on_hover_text("This distance (m)")
                            .changed();
                        if ui
                            .small_button(REMOVE)
                            .on_hover_text("Remove this row")
                            .clicked()
                        {
                            remove = Some(k);
                        }
                        ui.end_row();
                    }
                });
            if let Some(k) = remove {
                rows.remove(k);
                changed = true;
            }
            let sorted = rows.windows(2).all(|w| w[0].0 <= w[1].0);
            ui.horizontal(|ui| {
                if ui
                    .button("+ Add row")
                    .on_hover_text(
                        "A row up to the project's next voltage above the last row (else 100 kV \
                         above it), with the last row's distance",
                    )
                    .clicked()
                {
                    let row = new_row(rows, voltages);
                    rows.push(row);
                    changed = true;
                }
                if ui
                    .add_enabled(!sorted, egui::Button::new("Sort by voltage"))
                    .on_hover_text("The rows by voltage, lowest first")
                    .on_disabled_hover_text("The rows are in voltage order")
                    .clicked()
                {
                    sort_rows(rows);
                    changed = true;
                }
            });
            let hint = if sorted {
                "Lowest voltage first; the first row a wire's voltage does not exceed applies."
            } else {
                "Not in voltage order: Sort by voltage (the rows must increase)."
            };
            ui.label(egui::RichText::new(hint).small().color(if sorted {
                ui.visuals().weak_text_color()
            } else {
                ui.visuals().warn_fg_color
            }));
        }
        Requirement::Formula {
            base,
            per_kv,
            above_kv,
        } => {
            ui.horizontal_wrapped(|ui| {
                changed |= ui
                    .add(metres(base))
                    .on_hover_text("The base distance (m): at and below the voltage \"above\"")
                    .changed();
                ui.label("+");
                changed |= ui
                    .add(
                        egui::DragValue::new(per_kv)
                            .speed(0.0005)
                            .range(0.0..=1.0)
                            .max_decimals(5)
                            .suffix(" m/kV"),
                    )
                    .on_hover_text("Added per kV of nominal voltage above \"above\" (m per kV)")
                    .changed();
                ui.label("above");
                changed |= ui
                    .add(
                        egui::DragValue::new(above_kv)
                            .speed(1.0)
                            .range(0.0..=2000.0)
                            .suffix(" kV"),
                    )
                    .on_hover_text("The voltage the amount per kV counts from (kV)")
                    .changed();
            });
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forms_convert_and_preview() {
        let v = [220.0, 380.0];
        let f = Requirement::Fixed(7.0);
        let t = convert(&f, 1, &v);
        assert_eq!(t, Requirement::ByVoltage(vec![(380.0, 7.0)]));
        assert_eq!(
            convert(&t, 2, &v),
            Requirement::Formula {
                base: 7.0,
                per_kv: 0.0,
                above_kv: 0.0
            }
        );
        assert_eq!(convert(&t, 0, &v), f);
        // A table to one distance: its largest (on the safe side).
        let t2 = Requirement::ByVoltage(vec![(220.0, 6.0), (380.0, 8.0)]);
        assert_eq!(convert(&t2, 0, &v), Requirement::Fixed(8.0));
        // A formula to a table: a row per voltage of the line, the same
        // distances there: 5 + 0.01 · 220 = 7.2, 5 + 0.01 · 380 = 8.8.
        let fo = Requirement::Formula {
            base: 5.0,
            per_kv: 0.01,
            above_kv: 0.0,
        };
        match convert(&fo, 1, &v) {
            Requirement::ByVoltage(rows) => {
                assert_eq!(rows.len(), 2);
                assert!((rows[0].1 - 7.2).abs() < 1e-12 && (rows[1].1 - 8.8).abs() < 1e-12);
                assert_eq!((rows[0].0, rows[1].0), (220.0, 380.0));
            }
            other => panic!("{other:?}"),
        }
        // 5 m + 0.01 m/kV: 5, 7.2, 8.8 m.
        let p = preview(&fo, &v);
        let d: Vec<f64> = p.iter().map(|x| x.1.clone().unwrap()).collect();
        assert_eq!(
            p.iter().map(|x| x.0).collect::<Vec<_>>(),
            [0.0, 220.0, 380.0]
        );
        assert!((d[1] - 7.2).abs() < 1e-12 && (d[2] - 8.8).abs() < 1e-12);
        // A table up to 245 kV: 380 kV is not reached.
        let t = Requirement::ByVoltage(vec![(0.0, 5.0), (245.0, 6.5)]);
        assert_eq!(uncovered(&t, &v), [380.0]);
        assert!(
            warning(&t, &v)
                .unwrap()
                .contains("380 kV above the last row (245 kV)")
        );
        assert_eq!(new_row(&[(0.0, 5.0), (245.0, 6.5)], &v), (380.0, 6.5));
        assert_eq!(new_row(&[(400.0, 5.0)], &v), (500.0, 5.0));
        let mut rows = vec![(245.0, 6.5), (0.0, 5.0)];
        sort_rows(&mut rows);
        assert_eq!(rows, [(0.0, 5.0), (245.0, 6.5)]);
    }

    /// The live line: the distances at the line's voltages, a formula
    /// written out, earthed wires last.
    #[test]
    fn the_live_line() {
        let t = Requirement::ByVoltage(vec![(220.0, 7.0), (380.0, 9.0)]);
        assert_eq!(
            live_text(&t, &[220.0, 380.0]),
            "At 220 kV: 7.00 m; at 380 kV: 9.00 m"
        );
        assert_eq!(
            live_text(&t, &[0.0, 220.0]),
            "At 220 kV: 7.00 m; earthed wires (0 kV): 7.00 m"
        );
        assert_eq!(
            live_text(&Requirement::ByVoltage(vec![(220.0, 7.0)]), &[380.0]),
            "At 380 kV: no row reaches it"
        );
        let f = Requirement::Formula {
            base: 5.0,
            per_kv: 0.01,
            above_kv: 0.0,
        };
        assert_eq!(live_text(&f, &[]), "5 m + 0.01 m/kV above 0 kV");
        assert_eq!(
            live_text(&f, &[220.0]),
            "5 m + 0.01 m/kV above 0 kV — At 220 kV: 7.20 m"
        );
        assert_eq!(
            live_text(&Requirement::Fixed(4.0), &[]),
            "4.00 m at every voltage"
        );
    }

    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT as _, Queryable};

    const LINE: [f64; 2] = [220.0, 380.0];

    fn editor(r: Requirement) -> Harness<'static, Requirement> {
        let mut h = Harness::new_ui_state(
            |ui, r: &mut Requirement| {
                requirement_ui_stacked(ui, "test", r, &LINE);
            },
            r,
        );
        h.run();
        h
    }

    /// The selector switches the form; back to a form left without typing
    /// gives it back exactly; one distance and a base start from the
    /// largest distance at the line's voltages.
    #[test]
    fn the_form_selector_switches_and_keeps_values() {
        let table = Requirement::ByVoltage(vec![(220.0, 7.0), (380.0, 9.0)]);
        let mut h = editor(table.clone());
        h.get_by_label("One distance").click();
        h.run();
        assert_eq!(*h.state(), Requirement::Fixed(9.0));
        h.get_by_label("distance");
        h.get_by_label("Table by voltage").click();
        h.run();
        assert_eq!(*h.state(), table, "the table left is given back");
        h.get_by_label("Base + per kV").click();
        h.run();
        assert_eq!(
            *h.state(),
            Requirement::Formula {
                base: 9.0,
                per_kv: 0.0,
                above_kv: 0.0
            }
        );
        // Typed in between: the table is made anew from the formula.
        *h.state_mut() = Requirement::Formula {
            base: 5.0,
            per_kv: 0.01,
            above_kv: 0.0,
        };
        h.run();
        h.get_by_label("Table by voltage").click();
        h.run();
        match h.state() {
            Requirement::ByVoltage(rows) => {
                assert_eq!(rows.len(), 2);
                assert!((rows[1].1 - 8.8).abs() < 1e-12);
            }
            other => panic!("{other:?}"),
        }
    }

    /// A table's rows: the header, + Add row (the next voltage), Remove,
    /// Sort by voltage when they are out of order.
    #[test]
    fn table_rows_are_added_and_removed() {
        let mut h = editor(Requirement::ByVoltage(vec![(220.0, 7.0)]));
        h.get_by_label("voltage up to");
        h.get_by_label("+ Add row").click();
        h.run();
        assert_eq!(
            *h.state(),
            Requirement::ByVoltage(vec![(220.0, 7.0), (380.0, 7.0)])
        );
        assert_eq!(h.query_all_by_label(REMOVE).count(), 2);
        assert!(
            h.get_by_label("Sort by voltage")
                .accesskit_node()
                .is_disabled()
        );
        h.query_all_by_label(REMOVE).next().unwrap().click();
        h.run();
        assert_eq!(*h.state(), Requirement::ByVoltage(vec![(380.0, 7.0)]));
        *h.state_mut() = Requirement::ByVoltage(vec![(380.0, 9.0), (220.0, 7.0)]);
        h.run();
        h.get_by_label_contains("Not in voltage order");
        h.get_by_label("Sort by voltage").click();
        h.run();
        assert_eq!(
            *h.state(),
            Requirement::ByVoltage(vec![(220.0, 7.0), (380.0, 9.0)])
        );
    }

    /// The live line follows the values.
    #[test]
    fn the_live_line_is_shown() {
        let mut h = editor(Requirement::ByVoltage(vec![(220.0, 7.0), (380.0, 9.0)]));
        h.get_by_label("At 220 kV: 7.00 m; at 380 kV: 9.00 m");
        *h.state_mut() = Requirement::Formula {
            base: 5.0,
            per_kv: 0.01,
            above_kv: 0.0,
        };
        h.run();
        h.get_by_label("5 m + 0.01 m/kV above 0 kV — At 220 kV: 7.20 m; at 380 kV: 8.80 m");
    }

    /// Read-only: marked as a preview, every field and button greyed out.
    #[test]
    fn a_preview_is_read_only() {
        let mut h = Harness::new_ui(|ui| {
            requirement_view(
                ui,
                "preview",
                &Requirement::ByVoltage(vec![(220.0, 7.0)]),
                &LINE,
            );
        });
        h.run();
        h.get_by_label(PREVIEW_NOTE);
        for name in ["One distance", "+ Add row", REMOVE] {
            assert!(
                h.get_by_label(name).accesskit_node().is_disabled(),
                "{name}"
            );
        }
        h.get_by_label_contains("at 380 kV: no row reaches it");
    }
}
