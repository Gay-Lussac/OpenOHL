// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Messages in a window: long ones scroll in a box of fixed height, so a
//! window never grows beyond the screen and stays closable (owner,
//! 2026-10-26); the colour of a good result.

use eframe::egui;

/// Longest text shown whole in a window (characters, lines); longer text
/// scrolls in a box of [`TEXT_BOX_HEIGHT`].
const LONG_TEXT: (usize, usize) = (300, 4);

/// Height of the box long messages scroll in, points.
pub const TEXT_BOX_HEIGHT: f32 = 160.0;

/// A message in a window (`color`, or the usual text colour): long ones
/// (lists of errors, reports) scroll in a box of fixed height, so a window
/// never grows beyond the screen and its buttons stay in reach. Every
/// error and report in a window is shown with it ([`error_label`]).
pub fn message(
    ui: &mut egui::Ui,
    text: impl Into<egui::RichText>,
    color: Option<egui::Color32>,
) -> egui::Response {
    let text: egui::RichText = text.into();
    let show = |ui: &mut egui::Ui, text: egui::RichText| match color {
        Some(c) => ui.colored_label(c, text),
        None => ui.label(text),
    };
    let raw = text.text();
    if raw.len() <= LONG_TEXT.0 && raw.lines().count() <= LONG_TEXT.1 {
        return show(ui, text);
    }
    let id = egui::Id::new(("message", raw.len(), raw.lines().next().unwrap_or("")));
    egui::Frame::group(ui.style())
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(id)
                .max_height(TEXT_BOX_HEIGHT)
                .auto_shrink([false, true])
                .show(ui, |ui| show(ui, text))
                .inner
        })
        .inner
}

/// An error in a window, in the theme's error colour; long ones scroll
/// ([`message`]).
pub fn error_label(ui: &mut egui::Ui, text: impl Into<egui::RichText>) -> egui::Response {
    let color = ui.visuals().error_fg_color;
    message(ui, text, Some(color))
}

/// The colour of a good result (nothing found, a check passed): a green
/// readable on the dark and on the light theme (errors and warnings take
/// the theme's own colours, `error_fg_color` and `warn_fg_color`).
pub fn ok_color(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::LIGHT_GREEN
    } else {
        egui::Color32::from_rgb(0, 120, 40)
    }
}
