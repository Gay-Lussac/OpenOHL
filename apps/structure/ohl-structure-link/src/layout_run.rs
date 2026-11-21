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
    BoltArea, Join, JoinKind, JointPlan, LayoutSettings, Onto, Reach, flat_turns,
};
use crate::joint_order::JointOrder;
use crate::joints::{JointDetails, joint_details_at};
use crate::whole_roll::{EndTurn, RollWay, whole_roll};

/// The record's id and kind.
pub const RECORD_ID: &str = "joint-layout";
/// The record's kind (`ohl_structure_edit::body::JOINT_LAYOUT_KIND`).
pub const RECORD_KIND: &str = ohl_structure_edit::body::JOINT_LAYOUT_KIND;

/// The record's mark of a plate made in its member's own plane, welded to
/// a flat plate (`made`; the owner's step 4, 2026-11-19): the joint details
/// seat its member on the plate's outline, lifted along the flat plate's
/// normal ([`crate::joints`]).
pub const OWN_PLANE: &str = "own_plane";

/// The record's mark of a bent plate's flat in its members' plane (`made`,
/// with [`OWN_PLANE`]; J2b, 2026-11-19): its `[plate.bend]` the layout's
/// (one changed by hand since is kept, the user's).
pub const BENT: &str = "bend";

/// The record's mark of a plate's own bolts onto the member it lies on that
/// the layout gave (`made`; a bent plate's flat on the flange, J2b): the
/// plate is then also listed among the provisional bolts as (plate, joint).
pub const HOLD: &str = "leg_bolts";

/// The record's mark of a splice the layout made (`made`, with `cover`
/// and the covers' thickness as written; step 1c, 2026-11-19).
pub const SPLICE: &str = "splice";

/// The record's mark of a plate whose outline stops short of the work
/// point (`made`; L2's follow-up, 2026-11-20): the joint details draw it
/// round its members' legs from the flange's toe over their bolts and the
/// flange under them ([`crate::joints`]), and the run writes that outline.
pub const SHORT: &str = "short";

/// The record's mark of a member end whose `onto` was given by hand and
/// kept by the layout (`ends`, "onto=D4"; O44 L2's follow-up, 2026-11-20):
/// the end laid out onto that member, or onto a plate the layout made on
/// it. The value written is the layout's (the plate's id where it goes onto
/// one); taking the run's values out puts the hand value back (unless
/// values given by hand are replaced), so a run again keeps it.
pub const ONTO_HAND: &str = "onto=";

/// The `onto` given by hand at member `m`'s end at joint `j` before a run
/// (the file `t`, its record `record`): the record's [`ONTO_HAND`] mark,
/// else the file's value where the record does not take it as the
/// layout's.
pub(crate) fn hand_onto(
    t: &StructureType,
    record: &LayoutRecord,
    m: &str,
    j: &str,
) -> Option<String> {
    let fields = record.ends.get(&(m.to_owned(), j.to_owned()));
    if let Some(h) = fields.and_then(|f| f.iter().find_map(|x| x.strip_prefix(ONTO_HAND))) {
        return Some(h.to_owned());
    }
    if fields.is_some_and(|f| f.contains("onto")) {
        return None;
    }
    t.connection(m, j).and_then(|c| c.onto.clone())
}

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
            ..Default::default()
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
            && self.provisional.is_empty()
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
    /// The pieces in line through the joints asked that are not one part
    /// (O44 J9, D15): proposed `continuous` for the user to confirm
    /// ([`crate::in_line::confirm`]), or not and why. Never written by
    /// the run: until confirmed each piece ends at the joint (on the safe
    /// side).
    pub in_line: Vec<crate::InLine>,
    /// The leg splices proposed at the joints asked (O44 J7): pieces in
    /// line of other sections, both of a leg, proposed spliced for the
    /// user to confirm ([`crate::leg_splices::confirm`]), or not and why
    /// (the same section: [`crate::InLine::splice_bolts`] among
    /// `in_line`). Never written by the run: until confirmed each piece
    /// ends at the joint (on the safe side).
    pub splices: Vec<crate::LegSplice>,
    /// The leg pieces at the joints asked whose joints say they are turned
    /// about their axis (O44 L11, [`crate::leg_roll`]): proposed, never
    /// applied by the run (Joint details' Solve the clashes offers the turn
    /// with its effect).
    pub leg_rolls: Vec<crate::LegRoll>,
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
    if ask.settings.chord_turns {
        return chord_turned(text, ask);
    }
    lay_out_with(text, ask, true)
}

/// L12 (off by default, [`LayoutSettings::chord_turns`]): the run with
/// the chords whose joints say they are turned turned first (as Orient
/// angles turns a chord, before the members' own turns). In rounds: the
/// chords flagged on the run's result so far ([`chord_groups`]) all turned
/// and the run made again; each kept where its joints lay out more ends,
/// or as many on fewer plates, none fewer at any of them (planned again
/// with only those until each still helps); the next round from that
/// result, until no chord flagged there helps — so that a run again on the
/// result mostly finds nothing more to turn (measured: not on every tower,
/// the bracing turned after a chord may say another turn). The turns kept
/// said in the notes;
/// none kept: the run without them.
fn chord_turned(text: &str, ask: &RunAsk<'_>) -> Result<LayoutRun, String> {
    use crate::other_face::helps;
    let plain_settings = LayoutSettings {
        chord_turns: false,
        ..ask.settings.clone()
    };
    let plain = RunAsk {
        library: ask.library,
        bolts: ask.bolts,
        settings: &plain_settings,
        preset_fields: ask.preset_fields.clone(),
        joints: ask.joints.clone(),
        replace_hand: ask.replace_hand,
    };
    let r0 = lay_out_with(text, &plain, true)?;
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    let record = LayoutRecord::of(&t);
    let targets: BTreeSet<String> = ask
        .joints
        .clone()
        .unwrap_or_else(|| t.nodes.iter().map(|n| n.id.clone()).collect());
    let may = |m: &str| -> bool {
        let Some(x) = t.members.iter().find(|x| x.id == m) else {
            return false;
        };
        let automatic = record.rolled.contains_key(m) || record.oriented.contains_key(m);
        !(x.roll.is_some() && !automatic && !ask.replace_hand)
    };
    // The input with the turns `cum` (degrees, per member) added.
    let turned_text = |cum: &BTreeMap<String, f64>| -> Result<String, String> {
        let mut doc: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
        for (m, turn) in cum {
            let now = t
                .members
                .iter()
                .find(|x| &x.id == m)
                .map_or(0.0, ohl_structure_edit::edits::member_roll);
            let mut deg = now.to_degrees() + turn;
            while deg > 180.0 {
                deg -= 360.0;
            }
            while deg <= -180.0 {
                deg += 360.0;
            }
            let deg = (deg * 100.0).round() / 100.0;
            ohl_structure_edit::edits::set_member_roll(&mut doc, m, deg.to_radians());
        }
        Ok(doc.to_string())
    };
    let counts = |r: &LayoutRun, js: &[&str]| -> Vec<(usize, usize)> {
        js.iter()
            .map(|j| {
                r.joints.iter().find(|o| o.joint == *j).map_or((0, 0), |o| {
                    if o.written {
                        (o.plan.ends.len(), o.plates.len())
                    } else {
                        (0, 0)
                    }
                })
            })
            .collect()
    };
    let joints_of = |g: &ChordGroup| -> Vec<String> {
        let mut js: Vec<String> = g
            .members
            .iter()
            .filter_map(|m| t.members.iter().find(|x| &x.id == m))
            .flat_map(|x| x.nodes.iter())
            .filter(|j| targets.contains(*j))
            .cloned()
            .collect();
        js.sort_unstable();
        js.dedup();
        js
    };
    let sums = |g: &ChordGroup, r: &LayoutRun| -> Vec<(usize, usize)> {
        let js = joints_of(g);
        let js: Vec<&str> = js.iter().map(String::as_str).collect();
        counts(r, &js)
    };
    let mut cum: BTreeMap<String, f64> = BTreeMap::new();
    let mut base = r0;
    let mut words = Vec::new();
    let mut seconds = base.seconds;
    for _ in 0..CHORD_ROUNDS {
        let d = crate::joint_details(&base.text, ask.library, ask.bolts)?;
        let groups = chord_groups(&d, &targets, &may);
        if groups.is_empty() {
            break;
        }
        let with = |set: &BTreeSet<usize>| {
            let mut c = cum.clone();
            for &k in set {
                for m in &groups[k].members {
                    *c.entry(m.clone()).or_insert(0.0) += groups[k].turn;
                }
            }
            c
        };
        let all: BTreeSet<usize> = (0..groups.len()).collect();
        let mut r = lay_out_with(&turned_text(&with(&all))?, &plain, true)?;
        seconds += r.seconds;
        let mut set: BTreeSet<usize> = all
            .iter()
            .copied()
            .filter(|&k| helps(&sums(&groups[k], &base), &sums(&groups[k], &r)))
            .collect();
        let mut confirmed = set == all;
        for _ in 0..FACE_ROUNDS {
            if set.is_empty() || confirmed {
                break;
            }
            r = lay_out_with(&turned_text(&with(&set))?, &plain, true)?;
            seconds += r.seconds;
            let keep: BTreeSet<usize> = set
                .iter()
                .copied()
                .filter(|&k| helps(&sums(&groups[k], &base), &sums(&groups[k], &r)))
                .collect();
            if keep == set {
                confirmed = true;
            } else {
                set = keep;
            }
        }
        if set.is_empty() || !confirmed {
            break;
        }
        for &k in &set {
            let g = &groups[k];
            let total =
                |v: Vec<(usize, usize)>| v.iter().fold((0, 0), |(e, p), (x, y)| (e + x, p + y));
            let (e0, p0) = total(sums(g, &base));
            let (e1, p1) = total(sums(g, &r));
            words.push(format!(
                "{}: its joints lay out {e1} ends on {p1} plates (before {e0} on {p0}); its \
                 roll written turned (a chord's roll: Orient angles' — the owner decides)",
                g.words
            ));
        }
        cum = with(&set);
        base = r;
    }
    base.notes.extend(words);
    base.seconds = seconds;
    Ok(base)
}

/// L12: the most rounds [`chord_turned`] makes (the program's bound).
const CHORD_ROUNDS: usize = 4;

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
    // J9 (D15): the pieces in line not one part, proposed or not (said,
    // never written by the run).
    let in_line = crate::in_line::in_line_pieces(t, Some(&targets));
    // L1 (a, c): the tower's joints bottom up, then out (its body, arms and
    // top from the legs' lines).
    let order = if settings.bottom_up {
        crate::joint_order::joint_order(t)
    } else {
        JointOrder::default()
    };
    // L1's guard (the joints planned as today, and those L1 changed).
    let guard = std::cell::RefCell::new(Guard::default());
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
    // J7: the leg splices proposed (said, never written by the run), from
    // the joints as planned (the layout's own values taken out).
    let splices = crate::leg_splices::leg_splices(&around_details, t, Some(&targets));
    // L11: the leg pieces their joints say are turned (proposed, never
    // applied; said in the notes).
    let leg_rolls: Vec<crate::LegRoll> = crate::leg_rolls(&around_details)
        .into_iter()
        .filter(|r| r.flagged() && r.estimates.iter().any(|e| targets.contains(&e.joint)))
        .collect();
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
    // L8: the ends the run names what they bolt onto in the planning copy
    // (member, joint), with what the copy gave before (none: nothing) — a
    // name written only where the end is laid out.
    let mut run_named: BTreeMap<(String, String), Option<String>> = BTreeMap::new();
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
            let in_line_here = js.iter().any(|x| x.member == m || x.onto == m);
            if in_line_here || joined.contains(m.as_str()) {
                continue;
            }
            run_named
                .entry((m.clone(), j.id.clone()))
                .or_insert_with(|| onto_in(&planning, &m, &j.id));
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
    // (The planning copy as it now is; L1 d may still name what an end
    // bolts onto in it.)
    let planning = std::cell::RefCell::new(planning);
    // Each member's direction (as it leaves its first joint in detail).
    let mut dir_of: BTreeMap<String, [f64; 3]> = BTreeMap::new();
    for j in &around_details.joints {
        for a in &j.arrivals {
            dir_of.entry(a.member.clone()).or_insert(a.direction);
        }
    }
    // 2. The members lying off what they bolt onto: a turn that lays them
    // flat — one roll for the whole member, every end within the flatness
    // allowed (those at joints not asked too: a turn never spoils an end
    // that was flat; J3, [`crate::whole_roll`]); a roll given by hand kept
    // unless replaced, a leg's or chord's turned to the tower's faces
    // (Orient angles) always. Class by class (owner, 2026-11-19: "start
    // with aligning the main legs then diagonals and finally secondary
    // braces with redundant members being last"): the legs and chords are
    // not turned to suit what bolts onto them; then the diagonals, the
    // secondary bracing (horizontals, plan and hip members, and the members
    // of no known class), the redundants — each class's turns found with
    // the classes before it turned.
    let mut notes = Vec::new();
    let member = |m: &str| t.members.iter().find(|x| x.id == m);
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
    // Each member's class in the owner's order (its class and how it
    // lies; a plan or hip member turned by Orient angles with the
    // secondary bracing).
    let class_of = |m: &str| -> u8 {
        if record
            .oriented
            .get(m)
            .is_some_and(|(_, k)| *k == crate::OrientKind::Plan)
        {
            return crate::joint_layout::ClassRank::Secondary as u8;
        }
        let class = member(m)
            .and_then(|x| x.design.as_ref())
            .and_then(|d| d.get("class"))
            .and_then(toml::Value::as_str);
        let dir = dir_of.get(m).copied().unwrap_or([0.0, 0.0, 1.0]);
        crate::joint_layout::class_rank(class, dir) as u8
    };
    let mut turned: BTreeMap<String, f64> = BTreeMap::new();
    let mut unknown_class: BTreeSet<String> = BTreeSet::new();
    let mut stage_details: Option<JointDetails> = None;
    let mut changed = false;
    for stage in [
        crate::joint_layout::ClassRank::Diagonal,
        crate::joint_layout::ClassRank::Secondary,
        crate::joint_layout::ClassRank::Redundant,
    ] {
        let stage = stage as u8;
        // The joints in detail with the classes before turned (none
        // turned yet: as they are).
        if changed {
            changed = false;
            let mut d = planning.borrow().clone();
            for (m, turn) in &turned {
                ohl_structure_edit::edits::set_member_roll(&mut d, m, roll_of(m) + turn);
            }
            stage_details = Some(
                crate::joints::joint_details_and_model(
                    &text_of(&d, &keep_around),
                    ask.library,
                    ask.bolts,
                    Some(&around),
                )?
                .0,
            );
        }
        let details = stage_details.as_ref().unwrap_or(&around_details);
        let mut asked: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for j in details.joints.iter().filter(|j| targets.contains(&j.id)) {
            for (m, _) in flat_turns(j) {
                if held.contains(&m)
                    || one_part.contains(&m)
                    || turned.contains_key(&m)
                    || asked.get(&m).is_some_and(|x| x.contains(&j.id))
                {
                    continue;
                }
                let class = class_of(&m);
                if class != stage && class != crate::joint_layout::ClassRank::Leg as u8 {
                    continue;
                }
                if class == crate::joint_layout::ClassRank::Leg as u8 {
                    // Once, with the first class.
                    if stage == crate::joint_layout::ClassRank::Diagonal as u8 {
                        notes.push(format!(
                            "{m}: lies off what it bolts onto at {}; a leg or chord (turned to \
                             the tower's faces by Orient angles, not to suit what bolts onto \
                             it): kept",
                            j.id
                        ));
                    }
                    continue;
                }
                if record
                    .oriented
                    .get(&m)
                    .is_some_and(|(_, k)| *k == crate::OrientKind::Chord)
                {
                    notes.push(format!(
                        "{m}: lies off what it bolts onto at {}; a leg or chord turned to the \
                         tower's faces (Orient angles): kept",
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
                if member(&m)
                    .and_then(|x| x.design.as_ref())
                    .and_then(|d| d.get("class"))
                    .is_none()
                {
                    unknown_class.insert(m.clone());
                }
                asked.entry(m).or_default().push(j.id.clone());
            }
        }
        for (m, ends) in &asked {
            // Its turns at all its ends (those flat already too: the turn
            // keeps them flat), the end asked first; one roll for the whole
            // member: every end within the flatness allowed, the same legs
            // bolted first, else (an equal-leg angle) its leg bolted
            // changing between its ends; any turn, the smallest that does.
            let end_turn = |j: &str| -> Option<EndTurn> {
                let a = details.joint(j)?.arrivals.iter().find(|a| &a.member == m)?;
                let f = a.flat?;
                Some(EndTurn {
                    turn: f.turn,
                    either_leg: either_leg(a),
                })
            };
            let mut all: Vec<(String, EndTurn)> = Vec::new();
            for j in ends.iter().chain(details.joints.iter().map(|j| &j.id)) {
                if all.iter().any(|(x, _)| x == j) {
                    continue;
                }
                if let Some(e) = end_turn(j) {
                    all.push((j.clone(), e));
                }
            }
            if all.is_empty() {
                continue;
            }
            let turns: Vec<EndTurn> = all.iter().map(|(_, e)| *e).collect();
            let whole = whole_roll(&turns, crate::FLAT_OFF);
            // L1 (a), bottom up, then out (owner 2026-11-19: "More things
            // align on the body from the bottom up and then less at the
            // mast ends"): no one roll lays every end flat and none lies
            // flat now — the member turned to lie flat at its end first in
            // that order; its other ends as they come (step 4's welded
            // plate or a hand detail). A turn never spoils an end that was
            // flat (tried: turning to the first end whatever the others
            // laid out fewer joints whole on the real towers), and is kept
            // only where it helps (below).
            if whole.is_none()
                && settings.bottom_up
                && laid_elsewhere(m).is_none()
                && all.iter().all(|(j, _)| {
                    details
                        .joint(j)
                        .and_then(|j| j.arrivals.iter().find(|a| &a.member == m))
                        .and_then(|a| a.flat)
                        .is_some_and(|f| f.off > crate::FLAT_OFF)
                })
                && let Some((j0, e0)) = all
                    .iter()
                    .min_by_key(|(j, _)| (order.rank_of(j), j.clone()))
            {
                let turn = ((e0.turn.to_degrees() * 100.0).round() / 100.0).to_radians();
                notes.push(format!(
                    "{m}: no one roll lays all its ends flat ({}): turned to lie flat at {j0}, its \
                     end first bottom up, then out; its others as they come",
                    all.iter()
                        .map(|(j, x)| format!("{j} {:.1}°", x.turn.to_degrees()))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                turned.insert(m.clone(), turn);
                changed = true;
                continue;
            }
            let Some((mid, way)) = whole else {
                notes.push(format!(
                    "{m}: lies off what it bolts onto at {}, by different turns (one roll for \
                     the whole member: the turn would spoil the other end{}): not turned",
                    all.iter()
                        .map(|(j, x)| format!("{j} ({:.1}°)", x.turn.to_degrees()))
                        .collect::<Vec<_>>()
                        .join(" and "),
                    if turns.iter().any(|e| e.either_leg) {
                        ", whichever leg is bolted"
                    } else {
                        ""
                    }
                ));
                continue;
            };
            let mid = mid.to_degrees();
            if way == RollWay::LegChanges {
                notes.push(format!(
                    "{m}: one roll for the whole member lays both its ends flat with its leg \
                     bolted changing between them (an equal-leg angle; {})",
                    all.iter()
                        .map(|(j, x)| format!("{j} {:.1}°", x.turn.to_degrees()))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
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
            changed = true;
        }
    }
    for m in &unknown_class {
        notes.push(format!(
            "{m}: no class given: turned with the secondary bracing (after the diagonals, \
             before the redundants)"
        ));
    }
    // The joints asked in detail with `turns` (their model: the code's edge
    // distances), each planned.
    // (`s`: the settings planned with — the half turns' check without the
    // bent plates, J2b.)
    // (`g`: the guard the plans are told to — a scratch one for a plan only
    // weighed, L10.)
    let planned_in = |turns: &BTreeMap<&String, f64>,
                      s: &LayoutSettings,
                      g: &std::cell::RefCell<Guard>|
     -> Result<Planned, String> {
        let mut d = planning.borrow().clone();
        for (m, turn) in turns {
            ohl_structure_edit::edits::set_member_roll(&mut d, m, roll_of(m) + turn);
        }
        let (details, model) = crate::joints::joint_details_and_model(
            &text_of(&d, &keep_targets),
            ask.library,
            ask.bolts,
            Some(&targets),
        )?;
        let plans = plans_of(&details, model.as_ref(), &targets, s, &joins, &order, g);
        Ok((details, model, plans))
    };
    let planned_with =
        |turns: &BTreeMap<&String, f64>, s: &LayoutSettings| planned_in(turns, s, &guard);
    // The half turns are checked as before the bent plates (the owner's
    // order: a member flat on the flange or a flat plate before a bent
    // plate); their plans then made again with them.
    let flat_only = LayoutSettings {
        bent_plates: false,
        ..settings.clone()
    };
    let with_bent = |p: Planned| -> Planned {
        if !settings.bent_plates {
            return p;
        }
        let plans = plans_of(
            &p.0,
            p.1.as_ref(),
            &targets,
            &settings,
            &joins,
            &order,
            &guard,
        );
        (p.0, p.1, plans)
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
    let mut first: Option<Planned>;
    {
        let turns: BTreeMap<&String, f64> = turned.iter().map(|(m, x)| (m, *x)).collect();
        let base = planned_with(&turns, &flat_only)?;
        let mut flips: BTreeSet<String> = BTreeSet::new();
        for (_, p) in &base.2 {
            for h in &p.hand {
                let Some((m, why)) = h.split_once(": ") else {
                    continue;
                };
                let inner = why.contains("runs towards the angle's other leg")
                    || why.contains("do not all land on the inner face")
                    || why.contains("its outstanding leg points towards the flange");
                // L2 (c), 2026-11-20: an end the clash-aware plan leaves for
                // what it (or its plate) would run into — the other face of
                // the flange tried too (the half turn), kept as above.
                let clash = settings.clash_tries
                    && settings.exact_plan
                    && (why.contains("the plate would run into")
                        || why.contains("mm pulled back to clear"));
                if !(inner || clash)
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
                    || class_of(m) == crate::joint_layout::ClassRank::Leg as u8
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
        // Half a turn more, as written: degrees to a hundredth.
        let half = |x: f64| {
            let mut d = x.to_degrees() + 180.0;
            if d > 180.0 {
                d -= 360.0;
            }
            ((d * 100.0).round() / 100.0).to_radians()
        };
        // Ends laid out (planned) of member `m`, and at its joints.
        let count = |plans: &[(String, JointPlan)], m: &str| -> (usize, usize) {
            let joints = &members_here[m];
            plans
                .iter()
                .filter(|(j, _)| joints.contains(j))
                .fold((0, 0), |(own, all), (_, p)| {
                    (
                        own + usize::from(p.ends.iter().any(|e| e.member == m)),
                        all + p.ends.len(),
                    )
                })
        };
        let mut flipped: Option<Planned> = None;
        if !flips.is_empty() {
            let mut with = turned.clone();
            for m in &flips {
                let x = with.get(m).copied().unwrap_or(0.0);
                with.insert(m.clone(), half(x));
            }
            let turns: BTreeMap<&String, f64> = with.iter().map(|(m, x)| (m, *x)).collect();
            let p = planned_with(&turns, &flat_only)?;
            for m in &flips {
                let (own0, all0) = count(&base.2, m);
                let (own1, all1) = count(&p.2, m);
                if own1 > own0 && all1 >= all0 {
                    chosen.insert(m.clone());
                }
            }
            for m in &chosen {
                let x = turned.get(m).copied().unwrap_or(0.0);
                turned.insert(m.clone(), half(x));
            }
            flipped = Some(p);
        }
        // L3b (2026-11-20): an equal-leg angle the half turn did not help,
        // left at an end at most [`crate::BEND_SMALL_MAX`] off its flange
        // with its outstanding leg towards the flange — its other leg
        // bolted instead (a quarter turn: the leg mirrored about the heel,
        // its outstanding leg the other way, so that a plate bent by as
        // much can take that end; an end lying flat stays flat, the other
        // leg on its flange), the quarter turn that turns that end's
        // outstanding leg away from its flange. Kept as the half turns are.
        let mut mirrors: BTreeMap<String, f64> = BTreeMap::new();
        if settings.bend_facing && settings.small_bends {
            for (jid, p) in &base.2 {
                let Some(j) = base.0.joint(jid) else { continue };
                for h in &p.hand {
                    let Some((m, why)) = h.split_once(": ") else {
                        continue;
                    };
                    if !flips.contains(m)
                        || chosen.contains(m)
                        || mirrors.contains_key(m)
                        || !why.contains("its outstanding leg points towards the flange")
                    {
                        continue;
                    }
                    let Some(a) = j.arrivals.iter().find(|a| a.member == m) else {
                        continue;
                    };
                    let equal_legs = members_here[m].iter().all(|jj| {
                        base.0.joint(jj).is_none_or(|x| {
                            x.arrivals.iter().filter(|b| b.member == m).all(either_leg)
                        })
                    });
                    if !equal_legs || !a.flat.is_some_and(|f| f.off <= crate::BEND_SMALL_MAX) {
                        continue;
                    }
                    let q = [std::f64::consts::FRAC_PI_2, -std::f64::consts::FRAC_PI_2]
                        .into_iter()
                        .find(|q| {
                            crate::joint_layout::turned_end(j, a, *q, true)
                                .is_some_and(|(off, takes)| takes && off <= crate::BEND_SMALL_MAX)
                        });
                    if let Some(q) = q {
                        mirrors.insert(m.to_owned(), q);
                    }
                }
            }
        }
        let mut mirrored: BTreeSet<String> = BTreeSet::new();
        let mut mirror_plan: Option<Planned> = None;
        if !mirrors.is_empty() {
            // A quarter turn more, as written: degrees to a hundredth.
            let quarter = |x: f64, q: f64| {
                let mut d = (x + q).to_degrees();
                while d > 180.0 {
                    d -= 360.0;
                }
                while d <= -180.0 {
                    d += 360.0;
                }
                ((d * 100.0).round() / 100.0).to_radians()
            };
            let mut with = turned.clone();
            for (m, q) in &mirrors {
                let x = with.get(m).copied().unwrap_or(0.0);
                with.insert(m.clone(), quarter(x, *q));
            }
            let turns: BTreeMap<&String, f64> = with.iter().map(|(m, x)| (m, *x)).collect();
            let p = planned_with(&turns, &flat_only)?;
            for m in mirrors.keys() {
                let (own0, all0) = count(&base.2, m);
                let (own1, all1) = count(&p.2, m);
                if own1 > own0 && all1 >= all0 {
                    mirrored.insert(m.clone());
                }
            }
            for m in &mirrored {
                let x = turned.get(m).copied().unwrap_or(0.0);
                turned.insert(m.clone(), quarter(x, mirrors[m]));
            }
            if mirrored.len() == mirrors.len() {
                mirror_plan = Some(p);
            }
        }
        // The plan with the turns as now chosen, where one made has them.
        first = if !mirrored.is_empty() {
            mirror_plan.map(with_bent)
        } else if chosen.is_empty() {
            Some(with_bent(base))
        } else if chosen == flips {
            flipped.map(with_bent)
        } else {
            None
        };
        // 2c. L10 (the owner's idea of 2026-11-21, from photographs of real
        // towers: the horizontals on the outer face of a leg's flange, the
        // diagonal on its inner face, meeting at one point without a
        // plate): where two or more ends on one face of a leg's flange go
        // onto a plate, or one is left for want of room beside another,
        // one of them half a turn — onto the flange's other face
        // ([`crate::other_face`]). Weighed on the whole plan (with the bent
        // plates): kept for a member only where its joints lay out more
        // ends, or as many on fewer plates, and none fewer at any of them;
        // applied only where its leg then lies no farther from the work
        // point across the flange than now at every end so asked (the
        // analysis leaves every such seat out, the outer face's as the
        // inner's: nothing more left out than before) — else proposed in
        // the notes with the bending that offset would add.
        if settings.other_face {
            let turns: BTreeMap<&String, f64> = turned.iter().map(|(m, x)| (m, *x)).collect();
            let base = match first.take() {
                Some(p) => p,
                None => planned_with(&turns, &settings)?,
            };
            let may_turn = |m: &str| -> bool {
                // (One part with another, given or joined — as planned: its
                // roll stays alike with its partner's.)
                if held.contains(m)
                    || one_part.contains(m)
                    || members_here.get(m).is_some_and(|n| {
                        n.iter().any(|j| {
                            base.0.joint(j).is_some_and(|d| {
                                d.arrivals
                                    .iter()
                                    .any(|a| a.member == m && a.continuous.is_some())
                            })
                        })
                    })
                    || chosen.contains(m)
                    || mirrored.contains(m)
                    || !members_here.contains_key(m)
                    || laid_elsewhere(m).is_some()
                    || class_of(m) == crate::joint_layout::ClassRank::Leg as u8
                    || record
                        .oriented
                        .get(m)
                        .is_some_and(|(_, k)| *k == crate::OrientKind::Chord)
                {
                    return false;
                }
                let automatic = record.rolled.contains_key(m) || record.oriented.contains_key(m);
                let by_hand = member(m).is_some_and(|x| x.roll.is_some()) && !automatic;
                !by_hand || ask.replace_hand
            };
            let host_class = |m: &str| -> u8 {
                if record
                    .oriented
                    .get(m)
                    .is_some_and(|(_, k)| *k == crate::OrientKind::Chord)
                {
                    crate::joint_layout::ClassRank::Leg as u8
                } else {
                    class_of(m)
                }
            };
            let tries = crate::other_face::face_tries(
                &base.2,
                &base.0,
                &crate::other_face::MemberAsk {
                    may_turn: &may_turn,
                    class: &host_class,
                },
                settings.face_pattern,
            );
            let (kept, words, plan) =
                other_faces(&tries, &base, &turned, &members_here, &targets, &|t| {
                    let scratch = std::cell::RefCell::new(Guard {
                        changed: BTreeMap::new(),
                        plain: guard.borrow().plain.clone(),
                        l11: BTreeMap::new(),
                        no_l11: guard.borrow().no_l11.clone(),
                    });
                    let turns: BTreeMap<&String, f64> = t.iter().map(|(m, x)| (m, *x)).collect();
                    let p = planned_in(&turns, &settings, &scratch)?;
                    Ok((p, scratch.into_inner()))
                })?;
            notes.extend(words);
            match plan {
                Some((p, g)) => {
                    for m in &kept {
                        let x = turned.get(m).copied().unwrap_or(0.0);
                        turned.insert(m.clone(), half(x));
                    }
                    // (As if planned with the run's guard.)
                    guard.borrow_mut().changed.extend(g.changed);
                    guard.borrow_mut().l11.extend(g.l11);
                    first = Some(p);
                }
                None => first = Some(base),
            }
        }
    }
    // (L1 d: what each end bolted onto before, at the joints where it
    // changed.)
    let mut onto_back: BTreeMap<String, Vec<(String, Option<String>)>> = BTreeMap::new();
    // L1 (d), owner 2026-11-19: what an end bolts onto chosen in the
    // search — for an end the plans leave for a hand detail, the other
    // members it could bolt onto tried too (today's rule's choice first:
    // it stays where the others lay out no more of the joint's ends), at
    // most [`ONTO_TRIES`] passes, each the next member for every such end.
    // (Not for the planner relaxed: without writing, no clash rounds to
    // judge by.)
    if settings.onto_search && !relaxed {
        let turns: BTreeMap<&String, f64> = turned.iter().map(|(m, x)| (m, *x)).collect();
        let base = match first.take() {
            Some(p) => p,
            None => planned_with(&turns, &settings)?,
        };
        let mut plans: BTreeMap<String, JointPlan> = base.2.iter().cloned().collect();
        // (What an end bolts onto given by hand is kept, unless replaced.)
        let hand_onto = |m: &str, j: &str| {
            !ask.replace_hand
                && t.connection(m, j).is_some_and(|c| c.onto.is_some())
                && !record
                    .ends
                    .get(&(m.to_owned(), j.to_owned()))
                    .is_some_and(|f| f.contains("onto"))
        };
        let mut tried: BTreeSet<(String, String, String)> = BTreeSet::new();
        let mut done: BTreeSet<String> = BTreeSet::new();
        let mut accepted = false;
        // Today's ends at each joint (the guard's measure).
        let base_ends: BTreeMap<String, usize> = base
            .2
            .iter()
            .map(|(j, p)| {
                let n = guard
                    .borrow()
                    .changed
                    .get(j)
                    .copied()
                    .unwrap_or(p.ends.len());
                (j.clone(), n)
            })
            .collect();
        let mut chosen = connection_edits::Connections::of(&planning.borrow());
        for _ in 0..ONTO_TRIES {
            let mut alt: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
            for (jid, p) in &plans {
                let Some(j) = base.0.joint(jid).filter(|_| !done.contains(jid)) else {
                    continue;
                };
                for m in &p.hand_members {
                    let Some(a) = j.arrivals.iter().find(|a| &a.member == m) else {
                        continue;
                    };
                    if hand_onto(m, jid) {
                        continue;
                    }
                    let now = a.onto.value.clone();
                    if let Some(h) =
                        crate::joint_layout::onto_candidates(j, a)
                            .into_iter()
                            .find(|h| {
                                now.as_ref() != Some(h)
                                    && !tried.contains(&(jid.clone(), m.clone(), h.clone()))
                            })
                    {
                        tried.insert((jid.clone(), m.clone(), h.clone()));
                        alt.entry(jid.clone()).or_default().push((m.clone(), h));
                    }
                }
            }
            if alt.is_empty() {
                break;
            }
            let mut d = planning.borrow().clone();
            let mut c = connection_edits::Connections::of(&d);
            for (jid, list) in &alt {
                for (m, h) in list {
                    let _ = c.set(&mut d, m, jid, "onto", Some(&FieldValue::Word(h.clone())));
                }
            }
            for (m, turn) in &turns {
                ohl_structure_edit::edits::set_member_roll(&mut d, m, roll_of(m) + turn);
            }
            let affected: BTreeSet<String> = alt.keys().cloned().collect();
            let (dd, mm) = crate::joints::joint_details_and_model(
                &text_of(&d, &keep_for(&affected)),
                ask.library,
                ask.bolts,
                Some(&affected),
            )?;
            let alone = LayoutSettings {
                once_per_kind: false,
                ..settings.clone()
            };
            let scratch = std::cell::RefCell::new(Guard::default());
            for (jid, p) in plans_of(
                &dd,
                mm.as_ref(),
                &affected,
                &alone,
                &joins,
                &order,
                &scratch,
            ) {
                // (No end of it left as it is that was not before: an end
                // named onto a member it cannot lie on is not laid out.)
                let names = |l: &[String]| -> BTreeSet<String> {
                    l.iter()
                        .filter_map(|w| w.split_once(": ").map(|x| x.0.to_owned()))
                        .collect()
                };
                if let Some(b) = plans.get(&jid)
                    && p.ends.len() > b.ends.len()
                    && names(&p.left).is_subset(&names(&b.left))
                {
                    // For the guard: today's ends there, and what each end
                    // bolted onto in the planning copy (none: not named).
                    let j0 = base.0.joint(&jid);
                    let back: Vec<(String, Option<String>)> = alt[&jid]
                        .iter()
                        .map(|(m, _)| {
                            let was = j0
                                .and_then(|j| j.arrivals.iter().find(|a| &a.member == m))
                                .filter(|a| a.onto.source == crate::joints::Source::Given)
                                .and_then(|a| a.onto.value.clone());
                            (m.clone(), was)
                        })
                        .collect();
                    let n = base_ends.get(&jid).copied().unwrap_or(b.ends.len());
                    onto_back.insert(jid.clone(), back);
                    guard.borrow_mut().changed.insert(jid.clone(), n);
                    for (m, h) in &alt[&jid] {
                        run_named
                            .entry((m.clone(), jid.clone()))
                            .or_insert_with(|| onto_in(&planning.borrow(), m, &jid));
                        let _ = chosen.set(
                            &mut planning.borrow_mut(),
                            m,
                            &jid,
                            "onto",
                            Some(&FieldValue::Word(h.clone())),
                        );
                    }
                    plans.insert(jid.clone(), p);
                    done.insert(jid);
                    accepted = true;
                }
            }
        }
        if accepted {
            (around_details, around_model) = crate::joints::joint_details_and_model(
                &text_of(&planning.borrow(), &keep_around),
                ask.library,
                ask.bolts,
                Some(&around),
            )?;
            first = Some(planned_with(&turns, &settings)?);
        } else {
            first = Some(base);
        }
    }
    // 3.–4. Each joint planned with the turns and written; a turned member
    // none of whose joints is laid out with the turn is not turned (its
    // roll as before), and the joints planned again without it — until
    // every turn left helps.
    // (The owner's order of the classes: of two members in one clash the
    // later class left first.)
    let class_rank = |m: &str| -> u8 { class_of(m) };
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
    // L8: the joints planned as drawn in the pass before (their ends and
    // plates laid out as named, and the names taken out), and those done.
    type Names = Vec<(String, Option<String>)>;
    type Laid = (usize, usize, BTreeSet<String>);
    let mut as_drawn: BTreeMap<String, (Laid, Names)> = BTreeMap::new();
    let mut as_drawn_done: BTreeSet<String> = BTreeSet::new();
    let mut as_drawn_passes = 0;
    let mut as_drawn_plan: Option<Planned> = None;
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
        let held;
        let (details, model, plans) = if let Some(p) = as_drawn_plan.take() {
            // (L8: the joints planned as drawn again, the others as in the
            // pass before.)
            held = p;
            (&held.0, held.1.as_ref(), held.2.clone())
        } else {
            match (turns.is_empty(), refused.is_empty(), &first) {
                (true, _, _) => (
                    &around_details,
                    around_model.as_ref(),
                    plans_of(
                        &around_details,
                        around_model.as_ref(),
                        &targets,
                        &settings,
                        &joins,
                        &order,
                        &guard,
                    ),
                ),
                (false, true, Some(p)) => (&p.0, p.1.as_ref(), p.2.clone()),
                _ => {
                    fresh = planned_with(&turns, &settings)?;
                    (&fresh.0, fresh.1.as_ref(), fresh.2.clone())
                }
            }
        };
        let plans_first = plans.clone();
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
                      floors: &BTreeMap<String, (f64, String)>,
                      plated: &BTreeSet<String>,
                      direct: &BTreeSet<String>|
         -> Option<JointPlan> {
            let j = details.joint(joint)?;
            let edge = edge_rule(model);
            let mut p = crate::joint_layout::plan_joint_ctx(
                j,
                &settings,
                &edge,
                except,
                floors,
                crate::joint_layout::NODE_LIMIT,
                &ctx_of(
                    &settings,
                    &order,
                    joint,
                    plated,
                    direct,
                    guard.borrow().plain.contains(joint),
                    guard.borrow().no_l11.contains(joint),
                ),
            );
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
                in_line: in_line.clone(),
                splices: splices.clone(),
                leg_rolls: leg_rolls.clone(),
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
            // L1's guard: a joint L1 changed laid out with fewer ends than
            // today's plan of it lays out — planned again as today.
            if useless.is_empty() {
                let mut g = guard.borrow_mut();
                let mut back: Vec<String> = Vec::new();
                for o in &outcomes {
                    let Some(&n) = g.changed.get(&o.joint) else {
                        continue;
                    };
                    let laid = if o.written { o.plan.ends.len() } else { 0 };
                    if laid < n && !g.plain.contains(&o.joint) {
                        back.push(o.joint.clone());
                    }
                }
                // L11's guard: a joint its steps changed laid out with fewer
                // ends than its plan without them lays out — planned again
                // without them.
                let mut back11: Vec<String> = Vec::new();
                for o in &outcomes {
                    let Some(&n) = g.l11.get(&o.joint) else {
                        continue;
                    };
                    let laid = if o.written { o.plan.ends.len() } else { 0 };
                    if laid < n && !g.no_l11.contains(&o.joint) && !back.contains(&o.joint) {
                        back11.push(o.joint.clone());
                    }
                }
                for j in &back11 {
                    g.no_l11.insert(j.clone());
                    touched.insert(j.clone());
                }
                for j in &back {
                    g.plain.insert(j.clone());
                    touched.insert(j.clone());
                    if let Some(list) = onto_back.remove(j) {
                        let mut doc = planning.borrow_mut();
                        let mut c = connection_edits::Connections::of(&doc);
                        for (m, was) in list {
                            let _ =
                                c.set(&mut doc, &m, j, "onto", was.map(FieldValue::Word).as_ref());
                        }
                    }
                }
                if !back.is_empty() || !back11.is_empty() {
                    drop(g);
                    if settings.onto_search {
                        (around_details, around_model) = crate::joints::joint_details_and_model(
                            &text_of(&planning.borrow(), &keep_around),
                            ask.library,
                            ask.bolts,
                            Some(&around),
                        )?;
                    }
                    first = None;
                    continue;
                }
            }
            // L8 (L7's proposal 3): a member the run named what it bolts
            // onto in the planning copy but does not lay out is written
            // without the name — drawn elsewhere than planned. Its joint
            // planned once more with the name taken out (the member where
            // the file draws it); kept where it then lays out more ends, or
            // as many on no more plates, else named again (at most
            // [`AS_DRAWN_PASSES`] such passes).
            if useless.is_empty() && settings.as_drawn && !run_named.is_empty() {
                // A joint's ends laid out, its plates and the members it
                // lays out or leaves for a hand detail.
                let laid = |o: &JointOutcome| -> Laid {
                    let (ends, plates) = if o.written {
                        (o.plan.ends.len(), o.plates.len())
                    } else {
                        (0, 0)
                    };
                    let asked = o
                        .plan
                        .ends
                        .iter()
                        .map(|e| e.member.clone())
                        .chain(o.plan.hand_members.iter().cloned())
                        .collect();
                    (ends, plates, asked)
                };
                let mut again = false;
                let mut doc = planning.borrow_mut();
                let mut c = connection_edits::Connections::of(&doc);
                for (j, (before, names)) in std::mem::take(&mut as_drawn) {
                    let Some(o) = outcomes.iter().find(|o| o.joint == j) else {
                        continue;
                    };
                    let now = laid(o);
                    // (No member it laid out or left for a hand detail now
                    // left as it is, unsaid.)
                    let keep = now.2.is_superset(&before.2)
                        && (now.0 > before.0 || (now.0 == before.0 && now.1 <= before.1));
                    if keep {
                        let ms: Vec<&str> = names.iter().map(|(m, _)| m.as_str()).collect();
                        notes.push(format!(
                            "{j}: planned with {} where the file draws it (not laid out \
                             there: the run's name for what it bolts onto taken out)",
                            ms.join(", ")
                        ));
                        continue;
                    }
                    for (m, v) in names {
                        let _ = c.set(&mut doc, &m, &j, "onto", v.map(FieldValue::Word).as_ref());
                    }
                    touched.insert(j);
                    again = true;
                }
                if as_drawn_passes < AS_DRAWN_PASSES {
                    for o in &outcomes {
                        if as_drawn_done.contains(&o.joint) {
                            continue;
                        }
                        let names: Names = run_named
                            .iter()
                            .filter(|((m, j), plain)| {
                                j == &o.joint
                                    && !(o.written && o.plan.ends.iter().any(|e| &e.member == m))
                                    && onto_in(&doc, m, j) != **plain
                                    // (The face it lies on given by hand:
                                    // given for the member named, kept so.)
                                    && field_in(&doc, m, j, "face").is_none()
                            })
                            .map(|((m, _), _)| (m.clone(), onto_in(&doc, m, &o.joint)))
                            .collect();
                        if names.is_empty() {
                            continue;
                        }
                        for (m, _) in &names {
                            let plain = run_named[&(m.clone(), o.joint.clone())].clone();
                            let _ = c.set(
                                &mut doc,
                                m,
                                &o.joint,
                                "onto",
                                plain.map(FieldValue::Word).as_ref(),
                            );
                        }
                        as_drawn_done.insert(o.joint.clone());
                        as_drawn.insert(o.joint.clone(), (laid(o), names));
                        touched.insert(o.joint.clone());
                        again = true;
                    }
                    as_drawn_passes += 1;
                }
                drop(doc);
                if again {
                    // Only the joints whose names changed planned again
                    // (in detail from the planning copy now, with the
                    // turns); the others as in this pass.
                    let mut d = planning.borrow().clone();
                    for (m, turn) in &turns {
                        ohl_structure_edit::edits::set_member_roll(&mut d, m, roll_of(m) + turn);
                    }
                    let (dd, _) = crate::joints::joint_details_and_model(
                        &text_of(&d, &keep_for(&touched)),
                        ask.library,
                        ask.bolts,
                        Some(&touched),
                    )?;
                    let mut ps = plans_first;
                    for (jid, p) in
                        plans_of(&dd, model, &touched, &settings, &joins, &order, &guard)
                    {
                        if let Some(x) = ps.iter_mut().find(|x| x.0 == jid) {
                            x.1 = p;
                        }
                    }
                    let mut merged = details.clone();
                    for jd in dd.joints {
                        if let Some(x) = merged.joints.iter_mut().find(|x| x.id == jd.id) {
                            *x = jd;
                        }
                    }
                    as_drawn_plan = Some((merged, model.cloned(), ps));
                    if turns.is_empty() {
                        (around_details, around_model) = crate::joints::joint_details_and_model(
                            &text_of(&planning.borrow(), &keep_around),
                            ask.library,
                            ask.bolts,
                            Some(&around),
                        )?;
                    }
                    first = None;
                    continue;
                }
            }
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
            in_line: in_line.clone(),
            splices: splices.clone(),
            leg_rolls: leg_rolls.clone(),
        });
    }
}

/// What [`other_faces`] keeps: the members turned half a turn, the notes,
/// and the plan with exactly those turned and the scratch guard it was
/// planned with (none: nothing kept).
type FacesKept = (Vec<String>, Vec<String>, Option<(Planned, Guard)>);

/// The joints planned with the turns given, and the scratch guard they were
/// planned with ([`other_faces`]).
type PlanTurned<'a> = &'a dyn Fn(&BTreeMap<String, f64>) -> Result<(Planned, Guard), String>;

/// L10: the ends `tries` on their flange's other face weighed against
/// `base` (planned with `turned`): each member half a turn, all at once,
/// then kept where its joints (of `members_here` asked, `targets`) lay out
/// more ends, or as many on fewer plates, none fewer at any of them, and
/// every end of it so moved lies no farther from the work point across the
/// flange than now; planned again with only those until each kept still
/// helps (at most [`FACE_ROUNDS`] plans). Those that help but lie farther
/// out are proposed in the notes. `plan`: the joints planned with the turns
/// given (a scratch guard).
fn other_faces(
    tries: &[crate::other_face::FaceTry],
    base: &Planned,
    turned: &BTreeMap<String, f64>,
    members_here: &BTreeMap<&str, &[String; 2]>,
    targets: &BTreeSet<String>,
    plan: PlanTurned<'_>,
) -> Result<FacesKept, String> {
    use crate::other_face::{counts_at, ends_moved, helps, stress_ratio};
    if tries.is_empty() {
        return Ok((Vec::new(), Vec::new(), None));
    }
    let half = |x: f64| {
        let mut d = x.to_degrees() + 180.0;
        if d > 180.0 {
            d -= 360.0;
        }
        ((d * 100.0).round() / 100.0).to_radians()
    };
    let joints_of = |m: &str| -> Vec<&str> {
        members_here
            .get(m)
            .map(|n| {
                n.iter()
                    .filter(|j| targets.contains(*j))
                    .map(String::as_str)
                    .collect()
            })
            .unwrap_or_default()
    };
    let with = |set: &BTreeSet<String>| -> BTreeMap<String, f64> {
        let mut w = turned.clone();
        for m in set {
            let x = w.get(m).copied().unwrap_or(0.0);
            w.insert(m.clone(), half(x));
        }
        w
    };
    let all: BTreeSet<String> = tries.iter().map(|t| t.member.clone()).collect();
    let (p1, g1) = plan(&with(&all))?;
    let mut words = Vec::new();
    let mut auto: BTreeSet<String> = BTreeSet::new();
    let mm = |v: f64| v.abs() * 1000.0;
    for m in &all {
        let js = joints_of(m);
        let (b, a) = (counts_at(&base.2, &js), counts_at(&p1.2, &js));
        if !helps(&b, &a) {
            continue;
        }
        let moved = ends_moved(m, &js, &base.2, &base.0);
        let farther = moved
            .iter()
            .filter(|x| x.4.abs() > x.3.abs() + 1e-4)
            .max_by(|x, y| (x.4.abs() - x.3.abs()).total_cmp(&(y.4.abs() - y.3.abs())));
        let sum = |v: &[(usize, usize)]| v.iter().fold((0, 0), |(e, p), (x, y)| (e + x, p + y));
        let ((e0, p0), (e1, p1)) = (sum(&b), sum(&a));
        match farther {
            None => {
                auto.insert(m.clone());
            }
            Some((j, host, to_inner, now, then)) => {
                let e = then.abs() - now.abs();
                let ratio = base
                    .0
                    .joint(j)
                    .and_then(|d| d.arrivals.iter().find(|x| &x.member == m))
                    .and_then(|x| stress_ratio(x, e));
                words.push(format!(
                    "{m}: half a turn onto the {} face of {host}'s flange at {j} would lay out \
                     {e1} ends on {p1} plates at its joints (now {e0} on {p0}; planned with the \
                     other faces tried) — proposed, not applied (L10, members on either side of \
                     a leg's flange): its leg {:.0} mm from the work point across the flange \
                     there, now {:.0} mm — {:.0} mm farther{}, not in the analysis",
                    if *to_inner { "inner" } else { "outer" },
                    mm(*then),
                    mm(*now),
                    e * 1000.0,
                    ratio.map_or(String::new(), |r| format!(
                        " (its bending stress e·A/W {r:.2} of its axial stress more)"
                    ))
                ));
            }
        }
    }
    // Planned again with only those kept, until each still helps.
    let mut set = auto;
    let mut last = if set == all { Some((p1, g1)) } else { None };
    for _ in 0..FACE_ROUNDS {
        if set.is_empty() || last.is_some() {
            break;
        }
        let (p, g) = plan(&with(&set))?;
        let keep: BTreeSet<String> = set
            .iter()
            .filter(|m| {
                let js = joints_of(m);
                helps(&counts_at(&base.2, &js), &counts_at(&p.2, &js))
            })
            .cloned()
            .collect();
        if keep == set {
            last = Some((p, g));
        } else {
            set = keep;
        }
    }
    let Some(last) = last.filter(|_| !set.is_empty()) else {
        return Ok((Vec::new(), words, None));
    };
    for m in &set {
        let js = joints_of(m);
        let sum = |v: Vec<(usize, usize)>| v.iter().fold((0, 0), |(e, p), (x, y)| (e + x, p + y));
        let (e0, p0) = sum(counts_at(&base.2, &js));
        let (e1, p1) = sum(counts_at(&last.0.2, &js));
        for (j, host, to_inner, now, then) in ends_moved(m, &js, &base.2, &base.0) {
            words.push(format!(
                "{m}: half a turn onto the {} face of {host}'s flange at {j} (L10, members on \
                 either side of a leg's flange): its joints lay out {e1} ends on {p1} plates \
                 (before {e0} on {p0}); its leg {:.0} mm from the work point across the flange, \
                 before {:.0} mm (no farther: nothing more left out of the analysis)",
                if to_inner { "inner" } else { "outer" },
                mm(then),
                mm(now),
            ));
        }
    }
    Ok((set.into_iter().collect(), words, Some(last)))
}

/// L10: the most plans [`other_faces`] makes to confirm the members it
/// keeps (the program's bound).
const FACE_ROUNDS: usize = 3;

/// L12: a chord's pieces turned together and the turn their joints say
/// (degrees), in words.
struct ChordGroup {
    members: Vec<String>,
    turn: f64,
    words: String,
}

/// L12: the chords of `d` whose joints say they are turned
/// ([`crate::piece_rolls`] of [`crate::joint_classes::chord_pieces`]): the
/// pieces in line turned alike taken together (one chord: every piece
/// named as another's partner, transitively), their flagged pieces' turns
/// all one way within [`crate::leg_roll::SPREAD_DEG`], the chord turned by
/// their middle; only where a flagged estimate lies at a joint asked and
/// `may` turn every piece. Deterministic (the members' order).
fn chord_groups(
    d: &JointDetails,
    targets: &BTreeSet<String>,
    may: &dyn Fn(&str) -> bool,
) -> Vec<ChordGroup> {
    let rolls = crate::piece_rolls(d, &crate::joint_classes::chord_pieces);
    // One chord: the pieces linked as partners.
    let mut root: BTreeMap<String, String> = BTreeMap::new();
    fn find(root: &mut BTreeMap<String, String>, m: &str) -> String {
        let mut x = m.to_owned();
        while let Some(p) = root.get(&x).filter(|p| **p != x) {
            x = p.clone();
        }
        root.insert(m.to_owned(), x.clone());
        x
    }
    for r in &rolls {
        root.entry(r.member.clone())
            .or_insert_with(|| r.member.clone());
        for p in &r.partners {
            root.entry(p.clone()).or_insert_with(|| p.clone());
            let (a, b) = (find(&mut root, &r.member), find(&mut root, p));
            if a != b {
                let (lo, hi) = if a < b { (a, b) } else { (b, a) };
                root.insert(hi, lo);
            }
        }
    }
    let mut chords: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let members: Vec<String> = root.keys().cloned().collect();
    for m in members {
        let r = find(&mut root, &m);
        chords.entry(r).or_default().push(m);
    }
    let mut out = Vec::new();
    for ms in chords.into_values() {
        let flagged: Vec<&crate::LegRoll> = rolls
            .iter()
            .filter(|r| r.flagged() && ms.contains(&r.member))
            .collect();
        if flagged.is_empty()
            || !flagged
                .iter()
                .any(|r| r.estimates.iter().any(|e| targets.contains(&e.joint)))
            || !ms.iter().all(|m| may(m))
        {
            continue;
        }
        let mut turns: Vec<f64> = flagged.iter().filter_map(|r| r.turn).collect();
        turns.sort_by(f64::total_cmp);
        let (lo, hi) = (turns[0], turns[turns.len() - 1]);
        if lo.signum() != hi.signum() || hi - lo > crate::leg_roll::SPREAD_DEG {
            continue;
        }
        let mid = if turns.len() % 2 == 1 {
            turns[turns.len() / 2]
        } else {
            0.5 * (turns[turns.len() / 2 - 1] + turns[turns.len() / 2])
        };
        let turn = (mid * 100.0).round() / 100.0;
        out.push(ChordGroup {
            words: format!(
                "chord {} turned by {turn:+.2}° (L12, a chord turned against its joints: {})",
                ms.join(", "),
                flagged
                    .iter()
                    .map(|r| r.words())
                    .collect::<Vec<_>>()
                    .join("; ")
                    .replace("turn leg piece", "piece")
            ),
            members: ms,
            turn,
        });
    }
    out
}

/// Either leg of `a` may be its leg bolted (J3, [`crate::whole_roll`]): a
/// single equal-leg angle whose leg bolted is not given as both (an
/// equal-leg angle's long and short leg are alike). An unequal angle's leg
/// bolted is its design data: never changed.
fn either_leg(a: &crate::Arrival) -> bool {
    a.other_onto.is_none()
        && a.leg.value != Some(ohl_structures::connections::BoltedLeg::Both)
        && a.profile
            .as_ref()
            .and_then(|p| p.angle)
            .is_some_and(|g| (g.b - g.h).abs() <= 1e-9)
}

/// The joints asked in detail, their model and each one's plan.
type Planned = (
    JointDetails,
    Option<ohl_structures_fe::model::Model>,
    Vec<(String, JointPlan)>,
);

/// The plan of each joint of `details` among `targets` (in its order), the
/// code's edge distances from `model`; `order`: the tower's joints bottom
/// up, then out (L1 a and c); `guard`: the joints planned as today, and
/// those whose plan L1 changed (noted).
fn plans_of(
    details: &JointDetails,
    model: Option<&ohl_structures_fe::model::Model>,
    targets: &BTreeSet<String>,
    settings: &LayoutSettings,
    joins: &BTreeMap<String, Vec<Join>>,
    order: &JointOrder,
    guard: &std::cell::RefCell<Guard>,
) -> Vec<(String, JointPlan)> {
    let edge = edge_rule(model);
    // L1 (a): planned bottom up, then out (the joint a kind is planned at
    // the first of it in that order); listed in the file's order.
    let mut joints: Vec<&crate::JointDetail> = details
        .joints
        .iter()
        .filter(|j| targets.contains(&j.id))
        .collect();
    if settings.bottom_up {
        joints.sort_by_key(|j| order.rank_of(&j.id));
    }
    // Laid out once per kind (J4, [`crate::joint_kinds`]): a joint like
    // one planned before it (the tower's symmetry, a repeated panel; as
    // the planner reads them) takes that plan, mapped — checked after
    // writing in its own place like every joint, planned on its own where
    // it fails there.
    let kinds = if settings.once_per_kind {
        crate::joint_kinds::kinds_in(joints.iter().copied(), true)
    } else {
        BTreeMap::new()
    };
    let mut planned: BTreeMap<String, JointPlan> = BTreeMap::new();
    let mut out: BTreeMap<String, JointPlan> = BTreeMap::new();
    let mut g = guard.borrow_mut();
    let in_plan = settings.more_plates;
    for j in &joints {
        let plain = g.plain.contains(&j.id);
        let no_l11 = g.no_l11.contains(&j.id);
        let own_with = |plain: bool, no_l11: bool| {
            crate::joint_layout::plan_joint_ctx(
                j,
                settings,
                &edge,
                &BTreeMap::new(),
                &BTreeMap::new(),
                crate::joint_layout::NODE_LIMIT,
                &ctx_of(
                    settings,
                    order,
                    &j.id,
                    &BTreeSet::new(),
                    &BTreeSet::new(),
                    plain,
                    no_l11,
                ),
            )
        };
        let own = |plain: bool| own_with(plain, no_l11);
        let mut p = match kinds
            .get(&j.id)
            .filter(|_| !plain && !no_l11)
            .and_then(|l| planned.get(&l.of).map(|p| (l, p)))
        {
            Some((l, rep)) => {
                let mut p = crate::joint_kinds::map_plan(rep, l);
                if let Some(of) = joints.iter().find(|x| x.id == l.of) {
                    crate::joint_kinds::map_hold_legs(&mut p, l, of, j);
                }
                p
            }
            None => {
                let p = own(plain);
                if !plain && !no_l11 {
                    planned.insert(j.id.clone(), p.clone());
                }
                p
            }
        };
        // L11: the plan without its steps, for the guard (only where one of
        // them said something here).
        if !plain
            && !no_l11
            && (settings.plate_shift || settings.both_faces)
            && p.notes.iter().any(|n| {
                n.contains("within the longest pull-back")
                    || n.contains("both faces of one plate")
                    || n.contains("one part, their flanges in one plane")
            })
        {
            let pre = own_with(false, true);
            if pre.ends != p.ends || pre.plates != p.plates {
                g.l11.insert(j.id.clone(), pre.ends.len());
            }
        }
        // L1 (b, c in the plan): today's plan of the joint, for the guard.
        if in_plan && !plain {
            let today = own(true);
            if today.ends != p.ends || today.plates != p.plates {
                g.changed.insert(j.id.clone(), today.ends.len());
            }
        }
        p.joins = joins.get(&j.id).cloned().unwrap_or_default();
        out.insert(j.id.clone(), p);
    }
    details
        .joints
        .iter()
        .filter_map(|j| out.remove(&j.id).map(|p| (j.id.clone(), p)))
        .collect()
}

/// What the run tells joint `j`'s plan: the members put onto a plate by the
/// clash rounds (L1 b), those bolted directly instead of on a plate (L2 c),
/// of the tower's body (L1 c), planned as today (the guard).
fn ctx_of(
    settings: &LayoutSettings,
    order: &JointOrder,
    j: &str,
    plated: &BTreeSet<String>,
    direct: &BTreeSet<String>,
    plain: bool,
    no_l11: bool,
) -> crate::joint_layout::PlanCtx {
    crate::joint_layout::PlanCtx {
        plated: plated.clone(),
        direct: direct.clone(),
        body: settings.direct_first && order.zone_of(j) == crate::Zone::Body,
        plain,
        no_l11,
    }
}

/// The run's guard on L1 (owner 2026-11-19, kept only where it helps):
/// the joints whose plan L1's choices changed (b and c in the plan, d's
/// members to bolt onto), with the ends today's plan of each lays out; a
/// joint laid out with fewer ends after the clash rounds is planned again
/// as today ([`Guard::plain`]).
#[derive(Debug, Default)]
struct Guard {
    /// Joint → the ends today's plan lays out there.
    changed: BTreeMap<String, usize>,
    /// The joints planned as today (L1 b and c off in the plan, d's choice
    /// taken back).
    plain: BTreeSet<String>,
    /// L11: the joints whose plan L11's steps changed, with the ends their
    /// plan without them lays out; a joint laid out with fewer ends after
    /// the clash rounds is planned again without them ([`Guard::no_l11`]).
    l11: BTreeMap<String, usize>,
    /// L11: the joints planned without L11's steps.
    no_l11: BTreeSet<String>,
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
            let id = write_splice(
                doc,
                conns,
                j,
                [&x.member, &x.onto],
                &x.covers,
                x.thickness,
                taken,
            );
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

/// A `[[splice]]` at joint `j` joining `members` written into `doc`: its
/// covers outer and inner on each of `legs`, `thickness` thick, both ends
/// `onto` it; its id the first free `S1`, `S2`… of `taken` (taken then).
/// Step 1c's, and the leg splices confirmed (O44 J7,
/// [`crate::leg_splices::confirm`]).
pub(crate) fn write_splice(
    doc: &mut DocumentMut,
    conns: &mut connection_edits::Connections,
    j: &str,
    members: [&str; 2],
    legs: &[ohl_structures::connections::CoverLeg],
    thickness: f64,
    taken: &mut std::collections::HashSet<String>,
) -> String {
    let id = (1u64..)
        .map(|n| format!("S{n}"))
        .find(|x| !taken.contains(x))
        .expect("a free id");
    taken.insert(id.clone());
    let mut sp = Table::new();
    sp.insert("id", toml_edit::value(id.as_str()));
    sp.insert("joint", toml_edit::value(j));
    let mut list = Array::new();
    for m in members {
        list.push(m);
    }
    sp.insert("members", toml_edit::value(list));
    let mut covers = ArrayOfTables::new();
    for leg in legs {
        for face in ohl_structures::connections::CoverFace::ALL {
            let mut c = Table::new();
            c.insert("face", toml_edit::value(face.as_str()));
            c.insert("leg", toml_edit::value(leg.as_str()));
            c.insert("thickness", toml_edit::value(crate::mm_text(thickness)));
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
    for m in members {
        let _ = conns.set(doc, m, j, "onto", Some(&FieldValue::Word(id.clone())));
    }
    id
}

/// Provisional bolts `b` written at member `m`'s end at joint `j` (the
/// bolts' count and bolt or diameter, its gauge lines), as the layout
/// writes an end's that has none.
pub(crate) fn write_bolts(
    doc: &mut DocumentMut,
    conns: &mut connection_edits::Connections,
    m: &str,
    j: &str,
    b: &BoltArea,
) {
    let _ = conns.set(
        doc,
        m,
        j,
        "bolts",
        Some(&FieldValue::Bolts {
            count: b.count,
            bolt: b.bolt.clone(),
            diameter: b.bolt.is_none().then_some(b.diameter),
            hole: None,
            grade: None,
        }),
    );
    let _ = conns.set(doc, m, j, "gauge_lines", Some(&FieldValue::Count(b.lines)));
}

/// The ids in use in `doc` and `t`: its plates', splices' and members'.
pub(crate) fn ids_of(doc: &DocumentMut, t: &StructureType) -> std::collections::HashSet<String> {
    let mut out = ids_in(doc);
    out.extend(t.members.iter().map(|m| m.id.clone()));
    out
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
type Replan<'a> = dyn Fn(
        &str,
        &BTreeMap<String, String>,
        &BTreeMap<String, (f64, String)>,
        &BTreeSet<String>,
        &BTreeSet<String>,
    ) -> Option<JointPlan>
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

/// How many other members an end left for a hand detail is tried onto
/// (L1 d, [`LayoutSettings::onto_search`]): the program's bound on the
/// extra work, not a standard's.
pub const ONTO_TRIES: usize = 2;

/// How many passes of a run take out the names of the members it named
/// but does not lay out (L8, [`LayoutSettings::as_drawn`]): the program's
/// bound on the extra work, not a standard's.
pub const AS_DRAWN_PASSES: usize = 2;

/// What `m`'s end at joint `j` bolts onto as `doc` gives it (none: not
/// given).
fn onto_in(doc: &DocumentMut, m: &str, j: &str) -> Option<String> {
    field_in(doc, m, j, "onto")
}

/// `m`'s end at joint `j`: its field `field` as `doc` gives it (a word).
fn field_in(doc: &DocumentMut, m: &str, j: &str, field: &str) -> Option<String> {
    doc.get("connection")
        .and_then(Item::as_array_of_tables)
        .and_then(|a| {
            a.iter().find(|t| {
                t.get("member").and_then(Item::as_str) == Some(m)
                    && t.get("joint").and_then(Item::as_str) == Some(j)
            })
        })
        .and_then(|t| t.get(field))
        .and_then(Item::as_str)
        .map(str::to_owned)
}

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
    // L1 (b): the members at each joint put onto a plate for a clash (or a
    // misfit) instead of being pulled back or left.
    let mut plated: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    // L2 (c): the members at each joint bolted directly for a clash instead
    // of on their plate.
    let mut directed: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
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
        plated.insert(o.joint.clone(), c.plated.clone());
        directed.insert(o.joint.clone(), c.directed.clone());
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
            o.plates = write_plan(
                &mut doc,
                r.t,
                r.record,
                &mut rec,
                &o.plan,
                ask,
                r.hand_plates,
                &mut w,
            );
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
                    .filter(|(_, (_, p))| {
                        p.heel.is_some() || p.reach.is_some() || !p.cuts.is_empty()
                    })
                    .map(|(k, (id, p))| {
                        (
                            (o.joint.clone(), k),
                            id.clone(),
                            p.heel,
                            p.reach.clone(),
                            p.cuts.clone(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let new: BTreeSet<String> = heels
            .iter()
            .filter(|(key, _, _, _, _)| !outlines.contains_key(key))
            .map(|(key, _, _, _, _)| key.0.clone())
            .collect();
        if !new.is_empty() {
            // The record first: the drawing reads its marks.
            rec.write(&mut doc);
            let mid = (r.at)(&doc, Some(&new))?;
            for (key, id, heel, reach, cuts) in &heels {
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
                let mut cut = match heel {
                    Some((heel, across)) => clip3(&outline, |q| {
                        (0..3).map(|k| (q[k] - heel[k]) * across[k]).sum::<f64>() - half
                    }),
                    None => outline,
                };
                // (L2's follow-up a) A short plate cut straight short of
                // what it would still run into.
                for (at, n) in cuts {
                    cut = clip3(&cut, |q| 1e-9 - dot3(sub3(q, *at), *n));
                }
                outlines.insert(key.clone(), cut);
            }
        }
        for (key, id, _, _, _) in &heels {
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
                    // (L9, 2026-11-21: a flat plate with plates in their
                    // members' own planes welded to it — nothing bolted on
                    // it, perhaps — is those members' too: before, its
                    // clashes were nobody's and stayed written.)
                    crate::ClashPart::Plate(id) => o
                        .plates
                        .iter()
                        .position(|p| p == id)
                        .map(|k| {
                            let mut m = o.plan.plates[k].members.clone();
                            for q in &o.plan.plates {
                                if q.own.as_ref().is_some_and(|w| w.welded_to == k) {
                                    m.extend(q.members.iter().cloned());
                                }
                            }
                            m
                        })
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
            // J9 and D16 (2026-11-19): an end laid directly onto a member
            // as that member is written (laid out here, or left as it was:
            // then as the file has it, not as planned) — each bolt's hole
            // within that member's flange and on its steel where it ends at
            // the joint (the joint checks' own rule). Else left as it was
            // too, said, and the joint planned again.
            for e in &o.plan.ends {
                let crate::joint_layout::Onto::Member(h) = &e.onto else {
                    continue;
                };
                if out.contains_key(&e.member) {
                    continue;
                }
                let (Some(a), Some(host)) = (
                    j.arrivals.iter().find(|x| x.member == e.member),
                    j.arrivals.iter().find(|x| &x.member == h),
                ) else {
                    continue;
                };
                if let Some(why) = misfit_on(j, a, host) {
                    let words = format!("{}: {why}", e.member);
                    out.insert(
                        e.member.clone(),
                        format!("laid out, {why} (left as it was)"),
                    );
                    bad.push(words);
                }
            }
            // (2026-11-20, found measuring L1) The holes of the ends laid
            // directly onto a member (or the run it is one part of) not
            // overlapping each other's or those of the other ends onto it
            // as written (an end on its flange's outer face and one on its
            // inner face bolted through the same place) — the joint
            // checks' own rule; else the later end (class, then id) left
            // as it was too, said.
            {
                let run_of = |m: &str| -> String {
                    j.arrivals
                        .iter()
                        .find(|x| x.member == m)
                        .and_then(|x| x.continuous.clone())
                        .map_or_else(|| m.to_owned(), |c| c.min(m.to_owned()))
                };
                let laid: BTreeSet<&str> = o
                    .plan
                    .ends
                    .iter()
                    .filter(|e| matches!(e.onto, Onto::Member(_)))
                    .map(|e| e.member.as_str())
                    .collect();
                let rank = |m: &str| ((r.class_rank)(m), m.to_owned());
                let onto: Vec<&crate::Arrival> = j
                    .arrivals
                    .iter()
                    .filter(|x| {
                        x.onto
                            .value
                            .as_ref()
                            .is_some_and(|h| j.arrivals.iter().any(|y| &y.member == h))
                    })
                    .collect();
                let hole = |x: &crate::Arrival| {
                    x.bolts
                        .value
                        .as_ref()
                        .map_or(0.0, |b| b.hole.unwrap_or(b.diameter))
                };
                for e in onto.iter().filter(|x| laid.contains(x.member.as_str())) {
                    if out.contains_key(&e.member) {
                        continue;
                    }
                    let he = run_of(e.onto.value.as_deref().unwrap_or_default());
                    let clash = onto.iter().find(|x| {
                        x.member != e.member
                            && run_of(x.onto.value.as_deref().unwrap_or_default()) == he
                            && !(laid.contains(x.member.as_str())
                                && rank(&x.member) > rank(&e.member))
                            && e.bolt_points.iter().any(|p| {
                                x.bolt_points
                                    .iter()
                                    .any(|q| holes_overlap(*p, hole(e), *q, hole(x)))
                            })
                    });
                    if let Some(x) = clash {
                        let why = format!("its holes in {he} would overlap those of {}", x.member);
                        out.insert(
                            e.member.clone(),
                            format!("laid out, {why} (left as it was)"),
                        );
                        bad.push(format!("{}: {why}", e.member));
                    }
                }
            }
            // (2026-11-20, found measuring L1) A plate's own bolts onto the
            // member it lies on (a bent plate's flat, J2b) each within its
            // outline and on that member's steel as written — the joint
            // checks' own rules; else the
            // members on the plate (and on what is bent or welded to it)
            // left as they were too, said, and the joint planned again.
            for (k, id) in o.plates.iter().enumerate() {
                let Some(pd) = j.plates.iter().find(|p| &p.id == id) else {
                    continue;
                };
                let Some(h) = pd.leg_bolts.as_deref() else {
                    continue;
                };
                // (And on that member's steel where it ends at the joint,
                // past its cut: as for an end bolted onto it.)
                let start = j
                    .arrivals
                    .iter()
                    .find(|x| x.member == h.member)
                    .and_then(|host| {
                        let ends = host.continuous.is_none()
                            && !j
                                .arrivals
                                .iter()
                                .any(|b| b.continuous.as_deref() == Some(host.member.as_str()));
                        (ends && host.cut.known())
                            .then(|| (host.cut.value.unwrap_or(0.0), host.direction))
                    });
                let outside = pd.outline.len() >= 3
                    && h.bolt_points.iter().any(|at| {
                        crate::plate_checks::ray_to_outline(&pd.outline, *at, h.direction).is_none()
                    });
                let beyond = start.is_some_and(|(c, d)| {
                    h.bolt_points
                        .iter()
                        .any(|at| dot3(sub3(*at, j.position), d) < c - 0.001)
                });
                // (J2b fix, 2026-11-20) Nor through a hole of that member's
                // own end or of an end bolted onto it (or onto the run it is
                // one part of), as written — the joint checks' own rule.
                let hole_of = |x: &crate::Arrival| {
                    x.bolts
                        .value
                        .as_ref()
                        .map_or(0.0, |b| b.hole.unwrap_or(b.diameter))
                };
                let run = |m: &str| {
                    m == h.member
                        || j.arrivals.iter().any(|x| {
                            (x.member == h.member && x.continuous.as_deref() == Some(m))
                                || (x.member == m && x.continuous.as_deref() == Some(&h.member))
                        })
                };
                let overlap = j
                    .arrivals
                    .iter()
                    .filter(|x| x.member == h.member || x.onto.value.as_deref().is_some_and(run))
                    .find(|x| {
                        x.bolt_points.iter().any(|q| {
                            h.bolt_points
                                .iter()
                                .any(|p| holes_overlap(*p, hole_of(h), *q, hole_of(x)))
                        })
                    })
                    .map(|x| x.member.clone());
                if !outside && !beyond && overlap.is_none() {
                    continue;
                }
                let on = o.plan.plates.get(k).map_or("?", |p| p.on.as_str());
                let why = if outside {
                    format!(
                        "a plate on {on}: its own bolts onto {} would lie outside its outline",
                        h.member
                    )
                } else if beyond {
                    format!(
                        "a plate on {on}: its own bolts onto {} would lie beyond the end of its \
                         steel (not one part through the joint)",
                        h.member
                    )
                } else {
                    format!(
                        "a plate on {on}: its own bolts onto {} would overlap the holes of {}",
                        h.member,
                        overlap.as_deref().unwrap_or("?")
                    )
                };
                for (i, p) in o.plan.plates.iter().enumerate() {
                    if i == k || p.own.as_ref().is_some_and(|w| w.welded_to == k) {
                        for m in &p.members {
                            out.entry(m.clone())
                                .or_insert_with(|| format!("laid out, {why} (left as it was)"));
                        }
                    }
                }
                bad.push(why);
            }
            // (L2, 2026-11-20, found measuring) The bolts of each member on
            // a plate the layout made within the plate's outline as written
            // (a plate in its member's own plane, a bent plate's flat: their
            // outlines laid out in the plan) — the joint checks' own rule;
            // else that member left as it was too, said.
            for id in &o.plates {
                let Some(pd) = j.plates.iter().find(|p| &p.id == id) else {
                    continue;
                };
                if pd.outline.len() < 3 {
                    continue;
                }
                for a in crate::joints::bolted_onto(&j.arrivals, id) {
                    if out.contains_key(&a.member) {
                        continue;
                    }
                    let outside = a.bolt_points.iter().any(|at| {
                        crate::plate_checks::ray_to_outline(&pd.outline, *at, a.direction).is_none()
                    });
                    if outside {
                        let on = o
                            .plates
                            .iter()
                            .position(|p| p == id)
                            .map_or("?", |k| o.plan.plates[k].on.as_str());
                        let why = format!(
                            "its bolts on a plate on {on} would lie outside the plate's outline"
                        );
                        out.insert(
                            a.member.clone(),
                            format!("laid out, {why} (left as it was)"),
                        );
                        bad.push(format!("{}: {why}", a.member));
                    }
                }
                // (L2's follow-up a) A plate stopping short of the work
                // point, cut straight: every bolt on it its end and edge
                // distances from the outline as written (the joint details'
                // words); else the member left as it was, said.
                let short = o
                    .plates
                    .iter()
                    .position(|p| p == id)
                    .is_some_and(|k| o.plan.plates[k].short);
                if short {
                    for note in &pd.notes {
                        let Some(m) = note
                            .split_once(" of ")
                            .and_then(|(_, r)| r.split_once(' '))
                            .map(|(m, _)| m.to_owned())
                            .filter(|_| note.contains("from its edge"))
                        else {
                            continue;
                        };
                        if out.contains_key(&m) {
                            continue;
                        }
                        let why = format!(
                            "its bolts on a plate on {} would lie nearer the plate's edge than \
                             their distances",
                            o.plates
                                .iter()
                                .position(|p| p == id)
                                .map_or("?", |k| o.plan.plates[k].on.as_str())
                        );
                        out.insert(m.clone(), format!("laid out, {why} (left as it was)"));
                        bad.push(format!("{m}: {why}"));
                    }
                }
            }
            // (L2, 2026-11-20, found measuring) A member not laid out here
            // bolting onto one that is: its bolts, where they are, still
            // within that member's flange and on its steel as written (the
            // joint checks' own rule, as for an end laid onto a member left
            // as it was); else the member laid out left as it was, said. (A
            // member left for a hand detail is not asked: its hand detail
            // places its bolts, J2.)
            for x in &j.arrivals {
                if o.plan.ends.iter().any(|e| e.member == x.member)
                    || o.plan.hand_members.contains(&x.member)
                {
                    continue;
                }
                let Some(h) = x.onto.value.as_deref() else {
                    continue;
                };
                if out.contains_key(h) || !o.plan.ends.iter().any(|e| e.member == h) {
                    continue;
                }
                let Some(host) = j.arrivals.iter().find(|a| a.member == h) else {
                    continue;
                };
                if let Some(w) = misfit_on(j, x, host) {
                    let why = format!("{} (as it is) would then not fit onto it: {w}", x.member);
                    out.insert(h.to_owned(), format!("laid out, {why} (left as it was)"));
                    bad.push(format!("{h}: {why}"));
                }
            }
            if bad.is_empty() {
                continue;
            }
            for m in out.keys() {
                pull.remove(m);
            }
            // L1 (b), owner 2026-11-19 ("maybe increase slightly the
            // possibility of having plates"): an end bolted directly that
            // would be left as it was — or pulled back past its longest
            // pull-back — tried on a plate first (once), while a round is
            // left to check it; kept where the plan then lays it out.
            let tried: BTreeSet<String> = plated.get(&o.joint).cloned().unwrap_or_default();
            let direct_now: BTreeSet<String> = o
                .plan
                .ends
                .iter()
                .filter(|e| matches!(e.onto, Onto::Member(_)))
                .map(|e| e.member.clone())
                .collect();
            let may_plate = |m: &str| {
                ask.settings.more_plates
                    && round + 2 < CLASH_ROUNDS
                    && direct_now.contains(m)
                    && !tried.contains(m)
            };
            let mut trying: BTreeMap<String, String> = BTreeMap::new();
            out.retain(|m, why| {
                if may_plate(m) {
                    trying.insert(m.clone(), why.clone());
                    false
                } else {
                    true
                }
            });
            // L2 (c), 2026-11-20: an end on a plate that would be left as it
            // was bolted directly instead where its bolts land (once), while
            // a round is left to check it; kept where the plan lays it out.
            let dtried: BTreeSet<String> = directed.get(&o.joint).cloned().unwrap_or_default();
            let on_plate: BTreeSet<String> = o
                .plan
                .ends
                .iter()
                .filter(|e| matches!(e.onto, Onto::Plate(_)))
                .map(|e| e.member.clone())
                .collect();
            let mut dtrying: BTreeMap<String, String> = BTreeMap::new();
            out.retain(|m, why| {
                if ask.settings.clash_tries
                    && round + 2 < CLASH_ROUNDS
                    && on_plate.contains(m)
                    && !dtried.contains(m)
                {
                    dtrying.insert(m.clone(), why.clone());
                    false
                } else {
                    true
                }
            });
            let mut dl = dtried.clone();
            dl.extend(dtrying.keys().cloned());
            let fl = floors.entry(o.joint.clone()).or_default();
            for (m, f) in &pull {
                *pulls.entry((o.joint.clone(), m.clone())).or_default() += 1;
                fl.insert(m.clone(), f.clone());
            }
            let ex = except.entry(o.joint.clone()).or_default();
            ex.extend(out);
            let mut pl = tried.clone();
            pl.extend(trying.keys().cloned());
            let plan_now = |ex: &BTreeMap<String, String>,
                            fl: &BTreeMap<String, (f64, String)>,
                            pl: &BTreeSet<String>,
                            dl: &BTreeSet<String>| {
                (round + 1 < CLASH_ROUNDS)
                    .then(|| (r.replan)(&o.joint, ex, fl, pl, dl))
                    .flatten()
            };
            let mut again = plan_now(ex, fl, &pl, &dl);
            if ask.settings.more_plates {
                let laid = |p: &Option<JointPlan>, m: &str| {
                    p.as_ref()
                        .is_some_and(|p| p.ends.iter().any(|e| e.member == m))
                };
                // Pulled back past its longest pull-back (left by the plan):
                // on a plate instead.
                let lost: Vec<String> = pull
                    .keys()
                    .filter(|m| may_plate(m) && !laid(&again, m))
                    .cloned()
                    .collect();
                if !lost.is_empty() {
                    for m in &lost {
                        if let Some((_, why)) = fl.remove(m) {
                            trying.insert(m.clone(), why);
                        }
                        pl.insert(m.clone());
                    }
                    again = plan_now(ex, fl, &pl, &dl);
                }
                // Not laid out on a plate either: left as it was.
                let failed: Vec<String> = trying
                    .keys()
                    .filter(|m| !laid(&again, m))
                    .cloned()
                    .collect();
                if !failed.is_empty() {
                    for m in &failed {
                        pl.remove(m);
                        ex.insert(m.clone(), trying[m].clone());
                    }
                    again = plan_now(ex, fl, &pl, &dl);
                }
            }
            // (L2 c) Not laid out bolted directly either: left as it was.
            if !dtrying.is_empty() {
                let failed: Vec<String> = dtrying
                    .keys()
                    .filter(|m| {
                        !again
                            .as_ref()
                            .is_some_and(|p| p.ends.iter().any(|e| &e.member == *m))
                    })
                    .cloned()
                    .collect();
                if !failed.is_empty() {
                    for m in &failed {
                        dl.remove(m);
                        ex.insert(m.clone(), dtrying[m].clone());
                    }
                    again = plan_now(ex, fl, &pl, &dl);
                }
            }
            plated.insert(o.joint.clone(), pl);
            directed.insert(o.joint.clone(), dl);
            let again = again.filter(JointPlan::laid_out);
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
                plated: plated.get(&o.joint).cloned().unwrap_or_default(),
                directed: directed.get(&o.joint).cloned().unwrap_or_default(),
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

/// Two holes (where, their diameters, m) overlapping: nearer than their
/// radii together, but not one hole (within a quarter of the smaller
/// diameter, at least 2 mm) — the joint checks' own rule.
pub(crate) fn holes_overlap(p: [f64; 3], a: f64, q: [f64; 3], b: f64) -> bool {
    let d = sub3(p, q);
    let dist = dot3(d, d).sqrt();
    dist > (0.25 * a.min(b)).max(0.002) && dist < (a + b) / 2.0 - 1e-4
}

/// Why end `a` bolted directly onto `host` at joint `j` (both as written)
/// does not fit, by the joint checks' rule: a hole not within the host's
/// flange, or a bolt beyond its steel where it ends at the joint (not one
/// part through it) and where its steel stops is known; none: it fits.
fn misfit_on(j: &crate::JointDetail, a: &crate::Arrival, host: &crate::Arrival) -> Option<String> {
    let f = crate::joints::host_flange(a, host)?;
    let hole = a
        .bolts
        .value
        .as_ref()
        .map_or(0.0, |b| b.hole.unwrap_or(b.diameter));
    let ends = host.continuous.is_none()
        && !j
            .arrivals
            .iter()
            .any(|b| b.continuous.as_deref() == Some(host.member.as_str()));
    let start = (ends && host.cut.known()).then(|| host.cut.value.unwrap_or(0.0));
    for q in &a.bolt_points {
        let rel = sub3(sub3(*q, j.position), f.heel);
        let g = dot3(rel, f.across);
        if g - hole / 2.0 < -0.001 || g + hole / 2.0 > f.width + 0.001 {
            return Some(format!(
                "its bolt {:.0} mm from {}'s heel as that is written would not lie within its \
                 flange (0 to {:.0} mm)",
                g * 1000.0,
                host.member,
                f.width * 1000.0
            ));
        }
        if let Some(c) = start {
            let s = dot3(sub3(*q, j.position), host.direction);
            if s < c - 0.001 {
                return Some(format!(
                    "its bolt {:.0} mm from the work point would lie beyond the end of {}'s \
                     steel (from {:.0} mm; not one part through the joint)",
                    s * 1000.0,
                    host.member,
                    c * 1000.0
                ));
            }
        }
    }
    None
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
    /// The members its clashes put onto a plate (L1 b).
    plated: BTreeSet<String>,
    /// The members its clashes bolted directly instead of on a plate (L2 c).
    directed: BTreeSet<String>,
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
type OutlinePass = (
    (String, usize),
    String,
    Option<HeelLine>,
    Option<Reach>,
    Vec<([f64; 3], [f64; 3])>,
);

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
    /// Its bend's inside radius given by hand since (a bent plate's flat,
    /// J2b), m.
    bend_radius: Option<f64>,
    /// A bent plate's flat in its members' plane (J2b), not welded.
    bent: bool,
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
            // (A bent plate's flat on the flange, J2b: by those of the flat
            // bent from it.)
            if members.is_empty() {
                for q in t.plates.iter().filter(|q| &q.joint == j && &q.id != id) {
                    if q.welded_ids().contains(&id.as_str())
                        || p.welded_ids().contains(&q.id.as_str())
                        || q.bend.as_ref().is_some_and(|b| &b.to == id)
                    {
                        members.extend(onto(&q.id));
                    }
                }
            }
            let bent = auto.contains(BENT);
            Some(OldPlate {
                id: id.clone(),
                joint: j.clone(),
                members,
                own: auto.contains(OWN_PLANE),
                thickness: (!auto.contains("thickness")).then(|| p.thickness.si()),
                material: p.material.clone(),
                weld: (auto.contains(OWN_PLANE) && !bent && !auto.contains("weld"))
                    .then(|| p.welds.first().cloned())
                    .flatten(),
                bend_radius: p
                    .bend
                    .as_ref()
                    .filter(|_| !bent && auto.contains(OWN_PLANE))
                    .map(|b| b.inner_radius.si()),
                bent: bent || p.bend.is_some(),
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
        // (A plate's own bolts the layout gave: provisional as (plate,
        // joint), J2b.)
        rec.provisional.remove(&(id.clone(), j.to_owned()));
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
        // Onto a splice not the layout's (given by hand, or a leg splice the
        // user confirmed, O44 J7).
        let confirmed = c.onto.as_deref().is_some_and(|o| {
            !made.iter().any(|m| m == o) && t.splices.iter().any(|s| s.id == o && s.joint == j)
        });
        if replace_hand {
            go.extend(["cut", "shift"]);
            // `onto` given by hand: a member or a plate of the joint —
            // not that of pieces given one part through the joint (J9: what
            // the steel is, the user's, not a layout value), nor a splice's
            // (J7, the same).
            if !(crate::in_line::one_part(c.kind) && !auto.contains("kind")) && !confirmed {
                go.insert("onto");
            }
        }
        for f in go {
            if f == "gauge" {
                continue;
            }
            // (An `onto` given by hand the layout kept: back to it, unless
            // values given by hand are replaced.)
            if let Some(h) = f.strip_prefix(ONTO_HAND) {
                if !replace_hand {
                    let _ = w.conns.set(
                        doc,
                        &c.member,
                        j,
                        "onto",
                        Some(&FieldValue::Word(h.to_owned())),
                    );
                }
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
        // (The provisional bolts of an end onto a splice the user confirmed,
        // O44 J7: written with it, not the layout's values — kept, and
        // still provisional.)
        if !(confirmed && !auto.contains("bolts")) {
            rec.provisional.remove(&key);
        }
    }
    // A connection left with nothing in it goes now (as the edit crate's
    // own edits do: where the ones written again then stand in the file
    // does not hang on a run before; a run again identical).
    let empty = doc
        .get("connection")
        .and_then(Item::as_array_of_tables)
        .is_some_and(|a| {
            a.iter().any(|t| {
                // (L9, speed only) More than two fields: not empty, without
                // looking its joint up.
                t.len() <= 2
                    && t.get("joint").and_then(Item::as_str) == Some(j)
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
    before: &LayoutRecord,
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
        let bent = pl.own.as_ref().is_some_and(|o| o.bend.is_some());
        let before = old.iter().find(|o| {
            o.joint == p.joint
                && o.own == pl.own.is_some()
                && (!o.own || o.bent == bent)
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
        // L2 (2026-11-20): the member whose flange it lies on (a plate in
        // its member's own plane lies on none: it stands on the flat
        // plate), so that it is drawn, seated and checked on that member's
        // flange as planned — not on the joint's leg when that is another
        // member.
        if pl.own.is_none() && ask.settings.plate_on {
            x.insert("on", toml_edit::value(pl.on.as_str()));
            auto.insert("on".into());
            ohl_structure_edit::edits::raise_version(
                doc,
                i64::from(ohl_structures::connections::PLATE_ON_VERSION),
            );
        }
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
        // A bent plate's flat in its members' plane (J2b): bent from the
        // flat on the flange — its angle, inside radius (one given by hand
        // before kept) and bend line; no weld.
        if let Some((o, b)) = pl
            .own
            .as_ref()
            .and_then(|o| o.bend.as_ref().map(|b| (o, b)))
        {
            let mut bend = Table::new();
            bend.insert("to", toml_edit::value(ids[o.welded_to].as_str()));
            bend.insert(
                "angle",
                toml_edit::value(format!("{} deg", (b.angle * 100.0).round() / 100.0)),
            );
            let radius = before.and_then(|x| x.bend_radius);
            bend.insert(
                "inner_radius",
                toml_edit::value(crate::mm_text(radius.unwrap_or(b.inner_radius))),
            );
            let mut line = Array::new();
            for q in &b.line {
                let mut a = Array::new();
                for v in q {
                    a.push(format!("{:.4} m", v));
                }
                line.push(Value::Array(a));
            }
            bend.insert("line", Item::Value(Value::Array(line)));
            x.insert("bend", Item::Table(bend));
            auto.extend(["outline".into(), OWN_PLANE.into()]);
            if radius.is_none() {
                auto.insert(BENT.into());
            }
            ohl_structure_edit::edits::raise_version(
                doc,
                i64::from(ohl_structures::connections::PLATE_BEND_VERSION),
            );
        } else if let Some(o) = &pl.own {
            // A plate in its member's own plane (step 4): its outline, and
            // its weld to the flat plate (both sides, the throat each side).
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
        // A bent plate's flat on the flange (J2b): its own bolts onto the
        // member it lies on (`[plate.leg_bolts]`, as
        // `connection_edits::set_plate_leg_bolts` writes them).
        if let Some(h) = &pl.hold {
            x.insert("leg_bolts", Item::Table(hold_table(h)));
            ohl_structure_edit::edits::raise_version(
                doc,
                i64::from(ohl_structures::connections::PLATE_LEG_BOLTS_VERSION),
            );
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
        // A bent plate's flat on the flange (J2b): its outline as laid out,
        // and its own bolts onto that member (provisional).
        if let Some(q) = &pl.outline
            && connection_edits::set_plate_outline(doc, &id, q).is_ok()
        {
            auto.insert("outline".into());
        }
        if pl.hold.is_some() {
            auto.insert(HOLD.into());
        }
        // (L2's follow-up a) Its outline stopping short of the work point:
        // drawn so by the joint details (the record's mark), then written.
        if pl.short {
            auto.insert(SHORT.into());
        }
        if pl.hold.as_ref().is_some_and(|h| h.provisional) {
            rec.provisional.insert((id.clone(), p.joint.clone()));
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
        // (L2's follow-up, 2026-11-20) An `onto` given by hand kept: the end
        // laid out onto that member, or onto a plate the layout made on it —
        // the record keeps the hand value, so that a run again starts from it
        // (before, the record took it as the layout's and a run again took
        // it out).
        let hand = (!ask.replace_hand)
            .then(|| hand_onto(t, before, &e.member, &p.joint))
            .flatten()
            .filter(|h| match &e.onto {
                Onto::Member(m) => m == h,
                Onto::Plate(k) => p.plates.get(*k).is_some_and(|q| &q.on == h),
            });
        match hand {
            Some(h) => fields.insert(format!("{ONTO_HAND}{h}")),
            None => fields.insert("onto".into()),
        };
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

/// A plate's own bolts onto the member it lies on as the file writes them
/// (`[plate.leg_bolts]`; J2b's bent plate's flat on the flange).
fn hold_table(h: &crate::joint_layout::HoldPlan) -> Table {
    let mut lb = Table::new();
    lb.insert("member", toml_edit::value(h.member.as_str()));
    if let Some(l) = h.leg {
        lb.insert("leg", toml_edit::value(l));
    }
    let mut bt = toml_edit::InlineTable::new();
    bt.insert("count", Value::from(h.points.len() as i64));
    if let Some(x) = &h.bolt.bolt {
        bt.insert("bolt", Value::from(x.as_str()));
    }
    bt.insert("diameter", connection_edits::mm(h.bolt.diameter));
    lb.insert("bolts", Item::Value(Value::InlineTable(bt)));
    let mut a = Array::new();
    for p in &h.points {
        let mut q = Array::new();
        q.push(connection_edits::mm(p[0]));
        q.push(connection_edits::mm(p[1]));
        a.push(Value::Array(q));
    }
    lb.insert("bolt_points", Item::Value(Value::Array(a)));
    lb
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
                // (An `onto` given by hand the layout kept: the user's
                // again once the edit changes it.)
                x if x.starts_with(ONTO_HAND) => a.map(|c| &c.onto) == b.map(|c| &c.onto),
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
        // A bent plate's bend changed (J2b; its inside radius): the user's
        // now.
        if a.map(|p| &p.bend) != b.map(|p| &p.bend) && fields.remove(BENT) {
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
