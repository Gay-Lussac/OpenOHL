// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The window's look: the shared one ([`ohl_ui::style`]) under OHL-Design's
//! own storage key for the UI scale.

pub use ohl_ui::style::{apply, column_header, field, unit_of};

/// Key of the UI scale in the window's storage.
pub const SCALE_KEY: &str = "ohl-design-ui-scale";

/// The UI scale as kept in `storage` (none, or out of range: 100 %).
pub fn load_scale(storage: Option<&dyn eframe::Storage>) -> f32 {
    ohl_ui::style::load_scale(storage, SCALE_KEY)
}

/// Keeps the UI scale in `storage`.
pub fn save_scale(storage: &mut dyn eframe::Storage, scale: f32) {
    ohl_ui::style::save_scale(storage, SCALE_KEY, scale);
}

/// The UI scale menu's radio buttons (a tick menu: it stays open).
pub fn scale_menu(ui: &mut eframe::egui::Ui, scale: &mut f32) {
    ohl_ui::style::scale_menu(ui, scale, None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ohl_ui::style::sentence;

    #[test]
    fn headers_are_in_sentence_case_symbols_kept() {
        assert_eq!(sentence("sag [m]"), "Sag [m]");
        assert_eq!(sentence("H [kN]"), "H [kN]");
        assert_eq!(sentence("z"), "z");
        assert_eq!(sentence(""), "");
    }

    #[test]
    fn a_column_takes_its_unit_from_its_help() {
        assert_eq!(column_header("E", "base easting, m"), "E [m]");
        assert_eq!(
            column_header("z", "base height, m (new rows, empty: on the ground)"),
            "z [m]"
        );
        assert_eq!(
            column_header("orientation °", "cross-arm direction"),
            "Orientation [°]"
        );
        assert_eq!(column_header("id", "unique id"), "Id");
        assert_eq!(
            column_header("cable", "wire type id from the wire library"),
            "Cable"
        );
        assert_eq!(unit_of("area, m² (of the face)"), Some("m²"));
        assert_eq!(unit_of("its strings, A wire each"), None);
        assert_eq!(unit_of("what it is, in words"), None);
    }
}
