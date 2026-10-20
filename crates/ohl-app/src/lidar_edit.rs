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
    /// The points.
    pub points: Vec<EditedPoint>,
    /// Why it cannot be changed (a viewer, a cloud outside the project):
    /// shown read-only.
    pub read_only: Option<String>,
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
        for (q, class) in drawn {
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
            cloud: cloud.name,
            scale: cloud.scale,
            points,
            read_only,
        })
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
        let mut changes = Vec::new();
        for p in &self.points {
            if p.xyz.iter().any(|v| !v.is_finite()) {
                return Err("coordinates must be numbers".into());
            }
            let to = StoredPoint {
                xyz: p.xyz.map(q),
                class: p.class,
                ..p.stored
            };
            if to != p.stored {
                changes.push(PointChange {
                    from: CloudPoint::of(&p.stored, Some(p.index)),
                    to: CloudPoint::of(&to, None),
                });
            }
        }
        if changes.is_empty() {
            return Err("nothing changed".into());
        }
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
        // Several points: their class.
        let mut class = f.points[0].class;
        let before = class;
        ui.horizontal(|ui| {
            ui.label(format!("{} points: class", f.points.len()));
            class_box(ui, "lidar-points-class", &mut class);
        });
        if class != before {
            for p in &mut f.points {
                p.class = class;
            }
        }
        egui::ScrollArea::vertical()
            .max_height(160.0)
            .show(ui, |ui| {
                for p in &f.points {
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
            });
    }
    ui.label(
        egui::RichText::new(format!(
            "Point cloud {}, coordinates to {} m. Applied as one command (one undo step); \
             the clearances, the ground built from the cloud, vegetation and as-built \
             wires use the new values.",
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
        for (c, changes) in changed {
            if *c != cloud.name {
                continue;
            }
            for ch in changes {
                done.classes.insert(ch.from.class);
                done.classes.insert(ch.to.class);
                let (from, to) = (mm(&ch.from), mm(&ch.to));
                done.moved.push((from, to));
                if let Some(p) = self.points.iter_mut().find(|p| **p == from) {
                    *p = to;
                }
                if ch.from.class != ch.to.class {
                    let counts = &mut self.class_counts;
                    if let Some(n) = counts.iter_mut().find(|n| n.0 == ch.from.class) {
                        n.1 = n.1.saturating_sub(1);
                    }
                    match counts.iter_mut().find(|n| n.0 == ch.to.class) {
                        Some(n) => n.1 += 1,
                        None => {
                            counts.push((ch.to.class, 1));
                            counts.sort_by_key(|n| n.0);
                        }
                    }
                    counts.retain(|n| n.1 > 0);
                }
            }
        }
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

/// Every frame: after points changed, the 3D view and the profile read
/// them again, the clearances are checked again (when checked after every
/// edit), and the ground built from the cloud is built again in the
/// background when ground points changed; its answer taken.
pub fn frame(ctx: &egui::Context, r: &mut crate::app::Ready) {
    use crate::scene::Layer;
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
            && let Some(spec) = r.model.cloud.clone()
        {
            let line = r.model.line.clone();
            let corridor = r.model.sources.corridor;
            let ground: Vec<u8> = ground.into_iter().collect();
            r.lidar_ground
                .start(ctx, move || lidar_ground(&spec, &ground, &line, corridor));
        }
    }
    let running = r.lidar_ground.running();
    r.lidar_ground.poll();
    if running && !r.lidar_ground.running() {
        match r.lidar_ground.result.take() {
            Some(Ok(Some(g))) => {
                r.model.ground = Some(std::sync::Arc::new(g));
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
