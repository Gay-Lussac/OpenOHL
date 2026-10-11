// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The profile view of a route: ground along the centreline, wires and
//! structures by station; heights fitted to the visible stretch, or drawn
//! at a fixed ratio to the stations (View › Profile aspect ratio), kept
//! when zooming. Scroll: zoom at the pointer, drag: move (up and down too
//! at a fixed ratio), click: select, double-click: show the whole route.

use eframe::egui;
use ols_drawing::RouteDrawing;

use crate::model::Model;
use crate::scene::rgb;
use crate::selection::Selection;

/// State of the profile view.
#[derive(Default)]
pub struct ProfileView {
    /// Route shown (index in the line's routes).
    pub route: usize,
    /// The route was chosen in the list (else: the one with most structures).
    pub(crate) chosen: bool,
    /// Every alignment, one after the other in their order.
    pub(crate) all: bool,
    /// The corridor's half width typed (the application keeps it).
    pub(crate) new_corridor: Option<f64>,
    pub(crate) drawing: Option<(Key, Built)>,
    /// Visible stations.
    range: Option<[f64; 2]>,
    /// At a fixed aspect ratio: the height at the middle of the view.
    z_centre: Option<f64>,
    /// What the last frame showed: stations, heights, its size in points.
    pub(crate) shown: Option<([f64; 2], [f64; 2], [f64; 2])>,
    /// Where the last frame drew it.
    pub(crate) rect: Option<egui::Rect>,
    /// The wires' colours, line codes and directions.
    styles: crate::wire_style::StyleCache,
}

/// What the drawing was made for: the route (none: all), the weather
/// case, the corridor's half width.
type Key = (Option<usize>, usize, u64);

/// The profile drawing and what is seen in the corridor around it.
pub(crate) struct Built {
    pub(crate) drawing: RouteDrawing,
    /// Survey (ground surface) points in the corridor: station, z.
    pub(crate) survey: Vec<[f64; 2]>,
    /// 3D drawing lines in the corridor: drawing, colour, (station, z).
    pub(crate) lines: Vec<(String, [u8; 3], Vec<[f64; 2]>)>,
    /// 3D drawing points in the corridor: drawing, colour, (station, z).
    pub(crate) points: Vec<(String, [u8; 3], [f64; 2])>,
    /// All alignments: where each begins, and its name.
    pub(crate) joints: Vec<(f64, String)>,
}

/// Most survey points drawn (more are thinned evenly).
const MAX_SURVEY: usize = 200_000;

/// What lies within `depth` of the centreline `centre` (`length` long):
/// the ground surface's points, the 3D lines and points of the drawings.
fn corridor(model: &Model, centre: &[[f64; 3]], length: f64, depth: f64) -> Built {
    let mut out = Built {
        drawing: RouteDrawing {
            route: String::new(),
            name: String::new(),
            length,
            centreline: Vec::new(),
            stations: Vec::new(),
            structures: Vec::new(),
            ground: Vec::new(),
            sides: Vec::new(),
            wires: Vec::new(),
            markers: Vec::new(),
        },
        survey: Vec::new(),
        lines: Vec::new(),
        points: Vec::new(),
        joints: Vec::new(),
    };
    if depth <= 0.0 || centre.len() < 2 {
        return out;
    }
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in centre {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k] - depth);
            hi[k] = hi[k].max(p[k] + depth);
        }
    }
    // Within the corridor: beside the centreline (not beyond its ends).
    let inside = |p: [f64; 3]| -> Option<[f64; 2]> {
        if p[0] < lo[0] || p[0] > hi[0] || p[1] < lo[1] || p[1] > hi[1] {
            return None;
        }
        let (st, off) = ols_line::route::project(centre, p);
        let back = ols_line::route::point_at(centre, st, off);
        let beside = (back[0] - p[0]).hypot(back[1] - p[1]) < 0.01 * depth.max(1.0);
        (off.abs() <= depth && beside).then_some([st, p[2]])
    };
    if let Some(g) = model.ground.as_deref() {
        let pts = g.points();
        let near: Vec<&[f64; 3]> = pts
            .iter()
            .filter(|p| p[0] >= lo[0] && p[0] <= hi[0] && p[1] >= lo[1] && p[1] <= hi[1])
            .collect();
        let stride = near.len().div_ceil(MAX_SURVEY).max(1);
        out.survey = near
            .into_iter()
            .step_by(stride)
            .filter_map(|p| inside(*p))
            .collect();
    }
    for d in &model.backgrounds.drawings {
        let name = &d.info.name;
        for l in &d.drawing.lines {
            // Flat (2D) drawings have no heights to show.
            if l.points.iter().all(|p| p[2] == 0.0) {
                continue;
            }
            let mut run: Vec<[f64; 2]> = Vec::new();
            for p in &l.points {
                match inside(*p) {
                    Some(q) => run.push(q),
                    None if run.len() > 1 => {
                        out.lines
                            .push((name.clone(), l.color, std::mem::take(&mut run)))
                    }
                    None => run.clear(),
                }
            }
            if run.len() > 1 {
                out.lines.push((name.clone(), l.color, run));
            }
        }
        for m in &d.drawing.points {
            if m.at[2] != 0.0
                && let Some(q) = inside(m.at)
            {
                out.points.push((name.clone(), m.color, q));
            }
        }
    }
    out
}

/// `b` moved `ds` along the stations.
fn shifted(mut b: Built, ds: f64) -> Built {
    let d = &mut b.drawing;
    for run in d
        .ground
        .iter_mut()
        .chain(d.sides.iter_mut().flat_map(|s| &mut s.1))
    {
        for p in run {
            p[0] += ds;
        }
    }
    for w in &mut d.wires {
        for p in &mut w.points {
            p[0] += ds;
        }
    }
    for m in &mut d.structures {
        m.station += ds;
        for p in m
            .attachments
            .iter_mut()
            .chain(m.sticks.iter_mut().flatten())
        {
            p[0] += ds;
        }
    }
    for s in &mut d.stations {
        s.station += ds;
    }
    for m in &mut d.markers {
        m.0[0] += ds;
    }
    for p in b
        .survey
        .iter_mut()
        .chain(b.points.iter_mut().map(|x| &mut x.2))
    {
        p[0] += ds;
    }
    for l in &mut b.lines {
        for p in &mut l.2 {
            p[0] += ds;
        }
    }
    b
}

/// The alignments one after the other: each begins where the one before
/// ends (its first station after the last of the one before).
fn joined(parts: Vec<(String, Built)>) -> Option<Built> {
    let mut iter = parts.into_iter();
    let (name, first) = iter.next()?;
    let mut out = first;
    out.joints.push((0.0, name));
    let mut names = vec![out.drawing.name.clone()];
    for (name, b) in iter {
        let at = out.drawing.length;
        let b = shifted(b, at);
        out.joints.push((at, name));
        names.push(b.drawing.name.clone());
        let (d, e) = (&mut out.drawing, b.drawing);
        d.length += e.length;
        d.centreline.extend(e.centreline);
        d.stations.extend(e.stations);
        d.structures.extend(e.structures);
        d.ground.extend(e.ground);
        for (off, prof) in e.sides {
            match d.sides.iter_mut().find(|s| s.0 == off) {
                Some(s) => s.1.extend(prof),
                None => d.sides.push((off, prof)),
            }
        }
        d.wires.extend(e.wires);
        d.markers.extend(e.markers);
        out.survey.extend(b.survey);
        out.lines.extend(b.lines);
        out.points.extend(b.points);
    }
    out.drawing.name = format!("All alignments ({})", names.join(", "));
    Some(out)
}

impl ProfileView {
    /// Forgets the drawing (after the model or case changed).
    pub fn invalidate(&mut self) {
        self.drawing = None;
    }

    /// Shows the view.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        case: usize,
        selection: &mut Selection,
        snaps: &crate::selection::Snaps,
    ) -> Option<Selection> {
        let routes = model.line.effective_routes();
        if routes.is_empty() {
            ui.label("No route.");
            return None;
        }
        if !self.chosen {
            // The main run first, not a short spur listed before it.
            self.route = (0..routes.len())
                .rev()
                .max_by_key(|&k| routes[k].structures.len())
                .unwrap_or(0);
        }
        self.route = self.route.min(routes.len() - 1);
        ui.horizontal(|ui| {
            ui.label("Route");
            egui::ComboBox::from_id_salt("profile-route")
                .selected_text(if self.all {
                    "All alignments, in order".to_owned()
                } else {
                    format!("{} {}", routes[self.route].id, routes[self.route].name)
                })
                .show_ui(ui, |ui| {
                    for (k, r) in routes.iter().enumerate() {
                        if ui
                            .selectable_label(
                                !self.all && self.route == k,
                                format!("{} {}", r.id, r.name),
                            )
                            .clicked()
                        {
                            self.route = k;
                            self.all = false;
                            self.range = None;
                            self.chosen = true;
                        }
                    }
                    if routes.len() > 1
                        && ui
                            .selectable_label(self.all, "All alignments, in order")
                            .on_hover_text(
                                "Every alignment one after the other: the first, then the \
                                 second…",
                            )
                            .clicked()
                    {
                        self.all = true;
                        self.range = None;
                        self.chosen = true;
                    }
                });
            ui.separator();
            ui.label("corridor ±");
            let mut depth = model.profile_corridor;
            if ui
                .add(
                    egui::DragValue::new(&mut depth)
                        .range(0.0..=500.0)
                        .speed(0.5)
                        .max_decimals(1)
                        .suffix(" m"),
                )
                .on_hover_text(
                    "How far left and right of the alignment the ground, the survey points and \
                     the 3D drawings (DXF) are shown: the ground at both edges (thin, darker), the \
                     points and lines inside projected on the profile. 0: the centreline only",
                )
                .changed()
            {
                self.new_corridor = Some(depth.max(0.0));
            }
        });
        self.all &= routes.len() > 1;
        let depth = self.new_corridor.unwrap_or(model.profile_corridor).max(0.0);
        let key = ((!self.all).then_some(self.route), case, depth.to_bits());
        if self.drawing.as_ref().is_none_or(|(k, _)| *k != key) {
            let solved = model.cases.get(case);
            let shapes = model.shapes_shown(case);
            let options = ols_drawing::Options {
                side_offsets: if depth > 0.0 {
                    vec![-depth, depth]
                } else {
                    Vec::new()
                },
                ..ols_drawing::Options::default()
            };
            let wanted: Vec<usize> = if self.all {
                (0..routes.len()).collect()
            } else {
                vec![self.route]
            };
            let mut parts = Vec::new();
            for k in wanted {
                let d = ols_drawing::build(
                    &model.line,
                    &routes[k],
                    &shapes,
                    solved.map_or(&[][..], |s| &s.jumpers),
                    model.ground.as_deref(),
                    &[],
                    &options,
                );
                match d {
                    Ok(d) => {
                        let mut b = corridor(model, &d.centreline, d.length, depth);
                        b.drawing = d;
                        parts.push((format!("{} {}", routes[k].id, routes[k].name), b));
                    }
                    Err(e) => {
                        ui.label(format!("profile of {}: {e}", routes[k].id));
                        return None;
                    }
                }
            }
            self.drawing = Some((key, joined(parts)?));
        }
        let built = &self.drawing.as_ref().expect("built").1;
        let d = &built.drawing;
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        let range = self.range.get_or_insert([0.0, d.length.max(1.0)]);
        let width = f64::from(rect.width()).max(1.0);
        let height = f64::from(rect.height()).max(1.0);
        // A fixed ratio: pixels per metre of height, from the stations'.
        let ratio = model.profile_ratio.filter(|k| *k > 0.0);
        let vscale = |r: &[f64; 2]| ratio.map(|k| k * width / (r[1] - r[0]).max(1e-9));
        let before = *range;
        if response.dragged() {
            let ds = -f64::from(response.drag_delta().x) / width * (range[1] - range[0]);
            range[0] += ds;
            range[1] += ds;
            if let (Some(v), Some(z)) = (vscale(range), self.z_centre.as_mut()) {
                *z += f64::from(response.drag_delta().y) / v;
            }
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0
                && let Some(pos) = response.hover_pos()
            {
                let at = range[0] + f64::from(pos.x - rect.min.x) / width * (range[1] - range[0]);
                let f = (-f64::from(scroll) * 0.002).exp();
                let span = ((range[1] - range[0]) * f).clamp(5.0, d.length.max(5.0) * 2.0);
                let t = (at - range[0]) / (range[1] - range[0]);
                range[0] = at - t * span;
                range[1] = range[0] + span;
                // The height under the pointer stays under it.
                if let (Some(v0), Some(v1), Some(z)) =
                    (vscale(&before), vscale(range), self.z_centre.as_mut())
                {
                    let dy = f64::from(rect.center().y - pos.y);
                    let z_at = *z + dy / v0;
                    *z = z_at - dy / v1;
                }
            }
        }
        if response.double_clicked() {
            *range = [0.0, d.length.max(1.0)];
            self.z_centre = None;
        }
        let [s0, s1] = *range;
        // Heights of what is visible.
        let (mut z0, mut z1) = (f64::INFINITY, f64::NEG_INFINITY);
        let mut see = |s: f64, z: f64| {
            if s >= s0 && s <= s1 {
                z0 = z0.min(z);
                z1 = z1.max(z);
            }
        };
        for run in d.ground.iter().chain(d.sides.iter().flat_map(|s| &s.1)) {
            for p in run {
                see(p[0], p[1]);
            }
        }
        for w in &d.wires {
            for p in &w.points {
                see(p[0], p[2]);
            }
        }
        for m in &d.structures {
            see(m.station, m.base);
            see(m.station, m.top);
        }
        if !z0.is_finite() {
            (z0, z1) = (0.0, 100.0);
        }
        let pad = (z1 - z0).max(10.0) * 0.08;
        let (z0, z1) = match vscale(range) {
            // Fixed ratio: the heights the view's height holds, around
            // its centre (first: the middle of what is seen).
            Some(v) => {
                let c = *self.z_centre.get_or_insert(0.5 * (z0 + z1));
                let half = 0.5 * height / v;
                (c - half, c + half)
            }
            None => {
                self.z_centre = None;
                (z0 - pad, z1 + pad)
            }
        };
        self.shown = Some(([s0, s1], [z0, z1], [width, height]));
        self.rect = Some(rect);
        let to_screen = move |s: f64, z: f64| {
            egui::pos2(
                rect.min.x + ((s - s0) / (s1 - s0) * width) as f32,
                rect.max.y - ((z - z0) / (z1 - z0) * f64::from(rect.height())) as f32,
            )
        };
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(24, 28, 34));
        // Height grid.
        let step = [1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0]
            .into_iter()
            .find(|s| (z1 - z0) / s <= 8.0)
            .unwrap_or(1000.0);
        let mut z = (z0 / step).ceil() * step;
        while z < z1 {
            let y = to_screen(s0, z).y;
            painter.line_segment(
                [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
                egui::Stroke::new(1.0, egui::Color32::from_gray(45)),
            );
            painter.text(
                egui::pos2(rect.max.x - 4.0, y - 2.0),
                egui::Align2::RIGHT_BOTTOM,
                format!("{z:.0}"),
                egui::FontId::proportional(10.0),
                egui::Color32::from_gray(130),
            );
            z += step;
        }
        let visible = |p: &[f64; 2]| p[0] >= s0 - (s1 - s0) && p[0] <= s1 + (s1 - s0);
        // The corridor: survey points, the ground at its edges, the 3D
        // drawings in it (those not hidden), under the centreline ground.
        for p in built.survey.iter().filter(|p| p[0] >= s0 && p[0] <= s1) {
            painter.circle_filled(
                to_screen(p[0], p[1]),
                1.2,
                egui::Color32::from_rgb(120, 150, 110),
            );
        }
        for (_, prof) in &d.sides {
            for run in prof {
                let pts: Vec<egui::Pos2> = run
                    .iter()
                    .filter(|p| visible(p))
                    .map(|p| to_screen(p[0], p[1]))
                    .collect();
                painter.add(egui::Shape::line(
                    pts,
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(120, 100, 75)),
                ));
            }
        }
        let shown_drawing = |name: &str| {
            !model
                .hidden_backgrounds
                .contains(&crate::backgrounds::drawing_key(name))
        };
        for (name, c, run) in &built.lines {
            if !shown_drawing(name) || run.iter().all(|p| p[0] < s0 || p[0] > s1) {
                continue;
            }
            let pts: Vec<egui::Pos2> = run.iter().map(|p| to_screen(p[0], p[1])).collect();
            painter.add(egui::Shape::line(
                pts,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(c[0], c[1], c[2])),
            ));
        }
        for (name, c, p) in &built.points {
            if shown_drawing(name) && p[0] >= s0 && p[0] <= s1 {
                painter.circle_filled(
                    to_screen(p[0], p[1]),
                    2.0,
                    egui::Color32::from_rgb(c[0], c[1], c[2]),
                );
            }
        }
        for run in &d.ground {
            let pts: Vec<egui::Pos2> = run
                .iter()
                .filter(|p| visible(p))
                .map(|p| to_screen(p[0], p[1]))
                .collect();
            painter.add(egui::Shape::line(
                pts,
                egui::Stroke::new(1.5, egui::Color32::from_rgb(170, 140, 100)),
            ));
        }
        // All alignments: where each begins.
        if built.joints.len() > 1 {
            for (at, name) in &built.joints {
                if *at < s0 || *at > s1 {
                    continue;
                }
                let x = to_screen(*at, z0).x;
                painter.line_segment(
                    [egui::pos2(x, rect.min.y + 22.0), egui::pos2(x, rect.max.y)],
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(90, 120, 170)),
                );
                painter.text(
                    egui::pos2(x + 4.0, rect.min.y + 24.0),
                    egui::Align2::LEFT_TOP,
                    name,
                    egui::FontId::proportional(10.5),
                    egui::Color32::from_rgb(140, 170, 220),
                );
            }
        }
        for m in &d.structures {
            let selected = matches!(selection, Selection::Structure(id) if *id == m.id);
            let col = if selected {
                egui::Color32::WHITE
            } else {
                egui::Color32::from_gray(190)
            };
            if m.sticks.is_empty() {
                painter.line_segment(
                    [to_screen(m.station, m.base), to_screen(m.station, m.top)],
                    egui::Stroke::new(if selected { 3.0 } else { 1.5 }, col),
                );
            } else {
                for st in &m.sticks {
                    let pts: Vec<egui::Pos2> = st.iter().map(|p| to_screen(p[0], p[2])).collect();
                    painter.add(egui::Shape::line(
                        pts,
                        egui::Stroke::new(if selected { 2.0 } else { 1.0 }, col),
                    ));
                }
            }
            // Kept inside the view at its ends.
            let mut at = to_screen(m.station, m.top) + egui::vec2(0.0, -6.0);
            at.x = at.x.clamp(rect.min.x + 24.0, rect.max.x - 24.0);
            painter.text(
                at,
                egui::Align2::CENTER_BOTTOM,
                &m.id,
                egui::FontId::proportional(11.0),
                col,
            );
        }
        // Coloured and coded as Sections › Display options say.
        let display = &model.section_display;
        let arrows = display.direction[crate::annotations::View::Profile as usize];
        let styles = self.styles.get(&model.line, display.coloring);
        for w in &d.wires {
            let traced = trace_selection(&w.label);
            let selected = traced.as_ref().is_some_and(|s| s == selection);
            let (style, forward) = match &traced {
                Some(Selection::Span { section, .. }) => (
                    Some(styles.section(section)),
                    styles.directions.sections.get(section).copied(),
                ),
                Some(Selection::Jumper(id)) => (
                    Some(styles.jumper(id)),
                    styles.directions.jumpers.get(id).copied(),
                ),
                _ => (None, None),
            };
            let c = style.as_ref().map_or_else(|| rgb(&w.color), |s| s.color);
            let pts: Vec<egui::Pos2> = w.points.iter().map(|p| to_screen(p[0], p[2])).collect();
            crate::plan::styled_wire(
                &painter,
                &pts,
                style.as_ref().map_or(&[][..], |s| &s.pattern),
                forward.filter(|_| arrows),
                egui::Stroke::new(
                    if selected { 3.5 } else { 1.3 },
                    if selected {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::from_rgb(c[0], c[1], c[2])
                    },
                ),
            );
        }
        // Section labels and sags (Sections › Display options), on this
        // route's traces of the spans.
        let notes = crate::annotations::notes(model, case, crate::annotations::View::Profile);
        if !notes.is_empty() {
            let mut placer = crate::annotations::Placer::default();
            for w in &d.wires {
                let Some(Selection::Span { section, span }) = trace_selection(&w.label) else {
                    continue;
                };
                let (Some(first), Some(last)) = (w.points.first(), w.points.last()) else {
                    continue;
                };
                let (a, b) = (to_screen(first[0], first[2]), to_screen(last[0], last[2]));
                if a.distance(b) < crate::annotations::MIN_SPAN_PX {
                    continue;
                }
                // The trace's point at mid-station.
                let mid = 0.5 * (first[0] + last[0]);
                let Some(m) = w
                    .points
                    .iter()
                    .min_by(|p, q| (p[0] - mid).abs().total_cmp(&(q[0] - mid).abs()))
                else {
                    continue;
                };
                for note in notes
                    .iter()
                    .filter(|n| n.section == section && n.span == span)
                {
                    placer.paint(&painter, to_screen(m[0], m[2]), note);
                }
            }
        }
        painter.text(
            rect.left_top() + egui::vec2(8.0, 6.0),
            egui::Align2::LEFT_TOP,
            format!(
                "{} — stations {} to {}, {}",
                d.name,
                ols_drawing::format_station(s0.max(0.0)),
                ols_drawing::format_station(s1.min(d.length)),
                match ratio {
                    Some(k) => format!("aspect 1:{} (heights ×{})", ratio_text(k), ratio_text(k)),
                    None => "heights not to scale".into(),
                }
            ),
            egui::FontId::proportional(11.0),
            egui::Color32::from_gray(150),
        );
        // Picking: structures by station, wires by nearest point. A left
        // click selects; a right-click uses the held tool.
        let pick = |pos: egui::Pos2| -> Selection {
            let mut best: Option<(f32, Selection)> = None;
            let mut consider = |d: f32, s: Selection| {
                let on = s.kind().is_some_and(|k| snaps.on(k));
                if on && d < 12.0 && best.as_ref().is_none_or(|b| d < b.0) {
                    best = Some((d, s));
                }
            };
            for m in &d.structures {
                let a = to_screen(m.station, m.base);
                let b = to_screen(m.station, m.top);
                let dx = (pos.x - a.x).abs();
                if pos.y >= b.y - 12.0 && pos.y <= a.y + 4.0 {
                    consider(dx - 3.0, Selection::Structure(m.id.clone()));
                }
            }
            for w in &d.wires {
                let Some(sel) = trace_selection(&w.label) else {
                    continue;
                };
                let dmin = w
                    .points
                    .iter()
                    .map(|p| to_screen(p[0], p[2]).distance(pos))
                    .fold(f32::INFINITY, f32::min);
                consider(dmin, sel);
            }
            best.map_or(Selection::None, |b| b.1)
        };
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            *selection = pick(pos);
        }
        if response.secondary_clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            return Some(pick(pos));
        }
        None
    }
}

/// The aspect ratios offered (heights drawn this many times larger than
/// stations).
pub const RATIOS: [f64; 6] = [1.0, 2.0, 5.0, 10.0, 20.0, 50.0];

/// A ratio as written (`10`, `2.5`).
pub fn ratio_text(k: f64) -> String {
    let t = format!("{k:.2}");
    t.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// View › Profile aspect ratio: fitted heights, a ratio of the list or
/// one typed; whether it changed. The menu stays open (radio choices).
pub fn ratio_menu(ui: &mut egui::Ui, ratio: &mut Option<f64>) -> bool {
    let before = *ratio;
    ui.label(
        egui::RichText::new("Stations : heights in the profile view (yours only)")
            .small()
            .weak(),
    );
    if ui
        .radio(ratio.is_none(), "Fit the heights (changes when zooming)")
        .clicked()
    {
        *ratio = None;
    }
    for k in RATIOS {
        let text = if k == 1.0 {
            "1:1 (true scale)".to_owned()
        } else {
            format!("1:{} (heights ×{})", ratio_text(k), ratio_text(k))
        };
        if ui.radio(*ratio == Some(k), text).clicked() {
            *ratio = Some(k);
        }
    }
    ui.horizontal(|ui| {
        let mut other = ratio.filter(|k| !RATIOS.contains(k)).is_some();
        let mut k = ratio.unwrap_or(10.0);
        if ui.radio(other, "1:").clicked() {
            other = true;
            *ratio = Some(k);
        }
        if ui
            .add_enabled(
                other,
                egui::DragValue::new(&mut k)
                    .range(0.1..=500.0)
                    .speed(0.1)
                    .max_decimals(2),
            )
            .on_hover_text("Heights drawn this many times larger than stations")
            .changed()
        {
            *ratio = Some(k);
        }
    });
    *ratio != before
}

/// What a trace is, from its label: `S1 span 0 (circuit phase)` or
/// `jumper J1`.
fn trace_selection(label: &str) -> Option<Selection> {
    if let Some(id) = label.strip_prefix("jumper ") {
        return Some(Selection::Jumper(id.to_owned()));
    }
    let (sec, rest) = label.split_once(" span ")?;
    let span = rest.split_whitespace().next()?.parse().ok()?;
    Some(Selection::Span {
        section: sec.to_owned(),
        span,
    })
}

#[cfg(test)]
mod tests {
    use crate::selection::Selection;

    #[test]
    fn labels() {
        assert_eq!(
            super::trace_selection("sec12 span 3 (380 kV L1)"),
            Some(Selection::Span {
                section: "sec12".into(),
                span: 3
            })
        );
        assert_eq!(
            super::trace_selection("jumper J1"),
            Some(Selection::Jumper("J1".into()))
        );
        assert_eq!(super::trace_selection("ground"), None);
    }
}
