// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The whole validation as it was before the reference index (2026-11-18),
//! kept unchanged for the tests only: the validation by the index
//! ([`crate::Line::validate`]) and the local one
//! ([`crate::Line::validate_local`]) must give exactly what it gives.

#![allow(clippy::all)]

use std::collections::BTreeMap;

use crate::*;

impl Line {
    /// Checks references and basic consistency.
    pub(crate) fn validate_reference(&self) -> Result<(), LineError> {
        // Every number finite (a NaN or an infinity stored as JSON would no
        // longer read).
        finite::check(self).map_err(LineError::Invalid)?;
        // The structures by id (the first of an id, as `Line::structure`
        // finds it): looked up for every section point.
        let mut by_id: std::collections::HashMap<&str, &Structure> =
            std::collections::HashMap::with_capacity(self.structures.len());
        for s in &self.structures {
            by_id.entry(s.id.as_str()).or_insert(s);
        }
        let invalid = |m: String| Err(LineError::Invalid(m));
        self.bimetallic.validate().map_err(LineError::Invalid)?;
        if let Some(e) = self.creep_stretch.hot_problem() {
            return Err(LineError::Invalid(e));
        }
        self.electrical.validate().map_err(LineError::Invalid)?;
        if let Some(s) = self.structures.iter().find(|s| {
            s.soil_resistivity_ohm_m
                .is_some_and(|r| !(r.is_finite() && r > 0.0))
        }) {
            return invalid(format!(
                "structure {}: soil resistivity must be a number of Ω·m > 0",
                s.id
            ));
        }
        if let Some(s) = self.structures.iter().find(|s| {
            s.earthing_resistance_ohm
                .is_some_and(|r| !(r.is_finite() && r > 0.0))
        }) {
            return invalid(format!(
                "structure {}: earthing resistance must be a number of Ω > 0",
                s.id
            ));
        }
        if let Some(s) = self
            .structures
            .iter()
            .find(|s| s.no_load_cases && !s.load_cases.is_empty())
        {
            return invalid(format!(
                "structure {}: no_load_cases and load_cases ({}) contradict: give the list, or \
                 no_load_cases = true for a structure that takes none",
                s.id,
                s.load_cases.join(", ")
            ));
        }
        if let Some(p) = self.groups_problem() {
            return invalid(p);
        }
        for s in &self.structures {
            if let Some(p) = s
                .foot_movements
                .as_ref()
                .and_then(feet::FootMovements::problem)
            {
                return invalid(format!("structure {}: {p}", s.id));
            }
        }
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
        let mut ids = BTreeMap::new();
        for s in &self.structures {
            if ids.insert(s.id.as_str(), ()).is_some() {
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
        if self.routes.iter().any(Route::is_legacy) || !self.alignment.is_empty() {
            return invalid(
                "routes of a version 2 file not upgraded (Line::upgrade_routes)".to_owned(),
            );
        }
        // Systems, and one system phase per conductor path.
        self.section_phases()?;
        let mut route_ids = BTreeMap::new();
        for r in &self.routes {
            if r.id.trim().is_empty() || route_ids.insert(r.id.as_str(), ()).is_some() {
                return invalid(format!("route id \"{}\": empty or given twice", r.id));
            }
        }
        let known = |r: &String| route_ids.contains_key(r.as_str());
        let mut point_ids = BTreeMap::new();
        for p in &self.alignment_points {
            if p.id.trim().is_empty() || point_ids.insert(p.id.as_str(), ()).is_some() {
                return invalid(format!(
                    "alignment point id \"{}\": empty or given twice",
                    p.id
                ));
            }
            if !p.position.iter().all(|x| x.is_finite()) {
                return invalid(format!("alignment point {}: not a number", p.id));
            }
        }
        self.check_alignment_reference(&self.alignment_points)?;
        let mut alignment_ids = BTreeMap::new();
        for a in &self.alignments {
            if a.id.trim().is_empty() || alignment_ids.insert(a.id.as_str(), ()).is_some() {
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
                if !point_ids.contains_key(p.as_str()) {
                    return invalid(format!("alignment {}: unknown alignment point {p}", a.id));
                }
                if seen.insert(p.as_str(), ()).is_some() {
                    return invalid(format!("alignment {}: point {p} given twice", a.id));
                }
            }
        }
        if let Some((s, r)) = self
            .structures
            .iter()
            .find_map(|s| s.routes.iter().find(|r| !known(r)).map(|r| (s, r)))
        {
            return invalid(format!("structure {}: unknown route {r}", s.id));
        }
        let mut section_ids = BTreeMap::new();
        for sec in &self.sections {
            if section_ids.insert(sec.id.as_str(), ()).is_some() {
                return invalid(format!("duplicate section id {}", sec.id));
            }
            if sec.points.len() < 2 {
                return invalid(format!("section {}: needs at least two points", sec.id));
            }
            for p in &sec.points {
                let Some(structure) = by_id.get(p.structure.as_str()).copied() else {
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
                    || !self.inside_span(sec, p)
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
        let mut jumper_ids = BTreeMap::new();
        for j in &self.jumpers {
            let bad = |m: &str| invalid(format!("jumper {}: {m}", j.id));
            if jumper_ids.insert(j.id.as_str(), ()).is_some() {
                return bad("duplicate id");
            }
            // Its own bundle values, over those it takes from its section.
            let own = Bundle {
                count: j.bundle.unwrap_or_else(|| self.jumper_bundle(j).count),
                spacing: j.bundle_spacing,
                rotation_deg: j.bundle_rotation_deg,
            };
            own.check(&format!("jumper {}", j.id))
                .map_err(LineError::Invalid)?;
            for end in [&j.from, &j.to] {
                let end = match end {
                    JumperEnd::Tap { tap } => {
                        self.check_tap_reference(j, tap)?;
                        continue;
                    }
                    JumperEnd::Wire(p) => p,
                };
                let Some(structure) = by_id.get(end.structure.as_str()).copied() else {
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
            self.check_clamps_reference(j)
                .map_err(|m| LineError::Invalid(format!("jumper {}: {m}", j.id)))?;
            if let Some(c) = j.color.as_deref().filter(|c| !is_hex_color(c)) {
                return bad(&format!("colour \"{c}\" is not #rrggbb"));
            }
        }
        self.defaults.validate().map_err(LineError::Invalid)?;
        self.validate_circuits().map_err(LineError::Invalid)?;
        // Circuit starts: section points, each once.
        let mut starts = std::collections::BTreeSet::new();
        for p in &self.circuit_starts {
            if !starts.insert((p.structure.as_str(), p.set)) {
                return invalid(format!(
                    "circuit start {} set {} given twice",
                    p.structure, p.set
                ));
            }
            if !self.sections.iter().any(|s| s.points.contains(p)) {
                return invalid(format!(
                    "circuit start {} set {} is no section's point",
                    p.structure, p.set
                ));
            }
        }
        let mut ids = std::collections::BTreeSet::new();
        for s in &self.spacers {
            let bad = |m: &str| invalid(format!("spacer {}: {m}", s.id));
            if s.id.trim().is_empty() || !ids.insert(s.id.as_str()) {
                return invalid(format!("spacer ids must be given and unique ({:?})", s.id));
            }
            if s.a.section == s.b.section {
                return bad("its ends must be on two different sections");
            }
            let mut spans = Vec::new();
            for end in [&s.a, &s.b] {
                let Some(sec) = self.sections.iter().find(|x| x.id == end.section) else {
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
        self.jumper_order()?;
        Ok(())
    }

    /// Tap `tap` of jumper `j` holds: its point exists and holds a clamp;
    /// a span's section is solved by finite elements; another jumper.
    fn check_tap_reference(&self, j: &Jumper, tap: &Tap) -> Result<(), LineError> {
        let bad = |m: String| {
            Err(LineError::Invalid(format!(
                "jumper {}: tap {}: {m}",
                j.id,
                tap.text()
            )))
        };
        let object = match tap {
            Tap::Span(r) => {
                let Some(sec) = self.sections.iter().find(|s| s.id == r.section) else {
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
                let Some(host) = self.jumpers.iter().find(|x| &x.id == jumper) else {
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
    fn check_clamps_reference(&self, j: &Jumper) -> Result<(), String> {
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
            let Some(structure) = self.structure(id) else {
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
    fn check_alignment_reference(&self, points: &[AlignmentPoint]) -> Result<(), LineError> {
        for a in points {
            let Some(id) = &a.structure else { continue };
            let Some(s) = self.structure(id) else {
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
