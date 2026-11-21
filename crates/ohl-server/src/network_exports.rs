// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The exports' thread (owner, 2026-11-20: phase 12's question 6, "a
//! thread of its own"; built with phase 14).
//!
//! An offline working set's export (phase 12) is decided on the authority
//! and written on a thread of its own, so a large one (a long working set,
//! a reach of kilometres: seconds) no longer holds every user's edits:
//!
//! - **on the authority**, between two commands
//!   ([`NetHost::export_plan`]): the reach checked, the working set seen
//!   whole and exportable by the user's rights, main's changes recorded
//!   first (a revision) and the base taken, the line's and the other
//!   lines' neighbours within the reach chosen by the same rights, the
//!   replica (its rows), the meta and the export's number taken — all in
//!   one moment, so the file is the base revision's working set and its
//!   neighbours as they were then;
//! - **on the exports' thread** ([`write_export`], the same function the
//!   authority calls without the thread): the neighbours' structure types
//!   merged in, the project file written with its export revision, a
//!   client's file read back;
//! - **on the authority** when it comes back: the export recorded (its
//!   base, revision and tree) and audited, its answer sent. A user whose
//!   rights changed meanwhile gets nothing (the file removed, nothing
//!   recorded, the refusal audited); a client gone gets nothing.
//!
//! **Changes landing meanwhile** are not replayed into the file: it stays
//! the base revision's working set, which its record names — replayed, the
//! file would no longer be the revision recorded, and a bundle made from
//! it would carry those changes back as its own (applied twice, or
//! conflicting with themselves). Instead each change applied on a main
//! line while it is written is kept and, when it comes back, compared with
//! what it holds (its objects, lines and area): its report says how many
//! of its objects changed meanwhile and marks it **stale** — newer on the
//! server than in the file, exactly as if they had landed just after an
//! export on the authority; bringing its work back lists them as
//! conflicts with main or neighbours changed since (phase 12). Nothing is
//! lost and nothing applied twice.
//!
//! One client's export at a time (the queue of phase 13 orders them; a
//! check waiting behind one runs meanwhile); the command line's (the
//! network hosted in its own process) goes to the same thread.
//! `NetOptions::export_worker` (on) off keeps the authority's way.
//!
//! **A bundle brought back** (2026-11-20, the rest of the exports'
//! thread) goes the same way:
//!
//! - **on the authority**, between two commands
//!   ([`NetHost::bundle_decide`]): the bundle verified against the
//!   export's record (its base, its user, not brought back already, no
//!   library file nor read-only object changed) and the export's base read
//!   along the main line's ([`ohl_session::lines::LineSource`]);
//! - **on the exports' thread**: the branch's line made ready from it
//!   ([`ohl_session::lines::LineSource::prepare`]: the model read whole
//!   and validated, its spatial, reference and tree indexes — what took
//!   most of the time on a big model);
//! - **on the authority** when it comes back ([`NetHost::bundle_apply`],
//!   the final apply, the same function as without the thread): the
//!   branch made of the line made ready, the bundle's changes applied
//!   there as one command, checked by the rules of shared towers and the
//!   user's rights, committed, its conflicts with main listed — as main
//!   is then. The final apply stays on the authority: it changes the
//!   model (a branch, a command, a revision, the stream's position) and
//!   decides by the rights and the other lines as they are, which only
//!   the authority holds; it costs what the bundle changes.
//!
//! The line made ready is the export's base revision's, which no change
//! of main alters: **changes landing meanwhile** are main's when the
//! bundle is applied and are listed as its conflicts, exactly as on the
//! authority — nothing is replayed and nothing goes stale. **A user whose
//! rights changed meanwhile** gets nothing (nothing applied, the refusal
//! audited: bring it back again); a client gone gets nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc as std_mpsc;

use ohl_session::Kind;
use ohl_session::rights::UserRights;
use ohl_session::working_set::WorkingSetSpec;

use ohl_session::lines::{LineSource, PreparedLine};
use ohl_session::versions::Bundle;

use super::offline::{
    BundleDecided, ExportPlan, ExportWritten, Holds, Meanwhile, remove_project, write_export,
};
use super::{NetHost, keep, log};
use crate::server::Job;
use crate::{OfflineFile, ServerMsg, TileKey, random_code};

/// Where an export's answer goes.
pub(super) enum ExportTo {
    /// A client's export (`id` its question's).
    Client { client: u64, id: u64 },
    /// The command line's (the network hosted in its process).
    Command(std_mpsc::Sender<Result<serde_json::Value, String>>),
}

/// A change applied on a main line while an export was written.
struct Landed {
    code: String,
    objects: Vec<(Kind, String)>,
    tiles: BTreeSet<TileKey>,
    at_large: bool,
}

/// What the exports' thread does.
enum Work {
    /// An export written.
    Export(Box<ExportPlan>),
    /// A bundle's branch's line made ready.
    Bundle(Box<LineSource>),
}

/// What the exports' thread gives back (or why it could not).
enum Done {
    Export(Result<Box<ExportWritten>, String>),
    Bundle(Result<Box<PreparedLine>, String>),
}

/// What is in flight on the thread.
enum Flight {
    /// An export being written: where, and the changes applied since its
    /// content was taken.
    Export { path: PathBuf, landed: Vec<Landed> },
    /// A bundle decided, its branch's line being made ready.
    Bundle(Box<BundleDecided>),
}

/// Work on the exports' thread: who asked, with which rights, what.
struct InFlight {
    to: ExportTo,
    user: String,
    rights: Arc<UserRights>,
    flight: Flight,
}

/// Work back from the exports' thread.
pub(crate) struct ExportBack {
    key: u64,
    done: Done,
}

/// Tests: a gate the exports' thread waits at before each work while a
/// test holds it (`ExportGate::hold`) — the work decided on the
/// authority, then a change made, then the work let through: the change
/// lands while it is written or made ready, however long the test's own
/// steps take on a busy machine (a fixed wait of 1.5 s did not, 2026-11-21).
/// Never in the server outside the tests (a type with no state there).
#[derive(Default)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct ExportGate {
    #[cfg(test)]
    held: std::sync::Mutex<bool>,
    #[cfg(test)]
    moved: std::sync::Condvar,
}

/// A hold on an `ExportGate`: let go when dropped.
#[cfg(test)]
pub(crate) struct GateHeld<'a>(&'a ExportGate);

#[cfg(test)]
impl ExportGate {
    /// Holds the gate until the hold is dropped.
    pub(crate) fn hold(&self) -> GateHeld<'_> {
        *self.held.lock().unwrap_or_else(|e| e.into_inner()) = true;
        GateHeld(self)
    }
}

#[cfg(test)]
impl Drop for GateHeld<'_> {
    fn drop(&mut self) {
        *self.0.held.lock().unwrap_or_else(|e| e.into_inner()) = false;
        self.0.moved.notify_all();
    }
}

impl ExportGate {
    /// Waits while the gate is held (at most ten minutes: a test that
    /// forgot it fails on what it checks instead of hanging).
    fn pass(&self) {
        #[cfg(test)]
        {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(600);
            let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
            while *held {
                let left = until.saturating_duration_since(std::time::Instant::now());
                if left.is_zero() {
                    break;
                }
                held = self
                    .moved
                    .wait_timeout(held, left)
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
            }
        }
    }
}

/// The exports' thread and the work in flight.
#[derive(Default)]
pub(super) struct Exports {
    tx: Option<std_mpsc::Sender<(u64, Work)>>,
    next: u64,
    in_flight: BTreeMap<u64, InFlight>,
}

impl Exports {
    /// The exports' thread started; its exports come back on `jobs`
    /// (`gate`: tests, waited at before each work while a test holds it).
    pub(super) fn start(
        jobs: std_mpsc::Sender<Job>,
        gate: Option<Arc<ExportGate>>,
    ) -> Result<Self, String> {
        let (tx, rx) = std_mpsc::channel::<(u64, Work)>();
        std::thread::Builder::new()
            .name("ohl-exports".into())
            .spawn(move || {
                while let Ok((key, work)) = rx.recv() {
                    if let Some(g) = &gate {
                        g.pass();
                    }
                    // A panic is a refusal, not a stop.
                    let internal = |what: &str| {
                        format!("{what}: an internal error of the server (it keeps running)")
                    };
                    let done = match work {
                        Work::Export(plan) => {
                            let path = plan.head.path.clone();
                            Done::Export(
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    Box::new(write_export(*plan))
                                }))
                                .map_err(|_| {
                                    remove_project(&path);
                                    internal("the offline working set")
                                }),
                            )
                        }
                        Work::Bundle(source) => Done::Bundle(
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                source.prepare().map(Box::new).map_err(|e| e.to_string())
                            }))
                            .unwrap_or_else(|_| Err(internal("the bundle"))),
                        ),
                    };
                    if jobs
                        .send(Job::Exported(Box::new(ExportBack { key, done })))
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            tx: Some(tx),
            ..Self::default()
        })
    }

    /// Whether exports are written on the thread.
    pub(super) fn on_thread(&self) -> bool {
        self.tx.is_some()
    }

    /// Whether a client's work (an export, a bundle) is on the thread.
    pub(super) fn client_running(&self) -> bool {
        self.in_flight
            .values()
            .any(|f| matches!(f.to, ExportTo::Client { .. }))
    }
}

/// What of `holds` the changes `landed` touched.
fn meanwhile(landed: &[Landed], holds: &Holds) -> Meanwhile {
    let mut objects: BTreeSet<(&str, Kind, &str)> = BTreeSet::new();
    let mut at_large = false;
    for l in landed {
        let near = l.tiles.iter().any(|t| holds.tiles.contains(t));
        for (k, id) in &l.objects {
            if near || holds.objects.contains(&(l.code.clone(), *k, id.clone())) {
                objects.insert((&l.code, *k, id));
            }
        }
        at_large |= l.at_large && holds.models.contains(&l.code);
    }
    Meanwhile {
        objects: objects.len(),
        at_large,
    }
}

impl NetHost {
    /// Starts export `spec` of `user` (acting with `rights`) into `path`
    /// on the exports' thread: decided here, its answer to `to` when it
    /// comes back.
    #[allow(clippy::too_many_arguments)]
    fn start_export(
        &mut self,
        to: ExportTo,
        user: &str,
        rights: Arc<UserRights>,
        spec: WorkingSetSpec,
        reach: Option<f64>,
        path: PathBuf,
        for_client: bool,
    ) -> Result<(), String> {
        let started = std::time::Instant::now();
        let plan = self.export_plan(user, &rights, spec, reach, &path, for_client)?;
        keep(
            &mut self.stats.export_plan_ms,
            started.elapsed().as_secs_f64() * 1e3,
        );
        self.send_work(
            Work::Export(Box::new(plan)),
            InFlight {
                to,
                user: user.to_owned(),
                rights,
                flight: Flight::Export {
                    path,
                    landed: Vec::new(),
                },
            },
        )
    }

    /// Work sent to the exports' thread, kept in flight.
    fn send_work(&mut self, work: Work, flight: InFlight) -> Result<(), String> {
        let key = self.exports.next;
        self.exports.next += 1;
        let tx = self.exports.tx.as_ref().ok_or("no exports' thread")?;
        if tx.send((key, work)).is_err() {
            return Err("the exports' thread stopped".into());
        }
        self.exports.in_flight.insert(key, flight);
        Ok(())
    }

    /// Starts bringing `bundle` of `user` (acting with `rights`) back:
    /// decided here, its branch's line made ready on the exports' thread,
    /// applied when it comes back (answered to `to`).
    fn start_bundle(
        &mut self,
        to: ExportTo,
        user: &str,
        rights: Arc<UserRights>,
        bundle: Bundle,
        branch: Option<String>,
    ) -> Result<(), String> {
        let started = std::time::Instant::now();
        let decided = self.bundle_decide(user, &rights, bundle, branch)?;
        let source = self.bundle_source(&decided)?;
        keep(
            &mut self.stats.bundle_plan_ms,
            started.elapsed().as_secs_f64() * 1e3,
        );
        self.send_work(
            Work::Bundle(Box::new(source)),
            InFlight {
                to,
                user: user.to_owned(),
                rights,
                flight: Flight::Bundle(Box::new(decided)),
            },
        )
    }

    /// A client's bundle (its question `id`) started on the exports'
    /// thread.
    pub(super) fn start_client_bundle(
        &mut self,
        client: u64,
        user: &str,
        id: u64,
        bundle: Bundle,
        branch: Option<String>,
    ) -> Result<(), String> {
        let rights = self
            .client_rights(client)
            .ok_or("not joined (let go): join again")?;
        if self
            .clients
            .get(&client)
            .is_some_and(|c| c.see_as.is_some())
        {
            return Err("seeing as another user: nothing is changed so".into());
        }
        self.start_bundle(
            ExportTo::Client { client, id },
            user,
            rights,
            bundle,
            branch,
        )
    }

    /// The command line's bundle started on the exports' thread; `reply`
    /// answered when it comes back (or now, refused).
    pub(super) fn start_command_bundle(
        &mut self,
        user: &str,
        rights: Arc<UserRights>,
        bundle: Bundle,
        branch: Option<String>,
        reply: std_mpsc::Sender<Result<serde_json::Value, String>>,
    ) {
        let started = self.guarded("the bundle", |h| {
            h.start_bundle(
                ExportTo::Command(reply.clone()),
                user,
                rights.clone(),
                bundle,
                branch,
            )
        });
        if let Err(e) = started {
            let _ = reply.send(Err(self.sanitise(&rights, &e)));
        }
    }

    /// A client's export (its question `id`) started on the exports'
    /// thread.
    pub(super) fn start_client_export(
        &mut self,
        client: u64,
        user: &str,
        id: u64,
        spec: WorkingSetSpec,
        reach: Option<f64>,
    ) -> Result<(), String> {
        let rights = self
            .client_rights(client)
            .ok_or("not joined (let go): join again")?;
        if self
            .clients
            .get(&client)
            .is_some_and(|c| c.see_as.is_some())
        {
            return Err("seeing as another user: nothing is exported so".into());
        }
        // Phase 13: no name a client gave in a path.
        let path = self.copies.join(format!("offline-{}.ohl", random_code(16)));
        self.start_export(
            ExportTo::Client { client, id },
            user,
            rights,
            spec,
            reach,
            path,
            true,
        )
    }

    /// The command line's export (into `out`) started on the exports'
    /// thread; `reply` answered when it comes back (or now, refused).
    pub(super) fn start_command_export(
        &mut self,
        user: &str,
        rights: Arc<UserRights>,
        spec: WorkingSetSpec,
        reach: Option<f64>,
        out: PathBuf,
        reply: std_mpsc::Sender<Result<serde_json::Value, String>>,
    ) {
        let started = self.guarded("the offline working set", |h| {
            h.start_export(
                ExportTo::Command(reply.clone()),
                user,
                rights.clone(),
                spec,
                reach,
                out,
                false,
            )
        });
        if let Err(e) = started {
            let _ = reply.send(Err(self.sanitise(&rights, &e)));
        }
    }

    /// A change of main line `code` (see [`NetHost::stale_checks`]): kept
    /// for each export being written.
    pub(super) fn export_landed(
        &mut self,
        code: &str,
        objects: &[(Kind, String)],
        tiles: &BTreeSet<TileKey>,
        at_large: bool,
    ) {
        for f in self.exports.in_flight.values_mut() {
            if let Flight::Export { landed, .. } = &mut f.flight {
                landed.push(Landed {
                    code: code.to_owned(),
                    objects: objects.to_vec(),
                    tiles: tiles.clone(),
                    at_large,
                });
            }
        }
    }

    /// An export back from the exports' thread: recorded and answered
    /// (see the module's notes).
    pub(super) fn exported(&mut self, back: ExportBack) {
        let Some(f) = self.exports.in_flight.remove(&back.key) else {
            return;
        };
        // The rights as they are now (not read again before this job).
        self.refresh_rights();
        let InFlight {
            to,
            user,
            rights,
            flight,
        } = f;
        // The rights it was decided with, still the user's.
        let now = match &to {
            ExportTo::Client { client, .. } => self.client_rights(*client),
            ExportTo::Command(_) => {
                let active = self.rights.active(&user);
                active.then(|| self.rights_of(&user))
            }
        };
        let (path, landed, written) = match (flight, back.done) {
            (Flight::Export { path, landed }, Done::Export(w)) => (path, landed, w),
            (Flight::Bundle(d), Done::Bundle(ready)) => {
                self.bundle_back(to, &user, &rights, now, *d, ready);
                return;
            }
            _ => {
                log("the exports' thread: work came back as another kind (a bug)");
                return;
            }
        };
        let result = match (written, now) {
            (_, None) => {
                // Its client gone or let go: nothing to send, nothing kept.
                remove_project(&path);
                if let ExportTo::Command(reply) = to {
                    let _ = reply.send(Err(format!("{user} is not a user of this network")));
                }
                return;
            }
            (Err(e), Some(_)) => Err(e),
            (Ok(_), Some(now)) if !now.same_as(&rights) => {
                remove_project(&path);
                let why = "your rights changed while the offline working set was written: \
                           nothing exported — ask again"
                    .to_owned();
                self.audit_refused(&user, "an offline working set", &why);
                Err(why)
            }
            (Ok(w), Some(_)) => {
                let w = *w;
                let changed = meanwhile(&landed, &w.holds);
                let file = w.file.clone();
                self.guarded("the offline working set", |h| h.export_done(w, changed))
                    .map(|report| (report, file))
            }
        };
        if let Err(e) = &result {
            log(&format!("offline working set of {user}: {e}"));
        }
        match to {
            ExportTo::Client { client, id } => {
                let result = result
                    .and_then(|(report, file)| {
                        let file = file.ok_or("the file was not read back")??;
                        Ok(Box::new(OfflineFile { report, file }))
                    })
                    .map_err(|e| self.sanitise(&rights, &e));
                self.push(
                    client,
                    None,
                    None,
                    Some(ServerMsg::WorkingSetExported { id, result }),
                );
            }
            ExportTo::Command(reply) => {
                let result = result
                    .and_then(|(report, _)| serde_json::to_value(report).map_err(|e| e.to_string()))
                    .map_err(|e| self.sanitise(&rights, &e));
                let _ = reply.send(result);
            }
        }
    }
}

impl NetHost {
    /// A bundle's branch's line back from the exports' thread: applied
    /// ([`NetHost::bundle_apply`]) and answered — or nothing, its user's
    /// rights changed meanwhile or its client gone (see the module's
    /// notes).
    fn bundle_back(
        &mut self,
        to: ExportTo,
        user: &str,
        rights: &Arc<UserRights>,
        now: Option<Arc<UserRights>>,
        decided: BundleDecided,
        ready: Result<Box<PreparedLine>, String>,
    ) {
        let result = match (ready, now) {
            (_, None) => {
                if let ExportTo::Command(reply) = to {
                    let _ = reply.send(Err(format!("{user} is not a user of this network")));
                }
                return;
            }
            (Err(e), Some(_)) => Err(e),
            (Ok(_), Some(now)) if !now.same_as(rights) => {
                let why = "your rights changed while the bundle was made ready: nothing \
                           applied — bring it back again"
                    .to_owned();
                self.audit_refused(user, "a bundle", &why);
                Err(why)
            }
            (Ok(ready), Some(now)) => self.guarded("the bundle", |h| {
                h.bundle_apply(user, &now, decided, Some(*ready))
            }),
        };
        if let Err(e) = &result {
            log(&format!("bundle of {user}: {e}"));
        }
        let result = result.map_err(|e| self.sanitise(rights, &e));
        match to {
            ExportTo::Client { client, id } => {
                self.push(
                    client,
                    None,
                    None,
                    Some(ServerMsg::BundleImported {
                        id,
                        result: result.map(Box::new),
                    }),
                );
            }
            ExportTo::Command(reply) => {
                let _ = reply
                    .send(result.and_then(|r| serde_json::to_value(r).map_err(|e| e.to_string())));
            }
        }
    }
}
