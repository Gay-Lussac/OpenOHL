// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The operation stream by interest (protocol 3, ADR-0013 phase 5): what a
//! server on a network sends each client.
//!
//! A client declares its **interest** — whole models (a replica of a
//! model), routes (its working set) and tiles of the server's grid (the
//! context it views) — and receives every change touching it, nothing
//! else, plus a **heartbeat** carrying the server's current
//! [`Position`] and the position of the last change sent to that client:
//! a client whose own last position is older has missed something
//! ([`Gap`]) and catches up from its position ([`crate::ClientMsg::Catch`]).
//! Taking a tile sends it at its revision ([`TilePackage`]), then the
//! changes after; a change touching very many tiles goes to the clients
//! that only view those tiles as "tiles … changed: read again"
//! ([`crate::ServerMsg::TilesChanged`]).
//!
//! Order without copies: every change is encoded once and broadcast to
//! every connection ([`crate::server`]); each connection's forwarder keeps
//! its client's interest (`View`) and passes on only what touches it.
//! What is for one client alone — a tile package, a catch-up, a change of
//! its interest — goes on that client's own queue, marked with the place
//! in the broadcast it belongs after (`Feed`): the forwarder merges both
//! in the authority's order, so a tile taken is followed by exactly the
//! changes after its revision, none missed, none twice.

// Without a network's server, part of this is not used (a file server's
// clients see everything).
#![cfg_attr(not(feature = "network"), allow(dead_code))]

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::Utf8Bytes;

use crate::{Op, SceneSpan, ServerMsg};

/// A place in a network's stream of changes: the log entry the network is
/// at (`seq`, the change's own last entry if it made entries) and, for a
/// change that made none, its step after that entry (1, 2, …; 0: it made
/// entries). Positions only grow (one writer); they survive a restart of
/// the server (kept in the database).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Position {
    /// The network's log entry.
    pub seq: i64,
    /// Changes after it that made no log entry.
    pub step: u32,
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.step == 0 {
            write!(f, "{}", self.seq)
        } else {
            write!(f, "{}.{}", self.seq, self.step)
        }
    }
}

/// A tile of the server's packages: level, column, row of the network's
/// grid ([`ohl_core::grid::Tile`]), written `[level, x, y]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TileKey(pub i8, pub i64, pub i64);

impl TileKey {
    /// Of a grid tile.
    pub fn of(t: &ohl_core::grid::Tile) -> Self {
        Self(t.level, t.x, t.y)
    }

    /// As a grid tile.
    pub fn tile(&self) -> ohl_core::grid::Tile {
        ohl_core::grid::Tile {
            level: self.0,
            x: self.1,
            y: self.2,
        }
    }
}

impl std::fmt::Display for TileKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}/{}", self.0, self.1, self.2)
    }
}

/// A tile a client asks for, with the revision it holds (none: it holds
/// none — the package comes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeldTile {
    /// The tile.
    pub tile: TileKey,
    /// The revision the client holds.
    #[serde(default)]
    pub rev: Option<Position>,
}

/// A change applied on a network, as its clients receive it (and replay
/// it, when they hold a replica of its model).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Change {
    /// The model it acted on.
    pub model: String,
    /// Who made it.
    pub user: String,
    /// The operation.
    pub op: Op,
    /// The model's project line (branch) it acted on.
    pub line: String,
    /// The log entry the session gave as its own (its last), if any.
    pub seq: Option<i64>,
    /// The first log entry it made, if it made entries (a replica numbers
    /// its entries from there: [`crate::client::replay_change`]).
    #[serde(default)]
    pub first: Option<i64>,
    /// Its place in the stream; the new revision of the tiles it touched.
    pub at: Position,
    /// The tiles it touched (their content changed).
    #[serde(default)]
    pub tiles: Vec<TileKey>,
    /// The routes it touched.
    #[serde(default)]
    pub routes: Vec<String>,
}

/// What a network holds, sent after the welcome.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetworkInfo {
    /// The network's name (may be empty).
    pub name: String,
    /// Its coordinate system.
    pub crs: String,
    /// Width of level-0 tiles of its grid, m.
    pub grid_size: f64,
    /// The level of the tile packages.
    pub tile_level: i8,
    /// The weather case the context's wires are solved in.
    pub context_case: String,
    /// The position the stream is at.
    pub at: Position,
    /// The heartbeat's period, ms.
    pub heartbeat_ms: u64,
    /// Plan bounds of everything (x0, y0, x1, y1), if anything.
    pub bounds: Option<[f64; 4]>,
    /// The models.
    pub models: Vec<ModelInfo>,
    /// The routes of every model.
    pub routes: Vec<RouteInfo>,
    /// The admin messages' version the server speaks
    /// ([`crate::admin::ADMIN`]; 0: a server of before them).
    #[serde(default)]
    pub admin: u32,
}

/// A model of a network, listed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Its code.
    pub code: String,
    /// Its name.
    pub name: String,
    /// Structures of its main branch.
    pub structures: usize,
}

/// A route of a network, listed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteInfo {
    /// Its id (unique in the network).
    pub id: String,
    /// Its model.
    pub model: String,
    /// Its name.
    pub name: String,
    /// Plan bounds of its alignments (x0, y0, x1, y1), if it has any.
    pub bounds: Option<[f64; 4]>,
}

/// One tile of the context: light, read-only (plan §5.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TilePackage {
    /// The tile.
    pub tile: TileKey,
    /// Its revision: the position of the last change touching it.
    pub rev: Position,
    /// Its area (x0, y0, x1, y1), m.
    pub bounds: [f64; 4],
    /// The weather case of its wires.
    pub case: String,
    /// The structures standing in it.
    pub structures: Vec<ContextStructure>,
    /// The sections passing through it, solved in the context case.
    pub wires: Vec<ContextWire>,
    /// The alignments passing through it (route polylines).
    pub routes: Vec<ContextRoute>,
    /// Names to write: routes at their alignments' middles.
    pub labels: Vec<ContextLabel>,
    /// What could not be done.
    #[serde(default)]
    pub notes: Vec<String>,
}

/// A structure of the context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextStructure {
    /// Its model.
    pub model: String,
    /// Its id.
    pub id: String,
    /// Where it stands (centre at ground), m.
    pub position: [f64; 3],
    /// Orientation of its x axis, degrees counter-clockwise from east.
    pub orientation_deg: f64,
    /// Its type, if typed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    /// Its height above its position, m.
    pub height: f64,
    /// A simplified stick drawing: segments (from, to), m (centimetres).
    pub stick: Vec<[[f64; 3]; 2]>,
}

/// A section of the context, solved in the context case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextWire {
    /// Its model.
    pub model: String,
    /// Its id.
    pub section: String,
    /// Its colour (`#rrggbb`).
    pub color: String,
    /// One per span (empty when it did not solve).
    pub spans: Vec<SceneSpan>,
    /// Why it did not solve.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// An alignment of the context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextRoute {
    /// Its model.
    pub model: String,
    /// The route.
    pub route: String,
    /// The alignment.
    pub alignment: String,
    /// Its points, in order, m.
    pub points: Vec<[f64; 3]>,
}

/// A name to write.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextLabel {
    /// The text.
    pub text: String,
    /// Where, m.
    pub at: [f64; 3],
}

/// A heartbeat says a client missed changes: its last position is older
/// than the last one the server sent it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gap {
    /// What the client has.
    pub have: Option<Position>,
    /// What the server sent last.
    pub sent: Position,
}

/// Checks a heartbeat against what a client received last (`have`).
pub fn check_heartbeat(have: Option<Position>, last: Option<Position>) -> Result<(), Gap> {
    match last {
        Some(sent) if have.is_none_or(|h| h < sent) => Err(Gap { have, sent }),
        _ => Ok(()),
    }
}

/// What a change touched, for the forwarders' filter.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Touch {
    pub(crate) model: String,
    /// Sorted.
    pub(crate) tiles: Vec<TileKey>,
    pub(crate) routes: Vec<String>,
    pub(crate) at: Position,
    /// Too large to follow change by change for a client viewing tiles
    /// only: it reads its tiles again.
    pub(crate) large: bool,
    /// Phase 7: the clients that may not see all of it: they read their
    /// tiles it touched again (filtered), nothing more.
    pub(crate) restricted: Arc<HashSet<u64>>,
}

/// Who a broadcast is for.
#[derive(Debug, Clone)]
pub(crate) enum To {
    /// Everyone (file mode, users, locks, notices).
    All,
    /// The clients whose interest it touches (network mode).
    Touching(Arc<Touch>),
    /// Each network client turns it into its own heartbeat.
    Heartbeat(Position),
}

/// A change of a client's interest, applied in the stream's order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Update {
    /// The models it follows whole.
    Models(BTreeSet<String>),
    /// The routes of its working set.
    Routes(BTreeSet<String>),
    /// Tiles taken (their packages sent or held already).
    AddTiles(Vec<TileKey>),
    /// Tiles left.
    RemoveTiles(Vec<TileKey>),
    /// Changes up to this position were sent to it (a catch-up).
    Seen(Position),
}

/// Something for one client alone, after broadcast `after`.
#[derive(Debug, Clone)]
pub(crate) struct Own {
    pub(crate) after: u64,
    pub(crate) update: Option<Update>,
    /// A message encoded once (a tile package shared by several clients).
    pub(crate) text: Option<Arc<Utf8Bytes>>,
    /// A message of its own.
    pub(crate) msg: Option<ServerMsg>,
}

/// Messages waiting for one client on its own queue.
pub(crate) const OWN_QUEUE: usize = 1024;

/// What a forwarder passes to its client.
#[derive(Debug, Clone)]
pub(crate) enum Deliver {
    /// Encoded once.
    Shared(Utf8Bytes),
    /// Of its own.
    Own(Box<ServerMsg>),
}

/// A client's interest as its forwarder keeps it.
#[derive(Debug, Clone, Default)]
pub(crate) struct View {
    /// Everything (file mode: a full replica).
    pub(crate) all: bool,
    pub(crate) models: BTreeSet<String>,
    pub(crate) routes: BTreeSet<String>,
    pub(crate) tiles: HashSet<TileKey>,
    /// The last change passed on.
    pub(crate) last: Option<Position>,
    /// Phase 7: its user does not see everything (the users' presence and
    /// the soft locks taken out of the broadcasts).
    pub(crate) restricted: bool,
}

impl View {
    /// A view of everything (file mode).
    pub(crate) fn everything() -> Self {
        Self {
            all: true,
            ..Self::default()
        }
    }

    fn apply(&mut self, u: Update) {
        match u {
            Update::Models(m) => self.models = m,
            Update::Routes(r) => self.routes = r,
            Update::AddTiles(t) => self.tiles.extend(t),
            Update::RemoveTiles(t) => {
                for k in t {
                    self.tiles.remove(&k);
                }
            }
            Update::Seen(p) => self.last = self.last.max(Some(p)),
        }
    }

    /// What of broadcast `to` (its encoding `text`) goes to this client.
    pub(crate) fn broadcast(&mut self, to: &To, text: &Utf8Bytes, client: u64) -> Option<Deliver> {
        match to {
            To::All => Some(Deliver::Shared(text.clone())),
            To::Heartbeat(at) => (!self.all).then(|| {
                Deliver::Own(Box::new(ServerMsg::Heartbeat {
                    at: *at,
                    last: self.last,
                }))
            }),
            To::Touching(t) => {
                if self.all {
                    return Some(Deliver::Shared(text.clone()));
                }
                let model = self.models.contains(&t.model);
                let route = t.routes.iter().any(|r| self.routes.contains(r));
                let tiles: Vec<TileKey> = t
                    .tiles
                    .iter()
                    .filter(|k| self.tiles.contains(k))
                    .copied()
                    .collect();
                if !model && !route && tiles.is_empty() {
                    return None;
                }
                if t.restricted.contains(&client) {
                    // Not all of it may be seen: its tiles read again
                    // (through the filter), never the change.
                    if tiles.is_empty() {
                        return None;
                    }
                    self.last = self.last.max(Some(t.at));
                    return Some(Deliver::Own(Box::new(ServerMsg::TilesChanged {
                        tiles,
                        at: t.at,
                        reason: "changed: read these tiles again".into(),
                    })));
                }
                self.last = self.last.max(Some(t.at));
                if t.large && !model && !route {
                    return Some(Deliver::Own(Box::new(ServerMsg::TilesChanged {
                        tiles,
                        at: t.at,
                        reason: "a change of very many objects: read these tiles again".into(),
                    })));
                }
                Some(Deliver::Shared(text.clone()))
            }
        }
    }

    /// What of an item of the client's own queue goes to it.
    pub(crate) fn own(&mut self, own: Own) -> Option<Deliver> {
        if let Some(u) = own.update {
            self.apply(u);
        }
        match (own.text, own.msg) {
            (Some(t), _) => Some(Deliver::Shared((*t).clone())),
            (None, Some(m)) => Some(Deliver::Own(Box::new(m))),
            (None, None) => None,
        }
    }
}

/// One broadcast event as a feed gives it.
pub(crate) enum Item {
    Event(crate::server::Event),
    Own(Own),
}

/// Why a feed stopped.
pub(crate) enum FeedEnd {
    /// The client fell too far behind the broadcasts.
    Lagged,
    /// The authority let the client go (its own queue full, or its rights
    /// changed): what was queued for it delivered first.
    LetGo,
    /// The server stopped.
    Closed,
}

/// A connection's broadcasts and own queue, merged in the authority's
/// order (see the module's notes).
pub(crate) struct Feed {
    events: broadcast::Receiver<crate::server::Event>,
    own: Option<mpsc::Receiver<Own>>,
    held: VecDeque<Own>,
    held_event: Option<crate::server::Event>,
    /// The last broadcast passed on.
    index: u64,
    /// The authority let the client go: what is held is delivered, then
    /// the feed ends.
    let_go: bool,
}

impl Feed {
    pub(crate) fn new(
        events: broadcast::Receiver<crate::server::Event>,
        own: Option<mpsc::Receiver<Own>>,
        index: u64,
    ) -> Self {
        Self {
            events,
            own,
            held: VecDeque::new(),
            held_event: None,
            index,
            let_go: false,
        }
    }

    fn drain_own(&mut self) {
        if let Some(own) = &mut self.own {
            while let Ok(o) = own.try_recv() {
                self.held.push_back(o);
            }
        }
    }

    /// The next item, in order.
    pub(crate) async fn next(&mut self) -> Result<Item, FeedEnd> {
        loop {
            if self.let_go {
                return match self.held.pop_front() {
                    Some(o) => Ok(Item::Own(o)),
                    None => Err(FeedEnd::LetGo),
                };
            }
            let index = self.index;
            if let Some(o) = self.held.pop_front_if(|o| o.after <= index) {
                return Ok(Item::Own(o));
            }
            if let Some(ev) = self.held_event.take() {
                if let Some(o) = self.held.pop_front_if(|o| o.after < ev.index) {
                    self.held_event = Some(ev);
                    return Ok(Item::Own(o));
                }
                self.index = ev.index;
                return Ok(Item::Event(ev));
            }
            let own_open = self.own.is_some();
            tokio::select! {
                ev = self.events.recv() => match ev {
                    Ok(ev) => {
                        // Whatever the authority queued before this event
                        // is in the own queue already.
                        self.drain_own();
                        self.held_event = Some(ev);
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => return Err(FeedEnd::Lagged),
                    Err(broadcast::error::RecvError::Closed) => return Err(FeedEnd::Closed),
                },
                o = async { self.own.as_mut().expect("open").recv().await }, if own_open => match o {
                    Some(o) => self.held.push_back(o),
                    // The authority let the client go (too far behind, or
                    // its rights changed): what it queued first.
                    None => {
                        self.own = None;
                        self.let_go = true;
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A client's view passes on what touches its interest: its model
    /// whole, its routes, its tiles; a large change as "read again" (only
    /// its own tiles) to a client viewing tiles only; everything to a file
    /// server's client; a heartbeat with the last change passed on.
    #[test]
    fn views_filter_by_interest() {
        let p = |seq| Position { seq, step: 0 };
        let text = Utf8Bytes::from("{}");
        let touch = |model: &str, tiles: Vec<TileKey>, routes: &[&str], at, large| {
            To::Touching(Arc::new(Touch {
                model: model.into(),
                tiles,
                routes: routes.iter().map(|r| (*r).to_owned()).collect(),
                at,
                large,
                restricted: Arc::new(HashSet::new()),
            }))
        };
        let (t1, t2, t3) = (TileKey(2, 1, 1), TileKey(2, 1, 2), TileKey(2, 9, 9));
        let mut viewer = View::default();
        viewer.apply(Update::AddTiles(vec![t1, t2]));
        let mut desktop = View::default();
        desktop.apply(Update::Models(["A".to_owned()].into()));
        let mut worker = View::default();
        worker.apply(Update::Routes(["R7".to_owned()].into()));
        let mut file = View::everything();
        // A change of model B in tile t1 on route R7.
        let c = touch("B", vec![t1, t3], &["R7"], p(5), false);
        assert!(matches!(
            viewer.broadcast(&c, &text, 1),
            Some(Deliver::Shared(_))
        ));
        assert!(desktop.broadcast(&c, &text, 1).is_none());
        assert!(matches!(
            worker.broadcast(&c, &text, 1),
            Some(Deliver::Shared(_))
        ));
        assert!(matches!(
            file.broadcast(&c, &text, 1),
            Some(Deliver::Shared(_))
        ));
        // A large change of model A in tiles t2 and t3.
        let big = touch("A", vec![t2, t3], &[], p(6), true);
        match viewer.broadcast(&big, &text, 1) {
            Some(Deliver::Own(m)) => match *m {
                ServerMsg::TilesChanged { tiles, at, .. } => {
                    assert_eq!((tiles, at), (vec![t2], p(6)));
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            desktop.broadcast(&big, &text, 1),
            Some(Deliver::Shared(_))
        ));
        assert!(worker.broadcast(&big, &text, 1).is_none());
        // Tiles left: nothing more of them; the heartbeat tells the last.
        viewer.apply(Update::RemoveTiles(vec![t1, t2]));
        assert!(viewer.broadcast(&c, &text, 1).is_none());
        match viewer.broadcast(&To::Heartbeat(p(9)), &text, 1) {
            Some(Deliver::Own(m)) => {
                assert_eq!(
                    *m,
                    ServerMsg::Heartbeat {
                        at: p(9),
                        last: Some(p(6))
                    }
                );
            }
            other => panic!("{other:?}"),
        }
        assert!(file.broadcast(&To::Heartbeat(p(9)), &text, 1).is_none());
        // A catch-up seen.
        desktop.apply(Update::Seen(p(8)));
        assert_eq!(desktop.last, Some(p(8)));
    }

    /// Phase 7: a client that may not see all of a change gets its tiles
    /// to read again, never the change (and nothing when it views none of
    /// them, even following its model or routes).
    #[test]
    fn a_restricted_client_reads_its_tiles_again() {
        let p = |seq| Position { seq, step: 0 };
        let text = Utf8Bytes::from("{}");
        let (t1, t2) = (TileKey(2, 1, 1), TileKey(2, 5, 5));
        let to = To::Touching(Arc::new(Touch {
            model: "A".into(),
            tiles: vec![t1, t2],
            routes: vec!["R1".into()],
            at: p(4),
            large: false,
            restricted: Arc::new([7u64].into()),
        }));
        let mut viewer = View::default();
        viewer.apply(Update::AddTiles(vec![t1]));
        match viewer.broadcast(&to, &text, 7) {
            Some(Deliver::Own(m)) => match *m {
                ServerMsg::TilesChanged { tiles, at, .. } => {
                    assert_eq!((tiles, at), (vec![t1], p(4)))
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
        // Another client, not restricted: the change itself.
        assert!(matches!(
            viewer.broadcast(&to, &text, 8),
            Some(Deliver::Shared(_))
        ));
        let mut worker = View::default();
        worker.apply(Update::Routes(["R1".to_owned()].into()));
        assert!(worker.broadcast(&to, &text, 7).is_none());
    }

    #[test]
    fn heartbeats_tell_gaps() {
        let p = |seq, step| Position { seq, step };
        assert!(check_heartbeat(None, None).is_ok());
        assert!(check_heartbeat(Some(p(3, 0)), Some(p(3, 0))).is_ok());
        assert!(check_heartbeat(Some(p(4, 1)), Some(p(4, 0))).is_ok());
        let gap = check_heartbeat(Some(p(3, 0)), Some(p(4, 0))).unwrap_err();
        assert_eq!(gap.sent, p(4, 0));
        assert!(check_heartbeat(None, Some(p(1, 0))).is_err());
        assert!(p(4, 0) < p(4, 1) && p(4, 1) < p(5, 0));
        assert_eq!(p(4, 2).to_string(), "4.2");
        assert_eq!(
            serde_json::to_string(&TileKey(2, -3, 4)).unwrap(),
            "[2,-3,4]"
        );
    }
}
