// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › **Internal clearances…** (roadmap O6, first part; owner,
//! 2026-10-25): the electrified parts at each structure — the conductors
//! near it, its jumpers, its strings' live ends and the electrified
//! volumes on its strings (second part: the String library's shapes) —
//! against every member of its body, to the member's surface where its
//! solid is known, and against the grounded volumes on its strings, by
//! rules ([`ohl_clearance::internal`]). Only structures at detail level 2 or 3
//! (with a body); the others are listed as not checked. The rules are kept
//! in the project as a rule file (`<model>.internal-rules.ohlr`,
//! `[[internal_rule]]` tables, Use local conditions in its
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
use crate::job::Job;
use crate::model::Model;
use crate::selection::Selection;

/// What its results are computed from ([`crate::revision`]).
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::LINE;

/// Its **Use local conditions**: the wind taken, the check's name for
/// the run times, what the wind is for.
const LOCAL: (ohl_line::local::Purpose, &str, &str) = (
    ohl_line::local::Purpose::Clearance,
    "internal-clearances",
    "the swing of the strings and jumpers at the structures",
);

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
    /// The weather case groups' wind study to copy into the structures'
    /// local conditions (one undoable command; [`GroupStudy::command`]).
    pub convert: Option<GroupStudy>,
    /// A section of the window's user guide page to open ([`guide`]).
    pub guide: Option<&'static str>,
}

/// The window's own page of the user guide (owner, 2026-11-12: the
/// check pages' treatment, after the vertical and radial windows).
pub const GUIDE: &str = "user-guide/internal-clearances";

/// The sections of [`GUIDE`] the window's **user guide ↗** buttons open
/// (each a heading of the page: tested).
pub mod anchors {
    /// *1 What it checks*.
    pub const WHAT: &str = "1-what-it-checks";
    /// *2 Rules*.
    pub const RULES: &str = "2-rules";
    /// *3 Scope*.
    pub const SCOPE: &str = "3-scope";
    /// *4 Model*: the wind study.
    pub const MODEL: &str = "4-model-more-options-folded";
    /// The Results tab.
    pub const RESULTS: &str = "the-results";
}

/// The page and section a **user guide ↗** button opens.
pub fn guide(anchor: &'static str) -> (&'static str, &'static str) {
    (GUIDE, anchor)
}

/// The question's longer text, in the (?) of *1 What it checks*.
const WHAT_HELP: (&str, &str) = (
    "At each structure with a body: the conductors near it, its jumpers and its strings' live \
     ends against its members, per weather case.",
    "Run solves the line in each weather case of the rules, with its jumpers. At each structure \
     with a body (detail level 2 or 3) the electrified parts — the conductors out to the rule's \
     reach, the jumpers, the strings' live ends and electrified shapes — are measured to every \
     member (to its surface where its section is known, else to its centre line: on the unsafe \
     side, said per row) and to the strings' grounded shapes, and compared with the rule's \
     distance at the part's voltage. Earth wires and structures without a body are not checked.",
);

/// The rules' explanation, in the (?) of *2 Rules*.
const RULES_HELP: (&str, &str) = (
    "A rule: which live parts must keep which distance from the structure, in which weather \
     cases, for which voltages and insulation levels. Click a rule to edit it below.",
    "Every rule that takes a part (its weather case, its voltage, its system's insulation \
     level) gives its own row; there is no precedence among rules. Distance: one value, a \
     table \"up to kV: distance\" or a base plus an amount per kV; with wind, a distance of its \
     own if ticked. A preset (a design code's) is merged into the rules (a rule of the same \
     name replaced) or replaces them. Run is greyed while a rule's distance is not set.",
);

/// The scope's explanation, in the (?) of *3 Scope*.
const SCOPE_HELP: (&str, &str) = (
    "Circuits, routes and sections: those checked (none ticked: the whole line).",
    "Only the sections of the circuits ticked, on the routes, alignments or sections ticked, \
     are solved and their structures checked. Fewer: fewer sections solved, faster. Each rule \
     gives its own voltages and weather cases besides (2 Rules).",
);

/// The wind study's explanation, in the (?) of its heading under *4 Model*.
const WIND_STUDY_HELP: (&str, &str) = (
    "With Use local conditions ticked: a structure with a wind study (Structures table › wind \
     study Pa) is checked at its pressure in the cases below.",
    "A wind study may give a structure another wind than the weather case for the conductors' \
     distances to it. With Use local conditions ticked, each structure with a body whose local \
     conditions give a wind study is checked, in every weather case of the rules with wind that \
     is listed here (none listed: every one), a second time at the study's pressure in place of \
     the case's — its rows in those cases are that run's, marked \"(wind study … Pa)\". A \
     pressure below the least is taken at the least. The structures without one keep the \
     case's wind (times their wind factor for clearances). Time: one more solve of the line per \
     study pressure and case.",
);

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
    /// **Use local conditions** (owner, 2026-11-03): the swing with the structures' wind
    /// for clearances and wire ice on their spans, and their wind study
    /// (2026-11-04).
    pub local: bool,
    /// The least pressure of a structure's wind study, Pa (`[check]
    /// wind_study_min`).
    pub wind_study_min: Option<f64>,
    /// The cases taking the wind study (`[check] wind_study_cases`;
    /// empty: every case of the rules with wind).
    pub wind_study_cases: Vec<String>,
    /// The cases' field as typed.
    study_cases_text: Option<String>,
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
    /// What the rule editor offers of the line.
    choices: check_ui::LineChoices,
    /// The results table's rows, by the answer's count and its rows.
    rows: check_ui::CachedRows<(u64, usize)>,
}

/// The rule file of the window in a project: in its rules folder (its
/// working copy), else next to the model.
pub fn project_file(model: &Model) -> PathBuf {
    crate::check_ui::project_rule_file(model, "internal-rules.ohlr")
}

/// A wind study's winds (ESTI 240 Art. 33; owner 2026-11-04: "make the
/// wind study local"): the cases that take the structures' wind study,
/// and per pressure (its bits) the structures with a body that have one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WindStudy {
    /// The cases solved again at the structures' own wind.
    pub cases: std::collections::BTreeSet<String>,
    /// Per pressure, Pa (as `f64::to_bits`), the structures.
    pub at: std::collections::BTreeMap<u64, Vec<String>>,
    /// Values taken at the least pressure.
    pub notes: Vec<String>,
}

/// The wind study of `line`'s structures with a body (`bodies`) in the
/// check's weather cases `cases`, with **Use local conditions** ticked
/// (owner 2026-11-04): a structure whose local conditions give a
/// `wind_study` is checked, in each case with wind (a pressure above 0)
/// that `check` lets take it
/// ([`ohl_clearance::rules::CheckScope::takes_wind_study`]), at that pressure in place of the case's — raised
/// to the check's least (`wind_study_min`, said); a structure without one,
/// at the case's. Nothing when no structure with a body gives one.
pub fn wind_study(
    line: &ohl_line::Line,
    bodies: &[StructureBody],
    cases: &[ohl_weather::WeatherCase],
    check: &ohl_clearance::rules::CheckScope,
) -> WindStudy {
    let mut out = WindStudy::default();
    for st in &line.structures {
        let Some(v) = st.local.wind_study else {
            continue;
        };
        if !bodies.iter().any(|b| b.structure == st.id) {
            continue;
        }
        let pa = match check.wind_study_min {
            Some(min) if v < min => {
                out.notes.push(format!(
                    "{}: wind study {v} Pa below the least {min} Pa: taken at {min} Pa",
                    st.id
                ));
                min
            }
            _ => v,
        };
        out.at.entry(pa.to_bits()).or_default().push(st.id.clone());
    }
    if out.at.is_empty() {
        return WindStudy::default();
    }
    out.cases = cases
        .iter()
        .filter(|c| c.wind_pressure.si() > 0.0 && check.takes_wind_study(&c.name))
        .map(|c| c.name.clone())
        .collect();
    if out.cases.is_empty() {
        let n: usize = out.at.values().map(Vec::len).sum();
        return WindStudy {
            notes: vec![format!(
                "wind study at {n} structure(s) not taken: no weather case of the rules with \
                 wind{}",
                if check.wind_study_cases.is_empty() {
                    String::new()
                } else {
                    format!(" among {}", check.wind_study_cases.join(", "))
                }
            )],
            ..WindStudy::default()
        };
    }
    out
}

/// The weather case groups' wind study columns (2026-10-25, replaced by
/// the structures' local `wind_study` on 2026-11-04) as the conversion
/// takes them: per structure the pressure of its value in a group's
/// column, the groups' least (the largest of theirs: on the safe side,
/// more wind, more swing) and their cases.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GroupStudy {
    /// Per structure (by id), the pressure, Pa.
    pub values: Vec<(String, f64)>,
    /// The least pressure (none: no group gives one).
    pub min: Option<f64>,
    /// The groups' cases, by name.
    pub cases: Vec<String>,
    /// What was not taken, or chosen, in words.
    pub notes: Vec<String>,
}

impl GroupStudy {
    /// The groups' wind study on `line`: every group with a
    /// `structure_wind` column, every structure with a value there that
    /// reads as a pressure above 0 (as the check read it: Pa, with or
    /// without the unit); two groups giving a structure different values:
    /// the larger (said).
    pub fn of(line: &ohl_line::Line, groups: &[ohl_weather::CaseGroup]) -> Self {
        let mut out = Self::default();
        for g in groups.iter().filter(|g| !g.structure_wind.is_empty()) {
            if let Some(m) = g.structure_wind_min.map(ohl_core::units::Stress::si) {
                out.min = Some(out.min.map_or(m, |o: f64| o.max(m)));
            }
            for c in &g.cases {
                if !out.cases.contains(c) {
                    out.cases.push(c.clone());
                }
            }
            for st in &line.structures {
                let Some(text) = st
                    .fields
                    .get(&g.structure_wind)
                    .filter(|t| !t.trim().is_empty())
                else {
                    continue;
                };
                let Some(v) = ohl_weather::CaseGroup::structure_value(text) else {
                    out.notes.push(format!(
                        "{}: \"{text}\" in {} is not a pressure: not taken",
                        st.id, g.structure_wind
                    ));
                    continue;
                };
                match out.values.iter_mut().find(|(id, _)| *id == st.id) {
                    Some((_, old)) if *old != v => {
                        out.notes.push(format!(
                            "{}: {old} Pa and {v} Pa in two groups' columns: {} Pa taken (the \
                             larger)",
                            st.id,
                            old.max(v)
                        ));
                        *old = old.max(v);
                    }
                    Some(_) => {}
                    None => out.values.push((st.id.clone(), v)),
                }
            }
        }
        out
    }

    /// The structures whose local `wind_study` differs from the groups'
    /// value (the conversion would change them).
    pub fn pending<'a>(&'a self, line: &'a ohl_line::Line) -> impl Iterator<Item = &'a str> + 'a {
        self.values.iter().filter_map(|(id, v)| {
            (line.structure(id)?.local.wind_study != Some(*v)).then_some(id.as_str())
        })
    }

    /// The conversion as one command (undone as one): each structure
    /// pending takes the groups' value as its local `wind_study`; the
    /// plugin's column is kept (the plugin's data). None: nothing to
    /// change.
    pub fn command(
        &self,
        line: &ohl_line::Line,
    ) -> Result<Option<(String, ohl_session::Command)>, String> {
        let mut commands = Vec::new();
        let mut ids = Vec::new();
        for id in self.pending(line) {
            let Some(mut st) = line.structure(id).cloned() else {
                continue;
            };
            st.local.wind_study = self.values.iter().find(|(s, _)| s == id).map(|(_, v)| *v);
            commands.push(ohl_session::Command::Put {
                kind: ohl_session::Kind::Structure,
                id: st.id.clone(),
                value: serde_json::to_value(&st).map_err(|e| e.to_string())?,
                ord: None,
            });
            ids.push(id.to_owned());
        }
        if commands.is_empty() {
            return Ok(None);
        }
        Ok(Some((
            format!(
                "the weather case groups' wind study copied into the local conditions of {} \
                 structure(s): {}",
                ids.len(),
                ids.join(", ")
            ),
            ohl_session::Command::Batch { commands },
        )))
    }
}

/// The groups still giving a wind study column, in words: no longer read
/// (2026-11-04); none without.
pub fn group_study_note(groups: &[ohl_weather::CaseGroup]) -> Option<String> {
    let named: Vec<String> = groups
        .iter()
        .filter(|g| !g.structure_wind.is_empty())
        .map(|g| format!("\"{}\" ({})", g.name, g.structure_wind))
        .collect();
    (!named.is_empty()).then(|| {
        format!(
            "weather case group(s) {} give a wind study column: no longer read (2026-11-04) — \
             a structure's wind study is its local wind_study (Structures table › wind study \
             Pa); Copy into the structures (Setup, under 4 Model) takes the columns' values",
            named.join(", ")
        )
    })
}

/// The bodies of the line's structures at detail level 2 or 3, placed in
/// project coordinates (`ohl_clearance::internal::structure_bodies`, the
/// same as `ohl internal`).
pub fn structure_bodies(model: &Model) -> Vec<StructureBody> {
    ohl_clearance::internal::structure_bodies(&model.line, &model.bodies)
}

/// How many of the line's structures have a body (detail level 2 or 3:
/// their type drawn with members or a pole), of how many: the same test
/// as [`structure_bodies`], without placing them.
pub fn bodies_count(model: &Model) -> (usize, usize) {
    let with = model
        .line
        .structures
        .iter()
        .filter(|st| {
            st.structure_type
                .as_deref()
                .and_then(|t| model.bodies.get(t, st.extension.as_deref()))
                .is_some()
        })
        .count();
    (with, model.line.structures.len())
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
    /// (its internal rules and Use local conditions), else starts with one rule.
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
                        self.local = c.local;
                        self.scope = check_ui::Scope::of_check(&c);
                        self.wind_study_min = c.wind_study_min;
                        self.wind_study_cases = c.wind_study_cases;
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
        // The scope, Use local conditions and the wind study.
        let mut check = check_ui::check_of(&self.scope, self.local).unwrap_or_default();
        check.wind_study_min = self.wind_study_min;
        check.wind_study_cases = self.wind_study_cases.clone();
        RuleSet {
            name: "Internal clearances".into(),
            check: Some(check).filter(|c| !c.is_empty()),
            checks: vec![PresetCheck::Internal],
            internal_rules: self.rules.clone(),
            ..RuleSet::default()
        }
    }

    /// Writes the rule file; the project's working copy goes into the
    /// project.
    pub fn save(&mut self, model: &mut Model) {
        let file = self.file.get_or_insert_with(|| project_file(model)).clone();
        self.status = Some(crate::check_ui::rule_file_status(
            self.rule_set().save(&file),
            &file,
            model,
        ));
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
        // The line with the local conditions when ticked (the wind for
        // clearances).
        let local = crate::local_solve::prepare(&model.line, self.local, LOCAL.0);
        let library = model.library.clone();
        let rules = self.rules.clone();
        let bodies = structure_bodies(model);
        // The volumes on the strings: the String library's, the strings'
        // own merged in.
        let empty = ohl_structures::InsulatorLibrary::default();
        let fits =
            internal::string_fits(&model.line, model.types.as_ref().map_or(&empty, |(_, i)| i));
        let filter = ohl_weather::CaseFilter::default();
        // The scope's sections only (none: the whole line).
        let keep = self.scope.keeps(&model.line);
        let scoped = move |mut s: crate::model::Solved| {
            if let Some(k) = &keep {
                s.shapes.retain(|x| k.contains(&x.section));
            }
            s
        };
        // The structures' wind study (owner 2026-11-04) with Use local
        // conditions ticked; its runs on the line as it is (the study's
        // pressure in place of the case's, not times the structures'
        // factors for clearances).
        let check = self.rule_set().check.unwrap_or_default();
        let mut study = if self.local {
            wind_study(&model.line, &bodies, &cases, &check)
        } else {
            WindStudy::default()
        };
        let studies = model
            .line
            .structures
            .iter()
            .filter(|s| s.local.wind_study.is_some())
            .count();
        if !self.local && studies > 0 {
            study.notes.push(format!(
                "wind study of {studies} structure(s) not taken: Use local conditions unticked"
            ));
        }
        study
            .notes
            .extend(group_study_note(&crate::case_groups::groups(model)));
        let base = (!study.at.is_empty()).then(|| model.line.clone());
        self.job.start_at(ctx, model.revision(), move || {
            let started = std::time::Instant::now();
            let line = local.line();
            let mut solved = Vec::new();
            let mut notes = local.notes();
            notes.extend(study.notes.iter().cloned());
            for case in &cases {
                let s = scoped(local.solved(&library, case.clone()));
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
                internal::check(line, &views, &bodies, &fits, &rules, &filter, &budget)?;
            // The wind study's structures: their rows of the study's cases
            // taken from a run at their own wind instead.
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
                        base.as_ref().unwrap_or(line),
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
                let part = internal::check(line, &views, &these, &fits, &rules, &filter, &budget)?;
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
            local.finish(LOCAL.1, started.elapsed());
            Ok(Report { report, rules })
        });
    }

    /// What the last check was computed from that changed since, in
    /// words (`None`: nothing, or no report).
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        self.job.out_of_date(model, USES)
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
        let mut guide = None;
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
                    "internal-clearances-window",
                    findings,
                    self.job.generation,
                    why.is_some() && !self.job.running(),
                );
                match tab {
                    check_ui::Tab::Setup => help::body(ui, "internal-clearances-window", |ui| {
                        // The check pages' treatment (owner, 2026-11-12):
                        // the most used first (the rules), the rarely used
                        // folded under Model; the grey lines between the
                        // fields in the (?) of their section; warnings,
                        // what blocks Run and the time stay written.
                        guide = guide.or(check_ui::section(
                            ui,
                            1,
                            "What it checks",
                            Some(anchors::WHAT),
                        ));
                        help::explain(ui, WHAT_HELP.0, WHAT_HELP.1);
                        check_ui::how_computed(
                            ui,
                            "internal-clearances-window",
                            "The wires and jumpers solved in each weather case of the rules; at each structure with a body, every live part measured to every member's surface.",
                            help::internal_clearance,
                        );
                        // Which structures can be checked, before Run.
                        let (with, all) = bodies_count(model);
                        if with < all {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{with} of {all} structures have a body (detail levels 2 and \
                                     3): the others are listed as not checked."
                                ))
                                .small()
                                .color(ui.visuals().warn_fg_color),
                            );
                        }
                        ui.separator();
                        guide = guide.or(check_ui::section_help(
                            ui,
                            2,
                            "Rules",
                            Some(anchors::RULES),
                            Some(RULES_HELP),
                        ));
                        changed |= self.rules_ui(ui, model, presets, &mut out);
                        ui.separator();
                        guide = guide.or(check_ui::section_help(
                            ui,
                            3,
                            "Scope",
                            Some(anchors::SCOPE),
                            Some(SCOPE_HELP),
                        ));
                        if check_ui::ScopePicker::new("internal-clearances-scope", &model.line)
                            .route_of(model)
                            .wires(&mut self.scope, true)
                            .ui(ui)
                        {
                            changed = true;
                        }
                        ui.separator();
                        // Rarely changed: folded (check pages' guidelines);
                        // what it says that matters stays below the fold.
                        let model_guide =
                            check_ui::model_section(ui, "internal-clearances-window", 4, |ui| {
                                let g = ui
                                    .horizontal(|ui| {
                                        let g = help::heading(ui, "Wind study", anchors::MODEL);
                                        help::question_both(
                                            ui,
                                            WIND_STUDY_HELP.0,
                                            WIND_STUDY_HELP.1,
                                        );
                                        g
                                    })
                                    .inner;
                                changed |= self.study_ui(ui);
                                g
                            });
                        guide = guide.or(model_guide.flatten());
                        self.study_notes_ui(ui, model, &mut out);
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
                            guide = guide.or(help::heading(ui, "Results", anchors::RESULTS));
                            self.results_ui(ui, &stale, &mut run, &mut out);
                        })
                    }
                }
                help::footer(ui, "internal-clearances-window", |ui| {
                    let local = &mut self.local;
                    run |= check_ui::job_footer(
                        ui,
                        &mut self.job,
                        why.clone(),
                        "Solve each weather case of the rules and check every structure with a body",
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
        out.guide = guide;
        if changed {
            self.save(model);
        }
        if run {
            self.start(model, ctx);
        }
        out.marks = self.marks_change();
        out
    }

    /// The wind study's settings (owner 2026-11-04: "make the wind study
    /// local"), under *Model*: the least pressure and the cases taking
    /// it, kept in the rule file's `[check]`; whether a setting changed.
    fn study_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            let mut has = self.wind_study_min.is_some();
            if ui
                .checkbox(&mut has, "Least pressure [Pa]")
                .on_hover_text(
                    "A structure's wind study below it is taken at it (a design code's bound), \
                     said in the notes: more wind, more swing, on the safe side. Unticked: \
                     none, the study's pressure as given. No effect on the time.",
                )
                .changed()
            {
                self.wind_study_min = has.then_some(self.wind_study_min.unwrap_or(300.0));
                changed = true;
            }
            if let Some(p) = &mut self.wind_study_min {
                let r = crate::style::field(
                    ui,
                    egui::DragValue::new(p)
                        .range(1.0..=10_000.0)
                        .speed(5.0)
                        .suffix(" Pa"),
                );
                changed |= r.drag_stopped() || r.lost_focus();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Cases taking it").on_hover_text(
                "The weather cases taking the wind study, by name (* and ? wildcards, comma \
                 separated); empty: every case of the rules with wind. Each case listed is \
                 solved once more per study pressure.",
            );
            let text = self
                .study_cases_text
                .get_or_insert_with(|| self.wind_study_cases.join(", "));
            let r = ui.add(
                egui::TextEdit::singleline(text)
                    .hint_text("every case with wind")
                    .desired_width(260.0),
            );
            if r.lost_focus() {
                let list: Vec<String> = text
                    .split(',')
                    .map(|c| c.trim().to_owned())
                    .filter(|c| !c.is_empty())
                    .collect();
                if list != self.wind_study_cases {
                    self.wind_study_cases = list;
                    changed = true;
                }
                self.study_cases_text = None;
            }
        });
        changed
    }

    /// What the wind study says that matters, below the *Model* fold
    /// (always visible): how many structures give one and that it is not
    /// taken while Use local conditions is unticked; the weather case
    /// groups' old columns, offered once to copy into the structures.
    fn study_notes_ui(&mut self, ui: &mut egui::Ui, model: &Model, out: &mut Out) {
        let studies = model
            .line
            .structures
            .iter()
            .filter(|s| s.local.wind_study.is_some())
            .count();
        if studies > 0 {
            let text = format!(
                "{studies} structure(s) give a wind study{}",
                if self.local {
                    ": taken (4 Model › Wind study)"
                } else {
                    " — not taken: Use local conditions unticked"
                }
            );
            let text = egui::RichText::new(text).small();
            ui.label(if self.local {
                text.weak()
            } else {
                text.color(ui.visuals().warn_fg_color)
            });
        }
        // The weather case groups' columns of before (2026-10-25): offered
        // once to copy into the structures.
        let groups = crate::case_groups::groups_shared(model);
        if let Some(note) = group_study_note(&groups) {
            let study = GroupStudy::of(&model.line, &groups);
            let pending = study.pending(&model.line).count();
            ui.label(
                egui::RichText::new(note)
                    .small()
                    .color(ui.visuals().warn_fg_color),
            );
            if pending > 0
                && ui
                    .button(format!("Copy into the structures ({pending})"))
                    .on_hover_text(
                        "Each structure's value in the groups' wind study column becomes its \
                         local wind study (one change, undone with Undo; the plugin's column is \
                         kept); the groups' least pressure and cases become this check's when \
                         it gives none",
                    )
                    .clicked()
            {
                out.convert = Some(study);
            }
        }
    }

    /// Takes the groups' least pressure and cases where this check gives
    /// none (after their wind study was copied into the structures);
    /// whether anything changed.
    pub fn take_group_study(&mut self, study: &GroupStudy) -> bool {
        let mut changed = false;
        if self.wind_study_min.is_none() && study.min.is_some() {
            self.wind_study_min = study.min;
            changed = true;
        }
        if self.wind_study_cases.is_empty() && !study.cases.is_empty() {
            self.wind_study_cases.clone_from(&study.cases);
            self.study_cases_text = None;
            changed = true;
        }
        changed
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
        // The lists offered, made once per version of the line.
        let check_ui::LineChoices {
            cases,
            groups,
            voltages,
            ..
        } = self.choices.of(model);
        let t = check_ui::RuleTable::new("internal-clearance-rules", &mut self.rules, voltages)
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
                        ui.label("Weather cases").on_hover_text(
                            "The cases this rule checks in (none ticked: every case); Run solves \
                             the line once in every case any rule names: more cases, more time",
                        );
                        crate::menu::options_menu(
                            ui,
                            &crate::rule_table::cases_words(&r.cases),
                            |ui| {
                                changed |= crate::case_groups::pick_names(ui, groups, &mut r.cases);
                                for c in cases {
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
                        ui.label("Voltages").on_hover_text(
                            "Only the parts of these nominal voltages are checked by this rule \
                             (none ticked: every voltage; earth wires never)",
                        );
                        changed |= crate::rule_table::voltages_menu(
                            ui,
                            voltages,
                            &mut r.voltages,
                            "None ticked: every voltage",
                        );
                        ui.end_row();
                        ui.label("Insulation").on_hover_text(INSULATION_HELP);
                        changed |= insulation_ui(ui, r);
                        ui.end_row();
                        ui.label("Parts").on_hover_text(
                            "The live parts this rule measures against the structure; a part \
                             unticked is not checked by it",
                        );
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
                        .on_hover_text(
                            "How far along each span from its clamp the conductor is measured, in \
                             plan (default 20 m); shorter: the conductor farther out is not checked",
                        );
                        changed |= resp.drag_stopped() || resp.lost_focus();
                        ui.end_row();
                        ui.label("Distance").on_hover_text(
                            "The distance each part must keep from the members and the grounded \
                             fittings, read at the part's voltage: a part nearer is too near",
                        );
                        changed |= crate::requirement_ui::requirement_ui(
                            ui,
                            ("internal-distance", k),
                            &mut r.distance,
                            voltages,
                        );
                        ui.end_row();
                        // A distance of its own in the cases with wind
                        // (owner, 2026-10-19); none: the same.
                        ui.label("With wind").on_hover_text(
                            "In the weather cases with wind (a pressure above 0): a distance of \
                             its own, when ticked",
                        );
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
                                    voltages,
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
        // The verdict (the shared banner), first: green when every part is
        // at its distance, red with the smallest margin (or when nothing
        // was checked).
        let failed = report.report.failures().count();
        let worst = report
            .report
            .rows
            .iter()
            .min_by(|a, b| (a.distance - a.required).total_cmp(&(b.distance - b.required)));
        let text = match worst {
            Some(r) if failed > 0 => format!(
                "✖ {failed} part{} × case too near — worst {} {}: {:.2} m of {:.2} m (margin \
                 {:+.2} m), rule {}, case {}",
                if failed == 1 { "" } else { "s" },
                r.structure,
                r.part,
                r.distance,
                r.required,
                r.distance - r.required,
                report.rules[r.rule].label(),
                r.case
            ),
            Some(r) => format!(
                "✔ Every part at its distance ({} parts × cases; smallest margin {:+.2} m)",
                report.report.rows.len(),
                r.distance - r.required
            ),
            None => format!(
                "✖ Nothing checked: no structure with a body takes a rule ({} structure(s) not \
                 checked)",
                report.report.not_checked.len()
            ),
        };
        if check_ui::verdict(ui, failed > 0 || worst.is_none(), &text, failed > 0)
            && let Some(r) = worst
        {
            out.go = Some(Selection::Structure(r.structure.clone()));
        }
        // Every note visible; those not on the safe side as warnings.
        for n in &report.report.notes {
            let text = egui::RichText::new(n).small();
            ui.label(if n.contains("NOT ") || n.contains("unsafe side") {
                text.color(ui.visuals().warn_fg_color)
            } else {
                text.weak()
            });
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
        // Every row, sorted, filtered, grouped by the user; a row clicked
        // goes to its structure.
        use check_ui::Col;
        // Built once per answer, not every frame.
        let key = (self.job.generation, report.report.rows.len());
        let rows = self.rows.get_or_build(key, || table_rows(report));
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
        .rows_kept(rows)
        .group_by(&[0, 3, 4])
        .csv("internal-clearances-table.csv")
        .full_csv(
            "internal-clearances.csv",
            "Every row with every field (the member, the points on the part and on the member), \
             as ohl internal writes it",
            || report.report.csv(&report.rules),
        )
        .ui(ui)
        {
            out.go = Some(Selection::Structure(
                report.report.rows[k].structure.clone(),
            ));
        }
    }
}

/// The results table's rows: one per part × case × rule.
fn table_rows(report: &Report) -> Vec<check_ui::Row> {
    use check_ui::Cell;
    report
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
                    Cell::text(report.rules[r.rule].label()),
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
        .collect()
}

/// The three choices of a rule's insulation condition.
pub const INSULATION_ANY: &str = "any insulation";
/// Only the systems of low insulation.
pub const INSULATION_LOW: &str = "low insulation only";
/// Only the systems of high insulation.
pub const INSULATION_HIGH: &str = "high insulation only";

/// What the condition does to the results, on hover.
pub const INSULATION_HELP: &str = "Which conductors the rule applies to, by their system's \
     insulation level (Line › Sections › Electrical systems, column insulation; a system that \
     gives none is high). Any (the default): every conductor, as before. Low / high only: the \
     rule checks the conductors, jumpers, string ends and shapes of the systems of that \
     level and no others — give a second rule for the other level (e.g. a code's table with \
     a low and a high row per voltage).\n\nEffect: a conductor is measured against the \
     distance of the rule of its own level; a circuit that only rules of the other level \
     would take is NOT CHECKED, and the results say so (not on the safe side). No effect on \
     the time.";

/// The rule's insulation condition: any, low only or high only.
fn insulation_ui(ui: &mut egui::Ui, r: &mut InternalRule) -> bool {
    use ohl_line::electrical::InsulationLevel;
    let before = r.insulation;
    ui.horizontal(|ui| {
        for (value, label) in [
            (None, INSULATION_ANY),
            (Some(InsulationLevel::Low), INSULATION_LOW),
            (Some(InsulationLevel::High), INSULATION_HIGH),
        ] {
            ui.radio_value(&mut r.insulation, value, label)
                .on_hover_text(INSULATION_HELP);
        }
    });
    r.insulation != before
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
        if let Some(a) = &self.altitude {
            distance = format!("{distance}; {}", a.describe());
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

    /// The rule file keeps the window's rules and Use local conditions and reads them
    /// back; the bodies of the typed example line are placed for its
    /// structures with one (detail 2 or 3).
    #[test]
    fn rules_and_bodies() {
        let w = InternalClearancesWindow {
            rules: vec![InternalRule::blank("a")],
            local: true,
            ..Default::default()
        };
        let text = w.rule_set().to_toml();
        let back = RuleSet::parse(&text).unwrap();
        assert_eq!(back.internal_rules.len(), 1);
        assert!(back.check.unwrap().local);
        let m = crate::tests::typed_project("internal-window");
        let bodies = structure_bodies(&m);
        for b in &bodies {
            let st = m.line.structure(&b.structure).unwrap();
            assert!(st.structure_type.is_some());
            assert!(!b.members.is_empty());
        }
    }

    /// The weather case file of the old wind study (2026-10-25): a group
    /// "short" with the column `p.wind_study` and a least of 300 Pa.
    /// INVENTED.
    pub(super) fn old_groups(m: &mut Model) {
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
    }

    /// A wind study as a local condition (owner 2026-11-04: "make the wind
    /// study local"; ESTI 240 Art. 33): a structure with a body and a
    /// `wind_study` is checked in the cases with wind at that pressure, a
    /// value under the check's least raised to it (said); the cases
    /// without wind, or not listed, do not take it; without any structure
    /// giving one, nothing.
    #[test]
    fn wind_study_per_structure() {
        let mut m = crate::tests::typed_project("internal-wind-study");
        let bodies = structure_bodies(&m);
        assert!(bodies.len() >= 2, "the typed line has bodies");
        let (a, b) = (bodies[0].structure.clone(), bodies[1].structure.clone());
        let cases: Vec<ohl_weather::WeatherCase> = {
            let mut w = ohl_weather::WeatherCase::still(
                "wind",
                ohl_core::units::Temperature::from_celsius(10.0),
            );
            w.wind_pressure = ohl_core::units::Stress::from_si(650.0);
            vec![
                w,
                ohl_weather::WeatherCase::still(
                    "rest",
                    ohl_core::units::Temperature::from_celsius(10.0),
                ),
            ]
        };
        let check = ohl_clearance::rules::CheckScope {
            local: true,
            wind_study_min: Some(300.0),
            ..Default::default()
        };
        assert_eq!(
            wind_study(&m.line, &bodies, &cases, &check),
            WindStudy::default()
        );
        for st in &mut m.line.structures {
            if st.id == a {
                st.local.wind_study = Some(450.0);
            } else if st.id == b {
                st.local.wind_study = Some(200.0);
            }
        }
        let s = wind_study(&m.line, &bodies, &cases, &check);
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
        // Only the cases listed: none with wind among them, nothing.
        let listed = ohl_clearance::rules::CheckScope {
            wind_study_cases: vec!["rest".into()],
            ..check
        };
        let none = wind_study(&m.line, &bodies, &cases, &listed);
        assert!(none.at.is_empty() && none.cases.is_empty());
        assert!(none.notes[0].contains("not taken"), "{:?}", none.notes);
    }

    /// The old columns read for the conversion: each structure's value in
    /// the group's column, the least and the cases; the structures whose
    /// local wind study differs are pending; the note says the groups'
    /// columns are no longer read.
    #[test]
    fn the_groups_wind_study_as_read_for_the_conversion() {
        let mut m = crate::tests::typed_project("internal-wind-groups");
        old_groups(&mut m);
        let ids: Vec<String> = m.line.structures.iter().map(|s| s.id.clone()).collect();
        m.line.structures[0]
            .fields
            .insert("p.wind_study".into(), "450 Pa".into());
        m.line.structures[1]
            .fields
            .insert("p.wind_study".into(), "high".into());
        let groups = crate::case_groups::groups(&m);
        let study = GroupStudy::of(&m.line, &groups);
        assert_eq!(study.values, [(ids[0].clone(), 450.0)]);
        assert_eq!(study.min, Some(300.0));
        assert_eq!(study.cases, ["wind"]);
        assert!(
            study.notes[0].contains("not a pressure"),
            "{:?}",
            study.notes
        );
        assert_eq!(
            study.pending(&m.line).collect::<Vec<_>>(),
            [ids[0].as_str()]
        );
        assert!(
            group_study_note(&groups)
                .unwrap()
                .contains("no longer read")
        );
        m.line.structures[0].local.wind_study = Some(450.0);
        assert_eq!(study.pending(&m.line).count(), 0);
        assert!(study.command(&m.line).unwrap().is_none());
    }
}

#[cfg(test)]
mod app_tests {
    use eframe::egui;
    use egui_kittest::{Harness, kittest::Queryable};

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

    /// Every section the window's **user guide ↗** buttons open is a
    /// heading of its own page (mdBook's anchors: lower case, spaces as
    /// `-`, punctuation dropped; 2026-11-12).
    #[test]
    fn each_guide_button_opens_a_heading_of_the_page() {
        let text = std::fs::read_to_string(
            crate::tests::root().join("docs/user-guide/internal-clearances.md"),
        )
        .unwrap();
        let headings: Vec<String> = text
            .lines()
            .filter_map(|l| l.strip_prefix('#'))
            .map(|h| {
                h.trim_start_matches('#')
                    .trim()
                    .to_lowercase()
                    .chars()
                    .filter_map(|c| match c {
                        ' ' => Some('-'),
                        c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
                        _ => None,
                    })
                    .collect()
            })
            .collect();
        for a in [
            anchors::WHAT,
            anchors::RULES,
            anchors::SCOPE,
            anchors::MODEL,
            anchors::RESULTS,
        ] {
            let (page, s) = guide(a);
            assert_eq!(page, "user-guide/internal-clearances");
            assert!(headings.iter().any(|x| x == s), "{s} not in {headings:?}");
        }
    }

    /// The window's first **user guide ↗** (1 What it checks) opens its
    /// own page, not the OHL-Design page's section.
    #[test]
    fn the_user_guide_button_opens_the_window_page() {
        let model = crate::tests::typed_project("internal-guide");
        let mut h = Harness::builder()
            .with_size(egui::vec2(1500.0, 950.0))
            .build_eframe(|_| crate::app::App::with_model(model, None));
        h.run();
        {
            let r = h.state_mut().ready().unwrap();
            r.internal_clearances.show(&r.model);
        }
        h.run();
        h.query_all_by_label("user guide ↗")
            .next()
            .expect("a user guide button")
            .click();
        h.run();
        let r = h.state_mut().ready().unwrap();
        let (said, error) = r.status.clone().expect("what was opened");
        assert!(!error, "{said}");
        assert!(said.contains("internal-clearances"), "{said}");
        assert!(said.contains(anchors::WHAT), "{said}");
    }

    /// Runs the window's check on `h`'s model; its report.
    fn run(h: &mut Harness<'_, crate::app::App>, local: bool) -> InternalReport {
        let ctx = h.ctx.clone();
        {
            let r = h.state_mut().ready().unwrap();
            r.internal_clearances.show(&r.model);
            let mut rule = InternalRule::blank("invented 50 m");
            rule.distance = ohl_clearance::Requirement::Fixed(50.0);
            r.internal_clearances.rules = vec![rule];
            r.internal_clearances.local = local;
            r.internal_clearances.job.result = None;
            let w = &mut r.internal_clearances;
            w.start(&r.model, &ctx);
        }
        let started = std::time::Instant::now();
        loop {
            h.step();
            let r = h.state_mut().ready().unwrap();
            if let Some(res) = &r.internal_clearances.job.result {
                return res.as_ref().unwrap().report.clone();
            }
            assert!(started.elapsed().as_secs() < 120, "no report in 120 s");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// A structure's wind study (owner 2026-11-04: "make the wind study
    /// local"): with Use local conditions ticked, its rows in the cases with
    /// wind are of a run at its pressure, marked; unticked, none (said).
    #[test]
    fn the_wind_study_is_taken_with_the_switch_only() {
        let mut model = crate::tests::typed_project("internal-study-run");
        let bodies = structure_bodies(&model);
        let id = bodies[0].structure.clone();
        model
            .line
            .structures
            .iter_mut()
            .find(|s| s.id == id)
            .unwrap()
            .local
            .wind_study = Some(450.0);
        let windy = model.cases.iter().any(|c| c.case.wind_pressure.si() > 0.0);
        assert!(windy, "the example has a case with wind");
        let mut h = Harness::builder()
            .with_size(egui::vec2(1500.0, 950.0))
            .build_eframe(|_| crate::app::App::with_model(model, None));
        h.run();
        let ticked = run(&mut h, true);
        let studied: Vec<_> = ticked
            .rows
            .iter()
            .filter(|r| r.case.ends_with("(wind study 450 Pa)"))
            .collect();
        assert!(!studied.is_empty());
        assert!(studied.iter().all(|r| r.structure == id));
        let unticked = run(&mut h, false);
        assert!(!unticked.rows.iter().any(|r| r.case.contains("wind study")));
        assert!(
            unticked
                .notes
                .iter()
                .any(|n| n.contains("wind study of 1 structure(s) not taken")),
            "{:?}",
            unticked.notes
        );
    }

    /// The conversion (2026-11-04): the groups' column values copied into
    /// the structures' local wind study as one change, the groups' least
    /// and cases into the check; undone at once.
    #[test]
    fn the_conversion_copies_the_values_and_is_undone() {
        let mut model = crate::tests::typed_project("internal-study-convert");
        super::tests::old_groups(&mut model);
        let (a, b) = (
            model.line.structures[0].id.clone(),
            model.line.structures[1].id.clone(),
        );
        model.line.structures[0]
            .fields
            .insert("p.wind_study".into(), "450 Pa".into());
        model.line.structures[1]
            .fields
            .insert("p.wind_study".into(), "200".into());
        let mut h = Harness::builder()
            .with_size(egui::vec2(1500.0, 950.0))
            .build_eframe(|_| crate::app::App::with_model(model, None));
        h.run();
        let r = h.state_mut().ready().unwrap();
        r.internal_clearances.show(&r.model);
        let groups = crate::case_groups::groups(&r.model);
        let study = GroupStudy::of(&r.model.line, &groups);
        crate::app::clearance_panel::convert_group_study(r, &study);
        let at =
            |r: &crate::app::Ready, id: &str| r.model.line.structure(id).unwrap().local.wind_study;
        assert_eq!(at(r, &a), Some(450.0));
        assert_eq!(at(r, &b), Some(200.0));
        // The plugin's column kept.
        assert!(
            r.model
                .line
                .structure(&a)
                .unwrap()
                .fields
                .contains_key("p.wind_study")
        );
        assert_eq!(r.internal_clearances.wind_study_min, Some(300.0));
        assert_eq!(r.internal_clearances.wind_study_cases, ["wind"]);
        assert!(r.status.as_ref().unwrap().0.contains("2 structure(s)"));
        // Nothing left to copy.
        assert_eq!(
            GroupStudy::of(&r.model.line, &groups)
                .pending(&r.model.line)
                .count(),
            0
        );
        r.model.undo().unwrap();
        assert_eq!(at(r, &a), None);
        assert_eq!(at(r, &b), None);
    }
}
