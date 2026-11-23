// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The planes of a joint ([`crate::joint_planes`]) on invented joints.

use crate::joint_planes::{
    PLANE_TOL_DEG, PlaneEnd, joint_planes, nearer_flange, off_flange, planes_about,
};
use crate::joints_tests::library;

/// A host along z with its y along x and z along y.
const AXES: [[f64; 3]; 3] = [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];

/// A unit direction at `to` degrees to the host (z) and ψ `psi` degrees
/// from its y (x) towards its z (y).
fn dir(to: f64, psi: f64) -> [f64; 3] {
    let (t, p) = (to.to_radians(), psi.to_radians());
    [t.sin() * p.cos(), t.sin() * p.sin(), t.cos()]
}

fn end(m: &str, to: f64, psi: f64, area: f64, force: Option<f64>) -> PlaneEnd {
    PlaneEnd {
        member: m.into(),
        direction: dir(to, psi),
        area,
        force,
    }
}

/// Two diagonals in one plane (1° apart), a horizontal in the other
/// flange's plane, a redundant 6° off and a piece in line: three planes,
/// the biggest member's first; the in-line piece apart; each plane's
/// flange, the turn laying it there and how far each end lies out.
#[test]
fn ends_cluster_into_planes_through_the_host_high_to_low() {
    let ends = vec![
        end("D1", 45.0, 3.0, 8e-4, None),
        end("D2", 45.0, 4.0, 8e-4, None),
        end("H1", 90.0, 90.0, 12e-4, None),
        end("R1", 60.0, 96.0, 3e-4, None),
        end("RUN", 2.0, 30.0, 20e-4, None),
    ];
    let (planes, along) = planes_about(AXES, &ends, PLANE_TOL_DEG);
    assert_eq!(along, vec![4]);
    assert_eq!(planes.len(), 3, "{planes:?}");
    // High to low: the horizontal (largest), the diagonals, the redundant.
    assert_eq!(planes[0].ends[0].member, "H1");
    assert_eq!(planes[0].flange, 1);
    assert!(planes[0].turn.abs() < 1e-9);
    assert!(planes[0].in_flange());
    let d = &planes[1];
    assert_eq!(d.ends.len(), 2);
    assert!((d.angle - 3.5).abs() < 1e-6, "{d:?}");
    assert_eq!(d.flange, 0);
    assert!((d.turn - 3.5).abs() < 1e-6);
    assert!((d.spread - 1.0).abs() < 1e-6);
    // Out of flange 0's plane: asin(sin 45° · sin 4°) for D2.
    let want = (45f64.to_radians().sin() * 4f64.to_radians().sin())
        .asin()
        .to_degrees();
    let d2 = d.ends.iter().find(|e| e.member == "D2").unwrap();
    assert!((d2.off - want).abs() < 1e-3, "{d2:?}");
    assert!(!d.in_flange());
    let r = &planes[2];
    assert_eq!(r.flange, 1);
    assert!((r.turn - 6.0).abs() < 1e-6);
}

/// Planes either side of ψ = 0° (179.5° and 0.5°) are one plane, its
/// angle at the flange; ends 2.5° apart are two planes.
#[test]
fn a_plane_across_the_half_turn_is_one_plane() {
    let ends = vec![
        end("A", 50.0, 179.5, 5e-4, None),
        end("B", 50.0, 0.5, 5e-4, None),
        end("C", 50.0, 60.0, 5e-4, None),
    ];
    let (planes, _) = planes_about(AXES, &ends, PLANE_TOL_DEG);
    assert_eq!(planes.len(), 2, "{planes:?}");
    let p = planes.iter().find(|p| p.ends.len() == 2).unwrap();
    assert!(
        p.angle.abs() < 1e-6 || (p.angle - 180.0).abs() < 1e-6,
        "{p:?}"
    );
    assert!(p.turn.abs() < 1e-6);
    let ends = vec![
        end("A", 50.0, 10.0, 5e-4, None),
        end("B", 50.0, 12.5, 5e-4, None),
    ];
    assert_eq!(planes_about(AXES, &ends, PLANE_TOL_DEG).0.len(), 2);
}

/// Of two planes with members of one size, the one with the larger force
/// first; without forces the one with more ends.
#[test]
fn planes_of_one_size_are_ordered_by_force_then_count() {
    let ends = vec![
        end("A", 45.0, 0.0, 5e-4, Some(10e3)),
        end("B", 45.0, 90.0, 5e-4, Some(-40e3)),
        end("C", 45.0, 90.5, 2e-4, Some(1e3)),
    ];
    let (planes, _) = planes_about(AXES, &ends, PLANE_TOL_DEG);
    assert_eq!(planes[0].ends[0].member, "B");
    assert_eq!(planes[0].force, Some(40e3));
    let ends = vec![
        end("A", 45.0, 0.0, 5e-4, None),
        end("B", 45.0, 90.0, 5e-4, None),
        end("C", 45.0, 90.5, 2e-4, None),
    ];
    let (planes, _) = planes_about(AXES, &ends, PLANE_TOL_DEG);
    assert_eq!(planes[0].ends.len(), 2);
}

/// The nearer flange and the angle out of it.
#[test]
fn the_nearer_flange_and_how_far_out() {
    assert_eq!(nearer_flange(10.0), (0, 10.0));
    assert_eq!(nearer_flange(170.0), (0, -10.0));
    assert_eq!(nearer_flange(100.0), (1, 10.0));
    assert_eq!(nearer_flange(80.0), (1, -10.0));
    // At 90° to the host the angle out is the turn itself.
    assert!((off_flange(5.0, 90.0, 0, 0.0) - 5.0).abs() < 1e-9);
    // Turned onto it: in the plane.
    assert!(off_flange(5.0, 30.0, 0, 5.0).abs() < 1e-9);
}

/// The K-brace (both diagonals in the leg's face) with its leg turned 5°
/// about its axis: one plane of two ends, the turn laying the leg's flange
/// in it −5° (as the leg-rotation check reads it), each end out by more
/// than the owner's 2°; as the faces want it, in the flange.
#[test]
fn the_k_brace_planes_say_the_legs_turn() {
    let lib = library();
    let bolts = Default::default();
    let k = |deg: f64| {
        crate::joint_layout_tests::K.replace(
            "orientation = \"x+y+\"\ndesign = { class = \"leg\" }",
            &format!(
                "orientation = \"x+y+\"\nroll = \"{deg} deg\"\ndesign = {{ class = \"leg\" }}"
            ),
        )
    };
    let d = crate::joint_details(&k(5.0), &lib, &bolts).unwrap();
    let h = joint_planes(d.joint("N2").unwrap()).unwrap();
    assert!(h.host == "L1" || h.host == "L2", "{h:?}");
    assert!(h.run.is_some());
    assert_eq!(h.planes.len(), 1, "{h:?}");
    let p = &h.planes[0];
    assert_eq!(p.ends.len(), 2);
    assert!((p.turn + 5.0).abs() < 0.1, "{p:?}");
    assert!(!p.in_flange());
    assert_eq!(h.ends_within(), (2, 0));
    let d = crate::joint_details(&k(0.0), &lib, &bolts).unwrap();
    let h = joint_planes(d.joint("N2").unwrap()).unwrap();
    assert_eq!(h.ends_within(), (2, 2));
}
