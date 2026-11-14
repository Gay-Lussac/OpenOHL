// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The LiDAR points read again after an import (owner, 2026-11-14: the
//! whole project was opened again after every import): the point cloud
//! read on its own thread as the project's opening reads it
//! ([`crate::model::read_cloud`]), then put in the model in one frame —
//! the points, their classes, the class scheme — and the views told as
//! after an edit of points ([`crate::lidar_edit::frame`]: the points drawn
//! again, the profile, the cuts, the clearances out of date, the ground
//! built again from the ground classes where it comes from them). The
//! weather cases solved, the windows, the camera and the selection stay.

use std::collections::BTreeSet;

use eframe::egui;

use crate::app::Ready;
use crate::model::CloudRead;
use crate::scene::Layer;

/// The point cloud and what was said reading it.
pub type Read = (CloudRead, Vec<String>);

/// Starts reading the project's point cloud again (an import done).
pub fn start(ctx: &egui::Context, r: &mut Ready) {
    let sources = r.model.sources.clone();
    let line = r.model.line.clone();
    let origin = r.model.origin;
    r.status = Some(("reading the imported points…".into(), false));
    r.cloud_reload.start(ctx, move || {
        let mut log = Vec::new();
        // The ground: built again by the LiDAR ground's own run where it
        // comes from the points (lidar_edit::frame), not here.
        let read = crate::model::read_cloud(&sources, &line, origin, false, &mut |m| {
            log::info!("{m}");
            log.push(m);
        })?;
        Ok((read, log))
    });
}

/// Takes the points read, once they are.
pub fn frame(r: &mut Ready) {
    let running = r.cloud_reload.running();
    r.cloud_reload.poll();
    if !running || r.cloud_reload.running() {
        return;
    }
    match r.cloud_reload.result.take() {
        Some(Ok((read, log))) => apply(r, read, log),
        Some(Err(e)) => {
            r.status = Some((format!("the imported points were not read: {e}"), true));
        }
        None => {}
    }
}

/// Puts `read` in the model and tells the views.
pub fn apply(r: &mut Ready, read: CloudRead, log: Vec<String>) {
    use std::hash::{Hash, Hasher};
    let m = &mut r.model;
    // Every class there was or is: drawn again.
    let classes: BTreeSet<u8> = m
        .class_counts
        .iter()
        .chain(&read.class_counts)
        .map(|(c, _)| *c)
        .collect();
    let n = read.points.len();
    m.cloud = read.cloud;
    m.points = read.points;
    m.class_counts = read.class_counts;
    m.scheme = read.scheme;
    m.log.extend(log);
    // The points are others: what was computed from them is out of date
    // (a new revision, never the one before).
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (&m.cloud, &m.class_counts, n).hash(&mut h);
    let mark = h.finish() | 1;
    m.revisions.points = m.revisions.points.wrapping_add(mark);
    m.revisions.ground_points = m.revisions.ground_points.wrapping_add(mark);
    m.points_changed = Some(crate::lidar_edit::PointsChanged {
        classes,
        moved: Vec::new(),
    });
    r.visible.insert(Layer::Points);
    r.status = Some((
        format!(
            "{} points shown of the point cloud, read again after the import",
            n
        ),
        false,
    ));
}
