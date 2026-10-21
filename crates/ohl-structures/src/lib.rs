// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Structure type library (`*.ohls`) and insulator assembly library
//! (`*.ohli`): one item per file, TOML with units, loaded from directories
//! (DESIGN_GOALS §10–11).
//!
//! - [`structures`]: structure types in local axes, placement into a line.
//! - [`insulators`]: suspension, V-string and strain assemblies.
//! - [`derive`](mod@derive): draft libraries derived from the attachment geometry of an
//!   existing line.
//! - [`templates`]: pylon templates (`*.ohlpt`), the face of a pylon as a
//!   few dimensions, building structure types of level 1.
//! - [`pole`]: the pole of a pole structure, named (`.ohlp`) or described
//!   in the structure file (`[pole]`, levels 1 to 3).
//! - [`extensions`]: height extensions, their bodies and leg extensions.
//! - [`on_members`]: joints inserted on members (a point read from a
//!   file that put it along a member).

pub mod derive;
pub mod extensions;
pub mod insulators;
pub mod on_members;
pub mod pole;
pub mod structures;
pub mod templates;

#[cfg(test)]
mod joint_tests;
#[cfg(test)]
mod pole_extension_tests;

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

pub use extensions::{ExtendedBody, Extension, LegExtension};
pub use insulators::{AssemblyKind, InsulatorAssembly, InsulatorLibrary, InsulatorLimits};
pub use pole::{InlinePole, PoleRef, PoleRing, PoleShape, PoleWall};
pub use structures::{
    AllowedSpans, AttachmentCapacity, Capacity, Detail, Interaction, Joint, JointKind, JointSource,
    NOT_ATTACH, PlacedAttachment, PlacedPoint, PlacedString, PlacedStrings, PlacedWire, PointPlace,
    SAME_JOINT, SpanLimit, StructureLibrary, StructureType, StructureWind, TypePoint, TypeString,
    TypeWire, WindBand, point_joints,
};

/// Error loading a library item.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryError {
    /// File concerned.
    pub path: PathBuf,
    /// What went wrong.
    pub message: String,
}

impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

impl std::error::Error for LibraryError {}

/// An item stored one per file in a library directory.
pub trait LibraryItem: Sized + for<'de> serde::Deserialize<'de> {
    /// File extension, without the dot.
    const EXTENSION: &'static str;
    /// Human-readable kind, for messages.
    const KIND: &'static str;

    /// Unique id.
    fn id(&self) -> &str;

    /// Plausibility checks.
    fn check(&self) -> Result<(), String>;

    /// Fills in what follows from the rest of the file, before the checks.
    fn prepare(&mut self) -> Result<(), String> {
        Ok(())
    }

    /// Parses and checks one file's text.
    fn parse(path: &Path, text: &str) -> Result<Self, LibraryError> {
        let error = |message: String| LibraryError {
            path: path.to_owned(),
            message,
        };
        let mut item: Self = toml::from_str(text).map_err(|e| error(e.to_string()))?;
        let id = item.id().to_owned();
        item.prepare()
            .map_err(|m| error(format!("{} {id}: {m}", Self::KIND)))?;
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        {
            return Err(error(format!(
                "id \"{id}\" must use only letters, digits, -, _ and ."
            )));
        }
        item.check()
            .map_err(|m| error(format!("{} {}: {m}", Self::KIND, item.id())))?;
        Ok(item)
    }
}

/// Items loaded from a library directory, by id.
#[derive(Debug, Clone)]
pub struct Library<T> {
    items: BTreeMap<String, T>,
    /// The file each item was read from.
    paths: BTreeMap<String, PathBuf>,
}

impl<T> Default for Library<T> {
    fn default() -> Self {
        Self {
            items: BTreeMap::new(),
            paths: BTreeMap::new(),
        }
    }
}

impl<T: LibraryItem> Library<T> {
    /// Loads every `*.<extension>` file of a directory (not recursive), in
    /// name order; a missing directory gives an empty library.
    pub fn load_dir(dir: &Path) -> Result<Self, LibraryError> {
        let mut library = Self::default();
        if !dir.exists() {
            return Ok(library);
        }
        let io = |path: &Path, e: std::io::Error| LibraryError {
            path: path.to_owned(),
            message: e.to_string(),
        };
        std::fs::read_dir(dir).map_err(|e| io(dir, e))?;
        let paths = ohl_core::files::files_with_extension(dir, T::EXTENSION);
        for path in paths {
            let text = std::fs::read_to_string(&path).map_err(|e| io(&path, e))?;
            library.insert(&path, T::parse(&path, &text)?)?;
        }
        Ok(library)
    }

    /// Adds an item; duplicate ids are rejected.
    pub fn insert(&mut self, path: &Path, item: T) -> Result<(), LibraryError> {
        let id = item.id().to_owned();
        if self.items.contains_key(&id) {
            return Err(LibraryError {
                path: path.to_owned(),
                message: format!("duplicate {} id {id}", T::KIND),
            });
        }
        self.paths.insert(id.clone(), path.to_owned());
        self.items.insert(id, item);
        Ok(())
    }

    /// The file an item was read from (files it names are relative to
    /// its folder).
    pub fn path(&self, id: &str) -> Option<&Path> {
        self.paths.get(id).map(PathBuf::as_path)
    }

    /// Item by id.
    pub fn get(&self, id: &str) -> Option<&T> {
        self.items.get(id)
    }

    /// All items, sorted by id.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.values()
    }

    /// Number of items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the library is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
