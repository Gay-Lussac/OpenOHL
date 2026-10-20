// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The 3D view: the renderer's image of the visible layers, a camera
//! moved with the mouse and the keyboard as the chosen control scheme
//! says ([`Controls`]), and picking.

use std::collections::{HashMap, HashSet};

use eframe::egui;
use eframe::egui_wgpu;
use ohl_core::grid::Tile;
use ohl_pointcloud::octree::NodeCache;
use ohl_render::lod::{ChunkSet, Frustum};
use ohl_render::{Camera, GpuScene, Renderer};

use crate::controls::{Controls, Drag};
use crate::model::Model;
use crate::scene::{self, Layer};
use crate::selection::{self, Selection};

/// State of the 3D view.
#[derive(Default)]
pub struct View3d {
    /// The weather case of the copy picked by the last click (`Some(None)`:
    /// the case shown); taken by the app.
    pub picked_case: Option<Option<usize>>,
    /// The member clicked by the last click on a structure at level 2 or 3
    /// (structure id, member id); taken by the app (the structure screen
    /// selects it).
    pub picked_member: Option<(String, String)>,
    /// Camera (set when a model is shown).
    pub camera: Option<Camera>,
    /// Navigation controls.
    pub controls: Controls,
    renderer: Option<Renderer>,
    gpu: HashMap<Layer, GpuScene>,
    /// Chunked layers: their index, chunks on the GPU, the row they were
    /// built with ([`crate::lod`]).
    chunks: HashMap<Layer, Chunked>,
    /// The point cloud (the project's or a COPC file), streamed by node
    /// under the point budget.
    points: Option<Streamed>,
    /// Layers to rebuild before the next frame.
    pub dirty: HashSet<Layer>,
    texture: Option<egui::TextureId>,
    /// The mouse is captured (first person).
    pub captured: bool,
    /// Where the view is on the screen (its centre is the crosshair).
    pub rect: Option<egui::Rect>,
    /// It was drawn in the last frame (its page is in front).
    pub drawn: bool,
    /// The release of the press that captured the mouse is not a click.
    swallow_release: bool,
    /// The last left press was on a drag handle (its click only selects).
    left_press_on_handle: bool,
    /// Clearance violations to draw: (on the wire, on the target).
    pub marks: Vec<([f64; 3], [f64; 3])>,
    /// The check that found them is out of date: drawn dimmed.
    pub marks_stale: bool,
    /// What Checks › Vegetation found.
    pub vegetation: crate::vegetation::Overlay,
    /// A wire being strung: its points so far.
    pub draft: Vec<[f64; 3]>,
    /// What a replacement would remove (Replace structure's window open):
    /// polylines drawn red over the line.
    pub removed: Vec<Vec<[f64; 3]>>,
    /// A measured line to draw on top, with its label (the distance
    /// between conductors' worst station: d and z).
    pub measure: Option<crate::phase_distance::Measure>,
    /// The camera was fitted to the line's structures or wires (else to
    /// its alignments or the ground: fitted again when the first
    /// structures come).
    fitted_on_line: bool,
    /// Drag handles of the structure being edited ([`crate::handles`]).
    pub handles: crate::handles::Handles,
    /// A box or lasso being drawn to select LiDAR points
    /// ([`crate::area`]).
    pub area: Option<crate::area::AreaDrag>,
    /// The LiDAR points a box or lasso selected (at most
    /// [`ohl_session::points::MAX_POINTS`] + 1), how many it held, how long
    /// finding them took; taken by the app.
    pub area_picked: Option<(Vec<crate::model::LidarPoint>, usize, std::time::Duration)>,
    /// Chunks built so far (tests: none while the camera stands still).
    #[cfg(test)]
    pub chunks_built: usize,
    /// Images drawn so far (tests: none while nothing changes).
    #[cfg(test)]
    pub images_drawn: usize,
    /// Changes of what is on the GPU (layers rebuilt, chunks built, the
    /// point cloud's settings): part of [`View3d::image_key`].
    generation: u64,
    /// What the image shown was drawn from: drawn again only when that
    /// changes (a frame for another panel, the mouse moved elsewhere,
    /// shows the same image).
    image_key: Option<u64>,
}

/// A layer drawn in chunks.
struct Chunked {
    index: crate::chunks::ChunkIndex,
    set: ChunkSet<Tile>,
    /// Its member and pole solids, by chunk, drawn within the solids
    /// distance ([`crate::lod::LodSettings::solids`]): level 0 only.
    near: ChunkSet<Tile>,
    row: crate::lod::LodRow,
}

impl Chunked {
    /// What it draws in this frame: its chunks at their levels, and the
    /// solids within the solids distance.
    fn drawn(&self) -> impl Iterator<Item = &GpuScene> {
        self.set.drawn().chain(self.near.drawn())
    }
}

/// A point cloud streamed by node: its nodes on the GPU, those drawn.
struct Streamed {
    /// The cloud (`FILE.ohl`, `FILE.ohl#name` or a COPC file).
    spec: String,
    octree: crate::octree::PointOctree,
    cache: NodeCache<Node>,
    drawn: Vec<usize>,
}

/// A node of a streamed cloud: on the GPU, and its points as drawn (for
/// picking).
struct Node {
    gpu: GpuScene,
    points: crate::octree::NodePoints,
}

/// Time for building chunks in one frame (the nearest first; the rest in
/// the next frames).
const BUILD_BUDGET: std::time::Duration = std::time::Duration::from_millis(8);

impl View3d {
    /// Points drawn by a chunked or streamed layer in the last frame
    /// (tests).
    #[cfg(test)]
    pub fn drawn_points(&self, layer: Layer) -> u32 {
        if layer == Layer::Points
            && let Some(p) = &self.points
        {
            return p
                .drawn
                .iter()
                .filter_map(|&i| p.cache.peek(i))
                .map(|g| g.gpu.counts().1)
                .sum();
        }
        self.chunks
            .get(&layer)
            .map_or(0, |c| c.drawn().map(|g| g.counts().1).sum())
    }

    /// Segments, points and triangles drawn in the last frame, every
    /// layer shown (benchmark).
    #[cfg(test)]
    pub fn drawn_counts(&self, visible: &HashSet<Layer>) -> [u64; 3] {
        let mut n = [0u64; 3];
        let mut add = |g: &GpuScene| {
            let c = g.counts();
            n[0] += u64::from(c.0);
            n[1] += u64::from(c.1);
            n[2] += u64::from(c.2);
        };
        for l in Layer::ALL.iter().filter(|l| visible.contains(l)) {
            match (self.chunks.get(l), &self.points) {
                (_, Some(p)) if *l == Layer::Points => {
                    p.drawn
                        .iter()
                        .filter_map(|&i| p.cache.peek(i))
                        .for_each(|n| add(&n.gpu));
                }
                (Some(c), _) => c.drawn().for_each(&mut add),
                (None, _) => self.gpu.get(l).into_iter().for_each(&mut add),
            }
        }
        n
    }

    /// Chunks or nodes still to build or read (benchmark).
    #[cfg(test)]
    pub fn pending(&self) -> usize {
        self.chunks
            .values()
            .map(|c| c.set.stats.pending + c.near.stats.pending)
            .sum::<usize>()
            + self.points.as_ref().map_or(0, |p| p.octree.stats.pending)
    }

    /// The LiDAR points drawn in the last frame, for picking:
    /// the nodes of a streamed cloud, or the chunks of the points held in
    /// memory at the level each is drawn; nothing while the layer is
    /// hidden.
    fn lidar_shown<'a>(
        points: &'a Option<Streamed>,
        chunks: &'a HashMap<Layer, Chunked>,
        model: &Model,
        visible: &HashSet<Layer>,
    ) -> selection::LidarShown<'a> {
        use selection::{LidarGroup, LidarPoints, LidarShown};
        if !visible.contains(&Layer::Points) {
            return LidarShown::Nothing;
        }
        if let Some(p) = points
            && model.cloud.as_ref() == Some(&p.spec)
        {
            return LidarShown::Groups(
                p.drawn
                    .iter()
                    .filter_map(|&i| p.cache.peek(i))
                    .map(|n| LidarGroup {
                        bounds: n.points.bounds,
                        points: LidarPoints::Columns(&n.points.xyz, &n.points.class),
                        cells: &n.points.cells,
                    })
                    .collect(),
            );
        }
        let Some(c) = chunks.get(&Layer::Points) else {
            return LidarShown::Nothing;
        };
        LidarShown::Groups(
            c.set
                .shown()
                .filter_map(|(tile, level)| c.index.points_drawn(tile, level, &c.row))
                .map(|(b, indices, step)| LidarGroup {
                    bounds: Some([b.min, b.max]),
                    points: LidarPoints::Indices(indices, step),
                    cells: &[],
                })
                .collect(),
        )
    }

    /// Every LiDAR point drawn in the last frame (tests).
    #[cfg(test)]
    pub fn drawn_lidar(
        &self,
        model: &Model,
        visible: &HashSet<Layer>,
    ) -> Vec<crate::model::LidarPoint> {
        use selection::{LidarPoints, LidarShown};
        let mut out = Vec::new();
        if let LidarShown::Groups(groups) =
            Self::lidar_shown(&self.points, &self.chunks, model, visible)
        {
            for g in groups {
                match g.points {
                    LidarPoints::Columns(xyz, class) => {
                        out.extend(xyz.iter().copied().zip(class.iter().copied()));
                    }
                    LidarPoints::Indices(indices, step) => out.extend(
                        indices
                            .iter()
                            .filter(|k| (**k as usize).is_multiple_of(step))
                            .map(|&k| model.points[k as usize]),
                    ),
                }
            }
        }
        out
    }

    /// What a click at `at` (pixels from the view's top left, a view of
    /// `size`) picks: [`selection::pick_shown`] with the LiDAR points drawn
    /// in the last frame.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn pick(
        &self,
        camera: &Camera,
        size: [f64; 2],
        model: &Model,
        case: usize,
        snaps: &selection::Snaps,
        visible: &HashSet<Layer>,
        at: [f64; 2],
        radius: f64,
    ) -> (Selection, Option<usize>) {
        let project = camera.projector(size);
        let lidar = Self::lidar_shown(&self.points, &self.chunks, model, visible);
        selection::pick_shown(model, case, snaps, at, radius, &project, &lidar)
    }

    /// What a chunked layer drew in the last frame.
    pub fn lod_stats(&self, layer: Layer) -> Option<ohl_render::lod::PlanStats> {
        self.chunks.get(&layer).map(|c| c.set.stats)
    }

    /// What the point cloud streamed by node did in the last frame.
    pub fn stream_stats(&self) -> Option<crate::octree::StreamStats> {
        self.points.as_ref().map(|p| p.octree.stats)
    }

    /// One frame of the point cloud streamed by node: the settings of
    /// the model (hidden classes, colours, share shown) and of the Level
    /// of detail page, the nodes read since uploaded, the view's nodes
    /// picked and the missing ones asked for. Whether the settings
    /// changed (the nodes read again).
    fn stream_points(
        p: &mut Streamed,
        model: &Model,
        lod: &crate::lod::LodSettings,
        frustum: &Frustum,
        pixels_per_radian: f64,
        device: &eframe::wgpu::Device,
    ) -> bool {
        let colors: Vec<Option<[u8; 3]>> = (0..=255u8)
            .map(|c| scene::class_color(&model.scheme, c))
            .collect();
        let classes: Vec<u8> = (0..=255u8)
            .filter(|c| !model.hidden_classes.contains(c) && colors[*c as usize].is_some())
            .collect();
        let whole: Vec<u8> = model
            .class_counts
            .iter()
            .filter(|(_, n)| *n <= crate::model::SMALL_CLASS)
            .map(|(c, _)| *c)
            .collect();
        let every = (100.0 / model.points_percent.clamp(0.01, 100.0))
            .round()
            .max(1.0) as u64;
        let changed = p.octree.set(crate::octree::LoadSettings {
            classes: Some(classes),
            whole,
            every,
            colors: std::sync::Arc::new(colors),
        });
        if changed {
            p.cache.clear();
        }
        let bounds = |b: &ohl_pointcloud::octree::Box3| ohl_render::Bounds {
            min: b[0],
            max: b[1],
        };
        let distance = |b: &ohl_pointcloud::octree::Box3| frustum.distance(&bounds(b));
        let sees = |b: &ohl_pointcloud::octree::Box3| frustum.sees(&bounds(b));
        let view = ohl_pointcloud::octree::LodView {
            distance: &distance,
            sees: &sees,
            pixels_per_radian,
            render_distance: lod.row(Layer::Points).render_distance,
        };
        let limits = ohl_pointcloud::octree::LodLimits {
            point_budget: lod.points.budget,
            max_error: lod.points.max_error,
            share: 1.0 / every as f64,
        };
        p.drawn = p
            .octree
            .frame(&mut p.cache, &view, &limits, |s, points| Node {
                gpu: GpuScene::new(device, s),
                points,
            });
        changed
    }

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

    /// The LiDAR points read again with new colours (the class scheme
    /// changed).
    pub fn recolor_points(&mut self) {
        self.points = None;
        self.dirty.insert(Layer::Points);
    }

    /// Draws the handles of the structure being edited ([`View3d::ui`]).
    fn paint_handles(
        handles: &crate::handles::Handles,
        painter: &egui::Painter,
        spots: &[crate::handles::Spot],
        structure: Option<&ohl_line::Structure>,
        screen: impl Fn([f64; 3]) -> Option<egui::Pos2>,
    ) {
        use ohl_structure_edit::Handle;
        let dark = egui::Color32::from_black_alpha(200);
        let drag = handles.drag.as_ref();
        let global = |s: &ohl_line::Structure, p: [f64; 3]| {
            ohl_structures::structures::to_global(s.position, s.orientation_deg, p)
        };
        for spot in spots {
            let dragged = drag.is_some_and(|d| d.handle == spot.handle);
            let at = match (dragged, drag, structure) {
                (true, Some(d), Some(s)) => global(s, d.at),
                _ => spot.global,
            };
            let Some(p) = screen(at) else { continue };
            let hot = handles.hovered.as_ref() == Some(&spot.handle);
            let fill = match spot.handle {
                Handle::Point(_) => egui::Color32::from_rgb(255, 150, 60),
                Handle::Wire(_) => egui::Color32::from_rgb(90, 200, 255),
                _ => egui::Color32::from_rgb(170, 170, 180),
            };
            let r = if hot { 7.0 } else { 4.5 };
            let outline = egui::Stroke::new(
                if hot { 2.5 } else { 1.5 },
                if hot { egui::Color32::WHITE } else { dark },
            );
            if matches!(spot.handle, Handle::Joint(_)) {
                let rect = egui::Rect::from_center_size(p, egui::vec2(2.0 * r, 2.0 * r) * 0.85);
                painter.rect_filled(rect, 1.0, fill);
                painter.rect_stroke(rect, 1.0, outline, egui::StrokeKind::Outside);
            } else {
                painter.circle(p, r, fill, outline);
            }
            if hot && drag.is_none() {
                let tip = match &spot.fixed {
                    Some(_) => format!("{} (part of the body)", spot.handle.describe()),
                    None => format!(
                        "{}: x {:.3}  y {:.3}  z {:.3} m — drag to move",
                        spot.handle.describe(),
                        spot.local[0],
                        spot.local[1],
                        spot.local[2]
                    ),
                };
                Self::label(painter, p + egui::vec2(10.0, -10.0), tip);
            }
        }
        let (Some(d), Some(s)) = (drag, structure) else {
            return;
        };
        let (from, to) = (global(s, d.from), global(s, d.at));
        // The axis held to, through where it was, in its colour.
        if let crate::handles::Constraint::Axis(k) = d.constraint {
            let e = crate::handles::axis(s.orientation_deg, k);
            let reach = 1000.0;
            let a = [0, 1, 2].map(|i| from[i] - reach * e[i]);
            let b = [0, 1, 2].map(|i| from[i] + reach * e[i]);
            // Long lines: drawn between their points in front of the eye.
            let samples: Vec<egui::Pos2> = (0..=200)
                .filter_map(|n| {
                    let t = f64::from(n) / 200.0;
                    screen([0, 1, 2].map(|i| a[i] + t * (b[i] - a[i])))
                })
                .collect();
            let color = [
                egui::Color32::from_rgb(230, 70, 70),
                egui::Color32::from_rgb(80, 200, 90),
                egui::Color32::from_rgb(80, 130, 255),
            ][k];
            painter.add(egui::Shape::line(samples, egui::Stroke::new(1.5, color)));
        }
        if let (Some(p), Some(q)) = (screen(from), screen(to)) {
            painter.line_segment([p, q], egui::Stroke::new(2.0, egui::Color32::WHITE));
            painter.circle_stroke(p, 4.0, egui::Stroke::new(1.5, egui::Color32::WHITE));
            if let Some(text) = handles.readout() {
                Self::label(painter, q + egui::vec2(12.0, 12.0), text);
            }
        }
    }

    /// A line of text on a dark box.
    fn label(painter: &egui::Painter, at: egui::Pos2, text: String) {
        let galley =
            painter.layout_no_wrap(text, egui::FontId::proportional(13.0), egui::Color32::WHITE);
        painter.rect_filled(
            egui::Rect::from_min_size(at, galley.size()).expand(3.0),
            3.0,
            egui::Color32::from_black_alpha(190),
        );
        painter.galley(at, galley, egui::Color32::WHITE);
    }

    /// Marks every layer for rebuilding.
    pub fn rebuild_all(&mut self) {
        self.dirty.extend(Layer::ALL);
    }

    /// Shows the view.
    #[allow(clippy::too_many_arguments)]
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        rs: Option<&egui_wgpu::RenderState>,
        model: &Model,
        case: usize,
        selection: &mut Selection,
        visible: &HashSet<Layer>,
        snaps: &selection::Snaps,
        lod: &crate::lod::LodSettings,
        select_held: bool,
    ) -> Option<selection::Use> {
        let Some(rs) = rs else {
            // Shown (its page in front), only without an image.
            self.rect = Some(ui.max_rect());
            self.drawn = true;
            ui.centered_and_justified(|ui| ui.label("3D view needs a GPU (wgpu)."));
            return None;
        };
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        self.rect = Some(rect);
        self.drawn = true;
        // The first structures of a line that had none: fitted to them.
        if self.camera.is_some() && !self.fitted_on_line && !model.line.structures.is_empty() {
            self.camera = None;
        }
        let fitted_on_line = &mut self.fitted_on_line;
        let camera = self.camera.get_or_insert_with(|| {
            // The line's structures, else its wires, else its alignments,
            // else the ground.
            let mut s = scene::layer(model, Layer::Structures, case, selection);
            if s.bounds().is_none() {
                s = scene::layer(model, Layer::Wires, case, selection);
            }
            *fitted_on_line = s.bounds().is_some();
            for layer in [Layer::Alignment, Layer::Ground] {
                if s.bounds().is_none() {
                    s = scene::layer(model, layer, case, selection);
                }
            }
            s.bounds().map_or(
                Camera::fit(&ohl_render::Bounds {
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
        // Drag handles of the structure being edited: the one under the
        // pointer, a drag started on it, followed and ended.
        let structure = self
            .handles
            .target
            .as_ref()
            .and_then(|t| model.line.structure(&t.structure));
        let spots = match (&self.handles.target, self.captured) {
            (Some(t), false) => crate::handles::spots(model, &t.structure),
            _ => Vec::new(),
        };
        let still = *camera;
        let project_handle = still.projector(size);
        let on_handle = |at: [f64; 2], skip: Option<&ohl_structure_edit::Handle>| {
            spots
                .iter()
                .filter(|s| Some(&s.handle) != skip)
                .filter_map(|s| {
                    let (q, _) = project_handle(s.global)?;
                    let d = (q[0] - at[0]).hypot(q[1] - at[1]);
                    Some((d, s))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(d, s)| (d, s.clone()))
        };
        let pointer = ui
            .input(|i| i.pointer.latest_pos())
            .filter(|p| rect.contains(*p));
        let primary_down = ui.input(|i| i.pointer.primary_down());
        self.handles.hovered = match (&self.handles.drag, pointer) {
            (Some(d), _) => Some(d.handle.clone()),
            (None, Some(p)) if response.hovered() => on_handle(local(p), None)
                .filter(|(d, _)| *d <= crate::handles::RADIUS)
                .map(|(_, s)| s.handle),
            _ => None,
        };
        let pressed_on_handle = !self.captured
            && self.handles.hovered.is_some()
            && self.handles.drag.is_none()
            && response.hovered()
            && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary));
        if pressed_on_handle
            && let (Some(t), Some(h), Some(s)) = (
                self.handles.target.clone(),
                self.handles.hovered.clone(),
                structure,
            )
        {
            let spot = on_handle(local(pointer.unwrap_or(rect.center())), None)
                .map(|(_, s)| s)
                .filter(|s| s.handle == h);
            match (spot, &t.blocked) {
                (Some(spot), _) if spot.fixed.is_some() => {
                    self.handles.done = Some(crate::handles::Done::Refused(
                        spot.fixed.clone().unwrap_or_default(),
                    ));
                    self.handles.swallow = true;
                }
                (Some(_), Some(why)) => {
                    self.handles.done = Some(crate::handles::Done::Refused(why.clone()));
                    self.handles.swallow = true;
                }
                (Some(spot), None) => {
                    log::info!("3D view: drag {} of {}", h.describe(), s.id);
                    self.handles.drag = Some(crate::handles::Drag {
                        structure: s.id.clone(),
                        handle: h,
                        from: spot.local,
                        at: spot.local,
                        constraint: crate::handles::Constraint::Free,
                        snapped: None,
                    });
                    response.request_focus();
                }
                (None, _) => {}
            }
        }
        if let Some(d) = self.handles.drag.as_mut()
            && let Some(s) = structure.filter(|s| s.id == d.structure)
        {
            // X, Y, Z: on that axis; with Shift, in the plane across it.
            ui.input_mut(|i| {
                use egui::{Key, Modifiers};
                for (k, key) in [Key::X, Key::Y, Key::Z].into_iter().enumerate() {
                    if i.consume_key(Modifiers::SHIFT, key) {
                        d.constraint = d.constraint.toggled(k, true);
                    } else if i.consume_key(Modifiers::NONE, key) {
                        d.constraint = d.constraint.toggled(k, false);
                    }
                }
            });
            if let Some(p) = pointer
                && let Some((a, dir)) = camera.ray(local(p), size)
            {
                let eye = camera.eye();
                let n = [0, 1, 2].map(|k| eye[k] - camera.target[k]);
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
                let facing = n.map(|v| v / len);
                if let Some(at) = crate::handles::drag_to(s, d.from, d.constraint, facing, a, dir) {
                    let unsnapped = ui.input(|i| i.modifiers.alt);
                    let onto = on_handle(local(p), Some(&d.handle))
                        .filter(|(dist, _)| *dist <= crate::handles::SNAP_RADIUS)
                        .map(|(_, s)| s);
                    let grid = self.handles.target.as_ref().map_or(0.0, |t| t.grid);
                    (d.at, d.snapped) = if unsnapped {
                        (at, None)
                    } else {
                        crate::handles::snap(at, d.constraint, grid, onto.as_ref())
                    };
                }
            }
            if !primary_down {
                let d = self.handles.drag.take().expect("dragging");
                // A press and release without moving is a click: it
                // selects, it never moves (or snaps) the point.
                let click = ui.input(|i| i.pointer.could_any_button_be_click());
                if d.at != d.from && !click {
                    self.handles.done = Some(crate::handles::Done::Moved {
                        structure: d.structure,
                        handle: d.handle,
                        at: d.at,
                    });
                }
            }
            ui.ctx().request_repaint();
        } else if self.handles.drag.is_some() {
            // Its structure went (or is no longer edited).
            self.handles.drag = None;
        }
        if self.handles.swallow && !primary_down {
            self.handles.swallow = false;
        }
        let handle_drag = self.handles.drag.is_some() || self.handles.swallow;
        // Which button's click selects and which uses the held tool (the
        // option "Tools act on left-click"; drags are not clicks).
        let (select_button, tool_button) =
            crate::controls::click_buttons(self.controls.tools_on_left, select_held);
        if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
            self.left_press_on_handle = pressed_on_handle;
        }
        let mut pick_at: Option<([f64; 2], bool)> = None;
        // A box or lasso drawn (released this frame).
        let mut area_done: Option<crate::area::Area> = None;
        // A click of the tool button uses the held tool on what is there.
        let mut use_at: Option<[f64; 2]> = None;
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
            // Clicks aim with the crosshair; the release of the press that
            // captured the mouse is no click (of either role).
            let swallowed =
                |b: egui::PointerButton| self.swallow_release && b == egui::PointerButton::Primary;
            let (click, double) = ui.input(|i| {
                (
                    i.pointer.button_clicked(select_button),
                    i.pointer.button_double_clicked(select_button),
                )
            });
            if !swallowed(select_button) && (click || double) {
                pick_at = Some((centre, double));
            }
            if !swallowed(tool_button) && ui.input(|i| i.pointer.button_clicked(tool_button)) {
                use_at = Some(centre);
            }
            if self.swallow_release
                && ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary))
            {
                self.swallow_release = false;
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
            let alt = ui.input(|i| i.modifiers.alt);
            if self.controls.capture && pressed_here && !pressed_on_handle && !handle_drag && !alt {
                Self::set_captured(&mut self.captured, ui.ctx(), true);
                // Swallow its release, unless it came in the same frame.
                self.swallow_release =
                    !ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary));
                response.request_focus();
            }
            // In the first-person scheme the left button only captures the
            // mouse here (its clicks act once captured).
            let acts = |b: egui::PointerButton| {
                !(self.controls.capture && b == egui::PointerButton::Primary)
            };
            // A left-click on a drag handle (released without moving) only
            // selects, also when tools act on left-click.
            let on_handle = self.left_press_on_handle;
            let tool_click = acts(tool_button)
                && response.clicked_by(tool_button)
                && !(tool_button == egui::PointerButton::Primary && on_handle);
            let select_click = acts(select_button)
                && (response.clicked_by(select_button)
                    || response.double_clicked_by(select_button));
            let handle_click = acts(tool_button)
                && tool_button == egui::PointerButton::Primary
                && on_handle
                && (response.clicked() || response.double_clicked());
            if let Some(pos) = response.interact_pointer_pos() {
                if select_click {
                    pick_at = Some((local(pos), response.double_clicked_by(select_button)));
                } else if handle_click {
                    pick_at = Some((local(pos), false));
                }
                if tool_click {
                    use_at = Some(local(pos));
                }
            }
            // Alt + drag of the selecting button: a box or lasso of LiDAR
            // points (not on a drag handle).
            let (done, drawing) = if handle_drag || self.left_press_on_handle {
                (None, false)
            } else {
                crate::area::frame(&mut self.area, ui, &response, select_button)
            };
            area_done = done;
            if response.dragged()
                && !drawing
                && !(handle_drag && response.dragged_by(egui::PointerButton::Primary))
            {
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
        // Not flying while a handle is dragged (Shift + Z holds a plane),
        // nor while Alt is held or an area drawn (Alt + Shift: a lasso).
        let area_keys = self.area.is_some() || ui.input(|i| i.modifiers.alt);
        if !handle_drag
            && !area_keys
            && (self.captured || response.hovered() || response.has_focus())
        {
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
        let radius = if self.captured { 10.0 } else { 14.0 };
        let lidar = Self::lidar_shown(&self.points, &self.chunks, model, visible);
        if let Some(area) = area_done {
            let started = std::time::Instant::now();
            let project = camera.projector(size);
            let (points, total) = selection::lidar_in_area(
                model,
                &lidar,
                &area,
                &project,
                None,
                ohl_session::points::MAX_POINTS + 1,
            );
            let took = started.elapsed();
            log::info!("3D view: {total} LiDAR points in the area, found in {took:.1?}");
            self.area_picked = Some((points, total, took));
        }
        let used = use_at.map(|at| {
            let project = camera.projector(size);
            let target = selection::pick_shown(model, case, snaps, at, radius, &project, &lidar).0;
            let ground = camera
                .ray(at, size)
                .and_then(|(a, d)| model.ground_hit(a, d, camera.target[2]));
            log::info!(
                "3D view: tool click (mouse captured: {}) on {}",
                self.captured,
                target.describe()
            );
            let on = selection::on_span(model, case, &target, at, |p| project(p).map(|(q, _)| q));
            selection::Use {
                target,
                at: ground,
                on,
            }
        });
        if let Some((at, double)) = pick_at {
            let cam = *camera;
            let project = cam.projector(size);
            let started = std::time::Instant::now();
            let (picked, picked_case) =
                selection::pick_shown(model, case, snaps, at, radius, &project, &lidar);
            self.picked_case = Some(picked_case);
            self.picked_member = match &picked {
                Selection::Structure(id) => {
                    selection::picked_member(model, id, at, radius, |p| project(p).map(|(q, _)| q))
                        .map(|m| (id.clone(), m))
                }
                _ => None,
            };
            log::info!(
                "3D view: picked {} in {:.1?}",
                picked.describe(),
                started.elapsed()
            );
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
        let rebuild = crate::perf::span("3d layers rebuilt");
        if !self.dirty.is_empty() {
            self.generation += 1;
        }
        for layer in self.dirty.drain() {
            if layer == Layer::Points
                && let Some(spec) = &model.cloud
            {
                // The point cloud: streamed by node (hidden classes and
                // shares are applied every frame).
                self.chunks.remove(&layer);
                if self.points.as_ref().is_none_or(|p| &p.spec != spec) {
                    self.points = match crate::octree::PointOctree::open(
                        spec,
                        model.origin,
                        Some(ui.ctx().clone()),
                    ) {
                        Ok(octree) => Some(Streamed {
                            spec: spec.clone(),
                            octree,
                            cache: NodeCache::default(),
                            drawn: Vec::new(),
                        }),
                        Err(e) => {
                            log::warn!("LiDAR not streamed: {e}");
                            None
                        }
                    };
                }
                continue;
            }
            if layer == Layer::Points {
                self.points = None;
            }
            if crate::lod::CHUNKED.contains(&layer) {
                // Indexed now, built by chunk as the view needs them.
                let index = crate::chunks::ChunkIndex::new(model, layer, case);
                let mut set = ChunkSet::default();
                set.reset(index.keys());
                let mut near = ChunkSet::default();
                near.reset(index.near_keys());
                self.chunks.insert(
                    layer,
                    Chunked {
                        index,
                        set,
                        near,
                        row: lod.row(layer),
                    },
                );
                continue;
            }
            let s = match layer {
                Layer::Clearances => scene::marks(model.origin, &self.marks, self.marks_stale),
                Layer::GrowIn | Layer::FallIn | Layer::Isoclearance => {
                    scene::vegetation(model.origin, &self.vegetation, layer)
                }
                Layer::Draft => scene::draft(model.origin, &self.draft, &self.removed),
                _ => scene::layer(model, layer, case, selection),
            };
            self.gpu.insert(layer, GpuScene::new(device, &s));
        }
        drop(rebuild);
        // Chunks: the level of each by its distance, the missing ones built
        // nearest first within the frame's budget.
        let planning = crate::perf::span("3d chunk plan");
        let aspect = f64::from(rect.width()) / f64::from(rect.height().max(1.0));
        let frustum = Frustum::new(camera, aspect);
        let mut wanted = Vec::new();
        for layer in crate::lod::CHUNKED {
            let Some(c) = self.chunks.get_mut(&layer) else {
                continue;
            };
            if !visible.contains(&layer) {
                c.set.stats = Default::default();
                c.near.stats = Default::default();
                continue;
            }
            let row = lod.row(layer);
            if row.detail != c.row.detail {
                // Other details: the levels built go.
                c.set.reset(c.index.keys());
            }
            c.row = row;
            for w in c.set.plan(&frustum, &row.rule()) {
                wanted.push((layer, false, w));
            }
            // The solids by their own distance.
            if !c.near.is_empty() {
                for w in c.near.plan(&frustum, &lod.solids_rule(layer)) {
                    wanted.push((layer, true, w));
                }
            }
        }
        wanted.sort_by(|a, b| a.2.distance.total_cmp(&b.2.distance));
        drop(planning);
        let building = crate::perf::span("3d chunk build");
        let started = std::time::Instant::now();
        let mut built = HashSet::new();
        let mut n_built = 0;
        for (layer, near, w) in &wanted {
            if n_built > 0 && started.elapsed() > BUILD_BUDGET {
                break;
            }
            let Some(c) = self.chunks.get_mut(layer) else {
                continue;
            };
            if *near {
                let s = c.index.build_near(model, &w.key);
                c.near.insert(&w.key, 0, GpuScene::new(device, &s));
            } else {
                let s = c.index.build(model, *layer, &w.key, w.level, &c.row);
                c.set.insert(&w.key, w.level, GpuScene::new(device, &s));
            }
            built.insert(*layer);
            n_built += 1;
            #[cfg(test)]
            {
                self.chunks_built += 1;
            }
        }
        if n_built > 0 {
            self.generation += 1;
        }
        // What was built is drawn now; the rest in the next frames.
        for layer in &built {
            if let Some(c) = self.chunks.get_mut(layer) {
                let rule = c.row.rule();
                c.set.plan(&frustum, &rule);
                c.near.plan(&frustum, &lod.solids_rule(*layer));
            }
        }
        if wanted.len() > n_built {
            ui.ctx().request_repaint();
        }
        drop(building);
        let streaming = crate::perf::span("3d point stream");
        // The point cloud: the nodes for this view within the budget.
        if let Some(p) = &mut self.points {
            if visible.contains(&Layer::Points) {
                let px_height = f64::from(rect.height() * ui.ctx().pixels_per_point()).max(1.0);
                let pixels_per_radian = 0.5 * px_height / (0.5 * camera.fov_y).tan();
                if Self::stream_points(p, model, lod, &frustum, pixels_per_radian, device) {
                    self.generation += 1;
                }
                if p.octree.stats.pending > 0 {
                    // The nodes come back on the readers' repaints; keep
                    // planning.
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(50));
                }
            } else {
                p.drawn.clear();
            }
        }
        drop(streaming);
        let rendering = crate::perf::span("3d render");
        let renderer = self.renderer.get_or_insert_with(|| Renderer::new(device));
        let ppp = ui.ctx().pixels_per_point();
        let px = [
            (rect.width() * ppp).round().max(1.0) as u32,
            (rect.height() * ppp).round().max(1.0) as u32,
        ];
        let mut layers: Vec<&GpuScene> = Vec::new();
        for l in Layer::ALL.iter().filter(|l| visible.contains(l)) {
            match (self.chunks.get(l), &self.points) {
                (_, Some(p)) if *l == Layer::Points => {
                    layers.extend(
                        p.drawn
                            .iter()
                            .filter_map(|&i| p.cache.peek(i).map(|n| &n.gpu)),
                    );
                }
                (Some(c), _) => layers.extend(c.drawn()),
                (None, _) => layers.extend(self.gpu.get(l)),
            }
        }
        // The same camera, size and scenes as the image shown: kept.
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            let c: &Camera = camera;
            [
                c.target[0],
                c.target[1],
                c.target[2],
                c.distance,
                c.yaw,
                c.pitch,
                c.fov_y,
                c.exaggeration,
            ]
            .map(f64::to_bits)
            .hash(&mut h);
            (px, self.generation).hash(&mut h);
            for g in &layers {
                (std::ptr::from_ref::<GpuScene>(*g) as usize, g.counts()).hash(&mut h);
            }
            h.finish()
        };
        let id = match self.texture {
            Some(id) if self.image_key == Some(key) => id,
            _ => {
                #[cfg(test)]
                {
                    self.images_drawn += 1;
                }
                let texture = renderer.render(device, &rs.queue, &layers, camera, px);
                // egui takes texture bytes as sRGB-encoded: a plain view of
                // them.
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
                self.image_key = Some(key);
                id
            }
        };
        let painter = ui.painter_at(rect);
        painter.image(
            id,
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        // A box or lasso being drawn.
        if let Some(a) = &self.area {
            a.paint(&painter);
        }
        drop(rendering);
        let labels = crate::perf::span("3d labels");
        // Section labels and sags, structures' names and the other
        // labels (Sections and Structures › Display options).
        let project = camera.projector(size);
        let screen =
            |p: [f64; 3]| project(p).map(|(q, _)| rect.min + egui::vec2(q[0] as f32, q[1] as f32));
        let eye = camera.eye();
        // Only within the label distance of the eye (not built beyond).
        let near = Some((eye, model.section_display.label_range()));
        let mut placed: Vec<(f64, egui::Pos2, crate::annotations::Note)> =
            crate::annotations::notes(model, case, crate::annotations::View::ThreeD, near)
                .into_iter()
                .chain(crate::annotations::object_notes(
                    model,
                    case,
                    crate::annotations::View::ThreeD,
                    near,
                ))
                .filter_map(|note| {
                    let p = crate::annotations::anchor(&note, rect, screen)?;
                    let d = (0..3).map(|i| (note.at[i] - eye[i]).powi(2)).sum::<f64>();
                    Some((d, p, note))
                })
                .collect();
        // Structures' names first (they are written from afar, the
        // other labels give way to them), then the nearest first.
        let name = |n: &crate::annotations::Note| n.kind != crate::annotations::Kind::Structure;
        placed.sort_by(|a, b| name(&a.2).cmp(&name(&b.2)).then(a.0.total_cmp(&b.0)));
        let _paint = crate::perf::span("3d labels: paint");
        let mut placer = crate::annotations::Placer::default();
        for (_, p, note) in &placed {
            placer.paint(&painter, *p, note);
        }
        drop(labels);
        let _overlays = crate::perf::span("3d overlays");
        // A measured line: on top, its label at its middle.
        if let Some((a, b, label)) = &self.measure
            && let (Some(p), Some(q)) = (screen(*a), screen(*b))
        {
            for (w, c) in [
                (5.0, egui::Color32::from_black_alpha(180)),
                (2.5, egui::Color32::from_rgb(255, 210, 60)),
            ] {
                painter.line_segment([p, q], egui::Stroke::new(w, c));
            }
            for x in [p, q] {
                painter.circle_filled(x, 4.5, egui::Color32::from_rgb(255, 210, 60));
            }
            let mid = p + (q - p) * 0.5;
            let galley = painter.layout_no_wrap(
                label.clone(),
                egui::FontId::proportional(14.0),
                egui::Color32::WHITE,
            );
            let at = mid + egui::vec2(8.0, -8.0 - galley.size().y);
            painter.rect_filled(
                egui::Rect::from_min_size(at, galley.size()).expand(3.0),
                3.0,
                egui::Color32::from_black_alpha(170),
            );
            painter.galley(at, galley, egui::Color32::WHITE);
        }
        // Structures coloured by usage: the legend.
        if !model.usage_marks.is_empty() {
            crate::loads::paint_usage_legend(&painter, rect, model.usage_out_of_date);
        }
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
        // The handles: over everything, the one under the pointer (or
        // dragged) larger and outlined; a drag's way from where it was,
        // the axis it is held to and its coordinates.
        if !spots.is_empty() {
            Self::paint_handles(&self.handles, &painter, &spots, structure, screen);
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
            // Only how to get out; every key: Help › Controls › 3D.
            painter.text(
                rect.center_top() + egui::vec2(0.0, 8.0),
                egui::Align2::CENTER_TOP,
                "Esc: free the mouse",
                egui::FontId::proportional(12.0),
                egui::Color32::WHITE,
            );
        }
        used
    }
}
