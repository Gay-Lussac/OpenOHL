// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Apply this joint's detail to similar joints** (the automatic joint
//! layout's J4, 2026-11-19): one joint's detail — every member end's
//! connection there and the joint's plates — copied, mirrored or turned,
//! onto the joints of its kind ([`crate::joint_kinds::like_joint`]: the
//! same members, sections, rolls and classes, the tower's symmetry or a
//! repeated panel). Laid out by the layout or detailed by hand alike.
//!
//! Per joint like it: its own connections at the joint and its plates are
//! replaced by the copy — what an end bolts onto said with its own members
//! and the copied plates, the plates' outlines, bend lines mapped there,
//! the lengths as they are (a cut, a shift towards what the end bolts
//! onto, the bolts along and across a leg: the same on a mirrored or turned
//! joint) — and the layout's record follows (an automatic value copied
//! stays automatic, one given by hand stays the user's; the joint laid
//! out, or left for a hand detail with the same words, as the source).
//! Each copy is checked for clashes in its own place: a joint where the
//! copy runs into more than the source does is left as it was, said. A
//! joint with a splice (at the source or there) is left: its covers are
//! not copied (by hand).

use std::collections::{BTreeMap, BTreeSet};

use ohl_sections::SectionLibrary;
use ohl_structures::bolts::BoltLibrary;
use ohl_structures::{LibraryItem, StructureType};
use toml_edit::{Array, DocumentMut, Item, Table, Value};

use crate::joint_kinds::{Like, like_joint};
use crate::layout_run::LayoutRecord;

/// One joint the detail is applied to (or would be).
#[derive(Debug, Clone, PartialEq)]
pub struct Applied {
    /// The joint.
    pub joint: String,
    /// How it is like the source ("mirrored across x"…).
    pub how: String,
    /// Its member ends given the source's connections.
    pub ends: usize,
    /// Its plates replaced (before) and made (after).
    pub plates: (usize, usize),
}

/// What applying a joint's detail does.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ApplyRun {
    /// The file after (the same text when nothing changed).
    pub text: String,
    /// The joints given the detail, in the file's order.
    pub applied: Vec<Applied>,
    /// The joints like it left as they were, with why.
    pub left: Vec<(String, String)>,
    /// How long it took, s.
    pub seconds: f64,
}

/// Joint `joint`'s detail applied to the joints like it in `text` (only
/// those of `only` when given).
pub fn apply_detail(
    text: &str,
    library: &SectionLibrary,
    bolts: &BoltLibrary,
    joint: &str,
    only: Option<&BTreeSet<String>>,
) -> Result<ApplyRun, String> {
    let start = std::time::Instant::now();
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    let d = crate::joints::joint_details(text, library, bolts)?;
    let source = d
        .joint(joint)
        .ok_or_else(|| format!("no joint {joint} in detail"))?;
    let mut left = Vec::new();
    let mut likes: Vec<Like> = Vec::new();
    for l in like_joint(&d, joint) {
        if only.is_some_and(|o| !o.contains(&l.joint)) {
            continue;
        }
        if !source.splices.is_empty() {
            left.push((
                l.joint.clone(),
                format!("{joint} has a splice: its covers are not copied (by hand)"),
            ));
            continue;
        }
        if d.joint(&l.joint).is_some_and(|j| !j.splices.is_empty()) {
            left.push((
                l.joint.clone(),
                "a splice there: not replaced (by hand)".to_owned(),
            ));
            continue;
        }
        likes.push(l);
    }
    // The source's clashes: a copy may have as many in its place, no more.
    let clashes_at =
        |d: &crate::joints::JointDetails, j: &str| d.joint(j).map_or(0, |x| x.clashes.len());
    let own = clashes_at(&d, joint);
    // Written to all, checked, then written again to those that fit.
    let mut doc = write(text, &t, joint, &likes)?;
    let after = crate::joints::joint_details_at(
        &doc.to_string(),
        library,
        bolts,
        Some(&likes.iter().map(|l| l.joint.clone()).collect()),
    )?;
    let fits: Vec<Like> = likes
        .iter()
        .filter(|l| {
            let n = clashes_at(&after, &l.joint);
            if n > own {
                left.push((
                    l.joint.clone(),
                    format!(
                        "the copy would run into something there ({n} clash(es), {own} at {joint}): \
                         left as it was"
                    ),
                ));
                false
            } else {
                true
            }
        })
        .cloned()
        .collect();
    if fits.len() != likes.len() {
        doc = write(text, &t, joint, &fits)?;
    }
    let applied = fits
        .iter()
        .map(|l| Applied {
            joint: l.joint.clone(),
            how: l.map.name.to_owned(),
            ends: l
                .pairs
                .iter()
                .filter(|(a, _)| t.connection(a, joint).is_some())
                .count(),
            plates: (
                t.plates.iter().filter(|p| p.joint == l.joint).count(),
                t.plates.iter().filter(|p| p.joint == joint).count(),
            ),
        })
        .collect();
    let order: BTreeMap<&str, usize> = t
        .nodes
        .iter()
        .enumerate()
        .map(|(k, n)| (n.id.as_str(), k))
        .collect();
    left.sort_by_key(|(j, _)| order.get(j.as_str()).copied().unwrap_or(usize::MAX));
    let text_after = if fits.is_empty() {
        text.to_owned()
    } else {
        doc.to_string()
    };
    Ok(ApplyRun {
        text: text_after,
        applied,
        left,
        seconds: start.elapsed().as_secs_f64(),
    })
}

/// `text` with joint `joint`'s detail written onto each of `likes`.
fn write(
    text: &str,
    t: &StructureType,
    joint: &str,
    likes: &[Like],
) -> Result<DocumentMut, String> {
    let mut doc: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    let mut rec = LayoutRecord::of(t);
    let mut taken: BTreeSet<String> = t
        .plates
        .iter()
        .map(|p| p.id.clone())
        .chain(t.members.iter().map(|m| m.id.clone()))
        .collect();
    let source_plates: Vec<&ohl_structures::connections::TypePlate> =
        t.plates.iter().filter(|p| p.joint == joint).collect();
    for l in likes {
        let here: BTreeSet<&str> = l.pairs.iter().map(|(_, b)| b.as_str()).collect();
        // Its own detail out: its plates, its members' connections there,
        // the record's marks of them.
        // (Their ids free again: applied again, the same ids.)
        for p in t.plates.iter().filter(|p| p.joint == l.joint) {
            ohl_structure_edit::connection_edits::remove_plate(&mut doc, &p.id);
            rec.made.remove(&p.id);
            taken.remove(&p.id);
        }
        for m in &here {
            ohl_structure_edit::connection_edits::clear_connection(&mut doc, m, &l.joint);
            rec.ends.remove(&((*m).to_owned(), l.joint.clone()));
            rec.provisional.remove(&((*m).to_owned(), l.joint.clone()));
        }
        rec.joints.remove(&l.joint);
        rec.hand.remove(&l.joint);
        // The source's plates, new ids here.
        let mut plate_of: BTreeMap<String, String> = BTreeMap::new();
        for p in &source_plates {
            let id = ohl_structure_edit::edits::next_id(taken.iter().map(String::as_str), &p.id);
            taken.insert(id.clone());
            plate_of.insert(p.id.clone(), id);
        }
        let onto_here = |o: &str| -> Option<String> {
            l.member(o)
                .map(str::to_owned)
                .or_else(|| plate_of.get(o).cloned())
        };
        for p in &source_plates {
            let Some(mut x) = table_of(&doc, "plate", |x| {
                x.get("id").and_then(Item::as_str) == Some(p.id.as_str())
            }) else {
                continue;
            };
            x.insert("id", toml_edit::value(plate_of[&p.id].as_str()));
            x.insert("joint", toml_edit::value(l.joint.as_str()));
            if let Some(a) = x.get_mut("welded_to").and_then(Item::as_array_mut) {
                let ids: Vec<String> = a
                    .iter()
                    .filter_map(|v| v.as_str().and_then(onto_here))
                    .collect();
                *a = ids.iter().map(String::as_str).collect();
            }
            if let Some(ws) = x.get_mut("weld").and_then(Item::as_array_of_tables_mut) {
                for w in ws.iter_mut() {
                    if let Some(to) = w.get("to").and_then(Item::as_str).and_then(onto_here) {
                        w.insert("to", toml_edit::value(to));
                    }
                }
            }
            if let Some(b) = x.get_mut("bend").and_then(Item::as_table_mut) {
                if let Some(to) = b.get("to").and_then(Item::as_str).and_then(onto_here) {
                    b.insert("to", toml_edit::value(to));
                }
                if let Some(bend) = p.bend.as_ref().filter(|b| !b.line.is_empty()) {
                    let line: Vec<[f64; 3]> = bend
                        .line
                        .iter()
                        .map(|q| l.point(q.map(|v| v.si())))
                        .collect();
                    b.insert("line", points(&line));
                }
            }
            if let Some(h) = x.get_mut("leg_bolts").and_then(Item::as_table_like_mut)
                && let Some(m) = h.get("member").and_then(Item::as_str).and_then(onto_here)
            {
                h.insert("member", toml_edit::value(m));
            }
            if !p.outline.is_empty() {
                let o: Vec<[f64; 3]> = p
                    .outline
                    .iter()
                    .map(|q| l.point(q.map(|v| v.si())))
                    .collect();
                x.insert("outline", points(&o));
            }
            push(&mut doc, "plate", x);
            if let Some((_, fields)) = rec.made.get(&p.id).cloned() {
                rec.made
                    .insert(plate_of[&p.id].clone(), (l.joint.clone(), fields));
            }
        }
        // The source's connections at the joint, onto its members and plates
        // here.
        for (a, b) in &l.pairs {
            let Some(mut x) = table_of(&doc, "connection", |x| {
                x.get("member").and_then(Item::as_str) == Some(a.as_str())
                    && x.get("joint").and_then(Item::as_str) == Some(joint)
            }) else {
                continue;
            };
            x.insert("member", toml_edit::value(b.as_str()));
            x.insert("joint", toml_edit::value(l.joint.as_str()));
            for key in ["onto", "other_onto"] {
                if let Some(o) = x.get(key).and_then(Item::as_str).and_then(onto_here) {
                    x.insert(key, toml_edit::value(o));
                }
            }
            push(&mut doc, "connection", x);
            if let Some(f) = rec.ends.get(&(a.clone(), joint.to_owned())).cloned() {
                rec.ends.insert((b.clone(), l.joint.clone()), f);
            }
            if rec.provisional.contains(&(a.clone(), joint.to_owned())) {
                rec.provisional.insert((b.clone(), l.joint.clone()));
            }
        }
        if rec.joints.contains(joint) {
            rec.joints.insert(l.joint.clone());
        }
        if let Some(why) = rec.hand.get(joint).cloned() {
            rec.hand.insert(l.joint.clone(), l.words(&why));
        }
    }
    if rec != LayoutRecord::of(t) {
        rec.write(&mut doc);
    }
    Ok(doc)
}

/// A copy of the first table of array `key` that `pick` takes.
fn table_of(doc: &DocumentMut, key: &str, pick: impl Fn(&Table) -> bool) -> Option<Table> {
    doc.get(key)
        .and_then(Item::as_array_of_tables)
        .and_then(|a| a.iter().find(|x| pick(x)).cloned())
}

/// `x` added to the array of tables `key`.
fn push(doc: &mut DocumentMut, key: &str, x: Table) {
    if doc.get(key).and_then(Item::as_array_of_tables).is_none() {
        doc.insert(key, Item::ArrayOfTables(toml_edit::ArrayOfTables::new()));
    }
    if let Some(a) = doc.get_mut(key).and_then(Item::as_array_of_tables_mut) {
        a.push(x);
    }
}

/// Points as the files write them (metres to a tenth of a millimetre).
fn points(pts: &[[f64; 3]]) -> Item {
    let mut a = Array::new();
    for p in pts {
        let mut q = Array::new();
        for v in p {
            q.push(format!("{v:.4} m"));
        }
        a.push(Value::Array(q));
    }
    Item::Value(Value::Array(a))
}
