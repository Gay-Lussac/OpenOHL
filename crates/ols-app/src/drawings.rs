// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! File › Export drawings: plan and profile sheets and profile exports of
//! the routes (SVG, DXF, PDF, PNG, CSV, XML) at chosen scales and station
//! range, written in the background (a progress bar; the views stay
//! usable). The same files as `ols drawing`.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use ols_drawing::export::{Export, Format};
use ols_line::jumper::JumperShape;
use ols_line::solve::SectionShape;
use ols_line::{Line, Route};
use ols_terrain::Terrain;

use crate::model::Model;
use crate::survey_io::Job;

/// Sheet formats offered ("" = one continuous drawing).
const PAGES: [&str; 6] = ["", "A0", "A1", "A2", "A3", "A4"];

/// The Export drawings window.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawingsWindow {
    /// Route by index; `None`: every route.
    pub route: Option<usize>,
    /// Formats ticked.
    pub formats: Vec<Format>,
    /// Sheet format, or "" for one continuous drawing.
    pub page: String,
    /// Horizontal scale denominator.
    pub hscale: f64,
    /// Vertical scale denominator.
    pub vscale: f64,
    /// Only a station range.
    pub range: bool,
    /// Its first and last station, m.
    pub from: f64,
    /// Last station, m.
    pub to: f64,
    /// The profile alone.
    pub profile_only: bool,
    /// Required-clearance line above the ground, m.
    pub clearance: Option<f64>,
    /// The other solved weather cases dashed (else the case shown only).
    pub all_cases: bool,
    /// PNG resolution, dpi.
    pub dpi: f64,
    /// Output folder.
    pub out: Option<PathBuf>,
    /// Why it cannot start.
    pub error: Option<String>,
}

impl Default for DrawingsWindow {
    fn default() -> Self {
        Self {
            route: None,
            formats: vec![Format::Pdf],
            page: "A3".into(),
            hscale: 2000.0,
            vscale: 200.0,
            range: false,
            from: 0.0,
            to: 1000.0,
            profile_only: false,
            clearance: None,
            all_cases: false,
            dpi: 150.0,
            out: None,
            error: None,
        }
    }
}

/// The drawings window and the export it started.
#[derive(Debug, Default)]
pub struct Drawings {
    /// The window, when open.
    pub window: Option<DrawingsWindow>,
    /// The export running (or finished, until closed).
    pub job: Option<Job>,
}

/// What an export needs, taken from the model (sent to the background).
pub struct Input {
    /// The line.
    pub line: Line,
    /// Routes to draw.
    pub routes: Vec<Route>,
    /// Solved cases: name, sections, jumpers; the first drawn solid.
    pub cases: Vec<(String, Vec<SectionShape>, Vec<JumperShape>)>,
    /// Ground surface.
    pub ground: Option<Arc<Terrain>>,
    /// Drawing options (without the station range: set per route).
    pub options: ols_drawing::Options,
    /// Station range.
    pub range: Option<[f64; 2]>,
    /// Formats and sheets.
    pub export: Export,
    /// Output folder.
    pub out: PathBuf,
}

impl Input {
    /// The export the window asks for, from `model` showing case `case`.
    pub fn new(model: &Model, case: usize, w: &DrawingsWindow) -> Result<Self, String> {
        let out = w.out.clone().ok_or("choose an output folder")?;
        if w.formats.is_empty() {
            return Err("tick a format".into());
        }
        let all = model.line.effective_routes();
        let routes = match w.route {
            Some(k) => vec![all.get(k).ok_or("no such route")?.clone()],
            None => all,
        };
        if routes.is_empty() {
            return Err("the line has no route".into());
        }
        let page = if w.page.is_empty() {
            None
        } else {
            Some(ols_drawing::page_format(&w.page).ok_or("unknown sheet format")?)
        };
        // The case shown first (solid), then the other solved ones.
        let mut order = vec![case];
        if w.all_cases {
            order.extend((0..model.cases.len()).filter(|&k| k != case && model.cases[k].ready));
        }
        let cases: Vec<_> = order
            .into_iter()
            .filter_map(|k| {
                let solved = model.cases.get(k)?;
                Some((
                    solved.case.name.clone(),
                    model.shapes_shown(k),
                    solved.jumpers.clone(),
                ))
            })
            .collect();
        let first = cases.first().ok_or("no solved weather case")?;
        let names: Vec<String> = cases
            .iter()
            .enumerate()
            .map(|(k, c)| match k {
                0 => format!("{} (solid)", c.0),
                _ => format!("{} ({})", c.0, ols_drawing::case_dash_name(k)),
            })
            .collect();
        let title = vec![
            model.line.name.clone(),
            if names.len() > 1 {
                format!("Weather cases: {}", names.join(", "))
            } else {
                format!("Weather case: {}", first.0)
            },
        ];
        Ok(Self {
            line: model.line.clone(),
            routes,
            ground: model.ground.clone(),
            options: ols_drawing::Options {
                horizontal_scale: w.hscale,
                vertical_scale: w.vscale,
                clearance: w.clearance,
                title,
                plan: !w.profile_only,
                ..Default::default()
            },
            range: w.range.then_some([w.from.min(w.to), w.from.max(w.to)]),
            export: Export {
                formats: w.formats.clone(),
                page,
                dpi: w.dpi,
            },
            cases,
            out,
        })
    }
}

/// Writes the drawings of `input`, reporting progress per route.
///
/// Expected cost: under a second per route for SVG, DXF, PDF, CSV and
/// XML; ≈ 0.5 s per sheet for PNG. Exit clause: 30 minutes per route.
pub fn run(input: &Input, report: &dyn Fn(String, f32)) -> Result<String, String> {
    let started = std::time::Instant::now();
    let budget =
        ols_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(30 * 60));
    let (mut files, mut warnings) = (0, Vec::new());
    let n = input.routes.len();
    for (r, route) in input.routes.iter().enumerate() {
        report(
            format!("route {} ({} of {n})", route.id, r + 1),
            r as f32 / n as f32,
        );
        let (_, shapes, jumpers) = &input.cases[0];
        let mut d = ols_drawing::build(
            &input.line,
            route,
            shapes,
            jumpers,
            input.ground.as_deref(),
            &[],
            &input.options,
        )?;
        for (k, (name, shapes, jumpers)) in input.cases.iter().enumerate().skip(1) {
            let other = ols_drawing::build(
                &input.line,
                route,
                shapes,
                jumpers,
                None,
                &[],
                &input.options,
            )?;
            for mut w in other.wires {
                w.label = format!("{} — {name}", w.label);
                w.dash = Some(ols_drawing::case_dash(k));
                d.wires.push(w);
            }
        }
        let options = ols_drawing::Options {
            stations: input.range,
            ..input.options.clone()
        };
        let written =
            ols_drawing::export::write_route(&d, &options, &input.export, &input.out, &budget)?;
        files += written.files.len();
        warnings.extend(
            written
                .warnings
                .into_iter()
                .map(|w| format!("route {}: {w}", route.id)),
        );
    }
    report("done".into(), 1.0);
    let mut text = format!(
        "{files} files written to {} in {:.1?}",
        input.out.display(),
        started.elapsed()
    );
    for w in warnings {
        text.push_str("\n⚠ ");
        text.push_str(&w);
    }
    Ok(text)
}

impl DrawingsWindow {
    /// The window; `Some(job)` once started.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        model: &Model,
        case: usize,
        open: &mut bool,
    ) -> Option<Job> {
        let mut job = None;
        let routes = model.line.effective_routes();
        egui::Window::new("Export drawings")
            .open(open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("drawings-grid")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Route");
                        let name = |k: Option<usize>| match k.and_then(|k| routes.get(k)) {
                            Some(r) => format!("{} {}", r.id, r.name),
                            None => "every route".into(),
                        };
                        egui::ComboBox::from_id_salt("drawings-route")
                            .selected_text(name(self.route))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.route, None, name(None));
                                for k in 0..routes.len() {
                                    ui.selectable_value(&mut self.route, Some(k), name(Some(k)));
                                }
                            });
                        ui.end_row();

                        ui.label("Formats");
                        ui.vertical(|ui| {
                            for f in Format::ALL {
                                let mut on = self.formats.contains(&f);
                                if ui
                                    .checkbox(&mut on, f.name().to_uppercase())
                                    .on_hover_text(f.description())
                                    .changed()
                                {
                                    self.formats.retain(|x| *x != f);
                                    if on {
                                        self.formats.push(f);
                                    }
                                }
                            }
                        });
                        ui.end_row();

                        ui.label("Sheets");
                        egui::ComboBox::from_id_salt("drawings-page")
                            .selected_text(if self.page.is_empty() {
                                "one continuous drawing"
                            } else {
                                &self.page
                            })
                            .show_ui(ui, |ui| {
                                for p in PAGES {
                                    let text = if p.is_empty() {
                                        "one continuous drawing".to_owned()
                                    } else {
                                        format!("{p} landscape, split at structures")
                                    };
                                    ui.selectable_value(&mut self.page, p.to_owned(), text);
                                }
                            });
                        ui.end_row();

                        ui.label("Scales");
                        ui.horizontal(|ui| {
                            ui.label("horizontal 1:");
                            ui.add(
                                egui::DragValue::new(&mut self.hscale)
                                    .range(10.0..=100_000.0)
                                    .speed(10.0),
                            );
                            ui.label("vertical 1:");
                            ui.add(
                                egui::DragValue::new(&mut self.vscale)
                                    .range(10.0..=100_000.0)
                                    .speed(10.0),
                            );
                        });
                        ui.end_row();

                        ui.label("Stations");
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut self.range, "from");
                            ui.add_enabled(
                                self.range,
                                egui::DragValue::new(&mut self.from).suffix(" m").speed(5.0),
                            );
                            ui.label("to");
                            ui.add_enabled(
                                self.range,
                                egui::DragValue::new(&mut self.to).suffix(" m").speed(5.0),
                            );
                        });
                        ui.end_row();

                        ui.label("Clearance line");
                        ui.horizontal(|ui| {
                            let mut on = self.clearance.is_some();
                            ui.checkbox(&mut on, "ground +");
                            let mut c = self.clearance.unwrap_or(7.0);
                            ui.add_enabled(
                                on,
                                egui::DragValue::new(&mut c)
                                    .range(0.0..=100.0)
                                    .suffix(" m")
                                    .speed(0.1),
                            );
                            self.clearance = on.then_some(c);
                        });
                        ui.end_row();

                        ui.label("PNG");
                        ui.add(
                            egui::DragValue::new(&mut self.dpi)
                                .range(30.0..=600.0)
                                .suffix(" dpi"),
                        );
                        ui.end_row();
                    });
                ui.checkbox(&mut self.profile_only, "the profile alone (no plan)");
                ui.checkbox(
                    &mut self.all_cases,
                    "the other solved weather cases too (dashed)",
                );
                ui.horizontal(|ui| {
                    if ui.button("Folder…").clicked()
                        && let Some(dir) = rfd::FileDialog::new().pick_folder()
                    {
                        self.out = Some(dir);
                    }
                    ui.label(
                        self.out
                            .as_ref()
                            .map_or("no folder chosen".into(), |p| p.display().to_string()),
                    );
                });
                ui.label(
                    egui::RichText::new(
                        "DXF: R12 sheets in mm, plus the plan in project coordinates and the \
                         profile (station × exaggerated height) in metres. PDF: a page per \
                         sheet. CSV/XML: the profile's numbers.",
                    )
                    .small()
                    .weak(),
                );
                if let Some(e) = &self.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, e);
                }
                if ui.button("Export").clicked() {
                    match Input::new(model, case, self) {
                        Ok(input) => {
                            self.error = None;
                            job = Some(Job::start(
                                "Exporting drawings".into(),
                                false,
                                ctx,
                                move |report| run(&input, report),
                            ));
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
            });
        job
    }
}

impl Drawings {
    /// The window (if open) and the job's window, each frame.
    pub fn show(&mut self, ctx: &egui::Context, model: &Model, case: usize) {
        if let Some(w) = &mut self.window {
            let mut open = true;
            if let Some(job) = w.ui(ctx, model, case, &mut open) {
                self.job = Some(job);
            }
            if !open {
                self.window = None;
            }
        }
        if let Some(job) = &mut self.job
            && crate::survey_io::job_window(ctx, job)
        {
            self.job = None;
        }
    }
}
