// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A small 3D preview for windows (a structure in its own axes): a scene
//! built by the caller (again only when its key changes), turned with a
//! drag, panned with a right-drag, zoomed with the wheel, with labels
//! painted over it.

use eframe::egui;
use eframe::egui_wgpu;
use ohl_render::{Bounds, Camera, GpuScene, Renderer, Scene};

/// A text painted at a point of the preview.
pub type Label = ([f64; 3], String, egui::Color32);

/// State of a preview (kept by the application: the windows' forms are
/// copied every frame).
#[derive(Default)]
pub struct Preview {
    camera: Option<Camera>,
    renderer: Option<Renderer>,
    gpu: Option<GpuScene>,
    texture: Option<egui::TextureId>,
    key: Option<u64>,
    /// What it shows (a new one: fitted again).
    shows: Option<String>,
}

impl Preview {
    /// Shows the scene `build` makes (called when `key` differs from the
    /// last one), `size` points large, fitted to `fit` the first time (and
    /// after [`Preview::refit`]).
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        rs: Option<&egui_wgpu::RenderState>,
        size: egui::Vec2,
        key: u64,
        build: impl FnOnce() -> Scene,
        labels: &[Label],
    ) {
        let Some(rs) = rs else {
            ui.allocate_ui(size, |ui| {
                ui.centered_and_justified(|ui| ui.label("The preview needs a GPU (wgpu)."));
            });
            return;
        };
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let device = &rs.device;
        if self.key != Some(key) || self.gpu.is_none() {
            let scene = build();
            if self.camera.is_none() {
                self.camera = Some(fit(scene.bounds()));
            }
            self.gpu = Some(GpuScene::new(device, &scene));
            self.key = Some(key);
        }
        let camera = self.camera.get_or_insert_with(|| fit(None));
        if response.dragged() {
            let d = response.drag_delta();
            if response.dragged_by(egui::PointerButton::Primary) {
                camera.orbit(f64::from(d.x) * 0.008, f64::from(d.y) * 0.008);
            } else {
                camera.pan(f64::from(d.x), f64::from(d.y), f64::from(rect.height()));
            }
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                camera.zoom((-f64::from(scroll) * 0.002).exp());
            }
        }
        if response.double_clicked() {
            self.camera = None;
            self.key = None;
            ui.ctx().request_repaint();
        }
        let camera = *self.camera.get_or_insert_with(|| fit(None));
        let renderer = self.renderer.get_or_insert_with(|| Renderer::new(device));
        let ppp = ui.ctx().pixels_per_point();
        let px = [
            (rect.width() * ppp).round().max(1.0) as u32,
            (rect.height() * ppp).round().max(1.0) as u32,
        ];
        let layers: Vec<&GpuScene> = self.gpu.iter().collect();
        let texture = renderer.render(device, &rs.queue, &layers, &camera, px);
        let view = texture.create_view(&eframe::wgpu::TextureViewDescriptor {
            format: Some(eframe::wgpu::TextureFormat::Rgba8Unorm),
            ..Default::default()
        });
        let mut egui_renderer = rs.renderer.write();
        let id = match self.texture {
            Some(id) => {
                egui_renderer.update_egui_texture_from_wgpu_texture(
                    device,
                    &view,
                    eframe::wgpu::FilterMode::Linear,
                    id,
                );
                id
            }
            None => {
                let id = egui_renderer.register_native_texture(
                    device,
                    &view,
                    eframe::wgpu::FilterMode::Linear,
                );
                self.texture = Some(id);
                id
            }
        };
        drop(egui_renderer);
        let painter = ui.painter_at(rect);
        painter.image(
            id,
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        let size = [f64::from(rect.width()), f64::from(rect.height())];
        let project = camera.projector(size);
        let placed = place_labels(
            &painter,
            labels.iter().filter_map(|(at, text, color)| {
                let (q, _) = project(*at)?;
                Some((
                    rect.min + egui::vec2(q[0] as f32, q[1] as f32),
                    text,
                    *color,
                ))
            }),
            response.hover_pos(),
        );
        for (galley, at, color) in placed {
            painter.galley(at, galley, color);
        }
        painter.text(
            rect.left_bottom() + egui::vec2(6.0, -4.0),
            egui::Align2::LEFT_BOTTOM,
            "drag: turn · right-drag: pan · wheel: zoom · double-click: fit",
            egui::FontId::proportional(10.0),
            egui::Color32::GRAY,
        );
    }

    /// Says what it shows: something else than before is fitted again.
    pub fn shows(&mut self, what: &str) {
        if self.shows.as_deref() != Some(what) {
            self.shows = Some(what.to_owned());
            self.refit();
        }
    }

    /// Fit again on the next frame.
    pub fn refit(&mut self) {
        self.camera = None;
        self.key = None;
    }
}

/// A camera seeing `bounds` whole, from behind and to the right, a little
/// above.
fn fit(bounds: Option<Bounds>) -> Camera {
    let b = bounds.unwrap_or(Bounds {
        min: [-5.0, -5.0, 0.0],
        max: [5.0, 5.0, 10.0],
    });
    let mut c = Camera::fit(&b);
    c.distance *= 1.15;
    c.pitch = 20f64.to_radians();
    // Looking ahead (+y) from behind the right side.
    c.yaw = (-60f64).to_radians();
    c
}

/// The labels of a preview without overlaps (owner: OHL-Design UI review,
/// 2026-10-26: the structure preview's labels were written over each
/// other): each centred on its point, or moved a line down or up when
/// that is free; one that still meets a label already written is left
/// out — shown while the pointer (`hover`) is near its point, over the
/// others. The galleys and where they are written.
fn place_labels<'a>(
    painter: &egui::Painter,
    labels: impl Iterator<Item = (egui::Pos2, &'a String, egui::Color32)>,
    hover: Option<egui::Pos2>,
) -> Vec<(std::sync::Arc<egui::Galley>, egui::Pos2, egui::Color32)> {
    let mut taken: Vec<egui::Rect> = Vec::new();
    let mut out = Vec::new();
    let mut hovered = None;
    for (at, text, color) in labels {
        let galley = painter.layout_no_wrap(text.clone(), egui::FontId::proportional(11.0), color);
        let size = galley.size();
        let near = hover.is_some_and(|h| h.distance(at) < 10.0);
        let spots = [0.0, 1.0, -1.0]
            .map(|k| egui::Rect::from_center_size(at + egui::vec2(0.0, k * size.y), size));
        match spots
            .iter()
            .find(|r| !taken.iter().any(|t| t.expand(1.0).intersects(**r)))
        {
            Some(r) => {
                taken.push(*r);
                out.push((galley, r.min, color));
            }
            None if near => hovered = Some((galley, spots[0].min, color)),
            None => {}
        }
    }
    out.extend(hovered);
    out
}
