// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The edits of the members' connections and the plates (owner
//! 2026-11-02, stage 3 of the joint details): a field of a member end
//! given or cleared, a member end's connection copied to others (the
//! similar joints, a member group or class), cleared, saved as a preset
//! or a preset applied; plates added, changed, removed, welded together
//! (a welded plate, 2026-11-04: [`set_welded_to`], [`make_welded_plates`]).
//! Each a function
//! on the file's text; the file is raised to version 9 when it holds
//! connections or plates.

use ohl_structures::StructureType;
use ohl_structures::connections::{CONNECTIONS_VERSION, MAX_GAUGE_LINES};
use toml_edit::{DocumentMut, InlineTable, Item, Table, Value};

use crate::edits::{array, next_id, raise_version};

/// The fields of a connection a user gives (not its member and joint).
pub const FIELDS: [&str; 15] = [
    "kind",
    "onto",
    "leg",
    "bolts",
    "gauge_lines",
    "gauges",
    "pitch",
    "end_distance",
    "edge_distance",
    "bolt_points",
    "eccentric",
    "restrained",
    "eccentricity",
    "cut",
    "shift",
];

/// The fields a preset or a copy to other members carries: all but
/// `onto` (the member or plate it bolts onto is the joint's own) and
/// `bolt_points` (where the bolts of one member are).
pub const CARRIED: [&str; 11] = [
    "kind",
    "leg",
    "bolts",
    "gauge_lines",
    "gauges",
    "pitch",
    "end_distance",
    "edge_distance",
    "eccentric",
    "restrained",
    "eccentricity",
];

/// A length as written: millimetres (`"30 mm"`).
pub fn mm(v: f64) -> Value {
    let v = (v * 1e4).round() / 10.0;
    if v.fract() == 0.0 {
        Value::from(format!("{v:.0} mm"))
    } else {
        Value::from(format!("{v} mm"))
    }
}

/// A value of a connection's field.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    /// A word (`kind`, `onto`, `leg`).
    Word(String),
    /// A whole number (`gauge_lines`).
    Count(u32),
    /// A length, m (`pitch`, `end_distance`…).
    Length(f64),
    /// Lengths, m (`gauges`).
    Lengths(Vec<f64>),
    /// A length that may be negative, m (`shift`).
    SignedLength(f64),
    /// Yes or no (`eccentric`, `restrained`).
    Flag(bool),
    /// The bolts: count, a library bolt, diameter (m), hole (m), grade.
    Bolts {
        /// How many.
        count: u32,
        /// A bolt of the bolt libraries (its values not repeated here).
        bolt: Option<String>,
        /// Diameter, m (none: the library bolt's).
        diameter: Option<f64>,
        /// Hole, m.
        hole: Option<f64>,
        /// Grade.
        grade: Option<String>,
    },
}

impl FieldValue {
    fn value(&self) -> Value {
        match self {
            FieldValue::Word(w) => Value::from(w.as_str()),
            FieldValue::Count(n) => Value::from(i64::from(*n)),
            FieldValue::Length(v) | FieldValue::SignedLength(v) => mm(*v),
            FieldValue::Lengths(v) => {
                let mut a = toml_edit::Array::new();
                for x in v {
                    a.push(mm(*x));
                }
                Value::Array(a)
            }
            FieldValue::Flag(b) => Value::from(*b),
            FieldValue::Bolts {
                count,
                bolt,
                diameter,
                hole,
                grade,
            } => {
                let mut t = InlineTable::new();
                t.insert("count", Value::from(i64::from(*count)));
                if let Some(b) = bolt {
                    t.insert("bolt", Value::from(b.as_str()));
                }
                if let Some(d) = diameter {
                    t.insert("diameter", mm(*d));
                }
                if let Some(h) = hole {
                    t.insert("hole", mm(*h));
                }
                if let Some(g) = grade.as_ref().filter(|g| !g.trim().is_empty()) {
                    t.insert("grade", Value::from(g.trim()));
                }
                Value::InlineTable(t)
            }
        }
    }
}

/// The `[[connection]]` of `member` at `joint` (none: not in the file).
fn find<'a>(doc: &'a mut DocumentMut, member: &str, joint: &str) -> Option<&'a mut Table> {
    doc.get_mut("connection")?
        .as_array_of_tables_mut()?
        .iter_mut()
        .find(|t| {
            t.get("member").and_then(Item::as_str) == Some(member)
                && t.get("joint").and_then(Item::as_str) == Some(joint)
        })
}

/// The `[[connection]]` of `member` at `joint`, made when missing.
fn find_or_make<'a>(doc: &'a mut DocumentMut, member: &str, joint: &str) -> &'a mut Table {
    if find(doc, member, joint).is_none() {
        let mut t = Table::new();
        t.insert("member", toml_edit::value(member));
        t.insert("joint", toml_edit::value(joint));
        array(doc, "connection").push(t);
    }
    raise_version(doc, i64::from(CONNECTIONS_VERSION));
    find(doc, member, joint).expect("made above")
}

/// Drops the connections that give nothing (their member and joint
/// only), and the array when empty.
fn tidy(doc: &mut DocumentMut) {
    if let Some(a) = doc
        .get_mut("connection")
        .and_then(Item::as_array_of_tables_mut)
    {
        a.retain(|t| t.iter().any(|(k, _)| k != "member" && k != "joint"));
        if a.is_empty() {
            doc.remove("connection");
        }
    }
}

/// Gives (`Some`) or clears (`None`) `field` of `member`'s end at `joint`:
/// a connection made when missing, dropped when nothing is left in it.
pub fn set_connection_field(
    doc: &mut DocumentMut,
    member: &str,
    joint: &str,
    field: &str,
    value: Option<&FieldValue>,
) -> Result<(), String> {
    if !FIELDS.contains(&field) {
        return Err(format!("no connection field {field}"));
    }
    match value {
        Some(v) => {
            let t = find_or_make(doc, member, joint);
            t.insert(field, Item::Value(v.value()));
        }
        None => {
            if let Some(t) = find(doc, member, joint) {
                t.remove(field);
            }
            tidy(doc);
        }
    }
    Ok(())
}

/// Clears `member`'s end at `joint`: back to the member's design data and
/// what is worked out.
pub fn clear_connection(doc: &mut DocumentMut, member: &str, joint: &str) {
    if let Some(a) = doc
        .get_mut("connection")
        .and_then(Item::as_array_of_tables_mut)
    {
        a.retain(|t| {
            !(t.get("member").and_then(Item::as_str) == Some(member)
                && t.get("joint").and_then(Item::as_str) == Some(joint))
        });
    }
    tidy(doc);
}

/// The carried fields of `member`'s end at `joint` as the file gives
/// them (empty: none given).
pub fn carried(doc: &DocumentMut, member: &str, joint: &str) -> Table {
    let mut out = Table::new();
    let found = doc
        .get("connection")
        .and_then(Item::as_array_of_tables)
        .and_then(|a| {
            a.iter().find(|t| {
                t.get("member").and_then(Item::as_str) == Some(member)
                    && t.get("joint").and_then(Item::as_str) == Some(joint)
            })
        });
    if let Some(t) = found {
        for (k, v) in t.iter() {
            if CARRIED.contains(&k) {
                out.insert(k, v.clone());
            }
        }
    }
    out
}

/// Writes `fields` (carried fields) onto `member`'s end at `joint`,
/// replacing its carried fields (its `onto` and bolt points kept).
pub fn put_carried(doc: &mut DocumentMut, member: &str, joint: &str, fields: &Table) {
    let t = find_or_make(doc, member, joint);
    for k in CARRIED {
        t.remove(k);
    }
    for (k, v) in fields.iter() {
        if CARRIED.contains(&k) {
            let v = match v {
                // Inline in the connection's table.
                Item::Table(x) => Item::Value(Value::InlineTable(x.clone().into_inline_table())),
                other => other.clone(),
            };
            t.insert(k, v);
        }
    }
    tidy(doc);
}

/// One member end to another: the source's member and joint, the
/// target's, and the member the target bolts onto where the source names
/// one (a plate: copied to the target's joint).
#[derive(Debug, Clone, PartialEq)]
pub struct CopyEnd {
    /// The source member.
    pub from_member: String,
    /// The source joint.
    pub from_joint: String,
    /// The target member.
    pub to_member: String,
    /// The target joint.
    pub to_joint: String,
    /// The target's `onto` for the source's (a member's id), when the
    /// source bolts onto a member.
    pub onto: Option<String>,
}

/// Copies whole connections (`onto` mapped, bolt points kept) per
/// `ends`; a source onto a plate gets a copy of the plate at the target's
/// joint (one per source plate and joint). The targets' connections are
/// replaced; a source with none clears its target. How many ends were
/// given.
pub fn copy_ends(doc: &mut DocumentMut, t: &StructureType, ends: &[CopyEnd]) -> usize {
    let mut plates: std::collections::BTreeMap<(String, String), String> = Default::default();
    let mut given = 0;
    for e in ends {
        let Some(c) = t.connection(&e.from_member, &e.from_joint) else {
            clear_connection(doc, &e.to_member, &e.to_joint);
            continue;
        };
        given += 1;
        let fields = carried(doc, &e.from_member, &e.from_joint);
        clear_connection(doc, &e.to_member, &e.to_joint);
        put_carried(doc, &e.to_member, &e.to_joint, &fields);
        let onto = match &c.onto {
            Some(o) if t.plates.iter().any(|p| &p.id == o) => {
                let key = (o.clone(), e.to_joint.clone());
                let id = match plates.get(&key) {
                    Some(id) => id.clone(),
                    None => {
                        let id = copy_plate(doc, t, o, &e.to_joint);
                        plates.insert(key, id.clone());
                        id
                    }
                };
                Some(id)
            }
            Some(_) => e.onto.clone(),
            None => None,
        };
        let target = find_or_make(doc, &e.to_member, &e.to_joint);
        if let Some(o) = onto {
            target.insert("onto", toml_edit::value(o));
        }
        if !c.bolt_points.is_empty() {
            let mut a = toml_edit::Array::new();
            for p in &c.bolt_points {
                let mut q = toml_edit::Array::new();
                q.push(mm(p[0].si()));
                q.push(mm(p[1].si()));
                a.push(Value::Array(q));
            }
            target.insert("bolt_points", Item::Value(Value::Array(a)));
        }
        tidy(doc);
    }
    // The copied plates welded as their sources were (at the same target
    // joint).
    for ((src, joint), new) in &plates {
        let Some(p) = t.plates.iter().find(|p| &p.id == src) else {
            continue;
        };
        let to: Vec<String> = p
            .welded_to
            .iter()
            .filter_map(|o| plates.get(&(o.clone(), joint.clone())).cloned())
            .collect();
        if !to.is_empty() {
            let _ = set_welded_to(doc, new, &to);
        }
    }
    given
}

/// Copies plate `id` to `joint` (its outline, at another joint, left out:
/// drawn around its bolts); the new plate's id.
fn copy_plate(doc: &mut DocumentMut, t: &StructureType, id: &str, joint: &str) -> String {
    let Some(p) = t.plates.iter().find(|p| p.id == id) else {
        return id.to_owned();
    };
    let taken: Vec<String> = t
        .plates
        .iter()
        .map(|p| p.id.clone())
        .chain(plate_ids(doc))
        .collect();
    let new = next_id(taken.iter().map(String::as_str), id);
    let mut x = Table::new();
    x.insert("id", toml_edit::value(new.clone()));
    x.insert("joint", toml_edit::value(joint));
    x.insert("thickness", Item::Value(mm(p.thickness.si())));
    if let Some(m) = &p.material {
        x.insert("material", toml_edit::value(m.clone()));
    }
    if let Some(k) = p.kind {
        x.insert("kind", toml_edit::value(k.as_str()));
    }
    array(doc, "plate").push(x);
    raise_version(doc, i64::from(CONNECTIONS_VERSION));
    new
}

fn plate_ids(doc: &DocumentMut) -> Vec<String> {
    doc.get("plate")
        .and_then(Item::as_array_of_tables)
        .map(|a| {
            a.iter()
                .filter_map(|t| t.get("id").and_then(Item::as_str).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Adds a plate at `joint`, `thickness` m thick (a joint may have
/// several); its id (`G1`, `G2`…).
pub fn add_plate(doc: &mut DocumentMut, t: &StructureType, joint: &str, thickness: f64) -> String {
    let taken: Vec<String> = t
        .plates
        .iter()
        .map(|p| p.id.clone())
        .chain(t.members.iter().map(|m| m.id.clone()))
        .chain(plate_ids(doc))
        .collect();
    let like = t.plates.last().map_or("G1", |p| p.id.as_str());
    let id = next_id(taken.iter().map(String::as_str), like);
    let mut x = Table::new();
    x.insert("id", toml_edit::value(id.clone()));
    x.insert("joint", toml_edit::value(joint));
    x.insert("thickness", Item::Value(mm(thickness)));
    array(doc, "plate").push(x);
    raise_version(doc, i64::from(CONNECTIONS_VERSION));
    id
}

/// The plates plate `id` is welded to (`welded_to`; empty: the key
/// removed). The 2026-11-03 `kind` is dropped: the welding is now the
/// plates' own relation ([`ohl_structures::StructureType::welded_assemblies`]).
pub fn set_welded_to(doc: &mut DocumentMut, id: &str, to: &[String]) -> Result<(), String> {
    let t = doc
        .get_mut("plate")
        .and_then(Item::as_array_of_tables_mut)
        .and_then(|a| {
            a.iter_mut()
                .find(|t| t.get("id").and_then(Item::as_str) == Some(id))
        })
        .ok_or_else(|| format!("no plate {id}"))?;
    if to.iter().any(|x| x == id) {
        return Err(format!("plate {id}: not welded to itself"));
    }
    t.remove("kind");
    if to.is_empty() {
        t.remove("welded_to");
    } else {
        let mut a = toml_edit::Array::new();
        for x in to {
            a.push(x.as_str());
        }
        t.insert("welded_to", Item::Value(Value::Array(a)));
    }
    Ok(())
}

/// Plates `a` and `b` (of one joint) welded to each other (`on`) or not:
/// written as `a`'s `welded_to` (taken off both when not). The joint's
/// plates of the 2026-11-03 `kind = "welded"` are first written as what
/// they are read as (each welded to the one before it), their kind
/// dropped.
pub fn weld_pair(
    doc: &mut DocumentMut,
    t: &StructureType,
    a: &str,
    b: &str,
    on: bool,
) -> Result<(), String> {
    let (Some(pa), Some(pb)) = (
        t.plates.iter().find(|p| p.id == a),
        t.plates.iter().find(|p| p.id == b),
    ) else {
        return Err(format!("no plates {a} and {b}"));
    };
    if a == b || pa.joint != pb.joint {
        return Err(format!(
            "plates {a} and {b}: two plates of one joint are welded to each other"
        ));
    }
    let given = |id: &str| -> Vec<String> {
        t.plates
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.welded_to.clone())
            .unwrap_or_default()
    };
    let mut lists: std::collections::BTreeMap<String, Vec<String>> = t
        .plates
        .iter()
        .filter(|p| p.joint == pa.joint)
        .map(|p| (p.id.clone(), given(&p.id)))
        .collect();
    // The 2026-11-03 kind as it is read.
    let legacy: Vec<&str> = t
        .plates
        .iter()
        .filter(|p| {
            p.joint == pa.joint && p.kind() == ohl_structures::connections::PlateKind::Welded
        })
        .map(|p| p.id.as_str())
        .collect();
    for w in legacy.windows(2) {
        let l = lists.entry(w[1].to_owned()).or_default();
        if !l.iter().any(|x| x == w[0]) {
            l.push(w[0].to_owned());
        }
    }
    if on {
        let related = lists.get(a).is_some_and(|l| l.iter().any(|x| x == b))
            || lists.get(b).is_some_and(|l| l.iter().any(|x| x == a));
        if !related {
            lists.entry(a.to_owned()).or_default().push(b.to_owned());
        }
    } else {
        for (x, y) in [(a, b), (b, a)] {
            if let Some(l) = lists.get_mut(x) {
                l.retain(|o| o != y);
            }
        }
    }
    for (id, l) in &lists {
        let before = given(id);
        let was_legacy = legacy.contains(&id.as_str());
        if *l != before || was_legacy {
            set_welded_to(doc, id, l)?;
        }
    }
    Ok(())
}

/// A welded plate at `joint` (owner 2026-11-04: "multiple plates are
/// welded to be able to reach every member bolts that come on the
/// joint"): the joint's plates replaced by one plate per group of
/// `groups` (members lying in one plane, `ohl_structure_link::welded_plan`),
/// `thickness` m thick, each welded to the ones before it, each member of
/// a group bolted onto its plate (`onto`; its other fields kept). The
/// plates' outlines are left to be worked out round their bolts (the
/// code's edge distances kept). The new plates' ids.
pub fn make_welded_plates(
    doc: &mut DocumentMut,
    t: &StructureType,
    joint: &str,
    groups: &[Vec<String>],
    thickness: f64,
) -> Vec<String> {
    for p in t.plates.iter().filter(|p| p.joint == joint) {
        remove_plate(doc, &p.id);
    }
    // Ids not taken by the plates kept or the members.
    let mut taken: Vec<String> = t
        .plates
        .iter()
        .filter(|p| p.joint != joint)
        .map(|p| p.id.clone())
        .chain(t.members.iter().map(|m| m.id.clone()))
        .collect();
    let like = t.plates.first().map_or("G1", |p| p.id.as_str()).to_owned();
    let mut ids: Vec<String> = Vec::new();
    for g in groups {
        let id = next_id(taken.iter().map(String::as_str), &like);
        taken.push(id.clone());
        let mut x = Table::new();
        x.insert("id", toml_edit::value(id.clone()));
        x.insert("joint", toml_edit::value(joint));
        x.insert("thickness", Item::Value(mm(thickness)));
        if !ids.is_empty() {
            let mut a = toml_edit::Array::new();
            for w in &ids {
                a.push(w.as_str());
            }
            x.insert("welded_to", Item::Value(Value::Array(a)));
        }
        array(doc, "plate").push(x);
        for m in g {
            find_or_make(doc, m, joint).insert("onto", toml_edit::value(id.clone()));
        }
        ids.push(id);
    }
    if !ids.is_empty() {
        raise_version(doc, i64::from(CONNECTIONS_VERSION));
    }
    tidy(doc);
    ids
}

/// A plate's thickness (m) or material (empty: none).
pub fn set_plate(
    doc: &mut DocumentMut,
    id: &str,
    thickness: Option<f64>,
    material: Option<&str>,
) -> Result<(), String> {
    let t = doc
        .get_mut("plate")
        .and_then(Item::as_array_of_tables_mut)
        .and_then(|a| {
            a.iter_mut()
                .find(|t| t.get("id").and_then(Item::as_str) == Some(id))
        })
        .ok_or_else(|| format!("no plate {id}"))?;
    if let Some(v) = thickness {
        if v <= 0.0 {
            return Err("a plate's thickness above 0".into());
        }
        t.insert("thickness", Item::Value(mm(v)));
    }
    if let Some(m) = material {
        if m.trim().is_empty() {
            t.remove("material");
        } else {
            t.insert("material", toml_edit::value(m.trim()));
        }
    }
    Ok(())
}

/// Removes plate `id`; the connections onto it lose their `onto`, the
/// plates welded to it no longer name it.
pub fn remove_plate(doc: &mut DocumentMut, id: &str) {
    if let Some(a) = doc.get_mut("plate").and_then(Item::as_array_of_tables_mut) {
        a.retain(|t| t.get("id").and_then(Item::as_str) != Some(id));
        for t in a.iter_mut() {
            let empty = match t.get_mut("welded_to").and_then(Item::as_array_mut) {
                Some(w) => {
                    w.retain(|v| v.as_str() != Some(id));
                    w.is_empty()
                }
                None => false,
            };
            if empty {
                t.remove("welded_to");
            }
        }
        if a.is_empty() {
            doc.remove("plate");
        }
    }
    if let Some(a) = doc
        .get_mut("connection")
        .and_then(Item::as_array_of_tables_mut)
    {
        for t in a.iter_mut() {
            if t.get("onto").and_then(Item::as_str) == Some(id) {
                t.remove("onto");
            }
        }
    }
    tidy(doc);
}

/// The bolt layout helper: `count` bolts of `diameter` (m) in `lines`
/// gauge lines at `gauges`, `pitch` apart, the first `end` from the
/// member's end, `edge` from the leg's edge — written in one go (each
/// given; `None` clears).
#[derive(Debug, Clone, PartialEq)]
pub struct BoltLayout {
    /// How many.
    pub count: u32,
    /// A bolt of the bolt libraries: its diameter, hole and grade not
    /// written again (those typed differently are, overriding it).
    pub bolt: Option<String>,
    /// Diameter, m.
    pub diameter: f64,
    /// Hole, m.
    pub hole: Option<f64>,
    /// Grade.
    pub grade: Option<String>,
    /// Gauges, one per line (1 to 3), m.
    pub gauges: Vec<f64>,
    /// Pitch, m.
    pub pitch: f64,
    /// End distance, m.
    pub end: f64,
    /// Edge distance, m (none: not given).
    pub edge: Option<f64>,
}

impl BoltLayout {
    /// How far along the member the bolts reach from its end, m (the
    /// farthest bolt; every second line staggered by half a pitch, as the
    /// joints lay them out).
    pub fn reach(&self) -> f64 {
        let lines = self.gauges.len().max(1) as u32;
        let far = (0..self.count.max(1))
            .map(|k| {
                let stagger = if (k % lines) % 2 == 1 { 0.5 } else { 0.0 };
                f64::from(k / lines) + stagger
            })
            .fold(0.0, f64::max);
        self.end + far * self.pitch
    }

    /// What does not fit a leg of `width` m (empty: it fits): a gauge
    /// beyond the leg less the edge distance, two lines closer than the
    /// pitch's half… In words.
    pub fn misfits(&self, width: Option<f64>) -> Vec<String> {
        let mut out = Vec::new();
        if self.count == 0 {
            out.push("no bolts".into());
        }
        if !(1..=MAX_GAUGE_LINES as usize).contains(&self.gauges.len()) {
            out.push(format!("one to {MAX_GAUGE_LINES} gauge lines"));
        }
        if self.pitch <= self.diameter && self.count > self.gauges.len() as u32 {
            out.push("a pitch no larger than the bolt".into());
        }
        if let Some(w) = width {
            for g in &self.gauges {
                let room = w - self.edge.unwrap_or(0.0);
                if *g > room + 1e-9 {
                    out.push(format!(
                        "gauge {:.0} mm beyond the leg ({:.0} mm{})",
                        g * 1000.0,
                        w * 1000.0,
                        if self.edge.is_some() {
                            " less the edge distance"
                        } else {
                            ""
                        }
                    ));
                }
            }
        }
        out
    }
}

/// Writes `layout` on `member`'s end at `joint` (one undo step for the
/// caller): the bolts, the gauge lines and gauges, the pitch, end and
/// edge distances; bolt points given before are dropped.
pub fn set_bolt_layout(doc: &mut DocumentMut, member: &str, joint: &str, l: &BoltLayout) {
    let t = find_or_make(doc, member, joint);
    let bolts = FieldValue::Bolts {
        count: l.count,
        bolt: l.bolt.clone(),
        diameter: (l.bolt.is_none()).then_some(l.diameter),
        hole: if l.bolt.is_some() { None } else { l.hole },
        grade: if l.bolt.is_some() {
            None
        } else {
            l.grade.clone()
        },
    };
    t.insert("bolts", Item::Value(bolts.value()));
    t.insert(
        "gauge_lines",
        Item::Value(Value::from(l.gauges.len() as i64)),
    );
    t.insert(
        "gauges",
        Item::Value(FieldValue::Lengths(l.gauges.clone()).value()),
    );
    t.insert("pitch", Item::Value(mm(l.pitch)));
    t.insert("end_distance", Item::Value(mm(l.end)));
    match l.edge {
        Some(e) => {
            t.insert("edge_distance", Item::Value(mm(e)));
        }
        None => {
            t.remove("edge_distance");
        }
    }
    t.remove("bolt_points");
}

/// A preset of a connection: a name and its carried fields
/// (`[[preset]]` of an `.ohlcp` file, format `ohl-connection-presets`).
#[derive(Debug, Clone)]
pub struct Preset {
    /// Its name.
    pub name: String,
    /// Its fields.
    pub fields: Table,
}

/// The format of the connection presets file.
pub const PRESETS_FORMAT: &str = "ohl-connection-presets";
/// Its extension.
pub const PRESETS_EXTENSION: &str = "ohlcp";

/// The presets of an `.ohlcp` file's text; refused: another format, a
/// newer version, a field a connection has not (or `onto`, `bolt_points`,
/// `member`, `joint`), a preset without a name or named twice. The
/// fields are checked by reading them as a connection.
pub fn parse_presets(text: &str) -> Result<Vec<Preset>, String> {
    let doc: DocumentMut = text.parse().map_err(|e| format!("{e}"))?;
    if let Some(f) = doc.get("format").and_then(Item::as_str)
        && f != PRESETS_FORMAT
    {
        return Err(format!("format {f}: not {PRESETS_FORMAT}"));
    }
    if doc
        .get("version")
        .and_then(Item::as_integer)
        .is_some_and(|v| v > 1)
    {
        return Err("a newer version of connection presets".into());
    }
    let mut out: Vec<Preset> = Vec::new();
    if let Some(a) = doc.get("preset").and_then(Item::as_array_of_tables) {
        for t in a.iter() {
            let name = t
                .get("name")
                .and_then(Item::as_str)
                .filter(|n| !n.trim().is_empty())
                .ok_or("a preset without a name")?
                .trim()
                .to_owned();
            if out.iter().any(|p| p.name == name) {
                return Err(format!("preset {name}: named twice"));
            }
            let mut fields = Table::new();
            for (k, v) in t.iter() {
                if k == "name" {
                    continue;
                }
                if !CARRIED.contains(&k) {
                    return Err(format!("preset {name}: field {k} is not a preset's"));
                }
                fields.insert(k, v.clone());
            }
            // Read as a connection: the values' types and limits.
            let mut probe = DocumentMut::new();
            let mut c = fields.clone();
            c.insert("member", toml_edit::value("m"));
            c.insert("joint", toml_edit::value("j"));
            array(&mut probe, "connection").push(c);
            let text = format!(
                "id = \"p\"\nname = \"p\"\nkind = \"lattice\"\n\
                 [[node]]\nid = \"j\"\npoint = [\"0 m\", \"0 m\", \"0 m\"]\n\
                 [[node]]\nid = \"k\"\npoint = [\"0 m\", \"0 m\", \"1 m\"]\n\
                 [[member]]\nid = \"m\"\nnodes = [\"j\", \"k\"]\n{probe}"
            );
            <StructureType as ohl_structures::LibraryItem>::parse(
                std::path::Path::new("preset.ohls"),
                &text,
            )
            .map_err(|e| format!("preset {name}: {}", e.message))?;
            out.push(Preset { name, fields });
        }
    }
    Ok(out)
}

/// The text of an `.ohlcp` file holding `presets`.
pub fn presets_text(presets: &[Preset]) -> String {
    let mut doc = DocumentMut::new();
    doc.insert("format", toml_edit::value(PRESETS_FORMAT));
    doc.insert("version", toml_edit::value(1));
    for p in presets {
        let mut t = Table::new();
        t.insert("name", toml_edit::value(p.name.clone()));
        for (k, v) in p.fields.iter() {
            let v = match v {
                Item::Table(x) => Item::Value(Value::InlineTable(x.clone().into_inline_table())),
                other => other.clone(),
            };
            t.insert(k, v);
        }
        array(&mut doc, "preset").push(t);
    }
    format!("# Connection presets of OpenOHL (OHL-Structure › Joint details).\n{doc}")
}

/// Writes `member`'s bolts at `joint` as their places (`bolt_points`:
/// along the member from the joint, across its leg bolted from the heel,
/// m) — a bolt added or taken away by hand; the layout's gauge lines,
/// pitch and end distance go (the places are the bolts now), the bolts'
/// count follows. `bolts`: the bolts' kind to write when the end gives
/// none yet (its diameter: the member's or assumed; none: keep the
/// end's). No places left: the end's bolts go.
pub fn set_bolt_points(
    doc: &mut DocumentMut,
    member: &str,
    joint: &str,
    points: &[[f64; 2]],
    bolts: Option<&FieldValue>,
) {
    let t = find_or_make(doc, member, joint);
    for k in ["gauge_lines", "gauges", "pitch", "end_distance"] {
        t.remove(k);
    }
    if points.is_empty() {
        t.remove("bolt_points");
        t.remove("bolts");
        tidy(doc);
        return;
    }
    let mut a = toml_edit::Array::new();
    for p in points {
        let mut q = toml_edit::Array::new();
        q.push(mm(p[0]));
        q.push(mm(p[1]));
        a.push(Value::Array(q));
    }
    t.insert("bolt_points", Item::Value(Value::Array(a)));
    if let Some(b) = bolts.filter(|_| t.get("bolts").is_none()) {
        t.insert("bolts", Item::Value(b.value()));
    }
    if let Some(b) = t.get_mut("bolts").and_then(Item::as_inline_table_mut) {
        b.insert("count", Value::from(points.len() as i64));
    } else if let Some(b) = t.get_mut("bolts").and_then(Item::as_table_mut) {
        b.insert("count", toml_edit::value(points.len() as i64));
    }
}

/// Plate `id`'s outline written (structure's axes, m): fixed where it is
/// drawn now (around its bolts), to be edited.
pub fn set_plate_outline(
    doc: &mut DocumentMut,
    id: &str,
    outline: &[[f64; 3]],
) -> Result<(), String> {
    if outline.len() < 3 {
        return Err(format!("plate {id}: an outline of three points or more"));
    }
    let t = doc
        .get_mut("plate")
        .and_then(Item::as_array_of_tables_mut)
        .and_then(|a| {
            a.iter_mut()
                .find(|t| t.get("id").and_then(Item::as_str) == Some(id))
        })
        .ok_or_else(|| format!("no plate {id}"))?;
    let mut a = toml_edit::Array::new();
    for p in outline {
        let mut q = toml_edit::Array::new();
        for v in p {
            q.push(format!("{:.4} m", v));
        }
        a.push(Value::Array(q));
    }
    t.insert("outline", Item::Value(Value::Array(a)));
    Ok(())
}
