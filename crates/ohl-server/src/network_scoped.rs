// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Scoped computations on a network's server (ADR-0013, phase 10): a
//! check on a working set, its neighbours pulled in by the check's reach
//! (`ohl_scope`), computed on the authority from the models it holds and
//! their tile index — each part taken as `Session::drawing_part` takes
//! it, never a whole line copied.
//!
//! **Rights**: the check is computed with every neighbour, those its user
//! may not see too (a hidden neighbour computed on the server, owner
//! 2026-11-16); its report says only what the rights allow — a neighbour
//! the user may not see is never named nor placed: "a hidden object of
//! another line is within X m", "check result limited by an object you
//! may not see"; counted in what was pulled in. Its texts naming one are
//! replaced.
//!
//! **Stale**: each result is watched — the objects it used, the tiles its
//! reach covers, the lines it used — and the first change touching any of
//! them sends [`ServerMsg::CheckStale`] (naming only what the user may
//! see).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Instant;

use ohl_core::grid::Bounds;
use ohl_scope::{Cases, Neighbour, NeighbourKind, Neighbourhood, SpanChord, WorkingSet};
use ohl_session::Kind;
use ohl_session::rights::UserRights;
use ohl_session::working_set::{Extent, WorkingSetSpec};

use super::{Model, NetHost, keep};
use crate::scoped_report::Raw;
use crate::{ScopedCheck, ServerMsg, TileKey};

/// What the authority keeps for the scoped checks.
#[derive(Default)]
pub(super) struct ScopedState {
    /// Each model's largest span margin, at its revision.
    margins: HashMap<String, (u64, f64)>,
    /// Each model's wire library, at its library revision.
    libraries: HashMap<String, (u64, Arc<ohl_cables::Library>)>,
    /// The results watched for changes.
    watches: Vec<Watch>,
    /// Timings (the bench): per check, ms.
    pub(super) ms: BTreeMap<String, Vec<f64>>,
    /// Phase 13: the changes applied while a check runs on the worker
    /// (checked against its watch when it comes back).
    in_flight: Option<Vec<StaleEvent>>,
}

/// A change as the stale marks read it: its model, user, objects, tiles,
/// at large.
#[derive(Clone)]
pub(super) struct StaleEvent {
    code: String,
    user: String,
    objects: Vec<(Kind, String)>,
    tiles: BTreeSet<TileKey>,
    at_large: bool,
}

/// A result watched: what it used.
struct Watch {
    client: u64,
    id: u64,
    check: String,
    /// The working set's objects and the neighbours used.
    objects: BTreeSet<(String, Kind, String)>,
    /// The tiles its reach covers (a new object there changes it too).
    tiles: BTreeSet<TileKey>,
    /// The lines it read (a change of a line at large changes it).
    models: BTreeSet<String>,
}

fn kind_of(k: NeighbourKind) -> Kind {
    match k {
        NeighbourKind::Section => Kind::Section,
        NeighbourKind::Jumper => Kind::Jumper,
        NeighbourKind::Structure => Kind::Structure,
    }
}

/// The network as a scoped check reads it: the authority's models and
/// tile index.
struct HostNet<'a> {
    host: &'a mut NetHost,
    /// Phase 13: what a check may read at most (the working set's bounds
    /// grown by [`MAX_CHECK_REACH`] and the spans' margins): an area
    /// asked beyond it reads nothing more, and the check is refused after
    /// (`clipped`) — a check's reach comes from what its client sent (the
    /// rules' distances, the field's distance).
    limit: Option<Bounds>,
    clipped: bool,
}

impl<'a> HostNet<'a> {
    fn new(host: &'a mut NetHost) -> Self {
        Self {
            host,
            limit: None,
            clipped: false,
        }
    }
}

/// Phase 13: the farthest a check run on the server reads around its
/// working set, m (a plugin's declared reach at most, `ohl_scope`).
pub(crate) const MAX_CHECK_REACH: f64 = ohl_scope::reach::MAX_DECLARED_REACH;

/// Phase 13: the field distance a client may ask, m.
pub(crate) const MAX_FIELD_DISTANCE: f64 = 2_000.0;

/// Phase 13: the clearance rules' text a client may send, bytes.
pub(crate) const MAX_RULES: usize = 256 << 10;

/// Phase 13: structures a check of loads may name.
pub(crate) const MAX_CHECK_STRUCTURES: usize = 10_000;

/// Phase 13: a check's parameters as a client sent them, refused before
/// it waits for its turn (what it would make the server read or compute
/// beyond the limits).
pub(crate) fn check_parameters(check: &ScopedCheck) -> Result<(), String> {
    match check {
        ScopedCheck::Clearances { case, rules } => {
            if rules.len() > MAX_RULES || case.len() > 256 {
                return Err(format!(
                    "clearance rules of {} KB: at most {} KB",
                    rules.len() >> 10,
                    MAX_RULES >> 10
                ));
            }
        }
        ScopedCheck::Fields {
            case,
            distance,
            amps,
            limit_ut,
        } => {
            if let Some(d) = distance
                && !(d.is_finite() && *d > 0.0 && *d <= MAX_FIELD_DISTANCE)
            {
                return Err(format!(
                    "the field distance: more than 0 m and at most {MAX_FIELD_DISTANCE} m"
                ));
            }
            if amps.is_some_and(|a| !(a.is_finite() && a >= 0.0))
                || limit_ut.is_some_and(|l| !(l.is_finite() && l >= 0.0))
                || case.len() > 256
            {
                return Err("the field's current and limit: finite, not negative".into());
            }
        }
        ScopedCheck::ElectricField {
            case,
            distance,
            limit_kv_m,
        } => {
            if let Some(d) = distance
                && !(d.is_finite() && *d > 0.0 && *d <= MAX_FIELD_DISTANCE)
            {
                return Err(format!(
                    "the field distance: more than 0 m and at most {MAX_FIELD_DISTANCE} m"
                ));
            }
            if limit_kv_m.is_some_and(|l| !(l.is_finite() && l >= 0.0)) || case.len() > 256 {
                return Err("the field's limit: finite, not negative".into());
            }
        }
        ScopedCheck::StructureLoads { structures } => {
            if structures.len() > MAX_CHECK_STRUCTURES {
                return Err(format!(
                    "{} structures: a check of loads takes at most {MAX_CHECK_STRUCTURES}",
                    structures.len()
                ));
            }
        }
    }
    Ok(())
}

/// A model's cases from its weather and load case files (the defaults
/// where it has none) — the authority's and the checks' worker's.
pub(super) fn cases_of(
    model: &str,
    weather: &[(String, String)],
    loads: &[(String, String)],
) -> Result<Arc<Cases>, String> {
    let mut cases = ohl_scope::default_cases()?;
    if !weather.is_empty() {
        cases.weather.clear();
        for (name, text) in weather {
            let w = ohl_weather::WeatherCases::parse(text)
                .map_err(|e| format!("weather file {name}: {e}"))?;
            cases.weather.extend(w.cases);
        }
        cases.source = format!("line {model}'s own library");
    }
    if !loads.is_empty() {
        cases.loads.clear();
        for (name, text) in loads {
            let l = ohl_loads::LoadCases::parse(text)
                .map_err(|e| format!("load case file {name}: {e}"))?;
            cases.loads.extend(l.cases);
        }
        cases.source = format!("line {model}'s own library");
    }
    Ok(Arc::new(cases))
}

impl HostNet<'_> {
    fn objects_in(&mut self, area: &Bounds, kind: Kind) -> Vec<(String, String)> {
        // Beyond the limit: nothing more is read (the check is refused
        // after, its little work done on nothing).
        if self.clipped || self.limit.is_some_and(|l| !l.contains(area)) {
            self.clipped = true;
            return Vec::new();
        }
        let mut out = BTreeSet::new();
        for t in self.host.tiles_of(area) {
            if let Some(s) = self.host.tiles.get(&t) {
                for (m, k, id) in &s.objects {
                    if *k == kind {
                        out.insert((m.to_string(), id.clone()));
                    }
                }
            }
        }
        out.into_iter().collect()
    }

    fn model(&mut self, code: &str) -> Option<&mut Model> {
        self.host.models.get_mut(code)
    }
}

impl Neighbourhood for HostNet<'_> {
    fn sections_in(&mut self, area: &Bounds) -> Vec<(String, String)> {
        self.objects_in(area, Kind::Section)
    }

    fn structures_in(&mut self, area: &Bounds) -> Vec<(String, String)> {
        let found = self.objects_in(area, Kind::Structure);
        found
            .into_iter()
            .filter(|(m, id)| {
                self.host
                    .models
                    .get(m)
                    .and_then(|m| m.session.line().structure(id))
                    .is_some_and(|s| {
                        let p = [s.position[0], s.position[1]];
                        area.intersects(&Bounds::new(p, p))
                    })
            })
            .collect()
    }

    fn structure(&mut self, model: &str, id: &str) -> Option<(ohl_line::Point, Option<String>)> {
        self.host
            .models
            .get(model)?
            .session
            .line()
            .structure(id)
            .map(|s| (s.position, s.structure_type.clone()))
    }

    fn sections_at(&mut self, model: &str, id: &str) -> Vec<String> {
        let Some(m) = self.model(model) else {
            return Vec::new();
        };
        m.session
            .object_referrers(Kind::Structure, id)
            .into_iter()
            .filter(|(k, _)| *k == Kind::Section)
            .map(|(_, id)| id)
            .collect()
    }

    fn chords(&mut self, model: &str, sections: &[String]) -> Vec<(String, Vec<SpanChord>)> {
        let Some(m) = self.host.models.get(model) else {
            return Vec::new();
        };
        let line = m.session.line();
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
    ) -> Result<ohl_line::Line, String> {
        let work = self.host.copies.join("work").join("scoped");
        let caches = self.host.packager.caches.clone();
        let m = self
            .model(model)
            .ok_or_else(|| format!("the network has no line {model}"))?;
        let part =
            m.session
                .drawing_part(structures, sections, &[], ohl_session::interest::SOLVE_NEAR);
        if !m.session.any_typed() {
            return Ok(part);
        }
        let libs = crate::packager::model_libs(model, true, m.libs_rev, m.libs(), &caches, &work);
        Ok(crate::packager::resolve_part(part, &libs).0)
    }

    fn potentials(&mut self, model: &str) -> Vec<Option<ohl_line::electrical::Potential>> {
        self.host
            .models
            .get(model)
            .map(|m| ohl_scope::system_potentials(m.session.line()))
            .unwrap_or_default()
    }

    fn library(&mut self, model: &str) -> Result<Arc<ohl_cables::Library>, String> {
        let work = self.host.copies.join("work").join("scoped");
        let caches = self.host.packager.caches.clone();
        let m = self
            .host
            .models
            .get_mut(model)
            .ok_or_else(|| format!("the network has no line {model}"))?;
        let rev = m.libs_rev;
        if let Some((r, l)) = self.host.scoped.libraries.get(model)
            && *r == rev
        {
            return Ok(l.clone());
        }
        let typed = m.session.any_typed();
        let libs = crate::packager::model_libs(model, typed, rev, m.libs(), &caches, &work);
        let l = Arc::new(libs.cables.clone());
        self.host
            .scoped
            .libraries
            .insert(model.to_owned(), (rev, l.clone()));
        Ok(l)
    }

    fn cases(&mut self, model: &str) -> Result<Arc<Cases>, String> {
        let m = self
            .host
            .models
            .get(model)
            .ok_or_else(|| format!("the network has no line {model}"))?;
        let weather = m
            .session
            .library_files("weather")
            .map_err(|e| e.to_string())?;
        let loads = m
            .session
            .library_files("loadcases")
            .map_err(|e| e.to_string())?;
        cases_of(model, &weather, &loads)
    }

    fn max_margin(&mut self) -> f64 {
        let mut max: f64 = 0.0;
        let codes: Vec<String> = self.host.models.keys().cloned().collect();
        for code in codes {
            let m = &self.host.models[&code];
            let margin = match self.host.scoped.margins.get(&code) {
                Some((rev, v)) if *rev == m.rev => *v,
                // A bound may only grow: the larger of the last and now.
                old => {
                    let v = ohl_scope::bound::max_margin(m.session.line())
                        .max(old.map_or(0.0, |o| o.1));
                    self.host.scoped.margins.insert(code.clone(), (m.rev, v));
                    v
                }
            };
            max = max.max(margin);
        }
        max
    }
}

impl NetHost {
    /// Runs check `check` (its id `id`) for `client` on working set `spec`
    /// (none: the one it follows), its neighbours pulled in; the report as
    /// `rights` may see it, the result watched.
    pub(super) fn scoped_check(
        &mut self,
        client: u64,
        rights: &UserRights,
        id: u64,
        spec: Option<WorkingSetSpec>,
        check: ScopedCheck,
    ) -> Result<ServerMsg, String> {
        let started = Instant::now();
        let name = check.name().to_owned();
        check_parameters(&check)?;
        let (mut ws, extent) = self.check_working_set(client, rights, spec)?;
        let raw = {
            let mut net = HostNet::new(self);
            let margin = net.max_margin();
            net.limit = extent.summary.bounds.map(|b| {
                Bounds::new([b[0], b[1]], [b[2], b[3]]).grown(MAX_CHECK_REACH + 2.0 * margin)
            });
            let raw =
                crate::scoped_report::run(&mut net, &mut ws, &check, |n, m, s| n.has_twin(m, s))?;
            if net.clipped {
                return Err(format!(
                    "{name}: its reach is more than {MAX_CHECK_REACH} m around the working set \
                     (the rules' or the field's distances): refused"
                ));
            }
            raw
        };
        let seconds = started.elapsed().as_secs_f64();
        keep(
            self.scoped.ms.entry(name.clone()).or_default(),
            seconds * 1e3,
        );
        let report = crate::scoped_report::report(
            &mut HostViewer { host: self, rights },
            &ws,
            &name,
            &raw,
            seconds,
        );
        self.watch(client, id, &name, &ws, &extent, &raw);
        Ok(ServerMsg::CheckResult {
            id,
            result: Ok(Box::new(report)),
        })
    }

    /// A check's working set (given, or the one `client` follows): its
    /// extent seen whole by `rights` (refused naming nothing hidden).
    pub(super) fn check_working_set(
        &mut self,
        client: u64,
        rights: &UserRights,
        spec: Option<WorkingSetSpec>,
    ) -> Result<(WorkingSet, Extent), String> {
        let (model, extent): (String, Extent) = match spec {
            Some(spec) => {
                let extent = {
                    let m = self
                        .models
                        .get_mut(&spec.model)
                        .ok_or_else(|| format!("the network has no line {}", spec.model))?;
                    m.session
                        .pick_extent(&spec.picks)
                        .map_err(|e| e.to_string())
                };
                let extent = extent.map_err(|e| self.sanitise(rights, &e))?;
                self.check_extent(rights, &spec.model, &extent)?;
                (spec.model, extent)
            }
            None => {
                let c = self.clients.get(&client).ok_or("not joined")?;
                let ws =
                    c.ws.as_ref()
                        .ok_or("no working set: join with one, or give the working set to check")?;
                (ws.spec.model.clone(), ws.extent.clone())
            }
        };
        let of = |k: Kind| -> Vec<String> {
            extent
                .wanted
                .iter()
                .filter(|(x, _)| *x == k)
                .map(|(_, id)| id.clone())
                .collect()
        };
        let ws = WorkingSet {
            model: model.clone(),
            sections: of(Kind::Section),
            jumpers: of(Kind::Jumper),
            structures: of(Kind::Structure),
        };
        Ok((ws, extent))
    }

    /// Phase 13: a check runs on the worker (`on`), or came back: the
    /// changes applied meanwhile.
    pub(super) fn scoped_in_flight(&mut self, on: bool) -> Vec<StaleEvent> {
        let events = self.scoped.in_flight.take().unwrap_or_default();
        if on {
            self.scoped.in_flight = Some(Vec::new());
        }
        events
    }

    /// Phase 13: watch `k` (a check back from the worker) against the
    /// changes applied while it ran — the same test as [`stale_checks`],
    /// for it alone.
    ///
    /// [`stale_checks`]: NetHost::stale_checks
    pub(super) fn stale_one(&mut self, k: usize, events: &[StaleEvent]) {
        if k >= self.scoped.watches.len() || events.is_empty() {
            return;
        }
        let others: Vec<Watch> = {
            let mine = self.scoped.watches.remove(k);
            let others = std::mem::take(&mut self.scoped.watches);
            self.scoped.watches.push(mine);
            others
        };
        let kept = self.scoped.in_flight.take();
        for e in events {
            if self.scoped.watches.is_empty() {
                break;
            }
            self.stale_checks(&e.code, &e.user, &e.objects, &e.tiles, e.at_large);
        }
        self.scoped.in_flight = kept;
        let mine = std::mem::take(&mut self.scoped.watches);
        self.scoped.watches = others;
        self.scoped.watches.extend(mine);
    }

    /// Phase 13: the wire library of `code` made already, at its
    /// library revision.
    pub(super) fn scoped_library(&self, code: &str) -> Option<&Arc<ohl_cables::Library>> {
        let rev = self.models.get(code)?.libs_rev;
        match self.scoped.libraries.get(code) {
            Some((r, l)) if *r == rev => Some(l),
            _ => None,
        }
    }

    /// Phase 13: a wire library the worker made, kept (if still of the
    /// model's library revision).
    pub(super) fn keep_scoped_library(
        &mut self,
        code: &str,
        rev: u64,
        l: Arc<ohl_cables::Library>,
    ) {
        if self.models.get(code).is_some_and(|m| m.libs_rev == rev) {
            self.scoped.libraries.insert(code.to_owned(), (rev, l));
        }
    }

    /// Phase 13: a check's timings.
    pub(super) fn scoped_ms(&mut self, name: &str) -> &mut Vec<f64> {
        self.scoped.ms.entry(name.to_owned()).or_default()
    }

    /// Whether `text` names something `rights` may not see: an object, or
    /// a line it sees nothing of.
    fn names_hidden(&self, rights: &UserRights, text: &str) -> bool {
        if rights.sees_everything() {
            return false;
        }
        if self.sanitise(rights, text) != text {
            return true;
        }
        ohl_session::rights::words(text)
            .any(|w| self.models.contains_key(w) && !rights.sees_some_of_line(w))
    }

    fn see(&mut self, rights: &UserRights, n: &Neighbour) -> bool {
        rights.sees_everything() || self.visible(rights, &n.model, kind_of(n.kind), &n.id)
    }

    /// Watches `raw`'s result for `client`: what it used, the tiles its
    /// reach covers. Returns its place among the watches.
    pub(super) fn watch(
        &mut self,
        client: u64,
        id: u64,
        name: &str,
        ws: &WorkingSet,
        extent: &Extent,
        raw: &Raw,
    ) -> usize {
        let mut objects: BTreeSet<(String, Kind, String)> = extent
            .wanted
            .iter()
            .map(|(k, i)| (ws.model.clone(), *k, i.clone()))
            .collect();
        let mut models: BTreeSet<String> = BTreeSet::from([ws.model.clone()]);
        for n in &raw.pulled().read {
            objects.insert((n.model.clone(), kind_of(n.kind), n.id.clone()));
            models.insert(n.model.clone());
        }
        let grow = match raw.reach() {
            ohl_scope::Reach::Wires { metres } | ohl_scope::Reach::Lines { metres } => {
                metres + 2.0 * HostNet::new(self).max_margin()
            }
            _ => ohl_scope::reach::TWIN_TOLERANCE,
        };
        let bounds = extent
            .summary
            .bounds
            .map(|b| Bounds::new([b[0], b[1]], [b[2], b[3]]).grown(grow));
        let tiles = bounds
            .map(|b| self.tiles_of(&b).into_iter().collect())
            .unwrap_or_default();
        // A client keeps its last results watched (at most 32).
        let mine = self
            .scoped
            .watches
            .iter()
            .filter(|w| w.client == client)
            .count();
        if mine >= 32
            && let Some(k) = self.scoped.watches.iter().position(|w| w.client == client)
        {
            self.scoped.watches.remove(k);
        }
        self.scoped
            .watches
            .retain(|w| !(w.client == client && w.id == id));
        self.scoped.watches.push(Watch {
            client,
            id,
            check: name.to_owned(),
            objects,
            tiles,
            models,
        });
        self.scoped.watches.len() - 1
    }

    /// A change of model `code` by `user` touching `objects` and `tiles`
    /// (`at_large`: the line whole or its libraries): every watched result
    /// it touches is stale — told once, naming what its user may see.
    pub(super) fn stale_checks(
        &mut self,
        code: &str,
        user: &str,
        objects: &[(Kind, String)],
        tiles: &BTreeSet<TileKey>,
        at_large: bool,
    ) {
        if let Some(events) = &mut self.scoped.in_flight {
            events.push(StaleEvent {
                code: code.to_owned(),
                user: user.to_owned(),
                objects: objects.to_vec(),
                tiles: tiles.clone(),
                at_large,
            });
        }
        if self.scoped.watches.is_empty() {
            return;
        }
        let mut hit: Vec<(usize, Option<(Kind, String)>)> = Vec::new();
        for (k, w) in self.scoped.watches.iter().enumerate() {
            let named = objects
                .iter()
                .find(|(kind, id)| w.objects.contains(&(code.to_owned(), *kind, id.clone())))
                .cloned();
            if named.is_some()
                || (at_large && w.models.contains(code))
                || tiles.iter().any(|t| w.tiles.contains(t))
            {
                hit.push((k, named.or_else(|| objects.first().cloned())));
            }
        }
        if hit.is_empty() {
            return;
        }
        let mut gone = Vec::new();
        for (k, what) in hit {
            let (client, id, check) = {
                let w = &self.scoped.watches[k];
                (w.client, w.id, w.check.clone())
            };
            gone.push(k);
            let Some(rights) = self.client_rights(client) else {
                continue;
            };
            let named = what.filter(|(kind, i)| self.visible(&rights, code, *kind, i));
            let reason = match named {
                Some((kind, i)) => format!(
                    "{check}: line {code}: {} {i} changed (by {user}) — check again",
                    kind.as_str()
                ),
                None => format!(
                    "{check}: something within its reach you may not see changed — check again"
                ),
            };
            self.push(
                client,
                None,
                None,
                Some(ServerMsg::CheckStale { id, reason }),
            );
        }
        gone.sort_unstable();
        for k in gone.into_iter().rev() {
            self.scoped.watches.remove(k);
        }
    }

    /// The network's largest span margin (as a scoped check's candidate
    /// search takes it; phase 12: an offline working set's checks take
    /// the same).
    pub(super) fn network_max_margin(&mut self) -> f64 {
        HostNet::new(self).max_margin()
    }

    /// A client gone: its results no longer watched.
    pub(super) fn forget_checks(&mut self, client: u64) {
        self.scoped.watches.retain(|w| w.client != client);
    }
}

impl HostNet<'_> {
    /// Whether structure `id` of `model` has a twin in another line.
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
}

/// The server's reader of a report: a user's rights.
pub(super) struct HostViewer<'a> {
    pub(super) host: &'a mut NetHost,
    pub(super) rights: &'a UserRights,
}

impl crate::scoped_report::Viewer for HostViewer<'_> {
    fn see(&mut self, n: &Neighbour) -> bool {
        self.host.see(self.rights, n)
    }

    fn routes(&mut self, n: &Neighbour) -> Option<BTreeSet<String>> {
        let rs = self
            .host
            .models
            .get_mut(&n.model)?
            .session
            .object_routes(&[(kind_of(n.kind), n.id.clone())])
            .pop()
            .unwrap_or_default();
        Some(
            rs.into_iter()
                .filter(|r| self.host.visible(self.rights, &n.model, Kind::Route, r))
                .collect(),
        )
    }

    fn names_hidden(&self, text: &str) -> bool {
        self.host.names_hidden(self.rights, text)
    }
}
