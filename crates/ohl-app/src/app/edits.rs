// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Edits performed: jumpers added, actions performed (strings labelled,
//! library entries written), the views refreshed, objects deleted.

use super::*;

impl Ready {
    /// Adds jumper `j` (`what`: the change in words; `open_editor`: its
    /// window after). A jumper between two different circuits waits for
    /// the user first ([`crate::circuit_join`]).
    pub fn add_jumper(&mut self, j: ohl_line::Jumper, what: String, open_editor: bool) {
        if let Some(join) =
            crate::circuit_join::CircuitJoin::check(&self.model, &j, &what, open_editor)
        {
            self.status = Some((
                format!(
                    "Add jumper: {} joins two different circuits — choose in the window",
                    j.id
                ),
                false,
            ));
            self.circuit_join = Some(join);
            return;
        }
        let id = j.id.clone();
        let command = ohl_session::Command::Put {
            kind: ohl_session::Kind::Jumper,
            id: id.clone(),
            value: serde_json::to_value(&j).expect("a jumper"),
            ord: None,
        };
        self.jumper_added(
            id,
            crate::edit::Action::Command { what, command },
            open_editor,
        );
    }

    /// Performs the adding of jumper `id`, selects it, opens its window.
    pub(super) fn jumper_added(
        &mut self,
        id: String,
        action: crate::edit::Action,
        open_editor: bool,
    ) {
        if self.perform(action) {
            self.select(Selection::Jumper(id.clone()));
            if open_editor {
                self.editor = crate::editors::JumperForm::of(&self.model, &id)
                    .map(|f| crate::editors::Form::Jumper(Box::new(f)));
            }
        }
    }

    /// Replace structure on structure `id`: the structure picker, then
    /// (when not every set matches) the mapping window
    /// ([`crate::replace`]).
    pub fn start_replace(&mut self, id: &str) {
        match crate::replace::Replace::start(&self.model, id) {
            Ok(r) => {
                self.replace = Some(r);
                self.popup_open = false;
            }
            Err(e) => self.status = Some((e, true)),
        }
    }

    pub fn perform(&mut self, action: crate::edit::Action) -> bool {
        use crate::edit::Action;
        // The strings an edit puts without a library label are taken into
        // the library in the same command (one undo takes both back).
        let action = match action {
            Action::Command { what, mut command } if !self.cataloguing => {
                self.label_strings_in(&mut command);
                Action::Command { what, command }
            }
            other => other,
        };
        self.perform_one(action)
    }

    /// The structures `command` puts (in a batch too): their strings
    /// without a library label get one, new library entries written first
    /// ([`ohl_line::Line::catalogue_strings`] on the line as it will be).
    pub(super) fn label_strings_in(&mut self, command: &mut ohl_session::Command) {
        if self.model.can_edit().is_err() {
            return;
        }
        let mut puts: Vec<&mut serde_json::Value> = Vec::new();
        fn collect<'a>(c: &'a mut ohl_session::Command, out: &mut Vec<&'a mut serde_json::Value>) {
            match c {
                ohl_session::Command::Put {
                    kind: ohl_session::Kind::Structure,
                    value,
                    ..
                } => out.push(value),
                ohl_session::Command::Batch { commands } => {
                    for c in commands {
                        collect(c, out);
                    }
                }
                _ => {}
            }
        }
        collect(command, &mut puts);
        if puts.is_empty() {
            return;
        }
        let Some(mut line) = self.model.stored_line().cloned() else {
            return;
        };
        let mut put: Vec<String> = Vec::new();
        for value in &puts {
            let Ok(s) = serde_json::from_value::<ohl_line::Structure>((*value).clone()) else {
                return;
            };
            if !s.strings.iter().any(|x| x.insulator.is_none()) {
                continue;
            }
            put.push(s.id.clone());
            match line.structures.iter_mut().find(|t| t.id == s.id) {
                Some(t) => *t = s,
                None => line.structures.push(s),
            }
        }
        if put.is_empty() {
            return;
        }
        let empty_s = ohl_structures::StructureLibrary::default();
        let empty_i = ohl_structures::InsulatorLibrary::default();
        let (structures, library) = self
            .model
            .types
            .as_ref()
            .map_or((&empty_s, &empty_i), |(s, i)| (s, i));
        let Ok(resolved) = line.resolve(structures, library) else {
            return;
        };
        let mut c = line.catalogue_strings(&resolved, library);
        c.links.retain(|(s, _, _)| put.contains(s));
        let used: std::collections::BTreeSet<&str> =
            c.links.iter().map(|(_, _, id)| id.as_str()).collect();
        c.new.retain(|a| used.contains(a.id.as_str()));
        if c.is_empty() || !self.write_library_entries(&c.new) {
            return;
        }
        c.apply_to(&mut line);
        for value in puts {
            if let Some(id) = value.get("id").and_then(|v| v.as_str()).map(str::to_owned)
                && put.contains(&id)
                && let Some(s) = line.structure(&id)
            {
                *value = serde_json::to_value(s).expect("a structure");
            }
        }
    }

    /// Writes new insulator library entries as files (stored in the
    /// project, the libraries read again); false (the status says why)
    /// when one cannot be written.
    pub(super) fn write_library_entries(
        &mut self,
        new: &[ohl_structures::InsulatorAssembly],
    ) -> bool {
        if new.is_empty() {
            return true;
        }
        let dir = crate::library::LibraryKind::Insulators.new_file_dir(&self.model.sources);
        let mut first = None;
        for a in new {
            let path = dir.join(format!(
                "{}.{}",
                a.id,
                ohl_structures::insulators::INSULATOR_FILE_EXTENSION
            ));
            let written =
                std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, a.to_toml()));
            if let Err(e) = written {
                self.status = Some((format!("{}: {e}", path.display()), true));
                return false;
            }
            first.get_or_insert(path);
        }
        if let Some(path) = first {
            self.library_saved(
                crate::library::LibraryKind::Insulators,
                crate::library::PageAction::Saved(path),
            );
        }
        true
    }

    /// The model's own strings without a library label taken into the
    /// insulator library ([`ohl_line::Line::catalogue_strings`]): new
    /// entries written as library files (stored in the project), the
    /// strings labelled in one command. Only where the model can be
    /// edited; nothing to do: nothing happens.
    pub fn catalogue_strings(&mut self) {
        if self.model.can_edit().is_err() {
            return;
        }
        let Some(stored) = self.model.stored_line().cloned() else {
            return;
        };
        let empty = ohl_structures::InsulatorLibrary::default();
        let library = self.model.types.as_ref().map_or(&empty, |(_, i)| i);
        let c = stored.catalogue_strings(&self.model.line, library);
        if c.is_empty() {
            return;
        }
        if !self.write_library_entries(&c.new) {
            return;
        }
        self.cataloguing = true;
        let mut labelled = stored.clone();
        c.apply_to(&mut labelled);
        let touched: std::collections::BTreeSet<&str> =
            c.links.iter().map(|(s, _, _)| s.as_str()).collect();
        let commands = labelled
            .structures
            .iter()
            .filter(|s| touched.contains(s.id.as_str()))
            .map(|s| ohl_session::Command::Put {
                kind: ohl_session::Kind::Structure,
                id: s.id.clone(),
                value: serde_json::to_value(s).expect("a structure"),
                ord: None,
            })
            .collect();
        let what = format!(
            "strings taken into the library: {} labelled, {} new entries",
            c.links.len(),
            c.new.len()
        );
        log::info!("{what}");
        self.perform(crate::edit::Action::Command {
            what,
            command: ohl_session::Command::Batch { commands },
        });
        self.cataloguing = false;
    }

    pub(super) fn perform_one(&mut self, action: crate::edit::Action) -> bool {
        use crate::edit::{Action, EditError};
        let retry = action.confirmed();
        let result = match action {
            Action::PreviewMove { id, position } => {
                self.model.preview_move(self.case, &id, position);
                self.refresh_views();
                return true;
            }
            Action::CancelPreview { id } => {
                self.model.cancel_preview(self.case, &id);
                self.refresh_views();
                return true;
            }
            Action::EditType { id } => {
                self.edit_type(&id);
                return true;
            }
            Action::EditInStructureApp { id, structure } => {
                self.edit_in_structure_app(&id, structure.as_deref());
                return true;
            }
            Action::EditStructure { id } => {
                self.edit_structure(&id);
                return true;
            }
            Action::MakeFileAndEdit { id } => {
                self.make_file_and_edit(&id);
                return true;
            }
            Action::ReplaceStructure { id } => {
                self.start_replace(&id);
                return true;
            }
            Action::EditPlacement { id } => {
                self.editor = crate::editors::StructureForm::of(&self.model, &id)
                    .map(crate::editors::Form::Structure);
                // Opened at the mouse, over a window already open.
                self.popup_open = false;
                return true;
            }
            Action::Command { what, command } => self.model.apply(&what, command),
            Action::DeleteAttachment {
                structure,
                set,
                confirm,
            } => self.model.apply(
                &format!("deleted set {set} of {structure}"),
                ohl_session::Command::DeleteAttachment {
                    structure,
                    set,
                    phase: None,
                    confirm,
                },
            ),
            Action::Move { id, position } => self.model.apply(
                &format!("moved {id}"),
                ohl_session::Command::MoveStructure {
                    id: id.clone(),
                    position,
                },
            ),
            Action::Delete { kind, id, confirm } => self.model.apply(
                &format!("deleted {} {id}", kind.as_str()),
                ohl_session::Command::Delete { kind, id, confirm },
            ),
            Action::Undo => self.model.undo(),
            Action::Redo => self.model.redo(),
        };
        let done = match result {
            Ok(report) => {
                log::info!("{report}");
                // Structures on no route (a line with routes): said, not
                // refused.
                let line = &self.model.line;
                let lost: Vec<String> = if line.routes.is_empty() {
                    Vec::new()
                } else {
                    line.structure_routes()
                        .into_iter()
                        .filter(|(_, r)| r.is_empty())
                        .map(|(id, _)| id)
                        .collect()
                };
                let report = match lost.len() {
                    0 => report,
                    n => format!(
                        "{report}; warning: {n} structure(s) on no route ({}{}) — put them on an \
                         alignment or give their route",
                        lost.iter().take(5).cloned().collect::<Vec<_>>().join(", "),
                        if n > 5 { "…" } else { "" }
                    ),
                };
                self.status = Some((report, false));
                self.confirm = None;
                self.clearances.stale = true;
                true
            }
            Err(EditError::NeedsConfirmation(impact)) => {
                self.confirm = retry.map(|a| (impact, a));
                false
            }
            Err(e) => {
                log::warn!("{e}");
                self.status = Some((e.to_string(), true));
                false
            }
        };
        self.edit_position = None;
        if !self.selection_exists() {
            self.selection = Selection::None;
        }
        self.chain
            .retain(|p| self.model.line.wire_point(p).is_some());
        self.update_draft();
        self.refresh_views();
        done
    }

    /// After the line changed: the 3D layers of the line (not the ground,
    /// the LiDAR points and the DXF drawings: an edit does not change
    /// them, and indexing 5 million triangles took 0.3 s, a drawing of 17
    /// million vertices 0.4 s, its chunks then built again) and the
    /// profile again. The drawings are marked when one is hidden or shown
    /// ([`crate::backgrounds::menu`]); an import opens the project again.
    pub(crate) fn refresh_views(&mut self) {
        // Not what an edit of the line leaves as it is: the terrain, the
        // points, the backgrounds, and the check results (marked when
        // they go out of date, `revision`); the cut panels' slabs hold the
        // points and the ground only (made again when those change).
        self.view3d.dirty.extend(Layer::ALL.into_iter().filter(|l| {
            !matches!(
                l,
                Layer::Ground
                    | Layer::Surfaces
                    | Layer::Points
                    | Layer::Drawings
                    | Layer::Orthophotos
                    | Layer::GrowIn
                    | Layer::FallIn
                    | Layer::Isoclearance
                    | Layer::SafetyVolume
            )
        }));
        self.profile.invalidate();
        self.drawings.invalidate_preview();
    }

    /// Whether the selected object is still in the line.
    pub(super) fn selection_exists(&self) -> bool {
        let line = &self.model.line;
        match &self.selection {
            Selection::None | Selection::LidarPoint(_) | Selection::SurveyPoint(_) => true,
            Selection::Structure(id) => line.structure(id).is_some(),
            Selection::Jumper(id) => line.jumpers.iter().any(|j| &j.id == id),
            Selection::JumperPoint { jumper, label } => line
                .jumpers
                .iter()
                .any(|j| &j.id == jumper && j.points.iter().any(|p| &p.label == label)),
            Selection::Span { section, span } => line
                .sections
                .iter()
                .any(|s| &s.id == section && *span + 1 < s.points.len()),
            Selection::Attachment { structure, set } => line
                .wire_point(&ohl_line::SectionPoint::new(structure.clone(), *set))
                .is_some(),
            Selection::String { structure, label } => line
                .structure(structure)
                .is_some_and(|s| s.string(label).is_some()),
            Selection::Point { structure, label } => line
                .structure(structure)
                .is_some_and(|s| s.node(label).is_some()),
            Selection::AlignmentPoint { id } => line.alignment_point(id).is_some(),
            Selection::SpanPoint { section, label } => line
                .sections
                .iter()
                .any(|s| &s.id == section && s.span_points.iter().any(|p| &p.label == label)),
            Selection::Spread { section, index } => line
                .sections
                .iter()
                .any(|s| &s.id == section && *index < s.spreads.len()),
            Selection::Spacer(id) => line.spacers.iter().any(|s| &s.id == id),
        }
    }

    /// Deletes `target` (as the Delete tool, Delete selected): a spread,
    /// or a point a spread ends on, asks first what goes with it (the
    /// spread window); anything else is deleted (a confirmation when it
    /// takes more). False: nothing to delete.
    pub fn delete_object(&mut self, target: &Selection) -> bool {
        let line = &self.model.line;
        let ask = match target {
            Selection::Spread { section, index } => Some(SpreadDelete {
                section: section.clone(),
                spread: Some(*index),
                point: None,
            }),
            Selection::SpanPoint { section, label }
                if line
                    .sections
                    .iter()
                    .any(|s| &s.id == section && !s.spreads_at(label).is_empty()) =>
            {
                Some(SpreadDelete {
                    section: section.clone(),
                    spread: None,
                    point: Some(label.clone()),
                })
            }
            _ => None,
        };
        if let Some(ask) = ask {
            self.spread_delete = Some(ask);
            return true;
        }
        match delete_action(line, target) {
            Some(action) => {
                self.perform(action);
                true
            }
            None => false,
        }
    }

    /// Selects structure `id` and brings every view to it: the 3D camera
    /// within 150 m, the plan centred on it.
    /// Shows weather case `k` and goes to section or jumper `id` that
    /// failed in it: selected, the views brought to it (to its structure
    /// when it has no shape in that case).
    pub fn go_to_failure(&mut self, k: usize, id: &str) {
        if k != self.case && k < self.model.cases.len() {
            self.set_case(k);
        }
        let line = &self.model.line;
        let (target, structure) = if let Some(j) = line.jumpers.iter().find(|j| j.id == id) {
            (
                Selection::Jumper(j.id.clone()),
                line.end_structures(&j.from)
                    .into_iter()
                    .next()
                    .unwrap_or_default(),
            )
        } else if let Some(s) = line.sections.iter().find(|s| s.id == id) {
            (
                Selection::Span {
                    section: s.id.clone(),
                    span: 0,
                },
                s.points
                    .first()
                    .map(|p| p.structure.clone())
                    .unwrap_or_default(),
            )
        } else {
            return;
        };
        if self.selection_focus(&target).is_none() {
            self.go_to_structure(&structure);
        }
        self.go_to(target);
    }

    pub(super) fn selection_focus(&self, target: &Selection) -> Option<[f64; 3]> {
        target.focus(&self.model, self.case)
    }

    pub fn go_to_structure(&mut self, id: &str) {
        self.selection = Selection::Structure(id.to_owned());
        self.view3d.dirty.insert(Layer::Selection);
        if let Some(f) = self.selection.focus(&self.model, self.case) {
            if let Some(cam) = self.view3d.camera.as_mut() {
                cam.target = f;
                cam.distance = cam.distance.min(150.0);
            }
            self.plan.centre_on(f);
        }
    }
}

/// A deletion touching spread objects, asked first
/// ([`spread_delete_window`]): spread `spread` of section `section` (with
/// its points or not), or its point `point` with the spreads ending on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpreadDelete {
    /// Section id.
    pub section: String,
    /// The spread deleted (its index).
    pub spread: Option<usize>,
    /// The point deleted (the end of spreads).
    pub point: Option<String>,
}

/// The span spread `index` of section `section` lies on (that of its
/// first point).
pub(crate) fn spread_span(line: &ohl_line::Line, section: &str, index: usize) -> Option<usize> {
    let s = line.sections.iter().find(|s| s.id == section)?;
    let l = s.spreads.get(index)?;
    s.span_points
        .iter()
        .find(|p| p.label == l.from)
        .map(|p| p.span)
}

/// Whether `target` can be deleted ([`Ready::delete_object`]).
pub(super) fn deletable(line: &ohl_line::Line, target: &Selection) -> bool {
    matches!(target, Selection::Spread { .. }) || delete_action(line, target).is_some()
}

/// What keeps span point `label` of section `sec` when spread `but`
/// goes: its object, the other spreads on it, the spacers clamped on it,
/// the jumpers on it (a tap). Empty: a bare point nothing else uses.
pub(super) fn point_uses(
    line: &ohl_line::Line,
    sec: &ohl_line::Section,
    label: &str,
    but: usize,
) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(o) = sec
        .span_points
        .iter()
        .find(|p| p.label == label)
        .and_then(|p| p.object.as_ref())
    {
        out.push(format!("holds a {}", o.kind.as_str()));
    }
    for k in sec.spreads_at(label).into_iter().filter(|k| *k != but) {
        out.push(format!(
            "ends spread {}",
            crate::properties::spread_name(sec, k)
        ));
    }
    for x in line.spacers.iter().filter(|x| {
        [&x.a, &x.b]
            .iter()
            .any(|e| e.section == sec.id && e.point == label)
    }) {
        out.push(format!("spacer {}", x.id));
    }
    for j in line.jumpers.iter().filter(|j| {
        j.taps()
            .any(|t| matches!(t, ohl_line::Tap::Span(r) if r.section == sec.id && r.point == label))
    }) {
        out.push(format!("jumper {} (a tap)", j.id));
    }
    out
}

/// The deletion of span points `points` and spreads `spreads` (indices)
/// of section `section`, with the spacers clamped on those points: one
/// command.
pub(super) fn span_delete_action(
    line: &ohl_line::Line,
    section: &str,
    points: &[String],
    spreads: &[usize],
) -> Option<crate::edit::Action> {
    use ohl_session::Kind;
    let mut s = line.sections.iter().find(|s| s.id == section)?.clone();
    let names: Vec<String> = spreads
        .iter()
        .map(|k| crate::properties::spread_name(&s, *k))
        .collect();
    s.span_points.retain(|p| !points.contains(&p.label));
    let mut k = 0;
    s.spreads.retain(|_| {
        k += 1;
        !spreads.contains(&(k - 1))
    });
    let spacers: Vec<&ohl_line::Spacer> = line
        .spacers
        .iter()
        .filter(|x| {
            [&x.a, &x.b]
                .iter()
                .any(|e| e.section == section && points.contains(&e.point))
        })
        .collect();
    let mut commands: Vec<ohl_session::Command> = spacers
        .iter()
        .map(|x| ohl_session::Command::Delete {
            kind: Kind::Spacer,
            id: x.id.clone(),
            confirm: true,
        })
        .collect();
    commands.push(ohl_session::Command::Put {
        kind: Kind::Section,
        id: section.to_owned(),
        value: serde_json::to_value(&s).ok()?,
        ord: None,
    });
    let mut what = Vec::new();
    if !points.is_empty() {
        what.push(format!("span point(s) {}", points.join(", ")));
    }
    if !names.is_empty() {
        what.push(format!("spread(s) {}", names.join(", ")));
    }
    if !spacers.is_empty() {
        what.push(format!(
            "spacers {}",
            spacers
                .iter()
                .map(|x| x.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Some(crate::edit::Action::Command {
        what: format!("deleted {} of {section}", what.join(" and ")),
        command: ohl_session::Command::Batch { commands },
    })
}

/// The deletion of a right-clicked object: a structure, a string, a span's
/// section, a jumper.
pub(super) fn delete_action(
    line: &ohl_line::Line,
    target: &Selection,
) -> Option<crate::edit::Action> {
    use crate::edit::Action;
    use ohl_session::Kind;
    let del = |kind, id: &String| Action::Delete {
        kind,
        id: id.clone(),
        confirm: false,
    };
    Some(match target {
        Selection::Structure(id) => del(Kind::Structure, id),
        Selection::Span { section, .. } => del(Kind::Section, section),
        Selection::Jumper(id) => del(Kind::Jumper, id),
        Selection::Spacer(id) => del(Kind::Spacer, id),
        // The jumper without it, and the jumpers hanging on it (a tap).
        Selection::JumperPoint { jumper, label } => {
            let mut j = line.jumpers.iter().find(|j| &j.id == jumper)?.clone();
            j.points.retain(|p| &p.label != label);
            let hanging: Vec<&ohl_line::Jumper> = line
                .jumpers
                .iter()
                .filter(|x| {
                    x.taps().any(|t| {
                        matches!(t, ohl_line::Tap::Jumper { jumper: h, point } if h == jumper && point == label)
                    })
                })
                .collect();
            let mut commands: Vec<ohl_session::Command> = hanging
                .iter()
                .map(|x| ohl_session::Command::Delete {
                    kind: Kind::Jumper,
                    id: x.id.clone(),
                    confirm: true,
                })
                .collect();
            commands.push(ohl_session::Command::Put {
                kind: Kind::Jumper,
                id: j.id.clone(),
                value: serde_json::to_value(&j).ok()?,
                ord: None,
            });
            Action::Command {
                what: if hanging.is_empty() {
                    format!("deleted point {label} of jumper {jumper}")
                } else {
                    format!(
                        "deleted point {label} of jumper {jumper} and the jumpers on it: {}",
                        hanging
                            .iter()
                            .map(|x| x.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                },
                command: ohl_session::Command::Batch { commands },
            }
        }
        // The section without it, and the spacers clamped on it (a
        // spread's end: asked first, [`Ready::delete_object`]).
        Selection::SpanPoint { section, label } => {
            span_delete_action(line, section, std::slice::from_ref(label), &[])?
        }
        Selection::Attachment { structure, set } => Action::DeleteAttachment {
            structure: structure.clone(),
            set: *set,
            confirm: false,
        },
        // A string: the strings of its set, with what goes with them.
        Selection::String { structure, label } => {
            let s = line.structure(structure)?;
            let c = s.component(&s.string(label)?.wire);
            let set = c.wires.iter().find_map(|&i| s.wires[i].set)?;
            Action::DeleteAttachment {
                structure: structure.clone(),
                set,
                confirm: false,
            }
        }
        // Off every alignment it is on (asked first when on several).
        Selection::AlignmentPoint { id } => Action::Delete {
            kind: ohl_session::Kind::AlignmentPoint,
            id: id.clone(),
            confirm: false,
        },
        _ => return None,
    })
}
