// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Layout of plan and profile on sheets: one continuous drawing, sheets of
//! a page format split at structures, and model-space drawings (plan in
//! project coordinates, profile by station and exaggerated height).

use crate::furniture;
use crate::model::{Anchor, Circle, Clip, Color, Drawing, Item, Layer, Path, Pen, Text};
use crate::style::Sample;
use crate::{ElevationPlace, Options, Page, RouteDrawing, format_station, nice_step};

/// A stretch of the profile drawn with its own height range: the whole
/// profile, or one of the parts of a sheet whose datum is **stepped**
/// (the ground and the wires do not fit the panel in one height range at
/// the vertical scale; the scale is never changed).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Part {
    /// Station range.
    pub s0: f64,
    pub s1: f64,
    /// Height range shown, m.
    pub z0: f64,
    pub z1: f64,
    /// Grid step of the heights, m.
    pub step: f64,
}

/// Where things go on a sheet (all mm, except stations and heights).
pub(crate) struct Frame {
    /// Station range drawn: the panels end here (at the match lines).
    pub s0: f64,
    pub s1: f64,
    /// Left edge of the drawing area.
    pub left: f64,
    /// The plan panel is drawn.
    pub plan: bool,
    /// Plan panel top and height.
    pub plan_top: f64,
    pub plan_h: f64,
    /// Where the line's axis runs in the plan panel (y, mm).
    pub plan_axis: f64,
    /// Profile panel top and height.
    pub prof_top: f64,
    pub prof_h: f64,
    /// The profile's parts, in station order (one unless the datum is
    /// stepped).
    pub parts: Vec<Part>,
    /// mm per m: along the route, across in the plan, vertical.
    pub h: f64,
    pub plan_k: f64,
    pub v: f64,
    /// A structure at either end is drawn whole (sheets), not cut at the
    /// match line.
    pub whole_ends: bool,
    /// Where the paper ends along the route (the next sheet's content
    /// faded to there, when the template asks).
    pub paper_end: f64,
    /// The band rows' height under the profile, mm.
    pub bands_h: f64,
    /// The legend, made as the frame is drawn.
    pub legend: std::cell::RefCell<crate::style::Legend>,
}

impl Frame {
    pub(crate) fn x(&self, s: f64) -> f64 {
        self.left + (s - self.s0) * self.h
    }
    pub(crate) fn plan_y(&self, offset: f64) -> f64 {
        self.plan_axis + offset * self.plan_k
    }
    pub(crate) fn y_in(&self, p: &Part, z: f64) -> f64 {
        self.prof_top + (p.z1 - z) * self.v
    }
    pub(crate) fn width(&self) -> f64 {
        ((self.s1 - self.s0) * self.h).max(0.0)
    }
    /// The part holding station `s` (the first of two at a step).
    pub(crate) fn part_at(&self, s: f64) -> &Part {
        self.parts
            .iter()
            .find(|p| s <= p.s1 + 1e-6)
            .unwrap_or_else(|| self.parts.last().expect("a part"))
    }
    /// Whether structure `m` stands at either end of the frame.
    fn at_end(&self, m: &crate::Mark) -> bool {
        self.whole_ends
            && ((m.station - self.s0).abs() < 1e-6 || (m.station - self.s1).abs() < 1e-6)
    }
}

/// Room above a structure's top for its name and station on paper, mm.
pub(crate) const LABEL_ROOM_MM: f64 = 7.0;

/// The height ranges of what lies between stations `a` and `b`: what must
/// be seen (the ground and side ground, the clearance line, each wire's
/// lowest point, markers) and everything (the whole wires, the structures
/// and their drawings), m; infinite when there is nothing.
pub(crate) fn heights(d: &RouteDrawing, o: &Options, a: f64, b: f64) -> ([f64; 2], [f64; 2]) {
    let (essential, lows, full) = heights_split(d, o, a, b);
    joined(essential, lows.iter().copied(), full)
}

/// What [`heights`] joins: the essential range without the wires, each
/// wire's lowest height (infinite: none within), and the full range of
/// what is not essential. Each is a minimum or a maximum over the
/// stations, so the ranges of stretches side by side join exactly.
fn heights_split(d: &RouteDrawing, o: &Options, a: f64, b: f64) -> ([f64; 2], Vec<f64>, [f64; 2]) {
    let mut essential = [f64::INFINITY, f64::NEG_INFINITY];
    let within = |s: f64| s >= a - 1e-6 && s <= b + 1e-6;
    let see = |r: &mut [f64; 2], s: f64, z: f64| {
        if within(s) && z.is_finite() {
            r[0] = r[0].min(z);
            r[1] = r[1].max(z);
        }
    };
    let clearance = o.clearance.unwrap_or(0.0).max(0.0);
    for run in &d.ground {
        for p in run {
            see(&mut essential, p[0], p[1]);
            see(&mut essential, p[0], p[1] + clearance);
        }
    }
    for p in d.sides.iter().flat_map(|(_, r)| r).flatten() {
        see(&mut essential, p[0], p[1]);
    }
    // Of each wire its lowest point within the range (what the ground
    // clearance is about); the whole wire with the structures.
    let lows = d
        .wires
        .iter()
        .map(|w| {
            w.points
                .iter()
                .filter(|p| within(p[0]) && p[2].is_finite())
                .map(|p| p[2])
                .fold(f64::INFINITY, f64::min)
        })
        .collect();
    for (p, _) in &d.markers {
        see(&mut essential, p[0], p[2]);
    }
    let mut full = [f64::INFINITY, f64::NEG_INFINITY];
    // The ground along the corridor's edges: seen when it fits (the
    // LiDAR points are not: they are clipped at the panel).
    for p in d.corridor_ground.iter().flat_map(|(_, r)| r).flatten() {
        see(&mut full, p[0], p[1]);
    }
    for w in d
        .wires
        .iter()
        .chain(d.versions.iter().flat_map(|v| &v.wires))
    {
        for p in &w.points {
            see(&mut full, p[0], p[2]);
        }
    }
    for m in d
        .structures
        .iter()
        .chain(d.versions.iter().flat_map(|v| &v.structures))
    {
        see(&mut full, m.station, m.base);
        see(&mut full, m.station, m.top);
        for p in m.sticks.iter().chain(&m.members).chain(&m.pole).flatten() {
            see(&mut full, m.station, p[2]);
        }
    }
    (essential, lows, full)
}

/// [`heights`] from the parts of [`heights_split`].
fn joined(
    mut essential: [f64; 2],
    lows: impl Iterator<Item = f64>,
    rest: [f64; 2],
) -> ([f64; 2], [f64; 2]) {
    for z in lows.filter(|z| z.is_finite()) {
        essential[0] = essential[0].min(z);
        essential[1] = essential[1].max(z);
    }
    let full = [essential[0].min(rest[0]), essential[1].max(rest[1])];
    (essential, full)
}

/// The heights of each stretch between neighbouring stops, to join
/// those of a sheet quickly ([`StretchHeights::warnings`]).
pub(crate) struct StretchHeights {
    parts: Vec<([f64; 2], Vec<f64>, [f64; 2])>,
}

impl StretchHeights {
    pub(crate) fn new(d: &RouteDrawing, o: &Options, stations: &[Stop]) -> Self {
        Self {
            parts: stations
                .windows(2)
                .map(|w| heights_split(d, o, w[0].station, w[1].station))
                .collect(),
        }
    }

    /// [`heights`] from stop `i` to stop `j` (joined, exactly).
    pub(crate) fn between(&self, i: usize, j: usize) -> ([f64; 2], [f64; 2]) {
        let parts = &self.parts[i..j];
        let mut essential = [f64::INFINITY, f64::NEG_INFINITY];
        let mut rest = [f64::INFINITY, f64::NEG_INFINITY];
        let wires = parts.first().map_or(0, |p| p.1.len());
        let mut lows = vec![f64::INFINITY; wires];
        for (e, l, r) in parts {
            essential = [essential[0].min(e[0]), essential[1].max(e[1])];
            rest = [rest[0].min(r[0]), rest[1].max(r[1])];
            for (a, b) in lows.iter_mut().zip(l) {
                *a = a.min(*b);
            }
        }
        joined(essential, lows.into_iter(), rest)
    }

    /// The profile warnings [`fit`] gives a sheet from stop `i` to stop
    /// `j` in a panel `avail` mm tall, estimated: none when everything
    /// fits, one when the ground and the wires' lowest points fit (the
    /// tops cut), two when they do not (the datum stepped).
    pub(crate) fn warnings(&self, o: &Options, i: usize, j: usize, avail: f64) -> usize {
        let v = 1000.0 / o.vertical_scale;
        let room = (avail / v).max(1e-3);
        let (essential, full) = self.between(i, j);
        if !full[0].is_finite() || full[1] + LABEL_ROOM_MM / v - full[0] <= room + 1e-9 {
            0
        } else if !essential[0].is_finite() || essential[1] - essential[0] <= room + 1e-9 {
            1
        } else {
            2
        }
    }
}

/// Height range of what lies between stations `s0` and `s1`, rounded to
/// grid steps, with room on top for the structures' names: the
/// continuous drawing's profile.
pub(crate) fn height_range(d: &RouteDrawing, o: &Options, s0: f64, s1: f64) -> Part {
    let (essential, full) = heights(d, o, s0, s1);
    let [mut z0, mut z1] = if full[0].is_finite() { full } else { essential };
    if !z0.is_finite() {
        (z0, z1) = (0.0, 10.0);
    }
    let v = 1000.0 / o.vertical_scale;
    z1 += LABEL_ROOM_MM / v;
    let step = nice_step((z1 - z0).max(1.0), 8.0);
    Part {
        s0,
        s1,
        z0: (z0 / step).floor() * step,
        z1: (z1 / step).ceil() * step,
        step,
    }
}

/// The profile of a sheet from `s0` to `s1` in a panel `avail` mm tall at
/// `v` mm per m: one part when everything fits (the ground at the bottom
/// of the panel); when only the ground, wires and clearance line fit, one
/// part with the structures' tops cut (warned); else parts split at the
/// structures on the line (the datum stepped, warned), each as long as
/// its ground and wires fit. Never another vertical scale; never an
/// empty profile (a part whose ground and wires alone do not fit is
/// drawn from its lowest point, its top cut, warned).
pub(crate) fn fit(
    d: &RouteDrawing,
    o: &Options,
    s0: f64,
    s1: f64,
    avail: f64,
) -> (Vec<Part>, Vec<String>) {
    let v = 1000.0 / o.vertical_scale;
    let room = (avail / v).max(1e-3);
    let label = LABEL_ROOM_MM / v;
    let step = nice_step(room, 8.0);
    // A part from `a` to `b` showing `lo` to `hi` (kept within it if it
    // can be, at a grid line below `lo` if that keeps `hi` in).
    let part = |a: f64, b: f64, lo: f64, hi: f64| {
        let mut z0 = (lo / step).floor() * step;
        if z0 + room < hi {
            z0 = lo;
        }
        Part {
            s0: a,
            s1: b,
            z0,
            z1: z0 + room,
            step,
        }
    };
    let (essential, full) = heights(d, o, s0, s1);
    if !full[0].is_finite() {
        return (vec![part(s0, s1, 0.0, 0.0)], Vec::new());
    }
    let essential = if essential[0].is_finite() {
        essential
    } else {
        full
    };
    let mut warnings = Vec::new();
    let needed = |r: [f64; 2]| (r[1] - r[0]) * v;
    if full[1] + label - full[0] <= room + 1e-9 {
        return (vec![part(s0, s1, full[0], full[1] + label)], warnings);
    }
    let parts = if essential[1] - essential[0] <= room + 1e-9 {
        vec![part(s0, s1, essential[0], essential[1])]
    } else {
        // Datum steps at the structures on the line within the sheet.
        let mut stops: Vec<(f64, String)> = d
            .structures
            .iter()
            .filter(|m| !m.away && m.station > s0 + 1e-6 && m.station < s1 - 1e-6)
            .map(|m| (m.station, m.id.clone()))
            .collect();
        stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        stops.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-6);
        stops.push((s1, String::new()));
        let mut parts: Vec<Part> = Vec::new();
        let (mut a, mut k) = (s0, 0);
        while k < stops.len() {
            let mut best = None;
            for (j, stop) in stops.iter().enumerate().skip(k) {
                let r = heights(d, o, a, stop.0).0;
                if !r[0].is_finite() || r[1] - r[0] <= room + 1e-9 {
                    best = Some(j);
                } else {
                    break;
                }
            }
            let j = best.unwrap_or(k);
            let r = heights(d, o, a, stops[j].0).0;
            let r = if r[0].is_finite() {
                r
            } else {
                parts.last().map_or(essential, |p| [p.z0, p.z0])
            };
            if best.is_none() {
                warnings.push(format!(
                    "the ground and the wires' lowest points of {}–{} need {:.0} mm of height at 1:{}, the panel \
                     has {:.0} mm: drawn from the lowest point, the top cut",
                    format_station(a),
                    format_station(stops[j].0),
                    needed(r),
                    o.vertical_scale,
                    avail
                ));
            }
            parts.push(part(a, stops[j].0, r[0], r[1]));
            a = stops[j].0;
            k = j + 1;
        }
        for w in parts.windows(2) {
            let at = stops
                .iter()
                .find(|s| (s.0 - w[1].s0).abs() < 1e-6)
                .map_or_else(|| format_station(w[1].s0), |s| s.1.clone());
            warnings.push(format!(
                "datum stepped at {at} ({:+.0} m): the ground and the wires' lowest points of \
                 the sheet need \
                 {:.0} mm of height at 1:{}, the panel has {:.0} mm",
                w[1].z0 - w[0].z0,
                needed(essential),
                o.vertical_scale,
                avail
            ));
        }
        parts
    };
    // Structures whose tops (or names) are cut at the panel's top.
    let cut: Vec<String> = d
        .structures
        .iter()
        .filter(|m| m.station >= s0 - 1e-6 && m.station <= s1 + 1e-6)
        .filter(|m| {
            let p = parts
                .iter()
                .find(|p| m.station <= p.s1 + 1e-6)
                .unwrap_or(&parts[parts.len() - 1]);
            structure_top(m) + label > p.z1 + 1e-9
        })
        .map(|m| m.id.clone())
        .collect();
    if !cut.is_empty() {
        warnings.push(format!(
            "structure tops cut at the top of the profile ({}): with them the profile needs \
             {:.0} mm of height at 1:{}, the panel has {:.0} mm",
            cut.join(", "),
            needed([full[0], full[1] + label]),
            o.vertical_scale,
            avail
        ));
    }
    (parts, warnings)
}

/// The highest point drawn of structure `m`.
fn structure_top(m: &crate::Mark) -> f64 {
    m.members
        .iter()
        .chain(&m.pole)
        .chain(&m.sticks)
        .flatten()
        .map(|p| p[2])
        .fold(m.top, f64::max)
}

/// Grid values from `from` to `to` every `step` (at most 10 000: the
/// exit clause of the grid loops).
fn ticks(from: f64, to: f64, step: f64) -> impl Iterator<Item = f64> {
    let first = (from / step).ceil();
    let n = if step > 0.0 && to >= from {
        (((to + 1e-9) / step).floor() - first + 1.0).clamp(0.0, 10_000.0) as usize
    } else {
        0
    };
    (0..n).map(move |k| (first + k as f64) * step)
}

fn hex(c: &str) -> Color {
    Color::hex_or_black(c)
}

fn pen(c: &str, width: f64) -> Pen {
    Pen::new(hex(c), width)
}

fn text(layer: Layer, at: [f64; 2], size: f64, anchor: Anchor, t: impl Into<String>) -> Item {
    Item::Text(Text {
        layer,
        at,
        text: t.into(),
        size,
        anchor,
        bold: false,
        color: Color::BLACK,
    })
}

fn bold(mut item: Item) -> Item {
    if let Item::Text(t) = &mut item {
        t.bold = true;
    }
    item
}

fn path(layer: Layer, points: Vec<[f64; 2]>, pen: Pen, title: Option<String>) -> Item {
    Item::Path(Path {
        layer,
        points,
        closed: false,
        pen,
        title,
    })
}

fn rect(layer: Layer, x: f64, y: f64, w: f64, h: f64, pen: Pen) -> Item {
    Item::Path(Path {
        layer,
        points: vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
        closed: true,
        pen,
        title: None,
    })
}

fn marker(centre: [f64; 2], radius: f64, label: &str) -> Item {
    Item::Circle(Circle {
        layer: Layer::Markers,
        centre,
        radius,
        stroke: Some(pen("#d00", 0.5)),
        fill: None,
        title: Some(label.to_owned()),
    })
}

/// How far on paper (mm) the drawing of structure `m` reaches along the
/// route from its station, with a millimetre to spare.
fn half_width_mm(m: &crate::Mark, h: f64) -> f64 {
    m.attachments
        .iter()
        .chain(m.sticks.iter().flatten())
        .chain(m.members.iter().flatten())
        .chain(m.pole.iter().flatten())
        .map(|p| (p[0] - m.station).abs())
        .filter(|s| s.is_finite())
        .fold(0.0, f64::max)
        * h
        + 1.0
}

/// The pen of `layer` as the options' template sets it, from the
/// drawing's own colour and width.
fn lp(o: &Options, layer: Layer, color: &str, width: f64) -> Pen {
    o.template.pen(layer, pen(color, width))
}

/// The axis's pen in the plan: the template's `[centreline] pen` over its
/// layer's.
fn centreline_pen(o: &Options) -> Pen {
    o.template
        .centreline
        .pen
        .over(lp(o, Layer::Centreline, "#999", 0.25).dashed("4 1 1 1"))
}

/// The pen of the corridor's edge at `offset` (right positive) and
/// whether it is the near one: plan, key map and its ground in the
/// profile alike.
pub(crate) fn corridor_edge(o: &Options, offset: f64) -> (Pen, bool) {
    o.template.corridor.edge_pen(
        offset,
        lp(o, Layer::Corridor, "#6a6a6a", 0.25).dashed("6 1.5 1 1.5"),
    )
}

/// The legend's entries of a corridor of half width `w`: one (its label)
/// when both edges look alike and no ground is drawn along them, else
/// the near and the far edge.
fn corridor_legend(o: &Options, f: &Frame, d: &RouteDrawing, w: f64, label: &str) {
    let near = o.template.corridor.near_offset(w);
    let (pn, _) = corridor_edge(o, near);
    let (pf, _) = corridor_edge(o, -near);
    let mut legend = f.legend.borrow_mut();
    if pn == pf && d.corridor_ground.is_empty() {
        legend.add(Sample::Line(pn), label);
        return;
    }
    let width = crate::template::format_number(w, "");
    let side = |o: f64| if o >= 0.0 { "right" } else { "left" };
    let labels = &o.template.legend;
    legend.add(
        Sample::Line(pn),
        labels.label("corridor_near", &[("side", side(near)), ("width", &width)]),
    );
    legend.add(
        Sample::Line(pf),
        labels.label("corridor_far", &[("side", side(-near)), ("width", &width)]),
    );
}

/// The colour and diameter of LiDAR class `code`'s dots: the template's
/// style, else the class scheme's colour, else grey.
fn lidar_look(o: &Options, code: u8) -> (Color, f64) {
    let spec = &o.template.lidar;
    let style = spec.style(code);
    let color = style
        .and_then(|s| s.colour.as_deref())
        .and_then(Color::parse)
        .or_else(|| {
            o.class_names
                .get(&code)
                .and_then(|(_, c)| *c)
                .map(|c| Color(c[0], c[1], c[2]))
        })
        .unwrap_or(Color(0x80, 0x80, 0x80));
    let size = style.and_then(|s| s.size).map_or(spec.size.0, |s| s.0);
    (color, size)
}

/// The LiDAR dots of route `d` on frame `f` (from the sheet's start to
/// the paper's end), each point placed by `at` ((station, offset, z) →
/// paper; none: not drawn): **one dot per class in each square cell** of
/// the template's `cell` on paper (what is drawn only; the points are not
/// changed), each class in its colour, listed in the legend by its name.
fn lidar_dots(
    o: &Options,
    f: &Frame,
    d: &RouteDrawing,
    at: impl Fn(&[f64; 3]) -> Option<[f64; 2]>,
) -> Vec<Item> {
    let classes = o.lidar_class_list();
    if classes.is_empty() || d.lidar.is_empty() {
        return Vec::new();
    }
    let mut wanted = [false; 256];
    for &c in &classes {
        wanted[c as usize] = true;
    }
    let cell = o.template.lidar.cell.0;
    let (a, b) = (f.s0 - 5.0 / f.h, f.s1.max(f.paper_end) + 5.0 / f.h);
    let from = d.lidar.partition_point(|p| p.0[0] < a);
    let to = d.lidar.partition_point(|p| p.0[0] <= b);
    let mut seen = std::collections::HashSet::new();
    let mut drawn = [false; 256];
    let mut out = Vec::new();
    for (q, c) in &d.lidar[from..to.max(from)] {
        if !wanted[*c as usize] {
            continue;
        }
        let Some(p) = at(q) else { continue };
        if cell > 0.0
            && !seen.insert((
                (p[0] / cell).floor() as i64,
                (p[1] / cell).floor() as i64,
                *c,
            ))
        {
            continue;
        }
        let (color, size) = lidar_look(o, *c);
        drawn[*c as usize] = true;
        out.push(Item::Circle(Circle {
            layer: Layer::Lidar,
            centre: p,
            radius: 0.5 * size,
            stroke: None,
            fill: Some(color),
            title: None,
        }));
    }
    let mut legend = f.legend.borrow_mut();
    for c in classes {
        if !drawn[c as usize] {
            continue;
        }
        let (color, size) = lidar_look(o, c);
        // A group's label names the group (its key), once.
        let (name, code) = match o.template.lidar.style_entry(c) {
            Some((key, s)) if s.label.is_some() => {
                (s.label.clone().unwrap_or_default(), key.to_owned())
            }
            _ => (
                o.class_names
                    .get(&c)
                    .map_or_else(|| format!("class {c}"), |(n, _)| n.clone()),
                c.to_string(),
            ),
        };
        legend.add(
            Sample::Dots(color, size),
            o.template
                .legend
                .label("lidar", &[("name", &name), ("class", &code)]),
        );
    }
    out
}

/// The structures drawn in the plan and the profile: the line's, and
/// those of the other versions with their roles: (structure, role, its
/// name written, of another version).
fn all_structures<'a>(
    d: &'a RouteDrawing,
    o: &'a Options,
) -> impl Iterator<Item = (&'a crate::Mark, Option<&'a str>, bool, bool)> {
    d.structures
        .iter()
        .map(|m| (m, o.role.as_deref(), true, false))
        .chain(d.versions.iter().flat_map(|v| {
            let names = o
                .template
                .roles
                .get(&v.role)
                .and_then(|r| r.names)
                .unwrap_or(false);
            v.structures
                .iter()
                .map(move |m| (m, Some(v.role.as_str()), names, true))
        }))
}

/// The wires drawn: the line's in its role, the versions' in theirs.
fn all_wires<'a>(
    d: &'a RouteDrawing,
    o: &'a Options,
) -> impl Iterator<Item = (&'a crate::Trace, Option<&'a str>, Option<&'a str>)> {
    // The versions first: under the line's own wires.
    d.versions
        .iter()
        .flat_map(|v| {
            v.wires
                .iter()
                .map(move |w| (w, Some(v.role.as_str()), Some(v.label.as_str())))
        })
        .chain(d.wires.iter().map(|w| (w, o.role.as_deref(), None)))
}

/// The pen of wire `w` in `role` (with its version's `label`), its legend
/// entries added to frame `f`'s legend: its kind or phase and role, and
/// its weather case's dash.
fn wire_pen(
    f: &Frame,
    o: &Options,
    w: &crate::Trace,
    role: Option<&str>,
    label: Option<&str>,
) -> Pen {
    let (p, words) = crate::style::wire_pen(&o.template, w, role);
    let p = o.template.pen(Layer::Wires, p);
    let mut legend = f.legend.borrow_mut();
    let labels = &o.template.legend;
    let words = match label {
        Some(l) if !l.is_empty() => labels.label("version", &[("label", l), ("words", &words)]),
        _ => words,
    };
    legend.add(
        Sample::Line(Pen {
            dash: None,
            ..p.clone()
        }),
        words,
    );
    if let Some(d) = &w.dash {
        let case = if w.case.is_empty() {
            "another weather case"
        } else {
            w.case.as_str()
        };
        legend.add(
            Sample::Line(Pen::new(Color(0x44, 0x44, 0x44), 0.3).dashed(d)),
            labels.label("weather_case", &[("case", case)]),
        );
    } else if !w.case.is_empty() {
        legend.add(
            Sample::Line(Pen::new(Color(0x44, 0x44, 0x44), 0.3)),
            labels.label("weather_case", &[("case", &w.case)]),
        );
    }
    p
}

/// The content of the plan of frame `f`: what is clipped to the panel,
/// the structures at the frame's ends (drawn whole), and the names
/// (drawn whole outside the clip).
fn plan_content(d: &RouteDrawing, o: &Options, f: &Frame) -> (Vec<Item>, Vec<Item>, Vec<Item>) {
    let mut plan = Vec::new();
    let mut ends = Vec::new();
    let mut names = Vec::new();
    let labels = &o.template.legend;
    let axis = centreline_pen(o);
    plan.push(path(
        Layer::Centreline,
        vec![[f.x(0.0), f.plan_y(0.0)], [f.x(d.length), f.plan_y(0.0)]],
        axis.clone(),
        None,
    ));
    f.legend
        .borrow_mut()
        .add(Sample::Line(axis), labels.label("axis", &[]));
    // LiDAR points seen from above, under everything else.
    if o.template.lidar.plan {
        plan.extend(lidar_dots(o, f, d, |q| Some([f.x(q[0]), f.plan_y(q[1])])));
    }
    // The corridor's edges, each in its pen (near, far).
    if let Some(w) = o.corridor.filter(|w| *w > 0.0) {
        let label = crate::template::fill(
            o.template
                .corridor
                .label
                .as_deref()
                .unwrap_or("corridor ±{width} m"),
            &|n, _| (n == "width").then(|| crate::template::format_number(w, "")),
        );
        let near = o.template.corridor.near_offset(w);
        for offset in [near, -near] {
            let (p, _) = corridor_edge(o, offset);
            let y = f.plan_y(offset);
            plan.push(path(
                Layer::Corridor,
                vec![[f.x(0.0), y], [f.x(d.length), y]],
                p.clone(),
                Some(label.clone()),
            ));
            names.push(text(
                Layer::Corridor,
                [f.left + 1.0, y + if offset < 0.0 { -0.8 } else { 2.4 }],
                2.0,
                Anchor::Start,
                label.clone(),
            ));
        }
        corridor_legend(o, f, d, w, &label);
    }
    // Isoclearance lines under the wires.
    for w in &d.isoclearance {
        plan.push(path(
            Layer::Isoclearance,
            w.points
                .iter()
                .map(|p| [f.x(p[0]), f.plan_y(p[1])])
                .collect(),
            lp(o, Layer::Isoclearance, &w.color, 0.18),
            Some(w.label.clone()),
        ));
        f.legend.borrow_mut().add(
            Sample::Line(pen(&w.color, 0.3)),
            labels.label(
                "isoclearance",
                &[("level", w.label.trim_start_matches("isoclearance "))],
            ),
        );
    }
    for (w, role, label) in all_wires(d, o) {
        let layer = if label.is_some() {
            Layer::Versions
        } else {
            Layer::Wires
        };
        plan.push(path(
            layer,
            w.points
                .iter()
                .map(|p| [f.x(p[0]), f.plan_y(p[1])])
                .collect(),
            wire_pen(f, o, w, role, label),
            Some(w.label.clone()),
        ));
    }
    // A version's label at its first wire in the plan.
    for v in &d.versions {
        if v.label.is_empty() {
            continue;
        }
        if let Some(p) = v
            .wires
            .iter()
            .flat_map(|w| w.points.iter())
            .filter(|p| p[0] >= f.s0 && p[0] <= f.s1)
            .min_by(|a, b| a[0].total_cmp(&b[0]))
        {
            let mut t = text(
                Layer::Versions,
                [f.x(p[0]) + 1.0, f.plan_y(p[1]) - 1.0],
                2.0,
                Anchor::Start,
                v.label.clone(),
            );
            if let Item::Text(x) = &mut t {
                x.color = hex("#555");
            }
            plan.push(t);
        }
    }
    for (m, role, named, version) in all_structures(d, o) {
        let (cx, cy) = (f.x(m.station), f.plan_y(m.offset));
        let out = if f.at_end(m) { &mut ends } else { &mut plan };
        let layer = if version {
            Layer::Versions
        } else {
            Layer::Structures
        };
        let symbol = crate::style::structure_pen(&o.template, role, lp(o, layer, "#333", 0.3));
        // The symbol at level 1; members and poles draw the others.
        if !m.has_body() {
            out.push(rect(layer, cx - 1.0, cy - 1.0, 2.0, 2.0, symbol.clone()));
            f.legend.borrow_mut().add(
                Sample::Square(symbol.clone()),
                match role
                    .and_then(|r| o.template.roles.get(r))
                    .and_then(|r| r.label.clone())
                {
                    Some(l) => labels.label("structure_role", &[("role", &l)]),
                    None => labels.label("structure", &[]),
                },
            );
        }
        // Stick drawings seen from above; members (levels 2 and 3): the
        // footprint and the crossarms.
        let (layer, lines) = match (version, m.members.is_empty()) {
            (true, true) => (Layer::Versions, &m.sticks),
            (true, false) => (Layer::Versions, &m.members),
            (false, true) => (Layer::Structures, &m.sticks),
            (false, false) => (Layer::Members, &m.members),
        };
        let thin = crate::style::structure_pen(&o.template, role, lp(o, layer, "#555", 0.25));
        for stick in lines {
            out.push(path(
                layer,
                stick.iter().map(|p| [f.x(p[0]), f.plan_y(p[1])]).collect(),
                thin.clone(),
                None,
            ));
        }
        // A pole's outline at the ground line and at its top.
        for ring in pole_ends(&m.pole) {
            out.push(closed(
                Layer::Members,
                ring.iter().map(|p| [f.x(p[0]), f.plan_y(p[1])]).collect(),
                thin.clone(),
            ));
        }
        // Its name beside the symbol, above and right of it.
        if named {
            names.push(text(
                Layer::Structures,
                [cx + 1.8, cy - 1.8],
                2.5,
                Anchor::Start,
                m.id.clone(),
            ));
        }
    }
    for (p, label) in &d.markers {
        plan.push(marker([f.x(p[0]), f.plan_y(p[1])], 1.2, label));
        f.legend.borrow_mut().add(
            Sample::Circle(pen("#d00", 0.5)),
            labels.label("clearance_violation", &[]),
        );
    }
    (plan, ends, names)
}

/// The part of plan rectangle `r` (paper, mm) the backgrounds fill: with
/// a corridor, only between its edges (the area shown, "Bereich"), else
/// the whole rectangle.
fn background_rect(o: &Options, f: &Frame, r: [f64; 4]) -> [f64; 4] {
    let Some(w) = o.corridor.filter(|w| *w > 0.0) else {
        return r;
    };
    let (a, b) = (f.plan_y(w), f.plan_y(-w));
    let top = a.min(b).max(r[1]);
    let bottom = a.max(b).min(r[1] + r[3]);
    [r[0], top, r[2], (bottom - top).max(0.0)]
}

/// Grids, panels and content of one frame, clipped to its panels (the
/// structures at a sheet's ends drawn whole; with `beyond_match = "faded"`
/// the next sheet's content faded to the paper's edge); the structures'
/// elevations are drawn by [`draw_elevations`], after the match lines.
fn draw(items: &mut Vec<Item>, d: &RouteDrawing, o: &Options, f: &Frame) {
    let (s0, s1) = (f.s0, f.s1);
    let grid = lp(o, Layer::Grid, "#ddd", 0.2);
    let prof_bottom = f.prof_top + f.prof_h;
    // The plan's backgrounds (the project's orthophotos and drawings, when
    // asked for) under everything else.
    if f.plan {
        let rect = background_rect(o, f, [f.left, f.plan_top, f.width(), f.plan_h]);
        let mut under: Vec<Item> = crate::background::photo_item(d, o, f, rect)
            .into_iter()
            .collect();
        under.extend(crate::background::drawing_items(d, o, f, rect));
        if !under.is_empty() {
            items.push(Item::Clip(Clip { rect, items: under }));
        }
    }
    // Station grid in both panels, labels under the profile (and its
    // bands).
    let s_step = nice_step((s1 - s0).max(1.0), (f.width() / 40.0).max(1.0));
    for s in ticks(s0, s1, s_step) {
        let x = f.x(s);
        if f.plan {
            items.push(path(
                Layer::Grid,
                vec![[x, f.plan_top], [x, f.plan_top + f.plan_h]],
                grid.clone(),
                None,
            ));
        }
        items.push(path(
            Layer::Grid,
            vec![[x, f.prof_top], [x, prof_bottom]],
            grid.clone(),
            None,
        ));
        if o.template.layout.station_labels {
            items.push(text(
                Layer::Labels,
                [x, prof_bottom + f.bands_h + 4.0],
                2.5,
                Anchor::Middle,
                format_station(s),
            ));
        }
    }
    // Height grid and labels per part, inside the panel only.
    for (k, p) in f.parts.iter().enumerate() {
        let (xa, xb) = (f.x(p.s0), f.x(p.s1));
        for z in ticks(p.z0, p.z1, p.step) {
            let y = f.y_in(p, z);
            if y < f.prof_top - 1e-6 || y > prof_bottom + 1e-6 {
                continue;
            }
            items.push(path(
                Layer::Grid,
                vec![[xa, y], [xb, y]],
                grid.clone(),
                None,
            ));
            // Labels need 2 mm below the panel's top.
            if y < f.prof_top + 2.0 {
                continue;
            }
            items.push(if k == 0 {
                text(
                    Layer::Labels,
                    [f.left - 1.5, (y + 0.9).min(prof_bottom)],
                    2.5,
                    Anchor::End,
                    format!("{z:.0}"),
                )
            } else {
                text(
                    Layer::Labels,
                    [xa + 1.5, y - 0.6],
                    2.0,
                    Anchor::Start,
                    format!("{z:.0}"),
                )
            });
        }
        // A datum step: a double line across the panel.
        if k > 0 {
            let step = lp(o, Layer::Panels, "#555", 0.3);
            for dx in [-0.4, 0.4] {
                items.push(path(
                    Layer::Panels,
                    vec![[xa + dx, f.prof_top], [xa + dx, prof_bottom]],
                    step.clone(),
                    Some(format!("datum step at {}", format_station(p.s0))),
                ));
            }
            f.legend.borrow_mut().add(
                Sample::Line(step),
                o.template.legend.label("datum_step", &[]),
            );
        }
    }
    let panel = lp(o, Layer::Panels, "#888", 0.3);
    if f.plan {
        items.push(rect(
            Layer::Panels,
            f.left,
            f.plan_top,
            f.width(),
            f.plan_h,
            panel.clone(),
        ));
    }
    items.push(rect(
        Layer::Panels,
        f.left,
        f.prof_top,
        f.width(),
        f.prof_h,
        panel,
    ));
    // The panels' names above them, clear of what they hold (the
    // template's words).
    let lay = &o.template.layout;
    if f.plan && !lay.plan_title.is_empty() {
        items.push(bold(text(
            Layer::Labels,
            [f.left, f.plan_top - 1.5],
            3.0,
            Anchor::Start,
            lay.plan_title.clone(),
        )));
    }
    if !lay.profile_title.is_empty() {
        items.push(bold(text(
            Layer::Labels,
            [f.left, f.prof_top - 1.5],
            3.0,
            Anchor::Start,
            lay.profile_title.clone(),
        )));
    }

    // Names of the structures shown are written whole, outside the clip
    // regions (a name at a sheet's end is not cut in half).
    let plan_rect = [f.left, f.plan_top, f.width(), f.plan_h];
    let prof_rect = [f.left, f.prof_top, f.width(), f.prof_h];
    // Clip regions widened for the structures at the ends.
    let widen = all_structures(d, o)
        .filter(|(m, ..)| f.at_end(m))
        .map(|(m, ..)| half_width_mm(m, f.h))
        .fold(0.0, f64::max);
    let wide = |r: [f64; 4]| [r[0] - widen, r[1], r[2] + 2.0 * widen, r[3]];
    let inside_or =
        |names: &mut Vec<Item>, clipped: &mut Vec<Item>, item: Item, rect: [f64; 4]| match &item {
            Item::Text(t)
                if crate::model::inside(
                    [t.at[0], t.at[1]],
                    [rect[0] - 1e-6, rect[1], rect[2] + 2e-6, rect[3]],
                ) =>
            {
                names.push(item)
            }
            _ => clipped.push(item),
        };
    let mut names = Vec::new();
    // Beyond the match line, faded (the paper's width).
    let faded =
        o.template.layout.beyond_match == crate::template::Beyond::Faded && f.paper_end > s1 + 1e-6;
    let beyond = |r: [f64; 4]| [r[0] + r[2], r[1], (f.paper_end - s1) * f.h, r[3]];
    // Plan.
    if f.plan {
        let (mut plan, ends, plan_names) = plan_content(d, o, f);
        for n in plan_names {
            inside_or(&mut names, &mut plan, n, plan_rect);
        }
        if faded {
            let (more, more_ends, _) = plan_content(d, o, f);
            // (The drawings under it; the orthophoto ends at the match
            // line.)
            let mut all =
                crate::background::drawing_items(d, o, f, background_rect(o, f, beyond(plan_rect)));
            all.extend(more);
            all.extend(more_ends);
            items.push(Item::Clip(Clip {
                rect: beyond(plan_rect),
                items: fade(all),
            }));
        }
        items.push(Item::Clip(Clip {
            rect: plan_rect,
            items: plan,
        }));
        if !ends.is_empty() {
            items.push(Item::Clip(Clip {
                rect: wide(plan_rect),
                items: ends,
            }));
        }
    }

    // Profile: each part with its own heights.
    let curves = conductor_curves(d, o);
    for p in &f.parts {
        let (content, ends) = profile_part(d, o, f, p, &curves);
        let rect = [f.x(p.s0), f.prof_top, (p.s1 - p.s0) * f.h, f.prof_h];
        items.push(Item::Clip(Clip {
            rect,
            items: content,
        }));
        if !ends.is_empty() {
            items.push(Item::Clip(Clip {
                rect: wide(prof_rect),
                items: ends,
            }));
        }
    }
    if faded && let Some(last) = f.parts.last() {
        let (content, ends) = profile_part(d, o, f, last, &curves);
        let mut all = content;
        all.extend(ends);
        items.push(Item::Clip(Clip {
            rect: beyond(prof_rect),
            items: fade(all),
        }));
    }
    // The curves' labels: where the template says, clear of the
    // structures, the wires and the ground (`label_at = "auto"`).
    let obstacles = profile_obstacles(d, o, f, &curves);
    for c in &curves {
        if let Some(at) = curve_label_at(o, f, c, &obstacles) {
            let mut t = text(
                Layer::ConductorClearance,
                at,
                2.0,
                Anchor::Middle,
                c.label.clone(),
            );
            if let Item::Text(x) = &mut t {
                x.color = curve_pen(o).color;
            }
            names.push(t);
        }
    }
    // Structures' names over their tops, inside the panel (a top cut at
    // the panel's top: a break mark there).
    let mut prof_names = Vec::new();
    for (m, _, named, _) in all_structures(d, o) {
        if !named || m.station < s0 - 1e-6 || m.station > s1 + 1e-6 {
            continue;
        }
        let p = f.part_at(m.station);
        let cx = f.x(m.station);
        let top_y = f.y_in(p, structure_top(m));
        let cut = top_y < f.prof_top + LABEL_ROOM_MM - 1e-6;
        let y = top_y.max(f.prof_top + LABEL_ROOM_MM);
        let anchor = if cx < f.left + 6.0 {
            Anchor::Start
        } else if cx > f.left + f.width() - 6.0 {
            Anchor::End
        } else {
            Anchor::Middle
        };
        let dx = match anchor {
            Anchor::Start => 0.8,
            Anchor::End => -0.8,
            Anchor::Middle => 0.0,
        };
        let mut sink = Vec::new();
        let name = text(
            Layer::Structures,
            [cx + dx, y - 4.0],
            2.5,
            anchor,
            m.id.clone(),
        );
        inside_or(&mut prof_names, &mut sink, name, prof_rect);
        if let Item::Text(mut t) = text(
            Layer::Structures,
            [cx + dx, y - 1.5],
            2.0,
            anchor,
            format_station(m.station),
        ) {
            t.color = hex("#555");
            inside_or(&mut prof_names, &mut sink, Item::Text(t), prof_rect);
        }
        if cut && top_y < f.prof_top {
            // Two short strokes across the mast at the panel's top.
            for k in [0.0, 1.0] {
                prof_names.push(path(
                    Layer::Structures,
                    vec![
                        [cx - 1.2, f.prof_top + 0.8 + k],
                        [cx + 1.2, f.prof_top + 0.2 + k],
                    ],
                    lp(o, Layer::Structures, "#333", 0.25),
                    Some(format!("{} cut at the panel's top", m.id)),
                ));
            }
        }
    }
    items.extend(prof_names);
    items.extend(names);
}

/// Items with their colours faded to 30 % (what lies beyond the match
/// line).
fn fade(items: Vec<Item>) -> Vec<Item> {
    use crate::style::faded;
    items
        .into_iter()
        .map(|i| match i {
            Item::Path(mut p) => {
                p.pen.color = faded(p.pen.color, 0.3);
                Item::Path(p)
            }
            Item::Text(mut t) => {
                t.color = faded(t.color, 0.3);
                Item::Text(t)
            }
            Item::Circle(mut c) => {
                if let Some(s) = &mut c.stroke {
                    s.color = faded(s.color, 0.3);
                }
                c.fill = c.fill.map(|x| faded(x, 0.3));
                Item::Circle(c)
            }
            Item::Clip(mut c) => {
                c.items = fade(c.items);
                Item::Clip(c)
            }
            other => other,
        })
        .collect()
}

/// The pen of the lines below the lowest conductor.
fn curve_pen(o: &Options) -> Pen {
    o.template
        .conductor_clearance
        .pen
        .over(lp(o, Layer::ConductorClearance, "#a01818", 0.3).dashed("5 1 1 1 1 1"))
}

/// What a label in the profile stays clear of, on paper (mm): each
/// structure's box (its width, from its base to the room for its name
/// over its top) and the points of the wires, the ground lines and the
/// lines below the lowest conductor (every 1 mm along them), in columns
/// 2 mm wide.
struct Obstacles {
    boxes: Vec<[f64; 4]>,
    columns: std::collections::HashMap<i64, Vec<[f64; 2]>>,
}

impl Obstacles {
    /// How many structures and points lie in `r` (left, top, width,
    /// height).
    fn hits(&self, r: [f64; 4]) -> (usize, usize) {
        let boxes = self
            .boxes
            .iter()
            .filter(|b| {
                b[0] < r[0] + r[2] && r[0] < b[0] + b[2] && b[1] < r[1] + r[3] && r[1] < b[1] + b[3]
            })
            .count();
        let (a, b) = (
            (r[0] / 2.0).floor() as i64,
            ((r[0] + r[2]) / 2.0).floor() as i64,
        );
        let points = (a..=b)
            .filter_map(|k| self.columns.get(&k))
            .flatten()
            .filter(|p| crate::model::inside(**p, r))
            .count();
        (boxes, points)
    }
}

fn profile_obstacles(
    d: &RouteDrawing,
    o: &Options,
    f: &Frame,
    curves: &[crate::curves::Curve],
) -> Obstacles {
    let within = |s: f64| s >= f.s0 - 1e-6 && s <= f.s1 + 1e-6;
    let y = |s: f64, z: f64| f.y_in(f.part_at(s), z);
    let mut boxes = Vec::new();
    for (m, ..) in all_structures(d, o) {
        if !within(m.station) {
            continue;
        }
        let hw = half_width_mm(m, f.h);
        let top = y(m.station, structure_top(m)) - LABEL_ROOM_MM;
        let base = y(m.station, m.base);
        boxes.push([f.x(m.station) - hw, top, 2.0 * hw, base - top]);
    }
    let mut columns: std::collections::HashMap<i64, Vec<[f64; 2]>> = Default::default();
    let mut add = |run: Vec<[f64; 2]>| {
        let mut put = |p: [f64; 2]| {
            columns
                .entry((p[0] / 2.0).floor() as i64)
                .or_default()
                .push(p)
        };
        for w in run.windows(2) {
            let n = ((w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]).ceil() as usize).clamp(1, 500);
            for k in 0..n {
                let t = k as f64 / n as f64;
                put([
                    w[0][0] + t * (w[1][0] - w[0][0]),
                    w[0][1] + t * (w[1][1] - w[0][1]),
                ]);
            }
        }
        if let Some(p) = run.last() {
            put(*p);
        }
    };
    let paper = |s: f64, z: f64| [f.x(s), y(s, z)];
    for (w, ..) in all_wires(d, o) {
        add(w
            .points
            .iter()
            .filter(|p| within(p[0]))
            .map(|p| paper(p[0], p[2]))
            .collect());
    }
    let clearance = o.clearance.unwrap_or(0.0);
    for run in d
        .ground
        .iter()
        .chain(d.sides.iter().flat_map(|(_, r)| r))
        .chain(d.corridor_ground.iter().flat_map(|(_, r)| r))
    {
        add(run
            .iter()
            .filter(|p| within(p[0]))
            .map(|p| paper(p[0], p[1]))
            .collect());
    }
    if clearance > 0.0 {
        for run in &d.ground {
            add(run
                .iter()
                .filter(|p| within(p[0]))
                .map(|p| paper(p[0], p[1] + clearance))
                .collect());
        }
    }
    for c in curves {
        for run in &c.runs {
            add(run
                .iter()
                .filter(|p| within(p[0]))
                .map(|p| paper(p[0], p[1]))
                .collect());
        }
    }
    Obstacles { boxes, columns }
}

/// Where the label of line `c` below the lowest conductor goes (its
/// text's anchor, centred): `label_at = "middle"` the sheet's middle just
/// above the line; else, every 2 mm along the sheet, just above or just
/// below the line, the place hitting the fewest structures, then the
/// fewest wire and ground points, nearest the sheet's middle (`"auto"`),
/// start or end — above before below. None: no place in the panel.
fn curve_label_at(
    o: &Options,
    f: &Frame,
    c: &crate::curves::Curve,
    obstacles: &Obstacles,
) -> Option<[f64; 2]> {
    let size = 2.0;
    let prof_bottom = f.prof_top + f.prof_h;
    let within = |s: f64| s >= f.s0 - 1e-6 && s <= f.s1 + 1e-6;
    let mode = o.template.conductor_clearance.label_at.as_str();
    if mode == "middle" {
        let mid = 0.5 * (f.s0 + f.s1);
        let q = c
            .runs
            .iter()
            .flatten()
            .filter(|q| within(q[0]))
            .min_by(|a, b| (a[0] - mid).abs().total_cmp(&(b[0] - mid).abs()))?;
        let y = f.y_in(f.part_at(q[0]), q[1]) - 1.0;
        return (y > f.prof_top + 3.0 && y < prof_bottom).then_some([f.x(q[0]), y]);
    }
    let pts: Vec<[f64; 2]> = c
        .runs
        .iter()
        .flatten()
        .filter(|q| within(q[0]))
        .map(|q| [f.x(q[0]), f.y_in(f.part_at(q[0]), q[1])])
        .collect();
    let w = crate::pdf::text_width(&c.label, size, false);
    let target = match mode {
        "start" => f.left,
        "end" => f.left + f.width(),
        _ => f.left + 0.5 * f.width(),
    };
    let (lo, hi) = (f.left + 0.5 * w + 1.0, f.left + f.width() - 0.5 * w - 1.0);
    let mut best: Option<(f64, [f64; 2])> = None;
    let mut x = lo;
    while x <= hi + 1e-9 {
        let span = pts
            .iter()
            .filter(|p| p[0] >= x - 0.5 * w && p[0] <= x + 0.5 * w)
            .fold([f64::INFINITY, f64::NEG_INFINITY], |r, p| {
                [r[0].min(p[1]), r[1].max(p[1])]
            });
        if span[0].is_finite() {
            for (below, base) in [(false, span[0] - 1.0), (true, span[1] + 2.6)] {
                let r = [x - 0.5 * w - 0.5, base - 1.7, w + 1.0, 2.3];
                if r[1] < f.prof_top + 0.5 || r[1] + r[3] > prof_bottom - 0.5 {
                    continue;
                }
                let (boxes, points) = obstacles.hits(r);
                let cost = 1000.0 * boxes as f64
                    + 20.0 * points.min(40) as f64
                    + (x - target).abs() / f.width().max(1.0)
                    + if below { 0.3 } else { 0.0 };
                if best.is_none_or(|b| cost < b.0) {
                    best = Some((cost, [x, base]));
                }
            }
        }
        x += 2.0;
    }
    best.map(|b| b.1)
}

/// The lines below the lowest conductor the options ask for (none unless
/// the template shows them).
fn conductor_curves(d: &RouteDrawing, o: &Options) -> Vec<crate::curves::Curve> {
    let spec = &o.template.conductor_clearance;
    if !spec.show || o.conductor_clearance.is_empty() {
        return Vec::new();
    }
    let curves = crate::curves::curves(d, &o.conductor_clearance, &spec.circuits, &spec.label).0;
    if spec.envelope {
        // One line for the whole line: the lowest of them.
        return crate::curves::envelope(&curves, &spec.envelope_label)
            .into_iter()
            .collect();
    }
    curves
}

/// What to warn about the lines below the lowest conductor.
pub(crate) fn curve_warnings(d: &RouteDrawing, o: &Options) -> Vec<String> {
    let spec = &o.template.conductor_clearance;
    if !spec.show || o.conductor_clearance.is_empty() {
        return Vec::new();
    }
    crate::curves::curves(d, &o.conductor_clearance, &spec.circuits, &spec.label).1
}

/// The profile's content of part `p` of frame `f`: what is clipped to the
/// part, and the structures at the frame's ends (drawn whole).
fn profile_part(
    d: &RouteDrawing,
    o: &Options,
    f: &Frame,
    p: &Part,
    curves: &[crate::curves::Curve],
) -> (Vec<Item>, Vec<Item>) {
    let y = |z: f64| f.y_in(p, z);
    let mut prof = Vec::new();
    let mut ends = Vec::new();
    // LiDAR points by station and height, under everything else (the
    // legend's entries made as they are drawn).
    if o.template.lidar.profile {
        prof.extend(lidar_dots(o, f, d, |q| Some([f.x(q[0]), y(q[2])])));
    }
    let mut legend = f.legend.borrow_mut();
    let labels = &o.template.legend;
    for (offset, runs) in &d.sides {
        let side = lp(o, Layer::GroundSide, "#b08850", 0.25).dashed("1.5 1");
        for run in runs {
            prof.push(path(
                Layer::GroundSide,
                run.iter().map(|q| [f.x(q[0]), y(q[1])]).collect(),
                side.clone(),
                Some(format!("ground at offset {offset:+.1} m")),
            ));
        }
        legend.add(
            Sample::Line(side),
            labels.label("ground_side", &[("offset", &format!("{offset:+.1}"))]),
        );
    }
    // The ground along the corridor's edges, each in its edge's pen (the
    // legend's corridor entries name them).
    for (offset, runs) in &d.corridor_ground {
        let (p, near) = corridor_edge(o, *offset);
        for run in runs {
            prof.push(path(
                Layer::GroundSide,
                run.iter().map(|q| [f.x(q[0]), y(q[1])]).collect(),
                p.clone(),
                Some(format!(
                    "ground on the corridor's {} edge ({offset:+.1} m)",
                    if near { "near" } else { "far" }
                )),
            ));
        }
    }
    let instead = o.template.conductor_clearance.mode == "instead" && !curves.is_empty();
    // The ground on the axis: always solid (the template may only leave
    // it out, `[centreline] ground = false`).
    let ground = Pen {
        dash: None,
        ..lp(o, Layer::Ground, "#7a4a1a", 0.45)
    };
    for run in &d.ground {
        if o.template.centreline.ground {
            prof.push(path(
                Layer::Ground,
                run.iter().map(|q| [f.x(q[0]), y(q[1])]).collect(),
                ground.clone(),
                Some("ground (centreline)".into()),
            ));
            legend.add(Sample::Line(ground.clone()), labels.label("ground", &[]));
        }
        if let Some(c) = o.clearance.filter(|_| !instead) {
            let line = lp(o, Layer::Clearance, "#d22", 0.3).dashed("3 1.5");
            prof.push(path(
                Layer::Clearance,
                run.iter().map(|q| [f.x(q[0]), y(q[1] + c)]).collect(),
                line.clone(),
                Some(format!("ground + {c} m")),
            ));
            legend.add(
                Sample::Line(line),
                labels.label(
                    "clearance_line",
                    &[("distance", &crate::template::format_number(c, ""))],
                ),
            );
        }
    }
    let cp = curve_pen(o);
    for c in curves {
        let text = c.legend_with(&o.template.legend);
        for run in &c.runs {
            prof.push(path(
                Layer::ConductorClearance,
                run.iter().map(|q| [f.x(q[0]), y(q[1])]).collect(),
                cp.clone(),
                Some(text.clone()),
            ));
        }
        legend.add(Sample::Line(cp.clone()), text);
    }
    drop(legend);
    for (m, role, _, version) in all_structures(d, o) {
        let cx = f.x(m.station);
        let out = if f.at_end(m) { &mut ends } else { &mut prof };
        let own = if version {
            Layer::Versions
        } else {
            Layer::Structures
        };
        let mast = crate::style::structure_pen(&o.template, role, lp(o, own, "#333", 0.6));
        // The symbol at level 1; members and poles draw the others.
        if !m.has_body() {
            out.push(path(
                own,
                vec![[cx, y(m.base)], [cx, y(m.top)]],
                mast.clone(),
                None,
            ));
        }
        for a in &m.attachments {
            out.push(Item::Circle(Circle {
                layer: own,
                centre: [f.x(a[0]), y(a[2])],
                radius: 0.4,
                stroke: None,
                fill: Some(mast.color),
                title: None,
            }));
        }
        // Stick drawings seen along the route; members (levels 2 and 3):
        // the silhouette projected on the profile plane.
        let (layer, lines) = match (version, m.members.is_empty()) {
            (_, true) => (own, &m.sticks),
            (true, false) => (own, &m.members),
            (false, false) => (Layer::Members, &m.members),
        };
        let thin = crate::style::structure_pen(&o.template, role, lp(o, layer, "#555", 0.3));
        for stick in lines {
            out.push(path(
                layer,
                stick.iter().map(|q| [f.x(q[0]), y(q[2])]).collect(),
                thin.clone(),
                Some(m.id.clone()),
            ));
        }
        // A pole: its silhouette by height.
        let outline = silhouette(&m.pole, 0);
        if !outline.is_empty() {
            out.push(Item::Path(Path {
                layer: if version { own } else { Layer::Members },
                points: outline.iter().map(|q| [f.x(q[0]), y(q[1])]).collect(),
                closed: true,
                pen: thin.clone(),
                title: Some(m.id.clone()),
            }));
        }
        // Its strings: from where each hangs to its wire point, a little
        // heavier than the members.
        let string_pen = crate::style::structure_pen(&o.template, role, lp(o, own, "#333", 0.4));
        for s in &m.strings {
            out.push(path(
                own,
                vec![
                    [f.x(s.top[0]), y(s.top[2])],
                    [f.x(s.bottom[0]), y(s.bottom[2])],
                ],
                string_pen.clone(),
                Some(format!("{} string {}", m.id, s.label)),
            ));
        }
    }
    for (w, role, label) in all_wires(d, o) {
        let layer = if label.is_some() {
            Layer::Versions
        } else {
            Layer::Wires
        };
        prof.push(path(
            layer,
            w.points.iter().map(|q| [f.x(q[0]), y(q[2])]).collect(),
            wire_pen(f, o, w, role, label),
            Some(w.label.clone()),
        ));
    }
    for (q, label) in &d.markers {
        prof.push(marker([f.x(q[0]), y(q[2])], 1.2, label));
    }
    // The sag of each span, under each circuit's lowest wire at mid-span
    // (`[sag_labels]`).
    let sl = &o.template.sag_labels;
    if sl.show {
        let earth = o.template.legend.label("earth_wires", &[]);
        let color = Color::hex_or_black(&o.template.text.colour);
        // The labels written (their boxes): one that would touch another is
        // put under it.
        let mut placed: Vec<[f64; 4]> = Vec::new();
        for sp in d.sags.iter().filter(|sp| {
            let at = 0.5 * (sp.from + sp.to);
            at >= p.s0 && at <= p.s1
        }) {
            for (circuit, sag, mid) in &sp.sags {
                let name = circuit.clone().unwrap_or_else(|| earth.clone());
                if !sl.circuits.is_empty() && !sl.circuits.contains(&name) {
                    continue;
                }
                let text = crate::template::fill(&sl.label, &|n, _| match n {
                    "sag" => Some(format!("{sag:.2}")),
                    "circuit" => Some(name.clone()),
                    _ => None,
                });
                let size = sl.size.0;
                let w = 0.6 * size * text.chars().count() as f64;
                let x = f.x(mid[0]);
                let mut base = y(mid[1]) + sl.gap.0 + size;
                for _ in 0..8 {
                    let b = [x - w / 2.0, base - size, x + w / 2.0, base + 0.2 * size];
                    let touches = placed
                        .iter()
                        .any(|q| b[0] < q[2] && b[2] > q[0] && b[1] < q[3] && b[3] > q[1]);
                    if !touches {
                        placed.push(b);
                        break;
                    }
                    base += 1.25 * size;
                }
                prof.push(Item::Text(Text {
                    layer: Layer::Wires,
                    at: [x, base],
                    text,
                    size: sl.size.0,
                    anchor: Anchor::Middle,
                    bold: false,
                    color,
                }));
            }
        }
    }
    (prof, ends)
}

/// A closed path.
fn closed(layer: Layer, points: Vec<[f64; 2]>, pen: Pen) -> Item {
    Item::Path(Path {
        layer,
        points,
        closed: true,
        pen,
        title: None,
    })
}

/// A pole's outline at the ground line and at its top (the lowest and the
/// highest of its outlines), without the repeated closing point.
fn pole_ends(pole: &[Vec<[f64; 3]>]) -> Vec<Vec<[f64; 3]>> {
    let open = |r: &Vec<[f64; 3]>| r[..r.len().saturating_sub(1)].to_vec();
    match pole {
        [] => Vec::new(),
        [one] => vec![open(one)],
        [first, .., last] => vec![open(first), open(last)],
    }
}

/// A pole's silhouette by height seen across axis `axis` of its outlines
/// (0: along the route, the profile; 1: across it, an elevation): at each
/// height the outline's least and greatest coordinate, up one side and
/// down the other, as (coordinate, z); empty without a pole.
fn silhouette(pole: &[Vec<[f64; 3]>], axis: usize) -> Vec<[f64; 2]> {
    let spans: Vec<(f64, f64, f64)> = pole
        .iter()
        .filter(|r| !r.is_empty())
        .map(|r| {
            let (lo, hi) = r
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| {
                    (a.min(p[axis]), b.max(p[axis]))
                });
            (lo, hi, r[0][2])
        })
        .collect();
    if spans.len() < 2 {
        return Vec::new();
    }
    spans
        .iter()
        .map(|s| [s.0, s.2])
        .chain(spans.iter().rev().map(|s| [s.1, s.2]))
        .collect()
}

/// One structure's elevation on the plan ([`crate::Elevations`]), in the
/// route's coordinates (station, offset), m: what the sheets draw on
/// paper, and the app's sheet preview over its plan.
#[derive(Debug, Clone, PartialEq)]
pub struct ElevationDrawing {
    /// The structure.
    pub id: String,
    /// Its station.
    pub station: f64,
    /// Its lines (station, offset), and whether each is closed (a pole's
    /// silhouette).
    pub lines: Vec<(Vec<[f64; 2]>, bool)>,
    /// The leader from where it stands to the elevation's base (none:
    /// under half a millimetre on paper).
    pub leader: Option<[[f64; 2]; 2]>,
    /// Its labels (number, type, height above its top; the chain of
    /// heights on one side, the arms' heights and the vertical strings'
    /// lengths on the other): where (station, offset), the text, and
    /// where the point is along the text.
    pub labels: Vec<([f64; 2], String, Anchor)>,
    /// Its dimension lines and the leaders to labels moved clear of each
    /// other (station, offset), drawn thin.
    pub dims: Vec<Vec<[f64; 2]>>,
    /// The role of the version it belongs to (none: the line's own).
    pub role: Option<String>,
}

/// The structures' elevations on the plan of `d` with options `o`
/// ([`crate::Elevations`]; none when off or without a plan): every
/// structure drawn along the line ([`RouteDrawing::shown`]), those of the
/// other versions too, seen along the line — its members, its pole's
/// silhouette, else its stick drawing (a mast line without one) — true
/// shape at the elevations' scale (the horizontal one unless given), its
/// base at its station `offset_mm` on paper above or below the line, its
/// height at `rotation_deg` to the line pointing away from it (below: the
/// same turned half a turn), with the labels asked for, in the route's
/// (station, offset), m.
pub fn elevations(d: &RouteDrawing, o: &Options) -> Vec<ElevationDrawing> {
    if !o.plan || o.elevations.place == ElevationPlace::Off {
        return Vec::new();
    }
    elevations_of(&d.shown(o.plan_half_width), o)
}

/// [`elevations`] of what is drawn already ([`RouteDrawing::shown`]).
fn elevations_of(d: &RouteDrawing, o: &Options) -> Vec<ElevationDrawing> {
    let e = o.elevations;
    let side = if e.place == ElevationPlace::Below {
        -1.0
    } else {
        1.0
    };
    let h = 1000.0 / o.horizontal_scale; // mm per m along the route
    let he = 1000.0 / e.scale.unwrap_or(o.horizontal_scale); // the elevations'
    let plan_k = 1000.0 / o.plan_offset_scale.unwrap_or(o.horizontal_scale);
    let (sin, cos) = e.rotation_deg.to_radians().sin_cos();
    // On paper (y down, offsets right of the line down): the height away
    // from the line, the width across it.
    let up = [side * cos, -side * sin];
    let across = [-up[1], up[0]];
    // A point `dx`, `dy` mm on paper from `at`, in the route's metres.
    let move_by = |at: [f64; 2], dx: f64, dy: f64| [at[0] + dx / h, at[1] + dy / plan_k];
    let size = e.text_size.max(0.5);
    all_structures(d, o)
        .map(|(m, role, _, version)| {
            let base = [m.station, -side * e.offset_mm / plan_k];
            let at = |u: f64, z: f64| {
                let (u, z) = (u * he, (z - m.base) * he);
                move_by(base, u * across[0] + z * up[0], u * across[1] + z * up[1])
            };
            // Seen along the line: across it (the offset) and up: its
            // members, else its stick drawing (which may hold a whole
            // lattice); with the stick figure asked for, a schematic one
            // (the mast from its base to its top, an arm out to each
            // point, the strings), in (across, height), m.
            let top = structure_top(m);
            let schematic = || -> Vec<Vec<[f64; 2]>> {
                let mut f = vec![vec![[0.0, m.base], [0.0, top]]];
                for p in &m.attachments {
                    let u = p[1] - m.offset;
                    if u.abs() > 0.2 {
                        f.push(vec![[0.0, p[2]], [u, p[2]]]);
                    }
                }
                for st in &m.strings {
                    f.push(vec![
                        [st.top[1] - m.offset, st.top[2]],
                        [st.bottom[1] - m.offset, st.bottom[2]],
                    ]);
                }
                f
            };
            let of = |l: &Vec<[f64; 3]>| l.iter().map(|p| [p[1] - m.offset, p[2]]).collect();
            let mut figure: Vec<Vec<[f64; 2]>> = if e.stick {
                schematic()
            } else if m.members.is_empty() {
                m.sticks.iter().map(of).collect()
            } else {
                m.members.iter().map(of).collect()
            };
            // Its strings (the schematic figure has them already).
            if !e.stick {
                figure.extend(m.strings.iter().map(|st| {
                    vec![
                        [st.top[1] - m.offset, st.top[2]],
                        [st.bottom[1] - m.offset, st.bottom[2]],
                    ]
                }));
            }
            let mut lines: Vec<(Vec<[f64; 2]>, bool)> = figure
                .iter()
                .map(|l| (l.iter().map(|p| at(p[0], p[1])).collect(), false))
                .collect();
            let outline = silhouette(&m.pole, 1);
            if !outline.is_empty() && !e.stick {
                lines.push((
                    outline.iter().map(|p| at(p[0] - m.offset, p[1])).collect(),
                    true,
                ));
            }
            if lines.is_empty() {
                lines.push((vec![at(0.0, m.base), at(0.0, m.top)], false));
            }
            // Labels: above its top, one after the other away from it.
            let mut words = Vec::new();
            if e.labels.number {
                words.push(m.id.clone());
            }
            if e.labels.type_name
                && let Some(t) = &m.type_name
            {
                words.push(t.clone());
            }
            if e.labels.height {
                words.push(format!("{:.1} m", top - m.base));
            }
            let head = at(0.0, top);
            let mut labels: Vec<([f64; 2], String, Anchor)> = words
                .into_iter()
                .enumerate()
                .map(|(k, w)| {
                    let away = 2.0 + 0.36 * size + 1.25 * size * k as f64;
                    (move_by(head, up[0] * away, up[1] * away), w, Anchor::Middle)
                })
                .collect();
            // The sides: what is drawn's extent across, mm on paper from
            // the structure's axis (the point is turned with the height).
            let mut us: Vec<f64> = figure.iter().flatten().map(|p| p[0]).collect();
            if !e.stick {
                us.extend(m.pole.iter().flatten().map(|p| p[1] - m.offset));
            }
            let (u_min, u_max) = us
                .iter()
                .fold((0.0f64, 0.0f64), |(a, b), &u| (a.min(u), b.max(u)));
            let (u_min, u_max) = (u_min * he, u_max * he);
            let v = |z: f64| (z - m.base) * he; // mm up from the base
            // A point `u` mm across and `v` mm up from the base, in the
            // route's metres.
            let local = |u: f64, vv: f64| {
                move_by(base, u * across[0] + vv * up[0], u * across[1] + vv * up[1])
            };
            // Which way "across" runs on paper: the right-hand column's
            // texts start at their point, the left-hand one's end there.
            let right_anchor = if across[0] >= 0.0 {
                Anchor::Start
            } else {
                Anchor::End
            };
            let left_anchor = if across[0] >= 0.0 {
                Anchor::End
            } else {
                Anchor::Start
            };
            let words_of = |kind: &str, key: &str, value: f64| {
                o.template
                    .legend
                    .label(kind, &[(key, &format!("{value:.2}"))])
            };
            // Lengths within 5 cm of each other written as one range.
            let ranges = |mut ls: Vec<f64>| -> Vec<String> {
                ls.sort_by(f64::total_cmp);
                let mut groups: Vec<(f64, f64)> = Vec::new();
                for l in ls {
                    match groups.last_mut() {
                        Some(g) if l - g.0 < 0.05 => g.1 = l,
                        _ => groups.push((l, l)),
                    }
                }
                groups
                    .into_iter()
                    .map(|(a, b)| {
                        let v = if b - a < 0.005 {
                            format!("{a:.2}")
                        } else {
                            format!("{a:.2}–{b:.2}")
                        };
                        o.template
                            .legend
                            .label("elevation_string", &[("length", &v)])
                    })
                    .collect()
            };
            let mut dims: Vec<Vec<[f64; 2]>> = Vec::new();
            // The arms: the levels of its own points (where strings hang
            // from it; within 0.3 m taken as one), not the points strings
            // hang from below other strings.
            let mut arms: Vec<f64> = m.attachments.iter().map(|p| p[2]).collect();
            arms.retain(|z| z.is_finite() && *z > m.base + 0.1);
            arms.sort_by(f64::total_cmp);
            arms.dedup_by(|a, b| (*a - *b).abs() < 0.3);
            let gap = 1.2 * size; // between two texts of a column, mm
            if e.labels.dimensions && !arms.is_empty() {
                let mut levels = vec![m.base];
                levels.extend(arms.iter().copied().filter(|z| *z < top - 0.3));
                levels.push(top);
                let x = u_min - 2.5; // the chain, mm across
                dims.push(vec![local(x, 0.0), local(x, v(top))]);
                for &z in &levels {
                    dims.push(vec![local(x - 0.8, v(z)), local(x + 0.8, v(z))]);
                }
                let wanted: Vec<(f64, String)> = levels
                    .windows(2)
                    .map(|w| {
                        (
                            0.5 * (v(w[0]) + v(w[1])),
                            words_of("elevation_dimension", "length", w[1] - w[0]),
                        )
                    })
                    .collect();
                place_column(&wanted, gap, |vv, at_v, text| {
                    let tx = x - 1.2;
                    labels.push((local(tx, at_v - 0.35 * size), text, left_anchor));
                    if (at_v - vv).abs() > 0.3 {
                        dims.push(vec![local(tx + 0.2, at_v), local(x, vv)]);
                    }
                });
            }
            // The right-hand column, one line per arm: its height above
            // the base and the length of the vertical (suspension) strings
            // hanging from it (strain strings lie along the line; within
            // 20° of the vertical counts as vertical).
            let mut right: Vec<(f64, String)> = Vec::new();
            for &z in &arms {
                let mut parts = Vec::new();
                if e.labels.arm_heights {
                    parts.push(words_of("elevation_arm", "height", z - m.base));
                }
                if e.labels.string_length {
                    let lengths: Vec<f64> = m
                        .strings
                        .iter()
                        .filter(|s| (s.top[2] - z).abs() < 0.3)
                        .filter(|s| {
                            let d = [
                                s.bottom[0] - s.top[0],
                                s.bottom[1] - s.top[1],
                                s.bottom[2] - s.top[2],
                            ];
                            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                            len > 1e-6 && -d[2] / len > 20f64.to_radians().cos()
                        })
                        .map(|s| s.length)
                        .collect();
                    parts.extend(ranges(lengths));
                }
                if !parts.is_empty() {
                    right.push((v(z), parts.join(", ")));
                }
            }
            if !right.is_empty() {
                let x = u_max + 1.5;
                place_column(&right, gap, |vv, at_v, text| {
                    labels.push((local(x, at_v - 0.35 * size), text, right_anchor));
                    if (at_v - vv).abs() > 0.3 {
                        dims.push(vec![local(x - 0.2, at_v), local(u_max + 0.3, vv)]);
                    }
                });
            }
            let stand = [m.station, m.offset];
            let paper = ((stand[0] - base[0]) * h).hypot((stand[1] - base[1]) * plan_k);
            ElevationDrawing {
                id: m.id.clone(),
                station: m.station,
                lines,
                leader: (paper > 0.5 && !version).then_some([stand, base]),
                labels,
                dims,
                role: if version {
                    role.map(str::to_owned)
                } else {
                    None
                },
            }
        })
        .collect()
}

/// Places the texts of a column, `wanted` (height mm up the elevation,
/// text), from the highest down, each at its own height unless that is
/// within `gap` of the one above it: then just below that one. Calls
/// `put(wanted height, height placed, text)` for each.
fn place_column(wanted: &[(f64, String)], gap: f64, mut put: impl FnMut(f64, f64, String)) {
    let mut order: Vec<&(f64, String)> = wanted.iter().collect();
    order.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut last = f64::INFINITY;
    for (v, text) in order {
        let at = v.min(last - gap);
        put(*v, at, text.clone());
        last = at;
    }
}

/// The elevations of `d` (drawn already) within frame `f`'s station range,
/// on paper: each one's lines, leader and box (its labels in it).
#[allow(clippy::type_complexity)]
fn elevations_on_paper(
    d: &RouteDrawing,
    o: &Options,
    f: &Frame,
) -> Vec<(ElevationDrawing, Vec<(Vec<[f64; 2]>, bool)>, [f64; 4])> {
    if !f.plan || o.elevations.place == ElevationPlace::Off {
        return Vec::new();
    }
    let end = f.s1;
    let size = o.elevations.text_size;
    let to_paper = |p: &[f64; 2]| [f.x(p[0]), f.plan_y(p[1])];
    elevations_of(d, o)
        .into_iter()
        .filter(|e| e.station >= f.s0 - 1e-6 && e.station <= end + 1e-6)
        .map(|e| {
            let lines: Vec<(Vec<[f64; 2]>, bool)> = e
                .lines
                .iter()
                .map(|(l, closed)| (l.iter().map(to_paper).collect(), *closed))
                .collect();
            let base = e
                .leader
                .map_or_else(|| to_paper(&[e.station, 0.0]), |l| to_paper(&l[1]));
            let mut bb = [base[0], base[1], base[0], base[1]];
            for p in lines.iter().flat_map(|(l, _)| l) {
                bb = [
                    bb[0].min(p[0]),
                    bb[1].min(p[1]),
                    bb[2].max(p[0]),
                    bb[3].max(p[1]),
                ];
            }
            for (p, text, anchor) in &e.labels {
                let q = to_paper(p);
                // (An estimate of the text's width.)
                let w = 0.55 * size * text.chars().count() as f64;
                let (x0, x1) = match anchor {
                    Anchor::Start => (q[0], q[0] + w),
                    Anchor::Middle => (q[0] - w / 2.0, q[0] + w / 2.0),
                    Anchor::End => (q[0] - w, q[0]),
                };
                bb = [
                    bb[0].min(x0),
                    bb[1].min(q[1] - size),
                    bb[2].max(x1),
                    bb[3].max(q[1]),
                ];
            }
            for p in e.dims.iter().flatten() {
                let q = to_paper(p);
                bb = [
                    bb[0].min(q[0]),
                    bb[1].min(q[1]),
                    bb[2].max(q[0]),
                    bb[3].max(q[1]),
                ];
            }
            (e, lines, bb)
        })
        .collect()
}

/// What to warn about the elevations of `d` in frame `f`: two of the
/// line's own that overlap, one leaving the plan panel or reaching into
/// the profile (all kept, drawn anyway).
fn elevation_warnings(d: &RouteDrawing, o: &Options, f: &Frame) -> Vec<String> {
    let all = elevations_on_paper(d, o, f);
    let boxes: Vec<(String, [f64; 4])> = all
        .iter()
        .filter(|(e, ..)| e.role.is_none())
        .map(|(e, _, bb)| (e.id.clone(), *bb))
        .collect();
    let mut warnings = Vec::new();
    let overlap =
        |a: [f64; 4], b: [f64; 4]| a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3];
    for (i, (a, ba)) in boxes.iter().enumerate() {
        for (b, bb) in &boxes[i + 1..] {
            if overlap(*ba, *bb) {
                warnings.push(format!(
                    "the elevations of {a} and {b} overlap (drawn anyway): a larger scale or \
                     the other side may part them"
                ));
            }
        }
    }
    let profile = [
        f.left,
        f.prof_top,
        f.left + f.width(),
        f.prof_top + f.prof_h,
    ];
    let plan = [
        f.left - 1e-6,
        f.plan_top - 1e-6,
        f.left + f.width() + 1e-6,
        f.plan_top + f.plan_h + 1e-6,
    ];
    for (a, ba) in &boxes {
        // (Its height across the line; along it an elevation may stand
        // past the panel's end.)
        if ba[1] < plan[1] || ba[3] > plan[3] {
            warnings.push(format!(
                "the elevation of {a} leaves the plan panel (drawn anyway): a smaller offset \
                 or a smaller scale of the elevations keeps it in"
            ));
        }
        if overlap(*ba, profile) {
            warnings.push(format!(
                "the elevation of {a} reaches into the profile (drawn anyway): draw the \
                 elevations above the line, or nearer to it"
            ));
        }
    }
    warnings
}

/// The elevations of `d` drawn in frame `f` on their own layer, not
/// clipped, a thin dashed leader from where each structure stands, their
/// labels; what to warn about ([`elevation_warnings`]).
fn draw_elevations(items: &mut Vec<Item>, d: &RouteDrawing, o: &Options, f: &Frame) -> Vec<String> {
    let own = o
        .template
        .elevations
        .pen
        .over(lp(o, Layer::Elevations, "#555", 0.2));
    let size = o.elevations.text_size;
    for (e, lines, _) in elevations_on_paper(d, o, f) {
        let p = match &e.role {
            Some(r) => crate::style::structure_pen(&o.template, Some(r), own.clone()),
            None => own.clone(),
        };
        for (points, closed) in lines {
            items.push(Item::Path(Path {
                layer: Layer::Elevations,
                points,
                closed,
                pen: p.clone(),
                title: Some(format!("{} elevation", e.id)),
            }));
        }
        if let Some([a, b]) = e.leader {
            items.push(path(
                Layer::Elevations,
                vec![[f.x(a[0]), f.plan_y(a[1])], [f.x(b[0]), f.plan_y(b[1])]],
                lp(o, Layer::Elevations, "#888", 0.13).dashed("1 0.7"),
                None,
            ));
        }
        let thin = Pen {
            width: (p.width * 0.6).min(0.13),
            dash: None,
            ..p.clone()
        };
        for d in &e.dims {
            items.push(path(
                Layer::Elevations,
                d.iter().map(|q| [f.x(q[0]), f.plan_y(q[1])]).collect(),
                thin.clone(),
                None,
            ));
        }
        for (at, words, anchor) in &e.labels {
            items.push(Item::Text(Text {
                layer: Layer::Elevations,
                at: [f.x(at[0]), f.plan_y(at[1])],
                text: words.clone(),
                size,
                anchor: *anchor,
                bold: false,
                color: p.color,
            }));
        }
    }
    elevation_warnings(d, o, f)
}

/// The scales as written on the drawings.
pub(crate) fn scales_text(o: &Options) -> String {
    let across = o.plan_offset_scale.unwrap_or(o.horizontal_scale);
    let mut t = format!(
        "horizontal 1:{} · vertical 1:{} (×{:.0})",
        o.horizontal_scale,
        o.vertical_scale,
        o.horizontal_scale / o.vertical_scale
    );
    if o.plan && (across - o.horizontal_scale).abs() > 1e-9 {
        t.push_str(&format!(" · plan across 1:{across}"));
    }
    t
}

/// The heading of the drawings.
fn heading(d: &RouteDrawing, o: &Options) -> String {
    let name = if d.name.trim().is_empty() || d.name == d.route {
        format!("Route {}", d.route)
    } else {
        format!("Route {} — {}", d.route, d.name)
    };
    format!(
        "{name}: {}",
        if o.plan {
            "plan and profile"
        } else {
            "profile"
        }
    )
}

/// Station range of the continuous drawing: the options' range, else the
/// route and its structures.
fn whole_range(d: &RouteDrawing, o: &Options) -> (f64, f64) {
    if let Some([a, b]) = o.stations {
        return (a.min(b), a.max(b).max(a.min(b) + 1.0));
    }
    let (mut s0, mut s1) = (0.0f64, d.length);
    for m in &d.structures {
        s0 = s0.min(m.station);
        s1 = s1.max(m.station);
    }
    (s0, s1)
}

/// The route (or its station range) as one continuous drawing, with its
/// frame and what to warn about.
pub(crate) fn continuous(d: &RouteDrawing, o: &Options) -> (Drawing, Frame, Vec<String>) {
    let d = &d.shown(o.plan_half_width);
    let (s0, s1) = whole_range(d, o);
    let part = height_range(d, o, s0, s1);
    let (h, v) = (1000.0 / o.horizontal_scale, 1000.0 / o.vertical_scale);
    let plan_k = 1000.0 / o.plan_offset_scale.unwrap_or(o.horizontal_scale);
    let margin = 30.0;
    let title_h = 8.0 + 5.0 * o.title.len() as f64;
    let (above, below) = plan_room(d, o, plan_k);
    let plan_h = if o.plan { above + below } else { 0.0 };
    let plan_top = margin + title_h;
    let f = Frame {
        s0,
        s1,
        left: margin,
        plan: o.plan,
        plan_top,
        plan_h,
        plan_axis: plan_top + above,
        prof_top: margin + title_h + if o.plan { plan_h + 15.0 } else { 0.0 },
        prof_h: (part.z1 - part.z0) * v,
        parts: vec![part],
        h,
        plan_k,
        v,
        whole_ends: false,
        paper_end: s1,
        bands_h: 0.0,
        legend: Default::default(),
    };
    let width = 2.0 * margin + f.width();
    let height = f.prof_top + f.prof_h + margin;
    let mut drawing = Drawing::sheet(width, height);
    let items = &mut drawing.items;
    items.push(bold(text(
        Layer::Title,
        [margin, margin + 5.0],
        5.0,
        Anchor::Start,
        heading(d, o),
    )));
    for (k, t) in o.title.iter().enumerate() {
        items.push(text(
            Layer::Title,
            [margin, margin + 10.0 + 5.0 * k as f64],
            3.0,
            Anchor::Start,
            t.clone(),
        ));
    }
    items.push(text(
        Layer::Title,
        [width - margin, margin + 5.0],
        3.0,
        Anchor::End,
        scales_text(o),
    ));
    draw(items, d, o, &f);
    let warnings = draw_elevations(items, d, o, &f);
    (drawing, f, warnings)
}

/// The plan panel's room above and below the line's axis, mm: the line
/// width at the plan's scale across the line, the corridor (with room for
/// its labels), and what the structures' elevations need (with room for
/// their names and labels).
pub(crate) fn plan_room(d: &RouteDrawing, o: &Options, plan_k: f64) -> (f64, f64) {
    let half = o
        .corridor
        .map_or(o.plan_half_width, |w| o.plan_half_width.max(w))
        * plan_k
        + if o.corridor.is_some() { 3.0 } else { 0.0 };
    let (mut above, mut below) = (half, half);
    if o.plan && o.elevations.place != ElevationPlace::Off {
        let size = o.elevations.text_size;
        for e in elevations_of(d, o) {
            for p in e.lines.iter().flat_map(|(l, _)| l) {
                above = above.max(-p[1] * plan_k + 4.0);
                below = below.max(p[1] * plan_k + 1.0);
            }
            for (p, ..) in &e.labels {
                above = above.max(-p[1] * plan_k + size + 1.0);
                below = below.max(p[1] * plan_k + 1.0);
            }
        }
    }
    (above, below)
}

/// One sheet of a paged drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    /// Sheet number (1-based).
    pub number: usize,
    /// Station range.
    pub from: f64,
    /// End station.
    pub to: f64,
    /// Where the sheet starts and ends: a structure, a free alignment
    /// point, or the station where a station range starts or ends.
    pub structures: (String, String),
    /// Its page.
    pub page: Page,
    /// The sheet.
    pub drawing: Drawing,
    /// Problems: a span longer than the sheet, a profile taller than it.
    pub warnings: Vec<String>,
}

impl Sheet {
    /// The sheet as SVG.
    pub fn svg(&self) -> String {
        self.drawing.svg()
    }
}

/// A place a sheet may start or end: a structure, a free alignment
/// point, or the end of a station range.
pub(crate) struct Stop {
    pub(crate) station: f64,
    label: String,
    /// Why no sheet may break here (`None`: a free break).
    blocked: Option<String>,
}

/// Station range of what is drawn of structure `m` (its station, its
/// attachments and its stick drawing).
fn extent(m: &crate::Mark) -> (f64, f64) {
    m.attachments
        .iter()
        .chain(m.sticks.iter().flatten())
        .map(|p| p[0])
        .filter(|s| s.is_finite())
        .fold((m.station, m.station), |(a, b), s| (a.min(s), b.max(s)))
}

/// Where sheets may start and end, in station order: every structure
/// drawn (the first and last always) and every free alignment point
/// between them, or the ends of the station range. `d` is what is drawn
/// ([`RouteDrawing::shown`]). A break inside is **free**
///
/// - at a structure **on the line**: one of the run along the alignment
///   (tied to an alignment point, or standing on the alignment), never
///   one beside it (a connection away from the alignment, however near),
/// - or at an alignment point **without a structure** (an angle point
///   without a pylon),
///
/// whose station is at least `clear` m (the clearance on paper) from what
/// is drawn of every other structure.
fn stops(d: &RouteDrawing, o: &Options, clear: f64) -> Vec<Stop> {
    let extents: Vec<(f64, f64)> = d.structures.iter().map(extent).collect();
    // The first structure (other than structure `own`) whose drawing is
    // within the clearance of `station`.
    let near = |station: f64, own: Option<usize>| {
        d.structures
            .iter()
            .zip(&extents)
            .enumerate()
            .find(|&(j, (_, &(a, b)))| Some(j) != own && station > a - clear && station < b + clear)
            .map(|(_, (other, _))| other)
    };
    let too_near = |id: &str, other: &crate::Mark| {
        format!(
            "{id} is within {clear:.1} m of {} ({})",
            other.id,
            format_station(other.station)
        )
    };
    let mut stops: Vec<Stop> = d
        .structures
        .iter()
        .enumerate()
        .map(|(k, m)| Stop {
            station: m.station,
            label: m.id.clone(),
            blocked: if m.away {
                Some(format!(
                    "{} stands beside the line, {:.1} m {} of the alignment (not on it)",
                    m.id,
                    m.offset.abs(),
                    if m.offset > 0.0 { "right" } else { "left" },
                ))
            } else {
                near(m.station, Some(k)).map(|other| too_near(&m.id, other))
            },
        })
        .collect();
    stops.sort_by(|a, b| a.station.total_cmp(&b.station));
    // Free alignment points between the first and the last structure.
    if let (Some(first), Some(last)) = (stops.first(), stops.last()) {
        let (s0, s1) = (first.station, last.station);
        let free: Vec<Stop> = d
            .alignment
            .iter()
            .filter(|p| p.structure.is_none() && p.station > s0 + 1e-6 && p.station < s1 - 1e-6)
            .map(|p| Stop {
                station: p.station,
                label: p.id.clone(),
                blocked: near(p.station, None).map(|other| too_near(&p.id, other)),
            })
            .collect();
        if !free.is_empty() {
            stops.extend(free);
            stops.sort_by(|a, b| a.station.total_cmp(&b.station));
        }
    }
    if let Some([a, b]) = o.stations {
        let (a, b) = (a.min(b), a.max(b));
        stops.retain(|s| s.station > a + 1e-6 && s.station < b - 1e-6);
        let end = |s: f64| Stop {
            station: s,
            label: format_station(s),
            blocked: None,
        };
        stops.insert(0, end(a));
        stops.push(end(b));
    }
    stops
}

/// Where a sheet of [`sheet_layout`] lies along the route, and what is
/// wrong with it: a sheet without its drawing. The export draws each one
/// ([`sheets`]); the desktop app shows them over its continuous profile
/// (the sheet preview).
#[derive(Debug, Clone, PartialEq)]
pub struct SheetLayout {
    /// Sheet number (1-based).
    pub number: usize,
    /// Station range.
    pub from: f64,
    /// End station.
    pub to: f64,
    /// Metres of route the sheet's width holds at the horizontal scale
    /// (from `from`): its frame, longer than `to` − `from` when the
    /// stretch is shorter, shorter when the sheet runs on (warned).
    pub reach: f64,
    /// Where the sheet starts and ends: a structure, a free alignment
    /// point, or the station where a station range starts or ends.
    pub structures: (String, String),
    /// Problems: no free break within reach (the sheet runs on), a span
    /// longer than the sheet, a profile taller than it.
    pub warnings: Vec<String>,
    /// The sheet's page (the same on every sheet unless the layout was
    /// optimised with different sizes allowed).
    pub page: Page,
    /// The profile's parts, each with its height range (more than one
    /// where the datum is stepped).
    parts: Vec<Part>,
}

/// The paper geometry of sheets of `page` with options `o` and their
/// template: where the frame, the title block, the revision table, the
/// legend, the key map, the scale bars and the panels go.
pub(crate) struct Paper {
    /// mm per m: along the route, across in the plan.
    h: f64,
    plan_k: f64,
    left: f64,
    /// Width of the drawing area, mm.
    usable: f64,
    plan_top: f64,
    plan_h: f64,
    plan_axis: f64,
    prof_top: f64,
    /// Height of the profile panel, mm.
    available: f64,
    /// The band rows' height under the profile, mm.
    bands_h: f64,
    /// The frame: inside the margins.
    frame: furniture::Rect,
    /// Inside the frame's padding.
    inner: furniture::Rect,
    /// The title block.
    pub(crate) title_block: furniture::Rect,
    /// The revision table, the stringing table, the legend, the key map,
    /// the scale bars.
    revisions: Option<furniture::Rect>,
    stringing_table: Option<furniture::Rect>,
    legend: Option<furniture::Rect>,
    key_map: Option<furniture::Rect>,
    scale_bars: Option<furniture::Rect>,
    /// The bottom row (key map, stringing table, legend, scale bars)
    /// reaches into the title block: said on every sheet.
    bottom_row_overlap: Option<String>,
}

impl Paper {
    /// Metres of route a sheet holds at the horizontal scale.
    pub(crate) fn usable_m(&self) -> f64 {
        self.usable / self.h
    }

    /// Height of the profile panel, mm.
    pub(crate) fn available_mm(&self) -> f64 {
        self.available
    }

    /// The frame of a sheet from `s0` to `s1` with its profile's `parts`.
    fn frame(&self, o: &Options, s0: f64, s1: f64, parts: Vec<Part>) -> Frame {
        Frame {
            s0,
            s1,
            left: self.left,
            plan: o.plan,
            plan_top: self.plan_top,
            plan_h: self.plan_h,
            plan_axis: self.plan_axis,
            prof_top: self.prof_top,
            prof_h: self.available,
            parts,
            h: self.h,
            plan_k: self.plan_k,
            v: 1000.0 / o.vertical_scale,
            whole_ends: true,
            paper_end: s0 + self.usable / self.h,
            bands_h: self.bands_h,
            legend: Default::default(),
        }
    }

    pub(crate) fn new(d: &RouteDrawing, o: &Options, page: &Page) -> Self {
        let t = &o.template;
        let h = 1000.0 / o.horizontal_scale;
        let plan_k = 1000.0 / o.plan_offset_scale.unwrap_or(o.horizontal_scale);
        let m = &t.page.margin;
        let frame = [
            m.left.0,
            m.top.0,
            page.width - m.left.0 - m.right.0,
            page.height - m.top.0 - m.bottom.0,
        ];
        let pad = t.frame.padding.0;
        let inner = [
            frame[0] + pad,
            frame[1] + pad,
            frame[2] - 2.0 * pad,
            frame[3] - 2.0 * pad,
        ];
        // The title block in its corner, inside the frame's padding (as
        // everything around the panels: the revisions and the legend on
        // it, the bottom row, the notes).
        let tb_h: f64 = t.title_block.rows.iter().map(|r| r.0).sum();
        let tb_w = match t.title_block.width {
            crate::template::Size::Auto => inner[2],
            crate::template::Size::Fixed(w) => w.min(inner[2]),
        };
        let full = tb_w >= inner[2] - 1e-6;
        let tb_x = match t.title_block.corner {
            crate::template::Corner::BottomRight => inner[0] + inner[2] - tb_w,
            crate::template::Corner::BottomLeft => inner[0],
        };
        let title_block = [tb_x, inner[1] + inner[3] - tb_h, tb_w, tb_h];
        // Stacked on the title block: the revisions, the legend.
        let mut top = title_block[1];
        let mut stack = |height: f64, width: Option<f64>| {
            top -= height;
            let w = width.unwrap_or(tb_w).min(tb_w);
            [tb_x + tb_w - w, top, w, height]
        };
        let revisions = (t.revisions.place == "above_title_block"
            && !t.revisions.columns.is_empty())
        .then(|| {
            stack(
                furniture::revisions_height(t),
                Some(t.revisions.columns.iter().map(|c| c.width.0).sum()),
            )
        });
        let st = &t.stringing_table;
        let st_w: f64 = st.columns.iter().map(|c| c.width.0).sum();
        let mut stringing_table = (st.place == "above_title_block" && !st.columns.is_empty())
            .then(|| stack(furniture::stringing_table_height(t), Some(st_w)));
        let legend_w = t.legend.width.map(|w| w.0);
        let mut legend =
            (t.legend.place == "above_title_block").then(|| stack(t.legend.height.0, legend_w));
        let mut lowest = top;
        // The bottom row, beside the title block (above it when it is as
        // wide as the frame), from the frame's free side.
        let row_bottom = if full {
            top - 3.0
        } else {
            frame[1] + frame[3] - pad
        };
        let (mut x, dir) = match (full, t.title_block.corner) {
            (true, _) | (false, crate::template::Corner::BottomRight) => (inner[0], 1.0),
            (false, crate::template::Corner::BottomLeft) => (inner[0] + inner[2], -1.0),
        };
        let mut place = |w: f64, height: f64| {
            let left = if dir > 0.0 { x } else { x - w };
            x += dir * (w + 3.0);
            let r = [left, row_bottom - height, w, height];
            lowest = lowest.min(r[1]);
            r
        };
        let key_map =
            (t.key_map.place == "bottom").then(|| place(t.key_map.width.0, t.key_map.height.0));
        if st.place == "bottom" && !st.columns.is_empty() {
            stringing_table = Some(place(st_w, furniture::stringing_table_height(t)));
        }
        if t.legend.place == "bottom" {
            legend = Some(place(legend_w.unwrap_or(120.0), t.legend.height.0));
        }
        let scale_bars = matches!(t.scale_bar.place.as_str(), "bottom" | "title_block")
            .then(|| place(70.0, 16.0));
        // Beside the title block: the row must end before it.
        let row_end = x - dir * 3.0;
        let bottom_row_overlap = (!full
            && if dir > 0.0 {
                row_end > tb_x - 1.0
            } else {
                row_end < tb_x + tb_w + 1.0
            })
        .then(|| {
            format!(
                "the bottom row (key map, stringing table, legend, scale bars) is {:.0} mm too \
                 wide for this paper: it runs into the title block (narrower or fewer items \
                 in the sheet template, or a larger paper)",
                if dir > 0.0 {
                    row_end - tb_x
                } else {
                    tb_x + tb_w - row_end
                }
            )
        });
        // The panels between.
        let left = inner[0] + t.layout.axis_room.0;
        let (above, below) = plan_room(d, o, plan_k);
        let auto = above + below;
        let plan_h = if o.plan {
            match t.layout.plan_height {
                crate::template::Size::Auto => auto,
                crate::template::Size::Fixed(min) => auto.max(min),
            }
        } else {
            0.0
        };
        // The axis: the room above it as the line, corridor and
        // elevations need, the rest below.
        let extra = plan_h - auto;
        let plan_axis_in = above + extra / 2.0;
        let gap = t.layout.gap.0;
        let labels = if t.layout.station_labels { 8.0 } else { 2.0 };
        let bands_h = furniture::bands_height(t, o, d);
        let area_top = inner[1] + 6.0;
        let bottom = lowest - pad;
        let (plan_top, prof_top, prof_bottom) = if !o.plan {
            (area_top, area_top, bottom - labels - bands_h)
        } else if t.layout.plan_below() {
            let plan_top = bottom - plan_h;
            (plan_top, area_top, plan_top - gap - labels - bands_h)
        } else {
            let prof_top = area_top + plan_h + gap;
            (area_top, prof_top, bottom - labels - bands_h)
        };
        Self {
            h,
            plan_k,
            left,
            usable: inner[0] + inner[2] - left,
            plan_top,
            plan_h,
            plan_axis: plan_top + plan_axis_in,
            prof_top,
            available: (prof_bottom - prof_top).max(10.0),
            bands_h,
            frame,
            inner,
            title_block,
            revisions,
            stringing_table,
            legend,
            key_map,
            scale_bars,
            bottom_row_overlap,
        }
    }
}

/// Where the route splits over sheets of `page`, **at structures**, and
/// the warnings of each sheet: what [`sheets`] draws, without drawing it.
/// Each sheet holds as many whole spans as fit at the horizontal scale,
/// and the place where it ends starts the next sheet (match line), never
/// in the middle of a span. A sheet only ends at a structure **on the
/// line** (of the run along the alignment; never at one beside it, such
/// as a connection, however near) or at an alignment point **without a
/// structure**, at least [`Options::break_clearance`] on paper from what
/// is drawn of every other structure ([`RouteDrawing::shown`]: those
/// beside the line within the line width): otherwise the break moves
/// back to the last free place that fits, and when none fits the sheet
/// runs on to the next free one (drawn anyway, with a warning). With a
/// station range in the options, the first sheet starts and the last
/// ends at the range. The sheet template places the panels.
pub fn sheet_layout(d: &RouteDrawing, o: &Options, page: &Page) -> Vec<SheetLayout> {
    layout(&d.shown(o.plan_half_width), o, page)
}

/// [`sheet_layout`] of `d`, what is drawn of the route.
fn layout(d: &RouteDrawing, o: &Options, page: &Page) -> Vec<SheetLayout> {
    sheet_layout_on(d, o, &route_stops(d, o), page)
}

/// The sheets of `page` over `stations` of `d` (what is drawn): as
/// [`sheet_layout`].
pub(crate) fn sheet_layout_on(
    d: &RouteDrawing,
    o: &Options,
    stations: &[Stop],
    page: &Page,
) -> Vec<SheetLayout> {
    let paper = Paper::new(d, o, page);
    let plan: Vec<_> = greedy(stations, paper.usable_m())
        .into_iter()
        .map(|r| (r, page.clone()))
        .collect();
    layout_planned(d, o, stations, &plan)
}

/// Where sheets of the route `d` (what is drawn) may start and end: the
/// stops at the options' break clearance (the same on every page).
pub(crate) fn route_stops(d: &RouteDrawing, o: &Options) -> Vec<Stop> {
    let h = 1000.0 / o.horizontal_scale;
    let clearance =
        crate::check_break_clearance(o.break_clearance).unwrap_or(crate::BREAK_CLEARANCE_MM);
    stops(d, o, clearance / h)
}

/// Whether a sheet may end at stop `k` (the last always).
pub(crate) fn can_break(stations: &[Stop], k: usize) -> bool {
    k + 1 == stations.len() || stations[k].blocked.is_none()
}

/// The stops where a sheet starting at stop `i` may end: every free one
/// within `reach` m, or, when none is, the first free one after (the
/// sheet runs on, warned).
pub(crate) fn ends(stations: &[Stop], i: usize, reach: f64) -> Vec<usize> {
    let last = stations.len().saturating_sub(1);
    let within: Vec<usize> = (i + 1..=last)
        .take_while(|&k| stations[k].station - stations[i].station <= reach + 1e-9)
        .filter(|&k| can_break(stations, k))
        .collect();
    if within.is_empty() {
        (i + 1..=last)
            .find(|&k| can_break(stations, k))
            .into_iter()
            .collect()
    } else {
        within
    }
}

/// Break points: greedy, whole spans per sheet, at free stops only.
fn greedy(stations: &[Stop], reach: f64) -> Vec<(usize, usize)> {
    let last = stations.len().saturating_sub(1);
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < last {
        let j = ends(stations, i, reach).last().copied().unwrap_or(last);
        ranges.push((i, j));
        i = j;
    }
    if ranges.is_empty() && !stations.is_empty() {
        ranges.push((0, 0));
    }
    ranges
}

/// The sheets of `plan` (each sheet's first and last stop of
/// `stations`, and its page), numbered in order.
pub(crate) fn layout_planned(
    d: &RouteDrawing,
    o: &Options,
    stations: &[Stop],
    plan: &[((usize, usize), Page)],
) -> Vec<SheetLayout> {
    let curves = curve_warnings(d, o);
    let mut papers: Vec<(Page, Paper)> = Vec::new();
    plan.iter()
        .enumerate()
        .map(|(k, ((a, b), page))| {
            if !papers.iter().any(|(p, _)| p == page) {
                papers.push((page.clone(), Paper::new(d, o, page)));
            }
            let paper = &papers.iter().find(|(p, _)| p == page).expect("added").1;
            let mut sl = sheet_at(d, o, stations, (*a, *b), page, paper);
            sl.number = k + 1;
            if k == 0 {
                sl.warnings.extend(curves.iter().cloned());
            }
            sl
        })
        .collect()
}

/// The sheet from stop `a` to stop `b` on `page` (numbered 1, without
/// the route's curve warnings).
pub(crate) fn sheet_at(
    d: &RouteDrawing,
    o: &Options,
    stations: &[Stop],
    (a, b): (usize, usize),
    page: &Page,
    paper: &Paper,
) -> SheetLayout {
    let (h, usable) = (paper.h, paper.usable);
    let reach = usable / h; // metres per sheet
    let what = if o.plan { "span" } else { "stretch" };
    let (s0, s1) = (stations[a].station, stations[b].station);
    let mut warnings = Vec::new();
    // Structures within the sheet's reach where it could not break.
    let skipped: Vec<&str> = stations
        .get(a + 1..b)
        .unwrap_or_default()
        .iter()
        .filter(|s| s.station - s0 <= reach + 1e-9)
        .filter_map(|s| s.blocked.as_deref())
        .collect();
    if (s1 - s0) * h > usable + 1e-6 && !skipped.is_empty() {
        warnings.push(format!(
            "no free sheet break within the {reach:.0} m the sheet holds at 1:{} ({}): \
             the sheet runs on to {} ({:.0} m), drawn anyway",
            o.horizontal_scale,
            skipped.join("; "),
            stations[b].label,
            s1 - s0
        ));
    } else if (s1 - s0) * h > usable + 1e-6 {
        warnings.push(format!(
            "{what} {}–{} ({:.0} m) is longer than the sheet holds at 1:{} ({:.0} m)",
            stations[a].label,
            stations[b].label,
            s1 - s0,
            o.horizontal_scale,
            reach
        ));
    }
    let (parts, fitted) = fit(d, o, s0, s1, paper.available);
    warnings.extend(fitted);
    // Elevations overlapping each other or the profile.
    warnings.extend(elevation_warnings(
        d,
        o,
        &paper.frame(o, s0, s1, parts.clone()),
    ));
    SheetLayout {
        number: 1,
        from: s0,
        to: s1,
        reach,
        structures: (stations[a].label.clone(), stations[b].label.clone()),
        warnings,
        page: page.clone(),
        parts,
    }
}

/// The structures at a sheet's ends as its title names them: a station
/// (the end of a station range) is no structure, "–"; nothing when both
/// ends are stations.
fn end_names(sl: &SheetLayout) -> String {
    let name = |label: &str, s: f64| {
        if label == format_station(s) {
            "–".to_owned()
        } else {
            label.to_owned()
        }
    };
    let (a, b) = (
        name(&sl.structures.0, sl.from),
        name(&sl.structures.1, sl.to),
    );
    if a == "–" && b == "–" {
        String::new()
    } else {
        format!("   ({a} – {b})")
    }
}

/// The values of the fields of sheet `sl` of `n` on `page`.
fn sheet_values(
    d: &RouteDrawing,
    o: &Options,
    page: &Page,
    sl: &SheetLayout,
    n: usize,
) -> furniture::Values {
    let mut text = std::collections::BTreeMap::new();
    let mut number = std::collections::BTreeMap::new();
    let mut station = std::collections::BTreeMap::new();
    // The user's values first (defaults, then given, then this sheet's):
    // the computed ones are not theirs to change.
    for f in &o.template.fields {
        text.insert(f.key.clone(), f.default.clone());
    }
    for (k, v) in &o.fields.values {
        text.insert(k.clone(), v.clone());
    }
    for (k, m) in &o.fields.per_sheet {
        if let Some(v) = m.get(&sl.number) {
            text.insert(k.clone(), v.clone());
        }
    }
    let mut put = |k: &str, v: String| {
        text.insert(k.to_owned(), v);
    };
    put("project", o.info.project.clone());
    put("line", o.info.line.clone());
    put("route", d.route.clone());
    put("route_name", d.name.clone());
    put("heading", heading(d, o));
    put("title", o.title.join("\n"));
    for k in 0..3 {
        put(
            &format!("title{}", k + 1),
            o.title.get(k).cloned().unwrap_or_default(),
        );
    }
    let ends = |label: &str, s: f64| {
        if label == format_station(s) {
            "–".to_owned()
        } else {
            label.to_owned()
        }
    };
    put("from_structure", ends(&sl.structures.0, sl.from));
    put("to_structure", ends(&sl.structures.1, sl.to));
    put("ends", end_names(sl));
    put("scales", scales_text(o));
    put("format", page.name.clone());
    put(
        "date",
        if o.info.date.is_empty() {
            crate::today()
        } else {
            o.info.date.clone()
        },
    );
    put("cases", o.info.cases.clone());
    put("crs", o.info.crs.clone());
    put(
        "height_datum",
        if o.info.height_datum.trim().is_empty() {
            crate::height_datum_of(&o.info.crs)
        } else {
            o.info.height_datum.clone()
        },
    );
    put("program", format!("OpenOHL {}", env!("CARGO_PKG_VERSION")));
    put("plan_kind", o.template.legend.label("plan_sheets", &[]));
    for (k, v) in [
        ("sheet", sl.number as f64),
        ("sheets", n as f64),
        ("hscale", o.horizontal_scale),
        ("vscale", o.vertical_scale),
        (
            "plan_scale",
            o.plan_offset_scale.unwrap_or(o.horizontal_scale),
        ),
        ("exaggeration", o.horizontal_scale / o.vertical_scale),
        ("page_width", page.width.round()),
        ("page_height", page.height.round()),
    ] {
        number.insert(k.to_owned(), v);
    }
    station.insert("from".to_owned(), sl.from);
    station.insert("to".to_owned(), sl.to);
    furniture::Values {
        text,
        number,
        station,
        template: o.template.clone(),
    }
}

/// The route split over sheets of `page` as [`sheet_layout`] says, each
/// drawn with its own height range and what the sheet template puts
/// around the plan and the profile (frame, title block, revisions, notes,
/// legend, key map, scale bars, north arrows, band rows).
pub fn sheets(d: &RouteDrawing, o: &Options, page: &Page) -> Vec<Sheet> {
    let d = &d.shown(o.plan_half_width);
    draw_sheets(d, o, layout(d, o, page))
}

/// The sheets of `layout` (of `d`, what is drawn), each on its page.
pub(crate) fn draw_sheets(d: &RouteDrawing, o: &Options, layout: Vec<SheetLayout>) -> Vec<Sheet> {
    draw_some(d, o, layout, None)
}

/// Sheet `number` (1-based) of the route split as [`sheet_layout`] says,
/// drawn alone (the others are laid out, not drawn): what [`sheets`]
/// draws as that sheet. None: no such sheet.
pub fn sheet(d: &RouteDrawing, o: &Options, page: &Page, number: usize) -> Option<Sheet> {
    let d = &d.shown(o.plan_half_width);
    draw_some(d, o, layout(d, o, page), Some(number)).pop()
}

/// Sheet `number` of `layout` (of route `d`, not yet [`RouteDrawing::shown`]).
pub(crate) fn sheet_of(
    d: &RouteDrawing,
    o: &Options,
    layout: Vec<SheetLayout>,
    number: usize,
) -> Option<Sheet> {
    let d = &d.shown(o.plan_half_width);
    draw_some(d, o, layout, Some(number)).pop()
}

/// The sheets of `layout`, or only sheet `only` (1-based).
fn draw_some(
    d: &RouteDrawing,
    o: &Options,
    layout: Vec<SheetLayout>,
    only: Option<usize>,
) -> Vec<Sheet> {
    let n = layout.len();
    let t = &o.template;
    layout
        .into_iter()
        .filter(|sl| only.is_none_or(|k| sl.number == k))
        .map(|sl| {
            let page = &sl.page.clone();
            let paper = Paper::new(d, o, page);
            let (s0, s1) = (sl.from, sl.to);
            let f = paper.frame(o, s0, s1, sl.parts.clone());
            let values = sheet_values(d, o, page, &sl, n);
            let mut drawing = Drawing::sheet(page.width, page.height);
            let items = &mut drawing.items;
            for b in &t.frame.border {
                let r = paper.frame;
                items.push(rect(
                    Layer::Frame,
                    r[0] + b.inset.0,
                    r[1] + b.inset.0,
                    r[2] - 2.0 * b.inset.0,
                    r[3] - 2.0 * b.inset.0,
                    o.template.pen(Layer::Frame, pen("#000", b.width.0)),
                ));
            }
            draw(items, d, o, &f);
            if o.bands && !t.bands.is_empty() {
                furniture::bands(items, t, &f, d, o, f.prof_top + f.prof_h, sl.number);
            }
            // Match lines at the sheet's ends, over every panel.
            let top = if o.plan {
                f.plan_top.min(f.prof_top)
            } else {
                f.prof_top
            };
            let bottom = (f.prof_top + f.prof_h + f.bands_h).max(if o.plan {
                f.plan_top + f.plan_h
            } else {
                0.0
            });
            let match_pen = lp(o, Layer::Match, "#06c", 0.3).dashed("2 1");
            for x in [f.x(s0), f.x(s1)] {
                items.push(path(
                    Layer::Match,
                    vec![[x, top], [x, bottom]],
                    match_pen.clone(),
                    None,
                ));
            }
            f.legend.borrow_mut().add(
                crate::style::Sample::Line(match_pen),
                o.template.legend.label("match_line", &[]),
            );
            // The elevations over the match lines (their warnings are the
            // layout's).
            let _ = draw_elevations(items, d, o, &f);
            if o.plan && t.north_arrow.place == "plan" {
                furniture::plan_north_arrows(items, &f, d, t.north_arrow.size.0);
            }
            // Graphic scales.
            match t.scale_bar.place.as_str() {
                "plan" if o.plan => {
                    let (hl, _) = furniture::scale_lengths(t, o);
                    furniture::scale_bar(
                        items,
                        [f.left + 3.0, f.plan_top + f.plan_h - 4.0],
                        hl,
                        f.h,
                        &format!(
                            "1:{}",
                            crate::template::format_number(o.horizontal_scale, "")
                        ),
                    );
                }
                "profile" => {
                    let (hl, vl) = furniture::scale_lengths(t, o);
                    furniture::scale_bar(
                        items,
                        [f.left + 8.0, f.prof_top + 8.0],
                        hl,
                        f.h,
                        &format!(
                            "1:{}",
                            crate::template::format_number(o.horizontal_scale, "")
                        ),
                    );
                    furniture::vertical_scale_bar(
                        items,
                        [f.left + 3.0, f.prof_top + 6.0 + vl * f.v],
                        vl,
                        f.v,
                    );
                }
                _ => {
                    if let Some(r) = paper.scale_bars {
                        furniture::scale_bars_in(items, t, o, r);
                    }
                }
            }
            furniture::title_block(items, t, paper.title_block, &values, &o.logos);
            if let Some(r) = paper.revisions {
                furniture::revisions(items, t, r, &values, o);
            }
            if let Some(r) = paper.key_map {
                furniture::key_map(items, t, r, d, o, s0, s1);
            }
            let notes = furniture::notes(items, t, paper.inner, &values);
            let mut warnings = sl.warnings.clone();
            warnings.extend(paper.bottom_row_overlap.clone());
            if let Some(r) = paper.stringing_table {
                warnings.extend(furniture::stringing_table(items, t, r, &values, d, s0, s1));
            }
            // A note over the title block, the revisions, the legend, the
            // key map or the scale bars: said (the template places notes).
            for (k, n) in notes.iter().enumerate() {
                for (what, r) in [
                    ("the title block", Some(paper.title_block)),
                    ("the revision table", paper.revisions),
                    ("the stringing table", paper.stringing_table),
                    ("the legend", paper.legend),
                    ("the key map", paper.key_map),
                    ("the scale bars", paper.scale_bars),
                ] {
                    if let Some(r) = r
                        && furniture::overlap(*n, r)
                    {
                        warnings.push(format!(
                            "note {} lies over {what}: move it in the template (its `at`)",
                            k + 1
                        ));
                    }
                }
            }
            if let Some(r) = paper.legend {
                warnings.extend(furniture::legend(items, t, r, &f.legend.borrow()));
            }
            Sheet {
                number: sl.number,
                from: s0,
                to: s1,
                structures: sl.structures,
                page: sl.page,
                drawing,
                warnings,
            }
        })
        .collect()
}

/// The profile (or the options' station range of it) in model space:
/// x = station, y = height × exaggeration (horizontal / vertical scale),
/// metres; texts and grid as on the continuous drawing.
pub fn profile_model(d: &RouteDrawing, o: &Options) -> Drawing {
    let o = Options {
        plan: false,
        ..o.clone()
    };
    let (sheet, f, _) = continuous(d, &o);
    let k = o.horizontal_scale / 1000.0; // m per mm
    let exaggeration = o.horizontal_scale / o.vertical_scale;
    sheet.to_model(
        k,
        [
            f.s0 - f.left * k,
            f.prof_top * k + exaggeration * f.parts[0].z1,
        ],
    )
}

/// The parts of `points` between stations `a` and `b` (station first in
/// each point; the ends interpolated).
pub(crate) fn station_runs<const N: usize>(
    points: &[[f64; N]],
    a: f64,
    b: f64,
) -> Vec<Vec<[f64; N]>> {
    let lerp = |p: &[f64; N], q: &[f64; N], s: f64| {
        let t = if q[0] == p[0] {
            0.0
        } else {
            (s - p[0]) / (q[0] - p[0])
        };
        std::array::from_fn(|i| p[i] + t * (q[i] - p[i]))
    };
    let within = |s: f64| s >= a - 1e-9 && s <= b + 1e-9;
    let mut runs: Vec<Vec<[f64; N]>> = Vec::new();
    let mut current: Vec<[f64; N]> = Vec::new();
    for (k, p) in points.iter().enumerate() {
        if k > 0 {
            let q = &points[k - 1];
            // Crossings of the range ends between q and p, in order.
            let mut cuts: Vec<f64> = [a, b]
                .into_iter()
                .filter(|s| (q[0] - s) * (p[0] - s) < 0.0)
                .collect();
            if p[0] < q[0] {
                cuts.reverse();
            }
            for s in cuts {
                let c = lerp(q, p, s);
                if current.is_empty() {
                    current.push(c);
                } else {
                    current.push(c);
                    runs.push(std::mem::take(&mut current));
                }
            }
        }
        if within(p[0]) {
            current.push(*p);
        } else if !current.is_empty() {
            runs.push(std::mem::take(&mut current));
        }
    }
    if current.len() > 1 {
        runs.push(current);
    }
    runs.retain(|r| r.len() > 1);
    runs
}

/// The plan in model space, in project coordinates (m): centreline,
/// structures (a square and the name), wires and markers, within the
/// options' station range.
pub fn plan_model(d: &RouteDrawing, o: &Options) -> Drawing {
    let d = &d.shown(o.plan_half_width);
    let k = o.horizontal_scale / 1000.0; // m per mm on the sheet
    let (a, b) = o
        .stations
        .map_or((f64::NEG_INFINITY, f64::INFINITY), |[a, b]| {
            (a.min(b), a.max(b))
        });
    let mut items = Vec::new();
    // Centreline with its stations: (station, x, y).
    let mut s = 0.0;
    let mut centre: Vec<[f64; 3]> = Vec::new();
    for (i, p) in d.centreline.iter().enumerate() {
        if i > 0 {
            let q = d.centreline[i - 1];
            s += (p[0] - q[0]).hypot(p[1] - q[1]);
        }
        centre.push([s, p[0], p[1]]);
    }
    for run in station_runs(&centre, a, b) {
        items.push(path(
            Layer::Centreline,
            run.iter().map(|p| [p[1], p[2]]).collect(),
            pen("#999", 0.25).dashed("4 1 1 1"),
            None,
        ));
    }
    for (w, layer, width) in d
        .isoclearance
        .iter()
        .map(|w| (w, Layer::Isoclearance, 0.18))
        .chain(d.wires.iter().map(|w| (w, Layer::Wires, 0.35)))
    {
        let pts: Vec<[f64; 3]> = w
            .points
            .iter()
            .zip(&w.xy)
            .map(|(p, q)| [p[0], q[0], q[1]])
            .collect();
        for run in station_runs(&pts, a, b) {
            if layer == Layer::Isoclearance {
                items.push(path(
                    layer,
                    run.iter().map(|p| [p[1], p[2]]).collect(),
                    pen(&w.color, width),
                    Some(w.label.clone()),
                ));
                continue;
            }
            items.push(path(
                Layer::Wires,
                run.iter().map(|p| [p[1], p[2]]).collect(),
                pen(&w.color, 0.35).dashed(w.dash.as_deref().unwrap_or("")),
                Some(w.label.clone()),
            ));
        }
    }
    for m in d
        .structures
        .iter()
        .filter(|m| m.station >= a - 1e-6 && m.station <= b + 1e-6)
    {
        let [x, y] = m.xy;
        let r = k;
        // The symbol at level 1; the members' footprint and crossarms, a
        // pole's outline at the ground line and the top, at levels 2 and 3.
        if m.members_xy.is_empty() && m.pole_xy.is_empty() {
            items.push(Item::Path(Path {
                layer: Layer::Structures,
                points: vec![
                    [x - r, y - r],
                    [x + r, y - r],
                    [x + r, y + r],
                    [x - r, y + r],
                ],
                closed: true,
                pen: pen("#333", 0.3),
                title: None,
            }));
        }
        for l in &m.members_xy {
            items.push(path(Layer::Members, l.clone(), pen("#555", 0.25), None));
        }
        let ends: Vec<&Vec<[f64; 2]>> = match m.pole_xy.as_slice() {
            [] => Vec::new(),
            [one] => vec![one],
            [first, .., last] => vec![first, last],
        };
        for r in ends {
            items.push(closed(
                Layer::Members,
                r[..r.len().saturating_sub(1)].to_vec(),
                pen("#555", 0.25),
            ));
        }
        items.push(text(
            Layer::Structures,
            [x + 1.5 * k, y + 1.5 * k],
            2.5 * k,
            Anchor::Start,
            m.id.clone(),
        ));
    }
    for (p, label) in &d.markers {
        if p[0] >= a && p[0] <= b {
            let q = ohl_line::route::point_at(&d.centreline, p[0], p[1]);
            items.push(marker(q, 1.2 * k, label));
        }
    }
    let (mut x0, mut y0, mut x1, mut y1) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for p in &d.centreline {
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    Drawing {
        space: crate::model::Space::Model,
        width: (x1 - x0).max(0.0),
        height: (y1 - y0).max(0.0),
        line_scale: k,
        items,
    }
}

/// A page of `page` for the other plans (owner, 2026-10-24: phase plan,
/// route and situation plans): the template's frame and title block,
/// its values as a sheet of route `d` from `from` to `to` (between the
/// structures `ends`) would have them, sheet `number` of `n`, its scale
/// written `scale` (`{scales}`, the sheets' own scales otherwise wrong
/// there); and the room above the title block's row (mm) for the plan's
/// drawing.
pub(crate) fn framed_page(
    d: &RouteDrawing,
    o: &Options,
    page: &Page,
    (number, n): (usize, usize),
    (from, to): (f64, f64),
    ends: (String, String),
    (scale, kind): (&str, &str),
) -> (Drawing, furniture::Rect, furniture::Values) {
    let t = &o.template;
    let paper = Paper::new(d, o, page);
    let sl = SheetLayout {
        number,
        from,
        to,
        reach: to - from,
        structures: ends,
        warnings: Vec::new(),
        page: page.clone(),
        parts: Vec::new(),
    };
    let mut values = sheet_values(d, o, page, &sl, n);
    values.text.insert("scales".into(), scale.to_owned());
    values
        .text
        .insert("plan_kind".into(), o.template.legend.label(kind, &[]));
    let mut drawing = Drawing::sheet(page.width, page.height);
    let items = &mut drawing.items;
    for b in &t.frame.border {
        let r = paper.frame;
        items.push(rect(
            Layer::Frame,
            r[0] + b.inset.0,
            r[1] + b.inset.0,
            r[2] - 2.0 * b.inset.0,
            r[3] - 2.0 * b.inset.0,
            o.template.pen(Layer::Frame, pen("#000", b.width.0)),
        ));
    }
    furniture::title_block(items, t, paper.title_block, &values, &o.logos);
    let inner = paper.inner;
    let room = [
        inner[0],
        inner[1],
        inner[2],
        (paper.title_block[1] - 3.0 - inner[1]).max(10.0),
    ];
    (drawing, room, values)
}

/// A text item for the other plans.
pub(crate) fn plan_text(
    layer: Layer,
    at: [f64; 2],
    size: f64,
    anchor: Anchor,
    t: impl Into<String>,
    is_bold: bool,
) -> Item {
    let item = text(layer, at, size, anchor, t);
    if is_bold { bold(item) } else { item }
}

/// A polyline item for the other plans.
pub(crate) fn plan_path(layer: Layer, points: Vec<[f64; 2]>, color: &str, width: f64) -> Item {
    path(layer, points, pen(color, width), None)
}
