// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The plan view: alignments, wires and structures from above, north up.
//! Drag: pan, scroll: zoom at the pointer, click: select, double-click:
//! centre on the object.

use eframe::egui;

use crate::model::Model;
use crate::selection::{self, Selection};

/// State of the plan view.
#[derive(Debug, Default)]
pub struct PlanView {
    /// The weather case of the copy picked by the last click (`Some(None)`:
    /// the case shown); taken by the app.
    pub picked_case: Option<Option<usize>>,
    /// Project coordinates at the centre of the view.
    centre: Option<[f64; 2]>,
    /// Pixels per metre.
    scale: f64,
    /// Clearance violations to mark (points on the wires).
    pub marks: Vec<[f64; 3]>,
    /// The check that found them is out of date: drawn dimmed.
    pub marks_stale: bool,
    /// What Checks › Vegetation found.
    pub vegetation: crate::vegetation::Overlay,
    /// The lines on the ground beside the line (none: not shown).
    pub ground_lines: Option<std::sync::Arc<crate::ground_lines::Drawn>>,
    /// What Checks › Safety volumes built (their outlines).
    pub safety_volume: crate::safety_volume::Overlay,
    /// The magnetic field's isolines from above.
    pub magnetic_field: crate::field_overlay::Overlay,
    /// The electric field's isolines from above (none yet).
    pub electric_field: crate::field_overlay::Overlay,
    /// Other users (server mode): where each looks, with their name.
    pub peers: Vec<(String, [f64; 3])>,
    /// Where it was drawn in the last frame (its page in front), if it was.
    pub drawn: Option<egui::Rect>,
    /// Orthophoto tiles read.
    backgrounds: crate::backgrounds::PlanCache,
    /// The wires' colours, line codes and directions.
    styles: crate::wire_style::StyleCache,
    /// The alignments drawn.
    alignments: AlignmentViews,
    /// The sheet preview's sheets drawn in the last frame.
    pub sheets_drawn: usize,
    /// A measuring tool is held: what is under the cursor is found every
    /// frame the pointer moves ([`PlanView::hover_use`]).
    pub measuring: bool,
    /// What is under the cursor, as a click of the tool button would
    /// find it (a measuring tool held).
    pub hover_use: Option<selection::Use>,
    /// The measuring tools' lines and points.
    pub measure: crate::measure::Overlay,
    /// The kept measurement or note under the last tool click (its
    /// annotation's id); taken by the app.
    pub annotation_hit: Option<String>,
}

/// What Checks › Vegetation found, on the layers shown: isoclearance lines
/// in their colours, every grow-in point (a dot) and every danger tree (a
/// triangle at its base), in the user's colours; one mark a pixel.
fn paint_vegetation(
    painter: &egui::Painter,
    rect: egui::Rect,
    to_screen: &impl Fn([f64; 3]) -> egui::Pos2,
    overlay: &crate::vegetation::Overlay,
    visible: &std::collections::HashSet<crate::scene::Layer>,
) {
    use crate::scene::Layer;
    let color = |c: [u8; 3]| {
        crate::revision::dimmed32(egui::Color32::from_rgb(c[0], c[1], c[2]), overlay.stale)
    };
    // One mark a pixel: 10⁶ points stay quick to draw (what is drawn only).
    let mut dotted = crate::profile::PixelSet::new(rect);
    if visible.contains(&Layer::Isoclearance) {
        for (_, rgb, points) in &overlay.lines {
            let pts: Vec<egui::Pos2> = points.iter().map(|p| to_screen(*p)).collect();
            if pts.iter().any(|p| rect.expand(50.0).contains(*p)) {
                painter.add(egui::Shape::line(pts, egui::Stroke::new(1.2, color(*rgb))));
            }
        }
    }
    if visible.contains(&Layer::GrowIn) {
        let c = color(overlay.colors[0]);
        for p in &overlay.grow {
            let p = to_screen(*p);
            if rect.contains(p) && dotted.insert(p) {
                painter.circle_filled(p, 3.0, c);
            }
        }
    }
    let mut dotted = crate::profile::PixelSet::new(rect);
    if visible.contains(&Layer::FallIn) {
        let c = color(overlay.colors[1]);
        for (_, base) in &overlay.fall {
            let b = to_screen(*base);
            if !rect.contains(b) || !dotted.insert(b) {
                continue;
            }
            let d = 4.0;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    b + egui::vec2(0.0, -d),
                    b + egui::vec2(d * 0.87, d * 0.5),
                    b + egui::vec2(-d * 0.87, d * 0.5),
                ],
                c,
                egui::Stroke::NONE,
            ));
        }
    }
}

/// The sheet preview on the plan: each sheet's band along its alignment
/// (the line width each side, as the sheets' plan shows it), the match
/// lines across it at the sheet's ends, its number; warned sheets amber.
/// How many sheets were drawn.
fn paint_sheets(
    painter: &egui::Painter,
    rect: egui::Rect,
    to_screen: &impl Fn([f64; 3]) -> egui::Pos2,
    sheets: Option<&Result<crate::drawings::Preview, String>>,
) -> usize {
    use ohl_line::route::point_at;
    let note = |text: String| {
        painter.text(
            rect.left_top() + egui::vec2(10.0, 10.0),
            egui::Align2::LEFT_TOP,
            text,
            egui::FontId::proportional(11.0),
            egui::Color32::from_gray(170),
        );
    };
    let p = match sheets {
        None => return 0,
        Some(Err(e)) => {
            note(format!("Sheet preview: {e}"));
            return 0;
        }
        Some(Ok(p)) => p,
    };
    if p.paper.is_none() {
        note("Sheet preview: the export writes one continuous drawing".into());
        return 0;
    }
    if !p.plan {
        note("Sheet preview: the sheets have no plan (the profile alone)".into());
        return 0;
    }
    let blue = egui::Color32::from_rgb(70, 140, 230);
    let warn = egui::Color32::from_rgb(230, 160, 50);
    let w = p.line_width;
    let mut drawn = 0;
    for r in &p.routes {
        let c = &r.centreline;
        if c.len() < 2 {
            continue;
        }
        let at = |s: f64, o: f64| {
            let q = point_at(c, s, o);
            to_screen([q[0], q[1], 0.0])
        };
        // The structures' elevations the sheets carry (the export's own
        // layout), and their leaders.
        let grey = egui::Color32::from_gray(150);
        for e in &r.elevations {
            for (line, closed) in &e.lines {
                let pts: Vec<egui::Pos2> = line.iter().map(|p| at(p[0], p[1])).collect();
                let stroke = egui::Stroke::new(1.0, grey);
                painter.add(if *closed {
                    egui::Shape::closed_line(pts, stroke)
                } else {
                    egui::Shape::line(pts, stroke)
                });
            }
            if let Some([a, b]) = e.leader {
                painter.extend(egui::Shape::dashed_line(
                    &[at(a[0], a[1]), at(b[0], b[1])],
                    egui::Stroke::new(0.7, grey),
                    3.0,
                    3.0,
                ));
            }
        }
        for s in &r.sheets {
            let col = if s.warnings.is_empty() { blue } else { warn };
            let n = 24;
            let edge = |o: f64| -> Vec<egui::Pos2> {
                (0..=n)
                    .map(|k| at(s.from + (s.to - s.from) * k as f64 / n as f64, o))
                    .collect()
            };
            let (left, right) = (edge(-w), edge(w));
            let box_ = egui::Rect::from_points(&[left.clone(), right.clone()].concat());
            if !box_.intersects(rect) {
                continue;
            }
            drawn += 1;
            let faint = col.gamma_multiply(0.6);
            painter.add(egui::Shape::line(left, egui::Stroke::new(1.0, faint)));
            painter.add(egui::Shape::line(right, egui::Stroke::new(1.0, faint)));
            for end in [s.from, s.to] {
                painter.extend(egui::Shape::dashed_line(
                    &[at(end, -w), at(end, w)],
                    egui::Stroke::new(1.5, blue),
                    6.0,
                    4.0,
                ));
            }
            // The number outside the band, clear of the structures' names
            // on the line (beyond its right edge, 14 px out on screen).
            let mid = 0.5 * (s.from + s.to);
            let (axis, edge) = (at(mid, 0.0), at(mid, w));
            let out = (edge - axis).normalized();
            let out = if out.x.is_finite() && out.y.is_finite() {
                out
            } else {
                egui::vec2(0.0, 1.0)
            };
            painter.text(
                edge + out * 14.0,
                egui::Align2::CENTER_CENTER,
                format!(
                    "{} sheet {}{}",
                    r.id,
                    s.number,
                    if s.warnings.is_empty() { "" } else { " ⚠" }
                ),
                egui::FontId::proportional(11.0),
                col,
            );
        }
    }
    drawn
}

/// Length of a unit of the phases' line codes on screen, px.
const DASH_PX: f64 = 3.0;

/// A wire on screen in its line code, and its direction as an arrow head
/// (`Some`: forward or back) at its middle.
pub(crate) fn styled_wire(
    painter: &egui::Painter,
    pts: &[egui::Pos2],
    code: &[f64],
    arrow: Option<bool>,
    stroke: egui::Stroke,
) {
    let mut out = Vec::new();
    styled_wire_shapes(&mut out, pts, code, arrow, stroke);
    painter.extend(out);
}

/// [`styled_wire`]'s shapes, added to `out`.
pub(crate) fn styled_wire_shapes(
    out: &mut Vec<egui::Shape>,
    pts: &[egui::Pos2],
    code: &[f64],
    arrow: Option<bool>,
    stroke: egui::Stroke,
) {
    let flat: Vec<[f64; 3]> = pts
        .iter()
        .map(|p| [f64::from(p.x), f64::from(p.y), 0.0])
        .collect();
    let back = |l: &[[f64; 3]]| -> Vec<egui::Pos2> {
        l.iter()
            .map(|p| egui::pos2(p[0] as f32, p[1] as f32))
            .collect()
    };
    if code.is_empty() {
        out.push(egui::Shape::line(pts.to_vec(), stroke));
    } else {
        for d in crate::wire_style::dashes(&flat, code, DASH_PX) {
            out.push(egui::Shape::line(back(&d), stroke));
        }
    }
    if let Some(forward) = arrow
        && let Some(legs) =
            crate::wire_style::chevron(&flat, forward, 9.0, crate::wire_style::across)
    {
        for l in legs {
            out.push(egui::Shape::line(
                back(&l),
                egui::Stroke::new(1.5, stroke.color),
            ));
        }
    }
}

fn color(c: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(c[0], c[1], c[2])
}

/// The line's alignment views ([`ohl_line::Line::alignment_views`]: the
/// structures on each found by distance), kept between frames and made
/// again when what they come from changes: the alignments, their points,
/// the structures' ids, positions and routes (a hash of them, much
/// cheaper than the views).
#[derive(Debug, Default)]
pub(crate) struct AlignmentViews {
    key: Option<u64>,
    views: Vec<ohl_line::route::AlignmentView>,
}

impl AlignmentViews {
    /// The views of `line`.
    pub(crate) fn get(&mut self, line: &ohl_line::Line) -> &[ohl_line::route::AlignmentView] {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for a in &line.alignments {
            (&a.id, &a.name, &a.route, &a.points).hash(&mut h);
        }
        for p in &line.alignment_points {
            (&p.id, &p.structure, p.position.map(f64::to_bits)).hash(&mut h);
        }
        for s in &line.structures {
            (&s.id, &s.routes, s.position.map(f64::to_bits)).hash(&mut h);
        }
        let key = h.finish();
        if self.key != Some(key) {
            self.views = line.alignment_views();
            self.key = Some(key);
        }
        &self.views
    }
}

/// What of the plan is in view, in project coordinates (a margin
/// around the screen), to leave out what is not ([`Cull::sees`]).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Cull {
    lo: [f64; 2],
    hi: [f64; 2],
}

impl Cull {
    /// The plan seen in `rect` (centred on `centre`, `scale` px/m), with
    /// `margin` px around.
    pub(crate) fn new(rect: egui::Rect, centre: [f64; 2], scale: f64, margin: f32) -> Self {
        let half = [
            f64::from(rect.width() / 2.0 + margin) / scale,
            f64::from(rect.height() / 2.0 + margin) / scale,
        ];
        Self {
            lo: [centre[0] - half[0], centre[1] - half[1]],
            hi: [centre[0] + half[0], centre[1] + half[1]],
        }
    }

    /// Whether any of a polyline may be in view (its extent overlaps).
    pub(crate) fn sees(&self, points: &[[f64; 3]]) -> bool {
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for p in points {
            for k in 0..2 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        lo[0] <= self.hi[0] && hi[0] >= self.lo[0] && lo[1] <= self.hi[1] && hi[1] >= self.lo[1]
    }
}

/// A polyline on screen without the points closer than half a pixel to
/// the last one kept (its ends kept): the same line on screen, without
/// the points a far view crowds into one pixel.
pub(crate) fn on_screen(
    points: &[[f64; 3]],
    to_screen: impl Fn([f64; 3]) -> egui::Pos2,
) -> Vec<egui::Pos2> {
    thin_px(points.iter().map(|p| to_screen(*p)))
}

/// Screen points without those closer than half a pixel to the last one
/// kept; the last point kept too.
pub(crate) fn thin_px(points: impl Iterator<Item = egui::Pos2>) -> Vec<egui::Pos2> {
    let mut out: Vec<egui::Pos2> = Vec::new();
    let mut skipped: Option<egui::Pos2> = None;
    for q in points {
        match out.last() {
            Some(last) if last.distance_sq(q) < 0.25 => skipped = Some(q),
            _ => {
                out.push(q);
                skipped = None;
            }
        }
    }
    out.extend(skipped);
    out
}

impl PlanView {
    fn fit(&mut self, model: &Model, size: egui::Vec2) {
        let pts: Vec<[f64; 3]> = model.line.structures.iter().map(|s| s.position).collect();
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for p in &pts {
            for k in 0..2 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        if pts.is_empty() {
            lo = [model.origin[0] - 100.0, model.origin[1] - 100.0];
            hi = [model.origin[0] + 100.0, model.origin[1] + 100.0];
        }
        self.centre = Some([0.5 * (lo[0] + hi[0]), 0.5 * (lo[1] + hi[1])]);
        let w = (hi[0] - lo[0]).max(50.0) * 1.1;
        let h = (hi[1] - lo[1]).max(50.0) * 1.1;
        self.scale = (f64::from(size.x) / w).min(f64::from(size.y) / h);
    }

    /// Pixels per metre (0 before the first fit).
    #[cfg(test)]
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Project coordinates at the centre of the view (benchmark, tests).
    #[cfg(test)]
    pub fn centre(&self) -> Option<[f64; 2]> {
        self.centre
    }

    /// Looks at `centre` at `scale` px/m (benchmark, tests).
    #[cfg(test)]
    pub fn set_view(&mut self, centre: [f64; 2], scale: f64) {
        self.centre = Some(centre);
        self.scale = scale;
    }

    /// Fits the line again on the next frame.
    pub fn refit(&mut self) {
        self.centre = None;
    }

    /// Centres the view on a point.
    pub fn centre_on(&mut self, p: [f64; 3]) {
        self.centre = Some([p[0], p[1]]);
    }

    /// Shows the view (with the sheet preview's `sheets` when it is on);
    /// `buttons`: the select and tool buttons
    /// ([`crate::controls::click_buttons`]).
    #[allow(clippy::too_many_arguments)]
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        case: usize,
        selection: &mut Selection,
        snaps: &selection::Snaps,
        visible: &std::collections::HashSet<crate::scene::Layer>,
        sheets: Option<&Result<crate::drawings::Preview, String>>,
        buttons: (egui::PointerButton, egui::PointerButton),
    ) -> Option<selection::Use> {
        // The button whose click selects, the one whose click uses the
        // held tool ([`crate::controls::click_buttons`]).
        let (select_button, tool_button) = buttons;
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        self.drawn = Some(rect);
        // Not yet scaled (scale 0): fit, keeping a centre asked for before
        // (selecting in the 3D view centres the plan; before the plan was
        // first shown that left it at scale 0, every object on one point —
        // an empty-looking plan).
        let unscaled = !(self.scale.is_finite() && self.scale > 0.0);
        if (self.centre.is_none() || unscaled) && rect.width() > 1.0 && rect.height() > 1.0 {
            let wanted = self.centre.filter(|_| unscaled);
            self.fit(model, rect.size());
            if let Some(c) = wanted {
                self.centre = Some(c);
            }
        }
        if self.centre.is_none() || self.scale.is_nan() || self.scale <= 0.0 {
            return None;
        }
        let mid = rect.center();
        if response.dragged() {
            let d = response.drag_delta();
            let c = self.centre.as_mut().expect("fitted");
            c[0] -= f64::from(d.x) / self.scale;
            c[1] += f64::from(d.y) / self.scale;
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0
                && let Some(pos) = response.hover_pos()
            {
                // Zoom about the pointer.
                let c = self.centre.expect("fitted");
                let at = [
                    c[0] + f64::from(pos.x - mid.x) / self.scale,
                    c[1] - f64::from(pos.y - mid.y) / self.scale,
                ];
                let f = (f64::from(scroll) * 0.002).exp();
                self.scale = (self.scale * f).clamp(1e-4, 500.0);
                self.centre = Some([
                    at[0] - f64::from(pos.x - mid.x) / self.scale,
                    at[1] + f64::from(pos.y - mid.y) / self.scale,
                ]);
            }
        }
        let c = self.centre.expect("fitted");
        let scale = self.scale;
        let to_screen = move |p: [f64; 3]| {
            egui::pos2(
                mid.x + ((p[0] - c[0]) * scale) as f32,
                mid.y - ((p[1] - c[1]) * scale) as f32,
            )
        };
        // What a click of the tool button at `pos` finds there.
        let use_at = |pos: egui::Pos2| {
            // The plan view draws no LiDAR points: none can be clicked.
            let target = selection::pick_shown(
                model,
                case,
                snaps,
                [f64::from(pos.x), f64::from(pos.y)],
                12.0,
                &|p| {
                    let q = to_screen(p);
                    Some(([f64::from(q.x), f64::from(q.y)], 0.0))
                },
                &selection::LidarShown::Nothing,
            )
            .0;
            let (x, y) = (
                c[0] + f64::from(pos.x - mid.x) / scale,
                c[1] - f64::from(pos.y - mid.y) / scale,
            );
            // Without ground: the height of the nearest structure's base.
            let z = model.ground_at(x, y).unwrap_or_else(|| {
                model
                    .line
                    .structures
                    .iter()
                    .min_by(|a, b| {
                        let d =
                            |s: &&ohl_line::Structure| (s.position[0] - x).hypot(s.position[1] - y);
                        d(a).total_cmp(&d(b))
                    })
                    .map_or(model.origin[2], |s| s.position[2])
            });
            let on = selection::on_span(
                model,
                case,
                &target,
                [f64::from(pos.x), f64::from(pos.y)],
                |p| {
                    let q = to_screen(p);
                    Some([f64::from(q.x), f64::from(q.y)])
                },
            );
            selection::Use {
                target,
                at: Some([x, y, z]),
                on,
            }
        };
        // A click of the tool button uses the held tool on what is there.
        let used = if response.clicked_by(tool_button)
            && let Some(pos) = response.interact_pointer_pos()
        {
            self.annotation_hit = self.measure.hit(&|p| Some(to_screen(p)), pos, None);
            Some(use_at(pos))
        } else {
            None
        };
        // A measuring tool held: what is under the cursor, when it moved.
        if !self.measuring {
            self.hover_use = None;
        } else if let Some(pos) = response.hover_pos() {
            if ui.input(|i| i.pointer.is_moving()) || self.hover_use.is_none() {
                self.hover_use = Some(use_at(pos));
            }
        } else {
            self.hover_use = None;
        }
        if (response.clicked_by(select_button) || response.double_clicked_by(select_button))
            && let Some(pos) = response.interact_pointer_pos()
        {
            let (picked, picked_case) = selection::pick_shown(
                model,
                case,
                snaps,
                [f64::from(pos.x), f64::from(pos.y)],
                12.0,
                &|p| {
                    let q = to_screen(p);
                    Some(([f64::from(q.x), f64::from(q.y)], 0.0))
                },
                &selection::LidarShown::Nothing,
            );
            selection::clicked(ui.ctx(), &picked);
            *selection = picked;
            self.picked_case = Some(picked_case);
            if response.double_clicked_by(select_button)
                && let Some(f) = selection.focus(model, case)
            {
                self.centre_on(f);
            }
        }
        let painter = ui.painter_at(rect);
        let backgrounds = crate::perf::span("plan backgrounds");
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(24, 28, 34));
        self.backgrounds.paint(
            ui.ctx(),
            &painter,
            model,
            visible,
            rect,
            &to_screen,
            c,
            scale,
        );
        drop(backgrounds);
        // The lines on the ground beside the line, under the line.
        if visible.contains(&crate::scene::Layer::GroundLines) {
            crate::ground_lines::paint_plan(
                &painter,
                rect,
                &to_screen,
                self.ground_lines.as_deref(),
                &model.ground_lines,
            );
        }
        let wires = crate::perf::span("plan wires");
        let line = &model.line;
        let cull = Cull::new(rect, c, scale, 20.0);
        for r in self.alignments.get(line) {
            let pts: Vec<egui::Pos2> = r.centreline(line).into_iter().map(to_screen).collect();
            painter.add(egui::Shape::line(
                pts,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(90, 200, 220)),
            ));
        }
        // Every alignment point (those on no alignment too).
        for a in &line.alignment_points {
            let selected = selection::all_selected(selection, model)
                .any(|x| matches!(x, Selection::AlignmentPoint { id } if *id == a.id));
            painter.circle_filled(
                to_screen(a.position),
                if selected { 5.0 } else { 3.0 },
                if selected {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::from_rgb(90, 200, 220)
                },
            );
        }
        // Off the route worked on: faded.
        let scope = model.scope();
        let faded = egui::Color32::from_rgba_unmultiplied(130, 134, 140, 70);
        {
            // Each section in each of its display cases, coloured and
            // coded as Line › Sections › Display options say.
            // (Only what is in view; a far view's points crowded into a
            // pixel drawn once.)
            let shapes = model.shapes_shown_ref(case);
            let display = &model.section_display;
            let arrows = display.direction[crate::annotations::View::Plan as usize];
            let styles = self.styles.get(line, display.coloring);
            for shape in shapes {
                let style = styles.section(&shape.section);
                let forward = styles.directions.sections.get(&shape.section).copied();
                for (k, span) in shape.spans.iter().enumerate() {
                    if !cull.sees(&span.points) {
                        continue;
                    }
                    let selected = selection::all_selected(selection, model).any(|x| {
                        matches!(x, Selection::Span { section, span: n }
                        if *section == shape.section && *n == k)
                    });
                    let pts = on_screen(&span.points, to_screen);
                    let off = scope
                        .as_ref()
                        .is_some_and(|s| !s.sections.contains(&shape.section));
                    let (w, col) = if selected {
                        (4.0, egui::Color32::WHITE)
                    } else if off {
                        (1.0, faded)
                    } else {
                        (1.5, color(style.color))
                    };
                    let arrow = forward.filter(|_| arrows);
                    styled_wire(
                        &painter,
                        &pts,
                        &style.pattern,
                        arrow,
                        egui::Stroke::new(w, col),
                    );
                }
            }
            // Sections not solved in a case they are drawn in: dashed red
            // where they were (last solved shape, another case's, or the
            // chords); a click picks the span, its properties say why.
            let [r, g, b] = crate::scene::UNSOLVED_COLOR;
            let hover = response.hover_pos();
            let mut why: Option<String> = None;
            for (section, k_case, drawn) in model.unsolved_shown(case) {
                for (k, points) in drawn.spans.iter().enumerate() {
                    if !cull.sees(points) {
                        continue;
                    }
                    let selected = selection::all_selected(selection, model).any(|x| {
                        matches!(x, Selection::Span { section: s, span: n }
                        if *s == section && *n == k)
                    });
                    let pts = on_screen(points, to_screen);
                    // Hovered: a tooltip says why it is not solved.
                    if let Some(h) = hover
                        && why.is_none()
                        && pts.windows(2).any(|w| {
                            ohl_core::geometry::distance_to_segment(
                                [f64::from(h.x), f64::from(h.y)],
                                [f64::from(w[0].x), f64::from(w[0].y)],
                                [f64::from(w[1].x), f64::from(w[1].y)],
                            ) <= 6.0
                        })
                    {
                        why = Some(format!(
                            "{section} not solved in {}: {} — drawn dashed: {}",
                            model.cases[k_case].case.name, drawn.reason, drawn.from
                        ));
                    }
                    let (w, col) = if selected {
                        (4.0, egui::Color32::WHITE)
                    } else {
                        (2.0, egui::Color32::from_rgb(r, g, b))
                    };
                    styled_wire(
                        &painter,
                        &pts,
                        &crate::scene::UNSOLVED_DASHES,
                        None,
                        egui::Stroke::new(w, col),
                    );
                }
            }
            if let Some(text) = why {
                response.clone().on_hover_text_at_pointer(text);
            }
        }
        let styles = self.styles.get(line, model.section_display.coloring);
        for solved in model.cases.get(case).into_iter().flat_map(|c| c.drawn()) {
            for j in &solved.jumpers {
                if !cull.sees(&j.points) {
                    continue;
                }
                let selected = selection::all_selected(selection, model)
                    .any(|x| matches!(x, Selection::Jumper(id) if *id == j.jumper));
                let style = styles.jumper(&j.jumper);
                let arrow = styles
                    .directions
                    .jumpers
                    .get(&j.jumper)
                    .copied()
                    .filter(|_| {
                        model.section_display.direction[crate::annotations::View::Plan as usize]
                    });
                let pts = on_screen(&j.points, to_screen);
                let off = scope
                    .as_ref()
                    .is_some_and(|s| !s.jumpers.contains(&j.jumper));
                let (w, col) = if selected {
                    (4.0, egui::Color32::WHITE)
                } else if off {
                    (1.0, faded)
                } else {
                    (1.0, color(style.color))
                };
                styled_wire(
                    &painter,
                    &pts,
                    &style.pattern,
                    arrow,
                    egui::Stroke::new(w, col),
                );
            }
        }
        // Other project lines, in their colours.
        for other in &model.other_lines {
            let col = color(other.color);
            let whole = model.other_lines_whole;
            let d = &other.differs;
            // As the working line: each section in its display cases,
            // jumpers in the cases shown.
            for (k, solved) in other.cases.iter().enumerate() {
                for shape in &solved.shapes {
                    if !model.section_cases(&shape.section, case).contains(&k)
                        || !(whole || d.sections.contains(&shape.section))
                    {
                        continue;
                    }
                    for span in shape.spans.iter().filter(|s| cull.sees(&s.points)) {
                        let pts = on_screen(&span.points, to_screen);
                        painter.add(egui::Shape::line(pts, egui::Stroke::new(1.5, col)));
                    }
                }
                if model.shown(case).contains(&k) {
                    for j in solved
                        .jumpers
                        .iter()
                        .filter(|j| whole || d.jumpers.contains(&j.jumper))
                        .filter(|j| cull.sees(&j.points))
                    {
                        let pts = on_screen(&j.points, to_screen);
                        painter.add(egui::Shape::line(pts, egui::Stroke::new(1.0, col)));
                    }
                }
            }
            for st in other
                .line
                .structures
                .iter()
                .filter(|s| whole || d.structures.contains(&s.id))
            {
                let p = to_screen(st.position);
                if rect.expand(10.0).contains(p) {
                    painter.circle_stroke(p, 4.0, egui::Stroke::new(1.5, col));
                }
            }
        }
        drop(wires);
        let notes = crate::perf::span("plan labels");
        let mut placer = crate::annotations::Placer::default();
        // The structures' names first, at every zoom: they take their
        // place before any other label; where they would overlap, the
        // first one in line order is written (the selected one always).
        let plan = crate::annotations::View::Plan as usize;
        let with_description =
            model.section_display.structure_label[plan] && model.section_display.descriptions[plan];
        let name_color = egui::Color32::from_gray(200);
        let mut names: Vec<(egui::Pos2, std::sync::Arc<egui::Galley>)> = Vec::new();
        let chosen: Vec<String> = selection::all_selected(selection, model)
            .filter_map(|x| match x {
                Selection::Structure(id) => Some(id.clone()),
                _ => None,
            })
            .collect();
        let is_selected = |s: &ohl_line::Structure| chosen.contains(&s.id);
        for s in line
            .structures
            .iter()
            .filter(|s| is_selected(s))
            .chain(line.structures.iter().filter(|s| !is_selected(s)))
        {
            let p = to_screen(s.position);
            if !rect.expand(20.0).contains(p) {
                continue;
            }
            let text = if with_description && !s.description.trim().is_empty() {
                format!("{} — {}", s.id, s.description.trim())
            } else {
                s.id.clone()
            };
            let galley = painter.layout_no_wrap(text, egui::FontId::proportional(11.0), name_color);
            let at =
                egui::Align2::LEFT_BOTTOM.anchor_size(p + egui::vec2(6.0, -6.0), galley.size());
            if is_selected(s) {
                placer.force(at);
            } else if !placer.reserve(at) {
                continue;
            }
            names.push((at.min, galley));
        }
        // Section labels and sags: of the spans in view, long enough on
        // the screen (the others are not made).
        let in_view = Cull::new(rect, c, scale, 60.0);
        let keep = |pts: &[[f64; 3]], mid: [f64; 3]| {
            let (Some(a), Some(b)) = (pts.first(), pts.last()) else {
                return false;
            };
            in_view.sees(&[mid])
                && to_screen(*a).distance(to_screen(*b)) >= crate::annotations::MIN_SPAN_PX
        };
        for note in
            crate::annotations::notes_where(model, case, crate::annotations::View::Plan, keep)
        {
            placer.paint(&painter, to_screen(note.at), &note);
        }
        // Labels of the structures' points, strings, wire points (their
        // names are written above).
        for note in
            crate::annotations::object_notes(model, case, crate::annotations::View::Plan, None)
        {
            if note.kind == crate::annotations::Kind::Structure {
                continue;
            }
            let p = to_screen(note.at);
            if rect.contains(p) {
                placer.paint(&painter, p, &note);
            }
        }
        drop(notes);
        let _structures = crate::perf::span("plan structures and rest");
        for s in &line.structures {
            let p = to_screen(s.position);
            if !rect.expand(20.0).contains(p) {
                continue;
            }
            let selected = is_selected(s);
            let off = scope
                .as_ref()
                .is_some_and(|x| !x.structures.contains(&s.id));
            painter.circle_filled(
                p,
                if selected { 6.0 } else { 3.5 },
                if selected {
                    egui::Color32::WHITE
                } else if off {
                    faded
                } else if let Some(m) = model.usage_marks.get(&s.id) {
                    // Coloured by usage (a legend below).
                    let [r, g, b] = m.color();
                    egui::Color32::from_rgb(r, g, b)
                } else {
                    egui::Color32::from_gray(215)
                },
            );
        }
        for (at, galley) in names {
            painter.galley(at, galley, name_color);
        }
        paint_vegetation(&painter, rect, &to_screen, &self.vegetation, visible);
        if visible.contains(&crate::scene::Layer::SafetyVolume) {
            crate::safety_volume::paint_plan(&painter, rect, &to_screen, &self.safety_volume);
        }
        if visible.contains(&crate::scene::Layer::MagneticField) {
            crate::field_overlay::paint_plan(&painter, rect, &to_screen, &self.magnetic_field);
        }
        if visible.contains(&crate::scene::Layer::ElectricField) {
            crate::field_overlay::paint_plan(&painter, rect, &to_screen, &self.electric_field);
        }
        for m in &self.marks {
            let p = to_screen(*m);
            if rect.expand(10.0).contains(p) {
                painter.circle_stroke(
                    p,
                    6.0,
                    egui::Stroke::new(
                        2.0,
                        crate::revision::dimmed32(
                            egui::Color32::from_rgb(255, 60, 50),
                            self.marks_stale,
                        ),
                    ),
                );
            }
        }
        self.sheets_drawn = paint_sheets(&painter, rect, &to_screen, sheets);
        // Other users: a diamond where they look, their name.
        for (name, at) in &self.peers {
            let p = to_screen(*at);
            if rect.expand(10.0).contains(p) {
                let c = egui::Color32::from_rgb(90, 200, 255);
                let d = 6.0;
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        p + egui::vec2(0.0, -d),
                        p + egui::vec2(d, 0.0),
                        p + egui::vec2(0.0, d),
                        p + egui::vec2(-d, 0.0),
                    ],
                    c,
                    egui::Stroke::new(1.0, egui::Color32::BLACK),
                ));
                painter.text(
                    p + egui::vec2(8.0, -8.0),
                    egui::Align2::LEFT_BOTTOM,
                    name,
                    egui::FontId::proportional(12.0),
                    c,
                );
            }
        }
        if !model.usage_marks.is_empty() {
            crate::loads::paint_usage_legend(&painter, rect, model.usage_out_of_date);
        }
        self.measure.paint(&painter, &|p| Some(to_screen(p)), None);
        // Scale bar.
        let target = 120.0 / scale;
        let nice = [1.0, 2.0, 5.0]
            .iter()
            .flat_map(|m| (-2..6).map(move |e| m * 10f64.powi(e)))
            .filter(|v| *v <= target)
            .fold(1.0, f64::max);
        let len = (nice * scale) as f32;
        let a = rect.left_bottom() + egui::vec2(12.0, -14.0);
        painter.line_segment(
            [a, a + egui::vec2(len, 0.0)],
            egui::Stroke::new(2.0, egui::Color32::from_gray(200)),
        );
        painter.text(
            a + egui::vec2(len + 6.0, 0.0),
            egui::Align2::LEFT_CENTER,
            format!("{nice} m"),
            egui::FontId::proportional(11.0),
            egui::Color32::from_gray(200),
        );
        painter.text(
            rect.right_top() + egui::vec2(-10.0, 10.0),
            egui::Align2::RIGHT_TOP,
            "N",
            egui::FontId::proportional(13.0),
            egui::Color32::from_gray(200),
        );
        // The cursor's project coordinates (2026-10-25: to place a point
        // at a given easting and northing).
        if let Some(pos) = response.hover_pos() {
            let e = c[0] + f64::from(pos.x - mid.x) / scale;
            let n = c[1] - f64::from(pos.y - mid.y) / scale;
            painter.text(
                rect.right_bottom() + egui::vec2(-10.0, -8.0),
                egui::Align2::RIGHT_BOTTOM,
                format!("E {e:.2}  N {n:.2}"),
                egui::FontId::monospace(11.0),
                egui::Color32::from_gray(200),
            );
        }
        used
    }
}
