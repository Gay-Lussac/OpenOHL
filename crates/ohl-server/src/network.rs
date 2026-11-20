// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A server on a network: every model of a country server's database
//! (PostgreSQL, `ohl-store-pg`), served by interest (ADR-0013, phase 5;
//! plan §4, §5.4, §5.5).
//!
//! **The authority** takes the network's writer's lock (a second server
//! on the same database is refused) and opens a session on every model:
//! each model's line is read once at start and changed in place by each
//! command (phase 3). Each command is **one transaction**: the session's
//! rows, then what the clients' interest needs — each changed element's
//! routes and references (`object.routes`, `object_ref`), the models and
//! tiles its log entries touched (`oplog.lines`, `oplog.tiles`), and the
//! change itself at its [`Position`] (`stream`) — committed before it is
//! broadcast. A command that fails after the session applied it rolls the
//! transaction back and reads its model again.
//!
//! **What it keeps in memory**: every model's line and its indexes; the
//! tiles of the grid at the package level, each with its objects and its
//! revision (the position of the last change touching it); per client its
//! interest; bounded caches (settings): tile packages per (tile,
//! revision, rights class — phase 7), solved section shapes and
//! resolved structures (the packagers). **Not in memory**: the log,
//! revisions and blobs (read from the database when asked), and what
//! PostgreSQL does not hold yet — point clouds, backgrounds and tiled
//! terrain stay in the project files (phase 4's import lists them); a
//! model's surfaces are in the database whole and not served yet.
//!
//! **Readers**: a pool of read-only connections; a joining desktop's copy
//! of its model is written by a reader at a snapshot taken between two
//! commands (the authority goes on meanwhile), labelled with the model's
//! log entry; the client then receives the changes after it.
//!
//! **Clients** (protocol 3): a desktop follows one model whole (its copy,
//! then its changes, which it replays: [`crate::client::replay_change`]);
//! a viewer follows tiles (the web client: their packages, read again when
//! a change touched them); every client of a network gets heartbeats and
//! may catch up from its position (`stream`) after a disconnect.
//!
//! **Rights** (phase 7, [`crate::network_rights`]): only the network's
//! users join, each with their personal invitation; every command is
//! decided by the rights' one function before it runs (its rule) and on
//! every object it changed (in the same unit of work, put back when
//! refused); everything sent is filtered by the same function — the
//! network's lines and routes, tile packages (kept per rights class),
//! changes (a client that may not see all of one reads its tiles again),
//! catch-ups, working sets and their rows, the users' presence and locks,
//! error messages. The rights are read again whenever an admin changed
//! them (their revision is looked at before each job and with each
//! heartbeat); a client whose rights changed is let go and joins again.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ohl_core::grid::{Bounds, Grid};
use ohl_session::rights::{Permission, RightsData, Target, UserRights};
use ohl_session::shared::User;
use ohl_session::working_set::{Extent, WORKING_SET_KEY, WorkingSetSpec};
use ohl_session::{Kind, Role, Session};
use ohl_store_pg::Network;
use ohl_store_pg::stream::{StreamRow, interest_of, interest_of_line, tile_key, tiles_of};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Utf8Bytes;

use crate::network_rights::{self as nr, Compiled, ModelRoles, Places};
use crate::packager::{BuildJob, Built, Caches, LibFiles, Lru, Packager, Part};
use crate::server::{
    Common, CopyWait, Event, Host, Job, JoinOptions, Joined, Listening, MAX_COPIES, NetAsk,
    Options, Server, Snapshot, log, run,
};
use crate::stream::{OWN_QUEUE, Own, To, Touch, Update, View};
use crate::{
    Change, HeldTile, ModelInfo, NetworkInfo, Op, Position, RouteInfo, RowsMsg, SavedSet,
    ServerMsg, TileKey, random_code,
};

/// Broadcasts kept for slow readers (a client further behind is dropped
/// and catches up when it joins again).
const EVENTS: usize = 8192;

/// How a server on a network is started (beyond [`Options`]).
#[derive(Debug, Clone)]
pub struct NetOptions {
    /// The database: a PostgreSQL connection string (from the command
    /// line or the environment, never from a project file).
    pub database: String,
    /// Reader connections at most (copies for joining desktops).
    pub readers: usize,
    /// Threads making tile packages.
    pub packagers: usize,
    /// Threads solving one package's sections side by side.
    pub solve_threads: usize,
    /// Tile packages kept, MB.
    pub packages_mb: usize,
    /// Solved section shapes kept, MB.
    pub shapes_mb: usize,
    /// Models whose resolved line and libraries are kept.
    pub resolved_models: usize,
    /// A heartbeat this often.
    pub heartbeat: Duration,
    /// Tiles one client may view at once.
    pub max_tiles: usize,
    /// Changes one catch-up carries at most (more: read again).
    pub catch_up_max: usize,
    /// A change touching more tiles goes to the clients viewing only tiles
    /// as "read again".
    pub large_tiles: usize,
    /// Phase 6: the context case solved ahead while the server is idle
    /// (every tile once after a start, then the tiles changes touched),
    /// one tile at a time, its shapes kept within their bound.
    pub idle_solve: bool,
    /// Idle: no client's job for this long.
    pub idle_after: Duration,
}

impl Default for NetOptions {
    fn default() -> Self {
        Self {
            database: String::new(),
            readers: 4,
            packagers: 2,
            solve_threads: 4,
            packages_mb: 128,
            shapes_mb: 256,
            resolved_models: 256,
            heartbeat: Duration::from_secs(2),
            max_tiles: 256,
            catch_up_max: 10_000,
            large_tiles: 256,
            idle_solve: true,
            idle_after: Duration::from_millis(500),
        }
    }
}

/// What the bench asks the authority (measured inside it).
#[derive(Debug, Clone)]
pub enum Probe {
    /// Counts, caches, timings (JSON).
    Stats,
    /// The tiles and objects of an area from the authority's tile index
    /// (x0, y0, x1, y1), timed.
    Area([f64; 4]),
    /// Every cache emptied (to time packages made from scratch).
    ClearCaches,
}

impl Server {
    /// Hosts the network in the database of `net.database` as its one
    /// writer (refused while another holds the writer's lock). Returns once
    /// it listens: every model read, the tile index made.
    pub fn start_network(options: Options, net: NetOptions) -> Result<Server, String> {
        if options.newcomers.is_some() {
            return Err(
                "a network admits only its users, each with their personal invitation (no \
                 newcomers): add users with ohl network rights add-user"
                    .into(),
            );
        }
        let crate::server::Identity {
            config,
            acceptor,
            keys,
            code,
            host,
            fingerprint,
        } = Server::identity(&options)?;
        let (events, _) = broadcast::channel(EVENTS);
        let (jobs, job_rx) = std_mpsc::channel();
        let common = Common::new(keys.clone(), options.newcomers, events);
        let authority = NetHost::open(&net, common, jobs.clone())?;
        let authority_thread = std::thread::Builder::new()
            .name("ohl-authority".into())
            .spawn(move || run(authority, job_rx))
            .map_err(|e| e.to_string())?;
        Server::listen(
            options,
            Listening {
                config,
                acceptor,
                keys,
                code,
                host,
                fingerprint,
                jobs,
                authority_thread,
            },
        )
    }

    /// A measurement of the authority's own work (a network's server).
    pub fn probe(&self, what: Probe) -> Result<String, String> {
        let (tx, rx) = std_mpsc::channel();
        self.jobs
            .send(Job::Probe { what, reply: tx })
            .map_err(|_| "server stopped")?;
        rx.recv_timeout(Duration::from_secs(60))
            .map_err(|_| "no answer in 60 s".to_owned())?
    }
}

/// Read-only connections to the network.
struct Readers {
    url: String,
    idle: Mutex<Vec<Network>>,
    open: AtomicUsize,
    max: usize,
}

impl Readers {
    fn take(&self) -> Result<Network, String> {
        if let Some(n) = self.idle.lock().unwrap_or_else(|e| e.into_inner()).pop() {
            return Ok(n);
        }
        if self.open.fetch_add(1, Ordering::SeqCst) >= self.max {
            self.open.fetch_sub(1, Ordering::SeqCst);
            return Err(format!(
                "the server's {} readers are busy: join again in a minute",
                self.max
            ));
        }
        Network::connect(&self.url).map_err(|e| {
            self.open.fetch_sub(1, Ordering::SeqCst);
            e.to_string()
        })
    }

    fn give(&self, n: Network) {
        self.idle.lock().unwrap_or_else(|e| e.into_inner()).push(n);
    }
}

/// A model held by the authority.
struct Model {
    code: Arc<str>,
    name: String,
    session: Session,
    /// Changes of its main branch (and libraries) in this run.
    rev: u64,
    /// Changes of its library files in this run.
    libs_rev: u64,
    libs: Option<(u64, Arc<LibFiles>)>,
    /// Phase 7: where each object is as of the last command (the state
    /// before the next).
    places: Places,
}

impl Model {
    /// Its library files a package needs, at its library revision.
    fn libs(&mut self) -> Arc<LibFiles> {
        match &self.libs {
            Some((rev, l)) if *rev == self.libs_rev => l.clone(),
            _ => {
                let mut files = BTreeMap::new();
                for kind in crate::web::SCENE_LIBRARIES {
                    files.insert(
                        kind.to_owned(),
                        self.session.library_files(kind).unwrap_or_default(),
                    );
                }
                let l = Arc::new(LibFiles { files });
                self.libs = Some((self.libs_rev, l.clone()));
                l
            }
        }
    }
}

/// An object of a model, as the tile index keeps it.
type Placed = (Arc<str>, Kind, String);

/// A tile of the grid at the package level.
#[derive(Default)]
struct TileState {
    rev: Position,
    objects: BTreeSet<Placed>,
}

/// A client of the network.
struct NetClient {
    user: String,
    own: mpsc::Sender<Own>,
    model: Option<String>,
    routes: BTreeSet<String>,
    tiles: BTreeSet<TileKey>,
    /// Tiles asked for, their packages being made.
    waiting: BTreeSet<TileKey>,
    /// Phase 6: the working set it holds as a partial replica.
    ws: Option<Ws>,
    /// Phase 7: an admin seeing as this user.
    see_as: Option<String>,
}

/// A client's working set as its replica holds it (phase 6): the extent
/// last sent, worked out again after each change that may touch it.
struct Ws {
    spec: WorkingSetSpec,
    extent: Extent,
    users: BTreeMap<String, Vec<User>>,
    /// The routes picked and those of its extent: a change touching none
    /// of them (nor an object it holds) cannot change it — unless it
    /// picked an area.
    routes: BTreeSet<String>,
    area: bool,
    /// What its replica holds (each object's hash): for the changes that
    /// do not say which objects they changed (a library file, a merge),
    /// the rows that differ.
    sent: BTreeMap<(Kind, String), u64>,
}

impl Ws {
    fn new(spec: WorkingSetSpec, extent: Extent, users: BTreeMap<String, Vec<User>>) -> Self {
        let mut ws = Ws {
            spec,
            extent,
            users,
            routes: BTreeSet::new(),
            area: false,
            sent: BTreeMap::new(),
        };
        ws.note_routes();
        ws
    }

    /// `rows` sent: what its replica holds now.
    fn note_sent(&mut self, rows: &ohl_session::working_set::Rows) {
        for p in &rows.puts {
            self.sent.insert(
                (p.kind, p.id.clone()),
                ohl_session::working_set::object_hash(&p.value),
            );
        }
        for d in &rows.deletes {
            self.sent.remove(d);
        }
    }

    fn note_routes(&mut self) {
        use ohl_session::working_set::Pick;
        self.routes = self
            .extent
            .summary
            .routes
            .iter()
            .map(|r| r.route.clone())
            .collect();
        self.area = false;
        for p in &self.spec.picks {
            match p {
                Pick::Route { route }
                | Pick::Stations { route, .. }
                | Pick::Structures { route, .. } => {
                    self.routes.insert(route.clone());
                }
                Pick::Area { .. } => self.area = true,
            }
        }
    }
}

/// A tile being made, and who waits for it.
struct Building {
    rev: Position,
    waiting: BTreeSet<u64>,
    tries: u32,
}

/// Timings kept for the bench (the last ones).
#[derive(Default)]
struct Stats {
    applied_ms: Vec<f64>,
    built_ms: Vec<f64>,
    served_from_cache_us: Vec<f64>,
    /// Phase 6: a replica's extent and rows taken on the authority, ms.
    replica_ms: Vec<f64>,
    /// Phase 6: the working sets fed after a change, ms (per change).
    rows_ms: Vec<f64>,
    /// Phase 6: tiles made by the idle solve, their seconds, when the
    /// first pass started and ended (seconds after the start).
    idle_built: usize,
    idle_s: f64,
    idle_from: Option<f64>,
    idle_done: Option<f64>,
    /// The idle solve stopped: its shapes reached their bound.
    idle_full: bool,
    solved: usize,
    shapes_cached: usize,
    start_s: f64,
    /// Phase 7: a command's rights decided (its rule and its objects), µs.
    rights_check_us: Vec<f64>,
    /// Phase 7: a tile's rights class worked out for a client, µs.
    rights_class_us: Vec<f64>,
    /// Phase 7: a package filtered for a rights class, µs.
    rights_filter_us: Vec<f64>,
    /// Phase 7: the clients that may not see a change worked out, µs.
    rights_change_us: Vec<f64>,
    /// Phase 7: the rights compiled for a user, µs.
    rights_compile_us: Vec<f64>,
}

fn keep(v: &mut Vec<f64>, x: f64) {
    if v.len() >= 10_000 {
        v.remove(0);
    }
    v.push(x);
}

/// The kinds the tile index keeps: what a package draws, and the
/// alignment points (a point moved changes the segments beside it only).
const TILED: [Kind; 4] = [
    Kind::Structure,
    Kind::Section,
    Kind::Alignment,
    Kind::AlignmentPoint,
];

/// The authority of a network.
pub(crate) struct NetHost {
    common: Common,
    net: Network,
    readers: Arc<Readers>,
    name: String,
    crs: String,
    grid: Grid,
    level: i8,
    case: Arc<ohl_weather::WeatherCase>,
    models: BTreeMap<String, Model>,
    position: Position,
    tiles: HashMap<TileKey, TileState>,
    placed: HashMap<Placed, Vec<TileKey>>,
    clients: HashMap<u64, NetClient>,
    /// Packages by (tile, revision, rights class).
    packages: Lru<(TileKey, Position, Class), Arc<Utf8Bytes>>,
    building: HashMap<TileKey, Building>,
    packager: Packager,
    copies: PathBuf,
    live: Arc<AtomicUsize>,
    options: NetOptions,
    heartbeat_due: Instant,
    stats: Stats,
    /// The idle solve (phase 6): when a client's job came last, the tiles
    /// still to make, the one being made.
    last_busy: Instant,
    born: Instant,
    idle_todo: std::collections::VecDeque<TileKey>,
    idle_inflight: Option<TileKey>,
    /// Phase 7: the rights as read (at their revision), the users' rights
    /// compiled (shared with the models' sessions for their roles), the
    /// route parts resolved (per model revision), the lines of regions.
    rights: RightsData,
    compiled: Compiled,
    parts: HashMap<nr::PartKey, (u64, nr::Members)>,
    regions: BTreeMap<String, BTreeSet<String>>,
}

/// A tile package's rights class: the objects of the tile its client may
/// not see, exactly (none: the whole package, shared by most).
type Class = Arc<[Placed]>;

/// The class of those who see everything of a tile.
fn whole() -> Class {
    Arc::from(Vec::new())
}

impl NetHost {
    fn open(
        o: &NetOptions,
        common: Common,
        jobs: std_mpsc::Sender<Job>,
    ) -> Result<NetHost, String> {
        let started = Instant::now();
        let net = Network::connect_writer(&o.database).map_err(|e| e.to_string())?;
        let grid = net.grid().map_err(|e| e.to_string())?;
        let level = net.tile_level().map_err(|e| e.to_string())?;
        if net.meta("tile_level").map_err(|e| e.to_string())?.is_none() {
            net.set_meta("tile_level", &level.to_string())
                .map_err(|e| e.to_string())?;
        }
        let celsius = net
            .meta("context_temperature")
            .map_err(|e| e.to_string())?
            .and_then(|t| t.parse::<f64>().ok())
            .filter(|t| t.is_finite())
            .unwrap_or(15.0);
        let case = Arc::new(ohl_weather::WeatherCase::still(
            &format!("{celsius} °C, no wind"),
            ohl_core::units::Temperature::from_celsius(celsius),
        ));
        let rights = net.rights().map_err(|e| e.to_string())?;
        let copies = crate::private_temp_dir("ohl-network")?;
        let caches = Arc::new(Caches::new(o.shapes_mb, o.resolved_models));
        let packager = Packager::start(o.packagers, caches, jobs, copies.join("work"))?;
        let mut host = NetHost {
            common,
            name: net
                .meta("name")
                .map_err(|e| e.to_string())?
                .unwrap_or_default(),
            crs: net
                .meta("crs")
                .map_err(|e| e.to_string())?
                .unwrap_or_default(),
            net,
            readers: Arc::new(Readers {
                url: o.database.clone(),
                idle: Mutex::new(Vec::new()),
                open: AtomicUsize::new(0),
                max: o.readers.max(1),
            }),
            grid,
            level,
            case,
            models: BTreeMap::new(),
            position: Position::default(),
            tiles: HashMap::new(),
            placed: HashMap::new(),
            clients: HashMap::new(),
            packages: Lru::new(o.packages_mb << 20, usize::MAX),
            building: HashMap::new(),
            packager,
            copies,
            live: Arc::new(AtomicUsize::new(0)),
            options: o.clone(),
            heartbeat_due: Instant::now() + o.heartbeat,
            stats: Stats::default(),
            last_busy: Instant::now(),
            born: started,
            idle_todo: std::collections::VecDeque::new(),
            idle_inflight: None,
            rights,
            compiled: Compiled::default(),
            parts: HashMap::new(),
            regions: BTreeMap::new(),
        };
        host.load()?;
        host.stats.start_s = started.elapsed().as_secs_f64();
        if o.idle_solve {
            let mut all: Vec<TileKey> = host
                .tiles
                .iter()
                .filter(|(_, t)| !t.objects.is_empty())
                .map(|(k, _)| *k)
                .collect();
            all.sort();
            host.idle_todo = all.into();
        }
        log(&format!(
            "network \"{}\": {} lines, {} tiles of level {} read in {:.1} s; {} users, {} \
             assignments (rights revision {})",
            host.name,
            host.models.len(),
            host.tiles.len(),
            host.level,
            host.stats.start_s,
            host.rights.users.len(),
            host.rights.assignments.len(),
            host.rights.rev
        ));
        Ok(host)
    }

    /// Reads every model, keeps what is missing of their interest, makes
    /// the tile index and the tiles' revisions.
    fn load(&mut self) -> Result<(), String> {
        let e = |e: ohl_store_pg::NetError| e.to_string();
        self.models.clear();
        self.tiles.clear();
        self.placed.clear();
        let mut last: BTreeMap<String, i64> = BTreeMap::new();
        for row in self.net.lines().map_err(e)? {
            let store = self.net.store(&row.id).map_err(e)?;
            let mut session = Session::open_on(Box::new(store), &row.id)
                .map_err(|x| format!("line {}: {x}", row.id))?;
            // Imported before the server kept them (schema 2): counted once.
            if self.net.interest_missing(&row.id).map_err(e)? {
                let interest = interest_of_line(&mut session);
                self.net.begin_unit().map_err(e)?;
                let written = ohl_store_pg::stream::write_interest(
                    &self.net,
                    &row.id,
                    ohl_session::versions::MAIN,
                    &interest,
                );
                match written {
                    Ok(()) => self.net.commit_unit().map_err(e)?,
                    Err(x) => {
                        let _ = self.net.roll_back_unit();
                        return Err(x.to_string());
                    }
                }
            }
            last.insert(
                row.id.clone(),
                session.last_seq().map_err(|x| x.to_string())?,
            );
            // Phase 7: its users' roles from the network's rights.
            session.set_role_source(Some(Arc::new(ModelRoles {
                line: row.id.clone(),
                rights: self.compiled.clone(),
            })));
            let places = Places::of_session(&mut session);
            self.models.insert(
                row.id.clone(),
                Model {
                    code: Arc::from(row.id.as_str()),
                    name: row.name,
                    session,
                    rev: 0,
                    libs_rev: 0,
                    libs: None,
                    places,
                },
            );
        }
        let codes: Vec<String> = self.models.keys().cloned().collect();
        for code in &codes {
            self.place_model(code);
        }
        // Revisions: the last change of the stream touching a tile, and
        // never older than its models' last log entries (a change made
        // without the server, an import).
        for (p, tiles) in &self.placed {
            let Some(seq) = last.get(&*p.0) else {
                continue;
            };
            let rev = Position { seq: *seq, step: 0 };
            for t in tiles {
                if let Some(s) = self.tiles.get_mut(t) {
                    s.rev = s.rev.max(rev);
                }
            }
        }
        for (key, seq, step) in self.net.tile_positions().map_err(e)? {
            let t = TileKey::of(&ohl_store_pg::stream::tile_of_key(key));
            if let Some(s) = self.tiles.get_mut(&t) {
                s.rev = s.rev.max(Position {
                    seq,
                    step: step.max(0) as u32,
                });
            }
        }
        let (seq, step) = self.net.position().map_err(e)?;
        self.position = Position {
            seq,
            step: step.max(0) as u32,
        };
        Ok(())
    }

    /// The package tiles of an object's bounds.
    fn tiles_of(&self, bounds: &Bounds) -> Vec<TileKey> {
        tiles_of(&self.grid, bounds, self.level)
            .iter()
            .map(TileKey::of)
            .collect()
    }

    /// Places every tiled object of model `code` in the tile index.
    fn place_model(&mut self, code: &str) {
        let Some(m) = self.models.get(code) else {
            return;
        };
        let line = m.session.line();
        let mut objects: Vec<(Kind, String)> = Vec::new();
        objects.extend(
            line.structures
                .iter()
                .map(|x| (Kind::Structure, x.id.clone())),
        );
        objects.extend(line.sections.iter().map(|x| (Kind::Section, x.id.clone())));
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
        for (kind, id) in objects {
            self.place(code, kind, &id);
        }
    }

    /// The tiles whose drawing of the alignments through point `p` of
    /// model `code` changes when `p` moves from its tiles `old`: those of
    /// the segments beside it, before (bounded by `p`'s old tiles) and
    /// after (a package draws an alignment's segments in a tile, not the
    /// whole alignment). The alignments are placed again.
    fn moved_point(&mut self, code: &str, p: &str, old: &[TileKey]) -> BTreeSet<TileKey> {
        let mut out = BTreeSet::new();
        let Some(m) = self.models.get_mut(code) else {
            return out;
        };
        let alignments: Vec<String> = m
            .session
            .object_referrers(Kind::AlignmentPoint, p)
            .into_iter()
            .filter(|(k, _)| *k == Kind::Alignment)
            .map(|(_, id)| id)
            .collect();
        let line = m.session.line();
        let at = |id: &str| -> Option<[f64; 2]> {
            line.alignment_points
                .iter()
                .find(|x| x.id == id)
                .map(|x| [x.position[0], x.position[1]])
        };
        let now = at(p);
        let mut boxes: Vec<Bounds> = Vec::new();
        for a in &alignments {
            let Some(al) = line.alignments.iter().find(|x| &x.id == a) else {
                continue;
            };
            for (k, q) in al.points.iter().enumerate() {
                if q != p {
                    continue;
                }
                let beside = [k.checked_sub(1), Some(k + 1)];
                for n in beside.into_iter().flatten() {
                    let Some(q) = al.points.get(n).and_then(|q| at(q)) else {
                        continue;
                    };
                    if let Some(now) = now {
                        boxes.push(Bounds::new(q, now));
                    }
                    for t in old {
                        boxes.push(self.grid.bounds(&t.tile()).union(&Bounds::new(q, q)));
                    }
                }
            }
        }
        for b in boxes {
            out.extend(self.tiles_of(&b));
        }
        for a in alignments {
            self.place(code, Kind::Alignment, &a);
        }
        out
    }

    /// Places object `kind`/`id` of model `code` where it is now (removed
    /// from where it was); returns the tiles of both.
    fn place(&mut self, code: &str, kind: Kind, id: &str) -> Vec<TileKey> {
        let model: Arc<str> = self
            .models
            .get(code)
            .map(|m| m.code.clone())
            .unwrap_or_else(|| Arc::from(code));
        let key: Placed = (model, kind, id.to_owned());
        let old = self.placed.remove(&key).unwrap_or_default();
        for t in &old {
            if let Some(s) = self.tiles.get_mut(t) {
                s.objects.remove(&key);
            }
        }
        let new = self
            .models
            .get(code)
            .and_then(|m| m.session.spatial_entry(kind, id))
            .map(|e| self.tiles_of(&e.bounds))
            .unwrap_or_default();
        for t in &new {
            self.tiles
                .entry(*t)
                .or_default()
                .objects
                .insert(key.clone());
        }
        let mut touched = old;
        touched.extend(new.iter().copied());
        if !new.is_empty() {
            self.placed.insert(key, new);
        }
        touched.sort();
        touched.dedup();
        touched
    }

    /// The tiles holding objects of model `code`.
    fn model_tiles(&self, code: &str) -> BTreeSet<TileKey> {
        self.placed
            .iter()
            .filter(|(p, _)| &*p.0 == code)
            .flat_map(|(_, t)| t.iter().copied())
            .collect()
    }

    fn tile_rev(&self, t: &TileKey) -> Position {
        self.tiles.get(t).map(|s| s.rev).unwrap_or_default()
    }

    /// What the network holds, for a client with `rights`: the lines it
    /// may see something of, the routes it may see, their bounds.
    fn info(&mut self, rights: &UserRights) -> NetworkInfo {
        let all = rights.sees_everything();
        let mut bounds: Option<Bounds> = None;
        let mut routes = Vec::new();
        let mut models = Vec::new();
        for (code, m) in self.models.iter_mut() {
            if !all && !rights.sees_some_of_line(code) {
                continue;
            }
            let Model {
                session,
                places,
                name,
                ..
            } = m;
            // What it may see, as drawn (an alignment with its points and
            // the structures they are tied to: its bounds show them).
            let alignments: Vec<(String, String)> = session
                .line()
                .alignments
                .iter()
                .map(|a| (a.id.clone(), a.route.clone()))
                .collect();
            let mut of_route: BTreeMap<String, Bounds> = BTreeMap::new();
            for (id, route) in alignments {
                if !all && !nr::visible(rights, code, session, places, Kind::Alignment, &id) {
                    continue;
                }
                if let Some(e) = session.spatial_entry(Kind::Alignment, &id) {
                    let b = of_route.entry(route).or_insert(e.bounds);
                    *b = b.union(&e.bounds);
                }
            }
            for r in &session.line().routes {
                if !all
                    && !nr::sees(
                        rights,
                        code,
                        Kind::Route,
                        &r.id,
                        places.get(Kind::Route, &r.id),
                    )
                {
                    continue;
                }
                let b = of_route.get(r.id.as_str()).copied();
                if let Some(b) = b {
                    bounds = Some(bounds.map_or(b, |x| x.union(&b)));
                }
                routes.push(RouteInfo {
                    id: r.id.clone(),
                    model: code.clone(),
                    name: r.name.clone(),
                    bounds: b.map(|b| [b.min[0], b.min[1], b.max[0], b.max[1]]),
                });
            }
            let mut structures = 0;
            for s in &session.line().structures {
                let place = places.get(Kind::Structure, &s.id);
                if !all && !nr::sees(rights, code, Kind::Structure, &s.id, place) {
                    continue;
                }
                structures += 1;
                if let Some(e) = session.spatial_entry(Kind::Structure, &s.id) {
                    bounds = Some(bounds.map_or(e.bounds, |x| x.union(&e.bounds)));
                }
            }
            models.push(ModelInfo {
                code: code.clone(),
                name: name.clone(),
                structures,
            });
        }
        NetworkInfo {
            name: self.name.clone(),
            crs: self.crs.clone(),
            grid_size: self.grid.size,
            tile_level: self.level,
            context_case: self.case.name.clone(),
            at: self.position,
            heartbeat_ms: self.options.heartbeat.as_millis() as u64,
            bounds: bounds.map(|b| [b.min[0], b.min[1], b.max[0], b.max[1]]),
            models,
            routes,
        }
    }

    /// The rights moved on (an admin changed them; looked at before each
    /// job and with each heartbeat): read again, every user's compiled
    /// again when needed; a client whose rights changed is let go (it
    /// holds what it may no longer see, or follows what it may now).
    fn refresh_rights(&mut self) {
        let rev = match self.net.rights_rev() {
            Ok(r) => r,
            Err(e) => {
                log(&format!("the rights' revision: {e}"));
                return;
            }
        };
        if rev == self.rights.rev {
            return;
        }
        let data = match self.net.rights() {
            Ok(d) => d,
            Err(e) => {
                log(&format!("reading the rights again: {e}"));
                return;
            }
        };
        let old: HashMap<String, Arc<UserRights>> = std::mem::take(&mut *self.compiled_mut());
        self.rights = data;
        self.regions.clear();
        let clients: Vec<(u64, String, Option<String>)> = self
            .clients
            .iter()
            .map(|(id, c)| (*id, c.user.clone(), c.see_as.clone()))
            .collect();
        let mut let_go = 0;
        for (id, user, see_as) in clients {
            let changed = [Some(user), see_as].into_iter().flatten().any(|u| {
                let new = self.rights_of(&u);
                old.get(&u).is_none_or(|o| !o.same_as(&new))
            });
            if changed {
                self.let_go(id, "your rights on this network changed: join again");
                let_go += 1;
            }
        }
        log(&format!(
            "rights read again (revision {rev}): {} users, {} assignments; {let_go} clients let go",
            self.rights.users.len(),
            self.rights.assignments.len()
        ));
    }

    fn compiled_mut(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<String, Arc<UserRights>>> {
        self.compiled
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The rights of `user`, compiled (cached until the rights change, or
    /// a model their route parts are on changes).
    fn rights_of(&mut self, user: &str) -> Arc<UserRights> {
        if let Some(r) = self
            .compiled
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(user)
        {
            return r.clone();
        }
        let started = Instant::now();
        let resolved = self.resolve_for(user);
        let r = Arc::new(self.rights.compile(user, &resolved));
        self.compiled_mut().insert(user.to_owned(), r.clone());
        keep(
            &mut self.stats.rights_compile_us,
            started.elapsed().as_secs_f64() * 1e6,
        );
        r
    }

    /// The rights client `client` acts and sees with: its user's, or, an
    /// admin seeing as another, what both may.
    fn client_rights(&mut self, client: u64) -> Option<Arc<UserRights>> {
        let (user, see_as) = self
            .clients
            .get(&client)
            .map(|c| (c.user.clone(), c.see_as.clone()))?;
        let mine = self.rights_of(&user);
        Some(match see_as {
            Some(other) => {
                let theirs = self.rights_of(&other);
                Arc::new((*mine).clone().and((*theirs).clone()))
            }
            None => mine,
        })
    }

    /// The route parts and regions of `user`'s assignments, resolved on
    /// the models as they are.
    fn resolve_for(&mut self, user: &str) -> nr::Resolved {
        let mut out = nr::Resolved {
            parts: BTreeMap::new(),
            regions: BTreeMap::new(),
        };
        let scopes: Vec<ohl_session::rights::Scope> = self
            .rights
            .assignments_of(user)
            .into_iter()
            .map(|a| a.scope.clone())
            .collect();
        for sc in scopes {
            if let (Some(l), Some(r), Some(f), Some(t)) =
                (&sc.line, &sc.route, &sc.from_structure, &sc.to_structure)
            {
                let key = (l.clone(), r.clone(), f.clone(), t.clone());
                let rev = self.models.get(l).map_or(0, |m| m.rev);
                let members = match self.parts.get(&key) {
                    Some((at, m)) if *at == rev => m.clone(),
                    _ => {
                        let m: Arc<HashSet<(Kind, String)>> = Arc::new(
                            self.models
                                .get_mut(l)
                                .and_then(|m| {
                                    m.session
                                        .pick_extent(&[
                                            ohl_session::working_set::Pick::Structures {
                                                route: r.clone(),
                                                from: f.clone(),
                                                to: t.clone(),
                                            },
                                        ])
                                        .ok()
                                })
                                .map(|e| e.wanted.into_iter().collect())
                                .unwrap_or_default(),
                        );
                        self.parts.insert(key.clone(), (rev, m.clone()));
                        m
                    }
                };
                out.parts.insert(key, members);
            }
            if let (Some(region), None) = (&sc.region, &sc.line) {
                let k = serde_json::to_string(region).unwrap_or_default();
                if !self.regions.contains_key(&k) {
                    let lines = self.region_lines(region);
                    self.regions.insert(k.clone(), lines);
                }
                out.regions.insert(k.clone(), self.regions[&k].clone());
            }
        }
        out
    }

    /// The models with an object whose place touches `region`.
    fn region_lines(&self, region: &[[f64; 2]]) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for (code, m) in &self.models {
            if m.places.iter().any(|(_, p)| {
                p.bounds
                    .is_some_and(|b| ohl_session::rights::region_touches(region, &b))
            }) {
                out.insert(code.clone());
            }
        }
        out
    }

    /// Lets client `client` go (it joins again): told why first.
    fn let_go(&mut self, client: u64, why: &str) {
        self.push(
            client,
            None,
            None,
            Some(ServerMsg::Notice { text: why.into() }),
        );
        if let Some(c) = self.clients.remove(&client) {
            for t in c.waiting {
                if let Some(b) = self.building.get_mut(&t) {
                    b.waiting.remove(&client);
                }
            }
        }
    }

    /// A refusal in the audit (with what was asked, in full: the audit is
    /// the network admins').
    fn audit_refused(&self, user: &str, what: &str, why: &str) {
        if let Err(e) = self.net.audit(user, "refused", what, why) {
            log(&format!("the audit: {e}"));
        }
    }

    /// A message for `rights`: [`HIDDEN_REFUSAL`] when it names an object
    /// the user may not see.
    fn sanitise(&self, rights: &UserRights, text: &str) -> String {
        nr::sanitise(
            rights,
            text,
            self.models.iter().map(|(c, m)| (c.as_str(), &m.places)),
        )
    }

    /// What of tile `t` `rights` may not see (its rights class).
    fn class_of(&mut self, rights: &UserRights, t: TileKey) -> Class {
        if rights.sees_everything() {
            return whole();
        }
        let started = Instant::now();
        let objects: Vec<Placed> = self
            .tiles
            .get(&t)
            .map(|s| s.objects.iter().cloned().collect())
            .unwrap_or_default();
        let mut hidden = Vec::new();
        for p in objects {
            let Some(m) = self.models.get_mut(&*p.0) else {
                continue;
            };
            let Model {
                session, places, ..
            } = m;
            if !nr::visible(rights, &p.0, session, places, p.1, &p.2) {
                hidden.push(p);
            }
        }
        keep(
            &mut self.stats.rights_class_us,
            started.elapsed().as_secs_f64() * 1e6,
        );
        Arc::from(hidden)
    }

    /// Package `full` without what class `hidden` may not see.
    fn filtered(&mut self, full: &Utf8Bytes, hidden: &[Placed]) -> Arc<Utf8Bytes> {
        let started = Instant::now();
        let gone: HashSet<(&str, Kind, &str)> = hidden
            .iter()
            .map(|(m, k, id)| (&**m, *k, id.as_str()))
            .collect();
        let text = match serde_json::from_str::<ServerMsg>(full.as_str()) {
            Ok(ServerMsg::Tile(mut p)) => {
                p.structures.retain(|x| {
                    !gone.contains(&(x.model.as_str(), Kind::Structure, x.id.as_str()))
                });
                p.wires.retain(|x| {
                    !gone.contains(&(x.model.as_str(), Kind::Section, x.section.as_str()))
                });
                p.routes.retain(|x| {
                    !gone.contains(&(x.model.as_str(), Kind::Alignment, x.alignment.as_str()))
                });
                // Names of the routes still drawn only.
                let mut names: HashSet<String> = HashSet::new();
                for r in &p.routes {
                    names.insert(r.route.clone());
                    if let Some(route) = self
                        .models
                        .get(&r.model)
                        .and_then(|m| m.session.line().routes.iter().find(|x| x.id == r.route))
                    {
                        names.insert(route.name.clone());
                    }
                }
                p.labels.retain(|l| names.contains(&l.text));
                let ids: HashSet<&str> = hidden.iter().map(|(_, _, id)| id.as_str()).collect();
                p.notes
                    .retain(|n| !ohl_session::rights::words(n).any(|w| ids.contains(w)));
                serde_json::to_string(&ServerMsg::Tile(p)).unwrap_or_default()
            }
            // Unreadable (never: the server wrote it): a notice, nothing of
            // the tile.
            _ => serde_json::to_string(&ServerMsg::Notice {
                text: "a tile could not be filtered: not sent".into(),
            })
            .unwrap_or_default(),
        };
        keep(
            &mut self.stats.rights_filter_us,
            started.elapsed().as_secs_f64() * 1e6,
        );
        Arc::new(Utf8Bytes::from(text))
    }

    /// The package of tile `t` at `rev` for class `class`, from what is
    /// kept (made from the whole package when that is kept).
    fn package_for(&mut self, t: TileKey, rev: Position, class: &Class) -> Option<Arc<Utf8Bytes>> {
        if let Some(p) = self.packages.get(&(t, rev, class.clone())) {
            return Some(p);
        }
        if class.is_empty() {
            return None;
        }
        let full = self.packages.get(&(t, rev, whole()))?;
        let text = self.filtered(&full, class);
        let bytes = text.len();
        self.packages
            .put((t, rev, class.clone()), text.clone(), bytes);
        Some(text)
    }

    /// Whether `rights` may see object `kind`/`id` of model `code` as it
    /// is now (its place `place`, or the model's own).
    fn visible(&mut self, rights: &UserRights, code: &str, kind: Kind, id: &str) -> bool {
        match self.models.get_mut(code) {
            Some(m) => {
                let Model {
                    session, places, ..
                } = m;
                nr::visible(rights, code, session, places, kind, id)
            }
            None => false,
        }
    }

    /// The rows `rights` may hold of model `code` (a working set's): what
    /// it may not see left out — puts, the working set, the users of its
    /// structures; deletes only of what it was sent (`sent`).
    fn filter_rows(
        &mut self,
        rights: &UserRights,
        code: &str,
        rows: &mut ohl_session::working_set::Rows,
        sent: &BTreeMap<(Kind, String), u64>,
    ) -> usize {
        if rights.sees_everything() {
            return 0;
        }
        let before = rows.puts.len();
        let puts = std::mem::take(&mut rows.puts);
        rows.puts = puts
            .into_iter()
            .filter(|p| self.visible(rights, code, p.kind, &p.id))
            .collect();
        let left_out = before - rows.puts.len();
        rows.deletes.retain(|k| sent.contains_key(k));
        if let Some(w) = rows.wanted.take() {
            rows.wanted = Some(
                w.into_iter()
                    .filter(|(k, id)| self.visible(rights, code, *k, id))
                    .collect(),
            );
        }
        if let Some(u) = rows.users.take() {
            rows.users = Some(self.filter_users(rights, code, u));
        }
        left_out
    }

    /// The users of a working set's structures `rights` may see (each
    /// with the routes it may see).
    fn filter_users(
        &mut self,
        rights: &UserRights,
        code: &str,
        users: BTreeMap<String, Vec<User>>,
    ) -> BTreeMap<String, Vec<User>> {
        if rights.sees_everything() {
            return users;
        }
        let mut out = BTreeMap::new();
        for (structure, list) in users {
            let mut kept = Vec::new();
            for mut u in list {
                if !self.visible(rights, code, u.kind, &u.id) {
                    continue;
                }
                let routes = std::mem::take(&mut u.routes);
                u.routes = routes
                    .into_iter()
                    .filter(|r| self.visible(rights, code, Kind::Route, r))
                    .collect();
                kept.push(u);
            }
            if !kept.is_empty() {
                out.insert(structure, kept);
            }
        }
        out
    }

    /// Queues something for client `client` alone (after the broadcasts
    /// sent so far); a client whose queue is full is let go (it catches
    /// up when it joins again).
    fn push(
        &mut self,
        client: u64,
        update: Option<Update>,
        text: Option<Arc<Utf8Bytes>>,
        msg: Option<ServerMsg>,
    ) {
        let after = self.common.sent;
        let full = match self.clients.get(&client) {
            Some(c) => c
                .own
                .try_send(Own {
                    after,
                    update,
                    text,
                    msg,
                })
                .is_err(),
            None => false,
        };
        if full {
            log(&format!("client {client} too far behind: let go"));
            self.clients.remove(&client);
        }
    }

    fn join(
        &mut self,
        client: u64,
        user: &str,
        code: &str,
        o: JoinOptions,
    ) -> Result<Joined, String> {
        let personal = self.common.check_code(user, code)?;
        if o.copy && !o.web && o.model.is_none() && o.working_set.is_none() {
            return Err(format!(
                "this server holds a network of {} lines: join it with a working set (File › \
                 Join a network in OHL-Design), or follow one line whole (the command line)",
                self.models.len()
            ));
        }
        if o.model.is_some() && o.working_set.is_some() {
            return Err("follow one line whole or a working set, not both".into());
        }
        // Phase 7: the network's users only, each with their personal
        // invitation (default deny: the general one admits nobody).
        self.refresh_rights();
        if !personal || !self.rights.active(user) {
            let why = if personal {
                format!(
                    "{user} is not a user of this network (or is disabled): ask a network admin"
                )
            } else {
                format!(
                    "{user}: a network admits its users only, each with their personal \
                     invitation (ask a network admin)"
                )
            };
            self.audit_refused(user, "join", &why);
            return Err(why);
        }
        // "See as user": who manages rights, as another user (read only).
        if let Some(other) = &o.see_as {
            let mine = self.rights_of(user);
            let why = if !mine.manages_anything() {
                Some(format!(
                    "{user} manages no rights: may not see as another user"
                ))
            } else if !self.rights.users.contains_key(other) {
                Some(format!("no user {other}"))
            } else {
                None
            };
            if let Some(why) = why {
                self.audit_refused(user, &format!("see as {other}"), &why);
                return Err(why);
            }
            if let Err(e) =
                self.net
                    .audit(user, "see_as", &format!("user:{other}"), "joined seeing as")
            {
                log(&format!("the audit: {e}"));
            }
        }
        let mine = self.rights_of(user);
        let rights: Arc<UserRights> = match &o.see_as {
            Some(other) => {
                let theirs = self.rights_of(other);
                Arc::new((*mine).clone().and((*theirs).clone()))
            }
            None => mine,
        };
        let ws_model = o.working_set.as_ref().map(|w| w.model.clone());
        let followed = o.model.as_ref().or(ws_model.as_ref());
        if let Some(m) = followed
            && !self.models.contains_key(m)
        {
            return Err(format!("the network has no line {m}"));
        }
        // A model followed whole: seen whole.
        if let Some(m) = &o.model
            && !rights.sees_whole_line(m)
        {
            let why = format!(
                "{user} may not see all of line {m}: follow a working set of what you may see"
            );
            self.audit_refused(user, &format!("follow line {m}"), &why);
            return Err(why);
        }
        let role = followed
            .and_then(|m| rights.model_role(m))
            .or_else(|| {
                self.models
                    .keys()
                    .filter_map(|m| rights.model_role(m))
                    .max()
            })
            .unwrap_or(Role::Viewer);
        // A working set: worked out (refused when its picks do not hold, or
        // it reaches what the user may not see).
        let ws = match &o.working_set {
            Some(spec) => {
                let started = Instant::now();
                let m = self.models.get_mut(&spec.model).expect("checked");
                let extent = m
                    .session
                    .pick_extent(&spec.picks)
                    .map_err(|e| e.to_string());
                let extent = match extent {
                    Ok(e) => e,
                    Err(e) => return Err(self.sanitise(&rights, &e)),
                };
                if let Err(why) = self.check_extent(&rights, &spec.model, &extent) {
                    self.audit_refused(user, &format!("working set on line {}", spec.model), &why);
                    return Err(why);
                }
                let m = self.models.get_mut(&spec.model).expect("checked");
                let users = m.session.extent_users(&extent);
                let sent = m.session.hashes_of(&extent.keep);
                let users = self.filter_users(&rights, &spec.model, users);
                keep(
                    &mut self.stats.replica_ms,
                    started.elapsed().as_secs_f64() * 1e3,
                );
                let mut ws = Ws::new(spec.clone(), extent, users);
                ws.sent = sent;
                Some(ws)
            }
            None => None,
        };
        let restricted = !rights.sees_everything();
        let events = self.common.events.subscribe();
        let index = self.common.sent;
        let (own_tx, own_rx) = mpsc::channel(OWN_QUEUE);
        self.clients.insert(
            client,
            NetClient {
                user: user.to_owned(),
                own: own_tx,
                model: o.model.clone(),
                routes: BTreeSet::new(),
                tiles: BTreeSet::new(),
                waiting: BTreeSet::new(),
                ws: None,
                see_as: o.see_as.clone(),
            },
        );
        // In the stream's order: what it follows, what the network holds,
        // what it missed, the tiles it views.
        if let Some(m) = &o.model {
            self.push(client, Some(Update::Models([m.clone()].into())), None, None);
        }
        let info = self.info(&rights);
        self.push(client, None, None, Some(ServerMsg::Network(Box::new(info))));
        let (copy, seq) = match (&o.model, &ws) {
            (Some(m), _) if o.copy && !o.web => {
                let seq = self.models[m]
                    .session
                    .last_seq()
                    .map_err(|e| e.to_string())?;
                (Some(self.copy_of(m)?), seq)
            }
            (None, Some(ws)) if o.copy && !o.web => (
                Some(self.replica_copy(user, &rights, ws)?),
                self.position.seq,
            ),
            _ => (None, self.position.seq),
        };
        if let Some(c) = self.clients.get_mut(&client) {
            c.ws = ws;
        }
        if let Some(since) = o.since {
            self.catch_up(client, since, false)?;
        }
        if !o.routes.is_empty() || !o.tiles.is_empty() {
            self.interest(client, o.routes, o.tiles)?;
        }
        self.common.joined(user, role);
        let mut welcome = ServerMsg::Welcome {
            project: if self.name.is_empty() {
                "network".into()
            } else {
                self.name.clone()
            },
            role,
            seq,
            size: 0,
            peers: self.common.peers.values().cloned().collect(),
            locks: self.common.locks.clone(),
        };
        if restricted && let ServerMsg::Welcome { peers, locks, .. } = &mut welcome {
            if let Some(ServerMsg::Peers { peers: p }) = crate::server::redact(&ServerMsg::Peers {
                peers: std::mem::take(peers),
            }) {
                *peers = p;
            }
            locks.clear();
        }
        Ok(Joined {
            events,
            own: Some(own_rx),
            index,
            copy,
            welcome,
            view: View {
                restricted,
                ..View::default()
            },
        })
    }

    /// Refuses a working set reaching what `rights` may not see (its
    /// objects and what its replica holds beside them); the refusal names
    /// none of them.
    fn check_extent(
        &mut self,
        rights: &UserRights,
        code: &str,
        extent: &Extent,
    ) -> Result<(), String> {
        if rights.sees_everything() {
            return Ok(());
        }
        let mut hidden = 0usize;
        for (k, id) in extent.keep.iter().chain(&extent.wanted) {
            if !self.visible(rights, code, *k, id) {
                hidden += 1;
            }
        }
        if hidden > 0 {
            return Err(format!(
                "{}: this working set reaches objects you have no access to — pick less (whole \
                 tension sections of what you may see) or ask a network admin",
                rights.user()
            ));
        }
        Ok(())
    }

    /// A copy of model `code` for a joining client, written by a reader at
    /// a snapshot taken now (between two commands).
    fn copy_of(&mut self, code: &str) -> Result<CopyWait, String> {
        if self.live.load(Ordering::SeqCst) >= MAX_COPIES {
            return Err(format!(
                "the server is sending lines to {MAX_COPIES} others: join again in a minute"
            ));
        }
        let reader = self.readers.take()?;
        if let Err(e) = reader.begin_snapshot() {
            self.readers.give(reader);
            return Err(e.to_string());
        }
        self.live.fetch_add(1, Ordering::SeqCst);
        let path = self
            .copies
            .join(format!("{code}-{}-{}.ohl", self.position, random_code(8)));
        let (tx, rx) = oneshot::channel();
        let (readers, live, code) = (self.readers.clone(), self.live.clone(), code.to_owned());
        std::thread::Builder::new()
            .name("ohl-copy".into())
            .spawn(move || {
                let made = (|| -> Result<u64, String> {
                    use ohl_session::store::Store;
                    let store = reader.store(&code).map_err(|e| e.to_string())?;
                    store.copy_for_others(&path).map_err(|e| e.to_string())?;
                    std::fs::metadata(&path)
                        .map(|m| m.len())
                        .map_err(|e| e.to_string())
                })();
                let ended = reader.end_snapshot();
                if ended.is_ok() {
                    readers.give(reader);
                } else {
                    readers.open.fetch_sub(1, Ordering::SeqCst);
                }
                let snapshot = made.map(|size| {
                    Arc::new(Snapshot {
                        path: path.clone(),
                        size,
                        live: live.clone(),
                    })
                });
                if snapshot.is_err() {
                    let _ = std::fs::remove_file(&path);
                    live.fetch_sub(1, Ordering::SeqCst);
                }
                let _ = tx.send(snapshot);
            })
            .map_err(|e| {
                self.live.fetch_sub(1, Ordering::SeqCst);
                e.to_string()
            })?;
        Ok(CopyWait::Making(rx))
    }

    /// A partial replica of working set `ws` for joining user `user`: its
    /// rows taken now (between two commands; the extent's objects only),
    /// written into a project file by a thread of its own.
    fn replica_copy(
        &mut self,
        user: &str,
        rights: &UserRights,
        ws: &Ws,
    ) -> Result<CopyWait, String> {
        if self.live.load(Ordering::SeqCst) >= MAX_COPIES {
            return Err(format!(
                "the server is sending lines to {MAX_COPIES} others: join again in a minute"
            ));
        }
        let started = Instant::now();
        let code = ws.spec.model.clone();
        let m = self
            .models
            .get_mut(&code)
            .ok_or_else(|| format!("the network has no line {code}"))?;
        let mut replica = m.session.replica_of(&ws.extent);
        replica.users = self.filter_users(rights, &code, std::mem::take(&mut replica.users));
        keep(
            &mut self.stats.replica_ms,
            started.elapsed().as_secs_f64() * 1e3,
        );
        let meta = vec![
            ("network.name".to_owned(), self.name.clone()),
            ("network.line".to_owned(), code.clone()),
            (
                "network.position".to_owned(),
                serde_json::to_string(&self.position).map_err(|e| e.to_string())?,
            ),
            (
                WORKING_SET_KEY.to_owned(),
                serde_json::to_string(&ws.spec).map_err(|e| e.to_string())?,
            ),
        ];
        self.live.fetch_add(1, Ordering::SeqCst);
        let path = self.copies.join(format!(
            "{code}-ws-{}-{}.ohl",
            self.position,
            random_code(8)
        ));
        let (tx, rx) = oneshot::channel();
        let (live, user) = (self.live.clone(), user.to_owned());
        std::thread::Builder::new()
            .name("ohl-replica".into())
            .spawn(move || {
                let made = Session::write_replica(replica, &path, &user, &meta)
                    .map_err(|e| e.to_string())
                    .and_then(|_| {
                        std::fs::metadata(&path)
                            .map(|m| m.len())
                            .map_err(|e| e.to_string())
                    });
                let snapshot = made.map(|size| {
                    Arc::new(Snapshot {
                        path: path.clone(),
                        size,
                        live: live.clone(),
                    })
                });
                if snapshot.is_err() {
                    let _ = std::fs::remove_file(&path);
                    live.fetch_sub(1, Ordering::SeqCst);
                }
                let _ = tx.send(snapshot);
            })
            .map_err(|e| {
                self.live.fetch_sub(1, Ordering::SeqCst);
                e.to_string()
            })?;
        Ok(CopyWait::Making(rx))
    }

    /// The working sets on model `code` after a change of `user` (`what`)
    /// at `at` that changed `changed` (`all`: anything of the model may
    /// have changed) and touched `routes`: each worked out again where the
    /// change may touch it, and its client given the rows that change —
    /// on its own queue before the change's broadcast, so a client waiting
    /// for its answer has them first.
    #[allow(clippy::too_many_arguments)]
    fn feed_working_sets(
        &mut self,
        code: &str,
        user: &str,
        what: &str,
        at: Position,
        changed: &BTreeSet<(Kind, String)>,
        all: bool,
        routes: &[String],
    ) {
        // `all`: the change does not say which objects it changed (a
        // merge, a library file): the rows that differ from what the
        // replica holds.
        let started = Instant::now();
        let ids: Vec<u64> = self
            .clients
            .iter()
            .filter(|(_, c)| c.ws.as_ref().is_some_and(|w| w.spec.model == code))
            .map(|(id, _)| *id)
            .collect();
        if ids.is_empty() {
            return;
        }
        let mut out = Vec::new();
        for id in ids {
            // Taken out while its rows are worked out (put back below).
            let Some(mut ws) = self.clients.get_mut(&id).and_then(|c| c.ws.take()) else {
                continue;
            };
            let touches = all
                || ws.area
                || routes.iter().any(|r| ws.routes.contains(r))
                || changed.iter().any(|k| ws.extent.keep.contains(k));
            if !touches || !self.models.contains_key(code) {
                if let Some(c) = self.clients.get_mut(&id) {
                    c.ws = Some(ws);
                }
                continue;
            }
            let (new, mut rows) = {
                let m = self.models.get_mut(code).expect("checked");
                let new = m.session.extent_of(&ws.spec.picks);
                let rows = if all {
                    m.session.rows_against(&ws.sent, &new)
                } else {
                    m.session.rows_for(&ws.extent, &ws.users, &new, changed)
                };
                (new, rows)
            };
            // Phase 7: only what its user may see (what came into its
            // extent that the user may not see is left out of the replica).
            if let Some(rights) = self.client_rights(id) {
                let left = self.filter_rows(&rights, code, &mut rows, &ws.sent);
                if left > 0 {
                    log(&format!(
                        "client {id}: {left} objects left out of its working set's rows (no access)"
                    ));
                }
            }
            if rows.users.as_ref() == Some(&ws.users) {
                rows.users = None;
            }
            if all {
                // Sent always by `rows_against`: only when they differ.
                if rows.wanted.as_ref().is_some_and(|w| {
                    w.len() == ws.extent.wanted.len()
                        && w.iter().all(|k| ws.extent.wanted.contains(k))
                }) {
                    rows.wanted = None;
                }
                if rows.users.as_ref() == Some(&ws.users) {
                    rows.users = None;
                }
            }
            if let Some(u) = &rows.users {
                ws.users = u.clone();
            }
            ws.note_sent(&rows);
            ws.extent = new;
            ws.note_routes();
            if let Some(c) = self.clients.get_mut(&id) {
                c.ws = Some(ws);
            }
            if !rows.is_empty() {
                out.push((id, rows));
            }
        }
        for (id, rows) in out {
            self.push(
                id,
                Some(Update::Seen(at)),
                None,
                Some(ServerMsg::Rows(Box::new(RowsMsg {
                    model: code.to_owned(),
                    at,
                    user: Some(user.to_owned()),
                    what: Some(what.to_owned()),
                    rows,
                    spec: None,
                }))),
            );
        }
        keep(
            &mut self.stats.rows_ms,
            started.elapsed().as_secs_f64() * 1e3,
        );
    }

    /// A question of client `client` about working sets (phase 6); its
    /// answer on the client's own queue.
    fn net_ask(&mut self, client: u64, user: &str, ask: NetAsk) -> Result<(), String> {
        // A client let go (its rights changed) asks no more.
        let rights = self
            .client_rights(client)
            .ok_or("not joined (let go): join again")?;
        let msg = match ask {
            NetAsk::Pick { id, model, picks } => {
                let result = match self.models.get_mut(&model) {
                    Some(m) => m.session.pick_extent(&picks).map_err(|e| e.to_string()),
                    None => Err(format!("the network has no line {model}")),
                };
                let result = match result {
                    Ok(e) => self.check_extent(&rights, &model, &e).map(|()| e.summary),
                    Err(e) => Err(e),
                };
                let result = result.map_err(|e| self.sanitise(&rights, &e));
                ServerMsg::Extent { id, model, result }
            }
            NetAsk::RouteDetail { model, route } => {
                let result = if self.visible(&rights, &model, Kind::Route, &route) {
                    match self.models.get_mut(&model) {
                        Some(m) => m.session.route_detail(&route).map_err(|e| e.to_string()),
                        None => Err(format!("the network has no line {model}")),
                    }
                } else {
                    Err(format!("no route {route} you may see on line {model}"))
                };
                // A route's structures and alignments: those it may see.
                let result = result.map(|mut d| {
                    d.structures
                        .retain(|x| self.visible(&rights, &model, Kind::Structure, &x.id));
                    d.alignments
                        .retain(|x| self.visible(&rights, &model, Kind::Alignment, &x.id));
                    d
                });
                let result = result.map_err(|e| self.sanitise(&rights, &e));
                ServerMsg::RouteDetail { model, result }
            }
            NetAsk::Set { spec } => {
                let started = Instant::now();
                {
                    let c = self.clients.get(&client).ok_or("not joined")?;
                    let ws =
                        c.ws.as_ref()
                            .ok_or("join with a working set first: this connection holds none")?;
                    if spec.model != ws.spec.model {
                        return Err(format!(
                            "a working set is on one line: this one is on {}; open {} on its own",
                            ws.spec.model, spec.model
                        ));
                    }
                }
                let new = {
                    let m = self
                        .models
                        .get_mut(&spec.model)
                        .ok_or_else(|| format!("the network has no line {}", spec.model))?;
                    m.session
                        .pick_extent(&spec.picks)
                        .map_err(|e| e.to_string())
                };
                let new = new.map_err(|e| self.sanitise(&rights, &e))?;
                if let Err(why) = self.check_extent(&rights, &spec.model, &new) {
                    self.audit_refused(user, &format!("working set on line {}", spec.model), &why);
                    return Err(why);
                }
                let (old_extent, old_users, sent) = {
                    let ws = self.clients[&client].ws.as_ref().expect("checked");
                    (ws.extent.clone(), ws.users.clone(), ws.sent.clone())
                };
                let mut rows = self
                    .models
                    .get_mut(&spec.model)
                    .expect("checked")
                    .session
                    .rows_for(&old_extent, &old_users, &new, &BTreeSet::new());
                self.filter_rows(&rights, &spec.model, &mut rows, &sent);
                if rows.users.as_ref() == Some(&old_users) {
                    rows.users = None;
                }
                let ws = self
                    .clients
                    .get_mut(&client)
                    .and_then(|c| c.ws.as_mut())
                    .ok_or("not joined")?;
                if let Some(u) = &rows.users {
                    ws.users = u.clone();
                }
                ws.note_sent(&rows);
                ws.extent = new;
                ws.spec = spec.clone();
                ws.note_routes();
                keep(
                    &mut self.stats.replica_ms,
                    started.elapsed().as_secs_f64() * 1e3,
                );
                ServerMsg::Rows(Box::new(RowsMsg {
                    model: spec.model.clone(),
                    at: self.position,
                    user: None,
                    what: Some(format!(
                        "the working set: {}",
                        spec.picks
                            .iter()
                            .map(|p| p.describe())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                    rows,
                    spec: Some(spec),
                }))
            }
            NetAsk::Hold { objects } => {
                let started = Instant::now();
                let (model, extent) = {
                    let c = self.clients.get(&client).ok_or("not joined")?;
                    let ws =
                        c.ws.as_ref()
                            .ok_or("join with a working set first: this connection holds none")?;
                    (ws.spec.model.clone(), ws.extent.clone())
                };
                let held: BTreeMap<(Kind, String), u64> =
                    objects.into_iter().map(|(k, id, h)| ((k, id), h)).collect();
                let mut rows = self
                    .models
                    .get_mut(&model)
                    .ok_or_else(|| format!("the network has no line {model}"))?
                    .session
                    .rows_against(&held, &extent);
                // What it holds, as it says: deletes only of those.
                self.filter_rows(&rights, &model, &mut rows, &held);
                let ws = self
                    .clients
                    .get_mut(&client)
                    .and_then(|c| c.ws.as_mut())
                    .ok_or("not joined")?;
                ws.sent = held;
                ws.note_sent(&rows);
                keep(
                    &mut self.stats.replica_ms,
                    started.elapsed().as_secs_f64() * 1e3,
                );
                ServerMsg::Rows(Box::new(RowsMsg {
                    model: ws.spec.model.clone(),
                    at: self.position,
                    user: None,
                    what: None,
                    rows,
                    spec: Some(ws.spec.clone()),
                }))
            }
            NetAsk::Save { spec } => {
                let extent = {
                    let m = self
                        .models
                        .get_mut(&spec.model)
                        .ok_or_else(|| format!("the network has no line {}", spec.model))?;
                    m.session
                        .pick_extent(&spec.picks)
                        .map_err(|e| e.to_string())
                };
                let extent = extent.map_err(|e| self.sanitise(&rights, &e))?;
                self.check_extent(&rights, &spec.model, &extent)?;
                let routes: Vec<String> = extent
                    .summary
                    .routes
                    .iter()
                    .map(|r| r.route.clone())
                    .collect();
                self.net
                    .save_working_set(user, &spec, &routes)
                    .map_err(|e| e.to_string())?;
                self.saved_sets(user)?
            }
            NetAsk::Delete { name } => {
                self.net
                    .delete_working_set(user, &name)
                    .map_err(|e| e.to_string())?;
                self.saved_sets(user)?
            }
            NetAsk::List => self.saved_sets(user)?,
        };
        let at = self.position;
        let seen = matches!(msg, ServerMsg::Rows(_)).then_some(Update::Seen(at));
        self.push(client, seen, None, Some(msg));
        Ok(())
    }

    fn saved_sets(&self, user: &str) -> Result<ServerMsg, String> {
        Ok(ServerMsg::WorkingSets {
            sets: self
                .net
                .working_sets(user)
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|w| SavedSet {
                    spec: w.spec,
                    routes: w.routes,
                    time: w.time,
                })
                .collect(),
        })
    }

    fn request(
        &mut self,
        client: u64,
        user: &str,
        id: u64,
        op: Op,
        line: String,
        model: Option<String>,
    ) -> Result<(), String> {
        // A client let go (its rights changed) asks no more.
        let Some(c) = self.clients.get(&client) else {
            return Err("not joined (let go): join again".into());
        };
        if c.see_as.is_some() {
            return Err(
                "seeing as another user: nothing is changed so (join as yourself to edit)".into(),
            );
        }
        let model = model
            .or_else(|| c.model.clone())
            .ok_or("name the line (model) of the network the edit is on")?;
        let rights = self.rights_of(user);
        let m = self
            .models
            .get(&model)
            .ok_or_else(|| format!("the network has no line {model}"))?;
        // Soft locks: others' objects are refused (admins pass).
        let admin = m.session.role(user).map_err(|e| e.to_string())? == Some(Role::Admin);
        if !admin && self.common.locks.values().any(|u| u != user) {
            let targets = crate::server::targets(&m.session, user, &op, &line)?;
            self.common.locked(user, &line, targets)?;
        }
        self.apply(Some((client, id)), user, &model, &line, op)
            .map_err(|e| self.sanitise(&rights, &e))
    }

    /// Applies `op` of `user` on project line `line` of model `code` as one
    /// transaction with what the stream needs, then broadcasts it. Phase
    /// 7: decided by the rights — the operation's rule before it runs,
    /// every object it changed after (in the same unit: refused, nothing
    /// stays).
    fn apply(
        &mut self,
        answers: Option<(u64, u64)>,
        user: &str,
        code: &str,
        line: &str,
        op: Op,
    ) -> Result<(), String> {
        let started = Instant::now();
        let before = self.position;
        if !self.models.contains_key(code) {
            return Err(format!("the network has no line {code}"));
        }
        let rights = self.rights_of(user);
        let checking = Instant::now();
        if let Err(why) = nr::check_need(&rights, code, &op, &self.models[code].session) {
            self.audit_refused(user, &format!("{code}/{line}: {}", op.describe()), &why);
            return Err(format!("refused: {why}"));
        }
        let mut check_us = checking.elapsed().as_secs_f64() * 1e6;
        self.net.begin_unit().map_err(|e| e.to_string())?;
        let m = self.models.get_mut(code).expect("checked");
        let Model {
            session, places, ..
        } = m;
        let mut refusal: Option<String> = None;
        let mut objects_us = 0.0;
        let executed = crate::execute_checked(session, user, line, &op, |s, a| {
            let t = Instant::now();
            let decided = nr::check_objects(&rights, code, s, places, &a.objects);
            objects_us = t.elapsed().as_secs_f64() * 1e6;
            decided.map_err(|why| {
                refusal = Some(why.clone());
                ohl_session::SessionError::Refused(why)
            })
        });
        let applied = match executed {
            Ok(a) => a,
            Err(e) => {
                let _ = self.net.roll_back_unit();
                if let Some(why) = refusal {
                    self.audit_refused(user, &format!("{code}/{line}: {}", op.describe()), &why);
                }
                return Err(e.to_string());
            }
        };
        check_us += objects_us;
        keep(&mut self.stats.rights_check_us, check_us);
        let written = self.written(user, code, line, &op, applied.as_ref(), before);
        let (change, touch) = match written.and_then(|w| {
            self.net.commit_unit().map_err(|e| e.to_string())?;
            Ok(w)
        }) {
            Ok(w) => w,
            Err(e) => {
                let _ = self.net.roll_back_unit();
                log(&format!(
                    "{} on line {code}: not kept ({e}); the line is read again",
                    op.describe()
                ));
                self.reload(code);
                return Err(format!("{}: not kept ({e})", op.describe()));
            }
        };
        self.position = change.at;
        // The partial replicas of this model: their rows first.
        // A library file (applied, not given back), a merge or a line
        // rolled back (main changed whatever the line acted on) say not
        // which objects they changed: the rows that differ go.
        let main = ohl_session::versions::MAIN;
        let whole_op = matches!(
            op,
            Op::BringIn { .. } | Op::Merge { .. } | Op::Revert { .. }
        ) && line == main
            || matches!(
                op,
                Op::MergeReview { .. }
                    | Op::RollBackLine { .. }
                    | Op::RenameLine { .. }
                    | Op::DeleteLine { .. }
            );
        let library_op =
            line == main && matches!(op, Op::PutLibrary { .. } | Op::DeleteLibrary { .. });
        let on_main = applied.as_ref().filter(|_| line == main);
        if on_main.is_some() || whole_op || library_op {
            let changed: BTreeSet<(Kind, String)> = on_main
                .map(|a| a.objects.iter().cloned().collect())
                .unwrap_or_default();
            let all = whole_op || library_op || on_main.is_some_and(|a| a.whole);
            let what = op.describe();
            self.feed_working_sets(code, user, &what, change.at, &changed, all, &change.routes);
        }
        if let Op::SetSetting { key, value } = &op {
            log(&format!(
                "{user} {} setting {key} on {code}/{line}",
                if value.is_some() {
                    "changed"
                } else {
                    "removed"
                }
            ));
        }
        let role_changed = matches!(&op, Op::SetUser { .. });
        self.common.send(Event::to(
            answers,
            ServerMsg::Change(Box::new(change)),
            To::Touching(Arc::new(touch)),
        ));
        if let (true, Op::SetUser { name, role }) = (role_changed, &op)
            && let Some(p) = self.common.peers.get_mut(name)
        {
            p.role = *role;
            self.common.broadcast_peers();
        }
        keep(
            &mut self.stats.applied_ms,
            started.elapsed().as_secs_f64() * 1e3,
        );
        Ok(())
    }

    /// What an applied command touched, written with it (inside its
    /// transaction): the routes and references of what changed in the
    /// main branch, the tile index and revisions, the log's interest, the
    /// stream row. Returns the change and its touch.
    fn written(
        &mut self,
        user: &str,
        code: &str,
        line: &str,
        op: &Op,
        applied: Option<&ohl_session::Applied>,
        before: Position,
    ) -> Result<(Change, Touch), String> {
        let e = |e: ohl_store_pg::NetError| e.to_string();
        let main = ohl_session::versions::MAIN;
        let m = self.models.get_mut(code).expect("checked");
        let last = m.session.last_seq().map_err(|x| x.to_string())?;
        let logged = last > before.seq;
        let at = if logged {
            Position { seq: last, step: 0 }
        } else {
            Position {
                seq: before.seq,
                step: before.step + 1,
            }
        };
        let first = logged.then_some(before.seq + 1);
        // What changed of the main branch.
        let library = matches!(op,
            Op::PutLibrary { kind, .. } | Op::DeleteLibrary { kind, .. }
                if crate::web::SCENE_LIBRARIES.contains(&kind.as_str()));
        let whole = matches!(
            op,
            Op::BringIn { .. }
                | Op::Merge { .. }
                | Op::Revert { .. }
                | Op::MergeReview { .. }
                | Op::RollBackLine { .. }
                | Op::RenameLine { .. }
                | Op::DeleteLine { .. }
        ) && (line == main
            || matches!(
                op,
                Op::MergeReview { .. }
                    | Op::RollBackLine { .. }
                    | Op::RenameLine { .. }
                    | Op::DeleteLine { .. }
            ));
        let mut objects: BTreeSet<(Kind, String)> = BTreeSet::new();
        if line == main
            && let Some(a) = applied
        {
            objects.extend(a.objects.iter().cloned());
            objects.extend(a.sections.iter().map(|s| (Kind::Section, s.clone())));
        }
        let main_changed = whole || library || !objects.is_empty();
        let mut touched: BTreeSet<TileKey> = BTreeSet::new();
        let mut routes: BTreeSet<String> = BTreeSet::new();
        if whole {
            let interest = interest_of_line(&mut m.session);
            for (_, _, r) in &interest.routes {
                routes.extend(r.iter().cloned());
            }
            ohl_store_pg::stream::write_interest(&self.net, code, main, &interest).map_err(e)?;
            touched.extend(self.model_tiles(code));
            let stale: Vec<Placed> = self
                .placed
                .keys()
                .filter(|p| &*p.0 == code)
                .cloned()
                .collect();
            for p in stale {
                if let Some(old) = self.placed.remove(&p) {
                    for t in old {
                        if let Some(s) = self.tiles.get_mut(&t) {
                            s.objects.remove(&p);
                        }
                    }
                }
            }
            self.place_model(code);
            touched.extend(self.model_tiles(code));
        } else if !objects.is_empty() {
            let list: Vec<(Kind, String)> = objects.iter().cloned().collect();
            let interest = interest_of(&mut m.session, &list);
            for (_, _, r) in &interest.routes {
                routes.extend(r.iter().cloned());
            }
            ohl_store_pg::stream::write_interest(&self.net, code, main, &interest).map_err(e)?;
            for (kind, id) in &list {
                if *kind == Kind::AlignmentPoint {
                    // Its alignments are drawn otherwise beside it.
                    let old = self
                        .models
                        .get(code)
                        .and_then(|m| self.placed.get(&(m.code.clone(), *kind, id.clone())))
                        .cloned()
                        .unwrap_or_default();
                    touched.extend(self.place(code, *kind, id));
                    touched.extend(self.moved_point(code, id, &old));
                } else if TILED.contains(kind) {
                    touched.extend(self.place(code, *kind, id));
                }
            }
        }
        if line != main
            && let Some(a) = applied
        {
            // A branch's change: the routes its objects are on in main.
            let m = self.models.get_mut(code).expect("checked");
            for r in m.session.object_routes(&a.objects) {
                routes.extend(r);
            }
        }
        if library {
            touched.extend(self.model_tiles(code));
        }
        let m = self.models.get_mut(code).expect("checked");
        if main_changed {
            m.rev += 1;
        }
        if library {
            m.libs_rev += 1;
        }
        for t in &touched {
            self.tiles.entry(*t).or_default().rev = at;
        }
        if self.options.idle_solve {
            self.idle_todo.extend(touched.iter().copied());
        }
        let keys: Vec<i64> = touched.iter().filter_map(|t| tile_key(&t.tile())).collect();
        // Phase 7: what it touched, for the outgoing filter.
        let mut touched_objects: Vec<(Kind, String)> = applied
            .map(|a| {
                let mut v = a.objects.clone();
                v.extend(a.sections.iter().map(|s| (Kind::Section, s.clone())));
                v
            })
            .unwrap_or_default();
        touched_objects.extend(op.targets().unwrap_or_default());
        touched_objects.sort();
        touched_objects.dedup();
        if logged {
            self.net
                .set_entry_interest(code, before.seq + 1, last, &[code.to_owned()], &keys)
                .map_err(e)?;
        }
        let routes: Vec<String> = routes.into_iter().collect();
        self.net
            .append_stream(&StreamRow {
                seq: at.seq,
                step: at.step as i32,
                line: code.to_owned(),
                branch: line.to_owned(),
                user: user.to_owned(),
                op: serde_json::to_string(op).map_err(|x| x.to_string())?,
                first_seq: first,
                log_seq: applied.map(|a| a.seq),
                tiles: keys,
                routes: routes.clone(),
                objects: Some(nr::object_keys(&touched_objects)),
                whole: whole || library,
            })
            .map_err(e)?;
        // Who may not see it (on what it touched as it is now, and as it
        // was for what went), then the places moved on.
        let restricted =
            self.restricted_clients(code, line, op, &touched_objects, &routes, whole || library);
        self.update_places(code, line, op, whole, &objects);
        let tiles: Vec<TileKey> = touched.into_iter().collect();
        let large = whole || tiles.len() > self.options.large_tiles;
        Ok((
            Change {
                model: code.to_owned(),
                user: user.to_owned(),
                op: op.clone(),
                line: line.to_owned(),
                seq: applied.map(|a| a.seq),
                first,
                at,
                tiles: tiles.clone(),
                routes: routes.clone(),
            },
            Touch {
                model: code.to_owned(),
                tiles,
                routes,
                at,
                large,
                restricted: Arc::new(restricted),
            },
        ))
    }

    /// The connected clients that may not see a change of model `code`
    /// (project line `line`) touching `objects` and `routes` (`at_large`:
    /// a merge, a library file, …): they read the tiles it touched again,
    /// through the filter. Worked out once per user.
    fn restricted_clients(
        &mut self,
        code: &str,
        line: &str,
        op: &Op,
        objects: &[(Kind, String)],
        routes: &[String],
        at_large: bool,
    ) -> HashSet<u64> {
        let started = Instant::now();
        let mut out = HashSet::new();
        let ids: Vec<(u64, String, Option<String>)> = self
            .clients
            .iter()
            .map(|(id, c)| (*id, c.user.clone(), c.see_as.clone()))
            .collect();
        let mut by_user: HashMap<(String, Option<String>), bool> = HashMap::new();
        for (id, user, see_as) in ids {
            let key = (user, see_as);
            let hidden = match by_user.get(&key) {
                Some(h) => *h,
                None => {
                    let h = match self.client_rights(id) {
                        Some(r) => {
                            !self.may_see_change(&r, code, line, op, objects, routes, at_large)
                        }
                        None => true,
                    };
                    by_user.insert(key, h);
                    h
                }
            };
            if hidden {
                out.insert(id);
            }
        }
        keep(
            &mut self.stats.rights_change_us,
            started.elapsed().as_secs_f64() * 1e6,
        );
        out
    }

    /// Whether `rights` may see a change (see [`NetHost::restricted_clients`]).
    #[allow(clippy::too_many_arguments)]
    fn may_see_change(
        &mut self,
        rights: &UserRights,
        code: &str,
        line: &str,
        op: &Op,
        objects: &[(Kind, String)],
        routes: &[String],
        at_large: bool,
    ) -> bool {
        if rights.sees_everything() {
            return true;
        }
        if at_large || line != ohl_session::versions::MAIN {
            return rights.sees_whole_line(code);
        }
        match op {
            Op::Do { .. } | Op::Undo | Op::Redo => {
                for (k, id) in objects {
                    let exists = self
                        .models
                        .get(code)
                        .is_some_and(|m| m.session.has_object(*k, id));
                    let seen = if exists {
                        self.visible(rights, code, *k, id)
                    } else {
                        // Gone: as it was.
                        self.models
                            .get(code)
                            .is_some_and(|m| nr::sees(rights, code, *k, id, m.places.get(*k, id)))
                    };
                    if !seen {
                        return false;
                    }
                }
                routes
                    .iter()
                    .all(|r| self.visible(rights, code, Kind::Route, r))
            }
            Op::SetSetting { .. } => {
                rights.allows(Permission::See, &nr::target(code, Kind::Meta, "line", None))
            }
            Op::PutLibrary { kind, name, .. } | Op::DeleteLibrary { kind, name } => rights.allows(
                Permission::See,
                &Target::Library {
                    line: code,
                    kind,
                    file: name,
                    entry: None,
                },
            ),
            _ => rights.sees_whole_line(code),
        }
    }

    /// The places of model `code` after a command: of what it changed in
    /// the main branch (and what moved with a structure), of everything
    /// after a change at large.
    fn update_places(
        &mut self,
        code: &str,
        line: &str,
        op: &Op,
        whole: bool,
        objects: &BTreeSet<(Kind, String)>,
    ) {
        let Some(m) = self.models.get_mut(code) else {
            return;
        };
        if whole {
            m.places = Places::of_session(&mut m.session);
            return;
        }
        if line != ohl_session::versions::MAIN {
            return;
        }
        let mut list: Vec<(Kind, String)> = objects.iter().cloned().collect();
        for (k, id) in objects {
            if *k == Kind::Structure || *k == Kind::AlignmentPoint {
                list.extend(m.session.object_referrers(*k, id));
            }
        }
        if let Op::PutLibrary { kind, name, .. } | Op::DeleteLibrary { kind, name } = op {
            list.push((Kind::Library, format!("{kind}/{name}")));
        }
        list.sort();
        list.dedup();
        let Model {
            session, places, ..
        } = m;
        places.update(session, &list);
        // Route parts on this model: resolved again when next asked.
        self.compiled_mut().retain(|_, r| !r.has_route_parts(code));
    }

    /// Model `code` read again from the database (after a command not
    /// kept), its tiles placed again and made stale.
    fn reload(&mut self, code: &str) {
        let read = self
            .net
            .store(code)
            .map_err(|e| e.to_string())
            .and_then(|s| Session::open_on(Box::new(s), code).map_err(|e| e.to_string()));
        match read {
            Ok(session) => {
                let before = self.model_tiles(code);
                if let Some(m) = self.models.get_mut(code) {
                    m.session = session;
                    m.session.set_role_source(Some(Arc::new(ModelRoles {
                        line: code.to_owned(),
                        rights: self.compiled.clone(),
                    })));
                    m.places = Places::of_session(&mut m.session);
                    m.rev += 1;
                    m.libs_rev += 1;
                }
                let stale: Vec<Placed> = self
                    .placed
                    .keys()
                    .filter(|p| &*p.0 == code)
                    .cloned()
                    .collect();
                for p in stale {
                    if let Some(old) = self.placed.remove(&p) {
                        for t in old {
                            if let Some(s) = self.tiles.get_mut(&t) {
                                s.objects.remove(&p);
                            }
                        }
                    }
                }
                self.place_model(code);
                // Cached packages of its tiles are not taken again.
                for t in before.union(&self.model_tiles(code)) {
                    self.tiles.entry(*t).or_default().rev.step += 1;
                }
            }
            Err(e) => log(&format!("reading line {code} again: {e}")),
        }
    }

    /// The routes and tiles client `client` follows from now on.
    fn interest(
        &mut self,
        client: u64,
        routes: Vec<String>,
        tiles: Vec<HeldTile>,
    ) -> Result<(), String> {
        if tiles.len() > self.options.max_tiles {
            return Err(format!(
                "{} tiles asked: a client views at most {} at once",
                tiles.len(),
                self.options.max_tiles
            ));
        }
        if let Some(t) = tiles.iter().find(|t| t.tile.0 != self.level) {
            return Err(format!(
                "tile {}: the packages are tiles of level {}",
                t.tile, self.level
            ));
        }
        let Some(c) = self.clients.get_mut(&client) else {
            return Err("not joined".into());
        };
        let routes: BTreeSet<String> = routes.into_iter().collect();
        let mut updates = Vec::new();
        if routes != c.routes {
            c.routes = routes.clone();
            updates.push(Update::Routes(routes));
        }
        let want: BTreeMap<TileKey, Option<Position>> =
            tiles.into_iter().map(|t| (t.tile, t.rev)).collect();
        let removed: Vec<TileKey> = c
            .tiles
            .iter()
            .filter(|t| !want.contains_key(t))
            .copied()
            .collect();
        for t in &removed {
            c.tiles.remove(t);
        }
        c.waiting.retain(|t| want.contains_key(t));
        if !removed.is_empty() {
            updates.push(Update::RemoveTiles(removed));
        }
        // A tile it holds at another revision than the tile's comes again
        // (it keeps following it meanwhile); one being made, when made.
        let new: Vec<(TileKey, Option<Position>, bool)> = want
            .into_iter()
            .filter(|(t, _)| !c.waiting.contains(t))
            .map(|(t, have)| (t, have, c.tiles.contains(&t)))
            .collect();
        for u in updates {
            self.push(client, Some(u), None, None);
        }
        // Phase 7: each tile in the client's rights class.
        let rights = self
            .client_rights(client)
            .ok_or("not joined (let go): join again")?;
        let mut held = Vec::new();
        for (t, have, holding) in new {
            let rev = self.tile_rev(&t);
            if have == Some(rev) {
                if !holding {
                    held.push(t);
                }
                continue;
            }
            let started = Instant::now();
            let class = self.class_of(&rights, t);
            if let Some(text) = self.package_for(t, rev, &class) {
                if let Some(c) = self.clients.get_mut(&client) {
                    c.tiles.insert(t);
                }
                self.push(client, Some(Update::AddTiles(vec![t])), Some(text), None);
                keep(
                    &mut self.stats.served_from_cache_us,
                    started.elapsed().as_secs_f64() * 1e6,
                );
                continue;
            }
            if let Some(c) = self.clients.get_mut(&client) {
                c.waiting.insert(t);
            }
            self.make(t, client)?;
        }
        if !held.is_empty() {
            if let Some(c) = self.clients.get_mut(&client) {
                c.tiles.extend(held.iter().copied());
            }
            self.push(client, Some(Update::AddTiles(held)), None, None);
        }
        Ok(())
    }

    /// Has tile `t`'s package made for `client` (and whoever asks
    /// meanwhile).
    fn make(&mut self, t: TileKey, client: u64) -> Result<(), String> {
        let rev = self.tile_rev(&t);
        if let Some(b) = self.building.get_mut(&t)
            && b.rev == rev
        {
            b.waiting.insert(client);
            return Ok(());
        }
        let waiting: BTreeSet<u64> = self
            .building
            .remove(&t)
            .map(|b| b.waiting)
            .unwrap_or_default()
            .into_iter()
            .chain([client])
            .collect();
        self.dispatch(t, waiting, 0)
    }

    /// Sends tile `t` to the packagers (an empty tile made here).
    fn dispatch(&mut self, t: TileKey, waiting: BTreeSet<u64>, tries: u32) -> Result<(), String> {
        let rev = self.tile_rev(&t);
        let bounds = self.grid.bounds(&t.tile());
        // By model: its structures, sections and alignments in the tile.
        type Ids = (Vec<String>, Vec<String>, Vec<String>);
        let mut by_model: BTreeMap<Arc<str>, Ids> = BTreeMap::new();
        if let Some(s) = self.tiles.get(&t) {
            for (model, kind, id) in &s.objects {
                let e = by_model.entry(model.clone()).or_default();
                match kind {
                    Kind::Structure => e.0.push(id.clone()),
                    Kind::Section => e.1.push(id.clone()),
                    Kind::Alignment => e.2.push(id.clone()),
                    _ => {}
                }
            }
        }
        if by_model.is_empty() {
            let text = Arc::new(crate::packager::empty(t, rev, &bounds, &self.case.name));
            self.deliver_built(t, rev, rev, text, waiting);
            return Ok(());
        }
        let mut parts = Vec::new();
        for (model, (structures, sections, alignments)) in by_model {
            let Some(m) = self.models.get_mut(&*model) else {
                continue;
            };
            // The part of its line the tile reads (never a copy of the whole
            // line).
            let line = Arc::new(m.session.drawing_part(
                &structures,
                &sections,
                &alignments,
                crate::packager::NEAR_ALIGNMENTS,
            ));
            parts.push(Part {
                model: model.to_string(),
                line,
                typed: m.session.any_typed(),
                libs_rev: m.libs_rev,
                libs: m.libs(),
                structures,
                sections,
                alignments,
            });
        }
        self.building.insert(
            t,
            Building {
                rev,
                waiting,
                tries,
            },
        );
        self.packager.send(BuildJob {
            tile: t,
            rev,
            bounds,
            parts,
            case: self.case.clone(),
            threads: self.options.solve_threads,
        })
    }

    /// A package made: kept, and given to whoever waits for it.
    fn built(&mut self, b: Built) {
        if self.idle_inflight == Some(b.tile) {
            self.idle_inflight = None;
            self.stats.idle_built += 1;
            self.stats.idle_s += b.seconds;
        }
        let Some(building) = self.building.remove(&b.tile) else {
            return;
        };
        let rev = self.tile_rev(&b.tile);
        keep(&mut self.stats.built_ms, b.seconds * 1e3);
        self.stats.solved += b.solved;
        self.stats.shapes_cached += b.cached;
        if b.rev != rev && building.tries < 5 {
            // Changed while it was made: made again.
            if let Err(e) = self.dispatch(b.tile, building.waiting, building.tries + 1) {
                log(&format!("tile {}: {e}", b.tile));
            }
            return;
        }
        self.deliver_built(b.tile, b.rev, rev, Arc::new(b.text), building.waiting);
    }

    fn deliver_built(
        &mut self,
        t: TileKey,
        made_at: Position,
        rev: Position,
        text: Arc<Utf8Bytes>,
        waiting: BTreeSet<u64>,
    ) {
        // A package nobody waits for (the idle solve) is kept while the
        // packages take less than half their bound: it never pushes out
        // what clients asked for.
        let kept =
            !waiting.is_empty() || self.packages.bytes() < (self.options.packages_mb << 20) / 2;
        if made_at == rev && kept {
            let bytes = text.len();
            self.packages.put((t, rev, whole()), text.clone(), bytes);
        }
        for client in waiting {
            let wanted = self
                .clients
                .get_mut(&client)
                .is_some_and(|c| c.waiting.remove(&t));
            if !wanted {
                continue;
            }
            // Phase 7: in the client's rights class.
            let Some(rights) = self.client_rights(client) else {
                continue;
            };
            let class = self.class_of(&rights, t);
            let text = if class.is_empty() {
                text.clone()
            } else {
                match self.package_for(t, rev, &class) {
                    Some(p) if made_at == rev => p,
                    _ => self.filtered(&text, &class),
                }
            };
            if let Some(c) = self.clients.get_mut(&client) {
                c.tiles.insert(t);
            }
            self.push(client, Some(Update::AddTiles(vec![t])), Some(text), None);
            if made_at != rev {
                self.push(
                    client,
                    None,
                    None,
                    Some(ServerMsg::TilesChanged {
                        tiles: vec![t],
                        at: rev,
                        reason: "it kept changing while it was made: read it again".into(),
                    }),
                );
            }
        }
    }

    /// The changes after `since` touching client `client`'s interest (its
    /// models and routes; its tiles too unless `models_only`), in order.
    fn catch_up(&mut self, client: u64, since: Position, models_only: bool) -> Result<(), String> {
        if since > self.position {
            return Err(format!(
                "position {since} is ahead of the server's ({}): join again",
                self.position
            ));
        }
        let Some(c) = self.clients.get(&client) else {
            return Err("not joined".into());
        };
        let models: Vec<String> = c.model.iter().cloned().collect();
        let routes: Vec<String> = c.routes.iter().cloned().collect();
        let tiles: Vec<i64> = if models_only {
            Vec::new()
        } else {
            c.tiles.iter().filter_map(|t| tile_key(&t.tile())).collect()
        };
        let limit = self.options.catch_up_max;
        let rows = self
            .net
            .stream_after(
                since.seq,
                since.step as i32,
                &models,
                &routes,
                &tiles,
                limit as i64 + 1,
            )
            .map_err(|e| e.to_string())?;
        let incomplete = rows.len() > limit;
        let mut changes = Vec::new();
        // Phase 7: a change is caught up as it is only by a client seeing
        // its whole line (its operation may name, or have moved, what the
        // client may not see — then or now); the others read their tiles
        // it touched again (through the filter).
        let rights = self.client_rights(client).ok_or("not joined")?;
        let held: BTreeSet<TileKey> = self
            .clients
            .get(&client)
            .map(|c| c.tiles.clone())
            .unwrap_or_default();
        let mut again: BTreeSet<TileKey> = BTreeSet::new();
        for r in rows.into_iter().take(limit) {
            if !rights.sees_everything() && !rights.sees_whole_line(&r.line) {
                for k in &r.tiles {
                    let t = TileKey::of(&ohl_store_pg::stream::tile_of_key(*k));
                    if held.contains(&t) {
                        again.insert(t);
                    }
                }
                continue;
            }
            changes.push(Change {
                model: r.line,
                user: r.user,
                op: serde_json::from_str(&r.op).map_err(|e| format!("stream {}: {e}", r.seq))?,
                line: r.branch,
                seq: r.log_seq,
                first: r.first_seq,
                at: Position {
                    seq: r.seq,
                    step: r.step.max(0) as u32,
                },
                tiles: r
                    .tiles
                    .iter()
                    .map(|k| TileKey::of(&ohl_store_pg::stream::tile_of_key(*k)))
                    .collect(),
                routes: r.routes,
            });
        }
        let seen = changes.iter().map(|c| c.at).max();
        self.push(
            client,
            seen.map(Update::Seen),
            None,
            Some(ServerMsg::CatchUp {
                from: since,
                to: self.position,
                changes,
                incomplete,
            }),
        );
        if !again.is_empty() {
            self.push(
                client,
                Some(Update::Seen(self.position)),
                None,
                Some(ServerMsg::TilesChanged {
                    tiles: again.into_iter().collect(),
                    at: self.position,
                    reason: "changed while you were away: read these tiles again".into(),
                }),
            );
        }
        Ok(())
    }

    fn leave(&mut self, client: u64, user: &str) {
        if let Some(c) = self.clients.remove(&client) {
            for t in c.waiting {
                if let Some(b) = self.building.get_mut(&t) {
                    b.waiting.remove(&client);
                }
            }
            let _ = c.user;
        }
        self.common.leave(user);
    }

    /// When the idle solve may make its next tile (none: nothing to make,
    /// one being made, or switched off).
    fn idle_due(&self) -> Option<Instant> {
        (self.options.idle_solve
            && !self.stats.idle_full
            && self.idle_inflight.is_none()
            && !self.idle_todo.is_empty())
        .then(|| (self.last_busy + self.options.idle_after).max(Instant::now()))
    }

    /// The idle solve (phase 6; owner, 2026-11-19: the context case solved
    /// ahead when the server is idle, low priority, its memory bounded):
    /// while no client's job came for [`NetOptions::idle_after`] and no
    /// tile a client asked for is being made, the next tile not kept at
    /// its revision is made — one at a time, so a client's tile waits for
    /// one at most. Its sections' shapes are kept (their own bound); it
    /// stops when they reach nine tenths of it.
    fn idle_step(&mut self) {
        if !self.options.idle_solve
            || self.stats.idle_full
            || self.idle_inflight.is_some()
            || !self.building.is_empty()
            || self.last_busy.elapsed() < self.options.idle_after
        {
            return;
        }
        let (_, shape_bytes, _, _) = self.packager.caches.shapes_size();
        if shape_bytes > (self.options.shapes_mb << 20) / 10 * 9 {
            self.stats.idle_full = true;
            log("the idle solve stopped: its shapes reached their bound");
            return;
        }
        while let Some(t) = self.idle_todo.pop_front() {
            let rev = self.tile_rev(&t);
            if self.packages.contains(&(t, rev, whole()))
                || self.tiles.get(&t).is_none_or(|s| s.objects.is_empty())
            {
                continue;
            }
            if self.stats.idle_from.is_none() {
                self.stats.idle_from = Some(self.born.elapsed().as_secs_f64());
            }
            self.idle_inflight = Some(t);
            if let Err(e) = self.dispatch(t, BTreeSet::new(), 0) {
                self.idle_inflight = None;
                log(&format!("the idle solve, tile {t}: {e}"));
            }
            return;
        }
        if self.stats.idle_done.is_none() && self.stats.idle_from.is_some() {
            let at = self.born.elapsed().as_secs_f64();
            self.stats.idle_done = Some(at);
            log(&format!(
                "the idle solve: {} tiles made in {:.1} s of the packagers' time",
                self.stats.idle_built, self.stats.idle_s
            ));
        }
    }

    fn probe(&mut self, what: Probe) -> Result<String, String> {
        match what {
            Probe::Stats => {
                let (shapes, shape_bytes, hits, asked) = self.packager.caches.shapes_size();
                let objects: usize = self.placed.len();
                Ok(serde_json::json!({
                    "start_s": self.stats.start_s,
                    "models": self.models.len(),
                    "tiles": self.tiles.len(),
                    "tiled_objects": objects,
                    "position": self.position.to_string(),
                    "clients": self.clients.len(),
                    "packages": self.packages.len(),
                    "packages_bytes": self.packages.bytes(),
                    "shapes": shapes,
                    "shapes_bytes": shape_bytes,
                    "shapes_hits": hits,
                    "shapes_asked": asked,
                    "solved": self.stats.solved,
                    "shapes_cached": self.stats.shapes_cached,
                    "applied_ms": self.stats.applied_ms,
                    "built_ms": self.stats.built_ms,
                    "served_from_cache_us": self.stats.served_from_cache_us,
                    "replica_ms": self.stats.replica_ms,
                    "rows_ms": self.stats.rows_ms,
                    "idle_built": self.stats.idle_built,
                    "idle_s": self.stats.idle_s,
                    "idle_from_s": self.stats.idle_from,
                    "idle_done_s": self.stats.idle_done,
                    "idle_full": self.stats.idle_full,
                    "idle_left": self.idle_todo.len(),
                    "rights_rev": self.rights.rev,
                    "rights_users": self.rights.users.len(),
                    "rights_assignments": self.rights.assignments.len(),
                    "rights_check_us": self.stats.rights_check_us,
                    "rights_class_us": self.stats.rights_class_us,
                    "rights_filter_us": self.stats.rights_filter_us,
                    "rights_change_us": self.stats.rights_change_us,
                    "rights_compile_us": self.stats.rights_compile_us,
                    "places": self.models.values().map(|m| m.places.len()).sum::<usize>(),
                })
                .to_string())
            }
            Probe::Area(b) => {
                let started = Instant::now();
                let area = Bounds::new([b[0], b[1]], [b[2], b[3]]);
                let tiles = self.tiles_of(&area);
                let mut objects = 0usize;
                let mut held = 0usize;
                for t in &tiles {
                    if let Some(s) = self.tiles.get(t) {
                        held += 1;
                        objects += s.objects.len();
                    }
                }
                Ok(serde_json::json!({
                    "tiles": tiles.len(),
                    "tiles_with_objects": held,
                    "objects": objects,
                    "us": started.elapsed().as_secs_f64() * 1e6,
                })
                .to_string())
            }
            Probe::ClearCaches => {
                self.packages.clear();
                self.packager.caches.clear();
                Ok("{}".into())
            }
        }
    }

    /// Runs `f`; a panic inside it (a bug) is refused instead of stopping
    /// the server, and every model read again from the database.
    fn guarded<T>(
        &mut self,
        what: &str,
        f: impl FnOnce(&mut Self) -> Result<T, String>,
    ) -> Result<T, String> {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&mut *self))) {
            Ok(r) => r,
            Err(_) => {
                log(&format!(
                    "{what} failed inside the server (a bug): refused; the network is read again"
                ));
                let _ = self.net.roll_back_unit();
                if let Err(e) = self.load() {
                    log(&format!("reading the network again: {e}"));
                }
                Err(format!(
                    "{what}: refused, an internal error of the server (it keeps running)"
                ))
            }
        }
    }
}

impl Host for NetHost {
    fn common(&mut self) -> &mut Common {
        &mut self.common
    }

    fn handle(&mut self, job: Job) {
        if !matches!(job, Job::Built(_) | Job::Probe { .. }) {
            self.last_busy = Instant::now();
            // Phase 7: rights an admin changed hold from the next job on.
            if !matches!(job, Job::Leave { .. }) {
                self.refresh_rights();
            }
        }
        match job {
            Job::Join {
                client,
                user,
                code,
                options,
                reply,
            } => {
                let joined = self.guarded("joining", |h| h.join(client, &user, &code, options));
                if joined.is_err() {
                    self.clients.remove(&client);
                }
                let _ = reply.send(joined);
            }
            Job::Request {
                client,
                user,
                id,
                op,
                line,
                model,
                reply,
            } => {
                let what = op.describe();
                let _ = reply
                    .send(self.guarded(&what, |h| h.request(client, &user, id, op, line, model)));
            }
            Job::Leave { client, user } => self.leave(client, &user),
            Job::Scene { reply, .. } => {
                let _ = reply.send(Err(
                    "a network's server sends tiles, not a scene: ask for tiles".into(),
                ));
            }
            Job::Interest {
                client,
                routes,
                tiles,
                reply,
            } => {
                let _ =
                    reply.send(self.guarded("the interest", |h| h.interest(client, routes, tiles)));
            }
            Job::Catch {
                client,
                since,
                reply,
            } => {
                let _ =
                    reply.send(self.guarded("the catch-up", |h| h.catch_up(client, since, false)));
            }
            Job::Net {
                client,
                user,
                ask,
                reply,
            } => {
                let _ =
                    reply.send(self.guarded("the working set", |h| h.net_ask(client, &user, ask)));
            }
            Job::Built(b) => self.built(*b),
            Job::Probe { what, reply } => {
                let _ = reply.send(self.probe(what));
            }
            #[cfg(test)]
            Job::Panic { reply } => {
                let _ = reply.send(self.guarded("a test", |_| -> Result<(), String> {
                    panic!("a job failing inside the authority")
                }));
            }
            Job::Presence { .. }
            | Job::Lock { .. }
            | Job::Unlock { .. }
            | Job::Peers { .. }
            | Job::Stop => {}
        }
    }

    fn due(&self) -> Option<Instant> {
        Some(match self.idle_due() {
            Some(idle) => idle.min(self.heartbeat_due),
            None => self.heartbeat_due,
        })
    }

    fn tick(&mut self) {
        if Instant::now() >= self.heartbeat_due {
            // Phase 7: rights changed while nobody asked anything (clients
            // let go within a heartbeat).
            self.refresh_rights();
            let at = self.position;
            self.common.send(Event::to(
                None,
                ServerMsg::Heartbeat { at, last: None },
                To::Heartbeat(at),
            ));
            self.heartbeat_due = Instant::now() + self.options.heartbeat;
        }
        self.idle_step();
    }

    fn stop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.copies);
    }
}
