// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Galloping ellipses: the ellipses of the wires of each span by
//! the methods of a galloping parameter file (`.ohlg`), in one weather
//! case, and the clearance between them ([`ohl_clearance::galloping`]).
//! Computed on demand in the background; worst pairs first; a click goes
//! to the span.

use std::path::PathBuf;

use eframe::egui;
use ohl_clearance::galloping::{self, GallopingParams};

use crate::check_help as help;
use crate::check_ui;
use crate::job::{Job, save_csv};
use crate::model::Model;
use crate::selection::Selection;

/// What its results are computed from ([`crate::revision`]).
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::LINE;

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
    /// Pairs, worst first.
    pub rows: Vec<Row>,
}

/// The Galloping window.
#[derive(Default)]
pub struct GallopingWindow {
    /// Shown.
    pub open: bool,
    /// The structure and section overrides of its case.
    pub overrides: crate::case_groups::Overrides,
    /// The parameter file.
    pub file: Option<PathBuf>,
    /// Weather case (index in the model's cases).
    pub case: usize,
    /// The circuits, routes and sections checked (owner, 2026-10-28),
    /// kept in the project ([`SETTING`]).
    pub scope: check_ui::Scope,
    kept: check_ui::Kept,
    pub(crate) job: Job<Report>,
}

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
        let json = serde_json::to_string(&self.scope).unwrap_or_default();
        let mut kept = std::mem::take(&mut self.kept);
        if let check_ui::Step::Load(text) = kept.step(model, ctx, SETTING, "galloping scope", &json)
        {
            self.scope = text
                .as_deref()
                .and_then(|t| serde_json::from_str(t).ok())
                .unwrap_or_default();
            kept.loaded(serde_json::to_string(&self.scope).unwrap_or_default());
        }
        self.kept = kept;
    }

    fn start(&mut self, model: &Model, ctx: &egui::Context) {
        let Some(path) = self.file.clone() else {
            self.job.result = Some(Err("choose a galloping parameter file (.ohlg)".into()));
            return;
        };
        let Some(solved) = model.cases.get(self.case) else {
            return;
        };
        let line = model.line.clone();
        let library = model.library.clone();
        let shapes = solved.shapes.clone();
        let case = solved.case.name.clone();
        let failed = solved.failed.clone();
        let filter = self.overrides.filter(model);
        // The scope's sections only (none: the whole line).
        let keep = self.scope.keeps(&model.line);
        self.job.start_at(ctx, model.revision(), move || {
            let params = GallopingParams::load(&path)?;
            let mut notes: Vec<String> =
                failed.iter().map(|f| format!("not solved: {f}")).collect();
            if !params.applies(&case) {
                notes.push(format!(
                    "the parameters name the cases {:?}; checked in \"{case}\"",
                    params.cases
                ));
            }
            let mut wires = galloping::galloping_wires(&line, &shapes, &library);
            // The overrides: the spans that do not take the case left out.
            wires.retain(|w| {
                keep.as_ref().is_none_or(|k| k.contains(&w.wire.section))
                    && crate::case_groups::span_takes(
                        &line,
                        &filter,
                        &w.wire.section,
                        w.wire.span,
                        &case,
                    )
            });
            // Exit clause: well under a second for any line.
            let budget =
                ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(300));
            let mut pairs = galloping::check(&params, &wires, &budget)?;
            let short = |p: &galloping::GallopingPair| p.distance - p.required.unwrap_or(0.0);
            pairs.sort_by(|a, b| short(a).total_cmp(&short(b)));
            let span = |k: usize| (wires[k].wire.section.clone(), wires[k].wire.span);
            Ok(Report {
                case,
                notes,
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
        matches!(self.job.result, Some(Ok(_)))
            .then(|| self.job.stamp.out_of_date(model, USES))
            .flatten()
    }

    /// The window; returns a span to go to.
    pub fn ui(&mut self, ctx: &egui::Context, model: &Model) -> Option<Selection> {
        self.job.poll();
        let mut go = None;
        let mut again = false;
        let stale = self.out_of_date(model);
        let mut open = self.open;
        let why = (!model.cases.is_empty())
            .then_some(())
            .map_or(Some("the model has no weather case".to_owned()), |_| None)
            .or_else(|| self.scope.why_not(&model.line));
        let scope_words = check_ui::scope_words(&model.line, &self.scope, true);
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
                        check_ui::section(ui, 1, "What it checks", None);
                        help::explain(
                            ui,
                            "Ellipses around the wires of each span, sized from the sag by the \
                             coefficients of the parameter file, and their clearances.",
                            "Each method of the parameter file sizes an ellipse around each wire \
                             from its sag in the weather case chosen; the distance between the \
                             ellipses of two wires is compared with the requirement of the file.",
                        );
                        ui.separator();
                        check_ui::section(ui, 2, "Scope", None);
                        let names: Vec<String> =
                            model.cases.iter().map(|c| c.case.name.clone()).collect();
                        check_ui::ScopePicker::new(WINDOW, &model.line)
                            .wires(&mut self.scope, true)
                            .cases(check_ui::Cases::One(&names, &mut self.case))
                            .ui(ui);
                        ui.separator();
                        check_ui::section(ui, 3, "Rules", None);
                        ui.horizontal(|ui| {
                            let name = self.file.as_ref().map_or("no parameter file".into(), |p| {
                                p.file_name()
                                    .map_or(String::new(), |n| n.to_string_lossy().into_owned())
                            });
                            ui.label(format!("parameters: {name}"));
                            if ui.button("Choose…").clicked()
                                && let Some(p) = rfd::FileDialog::new()
                                    .add_filter(
                                        "Galloping parameters",
                                        &[ohl_clearance::galloping::GALLOPING_FILE_EXTENSION],
                                    )
                                    .pick_file()
                            {
                                self.file = Some(p);
                            }
                        });
                    }),
                    check_ui::Tab::Results => check_ui::results_area(ui, WINDOW, |ui| {
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
                        let n = report.rows.iter().filter(|r| short(r)).count();
                        check_ui::verdict(
                            ui,
                            n > 0,
                            &if n > 0 {
                                format!(
                                    "✖ {n} pair{} × method below the requirement in case \"{}\"",
                                    if n == 1 { "" } else { "s" },
                                    report.case
                                )
                            } else {
                                format!(
                                    "✔ Every pair at its requirement in case \"{}\" ({} pairs × \
                                     methods)",
                                    report.case,
                                    report.rows.len()
                                )
                            },
                            false,
                        );
                        ui.horizontal(|ui| {
                            if ui
                                .button("Export all CSV…")
                                .on_hover_text("Every pair with every value, as a CSV file")
                                .clicked()
                            {
                                save_csv("galloping.csv", &csv(report));
                            }
                        });
                        use check_ui::{Cell, Col};
                        let rows: Vec<check_ui::Row> = report
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
                            .collect();
                        if let Some(k) = check_ui::ResultTable::new(
                            "galloping-rows",
                            vec![
                                Col::text("Method"),
                                Col::text("Span"),
                                Col::text("Other span"),
                                Col::num("Distance [m]"),
                                Col::num("Required [m]"),
                                Col::num("At [%]").tip("Where along the span"),
                                Col::text("Ellipse [m]"),
                                Col::text("Other ellipse [m]"),
                            ],
                        )
                        .rows(rows)
                        .group_by(&[0, 1])
                        .csv("galloping-table.csv")
                        .below(check_ui::FOLDS_BELOW)
                        .ui(ui)
                        {
                            let r = &report.rows[k];
                            go = Some(Selection::Span {
                                section: r.a.0.clone(),
                                span: r.a.1,
                            });
                        }
                        check_ui::notes_fold(ui, WINDOW, &report.notes);
                    }),
                }
                help::footer(ui, WINDOW, |ui| {
                    let overrides = &mut self.overrides;
                    again |= check_ui::job_footer(
                        ui,
                        &mut self.job,
                        why.clone(),
                        "Size the ellipses of every span by the parameter file in the case chosen \
                         and check their clearances",
                        Some(scope_words),
                        |ui| {
                            overrides.ui(ui);
                        },
                    );
                });
            },
        );
        self.open = open;
        if again && why.is_none() {
            self.start(model, ctx);
        }
        go
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
