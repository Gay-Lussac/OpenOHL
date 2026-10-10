// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A small 3D preview for windows (a structure in its own axes): a scene
//! built by the caller (again only when its key changes), turned with a
//! drag, panned with a right-drag, zoomed with the wheel, with labels
//! painted over it.

use eframe::egui;
use eframe::egui_wgpu;
use ols_render::{Bounds, Camera, GpuScene, Renderer, Scene};

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
        for (at, text, color) in labels {
            if let Some((q, _)) = project(*at) {
                painter.text(
                    rect.min + egui::vec2(q[0] as f32, q[1] as f32),
                    egui::Align2::CENTER_CENTER,
                    text,
                    egui::FontId::proportional(11.0),
                    *color,
                );
            }
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
