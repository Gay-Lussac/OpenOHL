// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Detail view's tools (owner 2026-11-02: "the toolbar should also be
//! active on the joint and member view with the toolkit reduced to the
//! ones of the view (edit, add and remove bolts; if necessary move the
//! member to adapt where it lands on the joint)"), acting in the face
//! view and the member laid flat:
//!
//! - **Edit**: a bolt or member clicked: the member selected, its bolt
//!   layout form opened for that end;
//! - **Add bolt**: a click on a member's leg bolted: a bolt there (on its
//!   gauge line, along it to 5 mm);
//! - **Remove bolt**: a bolt clicked goes;
//! - **Member end**: a member dragged: where its steel stops (along) and
//!   its end's sideways shift (across, in the face view: an eccentricity
//!   the analysis takes) — the forces meet at the work point until it is
//!   shifted (owner 2026-11-02);
//! - **Plate around the bolts**: a plate without an outline gets the one
//!   it is drawn with, written into the file.
//!
//! A bolt added or taken away turns that end's layout into its bolts'
//! places (`bolt_points`); the bolt layout form makes a regular layout
//! again. Every action is one undo step.

use eframe::egui;
use ohl_structure_link::{
    Arrival, BoltRules, FlatLeg, JointDetail, MemberEnds, Snap, breaches, snap,
};

use crate::joint_tools::Act;

/// A member's drawn axis in the face view: its start and its direction
/// per metre along it.
type DrawnLine<'a> = dyn Fn(&Arrival) -> ([f64; 2], [f64; 2]) + 'a;

/// A tool of the Detail view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DetailTool {
    /// A bolt or member clicked: its bolt layout.
    #[default]
    Edit,
    /// A click on a member's leg: a bolt there.
    AddBolt,
    /// A bolt clicked: taken away.
    RemoveBolt,
    /// A bolt dragged: moved, snapped as Add bolt places it.
    MoveBolt,
    /// A member dragged: where its steel stops, its end's shift.
    MemberEnd,
}

impl DetailTool {
    const ALL: [DetailTool; 5] = [
        DetailTool::Edit,
        DetailTool::AddBolt,
        DetailTool::MoveBolt,
        DetailTool::RemoveBolt,
        DetailTool::MemberEnd,
    ];

    fn name(self) -> &'static str {
        match self {
            DetailTool::Edit => "Edit",
            DetailTool::AddBolt => "Add bolt",
            DetailTool::RemoveBolt => "Remove bolt",
            DetailTool::MoveBolt => "Move bolt",
            DetailTool::MemberEnd => "Member end",
        }
    }

    fn help(self) -> &'static str {
        match self {
            DetailTool::Edit => "Click a bolt or a member: its bolt layout, in the panel",
            DetailTool::AddBolt => {
                "Click on a member's leg bolted: a bolt there, snapped onto the nearest gauge line \
                 where the design code's end distance, spacing and edge distance hold; with no \
                 room there, a new gauge line is offered (you confirm). Bolts that break the code \
                 are marked red, the clause on hover"
            }
            DetailTool::MoveBolt => {
                "Drag a bolt: it moves, snapped as Add bolt places it (the code's distances kept; \
                 a new gauge line offered when there is no room)"
            }
            DetailTool::RemoveBolt => "Click a bolt: it goes",
            DetailTool::MemberEnd => {
                "Drag a member: along it, where its steel stops; across it (the face view), its \
                 end shifted sideways to fit — its force then passes beside the work point, an \
                 eccentricity the analysis takes"
            }
        }
    }
}

/// A member end being dragged with Member end.
#[derive(Debug, Clone, PartialEq)]
pub struct EndDrag {
    /// The member.
    pub member: String,
    /// The joint.
    pub joint: String,
    /// Where it was pressed and where the pointer is, in the view's
    /// metres (face view: the face's plane; member view: along, across).
    pub from: [f64; 2],
    /// Where the pointer is now.
    pub now: [f64; 2],
    /// The bolt dragged (Move bolt): its place in the end's bolts.
    pub bolt: Option<usize>,
}

/// The design code's bolt rules per member end (member, joint), as the
/// application worked them out for the ends drawn.
pub type Rules = std::collections::HashMap<(String, String), BoltRules>;

/// The act that places a bolt at `wanted` (along from the work point,
/// across from the heel; `across_known` false: the view does not show it)
/// on `a`'s end at `joint`, its other bolts `others` (a bolt moved left
/// out of them): by the code's rules when
/// there are some ([`snap`]: on a line, a new line offered, or why not),
/// else as before (on the nearest gauge line, along to 5 mm).
pub(crate) fn place(
    a: &Arrival,
    joint: &str,
    rules: Option<&BoltRules>,
    others: Vec<[f64; 2]>,
    wanted: [f64; 2],
    across_known: bool,
) -> Act {
    let points_with = |p: [f64; 2]| {
        let mut pts = others.clone();
        pts.push(p);
        pts
    };
    let bolts = |n: usize| Some(bolts_of(a, n as u32).value());
    let Some(r) = rules else {
        let across = if across_known {
            snap_across(a, wanted[1])
        } else {
            snap_across(a, 0.0)
        };
        let pts = points_with([round5mm(wanted[0].max(0.0)), across]);
        let n = pts.len();
        return Act::BoltPoints(a.member.clone(), joint.to_owned(), pts, bolts(n));
    };
    let mut lines: Vec<f64> = a.gauges.value.clone().unwrap_or_default();
    lines.extend(others.iter().map(|q| q[1]));
    let cut = a.cut.value.unwrap_or(0.0);
    let round = |p: [f64; 2]| [(p[0] * 1e4).round() / 1e4, (p[1] * 1e4).round() / 1e4];
    match snap(r, &others, &lines, cut, wanted, across_known) {
        Snap::OnLine(p) => {
            let pts = points_with(round(p));
            let n = pts.len();
            Act::BoltPoints(a.member.clone(), joint.to_owned(), pts, bolts(n))
        }
        Snap::NewLine { gauge, at } => {
            let pts = points_with(round(at));
            let n = pts.len();
            Act::OfferLine {
                member: a.member.clone(),
                joint: joint.to_owned(),
                points: pts,
                bolts: bolts(n),
                gauge,
                words: r.words(),
            }
        }
        Snap::Nowhere(why) => Act::Say(
            format!(
                "{} at {joint}: no place for the bolt there by {} — {why}",
                a.member, r.code
            ),
            true,
        ),
    }
}

/// Red rings on the bolts of `a` that break `r`, drawn at `at(i)`; the
/// breaches in words near the pointer `hover`.
fn mark_breaches(
    painter: &egui::Painter,
    a: &Arrival,
    r: &BoltRules,
    at: impl Fn(usize) -> Option<egui::Pos2>,
    radius: f32,
    hover: Option<egui::Pos2>,
) {
    let red = egui::Color32::from_rgb(220, 60, 50);
    let list = breaches(r, &a.bolt_flat, a.cut.value.unwrap_or(0.0));
    for b in &list {
        let Some(p) = at(b.bolt) else {
            continue;
        };
        painter.circle_stroke(p, radius + 3.0, egui::Stroke::new(2.0, red));
        if hover.is_some_and(|h| h.distance(p) < radius + 6.0) {
            let words: Vec<&str> = list
                .iter()
                .filter(|x| x.bolt == b.bolt)
                .map(|x| x.what.as_str())
                .collect();
            painter.text(
                p + egui::vec2(radius + 6.0, -radius - 6.0),
                egui::Align2::LEFT_BOTTOM,
                format!("{} bolt {}: {}", a.member, b.bolt + 1, words.join("; ")),
                egui::FontId::proportional(11.0),
                red,
            );
        }
    }
}

/// The tool strip; `active`: the tools act in this drawing (not the
/// close-up).
pub fn strip(ui: &mut egui::Ui, tool: &mut DetailTool, active: bool) {
    ui.horizontal_wrapped(|ui| {
        ui.add_enabled_ui(active, |ui| {
            for t in DetailTool::ALL {
                ui.selectable_value(tool, t, t.name())
                    .on_hover_text(t.help());
            }
        });
        let text = if active {
            tool.help()
        } else {
            "The tools act in the face view and the member view"
        };
        ui.label(egui::RichText::new(text).small().weak());
    });
}

/// The face view's fit into its rectangle: view metres to points.
#[derive(Debug, Clone, Copy)]
pub struct FaceFit {
    k: f64,
    c: [f64; 2],
    centre: egui::Pos2,
}

impl FaceFit {
    /// `bounds` (left, bottom, right, top, m) fitted into `rect`.
    pub fn new(rect: egui::Rect, bounds: [f64; 4]) -> Self {
        let [x0, y0, x1, y1] = bounds;
        let margin = 24.0;
        let k = ((f64::from(rect.width()) - 2.0 * margin) / (x1 - x0).max(1e-6))
            .min((f64::from(rect.height()) - 2.0 * margin) / (y1 - y0).max(1e-6));
        Self {
            k,
            c: [(x0 + x1) / 2.0, (y0 + y1) / 2.0],
            centre: rect.center(),
        }
    }

    /// Points per metre.
    pub fn scale(&self) -> f64 {
        self.k
    }

    /// A view point to the screen.
    pub fn at(&self, p: [f64; 2]) -> egui::Pos2 {
        self.centre
            + egui::vec2(
                ((p[0] - self.c[0]) * self.k) as f32,
                (-(p[1] - self.c[1]) * self.k) as f32,
            )
    }

    /// A screen point to the view.
    pub fn back(&self, q: egui::Pos2) -> [f64; 2] {
        [
            f64::from(q.x - self.centre.x) / self.k + self.c[0],
            -f64::from(q.y - self.centre.y) / self.k + self.c[1],
        ]
    }
}

fn sub2(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot2(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

/// The bolts' kind to write when an end gives none yet: what it shows
/// (the member's), else 16 mm assumed (said).
fn bolts_of(a: &Arrival, count: u32) -> connection_edits_value::Bolts {
    match &a.bolts.value {
        Some(b) => connection_edits_value::Bolts {
            count,
            bolt: b.bolt.clone(),
            diameter: b.bolt.is_none().then_some(b.diameter),
            hole: if b.bolt.is_none() { b.hole } else { None },
            grade: if b.bolt.is_none() {
                b.grade.clone()
            } else {
                None
            },
        },
        None => connection_edits_value::Bolts {
            count,
            bolt: None,
            diameter: Some(0.016),
            hole: None,
            grade: None,
        },
    }
}

/// The edit crate's bolts value, built here.
mod connection_edits_value {
    pub struct Bolts {
        pub count: u32,
        pub bolt: Option<String>,
        pub diameter: Option<f64>,
        pub hole: Option<f64>,
        pub grade: Option<String>,
    }

    impl Bolts {
        pub fn value(self) -> ohl_structure_edit::connection_edits::FieldValue {
            ohl_structure_edit::connection_edits::FieldValue::Bolts {
                count: self.count,
                bolt: self.bolt,
                diameter: self.diameter,
                hole: self.hole,
                grade: self.grade,
            }
        }
    }
}

/// The nearest gauge of `a` to `across` (its gauges, else its bolts'
/// places, else half its leg).
fn snap_across(a: &Arrival, across: f64) -> f64 {
    let mut lines: Vec<f64> = a.gauges.value.clone().unwrap_or_default();
    lines.extend(a.bolt_flat.iter().map(|q| q[1]));
    if lines.is_empty() {
        let w = a
            .profile
            .as_ref()
            .and_then(|p| p.angle)
            .map_or(0.05, |g| if a.leg_y { g.b } else { g.h });
        return w / 2.0;
    }
    lines
        .into_iter()
        .min_by(|x, y| (x - across).abs().total_cmp(&(y - across).abs()))
        .unwrap_or(across)
}

fn round5mm(x: f64) -> f64 {
    (x / 0.005).round() * 0.005
}

/// The face view's tool on `response` over the joint `j` seen from
/// `face` (`fit` as drawn): the act asked, and what to draw over it.
#[allow(clippy::too_many_arguments)]
pub fn face_act(
    j: &JointDetail,
    face: &str,
    fit: &FaceFit,
    response: &egui::Response,
    tool: DetailTool,
    drag: &mut Option<EndDrag>,
    painter: &egui::Painter,
    rules: &Rules,
) -> Vec<Act> {
    let [r, u, _] = ohl_structure_link::face_axes(face);
    let o = j.position;
    let to = |p: [f64; 3]| {
        let q = [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
        [
            q[0] * r[0] + q[1] * r[1] + q[2] * r[2],
            q[0] * u[0] + q[1] * u[1] + q[2] * u[2],
        ]
    };
    let reach = ohl_structure_link::reach(j);
    // A member's drawn axis in the view: its start and its direction per
    // metre along it.
    let line = |a: &Arrival| -> ([f64; 2], [f64; 2]) {
        let s = to([o[0] + a.offset[0], o[1] + a.offset[1], o[2] + a.offset[2]]);
        let e = to([
            o[0] + a.offset[0] + a.direction[0],
            o[1] + a.offset[1] + a.direction[1],
            o[2] + a.offset[2] + a.direction[2],
        ]);
        (s, sub2(e, s))
    };
    // Along a member (m from the joint) of a view point.
    let along = |a: &Arrival, v: [f64; 2]| {
        let (s, d) = line(a);
        dot2(sub2(v, s), d) / dot2(d, d).max(1e-12)
    };
    let tol = 8.0 / fit.scale();
    let hit_bolt = |v: [f64; 2]| -> Option<(usize, usize)> {
        let mut best: Option<(f64, usize, usize)> = None;
        for (ia, a) in j.arrivals.iter().enumerate() {
            let rad = a.bolts.value.as_ref().map_or(0.008, |b| b.diameter);
            for (ib, p) in a.bolt_points.iter().enumerate() {
                let d = sub2(to(*p), v);
                let d = dot2(d, d).sqrt();
                if d < rad.max(tol) && best.is_none_or(|b| d < b.0) {
                    best = Some((d, ia, ib));
                }
            }
        }
        best.map(|b| (b.1, b.2))
    };
    let hit_member = |v: [f64; 2]| -> Option<usize> {
        let mut best: Option<(f64, usize)> = None;
        for (ia, a) in j.arrivals.iter().enumerate() {
            let (s, d) = line(a);
            let t = along(a, v).clamp(a.cut.value.unwrap_or(0.0), a.length.min(reach));
            let p = [s[0] + d[0] * t, s[1] + d[1] * t];
            let dd = sub2(v, p);
            let dist = dot2(dd, dd).sqrt();
            let half = a
                .profile
                .as_ref()
                .and_then(|p| p.angle)
                .map_or(0.03, |g| g.b.max(g.h) / 2.0 + 0.01);
            if dist < half.max(tol) && best.is_none_or(|b| dist < b.0) {
                best = Some((dist, ia));
            }
        }
        best.map(|b| b.1)
    };
    let mut acts = Vec::new();
    let pointer = response.interact_pointer_pos().or(response.hover_pos());
    match tool {
        DetailTool::MemberEnd => {
            if response.drag_started()
                && let Some(q) = response.interact_pointer_pos()
            {
                let v = fit.back(q);
                if let Some(ia) = hit_member(v) {
                    let a = &j.arrivals[ia];
                    *drag = Some(EndDrag {
                        member: a.member.clone(),
                        joint: j.id.clone(),
                        from: v,
                        now: v,
                        bolt: None,
                    });
                }
            }
            if let (Some(d), Some(q)) = (drag.as_mut(), pointer) {
                d.now = fit.back(q);
                let a = j.arrivals.iter().find(|a| a.member == d.member);
                if let Some(a) = a {
                    let (s_along, s_across) = moved(a, d, &line, [r, u]);
                    let text = format!(
                        "{}: steel stops {:.0} mm from the work point, shift {:+.0} mm",
                        a.member,
                        s_along * 1000.0,
                        s_across * 1000.0
                    );
                    painter.line_segment(
                        [fit.at(d.from), fit.at(d.now)],
                        egui::Stroke::new(1.5, egui::Color32::from_rgb(90, 150, 230)),
                    );
                    painter.text(
                        fit.at(d.now) + egui::vec2(8.0, -8.0),
                        egui::Align2::LEFT_BOTTOM,
                        text,
                        egui::FontId::proportional(11.0),
                        egui::Color32::from_rgb(90, 150, 230),
                    );
                }
            }
            if response.drag_stopped()
                && let Some(d) = drag.take()
                && let Some(a) = j.arrivals.iter().find(|a| a.member == d.member)
            {
                let (cut, shift) = moved(a, &d, &line, [r, u]);
                acts.push(Act::CutShift(d.member.clone(), d.joint.clone(), cut, shift));
            }
        }
        DetailTool::MoveBolt => {
            if response.drag_started()
                && let Some(q) = response.interact_pointer_pos()
            {
                let v = fit.back(q);
                if let Some((ia, ib)) = hit_bolt(v) {
                    let a = &j.arrivals[ia];
                    *drag = Some(EndDrag {
                        member: a.member.clone(),
                        joint: j.id.clone(),
                        from: v,
                        now: v,
                        bolt: Some(ib),
                    });
                }
            }
            if let (Some(d), Some(q)) = (drag.as_mut(), pointer) {
                d.now = fit.back(q);
                painter.line_segment(
                    [fit.at(d.from), fit.at(d.now)],
                    egui::Stroke::new(1.5, egui::Color32::from_rgb(90, 150, 230)),
                );
            }
            if response.drag_stopped()
                && let Some(d) = drag.take()
                && let Some(ib) = d.bolt
                && let Some(a) = j.arrivals.iter().find(|a| a.member == d.member)
                && ib < a.bolt_flat.len()
            {
                // The face view shows the bolt's place along the member;
                // across the leg it keeps its line.
                let mut others = a.bolt_flat.clone();
                let old = others.remove(ib);
                let s = old[0] + along(a, d.now) - along(a, d.from);
                acts.push(place(
                    a,
                    &j.id,
                    rules.get(&(a.member.clone(), j.id.clone())),
                    others,
                    [s, old[1]],
                    true,
                ));
            }
        }
        _ => {
            if !response.clicked() {
                return acts;
            }
            let Some(q) = response.interact_pointer_pos() else {
                return acts;
            };
            let v = fit.back(q);
            match tool {
                DetailTool::Edit => {
                    let ia = hit_bolt(v).map(|x| x.0).or_else(|| hit_member(v));
                    if let Some(ia) = ia {
                        let a = &j.arrivals[ia];
                        acts.push(Act::Select(a.member.clone()));
                        acts.push(Act::OpenLayout(a.member.clone(), j.id.clone()));
                    }
                }
                DetailTool::AddBolt => {
                    if let Some(ia) = hit_member(v) {
                        let a = &j.arrivals[ia];
                        acts.push(place(
                            a,
                            &j.id,
                            rules.get(&(a.member.clone(), j.id.clone())),
                            a.bolt_flat.clone(),
                            [along(a, v).max(0.0), 0.0],
                            false,
                        ));
                    }
                }
                DetailTool::RemoveBolt => {
                    if let Some((ia, ib)) = hit_bolt(v) {
                        let a = &j.arrivals[ia];
                        let mut pts = a.bolt_flat.clone();
                        if ib < pts.len() {
                            pts.remove(ib);
                        }
                        let n = pts.len() as u32;
                        acts.push(Act::BoltPoints(
                            a.member.clone(),
                            j.id.clone(),
                            pts,
                            Some(bolts_of(a, n).value()),
                        ));
                    }
                }
                DetailTool::MemberEnd | DetailTool::MoveBolt => {}
            }
        }
    }
    // The bolts that break the code, marked.
    let hover = response.hover_pos();
    for a in &j.arrivals {
        if let Some(rl) = rules.get(&(a.member.clone(), j.id.clone())) {
            let rad =
                (a.bolts.value.as_ref().map_or(0.008, |b| b.diameter) / 2.0 * fit.scale()) as f32;
            mark_breaches(
                painter,
                a,
                rl,
                |i| a.bolt_points.get(i).map(|p| fit.at(to(*p))),
                rad.max(3.0),
                hover,
            );
        }
    }
    acts
}

/// Where a member end's steel stops and its shift, after a drag `d` in the
/// face view seen along `axes` (right, up): along it the cut moves by the
/// drag's part along the member; the shift by its part along the shift's
/// direction as the face shows it (none when that direction is square to
/// the face).
fn moved(a: &Arrival, d: &EndDrag, line: &DrawnLine<'_>, axes: [[f64; 3]; 2]) -> (f64, f64) {
    let (_, dir) = line(a);
    let delta = sub2(d.now, d.from);
    let ds = dot2(delta, dir) / dot2(dir, dir).max(1e-12);
    let cut = round5mm((a.cut.value.unwrap_or(0.0) + ds).max(0.0));
    let shift = match a.shift.1 {
        Some(u) => {
            let dot3 = |x: [f64; 3], y: [f64; 3]| x[0] * y[0] + x[1] * y[1] + x[2] * y[2];
            let u2 = [dot3(u, axes[0]), dot3(u, axes[1])];
            let l2 = dot2(u2, u2);
            if l2 > 0.04 {
                round5mm(a.shift.0 + dot2(delta, u2) / l2)
            } else {
                a.shift.0
            }
        }
        None => a.shift.0,
    };
    (cut, shift)
}

/// The member view's fit (the same as [`crate::member_panel::paint_member`]
/// draws with): along the member and across its leg to points.
#[derive(Debug, Clone, Copy)]
pub struct MemberFit {
    k: f64,
    x0: f64,
    y0: f64,
    r0: f64,
    r1: f64,
    broken: bool,
    length: f64,
}

impl MemberFit {
    /// `leg` fitted into `rect`.
    pub fn new(rect: egui::Rect, leg: &FlatLeg) -> Self {
        let reach = |k: usize| -> f64 {
            let far = leg
                .holes
                .iter()
                .map(|h| {
                    if k == 0 {
                        h.along
                    } else {
                        leg.length - h.along
                    }
                })
                .chain(std::iter::once(leg.cuts[k]))
                .filter(|d| *d < leg.length / 2.0)
                .fold(0.0, f64::max);
            (far + 0.1).max(0.25).min(leg.length / 2.0)
        };
        let (r0, r1) = (reach(0), reach(1));
        let broken = r0 + r1 < leg.length * 0.999;
        let shown = if broken { r0 + r1 + 0.08 } else { leg.length };
        let margin = 30.0;
        let k = ((f64::from(rect.width()) - 2.0 * margin) / shown)
            .min((f64::from(rect.height()) - 2.0 * margin - 30.0) / leg.width.max(1e-3));
        Self {
            k,
            x0: f64::from(rect.center().x) - shown * k / 2.0,
            y0: f64::from(rect.center().y) + leg.width * k / 2.0,
            r0,
            r1,
            broken,
            length: leg.length,
        }
    }

    /// Points per metre.
    pub fn scale(&self) -> f64 {
        self.k
    }

    /// The member is drawn broken in its middle.
    pub fn broken(&self) -> bool {
        self.broken
    }

    /// Along the member (from its first joint) to the drawing's x.
    pub fn x_of(&self, along: f64) -> f32 {
        let s = if !self.broken || along <= self.r0 {
            along
        } else if along >= self.length - self.r1 {
            along - (self.length - self.r1) + self.r0 + 0.08
        } else {
            self.r0 + 0.04
        };
        (self.x0 + s * self.k) as f32
    }

    /// Across the leg (from its heel) to the drawing's y.
    pub fn y_of(&self, across: f64) -> f32 {
        (self.y0 - across * self.k) as f32
    }

    /// A screen point to (along, across); none in the break.
    pub fn back(&self, q: egui::Pos2) -> Option<[f64; 2]> {
        let s = (f64::from(q.x) - self.x0) / self.k;
        let across = (self.y0 - f64::from(q.y)) / self.k;
        let along = if !self.broken || s <= self.r0 {
            s
        } else if s >= self.r0 + 0.08 {
            s - self.r0 - 0.08 + self.length - self.r1
        } else {
            return None;
        };
        Some([along.clamp(0.0, self.length), across])
    }
}

/// The member view's tool: the act asked.
#[allow(clippy::too_many_arguments)]
pub fn member_act(
    m: &MemberEnds,
    leg: &FlatLeg,
    fit: &MemberFit,
    response: &egui::Response,
    tool: DetailTool,
    drag: &mut Option<EndDrag>,
    painter: &egui::Painter,
    rules: &Rules,
) -> Vec<Act> {
    let mut acts = Vec::new();
    // The end a point along the member is nearer.
    let end_at = |along: f64| if along < m.length / 2.0 { 0 } else { 1 };
    let from_joint = |end: usize, along: f64| if end == 0 { along } else { m.length - along };
    let end_of = |end: usize| m.ends[end].as_ref();
    match tool {
        DetailTool::MemberEnd => {
            if response.drag_started()
                && let Some(v) = response.interact_pointer_pos().and_then(|q| fit.back(q))
                && let Some((j, a)) = end_of(end_at(v[0]))
            {
                *drag = Some(EndDrag {
                    member: a.member.clone(),
                    joint: j.clone(),
                    from: v,
                    now: v,
                    bolt: None,
                });
            }
            if let (Some(d), Some(v)) = (
                drag.as_mut(),
                response.interact_pointer_pos().and_then(|q| fit.back(q)),
            ) {
                d.now = v;
                let end = end_at(d.from[0]);
                let cut = round5mm(from_joint(end, v[0]).max(0.0));
                painter.text(
                    egui::pos2(fit.x_of(v[0]), fit.y_of(leg.width) - 6.0),
                    egui::Align2::CENTER_BOTTOM,
                    format!("steel stops {:.0} mm from {}", cut * 1000.0, d.joint),
                    egui::FontId::proportional(11.0),
                    egui::Color32::from_rgb(90, 150, 230),
                );
            }
            if response.drag_stopped()
                && let Some(d) = drag.take()
                && let Some((_, a)) = end_of(end_at(d.from[0]))
            {
                let end = end_at(d.from[0]);
                let cut = round5mm(from_joint(end, d.now[0]).max(0.0));
                acts.push(Act::CutShift(
                    d.member.clone(),
                    d.joint.clone(),
                    cut,
                    a.shift.0,
                ));
            }
        }
        DetailTool::MoveBolt => {
            // The bolt nearest a point (along the member, across), within
            // reach: its end and place.
            let near = |v: [f64; 2]| -> Option<(usize, usize)> {
                let end = end_at(v[0]);
                let (_, a) = end_of(end)?;
                let tol = (8.0 / fit.scale()).max(0.012);
                a.bolt_flat
                    .iter()
                    .enumerate()
                    .map(|(ib, q)| {
                        let along = if end == 0 { q[0] } else { m.length - q[0] };
                        (ib, (along - v[0]).hypot(q[1] - v[1]))
                    })
                    .filter(|x| x.1 < tol)
                    .min_by(|x, y| x.1.total_cmp(&y.1))
                    .map(|x| (end, x.0))
            };
            if response.drag_started()
                && let Some(v) = response.interact_pointer_pos().and_then(|q| fit.back(q))
                && let Some((end, ib)) = near(v)
                && let Some((j, a)) = end_of(end)
            {
                *drag = Some(EndDrag {
                    member: a.member.clone(),
                    joint: j.clone(),
                    from: v,
                    now: v,
                    bolt: Some(ib),
                });
            }
            if let (Some(d), Some(v)) = (
                drag.as_mut(),
                response.interact_pointer_pos().and_then(|q| fit.back(q)),
            ) {
                d.now = v;
                painter.line_segment(
                    [
                        egui::pos2(fit.x_of(d.from[0]), fit.y_of(d.from[1])),
                        egui::pos2(fit.x_of(v[0]), fit.y_of(v[1])),
                    ],
                    egui::Stroke::new(1.5, egui::Color32::from_rgb(90, 150, 230)),
                );
            }
            if response.drag_stopped()
                && let Some(d) = drag.take()
                && let Some(ib) = d.bolt
            {
                let end = end_at(d.from[0]);
                if let Some((j, a)) = end_of(end)
                    && ib < a.bolt_flat.len()
                {
                    let mut others = a.bolt_flat.clone();
                    others.remove(ib);
                    acts.push(place(
                        a,
                        j,
                        rules.get(&(a.member.clone(), j.clone())),
                        others,
                        [from_joint(end, d.now[0]), d.now[1]],
                        true,
                    ));
                }
            }
        }
        _ => {
            if !response.clicked() {
                return acts;
            }
            let Some(v) = response.interact_pointer_pos().and_then(|q| fit.back(q)) else {
                return acts;
            };
            let end = end_at(v[0]);
            let Some((j, a)) = end_of(end) else {
                return acts;
            };
            match tool {
                DetailTool::Edit => {
                    acts.push(Act::OpenLayout(a.member.clone(), j.clone()));
                }
                DetailTool::AddBolt => {
                    acts.push(place(
                        a,
                        j,
                        rules.get(&(a.member.clone(), j.clone())),
                        a.bolt_flat.clone(),
                        [from_joint(end, v[0]), v[1]],
                        true,
                    ));
                }
                DetailTool::RemoveBolt => {
                    let tol = 8.0 / fit.scale();
                    let near = a.bolt_flat.iter().enumerate().min_by(|x, y| {
                        let d = |q: &[f64; 2]| {
                            let along = if end == 0 { q[0] } else { m.length - q[0] };
                            (along - v[0]).hypot(q[1] - v[1])
                        };
                        d(x.1).total_cmp(&d(y.1))
                    });
                    if let Some((ib, q)) = near {
                        let along = if end == 0 { q[0] } else { m.length - q[0] };
                        if (along - v[0]).hypot(q[1] - v[1]) < tol.max(0.012) {
                            let mut pts = a.bolt_flat.clone();
                            pts.remove(ib);
                            let n = pts.len() as u32;
                            acts.push(Act::BoltPoints(
                                a.member.clone(),
                                j.clone(),
                                pts,
                                Some(bolts_of(a, n).value()),
                            ));
                        }
                    }
                }
                DetailTool::MemberEnd | DetailTool::MoveBolt => {}
            }
        }
    }
    // The bolts that break the code, marked.
    let hover = response.hover_pos();
    for (end, e) in m.ends.iter().enumerate() {
        let Some((j, a)) = e else {
            continue;
        };
        if let Some(rl) = rules.get(&(a.member.clone(), j.clone())) {
            let rad = (rl.diameter / 2.0 * fit.scale()) as f32;
            mark_breaches(
                painter,
                a,
                rl,
                |i| {
                    a.bolt_flat.get(i).map(|q| {
                        let along = if end == 0 { q[0] } else { m.length - q[0] };
                        egui::pos2(fit.x_of(along), fit.y_of(q[1]))
                    })
                },
                rad.max(3.0),
                hover,
            );
        }
    }
    acts
}
