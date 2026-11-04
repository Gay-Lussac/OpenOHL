// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A member's roll, its correction angle about its axis (owner
//! 2026-11-04: "i need to have a correction angle for the members to
//! rotate them on their axis as one face need to come in contact with the
//! plates so usually flat"): degrees typed, a quarter turn either way, a
//! half turn, and **Lay flat** — the turn that lays its leg bolted flat on
//! the plate or the leg it bolts onto ([`ohl_structure_link::flat_roll`]).
//! Shown in Member details, the Detail view (a member) and the Inspector;
//! each change one undo step on the member's `roll` (with an
//! `orientation`, a turn after it). The member's axes for the analysis
//! turn with it.

use eframe::egui;
use ohl_structure_link::{Flat, JointDetails};

/// A member end's leg bolted against what it bolts onto: its joint, what
/// it bolts onto and how far off.
#[derive(Debug, Clone)]
pub struct FlatEnd {
    /// The joint.
    pub joint: String,
    /// What it bolts onto (a plate, a member).
    pub onto: String,
    /// How far off and the turn that lays it flat.
    pub flat: Flat,
}

/// The ends of `member` that lie on something drawn (a plate, an angle's
/// flange), in the joints' order.
pub fn flat_ends(details: &JointDetails, member: &str) -> Vec<FlatEnd> {
    details
        .joints
        .iter()
        .flat_map(|j| {
            j.arrivals
                .iter()
                .filter(|a| a.member == member)
                .filter_map(|a| {
                    Some(FlatEnd {
                        joint: j.id.clone(),
                        onto: a.onto.value.clone()?,
                        flat: a.flat?,
                    })
                })
        })
        .collect()
}

/// The end Lay flat takes: the first that lies more than
/// [`ohl_structure_link::FLAT_OFF`] off, else the first.
fn worst(ends: &[FlatEnd]) -> Option<&FlatEnd> {
    ends.iter()
        .find(|e| e.flat.off > ohl_structure_link::FLAT_OFF)
        .or(ends.first())
}

/// The roll row of `member` (its roll now `roll`, radians; its ends'
/// `ends`): the degrees, −90°, +90°, 180°, Lay flat. The new roll when
/// one was asked for.
pub fn roll_control(ui: &mut egui::Ui, member: &str, roll: f64, ends: &[FlatEnd]) -> Option<f64> {
    let mut out = None;
    ui.horizontal_wrapped(|ui| {
        ui.label("Roll").on_hover_text(
            "Turns the member about its axis (its cross-section and so its legs), after its \
             orientation: a correction where a leg bolted should lie flat on a plate or a leg. \
             The member's axes for the analysis (its bending axes) turn with it. One undo step \
             each.",
        );
        let mut deg = (roll.to_degrees() * 100.0).round() / 100.0;
        let r = ui.push_id(("roll", member), |ui| {
            crate::style::field(
                ui,
                egui::DragValue::new(&mut deg)
                    .speed(0.0)
                    .range(-180.0..=180.0)
                    .max_decimals(2)
                    .suffix("°"),
            )
        });
        if r.inner.changed() {
            out = Some(deg.to_radians());
        }
        for (name, turn, tip) in [
            ("−90°", -90.0, "A quarter turn back"),
            ("+90°", 90.0, "A quarter turn on"),
            (
                "180°",
                180.0,
                "A half turn (the outstanding leg the other way)",
            ),
        ] {
            if ui.small_button(name).on_hover_text(tip).clicked() {
                out = Some(roll + f64::to_radians(turn));
            }
        }
        let pick = worst(ends);
        let tip = if ends.is_empty() {
            "Nothing drawn it bolts onto (an angle bolted onto a plate or an angle)".to_owned()
        } else {
            let each: Vec<String> = ends
                .iter()
                .map(|e| {
                    format!(
                        "at {}: {:.0}° off {} (turn {:+.1}°)",
                        e.joint,
                        e.flat.off,
                        e.onto,
                        e.flat.turn.to_degrees()
                    )
                })
                .collect();
            format!(
                "The smallest turn that lays its leg bolted flat on what it bolts onto ({}); a \
                 further 180° turns the outstanding leg the other way. {}",
                pick.map_or(String::new(), |e| format!("at {}", e.joint)),
                each.join("; ")
            )
        };
        let r = ui
            .add_enabled(pick.is_some(), egui::Button::new("Lay flat").small())
            .on_hover_text(tip.clone())
            .on_disabled_hover_text(tip);
        if r.clicked()
            && let Some(e) = pick
        {
            out = Some(roll + e.flat.turn);
        }
        if let Some(e) = pick
            && e.flat.off > ohl_structure_link::FLAT_OFF
        {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!("{:.0}° off {} at {}", e.flat.off, e.onto, e.joint),
            );
        }
    });
    out
}

/// The roll the file gives member `id`, radians (none: 0).
pub fn roll_of(t: &ohl_structures::StructureType, id: &str) -> f64 {
    t.members
        .iter()
        .find(|m| m.id == id)
        .map_or(0.0, ohl_structure_edit::edits::member_roll)
}
