// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › **String checks…** (owner, 2026-10-24): every string as
//! solved, by rules ([`ohl_clearance::strings`]) — its load against its
//! own maximum load, its swing against its own swing limits and the
//! rule's, and the suspension strings' uplift (going slack, or pushed
//! beyond their compression limit). Each rule: its weather cases, its
//! strings (suspension, strain or all; the voltages of their sections)
//! and what it checks. The rules are kept in the project as a rule file
//! (`<model>.string-rules.ohlr`, `[[string_rule]]` tables); a design-code
//! plugin's preset brings its string rules (only those). Run solves each
//! case of the rules in the background and lists every limit of every
//! string, failures first; the strings beyond a limit (else the worst)
//! are marked in the views while the window is open.

use std::path::PathBuf;

use eframe::egui;
use ohl_clearance::rules::{PresetCheck, RuleSet};
use ohl_clearance::strings::{self, Measure, Source, StringKind, StringReport, StringRule};

use crate::check_help as help;
use crate::check_ui;
use crate::job::{Job, save_csv};
use crate::model::Model;
use crate::selection::Selection;

/// What its results are computed from ([`crate::revision`]).
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::LINE;

/// A check's answer: the report and the rules it was made with.
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// The rows and notes.
    pub report: StringReport,
    /// The rules checked.
    pub rules: Vec<StringRule>,
}

/// What the window asks of the app.
#[derive(Default)]
pub struct Out {
    /// Go to a structure.
    pub go: Option<Selection>,
    /// New marks to draw (a new report, or the window opened again);
    /// `Some(empty)`: take them off.
    pub marks: Option<Vec<[f64; 3]>>,
    /// A design-code plugin's preset loaded (its plugin, the preset): its
    /// weather cases missing from the model to add.
    pub plugin_preset: Option<(String, ohl_io::codes::RulePresetChecked)>,
}

/// The String checks window.
#[derive(Default)]
pub struct StringChecksWindow {
    /// Shown.
    pub open: bool,
    /// The project's rule file read.
    loaded: bool,
    /// The rule file kept.
    pub file: Option<PathBuf>,
    /// The rules.
    pub rules: Vec<StringRule>,
    /// What was done last, in words (an error: true).
    pub status: Option<(String, bool)>,
    /// The structure and section overrides of its cases.
    pub overrides: crate::case_groups::Overrides,
    /// What it checks: the circuits, routes and sections (owner,
    /// 2026-10-28: the scope of every check window), kept in its rule
    /// file's `[check]`.
    pub scope: check_ui::Scope,
    /// The check.
    pub(crate) job: Job<Report>,
    /// Counted up with each answer.
    generation: u64,
    /// What the views draw of it: (shown, generation).
    drawn: Option<(bool, u64)>,
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
    dir.join(format!("{stem}.string-rules.ohlr"))
}

impl StringChecksWindow {
    /// Opens the window; the first time, reads the project's rule file
    /// (its string rules), else starts with one rule checking the
    /// strings' own limits and uplift in every case.
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
                    self.rules = set.string_rules;
                    // The overrides ticked, from its [check].
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
        } else {
            self.rules = vec![StringRule::blank("the strings' own limits")];
        }
    }

    /// Why the check cannot run (none: it can).
    pub fn why_not_run(&self) -> Option<String> {
        if self.rules.is_empty() {
            return Some("no rule: add one, or load a preset".into());
        }
        let why: Vec<String> = self.rules.iter().filter_map(StringRule::problem).collect();
        (!why.is_empty()).then(|| why.join("; "))
    }

    /// The rules as a rule file.
    pub fn rule_set(&self) -> RuleSet {
        RuleSet {
            name: "String checks".into(),
            rules: Vec::new(),
            wind: None,
            // The overrides ticked.
            // The scope and the overrides ticked.
            check: check_ui::check_of(&self.scope, self.overrides),
            vegetation: None,
            checks: vec![PresetCheck::Strings],
            tension_rules: Vec::new(),
            string_rules: self.rules.clone(),
            internal_rules: Vec::new(),
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

    /// Starts a check: each weather case of the rules solved in the
    /// background (as the line is), every string checked.
    pub fn start(&mut self, model: &Model, ctx: &egui::Context) {
        if let Some(why) = self
            .why_not_run()
            .or_else(|| self.scope.why_not(&model.line))
        {
            self.job.result = Some(Err(format!("not run: {why}")));
            return;
        }
        let cases: Vec<ohl_weather::WeatherCase> = model
            .cases
            .iter()
            .map(|c| c.case.clone())
            .filter(|c| self.rules.iter().any(|r| r.applies(&c.name)))
            .collect();
        if cases.is_empty() {
            self.job.result = Some(Err(
                "none of the rules' weather cases is in the model".into()
            ));
            return;
        }
        let line = model.line.clone();
        let library = model.library.clone();
        let rules = self.rules.clone();
        let filter = self.overrides.filter(model);
        // The scope's sections only (none: the whole line).
        let keep = self.scope.keeps(&model.line);
        self.job.start_at(ctx, model.revision(), move || {
            let mut solved = Vec::new();
            let mut notes = Vec::new();
            for case in &cases {
                let mut shapes = Vec::new();
                for (section, r) in
                    line.sections
                        .iter()
                        .zip(ohl_line::solve::solve_line_memo_default(
                            &line, &library, case,
                        ))
                {
                    match r {
                        Ok(s) if keep.as_ref().is_some_and(|k| !k.contains(&s.section)) => {}
                        Ok(s) => shapes.push(s),
                        Err(e) => notes.push(format!(
                            "\"{}\": section {} not solved: {e}",
                            case.name, section.id
                        )),
                    }
                }
                solved.push((case.name.clone(), shapes));
            }
            let views: Vec<(&str, &[ohl_line::solve::SectionShape])> = solved
                .iter()
                .map(|(n, s)| (n.as_str(), s.as_slice()))
                .collect();
            let mut report = strings::check(&line, &views, &rules, &filter);
            report.notes.splice(0..0, notes);
            Ok(Report { report, rules })
        });
    }

    /// What the last check was computed from that changed since, in
    /// words (`None`: nothing, or no report).
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        matches!(self.job.result, Some(Ok(_)))
            .then(|| self.job.stamp.out_of_date(model, USES))
            .flatten()
    }

    /// The marks to draw when what the views should show changed: drawn
    /// while the window is open, taken off once it is closed (`Some(empty)`).
    fn marks_change(&mut self) -> Option<Vec<[f64; 3]>> {
        let report = match &self.job.result {
            Some(Ok(r)) if self.open => Some(r),
            _ => None,
        };
        let key = (report.is_some(), self.generation);
        if self.drawn == Some(key) {
            return None;
        }
        let first = self.drawn.is_none();
        self.drawn = Some(key);
        match report {
            Some(r) => Some(r.report.marks()),
            None if first => None,
            None => Some(Vec::new()),
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
        let was_running = self.job.running();
        self.job.poll();
        if was_running && !self.job.running() {
            // A new answer: drawn anew.
            self.generation += 1;
        }
        if !self.open {
            out.marks = self.marks_change();
            return out;
        }
        let mut open = true;
        let mut run = false;
        let mut changed = false;
        let stale = self.out_of_date(model);
        help::window(
            ctx,
            crate::dockable::Panel::StringChecks,
            "String checks",
            [860.0, 760.0],
            &mut open,
            |ui| {
                let why = self
                    .why_not_run()
                    .or_else(|| self.scope.why_not(&model.line));
                let scope_words = check_ui::scope_words(&model.line, &self.scope, true);
                let findings = match &self.job.result {
                    Some(Ok(r)) => Some(r.report.failures().count()),
                    _ => None,
                };
                // Two tabs (owner, 2026-10-27): the setup scrolls, the
                // results fill theirs; Run in the footer, always visible.
                let tab = check_ui::tabs(
                    ui,
                    "string-checks-window",
                    findings,
                    self.job.generation,
                    why.is_some() && !self.job.running(),
                );
                match tab {
                    check_ui::Tab::Setup => help::body(ui, "string-checks-window", |ui| {
                        check_ui::section(ui, 1, "What it checks", None);
                        help::explain(
                            ui,
                            "Every string as solved, per weather case: its load and swing against its limits, the \
                             suspension strings' uplift.",
                            "None ticked: every voltage (0 kV: earth wires)",
                        );
                        ui.separator();
                        check_ui::section(ui, 2, "Scope", None);
                        if check_ui::ScopePicker::new("string-checks-scope", &model.line)
                            .wires(&mut self.scope, true)
                            .ui(ui)
                        {
                            changed = true;
                        }
                        ui.label(
                            egui::RichText::new(
                                "Each rule gives its own strings, voltages and weather cases besides (3 Rules).",
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
                    check_ui::Tab::Results => {
                        check_ui::results_area(ui, "string-checks-window", |ui| {
                            self.results_ui(ui, &stale, &mut run, &mut out);
                        })
                    }
                }
                help::footer(ui, "string-checks-window", |ui| {
                    let overrides = &mut self.overrides;
                    run |= check_ui::job_footer(
                        ui,
                        &mut self.job,
                        why.clone(),
                        "Solve each weather case of the rules and check every string",
                        Some(scope_words),
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
        out.marks = self.marks_change();
        out
    }

    /// The rules in the shared rule table (owner, 2026-10-28): one row
    /// each, the selected one's fields below it, the presets for this
    /// check (only their string rules), Import / Export; whether one
    /// changed.
    fn rules_ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        presets: &[crate::rule_editor::PluginPresets],
        out: &mut Out,
    ) -> bool {
        let part = crate::preset_cases::Part::Check(PresetCheck::Strings);
        let cases: Vec<String> = model.cases.iter().map(|c| c.case.name.clone()).collect();
        let groups = crate::case_groups::groups(model);
        let mut voltages: Vec<f64> = ohl_clearance::case::voltages_of(&model.line)
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.map_or(0.0, |p| p.nominal_kv))
            .collect();
        voltages.sort_by(f64::total_cmp);
        voltages.dedup();
        let t = check_ui::RuleTable::new("string-check-rules", &mut self.rules, &voltages)
            .add(
                "A string rule",
                "for every string in every case: their own limits and uplift",
                |n| StringRule::blank(&format!("rule {}", n + 1)),
            )
            .presets(model, presets, part, |set| {
                set.string_rules.iter().map(rule_text).collect()
            })
            .ui(ui, |ui, _, r| {
                let mut changed = false;
                egui::Grid::new("string-check-rule")
                    .num_columns(2)
                    .spacing(help::GRID_SPACING)
                    .show(ui, |ui| {
                        ui.label("Weather cases");
                        crate::menu::options_menu(
                            ui,
                            &crate::rule_table::cases_words(&r.cases),
                            |ui| {
                                changed |=
                                    crate::case_groups::pick_names(ui, &groups, &mut r.cases);
                                changed |= ticks(ui, &cases, &mut r.cases);
                            },
                        );
                        ui.end_row();
                        ui.label("Strings");
                        egui::ComboBox::from_id_salt("string-check-kind")
                            .selected_text(r.strings.describe())
                            .show_ui(ui, |ui| {
                                for kind in
                                    [StringKind::All, StringKind::Suspension, StringKind::Strain]
                                {
                                    changed |= ui
                                        .selectable_value(&mut r.strings, kind, kind.describe())
                                        .changed();
                                }
                            });
                        ui.end_row();
                        ui.label("Voltages");
                        changed |= crate::rule_table::voltages_menu(
                            ui,
                            &voltages,
                            &mut r.voltages,
                            "None ticked: every voltage (0 kV: earth wires)",
                        );
                        ui.end_row();
                        ui.label("Checks");
                        ui.horizontal_wrapped(|ui| {
                            changed |= ui
                                .checkbox(&mut r.load, "load")
                                .on_hover_text("The strings' own maximum load")
                                .changed();
                            changed |= ui
                                .checkbox(&mut r.swing, "own swing")
                                .on_hover_text("The strings' own swing limits")
                                .changed();
                            changed |= ui
                                .checkbox(&mut r.uplift, "uplift")
                                .on_hover_text(
                                    "Suspension strings going slack, or pushed beyond the \
                                     compression they may take",
                                )
                                .changed();
                        });
                        ui.end_row();
                        ui.label("Rule's swing limits");
                        ui.horizontal_wrapped(|ui| {
                            for (label, v) in [
                                ("vertical", &mut r.max_swing_deg),
                                ("across", &mut r.max_swing_across_deg),
                                ("along", &mut r.max_swing_along_deg),
                            ] {
                                changed |= angle(ui, label, v);
                            }
                        });
                        ui.end_row();
                    });
                changed
            });
        let mut changed = t.changed;
        if let Some(pick) = t.preset {
            match pick.taken(model, part) {
                Ok((p, lines)) => {
                    let done = crate::rule_table::merge(
                        &mut self.rules,
                        p.rules.string_rules.clone(),
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
                    self.rules = set.string_rules;
                    changed = true;
                }
                Err(e) => self.status = Some((e, true)),
            }
        }
        if t.export
            && let Some(p) = crate::rule_table::export_file("string-rules.ohlr")
        {
            self.status = Some(match self.rule_set().save(&p) {
                Ok(()) => (format!("exported to {}", p.display()), false),
                Err(e) => (e, true),
            });
        }
        changed
    }

    /// The results: every limit of every string, failures first.
    fn results_ui(
        &mut self,
        ui: &mut egui::Ui,
        stale: &Option<String>,
        run: &mut bool,
        out: &mut Out,
    ) {
        let report = match &self.job.result {
            None => {
                ui.label("Run to check every string.");
                return;
            }
            Some(Err(e)) => {
                crate::menu::error_label(ui, e.as_str());
                return;
            }
            Some(Ok(r)) => r,
        };
        if let Some(what) = stale {
            *run |= crate::revision::banner(ui, what, !self.job.running());
        }
        for n in &report.report.notes {
            ui.label(egui::RichText::new(n).small().weak());
        }
        let failed = report.report.failures().count();
        ui.horizontal(|ui| {
            ui.label(format!(
                "{} limits × strings × cases checked, {failed} beyond",
                report.report.rows.len()
            ));
            if ui.button("Export CSV…").clicked() {
                save_csv("string-checks.csv", &report.report.csv(&report.rules));
            }
        });
        // Every row, sorted, filtered, grouped by the user; a row clicked
        // goes to its structure.
        use check_ui::{Cell, Col};
        let rows: Vec<check_ui::Row> = report
            .report
            .rows
            .iter()
            .map(|r| {
                let rule = &report.rules[r.rule];
                let (value, limit, usage) = match r.measure {
                    Measure::Load => (
                        Cell::sorted(format!("{:.2} kN", r.value / 1000.0), r.value),
                        Cell::sorted(format!("{:.2} kN", r.limit / 1000.0), r.limit),
                        Cell::num(Some(100.0 * r.usage), 1),
                    ),
                    Measure::Uplift => (
                        Cell::sorted(format!("{:.2} kN", r.value / 1000.0), r.value),
                        Cell::text(if r.pass {
                            "hangs"
                        } else if r.value < 0.0 {
                            "pushed"
                        } else {
                            "slack"
                        }),
                        Cell::num(None, 1),
                    ),
                    _ => (
                        Cell::sorted(format!("{:.1}°", r.value), r.value),
                        Cell::sorted(format!("{:.1}°", r.limit), r.limit),
                        Cell::num(Some(100.0 * r.usage), 1),
                    ),
                };
                check_ui::Row::new(
                    vec![
                        Cell::text(format!("{} {}", r.structure, r.string)),
                        Cell::text(if r.strain { "strain" } else { "suspension" }),
                        Cell::text(&rule.name),
                        Cell::text(&r.case),
                        Cell::text(r.measure.describe()),
                        value.bad(!r.pass),
                        limit,
                        Cell::text(match r.source {
                            Source::String => "string",
                            Source::Rule => "rule",
                        }),
                        usage.bad(!r.pass),
                        Cell::text(if r.pass { "pass" } else { "fail" }).bad(!r.pass),
                    ],
                    !r.pass,
                )
            })
            .collect();
        if let Some(k) = check_ui::ResultTable::new(
            "string-check-rows",
            vec![
                Col::text("String"),
                Col::text("Kind"),
                Col::text("Rule"),
                Col::text("Case"),
                Col::text("Check"),
                Col::num("Value"),
                Col::num("Limit"),
                Col::text("From"),
                Col::num("Usage [%]"),
                Col::text("Verdict"),
            ],
        )
        .rows(rows)
        .group_by(&[2, 3, 0, 4])
        .csv("string-checks-table.csv")
        .ui(ui)
        {
            out.go = Some(Selection::Structure(
                report.report.rows[k].structure.clone(),
            ));
        }
    }
}

/// An optional angle typed in, degrees (empty: none); whether it changed.
fn angle(ui: &mut egui::Ui, label: &str, v: &mut Option<f64>) -> bool {
    let mut on = v.is_some();
    let mut changed = false;
    if ui.checkbox(&mut on, label).changed() {
        *v = on.then_some(v.unwrap_or(45.0));
        changed = true;
    }
    if let Some(x) = v.as_mut() {
        let r = crate::style::field(
            ui,
            egui::DragValue::new(x)
                .range(0.1..=180.0)
                .speed(0.5)
                .suffix("°"),
        );
        changed |= r.drag_stopped() || r.lost_focus();
    }
    changed
}

/// A rule in words: "wind: suspension strings, every voltage, cases
/// wind*; load, swing (the strings' own; ≤ 60° across), uplift".
pub fn rule_text(r: &StringRule) -> String {
    format!(
        "{}: {}, {}, {}; {}",
        r.name,
        r.strings.describe(),
        if r.voltages.is_empty() {
            "every voltage".to_owned()
        } else {
            r.voltages
                .iter()
                .map(|v| format!("{v} kV"))
                .collect::<Vec<_>>()
                .join(", ")
        },
        if r.cases.is_empty() {
            "every case".to_owned()
        } else {
            format!("cases {}", r.cases.join(", "))
        },
        r.describe_checks()
    )
}

/// A string rule as a row of the shared rule table.
impl check_ui::RuleRow for StringRule {
    fn name(&self) -> &str {
        &self.name
    }

    fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }

    fn summary(&self, _: &[f64]) -> check_ui::Summary {
        check_ui::Summary {
            target: self.strings.describe().to_owned(),
            voltages: crate::rule_table::voltages_words(&self.voltages),
            cases: crate::rule_table::cases_words(&self.cases),
            distance: self.describe_checks(),
            wires: "strings".into(),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    /// Checks › String checks: opened from the menu with one rule (the
    /// strings' own limits and uplift); an INVENTED rule of 0.1° across
    /// the line (not from any code) fails the suspension strings at
    /// angles; Run lists the rows, the failures marked; the rules kept in
    /// the rule file and read back; closed, the marks go.
    #[test]
    fn the_window_runs_marks_and_keeps_its_rules() {
        let model = crate::tests::synthetic();
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 1200.0))
            .build_eframe(|_| crate::app::App::with_model(model, None));
        harness.run();
        harness.get_by_label("Checks").click();
        harness.run();
        harness.get_by_label("String checks…").click();
        harness.run();
        let dir = std::env::temp_dir().join(format!("ohl-app-{}-strings", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.string-rules.ohlr");
        {
            let r = harness.state_mut().ready().unwrap();
            let w = &mut r.string_checks;
            assert!(w.open);
            assert_eq!(w.rules, vec![StringRule::blank("the strings' own limits")]);
            w.file = Some(file.clone());
            w.rules.push(StringRule {
                strings: StringKind::Suspension,
                max_swing_across_deg: Some(0.1),
                ..StringRule::blank("invented 0.1° across")
            });
            w.save(&mut r.model);
        }
        let back = RuleSet::load(&file).unwrap();
        assert_eq!(back.string_rules.len(), 2);
        assert_eq!(back.checks, [PresetCheck::Strings]);
        harness.run();
        harness.get_by_label("Run").click();
        let started = std::time::Instant::now();
        loop {
            harness.step();
            let r = harness.state_mut().ready().unwrap();
            if let Some(Ok(rep)) = &r.string_checks.job.result {
                assert!(rep.report.failures().count() > 0);
                assert!(
                    rep.report
                        .failures()
                        .all(|x| x.rule == 1 && x.measure == Measure::SwingAcross),
                    "{:?}",
                    rep.report.failures().next()
                );
                break;
            }
            assert!(started.elapsed().as_secs() < 120, "no report in 120 s");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        harness.run();
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.marks_from, crate::app::MarksFrom::Strings);
        assert!(!r.view3d.marks.is_empty());
        r.string_checks.open = false;
        harness.run();
        let r = harness.state_mut().ready().unwrap();
        assert!(r.view3d.marks.is_empty() && r.plan.marks.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
