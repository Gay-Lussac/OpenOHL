// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The 3D view: the renderer's image of the visible layers, a camera
//! moved with the mouse and the keyboard as the chosen control scheme
//! says ([`Controls`]), and picking.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use eframe::egui;
use eframe::egui_wgpu;
use ohl_core::grid::Tile;
use ohl_pointcloud::octree::NodeCache;
use ohl_render::lod::{ChunkSet, Frustum};
use ohl_render::{Camera, Clip, GpuScene, Renderer};

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
    pub dirty: Dirty,
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
    /// The lines on the ground beside the line (none: not shown).
    pub ground_lines: Option<std::sync::Arc<crate::ground_lines::Drawn>>,
    /// What Checks › Safety volumes built.
    pub safety_volume: crate::safety_volume::Overlay,
    /// The lightning shielding check's drawing.
    pub lightning: crate::lightning::Overlay,
    /// The ice shedding check's drawing (2026-11-02).
    pub ice_shedding: crate::ice_shedding::Overlay,
    /// The magnetic field's isolines, cuts and surfaces.
    pub magnetic_field: crate::field_overlay::Overlay,
    /// The electric field's cuts and surfaces.
    pub electric_field: crate::field_overlay::Overlay,
    /// A wire being strung: its points so far.
    pub draft: Vec<[f64; 3]>,
    /// What a replacement would remove (Replace structure's window open):
    /// polylines drawn red over the line.
    pub removed: Vec<Vec<[f64; 3]>>,
    /// A measuring tool is held: what is under the cursor is found every
    /// frame the pointer moves ([`View3d::hover_use`]).
    pub measuring: bool,
    /// The Move tool is held: the structure being edited's handles can
    /// be dragged (owner: OHL-Design UI review, 2026-10-26 — nothing
    /// moves on a click or drag with Select).
    pub move_held: bool,
    /// What is under the cursor, as a click of the tool button would
    /// find it (a measuring tool held).
    pub hover_use: Option<selection::Use>,
    /// The measuring tools' lines and points.
    pub user_measure: crate::measure::Overlay,
    /// The kept measurement or note under the last tool click (its
    /// annotation's id); taken by the app.
    pub annotation_hit: Option<String>,
    /// A measured line to draw on top, with its label (the distance
    /// between conductors' worst station: d and z).
    pub measure: Option<crate::phase_distance::Measure>,
    /// Wires to draw on top with their labels (under unequal wind: the
    /// pair's two conductors, each at its own wind).
    pub measure_wires: Vec<crate::phase_distance::MeasureWire>,
    /// The check whose result drew `measure` and `measure_wires`.
    pub measure_from: crate::phase_distance::Kind,
    /// The camera was fitted to the line's structures or wires (else to
    /// its alignments or the ground: fitted again when the first
    /// structures come).
    fitted_on_line: bool,
    /// Drag handles of the structure being edited ([`crate::handles`]).
    pub handles: crate::handles::Handles,
    /// The Link tool with an attachment point chosen: the joints it may be
    /// linked to, ringed; set by the application each frame
    /// ([`crate::link_tool`]).
    pub link: Option<crate::link_tool::LinkOverlay>,
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
    /// Only what lies in this box is drawn, culled and picked (a cut's
    /// panel, [`crate::xsection`]); none: everything.
    pub clip: Option<Clip>,
    /// The chunk indexes, shared by the views of one line (the main view
    /// and the cuts' panels index a layer once).
    pub indexes: SharedIndexes,
    /// The slabs of the cuts open, outlined (the main view): set by the
    /// application each frame.
    pub cut_outlines: Vec<CutOutline>,
    /// The cut whose outline the last click picked; taken by the app
    /// (its panel comes to the front).
    pub picked_cut: Option<u32>,
    /// Where a click under the pointer would pick (the snap marker,
    /// [`ohl_ui::snap_marker`]): found with the click's own search, again
    /// only when the pointer, the view or the model changed.
    pub snap_hover: ohl_ui::snap_marker::Hover,
}

/// A cut's slab outlined in the main 3D view: its tab's number, its
/// box's eight corners (the four of [`Clip::corners`] at the bottom, then
/// at the top), its label.
#[derive(Debug, Clone, PartialEq)]
pub struct CutOutline {
    /// The cut's tab number.
    pub id: u32,
    /// The box's corners: bottom four, then top four.
    pub corners: [[f64; 3]; 8],
    /// Its label ("Cut T3").
    pub label: String,
}

impl CutOutline {
    /// The box's twelve edges.
    pub fn edges(&self) -> impl Iterator<Item = ([f64; 3], [f64; 3])> + '_ {
        let c = &self.corners;
        (0..4).flat_map(move |k| {
            let n = (k + 1) % 4;
            [(c[k], c[n]), (c[4 + k], c[4 + n]), (c[k], c[4 + k])]
        })
    }
}

/// Chunk indexes shared by views: by layer, the index, the weather case
/// and the layer's change count it was made for ([`Dirty`]).
pub type SharedIndexes = Rc<RefCell<HashMap<Layer, (u64, usize, Arc<crate::chunks::ChunkIndex>)>>>;

thread_local! {
    /// How often each layer was marked for rebuilding (this thread's
    /// views): an index made at one count is current until the next.
    static CHANGES: RefCell<HashMap<Layer, u64>> = RefCell::new(HashMap::new());
}

/// How often `layer` was marked for rebuilding.
fn changes(layer: Layer) -> u64 {
    CHANGES.with(|c| c.borrow().get(&layer).copied().unwrap_or(0))
}

/// The layers a view rebuilds before its next frame. Every layer marked
/// is also noted in a log the application hands on to the cuts' panels
/// ([`Dirty::take_log`], [`Dirty::receive`]).
#[derive(Debug, Default, Clone)]
pub struct Dirty {
    set: HashSet<Layer>,
    log: HashSet<Layer>,
}

impl Dirty {
    /// Marks `layer` (its content changed): rebuilt before the next frame.
    pub fn insert(&mut self, layer: Layer) -> bool {
        CHANGES.with(|c| *c.borrow_mut().entry(layer).or_default() += 1);
        self.log.insert(layer);
        self.set.insert(layer)
    }

    /// Marks every layer of `layers`.
    pub fn extend(&mut self, layers: impl IntoIterator<Item = Layer>) {
        for l in layers {
            self.insert(l);
        }
    }

    /// Marks `layers` handed on from another view (no new change).
    pub fn receive(&mut self, layers: impl IntoIterator<Item = Layer>) {
        self.set.extend(layers);
    }

    /// The layers marked since the last call (to hand on).
    pub fn take_log(&mut self) -> HashSet<Layer> {
        std::mem::take(&mut self.log)
    }

    /// Whether `layer` is marked.
    #[cfg(test)]
    pub fn contains(&self, layer: &Layer) -> bool {
        self.set.contains(layer)
    }

    /// Whether nothing is marked.
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// Unmarks every layer.
    #[cfg(test)]
    pub fn clear(&mut self) {
        self.set.clear();
    }

    /// The layers marked, unmarked.
    pub fn drain(&mut self) -> impl Iterator<Item = Layer> + '_ {
        self.set.drain()
    }
}

/// `camera`'s projector for a view of `size` that shows only what lies in
/// `clip`: points outside it are not projected (not picked, not
/// labelled).
pub fn clipped_projector(
    camera: &Camera,
    size: [f64; 2],
    clip: Option<Clip>,
) -> impl Fn([f64; 3]) -> Option<([f64; 2], f64)> {
    let project = camera.projector(size);
    move |p| {
        if clip.is_some_and(|c| !c.contains(p)) {
            return None;
        }
        project(p)
    }
}

/// A layer drawn in chunks.
struct Chunked {
    index: Arc<crate::chunks::ChunkIndex>,
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

/// The distance from `p` to segment `a`–`b` (points).
fn to_segment(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    let t = if len2 > 0.0 {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(a + t * ab)
}

/// The points of a polyline on the screen less than a pixel from the last
/// kept left out (the last point always kept): what a long wire drawn as
/// an overlay needs at most (display only).
pub(crate) fn thin_on_screen(points: impl Iterator<Item = egui::Pos2>) -> Vec<egui::Pos2> {
    let mut out: Vec<egui::Pos2> = Vec::new();
    let mut last = None;
    for p in points {
        last = Some(p);
        if out.last().is_none_or(|q| q.distance_sq(p) >= 1.0) {
            out.push(p);
        }
    }
    if let (Some(p), Some(q)) = (last, out.last())
        && *q != p
    {
        out.push(p);
    }
    out
}

/// Points along each edge of a cut's outline (long edges are drawn
/// between their points in front of the eye).
const OUTLINE_SAMPLES: u32 = 24;

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

    /// Segments, points and triangles the chunked layers hold on the GPU,
    /// every level built (benchmark).
    #[cfg(test)]
    pub fn held_counts(&self) -> [u64; 3] {
        let mut n = [0u64; 3];
        for c in self.chunks.values() {
            for h in [c.set.held(), c.near.held()] {
                for k in 0..3 {
                    n[k] += h[k];
                }
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
        let project = clipped_projector(camera, size, self.clip);
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

    /// Draws the Link tool's joints and the joint under the pointer.
    fn paint_link(
        link: &crate::link_tool::LinkOverlay,
        painter: &egui::Painter,
        screen: impl Fn([f64; 3]) -> Option<egui::Pos2>,
    ) {
        // Only attach joints are ringed (only they take a link).
        let green = egui::Color32::from_rgb(120, 240, 140);
        for j in link.joints.iter().filter(|j| j.attach) {
            if let Some(q) = screen(j.at) {
                painter.circle_stroke(q, 9.0, egui::Stroke::new(2.0, green));
            }
        }
        let Some(p) = screen(link.point) else { return };
        painter.circle_stroke(
            p,
            7.0,
            egui::Stroke::new(2.5, egui::Color32::from_rgb(255, 150, 60)),
        );
        let Some(id) = &link.hovered else { return };
        let (Some(j), Some(text)) = (link.joints.iter().find(|j| &j.id == id), link.hover_text())
        else {
            return;
        };
        if let Some(q) = screen(j.at) {
            if j.attach {
                painter.circle_stroke(q, 12.0, egui::Stroke::new(3.0, egui::Color32::WHITE));
                painter.line_segment([q, p], egui::Stroke::new(2.0, egui::Color32::WHITE));
            }
            Self::label_inside(painter, q + egui::vec2(14.0, -26.0), text);
        }
    }

    /// A line of text on a dark box, kept inside the painter's area.
    fn label_inside(painter: &egui::Painter, at: egui::Pos2, text: String) {
        let galley =
            painter.layout_no_wrap(text, egui::FontId::proportional(13.0), egui::Color32::WHITE);
        let clip = painter.clip_rect().shrink(4.0);
        let size = galley.size();
        let at = egui::pos2(
            at.x.min(clip.right() - size.x).max(clip.left()),
            at.y.min(clip.bottom() - size.y).max(clip.top()),
        );
        painter.rect_filled(
            egui::Rect::from_min_size(at, size).expand(3.0),
            3.0,
            egui::Color32::from_black_alpha(190),
        );
        painter.galley(at, galley, egui::Color32::WHITE);
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

    /// The point cloud streamed by node read again (its classes or
    /// colours changed in another view), without marking a change.
    pub fn forget_stream(&mut self) {
        self.points = None;
    }

    /// Sets the camera (kept: not fitted to the line again).
    pub fn set_camera(&mut self, camera: Camera) {
        self.camera = Some(camera);
        self.fitted_on_line = true;
    }

    /// The index of chunked `layer` in `case`: the shared one when made
    /// since the layer last changed, else made now (and shared).
    fn index(
        indexes: &SharedIndexes,
        model: &Model,
        layer: Layer,
        case: usize,
    ) -> Arc<crate::chunks::ChunkIndex> {
        let now = changes(layer);
        if let Some((n, c, index)) = indexes.borrow().get(&layer)
            && *n == now
            && *c == case
        {
            return index.clone();
        }
        let index = Arc::new(crate::chunks::ChunkIndex::new(model, layer, case));
        indexes
            .borrow_mut()
            .insert(layer, (now, case, index.clone()));
        index
    }

    /// Draws the outlines of the cuts' slabs (main view): thin, dashed at
    /// the top, the label at a top corner.
    fn paint_outlines(
        outlines: &[CutOutline],
        painter: &egui::Painter,
        screen: impl Fn([f64; 3]) -> Option<egui::Pos2>,
    ) {
        let col = egui::Color32::from_rgb(120, 200, 255);
        for o in outlines {
            for (a, b) in o.edges() {
                let pts: Vec<egui::Pos2> = (0..=OUTLINE_SAMPLES)
                    .filter_map(|n| {
                        let t = f64::from(n) / f64::from(OUTLINE_SAMPLES);
                        screen([0, 1, 2].map(|k| a[k] + t * (b[k] - a[k])))
                    })
                    .collect();
                if pts.len() > 1 {
                    painter.add(egui::Shape::line(pts, egui::Stroke::new(1.5, col)));
                }
            }
            if let Some(p) = screen(o.corners[4]) {
                painter.text(
                    p + egui::vec2(4.0, -4.0),
                    egui::Align2::LEFT_BOTTOM,
                    &o.label,
                    egui::FontId::proportional(12.0),
                    col,
                );
            }
        }
    }

    /// The cut whose outline passes within `radius` points of `at`.
    fn outline_at(
        outlines: &[CutOutline],
        at: [f64; 2],
        radius: f64,
        project: impl Fn([f64; 3]) -> Option<([f64; 2], f64)>,
    ) -> Option<u32> {
        let at = egui::pos2(at[0] as f32, at[1] as f32);
        let mut best: Option<(f32, u32)> = None;
        for o in outlines {
            for (a, b) in o.edges() {
                let pts: Vec<egui::Pos2> = (0..=OUTLINE_SAMPLES)
                    .filter_map(|n| {
                        let t = f64::from(n) / f64::from(OUTLINE_SAMPLES);
                        project([0, 1, 2].map(|k| a[k] + t * (b[k] - a[k])))
                            .map(|(q, _)| egui::pos2(q[0] as f32, q[1] as f32))
                    })
                    .collect();
                for w in pts.windows(2) {
                    let d = to_segment(at, w[0], w[1]);
                    if d <= radius as f32 && best.is_none_or(|b| d < b.0) {
                        best = Some((d, o.id));
                    }
                }
            }
        }
        best.map(|b| b.1)
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
        self.fit_camera(model, case, selection);
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
        let clip = self.clip;
        let still = self.camera.expect("fitted above");
        let project_handle = clipped_projector(&still, size, clip);
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
            && self.move_held
            && self.handles.hovered.is_some()
            && self.handles.drag.is_none()
            && response.hovered()
            && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary));
        let fr = Frame {
            model,
            case,
            visible,
            snaps,
            lod,
            rect,
            size,
            centre,
            clip,
        };
        self.press_handle(
            &response,
            &fr,
            structure,
            pointer,
            pressed_on_handle,
            on_handle,
        );
        self.follow_handle_drag(ui, &fr, structure, pointer, primary_down, on_handle);
        if self.handles.swallow && !primary_down {
            self.handles.swallow = false;
        }
        let handle_drag = self.handles.drag.is_some() || self.handles.swallow;
        // Which button's click selects and which uses the held tool (the
        // Select tool held or not; drags are not clicks).
        let (select_button, tool_button) = crate::controls::click_buttons(select_held);
        if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
            self.left_press_on_handle = pressed_on_handle;
        }
        let clicks = if self.captured {
            self.captured_input(ui, centre, select_button, tool_button)
        } else {
            self.pointer_input(
                ui,
                &response,
                &fr,
                pressed_on_handle,
                handle_drag,
                (select_button, tool_button),
            )
        };
        self.fly(ui, &response, handle_drag);
        let used = self.pick_and_use(ui, &response, &fr, selection, pointer, clicks);
        self.rebuild_layers(ui, rs, &fr, selection);
        let frustum = self.build_chunks(ui, rs, &fr);
        self.stream_cloud(ui, rs, &fr, frustum);
        let painter = self.render_image(ui, rs, &fr);
        self.paint_over(painter, &fr, selection, spots, structure);
        used
    }

    /// Fits the camera when there is none (and again to the first
    /// structures of a line that had none).
    fn fit_camera(&mut self, model: &Model, case: usize, selection: &Selection) {
        // The first structures of a line that had none: fitted to them.
        if self.camera.is_some()
            && !self.fitted_on_line
            && self.clip.is_none()
            && !model.line.structures.is_empty()
        {
            self.camera = None;
        }
        let fitted_on_line = &mut self.fitted_on_line;
        self.camera.get_or_insert_with(|| {
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
    }

    /// A press on the drag handle under the pointer: its drag starts, or
    /// is refused and said why.
    fn press_handle(
        &mut self,
        response: &egui::Response,
        fr: &Frame<'_>,
        structure: Option<&ohl_line::Structure>,
        pointer: Option<egui::Pos2>,
        pressed_on_handle: bool,
        on_handle: impl Fn(
            [f64; 2],
            Option<&ohl_structure_edit::Handle>,
        ) -> Option<(f64, crate::handles::Spot)>,
    ) {
        let rect = fr.rect;
        let local = fr.local();
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
                        onto: None,
                    });
                    response.request_focus();
                }
                (None, _) => {}
            }
        }
    }

    /// A handle being dragged: held to an axis or plane by the keys,
    /// followed, snapped, and ended when the button is released.
    fn follow_handle_drag(
        &mut self,
        ui: &egui::Ui,
        fr: &Frame<'_>,
        structure: Option<&ohl_line::Structure>,
        pointer: Option<egui::Pos2>,
        primary_down: bool,
        on_handle: impl Fn(
            [f64; 2],
            Option<&ohl_structure_edit::Handle>,
        ) -> Option<(f64, crate::handles::Spot)>,
    ) {
        let size = fr.size;
        let local = fr.local();
        let camera = self.camera.as_ref().expect("fitted above");
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
                    d.onto = onto.filter(|_| !unsnapped).map(|s| s.handle);
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
                        onto: d.onto,
                    });
                }
            }
            ui.ctx().request_repaint();
        } else if self.handles.drag.is_some() {
            // Its structure went (or is no longer edited).
            self.handles.drag = None;
        }
    }

    /// The mouse captured (first person): its movement turns the view,
    /// clicks aim with the crosshair, Ctrl + wheel sets the speed.
    fn captured_input(
        &mut self,
        ui: &egui::Ui,
        centre: [f64; 2],
        select_button: egui::PointerButton,
        tool_button: egui::PointerButton,
    ) -> Clicks {
        let camera = self.camera.as_mut().expect("fitted above");
        let mut pick_at: Option<([f64; 2], bool)> = None;
        let mut use_at: Option<[f64; 2]> = None;
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
            self.controls.speed = (self.controls.speed * (notches * 0.1).exp()).clamp(1.0, 2000.0);
        }
        Clicks {
            pick_at,
            area_done: None,
            use_at,
        }
    }

    /// The mouse free: a press may capture it, clicks select or use the
    /// tool, Alt + drag draws an area, drags and the wheel move the view.
    fn pointer_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        fr: &Frame<'_>,
        pressed_on_handle: bool,
        handle_drag: bool,
        buttons: (egui::PointerButton, egui::PointerButton),
    ) -> Clicks {
        let size = fr.size;
        let local = fr.local();
        let (select_button, tool_button) = buttons;
        let camera = self.camera.as_mut().expect("fitted above");
        let mut pick_at: Option<([f64; 2], bool)> = None;
        let mut use_at: Option<[f64; 2]> = None;
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
        let acts =
            |b: egui::PointerButton| !(self.controls.capture && b == egui::PointerButton::Primary);
        // A left-click on a drag handle (released without moving) only
        // selects, also with a tool held.
        let on_handle = self.left_press_on_handle;
        let tool_click = acts(tool_button)
            && response.clicked_by(tool_button)
            && !(tool_button == egui::PointerButton::Primary && on_handle);
        let select_click = acts(select_button)
            && (response.clicked_by(select_button) || response.double_clicked_by(select_button));
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
            crate::area::frame(&mut self.area, ui, response, select_button)
        };
        let area_done = done;
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
        Clicks {
            pick_at,
            area_done,
            use_at,
        }
    }

    /// Flies with the keys held.
    fn fly(&mut self, ui: &egui::Ui, response: &egui::Response, handle_drag: bool) {
        let camera = self.camera.as_mut().expect("fitted above");
        // Not flying while a handle is dragged (Shift + Z holds a plane),
        // nor while Alt is held or an area drawn (Alt + Shift: a lasso).
        let area_keys = self.area.is_some() || ui.input(|i| i.modifiers.alt);
        // WASD only while the mouse is locked in first person (owner,
        // 2026-10-25: never while typing in a field elsewhere); without the
        // first-person scheme, over the view and no field being typed in.
        let flying = if self.controls.capture {
            self.captured
        } else {
            (response.hovered() || response.has_focus()) && !ui.ctx().egui_wants_keyboard_input()
        };
        if !handle_drag && !area_keys && flying {
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
    }

    /// What the frame's clicks and the pointer find: the Link tool's
    /// joint, the points of an area drawn, the tool's use (returned), what
    /// is under the cursor for a measuring tool, the selection picked.
    /// (One part: all of it reads the LiDAR points drawn, borrowed once.)
    fn pick_and_use(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        fr: &Frame<'_>,
        selection: &mut Selection,
        pointer: Option<egui::Pos2>,
        clicks: Clicks,
    ) -> Option<selection::Use> {
        let Frame {
            model,
            case,
            visible,
            snaps,
            size,
            centre,
            clip,
            ..
        } = *fr;
        let Clicks {
            pick_at,
            area_done,
            use_at,
        } = clicks;
        // The snap distance (the setting; the crosshair 4 px less, as
        // before it was one).
        let marker = ohl_ui::snap_marker::Settings::get(ui.ctx());
        let radius = marker.radius(if self.captured { 4.0 } else { 0.0 });
        let use_at = self.link_hover(fr, pointer, radius, use_at);
        let local = fr.local();
        let camera = self.camera.as_mut().expect("fitted above");
        let lidar = Self::lidar_shown(&self.points, &self.chunks, model, visible);
        if let Some(area) = area_done {
            let started = std::time::Instant::now();
            let project = clipped_projector(camera, size, clip);
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
        // What a click of the tool button at `at` (view pixels) finds, and
        // the point it snapped to (view pixels).
        let use_point = |at: [f64; 2]| {
            let project = clipped_projector(camera, size, clip);
            let (target, _, snapped) =
                selection::pick_shown_at(model, case, snaps, at, radius, &project, &lidar);
            // In a cut's panel: only ground within its slab.
            let ground = camera
                .ray(at, size)
                .and_then(|(a, d)| model.ground_hit(a, d, camera.target[2]))
                .filter(|p| clip.is_none_or(|c| c.contains(*p)));
            let on = selection::on_span(model, case, &target, at, |p| project(p).map(|(q, _)| q));
            (
                selection::Use {
                    target,
                    at: ground,
                    on,
                },
                snapped,
            )
        };
        if let Some(at) = use_at {
            let project = clipped_projector(camera, size, clip);
            self.annotation_hit = self.user_measure.hit(
                &|p| project(p).map(|(q, _)| egui::pos2(q[0] as f32, q[1] as f32)),
                egui::pos2(at[0] as f32, at[1] as f32),
                Some((camera.eye(), model.section_display.label_range())),
            );
        }
        let used = use_at.map(|at| {
            let u = use_point(at).0;
            log::info!(
                "3D view: tool click (mouse captured: {}) on {}",
                self.captured,
                u.target.describe()
            );
            u
        });
        // A measuring tool held: what is under the cursor (the crosshair
        // when the mouse is captured), found again when it moved.
        let hover_at = if self.captured {
            Some(centre)
        } else {
            response.hover_pos().map(local)
        };
        let moved = ui.input(|i| i.pointer.is_moving()) || self.captured;
        // The snap marker: found again only when the pointer moved or the
        // view, the model or the settings changed (the image's key holds
        // the layers and the model's state).
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (self.image_key, self.generation, case, self.captured).hash(&mut h);
            let c = &*camera;
            for v in [
                c.target[0],
                c.target[1],
                c.target[2],
                c.distance,
                c.yaw,
                c.pitch,
                size[0],
                size[1],
            ] {
                v.to_bits().hash(&mut h);
            }
            snaps.0.hash(&mut h);
            marker.distance.to_bits().hash(&mut h);
            h.finish()
        };
        let rect_min = fr.rect.min;
        let to_pos = move |q: [f64; 2]| rect_min + egui::vec2(q[0] as f32, q[1] as f32);
        let hover_pos = hover_at.map(to_pos);
        let marker_stale = marker.show && hover_pos.is_some_and(|p| self.snap_hover.stale(p, key));
        let mut hover_found = None;
        self.hover_use = match hover_at {
            Some(at) if self.measuring && (moved || self.hover_use.is_none() || marker_stale) => {
                let started = std::time::Instant::now();
                let (u, snapped) = use_point(at);
                log::trace!("3D view: measuring hover in {:.1?}", started.elapsed());
                hover_found = Some(snapped);
                Some(u)
            }
            Some(_) if self.measuring => self.hover_use.take(),
            _ => None,
        };
        match (hover_at, hover_pos) {
            (Some(at), Some(p)) if marker.show => {
                if marker_stale {
                    // One search for the marker and the measuring hover.
                    let snapped = hover_found.unwrap_or_else(|| {
                        let started = std::time::Instant::now();
                        let project = clipped_projector(camera, size, clip);
                        let s = selection::pick_shown_at(
                            model, case, snaps, at, radius, &project, &lidar,
                        )
                        .2;
                        log::trace!("3D view: snap marker in {:.1?}", started.elapsed());
                        s
                    });
                    self.snap_hover.found(p, key, snapped.map(to_pos));
                }
            }
            _ => self.snap_hover.clear(),
        }
        if let Some((at, double)) = pick_at {
            let cam = *camera;
            let project = clipped_projector(&cam, size, clip);
            let started = std::time::Instant::now();
            let (picked, picked_case) =
                selection::pick_shown(model, case, snaps, at, radius, &project, &lidar);
            // Nothing there but a cut's outline: its panel comes forward.
            if picked == Selection::None {
                self.picked_cut = Self::outline_at(&self.cut_outlines, at, radius, &project);
            }
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
            selection::clicked(ui.ctx(), &picked);
            if picked != *selection {
                *selection = picked;
            }
            if double && let Some(f) = selection.focus(model, case) {
                camera.target = f;
                camera.distance = camera.distance.min(150.0);
            }
        }
        used
    }

    /// The Link tool: the joint under the pointer; a tool click on one
    /// is the link's (the click left, none then).
    fn link_hover(
        &mut self,
        fr: &Frame<'_>,
        pointer: Option<egui::Pos2>,
        radius: f64,
        mut use_at: Option<[f64; 2]>,
    ) -> Option<[f64; 2]> {
        let Frame {
            size, centre, clip, ..
        } = *fr;
        let local = fr.local();
        let camera = self.camera.as_ref().expect("fitted above");
        // The Link tool: the joint under the pointer (the crosshair in
        // first person); the tool click on one links the point there.
        if let Some(link) = self.link.as_mut() {
            let project = clipped_projector(camera, size, clip);
            let at = if self.captured {
                Some(centre)
            } else {
                pointer.map(local)
            };
            link.hovered = at.and_then(|at| {
                link.joints
                    .iter()
                    .filter_map(|j| {
                        let (q, _) = project(j.at)?;
                        let d = (q[0] - at[0]).hypot(q[1] - at[1]);
                        (d <= radius).then_some((d, j.id.clone()))
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(_, id)| id)
            });
            if use_at.is_some() && link.click().is_some() {
                use_at = None;
            }
        }
        use_at
    }

    /// Makes the changed layers again (the chunked ones indexed, the
    /// point cloud opened).
    fn rebuild_layers(
        &mut self,
        ui: &egui::Ui,
        rs: &egui_wgpu::RenderState,
        fr: &Frame<'_>,
        selection: &Selection,
    ) {
        let Frame {
            model, case, lod, ..
        } = *fr;
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
                let index = Self::index(&self.indexes, model, layer, case);
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
                Layer::SafetyVolume => {
                    crate::safety_volume::scene(model.origin, &self.safety_volume)
                }
                Layer::Lightning => crate::lightning::scene(model.origin, &self.lightning),
                Layer::IceShedding => crate::ice_shedding::scene(model.origin, &self.ice_shedding),
                Layer::MagneticField => {
                    crate::field_overlay::scene(model.origin, &self.magnetic_field)
                }
                Layer::ElectricField => {
                    crate::field_overlay::scene(model.origin, &self.electric_field)
                }
                Layer::Draft => scene::draft(model.origin, &self.draft, &self.removed),
                Layer::GroundLines => crate::ground_lines::scene(
                    model.origin,
                    self.ground_lines.as_deref(),
                    &model.ground_lines,
                ),
                _ => scene::layer(model, layer, case, selection),
            };
            self.gpu.insert(layer, GpuScene::new(device, &s));
        }
        drop(rebuild);
    }

    /// The chunks: planned for this view, the missing ones built within
    /// the frame's budget; the view's frustum.
    fn build_chunks(
        &mut self,
        ui: &egui::Ui,
        rs: &egui_wgpu::RenderState,
        fr: &Frame<'_>,
    ) -> Frustum {
        let Frame {
            model,
            visible,
            lod,
            rect,
            clip,
            ..
        } = *fr;
        let device = &rs.device;
        let camera = self.camera.as_ref().expect("fitted above");
        // Chunks: the level of each by its distance, the missing ones built
        // nearest first within the frame's budget.
        let planning = crate::perf::span("3d chunk plan");
        let aspect = f64::from(rect.width()) / f64::from(rect.height().max(1.0));
        let frustum = Frustum::new(camera, aspect).clipped(clip);
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
        frustum
    }

    /// The point cloud's nodes for this view.
    fn stream_cloud(
        &mut self,
        ui: &egui::Ui,
        rs: &egui_wgpu::RenderState,
        fr: &Frame<'_>,
        frustum: Frustum,
    ) {
        let Frame {
            model,
            visible,
            lod,
            rect,
            ..
        } = *fr;
        let device = &rs.device;
        let camera = self.camera.as_ref().expect("fitted above");
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
    }

    /// Renders the visible layers (the image kept while nothing changes)
    /// and shows the image; the view's painter.
    fn render_image(
        &mut self,
        ui: &egui::Ui,
        rs: &egui_wgpu::RenderState,
        fr: &Frame<'_>,
    ) -> egui::Painter {
        let Frame {
            visible,
            rect,
            clip,
            ..
        } = *fr;
        let device = &rs.device;
        let camera = self.camera.as_ref().expect("fitted above");
        let rendering = crate::perf::span("3d render");
        let renderer = self.renderer.get_or_insert_with(|| Renderer::new(device));
        renderer.set_clip(clip);
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
            (px, self.generation, c.ortho).hash(&mut h);
            clip.map(|c| {
                [
                    c.origin[0],
                    c.origin[1],
                    c.ahead[0],
                    c.ahead[1],
                    c.depth,
                    c.width,
                ]
                .map(f64::to_bits)
            })
            .hash(&mut h);
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
        painter
    }

    /// What is written and drawn over the image: the labels, the measured
    /// lines, a result's case, the overlays, the handles, the crosshair.
    fn paint_over(
        &self,
        painter: egui::Painter,
        fr: &Frame<'_>,
        selection: &Selection,
        spots: Vec<crate::handles::Spot>,
        structure: Option<&ohl_line::Structure>,
    ) {
        let Frame {
            model,
            case,
            rect,
            size,
            clip,
            ..
        } = *fr;
        let camera = self.camera.as_ref().expect("fitted above");
        let labels = crate::perf::span("3d labels");
        // Section labels and sags, structures' names and the other
        // labels (Sections and Line › Structures › Display options).
        let project = clipped_projector(camera, size, clip);
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
        self.paint_measure(&painter, screen);
        self.paint_measure_wires(&painter, screen);
        paint_result_case(&painter, model, screen);
        // The kept ones' labels within the label distance, as the others.
        self.user_measure.paint(&painter, &|p| screen(p), near);
        // Structures coloured by usage: the legend.
        if !model.usage_marks.is_empty() {
            crate::loads::paint_usage_legend(&painter, rect, model.usage_out_of_date);
        }
        // The cuts' slabs (outlined in the main view).
        if !self.cut_outlines.is_empty() {
            let whole = camera.projector(size);
            Self::paint_outlines(&self.cut_outlines, &painter, |p| {
                whole(p).map(|(q, _)| rect.min + egui::vec2(q[0] as f32, q[1] as f32))
            });
        }
        // Label of the selection.
        if let Some(f) = selection.focus(model, case)
            && let Some((q, _)) = project(f)
        {
            painter.text(
                rect.min + egui::vec2(q[0] as f32 + 10.0, q[1] as f32 - 10.0),
                egui::Align2::LEFT_BOTTOM,
                selection.describe(),
                egui::FontId::proportional(14.0),
                egui::Color32::WHITE,
            );
        }
        // The Link tool: the joints the point may be linked to ringed
        // (attach joints brighter), the one under the pointer joined to
        // the point, with the offset and its moment arm.
        if let Some(link) = &self.link {
            Self::paint_link(link, &painter, screen);
        }
        // The handles: over everything, the one under the pointer (or
        // dragged) larger and outlined; a drag's way from where it was,
        // the axis it is held to and its coordinates.
        if !spots.is_empty() {
            Self::paint_handles(&self.handles, &painter, &spots, structure, screen);
        }
        if self.captured {
            paint_crosshair(&painter, rect);
        }
        // Where a click would pick, on top of everything.
        self.snap_hover.paint(&painter);
    }

    /// The measured line, its label at its middle.
    fn paint_measure(
        &self,
        painter: &egui::Painter,
        screen: impl Fn([f64; 3]) -> Option<egui::Pos2>,
    ) {
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
    }

    /// The wires measured at their own wind, each labelled at its middle.
    fn paint_measure_wires(
        &self,
        painter: &egui::Painter,
        screen: impl Fn([f64; 3]) -> Option<egui::Pos2>,
    ) {
        // Wires measured at their own wind: on top, each labelled at its
        // middle.
        for (k, (points, label)) in self.measure_wires.iter().enumerate() {
            let colour = if k == 0 {
                egui::Color32::from_rgb(80, 220, 255)
            } else {
                egui::Color32::from_rgb(255, 110, 220)
            };
            let shown: Vec<Option<egui::Pos2>> = points.iter().map(|p| screen(*p)).collect();
            // Each run in view one polyline, its points less than a pixel
            // apart on the screen left out (display only: the points
            // measured stay as they are; 2026-11-04, a shape per segment
            // before).
            for run in shown.split(Option::is_none) {
                let line = thin_on_screen(run.iter().flatten().copied());
                if line.len() < 2 {
                    continue;
                }
                for (w, c) in [(4.5, egui::Color32::from_black_alpha(170)), (2.0, colour)] {
                    painter.add(egui::Shape::line(line.clone(), egui::Stroke::new(w, c)));
                }
            }
            if let Some(Some(mid)) = shown.get(shown.len() / 2) {
                let galley =
                    painter.layout_no_wrap(label.clone(), egui::FontId::proportional(13.0), colour);
                let at = *mid + egui::vec2(8.0, 6.0 + 18.0 * k as f32);
                painter.rect_filled(
                    egui::Rect::from_min_size(at, galley.size()).expand(3.0),
                    3.0,
                    egui::Color32::from_black_alpha(170),
                );
                painter.galley(at, galley, colour);
            }
        }
    }
}

/// What a frame's parts share: the model and the case shown, the layers
/// visible, the kinds picked, the levels of detail; where the view is,
/// its size and centre in points, the box it is clipped to.
#[derive(Clone, Copy)]
struct Frame<'a> {
    model: &'a Model,
    case: usize,
    visible: &'a HashSet<Layer>,
    snaps: &'a selection::Snaps,
    lod: &'a crate::lod::LodSettings,
    rect: egui::Rect,
    size: [f64; 2],
    centre: [f64; 2],
    clip: Option<Clip>,
}

impl Frame<'_> {
    /// A position on the screen in the view's own points (from its top
    /// left).
    fn local(self) -> impl Fn(egui::Pos2) -> [f64; 2] + Copy + use<> {
        let rect = self.rect;
        move |p: egui::Pos2| [f64::from(p.x - rect.min.x), f64::from(p.y - rect.min.y)]
    }
}

/// What the pointer asked for in a frame: a pick (where, a double-click
/// or not), a box or lasso drawn (released this frame), a click of the
/// tool button (where).
struct Clicks {
    pick_at: Option<([f64; 2], bool)>,
    area_done: Option<crate::area::Area>,
    use_at: Option<[f64; 2]>,
}

/// A check result's weather case said beside its finding, its wire at
/// the finding's wind dashed.
fn paint_result_case(
    painter: &egui::Painter,
    model: &Model,
    screen: impl Fn([f64; 3]) -> Option<egui::Pos2>,
) {
    // A check result's weather case (and wind), said beside its
    // finding in its wire's colour without a box (owner, 2026-10-27:
    // view notes in the object's colour); its wire at the finding's
    // wind dashed, when the case drawn does not show it.
    if let Some(rc) = &model.result_case {
        let [r, g, b] = rc.colour;
        let colour = egui::Color32::from_rgb(r, g, b);
        for w in &rc.wires {
            let shown: Vec<Option<egui::Pos2>> = w.iter().map(|p| screen(*p)).collect();
            for run in shown.split(Option::is_none) {
                let points: Vec<egui::Pos2> = run.iter().flatten().copied().collect();
                if points.len() > 1 {
                    painter.extend(egui::Shape::dashed_line(
                        &points,
                        egui::Stroke::new(2.0, colour),
                        8.0,
                        5.0,
                    ));
                }
            }
        }
        if let Some(at) = screen(rc.at) {
            let text = if rc.solving() {
                format!("{} (its wind: solving…)", rc.says)
            } else if rc.wires.is_empty() {
                rc.says.clone()
            } else {
                format!("{} (dashed: the wire at that wind)", rc.says)
            };
            painter.text(
                at + egui::vec2(10.0, -10.0),
                egui::Align2::LEFT_BOTTOM,
                text,
                egui::FontId::proportional(14.0),
                colour,
            );
        }
    }
}

/// The crosshair of the captured mouse, and how to free it.
fn paint_crosshair(painter: &egui::Painter, rect: egui::Rect) {
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
#[cfg(test)]
mod thin_tests {
    use super::thin_on_screen;
    use eframe::egui::pos2;

    /// A wire of 10 000 points across 100 px: about 100 kept, its ends
    /// kept; a short one as it is.
    #[test]
    fn a_long_wire_is_drawn_with_a_point_per_pixel() {
        let many = (0..10_000).map(|k| pos2(k as f32 * 0.01, 5.0));
        let kept = thin_on_screen(many);
        assert!(kept.len() <= 102, "{}", kept.len());
        assert_eq!(kept[0], pos2(0.0, 5.0));
        assert_eq!(*kept.last().unwrap(), pos2(99.99, 5.0));
        let few = [pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(20.0, 5.0)];
        assert_eq!(thin_on_screen(few.into_iter()), few.to_vec());
    }
}
