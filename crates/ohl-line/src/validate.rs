// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The validation of a line: whole ([`Line::validate`]) or local
//! ([`Line::validate_local`]: the objects an edit changed and those whose
//! checks read them, found by the reference index [`LineIndex`]).
//!
//! Both run the same checks in the same order, so the first problem met
//! is the same: the local validation of an edit of a valid line gives
//! exactly what the whole validation of the edited line gives (tested on
//! random edits, `validate_tests`). Every check and its reach:
//!
//! | Check | Reads | Local reach |
//! |---|---|---|
//! | Every number finite | the object | the objects changed |
//! | Settings: bimetallic model, creep-stretch, electrical settings, structure groups, file format, heights' datum, display, defaults, circuits, systems | the settings | none: a change of the settings is validated whole (the session's settings rows) |
//! | Structure: soil and earthing resistance, load cases, foot movements, finite place, its strings and points | the structure | the structures changed |
//! | Ids given and unique (every kind) | the id index | the objects changed (a lookup in the index) |
//! | Routes of a version 2 file upgraded | the routes | the routes changed |
//! | One system phase per conductor path | the sections of the path (joined by shared wire points and jumpers, across routes) | every path holding a section changed, a section at a changed section's wire point, or a changed jumper's end section — each path whole |
//! | Alignment point: finite, tied to a structure that is there, within 1 mm | the point, the structure | the points changed and those tied to a structure changed |
//! | Alignment: its route and points there, no point twice | the alignment, the ids | the alignments changed, through a point changed or of a route changed |
//! | Structure: its routes there | the structure, the route ids | the structures changed and those giving a route changed |
//! | Section: its structures and wire points there, stringing, local wind and ice, span points inside their spans, spreads, colour, bundle | the section, its structures (a span point by station: the routes and every structure) | the sections changed, through a structure changed; with a structure, an alignment, a point or a route changed, every section with a span point by station |
//! | Jumper: its ends there, its bundle (from the first section at its end, or the tap's), taps, points, length, sag or tension, support clamps | the jumper, its structures, the sections at its ends, the section or jumper it taps | the jumpers changed, at a structure changed, at a wire point of a section changed, tapping a section or jumper changed, and the jumpers tapping those (all the way) |
//! | Circuit starts: section points, each once | the starts, the section points | every start, when a section changed |
//! | Spacer: its sections, span points with spacer clamps, finite elements, one span, its values | the spacer, its sections | the spacers changed and those on a section changed |
//! | No ring of jumpers hanging on each other | every jumper's taps | whole, when a jumper changed taps another jumper |
//!
//! Engineering checks (clearances, crossings, loads) are not part of the
//! validation: they are the check windows' (phase 10 of the country
//! server plan scopes them).

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::index::{LineIndex, Of, Target};
use crate::*;

/// What a local validation checks ([`Line::validate_local`]): objects by
/// kind and id, the conductor paths through some sections, and the checks
/// across the line it needs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scope {
    objects: [BTreeSet<String>; 7],
    /// The conductor paths through these sections (each whole).
    phases: BTreeSet<String>,
    /// Every circuit start.
    starts: bool,
    /// The jumpers' order (no ring of taps), whole.
    jumper_order: bool,
}

impl Scope {
    fn add(&mut self, of: Of, id: &str) -> bool {
        self.objects[of as usize].insert(id.to_owned())
    }

    /// The ids of kind `of` it checks.
    pub fn objects(&self, of: Of) -> &BTreeSet<String> {
        &self.objects[of as usize]
    }

    /// How many objects it checks.
    pub fn len(&self) -> usize {
        self.objects.iter().map(BTreeSet::len).sum()
    }

    /// Whether it checks nothing.
    pub fn is_empty(&self) -> bool {
        self.len() == 0 && self.phases.is_empty() && !self.starts && !self.jumper_order
    }

    /// Adds what `other` checks.
    pub fn merge(&mut self, other: Scope) {
        for (a, b) in self.objects.iter_mut().zip(other.objects) {
            a.extend(b);
        }
        self.phases.extend(other.phases);
        self.starts |= other.starts;
        self.jumper_order |= other.jumper_order;
    }

    /// What validating a change of the objects `changed` (kind, id) needs,
    /// read in `line` with its index `index`: call it on the line before
    /// the change and on the line after it, and [`Scope::merge`] the two
    /// (an object's referrers before and after). The line's settings must
    /// be the same before and after (a change of them is validated whole).
    pub fn of_change(line: &Line, index: &LineIndex, changed: &[(Of, String)]) -> Scope {
        let mut s = Scope::default();
        // Span points by station depend on the routes and every structure.
        let mut geometry = false;
        let object = |of: Of, id: &str| Target::Object(of, id.to_owned());
        for (of, id) in changed {
            s.add(*of, id);
            match of {
                Of::Structure => {
                    geometry = true;
                    // Sections, jumpers (ends, support clamps), tied points.
                    for (k, r) in index.referrers(&object(Of::Structure, id)) {
                        s.add(*k, r);
                    }
                }
                Of::Section => {
                    s.starts = true;
                    s.phases.insert(id.clone());
                    // Jumpers tapping it, spacers on it.
                    for (k, r) in index.referrers(&object(Of::Section, id)) {
                        s.add(*k, r);
                    }
                    // At its wire points: the jumpers (their section, the
                    // first there, may change) and the sections (their
                    // conductor path).
                    if let Some(sec) = index.section(line, id) {
                        for p in &sec.points {
                            for (k, r) in
                                index.referrers(&Target::Point(p.structure.clone(), p.set))
                            {
                                match k {
                                    Of::Section => {
                                        s.phases.insert(r.clone());
                                    }
                                    _ => {
                                        s.add(*k, r);
                                    }
                                }
                            }
                        }
                    }
                }
                Of::Jumper => {
                    for (k, r) in index.referrers(&object(Of::Jumper, id)) {
                        s.add(*k, r);
                    }
                    if let Some(j) = index.jumper(line, id) {
                        if j.taps().any(|t| matches!(t, Tap::Jumper { .. })) {
                            s.jumper_order = true;
                        }
                        // The conductor paths at its ends.
                        for end in [&j.from, &j.to] {
                            match end {
                                JumperEnd::Wire(p) => {
                                    s.phases.extend(
                                        index.at_point(&p.structure, p.set, Of::Section).cloned(),
                                    );
                                }
                                JumperEnd::Tap { tap } => {
                                    if let Some(sec) = index.tap_section(line, tap, 0) {
                                        s.phases.insert(sec.id.clone());
                                    }
                                }
                            }
                        }
                    }
                }
                Of::Spacer => {}
                Of::AlignmentPoint | Of::Route => {
                    geometry = true;
                    // Alignments through the point, of the route;
                    // structures giving the route.
                    for (k, r) in index.referrers(&object(*of, id)) {
                        s.add(*k, r);
                    }
                }
                Of::Alignment => geometry = true,
            }
        }
        if geometry {
            for id in index.station_sections() {
                s.add(Of::Section, id);
            }
        }
        // The jumpers tapping a jumper checked (their section is found
        // through it), all the way.
        let mut queue: Vec<String> = s.objects(Of::Jumper).iter().cloned().collect();
        while let Some(j) = queue.pop() {
            let tapping: Vec<String> = index
                .referrers_of(Of::Jumper, &j, Of::Jumper)
                .cloned()
                .collect();
            for r in tapping {
                if s.add(Of::Jumper, &r) {
                    queue.push(r);
                }
            }
        }
        s
    }
}

/// The places of kind `of` to check: every one, or those of `scope`, in
/// the line's order.
fn places(line: &Line, index: &LineIndex, scope: Option<&Scope>, of: Of) -> Vec<usize> {
    match scope {
        None => (0..crate::index::count(line, of)).collect(),
        Some(s) => {
            let mut v: Vec<usize> = s
                .objects(of)
                .iter()
                .filter_map(|id| index.position(of, id))
                .collect();
            v.sort_unstable();
            v.dedup();
            v
        }
    }
}

impl Line {
    /// Checks references and basic consistency (the whole line; see the
    /// module [`crate::validate`] for every check). Cost: one pass over the
    /// line to index it, then one per check.
    pub fn validate(&self) -> Result<(), LineError> {
        let index = LineIndex::of(self);
        self.validate_with(&index, None)
    }

    /// Checks what `scope` names ([`Scope::of_change`]) in this line,
    /// indexed by `index` (up to date): for an edit of a line that was
    /// valid, with its settings unchanged, exactly what [`Line::validate`]
    /// gives (the same first problem, or none). Cost: what the scope
    /// holds, not the size of the line.
    pub fn validate_local(&self, index: &LineIndex, scope: &Scope) -> Result<(), LineError> {
        self.validate_with(index, Some(scope))
    }

    /// The conductor paths through the sections `seeds` (each whole: the
    /// sections joined by shared wire points and by jumpers, whatever
    /// route they are on), checked in the order the whole validation
    /// checks them ([`Line::section_phases`]: by their first section in
    /// the line).
    fn phases_local(&self, index: &LineIndex, seeds: &BTreeSet<String>) -> Result<(), LineError> {
        let mut done: HashSet<usize> = HashSet::new();
        let mut paths: Vec<Vec<usize>> = Vec::new();
        for seed in seeds {
            let Some(k) = index.position(Of::Section, seed) else {
                continue;
            };
            if done.contains(&k) {
                continue;
            }
            let path = self.conductor_path(index, k);
            done.extend(path.iter().copied());
            paths.push(path.into_iter().collect());
        }
        paths.sort();
        for path in paths {
            self.path_phase(
                path.iter()
                    .map(|&k| (self.sections[k].id.as_str(), &self.sections[k])),
            )?;
        }
        Ok(())
    }

    /// The conductor path through section `start` (its place): the places
    /// of the sections joined to it, as [`electrical::conductor_paths`]
    /// joins them (sections at one wire point; a jumper's two ends, each
    /// the first section at its wire point or the tap's section).
    fn conductor_path(&self, index: &LineIndex, start: usize) -> BTreeSet<usize> {
        let mut path = BTreeSet::from([start]);
        let mut queue = vec![start];
        let mut seen_jumpers: HashSet<String> = HashSet::new();
        let mut jumpers: Vec<String> = Vec::new();
        let place = |s: &Section| index.position(Of::Section, &s.id);
        while let Some(k) = queue.pop() {
            let sec = &self.sections[k];
            for p in &sec.points {
                for (of, id) in index.referrers(&Target::Point(p.structure.clone(), p.set)) {
                    match of {
                        Of::Section => {
                            if let Some(x) = index.position(Of::Section, id)
                                && path.insert(x)
                            {
                                queue.push(x);
                            }
                        }
                        Of::Jumper => jumpers.push(id.clone()),
                        _ => {}
                    }
                }
            }
            jumpers.extend(
                index
                    .referrers_of(Of::Section, &sec.id, Of::Jumper)
                    .cloned(),
            );
            // Each jumper found joins its two ends; those tapping it are
            // found through it.
            while let Some(j) = jumpers.pop() {
                if !seen_jumpers.insert(j.clone()) {
                    continue;
                }
                jumpers.extend(index.referrers_of(Of::Jumper, &j, Of::Jumper).cloned());
                let Some(jumper) = index.jumper(self, &j) else {
                    continue;
                };
                let end = |e: &JumperEnd| match e {
                    JumperEnd::Wire(p) => index.first_section_at(self, &p.structure, p.set),
                    JumperEnd::Tap { tap } => index.tap_section(self, tap, 0),
                };
                if let (Some(a), Some(b)) = (end(&jumper.from), end(&jumper.to)) {
                    for x in [place(a), place(b)].into_iter().flatten() {
                        if path.insert(x) {
                            queue.push(x);
                        }
                    }
                }
            }
        }
        path
    }

    /// [`Line::inside_span`] by the index (the same answer).
    fn inside_span_in(&self, index: &LineIndex, section: &Section, p: &SpanPoint) -> bool {
        let Some(w) = section.points.get(p.span..=p.span + 1) else {
            return false;
        };
        match p.at {
            SpanPlace::Station(st) => self
                .span_stations(&w[0].structure, &w[1].structure)
                .is_none_or(|(a, b)| a.min(b) < st && st < a.max(b)),
            SpanPlace::Distance(d) => {
                match (index.wire_point(self, &w[0]), index.wire_point(self, &w[1])) {
                    (Some(a), Some(b)) => {
                        let (a, b) = (a.position, b.position);
                        d < (b[0] - a[0]).hypot(b[1] - a[1])
                    }
                    _ => true,
                }
            }
            SpanPlace::Fraction(_) => true,
        }
    }

    /// [`Line::jumper_bundle`] by the index (the same bundle).
    pub(crate) fn jumper_bundle_in(&self, index: &LineIndex, jumper: &Jumper) -> Bundle {
        let base = index
            .jumper_section(self, jumper)
            .map_or(Bundle::SINGLE, Section::bundle_geometry);
        let count = jumper.bundle.unwrap_or(base.count).max(1);
        Bundle {
            count,
            spacing: jumper
                .bundle_spacing
                .or(base.spacing)
                .filter(|_| count >= 2),
            rotation_deg: jumper.bundle_rotation_deg.or(base.rotation_deg),
        }
    }

    /// The validation by the index: of everything (`scope` none), or of
    /// the objects and checks `scope` names, in the same order — the
    /// first problem met is the one the whole validation meets first.
    pub(crate) fn validate_with(
        &self,
        index: &LineIndex,
        scope: Option<&Scope>,
    ) -> Result<(), LineError> {
        let whole = scope.is_none();
        let places = |of: Of| places(self, index, scope, of);
        // Every number finite (a NaN or an infinity stored as JSON would no
        // longer read).
        match scope {
            None => finite::check(self).map_err(LineError::Invalid)?,
            Some(_) => {
                for of in Of::ALL {
                    for k in places(of) {
                        let at = format!("{}[{k}]", of.field());
                        let checked = match of {
                            Of::Route => finite::check_at(&self.routes[k], &at),
                            Of::Alignment => finite::check_at(&self.alignments[k], &at),
                            Of::AlignmentPoint => finite::check_at(&self.alignment_points[k], &at),
                            Of::Structure => finite::check_at(&self.structures[k], &at),
                            Of::Section => finite::check_at(&self.sections[k], &at),
                            Of::Jumper => finite::check_at(&self.jumpers[k], &at),
                            Of::Spacer => finite::check_at(&self.spacers[k], &at),
                        };
                        checked.map_err(LineError::Invalid)?;
                    }
                }
            }
        }
        // The structures by id (the first of an id, as `Line::structure`
        // finds it): looked up for every section point.
        let by_id = |id: &str| index.structure(self, id);
        let structure_places = places(Of::Structure);
        let structures = || structure_places.iter().map(|&k| &self.structures[k]);
        let invalid = |m: String| Err(LineError::Invalid(m));
        if whole {
            self.bimetallic.validate().map_err(LineError::Invalid)?;
            if let Some(e) = self.creep_stretch.hot_problem() {
                return Err(LineError::Invalid(e));
            }
            self.electrical.validate().map_err(LineError::Invalid)?;
        }
        if let Some(s) = structures().find(|s| {
            s.soil_resistivity_ohm_m
                .is_some_and(|r| !(r.is_finite() && r > 0.0))
        }) {
            return invalid(format!(
                "structure {}: soil resistivity must be a number of Ω·m > 0",
                s.id
            ));
        }
        if let Some(s) = structures().find(|s| {
            s.earthing_resistance_ohm
                .is_some_and(|r| !(r.is_finite() && r > 0.0))
        }) {
            return invalid(format!(
                "structure {}: earthing resistance must be a number of Ω > 0",
                s.id
            ));
        }
        if let Some(s) = structures().find(|s| s.no_load_cases && !s.load_cases.is_empty()) {
            return invalid(format!(
                "structure {}: no_load_cases and load_cases ({}) contradict: give the list, or \
                 no_load_cases = true for a structure that takes none",
                s.id,
                s.load_cases.join(", ")
            ));
        }
        if whole && let Some(p) = self.groups_problem() {
            return invalid(p);
        }
        for s in structures() {
            if let Some(p) = s
                .foot_movements
                .as_ref()
                .and_then(feet::FootMovements::problem)
            {
                return invalid(format!("structure {}: {p}", s.id));
            }
        }
        if whole {
            self.check_settings()?;
        }
        for &k in &structure_places {
            let s = &self.structures[k];
            if index.position(Of::Structure, &s.id) != Some(k) {
                return invalid(format!("duplicate structure id {}", s.id));
            }
            let finite = |p: &Point| p.iter().all(|x| x.is_finite());
            if !finite(&s.position)
                || !s.orientation_deg.is_finite()
                || !s.height_adjust.is_finite()
            {
                return invalid(format!(
                    "structure {}: position, orientation or height adjustment not a number",
                    s.id
                ));
            }
            s.check_strings_as_stored().map_err(LineError::Invalid)?;
        }
        let legacy = if whole {
            self.routes.iter().any(Route::is_legacy)
        } else {
            places(Of::Route)
                .into_iter()
                .any(|k| self.routes[k].is_legacy())
        };
        if legacy || !self.alignment.is_empty() {
            return invalid(
                "routes of a version 2 file not upgraded (Line::upgrade_routes)".to_owned(),
            );
        }
        // Systems, and one system phase per conductor path.
        match scope {
            None => {
                self.section_phases()?;
            }
            Some(s) => self.phases_local(index, &s.phases)?,
        }
        for k in places(Of::Route) {
            let r = &self.routes[k];
            if r.id.trim().is_empty() || index.position(Of::Route, &r.id) != Some(k) {
                return invalid(format!("route id \"{}\": empty or given twice", r.id));
            }
        }
        let known = |r: &String| index.position(Of::Route, r).is_some();
        let points = places(Of::AlignmentPoint);
        for &k in &points {
            let p = &self.alignment_points[k];
            if p.id.trim().is_empty() || index.position(Of::AlignmentPoint, &p.id) != Some(k) {
                return invalid(format!(
                    "alignment point id \"{}\": empty or given twice",
                    p.id
                ));
            }
            if !p.position.iter().all(|x| x.is_finite()) {
                return invalid(format!("alignment point {}: not a number", p.id));
            }
        }
        self.check_alignment(index, points.iter().map(|&k| &self.alignment_points[k]))?;
        for k in places(Of::Alignment) {
            let a = &self.alignments[k];
            if a.id.trim().is_empty() || index.position(Of::Alignment, &a.id) != Some(k) {
                return invalid(format!("alignment id \"{}\": empty or given twice", a.id));
            }
            if a.legacy.is_some() {
                return invalid(format!(
                    "alignment {}: of a version 3 file, not upgraded (Line::upgrade_routes)",
                    a.id
                ));
            }
            if a.route.trim().is_empty() {
                return invalid(format!("alignment {}: belongs to no route", a.id));
            }
            if !known(&a.route) {
                return invalid(format!("alignment {}: unknown route {}", a.id, a.route));
            }
            let mut seen = BTreeMap::new();
            for p in &a.points {
                if index.position(Of::AlignmentPoint, p).is_none() {
                    return invalid(format!("alignment {}: unknown alignment point {p}", a.id));
                }
                if seen.insert(p.as_str(), ()).is_some() {
                    return invalid(format!("alignment {}: point {p} given twice", a.id));
                }
            }
        }
        if let Some((s, r)) =
            structures().find_map(|s| s.routes.iter().find(|r| !known(r)).map(|r| (s, r)))
        {
            return invalid(format!("structure {}: unknown route {r}", s.id));
        }
        for k in places(Of::Section) {
            let sec = &self.sections[k];
            if index.position(Of::Section, &sec.id) != Some(k) {
                return invalid(format!("duplicate section id {}", sec.id));
            }
            if sec.points.len() < 2 {
                return invalid(format!("section {}: needs at least two points", sec.id));
            }
            for p in &sec.points {
                let Some(structure) = by_id(&p.structure) else {
                    return invalid(format!(
                        "section {}: unknown structure {}",
                        sec.id, p.structure
                    ));
                };
                // Typed structures get their wire points from the library
                // (`resolve`), which validates again afterwards.
                if structure.structure_type.is_none() && structure.wire_at_set(p.set).is_none() {
                    return invalid(format!(
                        "section {}: no wire point with set {} on structure {}",
                        sec.id, p.set, p.structure
                    ));
                }
            }
            let spans = sec.points.len() - 1;
            check_stringing(&sec.stringing, spans)
                .map_err(|m| LineError::Invalid(format!("section {}: {m}", sec.id)))?;
            let place_ok = |p: &SpanPlace| match *p {
                SpanPlace::Station(s) => s.is_finite(),
                SpanPlace::Distance(d) => d.is_finite() && d > 0.0,
                SpanPlace::Fraction(f) => 0.0 < f && f < 1.0,
            };
            // A span's ends are places too (a spread object from or to
            // a clamp).
            let end_ok =
                |p: &SpanPlace| matches!(*p, SpanPlace::Fraction(f) if f == 0.0 || f == 1.0);
            let mut winds = std::collections::BTreeSet::new();
            for w in &sec.wind_factors {
                if w.span >= spans || !winds.insert(w.span) {
                    return invalid(format!(
                        "section {}: local wind of span {}: not one of its spans, or given twice",
                        sec.id,
                        w.span + 1
                    ));
                }
                if !(w.factor.is_finite() && (0.0..=MAX_WIND_FACTOR).contains(&w.factor)) {
                    return invalid(format!(
                        "section {}: local wind factor {} of span {} (0 to {MAX_WIND_FACTOR})",
                        sec.id,
                        w.factor,
                        w.span + 1
                    ));
                }
            }
            let mut ices = std::collections::BTreeSet::new();
            for w in &sec.ice_factors {
                if w.span >= spans || !ices.insert(w.span) {
                    return invalid(format!(
                        "section {}: local ice of span {}: not one of its spans, or given twice",
                        sec.id,
                        w.span + 1
                    ));
                }
                if !(w.factor.is_finite() && (0.0..=MAX_WIND_FACTOR).contains(&w.factor)) {
                    return invalid(format!(
                        "section {}: local ice factor {} of span {} (0 to {MAX_WIND_FACTOR})",
                        sec.id,
                        w.factor,
                        w.span + 1
                    ));
                }
            }
            let mut labels = std::collections::BTreeSet::new();
            for p in &sec.span_points {
                if p.label.trim().is_empty() || !labels.insert(p.label.as_str()) {
                    return invalid(format!(
                        "section {}: span point labels must be given and unique ({:?})",
                        sec.id, p.label
                    ));
                }
                if p.span >= spans
                    || !(place_ok(&p.at) || (p.object.is_none() && end_ok(&p.at)))
                    || !self.inside_span_in(index, sec, p)
                {
                    return invalid(format!(
                        "section {}: span point {} is not inside a span",
                        sec.id, p.label
                    ));
                }
                if let Some(o) = &p.object {
                    o.validate().map_err(|e| {
                        LineError::Invalid(format!(
                            "section {}: the object on {}: {e}",
                            sec.id, p.label
                        ))
                    })?;
                    if o.hold.is_some() {
                        return invalid(format!(
                            "section {}: the clamp on {} is held up: only a jumper's is (a support)",
                            sec.id, p.label
                        ));
                    }
                }
            }
            let mut spread_labels = std::collections::BTreeSet::new();
            for s in &sec.spreads {
                if !s.label.is_empty() && !spread_labels.insert(s.label.as_str()) {
                    return invalid(format!(
                        "section {}: spread label {:?} given twice",
                        sec.id, s.label
                    ));
                }
                let at = |l: &str| sec.span_points.iter().find(|p| p.label == l);
                let ok = match (at(&s.from), at(&s.to)) {
                    (Some(a), Some(b)) => a.span == b.span && s.from != s.to,
                    _ => false,
                };
                if !ok || !(s.load.is_finite() && s.load >= 0.0) {
                    return invalid(format!(
                        "section {}: spread {:?} needs two points of one span and a load ≥ 0",
                        sec.id, s.label
                    ));
                }
            }
            if let Some(c) = sec.color.as_deref().filter(|c| !is_hex_color(c)) {
                return invalid(format!("section {}: colour \"{c}\" is not #rrggbb", sec.id));
            }
            Bundle {
                count: sec.bundle,
                spacing: sec.bundle_spacing,
                rotation_deg: sec.bundle_rotation_deg,
            }
            .check(&format!("section {}", sec.id))
            .map_err(LineError::Invalid)?;
        }
        for k in places(Of::Jumper) {
            let j = &self.jumpers[k];
            let bad = |m: &str| invalid(format!("jumper {}: {m}", j.id));
            if index.position(Of::Jumper, &j.id) != Some(k) {
                return bad("duplicate id");
            }
            // Its own bundle values, over those it takes from its section.
            let own = Bundle {
                count: j
                    .bundle
                    .unwrap_or_else(|| self.jumper_bundle_in(index, j).count),
                spacing: j.bundle_spacing,
                rotation_deg: j.bundle_rotation_deg,
            };
            own.check(&format!("jumper {}", j.id))
                .map_err(LineError::Invalid)?;
            for end in [&j.from, &j.to] {
                let end = match end {
                    JumperEnd::Tap { tap } => {
                        self.check_tap(index, j, tap)?;
                        continue;
                    }
                    JumperEnd::Wire(p) => p,
                };
                let Some(structure) = by_id(&end.structure) else {
                    return bad(&format!("unknown structure {}", end.structure));
                };
                if structure.structure_type.is_none() && structure.wire_at_set(end.set).is_none() {
                    return bad(&format!(
                        "no wire point with set {} on structure {}",
                        end.set, end.structure
                    ));
                }
            }
            if j.from == j.to {
                return bad("both ends are the same wire point or tap");
            }
            if j.on_tap() && j.length.is_none() && !j.rigid {
                return bad(
                    "a jumper on a tap is given by its length (its sag would need a design case where the tap stays put)",
                );
            }
            let mut labels = std::collections::BTreeSet::new();
            for p in &j.points {
                if p.label.trim().is_empty() || !labels.insert(p.label.as_str()) {
                    return bad(&format!(
                        "point labels must be given and unique ({:?})",
                        p.label
                    ));
                }
                if !p.at.is_valid() {
                    return bad(&format!("point {} is not along it", p.label));
                }
                if let Some(o) = &p.object {
                    o.validate().map_err(|e| {
                        LineError::Invalid(format!(
                            "jumper {}: the object on {}: {e}",
                            j.id, p.label
                        ))
                    })?;
                    if o.has_force() {
                        return bad(&format!(
                            "the object on {} gives a constant force ({}): only a section's \
                             span point takes one",
                            p.label,
                            o.force_text()
                        ));
                    }
                    // Spacers join span points: nothing for a spacer
                    // clamp to hold on a jumper (owner, 2026-11-14).
                    if o.clamp_for == Some(ClampFor::Spacer) {
                        return bad(&format!(
                            "{} holds a spacer clamp: interphase spacers join span points \
                             only — a jumper point takes a tap clamp or a support clamp",
                            p.label
                        ));
                    }
                }
            }
            match (j.rigid, j.length, j.sag, j.tension) {
                (true, None, None, None) => {}
                (true, ..) => return bad("a rigid jumper has no length, sag or tension"),
                (false, Some(l), None, None) if l > 0.0 => {}
                (false, None, Some(s), None) if s > 0.0 => {}
                (false, None, None, Some(t)) if t > 0.0 && t.is_finite() => {
                    if j.on_tap() {
                        return bad(
                            "a jumper given by its tension (a riser) is dead-ended between two \
                             wire points, not on a tap",
                        );
                    }
                    if j.support().is_some() || !j.supports().is_empty() {
                        return bad(
                            "a jumper given by its tension (a riser) has no helping strings \
                             (post insulators along it are not modelled yet)",
                        );
                    }
                }
                _ => {
                    return bad(
                        "give either a positive length, a positive sag or a positive tension",
                    );
                }
            }
            if j.bending_stiffness
                .is_some_and(|e| !(e >= 0.0 && e.is_finite()))
            {
                return bad("bending stiffness must be ≥ 0");
            }
            if j.stiffness_factor.is_some_and(|f| !(f > 0.0 && f <= 1.0)) {
                return bad("the stiffness factor must be above 0 and at most 1");
            }
            if [j.angle_from_deg, j.angle_to_deg]
                .into_iter()
                .flatten()
                .any(|a| !(-90.0..=90.0).contains(&a))
            {
                return bad("clamp exit angles must be between −90° and 90°");
            }
            if let Some(support) = &j.support() {
                if j.length.is_none() {
                    return bad(
                        "a supported jumper needs a length (its sag depends on the support)",
                    );
                }
                if !(support.length > 0.0 && support.weight >= 0.0) {
                    return bad("support length must be > 0 and weight ≥ 0");
                }
                if !(0.0 < support.fraction && support.fraction < 1.0) {
                    return bad("support fraction must be between 0 and 1");
                }
            }
            self.check_clamps(index, j)
                .map_err(|m| LineError::Invalid(format!("jumper {}: {m}", j.id)))?;
            if let Some(c) = j.color.as_deref().filter(|c| !is_hex_color(c)) {
                return bad(&format!("colour \"{c}\" is not #rrggbb"));
            }
        }
        if whole {
            self.defaults.validate().map_err(LineError::Invalid)?;
            self.validate_circuits().map_err(LineError::Invalid)?;
        }
        // Circuit starts: section points, each once.
        let mut starts = std::collections::BTreeSet::new();
        let all_starts = whole || scope.is_some_and(|s| s.starts);
        for p in self.circuit_starts.iter().filter(|_| all_starts) {
            if !starts.insert((p.structure.as_str(), p.set)) {
                return invalid(format!(
                    "circuit start {} set {} given twice",
                    p.structure, p.set
                ));
            }
            if index
                .at_point(&p.structure, p.set, Of::Section)
                .next()
                .is_none()
            {
                return invalid(format!(
                    "circuit start {} set {} is no section's point",
                    p.structure, p.set
                ));
            }
        }
        for k in places(Of::Spacer) {
            let s = &self.spacers[k];
            let bad = |m: &str| invalid(format!("spacer {}: {m}", s.id));
            if s.id.trim().is_empty() || index.position(Of::Spacer, &s.id) != Some(k) {
                return invalid(format!("spacer ids must be given and unique ({:?})", s.id));
            }
            if s.a.section == s.b.section {
                return bad("its ends must be on two different sections");
            }
            let mut spans = Vec::new();
            for end in [&s.a, &s.b] {
                let Some(sec) = index.section(self, &end.section) else {
                    return bad(&format!("no section {}", end.section));
                };
                let Some(p) = sec.span_points.iter().find(|p| p.label == end.point) else {
                    return bad(&format!(
                        "no span point {} on section {}",
                        end.point, sec.id
                    ));
                };
                match p.object.as_ref() {
                    Some(o) if o.takes_spacer() => {}
                    Some(o) if o.kind == ObjectKind::Clamp => {
                        return bad(&format!(
                            "its end {} on section {} holds a tap clamp, which starts a                              jumper: a spacer ends on a spacer clamp",
                            end.point, sec.id
                        ));
                    }
                    _ => {
                        return bad(&format!(
                            "its end {} on section {} holds no clamp (put a clamp object there)",
                            end.point, sec.id
                        ));
                    }
                }
                if sec.calculation.method() != condition::Method::Fe {
                    return bad(&format!(
                        "section {} is calculated by ruling span: a spacer needs finite \
                         elements (its calculation: … FE)",
                        sec.id
                    ));
                }
                let span = &sec.points[p.span..=p.span + 1];
                let mut ends = [span[0].structure.clone(), span[1].structure.clone()];
                ends.sort();
                spans.push(ends);
            }
            if spans[0] != spans[1] {
                return bad("its ends must be in the same span (between the same two structures)");
            }
            let positive = |x: f64| x.is_finite() && x > 0.0;
            let at_least_0 = |x: f64| x.is_finite() && x >= 0.0;
            if !(at_least_0(s.weight) && at_least_0(s.wind_area))
                || s.stiffness.is_some_and(|k| !positive(k))
                || s.compression.is_some_and(|c| !at_least_0(c))
                || s.length.is_some_and(|l| !positive(l))
            {
                return bad("weight, wind area, compression ≥ 0; stiffness and length > 0");
            }
        }
        // Taps: no jumper hangs on itself, through other jumpers or not.
        if whole || scope.is_some_and(|s| s.jumper_order) {
            self.jumper_order()?;
        }
        Ok(())
    }

    /// The checks of the line's settings: the file's format and version,
    /// the heights' datum, the display.
    fn check_settings(&self) -> Result<(), LineError> {
        let invalid = |m: String| Err(LineError::Invalid(m));
        ohl_core::files::check_header(
            Some(&self.format),
            Some(self.version),
            LINE_FILE_FORMAT,
            LINE_FILE_VERSION,
            "line file",
        )
        .map_err(LineError::Invalid)?;
        if self
            .height_datum
            .as_ref()
            .is_some_and(HeightDatum::is_empty)
        {
            return invalid("height_datum: give its name, its EPSG code or both".into());
        }
        if !(0.0..360.0).contains(&self.display.ahead_deg) {
            return invalid(format!(
                "display: the angle ahead {} must be from 0 to 360°",
                self.display.ahead_deg
            ));
        }
        if let Some((label, c)) = self
            .display
            .phase_colors
            .iter()
            .find(|(_, c)| !is_hex_color(c))
        {
            return invalid(format!(
                "display: colour \"{c}\" of phase \"{label}\" is not #rrggbb"
            ));
        }
        Ok(())
    }

    /// Tap `tap` of jumper `j` holds: its point exists and holds a clamp;
    /// a span's section is solved by finite elements; another jumper.
    fn check_tap(&self, index: &LineIndex, j: &Jumper, tap: &Tap) -> Result<(), LineError> {
        let bad = |m: String| {
            Err(LineError::Invalid(format!(
                "jumper {}: tap {}: {m}",
                j.id,
                tap.text()
            )))
        };
        let object = match tap {
            Tap::Span(r) => {
                let Some(sec) = index.section(self, &r.section) else {
                    return bad("no such section".into());
                };
                if sec.calculation.method() != condition::Method::Fe {
                    return bad(format!(
                        "section {} is calculated by ruling span: a tap needs finite elements",
                        sec.id
                    ));
                }
                sec.span_points
                    .iter()
                    .find(|p| p.label == r.point)
                    .map(|p| &p.object)
            }
            Tap::Jumper { jumper, point } => {
                if *jumper == j.id {
                    return bad("a jumper cannot hang on itself".into());
                }
                let Some(host) = index.jumper(self, jumper) else {
                    return bad("no such jumper".into());
                };
                host.points
                    .iter()
                    .find(|p| &p.label == point)
                    .map(|p| &p.object)
            }
        };
        match object {
            None => bad("no such point".into()),
            Some(Some(o)) if o.kind == ObjectKind::Clamp && o.hold.is_some() => {
                bad("its clamp holds the jumper up (a support), not a tap".into())
            }
            Some(Some(o)) if o.takes_tap() => Ok(()),
            Some(Some(o)) if o.kind == ObjectKind::Clamp => bad(
                "its point holds a spacer clamp, which holds an interphase spacer's end: a \
                 jumper starts from a tap clamp"
                    .into(),
            ),
            Some(_) => bad("its point holds no clamp (put a clamp object there)".into()),
        }
    }

    /// Checks a jumper's support clamps: not with a `support` string, a
    /// cut length given, each clamp inside the jumper and after the one
    /// before (with room for both), its length above 0 and at most 1 m,
    /// its wire point held as [`clamp_strings`] says (a typed structure's
    /// wire points are checked once resolved).
    fn check_clamps(&self, index: &LineIndex, j: &Jumper) -> Result<(), String> {
        // Supports are placed by a fraction of the jumper's length; one
        // string from a fixed point at most.
        for p in &j.points {
            if p.object.as_ref().and_then(|o| o.hold.as_ref()).is_some()
                && !matches!(p.at, JumperPlace::Fraction(_))
            {
                return Err(format!(
                    "support {}: placed by a fraction of the jumper's length",
                    p.label
                ));
            }
        }
        if j.points
            .iter()
            .filter(|p| {
                p.object
                    .as_ref()
                    .and_then(|o| o.hold.as_ref())
                    .is_some_and(|h| matches!(h, objects::Hold::String { .. }))
            })
            .count()
            > 1
        {
            return Err("one support string from a fixed point at most".into());
        }
        let supports = j.supports();
        if supports.is_empty() {
            return Ok(());
        }
        if j.support().is_some() {
            return Err(
                "give a support string (support) or support clamps (supports), not both".into(),
            );
        }
        if j.rigid {
            return Err("a rigid jumper has no support clamps".into());
        }
        let Some(length) = j.length else {
            return Err(
                "a jumper with support clamps needs a length (its sag depends on them)".into(),
            );
        };
        let mut end_before = 0.0;
        for (k, c) in supports.iter().enumerate() {
            let n = k + 1;
            if !(c.clamp_length > 0.0 && c.clamp_length <= 1.0) {
                return Err(format!(
                    "support clamp {n}: its length must be above 0 and at most 1 m"
                ));
            }
            if !(c.at > 0.0 && c.at < 1.0) {
                return Err(format!(
                    "support clamp {n}: at must be between 0 and 1 (a fraction of the length)"
                ));
            }
            let (start, end) = (
                c.at * length - c.clamp_length / 2.0,
                c.at * length + c.clamp_length / 2.0,
            );
            if start <= end_before || end >= length {
                return Err(format!(
                    "support clamp {n} at {:.2} m along the jumper overlaps {} (clamps in order \
                     from `from`, each {:.2} m long)",
                    c.at * length,
                    if end >= length {
                        "the jumper's end".to_owned()
                    } else if k == 0 {
                        "the jumper's start".to_owned()
                    } else {
                        format!("clamp {k}")
                    },
                    c.clamp_length
                ));
            }
            end_before = end;
            let id = c.structure_id(j);
            let Some(structure) = index.structure(self, id) else {
                return Err(format!("support clamp {n}: unknown structure {id}"));
            };
            if structure.structure_type.is_some() && structure.wire(&c.wire).is_none() {
                // From the structure's type: checked once resolved.
                continue;
            }
            clamp_strings(structure, &c.wire).map_err(|m| format!("support clamp {n}: {m}"))?;
            if supports[..k]
                .iter()
                .any(|d| d.structure_id(j) == id && d.wire == c.wire)
            {
                return Err(format!(
                    "support clamps {n} and another hang on the same wire point {} of {id}",
                    c.wire
                ));
            }
        }
        Ok(())
    }

    /// Tied alignment points must be under an existing structure.
    fn check_alignment<'p>(
        &self,
        index: &LineIndex,
        points: impl Iterator<Item = &'p AlignmentPoint>,
    ) -> Result<(), LineError> {
        for a in points {
            let Some(id) = &a.structure else { continue };
            let Some(s) = index.structure(self, id) else {
                return Err(LineError::Invalid(format!(
                    "alignment point {} is tied to unknown structure {id}",
                    a.id
                )));
            };
            let d = (a.position[0] - s.position[0]).hypot(a.position[1] - s.position[1]);
            if d > 0.001 {
                return Err(LineError::Invalid(format!(
                    "alignment point {} is tied to {id} but {d:.3} m away from it (detach it to move it)",
                    a.id
                )));
            }
        }
        Ok(())
    }
}
