// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The larger of with and without the joint options (stage 4.4 follow-up,
//! 2026-11-24) on invented results: each member from the solve where its
//! usage is larger, said; each joint's larger displacement; the reactions
//! component by component, a sign flip said not on the safe side.

use std::time::Duration;

use crate::joint_envelope::{WITHOUT, envelope};
use crate::{CaseResult, JointResult, MemberResult, ReactionResult};

fn member(id: &str, axial: f64, usage: Option<f64>) -> MemberResult {
    MemberResult {
        id: id.into(),
        ends: [0, 1],
        axial,
        usage,
        governs: if usage.is_some() {
            "tension".into()
        } else {
            String::new()
        },
        slack: false,
        bowed: false,
        eccentric: None,
        end_forces: [[0.0; 6]; 2],
    }
}

fn case(members: Vec<MemberResult>, d: f64, reaction: [f64; 3], notes: &[&str]) -> CaseResult {
    CaseResult {
        case: "c".into(),
        joints: vec![
            JointResult {
                id: "F".into(),
                point: [0.0; 3],
                displacement: [0.0; 3],
                support: true,
            },
            JointResult {
                id: "T".into(),
                point: [0.0, 0.0, 1.0],
                displacement: [d, 0.0, 0.0],
                support: false,
            },
        ],
        members,
        reactions: vec![ReactionResult {
            joint: "F".into(),
            force: reaction,
            moment: [0.0; 3],
        }],
        load_sum: [0.0; 3],
        reaction_sum: reaction,
        iterations: 1,
        residual: 0.0,
        nonlinear: false,
        took: Duration::ZERO,
        notes: notes.iter().map(|n| (*n).to_owned()).collect(),
        points: Vec::new(),
        plates: Vec::new(),
        climbing: Vec::new(),
    }
}

/// a: 0.4 with, 0.6 without → without (its force −6 kN with it, said); b:
/// 0.8 with, 0.5 without → with; c: equal → with; d: no usage in either →
/// in neither count. The top moved 3 mm with, 2 mm without → 3 mm. The
/// reaction (1, −2, 10) kN with, (−1.5, −1, 9) kN without → (−1.5, −2,
/// 10) kN, x of opposite sign and both over 1 % of the largest: said NOT
/// ON THE SAFE SIDE; the reaction sum stays the solve's with them.
#[test]
fn each_member_takes_the_solve_where_its_usage_is_larger() {
    let with = case(
        vec![
            member("a", 4e3, Some(0.4)),
            member("b", 8e3, Some(0.8)),
            member("c", 1e3, Some(0.1)),
            member("d", 0.0, None),
        ],
        3e-3,
        [1e3, -2e3, 10e3],
        &[],
    );
    let without = case(
        vec![
            member("a", -6e3, Some(0.6)),
            member("b", 5e3, Some(0.5)),
            member("c", 1e3, Some(0.1)),
            member("d", 0.0, None),
        ],
        2e-3,
        [-1.5e3, -1e3, 9e3],
        &["NOT ON THE SAFE SIDE: something"],
    );
    let (out, g) = envelope(with, &without);
    assert_eq!(g.members, (2, 1));
    let a = &out.members[0];
    assert_eq!(a.usage, Some(0.6));
    assert_eq!(a.axial, -6e3);
    assert_eq!(a.governs, format!("tension ({WITHOUT})"));
    assert_eq!(out.members[1].usage, Some(0.8));
    assert_eq!(out.members[1].governs, "tension");
    assert_eq!(out.members[2].governs, "tension");
    assert_eq!(out.joints[1].displacement[0], 3e-3);
    assert_eq!(g.joints_without, 0);
    assert_eq!(out.reactions[0].force, [-1.5e3, -2e3, 10e3]);
    assert_eq!(out.reaction_sum, [1e3, -2e3, 10e3]);
    assert_eq!((g.reactions_without, g.reactions_flipped), (1, 1));
    let notes = out.notes.join("\n");
    assert!(notes.contains("2 member(s) governed by the solve with them, 1 by the solve without"));
    assert!(
        notes.contains(
            "NOT ON THE SAFE SIDE: 1 support reaction(s) with a component of opposite sign"
        )
    );
    assert!(notes.contains(&format!("NOT ON THE SAFE SIDE: something ({WITHOUT})")));
}

/// The solve without the options moving a joint further: its displacement
/// taken; reactions of one sign: no flip said.
#[test]
fn a_joint_moving_further_without_takes_that_displacement() {
    let with = case(
        vec![member("a", 1e3, Some(0.1))],
        1e-3,
        [0.0, 0.0, 5e3],
        &[],
    );
    let without = case(
        vec![member("a", 1e3, Some(0.1))],
        4e-3,
        [0.0, 0.0, 6e3],
        &[],
    );
    let (out, g) = envelope(with, &without);
    assert_eq!(out.joints[1].displacement[0], 4e-3);
    assert_eq!(g.joints_without, 1);
    assert_eq!(out.reactions[0].force[2], 6e3);
    assert_eq!(g.reactions_flipped, 0);
    assert!(
        !out.notes
            .iter()
            .any(|n| n.starts_with("NOT ON THE SAFE SIDE"))
    );
}
