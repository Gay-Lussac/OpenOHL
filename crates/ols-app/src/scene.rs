// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The 3D layers of a model, as renderer scenes.

use ols_render::Scene;

use crate::model::Model;
use crate::selection::Selection;

/// The layers of the 3D view, in drawing order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    /// Ground surface (the active terrain surface).
    Ground,
    /// Other terrain surfaces shown, each in its colour.
    Surfaces,
    /// LiDAR points (other than ground).
    Points,
    /// Route alignments.
    Alignment,
    /// Structures: stick drawings, insulator strings.
    Structures,
    /// Wires of the selected weather case.
    Wires,
    /// Wires of the other weather cases (faded).
    OtherCases,
    /// Saved project lines shown for comparison (Lines menu), each in its
    /// colour.
    OtherLines,
    /// Clearance violations: wire to target, red
    /// ([`View3d::marks`](crate::view3d::View3d::marks)).
    Clearances,
    /// A wire being strung (String wire tool).
    Draft,
    /// The selection, highlighted.
    Selection,
    /// Orthophotos (Terrain › Backgrounds), under the plan view.
    Orthophotos,
    /// DXF drawings (Terrain › Backgrounds), in the plan and 3D views.
    Drawings,
}

impl Layer {
    /// All layers.
    pub const ALL: [Layer; 13] = [
        Layer::Ground,
        Layer::Surfaces,
        Layer::Points,
        Layer::Alignment,
        Layer::Structures,
        Layer::Wires,
        Layer::OtherCases,
        Layer::OtherLines,
        Layer::Clearances,
        Layer::Draft,
        Layer::Selection,
        Layer::Orthophotos,
        Layer::Drawings,
    ];

    /// Name in the layer panel.
    pub fn name(self) -> &'static str {
        match self {
            Layer::Ground => "Ground",
            Layer::Surfaces => "Other surfaces",
            Layer::Points => "LiDAR points",
            Layer::Alignment => "Alignments",
            Layer::Structures => "Structures",
            Layer::Wires => "Wires (selected case)",
            Layer::OtherCases => "Wires (other cases)",
            Layer::OtherLines => "Other project lines",
            Layer::Clearances => "Clearance violations",
            Layer::Draft => "Wire being strung",
            Layer::Selection => "Selection",
            Layer::Orthophotos => "Orthophotos (plan)",
            Layer::Drawings => "Drawings (DXF)",
        }
    }
}

/// What a line of a structure drawing is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// The mast drawn when there is no stick drawing.
    Mast,
    /// A member: of the stick drawing, or an arm to an insulator.
    Member,
    /// An insulator string (or a V-string leg).
    Insulator,
}

/// The lines a structure is drawn with (and picked by): its stick drawing,
/// or a mast up to the highest point and arms out to its points; then its
/// strings as given (from their tops to their wire points).
pub fn structure_lines(st: &ols_line::Structure) -> Vec<(Part, Vec<[f64; 3]>)> {
    let mut out = Vec::new();
    if st.members.is_empty() {
        let top = st
            .points
            .iter()
            .map(|p| p.position[2])
            .fold(st.position[2], f64::max);
        let mast = |z: f64| [st.position[0], st.position[1], z];
        out.push((Part::Mast, vec![st.position, mast(top)]));
        for p in &st.points {
            out.push((Part::Member, vec![mast(p.position[2]), p.position]));
        }
    } else {
        for m in &st.members {
            out.push((Part::Member, m.clone()));
        }
    }
    for x in &st.strings {
        if let (Some(top), Some(wire)) = (st.node(&x.top), st.wire(&x.wire)) {
            out.push((Part::Insulator, vec![top.position(), wire.position]));
        }
    }
    out
}

/// A solved jumper's colour (its own, else its section's) and its
/// subconductor polylines, ending on the yokes at its ends
/// ([`ols_line::bundles`]).
pub fn jumper_lines(
    line: &ols_line::Line,
    j: &ols_line::jumper::JumperShape,
    bundles: &ols_line::bundles::BundleLines,
) -> ([u8; 3], Vec<Vec<[f64; 3]>>) {
    let def = line.jumpers.iter().find(|d| d.id == j.jumper);
    let col = def
        .and_then(|d| d.color.clone())
        .or_else(|| {
            def.and_then(|d| line.jumper_section(d))
                .map(|s| line.section_color(s))
        })
        .map_or([200, 200, 200], |c| rgb(&c));
    let lines = match (bundles.jumpers.get(&j.jumper), def) {
        (Some(l), _) => l.clone(),
        (None, Some(d)) => line.jumper_bundle(d).polylines(&j.points),
        (None, None) => vec![j.points.clone()],
    };
    (col, lines)
}

/// `#rrggbb` → sRGB bytes (grey if not a colour).
pub fn rgb(hex: &str) -> [u8; 3] {
    ols_core::text::parse_rgb(hex).unwrap_or([176, 184, 192])
}

/// Display colour of a LiDAR class, by its roles (noise magenta: shown
/// unless hidden in Terrain › LiDAR classes shown).
pub fn class_color(scheme: &ols_lidar::ClassScheme, code: u8) -> Option<[u8; 3]> {
    // The scheme's own colour, else one from the roles.
    if let Some(c) = scheme
        .class(code)
        .and_then(ols_lidar::classes::ClassDef::rgb)
    {
        return Some(c);
    }
    role_color(scheme, code)
}

/// The colour of a class from its roles.
pub fn role_color(scheme: &ols_lidar::ClassScheme, code: u8) -> Option<[u8; 3]> {
    let roles: Vec<&str> = scheme
        .class(code)
        .map(|c| c.roles.iter().map(String::as_str).collect())
        .unwrap_or_default();
    let has = |r: &str| roles.iter().any(|x| x.contains(r));
    Some(if has("noise") {
        [200, 60, 200]
    } else if has("vegetation") {
        [70, 150, 70]
    } else if has("building") || has("substation") || has("tank") {
        [205, 95, 80]
    } else if has("water") {
        [70, 130, 210]
    } else if has("conductor") || has("wire") || has("jumper") {
        [235, 205, 70]
    } else if has("railway") || has("ropeway") {
        [180, 120, 200]
    } else if has("road") || has("bridge") {
        [150, 150, 150]
    } else if has("structure") || has("pole") || has("insulator") {
        [230, 230, 230]
    } else if has("ground") {
        [150, 125, 95]
    } else {
        [120, 160, 170]
    })
}

fn rgba(c: [u8; 3], a: u8) -> [u8; 4] {
    [c[0], c[1], c[2], a]
}

/// A polyline to draw, with the levels of detail it is drawn at: the
/// layers' common output, drawn whole ([`layer`]) or by chunk
/// ([`crate::chunks`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Prim {
    /// Where it belongs in plan (its chunk): a structure's position, a
    /// span's middle.
    pub anchor: [f64; 2],
    /// Points.
    pub points: Vec<[f64; 3]>,
    /// Colour.
    pub color: [u8; 4],
    /// Width, pixels.
    pub width: f32,
    /// Lowest and highest level of detail it is drawn at.
    pub levels: (usize, usize),
    /// Thinned to the layer's points per span at levels 1 and 2 (wires).
    pub thinned: bool,
    /// The points are triangles (three by three), drawn filled and lit,
    /// not a polyline: solid bodies (marker balls).
    pub filled: bool,
}

impl Prim {
    fn new(anchor: [f64; 3], points: Vec<[f64; 3]>, color: [u8; 4], width: f32) -> Self {
        Self {
            anchor: [anchor[0], anchor[1]],
            points,
            color,
            width,
            levels: (0, 0),
            thinned: false,
            filled: false,
        }
    }

    fn at(mut self, lo: usize, hi: usize) -> Self {
        self.levels = (lo, hi);
        self
    }
}

/// Structures of `line` (solved in `cases`), their strings drawn to the
/// solved clamps in every weather case their section is shown in (strings
/// swing with the wind, move with temperature); strings without a section
/// hang as given. `tint`: one colour for everything (a project line).
///
/// Levels of detail: 0 everything; 1 mast and arms (a lattice: a mast line
/// up to its top attachment) and strings; 2 the mast line.
pub fn structures(
    out: &mut Vec<Prim>,
    model: &Model,
    line: &ols_line::Line,
    cases: &[crate::model::Solved],
    case: usize,
    tint: Option<[u8; 3]>,
) {
    // The strings as solved (top, bottom, beyond their compression), by
    // structure and string.
    type Drawn = Vec<([f64; 3], [f64; 3], bool)>;
    let mut solved: std::collections::HashMap<(&str, String), Drawn> =
        std::collections::HashMap::new();
    for sec in &line.sections {
        // Both wind sides shown: the strings to each side's clamps.
        let shapes = model
            .section_cases(&sec.id, case)
            .into_iter()
            .filter_map(|k| cases.get(k))
            .flat_map(|c| c.drawn())
            .filter_map(|c| c.shapes.iter().find(|x| x.section == sec.id));
        for shape in shapes {
            for (p, strings) in sec.points.iter().zip(&shape.strings) {
                for x in strings {
                    solved
                        .entry((p.structure.as_str(), x.label.clone()))
                        .or_default()
                        .push((x.top, x.bottom, x.over));
                }
            }
        }
    }
    // Strings holding jumpers' support clamps: to the solved clamps, in
    // the cases the jumpers are drawn in.
    for c in model
        .shown(case)
        .into_iter()
        .filter_map(|k| cases.get(k))
        .flat_map(|c| c.drawn())
    {
        for s in c.jumpers.iter().flat_map(|j| &j.supports) {
            let Some(st) = line.structure(&s.structure) else {
                continue;
            };
            for x in &s.strings {
                solved
                    .entry((st.id.as_str(), x.label.clone()))
                    .or_default()
                    .push((x.top, s.clamp, false));
            }
        }
    }
    let steel = tint.map_or([215, 215, 220, 255], |c| rgba(c, 230));
    let blue = tint.map_or([120, 180, 255, 255], |c| rgba(c, 230));
    for st in &line.structures {
        let at = st.position;
        let lattice = !st.members.is_empty();
        for (part, pts) in structure_lines(st) {
            let prim = match part {
                // Stick drawings: the mast at every level, arms down to 1.
                Part::Mast => Prim::new(at, pts, steel, 2.5).at(0, 2),
                Part::Member if lattice => Prim::new(at, pts, steel, 1.5).at(0, 0),
                Part::Member => Prim::new(at, pts, steel, 1.5).at(0, 1),
                Part::Insulator => continue,
            };
            out.push(prim);
        }
        if lattice {
            // Levels 1 and 2 of a lattice: a mast line to its top.
            let top = st
                .points
                .iter()
                .map(|p| p.position[2])
                .chain(st.members.iter().flatten().map(|p| p[2]))
                .fold(at[2], f64::max);
            out.push(Prim::new(at, vec![at, [at[0], at[1], top]], steel, 2.5).at(1, 2));
        }
        // The strings, as solved in the cases shown (else as given);
        // flagged ones (beyond the compression they may take) in red.
        let red = tint.map_or([235, 60, 50, 255], |c| rgba(c, 230));
        for x in &st.strings {
            match solved.get(&(st.id.as_str(), x.label.clone())) {
                Some(drawn) => {
                    for (top, bottom, over) in drawn {
                        let col = if *over { red } else { blue };
                        out.push(Prim::new(at, vec![*top, *bottom], col, 2.0).at(0, 1));
                    }
                }
                None => {
                    if let (Some(top), Some(wire)) = (st.node(&x.top), st.wire(&x.wire)) {
                        out.push(
                            Prim::new(at, vec![top.position(), wire.position], blue, 2.0).at(0, 1),
                        );
                    }
                }
            }
        }
    }
}

/// The middle of a polyline's plan extent (its chunk).
fn middle(points: &[[f64; 3]]) -> [f64; 3] {
    let (a, b) = (points[0], points[points.len() - 1]);
    [
        0.5 * (a[0] + b[0]),
        0.5 * (a[1] + b[1]),
        0.5 * (a[2] + b[2]),
    ]
}

/// Wires of `line` solved in `cases`: each section in its own display
/// cases, bundles as subconductors ending on the yokes, jumpers in the
/// cases shown for the line. `shown`: the cases shown (else the others,
/// faint). `tint`: one colour for everything (a project line).
///
/// Levels of detail: 0 every subconductor and the yokes; 1 and 2 one line
/// per bundle (the solved wire), thinned to the layer's points per span.
pub fn wires(
    out: &mut Vec<Prim>,
    model: &Model,
    line: &ols_line::Line,
    cases: &[crate::model::Solved],
    case: usize,
    shown: bool,
    tint: Option<[u8; 3]>,
) {
    let alpha = match (shown, tint) {
        (false, _) => 90,
        (true, Some(_)) => 230,
        (true, None) => 255,
    };
    // Colours and line codes (Sections › Display options), the
    // directions as arrows in the case shown.
    let display = &model.section_display;
    let styles = crate::wire_style::Styles::new(line, display.coloring);
    let arrows = tint.is_none() && display.direction[crate::annotations::View::ThreeD as usize];
    let wire = |out: &mut Vec<Prim>,
                points: &[[f64; 3]],
                lines: Vec<Vec<[f64; 3]>>,
                color: [u8; 4],
                code: &[f64],
                w: [f32; 2]| {
        if points.len() < 2 {
            return;
        }
        let at = middle(points);
        let bundle = lines.len() > 1;
        for l in lines {
            for d in crate::wire_style::dashes(&l, code, DASH_UNIT) {
                out.push(Prim::new(at, d, color, if bundle { w[0] } else { w[1] }).at(0, 0));
            }
        }
        for d in crate::wire_style::dashes(points, code, DASH_UNIT) {
            let mut centre = Prim::new(at, d, color, w[1]).at(1, 2);
            centre.thinned = true;
            out.push(centre);
        }
    };
    let arrow =
        |out: &mut Vec<Prim>, points: &[[f64; 3]], forward: bool, size: f64, color: [u8; 4]| {
            if let Some(legs) =
                crate::wire_style::chevron(points, forward, size, crate::wire_style::across)
            {
                for l in legs {
                    out.push(Prim::new(l[1], l.to_vec(), color, 2.0).at(0, 1));
                }
            }
        };
    // Both wind sides shown: each case's mirror drawn with it.
    for (k, solved) in cases
        .iter()
        .enumerate()
        .flat_map(|(k, c)| c.drawn().map(move |d| (k, d)))
    {
        let this_case = shown == model.shown(case).contains(&k);
        for shape in &solved.shapes {
            if shown != model.section_cases(&shape.section, case).contains(&k) {
                continue;
            }
            let style = styles.section(&shape.section);
            let col = tint.unwrap_or(style.color);
            let forward = styles.directions.sections.get(&shape.section).copied();
            for (n, span) in shape.spans.iter().enumerate() {
                // Bundles: each subconductor (one section, solved and
                // selected as one), ending on the yokes.
                let lines = match solved.bundles.spans.get(&(shape.section.clone(), n)) {
                    Some(l) => l.clone(),
                    None => vec![span.points.clone()],
                };
                wire(
                    out,
                    &span.points,
                    lines,
                    rgba(col, alpha),
                    &style.pattern,
                    [1.5, 2.0],
                );
                if arrows
                    && this_case
                    && let Some(f) = forward
                {
                    arrow(out, &span.points, f, 6.0, rgba(col, alpha));
                }
            }
            // Marker balls and other bodies with a size, on the wire
            // (the bundle's centre) where they hang.
            let ball = tint.unwrap_or(BALL_COLOR);
            for l in line
                .sections
                .iter()
                .find(|sec| sec.id == shape.section)
                .into_iter()
                .flat_map(|sec| &sec.loads)
            {
                let d = l
                    .wind_area
                    .map_or(l.diameter, |a| (4.0 * a / std::f64::consts::PI).sqrt());
                let Some(at) = shape
                    .spans
                    .get(l.span)
                    .and_then(|sp| at_fraction(&sp.points, l.fraction))
                else {
                    continue;
                };
                if d > 0.0 {
                    let mut p = Prim::new(at, sphere(at, d / 2.0), rgba(ball, alpha), 0.0).at(0, 1);
                    p.filled = true;
                    out.push(p);
                }
            }
        }
        for j in solved.jumpers.iter().filter(|_| this_case) {
            let (_, lines) = jumper_lines(line, j, &solved.bundles);
            let style = styles.jumper(&j.jumper);
            let col = tint.unwrap_or(style.color);
            wire(
                out,
                &j.points,
                lines,
                rgba(col, alpha),
                &style.pattern,
                [1.2, 1.5],
            );
            if arrows && let Some(f) = styles.directions.jumpers.get(&j.jumper) {
                arrow(out, &j.points, *f, 1.5, rgba(col, alpha));
            }
            // Its support clamps: a short bar along it.
            let steel = tint.unwrap_or([205, 205, 210]);
            let def = line.jumpers.iter().find(|d| d.id == j.jumper);
            for (s, c) in j
                .supports
                .iter()
                .zip(def.into_iter().flat_map(|d| &d.supports))
            {
                let h = c.clamp_length / 2.0;
                let (m, d) = (s.clamp, s.direction);
                let ends = vec![
                    [0, 1, 2].map(|k| m[k] - d[k] * h),
                    [0, 1, 2].map(|k| m[k] + d[k] * h),
                ];
                out.push(Prim::new(m, ends, rgba(steel, alpha), 4.0).at(0, 1));
            }
        }
        // Yokes at the clamps (a bar, a triangle, a polygon), with their
        // section, in its cases.
        let steel = tint.unwrap_or([205, 205, 210]);
        for y in solved
            .bundles
            .yokes
            .iter()
            .zip(&solved.bundles.yoke_sections)
            .filter(|(_, sec)| shown == model.section_cases(sec, case).contains(&k))
            .map(|(y, _)| y)
        {
            let mut ring = y.clone();
            if y.len() > 2 {
                ring.push(y[0]);
            }
            out.push(Prim::new(y[0], ring, rgba(steel, alpha), 2.0).at(0, 0));
        }
    }
}

/// Length of a unit of the phases' line codes in 3D, m.
const DASH_UNIT: f64 = 1.5;

/// Colour of marker balls and other bodies on the wires (aviation
/// orange).
const BALL_COLOR: [u8; 3] = [240, 110, 30];

/// The point of a span's polyline at a horizontal fraction of the way
/// between its ends (measured along their plan chord).
pub fn at_fraction(points: &[[f64; 3]], fraction: f64) -> Option<[f64; 3]> {
    let (a, b) = (points.first()?, points.last()?);
    let chord = [b[0] - a[0], b[1] - a[1]];
    let len2 = chord[0] * chord[0] + chord[1] * chord[1];
    if len2 <= 0.0 {
        return None;
    }
    let t = |p: &[f64; 3]| ((p[0] - a[0]) * chord[0] + (p[1] - a[1]) * chord[1]) / len2;
    points.windows(2).find_map(|w| {
        let (t0, t1) = (t(&w[0]), t(&w[1]));
        ((t0 - fraction) * (t1 - fraction) <= 0.0 && t1 != t0).then(|| {
            let u = (fraction - t0) / (t1 - t0);
            [0, 1, 2].map(|k| w[0][k] + u * (w[1][k] - w[0][k]))
        })
    })
}

/// A sphere of radius `r` around `c` as triangles (12 around, 8 from
/// pole to pole), counter-clockwise seen from outside.
pub fn sphere(c: [f64; 3], r: f64) -> Vec<[f64; 3]> {
    use std::f64::consts::PI;
    let (around, down) = (12, 8);
    let at = |i: usize, j: usize| {
        let (phi, theta) = (
            PI * j as f64 / down as f64,
            2.0 * PI * i as f64 / around as f64,
        );
        [
            c[0] + r * phi.sin() * theta.cos(),
            c[1] + r * phi.sin() * theta.sin(),
            c[2] + r * phi.cos(),
        ]
    };
    let mut out = Vec::with_capacity(around * down * 6);
    for j in 0..down {
        for i in 0..around {
            let [a, b, c, d] = [at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1)];
            if j > 0 {
                out.extend([a, c, b]);
            }
            if j + 1 < down {
                out.extend([b, c, d]);
            }
        }
    }
    out
}

#[cfg(test)]
#[test]
fn spheres_face_out_and_are_lit_from_above() {
    let ball = sphere([0.0; 3], 1.0);
    for t in ball.as_chunks::<3>().0 {
        // Outward: the normal points away from the centre.
        let u = [0, 1, 2].map(|k| t[1][k] - t[0][k]);
        let v = [0, 1, 2].map(|k| t[2][k] - t[0][k]);
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let c = [0, 1, 2].map(|k| (t[0][k] + t[1][k] + t[2][k]) / 3.0);
        assert!((0..3).map(|k| n[k] * c[k]).sum::<f64>() > 0.0);
    }
    let top = ball
        .as_chunks::<3>()
        .0
        .iter()
        .find(|t| t.iter().all(|p| p[2] > 0.9))
        .unwrap();
    let bottom = ball
        .as_chunks::<3>()
        .0
        .iter()
        .find(|t| t.iter().all(|p| p[2] < -0.9))
        .unwrap();
    let c = [200, 100, 50, 255];
    assert!(lit(top, c)[0] > lit(bottom, c)[0]);
}

/// A filled triangle's shade: its colour lit from the north-west, above,
/// on its outer side (counter-clockwise seen from outside); its shadow
/// side darker.
fn lit(t: &[[f64; 3]], color: [u8; 4]) -> [u8; 4] {
    let u = [0, 1, 2].map(|k| t[1][k] - t[0][k]);
    let v = [0, 1, 2].map(|k| t[2][k] - t[0][k]);
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-12);
    let light = [-0.5, 0.5, 0.75];
    let l = (0.5f64 * 0.5 + 0.5 * 0.5 + 0.75 * 0.75).sqrt();
    let cos = (0..3).map(|k| n[k] * light[k]).sum::<f64>() / (len * l);
    let f = 0.35 + 0.65 * cos.max(0.0);
    let [r, g, b, a] = color;
    let shade = |x: u8| (f64::from(x) * f).min(255.0) as u8;
    [shade(r), shade(g), shade(b), a]
}

/// The primitives of a layer drawn as primitives (structures, wires,
/// other cases, project lines).
pub fn prims(model: &Model, layer: Layer, case: usize) -> Vec<Prim> {
    let mut out = Vec::new();
    let line = &model.line;
    match layer {
        Layer::Structures => structures(&mut out, model, line, &model.cases, case, None),
        Layer::Wires | Layer::OtherCases => {
            wires(
                &mut out,
                model,
                line,
                &model.cases,
                case,
                layer == Layer::Wires,
                None,
            );
        }
        Layer::OtherLines => {
            // Each project line shown drawn as the working line is (strings
            // to their solved clamps, bundles, yokes, jumpers), in the same
            // weather cases, in its colour.
            for other in &model.other_lines {
                let tint = Some(other.color);
                structures(&mut out, model, &other.line, &other.cases, case, tint);
                wires(&mut out, model, &other.line, &other.cases, case, true, tint);
            }
        }
        _ => {}
    }
    out
}

/// Draws primitives at `level`, wires thinned to `per_span` points.
pub fn draw_prims<'a>(
    s: &mut Scene,
    prims: impl IntoIterator<Item = &'a Prim>,
    level: usize,
    per_span: usize,
) {
    for p in prims {
        if level < p.levels.0 || level > p.levels.1 {
            continue;
        }
        if p.filled {
            for t in p.points.as_chunks::<3>().0 {
                s.triangle(*t, [lit(t, p.color); 3]);
            }
        } else if p.thinned && level > 0 {
            s.polyline(&thin(&p.points, per_span), p.color, p.width);
        } else {
            s.polyline(&p.points, p.color, p.width);
        }
    }
}

/// `n` (≥ 2) points of a polyline, evenly by index, both ends kept.
pub fn thin(points: &[[f64; 3]], n: usize) -> Vec<[f64; 3]> {
    let n = n.max(2);
    if points.len() <= n {
        return points.to_vec();
    }
    let last = points.len() - 1;
    (0..n)
        .map(|i| points[(i * last + (n - 1) / 2) / (n - 1)])
        .collect()
}

/// Shade of a triangle of another surface: its colour, lit like the
/// ground.
pub fn tinted_color(a: [f64; 3], b: [f64; 3], c: [f64; 3], tint: [u8; 3]) -> [u8; 4] {
    let g = ground_color(a, b, c);
    // The ground's light, as a factor of its base colour.
    let lit = f64::from(g[0]) / (120.0 * 1.3);
    rgba(tint.map(|x| (f64::from(x) * lit).min(255.0) as u8), 255)
}

/// Shade of a ground triangle (light from the north-west, above).
pub fn ground_color(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [u8; 4] {
    let light = {
        let l = [-0.5f64, 0.5, 0.75];
        let n = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
        l.map(|v| v / n)
    };
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let mut n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-12);
    n = n.map(|x| x / len);
    if n[2] < 0.0 {
        n = n.map(|x| -x);
    }
    let lit = 0.4 + 0.6 * (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]).max(0.0);
    let col = [120.0, 108.0, 88.0].map(|x: f64| (x * lit * 1.3).min(255.0) as u8);
    rgba(col, 255)
}

/// Builds one layer of `model` for weather case `case`.
pub fn layer(model: &Model, layer: Layer, case: usize, selection: &Selection) -> Scene {
    let line = &model.line;
    let mut s = Scene::new(model.origin);
    match layer {
        Layer::Ground => {
            if let Some(g) = &model.ground {
                let (pts, tris) = g.mesh();
                for t in tris {
                    let [a, b, c] = t.map(|i| pts[i]);
                    s.triangle([a, b, c], [ground_color(a, b, c); 3]);
                }
            }
        }
        Layer::Surfaces => {
            for o in &model.other_surfaces {
                let pts = o.terrain.points();
                for t in o.terrain.triangles() {
                    let [a, b, c] = t.map(|i| pts[i as usize]);
                    s.triangle([a, b, c], [tinted_color(a, b, c, o.color); 3]);
                }
            }
        }
        Layer::Points => {
            for (q, c) in &model.points {
                if model.hidden_classes.contains(c) {
                    continue;
                }
                if let Some(col) = class_color(&model.scheme, *c) {
                    let p = [0, 1, 2].map(|k| model.origin[k] + f64::from(q[k]) / 1000.0);
                    s.point(p, rgba(col, 255), 2.0);
                }
            }
        }
        Layer::Alignment => {
            for r in line.effective_routes() {
                // Draped on the ground, sampled every 5 m.
                let c = r.centreline(line);
                let mut pts: Vec<[f64; 3]> = Vec::new();
                for w in c.windows(2) {
                    let len = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
                    let n = ((len / 5.0).ceil() as usize).max(1);
                    for i in 0..=n {
                        let t = i as f64 / n as f64;
                        let (x, y) = (
                            w[0][0] + (w[1][0] - w[0][0]) * t,
                            w[0][1] + (w[1][1] - w[0][1]) * t,
                        );
                        let z = model
                            .ground
                            .as_ref()
                            .and_then(|g| g.height(x, y))
                            .unwrap_or(w[0][2] + (w[1][2] - w[0][2]) * t);
                        pts.push([x, y, z + 0.3]);
                    }
                }
                s.polyline(&pts, [90, 200, 220, 200], 1.5);
                // Its points, when it has its own (not just its structures).
                if r.alignment.len() >= 2 {
                    for p in &c {
                        s.point(
                            crate::selection::draped(model, *p),
                            [90, 200, 220, 255],
                            8.0,
                        );
                    }
                }
            }
        }
        // Full detail (the chunks draw these by level: crate::chunks).
        Layer::Structures | Layer::Wires | Layer::OtherCases | Layer::OtherLines => {
            draw_prims(&mut s, &prims(model, layer, case), 0, usize::MAX);
        }
        // Drawn from the clearance report ([`marks`]); orthophotos in plan
        // only.
        Layer::Clearances | Layer::Draft | Layer::Orthophotos => {}
        Layer::Drawings => crate::backgrounds::scene(model, &mut s),
        Layer::Selection => {
            let hi = [255, 255, 255, 255];
            match selection {
                Selection::Structure(id) => {
                    if let Some(st) = line.structure(id) {
                        for w in &st.wires {
                            s.point(w.position, hi, 9.0);
                        }
                        s.point(st.position, hi, 12.0);
                    }
                }
                Selection::Span { section, span } => {
                    let shown = model.section_case(section, case);
                    if let Some(shape) = model.shape(section, case)
                        && let Some(sp) = shape.spans.get(*span)
                    {
                        let lines = match model.cases[shown]
                            .bundles
                            .spans
                            .get(&(section.clone(), *span))
                        {
                            Some(l) => l.clone(),
                            None => vec![sp.points.clone()],
                        };
                        let width = if lines.len() > 1 { 3.0 } else { 5.0 };
                        for l in &lines {
                            s.polyline(l, hi, width);
                        }
                    }
                }
                Selection::Jumper(id) => {
                    if let Some(j) = model
                        .cases
                        .get(case)
                        .and_then(|c| c.jumpers.iter().find(|x| &x.jumper == id))
                    {
                        let (_, lines) = jumper_lines(line, j, &model.cases[case].bundles);
                        for l in &lines {
                            s.polyline(l, hi, 3.0);
                        }
                    }
                }
                Selection::Attachment { structure, set } => {
                    // The strings holding it, as given.
                    if let Some(st) = line.structure(structure)
                        && let Some(w) = st.wire_at_set(*set)
                    {
                        for &i in &st.component(&w.label).strings {
                            let x = &st.strings[i];
                            if let (Some(top), Some(bottom)) = (st.node(&x.top), st.wire(&x.wire)) {
                                s.polyline(&[top.position(), bottom.position], hi, 3.0);
                                s.point(top.position(), hi, 9.0);
                            }
                        }
                        s.point(w.position, hi, 11.0);
                    }
                }
                Selection::String { structure, label } => {
                    // Where it is drawn (both wind sides), else as given.
                    let mut drawn = crate::selection::string_pieces(model, case)
                        .remove(&(structure.clone(), label.clone()))
                        .unwrap_or_default();
                    if drawn.is_empty()
                        && let Some(st) = line.structure(structure)
                        && let Some(x) = st.string(label)
                        && let (Some(top), Some(w)) = (st.node(&x.top), st.wire(&x.wire))
                    {
                        drawn.push((top.position(), w.position));
                    }
                    for (a, b) in drawn {
                        s.polyline(&[a, b], hi, 4.0);
                        s.point(a, hi, 9.0);
                        s.point(b, hi, 9.0);
                    }
                }
                Selection::AlignmentPoint { route, index } => {
                    if let Some(p) = crate::selection::alignment_point(model, route, *index) {
                        s.point(p, hi, 12.0);
                        s.polyline(&[p, [p[0], p[1], p[2] + 30.0]], hi, 1.5);
                    }
                }
                Selection::LidarPoint(k) => {
                    if let Some((q, _)) = model.points.get(*k) {
                        s.point(crate::selection::lidar_xyz(model, q), hi, 10.0);
                    }
                }
                Selection::None => {}
            }
        }
    }
    s
}

/// Clearance violations as drawn: a red line from the wire to the target,
/// a dot on the wire.
pub fn marks(origin: [f64; 3], marks: &[([f64; 3], [f64; 3])]) -> Scene {
    let mut s = Scene::new(origin);
    for (at, target) in marks {
        s.polyline(&[*at, *target], [255, 60, 50, 255], 2.5);
        s.point(*at, [255, 60, 50, 255], 9.0);
        s.point(*target, [255, 160, 60, 255], 6.0);
    }
    s
}

/// A wire being strung: its points joined, yellow.
pub fn draft(origin: [f64; 3], points: &[[f64; 3]]) -> Scene {
    let mut s = Scene::new(origin);
    if points.len() > 1 {
        s.polyline(points, [255, 220, 60, 255], 3.0);
    }
    for p in points {
        s.point(*p, [255, 220, 60, 255], 11.0);
    }
    s
}
