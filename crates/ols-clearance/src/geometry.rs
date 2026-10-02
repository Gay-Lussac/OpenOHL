// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Closest points between points, segments and triangles in 3D.

/// A point, m.
pub type P3 = [f64; 3];

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: P3, k: f64) -> P3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Distance between two points.
pub fn distance(a: P3, b: P3) -> f64 {
    let d = sub(a, b);
    dot(d, d).sqrt()
}

/// Closest point to `p` on the segment a–b, and its parameter t ∈ [0, 1].
pub fn closest_on_segment(p: P3, a: P3, b: P3) -> (P3, f64) {
    let d = sub(b, a);
    let len2 = dot(d, d);
    let t = if len2 > 0.0 {
        (dot(sub(p, a), d) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (add(a, scale(d, t)), t)
}

/// Closest points between segments p1–q1 and p2–q2 (Ericson, Real-Time
/// Collision Detection, 5.1.9): the point on each, in that order.
pub fn closest_between_segments(p1: P3, q1: P3, p2: P3, q2: P3) -> (P3, P3) {
    let (d1, d2, r) = (sub(q1, p1), sub(q2, p2), sub(p1, p2));
    let (a, e, f) = (dot(d1, d1), dot(d2, d2), dot(d2, r));
    let eps = 1e-24;
    let (s, t);
    if a <= eps && e <= eps {
        return (p1, p2);
    }
    if a <= eps {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = dot(d1, r);
        if e <= eps {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = dot(d1, d2);
            let denom = a * e - b * b;
            let mut s0 = if denom > eps * a * e {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mut t0 = (b * s0 + f) / e;
            if t0 < 0.0 {
                t0 = 0.0;
                s0 = (-c / a).clamp(0.0, 1.0);
            } else if t0 > 1.0 {
                t0 = 1.0;
                s0 = ((b - c) / a).clamp(0.0, 1.0);
            }
            s = s0;
            t = t0;
        }
    }
    (add(p1, scale(d1, s)), add(p2, scale(d2, t)))
}

/// Closest point to `p` on the triangle a, b, c (Ericson 5.1.5).
pub fn closest_on_triangle(p: P3, a: P3, b: P3, c: P3) -> P3 {
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return add(a, scale(ab, d1 / (d1 - d3)));
    }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return add(a, scale(ac, d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return add(b, scale(sub(c, b), w));
    }
    let denom = 1.0 / (va + vb + vc);
    let (v, w) = (vb * denom, vc * denom);
    add(a, add(scale(ab, v), scale(ac, w)))
}

/// Where segment a–b passes over or under the point (x, y) within a
/// horizontal distance `band`: the closest point of the segment in plan,
/// its height there and its horizontal distance. `None` if farther.
pub fn over_in_plan(x: f64, y: f64, a: P3, b: P3, band: f64) -> Option<(P3, f64)> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((x - a[0]) * dx + (y - a[1]) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let q = [a[0] + dx * t, a[1] + dy * t, a[2] + (b[2] - a[2]) * t];
    let h = (q[0] - x).hypot(q[1] - y);
    (h <= band).then_some((q, h))
}

/// Where two segments cross in plan (x, y): the point on each, if they do.
pub fn crossing_in_plan(p1: P3, q1: P3, p2: P3, q2: P3) -> Option<(P3, P3)> {
    let (d1, d2) = (
        [q1[0] - p1[0], q1[1] - p1[1]],
        [q2[0] - p2[0], q2[1] - p2[1]],
    );
    let den = d1[0] * d2[1] - d1[1] * d2[0];
    if den.abs() < 1e-12 * (d1[0].hypot(d1[1]) * d2[0].hypot(d2[1])).max(1e-300) {
        return None;
    }
    let r = [p2[0] - p1[0], p2[1] - p1[1]];
    let s = (r[0] * d2[1] - r[1] * d2[0]) / den;
    let t = (r[0] * d1[1] - r[1] * d1[0]) / den;
    ((0.0..=1.0).contains(&s) && (0.0..=1.0).contains(&t)).then(|| {
        (
            add(p1, scale(sub(q1, p1), s)),
            add(p2, scale(sub(q2, p2), t)),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_skew_parallel_and_degenerate() {
        // Skew: x-axis segment and a y-direction segment 3 m above x = 2.
        let (a, b) = closest_between_segments(
            [0.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
            [2.0, -1.0, 3.0],
            [2.0, 1.0, 3.0],
        );
        assert_eq!(a, [2.0, 0.0, 0.0]);
        assert_eq!(b, [2.0, 0.0, 3.0]);
        // Parallel, offset 1 m, overlapping.
        let (a, b) = closest_between_segments(
            [0.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [2.0, 1.0, 0.0],
            [6.0, 1.0, 0.0],
        );
        assert!((distance(a, b) - 1.0).abs() < 1e-12);
        // Beyond the ends: end to end.
        let (a, b) = closest_between_segments(
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
            [4.0, 1.0, 0.0],
        );
        assert_eq!((a, b), ([1.0, 0.0, 0.0], [3.0, 0.0, 0.0]));
        // A point segment.
        let (a, _) = closest_between_segments([1.0; 3], [1.0; 3], [0.0; 3], [2.0, 0.0, 0.0]);
        assert_eq!(a, [1.0; 3]);
    }

    #[test]
    fn triangle_regions() {
        let (a, b, c) = ([0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [0.0, 4.0, 0.0]);
        assert_eq!(
            closest_on_triangle([1.0, 1.0, 5.0], a, b, c),
            [1.0, 1.0, 0.0]
        );
        assert_eq!(closest_on_triangle([-1.0, -1.0, 0.0], a, b, c), a);
        assert_eq!(
            closest_on_triangle([2.0, -3.0, 0.0], a, b, c),
            [2.0, 0.0, 0.0]
        );
        let q = closest_on_triangle([3.0, 3.0, 0.0], a, b, c);
        assert!((q[0] - 2.0).abs() < 1e-12 && (q[1] - 2.0).abs() < 1e-12);
    }

    #[test]
    fn plan_queries() {
        let (q, h) = over_in_plan(2.0, 0.5, [0.0, 0.0, 10.0], [4.0, 0.0, 14.0], 1.0).unwrap();
        assert_eq!((q, h), ([2.0, 0.0, 12.0], 0.5));
        assert!(over_in_plan(2.0, 2.0, [0.0, 0.0, 10.0], [4.0, 0.0, 14.0], 1.0).is_none());
        let (a, b) = crossing_in_plan(
            [0.0, 0.0, 10.0],
            [4.0, 0.0, 10.0],
            [1.0, -1.0, 2.0],
            [1.0, 1.0, 4.0],
        )
        .unwrap();
        assert_eq!((a, b), ([1.0, 0.0, 10.0], [1.0, 0.0, 3.0]));
    }
}
