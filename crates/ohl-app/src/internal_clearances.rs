// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Wire clearances › **Internal clearances…** (roadmap O6, first part; owner,
//! 2026-10-25): the electrified parts at each structure — the conductors
//! near it, its jumpers, its strings' live ends and the electrified
//! volumes on its strings (second part: the String library's shapes) —
//! against every member of its body, to the member's surface where its
//! solid is known, and against the grounded volumes on its strings, by
//! rules ([`ohl_clearance::internal`]). Only structures at detail level 2 or 3
//! (with a body); the others are listed as not checked. The rules are kept
//! in the project as a rule file (`<model>.internal-rules.ohlr`,
//! `[[internal_rule]]` tables, the structure and section overrides in its
//! `[check]`); a design-code plugin's preset brings its internal rules.
//! Run solves each case of the rules in the background; every part is
//! listed, the nearest first; those too near (else the nearest) are marked
//! in the views while the window is open.

use std::path::PathBuf;

use eframe::egui;
use ohl_clearance::internal::{self, InternalReport, InternalRule, StructureBody};
use ohl_clearance::rules::{PresetCheck, RuleSet};

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
    /// The rows, the structures not checked, the notes.
    pub report: InternalReport,
    /// The rules checked.
    pub rules: Vec<InternalRule>,
}

/// What the window asks of the app.
#[derive(Default)]
pub struct Out {
    /// Go to a structure.
    pub go: Option<Selection>,
    /// New marks to draw; `Some(empty)`: take them off.
    pub marks: Option<Vec<[f64; 3]>>,
    /// A design-code plugin's preset loaded (its plugin, the preset).
    pub plugin_preset: Option<(String, ohl_io::codes::RulePresetChecked)>,
}

/// The Internal clearances window.
#[derive(Default)]
pub struct InternalClearancesWindow {
    /// Shown.
    pub open: bool,
    /// The project's rule file read.
    loaded: bool,
    /// The rule file kept.
    pub file: Option<PathBuf>,
    /// The rules.
    pub rules: Vec<InternalRule>,
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

/// The rule file of the window in a project: in its rules folder (its
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
    dir.join(format!("{stem}.internal-rules.ohlr"))
}

/// A wind study's winds (ESTI 240 Art. 33, owner 2026-10-25): the cases of
/// the groups that take a structure column's pressure, and per pressure
/// (its bits) the structures with a body that have it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WindStudy {
    /// The cases solved again at the structures' own wind.
    pub cases: std::collections::BTreeSet<String>,
    /// Per pressure, Pa (as `f64::to_bits`), the structures.
    pub at: std::collections::BTreeMap<u64, Vec<String>>,
    /// Values taken at a group's least pressure, values not read.
    pub notes: Vec<String>,
}

/// The wind study of the model's case groups and structures with a body:
/// a group with `structure_wind` names the structure column; a structure
/// with a value there is checked in the group's cases at that pressure
/// (raised to the group's least, said); without a value, at the case's.
pub fn wind_study(model: &Model, bodies: &[StructureBody]) -> WindStudy {
    let mut out = WindStudy::default();
    for g in crate::case_groups::groups(model) {
        if g.structure_wind.is_empty() {
            continue;
        }
        let mut any = false;
        for st in &model.line.structures {
            if !bodies.iter().any(|b| b.structure == st.id) {
                continue;
            }
            let Some(text) = st
                .fields
                .get(&g.structure_wind)
                .filter(|t| !t.trim().is_empty())
            else {
                continue;
            };
            let Some((pa, warn)) = g.structure_pressure(text) else {
                out.notes.push(format!(
                    "{}: wind study \"{text}\" is not a pressure: the case's wind taken",
                    st.id
                ));
                continue;
            };
            if let Some(w) = warn {
                out.notes.push(format!("{}: {w}", st.id));
            }
            let ids = out.at.entry(pa.to_bits()).or_default();
            if !ids.contains(&st.id) {
                ids.push(st.id.clone());
            }
            any = true;
        }
        if any {
            out.cases.extend(g.cases.iter().cloned());
        }
    }
    out
}

/// The bodies of the line's structures at detail level 2 or 3, placed in
/// project coordinates (`ohl_clearance::internal::structure_bodies`, the
/// same as `ohl internal`).
pub fn structure_bodies(model: &Model) -> Vec<StructureBody> {
    ohl_clearance::internal::structure_bodies(&model.line, &model.bodies)
}

/// Each case solved (the case, its sections, its jumpers) as the check
/// takes it: with wind when its wind pressure is above 0.
type SolvedCases = [(
    ohl_weather::WeatherCase,
    Vec<ohl_line::solve::SectionShape>,
    Vec<ohl_line::jumper::JumperShape>,
)];

fn views_of(solved: &SolvedCases) -> Vec<internal::SolvedCase<'_>> {
    solved
        .iter()
        .map(|(c, s, j)| internal::SolvedCase {
            name: &c.name,
            wind: c.wind_pressure.si() > 0.0,
            sections: s,
            jumpers: j,
        })
        .collect()
}

impl InternalClearancesWindow {
    /// Opens the window; the first time, reads the project's rule file
    /// (its internal rules and overrides), else starts with one rule.
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
                    self.rules = set.internal_rules;
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
            self.rules = vec![InternalRule::blank("rule 1")];
        }
    }

    /// Why the check cannot run (none: it can).
    pub fn why_not_run(&self) -> Option<String> {
        if self.rules.is_empty() {
            return Some("no rule: add one, or load a preset".into());
        }
        // A distance not set (0 m, a table or formula not filled in):
        // refused, as the ground windows do (2026-10-27).
        let why: Vec<String> = self
            .rules
            .iter()
            .filter_map(|r| {
                r.problem().or_else(|| {
                    std::iter::once(&r.distance)
                        .chain(r.distance_wind.as_ref())
                        .find_map(|d| d.problem())
                        .map(|p| format!("\"{}\": {p}", r.name))
                })
            })
            .collect();
        (!why.is_empty()).then(|| why.join("; "))
    }

    /// The rules as a rule file.
    pub fn rule_set(&self) -> RuleSet {
        RuleSet {
            name: "Internal clearances".into(),
            // The scope and the overrides ticked.
            check: check_ui::check_of(&self.scope, self.overrides),
            checks: vec![PresetCheck::Internal],
            internal_rules: self.rules.clone(),
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

    /// Starts a check: each weather case of the rules solved in the
    /// background (as the line is, with its jumpers), every structure with
    /// a body checked.
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
        let bodies = structure_bodies(model);
        // The volumes on the strings: the String library's, the strings'
        // own merged in.
        let empty = ohl_structures::InsulatorLibrary::default();
        let fits =
            internal::string_fits(&model.line, model.types.as_ref().map_or(&empty, |(_, i)| i));
        let filter = self.overrides.filter(model);
        // The scope's sections only (none: the whole line).
        let keep = self.scope.keeps(&model.line);
        let scoped = move |mut s: crate::model::Solved| {
            if let Some(k) = &keep {
                s.shapes.retain(|x| k.contains(&x.section));
            }
            s
        };
        let study = if self.overrides.structures {
            wind_study(model, &bodies)
        } else {
            WindStudy::default()
        };
        self.job.start_at(ctx, model.revision(), move || {
            let mut solved = Vec::new();
            let mut notes = study.notes.clone();
            for case in &cases {
                let s = scoped(crate::model::Solved::new_memo(
                    &line,
                    &library,
                    case.clone(),
                ));
                notes.extend(
                    s.failed
                        .iter()
                        .map(|f| format!("\"{}\": not solved: {f}", case.name)),
                );
                solved.push((case.clone(), s.shapes, s.jumpers));
            }
            let views = views_of(&solved);
            // Exit clause: ten minutes is far beyond a line's structures.
            let budget =
                ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(600));
            let mut report =
                internal::check(&line, &views, &bodies, &fits, &rules, &filter, &budget)?;
            // The wind study's structures (the override): their rows of the
            // group's cases taken from a run at their own wind instead.
            let studied = |st: &str| study.at.values().any(|ids| ids.iter().any(|i| i == st));
            report
                .rows
                .retain(|r| !(study.cases.contains(&r.case) && studied(&r.structure)));
            for (pressure, ids) in &study.at {
                let pa = f64::from_bits(*pressure);
                let mut solved = Vec::new();
                for case in cases.iter().filter(|c| study.cases.contains(&c.name)) {
                    let mut case = case.clone();
                    case.wind_pressure = ohl_core::units::Stress::from_si(pa);
                    let s = scoped(crate::model::Solved::new_memo(
                        &line,
                        &library,
                        case.clone(),
                    ));
                    notes.extend(s.failed.iter().map(|f| {
                        format!("\"{}\" at {pa} Pa (wind study): not solved: {f}", case.name)
                    }));
                    solved.push((case, s.shapes, s.jumpers));
                }
                let views = views_of(&solved);
                let these: Vec<StructureBody> = bodies
                    .iter()
                    .filter(|b| ids.contains(&b.structure))
                    .cloned()
                    .collect();
                let part = internal::check(&line, &views, &these, &fits, &rules, &filter, &budget)?;
                for n in part.notes {
                    if !report.notes.contains(&n) {
                        report.notes.push(n);
                    }
                }
                report.rows.extend(part.rows.into_iter().map(|mut r| {
                    r.case = format!("{} (wind study {pa} Pa)", r.case);
                    r
                }));
            }
            if !study.at.is_empty() {
                report.rows.sort_by(|a, b| {
                    (a.distance - a.required)
                        .total_cmp(&(b.distance - b.required))
                        .then_with(|| {
                            (&a.structure, &a.case, &a.part).cmp(&(&b.structure, &b.case, &b.part))
                        })
                });
            }
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

    /// The marks to draw when what the views should show changed: the
    /// parts too near (else the nearest), at the part's point; taken off
    /// once the window is closed (`Some(empty)`).
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
            Some(r) => {
                let failing: Vec<[f64; 3]> = r.report.failures().map(|x| x.at).collect();
                Some(if failing.is_empty() {
                    r.report.rows.first().map(|x| x.at).into_iter().collect()
                } else {
                    failing
                })
            }
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
            crate::dockable::Panel::InternalClearances,
            "Internal clearances",
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
                    "internal-clearances-window",
                    findings,
                    self.job.generation,
                    why.is_some() && !self.job.running(),
                );
                match tab {
                    check_ui::Tab::Setup => help::body(ui, "internal-clearances-window", |ui| {
                        check_ui::section(ui, 1, "What it checks", None);
                        help::explain(
                            ui,
                            "At each structure with a body: the conductors near it, its jumpers and its strings' \
                             live ends against its members, per weather case.",
                            "None ticked: every voltage",
                        );
                        ui.separator();
                        check_ui::section(ui, 2, "Scope", None);
                        if check_ui::ScopePicker::new("internal-clearances-scope", &model.line)
                            .wires(&mut self.scope, true)
                            .ui(ui)
                        {
                            changed = true;
                        }
                        ui.label(
                            egui::RichText::new(
                                "Each rule gives its own voltages and weather cases besides (3 Rules).",
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
                        check_ui::results_area(ui, "internal-clearances-window", |ui| {
                            self.results_ui(ui, &stale, &mut run, &mut out);
                        })
                    }
                }
                help::footer(ui, "internal-clearances-window", |ui| {
                    let overrides = &mut self.overrides;
                    run |= check_ui::job_footer(
                        ui,
                        &mut self.job,
                        why.clone(),
                        "Solve each weather case of the rules and check every structure with a body",
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
    /// each, the selected one's fields below it (its distance with the
    /// preview at the line's voltages), the presets for this check (only
    /// their internal rules), Import / Export; whether one changed.
    fn rules_ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        presets: &[crate::rule_editor::PluginPresets],
        out: &mut Out,
    ) -> bool {
        let part = crate::preset_cases::Part::Check(PresetCheck::Internal);
        let cases: Vec<String> = model.cases.iter().map(|c| c.case.name.clone()).collect();
        let groups = crate::case_groups::groups(model);
        let mut voltages: Vec<f64> = ohl_clearance::case::voltages_of(&model.line)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| p.map(|p| p.nominal_kv))
            .collect();
        voltages.sort_by(f64::total_cmp);
        voltages.dedup();
        let t = check_ui::RuleTable::new("internal-clearance-rules", &mut self.rules, &voltages)
            .add(
                "An internal rule",
                "for every electrified part in every case: set its distance",
                |n| InternalRule::blank(&format!("rule {}", n + 1)),
            )
            .presets(model, presets, part, |set| {
                set.internal_rules
                    .iter()
                    .map(|r| format!("{}: {}", r.name, r.describe_parts()))
                    .collect()
            })
            .ui(ui, |ui, k, r| {
                let mut changed = false;
                egui::Grid::new("internal-clearance-rule")
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
                                for c in &cases {
                                    let mut on = r.cases.contains(c);
                                    if ui.checkbox(&mut on, c).changed() {
                                        r.cases.retain(|x| x != c);
                                        if on {
                                            r.cases.push(c.clone());
                                        }
                                        changed = true;
                                    }
                                }
                            },
                        );
                        ui.end_row();
                        ui.label("Voltages");
                        changed |= crate::rule_table::voltages_menu(
                            ui,
                            &voltages,
                            &mut r.voltages,
                            "None ticked: every voltage",
                        );
                        ui.end_row();
                        ui.label("Parts");
                        ui.horizontal_wrapped(|ui| {
                            changed |= ui
                                .checkbox(&mut r.conductors, "conductors")
                                .on_hover_text("The conductors within the reach along each span")
                                .changed();
                            changed |= ui
                                .checkbox(&mut r.jumpers, "jumpers")
                                .on_hover_text("The jumpers ending at the structure")
                                .changed();
                            changed |= ui
                                .checkbox(&mut r.string_ends, "string ends")
                                .on_hover_text(
                                    "The strings' live ends (their wire points), as points: the \
                                     fittings' size not counted unless given as shapes — on the \
                                     unsafe side",
                                )
                                .changed();
                            changed |= ui
                                .checkbox(&mut r.shapes, "shapes")
                                .on_hover_text(
                                    "The volumes on the strings (Line › Structures › String \
                                     shapes): the electrified ones checked, every part against \
                                     the grounded ones",
                                )
                                .changed();
                        });
                        ui.end_row();
                        ui.label("Reach [m]");
                        let resp = crate::style::field(
                            ui,
                            egui::DragValue::new(&mut r.reach)
                                .range(1.0..=200.0)
                                .speed(0.5),
                        )
                        .on_hover_text("How far along each span from its clamp, in plan");
                        changed |= resp.drag_stopped() || resp.lost_focus();
                        ui.end_row();
                        ui.label("Distance");
                        changed |= crate::requirement_ui::requirement_ui(
                            ui,
                            ("internal-distance", k),
                            &mut r.distance,
                            &voltages,
                        );
                        ui.end_row();
                        // A distance of its own in the cases with wind
                        // (owner, 2026-10-19); none: the same.
                        ui.label("With wind");
                        ui.vertical(|ui| {
                            let mut own = r.distance_wind.is_some();
                            if ui
                                .checkbox(&mut own, "a distance of its own")
                                .on_hover_text(
                                    "A distance of its own in the weather cases with wind (a \
                                     wind pressure above 0); not ticked: the distance in every \
                                     case",
                                )
                                .changed()
                            {
                                r.distance_wind = own.then(|| r.distance.clone());
                                changed = true;
                            }
                            if let Some(d) = &mut r.distance_wind {
                                changed |= crate::requirement_ui::requirement_ui(
                                    ui,
                                    ("internal-distance-wind", k),
                                    d,
                                    &voltages,
                                );
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
                        p.rules.internal_rules.clone(),
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
                    self.rules = set.internal_rules;
                    changed = true;
                }
                Err(e) => self.status = Some((e, true)),
            }
        }
        if t.export
            && let Some(p) = crate::rule_table::export_file("internal-rules.ohlr")
        {
            self.status = Some(match self.rule_set().save(&p) {
                Ok(()) => (format!("exported to {}", p.display()), false),
                Err(e) => (e, true),
            });
        }
        changed
    }

    /// The results: every part, the nearest first; the structures not
    /// checked.
    fn results_ui(
        &mut self,
        ui: &mut egui::Ui,
        stale: &Option<String>,
        run: &mut bool,
        out: &mut Out,
    ) {
        let report = match &self.job.result {
            None => {
                ui.label("Run to check every structure with a body.");
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
        if !report.report.not_checked.is_empty() {
            egui::CollapsingHeader::new(format!(
                "{} structure(s) not checked",
                report.report.not_checked.len()
            ))
            .id_salt("internal-not-checked")
            .show(ui, |ui| {
                crate::properties::cases_box(ui, "internal-not-checked-list", |ui| {
                    for (s, why) in &report.report.not_checked {
                        ui.label(format!("{s}: {why}"));
                    }
                });
            });
        }
        let failed = report.report.failures().count();
        ui.horizontal(|ui| {
            ui.label(format!(
                "{} parts × cases checked, {failed} too near",
                report.report.rows.len()
            ));
            if ui.button("Export CSV…").clicked() {
                save_csv("internal-clearances.csv", &report.report.csv(&report.rules));
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
                let against = r.against();
                check_ui::Row::new(
                    vec![
                        Cell::text(&r.structure),
                        Cell::text(r.part.to_string()),
                        Cell::text(if against.is_empty() {
                            "—".into()
                        } else {
                            against
                        }),
                        Cell::text(&report.rules[r.rule].name),
                        Cell::text(&r.case),
                        Cell::num(Some(r.distance), 2).bad(!r.pass()),
                        Cell::num(Some(r.required), 2),
                        Cell::text(if r.to_surface {
                            "surface"
                        } else {
                            "centre line"
                        }),
                        Cell::text(if r.pass() { "pass" } else { "too near" }).bad(!r.pass()),
                    ],
                    !r.pass(),
                )
            })
            .collect();
        if let Some(k) = check_ui::ResultTable::new(
            "internal-clearance-rows",
            vec![
                Col::text("Structure"),
                Col::text("Part"),
                Col::text("Member or shape"),
                Col::text("Rule"),
                Col::text("Case"),
                Col::num("Distance [m]"),
                Col::num("Required [m]"),
                Col::text("To"),
                Col::text("Verdict"),
            ],
        )
        .rows(rows)
        .group_by(&[0, 3, 4])
        .csv("internal-clearances-table.csv")
        .ui(ui)
        {
            out.go = Some(Selection::Structure(
                report.report.rows[k].structure.clone(),
            ));
        }
    }
}

/// An internal clearance rule as a row of the shared rule table.
impl check_ui::RuleRow for InternalRule {
    fn name(&self) -> &str {
        &self.name
    }

    fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }

    fn summary(&self, _: &[f64]) -> check_ui::Summary {
        let mut distance = crate::rule_table::distance_words(&self.distance);
        if let Some(w) = &self.distance_wind {
            distance = format!(
                "{distance}; with wind {}",
                crate::rule_table::distance_words(w)
            );
        }
        check_ui::Summary {
            target: "the structure's members".into(),
            voltages: crate::rule_table::voltages_words(&self.voltages),
            cases: crate::rule_table::cases_words(&self.cases),
            distance,
            wires: self.describe_parts(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule file keeps the window's rules and overrides and reads them
    /// back; the bodies of the typed example line are placed for its
    /// structures with one (detail 2 or 3).
    #[test]
    fn rules_and_bodies() {
        let w = InternalClearancesWindow {
            rules: vec![InternalRule::blank("a")],
            overrides: crate::case_groups::Overrides {
                structures: true,
                sections: false,
            },
            ..Default::default()
        };
        let text = w.rule_set().to_toml();
        let back = RuleSet::parse(&text).unwrap();
        assert_eq!(back.internal_rules.len(), 1);
        assert!(back.check.unwrap().structure_override);
        let m = crate::tests::typed_project("internal-window");
        let bodies = structure_bodies(&m);
        for b in &bodies {
            let st = m.line.structure(&b.structure).unwrap();
            assert!(st.structure_type.is_some());
            assert!(!b.members.is_empty());
        }
    }

    /// A wind study (ESTI 240 Art. 33): a structure with a value in the
    /// group's column is checked in the group's cases at that pressure, a
    /// value under the group's least raised to it (said); one without a
    /// value, or of a group without the column, stays at the case's wind.
    #[test]
    fn wind_study_per_structure() {
        let mut m = crate::tests::typed_project("internal-wind-study");
        let dir = m.sources.file.parent().unwrap().to_path_buf();
        let weather = dir.join("cases.ohlwc");
        std::fs::write(
            &weather,
            "format = \"ohl-weather-cases\"\nversion = 1\n\
             [[case]]\nname = \"wind\"\ntemperature = \"10 °C\"\nwind_pressure = \"650 Pa\"\n\
             [[case]]\nname = \"rest\"\ntemperature = \"10 °C\"\n\
             [[group]]\nname = \"short\"\ncases = [\"wind\"]\n\
             structure_wind = \"p.wind_study\"\nstructure_wind_min = \"300 Pa\"\n\
             [[group]]\nname = \"at rest\"\ncases = [\"rest\"]\n",
        )
        .unwrap();
        m.sources.weather = Some(weather);
        let bodies = structure_bodies(&m);
        assert!(bodies.len() >= 2, "the typed line has bodies");
        let (a, b) = (bodies[0].structure.clone(), bodies[1].structure.clone());
        for st in &mut m.line.structures {
            if st.id == a {
                st.fields.insert("p.wind_study".into(), "450 Pa".into());
            } else if st.id == b {
                st.fields.insert("p.wind_study".into(), "200".into());
            }
        }
        let s = wind_study(&m, &bodies);
        assert_eq!(s.cases.iter().collect::<Vec<_>>(), ["wind"]);
        assert_eq!(s.at.get(&450f64.to_bits()), Some(&vec![a]));
        assert_eq!(s.at.get(&300f64.to_bits()), Some(&vec![b.clone()]));
        assert_eq!(s.at.len(), 2);
        assert!(
            s.notes
                .iter()
                .any(|n| n.starts_with(&b) && n.contains("300 Pa")),
            "{:?}",
            s.notes
        );
    }
}

#[cfg(test)]
mod app_tests {
    use eframe::egui;
    use egui_kittest::Harness;

    use super::*;

    /// Checks › Internal clearances on the typed example line: run with an
    /// INVENTED 50 m rule (no code's value: every part is too near), rows
    /// for the structures with a body, the others not checked, the marks
    /// drawn while open and taken off once closed.
    #[test]
    fn the_window_runs_and_marks() {
        let model = crate::tests::typed_project("internal-run");
        let mut h = Harness::builder()
            .with_size(egui::vec2(1500.0, 950.0))
            .build_eframe(|_| crate::app::App::with_model(model, None));
        h.run();
        let ctx = h.ctx.clone();
        {
            let r = h.state_mut().ready().unwrap();
            r.internal_clearances.show(&r.model);
            let mut rule = InternalRule::blank("invented 50 m");
            rule.distance = ohl_clearance::Requirement::Fixed(50.0);
            r.internal_clearances.rules = vec![rule];
            let w = &mut r.internal_clearances;
            w.start(&r.model, &ctx);
        }
        let started = std::time::Instant::now();
        loop {
            h.step();
            let r = h.state_mut().ready().unwrap();
            if let Some(res) = &r.internal_clearances.job.result {
                let rep = res.as_ref().unwrap();
                assert!(!rep.report.rows.is_empty());
                assert!(rep.report.failures().count() > 0);
                assert!(!rep.report.not_checked.is_empty());
                break;
            }
            assert!(started.elapsed().as_secs() < 120, "no report in 120 s");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        h.run();
        let r = h.state_mut().ready().unwrap();
        assert_eq!(r.marks_from, crate::app::MarksFrom::Internal);
        assert!(!r.view3d.marks.is_empty());
        r.internal_clearances.open = false;
        h.run();
        let r = h.state_mut().ready().unwrap();
        assert!(r.view3d.marks.is_empty());
    }
}
