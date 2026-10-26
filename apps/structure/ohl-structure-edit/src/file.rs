// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A structure file being edited: its text, the structure as last read,
//! the undo and redo of whole edits, saving.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use ohl_structures::{LibraryItem, StructureType};
use toml_edit::DocumentMut;

/// Most edits kept for undo.
const UNDO_STEPS: usize = 500;

/// The next revision, unique among every file of the process.
static NEXT_REVISION: AtomicU64 = AtomicU64::new(1);

fn next_revision() -> u64 {
    NEXT_REVISION.fetch_add(1, Ordering::Relaxed)
}

/// One step of the history: the text before (undo) or after (redo) it.
#[derive(Debug, Clone)]
struct Step {
    what: String,
    text: String,
}

/// A structure file being edited. Every change goes through
/// [`StructureFile::edit`]: one undo step each, or one for a whole group
/// ([`StructureFile::begin`] … [`StructureFile::end`], e.g. a drag).
#[derive(Debug, Clone)]
pub struct StructureFile {
    /// Where it is read from and saved to (none: not saved yet).
    path: Option<PathBuf>,
    doc: DocumentMut,
    /// The text of `doc`, written once per revision (asked for every frame:
    /// hashed, compared with the saved one).
    text: OnceLock<String>,
    /// Changes with every change of the text, unique among the files: what
    /// is worked out from the file is kept while it stays.
    revision: u64,
    /// The structure as last read (kept while the text does not read).
    read: Arc<StructureType>,
    /// Why the text does not read now.
    error: Option<String>,
    /// The text on disk (unsaved changes when different).
    saved: String,
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// A group of edits open: the text before it.
    group: Option<Step>,
}

impl StructureFile {
    /// The file `path`, read (refused when it does not read).
    pub fn open(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut f = Self::parse(&text, Some(path))?;
        f.saved = text;
        Ok(f)
    }

    /// From `text` (a file `path` or none: not saved yet). Points the file
    /// places along a member (written before 2026-10-21) are put on a
    /// joint inserted there, as the structure reads them
    /// ([`crate::links::insert_joints_on_members`]): the file shows
    /// changes not saved yet.
    pub fn parse(text: &str, path: Option<&Path>) -> Result<Self, String> {
        let mut doc: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
        let name = path.unwrap_or(Path::new("structure.ohls"));
        let mut read = StructureType::parse(name, text).map_err(|e| e.to_string())?;
        if crate::links::insert_joints_on_members(&mut doc)? {
            read = StructureType::parse(name, &doc.to_string()).map_err(|e| e.to_string())?;
        }
        crate::body::check(&read)?;
        Ok(Self {
            path: path.map(Path::to_path_buf),
            doc,
            text: OnceLock::new(),
            revision: next_revision(),
            read: Arc::new(read),
            error: None,
            saved: if path.is_some() {
                text.to_owned()
            } else {
                String::new()
            },
            undo: Vec::new(),
            redo: Vec::new(),
            group: None,
        })
    }

    /// Where it is saved.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The text as edited.
    pub fn text(&self) -> String {
        self.text_ref().to_owned()
    }

    /// The text as edited, without a copy.
    pub fn text_ref(&self) -> &str {
        self.text.get_or_init(|| self.doc.to_string())
    }

    /// The text's revision: another after every change (edit, undo, redo,
    /// reload), never the same for two files.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The file as edited.
    pub fn doc(&self) -> &DocumentMut {
        &self.doc
    }

    /// The structure as last read.
    pub fn structure(&self) -> &StructureType {
        &self.read
    }

    /// The structure as last read, shared (no copy).
    pub fn structure_arc(&self) -> Arc<StructureType> {
        Arc::clone(&self.read)
    }

    /// Why the text as edited does not read (the structure shown is the
    /// last that did).
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// How many edits can be undone (the place of an edit made elsewhere,
    /// e.g. in the section library, in the same history).
    pub fn undo_steps(&self) -> usize {
        self.undo.len()
    }

    /// How many undone edits can be redone.
    pub fn redo_steps(&self) -> usize {
        self.redo.len()
    }

    /// Changes not saved.
    pub fn dirty(&self) -> bool {
        self.text_ref() != self.saved
    }

    /// Makes an edit (`what`, in words): `f` changes the file given the
    /// structure as last read. One undo step when the text changed (none
    /// inside a group); the redo is dropped. A file whose recipes take a
    /// pressure of the line after the edit is raised to version 6
    /// ([`crate::raise_for_line_pressures`]).
    pub fn edit<R>(
        &mut self,
        what: &str,
        f: impl FnOnce(&mut DocumentMut, &StructureType) -> R,
    ) -> R {
        let before = self.text();
        let r = f(&mut self.doc, &self.read);
        crate::raise_for_line_pressures(&mut self.doc);
        let after = self.doc.to_string();
        // `f` had the document: its text is written again in any case.
        self.text = OnceLock::new();
        if after != before {
            if self.group.is_none() {
                self.push_undo(Step {
                    what: what.to_owned(),
                    text: before,
                });
            }
            self.redo.clear();
            self.reread(&after);
        }
        r
    }

    /// Opens a group: the edits until [`StructureFile::end`] are one undo
    /// step (`what`). A group already open stays.
    pub fn begin(&mut self, what: &str) {
        if self.group.is_none() {
            self.group = Some(Step {
                what: what.to_owned(),
                text: self.text(),
            });
        }
    }

    /// Closes the group: one undo step when it changed the text.
    pub fn end(&mut self) {
        if let Some(step) = self.group.take()
            && step.text != self.text_ref()
        {
            self.push_undo(step);
        }
    }

    /// A group is open.
    pub fn in_group(&self) -> bool {
        self.group.is_some()
    }

    fn push_undo(&mut self, step: Step) {
        self.undo.push(step);
        if self.undo.len() > UNDO_STEPS {
            self.undo.remove(0);
        }
    }

    /// What undo would take back.
    pub fn undo_name(&self) -> Option<&str> {
        self.undo.last().map(|s| s.what.as_str())
    }

    /// What redo would make again.
    pub fn redo_name(&self) -> Option<&str> {
        self.redo.last().map(|s| s.what.as_str())
    }

    /// Takes back the last edit; what it was.
    pub fn undo(&mut self) -> Option<String> {
        self.end();
        let step = self.undo.pop()?;
        let now = self.text();
        self.set_text(&step.text);
        self.redo.push(Step {
            what: step.what.clone(),
            text: now,
        });
        Some(step.what)
    }

    /// Makes the last edit taken back again; what it was.
    pub fn redo(&mut self) -> Option<String> {
        self.end();
        let step = self.redo.pop()?;
        let now = self.text();
        self.set_text(&step.text);
        self.push_undo(Step {
            what: step.what.clone(),
            text: now,
        });
        Some(step.what)
    }

    fn set_text(&mut self, text: &str) {
        if let Ok(doc) = text.parse::<DocumentMut>() {
            self.doc = doc;
            self.text = OnceLock::new();
        }
        self.reread(text);
    }

    fn reread(&mut self, text: &str) {
        self.revision = next_revision();
        let path = self
            .path
            .clone()
            .unwrap_or_else(|| PathBuf::from("structure.ohls"));
        match StructureType::parse(&path, text)
            .map_err(|e| e.message)
            .and_then(|t| crate::body::check(&t).map(|()| t))
        {
            Ok(t) => {
                self.read = Arc::new(t);
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// Saves to its file (or `to`, which it is then saved to from now on).
    /// Refused while the text does not read.
    pub fn save(&mut self, to: Option<&Path>) -> Result<PathBuf, String> {
        self.end();
        if let Some(e) = &self.error {
            return Err(format!("the file does not read: {e}"));
        }
        let path = to
            .map(Path::to_path_buf)
            .or_else(|| self.path.clone())
            .ok_or("no file yet: Save as")?;
        let text = self.text();
        StructureType::parse(&path, &text).map_err(|e| e.to_string())?;
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
        self.saved = text;
        self.path = Some(path.clone());
        Ok(path)
    }
}
