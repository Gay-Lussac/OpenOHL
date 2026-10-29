// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The field drawn in the views (owner, 2026-10-29; the Magnetic field
//! and Electric field layers): the levels' isolines from above (at the grids' height above
//! the ground, 3D view and plan), their isosurfaces (the volumes,
//! translucent, 3D view) and the vertical cuts in the cross-section
//! panels (the cells coloured by band, the isolines over them). Each level
//! in its colour ([`Level`], the window's rules).

use std::sync::Arc;

use eframe::egui;
use ohl_electrical::contour::Mesh;
use ohl_electrical::field_views::CutValues;

/// A level drawn: its value and colour, shown or not (the window's rule
/// table).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Level {
    /// The value, µT (the magnetic field) or kV/m (the electric field).
    pub value: f64,
    /// Its colour.
    pub rgb: [u8; 3],
    /// Drawn.
    pub on: bool,
}

/// The magnetic field's default levels: decades, µT, display only (no
/// limit).
pub fn default_levels() -> Vec<Level> {
    vec![
        Level {
            value: 1.0,
            rgb: [70, 150, 255],
            on: true,
        },
        Level {
            value: 10.0,
            rgb: [255, 170, 40],
            on: true,
        },
        Level {
            value: 100.0,
            rgb: [230, 50, 50],
            on: false,
        },
    ]
}

/// A vertical cut as drawn: polygons coloured by band, isolines.
#[derive(Debug, Clone, Default)]
pub struct CutDrawing {
    /// Cells (convex polygons, 3D), each with its colour.
    pub bands: Vec<([u8; 4], Vec<[f64; 3]>)>,
    /// Isolines (3D), each with its colour.
    pub lines: Vec<([u8; 3], Vec<[f64; 3]>)>,
}

/// What the field layer draws.
#[derive(Debug, Clone, Default)]
pub struct Overlay {
    /// Out of date (drawn faded).
    pub stale: bool,
    /// Isolines from above, 3D, each with its colour.
    pub plan: Vec<([u8; 3], Vec<[f64; 3]>)>,
    /// Isosurfaces, each with its colour.
    pub surfaces: Vec<([u8; 3], Arc<Mesh>)>,
    /// The cuts in the cross-section panels.
    pub cuts: Vec<CutDrawing>,
}

/// The levels shown, ascending.
pub fn shown(levels: &[Level]) -> Vec<Level> {
    let mut out: Vec<Level> = levels
        .iter()
        .filter(|l| l.on && l.value > 0.0)
        .cloned()
        .collect();
    out.sort_by(|a, b| a.value.total_cmp(&b.value));
    out
}

/// A cut as drawn with the levels (ascending): the bands (a cell takes
/// the colour of the highest level at or below its value, translucent;
/// with `fill`) and the isolines.
pub fn cut_drawing(v: &CutValues, levels: &[Level], fill: bool) -> CutDrawing {
    let to3 = |q: [f64; 2]| {
        let p = v.cut.at(q[0]);
        [p[0], p[1], q[1]]
    };
    let mut d = CutDrawing::default();
    if fill {
        let values: Vec<f64> = levels.iter().map(|l| l.value).collect();
        for (band, poly) in v.bands(&values) {
            let rgb = levels[band - 1].rgb;
            d.bands.push((
                [rgb[0], rgb[1], rgb[2], 45],
                poly.into_iter().map(to3).collect(),
            ));
        }
    }
    for l in levels {
        for line in v.isolines(l.value) {
            d.lines.push((l.rgb, line.into_iter().map(to3).collect()));
        }
    }
    d
}

/// The layer's scene.
pub fn scene(origin: [f64; 3], o: &Overlay) -> ohl_render::Scene {
    let mut s = ohl_render::Scene::new(origin);
    let c = |rgb: [u8; 3], a: u8| crate::revision::dimmed([rgb[0], rgb[1], rgb[2], a], o.stale);
    for cut in &o.cuts {
        for (rgba, poly) in &cut.bands {
            let col = crate::revision::dimmed(*rgba, o.stale);
            for k in 1..poly.len().saturating_sub(1) {
                s.triangle([poly[0], poly[k], poly[k + 1]], [col; 3]);
            }
        }
        for (rgb, line) in &cut.lines {
            s.polyline(line, c(*rgb, 255), 2.0);
        }
    }
    for (rgb, line) in &o.plan {
        s.polyline(line, c(*rgb, 255), 2.0);
    }
    for (rgb, mesh) in &o.surfaces {
        let col = c(*rgb, 70);
        for t in &mesh.triangles {
            let [a, b, cc] = t.map(|i| mesh.vertices[i as usize]);
            s.triangle([a, b, cc], [col; 3]);
        }
    }
    s
}

/// The plan: the isolines from above in their colours (the surfaces and
/// cuts are the 3D view's).
pub fn paint_plan(
    painter: &egui::Painter,
    rect: egui::Rect,
    to_screen: &impl Fn([f64; 3]) -> egui::Pos2,
    o: &Overlay,
) {
    for (rgb, line) in &o.plan {
        let col =
            crate::revision::dimmed32(egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]), o.stale);
        let pts: Vec<egui::Pos2> = line.iter().map(|p| to_screen(*p)).collect();
        if pts.iter().any(|p| rect.contains(*p)) {
            painter.add(egui::Shape::line(pts, egui::Stroke::new(1.5, col)));
        }
    }
}
