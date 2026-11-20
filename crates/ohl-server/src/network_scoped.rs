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
use ohl_scope::clearance::{ClearanceResult, Outcome, WireId};
use ohl_scope::fields::{FieldResult, FieldSpec};
use ohl_scope::loads::LoadsResult;
use ohl_scope::{Cases, Neighbour, NeighbourKind, Neighbourhood, PulledIn, SpanChord, WorkingSet};
use ohl_session::Kind;
use ohl_session::rights::UserRights;
use ohl_session::working_set::{Extent, WorkingSetSpec};

use super::{Model, NetHost, keep};
use crate::{Finding, PulledReport, ScopedCheck, ScopedReport, ServerMsg, TileKey};

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
}

impl HostNet<'_> {
    fn objects_in(&self, area: &Bounds, kind: Kind) -> Vec<(String, String)> {
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
        let mut cases = ohl_scope::default_cases()?;
        let weather = m
            .session
            .library_files("weather")
            .map_err(|e| e.to_string())?;
        if !weather.is_empty() {
            cases.weather.clear();
            for (name, text) in weather {
                let w = ohl_weather::WeatherCases::parse(&text)
                    .map_err(|e| format!("weather file {name}: {e}"))?;
                cases.weather.extend(w.cases);
            }
            cases.source = format!("line {model}'s own library");
        }
        let loads = m
            .session
            .library_files("loadcases")
            .map_err(|e| e.to_string())?;
        if !loads.is_empty() {
            cases.loads.clear();
            for (name, text) in loads {
                let l = ohl_loads::LoadCases::parse(&text)
                    .map_err(|e| format!("load case file {name}: {e}"))?;
                cases.loads.extend(l.cases);
            }
            cases.source = format!("line {model}'s own library");
        }
        Ok(Arc::new(cases))
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

/// What a check computed, before the rights' filter.
enum Raw {
    Clearances(Box<ClearanceResult>),
    Fields(Box<FieldResult>),
    Loads(Box<LoadsResult>),
}

impl Raw {
    fn pulled(&self) -> &PulledIn {
        match self {
            Raw::Clearances(r) => &r.pulled,
            Raw::Fields(r) => &r.pulled,
            Raw::Loads(r) => &r.pulled,
        }
    }

    fn reach(&self) -> ohl_scope::Reach {
        match self {
            Raw::Clearances(r) => r.reach,
            Raw::Fields(r) => r.reach,
            Raw::Loads(r) => r.reach(),
        }
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
        let mut ws = WorkingSet {
            model: model.clone(),
            sections: of(Kind::Section),
            jumpers: of(Kind::Jumper),
            structures: of(Kind::Structure),
        };
        let name = check.name().to_owned();
        let raw = {
            let mut net = HostNet { host: self };
            match &check {
                ScopedCheck::Clearances { case, rules } => {
                    let rules = ohl_clearance::RuleSet::parse(rules)?.rules;
                    let case = find_case(&mut net, &model, case)?;
                    Raw::Clearances(Box::new(ohl_scope::clearance::clearances(
                        &mut net, &ws, &rules, &case,
                    )?))
                }
                ScopedCheck::Fields {
                    case,
                    distance,
                    amps,
                    limit_ut,
                } => {
                    let case = find_case(&mut net, &model, case)?;
                    let spec = FieldSpec {
                        distance: distance.unwrap_or(ohl_scope::reach::FIELD_DISTANCE),
                        amps: *amps,
                        limit_ut: *limit_ut,
                        ..FieldSpec::default()
                    };
                    Raw::Fields(Box::new(ohl_scope::fields::fields(
                        &mut net, &ws, &case, &spec,
                    )?))
                }
                ScopedCheck::StructureLoads { structures } => {
                    if structures.is_empty() {
                        let all = std::mem::take(&mut ws.structures);
                        ws.structures = all
                            .into_iter()
                            .filter(|s| net.has_twin(&model, s))
                            .collect();
                    } else {
                        for s in structures {
                            if !ws.structures.contains(s) {
                                return Err(format!("structure {s} is not in the working set"));
                            }
                        }
                        ws.structures = structures.clone();
                    }
                    Raw::Loads(Box::new(ohl_scope::loads::shared_tower_loads(
                        &mut net,
                        &ws,
                        &ohl_loads::default_budget(),
                    )?))
                }
            }
        };
        let seconds = started.elapsed().as_secs_f64();
        keep(
            self.scoped.ms.entry(name.clone()).or_default(),
            seconds * 1e3,
        );
        let report = self.report(rights, &ws, &name, &raw, seconds);
        self.watch(client, id, &name, &ws, &extent, &raw);
        Ok(ServerMsg::CheckResult {
            id,
            result: Ok(Box::new(report)),
        })
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

    /// The report of `raw` as `rights` may see it.
    fn report(
        &mut self,
        rights: &UserRights,
        ws: &WorkingSet,
        name: &str,
        raw: &Raw,
        seconds: f64,
    ) -> ScopedReport {
        let pulled = raw.pulled();
        let mut report = PulledReport {
            read: pulled.read.len(),
            ..PulledReport::default()
        };
        let mut lines = BTreeSet::new();
        let mut routes = BTreeSet::new();
        for n in &pulled.used {
            if self.see(rights, n) {
                lines.insert(n.model.clone());
                let (ok, rs) = match self.models.get_mut(&n.model) {
                    Some(m) => (
                        true,
                        m.session
                            .object_routes(&[(kind_of(n.kind), n.id.clone())])
                            .pop()
                            .unwrap_or_default(),
                    ),
                    None => (false, BTreeSet::new()),
                };
                if ok {
                    for r in rs {
                        if self.visible(rights, &n.model, Kind::Route, &r) {
                            routes.insert(format!("line {}: route {r}", n.model));
                        }
                    }
                }
                report
                    .objects
                    .push(format!("line {}: {} {}", n.model, n.kind.word(), n.id));
            } else {
                report.hidden += 1;
            }
        }
        report.lines = lines.into_iter().collect();
        report.routes = routes.into_iter().collect();
        let mut findings = Vec::new();
        let (mut results, mut violations) = (0, 0);
        let mut not_all: Vec<String> = Vec::new();
        let mut notes: Vec<String> = Vec::new();
        match raw {
            Raw::Clearances(r) => {
                not_all.extend(r.not_all_checked.iter().cloned());
                notes.extend(r.notes.iter().cloned());
                for rule in &r.rules {
                    let mut lowest: BTreeMap<&'static str, Finding> = BTreeMap::new();
                    let mut push = |f: Finding, findings: &mut Vec<Finding>| {
                        if f.violates {
                            findings.push(f.clone());
                        }
                        let k = if f.measure == "radial" {
                            "radial"
                        } else {
                            "vertical"
                        };
                        if lowest.get(k).is_none_or(|l| f.value < l.value) {
                            lowest.insert(k, f);
                        }
                    };
                    match &rule.outcome {
                        Outcome::NotChecked(why) => {
                            not_all.push(format!("rule {}: {why}", rule.name));
                        }
                        Outcome::Wires(w) => {
                            for (a, x) in w {
                                results += 1;
                                violations += usize::from(x.violates());
                                for f in self.wire_findings(rights, &rule.name, a, None, x) {
                                    push(f, &mut findings);
                                }
                            }
                        }
                        Outcome::Pairs(p) => {
                            for (a, b, x) in p {
                                results += 1;
                                violations += usize::from(x.violates());
                                for f in self.wire_findings(rights, &rule.name, a, Some(b), x) {
                                    push(f, &mut findings);
                                }
                            }
                        }
                    }
                    for (_, f) in lowest {
                        if !f.violates {
                            findings.push(Finding {
                                what: format!("{} (lowest)", f.what),
                                ..f
                            });
                        }
                    }
                }
            }
            Raw::Fields(r) => {
                not_all.extend(r.not_all_checked.iter().cloned());
                notes.extend(r.notes.iter().cloned());
                for p in &r.profiles {
                    results += p.points.len();
                    let top = p.points.iter().max_by(|a, b| a.b_ut.total_cmp(&b.b_ut));
                    if let Some(t) = top {
                        findings.push(Finding {
                            what: "magnetic field".into(),
                            on: format!("span {}–{}", p.from, p.to),
                            against: None,
                            measure: "B (µT)".into(),
                            value: t.b_ut,
                            required: r.limit_ut,
                            violates: r.limit_ut.is_some_and(|l| t.b_ut >= l),
                            at: Some(t.at),
                            hidden: false,
                        });
                    }
                }
                violations = r.above_limit;
                if report.hidden > 0 {
                    notes.push(format!(
                        "the field of {} object(s) you may not see is in the values (not named)",
                        report.hidden
                    ));
                }
            }
            Raw::Loads(r) => {
                not_all.extend(r.not_all_checked.iter().cloned());
                for t in &r.towers {
                    let seen = t.twins.iter().filter(|n| self.see(rights, n)).count();
                    notes.push(format!(
                        "structure {}: {} twin(s) of other lines on it{}",
                        t.structure,
                        t.twins.len(),
                        if seen < t.twins.len() {
                            format!(
                                ", {} you may not see (their sections in the loads, not named)",
                                t.twins.len() - seen
                            )
                        } else {
                            String::new()
                        }
                    ));
                    notes.extend(t.notes.iter().cloned());
                    for c in &t.cases {
                        results += c.points.len();
                    }
                    let measures = [
                        "factored transverse (N)",
                        "factored longitudinal (N)",
                        "factored vertical (N)",
                    ];
                    for w in &t.worst {
                        for (k, measure) in measures.iter().enumerate() {
                            let (line, case) = &w.governing[k];
                            let own = *line == ws.model;
                            let line_seen = own
                                || t.twins
                                    .iter()
                                    .filter(|n| &n.model == line)
                                    .any(|n| self.see(rights, n));
                            findings.push(Finding {
                                what: "structure loads (worst)".into(),
                                on: format!(
                                    "structure {} point {}",
                                    t.structure,
                                    if w.point.is_empty() {
                                        "(body)"
                                    } else {
                                        &w.point
                                    }
                                ),
                                against: Some(if own {
                                    format!("case {case}")
                                } else if line_seen {
                                    format!("line {line}'s case {case}")
                                } else {
                                    "a case of a line you may not see".into()
                                }),
                                measure: (*measure).into(),
                                value: w.factored[k],
                                required: None,
                                violates: false,
                                at: None,
                                hidden: !line_seen,
                            });
                        }
                    }
                }
            }
        }
        // Texts naming what the user may not see: replaced.
        let not_all: Vec<String> = not_all
            .into_iter()
            .map(|t| {
                if self.names_hidden(rights, &t) {
                    "a neighbour you may not see could not be read or solved: not all checked"
                        .to_owned()
                } else {
                    t
                }
            })
            .collect();
        let mut notes: Vec<String> = notes
            .into_iter()
            .map(|t| {
                if !self.names_hidden(rights, &t) {
                    t
                } else if t.starts_with("NOT ON THE SAFE SIDE") {
                    "NOT ON THE SAFE SIDE: about a neighbour you may not see (not named)".into()
                } else {
                    "a note about a neighbour you may not see (not named)".into()
                }
            })
            .collect();
        notes.dedup();
        let mut not_all = not_all;
        not_all.dedup();
        ScopedReport {
            check: name.to_owned(),
            model: ws.model.clone(),
            reach: raw.reach().describe(),
            status: if not_all.is_empty() {
                "every part checked".into()
            } else {
                "NOT ALL CHECKED".into()
            },
            findings,
            results,
            violations,
            pulled: report,
            not_all_checked: not_all,
            notes,
            seconds,
        }
    }

    /// One clearance result as findings (radial, vertical), its target
    /// named and placed only when the user may see it.
    fn wire_findings(
        &mut self,
        rights: &UserRights,
        rule: &str,
        a: &WireId,
        b: Option<&WireId>,
        x: &ohl_clearance::WireResult,
    ) -> Vec<Finding> {
        let seen = b.is_none_or(|b| self.see(rights, &b.neighbour()));
        let against = b.map(|b| {
            if !seen {
                if b.model == a.model {
                    "a hidden object of this line".to_owned()
                } else {
                    "a hidden object of another line".to_owned()
                }
            } else if b.model == a.model {
                b.describe()
            } else {
                format!("line {}: {}", b.model, b.describe())
            }
        });
        let mut out = Vec::new();
        for (measure, hit, required, count) in x.measures() {
            let Some(h) = hit else { continue };
            let violates = required.is_some() && count > 0;
            out.push(Finding {
                what: rule.to_owned(),
                on: if !seen && violates {
                    format!(
                        "{} — check result limited by an object you may not see",
                        a.describe()
                    )
                } else {
                    a.describe()
                },
                against: against.clone(),
                measure: measure.to_owned(),
                value: h.distance,
                required,
                violates,
                at: seen.then_some(h.wire),
                hidden: !seen,
            });
        }
        out
    }

    /// Watches `raw`'s result for `client`: what it used, the tiles its
    /// reach covers.
    fn watch(
        &mut self,
        client: u64,
        id: u64,
        name: &str,
        ws: &WorkingSet,
        extent: &Extent,
        raw: &Raw,
    ) {
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
                metres + 2.0 * HostNet { host: self }.max_margin()
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

fn find_case(
    net: &mut HostNet<'_>,
    model: &str,
    name: &str,
) -> Result<ohl_weather::WeatherCase, String> {
    let cases = net.cases(model)?;
    cases
        .weather
        .iter()
        .find(|c| c.name == name)
        .cloned()
        .ok_or_else(|| {
            format!(
                "no weather case \"{name}\" on line {model} (its cases: {})",
                cases
                    .weather
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}
