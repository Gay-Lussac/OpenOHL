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
    /// Ground surface.
    Ground,
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
    pub const ALL: [Layer; 9] = [
        Layer::Ground,
        Layer::Points,
        Layer::Alignment,
        Layer::Structures,
        Layer::Wires,
        Layer::OtherCases,
        Layer::Clearances,
        Layer::Draft,
        Layer::Selection,
    ];

    /// Name in the layer panel.
    pub fn name(self) -> &'static str {
        match self {
            Layer::Ground => "Ground",
            Layer::Points => "LiDAR points",
            Layer::Alignment => "Alignments",
            Layer::Structures => "Structures",
            Layer::Wires => "Wires (selected case)",
            Layer::OtherCases => "Wires (other cases)",
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
    let h = hex.trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("b0"), 16).unwrap_or(176);
    if h.len() == 6 {
        [byte(0), byte(2), byte(4)]
    } else {
        [176, 184, 192]
    }
}

/// Display colour of a LiDAR class, by its roles (`None`: not shown).
pub fn class_color(scheme: &ols_lidar::ClassScheme, code: u8) -> Option<[u8; 3]> {
    let roles: Vec<&str> = scheme
        .class(code)
        .map(|c| c.roles.iter().map(String::as_str).collect())
        .unwrap_or_default();
    let has = |r: &str| roles.iter().any(|x| x.contains(r));
    Some(if has("noise") {
        return None;
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

/// Builds one layer of `model` for weather case `case`.
pub fn layer(model: &Model, layer: Layer, case: usize, selection: &Selection) -> Scene {
    let line = &model.line;
    let mut s = Scene::new(model.origin);
    match layer {
        Layer::Ground => {
            if let Some(g) = &model.ground {
                let (pts, tris) = g.mesh();
                // Light from the north-west, above.
                let light = {
                    let l = [-0.5f64, 0.5, 0.75];
                    let n = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
                    l.map(|v| v / n)
                };
                for t in tris {
                    let [a, b, c] = t.map(|i| pts[i]);
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
                    let lit =
                        0.4 + 0.6 * (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]).max(0.0);
                    let col = [120.0, 108.0, 88.0].map(|x: f64| (x * lit * 1.3).min(255.0) as u8);
                    s.triangle([a, b, c], [rgba(col, 255); 3]);
                }
            }
        }
        Layer::Points => {
            for (q, c) in &model.points {
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
        Layer::Structures => {
            for st in &line.structures {
                for (part, pts) in structure_lines(st) {
                    let (col, width) = match part {
                        Part::Mast => ([215, 215, 220, 255], 2.5),
                        Part::Member => ([215, 215, 220, 255], 1.5),
                        Part::Insulator => ([120, 180, 255, 255], 2.0),
                    };
                    s.polyline(&pts, col, width);
                }
            }
        }
        Layer::Wires | Layer::OtherCases => {
            for (k, solved) in model.cases.iter().enumerate() {
                // Jumpers and yokes in the case shown.
                let this_case = (layer == Layer::Wires) == (k == case);
                let alpha = if layer == Layer::Wires { 255 } else { 90 };
                for shape in &solved.shapes {
                    // Each section in its own display case.
                    let shown = model.section_cases(&shape.section, case).contains(&k);
                    if (layer == Layer::Wires) != shown {
                        continue;
                    }
                    let sec = line.sections.iter().find(|x| x.id == shape.section);
                    let col = sec.map_or([176, 184, 192], |sec| rgb(&line.section_color(sec)));
                    for (k, span) in shape.spans.iter().enumerate() {
                        // Bundles: each subconductor (one section, solved
                        // and selected as one), ending on the yokes.
                        let lines = match solved.bundles.spans.get(&(shape.section.clone(), k)) {
                            Some(l) => l.clone(),
                            None => vec![span.points.clone()],
                        };
                        let width = if lines.len() > 1 { 1.5 } else { 2.0 };
                        for l in &lines {
                            s.polyline(l, rgba(col, alpha), width);
                        }
                    }
                }
                for j in solved.jumpers.iter().filter(|_| this_case) {
                    let (col, lines) = jumper_lines(line, j, &solved.bundles);
                    let width = if lines.len() > 1 { 1.2 } else { 1.5 };
                    for l in &lines {
                        s.polyline(l, rgba(col, alpha), width);
                    }
                }
                // Yokes at the clamps: a bar, a triangle, a polygon.
                for y in solved.bundles.yokes.iter().filter(|_| this_case) {
                    let mut ring = y.clone();
                    if y.len() > 2 {
                        ring.push(y[0]);
                    }
                    s.polyline(&ring, [205, 205, 210, alpha], 2.0);
                }
            }
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
