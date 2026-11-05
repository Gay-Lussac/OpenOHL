// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The members' steel as the joint details place it (owner 2026-11-05:
//! "In the 3d view of the OHL structure, I need the profiles to be shown
//! with there actual right orientation and placement as defined in the
//! joints and members detail view"): one builder for the Detail view's
//! close-up and the structure's 3D view.
//!
//! A member end's steel ([`section_at`]): its profile's outline
//! ([`crate::Profile::outline`], about the centroid) turned by its axes
//! (the reference and the roll, as the finite-element model turns it),
//! moved by the end's [`crate::Arrival::offset`] (an angle seated with its
//! leg bolted flat on what it bolts onto, the end's sideways shift) and
//! starting where its steel stops ([`crate::Arrival::cut`]). The close-up
//! runs that section from the cut out to its reach; the 3D view
//! ([`member_solids`]) runs it from the cut at one end to the cut at the
//! other: the same corners at both ends, and between them a straight
//! prism (when the two ends are seated differently, the section moves
//! linearly from one end's place to the other's — each end exactly as
//! the close-up draws it). The work line still runs to the work points;
//! the steel stops short of them by the cuts.
//!
//! A member running on through a joint into the next as one part
//! (`kind = "continuous"`, 2026-11-05) has no cut, no shift and no end
//! face there: the two members' solids meet in one section, one solid
//! through the joint. The plates and a splice's covers are solids of
//! their thickness on their outlines ([`plate_solids`]).
//!
//! Display only: nothing here is used by a calculation.

use crate::joints::{Arrival, JointDetails, Profile};

/// One flat face of a solid: its triangles (each with the edges that are
/// the face's own, `p[k]` to `p[k + 1]`) and its outward unit normal.
#[derive(Debug, Clone, PartialEq)]
pub struct SolidFace {
    /// Its triangles and their face edges.
    pub triangles: Vec<([[f64; 3]; 3], [bool; 3])>,
    /// Its unit normal, out of the solid.
    pub normal: [f64; 3],
}

/// A member's steel as the joint details place it, structure axes, m.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberSolid {
    /// The member.
    pub member: String,
    /// Its section's name.
    pub section: String,
    /// Its outline is the section's shape (false: a section of values
    /// only, drawn as a square of its area, as the close-up draws it).
    pub shaped: bool,
    /// Its joints (first, second).
    pub joints: [String; 2],
    /// Its work points (the joints' positions): the work line the model
    /// and the 3D view's picking use.
    pub work: [[f64; 3]; 2],
    /// Where its steel stops from each work point along it, m.
    pub cut: [f64; 2],
    /// The section's corners where its steel stops at each end
    /// ([`section_at`] from that end's joint), in the outline's order.
    pub ends: [Vec<[f64; 3]>; 2],
    /// Its faces: the sides and the two ends (not at an end where it
    /// runs on into the next member as one part).
    pub faces: Vec<SolidFace>,
    /// It runs on into the next member as one part at each end (no end
    /// face there).
    pub runs_on: [bool; 2],
    /// It does not fit at an end as given ([`Arrival::misfit`]): drawn red.
    pub misfit: bool,
}

impl MemberSolid {
    /// Its section's size: the widest across its first end's corners, m.
    pub fn size(&self) -> f64 {
        let r = &self.ends[0];
        let mut out: f64 = 0.0;
        for a in r {
            for b in r {
                out = out.max(norm(sub(*a, *b)));
            }
        }
        out
    }

    /// Its triangles.
    pub fn triangle_count(&self) -> usize {
        self.faces.iter().map(|f| f.triangles.len()).sum()
    }
}

/// The corners of `a`'s section `along` m from its joint along it,
/// relative to the joint (structure axes): its profile `p`'s outline
/// turned by its axes, moved by the end's offset. The close-up's and the
/// 3D view's corners alike.
pub fn section_at(a: &Arrival, p: &Profile, along: f64) -> Vec<[f64; 3]> {
    p.outline
        .iter()
        .map(|q| {
            std::array::from_fn(|i| {
                a.offset[i] + a.direction[i] * along + p.axes[1][i] * q[0] + p.axes[2][i] * q[1]
            })
        })
        .collect()
}

/// Where `a`'s steel starts from its joint, m (its cut; 0 when none; below
/// 0 where it runs on past the work point, over the bolts of a member
/// nested in it — at most half its length).
pub fn steel_start(a: &Arrival) -> f64 {
    a.cut.value.unwrap_or(0.0).max(-a.length / 2.0)
}

/// The prism of `outline` (the profile's, in the section's plane) from the
/// corners `near` to the corners `far` (the same outline placed at two
/// places, in its order; `ey`, `ez` the section's axes): its side faces
/// and its two ends (each end's outline cut into triangles by its ears),
/// every normal out of the solid. None when the outline has fewer than
/// three corners or the two places are the same.
pub fn prism_faces(
    near: &[[f64; 3]],
    far: &[[f64; 3]],
    outline: &[[f64; 2]],
    ey: [f64; 3],
    ez: [f64; 3],
) -> Option<Vec<SolidFace>> {
    prism_faces_capped(near, far, outline, ey, ez, [true, true])
}

/// [`prism_faces`] with its end faces only where `caps` says (near, far):
/// none where a member runs on into the next as one part.
pub fn prism_faces_capped(
    near: &[[f64; 3]],
    far: &[[f64; 3]],
    outline: &[[f64; 2]],
    ey: [f64; 3],
    ez: [f64; 3],
    caps: [bool; 2],
) -> Option<Vec<SolidFace>> {
    let n = outline.len();
    if n < 3 || near.len() != n || far.len() != n {
        return None;
    }
    let along = sub(mean(far), mean(near));
    if norm(along) < 1e-12 {
        return None;
    }
    // The outline's turning: +1 counter-clockwise in (y, z).
    let area: f64 = (0..n)
        .map(|i| {
            let (a, b) = (outline[i], outline[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    let turning = if area >= 0.0 { 1.0 } else { -1.0 };
    let mut faces = Vec::with_capacity(n + 2);
    for i in 0..n {
        let i2 = (i + 1) % n;
        let quad = [near[i], near[i2], far[i2], far[i]];
        let Some(mut nv) = normal_of(&quad) else {
            continue;
        };
        // Out of the solid: the edge's outward normal in the section's
        // plane, (dz, −dy) for a counter-clockwise outline.
        let e = [
            outline[i2][0] - outline[i][0],
            outline[i2][1] - outline[i][1],
        ];
        let out2 = [e[1] * turning, -e[0] * turning];
        let out3: [f64; 3] = std::array::from_fn(|k| ey[k] * out2[0] + ez[k] * out2[1]);
        if dot(nv, out3) < 0.0 {
            nv = scale(nv, -1.0);
        }
        faces.push(SolidFace {
            triangles: fan(&quad),
            normal: nv,
        });
    }
    let ears = ear_clip(outline);
    let unit_along = scale(along, 1.0 / norm(along));
    for (ends, outward, cap) in [
        (near, scale(unit_along, -1.0), caps[0]),
        (far, unit_along, caps[1]),
    ] {
        if !cap {
            continue;
        }
        let Some(mut nv) = normal_of(ends) else {
            continue;
        };
        if dot(nv, outward) < 0.0 {
            nv = scale(nv, -1.0);
        }
        let edge = |u: usize, v: usize| (u + 1) % n == v || (v + 1) % n == u;
        let triangles = ears
            .iter()
            .map(|t| {
                (
                    [ends[t[0]], ends[t[1]], ends[t[2]]],
                    [edge(t[0], t[1]), edge(t[1], t[2]), edge(t[2], t[0])],
                )
            })
            .collect();
        faces.push(SolidFace {
            triangles,
            normal: nv,
        });
    }
    Some(faces)
}

/// Every member's steel with a profile at both its ends, as the joint
/// details place it: from the cut at its first joint to the cut at its
/// second, each end's section where the close-up draws it. Members
/// without a profile (no cross-section, below level 3) have none: the
/// views draw them as their work line.
pub fn member_solids(d: &JointDetails) -> Vec<MemberSolid> {
    // Each member's two ends: the joint and the arrival there.
    let mut ends: std::collections::BTreeMap<&str, [Option<End<'_>>; 2]> =
        std::collections::BTreeMap::new();
    let mut order: Vec<&str> = Vec::new();
    for j in &d.joints {
        for a in &j.arrivals {
            let e = ends.entry(a.member.as_str()).or_insert_with(|| {
                order.push(a.member.as_str());
                [None, None]
            });
            e[a.end.min(1)] = Some((j.id.as_str(), j.position, a));
        }
    }
    let mut out = Vec::new();
    for m in order {
        let [Some(e0), Some(e1)] = ends[m] else {
            continue;
        };
        if let Some(s) = solid_of(m, e0, e1) {
            out.push(s);
        }
    }
    out
}

/// A member end: its joint, the joint's position and the arrival there.
type End<'a> = (&'a str, [f64; 3], &'a Arrival);

/// The solid of member `m` between its ends `e0` and `e1` (joint, work
/// point, arrival).
fn solid_of(m: &str, e0: End<'_>, e1: End<'_>) -> Option<MemberSolid> {
    let (a0, a1) = (e0.2, e1.2);
    let p = a0.profile.as_ref().or(a1.profile.as_ref())?;
    let length = a0.length;
    // The cuts, the second within what the first leaves.
    let c0 = steel_start(a0).min(length);
    let c1 = steel_start(a1).min(length - c0);
    let runs_on = [a0.continuous.is_some(), a1.continuous.is_some()];
    let at = |e: End<'_>, c: f64| -> Vec<[f64; 3]> {
        let prof = e.2.profile.as_ref().unwrap_or(p);
        section_at(e.2, prof, c)
            .into_iter()
            .map(|q| add(q, e.1))
            .collect()
    };
    let near = at(e0, c0);
    let far = at(e1, c1);
    let caps = runs_on.map(|r| !r);
    let faces = prism_faces_capped(&near, &far, &p.outline, p.axes[1], p.axes[2], caps)?;
    Some(MemberSolid {
        member: m.to_owned(),
        section: p.section.clone(),
        shaped: p.shaped,
        joints: [e0.0.to_owned(), e1.0.to_owned()],
        work: [e0.1, e1.1],
        cut: [c0, c1],
        ends: [near, far],
        faces,
        runs_on,
        misfit: a0.misfit || a1.misfit,
    })
}

/// A plate or a splice's cover as a solid: its outline run through its
/// thickness (structure axes, m).
#[derive(Debug, Clone, PartialEq)]
pub struct PlateSolid {
    /// The plate's id, or the splice's id and the cover ("S1 outer long").
    pub id: String,
    /// Its joint.
    pub joint: String,
    /// A splice's cover (else a gusset plate).
    pub cover: bool,
    /// Its thickness, m.
    pub thickness: f64,
    /// Its outline's corners on its two faces.
    pub ends: [Vec<[f64; 3]>; 2],
    /// Its faces.
    pub faces: Vec<SolidFace>,
    /// Something at it does not fit (a member end onto it, its bolts too
    /// near its edge): drawn red.
    pub misfit: bool,
}

/// Every joint's plates and splices' covers with an outline as solids:
/// the outline from its face through its thickness (the close-up's way).
pub fn plate_solids(d: &JointDetails) -> Vec<PlateSolid> {
    let mut out = Vec::new();
    for j in &d.joints {
        let misfit_onto = |id: &str| {
            j.arrivals
                .iter()
                .any(|a| a.misfit && a.onto.value.as_deref() == Some(id))
        };
        for p in &j.plates {
            if let Some(s) = slab(&p.outline, p.through, p.thickness) {
                out.push(PlateSolid {
                    id: p.id.clone(),
                    joint: j.id.clone(),
                    cover: false,
                    thickness: p.thickness,
                    ends: s.0,
                    faces: s.1,
                    misfit: misfit_onto(&p.id) || (p.outline_given && !p.notes.is_empty()),
                });
            }
        }
        for sp in &j.splices {
            for c in &sp.covers {
                if let Some(s) = slab(&c.outline, c.through, c.thickness) {
                    out.push(PlateSolid {
                        id: format!("{} {}", sp.id, c.words()),
                        joint: j.id.clone(),
                        cover: true,
                        thickness: c.thickness,
                        ends: s.0,
                        faces: s.1,
                        misfit: misfit_onto(&sp.id) || (c.outline_given && !c.notes.is_empty()),
                    });
                }
            }
        }
    }
    out
}

/// A flat plate's solid: its two faces' corners and its faces.
type Slab = ([Vec<[f64; 3]>; 2], Vec<SolidFace>);

/// The solid of a flat plate: `outline` run along `through` by
/// `thickness` (its two faces' corners and its faces). None: no outline.
fn slab(outline: &[[f64; 3]], through: Option<[f64; 3]>, thickness: f64) -> Option<Slab> {
    let n = through?;
    if outline.len() < 3 {
        return None;
    }
    let far: Vec<[f64; 3]> = outline
        .iter()
        .map(|q| add(*q, scale(n, thickness)))
        .collect();
    // The outline in its own plane, for the faces' turning and ears.
    let u0 = sub(outline[1], outline[0]);
    let u = scale(u0, 1.0 / norm(u0).max(1e-12));
    let v = cross(n, u);
    let flat: Vec<[f64; 2]> = outline
        .iter()
        .map(|q| {
            let d = sub(*q, outline[0]);
            [dot(d, u), dot(d, v)]
        })
        .collect();
    let faces = prism_faces_capped(outline, &far, &flat, u, v, [true, true])?;
    Some(([outline.to_vec(), far], faces))
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

/// A polygon's unit normal by its first corners (none: degenerate).
fn normal_of(poly: &[[f64; 3]]) -> Option<[f64; 3]> {
    // Newell's method: robust for any planar polygon.
    let mut n = [0.0; 3];
    for (k, a) in poly.iter().enumerate() {
        let b = poly[(k + 1) % poly.len()];
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let l = norm(n);
    (poly.len() >= 3 && l >= 1e-15).then(|| scale(n, 1.0 / l))
}

fn mean(p: &[[f64; 3]]) -> [f64; 3] {
    let n = p.len().max(1) as f64;
    let mut m = [0.0; 3];
    for q in p {
        for k in 0..3 {
            m[k] += q[k] / n;
        }
    }
    m
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: [f64; 3], k: f64) -> [f64; 3] {
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
