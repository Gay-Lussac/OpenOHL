// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Layout of plan and profile on sheets: one continuous drawing, sheets of
//! a page format split at structures, and model-space drawings (plan in
//! project coordinates, profile by station and exaggerated height).

use crate::model::{Anchor, Circle, Clip, Color, Drawing, Image, Item, Layer, Path, Pen, Text};
use crate::{ElevationPlace, Options, Page, RouteDrawing, format_station, nice_step};

/// Where things go on a sheet (all mm, except stations and heights).
pub(crate) struct Frame {
    /// Station range drawn.
    pub s0: f64,
    pub s1: f64,
    /// Content is clipped at this station (the end of a station range).
    pub content_end: f64,
    /// Left edge of the drawing area.
    pub left: f64,
    /// The plan panel is drawn.
    pub plan: bool,
    /// Plan panel top and height.
    pub plan_top: f64,
    pub plan_h: f64,
    /// Profile panel top and height.
    pub prof_top: f64,
    pub prof_h: f64,
    /// Height range of the profile.
    pub z0: f64,
    pub z1: f64,
    /// mm per m: along the route, across in the plan, vertical.
    pub h: f64,
    pub plan_k: f64,
    pub v: f64,
}

impl Frame {
    fn x(&self, s: f64) -> f64 {
        self.left + (s - self.s0) * self.h
    }
    fn plan_y(&self, offset: f64) -> f64 {
        self.plan_top + self.plan_h / 2.0 + offset * self.plan_k
    }
    fn y(&self, z: f64) -> f64 {
        self.prof_top + (self.z1 - z) * self.v
    }
    fn width(&self) -> f64 {
        (self.s1 - self.s0) * self.h
    }
    fn content_width(&self) -> f64 {
        ((self.content_end.min(self.s1) - self.s0) * self.h).max(0.0)
    }
}

/// Height range of what lies between stations `s0` and `s1`, rounded to
/// grid steps (with one step of room on top).
pub(crate) fn height_range(d: &RouteDrawing, s0: f64, s1: f64) -> (f64, f64, f64) {
    let (mut z0, mut z1) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut see = |s: f64, z: f64| {
        if s >= s0 - 1e-6 && s <= s1 + 1e-6 && z.is_finite() {
            z0 = z0.min(z);
            z1 = z1.max(z);
        }
    };
    for m in &d.structures {
        see(m.station, m.base);
        see(m.station, m.top);
        for p in m.sticks.iter().chain(&m.members).chain(&m.pole).flatten() {
            see(p[0], p[2]);
        }
    }
    for w in &d.wires {
        for p in &w.points {
            see(p[0], p[2]);
        }
    }
    for run in d.ground.iter().chain(d.sides.iter().flat_map(|(_, p)| p)) {
        for p in run {
            see(p[0], p[1]);
        }
    }
    if !z0.is_finite() {
        (z0, z1) = (0.0, 10.0);
    }
    let step = nice_step((z1 - z0).max(1.0), 8.0);
    (
        (z0 / step).floor() * step,
        (z1 / step).ceil() * step + step,
        step,
    )
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

/// Grids, panels and content of one frame (clipped to its panels; the
/// structures' elevations, when drawn, not clipped); what to warn about.
fn draw(
    items: &mut Vec<Item>,
    d: &RouteDrawing,
    o: &Options,
    f: &Frame,
    z_step: f64,
) -> Vec<String> {
    let (s0, s1) = (f.s0, f.s1);
    let grid = pen("#ddd", 0.2);
    // Station grid in both panels, labels under the profile.
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
            vec![[x, f.prof_top], [x, f.prof_top + f.prof_h]],
            grid.clone(),
            None,
        ));
        items.push(text(
            Layer::Labels,
            [x, f.prof_top + f.prof_h + 4.0],
            2.5,
            Anchor::Middle,
            format_station(s),
        ));
    }
    for z in ticks(f.z0, f.z1, z_step) {
        items.push(path(
            Layer::Grid,
            vec![[f.left, f.y(z)], [f.left + f.width(), f.y(z)]],
            grid.clone(),
            None,
        ));
        items.push(text(
            Layer::Labels,
            [f.left - 1.5, f.y(z) + 0.9],
            2.5,
            Anchor::End,
            format!("{z:.0}"),
        ));
    }
    let panel = pen("#888", 0.3);
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
    if f.plan {
        items.push(bold(text(
            Layer::Labels,
            [f.left + 1.0, f.plan_top + 3.5],
            3.0,
            Anchor::Start,
            "Plan",
        )));
    }
    items.push(bold(text(
        Layer::Labels,
        [f.left + 1.0, f.prof_top + 3.5],
        3.0,
        Anchor::Start,
        "Profile",
    )));

    // Names of the structures shown are written whole, outside the clip
    // regions (a name at the end of a station range is not cut in half).
    let plan_rect = [f.left, f.plan_top, f.content_width(), f.plan_h];
    let prof_rect = [f.left, f.prof_top, f.content_width(), f.prof_h];
    let mut names = Vec::new();
    let mut label = |clipped: &mut Vec<Item>, item: Item, rect: [f64; 4]| match &item {
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
    // Plan.
    if f.plan {
        let mut plan = Vec::new();
        plan.push(path(
            Layer::Centreline,
            vec![[f.x(0.0), f.plan_y(0.0)], [f.x(d.length), f.plan_y(0.0)]],
            pen("#999", 0.25).dashed("4 1 1 1"),
            None,
        ));
        for w in &d.wires {
            plan.push(path(
                Layer::Wires,
                w.points
                    .iter()
                    .map(|p| [f.x(p[0]), f.plan_y(p[1])])
                    .collect(),
                pen(&w.color, 0.35).dashed(w.dash.as_deref().unwrap_or("")),
                Some(w.label.clone()),
            ));
        }
        for m in &d.structures {
            let (cx, cy) = (f.x(m.station), f.plan_y(m.offset));
            // The symbol at level 1; members and poles draw the others.
            if !m.has_body() {
                plan.push(rect(
                    Layer::Structures,
                    cx - 1.0,
                    cy - 1.0,
                    2.0,
                    2.0,
                    pen("#333", 0.3),
                ));
            }
            let name = text(
                Layer::Structures,
                [cx, f.plan_top + f.plan_h - 1.5],
                2.5,
                Anchor::Middle,
                m.id.clone(),
            );
            label(&mut plan, name, plan_rect);
        }
        // Stick drawings seen from above; members (levels 2 and 3): the
        // footprint and the crossarms.
        for m in &d.structures {
            let (layer, lines) = if m.members.is_empty() {
                (Layer::Structures, &m.sticks)
            } else {
                (Layer::Members, &m.members)
            };
            for stick in lines {
                plan.push(path(
                    layer,
                    stick.iter().map(|p| [f.x(p[0]), f.plan_y(p[1])]).collect(),
                    pen("#555", 0.25),
                    None,
                ));
            }
            // A pole's outline at the ground line and at its top.
            for ring in pole_ends(&m.pole) {
                plan.push(closed(
                    Layer::Members,
                    ring.iter().map(|p| [f.x(p[0]), f.plan_y(p[1])]).collect(),
                    pen("#555", 0.25),
                ));
            }
        }
        for (p, label) in &d.markers {
            plan.push(marker([f.x(p[0]), f.plan_y(p[1])], 1.2, label));
        }
        items.push(Item::Clip(Clip {
            rect: plan_rect,
            items: plan,
        }));
    }

    // Profile.
    let mut prof = Vec::new();
    for (offset, runs) in &d.sides {
        for run in runs {
            prof.push(path(
                Layer::GroundSide,
                run.iter().map(|p| [f.x(p[0]), f.y(p[1])]).collect(),
                pen("#b08850", 0.25).dashed("1.5 1"),
                Some(format!("ground at offset {offset:+.1} m")),
            ));
        }
    }
    for run in &d.ground {
        prof.push(path(
            Layer::Ground,
            run.iter().map(|p| [f.x(p[0]), f.y(p[1])]).collect(),
            pen("#7a4a1a", 0.45),
            Some("ground (centreline)".into()),
        ));
        if let Some(c) = o.clearance {
            prof.push(path(
                Layer::Clearance,
                run.iter().map(|p| [f.x(p[0]), f.y(p[1] + c)]).collect(),
                pen("#d22", 0.3).dashed("3 1.5"),
                Some(format!("ground + {c} m")),
            ));
        }
    }
    for m in &d.structures {
        let cx = f.x(m.station);
        // The symbol at level 1; members and poles draw the others (their
        // names over the highest member or the pole's top).
        if !m.has_body() {
            prof.push(path(
                Layer::Structures,
                vec![[cx, f.y(m.base)], [cx, f.y(m.top)]],
                pen("#333", 0.6),
                None,
            ));
        }
        let top = m
            .members
            .iter()
            .chain(&m.pole)
            .flatten()
            .map(|p| p[2])
            .fold(m.top, f64::max);
        let name = text(
            Layer::Structures,
            [cx, f.y(top) - 4.0],
            2.5,
            Anchor::Middle,
            m.id.clone(),
        );
        label(&mut prof, name, prof_rect);
        if let Item::Text(mut t) = text(
            Layer::Structures,
            [cx, f.y(top) - 1.5],
            2.0,
            Anchor::Middle,
            format_station(m.station),
        ) {
            t.color = hex("#555");
            label(&mut prof, Item::Text(t), prof_rect);
        }
        for a in &m.attachments {
            prof.push(Item::Circle(Circle {
                layer: Layer::Structures,
                centre: [f.x(a[0]), f.y(a[2])],
                radius: 0.4,
                stroke: None,
                fill: Some(hex("#333")),
                title: None,
            }));
        }
    }
    // Stick drawings seen along the route; members (levels 2 and 3): the
    // silhouette projected on the profile plane.
    for m in &d.structures {
        let (layer, lines) = if m.members.is_empty() {
            (Layer::Structures, &m.sticks)
        } else {
            (Layer::Members, &m.members)
        };
        for stick in lines {
            prof.push(path(
                layer,
                stick.iter().map(|p| [f.x(p[0]), f.y(p[2])]).collect(),
                pen("#555", 0.3),
                Some(m.id.clone()),
            ));
        }
        // A pole: its silhouette by height.
        let outline = silhouette(&m.pole, 0);
        if !outline.is_empty() {
            prof.push(Item::Path(Path {
                layer: Layer::Members,
                points: outline.iter().map(|p| [f.x(p[0]), f.y(p[1])]).collect(),
                closed: true,
                pen: pen("#555", 0.3),
                title: Some(m.id.clone()),
            }));
        }
    }
    for w in &d.wires {
        prof.push(path(
            Layer::Wires,
            w.points.iter().map(|p| [f.x(p[0]), f.y(p[2])]).collect(),
            pen(&w.color, 0.35).dashed(w.dash.as_deref().unwrap_or("")),
            Some(w.label.clone()),
        ));
    }
    for (p, label) in &d.markers {
        prof.push(marker([f.x(p[0]), f.y(p[2])], 1.2, label));
    }
    items.push(Item::Clip(Clip {
        rect: prof_rect,
        items: prof,
    }));
    items.extend(names);
    draw_elevations(items, d, o, f)
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
}

/// The structures' elevations on the plan of `d` with options `o`
/// ([`crate::Elevations`]; none when off or without a plan): every
/// structure drawn along the line ([`RouteDrawing::shown`]) seen along the
/// line — its members, its pole's silhouette, else its stick drawing (a
/// mast line without one) — true shape at the horizontal scale, its base
/// at its station `offset_mm` on paper above or below the line, its height
/// at `rotation_deg` to the line pointing away from it (below: the same
/// turned half a turn), in the route's (station, offset), m.
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
    let plan_k = 1000.0 / o.plan_offset_scale.unwrap_or(o.horizontal_scale);
    let (sin, cos) = e.rotation_deg.to_radians().sin_cos();
    // On paper (y down, offsets right of the line down): the height away
    // from the line, the width across it.
    let up = [side * cos, -side * sin];
    let across = [-up[1], up[0]];
    // A point `dx`, `dy` mm on paper from `at`, in the route's metres.
    let move_by = |at: [f64; 2], dx: f64, dy: f64| [at[0] + dx / h, at[1] + dy / plan_k];
    d.structures
        .iter()
        .map(|m| {
            let base = [m.station, -side * e.offset_mm / plan_k];
            let at = |u: f64, z: f64| {
                let (u, z) = (u * h, (z - m.base) * h);
                move_by(base, u * across[0] + z * up[0], u * across[1] + z * up[1])
            };
            // Seen along the line: across it (the offset) and up.
            let drawn = if m.members.is_empty() {
                &m.sticks
            } else {
                &m.members
            };
            let mut lines: Vec<(Vec<[f64; 2]>, bool)> = drawn
                .iter()
                .map(|l| (l.iter().map(|p| at(p[1] - m.offset, p[2])).collect(), false))
                .collect();
            let outline = silhouette(&m.pole, 1);
            if !outline.is_empty() {
                lines.push((
                    outline.iter().map(|p| at(p[0] - m.offset, p[1])).collect(),
                    true,
                ));
            }
            if lines.is_empty() {
                lines.push((vec![at(0.0, m.base), at(0.0, m.top)], false));
            }
            let stand = [m.station, m.offset];
            let paper = ((stand[0] - base[0]) * h).hypot((stand[1] - base[1]) * plan_k);
            ElevationDrawing {
                id: m.id.clone(),
                station: m.station,
                lines,
                leader: (paper > 0.5).then_some([stand, base]),
            }
        })
        .collect()
}

/// The elevations of `d` (drawn already) within frame `f`'s station range,
/// on paper: each one's lines, leader and box.
#[allow(clippy::type_complexity)]
fn elevations_on_paper(
    d: &RouteDrawing,
    o: &Options,
    f: &Frame,
) -> Vec<(ElevationDrawing, Vec<(Vec<[f64; 2]>, bool)>, [f64; 4])> {
    if !f.plan || o.elevations.place == ElevationPlace::Off {
        return Vec::new();
    }
    let end = f.content_end.min(f.s1);
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
            (e, lines, bb)
        })
        .collect()
}

/// What to warn about the elevations of `d` in frame `f`: two that
/// overlap, one reaching into the profile (both kept, drawn anyway).
fn elevation_warnings(d: &RouteDrawing, o: &Options, f: &Frame) -> Vec<String> {
    let boxes: Vec<(String, [f64; 4])> = elevations_on_paper(d, o, f)
        .into_iter()
        .map(|(e, _, bb)| (e.id, bb))
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
    for (a, ba) in &boxes {
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
/// clipped, a thin dashed leader from where each structure stands; what to
/// warn about ([`elevation_warnings`]).
fn draw_elevations(items: &mut Vec<Item>, d: &RouteDrawing, o: &Options, f: &Frame) -> Vec<String> {
    for (e, lines, _) in elevations_on_paper(d, o, f) {
        for (points, closed) in lines {
            items.push(Item::Path(Path {
                layer: Layer::Elevations,
                points,
                closed,
                pen: pen("#555", 0.2),
                title: Some(format!("{} elevation", e.id)),
            }));
        }
        if let Some([a, b]) = e.leader {
            items.push(path(
                Layer::Elevations,
                vec![[f.x(a[0]), f.plan_y(a[1])], [f.x(b[0]), f.plan_y(b[1])]],
                pen("#888", 0.13).dashed("1 0.7"),
                None,
            ));
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
    format!(
        "Route {} {} — {}",
        d.route,
        d.name,
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
    let (z0, z1, z_step) = height_range(d, s0, s1);
    let (h, v) = (1000.0 / o.horizontal_scale, 1000.0 / o.vertical_scale);
    let plan_k = 1000.0 / o.plan_offset_scale.unwrap_or(o.horizontal_scale);
    let margin = 30.0;
    let title_h = 8.0 + 5.0 * o.title.len() as f64;
    let plan_h = if o.plan {
        2.0 * o.plan_half_width * plan_k
    } else {
        0.0
    };
    let f = Frame {
        s0,
        s1,
        content_end: s1,
        left: margin,
        plan: o.plan,
        plan_top: margin + title_h,
        plan_h,
        prof_top: margin + title_h + if o.plan { plan_h + 15.0 } else { 0.0 },
        prof_h: (z1 - z0) * v,
        z0,
        z1,
        h,
        plan_k,
        v,
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
    let warnings = draw(items, d, o, &f, z_step);
    (drawing, f, warnings)
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

const SHEET_MARGIN: f64 = 10.0;
const TITLE_BLOCK: f64 = 32.0;
const AXIS_ROOM: f64 = 14.0;

/// A place a sheet may start or end: a structure, a free alignment
/// point, or the end of a station range.
struct Stop {
    station: f64,
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
    /// Height range and grid step of the profile.
    heights: (f64, f64, f64),
}

/// The paper geometry of sheets of `page` with options `o`.
struct Paper {
    /// mm per m: along the route, across in the plan, vertical.
    h: f64,
    plan_k: f64,
    v: f64,
    left: f64,
    /// Width of the drawing area, mm.
    usable: f64,
    plan_h: f64,
    draw_top: f64,
    prof_top: f64,
    /// Height of the profile panel at most, mm.
    available: f64,
}

impl Paper {
    /// The frame of a sheet from `s0` to `s1` holding `reach` m, its
    /// profile's `heights` (low, high, grid step).
    fn frame(&self, o: &Options, s0: f64, s1: f64, reach: f64, heights: (f64, f64, f64)) -> Frame {
        let (z0, z1, _) = heights;
        let needed = (z1 - z0) * self.v;
        Frame {
            s0,
            s1: s0 + reach.max(s1 - s0),
            content_end: if o.stations.is_some() {
                s1
            } else {
                f64::INFINITY
            },
            left: self.left,
            plan: o.plan,
            plan_top: self.draw_top,
            plan_h: self.plan_h,
            prof_top: self.prof_top,
            // The panel takes the height its range needs (the profile is
            // clipped to the sheet when it needs more: see warnings).
            prof_h: self.available.min(needed).max(10.0),
            z0,
            z1,
            h: self.h,
            plan_k: self.plan_k,
            v: self.v,
        }
    }

    fn new(o: &Options, page: &Page) -> Self {
        let (h, v) = (1000.0 / o.horizontal_scale, 1000.0 / o.vertical_scale);
        let plan_k = 1000.0 / o.plan_offset_scale.unwrap_or(o.horizontal_scale);
        let left = SHEET_MARGIN + AXIS_ROOM;
        let plan_h = if o.plan {
            2.0 * o.plan_half_width * plan_k
        } else {
            0.0
        };
        let draw_top = SHEET_MARGIN + 6.0;
        let draw_bottom = page.height - SHEET_MARGIN - TITLE_BLOCK - 8.0;
        let prof_top = if o.plan {
            draw_top + plan_h + 8.0
        } else {
            draw_top
        };
        Self {
            h,
            plan_k,
            v,
            left,
            usable: page.width - left - SHEET_MARGIN,
            plan_h,
            draw_top,
            prof_top,
            available: draw_bottom - prof_top,
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
/// ends at the range.
pub fn sheet_layout(d: &RouteDrawing, o: &Options, page: &Page) -> Vec<SheetLayout> {
    layout(&d.shown(o.plan_half_width), o, page)
}

/// [`sheet_layout`] of `d`, what is drawn of the route.
fn layout(d: &RouteDrawing, o: &Options, page: &Page) -> Vec<SheetLayout> {
    let paper = Paper::new(o, page);
    let (h, usable) = (paper.h, paper.usable);
    let reach = usable / h; // metres per sheet
    let clearance =
        crate::check_break_clearance(o.break_clearance).unwrap_or(crate::BREAK_CLEARANCE_MM);
    let stations = stops(d, o, clearance / h);
    // Break points: greedy, whole spans per sheet, at free stops only.
    let last = stations.len().saturating_sub(1);
    let can_break = |k: usize| k == last || stations[k].blocked.is_none();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < last {
        let fits = |k: &usize| stations[*k].station - stations[i].station <= reach + 1e-9;
        let j = (i + 1..=last)
            .filter(|&k| can_break(k))
            .take_while(fits)
            .last()
            .or_else(|| (i + 1..=last).find(|&k| can_break(k)))
            .unwrap_or(last);
        ranges.push((i, j));
        i = j;
    }
    if ranges.is_empty() && !stations.is_empty() {
        ranges.push((0, 0));
    }
    let what = if o.plan { "span" } else { "stretch" };
    ranges
        .iter()
        .enumerate()
        .map(|(k, &(a, b))| {
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
            let heights = height_range(d, s0, s1);
            let needed = (heights.1 - heights.0) * paper.v;
            if needed > paper.available + 1e-6 {
                warnings.push(format!(
                    "profile {}–{} needs {:.0} mm of height at 1:{}, the sheet has {:.0} mm",
                    stations[a].label, stations[b].label, needed, o.vertical_scale, paper.available
                ));
            }
            // Elevations overlapping each other or the profile.
            warnings.extend(elevation_warnings(
                d,
                o,
                &paper.frame(o, s0, s1, reach, heights),
            ));
            SheetLayout {
                number: k + 1,
                from: s0,
                to: s1,
                reach,
                structures: (stations[a].label.clone(), stations[b].label.clone()),
                warnings,
                heights,
            }
        })
        .collect()
}

/// The route split over sheets of `page` as [`sheet_layout`] says, each
/// drawn with its own height range and a title block (title lines, route,
/// sheet n/N, station range, scales, logos).
pub fn sheets(d: &RouteDrawing, o: &Options, page: &Page) -> Vec<Sheet> {
    let paper = Paper::new(o, page);
    let d = &d.shown(o.plan_half_width);
    let layout = layout(d, o, page);
    let n = layout.len();
    layout
        .into_iter()
        .map(|sl| {
            let (s0, s1) = (sl.from, sl.to);
            let z_step = sl.heights.2;
            let k = sl.number - 1;
            let f = paper.frame(o, s0, s1, sl.reach, sl.heights);
            let mut drawing = Drawing::sheet(page.width, page.height);
            let items = &mut drawing.items;
            items.push(rect(
                Layer::Frame,
                SHEET_MARGIN / 2.0,
                SHEET_MARGIN / 2.0,
                page.width - SHEET_MARGIN,
                page.height - SHEET_MARGIN,
                pen("#000", 0.5),
            ));
            // Its warnings, the elevations' too, are the layout's.
            let _ = draw(items, d, o, &f, z_step);
            // Match lines at the sheet's ends.
            let top = if o.plan { f.plan_top } else { f.prof_top };
            for x in [f.x(s0), f.x(s1)] {
                items.push(path(
                    Layer::Match,
                    vec![[x, top], [x, f.prof_top + f.prof_h]],
                    pen("#06c", 0.3).dashed("2 1"),
                    None,
                ));
            }
            // Title block across the bottom: logos, titles, sheet data.
            let tb_top = page.height - SHEET_MARGIN / 2.0 - TITLE_BLOCK;
            let tb_left = SHEET_MARGIN / 2.0;
            let tb_w = page.width - SHEET_MARGIN;
            items.push(rect(
                Layer::Frame,
                tb_left,
                tb_top,
                tb_w,
                TITLE_BLOCK,
                pen("#000", 0.4),
            ));
            let mut cursor = tb_left + 3.0;
            for logo in &o.logos {
                items.push(Item::Image(Image {
                    layer: Layer::Logo,
                    at: [cursor, tb_top + 3.0],
                    width: logo.width,
                    height: TITLE_BLOCK - 6.0,
                    href: logo.href.clone(),
                }));
                cursor += logo.width + 4.0;
            }
            let text_x = cursor + 2.0;
            items.push(bold(text(
                Layer::Title,
                [text_x, tb_top + 7.0],
                4.5,
                Anchor::Start,
                heading(d, o),
            )));
            for (t, line) in o.title.iter().enumerate().take(3) {
                items.push(text(
                    Layer::Title,
                    [text_x, tb_top + 13.0 + 5.0 * t as f64],
                    3.0,
                    Anchor::Start,
                    line.clone(),
                ));
            }
            let right = tb_left + tb_w - 3.0;
            for (t, line) in [
                format!("Sheet {} / {n}", k + 1),
                format!(
                    "{} – {}   ({} – {})",
                    format_station(s0),
                    format_station(s1),
                    sl.structures.0,
                    sl.structures.1
                ),
                scales_text(o),
                format!(
                    "Format {} ({:.0} × {:.0} mm)",
                    page.name, page.width, page.height
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let item = text(
                    Layer::Title,
                    [right, tb_top + 7.0 + 6.0 * t as f64],
                    if t == 0 { 4.5 } else { 3.0 },
                    Anchor::End,
                    line,
                );
                items.push(if t == 0 { bold(item) } else { item });
            }
            Sheet {
                number: sl.number,
                from: s0,
                to: s1,
                structures: sl.structures,
                drawing,
                warnings: sl.warnings,
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
    sheet.to_model(k, [f.s0 - f.left * k, f.prof_top * k + exaggeration * f.z1])
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
    for w in &d.wires {
        let pts: Vec<[f64; 3]> = w
            .points
            .iter()
            .zip(&w.xy)
            .map(|(p, q)| [p[0], q[0], q[1]])
            .collect();
        for run in station_runs(&pts, a, b) {
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
