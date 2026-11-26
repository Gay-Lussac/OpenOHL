// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **The joints left, for review** (O44 L31; the owner, 2026-11-26: *"setup
//! ways to study the latest joint and lay them out. The goal is not to reach
//! 100% but not far off so that a human can relatively quickly check the
//! missing joints"*): every joint the automatic layout did not lay out
//! whole, with why (the reasons its record keeps, [`LayoutRecord::hand`]),
//! grouped by reason, and a reviewer's tick per joint stored with the joint.
//!
//! **What is listed.** The joints where two or more members meet, and any
//! other joint the layout record names. *Whole*: laid out by a run and no
//! member there left for a hand detail. Every other joint is *left*: with
//! the reasons the record keeps, or, where it keeps none, in the group
//! [`NO_REASON`] (the layout found nothing it could lay out there, or no run
//! asked it). Nothing here runs the layout: the list reads the file.
//!
//! **The groups.** One reason, one group: [`reason_group`] — the one place
//! the grouping is decided (today from the reasons' words; the layout's own
//! reason classes, when it gives them, replace its body with one line). A
//! joint is in the group of its first reason in the groups' order
//! ([`ReasonGroup::rank`]); its other reasons are listed with it.
//!
//! **The tick.** `reviewed` on the joint's `[[node]]` (structure file
//! version 29, [`ohl_structures::REVIEWED_VERSION`]) holds the stamp of the
//! joint's layout when it was ticked ([`layout_stamps`]: the joint, the
//! members meeting there and their joints' places, its connections, plates
//! and splices, what the layout record says of it). The joint reads as
//! reviewed only while its layout still gives that stamp: any change of
//! the joint's layout (a run, a later edit followed, a value changed by
//! hand, Solve the clashes) clears it, without the layout's code knowing of
//! the tick. A file of before version 29 has none: every joint not
//! reviewed. The tick is the reviewer's mark only: no check takes it, and
//! the program does not lay out or check a joint more for it.

use std::collections::{BTreeMap, BTreeSet};

use ohl_structures::{LibraryItem, StructureType};
use toml_edit::{DocumentMut, Item};

use crate::layout_run::LayoutRecord;

/// The key of the tick on a `[[node]]`.
pub const REVIEWED_KEY: &str = "reviewed";

/// The group of a joint the record keeps no reason for.
pub const NO_REASON: &str =
    "no reason recorded (nothing the layout could lay out there, or no run asked it)";

/// A group of reasons: its place in the list (smaller first) and its name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReasonGroup {
    /// Its place among the groups (smaller first).
    pub rank: u32,
    /// Its name, as the list shows it.
    pub name: String,
}

impl ReasonGroup {
    fn of(rank: u32, name: &str) -> Self {
        Self {
            rank,
            name: name.to_owned(),
        }
    }
}

/// **The group of one reason** the layout recorded for a member left for a
/// hand detail ("D4: its axis runs 5° out of the face of L2's flange it
/// bolts onto"). The one place the grouping is decided: when the layout
/// gives its own reason classes (roadmap O44 L30), this body becomes one
/// line calling them.
pub fn reason_group(why: &str) -> ReasonGroup {
    group_by_words(why)
}

/// The groups by the reasons' words (O44 L31): in the order of the causes
/// the theory counts ([the joint layout, what leaves an end for a hand
/// detail](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#what-leaves-an-end-for-a-hand-detail)):
/// a leg splice, bolts joining nothing, out of the flange's plane, the leg
/// bolted not flat, a built-up member, then room in its kinds.
fn group_by_words(why: &str) -> ReasonGroup {
    let w = why;
    let has = |s: &str| w.contains(s);
    if has("splice") && (has("outer cover") || has(crate::joint_layout::SPLICE_UNKNOWN)) {
        return ReasonGroup::of(
            0,
            "a leg splice in the way (its cover, or its place not known)",
        );
    }
    if has(crate::joint_layout::BOLTS_JOIN_NOTHING) {
        return ReasonGroup::of(1, "its bolts join nothing");
    }
    if has("its axis runs") && has("out of the face") {
        return ReasonGroup::of(2, "an end out of the flange's plane");
    }
    if (has("its bolted leg lies") && has("off the flange"))
        || has("runs towards the angle's other leg")
        || has("a bent plate round that leg does not fit")
        || has(crate::joint_layout::NO_WELD)
    {
        return ReasonGroup::of(3, "a bolted leg not flat on the flange");
    }
    if has("is built up") || has("across the gap between") {
        return ReasonGroup::of(4, "a built-up member to bolt onto");
    }
    if has("(as it is) would then not")
        || has("a bolt of a member left as it is")
        || has("left for a hand detail (its place")
    {
        return ReasonGroup::of(8, "a member left as it is in the way");
    }
    // After the member's name: what the checks after writing found.
    let said = w.split_once(": ").map_or(w, |(_, r)| r);
    if w.starts_with("laid out") || said.starts_with("laid out") {
        return ReasonGroup::of(5, "a clash or a check after writing");
    }
    if has("plate would run into") {
        return ReasonGroup::of(6, "a plate would run into steel");
    }
    if has("no room on one plate") {
        return ReasonGroup::of(9, "no room on one plate");
    }
    if has("pulled back") {
        return ReasonGroup::of(7, "the pull-back or the shift not enough");
    }
    if has("too short for its bolts") {
        return ReasonGroup::of(10, "too short for its bolts");
    }
    ReasonGroup::of(11, "other")
}

/// The group of a joint the record keeps no reason for ([`NO_REASON`]),
/// last.
pub fn no_reason_group() -> ReasonGroup {
    ReasonGroup::of(u32::MAX, NO_REASON)
}

/// A joint's reasons as the record keeps them (one text, the members'
/// reasons joined by "; "), one per member: split at a "; " outside
/// brackets where the next reason begins with a name and ":" ("D4: …") or
/// with "laid out:" (a reason's own "; " stays in it).
pub fn reasons_of(joined: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    let bytes = joined.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth -= 1,
            b';' if depth <= 0 && joined[i..].starts_with("; ") => {
                let rest = &joined[i + 2..];
                let word = rest.split(' ').next().unwrap_or("");
                let named = word.len() > 1 && word.ends_with(':') && !word.contains('(');
                if named || rest.starts_with("laid out:") {
                    out.push(joined[start..i].trim().to_owned());
                    start = i + 2;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let last = joined[start..].trim();
    if !last.is_empty() {
        out.push(last.to_owned());
    }
    out.retain(|r| !r.is_empty());
    out
}

/// A joint left.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewRow {
    /// The joint.
    pub joint: String,
    /// Why it is left, one reason per member (empty: none recorded).
    pub reasons: Vec<String>,
    /// Its group: its first reason's in the groups' order.
    pub group: ReasonGroup,
    /// Something laid out there by a run (some members left).
    pub written: bool,
    /// Ticked reviewed, and its layout unchanged since.
    pub reviewed: bool,
    /// Ticked once, its layout changed since: the tick cleared.
    pub cleared: bool,
    /// The stamp of its layout now (what a tick writes).
    pub stamp: String,
    /// The members meeting there.
    pub members: Vec<String>,
}

/// The joints left after the layout, for review.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ReviewList {
    /// The joints counted: where two or more members meet, and any other
    /// the layout record names.
    pub total: usize,
    /// Of those, laid out whole.
    pub whole: usize,
    /// Every other, in the file's order.
    pub rows: Vec<ReviewRow>,
    /// A layout run is recorded in the file (none: nothing laid out yet,
    /// every joint left).
    pub run: bool,
}

impl ReviewList {
    /// The joints left.
    pub fn left(&self) -> usize {
        self.rows.len()
    }

    /// Of those, ticked reviewed (their layout unchanged since).
    pub fn reviewed(&self) -> usize {
        self.rows.iter().filter(|r| r.reviewed).count()
    }

    /// The row of joint `j`.
    pub fn row(&self, j: &str) -> Option<&ReviewRow> {
        self.rows.iter().find(|r| r.joint == j)
    }

    /// The groups in their order, each with its joints and how many of
    /// them are reviewed.
    pub fn groups(&self) -> Vec<(ReasonGroup, usize, usize)> {
        let mut m: BTreeMap<&ReasonGroup, (usize, usize)> = BTreeMap::new();
        for r in &self.rows {
            let e = m.entry(&r.group).or_default();
            e.0 += 1;
            e.1 += usize::from(r.reviewed);
        }
        m.into_iter().map(|(g, (n, k))| (g.clone(), n, k)).collect()
    }

    /// The rows by group (the groups' order, the file's order within
    /// each).
    pub fn by_group(&self) -> Vec<&ReviewRow> {
        let mut v: Vec<(usize, &ReviewRow)> = self.rows.iter().enumerate().collect();
        v.sort_by(|a, b| a.1.group.cmp(&b.1.group).then(a.0.cmp(&b.0)));
        v.into_iter().map(|(_, r)| r).collect()
    }

    /// The counts in one line: "312 of 400 joints laid out whole, 88 left
    /// (12 reviewed)".
    pub fn summary(&self) -> String {
        format!(
            "{} of {} joints laid out whole, {} left ({} reviewed)",
            self.whole,
            self.total,
            self.left(),
            self.reviewed()
        )
    }

    /// The list as text: the counts, the groups, then each joint left
    /// (by group).
    pub fn text(&self) -> String {
        let mut s = String::new();
        if !self.run {
            s.push_str("No joint layout run recorded in this file: every joint is left.\n");
        }
        s.push_str(&self.summary());
        s.push('\n');
        s.push_str(
            "A joint ticked reviewed is the reviewer's mark only: the program has not laid out \
             or checked it more for it.\n",
        );
        s.push_str("\nBy reason:\n");
        for (g, n, k) in self.groups() {
            s.push_str(&format!("  {n:>6}  ({k} reviewed)  {}\n", g.name));
        }
        s.push_str("\nJoints left (by reason):\n");
        for r in self.by_group() {
            let mark = if r.reviewed {
                "reviewed"
            } else if r.cleared {
                "cleared"
            } else {
                "-"
            };
            s.push_str(&format!("{}\t{}\t{mark}\n", r.joint, r.group.name));
            for w in &r.reasons {
                s.push_str(&format!("\t\t{w}\n"));
            }
        }
        s
    }

    /// The list as CSV (one row per joint left, by group): `joint`,
    /// `group`, `reviewed` (`yes`, `no`, `cleared` — ticked once, its
    /// layout changed since), `laid_out_in_part` (`yes`, `no`), `reasons`
    /// (joined by " | ").
    pub fn csv(&self) -> String {
        let mut s = String::from("joint,group,reviewed,laid_out_in_part,reasons\n");
        for r in self.by_group() {
            let mark = if r.reviewed {
                "yes"
            } else if r.cleared {
                "cleared"
            } else {
                "no"
            };
            s.push_str(&format!(
                "{},{},{mark},{},{}\n",
                csv_field(&r.joint),
                csv_field(&r.group.name),
                if r.written { "yes" } else { "no" },
                csv_field(&r.reasons.join(" | "))
            ));
        }
        s
    }
}

/// A CSV field: quoted where it holds a comma, a quote or a line break.
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}

/// The joints left of the structure file `text`.
pub fn review_list(text: &str) -> Result<ReviewList, String> {
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    review_list_of(&t, text)
}

/// [`review_list`] of a structure as read (`t`) and its text.
pub fn review_list_of(t: &StructureType, text: &str) -> Result<ReviewList, String> {
    let rec = LayoutRecord::of(t);
    let mut members: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for m in &t.members {
        for n in &m.nodes {
            members.entry(n.as_str()).or_default().push(m.id.clone());
        }
    }
    let counted = |id: &str| {
        members.get(id).is_some_and(|v| v.len() >= 2)
            || rec.joints.contains(id)
            || rec.hand.contains_key(id)
    };
    let mut list = ReviewList {
        run: rec.settings.is_some() || !rec.joints.is_empty() || !rec.hand.is_empty(),
        ..Default::default()
    };
    let mut left = Vec::new();
    for n in &t.nodes {
        if !counted(&n.id) {
            continue;
        }
        list.total += 1;
        let whole = rec.joints.contains(&n.id) && !rec.hand.contains_key(&n.id);
        if whole {
            list.whole += 1;
        } else {
            left.push(n);
        }
    }
    let wanted: BTreeSet<String> = left.iter().map(|n| n.id.clone()).collect();
    let stamps = layout_stamps(t, text, &wanted)?;
    for n in left {
        let reasons = rec
            .hand
            .get(&n.id)
            .map(|w| reasons_of(w))
            .unwrap_or_default();
        let group = reasons
            .iter()
            .map(|w| reason_group(w))
            .min()
            .unwrap_or_else(no_reason_group);
        let stamp = stamps.get(&n.id).cloned().unwrap_or_default();
        let given = n.reviewed.as_deref();
        list.rows.push(ReviewRow {
            joint: n.id.clone(),
            reasons,
            group,
            written: rec.joints.contains(&n.id),
            reviewed: given == Some(stamp.as_str()),
            cleared: given.is_some_and(|g| g != stamp),
            stamp,
            members: members.get(n.id.as_str()).cloned().unwrap_or_default(),
        });
    }
    Ok(list)
}

/// The keys of a member that make a joint's layout (its place, section,
/// roll and the data its ends take).
const MEMBER_KEYS: [&str; 8] = [
    "id",
    "nodes",
    "section",
    "roll",
    "reference",
    "orientation",
    "design",
    "kind",
];

/// **The stamp of each joint's layout** (of the joints `wanted`): a hash
/// (FNV-1a, 64 bits, as 16 hexadecimal digits) of the joint as written —
/// its `[[node]]` (without its tick), the members meeting there (their
/// place, section, roll and design data) and their joints' places, its
/// `[[connection]]`s, `[[plate]]`s and `[[splice]]`s, and what the layout
/// record says of it (laid out, its reasons, its automatic values, the
/// plates it made, its provisional bolts). Read from the file's values,
/// not its layout on the page: the same values give the same stamp
/// (whatever their order of keys or spacing); any change of them another.
pub fn layout_stamps(
    t: &StructureType,
    text: &str,
    wanted: &BTreeSet<String>,
) -> Result<BTreeMap<String, String>, String> {
    let table: toml::Table = text.parse().map_err(|e: toml::de::Error| e.to_string())?;
    let array = |k: &str| -> Vec<&toml::Table> {
        table
            .get(k)
            .and_then(toml::Value::as_array)
            .map(|a| a.iter().filter_map(toml::Value::as_table).collect())
            .unwrap_or_default()
    };
    let id_of =
        |x: &toml::Table, k: &str| x.get(k).and_then(toml::Value::as_str).map(str::to_owned);
    let nodes: BTreeMap<String, &toml::Table> = array("node")
        .into_iter()
        .filter_map(|n| id_of(n, "id").map(|i| (i, n)))
        .collect();
    let mut parts: BTreeMap<String, Vec<String>> =
        wanted.iter().map(|j| (j.clone(), Vec::new())).collect();
    let mut put = |j: &str, s: String| {
        if let Some(v) = parts.get_mut(j) {
            v.push(s);
        }
    };
    for (id, n) in &nodes {
        if wanted.contains(id) {
            let mut n = (*n).clone();
            n.remove(REVIEWED_KEY);
            put(id, format!("node {}", canon_table(&n)));
        }
    }
    for m in array("member") {
        let ends: Vec<String> = m
            .get("nodes")
            .and_then(toml::Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        if !ends.iter().any(|e| wanted.contains(e)) {
            continue;
        }
        let mut kept = toml::Table::new();
        for k in MEMBER_KEYS {
            if let Some(v) = m.get(k) {
                kept.insert(k.to_owned(), v.clone());
            }
        }
        let mut s = format!("member {}", canon_table(&kept));
        for e in &ends {
            let p = nodes.get(e).and_then(|n| n.get("point"));
            s.push_str(&format!(" at {}", p.map_or_else(String::new, canon)));
        }
        for e in &ends {
            put(e, s.clone());
        }
    }
    for k in ["connection", "plate", "splice"] {
        for x in array(k) {
            if let Some(j) = id_of(x, "joint") {
                put(&j, format!("{k} {}", canon_table(x)));
            }
        }
    }
    let rec = LayoutRecord::of(t);
    for j in wanted {
        put(j, format!("laid out {}", rec.joints.contains(j)));
        if let Some(w) = rec.hand.get(j) {
            put(j, format!("hand {w:?}"));
        }
    }
    for ((m, j), f) in &rec.ends {
        put(j, format!("end {m:?} {f:?}"));
    }
    for (id, (j, f)) in &rec.made {
        put(j, format!("made {id:?} {f:?}"));
    }
    for (m, j) in &rec.provisional {
        put(j, format!("provisional {m:?}"));
    }
    Ok(parts
        .into_iter()
        .map(|(j, mut v)| {
            v.sort();
            (j, format!("{:016x}", fnv1a(v.join("\n").as_bytes())))
        })
        .collect())
}

/// FNV-1a, 64 bits (the same on every machine and build).
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// A table's values in one canonical line (keys sorted).
fn canon_table(t: &toml::Table) -> String {
    let mut keys: Vec<&String> = t.keys().collect();
    keys.sort();
    let inner: Vec<String> = keys
        .into_iter()
        .map(|k| format!("{k:?}={}", canon(&t[k])))
        .collect();
    format!("{{{}}}", inner.join(","))
}

/// A value in one canonical line.
fn canon(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => format!("{s:?}"),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => format!("{f:?}"),
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Datetime(d) => d.to_string(),
        toml::Value::Array(a) => {
            let inner: Vec<String> = a.iter().map(canon).collect();
            format!("[{}]", inner.join(","))
        }
        toml::Value::Table(t) => canon_table(t),
    }
}

/// **Ticks or clears joints' review ticks** in the file `doc`: each joint
/// with the stamp to write (its layout's now, [`ReviewRow::stamp`]) or
/// none (the tick taken off). The file raised to version 29 when a tick is
/// written. A joint not in the file is refused, nothing written. Returns
/// how many ticks changed.
pub fn set_reviewed(
    doc: &mut DocumentMut,
    ticks: &[(String, Option<String>)],
) -> Result<usize, String> {
    let wanted: BTreeMap<&str, Option<&str>> = ticks
        .iter()
        .map(|(j, s)| (j.as_str(), s.as_deref()))
        .collect();
    let Some(nodes) = doc.get_mut("node").and_then(Item::as_array_of_tables_mut) else {
        return Err("the file has no joints".into());
    };
    let found: BTreeSet<String> = nodes
        .iter()
        .filter_map(|n| n.get("id").and_then(Item::as_str).map(str::to_owned))
        .collect();
    if let Some(j) = wanted.keys().find(|j| !found.contains(**j)) {
        return Err(format!("joint {j} is not in the file"));
    }
    let mut changed = 0;
    let mut written = false;
    for n in nodes.iter_mut() {
        let Some(id) = n.get("id").and_then(Item::as_str).map(str::to_owned) else {
            continue;
        };
        let Some(stamp) = wanted.get(id.as_str()) else {
            continue;
        };
        let now = n
            .get(REVIEWED_KEY)
            .and_then(Item::as_str)
            .map(str::to_owned);
        match stamp {
            Some(s) => {
                if now.as_deref() != Some(*s) {
                    n.insert(REVIEWED_KEY, toml_edit::value(*s));
                    changed += 1;
                }
                written = true;
            }
            None => {
                if n.remove(REVIEWED_KEY).is_some() {
                    changed += 1;
                }
            }
        }
    }
    if written {
        ohl_structure_edit::edits::raise_version(doc, i64::from(ohl_structures::REVIEWED_VERSION));
    }
    Ok(changed)
}
