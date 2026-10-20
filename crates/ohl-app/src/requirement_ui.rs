// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A clearance rule's distance, as the user sets it wherever a rule is
//! edited (the Vertical / Radial clearances windows and their "Add a
//! rule", the Clearances tab's rule editor): **one distance**, a **table
//! "up to kV → distance"** (rows added, removed and sorted; voltages
//! increasing, distances above 0; a hint when the project has a system
//! above the last row), or a **base plus an amount per kV** above a
//! voltage ([`Requirement`]). A preview gives the distance at each nominal
//! voltage of the project's systems and at 0 kV (earthed wires). The
//! table and the formula are edited in a menu that stays open.

use eframe::egui;
use ohl_clearance::rules::Requirement;

/// The three forms, as the form chooser names them.
const FORMS: [&str; 3] = ["one distance", "table by voltage", "base + per kV"];

fn form(r: &Requirement) -> usize {
    match r {
        Requirement::Fixed(_) => 0,
        Requirement::ByVoltage(_) => 1,
        Requirement::Formula { .. } => 2,
    }
}

/// The distance a requirement starts the other forms with: a distance,
/// the first row's, the base.
fn first_distance(r: &Requirement) -> f64 {
    match r {
        Requirement::Fixed(d) => *d,
        Requirement::ByVoltage(rows) => rows.first().map_or(0.0, |x| x.1),
        Requirement::Formula { base, .. } => *base,
    }
}

/// `r` turned into form `k`, keeping its distance: a table of one row up
/// to the project's highest voltage, a formula with nothing per kV yet.
pub fn convert(r: &Requirement, k: usize, voltages: &[f64]) -> Requirement {
    let d = first_distance(r);
    match k {
        0 => Requirement::Fixed(d),
        1 => Requirement::ByVoltage(vec![(voltages.iter().copied().fold(0.0, f64::max), d)]),
        _ => Requirement::Formula {
            base: d,
            per_kv: 0.0,
            above_kv: 0.0,
        },
    }
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

/// One line of the preview.
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

/// Sorts a table's rows by voltage.
pub fn sort_rows(rows: &mut [(f64, f64)]) {
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
}

/// The row "Add row" adds: up to the project's lowest voltage above the
/// last row (else 100 kV above it), the last row's distance.
pub fn new_row(rows: &[(f64, f64)], voltages: &[f64]) -> (f64, f64) {
    let last = rows.last().copied();
    let above = |kv: f64| last.is_none_or(|(u, _)| kv > u);
    let kv = preview_voltages(voltages)
        .into_iter()
        .find(|kv| above(*kv))
        .unwrap_or_else(|| last.map_or(0.0, |(u, _)| u + 100.0));
    (kv, last.map_or(0.0, |(_, d)| d))
}

/// The distance editor in a row (a grid's cell): the form, then the
/// distance (one distance) or a summary opening the table / formula with
/// its preview, then what is missing. True when it changed. `id` makes
/// the widgets unique.
pub fn requirement_ui(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug + Copy,
    r: &mut Requirement,
    voltages: &[f64],
) -> bool {
    ui.horizontal(|ui| parts(ui, id, r, voltages, false)).inner
}

/// [`requirement_ui`] for a narrow panel: the form and a distance on one
/// line, a table's or formula's button below it, then what is missing.
pub fn requirement_ui_stacked(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug + Copy,
    r: &mut Requirement,
    voltages: &[f64],
) -> bool {
    ui.vertical(|ui| parts(ui, id, r, voltages, true)).inner
}

fn parts(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug + Copy,
    r: &mut Requirement,
    voltages: &[f64],
    stacked: bool,
) -> bool {
    let mut changed = false;
    let mut first = |ui: &mut egui::Ui| {
        let mut k = form(r);
        egui::ComboBox::from_id_salt(("requirement-form", id))
            .selected_text(FORMS[k])
            .width(118.0)
            .show_ui(ui, |ui| {
                for (j, name) in FORMS.iter().enumerate() {
                    ui.selectable_value(&mut k, j, *name);
                }
            })
            .response
            .on_hover_text(
                "One distance for every voltage; a table \"up to kV: distance\" (the first \
                 row whose voltage is not exceeded); or a base distance plus an amount per kV \
                 above a voltage. Earthed wires count as 0 kV.",
            );
        if k != form(r) {
            *r = convert(r, k, voltages);
            changed = true;
        }
        if let Requirement::Fixed(d) = r {
            changed |= ui
                .add(
                    egui::DragValue::new(d)
                        .speed(0.05)
                        .range(0.0..=200.0)
                        .max_decimals(3)
                        .suffix(" m"),
                )
                .on_hover_text(
                    "The clearance to keep: the value of the regulation you work to, for \
                     every voltage",
                )
                .changed();
        }
    };
    if stacked {
        ui.horizontal(|ui| first(ui));
    } else {
        first(ui);
    }
    if !matches!(r, Requirement::Fixed(_)) {
        let title = summary(r);
        ui.push_id(("requirement-edit", id), |ui| {
            crate::menu::options_menu(ui, &title, |ui| {
                changed |= editor(ui, id, r, voltages);
            });
        });
    }
    if let Some(w) = warning(r, voltages) {
        let color = ui.visuals().warn_fg_color;
        // Not wrapped: in a grid a wrapped label would resize its column
        // every frame.
        ui.add(egui::Label::new(egui::RichText::new(w).small().color(color)).extend());
    }
    changed
}

/// The button text of a table or formula.
pub fn summary(r: &Requirement) -> String {
    match r {
        Requirement::Fixed(d) => format!("{d} m"),
        Requirement::ByVoltage(rows) => format!(
            "{} row{} up to {} kV…",
            rows.len(),
            if rows.len() == 1 { "" } else { "s" },
            rows.last().map_or(0.0, |x| x.0)
        ),
        Requirement::Formula {
            base,
            per_kv,
            above_kv,
        } => format!("{base} m + {per_kv} m/kV above {above_kv} kV…"),
    }
}

/// The table or formula with its preview (in the menu).
pub fn editor(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug + Copy,
    r: &mut Requirement,
    voltages: &[f64],
) -> bool {
    let mut changed = false;
    match r {
        Requirement::Fixed(d) => {
            changed |= ui
                .add(
                    egui::DragValue::new(d)
                        .speed(0.05)
                        .range(0.0..=200.0)
                        .suffix(" m"),
                )
                .changed();
        }
        Requirement::ByVoltage(rows) => {
            ui.label(
                egui::RichText::new(
                    "Each row: up to this nominal voltage, this distance. A wire takes the \
                     first row its voltage does not exceed (earthed wires: 0 kV).",
                )
                .small()
                .weak(),
            );
            let mut remove = None;
            egui::Grid::new(("requirement-rows", id))
                .num_columns(3)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.strong("up to kV");
                    ui.strong("distance m");
                    ui.label("");
                    ui.end_row();
                    for (k, row) in rows.iter_mut().enumerate() {
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut row.0)
                                    .speed(1.0)
                                    .range(0.0..=2000.0),
                            )
                            .changed();
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut row.1)
                                    .speed(0.05)
                                    .range(0.0..=200.0)
                                    .max_decimals(3),
                            )
                            .changed();
                        if ui.small_button("Remove").clicked() {
                            remove = Some(k);
                        }
                        ui.end_row();
                    }
                });
            if let Some(k) = remove {
                rows.remove(k);
                changed = true;
            }
            ui.horizontal(|ui| {
                if ui.button("Add row").clicked() {
                    let row = new_row(rows, voltages);
                    rows.push(row);
                    changed = true;
                }
                let sorted = rows.windows(2).all(|w| w[0].0 <= w[1].0);
                if ui
                    .add_enabled(!sorted, egui::Button::new("Sort"))
                    .on_hover_text("The rows by voltage, lowest first")
                    .clicked()
                {
                    sort_rows(rows);
                    changed = true;
                }
            });
        }
        Requirement::Formula {
            base,
            per_kv,
            above_kv,
        } => {
            ui.label(
                egui::RichText::new(
                    "base + per kV · (U − from kV) for a nominal voltage U above \"from\", \
                     else the base (earthed wires: 0 kV).",
                )
                .small()
                .weak(),
            );
            egui::Grid::new(("requirement-formula", id))
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("base m");
                    changed |= ui
                        .add(egui::DragValue::new(base).speed(0.05).range(0.0..=200.0))
                        .changed();
                    ui.end_row();
                    ui.label("per kV m");
                    changed |= ui
                        .add(
                            egui::DragValue::new(per_kv)
                                .speed(0.0005)
                                .range(0.0..=1.0)
                                .max_decimals(5),
                        )
                        .changed();
                    ui.end_row();
                    ui.label("from kV");
                    changed |= ui
                        .add(
                            egui::DragValue::new(above_kv)
                                .speed(1.0)
                                .range(0.0..=2000.0),
                        )
                        .changed();
                    ui.end_row();
                });
        }
    }
    if let Some(w) = warning(r, voltages) {
        ui.colored_label(ui.visuals().warn_fg_color, egui::RichText::new(w).small());
    }
    ui.separator();
    ui.label(egui::RichText::new("At the project's voltages").strong());
    for (kv, d) in preview(r, voltages) {
        let name = if kv == 0.0 {
            "0 kV (earthed)".to_owned()
        } else {
            format!("{kv} kV")
        };
        match d {
            Ok(d) => ui.label(format!("{name}: {d:.2} m")),
            Err(_) => ui.colored_label(
                ui.visuals().warn_fg_color,
                format!("{name}: no row reaches it"),
            ),
        };
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
        // 5 m + 0.01 m/kV: 5, 7.2, 8.8 m.
        let p = preview(
            &Requirement::Formula {
                base: 5.0,
                per_kv: 0.01,
                above_kv: 0.0,
            },
            &v,
        );
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
}
