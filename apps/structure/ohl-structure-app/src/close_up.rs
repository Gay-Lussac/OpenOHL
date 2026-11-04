// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The joint's close-up in the Detail view: the members as prisms of
//! their profile, the plates (a welded assembly's seams too), the bolts
//! (head, shank, nut), turned by dragging and zoomed by scrolling.
//!
//! Drawn far to near (a painter's list), with no depth buffer. Until
//! 2026-11-03 each face was one polygon sorted by its middle's depth: a
//! member's side face runs from the joint to the drawing's reach, so its
//! middle lies far from the bolts on it, and turned so that the member's
//! far end comes nearer, the whole face sorted in front of the bolts'
//! heads and covered them — the bolts "disappeared" at some angles. Since
//! then every face is cut into small triangles, each sorted by its own
//! depth, so a head is compared with the steel right under it.
//!
//! Smooth again (owner 2026-11-04: "the joint view is a bit laggy now …
//! but the bolts are good"): the pieces are cut **once** per joint and
//! file revision ([`CloseUpCache`]), in the structure's axes (a turn of
//! the view does not change them), finely only near the bolts — a piece's
//! size grows with its distance from the nearest bolt
//! ([`pieces_with`]) — and never more than [`MAX_PIECES`]; a turn sorts
//! them again (one key per piece), and the mesh drawn is kept until the
//! view, the size of the pane or the file changes.

use std::sync::Arc;

use eframe::egui;
use ohl_structure_link::{BoltPart, JointDetail, Source};

/// The most pieces a close-up is cut into: beyond it the finest size is
/// made coarser until it fits (a joint with very many bolts).
pub const MAX_PIECES: usize = 40_000;

/// How fast a piece may grow with its distance from the nearest bolt
/// beyond [`NEAR`]: its edges at most `fine + GROWTH × distance` (and
/// `coarse`).
const GROWTH: f64 = 0.5;

/// How far round a bolt the pieces stay at the finest size, m (a head
/// and its nut with some room).
const NEAR: f64 = 0.02;

/// A piece of the close-up, view-independent: a triangle in the
/// structure's axes from the joint (m), its colour, its face's normal (none:
/// drawn unshaded, a plate), which of its edges (`p[k]` to `p[k + 1]`) are
/// a face's edges, and the bolt part it belongs to.
#[derive(Debug, Clone, Copy)]
pub struct Piece {
    /// Its corners.
    pub p: [[f64; 3]; 3],
    /// Its colour before shading (rgb) and its alpha.
    pub base: [u8; 4],
    /// Its face's unit normal (none: not shaded).
    pub normal: Option<[f64; 3]>,
    /// Its edges drawn.
    pub edges: [bool; 3],
    /// The part of a bolt it is (none: steel, a plate, a weld).
    pub bolt: Option<BoltPart>,
}

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

/// The view's rotation, turned `yaw` about z and tilted `pitch` (radians):
/// its rows are the view's x (right), up and depth (towards the viewer)
/// in the structure's axes. Seen from the front (−y), z up.
fn rotation(yaw: f32, pitch: f32) -> [[f64; 3]; 3] {
    let (sy, cy) = f64::from(yaw).sin_cos();
    let (sp, cp) = f64::from(pitch + 0.35).sin_cos();
    [
        [cy, sy, 0.0],
        [sp * sy, -sp * cy, cp],
        [-cp * sy, cp * cy, sp],
    ]
}

fn turn(r: &[[f64; 3]; 3], d: [f64; 3]) -> [f64; 3] {
    [dot(r[0], d), dot(r[1], d), dot(r[2], d)]
}

/// The light, from the viewer's upper right, in the view's axes.
const LIGHT: [f64; 3] = [0.35, 0.55, 0.75];

/// The close-up's pieces of joint `j`, cut to at most `fine` m at the
/// bolts, growing with the distance from the nearest bolt up to `coarse`.
pub fn pieces_with(j: &JointDetail, dark: bool, fine: f64, coarse: f64) -> Vec<Piece> {
    let reach = ohl_structure_link::reach(j);
    let o = j.position;
    let rel = |p: [f64; 3]| sub(p, o);
    let bolts: Vec<[f64; 3]> = j
        .arrivals
        .iter()
        .flat_map(|a| a.bolt_points.iter())
        .map(|q| rel(*q))
        .collect();
    // A lower bound of a triangle's distance to the nearest bolt: from its
    // middle, less its corners' reach from there.
    let size = move |p: &[[f64; 3]; 3]| {
        if bolts.is_empty() {
            return coarse;
        }
        let c = [
            (p[0][0] + p[1][0] + p[2][0]) / 3.0,
            (p[0][1] + p[1][1] + p[2][1]) / 3.0,
            (p[0][2] + p[1][2] + p[2][2]) / 3.0,
        ];
        let r = p.iter().map(|q| norm(sub(*q, c))).fold(0.0, f64::max);
        let d = bolts
            .iter()
            .map(|b| norm(sub(*b, c)))
            .fold(f64::INFINITY, f64::min);
        (fine + GROWTH * (d - r - NEAR).max(0.0)).min(coarse)
    };
    let normal_of = |poly: &[[f64; 3]]| -> Option<[f64; 3]> {
        let n = cross(sub(poly[1], poly[0]), sub(poly[poly.len() - 1], poly[0]));
        let l = norm(n);
        (l >= 1e-15).then(|| scale3(n, 1.0 / l))
    };
    let mut out: Vec<Piece> = Vec::new();
    let mut add = |tris: Vec<([[f64; 3]; 3], [bool; 3])>,
                   base: [u8; 4],
                   normal: Option<[f64; 3]>,
                   bolt: Option<BoltPart>| {
        for (p, edges) in tris {
            split(
                Piece {
                    p,
                    base,
                    normal,
                    edges,
                    bolt,
                },
                &size,
                &mut out,
                0,
            );
        }
    };
    let steel: [u8; 4] = if dark {
        [150, 165, 185, 255]
    } else {
        [120, 135, 155, 255]
    };
    for a in &j.arrivals {
        let length = a.length.min(reach);
        let Some(p) = &a.profile else {
            continue;
        };
        let corner = |sd: f64, q: [f64; 2]| {
            std::array::from_fn(|i| {
                a.offset[i] + a.direction[i] * sd + p.axes[1][i] * q[0] + p.axes[2][i] * q[1]
            })
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
            if let Some(nv) = normal_of(&quad) {
                add(fan(&quad), steel, Some(nv), None);
            }
        }
        // The ends: the profile's outline, cut into triangles (an angle's
        // L, a channel, an I…).
        let caps = ear_clip(&p.outline);
        for ends in [&near, &far] {
            if let Some(nv) = normal_of(ends) {
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
                add(tris, steel, Some(nv), None);
            }
        }
    }
    for pl in &j.plates {
        if pl.outline.len() < 3 {
            continue;
        }
        let poly: Vec<[f64; 3]> = pl.outline.iter().map(|q| rel(*q)).collect();
        // A plate on its own blue; welded with others (an assembly) amber.
        let base = if pl.assembly.len() > 1 {
            [200, 135, 75, 150]
        } else {
            [70, 140, 210, 150]
        };
        add(fan(&poly), base, None, None);
        // Its weld seams: a fillet along each, between the two plates.
        for w in &pl.welds {
            let [s0, s1] = w.ends.map(rel);
            let leg = (pl.thickness * 0.8).max(0.004);
            let [da, db] = w.sides;
            let quad = [
                add3(s0, scale3(da, leg)),
                add3(s1, scale3(da, leg)),
                add3(s1, scale3(db, leg)),
                add3(s0, scale3(db, leg)),
            ];
            if let Some(nv) = normal_of(&quad) {
                add(fan(&quad), [215, 70, 50, 255], Some(nv), None);
            }
        }
    }
    // The bolts: head, shank and nut, shaded as the members (zinc; orange
    // where their layout is partly assumed).
    for a in &j.arrivals {
        let Some(b) = &a.bolts.value else { continue };
        let assumed = a.gauges.source == Source::Assumed || a.pitch.source == Source::Assumed;
        let base = if assumed {
            [235, 170, 60, 255]
        } else {
            [200, 202, 196, 255]
        };
        for p in &a.bolt_points {
            for (poly, part) in
                ohl_structure_link::bolt_faces(*p, a.bolt_axis, b.diameter, a.grip, b.drawn)
            {
                let poly: Vec<[f64; 3]> = poly.iter().map(|q| rel(*q)).collect();
                if let Some(nv) = normal_of(&poly) {
                    add(fan(&poly), base, Some(nv), Some(part));
                }
            }
        }
    }
    out
}

/// The finest size of the close-up of `j`: 40 % of its smallest bolt's
/// diameter (4 to 10 mm).
fn fine_of(j: &JointDetail) -> f64 {
    let d = j
        .arrivals
        .iter()
        .filter_map(|a| a.bolts.value.as_ref().map(|b| b.diameter))
        .fold(f64::INFINITY, f64::min);
    if d.is_finite() {
        (0.4 * d).clamp(0.004, 0.01)
    } else {
        0.01
    }
}

/// The close-up's pieces, cut as finely as the bolts need, never more
/// than [`MAX_PIECES`] (the finest size made coarser until they fit).
pub fn pieces(j: &JointDetail, dark: bool) -> Vec<Piece> {
    let mut fine = fine_of(j);
    loop {
        let out = pieces_with(j, dark, fine, 0.04);
        if out.len() <= MAX_PIECES || fine >= 0.04 {
            return out;
        }
        fine *= 1.5;
    }
}

/// The pieces seen turned `yaw` and tilted `pitch`: in view space,
/// shaded, sorted far to near.
pub fn project(pieces: &[Piece], yaw: f32, pitch: f32) -> Vec<Tri> {
    let r = rotation(yaw, pitch);
    // The light in the structure's axes (the rotation's transpose).
    let light: [f64; 3] = std::array::from_fn(|i| (0..3).map(|k| r[k][i] * LIGHT[k]).sum());
    let mut out: Vec<Tri> = pieces
        .iter()
        .map(|p| {
            let color = match p.normal {
                Some(n) => {
                    let s = 0.45 + 0.55 * dot(n, light).abs();
                    egui::Color32::from_rgba_unmultiplied(
                        (f64::from(p.base[0]) * s) as u8,
                        (f64::from(p.base[1]) * s) as u8,
                        (f64::from(p.base[2]) * s) as u8,
                        p.base[3],
                    )
                }
                None => egui::Color32::from_rgba_unmultiplied(
                    p.base[0], p.base[1], p.base[2], p.base[3],
                ),
            };
            Tri {
                p: p.p.map(|q| turn(&r, q)),
                color,
                edges: p.edges,
                bolt: p.bolt,
            }
        })
        .collect();
    // Far first (the larger depth is nearer the viewer: drawn last); one
    // key per piece.
    out.sort_unstable_by(|a, b| a.depth().total_cmp(&b.depth()));
    out
}

/// The close-up's triangles, far to near: cut to pieces of `fine` m at
/// the bolts, growing with the distance from them up to `coarse`.
#[cfg(test)]
pub fn triangles_with(
    j: &JointDetail,
    yaw: f32,
    pitch: f32,
    dark: bool,
    fine: f64,
    coarse: f64,
) -> Vec<Tri> {
    project(&pieces_with(j, dark, fine, coarse), yaw, pitch)
}

/// The close-up's triangles, far to near, cut as finely as the bolts need.
#[cfg(test)]
pub fn triangles(j: &JointDetail, yaw: f32, pitch: f32, dark: bool) -> Vec<Tri> {
    project(&pieces(j, dark), yaw, pitch)
}

/// What the close-up keeps between frames: its pieces, cut once per
/// joint, file revision, libraries and theme; the mesh drawn, kept while
/// the view and the pane stay the same.
#[derive(Default)]
pub struct CloseUpCache {
    /// The pieces and what they were cut for: (file revision, libraries
    /// stamp), the joint, dark.
    built: Option<(CutKey, Arc<Vec<Piece>>)>,
    /// The mesh drawn and the view it was drawn for.
    drawn: Option<(ViewKey, Arc<egui::Mesh>)>,
    /// How often the pieces were cut (the tests: once per joint).
    pub builds: usize,
    /// How often they were sorted (a turn, a zoom, a pane resized).
    pub sorts: usize,
}

/// What the pieces were cut for: (file revision, libraries stamp), the
/// joint, dark.
type CutKey = ((u64, u64), String, bool);

/// The view a mesh was drawn for: the cut's key, turn, tilt, zoom and
/// the pane's rectangle (as bits: the same frame to frame).
type ViewKey = (CutKey, [u32; 7]);

impl CloseUpCache {
    /// The pieces of `j`, cut again only when `key`, the joint or the
    /// theme changed.
    pub fn pieces(&mut self, key: (u64, u64), j: &JointDetail, dark: bool) -> Arc<Vec<Piece>> {
        let k = (key, j.id.clone(), dark);
        if let Some((have, p)) = &self.built
            && *have == k
        {
            return p.clone();
        }
        let p = Arc::new(pieces(j, dark));
        self.builds += 1;
        self.built = Some((k, p.clone()));
        self.drawn = None;
        p
    }

    /// How many pieces the joint shown is cut into.
    #[cfg(test)]
    pub fn count(&self) -> usize {
        self.built.as_ref().map_or(0, |b| b.1.len())
    }
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
fn split(t: Piece, size: &dyn Fn(&[[f64; 3]; 3]) -> f64, out: &mut Vec<Piece>, depth: u32) {
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
        Piece {
            p: [a, m, o],
            edges: [e[i], false, e[(i + 2) % 3]],
            ..t
        },
        size,
        out,
        depth + 1,
    );
    split(
        Piece {
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

/// The close-up painted into `rect`: the pieces (cut once, [`CloseUpCache`])
/// sorted for the view in one mesh, far to near, each face's edges after
/// its triangles — the mesh kept while the view and the pane stay the
/// same; the members without a profile as lines; the members' names at
/// their cut ends. `key`: the file revision and libraries the details are
/// of.
pub fn paint(
    painter: &egui::Painter,
    rect: egui::Rect,
    j: &JointDetail,
    s: &mut crate::joint_panel::JointPanel,
    key: (u64, u64),
    dark: bool,
) {
    let reach = ohl_structure_link::reach(j);
    let k = f64::from(rect.width().min(rect.height())) / (2.4 * reach) * f64::from(s.zoom).exp();
    let to = |q: [f64; 3]| rect.center() + egui::vec2((q[0] * k) as f32, (-q[1] * k) as f32);
    let pieces = s.close_up.pieces(key, j, dark);
    let view: ViewKey = (
        (key, j.id.clone(), dark),
        [
            s.yaw.to_bits(),
            s.pitch.to_bits(),
            s.zoom.to_bits(),
            rect.min.x.to_bits(),
            rect.min.y.to_bits(),
            rect.max.x.to_bits(),
            rect.max.y.to_bits(),
        ],
    );
    let mesh = match &s.close_up.drawn {
        Some((v, m)) if *v == view => m.clone(),
        _ => {
            let m = Arc::new(mesh_of(&project(&pieces, s.yaw, s.pitch), &to));
            s.close_up.sorts += 1;
            s.close_up.drawn = Some((view, m.clone()));
            m
        }
    };
    painter.add(egui::Shape::Mesh(mesh));
    let r = rotation(s.yaw, s.pitch);
    let at = |p: [f64; 3]| to(turn(&r, sub(p, j.position)));
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

/// The triangles `tris` (in order) as one mesh on the screen (`to`), each
/// face's edges as thin quads after its triangle.
fn mesh_of(tris: &[Tri], to: &dyn Fn([f64; 3]) -> egui::Pos2) -> egui::Mesh {
    let edge = egui::Color32::from_black_alpha(140);
    let mut mesh = egui::Mesh::default();
    let edges: usize = tris
        .iter()
        .map(|t| t.edges.iter().filter(|e| **e).count())
        .sum();
    mesh.reserve_vertices(3 * tris.len() + 4 * edges);
    mesh.reserve_triangles(tris.len() + 2 * edges);
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
    for t in tris {
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
    mesh
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

#[cfg(test)]
mod timing {
    /// A heavier joint: every member bolted, a plate with three members.
    pub(crate) fn heavy() -> String {
        let mut t = crate::joint_ui_tests::ANGLES.replace(
            "[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nonto = \"L1\"",
            "[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nonto = \"G1\"",
        );
        t.push_str(
            "\n[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
             \n[[connection]]\nmember = \"H1\"\njoint = \"N2\"\nonto = \"G1\"\n\
             bolts = { count = 6, diameter = \"16 mm\" }\ngauge_lines = 2\n\
             pitch = \"50 mm\"\nend_distance = \"30 mm\"\n\
             \n[[connection]]\nmember = \"L2\"\njoint = \"N2\"\nonto = \"L1\"\n\
             bolts = { count = 8, diameter = \"12 mm\" }\ngauge_lines = 2\n\
             pitch = \"40 mm\"\nend_distance = \"25 mm\"\n",
        );
        t
    }

    fn frame(
        ctx: &eframe::egui::Context,
        j: &ohl_structure_link::JointDetail,
        panel: &mut crate::joint_panel::JointPanel,
        key: (u64, u64),
    ) {
        let mut out = ctx.run_ui(Default::default(), |ui| {
            let rect = eframe::egui::Rect::from_min_size(
                eframe::egui::pos2(0.0, 0.0),
                eframe::egui::vec2(800.0, 600.0),
            );
            super::paint(ui.painter(), rect, j, panel, key, false);
        });
        out.textures_delta.clear();
    }

    /// The close-up stays smooth (owner 2026-11-04: "the joint view is a
    /// bit laggy now"): its pieces are bounded ([`super::MAX_PIECES`]) and
    /// far fewer than when every face was cut finely all over, cut once
    /// per joint and file revision, sorted again only when the view turns
    /// (frames without a change reuse the mesh) — timings are left to
    /// `close_up_timing`, run by hand.
    #[test]
    fn the_close_up_is_cut_once_and_bounded() {
        let lib = ohl_structure_link::section_library(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../libraries/sections/example-sections.ohlsec"),
        )
        .unwrap();
        let d = ohl_structure_link::joint_details(&heavy(), &lib, &Default::default()).unwrap();
        let j = d.joint("N2").unwrap();
        let fine = super::fine_of(j);
        let all_fine = super::pieces_with(j, false, fine, fine).len();
        let ctx = eframe::egui::Context::default();
        let mut panel = crate::joint_panel::JointPanel::default();
        for _ in 0..5 {
            frame(&ctx, j, &mut panel, (1, 0));
        }
        let n = panel.close_up.count();
        assert!(n <= super::MAX_PIECES, "{n} pieces");
        assert!(
            3 * n < all_fine,
            "{n} pieces, {all_fine} cut finely all over"
        );
        assert_eq!((panel.close_up.builds, panel.close_up.sorts), (1, 1));
        // Turned: sorted again, not cut again.
        panel.yaw = 0.7;
        frame(&ctx, j, &mut panel, (1, 0));
        frame(&ctx, j, &mut panel, (1, 0));
        assert_eq!((panel.close_up.builds, panel.close_up.sorts), (1, 2));
        // The file changed (a new revision): cut again.
        frame(&ctx, j, &mut panel, (2, 0));
        assert_eq!((panel.close_up.builds, panel.close_up.sorts), (2, 3));
    }

    #[test]
    #[ignore = "timing, run by hand"]
    fn close_up_timing() {
        let lib = ohl_structure_link::section_library(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../libraries/sections/example-sections.ohlsec"),
        )
        .unwrap();
        for (name, text) in [
            ("angles", crate::joint_ui_tests::ANGLES.to_owned()),
            ("heavy", heavy()),
        ] {
            let d = ohl_structure_link::joint_details(&text, &lib, &Default::default()).unwrap();
            let j = d.joint("N2").unwrap();
            let n = 20;
            let t0 = std::time::Instant::now();
            let mut count = 0;
            for k in 0..n {
                count = super::triangles(j, k as f32 * 0.1, 0.2, false).len();
            }
            let dt = t0.elapsed().as_secs_f64() * 1000.0 / n as f64;
            eprintln!("{name}: {count} triangles, {dt:.2} ms per frame (triangles only)");
            // A whole frame: painted and tessellated, turned each frame.
            let ctx = eframe::egui::Context::default();
            let mut panel = crate::joint_panel::JointPanel::default();
            let t0 = std::time::Instant::now();
            for k in 0..n {
                panel.yaw = k as f32 * 0.1;
                let mut out = ctx.run_ui(Default::default(), |ui| {
                    let rect = eframe::egui::Rect::from_min_size(
                        eframe::egui::pos2(0.0, 0.0),
                        eframe::egui::vec2(800.0, 600.0),
                    );
                    super::paint(ui.painter(), rect, j, &mut panel, (0, 0), false);
                });
                out.textures_delta.clear();
                let _ = ctx.tessellate(out.shapes, 1.0);
            }
            let dt = t0.elapsed().as_secs_f64() * 1000.0 / n as f64;
            eprintln!("{name}: {dt:.2} ms per frame turning (paint + tessellate)");
            // Frames without a turn (hover, another panel): the mesh kept.
            let t0 = std::time::Instant::now();
            for _ in 0..n {
                let mut out = ctx.run_ui(Default::default(), |ui| {
                    let rect = eframe::egui::Rect::from_min_size(
                        eframe::egui::pos2(0.0, 0.0),
                        eframe::egui::vec2(800.0, 600.0),
                    );
                    super::paint(ui.painter(), rect, j, &mut panel, (0, 0), false);
                });
                out.textures_delta.clear();
                let _ = ctx.tessellate(out.shapes, 1.0);
            }
            let dt = t0.elapsed().as_secs_f64() * 1000.0 / n as f64;
            eprintln!(
                "{name}: {dt:.2} ms per frame still (paint + tessellate; {} cut, {} sorted)",
                panel.close_up.builds, panel.close_up.sorts
            );
            let t0 = std::time::Instant::now();
            let p = super::pieces(j, false);
            eprintln!(
                "{name}: cut once in {:.2} ms ({} pieces)",
                t0.elapsed().as_secs_f64() * 1000.0,
                p.len()
            );
        }
    }
}
