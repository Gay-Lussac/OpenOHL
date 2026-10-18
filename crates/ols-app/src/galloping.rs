// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Checks › Galloping ellipses: the ellipses of the wires of each span by
//! the methods of a galloping parameter file (`.olsg`), in one weather
//! case, and the clearance between them ([`ols_clearance::galloping`]).
//! Computed on demand in the background; worst pairs first; a click goes
//! to the span.

use std::path::PathBuf;

use eframe::egui;
use ols_clearance::galloping::{self, GallopingParams};

use crate::job::{Job, case_combo, or_dash, save_csv};
use crate::model::Model;
use crate::selection::Selection;

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
    /// The parameter file.
    pub file: Option<PathBuf>,
    /// Weather case (index in the model's cases).
    pub case: usize,
    /// Only pairs below the requirement.
    pub only_short: bool,
    job: Job<Report>,
}

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

    fn start(&mut self, model: &Model, ctx: &egui::Context) {
        let Some(path) = self.file.clone() else {
            self.job.result = Some(Err("choose a galloping parameter file (.olsg)".into()));
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
        self.job.start(ctx, move || {
            let params = GallopingParams::load(&path)?;
            let mut notes: Vec<String> =
                failed.iter().map(|f| format!("not solved: {f}")).collect();
            if !params.applies(&case) {
                notes.push(format!(
                    "the parameters name the cases {:?}; checked in \"{case}\"",
                    params.cases
                ));
            }
            let wires = galloping::galloping_wires(&line, &shapes, &library);
            // Exit clause: well under a second for any line.
            let budget =
                ols_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(300));
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

    /// The window; returns a span to go to.
    pub fn ui(&mut self, ctx: &egui::Context, model: &Model) -> Option<Selection> {
        self.job.poll();
        let mut go = None;
        let mut open = self.open;
        egui::Window::new("Galloping ellipses")
            .open(&mut open)
            .default_width(760.0)
            .show(ctx, |ui| {
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
                                &[ols_clearance::galloping::GALLOPING_FILE_EXTENSION],
                            )
                            .pick_file()
                    {
                        self.file = Some(p);
                    }
                    let names: Vec<String> =
                        model.cases.iter().map(|c| c.case.name.clone()).collect();
                    case_combo(ui, "galloping-case", &names, &mut self.case);
                    if ui
                        .add_enabled(!self.job.running(), egui::Button::new("Check"))
                        .clicked()
                    {
                        self.start(model, ui.ctx());
                    }
                    self.job.status_ui(ui);
                });
                let report = match &self.job.result {
                    None => {
                        ui.label(
                            "Ellipses around the wires of each span, sized from the sag by the \
                             coefficients of the parameter file, and their clearances.",
                        );
                        return;
                    }
                    Some(Err(e)) => {
                        crate::menu::error_label(ui, e);
                        return;
                    }
                    Some(Ok(r)) => r,
                };
                for n in &report.notes {
                    ui.label(egui::RichText::new(n).small().weak());
                }
                let short = report
                    .rows
                    .iter()
                    .filter(|r| r.required.is_some_and(|q| r.distance < q))
                    .count();
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "\"{}\": {} pairs × methods, {short} below the requirement",
                        report.case,
                        report.rows.len()
                    ));
                    ui.checkbox(&mut self.only_short, "only those");
                    if ui.button("Export CSV…").clicked() {
                        save_csv("galloping.csv", &csv(report));
                    }
                });
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Grid::new("galloping-rows")
                            .striped(true)
                            .spacing([12.0, 3.0])
                            .show(ui, |ui| {
                                for h in [
                                    "method",
                                    "span",
                                    "other",
                                    "distance m",
                                    "required m",
                                    "at %",
                                    "ellipse m",
                                    "other m",
                                ] {
                                    ui.strong(h);
                                }
                                ui.end_row();
                                for r in report
                                    .rows
                                    .iter()
                                    .filter(|r| {
                                        !self.only_short
                                            || r.required.is_some_and(|q| r.distance < q)
                                    })
                                    .take(5000)
                                {
                                    ui.label(&r.method);
                                    for (s, k) in [&r.a, &r.b] {
                                        if ui
                                            .link(format!("{s} span {k}"))
                                            .on_hover_text("Go to this span")
                                            .clicked()
                                        {
                                            go = Some(Selection::Span {
                                                section: s.clone(),
                                                span: *k,
                                            });
                                        }
                                    }
                                    let bad = r.required.is_some_and(|q| r.distance < q);
                                    let d = egui::RichText::new(format!("{:.2}", r.distance))
                                        .monospace();
                                    ui.label(if bad {
                                        d.color(ui.visuals().error_fg_color)
                                    } else {
                                        d
                                    });
                                    ui.label(or_dash(r.required, 2));
                                    ui.label(format!("{:.0}", r.at * 100.0));
                                    for a in r.axes {
                                        ui.label(format!("{:.2} × {:.2}", a[0], a[1]));
                                    }
                                    ui.end_row();
                                }
                            });
                    });
            });
        self.open = open;
        go
    }
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
            ols_core::text::csv_field(&r.case),
            ols_core::text::csv_field(&row.method),
            ols_core::text::csv_field(&row.a.0),
            row.a.1,
            ols_core::text::csv_field(&row.b.0),
            row.b.1,
            row.distance,
            row.required.map_or(String::new(), |v| format!("{v:.3}")),
            row.at
        );
    }
    out
}
