// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Galloping ellipses: the ellipses of the wires of each span by
//! the methods of a galloping parameter file (`.ohlg`), in one weather
//! case, and the clearance between them ([`ohl_clearance::galloping`]).
//! Computed on demand in the background; worst pairs first; a click goes
//! to the span. Built as the check pages say (2026-11-15, the owner: "a
//! review and explanation page in the books for the galloping ellipses
//! check"): its own page of the user guide ([`GUIDE`]), the sections'
//! **user guide ↗** buttons, the explanations behind their (?), what the
//! file sets about the computation under *4 Model*.

use std::path::PathBuf;
use std::time::SystemTime;

use eframe::egui;
use ohl_clearance::galloping::{self, GallopingParams};

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
    "galloping",
    "the wires' sag and tension sizing the ellipses",
);

/// One pair of wires by one method.
#[derive(Debug, Clone)]
pub struct Row {
    /// Method name.
    pub method: String,
    /// First wire's span.
    pub a: (String, usize),
    /// Second wire's span.
    pub b: (String, usize),
    /// Smallest distance between the ellipses, m.
    pub distance: f64,
    /// Required, m.
    pub required: Option<f64>,
    /// Where, fraction of the span.
    pub at: f64,
    /// Both ellipses' axes at their largest (major, minor), m.
    pub axes: [[f64; 2]; 2],
}

/// What a check found.
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// Weather case.
    pub case: String,
    /// Notes (the parameters' cases, sections not solved).
    pub notes: Vec<String>,
    /// Where the check is not on the safe side
    /// ([`galloping::notes`]): written above the table, never folded.
    pub safety: Vec<String>,
    /// Pairs, worst first.
    pub rows: Vec<Row>,
}

/// The Galloping window.
#[derive(Default)]
pub struct GallopingWindow {
    /// Shown.
    pub open: bool,
    /// **Use local conditions** (owner, 2026-11-03): the wires with the structures' wind
    /// for strength and wire ice on their spans.
    pub local: bool,
    /// The parameter file.
    pub file: Option<PathBuf>,
    /// Weather case (index in the model's cases).
    pub case: usize,
    /// The circuits, routes and sections checked (owner, 2026-10-28),
    /// kept in the project ([`SETTING`]).
    pub scope: check_ui::Scope,
    kept: check_ui::Kept,
    pub(crate) job: Job<Report>,
    /// The results table's rows, by the answer's count and its rows.
    rows: check_ui::CachedRows<(u64, usize)>,
    /// The parameter file read (the file, when it was changed on disk,
    /// what it holds or why it is not read): its summary, *4 Model* and
    /// what blocks Run.
    params: Option<(PathBuf, Option<SystemTime>, Result<GallopingParams, String>)>,
    /// A section of the window's user guide page to open ([`guide`]).
    pub guide: Option<&'static str>,
}

/// The window's own page of the user guide (check pages' guidelines §6).
pub const GUIDE: &str = "user-guide/galloping";

/// The sections of [`GUIDE`] the window's **user guide ↗** buttons open
/// (each a heading of the page: tested).
pub mod anchors {
    /// *1 What it checks*.
    pub const WHAT: &str = "1-what-it-checks";
    /// *2 Scope*.
    pub const SCOPE: &str = "2-scope";
    /// *3 Rules: the parameter file*.
    pub const RULES: &str = "3-rules-the-parameter-file";
    /// *4 Model*: what the file sets about the computation.
    pub const MODEL: &str = "4-model-from-the-parameter-file-folded";
    /// The Results tab.
    pub const RESULTS: &str = "the-results";
}

/// The page and section a **user guide ↗** button opens.
pub fn guide(anchor: &'static str) -> (&'static str, &'static str) {
    (GUIDE, anchor)
}

/// The question, in *1 What it checks*: its line and the (?).
const WHAT_HELP: (&str, &str) = (
    "Ellipses around the wires of each span, sized from the sag by the coefficients of the \
     parameter file, and the clearance between them.",
    "Each method of the parameter file draws an ellipse around every wire of a span — the \
     space the iced wire may sweep when it gallops — its axes from the wire's sag in the \
     weather case chosen, the span length and the conductor diameter. Every two wires hanging \
     between the same two structures are paired; the smallest distance between their \
     ellipses along the span, less both wires' outer radii, is compared with the file's \
     requirement. OpenOHL ships no method: every coefficient is the file's.",
);

/// *2 Scope*'s (?).
const SCOPE_HELP: (&str, &str) = (
    "The weather case the ellipses are sized in, and the circuits, routes and sections checked.",
    "Weather case: the sags and the wires' positions at rest are those solved in it (the case \
     shown in the views when the window opens). Circuits: only the wires of the circuits \
     ticked are paired. Routes, alignments, sections: only the sections ticked, or with a \
     structure on a route or alignment ticked (none: the whole line). Fewer spans, a faster \
     run; the check itself stays well under a second.",
);

/// *3 Rules*' (?).
const RULES_HELP: (&str, &str) = (
    "The galloping parameter file (.ohlg): its methods, its requirement and the cases it names.",
    "A text file holding every coefficient of the methods you design to (OpenOHL ships none; \
     the example's numbers are invented). Each method draws one ellipse per wire: its major \
     axis A and minor axis B as sums of terms in the sag D, the span L, √D, the conductor \
     diameter d, a logarithmic term and a constant. The requirement is the distance two \
     ellipses must keep (none: the distances only). Edited outside the window; read again \
     when it changes on disk. Every field: the user guide.",
);

/// *4 Model*'s (?).
const MODEL_HELP: (&str, &str) = (
    "What the parameter file sets about how the check is computed (read-only).",
    "Stations: the ellipses compared at stations − 1 inner points along the span; a number \
     that is not a multiple of 2 × loops misses the middle of a loop, where the ellipse is \
     largest (said in the results). Calculation: the wires solved by this condition and \
     method (one more solve of the line). Pair's voltage: the voltage a requirement by \
     voltage is read at. Per method: the tilt of the major axis, the centre above the wire \
     at rest, and how the size varies along the span.",
);

/// The project setting the window's scope is kept in, per project line.
pub const SETTING: &str = "app.galloping";

impl GallopingWindow {
    /// Opens the window: a parameter file of the project (next to a line
    /// file), the case shown.
    pub fn show(&mut self, model: &Model, case: usize) {
        self.open = true;
        self.case = case;
        if self.file.is_none() {
            self.file = model.project_files("galloping").into_iter().next();
        }
    }

    /// The last check's report, and whether one runs.
    #[cfg(test)]
    pub fn report(&self) -> (Option<&Result<Report, String>>, bool) {
        (self.job.result.as_ref(), self.job.running())
    }

    /// Reads the scope from the project when not read yet for the line
    /// worked on, and writes it into it when it changed (project setting
    /// `app.galloping.<line>`, owner, 2026-10-28: the setups kept in the
    /// project).
    pub fn keep(&mut self, model: &mut Model, ctx: &egui::Context) {
        let json = self.kept_json();
        let mut kept = std::mem::take(&mut self.kept);
        if let check_ui::Step::Load(text) = kept.step(model, ctx, SETTING, "galloping scope", &json)
        {
            let value: Option<serde_json::Value> =
                text.as_deref().and_then(|t| serde_json::from_str(t).ok());
            self.scope = value
                .clone()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();
            // Use local conditions (2026-11-03) beside the scope's keys.
            self.local = value
                .as_ref()
                .and_then(|v| v.get("local"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            kept.loaded(self.kept_json());
        }
        self.kept = kept;
    }

    /// What is kept: the scope, and `local` when ticked.
    fn kept_json(&self) -> String {
        let mut v = serde_json::to_value(&self.scope).unwrap_or_default();
        if self.local
            && let Some(o) = v.as_object_mut()
        {
            o.insert("local".into(), true.into());
        }
        v.to_string()
    }

    fn start(&mut self, model: &Model, ctx: &egui::Context) {
        let Some(path) = self.file.clone() else {
            self.job.result = Some(Err("choose a galloping parameter file (.ohlg)".into()));
            return;
        };
        let Some(solved) = model.cases.get(self.case) else {
            return;
        };
        // The line with the local conditions when ticked: the case
        // solved again on it (else the model's solution).
        let local = crate::local_solve::prepare(&model.line, self.local, LOCAL.0);
        let library = model.library.clone();
        let shapes = solved.shapes.clone();
        let weather = solved.case.clone();
        // The sag of swung wires (said in the results).
        let windy = weather.wind_pressure.si() > 0.0;
        let wind = model.wind;
        let case = solved.case.name.clone();
        let failed = solved.failed.clone();
        // The scope's sections only (none: the whole line).
        let keep = self.scope.keeps(&model.line);
        self.job.start_at(ctx, model.revision(), move || {
            let started = std::time::Instant::now();
            let line = local.line();
            let params = GallopingParams::load(&path)?;
            let mut notes = local.notes();
            let (mut shapes, failed) = if local.applied() && params.calculation.is_none() {
                let s = local.solved(&library, weather.clone());
                (s.shapes, s.failed)
            } else {
                (shapes, failed)
            };
            notes.extend(failed.iter().map(|f| format!("not solved: {f}")));
            if !params.applies(&case) {
                notes.push(format!(
                    "the parameters name the cases {:?}; checked in \"{case}\"",
                    params.cases
                ));
            }
            // The parameters' calculation (2026-11-01): the case solved
            // anew by it.
            if params.calculation.is_some() {
                let l = ohl_clearance::calculation::line_for(line, params.calculation);
                let s = crate::model::Solved::with_wind(&l, &library, weather, wind);
                notes.extend(s.failed.iter().map(|f| format!("not solved: {f}")));
                shapes = s.shapes;
            }
            let mut wires = galloping::galloping_wires(line, &shapes, &library);
            wires.retain(|w| keep.as_ref().is_none_or(|k| k.contains(&w.wire.section)));
            // Exit clause: well under a second for any line.
            let budget =
                ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(300));
            let mut pairs = galloping::check(&params, &wires, &budget)?;
            let short = |p: &galloping::GallopingPair| p.distance - p.required.unwrap_or(0.0);
            pairs.sort_by(|a, b| short(a).total_cmp(&short(b)));
            local.finish(LOCAL.1, started.elapsed());
            let safety = galloping::notes(&params, &wires, &pairs, windy);
            let span = |k: usize| (wires[k].wire.section.clone(), wires[k].wire.span);
            Ok(Report {
                case,
                notes,
                safety,
                rows: pairs
                    .iter()
                    .map(|p| Row {
                        method: params.methods[p.method].name.clone(),
                        a: span(p.a),
                        b: span(p.b),
                        distance: p.distance,
                        required: p.required,
                        at: p.at,
                        axes: p.axes,
                    })
                    .collect(),
            })
        });
    }

    /// What the last check was computed from that changed since, in
    /// words (`None`: nothing, or no report): the line, its weather cases
    /// and files ([`crate::revision`]).
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        self.job.out_of_date(model, USES)
    }

    /// The window; returns a span to go to.
    pub fn ui(&mut self, ctx: &egui::Context, model: &Model) -> Option<Selection> {
        self.job.poll();
        let mut go = None;
        let mut again = false;
        let stale = self.out_of_date(model);
        let mut open = self.open;
        // The parameter file read again when it changed on disk: what
        // blocks Run is said before it (check pages' guidelines §3).
        self.read_params();
        let file_why = match (&self.file, &self.params) {
            (None, _) => Some("choose a galloping parameter file (.ohlg) in 3 Rules".to_owned()),
            (Some(_), Some((_, _, Err(_)))) => {
                Some("the parameter file is not read: see 3 Rules".to_owned())
            }
            _ => None,
        };
        let why = (!model.cases.is_empty())
            .then_some(())
            .map_or(Some("the model has no weather case".to_owned()), |_| None)
            .or(file_why)
            .or_else(|| self.scope.why_not(&model.line));
        // The case chosen not among the file's (the command line runs
        // only those): said before Run.
        let case_name = model.cases.get(self.case).map(|c| c.case.name.clone());
        let other_case = match (&self.params, &case_name) {
            (Some((_, _, Ok(p))), Some(c)) if !p.applies(c) => Some(format!(
                "\"{c}\" is not one of the cases the parameter file names ({}): the check runs \
                 in it all the same",
                p.cases.join(", ")
            )),
            _ => None,
        };
        let mut guide = None;
        let scope_words = check_ui::scope_words(&model.line, &self.scope, true);
        // The case is solved already: the time goes with the spans checked.
        let (sections, spans) = check_ui::scope_counts(&model.line, &self.scope);
        let plan = check_ui::Plan::new(
            format!(
                "{} ({}), the ellipses of each span",
                check_ui::count(sections, "section"),
                check_ui::count(spans, "span")
            ),
            spans,
        );
        let findings = match &self.job.result {
            Some(Ok(r)) => Some(r.rows.iter().filter(|r| short(r)).count()),
            _ => None,
        };
        help::window(
            ctx,
            crate::dockable::Panel::Galloping,
            "Galloping ellipses",
            [860.0, 760.0],
            &mut open,
            |ui| {
                if let Some(what) = &stale {
                    again = crate::revision::banner(ui, what, !self.job.running());
                }
                // Two tabs (owner, 2026-10-27): the setup scrolls, the
                // results fill theirs; Run in the footer, always visible.
                let tab = check_ui::tabs(
                    ui,
                    WINDOW,
                    findings,
                    self.job.generation,
                    why.is_some() && !self.job.running(),
                );
                match tab {
                    check_ui::Tab::Setup => help::body(ui, WINDOW, |ui| {
                        // The check pages' treatment (2026-11-15): the
                        // grey lines in the (?) of their section; what
                        // blocks Run and the warnings stay written.
                        guide = guide.or(check_ui::section(
                            ui,
                            1,
                            "What it checks",
                            Some(anchors::WHAT),
                        ));
                        help::explain(ui, WHAT_HELP.0, WHAT_HELP.1);
                        check_ui::how_computed(
                            ui,
                            WINDOW,
                            "The weather case's sags; per wire and method an ellipse whose axes \
                             A and B come from the sag by the method's coefficients; per pair \
                             of wires between the same two structures the shortest distance \
                             between their ellipses along the span, less both wires' outer \
                             radii.",
                            help::galloping_ellipses,
                        );
                        ui.separator();
                        guide = guide.or(check_ui::section_help(
                            ui,
                            2,
                            "Scope",
                            Some(anchors::SCOPE),
                            Some(SCOPE_HELP),
                        ));
                        let names: Vec<String> =
                            model.cases.iter().map(|c| c.case.name.clone()).collect();
                        check_ui::ScopePicker::new(WINDOW, &model.line)
                            .route_of(model)
                            .wires(&mut self.scope, true)
                            .cases(check_ui::Cases::One(&names, &mut self.case))
                            .ui(ui);
                        if let Some(w) = &other_case {
                            ui.label(
                                egui::RichText::new(w)
                                    .small()
                                    .color(ui.visuals().warn_fg_color),
                            );
                        }
                        ui.separator();
                        guide = guide.or(check_ui::section_help(
                            ui,
                            3,
                            "Rules: the parameter file",
                            Some(anchors::RULES),
                            Some(RULES_HELP),
                        ));
                        self.file_ui(ui);
                        ui.separator();
                        // Rarely looked at: folded (check pages' §1).
                        let model_guide = check_ui::model_section(ui, WINDOW, 4, |ui| {
                            let g = ui
                                .horizontal(|ui| {
                                    let g = help::heading(
                                        ui,
                                        "From the parameter file",
                                        anchors::MODEL,
                                    );
                                    help::question_both(ui, MODEL_HELP.0, MODEL_HELP.1);
                                    g
                                })
                                .inner;
                            self.model_ui(ui);
                            g
                        });
                        guide = guide.or(model_guide.flatten());
                    }),
                    check_ui::Tab::Results => check_ui::results_area(ui, WINDOW, |ui| {
                        guide = guide.or(help::heading(ui, "Results", anchors::RESULTS));
                        let report = match &self.job.result {
                            None => {
                                ui.label(
                                    egui::RichText::new("No run yet: Run at the bottom.").weak(),
                                );
                                return;
                            }
                            Some(Err(e)) => {
                                crate::menu::error_label(ui, e);
                                return;
                            }
                            Some(Ok(r)) => r,
                        };
                        let (failing, text) = verdict(report);
                        check_ui::verdict(ui, failing, &text, false);
                        // Where it is not on the safe side: always
                        // written (check pages' §6); what was assumed and
                        // left out folded above the table.
                        for n in &report.safety {
                            ui.label(
                                egui::RichText::new(n)
                                    .small()
                                    .color(ui.visuals().warn_fg_color),
                            );
                        }
                        check_ui::notes_fold(ui, WINDOW, &report.notes);
                        use check_ui::Col;
                        // Built once per answer, not every frame.
                        let key = (self.job.generation, report.rows.len());
                        let rows = self.rows.get_or_build(key, || table_rows(report));
                        if let Some(k) = check_ui::ResultTable::new(
                            "galloping-rows",
                            vec![
                                Col::text("Method"),
                                Col::text("Span"),
                                Col::text("Other span"),
                                Col::num("Distance [m]").tip(
                                    "The smallest distance between the two wires' ellipses \
                                     along the span, less both wires' outer radii (bundle, \
                                     conductor — iced with ice — and the polyline's bound); \
                                     0: they overlap",
                                ),
                                Col::num("Required [m]").tip(
                                    "The parameter file's requirement at the pair's voltage \
                                     (empty: the file gives none)",
                                ),
                                Col::num("At [%]").tip(
                                    "Where along the first wire's span, from its first \
                                     structure",
                                ),
                                Col::text("Ellipse [m]")
                                    .tip("The first wire's ellipse at its largest: A × B"),
                                Col::text("Other ellipse [m]")
                                    .tip("The second wire's ellipse at its largest: A × B"),
                            ],
                        )
                        .rows_kept(rows)
                        .group_by(&[0, 1])
                        .csv("galloping-table.csv")
                        .full_csv(
                            "galloping.csv",
                            "Every pair with the weather case, the spans' 0-based indices and \
                             where along the span as a fraction",
                            || csv(report),
                        )
                        .ui(ui)
                        {
                            let r = &report.rows[k];
                            go = Some(Selection::Span {
                                section: r.a.0.clone(),
                                span: r.a.1,
                            });
                        }
                    }),
                }
                help::footer(ui, WINDOW, |ui| {
                    let local = &mut self.local;
                    again |= check_ui::job_footer(
                        ui,
                        &mut self.job,
                        why.clone(),
                        "Size the ellipses of every span by the parameter file in the case chosen \
                         and check their clearances",
                        Some(scope_words),
                        Some(plan),
                        |ui| {
                            let _ = crate::local_solve::switch(
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
        self.guide = self.guide.or(guide);
        if again && why.is_none() {
            self.start(model, ctx);
        }
        go
    }

    /// Reads the parameter file with the check's own reader: once per
    /// file chosen, and again when it changed on disk (its modification
    /// time; a stat per frame).
    fn read_params(&mut self) {
        let Some(path) = self.file.clone() else {
            self.params = None;
            return;
        };
        let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if self
            .params
            .as_ref()
            .is_none_or(|(p, m, _)| *p != path || *m != modified)
        {
            let read = GallopingParams::load(&path);
            self.params = Some((path, modified, read));
        }
    }

    /// *3 Rules*: the file and **Choose…**, then what it holds — its
    /// name, requirement and cases, a row per method with its axes in
    /// words — or why it is not read, in the error colour.
    fn file_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let name = self.file.as_ref().map_or("none chosen".into(), |p| {
                p.file_name()
                    .map_or(String::new(), |n| n.to_string_lossy().into_owned())
            });
            ui.label("Parameter file").on_hover_text(
                "The .ohlg file whose methods draw the ellipses and whose requirement they are \
                 checked against; another file, other ellipses",
            );
            ui.strong(name);
            if ui
                .button("Choose…")
                .on_hover_text(
                    "Another galloping parameter file (.ohlg): the next Run draws its methods' \
                     ellipses and checks its requirement",
                )
                .clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter(
                        "Galloping parameters",
                        &[ohl_clearance::galloping::GALLOPING_FILE_EXTENSION],
                    )
                    .pick_file()
            {
                self.file = Some(p);
                self.read_params();
            }
        });
        let Some((_, _, read)) = &self.params else {
            return;
        };
        let p = match read {
            Ok(p) => p,
            Err(e) => {
                crate::menu::error_label(ui, format!("not read: {e}"));
                return;
            }
        };
        egui::Grid::new((WINDOW, "file"))
            .num_columns(2)
            .spacing(help::GRID_SPACING)
            .show(ui, |ui| {
                if !p.name.is_empty() {
                    ui.label("Name");
                    ui.label(&p.name);
                    ui.end_row();
                }
                ui.label("Required").on_hover_text(
                    "The distance two ellipses must keep (the file's required); a pair below it \
                     is a violation. None: the distances only, no verdict per pair",
                );
                ui.label(p.required.as_ref().map_or(
                    "no requirement: distances only".to_owned(),
                    crate::rule_table::distance_words,
                ));
                ui.end_row();
                ui.label("Cases").on_hover_text(
                    "The weather cases the file says its wires gallop in (its cases); the \
                     window runs in the case chosen in 2 Scope, the command line only in these",
                );
                ui.label(if p.cases.is_empty() {
                    "every case".to_owned()
                } else {
                    p.cases.join(", ")
                });
                ui.end_row();
            });
        egui::Grid::new((WINDOW, "methods"))
            .num_columns(4)
            .striped(true)
            .spacing(help::GRID_SPACING)
            .show(ui, |ui| {
                ui.strong("Method");
                ui.strong("Loops").on_hover_text(
                    "The number of loops n in the span: with along = sine, each loop's ellipse is \
                     largest in its middle and nothing at the loops' ends",
                );
                ui.strong("Major axis A [m]").on_hover_text(
                    "The ellipse's full major axis, from the sag D, the span L, the conductor \
                     diameter d and n: a larger A, a larger ellipse, a smaller clearance",
                );
                ui.strong("Minor axis B [m]").on_hover_text(
                    "The ellipse's full minor axis: the same terms and a share of A",
                );
                ui.end_row();
                for m in &p.methods {
                    ui.label(&m.name);
                    ui.label(m.loops.to_string());
                    ui.label(format!("A = {}", axis_words(&m.major)));
                    ui.label(format!("B = {}", axis_words(&m.minor)));
                    ui.end_row();
                }
            });
    }

    /// *4 Model*: what the file sets about how the check is computed,
    /// read-only, each with its effect on hover.
    fn model_ui(&self, ui: &mut egui::Ui) {
        let Some((_, _, Ok(p))) = &self.params else {
            ui.label(egui::RichText::new("no parameter file read").small().weak());
            return;
        };
        egui::Grid::new((WINDOW, "model"))
            .num_columns(2)
            .spacing(help::GRID_SPACING)
            .show(ui, |ui| {
                ui.label("Stations").on_hover_text(
                    "The ellipses are compared at stations − 1 inner points along the span: more, \
                     the smallest gap found more closely, in proportion more time (well under a \
                     second for a whole line); a multiple of 2 × loops compares every loop's \
                     largest ellipse",
                );
                ui.label(format!("{} ({} inner points)", p.stations, p.stations - 1));
                ui.end_row();
                ui.label("Calculation").on_hover_text(
                    "How the wires are solved for their sags and positions: the file's condition \
                     and method (one more solve of the line in the case), else each section's own",
                );
                ui.label(
                    p.calculation
                        .map_or("each section's own".to_owned(), |c| c.label()),
                );
                ui.end_row();
                ui.label("Pair's voltage").on_hover_text(
                    "The voltage a requirement by voltage is read at: the higher of the two \
                     wires' nominal voltages, their sum, or the voltage between them",
                );
                ui.label(match p.pair_voltage {
                    ohl_clearance::rules::PairVoltage::Higher => "the higher",
                    ohl_clearance::rules::PairVoltage::Sum => "the sum",
                    ohl_clearance::rules::PairVoltage::Difference => "the difference",
                });
                ui.end_row();
            });
        egui::Grid::new((WINDOW, "model-methods"))
            .num_columns(4)
            .striped(true)
            .spacing(help::GRID_SPACING)
            .show(ui, |ui| {
                ui.strong("Method");
                ui.strong("Tilt [°]").on_hover_text(
                    "The major axis from the vertical, its top to the right looking from the \
                     span's first structure to its last: tilted, the ellipses lean towards a \
                     neighbour on that side",
                );
                ui.strong("Centre above the wire [× A]").on_hover_text(
                    "The ellipse's centre along the major axis above the wire at rest: 0.5 puts \
                     the wire at the lower end, the ellipse reaching up towards a wire above",
                );
                ui.strong("Along the span").on_hover_text(
                    "sine: the size × |sin(nπt)|, largest in each loop's middle, nothing at the \
                     supports; constant: the full size all along (larger, a smaller clearance \
                     near the structures)",
                );
                ui.end_row();
                for m in &p.methods {
                    ui.label(&m.name);
                    ui.label(format!("{}", m.tilt_deg));
                    ui.label(format!("{}", m.centre_above_rest));
                    ui.label(match m.along {
                        galloping::Along::Sine => "sine",
                        galloping::Along::Constant => "constant",
                    });
                    ui.end_row();
                }
            });
    }

    /// The weather case of the results shown (a row clicked is drawn in
    /// it).
    pub fn result_case(&self) -> Option<String> {
        match &self.job.result {
            Some(Ok(r)) => Some(r.case.clone()),
            _ => None,
        }
    }
}

/// The id of the window's body, tabs and footer.
const WINDOW: &str = "galloping-window";

/// The results table's rows: one per pair × method.
fn table_rows(report: &Report) -> Vec<check_ui::Row> {
    use check_ui::Cell;
    report
        .rows
        .iter()
        .map(|r| {
            check_ui::Row::new(
                vec![
                    Cell::text(&r.method),
                    Cell::span(&r.a.0, r.a.1),
                    Cell::span(&r.b.0, r.b.1),
                    Cell::num(Some(r.distance), 2).bad(short(r)),
                    Cell::num(r.required, 2),
                    Cell::num(Some(r.at * 100.0), 0),
                    Cell::sorted(
                        format!("{:.2} × {:.2}", r.axes[0][0], r.axes[0][1]),
                        r.axes[0][0],
                    ),
                    Cell::sorted(
                        format!("{:.2} × {:.2}", r.axes[1][0], r.axes[1][1]),
                        r.axes[1][0],
                    ),
                ],
                short(r),
            )
        })
        .collect()
}

/// The verdict: whether it fails, and in words — the pairs below the
/// requirement, every pair at it, or (no requirement in the file) the
/// smallest distance found.
fn verdict(report: &Report) -> (bool, String) {
    let n = report.rows.iter().filter(|r| short(r)).count();
    let s = |k: usize| if k == 1 { "" } else { "s" };
    if n > 0 {
        return (
            true,
            format!(
                "✖ {n} pair{} × method below the requirement in case \"{}\"",
                s(n),
                report.case
            ),
        );
    }
    if report.rows.iter().all(|r| r.required.is_none()) {
        let least = report.rows.iter().map(|r| r.distance).reduce(f64::min);
        return (
            false,
            match least {
                Some(d) => format!(
                    "No requirement in the parameter file: distances only — the smallest \
                     {d:.2} m in case \"{}\" ({} pair{} × methods)",
                    report.case,
                    report.rows.len(),
                    s(report.rows.len())
                ),
                None => format!("No pair of wires to compare in case \"{}\"", report.case),
            },
        );
    }
    (
        false,
        format!(
            "✔ Every pair at its requirement in case \"{}\" ({} pairs × methods)",
            report.case,
            report.rows.len()
        ),
    )
}

/// An axis of a method in words: its terms (`1.2 D + 0.3 m`, `0.4 A`;
/// none: `0`).
pub(crate) fn axis_words(a: &galloping::AxisFormula) -> String {
    let num = |x: f64| format!("{}", (x * 1e6).round() / 1e6);
    let mut terms = Vec::new();
    for (k, what) in [
        (a.sag, "D"),
        (a.span, "L"),
        (a.sqrt_sag, "√D"),
        (a.diameter, "d"),
        (a.major, "A"),
    ] {
        if k != 0.0 {
            terms.push(format!("{} {what}", num(k)));
        }
    }
    if a.log_factor != 0.0 {
        terms.push(format!(
            "{} n d ln({} D / (n² d))",
            num(a.log_factor),
            num(a.log_scale)
        ));
    }
    if a.constant != 0.0 {
        terms.push(format!("{} m", num(a.constant)));
    }
    if terms.is_empty() {
        "0".to_owned()
    } else {
        terms.join(" + ")
    }
}

/// A pair below its requirement.
fn short(r: &Row) -> bool {
    r.required.is_some_and(|q| r.distance < q)
}

fn csv(r: &Report) -> String {
    use std::fmt::Write as _;
    let mut out = String::from(
        "case,method,section,span,other_section,other_span,distance_m,required_m,at_fraction\n",
    );
    for row in &r.rows {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{:.3},{},{:.3}",
            ohl_core::text::csv_field(&r.case),
            ohl_core::text::csv_field(&row.method),
            ohl_core::text::csv_field(&row.a.0),
            row.a.1,
            ohl_core::text::csv_field(&row.b.0),
            row.b.1,
            row.distance,
            row.required.map_or(String::new(), |v| format!("{v:.3}")),
            row.at
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    /// Every section the window's **user guide ↗** buttons open is a
    /// heading of its own page (mdBook's anchors: lower case, spaces as
    /// `-`, punctuation dropped).
    #[test]
    fn each_guide_button_opens_a_heading_of_the_page() {
        let text =
            std::fs::read_to_string(crate::tests::root().join("docs/user-guide/galloping.md"))
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
            anchors::SCOPE,
            anchors::RULES,
            anchors::MODEL,
            anchors::RESULTS,
        ] {
            let (page, s) = guide(a);
            assert_eq!(page, "user-guide/galloping");
            assert!(headings.iter().any(|x| x == s), "{s} not in {headings:?}");
        }
    }

    /// The window's first **user guide ↗** (1 What it checks) opens its
    /// own page.
    #[test]
    fn the_user_guide_button_opens_the_window_page() {
        let model = crate::tests::synthetic();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1500.0, 950.0))
            .build_eframe(|_| crate::app::App::with_model(model, None));
        h.run();
        {
            let r = h.state_mut().ready().unwrap();
            r.galloping.show(&r.model, r.case);
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
        assert!(said.contains("galloping"), "{said}");
        assert!(said.contains(anchors::WHAT), "{said}");
    }

    /// Without a parameter file, or with one that is not read, Run is
    /// greyed and the footer says why (before 2026-11-15 Run failed);
    /// a case the file does not name is said before Run; the file's
    /// methods are listed with their axes in words.
    #[test]
    fn what_blocks_run_and_the_file_said_before_run() {
        let model = crate::tests::synthetic();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1500.0, 950.0))
            .build_eframe(|_| crate::app::App::with_model(model, None));
        h.run();
        {
            let r = h.state_mut().ready().unwrap();
            r.galloping.show(&r.model, r.case);
            r.galloping.file = None;
        }
        h.run();
        assert!(
            h.query_all_by_label_contains("choose a galloping parameter file")
                .next()
                .is_some()
        );
        let bad = std::env::temp_dir().join(format!("galloping-bad-{}.ohlg", std::process::id()));
        std::fs::write(&bad, "name = \"no method\"\n").unwrap();
        h.state_mut().ready().unwrap().galloping.file = Some(bad.clone());
        h.run();
        assert!(
            h.query_all_by_label_contains("the parameter file is not read")
                .next()
                .is_some()
        );
        assert!(
            h.query_all_by_label_contains("give at least one [[method]]")
                .next()
                .is_some()
        );
        // The example file, read again once changed on disk: its methods
        // in words; the case shown, if not a glaze case, said.
        let example = crate::tests::root().join("examples/synthetic-line/galloping-example.ohlg");
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&bad, std::fs::read_to_string(&example).unwrap()).unwrap();
        h.run();
        let _ = std::fs::remove_file(&bad);
        assert!(h.query_all_by_label("A = 1.2 D + 0.3 m").next().is_some());
        assert!(h.query_all_by_label("B = 0.4 A").next().is_some());
        let r = h.state_mut().ready().unwrap();
        let named = r.model.cases[r.galloping.case].case.name.clone();
        if !named.starts_with("glaze") {
            assert!(
                h.query_all_by_label_contains("is not one of the cases the parameter file names")
                    .next()
                    .is_some()
            );
        }
    }

    #[test]
    fn axes_in_words() {
        let a = galloping::AxisFormula {
            sag: 1.25,
            constant: 0.3,
            ..Default::default()
        };
        assert_eq!(axis_words(&a), "1.25 D + 0.3 m");
        assert_eq!(axis_words(&galloping::AxisFormula::default()), "0");
        let l = galloping::AxisFormula {
            log_factor: 80.0,
            log_scale: 8.0,
            major: 0.4,
            ..Default::default()
        };
        assert_eq!(axis_words(&l), "0.4 A + 80 n d ln(8 D / (n² d))");
    }
}
