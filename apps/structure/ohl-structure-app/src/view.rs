// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The structure in 3D, in its own axes: joints, members, points, wire
//! points and strings, the ground at the feet with a compass. A click
//! picks (Shift + click adds), a drag from a joint or point moves it
//! (snapped to a grid or onto a joint: one undo step), elsewhere it turns
//! the view.

use eframe::egui;
use eframe::egui_wgpu;
use ohl_render::{Bounds, Camera, GpuScene, Renderer, Scene};
use ohl_structure_edit::symmetry::move_to;
use ohl_structure_edit::{Geometry, Handle, StructureFile, compass_radius, pick, position};

const STEEL: [u8; 4] = [215, 215, 220, 255];
const BARE: [u8; 4] = [200, 170, 110, 255];
const STICK: [u8; 4] = [150, 150, 160, 255];
const STRING: [u8; 4] = [120, 180, 255, 255];
const POINT: [u8; 4] = [255, 150, 60, 255];
const WIRE: [u8; 4] = [90, 200, 255, 255];
const PICKED: [u8; 4] = [255, 235, 60, 255];
const SUPPORT: [u8; 4] = [110, 220, 120, 255];
/// What a generator would build (before Apply).
const PREVIEW: [u8; 4] = [80, 230, 200, 255];
/// The pole's outline.
const POLE: [u8; 4] = [190, 190, 120, 255];
/// The members of the extension shown.
const EXTENSION: [u8; 4] = [170, 140, 230, 255];

/// A drag moving a joint or a (wire) point.
#[derive(Debug, Clone)]
struct Move {
    handle: Handle,
    /// A point of the plane it moves in (facing the camera).
    on: [f64; 3],
    /// The plane's normal.
    normal: [f64; 3],
}

/// The 3D view.
#[derive(Default)]
pub struct View {
    camera: Option<Camera>,
    renderer: Option<Renderer>,
    gpu: Option<GpuScene>,
    texture: Option<egui::TextureId>,
    /// The scene needs building.
    dirty: bool,
    drag: Option<Move>,
    /// A panel's drawing over the structure (loads, results), by its key.
    overlay: Option<(u64, GpuScene)>,
}

/// What the view needs to draw and edit.
pub struct ViewInput<'a> {
    /// The structure's geometry.
    pub g: &'a Geometry,
    /// Level of the feet.
    pub foot: f64,
    /// Picked, in order.
    pub selected: &'a mut Vec<Handle>,
    /// Grid a dragged joint or point snaps to, m (0: none).
    pub grid: f64,
    /// The angle that means ahead.
    pub ahead: f64,
    /// What the last action did (or why not: `true`).
    pub message: &'a mut Option<(String, bool)>,
    /// A panel's drawing over the structure (loads, results) and its key
    /// (built again when the key changes).
    pub overlay: Option<(u64, Scene)>,
    /// What a generator would build: lines drawn over the structure.
    pub preview: &'a [[[f64; 3]; 2]],
}

impl View {
    /// Draws the scene again on the next frame.
    pub fn redraw(&mut self) {
        self.dirty = true;
    }

    /// The whole structure in view on the next frame.
    pub fn fit(&mut self) {
        self.camera = None;
    }

    /// The view; edits go into `file`.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        rs: Option<&egui_wgpu::RenderState>,
        file: &mut StructureFile,
        v: ViewInput<'_>,
    ) {
        let Some(rs) = rs else {
            ui.centered_and_justified(|ui| ui.label("The 3D view needs a GPU (wgpu)."));
            return;
        };
        let g = v.g;
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        let camera = self.camera.get_or_insert_with(|| {
            let mut b: Option<Bounds> = None;
            for p in g.all().chain([[0.0, 0.0, v.foot]]) {
                let b = b.get_or_insert(Bounds { min: p, max: p });
                b.min = [0, 1, 2].map(|k| b.min[k].min(p[k]));
                b.max = [0, 1, 2].map(|k| b.max[k].max(p[k]));
            }
            let b = b.expect("the feet at least");
            let mut c = Camera::fit(&b);
            let aspect = f64::from(rect.width() / rect.height().max(1.0));
            c.distance *= 1.3 * (1.0 / aspect.min(1.0)).max(1.0);
            c.pitch = 15f64.to_radians();
            c
        });
        let size = [f64::from(rect.width()), f64::from(rect.height())];
        let local = |p: egui::Pos2| [f64::from(p.x - rect.min.x), f64::from(p.y - rect.min.y)];
        let cam = *camera;
        let projector = cam.projector(size);
        let project = |p: [f64; 3]| projector(p).map(|(q, _)| q);
        // Picking: a click picks, Shift + click adds.
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let shift = ui.input(|i| i.modifiers.shift);
            match pick(g, &project, local(pos), 12.0) {
                Some(h) if shift => {
                    if let Some(k) = v.selected.iter().position(|x| *x == h) {
                        v.selected.remove(k);
                    } else {
                        v.selected.push(h);
                    }
                }
                Some(h) => *v.selected = vec![h],
                None if !shift => v.selected.clear(),
                None => {}
            }
            self.dirty = true;
        }
        // A drag from a joint or point moves it; elsewhere it turns the view.
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
            && let Some(h) = pick(g, &project, local(pos), 12.0)
            && !matches!(h, Handle::Member(_))
            && let Some(at) = position(g, &h)
        {
            let eye = cam.eye();
            let n = [0, 1, 2].map(|k| eye[k] - cam.target[k]);
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
            *v.selected = vec![h.clone()];
            file.begin(&format!("move {}", h.describe()));
            self.drag = Some(Move {
                handle: h,
                on: at,
                normal: n.map(|x| x / len),
            });
        }
        if let Some(m) = self.drag.clone() {
            if response.dragged_by(egui::PointerButton::Primary)
                && let Some(pos) = response.interact_pointer_pos()
                && let Some((a, d)) = cam.ray(local(pos), size)
            {
                let dn: f64 = (0..3).map(|k| d[k] * m.normal[k]).sum();
                if dn.abs() > 1e-9 {
                    let s: f64 = (0..3).map(|k| (m.on[k] - a[k]) * m.normal[k]).sum::<f64>() / dn;
                    let mut at = [0, 1, 2].map(|k| a[k] + s * d[k]);
                    if v.grid > 0.0 {
                        at = at.map(|x| (x / v.grid).round() * v.grid);
                    }
                    // Onto a joint near the pointer.
                    let here = local(pos);
                    if let Some((_, p, _)) = g.joints.iter().find(|j| {
                        Handle::Joint(j.0.clone()) != m.handle
                            && project(j.1)
                                .is_some_and(|q| (q[0] - here[0]).hypot(q[1] - here[1]) < 8.0)
                    }) {
                        at = *p;
                    }
                    if position(g, &m.handle) != Some(at) {
                        let what = m.handle.describe();
                        match file.edit(&format!("move {what}"), |doc, t| {
                            move_to(doc, t, &m.handle, at)
                        }) {
                            Ok(()) => {
                                self.dirty = true;
                                *v.message = Some((
                                    format!("{what} at {:.3}, {:.3}, {:.3} m", at[0], at[1], at[2]),
                                    false,
                                ));
                            }
                            Err(e) => {
                                *v.message = Some((e, true));
                                self.drag = None;
                            }
                        }
                    }
                }
            }
            if !response.dragged() {
                self.drag = None;
                file.end();
            }
        } else if response.dragged() {
            let d = response.drag_delta();
            let camera = self.camera.as_mut().expect("set above");
            if response.dragged_by(egui::PointerButton::Primary) {
                camera.orbit(f64::from(d.x) * 0.008, f64::from(d.y) * 0.008);
            } else {
                camera.pan(f64::from(d.x), f64::from(d.y), size[1]);
            }
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let camera = self.camera.as_mut().expect("set above");
                camera.zoom((-f64::from(scroll) * 0.002).exp());
            }
        }
        // Draw.
        let device = &rs.device;
        if self.dirty || self.gpu.is_none() {
            self.gpu = Some(GpuScene::new(
                device,
                &scene(g, v.foot, v.selected, v.ahead, v.preview),
            ));
            self.dirty = false;
        }
        let camera = *self.camera.as_ref().expect("set above");
        let renderer = self.renderer.get_or_insert_with(|| Renderer::new(device));
        let ppp = ui.ctx().pixels_per_point();
        let px = [
            (rect.width() * ppp).round().max(1.0) as u32,
            (rect.height() * ppp).round().max(1.0) as u32,
        ];
        match v.overlay {
            Some((key, scene)) if self.overlay.as_ref().is_none_or(|o| o.0 != key) => {
                self.overlay = Some((key, GpuScene::new(device, &scene)));
            }
            Some(_) => {}
            None => self.overlay = None,
        }
        let layers: Vec<&GpuScene> = self
            .gpu
            .iter()
            .chain(self.overlay.as_ref().map(|o| &o.1))
            .collect();
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
        labels(
            &painter,
            rect,
            &camera.projector(size),
            g,
            v.foot,
            v.ahead,
            v.selected,
        );
    }
}

/// The scene: the ground at the feet (a 1 m grid), the axes, the compass,
/// the sticks, members, strings, joints, points and wire points, and a
/// generator's preview.
fn scene(
    g: &Geometry,
    foot: f64,
    selected: &[Handle],
    ahead: f64,
    preview: &[[[f64; 3]; 2]],
) -> Scene {
    let mut s = Scene::new([0.0; 3]);
    let extent = g
        .all()
        .map(|p| p[0].abs().max(p[1].abs()))
        .fold(5.0_f64, f64::max)
        .ceil()
        + 2.0;
    let n = extent as i64;
    for k in -n..=n {
        let v = k as f64;
        let c = if k % 5 == 0 {
            [70, 74, 82, 255]
        } else {
            [45, 48, 54, 255]
        };
        s.polyline(&[[v, -extent, foot], [v, extent, foot]], c, 1.0);
        s.polyline(&[[-extent, v, foot], [extent, v, foot]], c, 1.0);
    }
    s.polyline(&[[0.0; 3], [3.0, 0.0, 0.0]], [230, 80, 70, 255], 2.5);
    s.polyline(&[[0.0; 3], [0.0, 3.0, 0.0]], [90, 200, 90, 255], 2.5);
    s.polyline(&[[0.0; 3], [0.0, 0.0, 3.0]], [90, 140, 240, 255], 2.5);
    draw_compass(&mut s, compass_radius(g), ahead, foot);
    for l in &g.sticks {
        s.polyline(l, STICK, 1.5);
    }
    for l in &g.pole {
        s.polyline(l, POLE, 1.3);
    }
    let picked = |h: Handle| selected.contains(&h);
    for (id, a, b, section) in &g.members {
        let c = if picked(Handle::Member(id.clone())) {
            PICKED
        } else if g.extension.contains(id) {
            EXTENSION
        } else if *section {
            STEEL
        } else {
            BARE
        };
        s.polyline(&[*a, *b], c, 1.8);
    }
    for (_, a, b) in &g.strings {
        s.polyline(&[*a, *b], STRING, 2.5);
    }
    for l in preview {
        s.polyline(l, PREVIEW, 2.2);
    }
    for (id, p, support) in &g.joints {
        let (c, size) = match (picked(Handle::Joint(id.clone())), support) {
            (true, _) => (PICKED, 11.0),
            (false, true) => (SUPPORT, 9.0),
            (false, false) => (STEEL, 6.0),
        };
        s.point(*p, c, size);
    }
    for (l, p) in &g.points {
        let c = if picked(Handle::Point(l.clone())) {
            PICKED
        } else {
            POINT
        };
        s.point(*p, c, 10.0);
    }
    for (l, p, _) in &g.wires {
        let c = if picked(Handle::Wire(l.clone())) {
            PICKED
        } else {
            WIRE
        };
        s.point(*p, c, 10.0);
    }
    s
}

/// Where a plan angle as counted (clockwise, `ahead` meaning +y) points.
fn compass_direction(angle: f64, ahead: f64) -> [f64; 2] {
    let phi = (angle - ahead).to_radians();
    [phi.sin(), phi.cos()]
}

/// The compass on the ground (radius `r`, at height `z`): a ring with
/// ticks every 15°, ahead (green) and right (red) from the centre.
fn draw_compass(s: &mut Scene, r: f64, ahead: f64, z: f64) {
    let ring: Vec<[f64; 3]> = (0..=72)
        .map(|k| {
            let t = f64::from(k) * std::f64::consts::TAU / 72.0;
            [r * t.cos(), r * t.sin(), z]
        })
        .collect();
    s.polyline(&ring, [110, 115, 125, 255], 1.5);
    for k in 0..24 {
        let d = compass_direction(f64::from(k) * 15.0, ahead);
        let inner = if k % 6 == 0 {
            0.82
        } else if k % 2 == 0 {
            0.9
        } else {
            0.95
        };
        s.polyline(
            &[
                [d[0] * r * inner, d[1] * r * inner, z],
                [d[0] * r, d[1] * r, z],
            ],
            [150, 155, 165, 255],
            1.5,
        );
    }
    s.polyline(
        &[[0.0, 0.0, z], [0.0, r * 0.75, z]],
        [90, 200, 90, 255],
        3.0,
    );
    s.polyline(
        &[[0.0, 0.0, z], [r * 0.75, 0.0, z]],
        [230, 80, 70, 255],
        3.0,
    );
}

/// The texts over the view: the compass's angles, the points and wire
/// points (with their set), what is picked, how to move around.
fn labels(
    painter: &egui::Painter,
    rect: egui::Rect,
    projector: &impl Fn([f64; 3]) -> Option<([f64; 2], f64)>,
    g: &Geometry,
    foot: f64,
    ahead: f64,
    selected: &[Handle],
) {
    // Labels outside the dots, away from the middle of the view.
    let screen = |p: [f64; 3]| {
        projector(p).map(|(q, _)| {
            let at = rect.min + egui::vec2(q[0] as f32, q[1] as f32);
            if at.x < rect.center().x {
                (at + egui::vec2(-7.0, -7.0), egui::Align2::RIGHT_BOTTOM)
            } else {
                (at + egui::vec2(7.0, -7.0), egui::Align2::LEFT_BOTTOM)
            }
        })
    };
    let font = egui::FontId::proportional(12.0);
    let rgb = |c: [u8; 4]| egui::Color32::from_rgb(c[0], c[1], c[2]);
    let r = compass_radius(g) * 1.12;
    for a in [0.0, 90.0, 180.0, 270.0] {
        let d = compass_direction(a, ahead);
        let word = match ((a - ahead).rem_euclid(360.0)).round() as i64 {
            0 => " ahead",
            90 => " right",
            180 => " back",
            270 => " left",
            _ => "",
        };
        if let Some((q, _)) = projector([d[0] * r, d[1] * r, foot]) {
            painter.text(
                rect.min + egui::vec2(q[0] as f32, q[1] as f32),
                egui::Align2::CENTER_CENTER,
                format!("{a:.0}°{word}"),
                egui::FontId::proportional(11.0),
                egui::Color32::from_rgb(200, 200, 200),
            );
        }
    }
    for (l, p) in &g.points {
        if let Some((q, align)) = screen(*p) {
            painter.text(q, align, l, font.clone(), rgb(POINT));
        }
    }
    for (l, p, set) in &g.wires {
        if let Some((q, align)) = screen(*p) {
            let text = set.map_or(l.clone(), |s| format!("{l} (set {s})"));
            painter.text(q, align, text, font.clone(), rgb(WIRE));
        }
    }
    for h in selected {
        if let (Handle::Joint(id) | Handle::Member(id), Some(p)) = (h, position(g, h))
            && let Some((q, align)) = screen(p)
        {
            painter.text(q, align, id, font.clone(), rgb(PICKED));
        }
    }
    painter.text(
        rect.left_bottom() + egui::vec2(8.0, -8.0),
        egui::Align2::LEFT_BOTTOM,
        "x right of the line (red), y ahead (green), z up (blue) — drag: turn, right-drag: pan, \
         wheel: zoom; drag a joint or point to move it; Shift + click: pick another",
        egui::FontId::proportional(11.0),
        egui::Color32::GRAY,
    );
}
