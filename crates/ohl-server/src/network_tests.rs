// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A server on a network, on a throwaway PostgreSQL cluster
//! (`ohl_store_pg::testing`): a small synthetic country imported line by
//! line, then desktops following a model, viewers following tiles, an
//! editor on every model. Proved: every change a client receives touches
//! its interest and it receives every change that does (property, from
//! the stream kept in the database), in order; a replica equals the
//! server's model (its export compared row by row, log numbers included);
//! a disconnected desktop catches up with exactly what it missed; a
//! heartbeat shows a lost change, and the catch-up brings it; tiles at
//! their revision; one writer per database; protocol 2 refused. Without
//! PostgreSQL's binaries the tests say so and pass.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ohl_session::{Command, Role, Session};
use ohl_store_pg::Network;
use ohl_store_pg::import::{ImportOptions, import};
use ohl_store_pg::testing::Cluster;

use crate::client::{Incoming, Joined, NetJoin, join_network, replay_change};
use crate::network::NetOptions;
use crate::server::{Options, Server};
use crate::stream::check_heartbeat;
use crate::{
    Change, ClientMsg, HeldTile, Invitation, Op, Position, ServerMsg, TileKey, TilePackage,
};

/// A network of a small synthetic country, one model per route line;
/// `admin` its admin, `bob` and `carol` designers of every model.
pub(crate) struct TestNet {
    pub(crate) _cluster: Cluster,
    pub(crate) url: String,
    pub(crate) dir: PathBuf,
    pub(crate) codes: Vec<String>,
}

pub(crate) fn test_network(name: &str, pylons: usize) -> Option<TestNet> {
    let cluster = Cluster::start(name)?;
    cluster.create_db("net");
    let url = cluster.url("net");
    let dir = cluster.scratch();
    let country = ohl_synth::country(&ohl_synth::Settings {
        pylons,
        ..Default::default()
    })
    .unwrap();
    let files =
        ohl_synth::project::write_line_projects(&country, &dir.join("lines"), "admin").unwrap();
    let net = Network::connect_writer(&url).unwrap();
    let mut codes = Vec::new();
    for (code, path, _) in files {
        let mut o = ImportOptions::new(&code, "admin");
        o.revision = false;
        import(&net, &path, &o).unwrap();
        let s = Session::open_on(Box::new(net.store(&code).unwrap()), &code).unwrap();
        s.set_user("admin", "bob", Role::Designer).unwrap();
        s.set_user("admin", "carol", Role::Designer).unwrap();
        codes.push(code);
    }
    drop(net);
    // The import's writer's lock gone with its connection before a
    // server takes it (a moment after the drop: waited for).
    cluster.wait_for_locks_gone("net");
    Some(TestNet {
        _cluster: cluster,
        url,
        dir,
        codes,
    })
}

pub(crate) fn local(code: &str) -> Options {
    Options {
        bind: "127.0.0.1:0".parse().unwrap(),
        advertise: None,
        code: Some(code.into()),
        keys: None,
        newcomers: Some(Role::Viewer),
        web: None,
        ..Default::default()
    }
}

pub(crate) fn net_options(url: &str) -> NetOptions {
    NetOptions {
        database: url.into(),
        heartbeat: Duration::from_millis(150),
        packagers: 1,
        readers: 2,
        // The idle solve has its own test (working_set_tests).
        idle_solve: false,
        ..NetOptions::default()
    }
}

/// A client of the network: its replica of a model (if it follows one),
/// the changes and tiles it received.
struct C {
    joined: Joined,
    replica: Option<Session>,
    model: Option<String>,
    last: Option<Position>,
    changes: Vec<Change>,
    tiles: BTreeMap<TileKey, TilePackage>,
    heartbeats: Vec<(Position, Option<Position>)>,
    /// The next change is lost (a test of the heartbeat).
    lose_next: bool,
    /// The positions of the changes and "read again"s received.
    seen: Vec<Position>,
    next_id: u64,
}

impl C {
    fn join(invitation: &Invitation, user: &str, how: NetJoin) -> Result<C, String> {
        let joined = join_network(invitation, user, &how, &mut |_| {})?;
        let replica = match (&how.model, &how.file) {
            (Some(_), Some(f)) => Some(Session::open(f).map_err(|e| e.to_string())?),
            _ => None,
        };
        Ok(C {
            joined,
            replica,
            model: how.model,
            last: how.since,
            changes: Vec::new(),
            tiles: BTreeMap::new(),
            heartbeats: Vec::new(),
            lose_next: false,
            seen: Vec::new(),
            next_id: 0,
        })
    }

    fn take(&mut self, c: Change) {
        if let (Some(r), Some(m)) = (&mut self.replica, &self.model)
            && &c.model == m
        {
            replay_change(r, &c).unwrap();
        }
        self.last = self.last.max(Some(c.at));
        self.seen.push(c.at);
        self.changes.push(c);
    }

    /// Takes messages until `until` says stop (10 s at most).
    fn pump(&mut self, mut until: impl FnMut(&ServerMsg) -> bool) -> Vec<ServerMsg> {
        let started = Instant::now();
        let mut got = Vec::new();
        loop {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "{:?}",
                got.iter().rev().take(5).collect::<Vec<_>>()
            );
            match self.joined.link.recv_timeout(Duration::from_millis(20)) {
                Some(Incoming::Msg(m)) => {
                    match &m {
                        ServerMsg::Change(c) => {
                            if self.lose_next {
                                self.lose_next = false;
                            } else {
                                self.take((**c).clone());
                            }
                        }
                        ServerMsg::CatchUp { changes, .. } => {
                            for c in changes.clone() {
                                self.take(c);
                            }
                        }
                        ServerMsg::Tile(p) => {
                            self.tiles.insert(p.tile, (**p).clone());
                        }
                        ServerMsg::Heartbeat { at, last } => self.heartbeats.push((*at, *last)),
                        ServerMsg::TilesChanged { at, .. } => {
                            self.last = self.last.max(Some(*at));
                            self.seen.push(*at);
                        }
                        _ => {}
                    }
                    let stop = until(&m);
                    got.push(m);
                    if stop {
                        return got;
                    }
                }
                Some(Incoming::Closed(why)) => panic!("closed: {why}"),
                None => {}
            }
        }
    }

    /// Takes what comes for `ms` milliseconds.
    fn idle(&mut self, ms: u64) -> Vec<ServerMsg> {
        let until = Instant::now() + Duration::from_millis(ms);
        self.pump(|_| Instant::now() >= until)
    }

    /// An edit of model `model`; waits for its answer.
    fn edit(&mut self, model: &str, op: Op) -> Result<String, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.joined
            .link
            .send(ClientMsg::RequestOn {
                id,
                model: model.into(),
                op,
                line: "main".into(),
            })
            .unwrap();
        let got = self.pump(|m| matches!(m, ServerMsg::Response { id: i, .. } if *i == id));
        match got.last() {
            Some(ServerMsg::Response { result, .. }) => result.clone(),
            _ => unreachable!(),
        }
    }

    /// Waits for the next heartbeat; checks it shows no gap.
    fn heartbeat(&mut self) -> Result<(), crate::stream::Gap> {
        self.pump(|m| matches!(m, ServerMsg::Heartbeat { .. }));
        let (_, last) = *self.heartbeats.last().expect("a heartbeat");
        check_heartbeat(self.last, last)
    }

    /// Takes everything up to a heartbeat at `at` or later (every change
    /// sent before it has arrived then).
    fn settle(&mut self, at: Position) {
        loop {
            self.pump(|m| matches!(m, ServerMsg::Heartbeat { .. }));
            if self.heartbeats.last().is_some_and(|(p, _)| *p >= at) {
                return;
            }
        }
    }
}

/// The network's position now.
fn position(t: &TestNet) -> Position {
    let (seq, step) = Network::connect(&t.url).unwrap().position().unwrap();
    Position {
        seq,
        step: step as u32,
    }
}

pub(crate) fn moved(id: &str, p: [f64; 3]) -> Op {
    Op::Do {
        command: Command::MoveStructure {
            id: id.into(),
            position: p,
        },
    }
}

/// The objects, index and log of a project file (times left out): what a
/// replica must have as the server's export has.
fn rows(path: &Path) -> (Vec<String>, Vec<String>) {
    use ohl_session::store::Store;
    let s = ohl_session::store::sqlite::SqliteStore::open_read_only(path).unwrap();
    let mut objects: Vec<String> = s
        .object_rows()
        .unwrap()
        .into_iter()
        .map(|r| {
            format!(
                "{} {} {} {} {} {} {}",
                r.line, r.kind, r.id, r.ord, r.version, r.gone, r.json
            )
        })
        .collect();
    objects.sort();
    let entries = s
        .entries()
        .unwrap()
        .into_iter()
        .map(|e| {
            let (seq, _time, user, kind, command, inverse, objects, sections, undoes, undone, note, line) =
                e;
            format!(
                "{seq} {user} {kind} {command} {inverse} {objects} {sections} {undoes:?} {undone:?} \
                 {note:?} {line}"
            )
        })
        .collect();
    (objects, entries)
}

/// The replica of model `code` held by `c` equals the server's model:
/// the export of the model compared with the replica written out.
fn same_as_server(c: &C, t: &TestNet, code: &str, n: usize) {
    let reader = Network::connect(&t.url).unwrap();
    let theirs = t.dir.join(format!("export-{code}-{n}.ohl"));
    ohl_store_pg::export::export(&reader, code, &theirs, "test").unwrap();
    let mine = t.dir.join(format!("replica-{code}-{n}.ohl"));
    c.replica.as_ref().unwrap().copy_to(&mine).unwrap();
    let (a, b) = (rows(&mine), rows(&theirs));
    assert_eq!(a.0.len(), b.0.len());
    for (x, y) in a.0.iter().zip(&b.0) {
        assert_eq!(x, y);
    }
    assert_eq!(a.1, b.1, "the log, numbers included");
}

/// A structure of model `code` and where it stands.
pub(crate) fn structure_of(t: &TestNet, code: &str, k: usize) -> (String, [f64; 3]) {
    let net = Network::connect(&t.url).unwrap();
    let s = Session::open_on(Box::new(net.store(code).unwrap()), code).unwrap();
    let st = &s.line().structures[k % s.line().structures.len()];
    (st.id.clone(), st.position)
}

/// The tiles (level 2, 1 024 m) around a point: `r` on every side.
pub(crate) fn tiles_around(p: [f64; 3], r: i64) -> Vec<TileKey> {
    let g = ohl_core::grid::Grid::default();
    let t = g.tile_at(2, [p[0], p[1]]);
    let mut out = Vec::new();
    for dx in -r..=r {
        for dy in -r..=r {
            out.push(TileKey(2, t.x + dx, t.y + dy));
        }
    }
    out
}

fn held(tiles: &[TileKey], revs: &BTreeMap<TileKey, TilePackage>) -> Vec<HeldTile> {
    tiles
        .iter()
        .map(|t| HeldTile {
            tile: *t,
            rev: revs.get(t).map(|p| p.rev),
        })
        .collect()
}

/// Every change of the stream after `from` touching an interest (a model,
/// or tiles), in order (the property's reference).
fn expected(t: &TestNet, from: Position, model: Option<&str>, tiles: &[TileKey]) -> Vec<Position> {
    let net = Network::connect(&t.url).unwrap();
    let models: Vec<String> = model.iter().map(|m| (*m).to_owned()).collect();
    let keys: Vec<i64> = tiles
        .iter()
        .filter_map(|k| ohl_store_pg::stream::tile_key(&k.tile()))
        .collect();
    net.stream_after(from.seq, from.step as i32, &models, &[], &keys, 100_000)
        .unwrap()
        .into_iter()
        .map(|r| Position {
            seq: r.seq,
            step: r.step as u32,
        })
        .collect()
}

#[test]
fn a_network_serves_by_interest() {
    let Some(t) = test_network("interest", 360) else {
        return;
    };
    assert!(t.codes.len() >= 4, "{:?}", t.codes);
    // A change of more than 40 tiles is "read again" for a viewer.
    let server = Server::start_network(
        local("abcd2345"),
        NetOptions {
            large_tiles: 40,
            ..net_options(&t.url)
        },
    )
    .unwrap();
    // One writer per database.
    let e = Server::start_network(local("abcd2345"), net_options(&t.url))
        .err()
        .unwrap();
    assert!(e.contains("writer"), "{e}");
    // Protocol 2 (the alpha) refused, saying why.
    let old = crate::client::join_with(
        &server.invitation,
        "vic",
        ClientMsg::Hello {
            protocol: 2,
            user: "vic".into(),
            code: server.invitation.code.clone(),
        },
        Some(t.dir.join("old.ohl")),
        &mut |_| {},
    )
    .err()
    .unwrap();
    assert!(
        old.contains("protocol 2") && old.contains("update OpenOHL"),
        "{old}"
    );
    // A desktop's hello (no model) is refused with what to do.
    let hello = crate::client::join(&server.invitation, "vic", &t.dir.join("h.ohl"), &mut |_| {})
        .err()
        .unwrap();
    assert!(hello.contains("network"), "{hello}");

    let (a, b) = (t.codes[0].clone(), t.codes[t.codes.len() - 1].clone());
    // The admin follows model A whole (a replica of it).
    let mut d = C::join(
        &server.invitation_for("admin"),
        "admin",
        NetJoin {
            model: Some(a.clone()),
            file: Some(t.dir.join("replica-a.ohl")),
            ..NetJoin::default()
        },
    )
    .unwrap();
    let info = d.pump(|m| matches!(m, ServerMsg::Network(_)));
    let ServerMsg::Network(info) = info.last().unwrap() else {
        unreachable!()
    };
    assert_eq!(info.models.len(), t.codes.len());
    assert_eq!(info.tile_level, 2);
    same_as_server(&d, &t, &a, 0);
    // Vic views the tiles around a structure of B (a newcomer: a viewer).
    let (sb, pb) = structure_of(&t, &b, 3);
    let vtiles = tiles_around(pb, 1);
    let mut v = C::join(
        &server.invitation,
        "vic",
        NetJoin {
            tiles: held(&vtiles, &BTreeMap::new()),
            ..NetJoin::default()
        },
    )
    .unwrap();
    let n = vtiles.len();
    let mut seen = 0;
    v.pump(|m| {
        if matches!(m, ServerMsg::Tile(_)) {
            seen += 1;
        }
        seen == n
    });
    let shown = v
        .tiles
        .values()
        .flat_map(|p| p.structures.iter())
        .find(|s| s.id == sb)
        .expect("B's structure in its tile");
    assert_eq!(shown.model, b);
    assert!(
        v.tiles.values().any(|p| !p.wires.is_empty()),
        "wires solved"
    );
    assert!(
        v.tiles.values().any(|p| !p.routes.is_empty()),
        "routes drawn"
    );
    // Bob edits any model (following none).
    let mut e = C::join(&server.invitation_for("bob"), "bob", NetJoin::default()).unwrap();
    // A move of B's structure: Vic is told (its tile), Alice not.
    e.edit(&b, moved(&sb, [pb[0] + 1.0, pb[1], pb[2]])).unwrap();
    let got = v.pump(|m| matches!(m, ServerMsg::Change(_)));
    let ServerMsg::Change(c) = got.last().unwrap() else {
        unreachable!()
    };
    assert_eq!(
        (c.model.as_str(), &c.op),
        (b.as_str(), &moved(&sb, [pb[0] + 1.0, pb[1], pb[2]]))
    );
    assert!(c.first.is_some() && c.seq == Some(c.at.seq));
    let touched: BTreeSet<TileKey> = c.tiles.iter().copied().collect();
    assert!(!touched.is_empty());
    // Kept with the command: the structure's routes and references, the
    // log entry's models and tiles, the change in the stream.
    {
        let net = Network::connect(&t.url).unwrap();
        let (routes, refs) = net.kept_interest(&b, "structure", &sb).unwrap();
        let routes = routes.expect("routes counted");
        assert!(!routes.is_empty() && routes.iter().all(|r| c.routes.contains(r)));
        let s = Session::open_on(Box::new(net.store(&b).unwrap()), &b).unwrap();
        let st = s.line().structure(&sb).unwrap().clone();
        assert_eq!(
            refs.len(),
            st.routes.len(),
            "a structure refers to its own routes only"
        );
        let (lines, tiles) = net.entry_interest(&b, c.at.seq).unwrap();
        assert_eq!(lines, vec![b.clone()]);
        let keys: BTreeSet<i64> = c
            .tiles
            .iter()
            .filter_map(|k| ohl_store_pg::stream::tile_key(&k.tile()))
            .collect();
        assert_eq!(tiles.into_iter().collect::<BTreeSet<_>>(), keys);
        let rows: Vec<_> = net
            .stream_after(
                c.at.seq - 1,
                i32::MAX,
                std::slice::from_ref(&b),
                &[],
                &[],
                10,
            )
            .unwrap()
            .into_iter()
            .filter(|r| r.seq == c.at.seq)
            .collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].op, serde_json::to_string(&c.op).unwrap());
        // A section through it refers to it and its wire points.
        let section = s
            .line()
            .sections
            .iter()
            .find(|x| x.points.iter().any(|p| p.structure == sb))
            .unwrap();
        let (_, refs) = net.kept_interest(&b, "section", &section.id).unwrap();
        assert!(
            refs.iter()
                .any(|(k, id, set)| k == "structure" && id == &sb && *set == 0)
        );
        assert!(
            refs.iter()
                .any(|(k, id, set)| k == "structure" && id == &sb && *set > 0)
        );
    }
    // Read again: only the tiles that changed come, at the change's revision.
    let before = v.tiles.clone();
    v.joined
        .link
        .send(ClientMsg::Interest {
            routes: Vec::new(),
            tiles: held(&vtiles, &before),
        })
        .unwrap();
    let changed: Vec<TileKey> = vtiles
        .iter()
        .filter(|k| touched.contains(k))
        .copied()
        .collect();
    let mut seen = BTreeSet::new();
    v.pump(|m| {
        if let ServerMsg::Tile(p) = m {
            seen.insert(p.tile);
        }
        seen.len() == changed.len()
    });
    assert_eq!(seen, changed.iter().copied().collect::<BTreeSet<_>>());
    let moved_one = v
        .tiles
        .values()
        .flat_map(|p| p.structures.iter())
        .find(|s| s.id == sb)
        .unwrap();
    assert_eq!(moved_one.position[0], pb[0] + 1.0);
    for k in &changed {
        assert_eq!(v.tiles[k].rev, c.at);
    }
    // Nothing more comes (the other tiles were held at their revision).
    assert!(v.idle(300).iter().all(|m| !matches!(m, ServerMsg::Tile(_))));

    // The property: random moves on random models; every client receives
    // exactly the changes of its interest, in order.
    let mut rng = 0x2545_f491_u64;
    let mut next = |n: usize| {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng % n as u64) as usize
    };
    let from = position(&t);
    for k in 0..24 {
        let (code, n) = match k % 4 {
            0 => (a.clone(), next(50)),
            // Near the viewer's structure.
            1 => (b.clone(), 2 + next(3)),
            _ => (t.codes[next(t.codes.len())].clone(), next(50)),
        };
        let (id, p) = structure_of(&t, &code, n);
        let op = if k % 7 == 6 {
            Op::Undo
        } else {
            moved(&id, [p[0] + 0.5, p[1] - 0.25, p[2]])
        };
        e.edit(&code, op).unwrap();
    }
    let now = position(&t);
    for c in [&mut d, &mut v] {
        c.settle(now);
        assert!(c.heartbeat().is_ok());
    }
    let got_d: Vec<Position> = d
        .changes
        .iter()
        .map(|c| c.at)
        .filter(|p| *p > from)
        .collect();
    assert_eq!(got_d, expected(&t, from, Some(&a), &[]), "the desktop of A");
    for c in &d.changes {
        assert_eq!(c.model, a, "only A's changes to A's desktop");
    }
    // A change touching many tiles comes to a viewer as "read again"
    // (counted with the changes).
    let got_v: Vec<Position> = v.seen.iter().copied().filter(|p| *p > from).collect();
    let want_v = expected(&t, from, None, &vtiles);
    assert!(!want_v.is_empty(), "the viewer's tiles were edited");
    assert_eq!(got_v, want_v, "the viewer's tiles");
    for c in &v.changes {
        assert!(c.tiles.iter().any(|k| vtiles.contains(k)), "{c:?}");
    }
    same_as_server(&d, &t, &a, 1);

    // A wire library of B: every tile of B changed: the viewer reads its
    // tiles again.
    let library = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables/acsr-examples.ohlw"),
    )
    .unwrap();
    e.edit(
        &b,
        Op::PutLibrary {
            kind: "cables".into(),
            name: "own.ohlw".into(),
            text: library,
        },
    )
    .unwrap();
    let got = v.pump(|m| matches!(m, ServerMsg::TilesChanged { .. }));
    let ServerMsg::TilesChanged { tiles, at, .. } = got.last().unwrap() else {
        unreachable!()
    };
    assert_eq!(*at, position(&t));
    assert!(!tiles.is_empty() && tiles.iter().all(|k| vtiles.contains(k)));
    let again = tiles.clone();
    let before = v.tiles.clone();
    v.joined
        .link
        .send(ClientMsg::Interest {
            routes: Vec::new(),
            tiles: held(&vtiles, &before),
        })
        .unwrap();
    let mut seen = BTreeSet::new();
    v.pump(|m| {
        if let ServerMsg::Tile(p) = m {
            seen.insert(p.tile);
        }
        seen.len() == again.len()
    });
    for k in &again {
        assert_eq!(v.tiles[k].rev, *at);
    }

    // A lost change: the heartbeat shows it, the catch-up brings it.
    v.lose_next = true;
    let (sb2, pb2) = structure_of(&t, &b, 3);
    assert_eq!(sb2, sb);
    e.edit(&b, moved(&sb, [pb2[0] + 2.0, pb2[1], pb2[2]]))
        .unwrap();
    let lost_at = Some(position(&t));
    let gap = loop {
        match v.heartbeat() {
            Err(gap) => break gap,
            Ok(()) => continue,
        }
    };
    assert_eq!(Some(gap.sent), lost_at);
    let since = v.last.unwrap();
    v.joined.link.send(ClientMsg::Catch { since }).unwrap();
    let got = v.pump(|m| matches!(m, ServerMsg::CatchUp { .. }));
    let ServerMsg::CatchUp {
        changes,
        incomplete,
        ..
    } = got.last().unwrap()
    else {
        unreachable!()
    };
    assert!(!incomplete);
    assert_eq!(changes.len(), 1, "{changes:?}");
    assert_eq!(Some(changes[0].at), lost_at);
    assert!(v.heartbeat().is_ok());

    // The admin leaves; meanwhile Bob moves three of A's structures and
    // undoes one; the admin joins again from the position held: exactly
    // those, in order.
    let left_at = d.last.unwrap();
    let replica = d.replica.take().unwrap();
    drop(d);
    let peers = Instant::now();
    while server.peers().iter().any(|p| p.user == "admin") {
        assert!(peers.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut missed = Vec::new();
    for k in 0..3 {
        let (id, p) = structure_of(&t, &a, 10 + k);
        e.edit(&a, moved(&id, [p[0] - 1.0, p[1], p[2]])).unwrap();
        missed.push(position(&t));
    }
    e.edit(&a, Op::Undo).unwrap();
    missed.push(position(&t));
    let mut d = C::join(
        &server.invitation_for("admin"),
        "admin",
        NetJoin {
            model: Some(a.clone()),
            since: Some(left_at),
            held: true,
            ..NetJoin::default()
        },
    )
    .unwrap();
    d.replica = Some(replica);
    let got = d.pump(|m| matches!(m, ServerMsg::CatchUp { .. }));
    let ServerMsg::CatchUp { changes, .. } = got.last().unwrap() else {
        unreachable!()
    };
    let positions: Vec<Position> = changes.iter().map(|c| c.at).collect();
    assert_eq!(positions, missed, "what was missed, in order, once each");
    same_as_server(&d, &t, &a, 2);
    // Then the live stream, after it.
    let (id, p) = structure_of(&t, &a, 20);
    e.edit(&a, moved(&id, [p[0], p[1] + 1.0, p[2]])).unwrap();
    let got = d.pump(|m| matches!(m, ServerMsg::Change(_)));
    let ServerMsg::Change(live) = got.last().unwrap() else {
        unreachable!()
    };
    assert!(live.at > *missed.last().unwrap());
    same_as_server(&d, &t, &a, 3);
    let stats = server.probe(crate::network::Probe::Stats).unwrap();
    assert!(stats.contains("\"models\""), "{stats}");
    drop((d, v, e));
    server.stop();
    drop(t);
}

/// Edits by roles: a viewer's edit refused by the model's roles; a model
/// the network has not refused; the soft locks hold on a network as on a
/// project file.
#[test]
fn a_network_keeps_roles_and_locks() {
    let Some(t) = test_network("roles", 120) else {
        return;
    };
    let server = Server::start_network(local("abcd2345"), net_options(&t.url)).unwrap();
    let a = t.codes[0].clone();
    let (id, p) = structure_of(&t, &a, 1);
    let mut vic = C::join(&server.invitation, "vic", NetJoin::default()).unwrap();
    let e = vic.edit(&a, moved(&id, p)).unwrap_err();
    assert!(e.contains("unknown user vic"), "{e}");
    let mut bob = C::join(&server.invitation_for("bob"), "bob", NetJoin::default()).unwrap();
    let e = bob.edit("NOPE", moved(&id, p)).unwrap_err();
    assert!(e.contains("no line NOPE"), "{e}");
    // Carol holds the structure: Bob's move refused, the admin's passes.
    let carol = C::join(&server.invitation_for("carol"), "carol", NetJoin::default()).unwrap();
    let target = crate::lock_target("main", ohl_session::Kind::Structure, &id);
    carol
        .joined
        .link
        .send(ClientMsg::Lock {
            target: target.clone(),
        })
        .unwrap();
    bob.pump(|m| matches!(m, ServerMsg::Locks { locks } if locks.contains_key(&target)));
    let e = bob
        .edit(&a, moved(&id, [p[0] + 1.0, p[1], p[2]]))
        .unwrap_err();
    assert!(e.contains("being edited by carol"), "{e}");
    let mut admin = C::join(&server.invitation_for("admin"), "admin", NetJoin::default()).unwrap();
    admin
        .edit(&a, moved(&id, [p[0] + 1.0, p[1], p[2]]))
        .unwrap();
    drop((vic, bob, carol, admin));
    server.stop();
}
