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

/// O44 L16 (L15's question 4): an end naming nothing it bolts onto is
/// planned on the face the joint details then seat it on. With a leg `V`
/// at the joint (the joint's leg worked out for the redundant `R`), the
/// layout plans R onto the diagonal running through: the face the plan
/// takes there ([`crate::joints::seat_face`]) is the one the joint details
/// seat R on once the file names the diagonal as what it bolts onto — at
/// every roll compared — and at some rolls not the one R sat on while it
/// named nothing (before L16 the plan's).
#[test]
fn an_end_naming_nothing_is_planned_on_the_face_it_is_drawn_on() {
    use ohl_structures::connections::Face;
    let leg = "\n\n[[node]]\nid = \"T\"\npoint = [\"2 m\", \"0 m\", \"4 m\"]\nsupport = \"fixed\"\n\n\
               [[member]]\nid = \"V\"\nnodes = [\"N\", \"T\"]\nsection = \"EA-61x7\"\n\
               design = { class = \"leg\" }\n";
    let named = "\n\n[[connection]]\nmember = \"R\"\njoint = \"N\"\nonto = \"B1\"\n";
    let at = |text: &str| {
        let d = joint_details(text, &library(), &Default::default()).unwrap();
        let j = d.joint("N").unwrap().clone();
        let r = j.arrivals.iter().find(|a| a.member == "R").unwrap().clone();
        let b1 = j
            .arrivals
            .iter()
            .find(|a| a.member == "B1")
            .unwrap()
            .clone();
        (r, b1)
    };
    let (mut compared, mut moved) = (0, 0);
    for roll in (0..72).map(|k| k * 5) {
        let extra = format!("\nroll = \"{roll} deg\"\n{leg}");
        let (r, b1) = at(&diagonal([2.0, 1.5, 2.0], &extra));
        assert_eq!(r.onto.value.as_deref(), Some("V"), "roll {roll}: {r:?}");
        let (r2, _) = at(&diagonal([2.0, 1.5, 2.0], &format!("{extra}{named}")));
        assert_eq!(r2.onto.value.as_deref(), Some("B1"));
        let plan = crate::joints::seat_face(&r, &b1);
        if r.leg_y != r2.leg_y || r2.sits.value.is_none() {
            continue;
        }
        compared += 1;
        assert_eq!(plan, r2.sits.value, "roll {roll}: {r:?} {r2:?}");
        if r.sits.value.unwrap_or(Face::Outside) != plan.unwrap_or(Face::Outside) {
            moved += 1;
        }
    }
    assert!(compared > 0 && moved > 0, "{compared} {moved}");
}
