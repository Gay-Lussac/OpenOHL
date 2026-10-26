// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The window's look (owner: UI review step 1, 2026-10-26): a slightly
//! larger text, scroll bars that always show, value fields that do not
//! look like buttons (a darker inset fill and a frame), the UI scale
//! chosen in View › UI scale (kept between runs), and the helpers the
//! tables share: headers in sentence case with their unit ("x [m]"), the
//! numbers right-aligned without a unit in every cell.

use eframe::egui;

/// Key of the UI scale in the window's storage.
pub const SCALE_KEY: &str = "ohl-structure-ui-scale";

/// The UI scales offered (View › UI scale).
pub const SCALES: [f32; 6] = [0.9, 1.0, 1.1, 1.25, 1.4, 1.5];

/// The UI scale as kept in `storage` (none, or out of range: 100 %).
pub fn load_scale(storage: Option<&dyn eframe::Storage>) -> f32 {
    storage
        .and_then(|s| eframe::get_value::<f32>(s, SCALE_KEY))
        .filter(|s| (0.5..=3.0).contains(s))
        .unwrap_or(1.0)
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
/// widgets' frames visible.
pub fn apply(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        text_styles(style);
        style.spacing.scroll = egui::style::ScrollStyle::solid();
        style.spacing.button_padding = egui::vec2(6.0, 3.0);
        style.spacing.interact_size.y = 22.0;
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

/// `text` in sentence case: its first letter a capital (the rest kept:
/// "x [m]" stays as it is, being a symbol).
pub fn sentence(text: &str) -> String {
    // A one-letter symbol (x, y, z, k), alone or with its unit, stays.
    let symbol = text.chars().nth(1).is_none_or(|s| s == ' ');
    let mut c = text.chars();
    match c.next() {
        Some(f) if !symbol => f.to_uppercase().chain(c).collect(),
        _ => text.to_owned(),
    }
}

/// A column header: in sentence case, bold, its tip on hover (none when
/// empty).
pub fn header(ui: &mut egui::Ui, text: &str, tip: &str) -> egui::Response {
    let r = ui.strong(sentence(text));
    if tip.is_empty() {
        r
    } else {
        r.on_hover_text(tip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_are_in_sentence_case_symbols_kept() {
        assert_eq!(sentence("attachment point"), "Attachment point");
        assert_eq!(sentence("x [m]"), "x [m]");
        assert_eq!(sentence("x"), "x");
        assert_eq!(sentence("offset x [m]"), "Offset x [m]");
        assert_eq!(sentence(""), "");
    }
}
