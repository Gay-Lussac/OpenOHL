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
    /// Only the rows beyond their limit.
    pub only_failures: bool,
    /// The structure and section overrides of its cases.
    pub overrides: crate::case_groups::Overrides,
    /// The plugin preset chosen in the dropdown (plugin, preset).
    plugin_preset: Option<(usize, usize)>,
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
            check: (self.overrides.structures || self.overrides.sections).then(|| {
                ohl_clearance::CheckScope {
                    structure_override: self.overrides.structures,
                    section_override: self.overrides.sections,
                    ..Default::default()
                }
            }),
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
        if let Some(why) = self.why_not_run() {
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
            [900.0, 640.0],
            &mut open,
            |ui| {
                help::body(ui, "string-checks-window", |ui| {
                    ui.label(
                        egui::RichText::new(
                            "Every string as solved, in the weather cases of each rule: its \
                             load against its own maximum load, its swing against its own \
                             swing limits and those a rule gives (from vertical, across and \
                             along the line, either side), and the suspension strings' \
                             uplift — going slack, or pushed beyond the compression they may \
                             take. A string's limits are its own (else its insulator's), set \
                             in its window or the Strings table; a design code's preset may \
                             give rules.",
                        )
                        .small()
                        .weak(),
                    );
                    self.presets_ui(ui, model, presets, &mut out);
                    changed |= self.rules_ui(ui, model);
                    if let Some((text, error)) = &self.status {
                        if *error {
                            crate::menu::error_label(ui, text.as_str());
                        } else {
                            ui.label(egui::RichText::new(text).small().weak());
                        }
                    }
                    ui.separator();
                    self.results_ui(ui, &stale, &mut run, &mut out);
                });
                help::footer(ui, "string-checks-window", |ui| {
                    changed |= self.overrides.ui(ui);
                    let why = self.why_not_run();
                    run |= help::run_button(
                        ui,
                        !self.job.running(),
                        "Solve each weather case of the rules and check every string",
                        why.as_deref(),
                    );
                    self.job.status_ui(ui);
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

    /// The plugin presets for this check (those with string rules, or
    /// declaring `checks = ["strings"]`): only their string rules loaded.
    fn presets_ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &mut Model,
        presets: &[crate::rule_editor::PluginPresets],
        out: &mut Out,
    ) {
        let part = crate::preset_cases::Part::Check(PresetCheck::Strings);
        ui.horizontal(|ui| {
            ui.label("Plugin preset");
            crate::rule_editor::preset_dropdown(
                ui,
                "string-checks-plugin-preset",
                presets,
                move |p| part.lists(&p.rules),
                &mut self.plugin_preset,
            );
            let chosen = self.plugin_preset.and_then(|(i, k)| {
                let pp = presets.get(i)?;
                Some((pp.plugin.clone(), pp.presets.as_ref().ok()?.get(k)?.clone()))
            });
            let Some((plugin, p)) = chosen else {
                return;
            };
            let (p, cases) = match crate::preset_cases::for_project(model, &p, part) {
                Ok((adapted, plan)) => (adapted, plan.lines()),
                Err(e) => {
                    self.status = Some((format!("preset {}: {e}", p.name), true));
                    return;
                }
            };
            let preview: Vec<String> = p.rules.string_rules.iter().map(rule_text).collect();
            if ui
                .button("Load")
                .on_hover_text(format!(
                    "{}\n{}\n{}\nIts string rules replace those listed; your weather cases of \
                     the same weather are used, the others created",
                    p.description,
                    preview.join("\n"),
                    cases.join("\n")
                ))
                .clicked()
            {
                self.rules = p.rules.string_rules.clone();
                self.save(model);
                let mut text = format!(
                    "preset {plugin} {}: {} string rule(s) loaded",
                    p.name,
                    p.rules.string_rules.len()
                );
                for line in &cases {
                    text = format!("{text}\n{line}");
                }
                self.status = Some((text, false));
                out.plugin_preset = Some((plugin, p));
            }
        });
    }

    /// The rules, one row each; whether one changed.
    fn rules_ui(&mut self, ui: &mut egui::Ui, model: &Model) -> bool {
        let mut changed = false;
        let cases: Vec<String> = model.cases.iter().map(|c| c.case.name.clone()).collect();
        let groups = crate::case_groups::groups(model);
        let mut voltages: Vec<f64> = ohl_clearance::case::voltages_of(&model.line)
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.map_or(0.0, |p| p.nominal_kv))
            .collect();
        voltages.sort_by(f64::total_cmp);
        voltages.dedup();
        let mut remove = None;
        egui::Grid::new("string-check-rules")
            .striped(true)
            .spacing(help::GRID_SPACING)
            .show(ui, |ui| {
                for h in [
                    "rule",
                    "weather cases",
                    "strings",
                    "voltages",
                    "load",
                    "own swing",
                    "uplift",
                    "rule's swing limits °",
                    "",
                ] {
                    ui.strong(h);
                }
                ui.end_row();
                for (k, r) in self.rules.iter_mut().enumerate() {
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut r.name).desired_width(160.0))
                        .lost_focus();
                    let shown = if r.cases.is_empty() {
                        "every case".to_owned()
                    } else {
                        r.cases.join(", ")
                    };
                    crate::menu::options_menu(ui, &shown, |ui| {
                        changed |= crate::case_groups::pick_names(ui, &groups, &mut r.cases);
                        changed |= ticks(ui, &cases, &mut r.cases);
                    });
                    egui::ComboBox::from_id_salt(("string-check-kind", k))
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
                    let shown = if r.voltages.is_empty() {
                        "every voltage".to_owned()
                    } else {
                        r.voltages
                            .iter()
                            .map(|v| format!("{v} kV"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    crate::menu::options_menu(ui, &shown, |ui| {
                        ui.label(
                            egui::RichText::new("None ticked: every voltage (0 kV: earth wires)")
                                .small()
                                .weak(),
                        );
                        for v in &voltages {
                            let mut on = r.voltages.iter().any(|x| (x - v).abs() < 1e-6);
                            if ui.checkbox(&mut on, format!("{v} kV")).changed() {
                                if on {
                                    r.voltages.push(*v);
                                } else {
                                    r.voltages.retain(|x| (x - v).abs() >= 1e-6);
                                }
                                changed = true;
                            }
                        }
                    });
                    changed |= ui
                        .checkbox(&mut r.load, "")
                        .on_hover_text("The strings' own maximum load")
                        .changed();
                    changed |= ui
                        .checkbox(&mut r.swing, "")
                        .on_hover_text("The strings' own swing limits")
                        .changed();
                    changed |= ui
                        .checkbox(&mut r.uplift, "")
                        .on_hover_text(
                            "Suspension strings going slack, or pushed beyond the compression \
                             they may take",
                        )
                        .changed();
                    ui.horizontal(|ui| {
                        for (label, v) in [
                            ("vertical", &mut r.max_swing_deg),
                            ("across", &mut r.max_swing_across_deg),
                            ("along", &mut r.max_swing_along_deg),
                        ] {
                            changed |= angle(ui, label, v);
                        }
                    });
                    if ui
                        .small_button("×")
                        .on_hover_text("Remove this rule")
                        .clicked()
                    {
                        remove = Some(k);
                    }
                    ui.end_row();
                }
            });
        if let Some(k) = remove {
            self.rules.remove(k);
            changed = true;
        }
        if ui
            .button("Add rule")
            .on_hover_text("A rule for every string in every case: their own limits and uplift")
            .clicked()
        {
            self.rules
                .push(StringRule::blank(&format!("rule {}", self.rules.len() + 1)));
            changed = true;
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
            ui.checkbox(&mut self.only_failures, "only those");
            if ui.button("Export CSV…").clicked() {
                save_csv("string-checks.csv", &report.report.csv(&report.rules));
            }
        });
        help::list(
            ui,
            "string-checks-list-1",
            500.0,
            egui::ScrollArea::vertical().auto_shrink([false, false]),
            |ui| {
                // Every row (those asked for), only those in view drawn
                // (2026-10-26).
                let rows: Vec<_> = report
                    .report
                    .rows
                    .iter()
                    .filter(|r| !self.only_failures || !r.pass)
                    .collect();
                let mut table = crate::rows_in_view::Table::new(ui, "string-check-rows", 10)
                    .spacing(help::GRID_SPACING[0], help::GRID_SPACING[1]);
                table.header(
                    ui,
                    &[
                        ("string", ""),
                        ("kind", ""),
                        ("rule", ""),
                        ("case", ""),
                        ("check", ""),
                        ("value", ""),
                        ("limit", ""),
                        ("from", ""),
                        ("usage %", ""),
                        ("", ""),
                    ],
                );
                table.rows(ui, rows.len(), None, |c, i| {
                    let r = rows[i];
                    if c.cell(|ui| {
                        ui.link(format!("{} {}", r.structure, r.string))
                            .on_hover_text(format!(
                                "Go to {} (section {}, point {})",
                                r.structure,
                                r.section,
                                r.point + 1
                            ))
                            .clicked()
                    }) {
                        out.go = Some(Selection::Structure(r.structure.clone()));
                    }
                    c.cell(|ui| ui.label(if r.strain { "strain" } else { "suspension" }));
                    let rule = &report.rules[r.rule];
                    c.cell(|ui| ui.label(&rule.name).on_hover_text(rule_text(rule)));
                    c.cell(|ui| ui.label(&r.case));
                    c.cell(|ui| ui.label(r.measure.describe()));
                    let (value, limit, usage) = match r.measure {
                        Measure::Load => (
                            format!("{:.2} kN", r.value / 1000.0),
                            format!("{:.2} kN", r.limit / 1000.0),
                            format!("{:.1}", 100.0 * r.usage),
                        ),
                        Measure::Uplift => (
                            format!("{:.2} kN", r.value / 1000.0),
                            if r.pass {
                                "hangs".to_owned()
                            } else if r.value < 0.0 {
                                "pushed".to_owned()
                            } else {
                                "slack".to_owned()
                            },
                            String::new(),
                        ),
                        _ => (
                            format!("{:.1}°", r.value),
                            format!("{:.1}°", r.limit),
                            format!("{:.1}", 100.0 * r.usage),
                        ),
                    };
                    c.cell(|ui| ui.label(value));
                    c.cell(|ui| ui.label(limit));
                    c.cell(|ui| {
                        ui.label(match r.source {
                            Source::String => "string",
                            Source::Rule => "rule",
                        })
                    });
                    let u = egui::RichText::new(usage).monospace();
                    if r.pass {
                        c.cell(|ui| ui.label(u));
                        c.cell(|ui| ui.label("pass"));
                    } else {
                        c.cell(|ui| ui.label(u.color(ui.visuals().error_fg_color)));
                        c.cell(|ui| {
                            ui.label(egui::RichText::new("fail").color(ui.visuals().error_fg_color))
                        });
                    }
                });
            },
        );
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
