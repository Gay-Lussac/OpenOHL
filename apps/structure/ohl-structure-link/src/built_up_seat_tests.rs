// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! O44 L13: an end on a built-up member seated by the plane its axis lies
//! in, also when it names nothing it bolts onto — on an invented joint.

use crate::joints::joint_details;
use crate::joints_tests::library;

/// An invented built-up diagonal (two angles back to back) in two pieces
/// B1, B2 through joint N, inclined 45°, and a redundant `R` arriving at N
/// from `far`.
fn diagonal(far: [f64; 3], r_extra: &str) -> String {
    format!(
        r#"id = "B"
name = "b"
kind = "lattice"

[[node]]
id = "N0"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "N"
point = ["2 m", "0 m", "2 m"]

[[node]]
id = "N1"
point = ["4 m", "0 m", "4 m"]
support = "fixed"

[[node]]
id = "F"
point = ["{} m", "{} m", "{} m"]
support = "fixed"

[[member]]
id = "B1"
nodes = ["N0", "N"]
section = "2L-83x52x6"
design = {{ class = "bracing" }}

[[member]]
id = "B2"
nodes = ["N", "N1"]
section = "2L-83x52x6"
design = {{ class = "bracing" }}

[[member]]
id = "R"
nodes = ["N", "F"]
section = "EA-61x7"
design = {{ class = "redundant" }}{r_extra}
"#,
        far[0], far[1], far[2]
    )
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[test]
fn an_end_on_a_built_up_member_is_seated_by_the_plane_its_axis_lies_in() {
    // The host's axes first (the redundant anywhere).
    let probe = diagonal([2.0, 0.0, 0.0], "");
    let d = joint_details(&probe, &library(), &Default::default()).unwrap();
    let b1 = d
        .joint("N")
        .unwrap()
        .arrivals
        .iter()
        .find(|a| a.member == "B1")
        .unwrap();
    let q = b1.profile.as_ref().unwrap();
    assert!(q.angle.is_none() && !q.part_angles.is_empty(), "{q:?}");
    // The redundant in the plane of the host's flanges square to its z
    // (along its y and its axis), at several rolls, bolting onto nothing
    // named: the layout plans it onto the diagonal running through.
    let far = [
        2.0 + 1.5 * q.axes[1][0] - 1.0 * q.axes[0][0],
        1.5 * q.axes[1][1] - 1.0 * q.axes[0][1],
        2.0 + 1.5 * q.axes[1][2] - 1.0 * q.axes[0][2],
    ];
    let n = q.axes[2];
    for roll in [10, 35, 55, 80, 100, 125, 145, 170] {
        let text = diagonal(
            far.map(|v| (v * 1e4).round() / 1e4),
            &format!("\nroll = \"{roll} deg\"\n"),
        );
        let d = joint_details(&text, &library(), &Default::default()).unwrap();
        let r = d
            .joint("N")
            .unwrap()
            .arrivals
            .iter()
            .find(|a| a.member == "R")
            .unwrap();
        assert!(dot(r.direction, n).abs() < 2f64.to_radians().sin(), "{r:?}");
        let p = r.profile.as_ref().unwrap();
        // Its leg bolted: the one whose normal lies nearer that flange's.
        let (bolted, other) = if r.leg_y {
            (p.axes[2], p.axes[1])
        } else {
            (p.axes[1], p.axes[2])
        };
        assert!(
            dot(bolted, n).abs() >= dot(other, n).abs() - 1e-9,
            "roll {roll}: {r:?}"
        );
    }
}
