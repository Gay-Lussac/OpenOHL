// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The artwork of `assets/` in the window: the tool icons, the OpenOHL
//! mark and the application icon. The SVG files are built in (read at
//! compile time), so a redrawn file with the same name and format is used
//! at the next build. They are rasterised with resvg (pure Rust) into
//! textures of the size shown, kept for the next frames.
//!
//! Tool icons are drawn in `currentColor`: they are rasterised white and
//! tinted when shown (the text colour of the theme, or the slot's colour).

use eframe::egui;
pub use ohl_ui::icons::{image, mark};

use crate::tools::Tool;

/// OHL-Design's application icon (the window icon).
pub const APP_ICON: &[u8] = include_bytes!("../../../assets/brand/ohl-design-icon.svg");

impl Tool {
    /// Its icon, an SVG of `assets/icons/tools/` (24 × 24, `currentColor`).
    pub fn svg(self) -> &'static [u8] {
        macro_rules! tool {
            ($f:literal) => {
                include_bytes!(concat!("../../../assets/icons/tools/", $f, ".svg"))
            };
        }
        match self {
            Tool::Select => tool!("select"),
            Tool::Move => tool!("move"),
            Tool::Delete => tool!("delete"),
            Tool::GoTo => tool!("goto"),
            Tool::Edit => tool!("edit"),
            Tool::AddStructure => tool!("add-structure"),
            Tool::AddString => tool!("add-string"),
            Tool::AddPost => tool!("add-post"),
            Tool::StringWire => tool!("string-wire"),
            Tool::AddAlignment => tool!("add-alignment"),
            Tool::NewAlignment => tool!("new-alignment"),
            Tool::EditAlignment => tool!("edit-alignment"),
            Tool::AddJumper => tool!("add-jumper"),
            Tool::OnSpan => tool!("on-span"),
            Tool::Replace => tool!("replace"),
            Tool::Link => tool!("link"),
            Tool::MeasureDistance => tool!("measure-distance"),
            Tool::MeasureHeight => tool!("measure-height"),
            Tool::MeasureStations => tool!("measure-stations"),
            Tool::Note => tool!("note"),
            Tool::MeasureWires => tool!("measure-wires"),
            Tool::MeasureClearance => tool!("measure-clearance"),
            Tool::MeasureObject => tool!("measure-object"),
            Tool::MeasureAngle => tool!("measure-angle"),
            Tool::Follow => tool!("follow"),
        }
    }

    /// Its icon, `size` points square, white: tint it
    /// ([`egui::Image::tint`]) or let a button tint it with its text
    /// colour.
    pub fn image(self, ctx: &egui::Context, size: f32) -> egui::Image<'static> {
        image(ctx, self.key_name(), self.svg(), size)
    }
}

/// The window icon: OHL-Design's application icon, 256 × 256.
pub fn window_icon() -> egui::IconData {
    ohl_ui::icons::window_icon(APP_ICON)
}

/// The colour a tool icon takes beside text in `visuals` (menus, lists):
/// the text colour, light on the dark theme and dark on the light one.
pub fn text_tint(visuals: &egui::Visuals) -> egui::Color32 {
    visuals.text_color()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ohl_ui::icons::{MARK, rasterise};

    /// Every tool has an icon that parses and draws something.
    #[test]
    fn every_tool_icon_parses() {
        for t in Tool::ALL {
            let (rgba, px) = rasterise(t.svg(), 24).unwrap_or_else(|e| panic!("{t:?}: {e}"));
            assert_eq!(rgba.len(), (px * px * 4) as usize);
            assert!(rgba.chunks(4).any(|p| p[3] > 0), "{t:?}: nothing drawn");
            // Drawn in currentColor: white once rasterised (then tinted).
            assert!(
                rgba.chunks(4)
                    .filter(|p| p[3] == 255)
                    .all(|p| p[..3] == [255, 255, 255]),
                "{t:?}: not drawn in currentColor"
            );
        }
        rasterise(MARK, 64).unwrap();
    }

    #[test]
    fn the_window_icon_is_square_and_drawn() {
        let icon = window_icon();
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        assert!(icon.rgba.chunks(4).any(|p| p[3] == 255));
    }

    /// Icons beside text follow the theme: light on dark, dark on light.
    #[test]
    fn the_tint_follows_the_theme() {
        let dark = text_tint(&egui::Visuals::dark());
        let light = text_tint(&egui::Visuals::light());
        let lum = |c: egui::Color32| u32::from(c.r()) + u32::from(c.g()) + u32::from(c.b());
        assert!(lum(dark) > lum(light), "{dark:?} vs {light:?}");
        assert!(lum(dark) > 3 * 128 && lum(light) < 3 * 128);
    }

    /// Drawn on a button, a tool icon takes the button's text colour:
    /// light lines on the dark theme, dark lines on the light theme.
    #[test]
    fn a_drawn_icon_follows_the_theme() {
        let gpu = ohl_render::headless_device().is_ok();
        assert!(
            gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
            "no GPU"
        );
        if !gpu {
            return;
        }
        let lum = |p: &image::Rgba<u8>| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]);
        let mut range = Vec::new();
        for theme in [egui::Theme::Dark, egui::Theme::Light] {
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::vec2(80.0, 80.0))
                .with_theme(theme)
                .wgpu()
                .build_ui(|ui| {
                    ui.add(
                        egui::Button::image(Tool::Delete.image(ui.ctx(), 48.0))
                            .frame(false)
                            .image_tint_follows_text_color(true),
                    );
                });
            harness.run();
            let image = harness.render().unwrap();
            let (lo, hi) = image.pixels().fold((u32::MAX, 0), |(lo, hi), p| {
                (lo.min(lum(p)), hi.max(lum(p)))
            });
            range.push((lo, hi));
        }
        // Dark theme: the background is dark, the brightest pixels are the
        // icon's (light); light theme: the darkest are the icon's (dark).
        assert!(range[0].1 > 3 * 150, "dark theme: {range:?}");
        assert!(range[1].0 < 3 * 100, "light theme: {range:?}");
    }
}
