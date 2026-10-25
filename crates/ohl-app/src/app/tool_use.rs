// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The tools used: the cross-section, members picked, snaps, the held tool
//! used at a point, the Link tool.

use super::*;

impl Ready {
    /// Opens the Cross-section window (C, View › Cross-section…): at the
    /// structure selected, mid-span of the span selected, else the start
    /// of the alignment the profile shows.
    pub fn open_cut_dialog(&mut self) {
        let alignment = self.profile_alignment();
        let width = self.drawings.line_width();
        self.xsections
            .open_dialog(&self.model, &self.selection, alignment, width);
        self.popup_open = false;
    }

    /// The first alignment of what the profile shows (by default the
    /// route with most structures).
    pub(super) fn profile_alignment(&mut self) -> Option<String> {
        let views = self.xsections.alignments(&self.model.line);
        let show = self
            .profile
            .show
            .clone()
            .unwrap_or_else(|| crate::profile::default_show(&self.model.line, &views));
        views
            .iter()
            .find(|v| match &show {
                crate::profile::Show::Route(r) => &v.route == r,
                crate::profile::Show::Alignment(a) => &v.id == a,
                crate::profile::Show::All => true,
            })
            .map(|v| v.id.clone())
    }

    /// Opens cut `spec` in a tab of its own.
    pub fn open_cut(&mut self, spec: crate::xsection::CutSpec) {
        let id = self.xsections.open(spec);
        self.open_page = Some((Tab::CrossSection(id), None));
    }

    /// Opens a cut at structure `id` at once (its menu: Cross-section
    /// here), the depth by default, the line width across.
    pub fn cross_section_at(&mut self, id: &str) {
        let views = self.xsections.alignments(&self.model.line);
        let preferred = self.profile_alignment().unwrap_or_default();
        let d = crate::xsection::Dialog::new(
            &self.model,
            &views,
            &Selection::Structure(id.to_owned()),
            Some(preferred),
            self.drawings.line_width(),
        );
        if let Some(spec) = d.spec(&views) {
            self.open_cut(spec);
        }
    }

    /// Selects member `member` of structure `structure` in the structure
    /// screen (the Structure types page's pylon workspace) when it is open
    /// on the structure's type; whether it was.
    pub fn select_member(&mut self, structure: &str, member: &str) -> bool {
        let Some(type_id) = self
            .model
            .line
            .structure(structure)
            .and_then(|s| s.structure_type.clone())
        else {
            return false;
        };
        let Some(page) = self
            .libraries
            .get_mut(&crate::library::LibraryKind::StructureTypes)
        else {
            return false;
        };
        let w = &mut page.workspace;
        if !w.on || w.shown().is_none_or(|t| t.id != type_id) {
            return false;
        }
        w.selected = vec![crate::type_workspace::Handle::Member(member.to_owned())];
        w.redraw();
        true
    }

    /// What can be right-clicked now: the Snap menu's choice, and strings
    /// while a tool that works on them is held.
    pub fn effective_snaps(&self) -> crate::selection::Snaps {
        let mut s = self.snaps.clone();
        if self.hotbar.tool().picks_strings() {
            s.0.insert(crate::selection::Snap::Attachment);
        }
        if self.hotbar.tool().picks_points() {
            s.0.insert(crate::selection::Snap::Point);
        }
        // The rigid links shown are clicked like their attachment points.
        if self.visible.contains(&Layer::RigidLinks) {
            s.0.insert(crate::selection::Snap::RigidLink);
        }
        if self.hotbar.tool().picks_spans() {
            s.0.insert(crate::selection::Snap::Span);
        }
        if self.hotbar.tool().picks_jumpers() {
            s.0.insert(crate::selection::Snap::Jumper);
        }
        if self.hotbar.tool().picks_alignment() {
            s.0.insert(crate::selection::Snap::AlignmentPoint);
        }
        // New alignment points snap onto the survey and LiDAR points shown,
        // and onto structures.
        if matches!(
            self.hotbar.tool(),
            crate::tools::Tool::NewAlignment
                | crate::tools::Tool::AddAlignment
                | crate::tools::Tool::EditAlignment
        ) {
            s.0.insert(crate::selection::Snap::Structure);
            if self.visible.contains(&Layer::Ground) {
                s.0.insert(crate::selection::Snap::SurveyPoint);
            }
            if self.visible.contains(&Layer::Points) {
                s.0.insert(crate::selection::Snap::LidarPoint);
            }
        }
        s
    }

    /// Uses the held tool on what was clicked with the tool button in a
    /// view (a right-click, or a left-click with the option "Tools act on
    /// left-click"): Select
    /// selects, Go to opens its window, the editing tools act on the object
    /// (or, Add structure, on the ground point).
    pub fn use_tool(&mut self, u: crate::selection::Use) {
        // A window the click opens, or one it changes to another object,
        // opens next to the click (not where an earlier one was, maybe
        // off screen).
        let before = (
            self.editor.clone(),
            self.goto.clone(),
            self.coord_dialog.clone(),
            self.move_dialog.clone(),
            self.point_delete.clone(),
            self.spread_delete.clone(),
        );
        self.use_tool_at(u);
        log::info!(
            "{}: window {}{}",
            self.hotbar.tool().name(),
            self.editor
                .as_ref()
                .map_or("none".to_owned(), |f| format!("\"{}\"", f.title())),
            self.status
                .as_ref()
                .map_or(String::new(), |(s, _)| format!("; status: {s}"))
        );
        if (self.editor.is_some() && self.editor != before.0)
            || (self.goto.is_some() && self.goto != before.1)
            || (self.coord_dialog.is_some() && self.coord_dialog != before.2)
            || (self.move_dialog.is_some() && self.move_dialog != before.3)
            || (self.point_delete.is_some() && self.point_delete != before.4)
            || (self.spread_delete.is_some() && self.spread_delete != before.5)
        {
            self.popup_open = false;
        }
    }

    pub(super) fn use_tool_at(&mut self, u: crate::selection::Use) {
        use crate::tools::Tool;
        let tool = self.hotbar.tool();
        // The Note tool: a note's window on what was clicked.
        if tool == Tool::Note {
            let anchor = crate::measure::anchor_of(&self.model, self.case, &u);
            let pick = anchor
                .as_ref()
                .and_then(|a| crate::measure::pick_of(&self.model, self.case, a));
            match (anchor, pick) {
                (Some(anchor), Some(pick)) => {
                    self.measuring.note = Some(crate::measure::NoteDraft {
                        anchor,
                        what: pick.what,
                        text: String::new(),
                    });
                }
                _ => self.hint(tool),
            }
            return;
        }
        // A measuring tool: a pick (no editing rights needed).
        if let Some(kind) = tool.measure() {
            let case_name = self
                .model
                .cases
                .get(self.case)
                .map_or(String::new(), |c| c.case.name.clone());
            match crate::measure::resolve(&self.model, self.case, &u) {
                Some(pick) => {
                    self.measuring.click(&self.model, kind, &case_name, pick);
                    self.status = None;
                }
                None => self.hint(tool),
            }
            return;
        }
        let target = u.target;
        match tool {
            Tool::Select => {
                self.select(target);
                return;
            }
            Tool::GoTo => {
                self.goto = Some(match &target {
                    Selection::Structure(id) => id.clone(),
                    _ => self
                        .model
                        .line
                        .structures
                        .first()
                        .map(|s| s.id.clone())
                        .unwrap_or_default(),
                });
                return;
            }
            _ => {}
        }
        // A LiDAR point: its window (read-only for a viewer).
        if tool == Tool::Edit
            && let Selection::LidarPoint(p) = &target
        {
            match crate::lidar_edit::PointsForm::of(&self.model, &[*p]) {
                Ok(f) => self.editor = Some(crate::editors::Form::Points(f)),
                Err(e) => self.status = Some((e, true)),
            }
            self.select(target);
            return;
        }
        if let Err(why) = self.model.can_edit() {
            self.status = Some((format!("{}: read-only — {why}", tool.name()), true));
            return;
        }
        // A spread is deleted as itself; the other tools act on its span
        // (adding on it where the wire was clicked, editing its section).
        let target = match target {
            Selection::Spread { section, index } if tool != Tool::Delete => {
                match spread_span(&self.model.line, &section, index) {
                    Some(span) => Selection::Span { section, span },
                    None => Selection::Spread { section, index },
                }
            }
            t => t,
        };
        use crate::editors::{Form, SectionForm, StringForm, StructureForm};
        let model = &self.model;
        match (tool, &target) {
            (Tool::Move, Selection::Structure(id)) => {
                let id = id.clone();
                self.select(target);
                self.use_tool_on(&id);
            }
            (Tool::Replace, Selection::Structure(id)) => {
                let id = id.clone();
                self.select(target);
                self.start_replace(&id);
            }
            // A point on several alignments: which to take it off (a window).
            (Tool::Delete, Selection::AlignmentPoint { id })
                if self.model.line.point_alignments(id).len() > 1 =>
            {
                let on = self.model.line.point_alignments(id);
                self.point_delete = Some((id.clone(), on.into_iter().map(|a| (a, true)).collect()));
                self.select(target);
            }
            (Tool::Delete, _) => {
                if deletable(&self.model.line, &target) {
                    self.select(target.clone());
                    self.delete_object(&target);
                } else {
                    self.hint(tool);
                }
            }
            // Its pylon: the type in the pylon workspace, a structure
            // without a type in the structure editor (as Strings and points… in
            // Properties).
            (Tool::Edit, Selection::Structure(id)) => {
                let (id, typed) = (
                    id.clone(),
                    model
                        .line
                        .structure(id)
                        .and_then(|s| s.structure_type.clone()),
                );
                self.select(target);
                match typed {
                    Some(t) => self.edit_type(&t),
                    None => self.edit_structure(&id),
                }
                // The page or the structure screen needs the cursor.
                self.release_mouse = true;
            }
            (Tool::Edit, Selection::Attachment { structure, set }) => {
                self.editor = StringForm::of(model, structure, *set).map(Form::String);
                self.select(target);
            }
            (Tool::Edit, Selection::String { structure, label }) => {
                self.editor = StringForm::of_string(model, structure, label).map(Form::String);
                self.select(target);
            }
            (Tool::Edit, Selection::AlignmentPoint { id }) => {
                self.editor = crate::editors::AlignmentForm::of(model, id).map(Form::Alignment);
                self.select(target);
            }
            // A point: its own window (place, label, object).
            (Tool::Edit, Selection::JumperPoint { jumper, label }) => {
                self.editor =
                    crate::span_tool::SpanForm::edit_point(model, self.case, jumper, true, label)
                        .map(Form::Span);
                self.select(target);
            }
            (Tool::Edit, Selection::SpanPoint { section, label }) => {
                self.editor =
                    crate::span_tool::SpanForm::edit_point(model, self.case, section, false, label)
                        .map(Form::Span);
                self.select(target);
            }
            (Tool::Edit, Selection::Jumper(id)) => {
                // Its window, as a section's (it frees the mouse).
                self.editor =
                    crate::editors::JumperForm::of(model, id).map(|f| Form::Jumper(Box::new(f)));
                self.select(target);
            }
            (Tool::AddJumper, Selection::Structure(id)) => {
                let (jumpers, unpaired) = crate::editors::missing_jumpers(model, id);
                let id = id.clone();
                if jumpers.is_empty() {
                    self.status = Some((
                        format!(
                            "Add jumper: none missing at {id}{}",
                            if unpaired.is_empty() {
                                String::new()
                            } else {
                                format!(" ({})", unpaired.join("; "))
                            }
                        ),
                        !unpaired.is_empty(),
                    ));
                    return;
                }
                let n = jumpers.len();
                let commands = jumpers
                    .iter()
                    .map(|j| ohl_session::Command::Put {
                        kind: ohl_session::Kind::Jumper,
                        id: j.id.clone(),
                        value: serde_json::to_value(j).expect("a jumper"),
                        ord: None,
                    })
                    .collect();
                if self.perform(crate::edit::Action::Command {
                    what: format!("added {n} jumpers at {id}"),
                    command: ohl_session::Command::Batch { commands },
                }) && !unpaired.is_empty()
                {
                    self.status = Some((
                        format!(
                            "added {n} jumpers at {id}; not joined: {}",
                            unpaired.join("; ")
                        ),
                        false,
                    ));
                }
            }
            // A suspension string of the structure (to a wire point without
            // a set: not one holding a wire), after the first end: the
            // jumper passes through a clamp on it, in click order.
            (Tool::AddJumper, Selection::String { structure, label })
                if self.jumper_from.is_some() =>
            {
                let st = model.line.structure(structure);
                let wire = st.and_then(|s| s.string(label)).map(|x| x.wire.clone());
                let helping = st.zip(wire.as_ref()).is_some_and(|(s, w)| {
                    s.wire(w).is_some_and(|w| w.set.is_none())
                        && !s
                            .component(w)
                            .wires
                            .iter()
                            .any(|&i| s.wires[i].set.is_some())
                });
                match wire {
                    Some(w) if helping => {
                        if self
                            .jumper_via
                            .iter()
                            .any(|v| v.0 == *structure && v.1 == w)
                        {
                            self.status = Some((
                                format!("Add jumper: {structure} {w} holds the jumper already"),
                                true,
                            ));
                            return;
                        }
                        self.jumper_via.push((structure.clone(), w));
                        let via: Vec<String> = self
                            .jumper_via
                            .iter()
                            .map(|v| format!("{} {}", v.0, v.1))
                            .collect();
                        let from = self.jumper_from.clone().expect("checked");
                        self.status = Some((
                            format!(
                                "Add jumper from {} set {} through {}: right-click more suspension strings, then the other end",
                                from.structure,
                                from.set,
                                via.join(", ")
                            ),
                            false,
                        ));
                    }
                    _ => {
                        self.status = Some((
                            format!(
                                "Add jumper: {structure} {label} holds a wire (its wire point has a set): right-click a suspension string to a wire point without a set, or the other end"
                            ),
                            true,
                        ));
                    }
                }
            }
            // A tap: a clamp on a span point or on a jumper point.
            (Tool::AddJumper, Selection::SpanPoint { .. } | Selection::JumperPoint { .. }) => {
                let (tap, object) = match &target {
                    Selection::SpanPoint { section, label } => (
                        ohl_line::Tap::Span(ohl_line::SpanPointRef {
                            section: section.clone(),
                            point: label.clone(),
                        }),
                        model
                            .line
                            .sections
                            .iter()
                            .find(|s| &s.id == section)
                            .and_then(|s| s.span_points.iter().find(|p| &p.label == label))
                            .and_then(|p| p.object.clone()),
                    ),
                    Selection::JumperPoint { jumper, label } => (
                        ohl_line::Tap::Jumper {
                            jumper: jumper.clone(),
                            point: label.clone(),
                        },
                        model
                            .line
                            .jumpers
                            .iter()
                            .find(|j| &j.id == jumper)
                            .and_then(|j| j.points.iter().find(|p| &p.label == label))
                            .and_then(|p| p.object.clone()),
                    ),
                    _ => unreachable!("matched"),
                };
                if !object.is_some_and(|o| o.kind == ohl_line::ObjectKind::Clamp) {
                    self.status = Some((
                        format!(
                            "Add jumper: {} holds no clamp — put a clamp object there (Add on span, object, a tap clamp template), then start the jumper from it",
                            tap.text()
                        ),
                        true,
                    ));
                    return;
                }
                let first = match (self.jumper_from.take(), self.jumper_tap.take()) {
                    (_, Some(t)) if t == tap => {
                        self.status = Some(("Add jumper: cancelled".into(), false));
                        return;
                    }
                    (_, Some(t)) => Some(ohl_line::JumperEnd::tap(t)),
                    (Some(p), None) => Some(ohl_line::JumperEnd::Wire(p)),
                    (None, None) => None,
                };
                match first {
                    None => {
                        self.status = Some((
                            format!(
                                "Add jumper from the tap {}: right-click its other end (a string, or another tap's clamp; the same one again or Escape: cancel)",
                                tap.text()
                            ),
                            false,
                        ));
                        self.jumper_tap = Some(tap);
                    }
                    Some(from) => {
                        match crate::editors::new_tap_jumper(
                            model,
                            from,
                            ohl_line::JumperEnd::tap(tap.clone()),
                        ) {
                            Ok(j) => {
                                let what =
                                    format!("added jumper {} on the tap {}", j.id, tap.text());
                                self.add_jumper(j, what, true);
                            }
                            Err(e) => self.status = Some((e, true)),
                        }
                    }
                }
            }
            (Tool::AddJumper, Selection::Attachment { structure, set })
                if self.jumper_tap.is_some() =>
            {
                let p = ohl_line::SectionPoint::new(structure.clone(), *set);
                let tap = self.jumper_tap.take().expect("checked");
                match crate::editors::new_tap_jumper(
                    model,
                    ohl_line::JumperEnd::tap(tap.clone()),
                    ohl_line::JumperEnd::Wire(p),
                ) {
                    Ok(j) => {
                        let what = format!("added jumper {} from the tap {}", j.id, tap.text());
                        self.add_jumper(j, what, true);
                    }
                    Err(e) => self.status = Some((e, true)),
                }
            }
            (Tool::AddJumper, Selection::Attachment { structure, set }) => {
                let p = ohl_line::SectionPoint::new(structure.clone(), *set);
                // A jumper ends where a section's wire is clamped: a wire
                // point whose set no section uses is refused (a string the
                // jumper should pass through, its set given by mistake —
                // else one jumper would be laid as several).
                if !model.line.sections.iter().any(|s| s.points.contains(&p)) {
                    let wire = model
                        .line
                        .wire_point(&p)
                        .map_or(String::new(), |w| format!(" (wire point {})", w.label));
                    self.status = Some((
                        format!(
                            "Add jumper: set {set} on {structure}{wire} holds no section, so no jumper ends there. To pass the jumper through this string, untick its wire point's set in the string window (Edit on the string); then right-click it between the jumper's ends"
                        ),
                        true,
                    ));
                    return;
                }
                match self.jumper_from.take() {
                    None => {
                        self.status = Some((
                            format!(
                                "Add jumper from {structure} set {set}: right-click the suspension strings it passes through in order (if any), then the other end (the same one again or Escape: cancel)"
                            ),
                            false,
                        ));
                        self.jumper_from = Some(p);
                        self.jumper_via.clear();
                    }
                    Some(from) if from == p => {
                        self.jumper_via.clear();
                        self.status = Some(("Add jumper: cancelled".into(), false));
                    }
                    Some(from) if !self.jumper_via.is_empty() => {
                        let via = std::mem::take(&mut self.jumper_via);
                        match crate::editors::new_jumper_through(model, &from, &p, &via) {
                            Ok(j) => {
                                let what = format!(
                                    "added jumper {} through {} suspension strings",
                                    j.id,
                                    via.len()
                                );
                                // Its window: the lengths between its clamps.
                                self.add_jumper(j, what, true);
                            }
                            Err(e) => self.status = Some((e, true)),
                        }
                    }
                    Some(from) => match crate::editors::new_jumper(model, &from, &p, &[]) {
                        Ok(j) => {
                            let what = format!("added jumper {}", j.id);
                            self.add_jumper(j, what, false);
                        }
                        Err(e) => self.status = Some((e, true)),
                    },
                }
            }
            (Tool::NewAlignment | Tool::AddAlignment | Tool::EditAlignment, _) => {
                use crate::alignment_tools::{CoordDialog, CoordFor};
                let stored = model.stored_line().unwrap_or(&model.line);
                // What was right-clicked gives the point: an alignment
                // point (used as it is: a junction), a structure (tied
                // under it), a survey or LiDAR point shown (its
                // coordinates); else the coordinates are asked for. A new
                // point has no id yet.
                let point = match &target {
                    Selection::AlignmentPoint { id } => stored.alignment_point(id).cloned(),
                    Selection::Structure(id) => {
                        model.line.structure(id).map(|s| ohl_line::AlignmentPoint {
                            structure: Some(s.id.clone()),
                            ..ohl_line::AlignmentPoint::free("", s.position)
                        })
                    }
                    Selection::SurveyPoint(_) | Selection::LidarPoint(_) => {
                        crate::selection::position(model, &target)
                            .map(|p| ohl_line::AlignmentPoint::free("", p))
                    }
                    _ => None,
                };
                match tool {
                    // Always its window: the coordinates (from what was
                    // clicked) and whether it joins the nearest alignment.
                    Tool::AddAlignment => match point {
                        Some(p) if !p.id.is_empty() => {
                            self.status = Some((
                                format!("there is an alignment point here already: {}", p.id),
                                true,
                            ));
                        }
                        Some(p) => {
                            let mut d = CoordDialog::new(CoordFor::AddAlignment, Some(p.position));
                            d.structure = p.structure;
                            self.coord_dialog = Some(d);
                        }
                        None => {
                            self.coord_dialog = Some(CoordDialog::new(CoordFor::AddAlignment, u.at))
                        }
                    },
                    _ => {
                        let purpose = if tool == Tool::NewAlignment {
                            CoordFor::NewAlignment
                        } else {
                            CoordFor::Reroute
                        };
                        match point {
                            Some(p) => self.place_alignment_point(purpose, p),
                            None => self.coord_dialog = Some(CoordDialog::new(purpose, u.at)),
                        }
                    }
                }
            }
            (Tool::OnSpan, Selection::Jumper(id)) => {
                self.editor = crate::span_tool::SpanForm::on_jumper(model, self.case, id, u.on)
                    .map(Form::Span);
                self.select(target);
            }
            (Tool::OnSpan, Selection::Span { section, span }) => {
                self.editor =
                    crate::span_tool::SpanForm::new(model, self.case, section, *span, u.on)
                        .map(Form::Span);
                self.select(target);
            }
            // A spacer: from a span point to another.
            (Tool::AddString, Selection::SpanPoint { section, label }) => {
                let p = ohl_line::SpanPointRef {
                    section: section.clone(),
                    point: label.clone(),
                };
                match self.spacer_from.take() {
                    None => {
                        self.status = Some((
                            format!(
                                "Add spacer from {section} span point {label}: right-click the span point at its other end (the same one again or Escape: cancel)"
                            ),
                            false,
                        ));
                        self.spacer_from = Some(p);
                    }
                    Some(from) if from == p => {
                        self.status = Some(("Add spacer: cancelled".into(), false));
                    }
                    Some(from) => {
                        self.editor = Some(Form::Spacer(crate::span_tool::SpacerForm::new(
                            model, from, p,
                        )));
                    }
                }
                self.select(target);
            }
            (Tool::Edit, Selection::Span { section, .. }) => {
                self.editor = SectionForm::of(model, section).map(Form::Section);
                self.select(target);
            }
            (Tool::Edit, Selection::Spacer(id)) => {
                self.editor = crate::span_tool::SpacerForm::of(model, id).map(Form::Spacer);
                self.select(target);
            }
            // On a free alignment point: on it, tied to it (a point with a
            // structure already: where it is, the window gives the place).
            (Tool::AddStructure, Selection::AlignmentPoint { id }) => {
                let free = crate::editors::alignment_points(&model.line)
                    .into_iter()
                    .find(|p| p.4 == *id && p.3.is_none());
                let at = free
                    .as_ref()
                    .map(|p| p.2)
                    .or(u.at)
                    .or_else(|| model.line.alignment_point(id).map(|p| p.position))
                    .unwrap_or_else(|| structure_spot(model));
                let mut f = StructureForm::new_at(model, at);
                if let Some(p) = free {
                    f.place_on(model, (p.0, p.1));
                }
                self.editor = Some(Form::Structure(f));
                self.select(target);
            }
            // Where it was clicked; with no ground there, the first free
            // alignment point (the window gives its place).
            (Tool::AddStructure, _) => {
                let at = u.at.unwrap_or_else(|| structure_spot(model));
                self.editor = Some(Form::Structure(StructureForm::new_at(model, at)));
            }
            (
                Tool::AddString,
                Selection::Structure(id) | Selection::Attachment { structure: id, .. },
            ) => {
                self.editor = StringForm::new_on(model, id).map(Form::String);
                self.select(Selection::Structure(id.clone()));
            }
            (Tool::AddString, Selection::Point { structure, label }) => {
                self.editor = StringForm::hanging_from(model, structure, label).map(Form::String);
                self.select(target);
            }
            (Tool::Link, Selection::Point { structure, label }) => {
                match crate::link_tool::overlay(&self.model, structure, label) {
                    Ok(o) => {
                        self.status = Some((
                            format!(
                                "Link {label} of {structure}: right-click a ringed joint of its \
                                 type ({} joint(s) it may be linked to; Escape: cancel)",
                                o.joints.len()
                            ),
                            false,
                        ));
                        self.link_from = Some((structure.clone(), label.clone()));
                    }
                    Err(e) => {
                        self.link_from = None;
                        self.status = Some((format!("Link: {e}"), true));
                    }
                }
                self.select(target);
            }
            (Tool::Link, _) => {
                if self.link_from.take().is_some() {
                    self.status = Some((
                        "Link: cancelled (right-click a ringed joint to link the point)".into(),
                        false,
                    ));
                } else {
                    self.hint(tool);
                }
            }
            (Tool::StringWire, Selection::Attachment { structure, set }) => {
                let p = ohl_line::SectionPoint::new(structure.clone(), *set);
                if self.chain.last() == Some(&p) {
                    return;
                }
                if self.chain.contains(&p) {
                    self.status = Some((
                        format!("String wire: {structure} set {set} is in the section already"),
                        true,
                    ));
                    return;
                }
                self.chain.push(p);
                self.update_draft();
                self.status = Some((
                    format!(
                        "String wire: {} points — right-click the next string, Enter: section, Backspace: undo a point, Escape: cancel",
                        self.chain.len()
                    ),
                    false,
                ));
            }
            (_, Selection::None) => {
                self.status = Some((
                    format!("{}: nothing here — {}", tool.name(), tool.help()),
                    true,
                ));
            }
            _ => self.hint(tool),
        }
    }

    /// The Link tool's overlay of the 3D view: the point chosen and the
    /// joints it may be linked to (none: the tool not held, or no point
    /// chosen; a point whose structure went is dropped).
    pub fn link_overlay(&mut self) -> Option<crate::link_tool::LinkOverlay> {
        if self.hotbar.tool() != crate::tools::Tool::Link {
            self.link_from = None;
            return None;
        }
        let (id, label) = self.link_from.clone()?;
        match crate::link_tool::overlay(&self.model, &id, &label) {
            Ok(mut o) => {
                if let Some(old) = &self.view3d.link
                    && old.structure == o.structure
                    && old.label == o.label
                {
                    o.hovered.clone_from(&old.hovered);
                }
                Some(o)
            }
            Err(_) => {
                self.link_from = None;
                None
            }
        }
    }

    /// The Link tool's second click: the point chosen linked to `joint`
    /// where it is (one command).
    pub fn link_to_joint(&mut self, joint: &str) {
        let Some((id, label)) = self.link_from.take() else {
            return;
        };
        match crate::link_tool::link_command(&self.model, &id, &label, joint) {
            Ok((what, command)) => {
                self.perform(crate::edit::Action::Command { what, command });
                self.selection = Selection::Point {
                    structure: id,
                    label,
                };
            }
            Err(e) => self.status = Some((format!("Link: {e}"), true)),
        }
    }

    /// "Tool: what to right-click" in the status bar.
    pub(super) fn hint(&mut self, tool: crate::tools::Tool) {
        self.status = Some((format!("{}: {}", tool.name(), tool.help()), true));
    }
}
