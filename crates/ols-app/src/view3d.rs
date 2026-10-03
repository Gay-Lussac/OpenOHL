// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The 3D view: the renderer's image of the visible layers, a camera
//! moved with the mouse and the keyboard as the chosen control scheme
//! says ([`Controls`]), and picking.

use std::collections::{HashMap, HashSet};

use eframe::egui;
use eframe::egui_wgpu;
use ols_render::{Camera, GpuScene, Renderer};

use crate::controls::{Controls, Drag};
use crate::model::Model;
use crate::scene::{self, Layer};
use crate::selection::{self, Selection};

/// State of the 3D view.
#[derive(Default)]
pub struct View3d {
    /// Camera (set when a model is shown).
    pub camera: Option<Camera>,
    /// Navigation controls.
    pub controls: Controls,
    renderer: Option<Renderer>,
    gpu: HashMap<Layer, GpuScene>,
    /// Layers to rebuild before the next frame.
    pub dirty: HashSet<Layer>,
    texture: Option<egui::TextureId>,
    /// The mouse is captured (first person).
    pub captured: bool,
    /// The release of the press that captured the mouse is not a click.
    swallow_release: bool,
}

impl View3d {
    /// Captures (`on`) or frees the mouse: captured, it is locked and
    /// hidden and moving it turns the view.
    pub fn set_captured(captured: &mut bool, ctx: &egui::Context, on: bool) {
        *captured = on;
        log::info!("3D view: mouse {}", if on { "captured" } else { "freed" });
        let grab = if on {
            egui::CursorGrab::Locked
        } else {
            egui::CursorGrab::None
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::CursorGrab(grab));
        ctx.send_viewport_cmd(egui::ViewportCommand::CursorVisible(!on));
    }

    /// Marks every layer for rebuilding.
    pub fn rebuild_all(&mut self) {
        self.dirty.extend(Layer::ALL);
    }

    /// Shows the view.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        rs: Option<&egui_wgpu::RenderState>,
        model: &Model,
        case: usize,
        selection: &mut Selection,
        visible: &HashSet<Layer>,
    ) {
        let Some(rs) = rs else {
            ui.centered_and_justified(|ui| ui.label("3D view needs a GPU (wgpu)."));
            return;
        };
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        let camera = self.camera.get_or_insert_with(|| {
            let mut s = scene::layer(model, Layer::Structures, case, selection);
            if s.bounds().is_none() {
                s = scene::layer(model, Layer::Wires, case, selection);
            }
            s.bounds().map_or(
                Camera::fit(&ols_render::Bounds {
                    min: model.origin,
                    max: model.origin,
                }),
                |b| Camera::fit(&b),
            )
        });
        // Input.
        let size = [f64::from(rect.width()), f64::from(rect.height())];
        let local = |p: egui::Pos2| [f64::from(p.x - rect.min.x), f64::from(p.y - rect.min.y)];
        let centre = [0.5 * size[0], 0.5 * size[1]];
        // Mouse capture (first person): released by Escape, by losing the
        // window's focus, or by a scheme without capture.
        if self.captured {
            let release = ui.input(|i| i.key_pressed(egui::Key::Escape) || !i.focused)
                || !self.controls.capture;
            if release {
                Self::set_captured(&mut self.captured, ui.ctx(), false);
            }
        }
        let mut pick_at: Option<([f64; 2], bool)> = None;
        if self.captured {
            // Raw mouse movement turns the view (mouse down: look down).
            let d = ui.input(|i| {
                i.events.iter().fold(egui::Vec2::ZERO, |acc, e| match e {
                    egui::Event::MouseMoved(d) => acc + *d,
                    _ => acc,
                })
            });
            if d != egui::Vec2::ZERO {
                log::trace!("3D view: mouse moved {d:?}");
                let k = self.controls.mouse_sensitivity;
                camera.look(f64::from(d.x) * k, -f64::from(d.y) * k);
            }
            // Clicks aim with the crosshair.
            let (click, double) = ui.input(|i| {
                (
                    i.pointer.button_clicked(egui::PointerButton::Primary),
                    i.pointer
                        .button_double_clicked(egui::PointerButton::Primary),
                )
            });
            if self.swallow_release {
                if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
                    self.swallow_release = false;
                }
            } else if click || double {
                pick_at = Some((centre, double));
            }
            // Ctrl + wheel sets the speed (the wheel alone does nothing).
            // Raw wheel events: egui turns Ctrl + wheel into a zoom.
            let notches = ui.input(|i| {
                i.events
                    .iter()
                    .map(|e| match e {
                        egui::Event::MouseWheel {
                            unit,
                            delta,
                            modifiers,
                            ..
                        } if modifiers.ctrl => {
                            f64::from(delta.y)
                                * match unit {
                                    egui::MouseWheelUnit::Line => 1.0,
                                    egui::MouseWheelUnit::Point => 1.0 / 40.0,
                                    egui::MouseWheelUnit::Page => 10.0,
                                }
                        }
                        _ => 0.0,
                    })
                    .sum::<f64>()
            });
            if notches != 0.0 {
                self.controls.speed =
                    (self.controls.speed * (notches * 0.1).exp()).clamp(1.0, 2000.0);
            }
        } else {
            // Keys act while the pointer is over the view or it has focus.
            if response.clicked() || response.drag_started() {
                response.request_focus();
            }
            // Capture on the press itself (a click that moves a little is
            // a drag, not a click); the release of that press selects
            // nothing.
            let pressed_here = response.hovered()
                && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary));
            if pressed_here {
                log::info!(
                    "3D view: press (scheme {}, captures the mouse: {})",
                    self.controls.name,
                    self.controls.capture
                );
            }
            if self.controls.capture && pressed_here {
                Self::set_captured(&mut self.captured, ui.ctx(), true);
                // Swallow its release, unless it came in the same frame.
                self.swallow_release =
                    !ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary));
                response.request_focus();
            } else if self.controls.capture {
                // Nothing else for the left button in this scheme.
            } else if (response.clicked() || response.double_clicked())
                && let Some(pos) = response.interact_pointer_pos()
            {
                pick_at = Some((local(pos), response.double_clicked()));
            }
            if response.dragged() {
                let d = response.drag_delta();
                let (dx, dy) = (f64::from(d.x), f64::from(d.y));
                let action = [
                    egui::PointerButton::Primary,
                    egui::PointerButton::Secondary,
                    egui::PointerButton::Middle,
                ]
                .into_iter()
                .find(|b| response.dragged_by(*b))
                .map_or(Drag::None, |b| self.controls.drag(b));
                match action {
                    Drag::Orbit => camera.orbit(dx * 0.008, dy * 0.008),
                    Drag::Look => camera.look(dx * 0.004, -dy * 0.004),
                    Drag::Pan => camera.pan(dx, dy, size[1]),
                    Drag::None => {}
                }
            }
            if response.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    camera.zoom((-f64::from(scroll) * 0.002).exp());
                }
            }
        }
        if self.captured || response.hovered() || response.has_focus() {
            let (v, sprint, dt) = ui.input(|i| {
                let (v, sprint) = self.controls.held(i);
                (v, sprint, f64::from(i.stable_dt).min(0.1))
            });
            if v != [0.0; 3] {
                let speed = self.controls.speed * if sprint { self.controls.sprint } else { 1.0 };
                camera.fly(v[0] * speed * dt, v[1] * speed * dt, v[2] * speed * dt);
                // Keep moving while the keys are held.
                ui.ctx().request_repaint();
            }
        }
        if let Some((at, double)) = pick_at {
            let cam = *camera;
            let cands = selection::candidates(model, case);
            let radius = if self.captured { 10.0 } else { 14.0 };
            let picked =
                selection::pick(&cands, at, radius, |p| cam.project(p, size).map(|(q, _)| q));
            if picked != *selection {
                *selection = picked;
            }
            if double && let Some(f) = selection.focus(model, case) {
                camera.target = f;
                camera.distance = camera.distance.min(150.0);
            }
        }
        // Upload changed layers, render, show.
        let device = &rs.device;
        for layer in self.dirty.drain() {
            let s = scene::layer(model, layer, case, selection);
            self.gpu.insert(layer, GpuScene::new(device, &s));
        }
        let renderer = self.renderer.get_or_insert_with(|| Renderer::new(device));
        let ppp = ui.ctx().pixels_per_point();
        let px = [
            (rect.width() * ppp).round().max(1.0) as u32,
            (rect.height() * ppp).round().max(1.0) as u32,
        ];
        let layers: Vec<&GpuScene> = Layer::ALL
            .iter()
            .filter(|l| visible.contains(l))
            .filter_map(|l| self.gpu.get(l))
            .collect();
        let texture = renderer.render(device, &rs.queue, &layers, camera, px);
        // egui takes texture bytes as sRGB-encoded: a plain view of them.
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
        // Label of the selection.
        if let Some(f) = selection.focus(model, case)
            && let Some((q, _)) = camera.project(f, size)
        {
            painter.text(
                rect.min + egui::vec2(q[0] as f32 + 10.0, q[1] as f32 - 10.0),
                egui::Align2::LEFT_BOTTOM,
                selection.describe(),
                egui::FontId::proportional(14.0),
                egui::Color32::WHITE,
            );
        }
        if self.captured {
            let c = rect.center();
            for (a, b) in [
                (egui::vec2(-9.0, 0.0), egui::vec2(9.0, 0.0)),
                (egui::vec2(0.0, -9.0), egui::vec2(0.0, 9.0)),
            ] {
                painter.line_segment(
                    [c + a, c + b],
                    egui::Stroke::new(4.0, egui::Color32::from_black_alpha(160)),
                );
                painter.line_segment([c + a, c + b], egui::Stroke::new(2.0, egui::Color32::WHITE));
            }
            painter.text(
                rect.center_top() + egui::vec2(0.0, 8.0),
                egui::Align2::CENTER_TOP,
                format!(
                    "Esc: free the mouse · click: select at the crosshair · W A S D Space Shift · Ctrl faster · Ctrl + wheel: speed {:.0} m/s",
                    self.controls.speed
                ),
                egui::FontId::proportional(12.0),
                egui::Color32::WHITE,
            );
        }
        painter.text(
            rect.left_bottom() + egui::vec2(8.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            format!(
                "{}: W A S D move · Space up · Shift down · Ctrl faster · {} · right-drag: pan · scroll: zoom · double-click: fly to",
                self.controls.name,
                if self.controls.capture {
                    "click: capture the mouse (Esc frees it)"
                } else if self.controls.left == Drag::Look {
                    "drag: look around"
                } else {
                    "drag: turn"
                }
            ),
            egui::FontId::proportional(11.0),
            egui::Color32::from_gray(150),
        );
    }
}
