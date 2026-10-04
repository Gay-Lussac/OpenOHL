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
}

impl Layer {
    /// All layers.
    pub const ALL: [Layer; 11] = [
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
/// or a mast up to the highest attachment and arms out to the insulators;
/// then the insulator strings (V-strings: both legs).
pub fn structure_lines(st: &ols_line::Structure) -> Vec<(Part, Vec<[f64; 3]>)> {
    let mut out = Vec::new();
    if st.members.is_empty() {
        let top = st
            .attachments
            .iter()
            .map(|a| a.insulator_point[2])
            .fold(st.position[2], f64::max);
        let mast = |z: f64| [st.position[0], st.position[1], z];
        out.push((Part::Mast, vec![st.position, mast(top)]));
        for a in &st.attachments {
            out.push((
                Part::Member,
                vec![mast(a.insulator_point[2]), a.insulator_point],
            ));
        }
    } else {
        for m in &st.members {
            out.push((Part::Member, m.clone()));
        }
    }
    for a in &st.attachments {
        match &a.legs {
            Some(legs) => {
                for l in legs {
                    out.push((Part::Insulator, vec![*l, a.wire_point]));
                }
            }
            None => out.push((Part::Insulator, vec![a.insulator_point, a.wire_point])),
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
    let mut clamps: std::collections::HashMap<(&str, u32, u32), Vec<[f64; 3]>> =
        std::collections::HashMap::new();
    for sec in &line.sections {
        for k in model.section_cases(&sec.id, case) {
            let Some(shape) = cases
                .get(k)
                .and_then(|c| c.shapes.iter().find(|x| x.section == sec.id))
            else {
                continue;
            };
            for (p, c) in sec.points.iter().zip(&shape.clamps) {
                clamps
                    .entry((p.structure.as_str(), p.set, p.phase))
                    .or_default()
                    .push(*c);
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
                .attachments
                .iter()
                .map(|a| a.insulator_point[2])
                .chain(st.members.iter().flatten().map(|p| p[2]))
                .fold(at[2], f64::max);
            out.push(Prim::new(at, vec![at, [at[0], at[1], top]], steel, 2.5).at(1, 2));
        }
        for a in &st.attachments {
            let solved = clamps.get(&(st.id.as_str(), a.set, a.phase));
            let ends: Vec<[f64; 3]> = match solved {
                Some(c) if !c.is_empty() => c.clone(),
                _ => vec![a.wire_point],
            };
            for end in ends {
                match &a.legs {
                    Some(legs) => {
                        for l in legs {
                            out.push(Prim::new(at, vec![*l, end], blue, 2.0).at(0, 1));
                        }
                    }
                    None => {
                        out.push(Prim::new(at, vec![a.insulator_point, end], blue, 2.0).at(0, 1))
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
    let colors: std::collections::HashMap<&str, [u8; 3]> = line
        .sections
        .iter()
        .map(|sec| (sec.id.as_str(), rgb(&line.section_color(sec))))
        .collect();
    let wire = |out: &mut Vec<Prim>,
                points: &[[f64; 3]],
                lines: Vec<Vec<[f64; 3]>>,
                color: [u8; 4],
                w: [f32; 2]| {
        if points.len() < 2 {
            return;
        }
        let at = middle(points);
        let bundle = lines.len() > 1;
        for l in lines {
            out.push(Prim::new(at, l, color, if bundle { w[0] } else { w[1] }).at(0, 0));
        }
        let mut centre = Prim::new(at, points.to_vec(), color, w[1]).at(1, 2);
        centre.thinned = true;
        out.push(centre);
    };
    for (k, solved) in cases.iter().enumerate() {
        let this_case = shown == model.shown(case).contains(&k);
        for shape in &solved.shapes {
            if shown != model.section_cases(&shape.section, case).contains(&k) {
                continue;
            }
            let col = tint.unwrap_or_else(|| {
                colors
                    .get(shape.section.as_str())
                    .copied()
                    .unwrap_or([176, 184, 192])
            });
            for (n, span) in shape.spans.iter().enumerate() {
                // Bundles: each subconductor (one section, solved and
                // selected as one), ending on the yokes.
                let lines = match solved.bundles.spans.get(&(shape.section.clone(), n)) {
                    Some(l) => l.clone(),
                    None => vec![span.points.clone()],
                };
                wire(out, &span.points, lines, rgba(col, alpha), [1.5, 2.0]);
            }
        }
        for j in solved.jumpers.iter().filter(|_| this_case) {
            let (col, lines) = jumper_lines(line, j, &solved.bundles);
            let col = tint.unwrap_or(col);
            wire(out, &j.points, lines, rgba(col, alpha), [1.2, 1.5]);
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
        if p.thinned && level > 0 {
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
        // Drawn from the clearance report ([`marks`]).
        Layer::Clearances | Layer::Draft => {}
        Layer::Selection => {
            let hi = [255, 255, 255, 255];
            match selection {
                Selection::Structure(id) => {
                    if let Some(st) = line.structure(id) {
                        for a in &st.attachments {
                            s.point(a.wire_point, hi, 9.0);
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
                Selection::Attachment {
                    structure,
                    set,
                    phase,
                } => {
                    if let Some(a) = line.attachment(&ols_line::SectionPoint {
                        structure: structure.clone(),
                        set: *set,
                        phase: *phase,
                    }) {
                        s.polyline(&[a.insulator_point, a.wire_point], hi, 3.0);
                        s.point(a.insulator_point, hi, 9.0);
                        s.point(a.wire_point, hi, 11.0);
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
