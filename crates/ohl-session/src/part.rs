// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A session over a part of a line (country server, phase 3): the seed of
//! the desktop's partial replica (plan §5.3). The server and the
//! single-file desktop keep loading everything; this is the other way.
//!
//! [`Session::write_part`] writes a **working set** of a line — the objects
//! asked for, and what they refer to (their structures, sections, jumpers
//! tapped, alignment points, routes: the **context**) — with the line's
//! settings and library files into a project file of its own; the part
//! is noted in it (`ols_meta` key `part`, the objects of the working set).
//! A session opened on that file is a session like any other, over fewer
//! objects: its commands are applied in place and validated locally on
//! what it holds. What it cannot see is kept from it:
//!
//! - objects of the context are not changed (they belong to work outside
//!   the working set);
//! - deletions and renames (and the commands made of them: replacing a
//!   structure or the line, renumbering a wire point, relabelling a
//!   point, renaming a system) are refused: what they take with them or
//!   must follow may be outside the part;
//! - an object of the working set may be referred to by objects outside
//!   it (another route's section through a tower of the working set):
//!   the part's validation cannot see them. They are recorded in the
//!   part (`ols_meta` key `part_users`: by structure, each object's kind,
//!   id, routes, the sets it uses and the clamps it hangs on; phase 8)
//!   and the rules of shared towers ([`crate::shared`], rules 1–7) are
//!   checked against them; their own validation (rule 8) is not. A
//!   change made in a part counts once the session holding the whole
//!   line has applied and validated it (the server, phase 5) — the
//!   part's own check is a preview, said so where it is shown.

use std::collections::BTreeSet;
use std::path::Path;

use ohl_line::Line;
use ohl_line::index::{Item, LineIndex, Of, Target};

use crate::in_place::{kind_of, of_kind};
use crate::{Command, Kind, Session, SessionError, refused};

/// The `ols_meta` key holding a part's working set.
pub const PART_KEY: &str = "part";

/// What a part written holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartSummary {
    /// Objects of the working set.
    pub wanted: usize,
    /// Objects of the context (referred to, not to be changed there).
    pub context: usize,
}

impl Session {
    /// Writes the working set `wanted` (objects of the line worked on) and
    /// what it refers to into a new project file at `path` (refused when it
    /// exists), with the line's settings and library files, as one import
    /// by `user` (its admin). Refused: an object not in the line, a part
    /// that is not a valid line by itself.
    pub fn write_part(
        &self,
        wanted: &BTreeSet<(Kind, String)>,
        path: &Path,
        user: &str,
    ) -> Result<PartSummary, SessionError> {
        if path.exists() {
            return Err(refused(format!("{}: exists already", path.display())));
        }
        let index = match &self.index {
            Some(index) => std::borrow::Cow::Borrowed(index),
            None => std::borrow::Cow::Owned(LineIndex::of(&self.line)),
        };
        let mut keep: BTreeSet<(Of, String)> = BTreeSet::new();
        let mut queue: Vec<(Of, String)> = Vec::new();
        for (kind, id) in wanted {
            let of = of_kind(*kind)
                .ok_or_else(|| refused(format!("a {} is not part of a line", kind.as_str())))?;
            if index.position(of, id).is_none() {
                return Err(refused(format!("no {} {id}", kind.as_str())));
            }
            queue.push((of, id.clone()));
        }
        // What they refer to, all the way.
        while let Some((of, id)) = queue.pop() {
            if !keep.insert((of, id.clone())) {
                continue;
            }
            let Some(item) = index.get(&self.line, of, &id) else {
                continue;
            };
            for t in item.targets() {
                let (of, id) = match t {
                    Target::Object(of, id) => (of, id),
                    Target::Point(structure, _) => (Of::Structure, structure),
                };
                if index.position(of, &id).is_some() && !keep.contains(&(of, id.clone())) {
                    queue.push((of, id));
                }
            }
        }
        let line = part_of(&self.line, &index, &keep);
        let mut part = Session::open(path)?;
        part.import(&line, user)?;
        // The library files, as one command.
        let libraries: Vec<Command> = self
            .objects
            .iter()
            .filter(|((k, _), _)| *k == Kind::Library)
            .map(|((_, id), o)| Command::Put {
                kind: Kind::Library,
                id: id.clone(),
                value: (*o.value).clone(),
                ord: Some(o.ord),
            })
            .collect();
        if !libraries.is_empty() {
            part.apply_as(
                user,
                Command::Batch {
                    commands: libraries,
                },
                "import",
                None,
                false,
                false,
            )?;
        }
        let listed: Vec<(Kind, String)> = wanted.iter().cloned().collect();
        part.conn.set_meta(
            PART_KEY,
            &serde_json::to_string(&listed).expect("serialises"),
        )?;
        part.part = Some(wanted.clone());
        // The objects of the line using its structures that it does not
        // hold (phase 8, [`crate::shared`]): the rules of shared towers
        // are checked against them in the part.
        let users: std::collections::BTreeMap<String, Vec<crate::shared::User>> = wanted
            .iter()
            .filter(|(k, _)| *k == Kind::Structure)
            .map(|(_, id)| (id.clone(), self.users_outside(&index, id, &keep)))
            .filter(|(_, u)| !u.is_empty())
            .collect();
        if !users.is_empty() {
            part.conn.set_meta(
                crate::shared::PART_USERS_KEY,
                &serde_json::to_string(&users).expect("serialises"),
            )?;
            part.part_users = users;
        }
        part.checkpoint()?;
        Ok(PartSummary {
            wanted: wanted.len(),
            context: keep.len() - wanted.len(),
        })
    }

    /// The working set when the session is over a part ([`mod@crate::part`]).
    pub fn part(&self) -> Option<&BTreeSet<(Kind, String)>> {
        self.part.as_ref()
    }

    /// Reads the part noted in the project file, if any.
    pub(crate) fn read_part(&mut self) -> Result<(), SessionError> {
        self.part = match self.conn.meta(PART_KEY)? {
            Some(text) => {
                let listed: Vec<(Kind, String)> = serde_json::from_str(&text)
                    .map_err(|e| SessionError::Store(format!("part: {e}")))?;
                Some(listed.into_iter().collect())
            }
            None => None,
        };
        Ok(())
    }

    /// In a part: refuses a user's command it cannot see the whole of (a
    /// deletion, a rename and what is made of them).
    pub(crate) fn check_part_command(&self, command: &Command) -> Result<(), SessionError> {
        if self.part.is_none() {
            return Ok(());
        }
        let whole = |what: &str| {
            Err(refused(format!(
                "{what} is made on the whole line: in a working set what it takes with it \
                 or must follow may be outside it"
            )))
        };
        match command {
            Command::Batch { commands } => {
                for c in commands {
                    self.check_part_command(c)?;
                }
                Ok(())
            }
            Command::Delete { kind, .. } if of_kind(*kind).is_some() => whole("a deletion"),
            Command::DeleteStructure { .. } | Command::DeleteAttachment { .. } => {
                whole("a deletion")
            }
            Command::Rename { .. }
            | Command::RenameAttachment { .. }
            | Command::RenamePoint { .. }
            | Command::RenameSystem { .. } => whole("a rename"),
            Command::ReplaceStructure { .. } => whole("replacing a structure"),
            Command::ReplaceLine { .. } => whole("replacing the line"),
            _ => Ok(()),
        }
    }

    /// In a part: refuses changing an object of the context (of the line,
    /// there and not in the working set).
    pub(crate) fn check_part_objects(&self, prims: &[Command]) -> Result<(), SessionError> {
        let Some(wanted) = &self.part else {
            return Ok(());
        };
        for p in prims {
            let (Command::Put { kind, id, .. } | Command::Delete { kind, id, .. }) = p else {
                continue;
            };
            let key = (*kind, id.clone());
            if of_kind(*kind).is_some() && self.objects.contains_key(&key) && !wanted.contains(&key)
            {
                return Err(refused(format!(
                    "{} {id} is outside the working set (it is there because the working set \
                     refers to it): it is changed on the whole line",
                    kind.as_str()
                )));
            }
        }
        Ok(())
    }

    /// Requires `user` to be at least `role` (re-exported for the part's
    /// tests).
    #[cfg(test)]
    pub(crate) fn part_role(&self, user: &str, role: crate::Role) -> Result<(), SessionError> {
        self.require(user, role)
    }
}

/// The line `line` holding only the objects `keep`, in their order, with
/// its settings.
fn part_of(line: &Line, index: &LineIndex, keep: &BTreeSet<(Of, String)>) -> Line {
    let mut part = line.clone();
    part.routes.clear();
    part.alignments.clear();
    part.alignment_points.clear();
    part.structures.clear();
    part.sections.clear();
    part.jumpers.clear();
    part.spacers.clear();
    for of in Of::ALL {
        let mut at: Vec<usize> = keep
            .iter()
            .filter(|(k, _)| *k == of)
            .filter_map(|(_, id)| index.position(of, id))
            .collect();
        at.sort_unstable();
        for k in at {
            let id = match of {
                Of::Route => &line.routes[k].id,
                Of::Alignment => &line.alignments[k].id,
                Of::AlignmentPoint => &line.alignment_points[k].id,
                Of::Structure => &line.structures[k].id,
                Of::Section => &line.sections[k].id,
                Of::Jumper => &line.jumpers[k].id,
                Of::Spacer => &line.spacers[k].id,
            };
            match index.get(line, of, id) {
                Some(Item::Route(x)) => part.routes.push(x),
                Some(Item::Alignment(x)) => part.alignments.push(x),
                Some(Item::AlignmentPoint(x)) => part.alignment_points.push(x),
                Some(Item::Structure(x)) => part.structures.push(*x),
                Some(Item::Section(x)) => part.sections.push(*x),
                Some(Item::Jumper(x)) => part.jumpers.push(*x),
                Some(Item::Spacer(x)) => part.spacers.push(x),
                None => {}
            }
        }
    }
    // Starts on sections left out go (as a line read drops them).
    part.drop_stale_circuit_starts();
    let _ = kind_of;
    part
}
