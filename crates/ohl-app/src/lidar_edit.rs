// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Edit tool on a LiDAR point: its window changes the point's
//! coordinates and class (from the project's class scheme, by name). The
//! point clicked is drawn to the millimetre; the window finds the stored
//! point behind it and applies one session command
//! ([`ohl_session::Command::ChangePoints`]: logged, undoable as one step,
//! sent to the server when joined). Viewers see the window read-only.
//!
//! After a change (its undo, someone else's), the views read the points
//! again, the clearances are checked again, and the ground built from the
//! cloud's ground classes when the project opened is built again
//! ([`frame`]); the checks run on demand (vegetation, as-built wires,
//! vertical and radial clearances) read the cloud when they run.

use std::collections::BTreeSet;

use eframe::egui;
use ohl_pointcloud::edit::StoredPoint;
use ohl_session::Command;
use ohl_session::points::{CloudPoint, PointChange};

use crate::model::{LidarPoint, Model};
use crate::selection::Selection;

/// Distance within which the stored point behind a point drawn is looked
/// for, m (drawn coordinates are rounded to the millimetre).
const FIND_WITHIN: f64 = 0.002;

/// Points of the cloud changed (an edit, its undo, someone else's).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PointsChanged {
    /// The classes they had and have.
    pub classes: BTreeSet<u8>,
    /// Each point as drawn before and after (a selection follows).
    pub moved: Vec<(LidarPoint, LidarPoint)>,
}

/// One point in the window: as stored, and as edited.
#[derive(Debug, Clone, PartialEq)]
pub struct EditedPoint {
    /// The point exactly as stored.
    pub stored: StoredPoint,
    /// Its index in its block.
    pub index: u64,
    /// Coordinates as edited, m.
    pub xyz: [f64; 3],
    /// Class as edited.
    pub class: u8,
}

/// The window of the Edit tool on LiDAR points.
#[derive(Debug, Clone, PartialEq)]
pub struct PointsForm {
    /// The point cloud (its name in the project file).
    pub cloud: String,
    /// Its coordinate scale, m.
    pub scale: f64,
    /// Its chunk size in units of the scale (a point's block: its chunk
    /// and class).
    pub units: i64,
    /// The points.
    pub points: Vec<EditedPoint>,
    /// Classes of the points selected left as they are (several points:
    /// the class filter).
    pub skip: BTreeSet<u8>,
    /// Why it cannot be changed (a viewer, a cloud outside the project):
    /// shown read-only.
    pub read_only: Option<String>,
}

/// Points selected up to this many are looked for one by one (within
/// [`FIND_WITHIN`]); more, by reading their blocks once.
const ONE_BY_ONE: usize = 8;

/// The points drawn `drawn` as stored: the blocks they lie in read once,
/// each stored point drawn as the views draw it (mm from the model's
/// origin) matched to those selected; points not matched so (a few) are
/// looked for one by one within [`FIND_WITHIN`].
///
/// Cost: decoding the blocks (~10⁷ points a second) and a hash look-up
/// per stored point in them.
fn find_many(
    model: &Model,
    source: &ohl_pointcloud::CloudSource,
    cloud: &ohl_pointcloud::CloudInfo,
    drawn: &[LidarPoint],
) -> Result<Vec<EditedPoint>, String> {
    use std::collections::HashMap;
    let mut wanted: HashMap<LidarPoint, usize> = HashMap::with_capacity(drawn.len());
    let mut keys = BTreeSet::new();
    for p in drawn {
        *wanted.entry(*p).or_default() += 1;
        let at = crate::selection::lidar_xyz(model, &p.0);
        for dx in [-FIND_WITHIN, FIND_WITHIN] {
            for dy in [-FIND_WITHIN, FIND_WITHIN] {
                keys.insert(ohl_pointcloud::edit::block_of(
                    cloud,
                    [at[0] + dx, at[1] + dy, at[2]],
                    p.1,
                ));
            }
        }
    }
    let keys: Vec<_> = keys.into_iter().collect();
    let blocks = source
        .store()
        .block_points(&cloud.name, &keys)
        .map_err(|e| e.to_string())?;
    let origin = model.origin;
    let mut out = Vec::with_capacity(drawn.len());
    for points in &blocks {
        for (i, sp) in points.iter().enumerate() {
            let at = sp.position(cloud);
            let q = (
                [0, 1, 2].map(|k| ((at[k] - origin[k]) * 1000.0).round() as i32),
                sp.class,
            );
            if let Some(n) = wanted.get_mut(&q)
                && *n > 0
            {
                *n -= 1;
                out.push(EditedPoint {
                    stored: *sp,
                    index: i as u64,
                    xyz: at,
                    class: sp.class,
                });
            }
        }
    }
    let missing: Vec<(LidarPoint, usize)> = wanted.into_iter().filter(|w| w.1 > 0).collect();
    let n: usize = missing.iter().map(|m| m.1).sum();
    if n > 100 {
        return Err(format!(
            "{n} of the {} points selected are not stored as drawn (changed since?)",
            drawn.len()
        ));
    }
    for ((q, class), _) in missing {
        let at = crate::selection::lidar_xyz(model, &q);
        let (stored, index) = source
            .store()
            .find_point(&cloud.name, at, Some(class), FIND_WITHIN)
            .map_err(|e| e.to_string())?
            .filter(|f| !out.iter().any(|p| p.stored == f.0 && p.index == f.1))
            .ok_or_else(|| {
                format!(
                    "no stored point of class {class} at {:.3}, {:.3}, {:.3} (changed since?)",
                    at[0], at[1], at[2]
                )
            })?;
        out.push(EditedPoint {
            stored,
            index,
            xyz: stored.position(cloud),
            class: stored.class,
        });
    }
    Ok(out)
}

/// The project's cloud of `model`, editable or not: the cloud's file and
/// name, or why its points cannot be changed here.
fn cloud_of(model: &Model) -> Result<(std::path::PathBuf, Option<String>), String> {
    let spec = model
        .cloud
        .as_deref()
        .ok_or("no point cloud in the project (LiDAR files given at start are read as they are)")?;
    let (file, name) = ohl_lidar::cloud_spec(spec);
    if ohl_pointcloud::copc::is_copc(file) {
        return Err(
            "a COPC file is read as it is: import it into the project (File › Import › LiDAR \
             points) to change its points"
                .into(),
        );
    }
    Ok((file.to_path_buf(), name.map(str::to_owned)))
}

impl PointsForm {
    /// The window for the points drawn `drawn` (mm from the model's origin
    /// and class): each found as stored. Read-only when the model cannot
    /// be edited (a viewer) or the cloud is not the project's own.
    pub fn of(model: &Model, drawn: &[LidarPoint]) -> Result<Self, String> {
        let (file, name) = cloud_of(model)?;
        let source =
            ohl_pointcloud::CloudSource::open(&file, name.as_deref()).map_err(|e| e.to_string())?;
        let cloud = source.cloud().clone();
        let mut points = Vec::new();
        let one_by_one = if drawn.len() > ONE_BY_ONE {
            points = find_many(model, &source, &cloud, drawn)?;
            &[]
        } else {
            drawn
        };
        for (q, class) in one_by_one {
            let at = crate::selection::lidar_xyz(model, q);
            let (stored, index) = source
                .store()
                .find_point(&cloud.name, at, Some(*class), FIND_WITHIN)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| {
                    format!(
                        "no stored point of class {class} at {:.3}, {:.3}, {:.3} (changed since?)",
                        at[0], at[1], at[2]
                    )
                })?;
            points.push(EditedPoint {
                stored,
                index,
                xyz: stored.position(&cloud),
                class: stored.class,
            });
        }
        if points.is_empty() {
            return Err("no LiDAR point selected".into());
        }
        let ours = file == model.sources.file;
        let read_only = match model.can_edit() {
            Err(why) => Some(why),
            Ok(()) if !ours => Some(format!(
                "the point cloud of {} is not this project's own",
                file.display()
            )),
            Ok(()) => None,
        };
        Ok(Self {
            units: (cloud.chunk_size / cloud.scale).round() as i64,
            cloud: cloud.name,
            scale: cloud.scale,
            points,
            skip: BTreeSet::new(),
            read_only,
        })
    }

    /// The points per class of those selected (as stored).
    pub fn classes(&self) -> std::collections::BTreeMap<u8, usize> {
        let mut out = std::collections::BTreeMap::new();
        for p in &self.points {
            *out.entry(p.stored.class).or_default() += 1;
        }
        out
    }

    /// The (first) point as it is drawn once changed: mm from the model's
    /// origin, class.
    pub fn drawn_after(&self, model: &Model) -> Option<LidarPoint> {
        let p = self.points.first()?;
        let q = |v: f64| (v / self.scale).round() * self.scale;
        Some((
            [0, 1, 2].map(|k| ((q(p.xyz[k]) - model.origin[k]) * 1000.0).round() as i32),
            p.class,
        ))
    }

    /// Window title.
    pub fn title(&self) -> String {
        match self.points.len() {
            1 => "LiDAR point".into(),
            n => format!("LiDAR points ({n})"),
        }
    }

    /// The command changing the points edited (those unchanged left out).
    pub fn command(&self) -> Result<(String, Command), String> {
        if let Some(why) = &self.read_only {
            return Err(format!("read-only: {why}"));
        }
        let q = |v: f64| (v / self.scale).round() as i64;
        let one = self.points.len() == 1;
        // (block, index, change)
        let mut changes = Vec::new();
        for p in &self.points {
            if self.skip.contains(&p.stored.class) {
                continue;
            }
            if p.xyz.iter().any(|v| !v.is_finite()) {
                return Err("coordinates must be numbers".into());
            }
            // Several points: their class only.
            let to = StoredPoint {
                xyz: if one { p.xyz.map(q) } else { p.stored.xyz },
                class: p.class,
                ..p.stored
            };
            if to != p.stored {
                let s = &p.stored;
                let block = (
                    s.xyz[0].div_euclid(self.units.max(1)),
                    s.xyz[1].div_euclid(self.units.max(1)),
                    s.class,
                );
                changes.push((
                    block,
                    p.index,
                    PointChange {
                        from: CloudPoint::of(&p.stored, Some(p.index)),
                        to: CloudPoint::of(&to, None),
                    },
                ));
            }
        }
        if changes.is_empty() {
            return Err("nothing changed".into());
        }
        if changes.len() > ohl_session::points::MAX_POINTS {
            return Err(format!(
                "{} points: at most {} are changed by one command — select a smaller area",
                changes.len(),
                ohl_session::points::MAX_POINTS
            ));
        }
        // Block by block, from the highest index down: each point is then
        // found at its index as it is taken out (quick for many points).
        changes.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
        let changes: Vec<PointChange> = changes.into_iter().map(|c| c.2).collect();
        let what = match changes.len() {
            1 => format!("changed a LiDAR point of {}", self.cloud),
            n => format!("changed {n} LiDAR points of {}", self.cloud),
        };
        Ok((
            what,
            Command::ChangePoints {
                cloud: self.cloud.clone(),
                points: changes,
            },
        ))
    }
}

/// The class's number and name in the scheme.
fn class_text(model: &Model, code: u8) -> String {
    match model.scheme.class(code) {
        Some(d) => format!("{code} {}", d.name),
        None => format!("{code} (not in the scheme)"),
    }
}

/// The window's content.
pub fn points_ui(ui: &mut egui::Ui, f: &mut PointsForm, model: &Model) {
    let editable = f.read_only.is_none();
    if let Some(why) = &f.read_only {
        crate::menu::message(
            ui,
            egui::RichText::new(format!("Read-only: {why}")).small(),
            Some(ui.visuals().warn_fg_color),
        );
    }
    let mut classes: Vec<(u8, String)> = model
        .scheme
        .classes
        .iter()
        .map(|c| (c.code, format!("{} {}", c.code, c.name)))
        .collect();
    for p in &f.points {
        if !classes.iter().any(|c| c.0 == p.stored.class) {
            classes.push((p.stored.class, class_text(model, p.stored.class)));
        }
    }
    classes.sort_by_key(|c| c.0);
    let class_box = |ui: &mut egui::Ui, id: &str, class: &mut u8| {
        ui.add_enabled_ui(editable, |ui| {
            egui::ComboBox::from_id_salt(id)
                .selected_text(class_text(model, *class))
                .height(400.0)
                .show_ui(ui, |ui| {
                    for (code, text) in &classes {
                        ui.selectable_value(class, *code, text);
                    }
                });
        });
    };
    let step = f.scale.max(0.001);
    let decimals = (-step.log10()).ceil().max(0.0) as usize;
    if let [p] = f.points.as_mut_slice() {
        egui::Grid::new("lidar-point-form")
            .num_columns(2)
            .show(ui, |ui| {
                for (k, name) in ["easting", "northing", "height"].into_iter().enumerate() {
                    ui.label(name);
                    ui.add_enabled(
                        editable,
                        egui::DragValue::new(&mut p.xyz[k])
                            .speed(0.01)
                            .min_decimals(decimals)
                            .max_decimals(decimals)
                            .suffix(" m"),
                    );
                    ui.end_row();
                }
                ui.label("class");
                class_box(ui, "lidar-point-class", &mut p.class);
                ui.end_row();
                if let Some(def) = model.scheme.class(p.class)
                    && !def.roles.is_empty()
                {
                    ui.label("roles");
                    ui.label(def.roles.join(", "));
                    ui.end_row();
                }
                ui.label("as stored");
                ui.label(
                    egui::RichText::new(format!(
                        "intensity {}, return {} of {} (kept)",
                        p.stored.intensity,
                        p.stored.returns >> 4,
                        p.stored.returns & 0x0f
                    ))
                    .small(),
                );
                ui.end_row();
            });
    } else {
        // Several points: one class for all of them (coordinates are
        // changed one point at a time), those of the classes ticked.
        let counts = f.classes();
        let mut class = f
            .points
            .iter()
            .find(|p| !f.skip.contains(&p.stored.class))
            .map_or(f.points[0].class, |p| p.class);
        let before = class;
        ui.horizontal(|ui| {
            ui.label(format!("{} points selected — new class", f.points.len()));
            class_box(ui, "lidar-points-class", &mut class);
        });
        if class != before {
            for p in &mut f.points {
                p.class = class;
            }
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("change the points of class:");
            for (code, n) in &counts {
                let mut on = !f.skip.contains(code);
                if ui
                    .add_enabled(
                        editable,
                        egui::Checkbox::new(&mut on, format!("{} ({n})", class_text(model, *code))),
                    )
                    .changed()
                {
                    if on {
                        f.skip.remove(code);
                    } else {
                        f.skip.insert(*code);
                    }
                }
            }
        });
        let changing = f
            .points
            .iter()
            .filter(|p| !f.skip.contains(&p.stored.class) && p.class != p.stored.class)
            .count();
        ui.label(format!(
            "{changing} points change class (of the classes ticked, not of the new class already)"
        ));
        if changing > ohl_session::points::MAX_POINTS {
            crate::menu::message(
                ui,
                format!(
                    "at most {} points are changed by one command: untick classes or select a \
                     smaller area",
                    ohl_session::points::MAX_POINTS
                ),
                Some(ui.visuals().error_fg_color),
            );
        }
        let row = ui.text_style_height(&egui::TextStyle::Small) + 2.0;
        egui::ScrollArea::vertical().max_height(160.0).show_rows(
            ui,
            row,
            f.points.len(),
            |ui, rows| {
                for p in &f.points[rows] {
                    ui.label(
                        egui::RichText::new(format!(
                            "{:.3}, {:.3}, {:.3} — {}",
                            p.xyz[0],
                            p.xyz[1],
                            p.xyz[2],
                            class_text(model, p.stored.class)
                        ))
                        .small(),
                    );
                }
            },
        );
    }
    ui.label(
        egui::RichText::new(format!(
            "Point cloud {}, coordinates to {} m. Applied as one command (one undo step); \
             the clearances are checked again and the ground built from the cloud again; \
             results computed from the points (vegetation, as-built wires, clearance \
             windows) show they are out of date.",
            f.cloud, f.scale
        ))
        .small()
        .weak(),
    );
}

impl Model {
    /// Takes changed points (an edit, its undo, someone else's) into the
    /// points held in memory and the counts per class; the views read the
    /// cloud again next frame ([`frame`]).
    pub(crate) fn take_point_changes(&mut self, changed: &[(String, Vec<PointChange>)]) {
        let Ok((file, name)) = cloud_of(self) else {
            return;
        };
        let Ok(source) = ohl_pointcloud::CloudSource::open(&file, name.as_deref()) else {
            return;
        };
        let cloud = source.cloud().clone();
        let origin = self.origin;
        let mm = |p: &CloudPoint| -> LidarPoint {
            let at = p.stored().position(&cloud);
            (
                [0, 1, 2].map(|k| ((at[k] - origin[k]) * 1000.0).round() as i32),
                p.class,
            )
        };
        let mut done = self.points_changed.take().unwrap_or_default();
        let ground: BTreeSet<u8> = self.scheme.codes_with_role("ground").into_iter().collect();
        // The points in memory by what they are (many changes: found at
        // once, not looked for one by one).
        let many = changed.iter().map(|c| c.1.len()).sum::<usize>() > 16;
        let mut at: std::collections::HashMap<LidarPoint, Vec<usize>> = Default::default();
        if many {
            for (k, p) in self.points.iter().enumerate() {
                at.entry(*p).or_default().push(k);
            }
        }
        let mut counts: std::collections::BTreeMap<u8, u64> =
            self.class_counts.iter().copied().collect();
        for (c, changes) in changed {
            if *c != cloud.name {
                continue;
            }
            for ch in changes {
                self.revisions
                    .point_changed(c, &ch.from.stored(), &ch.to.stored(), |k| {
                        ground.contains(&k)
                    });
                done.classes.insert(ch.from.class);
                done.classes.insert(ch.to.class);
                let (from, to) = (mm(&ch.from), mm(&ch.to));
                done.moved.push((from, to));
                let k = if many {
                    at.get_mut(&from).and_then(Vec::pop)
                } else {
                    self.points.iter().position(|p| *p == from)
                };
                if let Some(k) = k {
                    self.points[k] = to;
                    if many {
                        at.entry(to).or_default().push(k);
                    }
                }
                if ch.from.class != ch.to.class {
                    if let Some(n) = counts.get_mut(&ch.from.class) {
                        *n = n.saturating_sub(1);
                    }
                    *counts.entry(ch.to.class).or_default() += 1;
                }
            }
        }
        self.class_counts = counts.into_iter().filter(|n| n.1 > 0).collect();
        if !done.classes.is_empty() {
            self.points_changed = Some(done);
        }
    }
}

/// The ground from the ground classes of the cloud `spec` within
/// `corridor` m of the spans of `line`, as the project builds it when it
/// opens (the lowest point per cell, [`crate::surfaces::LidarBuild`]).
///
/// Cost: reading the ground classes in the corridor (~1 s per 10⁷
/// points). Exit clause: 10 minutes.
pub fn lidar_ground(
    spec: &str,
    classes: &[u8],
    line: &ohl_line::Line,
    corridor: f64,
) -> Result<Option<ohl_terrain::Terrain>, String> {
    let source = ohl_pointcloud::open_source(spec)?;
    let corridor = ohl_lidar::Corridor::along_spans(line, corridor);
    let build = crate::surfaces::LidarBuild::default();
    let mut ground = ohl_lidar::LowestPerCell::new(Some(build.cell));
    let budget = ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(600));
    source.visit(
        Some(&corridor),
        Some(classes),
        &budget,
        &mut |x, y, z, _| ground.add(x, y, z),
    )?;
    if ground.points().len() < 3 {
        return Ok(None);
    }
    ground
        .terrain(Some(build.max_edge))
        .map(Some)
        .map_err(|e| e.to_string())
}

/// The LiDAR points a box or lasso selected in a view (`picked`: at most
/// one more than a command takes, `total` in it, found in `took`): the
/// Edit window of LiDAR points on them, or why not (none drawn there,
/// too many).
pub fn area_picked(
    r: &mut crate::app::Ready,
    (picked, total, took): (Vec<LidarPoint>, usize, std::time::Duration),
) {
    use ohl_session::points::MAX_POINTS;
    if total == 0 {
        r.status = Some((
            "no LiDAR point drawn in the box (only points drawn can be selected: of a class \
             shown with a colour, within the share and distance drawn)"
                .into(),
            true,
        ));
        return;
    }
    if total > MAX_POINTS {
        r.status = Some((
            format!(
                "{total} LiDAR points in the box: at most {MAX_POINTS} are changed at once — \
                 select a smaller area or hide classes"
            ),
            true,
        ));
        return;
    }
    let started = std::time::Instant::now();
    match PointsForm::of(&r.model, &picked) {
        Ok(f) => {
            r.status = Some((
                format!(
                    "{total} LiDAR points selected in {took:.1?}, found as stored in {:.1?}",
                    started.elapsed()
                ),
                false,
            ));
            log::info!(
                "{total} LiDAR points selected in {took:.1?}, found as stored in {:.1?}",
                started.elapsed()
            );
            r.editor = Some(crate::editors::Form::Points(f));
        }
        Err(e) => r.status = Some((e, true)),
    }
}

/// Every frame: after points changed, the 3D view and the profile read
/// them again, the clearances are checked again (when checked after every
/// edit), and the ground built from the cloud is built again in the
/// background when ground points changed; its answer taken.
pub fn frame(ctx: &egui::Context, r: &mut crate::app::Ready) {
    use crate::scene::Layer;
    // A box or lasso drawn in a view: its points in the Edit window.
    for picked in [r.view3d.area_picked.take(), r.profile.area_picked.take()]
        .into_iter()
        .flatten()
    {
        area_picked(r, picked);
    }
    if let Some(changed) = r.model.points_changed.take() {
        // A point selected follows.
        for (from, to) in &changed.moved {
            if r.selection == Selection::LidarPoint(*from) {
                r.selection = Selection::LidarPoint(*to);
            }
        }
        let classes = changed.classes;
        r.view3d.recolor_points();
        r.profile.invalidate();
        r.clearances.stale = true;
        let ground: BTreeSet<u8> = r
            .model
            .scheme
            .codes_with_role("ground")
            .into_iter()
            .collect();
        if r.model.ground_from_lidar
            && r.model.ground_source.is_empty()
            && classes.iter().any(|c| ground.contains(c))
        {
            r.lidar_ground_again = true;
        }
    }
    // The ground built again (once the build under way, if any, is done:
    // it read the points before this change).
    if r.lidar_ground_again
        && !r.lidar_ground.running()
        && let Some(spec) = r.model.cloud.clone()
    {
        r.lidar_ground_again = false;
        let ground: Vec<u8> = r.model.scheme.codes_with_role("ground");
        let line = r.model.line.clone();
        let corridor = r.model.sources.corridor;
        // The ground it gives: that of the ground points now.
        r.lidar_ground_rev = r.model.revisions.ground_points;
        r.lidar_ground
            .start(ctx, move || lidar_ground(&spec, &ground, &line, corridor));
    }
    let running = r.lidar_ground.running();
    r.lidar_ground.poll();
    if running && !r.lidar_ground.running() {
        match r.lidar_ground.result.take() {
            Some(Ok(Some(g))) => {
                r.model.ground = Some(std::sync::Arc::new(g));
                r.model.revisions.ground = r.lidar_ground_rev;
                r.view3d.dirty.insert(Layer::Ground);
                r.profile.invalidate();
                r.clearances.stale = true;
                r.status = Some((
                    "the ground was built again from the changed LiDAR points".into(),
                    false,
                ));
            }
            Some(Ok(None)) => {}
            Some(Err(e)) => r.status = Some((format!("ground from LiDAR not built: {e}"), true)),
            None => {}
        }
    }
}
