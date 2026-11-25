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
use ohl_scope::loads::{StructureChecks, TowerUsage};
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
    /// Per shared tower computed, its usage on every line's loads by the
    /// window's own structure check (none: the loads not computed; an
    /// error: why the usage was not checked).
    pub usage: Vec<(String, Result<TowerUsage, String>)>,
    /// The usage on every line's loads took, s.
    pub usage_seconds: f64,
    /// The window's **Use local conditions** was ticked (the combined
    /// loads are without them: said).
    pub local: bool,
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
        for (s, u) in &self.usage {
            match u {
                Err(e) => out.push(format!(
                    "NOT ALL CHECKED: structure {s}: its usage on every line's loads not \
                     checked ({e}) — every line's loads can use more of its capacity than this \
                     line's alone; its usage here is this line's loads only"
                )),
                Ok(u) => out.extend(
                    u.not_all_checked
                        .iter()
                        .map(|t| format!("NOT ALL CHECKED: {t}")),
                ),
            }
        }
        out.dedup();
        out
    }

    /// What is said of the shared towers' usage (none: no usage checked):
    /// NOT ON THE SAFE SIDE with the window's local conditions.
    pub fn usage_notes(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.local && self.usage.iter().any(|(_, u)| u.is_ok()) {
            out.push(
                "NOT ON THE SAFE SIDE: the usage on every line's loads takes the loads without \
                 the local conditions (the network's check takes each line's cases as written), \
                 while this line's own usage takes them — where a span's or a structure's local \
                 factor is above 1 every line's loads can be larger than computed; the larger \
                 of the two usages is kept, so a tower governed by this line's own loads is \
                 safe; untick Use local conditions to compare both without them"
                    .to_owned(),
            );
        }
        out
    }

    /// Per (line, case) of every line's loads, the shared towers' usage
    /// in it: what the window adds to its usage, as cases of their own
    /// named [`combined_case_name`].
    pub fn usage_cases(&self) -> Vec<ohl_loads::capacity::CaseUsage> {
        let mut out: Vec<ohl_loads::capacity::CaseUsage> = Vec::new();
        for (_, u) in &self.usage {
            let Ok(u) = u else { continue };
            for (line, case, s) in &u.cases {
                let name = combined_case_name(line, case);
                match out.iter_mut().find(|c| c.case == name) {
                    Some(c) => c.structures.push(s.clone()),
                    None => out.push(ohl_loads::capacity::CaseUsage {
                        case: name,
                        structures: vec![s.clone()],
                    }),
                }
            }
        }
        out
    }
}

/// The name a case of every line's loads on the shared towers has in the
/// window's usage: `line L's case C, every line` (2026-11-25).
pub fn combined_case_name(line: &str, case: &str) -> String {
    format!("line {line}'s case {case}, every line (shared towers)")
}

/// What the shared towers' usage is checked with: the window's own
/// structure check's inputs (the same check as this line's own loads).
pub struct UsageInputs {
    /// The line as the window solves it.
    pub line: ohl_line::Line,
    /// Its wires.
    pub library: ohl_cables::Library,
    /// The structure types.
    pub types: ohl_structures::StructureLibrary,
    /// The cross-section library (or why it does not read).
    pub sections: Result<ohl_sections::SectionLibrary, String>,
    /// **Use local conditions** ticked.
    pub local: bool,
}

/// Each shared tower of `r` checked on every line's loads with `i`.
fn usages(r: &SharedTowerLoads, i: &UsageInputs) -> Vec<(String, Result<TowerUsage, String>)> {
    let checks = i.sections.clone().map(|sections| StructureChecks {
        types: i.types.clone(),
        sections,
        folder: None,
    });
    r.loads
        .towers
        .iter()
        .map(|t| {
            let u = match &checks {
                Err(e) => Err(e.clone()),
                Ok(c) => ohl_scope::loads::tower_usage(
                    &i.line,
                    &t.structure,
                    c,
                    &i.library,
                    &t.combined,
                    &ohl_loads::capacity::Progress::default(),
                )
                .ok_or_else(|| "stopped".to_owned()),
            };
            (t.structure.clone(), u)
        })
        .collect()
}

impl Computed {
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
    pub fn start(
        &mut self,
        model: &Model,
        order: &[String],
        inputs: impl FnOnce() -> UsageInputs,
        ctx: &egui::Context,
    ) {
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
        let inputs = inputs();
        let local = inputs.local;
        let (tx, rx) = channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let t0 = Instant::now();
            let result = if inside.is_empty() {
                Err("no shared tower of the working set among the structures computed".into())
            } else {
                ohl_server::offline_checks::shared_tower_loads(&path, &inside)
            };
            // Their usage on every line's loads: the window's own check.
            let t1 = Instant::now();
            let usage = match &result {
                Ok(r) => usages(r, &inputs),
                Err(_) => Vec::new(),
            };
            let _ = tx.send(Computed {
                twins,
                outside,
                result,
                words,
                took: t0.elapsed(),
                usage,
                usage_seconds: t1.elapsed().as_secs_f64(),
                local,
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

    /// What the Usage view says of the shared towers (none: no shared
    /// tower): (text, whether in red).
    pub fn usage_words(&self) -> Vec<(String, bool)> {
        if self.running.is_some() {
            return vec![(
                "NOT ALL CHECKED yet: the shared towers' usage on every line's loads is being \
                 computed — until it is, their usage here is this line's loads alone"
                    .to_owned(),
                true,
            )];
        }
        let Some(c) = self.last.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let checked: Vec<&str> = c
            .usage
            .iter()
            .filter(|(_, u)| u.as_ref().is_ok_and(|u| u.governing().is_some()))
            .map(|(s, _)| s.as_str())
            .collect();
        let nothing: Vec<&str> = c
            .usage
            .iter()
            .filter(|(_, u)| u.as_ref().is_ok_and(|u| u.governing().is_none()))
            .map(|(s, _)| s.as_str())
            .collect();
        if !nothing.is_empty() {
            out.push((
                format!(
                    "The shared tower(s) {} have nothing to check against (no structure type, or \
                     a type without capacity): not checked on every line's loads, as on this \
                     line's own",
                    some(&nothing)
                ),
                false,
            ));
        }
        if !checked.is_empty() {
            out.push((
                format!(
                    "The shared tower(s) {} are checked on this line's loads and on every line's \
                     loads (the cases \"…, every line (shared towers)\"): the larger usage \
                     governs ({:.1} s)",
                    some(&checked),
                    c.usage_seconds
                ),
                false,
            ));
        }
        let mut missing: Vec<&str> = c
            .twins
            .iter()
            .map(|t| t.structure.as_str())
            .filter(|s| !checked.contains(s) && !nothing.contains(s))
            .collect();
        missing.dedup();
        if !missing.is_empty() {
            out.push((
                format!(
                    "NOT ALL CHECKED: the usage of the shared tower(s) {} is this line's loads \
                     alone — every line's loads on them can use more of their capacity; see the \
                     Shared towers view for why they were not computed",
                    some(&missing)
                ),
                true,
            ));
        }
        for n in c.usage_notes() {
            out.push((n, true));
        }
        for (_, u) in &c.usage {
            if let Ok(u) = u {
                for t in &u.not_all_checked {
                    out.push((format!("NOT ALL CHECKED: {t}"), true));
                }
            }
        }
        out.dedup();
        out
    }

    /// The answer's generation (counts the answers taken).
    pub fn generation(&self) -> u64 {
        self.generation
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
                 4 is said NOT ON THE SAFE SIDE (its structures' flexibility can add load; \
                 check that tower with that line's own loads).",
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
        // Each tower's usage on every line's loads (its governing case).
        for (s, u) in &c.usage {
            let Ok(u) = u else { continue };
            let Some((g, line, case)) = u
                .governing()
                .and_then(|(l, k, su)| su.governing().map(|g| (g, l, k)))
            else {
                ui.label(
                    egui::RichText::new(format!(
                        "structure {s}: usage on every line's loads not checked (no type or no \
                         capacity: as in the Usage view)"
                    ))
                    .small(),
                );
                continue;
            };
            let text = format!(
                "structure {s}: usage on every line's loads {:.0} % — {} ({}: {})",
                g.usage * 100.0,
                case_words(line, case),
                g.method.as_str(),
                g.governs
            );
            if g.usage > 1.0 || !g.usage.is_finite() {
                ui.colored_label(red, egui::RichText::new(text).small());
            } else {
                ui.label(egui::RichText::new(text).small());
            }
        }
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
