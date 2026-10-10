// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Editing the model through the project's session: every change is a
//! logged command ([`ols_session::Command`]), undoable per user; after it
//! only the sections it touched are re-solved, in every weather case.
//!
//! While a structure is dragged the change is previewed without the
//! session ([`Model::preview_move`]): the line is changed in memory and the
//! shown case re-solves the sections through the structure. Releasing
//! applies one command ([`Model::apply`]); cancelling goes back to the
//! session's line ([`Model::cancel_preview`]).

use std::time::Instant;

use ols_line::{Line, Point};
use ols_session::{Applied, Command, Impact, SessionError};

use crate::model::Model;

/// Why an edit was not made.
#[derive(Debug, Clone, PartialEq)]
pub enum EditError {
    /// The model cannot be edited (a line file, no role…).
    ReadOnly(String),
    /// A deletion takes other objects with it: confirm to apply.
    NeedsConfirmation(Box<Impact>),
    /// The session refused the command (the line would not be valid…).
    Refused(String),
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::ReadOnly(why) => write!(f, "read-only: {why}"),
            EditError::NeedsConfirmation(i) => write!(f, "{}", i.summary()),
            EditError::Refused(why) => write!(f, "{why}"),
        }
    }
}

impl From<SessionError> for EditError {
    fn from(e: SessionError) -> Self {
        match e {
            SessionError::NeedsConfirmation(i) => EditError::NeedsConfirmation(i),
            other => EditError::Refused(other.to_string()),
        }
    }
}

impl Model {
    /// Whether the model can be edited.
    pub fn can_edit(&self) -> Result<(), String> {
        self.editing.as_ref().map(|_| ()).map_err(Clone::clone)
    }

    /// The session's line, structure types resolved.
    fn session_line(&self) -> Result<Line, EditError> {
        // A read-only replica follows the server too.
        let line = match (&self.editing, &self.viewer_session) {
            (Ok(e), _) => e.session.export(),
            (Err(_), Some(s)) => s.export(),
            (Err(why), None) => return Err(EditError::ReadOnly(why.clone())),
        };
        let mut line = match &self.types {
            Some((s, i)) => line
                .resolve(s, i)
                .map_err(|e| EditError::Refused(e.to_string()))?,
            None => line,
        };
        // The creep-stretch weather cases, for sections after creep or load.
        let cases: Vec<_> = self.cases.iter().map(|c| c.case.clone()).collect();
        line.set_weather_cases(&cases);
        Ok(line)
    }

    /// Brings the model up to the session after `applied`: re-solves its
    /// dirty sections in every case. Returns a one-line report.
    pub(crate) fn sync(
        &mut self,
        what: &str,
        applied: Applied,
        started: Instant,
    ) -> Result<String, EditError> {
        let line = self.session_line()?;
        // Other creep-stretch weather cases (Criteria › Creep-Stretch, or
        // its undo): every section after creep or load changes.
        let all: Vec<String>;
        // The bimetallic conductor model too.
        let dirty = if line.creep_stretch != self.line.creep_stretch
            || line.bimetallic != self.line.bimetallic
        {
            all = line.sections.iter().map(|s| s.id.clone()).collect();
            &all
        } else {
            &applied.sections
        };
        let mut solved = 0;
        for c in &mut self.cases {
            solved += c.update(&line, &self.library, dirty);
        }
        self.line = line;
        self.line_version += 1;
        Ok(format!(
            "{what} (log entry {}): {} sections touched, {solved} re-solved over {} weather cases in {:.0?}",
            applied.seq,
            applied.sections.len(),
            self.cases.len(),
            started.elapsed()
        ))
    }

    fn user(&self) -> Result<String, EditError> {
        self.editing
            .as_ref()
            .map(|e| e.user.clone())
            .map_err(|why| EditError::ReadOnly(why.clone()))
    }

    /// Applies a command as the editing user.
    pub fn apply(&mut self, what: &str, command: Command) -> Result<String, EditError> {
        if self.remote.is_some() {
            return self.remote_request(what, ols_server::Op::Do { command });
        }
        let started = Instant::now();
        let user = self.user()?;
        let applied = self
            .editing
            .as_mut()
            .map_err(|why| EditError::ReadOnly(why.clone()))?
            .session
            .apply(&user, command)?;
        self.sync(what, applied, started)
    }

    /// Undoes the user's last command.
    pub fn undo(&mut self) -> Result<String, EditError> {
        if self.remote.is_some() {
            return self.remote_request("undo", ols_server::Op::Undo);
        }
        let started = Instant::now();
        let user = self.user()?;
        let applied = self
            .editing
            .as_mut()
            .map_err(|why| EditError::ReadOnly(why.clone()))?
            .session
            .undo(&user)?;
        self.sync("undo", applied, started)
    }

    /// Redoes the user's last undo.
    pub fn redo(&mut self) -> Result<String, EditError> {
        if self.remote.is_some() {
            return self.remote_request("redo", ols_server::Op::Redo);
        }
        let started = Instant::now();
        let user = self.user()?;
        let applied = self
            .editing
            .as_mut()
            .map_err(|why| EditError::ReadOnly(why.clone()))?
            .session
            .redo(&user)?;
        self.sync("redo", applied, started)
    }

    /// The sections through structure `id`.
    fn sections_through(&self, id: &str) -> Vec<String> {
        self.line
            .sections
            .iter()
            .filter(|s| s.points.iter().any(|p| p.structure == id))
            .map(|s| s.id.clone())
            .collect()
    }

    /// Previews structure `id` at `position` in case `case` (no command
    /// yet): only the sections through it are re-solved.
    pub fn preview_move(&mut self, case: usize, id: &str, position: Point) {
        let Some(k) = self.line.structures.iter().position(|s| s.id == id) else {
            return;
        };
        self.line.structures[k] = self.line.structures[k].moved_to(position);
        self.line_version += 1;
        let dirty = self.sections_through(id);
        if let Some(c) = self.cases.get_mut(case) {
            c.update(&self.line, &self.library, &dirty);
        }
    }

    /// Ends a preview without a command: back to the session's line.
    pub fn cancel_preview(&mut self, case: usize, id: &str) {
        let Ok(line) = self.session_line() else {
            return;
        };
        let dirty = self.sections_through(id);
        self.line = line;
        self.line_version += 1;
        if let Some(c) = self.cases.get_mut(case) {
            c.update(&self.line, &self.library, &dirty);
        }
    }

    /// The project's line as stored (structure types not resolved), when
    /// it can be edited.
    pub fn stored_line(&self) -> Option<&Line> {
        self.editing.as_ref().ok().map(|e| e.session.line())
    }

    /// Where the ray from `a` along unit direction `d` first meets the
    /// ground (within 20 km), else the horizontal plane through
    /// `fallback_z` (when the ray goes down to it).
    pub fn ground_hit(&self, a: [f64; 3], d: [f64; 3], fallback_z: f64) -> Option<[f64; 3]> {
        let at = |t: f64| [a[0] + t * d[0], a[1] + t * d[1], a[2] + t * d[2]];
        let below = |t: f64| {
            let p = at(t);
            self.ground_at(p[0], p[1]).map(|z| p[2] <= z)
        };
        if self.ground.is_some() {
            // Steps growing with the distance (1 % of it, at least 0.5 m),
            // then halving to 1 cm.
            let (mut t0, mut t) = (0.0, 0.5);
            while t < 20_000.0 {
                if below(t) == Some(true) {
                    let mut t1 = t;
                    while t1 - t0 > 0.01 {
                        let m = 0.5 * (t0 + t1);
                        if below(m) == Some(true) {
                            t1 = m;
                        } else {
                            t0 = m;
                        }
                    }
                    let p = at(t1);
                    return Some([p[0], p[1], self.ground_at(p[0], p[1]).unwrap_or(p[2])]);
                }
                t0 = t;
                t += (0.01 * t).max(0.5);
            }
        }
        (d[2] < -1e-9).then(|| at((fallback_z - a[2]) / d[2]))
    }

    /// The ground height under (x, y), if there is ground there.
    pub fn ground_at(&self, x: f64, y: f64) -> Option<f64> {
        self.ground.as_ref().and_then(|g| g.height(x, y))
    }
}

/// An edit asked for by a view or panel; the application performs it.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Show structure `id` at `position` while it is dragged.
    PreviewMove {
        /// Structure.
        id: String,
        /// Where it is now.
        position: Point,
    },
    /// Move structure `id` to `position` (one command).
    Move {
        /// Structure.
        id: String,
        /// New position.
        position: Point,
    },
    /// Give up a preview.
    CancelPreview {
        /// Structure.
        id: String,
    },
    /// Apply a session command (made by an editor window).
    Command {
        /// What it does, in words.
        what: String,
        /// The command.
        command: Command,
    },
    /// Delete a structure's string (asks for a confirmation when sections
    /// or jumpers go with it).
    DeleteAttachment {
        /// Structure.
        structure: String,
        /// Set.
        set: u32,
        /// Confirmed.
        confirm: bool,
    },
    /// Delete an object (asks for a confirmation when it takes others).
    Delete {
        /// Kind.
        kind: ols_session::Kind,
        /// Id.
        id: String,
        /// Confirmed.
        confirm: bool,
    },
    /// Open structure type `id` in the pylon workspace (no edit by
    /// itself).
    EditType {
        /// Structure type.
        id: String,
    },
    /// Open structure `id` (one without a type) in the pylon editor (no
    /// edit by itself).
    EditStructure {
        /// Structure.
        id: String,
    },
    /// Undo the user's last command.
    Undo,
    /// Redo the user's last undo.
    Redo,
}

impl Action {
    /// The same deletion, confirmed (`None` for other actions).
    pub fn confirmed(&self) -> Option<Action> {
        match self {
            Action::Delete { kind, id, .. } => Some(Action::Delete {
                kind: *kind,
                id: id.clone(),
                confirm: true,
            }),
            Action::DeleteAttachment { structure, set, .. } => Some(Action::DeleteAttachment {
                structure: structure.clone(),
                set: *set,
                confirm: true,
            }),
            _ => None,
        }
    }
}
