// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The automatic joint layout run on a structure file (2026-11-18, stages
//! 3 and 4): the engine's plans ([`crate::joint_layout`]) written into the
//! file, the plates made, what was set marked *automatic*, and the layout
//! following later changes.
//!
//! **The record.** What the layout set is listed in one `[[generator]]`
//! of kind `joint_layout` (read by the structure application only, as the
//! body generators; the core passes it through): its settings, the joints
//! laid out, each automatic value (`ends`: member, joint, the fields;
//! `members`: a roll; `made`: a plate it made and its fields), the ends
//! whose bolts are *provisional*, the joints left for a hand detail with
//! why. The values themselves stay where they always were (`cut`,
//! `shift`, `onto`, `bolts`, `[[plate]]`): the analysis, the exports and
//! the core read them as before. A file without the record reads as
//! before.
//!
//! **A run.** For the joints asked: the automatic values taken out (and
//! the ones given by hand too when *Replace values given by hand*), the
//! members lying off what they bolt onto turned flat, each joint planned
//! from that; the joints that fit written into the file as it was (their
//! old automatic values replaced, hand values kept), the others left
//! exactly as they were, with why. Then the joint details again and the
//! clash check: a joint laid out whose members, bolts or plates run into
//! anything is taken back and listed with the clash. The same file and
//! settings give the same result: a run again changes nothing.
//!
//! **Following.** An edit that touches a laid-out joint (a member's
//! section, roll or bolts, a plate's thickness or material, a value at an
//! end, a joint moved, the record's settings) lays that joint out again
//! in the same undo step ([`follow`]): an automatic value the edit changed
//! becomes the user's (kept); no fit: the edit kept, the joint's layout
//! kept, the joint listed for a hand detail with why, said in the edit's
//! message.

use std::collections::{BTreeMap, BTreeSet};

use ohl_sections::SectionLibrary;
use ohl_structure_edit::connection_edits::{self, FieldValue};
use ohl_structures::bolts::BoltLibrary;
use ohl_structures::connections::{TypeConnection, TypePlate};
use ohl_structures::{LibraryItem, StructureType};
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, Value};

use crate::joint_layout::{
    BoltArea, Join, JoinKind, JointPlan, LayoutSettings, Onto, Reach, flat_turns, plan_joint,
};
use crate::joints::{JointDetails, joint_details_at};

/// The record's id and kind.
pub const RECORD_ID: &str = "joint-layout";
/// The record's kind (`ohl_structure_edit::body::JOINT_LAYOUT_KIND`).
pub const RECORD_KIND: &str = ohl_structure_edit::body::JOINT_LAYOUT_KIND;

/// The record's mark of a plate made in its member's own plane, welded to
/// a flat plate (`made`; the owner's step 4, 2026-11-19): the joint details
/// seat its member on the plate's outline, lifted along the flat plate's
/// normal ([`crate::joints`]).
pub const OWN_PLANE: &str = "own_plane";

/// The record's mark of a splice the layout made (`made`, with `cover`
/// and the covers' thickness as written; step 1c, 2026-11-19).
pub const SPLICE: &str = "splice";

/// The fields of a member end the layout sets.
pub const END_FIELDS: [&str; 8] = [
    "cut",
    "shift",
    "onto",
    "bolts",
    "gauge_lines",
    "preset",
    "kind",
    "face",
];

/// What the layout set in a file, and its settings.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LayoutRecord {
    /// The settings of the last run (none: no run yet).
    pub settings: Option<RecordSettings>,
    /// The joints laid out.
    pub joints: BTreeSet<String>,
    /// Each member end's automatic fields: (member, joint) → fields.
    pub ends: BTreeMap<(String, String), BTreeSet<String>>,
    /// Members turned flat by the layout (their roll automatic) and the
    /// roll each had before, as written (empty: none; a run again starts
    /// from exactly it).
    pub rolled: BTreeMap<String, String>,
    /// The plates the layout made: id → its joint and its automatic
    /// fields (`thickness`, `welded_to`).
    pub made: BTreeMap<String, (String, BTreeSet<String>)>,
    /// The member ends whose bolts are provisional (the layout's).
    pub provisional: BTreeSet<(String, String)>,
    /// The joints that need a hand detail, with why (left by a run, or no
    /// longer fitting after an edit).
    pub hand: BTreeMap<String, String>,
    /// Members turned by Orient angles ([`crate::orient_angles`]): the
    /// roll each had before (as written) and as what (a leg or chord, a
    /// plan member).
    pub oriented: BTreeMap<String, (String, crate::OrientKind)>,
    /// The option **estimated eccentricities** (O45, owner 2026-11-19;
    /// `estimated_eccentricities = true`, written only when on): at level
    /// 3 the analysis takes the member ends' eccentricities the layout's
    /// planner estimates ([`crate::eccentricities`]); off by default.
    pub estimated: bool,
}

/// The settings a run is made with, as the record keeps them.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordSettings {
    /// The gap between steel, m.
    pub gap: f64,
    /// The longest pull-back, m.
    pub pull_back: f64,
    /// The shift limit, m.
    pub shift_limit: f64,
    /// The plates of a joint welded together.
    pub welded: bool,
    /// The bolt picked (a library bolt) and its diameter, m.
    pub bolt: Option<(Option<String>, f64)>,
    /// The connection preset for each member class, by name.
    pub presets: BTreeMap<String, String>,
}

/// The bolt area of connection preset `name` (its fields as a
/// connection's): its bolts (count, a library bolt or a diameter), gauge
/// lines, gauges, pitch and end distance. None: no bolts in it.
pub fn preset_area(name: &str, fields: &Table, bolts: &BoltLibrary) -> Option<BoltArea> {
    let len = |v: Option<&Item>| {
        v.and_then(Item::as_str)
            .and_then(|s| s.parse::<ohl_core::units::Length>().ok())
            .map(|l| l.si())
    };
    let b = fields.get("bolts")?;
    let get = |k: &str| -> Option<&Item> { b.get(k) };
    let count = get("count").and_then(Item::as_integer)? as u32;
    let bolt = get("bolt").and_then(Item::as_str).map(str::to_owned);
    let lib = bolt.as_deref().and_then(|id| bolts.get(id));
    let diameter = len(get("diameter")).or(lib.map(|l| l.diameter.si()))?;
    let lines = fields
        .get("gauge_lines")
        .and_then(Item::as_integer)
        .unwrap_or(1)
        .max(1) as u32;
    let gauges = fields
        .get("gauges")
        .and_then(Item::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str()?.parse::<ohl_core::units::Length>().ok())
                .map(|l| l.si())
                .collect()
        })
        .unwrap_or_default();
    Some(BoltArea {
        from: format!("preset {name}"),
        preset: Some(name.to_owned()),
        count,
        lines,
        bolt,
        diameter,
        flats: lib.and_then(|l| l.drawn()[0]),
        gauges,
        pitch: len(fields.get("pitch")),
        end: len(fields.get("end_distance")),
    })
}

/// The engine's presets by class and the fields to write, from the
/// presets named by class (`by_class`: class → name) among `all` (name,
/// fields).
pub fn presets_by_class(
    by_class: &BTreeMap<String, String>,
    all: &[(String, Table)],
    bolts: &BoltLibrary,
) -> (BTreeMap<String, BoltArea>, BTreeMap<String, Table>) {
    let mut areas = BTreeMap::new();
    let mut fields = BTreeMap::new();
    for (class, name) in by_class {
        if let Some((_, f)) = all.iter().find(|(n, _)| n == name)
            && let Some(a) = preset_area(name, f, bolts)
        {
            areas.insert(class.clone(), a);
            fields.insert(class.clone(), f.clone());
        }
    }
    (areas, fields)
}

impl RecordSettings {
    /// As the engine takes them (the presets are the application's).
    pub fn layout(&self, presets: BTreeMap<String, BoltArea>) -> LayoutSettings {
        LayoutSettings {
            gap: self.gap,
            pull_back: self.pull_back,
            shift_limit: self.shift_limit,
            presets,
            bolt: self
                .bolt
                .clone()
                .map(|(id, d)| BoltArea::picked(id, d, None)),
            welded: self.welded,
            // The structure file's, taken by the run.
            defaults: Default::default(),
        }
    }

    /// From the engine's settings.
    pub fn of(s: &LayoutSettings) -> Self {
        Self {
            gap: s.gap,
            pull_back: s.pull_back,
            shift_limit: s.shift_limit,
            welded: s.welded,
            bolt: s.bolt.as_ref().map(|b| (b.bolt.clone(), b.diameter)),
            presets: s
                .presets
                .iter()
                .filter_map(|(c, a)| a.preset.clone().map(|n| (c.clone(), n)))
                .collect(),
        }
    }
}

impl LayoutRecord {
    /// The record of structure `t` (empty: none).
    pub fn of(t: &StructureType) -> Self {
        t.generators
            .iter()
            .find(|g| g.get("kind").and_then(toml::Value::as_str) == Some(RECORD_KIND))
            .map(Self::from_table)
            .unwrap_or_default()
    }

    fn from_table(g: &toml::Table) -> Self {
        let len = |k: &str| {
            g.get(k)
                .and_then(toml::Value::as_str)
                .and_then(|s| s.parse::<ohl_core::units::Length>().ok())
                .map(|l| l.si())
        };
        let rows = |k: &str| -> Vec<Vec<String>> {
            g.get(k)
                .and_then(toml::Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|r| {
                            r.as_array().map(|r| {
                                r.iter()
                                    .filter_map(|v| v.as_str().map(str::to_owned))
                                    .collect()
                            })
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        let words = |k: &str| -> BTreeSet<String> {
            g.get(k)
                .and_then(toml::Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        let settings = match (len("gap"), len("pull_back"), len("shift_limit")) {
            (Some(gap), Some(pull_back), Some(shift_limit)) => Some(RecordSettings {
                gap,
                pull_back,
                shift_limit,
                welded: g.get("plates").and_then(toml::Value::as_str) == Some("welded"),
                presets: rows("presets")
                    .into_iter()
                    .filter(|r| r.len() == 2)
                    .map(|r| (r[0].clone(), r[1].clone()))
                    .collect(),
                bolt: len("bolt_diameter").map(|d| {
                    (
                        g.get("bolt")
                            .and_then(toml::Value::as_str)
                            .map(str::to_owned),
                        d,
                    )
                }),
            }),
            _ => None,
        };
        let mut r = Self {
            settings,
            joints: words("joints"),
            rolled: rows("rolled")
                .into_iter()
                .filter(|r| r.len() == 2)
                .map(|r| (r[0].clone(), r[1].clone()))
                .collect(),
            estimated: g
                .get(crate::eccentricities::OPTION_KEY)
                .and_then(toml::Value::as_bool)
                == Some(true),
            ..Default::default()
        };
        for row in rows("ends") {
            if row.len() >= 2 {
                r.ends.insert(
                    (row[0].clone(), row[1].clone()),
                    row[2..].iter().cloned().collect(),
                );
            }
        }
        for row in rows("made") {
            if row.len() >= 2 {
                r.made.insert(
                    row[0].clone(),
                    (row[1].clone(), row[2..].iter().cloned().collect()),
                );
            }
        }
        for row in rows("provisional") {
            if row.len() == 2 {
                r.provisional.insert((row[0].clone(), row[1].clone()));
            }
        }
        for row in rows("hand") {
            if row.len() == 2 {
                r.hand.insert(row[0].clone(), row[1].clone());
            }
        }
        for row in rows("oriented") {
            if row.len() == 3
                && let Some(k) = crate::OrientKind::parse(&row[2])
            {
                r.oriented.insert(row[0].clone(), (row[1].clone(), k));
            }
        }
        r
    }

    /// Nothing recorded.
    pub fn is_empty(&self) -> bool {
        self.joints.is_empty()
            && self.ends.is_empty()
            && self.made.is_empty()
            && self.rolled.is_empty()
            && self.hand.is_empty()
            && self.oriented.is_empty()
    }

    /// Written into `doc` (replacing the record there; none when empty).
    pub fn write(&self, doc: &mut DocumentMut) {
        let keep: Vec<Table> = doc
            .get("generator")
            .and_then(Item::as_array_of_tables)
            .map(|a| {
                a.iter()
                    .filter(|t| t.get("kind").and_then(Item::as_str) != Some(RECORD_KIND))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        doc.remove("generator");
        let mut a = ArrayOfTables::new();
        for t in keep {
            a.push(t);
        }
        if !self.is_empty() || self.settings.is_some() || self.estimated {
            let mut t = Table::new();
            t.insert("id", toml_edit::value(RECORD_ID));
            t.insert("kind", toml_edit::value(RECORD_KIND));
            if self.estimated {
                t.insert(crate::eccentricities::OPTION_KEY, toml_edit::value(true));
            }
            if let Some(s) = &self.settings {
                t.insert("gap", toml_edit::value(crate::mm_text(s.gap)));
                t.insert("pull_back", toml_edit::value(crate::mm_text(s.pull_back)));
                t.insert(
                    "shift_limit",
                    toml_edit::value(crate::mm_text(s.shift_limit)),
                );
                t.insert(
                    "plates",
                    toml_edit::value(if s.welded { "welded" } else { "bolted" }),
                );
                if let Some((id, d)) = &s.bolt {
                    if let Some(id) = id {
                        t.insert("bolt", toml_edit::value(id.as_str()));
                    }
                    t.insert("bolt_diameter", toml_edit::value(crate::mm_text(*d)));
                }
            }
            let presets: Vec<Vec<String>> = self
                .settings
                .iter()
                .flat_map(|s| &s.presets)
                .map(|(c, n)| vec![c.clone(), n.clone()])
                .collect();
            let list = |items: Vec<Vec<String>>| -> Item {
                let mut out = Array::new();
                for row in items {
                    let v: Value = if row.len() == 1 {
                        Value::from(row[0].as_str())
                    } else {
                        let mut r = Array::new();
                        for x in row {
                            r.push(x);
                        }
                        Value::Array(r)
                    };
                    out.push_formatted(v.decorated("\n  ", ""));
                }
                out.set_trailing("\n");
                out.set_trailing_comma(true);
                Item::Value(Value::Array(out))
            };
            let one = |s: &BTreeSet<String>| s.iter().map(|x| vec![x.clone()]).collect();
            if !presets.is_empty() {
                t.insert("presets", list(presets));
            }
            if !self.joints.is_empty() {
                t.insert("joints", list(one(&self.joints)));
            }
            if !self.ends.is_empty() {
                t.insert(
                    "ends",
                    list(
                        self.ends
                            .iter()
                            .map(|((m, j), f)| {
                                let mut r = vec![m.clone(), j.clone()];
                                r.extend(f.iter().cloned());
                                r
                            })
                            .collect(),
                    ),
                );
            }
            if !self.rolled.is_empty() {
                t.insert(
                    "rolled",
                    list(
                        self.rolled
                            .iter()
                            .map(|(m, r)| vec![m.clone(), r.clone()])
                            .collect(),
                    ),
                );
            }
            if !self.made.is_empty() {
                t.insert(
                    "made",
                    list(
                        self.made
                            .iter()
                            .map(|(id, (j, f))| {
                                let mut r = vec![id.clone(), j.clone()];
                                r.extend(f.iter().cloned());
                                r
                            })
                            .collect(),
                    ),
                );
            }
            if !self.provisional.is_empty() {
                t.insert(
                    "provisional",
                    list(
                        self.provisional
                            .iter()
                            .map(|(m, j)| vec![m.clone(), j.clone(), String::new()])
                            .map(|mut r| {
                                r.pop();
                                r
                            })
                            .collect(),
                    ),
                );
            }
            if !self.hand.is_empty() {
                t.insert(
                    "hand",
                    list(
                        self.hand
                            .iter()
                            .map(|(j, w)| vec![j.clone(), w.clone()])
                            .collect(),
                    ),
                );
            }
            if !self.oriented.is_empty() {
                t.insert(
                    "oriented",
                    list(
                        self.oriented
                            .iter()
                            .map(|(m, (r, k))| vec![m.clone(), r.clone(), k.as_str().to_owned()])
                            .collect(),
                    ),
                );
            }
            a.push(t);
        }
        if !a.is_empty() {
            doc.insert("generator", Item::ArrayOfTables(a));
        }
    }
}

/// The member ends of structure `t` whose bolts are provisional (the
/// layout's, until the code step chooses bolts): a tower with any stays at
/// level 3 for the joint details (owner 2026-11-18, question 6) — read by
/// the core since the third round
/// ([`ohl_structures::StructureType::provisional_bolts`], so that every
/// application and the command line give level 3).
pub fn provisional_bolts(t: &StructureType) -> Vec<(String, String)> {
    t.provisional_bolts()
}

/// One joint's outcome of a run.
#[derive(Debug, Clone, PartialEq)]
pub struct JointOutcome {
    /// The joint.
    pub joint: String,
    /// Its plan (as the engine made it).
    pub plan: JointPlan,
    /// Laid out and written (false: left as it was).
    pub written: bool,
    /// Why it was left for a hand detail (the plan's, or a clash after).
    pub hand: Vec<String>,
    /// The members turned flat for it.
    pub turned: Vec<String>,
    /// The plates written for it (ids).
    pub plates: Vec<String>,
}

/// A run of the layout on a structure file.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutRun {
    /// The file after (the same text when nothing changed).
    pub text: String,
    /// Each joint asked, in the file's order.
    pub joints: Vec<JointOutcome>,
    /// Clashes at the joints asked before and after (before: 0 in a
    /// follow, [`follow`], which does not say them).
    pub clashes: (usize, usize),
    /// Notes on the run (members turned, rolls refused).
    pub notes: Vec<String>,
    /// How long it took, s.
    pub seconds: f64,
}

impl LayoutRun {
    /// The joints laid out (written).
    pub fn laid_out(&self) -> impl Iterator<Item = &JointOutcome> {
        self.joints.iter().filter(|j| j.written)
    }

    /// The joints left for a hand detail.
    pub fn for_hand(&self) -> impl Iterator<Item = &JointOutcome> {
        self.joints.iter().filter(|j| !j.hand.is_empty())
    }
}

/// What a run is asked to do.
#[derive(Debug, Clone)]
pub struct RunAsk<'a> {
    /// The section library.
    pub library: &'a SectionLibrary,
    /// The bolt library.
    pub bolts: &'a BoltLibrary,
    /// The settings.
    pub settings: &'a LayoutSettings,
    /// The presets' fields by class (written for an end given a preset's
    /// bolts): class → (name, fields as a connection's).
    pub preset_fields: BTreeMap<String, Table>,
    /// The joints to lay out (none: every joint).
    pub joints: Option<BTreeSet<String>>,
    /// Values given by hand replaced too.
    pub replace_hand: bool,
}

/// The layout run on `text` (a structure file) as `ask` says.
pub fn lay_out(text: &str, ask: &RunAsk<'_>) -> Result<LayoutRun, String> {
    lay_out_with(text, ask, true)
}

/// [`lay_out`] with the joints in detail from the whole file, never from
/// the file cut down to what they read (`reduce`): the same
/// result, slower on a large tower; for the comparison of the two.
#[doc(hidden)]
pub fn lay_out_whole(text: &str, ask: &RunAsk<'_>) -> Result<LayoutRun, String> {
    lay_out_with(text, ask, false)
}

/// [`lay_out`]; `cut`: a few joints of a large tower in detail from the
/// file cut down to what they read (the same details, much quicker).
fn lay_out_with(text: &str, ask: &RunAsk<'_>, cut: bool) -> Result<LayoutRun, String> {
    let start = std::time::Instant::now();
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    let doc0: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    lay_out_read(
        &t,
        LayoutRecord::of(&t),
        doc0,
        ask,
        (cut, true),
        start,
        false,
    )
}

/// The layout's planner **relaxed** (O45 A1, owner 2026-11-19): the
/// joints asked planned as a run plans them — the automatic values taken
/// out, the members turned flat where that helps, each joint searched —
/// and nothing written: no clash rounds, the file's text unchanged, every
/// outcome `written = false` with its plan as the planner made it (the
/// planned shift kept even where a run would not write the joint). The
/// estimated eccentricities read these plans
/// ([`crate::eccentricities::estimate`]); never shown as a laid-out joint.
/// Deterministic: the same file and settings give the same plans.
pub fn plan_relaxed(text: &str, ask: &RunAsk<'_>) -> Result<LayoutRun, String> {
    let start = std::time::Instant::now();
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    let doc0: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    lay_out_read(
        &t,
        LayoutRecord::of(&t),
        doc0,
        ask,
        (true, false),
        start,
        true,
    )
}

/// [`lay_out_with`] on the file read already: `t` as the structure (its
/// layout record `record`, which may be newer than `t`'s), `doc0` as the
/// text (the same file, with `record`).
fn lay_out_read(
    t: &StructureType,
    record: LayoutRecord,
    doc0: DocumentMut,
    ask: &RunAsk<'_>,
    // Cut down ([`lay_out_with`]); the clashes before counted (a follow
    // does not say them).
    (cut, count): (bool, bool),
    start: std::time::Instant,
    // The planner relaxed ([`plan_relaxed`]): planned, nothing written.
    relaxed: bool,
) -> Result<LayoutRun, String> {
    lay_out_once(t, record, doc0, ask, (cut, count), start, relaxed)
}

/// [`lay_out_read`]'s work.
fn lay_out_once(
    t: &StructureType,
    record: LayoutRecord,
    doc0: DocumentMut,
    ask: &RunAsk<'_>,
    (cut, count): (bool, bool),
    start: std::time::Instant,
    relaxed: bool,
) -> Result<LayoutRun, String> {
    // The structure's default minimum bolt distances (third round,
    // 2026-11-18): for the ends' pitch and end distance not given and the
    // edge distance where no code gives one.
    let settings = LayoutSettings {
        defaults: t.default_bolt_minimums.unwrap_or_default(),
        ..ask.settings.clone()
    };
    let targets: BTreeSet<String> = match &ask.joints {
        Some(j) => j.clone(),
        None => t.nodes.iter().map(|n| n.id.clone()).collect(),
    };
    // The members at the joints asked and their other joints (their ends
    // there: flat or not, laid out or not).
    let members_here: BTreeMap<&str, &[String; 2]> = t
        .members
        .iter()
        .filter(|m| m.nodes.iter().any(|n| targets.contains(n)))
        .map(|m| (m.id.as_str(), &m.nodes))
        .collect();
    let around: BTreeSet<String> = members_here
        .values()
        .flat_map(|n| n.iter().cloned())
        .chain(targets.iter().cloned())
        .collect();
    // The joints `joints` in detail from the file cut down to what they
    // read, when that is quicker ([`crate::reduce`]; the members kept
    // found once: the run changes no member's joints, and what an end
    // bolts onto stays at its joint).
    let keep_for = |joints: &BTreeSet<String>| {
        if !cut {
            return None;
        }
        let k = crate::reduce::members_for(&doc0, joints);
        crate::reduce::worth(&doc0, &k).then_some(k)
    };
    let keep_targets = keep_for(&targets);
    let keep_around = keep_for(&around);
    let text_of = |doc: &DocumentMut, keep: &Option<BTreeSet<String>>| -> String {
        keep.as_ref().map_or_else(
            || doc.to_string(),
            |k| crate::reduce::cut_down(doc, k).to_string(),
        )
    };
    // 1. The automatic values (and hand ones when replaced) taken out (of
    // the file cut down to the joints and their neighbours when it is).
    let mut planning = match &keep_around {
        Some(k) => crate::reduce::cut_down(&doc0, k),
        None => doc0.clone(),
    };
    let hand_plates = strip(&mut planning, t, &record, &targets, ask.replace_hand);

    // (A few of them: from the file cut down to what those read.)
    let at = |doc: &DocumentMut, only: Option<&BTreeSet<String>>| {
        let some;
        let keep = match only {
            Some(o) => {
                some = keep_for(o);
                &some
            }
            None => &keep_targets,
        };
        joint_details_at(
            &text_of(doc, keep),
            ask.library,
            ask.bolts,
            Some(only.unwrap_or(&targets)),
        )
    };
    let clashes_before = if count {
        count_clashes(&at(&doc0, None)?, &targets)
    } else {
        0
    };
    // An end of `m` laid out at a joint not asked (with its roll now).
    let laid_elsewhere = |m: &str| -> Option<&str> {
        record
            .ends
            .keys()
            .find(|(x, j)| x == m && !targets.contains(j))
            .map(|(_, j)| j.as_str())
    };
    // The members the layout turned before, at the joints asked: back to
    // the roll they had (a run again starts from the same) — unless an end
    // of theirs not asked is laid out with that turn: it still helps
    // there, kept (owner 2026-11-18: turned only where it helps).
    let mut restored: BTreeMap<String, String> = BTreeMap::new();
    let mut held: BTreeSet<String> = BTreeSet::new();
    for (m, r) in &record.rolled {
        if !members_here.contains_key(m.as_str()) {
            continue;
        }
        if laid_elsewhere(m).is_some() {
            held.insert(m.clone());
        } else {
            restored.insert(m.clone(), r.clone());
        }
    }
    for (m, r) in &restored {
        put_roll(&mut planning, m, r);
    }

    // (Its model gives the code's edge distances when no member turns.)
    let (mut around_details, mut around_model) = crate::joints::joint_details_and_model(
        &text_of(&planning, &keep_around),
        ask.library,
        ask.bolts,
        Some(&around),
    )?;
    // The ends bolting onto nothing named, at the joints asked: the member
    // running through the joint named in the planning copy (as the layout
    // writes it), so that their leg bolted and the turn laying it flat are
    // those on that member; then the joints in detail again.
    // First (the owner's step 1) the members in line through each joint
    // joined: one part, or the smaller nested in the bigger.
    let mut named = false;
    let mut conns = connection_edits::Connections::of(&planning);
    let mut joins: BTreeMap<String, Vec<Join>> = BTreeMap::new();
    let mut taken = ids_in(&planning);
    for j in around_details
        .joints
        .iter()
        .filter(|j| targets.contains(&j.id))
    {
        // One part only where the file then reads them so (in line within
        // 1°, the same section, roll and reference: the core's rule).
        let js: Vec<Join> = crate::joint_layout::joins_of(j)
            .into_iter()
            .filter(|x| {
                x.how != JoinKind::Continuous
                    || t.continuous_problem(&x.member, &x.onto, &j.id).is_none()
            })
            .collect();
        for x in &js {
            write_join(&mut planning, &mut conns, &j.id, x, &mut taken);
            named = true;
        }
        let joined: BTreeSet<&str> = js.iter().map(|x| x.member.as_str()).collect();
        for (m, host) in crate::joint_layout::hosts_to_name(j) {
            let in_line = js.iter().any(|x| x.member == m || x.onto == m);
            if in_line || joined.contains(m.as_str()) {
                continue;
            }
            let _ = conns.set(
                &mut planning,
                &m,
                &j.id,
                "onto",
                Some(&FieldValue::Word(host)),
            );
            named = true;
        }
        if !js.is_empty() {
            joins.insert(j.id.clone(), js);
        }
    }
    // The members joined as one part are not turned (their rolls stay
    // alike).
    let one_part: BTreeSet<String> = joins
        .values()
        .flatten()
        .filter(|x| x.how == JoinKind::Continuous)
        .flat_map(|x| [x.member.clone(), x.onto.clone()])
        .collect();
    if named {
        (around_details, around_model) = crate::joints::joint_details_and_model(
            &text_of(&planning, &keep_around),
            ask.library,
            ask.bolts,
            Some(&around),
        )?;
    }
    // 2. The members lying off what they bolt onto: a turn that lays them
    // flat where every end agrees within the flatness allowed (those at
    // joints not asked too: a turn never spoils an end that was flat); a
    // roll given by hand kept unless replaced, a leg's or chord's turned
    // to the tower's faces (Orient angles) always.
    let mut notes = Vec::new();
    let member = |m: &str| t.members.iter().find(|x| x.id == m);
    let mut asked: BTreeMap<String, Vec<(String, f64)>> = BTreeMap::new();
    for j in around_details
        .joints
        .iter()
        .filter(|j| targets.contains(&j.id))
    {
        for (m, turn) in flat_turns(j) {
            if held.contains(&m) || one_part.contains(&m) {
                continue;
            }
            if record
                .oriented
                .get(&m)
                .is_some_and(|(_, k)| *k == crate::OrientKind::Chord)
            {
                notes.push(format!(
                    "{m}: lies off what it bolts onto at {}; a leg or chord turned to the tower's \
                     faces (Orient angles): kept",
                    j.id
                ));
                continue;
            }
            let automatic = record.rolled.contains_key(&m) || record.oriented.contains_key(&m);
            let by_hand = member(&m).is_some_and(|x| x.roll.is_some()) && !automatic;
            if by_hand && !ask.replace_hand {
                notes.push(format!(
                    "{m}: lies off what it bolts onto at {}, its roll given by hand kept",
                    j.id
                ));
                continue;
            }
            asked.entry(m).or_default().push((j.id.clone(), turn));
        }
    }
    let mut turned: BTreeMap<String, f64> = BTreeMap::new();
    for (m, ends) in asked.iter_mut() {
        // Its turns at all its ends (those flat already too: the turn
        // keeps them flat), each within a quarter turn of the first (a
        // turn and a half turn more lay a leg flat alike).
        for j in &around_details.joints {
            if ends.iter().any(|(x, _)| x == &j.id) {
                continue;
            }
            if let Some(f) = j
                .arrivals
                .iter()
                .find(|a| &a.member == m)
                .and_then(|a| a.flat)
            {
                ends.push((j.id.clone(), f.turn));
            }
        }
        let first = ends[0].1;
        for e in ends.iter_mut() {
            e.1 -= std::f64::consts::PI * ((e.1 - first) / std::f64::consts::PI).round();
        }
        // Within the flatness allowed of each other: the middle of them (each
        // end then within half their spread of flat), to a hundredth of a
        // degree as written.
        let lo = ends.iter().map(|e| e.1).fold(f64::INFINITY, f64::min);
        let hi = ends.iter().map(|e| e.1).fold(f64::NEG_INFINITY, f64::max);
        let mid = (((lo + hi) / 2.0).to_degrees() * 100.0).round() / 100.0;
        if (hi - lo).to_degrees() > crate::FLAT_OFF {
            notes.push(format!(
                "{m}: lies off what it bolts onto at {}, by different turns (one roll for \
                 both ends; the turn would spoil the other): not turned",
                ends.iter()
                    .map(|(j, x)| format!("{j} ({:.1}°)", x.to_degrees()))
                    .collect::<Vec<_>>()
                    .join(" and ")
            ));
            continue;
        }
        if mid.abs() >= 0.005
            && let Some(j) = laid_elsewhere(m)
        {
            notes.push(format!(
                "{m}: not turned: its end at {j} is laid out (not asked) with its roll now"
            ));
            continue;
        }
        turned.insert(m.clone(), mid.to_radians());
    }
    // A member's roll before this run (as written; radians).
    let roll_text = |m: &str| -> String {
        restored.get(m).cloned().unwrap_or_else(|| {
            member(m)
                .and_then(|x| x.roll.as_ref())
                .map(|v| match v {
                    toml::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_default()
        })
    };
    let roll_of = |m: &str| -> f64 {
        let text = roll_text(m);
        if text.is_empty() {
            return 0.0;
        }
        toml::Value::String(text)
            .try_into::<ohl_core::units::Angle>()
            .map_or(0.0, |a| a.radians())
    };
    // The joints asked in detail with `turns` (their model: the code's edge
    // distances), each planned.
    let planned_with = |turns: &BTreeMap<&String, f64>| -> Result<Planned, String> {
        let mut d = planning.clone();
        for (m, turn) in turns {
            ohl_structure_edit::edits::set_member_roll(&mut d, m, roll_of(m) + turn);
        }
        let (details, model) = crate::joints::joint_details_and_model(
            &text_of(&d, &keep_targets),
            ask.library,
            ask.bolts,
            Some(&targets),
        )?;
        let plans = plans_of(&details, model.as_ref(), &targets, &settings, &joins);
        Ok((details, model, plans))
    };
    // 2b. A half turn where an end lies on the inner face of what it bolts
    // onto and cannot be laid out there (it runs towards the angle's other
    // leg, or its bolts miss the flange): its outstanding leg to the other
    // side, so that it lies on the outer face (an X-brace's diagonals: one
    // on the outer faces of the legs, the other on their inner faces, back
    // to back where they cross). Also where an end no roll lays flat has
    // its leg bolted facing away from the face its welded plate would lie
    // on (step 4, 2026-11-19: its outstanding leg towards the flange).
    // Kept for a member only when more of its ends are planned laid out
    // with it and its joints no worse; the rest as for any turn (tuning on
    // real towers, 2026-11-18).
    let mut first: Option<Planned> = None;
    {
        let turns: BTreeMap<&String, f64> = turned.iter().map(|(m, x)| (m, *x)).collect();
        let base = planned_with(&turns)?;
        let mut flips: BTreeSet<String> = BTreeSet::new();
        for (_, p) in &base.2 {
            for h in &p.hand {
                let Some((m, why)) = h.split_once(": ") else {
                    continue;
                };
                let inner = why.contains("runs towards the angle's other leg")
                    || why.contains("do not all land on the inner face")
                    || why.contains("its outstanding leg points towards the flange");
                if !inner
                    || held.contains(m)
                    || one_part.contains(m)
                    || !members_here.contains_key(m)
                {
                    continue;
                }
                if record
                    .oriented
                    .get(m)
                    .is_some_and(|(_, k)| *k == crate::OrientKind::Chord)
                {
                    continue;
                }
                let automatic = record.rolled.contains_key(m) || record.oriented.contains_key(m);
                let by_hand = member(m).is_some_and(|x| x.roll.is_some()) && !automatic;
                if (by_hand && !ask.replace_hand) || laid_elsewhere(m).is_some() {
                    continue;
                }
                flips.insert(m.to_owned());
            }
        }
        let mut chosen: BTreeSet<String> = BTreeSet::new();
        if !flips.is_empty() {
            // Half a turn more, as written: degrees to a hundredth.
            let half = |x: f64| {
                let mut d = x.to_degrees() + 180.0;
                if d > 180.0 {
                    d -= 360.0;
                }
                ((d * 100.0).round() / 100.0).to_radians()
            };
            let mut with = turned.clone();
            for m in &flips {
                let x = with.get(m).copied().unwrap_or(0.0);
                with.insert(m.clone(), half(x));
            }
            let turns: BTreeMap<&String, f64> = with.iter().map(|(m, x)| (m, *x)).collect();
            let flipped = planned_with(&turns)?;
            // Ends laid out (planned) of member `m`, and at its joints.
            let count = |plans: &[(String, JointPlan)], m: &str| -> (usize, usize) {
                let joints = &members_here[m];
                plans.iter().filter(|(j, _)| joints.contains(j)).fold(
                    (0, 0),
                    |(own, all), (_, p)| {
                        (
                            own + usize::from(p.ends.iter().any(|e| e.member == m)),
                            all + p.ends.len(),
                        )
                    },
                )
            };
            for m in &flips {
                let (own0, all0) = count(&base.2, m);
                let (own1, all1) = count(&flipped.2, m);
                if own1 > own0 && all1 >= all0 {
                    chosen.insert(m.clone());
                }
            }
            for m in &chosen {
                let x = turned.get(m).copied().unwrap_or(0.0);
                turned.insert(m.clone(), half(x));
            }
            if chosen == flips {
                first = Some(flipped);
            }
        }
        if chosen.is_empty() {
            first = Some(base);
        }
    }
    // 3.–4. Each joint planned with the turns and written; a turned member
    // none of whose joints is laid out with the turn is not turned (its
    // roll as before), and the joints planned again without it — until
    // every turn left helps.
    let class_rank = |m: &str| -> u8 {
        match member(m)
            .and_then(|x| x.design.as_ref())
            .and_then(|d| d.get("class"))
            .and_then(toml::Value::as_str)
        {
            Some("leg") => 0,
            Some("bracing") | Some("crossing") => 1,
            Some("redundant") => 2,
            _ => 3,
        }
    };
    let mut refused: BTreeSet<String> = BTreeSet::new();
    // The joints as the last pass's clash check left them, and those the
    // turns refused since then touch (checked again).
    let mut memo: BTreeMap<String, Checked> = BTreeMap::new();
    let mut touched: BTreeSet<String> = BTreeSet::new();
    let mut joint_members: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (m, nodes) in &members_here {
        for n in nodes.iter() {
            joint_members.entry(n.as_str()).or_default().push(m);
        }
    }
    loop {
        let turns: BTreeMap<&String, f64> = turned
            .iter()
            .filter(|(m, _)| !refused.contains(*m))
            .map(|(m, x)| (m, *x))
            .collect();
        let mut turned_at: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for m in turns.keys() {
            for j in members_here[m.as_str()].iter() {
                if targets.contains(j) {
                    turned_at.entry(j.clone()).or_default().push((*m).clone());
                }
            }
        }
        // The joints asked in detail with the turns, each planned (those
        // of the half turns' check when the turns are the same).
        let fresh;
        let (details, model, plans) = match (turns.is_empty(), refused.is_empty(), &first) {
            (true, _, _) => (
                &around_details,
                around_model.as_ref(),
                plans_of(
                    &around_details,
                    around_model.as_ref(),
                    &targets,
                    &settings,
                    &joins,
                ),
            ),
            (false, true, Some(p)) => (&p.0, p.1.as_ref(), p.2.clone()),
            _ => {
                fresh = planned_with(&turns)?;
                (&fresh.0, fresh.1.as_ref(), fresh.2.clone())
            }
        };
        let mut outcomes: Vec<JointOutcome> = plans
            .into_iter()
            .map(|(j, plan)| JointOutcome {
                hand: plan.hand.clone(),
                written: false,
                turned: turned_at.get(&j).cloned().unwrap_or_default(),
                plates: Vec::new(),
                joint: j,
                plan,
            })
            .collect();
        // A joint planned again with members left for a hand detail (the
        // clash check's rounds).
        let replan = |joint: &str,
                      except: &BTreeMap<String, String>,
                      floors: &BTreeMap<String, (f64, String)>|
         -> Option<JointPlan> {
            let j = details.joint(joint)?;
            let edge = edge_rule(model);
            let mut p = crate::joint_layout::plan_joint_with(j, &settings, &edge, except, floors);
            p.joins = joins
                .get(joint)
                .into_iter()
                .flatten()
                .filter(|x| !except.contains_key(&x.member) && !except.contains_key(&x.onto))
                .cloned()
                .collect();
            for x in joins.get(joint).into_iter().flatten() {
                for m in [&x.member, &x.onto] {
                    if let Some(why) = except.get(m)
                        && !p.hand_members.contains(m)
                    {
                        p.hand_members.push(m.clone());
                        p.hand.push(format!("{m}: {why}"));
                    }
                }
            }
            Some(p)
        };
        // A turn that helps no joint is not made: the member among the
        // ends laid out at none of its joints — as planned first (nothing
        // written yet), then as written (a joint taken back by the clash
        // check).
        let unhelpful = |outcomes: &[JointOutcome], written: bool| -> Vec<String> {
            turns
                .keys()
                .filter(|m| {
                    !outcomes.iter().any(|o| {
                        (o.written || !written) && o.plan.ends.iter().any(|e| e.member == ***m)
                    })
                })
                .map(|m| (*m).clone())
                .collect()
        };
        let mut useless = unhelpful(&outcomes, false);
        // Relaxed (O45 A1): the plans with the turns that help, nothing
        // written.
        if relaxed && useless.is_empty() {
            return Ok(LayoutRun {
                text: doc0.to_string(),
                joints: outcomes,
                clashes: (0, 0),
                notes,
                seconds: start.elapsed().as_secs_f64(),
            });
        }
        let mut done = None;
        if useless.is_empty() {
            let written = write_rounds(
                &Rounds {
                    doc0: &doc0,
                    t,
                    record: &record,
                    ask,
                    targets: &targets,
                    restored: &restored,
                    turns: &turns,
                    roll_of: &roll_of,
                    roll_text: &roll_text,
                    hand_plates: &hand_plates,
                    at: &at,
                    replan: &replan,
                    class_rank: &class_rank,
                },
                &mut outcomes,
                &mut memo,
                &targets.difference(&touched).cloned().collect(),
            )?;
            touched.clear();
            useless = unhelpful(&outcomes, true);
            done = Some(written);
        }
        if !useless.is_empty() {
            for m in useless {
                let at_joints: Vec<&str> = members_here[m.as_str()]
                    .iter()
                    .filter(|j| targets.contains(*j))
                    .map(String::as_str)
                    .collect();
                notes.push(format!(
                    "{m}: not turned: no joint of it is laid out with the turn ({} left for a \
                     hand detail); its roll as before",
                    at_joints.join(" and ")
                ));
                // Its joints and those of the members meeting there (their
                // ends' conditions read each other's).
                for j in members_here[m.as_str()].iter() {
                    for k in joint_members.get(j.as_str()).into_iter().flatten() {
                        touched.extend(
                            members_here[*k]
                                .iter()
                                .filter(|x| targets.contains(*x))
                                .cloned(),
                        );
                    }
                    if targets.contains(j) {
                        touched.insert(j.clone());
                    }
                }
                refused.insert(m);
            }
            continue;
        }
        let Some((result, after_count)) = done else {
            continue;
        };
        for m in restored.keys() {
            if !turns.contains_key(m) {
                notes.push(format!(
                    "{m}: its automatic roll taken back (no joint of it laid out with it)"
                ));
            }
        }
        return Ok(LayoutRun {
            text: result,
            joints: outcomes,
            clashes: (clashes_before, after_count),
            notes,
            seconds: start.elapsed().as_secs_f64(),
        });
    }
}

/// The joints asked in detail, their model and each one's plan.
type Planned = (
    JointDetails,
    Option<ohl_structures_fe::model::Model>,
    Vec<(String, JointPlan)>,
);

/// The plan of each joint of `details` among `targets` (in its order), the
/// code's edge distances from `model`.
fn plans_of(
    details: &JointDetails,
    model: Option<&ohl_structures_fe::model::Model>,
    targets: &BTreeSet<String>,
    settings: &LayoutSettings,
    joins: &BTreeMap<String, Vec<Join>>,
) -> Vec<(String, JointPlan)> {
    let edge = edge_rule(model);
    details
        .joints
        .iter()
        .filter(|j| targets.contains(&j.id))
        .map(|j| {
            let mut p = plan_joint(j, settings, &edge);
            p.joins = joins.get(&j.id).cloned().unwrap_or_default();
            (j.id.clone(), p)
        })
        .collect()
}

/// Join `x` at joint `j` written into `doc` (its end's `kind` and `onto`,
/// or `face` and `onto`); the fields written.
fn write_join(
    doc: &mut DocumentMut,
    conns: &mut connection_edits::Connections,
    j: &str,
    x: &Join,
    taken: &mut std::collections::HashSet<String>,
) -> (Vec<&'static str>, Option<String>) {
    let (f, v) = match x.how {
        JoinKind::Continuous => ("kind", "continuous"),
        JoinKind::Nested => ("face", "nested"),
        JoinKind::Spliced => {
            // Step 1c: a splice of covers, outer and inner on each leg
            // bolted, both ends onto it (the first free id).
            let id = (1u64..)
                .map(|n| format!("S{n}"))
                .find(|x| !taken.contains(x))
                .expect("a free id");
            taken.insert(id.clone());
            let mut sp = Table::new();
            sp.insert("id", toml_edit::value(id.as_str()));
            sp.insert("joint", toml_edit::value(j));
            let mut list = Array::new();
            list.push(x.member.as_str());
            list.push(x.onto.as_str());
            sp.insert("members", toml_edit::value(list));
            let mut covers = ArrayOfTables::new();
            for leg in &x.covers {
                for face in ohl_structures::connections::CoverFace::ALL {
                    let mut c = Table::new();
                    c.insert("face", toml_edit::value(face.as_str()));
                    c.insert("leg", toml_edit::value(leg.as_str()));
                    c.insert("thickness", toml_edit::value(crate::mm_text(x.thickness)));
                    covers.push(c);
                }
            }
            sp.insert("cover", Item::ArrayOfTables(covers));
            if doc
                .get("splice")
                .and_then(Item::as_array_of_tables)
                .is_none()
            {
                doc.insert("splice", Item::ArrayOfTables(ArrayOfTables::new()));
            }
            doc["splice"]
                .as_array_of_tables_mut()
                .expect("made above")
                .push(sp);
            for m in [&x.member, &x.onto] {
                let _ = conns.set(doc, m, j, "onto", Some(&FieldValue::Word(id.clone())));
            }
            return (vec!["onto"], Some(id));
        }
    };
    let _ = conns.set(doc, &x.member, j, f, Some(&FieldValue::Word(v.into())));
    let _ = conns.set(
        doc,
        &x.member,
        j,
        "onto",
        Some(&FieldValue::Word(x.onto.clone())),
    );
    (vec![f, "onto"], None)
}

/// The ids in use in `doc`: its plates', splices' and members'.
fn ids_in(doc: &DocumentMut) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for key in ["plate", "splice", "member"] {
        if let Some(a) = doc.get(key).and_then(Item::as_array_of_tables) {
            out.extend(
                a.iter()
                    .filter_map(|x| x.get("id").and_then(Item::as_str).map(str::to_owned)),
            );
        }
    }
    out
}

/// What the layout made, `id` at a joint, taken out of `doc` and `rec`: a
/// plate; a splice (step 1c) as it made it — one changed by hand since
/// (its covers not those written) is kept, the user's, its members' ends
/// onto it with it. Whether it was taken out.
fn take_out_made(doc: &mut DocumentMut, rec: &mut LayoutRecord, id: &str) -> bool {
    let Some((joint, fields)) = rec.made.remove(id) else {
        return false;
    };
    if !fields.contains(SPLICE) {
        connection_edits::remove_plate(doc, id);
        return true;
    }
    let cover = fields
        .iter()
        .find_map(|f| f.strip_prefix("cover "))
        .unwrap_or_default()
        .to_owned();
    let table = doc
        .get("splice")
        .and_then(Item::as_array_of_tables)
        .and_then(|a| {
            a.iter()
                .find(|x| x.get("id").and_then(Item::as_str) == Some(id))
        });
    let intact = table.is_some_and(|x| {
        x.get("cover")
            .and_then(Item::as_array_of_tables)
            .is_some_and(|c| {
                c.iter().all(|c| {
                    c.get("thickness").and_then(Item::as_str) == Some(cover.as_str())
                        && c.iter()
                            .all(|(k, _)| matches!(k, "face" | "leg" | "thickness"))
                })
            })
    });
    if intact {
        connection_edits::remove_splice(doc, id);
        return true;
    }
    // Changed by hand: kept, with its members' ends onto it.
    let members: Vec<String> = table
        .and_then(|x| x.get("members"))
        .and_then(Item::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    for m in members {
        if let Some(f) = rec.ends.get_mut(&(m, joint.clone())) {
            f.remove("onto");
        }
    }
    false
}

/// The code's edge and end distances of a member end's bolts in a part
/// of a thickness, from `model` (none: no model, or no rule).
fn edge_rule(
    model: Option<&ohl_structures_fe::model::Model>,
) -> impl Fn(&crate::Arrival, f64) -> Option<(f64, f64)> + '_ {
    move |a: &crate::Arrival, thick: f64| -> Option<(f64, f64)> {
        let m = model?;
        let idx = m.members.iter().position(|x| x.id == a.member)?;
        crate::member_view::rules_in(m, idx, a.end, None, Some((thick, None)))
            .map(|r| (r.edge, r.end))
    }
}

/// The joints of a file in detail (those given, else all asked).
type DetailsOf<'a> =
    dyn Fn(&DocumentMut, Option<&BTreeSet<String>>) -> Result<JointDetails, String> + 'a;

/// A joint planned again with the members given left for a hand detail,
/// each with why.
type Replan<'a> = dyn Fn(&str, &BTreeMap<String, String>, &BTreeMap<String, (f64, String)>) -> Option<JointPlan>
    + 'a;

/// What [`write_rounds`] writes with.
struct Rounds<'a> {
    doc0: &'a DocumentMut,
    t: &'a StructureType,
    record: &'a LayoutRecord,
    ask: &'a RunAsk<'a>,
    targets: &'a BTreeSet<String>,
    /// The rolls turned before at the joints asked, back as written.
    restored: &'a BTreeMap<String, String>,
    /// This run's turns.
    turns: &'a BTreeMap<&'a String, f64>,
    roll_of: &'a dyn Fn(&str) -> f64,
    roll_text: &'a dyn Fn(&str) -> String,
    hand_plates: &'a [OldPlate],
    /// The joints asked in detail (those given, else all asked).
    at: &'a DetailsOf<'a>,
    /// A joint planned again with members left for a hand detail.
    replan: &'a Replan<'a>,
    /// A member's class rank (leg, bracing, redundant, other).
    class_rank: &'a dyn Fn(&str) -> u8,
}

/// How many times a joint is planned again without the members whose
/// laid-out parts would clash, before it is taken back whole.
const CLASH_ROUNDS: usize = 6;

/// How many times an end laid out is pulled back past what it would run
/// into before it is left as it was (J1, 2026-11-19).
const PULLS: usize = 3;

/// The plans of `outcomes` written into the file as it was, with the rolls
/// (those turned before at the joints asked back, this run's turns on
/// them); then the clash check: a joint whose laid-out parts run into
/// anything is planned again without the members in those clashes (left
/// as they were, with the clash; tuning on real towers, 2026-11-18: one
/// member's clash no longer takes the whole joint back), up to
/// [`CLASH_ROUNDS`] times, then taken back whole. The file after and the
/// clashes at the joints asked after.
///
/// `memo`: each joint as an earlier pass of the run left it; a joint of
/// `reuse` planned as then (its members' rolls the same) is taken as then,
/// not checked again (the same joint gives the same check). Updated.
fn write_rounds(
    r: &Rounds<'_>,
    outcomes: &mut [JointOutcome],
    memo: &mut BTreeMap<String, Checked>,
    reuse: &BTreeSet<String>,
) -> Result<(String, usize), String> {
    let ask = r.ask;
    let mut take_back: BTreeSet<String> = BTreeSet::new();
    // Each joint's clashes after (the joints asked), and the joints to
    // check in the next round (none: all of them).
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    // The members left at each joint for a clash, with why.
    let mut except: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    // The least cuts the clashes asked for at each joint (J1: an end
    // pulled back past what it ran into before it is left), and how many
    // times each end was so pulled back.
    let mut floors: BTreeMap<String, BTreeMap<String, (f64, String)>> = BTreeMap::new();
    let mut pulls: BTreeMap<(String, String), usize> = BTreeMap::new();
    // The plates' outlines cut at the heel, by joint and plate (kept while
    // the joint's plan stays).
    let mut outlines: BTreeMap<(String, usize), Vec<[f64; 3]>> = BTreeMap::new();
    // The plans as planned, and the joints taken as an earlier pass left
    // them.
    let planned: Vec<JointPlan> = outcomes.iter().map(|o| o.plan.clone()).collect();
    let mut known: BTreeSet<String> = BTreeSet::new();
    for o in outcomes.iter_mut() {
        let Some(c) = memo
            .get(&o.joint)
            .filter(|c| reuse.contains(&o.joint) && c.planned == o.plan)
        else {
            continue;
        };
        o.plan = c.plan.clone();
        if c.taken_back {
            take_back.insert(o.joint.clone());
            o.hand = c.hand.clone();
        }
        except.insert(o.joint.clone(), c.except.clone());
        floors.insert(o.joint.clone(), c.floors.clone());
        for (k, x) in &c.outlines {
            outlines.insert((o.joint.clone(), *k), x.clone());
        }
        counts.insert(o.joint.clone(), c.clashes);
        known.insert(o.joint.clone());
    }
    let mut check: Option<BTreeSet<String>> = (!known.is_empty()).then(|| {
        r.targets
            .iter()
            .filter(|j| !known.contains(*j))
            .cloned()
            .collect()
    });
    let mut round = 0;
    let result = loop {
        let mut doc = r.doc0.clone();
        let mut rec = r.record.clone();
        rec.settings = Some(RecordSettings::of(ask.settings));
        // The rolls (the members of the joints asked): those turned before
        // back, then the turns of this run.
        for (m, roll) in r.restored {
            put_roll(&mut doc, m, roll);
            rec.rolled.remove(m);
        }
        for (m, turn) in r.turns {
            ohl_structure_edit::edits::set_member_roll(&mut doc, m, (r.roll_of)(m) + turn);
            rec.rolled.insert((*m).clone(), (r.roll_text)(m));
        }
        let mut w = Writing::of(&doc, r.t);
        // First the plates the layout made at the joints written or cleared
        // now taken out, so their ids are free (the ids a run gives do not
        // hang on a run before's: a run again identical); then each such
        // joint's old automatic values out and its plan written.
        let going: Vec<String> = rec
            .made
            .iter()
            .filter(|(_, (j, _))| {
                outcomes.iter().any(|o| {
                    &o.joint == j
                        && ((o.plan.laid_out() && !take_back.contains(&o.joint))
                            || (o.plan.ends.is_empty() && o.plan.hand.is_empty()))
                })
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in &going {
            if take_out_made(&mut doc, &mut rec, id) {
                w.taken.remove(id);
            }
        }
        if !going.is_empty() {
            w.conns = connection_edits::Connections::of(&doc);
        }
        for o in outcomes.iter_mut() {
            o.written = false;
            o.plates.clear();
            if !take_back.contains(&o.joint) {
                o.hand = o.plan.hand.clone();
            }
            if !o.plan.laid_out() || take_back.contains(&o.joint) {
                if o.plan.ends.is_empty() && o.plan.hand.is_empty() {
                    // Nothing to lay out: the old automatic values go.
                    strip_joint(&mut doc, r.t, &mut rec, &o.joint, ask.replace_hand, &mut w);
                    rec.joints.remove(&o.joint);
                    rec.hand.remove(&o.joint);
                } else if !o.hand.is_empty() {
                    rec.hand.insert(o.joint.clone(), o.hand.join("; "));
                }
                continue;
            }
            strip_joint(&mut doc, r.t, &mut rec, &o.joint, ask.replace_hand, &mut w);
            o.plates = write_plan(&mut doc, r.t, &mut rec, &o.plan, ask, r.hand_plates, &mut w);
            o.written = true;
            rec.joints.insert(o.joint.clone());
            if o.plan.hand.is_empty() {
                rec.hand.remove(&o.joint);
            } else {
                rec.hand.insert(o.joint.clone(), o.plan.hand.join("; "));
            }
        }
        w.conns.tidy(&mut doc);
        // The plates' outlines worked out round their bolts, cut at the
        // flange's heel by half the gap (clear of the other face's), written
        // (automatic); those of a joint planned as before as before.
        let heels: Vec<OutlinePass> = outcomes
            .iter()
            .filter(|o| o.written)
            .flat_map(|o| {
                o.plates
                    .iter()
                    .zip(&o.plan.plates)
                    .enumerate()
                    .filter(|(_, (_, p))| p.heel.is_some() || p.reach.is_some())
                    .map(|(k, (id, p))| ((o.joint.clone(), k), id.clone(), p.heel, p.reach.clone()))
                    .collect::<Vec<_>>()
            })
            .collect();
        let new: BTreeSet<String> = heels
            .iter()
            .filter(|(key, _, _, _)| !outlines.contains_key(key))
            .map(|(key, _, _, _)| key.0.clone())
            .collect();
        if !new.is_empty() {
            // The record first: the drawing reads its marks.
            rec.write(&mut doc);
            let mid = (r.at)(&doc, Some(&new))?;
            for (key, id, heel, reach) in &heels {
                if outlines.contains_key(key) {
                    continue;
                }
                let Some(p) = mid
                    .joints
                    .iter()
                    .flat_map(|j| &j.plates)
                    .find(|p| &p.id == id)
                else {
                    continue;
                };
                // Step 4: round the seams of the plates welded to it in
                // their members' own planes too, on its side of each.
                let mut outline = p.outline.clone();
                if let Some(rc) = reach {
                    let mut pts = outline.clone();
                    pts.extend(rc.points.iter().copied());
                    let along = sub3(rc.points[1], rc.points[0]);
                    outline = crate::joints::face_hull(&pts, rc.points[0], rc.normal, along, 0.0);
                    for (at, side) in &rc.keep_to {
                        outline = clip3(&outline, |q| dot3(sub3(q, *at), *side) + 1e-9);
                    }
                }
                let half = ask.settings.gap / 2.0;
                let cut = match heel {
                    Some((heel, across)) => clip3(&outline, |q| {
                        (0..3).map(|k| (q[k] - heel[k]) * across[k]).sum::<f64>() - half
                    }),
                    None => outline,
                };
                outlines.insert(key.clone(), cut);
            }
        }
        for (key, id, _, _) in &heels {
            if let Some(cut) = outlines.get(key)
                && cut.len() >= 3
                && connection_edits::set_plate_outline(&mut doc, id, cut).is_ok()
                && let Some(m) = rec.made.get_mut(id)
            {
                m.1.insert("outline".into());
            }
        }
        rec.write(&mut doc);
        if check.as_ref().is_some_and(BTreeSet::is_empty) {
            break doc;
        }
        let after = (r.at)(&doc, check.as_ref())?;
        for j in &after.joints {
            if r.targets.contains(&j.id) {
                counts.insert(j.id.clone(), j.clashes.len());
            }
        }
        let mut next: BTreeSet<String> = BTreeSet::new();
        for o in outcomes.iter_mut().filter(|o| o.written) {
            if check.as_ref().is_some_and(|c| !c.contains(&o.joint)) {
                continue;
            }
            let Some(j) = after.joint(&o.joint) else {
                continue;
            };
            let laid: BTreeSet<&str> = o
                .plan
                .ends
                .iter()
                .map(|e| e.member.as_str())
                .chain(o.plan.joins.iter().map(|x| x.member.as_str()))
                .chain(
                    o.plan
                        .joins
                        .iter()
                        .filter(|x| x.how == JoinKind::Spliced)
                        .map(|x| x.onto.as_str()),
                )
                .collect();
            // The members laid out a part is of (a plate: those on it; a
            // cover of a splice it made: the two it joins).
            let ours = |x: &crate::ClashPart| -> Vec<String> {
                match x {
                    crate::ClashPart::Plate(id) => o
                        .plates
                        .iter()
                        .position(|p| p == id)
                        .map(|k| o.plan.plates[k].members.clone())
                        .unwrap_or_default(),
                    crate::ClashPart::Cover(id, _) => j
                        .splices
                        .iter()
                        .find(|s| &s.id == id)
                        .and_then(|s| {
                            o.plan.joins.iter().find(|x| {
                                x.how == JoinKind::Spliced
                                    && s.members.contains(&x.member)
                                    && s.members.contains(&x.onto)
                            })
                        })
                        .map(|x| vec![x.member.clone(), x.onto.clone()])
                        .unwrap_or_default(),
                    other => other
                        .member()
                        .filter(|m| laid.contains(m))
                        .map(|m| vec![m.to_owned()])
                        .unwrap_or_default(),
                }
            };
            // A member left for a hand detail is as it was: its clashes
            // are said with it, they do not take the rest back — unless
            // its place is kept free (J2, below).
            let left = |x: &crate::ClashPart| {
                x.member()
                    .is_some_and(|m| o.plan.hand_members.iter().any(|h| h == m))
            };
            // A plate it would make named by where it lies (its id is not
            // kept when the joint is left as it was).
            let name = |x: &crate::ClashPart| match x {
                crate::ClashPart::Plate(id) => o
                    .plates
                    .iter()
                    .position(|p| p == id)
                    .map(|k| format!("a plate on {}", o.plan.plates[k].on))
                    .unwrap_or_else(|| x.words()),
                other => other.words(),
            };
            let mut bad: Vec<String> = Vec::new();
            let mut out: BTreeMap<String, String> = BTreeMap::new();
            let mut pull: BTreeMap<String, (f64, String)> = BTreeMap::new();
            for c in &j.clashes {
                let (a, b) = (ours(&c.a), ours(&c.b));
                // J2 (2026-11-19): a member left for a hand detail keeps
                // its place: what is laid out may not run into it either.
                // (An end laid out, its steel or bolts: a plate may lie
                // under the member left — its hand detail may bolt it
                // there —, and the members joined first keep theirs.)
                let kept = |x: &crate::ClashPart| {
                    x.member()
                        .is_some_and(|m| o.plan.kept_free.iter().any(|h| h == m))
                };
                let end = |x: &crate::ClashPart| {
                    x.member()
                        .is_some_and(|m| o.plan.ends.iter().any(|e| e.member == m))
                };
                if (a.is_empty() && b.is_empty())
                    || (left(&c.a) && !(end(&c.b) && kept(&c.a)))
                    || (left(&c.b) && !(end(&c.a) && kept(&c.b)))
                {
                    continue;
                }
                let words = format!(
                    "{} and {} would run {:.0} mm into each other",
                    name(&c.a),
                    name(&c.b),
                    (c.depth * 1000.0).max(1.0)
                );
                // Left: the side laid out; of two, a member before a
                // plate's members, then the later class (redundants before
                // bracing), then the larger id.
                let worse = |x: &[String]| {
                    (
                        x.len() == 1,
                        x.iter().map(|m| (r.class_rank)(m)).max().unwrap_or(0),
                        x.iter().max().cloned().unwrap_or_default(),
                    )
                };
                let pick_a = !a.is_empty() && (b.is_empty() || worse(&a) >= worse(&b));
                let side = if pick_a { a } else { b };
                let why = format!("laid out, {words} (left as it was)");
                // J1: first pulled back past what it ran into (a few times,
                // while rounds are left), then left as it was.
                for m in side {
                    let cut = o
                        .plan
                        .ends
                        .iter()
                        .find(|e| e.member == m)
                        .and_then(|e| e.cut);
                    let dir = j
                        .arrivals
                        .iter()
                        .find(|x| x.member == m)
                        .map(|x| x.direction);
                    let n = pulls
                        .get(&(o.joint.clone(), m.clone()))
                        .copied()
                        .unwrap_or(0);
                    match (cut, dir) {
                        (Some(cut), Some(d)) if n < PULLS && round + 2 < CLASH_ROUNDS => {
                            let along = dot3(sub3(c.at, j.position), d);
                            let f = (cut.max(along) + c.depth + ask.settings.gap).max(cut + 0.001);
                            let f = (f * 1000.0 - 1e-9).ceil() / 1000.0;
                            let e = pull.entry(m).or_insert((f, why.clone()));
                            if f > e.0 {
                                *e = (f, why.clone());
                            }
                        }
                        _ => {
                            out.entry(m).or_insert_with(|| why.clone());
                        }
                    }
                }
                bad.push(words);
            }
            if bad.is_empty() {
                continue;
            }
            for m in out.keys() {
                pull.remove(m);
            }
            let fl = floors.entry(o.joint.clone()).or_default();
            for (m, f) in pull {
                *pulls.entry((o.joint.clone(), m.clone())).or_default() += 1;
                fl.insert(m, f);
            }
            let ex = except.entry(o.joint.clone()).or_default();
            ex.extend(out);
            let again = (round + 1 < CLASH_ROUNDS)
                .then(|| (r.replan)(&o.joint, ex, fl))
                .flatten()
                .filter(JointPlan::laid_out);
            match again {
                Some(p) => {
                    outlines.retain(|(j, _), _| j != &o.joint);
                    o.plan = p;
                }
                None => {
                    // A splice whose place is not known first (its legs
                    // drawn lapped, as given: often what the rest runs
                    // into).
                    o.hand = o
                        .plan
                        .left
                        .iter()
                        .filter(|w| w.contains(crate::joint_layout::SPLICE_UNKNOWN))
                        .cloned()
                        .chain(bad.into_iter().map(|w| format!("laid out: {w}")))
                        .chain(o.plan.hand.iter().cloned())
                        .collect();
                    take_back.insert(o.joint.clone());
                }
            }
            next.insert(o.joint.clone());
        }
        if next.is_empty() {
            break doc;
        }
        round += 1;
        check = Some(next);
    };
    // The plates, connections and generators written at the end of the
    // file, in that order and each in its list's order (not where an
    // earlier run's text had some of them): the same layout gives the same
    // text (a run again identical).
    let mut result = result;
    for key in ["splice", "plate", "connection", "generator"] {
        if let Some(mut item) = result.remove(key) {
            if let Some(a) = item.as_array_of_tables_mut() {
                for t in a.iter_mut() {
                    t.set_position(None);
                }
            }
            result.insert(key, item);
        }
    }
    let after_count = r
        .targets
        .iter()
        .map(|j| counts.get(j).copied().unwrap_or(0))
        .sum();
    for (o, p) in outcomes.iter().zip(planned) {
        memo.insert(
            o.joint.clone(),
            Checked {
                planned: p,
                plan: o.plan.clone(),
                hand: o.hand.clone(),
                taken_back: take_back.contains(&o.joint),
                except: except.get(&o.joint).cloned().unwrap_or_default(),
                floors: floors.get(&o.joint).cloned().unwrap_or_default(),
                outlines: outlines
                    .iter()
                    .filter(|((j, _), _)| j == &o.joint)
                    .map(|((_, k), x)| (*k, x.clone()))
                    .collect(),
                clashes: counts.get(&o.joint).copied().unwrap_or(0),
            },
        );
    }
    Ok((result.to_string(), after_count))
}

/// A joint as the clash check of a run's pass left it ([`write_rounds`]).
#[derive(Debug, Clone)]
struct Checked {
    /// Its plan as planned.
    planned: JointPlan,
    /// Its plan as written (members left for a clash).
    plan: JointPlan,
    /// Why it needs a hand detail.
    hand: Vec<String>,
    /// Taken back whole.
    taken_back: bool,
    /// The members left for a clash, with why.
    except: BTreeMap<String, String>,
    /// The least cuts its clashes asked for, with why.
    floors: BTreeMap<String, (f64, String)>,
    /// Its plates' outlines cut at the heel, by plate.
    outlines: Vec<(usize, Vec<[f64; 3]>)>,
    /// Its clashes after.
    clashes: usize,
}

/// Member `m`'s roll written as `text` (empty: none), exactly.
fn put_roll(doc: &mut DocumentMut, m: &str, text: &str) {
    if let Some(e) = doc
        .get_mut("member")
        .and_then(Item::as_array_of_tables_mut)
        .and_then(|a| {
            a.iter_mut()
                .find(|x| x.get("id").and_then(Item::as_str) == Some(m))
        })
    {
        if text.is_empty() {
            e.remove("roll");
        } else {
            e.insert("roll", toml_edit::value(text));
        }
    }
}

/// A flange's heel line: a point on it and across the flange (m, unit).
type HeelLine = ([f64; 3], [f64; 3]);

/// A plate whose outline the run works out after writing: (its joint and
/// index in the plan), its id, the flange's heel line it is cut at, what
/// it must also cover and keep to (step 4).
type OutlinePass = ((String, usize), String, Option<HeelLine>, Option<Reach>);

fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `poly` (planar, 3D) cut to where `f` ≥ 0 (`f` linear).
fn clip3(poly: &[[f64; 3]], f: impl Fn([f64; 3]) -> f64) -> Vec<[f64; 3]> {
    let n = poly.len();
    let mut out = Vec::new();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (fa, fb) = (f(a), f(b));
        if fa >= 0.0 {
            out.push(a);
        }
        if (fa >= 0.0) != (fb >= 0.0) {
            let t = fa / (fa - fb);
            out.push(std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t));
        }
    }
    out
}

fn count_clashes(d: &JointDetails, targets: &BTreeSet<String>) -> usize {
    d.joints
        .iter()
        .filter(|j| targets.contains(&j.id))
        .map(|j| j.clashes.len())
        .sum()
}

/// A plate the layout made before, kept for a run again: its joint, the
/// members on it, its fields given by hand since (thickness, material).
#[derive(Debug, Clone)]
struct OldPlate {
    id: String,
    joint: String,
    members: Vec<String>,
    thickness: Option<f64>,
    material: Option<String>,
    /// Its weld to the flat plate given by hand since (a plate in its
    /// member's own plane, step 4): its size, length and kind.
    weld: Option<ohl_structures::connections::TypeWeld>,
    /// It lay in its member's own plane (step 4).
    own: bool,
}

/// The automatic values at `targets` taken out of `doc` (for planning);
/// the plates made before, with what was given on them by hand.
fn strip(
    doc: &mut DocumentMut,
    t: &StructureType,
    record: &LayoutRecord,
    targets: &BTreeSet<String>,
    replace_hand: bool,
) -> Vec<OldPlate> {
    let mut rec = record.clone();
    let old: Vec<OldPlate> = old_plates(t, record, targets);
    let mut w = Writing::of(doc, t);
    for j in targets {
        strip_joint(doc, t, &mut rec, j, replace_hand, &mut w);
    }
    w.conns.tidy(doc);
    // The record without them (its marks, `gauge`, read by the drawing).
    rec.write(doc);
    old
}

fn old_plates(
    t: &StructureType,
    record: &LayoutRecord,
    targets: &BTreeSet<String>,
) -> Vec<OldPlate> {
    record
        .made
        .iter()
        .filter(|(_, (j, _))| targets.contains(j))
        .filter_map(|(id, (j, auto))| {
            let p = t.plates.iter().find(|p| &p.id == id)?;
            let onto = |id: &str| -> Vec<String> {
                t.connections
                    .iter()
                    .filter(|c| &c.joint == j && c.onto.as_deref() == Some(id))
                    .map(|c| c.member.clone())
                    .collect()
            };
            let mut members = onto(id);
            // Nothing bolted onto it (a flat plate with only plates in their
            // members' own planes welded to it, step 4): known by theirs.
            if members.is_empty() {
                for q in t.plates.iter().filter(|q| &q.joint == j && &q.id != id) {
                    if q.welded_ids().contains(&id.as_str())
                        || p.welded_ids().contains(&q.id.as_str())
                    {
                        members.extend(onto(&q.id));
                    }
                }
            }
            Some(OldPlate {
                id: id.clone(),
                joint: j.clone(),
                members,
                own: auto.contains(OWN_PLANE),
                thickness: (!auto.contains("thickness")).then(|| p.thickness.si()),
                material: p.material.clone(),
                weld: (auto.contains(OWN_PLANE) && !auto.contains("weld"))
                    .then(|| p.welds.first().cloned())
                    .flatten(),
            })
        })
        .collect()
}

/// The automatic values of joint `j` taken out of `doc` and `rec` (and the
/// layout's own fields given by hand too when `replace_hand`).
fn strip_joint(
    doc: &mut DocumentMut,
    t: &StructureType,
    rec: &mut LayoutRecord,
    j: &str,
    replace_hand: bool,
    w: &mut Writing,
) {
    // The plates it made.
    let made: Vec<String> = rec
        .made
        .iter()
        .filter(|(_, (pj, _))| pj == j)
        .map(|(id, _)| id.clone())
        .collect();
    for id in &made {
        if take_out_made(doc, rec, id) {
            w.taken.remove(id);
        }
    }
    // (Removing a plate tidies the connections: their places change.)
    if !made.is_empty() {
        w.conns = connection_edits::Connections::of(doc);
    }
    for c in t.connections.iter().filter(|c| c.joint == j) {
        let key = (c.member.clone(), c.joint.clone());
        let auto = rec.ends.remove(&key).unwrap_or_default();
        let mut go: BTreeSet<&str> = auto.iter().map(String::as_str).collect();
        if replace_hand {
            go.extend(["cut", "shift"]);
            // `onto` given by hand: a member or a plate of the joint.
            go.insert("onto");
        }
        for f in go {
            if f == "gauge" {
                continue;
            }
            if f == "preset" {
                for k in connection_edits::CARRIED {
                    let _ = w.conns.set(doc, &c.member, j, k, None);
                }
                continue;
            }
            let _ = w.conns.set(doc, &c.member, j, f, None);
        }
        rec.provisional.remove(&key);
    }
    // A connection left with nothing in it goes now (as the edit crate's
    // own edits do: where the ones written again then stand in the file
    // does not hang on a run before; a run again identical).
    let empty = doc
        .get("connection")
        .and_then(Item::as_array_of_tables)
        .is_some_and(|a| {
            a.iter().any(|t| {
                t.get("joint").and_then(Item::as_str) == Some(j)
                    && t.iter().all(|(k, _)| k == "member" || k == "joint")
            })
        });
    if empty {
        w.conns.tidy(doc);
    }
}

/// A file being written: its connections by member and joint, and the ids
/// in use (its plates and members), for many edits in a row (the edit
/// crate's own look each up in the whole file: on a tower of 2 500
/// members most of a run's time).
struct Writing {
    conns: connection_edits::Connections,
    taken: std::collections::HashSet<String>,
}

impl Writing {
    fn of(doc: &DocumentMut, t: &StructureType) -> Self {
        let mut taken = ids_in(doc);
        taken.extend(t.members.iter().map(|m| m.id.clone()));
        Self {
            conns: connection_edits::Connections::of(doc),
            taken,
        }
    }
}

/// Writes plan `p` into `doc` (its old values taken out already): each
/// end's values, the plates; recorded in `rec`. The plates' ids.
#[allow(clippy::too_many_arguments)]
fn write_plan(
    doc: &mut DocumentMut,
    t: &StructureType,
    rec: &mut LayoutRecord,
    p: &JointPlan,
    ask: &RunAsk<'_>,
    old: &[OldPlate],
    w: &mut Writing,
) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for x in &p.joins {
        let (fields, splice) = write_join(doc, &mut w.conns, &p.joint, x, &mut w.taken);
        let e = rec
            .ends
            .entry((x.member.clone(), p.joint.clone()))
            .or_default();
        e.extend(fields.iter().map(|f| (*f).to_owned()));
        if let Some(id) = splice {
            rec.ends
                .entry((x.onto.clone(), p.joint.clone()))
                .or_default()
                .insert("onto".into());
            rec.made.insert(
                id,
                (
                    p.joint.clone(),
                    BTreeSet::from([
                        SPLICE.to_owned(),
                        format!("cover {}", crate::mm_text(x.thickness)),
                    ]),
                ),
            );
        }
    }
    for (k, pl) in p.plates.iter().enumerate() {
        // The plate made before for (some of) these members: its id and
        // what was given on it by hand (a flat plate with nothing bolted
        // onto it, step 4: by the members of the plates welded to it).
        let mut mine = pl.members.clone();
        if mine.is_empty() {
            for q in p
                .plates
                .iter()
                .filter(|q| q.own.as_ref().is_some_and(|o| o.welded_to == k))
            {
                mine.extend(q.members.iter().cloned());
            }
        }
        let before = old.iter().find(|o| {
            o.joint == p.joint
                && o.own == pl.own.is_some()
                && o.members.iter().any(|m| mine.contains(m))
                && !w.taken.contains(&o.id)
        });
        // Its id kept when something was given on it by hand; else the
        // first free id after G0 (as `edits::next_id`).
        let id = match before.filter(|o| o.thickness.is_some() || o.material.is_some()) {
            Some(o) => o.id.clone(),
            None => (1u64..)
                .map(|n| format!("G{n}"))
                .find(|x| !w.taken.contains(x))
                .expect("a free id"),
        };
        w.taken.insert(id.clone());
        let thickness = before
            .and_then(|o| o.thickness)
            .unwrap_or(pl.thickness)
            .max(0.001);
        let mut x = Table::new();
        x.insert("id", toml_edit::value(id.clone()));
        x.insert("joint", toml_edit::value(p.joint.as_str()));
        x.insert("thickness", toml_edit::value(crate::mm_text(thickness)));
        if let Some(m) = before.and_then(|o| o.material.clone()) {
            x.insert("material", toml_edit::value(m));
        }
        let mut auto: BTreeSet<String> = BTreeSet::new();
        if before.and_then(|o| o.thickness).is_none() {
            auto.insert("thickness".into());
        }
        if ask.settings.welded && !ids.is_empty() {
            let mut a = Array::new();
            for w in &ids {
                a.push(w.as_str());
            }
            x.insert("welded_to", Item::Value(Value::Array(a)));
            auto.insert("welded_to".into());
        }
        // A plate in its member's own plane (step 4): its outline, and its
        // weld to the flat plate (both sides, the throat each side).
        if let Some(o) = &pl.own {
            // Its weld: as given by hand on it before (its size, length,
            // kind kept), else the layout's.
            let hand = before.and_then(|b| b.weld.as_ref());
            let mut weld = Table::new();
            weld.insert("to", toml_edit::value(ids[o.welded_to].as_str()));
            match hand {
                Some(h) => {
                    if let Some(v) = h.size {
                        weld.insert("size", toml_edit::value(crate::mm_text(v.si())));
                    }
                    if let Some(v) = h.length {
                        weld.insert("length", toml_edit::value(crate::mm_text(v.si())));
                    }
                    if let Some(k) = h.kind {
                        weld.insert("kind", toml_edit::value(k.as_str()));
                    }
                }
                None => {
                    weld.insert("size", toml_edit::value(crate::mm_text(o.weld_size)));
                    weld.insert("length", toml_edit::value(crate::mm_text(o.weld_length)));
                    auto.insert("weld".into());
                }
            }
            let mut welds = ArrayOfTables::new();
            welds.push(weld);
            x.insert("weld", Item::ArrayOfTables(welds));
            auto.extend(["outline".into(), OWN_PLANE.into()]);
        }
        if doc
            .get("plate")
            .and_then(Item::as_array_of_tables)
            .is_none()
        {
            doc.insert("plate", Item::ArrayOfTables(ArrayOfTables::new()));
        }
        doc["plate"]
            .as_array_of_tables_mut()
            .expect("made above")
            .push(x);
        if let Some(o) = &pl.own {
            let _ = connection_edits::set_plate_outline(doc, &id, &o.outline);
        }
        rec.made.insert(id.clone(), (p.joint.clone(), auto));
        ids.push(id);
    }
    for e in &p.ends {
        let key = (e.member.clone(), p.joint.clone());
        let fields = rec.ends.entry(key.clone()).or_default();
        let onto = match &e.onto {
            Onto::Member(m) => m.clone(),
            Onto::Plate(k) => ids[*k].clone(),
        };
        let set = |w: &mut Writing, doc: &mut DocumentMut, f: &str, v: FieldValue| {
            let _ = w.conns.set(doc, &e.member, &p.joint, f, Some(&v));
        };
        set(w, doc, "onto", FieldValue::Word(onto));
        fields.insert("onto".into());
        // Drawn with its gauge line through the work point (the record's
        // mark only: no field of the file).
        if e.on_gauge {
            fields.insert("gauge".into());
        }
        if let Some(c) = e.cut {
            set(w, doc, "cut", FieldValue::Length(c));
            fields.insert("cut".into());
        }
        if let Some(s) = e.shift {
            if s.abs() >= 5e-5 {
                set(w, doc, "shift", FieldValue::SignedLength(s));
                fields.insert("shift".into());
            } else {
                let _ = w.conns.set(doc, &e.member, &p.joint, "shift", None);
            }
        }
        if let Some(b) = &e.bolts {
            let class = t
                .members
                .iter()
                .find(|m| m.id == e.member)
                .and_then(|m| m.design.as_ref())
                .and_then(|d| d.get("class"))
                .and_then(toml::Value::as_str)
                .map(str::to_owned);
            match (&b.preset, class.and_then(|c| ask.preset_fields.get(&c))) {
                (Some(_), Some(fields_of)) => {
                    connection_edits::put_carried(doc, &e.member, &p.joint, fields_of);
                    fields.insert("preset".into());
                }
                _ => {
                    set(
                        w,
                        doc,
                        "bolts",
                        FieldValue::Bolts {
                            count: b.count,
                            bolt: b.bolt.clone(),
                            diameter: b.bolt.is_none().then_some(b.diameter),
                            hole: None,
                            grade: None,
                        },
                    );
                    set(w, doc, "gauge_lines", FieldValue::Count(b.lines));
                    fields.insert("bolts".into());
                    fields.insert("gauge_lines".into());
                }
            }
            rec.provisional.insert(key.clone());
        }
    }
    ids
}

/// The joints an edit from `before` to `after` touches: a member changed
/// (its section, roll, orientation, design data, joints), a value at an
/// end, a plate, a joint moved (and the joints of its members), the
/// layout's settings (every joint laid out).
pub fn touched(before: &StructureType, after: &StructureType) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let pos = |t: &StructureType| -> BTreeMap<String, [f64; 3]> {
        t.nodes
            .iter()
            .map(|n| (n.id.clone(), n.point.map(|v| v.si())))
            .collect()
    };
    let (p0, p1) = (pos(before), pos(after));
    let mut moved: BTreeSet<String> = BTreeSet::new();
    for (id, a) in &p1 {
        if p0.get(id) != Some(a) {
            moved.insert(id.clone());
        }
    }
    for id in p0.keys() {
        if !p1.contains_key(id) {
            moved.insert(id.clone());
        }
    }
    // Compared as read (not as text: quick on a large tower).
    fn members(t: &StructureType) -> BTreeMap<String, &ohl_structures::structures::TypeMember> {
        t.members.iter().map(|m| (m.id.clone(), m)).collect()
    }
    let (m0, m1) = (members(before), members(after));
    for t in [before, after] {
        for m in &t.members {
            let changed = m0.get(&m.id) != m1.get(&m.id);
            if changed || m.nodes.iter().any(|n| moved.contains(n)) {
                out.extend(m.nodes.iter().cloned());
            }
        }
    }
    out.extend(moved);
    fn conns(t: &StructureType) -> BTreeMap<(String, String), &TypeConnection> {
        t.connections
            .iter()
            .map(|c| ((c.member.clone(), c.joint.clone()), c))
            .collect()
    }
    let (c0, c1) = (conns(before), conns(after));
    for k in c0.keys().chain(c1.keys()) {
        if c0.get(k) != c1.get(k) {
            out.insert(k.1.clone());
        }
    }
    fn plates(t: &StructureType) -> BTreeMap<String, (String, &TypePlate)> {
        t.plates
            .iter()
            .map(|p| (p.id.clone(), (p.joint.clone(), p)))
            .collect()
    }
    let (q0, q1) = (plates(before), plates(after));
    for k in q0.keys().chain(q1.keys()) {
        if q0.get(k) != q1.get(k) {
            for x in [q0.get(k), q1.get(k)].into_iter().flatten() {
                out.insert(x.0.clone());
            }
        }
    }
    // A splice (its covers, step 1c): its joint.
    let splices = |t: &StructureType| -> BTreeMap<String, (String, String)> {
        t.splices
            .iter()
            .map(|s| (s.id.clone(), (s.joint.clone(), format!("{s:?}"))))
            .collect()
    };
    let (s0, s1) = (splices(before), splices(after));
    for k in s0.keys().chain(s1.keys()) {
        if s0.get(k) != s1.get(k) {
            for x in [s0.get(k), s1.get(k)].into_iter().flatten() {
                out.insert(x.0.clone());
            }
        }
    }
    let (r0, r1) = (LayoutRecord::of(before), LayoutRecord::of(after));
    if r0.settings != r1.settings {
        out.extend(r1.joints.iter().cloned());
    }
    out
}

/// What [`follow`] made of an edit.
#[derive(Debug, Clone, PartialEq)]
pub struct Followed {
    /// The file after (the edit and the joints laid out again).
    pub text: String,
    /// In words, for the edit's message.
    pub message: String,
    /// The joints laid out again.
    pub again: Vec<String>,
    /// The joints that no longer fit (their layout kept), with why.
    pub misfits: Vec<(String, String)>,
}

/// The layout following an edit from `before` to `after` (structure
/// files): the laid-out joints it touches laid out again (hand values
/// kept; an automatic value the edit changed becomes the user's). None:
/// no layout in the file, or no laid-out joint touched.
pub fn follow(
    before: &str,
    after: &str,
    library: &SectionLibrary,
    bolts: &BoltLibrary,
    presets: &[(String, Table)],
) -> Option<Followed> {
    follow_with(before, after, library, bolts, presets, true)
}

/// [`follow`] with the joints in detail from the whole file
/// ([`lay_out_whole`]): the same result, slower on a large tower; for the
/// comparison of the two.
#[doc(hidden)]
pub fn follow_whole(
    before: &str,
    after: &str,
    library: &SectionLibrary,
    bolts: &BoltLibrary,
    presets: &[(String, Table)],
) -> Option<Followed> {
    follow_with(before, after, library, bolts, presets, false)
}

fn follow_with(
    before: &str,
    after: &str,
    library: &SectionLibrary,
    bolts: &BoltLibrary,
    presets: &[(String, Table)],
    cut: bool,
) -> Option<Followed> {
    if !after.contains(RECORD_KIND) {
        return None;
    }
    let t0 = StructureType::parse(std::path::Path::new("structure.ohls"), before).ok()?;
    let doc: DocumentMut = after.parse().ok()?;
    follow_parsed(&t0, after, doc, library, bolts, presets, cut)
}

/// [`follow`] of an edit as the application has it: the structure before
/// as read, the text and document after (the file not read again: quick
/// on a large tower).
pub fn follow_read(
    before: &StructureType,
    after_text: &str,
    after: DocumentMut,
    library: &SectionLibrary,
    bolts: &BoltLibrary,
    presets: &[(String, Table)],
) -> Option<Followed> {
    follow_parsed(before, after_text, after, library, bolts, presets, true)
}

#[allow(clippy::too_many_arguments)]
fn follow_parsed(
    t0: &StructureType,
    after: &str,
    mut doc: DocumentMut,
    library: &SectionLibrary,
    bolts: &BoltLibrary,
    presets: &[(String, Table)],
    cut: bool,
) -> Option<Followed> {
    if !after.contains(RECORD_KIND) {
        return None;
    }
    let t1 = StructureType::parse(std::path::Path::new("structure.ohls"), after).ok()?;
    let mut record = LayoutRecord::of(&t1);
    if record.joints.is_empty() {
        return None;
    }
    let settings = record.settings.clone()?;
    // An automatic value the edit changed is the user's now.
    let mut changed_record = false;
    for ((m, j), fields) in record.ends.iter_mut() {
        let (a, b) = (t0.connection(m, j), t1.connection(m, j));
        fields.retain(|f| {
            let same = match f.as_str() {
                "cut" => a.map(|c| &c.cut) == b.map(|c| &c.cut),
                "shift" => a.map(|c| &c.shift) == b.map(|c| &c.shift),
                "onto" => a.map(|c| &c.onto) == b.map(|c| &c.onto),
                "bolts" | "preset" => a.map(|c| &c.bolts) == b.map(|c| &c.bolts),
                "gauge_lines" => a.map(|c| &c.gauge_lines) == b.map(|c| &c.gauge_lines),
                // The drawing's mark: the user's once the shift is.
                "gauge" => a.map(|c| &c.shift) == b.map(|c| &c.shift),
                _ => true,
            };
            if !same {
                changed_record = true;
            }
            same
        });
    }
    // A roll the layout gave, changed by the edit: the user's now.
    let roll = |t: &StructureType, m: &str| {
        t.members
            .iter()
            .find(|x| x.id == m)
            .and_then(|x| x.roll.clone())
    };
    let before_rolled = record.rolled.len();
    record.rolled.retain(|m, _| roll(t0, m) == roll(&t1, m));
    changed_record |= record.rolled.len() != before_rolled;
    for (id, (_, fields)) in record.made.iter_mut() {
        let (a, b) = (
            t0.plates.iter().find(|p| &p.id == id),
            t1.plates.iter().find(|p| &p.id == id),
        );
        if a.map(|p| &p.thickness) != b.map(|p| &p.thickness) && fields.remove("thickness") {
            changed_record = true;
        }
        // A welded plate's weld changed (step 4): the user's now.
        if a.map(|p| &p.welds) != b.map(|p| &p.welds) && fields.remove("weld") {
            changed_record = true;
        }
    }
    let touched: BTreeSet<String> = touched(t0, &t1)
        .into_iter()
        .filter(|j| record.joints.contains(j))
        .collect();
    if touched.is_empty() {
        if changed_record {
            record.write(&mut doc);
            return Some(Followed {
                text: doc.to_string(),
                message: String::new(),
                again: Vec::new(),
                misfits: Vec::new(),
            });
        }
        return None;
    }
    if changed_record {
        record.write(&mut doc);
    }
    let (areas, preset_fields) = presets_by_class(&settings.presets, presets, bolts);
    let s = settings.layout(areas);
    let ask = RunAsk {
        library,
        bolts,
        settings: &s,
        preset_fields,
        joints: Some(touched.clone()),
        replace_hand: false,
    };
    // The edited file as read (its record as changed above).
    let run = lay_out_read(
        &t1,
        record,
        doc,
        &ask,
        (cut, false),
        std::time::Instant::now(),
        false,
    )
    .ok()?;
    let again: Vec<String> = run.laid_out().map(|o| o.joint.clone()).collect();
    // Left as it was (the joint did not fit); a joint laid out again with
    // members still for a hand detail is said with its count.
    let misfits: Vec<(String, String)> = run
        .for_hand()
        .filter(|o| !o.written)
        .map(|o| (o.joint.clone(), o.hand.join("; ")))
        .collect();
    let mut message = String::new();
    if !again.is_empty() {
        let partly: usize = run.laid_out().map(|o| o.plan.hand_members.len()).sum();
        message.push_str(&format!(
            "joint layout followed: {} laid out again{}",
            again.join(", "),
            if partly > 0 {
                format!(" ({partly} member(s) still for a hand detail, as before)")
            } else {
                String::new()
            }
        ));
    }
    if !misfits.is_empty() {
        if !message.is_empty() {
            message.push_str("; ");
        }
        message.push_str(&format!(
            "no longer fits, its layout kept, needs a hand detail: {}",
            misfits
                .iter()
                .map(|(j, w)| format!("{j} ({w})"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Some(Followed {
        text: run.text,
        message,
        again,
        misfits,
    })
}
