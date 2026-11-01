// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Electrical › **Lightning shielding…** (owner, 2026-10-31;
//! built, in active development, not approved yet): whether the shielding
//! wires keep lightning off the phases, by a fixed shielding angle or the
//! rolling sphere ([`ohl_electrical::lightning`]), by the rules of a rule
//! file kept in the project (`<model>.lightning-rules.ohlr`,
//! `[[lightning_rule]]` tables; a design-code plugin's preset brings its
//! lightning rules). Run solves each weather case of the rules in the
//! background and checks every span in cross-sections square to it; the
//! results list per phase and span the length exposed, where it is worst
//! and the margin. A row clicked selects its span (drawn in the rule's
//! case) and draws its worst cross-section below the table: the ground,
//! the shielding wires, the phases, the covered zone's boundary and the
//! sphere's centres. The 3D view's layer *Lightning shielding* draws the
//! exposed parts of the phases in red and the covered zone's boundary,
//! translucent, while the window is open.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use eframe::egui;
use ohl_clearance::lightning::{LightningRule, Method, StrikingDistance};
use ohl_clearance::rules::{PresetCheck, RuleSet};
use ohl_electrical::lightning::{self as engine, Cut, Report as EngineReport, Row};

use crate::check_help as help;
use crate::check_ui;
use crate::job::Job;
use crate::model::Model;
use crate::selection::Selection;

/// What its results are computed from ([`crate::revision`]).
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::ALL;

/// The id of the window's body, tabs and footer.
const WINDOW: &str = "lightning-window";

/// A check's answer: the report and the rules it was made with.
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// The rows, cross-sections and notes.
    pub report: EngineReport,
    /// The rules checked.
    pub rules: Vec<LightningRule>,
}

impl Report {
    /// The rows exposed.
    pub fn exposed(&self) -> impl Iterator<Item = &Row> {
        self.report.rows.iter().filter(|r| !r.passes())
    }

    /// The cross-section of row `r` at its worst place (of its span).
    pub fn worst_cut(&self, r: &Row) -> Option<&Cut> {
        self.report
            .cuts
            .iter()
            .filter(|c| c.from == r.from && c.to == r.to)
            .min_by(|a, b| {
                (a.along - r.worst_along)
                    .abs()
                    .total_cmp(&(b.along - r.worst_along).abs())
            })
    }
}

/// What the window asks of the app.
#[derive(Default)]
pub struct Out {
    /// Go to a span.
    pub go: Option<Selection>,
    /// The weather case of the row clicked: the span drawn in it.
    pub case: Option<String>,
    /// A design-code plugin's preset loaded (its plugin, the preset): its
    /// weather cases missing from the model to add.
    pub plugin_preset: Option<(String, ohl_io::codes::RulePresetChecked)>,
    /// What the 3D view draws changed (a new report, the window closed).
    pub overlay: Option<Overlay>,
}

/// What the views draw of a report.
#[derive(Debug, Clone, Default)]
pub struct Overlay {
    /// The report (none: nothing drawn).
    pub report: Option<Arc<Report>>,
    /// Out of date: dimmed.
    pub stale: bool,
}

/// The Lightning shielding window.
#[derive(Default)]
pub struct LightningWindow {
    /// Shown.
    pub open: bool,
    /// The structure and section overrides of its cases.
    pub overrides: crate::case_groups::Overrides,
    /// The project's rule file read.
    loaded: bool,
    /// The rule file kept.
    pub file: Option<PathBuf>,
    /// The rules.
    pub rules: Vec<LightningRule>,
    /// What was done last, in words (an error: true).
    pub status: Option<(String, bool)>,
    /// The check.
    pub(crate) job: Job<Arc<Report>>,
    /// The spans done, of how many (while it runs).
    progress: Option<(Arc<AtomicUsize>, usize, Arc<AtomicBool>)>,
    /// The row whose cross-section is drawn (index in the rows).
    picked: Option<usize>,
    /// What the views draw: (shown, generation, stale).
    drawn: Option<(bool, u64, bool)>,
    /// The line's spans counted, by the version of the line it was
    /// counted on (counted once per version, not every frame).
    spans: Option<(u64, usize)>,
}

/// The rule file of the window in a project: in its clearance rules (its
/// working copy), else next to the model.
pub fn project_file(model: &Model) -> PathBuf {
    let model_file = &model.sources.file;
    let name = model_file
        .file_name()
        .map_or(String::from("line"), |n| n.to_string_lossy().into_owned());
    let stem = name.split('.').next().unwrap_or("line").to_owned();
    let dir = model
        .library_dir
        .as_ref()
        .filter(|_| model.editing.is_ok())
        .map(|w| w.join("rules"))
        .filter(|d| d.is_dir())
        .unwrap_or_else(|| ohl_core::files::dir_of(model_file));
    dir.join(format!("{stem}.lightning-rules.ohlr"))
}

impl LightningWindow {
    /// Opens the window; the first time, reads the project's rule file.
    pub fn show(&mut self, model: &Model) {
        self.open = true;
        if self.loaded {
            return;
        }
        self.loaded = true;
        let file = self.file.get_or_insert_with(|| project_file(model)).clone();
        if file.exists() {
            match RuleSet::load(&file) {
                Ok(set) => {
                    self.rules = set.lightning_rules;
                    if let Some(c) = set.check {
                        self.overrides = crate::case_groups::Overrides {
                            structures: c.structure_override,
                            sections: c.section_override,
                        };
                    }
                }
                Err(e) => self.status = Some((e, true)),
            }
        }
    }

    /// Why the check cannot run (none: it can).
    pub fn why_not_run(&self, model: &Model) -> Option<String> {
        if self.rules.is_empty() {
            return Some("no rule: add one, or load a preset (3 Rules)".into());
        }
        let why: Vec<String> = self
            .rules
            .iter()
            .filter_map(LightningRule::problem)
            .collect();
        if !why.is_empty() {
            return Some(why.join("; "));
        }
        if !model
            .cases
            .iter()
            .any(|c| self.rules.iter().any(|r| r.applies(&c.case.name)))
        {
            return Some("none of the rules' weather cases is in the model (3 Rules)".into());
        }
        let names: Vec<&str> = model.cases.iter().map(|c| c.case.name.as_str()).collect();
        if let Some(r) = self
            .rules
            .iter()
            .find(|r| !r.shield_cases.is_empty() && r.shield_cases_for("", &names).is_empty())
        {
            return Some(format!(
                "\"{}\": none of its shielding wires' cases is in the model (3 Rules)",
                r.name
            ));
        }
        None
    }

    /// The rules as a rule file.
    pub fn rule_set(&self) -> RuleSet {
        RuleSet {
            name: "Lightning shielding".into(),
            check: check_ui::check_of(&check_ui::Scope::default(), self.overrides),
            checks: vec![PresetCheck::Lightning],
            lightning_rules: self.rules.clone(),
            ..RuleSet::default()
        }
    }

    /// Writes the rule file; the project's working copy goes into the
    /// project.
    pub fn save(&mut self, model: &mut Model) {
        let file = self.file.get_or_insert_with(|| project_file(model)).clone();
        let in_project = model
            .library_dir
            .as_ref()
            .is_some_and(|w| file.starts_with(w));
        self.status = Some(match self.rule_set().save(&file) {
            Ok(()) if in_project => match model.store_libraries() {
                Ok(_) => ("kept in the project".to_owned(), false),
                Err(e) => (format!("keeping the rules in the project: {e}"), true),
            },
            Ok(()) => (format!("kept in {}", file.display()), false),
            Err(e) => (e, true),
        });
    }

    /// (rules × cases to run, spans of the line).
    fn work(&mut self, model: &Model) -> (usize, usize) {
        let names: Vec<&str> = model.cases.iter().map(|c| c.case.name.as_str()).collect();
        let pairs = engine::runs(&self.rules, &names, names.len()).len();
        let spans = match self.spans {
            Some((v, n)) if v == model.line_version => n,
            _ => {
                let n = ohl_electrical::cross_section::line_spans(&model.line).len();
                self.spans = Some((model.line_version, n));
                n
            }
        };
        (pairs, spans)
    }

    /// Starts a check: each weather case of the rules solved in the
    /// background, every span checked by each rule of the case.
    pub fn start(&mut self, model: &Model, ctx: &egui::Context) {
        if let Some(why) = self.why_not_run(model) {
            self.job.result = Some(Err(format!("not run: {why}")));
            return;
        }
        let cases: Vec<ohl_weather::WeatherCase> =
            model.cases.iter().map(|c| c.case.clone()).collect();
        let line = model.line.clone();
        let library = model.library.clone();
        let rules = self.rules.clone();
        let ground = model.ground_for_checks();
        let (pairs, spans) = self.work(model);
        let done = Arc::new(AtomicUsize::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        self.progress = Some((done.clone(), (pairs * spans).max(1), cancel.clone()));
        self.picked = None;
        self.job.start_at(ctx, model.revision(), move || {
            let height = |x: f64, y: f64| ground.as_ref().and_then(|t| t.height(x, y));
            let g: Option<&(dyn Fn(f64, f64) -> Option<f64> + Sync)> =
                ground.as_ref().map(|_| &height as _);
            let solve =
                |line: &ohl_line::Line, case: &ohl_weather::WeatherCase| -> engine::Solved {
                    ohl_line::solve::solve_line_memo_default(line, &library, case)
                        .into_iter()
                        .map(|r| r.map_err(|e| e.to_string()))
                        .collect()
                };
            let mut report = engine::run(
                &line,
                &library,
                &cases,
                cases.len(),
                &rules,
                g,
                &solve,
                &done,
                &cancel,
            )?;
            if ground.is_none() {
                report.notes.insert(
                    0,
                    "no terrain for the checks: the ground straight between the structures' feet \
                     and level across (Line › Terrain)"
                        .into(),
                );
            }
            Ok(Arc::new(Report { report, rules }))
        });
    }

    /// What the last check was computed from that changed since, in
    /// words (`None`: nothing, or no report).
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        matches!(self.job.result, Some(Ok(_)))
            .then(|| self.job.stamp.out_of_date(model, USES))
            .flatten()
    }

    /// The overlay to draw when what the views should show changed.
    fn overlay_change(&mut self, model: &Model) -> Option<Overlay> {
        let report = match &self.job.result {
            Some(Ok(r)) if self.open => Some(r.clone()),
            _ => None,
        };
        let stale = self.out_of_date(model).is_some();
        let key = (report.is_some(), self.job.generation, stale);
        if self.drawn == Some(key) {
            return None;
        }
        let first = self.drawn.is_none();
        self.drawn = Some(key);
        match report {
            Some(r) => Some(Overlay {
                report: Some(r),
                stale,
            }),
            None if first => None,
            None => Some(Overlay::default()),
        }
    }

    /// The window.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        model: &mut Model,
        presets: &[crate::rule_editor::PluginPresets],
    ) -> Out {
        let mut out = Out::default();
        self.job.poll();
        if !self.job.running() {
            self.progress = None;
        }
        if !self.open {
            out.overlay = self.overlay_change(model);
            return out;
        }
        let mut open = true;
        let mut run = false;
        let mut changed = false;
        let stale = self.out_of_date(model);
        help::window(
            ctx,
            crate::dockable::Panel::Lightning,
            "Lightning shielding",
            [860.0, 760.0],
            &mut open,
            |ui| {
                let why = self.why_not_run(model);
                let findings = match &self.job.result {
                    Some(Ok(r)) => Some(r.exposed().count()),
                    _ => None,
                };
                let tab = check_ui::tabs(
                    ui,
                    WINDOW,
                    findings,
                    self.job.generation,
                    why.is_some() && !self.job.running(),
                );
                match tab {
                    check_ui::Tab::Setup => help::body(ui, WINDOW, |ui| {
                        check_ui::section(ui, 1, "What it checks", None);
                        help::explain(
                            ui,
                            "Whether the shielding wires keep lightning off the phases: by a \
                             fixed shielding angle, or by the rolling sphere of the \
                             electrogeometric model, along every span.",
                            "Run solves the line in each weather case of the rules and cuts every \
                             span square to it every step: the wires where they cross, the ground \
                             across. Shielding angle: a phase is covered when the line from a \
                             shielding wire above it leans from the vertical by at most the \
                             rule's angle (inner towards another shielding wire). Rolling sphere: \
                             a sphere of the striking distance rolled over the shielding wires and \
                             the ground (β times the distance from it) must not touch the phase. \
                             The results: per phase and span, the length exposed, where it is \
                             worst, the margin. No value is built in: the rules' are yours or a \
                             design-code plugin's. Built, in active development, not approved \
                             yet.",
                        );
                        check_ui::how_computed(
                            ui,
                            WINDOW,
                            "In each cross-section, square to the span at the middle of each step: \
                             the shielding angle each phase needs from the shielding wire covering \
                             it best (to the far side of its bundle); or whether a sphere resting \
                             on the shielding wires and the ground can reach its surface — exactly, \
                             at the nearest point of the free region. The margin: the angle \
                             needed, or r_max, the largest striking distance still reaching the \
                             phase (and its stroke current).",
                            |ui| {
                                ui.horizontal_top(|ui| {
                                    help::shielding_angle(ui);
                                    help::rolling_sphere(ui);
                                });
                            },
                        );
                        ui.separator();
                        check_ui::section(ui, 2, "Scope", None);
                        let (pairs, spans) = self.work(model);
                        ui.label(
                            egui::RichText::new(format!(
                                "Every span of the line ({spans}); each rule gives its phases, its \
                                 shielding wires and its weather cases ({pairs} rule × case pair{} \
                                 to run, 3 Rules).",
                                if pairs == 1 { "" } else { "s" }
                            ))
                            .small()
                            .weak(),
                        );
                        if model.ground_for_checks().is_none() {
                            ui.colored_label(
                                ui.visuals().warn_fg_color,
                                egui::RichText::new(
                                    "No terrain for the checks: the ground is taken straight \
                                     between the structures' feet and level across (Line › \
                                     Terrain). The sphere's verdict depends on the ground.",
                                )
                                .small(),
                            );
                        }
                        ui.separator();
                        check_ui::section(ui, 3, "Rules", None);
                        changed |= self.rules_ui(ui, model, presets, &mut out);
                        if let Some((text, error)) = &self.status {
                            if *error {
                                crate::menu::error_label(ui, text.as_str());
                            } else {
                                ui.label(egui::RichText::new(text).small().weak());
                            }
                        }
                    }),
                    check_ui::Tab::Results => check_ui::results_area(ui, WINDOW, |ui| {
                        self.results_ui(ui, &stale, &mut run, &mut out);
                    }),
                }
                let (pairs, spans) = self.work(model);
                help::footer(ui, WINDOW, |ui| {
                    if let Some((done, total, cancel)) = &self.progress {
                        let d = done.load(Ordering::Relaxed);
                        let elapsed = self.job.elapsed().unwrap_or_default();
                        let f = check_ui::RunFooter {
                            running: Some(check_ui::Progress::of(elapsed, d, *total)),
                            ..Default::default()
                        };
                        if check_ui::run_footer(ui, f, |_| {}) == check_ui::Action::Stop {
                            cancel.store(true, Ordering::Relaxed);
                            self.job.stop();
                        }
                        return;
                    }
                    let overrides = &mut self.overrides;
                    run |= check_ui::job_footer(
                        ui,
                        &mut self.job,
                        why.clone(),
                        "Solve each weather case of the rules and check every span's phases \
                         against their shielding",
                        None,
                        Some(check_ui::Plan::new(
                            format!(
                                "{} × {}",
                                check_ui::count(spans, "span"),
                                check_ui::count(pairs, "rule × case pair")
                            ),
                            spans * pairs,
                        )),
                        |ui| {
                            changed |= overrides.ui(ui);
                        },
                    );
                });
            },
        );
        self.open = open;
        if changed {
            self.save(model);
        }
        if run {
            self.start(model, ctx);
        }
        out.overlay = self.overlay_change(model);
        out
    }

    /// The rules in the shared rule table; whether one changed.
    fn rules_ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        presets: &[crate::rule_editor::PluginPresets],
        out: &mut Out,
    ) -> bool {
        let part = crate::preset_cases::Part::Check(PresetCheck::Lightning);
        let cases: Vec<String> = model.cases.iter().map(|c| c.case.name.clone()).collect();
        let groups = crate::case_groups::groups(model);
        let mut types: Vec<String> = model
            .line
            .sections
            .iter()
            .map(|s| s.cable.clone())
            .collect();
        types.sort();
        types.dedup();
        let mut systems: Vec<String> = model.line.systems.iter().map(|s| s.id.clone()).collect();
        systems.sort();
        let circuits: Vec<String> = model.line.circuits.iter().map(|c| c.name.clone()).collect();
        let t = crate::rule_table::RuleTable::new("lightning-rules", &mut self.rules, &[])
            .heads("Method", "Phases")
            .add(
                "A shielding-angle rule",
                "every phase, the earthed wires shielding; set its angles",
                |n| LightningRule::blank(&format!("angle {}", n + 1)),
            )
            .add(
                "A rolling-sphere rule",
                "every phase, the earthed wires shielding; set its striking distance",
                |n| LightningRule {
                    method: Method::Sphere {
                        distance: StrikingDistance::Typed(0.0),
                        ground_factor: 1.0,
                    },
                    ..LightningRule::blank(&format!("sphere {}", n + 1))
                },
            )
            .presets(model, presets, part, |set| {
                set.lightning_rules.iter().map(rule_text).collect()
            })
            .ui(ui, |ui, _, r| {
                rule_fields(ui, r, &cases, &groups, &types, &systems, &circuits)
            });
        let mut changed = t.changed;
        if let Some(pick) = t.preset {
            match pick.taken(model, part) {
                Ok((p, lines)) => {
                    let done = crate::rule_table::merge(
                        &mut self.rules,
                        p.rules.lightning_rules.clone(),
                        pick.how,
                    );
                    self.status = Some((pick.said(&done, &lines), false));
                    out.plugin_preset = Some((pick.group, p));
                    changed = true;
                }
                Err(e) => self.status = Some((e, true)),
            }
        }
        if t.import
            && let Some(p) = crate::rule_table::import_file()
        {
            match RuleSet::load(&p) {
                Ok(set) => {
                    self.rules = set.lightning_rules;
                    changed = true;
                }
                Err(e) => self.status = Some((e, true)),
            }
        }
        if t.export
            && let Some(p) = crate::rule_table::export_file("lightning-rules.ohlr")
        {
            self.status = Some(match self.rule_set().save(&p) {
                Ok(()) => (format!("exported to {}", p.display()), false),
                Err(e) => (e, true),
            });
        }
        changed
    }

    /// The results: the verdict, the notes, every phase and span (exposed
    /// first), the cross-section of the row picked.
    fn results_ui(
        &mut self,
        ui: &mut egui::Ui,
        stale: &Option<String>,
        run: &mut bool,
        out: &mut Out,
    ) {
        use check_ui::{Cell, Col};
        let report = match &self.job.result {
            None => {
                ui.label("Run to check every span.");
                return;
            }
            Some(Err(e)) => {
                crate::menu::error_label(ui, e.as_str());
                return;
            }
            Some(Ok(r)) => r.clone(),
        };
        if let Some(what) = stale {
            *run |= crate::revision::banner(ui, what, !self.job.running());
        }
        let exposed: Vec<&Row> = report.exposed().collect();
        let total = exposed.iter().fold(0.0, |a, r| a + r.exposed);
        let worst = report.report.rows.first();
        let text = match worst {
            Some(r) if !exposed.is_empty() => format!(
                "✖ {} phase × span{} exposed, {total:.0} m in all — the longest {}: {:.0} m of \
                 {:.0} m ({}, rule {}, case {})",
                exposed.len(),
                if exposed.len() == 1 { "" } else { "s" },
                Cell::span(&r.section, r.span).text,
                r.exposed,
                r.length,
                r.phase,
                report.rules[r.rule].name,
                r.case
            ),
            Some(_) => format!(
                "✔ Every phase covered ({} phases × spans × cases × rules)",
                report.report.rows.len()
            ),
            None => "✖ Nothing checked: no span holds a phase of the rules".to_owned(),
        };
        if check_ui::verdict(
            ui,
            !exposed.is_empty() || worst.is_none(),
            &text,
            !exposed.is_empty(),
        ) && let Some(r) = worst
        {
            self.picked = Some(0);
            out.go = Some(Selection::Span {
                section: r.section.clone(),
                span: r.span,
            });
            out.case = Some(r.case.clone());
        }
        check_ui::notes_fold(ui, WINDOW, &report.report.notes);
        let rows: Vec<check_ui::Row> = report
            .report
            .rows
            .iter()
            .map(|r| {
                let rule = &report.rules[r.rule];
                let bad = !r.passes();
                check_ui::Row::new(
                    vec![
                        Cell::span(&r.section, r.span),
                        Cell::text(&r.phase),
                        Cell::text(&rule.name),
                        Cell::text(&r.case),
                        Cell::num(Some(r.exposed), 1).bad(bad),
                        Cell::num(Some(r.length), 1),
                        Cell::num(Some(r.worst_along), 1),
                        margin_cell(rule, r),
                        Cell::text(if bad { "exposed" } else { "covered" }).bad(bad),
                    ],
                    bad,
                )
            })
            .collect();
        if let Some(k) = check_ui::ResultTable::new(
            "lightning-rows",
            vec![
                Col::text("Span"),
                Col::text("Phase"),
                Col::text("Rule"),
                Col::text("Case"),
                Col::num("Exposed [m]"),
                Col::num("Span [m]"),
                Col::num("Worst at [m]").tip("Along the span from its first structure"),
                Col::num("Margin").tip(
                    "Shielding angle: the largest angle the phase needs, °. Rolling sphere: \
                     r_max, the largest striking distance still reaching it at its worst place, \
                     m (and its stroke current)",
                ),
                Col::text("Verdict"),
            ],
        )
        .rows(rows)
        .group_by(&[2, 3, 0])
        .csv("lightning-table.csv")
        .full_csv(
            "lightning.csv",
            "Every row with every field, as ohl lightning writes it",
            || engine::csv(&report.report.rows, &report.rules),
        )
        .below(400.0)
        .ui(ui)
        {
            self.picked = Some(k);
            let r = &report.report.rows[k];
            out.go = Some(Selection::Span {
                section: r.section.clone(),
                span: r.span,
            });
            out.case = Some(r.case.clone());
        }
        // The cross-section of the row picked, at its worst place.
        if let Some(r) = self.picked.and_then(|k| report.report.rows.get(k)) {
            let rule = &report.rules[r.rule];
            match report.worst_cut(r) {
                Some(cut) => {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} {}, {:.1} m along the span from {} — {}",
                            Cell::span(&r.section, r.span).text,
                            r.phase,
                            cut.along,
                            r.from,
                            rule.method.describe()
                        ))
                        .small(),
                    );
                    paint_cut(ui, cut, rule.method, &r.section);
                    if report.rules.len() > 1 {
                        ui.label(
                            egui::RichText::new(
                                "The cross-sections drawn are the last rule's of each span.",
                            )
                            .small()
                            .weak(),
                        );
                    }
                }
                None => {
                    ui.label(
                        egui::RichText::new("No cross-section drawn here.")
                            .small()
                            .weak(),
                    );
                }
            }
        } else {
            ui.label(
                egui::RichText::new("Click a row to draw its cross-section at its worst place.")
                    .small()
                    .weak(),
            );
        }
    }
}

/// A row's margin cell.
fn margin_cell(rule: &LightningRule, r: &Row) -> check_ui::Cell {
    use check_ui::Cell;
    match (rule.method, r.margin) {
        (Method::Angle { .. }, Some(m)) => Cell::sorted(format!("{m:.1}°"), m),
        (Method::Sphere { .. }, Some(m)) if m >= engine::R_MAX => {
            Cell::sorted(format!("> {:.0} m", engine::R_MAX), m)
        }
        (Method::Sphere { .. }, Some(m)) => Cell::sorted(
            match r.current {
                Some(i) => format!("{m:.1} m ({i:.1} kA)"),
                None => format!("{m:.1} m"),
            },
            m,
        ),
        (_, None) => Cell::text("-"),
    }
}

/// A rule's fields; whether one changed.
fn rule_fields(
    ui: &mut egui::Ui,
    r: &mut LightningRule,
    cases: &[String],
    groups: &[ohl_weather::CaseGroup],
    types: &[String],
    systems: &[String],
    circuits: &[String],
) -> bool {
    let mut changed = false;
    let drag =
        |ui: &mut egui::Ui, v: &mut f64, range: std::ops::RangeInclusive<f64>, suffix: &str| {
            let mut x = *v;
            let resp = crate::style::field(
                ui,
                egui::DragValue::new(&mut x)
                    .range(range)
                    .speed(0.1)
                    .max_decimals(3)
                    .suffix(suffix),
            );
            if resp.changed() {
                *v = x;
            }
            resp.drag_stopped() || resp.lost_focus()
        };
    egui::Grid::new("lightning-rule")
        .num_columns(2)
        .spacing(help::GRID_SPACING)
        .show(ui, |ui| {
            ui.label("Weather cases");
            crate::menu::options_menu(ui, &crate::rule_table::cases_words(&r.cases), |ui| {
                changed |= crate::case_groups::pick_names(ui, groups, &mut r.cases);
                changed |= ticks(ui, cases, &mut r.cases);
            });
            ui.end_row();
            // The shielding wires in their own cases (owner, 2026-11-01).
            ui.label("Shielding wires' cases");
            crate::menu::options_menu(ui, &shield_cases_words(r), |ui| {
                ui.label(
                    egui::RichText::new(
                        "None ticked: the shielding wires hang in the phases' case; else each \
                         case above is checked with the shielding wires in each case ticked here",
                    )
                    .small()
                    .weak(),
                );
                changed |= crate::case_groups::pick_names(ui, groups, &mut r.shield_cases);
                changed |= ticks(ui, cases, &mut r.shield_cases);
            });
            ui.end_row();
            // How each is solved (owner, 2026-11-01: "define the
            // computation type for the wires").
            ui.label("Phases' calculation");
            changed |= calculation_combo(ui, "lightning-phases-calc", &mut r.calculation);
            ui.end_row();
            ui.label("Shielding wires' calculation");
            changed |= calculation_combo(ui, "lightning-shields-calc", &mut r.shield_calculation);
            ui.end_row();
            ui.label("Method");
            ui.horizontal(|ui| {
                let angle = matches!(r.method, Method::Angle { .. });
                if ui.selectable_label(angle, "Shielding angle").clicked() && !angle {
                    r.method = Method::Angle {
                        outer: 0.0,
                        inner: 0.0,
                    };
                    changed = true;
                }
                if ui.selectable_label(!angle, "Rolling sphere").clicked() && angle {
                    r.method = Method::Sphere {
                        distance: StrikingDistance::Typed(0.0),
                        ground_factor: 1.0,
                    };
                    changed = true;
                }
            });
            ui.end_row();
            match &mut r.method {
                Method::Angle { outer, inner } => {
                    ui.label("Outer angle");
                    changed |= drag(ui, outer, 0.0..=89.9, " °");
                    ui.end_row();
                    ui.label("Inner angle");
                    changed |= drag(ui, inner, 0.0..=89.9, " °");
                    ui.end_row();
                }
                Method::Sphere {
                    distance,
                    ground_factor,
                } => {
                    ui.label("Striking distance");
                    ui.horizontal(|ui| {
                        let typed = matches!(distance, StrikingDistance::Typed(_));
                        if ui.selectable_label(typed, "typed").clicked() && !typed {
                            *distance = StrikingDistance::Typed(distance.metres());
                            changed = true;
                        }
                        if ui.selectable_label(!typed, "from a current").clicked() && typed {
                            *distance = StrikingDistance::Current {
                                ka: 10.0,
                                a: ohl_clearance::lightning::DEFAULT_A,
                                b: ohl_clearance::lightning::DEFAULT_B,
                            };
                            changed = true;
                        }
                        match distance {
                            StrikingDistance::Typed(m) => {
                                changed |= drag(ui, m, 0.0..=1000.0, " m");
                            }
                            StrikingDistance::Current { ka, a, b } => {
                                changed |= drag(ui, ka, 0.1..=300.0, " kA");
                                ui.label("r = A·I^b, A");
                                changed |= drag(ui, a, 0.1..=100.0, "");
                                ui.label("b");
                                changed |= drag(ui, b, 0.1..=2.0, "");
                            }
                        }
                    });
                    ui.end_row();
                    ui.label("");
                    ui.label(
                        egui::RichText::new(format!("{} to the wires", distance.describe()))
                            .small()
                            .weak(),
                    );
                    ui.end_row();
                    ui.label("Ground factor β");
                    changed |= drag(ui, ground_factor, 0.1..=2.0, "");
                    ui.end_row();
                }
            }
            ui.label("Step along the span");
            changed |= drag(ui, &mut r.step, 0.1..=100.0, " m");
            ui.end_row();
            ui.label("Phases protected");
            crate::menu::options_menu(ui, &phases_words(r), |ui| {
                ui.label(
                    egui::RichText::new(
                        "None ticked: every phase (a section with a system); a phase counts when \
                         it is in every list ticked",
                    )
                    .small()
                    .weak(),
                );
                ui.label(egui::RichText::new("Wire types").strong());
                changed |= ticks(ui, types, &mut r.targets.wire_types);
                ui.label(egui::RichText::new("Electrical systems").strong());
                changed |= ticks(ui, systems, &mut r.targets.systems);
                ui.label(egui::RichText::new("Circuits").strong());
                changed |= ticks(ui, circuits, &mut r.targets.circuits);
            });
            ui.end_row();
            ui.label("Shielding wires");
            crate::menu::options_menu(ui, &shields_words(r), |ui| {
                ui.label(
                    egui::RichText::new(
                        "None ticked: every earthed wire (a section without a system); else the \
                         wire types ticked",
                    )
                    .small()
                    .weak(),
                );
                changed |= ticks(ui, types, &mut r.shields);
            });
            ui.end_row();
        });
    help::explain(
        ui,
        match r.method {
            Method::Angle { .. } => {
                "The phases must lie within the angle from the vertical through a shielding wire \
                 above them: the inner angle towards another shielding wire, the outer elsewhere."
            }
            Method::Sphere { .. } => {
                "A sphere of the striking distance rolled over the shielding wires and the ground \
                 (β times the distance from it) must not touch a phase; smaller distances (weaker \
                 strokes) reach further in."
            }
        },
        "Each cross-section is square to its span at the middle of each step; a smaller step \
         finds shorter exposed lengths and takes longer (the time grows with the number of \
         cross-sections). No value is built in: the angles, the striking distance or current, \
         A, b and β are yours or a design code's.",
    );
    changed
}

/// The phases a rule protects, in words.
fn phases_words(r: &LightningRule) -> String {
    if r.targets.is_all() {
        "every phase".into()
    } else {
        r.targets.describe()
    }
}

/// A calculation to pick (none: each section's own); whether it changed.
fn calculation_combo(
    ui: &mut egui::Ui,
    id: &str,
    calc: &mut Option<ohl_line::condition::Calculation>,
) -> bool {
    let words = |c: Option<ohl_line::condition::Calculation>| {
        c.map_or("each section's own".to_owned(), |k| k.label())
    };
    let before = *calc;
    egui::ComboBox::from_id_salt(id)
        .selected_text(words(*calc))
        .show_ui(ui, |ui| {
            ui.selectable_value(calc, None, words(None));
            for k in ohl_line::condition::Calculation::ALL {
                ui.selectable_value(calc, Some(k), k.label());
            }
        });
    *calc != before
}

/// The shielding wires' cases of a rule, in words.
fn shield_cases_words(r: &LightningRule) -> String {
    let cases = if r.shield_cases.is_empty() {
        "the phases' case".into()
    } else {
        crate::rule_table::cases_words(&r.shield_cases)
    };
    match r.shield_calculation {
        Some(k) => format!("{cases}, {}", k.label()),
        None => cases,
    }
}

/// The shielding wires of a rule, in words.
fn shields_words(r: &LightningRule) -> String {
    if r.shields.is_empty() {
        "every earthed wire".into()
    } else {
        format!("wire types {}", r.shields.join(", "))
    }
}

/// A rule in words.
pub fn rule_text(r: &LightningRule) -> String {
    format!(
        "{}: {}, every {} m, {}; {}; shielding {} in {}",
        r.name,
        r.method.describe(),
        r.step,
        match r.calculation {
            Some(k) => format!(
                "{}, {}",
                crate::rule_table::cases_words(&r.cases),
                k.label()
            ),
            None => crate::rule_table::cases_words(&r.cases),
        },
        phases_words(r),
        shields_words(r),
        shield_cases_words(r)
    )
}

/// A lightning rule as a row of the shared rule table.
impl crate::rule_table::RuleRow for LightningRule {
    fn name(&self) -> &str {
        &self.name
    }

    fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }

    fn summary(&self, _: &[f64]) -> crate::rule_table::Summary {
        crate::rule_table::Summary {
            target: shields_words(self),
            voltages: "-".into(),
            cases: if self.shield_cases.is_empty() {
                crate::rule_table::cases_words(&self.cases)
            } else {
                format!(
                    "{}; shielding wires {}",
                    crate::rule_table::cases_words(&self.cases),
                    crate::rule_table::cases_words(&self.shield_cases)
                )
            },
            distance: if self.problem().is_some() {
                "not set".into()
            } else {
                match self.method {
                    Method::Angle { outer, inner } => format!("angle {outer}° / {inner}°"),
                    Method::Sphere { distance, .. } => format!("sphere {}", distance.describe()),
                }
            },
            wires: phases_words(self),
        }
    }
}

/// A tick per item of `all`, `chosen` the ticked; whether one changed.
fn ticks(ui: &mut egui::Ui, all: &[String], chosen: &mut Vec<String>) -> bool {
    let mut changed = false;
    for item in all {
        let mut on = chosen.contains(item);
        if ui.checkbox(&mut on, item).changed() {
            if on {
                chosen.push(item.clone());
            } else {
                chosen.retain(|x| x != item);
            }
            changed = true;
        }
    }
    for gone in chosen.clone().iter().filter(|x| !all.contains(x)) {
        let mut on = true;
        if ui
            .checkbox(&mut on, format!("{gone} (not in the model)"))
            .changed()
        {
            chosen.retain(|x| x != gone);
            changed = true;
        }
    }
    changed
}

/// A cross-section drawn to scale: the ground, the shielding wires, the
/// phases (exposed in red, `section` ringed), the covered zone's boundary
/// (dashed), the sphere's centres and its circles through them.
pub fn paint_cut(ui: &mut egui::Ui, cut: &Cut, method: Method, section: &str) {
    let width = ui.available_width().clamp(300.0, 820.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 300.0), egui::Sense::hover());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "lightning cross-section")
    });
    let v = ui.visuals();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, v.extreme_bg_color);
    // What to show: the wires and the zone, with room.
    let mut xs: Vec<f64> = cut.shields.iter().map(|w| w.s).collect();
    xs.extend(cut.phases.iter().map(|p| p.wire.s));
    let mut zs: Vec<f64> = cut.shields.iter().map(|w| w.z).collect();
    zs.extend(cut.phases.iter().map(|p| p.wire.z));
    let r = match method {
        Method::Sphere { distance, .. } => distance.metres(),
        Method::Angle { .. } => 0.0,
    };
    let (x0, x1) = xs
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| {
            (a.min(*x), b.max(*x))
        });
    let pad = (0.6 * r).max(25.0);
    let (x0, x1) = (x0 - pad, x1 + pad);
    let g_at = |s: f64| {
        cut.ground
            .iter()
            .min_by(|a, b| (a[0] - s).abs().total_cmp(&(b[0] - s).abs()))
            .map_or(0.0, |p| p[1])
    };
    let low = (x0 as i64..=x1 as i64)
        .map(|s| g_at(s as f64))
        .fold(f64::INFINITY, f64::min)
        .min(zs.iter().copied().fold(f64::INFINITY, f64::min));
    let high = zs.iter().copied().fold(f64::NEG_INFINITY, f64::max) + 0.25 * r.max(20.0);
    let scale = ((rect.width() as f64 - 20.0) / (x1 - x0))
        .min((rect.height() as f64 - 20.0) / (high - low).max(1.0));
    let to = |p: [f64; 2]| {
        egui::pos2(
            rect.center().x + ((p[0] - 0.5 * (x0 + x1)) * scale) as f32,
            rect.bottom() - 10.0 - ((p[1] - low) * scale) as f32,
        )
    };
    let ground: Vec<egui::Pos2> = cut
        .ground
        .iter()
        .filter(|p| p[0] >= x0 && p[0] <= x1)
        .map(|p| to(*p))
        .collect();
    painter.add(egui::Shape::line(
        ground,
        egui::Stroke::new(1.5, egui::Color32::from_rgb(150, 120, 80)),
    ));
    let zone = egui::Color32::from_rgb(90, 160, 230);
    // The sphere's circles at its centres (faint) and the centres.
    if r > 0.0 {
        for c in &cut.centres {
            painter.circle_stroke(
                to(*c),
                (r * scale) as f32,
                egui::Stroke::new(0.6, v.weak_text_color().gamma_multiply(0.5)),
            );
            painter.circle_filled(to(*c), 2.0, v.weak_text_color());
        }
    }
    let line: Vec<egui::Pos2> = cut
        .envelope
        .iter()
        .filter(|p| p[0] >= x0 && p[0] <= x1)
        .map(|p| to(*p))
        .collect();
    for w in line.windows(2) {
        crate::check_help::dashed(&painter, w[0], w[1], zone);
    }
    for w in &cut.shields {
        painter.circle_filled(to([w.s, w.z]), 3.5, v.text_color());
    }
    let red = egui::Color32::from_rgb(235, 70, 60);
    let green = egui::Color32::from_rgb(110, 200, 120);
    for p in &cut.phases {
        let at = to([p.wire.s, p.wire.z]);
        let c = if p.exposed { red } else { green };
        painter.circle_filled(at, 4.0, c);
        if p.section == section {
            painter.circle_stroke(at, 8.0, egui::Stroke::new(1.5, c));
            painter.text(
                at + egui::vec2(10.0, -10.0),
                egui::Align2::LEFT_BOTTOM,
                &p.section,
                egui::FontId::proportional(11.0),
                c,
            );
        }
    }
    // A scale bar.
    let bar = 10f64.powf((40.0 / scale).log10().ceil());
    let b0 = egui::pos2(rect.left() + 10.0, rect.top() + 14.0);
    painter.line_segment(
        [b0, b0 + egui::vec2((bar * scale) as f32, 0.0)],
        egui::Stroke::new(1.5, v.text_color()),
    );
    painter.text(
        b0 + egui::vec2((bar * scale) as f32 + 6.0, 0.0),
        egui::Align2::LEFT_CENTER,
        format!("{bar:.0} m"),
        egui::FontId::proportional(10.0),
        v.text_color(),
    );
    painter.text(
        egui::pos2(rect.right() - 10.0, rect.top() + 14.0),
        egui::Align2::RIGHT_CENTER,
        "dashed: the covered zone's boundary · red: exposed · green: covered",
        egui::FontId::proportional(10.0),
        v.weak_text_color(),
    );
}

/// The 3D layer: the exposed parts of the phases (red), the covered
/// zone's boundary stitched between the cross-sections of each span
/// (translucent), dimmed while out of date.
pub fn scene(origin: [f64; 3], overlay: &Overlay) -> ohl_render::Scene {
    let mut s = ohl_render::Scene::new(origin);
    let Some(r) = &overlay.report else {
        return s;
    };
    let red = crate::revision::dimmed([235, 70, 60, 255], overlay.stale);
    for row in r.exposed() {
        for part in &row.exposed_parts {
            s.polyline(part, red, 4.0);
        }
    }
    let zone = crate::revision::dimmed([90, 160, 230, 50], overlay.stale);
    // Each span's envelopes resampled to one count, then stitched.
    const N: usize = 48;
    let resample = |line: &[[f64; 2]]| -> Option<Vec<[f64; 2]>> {
        if line.len() < 2 {
            return None;
        }
        let mut lens = vec![0.0];
        for w in line.windows(2) {
            let d = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
            lens.push(lens.last().unwrap_or(&0.0) + d);
        }
        let total = *lens.last()?;
        if total <= 0.0 {
            return None;
        }
        let mut out = Vec::with_capacity(N);
        let mut k = 0;
        for i in 0..N {
            let at = total * i as f64 / (N - 1) as f64;
            while k + 1 < lens.len() - 1 && lens[k + 1] < at {
                k += 1;
            }
            let f = ((at - lens[k]) / (lens[k + 1] - lens[k]).max(1e-12)).clamp(0.0, 1.0);
            out.push([
                line[k][0] + f * (line[k + 1][0] - line[k][0]),
                line[k][1] + f * (line[k + 1][1] - line[k][1]),
            ]);
        }
        Some(out)
    };
    for pair in r.report.cuts.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if a.from != b.from || a.to != b.to {
            continue;
        }
        let (Some(la), Some(lb)) = (resample(&a.envelope), resample(&b.envelope)) else {
            continue;
        };
        for i in 0..N - 1 {
            let (p, q) = (a.world(la[i]), a.world(la[i + 1]));
            let (u, w) = (b.world(lb[i]), b.world(lb[i + 1]));
            s.triangle([p, q, w], [zone; 3]);
            s.triangle([p, w, u], [zone; 3]);
        }
    }
    s
}

/// What the window asks of the app: go to a span (drawn in the rule's
/// case), its preset's weather cases added, the 3D layer redrawn.
pub fn handle(r: &mut crate::app::Ready, out: Out) {
    if let Some(target) = out.go {
        r.go_to(target.clone());
        if let Some(case) = &out.case {
            r.show_check_case(crate::dockable::Panel::Lightning, target, case);
        }
    }
    if let Some((plugin, p)) = out.plugin_preset
        && let Some(wc) = &p.weather
        && let Err(e) = crate::rule_editor::preset_cases(r, &plugin, &p.name, &p.weather_text, wc)
    {
        r.status = Some((
            format!("preset {plugin} {}: weather cases: {e}", p.name),
            true,
        ));
    }
    if let Some(overlay) = out.overlay {
        if overlay.report.is_some() {
            r.visible.insert(crate::scene::Layer::Lightning);
        }
        r.view3d.lightning = overlay;
        r.view3d.dirty.insert(crate::scene::Layer::Lightning);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    /// Checks › Electrical › Lightning shielding on the shielded example:
    /// its rule file read when the window opens (named after the line);
    /// Run lists every phase and span by both rules: the outer phases
    /// exposed to the 8 kA sphere (their r_max above its 38.6 m), the low
    /// middle one covered, every phase within the 30° angle; a row
    /// clicked draws its cross-section; the 3D layer drawn while open,
    /// taken off once closed.
    #[test]
    fn the_window_runs_on_the_shielded_example() {
        let model = crate::tests::shielded();
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 1200.0))
            .build_eframe(|_| crate::app::App::with_model(model, None));
        harness.run();
        crate::tests::submenu(&mut harness, "Checks", "Electrical");
        harness.get_by_label("Lightning shielding…").click();
        harness.run();
        {
            let r = harness.state_mut().ready().unwrap();
            assert!(r.lightning.open);
            assert_eq!(r.lightning.rules.len(), 2, "{:?}", r.lightning.status);
            assert!(r.lightning.why_not_run(&r.model).is_none());
        }
        harness.run();
        harness
            .query_all_by_label("Run")
            .max_by(|a, b| a.rect().bottom().total_cmp(&b.rect().bottom()))
            .expect("Run in the footer")
            .click();
        let started = std::time::Instant::now();
        loop {
            harness.step();
            let r = harness.state_mut().ready().unwrap();
            if r.lightning.job.result.is_some() {
                break;
            }
            assert!(started.elapsed().as_secs() < 180, "no report in 180 s");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        harness.run();
        {
            let r = harness.state_mut().ready().unwrap();
            let Some(Ok(rep)) = &r.lightning.job.result else {
                panic!("{:?}", r.lightning.job.result.as_ref().map(|r| r.is_ok()));
            };
            // 3 phases × 4 spans × 2 rules.
            assert_eq!(rep.report.rows.len(), 24);
            let exposed: Vec<&Row> = rep.exposed().collect();
            assert!(!exposed.is_empty());
            for x in &exposed {
                // The sphere only (the 30° angle covers every phase), never
                // the low middle phase L3, and r_max above the rule's r.
                let Method::Sphere { distance, .. } = rep.rules[x.rule].method else {
                    panic!("{} exposed by an angle", x.phase);
                };
                assert!(!x.phase.ends_with("L3"), "{}", x.phase);
                assert!(x.margin.unwrap() > distance.metres(), "{:?}", x.margin);
            }
            // Covered rows' r_max below it.
            for x in rep.report.rows.iter().filter(|x| x.passes()) {
                if let Method::Sphere { distance, .. } = rep.rules[x.rule].method {
                    assert!(x.margin.unwrap_or(0.0) <= distance.metres() + 0.01);
                }
            }
            assert!(r.view3d.lightning.report.is_some());
            assert!(r.visible.contains(&crate::scene::Layer::Lightning));
            let (lines, _, triangles) = scene([0.0; 3], &r.view3d.lightning).counts();
            assert!(
                lines > 0 && triangles > 0,
                "{lines} lines, {triangles} triangles"
            );
        }
        // The verdict, then a row clicked: its cross-section drawn.
        harness.get_by_label_contains("exposed, ");
        harness.get_by_label("Click a row to draw its cross-section at its worst place.");
        harness.get_by_label_contains("✖ ").click();
        harness.run();
        harness.run();
        harness.get_by_label("lightning cross-section");
        let r = harness.state_mut().ready().unwrap();
        r.lightning.open = false;
        harness.run();
        let r = harness.state_mut().ready().unwrap();
        assert!(r.view3d.lightning.report.is_none());
    }

    /// The 3D layer rendered for review (with `OHL_RENDER_PNG` and a GPU;
    /// else nothing): the shielded example checked, the camera on span
    /// T2–T3 from the side.
    #[test]
    fn the_3d_layer_rendered_for_review() {
        let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG")
            .filter(|_| ohl_render::headless_device().is_ok())
        else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .wgpu()
            .build_eframe(|cc| {
                crate::app::App::with_model(crate::tests::shielded(), cc.wgpu_render_state.clone())
            });
        harness.run();
        let ctx = harness.ctx.clone();
        {
            let r = harness.state_mut().ready().unwrap();
            r.lightning.show(&r.model);
            r.lightning.start(&r.model, &ctx);
        }
        let started = std::time::Instant::now();
        while harness.state_mut().ready().unwrap().lightning.job.running() {
            harness.step();
            assert!(started.elapsed().as_secs() < 180);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        harness.run();
        {
            let r = harness.state_mut().ready().unwrap();
            r.lightning.job.poll();
            let Some(Ok(rep)) = r.lightning.job.result.clone() else {
                panic!("no report");
            };
            r.lightning.open = false;
            harness.run();
            let r = harness.state_mut().ready().unwrap();
            r.view3d.lightning = Overlay {
                report: Some(rep),
                stale: false,
            };
            r.visible.insert(crate::scene::Layer::Lightning);
            r.view3d.dirty.insert(crate::scene::Layer::Lightning);
            let (a, b) = (
                r.model.line.structure("T2").unwrap().position,
                r.model.line.structure("T3").unwrap().position,
            );
            let target = [
                0.5 * (a[0] + b[0]),
                0.5 * (a[1] + b[1]),
                0.5 * (a[2] + b[2]) + 20.0,
            ];
            if let crate::field_results::RowAction::Ask(request) =
                crate::field_results::look(&r.model, "T2", "T3", target, 330.0)
            {
                crate::app::field_request(r, request, crate::scene::Layer::Lightning);
            }
        }
        harness.state_mut().show_tab(crate::app::Tab::View3d);
        for _ in 0..8 {
            harness.run();
        }
        harness
            .render()
            .unwrap()
            .save(dir.join("lightning-3d.png"))
            .unwrap();
    }
}
