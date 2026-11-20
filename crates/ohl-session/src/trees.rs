// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Split trees** (country server, phase 11; ADR-0013 "Versions at the
//! scale of a country"): a revision of a big model that neither hashes nor
//! stores the whole line.
//!
//! A flat tree ([`crate::versions`], schema 2) lists every object of a
//! state in one blob: a commit hashes every object and writes that list
//! again, a diff reads it whole, a merge reads every object of three
//! states. A **split tree** (tree version 2) lists the objects in a trie
//! keyed by the SHA-256 of each object's kind and id (its *key hash*):
//!
//! - a **node** holding at most [`LEAF`] objects is a leaf (each object's
//!   kind, id, place among its kind and value hash, by key hash);
//! - a node holding more is split into up to 16 children by the next
//!   hexadecimal digit of the key hash (down to [`DEPTH`] digits);
//! - the **root** names the trie's top node and lists the library files
//!   (kind, name, text hash) — a few, as the flat tree did.
//!
//! The trie is **deterministic**: it depends on the objects only (their
//! kinds, ids, places and contents), never on the order they came in, on
//! geometry or on settings — the server, a replica and an offline copy
//! make the same tree and so the same revision id. The unit is the
//! object's key, not its route or tension section: an object's routes
//! change with edits and a structure may stand on several routes, so a
//! trie by route would move objects between subtrees and could not be
//! made from the objects alone. What a route's diff or merge reads is what
//! changed in it: the nodes whose hashes differ, and the objects in them
//! that differ.
//!
//! **A commit costs what changed**: the session keeps per project line a
//! `TreeIndex` — each object's value hash with the JSON it was taken
//! from (shared, never copied) and the hashes of the trie's nodes. A
//! commit compares the line's objects with it by their shared JSON (a
//! pointer each), hashes the objects changed since, makes again the nodes
//! on their paths, and writes only the nodes and objects that are not in
//! the parent revision's tree (read along the paths that differ). The
//! first commit of a line after the model was opened hashes every object
//! once (as a flat tree does at every commit).
//!
//! **Where**: a store says whether it keeps split trees
//! ([`crate::store::Store::split_trees`]): PostgreSQL does (a network's
//! models), a project file does not — **the desktop's `.ohl` file is
//! written as before** (flat trees, the same bytes). Both kinds are read
//! everywhere: a model exported from a network keeps its split trees (its
//! revision ids are kept), and a model's flat revisions of before (its
//! import) stay its history, compared with split ones object by object.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::versions::canonical;
use crate::{Kind, Object, SessionError};

/// A SHA-256 hash.
pub(crate) type H = [u8; 32];

/// Most objects in a leaf (more: the node is split).
pub const LEAF: usize = 64;
/// Deepest node (hexadecimal digits of the key hash): a leaf whatever
/// its size (two keys sharing 16 digits do not occur in practice).
pub const DEPTH: usize = 16;

/// The tree version of split trees (a flat tree has no version field).
pub const VERSION: u64 = 2;

/// An object as a tree lists it: kind, id, place among its kind, value
/// hash.
pub(crate) type Entry = (Kind, String, i64, H);

/// Objects that differ between two trees: (as in the first, as in the
/// second), each its place and value hash.
pub(crate) type Changed = BTreeMap<(Kind, String), (Option<(i64, H)>, Option<(i64, H)>)>;

/// Hexadecimal.
pub(crate) fn hex(h: &H) -> String {
    h.iter().map(|b| format!("{b:02x}")).collect()
}

/// A hash from its hexadecimal (`None`: not one).
pub(crate) fn unhex(s: &str) -> Option<H> {
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (k, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(s.get(2 * k..2 * k + 2)?, 16).ok()?;
    }
    Some(out)
}

/// SHA-256 of a text.
pub(crate) fn sha(text: &str) -> H {
    Sha256::digest(text.as_bytes()).into()
}

/// The key hash of object `kind`/`id`.
pub(crate) fn key_hash(kind: Kind, id: &str) -> H {
    sha(&format!("{}\n{id}", kind.as_str()))
}

fn nibble(h: &H, k: usize) -> u8 {
    let b = h[k / 2];
    if k.is_multiple_of(2) { b >> 4 } else { b & 15 }
}

/// A node's place in the trie: the first `len` hexadecimal digits of the
/// key hashes under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Prefix {
    len: u8,
    digits: u64,
}

impl Prefix {
    /// The top node.
    pub(crate) const ROOT: Prefix = Prefix { len: 0, digits: 0 };

    fn child(self, d: u8) -> Prefix {
        // Below the deepest level only a damaged tree goes (refused when
        // its node is read): the digits are not kept further.
        if self.deepest() {
            return Prefix {
                len: self.len.saturating_add(1),
                digits: self.digits,
            };
        }
        Prefix {
            len: self.len + 1,
            digits: self.digits | (u64::from(d & 15) << (60 - 4 * u32::from(self.len))),
        }
    }

    fn holds(self, h: &H) -> bool {
        (0..usize::from(self.len)).all(|k| nibble(h, k) == self.digit(k))
    }

    fn digit(self, k: usize) -> u8 {
        ((self.digits >> (60 - 4 * k)) & 15) as u8
    }

    /// The smallest key hash under it.
    fn low(self) -> H {
        let mut out = [0u8; 32];
        for k in 0..usize::from(self.len) {
            let d = self.digit(k);
            out[k / 2] |= if k % 2 == 0 { d << 4 } else { d };
        }
        out
    }

    fn deepest(self) -> bool {
        usize::from(self.len) >= DEPTH
    }
}

/// A node of a split tree.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Node {
    /// Its objects, by key hash.
    Leaf(Vec<Entry>),
    /// Its children by digit (`None`: nothing under that digit).
    Inner(Box<[Option<H>; 16]>),
}

/// A leaf's text.
fn leaf_text<'a>(entries: impl Iterator<Item = (Kind, &'a str, i64, &'a H)>) -> String {
    let list: Vec<Value> = entries
        .map(|(k, id, ord, h)| serde_json::json!([k, id, ord, hex(h)]))
        .collect();
    canonical(&serde_json::json!({ "e": list }))
}

/// An inner node's text.
fn inner_text(children: &[Option<H>; 16]) -> String {
    let list: Vec<Value> = children
        .iter()
        .enumerate()
        .filter_map(|(d, h)| h.map(|h| serde_json::json!([format!("{d:x}"), hex(&h)])))
        .collect();
    canonical(&serde_json::json!({ "c": list }))
}

/// A root's text: the top node (`None`: no object) and the library files.
pub(crate) fn root_text(objects: Option<&H>, libraries: &BTreeMap<(String, String), H>) -> String {
    let libs: Vec<Value> = libraries
        .iter()
        .map(|((k, n), h)| serde_json::json!([k, n, hex(h)]))
        .collect();
    canonical(&serde_json::json!({
        "v": VERSION,
        "objects": objects.map(hex),
        "libraries": libs,
    }))
}

fn bad(what: &str) -> SessionError {
    SessionError::Store(format!("version store: {what}"))
}

/// Reads a node's text.
pub(crate) fn parse_node(text: &str) -> Result<Node, SessionError> {
    let v: Value = serde_json::from_str(text).map_err(|e| bad(&format!("a tree node: {e}")))?;
    if let Some(list) = v.get("e").and_then(Value::as_array) {
        let mut out = Vec::with_capacity(list.len());
        for x in list {
            let (kind, id, ord, h): (Kind, String, i64, String) =
                serde_json::from_value(x.clone()).map_err(|e| bad(&format!("a tree leaf: {e}")))?;
            out.push((
                kind,
                id,
                ord,
                unhex(&h).ok_or_else(|| bad("a tree leaf's hash"))?,
            ));
        }
        return Ok(Node::Leaf(out));
    }
    if let Some(list) = v.get("c").and_then(Value::as_array) {
        let mut out: Box<[Option<H>; 16]> = Box::new([None; 16]);
        for x in list {
            let (d, h): (String, String) =
                serde_json::from_value(x.clone()).map_err(|e| bad(&format!("a tree node: {e}")))?;
            let d = u8::from_str_radix(&d, 16)
                .ok()
                .filter(|d| *d < 16)
                .ok_or_else(|| bad("a tree node's digit"))?;
            out[usize::from(d)] = Some(unhex(&h).ok_or_else(|| bad("a tree node's hash"))?);
        }
        return Ok(Node::Inner(out));
    }
    Err(bad("a tree node of no known form"))
}

/// A revision's tree as stored.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TreeBlob {
    /// Every object listed (schema 2's form).
    Flat {
        objects: Vec<Entry>,
        libraries: BTreeMap<(String, String), H>,
    },
    /// The trie's top node and the library files.
    Split {
        objects: Option<H>,
        libraries: BTreeMap<(String, String), H>,
    },
}

impl TreeBlob {
    /// Reads a tree's text (flat or split).
    pub(crate) fn parse(text: &str) -> Result<Self, SessionError> {
        let v: Value = serde_json::from_str(text).map_err(|e| bad(&format!("a tree: {e}")))?;
        let libraries = |v: &Value| -> Result<BTreeMap<(String, String), H>, SessionError> {
            let list: Vec<(String, String, String)> =
                serde_json::from_value(v.get("libraries").cloned().unwrap_or(Value::Null))
                    .map_err(|e| bad(&format!("a tree's library files: {e}")))?;
            list.into_iter()
                .map(|(k, n, h)| Ok(((k, n), unhex(&h).ok_or_else(|| bad("a file's hash"))?)))
                .collect()
        };
        match v.get("v").and_then(Value::as_u64) {
            None => {
                let list: Vec<(Kind, String, i64, String)> =
                    serde_json::from_value(v.get("objects").cloned().unwrap_or(Value::Null))
                        .map_err(|e| bad(&format!("a tree's objects: {e}")))?;
                let objects = list
                    .into_iter()
                    .map(|(k, id, ord, h)| {
                        Ok((
                            k,
                            id,
                            ord,
                            unhex(&h).ok_or_else(|| bad("an object's hash"))?,
                        ))
                    })
                    .collect::<Result<Vec<_>, SessionError>>()?;
                Ok(TreeBlob::Flat {
                    objects,
                    libraries: libraries(&v)?,
                })
            }
            Some(VERSION) => {
                let objects = match v.get("objects") {
                    Some(Value::String(h)) => {
                        Some(unhex(h).ok_or_else(|| bad("a tree's top node"))?)
                    }
                    Some(Value::Null) | None => None,
                    Some(_) => return Err(bad("a tree's top node")),
                };
                Ok(TreeBlob::Split {
                    objects,
                    libraries: libraries(&v)?,
                })
            }
            Some(n) => Err(bad(&format!(
                "a tree of version {n} (newer than this program reads)"
            ))),
        }
    }

    /// Its library files.
    pub(crate) fn libraries(&self) -> &BTreeMap<(String, String), H> {
        match self {
            TreeBlob::Flat { libraries, .. } | TreeBlob::Split { libraries, .. } => libraries,
        }
    }
}

/// Where a trie's nodes come from: the version store, or a line's
/// [`TreeIndex`].
pub(crate) trait Src {
    /// The node of hash `h` at `p`.
    fn node(&mut self, p: Prefix, h: &H) -> Result<Node, SessionError>;
}

/// Every object under the node of hash `h` at `p`.
pub(crate) fn entries_under(
    src: &mut dyn Src,
    p: Prefix,
    h: &H,
    out: &mut Vec<Entry>,
) -> Result<(), SessionError> {
    match src.node(p, h)? {
        Node::Leaf(list) => out.extend(list),
        Node::Inner(children) => {
            for (d, c) in children.iter().enumerate() {
                if let Some(c) = c {
                    entries_under(src, p.child(d as u8), c, out)?;
                }
            }
        }
    }
    Ok(())
}

/// The objects that differ between the tries under `ha` (of `a`) and
/// `hb` (of `b`) at `p`: only the nodes whose hashes differ are read.
pub(crate) fn diff_nodes(
    a: &mut dyn Src,
    b: &mut dyn Src,
    p: Prefix,
    ha: Option<&H>,
    hb: Option<&H>,
    out: &mut Changed,
) -> Result<(), SessionError> {
    if ha == hb {
        return Ok(());
    }
    let na = ha.map(|h| a.node(p, h)).transpose()?;
    let nb = hb.map(|h| b.node(p, h)).transpose()?;
    if let (Some(Node::Inner(ca)), Some(Node::Inner(cb))) = (&na, &nb) {
        for d in 0..16u8 {
            diff_nodes(
                a,
                b,
                p.child(d),
                ca[usize::from(d)].as_ref(),
                cb[usize::from(d)].as_ref(),
                out,
            )?;
        }
        return Ok(());
    }
    let ea = node_entries(a, p, na)?;
    let eb = node_entries(b, p, nb)?;
    compare_entries(ea, eb, out);
    Ok(())
}

/// Every object under `node` (at `p`) of `src`.
fn node_entries(
    src: &mut dyn Src,
    p: Prefix,
    node: Option<Node>,
) -> Result<Vec<Entry>, SessionError> {
    let mut list = Vec::new();
    match node {
        None => {}
        Some(Node::Leaf(l)) => list = l,
        Some(Node::Inner(children)) => {
            for (d, c) in children.iter().enumerate() {
                if let Some(c) = c {
                    entries_under(src, p.child(d as u8), c, &mut list)?;
                }
            }
        }
    }
    Ok(list)
}

/// Two lists of objects compared (each object once in each).
pub(crate) fn compare_entries(a: Vec<Entry>, b: Vec<Entry>, out: &mut Changed) {
    let mut a: BTreeMap<(Kind, String), (i64, H)> = a
        .into_iter()
        .map(|(k, id, o, h)| ((k, id), (o, h)))
        .collect();
    for (k, id, o, h) in b {
        let key = (k, id);
        match a.remove(&key) {
            Some(x) if x == (o, h) => {}
            x => {
                out.insert(key, (x, Some((o, h))));
            }
        }
    }
    for (key, x) in a {
        out.insert(key, (Some(x), None));
    }
}

/// A trie's nodes read from the version store, each once.
pub(crate) struct Stored<'a> {
    read: &'a dyn Fn(&str) -> Result<String, SessionError>,
    cache: HashMap<H, Node>,
}

impl<'a> Stored<'a> {
    /// Reading blobs with `read` (by hexadecimal hash).
    pub(crate) fn new(read: &'a dyn Fn(&str) -> Result<String, SessionError>) -> Self {
        Self {
            read,
            cache: HashMap::new(),
        }
    }
}

impl Src for Stored<'_> {
    fn node(&mut self, p: Prefix, h: &H) -> Result<Node, SessionError> {
        if usize::from(p.len) > DEPTH {
            return Err(bad("a tree deeper than split trees are"));
        }
        if let Some(n) = self.cache.get(h) {
            return Ok(n.clone());
        }
        let n = parse_node(&(self.read)(&hex(h))?)?;
        self.cache.insert(*h, n.clone());
        Ok(n)
    }
}

/// What a tree keeps of one object: its JSON as hashed, its place, its
/// value and key hashes.
type Memo = (Arc<Value>, i64, H, H);

/// What the session keeps of a project line's tree: each object's value
/// hash (with the JSON it was taken from: shared, never copied) and the
/// hashes of the trie's nodes, so that the next tree costs what changed.
#[derive(Debug, Default)]
pub(crate) struct TreeIndex {
    /// Per object: its JSON as hashed, its place, its value and key hashes.
    memo: HashMap<(Kind, String), Memo>,
    /// Every object by key hash: place and value hash.
    entries: BTreeMap<(H, Kind, String), (i64, H)>,
    /// The nodes' hashes known (none for a node under which something
    /// changed since).
    nodes: HashMap<Prefix, H>,
    /// Library files: their object's JSON and the text's hash.
    libraries: BTreeMap<(String, String), (Arc<Value>, H)>,
}

impl TreeIndex {
    /// Brings the index to `objects` (a project line's): the objects whose
    /// JSON is not the one hashed (a pointer compared) are hashed again,
    /// the nodes on their paths forgotten.
    pub(crate) fn update(&mut self, objects: &BTreeMap<(Kind, String), Object>) {
        let mut libraries: BTreeMap<(String, String), (Arc<Value>, H)> = BTreeMap::new();
        let mut count = 0usize;
        for (key, o) in objects {
            if key.0 == Kind::Library {
                let Some((kind, name, text)) = crate::library_entry(&o.value) else {
                    continue;
                };
                let h = match self.libraries.get(&(kind.clone(), name.clone())) {
                    Some((v, h)) if Arc::ptr_eq(v, &o.value) => *h,
                    _ => sha(&text),
                };
                libraries.insert((kind, name), (Arc::clone(&o.value), h));
                continue;
            }
            count += 1;
            if let Some((v, ord, _, _)) = self.memo.get(key)
                && Arc::ptr_eq(v, &o.value)
                && *ord == o.ord
            {
                continue;
            }
            let value_hash = match self.memo.get(key) {
                Some((v, _, h, _)) if Arc::ptr_eq(v, &o.value) => *h,
                _ => sha(&canonical(&o.value)),
            };
            let kh = key_hash(key.0, &key.1);
            self.forget(&kh);
            self.entries
                .insert((kh, key.0, key.1.clone()), (o.ord, value_hash));
            self.memo
                .insert(key.clone(), (Arc::clone(&o.value), o.ord, value_hash, kh));
        }
        if self.memo.len() > count {
            let gone: Vec<(Kind, String)> = self
                .memo
                .keys()
                .filter(|k| !objects.contains_key(*k))
                .cloned()
                .collect();
            for key in gone {
                if let Some((_, _, _, kh)) = self.memo.remove(&key) {
                    self.forget(&kh);
                    self.entries.remove(&(kh, key.0, key.1));
                }
            }
        }
        self.libraries = libraries;
    }

    /// The nodes on the path of key hash `kh` forgotten.
    fn forget(&mut self, kh: &H) {
        let mut p = Prefix::ROOT;
        loop {
            self.nodes.remove(&p);
            if p.deepest() {
                break;
            }
            p = p.child(nibble(kh, usize::from(p.len)));
        }
    }

    /// The objects under `p`, by key hash (at most `limit`).
    fn under(&self, p: Prefix, limit: usize) -> impl Iterator<Item = Entry> + '_ {
        self.entries
            .range((p.low(), Kind::Meta, String::new())..)
            .take_while(move |((kh, _, _), _)| p.holds(kh))
            .take(limit)
            .map(|((_, k, id), (o, h))| (*k, id.clone(), *o, *h))
    }

    fn is_leaf(&self, p: Prefix) -> bool {
        p.deepest() || self.under(p, LEAF + 1).count() <= LEAF
    }

    /// The hash of the node at `p` (`None`: nothing under it), made again
    /// where forgotten.
    fn node_hash(&mut self, p: Prefix) -> Option<H> {
        if let Some(h) = self.nodes.get(&p) {
            return Some(*h);
        }
        let text = self.node_text(p)?;
        let h = sha(&text);
        self.nodes.insert(p, h);
        Some(h)
    }

    /// The text of the node at `p` (its children's hashes made first).
    fn node_text(&mut self, p: Prefix) -> Option<String> {
        self.under(p, 1).next()?;
        if self.is_leaf(p) {
            let list: Vec<Entry> = self.under(p, usize::MAX).collect();
            return Some(leaf_text(
                list.iter().map(|(k, id, o, h)| (*k, id.as_str(), *o, h)),
            ));
        }
        let mut children = [None; 16];
        for (d, c) in children.iter_mut().enumerate() {
            *c = self.node_hash(p.child(d as u8));
        }
        Some(inner_text(&children))
    }

    /// The tree: its root's hash and text.
    pub(crate) fn root(&mut self) -> (H, String) {
        let top = self.node_hash(Prefix::ROOT);
        let libraries: BTreeMap<(String, String), H> = self
            .libraries
            .iter()
            .map(|(k, (_, h))| (k.clone(), *h))
            .collect();
        let text = root_text(top.as_ref(), &libraries);
        (sha(&text), text)
    }

    /// The top node's hash (after [`TreeIndex::root`]).
    pub(crate) fn top(&mut self) -> Option<H> {
        self.node_hash(Prefix::ROOT)
    }

    /// The library files: text hashes.
    pub(crate) fn library_hashes(&self) -> BTreeMap<(String, String), H> {
        self.libraries
            .iter()
            .map(|(k, (_, h))| (k.clone(), *h))
            .collect()
    }

    /// The nodes of this trie not in the trie under `parent` at `p` (read
    /// from `stored` along the paths that differ): (place, text) each.
    pub(crate) fn new_nodes(
        &mut self,
        stored: &mut dyn Src,
        p: Prefix,
        parent: Option<&H>,
        out: &mut Vec<(H, String)>,
    ) -> Result<(), SessionError> {
        let Some(h) = self.node_hash(p) else {
            return Ok(());
        };
        if parent == Some(&h) {
            return Ok(());
        }
        let text = self.node_text(p).expect("a node is here");
        out.push((h, text));
        if self.is_leaf(p) {
            return Ok(());
        }
        let parent_children = match parent {
            Some(ph) => match stored.node(p, ph)? {
                Node::Inner(c) => Some(c),
                Node::Leaf(_) => None,
            },
            None => None,
        };
        for d in 0..16u8 {
            let ph = parent_children.as_ref().and_then(|c| c[usize::from(d)]);
            self.new_nodes(stored, p.child(d), ph.as_ref(), out)?;
        }
        Ok(())
    }
}

impl Src for TreeIndex {
    fn node(&mut self, p: Prefix, _: &H) -> Result<Node, SessionError> {
        if self.is_leaf(p) {
            return Ok(Node::Leaf(self.under(p, usize::MAX).collect()));
        }
        let mut children: Box<[Option<H>; 16]> = Box::new([None; 16]);
        for (d, c) in children.iter_mut().enumerate() {
            *c = self.node_hash(p.child(d as u8));
        }
        Ok(Node::Inner(children))
    }
}

/// The objects of a flat tree as a [`Src`]-less list, or of a split one
/// walked whole.
pub(crate) fn all_entries(src: &mut dyn Src, tree: &TreeBlob) -> Result<Vec<Entry>, SessionError> {
    match tree {
        TreeBlob::Flat { objects, .. } => Ok(objects.clone()),
        TreeBlob::Split { objects: None, .. } => Ok(Vec::new()),
        TreeBlob::Split {
            objects: Some(h), ..
        } => {
            let mut out = Vec::new();
            entries_under(src, Prefix::ROOT, h, &mut out)?;
            Ok(out)
        }
    }
}

/// Every node hash of a split trie (to carry a revision in a bundle).
pub(crate) fn node_hashes(
    src: &mut dyn Src,
    top: &H,
    out: &mut Vec<H>,
) -> Result<(), SessionError> {
    fn walk(src: &mut dyn Src, p: Prefix, h: &H, out: &mut Vec<H>) -> Result<(), SessionError> {
        out.push(*h);
        if let Node::Inner(c) = src.node(p, h)? {
            for (d, x) in c.iter().enumerate() {
                if let Some(x) = x {
                    walk(src, p.child(d as u8), x, out)?;
                }
            }
        }
        Ok(())
    }
    walk(src, Prefix::ROOT, top, out)
}

/// One side of a comparison of trees: a revision's stored tree, or a
/// project line's working state (its [`TreeIndex`]).
#[derive(Debug, Clone, Copy)]
pub(crate) enum Side<'a> {
    /// A stored tree, by its hash.
    Stored(&'a str),
    /// A project line, by name.
    Line(&'a str),
}

/// What differs between two sides.
#[derive(Debug, Default)]
pub(crate) struct Differs {
    /// The objects.
    pub(crate) objects: Changed,
    /// The library files (text hashes: first side, second side).
    pub(crate) libraries: BTreeMap<(String, String), (Option<H>, Option<H>)>,
}

/// A side's top node and library files.
enum Top {
    Flat(Vec<Entry>),
    Split(Option<H>),
}

impl crate::Session {
    pub(crate) fn line_objects_named(
        &self,
        name: &str,
    ) -> Result<&BTreeMap<(Kind, String), Object>, SessionError> {
        if name == self.current {
            return Ok(&self.objects);
        }
        self.others
            .get(name)
            .map(|s| &s.objects)
            .ok_or_else(|| crate::refused(format!("no project line \"{name}\"")))
    }

    fn trees_lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, TreeIndex>> {
        self.trees
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Project line `name`'s split tree as it is now: its hash and text.
    /// Costs what changed since its last tree (the first after opening:
    /// every object hashed once).
    pub(crate) fn line_tree(&self, name: &str) -> Result<(String, String), SessionError> {
        let objects = self.line_objects_named(name)?;
        let mut trees = self.trees_lock();
        let index = trees.entry(name.to_owned()).or_default();
        index.update(objects);
        let (h, text) = index.root();
        Ok((hex(&h), text))
    }

    /// Whether the tree stored under `hash` is split (its text read, not
    /// parsed: a split root's canonical text ends with its version).
    pub(crate) fn tree_is_split(&self, hash: &str) -> Result<bool, SessionError> {
        Ok(self
            .read_blob(hash)?
            .ends_with(&format!(",\"v\":{VERSION}}}")))
    }

    /// The tree stored under `hash`.
    pub(crate) fn stored_tree(&self, hash: &str) -> Result<TreeBlob, SessionError> {
        TreeBlob::parse(&self.read_blob(hash)?)
    }

    /// What differs between two sides (at most one a project line),
    /// reading only the nodes that differ where both are split (a flat
    /// tree: its list read whole); no object is read.
    pub(crate) fn differs(&self, a: Side<'_>, b: Side<'_>) -> Result<Differs, SessionError> {
        if let (Side::Line(_), Side::Line(_)) = (a, b) {
            return Err(SessionError::Store(
                "two project lines' trees are compared through a revision".into(),
            ));
        }
        let read = |h: &str| self.read_blob(h);
        let mut stored = Stored::new(&read);
        let mut stored_b = Stored::new(&read);
        let mut trees = self.trees_lock();
        let mut tops = Vec::new();
        let mut libs = Vec::new();
        let mut line = None;
        for side in [a, b] {
            match side {
                Side::Stored(h) => {
                    let t = self.stored_tree(h)?;
                    libs.push(t.libraries().clone());
                    tops.push(match t {
                        TreeBlob::Flat { objects, .. } => Top::Flat(objects),
                        TreeBlob::Split { objects, .. } => Top::Split(objects),
                    });
                }
                Side::Line(name) => {
                    let objects = self.line_objects_named(name)?;
                    let index = trees.entry(name.to_owned()).or_default();
                    index.update(objects);
                    index.root();
                    libs.push(index.library_hashes());
                    tops.push(Top::Split(index.top()));
                    line = Some(name.to_owned());
                }
            }
        }
        let mut out = Differs::default();
        let lb = libs.pop().unwrap_or_default();
        let la = libs.pop().unwrap_or_default();
        for key in la.keys().chain(lb.keys()) {
            let (x, y) = (la.get(key).copied(), lb.get(key).copied());
            if x != y {
                out.libraries.insert(key.clone(), (x, y));
            }
        }
        let tb = tops.pop().expect("two sides");
        let ta = tops.pop().expect("two sides");
        let a_is_line = matches!(a, Side::Line(_));
        let b_is_line = matches!(b, Side::Line(_));
        let mut index = line.map(|n| trees.get_mut(&n).expect("made above"));
        match (ta, tb) {
            (Top::Split(x), Top::Split(y)) => {
                let (sa, sb): (&mut dyn Src, &mut dyn Src) = match (a_is_line, b_is_line) {
                    (true, _) => (index.as_deref_mut().expect("a line"), &mut stored),
                    (_, true) => (&mut stored, index.as_deref_mut().expect("a line")),
                    _ => (&mut stored, &mut stored_b),
                };
                diff_nodes(
                    sa,
                    sb,
                    Prefix::ROOT,
                    x.as_ref(),
                    y.as_ref(),
                    &mut out.objects,
                )?;
            }
            (ta, tb) => {
                let mut lists = Vec::new();
                for (t, is_line) in [(ta, a_is_line), (tb, b_is_line)] {
                    let mut v = Vec::new();
                    match t {
                        Top::Flat(list) => v = list,
                        Top::Split(None) => {}
                        Top::Split(Some(h)) => {
                            let src: &mut dyn Src = if is_line {
                                index.as_deref_mut().expect("a line")
                            } else {
                                &mut stored
                            };
                            entries_under(src, Prefix::ROOT, &h, &mut v)?;
                        }
                    }
                    lists.push(v);
                }
                let eb = lists.pop().expect("two");
                let ea = lists.pop().expect("two");
                compare_entries(ea, eb, &mut out.objects);
            }
        }
        Ok(out)
    }

    /// Whether project line `name`'s working state is revision `rev`'s
    /// (its tree split or flat).
    pub(crate) fn line_is_revision(&self, name: &str, rev: &str) -> Result<bool, SessionError> {
        let tree = self.revision(rev)?.tree;
        let (now, _) = self.line_tree(name)?;
        if now == tree {
            return Ok(true);
        }
        if self.tree_is_split(&tree)? {
            return Ok(false);
        }
        let d = self.differs(Side::Stored(&tree), Side::Line(name))?;
        Ok(d.objects.is_empty() && d.libraries.is_empty())
    }

    /// The objects `entries` of stored trees (place and value hash), read
    /// from the store (a value read once however many objects hold it).
    pub(crate) fn stored_values(
        &self,
        entries: impl Iterator<Item = ((Kind, String), (i64, H))>,
    ) -> Result<BTreeMap<(Kind, String), crate::versions::Placed>, SessionError> {
        let mut read: HashMap<H, Arc<Value>> = HashMap::new();
        let mut out = BTreeMap::new();
        for (key, (ord, h)) in entries {
            let v = match read.get(&h) {
                Some(v) => Arc::clone(v),
                None => {
                    let v: Value = serde_json::from_str(&self.read_blob(&hex(&h))?)
                        .map_err(|e| bad(&format!("an object: {e}")))?;
                    let v = Arc::new(v);
                    read.insert(h, Arc::clone(&v));
                    v
                }
            };
            out.insert(key, (ord, v));
        }
        Ok(out)
    }

    /// Writes project line `name`'s split tree into `tx`: the nodes and
    /// objects not in the tree `parent` (a stored tree's hash, read along
    /// the paths that differ), and the root. Returns the tree's hash.
    pub(crate) fn write_line_tree(
        &self,
        tx: &dyn crate::store::Store,
        name: &str,
        parent: Option<&str>,
    ) -> Result<String, SessionError> {
        let objects = self.line_objects_named(name)?;
        let parent_tree = parent.map(|p| self.stored_tree(p)).transpose()?;
        let read = |h: &str| self.read_blob(h);
        let mut stored = Stored::new(&read);
        let mut trees = self.trees_lock();
        let index = trees.entry(name.to_owned()).or_default();
        index.update(objects);
        let (root_h, root) = index.root();
        let top = index.top();
        // The nodes new to this tree.
        let mut nodes = Vec::new();
        let parent_top = match &parent_tree {
            Some(TreeBlob::Split { objects, .. }) => *objects,
            _ => None,
        };
        index.new_nodes(&mut stored, Prefix::ROOT, parent_top.as_ref(), &mut nodes)?;
        // The objects new to it: those that differ from the parent's.
        let mut changed = Changed::new();
        match &parent_tree {
            Some(TreeBlob::Split { objects: p, .. }) => {
                diff_nodes(
                    &mut stored,
                    index,
                    Prefix::ROOT,
                    p.as_ref(),
                    top.as_ref(),
                    &mut changed,
                )?;
            }
            other => {
                let mut now = Vec::new();
                if let Some(t) = &top {
                    entries_under(index, Prefix::ROOT, t, &mut now)?;
                }
                let before = match other {
                    Some(TreeBlob::Flat { objects: p, .. }) => p.clone(),
                    _ => Vec::new(),
                };
                compare_entries(before, now, &mut changed);
            }
        }
        let parent_libs = parent_tree
            .as_ref()
            .map(|t| t.libraries().clone())
            .unwrap_or_default();
        let libraries = index.library_hashes();
        drop(trees);
        for (key, (was, now)) in &changed {
            let Some((_, h)) = now else {
                continue;
            };
            if was.is_some_and(|(_, w)| w == *h) {
                continue;
            }
            let o = objects
                .get(key)
                .ok_or_else(|| bad("an object of the tree went while written"))?;
            Self::write_blob(tx, &hex(h), &canonical(&o.value))?;
        }
        for ((kind, file), h) in &libraries {
            if parent_libs.get(&(kind.clone(), file.clone())) == Some(h) {
                continue;
            }
            let text = objects
                .get(&(Kind::Library, crate::library_id(kind, file)))
                .and_then(|o| crate::library_entry(&o.value))
                .map(|(_, _, t)| t)
                .ok_or_else(|| bad("a library file of the tree went while written"))?;
            Self::write_blob(tx, &hex(h), &text)?;
        }
        for (h, text) in &nodes {
            Self::write_blob(tx, &hex(h), text)?;
        }
        let root_hex = hex(&root_h);
        Self::write_blob(tx, &root_hex, &root)?;
        Ok(root_hex)
    }

    /// The objects of revision `rev`'s state as a line holds them
    /// ([`crate::lines::objects_of_state`]: every object at version 1, the
    /// library files after them by kind and name), **read along what
    /// differs** from the objects `objects` of project line `name` (its
    /// tree kept as a commit keeps it — the first time every object
    /// hashed once, then what changed): when `rev`'s tree is split only
    /// the nodes whose hashes differ and the objects in them that differ
    /// are read, the rest taken from `objects` (their JSON shared). `None`:
    /// `rev`'s tree is flat (read it whole).
    ///
    /// Phase 12's second item: opening a model whose branches are kept as
    /// differences read each branch's base revision whole.
    pub(crate) fn objects_at(
        &self,
        rev: &str,
        name: &str,
        objects: &BTreeMap<(Kind, String), Object>,
    ) -> Result<Option<BTreeMap<(Kind, String), Object>>, SessionError> {
        let tree = self.stored_tree(&self.revision(rev)?.tree)?;
        let TreeBlob::Split {
            objects: top,
            libraries,
        } = &tree
        else {
            return Ok(None);
        };
        let read = |h: &str| self.read_blob(h);
        let mut stored = Stored::new(&read);
        let mut changed = Changed::new();
        let main_libs = {
            let mut trees = self.trees_lock();
            let index = trees.entry(name.to_owned()).or_default();
            index.update(objects);
            index.root();
            let mine = index.top();
            diff_nodes(
                index,
                &mut stored,
                Prefix::ROOT,
                mine.as_ref(),
                top.as_ref(),
                &mut changed,
            )?;
            index.library_hashes()
        };
        let mut out: BTreeMap<(Kind, String), Object> = objects
            .iter()
            .filter(|(k, _)| k.0 != Kind::Library)
            .map(|(k, o)| {
                (
                    k.clone(),
                    Object {
                        ord: o.ord,
                        version: 1,
                        value: Arc::clone(&o.value),
                    },
                )
            })
            .collect();
        let mut wanted = Vec::new();
        for (key, (was, now)) in changed {
            match now {
                None => {
                    out.remove(&key);
                }
                // The same JSON at another place: its place only.
                Some((ord, h)) if was.is_some_and(|(_, w)| w == h) => {
                    if let Some(o) = out.get_mut(&key) {
                        o.ord = ord;
                    }
                }
                Some(e) => wanted.push((key, e)),
            }
        }
        for (key, (ord, value)) in self.stored_values(wanted.into_iter())? {
            out.insert(
                key,
                Object {
                    ord,
                    version: 1,
                    value,
                },
            );
        }
        // The library files: the line's own text where the hash is the
        // same, else read; placed as a state places them.
        let texts: BTreeMap<(String, String), String> = objects
            .values()
            .filter_map(|o| crate::library_entry(&o.value))
            .map(|(k, n, t)| ((k, n), t))
            .collect();
        for (k, ((kind, file), h)) in libraries.iter().enumerate() {
            let key = (kind.clone(), file.clone());
            let text = match (main_libs.get(&key), texts.get(&key)) {
                (Some(m), Some(t)) if m == h => t.clone(),
                _ => self.read_blob(&hex(h))?,
            };
            out.insert(
                (Kind::Library, crate::library_id(kind, file)),
                Object {
                    ord: k as i64,
                    version: 1,
                    value: Arc::new(crate::library_value(kind, file, &text)),
                },
            );
        }
        Ok(Some(out))
    }

    /// Forgets what was kept of project line `name`'s tree (deleted).
    pub(crate) fn forget_tree(&self, name: &str) {
        self.trees_lock().remove(name);
    }

    /// Keeps what was kept of project line `from`'s tree under `to`
    /// (renamed).
    pub(crate) fn rename_tree(&self, from: &str, to: &str) {
        let mut trees = self.trees_lock();
        if let Some(t) = trees.remove(from) {
            trees.insert(to.to_owned(), t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn objects(n: usize, tag: &str) -> BTreeMap<(Kind, String), Object> {
        (0..n)
            .map(|k| {
                let id = format!("T{k}");
                (
                    (Kind::Structure, id.clone()),
                    Object {
                        ord: k as i64,
                        version: 1,
                        value: Arc::new(serde_json::json!({"id": id, "tag": tag})),
                    },
                )
            })
            .collect()
    }

    /// The trie depends on the objects only: made at once, or step by step
    /// with objects added, changed and removed, it is the same.
    #[test]
    fn the_trie_is_the_objects_own() {
        let all = objects(500, "a");
        let mut whole = TreeIndex::default();
        whole.update(&all);
        let (h1, _) = whole.root();
        let mut steps = TreeIndex::default();
        steps.update(&objects(10, "a"));
        steps.root();
        steps.update(&objects(800, "b"));
        steps.root();
        steps.update(&all);
        let (h2, _) = steps.root();
        assert_eq!(h1, h2);
        // A leaf's limit: 500 objects need more than one level.
        assert!(!whole.is_leaf(Prefix::ROOT));
        let mut few = TreeIndex::default();
        few.update(&objects(LEAF, "a"));
        assert!(few.is_leaf(Prefix::ROOT));
    }

    /// The diff of two tries reads only what differs and finds exactly the
    /// objects changed, added and removed.
    #[test]
    fn diffs_are_exact() {
        let a = objects(700, "a");
        let mut b = a.clone();
        let changed = (Kind::Structure, "T5".to_owned());
        b.get_mut(&changed).expect("there").value = Arc::new(serde_json::json!({"id": "T5"}));
        b.remove(&(Kind::Structure, "T6".to_owned()));
        b.insert(
            (Kind::Structure, "N1".to_owned()),
            Object {
                ord: 9,
                version: 1,
                value: Arc::new(serde_json::json!({"id": "N1"})),
            },
        );
        let (mut ia, mut ib) = (TreeIndex::default(), TreeIndex::default());
        ia.update(&a);
        ib.update(&b);
        let (ta, tb) = (ia.top(), ib.top());
        let mut out = Changed::new();
        diff_nodes(
            &mut ia,
            &mut ib,
            Prefix::ROOT,
            ta.as_ref(),
            tb.as_ref(),
            &mut out,
        )
        .expect("in memory");
        let keys: Vec<&str> = out.keys().map(|(_, id)| id.as_str()).collect();
        assert_eq!(keys, ["N1", "T5", "T6"]);
        // The texts read back.
        let (_, text) = ib.root();
        assert!(matches!(
            TreeBlob::parse(&text).expect("reads"),
            TreeBlob::Split { .. }
        ));
        let mut nodes = Vec::new();
        ib.new_nodes(&mut ia, Prefix::ROOT, ta.as_ref(), &mut nodes)
            .expect("in memory");
        // Only the paths of the three objects: a few nodes, not the trie.
        assert!(nodes.len() <= 3 * 3, "{} nodes", nodes.len());
        for (h, text) in &nodes {
            assert_eq!(sha(text), *h);
            parse_node(text).expect("a node reads back");
        }
    }

    #[test]
    fn hashes_and_prefixes() {
        let h = sha("x");
        assert_eq!(unhex(&hex(&h)), Some(h));
        assert_eq!(unhex("zz"), None);
        let p = Prefix::ROOT
            .child(nibble(&h, 0))
            .child(nibble(&h, 1))
            .child(nibble(&h, 2));
        assert!(p.holds(&h));
        assert!(p.low() <= h);
        let q = Prefix::ROOT.child((nibble(&h, 0) + 1) % 16);
        assert!(!q.holds(&h));
    }
}
