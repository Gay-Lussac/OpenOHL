// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Changing single points of a stored cloud: their coordinates and their
//! class (the session's `change_points` command, undoable).
//!
//! A point is named exactly as stored ([`StoredPoint`]): its integer
//! coordinates (of the cloud's scale), class, intensity and returns —
//! where it lies gives its block (chunk and class). A change takes the
//! point out of its block and puts the new one into the block of its new
//! chunk and class (made when missing; a block left empty is removed).
//! The other points of a block keep their order, so the levels of detail
//! built again from it ([`crate::lod_level`] by index) and the
//! compressed data are exactly what they were once a change is undone.
//! After the blocks, the cloud's count and bounds are computed again from
//! its blocks as an import does; points per class are counted from the
//! blocks ([`crate::Store::class_counts`]).
//!
//! The changes run in the caller's transaction (the session writes them
//! with its log entry).

use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, params};

use crate::codec::{self, Columns};
use crate::{CloudInfo, LOD_LEVELS, StoreError, err, lod_level};

/// A point exactly as stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StoredPoint {
    /// Coordinates, integers of the cloud's scale (metres / scale).
    pub xyz: [i64; 3],
    /// Class.
    pub class: u8,
    /// Intensity.
    pub intensity: u16,
    /// Return number (high 4 bits) and number of returns (low 4 bits).
    pub returns: u8,
}

impl StoredPoint {
    /// Its project coordinates, m.
    pub fn position(&self, cloud: &CloudInfo) -> [f64; 3] {
        self.xyz.map(|v| v as f64 * cloud.scale)
    }

    /// Integer coordinates of a position, m (rounded to the scale).
    pub fn quantize(p: [f64; 3], cloud: &CloudInfo) -> [i64; 3] {
        p.map(|v| (v / cloud.scale).round() as i64)
    }

    /// Its block: chunk x, chunk y, class.
    fn block(&self, units: i64) -> (i64, i64, u8) {
        (
            self.xyz[0].div_euclid(units),
            self.xyz[1].div_euclid(units),
            self.class,
        )
    }
}

/// One point changed: `from` becomes `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointChange {
    /// The point as it is.
    pub from: StoredPoint,
    /// Its index in its block, when known (else the first point equal to
    /// it is taken).
    pub from_index: Option<u64>,
    /// The point as it will be.
    pub to: StoredPoint,
    /// Where it goes in its block, when given and in the block's order
    /// (else after the points of the same or a lower (x, y)).
    pub to_index: Option<u64>,
}

impl PointChange {
    /// The change back.
    pub fn inverse(&self) -> Self {
        Self {
            from: self.to,
            from_index: self.to_index,
            to: self.from,
            to_index: self.from_index,
        }
    }
}

/// A block decoded: absolute integer columns.
struct Block {
    id: Option<i64>,
    x: Vec<i64>,
    y: Vec<i64>,
    z: Vec<i64>,
    intensity: Vec<u16>,
    returns: Vec<u8>,
    /// Its intensities and returns were not stored (kept so while they
    /// stay 0).
    no_intensity: bool,
    no_returns: bool,
    /// It has levels of detail stored.
    has_lod: bool,
}

impl Block {
    fn empty() -> Self {
        Self {
            id: None,
            x: vec![],
            y: vec![],
            z: vec![],
            intensity: vec![],
            returns: vec![],
            no_intensity: false,
            no_returns: false,
            has_lod: false,
        }
    }

    fn len(&self) -> usize {
        self.x.len()
    }

    fn at(&self, i: usize, class: u8) -> StoredPoint {
        StoredPoint {
            xyz: [self.x[i], self.y[i], self.z[i]],
            class,
            intensity: self.intensity[i],
            returns: self.returns[i],
        }
    }

    fn remove(&mut self, i: usize) {
        self.x.remove(i);
        self.y.remove(i);
        self.z.remove(i);
        self.intensity.remove(i);
        self.returns.remove(i);
    }

    fn insert(&mut self, i: usize, p: &StoredPoint) {
        self.x.insert(i, p.xyz[0]);
        self.y.insert(i, p.xyz[1]);
        self.z.insert(i, p.xyz[2]);
        self.intensity.insert(i, p.intensity);
        self.returns.insert(i, p.returns);
    }

    /// The index of `p` (at `hint` when it is there).
    fn find(&self, p: &StoredPoint, hint: Option<u64>) -> Option<usize> {
        let same = |i: usize| self.at(i, p.class) == *p;
        if let Some(h) = hint.and_then(|h| usize::try_from(h).ok())
            && h < self.len()
            && same(h)
        {
            return Some(h);
        }
        (0..self.len()).find(|&i| same(i))
    }

    /// Where `p` goes: at `hint` when that keeps the (x, y) order, else
    /// after the points of the same or a lower (x, y).
    fn place(&self, p: &StoredPoint, hint: Option<u64>) -> usize {
        let key = |i: usize| (self.x[i], self.y[i]);
        let k = (p.xyz[0], p.xyz[1]);
        if let Some(h) = hint.and_then(|h| usize::try_from(h).ok())
            && h <= self.len()
            && (h == 0 || key(h - 1) <= k)
            && (h == self.len() || k <= key(h))
        {
            return h;
        }
        let (mut lo, mut hi) = (0, self.len());
        while lo < hi {
            let mid = (lo + hi) / 2;
            if key(mid) <= k {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }
}

/// Reads block (cx, cy, class) of `cloud` (an empty one when missing).
fn read_block(
    conn: &Connection,
    cloud: &CloudInfo,
    units: i64,
    (cx, cy, class): (i64, i64, u8),
) -> Result<Block, StoreError> {
    type Row = (i64, i64, i64, Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>);
    let row: Option<Row> = conn
        .query_row(
            "SELECT id, count, z0, xyz, intensity, returns FROM pointcloud_block
             WHERE cloud = ?1 AND cx = ?2 AND cy = ?3 AND class = ?4",
            params![cloud.id, cx, cy, class],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .optional()?;
    let Some((id, count, z0, xyz, intensity, returns)) = row else {
        return Ok(Block::empty());
    };
    let n = codec::block_count(count).map_err(err)?;
    let (x, y, z) = Columns::decode_xyz(&xyz, n).map_err(err)?;
    let has_lod = conn
        .prepare("SELECT 1 FROM pointcloud_lod WHERE block = ?1")?
        .exists(params![id])?;
    Ok(Block {
        id: Some(id),
        x: x.iter().map(|&v| cx * units + i64::from(v)).collect(),
        y: y.iter().map(|&v| cy * units + i64::from(v)).collect(),
        z: z.iter().map(|&v| z0 + i64::from(v)).collect(),
        intensity: match &intensity {
            Some(b) => Columns::decode_intensity(b, n).map_err(err)?,
            None => vec![0; n],
        },
        returns: match &returns {
            Some(b) => Columns::decode_returns(b, n).map_err(err)?,
            None => vec![0; n],
        },
        no_intensity: intensity.is_none(),
        no_returns: returns.is_none(),
        has_lod,
    })
}

/// Writes a changed block back: removed when empty; its levels of detail
/// built again when it had them (a new block: when the cloud has them).
fn write_block(
    conn: &Connection,
    cloud: &CloudInfo,
    units: i64,
    (cx, cy, class): (i64, i64, u8),
    b: &Block,
    cloud_has_lod: bool,
) -> Result<(), StoreError> {
    if b.len() == 0 {
        if let Some(id) = b.id {
            conn.execute("DELETE FROM pointcloud_lod WHERE block = ?1", params![id])?;
            conn.execute("DELETE FROM pointcloud_block WHERE id = ?1", params![id])?;
        }
        return Ok(());
    }
    let z0 = b.z.iter().copied().min().unwrap_or(0);
    let zmax = b.z.iter().copied().max().unwrap_or(0);
    if zmax - z0 > i64::from(u32::MAX) {
        return Err(err("height range of a block exceeds the integer range"));
    }
    if b.len() > codec::MAX_BLOCK_POINTS {
        return Err(err("too many points of one class in one chunk"));
    }
    let columns = Columns {
        x: b.x.iter().map(|&v| (v - cx * units) as u32).collect(),
        y: b.y.iter().map(|&v| (v - cy * units) as u32).collect(),
        z: b.z.iter().map(|&v| (v - z0) as u32).collect(),
        intensity: b.intensity.clone(),
        returns: b.returns.clone(),
    };
    let (xyz, intensity, returns) = columns.encode_in_order().map_err(err)?;
    let intensity = (!(b.no_intensity && b.intensity.iter().all(|&v| v == 0))).then_some(intensity);
    let returns = (!(b.no_returns && b.returns.iter().all(|&v| v == 0))).then_some(returns);
    let count = b.len() as i64;
    let id = match b.id {
        Some(id) => {
            conn.execute(
                "UPDATE pointcloud_block SET count = ?2, z0 = ?3, xyz = ?4, intensity = ?5,
                 returns = ?6 WHERE id = ?1",
                params![id, count, z0, xyz, intensity, returns],
            )?;
            id
        }
        None => {
            conn.execute(
                "INSERT INTO pointcloud_block (cloud, cx, cy, class, count, z0, xyz, intensity, returns)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![cloud.id, cx, cy, class, count, z0, xyz, intensity, returns],
            )?;
            conn.last_insert_rowid()
        }
    };
    if b.has_lod || (b.id.is_none() && cloud_has_lod) {
        // As [`crate::Store::build_lod`] builds them.
        let top = z0 + i64::from(columns.z.iter().copied().max().unwrap_or(0));
        let marks: Vec<u8> = (0..b.len()).map(lod_level).collect();
        for k in 1..=LOD_LEVELS {
            let keep = |v: &[u32]| {
                v.iter()
                    .zip(&marks)
                    .filter(|(_, m)| **m >= k)
                    .map(|(v, _)| *v)
                    .collect::<Vec<_>>()
            };
            let (sx, sy, sz) = (keep(&columns.x), keep(&columns.y), keep(&columns.z));
            let blob = codec::encode_xyz(&sx, &sy, &sz).map_err(err)?;
            conn.execute(
                "INSERT OR REPLACE INTO pointcloud_lod (block, level, count, z1, xyz)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, i64::from(k), sx.len() as i64, top, blob],
            )?;
        }
    }
    Ok(())
}

/// The cloud `name` of the project file `conn`.
pub fn cloud_named(conn: &Connection, name: &str) -> Result<CloudInfo, StoreError> {
    crate::clouds_in(conn)?
        .into_iter()
        .find(|c| c.name == name)
        .ok_or_else(|| err(format!("no point cloud \"{name}\"")))
}

fn units_of(cloud: &CloudInfo) -> i64 {
    (cloud.chunk_size / cloud.scale).round() as i64
}

/// Applies `changes`, in order, to `cloud` in `conn` (the caller's
/// transaction; nothing is written when one fails: the caller rolls
/// back). Returns them as applied — with the indices where each point was
/// taken from and put —, whose inverses in reverse order give the blocks
/// back bit for bit.
///
/// Refused: a point not in the cloud (changed or removed since), a change
/// whose block would exceed the integer ranges.
///
/// Cost: decoding and encoding the blocks touched (a block of 10⁵ points
/// in a few ms).
pub fn change_points(
    conn: &Connection,
    cloud: &CloudInfo,
    changes: &[PointChange],
) -> Result<Vec<PointChange>, StoreError> {
    let units = units_of(cloud);
    let mut blocks: BTreeMap<(i64, i64, u8), Block> = BTreeMap::new();
    let mut applied = Vec::with_capacity(changes.len());
    for c in changes {
        let from_key = c.from.block(units);
        if let std::collections::btree_map::Entry::Vacant(e) = blocks.entry(from_key) {
            e.insert(read_block(conn, cloud, units, from_key)?);
        }
        let from_block = blocks.get_mut(&from_key).expect("read above");
        let Some(i) = from_block.find(&c.from, c.from_index) else {
            let p = c.from.position(cloud);
            return Err(err(format!(
                "no point of class {} at {:.3}, {:.3}, {:.3} in point cloud \"{}\" (changed since?)",
                c.from.class, p[0], p[1], p[2], cloud.name
            )));
        };
        from_block.remove(i);
        let to_key = c.to.block(units);
        if let std::collections::btree_map::Entry::Vacant(e) = blocks.entry(to_key) {
            e.insert(read_block(conn, cloud, units, to_key)?);
        }
        let to_block = blocks.get_mut(&to_key).expect("read above");
        let j = to_block.place(&c.to, c.to_index);
        to_block.insert(j, &c.to);
        applied.push(PointChange {
            from: c.from,
            from_index: Some(i as u64),
            to: c.to,
            to_index: Some(j as u64),
        });
    }
    let cloud_has_lod = conn
        .prepare(
            "SELECT 1 FROM pointcloud_lod l JOIN pointcloud_block b ON b.id = l.block
             WHERE b.cloud = ?1 LIMIT 1",
        )?
        .exists(params![cloud.id])?;
    for (key, b) in &blocks {
        write_block(conn, cloud, units, *key, b, cloud_has_lod)?;
    }
    crate::update_summary(conn, cloud)?;
    if cloud_has_lod {
        crate::update_top(conn, cloud)?;
    }
    Ok(applied)
}

/// Whether every point of `points` is in `cloud` (as often as listed).
pub fn points_present(
    conn: &Connection,
    cloud: &CloudInfo,
    points: &[StoredPoint],
) -> Result<bool, StoreError> {
    let units = units_of(cloud);
    let mut wanted: BTreeMap<(i64, i64, u8), BTreeMap<StoredPoint, usize>> = BTreeMap::new();
    for p in points {
        *wanted
            .entry(p.block(units))
            .or_default()
            .entry(*p)
            .or_default() += 1;
    }
    for (key, ps) in wanted {
        let b = read_block(conn, cloud, units, key)?;
        let mut have: BTreeMap<StoredPoint, usize> = BTreeMap::new();
        for i in 0..b.len() {
            let p = b.at(i, key.2);
            if ps.contains_key(&p) {
                *have.entry(p).or_default() += 1;
            }
        }
        if ps
            .iter()
            .any(|(p, n)| have.get(p).copied().unwrap_or(0) < *n)
        {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The stored point nearest to `at` (m) within `within` m (in 3D), of
/// class `class` (any without one), with its index in its block: the
/// exact point behind a point drawn (drawn coordinates are rounded).
///
/// Cost: decoding the blocks of the chunks around `at` (a few ms).
pub fn find_point(
    conn: &Connection,
    cloud: &CloudInfo,
    at: [f64; 3],
    class: Option<u8>,
    within: f64,
) -> Result<Option<(StoredPoint, u64)>, StoreError> {
    let units = units_of(cloud);
    let c = cloud.chunk_size;
    let lo = [
        ((at[0] - within) / c).floor() as i64,
        ((at[1] - within) / c).floor() as i64,
    ];
    let hi = [
        ((at[0] + within) / c).floor() as i64,
        ((at[1] + within) / c).floor() as i64,
    ];
    let keys: Vec<(i64, i64, u8)> = conn
        .prepare(
            "SELECT cx, cy, class FROM pointcloud_block
             WHERE cloud = ?1 AND cx BETWEEN ?2 AND ?3 AND cy BETWEEN ?4 AND ?5",
        )?
        .query_map(params![cloud.id, lo[0], hi[0], lo[1], hi[1]], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? as u8))
        })?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|k| class.is_none_or(|c| c == k.2))
        .collect();
    let mut best: Option<(f64, StoredPoint, u64)> = None;
    for key in keys {
        let b = read_block(conn, cloud, units, key)?;
        for i in 0..b.len() {
            let p = b.at(i, key.2);
            let q = p.position(cloud);
            let d =
                ((q[0] - at[0]).powi(2) + (q[1] - at[1]).powi(2) + (q[2] - at[2]).powi(2)).sqrt();
            if d <= within && best.as_ref().is_none_or(|b| d < b.0) {
                best = Some((d, p, i as u64));
            }
        }
    }
    Ok(best.map(|(_, p, i)| (p, i)))
}

impl crate::Store {
    /// [`change_points`] on cloud `name`, in a transaction of its own.
    pub fn change_points(
        &mut self,
        name: &str,
        changes: &[PointChange],
    ) -> Result<Vec<PointChange>, StoreError> {
        let tx = self.conn.transaction()?;
        let cloud = cloud_named(&tx, name)?;
        let applied = change_points(&tx, &cloud, changes)?;
        tx.commit()?;
        Ok(applied)
    }

    /// [`find_point`] in cloud `name`.
    pub fn find_point(
        &self,
        name: &str,
        at: [f64; 3],
        class: Option<u8>,
        within: f64,
    ) -> Result<Option<(StoredPoint, u64)>, StoreError> {
        let cloud = cloud_named(&self.conn, name)?;
        find_point(&self.conn, &cloud, at, class, within)
    }
}
