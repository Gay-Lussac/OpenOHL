// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What the server keeps beside the session's rows so that its clients can
//! be served by interest (ADR-0013, phase 5; schema 2):
//!
//! - `object.routes`: the routes each element of a model's main branch is
//!   on ([`ohl_session::Session::object_routes`]), written with every
//!   command and at import;
//! - `object_ref`: what each element refers to (the reference index's
//!   targets), written with every command and at import;
//! - `oplog.lines` and `oplog.tiles`: the models (design lines) and the
//!   package tiles an entry touched;
//! - `stream`: every change the server applied and broadcast, logged or
//!   not, at its **position** (the network's log number and, for a change
//!   that made no log entry, a step after it), with the operation as the
//!   clients replay it and what it touched — read again by a client
//!   catching up after a disconnect.
//!
//! Tiles are kept as one number each ([`tile_key`]): level, column and
//! row of the network's grid. A position is (`seq`, `step`): `seq` the
//! network's last log entry when the change was made (its own, if it made
//! entries), `step` 0 for a change that made log entries, 1, 2, … for the
//! changes after the same entry that made none. Positions only grow: one
//! writer.

use std::collections::BTreeSet;

use ohl_core::grid::{Bounds, Grid, Tile};
use ohl_session::{Kind, Session};

use crate::{NetError, Network, lock};

/// The kinds whose routes and references are kept (the line's elements).
pub const LINE_KINDS: [Kind; 7] = [
    Kind::Route,
    Kind::Alignment,
    Kind::AlignmentPoint,
    Kind::Structure,
    Kind::Section,
    Kind::Jumper,
    Kind::Spacer,
];

/// The level of the server's tile packages unless the network says
/// otherwise (`net_meta` `tile_level`): 2, tiles of 1 024 m with the
/// default grid (a view of a few kilometres takes a few dozen).
pub const DEFAULT_TILE_LEVEL: i8 = 2;

const BITS: u32 = 28;
const MASK: i64 = (1 << BITS) - 1;

/// A tile as one number: the level (+128) in the top 8 bits, the column
/// and the row in 28 bits each (two's complement). `None`: a column or
/// row beyond ±2²⁷ (no grid on Earth has so many tiles).
pub fn tile_key(t: &Tile) -> Option<i64> {
    let limit = 1i64 << (BITS - 1);
    if !(-limit..limit).contains(&t.x) || !(-limit..limit).contains(&t.y) {
        return None;
    }
    let level = (i64::from(t.level) + 128) & 0xff;
    Some((level << 56) | ((t.x & MASK) << BITS) | (t.y & MASK))
}

/// The tile of a number made by [`tile_key`].
pub fn tile_of_key(key: i64) -> Tile {
    let signed = |v: i64| {
        let v = v & MASK;
        if v >= 1 << (BITS - 1) {
            v - (1 << BITS)
        } else {
            v
        }
    };
    Tile {
        level: (((key >> 56) & 0xff) - 128) as i8,
        x: signed(key >> BITS),
        y: signed(key),
    }
}

/// The package tiles of `level` that `bounds` touches.
pub fn tiles_of(grid: &Grid, bounds: &Bounds, level: i8) -> Vec<Tile> {
    let r = grid.covering(bounds, level);
    let mut out = Vec::new();
    // A degenerate or absurd extent: no tiles rather than millions.
    if r.x[1] - r.x[0] > 4096 || r.y[1] - r.y[0] > 4096 {
        return out;
    }
    for x in r.x[0]..=r.x[1] {
        for y in r.y[0]..=r.y[1] {
            out.push(Tile { level, x, y });
        }
    }
    out
}

/// A change the server applied and broadcast (a row of `stream`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamRow {
    /// The network's log number it is at (its own last entry, if it made
    /// entries).
    pub seq: i64,
    /// 0: it made log entries; 1, 2, …: the changes after entry `seq` that
    /// made none.
    pub step: i32,
    /// The model it acted on.
    pub line: String,
    /// The branch (project line) it acted on.
    pub branch: String,
    /// Who made it.
    pub user: String,
    /// The operation as the clients replay it (the server's protocol,
    /// JSON).
    pub op: String,
    /// The first log entry it made (none: it made none).
    pub first_seq: Option<i64>,
    /// The log entry the session gave as its own (its last), if any.
    pub log_seq: Option<i64>,
    /// The package tiles it touched ([`tile_key`]).
    pub tiles: Vec<i64>,
    /// The routes it touched.
    pub routes: Vec<String>,
    /// The objects it touched (`kind/id`; phase 7, for the outgoing filter
    /// of a catch-up; `None`: not kept, before schema 4).
    pub objects: Option<Vec<String>>,
    /// It touched its model at large (a merge, a library file).
    pub whole: bool,
}

/// What each element of a model's main branch is on and refers to, as
/// [`write_interest`] keeps it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Interest {
    /// (kind, id, routes) per element.
    pub routes: Vec<(Kind, String, BTreeSet<String>)>,
    /// (from kind, from id, to kind, to id, set number) per reference;
    /// set number 0: the object itself, n + 1: its wire point set n.
    pub refs: Vec<(Kind, String, Kind, String, i32)>,
    /// The elements whose references these are (their old rows go).
    pub from: Vec<(Kind, String)>,
}

/// The routes and references of `objects` of `session`'s line worked on
/// (its main branch); elements no longer in the line get no routes and no
/// references (their rows go).
pub fn interest_of(session: &mut Session, objects: &[(Kind, String)]) -> Interest {
    let objects: Vec<(Kind, String)> = objects
        .iter()
        .filter(|(k, _)| LINE_KINDS.contains(k))
        .cloned()
        .collect();
    let routes = session.object_routes(&objects);
    let mut out = Interest::default();
    for ((kind, id), r) in objects.iter().zip(routes) {
        out.routes.push((*kind, id.clone(), r));
        for (to_kind, to_id, set) in session.object_refs(*kind, id) {
            let set_no = set.map_or(0, |s| i32::try_from(s).unwrap_or(i32::MAX - 1) + 1);
            out.refs.push((*kind, id.clone(), to_kind, to_id, set_no));
        }
        out.from.push((*kind, id.clone()));
    }
    out
}

/// The routes and references of every element of `session`'s line.
pub fn interest_of_line(session: &mut Session) -> Interest {
    let line = session.line();
    let mut objects: Vec<(Kind, String)> = Vec::new();
    objects.extend(line.routes.iter().map(|x| (Kind::Route, x.id.clone())));
    objects.extend(
        line.alignments
            .iter()
            .map(|x| (Kind::Alignment, x.id.clone())),
    );
    objects.extend(
        line.alignment_points
            .iter()
            .map(|x| (Kind::AlignmentPoint, x.id.clone())),
    );
    objects.extend(
        line.structures
            .iter()
            .map(|x| (Kind::Structure, x.id.clone())),
    );
    objects.extend(line.sections.iter().map(|x| (Kind::Section, x.id.clone())));
    objects.extend(line.jumpers.iter().map(|x| (Kind::Jumper, x.id.clone())));
    objects.extend(line.spacers.iter().map(|x| (Kind::Spacer, x.id.clone())));
    interest_of(session, &objects)
}

/// Batches of rows written in one statement.
const BATCH: usize = 2000;

/// The routes joined for one text column (unit separator; empty: none).
fn joined(routes: &BTreeSet<String>) -> String {
    routes.iter().cloned().collect::<Vec<_>>().join("\u{1f}")
}

/// Writes `interest` of model `line`, branch `branch`: each element's
/// routes, and its references in place of its old ones.
pub fn write_interest(
    net: &Network,
    line: &str,
    branch: &str,
    interest: &Interest,
) -> Result<(), NetError> {
    net.require_writer()?;
    let shared = net.shared();
    for chunk in interest.routes.chunks(BATCH) {
        let kinds: Vec<&str> = chunk.iter().map(|(k, _, _)| k.as_str()).collect();
        let ids: Vec<&str> = chunk.iter().map(|(_, i, _)| i.as_str()).collect();
        let routes: Vec<String> = chunk.iter().map(|(_, _, r)| joined(r)).collect();
        lock(&shared).execute(
            "UPDATE object o SET routes = CASE WHEN v.r = '' THEN '{}'::text[]
                 ELSE string_to_array(v.r, chr(31)) END
             FROM unnest($3::text[], $4::text[], $5::text[]) AS v(k, i, r)
             WHERE o.line_id = $1 AND o.branch = $2 AND o.kind = v.k AND o.id = v.i",
            &[&line, &branch, &kinds, &ids, &routes],
        )?;
    }
    for chunk in interest.from.chunks(BATCH) {
        let kinds: Vec<&str> = chunk.iter().map(|(k, _)| k.as_str()).collect();
        let ids: Vec<&str> = chunk.iter().map(|(_, i)| i.as_str()).collect();
        lock(&shared).execute(
            "DELETE FROM object_ref r USING unnest($3::text[], $4::text[]) AS v(k, i)
             WHERE r.line_id = $1 AND r.branch = $2 AND r.from_kind = v.k AND r.from_id = v.i",
            &[&line, &branch, &kinds, &ids],
        )?;
    }
    for chunk in interest.refs.chunks(BATCH) {
        let fk: Vec<&str> = chunk.iter().map(|r| r.0.as_str()).collect();
        let fi: Vec<&str> = chunk.iter().map(|r| r.1.as_str()).collect();
        let tk: Vec<&str> = chunk.iter().map(|r| r.2.as_str()).collect();
        let ti: Vec<&str> = chunk.iter().map(|r| r.3.as_str()).collect();
        let sets: Vec<i32> = chunk.iter().map(|r| r.4).collect();
        lock(&shared).execute(
            "INSERT INTO object_ref (line_id, branch, from_kind, from_id, to_kind, to_id, set_no)
             SELECT $1, $2, a, b, c, d, e
             FROM unnest($3::text[], $4::text[], $5::text[], $6::text[], $7::int[])
                 AS v(a, b, c, d, e)
             ON CONFLICT DO NOTHING",
            &[&line, &branch, &fk, &fi, &tk, &ti, &sets],
        )?;
    }
    Ok(())
}

impl Network {
    /// The level of the server's tile packages (`net_meta` `tile_level`,
    /// else [`DEFAULT_TILE_LEVEL`]).
    pub fn tile_level(&self) -> Result<i8, NetError> {
        Ok(self
            .meta("tile_level")?
            .and_then(|v| v.parse::<i8>().ok())
            .filter(|l| (Grid::MIN_LEVEL..=Grid::MAX_LEVEL).contains(l))
            .unwrap_or(DEFAULT_TILE_LEVEL))
    }

    /// The network's grid (`net_meta` `grid_size`, else the default).
    pub fn grid(&self) -> Result<Grid, NetError> {
        Ok(match self.meta("grid_size")? {
            Some(text) => text
                .parse::<f64>()
                .map_err(|e| e.to_string())
                .and_then(Grid::new)
                .map_err(|e| NetError::Database(format!("grid_size \"{text}\": {e}")))?,
            None => Grid::default(),
        })
    }

    /// Notes the models and tiles log entries `from..=to` of model `line`
    /// touched (`oplog.lines`, `oplog.tiles`).
    pub fn set_entry_interest(
        &self,
        line: &str,
        from: i64,
        to: i64,
        lines: &[String],
        tiles: &[i64],
    ) -> Result<(), NetError> {
        self.require_writer()?;
        lock(&self.shared()).execute(
            "UPDATE oplog SET lines = $4, tiles = $5
             WHERE line_id = $1 AND seq BETWEEN $2 AND $3",
            &[&line, &from, &to, &lines, &tiles],
        )?;
        Ok(())
    }

    /// Appends a change to `stream`.
    pub fn append_stream(&self, r: &StreamRow) -> Result<(), NetError> {
        self.require_writer()?;
        lock(&self.shared()).execute(
            "INSERT INTO stream (seq, step, line_id, branch, user_name, op, first_seq, log_seq,
                 tiles, routes, objects, whole)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
            &[
                &r.seq,
                &r.step,
                &r.line,
                &r.branch,
                &r.user,
                &r.op,
                &r.first_seq,
                &r.log_seq,
                &r.tiles,
                &r.routes,
                &r.objects,
                &r.whole,
            ],
        )?;
        Ok(())
    }

    /// The network's position: its last log entry, and the changes after
    /// it that made none (the last `stream` row's step at that entry).
    pub fn position(&self) -> Result<(i64, i32), NetError> {
        let mut c = lock(&self.conn);
        let seq: i64 = c
            .query_one("SELECT COALESCE(MAX(seq), 0) FROM oplog", &[])?
            .get(0);
        let step: i32 = c
            .query_one(
                "SELECT COALESCE(MAX(step), 0) FROM stream WHERE seq = $1",
                &[&seq],
            )?
            .get(0);
        Ok((seq, step))
    }

    /// The changes after position (`seq`, `step`) of models `lines`, or
    /// touching `routes` or `tiles`, in order; at most `limit` (one more
    /// than that tells the caller there are more).
    pub fn stream_after(
        &self,
        seq: i64,
        step: i32,
        lines: &[String],
        routes: &[String],
        tiles: &[i64],
        limit: i64,
    ) -> Result<Vec<StreamRow>, NetError> {
        Ok(lock(&self.shared())
            .query(
                "SELECT seq, step, line_id, branch, user_name, op, first_seq, log_seq, tiles,
                     routes, objects, whole
                 FROM stream
                 WHERE (seq, step) > ($1, $2)
                   AND (line_id = ANY($3) OR routes && $4 OR tiles && $5)
                 ORDER BY seq, step
                 LIMIT $6",
                &[&seq, &step, &lines, &routes, &tiles, &limit],
            )?
            .into_iter()
            .map(|r| StreamRow {
                seq: r.get(0),
                step: r.get(1),
                line: r.get(2),
                branch: r.get(3),
                user: r.get(4),
                op: r.get(5),
                first_seq: r.get(6),
                log_seq: r.get(7),
                tiles: r.get(8),
                routes: r.get(9),
                objects: r.get(10),
                whole: r.get(11),
            })
            .collect())
    }

    /// The position of the last change of `stream` touching each tile.
    pub fn tile_positions(&self) -> Result<Vec<(i64, i64, i32)>, NetError> {
        Ok(lock(&self.shared())
            .query(
                "SELECT DISTINCT ON (t) t, seq, step
                 FROM (SELECT unnest(tiles) AS t, seq, step FROM stream) x
                 ORDER BY t, seq DESC, step DESC",
                &[],
            )?
            .into_iter()
            .map(|r| (r.get(0), r.get(1), r.get(2)))
            .collect())
    }

    /// Whether model `line`'s main branch has elements whose routes are not
    /// kept yet (imported before schema 2), or no references kept.
    pub fn interest_missing(&self, line: &str) -> Result<bool, NetError> {
        let kinds: Vec<&str> = LINE_KINDS.iter().map(|k| k.as_str()).collect();
        let mut c = lock(&self.conn);
        let unknown = c
            .query_opt(
                "SELECT 1 FROM object WHERE line_id = $1 AND branch = 'main' AND gone = 0
                     AND routes IS NULL AND kind = ANY($2) LIMIT 1",
                &[&line, &kinds],
            )?
            .is_some();
        Ok(unknown)
    }

    /// The elements of every model's main branch whose bounds touch the
    /// area (x0, y0, x1, y1): the spatial index's GiST query.
    pub fn objects_in_area(&self, area: [f64; 4]) -> Result<i64, NetError> {
        Ok(lock(&self.conn)
            .query_one(
                "SELECT count(*) FROM spatial
                 WHERE branch = 'main' AND geom && ST_MakeEnvelope($1, $2, $3, $4)",
                &[&area[0], &area[1], &area[2], &area[3]],
            )?
            .get(0))
    }

    /// The centres of `n` structures of the models' main branches, a
    /// sample the same every time (measurements).
    pub fn sample_structures(&self, n: i64) -> Result<Vec<[f64; 2]>, NetError> {
        Ok(lock(&self.conn)
            .query(
                "SELECT (x0 + x1) / 2, (y0 + y1) / 2 FROM spatial
                 WHERE branch = 'main' AND kind = 'structure'
                 ORDER BY md5(line_id || '/' || id) LIMIT $1",
                &[&n],
            )?
            .into_iter()
            .map(|r| [r.get(0), r.get(1)])
            .collect())
    }

    /// What is kept of element `kind`/`id` of model `line`'s main branch
    /// for the interest: its routes (`None`: not counted), its references
    /// (to kind, to id, set number).
    pub fn kept_interest(
        &self,
        line: &str,
        kind: &str,
        id: &str,
    ) -> Result<KeptInterest, NetError> {
        let mut c = lock(&self.conn);
        let routes = c
            .query_opt(
                "SELECT routes FROM object
                 WHERE line_id = $1 AND branch = 'main' AND kind = $2 AND id = $3",
                &[&line, &kind, &id],
            )?
            .and_then(|r| r.get::<_, Option<Vec<String>>>(0));
        let refs = c
            .query(
                "SELECT to_kind, to_id, set_no FROM object_ref
                 WHERE line_id = $1 AND branch = 'main' AND from_kind = $2 AND from_id = $3
                 ORDER BY to_kind, to_id, set_no",
                &[&line, &kind, &id],
            )?
            .into_iter()
            .map(|r| (r.get(0), r.get(1), r.get(2)))
            .collect();
        Ok((routes, refs))
    }

    /// The models and tiles log entry `seq` of model `line` touched.
    pub fn entry_interest(
        &self,
        line: &str,
        seq: i64,
    ) -> Result<(Vec<String>, Vec<i64>), NetError> {
        let r = lock(&self.conn).query_one(
            "SELECT lines, tiles FROM oplog WHERE line_id = $1 AND seq = $2",
            &[&line, &seq],
        )?;
        Ok((r.get(0), r.get(1)))
    }

    /// Opens a snapshot on this connection: every read until
    /// [`Network::end_snapshot`] sees the database as it is now (a
    /// `REPEATABLE READ`, read-only transaction; its snapshot taken at
    /// once). A reader for an export or a client's copy while the writer
    /// goes on.
    pub fn begin_snapshot(&self) -> Result<(), NetError> {
        let mut c = lock(&self.conn);
        if !c.steps.is_empty() {
            return Err(NetError::Refused(
                "a snapshot during a unit of work of the same connection".into(),
            ));
        }
        c.client
            .batch_execute("BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY; SELECT 1")?;
        c.steps.push("ohl_snapshot".into());
        Ok(())
    }

    /// Ends the snapshot [`Network::begin_snapshot`] opened.
    pub fn end_snapshot(&self) -> Result<(), NetError> {
        let mut c = lock(&self.conn);
        c.steps.clear();
        c.client.batch_execute("COMMIT")?;
        Ok(())
    }

    /// Begins a unit of work of the writer: every statement until
    /// [`Network::commit_unit`] (or [`Network::roll_back_unit`]) is one
    /// transaction, the sessions' own steps nested in it. The server makes
    /// each command one: the session's rows, the routes and references,
    /// the log's interest and the stream row together.
    pub fn begin_unit(&self) -> Result<(), NetError> {
        self.require_writer()?;
        use ohl_session::store::Store;
        Ok(crate::PgStore::new(self.shared(), "").savepoint(UNIT)?)
    }

    /// Commits the unit of work.
    pub fn commit_unit(&self) -> Result<(), NetError> {
        use ohl_session::store::Store;
        Ok(crate::PgStore::new(self.shared(), "").release(UNIT)?)
    }

    /// Rolls the unit of work back.
    pub fn roll_back_unit(&self) -> Result<(), NetError> {
        use ohl_session::store::Store;
        Ok(crate::PgStore::new(self.shared(), "").roll_back_to(UNIT)?)
    }
}

/// An element's routes kept (`None`: not counted) and its references
/// (to kind, to id, set number).
pub type KeptInterest = (Option<Vec<String>>, Vec<(String, String, i32)>);

/// The savepoint of the server's unit of work.
const UNIT: &str = "ohl_server_unit";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_keys_round_trip() {
        for t in [
            Tile {
                level: 0,
                x: 0,
                y: 0,
            },
            Tile {
                level: 2,
                x: 1953,
                y: 390,
            },
            Tile {
                level: -8,
                x: -1,
                y: -134_217_728,
            },
            Tile {
                level: 14,
                x: 134_217_727,
                y: -5,
            },
        ] {
            assert_eq!(tile_of_key(tile_key(&t).unwrap()), t, "{t:?}");
        }
        assert!(
            tile_key(&Tile {
                level: 0,
                x: 1 << 27,
                y: 0
            })
            .is_none()
        );
        let a = tile_key(&Tile {
            level: 2,
            x: 1,
            y: 2,
        })
        .unwrap();
        let b = tile_key(&Tile {
            level: 2,
            x: 2,
            y: 1,
        })
        .unwrap();
        assert_ne!(a, b);
    }
}
