// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Structure loads: the loads the wires and strings put on the
//! structures in each load case (the load case file used: the project's,
//! Line › Criteria › Load cases), computed on demand in the background — the
//! model is not changed. One load case shown at a time; factored or not;
//! a click goes to the structure; CSV export. **Usage**: how much of its
//! capacity each structure uses (its type's allowable spans and loads,
//! structure model and pole, [`ohl_loads::capacity`]), the governing load
//! case or one case, worst first. A structure whose loads in a case lack
//! a section or jumper that did not solve there is **incomplete**: its
//! rows are greyed, "not checked: S12 not solved in ice", never a pass.
//! **Colour structures by usage** colours them in the plan and 3D views
//! ([`UsageMark`], with a legend): incomplete ones in a neutral colour,
//! "not checked".

use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use eframe::egui;
use ohl_loads::CaseLoads;
use ohl_loads::capacity::{CaseUsage, StructureUsage, UsageStatus};
// Usage from `ohl_structures_fe::check::NEAR_FULL` is shown as a warning
// (orange): one classification for both applications.
use ohl_structures_fe::check::{UsageClass, classify_usage};

use crate::check_help as help;
use crate::model::Model;

/// What its results are computed from ([`crate::revision`]).
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::LINE;

type Outcome = Result<Vec<CaseLoads>, String>;

/// A computation's answer: the loads, and the usage computed from them.
type Finished = (Outcome, Vec<CaseUsage>);

/// Colour of what is not checked because a wire did not solve (grey).
const INCOMPLETE: egui::Color32 = egui::Color32::from_rgb(150, 156, 170);

/// How a structure is coloured by usage in the plan and 3D views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageMark {
    /// Below 90 %.
    Within,
    /// From 90 % to 100 %.
    Near,
    /// Over 100 % (or not allowed).
    Over,
    /// Not checked: a section or jumper at it did not solve (its loads
    /// are incomplete).
    Incomplete,
    /// Approximate: its finite-element check kept tension-only members
    /// active because their set did not settle.
    Approximate,
    /// Nothing to check against (no type, no capacity).
    Unchecked,
}

impl UsageMark {
    /// Every mark, in the legend's order.
    pub const ALL: [UsageMark; 6] = [
        UsageMark::Within,
        UsageMark::Near,
        UsageMark::Over,
        UsageMark::Incomplete,
        UsageMark::Approximate,
        UsageMark::Unchecked,
    ];

    /// The mark of a structure's usage (in one case, or its governing one).
    pub fn of(s: &StructureUsage) -> Self {
        match s.status() {
            UsageStatus::Incomplete => Self::Incomplete,
            UsageStatus::Approximate => Self::Approximate,
            UsageStatus::NotChecked => Self::Unchecked,
            UsageStatus::Over => Self::Over,
            UsageStatus::Pass => match s.governing() {
                Some(g) if classify_usage(g.usage) == UsageClass::Near => Self::Near,
                _ => Self::Within,
            },
        }
    }

    /// Its colour.
    pub fn color(self) -> [u8; 3] {
        match self {
            Self::Within => [110, 200, 120],
            Self::Near => [255, 170, 60],
            Self::Over => [235, 70, 60],
            Self::Incomplete => [150, 156, 170],
            Self::Approximate => [175, 120, 220],
            Self::Unchecked => [95, 100, 110],
        }
    }

    /// In the legend.
    pub fn label(self) -> &'static str {
        match self {
            Self::Within => "below 90 %",
            Self::Near => "90 – 100 %",
            Self::Over => "over 100 %",
            Self::Incomplete => "not checked: a wire not solved",
            Self::Approximate => "approximate (tension-only kept)",
            Self::Unchecked => "no capacity to check",
        }
    }
}

/// The legend of the colours by usage, at the bottom right of `rect`.
pub fn paint_usage_legend(painter: &egui::Painter, rect: egui::Rect, out_of_date: bool) {
    let font = egui::FontId::proportional(11.0);
    let row = 15.0;
    let width = 190.0;
    let height = row * UsageMark::ALL.len() as f32 + 22.0;
    let at = rect.right_bottom() + egui::vec2(-8.0 - width, -8.0 - height);
    let frame = egui::Rect::from_min_size(at, egui::vec2(width, height));
    painter.rect_filled(frame, 4.0, egui::Color32::from_black_alpha(170));
    painter.text(
        at + egui::vec2(6.0, 4.0),
        egui::Align2::LEFT_TOP,
        if out_of_date {
            "structures by usage: OUT OF DATE"
        } else {
            "structures by usage"
        },
        font.clone(),
        if out_of_date {
            egui::Color32::from_rgb(255, 190, 60)
        } else {
            egui::Color32::WHITE
        },
    );
    for (k, m) in UsageMark::ALL.iter().enumerate() {
        let y = at.y + 20.0 + row * k as f32;
        let [r, g, b] = m.color();
        painter.circle_filled(
            egui::pos2(at.x + 12.0, y + 6.0),
            4.5,
            egui::Color32::from_rgb(r, g, b),
        );
        painter.text(
            egui::pos2(at.x + 22.0, y),
            egui::Align2::LEFT_TOP,
            m.label(),
            font.clone(),
            egui::Color32::WHITE,
        );
    }
}

/// The Structure loads page.
#[derive(Default)]
pub struct LoadsPage {
    /// The last results (or why there are none).
    pub results: Option<Outcome>,
    /// The structure override of its cases (its loads are at the
    /// structures: a load case at a structure with groups only when its
    /// weather case is in them; the section override not offered).
    pub overrides: crate::case_groups::Overrides,
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
    /// The structures coloured by usage in the plan and 3D views (the
    /// usage's load case).
    pub colour_structures: bool,
    /// What the results were computed from ([`crate::revision`]).
    stamp: crate::revision::Stamp,
}

impl LoadsPage {
    /// Whether a computation runs.
    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    /// The colour of each structure by usage in the usage's load case (the
    /// governing one: incomplete in any case is incomplete), when
    /// colouring by usage is on (else none).
    pub fn usage_marks(&self) -> std::collections::BTreeMap<String, UsageMark> {
        let mut out = std::collections::BTreeMap::new();
        if !self.colour_structures {
            return out;
        }
        match self.usage_case {
            0 => {
                for c in &self.usage {
                    for s in &c.structures {
                        out.entry(s.structure.clone())
                            .or_insert(UsageMark::Unchecked);
                    }
                }
                for (_, s) in ohl_loads::capacity::governing(&self.usage) {
                    out.insert(s.structure.clone(), UsageMark::of(s));
                }
                for c in &self.usage {
                    for s in c.structures.iter().filter(|s| s.is_incomplete()) {
                        out.insert(s.structure.clone(), UsageMark::Incomplete);
                    }
                }
            }
            k => {
                if let Some(c) = self.usage.get(k - 1) {
                    for s in &c.structures {
                        out.insert(s.structure.clone(), UsageMark::of(s));
                    }
                }
            }
        }
        out
    }

    /// Computes the loads of the model as it is now (in the background).
    pub fn start(&mut self, model: &Model, ctx: &egui::Context) {
        let Some(path) = model.sources.load_cases.clone() else {
            self.results = Some(Err(
                "no load case file: Line › Criteria › Load cases library (the project's own, or a \
                 .ohllc file next to a line file)"
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
            .map_err(|e| format!("cross-section library: {e}"));
        // The route worked on only (View › Work on route).
        let scope = model.scope().map(|s| s.structures);
        let filter = self.overrides.filter(model);
        let (tx, rx) = channel();
        let c = ctx.clone();
        self.stamp.started(model.revision());
        std::thread::spawn(move || {
            let r = sections
                .clone()
                .and_then(|_| ohl_loads::LoadCases::load(&path))
                .and_then(|set| {
                    ohl_loads::structure_loads(
                        &line,
                        &library,
                        &weather,
                        &set.cases,
                        &ohl_loads::default_budget(),
                    )
                });
            let mut r = r;
            if let Ok(loads) = &mut r {
                ohl_loads::add_structure_wind(loads, &line, &types, &weather);
                if let Some(on) = &scope {
                    for c in loads.iter_mut() {
                        c.loads.retain(|l| on.contains(&l.structure));
                        c.missing.retain(|m| on.contains(&m.structure));
                    }
                }
                // The structure override: a load case at a structure with
                // groups only when its weather case is in them.
                for c in loads.iter_mut() {
                    let w = c.case.weather.clone();
                    c.loads
                        .retain(|l| filter.takes(Some(&l.structure), None, &w));
                    c.missing
                        .retain(|m| filter.takes(Some(&m.structure), None, &w));
                }
            }
            let usage = r.as_ref().map_or_else(
                |_| Vec::new(),
                |loads| {
                    let sections = sections.clone().unwrap_or_default();
                    ohl_loads::capacity::usage_with_sections(
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
            self.stamp.arrived();
            self.running = None;
        }
    }

    /// What the results were computed from that changed since, in words
    /// (`None`: nothing, or no results): the line, its cases and files
    /// ([`crate::revision`]).
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        matches!(self.results, Some(Ok(_)))
            .then(|| self.stamp.out_of_date(model, USES))
            .flatten()
    }

    /// The page; returns a structure to go to. Run in the footer at the
    /// bottom left, always visible (owner, 2026-10-21); the rest scrolls
    /// above it.
    pub fn ui(&mut self, ui: &mut egui::Ui, model: &Model) -> Option<String> {
        let go = help::body(ui, "loads-page", |ui| self.body_ui(ui, model));
        help::footer(ui, "loads-page", |ui| {
            ui.checkbox(&mut self.overrides.structures, "structure override")
                .on_hover_text(
                    "At a structure with weather case groups (Structures table): only the load \
                     cases whose weather case is in them",
                );
            if help::run_button(
                ui,
                !self.running(),
                "Solve every section in every load case (finite elements)",
                None,
            ) {
                self.start(model, ui.ctx());
            }
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
        go
    }

    /// The page above its footer.
    fn body_ui(&mut self, ui: &mut egui::Ui, model: &Model) -> Option<String> {
        let mut go = None;
        ui.horizontal(|ui| {
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
        });
        if model.sources.load_cases.is_none() {
            // Expected in a new project: not an error.
            ui.label("No load case file yet: the structure loads are computed for its cases.");
            ui.label(
                egui::RichText::new(
                    "Add one in the Load cases library (Line › Criteria): Browse… for a .ohllc \
                     file, or New file… from a template.",
                )
                .small()
                .weak(),
            );
            if ui.button("Open the Load cases library").clicked() {
                crate::menu::request_page(
                    ui.ctx(),
                    crate::app::Tab::Library(crate::library::LibraryKind::LoadCases),
                );
            }
            return None;
        }
        let results = match &self.results {
            None => {
                ui.label(
                    "The loads the wires and strings put on the structures, at every \
                     attachment, in each load case. Run to see them.",
                );
                return None;
            }
            Some(Err(e)) => {
                crate::menu::error_label(ui, e);
                return None;
            }
            Some(Ok(r)) => r,
        };
        if let Some(what) = self.out_of_date(model)
            && crate::revision::banner(ui, &what, !self.running())
        {
            self.start(model, ui.ctx());
            return None;
        }
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
                .map(|r| {
                    let mut at: Vec<&str> =
                        r.missing.iter().map(|m| m.structure.as_str()).collect();
                    at.dedup();
                    format!(
                        "{} ({} sections or jumpers; incomplete at {} structures)",
                        r.case.name,
                        r.failed.len(),
                        at.len()
                    )
                })
                .collect();
            if !incomplete.is_empty() {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!(
                        "not solved in: {} — the structures they hang on are not checked there \
                         (grey); see Loads",
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
                && let Err(e) = std::fs::write(&path, ohl_loads::to_csv(results))
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
        if !r.failed.is_empty() {
            // In a box of its own height, scrolled: a long list stays small.
            egui::ScrollArea::vertical()
                .id_salt("loads-not-solved")
                .max_height(90.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for f in &r.failed {
                        crate::menu::error_label(ui, format!("not solved: {f}"));
                    }
                    // The structures whose loads lack them.
                    let mut by: std::collections::BTreeMap<&str, Vec<String>> =
                        std::collections::BTreeMap::new();
                    for m in &r.missing {
                        by.entry(m.structure.as_str()).or_default().push(m.short());
                    }
                    for (s, wires) in by {
                        ui.colored_label(
                            INCOMPLETE,
                            format!(
                                "{s}: incomplete — not checked: {} in {}",
                                wires.join(", "),
                                r.case.name
                            ),
                        );
                    }
                });
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
                            let missing: Vec<String> =
                                r.missing_at(&l.structure).map(|m| m.short()).collect();
                            let link = if missing.is_empty() {
                                ui.link(&l.structure)
                            } else {
                                ui.link(egui::RichText::new(&l.structure).color(INCOMPLETE))
                            };
                            if link.on_hover_text("Go to this structure").clicked() {
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
                            if missing.is_empty() {
                                ui.label(l.sections.join(", "));
                            } else {
                                ui.colored_label(
                                    INCOMPLETE,
                                    format!(
                                        "{} — incomplete: {}",
                                        l.sections.join(", "),
                                        missing.join(", ")
                                    ),
                                );
                            }
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
        ui.horizontal_wrapped(|ui| {
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
                && let Err(e) = std::fs::write(&path, ohl_loads::capacity::usage_csv(&self.usage))
            {
                log::warn!("{}: {e}", path.display());
            }
            ui.checkbox(&mut self.colour_structures, "Colour structures by usage")
                .on_hover_text(
                    "The structures coloured in the plan and 3D views by their usage in the \
                     load case chosen here (a legend in the views); those not checked because a \
                     wire at them did not solve in a neutral colour",
                );
        });
        let rows: Vec<(&str, &StructureUsage)> = match self.usage_case {
            0 => ohl_loads::capacity::governing(&self.usage),
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
        // Incomplete: in the case shown, or (governing) in any case — with
        // the cases and the wires not solved.
        let not_checked = |s: &StructureUsage| -> Option<String> {
            match self.usage_case {
                0 => {
                    let cases = ohl_loads::capacity::incomplete_cases(&self.usage, &s.structure);
                    (!cases.is_empty()).then(|| {
                        let mut wires: Vec<String> = Vec::new();
                        for (_, u) in &cases {
                            for m in &u.missing {
                                let w = m.short();
                                if !wires.contains(&w) {
                                    wires.push(w);
                                }
                            }
                        }
                        format!(
                            "not checked: {} in {}",
                            wires.join(", "),
                            cases.iter().map(|c| c.0).collect::<Vec<_>>().join(", ")
                        )
                    })
                }
                k => s.is_incomplete().then(|| {
                    format!(
                        "{} in {}",
                        s.not_checked_text(),
                        names.get(k - 1).map_or("", String::as_str)
                    )
                }),
            }
        };
        let checked = rows
            .iter()
            .filter(|r| r.1.governing().is_some() && not_checked(r.1).is_none())
            .count();
        let over = rows
            .iter()
            .filter(|r| {
                r.1.governing()
                    .is_some_and(|g| classify_usage(g.usage) == UsageClass::Over)
            })
            .count();
        let incomplete: Vec<&StructureUsage> = match self.usage_case {
            0 => {
                let mut seen = std::collections::BTreeSet::new();
                self.usage
                    .iter()
                    .flat_map(|c| c.structures.iter())
                    .filter(|s| s.is_incomplete() && seen.insert(s.structure.clone()))
                    .collect()
            }
            k => self.usage.get(k - 1).map_or_else(Vec::new, |c| {
                c.structures.iter().filter(|s| s.is_incomplete()).collect()
            }),
        };
        ui.label(
            egui::RichText::new(format!(
                "{checked} structures checked, {over} over 100 %, {} not checked (a wire not \
                 solved){}",
                incomplete.len(),
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
        let (error, ok) = (ui.visuals().error_fg_color, crate::menu::ok_color(ui));
        let pct = |c: Option<&ohl_loads::capacity::Check>| match c {
            None => egui::RichText::new("—").weak(),
            Some(c) => {
                let text = if c.usage.is_finite() {
                    format!("{:.0} %", c.usage * 100.0)
                } else {
                    "not allowed".into()
                };
                let colour = match classify_usage(c.usage) {
                    UsageClass::Over => error,
                    UsageClass::Near => egui::Color32::from_rgb(255, 170, 60),
                    UsageClass::Pass => ok,
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
                        let listed: std::collections::BTreeSet<&str> = rows
                            .iter()
                            .filter(|r| r.1.governing().is_some())
                            .map(|r| r.1.structure.as_str())
                            .chain(unchecked.iter().map(|s| s.structure.as_str()))
                            .collect();
                        // Incomplete without any check: listed too.
                        let extra: Vec<(&str, &StructureUsage)> = incomplete
                            .iter()
                            .filter(|s| !listed.contains(s.structure.as_str()))
                            .map(|s| ("", *s))
                            .collect();
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
                            .chain(extra.iter())
                            .filter(|r| {
                                filter.is_empty() || r.1.structure.to_lowercase().contains(&filter)
                            })
                            .take(5000)
                            .cloned()
                            .collect::<Vec<_>>();
                        for (case, s) in shown {
                            let not = not_checked(s);
                            // Incomplete: grey, "not checked", the usage
                            // of the loads there are only on hover.
                            let grey = |t: egui::RichText| {
                                if not.is_some() {
                                    t.color(INCOMPLETE).italics()
                                } else {
                                    t
                                }
                            };
                            if ui
                                .link(grey(egui::RichText::new(&s.structure)))
                                .on_hover_text("Go to this structure")
                                .clicked()
                            {
                                go = Some(s.structure.clone());
                            }
                            ui.label(grey(egui::RichText::new(
                                s.structure_type.as_deref().unwrap_or("—"),
                            )));
                            let g = s.governing();
                            if not.is_some() {
                                let r = ui.label(
                                    egui::RichText::new("not checked")
                                        .color(INCOMPLETE)
                                        .italics(),
                                );
                                if let Some(g) = g {
                                    r.on_hover_text(format!(
                                        "{:.0} % from the loads there are (a wire is missing)",
                                        g.usage * 100.0
                                    ));
                                }
                                for c in [&s.spans, &s.loads, &s.fe, &s.pole] {
                                    ui.label(
                                        egui::RichText::new(if c.is_some() { "?" } else { "—" })
                                            .color(INCOMPLETE),
                                    );
                                }
                            } else {
                                ui.label(pct(g));
                                ui.label(pct(s.spans.as_ref()));
                                ui.label(pct(s.loads.as_ref()));
                                ui.label(pct(s.fe.as_ref()));
                                ui.label(pct(s.pole.as_ref()));
                            }
                            match (&not, g) {
                                (Some(text), _) => ui.colored_label(INCOMPLETE, text),
                                (None, Some(g)) => ui.label(&g.governs),
                                (None, None) => {
                                    ui.label(egui::RichText::new(s.notes.join("; ")).weak())
                                }
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
