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

use std::collections::HashMap;

use eframe::egui;
use resvg::{tiny_skia, usvg};

use crate::tools::Tool;

/// The OpenOHL mark (start page, Help menu).
pub const MARK: &[u8] = include_bytes!("../../../assets/brand/openohl-mark.svg");

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
        }
    }

    /// Its icon, `size` points square, white: tint it
    /// ([`egui::Image::tint`]) or let a button tint it with its text
    /// colour.
    pub fn image(self, ctx: &egui::Context, size: f32) -> egui::Image<'static> {
        image(ctx, self.key_name(), self.svg(), size)
    }
}

/// An SVG rasterised: RGBA, unmultiplied alpha, `px` × `px` (the drawing
/// fitted, centred). `currentColor` is drawn white.
pub fn rasterise(svg: &[u8], px: u32) -> Result<(Vec<u8>, u32), String> {
    let text = std::str::from_utf8(svg).map_err(|e| e.to_string())?;
    let text = text.replace("currentColor", "#ffffff");
    let tree = usvg::Tree::from_str(&text, &usvg::Options::default()).map_err(|e| e.to_string())?;
    let px = px.max(1);
    let mut pixmap = tiny_skia::Pixmap::new(px, px).ok_or("empty image")?;
    let size = tree.size();
    let scale = px as f32 / size.width().max(size.height());
    let dx = (px as f32 - size.width() * scale) / 2.0;
    let dy = (px as f32 - size.height() * scale) / 2.0;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale).post_translate(dx, dy),
        &mut pixmap.as_mut(),
    );
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    Ok((rgba, px))
}

/// The window icon: OHL-Design's application icon, 256 × 256.
pub fn window_icon() -> egui::IconData {
    let (rgba, px) = rasterise(APP_ICON, 256).unwrap_or_default();
    egui::IconData {
        rgba,
        width: px,
        height: px,
    }
}

/// Textures made, by name and size in pixels.
#[derive(Clone, Default)]
struct Textures(HashMap<(&'static str, u32), egui::TextureHandle>);

/// `svg` as an image `size` points square (rasterised once per size in
/// pixels, kept in the context).
pub fn image(
    ctx: &egui::Context,
    name: &'static str,
    svg: &[u8],
    size: f32,
) -> egui::Image<'static> {
    let px = (size * ctx.pixels_per_point()).round().max(1.0) as u32;
    let id = egui::Id::new("ohl-icon-textures");
    let key = (name, px);
    let known = ctx.data(|d| {
        d.get_temp::<Textures>(id)
            .and_then(|t| t.0.get(&key).cloned())
    });
    let texture = known.unwrap_or_else(|| {
        let image = match rasterise(svg, px) {
            Ok((rgba, px)) => {
                egui::ColorImage::from_rgba_unmultiplied([px as usize, px as usize], &rgba)
            }
            Err(e) => {
                log::warn!("icon {name}: {e}");
                egui::ColorImage::filled([1, 1], egui::Color32::TRANSPARENT)
            }
        };
        let t = ctx.load_texture(format!("icon:{name}"), image, egui::TextureOptions::LINEAR);
        ctx.data_mut(|d| {
            d.get_temp_mut_or_default::<Textures>(id)
                .0
                .insert(key, t.clone());
        });
        t
    });
    egui::Image::from_texture(egui::load::SizedTexture::new(
        texture.id(),
        egui::vec2(size, size),
    ))
}

/// The OpenOHL mark, `size` points square (its own colours).
pub fn mark(ctx: &egui::Context, size: f32) -> egui::Image<'static> {
    image(ctx, "openohl-mark", MARK, size)
}

/// The colour a tool icon takes beside text in `visuals` (menus, lists):
/// the text colour, light on the dark theme and dark on the light one.
pub fn text_tint(visuals: &egui::Visuals) -> egui::Color32 {
    visuals.text_color()
}

#[cfg(test)]
mod tests {
    use super::*;

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
