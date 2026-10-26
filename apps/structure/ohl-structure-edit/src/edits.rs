// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The edits of a structure file: each a function on the file as edited
//! (`doc`), given the structure as last read (`t`). Only the tables and
//! fields concerned change; comments and the rest of the file stay.

use ohl_structures::{AssemblyKind, InsulatorAssembly, InsulatorLibrary, StructureType};
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, Value};

use crate::geometry::{Geometry, Handle, polar};

/// `like` with its trailing number increased until no id in `taken` has
/// it (`T7` → `T8`; `S` → `S-1`).
pub fn next_id<'a>(taken: impl Iterator<Item = &'a str> + Clone, like: &str) -> String {
    let digits = like.len() - like.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let (stem, num) = like.split_at(like.len() - digits);
    let width = num.len();
    let (stem, mut n) = match num.parse::<u64>() {
        Ok(n) => (stem.to_owned(), n + 1),
        Err(_) => (format!("{like}-"), 1),
    };
    loop {
        let id = format!("{stem}{n:0width$}");
        if !taken.clone().any(|t| t == id) {
            return id;
        }
        n += 1;
    }
}

/// `like` when it is free, else the next id after it.
pub(crate) fn fresh<'a>(taken: impl Iterator<Item = &'a str> + Clone, like: &str) -> String {
    if taken.clone().any(|t| t == like) {
        next_id(taken, like)
    } else {
        like.to_owned()
    }
}

/// A position as written in files: `["1.250 m", …]`.
pub(crate) fn point_value(p: [f64; 3]) -> Item {
    let mut a = Array::new();
    for v in p {
        let v = if v.abs() < 5e-4 { 0.0 } else { v };
        a.push(format!("{v:.3} m"));
    }
    Item::Value(Value::Array(a))
}

/// A length as written in files: `"1.250 m"`.
fn length(v: f64) -> Item {
    toml_edit::value(format!("{v:.3} m"))
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// The array of tables `key` of the file (made when missing).
pub(crate) fn array<'a>(doc: &'a mut DocumentMut, key: &str) -> &'a mut ArrayOfTables {
    if doc.get(key).and_then(Item::as_array_of_tables).is_none() {
        doc.insert(key, Item::ArrayOfTables(ArrayOfTables::new()));
    }
    doc.get_mut(key)
        .and_then(Item::as_array_of_tables_mut)
        .expect("made above")
}

/// The table of `key` whose `field` is `value`.
pub(crate) fn entry<'a>(
    doc: &'a mut DocumentMut,
    key: &str,
    field: &str,
    value: &str,
) -> Option<&'a mut Table> {
    doc.get_mut(key)?
        .as_array_of_tables_mut()?
        .iter_mut()
        .find(|t| t.get(field).and_then(Item::as_str) == Some(value))
}

/// The labels of the points and wire points (they share one name space).
fn labels(t: &StructureType) -> Vec<&str> {
    t.points
        .iter()
        .map(|p| p.label.as_str())
        .chain(t.wires.iter().map(|w| w.label.as_str()))
        .collect()
}

/// What goes with a deletion.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Removal {
    /// Joints.
    pub joints: Vec<String>,
    /// Members.
    pub members: Vec<String>,
    /// Points.
    pub points: Vec<String>,
    /// Wire points.
    pub wires: Vec<String>,
    /// Strings.
    pub strings: Vec<String>,
}

impl Removal {
    /// `more` added (each item once).
    pub fn merge(&mut self, more: Removal) {
        for (all, add) in [
            (&mut self.joints, more.joints),
            (&mut self.members, more.members),
            (&mut self.points, more.points),
            (&mut self.wires, more.wires),
            (&mut self.strings, more.strings),
        ] {
            for x in add {
                if !all.contains(&x) {
                    all.push(x);
                }
            }
        }
    }

    /// One line per kind, for the confirmation.
    pub fn lines(&self) -> Vec<String> {
        [
            ("joint", &self.joints),
            ("member", &self.members),
            ("point", &self.points),
            ("wire point", &self.wires),
            ("string", &self.strings),
        ]
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{} {k}(s): {}", v.len(), v.join(", ")))
        .collect()
    }
}

/// What goes when `h` is deleted: a joint takes its members and the
/// points on it; a point or wire point the strings from or to it.
pub fn removal(t: &StructureType, h: &Handle) -> Removal {
    let mut r = Removal::default();
    match h {
        Handle::Joint(id) => {
            r.joints.push(id.clone());
            r.members = t
                .members
                .iter()
                .filter(|m| m.nodes.contains(id))
                .map(|m| m.id.clone())
                .collect();
        }
        Handle::Member(id) => r.members.push(id.clone()),
        Handle::Point(l) => r.points.push(l.clone()),
        Handle::Wire(l) => r.wires.push(l.clone()),
    }
    for p in &t.points {
        let on = p.node.as_ref().is_some_and(|n| r.joints.contains(n));
        if on && !r.points.contains(&p.label) {
            r.points.push(p.label.clone());
        }
    }
    for s in &t.strings {
        let gone = [&s.top, &s.wire]
            .iter()
            .any(|l| r.points.contains(l) || r.wires.contains(l));
        if gone {
            r.strings.push(s.label.clone());
        }
    }
    r
}

/// Removes everything in `r` from the file.
pub fn remove(doc: &mut DocumentMut, r: &Removal) {
    for (key, field, gone) in [
        ("node", "id", &r.joints),
        ("member", "id", &r.members),
        ("point", "label", &r.points),
        ("wire", "label", &r.wires),
        ("string", "label", &r.strings),
    ] {
        if let Some(a) = doc.get_mut(key).and_then(Item::as_array_of_tables_mut) {
            a.retain(|t| {
                !t.get(field)
                    .and_then(Item::as_str)
                    .is_some_and(|v| gone.iter().any(|g| g == v))
            });
        }
    }
}

/// Adds a joint at `at` (local axes); its id.
pub fn add_joint(doc: &mut DocumentMut, t: Option<&StructureType>, at: [f64; 3]) -> String {
    let like = t
        .and_then(|t| t.nodes.last())
        .map_or("N1", |n| n.id.as_str());
    let id = next_id(
        t.into_iter()
            .flat_map(|t| t.nodes.iter().map(|n| n.id.as_str())),
        like,
    );
    let mut table = Table::new();
    table.insert("id", toml_edit::value(id.clone()));
    table.insert("point", point_value(at));
    array(doc, "node").push(table);
    id
}

/// Adds a member between joints `a` and `b`, like the last member (its
/// section and kind); its id (or why not).
pub fn add_member(
    doc: &mut DocumentMut,
    t: &StructureType,
    a: &str,
    b: &str,
) -> Result<String, String> {
    if a == b {
        return Err("pick two different joints".into());
    }
    if t.members
        .iter()
        .any(|m| (m.nodes[0] == a && m.nodes[1] == b) || (m.nodes[0] == b && m.nodes[1] == a))
    {
        return Err(format!("joints {a} and {b} are joined already"));
    }
    let like = t.members.last().map_or("M1", |m| m.id.as_str());
    let id = next_id(t.members.iter().map(|m| m.id.as_str()), like);
    let mut table = Table::new();
    table.insert("id", toml_edit::value(id.clone()));
    let mut nodes = Array::new();
    nodes.push(a);
    nodes.push(b);
    table.insert("nodes", Item::Value(Value::Array(nodes)));
    if let Some(last) = t.members.last() {
        if let Some(s) = &last.section {
            table.insert("section", toml_edit::value(s.clone()));
        }
        if let Some(k) = &last.kind {
            table.insert("kind", toml_edit::value(k.clone()));
        }
    }
    array(doc, "member").push(table);
    Ok(id)
}

/// A point of the structure at joint `joint`; its label.
pub fn add_point_at_joint(doc: &mut DocumentMut, t: &StructureType, joint: &str) -> String {
    let label = fresh(labels(t).into_iter(), "A1");
    let mut table = Table::new();
    table.insert("label", toml_edit::value(label.clone()));
    table.insert("node", toml_edit::value(joint));
    array(doc, "point").push(table);
    label
}

/// A point of the structure at `at` (on no joint: a stick figure's);
/// its label.
pub fn add_point(doc: &mut DocumentMut, t: &StructureType, at: [f64; 3]) -> String {
    let label = fresh(labels(t).into_iter(), "A1");
    let mut table = Table::new();
    table.insert("label", toml_edit::value(label.clone()));
    table.insert("point", point_value(at));
    array(doc, "point").push(table);
    label
}

/// Presets: an I-string of `length` from `top` (a point or a wire point)
/// down to a new wire point; or, with two tops, a V: two strings to one
/// wire point `length` below the middle of the tops. No set: the set is
/// put on it by hand. The new wire point's label.
pub fn add_strings(
    doc: &mut DocumentMut,
    t: &StructureType,
    tops: &[String],
    length: f64,
) -> Result<String, String> {
    if tops.is_empty() || tops.len() > 2 {
        return Err("pick one point (one string) or two (two strings to one wire point)".into());
    }
    if !(length > 0.0 && length < 50.0) {
        return Err("the string length must be between 0 and 50 m".into());
    }
    let wire = fresh(labels(t).into_iter(), "W1");
    let mut w = Table::new();
    w.insert("label", toml_edit::value(wire.clone()));
    array(doc, "wire").push(w);
    let mut taken: Vec<String> = t.strings.iter().map(|s| s.label.clone()).collect();
    for top in tops {
        let label = fresh(taken.iter().map(String::as_str), "S1");
        taken.push(label.clone());
        let mut s = Table::new();
        s.insert("label", toml_edit::value(label));
        s.insert("top", toml_edit::value(top.clone()));
        s.insert("wire", toml_edit::value(wire.clone()));
        s.insert("length", length_or_leg(t, tops, length));
        array(doc, "string").push(s);
    }
    Ok(wire)
}

/// The length written for a preset string: `length` for an I-string; for
/// a V the leg that puts the wire point `length` below the middle of the
/// tops.
fn length_or_leg(t: &StructureType, tops: &[String], l: f64) -> Item {
    if tops.len() != 2 {
        return length(l);
    }
    let at = |label: &str| {
        t.points
            .iter()
            .find(|p| p.label == label)
            .map(|p| p.position().map(|v| v.si()))
    };
    match (at(&tops[0]), at(&tops[1])) {
        (Some(a), Some(b)) => {
            let half = 0.5 * (0..3).map(|k| (b[k] - a[k]).powi(2)).sum::<f64>().sqrt();
            length(half.hypot(l))
        }
        _ => length(l),
    }
}

/// Moves a joint, a point or a wire point to `at`: a point on a joint
/// stays linked to it, its offset the new distance from it (the joint
/// does not move: owner decision 2026-10-21,
/// [`crate::links::move_point`]); a joint moved takes the points on it
/// along (their offsets kept); a member does not move.
pub fn move_to(
    doc: &mut DocumentMut,
    t: &StructureType,
    h: &Handle,
    at: [f64; 3],
) -> Result<(), String> {
    match h {
        Handle::Joint(id) => {
            let n = entry(doc, "node", "id", id).ok_or("no such joint")?;
            n.insert("point", point_value(at));
        }
        Handle::Point(l) => crate::links::move_point(doc, t, l, at)?,
        Handle::Wire(l) => {
            let w = entry(doc, "wire", "label", l).ok_or("no such wire point")?;
            w.insert("point", point_value(at));
        }
        Handle::Member(_) => return Err("move its joints".into()),
    }
    Ok(())
}

/// The support of joint `id`: `"pinned"`, `"fixed"`, or none.
pub fn set_support(doc: &mut DocumentMut, id: &str, support: Option<&str>) {
    if let Some(e) = entry(doc, "node", "id", id) {
        match support {
            Some(s) => {
                e.insert("support", toml_edit::value(s));
            }
            None => {
                e.remove("support");
            }
        }
    }
}

/// The section of member `id` (a `[[section]]` id), or none (a stick).
pub fn set_member_section(doc: &mut DocumentMut, id: &str, section: Option<&str>) {
    if let Some(e) = entry(doc, "member", "id", id) {
        match section {
            Some(s) => {
                e.insert("section", toml_edit::value(s));
            }
            None => {
                e.remove("section");
            }
        }
    }
}

/// The kinds of member, with what they are (the member tables' choice).
pub const MEMBER_KINDS: [(&str, &str); 4] = [
    ("truss", "axial force only"),
    ("frame", "bending too"),
    ("cable", "tension only, may be pretensioned: guys"),
    (
        "tension",
        "a truss in tension only (T-only bracing): in compression up to its own buckling \
         load, bowed beyond it; slack without a second moment",
    ),
];

/// The structure file version that has the member kind `tension`.
pub const TENSION_VERSION: i64 = 5;

/// The kind of member `id`: `truss`, `frame`, `cable` or `tension` (the
/// file raised to [`TENSION_VERSION`]).
pub fn set_member_kind(doc: &mut DocumentMut, id: &str, kind: &str) {
    if let Some(e) = entry(doc, "member", "id", id) {
        e.insert("kind", toml_edit::value(kind));
        if kind == "tension" {
            raise_version(doc, TENSION_VERSION);
        }
    }
}

/// The effective length factor of member `id` (none: the default).
pub fn set_member_k(doc: &mut DocumentMut, id: &str, k: Option<f64>) {
    if let Some(e) = entry(doc, "member", "id", id) {
        match k {
            Some(k) => {
                e.insert("k", toml_edit::value(k));
            }
            None => {
                e.remove("k");
            }
        }
    }
}

/// The counterweight at wire point `label`, N (0: none).
pub fn set_counterweight(doc: &mut DocumentMut, label: &str, weight: f64) {
    if let Some(e) = entry(doc, "wire", "label", label) {
        if weight > 0.0 {
            e.insert("weight", toml_edit::value(format!("{weight} N")));
        } else {
            e.remove("weight");
        }
    }
}

/// Set number `n` on wire point `label` (refused when another wire point
/// has it).
pub fn set_set(
    doc: &mut DocumentMut,
    t: &StructureType,
    label: &str,
    n: u32,
) -> Result<(), String> {
    if t.wires.iter().any(|x| x.label != label && x.set == Some(n)) {
        return Err(format!("set {n} is on another wire point already"));
    }
    let e = entry(doc, "wire", "label", label).ok_or("no such wire point")?;
    e.insert("set", toml_edit::value(i64::from(n)));
    Ok(())
}

/// The set of wire point `from` moved to wire point `to`.
pub fn move_set(doc: &mut DocumentMut, from: &str, to: &str, set: u32) {
    if let Some(e) = entry(doc, "wire", "label", from) {
        e.remove("set");
    }
    if let Some(e) = entry(doc, "wire", "label", to) {
        e.insert("set", toml_edit::value(i64::from(set)));
    }
}

/// The set taken off wire point `label` (the wire point stays).
pub fn drop_set(doc: &mut DocumentMut, label: &str) {
    if let Some(e) = entry(doc, "wire", "label", label) {
        e.remove("set");
    }
}

/// A set on wire point `label`: the next free number; that number.
pub fn add_set(doc: &mut DocumentMut, t: &StructureType, label: &str) -> Option<u32> {
    let next = (1..).find(|s| !t.wires.iter().any(|w| w.set == Some(*s)))?;
    let e = entry(doc, "wire", "label", label)?;
    e.insert("set", toml_edit::value(i64::from(next)));
    Some(next)
}

/// A new insulator from `top` (a point or a wire point): from the library
/// (`assembly`: its length, weight, stiffness, wind area; strain when it is
/// a strain assembly) or of `length`, hanging (or, strain, level along the
/// line: 90°), to `wire` (an existing wire point: a V with the insulator
/// there; none: a new wire point). Its label and its wire point's.
pub fn new_string(
    doc: &mut DocumentMut,
    t: &StructureType,
    top: &str,
    assembly: Option<&InsulatorAssembly>,
    length: f64,
    wire: Option<&str>,
) -> Result<(String, String), String> {
    if !labels(t).contains(&top) {
        return Err(format!("no attachment point or wire point {top}"));
    }
    if !(length > 0.0 && length < 50.0) && assembly.is_none() {
        return Err("the string length must be between 0 and 50 m".into());
    }
    if wire == Some(top) {
        return Err("a string cannot end where it hangs from".into());
    }
    let wire = match wire {
        Some(w) => w.to_owned(),
        None => {
            let w = fresh(labels(t).into_iter(), "W1");
            let mut table = Table::new();
            table.insert("label", toml_edit::value(w.clone()));
            array(doc, "wire").push(table);
            w
        }
    };
    let label = fresh(t.strings.iter().map(|x| x.label.as_str()), "S1");
    let mut x = Table::new();
    x.insert("label", toml_edit::value(label.clone()));
    x.insert("top", toml_edit::value(top));
    x.insert("wire", toml_edit::value(wire.clone()));
    match assembly {
        Some(a) => {
            x.insert("insulator", toml_edit::value(a.id.clone()));
            if a.kind == AssemblyKind::Strain {
                x.insert("strain", toml_edit::value(true));
                x.insert("angle", toml_edit::value("90 deg"));
            }
        }
        None => {
            x.insert("length", self::length(length));
        }
    }
    array(doc, "string").push(x);
    Ok((label, wire))
}

/// Deletes insulator `label`. Its wire point, held by no other insulator:
/// removed when nothing else is on it, else kept where it is (fixed: its
/// position written).
pub fn delete_string(doc: &mut DocumentMut, t: &StructureType, g: &Geometry, label: &str) {
    let Some(x) = t.strings.iter().find(|x| x.label == label) else {
        return;
    };
    if let Some(a) = doc.get_mut("string").and_then(Item::as_array_of_tables_mut) {
        a.retain(|e| e.get("label").and_then(Item::as_str) != Some(label));
    }
    let others = t
        .strings
        .iter()
        .any(|y| y.label != label && y.wire == x.wire);
    if others {
        return;
    }
    let w = t.wires.iter().find(|w| w.label == x.wire);
    let used = w.is_some_and(|w| w.set.is_some()) || t.strings.iter().any(|y| y.top == x.wire);
    if used {
        let at = g.wires.iter().find(|v| v.0 == x.wire).map(|v| v.1);
        if let (Some(at), Some(e)) = (at, entry(doc, "wire", "label", &x.wire)) {
            e.insert("point", point_value(at));
        }
    } else if let Some(a) = doc.get_mut("wire").and_then(Item::as_array_of_tables_mut) {
        a.retain(|e| e.get("label").and_then(Item::as_str) != Some(x.wire.as_str()));
    }
}

/// A change of one field of an insulator (a row of the Insulators table).
#[derive(Debug, Clone, PartialEq)]
pub enum StringChange {
    /// What it hangs from: a point or another insulator's wire point.
    Top(String),
    /// Its wire point (another insulator's makes a V).
    Wire(String),
    /// Strain (else suspension).
    Strain(bool),
    /// An assembly of the insulator library (its id), or none: a length.
    Assembly(Option<String>),
    /// Its length, m.
    Length(f64),
    /// Its angle from straight down and its direction in plan in the
    /// structure's axes (0 ahead, clockwise), degrees.
    Direction(f64, f64),
    /// Its weight, N.
    Weight(f64),
    /// Its wind area, m².
    WindArea(f64),
    /// The push it may take, N (none: tension only).
    Compression(Option<f64>),
    /// What it is, in words.
    Description(String),
    /// The tension it is rated for, N (none: not given; its insulator's
    /// then). Kept, not checked by OpenOHL.
    MaxLoad(Option<f64>),
    /// The swing from straight down it may take, any direction, degrees
    /// (none: not given). Kept, not checked.
    MaxSwing(Option<f64>),
    /// The swing across the line it may take, degrees (none: not given).
    MaxSwingAcross(Option<f64>),
    /// The swing along the line it may take, degrees (none: not given).
    MaxSwingAlong(Option<f64>),
}

/// The structure file version that has a string's limits.
pub const LIMITS_VERSION: i64 = 4;

/// One of an insulator's limits as an edit field (kept, not checked by
/// OpenOHL): what the tables of both applications show.
#[derive(Debug, Clone, Copy)]
pub struct LimitField {
    /// Its short name (`load`, `swing`, `across`, `along`).
    pub name: &'static str,
    /// The insulator's own value (none: not given).
    pub own: Option<f64>,
    /// Its assembly's (taken where it gives none).
    pub inherited: Option<f64>,
    /// The value a newly ticked field starts from.
    pub start: f64,
    /// An angle, degrees 0–180 (else a force, N).
    pub angle: bool,
    /// The change that sets it (none: not given).
    pub change: fn(Option<f64>) -> StringChange,
}

/// The four limits of an insulator as edit fields: its own (`own`) and
/// its assembly's (`from`).
pub fn limit_fields(
    own: ohl_structures::InsulatorLimits,
    from: ohl_structures::InsulatorLimits,
) -> [LimitField; 4] {
    let field = |name, own, inherited, start, angle, change| LimitField {
        name,
        own,
        inherited,
        start,
        angle,
        change,
    };
    [
        field(
            "load",
            own.max_load,
            from.max_load,
            100_000.0,
            false,
            StringChange::MaxLoad,
        ),
        field(
            "swing",
            own.max_swing_deg,
            from.max_swing_deg,
            60.0,
            true,
            StringChange::MaxSwing,
        ),
        field(
            "across",
            own.max_swing_across_deg,
            from.max_swing_across_deg,
            60.0,
            true,
            StringChange::MaxSwingAcross,
        ),
        field(
            "along",
            own.max_swing_along_deg,
            from.max_swing_along_deg,
            30.0,
            true,
            StringChange::MaxSwingAlong,
        ),
    ]
}

/// Raises the file's `version` to `version` (a file without a version
/// stays without: it reads as the newest).
pub fn raise_version(doc: &mut DocumentMut, version: i64) {
    if doc
        .get("version")
        .and_then(Item::as_integer)
        .is_some_and(|v| v < version)
    {
        doc["version"] = toml_edit::value(version);
    }
}

impl StringChange {
    /// Whether it moves the wire point (placed by the insulator again).
    fn moves(&self) -> bool {
        matches!(
            self,
            StringChange::Top(_)
                | StringChange::Wire(_)
                | StringChange::Assembly(_)
                | StringChange::Length(_)
                | StringChange::Direction(..)
        )
    }
}

/// Changes one field of insulator `label`. When its geometry changes its
/// wire point is placed by it again (a position written before goes, and
/// the other legs of a V without a length get their length now).
pub fn set_string(
    doc: &mut DocumentMut,
    t: &StructureType,
    g: &Geometry,
    insulators: &InsulatorLibrary,
    label: &str,
    change: StringChange,
) -> Result<(), String> {
    let s = t
        .strings
        .iter()
        .find(|s| s.label == label)
        .ok_or_else(|| format!("no string {label}"))?;
    let ends = g.at(&s.top).zip(g.at(&s.wire));
    let len_now = ends.map_or(0.0, |(a, b)| polar(a, b).2);
    let has_assembly = s
        .insulator
        .as_deref()
        .is_some_and(|i| insulators.get(i).is_some());
    let moves = change.moves();
    // A limit given: the file needs the version that has them.
    let limit_given = matches!(
        change,
        StringChange::MaxLoad(Some(_))
            | StringChange::MaxSwing(Some(_))
            | StringChange::MaxSwingAcross(Some(_))
            | StringChange::MaxSwingAlong(Some(_))
    );
    let swing_key = match change {
        StringChange::MaxSwing(_) => Some("max_swing"),
        StringChange::MaxSwingAcross(_) => Some("max_swing_across"),
        StringChange::MaxSwingAlong(_) => Some("max_swing_along"),
        _ => None,
    };
    let e = entry(doc, "string", "label", label).ok_or_else(|| format!("no string {label}"))?;
    match change {
        StringChange::Top(top) => {
            if top == s.wire {
                return Err("a string cannot end where it hangs from".into());
            }
            e.insert("top", toml_edit::value(top));
        }
        StringChange::Wire(wire) => {
            if wire == s.top {
                return Err("a string cannot end where it hangs from".into());
            }
            e.insert("wire", toml_edit::value(wire));
        }
        StringChange::Strain(strain) => {
            if strain {
                e.insert("strain", toml_edit::value(true));
            } else {
                e.remove("strain");
            }
        }
        StringChange::Assembly(id) => match id.as_deref().and_then(|i| insulators.get(i)) {
            Some(a) => {
                e.insert("insulator", toml_edit::value(a.id.clone()));
                for k in ["length", "weight", "wind_area"] {
                    e.remove(k);
                }
                if a.kind == AssemblyKind::Strain {
                    e.insert("strain", toml_edit::value(true));
                } else {
                    e.remove("strain");
                }
            }
            None if id.is_some() => return Err(format!("no insulator assembly {id:?}")),
            None => {
                e.remove("insulator");
                e.insert("length", length(len_now.max(0.01)));
            }
        },
        StringChange::Length(l) => {
            if !(l > 0.0 && l <= 50.0) {
                return Err("the string length must be between 0 and 50 m".into());
            }
            e.insert("length", length(l));
        }
        StringChange::Direction(angle, direction) => {
            e.insert("angle", toml_edit::value(format!("{angle} deg")));
            e.insert(
                "direction",
                toml_edit::value(format!("{} deg", direction.rem_euclid(360.0))),
            );
            e.remove("side");
            // Written before without a length: its length now.
            if s.length.is_none() && !has_assembly {
                e.insert("length", length(len_now.max(0.01)));
            }
        }
        StringChange::Weight(w) => {
            e.insert("weight", toml_edit::value(format!("{w} N")));
        }
        StringChange::WindArea(a) => {
            e.insert("wind_area", toml_edit::value(format!("{a} m²")));
        }
        StringChange::Compression(c) => match c {
            Some(c) => {
                e.insert("compression", toml_edit::value(format!("{c} N")));
            }
            None => {
                e.remove("compression");
            }
        },
        StringChange::Description(d) => {
            if d.trim().is_empty() {
                e.remove("description");
            } else {
                e.insert("description", toml_edit::value(d));
            }
        }
        StringChange::MaxLoad(v) => {
            if v.is_some_and(|l| !(l.is_finite() && l > 0.0)) {
                return Err("the maximum load must be > 0".into());
            }
            match v {
                Some(l) => {
                    e.insert("max_load", toml_edit::value(format!("{l} N")));
                }
                None => {
                    e.remove("max_load");
                }
            }
        }
        StringChange::MaxSwing(v)
        | StringChange::MaxSwingAcross(v)
        | StringChange::MaxSwingAlong(v) => {
            let key = swing_key.expect("a swing limit");
            if v.is_some_and(|a| !(0.0..=180.0).contains(&a)) {
                return Err("a maximum swing must be from 0 to 180°".into());
            }
            match v {
                Some(a) => {
                    e.insert(key, toml_edit::value(format!("{a} deg")));
                }
                None => {
                    e.remove(key);
                }
            }
        }
    }
    if limit_given {
        raise_version(doc, LIMITS_VERSION);
    }
    if moves {
        // The other insulators to it (a V) without a length (written
        // before): their length now, so it can be placed from them.
        for x in t.strings.iter().filter(|x| {
            x.wire == s.wire && x.label != s.label && x.length.is_none() && x.insulator.is_none()
        }) {
            let l = g
                .at(&x.top)
                .zip(g.at(&x.wire))
                .map_or(0.01, |(p, q)| polar(p, q).2.max(0.01));
            if let Some(o) = entry(doc, "string", "label", &x.label) {
                o.insert("length", length(l));
            }
        }
        if let Some(w) = entry(doc, "wire", "label", &s.wire) {
            w.remove("point");
        }
    }
    Ok(())
}

/// Insulator `label` as an assembly of the insulator library (`.ohli`
/// text, id `id`): its length, weight, wind area, strain or suspension (a
/// leg of a V: a V-string assembly).
pub fn assembly_text(
    t: &StructureType,
    g: &Geometry,
    insulators: &InsulatorLibrary,
    label: &str,
    id: &str,
) -> Result<String, String> {
    let x = t
        .strings
        .iter()
        .find(|x| x.label == label)
        .ok_or_else(|| format!("no string {label}"))?;
    let a = x.insulator.as_deref().and_then(|i| insulators.get(i));
    let length = match (a, x.length) {
        (Some(a), _) => a.length.si(),
        (None, Some(l)) => l.si(),
        (None, None) => g
            .at(&x.top)
            .zip(g.at(&x.wire))
            .map(|(p, q)| (0..3).map(|k| (p[k] - q[k]).powi(2)).sum::<f64>().sqrt())
            .ok_or("its length is not known")?,
    };
    let legs = t.strings.iter().filter(|y| y.wire == x.wire).count();
    let kind = if legs >= 2 {
        "v_string"
    } else if x.strain {
        "strain"
    } else {
        "suspension"
    };
    let per_leg = if legs >= 2 { legs as f64 } else { 1.0 };
    let weight = x
        .weight
        .map(|w| w.si())
        .or(a.map(|a| a.weight.si()))
        .unwrap_or(0.0)
        * per_leg;
    let wind = x
        .wind_area
        .map(|w| w.si())
        .or(a.map(|a| a.wind_area.si()))
        .unwrap_or(0.0)
        * per_leg;
    let stiffness = a.map_or(5e7, |a| a.axial_stiffness.si());
    // Its limits (its own, else its assembly's): version 2 when given.
    let limits = x
        .limits()
        .or(a.map(InsulatorAssembly::limits).unwrap_or_default());
    let mut text = String::new();
    text.push_str(&format!(
        "format = \"ohl-insulator\"\nversion = {}\n",
        if limits.is_empty() { 1 } else { 2 }
    ));
    text.push_str(&format!("id = {}\n", toml_edit::value(id)));
    text.push_str(&format!(
        "name = {}\n",
        toml_edit::value(format!("{id} (from string {label} of {})", t.id))
    ));
    text.push_str(&format!("kind = \"{kind}\"\n"));
    text.push_str(&format!("length = \"{} m\"\n", round3(length)));
    text.push_str(&format!("weight = \"{} N\"\n", round3(weight)));
    text.push_str(&format!("axial_stiffness = \"{stiffness} N\"\n"));
    if wind > 0.0 {
        text.push_str(&format!("wind_area = \"{} m²\"\n", round3(wind)));
    }
    if legs >= 2 {
        // The V's half width: from the middle of the legs' tops.
        let tops: Vec<[f64; 3]> = t
            .strings
            .iter()
            .filter(|y| y.wire == x.wire)
            .filter_map(|y| g.at(&y.top))
            .collect();
        if let [p, q, ..] = tops.as_slice() {
            let half = 0.5 * (p[0] - q[0]).hypot(p[1] - q[1]);
            text.push_str(&format!("half_width = \"{} m\"\n", round3(half)));
        }
    }
    text.push_str(&limits.to_toml());
    <InsulatorAssembly as ohl_structures::LibraryItem>::parse(
        std::path::Path::new("new.ohli"),
        &text,
    )
    .map_err(|e| e.message)?;
    Ok(text)
}

/// A field of the file itself (`name`, `height`, `foot`…): a text, or a
/// length in metres when `meters` (`32.5` or `32.5 m`); none removes it.
/// Err: a length that is not a finite number (the file unchanged; code
/// review 2026-10-26: a mistype was written as 0 m).
pub fn set_top_field(
    doc: &mut DocumentMut,
    field: &str,
    value: Option<&str>,
    meters: bool,
) -> Result<(), String> {
    match value {
        Some(v) if meters => {
            let m = ohl_core::units::parse_finite(v.trim().trim_end_matches('m'))
                .map_err(|e| format!("{field}: {e} (a length in m)"))?;
            doc.insert(field, length(m));
        }
        Some(v) => {
            doc.insert(field, toml_edit::value(v));
        }
        None => {
            doc.remove(field);
        }
    }
    Ok(())
}

/// A height extension `name` raising the structure by `dz`, m (a new one
/// when there is none of that name).
pub fn set_extension(doc: &mut DocumentMut, name: &str, dz: f64) {
    match entry(doc, "extension", "name", name) {
        Some(e) => {
            e.insert("dz", length(dz));
        }
        None => {
            let mut e = Table::new();
            e.insert("name", toml_edit::value(name));
            e.insert("dz", length(dz));
            array(doc, "extension").push(e);
        }
    }
}

/// Removes the height extension `name`.
pub fn remove_extension(doc: &mut DocumentMut, name: &str) {
    remove_item(doc, "extension", "name", name);
}

/// The ids of the tables `key` (`section`, `material`).
pub fn item_ids(doc: &DocumentMut, key: &str) -> Vec<String> {
    doc.get(key)
        .and_then(Item::as_array_of_tables)
        .map(|a| {
            a.iter()
                .filter_map(|t| t.get("id").and_then(Item::as_str).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// A new table of `key` (a `[[section]]`, a `[[material]]`) with the
/// `fields` given (text values), its id the next free after `like`; its id.
pub fn add_item(doc: &mut DocumentMut, key: &str, like: &str, fields: &[(&str, &str)]) -> String {
    let ids = item_ids(doc, key);
    let id = fresh(ids.iter().map(String::as_str), like);
    let mut t = Table::new();
    t.insert("id", toml_edit::value(id.clone()));
    for (k, v) in fields {
        t.insert(k, toml_edit::value(*v));
    }
    array(doc, key).push(t);
    id
}

/// Field `field` of the table of `key` with id `id`, as typed: a number,
/// `true`/`false`, else a text (`355 MPa`); empty removes it.
pub fn set_item_field(
    doc: &mut DocumentMut,
    key: &str,
    id: &str,
    field: &str,
    typed: &str,
) -> Result<(), String> {
    let field = field.trim();
    if field.is_empty() || field == "id" {
        return Err("a field name (not id)".into());
    }
    let e = entry(doc, key, "id", id).ok_or_else(|| format!("no {key} {id}"))?;
    let typed = typed.trim();
    if typed.is_empty() {
        e.remove(field);
        return Ok(());
    }
    e.insert(field, typed_value(typed)?);
    Ok(())
}

/// A value as typed: a number, `true`/`false`, else a text (`355 MPa`).
/// Err: a number that is not finite (`nan`, `inf`, `1e400`): refused, not
/// written into the file as TOML `nan`/`inf` (code review 2026-10-26).
fn typed_value(typed: &str) -> Result<Item, String> {
    Ok(if let Ok(v) = typed.parse::<i64>() {
        toml_edit::value(v)
    } else if typed.parse::<f64>().is_ok() {
        toml_edit::value(ohl_core::units::parse_finite(typed)?)
    } else if let Ok(v) = typed.parse::<bool>() {
        toml_edit::value(v)
    } else {
        toml_edit::value(typed.trim_matches('"'))
    })
}

/// Field `field` of the `[analysis]` table (the finite-element check's
/// settings: `stabilise`, `dead_load_factor`…), as typed: a number,
/// `true`/`false`, else a text; empty removes it (and the table when it
/// is left empty).
pub fn set_analysis_field(doc: &mut DocumentMut, field: &str, typed: &str) -> Result<(), String> {
    let field = field.trim();
    if field.is_empty() {
        return Err("a field name".into());
    }
    let typed = typed.trim();
    if typed.is_empty() {
        if let Some(t) = doc.get_mut("analysis").and_then(Item::as_table_mut) {
            t.remove(field);
            if t.is_empty() {
                doc.remove("analysis");
            }
        }
        return Ok(());
    }
    // Checked before anything is changed.
    let value = typed_value(typed)?;
    if doc.get("analysis").and_then(Item::as_table).is_none() {
        doc.insert("analysis", Item::Table(Table::new()));
    }
    let t = doc
        .get_mut("analysis")
        .and_then(Item::as_table_mut)
        .ok_or("[analysis] is not a table")?;
    t.insert(field, value);
    Ok(())
}

/// Removes the table of `key` whose `field` is `value`.
pub fn remove_item(doc: &mut DocumentMut, key: &str, field: &str, value: &str) {
    if let Some(a) = doc.get_mut(key).and_then(Item::as_array_of_tables_mut) {
        a.retain(|t| t.get(field).and_then(Item::as_str) != Some(value));
        if a.is_empty() {
            doc.remove(key);
        }
    }
}

/// The fields of the table of `key` with id `id` (but the id), as text.
pub fn item_fields(doc: &DocumentMut, key: &str, id: &str) -> Vec<(String, String)> {
    doc.get(key)
        .and_then(Item::as_array_of_tables)
        .and_then(|a| {
            a.iter()
                .find(|t| t.get("id").and_then(Item::as_str) == Some(id))
        })
        .map(|t| {
            t.iter()
                .filter(|(k, _)| *k != "id")
                .map(|(k, v)| {
                    let text = match v.as_str() {
                        Some(s) => s.to_owned(),
                        None => v.to_string().trim().to_owned(),
                    };
                    (k.to_owned(), text)
                })
                .collect()
        })
        .unwrap_or_default()
}
