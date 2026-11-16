// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A placed structure's measured foot movements ([`ohl_line::feet`];
//! owner 2026-11-16) on what the structure analysis takes: its model's
//! supports ([`on_model`]: prescribed displacements, by the one rule of
//! [`ohl_structures_fe::feet`]), a pole's base ([`on_pole`]), or the whole
//! structure as one body ([`as_one_body`]: a structure without a model).
//! One rule for the reference state, the calculation-level-3 setup, the
//! deflected bodies and the structure checks.

use ohl_line::feet::{ALL_FEET, FootMovement, POLE_BASE};
use ohl_structures_fe::feet::FootMove;
use ohl_structures_fe::{Model, Prescribed};

/// A placed structure's foot movements on its model.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct FeetOn {
    /// The prescribed displacements of its supports.
    pub prescribed: Vec<Prescribed>,
    /// What was left out (a free direction, a pinned foot's tilt), in
    /// words.
    pub notes: Vec<String>,
    /// In words for the results: "3 feet (survey, 2026-05-12)".
    pub summary: String,
}

/// `placed`'s measured foot movements, as the engine takes them (none:
/// none, or all zero).
pub(crate) fn moves(placed: Option<&ohl_line::Structure>) -> Option<(Vec<FootMove>, String)> {
    let feet = placed
        .and_then(|s| s.foot_movements.as_ref())
        .filter(|f| !f.is_empty())?;
    Some((
        feet.feet
            .iter()
            .map(|f| FootMove {
                joint: f.joint.clone(),
                displacement: f.displacement(),
            })
            .collect(),
        feet.summary(),
    ))
}

/// The foot movements of `placed` on `model`'s supports
/// ([`ohl_structures_fe::feet::on_supports`]). None: no foot movements.
/// Err: a foot that is not a support joint of the model — the movements
/// cannot be taken.
pub(crate) fn on_model(
    model: &Model,
    placed: Option<&ohl_line::Structure>,
) -> Result<Option<FeetOn>, String> {
    let Some((feet, summary)) = moves(placed) else {
        return Ok(None);
    };
    let on = ohl_structures_fe::feet::on_supports(model, &feet)?;
    Ok(Some(FeetOn {
        prescribed: on.prescribed,
        notes: on.notes,
        summary,
    }))
}

/// The movement of `placed`'s pole base (translations m, rotations rad,
/// structure axes): its foot [`POLE_BASE`] or [`ALL_FEET`]. None: no foot
/// movements. Err: another foot named (a pole stands on one base).
pub(crate) fn on_pole(placed: Option<&ohl_line::Structure>) -> Result<Option<[f64; 6]>, String> {
    match moves(placed) {
        None => Ok(None),
        Some((feet, _)) => ohl_structures_fe::feet::pole_base(&feet),
    }
}

/// The movement of `placed` as one body (a structure without a model):
/// its foot [`ALL_FEET`] (or [`POLE_BASE`]). None: no foot movements.
/// Err: single feet named — without a model they have no joint.
pub(crate) fn as_one_body(
    placed: Option<&ohl_line::Structure>,
) -> Result<Option<&FootMovement>, String> {
    let Some(feet) = placed
        .and_then(|s| s.foot_movements.as_ref())
        .filter(|f| !f.is_empty())
    else {
        return Ok(None);
    };
    let moving: Vec<&FootMovement> = feet.feet.iter().filter(|f| !f.is_zero()).collect();
    match moving.as_slice() {
        [one] if one.joint == ALL_FEET || one.joint == POLE_BASE => Ok(Some(one)),
        _ => Err(format!(
            "no structure model: single feet ({}) need one — give \"{ALL_FEET}\" to move it as \
             one body",
            moving
                .iter()
                .map(|f| f.joint.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}
