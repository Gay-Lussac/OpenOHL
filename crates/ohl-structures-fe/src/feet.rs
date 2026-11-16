// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Measured foot movements of a structure on its model (owner
//! 2026-11-16): each foot's movement put on its support joint as a
//! prescribed displacement ([`crate::Prescribed`]) — the one rule of the
//! line's structure checks, its reference state and OHL-Structure.
//!
//! - A foot is a support joint of the model (its id), or [`ALL`]: every
//!   support joint moved with the structure as one body (t + θ × r about
//!   the structure's origin).
//! - Each movement is put on the directions the support holds (fixed, or
//!   sprung: the spring's ground end); a rotation only where a member that
//!   is always a frame meets the joint. A movement in a direction the
//!   support does not hold is left out (the structure follows freely
//!   there), said.
//! - The checks keep, per member, the larger usage of with and without
//!   the movements ([`larger_usage`]): a settlement's forces added where
//!   they load a member, never taken to relieve one.

use crate::check::{MemberUsage, usage_order};
use crate::model::Model;
use crate::solve::{DOF_NAMES, Prescribed};

/// Every foot alike: the structure moved as one body.
pub const ALL: &str = "all";

/// A pole's base.
pub const BASE: &str = "base";

/// One foot's movement: its joint and its six displacements
/// (translations m, rotations rad; structure axes).
#[derive(Debug, Clone, PartialEq)]
pub struct FootMove {
    /// The support joint's id, [`ALL`] or [`BASE`].
    pub joint: String,
    /// Translations and rotations.
    pub displacement: [f64; 6],
}

/// The movements put on a model's supports ([`on_supports`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OnSupports {
    /// The prescribed displacements.
    pub prescribed: Vec<Prescribed>,
    /// What was left out, in words.
    pub notes: Vec<String>,
}

/// The move of a point at `r` (structure axes, m) when the structure
/// moves as one body by `d` (translations and rotations about its
/// origin): t + θ × r.
pub fn rigid_at(d: [f64; 6], r: [f64; 3]) -> [f64; 3] {
    let w = [d[3], d[4], d[5]];
    [
        d[0] + w[1] * r[2] - w[2] * r[1],
        d[1] + w[2] * r[0] - w[0] * r[2],
        d[2] + w[0] * r[1] - w[1] * r[0],
    ]
}

/// `feet` on `model`'s supports (module docs). Err: a foot that is not a
/// support joint of the model.
pub fn on_supports(model: &Model, feet: &[FootMove]) -> Result<OnSupports, String> {
    let mut out = OnSupports::default();
    let mut free: Vec<String> = Vec::new();
    let mut take = |n: usize, d: [f64; 6], one_body: bool, out: &mut OnSupports| {
        let node = &model.nodes[n];
        let mut held = [0.0; 6];
        for (k, v) in d.iter().enumerate() {
            if *v == 0.0 {
                continue;
            }
            let holds = node.fixed[k] || node.spring[k] != 0.0;
            if holds && (k < 3 || model.frame_at(n)) {
                held[k] = *v;
            } else if !one_body || k >= 3 {
                free.push(format!("{} {}", node.id, DOF_NAMES[k]));
            }
        }
        if held != [0.0; 6] {
            out.prescribed.push(Prescribed {
                node: n,
                displacement: held,
            });
        }
    };
    for f in feet.iter().filter(|f| f.displacement != [0.0; 6]) {
        if f.joint == ALL {
            for (n, node) in model.nodes.iter().enumerate() {
                if node.is_support() {
                    let t = rigid_at(f.displacement, node.point);
                    let d = f.displacement;
                    take(n, [t[0], t[1], t[2], d[3], d[4], d[5]], true, &mut out);
                }
            }
            continue;
        }
        let n = model
            .nodes
            .iter()
            .position(|x| x.id == f.joint)
            .ok_or_else(|| format!("foot {}: no joint of that id in the model", f.joint))?;
        if !model.nodes[n].is_support() {
            return Err(format!(
                "foot {}: the joint is not a support of the model",
                f.joint
            ));
        }
        take(n, f.displacement, false, &mut out);
    }
    if !free.is_empty() {
        out.notes.push(format!(
            "foot movements: left out where the support does not hold the joint (it follows \
             freely there; a tilt where no frame member meets it): {}",
            free.join(", ")
        ));
    }
    Ok(out)
}

/// A pole's base movement from `feet` (its [`BASE`] or [`ALL`]); none
/// when zero. Err: another foot named (a pole stands on its base).
pub fn pole_base(feet: &[FootMove]) -> Result<Option<[f64; 6]>, String> {
    let mut base = [0.0; 6];
    for f in feet.iter().filter(|f| f.displacement != [0.0; 6]) {
        if f.joint != BASE && f.joint != ALL {
            return Err(format!(
                "foot {}: a pole stands on its base — give \"{BASE}\"",
                f.joint
            ));
        }
        base = f.displacement;
    }
    Ok((base != [0.0; 6]).then_some(base))
}

/// The lean of a pole whose base turns by `base` (its rotations): the
/// axis's horizontal move per metre of height, x and y (θ_y, −θ_x)
/// ([`crate::check_pole_leaning`]).
pub fn pole_lean(base: [f64; 6]) -> [f64; 2] {
    [base[4], -base[3]]
}

/// Per member, the larger usage of `plain` (without the foot movements)
/// and `with` (with them): a member's usage with them unless without is
/// the larger. A usage that is not a number is the largest
/// ([`usage_order`]); a member without a usage in one keeps the other's.
pub fn larger_usage(plain: &[MemberUsage], with: Vec<MemberUsage>) -> Vec<MemberUsage> {
    with.into_iter()
        .map(
            |w| match (plain.iter().find(|p| p.member == w.member), w.usage) {
                (Some(p), Some(b)) => match p.usage {
                    Some(a) if usage_order(a, b).is_gt() => p.clone(),
                    _ => w,
                },
                (Some(p), None) if p.usage.is_some() => p.clone(),
                _ => w,
            },
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Member, MemberKind, Node, Section};

    fn tower() -> Model {
        let mut m = Model {
            sections: vec![Section::elastic("s", 2.1e11, 1e-3, 1e-6)],
            ..Model::default()
        };
        m.nodes = vec![
            Node::pinned("F1", [-2.0, 0.0, 0.0]),
            Node::pinned("F2", [2.0, 0.0, 0.0]),
            Node::free("T", [0.0, 0.0, 10.0]),
        ];
        m.members = vec![
            Member::new("a", MemberKind::Truss, 0, 2, 0),
            Member::new("b", MemberKind::Truss, 1, 2, 0),
        ];
        m
    }

    #[test]
    fn feet_go_on_held_directions_and_unknown_ones_are_refused() {
        let m = tower();
        let on = on_supports(
            &m,
            &[FootMove {
                joint: "F2".into(),
                displacement: [0.0, 0.0, -0.01, 0.0, 0.002, 0.0],
            }],
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(on.prescribed.len(), 1);
        assert_eq!(
            on.prescribed[0].displacement,
            [0.0, 0.0, -0.01, 0.0, 0.0, 0.0]
        );
        // The pinned foot's tilt left out, said.
        assert!(on.notes[0].contains("F2 ry"), "{:?}", on.notes);
        assert!(
            on_supports(
                &m,
                &[FootMove {
                    joint: "T".into(),
                    displacement: [0.0, 0.0, -0.01, 0.0, 0.0, 0.0]
                }]
            )
            .is_err()
        );
        // Every foot alike: each moves by t + θ × r.
        let all = on_supports(
            &m,
            &[FootMove {
                joint: ALL.into(),
                displacement: [0.0, 0.0, 0.0, 0.0, 0.001, 0.0],
            }],
        )
        .unwrap_or_else(|e| panic!("{e}"));
        // θ_y × (±2, 0, 0) = (0, 0, ∓0.002).
        assert_eq!(all.prescribed.len(), 2);
        assert!((all.prescribed[0].displacement[2] - 0.002).abs() < 1e-15);
        assert!((all.prescribed[1].displacement[2] + 0.002).abs() < 1e-15);
    }
}
