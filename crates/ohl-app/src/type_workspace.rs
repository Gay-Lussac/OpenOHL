// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The structure screen of the line application (the Edit tool on a
//! structure, Strings and points… in Properties): a structure type
//! (`.ohls`) in its own axes (x right of the line, y ahead, z up, 0;0 at
//! the centre) — the tables on the left, its 3D view on the right (the
//! divider dragged, its place kept for the session). Only the basics are
//! edited here: the attachment points, the strings (insulators) and the
//! wire points with their sets. The body (joints, members, sections,
//! materials, supports, generators, pole, extensions, load cases) is
//! drawn for context and edited in the structure application
//! (`ohl-structure`): **Edit in the structure application…**. Points and
//! wire points are picked and dragged in the view (snapped to a grid or
//! to a joint); strings come from presets. Every change is made in the
//! file as edited (the library page saves it, and every structure of the
//! type follows).

use eframe::egui;
use eframe::egui_wgpu;
use ohl_render::{Bounds, Camera, GpuScene, Renderer, Scene};
use ohl_structure_edit::{self as edit, StringChange, compass_radius, pick, polar, position};
use ohl_structures::{InsulatorLibrary, LibraryItem, StructureType};
use toml_edit::DocumentMut;

/// The edits are those of the structure application
/// (`ohl-structure-edit`): one implementation for both.
pub use ohl_structure_edit::{Geometry, Handle, Removal, geometry};

/// The label of the button opening the structure application.
pub const EDIT_IN_APP: &str = "Open this type in OHL-Structure…";

/// The 3D view's name (for screen readers and tests).
pub const VIEW_LABEL: &str = "Pylon in 3D";

/// The table shown left of the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sheet {
    /// Attachment points of the structure (where insulators hang).
    #[default]
    Points,
    /// Insulators (strings).
    Strings,
    /// Wire attachment points (where a string ends).
    Wires,
    /// Set points (where a wire is strung).
    Sets,
}

/// What the workspace asks of the application.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// Save an insulator assembly (`.ohli` text) into the insulator library.
    SaveInsulator {
        /// Its id.
        id: String,
        /// The file's text.
        text: String,
    },
    /// Open the insulator library (to import, export, edit assemblies).
    OpenInsulators,
    /// Open the structure in the structure application (its body, the
    /// design: what this screen does not edit).
    EditInStructureApp,
}

/// A drag moving a joint or a (wire) point.
#[derive(Debug, Clone)]
struct Move {
    handle: Handle,
    /// A point of the plane it moves in (facing the camera).
    on: [f64; 3],
    /// The plane's normal.
    normal: [f64; 3],
}

/// State of the workspace.
pub struct Workspace {
    /// Shown instead of the fields of the file.
    pub on: bool,
    /// The type as last read (kept while the file as edited does not read).
    shown: Option<StructureType>,
    /// The text `shown` was read from.
    read_from: String,
    camera: Option<Camera>,
    renderer: Option<Renderer>,
    gpu: Option<GpuScene>,
    texture: Option<egui::TextureId>,
    /// The scene needs building.
    dirty: bool,
    /// Picked, in order (two joints: a member between them).
    pub selected: Vec<Handle>,
    drag: Option<Move>,
    /// The table shown.
    pub sheet: Sheet,
    /// Grid a dragged joint or point snaps to, m (0: none).
    pub grid: f64,
    /// Length of a new string (presets), m.
    pub string_length: f64,
    /// A deletion waiting for its confirmation.
    pub confirm: Option<Removal>,
    /// The angle that means ahead (the compass), as the project or the
    /// user counts angles.
    pub ahead: f64,
    /// What the last action did (or why not: `true`).
    pub message: Option<(String, bool)>,
    /// What it asks of the application (taken by it).
    pub request: Option<Request>,
    /// Its own **Edit in the structure application…** button (a structure
    /// without a file has its own: make it a file first).
    pub app_button: bool,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            on: false,
            shown: None,
            read_from: String::new(),
            camera: None,
            renderer: None,
            gpu: None,
            texture: None,
            dirty: true,
            selected: Vec::new(),
            drag: None,
            sheet: Sheet::default(),
            grid: 0.05,
            string_length: 4.0,
            confirm: None,
            message: None,
            ahead: 0.0,
            request: None,
            app_button: true,
        }
    }
}

const STEEL: [u8; 4] = [215, 215, 220, 255];
const BARE: [u8; 4] = [200, 170, 110, 255];
const STICK: [u8; 4] = [150, 150, 160, 255];
const STRING: [u8; 4] = [120, 180, 255, 255];
const POINT: [u8; 4] = [255, 150, 60, 255];
const WIRE: [u8; 4] = [90, 200, 255, 255];
const PICKED: [u8; 4] = [255, 235, 60, 255];

impl Workspace {
    /// Reads the file as edited again when it changed (what is drawn
    /// follows).
    fn refresh(&mut self, doc: &DocumentMut) {
        let text = doc.to_string();
        if text == self.read_from && self.shown.is_some() {
            return;
        }
        if let Ok(t) = StructureType::parse(std::path::Path::new("type.ohls"), &text) {
            self.shown = Some(t);
        }
        self.read_from = text;
        self.dirty = true;
    }

    /// Draws its scene again on the next frame.
    pub fn redraw(&mut self) {
        self.dirty = true;
    }

    /// The type as last read.
    pub fn shown(&self) -> Option<&StructureType> {
        self.shown.as_ref()
    }

    fn scene(&self, g: &Geometry, foot: f64) -> Scene {
        let mut s = Scene::new([0.0; 3]);
        // The ground at the feet, a 1 m grid; the axes at the origin.
        let extent = g
            .joints
            .iter()
            .map(|j| j.1)
            .chain(g.points.iter().map(|p| p.1))
            .chain(g.wires.iter().map(|w| w.1))
            .chain(g.sticks.iter().flatten().copied())
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
        // The compass on the ground at the feet: the angles as counted.
        crate::editors::draw_compass(&mut s, compass_radius(g), self.ahead, None, foot);
        for l in &g.sticks {
            s.polyline(l, STICK, 1.5);
        }
        let picked = |h: Handle| self.selected.contains(&h);
        for (id, a, b, section) in &g.members {
            let c = if picked(Handle::Member(id.clone())) {
                PICKED
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
        for (id, p, support) in &g.joints {
            let (c, size) = match (picked(Handle::Joint(id.clone())), support) {
                (true, _) => (PICKED, 11.0),
                (false, true) => ([110, 220, 120, 255], 9.0),
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

    /// The structure screen: its level and what it holds, Edit in the
    /// structure application…, the tools; the tables on the left, the view
    /// on the right. `doc` is the file as edited; `true` when it was
    /// changed. `note`: the structures of the line with this type.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        doc: &mut DocumentMut,
        rs: Option<&egui_wgpu::RenderState>,
        insulators: &InsulatorLibrary,
        note: &str,
    ) -> bool {
        self.refresh(doc);
        let Some(t) = self.shown.clone() else {
            ui.label("The file does not read yet: see the error above (Fields to correct it).");
            return false;
        };
        let mut changed = false;
        // The level, worked out from the content; the basics it holds; the
        // body (drawn only) and the structure application for it.
        ui.horizontal_wrapped(|ui| {
            let d = t.detail();
            ui.strong(format!("Level {} — {}", d.level(), d.name()));
            ui.separator();
            let sets = t.wires.iter().filter(|w| w.set.is_some()).count();
            ui.label(format!(
                "{} attachment point(s), {} string(s), {} wire point(s), {} set(s)",
                t.points.len(),
                t.strings.len(),
                t.wires.len(),
                sets
            ));
            ui.separator();
            ui.label(note);
            ui.separator();
            if self.app_button
                && ui
                    .button(EDIT_IN_APP)
                    .on_hover_text(
                        "The body (joints, members, sections, materials, supports, generators, \
                         pole, extensions) and its design (load cases, analysis) are edited \
                         in OHL-Structure, a window of its own; saved there, the \
                         line follows",
                    )
                    .clicked()
            {
                self.request = Some(Request::EditInStructureApp);
            }
        });
        if !t.members.is_empty() || !t.nodes.is_empty() {
            ui.label(
                egui::RichText::new(format!(
                    "Body: {} joint(s), {} member(s) — drawn here, edited in the structure \
                     application",
                    t.nodes.len(),
                    t.members.len()
                ))
                .small()
                .weak(),
            );
        }
        changed |= self.tools(ui, doc, &t);
        if let Some((m, error)) = &self.message {
            let text = egui::RichText::new(m).small();
            if *error {
                crate::menu::error_label(ui, text);
            } else {
                ui.colored_label(crate::menu::ok_color(ui), text);
            }
        }
        changed |= self.confirm_ui(ui, doc, &t);
        let g = geometry(&t, insulators);
        // Split: the tables left, the view right (the divider is dragged;
        // egui keeps its place for the session).
        let width = ui.available_width();
        egui::Panel::left(egui::Id::new("type-workspace-tables"))
            .resizable(true)
            .default_size((width * 0.45).max(280.0))
            .min_size(220.0)
            .show(ui, |ui| {
                changed |= self.tables(ui, doc, &t, &g, insulators);
            });
        egui::CentralPanel::default().show(ui, |ui| {
            changed |= self.view(ui, doc, rs, &t, &g);
        });
        if changed {
            self.refresh(doc);
        }
        changed
    }

    fn tools(&mut self, ui: &mut egui::Ui, doc: &mut DocumentMut, t: &StructureType) -> bool {
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.label("snap");
            ui.add(
                egui::DragValue::new(&mut self.grid)
                    .range(0.0..=5.0)
                    .speed(0.01)
                    .max_decimals(3)
                    .suffix(" m"),
            )
            .on_hover_text("A dragged point snaps to this grid (0: none), or onto a joint");
            ui.separator();
            if ui
                .button("+ Attachment point")
                .on_hover_text(
                    "A point where strings hang, beside the point picked (or at the top of the \
                     centre); drag it in the view or type where it is",
                )
                .clicked()
            {
                let at = match self.selected.last() {
                    Some(Handle::Point(l)) => t
                        .points
                        .iter()
                        .find(|p| &p.label == l)
                        .map(|p| p.position().map(|v| v.si()))
                        .map(|p| [-p[0], p[1], p[2]]),
                    _ => None,
                }
                .unwrap_or_else(|| {
                    let top = t
                        .points
                        .iter()
                        .map(|p| p.position()[2].si())
                        .fold(f64::NAN, f64::max);
                    [0.0, 0.0, if top.is_finite() { top } else { 20.0 }]
                });
                let l = edit::add_point(doc, t, at);
                self.selected = vec![Handle::Point(l.clone())];
                self.message = Some((format!("added attachment point {l}"), false));
                changed = true;
            }
            ui.separator();
            let tops: Vec<String> = self
                .selected
                .iter()
                .filter_map(|h| match h {
                    Handle::Point(l) | Handle::Wire(l) => Some(l.clone()),
                    _ => None,
                })
                .collect();
            ui.label("string");
            ui.add(
                egui::DragValue::new(&mut self.string_length)
                    .range(0.05..=50.0)
                    .speed(0.05)
                    .max_decimals(3)
                    .suffix(" m"),
            );
            for (name, n, help) in [
                ("I", 1, "An insulator from the point picked down to a new wire point (its set: Set points)"),
                ("V", 2, "A V: two strings from the two points picked to one wire point below their middle"),
            ] {
                if ui
                    .add_enabled(tops.len() == n, egui::Button::new(name))
                    .on_hover_text(help)
                    .on_disabled_hover_text(if n == 1 {
                        "Pick one point (or a wire point: a string below it)"
                    } else {
                        "Pick two points (Shift + click the second)"
                    })
                    .clicked()
                {
                    match edit::add_strings(doc, t, &tops, self.string_length) {
                        Ok(w) => {
                            self.message = Some((
                                format!("added the insulators to wire point {w}: put its set in Set points"),
                                false,
                            ));
                            self.selected = vec![Handle::Wire(w)];
                            changed = true;
                        }
                        Err(e) => self.message = Some((e, true)),
                    }
                }
            }
            ui.separator();
            if ui
                .add_enabled(self.selected.len() == 1, egui::Button::new("Delete…"))
                .on_hover_text("Delete what is picked, with what goes with it (asks first)")
                .clicked()
            {
                self.confirm = Some(edit::removal(t, &self.selected[0]));
            }
            if ui.button("Fit").on_hover_text("Show the whole pylon").clicked() {
                self.camera = None;
            }
            if !self.selected.is_empty() {
                ui.label(
                    egui::RichText::new(
                        self.selected
                            .iter()
                            .map(Handle::describe)
                            .collect::<Vec<_>>()
                            .join(", "),
                    )
                    .color(egui::Color32::from_rgb(255, 235, 60)),
                );
            }
        });
        changed
    }

    fn confirm_ui(&mut self, ui: &mut egui::Ui, doc: &mut DocumentMut, _t: &StructureType) -> bool {
        let Some(r) = self.confirm.clone() else {
            return false;
        };
        let mut changed = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label("Delete, with everything that goes with it:");
            egui::ScrollArea::vertical()
                .max_height(90.0)
                .id_salt("type-workspace-removal")
                .show(ui, |ui| {
                    for l in r.lines() {
                        ui.label(l);
                    }
                });
            ui.horizontal(|ui| {
                if ui.button("Delete").clicked() {
                    edit::remove(doc, &r);
                    self.selected.clear();
                    self.message = Some((format!("deleted {}", r.lines().join("; ")), false));
                    self.confirm = None;
                    changed = true;
                }
                if ui.button("Cancel").clicked() {
                    self.confirm = None;
                }
            });
        });
        changed
    }

    fn view(
        &mut self,
        ui: &mut egui::Ui,
        doc: &mut DocumentMut,
        rs: Option<&egui_wgpu::RenderState>,
        t: &StructureType,
        g: &Geometry,
    ) -> bool {
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, VIEW_LABEL));
        let Some(rs) = rs else {
            ui.painter_at(rect).text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "The pylon's 3D view needs a GPU (wgpu).",
                egui::FontId::proportional(13.0),
                ui.visuals().text_color(),
            );
            return false;
        };
        let foot = t.foot_level();
        let camera = self.camera.get_or_insert_with(|| {
            let mut b: Option<Bounds> = None;
            for p in g
                .joints
                .iter()
                .map(|j| j.1)
                .chain(g.points.iter().map(|p| p.1))
                .chain(g.wires.iter().map(|w| w.1))
                .chain(g.sticks.iter().flatten().copied())
                .chain([[0.0, 0.0, foot]])
            {
                let b = b.get_or_insert(Bounds { min: p, max: p });
                b.min = [0, 1, 2].map(|k| b.min[k].min(p[k]));
                b.max = [0, 1, 2].map(|k| b.max[k].max(p[k]));
            }
            let b = b.expect("the feet at least");
            let mut c = Camera::fit(&b);
            // The whole pylon in a view wider than high.
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
        let mut changed = false;
        // Only the basics are picked: points and wire points (the body is
        // edited in the structure application).
        let basics = Geometry {
            joints: Vec::new(),
            members: Vec::new(),
            ..g.clone()
        };
        // Picking: a click picks, Shift + click adds.
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let shift = ui.input(|i| i.modifiers.shift);
            match pick(&basics, &project, local(pos), 12.0) {
                Some(h) if shift => {
                    if let Some(k) = self.selected.iter().position(|x| *x == h) {
                        self.selected.remove(k);
                    } else {
                        self.selected.push(h);
                    }
                }
                Some(h) => self.selected = vec![h],
                None if !shift => self.selected.clear(),
                None => {}
            }
            self.dirty = true;
        }
        // A drag from a point or wire point moves it (one on a joint moves
        // with the body: not here); elsewhere it turns the view.
        let on_body = |h: &Handle| match h {
            Handle::Point(l) => t
                .points
                .iter()
                .find(|p| &p.label == l)
                .and_then(|p| p.node.as_ref().or(p.member.as_ref()))
                .cloned(),
            _ => None,
        };
        let grabbed = if response.drag_started_by(egui::PointerButton::Primary) {
            response
                .interact_pointer_pos()
                .and_then(|pos| pick(&basics, &project, local(pos), 12.0))
        } else {
            None
        };
        if let Some(h) = &grabbed
            && let Some(on) = on_body(h)
        {
            self.selected = vec![h.clone()];
            self.message = Some((
                format!(
                    "{} sits on {on} of the body: it moves with the body (Edit in the structure \
                     application…)",
                    h.describe()
                ),
                true,
            ));
        } else if let Some(h) = grabbed
            && let Some(at) = position(g, &h)
        {
            let eye = cam.eye();
            let n = [0, 1, 2].map(|k| eye[k] - cam.target[k]);
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
            self.selected = vec![h.clone()];
            self.drag = Some(Move {
                handle: h,
                on: at,
                normal: n.map(|v| v / len),
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
                    if self.grid > 0.0 {
                        at = at.map(|v| (v / self.grid).round() * self.grid);
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
                        match edit::move_to(doc, t, &m.handle, at) {
                            Ok(()) => {
                                changed = true;
                                self.message = Some((
                                    format!(
                                        "{} at {:.3}, {:.3}, {:.3} m",
                                        m.handle.describe(),
                                        at[0],
                                        at[1],
                                        at[2]
                                    ),
                                    false,
                                ));
                            }
                            Err(e) => {
                                self.message = Some((e, true));
                                self.drag = None;
                            }
                        }
                    }
                }
            }
            if !response.dragged() {
                self.drag = None;
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
        if self.dirty || changed || self.gpu.is_none() {
            self.gpu = Some(GpuScene::new(device, &self.scene(g, foot)));
            self.dirty = false;
        }
        let camera = *self.camera.as_ref().expect("set above");
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
        // Labels: points and wire points (with their set), what is picked.
        let projector = camera.projector(size);
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
        for (p, text, color) in
            crate::editors::compass_texts(compass_radius(g), self.ahead, None, foot)
        {
            if let Some((q, _)) = projector(p) {
                painter.text(
                    rect.min + egui::vec2(q[0] as f32, q[1] as f32),
                    egui::Align2::CENTER_CENTER,
                    text,
                    egui::FontId::proportional(11.0),
                    color,
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
        for h in &self.selected {
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
             wheel: zoom; drag a point or wire point to move it; Shift + click: pick another",
            egui::FontId::proportional(11.0),
            egui::Color32::GRAY,
        );
        changed
    }

    fn tables(
        &mut self,
        ui: &mut egui::Ui,
        doc: &mut DocumentMut,
        t: &StructureType,
        g: &Geometry,
        insulators: &InsulatorLibrary,
    ) -> bool {
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            for (s, name, tip) in [
                (
                    Sheet::Points,
                    "Attachment points",
                    "Where insulators hang on the structure: part of the pylon, moved only here",
                ),
                (
                    Sheet::Strings,
                    "Insulators",
                    "The strings: each from an attachment point (or another insulator's wire point) to its wire point, by its length and direction",
                ),
                (
                    Sheet::Wires,
                    "Wire points",
                    "Where an insulator ends: placed by its string(s); another insulator may hang from it, or a set be on it",
                ),
                (
                    Sheet::Sets,
                    "Set points",
                    "Where a wire is strung: a set number on a wire point; suspension or strain from its insulator",
                ),
            ] {
                ui.selectable_value(&mut self.sheet, s, name).on_hover_text(tip);
            }
        });
        let coord = |ui: &mut egui::Ui, v: &mut f64| {
            ui.add(
                egui::DragValue::new(v)
                    .speed(0.01)
                    .max_decimals(3)
                    .suffix(" m"),
            )
            .changed()
        };
        egui::ScrollArea::both()
            .id_salt("type-workspace-sheet")
            .auto_shrink(false)
            .show(ui, |ui| match self.sheet {
                Sheet::Points => {
                    egui::Grid::new("type-points").striped(true).show(ui, |ui| {
                        for h in ["attachment point", "x", "y", "z", "on", "insulators"] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for p in &t.points {
                            let hanging = t.strings.iter().filter(|x| x.top == p.label).count();
                            let h = Handle::Point(p.label.clone());
                            if ui.selectable_label(self.selected.contains(&h), &p.label).clicked() {
                                self.selected = vec![h.clone()];
                                self.dirty = true;
                            }
                            let mut at = p.position().map(|v| v.si());
                            let free = p.node.is_none() && p.member.is_none();
                            let mut moved = false;
                            for v in &mut at {
                                if free {
                                    moved |= coord(ui, v);
                                } else {
                                    ui.label(format!("{v:.3} m"));
                                }
                            }
                            if moved && edit::move_to(doc, t, &h, at).is_ok() {
                                changed = true;
                            }
                            ui.label(match (&p.node, &p.member, p.at) {
                                (Some(n), _, _) => format!("joint {n}"),
                                (_, Some(m), Some(f)) => format!("member {m} at {f}"),
                                _ => "—".into(),
                            });
                            ui.label(hanging.to_string());
                            ui.end_row();
                        }
                    });
                }
                Sheet::Wires => {
                    egui::Grid::new("type-wires").striped(true).show(ui, |ui| {
                        for (h, tip) in [
                            ("wire point", "Its label"),
                            ("x", "Placed by its insulator(s); typed only for one no insulator reaches (fixed)"),
                            ("y", ""),
                            ("z", ""),
                            ("held by", "The insulators ending here (two: a V)"),
                            ("below it", "Insulators hanging from it (a TAK, a V with a string below)"),
                            ("set", "A set on it: a wire is strung here (Set points)"),
                            ("counterweight", "A weight hanging at it"),
                        ] {
                            ui.strong(h).on_hover_text(tip);
                        }
                        ui.end_row();
                        for w in &t.wires {
                            let h = Handle::Wire(w.label.clone());
                            if ui.selectable_label(self.selected.contains(&h), &w.label).clicked() {
                                self.selected = vec![h.clone()];
                                self.dirty = true;
                            }
                            let held: Vec<&str> = t
                                .strings
                                .iter()
                                .filter(|x| x.wire == w.label)
                                .map(|x| x.label.as_str())
                                .collect();
                            let mut at = g
                                .wires
                                .iter()
                                .find(|x| x.0 == w.label)
                                .map_or([0.0; 3], |x| x.1);
                            if held.is_empty() {
                                let mut moved = false;
                                for v in &mut at {
                                    moved |= coord(ui, v);
                                }
                                if moved && edit::move_to(doc, t, &h, at).is_ok() {
                                    changed = true;
                                }
                            } else {
                                for v in at {
                                    ui.label(format!("{v:.3} m"))
                                        .on_hover_text("Placed by its insulator(s): change their length or direction");
                                }
                            }
                            ui.label(if held.is_empty() {
                                "— (fixed)".to_owned()
                            } else {
                                held.join(", ")
                            });
                            let below: Vec<&str> = t
                                .strings
                                .iter()
                                .filter(|x| x.top == w.label)
                                .map(|x| x.label.as_str())
                                .collect();
                            ui.label(below.join(", "));
                            ui.label(w.set.map_or("—".to_owned(), |s| s.to_string()));
                            let mut cw = w.weight.map_or(0.0, |f| f.si());
                            if ui
                                .add(egui::DragValue::new(&mut cw).range(0.0..=1e6).speed(10.0).max_decimals(0).suffix(" N"))
                                .changed()
                            {
                                edit::set_counterweight(doc, &w.label, cw);
                                changed = true;
                            }
                            ui.end_row();
                        }
                    });
                }
                Sheet::Sets => {
                    changed |= self.sets_sheet(ui, doc, t);
                }
                Sheet::Strings => {
                    changed |= self.strings_sheet(ui, doc, t, g, insulators);
                }
            });
        changed
    }
}

impl Workspace {
    /// The Insulators sheet: each insulator's top (an attachment point or
    /// another insulator's wire point), wire point, kind (suspension or
    /// strain), assembly from the library (or a length), its direction (an
    /// angle from straight down and one in plan; a V's legs follow where
    /// they meet), weight, wind area, compression, description — all
    /// editable — and New, From library, Save to library, Delete.
    fn strings_sheet(
        &mut self,
        ui: &mut egui::Ui,
        doc: &mut DocumentMut,
        t: &StructureType,
        g: &Geometry,
        insulators: &InsulatorLibrary,
    ) -> bool {
        let mut changed = false;
        let ahead = self.ahead;
        let at = |label: &str| g.at(label);
        let tops: Vec<&str> = t
            .points
            .iter()
            .map(|p| p.label.as_str())
            .chain(t.wires.iter().map(|w| w.label.as_str()))
            .collect();
        let assemblies: Vec<&ohl_structures::InsulatorAssembly> = insulators.iter().collect();
        // New, from the library: from the point picked (else the first).
        let picked = self
            .selected
            .iter()
            .find_map(|h| match h {
                Handle::Point(l) | Handle::Wire(l) => Some(l.clone()),
                _ => None,
            })
            .or_else(|| t.points.first().map(|p| p.label.clone()));
        ui.horizontal_wrapped(|ui| {
            let from = picked.clone().unwrap_or_default();
            let can = picked.is_some();
            if ui
                .add_enabled(can, egui::Button::new("+ New insulator"))
                .on_hover_text(format!(
                    "From {from} (the point picked), {:.2} m, hanging, to a new wire point",
                    self.string_length
                ))
                .on_disabled_hover_text("Add an attachment point first")
                .clicked()
            {
                match edit::new_string(doc, t, &from, None, self.string_length, None) {
                    Ok((l, w)) => {
                        self.message = Some((format!("insulator {l} from {from} to {w}"), false));
                        self.selected = vec![Handle::Wire(w)];
                        changed = true;
                    }
                    Err(e) => self.message = Some((e, true)),
                }
            }
            let mut chosen: Option<String> = None;
            ui.add_enabled_ui(can && !assemblies.is_empty(), |ui| {
                egui::ComboBox::from_id_salt("insulator-from-library")
                    .selected_text("From the library…")
                    .height(300.0)
                    .show_ui(ui, |ui| {
                        for a in &assemblies {
                            if ui
                                .selectable_label(false, format!("{} — {}", a.id, a.name))
                                .on_hover_text(format!(
                                    "{:?}: {:.3} m, {:.0} N",
                                    a.kind,
                                    a.length.si(),
                                    a.weight.si()
                                ))
                                .clicked()
                            {
                                chosen = Some(a.id.clone());
                            }
                        }
                    })
                    .response
                    .on_hover_text(format!(
                        "An insulator of the library from {from} (the point picked)"
                    ));
            });
            if let Some(id) = chosen
                && let Some(a) = assemblies.iter().find(|a| a.id == id)
            {
                match edit::new_string(doc, t, &from, Some(a), a.length.si(), None) {
                    Ok((l, w)) => {
                        self.message =
                            Some((format!("insulator {l} ({id}) from {from} to {w}"), false));
                        self.selected = vec![Handle::Wire(w)];
                        changed = true;
                    }
                    Err(e) => self.message = Some((e, true)),
                }
            }
            ui.label("length");
            ui.add(
                egui::DragValue::new(&mut self.string_length)
                    .range(0.05..=50.0)
                    .speed(0.05)
                    .max_decimals(3)
                    .suffix(" m"),
            );
            ui.separator();
            if ui
                .button("Insulator library…")
                .on_hover_text("The insulator assemblies: import, export, edit, delete")
                .clicked()
            {
                self.request = Some(Request::OpenInsulators);
            }
        });
        let mut delete: Option<String> = None;
        let mut save: Option<String> = None;
        let mut changes: Vec<(String, StringChange)> = Vec::new();
        egui::Grid::new("type-strings").striped(true).show(ui, |ui| {
            for (h, tip) in [
                ("insulator", "Its label"),
                ("hangs from", "An attachment point of the structure, or another insulator's wire point (a TAK)"),
                ("wire point", "Where it ends: a new one, or another insulator's (two make a V)"),
                ("kind", "Suspension: the wire runs through its set. Strain: the wire ends at its set"),
                ("assembly", "An assembly of the insulator library (its length, weight, wind area), or none: a length"),
                ("length", "From its top to its wire point: it places the wire point"),
                ("angle", "From straight down: 0° hanging, 90° level, 180° up"),
                ("direction", "In plan, clockwise seen from above (ahead at the project's angle)"),
                ("weight", "Its weight (default: its assembly's)"),
                ("wind area", "The area the wind blows on (default: its assembly's)"),
                ("compression", "The push it may take (V legs); none: tension only"),
                ("limits", "Kept, not checked by OpenOHL (for a plugin or a report): the tension it is rated for, the swing from straight down it may take (any direction), across and along the line. Unticked: not given — its assembly's, if it gives one (shown dimmed)"),
                ("description", "What it is, in words"),
                ("", ""),
            ] {
                ui.strong(h).on_hover_text(tip);
            }
            ui.end_row();
            for s in &t.strings {
                let a = s
                    .insulator
                    .as_ref()
                    .and_then(|i| assemblies.iter().find(|a| &a.id == i).copied());
                let mut put = |c: StringChange| changes.push((s.label.clone(), c));
                ui.label(&s.label);
                // Hangs from.
                let mut top = s.top.clone();
                egui::ComboBox::from_id_salt(("string-top", &s.label))
                    .selected_text(&top)
                    .show_ui(ui, |ui| {
                        for l in tops.iter().filter(|l| **l != s.wire) {
                            ui.selectable_value(&mut top, (*l).to_owned(), *l);
                        }
                    });
                if top != s.top {
                    put(StringChange::Top(top));
                }
                // Its wire point: another insulator's makes a V.
                let mut wire = s.wire.clone();
                egui::ComboBox::from_id_salt(("string-wire", &s.label))
                    .selected_text(&wire)
                    .show_ui(ui, |ui| {
                        for w in t.wires.iter().filter(|w| w.label != s.top) {
                            ui.selectable_value(&mut wire, w.label.clone(), &w.label);
                        }
                    });
                if wire != s.wire {
                    put(StringChange::Wire(wire));
                }
                // Kind.
                let mut strain = s.strain;
                egui::ComboBox::from_id_salt(("string-clamp", &s.label))
                    .selected_text(if strain { "strain" } else { "suspension" })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut strain, false, "suspension");
                        ui.selectable_value(&mut strain, true, "strain");
                    });
                if strain != s.strain {
                    put(StringChange::Strain(strain));
                }
                // Assembly.
                let mut pick = s.insulator.clone();
                egui::ComboBox::from_id_salt(("string-insulator", &s.label))
                    .selected_text(pick.as_deref().unwrap_or("— (a length)"))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut pick, None, "— (a length)");
                        for a in &assemblies {
                            ui.selectable_value(&mut pick, Some(a.id.clone()), &a.id)
                                .on_hover_text(format!(
                                    "{}: {:.3} m, {:.0} N",
                                    a.name,
                                    a.length.si(),
                                    a.weight.si()
                                ));
                        }
                    });
                let ends = at(&s.top).zip(at(&s.wire));
                let len_now = ends.map_or(0.0, |(a, b)| polar(a, b).2);
                if pick != s.insulator {
                    put(StringChange::Assembly(pick));
                }
                // Length: it places the wire point.
                let mut l = a.map_or(s.length.map_or(len_now, |l| l.si()), |a| a.length.si());
                if a.is_some() {
                    ui.label(format!("{l:.3} m")).on_hover_text("Its assembly's");
                } else if ui
                    .add(egui::DragValue::new(&mut l).range(0.01..=50.0).speed(0.01).max_decimals(3).suffix(" m"))
                    .changed()
                {
                    put(StringChange::Length(l));
                }
                // Direction: its own, or (a V's leg) where the legs meet.
                let legs = t.strings.iter().filter(|x| x.wire == s.wire).count();
                if legs == 1 {
                    let (angle, direction) = s.angles();
                    let (mut dn, mut dr) = (angle, (direction + ahead).rem_euclid(360.0));
                    let a1 = ui.add(egui::DragValue::new(&mut dn).range(0.0..=180.0).speed(0.5).max_decimals(2).suffix("°"));
                    let a2 = ui.add(egui::DragValue::new(&mut dr).range(0.0..=360.0).speed(0.5).max_decimals(2).suffix("°"));
                    if a1.changed() || a2.changed() {
                        put(StringChange::Direction(dn, (dr - ahead).rem_euclid(360.0)));
                    }
                } else {
                    let (dn, dr, _) = ends.map_or((0.0, 0.0, 0.0), |(a, b)| polar(a, b));
                    let why = "A leg of a V: its direction follows where the legs meet (their lengths)";
                    ui.label(format!("{dn:.2}°")).on_hover_text(why);
                    ui.label(format!("{:.2}°", (dr + ahead).rem_euclid(360.0))).on_hover_text(why);
                }
                // Weight, wind area, compression.
                let mut w = s.weight.map(|w| w.si()).or(a.map(|a| a.weight.si())).unwrap_or(0.0);
                if ui.add(egui::DragValue::new(&mut w).range(0.0..=1e6).speed(1.0).max_decimals(1).suffix(" N")).changed() {
                    put(StringChange::Weight(w));
                }
                let mut wa = s.wind_area.map(|w| w.si()).or(a.map(|a| a.wind_area.si())).unwrap_or(0.0);
                if ui.add(egui::DragValue::new(&mut wa).range(0.0..=100.0).speed(0.005).max_decimals(3).suffix(" m²")).changed() {
                    put(StringChange::WindArea(wa));
                }
                ui.horizontal(|ui| {
                    let mut push = s.compression.is_some();
                    let mut c = s.compression.map_or(0.0, |c| c.si());
                    let a = ui.checkbox(&mut push, "");
                    let b = ui.add_enabled(push, egui::DragValue::new(&mut c).range(0.0..=1e7).speed(10.0).max_decimals(0).suffix(" N"));
                    if a.changed() || b.changed() {
                        put(StringChange::Compression(push.then_some(c)));
                    }
                });
                for c in limits_cell(ui, s.limits(), a.map(|a| a.limits()).unwrap_or_default()) {
                    put(c);
                }
                let mut d = s.description.clone();
                if ui.add(egui::TextEdit::singleline(&mut d).desired_width(160.0)).changed() {
                    put(StringChange::Description(d));
                }
                ui.horizontal(|ui| {
                    if ui
                        .small_button("Save to library")
                        .on_hover_text("Its length, weight, wind area and kind as an assembly of the insulator library")
                        .clicked()
                    {
                        save = Some(s.label.clone());
                    }
                    if ui.small_button("🗑").on_hover_text("Delete this insulator").clicked() {
                        delete = Some(s.label.clone());
                    }
                });
                ui.end_row();
            }
        });
        // The changes, each by the edit of the structure application (its
        // wire point placed again when its geometry changed).
        for (label, change) in changes {
            match edit::set_string(doc, t, g, insulators, &label, change) {
                Ok(()) => changed = true,
                Err(e) => self.message = Some((e, true)),
            }
        }
        if let Some(label) = delete {
            edit::delete_string(doc, t, g, &label);
            self.message = Some((format!("deleted insulator {label}"), false));
            changed = true;
        }
        if let Some(label) = save {
            let id = format!("{}-{label}", t.id);
            match edit::assembly_text(t, g, insulators, &label, &id) {
                Ok(text) => self.request = Some(Request::SaveInsulator { id, text }),
                Err(e) => self.message = Some((e, true)),
            }
        }
        if t.strings.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No insulator yet: pick an attachment point (or add one), then + New insulator or From the library.",
                )
                .weak(),
            );
        }
        changed
    }

    /// The Set points sheet: each set — its number (typed), the wire
    /// point it is on, suspension or strain (from its insulators), a
    /// delete; and a set added on a wire point without one.
    fn sets_sheet(&mut self, ui: &mut egui::Ui, doc: &mut DocumentMut, t: &StructureType) -> bool {
        let mut changed = false;
        let kind = |w: &str| {
            let on: Vec<&ohl_structures::TypeString> =
                t.strings.iter().filter(|x| x.wire == w).collect();
            if on.is_empty() {
                "fixed (no insulator)"
            } else if on.iter().any(|x| x.strain) {
                "strain"
            } else {
                "suspension"
            }
        };
        let mut drop_set: Option<String> = None;
        egui::Grid::new("type-sets").striped(true).show(ui, |ui| {
            for (h, tip) in [
                ("set", "Its number: sections and jumpers end here (unique on the structure)"),
                ("on wire point", "The wire point it is on"),
                ("kind", "From its insulator: suspension (the wire runs through) or strain (the wire ends)"),
                ("", ""),
            ] {
                ui.strong(h).on_hover_text(tip);
            }
            ui.end_row();
            let mut sets: Vec<&ohl_structures::TypeWire> = t.wires.iter().filter(|w| w.set.is_some()).collect();
            sets.sort_by_key(|w| w.set);
            for w in sets {
                let set = w.set.expect("filtered");
                let mut n = i64::from(set);
                if ui.add(egui::DragValue::new(&mut n).range(1..=99_999)).changed() {
                    match edit::set_set(doc, t, &w.label, u32::try_from(n).unwrap_or(set)) {
                        Ok(()) => changed = true,
                        Err(e) => self.message = Some((e, true)),
                    }
                }
                // Moved to another wire point (one without a set).
                let mut on = w.label.clone();
                egui::ComboBox::from_id_salt(("set-on", set))
                    .selected_text(&on)
                    .show_ui(ui, |ui| {
                        for x in t.wires.iter().filter(|x| x.set.is_none() || x.label == w.label) {
                            ui.selectable_value(&mut on, x.label.clone(), &x.label);
                        }
                    });
                if on != w.label {
                    edit::move_set(doc, &w.label, &on, set);
                    changed = true;
                }
                ui.label(kind(&w.label));
                if ui.small_button("🗑").on_hover_text("Take the set off (the wire point stays)").clicked() {
                    drop_set = Some(w.label.clone());
                }
                ui.end_row();
            }
        });
        if let Some(l) = drop_set {
            edit::drop_set(doc, &l);
            changed = true;
        }
        // A new set on a wire point without one.
        let free: Vec<&str> = t
            .wires
            .iter()
            .filter(|w| w.set.is_none())
            .map(|w| w.label.as_str())
            .collect();
        let mut add: Option<String> = None;
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!free.is_empty(), |ui| {
                egui::ComboBox::from_id_salt("set-add")
                    .selected_text("+ Set on wire point…")
                    .show_ui(ui, |ui| {
                        for l in &free {
                            if ui.selectable_label(false, *l).clicked() {
                                add = Some((*l).to_owned());
                            }
                        }
                    })
                    .response
                    .on_hover_text("A set where a wire will be strung: its number is the next free one; change it here");
            });
        });
        if let Some(l) = add
            && let Some(next) = edit::add_set(doc, t, &l)
        {
            changed = true;
            self.message = Some((format!("set {next} on {l}"), false));
        }
        changed
    }
}

/// An insulator's limits in a table cell (kept, not checked by OpenOHL):
/// each ticked and given, or unticked — not given, its assembly's shown
/// dimmed. The changes asked for.
fn limits_cell(
    ui: &mut egui::Ui,
    own: ohl_structures::InsulatorLimits,
    from: ohl_structures::InsulatorLimits,
) -> Vec<StringChange> {
    let fields = edit::limit_fields(own, from);
    let mut out = Vec::new();
    ui.horizontal(|ui| {
        for edit::LimitField {
            name,
            own: v,
            inherited,
            start: default,
            angle,
            change: make,
        } in fields
        {
            let mut on = v.is_some();
            let mut x = v.or(inherited).unwrap_or(default);
            let tick = ui.checkbox(&mut on, name);
            let d = if on {
                ui.add(if angle {
                    egui::DragValue::new(&mut x)
                        .range(0.0..=180.0)
                        .speed(0.5)
                        .max_decimals(2)
                        .suffix("°")
                } else {
                    egui::DragValue::new(&mut x)
                        .range(1.0..=1e9)
                        .speed(100.0)
                        .max_decimals(0)
                        .suffix(" N")
                })
            } else if let Some(i) = inherited {
                ui.weak(if angle {
                    format!("{i:.2}°")
                } else {
                    format!("{i:.0} N")
                })
                .on_hover_text("Its assembly's")
            } else {
                ui.weak("—")
            };
            if tick.changed() || (on && d.changed()) {
                out.push(make(on.then_some(x)));
            }
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PYLON: &str = r#"id = "P"
name = "p"
kind = "lattice"

[[node]]
id = "N1"
point = ["0 m", "0 m", "0 m"]
support = "pinned"

[[node]]
id = "N2"
point = ["0 m", "0 m", "30 m"]

[[node]]
id = "N3"
point = ["-6 m", "0 m", "30 m"]

[[member]]
id = "M1"
nodes = ["N1", "N2"]

[[member]]
id = "M2"
nodes = ["N2", "N3"]

[[point]]
label = "A1"
node = "N3"
"#;

    fn read(doc: &DocumentMut) -> StructureType {
        StructureType::parse(std::path::Path::new("p.ohls"), &doc.to_string()).unwrap()
    }

    /// The screen's edits are those of the structure application, on the
    /// basics: an attachment point, an I-string under it with a set on its
    /// wire point, the point moved (its wire point follows), read back (the
    /// edits themselves are tested in `ohl-structure-edit`); the body is
    /// left as it was.
    #[test]
    fn the_screen_uses_the_shared_edits_on_the_basics() {
        let mut doc: DocumentMut = PYLON.parse().unwrap();
        let t = read(&doc);
        let a2 = edit::add_point(&mut doc, &t, [6.0, 0.0, 30.0]);
        let t = read(&doc);
        let w = edit::add_strings(&mut doc, &t, std::slice::from_ref(&a2), 4.0).unwrap();
        let t = read(&doc);
        let set = edit::add_set(&mut doc, &t, &w).unwrap();
        let t = read(&doc);
        edit::move_to(&mut doc, &t, &Handle::Point(a2), [7.0, 0.0, 30.0]).unwrap();
        let t = read(&doc);
        let g = geometry(&t, &InsulatorLibrary::default());
        assert_eq!(g.at(&w), Some([7.0, 0.0, 26.0]));
        assert_eq!(t.wires[0].set, Some(set));
        assert_eq!((t.nodes.len(), t.members.len()), (3, 2));
        assert_eq!(compass_radius(&g), 8.75_f64.max(0.35 * 30.0));
    }
}
