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
use ohl_structures::{LibraryItem, StructureType};
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, Value};

use crate::joint_layout::{BoltArea, JointPlan, LayoutSettings, Onto, flat_turns, plan_joint};
use crate::joints::{JointDetails, joint_details_at};

/// The record's id and kind.
pub const RECORD_ID: &str = "joint-layout";
/// The record's kind (`ohl_structure_edit::body::JOINT_LAYOUT_KIND`).
pub const RECORD_KIND: &str = ohl_structure_edit::body::JOINT_LAYOUT_KIND;

/// The fields of a member end the layout sets.
pub const END_FIELDS: [&str; 6] = ["cut", "shift", "onto", "bolts", "gauge_lines", "preset"];

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
    /// roll each had before, radians (a run again starts from it).
    pub rolled: BTreeMap<String, f64>,
    /// The plates the layout made: id → its joint and its automatic
    /// fields (`thickness`, `welded_to`).
    pub made: BTreeMap<String, (String, BTreeSet<String>)>,
    /// The member ends whose bolts are provisional (the layout's).
    pub provisional: BTreeSet<(String, String)>,
    /// The joints that need a hand detail, with why (left by a run, or no
    /// longer fitting after an edit).
    pub hand: BTreeMap<String, String>,
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
                .map(|r| {
                    let deg = r[1]
                        .trim()
                        .trim_end_matches("deg")
                        .trim()
                        .parse::<f64>()
                        .unwrap_or(0.0);
                    (r[0].clone(), deg.to_radians())
                })
                .collect(),
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
        r
    }

    /// Nothing recorded.
    pub fn is_empty(&self) -> bool {
        self.joints.is_empty()
            && self.ends.is_empty()
            && self.made.is_empty()
            && self.rolled.is_empty()
            && self.hand.is_empty()
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
        if !self.is_empty() || self.settings.is_some() {
            let mut t = Table::new();
            t.insert("id", toml_edit::value(RECORD_ID));
            t.insert("kind", toml_edit::value(RECORD_KIND));
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
                            .map(|(m, r)| {
                                vec![
                                    m.clone(),
                                    format!("{} deg", (r.to_degrees() * 100.0).round() / 100.0),
                                ]
                            })
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
            a.push(t);
        }
        if !a.is_empty() {
            doc.insert("generator", Item::ArrayOfTables(a));
        }
    }
}

/// The member ends of structure `t` whose bolts are provisional (the
/// layout's, until the code step chooses bolts): a tower with any stays at
/// level 3 for the joint details (owner 2026-11-18, question 6).
pub fn provisional_bolts(t: &StructureType) -> Vec<(String, String)> {
    LayoutRecord::of(t).provisional.into_iter().collect()
}

/// Why the joint details are not taken although asked for and level 4 by
/// the file: provisional bolts (none: none provisional, or not asked).
pub fn provisional_note(t: &StructureType) -> Option<String> {
    let n = provisional_bolts(t).len();
    (n > 0).then(|| {
        format!(
            "{n} member end(s) with provisional bolts (the joint layout's, not chosen yet): \
             level 3 until no bolt is provisional — the joint details are not taken in the \
             calculation"
        )
    })
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
    /// Clashes at the joints asked before and after.
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
    let start = std::time::Instant::now();
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    let doc0: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    let record = LayoutRecord::of(&t);
    let targets: BTreeSet<String> = match &ask.joints {
        Some(j) => j.clone(),
        None => t.nodes.iter().map(|n| n.id.clone()).collect(),
    };
    // 1. The automatic values (and hand ones when replaced) taken out.
    let mut planning = doc0.clone();
    let hand_plates = strip(&mut planning, &t, &record, &targets, ask.replace_hand);
    let at = |text: &str| joint_details_at(text, ask.library, ask.bolts, Some(&targets));
    let before = at(text)?;
    let clashes_before = count_clashes(&before, &targets);
    // The members the layout turned before, at the joints asked: back to
    // the roll they had (a run again starts from the same).
    let restored: BTreeMap<String, f64> = record
        .rolled
        .iter()
        .filter(|(m, _)| {
            t.members
                .iter()
                .find(|x| &x.id == *m)
                .is_some_and(|x| x.nodes.iter().any(|n| targets.contains(n)))
        })
        .map(|(m, r)| (m.clone(), *r))
        .collect();
    for (m, r) in &restored {
        ohl_structure_edit::edits::set_member_roll(&mut planning, m, *r);
    }
    let details = at(&planning.to_string())?;
    // 2. The members lying off what they bolt onto turned flat, where every
    // end asked agrees (a member's roll given by hand kept unless
    // replaced).
    let mut notes = Vec::new();
    let mut asked: BTreeMap<String, Vec<(String, f64)>> = BTreeMap::new();
    for j in details.joints.iter().filter(|j| targets.contains(&j.id)) {
        for (m, turn) in flat_turns(j) {
            let by_hand = t
                .members
                .iter()
                .find(|x| x.id == m)
                .is_some_and(|x| x.roll.is_some())
                && !record.rolled.contains_key(&m);
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
    let mut turned_at: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // Each such member's turns at all its ends asked (those flat already
    // too: the turn keeps them flat).
    for (m, ends) in asked.iter_mut() {
        for j in details.joints.iter().filter(|j| targets.contains(&j.id)) {
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
    }
    for (m, ends) in &asked {
        // Within the flatness allowed of each other: the middle of them (each
        // end then within half their spread of flat), to a hundredth of a
        // degree as written.
        let lo = ends.iter().map(|e| e.1).fold(f64::INFINITY, f64::min);
        let hi = ends.iter().map(|e| e.1).fold(f64::NEG_INFINITY, f64::max);
        let first = (((lo + hi) / 2.0).to_degrees() * 100.0).round() / 100.0;
        let first = first.to_radians();
        if (hi - lo).to_degrees() > crate::FLAT_OFF {
            notes.push(format!(
                "{m}: lies off what it bolts onto at {}, by different turns (one roll for \
                 both ends): not turned",
                ends.iter()
                    .map(|(j, x)| format!("{j} ({:.1}°)", x.to_degrees()))
                    .collect::<Vec<_>>()
                    .join(" and ")
            ));
            continue;
        }
        turned.insert(m.clone(), first);
        for (j, _) in ends {
            turned_at.entry(j.clone()).or_default().push(m.clone());
        }
    }
    let roll_of = |m: &str| {
        restored.get(m).copied().unwrap_or_else(|| {
            t.members
                .iter()
                .find(|x| x.id == m)
                .map_or(0.0, ohl_structure_edit::edits::member_roll)
        })
    };
    for (m, turn) in &turned {
        ohl_structure_edit::edits::set_member_roll(&mut planning, m, roll_of(m) + turn);
    }
    let details = if turned.is_empty() {
        details
    } else {
        at(&planning.to_string())?
    };
    // 3. Each joint planned.
    let model = ohl_structures_fe::model::Model::from_structure_type_with_joint_details(
        &planning.to_string(),
        ask.library,
        ask.bolts,
        None,
    )
    .ok();
    let edge = |a: &crate::Arrival, thick: f64| -> Option<f64> {
        let m = model.as_ref()?;
        let idx = m.members.iter().position(|x| x.id == a.member)?;
        crate::member_view::rules_in(m, idx, a.end, None, Some((thick, None))).map(|r| r.edge)
    };
    let mut outcomes: Vec<JointOutcome> = details
        .joints
        .iter()
        .filter(|j| targets.contains(&j.id))
        .map(|j| {
            let plan = plan_joint(j, ask.settings, &edge);
            JointOutcome {
                joint: j.id.clone(),
                hand: plan.hand.clone(),
                written: false,
                turned: turned_at.get(&j.id).cloned().unwrap_or_default(),
                plates: Vec::new(),
                plan,
            }
        })
        .collect();
    // 4. Written into the file as it was; then the clash check: a joint
    // whose laid-out parts run into anything taken back.
    let mut take_back: BTreeSet<String> = BTreeSet::new();
    let mut result = String::new();
    let mut after_count = 0;
    for _round in 0..2 {
        let mut doc = doc0.clone();
        let mut rec = record.clone();
        rec.settings = Some(RecordSettings::of(ask.settings));
        // The rolls (the members of the joints asked): those turned before
        // back, then the turns of this run.
        for (m, r) in &restored {
            ohl_structure_edit::edits::set_member_roll(&mut doc, m, *r);
            rec.rolled.remove(m);
        }
        for (m, turn) in &turned {
            ohl_structure_edit::edits::set_member_roll(&mut doc, m, roll_of(m) + turn);
            rec.rolled.insert(m.clone(), roll_of(m));
        }
        for o in &mut outcomes {
            o.written = false;
            o.plates.clear();
            if !take_back.contains(&o.joint) {
                o.hand = o.plan.hand.clone();
            }
            if !o.plan.laid_out() || take_back.contains(&o.joint) {
                if o.plan.ends.is_empty() && o.plan.hand.is_empty() {
                    // Nothing to lay out: the old automatic values go.
                    strip_joint(&mut doc, &t, &mut rec, &o.joint, ask.replace_hand);
                    rec.joints.remove(&o.joint);
                    rec.hand.remove(&o.joint);
                } else if !o.hand.is_empty() {
                    rec.hand.insert(o.joint.clone(), o.hand.join("; "));
                }
                continue;
            }
            strip_joint(&mut doc, &t, &mut rec, &o.joint, ask.replace_hand);
            o.plates = write_plan(&mut doc, &t, &mut rec, &o.plan, ask, &hand_plates);
            o.written = true;
            rec.joints.insert(o.joint.clone());
            if o.plan.hand.is_empty() {
                rec.hand.remove(&o.joint);
            } else {
                rec.hand.insert(o.joint.clone(), o.plan.hand.join("; "));
            }
        }
        // The plates' outlines worked out round their bolts, cut at the
        // flange's heel by half the gap (clear of the other face's), written
        // (automatic).
        let heels: Vec<(String, HeelLine)> = outcomes
            .iter()
            .filter(|o| o.written)
            .flat_map(|o| {
                o.plates
                    .iter()
                    .zip(&o.plan.plates)
                    .filter_map(|(id, p)| p.heel.map(|h| (id.clone(), h)))
                    .collect::<Vec<_>>()
            })
            .collect();
        if !heels.is_empty() {
            // The record first: the drawing reads its marks.
            rec.write(&mut doc);
            let mid = at(&doc.to_string())?;
            for (id, (heel, across)) in &heels {
                let Some(p) = mid
                    .joints
                    .iter()
                    .flat_map(|j| &j.plates)
                    .find(|p| &p.id == id)
                else {
                    continue;
                };
                let half = ask.settings.gap / 2.0;
                let cut = clip3(&p.outline, |q| {
                    (0..3).map(|k| (q[k] - heel[k]) * across[k]).sum::<f64>() - half
                });
                if cut.len() >= 3
                    && connection_edits::set_plate_outline(&mut doc, id, &cut).is_ok()
                    && let Some(m) = rec.made.get_mut(id)
                {
                    m.1.insert("outline".into());
                }
            }
        }
        rec.write(&mut doc);
        result = doc.to_string();
        let after = at(&result)?;
        let mut again = false;
        for o in outcomes.iter_mut().filter(|o| o.written) {
            let Some(j) = after.joint(&o.joint) else {
                continue;
            };
            let laid: BTreeSet<&str> = o.plan.ends.iter().map(|e| e.member.as_str()).collect();
            let ours = |x: &crate::ClashPart| match x {
                crate::ClashPart::Plate(id) => o.plates.contains(id),
                other => other.member().is_some_and(|m| laid.contains(m)),
            };
            // A member left for a hand detail is as it was: its clashes
            // are said with it, they do not take the rest back.
            let left = |x: &crate::ClashPart| {
                x.member()
                    .is_some_and(|m| o.plan.hand_members.iter().any(|h| h == m))
            };
            let bad: Vec<String> = j
                .clashes
                .iter()
                .filter(|c| (ours(&c.a) || ours(&c.b)) && !left(&c.a) && !left(&c.b))
                .map(|c| {
                    // A plate it would make named by where it lies (its id
                    // is not kept: the joint is left as it was).
                    let name = |x: &crate::ClashPart| match x {
                        crate::ClashPart::Plate(id) => o
                            .plates
                            .iter()
                            .position(|p| p == id)
                            .map(|k| format!("a plate on {}", o.plan.plates[k].on))
                            .unwrap_or_else(|| x.words()),
                        other => other.words(),
                    };
                    format!(
                        "{} and {} would run {:.0} mm into each other",
                        name(&c.a),
                        name(&c.b),
                        (c.depth * 1000.0).max(1.0)
                    )
                })
                .collect();
            if !bad.is_empty() {
                o.hand = bad
                    .into_iter()
                    .map(|w| format!("laid out: {w}"))
                    .chain(o.plan.hand.iter().cloned())
                    .collect();
                take_back.insert(o.joint.clone());
                again = true;
            }
        }
        after_count = count_clashes(&after, &targets);
        if !again {
            break;
        }
    }
    // Still clashing after the second round: those taken back are left as
    // they were (the second round wrote none of them).
    Ok(LayoutRun {
        text: result,
        joints: outcomes,
        clashes: (clashes_before, after_count),
        notes,
        seconds: start.elapsed().as_secs_f64(),
    })
}

/// A flange's heel line: a point on it and across the flange (m, unit).
type HeelLine = ([f64; 3], [f64; 3]);

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
    for j in targets {
        strip_joint(doc, t, &mut rec, j, replace_hand);
    }
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
            Some(OldPlate {
                id: id.clone(),
                joint: j.clone(),
                members: t
                    .connections
                    .iter()
                    .filter(|c| &c.joint == j && c.onto.as_deref() == Some(id.as_str()))
                    .map(|c| c.member.clone())
                    .collect(),
                thickness: (!auto.contains("thickness")).then(|| p.thickness.si()),
                material: p.material.clone(),
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
) {
    // The plates it made.
    let made: Vec<String> = rec
        .made
        .iter()
        .filter(|(_, (pj, _))| pj == j)
        .map(|(id, _)| id.clone())
        .collect();
    for id in &made {
        connection_edits::remove_plate(doc, id);
        rec.made.remove(id);
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
                    let _ = connection_edits::set_connection_field(doc, &c.member, j, k, None);
                }
                continue;
            }
            let _ = connection_edits::set_connection_field(doc, &c.member, j, f, None);
        }
        rec.provisional.remove(&key);
    }
}

/// Writes plan `p` into `doc` (its old values taken out already): each
/// end's values, the plates; recorded in `rec`. The plates' ids.
fn write_plan(
    doc: &mut DocumentMut,
    t: &StructureType,
    rec: &mut LayoutRecord,
    p: &JointPlan,
    ask: &RunAsk<'_>,
    old: &[OldPlate],
) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    // Ids in use: the plates of the file now and the members.
    let mut taken: Vec<String> = doc
        .get("plate")
        .and_then(Item::as_array_of_tables)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.get("id").and_then(Item::as_str).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    taken.extend(t.members.iter().map(|m| m.id.clone()));
    for pl in &p.plates {
        // The plate made before for (some of) these members: its id and
        // what was given on it by hand.
        let before = old.iter().find(|o| {
            o.joint == p.joint
                && o.members.iter().any(|m| pl.members.contains(m))
                && !taken.contains(&o.id)
        });
        let id = match before {
            Some(o) => o.id.clone(),
            None => ohl_structure_edit::edits::next_id(taken.iter().map(String::as_str), "G0"),
        };
        taken.push(id.clone());
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
        let set = |doc: &mut DocumentMut, f: &str, v: FieldValue| {
            let _ = connection_edits::set_connection_field(doc, &e.member, &p.joint, f, Some(&v));
        };
        set(doc, "onto", FieldValue::Word(onto));
        fields.insert("onto".into());
        // Drawn with its gauge line through the work point (the record's
        // mark only: no field of the file).
        if e.on_gauge {
            fields.insert("gauge".into());
        }
        if let Some(c) = e.cut {
            set(doc, "cut", FieldValue::Length(c));
            fields.insert("cut".into());
        }
        if let Some(s) = e.shift {
            if s.abs() >= 5e-5 {
                set(doc, "shift", FieldValue::SignedLength(s));
                fields.insert("shift".into());
            } else {
                let _ =
                    connection_edits::set_connection_field(doc, &e.member, &p.joint, "shift", None);
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
                    set(doc, "gauge_lines", FieldValue::Count(b.lines));
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
    let members = |t: &StructureType| -> BTreeMap<String, String> {
        t.members
            .iter()
            .map(|m| (m.id.clone(), format!("{m:?}")))
            .collect()
    };
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
    let conns = |t: &StructureType| -> BTreeMap<(String, String), String> {
        t.connections
            .iter()
            .map(|c| ((c.member.clone(), c.joint.clone()), format!("{c:?}")))
            .collect()
    };
    let (c0, c1) = (conns(before), conns(after));
    for k in c0.keys().chain(c1.keys()) {
        if c0.get(k) != c1.get(k) {
            out.insert(k.1.clone());
        }
    }
    let plates = |t: &StructureType| -> BTreeMap<String, (String, String)> {
        t.plates
            .iter()
            .map(|p| (p.id.clone(), (p.joint.clone(), format!("{p:?}"))))
            .collect()
    };
    let (q0, q1) = (plates(before), plates(after));
    for k in q0.keys().chain(q1.keys()) {
        if q0.get(k) != q1.get(k) {
            for x in [q0.get(k), q1.get(k)].into_iter().flatten() {
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
    if !after.contains(RECORD_KIND) {
        return None;
    }
    let path = std::path::Path::new("structure.ohls");
    let t0 = StructureType::parse(path, before).ok()?;
    let t1 = StructureType::parse(path, after).ok()?;
    let mut record = LayoutRecord::of(&t1);
    if record.joints.is_empty() {
        return None;
    }
    let settings = record.settings.clone()?;
    // An automatic value the edit changed is the user's now.
    let mut doc: DocumentMut = after.parse().ok()?;
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
    record.rolled.retain(|m, _| roll(&t0, m) == roll(&t1, m));
    changed_record |= record.rolled.len() != before_rolled;
    for (id, (_, fields)) in record.made.iter_mut() {
        let (a, b) = (
            t0.plates.iter().find(|p| &p.id == id),
            t1.plates.iter().find(|p| &p.id == id),
        );
        if a.map(|p| &p.thickness) != b.map(|p| &p.thickness) && fields.remove("thickness") {
            changed_record = true;
        }
    }
    let touched: BTreeSet<String> = touched(&t0, &t1)
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
    let text = doc.to_string();
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
    let run = lay_out(&text, &ask).ok()?;
    let again: Vec<String> = run.laid_out().map(|o| o.joint.clone()).collect();
    let misfits: Vec<(String, String)> = run
        .for_hand()
        .map(|o| (o.joint.clone(), o.hand.join("; ")))
        .collect();
    let mut message = String::new();
    if !again.is_empty() {
        message.push_str(&format!(
            "joint layout followed: {} laid out again",
            again.join(", ")
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
