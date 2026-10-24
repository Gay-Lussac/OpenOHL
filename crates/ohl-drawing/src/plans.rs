// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The plans beside the plan-and-profile sheets (owner, 2026-10-23/24,
//! roadmap O14). The **phase plan**: where the phase arrangement changes
//! along the line, and nothing else — each structure's stick figure seen
//! along the line, every wire position labelled with its circuit and
//! phase, drawn only where the arrangement differs from the structure
//! before (a transposition, a circuit leaving or joining, another arm);
//! between, a text "from A to B: arrangement unchanged". Taken from the
//! line as it is drawn (its wires' circuits and phases at the
//! structures): nothing typed twice.

use crate::Page;
use crate::furniture;
use crate::layout::{framed_page, plan_path, plan_text};
use crate::model::{Anchor, Item, Layer};
use crate::{Options, RouteDrawing, Sheet};

/// A wire's position at a structure.
#[derive(Debug, Clone, PartialEq)]
pub struct WirePosition {
    /// Across the line from the structure's axis, m (positive to the
    /// right looking ahead).
    pub offset: f64,
    /// Above the structure's base, m.
    pub height: f64,
    /// The circuit.
    pub circuit: String,
    /// The phase label (empty: none given).
    pub phase: String,
    /// The wire's colour.
    pub color: String,
    /// An earth wire (no voltage).
    pub earth: bool,
}

/// What hangs on one structure, seen along the line.
#[derive(Debug, Clone, PartialEq)]
pub struct Arrangement {
    /// The structure.
    pub structure: String,
    /// Its station.
    pub station: f64,
    /// Its stick figure (or members): polylines of (offset, height), m.
    pub figure: Vec<Vec<[f64; 2]>>,
    /// The wire positions, highest first, then left to right.
    pub wires: Vec<WirePosition>,
}

impl Arrangement {
    /// What the arrangement is, regardless of the structure's size: for
    /// each wire its level (from the top, heights within 1 m one level),
    /// its place on the level from the left, its circuit and phase.
    pub fn signature(&self) -> Vec<(usize, usize, String, String)> {
        let mut levels: Vec<f64> = Vec::new();
        for w in &self.wires {
            if !levels.iter().any(|z| (z - w.height).abs() < 1.0) {
                levels.push(w.height);
            }
        }
        levels.sort_by(|a, b| b.total_cmp(a));
        let level = |h: f64| levels.iter().position(|z| (z - h).abs() < 1.0).unwrap_or(0);
        let mut out = Vec::new();
        for (k, _) in levels.iter().enumerate() {
            let mut on: Vec<&WirePosition> =
                self.wires.iter().filter(|w| level(w.height) == k).collect();
            on.sort_by(|a, b| a.offset.total_cmp(&b.offset));
            for (p, w) in on.into_iter().enumerate() {
                out.push((k, p, w.circuit.clone(), w.phase.clone()));
            }
        }
        out
    }
}

/// The arrangement at every structure on the line (not those beside it),
/// in station order: the wires drawn solid (the case shown) leaving it
/// ahead along the route (the last structure: those arriving), their
/// clamps' positions relative to its axis and base.
pub fn arrangements(d: &RouteDrawing) -> Vec<Arrangement> {
    let mut marks: Vec<&crate::Mark> = d.structures.iter().filter(|m| !m.away).collect();
    marks.sort_by(|a, b| a.station.total_cmp(&b.station));
    let n = marks.len();
    marks
        .into_iter()
        .enumerate()
        .map(|(idx, m)| {
            let ahead = idx + 1 < n;
            let rel = |p: &[f64; 3]| [p[1] - m.offset, p[2] - m.base];
            let mut wires: Vec<WirePosition> = Vec::new();
            for t in d.wires.iter().filter(|t| t.dash.is_none()) {
                let (Some(first), Some(last)) = (t.points.first(), t.points.last()) else {
                    continue;
                };
                // A span (a jumper stays at its structure).
                if (last[0] - first[0]).abs() < 1.0 {
                    continue;
                }
                for (end, other) in [(first, last), (last, first)] {
                    // Its clamp: at a dead end a few metres along the line
                    // from the structure (its tension string).
                    if (end[0] - m.station).abs() > NEAR {
                        continue;
                    }
                    // The span ahead (the last structure: behind).
                    if (other[0] > end[0]) != ahead {
                        continue;
                    }
                    let [offset, height] = rel(end);
                    let same = wires.iter().any(|w| {
                        (w.offset - offset).abs() < 0.05
                            && (w.height - height).abs() < 0.05
                            && w.circuit == t.circuit
                    });
                    if !same {
                        wires.push(WirePosition {
                            offset,
                            height,
                            circuit: t.circuit.clone(),
                            phase: t.phase.clone(),
                            color: t.color.clone(),
                            earth: t.voltage_kv.is_none_or(|v| v <= 0.0),
                        });
                    }
                }
            }
            wires.sort_by(|a, b| {
                b.height
                    .total_cmp(&a.height)
                    .then(a.offset.total_cmp(&b.offset))
            });
            let body = if m.members.is_empty() {
                &m.sticks
            } else {
                &m.members
            };
            let figure = body.iter().map(|l| l.iter().map(rel).collect()).collect();
            Arrangement {
                structure: m.id.clone(),
                station: m.station,
                figure,
                wires,
            }
        })
        .collect()
}

/// What the phase plan shows, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum PhaseStep {
    /// A structure drawn (index in the arrangements): the first, the
    /// last, and each whose arrangement differs from the one before.
    Drawn(usize),
    /// The structures from `from` to `to` (ids) with the arrangement of
    /// the one drawn before.
    Unchanged {
        /// The first.
        from: String,
        /// The last.
        to: String,
    },
}

/// The steps of the phase plan of `a`.
pub fn phase_steps(a: &[Arrangement]) -> Vec<PhaseStep> {
    let mut out = Vec::new();
    let mut run: Option<(String, String)> = None;
    for (k, x) in a.iter().enumerate() {
        let changed = k == 0 || x.signature() != a[k - 1].signature();
        let last = k + 1 == a.len();
        if changed || last {
            if let Some((from, to)) = run.take() {
                out.push(PhaseStep::Unchanged { from, to });
            }
            out.push(PhaseStep::Drawn(k));
        } else {
            match &mut run {
                Some((_, to)) => *to = x.structure.clone(),
                None => run = Some((x.structure.clone(), x.structure.clone())),
            }
        }
    }
    out
}

/// How far along the line a wire's end may be from its structure, m (a
/// dead end's clamp at the end of its tension string; spans are longer).
const NEAR: f64 = 15.0;

/// One cell's width and height on paper, mm.
const CELL: [f64; 2] = [90.0, 110.0];

/// The width of a cell between two drawn structures, mm.
const BETWEEN: f64 = 40.0;

/// The phase plan of route `d` on pages of `page` (the template's frame
/// and title block): the steps left to right, row by row, page by page;
/// every figure at one scale (the largest structure filling its cell).
pub fn phase_plan(d: &RouteDrawing, o: &Options, page: &Page) -> Vec<Sheet> {
    let a = arrangements(d);
    let steps = phase_steps(&a);
    if steps.is_empty() {
        return Vec::new();
    }
    // One scale for every figure, m → mm.
    let (mut w, mut h) = (1.0f64, 1.0f64);
    for x in &a {
        for p in x.figure.iter().flatten().chain(
            x.wires
                .iter()
                .map(|w| [w.offset, w.height])
                .collect::<Vec<_>>()
                .iter(),
        ) {
            w = w.max(2.0 * p[0].abs());
            h = h.max(p[1]);
        }
    }
    let k = ((CELL[0] - 30.0) / w).min((CELL[1] - 22.0) / h);
    let size = 1.8;
    // The circuits numbered in the order they first appear: the key.
    let mut circuits: Vec<String> = Vec::new();
    for x in &a {
        for w in &x.wires {
            if !circuits.contains(&w.circuit) {
                circuits.push(w.circuit.clone());
            }
        }
    }
    let number = |c: &str| circuits.iter().position(|x| x == c).map_or(0, |k| k + 1);
    let key_cols = 3usize;
    let key_rows = circuits.len().div_ceil(key_cols).max(1);
    let key_h = key_rows as f64 * size * 1.6 + 6.0;
    // The steps laid out on pages: (page, x, y, width) of each.
    let (_, room, _) = framed_page(
        d,
        o,
        page,
        (1, 1),
        (0.0, d.length),
        (String::new(), String::new()),
        "–",
    );
    let room = [room[0], room[1], room[2], room[3] - key_h];
    let mut placed: Vec<(usize, [f64; 2], f64)> = Vec::new();
    let (mut p, mut x, mut y) = (0usize, room[0], room[1]);
    for s in &steps {
        let width = match s {
            PhaseStep::Drawn(_) => CELL[0],
            PhaseStep::Unchanged { .. } => BETWEEN,
        };
        if x + width > room[0] + room[2] {
            x = room[0];
            y += CELL[1];
        }
        if y + CELL[1] > room[1] + room[3] {
            p += 1;
            x = room[0];
            y = room[1];
        }
        placed.push((p, [x, y], width));
        x += width;
    }
    let pages = p + 1;
    let t = &o.template;
    let mut out = Vec::new();
    for page_no in 0..pages {
        let on: Vec<(&PhaseStep, [f64; 2])> = steps
            .iter()
            .zip(&placed)
            .filter(|(_, (pp, ..))| *pp == page_no)
            .map(|(s, (_, at, _))| (s, *at))
            .collect();
        let ids = |s: &PhaseStep| match s {
            PhaseStep::Drawn(i) => (a[*i].structure.clone(), a[*i].station),
            PhaseStep::Unchanged { from, .. } => (
                from.clone(),
                a.iter()
                    .find(|x| &x.structure == from)
                    .map_or(0.0, |x| x.station),
            ),
        };
        let (first, s0) = on.first().map(|(s, _)| ids(s)).unwrap_or_default();
        let (last, s1) = on.last().map(|(s, _)| ids(s)).unwrap_or_default();
        let (mut drawing, _, _) = framed_page(
            d,
            o,
            page,
            (page_no + 1, pages),
            (s0, s1),
            (first, last),
            "–",
        );
        let items = &mut drawing.items;
        // The key of the circuits, under the figures.
        let key_top = room[1] + room[3] + 3.0;
        let col_w = room[2] / key_cols as f64;
        for (c, name) in circuits.iter().enumerate() {
            let (r, col) = (c % key_rows, c / key_rows);
            items.push(plan_text(
                Layer::Labels,
                [
                    room[0] + col as f64 * col_w,
                    key_top + (r as f64 + 1.0) * size * 1.6,
                ],
                size,
                Anchor::Start,
                format!("{}  {}", c + 1, name),
                false,
            ));
        }
        for (s, at) in on {
            match s {
                PhaseStep::Drawn(i) => draw_arrangement(items, &a[*i], at, k, size, &number),
                PhaseStep::Unchanged { from, to } => {
                    let text = t
                        .legend
                        .label("phase_unchanged", &[("from", from), ("to", to)]);
                    let c = [at[0] + BETWEEN / 2.0, at[1] + CELL[1] / 2.0];
                    for (n, line) in furniture::wrap(&text, size, false, BETWEEN - 6.0)
                        .iter()
                        .enumerate()
                    {
                        items.push(plan_text(
                            Layer::Labels,
                            [c[0], c[1] + n as f64 * size * 1.4],
                            size,
                            Anchor::Middle,
                            line.clone(),
                            false,
                        ));
                    }
                    items.push(plan_path(
                        Layer::Centreline,
                        vec![
                            [at[0] + 3.0, c[1] - 6.0],
                            [at[0] + BETWEEN - 3.0, c[1] - 6.0],
                        ],
                        "#888",
                        0.25,
                    ));
                }
            }
        }
        out.push(Sheet {
            number: page_no + 1,
            from: s0,
            to: s1,
            structures: (String::new(), String::new()),
            page: page.clone(),
            drawing,
            warnings: Vec::new(),
        });
    }
    out
}

/// One structure's figure in its cell at `at` (top left), `k` mm per m:
/// its id on top, its stick figure, each wire position a dot in its
/// colour with its circuit and phase beside it, outward.
fn draw_arrangement(
    items: &mut Vec<Item>,
    x: &Arrangement,
    at: [f64; 2],
    k: f64,
    size: f64,
    number: &dyn Fn(&str) -> usize,
) {
    let base = [at[0] + CELL[0] / 2.0, at[1] + CELL[1] - 8.0];
    let to = |p: [f64; 2]| [base[0] + p[0] * k, base[1] - p[1] * k];
    items.push(plan_text(
        Layer::Labels,
        [base[0], at[1] + 5.0],
        size * 1.3,
        Anchor::Middle,
        x.structure.clone(),
        true,
    ));
    for l in &x.figure {
        if l.len() >= 2 {
            items.push(plan_path(
                Layer::Structures,
                l.iter().map(|p| to(*p)).collect(),
                "#000",
                0.25,
            ));
        }
    }
    items.push(plan_path(
        Layer::Ground,
        vec![[base[0] - 12.0, base[1]], [base[0] + 12.0, base[1]]],
        "#000",
        0.35,
    ));
    for w in &x.wires {
        let c = to([w.offset, w.height]);
        items.push(Item::Circle(crate::model::Circle {
            layer: Layer::Wires,
            centre: c,
            radius: 0.9,
            stroke: None,
            fill: Some(crate::model::Color::hex_or_black(&w.color)),
            title: None,
        }));
        // The circuit by its number in the key, then the phase.
        let label = format!("{} {}", number(&w.circuit), w.phase)
            .trim()
            .to_owned();
        let (dx, anchor) = if w.offset < 0.0 {
            (-1.6, Anchor::End)
        } else {
            (1.6, Anchor::Start)
        };
        items.push(plan_text(
            Layer::Labels,
            [c[0] + dx, c[1] + 0.35 * size],
            size,
            anchor,
            label,
            false,
        ));
    }
}

/// Which plan of the route on a map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapKind {
    /// The route plan: the whole route at a small scale (1:5000 to
    /// 1:25 000), on as few sheets as it needs.
    Route,
    /// Situation plans: the route in parts at a large scale (1:500 to
    /// 1:2000), with the parcels around the line.
    Situation,
}

impl MapKind {
    /// Its name in files: `route-plan` or `situation-plan`.
    pub fn file_name(self) -> &'static str {
        match self {
            MapKind::Route => "route-plan",
            MapKind::Situation => "situation-plan",
        }
    }

    /// The scales it is drawn at (1:…).
    pub fn scales(self) -> std::ops::RangeInclusive<f64> {
        match self {
            MapKind::Route => 5000.0..=25_000.0,
            MapKind::Situation => 500.0..=2000.0,
        }
    }
}

/// How a route or situation plan is drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct MapPlan {
    /// Which plan.
    pub kind: MapKind,
    /// Its scale (1:…).
    pub scale: f64,
    /// The corridor's half width drawn along the line, m (none: not).
    pub corridor: Option<f64>,
    /// The project's drawings under it (`map_background`) and its
    /// orthophotos (the route's photo).
    pub drawings: bool,
    /// The orthophotos under it, under a white veil (0 to 1).
    pub photo: Option<f64>,
    /// Each sheet overlaps the one before by this much of its width.
    pub overlap: f64,
}

impl MapPlan {
    /// The plan of `kind` at its usual scale (route 1:10 000, situation
    /// 1:1000), drawings and photo on, the corridor at `corridor` m.
    pub fn new(kind: MapKind, corridor: Option<f64>) -> Self {
        Self {
            kind,
            scale: match kind {
                MapKind::Route => 10_000.0,
                MapKind::Situation => 1000.0,
            },
            corridor,
            drawings: true,
            photo: Some(0.4),
            overlap: 0.1,
        }
    }
}

/// The centreline with no piece longer than `step`, m.
fn densified(c: &[[f64; 3]], step: f64) -> Vec<[f64; 2]> {
    let mut out: Vec<[f64; 2]> = Vec::new();
    for w in c.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        let n = (len / step).ceil().max(1.0) as usize;
        for k in 0..n {
            let t = k as f64 / n as f64;
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    if let Some(l) = c.last() {
        out.push([l[0], l[1]]);
    }
    out
}

/// Where the sheets of a map go: their middles (x, y), m. The route is
/// walked and each sheet takes as much of it as fits within its room
/// (`w` × `h` m less a margin), the next starting `overlap` of its length
/// back.
fn map_frames(line: &[[f64; 2]], w: f64, h: f64, overlap: f64) -> Vec<[f64; 2]> {
    let (mw, mh) = (w * 0.9, h * 0.9);
    let mut out = Vec::new();
    let mut start = 0usize;
    while start + 1 < line.len() {
        let (mut lo, mut hi) = (line[start], line[start]);
        let mut end = start;
        while end + 1 < line.len() {
            let p = line[end + 1];
            let (nlo, nhi) = (
                [lo[0].min(p[0]), lo[1].min(p[1])],
                [hi[0].max(p[0]), hi[1].max(p[1])],
            );
            if nhi[0] - nlo[0] > mw || nhi[1] - nlo[1] > mh {
                break;
            }
            (lo, hi) = (nlo, nhi);
            end += 1;
        }
        out.push([(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0]);
        if end + 1 >= line.len() {
            break;
        }
        let back = (((end - start) as f64) * overlap).round() as usize;
        start = (end - back).max(start + 1);
    }
    out
}

/// A round length for the scale bar: the largest of 1, 2, 5 × 10ⁿ m at
/// most `max` mm long at `k` mm per m.
fn round_length(max: f64, k: f64) -> f64 {
    let mut best = 1.0;
    for e in 0..6 {
        for m in [1.0, 2.0, 5.0] {
            let l = m * 10f64.powi(e);
            if l * k <= max {
                best = l;
            }
        }
    }
    best
}

/// The route or situation plan of route `d` on pages of `page`: north
/// up at `m.scale`, the sheets along the route (overlapping), each with
/// the project's drawings and orthophotos as asked, the corridor, the
/// line's axis, its wires, its structures numbered, a north arrow, a
/// scale bar and a legend, in the template's frame and title block.
pub fn map_plan(d: &RouteDrawing, o: &Options, page: &Page, m: &MapPlan) -> Vec<Sheet> {
    if d.centreline.len() < 2 {
        return Vec::new();
    }
    let k = 1000.0 / m.scale; // mm per m
    let scale_text = format!("1:{}", crate::template::format_number(m.scale, ""));
    let (_, room, _) = framed_page(
        d,
        o,
        page,
        (1, 1),
        (0.0, d.length),
        (String::new(), String::new()),
        &scale_text,
    );
    let (w, h) = (room[2] / k, room[3] / k);
    let line = densified(&d.centreline, w.min(h) / 8.0);
    let frames = map_frames(&line, w, h, m.overlap.clamp(0.0, 0.5));
    let n = frames.len();
    let t = &o.template;
    let size = 1.8;
    let mut out = Vec::new();
    for (q, c) in frames.iter().enumerate() {
        let mid = [room[0] + room[2] / 2.0, room[1] + room[3] / 2.0];
        let to = |p: [f64; 2]| [mid[0] + (p[0] - c[0]) * k, mid[1] - (p[1] - c[1]) * k];
        // The stations the sheet shows (for its title block).
        let inside = |p: [f64; 2]| crate::model::inside(to(p), room);
        let on: Vec<&crate::Mark> = d
            .structures
            .iter()
            .filter(|s| !s.away && inside(s.xy))
            .collect();
        let s0 = on.iter().map(|s| s.station).fold(f64::INFINITY, f64::min);
        let s1 = on
            .iter()
            .map(|s| s.station)
            .fold(f64::NEG_INFINITY, f64::max);
        let first = on.iter().min_by(|a, b| a.station.total_cmp(&b.station));
        let last = on.iter().max_by(|a, b| a.station.total_cmp(&b.station));
        let (mut drawing, _, _) = framed_page(
            d,
            o,
            page,
            (q + 1, n),
            (
                if s0.is_finite() { s0 } else { 0.0 },
                if s1.is_finite() { s1 } else { d.length },
            ),
            (
                first.map_or(String::new(), |s| s.id.clone()),
                last.map_or(String::new(), |s| s.id.clone()),
            ),
            &scale_text,
        );
        let items = &mut drawing.items;
        let mut legend: Vec<(String, String)> = Vec::new();
        if let (Some(veil), Some(photo)) = (m.photo, &d.photo)
            && let Some(img) = crate::background::north_up_photo(
                photo,
                o,
                room,
                (c[0], c[1], k),
                veil,
                Layer::BackgroundPhoto,
            )
        {
            items.push(img);
            legend.push(("#bbb".into(), photo.name.clone()));
        }
        let clip = |pts: Vec<[f64; 2]>| crate::model::clip_polyline(&pts, room);
        if m.drawings {
            for dr in d.map_background.iter() {
                for (l, color) in &dr.lines {
                    for run in clip(l.iter().map(|p| to(*p)).collect()) {
                        items.push(Item::Path(crate::model::Path {
                            layer: Layer::BackgroundDrawings,
                            points: run,
                            closed: false,
                            pen: crate::model::Pen::new(*color, 0.13),
                            title: None,
                        }));
                    }
                }
                for (at, height, text, color) in &dr.texts {
                    let p = to(*at);
                    if crate::model::inside(p, room) {
                        items.push(Item::Text(crate::model::Text {
                            layer: Layer::BackgroundDrawings,
                            at: p,
                            text: text.clone(),
                            size: (height * k).clamp(1.2, 3.0),
                            anchor: Anchor::Start,
                            bold: false,
                            color: *color,
                        }));
                    }
                }
                legend.push(("#8c8c8c".into(), dr.name.clone()));
            }
        }
        // The corridor's edges either side of the axis.
        if let Some(half) = m.corridor.filter(|x| *x > 0.0) {
            for side in [-1.0, 1.0] {
                let edge: Vec<[f64; 2]> = line
                    .windows(2)
                    .map(|p| {
                        let (dx, dy) = (p[1][0] - p[0][0], p[1][1] - p[0][1]);
                        let l = dx.hypot(dy).max(1e-9);
                        to([
                            p[0][0] + side * half * dy / l,
                            p[0][1] - side * half * dx / l,
                        ])
                    })
                    .collect();
                for run in clip(edge) {
                    items.push(plan_path(Layer::Corridor, run, "#2a7", 0.25));
                }
            }
            legend.push((
                "#2a7".into(),
                t.legend.label(
                    "corridor_near",
                    &[
                        ("side", "±"),
                        ("width", &crate::template::format_number(half, "")),
                    ],
                ),
            ));
        }
        for run in clip(line.iter().map(|p| to(*p)).collect()) {
            items.push(plan_path(Layer::Centreline, run, "#000", 0.25));
        }
        legend.push(("#000".into(), t.legend.label("axis", &[])));
        for tr in d.wires.iter().filter(|x| x.dash.is_none()) {
            for run in clip(tr.xy.iter().map(|p| to(*p)).collect()) {
                items.push(plan_path(Layer::Wires, run, &tr.color, 0.18));
            }
        }
        for s in &d.structures {
            let p = to(s.xy);
            if !crate::model::inside(p, room) {
                continue;
            }
            let r = 0.9;
            items.push(plan_path(
                Layer::Structures,
                vec![
                    [p[0] - r, p[1] - r],
                    [p[0] + r, p[1] - r],
                    [p[0] + r, p[1] + r],
                    [p[0] - r, p[1] + r],
                    [p[0] - r, p[1] - r],
                ],
                "#000",
                0.3,
            ));
            items.push(plan_text(
                Layer::Labels,
                [p[0] + 1.6, p[1] - 1.4],
                size,
                Anchor::Start,
                s.id.clone(),
                true,
            ));
        }
        // North arrow (top right), scale bar (bottom left), legend.
        furniture::north_arrow(
            items,
            [room[0] + room[2] - 10.0, room[1] + 18.0],
            [0.0, -1.0],
            12.0,
            Layer::Labels,
        );
        let len = round_length(room[2] / 4.0, k);
        furniture::scale_bar(
            items,
            [room[0] + 6.0, room[1] + room[3] - 6.0],
            len,
            k,
            &format!("1:{}", crate::template::format_number(m.scale, "")),
        );
        let mut y = room[1] + room[3] - 16.0 - legend.len() as f64 * size * 1.6;
        for (color, name) in &legend {
            items.push(plan_path(
                Layer::Legend,
                vec![[room[0] + 6.0, y - 0.6], [room[0] + 14.0, y - 0.6]],
                color,
                0.5,
            ));
            items.push(plan_text(
                Layer::Legend,
                [room[0] + 16.0, y],
                size,
                Anchor::Start,
                name.clone(),
                false,
            ));
            y += size * 1.6;
        }
        out.push(Sheet {
            number: q + 1,
            from: if s0.is_finite() { s0 } else { 0.0 },
            to: if s1.is_finite() { s1 } else { d.length },
            structures: (
                first.map_or(String::new(), |s| s.id.clone()),
                last.map_or(String::new(), |s| s.id.clone()),
            ),
            page: page.clone(),
            drawing,
            warnings: Vec::new(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::main_route;

    /// The example line's phase plan: every structure on the line has its
    /// wires; the first and the last are drawn; a structure with the same
    /// arrangement as the one before is not drawn but said; one with its
    /// wires swapped (a transposition) is drawn; the sheets carry the
    /// template's frame and the figures' labels.
    #[test]
    fn the_phase_plan_draws_the_changes_only() {
        let d = main_route(&Options::default());
        let a = arrangements(&d);
        assert!(a.len() >= 3, "{}", a.len());
        assert!(a.iter().all(|x| !x.wires.is_empty()), "{a:?}");
        let steps = phase_steps(&a);
        assert_eq!(steps.first(), Some(&PhaseStep::Drawn(0)));
        assert_eq!(steps.last(), Some(&PhaseStep::Drawn(a.len() - 1)));
        // Every structure either drawn or in a run said unchanged.
        for (k, x) in a.iter().enumerate() {
            let drawn = steps.contains(&PhaseStep::Drawn(k));
            let said = steps
                .iter()
                .any(|s| matches!(s, PhaseStep::Unchanged { .. }));
            assert!(drawn || said, "{} neither", x.structure);
        }
        // The same arrangements throughout: only the ends drawn.
        let same: Vec<Arrangement> = (0..4)
            .map(|k| Arrangement {
                structure: format!("T{k}"),
                ..a[0].clone()
            })
            .collect();
        assert_eq!(
            phase_steps(&same),
            vec![
                PhaseStep::Drawn(0),
                PhaseStep::Unchanged {
                    from: "T1".into(),
                    to: "T2".into()
                },
                PhaseStep::Drawn(3)
            ]
        );
        // Two wires of one level swapped (a transposition): drawn.
        let mut swapped = same.clone();
        let w = &mut swapped[2].wires;
        if w.len() >= 2 {
            let (p0, p1) = (w[0].phase.clone(), w[1].phase.clone());
            let (c0, c1) = (w[0].circuit.clone(), w[1].circuit.clone());
            w[0].phase = format!("{p1}x");
            w[1].phase = format!("{p0}x");
            w[0].circuit = c1;
            w[1].circuit = c0;
            assert!(phase_steps(&swapped).contains(&PhaseStep::Drawn(2)));
        }
        let sheets = phase_plan(&d, &Options::default(), &crate::page_format("A3").unwrap());
        assert!(!sheets.is_empty());
        let svg: String = sheets.iter().map(Sheet::svg).collect();
        assert!(
            svg.contains(&format!(">{}<", a[0].structure)),
            "the first's id"
        );
    }

    /// The route and situation plans of the example line: every structure
    /// on a sheet, north up; the situation plan at 1:1000 needs more
    /// sheets than the route plan at 1:10 000; the sheets overlap; a
    /// drawing near the line is drawn, its text too.
    #[test]
    fn the_route_and_situation_plans_cover_the_line() {
        let mut d = main_route(&Options::default());
        let mut survey = ohl_background::Drawing::default();
        let p0 = d.structures[0].xy;
        survey.lines.push(ohl_background::dxf::Polyline {
            layer: 0,
            color: [10, 20, 30],
            points: vec![
                [p0[0] - 20.0, p0[1] - 20.0, 0.0],
                [p0[0] + 20.0, p0[1] + 20.0, 0.0],
            ],
            closed: false,
        });
        survey.texts.push(ohl_background::dxf::Label {
            layer: 0,
            color: [10, 20, 30],
            at: [p0[0] + 5.0, p0[1] + 5.0, 0.0],
            height: 2.0,
            angle_deg: 0.0,
            text: "Parcel 4711".into(),
        });
        d.add_map_drawing("survey", &survey, 60.0, 0.05);
        assert_eq!(d.map_background.len(), 1);
        let page = crate::page_format("A3").unwrap();
        let o = Options::default();
        let route = map_plan(&d, &o, &page, &MapPlan::new(MapKind::Route, Some(50.0)));
        let situation = map_plan(&d, &o, &page, &MapPlan::new(MapKind::Situation, Some(50.0)));
        assert!(!route.is_empty());
        assert!(
            situation.len() > route.len(),
            "{} {}",
            situation.len(),
            route.len()
        );
        for plan in [&route, &situation] {
            let svg: String = plan.iter().map(Sheet::svg).collect();
            for m in d.structures.iter().filter(|m| !m.away) {
                assert!(svg.contains(&format!(">{}<", m.id)), "{} missing", m.id);
            }
        }
        let first: String = situation[0].svg();
        assert!(first.contains(">Parcel 4711<"), "the drawing's text");
        // Its own scale in the title block, not the sheets'.
        assert!(first.contains(">1:1000<"), "the plan's scale");
    }
}
