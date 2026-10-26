// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The window's look (owner: OHL-Design UI review, 2026-10-26), as
//! OHL-Structure's (its own copy: the two programs do not depend on each
//! other): a slightly larger text, scroll bars that always show, value
//! fields that do not look like buttons (a darker inset fill and a
//! frame), the UI scale chosen in View › UI scale (kept between runs),
//! and the helpers the tables share: headers in sentence case with their
//! unit ("H [kN]"), the numbers right-aligned without a unit in each cell.

use eframe::egui;

/// Key of the UI scale in the window's storage.
pub const SCALE_KEY: &str = "ohl-design-ui-scale";

/// The UI scales offered (View › UI scale).
pub const SCALES: [f32; 6] = [0.9, 1.0, 1.1, 1.25, 1.4, 1.5];

/// The UI scale as kept in `storage` (none, or out of range: 100 %).
pub fn load_scale(storage: Option<&dyn eframe::Storage>) -> f32 {
    storage
        .and_then(|s| s.get_string(SCALE_KEY))
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|s| (0.5..=3.0).contains(s))
        .unwrap_or(1.0)
}

/// Keeps the UI scale in `storage`.
pub fn save_scale(storage: &mut dyn eframe::Storage, scale: f32) {
    storage.set_string(SCALE_KEY, scale.to_string());
}

/// The UI scale menu's radio buttons (a tick menu: it stays open).
pub fn scale_menu(ui: &mut egui::Ui, scale: &mut f32) {
    for s in SCALES {
        ui.radio_value(scale, s, format!("{:.0} %", s * 100.0));
    }
}

/// The text sizes, points: one step above egui's (12.5).
fn text_styles(style: &mut egui::Style) {
    use egui::{FontFamily, FontId, TextStyle};
    style.text_styles = [
        (
            TextStyle::Small,
            FontId::new(10.5, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(13.5, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(13.5, FontFamily::Proportional),
        ),
        (
            TextStyle::Heading,
            FontId::new(19.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(13.0, FontFamily::Monospace),
        ),
    ]
    .into();
}

/// The style of the window: the text a step larger, scroll bars always
/// shown (a table wider than its panel shows it scrolls sideways), the
/// value fields unlike the buttons.
pub fn apply(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        text_styles(style);
        style.spacing.scroll = egui::style::ScrollStyle::solid();
        style.spacing.button_padding = egui::vec2(6.0, 3.0);
        // A button keeps its raised fill and gets a light frame.
        let w = &mut style.visuals.widgets;
        w.inactive.bg_stroke = egui::Stroke::new(1.0, w.inactive.bg_fill.gamma_multiply(1.6));
    });
}

/// The visuals of a value field (a number dragged or typed): the inset
/// fill of a text field and a frame, so it does not read as a button.
pub fn field_visuals(ui: &mut egui::Ui) {
    let v = ui.visuals_mut();
    let fill = v.text_edit_bg_color();
    let frame = v.widgets.noninteractive.bg_stroke.color;
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
    }
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, frame);
}

/// A value field: `drag` with the look of a text field (the visuals set
/// for it alone, no child layout: it sits in grids and rows as before).
pub fn field(ui: &mut egui::Ui, drag: egui::DragValue<'_>) -> egui::Response {
    let kept = ui.visuals().widgets.clone();
    field_visuals(ui);
    let r = ui.add(drag);
    ui.visuals_mut().widgets = kept;
    r
}

/// `text` in sentence case: its first letter a capital (the rest kept:
/// "H [kN]" stays as it is, being a symbol).
pub fn sentence(text: &str) -> String {
    // A one-letter symbol (x, y, z, H), alone or with its unit, stays.
    let symbol = text.chars().nth(1).is_none_or(|s| s == ' ');
    let mut c = text.chars();
    match c.next() {
        Some(f) if !symbol => f.to_uppercase().chain(c).collect(),
        _ => text.to_owned(),
    }
}

/// The units a column's help may end with (", m", ", kN (…)"), longest
/// first so "m²" is not read as "m".
const UNITS: [&str; 16] = [
    "kg/m", "N/m", "m/s", "m²", "°C", "kN", "kV", "mm", "kA", "MPa", "N", "m", "°", "A", "s", "%",
];

/// The unit of a table column whose help gives it after a comma ("base
/// height, m (new rows …)" → "m"), none when it gives none.
pub fn unit_of(help: &str) -> Option<&'static str> {
    let (_, rest) = help.split_once(", ")?;
    // The first word after the first comma (the unit), then nothing, a
    // bracket, a comma or a semicolon ("A wire …" is no unit).
    let word = rest
        .split([' ', '(', ',', ';'])
        .next()
        .unwrap_or_default()
        .trim();
    let after = rest[word.len()..].trim_start();
    if !(after.is_empty() || after.starts_with(['(', ',', ';'])) {
        return None;
    }
    UNITS.into_iter().find(|u| *u == word)
}

/// A table column's header: in sentence case, with its unit in square
/// brackets after it ("E [m]"; a name ending with its unit, as
/// "orientation °", gets it in brackets).
pub fn column_header(name: &str, help: &str) -> String {
    for u in UNITS {
        if let Some(bare) = name.strip_suffix(&format!(" {u}")) {
            return format!("{} [{u}]", sentence(bare));
        }
    }
    if name.contains('[') {
        return sentence(name);
    }
    match unit_of(help) {
        Some(u) => format!("{} [{u}]", sentence(name)),
        None => sentence(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
