// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **The offline working set** (country server, phase 12; ADR-0013,
//! "Offline working sets"): a user's working set exported from a
//! network's server as a project file of today's format, worked on with
//! no connection, and brought back as a **bundle of only the changes**.
//!
//! **The file** is a part of the line ([`mod@crate::part`], as phase 6's
//! replica): the working set editable, its context and the line's own
//! neighbours within the checks' reach read-only, the line's settings and
//! library files; other lines' neighbours (what solving their sections
//! near the working set reads, structure types resolved) in the meta key
//! [`NEIGHBOURS_KEY`]; and the export's record in [`OFFLINE_KEY`]
//! ([`OfflineMeta`]: the network, the model, the user, the server's base
//! revision, the export's own revision, the scope, the reach, the library
//! pins, what was left out because the user may not see it). The file's
//! first revision is the **export's revision** (`E`): the exported state,
//! made by the server when it writes the file and recorded on the
//! server — a bundle's base is checked against it. The desktop's `.ohl`
//! format is unchanged (meta keys added, documented in the data model).
//!
//! **The bundle** ([`Session::offline_bundle`]) is the existing bundle
//! format ([`crate::versions::Bundle`]) holding only the revisions made
//! since `E` and the blobs they need that `E` does not hold — never the
//! exported state again. Uncommitted work refuses it (commit first).
//!
//! **Bringing it back** ([`offline_changes`], on the server): the bundle
//! verified as any bundle is (every blob against its hash, every revision
//! against its id, every parent known, every tree whole) and against the
//! export's revision the server recorded (a base it does not know, or a
//! revision not made from it, is refused); what it changes is the
//! difference between `E` and its head — objects put or deleted, library
//! files changed (the server refuses those). Places among a kind that
//! changed alone are not changes. The server then applies the changes on
//! a branch over the working set made from its base revision, checked
//! against the user's current rights and the rules of shared towers.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::part::PartSummary;
use crate::versions::{
    BUNDLE_FILE_FORMAT, BUNDLE_FILE_VERSION, Bundle, MAIN, Revision, canonical, hash, short,
};
use crate::working_set::{Replica, WorkingSetSpec};
use crate::{Kind, Session, SessionError, refused};

/// The `ols_meta` key of an offline working set's record (JSON,
/// [`OfflineMeta`]).
pub const OFFLINE_KEY: &str = "network.offline";
/// The `ols_meta` key holding the other lines' neighbours of an offline
/// working set (JSON, a list of [`NeighbourLine`]).
pub const NEIGHBOURS_KEY: &str = "network.neighbours";

/// What was left out of an export because its user may not see it:
/// counted, never named nor placed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LeftOut {
    /// Sections and jumpers of other lines within the reach.
    pub other_lines: usize,
    /// Objects of this line within the reach (its neighbours beyond the
    /// working set left out whole when one of them is hidden).
    pub this_line: usize,
}

impl LeftOut {
    /// Nothing left out.
    pub fn is_empty(&self) -> bool {
        self.other_lines == 0 && self.this_line == 0
    }
}

/// The record of an offline working set ([`OFFLINE_KEY`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflineMeta {
    /// The network's name.
    pub network: String,
    /// The model (line) it is of.
    pub model: String,
    /// Who exported it (the bundle comes back from them).
    pub user: String,
    /// When (UTC).
    pub time: String,
    /// The server's revision of the model's main line it was taken from.
    pub base: String,
    /// The export's own revision (the file's first; a bundle's base).
    pub export: String,
    /// The working set picked.
    pub spec: WorkingSetSpec,
    /// Its routes.
    pub routes: Vec<String>,
    /// The checks' reach the neighbours were taken within, m.
    pub reach_m: f64,
    /// What the neighbours cover in plan, x0, y0, x1, y1: the working
    /// set's bounds grown by the reach and three times the network's
    /// largest span margin (a check asking for anything outside says NOT
    /// ALL CHECKED).
    pub area: [f64; 4],
    /// The network's largest span margin when exported, m (the scoped
    /// checks' candidate search, as on the server).
    pub max_margin: f64,
    /// The model's library pins (kind → release).
    #[serde(default)]
    pub library_pins: BTreeMap<String, String>,
    /// What its user may not see, left out (counted).
    #[serde(default)]
    pub left_out: LeftOut,
    /// Other lines' neighbours held ([`NEIGHBOURS_KEY`]): lines, sections.
    #[serde(default)]
    pub neighbour_lines: usize,
    /// See `neighbour_lines`.
    #[serde(default)]
    pub neighbour_sections: usize,
}

/// Another line's neighbours of an offline working set: what solving its
/// sections within the reach reads (structure types resolved), with the
/// library files its checks read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NeighbourLine {
    /// The model (line).
    pub model: String,
    /// Its part.
    pub line: ohl_line::Line,
    /// Its library files by kind and name (wires, weather and load cases).
    #[serde(default)]
    pub libraries: BTreeMap<String, BTreeMap<String, String>>,
}

/// What a bundle brought back changes ([`offline_changes`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OfflineChanges {
    /// The bundle's head.
    pub head: String,
    /// Its revisions after the export's: id, author, message (parents
    /// first).
    pub revisions: Vec<(String, String, String)>,
    /// Objects new or changed: kind, id, JSON.
    pub puts: Vec<(Kind, String, Value)>,
    /// Objects deleted.
    pub deletes: Vec<(Kind, String)>,
    /// Library files changed (kind, name).
    pub libraries: Vec<(String, String)>,
}

impl OfflineChanges {
    /// Every object it changes.
    pub fn objects(&self) -> BTreeSet<(Kind, String)> {
        self.puts
            .iter()
            .map(|(k, id, _)| (*k, id.clone()))
            .chain(self.deletes.iter().cloned())
            .collect()
    }
}

/// The hash a tree gives an object's JSON (SHA-256 of its canonical text,
/// hexadecimal): an object of the server and of an offline file compared.
pub fn value_hash(v: &Value) -> String {
    hash(&canonical(v))
}

/// The objects of a flat tree's text, each with its value hash (a split
/// tree's: refused — an offline file keeps flat trees).
pub fn tree_object_hashes(tree: &str) -> Result<BTreeMap<(Kind, String), String>, SessionError> {
    match crate::trees::TreeBlob::parse(tree)? {
        crate::trees::TreeBlob::Flat { objects, .. } => Ok(objects
            .into_iter()
            .map(|(k, id, _, h)| ((k, id), crate::trees::hex(&h)))
            .collect()),
        crate::trees::TreeBlob::Split { .. } => Err(refused(
            "the export's tree is split: an offline working set's file keeps flat trees",
        )),
    }
}

/// The changes bundle `bundle` brings back onto the export's revision
/// `base` (its id) whose tree is `base_tree` (as the server recorded it).
/// Refused: a bundle not of the bundle format, a blob not matching its
/// hash, a revision whose id is not its content's, a revision not made
/// from the export (a parent unknown, another first revision), a tree or
/// an object it names missing, a head not among its revisions, nothing
/// changed.
pub fn offline_changes(
    bundle: &Bundle,
    base: &str,
    base_tree: &str,
) -> Result<OfflineChanges, SessionError> {
    ohl_core::files::check_header(
        Some(&bundle.format),
        Some(bundle.version),
        BUNDLE_FILE_FORMAT,
        BUNDLE_FILE_VERSION,
        "bundle",
    )
    .map_err(refused)?;
    for (h, text) in &bundle.blobs {
        if hash(text) != *h {
            return Err(refused(format!(
                "bundle: blob {} does not match its hash (the file was changed)",
                short(h)
            )));
        }
    }
    let base_hashes = tree_object_hashes(base_tree)?;
    let mut seen: BTreeSet<String> = [base.to_owned()].into();
    let mut out = OfflineChanges {
        head: bundle.head.clone(),
        ..OfflineChanges::default()
    };
    let read = |h: &str| -> Result<String, SessionError> {
        bundle.blobs.get(h).cloned().ok_or_else(|| {
            refused(format!(
                "bundle: blob {} is missing (the file is not whole)",
                short(h)
            ))
        })
    };
    let mut trees: BTreeMap<String, crate::trees::TreeBlob> = BTreeMap::new();
    for r in &bundle.revisions {
        if crate::versions::revision_id(&r.tree, &r.parents, &r.author, &r.message) != r.id {
            return Err(refused(format!(
                "bundle: revision {} is damaged (the file was changed)",
                r.short()
            )));
        }
        // The export's revision itself (a whole bundle carries it).
        if r.id == base {
            continue;
        }
        if r.parents.is_empty() {
            return Err(refused(format!(
                "bundle: revision {} was not made from the exported working set",
                r.short()
            )));
        }
        if let Some(p) = r.parents.iter().find(|p| !seen.contains(*p)) {
            return Err(refused(format!(
                "bundle: revision {} comes from {}, which is neither the export nor in the \
                 bundle",
                r.short(),
                short(p)
            )));
        }
        let tree = crate::trees::TreeBlob::parse(&read(&r.tree)?)
            .map_err(|e| refused(format!("bundle: tree of {}: {e}", r.short())))?;
        let src_read = |h: &str| read(h);
        let mut nodes = crate::trees::Stored::new(&src_read);
        let entries = crate::trees::all_entries(&mut nodes, &tree)?;
        for (kind, id, _, h) in &entries {
            let h = crate::trees::hex(h);
            let as_exported = base_hashes.get(&(*kind, id.clone())) == Some(&h);
            if !as_exported && !bundle.blobs.contains_key(&h) {
                return Err(refused(format!(
                    "bundle: {} {id} of revision {} is missing (the file is not whole)",
                    kind.as_str(),
                    r.short()
                )));
            }
        }
        seen.insert(r.id.clone());
        out.revisions
            .push((r.id.clone(), r.author.clone(), r.message.clone()));
        trees.insert(r.id.clone(), tree);
    }
    if bundle.head == base || out.revisions.is_empty() {
        return Err(refused(
            "bundle: nothing was committed since the export (commit your work, then make the \
             bundle again)",
        ));
    }
    let Some(head) = trees.get(&bundle.head) else {
        return Err(refused("bundle: its head is not among its revisions"));
    };
    let base_blob = crate::trees::TreeBlob::parse(base_tree)?;
    let src_read = |h: &str| read(h);
    let mut nodes = crate::trees::Stored::new(&src_read);
    let now = crate::trees::all_entries(&mut nodes, head)?;
    let was = match &base_blob {
        crate::trees::TreeBlob::Flat { objects, .. } => objects.clone(),
        crate::trees::TreeBlob::Split { .. } => unreachable!("refused above"),
    };
    let mut changed = crate::trees::Changed::new();
    crate::trees::compare_entries(was, now, &mut changed);
    for ((kind, id), (was, now)) in changed {
        match now {
            None => out.deletes.push((kind, id)),
            // Its place among its kind alone: not a change of the line.
            Some((_, h)) if was.is_some_and(|(_, w)| w == h) => {}
            Some((_, h)) => {
                let text = read(&crate::trees::hex(&h))?;
                let value: Value = serde_json::from_str(&text)
                    .map_err(|e| refused(format!("bundle: {} {id}: {e}", kind.as_str())))?;
                out.puts.push((kind, id, value));
            }
        }
    }
    let (lb, lh) = (base_blob.libraries(), head.libraries());
    for key in lb.keys().chain(lh.keys()).collect::<BTreeSet<_>>() {
        if lb.get(key) != lh.get(key) {
            out.libraries.push(key.clone());
        }
    }
    Ok(out)
}

impl Session {
    /// Writes `replica` (an offline working set: its rows, working set and
    /// structures' users) into a new project file at `path` as
    /// [`Session::write_replica`] does, with `meta` beside it, makes its
    /// first revision by `user` with `message` — the export's revision —
    /// and writes `offline` with that revision as its record
    /// ([`OFFLINE_KEY`]). Returns what it holds, the record and the
    /// revision's tree text (what the server keeps).
    pub fn write_offline(
        replica: Replica,
        path: &Path,
        user: &str,
        meta: &[(String, String)],
        mut offline: OfflineMeta,
        message: &str,
    ) -> Result<(PartSummary, OfflineMeta, String), SessionError> {
        let summary = Session::write_replica(replica, path, user, meta)?;
        let mut s = Session::open(path)?;
        let rev: Revision = s.commit(user, message)?;
        let tree = s.read_blob(&rev.tree)?;
        offline.export = rev.id;
        s.conn.set_meta(
            OFFLINE_KEY,
            &serde_json::to_string(&offline).expect("serialises"),
        )?;
        s.checkpoint()?;
        Ok((summary, offline, tree))
    }

    /// The offline working set's record, when this project is one.
    pub fn offline_meta(&self) -> Result<Option<OfflineMeta>, SessionError> {
        match self.conn.meta(OFFLINE_KEY)? {
            Some(t) => serde_json::from_str(&t)
                .map(Some)
                .map_err(|e| SessionError::Store(format!("{OFFLINE_KEY}: {e}"))),
            None => Ok(None),
        }
    }

    /// The other lines' neighbours an offline working set holds.
    pub fn offline_neighbours(&self) -> Result<Vec<NeighbourLine>, SessionError> {
        match self.conn.meta(NEIGHBOURS_KEY)? {
            Some(t) => serde_json::from_str(&t)
                .map_err(|e| SessionError::Store(format!("{NEIGHBOURS_KEY}: {e}"))),
            None => Ok(Vec::new()),
        }
    }

    /// The bundle of an offline working set's work: the main line's
    /// revisions since the export's revision and the blobs they need that
    /// the export does not hold (the existing bundle format, only the
    /// changes). Refused: not an offline working set, uncommitted changes
    /// (commit them first), nothing committed since the export, a main line
    /// not made from it.
    pub fn offline_bundle(&self) -> Result<Bundle, SessionError> {
        let meta = self.offline_meta()?.ok_or_else(|| {
            refused("this project is not an offline working set (ohl network export-working-set)")
        })?;
        if self.has_uncommitted(MAIN)? {
            return Err(refused(
                "the working set has changes not committed: commit them first (they would not \
                 come back)",
            ));
        }
        let head = self
            .branch_revision(MAIN)?
            .ok_or_else(|| refused("the main line has no revision"))?;
        if head == meta.export {
            return Err(refused(
                "nothing was committed since the export: commit your work first",
            ));
        }
        let ancestors = self.ancestors(&head)?;
        if !ancestors.contains_key(&meta.export) {
            return Err(refused(format!(
                "the main line does not come from the export's revision {}",
                short(&meta.export)
            )));
        }
        let before = self.ancestors(&meta.export)?;
        let mut revisions: Vec<Revision> = self
            .revisions()?
            .into_iter()
            .filter(|r| ancestors.contains_key(&r.id) && !before.contains_key(&r.id))
            .collect();
        revisions.reverse();
        let base_tree = self.read_blob(&self.revision(&meta.export)?.tree)?;
        let held = tree_object_hashes(&base_tree)?;
        let base_libs: BTreeSet<String> = crate::trees::TreeBlob::parse(&base_tree)?
            .libraries()
            .values()
            .map(crate::trees::hex)
            .collect();
        let mut blobs = BTreeMap::new();
        let read = |h: &str| self.read_blob(h);
        let mut nodes = crate::trees::Stored::new(&read);
        for r in &revisions {
            let text = self.read_blob(&r.tree)?;
            let tree = crate::trees::TreeBlob::parse(&text)?;
            blobs.insert(r.tree.clone(), text);
            if let crate::trees::TreeBlob::Split {
                objects: Some(top), ..
            } = &tree
            {
                let mut list = Vec::new();
                crate::trees::node_hashes(&mut nodes, top, &mut list)?;
                for h in list {
                    let h = crate::trees::hex(&h);
                    if !blobs.contains_key(&h) {
                        blobs.insert(h.clone(), self.read_blob(&h)?);
                    }
                }
            }
            // An object as exported (the same JSON under the same id)
            // stays behind; every other value travels.
            let named: Vec<String> = crate::trees::all_entries(&mut nodes, &tree)?
                .into_iter()
                .filter(|(k, id, _, h)| held.get(&(*k, id.clone())) != Some(&crate::trees::hex(h)))
                .map(|e| crate::trees::hex(&e.3))
                .chain(
                    tree.libraries()
                        .values()
                        .map(crate::trees::hex)
                        .filter(|h| !base_libs.contains(h)),
                )
                .collect();
            for h in named {
                if !blobs.contains_key(&h) {
                    blobs.insert(h.clone(), self.read_blob(&h)?);
                }
            }
        }
        Ok(Bundle {
            format: BUNDLE_FILE_FORMAT.into(),
            version: BUNDLE_FILE_VERSION,
            branch: format!("offline {} {}", meta.model, meta.user),
            head,
            revisions,
            blobs,
        })
    }
}
