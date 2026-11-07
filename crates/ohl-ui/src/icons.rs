// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The artwork of `assets/` in the window: the OpenOHL mark and the way
//! both applications show their SVG files. The files are built in (read
//! at compile time), so a redrawn file with the same name and format is
//! used at the next build. They are rasterised with resvg (pure Rust) into
//! textures of the size shown, kept for the next frames.

use std::collections::HashMap;

use eframe::egui;
use resvg::{tiny_skia, usvg};

/// The OpenOHL mark (start page, Help menu, About).
pub const MARK: &[u8] = include_bytes!("../../../assets/brand/openohl-mark.svg");

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

/// The window icon: an application's icon `svg`, 256 × 256.
pub fn window_icon(svg: &[u8]) -> egui::IconData {
    let (rgba, px) = rasterise(svg, 256).unwrap_or_default();
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
