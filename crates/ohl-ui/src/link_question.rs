// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The question asked when an attachment point is linked to a joint it is
//! not on (owner 2026-11-15: "linking it should ask if we want to move it
//! on the joint or create an offset"): **Move the point onto the joint**
//! (the default) or **Keep its position** (its distance kept as its
//! offset). One copy for every place that asks: OHL-Structure's list,
//! Link and Move attachment point; OHL-Design's Link and Move attachment
//! point and its Structure types page (2026-11-16). Each window keeps its
//! own frame (its title, its (?) help, its buttons).

use eframe::egui;

/// The offset's length, m.
fn length(offset: [f64; 3]) -> f64 {
    offset.iter().map(|v| v * v).sum::<f64>().sqrt()
}

/// The question in words, for the window's top sentence: point `label`
/// is `offset` (from the joint to the point) from joint `joint`.
pub fn asked(label: &str, joint: &str, offset: [f64; 3]) -> String {
    format!(
        "Point {label} is {:.3} m from joint {joint}: move it onto the joint, or keep its \
         position at an offset?",
        length(offset)
    )
}

/// The two choices, `how` the answer: `onto` (the point moves onto joint
/// `joint`, its strings with it) or `keep` (its position kept, `offset`
/// from the joint; `keep_hover`: the offset in words, on hover).
pub fn choices<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    how: &mut T,
    onto: T,
    keep: T,
    joint: &str,
    offset: [f64; 3],
    keep_hover: &str,
) {
    let o = offset;
    ui.radio_value(how, onto, "Move the point onto the joint")
        .on_hover_text(format!(
            "The point moves {:.3} m onto joint {joint}; the strings hanging from it follow (the \
             default)",
            length(o)
        ));
    ui.radio_value(
        how,
        keep,
        format!(
            "Keep its position (offset {:.3}, {:.3}, {:.3} m from the joint)",
            o[0], o[1], o[2]
        ),
    )
    .on_hover_text(keep_hover);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_question_gives_the_distance() {
        assert_eq!(
            asked("A1", "N2", [0.0, 3.0, 4.0]),
            "Point A1 is 5.000 m from joint N2: move it onto the joint, or keep its position at \
             an offset?"
        );
    }
}
