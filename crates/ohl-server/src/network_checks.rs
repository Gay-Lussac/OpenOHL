// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The checks' worker thread (owner, 2026-11-20: phase 10's question 1,
//! "a worker thread of their own"; built with phase 13).
//!
//! A scoped check (phase 10) is computed on a thread of its own, so a
//! long one (a whole route's clearances: seconds) no longer holds every
//! user's edits. **The same result as on the authority**:
//!
//! - its working set worked out, its rights decided (the extent seen
//!   whole, the parameters bounded) on the authority, at the rights'
//!   revision of that moment;
//! - **a snapshot** taken there in the same moment ([`Snapshot`]): the
//!   tile index of everything the check may read (its working set's
//!   bounds grown by the farthest a check reads, 10 km, and the spans'
//!   margins — beyond, the check is refused as on the authority), each
//!   model with something there as a copy of its line, its library
//!   files, weather and load case files, and the network's largest span
//!   margin. The worker reads only the snapshot, through the same
//!   neighbourhood answers the authority gives (`HostNet`): the same
//!   sections and structures by tile, each part made by
//!   [`ohl_session::interest::line_part`] (the sub-line
//!   `Session::drawing_part` makes, tested bit for bit), the same
//!   libraries;
//! - its report written on the authority when it comes back, with the
//!   client's rights (a client whose rights changed meanwhile was let go:
//!   no report), and watched;
//! - **stale marks**: every change applied while it ran is kept and
//!   checked against the result's watch when it comes back — a change
//!   touching what it used marks it stale at once, as it would have been
//!   had the check run on the authority before the change.
//!
//! One check at a time (the queue of phase 13 orders them); the offline
//! export still runs on the authority (phase 12's question 6: next).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use ohl_core::grid::{Bounds, Grid};
use ohl_line::Line;
use ohl_line::index::{LineIndex, Of, Target};
use ohl_scope::{Cases, Neighbourhood, SpanChord, WorkingSet};
use ohl_session::Kind;
use ohl_session::working_set::{Extent, WorkingSetSpec};

use super::scoped::{HostViewer, MAX_CHECK_REACH, check_parameters};
use super::{NetHost, Placed, keep, log};
use crate::packager::{Caches, LibFiles};
use crate::scoped_report::Raw;
use crate::server::Job;
use crate::{ScopedCheck, ServerMsg, TileKey};

/// A model as a check's snapshot holds it.
struct SnapModel {
    line: Line,
    index: OnceLock<LineIndex>,
    typed: bool,
    libs_rev: u64,
    libs: Arc<LibFiles>,
    /// The wire library the authority had made already, if any.
    cables: Option<Arc<ohl_cables::Library>>,
    weather: Vec<(String, String)>,
    loads: Vec<(String, String)>,
    /// The working set's model of a check of loads: its structure
    /// libraries' files, for the shared towers' usage (2026-11-25).
    checks: Option<BTreeMap<String, Vec<(String, String)>>>,
}

/// What a check reads, taken on the authority when it starts.
pub(super) struct Snapshot {
    grid: Grid,
    level: i8,
    limit: Option<Bounds>,
    tiles: HashMap<TileKey, Vec<Placed>>,
    models: HashMap<String, SnapModel>,
    max_margin: f64,
    caches: Arc<Caches>,
    work: PathBuf,
}

/// A check for the worker.
pub(super) struct CheckJob {
    client: u64,
    id: u64,
    check: ScopedCheck,
    ws: WorkingSet,
    extent: Extent,
    snapshot: Snapshot,
    /// The authority's position and the rights' revision when it started.
    position: crate::Position,
    rights_rev: i64,
}

/// A check done by the worker.
pub(crate) struct CheckDone {
    client: u64,
    id: u64,
    name: String,
    ws: WorkingSet,
    extent: Extent,
    raw: Result<Raw, String>,
    seconds: f64,
    position: crate::Position,
    rights_rev: i64,
    /// The wire libraries the worker made (kept by the authority).
    libraries: Vec<(String, u64, Arc<ohl_cables::Library>)>,
}

/// The worker: one thread, one check at a time.
pub(super) struct Worker {
    tx: std_mpsc::Sender<CheckJob>,
}

impl Worker {
    /// Starts the worker; its results come back on `jobs`.
    pub(super) fn start(jobs: std_mpsc::Sender<Job>) -> Result<Self, String> {
        let (tx, rx) = std_mpsc::channel::<CheckJob>();
        std::thread::Builder::new()
            .name("ohl-checks".into())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    let done = run(job);
                    if jobs.send(Job::Checked(Box::new(done))).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { tx })
    }
}

/// Runs a check on its snapshot (a panic is a refusal, not a stop).
fn run(job: CheckJob) -> CheckDone {
    let started = Instant::now();
    let CheckJob {
        client,
        id,
        check,
        mut ws,
        extent,
        snapshot,
        position,
        rights_rev,
    } = job;
    let name = check.name().to_owned();
    let mut net = SnapNet {
        snap: snapshot,
        clipped: false,
        made: Vec::new(),
    };
    let raw = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::scoped_report::run(&mut net, &mut ws, &check, |n, m, s| n.has_twin(m, s))
    }))
    .unwrap_or_else(|_| {
        Err(format!(
            "{name}: refused, an internal error of the server (it keeps running)"
        ))
    })
    .and_then(|raw| {
        if net.clipped {
            Err(format!(
                "{name}: its reach is more than {MAX_CHECK_REACH} m around the working set \
                 (the rules' or the field's distances): refused"
            ))
        } else {
            Ok(raw)
        }
    });
    CheckDone {
        client,
        id,
        name,
        ws,
        extent,
        raw,
        seconds: started.elapsed().as_secs_f64(),
        position,
        rights_rev,
        libraries: net.made,
    }
}

/// The neighbourhood a snapshot gives: the authority's answers
/// (`HostNet`) as they were when the check started.
struct SnapNet {
    snap: Snapshot,
    clipped: bool,
    made: Vec<(String, u64, Arc<ohl_cables::Library>)>,
}

impl SnapNet {
    fn objects_in(&mut self, area: &Bounds, kind: Kind) -> Vec<(String, String)> {
        if self.clipped || self.snap.limit.is_some_and(|l| !l.contains(area)) {
            self.clipped = true;
            return Vec::new();
        }
        let mut out = BTreeSet::new();
        for t in ohl_store_pg::stream::tiles_of(&self.snap.grid, area, self.snap.level).iter() {
            if let Some(objects) = self.snap.tiles.get(&TileKey::of(t)) {
                for (m, k, id) in objects {
                    if *k == kind {
                        out.insert((m.to_string(), id.clone()));
                    }
                }
            }
        }
        out.into_iter().collect()
    }

    fn line(&self, model: &str) -> Option<&Line> {
        self.snap.models.get(model).map(|m| &m.line)
    }

    fn has_twin(&mut self, model: &str, id: &str) -> bool {
        let Some((p, kind)) = self.structure(model, id) else {
            return false;
        };
        let tol = ohl_scope::reach::TWIN_TOLERANCE;
        let area = Bounds::new([p[0], p[1]], [p[0], p[1]]).grown(tol);
        let near = self.structures_in(&area);
        near.into_iter().any(|(m, t)| {
            m != model
                && self
                    .structure(&m, &t)
                    .is_some_and(|(q, k)| (q[0] - p[0]).hypot(q[1] - p[1]) <= tol && k == kind)
        })
    }

    fn work(&self) -> PathBuf {
        self.snap.work.clone()
    }
}

impl Neighbourhood for SnapNet {
    fn sections_in(&mut self, area: &Bounds) -> Vec<(String, String)> {
        self.objects_in(area, Kind::Section)
    }

    fn structures_in(&mut self, area: &Bounds) -> Vec<(String, String)> {
        let found = self.objects_in(area, Kind::Structure);
        found
            .into_iter()
            .filter(|(m, id)| {
                self.line(m).and_then(|l| l.structure(id)).is_some_and(|s| {
                    let p = [s.position[0], s.position[1]];
                    area.intersects(&Bounds::new(p, p))
                })
            })
            .collect()
    }

    fn structure(&mut self, model: &str, id: &str) -> Option<(ohl_line::Point, Option<String>)> {
        self.line(model)?
            .structure(id)
            .map(|s| (s.position, s.structure_type.clone()))
    }

    fn sections_at(&mut self, model: &str, id: &str) -> Vec<String> {
        let Some(m) = self.snap.models.get(model) else {
            return Vec::new();
        };
        let index = m.index.get_or_init(|| LineIndex::of(&m.line));
        let mut out: Vec<String> = index
            .referrers(&Target::Object(Of::Structure, id.to_owned()))
            .filter(|(of, _)| *of == Of::Section)
            .map(|(_, id)| id.clone())
            .collect();
        out.sort();
        out.dedup();
        out
    }

    fn chords(&mut self, model: &str, sections: &[String]) -> Vec<(String, Vec<SpanChord>)> {
        let Some(line) = self.line(model) else {
            return Vec::new();
        };
        let wanted: BTreeSet<&String> = sections.iter().collect();
        line.sections
            .iter()
            .filter(|s| wanted.contains(&s.id))
            .map(|s| {
                (
                    s.id.clone(),
                    ohl_scope::bound::chords_of(s, |id| line.structure(id)),
                )
            })
            .collect()
    }

    fn part(
        &mut self,
        model: &str,
        structures: &[String],
        sections: &[String],
    ) -> Result<Line, String> {
        let work = self.work();
        let m = self
            .snap
            .models
            .get(model)
            .ok_or_else(|| format!("the network has no line {model}"))?;
        let part = ohl_session::interest::line_part(
            &m.line,
            structures,
            sections,
            &[],
            ohl_session::interest::SOLVE_NEAR,
        );
        if !m.typed {
            return Ok(part);
        }
        let libs = crate::packager::model_libs(
            model,
            true,
            m.libs_rev,
            m.libs.clone(),
            &self.snap.caches,
            &work,
        );
        Ok(crate::packager::resolve_part(part, &libs).0)
    }

    fn potentials(&mut self, model: &str) -> Vec<Option<ohl_line::electrical::Potential>> {
        self.line(model)
            .map(ohl_scope::system_potentials)
            .unwrap_or_default()
    }

    fn library(&mut self, model: &str) -> Result<Arc<ohl_cables::Library>, String> {
        let work = self.work();
        let m = self
            .snap
            .models
            .get_mut(model)
            .ok_or_else(|| format!("the network has no line {model}"))?;
        if let Some(l) = &m.cables {
            return Ok(l.clone());
        }
        let libs = crate::packager::model_libs(
            model,
            m.typed,
            m.libs_rev,
            m.libs.clone(),
            &self.snap.caches,
            &work,
        );
        let l = Arc::new(libs.cables.clone());
        m.cables = Some(l.clone());
        self.made.push((model.to_owned(), m.libs_rev, l.clone()));
        Ok(l)
    }

    fn cases(&mut self, model: &str) -> Result<Arc<Cases>, String> {
        let m = self
            .snap
            .models
            .get(model)
            .ok_or_else(|| format!("the network has no line {model}"))?;
        super::scoped::cases_of(model, &m.weather, &m.loads)
    }

    fn max_margin(&mut self) -> f64 {
        self.snap.max_margin
    }

    fn structure_checks(
        &mut self,
        model: &str,
    ) -> Option<Result<Arc<ohl_scope::loads::StructureChecks>, String>> {
        let Some(files) = self.snap.models.get(model).and_then(|m| m.checks.as_ref()) else {
            return Some(Err(format!("line {model}'s structure libraries not read")));
        };
        let work = self.snap.work.with_file_name("checks-types");
        Some(crate::packager::structure_checks(files, &work, false).map(Arc::new))
    }
}

impl NetHost {
    /// Starts check `check` (its id `id`) for `client` on working set
    /// `spec` (none: the one it follows) on the worker: its extent, rights
    /// and parameters decided here, its snapshot taken here.
    pub(super) fn start_check(
        &mut self,
        client: u64,
        id: u64,
        spec: Option<WorkingSetSpec>,
        check: ScopedCheck,
    ) -> Result<(), String> {
        let started = Instant::now();
        let rights = self
            .client_rights(client)
            .ok_or("not joined (let go): join again")?;
        check_parameters(&check)?;
        let (ws, extent) = self.check_working_set(client, &rights, spec)?;
        let max_margin = self.network_max_margin();
        let limit = extent.summary.bounds.map(|b| {
            Bounds::new([b[0], b[1]], [b[2], b[3]]).grown(MAX_CHECK_REACH + 2.0 * max_margin)
        });
        // The tiles it may read, and the models with something there.
        let mut tiles = HashMap::new();
        let mut codes: BTreeSet<String> = BTreeSet::from([ws.model.clone()]);
        if let Some(l) = &limit {
            for t in self.tiles_of(l) {
                if let Some(s) = self.tiles.get(&t) {
                    let objects: Vec<Placed> = s
                        .objects
                        .iter()
                        .filter(|(_, k, _)| matches!(k, Kind::Section | Kind::Structure))
                        .cloned()
                        .collect();
                    codes.extend(objects.iter().map(|(m, _, _)| m.to_string()));
                    if !objects.is_empty() {
                        tiles.insert(t, objects);
                    }
                }
            }
        }
        let mut models = HashMap::new();
        for code in codes {
            let cables = self.scoped_library(&code).map(Clone::clone);
            let Some(m) = self.models.get_mut(&code) else {
                continue;
            };
            let files = |kind: &str| m.session.library_files(kind).map_err(|e| e.to_string());
            let (weather, loads) = (files("weather")?, files("loadcases")?);
            let checks = if code == ws.model && matches!(check, ScopedCheck::StructureLoads { .. })
            {
                let mut kinds = BTreeMap::new();
                for (kind, _) in crate::packager::CHECK_LIBRARIES {
                    kinds.insert(kind.to_owned(), files(kind)?);
                }
                Some(kinds)
            } else {
                None
            };
            models.insert(
                code,
                SnapModel {
                    line: m.session.line().clone(),
                    index: OnceLock::new(),
                    typed: m.session.any_typed(),
                    libs_rev: m.libs_rev,
                    libs: m.libs(),
                    cables,
                    weather,
                    loads,
                    checks,
                },
            );
        }
        let snapshot = Snapshot {
            grid: self.grid,
            level: self.level,
            limit,
            tiles,
            models,
            max_margin,
            caches: self.packager.caches.clone(),
            work: self.copies.join("work").join("checks"),
        };
        keep(
            &mut self.stats.check_snapshot_ms,
            started.elapsed().as_secs_f64() * 1e3,
        );
        self.scoped_in_flight(true);
        let job = CheckJob {
            client,
            id,
            check,
            ws,
            extent,
            snapshot,
            position: self.position,
            rights_rev: self.rights.rev,
        };
        let worker = self.worker.as_ref().ok_or("no checks' worker")?;
        if worker.tx.send(job).is_err() {
            self.scoped_in_flight(false);
            return Err("the checks' worker stopped".into());
        }
        self.check_running = true;
        Ok(())
    }

    /// A check back from the worker: its report as its client may see it,
    /// watched, and stale at once if a change applied meanwhile touched
    /// what it used.
    pub(super) fn finish_check(&mut self, done: CheckDone) {
        self.check_running = false;
        let events = self.scoped_in_flight(false);
        for (code, rev, l) in done.libraries {
            self.keep_scoped_library(&code, rev, l);
        }
        let CheckDone {
            client,
            id,
            name,
            ws,
            extent,
            raw,
            seconds,
            position,
            rights_rev,
            ..
        } = done;
        keep(self.scoped_ms(&name), seconds * 1e3);
        // Its client gone, or let go because its rights changed: nothing
        // to send.
        let Some(rights) = self.client_rights(client) else {
            return;
        };
        if self.rights.rev != rights_rev {
            log(&format!(
                "check {id} of client {client}: the rights changed while it ran (the client's \
                 the same)"
            ));
        }
        let msg = match raw {
            Ok(raw) => {
                let report = crate::scoped_report::report(
                    &mut HostViewer {
                        host: self,
                        rights: &rights,
                    },
                    &ws,
                    &name,
                    &raw,
                    seconds,
                );
                let k = self.watch(client, id, &name, &ws, &extent, &raw);
                self.push(
                    client,
                    None,
                    None,
                    Some(ServerMsg::CheckResult {
                        id,
                        result: Ok(Box::new(report)),
                    }),
                );
                if self.position != position {
                    self.stale_one(k, &events);
                }
                return;
            }
            Err(e) => ServerMsg::CheckResult {
                id,
                result: Err(self.sanitise(&rights, &e)),
            },
        };
        self.push(client, None, None, Some(msg));
    }
}
