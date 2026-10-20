// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! File › Export drawings: plan and profile sheets and profile exports of
//! the routes (SVG, DXF, PDF, PNG, CSV, XML) at chosen scales and station
//! range, written in the background (a progress bar; the views stay
//! usable). The same files as `ohl drawing`.
//!
//! The window's settings (formats, scales, sheets, line width, break
//! clearance, options, the sheet preview on or off) are kept in the
//! project, per project line ([`SETTING`]): there when the project is
//! opened again; a project without them opens with the defaults. The
//! **line width** is one setting for the drawings and the profile view:
//! what is shown along the line in both.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use ohl_drawing::export::{Export, Format};
use ohl_line::jumper::JumperShape;
use ohl_line::solve::SectionShape;
use ohl_line::{AlignmentView, Line};
use ohl_terrain::Terrain;
use serde::{Deserialize, Serialize};

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
    /// Line width: half width of the plan, m — what is shown along the
    /// line in the drawings and in the profile view (one setting).
    pub line_width: f64,
    /// Clearance on paper between a sheet break and every other
    /// structure's drawing, mm.
    pub break_clearance: f64,
    /// The structures' elevations on the plan (off by default).
    pub elevations: ohl_drawing::Elevations,
    /// The isoclearance lines of the last Checks › Vegetation run, drawn
    /// on the plan (layer OHL_ISOCLEARANCE): (clearance, points), the top
    /// line.
    pub isoclearance: Option<Arc<IsoLines>>,
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
            line_width: DEFAULT_LINE_WIDTH,
            break_clearance: ohl_drawing::BREAK_CLEARANCE_MM,
            elevations: ohl_drawing::Elevations::default(),
            isoclearance: None,
            out: None,
            error: None,
        }
    }
}

/// Isoclearance lines: (clearance m, points on the ground), and the top
/// line (for the colours).
pub type IsoLines = (Vec<(f64, Vec<[f64; 3]>)>, f64);

/// The line width by default, m (the drawings' plan half width).
pub const DEFAULT_LINE_WIDTH: f64 = 40.0;

/// The elevations' base offsets accepted, mm on paper.
pub const ELEVATION_OFFSETS: std::ops::RangeInclusive<f64> = 0.0..=500.0;

/// The line widths accepted, m.
pub const LINE_WIDTHS: std::ops::RangeInclusive<f64> = 1.0..=2000.0;

/// The project setting keeping the drawing settings: one per project
/// line, `app.drawings.<line>` ([`setting_key`]), a [`DrawingSettings`]
/// as JSON. Like the project's other settings it is not undone and
/// follows everyone on a server (a designer changes it).
pub const SETTING: &str = "app.drawings";

/// The key of project line `line`'s drawing settings.
pub fn setting_key(line: &str) -> String {
    format!("{SETTING}.{line}")
}

/// The drawing settings kept in the project ([`SETTING`]): the export
/// window's, without the route, the output folder (this computer's) and
/// the message. Missing values take the defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DrawingSettings {
    /// Formats ticked (`svg`, `dxf`, `pdf`, `png`, `csv`, `xml`).
    pub formats: Vec<String>,
    /// Sheet format, or "" for one continuous drawing.
    pub page: String,
    /// Horizontal scale denominator.
    pub hscale: f64,
    /// Vertical scale denominator.
    pub vscale: f64,
    /// Only a station range.
    pub range: bool,
    /// Its first station, m.
    pub from: f64,
    /// Its last station, m.
    pub to: f64,
    /// The profile alone.
    pub profile_only: bool,
    /// Required-clearance line above the ground, m.
    pub clearance: Option<f64>,
    /// The other solved weather cases dashed.
    pub all_cases: bool,
    /// PNG resolution, dpi.
    pub dpi: f64,
    /// Line width (half width), m: the drawings' and the profile's.
    pub line_width: f64,
    /// Sheet break clearance on paper, mm.
    pub break_clearance: f64,
    /// The structures' elevations on the plan: `off`, `above` or `below`.
    pub elevations: String,
    /// From the line to the elevations' base, mm on paper.
    pub elevation_offset: f64,
    /// Angle between the line and the elevations' height, degrees.
    pub elevation_rotation: f64,
    /// The sheet preview on.
    pub preview: bool,
}

impl Default for DrawingSettings {
    fn default() -> Self {
        Self::of(&DrawingsWindow::default(), false)
    }
}

impl DrawingSettings {
    /// The settings of window `w`, the preview on or not.
    pub fn of(w: &DrawingsWindow, preview: bool) -> Self {
        Self {
            formats: w.formats.iter().map(|f| f.name().to_owned()).collect(),
            page: w.page.clone(),
            hscale: w.hscale,
            vscale: w.vscale,
            range: w.range,
            from: w.from,
            to: w.to,
            profile_only: w.profile_only,
            clearance: w.clearance,
            all_cases: w.all_cases,
            dpi: w.dpi,
            line_width: w.line_width,
            break_clearance: w.break_clearance,
            elevations: w.elevations.place.key().to_owned(),
            elevation_offset: w.elevations.offset_mm,
            elevation_rotation: w.elevations.rotation_deg,
            preview,
        }
    }

    /// Puts them into window `w` (its route, folder and message kept);
    /// a value out of range (a file edited by hand) takes the default.
    pub fn apply(&self, w: &mut DrawingsWindow) {
        let d = DrawingsWindow::default();
        let ok = |v: f64, range: std::ops::RangeInclusive<f64>, default: f64| {
            if range.contains(&v) { v } else { default }
        };
        w.formats = Format::ALL
            .into_iter()
            .filter(|f| self.formats.iter().any(|n| n == f.name()))
            .collect();
        w.page = if PAGES.contains(&self.page.as_str()) {
            self.page.clone()
        } else {
            d.page.clone()
        };
        w.hscale = ok(self.hscale, 10.0..=100_000.0, d.hscale);
        w.vscale = ok(self.vscale, 10.0..=100_000.0, d.vscale);
        w.range = self.range;
        w.from = if self.from.is_finite() {
            self.from
        } else {
            d.from
        };
        w.to = if self.to.is_finite() { self.to } else { d.to };
        w.profile_only = self.profile_only;
        w.clearance = self.clearance.filter(|c| c.is_finite() && *c >= 0.0);
        w.all_cases = self.all_cases;
        w.dpi = ok(self.dpi, 10.0..=2400.0, d.dpi);
        w.line_width = ok(self.line_width, LINE_WIDTHS, d.line_width);
        w.break_clearance =
            ohl_drawing::check_break_clearance(self.break_clearance).unwrap_or(d.break_clearance);
        w.elevations = ohl_drawing::Elevations {
            place: ohl_drawing::ElevationPlace::parse(&self.elevations)
                .unwrap_or(d.elevations.place),
            offset_mm: ok(
                self.elevation_offset,
                ELEVATION_OFFSETS,
                d.elevations.offset_mm,
            ),
            rotation_deg: ok(
                self.elevation_rotation,
                -180.0..=180.0,
                d.elevations.rotation_deg,
            ),
        };
    }
}

/// The drawings window, the export it started and the sheet preview.
#[derive(Debug, Default)]
pub struct Drawings {
    /// The window, when open.
    pub window: Option<DrawingsWindow>,
    /// The settings of the window when it was last closed (for this
    /// session: the next window starts from them, the preview uses them).
    pub last: DrawingsWindow,
    /// The export running (or finished, until closed).
    pub job: Option<Job>,
    /// Show the export's sheets over the profile and the plan (the sheet
    /// preview; off by default).
    pub preview_on: bool,
    /// The preview made, for what (the settings and the case).
    preview: Option<(PreviewKey, Result<Preview, String>)>,
    /// The settings as last read from or written to the project: the
    /// setting's key and the JSON ([`Drawings::save`] writes them again
    /// when they differ).
    saved: Option<(String, String)>,
}

/// What a preview was made for: the settings that shape the sheets and
/// the weather case shown.
type PreviewKey = (DrawingsWindow, usize);

/// The export's sheets of one alignment, laid out by the export's own
/// code ([`ohl_drawing::sheet_layout`]).
#[derive(Debug, Clone)]
pub struct PreviewRoute {
    /// The alignment (a route of the export).
    pub id: String,
    /// Its centreline (for the plan).
    pub centreline: Vec<[f64; 3]>,
    /// Its sheets: stations, structures at the ends, warnings.
    pub sheets: Vec<ohl_drawing::SheetLayout>,
    /// The structures' elevations the sheets' plan carries (none when
    /// off), in the route's (station, offset).
    pub elevations: Vec<ohl_drawing::ElevationDrawing>,
}

/// The export's sheets, as File › Export › Drawings… would write them now.
#[derive(Debug, Clone)]
pub struct Preview {
    /// The sheet format (none: the export writes one continuous drawing).
    pub page: Option<ohl_drawing::Page>,
    /// The sheets of every alignment.
    pub routes: Vec<PreviewRoute>,
    /// The sheets have a plan above the profile.
    pub plan: bool,
    /// The line width (half width of the plan), m.
    pub line_width: f64,
}

impl Preview {
    /// The export's warnings, as the export writes them.
    pub fn warnings(&self) -> Vec<String> {
        self.routes
            .iter()
            .flat_map(|r| {
                r.sheets.iter().flat_map(move |s| {
                    s.warnings
                        .iter()
                        .map(move |w| format!("route {}: sheet {}: {w}", r.id, s.number))
                })
            })
            .collect()
    }

    /// The sheets of alignment `id`.
    pub fn route(&self, id: &str) -> Option<&PreviewRoute> {
        self.routes.iter().find(|r| r.id == id)
    }
}

/// The sheets the export would write with settings `w` for `model`
/// showing case `case`, laid out by the export's own code.
///
/// Expected cost: what the export takes to collect the drawing data
/// (under a second per route), without writing anything.
pub fn preview(model: &Model, case: usize, w: &DrawingsWindow) -> Result<Preview, String> {
    let every = DrawingsWindow {
        route: None,
        ..w.clone()
    };
    let input = Input::prepare(model, case, &every, PathBuf::new())?;
    let options = input.route_options();
    let mut routes = Vec::new();
    if let Some(page) = &input.export.page {
        for route in &input.routes {
            let d = input.drawing(route)?;
            routes.push(PreviewRoute {
                id: route.id.clone(),
                sheets: ohl_drawing::sheet_layout(&d, &options, page),
                elevations: ohl_drawing::elevations(&d, &options),
                centreline: d.centreline,
            });
        }
    }
    Ok(Preview {
        page: input.export.page.clone(),
        routes,
        plan: options.plan,
        line_width: options.plan_half_width,
    })
}

/// What an export needs, taken from the model (sent to the background).
pub struct Input {
    /// The line.
    pub line: Line,
    /// Its structure types' bodies (levels 2 and 3: members drawn on their
    /// own layer).
    pub bodies: Arc<ohl_structures_fe::drawn::Bodies>,
    /// Routes to draw.
    pub routes: Vec<AlignmentView>,
    /// Solved cases: name, sections, jumpers; the first drawn solid.
    pub cases: Vec<(String, Vec<SectionShape>, Vec<JumperShape>)>,
    /// Ground surface.
    pub ground: Option<Arc<Terrain>>,
    /// Drawing options (without the station range: set per route).
    pub options: ohl_drawing::Options,
    /// Station range.
    pub range: Option<[f64; 2]>,
    /// Formats and sheets.
    pub export: Export,
    /// Isoclearance lines on the plan.
    pub isoclearance: Option<Arc<IsoLines>>,
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
        Self::prepare(model, case, w, out)
    }

    /// The export of `w` into `out`, the folder and formats not checked.
    fn prepare(
        model: &Model,
        case: usize,
        w: &DrawingsWindow,
        out: PathBuf,
    ) -> Result<Self, String> {
        let break_clearance = ohl_drawing::check_break_clearance(w.break_clearance)?;
        if !(w.line_width.is_finite() && w.line_width > 0.0) {
            return Err(format!(
                "line width ±{} m: give a positive width",
                w.line_width
            ));
        }
        let all = model.line.alignment_views();
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
            Some(ohl_drawing::page_format(&w.page).ok_or("unknown sheet format")?)
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
                _ => format!("{} ({})", c.0, ohl_drawing::case_dash_name(k)),
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
            bodies: model.bodies.clone(),
            routes,
            ground: model.ground.clone(),
            options: ohl_drawing::Options {
                horizontal_scale: w.hscale,
                vertical_scale: w.vscale,
                clearance: w.clearance,
                title,
                plan: !w.profile_only,
                plan_half_width: w.line_width,
                break_clearance,
                elevations: w.elevations,
                ..Default::default()
            },
            range: w.range.then_some([w.from.min(w.to), w.from.max(w.to)]),
            export: Export {
                formats: w.formats.clone(),
                page,
                dpi: w.dpi,
            },
            cases,
            isoclearance: w.isoclearance.clone(),
            out,
        })
    }

    /// The drawing data of `route`: the first case's wires solid, the
    /// others' dashed.
    pub fn drawing(&self, route: &AlignmentView) -> Result<ohl_drawing::RouteDrawing, String> {
        let (_, shapes, jumpers) = &self.cases[0];
        let mut d = ohl_drawing::build(
            &self.line,
            route,
            shapes,
            jumpers,
            self.ground.as_deref(),
            &[],
            &self.options,
        )?;
        crate::scene::set_drawn_members(&mut d, &self.line, &self.bodies);
        if let Some(lines) = &self.isoclearance {
            d.set_isoclearance(&lines.0, lines.1);
        }
        for (k, (name, shapes, jumpers)) in self.cases.iter().enumerate().skip(1) {
            let other =
                ohl_drawing::build(&self.line, route, shapes, jumpers, None, &[], &self.options)?;
            for mut w in other.wires {
                w.label = format!("{} — {name}", w.label);
                w.dash = Some(ohl_drawing::case_dash(k));
                d.wires.push(w);
            }
        }
        Ok(d)
    }

    /// The options of each route's drawings (with the station range).
    pub fn route_options(&self) -> ohl_drawing::Options {
        ohl_drawing::Options {
            stations: self.range,
            ..self.options.clone()
        }
    }
}

/// Writes the drawings of `input`, reporting progress per route.
///
/// Expected cost: under a second per route for SVG, DXF, PDF, CSV and
/// XML; ≈ 0.5 s per sheet for PNG. Exit clause: 30 minutes per route.
pub fn run(input: &Input, report: &dyn Fn(String, f32)) -> Result<String, String> {
    let started = std::time::Instant::now();
    let budget =
        ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(30 * 60));
    let (mut files, mut warnings) = (0, Vec::new());
    let n = input.routes.len();
    for (r, route) in input.routes.iter().enumerate() {
        report(
            format!("route {} ({} of {n})", route.id, r + 1),
            r as f32 / n as f32,
        );
        let d = input.drawing(route)?;
        let options = input.route_options();
        let written =
            ohl_drawing::export::write_route(&d, &options, &input.export, &input.out, &budget)?;
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
        preview_on: &mut bool,
    ) -> Option<Job> {
        let mut job = None;
        let routes = model.line.alignment_views();
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

                        ui.label("Line width");
                        ui.horizontal(|ui| {
                            ui.label("±");
                            ui.add(
                                egui::DragValue::new(&mut self.line_width)
                                    .range(LINE_WIDTHS)
                                    .suffix(" m")
                                    .speed(1.0),
                            )
                            .on_hover_text(LINE_WIDTH_HELP);
                        });
                        ui.end_row();

                        ui.label("Sheet breaks");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.break_clearance)
                                    .range(0.0..=ohl_drawing::MAX_BREAK_CLEARANCE_MM)
                                    .suffix(" mm")
                                    .speed(0.1)
                                    .max_decimals(1),
                            )
                            .on_hover_text(
                                "Clearance on paper between a sheet break and what is drawn \
                                 of every other structure (0 to 20 mm)",
                            );
                            ui.label("on paper clear of other structures");
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

                        ui.label("Elevations");
                        ui.horizontal(|ui| {
                            let e = &mut self.elevations;
                            egui::ComboBox::from_id_salt("drawings-elevations")
                                .selected_text(e.place.key())
                                .show_ui(ui, |ui| {
                                    for p in ohl_drawing::ElevationPlace::ALL {
                                        ui.selectable_value(&mut e.place, p, p.key());
                                    }
                                })
                                .response
                                .on_hover_text(
                                    "Each structure seen along the line on the plan, at the \
                                     horizontal scale, a leader from where it stands \
                                     (layer OHL_ELEVATIONS)",
                                );
                            let on = e.place != ohl_drawing::ElevationPlace::Off;
                            ui.label("base");
                            ui.add_enabled(
                                on,
                                egui::DragValue::new(&mut e.offset_mm)
                                    .range(ELEVATION_OFFSETS)
                                    .suffix(" mm")
                                    .speed(0.5),
                            )
                            .on_hover_text("From the line to the elevation's base, on paper");
                            ui.label("at");
                            ui.add_enabled(
                                on,
                                egui::DragValue::new(&mut e.rotation_deg)
                                    .range(-180.0..=180.0)
                                    .suffix("°")
                                    .speed(1.0),
                            )
                            .on_hover_text(
                                "Angle between the line and the elevation's height \
                                 (90°: standing perpendicular to the line)",
                            );
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
                ui.checkbox(preview_on, PREVIEW_TICK)
                    .on_hover_text(PREVIEW_HELP);
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
                if let Some(lines) = &self.isoclearance {
                    let mut keep = true;
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "Isoclearance lines of Checks › Vegetation: {} on the plan (OHL_ISOCLEARANCE)",
                            lines.0.len()
                        ));
                        if ui.button("Leave them out").clicked() {
                            keep = false;
                        }
                    });
                    if !keep {
                        self.isoclearance = None;
                    }
                }
                if let Some(e) = &self.error {
                    crate::menu::error_label(ui, e);
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
    /// The drawing settings kept in `model`'s project for the project
    /// line worked on (the defaults when there are none).
    pub fn loaded(model: &Model) -> Self {
        let mut d = Self::default();
        d.load(model);
        d
    }

    /// Reads the drawing settings of the project line worked on from the
    /// project (after opening it, switching lines, or another user
    /// changing them); a project without them gets the defaults.
    pub fn load(&mut self, model: &Model) {
        let Some(session) = model.session() else {
            return;
        };
        let key = setting_key(session.current_line());
        let stored: DrawingSettings = session
            .setting(&key)
            .ok()
            .flatten()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        stored.apply(&mut self.last);
        if let Some(w) = &mut self.window {
            stored.apply(w);
        }
        self.preview_on = stored.preview;
        self.preview = None;
        self.saved = serde_json::to_string(&self.stored())
            .ok()
            .map(|json| (key, json));
    }

    /// The settings as they are kept in the project.
    pub fn stored(&self) -> DrawingSettings {
        DrawingSettings::of(self.settings(), self.preview_on)
    }

    /// Writes the drawing settings into the project when they changed
    /// (each frame; not while a value is being dragged or typed). A
    /// project this user cannot change keeps them for the session.
    pub fn save(&mut self, model: &mut Model, ctx: &egui::Context) {
        let Some(line) = model.session().map(|s| s.current_line().to_owned()) else {
            return;
        };
        let key = setting_key(&line);
        if self.saved.as_ref().is_some_and(|(k, _)| *k != key) {
            // Another project line worked on: its own settings.
            self.load(model);
            return;
        }
        let Ok(json) = serde_json::to_string(&self.stored()) else {
            return;
        };
        if self.saved.as_ref().is_some_and(|(_, j)| *j == json)
            || ctx.input(|i| i.pointer.any_down())
            || ctx.memory(|m| m.focused().is_some())
        {
            return;
        }
        let op = ohl_server::Op::SetSetting {
            key: key.clone(),
            value: Some(json.clone()),
        };
        // (Read-only for this user: kept for the session.)
        let _ = model.project_op("drawing settings", op);
        self.saved = Some((key, json));
    }

    /// The line width, m: the drawings' and the profile view's.
    pub fn line_width(&self) -> f64 {
        self.settings().line_width
    }

    /// Sets the line width (from the profile view too).
    pub fn set_line_width(&mut self, width: f64) {
        let w = width.clamp(*LINE_WIDTHS.start(), *LINE_WIDTHS.end());
        if let Some(win) = &mut self.window {
            win.line_width = w;
        }
        self.last.line_width = w;
    }

    /// The window (if open) and the job's window, each frame.
    pub fn show(&mut self, ctx: &egui::Context, model: &Model, case: usize) {
        if let Some(w) = &mut self.window {
            let mut open = true;
            if let Some(job) = w.ui(ctx, model, case, &mut open, &mut self.preview_on) {
                self.job = Some(job);
            }
            if !open {
                self.last = self.window.take().unwrap_or_default();
                self.last.error = None;
            }
        }
        if let Some(job) = &mut self.job
            && crate::survey_io::job_window(ctx, job)
        {
            self.job = None;
        }
    }

    /// Opens the window (with the settings it had when last closed).
    pub fn open(&mut self) {
        if self.window.is_none() {
            self.window = Some(self.last.clone());
        }
    }

    /// Gives the drawings the isoclearance lines of a vegetation run (none:
    /// without them).
    pub fn set_isoclearance(&mut self, lines: Option<Arc<IsoLines>>) {
        self.last.isoclearance = lines.clone();
        if let Some(w) = &mut self.window {
            w.isoclearance = lines;
        }
    }

    /// The export settings: the window's, or those it had when closed.
    pub fn settings(&self) -> &DrawingsWindow {
        self.window.as_ref().unwrap_or(&self.last)
    }

    /// Forgets the preview (after the line or the solved cases changed).
    pub fn invalidate_preview(&mut self) {
        self.preview = None;
    }

    /// The sheet preview when it is on: made again when the settings or
    /// the case change (and after [`Self::invalidate_preview`]).
    pub fn preview(&mut self, model: &Model, case: usize) -> Option<&Result<Preview, String>> {
        if !self.preview_on {
            return None;
        }
        // Only what shapes the sheets (not the folder, formats, message).
        let key: PreviewKey = (
            DrawingsWindow {
                route: None,
                formats: Vec::new(),
                dpi: 0.0,
                out: None,
                error: None,
                ..self.settings().clone()
            },
            case,
        );
        if self.preview.as_ref().is_none_or(|(k, _)| *k != key) {
            let made = preview(model, case, &key.0);
            self.preview = Some((key, made));
        }
        self.preview.as_ref().map(|(_, p)| p)
    }

    /// The sheet preview's options, in a menu of ticks (it stays open on
    /// a tick); its button opens the drawing settings.
    pub fn preview_menu(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.preview_on, PREVIEW_TICK)
            .on_hover_text(PREVIEW_HELP);
        let s = self.settings();
        let text = if s.page.is_empty() {
            "Export settings: one continuous drawing (no sheets). Choose a sheet format in \
             the drawing settings."
                .to_owned()
        } else {
            format!(
                "Export settings: {} at 1:{} / 1:{}{}, line width ±{} m, breaks {} mm clear \
                 of other structures{}",
                s.page,
                s.hscale,
                s.vscale,
                if s.profile_only {
                    ", the profile alone"
                } else {
                    ", plan and profile"
                },
                s.line_width,
                s.break_clearance,
                if s.range {
                    format!(", stations {} to {}", s.from, s.to)
                } else {
                    String::new()
                }
            )
        };
        ui.label(egui::RichText::new(text).small().weak());
        if ui.button("Drawing settings…").clicked() {
            self.open();
            ui.close();
        }
    }
}

/// What the line width is (the export window and the profile view).
pub const LINE_WIDTH_HELP: &str = "How far left and right of the alignment the line is shown, \
     in the drawings (the plan's half width) and in the profile view (one setting, kept with the \
     project line): the ground at both edges, the survey and LiDAR points and the 3D drawings \
     within it projected on the profile; structures beside the line (connections) only within \
     it, wires cut at it. Where sheets end does not depend on it";

/// The sheet preview's tick.
pub const PREVIEW_TICK: &str = "Show the export's sheets over the profile and the plan";

/// What the sheet preview shows.
const PREVIEW_HELP: &str = "Sheet edges (match lines), sheet numbers and the export's warnings \
     over the continuous profile and the plan, laid out by the export itself with the settings \
     of File › Export › Drawings…; nothing is computed differently";
