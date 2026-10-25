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
use ohl_session::annotations::Anchor;

/// A measuring tool's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Between two points.
    Distance,
    /// A point's height above the ground under it.
    Height,
    /// Two points' stations and offsets along the route.
    Stations,
    /// The shortest distance between two wires (their spans or jumpers
    /// picked), exact on the wires as solved.
    WireToWire,
    /// A wire's clearance to the ground at the point picked (vertical and
    /// shortest) and the smallest along its span.
    Clearance,
    /// The shortest distance from a wire (its span or jumper picked) to
    /// an object: a structure's members, a point.
    WireToObject,
}

impl Kind {
    /// The picks it takes.
    pub fn picks(self) -> usize {
        match self {
            Kind::Height | Kind::Clearance => 1,
            Kind::Distance | Kind::Stations | Kind::WireToWire | Kind::WireToObject => 2,
        }
    }

    /// Its key in a kept annotation (`tool`).
    pub fn key(self) -> &'static str {
        match self {
            Kind::Distance => "distance",
            Kind::Height => "height",
            Kind::Stations => "stations",
            Kind::WireToWire => "wire_to_wire",
            Kind::Clearance => "clearance",
            Kind::WireToObject => "wire_to_object",
        }
    }

    /// From its key.
    pub fn of_key(key: &str) -> Option<Kind> {
        [
            Kind::Distance,
            Kind::Height,
            Kind::Stations,
            Kind::WireToWire,
            Kind::Clearance,
            Kind::WireToObject,
        ]
        .into_iter()
        .find(|k| k.key() == key)
    }

    /// Its name.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Distance => "Distance",
            Kind::Height => "Height above ground",
            Kind::Stations => "Stations",
            Kind::WireToWire => "Wire to wire",
            Kind::Clearance => "Clearance at a point",
            Kind::WireToObject => "Wire to object",
        }
    }

    /// Whether pick `k` (from 0) must be on a wire.
    pub fn wants_wire(self, k: usize) -> bool {
        match self {
            Kind::WireToWire => true,
            Kind::Clearance | Kind::WireToObject => k == 0,
            _ => false,
        }
    }
}

type P3 = [f64; 3];

fn dist(a: P3, b: P3) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

/// The wire an anchor is on, as solved in case `case`: its span's or
/// jumper's axis.
fn wire_line(model: &Model, case: usize, anchor: &Anchor) -> Option<Vec<P3>> {
    match anchor {
        Anchor::Wire { section, span, .. } => {
            Some(model.shape(section, case)?.spans.get(*span)?.points.clone())
        }
        Anchor::Jumper { jumper, .. } => Some(model.jumper_shape(jumper, case)?.points.clone()),
        _ => None,
    }
}

/// The closest points of two polylines (exact: every pair of
/// segments) and their distance.
fn closest_lines(a: &[P3], b: &[P3]) -> Option<(P3, P3, f64)> {
    let mut best: Option<(P3, P3, f64)> = None;
    for u in a.windows(2) {
        for v in b.windows(2) {
            let (p, q) = ohl_clearance::geometry::closest_between_segments(u[0], u[1], v[0], v[1]);
            let d = dist(p, q);
            if best.is_none_or(|x| d < x.2) {
                best = Some((p, q, d));
            }
        }
    }
    best
}

/// The point of a polyline closest to `p`, and its distance.
fn closest_on_line(a: &[P3], p: P3) -> Option<(P3, f64)> {
    a.windows(2)
        .map(|w| ohl_clearance::geometry::closest_on_segment(p, w[0], w[1]))
        .min_by(|x, y| x.1.total_cmp(&y.1))
}

/// The ground surface's point closest to `p` within `radius` (in plan)
/// and its distance: every triangle near, exactly.
fn closest_ground(model: &Model, p: P3, radius: f64) -> Option<(P3, f64)> {
    let ground = model.ground_for_checks()?;
    let mut best: Option<(P3, f64)> = None;
    ground.triangles_near(p[0], p[1], radius, |t| {
        let q = ohl_clearance::geometry::closest_on_triangle(p, t[0], t[1], t[2]);
        let d = dist(p, q);
        if best.is_none_or(|b| d < b.1) {
            best = Some((q, d));
        }
    });
    best
}

/// How far around point `p` (in plan) the ground's closest point can
/// be: no farther than the ground straight under it (so its height above
/// that, and a metre), else 100 m.
fn ground_radius(model: &Model, p: P3) -> f64 {
    model
        .ground_at(p[0], p[1])
        .map_or(100.0, |g| (p[2] - g).abs() + 1.0)
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
    /// What it is attached to (kept with a measurement).
    pub anchor: Anchor,
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

/// The point clicked as a pick, in weather case `case` of `model`: what
/// it is attached to ([`anchor_of`]), then its point from that
/// ([`pick_of`]) — the same way a kept measurement is computed again.
pub fn resolve(model: &Model, case: usize, u: &Use) -> Option<Pick> {
    pick_of(model, case, &anchor_of(model, case, u)?)
}

/// What a click is attached to: on a span or a jumper the point of its
/// wire clicked (by its share of the wire's length); an object by id; a
/// LiDAR point by its coordinates and class; else the point under the
/// cursor (the ground; in the profile, the point of its plane).
pub fn anchor_of(model: &Model, case: usize, u: &Use) -> Option<Anchor> {
    Some(match (&u.target, u.on) {
        (Selection::Span { section, span }, Some(on)) => {
            let points = &model.shape(section, case)?.spans.get(*span)?.points;
            Anchor::Wire {
                section: section.clone(),
                span: *span,
                fraction: fraction_along(points, on),
            }
        }
        (Selection::Jumper(id), Some(on)) => Anchor::Jumper {
            jumper: id.clone(),
            fraction: fraction_along(&model.jumper_shape(id, case)?.points, on),
        },
        (Selection::Structure(id), _) => Anchor::Structure {
            structure: id.clone(),
        },
        (Selection::Attachment { structure, set }, _) => Anchor::Attachment {
            structure: structure.clone(),
            set: *set,
        },
        (Selection::SpanPoint { section, label }, _) => Anchor::SpanPoint {
            section: section.clone(),
            label: label.clone(),
        },
        (Selection::AlignmentPoint { id }, _) => Anchor::AlignmentPoint { id: id.clone() },
        (Selection::LidarPoint((_, class)), _) => Anchor::Lidar {
            at: u.target.focus(model, case)?,
            class: *class,
        },
        (Selection::None, _) => Anchor::Point {
            at: u.at?,
            what: String::new(),
        },
        (t, _) => Anchor::Point {
            at: t.focus(model, case)?,
            what: match t {
                Selection::SurveyPoint(k) => format!("survey point {}", k + 1),
                other => other.describe(),
            },
        },
    })
}

/// The share of `points`' length (a polyline) from its start to the
/// point of it nearest to `p`, 0 to 1.
pub fn fraction_along(points: &[[f64; 3]], p: [f64; 3]) -> f64 {
    let d = |a: [f64; 3], b: [f64; 3]| (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>().sqrt();
    let total: f64 = points.windows(2).map(|w| d(w[0], w[1])).sum();
    if total <= 0.0 {
        return 0.0;
    }
    let (mut best, mut at, mut start) = (f64::INFINITY, 0.0, 0.0);
    for w in points.windows(2) {
        let l = d(w[0], w[1]);
        let u = if l > 0.0 {
            ((0..3)
                .map(|k| (p[k] - w[0][k]) * (w[1][k] - w[0][k]))
                .sum::<f64>()
                / (l * l))
                .clamp(0.0, 1.0)
        } else {
            0.0
        };
        let q = [0, 1, 2].map(|k| w[0][k] + u * (w[1][k] - w[0][k]));
        let e = d(q, p);
        if e < best {
            best = e;
            at = start + u * l;
        }
        start += l;
    }
    (at / total).clamp(0.0, 1.0)
}

/// The point at share `f` of `points`' length.
pub fn point_along(points: &[[f64; 3]], f: f64) -> Option<[f64; 3]> {
    let d = |a: [f64; 3], b: [f64; 3]| (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>().sqrt();
    let total: f64 = points.windows(2).map(|w| d(w[0], w[1])).sum();
    let mut left = f.clamp(0.0, 1.0) * total;
    for w in points.windows(2) {
        let l = d(w[0], w[1]);
        if left <= l && l > 0.0 {
            let u = left / l;
            return Some([0, 1, 2].map(|k| w[0][k] + u * (w[1][k] - w[0][k])));
        }
        left -= l;
    }
    points.last().copied()
}

/// The point `anchor` is at in weather case `case` (a wire: with its
/// subconductors and radius, for its surface); none when its object is
/// not there (deleted, not solved in that case).
pub fn pick_of(model: &Model, case: usize, anchor: &Anchor) -> Option<Pick> {
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
    let plain = |at: [f64; 3], what: String| Pick {
        at,
        what,
        wire: None,
        anchor: anchor.clone(),
    };
    Some(match anchor {
        Anchor::Wire {
            section,
            span,
            fraction,
        } => {
            let points = &model.shape(section, case)?.spans.get(*span)?.points;
            let at = point_along(points, *fraction)?;
            Pick {
                at,
                what: format!("wire {section} span {}", span + 1),
                wire: wire(section, points, at),
                anchor: anchor.clone(),
            }
        }
        Anchor::Jumper { jumper, fraction } => {
            let j = model.line.jumpers.iter().find(|j| &j.id == jumper)?;
            let section = model
                .line
                .sections
                .iter()
                .find(|s| match &j.from {
                    ohl_line::JumperEnd::Wire(p) => s.points.contains(p),
                    _ => false,
                })
                .map(|s| s.id.clone());
            let points = &model.jumper_shape(jumper, case)?.points;
            let at = point_along(points, *fraction)?;
            Pick {
                at,
                what: format!("jumper {jumper}"),
                wire: section.and_then(|s| wire(&s, points, at)),
                anchor: anchor.clone(),
            }
        }
        Anchor::Structure { structure } => plain(
            model.line.structure(structure)?.position,
            format!("structure {structure} (base)"),
        ),
        Anchor::OnStructure { structure, offset } => {
            let base = model.line.structure(structure)?.position;
            plain(
                [0, 1, 2].map(|k| base[k] + offset[k]),
                format!("structure {structure}"),
            )
        }
        Anchor::Attachment { structure, set } => plain(
            model
                .line
                .wire_point(&ohl_line::SectionPoint::new(structure.clone(), *set))?
                .position,
            format!("wire point {structure} set {set} (as placed)"),
        ),
        Anchor::SpanPoint { section, label } => plain(
            Selection::SpanPoint {
                section: section.clone(),
                label: label.clone(),
            }
            .focus(model, case)?,
            format!("span point {label} of {section}"),
        ),
        Anchor::AlignmentPoint { id } => plain(
            crate::selection::alignment_point(model, id)?,
            format!("alignment point {id}"),
        ),
        Anchor::Lidar { at, class } => plain(*at, format!("LiDAR point (class {class})")),
        Anchor::Point { at, what } => plain(
            *at,
            if what.is_empty() {
                "point".to_owned()
            } else {
                what.clone()
            },
        ),
    })
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
    measure_with(model, lines, kind, picks, case_name, true)
}

/// [`measure`]; without `whole`, what follows the cursor: the clearance's
/// sweep along the span left for the click.
pub fn measure_with(
    model: &Model,
    lines: &Centrelines,
    kind: Kind,
    picks: &[Pick],
    case_name: &str,
    whole: bool,
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
        Kind::WireToWire | Kind::WireToObject => {
            let case = model
                .cases
                .iter()
                .position(|c| c.case.name == case_name)
                .unwrap_or(0);
            let a_line = wire_line(model, case, &picks[0].anchor);
            // The other end: a wire, a structure's lines, else a point.
            let b_lines: Vec<Vec<P3>> = match (&picks[1].anchor, kind) {
                (a, Kind::WireToWire) => wire_line(model, case, a).into_iter().collect(),
                (Anchor::Structure { structure } | Anchor::OnStructure { structure, .. }, _) => {
                    model
                        .line
                        .structure(structure)
                        .map(|st| {
                            crate::scene::structure_lines(&model.bodies, st)
                                .into_iter()
                                .map(|(_, l)| l)
                                .collect()
                        })
                        .unwrap_or_default()
                }
                _ => vec![vec![picks[1].at]],
            };
            let found = a_line.as_ref().and_then(|a| {
                b_lines
                    .iter()
                    .filter_map(|b| match b.len() {
                        1 => closest_on_line(a, b[0]).map(|(p, d)| (p, b[0], d)),
                        _ => closest_lines(a, b),
                    })
                    .min_by(|x, y| x.2.total_cmp(&y.2))
            });
            match found {
                None => notes.push(if a_line.is_none() {
                    "the first pick is not on a wire (a span or a jumper)".into()
                } else {
                    "nothing to measure to".into()
                }),
                Some((pa, pb, axis)) => {
                    // Each wire's surface at the points found.
                    let on_wire = |pick: &Pick, at: P3| -> Option<Pick> {
                        let line = wire_line(model, case, &pick.anchor)?;
                        let anchor = match &pick.anchor {
                            Anchor::Wire { section, span, .. } => Anchor::Wire {
                                section: section.clone(),
                                span: *span,
                                fraction: fraction_along(&line, at),
                            },
                            Anchor::Jumper { jumper, .. } => Anchor::Jumper {
                                jumper: jumper.clone(),
                                fraction: fraction_along(&line, at),
                            },
                            _ => return None,
                        };
                        pick_of(model, case, &anchor)
                    };
                    let a = on_wire(&picks[0], pa);
                    let b = (kind == Kind::WireToWire)
                        .then(|| on_wire(&picks[1], pb))
                        .flatten();
                    let p = a.as_ref().map_or(pa, |x| surface_towards(x, pb));
                    let q = b.as_ref().map_or(pb, |x| surface_towards(x, p));
                    let p = a.as_ref().map_or(p, |x| surface_towards(x, q));
                    let d = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
                    let h = d[0].hypot(d[1]);
                    rows.push(row("shortest distance", h.hypot(d[2]), "m"));
                    rows.push(row("between the axes", axis, "m"));
                    rows.push(row("horizontal", h, "m"));
                    rows.push(row("vertical (second − first)", d[2], "m"));
                    if h.hypot(d[2]) > 0.0 {
                        rows.push(row(
                            "angle from the vertical",
                            h.atan2(d[2].abs()).to_degrees(),
                            "°",
                        ));
                    }
                    notes.push(
                        "the closest points of the whole span(s) as solved in this case, every \
                         segment against every segment (exact on the solved wire); to the \
                         wires' surfaces"
                            .into(),
                    );
                    line = Some((p, q));
                }
            }
        }
        Kind::Clearance => {
            let case = model
                .cases
                .iter()
                .position(|c| c.case.name == case_name)
                .unwrap_or(0);
            let a = &picks[0];
            let radius = a.wire.as_ref().map_or(0.0, |w| w.radius);
            let lowest = a
                .wire
                .as_ref()
                .and_then(|w| w.axes.iter().copied().min_by(|x, y| x[2].total_cmp(&y[2])))
                .unwrap_or(a.at);
            let bottom = [lowest[0], lowest[1], lowest[2] - radius];
            if let Some(g) = model.ground_at(bottom[0], bottom[1]) {
                rows.push(row("vertical clearance here", bottom[2] - g, "m"));
            }
            // Shortest: from each subconductor's surface.
            let axes = a.wire.as_ref().map_or(vec![a.at], |w| w.axes.clone());
            let shortest = axes
                .iter()
                .filter_map(|x| {
                    closest_ground(model, *x, ground_radius(model, *x)).map(|(q, d)| (*x, q, d))
                })
                .min_by(|x, y| x.2.total_cmp(&y.2));
            match shortest {
                Some((from, q, d)) => {
                    let p = surface_towards(
                        &Pick {
                            at: from,
                            what: String::new(),
                            wire: Some(OnWire {
                                axes: vec![from],
                                radius,
                            }),
                            anchor: a.anchor.clone(),
                        },
                        q,
                    );
                    rows.insert(0, row("shortest to the ground here", d - radius, "m"));
                    line = Some((p, q));
                }
                None => notes.push("no ground surface near this point".into()),
            }
            // Along the whole span (on the click): at each of its solved
            // points, the lowest above the ground first; a point's ground
            // search no wider than the best so far (nothing farther can be
            // nearer).
            if let Some(span) = wire_line(model, case, &a.anchor).filter(|_| whole) {
                let mut low_v: Option<(f64, P3)> = None;
                let mut low_s: Option<(f64, P3)> = None;
                let mut order: Vec<(f64, P3)> = span
                    .iter()
                    .map(|x| {
                        let v = model
                            .ground_at(x[0], x[1])
                            .map_or(f64::INFINITY, |g| x[2] - g);
                        (v, *x)
                    })
                    .collect();
                order.sort_by(|p, q| p.0.total_cmp(&q.0));
                for (v, x) in order {
                    if v.is_finite() && low_v.is_none_or(|b| v - radius < b.0) {
                        low_v = Some((v - radius, x));
                    }
                    let reach = match low_s {
                        Some((best, _)) => (best + radius).min(ground_radius(model, x)),
                        None => ground_radius(model, x),
                    };
                    if let Some((_, d)) = closest_ground(model, x, reach) {
                        let v = d - radius;
                        if low_s.is_none_or(|b| v < b.0) {
                            low_s = Some((v, x));
                        }
                    }
                }
                if let Some((v, _)) = low_v {
                    rows.push(row("smallest vertical along the span", v, "m"));
                }
                if let Some((v, _)) = low_s {
                    rows.push(row("smallest shortest along the span", v, "m"));
                }
                notes.push(format!(
                    "along the span: at its {} solved points (the wire's axis less its radius; \
                     a bundle's subconductors not apart — simplification)",
                    span.len()
                ));
            }
            notes.push(
                "ground: the surface the checks use; the shortest distance over every triangle \
                 that can be nearer than the ground straight under the point, exact; a \
                 measurement, not a check (no rule, no limit)"
                    .into(),
            );
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
    /// The annotations kept in the project, with what each gives now.
    pub kept: Vec<Kept>,
    /// What `kept` was computed from: the annotations' stamp, the line
    /// version, the case and whether it was solved.
    kept_key: Option<(u64, u64, usize, bool)>,
    /// A note being written (the Note tool clicked an object).
    pub note: Option<NoteDraft>,
    /// Commands to perform (keep, delete), taken by the app each frame.
    pub actions: Vec<(String, ohl_session::Command)>,
    /// The follow-and-snap tool ([`crate::follow`]).
    pub follow: crate::follow::Follow,
}

/// An annotation kept in the project and what it gives now.
#[derive(Debug, Clone)]
pub struct Kept {
    /// As kept.
    pub ann: ohl_session::annotations::Annotation,
    /// A dimension computed again (or why it cannot be: its object not
    /// drawn in the case shown).
    pub now: Result<Measurement, String>,
    /// A note's point.
    pub at: Option<[f64; 3]>,
    /// Made by the user (else by another: hidden on a tick).
    pub mine: bool,
}

/// A note being written.
#[derive(Debug, Clone, PartialEq)]
pub struct NoteDraft {
    /// What it is attached to.
    pub anchor: Anchor,
    /// What that is.
    pub what: String,
    /// Its text so far.
    pub text: String,
}

/// Others' kept measurements and notes shown (the Display options' ticks,
/// [`crate::annotations::SectionDisplay`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Others {
    /// Others' measurements.
    pub measurements: bool,
    /// Others' notes.
    pub notes: bool,
}

impl Measuring {
    /// A pick of tool `kind`: kept, and once there are enough the
    /// measurement made (the panel opened).
    pub fn click(
        &mut self,
        model: &Model,
        kind: Kind,
        case_name: &str,
        pick: Pick,
    ) -> Result<(), String> {
        if self.kind != Some(kind) {
            self.picks.clear();
            self.kind = Some(kind);
        }
        if kind.wants_wire(self.picks.len())
            && !matches!(pick.anchor, Anchor::Wire { .. } | Anchor::Jumper { .. })
        {
            return Err(format!(
                "{}: pick a wire (a span or a jumper), not {}",
                kind.name(),
                pick.what
            ));
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
        Ok(())
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
            (self.picks.len() + 1 == kind.picks()
                && (!kind.wants_wire(self.picks.len())
                    || matches!(p.anchor, Anchor::Wire { .. } | Anchor::Jumper { .. })))
            .then(|| {
                let mut all = self.picks.clone();
                all.push(p.clone());
                measure_with(model, lines, kind, &all, case_name, false)
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
    pub fn overlay(&self, others: Others) -> Overlay {
        let mut lines = Vec::new();
        if let Some(m) = self.live.as_ref().or(self.done.last())
            && let Some((a, b)) = m.line
        {
            lines.push((a, b, m.label()));
        }
        let mut kept = Vec::new();
        let mut notes = Vec::new();
        for k in &self.kept {
            let by = if k.mine {
                String::new()
            } else {
                format!(" ({})", k.ann.author)
            };
            match (&k.now, k.at) {
                (Ok(m), _) if k.mine || others.measurements => {
                    if let Some((a, b)) = m.line {
                        let text = if k.ann.text.is_empty() {
                            m.label()
                        } else {
                            format!("{} {}", k.ann.text, m.label())
                        };
                        kept.push((a, b, format!("{text}{by}")));
                    }
                }
                (_, Some(at)) if k.mine || others.notes => {
                    notes.push((at, format!("{}{by}", k.ann.text)));
                }
                _ => {}
            }
        }
        let mut points: Vec<[f64; 3]> = self.picks.iter().map(|p| p.at).collect();
        if let Some(h) = &self.hover {
            points.push(h.at);
        }
        Overlay {
            lines,
            points,
            hover: self.hover.as_ref().map(|h| (h.at, h.what.clone())),
            kept,
            notes,
        }
    }

    /// The kept annotations computed again when they, the line, the case
    /// shown or its solution changed.
    pub fn refresh_kept(&mut self, model: &Model, case: usize) {
        let Some(session) = model.session() else {
            self.kept.clear();
            return;
        };
        let ready = model.cases.get(case).is_some_and(|c| c.ready);
        let key = (session.annotations_stamp(), model.line_version, case, ready);
        if self.kept_key == Some(key) {
            return;
        }
        self.kept_key = Some(key);
        self.lines.update(model);
        let user = model.user_name();
        let case_name = model
            .cases
            .get(case)
            .map_or(String::new(), |c| c.case.name.clone());
        self.kept = session
            .annotations()
            .into_iter()
            .map(|ann| {
                let picks: Option<Vec<Pick>> = ann
                    .anchors
                    .iter()
                    .map(|a| pick_of(model, case, a))
                    .collect();
                let (now, at) = match (Kind::of_key(&ann.tool), picks) {
                    (Some(kind), Some(p)) if p.len() == kind.picks() => {
                        (Ok(measure(model, &self.lines, kind, &p, &case_name)), None)
                    }
                    (None, Some(p)) => (Err(String::new()), p.first().map(|x| x.at)),
                    _ => (Err(format!("not drawn in weather case {case_name}")), None),
                };
                Kept {
                    mine: ann.author == user,
                    ann,
                    now,
                    at,
                }
            })
            .collect();
    }

    /// Measurement `k` of the panel kept in the project (a command to
    /// perform), by `user` at `time`.
    pub fn keep(&mut self, k: usize, user: &str, time: String) {
        let Some(m) = self.done.get(k) else {
            return;
        };
        let id = self.next_id(user);
        let ann = ohl_session::annotations::Annotation {
            id: id.clone(),
            kind: ohl_session::annotations::AnnotationKind::Dimension,
            tool: m.kind.key().to_owned(),
            anchors: m.picks.iter().map(|p| p.anchor.clone()).collect(),
            text: String::new(),
            author: user.to_owned(),
            time,
            last: m.rows.first().map(|r| ohl_session::annotations::LastValue {
                value: r.value,
                unit: r.unit.to_owned(),
                case: m.case.clone(),
            }),
        };
        self.actions.push((
            format!("kept {} {}", m.kind.name().to_lowercase(), m.label()),
            ann.put(),
        ));
        self.done.remove(k);
    }

    /// The note written kept (a command to perform).
    pub fn keep_note(&mut self, user: &str, time: String) {
        let Some(n) = self.note.take() else {
            return;
        };
        if n.text.trim().is_empty() {
            return;
        }
        let ann = ohl_session::annotations::Annotation {
            id: self.next_id(user),
            kind: ohl_session::annotations::AnnotationKind::Note,
            tool: String::new(),
            anchors: vec![n.anchor],
            text: n.text.trim().to_owned(),
            author: user.to_owned(),
            time,
            last: None,
        };
        self.actions
            .push((format!("note on {}", n.what), ann.put()));
    }

    /// A free id for `user`'s next annotation: `user-n`.
    fn next_id(&self, user: &str) -> String {
        let prefix = format!("{user}-");
        let taken = self
            .kept
            .iter()
            .filter_map(|k| k.ann.id.strip_prefix(&prefix)?.parse::<u64>().ok())
            .chain(self.actions.iter().filter_map(|(_, c)| match c {
                ohl_session::Command::Put { id, .. } => id.strip_prefix(&prefix)?.parse().ok(),
                _ => None,
            }))
            .max()
            .unwrap_or(0);
        format!("{prefix}{}", taken + 1)
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
    pub fn window(
        &mut self,
        ctx: &egui::Context,
        tools_on_left: bool,
        display: &mut crate::annotations::SectionDisplay,
        user: &str,
        now: &dyn Fn() -> String,
    ) {
        self.note_window(ctx, user, now);
        if !self.open {
            return;
        }
        let mut open = true;
        let mut delete = None;
        let mut keep = None;
        let mut unkeep = None;
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
                others_ticks(ui, display);
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(420.0)
                    .show(ui, |ui| {
                        // Kept in the project: every user's (others' on
                        // their ticks), each computed again in the case
                        // shown.
                        let shown: Vec<&Kept> = self
                            .kept
                            .iter()
                            .filter(|k| {
                                k.mine
                                    || if k.ann.tool.is_empty() {
                                        !display.hide_others_notes
                                    } else {
                                        !display.hide_others_measurements
                                    }
                            })
                            .collect();
                        if !shown.is_empty() {
                            ui.label(egui::RichText::new("Kept in the project").strong());
                            for k in shown {
                                ui.horizontal(|ui| {
                                    let value = match &k.now {
                                        Ok(m) => m.label(),
                                        Err(e) if e.is_empty() => k.ann.text.clone(),
                                        Err(e) => match &k.ann.last {
                                            Some(l) => format!(
                                                "{} {} (last, {}; {e})",
                                                fmt(l.value, &l.unit),
                                                l.unit,
                                                l.case
                                            ),
                                            None => e.clone(),
                                        },
                                    };
                                    ui.label(egui::RichText::new(value).strong());
                                    let what = Kind::of_key(&k.ann.tool).map_or("Note", Kind::name);
                                    ui.label(
                                        egui::RichText::new(format!("{what} — {}", k.ann.author))
                                            .small()
                                            .weak(),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .small_button("Delete")
                                                .on_hover_text("Delete it from the project (undo brings it back)")
                                                .clicked()
                                            {
                                                unkeep = Some(k.ann.id.clone());
                                            }
                                        },
                                    );
                                });
                            }
                            ui.separator();
                        }
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
                                            if ui
                                                .small_button("Keep")
                                                .on_hover_text(
                                                    "Keep it in the project: shared, computed \
                                                     again when the line changes, deleted with \
                                                     what it is attached to",
                                                )
                                                .clicked()
                                            {
                                                keep = Some(k);
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
        if let Some(k) = keep {
            self.keep(k, user, now());
        }
        if let Some(id) = unkeep {
            self.actions.push((
                format!("deleted annotation {id}"),
                ohl_session::annotations::delete(&id),
            ));
        }
        self.open = open;
    }

    /// The window of a note being written: its text, Keep (Enter) or
    /// Cancel (Escape).
    fn note_window(&mut self, ctx: &egui::Context, user: &str, now: &dyn Fn() -> String) {
        let Some(n) = &mut self.note else {
            return;
        };
        let (mut keep, mut cancel) = (false, false);
        egui::Window::new("Note")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!("On {}", n.what));
                let r = ui.text_edit_singleline(&mut n.text);
                r.request_focus();
                keep |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                cancel |= ui.input(|i| i.key_pressed(egui::Key::Escape));
                ui.horizontal(|ui| {
                    keep |= ui
                        .add_enabled(!n.text.trim().is_empty(), egui::Button::new("Keep"))
                        .clicked();
                    cancel |= ui.button("Cancel").clicked();
                });
                ui.label(
                    egui::RichText::new(
                        "Kept in the project, shared; deleted with what it is attached to.",
                    )
                    .small()
                    .weak(),
                );
            });
        if keep {
            self.keep_note(user, now());
            self.open = true;
        } else if cancel {
            self.note = None;
        }
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
    /// The kept measurements' lines and labels.
    pub kept: Vec<([f64; 3], [f64; 3], String)>,
    /// The kept notes: their point and text.
    pub notes: Vec<([f64; 3], String)>,
}

/// The kept measurements' and notes' colour.
const KEPT: egui::Color32 = egui::Color32::from_rgb(255, 190, 90);

/// The overlay's colour (apart from the checks' yellow).
const COLOUR: egui::Color32 = egui::Color32::from_rgb(90, 230, 255);

impl Overlay {
    /// Whether there is nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
            && self.points.is_empty()
            && self.kept.is_empty()
            && self.notes.is_empty()
    }

    /// Draws it with `painter`, points to the screen by `screen`.
    pub fn paint(&self, painter: &egui::Painter, screen: &dyn Fn([f64; 3]) -> Option<egui::Pos2>) {
        // The kept ones under what is being measured.
        for (a, b, label) in &self.kept {
            let (Some(p), Some(q)) = (screen(*a), screen(*b)) else {
                continue;
            };
            for (w, c) in [(3.5, egui::Color32::from_black_alpha(160)), (1.5, KEPT)] {
                painter.line_segment([p, q], egui::Stroke::new(w, c));
            }
            for x in [p, q] {
                painter.circle_filled(x, 2.5, KEPT);
            }
            let mid = p + (q - p) * 0.5;
            label_at(painter, mid + egui::vec2(8.0, -8.0), label, KEPT);
        }
        // A note: a short leader up to its text.
        for (at, text) in &self.notes {
            let Some(p) = screen(*at) else {
                continue;
            };
            let to = p + egui::vec2(18.0, -22.0);
            painter.line_segment([p, to], egui::Stroke::new(1.5, KEPT));
            painter.circle_filled(p, 3.0, KEPT);
            label_at(painter, to + egui::vec2(2.0, 4.0), text, KEPT);
        }
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

/// The ticks hiding others' kept measurements and notes (the user's view
/// preference).
pub fn others_ticks(ui: &mut egui::Ui, display: &mut crate::annotations::SectionDisplay) {
    let mut m = !display.hide_others_measurements;
    let mut n = !display.hide_others_notes;
    ui.horizontal(|ui| {
        if ui
            .checkbox(&mut m, "Others' measurements")
            .on_hover_text("Measurements other users kept in the project")
            .changed()
        {
            display.hide_others_measurements = !m;
        }
        if ui
            .checkbox(&mut n, "Others' notes")
            .on_hover_text("Notes other users kept in the project")
            .changed()
        {
            display.hide_others_notes = !n;
        }
    });
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

    fn free(at: [f64; 3]) -> Anchor {
        Anchor::Point {
            at,
            what: String::new(),
        }
    }

    fn point(at: [f64; 3]) -> Pick {
        Pick {
            at,
            what: "p".into(),
            wire: None,
            anchor: free(at),
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
            anchor: free([0.0, 0.0, 10.0]),
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
            anchor: free(s.position),
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
        state
            .click(&m, Kind::Stations, "c", point(a.position))
            .unwrap();
        assert!(state.done.is_empty());
        state.hover(&m, Kind::Stations, "c", Some(point(b.position)));
        assert!(state.live.is_some(), "the value before the click");
        state
            .click(&m, Kind::Stations, "c", point(b.position))
            .unwrap();
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

/// Phase 2 in the app: a distance kept goes into the project (computed
/// again from its anchors), a note is written with the Note tool, and
/// deleting the structure a measurement is attached to deletes it in the
/// same command — undo brings both back; others' are hidden on a tick.
#[cfg(test)]
mod keep_tests {
    use eframe::egui;
    use egui_kittest::Harness;

    use crate::selection::{Selection, Use};
    use crate::tools::Tool;

    #[test]
    fn kept_measurements_and_notes() {
        let mut h = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_eframe(|_| {
                crate::app::App::with_model(crate::tests::synthetic_project("keep"), None)
            });
        h.run();
        let (t1, t2, mid) = {
            let r = h.state_mut().ready().unwrap();
            let s = &r.model.line.sections[0];
            let mid = r.model.shape(&s.id, r.case).unwrap().spans[0].mid_point;
            let st = &r.model.line.structures;
            (st[0].id.clone(), st[st.len() - 1].id.clone(), mid)
        };
        let structure = |id: &str| Use {
            target: Selection::Structure(id.to_owned()),
            at: None,
            on: None,
        };
        let click = |h: &mut Harness<'_, crate::app::App>, tool: Tool, u: Use| {
            let r = h.state_mut().ready().unwrap();
            r.hotbar.hold(tool);
            r.use_tool(u);
            h.run();
        };
        click(&mut h, Tool::MeasureDistance, structure(&t1));
        click(
            &mut h,
            Tool::MeasureDistance,
            Use {
                target: Selection::None,
                at: Some(mid),
                on: None,
            },
        );
        {
            let r = h.state_mut().ready().unwrap();
            let user = r.model.user_name();
            r.measuring.keep(0, &user, String::new());
        }
        h.run();
        click(&mut h, Tool::Note, structure(&t2));
        {
            let r = h.state_mut().ready().unwrap();
            r.measuring.note.as_mut().unwrap().text = "access road".into();
            let user = r.model.user_name();
            r.measuring.keep_note(&user, String::new());
        }
        h.run();
        let kept = |h: &mut Harness<'_, crate::app::App>| {
            let r = h.state_mut().ready().unwrap();
            r.model.session().unwrap().annotations().len()
        };
        assert_eq!(kept(&mut h), 2);
        {
            let r = h.state_mut().ready().unwrap();
            assert!(r.measuring.kept.iter().all(|k| k.mine));
            let m = r.measuring.kept[0].now.as_ref().unwrap();
            assert!(m.rows[0].value > 0.0);
            // Others' hidden: one's own still drawn.
            let o = r.measuring.overlay(super::Others {
                measurements: false,
                notes: false,
            });
            assert_eq!((o.kept.len(), o.notes.len()), (1, 1));
        }
        // T1 deleted: its measurement goes with it, the note stays.
        {
            let r = h.state_mut().ready().unwrap();
            r.perform(crate::edit::Action::Command {
                what: "delete".into(),
                command: ohl_session::Command::DeleteStructure {
                    id: t1.clone(),
                    confirm: true,
                },
            });
        }
        h.run();
        assert_eq!(kept(&mut h), 1);
        h.state_mut().ready().unwrap().model.undo().unwrap();
        h.run();
        assert_eq!(kept(&mut h), 2);
        // Follow and snap: the first click picks the object (selected);
        // with a measurement found, the next click keeps it.
        let span = Selection::Span {
            section: h.state_mut().ready().unwrap().model.line.sections[0]
                .id
                .clone(),
            span: 0,
        };
        click(
            &mut h,
            Tool::Follow,
            Use {
                target: span.clone(),
                at: None,
                on: Some(mid),
            },
        );
        {
            let r = h.state_mut().ready().unwrap();
            assert_eq!(r.selection, span);
            assert!(r.measuring.follow.subject.is_some());
            let lines = super::Centrelines::of(&r.model);
            let wire = super::resolve(
                &r.model,
                r.case,
                &Use {
                    target: span.clone(),
                    at: None,
                    on: Some(mid),
                },
            )
            .unwrap();
            let ground = super::Pick {
                at: [mid[0], mid[1], mid[2] - 12.0],
                what: "LiDAR point".into(),
                wire: None,
                anchor: ohl_session::annotations::Anchor::Lidar {
                    at: [mid[0], mid[1], mid[2] - 12.0],
                    class: 5,
                },
            };
            r.measuring.follow.live = Some(super::measure(
                &r.model,
                &lines,
                super::Kind::Distance,
                &[wire, ground],
                "c",
            ));
            r.use_tool(Use {
                target: Selection::None,
                at: None,
                on: None,
            });
        }
        h.run();
        assert_eq!(kept(&mut h), 3);
    }
}

/// Phase 3's wire tools on the example line: wire to wire is the closest
/// approach of the two spans (to the surfaces: less than between the
/// axes), the clearance's shortest distance is no more than its vertical
/// one, a wire to a structure is measured to its lines.
#[cfg(test)]
mod wire_tests {
    use super::*;

    fn wire(m: &Model, section: usize, f: f64) -> Pick {
        let s = &m.line.sections[section];
        pick_of(
            m,
            0,
            &Anchor::Wire {
                section: s.id.clone(),
                span: 0,
                fraction: f,
            },
        )
        .unwrap()
    }

    #[test]
    fn wire_tools() {
        let m = crate::tests::synthetic();
        let lines = Centrelines::of(&m);
        let case = m.cases[0].case.name.clone();
        let (a, b) = (wire(&m, 0, 0.5), wire(&m, 1, 0.5));
        let r = measure(&m, &lines, Kind::WireToWire, &[a.clone(), b.clone()], &case);
        let (d, axes) = (r.rows[0].value, r.rows[1].value);
        assert!(d > 0.0 && d < axes, "{r:?}");
        // No closer than the brute force on the axes.
        let la = wire_line(&m, 0, &a.anchor).unwrap();
        let lb = wire_line(&m, 0, &b.anchor).unwrap();
        let brute = la
            .iter()
            .flat_map(|p| lb.iter().map(move |q| dist(*p, *q)))
            .fold(f64::INFINITY, f64::min);
        assert!(axes <= brute + 1e-9, "{axes} {brute}");
        // Clearance: shortest no more than vertical.
        let c = measure(&m, &lines, Kind::Clearance, std::slice::from_ref(&a), &case);
        let get = |name: &str| c.rows.iter().find(|r| r.name == name).map(|r| r.value);
        if let (Some(s), Some(v)) = (
            get("shortest to the ground here"),
            get("vertical clearance here"),
        ) {
            assert!(s <= v + 1e-9 && s > 0.0, "{c:?}");
        }
        assert!(get("smallest vertical along the span").is_some(), "{c:?}");
        // A wire to a structure.
        let t = pick_of(
            &m,
            0,
            &Anchor::Structure {
                structure: m.line.structures[2].id.clone(),
            },
        )
        .unwrap();
        let o = measure(&m, &lines, Kind::WireToObject, &[a, t], &case);
        assert!(
            o.rows[0].value.is_finite() && o.rows[0].value >= 0.0,
            "{o:?}"
        );
        // Not a wire first: refused, said why.
        let mut st = Measuring::default();
        let free = Pick {
            at: [0.0, 0.0, 0.0],
            what: "point".into(),
            wire: None,
            anchor: Anchor::Point {
                at: [0.0, 0.0, 0.0],
                what: String::new(),
            },
        };
        assert!(st.click(&m, Kind::WireToWire, &case, free).is_err());
    }
}
