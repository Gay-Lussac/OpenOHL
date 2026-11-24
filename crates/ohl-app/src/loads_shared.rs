// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **The loads on shared towers with the export's other lines** in the
//! Structure loads window on an offline working set's file (2026-11-24;
//! ROADMAP O17, the follow-up of 2026-11-23's "left"). A tower of the
//! working set shared with another line of the export (a twin: one of the
//! same type within [`ohl_scope::reach::TWIN_TOLERANCE`] in plan) carries
//! that line's sections too. With every Run the window computes, beside
//! its own loads, each such tower **with every line's sections on it,
//! under each line's load cases, the worst kept** — by **the network's
//! own check** on the file
//! ([`ohl_server::offline_checks::shared_tower_loads`]: each line solved
//! with its own settings, wires and cases), so the results are those of
//! the network's check on the same file (tested). Shown in the Results'
//! **Shared towers** view.
//!
//! **Never silently**: the view says which export's neighbours were read,
//! that they may be stale, and **NOT ALL CHECKED** in red when anything is
//! missing — a shared tower outside the working set (read-only in the
//! file: the network computes it for a working set holding it), the
//! result's own reasons (a line's sections not read or not solved, its
//! cases not read), a check reaching beyond the export, neighbours left
//! out of the export. The Loads and Usage views keep this line's own
//! loads; the usage of a shared tower there is this line's alone — said
//! (NOT ALL CHECKED for it).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use eframe::egui;
use ohl_server::offline_checks::SharedTowerLoads;

use crate::check_help as help;
use crate::model::Model;
use crate::offline_neighbours::{Neighbours, Twin};

/// One run's shared towers.
#[derive(Debug, Clone)]
pub struct Computed {
    /// The towers shared with another line of the export among those the
    /// window computes.
    pub twins: Vec<Twin>,
    /// Of them, those outside the working set (not computed with their
    /// other lines here).
    pub outside: Vec<String>,
    /// The network's result on the file, or why none.
    pub result: Result<SharedTowerLoads, String>,
    /// What is said of the export: its neighbours read, that they may be
    /// stale, its gaps.
    pub words: Vec<String>,
    /// How long it took.
    pub took: Duration,
}

impl Computed {
    /// Every NOT ALL CHECKED reason.
    pub fn not_all_checked(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.outside.is_empty() {
            out.push(format!(
                "NOT ALL CHECKED: structure(s) {} shared with another line lie outside your \
                 working set (read-only in the file): their other lines' sections are not \
                 added — the network computes them for a working set holding them",
                some(&self.outside)
            ));
        }
        match &self.result {
            Err(e) => out.push(format!(
                "NOT ALL CHECKED: the loads on the shared towers with the other lines were not \
                 computed: {e}"
            )),
            Ok(r) => out.extend(
                r.not_all_checked
                    .iter()
                    .map(|t| format!("NOT ALL CHECKED: {t}")),
            ),
        }
        out.extend(
            self.words
                .iter()
                .filter(|w| w.starts_with("NOT ALL CHECKED"))
                .cloned(),
        );
        out.dedup();
        out
    }

    /// The structures computed with their other lines.
    pub fn computed(&self) -> Vec<&str> {
        match &self.result {
            Ok(r) => r
                .loads
                .towers
                .iter()
                .map(|t| t.structure.as_str())
                .collect(),
            Err(_) => Vec::new(),
        }
    }
}

/// The window's shared towers: the last run's, the one under way, what is
/// shown.
#[derive(Default)]
pub struct Shared {
    /// The last run's (none: not an offline working set's file, or no
    /// shared tower among the structures computed).
    pub last: Option<Computed>,
    /// A computation under way.
    running: Option<Receiver<Computed>>,
    /// The view shown.
    pub show: bool,
    /// The case shown: 0 the worst of every line's cases, k + 1 case k
    /// (of [`cases`]).
    pub case: usize,
    /// Counts the answers taken.
    generation: u64,
    /// The table's rows by answer and case.
    rows: crate::check_ui::CachedRows<(u64, usize)>,
}

/// The towers among `order` (ids of the line) shared with another line
/// of the export `n`, with those outside the file's working set.
pub fn twins_of(model: &Model, n: &Neighbours, order: &[String]) -> (Vec<Twin>, Vec<String>) {
    let twins = n.twins(&model.line, order);
    let part: Option<BTreeSet<String>> = model.session().and_then(|s| {
        s.part().map(|p| {
            p.iter()
                .filter(|(k, _)| *k == ohl_session::Kind::Structure)
                .map(|(_, id)| id.clone())
                .collect()
        })
    });
    let mut outside: Vec<String> = twins
        .iter()
        .filter(|t| part.as_ref().is_some_and(|p| !p.contains(&t.structure)))
        .map(|t| t.structure.clone())
        .collect();
    outside.dedup();
    (twins, outside)
}

impl Shared {
    /// Starts the shared towers of `order` (the structures the run
    /// computes) on an offline working set's file, in the background;
    /// nothing otherwise.
    pub fn start(&mut self, model: &Model, order: &[String], ctx: &egui::Context) {
        self.running = None;
        self.last = None;
        let Some(n) = model.offline_neighbours() else {
            return;
        };
        let Ok(n) = n.as_ref() else {
            return;
        };
        let (twins, outside) = twins_of(model, n, order);
        if twins.is_empty() {
            return;
        }
        let mut words = vec![n.included_words("the other lines' sections on shared towers")];
        words.push(n.stale_words());
        words.extend(n.gaps());
        let mut inside: Vec<String> = twins
            .iter()
            .map(|t| t.structure.clone())
            .filter(|s| !outside.contains(s))
            .collect();
        inside.dedup();
        let path = model.sources.file.clone();
        let (tx, rx) = channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let t0 = Instant::now();
            let result = if inside.is_empty() {
                Err("no shared tower of the working set among the structures computed".into())
            } else {
                ohl_server::offline_checks::shared_tower_loads(&path, &inside)
            };
            let _ = tx.send(Computed {
                twins,
                outside,
                result,
                words,
                took: t0.elapsed(),
            });
            c.request_repaint();
        });
        self.running = Some(rx);
    }

    /// Takes a finished computation.
    pub fn poll(&mut self) {
        let Some(rx) = &self.running else {
            return;
        };
        match rx.try_recv() {
            Ok(c) => {
                self.last = Some(c);
                self.running = None;
                self.generation += 1;
                self.case = 0;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => self.running = None,
        }
    }

    /// Whether it is computing.
    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    /// Whether the window has the view (a run with shared towers, or one
    /// computing them).
    pub fn has_view(&self) -> bool {
        self.running.is_some() || self.last.is_some()
    }

    /// The red line the Usage view says for the shared towers (none: no
    /// shared tower).
    pub fn usage_words(&self) -> Option<String> {
        let c = self.last.as_ref()?;
        let mut ids: Vec<&str> = c.twins.iter().map(|t| t.structure.as_str()).collect();
        ids.dedup();
        Some(format!(
            "NOT ALL CHECKED: the usage of the shared tower(s) {} is this line's loads alone — \
             the other lines' sections on them are in the Shared towers view, not in this usage",
            some(&ids)
        ))
    }

    /// The view; returns a structure to go to.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<String> {
        let mut go = None;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Shared towers with the export's other lines").strong());
            help::question_both(
                ui,
                "Each tower of your working set shared with another line, with every line's \
                 sections on it, under each line's load cases, the worst kept.",
                "On an offline working set's file, a tower of your working set shared with \
                 another line of the export (one of the same type within 0.5 m) also carries \
                 that line's sections. With every Run the window computes each such tower with \
                 every line's sections on it, under each line's load cases (its own library's, \
                 else the shipped defaults), each line solved with its own settings and wires \
                 — the network's own check on the file, so the same results as the network's \
                 check (Versions › Offline copy › Check the offline working set… › Loads on \
                 shared towers). At a point holding several lines' sections their loads are \
                 added (each line's strings there counted again: more load, conservative). \
                 The worst per point and direction is the largest factored load by size, with \
                 the line and case giving it. Solved rigid (level 2): a line set to level 3 or \
                 4 is said NOT ON THE SAFE SIDE.",
            );
        });
        if self.running.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Computing the shared towers with the other lines' sections…");
            });
            return None;
        }
        let Some(c) = &self.last else {
            ui.label("No shared tower among the structures computed.");
            return None;
        };
        let red = ui.visuals().error_fg_color;
        // The lines read (that they may be stale is said above the views,
        // the export's gaps in the red list below).
        for w in &c.words {
            if !(w.starts_with("NOT ALL CHECKED")
                || w.contains("STALE")
                || w.starts_with("They may be stale"))
            {
                ui.label(egui::RichText::new(w).small());
            }
        }
        let not_all = c.not_all_checked();
        let computed = c.computed();
        let verdict = if not_all.is_empty() {
            format!(
                "✔ {} shared tower(s) computed with every line's sections on them: {}",
                computed.len(),
                some(&computed)
            )
        } else {
            format!(
                "NOT ALL CHECKED — {} shared tower(s) computed with their other lines{}",
                computed.len(),
                if computed.is_empty() {
                    String::new()
                } else {
                    format!(": {}", some(&computed))
                }
            )
        };
        if not_all.is_empty() {
            ui.label(egui::RichText::new(verdict).strong());
        } else {
            ui.colored_label(red, egui::RichText::new(verdict).strong());
        }
        let notes: Vec<String> = match &c.result {
            Ok(r) => r
                .loads
                .towers
                .iter()
                .flat_map(|t| {
                    let lines: BTreeSet<&str> = t.twins.iter().map(|n| n.model.as_str()).collect();
                    std::iter::once(format!(
                        "structure {}: shared with line(s) {} ({} case(s) of every line)",
                        t.structure,
                        lines.into_iter().collect::<Vec<_>>().join(", "),
                        t.cases.len()
                    ))
                    .chain(
                        t.notes
                            .iter()
                            .map(move |n| format!("structure {}: {n}", t.structure)),
                    )
                })
                .collect(),
            Err(_) => Vec::new(),
        };
        if !not_all.is_empty() || !notes.is_empty() {
            help::list(
                ui,
                "loads-shared-notes",
                110.0,
                egui::ScrollArea::vertical()
                    .id_salt("loads-shared-notes")
                    .auto_shrink([false, true]),
                |ui| {
                    for t in &not_all {
                        ui.colored_label(red, egui::RichText::new(t).small());
                    }
                    for n in &notes {
                        if n.contains("NOT ON THE SAFE SIDE") {
                            ui.colored_label(red, egui::RichText::new(n).small());
                        } else {
                            ui.label(egui::RichText::new(format!("• {n}")).small());
                        }
                    }
                },
            );
        }
        let Ok(r) = &c.result else {
            return None;
        };
        let cases = cases(r);
        ui.horizontal(|ui| {
            ui.label("Load case");
            let shown = match self.case {
                0 => "the worst of every line's cases".to_owned(),
                k => cases
                    .get(k - 1)
                    .map_or(String::new(), |(l, n)| case_words(l, n)),
            };
            egui::ComboBox::from_id_salt("structure-loads-shared-case")
                .selected_text(shown)
                .width(260.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.case, 0, "the worst of every line's cases");
                    for (k, (l, n)) in cases.iter().enumerate() {
                        ui.selectable_value(&mut self.case, k + 1, case_words(l, n));
                    }
                });
            ui.label(
                egui::RichText::new(format!(
                    "factored · {:.1} s beside the run ({:.1} s the network's code on the file)",
                    c.took.as_secs_f64(),
                    r.seconds
                ))
                .small()
                .weak(),
            );
        });
        use crate::check_ui::Col;
        let case = self.case.min(cases.len());
        let rows = self
            .rows
            .get_or_build((self.generation, case), || rows(r, &cases, case));
        let last = if case == 0 {
            "Governing (transverse / longitudinal / vertical)"
        } else {
            "Sections"
        };
        if let Some(k) = crate::check_ui::ResultTable::new(
            "structure-loads-shared-rows",
            vec![
                Col::text("Structure"),
                Col::text("Point"),
                Col::text("Lines"),
                Col::num("Transverse [kN]"),
                Col::num("Longitudinal [kN]"),
                Col::num("Vertical [kN]"),
                Col::text(last),
            ],
        )
        .rows_kept(rows)
        .no_filter()
        .group_by(&[0])
        .csv("structure-loads-shared-towers.csv")
        .ui(ui)
        {
            go = row_structures(r, &cases, case).get(k).cloned();
        }
        go
    }
}

/// Every line's cases on the towers, in their order: (line, case).
pub fn cases(r: &SharedTowerLoads) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for t in &r.loads.towers {
        for c in &t.cases {
            let k = (c.line.clone(), c.case.clone());
            if !out.contains(&k) {
                out.push(k);
            }
        }
    }
    out
}

/// A list in words, the first few named: "A, B, C, D, E, F and 26 more".
pub fn some<S: AsRef<str>>(list: &[S]) -> String {
    const NAMED: usize = 6;
    let named: Vec<&str> = list.iter().take(NAMED).map(AsRef::as_ref).collect();
    if list.len() > NAMED {
        format!("{} and {} more", named.join(", "), list.len() - NAMED)
    } else {
        named.join(", ")
    }
}

fn case_words(line: &str, case: &str) -> String {
    format!("line {line}'s case {case}")
}

fn point_words(p: &str) -> &str {
    if p.is_empty() { "(body)" } else { p }
}

/// The structure of each row of [`rows`].
fn row_structures(r: &SharedTowerLoads, cases: &[(String, String)], case: usize) -> Vec<String> {
    let mut out = Vec::new();
    for t in &r.loads.towers {
        let n = if case == 0 {
            t.worst.len()
        } else {
            let (l, c) = &cases[case - 1];
            t.cases
                .iter()
                .filter(|x| &x.line == l && &x.case == c)
                .map(|x| x.points.len())
                .sum()
        };
        out.extend(std::iter::repeat_n(t.structure.clone(), n));
    }
    out
}

/// The table's rows: per tower and point the worst (`case` 0) or case
/// `case - 1`'s loads, factored.
pub fn rows(
    r: &SharedTowerLoads,
    cases: &[(String, String)],
    case: usize,
) -> Vec<crate::check_ui::Row> {
    use crate::check_ui::{Cell, Row};
    let mut out = Vec::new();
    for t in &r.loads.towers {
        if case == 0 {
            // The lines holding sections at each point, over every case.
            let mut lines: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
            for c in &t.cases {
                for p in &c.points {
                    lines
                        .entry(p.point.as_str())
                        .or_default()
                        .extend(p.lines.iter().map(String::as_str));
                }
            }
            for w in &t.worst {
                let gov: Vec<String> = w.governing.iter().map(|(l, c)| case_words(l, c)).collect();
                out.push(Row::new(
                    vec![
                        Cell::text(&t.structure),
                        Cell::text(point_words(&w.point)),
                        Cell::text(
                            lines
                                .get(w.point.as_str())
                                .map(|s| s.iter().copied().collect::<Vec<_>>().join(", "))
                                .unwrap_or_default(),
                        ),
                        Cell::num(Some(w.factored[0] / 1000.0), 2),
                        Cell::num(Some(w.factored[1] / 1000.0), 2),
                        Cell::num(Some(w.factored[2] / 1000.0), 2),
                        Cell::text(if gov.iter().all(|g| *g == gov[0]) {
                            format!("{} (all three)", gov[0])
                        } else {
                            gov.join(" / ")
                        }),
                    ],
                    false,
                ));
            }
        } else {
            let (l, n) = &cases[case - 1];
            for c in t.cases.iter().filter(|x| &x.line == l && &x.case == n) {
                for p in &c.points {
                    out.push(Row::new(
                        vec![
                            Cell::text(&t.structure),
                            Cell::text(point_words(&p.point)),
                            Cell::text(p.lines.join(", ")),
                            Cell::num(Some(p.factored[0] / 1000.0), 2),
                            Cell::num(Some(p.factored[1] / 1000.0), 2),
                            Cell::num(Some(p.factored[2] / 1000.0), 2),
                            Cell::text(p.sections.join(", ")),
                        ],
                        false,
                    ));
                }
            }
        }
    }
    out
}
