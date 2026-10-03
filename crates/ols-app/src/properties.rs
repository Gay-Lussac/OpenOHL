// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The properties of the selection: a structure's data and attachments
//! with the sections through it, a span's section, electrical system and
//! results in every weather case.

use eframe::egui;

use crate::model::Model;
use crate::selection::Selection;

fn grid(ui: &mut egui::Ui, id: &str, rows: &[(&str, String)]) {
    egui::Grid::new(id)
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            for (k, v) in rows {
                ui.label(*k);
                ui.label(v);
                ui.end_row();
            }
        });
}

/// Shows the properties of `selection`; clicking a related object selects
/// it.
pub fn ui(ui: &mut egui::Ui, model: &Model, case: usize, selection: &mut Selection) {
    let line = &model.line;
    let mut next: Option<Selection> = None;
    egui::ScrollArea::vertical().show(ui, |ui| match selection.clone() {
        Selection::None => {
            ui.label("Click a structure or a wire in a view.");
            ui.separator();
            ui.heading(&line.name);
            grid(
                ui,
                "line",
                &[
                    ("CRS", line.crs.clone()),
                    ("structures", line.structures.len().to_string()),
                    ("sections", line.sections.len().to_string()),
                    ("jumpers", line.jumpers.len().to_string()),
                    ("systems", line.systems.len().to_string()),
                    ("weather cases", model.cases.len().to_string()),
                ],
            );
        }
        Selection::Structure(id) => {
            let Some(s) = line.structure(&id) else {
                ui.label(format!("{id}: not in the line"));
                return;
            };
            ui.heading(format!("Structure {id}"));
            grid(
                ui,
                "structure",
                &[
                    (
                        "type",
                        s.structure_type.clone().unwrap_or_else(|| "—".into()),
                    ),
                    (
                        "extension",
                        s.extension.clone().unwrap_or_else(|| "—".into()),
                    ),
                    ("easting", format!("{:.3} m", s.position[0])),
                    ("northing", format!("{:.3} m", s.position[1])),
                    ("ground", format!("{:.3} m", s.position[2])),
                    ("orientation", format!("{:.2}°", s.orientation_deg)),
                    ("routes", line.routes_of(&id).join(", ")),
                ],
            );
            ui.separator();
            ui.label(egui::RichText::new("Attachments").strong());
            egui::Grid::new("attachments").striped(true).show(ui, |ui| {
                for h in ["set", "phase", "label", "string", "clamp z", "weight"] {
                    ui.label(egui::RichText::new(h).small());
                }
                ui.end_row();
                for a in &s.attachments {
                    ui.label(a.set.to_string());
                    ui.label(a.phase.to_string());
                    ui.label(&a.label);
                    let kind = if a.legs.is_some() {
                        "V-string"
                    } else if a.strain_member {
                        "strain"
                    } else {
                        "I / fixed"
                    };
                    ui.label(kind);
                    ui.label(format!("{:.2}", a.wire_point[2]));
                    ui.label(format!("{:.0} N", a.insulator_weight));
                    ui.end_row();
                }
            });
            ui.separator();
            ui.label(egui::RichText::new("Spans at this structure").strong());
            for sec in &line.sections {
                for (k, w) in sec.points.windows(2).enumerate() {
                    if w[0].structure == id || w[1].structure == id {
                        let text = format!(
                            "{} span {} ({} {}): {} → {}",
                            sec.id,
                            k + 1,
                            sec.circuit,
                            sec.phase_label,
                            w[0].structure,
                            w[1].structure
                        );
                        if ui.link(text).clicked() {
                            next = Some(Selection::Span {
                                section: sec.id.clone(),
                                span: k,
                            });
                        }
                    }
                }
            }
            let jumpers: Vec<&ols_line::Jumper> = line
                .jumpers
                .iter()
                .filter(|jp| jp.from.structure == id || jp.to.structure == id)
                .collect();
            if !jumpers.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new("Jumpers at this structure").strong());
                for jp in jumpers {
                    let text = format!("{}: set {} → set {}", jp.id, jp.from.set, jp.to.set);
                    if ui.link(text).clicked() {
                        next = Some(Selection::Jumper(jp.id.clone()));
                    }
                }
            }
        }
        Selection::Jumper(id) => {
            let Some(j) = line.jumpers.iter().find(|j| j.id == id) else {
                ui.label(format!("{id}: not in the line"));
                return;
            };
            ui.heading(format!("Jumper {id}"));
            ui.horizontal(|ui| {
                for end in [&j.from, &j.to] {
                    if ui
                        .link(format!(
                            "{} set {} phase {}",
                            end.structure, end.set, end.phase
                        ))
                        .clicked()
                    {
                        next = Some(Selection::Structure(end.structure.clone()));
                    }
                }
            });
            let section = line.jumper_section(j);
            let b = line.jumper_bundle(j);
            // Where each bundle value comes from: the jumper or its section.
            let from = |own: bool| {
                if own {
                    "set on the jumper"
                } else {
                    "from its section"
                }
            };
            grid(
                ui,
                "jumper",
                &[
                    ("cable", j.cable.clone()),
                    (
                        "section",
                        section.map_or("—".into(), |s| {
                            format!("{} ({} {})", s.id, s.circuit, s.phase_label)
                        }),
                    ),
                    (
                        "bundle",
                        match (b.count, b.spacing) {
                            (1, _) => format!("single ({})", from(j.bundle.is_some())),
                            (n, Some(s)) => format!(
                                "{n} × {:.0} mm ({})",
                                s * 1000.0,
                                from(j.bundle.is_some() || j.bundle_spacing.is_some())
                            ),
                            (n, None) => format!("{n}, no spacing ({})", from(j.bundle.is_some())),
                        },
                    ),
                    (
                        "defined by",
                        match (j.length, j.sag) {
                            (Some(l), _) => format!("length {l:.3} m at {} °C", j.temperature),
                            (_, Some(s)) => format!("sag {s:.2} m at {} °C", j.temperature),
                            _ => "—".into(),
                        },
                    ),
                    (
                        "support string",
                        j.support.as_ref().map_or("none".into(), |s| {
                            format!("{:.2} m, {:.0} N", s.length, s.weight)
                        }),
                    ),
                ],
            );
            ui.separator();
            ui.label(egui::RichText::new("Weather cases").strong());
            egui::Grid::new("jumper-cases")
                .striped(true)
                .show(ui, |ui| {
                    for h in ["case", "length (m)", "sag (m)", "max T (N)", "low z (m)"] {
                        ui.label(egui::RichText::new(h).small());
                    }
                    ui.end_row();
                    for (k, solved) in model.cases.iter().enumerate() {
                        let name = if k == case {
                            egui::RichText::new(&solved.case.name).strong()
                        } else {
                            egui::RichText::new(&solved.case.name)
                        };
                        ui.label(name);
                        match solved.jumpers.iter().find(|x| x.jumper == id) {
                            Some(js) => {
                                ui.label(format!("{:.3}", js.unstressed_length));
                                ui.label(format!("{:.3}", js.sag));
                                ui.label(format!("{:.0}", js.max_tension));
                                ui.label(format!("{:.2}", js.low_point[2]));
                            }
                            None => {
                                ui.label("not solved");
                            }
                        }
                        ui.end_row();
                    }
                });
            if let Some(why) = model
                .cases
                .get(case)
                .and_then(|c| c.failed.iter().find(|f| f.starts_with(&format!("{id}:"))))
            {
                ui.colored_label(egui::Color32::LIGHT_RED, why);
            }
        }
        Selection::Span { section, span } => {
            let Some(sec) = line.sections.iter().find(|s| s.id == section) else {
                ui.label(format!("{section}: not in the line"));
                return;
            };
            let (a, b) = (&sec.points[span].structure, &sec.points[span + 1].structure);
            ui.heading(format!("{section} span {}", span + 1));
            ui.horizontal(|ui| {
                for st in [a, b] {
                    if ui.link(format!("structure {st}")).clicked() {
                        next = Some(Selection::Structure(st.clone()));
                    }
                }
            });
            let phases = line.section_phases().unwrap_or_default();
            let system = phases
                .get(&section)
                .copied()
                .flatten()
                .map(|(k, p)| {
                    let s = &line.systems[k];
                    format!(
                        "{} {} — {} kV {}{}",
                        s.id,
                        s.phases[p].label,
                        s.voltage_kv,
                        match s.kind {
                            ols_line::electrical::SystemKind::Ac => "AC",
                            ols_line::electrical::SystemKind::Dc => "DC",
                        },
                        s.frequency_hz.map_or(String::new(), |f| format!(" {f} Hz"))
                    )
                })
                .unwrap_or_else(|| "earthed (no system)".into());
            grid(
                ui,
                "span",
                &[
                    ("cable", sec.cable.clone()),
                    (
                        "circuit / phase",
                        format!("{} {}", sec.circuit, sec.phase_label),
                    ),
                    ("system", system),
                    (
                        "bundle",
                        match (sec.bundle, sec.bundle_spacing) {
                            (1, _) => "single".into(),
                            (n, Some(s)) => format!("{n} × {:.0} mm", s * 1000.0),
                            (n, None) => format!("{n} (no spacing)"),
                        },
                    ),
                    ("spans in section", (sec.points.len() - 1).to_string()),
                ],
            );
            ui.separator();
            ui.label(egui::RichText::new("Weather cases").strong());
            egui::Grid::new("cases").striped(true).show(ui, |ui| {
                for h in ["case", "H (kN)", "max T (kN)", "sag (m)", "low z (m)"] {
                    ui.label(egui::RichText::new(h).small());
                }
                ui.end_row();
                for (k, solved) in model.cases.iter().enumerate() {
                    let name = if k == case {
                        egui::RichText::new(&solved.case.name).strong()
                    } else {
                        egui::RichText::new(&solved.case.name)
                    };
                    ui.label(name);
                    match solved
                        .shapes
                        .iter()
                        .find(|x| x.section == section)
                        .and_then(|x| x.spans.get(span))
                    {
                        Some(sp) => {
                            ui.label(format!("{:.2}", sp.horizontal_tension / 1e3));
                            ui.label(format!("{:.2}", sp.max_tension / 1e3));
                            ui.label(format!("{:.3}", sp.sag));
                            ui.label(format!("{:.2}", sp.low_point[2]));
                        }
                        None => {
                            ui.label("not solved");
                        }
                    }
                    ui.end_row();
                }
            });
            if let Some(why) = model.cases.get(case).and_then(|c| {
                c.failed
                    .iter()
                    .find(|f| f.starts_with(&format!("{section}:")))
            }) {
                ui.colored_label(egui::Color32::LIGHT_RED, why);
            }
        }
    });
    if let Some(n) = next {
        *selection = n;
    }
}
