// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › **Wire tensions…** (owner, 2026-10-22): the largest tension a
//! wire may carry, by rules ([`ohl_clearance::tension`]). Each rule: its
//! wires (wire types, systems, circuits; none: every wire), its weather
//! cases, a factor of the wire type's RTS (the allowed tension), and the
//! tension compared (the maximum, at the span's most tensioned support,
//! by default; or the horizontal). The rules are kept in the project as a
//! rule file (`<model>.tension-rules.ohlr`, `[[tension_rule]]` tables); a
//! design-code plugin's preset brings its tension rules (only those).
//! Run solves each case of the rules in the background and lists every
//! span: tension, allowed, usage, pass or fail, the largest usage first;
//! the spans above their allowed tension (else the worst) are marked in
//! the views while the window is open.

use std::path::PathBuf;

use eframe::egui;
use ohl_clearance::rules::{PresetCheck, RuleSet};
use ohl_clearance::tension::{self, TensionMeasure, TensionReport, TensionRule};

use crate::check_help as help;
use crate::check_ui;
use crate::job::Job;
use crate::model::Model;
use crate::selection::Selection;

/// What its results are computed from ([`crate::revision`]).
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::LINE;

/// Its **Use local conditions**: the wind taken, the check's name for
/// the run times, what the wind is for.
const LOCAL: (ohl_line::local::Purpose, &str, &str) = (
    ohl_line::local::Purpose::Strength,
    "wire-tensions",
    "the wires' tensions",
);

/// A check's answer: the report and the rules it was made with.
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// The rows and notes.
    pub report: TensionReport,
    /// The rules checked.
    pub rules: Vec<TensionRule>,
}

/// What the window asks of the app.
#[derive(Default)]
pub struct Out {
    /// Go to a span.
    pub go: Option<Selection>,
    /// The weather case of the row clicked: the span drawn in it
    /// ([`crate::result_case`]).
    pub case: Option<String>,
    /// New marks to draw (a new report, or the window opened again);
    /// `Some(empty)`: take them off.
    pub marks: Option<Vec<[f64; 3]>>,
    /// A design-code plugin's preset loaded (its plugin, the preset): its
    /// weather cases missing from the model to add.
    pub plugin_preset: Option<(String, ohl_io::codes::RulePresetChecked)>,
}

/// The Wire tensions window.
#[derive(Default)]
pub struct WireTensionsWindow {
    /// Shown.
    pub open: bool,
    /// **Use local conditions** (owner, 2026-11-03): the tensions with
    /// the structures' wind for strength and wire ice on their spans.
    pub local: bool,
    /// What it checks: the circuits, routes and sections (owner,
    /// 2026-10-28: the scope of every check window), kept in its rule
    /// file's `[check]`.
    pub scope: check_ui::Scope,
    /// The project's rule file read.
    loaded: bool,
    /// The rule file kept.
    pub file: Option<PathBuf>,
    /// The rules.
    pub rules: Vec<TensionRule>,
    /// What was done last, in words (an error: true).
    pub status: Option<(String, bool)>,
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
    dir.join(format!("{stem}.tension-rules.ohlr"))
}

impl WireTensionsWindow {
    /// Opens the window; the first time, reads the project's rule file
    /// (its tension rules).
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
                    self.rules = set.tension_rules;
                    // Use local conditions, from its [check].
                    if let Some(c) = set.check {
                        self.local = c.local;
                        self.scope = check_ui::Scope::of_check(&c);
                    }
                }
                Err(e) => self.status = Some((e, true)),
            }
        }
    }

    /// Why the check cannot run (none: it can).
    pub fn why_not_run(&self) -> Option<String> {
        if self.rules.is_empty() {
            return Some("no rule: add one, or load a preset".into());
        }
        let why: Vec<String> = self.rules.iter().filter_map(TensionRule::problem).collect();
        (!why.is_empty()).then(|| why.join("; "))
    }

    /// The rules as a rule file.
    pub fn rule_set(&self) -> RuleSet {
        RuleSet {
            name: "Wire tensions".into(),
            rules: Vec::new(),
            wind: None,
            // The scope and Use local conditions.
            check: check_ui::check_of(&self.scope, self.local),
            vegetation: None,
            checks: vec![PresetCheck::Tension],
            tension_rules: self.rules.clone(),
            string_rules: Vec::new(),
            internal_rules: Vec::new(),
            lightning_rules: Vec::new(),
            ice_shedding_rules: Vec::new(),
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
    /// background (as the line is), every span checked.
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
        // The line with the local conditions when ticked (the wind for
        // strength).
        let local = crate::local_solve::prepare(&model.line, self.local, LOCAL.0);
        let library = model.library.clone();
        let rules = self.rules.clone();
        // The scope's sections only (none: the whole line).
        let keep = self.scope.keeps(&model.line);
        self.job.start_at(ctx, model.revision(), move || {
            let started = std::time::Instant::now();
            let line = local.line();
            let mut solved = Vec::new();
            let mut notes = local.notes();
            // Each case once per calculation its rules ask (2026-11-01).
            let names: Vec<&str> = cases.iter().map(|c| c.name.as_str()).collect();
            let plan = ohl_clearance::calculation::solves(
                &names,
                &rules,
                |r, c| r.applies(c),
                |r| r.calculation,
            );
            for (k, calc) in plan {
                let case = &cases[k];
                let line = ohl_clearance::calculation::line_for(line, calc);
                let mut shapes = Vec::new();
                for (section, r) in line
                    .sections
                    .iter()
                    .zip(local.sections(&line, &library, case))
                {
                    match r {
                        Ok(s) if keep.as_ref().is_some_and(|k| !k.contains(&s.section)) => {}
                        Ok(s) => shapes.push(s),
                        Err(e) => notes.push(format!(
                            "\"{}\": section {} not solved: {e}",
                            ohl_clearance::calculation::label(&case.name, calc),
                            section.id
                        )),
                    }
                }
                solved.push((case.name.clone(), calc, shapes));
            }
            let views: Vec<(
                &str,
                ohl_clearance::calculation::Calc,
                &[ohl_line::solve::SectionShape],
            )> = solved
                .iter()
                .map(|(n, c, s)| (n.as_str(), *c, s.as_slice()))
                .collect();
            let mut report = tension::check(line, &library, &views, &rules, &Default::default());
            report.notes.splice(0..0, notes);
            local.finish(LOCAL.1, started.elapsed());
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
            crate::dockable::Panel::WireTensions,
            "Wire tensions",
            [860.0, 760.0],
            &mut open,
            |ui| {
                let why = self
                    .why_not_run()
                    .or_else(|| self.scope.why_not(&model.line));
                let scope_words = check_ui::scope_words(&model.line, &self.scope, true);
                let rule_cases = model
                    .cases
                    .iter()
                    .filter(|c| self.rules.iter().any(|r| r.applies(&c.case.name)))
                    .count();
                let findings = match &self.job.result {
                    Some(Ok(r)) => Some(r.report.failures().count()),
                    _ => None,
                };
                // Two tabs (owner, 2026-10-27): the setup scrolls, the
                // results fill theirs; Run in the footer, always visible.
                let tab = check_ui::tabs(
                    ui,
                    "wire-tensions-window",
                    findings,
                    self.job.generation,
                    why.is_some() && !self.job.running(),
                );
                match tab {
                    check_ui::Tab::Setup => help::body(ui, "wire-tensions-window", |ui| {
                        check_ui::section(ui, 1, "What it checks", None);
                        help::explain(
                            ui,
                            "The tension of every span against the largest a rule allows: a factor of the wire \
                             type's rated tensile strength (RTS), per weather case.",
                            "Run solves the line in each weather case of the rules. In every span \
                             of the wires a rule applies to, the tension (by default the larger \
                             end tension, at the most tensioned support; or the horizontal \
                             tension, as the rule says) is compared with the rule's factor × the \
                             wire type's RTS. The usage is tension / allowed; above 100 % the span \
                             fails. No factor is built in: the rules' factors are yours or a \
                             design-code plugin's preset.",
                        );
                        check_ui::how_computed(
                            ui,
                            "wire-tensions-window",
                            "The wires solved in each weather case of the rules; per span the tension at its most tensioned support (or the horizontal tension, as the rule says) against the rule's factor × the wire type's RTS.",
                            help::tension_check,
                        );
                        ui.separator();
                        check_ui::section(ui, 2, "Scope", None);
                        if check_ui::ScopePicker::new("wire-tensions-scope", &model.line)
                            .route_of(model)
                            .wires(&mut self.scope, true)
                            .ui(ui)
                        {
                            changed = true;
                        }
                        ui.label(
                            egui::RichText::new(
                                "Each rule gives its own wires and weather cases besides (3 Rules).",
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
                        check_ui::results_area(ui, "wire-tensions-window", |ui| {
                            self.results_ui(ui, &stale, &mut run, &mut out);
                        })
                    }
                }
                help::footer(ui, "wire-tensions-window", |ui| {
                    let local = &mut self.local;
                    run |= check_ui::job_footer(
                        ui,
                        &mut self.job,
                        why.clone(),
                        "Solve each weather case of the rules and check the tension of every span against its allowed tension",
                        Some(scope_words),
                        Some(check_ui::scope_plan(&model.line, &self.scope, rule_cases)),
                        |ui| {
                            changed |= crate::local_solve::switch(
                                ui,
                                local,
                                LOCAL.1,
                                LOCAL.0,
                                LOCAL.2,
                                &model.line,
                            );
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
    /// check (only their tension rules), Import / Export; whether one
    /// changed.
    fn rules_ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        presets: &[crate::rule_editor::PluginPresets],
        out: &mut Out,
    ) -> bool {
        let part = crate::preset_cases::Part::Check(PresetCheck::Tension);
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
        systems.push("none".into());
        let circuits: Vec<String> = model.line.circuits.iter().map(|c| c.name.clone()).collect();
        let t = check_ui::RuleTable::new("wire-tension-rules", &mut self.rules, &[])
            .add(
                "A tension rule",
                "for every wire in every case; set its factor of the RTS",
                |n| TensionRule::blank(&format!("rule {}", n + 1)),
            )
            .presets(model, presets, part, |set| {
                set.tension_rules.iter().map(rule_text).collect()
            })
            .ui(ui, |ui, _, r| {
                let mut changed = false;
                egui::Grid::new("wire-tension-rule")
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
                        // How the wires are solved (owner, 2026-11-01).
                        ui.label("Calculation");
                        changed |= crate::rule_table::calculation_combo(
                            ui,
                            "tension-rule-calc",
                            &mut r.calculation,
                        );
                        ui.end_row();
                        ui.label("Factor of RTS");
                        ui.horizontal(|ui| {
                            let mut f = r.factor;
                            let response = crate::style::field(
                                ui,
                                egui::DragValue::new(&mut f)
                                    .range(0.0..=1.0)
                                    .speed(0.005)
                                    .max_decimals(3),
                            );
                            if response.changed() {
                                r.factor = f;
                            }
                            changed |= response.drag_stopped() || response.lost_focus();
                            if r.factor <= 0.0 {
                                ui.label(
                                    egui::RichText::new("set the factor")
                                        .color(ui.visuals().warn_fg_color),
                                );
                            }
                        });
                        ui.end_row();
                        ui.label("Tension");
                        egui::ComboBox::from_id_salt("wire-tension-measure")
                            .selected_text(r.measure.describe())
                            .show_ui(ui, |ui| {
                                for m in [TensionMeasure::Max, TensionMeasure::Horizontal] {
                                    changed |= ui
                                        .selectable_value(&mut r.measure, m, m.describe())
                                        .changed();
                                }
                            });
                        ui.end_row();
                        ui.label("Wires");
                        crate::menu::options_menu(ui, &r.targets.describe(), |ui| {
                            ui.label(
                                egui::RichText::new(
                                    "None ticked: every wire; a wire counts when it is in every \
                                     list ticked",
                                )
                                .small()
                                .weak(),
                            );
                            ui.label(egui::RichText::new("Wire types").strong());
                            changed |= ticks(ui, &types, &mut r.targets.wire_types);
                            ui.label(egui::RichText::new("Electrical systems").strong());
                            changed |= ticks(ui, &systems, &mut r.targets.systems);
                            ui.label(egui::RichText::new("Circuits").strong());
                            changed |= ticks(ui, &circuits, &mut r.targets.circuits);
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
                        p.rules.tension_rules.clone(),
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
                    self.rules = set.tension_rules;
                    changed = true;
                }
                Err(e) => self.status = Some((e, true)),
            }
        }
        if t.export
            && let Some(p) = crate::rule_table::export_file("tension-rules.ohlr")
        {
            self.status = Some(match self.rule_set().save(&p) {
                Ok(()) => (format!("exported to {}", p.display()), false),
                Err(e) => (e, true),
            });
        }
        changed
    }

    /// The results: every span, the largest usage first.
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
            Some(Ok(r)) => r,
        };
        if let Some(what) = stale {
            *run |= crate::revision::banner(ui, what, !self.job.running());
        }
        // The verdict (the shared banner), first: green when every span is
        // within its allowed tension, red with the highest usage.
        let failed = report.report.failures().count();
        let worst = report
            .report
            .rows
            .iter()
            .max_by(|a, b| a.usage.total_cmp(&b.usage));
        let text = match worst {
            Some(r) if failed > 0 => format!(
                "✖ {failed} span{} × case above the allowed tension — worst {}: {:.1} kN of \
                 {:.1} kN ({:.0} %), rule {}, case {}",
                if failed == 1 { "" } else { "s" },
                Cell::span(&r.section, r.span).text,
                r.tension / 1000.0,
                r.allowed / 1000.0,
                100.0 * r.usage,
                report.rules[r.rule].name,
                r.case
            ),
            Some(r) => format!(
                "✔ Every span within its allowed tension ({} spans × cases × rules; highest usage \
                 {:.0} %)",
                report.report.rows.len(),
                100.0 * r.usage
            ),
            None => "✖ Nothing checked: no span takes a rule's weather case".to_owned(),
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
        for n in &report.report.notes {
            ui.label(egui::RichText::new(n).small().weak());
        }
        // Every row, sorted, filtered, grouped by the user; a row clicked
        // selects its span and draws it in the row's weather case.
        let rows: Vec<check_ui::Row> = report
            .report
            .rows
            .iter()
            .map(|r| {
                let rule = &report.rules[r.rule];
                check_ui::Row::new(
                    vec![
                        Cell::span(&r.section, r.span),
                        Cell::text(&rule.name),
                        Cell::text(ohl_clearance::calculation::label(&r.case, r.calculation)),
                        Cell::num(Some(r.tension / 1000.0), 2).bad(!r.passes()),
                        Cell::num(Some(r.allowed / 1000.0), 2),
                        Cell::num(Some(100.0 * r.usage), 1).bad(!r.passes()),
                        Cell::text(if r.passes() { "pass" } else { "fail" }).bad(!r.passes()),
                    ],
                    !r.passes(),
                )
            })
            .collect();
        if let Some(k) = check_ui::ResultTable::new(
            "wire-tension-rows",
            vec![
                Col::text("Span"),
                Col::text("Rule"),
                Col::text("Case"),
                Col::num("Tension [kN]"),
                Col::num("Allowed [kN]"),
                Col::num("Usage [%]"),
                Col::text("Verdict"),
            ],
        )
        .rows(rows)
        .group_by(&[1, 2, 0])
        .csv("wire-tensions-table.csv")
        .full_csv(
            "wire-tensions.csv",
            "Every row with every field (wire type, RTS, factor, tensions in N, the point), as \
             ohl tensions writes it",
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
    }
}

/// A rule in words: "ice: 0.5 × RTS, maximum (most tensioned support),
/// cases ice; every wire".
pub fn rule_text(r: &TensionRule) -> String {
    format!(
        "{}: {} × RTS, {}, {}; {}",
        r.name,
        r.factor,
        r.measure.describe(),
        if r.cases.is_empty() {
            "every case".to_owned()
        } else {
            format!("cases {}", r.cases.join(", "))
        },
        r.targets.describe()
    )
}

/// A tension rule as a row of the shared rule table.
impl check_ui::RuleRow for TensionRule {
    fn name(&self) -> &str {
        &self.name
    }

    fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }

    fn summary(&self, _: &[f64]) -> check_ui::Summary {
        check_ui::Summary {
            target: format!("tension, {}", self.measure.describe()),
            voltages: "-".into(),
            cases: crate::rule_table::cases_words(&self.cases),
            distance: if self.problem().is_some() {
                "not set".into()
            } else {
                format!("{} × RTS", self.factor)
            },
            wires: self.targets.describe(),
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
    // Names no longer in the model, kept.
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

    /// Use local conditions ticked is kept in the rule file's [check] and
    /// read back from it (owner, 2026-11-03).
    #[test]
    fn use_local_conditions_is_kept_in_the_rule_file() {
        let w = WireTensionsWindow {
            local: true,
            ..Default::default()
        };
        let text = w.rule_set().to_toml();
        assert!(text.contains("local = true"), "{text}");
        assert!(!text.contains("override"), "{text}");
        let back = RuleSet::parse(&text).unwrap();
        assert!(back.check.unwrap().local);
        // None ticked: no [check] at all.
        assert!(
            !WireTensionsWindow::default()
                .rule_set()
                .to_toml()
                .contains("[check]")
        );
    }
    use eframe::egui;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    /// Runs the check on `model` in the high wind (Use local conditions
    /// ticked or not) and waits for its report.
    fn run_high_wind(model: &Model, local: bool) -> Report {
        let ctx = egui::Context::default();
        let mut w = WireTensionsWindow {
            local,
            rules: vec![TensionRule {
                factor: 1.0,
                cases: vec!["high wind".into()],
                ..TensionRule::blank("invented 1.0")
            }],
            ..Default::default()
        };
        w.start(model, &ctx);
        let started = std::time::Instant::now();
        loop {
            w.job.poll();
            if let Some(r) = w.job.result.take() {
                return r.unwrap();
            }
            assert!(started.elapsed().as_secs() < 120, "no answer in 120 s");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// Use local conditions (owner, 2026-11-03): structure T2's wind for
    /// strength 1.5 makes the tensions of its spans in the high wind
    /// larger than with the switch unticked, the notes say so; ticked
    /// with no local conditions, the same tensions as unticked.
    #[test]
    fn local_wind_at_a_structure_raises_its_spans_tensions() {
        let mut model = crate::tests::synthetic();
        let t = |r: &Report, span: usize| {
            r.report
                .rows
                .iter()
                .find(|x| x.section == "S1" && x.span == span)
                .unwrap()
                .tension
        };
        let base = run_high_wind(&model, false);
        let same = run_high_wind(&model, true);
        assert_eq!(t(&same, 0), t(&base, 0));
        assert!(
            same.report
                .notes
                .iter()
                .any(|n| n.contains("no structure gives any")),
            "{:?}",
            same.report.notes
        );
        model
            .line
            .structures
            .iter_mut()
            .find(|s| s.id == "T2")
            .unwrap()
            .local
            .wind_static = Some(1.5);
        let with = run_high_wind(&model, true);
        for span in [0, 1] {
            assert!(
                t(&with, span) > t(&base, span) * 1.01,
                "span {span}: {} > {}",
                t(&with, span),
                t(&base, span)
            );
        }
        assert!(
            with.report
                .notes
                .iter()
                .any(|n| n.contains("1 structure(s), spans worst; solved again")),
            "{:?}",
            with.report.notes
        );
        // Unticked: the weather case as written.
        assert_eq!(t(&run_high_wind(&model, false), 0), t(&base, 0));
    }

    /// Checks › Wire tensions: a rule with an invented factor so small that
    /// every span fails; Run lists every span, the failures marked in the
    /// views at their most tensioned support; the rules kept in the rule
    /// file and read back; closed, the marks go.
    #[test]
    fn the_window_runs_marks_and_keeps_its_rules() {
        let model = crate::tests::synthetic();
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 1200.0))
            .build_eframe(|_| crate::app::App::with_model(model, None));
        harness.run();
        harness.get_by_label("Checks").click();
        harness.run();
        harness.get_by_label("Wire tensions…").click();
        harness.run();
        let dir = std::env::temp_dir().join(format!("ohl-app-{}-tensions", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.tension-rules.ohlr");
        {
            let r = harness.state_mut().ready().unwrap();
            let w = &mut r.wire_tensions;
            assert!(w.open);
            assert!(w.why_not_run().is_some());
            w.file = Some(file.clone());
            w.rules = vec![TensionRule {
                factor: 0.01,
                ..TensionRule::blank("invented 0.01")
            }];
            w.save(&mut r.model);
        }
        let back = RuleSet::load(&file).unwrap();
        assert_eq!(back.tension_rules.len(), 1);
        assert_eq!(back.checks, [PresetCheck::Tension]);
        harness.run();
        harness.get_by_label("Run").click();
        let started = std::time::Instant::now();
        loop {
            harness.step();
            let r = harness.state_mut().ready().unwrap();
            if let Some(Ok(rep)) = &r.wire_tensions.job.result {
                let spans: usize = r
                    .model
                    .line
                    .sections
                    .iter()
                    .map(|s| s.points.len() - 1)
                    .sum();
                let cases = r.model.cases.len();
                assert_eq!(rep.report.rows.len(), spans * cases);
                assert_eq!(rep.report.failures().count(), spans * cases);
                break;
            }
            assert!(started.elapsed().as_secs() < 120, "no report in 120 s");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        harness.run();
        // The verdict first (2026-10-31), red: every span fails.
        harness.get_by_label_contains("above the allowed tension — worst");
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.marks_from, crate::app::MarksFrom::Tensions);
        assert!(!r.view3d.marks.is_empty());
        assert!(r.view3d.marks.iter().all(|(a, b)| a == b));
        assert_eq!(r.plan.marks.len(), r.view3d.marks.len());
        r.wire_tensions.open = false;
        harness.run();
        let r = harness.state_mut().ready().unwrap();
        assert!(r.view3d.marks.is_empty() && r.plan.marks.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
