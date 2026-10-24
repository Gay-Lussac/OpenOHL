// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Structures edited: structure types in the pylon workspace, files in
//! OHL-Structure, handles, the placed structure's editor, built types.

use super::*;

impl Ready {
    /// Performs an edit and updates the views; whether it was made.
    /// Opens structure type `id` in the pylon workspace of the Structure
    /// types page.
    pub fn edit_type(&mut self, id: &str) {
        let kind = crate::library::LibraryKind::StructureTypes;
        let Some(path) = self
            .model
            .types
            .as_ref()
            .and_then(|(t, _)| t.path(id))
            .map(std::path::Path::to_path_buf)
        else {
            self.status = Some((format!("structure type {id}: no file read"), true));
            return;
        };
        let sources = self.model.sources.clone();
        let page = self
            .libraries
            .entry(kind)
            .or_insert_with(|| crate::library::LibraryPage::new(kind, &sources));
        if page.file.as_deref() != Some(path.as_path()) {
            if page.dirty() {
                self.status = Some((
                    "the Structure library page has unsaved changes: save or discard them first"
                        .into(),
                    true,
                ));
                self.open_page = Some((Tab::Library(kind), None));
                return;
            }
            page.add_file(&path);
        }
        page.workspace.on = true;
        self.open_page = Some((Tab::Library(kind), None));
    }

    /// **Edit**: structure type `id` in the structure application, on its
    /// file (in a project, the working copy); saved there, it comes back
    /// into the line ([`Ready::structure_apps_frame`]). From placed
    /// structure `placed`: with that place's line values and typed loads.
    pub fn edit_in_structure_app(&mut self, id: &str, placed: Option<&str>) {
        match crate::structure_app::prepare_edit(&self.model, id, placed) {
            Ok((path, placed)) => self.launch_structure_app(&path, id, placed),
            Err(e) => {
                log::info!("structure application: {e}");
                self.structure_apps.reports.push(crate::structure_app::Report {
                    title: "OHL-Structure was not started".into(),
                    text: format!("{e}: the structure type's file was not read (Structures › Structure library)."),
                    error: true,
                });
                self.status = Some((e, true));
            }
        }
    }

    /// The structure application on the structure file `path` (structure
    /// `id`).
    pub fn edit_file_in_structure_app(&mut self, path: &std::path::Path, id: &str) {
        self.launch_structure_app(path, id, None);
    }

    pub(super) fn launch_structure_app(
        &mut self,
        path: &std::path::Path,
        id: &str,
        placed: Option<crate::structure_app::Placed>,
    ) {
        let insulators = crate::model::insulators_dir(&self.model.sources);
        let sections = self.model.sections_folder();
        let ahead = self.model.line.display.ahead_deg;
        let whose = placed.as_ref().map(|p| p.structure.clone());
        self.status = Some(
            match self.structure_apps.launch(
                path,
                id,
                Some(&insulators),
                ahead,
                Some(&sections),
                placed,
            ) {
                Ok(()) => (
                    match whose {
                        Some(s) => format!(
                            "structure {id} opened in OHL-Structure with the loads \
                             of {s}: saved there, the line follows"
                        ),
                        None => format!(
                            "structure {id} opened in OHL-Structure: saved there, \
                             the line follows"
                        ),
                    },
                    false,
                ),
                Err(e) => (e, true),
            },
        );
    }

    /// Each frame: the structure files open in the structure application
    /// looked at (twice a second); one saved there goes into the project
    /// and the libraries are read again, as a save of the Structure types
    /// page; typed loads saved there go into the project; the placed
    /// structures' line values written again when the line changed.
    pub fn structure_apps_frame(&mut self, ctx: &egui::Context) {
        use crate::structure_app::Change;
        if !self.structure_apps.active() {
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
        for c in self.structure_apps.poll() {
            match c {
                Change::Structure(path, id) => self.structure_file_saved(&path, &id),
                Change::Loads(_, whose) => {
                    self.status = Some(match self.model.store_libraries() {
                        Ok(n) if n > 0 => {
                            (format!("typed loads of {whose} kept in the project"), false)
                        }
                        Ok(_) => (
                            format!("typed loads of {whose} saved beside the structure file"),
                            false,
                        ),
                        Err(e) => (format!("keeping the typed loads of {whose}: {e}"), true),
                    });
                }
                // Libraries imported in the structure application (its
                // File › Libraries, into the working copy's folders): into
                // the project, and read again, as a save of a library page.
                Change::Libraries(dir, id) => {
                    log::info!(
                        "structure application on {id}: library files changed in {}",
                        dir.display()
                    );
                    self.library_saved(
                        crate::library::LibraryKind::StructureTypes,
                        crate::library::PageAction::Saved(dir),
                    );
                }
            }
        }
        if let Some(e) = self.structure_apps.refresh_values(&self.model).pop() {
            self.status = Some((e, true));
        }
    }

    /// Structure file `path` (structure `id`) saved outside: stored into
    /// the project, the libraries read again; the Structure types page
    /// shows the new text when it has no changes of its own.
    pub fn structure_file_saved(&mut self, path: &std::path::Path, id: &str) {
        use crate::library::{LibraryKind, PageAction};
        let kind = LibraryKind::StructureTypes;
        self.library_saved(kind, PageAction::Saved(path.to_path_buf()));
        if let Some(page) = self.libraries.get_mut(&kind)
            && page.file.as_deref() == Some(path)
        {
            if page.dirty() {
                self.status = Some((
                    format!(
                        "structure {id} was saved in OHL-Structure; the Structure \
                         types page has changes of its own (Discard to see the saved file)"
                    ),
                    true,
                ));
            } else {
                page.open(path);
            }
        }
    }

    /// **Edit…** on structure `id` (from Properties, the Structures
    /// table): its type in the structure application with the loads of
    /// this place; one without a type has no file to open there: its
    /// structure screen, asking to make it a structure file first.
    pub fn edit_structure_in_app(&mut self, id: &str) {
        let typed = self
            .model
            .line
            .structure(id)
            .and_then(|s| s.structure_type.clone());
        match typed {
            Some(t) => self.edit_in_structure_app(&t, Some(id)),
            None => self.make_file_and_edit(id),
        }
    }

    /// Structure `id` without a type: its structure screen, with the
    /// type id asked for — saved, used and opened in the structure
    /// application.
    pub fn make_file_and_edit(&mut self, id: &str) {
        log::info!("structure application: {id} has no structure file, make it one first");
        self.edit_structure(id);
        if let Some(p) = self.placed.as_mut().filter(|p| p.id == id) {
            p.ask_file_and_edit();
            self.status = Some((
                format!(
                    "{id} has no structure file: give it a type id and press Save, use and edit"
                ),
                false,
            ));
        }
    }

    /// The structure whose drag handles the 3D view shows: the selected
    /// one (or that of the string, wire point or point selected) while its
    /// structure screen is open (`open`: whether a page is) — a structure
    /// without a type in the Structure editor, a typed one with its type on
    /// the Structure types page; none when the project cannot be edited.
    /// A drag waits while that editor, or an editor window on it, has
    /// changes the drag would not keep.
    pub fn handle_target(&self, open: impl Fn(&Tab) -> bool) -> Option<crate::handles::Target> {
        use crate::library::LibraryKind;
        self.model.can_edit().ok()?;
        let id = match &self.selection {
            Selection::Structure(id) => id,
            Selection::Attachment { structure, .. }
            | Selection::String { structure, .. }
            | Selection::Point { structure, .. } => structure,
            _ => return None,
        };
        let s = self.model.line.structure(id)?;
        let mut blocked = None;
        let grid = match &s.structure_type {
            None => {
                let p = self.placed.as_ref().filter(|p| &p.id == id)?;
                if !open(&Tab::StructureEditor) {
                    return None;
                }
                if p.dirty() {
                    blocked = Some(format!(
                        "the structure editor of {id} has changes not applied: apply or revert                          them before dragging its points"
                    ));
                }
                p.workspace.grid
            }
            Some(t) => {
                let kind = LibraryKind::StructureTypes;
                let page = self.libraries.get(&kind)?;
                let path = self.model.types.as_ref()?.0.path(t)?;
                if !(page.workspace.on
                    && page.file.as_deref() == Some(path)
                    && open(&Tab::Library(kind)))
                {
                    return None;
                }
                page.workspace.grid
            }
        };
        let window = match &self.editor {
            Some(crate::editors::Form::String(f)) => f.structure == *id,
            Some(crate::editors::Form::Structure(f)) => !f.new && f.id == *id,
            _ => false,
        };
        if window && blocked.is_none() {
            blocked = Some(format!(
                "a window editing {id} is open: apply or cancel it before dragging its points"
            ));
        }
        Some(crate::handles::Target {
            structure: id.clone(),
            grid,
            blocked,
        })
    }

    /// A drag of a handle in the 3D view ended: the point moved (one
    /// command, as typed in its table), or why not.
    pub fn handle_done(&mut self, done: crate::handles::Done) {
        use crate::handles::Done;
        match done {
            Done::Moved {
                structure,
                handle,
                at,
                onto,
            } => match crate::handles::command_onto(
                &self.model,
                &structure,
                &handle,
                at,
                onto.as_ref(),
            ) {
                Ok((what, command)) => {
                    log::info!("{what}");
                    if self.perform(crate::edit::Action::Command {
                        what: what.clone(),
                        command,
                    }) {
                        self.status = Some((what, false));
                        // The structure editor shows the structure as it is
                        // now (it had no changes of its own: the drag waits
                        // otherwise).
                        if let Some(p) = self.placed.as_mut().filter(|p| p.id == structure)
                            && let Some(s) = self.model.line.structure(&structure)
                        {
                            p.doc = crate::placed_editor::as_type(s);
                            p.applied();
                            p.workspace.redraw();
                        }
                    }
                }
                Err(e) => self.status = Some((e, true)),
            },
            Done::Refused(why) => self.status = Some((why, true)),
            Done::Cancelled => {
                self.status = Some(("drag cancelled: nothing changed".into(), false));
            }
        }
    }

    /// Opens structure `id` (one without a type) in the pylon editor.
    pub fn edit_structure(&mut self, id: &str) {
        if self
            .placed
            .as_ref()
            .is_some_and(|p| p.id != id && p.dirty())
        {
            self.status = Some((
                "the structure editor has changes not applied: apply or revert them first".into(),
                true,
            ));
        } else if self.placed.as_ref().is_none_or(|p| p.id != id) {
            match crate::placed_editor::PlacedEditor::open(&self.model.line, id) {
                Ok(p) => self.placed = Some(p),
                Err(e) => {
                    self.status = Some((e, true));
                    return;
                }
            }
        }
        self.open_page = Some((Tab::StructureEditor, None));
    }

    /// The structure editor page.
    pub(super) fn placed_ui(&mut self, ui: &mut egui::Ui, rs: Option<&egui_wgpu::RenderState>) {
        use crate::placed_editor::PlacedAction;
        let Some(mut p) = self.placed.take() else {
            ui.label(
                "No structure open. Select a structure without a type and press \
                 Strings and points… in Properties.",
            );
            return;
        };
        let empty = ohl_structures::InsulatorLibrary::default();
        let insulators = self.model.types.as_ref().map_or(&empty, |(_, i)| i);
        let line = self.model.line.clone();
        let action = p.ui(ui, rs, insulators, &line);
        let request = p.workspace.request.take();
        let stored = self.model.stored_line().unwrap_or(&self.model.line).clone();
        match action {
            Some(PlacedAction::Apply) => match p.command(&stored, insulators) {
                Ok((what, command)) => {
                    if self.perform(crate::edit::Action::Command { what, command }) {
                        p.applied();
                        p.message = Some(("applied".into(), false));
                    }
                }
                Err(e) => p.message = Some((e, true)),
            },
            Some(PlacedAction::MakeType { id: type_id, edit }) => {
                match self.make_type(&p, &stored, &type_id) {
                    Ok(()) => {
                        log::info!("structure {} made structure type {type_id}", p.id);
                        self.status = Some((format!("{} has type {type_id} now", p.id), false));
                        self.placed = None;
                        self.edit_type(&type_id);
                        if edit {
                            self.edit_in_structure_app(&type_id, Some(&p.id));
                        }
                        return;
                    }
                    Err(e) => p.message = Some((e, true)),
                }
            }
            None => {}
        }
        if let Some(req) = request {
            self.workspace_request(req);
        }
        self.placed = Some(p);
    }

    /// What the pylon editor asks: an insulator saved into the insulator
    /// library (refused when its id is taken), the insulator library
    /// opened.
    pub fn workspace_request(&mut self, req: crate::type_workspace::Request) {
        use crate::library::{LibraryKind, PageAction};
        use crate::type_workspace::Request;
        match req {
            // Taken by the page (the file as saved) or the structure
            // without a type (a file first).
            Request::EditInStructureApp => {}
            Request::OpenInsulators => {
                self.open_page = Some((Tab::Library(LibraryKind::Insulators), None));
            }
            Request::SaveInsulator { id, text } => {
                let taken = self
                    .model
                    .types
                    .as_ref()
                    .is_some_and(|(_, i)| i.get(&id).is_some());
                let dir = LibraryKind::Insulators.new_file_dir(&self.model.sources);
                let path = dir.join(format!(
                    "{id}.{}",
                    ohl_structures::insulators::INSULATOR_FILE_EXTENSION
                ));
                let result = if taken || path.exists() {
                    Err(format!(
                        "there is an insulator assembly {id} already (rename it in the library)"
                    ))
                } else {
                    std::fs::create_dir_all(&dir)
                        .and_then(|()| std::fs::write(&path, &text))
                        .map_err(|e| format!("{}: {e}", path.display()))
                };
                match result {
                    Ok(()) => {
                        self.library_saved(
                            LibraryKind::Insulators,
                            PageAction::Saved(path.clone()),
                        );
                        self.status = Some((
                            format!("insulator assembly {id} saved to the library"),
                            false,
                        ));
                    }
                    Err(e) => self.status = Some((e, true)),
                }
            }
        }
    }

    /// The folder of the project's own structure types: its folder, else a
    /// Structures folder next to the line (never the shipped library).
    pub(super) fn project_types_dir(&self) -> std::path::PathBuf {
        let sources = &self.model.sources;
        match &sources.structures {
            Some(_) => crate::library::LibraryKind::StructureTypes.new_file_dir(sources),
            None => crate::model::model_dir(sources).join("Structures"),
        }
    }

    /// Writes structure type `id` built from a pylon template (`replace`:
    /// the file of the type it replaces); the library is read again.
    pub fn save_built_type(
        &mut self,
        id: &str,
        text: &str,
        replace: Option<std::path::PathBuf>,
    ) -> Result<std::path::PathBuf, String> {
        let path = match replace {
            Some(p) => p,
            None => {
                let dir = self.project_types_dir();
                std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                let p = dir.join(format!("{id}.ohls"));
                if ohl_core::files::exists_by_either_name(&p) {
                    return Err(format!("{} exists already", p.display()));
                }
                p
            }
        };
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        self.library_saved(
            crate::library::LibraryKind::StructureTypes,
            crate::library::PageAction::Saved(path.clone()),
        );
        Ok(path)
    }

    /// Saves the editor's document as structure type `type_id` of the
    /// project and gives its structure that type (its own points, strings
    /// and sticks go: they come from the type).
    pub(super) fn make_type(
        &mut self,
        p: &crate::placed_editor::PlacedEditor,
        stored: &ohl_line::Line,
        type_id: &str,
    ) -> Result<(), String> {
        let kind = crate::library::LibraryKind::StructureTypes;
        if self
            .model
            .types
            .as_ref()
            .is_some_and(|(t, _)| t.path(type_id).is_some())
        {
            return Err(format!("there is a structure type {type_id} already"));
        }
        let dir = self.project_types_dir();
        let path = dir.join(format!("{type_id}.ohls"));
        if ohl_core::files::exists_by_either_name(&path) {
            return Err(format!("{} exists already", path.display()));
        }
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        std::fs::write(&path, p.type_text(type_id))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        self.library_saved(kind, crate::library::PageAction::Saved(path.clone()));
        let mut s = stored
            .structure(&p.id)
            .ok_or_else(|| format!("no structure {}", p.id))?
            .clone();
        s.structure_type = Some(type_id.to_owned());
        s.points.clear();
        s.strings.clear();
        s.wires.clear();
        s.members.clear();
        s.omit.clear();
        let command = ohl_session::Command::Put {
            kind: ohl_session::Kind::Structure,
            id: p.id.clone(),
            value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
            ord: None,
        };
        if self.perform(crate::edit::Action::Command {
            what: format!("{} takes structure type {type_id}", p.id),
            command,
        }) {
            Ok(())
        } else {
            Err(self
                .status
                .clone()
                .map_or_else(|| "not changed".into(), |(m, _)| m))
        }
    }
}
