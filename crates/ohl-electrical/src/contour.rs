// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Isolines on a grid of values (marching squares) and isosurfaces in a
//! lattice of values (marching tetrahedra), for the fields' cuts and
//! volumes ([`crate::field_views`]). Only the drawing is interpolated: the
//! values at the grid's or lattice's points are the computed ones.
//!
//! Between two points the crossing of a level is placed by the logarithm
//! of the values (a field falls roughly as a power of the distance, so
//! its logarithm is closer to straight between the points than the value
//! itself), linearly when a value is 0 or negative. A value that is not a
//! number marks a point not computed: no line or surface goes through a
//! cell touching it.

use std::collections::HashMap;

/// Where between values `a` and `b` (0 at `a`, 1 at `b`) the level lies.
fn crossing(a: f64, b: f64, level: f64) -> f64 {
    let t = if a > 0.0 && b > 0.0 && level > 0.0 {
        let (la, lb) = (a.ln(), b.ln());
        if (lb - la).abs() < 1e-300 {
            0.5
        } else {
            (level.ln() - la) / (lb - la)
        }
    } else if (b - a).abs() < 1e-300 {
        0.5
    } else {
        (level - a) / (b - a)
    };
    t.clamp(0.0, 1.0)
}

/// The isolines of `level` on a grid of `columns` × `rows` values
/// (row-major: `values[row * columns + column]`), as polylines in grid
/// coordinates (column, row; fractional between points), joined where they
/// meet. A closed line ends on its first point.
pub fn isolines(values: &[f64], columns: usize, rows: usize, level: f64) -> Vec<Vec<[f64; 2]>> {
    assert_eq!(values.len(), columns * rows, "a value per grid point");
    if columns < 2 || rows < 2 {
        return Vec::new();
    }
    let v = |c: usize, r: usize| values[r * columns + c];
    // An edge by its first point and its direction (0 along a row, 1 up a
    // column).
    let edge = |c: usize, r: usize, up: usize| 2 * (r * columns + c) + up;
    let mut at: HashMap<usize, [f64; 2]> = HashMap::new();
    let mut point = |e: usize, c: usize, r: usize, up: usize| -> usize {
        at.entry(e).or_insert_with(|| {
            let (a, b) = if up == 0 {
                (v(c, r), v(c + 1, r))
            } else {
                (v(c, r), v(c, r + 1))
            };
            let t = crossing(a, b, level);
            if up == 0 {
                [c as f64 + t, r as f64]
            } else {
                [c as f64, r as f64 + t]
            }
        });
        e
    };
    let mut segments: Vec<[usize; 2]> = Vec::new();
    for r in 0..rows - 1 {
        for c in 0..columns - 1 {
            let q = [v(c, r), v(c + 1, r), v(c + 1, r + 1), v(c, r + 1)];
            if q.iter().any(|x| !x.is_finite()) {
                continue;
            }
            let inside = q.map(|x| x >= level);
            let n = inside.iter().filter(|&&b| b).count();
            if n == 0 || n == 4 {
                continue;
            }
            // The cell's edges: bottom, right, top, left.
            let edges = [
                (edge(c, r, 0), c, r, 0),
                (edge(c + 1, r, 1), c + 1, r, 1),
                (edge(c, r + 1, 0), c, r + 1, 0),
                (edge(c, r, 1), c, r, 1),
            ];
            let crossed: Vec<usize> = (0..4)
                .filter(|&k| inside[k] != inside[(k + 1) % 4])
                .collect();
            let mut e = |k: usize| {
                let (id, c, r, up) = edges[k];
                point(id, c, r, up)
            };
            if crossed.len() == 2 {
                segments.push([e(crossed[0]), e(crossed[1])]);
            } else {
                // A saddle: the centre's value (the mean) decides which
                // corners join.
                let centre = q.iter().sum::<f64>() / 4.0;
                if (centre >= level) == inside[0] {
                    segments.push([e(0), e(1)]);
                    segments.push([e(2), e(3)]);
                } else {
                    segments.push([e(3), e(0)]);
                    segments.push([e(1), e(2)]);
                }
            }
        }
    }
    join(&segments, &at)
}

/// Segments between keyed points joined into polylines.
fn join(segments: &[[usize; 2]], at: &HashMap<usize, [f64; 2]>) -> Vec<Vec<[f64; 2]>> {
    let mut ends: HashMap<usize, Vec<usize>> = HashMap::new();
    for (k, s) in segments.iter().enumerate() {
        ends.entry(s[0]).or_default().push(k);
        ends.entry(s[1]).or_default().push(k);
    }
    let mut used = vec![false; segments.len()];
    let mut out = Vec::new();
    // Open lines first (from an end met once), then the closed ones.
    let mut starts: Vec<usize> = ends
        .iter()
        .filter(|(_, v)| v.len() == 1)
        .map(|(&p, _)| p)
        .collect();
    starts.sort_unstable();
    let walk = |start: usize, used: &mut Vec<bool>| -> Option<Vec<usize>> {
        let mut line = vec![start];
        let mut here = start;
        loop {
            let next = ends
                .get(&here)
                .and_then(|v| v.iter().copied().find(|&k| !used[k]));
            let Some(k) = next else { break };
            used[k] = true;
            here = if segments[k][0] == here {
                segments[k][1]
            } else {
                segments[k][0]
            };
            line.push(here);
        }
        (line.len() > 1).then_some(line)
    };
    for s in starts {
        if let Some(l) = walk(s, &mut used) {
            out.push(l.iter().map(|p| at[p]).collect());
        }
    }
    for k in 0..segments.len() {
        if !used[k]
            && let Some(l) = walk(segments[k][0], &mut used)
        {
            out.push(l.iter().map(|p| at[p]).collect());
        }
    }
    out
}

/// A triangle mesh: vertices and corner indices.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Mesh {
    /// Vertices.
    pub vertices: Vec<[f64; 3]>,
    /// Triangles, by vertex index.
    pub triangles: Vec<[u32; 3]>,
}

/// The six tetrahedra of a cube (corners numbered by bits: x 1, y 2, z 4),
/// all around its diagonal 0–7, so neighbouring cubes split their shared
/// faces alike and the surface has no cracks.
const TETRAHEDRA: [[usize; 4]; 6] = [
    [0, 1, 3, 7],
    [0, 3, 2, 7],
    [0, 2, 6, 7],
    [0, 6, 4, 7],
    [0, 4, 5, 7],
    [0, 5, 1, 7],
];

/// The isosurface of `level` in a lattice of `n[0]` × `n[1]` × `n[2]`
/// points (`value(i, j, k)`; not a number: not computed), in lattice
/// coordinates (fractional between points). Cubes are cut into six
/// tetrahedra; a vertex is shared by the triangles meeting at it. The
/// triangles face the lower values.
pub fn isosurface(n: [usize; 3], value: &dyn Fn(usize, usize, usize) -> f64, level: f64) -> Mesh {
    let mut mesh = Mesh::default();
    if n.iter().any(|&m| m < 2) {
        return mesh;
    }
    let id = |i: usize, j: usize, k: usize| (k * n[1] + j) * n[0] + i;
    let mut shared: HashMap<(usize, usize), u32> = HashMap::new();
    for k in 0..n[2] - 1 {
        for j in 0..n[1] - 1 {
            for i in 0..n[0] - 1 {
                let corner = |c: usize| (i + (c & 1), j + ((c >> 1) & 1), k + ((c >> 2) & 1));
                let vals: [f64; 8] = std::array::from_fn(|c| {
                    let (a, b, d) = corner(c);
                    value(a, b, d)
                });
                if vals.iter().any(|x| !x.is_finite()) {
                    continue;
                }
                let above = vals.iter().filter(|&&x| x >= level).count();
                if above == 0 || above == 8 {
                    continue;
                }
                for tet in TETRAHEDRA {
                    let inside: Vec<usize> =
                        tet.iter().copied().filter(|&c| vals[c] >= level).collect();
                    let outside: Vec<usize> =
                        tet.iter().copied().filter(|&c| vals[c] < level).collect();
                    if inside.is_empty() || outside.is_empty() {
                        continue;
                    }
                    let mut vertex = |a: usize, b: usize| -> u32 {
                        let (pa, pb) = (corner(a), corner(b));
                        let (ia, ib) = (id(pa.0, pa.1, pa.2), id(pb.0, pb.1, pb.2));
                        // Placed from the low-index end, so both cubes
                        // sharing the edge place it alike.
                        let (lo, hi, vl, vh) = if ia < ib {
                            (pa, pb, vals[a], vals[b])
                        } else {
                            (pb, pa, vals[b], vals[a])
                        };
                        let t = crossing(vl, vh, level);
                        // A crossing at a point itself (the level met
                        // there exactly) is that point's vertex, shared by
                        // every edge through it: no crack.
                        let key = if t <= 0.0 {
                            (ia.min(ib), ia.min(ib))
                        } else if t >= 1.0 {
                            (ia.max(ib), ia.max(ib))
                        } else {
                            (ia.min(ib), ia.max(ib))
                        };
                        *shared.entry(key).or_insert_with(|| {
                            mesh.vertices.push([
                                lo.0 as f64 + t * (hi.0 as f64 - lo.0 as f64),
                                lo.1 as f64 + t * (hi.1 as f64 - lo.1 as f64),
                                lo.2 as f64 + t * (hi.2 as f64 - lo.2 as f64),
                            ]);
                            (mesh.vertices.len() - 1) as u32
                        })
                    };
                    let mut tris: Vec<[u32; 3]> = Vec::new();
                    match (inside.len(), outside.len()) {
                        (1, 3) => {
                            let a = inside[0];
                            tris.push([
                                vertex(a, outside[0]),
                                vertex(a, outside[1]),
                                vertex(a, outside[2]),
                            ]);
                        }
                        (3, 1) => {
                            let b = outside[0];
                            tris.push([
                                vertex(inside[0], b),
                                vertex(inside[1], b),
                                vertex(inside[2], b),
                            ]);
                        }
                        _ => {
                            let (a, b) = (inside[0], inside[1]);
                            let (c, d) = (outside[0], outside[1]);
                            let (ac, ad, bc, bd) =
                                (vertex(a, c), vertex(a, d), vertex(b, c), vertex(b, d));
                            tris.push([ac, ad, bd]);
                            tris.push([ac, bd, bc]);
                        }
                    }
                    // Facing the lower values: away from the inside
                    // corners' centre.
                    let centre = |cs: &[usize]| {
                        let mut s = [0.0; 3];
                        for &c in cs {
                            let p = corner(c);
                            s[0] += p.0 as f64;
                            s[1] += p.1 as f64;
                            s[2] += p.2 as f64;
                        }
                        s.map(|x| x / cs.len() as f64)
                    };
                    let (ci, co) = (centre(&inside), centre(&outside));
                    let out_dir = [co[0] - ci[0], co[1] - ci[1], co[2] - ci[2]];
                    for t in tris {
                        // Collapsed to a point or a line by a level met at
                        // a lattice point (its vertices shared): nothing.
                        if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
                            continue;
                        }
                        let [p, q, r] = t.map(|v| mesh.vertices[v as usize]);
                        let u = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
                        let w = [r[0] - p[0], r[1] - p[1], r[2] - p[2]];
                        let nrm = [
                            u[1] * w[2] - u[2] * w[1],
                            u[2] * w[0] - u[0] * w[2],
                            u[0] * w[1] - u[1] * w[0],
                        ];
                        let s = nrm[0] * out_dir[0] + nrm[1] * out_dir[1] + nrm[2] * out_dir[2];
                        mesh.triangles
                            .push(if s >= 0.0 { t } else { [t[0], t[2], t[1]] });
                    }
                }
            }
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A circle's isoline: closed, every point at the radius (the values
    /// fall as 1/r, so the logarithmic placement is exact on the axes).
    #[test]
    fn a_point_source_gives_a_closed_circle() {
        let (n, c) = (41, 20.0);
        let mut v = vec![0.0; n * n];
        for r in 0..n {
            for k in 0..n {
                let d = ((k as f64 - c).powi(2) + (r as f64 - c).powi(2)).sqrt();
                v[r * n + k] = 1.0 / d.max(1e-3);
            }
        }
        let lines = isolines(&v, n, n, 1.0 / 10.0);
        assert_eq!(lines.len(), 1);
        let l = &lines[0];
        assert_eq!(l.first(), l.last(), "closed");
        for p in l {
            let d = ((p[0] - c).powi(2) + (p[1] - c).powi(2)).sqrt();
            assert!((d - 10.0).abs() < 0.05, "{d}");
        }
    }

    /// Points not computed break the line; a level outside the values
    /// gives none.
    #[test]
    fn unknown_points_and_levels_out_of_range() {
        let n = 5;
        let mut v: Vec<f64> = (0..n * n).map(|k| (k % n) as f64 + 1.0).collect();
        assert_eq!(isolines(&v, n, n, 2.5).len(), 1);
        assert!(isolines(&v, n, n, 100.0).is_empty());
        v[2 * n + 1] = f64::NAN;
        let lines = isolines(&v, n, n, 2.5);
        assert_eq!(lines.len(), 2, "{lines:?}");
    }

    /// A sphere's isosurface: closed (every edge in two triangles), its
    /// vertices at the radius, its triangles facing outward.
    #[test]
    fn a_point_source_gives_a_closed_sphere() {
        let n = 21;
        let c = 10.0;
        let f = |i: usize, j: usize, k: usize| {
            let d =
                ((i as f64 - c).powi(2) + (j as f64 - c).powi(2) + (k as f64 - c).powi(2)).sqrt();
            1.0 / d.max(1e-3)
        };
        let m = isosurface([n, n, n], &f, 1.0 / 6.3);
        assert!(m.triangles.len() > 200);
        for p in &m.vertices {
            let d = ((p[0] - c).powi(2) + (p[1] - c).powi(2) + (p[2] - c).powi(2)).sqrt();
            assert!((d - 6.3).abs() < 0.3, "{d}");
        }
        let mut edges: HashMap<(u32, u32), usize> = HashMap::new();
        for t in &m.triangles {
            for e in 0..3 {
                let (a, b) = (t[e], t[(e + 1) % 3]);
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
            let [p, q, r] = t.map(|v| m.vertices[v as usize]);
            let u = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
            let w = [r[0] - p[0], r[1] - p[1], r[2] - p[2]];
            let nrm = [
                u[1] * w[2] - u[2] * w[1],
                u[2] * w[0] - u[0] * w[2],
                u[0] * w[1] - u[1] * w[0],
            ];
            let out = [p[0] - c, p[1] - c, p[2] - c];
            assert!(nrm[0] * out[0] + nrm[1] * out[1] + nrm[2] * out[2] > 0.0);
        }
        assert!(edges.values().all(|&k| k == 2), "closed");
        // A radius met exactly at lattice points (6: e.g. (16, 10, 10)):
        // still closed, the vertices there shared.
        let m = isosurface([n, n, n], &f, 1.0 / 6.0);
        let mut edges: HashMap<(u32, u32), usize> = HashMap::new();
        for t in &m.triangles {
            for e in 0..3 {
                let (a, b) = (t[e], t[(e + 1) % 3]);
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        assert!(edges.values().all(|&k| k == 2), "closed at lattice points");
    }
}
