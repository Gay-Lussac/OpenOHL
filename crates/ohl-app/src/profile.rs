// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The profile view of a route: ground along the centreline, wires and
//! structures by station; heights fitted to the visible stretch, or drawn
//! at a fixed ratio to the stations (View › Profile aspect ratio), kept
//! when zooming. Scroll: zoom at the pointer, drag: move (up and down too
//! at a fixed ratio), click: select, double-click: show the whole route.

use eframe::egui;
use ohl_drawing::RouteDrawing;

use crate::model::Model;
use crate::scene::rgb;
use crate::selection::Selection;

/// State of the profile view.
#[derive(Default)]
pub struct ProfileView {
    /// What it shows (none yet: the route with most structures).
    pub(crate) show: Option<Show>,
    pub(crate) drawing: Option<(Key, Built)>,
    /// Each route's corridor as last made (by alignment id), kept while
    /// what it is made from is unchanged (an edit of the line leaves it).
    corridors: Vec<(String, CorridorKey, Built)>,
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
    /// Kept while the view does not move (forgotten with the drawing):
    /// the heights seen at the stations shown,
    extent: Option<([u64; 2], (f64, f64))>,
    /// the corridor's shapes (survey dots, ground, drawings),
    back: Option<((ViewKey, u64), Vec<egui::Shape>)>,
    /// the wires' shapes,
    wires: Option<(WiresKey, Vec<egui::Shape>)>,
    /// the LiDAR points dotted (one per pixel and class colour): where,
    /// and the point (what a click picks, [`crate::selection::LidarShown`]).
    lidar_dots: Vec<(egui::Pos2, crate::model::LidarPoint)>,
    /// The sheet preview's sheets drawn in the last frame: alignment,
    /// sheet number, first and last station on the profile (shifted as
    /// the alignment is), with warnings or not.
    pub(crate) sheets_drawn: Vec<(String, usize, f64, f64, bool)>,
    /// The lines on the ground beside the line, when shown in the
    /// profile too (none: not drawn) ...
    pub ground_lines: Option<std::sync::Arc<crate::ground_lines::Drawn>>,
    /// ... along the alignments shown (the lines they were found for,
    /// the profile's alignments): each side's pieces of (station, height).
    ground_along: Option<GroundAlong>,
    /// What Checks › Vegetation found ...
    pub vegetation: crate::vegetation::Overlay,
    /// ... and whether its grow-in and danger-tree layers are shown;
    pub vegetation_layers: [bool; 2],
    /// its points' stations and offsets along a route (route, the
    /// report's serial, grow-in, danger trees),
    vegetation_along: Option<VegetationAlong>,
    /// counted up when another report is shown.
    vegetation_serial: u64,
    /// The clearances below a requirement (the Clearances window, the
    /// Vertical and Radial clearances windows), as in the 3D view: (the
    /// end at the wire, the end at the target); a vertical one has the
    /// same x and y at both ends, so it is drawn vertical here too ...
    pub marks: Vec<([f64; 3], [f64; 3])>,
    /// ... dimmed while out of date;
    pub marks_stale: bool,
    /// their ends' stations and offsets along the profile's centreline
    /// ([`MarksAlong`]).
    marks_along: MarksAlong,
    /// Counted up when the drawing (its centreline) is made again.
    drawing_serial: u64,
    /// A box or lasso being drawn to select LiDAR points
    /// ([`crate::area`]).
    pub area: Option<crate::area::AreaDrag>,
    /// The LiDAR points a box or lasso selected (its dots), how many, how
    /// long finding them took; taken by the app.
    pub area_picked: Option<(Vec<crate::model::LidarPoint>, usize, std::time::Duration)>,
    /// A right-click opens a menu of its own with "Cross-section here"
    /// (off in the app: the object's menu has it, [`ProfileView::cut_at`]).
    pub cut_menu: bool,
    /// Where the last right-click was: alignment, station on it.
    cut_at: Option<(String, f64)>,
    /// "Cross-section here" chosen: alignment, station; taken by the app.
    pub cut_request: Option<(String, f64)>,
    /// A measuring tool is held: what is under the cursor is found every
    /// frame the pointer moves ([`ProfileView::hover_use`]).
    pub measuring: bool,
    /// What is under the cursor, as a click of the tool button would
    /// find it (a measuring tool held).
    pub hover_use: Option<crate::selection::Use>,
    /// The measuring tools' lines and points.
    pub measure: crate::measure::Overlay,
    /// The kept measurement or note under the last tool click (its
    /// annotation's id); taken by the app.
    pub annotation_hit: Option<String>,
}

/// The stations and offsets of the vegetation points along a route: the
/// route, the report's serial, grow-in, danger trees.
type VegetationAlong = (String, u64, Vec<(f64, f64)>, Vec<(f64, f64)>);

/// A clearance mark: the end at the wire, the end at the target.
type Mark = ([f64; 3], [f64; 3]);

/// The clearance marks' ends along the profile's centreline: each end's
/// (station, offset), worked out once per drawing and set of marks
/// (2026-11-10: both ends of every mark were projected onto the
/// centreline on every frame). Kept with the marks they are of: marks
/// set from anywhere are seen by comparing them.
#[derive(Default)]
struct MarksAlong {
    /// The drawing's serial and the marks these are of (none: not made).
    made_for: Option<(u64, Vec<Mark>)>,
    along: Vec<[(f64, f64); 2]>,
}

#[cfg(test)]
thread_local! {
    /// How often this thread projected a set of marks.
    static MARKS_PROJECTED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl MarksAlong {
    /// The ends of `marks` along `centreline` (the drawing `serial`'s),
    /// in the marks' order.
    fn get(&mut self, serial: u64, centreline: &[[f64; 3]], marks: &[Mark]) -> &[[(f64, f64); 2]] {
        if self
            .made_for
            .as_ref()
            .is_none_or(|(s, m)| *s != serial || m != marks)
        {
            #[cfg(test)]
            MARKS_PROJECTED.with(|n| n.set(n.get() + 1));
            self.along = marks
                .iter()
                .map(|(at, to)| [at, to].map(|p| ohl_line::route::project(centreline, *p)))
                .collect();
            self.made_for = Some((serial, marks.to_vec()));
        }
        &self.along
    }
}

/// The lines on the ground along the profile: what they were found for
/// (the lines, the alignments' starts), each side's pieces of (station,
/// height).
type GroundAlong = (
    usize,
    Vec<(f64, String)>,
    Vec<(crate::ground_lines::Side, Vec<Vec<[f64; 2]>>)>,
);

/// Where the view is: stations, heights, its rectangle (bits of each).
type ViewKey = [u64; 8];

/// What the wires were drawn for: the view, the selection, the styles,
/// arrows or not.
type WiresKey = (ViewKey, Vec<Selection>, Option<u64>, bool);

/// What the drawing was made for: the route (none: all), the weather
/// case, the line width (the corridor's half width).
type Key = (Show, usize, u64);

/// What the profile shows.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Show {
    /// A route: its alignments one after the other, in their order.
    Route(String),
    /// One alignment alone.
    Alignment(String),
    /// Every alignment, one after the other, in their order.
    All,
}

/// What the profile shows first: the route with most structures (a main
/// run, not a short spur); `routes`: the line's alignments.
pub(crate) fn default_show(
    line: &ohl_line::Line,
    routes: &[ohl_line::route::AlignmentView],
) -> Show {
    let has = |r: &str| routes.iter().any(|a| a.route == r);
    let count = |r: &str| {
        let mut ids: Vec<&String> = routes
            .iter()
            .filter(|a| a.route == *r)
            .flat_map(|a| &a.structures)
            .collect();
        ids.sort();
        ids.dedup();
        ids.len()
    };
    line.routes
        .iter()
        .filter(|r| has(&r.id))
        .rev()
        .max_by_key(|r| count(&r.id))
        .map_or(Show::All, |r| Show::Route(r.id.clone()))
}

/// What a route's corridor ([`corridor`]) is made from: its centreline
/// (bits), the width (bits), the ground surface and the drawings (held,
/// compared as the same ones), the LiDAR points loaded (where, how many,
/// their revision) and the ground's revision.
struct CorridorKey {
    centre: Vec<u64>,
    depth: u64,
    ground: Option<std::sync::Arc<ohl_terrain::Terrain>>,
    points: (usize, usize, u64, u64),
    drawings: Vec<std::sync::Arc<ohl_background::Drawing>>,
}

impl PartialEq for CorridorKey {
    fn eq(&self, o: &Self) -> bool {
        let same =
            |a: &Option<std::sync::Arc<ohl_terrain::Terrain>>,
             b: &Option<std::sync::Arc<ohl_terrain::Terrain>>| match (a, b) {
                (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            };
        self.centre == o.centre
            && self.depth == o.depth
            && same(&self.ground, &o.ground)
            && self.points == o.points
            && self.drawings.len() == o.drawings.len()
            && self
                .drawings
                .iter()
                .zip(&o.drawings)
                .all(|(a, b)| std::sync::Arc::ptr_eq(a, b))
    }
}

/// What `corridor` would be made from now.
fn corridor_key(model: &Model, centre: &[[f64; 3]], depth: f64) -> CorridorKey {
    CorridorKey {
        centre: centre.iter().flatten().map(|v| v.to_bits()).collect(),
        depth: depth.to_bits(),
        ground: model.ground.clone(),
        points: (
            model.points.as_ptr() as usize,
            model.points.len(),
            model.revisions.points,
            model.revisions.ground,
        ),
        drawings: model
            .backgrounds
            .drawings
            .iter()
            .map(|d| d.drawing.clone())
            .collect(),
    }
}

/// The profile drawing and what is seen in the corridor around it.
#[derive(Clone)]
pub(crate) struct Built {
    pub(crate) drawing: RouteDrawing,
    /// Survey (ground surface) points in the corridor: station, z.
    pub(crate) survey: Vec<[f64; 2]>,
    /// 3D drawing lines in the corridor: drawing, colour, (station, z).
    pub(crate) lines: Vec<(String, [u8; 3], Vec<[f64; 2]>)>,
    /// 3D drawing points in the corridor: drawing, colour, (station, z).
    pub(crate) points: Vec<(String, [u8; 3], [f64; 2])>,
    /// LiDAR points shown (of the budget loaded) in the corridor:
    /// (station, z), class, the point (mm from the model's origin); by
    /// station.
    pub(crate) lidar: Vec<([f64; 2], u8, [i32; 3])>,
    /// All alignments: where each begins, and its name.
    pub(crate) joints: Vec<(f64, String)>,
}

/// Most survey points drawn (more are thinned evenly).
const MAX_SURVEY: usize = 200_000;

/// What lies within `depth` (the line width) of the centreline `centre`
/// (`length` long): the ground surface's points, the LiDAR points loaded,
/// the 3D lines and points of the drawings.
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
            circuits: Vec::new(),
            isoclearance: Vec::new(),
            markers: Vec::new(),
            alignment: Vec::new(),
            ..RouteDrawing::default()
        },
        survey: Vec::new(),
        lines: Vec::new(),
        points: Vec::new(),
        lidar: Vec::new(),
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
        let (st, off) = ohl_line::route::project(centre, p);
        let back = ohl_line::route::point_at(centre, st, off);
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
    // The LiDAR points loaded (thinned to the point budget), each class
    // as in the other views.
    let near_lidar: Vec<&crate::model::LidarPoint> = model
        .points
        .iter()
        .filter(|(q, _)| {
            let (x, y) = (
                model.origin[0] + f64::from(q[0]) / 1000.0,
                model.origin[1] + f64::from(q[1]) / 1000.0,
            );
            x >= lo[0] && x <= hi[0] && y >= lo[1] && y <= hi[1]
        })
        .collect();
    let stride = near_lidar.len().div_ceil(MAX_SURVEY).max(1);
    out.lidar = near_lidar
        .into_iter()
        .step_by(stride)
        .filter_map(|(q, c)| {
            let p = [0, 1, 2].map(|k| model.origin[k] + f64::from(q[k]) / 1000.0);
            Some((inside(p)?, *c, *q))
        })
        .collect();
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
            .chain(m.members.iter_mut().flatten())
            .chain(m.pole.iter_mut().flatten())
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
        .chain(b.lidar.iter_mut().map(|x| &mut x.0))
    {
        p[0] += ds;
    }
    for p in &mut b.drawing.alignment {
        p.station += ds;
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
    // Each by its drawing's name, else (an alignment without a name: one
    // made by New alignment) its id, from the name it joins with.
    let named = |d: &str, joint: &str| {
        if d.trim().is_empty() {
            joint.trim_start_matches("alignment ").to_owned()
        } else {
            d.to_owned()
        }
    };
    let mut names = vec![named(&out.drawing.name, &name)];
    out.joints.push((0.0, name));
    for (name, b) in iter {
        let at = out.drawing.length;
        let b = shifted(b, at);
        names.push(named(&b.drawing.name, &name));
        out.joints.push((at, name));
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
        d.alignment.extend(e.alignment);
        out.survey.extend(b.survey);
        out.lidar.extend(b.lidar);
        out.lines.extend(b.lines);
        out.points.extend(b.points);
    }
    out.drawing.name = format!("All alignments ({})", names.join(", "));
    Some(out)
}

impl ProfileView {
    /// The alignment and station of the last right-click, if on one.
    pub fn cut_at(&self) -> Option<(String, f64)> {
        self.cut_at.clone()
    }

    /// The LiDAR points dotted in the last frame: where, and the point
    /// (tests).
    #[cfg(test)]
    pub fn lidar_dots(&self) -> &[(egui::Pos2, crate::model::LidarPoint)] {
        &self.lidar_dots
    }

    /// The pieces of the lines on the ground drawn along the profile, by
    /// side: left, right (tests).
    #[cfg(test)]
    pub fn ground_profiles(&self) -> [usize; 2] {
        let mut n = [0; 2];
        for (side, pieces) in self.ground_along.iter().flat_map(|g| &g.2) {
            n[usize::from(*side == crate::ground_lines::Side::Right)] += pieces.len();
        }
        n
    }

    /// Draws what Checks › Vegetation found (nothing: the default).
    pub fn set_vegetation(&mut self, overlay: crate::vegetation::Overlay) {
        self.vegetation = overlay;
        self.vegetation_serial += 1;
    }

    /// Shows stations `range` (tests).
    #[cfg(test)]
    pub fn set_range(&mut self, range: [f64; 2]) {
        self.range = Some(range);
    }

    /// Forgets the drawing (after the model or case changed).
    pub fn invalidate(&mut self) {
        self.drawing = None;
        self.extent = None;
        self.back = None;
        self.wires = None;
    }

    /// Shows the view.
    #[allow(clippy::too_many_arguments)]
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        case: usize,
        selection: &mut Selection,
        snaps: &crate::selection::Snaps,
        drawings: &mut crate::drawings::Drawings,
        buttons: (egui::PointerButton, egui::PointerButton),
    ) -> Option<crate::selection::Use> {
        self.sheets_drawn.clear();
        let routes = model.alignment_views();
        if routes.is_empty() {
            ui.label(
                "No alignment yet: define the route, then draw its alignment (New alignment tool).",
            );
            return None;
        }
        let line = &model.line;
        let has = |r: &str| routes.iter().any(|a| a.route == r);
        let valid = |s: &Show| match s {
            Show::Route(r) => has(r),
            Show::Alignment(a) => routes.iter().any(|x| &x.id == a),
            Show::All => true,
        };
        if !self.show.as_ref().is_some_and(valid) {
            self.show = Some(default_show(line, &routes));
        }
        let show = self.show.clone().expect("chosen above");
        self.header_ui(ui, line, &routes, &show, drawings);
        // The line width: the drawings' (one setting).
        let depth = drawings.line_width();
        self.build(ui, model, case, &routes, &show, depth)?;
        let (sheets, curves) = sheet_preview(
            ui,
            drawings,
            model,
            case,
            &self.drawing.as_ref().expect("built").1,
        );
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        let width = f64::from(rect.width()).max(1.0);
        let height = f64::from(rect.height()).max(1.0);
        // A fixed ratio: pixels per metre of height, from the stations'.
        let ratio = model.profile_ratio.filter(|k| *k > 0.0);
        let ([s0, s1], area_done) = self.navigate(ui, &response, rect, width, ratio, buttons);
        let (z0, z1) = self.heights_shown(&[s0, s1], ratio, width, height);
        self.shown = Some(([s0, s1], [z0, z1], [width, height]));
        self.rect = Some(rect);
        let fr = Frame {
            rect,
            s0,
            s1,
            z0,
            z1,
            width,
        };
        let painter = ui.painter_at(rect);
        paint_grid(&painter, &fr);
        self.paint_vegetation(&painter, &fr, depth);
        self.paint_marks(&painter, &fr, depth);
        let view_key: ViewKey = [
            s0.to_bits(),
            s1.to_bits(),
            z0.to_bits(),
            z1.to_bits(),
            u64::from(rect.min.x.to_bits()),
            u64::from(rect.min.y.to_bits()),
            u64::from(rect.max.x.to_bits()),
            u64::from(rect.max.y.to_bits()),
        ];
        self.paint_corridor(&painter, &fr, model, view_key);
        self.paint_ground_lines(&painter, &fr, model);
        self.paint_joints(&painter, &fr);
        self.paint_structures(&painter, &fr, model, selection);
        self.paint_wires(&painter, &fr, model, case, selection, view_key);
        self.paint_sheets(&painter, &fr, sheets, curves);
        self.paint_title(&painter, &fr, ratio);
        self.hover_and_measure(ui, &response, &fr, snaps);
        self.area_ui(ui, &fr, area_done);
        self.clicks(ui, &response, &fr, snaps, selection, buttons)
    }

    /// The bar above the view: what the profile is of, the line width,
    /// the sheet preview's menu.
    fn header_ui(
        &mut self,
        ui: &mut egui::Ui,
        line: &ohl_line::Line,
        routes: &[ohl_line::route::AlignmentView],
        show: &Show,
        drawings: &mut crate::drawings::Drawings,
    ) {
        let has = |r: &str| routes.iter().any(|a| a.route == r);
        ui.horizontal(|ui| {
            ui.label("Profile of");
            let label = |s: &Show| match s {
                Show::Route(r) => format!("route {r}"),
                Show::Alignment(a) => format!("alignment {a}"),
                Show::All => "all alignments, in order".to_owned(),
            };
            egui::ComboBox::from_id_salt("profile-route")
                .selected_text(label(show))
                .show_ui(ui, |ui| {
                    let mut pick = |ui: &mut egui::Ui, s: Show, text: String| {
                        if ui.selectable_label(*show == s, text).clicked() {
                            self.show = Some(s);
                            self.range = None;
                        }
                    };
                    for r in line.routes.iter().filter(|r| has(&r.id)) {
                        let name = if r.name.is_empty() || r.name == r.id {
                            String::new()
                        } else {
                            format!(" — {}", r.name)
                        };
                        pick(
                            ui,
                            Show::Route(r.id.clone()),
                            format!("route {}{name}", r.id),
                        );
                        for a in routes.iter().filter(|a| a.route == r.id) {
                            pick(
                                ui,
                                Show::Alignment(a.id.clone()),
                                format!("    alignment {}", a.id),
                            );
                        }
                    }
                    if routes.len() > 1 {
                        pick(ui, Show::All, "all alignments, in order".into());
                    }
                })
                .response
                .on_hover_text(
                    "A route: its alignments one after the other, in their order; or one \
                     alignment alone",
                );
            ui.separator();
            ui.label("line width ±");
            let mut width = drawings.line_width();
            if ui
                .add(
                    egui::DragValue::new(&mut width)
                        .range(crate::drawings::LINE_WIDTHS)
                        .speed(0.5)
                        .max_decimals(1)
                        .suffix(" m"),
                )
                .on_hover_text(crate::drawings::LINE_WIDTH_HELP)
                .changed()
            {
                drawings.set_line_width(width);
            }
            ui.separator();
            // Ticks keep it open.
            crate::menu::options_menu(ui, "Profile sheets", |ui| drawings.preview_menu(ui));
        });
    }

    /// Makes the drawing of what is shown when it is not the one kept
    /// (none: an alignment has no profile, said in the view).
    fn build(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        case: usize,
        routes: &[ohl_line::route::AlignmentView],
        show: &Show,
        depth: f64,
    ) -> Option<()> {
        let key = (show.clone(), case, depth.to_bits());
        if self.drawing.as_ref().is_none_or(|(k, _)| *k != key) {
            let solved = model.cases.get(case);
            let shapes = model.shapes_shown(case);
            let options = ohl_drawing::Options {
                side_offsets: if depth > 0.0 {
                    vec![-depth, depth]
                } else {
                    Vec::new()
                },
                ..ohl_drawing::Options::default()
            };
            let wanted: Vec<usize> = (0..routes.len())
                .filter(|&k| match &show {
                    Show::Route(r) => &routes[k].route == r,
                    Show::Alignment(a) => &routes[k].id == a,
                    Show::All => true,
                })
                .collect();
            let mut parts = Vec::new();
            for k in wanted {
                let d = ohl_drawing::build(
                    &model.line,
                    &routes[k],
                    &shapes,
                    solved.map_or(&[][..], |s| &s.jumpers),
                    model.ground.as_deref(),
                    &[],
                    &options,
                );
                match d {
                    Ok(mut d) => {
                        crate::scene::set_drawn_members(&mut d, &model.line, &model.bodies);
                        let ck = corridor_key(model, &d.centreline, depth);
                        let route = &routes[k].id;
                        let kept = self
                            .corridors
                            .iter()
                            .find(|(r, key, _)| r == route && *key == ck);
                        let mut b = match kept {
                            Some((.., b)) => b.clone(),
                            None => {
                                let b = corridor(model, &d.centreline, d.length, depth);
                                // One per route.
                                self.corridors.retain(|(r, ..)| r != route);
                                self.corridors.push((route.clone(), ck, b.clone()));
                                b
                            }
                        };
                        // What the drawings show along the line: a
                        // connection beside it only within the width.
                        b.drawing = d.shown(depth);
                        parts.push((format!("alignment {}", routes[k].id), b));
                    }
                    Err(e) => {
                        ui.label(format!("profile of {}: {e}", routes[k].id));
                        return None;
                    }
                }
            }
            let mut built = joined(parts)?;
            // By station: those of the view found by bisection.
            built.survey.sort_by(|a, b| a[0].total_cmp(&b[0]));
            built.lidar.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]));
            self.drawing = Some((key, built));
            self.drawing_serial += 1;
            self.extent = None;
            self.back = None;
            self.wires = None;
        }
        Some(())
    }

    /// Moves the view as the pointer says (drag, scroll, double-click) and
    /// follows a box or lasso being drawn: the stations shown now, and
    /// the area when one was finished.
    fn navigate(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
        width: f64,
        ratio: Option<f64>,
        buttons: (egui::PointerButton, egui::PointerButton),
    ) -> ([f64; 2], Option<crate::area::Area>) {
        let d = &self.drawing.as_ref().expect("built").1.drawing;
        let range = self.range.get_or_insert([0.0, d.length.max(1.0)]);
        let vscale = |r: &[f64; 2]| ratio.map(|k| k * width / (r[1] - r[0]).max(1e-9));
        let before = *range;
        // Alt + drag of the selecting button: a box or lasso of the LiDAR
        // points dotted (the view does not move meanwhile).
        let (area_done, drawing) = crate::area::frame(&mut self.area, ui, response, buttons.0);
        if response.dragged() && !drawing {
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
        // A double-click of the select button ([`crate::controls::click_buttons`]).
        if response.double_clicked_by(buttons.0) {
            *range = [0.0, d.length.max(1.0)];
            self.z_centre = None;
        }
        (*range, area_done)
    }

    /// The heights shown at the stations `range`: those of what is
    /// visible, or at a fixed ratio those the view's height holds.
    fn heights_shown(
        &mut self,
        range: &[f64; 2],
        ratio: Option<f64>,
        width: f64,
        height: f64,
    ) -> (f64, f64) {
        let d = &self.drawing.as_ref().expect("built").1.drawing;
        let vscale = |r: &[f64; 2]| ratio.map(|k| k * width / (r[1] - r[0]).max(1e-9));
        let [s0, s1] = *range;
        // Heights of what is visible (found again when the stations
        // shown change).
        let stations = [s0.to_bits(), s1.to_bits()];
        let (z0, z1) = match self.extent {
            Some((k, z)) if k == stations => z,
            _ => {
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
                self.extent = Some((stations, (z0, z1)));
                (z0, z1)
            }
        };
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
        (z0, z1)
    }

    /// The vegetation check's grow-in points and danger trees.
    fn paint_vegetation(&mut self, painter: &egui::Painter, fr: &Frame, depth: f64) {
        let Frame { rect, s0, s1, .. } = *fr;
        let to_screen = fr.to_screen();
        let d = &self.drawing.as_ref().expect("built").1.drawing;
        // What Checks › Vegetation found within the line width: grow-in
        // points (cyan dots), danger trees (an orange trunk).
        let [grows, falls] = self.vegetation_layers;
        let stale = self.vegetation.stale;
        let color = |c: [u8; 3]| {
            crate::revision::dimmed32(egui::Color32::from_rgb(c[0], c[1], c[2]), stale)
        };
        // Each point's station and offset along this route, worked out
        // once per report and route; one mark a pixel.
        if self
            .vegetation_along
            .as_ref()
            .is_none_or(|v| v.0 != d.route || v.1 != self.vegetation_serial)
        {
            let along = |p: &[f64; 3]| ohl_line::route::project(&d.centreline, *p);
            self.vegetation_along = Some((
                d.route.clone(),
                self.vegetation_serial,
                self.vegetation.grow.iter().map(along).collect(),
                self.vegetation.fall.iter().map(|f| along(&f.0)).collect(),
            ));
        }
        let near =
            |(st, off): (f64, f64)| (off.abs() <= depth && st >= s0 && st <= s1).then_some(st);
        if let Some((_, _, grow_at, fall_at)) = &self.vegetation_along {
            if grows {
                let mut dotted = PixelSet::new(rect);
                let c = color(self.vegetation.colors[0]);
                for (p, at) in self.vegetation.grow.iter().zip(grow_at) {
                    if let Some(st) = near(*at) {
                        let q = to_screen(st, p[2]);
                        if dotted.insert(q) {
                            painter.circle_filled(q, 3.0, c);
                        }
                    }
                }
            }
            if falls {
                let mut dotted = PixelSet::new(rect);
                let c = color(self.vegetation.colors[1]);
                for ((top, base), at) in self.vegetation.fall.iter().zip(fall_at) {
                    if let Some(st) = near(*at) {
                        let q = to_screen(st, top[2]);
                        if dotted.insert(q) {
                            painter.line_segment(
                                [to_screen(st, base[2]), q],
                                egui::Stroke::new(2.0, c),
                            );
                        }
                    }
                }
            }
        }
    }

    /// The clearance marks.
    fn paint_marks(&mut self, painter: &egui::Painter, fr: &Frame, depth: f64) {
        let Frame { s0, s1, .. } = *fr;
        let to_screen = fr.to_screen();
        let d = &self.drawing.as_ref().expect("built").1.drawing;
        // The clearances below a requirement within the line width: each
        // end at its own station (a vertical one: one station, a vertical
        // stroke from the target up to the wire's height).
        if !self.marks.is_empty() {
            let c =
                crate::revision::dimmed32(egui::Color32::from_rgb(255, 60, 50), self.marks_stale);
            let target =
                crate::revision::dimmed32(egui::Color32::from_rgb(255, 160, 60), self.marks_stale);
            let along = self
                .marks_along
                .get(self.drawing_serial, &d.centreline, &self.marks);
            for ((at, to), &[(sa, oa), (st, ot)]) in self.marks.iter().zip(along) {
                let within = |s: f64, o: f64| o.abs() <= depth && s >= s0 && s <= s1;
                if !(within(sa, oa) || within(st, ot)) {
                    continue;
                }
                let (a, b) = (to_screen(sa, at[2]), to_screen(st, to[2]));
                painter.line_segment([a, b], egui::Stroke::new(2.0, c));
                painter.circle_filled(a, 3.5, c);
                painter.circle_filled(b, 2.5, target);
            }
        }
    }

    /// The corridor behind the line: survey and LiDAR dots, the ground at
    /// its edges and under the centreline, the 3D drawings in it.
    fn paint_corridor(
        &mut self,
        painter: &egui::Painter,
        fr: &Frame,
        model: &Model,
        view_key: ViewKey,
    ) {
        let Frame { rect, s0, s1, .. } = *fr;
        let to_screen = fr.to_screen();
        let built = &self.drawing.as_ref().expect("built").1;
        let d = &built.drawing;
        let visible = |p: &[f64; 2]| p[0] >= s0 - (s1 - s0) && p[0] <= s1 + (s1 - s0);
        // The corridor: survey points, the ground at its edges, the 3D
        // drawings in it (those not hidden), under the centreline ground;
        // made again when the view moves.
        let hidden = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            model.hidden_backgrounds.hash(&mut h);
            model.hidden_classes.hash(&mut h);
            h.finish()
        };
        if self
            .back
            .as_ref()
            .is_none_or(|(k, _)| *k != (view_key, hidden))
        {
            let mut out = Vec::new();
            // The dots as one mesh of small squares (as a circle that
            // small looks): tens of thousands of circle shapes cost their
            // tessellation every frame, a mesh next to nothing.
            let mut dots = egui::epaint::Mesh::default();
            let mut dot = |q: egui::Pos2, r: f32, c: egui::Color32| {
                dots.add_colored_rect(
                    egui::Rect::from_center_size(q, egui::vec2(2.0 * r, 2.0 * r)),
                    c,
                );
            };
            // One dot per pixel: points falling on a pixel already dotted
            // look the same drawn once (a far view of a long corridor has
            // tens of thousands).
            let survey = &built.survey;
            let from = survey.partition_point(|p| p[0] < s0);
            let to = survey.partition_point(|p| p[0] <= s1);
            let mut dotted = PixelSet::new(rect);
            for p in &survey[from..to.max(from)] {
                let q = to_screen(p[0], p[1]);
                if dotted.insert(q) {
                    dot(q, 1.2, egui::Color32::from_rgb(120, 150, 110));
                }
            }
            // The LiDAR points (classes hidden in Line › Terrain › LiDAR classes
            // shown left out), one per pixel and class colour.
            let lidar = &built.lidar;
            let from = lidar.partition_point(|p| p.0[0] < s0);
            let to = lidar.partition_point(|p| p.0[0] <= s1);
            let mut dotted = PixelSet::new(rect);
            self.lidar_dots.clear();
            for (p, c, at) in &lidar[from..to.max(from)] {
                if model.hidden_classes.contains(c) {
                    continue;
                }
                let Some(col) = crate::scene::class_color(&model.scheme, *c) else {
                    continue;
                };
                let q = to_screen(p[0], p[1]);
                if dotted.insert(q) {
                    dot(q, 1.0, egui::Color32::from_rgb(col[0], col[1], col[2]));
                    self.lidar_dots.push((q, (*at, *c)));
                }
            }
            out.push(egui::Shape::mesh(dots));
            for (_, prof) in &d.sides {
                for run in prof {
                    let pts = crate::plan::thin_px(
                        run.iter()
                            .filter(|p| visible(p))
                            .map(|p| to_screen(p[0], p[1])),
                    );
                    out.push(egui::Shape::line(
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
                let pts = crate::plan::thin_px(run.iter().map(|p| to_screen(p[0], p[1])));
                out.push(egui::Shape::line(
                    pts,
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(c[0], c[1], c[2])),
                ));
            }
            for (name, c, p) in &built.points {
                if shown_drawing(name) && p[0] >= s0 && p[0] <= s1 {
                    out.push(egui::Shape::circle_filled(
                        to_screen(p[0], p[1]),
                        2.0,
                        egui::Color32::from_rgb(c[0], c[1], c[2]),
                    ));
                }
            }
            for run in &d.ground {
                let pts = crate::plan::thin_px(
                    run.iter()
                        .filter(|p| visible(p))
                        .map(|p| to_screen(p[0], p[1])),
                );
                out.push(egui::Shape::line(
                    pts,
                    egui::Stroke::new(1.5, egui::Color32::from_rgb(170, 140, 100)),
                ));
            }
            self.back = Some(((view_key, hidden), out));
        }
        if let Some((_, shapes)) = &self.back {
            painter.extend(shapes.iter().cloned());
        }
    }

    /// The lines on the ground beside the line, along the profile.
    fn paint_ground_lines(&mut self, painter: &egui::Painter, fr: &Frame, model: &Model) {
        let Frame { s0, s1, .. } = *fr;
        let to_screen = fr.to_screen();
        let built = &self.drawing.as_ref().expect("built").1;
        let visible = |p: &[f64; 2]| p[0] >= s0 - (s1 - s0) && p[0] <= s1 + (s1 - s0);
        // The lines on the ground beside the line (View › Lines on the
        // ground beside the line, "in the profile too"): the ground along
        // each, by the station of its points; the right side (near)
        // solid, the left (far) dashed.
        match self
            .ground_lines
            .as_ref()
            .filter(|_| model.ground_lines.profile)
        {
            None => self.ground_along = None,
            Some(drawn) => {
                let id = std::sync::Arc::as_ptr(drawn) as usize;
                if self
                    .ground_along
                    .as_ref()
                    .is_none_or(|g| g.0 != id || g.1 != built.joints)
                {
                    let mut sides = Vec::new();
                    for (at, name) in &built.joints {
                        let alignment = name.strip_prefix("alignment ").unwrap_or(name);
                        sides.extend(crate::ground_lines::along(drawn, alignment, *at));
                    }
                    self.ground_along = Some((id, built.joints.clone(), sides));
                }
                let [r, g, b] = model.ground_lines.color;
                let stroke =
                    egui::Stroke::new(model.ground_lines.width, egui::Color32::from_rgb(r, g, b));
                for (side, pieces) in &self.ground_along.as_ref().expect("made above").2 {
                    for piece in pieces {
                        if piece.iter().all(|p| p[0] < s0 || p[0] > s1) {
                            continue;
                        }
                        let pts = crate::plan::thin_px(
                            piece
                                .iter()
                                .filter(|p| visible(p))
                                .map(|p| to_screen(p[0], p[1])),
                        );
                        if pts.len() < 2 {
                            continue;
                        }
                        if *side == crate::ground_lines::Side::Right {
                            painter.add(egui::Shape::line(pts, stroke));
                        } else {
                            painter.extend(egui::Shape::dashed_line(&pts, stroke, 6.0, 4.0));
                        }
                    }
                }
            }
        }
    }

    /// Where each alignment begins (several shown).
    fn paint_joints(&self, painter: &egui::Painter, fr: &Frame) {
        let Frame {
            rect, s0, s1, z0, ..
        } = *fr;
        let to_screen = fr.to_screen();
        let built = &self.drawing.as_ref().expect("built").1;
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
    }

    /// The structures: members, sticks or a mast line, a pole's
    /// silhouette, the name.
    fn paint_structures(
        &self,
        painter: &egui::Painter,
        fr: &Frame,
        model: &Model,
        selection: &Selection,
    ) {
        let Frame { rect, s0, s1, .. } = *fr;
        let to_screen = fr.to_screen();
        let d = &self.drawing.as_ref().expect("built").1.drawing;
        for m in &d.structures {
            // Only those within the stretch shown (with room for their
            // width and name).
            if m.station < s0 - 100.0 || m.station > s1 + 100.0 {
                continue;
            }
            let selected = crate::selection::all_selected(selection, model)
                .any(|x| matches!(x, Selection::Structure(id) if *id == m.id));
            // Away from the alignment (a connection beside the line, on the
            // route because it gives it): projected, fainter, its distance
            // written.
            let away = m.offset.abs() > ohl_line::NEAR_ALIGNMENT;
            let col = if selected {
                egui::Color32::WHITE
            } else if away {
                egui::Color32::from_gray(120)
            } else {
                egui::Color32::from_gray(190)
            };
            // Members (levels 2 and 3), else the stick drawing, else a
            // mast line.
            let lines = if m.members.is_empty() {
                &m.sticks
            } else {
                &m.members
            };
            // Under 80 px tall on the screen its bars blur into one shape:
            // the mast and an arm out to each point instead (display only;
            // a lattice has thousands of bars, each a shape every frame).
            let tall = (to_screen(m.station, m.base).y - to_screen(m.station, m.top).y).abs();
            if !lines.is_empty() && tall < 80.0 && lines.len() > 40 {
                let stroke = egui::Stroke::new(if selected { 2.0 } else { 1.0 }, col);
                painter.line_segment(
                    [to_screen(m.station, m.base), to_screen(m.station, m.top)],
                    stroke,
                );
                for p in &m.attachments {
                    painter
                        .line_segment([to_screen(m.station, p[2]), to_screen(p[0], p[2])], stroke);
                }
            } else if lines.is_empty() {
                painter.line_segment(
                    [to_screen(m.station, m.base), to_screen(m.station, m.top)],
                    egui::Stroke::new(if selected { 3.0 } else { 1.5 }, col),
                );
            } else {
                for st in lines {
                    let pts: Vec<egui::Pos2> = st.iter().map(|p| to_screen(p[0], p[2])).collect();
                    painter.add(egui::Shape::line(
                        pts,
                        egui::Stroke::new(if selected { 2.0 } else { 1.0 }, col),
                    ));
                }
            }
            // A pole (levels 2 and 3): its silhouette by height.
            if m.pole.len() >= 2 {
                let sides: Vec<(f64, f64, f64)> = m
                    .pole
                    .iter()
                    .filter(|r| !r.is_empty())
                    .map(|r| {
                        let lo = r.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
                        let hi = r.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
                        (lo, hi, r[0][2])
                    })
                    .collect();
                let pts: Vec<egui::Pos2> = sides
                    .iter()
                    .map(|s| to_screen(s.0, s.2))
                    .chain(sides.iter().rev().map(|s| to_screen(s.1, s.2)))
                    .collect();
                painter.add(egui::Shape::closed_line(
                    pts,
                    egui::Stroke::new(if selected { 2.0 } else { 1.0 }, col),
                ));
            }
            // Kept inside the view at its ends.
            let mut at = to_screen(m.station, m.top) + egui::vec2(0.0, -6.0);
            at.x = at.x.clamp(rect.min.x + 24.0, rect.max.x - 24.0);
            let name = if away {
                let side = if m.offset > 0.0 { "right" } else { "left" };
                format!("{} ({:.0} m {side})", m.id, m.offset.abs())
            } else {
                m.id.clone()
            };
            painter.text(
                at,
                egui::Align2::CENTER_BOTTOM,
                name,
                egui::FontId::proportional(11.0),
                col,
            );
        }
    }

    /// The wires, and the section labels and sags on them.
    fn paint_wires(
        &mut self,
        painter: &egui::Painter,
        fr: &Frame,
        model: &Model,
        case: usize,
        selection: &Selection,
        view_key: ViewKey,
    ) {
        let Frame { rect, s0, s1, .. } = *fr;
        let to_screen = fr.to_screen();
        let d = &self.drawing.as_ref().expect("built").1.drawing;
        // Coloured and coded as Line › Sections › Display options say (made
        // again when the view, the selection or the styles change).
        let display = &model.section_display;
        let arrows = display.direction[crate::annotations::View::Profile as usize];
        // (The styles made again first when the line changed: their key.)
        self.styles.get(&model.line, display.coloring);
        let wires_key: WiresKey = (
            view_key,
            crate::selection::all_selected(selection, model)
                .cloned()
                .collect(),
            self.styles.key(),
            arrows,
        );
        if self.wires.as_ref().is_none_or(|(k, _)| *k != wires_key) {
            let styles = self.styles.get(&model.line, display.coloring);
            let mut out = Vec::new();
            for w in &d.wires {
                // Out of the stations shown: not drawn.
                if w.points.iter().all(|p| p[0] < s0) || w.points.iter().all(|p| p[0] > s1) {
                    continue;
                }
                let traced = trace_selection(&w.label);
                let selected = traced
                    .as_ref()
                    .is_some_and(|s| crate::selection::is_selected(selection, model, s));
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
                let pts = crate::plan::thin_px(w.points.iter().map(|p| to_screen(p[0], p[2])));
                crate::plan::styled_wire_shapes(
                    &mut out,
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
            self.wires = Some((wires_key, out));
        }
        if let Some((_, shapes)) = &self.wires {
            painter.extend(shapes.iter().cloned());
        }
        // Section labels and sags (Line › Sections › Display options), on this
        // route's traces of the spans.
        paint_span_notes(painter, rect, model, case, &d.wires, to_screen);
    }

    /// The sheet preview: the lines the sheets carry, the match lines and
    /// the strip of sheets along the bottom.
    fn paint_sheets(
        &mut self,
        painter: &egui::Painter,
        fr: &Frame,
        sheets: Sheets<'_>,
        curves: Curves<'_>,
    ) {
        let Frame {
            rect, s0, s1, z0, ..
        } = *fr;
        let to_screen = fr.to_screen();
        let built = &self.drawing.as_ref().expect("built").1;
        // The sheet preview: match lines across the view at each sheet's
        // ends, the sheets in a strip along the bottom (warned ones amber,
        // the part beyond what the sheet holds darker), their numbers.
        for (at, c) in &curves {
            for run in &c.runs {
                let pts: Vec<egui::Pos2> = run.iter().map(|q| to_screen(at + q[0], q[1])).collect();
                painter.extend(egui::Shape::dashed_line(
                    &pts,
                    egui::Stroke::new(1.2, egui::Color32::from_rgb(200, 60, 60)),
                    8.0,
                    3.0,
                ));
            }
            if let Some(q) = c
                .runs
                .iter()
                .flatten()
                .nth(c.runs.iter().map(Vec::len).sum::<usize>() / 2)
            {
                painter.text(
                    to_screen(at + q[0], q[1]) + egui::vec2(0.0, -4.0),
                    egui::Align2::CENTER_BOTTOM,
                    &c.label,
                    egui::FontId::proportional(10.0),
                    egui::Color32::from_rgb(200, 60, 60),
                );
            }
        }
        let match_line = egui::Color32::from_rgb(70, 140, 230);
        let warn = egui::Color32::from_rgb(230, 160, 50);
        let strip = rect.max.y - 18.0..=rect.max.y;
        let several = built.joints.len() > 1;
        for (k, (at, id, s)) in sheets.iter().enumerate() {
            let (a, b) = (at + s.from, at + s.to);
            if b < s0 || a > s1 {
                continue;
            }
            let warned = !s.warnings.is_empty();
            self.sheets_drawn
                .push((id.to_string(), s.number, a, b, warned));
            let (xa, xb) = (to_screen(a, z0).x, to_screen(b, z0).x);
            let clip = |x: f32| x.clamp(rect.min.x, rect.max.x);
            let fill = if warned {
                egui::Color32::from_rgba_unmultiplied(230, 160, 50, 60)
            } else if k % 2 == 0 {
                egui::Color32::from_rgba_unmultiplied(70, 140, 230, 55)
            } else {
                egui::Color32::from_rgba_unmultiplied(70, 140, 230, 30)
            };
            painter.rect_filled(
                egui::Rect::from_x_y_ranges(clip(xa)..=clip(xb), strip.clone()),
                0.0,
                fill,
            );
            if s.to - s.from > s.reach + 1e-6 {
                let xr = to_screen(at + s.from + s.reach, z0).x;
                painter.rect_filled(
                    egui::Rect::from_x_y_ranges(clip(xr)..=clip(xb), strip.clone()),
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(230, 160, 50, 110),
                );
            }
            for x in [xa, xb] {
                if x >= rect.min.x && x <= rect.max.x {
                    painter.extend(egui::Shape::dashed_line(
                        &[egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                        egui::Stroke::new(1.0, match_line),
                        6.0,
                        4.0,
                    ));
                }
            }
            let label = format!(
                "{}Sheet {}{}",
                if several {
                    format!("{id} · ")
                } else {
                    String::new()
                },
                s.number,
                if warned { " ⚠" } else { "" }
            );
            let (l, r) = (clip(xa), clip(xb));
            if r - l > 24.0 {
                painter.text(
                    egui::pos2(0.5 * (l + r), rect.max.y - 9.0),
                    egui::Align2::CENTER_CENTER,
                    if r - l > 90.0 {
                        label
                    } else {
                        s.number.to_string()
                    },
                    egui::FontId::proportional(10.5),
                    if warned { warn } else { match_line },
                );
            }
        }
    }

    /// The line at the top: what is shown, from where to where, at which
    /// ratio.
    fn paint_title(&self, painter: &egui::Painter, fr: &Frame, ratio: Option<f64>) {
        let Frame { rect, s0, s1, .. } = *fr;
        let d = &self.drawing.as_ref().expect("built").1.drawing;
        painter.text(
            rect.left_top() + egui::vec2(8.0, 6.0),
            egui::Align2::LEFT_TOP,
            format!(
                "{} — stations {} to {}, {}",
                d.name,
                ohl_drawing::format_station(s0.max(0.0)),
                ohl_drawing::format_station(s1.min(d.length)),
                match ratio {
                    Some(k) => format!("aspect 1:{} (heights ×{})", ratio_text(k), ratio_text(k)),
                    None => "heights not to scale".into(),
                }
            ),
            egui::FontId::proportional(11.0),
            egui::Color32::from_gray(150),
        );
    }

    /// What is under the cursor while a measuring tool is held, and the
    /// measuring tools' lines and points.
    fn hover_and_measure(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        fr: &Frame,
        snaps: &crate::selection::Snaps,
    ) {
        let rect = fr.rect;
        let to_screen = fr.to_screen();
        let built = &self.drawing.as_ref().expect("built").1;
        let d = &built.drawing;
        // One alignment shown (not several one after the other): a point
        // of the profile's plane is a point on its axis.
        let single = built.joints.len() <= 1;
        let picker = Picker {
            fr: *fr,
            d,
            lidar_dots: &self.lidar_dots,
            snaps,
            single,
        };
        let use_at = |pos: egui::Pos2| picker.use_at(pos);
        if !self.measuring {
            self.hover_use = None;
        } else if let Some(pos) = response.hover_pos() {
            if ui.input(|i| i.pointer.is_moving()) || self.hover_use.is_none() {
                self.hover_use = Some(use_at(pos));
            }
        } else {
            self.hover_use = None;
        }
        // The measuring tools' lines and points, at their stations (one
        // alignment shown).
        if single && !self.measure.is_empty() {
            let painter = ui.painter_at(rect);
            self.measure.paint(
                &painter,
                &|p| {
                    let (s, _) = ohl_line::route::project(&d.centreline, p);
                    Some(to_screen(s, p[2]))
                },
                None,
            );
        }
    }

    /// A box or lasso: the LiDAR points it selected when finished, the
    /// one being drawn.
    fn area_ui(&mut self, ui: &egui::Ui, fr: &Frame, area_done: Option<crate::area::Area>) {
        let rect = fr.rect;
        if let Some(area) = area_done {
            let started = std::time::Instant::now();
            let picked: Vec<crate::model::LidarPoint> = self
                .lidar_dots
                .iter()
                .filter(|(q, _)| {
                    area.contains([f64::from(q.x - rect.min.x), f64::from(q.y - rect.min.y)])
                })
                .map(|(_, p)| *p)
                .collect();
            let n = picked.len();
            self.area_picked = Some((picked, n, started.elapsed()));
        }
        if let Some(a) = &self.area {
            a.paint(&ui.painter_at(rect));
        }
    }

    /// The clicks: select, the right-click's place and its menu, the
    /// tool's use (what it found).
    fn clicks(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        fr: &Frame,
        snaps: &crate::selection::Snaps,
        selection: &mut Selection,
        buttons: (egui::PointerButton, egui::PointerButton),
    ) -> Option<crate::selection::Use> {
        let Frame {
            rect,
            s0,
            s1,
            width,
            ..
        } = *fr;
        let to_screen = fr.to_screen();
        let built = &self.drawing.as_ref().expect("built").1;
        let d = &built.drawing;
        let single = built.joints.len() <= 1;
        let picker = Picker {
            fr: *fr,
            d,
            lidar_dots: &self.lidar_dots,
            snaps,
            single,
        };
        let pick = |pos: egui::Pos2| picker.pick(pos);
        let use_at = |pos: egui::Pos2| picker.use_at(pos);
        let (select_button, tool_button) = buttons;
        if response.clicked_by(select_button)
            && let Some(pos) = response.interact_pointer_pos()
        {
            let picked = pick(pos);
            crate::selection::clicked(ui.ctx(), &picked);
            *selection = picked;
        }
        // A right-click: where it was along which alignment, for the menu.
        if response.secondary_clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let s = s0 + f64::from(pos.x - rect.min.x) / width * (s1 - s0);
            self.cut_at = built
                .joints
                .iter()
                .rev()
                .find(|(at, _)| *at <= s)
                .or(built.joints.first())
                .map(|(at, name)| {
                    let id = name.strip_prefix("alignment ").unwrap_or(name);
                    (id.to_owned(), (s - at).max(0.0))
                });
        }
        if self.cut_menu
            && let Some((alignment, station)) = self.cut_at.clone()
        {
            let mut chosen = false;
            response.context_menu(|ui| {
                if ui
                    .button(format!(
                        "Cross-section here (station {})",
                        ohl_drawing::format_station(station)
                    ))
                    .on_hover_text(
                        "A cut across the line at this station, perpendicular to the \
                         alignment, in a tab of its own (C: the window)",
                    )
                    .clicked()
                {
                    chosen = true;
                    ui.close();
                }
            });
            if chosen {
                self.cut_request = Some((alignment, station));
            }
        }
        if response.clicked_by(tool_button)
            && let Some(pos) = response.interact_pointer_pos()
        {
            if single {
                self.annotation_hit = self.measure.hit(
                    &|p| {
                        let (s, _) = ohl_line::route::project(&d.centreline, p);
                        Some(to_screen(s, p[2]))
                    },
                    pos,
                    None,
                );
            }
            return Some(use_at(pos));
        }
        None
    }
}

/// Where a frame draws: the view's rectangle, the stations and heights
/// shown, the rectangle's width in points.
#[derive(Clone, Copy)]
struct Frame {
    rect: egui::Rect,
    s0: f64,
    s1: f64,
    z0: f64,
    z1: f64,
    width: f64,
}

impl Frame {
    /// Where a station and a height are on the screen.
    fn to_screen(self) -> impl Fn(f64, f64) -> egui::Pos2 + Copy + use<> {
        let Frame {
            rect,
            s0,
            s1,
            z0,
            z1,
            width,
        } = self;
        move |s: f64, z: f64| {
            egui::pos2(
                rect.min.x + ((s - s0) / (s1 - s0) * width) as f32,
                rect.max.y - ((z - z0) / (z1 - z0) * f64::from(rect.height())) as f32,
            )
        }
    }
}

/// The sheet preview's sheets: where each one's alignment begins on the
/// profile, the alignment, the sheet.
type Sheets<'a> = Vec<(f64, &'a str, &'a ohl_drawing::SheetLayout)>;

/// The lines below the lowest conductor the sheets carry: where each
/// one's alignment begins on the profile, the line.
type Curves<'a> = Vec<(f64, &'a ohl_drawing::curves::Curve)>;

/// The export's sheets of the alignments shown, each moved as its
/// alignment is on this profile, and the lines below the lowest conductor
/// they carry; what the preview says is written above the view.
fn sheet_preview<'a>(
    ui: &mut egui::Ui,
    drawings: &'a mut crate::drawings::Drawings,
    model: &Model,
    case: usize,
    built: &Built,
) -> (Sheets<'a>, Curves<'a>) {
    // The sheet preview: the export's sheets of the alignments shown,
    // each moved as its alignment is on this profile.
    let mut sheets: Vec<(f64, &str, &ohl_drawing::SheetLayout)> = Vec::new();
    // The lines below the lowest conductor the sheets carry.
    let mut curves: Vec<(f64, &ohl_drawing::curves::Curve)> = Vec::new();
    match drawings.preview(ui.ctx(), model, case) {
        None => {}
        Some(Err(e)) => {
            crate::menu::message(
                ui,
                format!("Sheet preview: {e}"),
                Some(ui.visuals().warn_fg_color),
            );
        }
        Some(Ok(p)) if p.paper.is_none() => {
            ui.label(
                egui::RichText::new(
                    "Sheet preview: the export writes one continuous drawing (no sheet \
                         format chosen in Plans & sheets › Profile sheets…)",
                )
                .small()
                .weak(),
            );
        }
        Some(Ok(p)) => {
            for (at, name) in &built.joints {
                let id = name.strip_prefix("alignment ").unwrap_or(name);
                if let Some(r) = p.route(id) {
                    sheets.extend(r.sheets.iter().map(|s| (*at, r.id.as_str(), s)));
                    curves.extend(r.curves.iter().map(|c| (*at, c)));
                }
            }
            // Those of the alignments shown.
            let warnings: Vec<String> = p
                .warnings()
                .into_iter()
                .filter(|w| {
                    sheets
                        .iter()
                        .any(|(_, id, _)| w.starts_with(&format!("route {id}: ")))
                })
                .collect();
            let page = p.paper.as_deref().unwrap_or("");
            ui.label(
                egui::RichText::new(format!(
                    "Sheet preview: {} sheet(s) {page}, {} warning(s) — as Plans & sheets › \
                         Profile sheets… writes them",
                    sheets.len(),
                    warnings.len()
                ))
                .small(),
            );
            if !warnings.is_empty() {
                crate::menu::message(ui, warnings.join("\n"), Some(ui.visuals().warn_fg_color));
            }
        }
    }
    (sheets, curves)
}

/// The background and the height grid.
fn paint_grid(painter: &egui::Painter, fr: &Frame) {
    let Frame {
        rect, s0, z0, z1, ..
    } = *fr;
    let to_screen = fr.to_screen();
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
}

/// What a click finds: where the frame draws, the drawing, the LiDAR
/// points dotted, the kinds picked, whether one alignment is shown.
struct Picker<'a> {
    fr: Frame,
    d: &'a RouteDrawing,
    lidar_dots: &'a [(egui::Pos2, crate::model::LidarPoint)],
    snaps: &'a crate::selection::Snaps,
    single: bool,
}

impl Picker<'_> {
    /// What is at `pos`.
    fn pick(&self, pos: egui::Pos2) -> Selection {
        let (d, snaps) = (self.d, self.snaps);
        let to_screen = self.fr.to_screen();
        // Picking: structures by station, wires by nearest point. Select
        // held, a left-click selects; another tool, a left-click uses it
        // and a right-click selects ([`crate::controls::click_buttons`]).
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
        // The LiDAR points dotted (every one drawn can be clicked; a
        // point under another's dot is drawn as that one).
        if let Some((d, p)) = self
            .lidar_dots
            .iter()
            .map(|(q, p)| (q.distance(pos), *p))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .filter(|(d, _)| *d <= 12.0)
        {
            // Over the wires at about the same distance (as in 3D).
            consider((d - 4.0).max(0.0), Selection::LidarPoint(p));
        }
        best.map_or(Selection::None, |b| b.1)
    }

    /// What a click of the tool button at `pos` finds.
    fn use_at(&self, pos: egui::Pos2) -> crate::selection::Use {
        // What a click of the tool button at `pos` finds: the object, on a
        // wire the point of it clicked, else the point of the profile's
        // plane there (on the axis, at the cursor's height).
        let Frame {
            rect,
            s0,
            s1,
            z0,
            z1,
            width,
        } = self.fr;
        let (d, single) = (self.d, self.single);
        let to_screen = self.fr.to_screen();
        let target = self.pick(pos);
        let s = s0 + f64::from(pos.x - rect.min.x) / width * (s1 - s0);
        let z = z0 + f64::from(rect.max.y - pos.y) / f64::from(rect.height()) * (z1 - z0);
        let at = single.then(|| {
            let xy = ohl_line::route::point_at(&d.centreline, s, 0.0);
            [xy[0], xy[1], z]
        });
        let mut on: Option<(f32, [f64; 3])> = None;
        if matches!(target, Selection::Span { .. } | Selection::Jumper(_)) {
            for w in d
                .wires
                .iter()
                .filter(|w| trace_selection(&w.label).as_ref() == Some(&target))
            {
                for k in 1..w.points.len().min(w.xy.len()) {
                    let (p, q) = (w.points[k - 1], w.points[k]);
                    let (a, b) = (to_screen(p[0], p[2]), to_screen(q[0], q[2]));
                    let ab = b - a;
                    let len2 = ab.length_sq();
                    let u = if len2 > 0.0 {
                        ((pos - a).dot(ab) / len2).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let e = (a + ab * u).distance(pos);
                    if on.is_none_or(|o| e < o.0) {
                        let u = f64::from(u);
                        let (xa, xb) = (w.xy[k - 1], w.xy[k]);
                        on = Some((
                            e,
                            [
                                xa[0] + u * (xb[0] - xa[0]),
                                xa[1] + u * (xb[1] - xa[1]),
                                p[2] + u * (q[2] - p[2]),
                            ],
                        ));
                    }
                }
            }
        }
        crate::selection::Use {
            target,
            at,
            on: on.map(|o| o.1),
        }
    }
}
/// The pixels of a view already drawn on (one bit each).
pub(crate) struct PixelSet {
    origin: egui::Pos2,
    width: usize,
    height: usize,
    bits: Vec<u64>,
}

impl PixelSet {
    /// None of `rect`'s pixels yet (a margin of a pixel around it).
    pub(crate) fn new(rect: egui::Rect) -> Self {
        let width = rect.width().max(0.0) as usize + 3;
        let height = rect.height().max(0.0) as usize + 3;
        Self {
            origin: rect.min - egui::vec2(1.0, 1.0),
            width,
            height,
            bits: vec![0; (width * height).div_ceil(64)],
        }
    }

    /// Marks the pixel of `p`; whether it was not marked yet (points off
    /// the view: false, nothing to draw).
    pub(crate) fn insert(&mut self, p: egui::Pos2) -> bool {
        let (x, y) = ((p.x - self.origin.x).round(), (p.y - self.origin.y).round());
        if !(x >= 0.0 && y >= 0.0) {
            return false;
        }
        let (x, y) = (x as usize, y as usize);
        if x >= self.width || y >= self.height {
            return false;
        }
        let k = y * self.width + x;
        let (word, bit) = (k / 64, 1u64 << (k % 64));
        let fresh = self.bits[word] & bit == 0;
        self.bits[word] |= bit;
        fresh
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
/// The typed choice is remembered as chosen (in the menu's memory) even
/// when its value is one of the list's, and keeps its last value.
pub fn ratio_menu(ui: &mut egui::Ui, ratio: &mut Option<f64>) -> bool {
    let before = *ratio;
    let id = egui::Id::new("profile-ratio-typed");
    // (chosen, its value).
    let (mut typed, mut value) = ui
        .data(|d| d.get_temp::<(bool, f64)>(id))
        .unwrap_or((false, 3.0));
    // A ratio not in the list is a typed one (set elsewhere, remembered).
    if let Some(k) = *ratio
        && !RATIOS.contains(&k)
    {
        typed = true;
        value = k;
    }
    if ratio.is_none() {
        typed = false;
    }
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
        typed = false;
    }
    for k in RATIOS {
        let text = if k == 1.0 {
            "1:1 (true scale)".to_owned()
        } else {
            format!("1:{} (heights ×{})", ratio_text(k), ratio_text(k))
        };
        if ui.radio(!typed && *ratio == Some(k), text).clicked() {
            *ratio = Some(k);
            typed = false;
        }
    }
    ui.horizontal(|ui| {
        if ui
            .radio(typed, "1:")
            .on_hover_text("A ratio of your own: type it or drag")
            .clicked()
        {
            typed = true;
            *ratio = Some(value);
        }
        let r = crate::style::field(
            ui,
            egui::DragValue::new(&mut value)
                .range(0.1..=500.0)
                .speed(0.1)
                .max_decimals(2),
        );
        // Typing or dragging the value chooses it.
        if r.changed() || (r.gained_focus() && !typed) {
            typed = true;
            *ratio = Some(value);
        }
        r.on_hover_text("Heights drawn this many times larger than stations");
    });
    ui.data_mut(|d| d.insert_temp(id, (typed, value)));
    *ratio != before
}

/// What a trace is, from its label: `S1 span 0 (circuit phase)` or
/// `jumper J1`.
/// Traces farther than this beyond the view's sides, px (the view's own
/// width when that is more), get no note made: a note is centred on its
/// trace, so one that far out neither shows nor takes the place of one
/// that shows.
const NOTE_MARGIN_PX: f32 = 2000.0;

/// The section labels and sags (Line › Sections › Display options) on
/// the route's traces of the spans, `wires`: a note at each trace's point
/// at mid-station, overlapping ones left out ([`span_notes`]).
fn paint_span_notes(
    painter: &egui::Painter,
    rect: egui::Rect,
    model: &Model,
    case: usize,
    wires: &[ohl_drawing::Trace],
    to_screen: impl Fn(f64, f64) -> egui::Pos2,
) {
    let (notes, written) = span_notes(rect, model, case, wires, to_screen);
    let mut placer = crate::annotations::Placer::default();
    for (at, k) in written {
        placer.paint(painter, at, &notes[k]);
    }
}

/// The notes of [`paint_span_notes`] and, in the order they are written,
/// where each is (a note of a span is written on each of its traces).
/// Only the notes of the traces written on are made, found by section
/// and span (owner, 2026-11-10: a note was made for every span of the
/// line on every frame, and all of them looked through for every trace).
fn span_notes(
    rect: egui::Rect,
    model: &Model,
    case: usize,
    wires: &[ohl_drawing::Trace],
    to_screen: impl Fn(f64, f64) -> egui::Pos2,
) -> (Vec<crate::annotations::Note>, Vec<(egui::Pos2, usize)>) {
    let view = crate::annotations::View::Profile;
    let display = &model.section_display;
    if !display.label[view as usize] && !display.sag[view as usize] {
        return Default::default();
    }
    let on = traces_noted(rect, wires, to_screen);
    if on.is_empty() {
        return Default::default();
    }
    let wanted: std::collections::HashSet<(&str, usize)> =
        on.iter().map(|(s, k, _)| (s.as_str(), *k)).collect();
    let notes = crate::annotations::notes_of(model, case, view, |shape, k| {
        wanted.contains(&(shape.section.as_str(), k))
    });
    // A span's notes in the order they were made (its label, its sag).
    let mut of_span: std::collections::HashMap<(&str, usize), Vec<usize>> =
        std::collections::HashMap::new();
    for (k, n) in notes.iter().enumerate() {
        of_span
            .entry((n.section.as_str(), n.span))
            .or_default()
            .push(k);
    }
    let written = on
        .iter()
        .flat_map(|(section, span, at)| {
            of_span
                .get(&(section.as_str(), *span))
                .into_iter()
                .flatten()
                .map(|k| (*at, *k))
        })
        .collect();
    (notes, written)
}

/// The traces of spans a note is written on, in their order: (section,
/// span, where on screen — the trace's point at mid-station). Not those
/// too short on screen, nor those far beyond the view's sides
/// ([`NOTE_MARGIN_PX`]).
fn traces_noted(
    rect: egui::Rect,
    wires: &[ohl_drawing::Trace],
    to_screen: impl Fn(f64, f64) -> egui::Pos2,
) -> Vec<(String, usize, egui::Pos2)> {
    let margin = rect.width().max(NOTE_MARGIN_PX);
    let (lo, hi) = (rect.min.x - margin, rect.max.x + margin);
    let mut on = Vec::new();
    for w in wires {
        let (Some(first), Some(last)) = (w.points.first(), w.points.last()) else {
            continue;
        };
        let (a, b) = (to_screen(first[0], first[2]), to_screen(last[0], last[2]));
        if a.distance(b) < crate::annotations::MIN_SPAN_PX || a.x.max(b.x) < lo || a.x.min(b.x) > hi
        {
            continue;
        }
        let Some(Selection::Span { section, span }) = trace_selection(&w.label) else {
            continue;
        };
        // The trace's point at mid-station.
        let mid = 0.5 * (first[0] + last[0]);
        let Some(m) = w
            .points
            .iter()
            .min_by(|p, q| (p[0] - mid).abs().total_cmp(&(q[0] - mid).abs()))
        else {
            continue;
        };
        on.push((section, span, to_screen(m[0], m[2])));
    }
    on
}

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

    /// The clearance marks' ends along the centreline are projected once
    /// per drawing and set of marks (2026-11-10), and are what projecting
    /// each end gives.
    #[test]
    fn marks_are_projected_once_per_drawing_and_marks() {
        use super::{MARKS_PROJECTED, MarksAlong};
        let centreline = [[0.0, 0.0, 0.0], [100.0, 0.0, 5.0], [100.0, 80.0, 9.0]];
        let other = [[0.0, 10.0, 0.0], [200.0, 10.0, 0.0]];
        let marks = vec![
            ([30.0, 4.0, 20.0], [30.0, 4.0, 2.0]),
            ([103.0, 40.0, 25.0], [110.0, 42.0, 1.0]),
        ];
        let direct = |c: &[[f64; 3]], m: &[super::Mark]| -> Vec<[(f64, f64); 2]> {
            m.iter()
                .map(|(at, to)| {
                    [
                        ohl_line::route::project(c, *at),
                        ohl_line::route::project(c, *to),
                    ]
                })
                .collect()
        };
        let projected = || MARKS_PROJECTED.with(std::cell::Cell::get);
        let mut along = MarksAlong::default();
        let before = projected();
        for _ in 0..5 {
            assert_eq!(
                along.get(1, &centreline, &marks),
                direct(&centreline, &marks)
            );
        }
        assert_eq!(projected(), before + 1);
        // The first mark: 30 m along, 4 m to the left.
        assert_eq!(along.get(1, &centreline, &marks)[0][0], (30.0, -4.0));
        // Another drawing (its centreline): again.
        assert_eq!(along.get(2, &other, &marks), direct(&other, &marks));
        assert_eq!(along.get(2, &other, &marks), direct(&other, &marks));
        assert_eq!(projected(), before + 2);
        // Other marks, set from anywhere: again; none: none.
        let fewer = marks[1..].to_vec();
        assert_eq!(along.get(2, &other, &fewer), direct(&other, &fewer));
        assert_eq!(along.get(2, &other, &fewer), direct(&other, &fewer));
        assert_eq!(projected(), before + 3);
        assert!(along.get(2, &other, &[]).is_empty());
    }

    /// The profile's section labels and sags as written until 2026-11-10:
    /// a note made for every span of the line, all of them looked through
    /// for every trace. Where each note is written, in order.
    fn span_notes_before(
        model: &crate::model::Model,
        case: usize,
        wires: &[ohl_drawing::Trace],
        to_screen: impl Fn(f64, f64) -> eframe::egui::Pos2,
    ) -> Vec<(eframe::egui::Pos2, crate::annotations::Note)> {
        let notes = crate::annotations::notes(model, case, crate::annotations::View::Profile, None);
        let mut out = Vec::new();
        for w in wires {
            let Some(Selection::Span { section, span }) = super::trace_selection(&w.label) else {
                continue;
            };
            let (Some(first), Some(last)) = (w.points.first(), w.points.last()) else {
                continue;
            };
            let (a, b) = (to_screen(first[0], first[2]), to_screen(last[0], last[2]));
            if a.distance(b) < crate::annotations::MIN_SPAN_PX {
                continue;
            }
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
                out.push((to_screen(m[0], m[2]), note.clone()));
            }
        }
        out
    }

    /// The notes written on the profile's traces are those written
    /// before (same notes, same places, same order) for every trace
    /// within the view and its margin; traces far beyond the view's
    /// sides get none any more (they neither showed nor took a place).
    #[test]
    fn span_notes_are_those_written_before() {
        use eframe::egui;
        let mut model = crate::tests::synthetic();
        let view = crate::annotations::View::Profile as usize;
        // Traces as the profile has them: station, offset, height (the
        // easting for the station), two of each span, a jumper, the
        // ground.
        let mut wires = Vec::new();
        for shape in &model.cases[0].shapes {
            for (k, span) in shape.spans.iter().enumerate() {
                for copy in ["", " (other case)"] {
                    wires.push(ohl_drawing::Trace {
                        label: format!("{} span {k}{copy}", shape.section),
                        points: span.points.iter().map(|p| [p[0], 0.0, p[2]]).collect(),
                        ..Default::default()
                    });
                }
            }
        }
        wires.push(ohl_drawing::Trace {
            label: "jumper J1".into(),
            points: vec![[0.0, 0.0, 10.0], [500.0, 0.0, 10.0]],
            ..Default::default()
        });
        wires.push(ohl_drawing::Trace::default());
        assert!(wires.len() > 10);
        let (lo, hi) = wires
            .iter()
            .flat_map(|w| &w.points)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p[0]), hi.max(p[0]))
            });
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(1200.0, 600.0));
        // The stations `from`..`to` across the view.
        let screen = |from: f64, to: f64| {
            move |s: f64, z: f64| {
                egui::pos2(
                    rect.min.x + ((s - from) / (to - from) * f64::from(rect.width())) as f32,
                    rect.max.y - (z * 2.0) as f32,
                )
            }
        };
        let now = |model: &crate::model::Model, to_screen: &dyn Fn(f64, f64) -> egui::Pos2| {
            let (notes, written) = super::span_notes(rect, model, 0, &wires, to_screen);
            written
                .into_iter()
                .map(|(at, k)| (at, notes[k].clone()))
                .collect::<Vec<_>>()
        };
        // Nothing ticked: nothing, as before.
        let whole = screen(lo, hi);
        assert!(now(&model, &whole).is_empty());
        assert!(span_notes_before(&model, 0, &wires, whole).is_empty());
        for (label, sag) in [(true, false), (false, true), (true, true)] {
            model.section_display.label[view] = label;
            model.section_display.sag[view] = sag;
            // The whole line in view, and a part of it enlarged within
            // the margin of the view's sides.
            let (third, span) = (lo + (hi - lo) / 3.0, (hi - lo) / 3.0);
            for to_screen in [screen(lo, hi), screen(third, third + span)] {
                let before = span_notes_before(&model, 0, &wires, to_screen);
                assert!(!before.is_empty());
                assert_eq!(now(&model, &to_screen), before, "label {label}, sag {sag}");
            }
        }
        // Enlarged a hundredfold on the line's start: the notes of the
        // traces within the margin of the view, as before; none of those
        // farther out.
        let near = screen(lo, lo + (hi - lo) / 100.0);
        let margin = rect.width().max(super::NOTE_MARGIN_PX);
        let all = span_notes_before(&model, 0, &wires, near);
        let kept = now(&model, &near);
        assert!(!kept.is_empty() && kept.iter().all(|k| all.contains(k)));
        // (Those left out: farther than the margin beyond the view.)
        assert!(kept.len() < all.len());
        for gone in all.iter().filter(|n| !kept.contains(n)) {
            assert!(gone.0.x > rect.max.x + margin, "{gone:?}");
        }
        // The order of those kept is the order before.
        let order: Vec<_> = all.iter().filter(|n| kept.contains(n)).cloned().collect();
        assert_eq!(kept, order);
    }
}
