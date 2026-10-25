// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The measuring tools, phase 1 of the measuring tools and annotations
//! spec (docs/development/measuring-tools-spec.md, owner 2026-10-25):
//! **Distance** (two picks), **Height above ground** (one pick) and
//! **Stations** (two picks: along the route), with the station and offset
//! of the point under the cursor in the status bar while one is held.
//! The tools' main goal (owner): responsive, fast, intuitive — the value
//! follows the cursor before the click, nothing is solved again to
//! measure, and computed values are never coarsened (full precision in
//! copy and CSV, 0.01 m shown).
//!
//! A pick is what the tool button clicks ([`crate::selection::Use`]): a
//! point on a wire where it was clicked (in the weather case shown), a
//! structure's base, an attachment point, a LiDAR or survey point, an
//! alignment point, else the ground under the cursor (a free point). A
//! wire is measured to its **surface** (owner, 2026-10-25): the
//! subconductor nearest to the other end, less its radius.

use eframe::egui;

use crate::model::Model;
use crate::selection::{Selection, Use};

/// A measuring tool's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Between two points.
    Distance,
    /// A point's height above the ground under it.
    Height,
    /// Two points' stations and offsets along the route.
    Stations,
}

impl Kind {
    /// The picks it takes.
    pub fn picks(self) -> usize {
        match self {
            Kind::Height => 1,
            Kind::Distance | Kind::Stations => 2,
        }
    }

    /// Its name.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Distance => "Distance",
            Kind::Height => "Height above ground",
            Kind::Stations => "Stations",
        }
    }
}

/// A wire picked: what its surface needs.
#[derive(Debug, Clone, PartialEq)]
pub struct OnWire {
    /// Its subconductors' axes at the point picked (one for a single
    /// wire).
    pub axes: Vec<[f64; 3]>,
    /// The conductor's radius, m (0: its cable not in the library — the
    /// axis, said so).
    pub radius: f64,
}

/// A point picked.
#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    /// Where (project coordinates, m).
    pub at: [f64; 3],
    /// What it is, in words ("wire S4 span 2", "structure T12 base", …).
    pub what: String,
    /// On a wire: its subconductors and radius.
    pub wire: Option<OnWire>,
}

/// One line of a result: its name, value and unit (empty: a text).
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// What it is.
    pub name: String,
    /// The value, as computed.
    pub value: f64,
    /// Its unit ("m", "°"; "" for a count or a text in `name`).
    pub unit: &'static str,
}

/// A measurement made.
#[derive(Debug, Clone, PartialEq)]
pub struct Measurement {
    /// The tool.
    pub kind: Kind,
    /// The points picked.
    pub picks: Vec<Pick>,
    /// The weather case the wires were in.
    pub case: String,
    /// The main value (the first row) and the others.
    pub rows: Vec<Row>,
    /// Notes: how the ends were taken (a wire's surface, the ground
    /// surface missing…).
    pub notes: Vec<String>,
    /// The line drawn in the views (none: a single point's height —
    /// from the point down to the ground).
    pub line: Option<([f64; 3], [f64; 3])>,
}

impl Measurement {
    /// The value in the views and the panel's heading (0.01 m).
    pub fn label(&self) -> String {
        match self.rows.first() {
            Some(r) => format!("{} {}", fmt(r.value, r.unit), r.unit)
                .trim()
                .to_owned(),
            None => self.notes.first().cloned().unwrap_or_default(),
        }
    }

    /// As tab-separated text with every value in full: a heading line
    /// (tool, case, the picks) and one line per row.
    pub fn text(&self) -> String {
        let mut out = format!("{}\t{}", self.kind.name(), self.case);
        for p in &self.picks {
            out.push_str(&format!(
                "\t{} ({}, {}, {})",
                p.what, p.at[0], p.at[1], p.at[2]
            ));
        }
        out.push('\n');
        for r in &self.rows {
            out.push_str(&format!("{}\t{}\t{}\n", r.name, r.value, r.unit));
        }
        for n in &self.notes {
            out.push_str(&format!("{n}\n"));
        }
        out
    }
}

/// A value shown: 0.01 (m, °), the full value kept.
pub fn fmt(v: f64, unit: &str) -> String {
    if unit.is_empty() {
        format!("{v}")
    } else {
        format!("{v:.2}")
    }
}

/// The point clicked as a pick, in weather case `case` of `model`: on a
/// span or jumper the point of its wire clicked; an object its point
/// (a structure: its base); else the ground under the cursor.
pub fn resolve(model: &Model, case: usize, u: &Use) -> Option<Pick> {
    let wire = |section: &str, points: &[[f64; 3]], at: [f64; 3]| {
        let sec = model.line.sections.iter().find(|s| s.id == section)?;
        let mut axes = crate::scene::subconductors_at(sec, points, at);
        if axes.is_empty() {
            axes = vec![at];
        }
        let radius = model
            .library
            .get(&sec.cable)
            .map_or(0.0, |c| 0.5 * c.diameter.si());
        Some(OnWire { axes, radius })
    };
    match (&u.target, u.on) {
        (Selection::Span { section, span }, Some(on)) => {
            let points = &model.shape(section, case)?.spans.get(*span)?.points;
            Some(Pick {
                at: on,
                what: format!("wire {section} span {}", span + 1),
                wire: wire(section, points, on),
            })
        }
        (Selection::Jumper(id), Some(on)) => {
            let j = model.line.jumpers.iter().find(|j| &j.id == id)?;
            let section = model
                .line
                .sections
                .iter()
                .find(|s| match &j.from {
                    ohl_line::JumperEnd::Wire(p) => s.points.contains(p),
                    _ => false,
                })
                .map(|s| s.id.clone());
            let points = &model.jumper_shape(id, case)?.points;
            Some(Pick {
                at: on,
                what: format!("jumper {id}"),
                wire: section.and_then(|s| wire(&s, points, on)),
            })
        }
        (Selection::Structure(id), _) => {
            let s = model.line.structure(id)?;
            Some(Pick {
                at: s.position,
                what: format!("structure {id} (base)"),
                wire: None,
            })
        }
        (Selection::None, _) => u.at.map(|at| Pick {
            at,
            what: "ground".into(),
            wire: None,
        }),
        (t, _) => {
            let at = t.focus(model, case)?;
            let what = match t {
                Selection::Attachment { structure, set } => {
                    format!("wire point {structure} set {set} (as placed)")
                }
                Selection::LidarPoint((_, class)) => format!("LiDAR point (class {class})"),
                Selection::SurveyPoint(k) => format!("survey point {}", k + 1),
                Selection::AlignmentPoint { id } => format!("alignment point {id}"),
                other => other.describe(),
            };
            Some(Pick {
                at,
                what,
                wire: None,
            })
        }
    }
}

/// The point of `p` measured towards `to`: on a wire the surface of the
/// subconductor nearest to `to` (its axis when the radius is unknown).
fn surface_towards(p: &Pick, to: [f64; 3]) -> [f64; 3] {
    let Some(w) = &p.wire else {
        return p.at;
    };
    let d = |a: [f64; 3]| (0..3).map(|k| (a[k] - to[k]).powi(2)).sum::<f64>().sqrt();
    let axis = w
        .axes
        .iter()
        .copied()
        .min_by(|a, b| d(*a).total_cmp(&d(*b)))
        .unwrap_or(p.at);
    let l = d(axis);
    if l <= w.radius || l == 0.0 {
        return axis;
    }
    [0, 1, 2].map(|k| axis[k] + (to[k] - axis[k]) * w.radius / l)
}

/// The line's alignments: name and centreline (made once per line
/// version: the readout and the live value ask every frame).
#[derive(Debug, Clone, Default)]
pub struct Centrelines {
    /// The line version they are of.
    version: Option<u64>,
    /// Name and centreline of each alignment.
    lines: Vec<(String, Vec<[f64; 3]>)>,
}

impl Centrelines {
    /// The alignments of `model`'s line.
    pub fn of(model: &Model) -> Self {
        let lines = model
            .line
            .alignment_views()
            .into_iter()
            .map(|v| {
                let name = if v.name.is_empty() {
                    v.id.clone()
                } else {
                    v.name.clone()
                };
                (name, v.centreline(&model.line))
            })
            .filter(|(_, c)| c.len() >= 2)
            .collect();
        Self {
            version: Some(model.line_version),
            lines,
        }
    }

    /// Made again if the line changed.
    pub fn update(&mut self, model: &Model) {
        if self.version != Some(model.line_version) {
            *self = Self::of(model);
        }
    }

    /// The station and offset of `p` on the nearest alignment (by its
    /// offset): alignment name, station, offset (right of the direction
    /// of stationing positive), m.
    pub fn station_of(&self, p: [f64; 3]) -> Option<(String, f64, f64)> {
        let mut best: Option<(String, f64, f64)> = None;
        for (name, c) in &self.lines {
            let (s, o) = ohl_line::route::project(c, p);
            if best.as_ref().is_none_or(|b| o.abs() < b.2.abs()) {
                best = Some((name.clone(), s, o));
            }
        }
        best
    }
}

/// The measurement `kind` of `picks` (as many as it takes), wires in
/// weather case `case_name`; stations on `lines`.
pub fn measure(
    model: &Model,
    lines: &Centrelines,
    kind: Kind,
    picks: &[Pick],
    case_name: &str,
) -> Measurement {
    let mut rows = Vec::new();
    let mut notes = Vec::new();
    let mut line = None;
    let row = |name: &str, value: f64, unit: &'static str| Row {
        name: name.into(),
        value,
        unit,
    };
    match kind {
        Kind::Distance => {
            let (a, b) = (&picks[0], &picks[1]);
            // Each end towards the other (a wire: its surface).
            let p = surface_towards(a, b.at);
            let q = surface_towards(b, p);
            let p = surface_towards(a, q);
            let d = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
            let h = d[0].hypot(d[1]);
            rows.push(row("distance", h.hypot(d[2]), "m"));
            rows.push(row("horizontal", h, "m"));
            rows.push(row("vertical (second − first)", d[2], "m"));
            if let (Some(sa), Some(sb)) = (lines.station_of(p), lines.station_of(q))
                && sa.0 == sb.0
            {
                rows.push(row(&format!("along {}", sa.0), sb.1 - sa.1, "m"));
                rows.push(row("across (offset change)", sb.2 - sa.2, "m"));
            }
            if h > 0.0 {
                rows.push(row(
                    "slope from the horizontal",
                    d[2].atan2(h).to_degrees(),
                    "°",
                ));
                rows.push(row(
                    "bearing (from grid north, clockwise)",
                    d[0].atan2(d[1]).to_degrees().rem_euclid(360.0),
                    "°",
                ));
            }
            for (x, end) in [(a, "first"), (b, "second")] {
                if let Some(w) = &x.wire {
                    notes.push(if w.radius > 0.0 {
                        format!(
                            "{end}: the wire's surface (radius {:.1} mm)",
                            w.radius * 1e3
                        )
                    } else {
                        format!("{end}: the wire's axis (its cable is not in the library)")
                    });
                }
            }
            line = Some((p, q));
        }
        Kind::Height => {
            let a = &picks[0];
            // A wire: the bottom of its lowest subconductor.
            let (low, radius) = match &a.wire {
                Some(w) => (
                    w.axes
                        .iter()
                        .copied()
                        .min_by(|x, y| x[2].total_cmp(&y[2]))
                        .unwrap_or(a.at),
                    w.radius,
                ),
                None => (a.at, 0.0),
            };
            let bottom = [low[0], low[1], low[2] - radius];
            match model.ground_at(bottom[0], bottom[1]) {
                Some(g) => {
                    rows.push(row("height above ground", bottom[2] - g, "m"));
                    rows.push(row("point height", bottom[2], "m"));
                    rows.push(row("ground height", g, "m"));
                    line = Some((bottom, [bottom[0], bottom[1], g]));
                }
                None => notes.push("no ground surface under this point".into()),
            }
            if let Some(w) = &a.wire {
                notes.push(if w.radius > 0.0 {
                    format!(
                        "the bottom of the wire's lowest subconductor (radius {:.1} mm)",
                        w.radius * 1e3
                    )
                } else {
                    "the wire's axis (its cable is not in the library)".into()
                });
            }
            notes.push(
                "ground: the project's ground surface (the one the checks use), straight under \
                 the point"
                    .into(),
            );
        }
        Kind::Stations => {
            let ends: Vec<Option<(String, f64, f64)>> =
                picks.iter().map(|p| lines.station_of(p.at)).collect();
            for (k, e) in ends.iter().enumerate() {
                let which = if k == 0 { "first" } else { "second" };
                match e {
                    Some((name, s, o)) => {
                        rows.push(row(&format!("{which}: station on {name}"), *s, "m"));
                        rows.push(row(&format!("{which}: offset"), *o, "m"));
                    }
                    None => notes.push(format!("{which}: no alignment")),
                }
            }
            if let [Some(a), Some(b)] = &ends[..] {
                if a.0 == b.0 {
                    rows.insert(0, row(&format!("along {}", a.0), b.1 - a.1, "m"));
                } else {
                    notes.push(format!(
                        "on two alignments ({} and {}): no station difference",
                        a.0, b.0
                    ));
                }
            }
            let (p, q) = (picks[0].at, picks[1].at);
            rows.push(row(
                "straight, in plan",
                (q[0] - p[0]).hypot(q[1] - p[1]),
                "m",
            ));
            line = Some((p, q));
        }
    }
    Measurement {
        kind,
        picks: picks.to_vec(),
        case: case_name.to_owned(),
        rows,
        notes,
        line,
    }
}

/// The measuring tools' state: the picks so far, the point under the
/// cursor, the measurements made (the panel's).
#[derive(Debug, Default)]
pub struct Measuring {
    /// The tool the picks are for.
    pub kind: Option<Kind>,
    /// The picks so far.
    pub picks: Vec<Pick>,
    /// The point under the cursor (a measuring tool held).
    pub hover: Option<Pick>,
    /// The measurement to the point under the cursor, before the click.
    pub live: Option<Measurement>,
    /// The measurements made, newest last.
    pub done: Vec<Measurement>,
    /// The panel shown.
    pub open: bool,
    /// The line's alignments, for the stations.
    pub lines: Centrelines,
}

impl Measuring {
    /// A pick of tool `kind`: kept, and once there are enough the
    /// measurement made (the panel opened).
    pub fn click(&mut self, model: &Model, kind: Kind, case_name: &str, pick: Pick) {
        if self.kind != Some(kind) {
            self.picks.clear();
            self.kind = Some(kind);
        }
        self.picks.push(pick);
        if self.picks.len() >= kind.picks() {
            let picks = std::mem::take(&mut self.picks);
            self.lines.update(model);
            self.done
                .push(measure(model, &self.lines, kind, &picks, case_name));
            self.open = true;
        }
        self.live = None;
    }

    /// The point under the cursor with tool `kind` held: the live
    /// measurement from the picks so far (the last pick missing: this
    /// point).
    pub fn hover(&mut self, model: &Model, kind: Kind, case_name: &str, pick: Option<Pick>) {
        if self.kind != Some(kind) {
            self.picks.clear();
            self.kind = Some(kind);
        }
        self.lines.update(model);
        let lines = &self.lines;
        self.live = pick.as_ref().and_then(|p| {
            (self.picks.len() + 1 == kind.picks()).then(|| {
                let mut all = self.picks.clone();
                all.push(p.clone());
                measure(model, lines, kind, &all, case_name)
            })
        });
        self.hover = pick;
    }

    /// The picks given up (Escape, another tool).
    pub fn cancel(&mut self) {
        self.picks.clear();
        self.hover = None;
        self.live = None;
    }

    /// What the views draw on top: lines with their labels, and points
    /// (the picks so far and the point under the cursor).
    pub fn overlay(&self) -> Overlay {
        let mut lines = Vec::new();
        if let Some(m) = self.live.as_ref().or(self.done.last())
            && let Some((a, b)) = m.line
        {
            lines.push((a, b, m.label()));
        }
        let mut points: Vec<[f64; 3]> = self.picks.iter().map(|p| p.at).collect();
        if let Some(h) = &self.hover {
            points.push(h.at);
        }
        Overlay {
            lines,
            points,
            hover: self.hover.as_ref().map(|h| (h.at, h.what.clone())),
        }
    }

    /// The status bar's readout while a measuring tool is held: the
    /// point under the cursor (its station and offset, its height) and
    /// the live value.
    pub fn readout(&self) -> Option<String> {
        let h = self.hover.as_ref()?;
        let mut out = match self.lines.station_of(h.at) {
            Some((name, s, o)) => format!("{name}: station {s:.2} m, offset {o:.2} m"),
            None => "no alignment".to_owned(),
        };
        out.push_str(&format!(", height {:.2} m — {}", h.at[2], h.what));
        if let Some(m) = &self.live {
            out.push_str(&format!(" — {}: {}", m.kind.name(), m.label()));
        }
        Some(out)
    }

    /// All the measurements as tab-separated text (Copy all, Save CSV).
    pub fn text(&self) -> String {
        self.done
            .iter()
            .map(Measurement::text)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The Measurements panel: the measurements made, newest first, each
    /// with its rows; copy, CSV, delete, clear.
    pub fn window(&mut self, ctx: &egui::Context, tools_on_left: bool) {
        if !self.open {
            return;
        }
        let mut open = true;
        let mut delete = None;
        egui::Window::new("Measurements")
            .open(&mut open)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(crate::controls::click_words(
                        "Hold a measuring tool (Tools › Measure) and right-click the points; \
                         values shown to 0.01, copied in full.",
                        tools_on_left,
                    ))
                    .small()
                    .weak(),
                );
                ui.horizontal(|ui| {
                    if ui.button("Copy all").clicked() {
                        ui.ctx().copy_text(self.text());
                    }
                    if ui.button("Save CSV…").clicked() {
                        crate::job::save_csv("measurements.csv", &self.text().replace('\t', ","));
                    }
                    if ui.button("Clear").clicked() {
                        self.done.clear();
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(420.0)
                    .show(ui, |ui| {
                        if self.done.is_empty() {
                            ui.label(egui::RichText::new("No measurement yet.").weak());
                        }
                        for (k, m) in self.done.iter().enumerate().rev() {
                            ui.push_id(k, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(m.label()).strong().size(16.0));
                                    ui.label(m.kind.name());
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.small_button("Delete").clicked() {
                                                delete = Some(k);
                                            }
                                            if ui.small_button("Copy").clicked() {
                                                ui.ctx().copy_text(m.text());
                                            }
                                        },
                                    );
                                });
                                egui::CollapsingHeader::new("details")
                                    .default_open(k + 1 == self.done.len())
                                    .show(ui, |ui| {
                                        egui::Grid::new("rows").striped(true).show(ui, |ui| {
                                            for r in &m.rows {
                                                ui.label(&r.name);
                                                ui.label(
                                                    format!("{} {}", fmt(r.value, r.unit), r.unit)
                                                        .trim()
                                                        .to_owned(),
                                                );
                                                ui.end_row();
                                            }
                                        });
                                        for p in &m.picks {
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "{}: {:.2}, {:.2}, {:.2}",
                                                    p.what, p.at[0], p.at[1], p.at[2]
                                                ))
                                                .small(),
                                            );
                                        }
                                        ui.label(
                                            egui::RichText::new(format!("weather case {}", m.case))
                                                .small()
                                                .weak(),
                                        );
                                        for n in &m.notes {
                                            ui.label(egui::RichText::new(n).small().weak());
                                        }
                                    });
                                ui.separator();
                            });
                        }
                    });
            });
        if let Some(k) = delete {
            self.done.remove(k);
        }
        self.open = open;
    }
}

/// What the views draw for the measuring tools.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overlay {
    /// Measured lines and their labels.
    pub lines: Vec<([f64; 3], [f64; 3], String)>,
    /// The picks so far and the point under the cursor.
    pub points: Vec<[f64; 3]>,
    /// The point under the cursor and what it is.
    pub hover: Option<([f64; 3], String)>,
}

/// The overlay's colour (apart from the checks' yellow).
const COLOUR: egui::Color32 = egui::Color32::from_rgb(90, 230, 255);

impl Overlay {
    /// Whether there is nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.points.is_empty()
    }

    /// Draws it with `painter`, points to the screen by `screen`.
    pub fn paint(&self, painter: &egui::Painter, screen: &dyn Fn([f64; 3]) -> Option<egui::Pos2>) {
        for (a, b, label) in &self.lines {
            let (Some(p), Some(q)) = (screen(*a), screen(*b)) else {
                continue;
            };
            for (w, c) in [(4.5, egui::Color32::from_black_alpha(180)), (2.0, COLOUR)] {
                painter.line_segment([p, q], egui::Stroke::new(w, c));
            }
            let mid = p + (q - p) * 0.5;
            label_at(
                painter,
                mid + egui::vec2(8.0, -8.0),
                label,
                egui::Color32::WHITE,
            );
        }
        for x in self.points.iter().filter_map(|p| screen(*p)) {
            painter.circle_stroke(x, 5.0, egui::Stroke::new(2.0, COLOUR));
            painter.circle_filled(x, 2.0, COLOUR);
        }
        // What the cursor is on, named before the click.
        if let Some((at, what)) = &self.hover
            && let Some(x) = screen(*at)
        {
            label_at(painter, x + egui::vec2(10.0, 8.0), what, COLOUR);
        }
    }
}

fn label_at(painter: &egui::Painter, at: egui::Pos2, text: &str, colour: egui::Color32) {
    let galley = painter.layout_no_wrap(text.to_owned(), egui::FontId::proportional(13.0), colour);
    let at = egui::pos2(at.x, at.y - galley.size().y);
    painter.rect_filled(
        egui::Rect::from_min_size(at, galley.size()).expand(3.0),
        3.0,
        egui::Color32::from_black_alpha(170),
    );
    painter.galley(at, galley, colour);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(at: [f64; 3]) -> Pick {
        Pick {
            at,
            what: "p".into(),
            wire: None,
        }
    }

    /// Distance: the slant, horizontal and vertical parts of a 3-4-12
    /// box diagonal (13 m), the bearing of a pick due east (90°).
    #[test]
    fn distance_components() {
        let m = crate::tests::synthetic();
        let r = measure(
            &m,
            &Centrelines::of(&m),
            Kind::Distance,
            &[point([0.0, 0.0, 0.0]), point([3.0, 4.0, 12.0])],
            "c",
        );
        assert!((r.rows[0].value - 13.0).abs() < 1e-12);
        assert!((r.rows[1].value - 5.0).abs() < 1e-12);
        assert!((r.rows[2].value - 12.0).abs() < 1e-12);
        let east = measure(
            &m,
            &Centrelines::of(&m),
            Kind::Distance,
            &[point([0.0, 0.0, 0.0]), point([10.0, 0.0, 0.0])],
            "c",
        );
        let bearing = east
            .rows
            .iter()
            .find(|r| r.name.starts_with("bearing"))
            .unwrap();
        assert!((bearing.value - 90.0).abs() < 1e-9);
        assert_eq!(east.label(), "10.00 m");
    }

    /// A wire measured to its surface: a single conductor of radius r,
    /// 10 m above a point: 10 − r; two subconductors 0.4 m apart side by
    /// side: the nearer one.
    #[test]
    fn wires_to_their_surface() {
        let m = crate::tests::synthetic();
        let wire = |axes: Vec<[f64; 3]>| Pick {
            at: [0.0, 0.0, 10.0],
            what: "wire".into(),
            wire: Some(OnWire {
                axes,
                radius: 0.015,
            }),
        };
        let r = measure(
            &m,
            &Centrelines::of(&m),
            Kind::Distance,
            &[wire(vec![[0.0, 0.0, 10.0]]), point([0.0, 0.0, 0.0])],
            "c",
        );
        assert!(
            (r.rows[0].value - 9.985).abs() < 1e-12,
            "{}",
            r.rows[0].value
        );
        let r = measure(
            &m,
            &Centrelines::of(&m),
            Kind::Distance,
            &[
                wire(vec![[-0.2, 0.0, 10.0], [0.2, 0.0, 10.0]]),
                point([5.0, 0.0, 10.0]),
            ],
            "c",
        );
        assert!(
            (r.rows[0].value - (4.8 - 0.015)).abs() < 1e-12,
            "{}",
            r.rows[0].value
        );
    }

    /// Height above ground: the example line's ground under a structure
    /// is where its surface says; a point 20 m above it is 20 m up; the
    /// bottom of a wire's lowest subconductor counts.
    #[test]
    fn height_above_ground() {
        let m = crate::tests::synthetic();
        let s = &m.line.structures[0];
        let Some(g) = m.ground_at(s.position[0], s.position[1]) else {
            return;
        };
        let r = measure(
            &m,
            &Centrelines::of(&m),
            Kind::Height,
            &[point([s.position[0], s.position[1], g + 20.0])],
            "c",
        );
        assert!((r.rows[0].value - 20.0).abs() < 1e-9);
        let w = Pick {
            at: [s.position[0], s.position[1], g + 20.0],
            what: "wire".into(),
            wire: Some(OnWire {
                axes: vec![
                    [s.position[0], s.position[1], g + 20.2],
                    [s.position[0], s.position[1], g + 19.8],
                ],
                radius: 0.01,
            }),
        };
        let r = measure(&m, &Centrelines::of(&m), Kind::Height, &[w], "c");
        assert!(
            (r.rows[0].value - 19.79).abs() < 1e-9,
            "{}",
            r.rows[0].value
        );
    }

    /// Stations: two structures of the example line on its alignment: the
    /// difference along it is their stations' difference, and the panel
    /// state takes two picks before measuring.
    #[test]
    fn stations_along_the_route() {
        let m = crate::tests::synthetic();
        let (a, b) = (&m.line.structures[0], &m.line.structures[1]);
        let lines = Centrelines::of(&m);
        let (sa, sb) = (
            lines.station_of(a.position).unwrap(),
            lines.station_of(b.position).unwrap(),
        );
        let mut state = Measuring::default();
        state.click(&m, Kind::Stations, "c", point(a.position));
        assert!(state.done.is_empty());
        state.hover(&m, Kind::Stations, "c", Some(point(b.position)));
        assert!(state.live.is_some(), "the value before the click");
        state.click(&m, Kind::Stations, "c", point(b.position));
        let r = &state.done[0];
        if sa.0 == sb.0 {
            assert!((r.rows[0].value - (sb.1 - sa.1)).abs() < 1e-9);
        }
        assert!(state.open && state.picks.is_empty() && state.live.is_none());
        assert!(r.text().contains('\t'));
    }
}

/// The tools in the app: held, two right-clicks (a structure, a wire at
/// mid-span) make a distance in the Measurements panel; the height tool
/// on the same wire point gives a positive height; Escape gives a first
/// pick up; read-only is no obstacle.
#[cfg(test)]
mod app_tests {
    use eframe::egui;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use crate::selection::{Selection, Use};
    use crate::tools::Tool;

    #[test]
    fn measuring_in_the_app() {
        let mut h = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_eframe(|_| crate::app::App::with_model(crate::tests::synthetic(), None));
        h.run();
        let (t1, section, mid) = {
            let r = h.state_mut().ready().unwrap();
            let s = r.model.line.sections[0].clone();
            let mid = r.model.shape(&s.id, r.case).unwrap().spans[0].mid_point;
            (r.model.line.structures[0].clone(), s.id, mid)
        };
        let click = |h: &mut Harness<'_, crate::app::App>, tool: Tool, u: Use| {
            let r = h.state_mut().ready().unwrap();
            r.hotbar.hold(tool);
            r.use_tool(u);
            h.run();
        };
        let span = Use {
            target: Selection::Span {
                section: section.clone(),
                span: 0,
            },
            at: None,
            on: Some(mid),
        };
        click(
            &mut h,
            Tool::MeasureDistance,
            Use {
                target: Selection::Structure(t1.id.clone()),
                at: None,
                on: None,
            },
        );
        assert!(h.state_mut().ready().unwrap().measuring.done.is_empty());
        click(&mut h, Tool::MeasureDistance, span.clone());
        {
            let r = h.state_mut().ready().unwrap();
            let m = &r.measuring.done[0];
            let d: f64 = (0..3)
                .map(|k| (mid[k] - t1.position[k]).powi(2))
                .sum::<f64>()
                .sqrt();
            // To the wire's surface: a little less than to its axis.
            assert!(
                m.rows[0].value < d && m.rows[0].value > d - 0.3,
                "{m:?} {d}"
            );
            assert!(r.measuring.open);
        }
        h.get_by_label("Copy all");
        click(&mut h, Tool::MeasureHeight, span);
        {
            let r = h.state_mut().ready().unwrap();
            let m = r.measuring.done.last().unwrap();
            if !m.rows.is_empty() {
                assert!(m.rows[0].value > 0.0, "{m:?}");
            }
        }
        // A first pick, then Escape: given up.
        click(
            &mut h,
            Tool::MeasureDistance,
            Use {
                target: Selection::Structure(t1.id),
                at: None,
                on: None,
            },
        );
        assert_eq!(h.state_mut().ready().unwrap().measuring.picks.len(), 1);
        h.key_press(egui::Key::Escape);
        h.run();
        assert!(h.state_mut().ready().unwrap().measuring.picks.is_empty());
    }
}
