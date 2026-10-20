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

/// The most points one command changes (a selection of more is changed
/// by several commands): 10⁶ points apply in a few seconds and keep the
/// log entry to some 10 MB ([`packed`]).
pub const MAX_POINTS: usize = 1_000_000;

/// Checks a change command before it is applied: a cloud named, points
/// given (at most [`MAX_POINTS`]), each changed (refused: nothing to do).
pub(crate) fn check(cloud: &str, points: &[PointChange]) -> Result<(), SessionError> {
    if cloud.is_empty() {
        return Err(refused("change points: no point cloud named"));
    }
    if points.is_empty() {
        return Err(refused("change points: no point given"));
    }
    if points.len() > MAX_POINTS {
        return Err(refused(format!(
            "change points: {} points given, at most {MAX_POINTS} in one command",
            points.len()
        )));
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

impl crate::Session {
    /// The state of the project's point clouds, for a saved result to
    /// tell whether they changed since: the last log entry that changed
    /// points (any line: the clouds are the project's) and each cloud's
    /// name, point count and creation time (an import makes another).
    /// An undo is a change here too (**conservative**: the points may be
    /// as before).
    pub fn points_state(&self) -> String {
        let seq: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) FROM oplog WHERE objects LIKE '%\"point_cloud\"%'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let clouds: String = self
            .conn
            .query_row(
                "SELECT COALESCE(group_concat(name || ':' || points || ':' || created, ';'), '')
                 FROM (SELECT name, points, created FROM pointcloud ORDER BY name)",
                [],
                |r| r.get(0),
            )
            .unwrap_or_default();
        format!("{seq}|{clouds}")
    }
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

/// The points of a [`crate::Command::ChangePoints`] as written: below
/// [`PACK_FROM`](packed::PACK_FROM) points a list of `{from, to}` objects;
/// from there on one string `"zstd+base64:…"` — the changes column by
/// column (each coordinate as the difference to the one before, zigzag
/// varints; class, intensity, returns; index + 1, 0 without one),
/// zstd-compressed and base64-encoded: some 10 bytes a point instead of
/// some 200. Both are read.
pub mod packed {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::{CloudPoint, PointChange};

    /// From this many points on, the list is written packed.
    pub const PACK_FROM: usize = 100;

    const PREFIX: &str = "zstd+base64:";
    const VERSION: u8 = 1;

    fn varint(out: &mut Vec<u8>, mut v: u64) {
        while v >= 0x80 {
            out.push((v as u8) | 0x80);
            v >>= 7;
        }
        out.push(v as u8);
    }

    fn zigzag(v: i64) -> u64 {
        ((v << 1) ^ (v >> 63)) as u64
    }

    fn unzigzag(v: u64) -> i64 {
        ((v >> 1) as i64) ^ -((v & 1) as i64)
    }

    struct Reader<'a>(&'a [u8]);

    impl Reader<'_> {
        fn byte(&mut self) -> Result<u8, String> {
            let (&b, rest) = self.0.split_first().ok_or("packed points: cut short")?;
            self.0 = rest;
            Ok(b)
        }

        fn varint(&mut self) -> Result<u64, String> {
            let mut v = 0u64;
            for shift in (0..64).step_by(7) {
                let b = self.byte()?;
                v |= u64::from(b & 0x7f) << shift;
                if b & 0x80 == 0 {
                    return Ok(v);
                }
            }
            Err("packed points: a number too long".into())
        }
    }

    /// The changes as bytes (before compression).
    fn encode(points: &[PointChange]) -> Vec<u8> {
        let mut out = vec![VERSION];
        varint(&mut out, points.len() as u64);
        let sides: [fn(&PointChange) -> CloudPoint; 2] = [|c| c.from, |c| c.to];
        for side in sides {
            for k in 0..3 {
                let mut last = 0i64;
                for c in points {
                    let v = side(c).xyz[k];
                    varint(&mut out, zigzag(v.wrapping_sub(last)));
                    last = v;
                }
            }
            out.extend(points.iter().map(|c| side(c).class));
            for c in points {
                varint(&mut out, u64::from(side(c).intensity));
            }
            out.extend(points.iter().map(|c| side(c).returns));
            for c in points {
                varint(&mut out, side(c).index.map_or(0, |i| i.saturating_add(1)));
            }
        }
        out
    }

    fn decode(bytes: &[u8]) -> Result<Vec<PointChange>, String> {
        let mut r = Reader(bytes);
        if r.byte()? != VERSION {
            return Err("packed points: unknown version".into());
        }
        let n = usize::try_from(r.varint()?).map_err(|e| e.to_string())?;
        // Each point takes at least 9 bytes.
        if n > bytes.len() {
            return Err(format!(
                "packed points: {n} points in {} bytes",
                bytes.len()
            ));
        }
        let mut sides = [vec![], vec![]];
        for side in &mut sides {
            let mut pts = vec![
                CloudPoint {
                    xyz: [0; 3],
                    class: 0,
                    intensity: 0,
                    returns: 0,
                    index: None,
                };
                n
            ];
            for k in 0..3 {
                let mut last = 0i64;
                for p in &mut pts {
                    last = last.wrapping_add(unzigzag(r.varint()?));
                    p.xyz[k] = last;
                }
            }
            for p in &mut pts {
                p.class = r.byte()?;
            }
            for p in &mut pts {
                p.intensity = u16::try_from(r.varint()?).map_err(|e| e.to_string())?;
            }
            for p in &mut pts {
                p.returns = r.byte()?;
            }
            for p in &mut pts {
                p.index = r.varint()?.checked_sub(1);
            }
            *side = pts;
        }
        if !r.0.is_empty() {
            return Err("packed points: bytes left over".into());
        }
        let [from, to] = sides;
        Ok(from
            .into_iter()
            .zip(to)
            .map(|(from, to)| PointChange { from, to })
            .collect())
    }

    /// Writes `points` (packed from [`PACK_FROM`] points on).
    pub fn serialize<S: Serializer>(points: &[PointChange], s: S) -> Result<S::Ok, S::Error> {
        if points.len() < PACK_FROM {
            return points.serialize(s);
        }
        let bytes = zstd::encode_all(encode(points).as_slice(), 3)
            .map_err(|e| serde::ser::Error::custom(e.to_string()))?;
        let text = base64::engine::general_purpose::STANDARD.encode(bytes);
        s.serialize_str(&format!("{PREFIX}{text}"))
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Written {
        List(Vec<PointChange>),
        Packed(String),
    }

    /// Reads a list of points or a packed string.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<PointChange>, D::Error> {
        let bad = |e: String| serde::de::Error::custom(e);
        match Written::deserialize(d)? {
            Written::List(v) => Ok(v),
            Written::Packed(text) => {
                let body = text
                    .strip_prefix(PREFIX)
                    .ok_or_else(|| bad(format!("points: a list or \"{PREFIX}…\"")))?;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(body)
                    .map_err(|e| bad(e.to_string()))?;
                // Unpacked, at most some 60 bytes a point of a command.
                let limit = (super::MAX_POINTS * 64) as u64;
                let mut raw = Vec::new();
                let decoder =
                    zstd::Decoder::new(bytes.as_slice()).map_err(|e| bad(e.to_string()))?;
                std::io::Read::read_to_end(&mut std::io::Read::take(decoder, limit), &mut raw)
                    .map_err(|e| bad(e.to_string()))?;
                decode(&raw).map_err(bad)
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn packed_points_read_back_exactly() {
            let p = |i: i64, class: u8, index: Option<u64>| CloudPoint {
                xyz: [
                    2_600_000_000 + 37 * i,
                    1_200_000_000 - 11 * i,
                    400_000 + (i % 9) - 4,
                ],
                class,
                intensity: (i * 13 % 65_000) as u16,
                returns: (i % 3) as u8 * 16 + 2,
                index,
            };
            let points: Vec<PointChange> = (0..5000)
                .map(|i| PointChange {
                    from: p(i, 2, (i % 4 != 0).then_some((5000 - i) as u64)),
                    to: p(i, if i % 2 == 0 { 5 } else { 255 }, None),
                })
                .collect();
            let cmd = crate::Command::ChangePoints {
                cloud: "c".into(),
                points: points.clone(),
            };
            let json = serde_json::to_string(&cmd).unwrap();
            assert!(json.contains(PREFIX), "packed");
            assert!(json.len() < 20 * points.len(), "{} bytes", json.len());
            let back: crate::Command = serde_json::from_str(&json).unwrap();
            assert_eq!(back, cmd);
            // Few points: a list, read back too.
            let few = crate::Command::ChangePoints {
                cloud: "c".into(),
                points: points[..3].to_vec(),
            };
            let json = serde_json::to_string(&few).unwrap();
            assert!(!json.contains(PREFIX));
            assert_eq!(serde_json::from_str::<crate::Command>(&json).unwrap(), few);
            // Damaged: an error, no panic.
            for bad in ["\"zstd+base64:AAAA\"", "\"zstd+base64:!!\"", "\"points\""] {
                let text = format!("{{\"op\":\"change_points\",\"cloud\":\"c\",\"points\":{bad}}}");
                assert!(
                    serde_json::from_str::<crate::Command>(&text).is_err(),
                    "{bad}"
                );
            }
        }
    }
}
