// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Vegetation: danger trees — points of the vegetation classes
//! (by role) near the wires of the weather cases ticked (each windy case
//! blown left and right, optionally along its swing arc, with unbalanced
//! ice on the spans near vegetation): grow-in now or after the growth
//! until the next cut, and trees that could fall onto a wire turning about
//! their root ball (heights above the surface of the checks) — per span
//! with the case and position of the worst ([`ohl_clearance::vegetation`]);
//! optionally the isoclearance lines of the ground
//! ([`ohl_clearance::isoclearance`]). The points are read again from the
//! point cloud in a corridor around the spans, on a background thread.
//!
//! What it found is drawn on three layers ([`Overlay`]): grow-in points
//! (a cyan dot joined to the wire), danger trees (an orange trunk from
//! its base to its top, a dashed line to the wire point it falls to), and
//! the isoclearance lines in the colours of their clearance.

use std::sync::Arc;

use eframe::egui;
use ohl_clearance::isoclearance::{Contour, IsoParams};
use ohl_clearance::positions::{PositionOptions, Positions, UnbalancedIce};
use ohl_clearance::vegetation::{
    DEFAULT_MAX_BASE_GROUND_DISTANCE, DEFAULT_SEARCH_DISTANCE, VegetationCheck, VegetationParams,
    WireVegetation,
};

use crate::job::{Job, or_dash, save_csv};
use crate::model::Model;
use crate::selection::Selection;

/// What a check found.
#[derive(Debug, Clone)]
pub struct Report {
    /// The spans and positions checked.
    pub positions: Arc<Positions>,
    /// What was found near each span.
    pub results: Vec<WireVegetation>,
    /// The parameters used.
    pub params: VegetationParams,
    /// Vegetation points read.
    pub points: u64,
    /// The isoclearance lines, when asked for (and their top line).
    pub lines: Option<(Vec<Contour>, f64)>,
    /// Notes.
    pub notes: Vec<String>,
}

/// What the views draw of a report.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overlay {
    /// Grow-in points: the point and the wire point nearest to it.
    pub grow: Vec<([f64; 3], [f64; 3])>,
    /// Danger trees: the top, the base and the wire point it falls to.
    pub fall: Vec<([f64; 3], [f64; 3], [f64; 3])>,
    /// Isoclearance lines: clearance, colour, points on the ground.
    pub lines: Vec<(f64, [u8; 3], Vec<[f64; 3]>)>,
    /// The report is out of date: drawn dimmed ([`crate::revision`]).
    pub stale: bool,
}

impl Overlay {
    /// The points kept of `report` (each span's nearest, worst falling
    /// tree and worst points), and its lines.
    pub fn of(report: &Report) -> Self {
        let mut out = Self::default();
        let p = &report.params;
        for r in &report.results {
            let kept = r.nearest.iter().chain(&r.worst_fall).chain(&r.worst);
            for v in kept {
                let grows =
                    v.distance.is_finite() && v.distance - p.growth() - p.buffer < p.clearance;
                if grows && !out.grow.iter().any(|g| g.0 == v.point) {
                    out.grow.push((v.point, v.wire_point));
                }
                if let (Some(m), Some(h), Some(q)) = (v.fall_margin, v.height, v.fall_point)
                    && m < 0.0
                    && !out.fall.iter().any(|f| f.0 == v.point)
                {
                    let base = [v.point[0], v.point[1], v.point[2] - h];
                    out.fall.push((v.point, base, q));
                }
            }
        }
        if let Some((lines, top)) = &report.lines {
            for c in lines {
                let hex = ohl_drawing::isoclearance_color(c.level, *top);
                let rgb = crate::scene::rgb(&hex);
                out.lines.push((c.level, rgb, c.points.clone()));
            }
        }
        out
    }
}

/// How unbalanced ice is checked (none: not).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Ice {
    Off,
    OfCase,
    /// More radial ice, mm.
    Thickness,
    /// More ice load per wire, N/m.
    Extra,
    Concentrated,
}

/// What a vegetation report is computed from.
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::ALL;

/// The Vegetation window.
pub struct VegetationWindow {
    /// Shown.
    pub open: bool,
    /// Weather cases ticked (indices in the model's cases).
    pub cases: Vec<usize>,
    /// What to look for.
    pub params: VegetationParams,
    /// Each span along its swing arc too.
    swing_arc: bool,
    /// Unbalanced ice and its value (mm, N/m or N).
    ice: (Ice, f64),
    /// A limit to the distance of a tree base's ground point (and its
    /// value, m).
    base_limit: (bool, f64),
    /// Half width of the corridor read and drawn, m.
    corridor: f64,
    /// Isoclearance lines asked for, and how.
    pub(crate) iso: (bool, IsoParams),
    job: Job<Report>,
    /// A new report for the views to draw.
    fresh: bool,
}

impl Default for VegetationWindow {
    fn default() -> Self {
        Self {
            open: false,
            cases: Vec::new(),
            // The defaults: the utility practice (root ball, allowance,
            // base ground point, search distance), as the command line.
            params: VegetationParams {
                fall_in: true,
                ..VegetationParams::default()
            },
            swing_arc: false,
            ice: (Ice::Off, 5.0),
            base_limit: (true, DEFAULT_MAX_BASE_GROUND_DISTANCE),
            corridor: DEFAULT_SEARCH_DISTANCE,
            iso: (false, IsoParams::default()),
            job: Job::default(),
            fresh: false,
        }
    }
}

impl VegetationWindow {
    /// Opens the window on the case shown (ticked, if nothing is).
    pub fn show(&mut self, case: usize) {
        self.open = true;
        if self.cases.is_empty() {
            self.cases.push(case);
        }
    }

    /// The last run's answer, and whether one runs.
    #[cfg(test)]
    pub fn result(&self) -> (Option<&Result<Report, String>>, bool) {
        (self.job.result.as_ref(), self.job.running())
    }

    /// The isoclearance lines of the last report, for the drawings.
    pub fn isoclearance_lines(&self) -> Option<Arc<crate::drawings::IsoLines>> {
        let Some(Ok(r)) = &self.job.result else {
            return None;
        };
        let (lines, top) = r.lines.as_ref()?;
        Some(Arc::new((
            lines.iter().map(|c| (c.level, c.points.clone())).collect(),
            *top,
        )))
    }

    /// What to draw of a report that came since the last call.
    pub fn take_overlay(&mut self) -> Option<Overlay> {
        if !std::mem::take(&mut self.fresh) {
            return None;
        }
        Some(match &self.job.result {
            Some(Ok(r)) => Overlay::of(r),
            _ => Overlay::default(),
        })
    }

    fn start(&mut self, model: &Model, ctx: &egui::Context) {
        let Some(spec) = model.cloud.clone() else {
            self.job.result = Some(Err("no point cloud in this model".into()));
            return;
        };
        let classes = model.scheme.codes_with_role("vegetation");
        if classes.is_empty() {
            self.job.result = Some(Err(format!(
                "no class of \"{}\" has the role vegetation",
                model.scheme.name
            )));
            return;
        }
        let cases: Vec<ohl_weather::WeatherCase> = self
            .cases
            .iter()
            .filter_map(|&k| model.cases.get(k).map(|c| c.case.clone()))
            .collect();
        if cases.is_empty() {
            self.job.result = Some(Err("tick a weather case".into()));
            return;
        }
        let line = model.line.clone();
        let library = model.library.clone();
        let ground = model.ground_for_checks();
        let mut params = self.params.clone();
        params.max_base_ground_distance = self.base_limit.0.then_some(self.base_limit.1);
        let swing_arc = self.swing_arc;
        let ice = match self.ice.0 {
            Ice::Off => None,
            Ice::OfCase => Some(UnbalancedIce::CaseIce),
            Ice::Thickness => Some(UnbalancedIce::ExtraThickness(self.ice.1 / 1000.0)),
            Ice::Extra => Some(UnbalancedIce::ExtraLoad(self.ice.1)),
            Ice::Concentrated => Some(UnbalancedIce::Concentrated(self.ice.1)),
        };
        let corridor_width = self.corridor;
        let iso = self.iso.0.then_some(self.iso.1);
        let origin = model.origin;
        self.job.start_at(ctx, model.revision(), move || {
            // Exit clause: ~1 µs a point and position set; an hour is far
            // beyond what a line needs.
            let budget =
                ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(3600));
            let mut positions = ohl_clearance::positions::solve_positions(
                &line,
                &library,
                &cases,
                PositionOptions { swing_arc },
                &budget,
            )?;
            // The points, as whole millimetres from the model's origin.
            let (file, name) = ohl_lidar::cloud_spec(&spec);
            let source =
                ohl_pointcloud::CloudSource::open(file, name).map_err(|e| e.to_string())?;
            let reach = params.near_reach().max(if params.fall_in {
                params.fall_reach()
            } else {
                0.0
            });
            let corridor = ohl_lidar::Corridor::along_spans(&line, reach.max(corridor_width));
            let mm = |v: f64, k: usize| ((v - origin[k]) * 1000.0).round() as i32;
            let mut points: Vec<([i32; 3], u8)> = Vec::new();
            ohl_pointcloud::PointSource::visit(
                &source,
                Some(&corridor),
                Some(&classes),
                &budget,
                &mut |x, y, z, c| points.push(([mm(x, 0), mm(y, 1), mm(z, 2)], c)),
            )?;
            let at = |v: i32, k: usize| v as f64 / 1000.0 + origin[k];
            let mut notes = positions.notes.clone();
            if let Some(mode) = ice {
                let spans = ohl_clearance::vegetation::spans_near(
                    &positions,
                    reach,
                    points.iter().map(|(p, _)| [at(p[0], 0), at(p[1], 1)]),
                );
                let solves = ohl_clearance::positions::add_unbalanced_ice(
                    &mut positions,
                    &line,
                    &library,
                    &cases,
                    mode,
                    &spans,
                    &budget,
                )?;
                notes.push(format!(
                    "unbalanced ice: {} of {} spans with vegetation within reach, {solves} solves",
                    spans.len(),
                    positions.spans.len()
                ));
            }
            let positions = Arc::new(positions);
            let mut check =
                VegetationCheck::with_positions(Arc::clone(&positions), &classes, params.clone());
            check.visit_parallel(
                ground.as_deref(),
                &points,
                |(p, c)| (at(p[0], 0), at(p[1], 1), at(p[2], 2), *c),
                &budget,
            )?;
            if params.fall_in && ground.is_none() {
                notes.push("no ground surface: no fall-in check".into());
            } else if check.without_ground > 0 {
                notes.push(format!(
                    "{} points without a tree base (no fall-in check): {} with no ground below them, {} with no ground point within the distance",
                    check.without_ground,
                    check.without_ground - check.ground_too_far,
                    check.ground_too_far
                ));
            }
            let lines = match (iso, ground.as_deref()) {
                (Some(iso), Some(g)) => {
                    let area = ohl_lidar::Corridor::along_spans(&line, corridor_width);
                    let bounds = area.bounds().ok_or("no span to draw the lines along")?;
                    let found = ohl_clearance::isoclearance::isoclearance(
                        &positions,
                        g,
                        bounds,
                        &|x, y| area.contains(x, y),
                        &iso,
                        &budget,
                    )?;
                    Some((found.contours, iso.max_level))
                }
                (Some(_), None) => {
                    notes.push("no ground surface: no isoclearance lines".into());
                    None
                }
                _ => None,
            };
            let points = check.points;
            let results = check.finish();
            Ok(Report {
                positions,
                results,
                params,
                points,
                lines,
                notes,
            })
        });
    }

    /// What the last report was computed from that changed since, in
    /// words (`None`: nothing, or no report): the line, the LiDAR points,
    /// the ground ([`crate::revision`]).
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        matches!(self.job.result, Some(Ok(_)))
            .then(|| self.job.stamp.out_of_date(model, USES))
            .flatten()
    }

    /// The window; returns a span to go to.
    pub fn ui(&mut self, ctx: &egui::Context, model: &Model) -> Option<Selection> {
        let was = self.job.running();
        self.job.poll();
        if was && !self.job.running() {
            self.fresh = true;
        }
        let mut go = None;
        let mut again = false;
        let stale = self.out_of_date(model);
        let mut open = self.open;
        egui::Window::new("Vegetation")
            .open(&mut open)
            .default_width(860.0)
            .show(ctx, |ui| {
                self.settings_ui(ui, model);
                let report = match &self.job.result {
                    None => {
                        ui.label(
                            "Vegetation near the wires of the cases ticked (each windy case \
                             blown left and right, along its swing arc if ticked), now and \
                             after growth, and trees that could fall onto them turning about \
                             their root ball.",
                        );
                        return;
                    }
                    Some(Err(e)) => {
                        crate::menu::error_label(ui, e);
                        return;
                    }
                    Some(Ok(r)) => r,
                };
                if let Some(what) = &stale {
                    again = crate::revision::banner(ui, what, !self.job.running());
                }
                go = report_ui(ui, report);
            });
        self.open = open;
        if again {
            self.start(model, ctx);
        }
        go
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        let p = &mut self.params;
        let num = |ui: &mut egui::Ui, label: &str, v: &mut f64, speed: f64, max: f64| {
            ui.label(label);
            ui.add(egui::DragValue::new(v).speed(speed).range(0.0..=max));
        };
        ui.horizontal(|ui| {
            num(ui, "clearance m", &mut p.clearance, 0.1, 100.0);
            num(ui, "growth m/year", &mut p.growth_per_year, 0.05, 5.0);
            num(ui, "years", &mut p.years, 0.1, 30.0);
            num(ui, "buffer m", &mut p.buffer, 0.05, 20.0);
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut p.fall_in, "trees falling in");
            ui.add_enabled_ui(p.fall_in, |ui| {
                num(ui, "root ball × height", &mut p.root_ball, 0.01, 0.5);
                num(ui, "allowance m", &mut p.fall_allowance, 0.05, 20.0);
                num(ui, "tallest tree m", &mut p.max_tree_height, 0.5, 120.0);
                ui.checkbox(&mut self.base_limit.0, "ground point within m")
                    .on_hover_text(
                        "A tree's base is taken from the ground only with a ground point \
                         this near in plan (else no fall-in check)",
                    );
                ui.add_enabled(
                    self.base_limit.0,
                    egui::DragValue::new(&mut self.base_limit.1)
                        .speed(0.1)
                        .range(0.1..=100.0),
                );
            });
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.swing_arc, "swing arc")
                .on_hover_text("Each span anywhere between its left and right blowout");
            ui.label("unbalanced ice");
            let names = [
                (Ice::Off, "off"),
                (Ice::OfCase, "the case's ice on the span"),
                (Ice::Thickness, "more ice mm"),
                (Ice::Extra, "more ice load N/m"),
                (Ice::Concentrated, "load at mid-span N"),
            ];
            let shown = names
                .iter()
                .find(|n| n.0 == self.ice.0)
                .map_or("off", |n| n.1);
            egui::ComboBox::from_id_salt("vegetation-ice")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for (k, n) in names {
                        ui.selectable_value(&mut self.ice.0, k, n);
                    }
                });
            if matches!(self.ice.0, Ice::Thickness | Ice::Extra | Ice::Concentrated) {
                ui.add(
                    egui::DragValue::new(&mut self.ice.1)
                        .speed(1.0)
                        .range(0.0..=1e6),
                );
            }
            num(ui, "search ± m", &mut self.corridor, 1.0, 500.0);
            ui.label("points kept per span");
            ui.add(egui::DragValue::new(&mut self.params.keep).range(0..=1000));
        });
        ui.horizontal(|ui| {
            let iso = &mut self.iso;
            ui.checkbox(&mut iso.0, "isoclearance lines");
            ui.add_enabled_ui(iso.0, |ui| {
                num(ui, "every m", &mut iso.1.interval, 0.1, 50.0);
                num(ui, "hide above m", &mut iso.1.max_level, 0.5, 200.0);
                num(ui, "grid m", &mut iso.1.spacing, 0.1, 10.0);
            });
        });
        // The cases: a tick each (the list stays open while ticking).
        egui::CollapsingHeader::new(format!("Weather cases ({} ticked)", self.cases.len()))
            .id_salt("vegetation-cases")
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(140.0)
                    .show(ui, |ui| {
                        for (k, c) in model.cases.iter().enumerate() {
                            let mut on = self.cases.contains(&k);
                            if ui.checkbox(&mut on, &c.case.name).changed() {
                                if on {
                                    self.cases.push(k);
                                    self.cases.sort_unstable();
                                } else {
                                    self.cases.retain(|x| *x != k);
                                }
                            }
                        }
                    });
            });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!self.job.running(), egui::Button::new("Check"))
                .on_hover_text(
                    "Solve the cases, read the vegetation points again and check every span",
                )
                .clicked()
            {
                self.start(model, ui.ctx());
            }
            self.job.status_ui(ui);
        });
    }
}

/// The results: totals, exports, a table of the spans with findings.
fn report_ui(ui: &mut egui::Ui, report: &Report) -> Option<Selection> {
    let mut go = None;
    egui::ScrollArea::vertical()
        .id_salt("vegetation-notes")
        .max_height(60.0)
        .show(ui, |ui| {
            for n in &report.notes {
                ui.label(egui::RichText::new(n).small().weak());
            }
        });
    let p = &report.params;
    ui.horizontal(|ui| {
        let (enc, fall) = report
            .results
            .iter()
            .fold((0, 0), |(e, f), r| (e + r.encroaching, f + r.fall_in));
        ui.label(format!(
            "{} vegetation points, {} positions; grow-in {enc} within {:.2} m, fall-in {fall}",
            report.points,
            report.positions.positions.len(),
            p.clearance + p.growth() + p.buffer
        ));
        if ui.button("Export CSV…").clicked() {
            save_csv(
                "danger-trees.csv",
                &ohl_clearance::vegetation::danger_csv(&report.positions, &report.results, p),
            );
        }
        if let Some((lines, _)) = &report.lines
            && ui
                .button(format!("Export {} lines (CSV)…", lines.len()))
                .clicked()
        {
            save_csv(
                "isoclearance.csv",
                &ohl_clearance::isoclearance::to_csv(lines),
            );
        }
    });
    let place = |k: Option<usize>| k.map_or("-".to_owned(), |k| report.positions.describe(k));
    egui::ScrollArea::vertical()
        .id_salt("vegetation-rows")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("vegetation-rows")
                .striped(true)
                .spacing([14.0, 3.0])
                .show(ui, |ui| {
                    for h in [
                        "span",
                        "grow-in",
                        "fall-in",
                        "nearest m",
                        "case, position",
                        "fall margin m",
                        "case, position",
                    ] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for (s, r) in report
                        .positions
                        .spans
                        .iter()
                        .zip(&report.results)
                        .filter(|(_, r)| r.encroaching > 0 || r.fall_in > 0)
                    {
                        let w = &s.wire;
                        if ui
                            .link(format!("{} span {}", w.section, w.span))
                            .on_hover_text("Go to this span")
                            .clicked()
                        {
                            go = Some(Selection::Span {
                                section: w.section.clone(),
                                span: w.span,
                            });
                        }
                        let error = ui.visuals().error_fg_color;
                        let red = |n: u64| {
                            let t = egui::RichText::new(n.to_string()).monospace();
                            if n > 0 { t.color(error) } else { t }
                        };
                        ui.label(red(r.encroaching));
                        ui.label(red(r.fall_in));
                        ui.label(or_dash(r.nearest.map(|v| v.distance), 2));
                        ui.label(place(r.nearest.and_then(|v| v.position)));
                        ui.label(or_dash(r.worst_fall.and_then(|v| v.fall_margin), 2));
                        ui.label(place(r.worst_fall.and_then(|v| v.fall_position)));
                        ui.end_row();
                    }
                });
        });
    go
}
