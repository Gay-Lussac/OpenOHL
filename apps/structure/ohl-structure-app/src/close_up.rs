// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The joint's close-up in the Detail view: the members as prisms of
//! their profile, the plates, the bolts (head, shank, nut), turned by
//! dragging and zoomed by scrolling.
//!
//! Drawn far to near (a painter's list), with no depth buffer. Until
//! 2026-11-03 each face was one polygon sorted by its middle's depth: a
//! member's side face runs from the joint to the drawing's reach, so its
//! middle lies far from the bolts on it, and turned so that the member's
//! far end comes nearer, the whole face sorted in front of the bolts'
//! heads and covered them — the bolts "disappeared" at some angles. Now
//! every face is cut into small triangles (about 40 % of the smallest
//! bolt's diameter near the joint, coarser beyond the bolts), each sorted
//! by its own depth, so a head is compared with the steel right under it;
//! the triangles go into one mesh, in order, with the faces' edges drawn
//! after each of their triangles.

use eframe::egui;
use ohl_structure_link::{BoltPart, JointDetail, Source};
use ohl_structures::connections::PlateKind;

/// A triangle of the close-up in view space (x right, y up, z towards the
/// viewer: larger is nearer, m from the joint), its colour, which of its
/// edges (`p[k]` to `p[k + 1]`) are a face's edges, and the bolt part it
/// belongs to.
#[derive(Debug, Clone, Copy)]
pub struct Tri {
    /// Its corners.
    pub p: [[f64; 3]; 3],
    /// Its colour.
    pub color: egui::Color32,
    /// Its edges drawn.
    pub edges: [bool; 3],
    /// The part of a bolt it is (none: steel or a plate): the tests find
    /// the heads by it.
    #[cfg_attr(not(test), expect(dead_code))]
    pub bolt: Option<BoltPart>,
}

impl Tri {
    /// Its depth: the mean of its corners'.
    pub fn depth(&self) -> f64 {
        (self.p[0][2] + self.p[1][2] + self.p[2][2]) / 3.0
    }
}

/// The view: turned `yaw` about z, tilted `pitch` (radians).
fn view_of(yaw: f32, pitch: f32) -> impl Fn([f64; 3], [f64; 3]) -> [f64; 3] {
    let (cy, sy) = (f64::from(yaw).cos(), f64::from(yaw).sin());
    let (cp, sp) = (f64::from(pitch + 0.35).cos(), f64::from(pitch + 0.35).sin());
    // Seen from the front (−y), turned and tilted: x right, z up.
    move |o: [f64; 3], p: [f64; 3]| {
        let d = [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
        let x = d[0] * cy + d[1] * sy;
        let y = -d[0] * sy + d[1] * cy;
        let up = d[2] * cp - y * sp;
        let depth = d[2] * sp + y * cp;
        [x, up, depth]
    }
}

/// The close-up's triangles, far to near: each face cut to pieces of at
/// most `fine` m within the bolts' reach of the joint, `coarse` beyond.
pub fn triangles_with(
    j: &JointDetail,
    yaw: f32,
    pitch: f32,
    dark: bool,
    fine: f64,
    coarse: f64,
) -> Vec<Tri> {
    let reach = ohl_structure_link::reach(j);
    let view = view_of(yaw, pitch);
    let at = |p: [f64; 3]| view(j.position, p);
    // Within the bolts (and 5 cm more) of the joint: fine.
    let near = j
        .arrivals
        .iter()
        .flat_map(|a| a.bolt_points.iter())
        .map(|q| norm(sub(*q, j.position)))
        .fold(0.15f64, f64::max)
        + 0.05;
    let size = move |p: &[[f64; 3]; 3]| {
        if p.iter().any(|q| norm(*q) < near) {
            fine
        } else {
            coarse
        }
    };
    let light = [0.35, 0.55, 0.75];
    let shade = |base: [f64; 3], alpha: u8, poly: &[[f64; 3]]| -> Option<egui::Color32> {
        let n = cross(sub(poly[1], poly[0]), sub(poly[poly.len() - 1], poly[0]));
        let l = norm(n);
        if l < 1e-15 {
            return None;
        }
        let s = 0.45 + 0.55 * (dot(n, light) / l).abs();
        Some(egui::Color32::from_rgba_unmultiplied(
            (base[0] * s) as u8,
            (base[1] * s) as u8,
            (base[2] * s) as u8,
            alpha,
        ))
    };
    let mut out: Vec<Tri> = Vec::new();
    let mut add =
        |tris: Vec<([[f64; 3]; 3], [bool; 3])>, color: egui::Color32, bolt: Option<BoltPart>| {
            for (p, edges) in tris {
                split(
                    Tri {
                        p,
                        color,
                        edges,
                        bolt,
                    },
                    &size,
                    &mut out,
                    0,
                );
            }
        };
    let steel = if dark {
        [150.0, 165.0, 185.0]
    } else {
        [120.0, 135.0, 155.0]
    };
    for a in &j.arrivals {
        let length = a.length.min(reach);
        let Some(p) = &a.profile else {
            continue;
        };
        let corner = |sd: f64, q: [f64; 2]| {
            at(std::array::from_fn(|i| {
                j.position[i]
                    + a.offset[i]
                    + a.direction[i] * sd
                    + p.axes[1][i] * q[0]
                    + p.axes[2][i] * q[1]
            }))
        };
        let start = a.cut.value.unwrap_or(0.0).min(length);
        let near: Vec<[f64; 3]> = p.outline.iter().map(|q| corner(start, *q)).collect();
        let far: Vec<[f64; 3]> = p.outline.iter().map(|q| corner(length, *q)).collect();
        let n = near.len();
        if n < 3 {
            continue;
        }
        for i in 0..n {
            let i2 = (i + 1) % n;
            let quad = [near[i], near[i2], far[i2], far[i]];
            if let Some(c) = shade(steel, 255, &quad) {
                add(fan(&quad), c, None);
            }
        }
        // The ends: the profile's outline, cut into triangles (an angle's
        // L, a channel, an I…).
        let caps = ear_clip(&p.outline);
        for ends in [&near, &far] {
            if let Some(c) = shade(steel, 255, ends) {
                let tris = caps
                    .iter()
                    .map(|t| {
                        let edge = |u: usize, v: usize| (u + 1) % n == v || (v + 1) % n == u;
                        (
                            [ends[t[0]], ends[t[1]], ends[t[2]]],
                            [edge(t[0], t[1]), edge(t[1], t[2]), edge(t[2], t[0])],
                        )
                    })
                    .collect();
                add(tris, c, None);
            }
        }
    }
    for pl in &j.plates {
        if pl.outline.len() < 3 {
            continue;
        }
        let poly: Vec<[f64; 3]> = pl.outline.iter().map(|q| at(*q)).collect();
        let base = match pl.kind {
            PlateKind::Bolted => [70.0, 140.0, 210.0],
            PlateKind::Welded => [200.0, 135.0, 75.0],
        };
        let c =
            egui::Color32::from_rgba_unmultiplied(base[0] as u8, base[1] as u8, base[2] as u8, 150);
        add(fan(&poly), c, None);
    }
    // The bolts: head, shank and nut, shaded as the members (zinc; orange
    // where their layout is partly assumed).
    for a in &j.arrivals {
        let Some(b) = &a.bolts.value else { continue };
        let assumed = a.gauges.source == Source::Assumed || a.pitch.source == Source::Assumed;
        let base = if assumed {
            [235.0, 170.0, 60.0]
        } else {
            [200.0, 202.0, 196.0]
        };
        for p in &a.bolt_points {
            for (poly, part) in
                ohl_structure_link::bolt_faces(*p, a.bolt_axis, b.diameter, a.grip, b.drawn)
            {
                let poly: Vec<[f64; 3]> = poly.iter().map(|q| at(*q)).collect();
                if let Some(c) = shade(base, 255, &poly) {
                    add(fan(&poly), c, Some(part));
                }
            }
        }
    }
    // Far first (the larger depth is nearer the viewer: drawn last).
    out.sort_by(|a, b| a.depth().total_cmp(&b.depth()));
    out
}

/// The close-up's triangles, far to near, cut as finely as the bolts need.
pub fn triangles(j: &JointDetail, yaw: f32, pitch: f32, dark: bool) -> Vec<Tri> {
    let d = j
        .arrivals
        .iter()
        .filter_map(|a| a.bolts.value.as_ref().map(|b| b.diameter))
        .fold(f64::INFINITY, f64::min);
    let fine = if d.is_finite() {
        (0.4 * d).clamp(0.004, 0.01)
    } else {
        0.01
    };
    triangles_with(j, yaw, pitch, dark, fine, 0.04)
}

/// A convex polygon as a fan of triangles, its own edges marked.
fn fan(poly: &[[f64; 3]]) -> Vec<([[f64; 3]; 3], [bool; 3])> {
    let n = poly.len();
    (1..n.saturating_sub(1))
        .map(|i| {
            (
                [poly[0], poly[i], poly[i + 1]],
                [i == 1, true, i + 1 == n - 1],
            )
        })
        .collect()
}

/// `t` cut in halves across its longest edge until each piece's edges are
/// no longer than `size` of it.
fn split(t: Tri, size: &dyn Fn(&[[f64; 3]; 3]) -> f64, out: &mut Vec<Tri>, depth: u32) {
    let len = |k: usize| norm(sub(t.p[(k + 1) % 3], t.p[k]));
    let (i, longest) = (0..3)
        .map(|k| (k, len(k)))
        .max_by(|x, y| x.1.total_cmp(&y.1))
        .expect("three edges");
    if longest <= size(&t.p) || depth >= 18 {
        out.push(t);
        return;
    }
    let (a, b, o) = (t.p[i], t.p[(i + 1) % 3], t.p[(i + 2) % 3]);
    let m = [
        (a[0] + b[0]) / 2.0,
        (a[1] + b[1]) / 2.0,
        (a[2] + b[2]) / 2.0,
    ];
    let e = t.edges;
    split(
        Tri {
            p: [a, m, o],
            edges: [e[i], false, e[(i + 2) % 3]],
            ..t
        },
        size,
        out,
        depth + 1,
    );
    split(
        Tri {
            p: [m, b, o],
            edges: [e[i], e[(i + 1) % 3], false],
            ..t
        },
        size,
        out,
        depth + 1,
    );
}

/// A simple polygon (2D, either turning) cut into triangles by its ears;
/// the corners' indices.
pub fn ear_clip(poly: &[[f64; 2]]) -> Vec<[usize; 3]> {
    let n = poly.len();
    if n < 3 {
        return Vec::new();
    }
    let area: f64 = (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    let turn = if area >= 0.0 { 1.0 } else { -1.0 };
    let cross2 = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut left: Vec<usize> = (0..n).collect();
    let mut out = Vec::new();
    let mut guard = 0;
    while left.len() > 3 && guard < 4 * n * n {
        guard += 1;
        let m = left.len();
        let mut cut = None;
        for k in 0..m {
            let (ia, ib, ic) = (left[(k + m - 1) % m], left[k], left[(k + 1) % m]);
            let (a, b, c) = (poly[ia], poly[ib], poly[ic]);
            // Convex at b, and no other corner inside a-b-c.
            if cross2(a, b, c) * turn <= 1e-15 {
                continue;
            }
            let inside = left.iter().any(|&q| {
                if q == ia || q == ib || q == ic {
                    return false;
                }
                let p = poly[q];
                cross2(a, b, p) * turn >= 0.0
                    && cross2(b, c, p) * turn >= 0.0
                    && cross2(c, a, p) * turn >= 0.0
            });
            if !inside {
                cut = Some(k);
                out.push([ia, ib, ic]);
                break;
            }
        }
        match cut {
            Some(k) => {
                left.remove(k);
            }
            // Degenerate: the rest as a fan.
            None => break,
        }
    }
    for k in 1..left.len().saturating_sub(1) {
        out.push([left[0], left[k], left[k + 1]]);
    }
    out
}

/// The close-up painted into `rect`: the triangles in one mesh, far to
/// near, each face's edges after its triangles; the members without a
/// profile as lines; the members' names at their cut ends.
pub fn paint(
    painter: &egui::Painter,
    rect: egui::Rect,
    j: &JointDetail,
    s: &crate::joint_panel::JointPanel,
    dark: bool,
) {
    let reach = ohl_structure_link::reach(j);
    let k = f64::from(rect.width().min(rect.height())) / (2.4 * reach) * f64::from(s.zoom).exp();
    let to = |q: [f64; 3]| rect.center() + egui::vec2((q[0] * k) as f32, (-q[1] * k) as f32);
    let edge = egui::Color32::from_black_alpha(140);
    let mut mesh = egui::Mesh::default();
    let line = |mesh: &mut egui::Mesh, a: egui::Pos2, b: egui::Pos2| {
        let d = b - a;
        let l = d.length();
        if l < 1e-3 {
            return;
        }
        let n = egui::vec2(-d.y, d.x) / l * 0.35;
        let i = mesh.vertices.len() as u32;
        for p in [a + n, a - n, b - n, b + n] {
            mesh.colored_vertex(p, edge);
        }
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i, i + 2, i + 3);
    };
    for t in triangles(j, s.yaw, s.pitch, dark) {
        let pts = t.p.map(to);
        let i = mesh.vertices.len() as u32;
        for p in pts {
            mesh.colored_vertex(p, t.color);
        }
        mesh.add_triangle(i, i + 1, i + 2);
        for e in 0..3 {
            if t.edges[e] {
                line(&mut mesh, pts[e], pts[(e + 1) % 3]);
            }
        }
    }
    painter.add(egui::Shape::mesh(mesh));
    let view = view_of(s.yaw, s.pitch);
    let at = |p: [f64; 3]| to(view(j.position, p));
    // Members without a profile: lines.
    for a in j.arrivals.iter().filter(|a| a.profile.is_none()) {
        let far = add3(j.position, scale3(a.direction, a.length.min(reach)));
        painter.line_segment(
            [at(j.position), at(far)],
            egui::Stroke::new(2.0, egui::Color32::GRAY),
        );
    }
    // The labels at the members' cut ends.
    let text = if dark {
        egui::Color32::from_gray(220)
    } else {
        egui::Color32::from_gray(30)
    };
    for a in &j.arrivals {
        let end = add3(j.position, scale3(a.direction, a.length.min(reach) + 0.03));
        painter.text(
            at(end),
            egui::Align2::CENTER_CENTER,
            &a.member,
            egui::FontId::proportional(12.0),
            text,
        );
    }
    painter.circle_filled(at(j.position), 3.0, text);
    painter.text(
        rect.left_top() + egui::vec2(6.0, 6.0),
        egui::Align2::LEFT_TOP,
        format!("joint {} — drag to turn, scroll to zoom", j.id),
        egui::FontId::proportional(11.0),
        text,
    );
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale3(a: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_l_is_cut_into_four_triangles() {
        let l = [
            [0.0, 0.0],
            [0.06, 0.0],
            [0.06, 0.006],
            [0.006, 0.006],
            [0.006, 0.06],
            [0.0, 0.06],
        ];
        let t = ear_clip(&l);
        assert_eq!(t.len(), 4);
        let area: f64 = t
            .iter()
            .map(|t| {
                let (a, b, c) = (l[t[0]], l[t[1]], l[t[2]]);
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() / 2.0
            })
            .sum();
        let want = 0.06 * 0.006 + 0.006 * (0.06 - 0.006);
        assert!((area - want).abs() < 1e-12, "{area} {want}");
    }

    /// The pieces' depths at a point of the screen, painted in order:
    /// the last piece covering it, and the nearest.
    fn at_point(tris: &[Tri], x: f64, y: f64) -> Option<(usize, f64, f64)> {
        let mut last = None;
        let mut nearest = f64::NEG_INFINITY;
        for (i, t) in tris.iter().enumerate() {
            let [a, b, c] = t.p;
            let det = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
            if det.abs() < 1e-14 {
                continue;
            }
            let l1 = ((b[1] - c[1]) * (x - c[0]) + (c[0] - b[0]) * (y - c[1])) / det;
            let l2 = ((c[1] - a[1]) * (x - c[0]) + (a[0] - c[0]) * (y - c[1])) / det;
            let l3 = 1.0 - l1 - l2;
            if l1 < 0.0 || l2 < 0.0 || l3 < 0.0 {
                continue;
            }
            let z = l1 * a[2] + l2 * b[2] + l3 * c[2];
            nearest = nearest.max(z);
            last = Some((i, z));
        }
        last.map(|(i, z)| (i, z, nearest))
    }

    /// How often a bolt's head is painted over by steel nearer the joint's
    /// far side than it (sampled at each head triangle's middle, over many
    /// turns and tilts), out of how many samples.
    fn hidden_heads(j: &JointDetail, fine: f64, coarse: f64) -> (usize, usize) {
        let (mut wrong, mut all) = (0, 0);
        for yaw in 0..12 {
            for pitch in -1..=1 {
                let (yaw, pitch) = (yaw as f32 * 0.52, pitch as f32 * 0.6);
                let tris = triangles_with(j, yaw, pitch, false, fine, coarse);
                for t in tris
                    .iter()
                    .filter(|t| t.bolt == Some(BoltPart::Head))
                    .step_by(3)
                {
                    let x = (t.p[0][0] + t.p[1][0] + t.p[2][0]) / 3.0;
                    let y = (t.p[0][1] + t.p[1][1] + t.p[2][1]) / 3.0;
                    let Some((_, z, nearest)) = at_point(&tris, x, y) else {
                        continue;
                    };
                    all += 1;
                    // Painted over by something that lies 1 mm or more
                    // behind what is nearest there.
                    if z < nearest - 0.001 {
                        wrong += 1;
                    }
                }
            }
        }
        (wrong, all)
    }

    /// The bolts no longer disappear when the close-up is turned (owner
    /// 2026-11-03): whole faces sorted by their middles hid heads at some
    /// angles; the faces cut small do not (under 0.5 % of the samples,
    /// where pieces meet edge-on).
    #[test]
    fn bolts_stay_visible_when_turned() {
        let lib = ohl_structure_link::section_library(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../libraries/sections/example-sections.ohlsec"),
        )
        .unwrap();
        let text = crate::joint_ui_tests::ANGLES;
        let d = ohl_structure_link::joint_details(text, &lib, &Default::default()).unwrap();
        let j = d.joint("N2").unwrap();
        assert!(j.arrivals.iter().any(|a| !a.bolt_points.is_empty()));
        let (old, n_old) = hidden_heads(j, f64::INFINITY, f64::INFINITY);
        eprintln!("whole faces: {old} of {n_old} head samples hidden");
        assert!(old > 0, "whole faces hide heads ({old} of {n_old})");
        let (now, n) = hidden_heads(j, 0.0064, 0.04);
        eprintln!("small pieces: {now} of {n} head samples hidden");
        assert!(
            (now as f64) < 0.005 * n as f64,
            "{now} of {n} head samples hidden (whole faces: {old} of {n_old})"
        );
    }
}
