// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The structure in 3D, in its own axes: joints, members, points, wire
//! points and strings, the ground at the feet with a compass. A click
//! picks (Shift + click adds), a drag from a joint or point moves it
//! (snapped to a grid or onto a joint: one undo step), elsewhere it turns
//! the view. Attach joints are ringed in orange, support joints marked
//! with a green triangle; the rigid links (a point at an offset from its
//! joint) are drawn as solid magenta rods, picked like their point. With
//! the Link tool ([`crate::link_tool`]) a click chooses the point, then
//! its joint; a right-click on a point opens its menu (Link…, Unlink).

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
/// An attach joint's ring.
const ATTACH: [u8; 4] = [255, 170, 40, 255];
/// A rigid link: from a joint to an attachment point at an offset.
pub const RIGID: [u8; 4] = [230, 80, 220, 255];

/// What the view asks of the application (taken after it is drawn).
#[derive(Debug, Clone, PartialEq)]
pub enum ViewAction {
    /// A click with the Link tool: on this (a point, or a joint the point
    /// may be linked to; none: nothing there).
    LinkClick(Option<Handle>),
    /// A right-click with the Link tool: cancel.
    Cancel,
    /// A point's menu: link, unlink.
    Link(crate::link_tool::LinkAction),
    /// A click with an acting tool ([`crate::tools::Tool`]) on this (none:
    /// nothing there), and where it meets the plane facing the camera
    /// through the point looked at.
    ToolClick(Option<Handle>, Option<[f64; 3]>),
}

/// A standard view of the 3D view's buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standard {
    /// Looking along +y (ahead), parallel projection.
    Front,
    /// Looking along -x (from the right of the line), parallel projection.
    Side,
    /// From above, +y up on the screen, parallel projection.
    Top,
    /// From the back left, above (the view a file opens with), in
    /// perspective.
    Iso,
}

impl Standard {
    /// Every standard view, in the order of the buttons: with the name and
    /// what it shows.
    pub const ALL: [(Standard, &'static str, &'static str); 4] = [
        (
            Standard::Front,
            "Front",
            "Looking along +y (ahead), parallel projection",
        ),
        (
            Standard::Side,
            "Side",
            "Looking along -x (from the right of the line), parallel projection",
        ),
        (
            Standard::Top,
            "Top",
            "From above, ahead (+y) up, parallel projection",
        ),
        (
            Standard::Iso,
            "Iso",
            "From the back left and above, in perspective",
        ),
    ];

    /// `camera` turned to this view (its target and distance kept).
    pub fn apply(self, camera: &mut Camera) {
        let (yaw, pitch, ortho): (f64, f64, bool) = match self {
            Standard::Front => (-90.0, 0.0, true),
            Standard::Side => (0.0, 0.0, true),
            Standard::Top => (-90.0, 89.9, true),
            Standard::Iso => (-135.0, 35.0, false),
        };
        camera.yaw = yaw.to_radians();
        camera.pitch = pitch.to_radians();
        camera.ortho = ortho;
    }
}

/// How to move around in the view and what the colours mean (the "?"
/// button's hover text).
pub const CONTROLS: &str = "Drag: turn the view. Right-drag: pan. Wheel: zoom.\n\
     Click: select (Shift + click: select another, or drop it); with a tool of the hotbar \
     held, a click uses it.\n\
     Move tool: drag a joint or point to move it (snapped to the grid, or onto a joint; one \
     undo step).\n\
     Hotbar: keys 1-9 hold a tool, E: the inventory, Escape: let go of the tool.\n\
     Right-click a point: link, unlink.\n\
     Axes: x right of the line (red), y ahead (green), z up (blue).";

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
    /// The point whose menu is open (right-clicked).
    menu_point: Option<String>,
    /// Where it was drawn last and its camera (the hotbar's place;
    /// `View::screen_of` in the tests).
    last: Option<(egui::Rect, Camera)>,
    /// The joint under the pointer with the Link tool and a point chosen.
    pub link_hover: Option<String>,
    /// A standard view asked for (a button), turned to on the next frame.
    pub standard: Option<Standard>,
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
    /// A panel's drawing over the structure (loads, results) and its key:
    /// the scene only when the key is not the one drawn
    /// ([`View::overlay_key`]; none: keep it).
    pub overlay: Option<(u64, Option<Scene>)>,
    /// What a generator would build: lines drawn over the structure.
    pub preview: &'a [[[f64; 3]; 2]],
    /// The Link tool, when held.
    pub link: Option<&'a mut crate::link_tool::LinkTool>,
    /// The joints a point may be linked to: id, position, attach joint
    /// already ([`crate::link_tool::linkable`]).
    pub linkable: &'a [(String, [f64; 3], bool)],
    /// The moment of each point's offset in the case the results show:
    /// label, in words.
    pub moments: &'a [(String, String)],
    /// The volumes on the strings to draw (none: not shown).
    pub volumes: &'a [ohl_structure_link::VolumeMesh],
    /// What the view asks of the application.
    pub action: &'a mut Option<ViewAction>,
    /// The members are coloured by usage (the results): the legend shown.
    pub usage_legend: bool,
    /// The tool held ([`crate::tools`]): a joint or point moves only with
    /// Move; a click acts with an acting tool.
    pub tool: crate::tools::Tool,
}

impl View {
    /// The key of the overlay drawn now (its scene is not built again
    /// while the key stays).
    pub fn overlay_key(&self) -> Option<u64> {
        self.overlay.as_ref().map(|o| o.0)
    }

    /// Draws the scene again on the next frame.
    pub fn redraw(&mut self) {
        self.dirty = true;
    }

    /// The whole structure in view on the next frame.
    pub fn fit(&mut self) {
        self.camera = None;
    }

    /// Where the view was drawn last.
    pub fn rect(&self) -> Option<egui::Rect> {
        self.last.map(|l| l.0)
    }

    /// The camera (tests).
    #[cfg(test)]
    pub fn camera(&self) -> Option<Camera> {
        self.camera
    }

    /// Where `p` (local axes) was drawn on the screen last (tests).
    #[cfg(test)]
    pub fn screen_of(&self, p: [f64; 3]) -> Option<egui::Pos2> {
        let (rect, camera) = self.last?;
        let size = [f64::from(rect.width()), f64::from(rect.height())];
        camera.projector(size)(p).map(|(q, _)| rect.min + egui::vec2(q[0] as f32, q[1] as f32))
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
        if let Some(s) = self.standard.take() {
            s.apply(camera);
        }
        let size = [f64::from(rect.width()), f64::from(rect.height())];
        let local = |p: egui::Pos2| [f64::from(p.x - rect.min.x), f64::from(p.y - rect.min.y)];
        let cam = *camera;
        let projector = cam.projector(size);
        let project = |p: [f64; 3]| projector(p).map(|(q, _)| q);
        let linking = v.link.as_ref().map(|l| l.point.clone());
        // The joint under the pointer while a point is chosen (any but a
        // support joint; only an attach joint takes the link).
        let joint_at = |at: [f64; 2]| -> Option<(String, [f64; 3], bool)> {
            v.linkable
                .iter()
                .filter_map(|(id, p, attach)| {
                    let q = project(*p)?;
                    let d = (q[0] - at[0]).hypot(q[1] - at[1]);
                    (d <= 12.0).then_some((d, id.clone(), *p, *attach))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, id, p, attach)| (id, p, attach))
        };
        let hover = response
            .hover_pos()
            .filter(|_| matches!(linking, Some(Some(_))))
            .and_then(|p| joint_at(local(p)));
        self.link_hover = hover.as_ref().map(|h| h.0.clone());
        if linking.is_some() {
            // The Link tool: a click chooses the point, then its joint; a
            // right-click cancels.
            if response.secondary_clicked() {
                *v.action = Some(ViewAction::Cancel);
            } else if response.clicked()
                && let Some(pos) = response.interact_pointer_pos()
            {
                let at = local(pos);
                let picked = match pick(g, &project, at, 12.0) {
                    Some(Handle::Point(l)) => Some(Handle::Point(l)),
                    _ if matches!(linking, Some(Some(_))) => {
                        joint_at(at).map(|(id, _, _)| Handle::Joint(id))
                    }
                    other => other,
                };
                *v.action = Some(ViewAction::LinkClick(picked));
                self.dirty = true;
            }
        } else if response.secondary_clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            // A point's menu.
            self.menu_point = match pick(g, &project, local(pos), 12.0) {
                Some(Handle::Point(l)) => Some(l),
                _ => None,
            };
        }
        if linking.is_none()
            && let Some(l) = self.menu_point.clone()
        {
            response.context_menu(|ui| {
                ui.label(egui::RichText::new(format!("point {l}")).strong());
                if ui
                    .button("Link to a joint…")
                    .on_hover_text("The Link tool on this point: click the joint to link it to")
                    .clicked()
                {
                    *v.action = Some(ViewAction::Link(crate::link_tool::LinkAction::Start(
                        l.clone(),
                    )));
                    ui.close();
                }
                if ui
                    .button("Unlink")
                    .on_hover_text(
                        "It keeps its place, a joint of its own on no member (its load then \
                         reaches no member: link it again for the analysis)",
                    )
                    .clicked()
                {
                    *v.action = Some(ViewAction::Link(crate::link_tool::LinkAction::Unlink(
                        l.clone(),
                    )));
                    ui.close();
                }
            });
        }
        // An acting tool: a click acts (on what is under the pointer, or
        // where it meets the plane facing the camera through the target).
        use crate::tools::Tool;
        let acting = !matches!(
            v.tool,
            Tool::Select | Tool::Move | Tool::Symmetry | Tool::Link
        );
        if linking.is_none()
            && acting
            && response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let here = local(pos);
            let picked = pick(g, &project, here, 12.0);
            let at = cam.ray(here, size).and_then(|(a, d)| {
                let eye = cam.eye();
                let n = [0, 1, 2].map(|k| eye[k] - cam.target[k]);
                let dn: f64 = (0..3).map(|k| d[k] * n[k]).sum();
                (dn.abs() > 1e-12).then(|| {
                    let s: f64 = (0..3).map(|k| (cam.target[k] - a[k]) * n[k]).sum::<f64>() / dn;
                    [0, 1, 2].map(|k| a[k] + s * d[k])
                })
            });
            *v.action = Some(ViewAction::ToolClick(picked, at));
            self.dirty = true;
        }
        // Picking: a click picks, Shift + click adds.
        if linking.is_none()
            && !acting
            && response.clicked()
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
        // With the Move tool, a drag from a joint or point moves it;
        // elsewhere (and with any other tool) it turns the view.
        if linking.is_none()
            && v.tool == Tool::Move
            && response.drag_started_by(egui::PointerButton::Primary)
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
                &scene(g, v.foot, v.selected, v.ahead, v.preview, v.volumes),
            ));
            self.dirty = false;
        }
        let camera = *self.camera.as_ref().expect("set above");
        self.last = Some((rect, camera));
        let renderer = self.renderer.get_or_insert_with(|| Renderer::new(device));
        let ppp = ui.ctx().pixels_per_point();
        let px = [
            (rect.width() * ppp).round().max(1.0) as u32,
            (rect.height() * ppp).round().max(1.0) as u32,
        ];
        match v.overlay {
            Some((key, Some(scene))) if self.overlay.as_ref().is_none_or(|o| o.0 != key) => {
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
        marks(
            &painter,
            rect,
            &camera.projector(size),
            g,
            v.selected,
            v.moments,
        );
        if let Some(Some(point)) = &linking {
            link_marks(
                &painter,
                rect,
                &camera.projector(size),
                g,
                v.linkable,
                point,
                hover.as_ref(),
            );
        }
        if v.usage_legend {
            usage_legend(&painter, rect);
        }
        self.buttons(ui, rect);
    }

    /// The buttons over the view's top left: the standard views, Fit, and
    /// "?" (the controls on hover).
    fn buttons(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        let bar = egui::Rect::from_min_size(
            rect.min + egui::vec2(6.0, 6.0),
            egui::vec2((rect.width() - 12.0).max(0.0), 28.0),
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(bar)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                for (s, name, tip) in Standard::ALL {
                    if ui.small_button(name).on_hover_text(tip).clicked() {
                        self.standard = Some(s);
                        ui.ctx().request_repaint();
                    }
                }
                if ui
                    .small_button("Fit")
                    .on_hover_text("Show the whole structure (the view a file opens with)")
                    .clicked()
                {
                    self.fit();
                    ui.ctx().request_repaint();
                }
                ui.small_button("?").on_hover_text(CONTROLS);
            },
        );
    }
}

/// The colours of the members coloured by usage (the results), at the
/// view's bottom right.
fn usage_legend(painter: &egui::Painter, rect: egui::Rect) {
    let near = crate::results_tab::NEAR_FULL * 100.0;
    let rows = [
        (Some(1.5), "over 100 %".to_owned()),
        (Some(0.95), format!("{near:.0} to 100 %")),
        (Some(0.5), format!("under {near:.0} %")),
        (None, "no capacity".to_owned()),
    ];
    let font = egui::FontId::proportional(12.0);
    let line = 16.0;
    let size = egui::vec2(150.0, line * (rows.len() as f32 + 1.0) + 8.0);
    let at = rect.right_bottom() - size - egui::vec2(8.0, 8.0);
    painter.rect_filled(
        egui::Rect::from_min_size(at, size),
        3.0,
        egui::Color32::from_black_alpha(190),
    );
    painter.text(
        at + egui::vec2(6.0, 4.0),
        egui::Align2::LEFT_TOP,
        "Members by usage",
        font.clone(),
        egui::Color32::WHITE,
    );
    for (k, (u, text)) in rows.iter().enumerate() {
        let y = at.y + 4.0 + line * (k as f32 + 1.0) + line / 2.0;
        let c = crate::results_tab::usage_colour(*u);
        painter.line_segment(
            [egui::pos2(at.x + 8.0, y), egui::pos2(at.x + 28.0, y)],
            egui::Stroke::new(3.0, egui::Color32::from_rgb(c[0], c[1], c[2])),
        );
        painter.text(
            egui::pos2(at.x + 34.0, y),
            egui::Align2::LEFT_CENTER,
            text,
            font.clone(),
            egui::Color32::WHITE,
        );
    }
}

/// Over the view: attach joints ringed, support joints marked with a
/// triangle; the rigid link of the point picked described (its joint,
/// offset, length, the moment in the case the results show).
fn marks(
    painter: &egui::Painter,
    rect: egui::Rect,
    projector: &impl Fn([f64; 3]) -> Option<([f64; 2], f64)>,
    g: &Geometry,
    selected: &[Handle],
    moments: &[(String, String)],
) {
    let at =
        |p: [f64; 3]| projector(p).map(|(q, _)| rect.min + egui::vec2(q[0] as f32, q[1] as f32));
    let rgb = |c: [u8; 4]| egui::Color32::from_rgb(c[0], c[1], c[2]);
    for (id, p, support) in &g.joints {
        let Some(q) = at(*p) else { continue };
        if g.attach.contains(id) {
            painter.circle_stroke(q, 7.0, egui::Stroke::new(2.0, rgb(ATTACH)));
        } else if *support {
            let r = 7.0;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    q + egui::vec2(0.0, 1.0),
                    q + egui::vec2(-r, r + 4.0),
                    q + egui::vec2(r, r + 4.0),
                ],
                egui::Color32::TRANSPARENT,
                egui::Stroke::new(2.0, rgb(SUPPORT)),
            ));
        }
    }
    for h in selected {
        let Handle::Point(l) = h else { continue };
        let Some(link) = g.links.iter().find(|x| &x.label == l) else {
            continue;
        };
        let Some(q) = at(link.to) else { continue };
        let o = link.offset();
        let mut text = format!(
            "rigid link {l} — joint {}, offset ({:.3}, {:.3}, {:.3}) m, {:.3} m",
            link.joint,
            o[0],
            o[1],
            o[2],
            link.length()
        );
        for (_, m) in moments.iter().filter(|m| &m.0 == l) {
            text.push_str(&format!("\n{m}"));
        }
        note(painter, q + egui::vec2(12.0, 12.0), text);
    }
}

/// The Link tool with a point chosen: the attach joints ringed (only
/// they take a link), a line to the one under the pointer with the
/// offset's length and moment arm; another joint under the pointer says
/// why it takes none.
fn link_marks(
    painter: &egui::Painter,
    rect: egui::Rect,
    projector: &impl Fn([f64; 3]) -> Option<([f64; 2], f64)>,
    g: &Geometry,
    linkable: &[(String, [f64; 3], bool)],
    point: &str,
    hover: Option<&(String, [f64; 3], bool)>,
) {
    let at =
        |p: [f64; 3]| projector(p).map(|(q, _)| rect.min + egui::vec2(q[0] as f32, q[1] as f32));
    let green = egui::Color32::from_rgb(120, 240, 140);
    for (_, p, _) in linkable.iter().filter(|j| j.2) {
        if let Some(q) = at(*p) {
            painter.circle_stroke(q, 10.0, egui::Stroke::new(2.0, green));
        }
    }
    let Some(from) = g.points.iter().find(|p| p.0 == point).map(|p| p.1) else {
        return;
    };
    let Some((id, j, attach)) = hover else { return };
    if !attach {
        if let Some(a) = at(*j) {
            note(
                painter,
                a + egui::vec2(14.0, -24.0),
                format!("joint {id}: {}", ohl_structures::NOT_ATTACH),
            );
        }
        return;
    }
    if let (Some(a), Some(b)) = (at(*j), at(from)) {
        painter.circle_stroke(a, 13.0, egui::Stroke::new(3.0, egui::Color32::WHITE));
        painter.line_segment([a, b], egui::Stroke::new(2.0, egui::Color32::WHITE));
        let o = [0, 1, 2].map(|k| from[k] - j[k]);
        note(
            painter,
            a + egui::vec2(14.0, -24.0),
            format!(
                "link {point} to joint {id}: {}",
                crate::link_tool::offset_words(o)
            ),
        );
    }
}

/// Text on a dark box, kept inside the view.
fn note(painter: &egui::Painter, at: egui::Pos2, text: String) {
    let galley =
        painter.layout_no_wrap(text, egui::FontId::proportional(13.0), egui::Color32::WHITE);
    let clip = painter.clip_rect().shrink(4.0);
    let size = galley.size();
    let at = egui::pos2(
        at.x.min(clip.right() - size.x).max(clip.left()),
        at.y.min(clip.bottom() - size.y).max(clip.top()),
    );
    painter.rect_filled(
        egui::Rect::from_min_size(at, galley.size()).expand(3.0),
        3.0,
        egui::Color32::from_black_alpha(190),
    );
    painter.galley(at, galley, egui::Color32::WHITE);
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
    volumes: &[ohl_structure_link::VolumeMesh],
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
    // The volumes on the strings, filled, in the line's colours.
    for m in volumes {
        let c = match m.state {
            ohl_structures::ElectricalState::Grounded => [150, 150, 155, 255],
            ohl_structures::ElectricalState::Electrified => [240, 150, 60, 255],
            ohl_structures::ElectricalState::Insulated => [225, 220, 180, 255],
        };
        for t in &m.triangles {
            s.triangle(*t, [c; 3]);
        }
    }
    // The rigid links: solid rods from the joint to the point.
    for l in &g.links {
        let c = if picked(Handle::Point(l.label.clone())) {
            PICKED
        } else {
            RIGID
        };
        s.polyline(&[l.from, l.to], c, 4.0);
    }
    for l in preview {
        s.polyline(l, PREVIEW, 2.2);
    }
    for (id, p, support) in &g.joints {
        let (c, size) = match (picked(Handle::Joint(id.clone())), support) {
            (true, _) => (PICKED, 11.0),
            _ if g.attach.contains(id) => (ATTACH, 8.0),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The volumes on the strings are drawn as triangles of the scene, as
    /// many as their meshes hold.
    #[test]
    fn the_volumes_are_triangles_of_the_scene() {
        let g = Geometry::default();
        let tri = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let v = vec![ohl_structure_link::VolumeMesh {
            string: "S1".into(),
            name: "horn".into(),
            state: ohl_structures::ElectricalState::Electrified,
            triangles: vec![tri; 5],
        }];
        let without = scene(&g, 0.0, &[], 0.0, &[], &[]).counts().2;
        let with = scene(&g, 0.0, &[], 0.0, &[], &v).counts().2;
        assert_eq!(with - without, 5);
    }
}
