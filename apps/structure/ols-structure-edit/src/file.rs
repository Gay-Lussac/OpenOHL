// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A structure file being edited: its text, the structure as last read,
//! the undo and redo of whole edits, saving.

use std::path::{Path, PathBuf};

use ols_structures::{LibraryItem, StructureType};
use toml_edit::DocumentMut;

/// Most edits kept for undo.
const UNDO_STEPS: usize = 500;

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
    /// The structure as last read (kept while the text does not read).
    read: StructureType,
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

    /// From `text` (a file `path` or none: not saved yet).
    pub fn parse(text: &str, path: Option<&Path>) -> Result<Self, String> {
        let doc: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
        let read = StructureType::parse(path.unwrap_or(Path::new("structure.olss")), text)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            path: path.map(Path::to_path_buf),
            doc,
            read,
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
        self.doc.to_string()
    }

    /// The file as edited.
    pub fn doc(&self) -> &DocumentMut {
        &self.doc
    }

    /// The structure as last read.
    pub fn structure(&self) -> &StructureType {
        &self.read
    }

    /// Why the text as edited does not read (the structure shown is the
    /// last that did).
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Changes not saved.
    pub fn dirty(&self) -> bool {
        self.text() != self.saved
    }

    /// Makes an edit (`what`, in words): `f` changes the file given the
    /// structure as last read. One undo step when the text changed (none
    /// inside a group); the redo is dropped.
    pub fn edit<R>(
        &mut self,
        what: &str,
        f: impl FnOnce(&mut DocumentMut, &StructureType) -> R,
    ) -> R {
        let before = self.doc.to_string();
        let r = f(&mut self.doc, &self.read);
        let after = self.doc.to_string();
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
                text: self.doc.to_string(),
            });
        }
    }

    /// Closes the group: one undo step when it changed the text.
    pub fn end(&mut self) {
        if let Some(step) = self.group.take()
            && step.text != self.doc.to_string()
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
        let now = self.doc.to_string();
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
        let now = self.doc.to_string();
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
        }
        self.reread(text);
    }

    fn reread(&mut self, text: &str) {
        let path = self
            .path
            .clone()
            .unwrap_or_else(|| PathBuf::from("structure.olss"));
        match StructureType::parse(&path, text) {
            Ok(t) => {
                self.read = t;
                self.error = None;
            }
            Err(e) => self.error = Some(e.message),
        }
    }

    /// The file changed on disk: read again (the history stays, the
    /// reload one undo step), unless there are unsaved changes.
    pub fn reload(&mut self) -> Result<bool, String> {
        let Some(path) = self.path.clone() else {
            return Ok(false);
        };
        if self.dirty() {
            return Ok(false);
        }
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if text == self.saved {
            return Ok(false);
        }
        StructureType::parse(&path, &text).map_err(|e| e.to_string())?;
        let before = self.doc.to_string();
        self.push_undo(Step {
            what: "changed on disk".into(),
            text: before,
        });
        self.redo.clear();
        self.set_text(&text);
        self.saved = text;
        Ok(true)
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
