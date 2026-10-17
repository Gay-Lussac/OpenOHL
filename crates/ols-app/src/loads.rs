// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Checks › Structure loads: the loads the wires and strings put on the
//! structures in each load case (the load case file used: the project's,
//! Criteria › Load cases), computed on demand in the background — the
//! model is not changed. One load case shown at a time; factored or not;
//! a click goes to the structure; CSV export. **Usage**: how much of its
//! capacity each structure uses (its type's allowable spans and loads,
//! structure model and pole, [`ols_loads::capacity`]), the governing load
//! case or one case, worst first.

use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use eframe::egui;
use ols_loads::CaseLoads;
use ols_loads::capacity::{CaseUsage, StructureUsage};

use crate::model::Model;

type Outcome = Result<Vec<CaseLoads>, String>;

/// A computation's answer: the loads, and the usage computed from them.
type Finished = (Outcome, Vec<CaseUsage>);

/// Usage at or above this share is shown as a warning (orange).
const NEAR_FULL: f64 = 0.9;

/// The Structure loads page.
#[derive(Default)]
pub struct LoadsPage {
    /// The last results (or why there are none).
    pub results: Option<Outcome>,
    /// A computation running: since, its answer.
    running: Option<(Instant, Receiver<Finished>)>,
    /// How much of their capacity the structures use, per load case (with
    /// the last results).
    pub usage: Vec<CaseUsage>,
    /// The usage shown instead of the loads.
    pub show_usage: bool,
    /// How long the last one took.
    pub took: Option<std::time::Duration>,
    /// The load case shown.
    pub case: usize,
    /// Structures shown (id contains this text).
    pub filter: String,
    /// Factored loads shown (else unfactored).
    pub unfactored: bool,
    /// The usage's load case: 0 the governing one, k + 1 case k.
    pub usage_case: usize,
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
                "no load case file: Criteria › Load cases (a .olslc file next to the line, \
                 or the project's own)"
                    .into(),
            ));
            return;
        };
        let line = model.line.clone();
        let library = model.library.clone();
        let types = model
            .types
            .as_ref()
            .map(|(t, _)| t.clone())
            .unwrap_or_default();
        let weather: Vec<_> = model.cases.iter().map(|c| c.case.clone()).collect();
        // The structure models' section library (the project's).
        let sections = model
            .section_library()
            .map_err(|e| format!("section library: {e}"));
        // The route worked on only (View › Work on route).
        let scope = model.scope().map(|s| s.structures);
        let (tx, rx) = channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let r = sections
                .clone()
                .and_then(|_| ols_loads::LoadCases::load(&path))
                .and_then(|set| {
                    ols_loads::structure_loads(
                        &line,
                        &library,
                        &weather,
                        &set.cases,
                        &ols_loads::default_budget(),
                    )
                });
            let mut r = r;
            if let Ok(loads) = &mut r {
                ols_loads::add_structure_wind(loads, &line, &types, &weather);
                if let Some(on) = &scope {
                    for c in loads.iter_mut() {
                        c.loads.retain(|l| on.contains(&l.structure));
                    }
                }
            }
            let usage = r.as_ref().map_or_else(
                |_| Vec::new(),
                |loads| {
                    let sections = sections.clone().unwrap_or_default();
                    ols_loads::capacity::usage_with_sections(
                        &line, &types, &sections, &library, &weather, loads,
                    )
                },
            );
            let _ = tx.send((r, usage));
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
            Ok((r, usage)) => {
                self.usage = usage;
                Some(r)
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.usage.clear();
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
                crate::menu::error_label(ui, e);
                return None;
            }
            Some(Ok(r)) => r,
        };
        if results.is_empty() {
            ui.label("The load case file has no case.");
            return None;
        }
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.show_usage, false, "Loads");
            ui.selectable_value(&mut self.show_usage, true, "Usage")
                .on_hover_text(
                    "How much of its capacity each structure uses: its type's allowable spans \
                     (Method 1), allowable loads (Method 2), structure model (fe) and pole",
                );
        });
        if self.show_usage {
            let names: Vec<String> = results.iter().map(|r| r.case.name.clone()).collect();
            // A case with sections not solved misses their loads: said so.
            let incomplete: Vec<String> = results
                .iter()
                .filter(|r| !r.failed.is_empty())
                .map(|r| format!("{} ({} sections)", r.case.name, r.failed.len()))
                .collect();
            if !incomplete.is_empty() {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    format!(
                        "sections not solved in: {} — their loads are missing (see Loads)",
                        incomplete.join(", ")
                    ),
                );
            }
            return self.usage_ui(ui, &names);
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
            crate::menu::error_label(ui, format!("not solved: {f}"));
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
                            "point",
                            "sets",
                            "",
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
                            ui.label(&l.label);
                            ui.label(
                                l.sets
                                    .iter()
                                    .map(u32::to_string)
                                    .collect::<Vec<_>>()
                                    .join(" "),
                            );
                            ui.label("");
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

    /// The usage view; returns a structure to go to.
    fn usage_ui(&mut self, ui: &mut egui::Ui, names: &[String]) -> Option<String> {
        let mut go = None;
        // Load case: 0 = the governing one of each structure, k + 1 = case k.
        ui.horizontal(|ui| {
            let shown = match self.usage_case {
                0 => "governing (every case)".to_owned(),
                k => names.get(k - 1).cloned().unwrap_or_default(),
            };
            egui::ComboBox::from_label("load case")
                .selected_text(shown)
                .width(220.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.usage_case, 0, "governing (every case)");
                    for (k, n) in names.iter().enumerate() {
                        ui.selectable_value(&mut self.usage_case, k + 1, n);
                    }
                });
            ui.label("structure");
            ui.add(egui::TextEdit::singleline(&mut self.filter).desired_width(100.0));
            if ui
                .button("Export CSV…")
                .on_hover_text("Every load case, every structure")
                .clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("CSV", &["csv"])
                    .set_file_name("structure-usage.csv")
                    .save_file()
                && let Err(e) = std::fs::write(&path, ols_loads::capacity::usage_csv(&self.usage))
            {
                log::warn!("{}: {e}", path.display());
            }
        });
        let rows: Vec<(&str, &StructureUsage)> = match self.usage_case {
            0 => ols_loads::capacity::governing(&self.usage),
            k => self.usage.get(k - 1).map_or_else(Vec::new, |c| {
                let mut v: Vec<(&str, &StructureUsage)> =
                    c.structures.iter().map(|s| (c.case.as_str(), s)).collect();
                v.sort_by(|a, b| {
                    let u = |s: &StructureUsage| s.governing().map_or(-1.0, |g| g.usage);
                    u(b.1).total_cmp(&u(a.1))
                });
                v
            }),
        };
        let checked = rows.iter().filter(|r| r.1.governing().is_some()).count();
        let over = rows
            .iter()
            .filter(|r| r.1.governing().is_some_and(|g| g.usage > 1.0))
            .count();
        ui.label(
            egui::RichText::new(format!(
                "{checked} structures checked, {over} over 100 %{}",
                if checked == 0 {
                    " — give the structure types a [capacity] (allowable spans, \
                     allowable loads) to check them"
                } else {
                    ""
                }
            ))
            .small(),
        );
        // Not checked: at the end, with why.
        let unchecked: Vec<&StructureUsage> = match self.usage_case {
            0 => self
                .usage
                .first()
                .map(|c| {
                    c.structures
                        .iter()
                        .filter(|s| s.governing().is_none())
                        .collect()
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let filter = self.filter.trim().to_lowercase();
        let pct = |c: Option<&ols_loads::capacity::Check>| match c {
            None => egui::RichText::new("—").weak(),
            Some(c) => {
                let text = if c.usage.is_finite() {
                    format!("{:.0} %", c.usage * 100.0)
                } else {
                    "not allowed".into()
                };
                let colour = if c.usage > 1.0 {
                    egui::Color32::LIGHT_RED
                } else if c.usage >= NEAR_FULL {
                    egui::Color32::from_rgb(255, 170, 60)
                } else {
                    egui::Color32::LIGHT_GREEN
                };
                egui::RichText::new(text).monospace().color(colour)
            }
        };
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("structure-usage")
                    .striped(true)
                    .spacing([14.0, 3.0])
                    .show(ui, |ui| {
                        for h in [
                            "structure",
                            "type",
                            "usage",
                            "spans",
                            "loads",
                            "fe",
                            "pole",
                            "governs",
                            "load case",
                            "angle °",
                            "wind span m",
                            "weight span m",
                        ] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        let shown = rows
                            .iter()
                            .filter(|r| r.1.governing().is_some())
                            .chain(
                                unchecked
                                    .iter()
                                    .map(|s| ("", *s))
                                    .collect::<Vec<_>>()
                                    .iter(),
                            )
                            .filter(|r| {
                                filter.is_empty() || r.1.structure.to_lowercase().contains(&filter)
                            })
                            .take(5000)
                            .cloned()
                            .collect::<Vec<_>>();
                        for (case, s) in shown {
                            if ui
                                .link(&s.structure)
                                .on_hover_text("Go to this structure")
                                .clicked()
                            {
                                go = Some(s.structure.clone());
                            }
                            ui.label(s.structure_type.as_deref().unwrap_or("—"));
                            let g = s.governing();
                            ui.label(pct(g));
                            ui.label(pct(s.spans.as_ref()));
                            ui.label(pct(s.loads.as_ref()));
                            ui.label(pct(s.fe.as_ref()));
                            ui.label(pct(s.pole.as_ref()));
                            match g {
                                Some(g) => ui.label(&g.governs),
                                None => ui.label(egui::RichText::new(s.notes.join("; ")).weak()),
                            };
                            ui.label(case);
                            ui.label(format!("{:.1}", s.line_angle_deg));
                            ui.label(format!("{:.0}", s.wind_span));
                            ui.label(s.weight_span.map_or(String::new(), |[lo, hi]| {
                                if (hi - lo).abs() < 0.5 {
                                    format!("{hi:.0}")
                                } else {
                                    format!("{lo:.0} – {hi:.0}")
                                }
                            }));
                            ui.end_row();
                        }
                    });
            });
        go
    }
}
