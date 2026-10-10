// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The plan view: alignments, wires and structures from above, north up.
//! Drag: pan, scroll: zoom at the pointer, click: select, double-click:
//! centre on the object.

use eframe::egui;

use crate::model::Model;
use crate::selection::{self, Selection};

/// State of the plan view.
#[derive(Debug, Default)]
pub struct PlanView {
    /// Project coordinates at the centre of the view.
    centre: Option<[f64; 2]>,
    /// Pixels per metre.
    scale: f64,
    /// Clearance violations to mark (points on the wires).
    pub marks: Vec<[f64; 3]>,
    /// Other users (server mode): where each looks, with their name.
    pub peers: Vec<(String, [f64; 3])>,
    /// Where it was drawn in the last frame (its page in front), if it was.
    pub drawn: Option<egui::Rect>,
    /// Orthophoto tiles read.
    backgrounds: crate::backgrounds::PlanCache,
    /// The wires' colours, line codes and directions.
    styles: crate::wire_style::StyleCache,
}

/// Length of a unit of the phases' line codes on screen, px.
const DASH_PX: f64 = 3.0;

/// A wire on screen in its line code, and its direction as an arrow head
/// (`Some`: forward or back) at its middle.
pub(crate) fn styled_wire(
    painter: &egui::Painter,
    pts: &[egui::Pos2],
    code: usize,
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
    for d in crate::wire_style::dashes(&flat, code, DASH_PX) {
        painter.add(egui::Shape::line(back(&d), stroke));
    }
    if let Some(forward) = arrow
        && let Some(legs) =
            crate::wire_style::chevron(&flat, forward, 9.0, crate::wire_style::across)
    {
        for l in legs {
            painter.add(egui::Shape::line(
                back(&l),
                egui::Stroke::new(1.5, stroke.color),
            ));
        }
    }
}

fn color(c: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(c[0], c[1], c[2])
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

    /// Fits the line again on the next frame.
    pub fn refit(&mut self) {
        self.centre = None;
    }

    /// Centres the view on a point.
    pub fn centre_on(&mut self, p: [f64; 3]) {
        self.centre = Some([p[0], p[1]]);
    }

    /// Shows the view.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        case: usize,
        selection: &mut Selection,
        snaps: &selection::Snaps,
        visible: &std::collections::HashSet<crate::scene::Layer>,
    ) -> Option<selection::Use> {
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
        // A right-click uses the held tool on what is there.
        let used = if response.secondary_clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let target = selection::pick_in_model(
                model,
                case,
                snaps,
                [f64::from(pos.x), f64::from(pos.y)],
                12.0,
                |p| {
                    let q = to_screen(p);
                    Some([f64::from(q.x), f64::from(q.y)])
                },
            );
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
                            |s: &&ols_line::Structure| (s.position[0] - x).hypot(s.position[1] - y);
                        d(a).total_cmp(&d(b))
                    })
                    .map_or(model.origin[2], |s| s.position[2])
            });
            Some(selection::Use {
                target,
                at: Some([x, y, z]),
            })
        } else {
            None
        };
        if (response.clicked() || response.double_clicked())
            && let Some(pos) = response.interact_pointer_pos()
        {
            *selection = selection::pick_in_model(
                model,
                case,
                snaps,
                [f64::from(pos.x), f64::from(pos.y)],
                12.0,
                |p| {
                    let q = to_screen(p);
                    Some([f64::from(q.x), f64::from(q.y)])
                },
            );
            if response.double_clicked()
                && let Some(f) = selection.focus(model, case)
            {
                self.centre_on(f);
            }
        }
        let painter = ui.painter_at(rect);
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
        let line = &model.line;
        for r in line.effective_routes() {
            let pts: Vec<egui::Pos2> = r.centreline(line).into_iter().map(to_screen).collect();
            // Its points, when it has its own.
            if r.alignment.len() >= 2 {
                for (k, p) in pts.iter().enumerate() {
                    let selected = matches!(selection, Selection::AlignmentPoint { route, index }
                        if *route == r.id && *index == k);
                    painter.circle_filled(
                        *p,
                        if selected { 5.0 } else { 3.0 },
                        if selected {
                            egui::Color32::WHITE
                        } else {
                            egui::Color32::from_rgb(90, 200, 220)
                        },
                    );
                }
            }
            painter.add(egui::Shape::line(
                pts,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(90, 200, 220)),
            ));
        }
        {
            // Each section in each of its display cases, coloured and
            // coded as Sections › Display options say.
            let shapes = model.shapes_shown(case);
            let display = &model.section_display;
            let arrows = display.direction[crate::annotations::View::Plan as usize];
            let styles = self.styles.get(line, display.coloring);
            for shape in &shapes {
                let style = styles.section(&shape.section);
                let forward = styles.directions.sections.get(&shape.section).copied();
                for (k, span) in shape.spans.iter().enumerate() {
                    let selected = matches!(selection, Selection::Span { section, span: n }
                        if *section == shape.section && *n == k);
                    let pts: Vec<egui::Pos2> = span.points.iter().map(|p| to_screen(*p)).collect();
                    let (w, col) = if selected {
                        (4.0, egui::Color32::WHITE)
                    } else {
                        (1.5, color(style.color))
                    };
                    let arrow = forward.filter(|_| arrows);
                    styled_wire(&painter, &pts, style.code, arrow, egui::Stroke::new(w, col));
                }
            }
        }
        for solved in model.cases.get(case).into_iter().flat_map(|c| c.drawn()) {
            for j in &solved.jumpers {
                let selected = matches!(selection, Selection::Jumper(id) if *id == j.jumper);
                let styles = self.styles.get(line, model.section_display.coloring);
                let style = styles.jumper(&j.jumper);
                let arrow = styles
                    .directions
                    .jumpers
                    .get(&j.jumper)
                    .copied()
                    .filter(|_| {
                        model.section_display.direction[crate::annotations::View::Plan as usize]
                    });
                let pts: Vec<egui::Pos2> = j.points.iter().map(|p| to_screen(*p)).collect();
                let (w, col) = if selected {
                    (4.0, egui::Color32::WHITE)
                } else {
                    (1.0, color(style.color))
                };
                styled_wire(&painter, &pts, style.code, arrow, egui::Stroke::new(w, col));
            }
        }
        // Other project lines, in their colours.
        for other in &model.other_lines {
            let col = color(other.color);
            // As the working line: each section in its display cases,
            // jumpers in the cases shown.
            for (k, solved) in other.cases.iter().enumerate() {
                for shape in &solved.shapes {
                    if !model.section_cases(&shape.section, case).contains(&k) {
                        continue;
                    }
                    for span in &shape.spans {
                        let pts: Vec<egui::Pos2> =
                            span.points.iter().map(|p| to_screen(*p)).collect();
                        painter.add(egui::Shape::line(pts, egui::Stroke::new(1.5, col)));
                    }
                }
                if model.shown(case).contains(&k) {
                    for j in &solved.jumpers {
                        let pts: Vec<egui::Pos2> = j.points.iter().map(|p| to_screen(*p)).collect();
                        painter.add(egui::Shape::line(pts, egui::Stroke::new(1.0, col)));
                    }
                }
            }
            for st in &other.line.structures {
                painter.circle_stroke(to_screen(st.position), 4.0, egui::Stroke::new(1.5, col));
            }
        }
        let mut placer = crate::annotations::Placer::default();
        for note in crate::annotations::notes(model, case, crate::annotations::View::Plan) {
            let [a, b] = note.ends.map(to_screen);
            if a.distance(b) >= crate::annotations::MIN_SPAN_PX {
                placer.paint(&painter, to_screen(note.at), &note);
            }
        }
        let labels = scale > 0.08;
        for s in &line.structures {
            let p = to_screen(s.position);
            if !rect.expand(20.0).contains(p) {
                continue;
            }
            let selected = matches!(selection, Selection::Structure(id) if *id == s.id);
            painter.circle_filled(
                p,
                if selected { 6.0 } else { 3.5 },
                if selected {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::from_gray(215)
                },
            );
            if labels || selected {
                painter.text(
                    p + egui::vec2(6.0, -6.0),
                    egui::Align2::LEFT_BOTTOM,
                    &s.id,
                    egui::FontId::proportional(11.0),
                    egui::Color32::from_gray(200),
                );
            }
        }
        for m in &self.marks {
            let p = to_screen(*m);
            if rect.expand(10.0).contains(p) {
                painter.circle_stroke(
                    p,
                    6.0,
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 60, 50)),
                );
            }
        }
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
        used
    }
}
