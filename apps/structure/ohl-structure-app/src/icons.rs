// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The artwork of `assets/` in the window: the action icons (Body and
//! Design), the OpenOHL mark and the application icon. The SVG files are
//! built in (read at compile time), so a redrawn file with the same name
//! and format is used at the next build. They are rasterised with resvg
//! (pure Rust) into textures of the size shown, kept for the next frames.
//!
//! Action icons are drawn in `currentColor`: they are rasterised white and
//! tinted with the text colour of the theme where they are shown.

use std::collections::HashMap;

use eframe::egui;
use resvg::{tiny_skia, usvg};

/// The OpenOHL mark (start page, About).
pub const MARK: &[u8] = include_bytes!("../../../../assets/brand/openohl-mark.svg");

/// OHL-Structure's application icon (the window icon).
pub const APP_ICON: &[u8] = include_bytes!("../../../../assets/brand/ohl-structure-icon.svg");

/// An action with an icon of `assets/icons/tools/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// Body › Add a joint.
    AddJoint,
    /// Body › Member between the two joints.
    AddMember,
    /// Body › Point on the joint.
    PointOnJoint,
    /// Body › Generators.
    Generator,
    /// Body › Symmetry.
    Symmetry,
    /// Design › Supports.
    Support,
}

impl Icon {
    /// Every icon (the tests check each file).
    #[cfg(test)]
    pub const ALL: [Icon; 6] = [
        Icon::AddJoint,
        Icon::AddMember,
        Icon::PointOnJoint,
        Icon::Generator,
        Icon::Symmetry,
        Icon::Support,
    ];

    /// Its file's name (without `.svg`).
    pub fn name(self) -> &'static str {
        match self {
            Icon::AddJoint => "add-joint",
            Icon::AddMember => "add-member",
            Icon::PointOnJoint => "point-on-joint",
            Icon::Generator => "generator",
            Icon::Symmetry => "symmetry",
            Icon::Support => "support",
        }
    }

    /// Its SVG (24 × 24, `currentColor`).
    pub fn svg(self) -> &'static [u8] {
        macro_rules! icon {
            ($f:literal) => {
                include_bytes!(concat!("../../../../assets/icons/tools/", $f, ".svg"))
            };
        }
        match self {
            Icon::AddJoint => icon!("add-joint"),
            Icon::AddMember => icon!("add-member"),
            Icon::PointOnJoint => icon!("point-on-joint"),
            Icon::Generator => icon!("generator"),
            Icon::Symmetry => icon!("symmetry"),
            Icon::Support => icon!("support"),
        }
    }

    /// The icon, `size` points square, white: a button tints it with its
    /// text colour ([`egui::Button::image_tint_follows_text_color`]).
    pub fn image(self, ctx: &egui::Context, size: f32) -> egui::Image<'static> {
        image(ctx, self.name(), self.svg(), size)
    }

    /// A button: the icon (in the text colour) and `text`.
    pub fn button(self, ui: &egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Button<'static> {
        egui::Button::image_and_text(self.image(ui.ctx(), 16.0), text)
            .image_tint_follows_text_color(true)
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

/// The window icon: OHL-Structure's application icon, 256 × 256.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every action icon parses and draws something, in `currentColor`.
    #[test]
    fn every_icon_parses() {
        for i in Icon::ALL {
            let (rgba, px) = rasterise(i.svg(), 24).unwrap_or_else(|e| panic!("{i:?}: {e}"));
            assert_eq!(rgba.len(), (px * px * 4) as usize);
            assert!(rgba.chunks(4).any(|p| p[3] > 0), "{i:?}: nothing drawn");
            assert!(
                rgba.chunks(4)
                    .filter(|p| p[3] == 255)
                    .all(|p| p[..3] == [255, 255, 255]),
                "{i:?}: not drawn in currentColor"
            );
        }
        rasterise(MARK, 64).unwrap();
    }

    /// The textures of icons made so far (their names).
    fn icons(h: &egui_kittest::Harness<'_, crate::app::StructureApp>) -> Vec<String> {
        h.ctx
            .tex_manager()
            .read()
            .allocated()
            .filter_map(|(_, m)| m.name.strip_prefix("icon:").map(str::to_owned))
            .collect()
    }

    fn screenshot(h: &mut egui_kittest::Harness<'_, crate::app::StructureApp>, name: &str) {
        if ohl_render::headless_device().is_ok()
            && let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG")
        {
            let image = h.render().unwrap();
            image.save(std::path::Path::new(&dir).join(name)).unwrap();
        }
    }

    /// The mark on the start page and in About; the icons on the Body and
    /// Design menus.
    #[test]
    fn the_mark_and_the_action_icons_are_shown() {
        use egui_kittest::kittest::Queryable;
        let gpu = ohl_render::headless_device().is_ok();
        assert!(
            gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
            "no GPU"
        );
        let mut builder = egui_kittest::Harness::builder().with_size(egui::vec2(1300.0, 900.0));
        if gpu {
            builder = builder.wgpu();
        }
        let mut h =
            builder.build_eframe(|cc| crate::app::StructureApp::new(cc.wgpu_render_state.clone()));
        h.run();
        h.get_by_label("Open a structure…");
        assert!(icons(&h).iter().any(|n| n == "openohl-mark"));
        screenshot(&mut h, "ohl-structure-start-page.png");
        crate::tests::menu(&mut h, "Help", "About OHL-Structure");
        h.get_by_label("About OHL-Structure");
        screenshot(&mut h, "ohl-structure-about.png");
        let path = crate::tests::copy_of("type-02.ohls", "icons");
        h.state_mut().open(&path);
        h.state_mut().about = false;
        h.run();
        h.get_by_label("Body").click();
        h.run();
        h.get_by_label("Add a joint");
        h.get_by_label("Generators…");
        for i in [
            Icon::AddJoint,
            Icon::AddMember,
            Icon::PointOnJoint,
            Icon::Generator,
            Icon::Symmetry,
        ] {
            assert!(icons(&h).iter().any(|n| n == i.name()), "{i:?}");
        }
        screenshot(&mut h, "ohl-structure-body-menu.png");
        h.key_press(egui::Key::Escape);
        h.run();
        h.get_by_label("Design").click();
        h.run();
        h.get_by_label("Supports…");
        assert!(icons(&h).iter().any(|n| n == Icon::Support.name()));
    }

    #[test]
    fn the_window_icon_is_square_and_drawn() {
        let icon = window_icon();
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        assert!(icon.rgba.chunks(4).any(|p| p[3] == 255));
    }
}
