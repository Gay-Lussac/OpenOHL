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
    /// The selection, highlighted.
    Selection,
}

impl Layer {
    /// All layers.
    pub const ALL: [Layer; 7] = [
        Layer::Ground,
        Layer::Points,
        Layer::Alignment,
        Layer::Structures,
        Layer::Wires,
        Layer::OtherCases,
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
            Layer::Selection => "Selection",
        }
    }
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
            }
        }
        Layer::Structures => {
            let grey = [215, 215, 220, 255];
            for st in &line.structures {
                if st.members.is_empty() {
                    // No stick drawing: a mast up to the highest attachment
                    // and arms out to the insulators.
                    let top = st
                        .attachments
                        .iter()
                        .map(|a| a.insulator_point[2])
                        .fold(st.position[2], f64::max);
                    let mast = |z: f64| [st.position[0], st.position[1], z];
                    s.polyline(&[st.position, mast(top)], grey, 2.5);
                    for a in &st.attachments {
                        s.polyline(&[mast(a.insulator_point[2]), a.insulator_point], grey, 1.5);
                    }
                } else {
                    for m in &st.members {
                        s.polyline(m, grey, 1.5);
                    }
                }
                // Insulator strings (V-strings: both legs).
                for a in &st.attachments {
                    let ins = [120, 180, 255, 255];
                    match &a.legs {
                        Some(legs) => {
                            for l in legs {
                                s.polyline(&[*l, a.wire_point], ins, 2.0);
                            }
                        }
                        None => s.polyline(&[a.insulator_point, a.wire_point], ins, 2.0),
                    }
                }
            }
        }
        Layer::Wires | Layer::OtherCases => {
            for (k, solved) in model.cases.iter().enumerate() {
                if (layer == Layer::Wires) != (k == case) {
                    continue;
                }
                let alpha = if layer == Layer::Wires { 255 } else { 90 };
                for shape in &solved.shapes {
                    let col = line
                        .sections
                        .iter()
                        .find(|x| x.id == shape.section)
                        .map_or([176, 184, 192], |sec| rgb(&line.section_color(sec)));
                    for span in &shape.spans {
                        s.polyline(&span.points, rgba(col, alpha), 2.0);
                    }
                }
                for j in &solved.jumpers {
                    s.polyline(&j.points, rgba([200, 200, 200], alpha), 1.5);
                }
            }
        }
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
                    if let Some(shape) = model
                        .cases
                        .get(case)
                        .and_then(|c| c.shapes.iter().find(|x| &x.section == section))
                        && let Some(sp) = shape.spans.get(*span)
                    {
                        s.polyline(&sp.points, hi, 5.0);
                    }
                }
                Selection::None => {}
            }
        }
    }
    s
}
