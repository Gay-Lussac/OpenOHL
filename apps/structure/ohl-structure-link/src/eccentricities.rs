// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Estimated eccentricities** (O45 A1–A2, owner 2026-11-19; built, in
//! active development, not approved yet): at level 3 — the joints not all
//! detailed, the members checked in the simple form — the analysis may
//! take the member ends' sideways shifts the joint layout would make, as
//! the rigid end offsets the level 4 analysis takes from the connections'
//! `shift` ([`ohl_structures_fe::offsets`]). An option of the structure
//! (`estimated_eccentricities = true` in the layout's record), off by
//! default.
//!
//! **A1, the estimates** ([`estimate`]): the layout's planner run
//! *relaxed* on the file as it is ([`crate::layout_run::plan_relaxed`]:
//! the automatic values taken out, the members turned flat where that
//! helps, each joint searched, nothing written, no clash rounds). Per
//! member end the planner takes (a single angle bolting by one leg onto a
//! member at the joint, not in line with it):
//!
//! - **laid out**: the joint layout laid it out (the record's `ends`): its
//!   `shift` as written (none: concentric);
//! - **given**: a `shift` given by hand at the end: as given;
//! - **estimated**: the planner places it: the shift it plans (none:
//!   concentric) — kept even where a run would not write the joint (a
//!   clash after, or the joint left whole); on a plate in its own plane
//!   (step 4) its axis farther out of the flange by the plan's
//!   `out_more` too;
//! - **upper bound**: no layout fits: the shift limit (the record's, 30 mm
//!   to start with), and where a plate in its own plane would be needed
//!   (its bolted leg lies off the flange) that plate's offset too, at its
//!   limit (step 4: its leg's thickness) — said "upper bound".
//!
//! Kept as an analysis value apart from the fabrication record: nothing
//! is written into the file, nothing is shown as a laid-out joint, and the
//! estimates are made again from the file at every run (so they follow
//! every edit). Deterministic: the same file, libraries and presets give
//! the same estimates.
//!
//! **A2, in the analysis** ([`offsets`], [`envelope`]): each end's offset
//! r = shift · u (+ the plate's offset · n), u the shift's direction
//! ([`ohl_structures::connections::shift_direction`]: across the member,
//! towards what it bolts onto — as the planner measures it for an
//! estimate, as the level 4 analysis does for a value in the file — else
//! the joint's leg), n out of the face (u × the member). An upper bound's
//! sign is not known, nor is the side of a plate's offset (2026-11-19, the
//! owner's decision on A1): such an offset is never in the solve — it is
//! the member's offset of unknown sign
//! ([`ohl_structures_fe::Member::free_offsets`]), taken only in its own
//! check, the moment N·e whole and sign-free, at frame joints as where only
//! bars meet. The cases are solved twice — without the estimates and with
//! them — and each member and plate takes its larger usage (on the safe
//! side: an estimate may not be built as planned).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ohl_sections::SectionLibrary;
use ohl_structures::bolts::BoltLibrary;
use ohl_structures::{LibraryItem, StructureType};
use ohl_structures_fe::Model;
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

use crate::joint_layout::{BoltArea, LayoutSettings, Onto};
use crate::layout_run::{LayoutRecord, RECORD_ID, RECORD_KIND, RunAsk, presets_by_class};
use crate::{CaseResult, MemberResult};

/// The option's key in the joint layout's record (`[[generator]] kind =
/// "joint_layout"`).
pub const OPTION_KEY: &str = "estimated_eccentricities";

/// The bolt the estimates take for an end without bolts and without a
/// preset when the file has no layout settings: the layout window's
/// starting bolt (16 mm; the library's bolt of that diameter where there
/// is one).
pub const BOLT_START: f64 = 0.016;

/// Where an end's eccentricity comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Source {
    /// Laid out by the joint layout: its `shift` as written.
    LaidOut,
    /// A `shift` given by hand.
    Given,
    /// Planned by the layout's planner, not laid out.
    Estimated,
    /// No layout fits: the upper bound.
    UpperBound,
}

impl Source {
    /// In words.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LaidOut => "laid out",
            Self::Given => "given",
            Self::Estimated => "estimated",
            Self::UpperBound => "upper bound",
        }
    }
}

/// One member end's eccentricity.
#[derive(Debug, Clone, PartialEq)]
pub struct EstimatedEnd {
    /// The member.
    pub member: String,
    /// The joint.
    pub joint: String,
    /// Where it comes from.
    pub source: Source,
    /// The sideways shift, m: signed (the connection's convention) for
    /// the others, the size for an upper bound (its sign not known).
    pub shift: f64,
    /// The member its shift is measured towards (none: the joint's leg —
    /// as the level 4 analysis measures a shift onto a plate or onto
    /// nothing named).
    pub towards: Option<String>,
    /// Its axis farther out of the face, m (0: none): a plate in its own
    /// plane (the plan's, or an upper bound's at step 4's limit).
    pub plate: f64,
    /// Why no layout fits (an upper bound), the planner's words.
    pub why: Option<String>,
}

impl EstimatedEnd {
    /// Its offset's sign is not known (an upper bound, or a plate's side).
    pub fn either_sign(&self) -> bool {
        self.source == Source::UpperBound || self.plate != 0.0
    }
}

/// The estimates of a structure.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Estimates {
    /// Every member end the planner takes and every shift in the file.
    pub ends: Vec<EstimatedEnd>,
    /// The shift limit the upper bounds take, m.
    pub shift_limit: f64,
    /// How long the estimates took, s.
    pub seconds: f64,
}

impl Estimates {
    /// The ends from `source`.
    pub fn count(&self, source: Source) -> usize {
        self.ends.iter().filter(|e| e.source == source).count()
    }

    /// The ends with an eccentricity (a shift or a plate's offset).
    pub fn eccentric(&self) -> usize {
        self.ends
            .iter()
            .filter(|e| e.shift != 0.0 || e.plate != 0.0)
            .count()
    }

    /// The upper bounds with a plate's offset (the plate case).
    pub fn plate_bounds(&self) -> usize {
        self.ends
            .iter()
            .filter(|e| e.source == Source::UpperBound && e.plate != 0.0)
            .count()
    }

    /// Any end whose offset's sign is not known.
    pub fn either_sign(&self) -> bool {
        self.ends.iter().any(EstimatedEnd::either_sign)
    }
}

/// The estimated eccentricities a run used (O45 A2), and its measured
/// time.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EstimateUse {
    /// The estimates.
    pub estimates: Estimates,
    /// The ends with an offset in the model.
    pub offsets: usize,
    /// The ends whose offset (or a part of it) is of unknown sign: taken
    /// only in their member's own check (2026-11-19).
    pub sign_free: usize,
    /// The solves of every case: 2 (without and with them).
    pub solves: usize,
    /// How long the solves took in all, s.
    pub solve_seconds: f64,
}

/// What the estimates read besides the file.
#[derive(Debug, Clone, Copy)]
pub struct EstimateAsk<'a> {
    /// The section library.
    pub library: &'a SectionLibrary,
    /// The bolt library.
    pub bolts: &'a BoltLibrary,
    /// The connection presets (name, fields as a connection's), for the
    /// classes' presets the layout's record names.
    pub presets: &'a [(String, Table)],
}

/// Whether structure `t` asks for the estimated eccentricities.
pub fn option_on(t: &StructureType) -> bool {
    LayoutRecord::of(t).estimated
}

/// Sets the option in `doc` (a structure file): the layout's record made
/// when on and there is none; off: the key removed, and the record too
/// when nothing else is left in it (the file as before).
pub fn set_option(doc: &mut DocumentMut, on: bool) {
    let is_record = |t: &Table| t.get("kind").and_then(Item::as_str) == Some(RECORD_KIND);
    let found = doc
        .get_mut("generator")
        .and_then(Item::as_array_of_tables_mut)
        .and_then(|a| a.iter_mut().find(|t| is_record(t)));
    match found {
        Some(t) if on => {
            t.insert(OPTION_KEY, toml_edit::value(true));
        }
        Some(t) => {
            t.remove(OPTION_KEY);
        }
        None if on => {
            let mut t = Table::new();
            t.insert("id", toml_edit::value(RECORD_ID));
            t.insert("kind", toml_edit::value(RECORD_KIND));
            t.insert(OPTION_KEY, toml_edit::value(true));
            match doc
                .get_mut("generator")
                .and_then(Item::as_array_of_tables_mut)
            {
                Some(a) => a.push(t),
                None => {
                    let mut a = ArrayOfTables::new();
                    a.push(t);
                    doc.insert("generator", Item::ArrayOfTables(a));
                }
            }
        }
        None => {}
    }
    if !on {
        let mut empty = false;
        if let Some(a) = doc
            .get_mut("generator")
            .and_then(Item::as_array_of_tables_mut)
        {
            a.retain(|t| !(is_record(t) && t.iter().all(|(k, _)| k == "id" || k == "kind")));
            empty = a.is_empty();
        }
        if empty {
            doc.remove("generator");
        }
    }
}

/// The connection presets beside a structure's folder `dir`
/// (`Connections/connection-presets.ohlcp`, as OHL-Structure reads them);
/// none when missing or unreadable.
pub fn presets_beside(dir: &Path) -> Vec<(String, Table)> {
    let path = dir.join("Connections").join(format!(
        "connection-presets.{}",
        ohl_structure_edit::connection_edits::PRESETS_EXTENSION
    ));
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| ohl_structure_edit::connection_edits::parse_presets(&t).ok())
        .map(|p| p.into_iter().map(|p| (p.name, p.fields)).collect())
        .unwrap_or_default()
}

/// The layout settings the estimates take: the record's (its classes'
/// presets from `presets`), else the starting values with the starting
/// bolt ([`BOLT_START`], the library's bolt of that diameter where there
/// is one).
pub fn settings_for(
    t: &StructureType,
    record: &LayoutRecord,
    ask: &EstimateAsk<'_>,
) -> (LayoutSettings, BTreeMap<String, Table>) {
    match &record.settings {
        Some(s) => {
            let (areas, fields) = presets_by_class(&s.presets, ask.presets, ask.bolts);
            (record.with_practice(t, s.layout(areas)), fields)
        }
        None => {
            let lib = ask
                .bolts
                .bolts
                .iter()
                .find(|b| (b.diameter.si() - BOLT_START).abs() < 0.5e-3);
            let bolt = BoltArea::picked(
                lib.map(|b| b.id.clone()),
                lib.map_or(BOLT_START, |b| b.diameter.si()),
                lib.and_then(|b| b.drawn()[0]),
            );
            (
                record.with_practice(
                    t,
                    LayoutSettings {
                        bolt: Some(bolt),
                        ..LayoutSettings::default()
                    },
                ),
                BTreeMap::new(),
            )
        }
    }
}

/// The estimated eccentricities of structure file `text` (A1; see the
/// module). Err: the file does not read, or the planner fails on it.
///
/// Expected cost: the layout's planning without its clash rounds — about
/// as long as the joint details of the whole tower a few times (measured
/// on real towers: see the theory page).
pub fn estimate(text: &str, ask: &EstimateAsk<'_>) -> Result<Estimates, String> {
    let start = std::time::Instant::now();
    let t = StructureType::parse(Path::new("structure.ohls"), text).map_err(|e| e.message)?;
    let record = LayoutRecord::of(&t);
    let (settings, preset_fields) = settings_for(&t, &record, ask);
    let run = crate::layout_run::plan_relaxed(
        text,
        &RunAsk {
            library: ask.library,
            bolts: ask.bolts,
            settings: &settings,
            preset_fields,
            joints: None,
            replace_hand: false,
        },
    )?;
    let at_joint = |m: &str, j: &str| {
        t.members
            .iter()
            .any(|x| x.id == m && x.nodes.iter().any(|n| n == j))
    };
    // A value in the file (laid out, or given by hand): measured as the
    // level 4 analysis measures it — towards the member named, else the
    // joint's leg.
    let in_file = |m: &str, j: &str| -> Option<EstimatedEnd> {
        let c = t.connection(m, j);
        let laid = record.ends.contains_key(&(m.to_owned(), j.to_owned()));
        let shift = c.and_then(|c| c.shift).map(|s| s.si()).unwrap_or(0.0);
        if !laid && shift == 0.0 {
            return None;
        }
        // (L20 item 2) Laid out on a plate in its own plane putting its axis
        // more than a plate's thickness out: that offset (the record's).
        let plate = c
            .and_then(|c| c.onto.as_deref())
            .and_then(|o| record.own_out(o))
            .filter(|_| laid)
            .unwrap_or(0.0);
        if !laid && shift == 0.0 && plate == 0.0 {
            return None;
        }
        Some(EstimatedEnd {
            member: m.to_owned(),
            joint: j.to_owned(),
            source: if laid { Source::LaidOut } else { Source::Given },
            shift,
            towards: c.and_then(|c| c.onto.clone()).filter(|o| at_joint(o, j)),
            plate,
            why: None,
        })
    };
    let mut ends = Vec::new();
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    for o in &run.joints {
        let j = &o.joint;
        for e in &o.plan.ends {
            if !seen.insert((e.member.clone(), j.clone())) {
                continue;
            }
            if let Some(x) = in_file(&e.member, j) {
                ends.push(x);
                continue;
            }
            // The planner measures its shift towards the member it bolts
            // onto (on a plate: the member the plate lies on).
            let (towards, plate) = match &e.onto {
                Onto::Member(h) => (Some(h.clone()), 0.0),
                Onto::Plate(k) => {
                    let p = o.plan.plates.get(*k);
                    (
                        p.map(|p| p.on.clone()),
                        p.and_then(|p| p.own.as_ref())
                            .map_or(0.0, |w| w.out_more.max(0.0)),
                    )
                }
            };
            ends.push(EstimatedEnd {
                member: e.member.clone(),
                joint: j.clone(),
                source: Source::Estimated,
                shift: e.shift.unwrap_or(0.0),
                towards,
                plate,
                why: None,
            });
        }
        for m in &o.plan.hand_members {
            if !seen.insert((m.clone(), j.clone())) {
                continue;
            }
            if let Some(x) = in_file(m, j) {
                ends.push(x);
                continue;
            }
            let plate = o
                .plan
                .off_flange
                .iter()
                .find(|(x, _)| x == m)
                .map_or(0.0, |(_, t)| *t);
            // The planner's words naming it first ("R1: …", "R1 and D1
            // within 1°: …").
            let why = o
                .plan
                .hand
                .iter()
                .find(|h| h.starts_with(&format!("{m}:")))
                .map(|h| h[m.len() + 1..].trim_start().to_owned())
                .or_else(|| {
                    o.plan
                        .hand
                        .iter()
                        .find(|h| h.starts_with(&format!("{m} ")))
                        .cloned()
                });
            // Towards what it would bolt onto (as an estimate's).
            let towards = o
                .plan
                .hand_hosts
                .iter()
                .find(|(x, _)| x == m)
                .map(|(_, h)| h.clone());
            ends.push(EstimatedEnd {
                member: m.clone(),
                joint: j.clone(),
                source: Source::UpperBound,
                shift: settings.shift_limit,
                towards,
                plate,
                why,
            });
        }
    }
    // A shift given anywhere else (an end the planner does not take): as
    // given.
    for c in &t.connections {
        if seen.contains(&(c.member.clone(), c.joint.clone())) {
            continue;
        }
        if let Some(x) = in_file(&c.member, &c.joint)
            && x.shift != 0.0
        {
            seen.insert((c.member.clone(), c.joint.clone()));
            ends.push(x);
        }
    }
    Ok(Estimates {
        ends,
        shift_limit: settings.shift_limit,
        seconds: start.elapsed().as_secs_f64(),
    })
}

/// One end's offset in a model.
#[derive(Debug, Clone, PartialEq)]
pub struct EndOffset {
    /// The member (its index in the model).
    pub member: usize,
    /// Its end: 0 at its first joint, 1 at its second.
    pub end: usize,
    /// The rigid end offset of known sign (in the solve), structure axes,
    /// m: an estimated, given or laid-out shift; zero: none.
    pub offset: [f64; 3],
    /// The parts whose sign is not known (2026-11-19): an upper bound's
    /// shift, a plate's offset out of the face — each the member's offset
    /// of unknown sign, only in its own check
    /// ([`ohl_structures_fe::Member::free_offsets`]).
    pub free: Vec<[f64; 3]>,
    /// Where it comes from.
    pub source: Source,
}

impl EndOffset {
    /// Its eccentricity, m: the known offset and the parts of unknown sign
    /// square to each other.
    pub fn e(&self) -> f64 {
        (norm(self.offset).powi(2) + self.free.iter().map(|r| norm(*r).powi(2)).sum::<f64>()).sqrt()
    }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn norm(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let l = norm(a);
    (l > 1e-12).then(|| a.map(|c| c / l))
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The offsets of `estimates` in `model` (A2): a shift of known sign as
/// the end's rigid offset, an upper bound's shift and a plate's offset as
/// parts of unknown sign (2026-11-19: only in the member's own check); with
/// notes on the ends left out (not in the model, no direction to measure
/// in).
pub fn offsets(model: &Model, estimates: &Estimates) -> (Vec<EndOffset>, Vec<String>) {
    let mut out = Vec::new();
    let mut notes = Vec::new();
    let nodes: BTreeMap<&str, usize> = model
        .nodes
        .iter()
        .enumerate()
        .map(|(k, n)| (n.id.as_str(), k))
        .collect();
    let members: BTreeMap<&str, usize> = model
        .members
        .iter()
        .enumerate()
        .map(|(k, m)| (m.id.as_str(), k))
        .collect();
    for e in &estimates.ends {
        if e.shift == 0.0 && e.plate == 0.0 {
            continue;
        }
        let (Some(&idx), Some(&node)) =
            (members.get(e.member.as_str()), nodes.get(e.joint.as_str()))
        else {
            notes.push(format!(
                "{} at {}: not in the model, its eccentricity left out",
                e.member, e.joint
            ));
            continue;
        };
        let m = &model.members[idx];
        let (end, other) = if m.i == node {
            (0, m.j)
        } else if m.j == node {
            (1, m.i)
        } else {
            continue;
        };
        let at = model.nodes[node].point;
        let Some(along) = unit(sub(model.nodes[other].point, at)) else {
            continue;
        };
        // Towards the member named (its direction from the joint), else
        // the joint's leg as the level 4 analysis takes it.
        let named = e.towards.as_deref().and_then(|h| {
            let &k = members.get(h)?;
            let x = &model.members[k];
            let far = if x.i == node {
                x.j
            } else if x.j == node {
                x.i
            } else {
                return None;
            };
            unit(sub(model.nodes[far].point, at))
        });
        let towards = named
            .or_else(|| ohl_structures_fe::offsets::shift_towards(model, idx, end).map(|(_, d)| d));
        let u = towards.and_then(|t| ohl_structures::connections::shift_direction(along, t));
        let Some(u) = u else {
            notes.push(format!(
                "{} at {}: {} eccentricity left out — nothing at the joint to measure it towards \
                 (or it runs in line)",
                e.member,
                e.joint,
                e.source.as_str()
            ));
            continue;
        };
        let n = unit(cross(along, u)).unwrap_or([0.0; 3]);
        let shift: [f64; 3] = u.map(|c| c * e.shift);
        let plate: [f64; 3] = n.map(|c| c * e.plate);
        let (offset, mut free) = if e.source == Source::UpperBound {
            ([0.0; 3], vec![shift])
        } else {
            (shift, Vec::new())
        };
        free.push(plate);
        free.retain(|r| norm(*r) > 0.0);
        if norm(offset) == 0.0 && free.is_empty() {
            continue;
        }
        out.push(EndOffset {
            member: idx,
            end,
            offset,
            free,
            source: e.source,
        });
    }
    (out, notes)
}

/// `model` with `offsets` as its members' rigid end offsets and offsets of
/// unknown sign.
pub fn apply(model: &mut Model, offsets: &[EndOffset]) {
    for o in offsets {
        let m = &mut model.members[o.member];
        m.offsets[o.end] = o.offset;
        m.free_offsets[o.end] = o.free.clone();
    }
}

/// The ends' sources of member `id` in words, for its eccentric words:
/// "estimated at N2; upper bound at N3".
fn sources_of(estimates: &Estimates, id: &str) -> Option<String> {
    let parts: Vec<String> = estimates
        .ends
        .iter()
        .filter(|e| e.member == id && (e.shift != 0.0 || e.plate != 0.0))
        .map(|e| format!("{} at {}", e.source.as_str(), e.joint))
        .collect();
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// The usage order of two usages (none below any number).
fn larger(a: Option<f64>, b: Option<f64>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => ohl_structures_fe::check::usage_order(a, b).is_gt(),
        (Some(_), None) => true,
        _ => false,
    }
}

/// One case's results of the two solves (A2): `with` the estimates (the
/// offsets of unknown sign in their members' own checks, 2026-11-19) and
/// `without` them. Each member takes the solve where its usage is larger
/// (its forces, usage and what governs from it), each plate and weld the
/// same; the joints, reactions and sums are those with the estimates. A
/// solve that fails fails the case, said.
pub fn envelope(
    with: Result<CaseResult, String>,
    without: Result<CaseResult, String>,
    estimates: &Estimates,
) -> Result<CaseResult, String> {
    let without = without?;
    let mut out = with.map_err(|e| format!("with the estimated eccentricities: {e}"))?;
    let alternatives: Vec<(&CaseResult, &str)> =
        vec![(&without, "without the estimated eccentricities")];
    for (k, m) in out.members.iter_mut().enumerate() {
        let words = sources_of(estimates, &m.id);
        for (alt, why) in &alternatives {
            if let Some(a) = alt.members.get(k)
                && a.id == m.id
                && larger(a.usage, m.usage)
            {
                let keep: MemberResult = a.clone();
                *m = MemberResult {
                    governs: if keep.governs.is_empty() {
                        keep.governs.clone()
                    } else {
                        format!("{} ({why})", keep.governs)
                    },
                    eccentric: m.eccentric.clone(),
                    ..keep
                };
            }
        }
        if let Some(w) = words {
            m.eccentric = Some(match &m.eccentric {
                Some(e) => format!("{e} [{w}]"),
                None => format!("[{w}]"),
            });
        }
    }
    for (k, p) in out.plates.iter_mut().enumerate() {
        for (alt, why) in &alternatives {
            if let Some(a) = alt.plates.get(k)
                && a.joint == p.joint
                && larger(a.usage, p.usage)
            {
                *p = a.clone();
                p.governs = format!("{} ({why})", p.governs);
            }
        }
    }
    for n in out.notes.iter_mut() {
        if n.starts_with("eccentric member ends (connection shifts)") {
            *n = n.replacen(
                "eccentric member ends (connection shifts)",
                "eccentric member ends (the estimated eccentricities)",
                1,
            );
        }
    }
    // What the other solves say is not on the safe side, once.
    for (alt, why) in &alternatives {
        for n in &alt.notes {
            if n.starts_with("NOT ON THE SAFE SIDE") && !out.notes.contains(n) {
                out.notes.push(format!("{n} ({why})"));
            }
        }
    }
    Ok(out)
}
