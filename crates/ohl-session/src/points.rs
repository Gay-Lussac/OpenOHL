// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Points of the project's point clouds changed by a command
//! ([`crate::Command::ChangePoints`]): their coordinates and class, in the
//! cloud's blocks ([`ohl_pointcloud::edit`]), in the same transaction as
//! the log entry. Undo applies the inverse (the points back, at their
//! places in their blocks: every stored byte as before).
//!
//! The point clouds belong to the project file, not to a project line:
//! a change is logged (and undone) on the line it was made on, and every
//! line sees it. Revisions of the line do not hold the points.

use ohl_core::grid::{Bounds, Grid};
use ohl_pointcloud::edit::{PointChange as StoreChange, StoredPoint};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::spatial::Entry;
use crate::{SessionError, refused};

/// A point of a cloud exactly as stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloudPoint {
    /// Coordinates: integers of the cloud's scale (metres / scale, e.g.
    /// millimetres).
    pub xyz: [i64; 3],
    /// Class.
    pub class: u8,
    /// Intensity.
    #[serde(default)]
    pub intensity: u16,
    /// Return number (high 4 bits) and number of returns (low 4 bits).
    #[serde(default)]
    pub returns: u8,
    /// Its index in its block (chunk and class), when known: where it is
    /// (`from`) or goes (`to`). Written by the session; an undo puts the
    /// point back there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u64>,
}

impl CloudPoint {
    /// The point as the store names it.
    pub fn stored(&self) -> StoredPoint {
        StoredPoint {
            xyz: self.xyz,
            class: self.class,
            intensity: self.intensity,
            returns: self.returns,
        }
    }

    /// A stored point, at `index` of its block.
    pub fn of(p: &StoredPoint, index: Option<u64>) -> Self {
        Self {
            xyz: p.xyz,
            class: p.class,
            intensity: p.intensity,
            returns: p.returns,
            index,
        }
    }
}

/// One point changed: `from` (as stored now) becomes `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointChange {
    /// The point as it is.
    pub from: CloudPoint,
    /// The point as it will be.
    pub to: CloudPoint,
}

impl PointChange {
    fn store(&self) -> StoreChange {
        StoreChange {
            from: self.from.stored(),
            from_index: self.from.index,
            to: self.to.stored(),
            to_index: self.to.index,
        }
    }

    fn of(c: &StoreChange) -> Self {
        Self {
            from: CloudPoint::of(&c.from, c.from_index),
            to: CloudPoint::of(&c.to, c.to_index),
        }
    }
}

/// Checks a change command before it is applied: a cloud named, points
/// given, each changed (refused: nothing to do).
pub(crate) fn check(cloud: &str, points: &[PointChange]) -> Result<(), SessionError> {
    if cloud.is_empty() {
        return Err(refused("change points: no point cloud named"));
    }
    if points.is_empty() {
        return Err(refused("change points: no point given"));
    }
    if points.iter().any(|c| c.from.stored() == c.to.stored()) {
        return Err(refused("change points: a point given unchanged"));
    }
    Ok(())
}

/// Applies the changes in `conn` (the session's transaction): returns
/// them as applied (with the points' places in their blocks) and their
/// inverse.
pub(crate) fn apply(
    conn: &Connection,
    cloud: &str,
    points: &[PointChange],
) -> Result<(Vec<PointChange>, Vec<PointChange>), SessionError> {
    let store = |e: ohl_pointcloud::StoreError| refused(e.0);
    let info = ohl_pointcloud::edit::cloud_named(conn, cloud).map_err(store)?;
    let changes: Vec<StoreChange> = points.iter().map(PointChange::store).collect();
    let applied = ohl_pointcloud::edit::change_points(conn, &info, &changes).map_err(store)?;
    let inverse = applied
        .iter()
        .rev()
        .map(|c| PointChange::of(&c.inverse()))
        .collect();
    Ok((applied.iter().map(PointChange::of).collect(), inverse))
}

/// Whether the points an entry left (each change's `to`) are still in the
/// cloud: else the entry cannot be reversed (someone changed them since).
pub(crate) fn still_there(
    conn: &Connection,
    cloud: &str,
    points: &[PointChange],
) -> Result<bool, SessionError> {
    let store = |e: ohl_pointcloud::StoreError| SessionError::Store(e.0);
    let Ok(info) = ohl_pointcloud::edit::cloud_named(conn, cloud) else {
        return Ok(false);
    };
    let to: Vec<StoredPoint> = points.iter().map(|c| c.to.stored()).collect();
    ohl_pointcloud::edit::points_present(conn, &info, &to).map_err(store)
}

/// The plan area of the points changed (where they were and where they
/// go), for the edit rights limited to an area: `None` when the cloud is
/// missing.
pub(crate) fn entry(
    conn: &Connection,
    grid: &Grid,
    cloud: &str,
    points: &[PointChange],
) -> Option<Entry> {
    let info = ohl_pointcloud::edit::cloud_named(conn, cloud).ok()?;
    let at: Vec<[f64; 3]> = points
        .iter()
        .flat_map(|c| [c.from.stored(), c.to.stored()])
        .map(|p| p.position(&info))
        .collect();
    let bounds = Bounds::of_points(&at)?;
    Some(Entry {
        tile: grid.home(&bounds),
        bounds,
    })
}
