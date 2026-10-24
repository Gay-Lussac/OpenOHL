// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Drafting: a wire being strung, alignments being laid out or rerouted,
//! a tool used on an object.

use super::*;

impl Ready {
    /// The wire being strung, drawn.
    pub fn update_draft(&mut self) {
        self.view3d.draft = if self.new_alignment.is_empty() {
            self.chain
                .iter()
                .filter_map(|p| self.model.line.wire_point(p).map(|w| w.position))
                .collect()
        } else {
            self.new_alignment.iter().map(|p| p.position).collect()
        };
        self.view3d.dirty.insert(Layer::Draft);
    }

    /// New alignment: a point added (or taken back); the draft and the
    /// status follow.
    pub(super) fn alignment_drafted(&mut self) {
        self.update_draft();
        // The first point of an empty line: the views go to it (it would
        // be out of sight where the empty views look).
        if self.new_alignment.len() == 1
            && self.model.line.alignments.is_empty()
            && self.model.line.structures.is_empty()
        {
            let p = self.new_alignment[0].position;
            if let Some(cam) = self.view3d.camera.as_mut() {
                cam.target = p;
                cam.distance = cam.distance.clamp(150.0, 600.0);
            }
            self.plan.centre_on(p);
        }
        let text = if self.reroute {
            format!(
                "Edit alignment: from {} through {} new point(s) — right-click the next, or another point of the alignment to join it there; Enter at an end: add them there; Backspace: undo a point, Escape: cancel",
                self.new_alignment.first().map_or("", |p| p.id.as_str()),
                self.new_alignment.len().saturating_sub(1)
            )
        } else {
            format!(
                "New alignment: {} points — right-click the next (an alignment point: a junction), Enter: create, Backspace: undo a point, Escape: cancel",
                self.new_alignment.len()
            )
        };
        self.status = Some((text, false));
    }

    /// A point for the alignment tools: the next point of the new
    /// alignment, one inserted into the nearest alignment, or a point on
    /// no alignment yet. A point without id is new; one with an id exists
    /// (shared).
    pub fn place_alignment_point(
        &mut self,
        purpose: crate::alignment_tools::CoordFor,
        p: ohl_line::AlignmentPoint,
    ) {
        use crate::alignment_tools::CoordFor;
        match purpose {
            CoordFor::NewAlignment => {
                if self.reroute {
                    self.new_alignment.clear();
                    self.reroute = false;
                }
                if !p.id.is_empty() && self.new_alignment.iter().any(|q| q.id == p.id) {
                    self.status = Some((
                        format!("New alignment: point {} is on it already", p.id),
                        true,
                    ));
                    return;
                }
                self.new_alignment.push(p);
                self.alignment_drafted();
            }
            CoordFor::Reroute => {
                if !self.reroute {
                    self.new_alignment.clear();
                }
                let stored = self.model.stored_line().unwrap_or(&self.model.line);
                let Some(first) = self.new_alignment.first().map(|q| q.id.clone()) else {
                    // It starts on a point of an alignment.
                    if p.id.is_empty() || stored.point_alignments(&p.id).is_empty() {
                        self.status = Some((
                            "Edit alignment: right-click a point of an alignment first".into(),
                            true,
                        ));
                        return;
                    }
                    self.reroute = true;
                    self.new_alignment.push(p);
                    self.alignment_drafted();
                    return;
                };
                // Another point of the same alignment: done.
                let together = !p.id.is_empty()
                    && p.id != first
                    && stored
                        .alignments
                        .iter()
                        .any(|a| a.points.contains(&first) && a.points.contains(&p.id));
                if together {
                    self.finish_reroute(Some(p.id));
                    return;
                }
                if !p.id.is_empty() && self.new_alignment.iter().any(|q| q.id == p.id) {
                    self.status = Some((
                        format!("Edit alignment: point {} is in it already", p.id),
                        true,
                    ));
                    return;
                }
                self.new_alignment.push(p);
                self.alignment_drafted();
            }
            CoordFor::AddAlignment => {
                match crate::editors::insert_alignment_point(&self.model, p) {
                    Ok((what, command, id)) => {
                        if self.perform(crate::edit::Action::Command { what, command }) {
                            self.select(Selection::AlignmentPoint { id });
                        }
                    }
                    Err(e) => self.status = Some((e, true)),
                }
            }
        }
    }

    /// Add alignment point: `p` (new) on no alignment yet.
    pub fn add_free_point(&mut self, p: ohl_line::AlignmentPoint) {
        let stored = self.model.stored_line().unwrap_or(&self.model.line);
        let id = stored.new_point_id();
        let point = ohl_line::AlignmentPoint {
            id: id.clone(),
            ..p
        };
        let command = ohl_session::Command::Put {
            kind: ohl_session::Kind::AlignmentPoint,
            id: id.clone(),
            value: serde_json::to_value(&point).expect("a point"),
            ord: None,
        };
        if self.perform(crate::edit::Action::Command {
            what: format!("new alignment point {id} (on no alignment)"),
            command,
        }) {
            self.select(Selection::AlignmentPoint { id });
        }
    }

    /// Edit alignment: the points laid out from a point of an alignment
    /// to `end` (another point of it), or — Enter, `None` — at its end. A
    /// first point in the middle of its alignments: a new alignment is
    /// offered (its route asked, as for New alignment).
    pub fn finish_reroute(&mut self, end: Option<String>) {
        let Some(first) = self.new_alignment.first().map(|p| p.id.clone()) else {
            return;
        };
        let middle: Vec<ohl_line::AlignmentPoint> = self.new_alignment[1..].to_vec();
        let stored = self.model.stored_line().unwrap_or(&self.model.line).clone();
        match crate::editors::reroute(&stored, &first, end.as_deref(), &middle) {
            Ok(crate::editors::Reroute::Done(what, command)) => {
                if self.perform(crate::edit::Action::Command { what, command }) {
                    self.new_alignment.clear();
                    self.reroute = false;
                    self.update_draft();
                }
            }
            Ok(crate::editors::Reroute::Branch) => {
                let on = stored.point_alignments(&first).join(", ");
                let points = std::mem::take(&mut self.new_alignment);
                self.reroute = false;
                self.update_draft();
                self.route_choice = Some(crate::alignment_tools::RouteChoice::new(
                    &stored,
                    points,
                    self.model.work_route.as_deref(),
                ));
                self.status = Some((
                    format!(
                        "Point {first} is in the middle of {on}: an alignment has two ends only — \
                         these points make a new alignment (choose its route)"
                    ),
                    false,
                ));
            }
            Err(e) => self.status = Some((e, true)),
        }
    }

    /// New alignment: Enter — its route is asked for (a new one offered
    /// when there is none).
    pub fn finish_new_alignment(&mut self) {
        if self.new_alignment.is_empty() {
            self.status = Some(("New alignment: right-click its first point".into(), true));
            return;
        }
        let points = std::mem::take(&mut self.new_alignment);
        let stored = self.model.stored_line().unwrap_or(&self.model.line);
        self.route_choice = Some(crate::alignment_tools::RouteChoice::new(
            stored,
            points,
            self.model.work_route.as_deref(),
        ));
    }

    /// The new alignment on `route` (made first when `new`): its new
    /// points made, the existing ones shared.
    pub fn make_new_alignment(&mut self, route: String, new: bool) {
        let Some(choice) = self.route_choice.take() else {
            return;
        };
        let stored = self.model.stored_line().unwrap_or(&self.model.line).clone();
        let mut commands = Vec::new();
        let mut line = stored.clone();
        if new {
            match crate::editors::new_route(&stored, &route, &route) {
                Ok((_, c)) => commands.push(c),
                Err(e) => {
                    self.status = Some((e, true));
                    self.route_choice = Some(choice);
                    return;
                }
            }
            line.routes.push(ohl_line::Route::new(&route, &route));
        }
        let mut ids = Vec::new();
        for p in &choice.points {
            if p.id.is_empty() {
                let id = line.new_point_id();
                let point = ohl_line::AlignmentPoint {
                    id: id.clone(),
                    ..p.clone()
                };
                commands.push(ohl_session::Command::Put {
                    kind: ohl_session::Kind::AlignmentPoint,
                    id: id.clone(),
                    value: serde_json::to_value(&point).expect("a point"),
                    ord: None,
                });
                line.alignment_points.push(point);
                ids.push(id);
            } else {
                ids.push(p.id.clone());
            }
        }
        let id = crate::editors::free_alignment_id(&line);
        let a = ohl_line::Alignment::new(&id, &route, ids.clone());
        commands.push(ohl_session::Command::Put {
            kind: ohl_session::Kind::Alignment,
            id: id.clone(),
            value: serde_json::to_value(&a).expect("an alignment"),
            ord: Some(line.alignments.len() as i64),
        });
        let command = match commands.len() {
            1 => commands.remove(0),
            _ => ohl_session::Command::Batch { commands },
        };
        if self.perform(crate::edit::Action::Command {
            what: format!("new alignment {id} on route {route}"),
            command,
        }) {
            self.update_draft();
            if let Some(first) = ids.into_iter().next() {
                self.select(Selection::AlignmentPoint { id: first });
            }
        } else {
            self.route_choice = Some(choice);
        }
    }

    /// New alignment: Escape — the points go; the tool stays held, the
    /// view keeps the mouse.
    pub fn cancel_new_alignment(&mut self) {
        self.new_alignment.clear();
        self.reroute = false;
        self.update_draft();
        self.status = Some(("New alignment: cancelled".into(), false));
    }

    /// Ends the wire being strung: its section's window (a new section, or
    /// the one whose points are picked again).
    pub fn finish_chain(&mut self) {
        use crate::editors::{Form, SectionForm};
        if self.chain.len() < 2 {
            self.status = Some(("String wire: right-click two strings or more".into(), true));
            return;
        }
        let points = std::mem::take(&mut self.chain);
        let form = match self.restring.take() {
            Some(id) => SectionForm::of(&self.model, &id).map(|mut f| {
                f.set_points(&self.model, self.case, points);
                f
            }),
            None => {
                let template = match &self.selection {
                    Selection::Span { section, .. } => Some(section.clone()),
                    _ => None,
                };
                Some(SectionForm::new_through(
                    &self.model,
                    self.case,
                    points,
                    template.as_deref(),
                ))
            }
        };
        self.editor = form.map(Form::Section);
        self.update_draft();
    }

    /// Cancels the wire being strung (Escape): its points go; the tool
    /// stays held, the view keeps the mouse.
    pub fn cancel_string_wire(&mut self) {
        self.chain.clear();
        self.restring = None;
        self.update_draft();
        self.status = Some(("String wire: cancelled".into(), false));
    }

    /// The held Move or Delete tool used on structure `id`.
    pub fn use_tool_on(&mut self, id: &str) {
        use crate::tools::Tool;
        let tool = self.hotbar.tool();
        if tool.edits()
            && let Err(why) = self.model.can_edit()
        {
            self.status = Some((format!("{}: read-only — {why}", tool.name()), true));
            return;
        }
        match tool {
            Tool::Move => {
                let target = ohl_server::lock_target(
                    &self.model.project_line(),
                    ohl_session::Kind::Structure,
                    id,
                );
                if let Some(user) = self.model.locked_by_other(&target) {
                    self.status = Some((format!("{id} is being moved by {user}"), true));
                    return;
                }
                // A preview still open goes back first (it may be this one).
                self.cancel_move_preview();
                if let Some(p) = self.model.line.structure(id).map(|s| s.position) {
                    // The window frees the mouse when it opens ([`popups`]).
                    self.move_dialog = Some(MoveDialog {
                        id: id.to_owned(),
                        start: p,
                        position: p,
                        on_ground: false,
                        shown: p,
                    });
                }
            }
            Tool::Delete => {
                self.perform(crate::edit::Action::Delete {
                    kind: ohl_session::Kind::Structure,
                    id: id.to_owned(),
                    confirm: false,
                });
            }
            _ => {}
        }
    }

    /// Ends a Move window's preview without moving.
    pub(super) fn cancel_move_preview(&mut self) {
        if let Some(d) = self.move_dialog.take()
            && d.shown != d.start
        {
            self.perform(crate::edit::Action::CancelPreview { id: d.id });
        }
    }
}
