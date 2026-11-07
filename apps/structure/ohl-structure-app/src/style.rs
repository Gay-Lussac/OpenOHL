// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The window's look: the shared one ([`ohl_ui::style`]) under
//! OHL-Structure's own storage key for the UI scale, with what is its own:
//! taller rows and value fields right-aligned in their cells.

use eframe::egui;
pub use ohl_ui::style::{field_visuals, header};

/// Key of the UI scale in the window's storage.
pub const SCALE_KEY: &str = "ohl-structure-ui-scale";

/// The UI scale as kept in `storage` (none, or out of range: 100 %).
pub fn load_scale(storage: Option<&dyn eframe::Storage>) -> f32 {
    ohl_ui::style::load_scale(storage, SCALE_KEY)
}

/// Keeps the UI scale in `storage`.
pub fn save_scale(storage: &mut dyn eframe::Storage, scale: f32) {
    ohl_ui::style::save_scale(storage, SCALE_KEY, scale);
}

/// The style of the window: the shared look, its rows a little taller.
pub fn apply(ctx: &egui::Context) {
    ohl_ui::style::apply(ctx);
    ctx.all_styles_mut(|style| style.spacing.interact_size.y = 22.0);
}

/// A value field: `drag` with the look of a text field, its number
/// right-aligned in its cell.
pub fn field(ui: &mut egui::Ui, drag: egui::DragValue<'_>) -> egui::Response {
    num(ui, |ui| {
        field_visuals(ui);
        ui.add(drag)
    })
}

/// `add` right-aligned in its cell (a number column of a grid).
pub fn num<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add)
        .inner
}

/// A number shown, right-aligned in its cell.
pub fn num_label(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    num(ui, |ui| ui.label(text))
}

#[cfg(test)]
mod tests {
    use ohl_ui::style::sentence;

    #[test]
    fn headers_are_in_sentence_case_symbols_kept() {
        assert_eq!(sentence("attachment point"), "Attachment point");
        assert_eq!(sentence("x [m]"), "x [m]");
        assert_eq!(sentence("x"), "x");
        assert_eq!(sentence("offset x [m]"), "Offset x [m]");
        assert_eq!(sentence(""), "");
    }
}
