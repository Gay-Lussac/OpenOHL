// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Plane geometry shared by several crates.

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
