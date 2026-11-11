// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Detail view's tools (owner 2026-11-02: "the toolbar should also be
//! active on the joint and member view with the toolkit reduced to the
//! ones of the view (edit, add and remove bolts; if necessary move the
//! member to adapt where it lands on the joint)"), acting in the face
//! view and the member laid flat. Since 2026-11-03 they sit in a toolbar
//! over the bottom of the drawing, as the 3D view's hotbar ([`hotbar`]),
//! the tool held named above it ([`strip`]):
//!
//! - **Edit**: a bolt clicked: its properties below, with the bolt
//!   libraries to change it ([`crate::detail_view`]); a plate: its
//!   properties; a member (face view): the member in detail, back to the
//!   joint from there; elsewhere on a member (member view): that end's
//!   bolt layout form;
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
    /// A member dragged sideways in the close-up: rolled about its axis.
    RotateMember,
}

impl DetailTool {
    /// Every tool, in the toolbar's order.
    pub const ALL: [DetailTool; 6] = [
        DetailTool::Edit,
        DetailTool::AddBolt,
        DetailTool::MoveBolt,
        DetailTool::RemoveBolt,
        DetailTool::MemberEnd,
        DetailTool::RotateMember,
    ];

    /// Its icon (of `assets/icons/tools/`).
    pub fn icon(self) -> crate::icons::Icon {
        use crate::icons::Icon;
        match self {
            DetailTool::Edit => Icon::Edit,
            DetailTool::AddBolt => Icon::AddBolt,
            DetailTool::RemoveBolt => Icon::RemoveBolt,
            DetailTool::MoveBolt => Icon::MoveBolt,
            DetailTool::MemberEnd => Icon::MemberEnd,
            DetailTool::RotateMember => Icon::RotateMember,
        }
    }

    /// What its slot shows.
    pub(crate) fn slot(self) -> crate::tools::SlotTool {
        (self.icon(), self.name(), self.help())
    }

    /// Its name (the slot's hover text, the strip).
    pub fn name(self) -> &'static str {
        match self {
            DetailTool::Edit => "Edit",
            DetailTool::AddBolt => "Add bolt",
            DetailTool::RemoveBolt => "Remove bolt",
            DetailTool::MoveBolt => "Move bolt",
            DetailTool::MemberEnd => "Member end",
            DetailTool::RotateMember => "Rotate member",
        }
    }

    /// What a click (or a drag) in the drawing does with it.
    pub fn help(self) -> &'static str {
        match self {
            DetailTool::Edit => {
                "Click a bolt: its properties below, with the bolt library to change it; a plate: \
                 its kind, thickness, outline and edges below (a plate with its outline in the \
                 file: its corners dragged in the face view, in the plate's plane); a member: the member in detail (‹ Joint or \
                 Escape: back to the joint)"
            }
            DetailTool::AddBolt => {
                "Click on a member's leg bolted: a bolt there, snapped onto the nearest gauge line \
                 where the design code's end distance, spacing and edge distance hold; with no \
                 room there, a new gauge line is offered (you confirm). Bolts that break the code \
                 are marked red, the clause on hover"
            }
            DetailTool::MoveBolt => {
                "Click a bolt: it is picked (ringed blue); drag the picked bolt: it moves, snapped \
                 as Add bolt places it (the code's distances kept; a new gauge line offered when \
                 there is no room). A drag started on another bolt picks it and moves nothing"
            }
            DetailTool::RemoveBolt => "Click a bolt: it goes",
            DetailTool::MemberEnd => {
                "Click a member: its end is picked (drawn blue); drag the picked member: along \
                 it, where its steel stops; across it (the face view), its end shifted sideways \
                 to fit — its force then passes beside the work point, an eccentricity the \
                 analysis takes. A drag started on another member picks it and moves nothing"
            }
            DetailTool::RotateMember => {
                "Click a member: it is picked (its axis drawn blue, its roll above the drawing); \
                 drag the picked member sideways: it rolls about its axis, half a degree a point \
                 in steps of the tool's step, drawn turned as you drag, set when let go (one \
                 undo step, the member's roll). A drag started on another member picks it and \
                 turns nothing; a drag on nothing turns the view"
            }
        }
    }

    /// Where it acts: Rotate member in the joint's close-up, the others in
    /// the face view and the member view.
    pub fn in_close_up(self) -> bool {
        self == DetailTool::RotateMember
    }
}

/// The steps the Detail view's drag tools snap to (owner 2026-11-07: "the
/// rotation should be 5 degrees by default and for the end 5 mm as today
/// but there should be a tool parameter somewhere in the app where it can
/// be changed"): set in the strip above the drawing while the tool is
/// held, kept between runs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolSteps {
    /// Member end: where the steel stops and the end's shift, m.
    pub end: f64,
    /// Rotate member: the roll, radians.
    pub roll: f64,
}

impl Default for ToolSteps {
    fn default() -> Self {
        Self {
            end: 0.005,
            roll: 5.0_f64.to_radians(),
        }
    }
}

/// Where the steps are kept: "end mm,roll degrees".
const STEPS_KEY: &str = "detail_tool_steps";

impl ToolSteps {
    /// The step's range, mm, for Member end.
    pub const END_MM: std::ops::RangeInclusive<f64> = 0.5..=100.0;
    /// The step's range, degrees, for Rotate member.
    pub const ROLL_DEG: std::ops::RangeInclusive<f64> = 0.5..=90.0;

    /// As kept by [`ToolSteps::save`] (none kept, or out of range: the
    /// default).
    pub fn load(storage: Option<&dyn eframe::Storage>) -> Self {
        let d = Self::default();
        let Some(text) = storage.and_then(|s| s.get_string(STEPS_KEY)) else {
            return d;
        };
        let mut it = text.split(',').map(|x| x.trim().parse::<f64>().ok());
        let end = it
            .next()
            .flatten()
            .filter(|x| Self::END_MM.contains(x))
            .map_or(d.end, |x| x / 1000.0);
        let roll = it
            .next()
            .flatten()
            .filter(|x| Self::ROLL_DEG.contains(x))
            .map_or(d.roll, f64::to_radians);
        Self { end, roll }
    }

    /// Kept in `storage`.
    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        storage.set_string(
            STEPS_KEY,
            format!("{},{}", self.end * 1000.0, self.roll.to_degrees()),
        );
    }
}

/// `x` to the nearest multiple of `step` (`step` 0 or less: `x`).
pub fn round_to(x: f64, step: f64) -> f64 {
    if step > 0.0 {
        (x / step).round() * step
    } else {
        x
    }
}

/// What Member end or Move bolt has picked (owner 2026-11-07: "with a
/// simple click highlight the member clicked and then click and drag edits
/// the end of the member; same for the tools that move bolts").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolPick {
    /// A member's end at a joint (Member end).
    End {
        /// The member.
        member: String,
        /// The joint.
        joint: String,
    },
    /// A bolt (Move bolt): its member end and its place among its bolts.
    Bolt {
        /// The member.
        member: String,
        /// The joint.
        joint: String,
        /// Its place among the end's bolts: the leg bolted's
        /// (`Arrival::bolt_flat`), then the other leg's (`other_flat`).
        index: usize,
    },
}

/// The act that puts `a`'s bolts at `joint` as `bolted` (on the leg
/// bolted) and `other` (on the other leg), with the leg it then bolts by
/// (owner, 2026-11-08: "moving single bolts to the other leg"): both
/// legs when each has bolts; one leg only: that leg (the other leg's
/// bolts then those of the leg bolted).
pub(crate) fn leg_bolts(
    a: &Arrival,
    joint: &str,
    mut bolted: Vec<[f64; 2]>,
    mut other: Vec<[f64; 2]>,
) -> Act {
    let name = |along_y: bool| -> Option<&'static str> {
        let g = a.profile.as_ref()?.angle?;
        let (mine, theirs) = if along_y { (g.b, g.h) } else { (g.h, g.b) };
        Some(if mine >= theirs { "long" } else { "short" })
    };
    let leg = if !bolted.is_empty() && !other.is_empty() {
        Some("both")
    } else if bolted.is_empty() && !other.is_empty() {
        std::mem::swap(&mut bolted, &mut other);
        name(!a.leg_y)
    } else {
        name(a.leg_y)
    };
    let n = (bolted.len() + other.len()) as u32;
    Act::LegBolts {
        member: a.member.clone(),
        joint: joint.to_owned(),
        bolted,
        other,
        leg,
        bolts: Some(bolts_of(a, n).value()),
    }
}

/// The act that moves bolt `index` of `a`'s end at `joint` (its leg
/// bolted's, then the other leg's: [`ToolPick::Bolt`]) to the other leg,
/// where it was along the member and across from the heel, kept within
/// that leg (5 mm from its edge).
pub(crate) fn to_other_leg(a: &Arrival, joint: &str, index: usize) -> Option<Act> {
    let g = a.profile.as_ref()?.angle?;
    let (mut bolted, mut other) = (a.bolt_flat.clone(), a.other_flat.clone());
    let n = bolted.len();
    let (from, to, width) = if index < n {
        (&mut bolted, &mut other, if a.leg_y { g.h } else { g.b })
    } else {
        (&mut other, &mut bolted, if a.leg_y { g.b } else { g.h })
    };
    let k = if index < n { index } else { index - n };
    if k >= from.len() {
        return None;
    }
    let q = from.remove(k);
    to.push([q[0], q[1].clamp(0.0, (width - 0.005).max(0.0))]);
    Some(leg_bolts(a, joint, bolted, other))
}

/// A press of Member end, Move bolt or Rotate member on `hit` (what is
/// under the pointer): a click picks it (nothing there: nothing picked); a
/// drag moves only what is picked — one started on something else picks
/// that and moves nothing, one started on nothing does nothing. Whether
/// the drag moves what it started on.
pub fn press<T: PartialEq>(
    pick: &mut Option<T>,
    hit: Option<T>,
    clicked: bool,
    drag_started: bool,
) -> bool {
    if clicked {
        *pick = hit;
        return false;
    }
    if drag_started && let Some(h) = hit {
        if pick.as_ref() == Some(&h) {
            return true;
        }
        *pick = Some(h);
    }
    false
}

/// Rotate member's turn for a sideways drag of `dx` points: half a degree
/// a point, in steps of `step` (radians).
pub fn roll_steps(dx: f32, step: f64) -> f64 {
    round_to((f64::from(dx) * 0.5).to_radians(), step)
}

/// The colour of what a tool has picked, and of its outline under the
/// pointer.
pub const PICKED: egui::Color32 = egui::Color32::from_rgb(90, 150, 230);

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

/// The strip above the drawing, as the 3D view's: the tool held and what
/// a click does with it; `active`: the tools act in this drawing (not the
/// close-up).
pub fn strip(ui: &mut egui::Ui, tool: DetailTool, active: bool, steps: &mut ToolSteps) {
    ui.horizontal_wrapped(|ui| {
        ui.add(
            tool.icon()
                .image(ui.ctx(), 16.0)
                .tint(ui.visuals().text_color()),
        );
        ui.strong(tool.name()).on_hover_text(tool.help());
        // The tool's step, where it has one (kept between runs).
        let tip = "The step the drag snaps to (kept between runs)";
        match tool {
            DetailTool::MemberEnd => {
                let mut mm = steps.end * 1000.0;
                let r = ui.add(
                    egui::DragValue::new(&mut mm)
                        .range(ToolSteps::END_MM)
                        .speed(0.5)
                        .max_decimals(1)
                        .prefix("step ")
                        .suffix(" mm"),
                );
                if r.on_hover_text(tip).changed() {
                    steps.end = mm / 1000.0;
                }
            }
            DetailTool::RotateMember => {
                let mut deg = steps.roll.to_degrees();
                let r = ui.add(
                    egui::DragValue::new(&mut deg)
                        .range(ToolSteps::ROLL_DEG)
                        .speed(0.5)
                        .max_decimals(1)
                        .prefix("step ")
                        .suffix("°"),
                );
                if r.on_hover_text(tip).changed() {
                    steps.roll = deg.to_radians();
                }
            }
            _ => {}
        }
        let text = if active {
            tool.help()
        } else if tool.in_close_up() {
            "Rotate member acts in the joint's close-up"
        } else {
            "The bolt and member end tools act in the face view and the member view; Rotate \
             member in the joint's close-up"
        };
        ui.label(egui::RichText::new(text).small().weak());
    });
}

/// The Detail view's toolbar over the bottom of its drawing `rect`, as the
/// 3D view's hotbar: a click on a slot holds its tool (Escape: Edit
/// again); a tool `active` says no to: shown, not usable there.
pub fn hotbar(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    tool: &mut DetailTool,
    active: &dyn Fn(DetailTool) -> bool,
) {
    let n = DetailTool::ALL.len() as f32;
    let bar = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, rect.bottom() - 30.0),
        egui::vec2(n * 43.0, 44.0),
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(bar)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            for t in DetailTool::ALL {
                ui.add_enabled_ui(active(t), |ui| {
                    let r = crate::tools::slot_frame(ui, Some(t.slot()), *tool == t, None)
                        .on_disabled_hover_text(if t.in_close_up() {
                            "Rotate member acts in the joint's close-up"
                        } else {
                            "The tool acts in the face view and the member view"
                        });
                    if r.clicked() {
                        *tool = t;
                    }
                });
            }
        },
    );
}

/// The face view's fit into its rectangle: view metres to points.
#[derive(Debug, Clone, Copy)]
pub struct FaceFit {
    k: f64,
    c: [f64; 2],
    centre: egui::Pos2,
}

/// The room the Detail view's tool strip takes at the bottom, points.
pub const STRIP_ROOM: f32 = 52.0;

impl FaceFit {
    /// `bounds` (left, bottom, right, top, m) fitted into `rect`.
    pub fn new(rect: egui::Rect, bounds: [f64; 4]) -> Self {
        let [x0, y0, x1, y1] = bounds;
        let margin = 24.0;
        // Clear of the tool strip at the bottom of the view: it covered
        // the lowest member's label (UI review 2026-11-11).
        let rect = egui::Rect::from_min_max(rect.min, rect.max - egui::vec2(0.0, STRIP_ROOM));
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

/// Whether `v` lies inside the polygon `pts` (even-odd).
fn inside2(pts: &[[f64; 2]], v: [f64; 2]) -> bool {
    let n = pts.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        if (a[1] > v[1]) != (b[1] > v[1]) {
            let x = a[0] + (v[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if v[0] < x {
                inside = !inside;
            }
        }
    }
    inside
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
    (drag, pick, steps): (&mut Option<EndDrag>, &mut Option<ToolPick>, ToolSteps),
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
            let end_of = |ia: usize| ToolPick::End {
                member: j.arrivals[ia].member.clone(),
                joint: j.id.clone(),
            };
            if (response.clicked() || response.drag_started())
                && let Some(q) = response.interact_pointer_pos()
            {
                let v = fit.back(q);
                let hit = hit_member(v);
                if press(
                    pick,
                    hit.map(end_of),
                    response.clicked(),
                    response.drag_started(),
                ) && let Some(ia) = hit
                {
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
            // The member picked, drawn blue; the one under the pointer, a
            // thin outline (what a click would pick).
            let draw = |ia: usize, picked: bool| {
                let a = &j.arrivals[ia];
                let (s0, d) = line(a);
                let t0 = a.cut.value.unwrap_or(0.0);
                let t1 = a.length.min(reach);
                let half = a
                    .profile
                    .as_ref()
                    .and_then(|p| p.angle)
                    .map_or(0.03, |g| g.b.max(g.h) / 2.0);
                let w = ((2.0 * half * fit.scale()) as f32).max(6.0);
                let ends = [
                    fit.at([s0[0] + d[0] * t0, s0[1] + d[1] * t0]),
                    fit.at([s0[0] + d[0] * t1, s0[1] + d[1] * t1]),
                ];
                let stroke = if picked {
                    egui::Stroke::new(w, PICKED.gamma_multiply(0.45))
                } else {
                    egui::Stroke::new(1.5, PICKED.gamma_multiply(0.8))
                };
                painter.line_segment(ends, stroke);
                if picked {
                    painter.text(
                        ends[1] + egui::vec2(6.0, -6.0),
                        egui::Align2::LEFT_BOTTOM,
                        format!("{} picked: drag it", a.member),
                        egui::FontId::proportional(11.0),
                        PICKED,
                    );
                }
            };
            if let Some(ToolPick::End { member, joint }) = pick.as_ref()
                && joint == &j.id
                && let Some(ia) = j.arrivals.iter().position(|a| &a.member == member)
            {
                draw(ia, true);
            }
            if drag.is_none()
                && let Some(q) = response.hover_pos()
                && let Some(ia) = hit_member(fit.back(q))
            {
                draw(ia, false);
            }
            if let (Some(d), Some(q)) = (drag.as_mut(), pointer) {
                d.now = fit.back(q);
                let a = j.arrivals.iter().find(|a| a.member == d.member);
                if let Some(a) = a {
                    let (s_along, s_across) = moved(a, d, &line, [r, u], steps.end);
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
                let (cut, shift) = moved(a, &d, &line, [r, u], steps.end);
                acts.push(Act::CutShift(d.member.clone(), d.joint.clone(), cut, shift));
            }
        }
        DetailTool::MoveBolt => {
            let bolt_of = |(ia, ib): (usize, usize)| ToolPick::Bolt {
                member: j.arrivals[ia].member.clone(),
                joint: j.id.clone(),
                index: ib,
            };
            if (response.clicked() || response.drag_started())
                && let Some(q) = response.interact_pointer_pos()
            {
                let v = fit.back(q);
                let hit = hit_bolt(v);
                if press(
                    pick,
                    hit.map(bolt_of),
                    response.clicked(),
                    response.drag_started(),
                ) && let Some((ia, ib)) = hit
                {
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
            // The bolt picked, ringed blue; the one under the pointer, a
            // thin ring.
            let ring = |ia: usize, ib: usize, picked: bool| {
                let a = &j.arrivals[ia];
                if let Some(p) = a.bolt_points.get(ib) {
                    let rad = a.bolts.value.as_ref().map_or(0.008, |b| b.diameter) / 2.0;
                    let r = ((rad * fit.scale()) as f32).max(3.0) + if picked { 5.0 } else { 3.0 };
                    let stroke = egui::Stroke::new(if picked { 2.5 } else { 1.2 }, PICKED);
                    painter.circle_stroke(fit.at(to(*p)), r, stroke);
                }
            };
            if let Some(ToolPick::Bolt {
                member,
                joint,
                index,
            }) = pick.as_ref()
                && joint == &j.id
                && let Some(ia) = j.arrivals.iter().position(|a| &a.member == member)
            {
                ring(ia, *index, true);
            }
            if drag.is_none()
                && let Some(q) = response.hover_pos()
                && let Some((ia, ib)) = hit_bolt(fit.back(q))
            {
                ring(ia, ib, false);
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
                    // A bolt: its properties below; a member: the member
                    // in detail (back to the joint from there); a plate:
                    // its properties below.
                    if let Some((ia, ib)) = hit_bolt(v) {
                        let a = &j.arrivals[ia];
                        acts.push(Act::PickBolt(a.member.clone(), j.id.clone(), ib));
                    } else if let Some(ia) = hit_member(v) {
                        acts.push(Act::Select(j.arrivals[ia].member.clone()));
                    } else if let Some(p) = j.plates.iter().find(|p| {
                        let pts: Vec<[f64; 2]> = p.outline.iter().map(|q| to(*q)).collect();
                        inside2(&pts, v)
                    }) {
                        acts.push(Act::PickPlate(p.id.clone()));
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
                DetailTool::MemberEnd | DetailTool::MoveBolt | DetailTool::RotateMember => {}
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
fn moved(
    a: &Arrival,
    d: &EndDrag,
    line: &DrawnLine<'_>,
    axes: [[f64; 3]; 2],
    step: f64,
) -> (f64, f64) {
    let (_, dir) = line(a);
    let delta = sub2(d.now, d.from);
    let ds = dot2(delta, dir) / dot2(dir, dir).max(1e-12);
    let cut = round_to((a.cut.value.unwrap_or(0.0) + ds).max(0.0), step);
    let shift = match a.shift.1 {
        Some(u) => {
            let dot3 = |x: [f64; 3], y: [f64; 3]| x[0] * y[0] + x[1] * y[1] + x[2] * y[2];
            let u2 = [dot3(u, axes[0]), dot3(u, axes[1])];
            let l2 = dot2(u2, u2);
            if l2 > 0.04 {
                round_to(a.shift.0 + dot2(delta, u2) / l2, step)
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
        // Both legs: the leg bolted above the heel, the other folded out
        // below it (2026-11-08).
        let tall = leg.width + leg.other_width;
        let k = ((f64::from(rect.width()) - 2.0 * margin) / shown)
            .min((f64::from(rect.height()) - 2.0 * margin - 30.0) / tall.max(1e-3));
        Self {
            k,
            x0: f64::from(rect.center().x) - shown * k / 2.0,
            y0: f64::from(rect.center().y) + (leg.width - leg.other_width) * k / 2.0,
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

    /// Across the leg (from its heel) to the drawing's y; below 0: the
    /// other leg folded out, as far from the heel.
    pub fn y_of(&self, across: f64) -> f32 {
        (self.y0 - across * self.k) as f32
    }

    /// A screen point to (along, across); none in the break. Across below
    /// 0: on the other leg, `-across` from the heel.
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
    (drag, pick, steps): (&mut Option<EndDrag>, &mut Option<ToolPick>, ToolSteps),
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
            // An end is picked by a click on its half of the member.
            let hit = |v: [f64; 2]| {
                end_of(end_at(v[0])).map(|(j, a)| ToolPick::End {
                    member: a.member.clone(),
                    joint: j.clone(),
                })
            };
            if (response.clicked() || response.drag_started())
                && let Some(v) = response.interact_pointer_pos().and_then(|q| fit.back(q))
                && press(pick, hit(v), response.clicked(), response.drag_started())
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
            // The end picked (its half of the member) drawn blue; under the
            // pointer, an outline.
            let half = |end: usize, picked: bool| {
                let (x0, x1) = if end == 0 {
                    (0.0, m.length / 2.0)
                } else {
                    (m.length / 2.0, m.length)
                };
                let r = egui::Rect::from_two_pos(
                    egui::pos2(fit.x_of(x0), fit.y_of(0.0)),
                    egui::pos2(fit.x_of(x1), fit.y_of(leg.width)),
                )
                .expand(4.0);
                if picked {
                    painter.rect_filled(r, 3.0, PICKED.gamma_multiply(0.18));
                    painter.rect_stroke(
                        r,
                        3.0,
                        egui::Stroke::new(2.0, PICKED),
                        egui::StrokeKind::Outside,
                    );
                } else {
                    painter.rect_stroke(
                        r,
                        3.0,
                        egui::Stroke::new(1.0, PICKED.gamma_multiply(0.7)),
                        egui::StrokeKind::Outside,
                    );
                }
            };
            for end in 0..2 {
                if let Some((j, a)) = end_of(end)
                    && pick.as_ref()
                        == Some(&ToolPick::End {
                            member: a.member.clone(),
                            joint: j.clone(),
                        })
                {
                    half(end, true);
                }
            }
            if drag.is_none()
                && let Some(v) = response.hover_pos().and_then(|q| fit.back(q))
            {
                half(end_at(v[0]), false);
            }
            if let (Some(d), Some(v)) = (
                drag.as_mut(),
                response.interact_pointer_pos().and_then(|q| fit.back(q)),
            ) {
                d.now = v;
                let end = end_at(d.from[0]);
                let cut = round_to(from_joint(end, v[0]).max(0.0), steps.end);
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
                let cut = round_to(from_joint(end, d.now[0]).max(0.0), steps.end);
                acts.push(Act::CutShift(
                    d.member.clone(),
                    d.joint.clone(),
                    cut,
                    a.shift.0,
                ));
            }
        }
        DetailTool::MoveBolt => {
            // The bolt nearest a point (along the member, across: below
            // the heel the other leg folded out), within reach: its end
            // and place (the leg bolted's, then the other leg's).
            let combined = |h: &ohl_structure_link::Hole| -> Option<(usize, usize)> {
                let (_, a) = end_of(h.end)?;
                Some((
                    h.end,
                    if h.other {
                        a.bolt_flat.len() + h.index
                    } else {
                        h.index
                    },
                ))
            };
            let drawn = |h: &ohl_structure_link::Hole| {
                [h.along, if h.other { -h.across } else { h.across }]
            };
            let near = |v: [f64; 2]| -> Option<(usize, usize)> {
                let tol = (8.0 / fit.scale()).max(0.012);
                leg.holes
                    .iter()
                    .map(|h| (h, (drawn(h)[0] - v[0]).hypot(drawn(h)[1] - v[1])))
                    .filter(|x| x.1 < tol)
                    .min_by(|x, y| x.1.total_cmp(&y.1))
                    .and_then(|x| combined(x.0))
            };
            let bolt_of = |(end, ib): (usize, usize)| {
                end_of(end).map(|(j, a)| ToolPick::Bolt {
                    member: a.member.clone(),
                    joint: j.clone(),
                    index: ib,
                })
            };
            if (response.clicked() || response.drag_started())
                && let Some(v) = response.interact_pointer_pos().and_then(|q| fit.back(q))
            {
                let hit = near(v);
                if press(
                    pick,
                    hit.and_then(bolt_of),
                    response.clicked(),
                    response.drag_started(),
                ) && let Some((end, ib)) = hit
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
            }
            // The bolt picked, ringed blue; under the pointer, a thin ring.
            let ring = |end: usize, ib: usize, picked: bool| {
                let Some(h) = leg.holes.iter().find(|h| combined(h) == Some((end, ib))) else {
                    return;
                };
                let p = drawn(h);
                let r =
                    ((h.bolt / 2.0 * fit.scale()) as f32).max(3.0) + if picked { 5.0 } else { 3.0 };
                painter.circle_stroke(
                    egui::pos2(fit.x_of(p[0]), fit.y_of(p[1])),
                    r,
                    egui::Stroke::new(if picked { 2.5 } else { 1.2 }, PICKED),
                );
            };
            for end in 0..2 {
                if let Some((j, a)) = end_of(end)
                    && let Some(ToolPick::Bolt {
                        member,
                        joint,
                        index,
                    }) = pick.as_ref()
                    && member == &a.member
                    && joint == j
                {
                    ring(end, *index, true);
                }
            }
            if drag.is_none()
                && let Some(v) = response.hover_pos().and_then(|q| fit.back(q))
                && let Some((end, ib)) = near(v)
            {
                ring(end, ib, false);
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
                if (v[1] < 0.0) != (d.from[1] < 0.0) {
                    painter.text(
                        egui::pos2(fit.x_of(v[0]), fit.y_of(v[1]) - 10.0),
                        egui::Align2::CENTER_BOTTOM,
                        "to the other leg",
                        egui::FontId::proportional(11.0),
                        PICKED,
                    );
                }
            }
            if response.drag_stopped()
                && let Some(d) = drag.take()
                && let Some(ib) = d.bolt
            {
                let end = end_at(d.from[0]);
                if let Some((j, a)) = end_of(end) {
                    let n = a.bolt_flat.len();
                    let on_other = d.now[1] < 0.0;
                    let at = [
                        round5mm(from_joint(end, d.now[0]).max(0.0)),
                        (d.now[1].abs() * 1000.0).round() / 1000.0,
                    ];
                    if ib < n && !on_other {
                        // On the leg bolted, as before: by the code's rules.
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
                    } else if ib < n + a.other_flat.len() {
                        // To or on the other leg (2026-11-08).
                        let (mut bolted, mut other) = (a.bolt_flat.clone(), a.other_flat.clone());
                        if ib < n {
                            bolted.remove(ib);
                        } else {
                            other.remove(ib - n);
                        }
                        if on_other {
                            other.push(at);
                        } else {
                            bolted.push(at);
                        }
                        acts.push(leg_bolts(a, j, bolted, other));
                    }
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
                    // A bolt: its properties below; elsewhere on the
                    // member: that end's bolt layout.
                    let tol = (8.0 / fit.scale()).max(0.012);
                    let near = a
                        .bolt_flat
                        .iter()
                        .enumerate()
                        .map(|(ib, q)| {
                            let along = if end == 0 { q[0] } else { m.length - q[0] };
                            (ib, (along - v[0]).hypot(q[1] - v[1]))
                        })
                        .filter(|x| x.1 < tol)
                        .min_by(|x, y| x.1.total_cmp(&y.1));
                    match near {
                        Some((ib, _)) => {
                            acts.push(Act::PickBolt(a.member.clone(), j.clone(), ib));
                        }
                        None => acts.push(Act::OpenLayout(a.member.clone(), j.clone())),
                    }
                }
                // Below the heel: onto the other leg (2026-11-08).
                DetailTool::AddBolt if v[1] < 0.0 => {
                    let mut other = a.other_flat.clone();
                    other.push([
                        round5mm(from_joint(end, v[0]).max(0.0)),
                        (-v[1] * 1000.0).round() / 1000.0,
                    ]);
                    acts.push(leg_bolts(a, j, a.bolt_flat.clone(), other));
                }
                DetailTool::RemoveBolt if v[1] < 0.0 && !a.other_flat.is_empty() => {
                    let tol = (8.0 / fit.scale()).max(0.012);
                    let d = |q: &[f64; 2]| {
                        let along = if end == 0 { q[0] } else { m.length - q[0] };
                        (along - v[0]).hypot(q[1] + v[1])
                    };
                    if let Some((ib, _)) = a
                        .other_flat
                        .iter()
                        .enumerate()
                        .filter(|x| d(x.1) < tol)
                        .min_by(|x, y| d(x.1).total_cmp(&d(y.1)))
                    {
                        let mut other = a.other_flat.clone();
                        other.remove(ib);
                        acts.push(leg_bolts(a, j, a.bolt_flat.clone(), other));
                    }
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
                DetailTool::MemberEnd | DetailTool::MoveBolt | DetailTool::RotateMember => {}
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

#[cfg(test)]
mod pick_tests {
    use super::{ToolPick, press, roll_steps};

    fn end(m: &str) -> Option<ToolPick> {
        Some(ToolPick::End {
            member: m.into(),
            joint: "N2".into(),
        })
    }

    /// Pick before drag (owner 2026-11-07): a click picks what is under the
    /// pointer (nothing: nothing picked); a drag moves only what is picked;
    /// one started on something else picks it and moves nothing; one
    /// started on nothing keeps the pick and moves nothing.
    #[test]
    fn a_click_picks_and_only_the_picked_is_dragged() {
        let mut pick = None;
        // A drag on a member not picked: picked, not moved.
        assert!(!press(&mut pick, end("D1"), false, true));
        assert_eq!(pick, end("D1"));
        // Dragged again: now it moves.
        assert!(press(&mut pick, end("D1"), false, true));
        // A drag on another: that one picked, nothing moved.
        assert!(!press(&mut pick, end("H1"), false, true));
        assert_eq!(pick, end("H1"));
        // A drag on nothing: the pick kept, nothing moved.
        assert!(!press(&mut pick, None, false, true));
        assert_eq!(pick, end("H1"));
        // A click picks; a click on nothing drops the pick.
        assert!(!press(&mut pick, end("D1"), true, false));
        assert_eq!(pick, end("D1"));
        assert!(!press(&mut pick, None, true, false));
        assert_eq!(pick, None);
        // A bolt is its own pick: the same member's bolts apart.
        let bolt = |i| ToolPick::Bolt {
            member: "D1".into(),
            joint: "N2".into(),
            index: i,
        };
        press(&mut pick, Some(bolt(0)), true, false);
        assert!(!press(&mut pick, Some(bolt(1)), false, true));
        assert!(press(&mut pick, Some(bolt(1)), false, true));
    }

    /// The tools' steps: 5 mm and 5° unless set; kept between runs, a
    /// value out of range read as the default.
    #[test]
    fn the_steps_are_kept_between_runs() {
        use super::ToolSteps;
        #[derive(Default)]
        struct Kept(std::collections::HashMap<String, String>);
        impl eframe::Storage for Kept {
            fn get_string(&self, key: &str) -> Option<String> {
                self.0.get(key).cloned()
            }
            fn set_string(&mut self, key: &str, value: String) {
                self.0.insert(key.into(), value);
            }
            fn remove_string(&mut self, key: &str) {
                self.0.remove(key);
            }
            fn flush(&mut self) {}
        }
        let d = ToolSteps::default();
        assert!((d.end - 0.005).abs() < 1e-12);
        assert!((d.roll.to_degrees() - 5.0).abs() < 1e-12);
        assert_eq!(ToolSteps::load(None), d);
        let mut kept = Kept::default();
        let set = ToolSteps {
            end: 0.01,
            roll: 15.0_f64.to_radians(),
        };
        set.save(&mut kept);
        let back = ToolSteps::load(Some(&kept));
        assert!((back.end - 0.01).abs() < 1e-12);
        assert!((back.roll.to_degrees() - 15.0).abs() < 1e-9);
        kept.0.insert("detail_tool_steps".into(), "0,400".into());
        assert_eq!(ToolSteps::load(Some(&kept)), d);
        assert!((super::round_to(0.0237, 0.01) - 0.02).abs() < 1e-12);
        assert!((roll_steps(60.0, 15.0_f64.to_radians()).to_degrees() - 30.0).abs() < 1e-9);
    }

    /// Rotate member turns half a degree a point, in 5° steps.
    #[test]
    fn a_sideways_drag_rolls_in_five_degree_steps() {
        let deg = |dx: f32| roll_steps(dx, 5.0_f64.to_radians()).to_degrees();
        assert_eq!(deg(0.0), 0.0);
        assert_eq!(deg(4.0), 0.0);
        assert!((deg(6.0) - 5.0).abs() < 1e-9);
        assert!((deg(-30.0) + 15.0).abs() < 1e-9);
        assert!((deg(180.0) - 90.0).abs() < 1e-9);
    }
}
