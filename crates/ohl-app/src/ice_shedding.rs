// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › **Ice shedding…** (roadmap O13, owner 2026-11-02: the
//! simplified method; built, in active development, not approved yet):
//! the conductor jump when the ice drops ([`ohl_clearance::ice_shedding`]).
//! The rules are kept in the project as a rule file
//! (`<model>.ice-shedding-rules.ohlr`, `[[ice_shedding_rule]]` tables; a
//! design-code plugin's preset brings its ice shedding rules). Run solves
//! each iced case of the rules (and, for a rule shedding every span, its
//! bare case) in the background; per span of the wires named, the jumped
//! wire against each other wire of its span: the clearance before and
//! after, the required, the smallest margin first. A row clicked selects
//! its span drawn in its iced case. The 3D view's layer *Ice shedding*
//! draws the jumped spans (red where a clearance is short) while the
//! window is open. A run is saved in Checks › Performed checks.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use ohl_clearance::ice_shedding::{
    self as engine, IceSheddingReport, IceSheddingRule, Method, Shedding,
};
use ohl_clearance::rules::{PairVoltage, PresetCheck, RuleSet};
use ohl_session::reports::CheckReport;
use serde_json::json;

use crate::check_help as help;
use crate::check_ui;
use crate::job::Job;
use crate::model::Model;
use crate::selection::Selection;

/// What its results are computed from ([`crate::revision`]).
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::LINE;

/// Its id in the library of performed checks.
pub const CHECK_ID: &str = "ohl.ice-shedding";

/// The id of the window's body, tabs and footer.
const WINDOW: &str = "ice-shedding-window";

/// A check's answer: the report and the rules it was made with.
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// The rows, jumped spans and notes.
    pub report: IceSheddingReport,
    /// The rules checked.
    pub rules: Vec<IceSheddingRule>,
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

/// The Ice shedding window.
#[derive(Default)]
pub struct IceSheddingWindow {
    /// Shown.
    pub open: bool,
    /// The structure and section overrides of its cases.
    pub overrides: crate::case_groups::Overrides,
    /// The circuits, routes and sections that shed (kept in its rule
    /// file's `[check]`).
    pub scope: check_ui::Scope,
    /// The project's rule file read.
    loaded: bool,
    /// The rule file kept.
    pub file: Option<PathBuf>,
    /// The rules.
    pub rules: Vec<IceSheddingRule>,
    /// What was done last, in words (an error: true).
    pub status: Option<(String, bool)>,
    /// The check.
    pub(crate) job: Job<Arc<Report>>,
    /// What the views draw: (shown, generation, stale).
    drawn: Option<(bool, u64, bool)>,
    /// Save in Performed checks.
    saving: crate::field_saved::Saving,
    /// A report saved: the command the app applies.
    pub apply: Option<(String, ohl_session::Command)>,
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
    dir.join(format!("{stem}.ice-shedding-rules.ohlr"))
}

impl IceSheddingWindow {
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
                    self.rules = set.ice_shedding_rules;
                    if let Some(c) = set.check {
                        self.overrides = crate::case_groups::Overrides {
                            structures: c.structure_override,
                            sections: c.section_override,
                        };
                        self.scope = check_ui::Scope::of_check(&c);
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
            .filter_map(IceSheddingRule::problem)
            .collect();
        if !why.is_empty() {
            return Some(why.join("; "));
        }
        if !model
            .cases
            .iter()
            .any(|c| c.case.has_ice() && self.rules.iter().any(|r| r.applies(&c.case.name)))
        {
            return Some("none of the rules' weather cases has ice (3 Rules)".into());
        }
        if let Some(r) = self.rules.iter().find(|r| {
            r.bare
                .as_ref()
                .is_some_and(|b| !model.cases.iter().any(|c| &c.case.name == b))
        }) {
            return Some(format!(
                "\"{}\": its bare case is not a weather case of the model (3 Rules)",
                r.name
            ));
        }
        self.scope.why_not(&model.line)
    }

    /// The rules as a rule file.
    pub fn rule_set(&self) -> RuleSet {
        RuleSet {
            name: "Ice shedding".into(),
            check: check_ui::check_of(&self.scope, self.overrides),
            checks: vec![PresetCheck::IceShedding],
            ice_shedding_rules: self.rules.clone(),
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

    /// The iced cases × rules to run.
    fn work(&self, model: &Model) -> usize {
        model
            .cases
            .iter()
            .filter(|c| c.case.has_ice())
            .map(|c| {
                self.rules
                    .iter()
                    .filter(|r| r.applies(&c.case.name))
                    .count()
            })
            .sum()
    }

    /// Starts a check: each iced case of the rules (and the bare cases
    /// asked) solved in the background, every span checked.
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
        let filter = self.overrides.filter(model);
        let keep = self.scope.keeps(&model.line);
        self.saving.stale = None;
        self.job.start_at(ctx, model.revision(), move || {
            let (plan, mut notes) = engine::solves(&rules, &cases, cases.len());
            let mut shapes = Vec::new();
            for (case, calc) in &plan {
                let l = ohl_clearance::calculation::line_for(&line, *calc);
                let mut solved = Vec::new();
                for (section, r) in l
                    .sections
                    .iter()
                    .zip(ohl_line::solve::solve_line_memo_default(&l, &library, case))
                {
                    match r {
                        Ok(s) => solved.push(s),
                        Err(e) => notes.push(format!(
                            "\"{}\": section {} not solved: {e}",
                            ohl_clearance::calculation::label(&case.name, *calc),
                            section.id
                        )),
                    }
                }
                shapes.push(solved);
            }
            let solved: Vec<engine::Solved<'_>> = plan
                .iter()
                .zip(&shapes)
                .map(|((c, k), s)| (c, *k, s.as_slice()))
                .collect();
            // Exit clause: seconds for a long line.
            let budget =
                ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(600));
            let mut report = engine::check(
                &line,
                &library,
                &rules,
                &cases,
                cases.len(),
                &solved,
                &filter,
                keep.as_ref(),
                &budget,
            )?;
            report.notes.splice(0..0, notes);
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

    /// The saved run `r` shown (its rules and results), with what changed
    /// since it was saved.
    pub fn load_saved(&mut self, r: &CheckReport, model: &Model) -> Result<(), String> {
        let text = r
            .parameters
            .get("rules")
            .and_then(serde_json::Value::as_str)
            .ok_or("the saved run has no rules")?;
        let rules = RuleSet::parse(text)?.ice_shedding_rules;
        let report: IceSheddingReport =
            serde_json::from_value(r.results.get("report").cloned().unwrap_or_default())
                .map_err(|e| format!("the saved results: {e}"))?;
        if report.rows.iter().any(|x| x.rule >= rules.len()) {
            return Err("the saved results name a rule not saved".into());
        }
        self.job.stop();
        self.job
            .stamp
            .set(crate::revision::saved_revision(&r.parameters, model));
        self.job.result = Some(Ok(Arc::new(Report {
            report,
            rules: rules.clone(),
        })));
        self.job.generation += 1;
        self.rules = rules;
        self.loaded = true;
        self.saving.stale = crate::field_saved::changed_since(r, model);
        self.saving.name = r.id.clone();
        self.open = true;
        Ok(())
    }

    /// The run shown as report `name` of the performed checks.
    fn check_report(&self, model: &Model, name: &str) -> Result<CheckReport, String> {
        let Some(Ok(a)) = &self.job.result else {
            return Err("run it first".into());
        };
        let failed = a.report.failures().count();
        let summary = format!(
            "{} pair{} × case × rule, {failed} below the required clearance after the jump",
            a.report.rows.len(),
            if a.report.rows.len() == 1 { "" } else { "s" }
        );
        let rules = RuleSet {
            name: "Ice shedding".into(),
            checks: vec![PresetCheck::IceShedding],
            ice_shedding_rules: a.rules.clone(),
            ..RuleSet::default()
        };
        crate::field_saved::report(
            model,
            (CHECK_ID, "Ice shedding"),
            name,
            summary,
            json!({
                "version": crate::field_saved::SAVED_VERSION,
                "loads_per": self.job.stamp.loads_per(model).as_str(),
                "rules": rules.to_toml(),
            }),
            json!({ "report": serde_json::to_value(&a.report).map_err(|e| e.to_string())? }),
        )
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
            crate::dockable::Panel::IceShedding,
            "Ice shedding",
            [860.0, 760.0],
            &mut open,
            |ui| {
                let why = self.why_not_run(model);
                let scope_words = check_ui::scope_words(&model.line, &self.scope, true);
                let findings = match &self.job.result {
                    Some(Ok(r)) => Some(r.report.failures().count()),
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
                            "The jump of a wire when its ice drops, against the other wires of \
                             its span (phase to phase, phase to earth wire): the clearance after \
                             the jump against the rule's distance.",
                            "Simplified method: the static rise from the iced to the bare \
                             position at rest, times the rule's rebound factor, is added to the \
                             iced position. One span shedding alone (the default) holds the \
                             span's iced tension — the bare wire rises the most (conservative); \
                             every span shedding solves the line in the bare case. The other \
                             wires keep their ice. No factor is built in: take it from the \
                             published source you design to. Built, in active development, not \
                             approved yet.",
                        );
                        check_ui::how_computed(
                            ui,
                            WINDOW,
                            "Each iced case of the rules solved; per span of the wires named the \
                             rise Δ (bare less iced at rest) and the jumped wire, iced + k Δ; the \
                             shortest distance from it to each other wire of the span (segment \
                             against segment, to the surfaces: the jumped wire bare, the other \
                             iced).",
                            help::ice_jump,
                        );
                        ui.separator();
                        check_ui::section(ui, 2, "Scope", None);
                        if check_ui::ScopePicker::new("ice-shedding-scope", &model.line)
                            .route_of(model)
                            .wires(&mut self.scope, true)
                            .ui(ui)
                        {
                            changed = true;
                        }
                        ui.label(
                            egui::RichText::new(
                                "The wires that shed; the other wires of their spans are checked \
                                 against in any case. Each rule gives its own wires and iced \
                                 cases besides (3 Rules).",
                            )
                            .small()
                            .weak(),
                        );
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
                        self.results_ui(ui, model, &stale, &mut run, &mut out);
                    }),
                }
                let work = self.work(model);
                help::footer(ui, WINDOW, |ui| {
                    let overrides = &mut self.overrides;
                    run |= check_ui::job_footer(
                        ui,
                        &mut self.job,
                        why.clone(),
                        "Solve each iced case of the rules and check every span's jump against \
                         the other wires of its span",
                        Some(scope_words),
                        Some(check_ui::scope_plan(&model.line, &self.scope, work)),
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
        let part = crate::preset_cases::Part::Check(PresetCheck::IceShedding);
        let cases: Vec<String> = model.cases.iter().map(|c| c.case.name.clone()).collect();
        let groups = crate::case_groups::groups(model);
        let mut voltages: Vec<f64> = ohl_clearance::case::voltages_of(&model.line)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| p.map(|p| p.nominal_kv))
            .collect();
        voltages.sort_by(f64::total_cmp);
        voltages.dedup();
        let mut types: Vec<String> = model
            .line
            .sections
            .iter()
            .map(|s| s.cable.clone())
            .collect();
        types.sort();
        types.dedup();
        let mut systems: Vec<String> = model.line.systems.iter().map(|s| s.id.clone()).collect();
        systems.push("none".into());
        let circuits: Vec<String> = model.line.circuits.iter().map(|c| c.name.clone()).collect();
        let t = check_ui::RuleTable::new("ice-shedding-rules", &mut self.rules, &voltages)
            .add(
                "An ice shedding rule",
                "every wire, one span shedding alone; set its rebound factor and distance",
                |n| IceSheddingRule::blank(&format!("rule {}", n + 1)),
            )
            .presets(model, presets, part, |set| {
                set.ice_shedding_rules.iter().map(rule_text).collect()
            })
            .ui(ui, |ui, k, r| {
                rule_fields(
                    ui,
                    k,
                    r,
                    &cases,
                    &groups,
                    &voltages,
                    (&types, &systems, &circuits),
                )
            });
        let mut changed = t.changed;
        if let Some(pick) = t.preset {
            match pick.taken(model, part) {
                Ok((p, lines)) => {
                    let done = crate::rule_table::merge(
                        &mut self.rules,
                        p.rules.ice_shedding_rules.clone(),
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
                    self.rules = set.ice_shedding_rules;
                    changed = true;
                }
                Err(e) => self.status = Some((e, true)),
            }
        }
        if t.export
            && let Some(p) = crate::rule_table::export_file("ice-shedding-rules.ohlr")
        {
            self.status = Some(match self.rule_set().save(&p) {
                Ok(()) => (format!("exported to {}", p.display()), false),
                Err(e) => (e, true),
            });
        }
        changed
    }

    /// The results: the verdict, the notes, every pair (the smallest
    /// margin first), Save in Performed checks.
    fn results_ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
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
        } else if let Some(what) = &self.saving.stale {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!("Saved run: {what} changed since it was saved."),
            );
        }
        let failed = report.report.failures().count();
        let worst = report.report.rows.first();
        let text = match worst {
            Some(r) if failed > 0 => format!(
                "✖ {failed} pair{} × case below the required clearance after the jump — worst {} \
                 to {} span {}: {:.2} m of {:.2} m (jump {:.2} m, rule {}, case {})",
                if failed == 1 { "" } else { "s" },
                Cell::span(&r.section, r.span).text,
                r.other,
                r.other_span + 1,
                r.after,
                r.required,
                r.jump,
                report.rules[r.rule].name,
                r.case
            ),
            Some(r) => format!(
                "✔ Every clearance kept after the jump ({} pairs × cases × rules; smallest \
                 margin {:.2} m)",
                report.report.rows.len(),
                r.margin()
            ),
            None => {
                "✖ Nothing checked: no span of the rules' wires with another wire in it".to_owned()
            }
        };
        if check_ui::verdict(ui, failed > 0 || worst.is_none(), &text, failed > 0)
            && let Some(r) = worst
        {
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
                        Cell::span(&r.other, r.other_span),
                        Cell::text(&rule.name),
                        Cell::text(ohl_clearance::calculation::label(&r.case, r.calculation)),
                        Cell::num(Some(r.rise), 2),
                        Cell::num(Some(r.jump), 2),
                        Cell::num(Some(r.before), 2),
                        Cell::num(Some(r.after), 2).bad(bad),
                        Cell::num(Some(r.required), 2),
                        Cell::num(Some(r.at * 100.0), 0),
                        Cell::text(if bad { "fail" } else { "pass" }).bad(bad),
                    ],
                    bad,
                )
            })
            .collect();
        if let Some(k) = check_ui::ResultTable::new(
            "ice-shedding-rows",
            vec![
                Col::text("Span"),
                Col::text("Other wire"),
                Col::text("Rule"),
                Col::text("Case"),
                Col::num("Rise [m]").tip("The largest static rise, iced to bare at rest"),
                Col::num("Jump [m]").tip("The rule's rebound factor × the rise"),
                Col::num("Before [m]").tip("The clearance with both wires iced"),
                Col::num("After [m]").tip("The clearance with the wire jumped"),
                Col::num("Required [m]"),
                Col::num("At [%]").tip("Where along the span, in plan"),
                Col::text("Verdict"),
            ],
        )
        .rows(rows)
        .group_by(&[2, 3, 0])
        .csv("ice-shedding-table.csv")
        .full_csv(
            "ice-shedding.csv",
            "Every row with every field (the bare case, the factor, the margin, the closest \
             points), as ohl ice-shedding writes it",
            || report.report.csv(&report.rules),
        )
        .ui(ui)
        {
            let r = &report.report.rows[k];
            out.go = Some(Selection::Span {
                section: r.section.clone(),
                span: r.span,
            });
            out.case = Some(r.case.clone());
        }
        // Save in Performed checks.
        ui.separator();
        if let Some(name) = self.saving.ui(ui, model) {
            let r = self.check_report(model, &name).map(|report| {
                self.apply = Some((format!("saved the ice shedding check {name}"), report.put()));
            });
            self.saving.outcome(&name, &r);
        }
    }
}

/// A rule in words: "drop: k = 1.5, one span alone (tension held), 2 m;
/// cases glaze ice; every wire".
pub fn rule_text(r: &IceSheddingRule) -> String {
    format!(
        "{}: rebound factor {}, {}, distance {}, {}; {}",
        r.name,
        r.factor,
        r.shedding.describe(),
        crate::rule_table::distance_words(&r.distance),
        cases_words(r),
        r.targets.describe()
    )
}

/// The iced and bare cases of a rule, in words.
fn cases_words(r: &IceSheddingRule) -> String {
    let iced = if r.cases.is_empty() {
        "every case with ice".to_owned()
    } else {
        r.cases.join(", ")
    };
    let bare = match &r.bare {
        Some(b) => format!("bare: {b}"),
        None => "bare: the iced case without ice".to_owned(),
    };
    match r.calculation {
        Some(k) => format!("{iced} ({bare}), {}", k.label()),
        None => format!("{iced} ({bare})"),
    }
}

/// An ice shedding rule as a row of the shared rule table.
impl check_ui::RuleRow for IceSheddingRule {
    fn name(&self) -> &str {
        &self.name
    }

    fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }

    fn summary(&self, _: &[f64]) -> check_ui::Summary {
        check_ui::Summary {
            target: format!("jump × {}, {}", self.factor, self.shedding.as_str()),
            voltages: "-".into(),
            cases: cases_words(self),
            distance: crate::rule_table::distance_words(&self.distance),
            wires: self.targets.describe(),
        }
    }
}

/// A rule's fields; whether one changed.
fn rule_fields(
    ui: &mut egui::Ui,
    k: usize,
    r: &mut IceSheddingRule,
    cases: &[String],
    groups: &[ohl_weather::CaseGroup],
    voltages: &[f64],
    (types, systems, circuits): (&[String], &[String], &[String]),
) -> bool {
    let mut changed = false;
    egui::Grid::new("ice-shedding-rule")
        .num_columns(2)
        .spacing(help::GRID_SPACING)
        .show(ui, |ui| {
            ui.label("Iced cases");
            crate::menu::options_menu(
                ui,
                &if r.cases.is_empty() {
                    "every case with ice".to_owned()
                } else {
                    r.cases.join(", ")
                },
                |ui| {
                    ui.label(
                        egui::RichText::new(
                            "None ticked: every case with ice; a case without ice is skipped",
                        )
                        .small()
                        .weak(),
                    );
                    changed |= crate::case_groups::pick_names(ui, groups, &mut r.cases);
                    changed |= ticks(ui, cases, &mut r.cases);
                },
            );
            ui.end_row();
            ui.label("Bare case");
            let bare_text = r
                .bare
                .clone()
                .unwrap_or_else(|| "the iced case without its ice".into());
            egui::ComboBox::from_id_salt(("ice-shedding-bare", k))
                .selected_text(bare_text)
                .show_ui(ui, |ui| {
                    changed |= ui
                        .selectable_value(&mut r.bare, None, "the iced case without its ice")
                        .changed();
                    for c in cases {
                        changed |= ui
                            .selectable_value(&mut r.bare, Some(c.clone()), c)
                            .changed();
                    }
                });
            ui.end_row();
            ui.label("Calculation");
            changed |=
                crate::rule_table::calculation_combo(ui, "ice-shedding-calc", &mut r.calculation);
            ui.end_row();
            ui.label("Method");
            egui::ComboBox::from_id_salt(("ice-shedding-method", k))
                .selected_text(r.method.describe())
                .show_ui(ui, |ui| {
                    changed |= ui
                        .selectable_value(
                            &mut r.method,
                            Method::Simplified,
                            Method::Simplified.describe(),
                        )
                        .changed();
                });
            ui.end_row();
            ui.label("Spans shedding");
            ui.horizontal(|ui| {
                for s in [Shedding::One, Shedding::All] {
                    changed |= ui
                        .selectable_value(&mut r.shedding, s, s.describe())
                        .changed();
                }
            });
            ui.end_row();
            ui.label("Rebound factor k");
            ui.horizontal(|ui| {
                let mut f = r.factor;
                let resp = crate::style::field(
                    ui,
                    egui::DragValue::new(&mut f)
                        .range(0.0..=10.0)
                        .speed(0.01)
                        .max_decimals(3),
                );
                if resp.changed() {
                    r.factor = f;
                }
                changed |= resp.drag_stopped() || resp.lost_focus();
                if r.factor <= 0.0 {
                    ui.label(
                        egui::RichText::new("set the factor (from a published source)")
                            .color(ui.visuals().warn_fg_color),
                    );
                }
            });
            ui.end_row();
            ui.label("Distance");
            changed |= crate::requirement_ui::requirement_ui(
                ui,
                ("ice-shedding-distance", k),
                &mut r.distance,
                voltages,
            );
            ui.end_row();
            ui.label("Pair voltage");
            egui::ComboBox::from_id_salt(("ice-shedding-pair", k))
                .selected_text(pair_words(r.pair_voltage))
                .show_ui(ui, |ui| {
                    for p in [
                        PairVoltage::Higher,
                        PairVoltage::Sum,
                        PairVoltage::Difference,
                    ] {
                        changed |= ui
                            .selectable_value(&mut r.pair_voltage, p, pair_words(p))
                            .changed();
                    }
                });
            ui.end_row();
            ui.label("Wires that shed");
            crate::menu::options_menu(ui, &r.targets.describe(), |ui| {
                ui.label(
                    egui::RichText::new(
                        "None ticked: every wire; a wire counts when it is in every list ticked",
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
        });
    // What each setting does to the results (settings explain their
    // effect).
    help::explain(
        ui,
        "The jump is k × the static rise; k = 1 is the bare wire at rest, a larger k a higher \
         overshoot and a smaller clearance after the jump.",
        "Spans shedding — one span alone: the iced tension held (the neighbouring iced spans \
         hold it through the strings), the largest rise: conservative, one solve per iced case. \
         Every span: the line solved also in the bare case (one solve more per iced case), the \
         tension drops and the rise is smaller. Bare case: its loads give the bare wire's \
         weight (one span) or the bare line (every span); the iced case without its ice keeps \
         its temperature. Pair voltage: the voltage the distance is read at for a distance by \
         voltage. No value is built in: the factor from a published source, the distance from \
         your regulation.",
    );
    changed
}

/// A pair voltage in words.
fn pair_words(p: PairVoltage) -> &'static str {
    match p {
        PairVoltage::Higher => "the higher of the two",
        PairVoltage::Sum => "the sum of both",
        PairVoltage::Difference => "the voltage between them",
    }
}

/// A tick per item of `all`, `chosen` the ticked; whether one changed (the
/// menu stays open).
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

/// The 3D layer: each jumped span (red where a clearance after the jump
/// is short, else orange) and, for each pair below its requirement, the
/// shortest distance as a red line.
pub fn scene(origin: [f64; 3], overlay: &Overlay) -> ohl_render::Scene {
    let mut s = ohl_render::Scene::new(origin);
    let Some(r) = &overlay.report else {
        return s;
    };
    let red = crate::revision::dimmed([235, 70, 60, 255], overlay.stale);
    let orange = crate::revision::dimmed([240, 160, 40, 230], overlay.stale);
    for j in &r.report.jumped {
        s.polyline(&j.points, if j.short { red } else { orange }, 3.0);
    }
    for row in r.report.failures() {
        s.polyline(&row.points, red, 2.0);
    }
    s
}

/// What the window asks of the app: go to a span (drawn in the row's
/// case), a preset's weather cases, the 3D layer.
pub fn handle(r: &mut crate::app::Ready, out: Out) {
    if let Some(target) = out.go {
        r.go_to(target.clone());
        if let Some(case) = &out.case {
            r.show_check_case(crate::dockable::Panel::IceShedding, target, case);
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
            r.visible.insert(crate::scene::Layer::IceShedding);
        }
        r.view3d.ice_shedding = overlay;
        r.view3d.dirty.insert(crate::scene::Layer::IceShedding);
    }
    if let Some((what, command)) = r.ice_shedding.apply.take() {
        r.perform(crate::edit::Action::Command { what, command });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    /// The rules and the scope are kept in the rule file and read back.
    #[test]
    fn the_rules_are_kept_in_the_rule_file() {
        let mut w = IceSheddingWindow::default();
        w.overrides.sections = true;
        w.rules = vec![IceSheddingRule {
            factor: 1.5,
            distance: ohl_clearance::rules::Requirement::Fixed(1.0),
            ..IceSheddingRule::blank("invented")
        }];
        let text = w.rule_set().to_toml();
        assert!(text.contains("[[ice_shedding_rule]]"), "{text}");
        let back = RuleSet::parse(&text).unwrap();
        assert_eq!(back.ice_shedding_rules, w.rules);
        assert_eq!(back.checks, [PresetCheck::IceShedding]);
        assert!(back.check.unwrap().section_override);
    }

    /// Checks › Ice shedding… on the example line with the example's
    /// rules (invented factors): Run lists every pair of S1 and S2 by
    /// both rules, the smallest margin first; the 3D layer drawn while the
    /// window is open; a row clicked selects its span; a distance too
    /// large fails; the run saved in Performed checks and opened back;
    /// closed, the layer goes.
    #[test]
    fn the_window_runs_draws_and_saves() {
        let model = crate::tests::synthetic_project("ice-shedding-window");
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 1200.0))
            .build_eframe(|_| crate::app::App::with_model(model, None));
        harness.run();
        harness.get_by_label("Checks").click();
        harness.run();
        harness.get_by_label("Ice shedding…").click();
        harness.run();
        {
            let r = harness.state_mut().ready().unwrap();
            let w = &mut r.ice_shedding;
            assert!(w.open);
            assert!(w.why_not_run(&r.model).is_some());
            w.rules = RuleSet::load(
                &crate::tests::root().join("examples/synthetic-line/ice-shedding-rules.ohlr"),
            )
            .unwrap()
            .ice_shedding_rules;
            // A distance no pair keeps: every row fails.
            w.rules[1].distance = ohl_clearance::rules::Requirement::Fixed(50.0);
            assert!(w.why_not_run(&r.model).is_none());
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
            if let Some(res) = &r.ice_shedding.job.result {
                let rep = res.as_ref().unwrap();
                // S1 and S2 in four spans: 8 pairs per rule.
                assert_eq!(rep.report.rows.len(), 16, "{:?}", rep.report.notes);
                assert_eq!(rep.report.failures().count(), 8);
                assert!(rep.report.failures().all(|x| x.rule == 1));
                assert!(
                    rep.report
                        .rows
                        .windows(2)
                        .all(|w| w[0].margin() <= w[1].margin())
                );
                break;
            }
            assert!(started.elapsed().as_secs() < 120, "no report in 120 s");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        harness.run();
        harness.get_by_label_contains("below the required clearance after the jump — worst");
        {
            let r = harness.state_mut().ready().unwrap();
            assert!(r.visible.contains(&crate::scene::Layer::IceShedding));
            // Every span jumped, by two rules: S1 and S2 four spans each,
            // the branch S3 two (no other wire in them: no row).
            assert_eq!(
                r.view3d
                    .ice_shedding
                    .report
                    .as_ref()
                    .map(|x| x.report.jumped.len()),
                Some(20)
            );
            // Saved in Performed checks and read back.
            let report = r.ice_shedding.check_report(&r.model, "ice run").unwrap();
            assert_eq!(report.check, CHECK_ID);
            let mut again = IceSheddingWindow::default();
            again.load_saved(&report, &r.model).unwrap();
            let (Some(Ok(a)), Some(Ok(b))) = (&again.job.result, &r.ice_shedding.job.result) else {
                panic!("no result");
            };
            assert_eq!(a.report, b.report);
            assert_eq!(a.rules, b.rules);
            r.ice_shedding.open = false;
        }
        harness.run();
        let r = harness.state_mut().ready().unwrap();
        assert!(r.view3d.ice_shedding.report.is_none());
    }
}
