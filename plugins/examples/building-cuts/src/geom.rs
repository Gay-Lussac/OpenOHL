// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Plan geometry: hulls, rings, distances, simplification.

/// A plan point, m.
pub type P2 = [f64; 2];

fn sub(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}

fn cross(o: P2, a: P2, b: P2) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// The convex hull, counter-clockwise, without collinear corners
/// (Andrew's monotone chain; independent of the points' order).
pub fn convex_hull(points: &[P2]) -> Vec<P2> {
    let mut p: Vec<P2> = points.to_vec();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup();
    if p.len() < 3 {
        return p;
    }
    let mut lower: Vec<P2> = Vec::new();
    for &q in &p {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], q) <= 0.0 {
            lower.pop();
        }
        lower.push(q);
    }
    let mut upper: Vec<P2> = Vec::new();
    for &q in p.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], q) <= 0.0 {
            upper.pop();
        }
        upper.push(q);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// The ring's signed area (counter-clockwise positive), m².
pub fn signed_area(ring: &[P2]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

/// The ring counter-clockwise.
pub fn ccw(mut ring: Vec<P2>) -> Vec<P2> {
    if signed_area(&ring) < 0.0 {
        ring.reverse();
    }
    ring
}

/// The ring's centroid (its corners' mean when it has no area).
pub fn centroid(ring: &[P2]) -> P2 {
    let a = signed_area(ring);
    let n = ring.len();
    if a.abs() < 1e-9 || n < 3 {
        let k = n.max(1) as f64;
        return [
            ring.iter().map(|p| p[0]).sum::<f64>() / k,
            ring.iter().map(|p| p[1]).sum::<f64>() / k,
        ];
    }
    // About the first corner: fewer digits lost far from the origin.
    let o = ring[0];
    let (mut cx, mut cy) = (0.0, 0.0);
    for i in 0..n {
        let (p, q) = (sub(ring[i], o), sub(ring[(i + 1) % n], o));
        let c = p[0] * q[1] - q[0] * p[1];
        cx += (p[0] + q[0]) * c;
        cy += (p[1] + q[1]) * c;
    }
    [o[0] + cx / (6.0 * a), o[1] + cy / (6.0 * a)]
}

/// Whether `p` is inside the ring (even–odd).
pub fn inside(p: P2, ring: &[P2]) -> bool {
    let n = ring.len();
    let mut c = false;
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + n - 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            c = !c;
        }
    }
    c
}

/// The point of segment `a`–`b` nearest `p`, and its distance.
pub fn nearest_on_segment(p: P2, a: P2, b: P2) -> (P2, f64) {
    let d = sub(b, a);
    let l2 = d[0] * d[0] + d[1] * d[1];
    let t = if l2 > 0.0 {
        (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let q = [a[0] + t * d[0], a[1] + t * d[1]];
    (q, (p[0] - q[0]).hypot(p[1] - q[1]))
}

/// The distance from `p` to the ring's area (0 inside).
pub fn distance_to_area(p: P2, ring: &[P2]) -> f64 {
    if inside(p, ring) {
        return 0.0;
    }
    distance_to_boundary(p, ring)
}

/// The distance from `p` to the ring's boundary.
pub fn distance_to_boundary(p: P2, ring: &[P2]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|i| nearest_on_segment(p, ring[i], ring[(i + 1) % n]).1)
        .fold(f64::INFINITY, f64::min)
}

fn segments_cross(a: P2, b: P2, c: P2, d: P2) -> bool {
    let (d1, d2) = (cross(c, d, a), cross(c, d, b));
    let (d3, d4) = (cross(a, b, c), cross(a, b, d));
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
}

/// The distance from segment `a`–`b` to the ring's area (0 when they
/// meet).
pub fn segment_to_area(a: P2, b: P2, ring: &[P2]) -> f64 {
    if inside(a, ring) || inside(b, ring) {
        return 0.0;
    }
    let n = ring.len();
    let mut best = f64::INFINITY;
    for i in 0..n {
        let (c, d) = (ring[i], ring[(i + 1) % n]);
        if segments_cross(a, b, c, d) {
            return 0.0;
        }
        best = best
            .min(nearest_on_segment(c, a, b).1)
            .min(nearest_on_segment(a, c, d).1)
            .min(nearest_on_segment(b, c, d).1);
    }
    best
}

/// Where the line `origin + s·dir` (`dir` a unit vector) crosses the
/// ring: the least and the largest `s`, if it does.
pub fn line_interval(origin: P2, dir: P2, ring: &[P2]) -> Option<(f64, f64)> {
    let normal = [-dir[1], dir[0]];
    let side = |p: P2| (p[0] - origin[0]) * normal[0] + (p[1] - origin[1]) * normal[1];
    let along = |p: P2| (p[0] - origin[0]) * dir[0] + (p[1] - origin[1]) * dir[1];
    let n = ring.len();
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        let (sa, sb) = (side(a), side(b));
        let mut hit = |s: f64| {
            lo = lo.min(s);
            hi = hi.max(s);
        };
        if sa == 0.0 {
            hit(along(a));
        }
        if (sa > 0.0 && sb < 0.0) || (sa < 0.0 && sb > 0.0) {
            let t = sa / (sa - sb);
            hit(along([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]));
        }
    }
    (lo <= hi).then_some((lo, hi))
}

/// The ring without corners within `tol` of the line through their
/// neighbours' simplified chain (Douglas–Peucker on a closed ring,
/// anchored at its first corner and the corner farthest from it). Three
/// corners at least are kept.
pub fn simplify_ring(ring: &[P2], tol: f64) -> Vec<P2> {
    let n = ring.len();
    if n <= 3 || tol <= 0.0 {
        return ring.to_vec();
    }
    let far = (1..n)
        .max_by(|&i, &j| {
            let di = (ring[i][0] - ring[0][0]).hypot(ring[i][1] - ring[0][1]);
            let dj = (ring[j][0] - ring[0][0]).hypot(ring[j][1] - ring[0][1]);
            di.total_cmp(&dj).then(j.cmp(&i))
        })
        .unwrap_or(1);
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[far] = true;
    let chain_a: Vec<usize> = (0..=far).collect();
    let chain_b: Vec<usize> = (far..n).chain(std::iter::once(0)).collect();
    for chain in [chain_a, chain_b] {
        dp(ring, &chain, tol, &mut keep);
    }
    let out: Vec<P2> = (0..n).filter(|&i| keep[i]).map(|i| ring[i]).collect();
    if out.len() < 3 {
        // Keep the corner farthest from the chord as well.
        let mut idx: Vec<usize> = (0..n).filter(|&i| keep[i]).collect();
        let extra = (0..n).filter(|i| !keep[*i]).max_by(|&i, &j| {
            let di = nearest_on_segment(ring[i], ring[0], ring[far]).1;
            let dj = nearest_on_segment(ring[j], ring[0], ring[far]).1;
            di.total_cmp(&dj)
        });
        idx.extend(extra);
        idx.sort_unstable();
        return idx.into_iter().map(|i| ring[i]).collect();
    }
    out
}

fn dp(ring: &[P2], chain: &[usize], tol: f64, keep: &mut [bool]) {
    if chain.len() < 3 {
        return;
    }
    let (a, b) = (ring[chain[0]], ring[chain[chain.len() - 1]]);
    let mut best = (0usize, -1.0);
    for (k, &i) in chain.iter().enumerate().take(chain.len() - 1).skip(1) {
        let d = nearest_on_segment(ring[i], a, b).1;
        if d > best.1 {
            best = (k, d);
        }
    }
    if best.1 > tol {
        keep[chain[best.0]] = true;
        dp(ring, &chain[..=best.0], tol, keep);
        dp(ring, &chain[best.0..], tol, keep);
    }
}

/// The value at percentile `q` (0–100) of `values` (sorted here; nearest
/// rank); none when empty.
pub fn percentile(values: &mut [f64], q: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let k = ((values.len() - 1) as f64 * q / 100.0).round() as usize;
    Some(values[k.min(values.len() - 1)])
}
