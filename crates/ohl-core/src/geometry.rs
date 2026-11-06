// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Geometry shared by several crates.

/// The cross product `a` × `b`.
#[inline]
pub fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Distance from `p` to the segment `a`–`b` in the plane.
pub fn distance_to_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let len2 = d[0] * d[0] + d[1] * d[1];
    let t = if len2 > 0.0 {
        (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a[0] + t * d[0] - p[0]).hypot(a[1] + t * d[1] - p[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_distance() {
        let (a, b) = ([0.0, 0.0], [10.0, 0.0]);
        assert_eq!(distance_to_segment([5.0, 3.0], a, b), 3.0);
        assert_eq!(distance_to_segment([13.0, 4.0], a, b), 5.0);
        assert_eq!(distance_to_segment([-3.0, 4.0], a, b), 5.0);
        assert_eq!(distance_to_segment([3.0, 4.0], a, a), 5.0);
    }
}
