// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A whole project copied from one [`Store`] into another through the
//! interface (ADR-0013, phase 4): a model of a country server's database
//! written as a project file of its own (the per-model export), or a
//! project file into any store. Every row is kept as it is — objects with
//! their order, versions and JSON text, log entries with their numbers,
//! times and marks, revisions with their times, branches and tags,
//! grants, reviews and their notes, surfaces, the spatial index, meta
//! rows and users.
//!
//! Not copied: point clouds and backgrounds (their own stores, ADR-0007;
//! not kept by the network's store yet). [`Copied`] says what was.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ohl_core::grid::Grid;

use super::Store;
use crate::{Kind, SessionError, Step};

/// What [`copy_all`] copied.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Copied {
    /// Meta rows.
    pub meta: usize,
    /// Users.
    pub users: usize,
    /// Object rows (tombstones and places included).
    pub objects: usize,
    /// Log entries.
    pub entries: usize,
    /// Spatial index rows.
    pub spatial: usize,
    /// Blobs.
    pub blobs: usize,
    /// Revisions.
    pub revisions: usize,
    /// Branches and tags.
    pub refs: usize,
    /// Grants.
    pub grants: usize,
    /// Reviews.
    pub reviews: usize,
    /// Notes of reviews.
    pub review_notes: usize,
    /// Surfaces.
    pub surfaces: usize,
}

/// The project lines (branches) a store keeps rows of: those of its
/// objects and its branches, by name.
pub fn lines_of(store: &dyn Store) -> Result<BTreeSet<String>, SessionError> {
    let mut lines: BTreeSet<String> = store.object_rows()?.into_iter().map(|r| r.line).collect();
    for r in store.refs()? {
        if r.kind == "branch" {
            lines.insert(r.name);
        }
    }
    Ok(lines)
}

/// Copies everything `from` keeps into `to`, an empty store (prepared
/// here, `label` naming it in messages), as one step: on an error
/// nothing of it stays in `to`. Returns `to`'s grid and what was copied.
pub fn copy_all(
    from: &dyn Store,
    to: &dyn Store,
    label: &Path,
) -> Result<(Grid, Copied), SessionError> {
    let grid = to.prepare(label, true)?;
    let tx = Step::begin(to)?;
    let mut n = Copied::default();
    for (key, value) in from.meta_rows()? {
        to.set_meta(&key, &value)?;
        n.meta += 1;
    }
    for (name, role) in from.users()? {
        to.put_user(&name, &role)?;
        n.users += 1;
    }
    for r in from.object_rows()? {
        let kind = Kind::parse(&r.kind)?;
        match r.gone {
            0 => to.write_object(&r.line, kind, &r.id, Some((r.ord, r.version, &r.json)))?,
            1 => to.write_object(&r.line, kind, &r.id, None)?,
            _ => to.write_place(&r.line, kind, &r.id, r.ord, r.version)?,
        }
        n.objects += 1;
    }
    for e in from.entries()? {
        to.put_entry_row(&e)?;
        n.entries += 1;
    }
    let none = BTreeMap::new();
    for line in lines_of(from)? {
        let entries = from.spatial_load(&line)?;
        n.spatial += to.spatial_write_diff(&line, &none, &entries)?;
    }
    for hash in from.blob_hashes()? {
        let data = from
            .blob(&hash)?
            .ok_or_else(|| SessionError::Store(format!("blob {hash} went while copied")))?;
        if !to.has_blob(&hash)? {
            to.put_blob(&hash, &data)?;
        }
        n.blobs += 1;
    }
    // Oldest first, so that "newest first" reads the same in `to`.
    let mut revisions = from.revisions()?;
    revisions.reverse();
    for r in &revisions {
        to.put_revision_with_time(r)?;
        n.revisions += 1;
    }
    for r in from.refs()? {
        let (description, base) = if r.kind == "branch" {
            (
                from.line_description(&r.name)?.unwrap_or_default(),
                from.line_base(&r.name)?,
            )
        } else {
            (String::new(), None)
        };
        to.put_ref_row(&r, &description, base.as_deref())?;
        n.refs += 1;
    }
    for g in from.grants()? {
        to.put_grant_row(&g)?;
        n.grants += 1;
    }
    let mut reviews = from.reviews()?;
    reviews.reverse();
    for r in &reviews {
        to.put_review_row(r)?;
        n.reviews += 1;
    }
    for note in from.review_notes()? {
        to.put_review_note_row(&note)?;
        n.review_notes += 1;
    }
    for s in from.surfaces()? {
        let data = from
            .surface_data(&s.name)?
            .ok_or_else(|| SessionError::Store(format!("surface {} went while copied", s.name)))?;
        to.put_surface_row(&s, &data)?;
        n.surfaces += 1;
    }
    tx.commit()?;
    Ok((grid, n))
}
