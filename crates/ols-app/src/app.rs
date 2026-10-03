// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The application: loads a model in the background, then shows it in
//! dockable views that share the weather case and the selection.

use std::collections::HashSet;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use eframe::egui_wgpu;
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};

use crate::model::{self, Model, Sources};
use crate::plan::PlanView;
use crate::profile::ProfileView;
use crate::scene::Layer;
use crate::selection::Selection;
use crate::view3d::View3d;

/// The pages (tabs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tab {
    /// 3D view.
    View3d,
    /// Plan view.
    Plan,
    /// Profile view.
    Profile,
    /// Properties of the selection.
    Properties,
    /// Layers, weather cases, load log.
    Model,
    /// Clearance violations.
    Clearances,
    /// A library editor.
    Library(crate::library::LibraryKind),
    /// A table of the line.
    Table(crate::tables::TableKind),
}

impl Tab {
    /// The pages of the View menu (library pages: the Libraries menu).
    pub const VIEWS: [Tab; 6] = [
        Tab::View3d,
        Tab::Plan,
        Tab::Profile,
        Tab::Properties,
        Tab::Model,
        Tab::Clearances,
    ];

    /// Title on its tab.
    pub fn title(self) -> &'static str {
        match self {
            Tab::View3d => "3D",
            Tab::Plan => "Plan",
            Tab::Profile => "Profile",
            Tab::Properties => "Properties",
            Tab::Model => "Model",
            Tab::Clearances => "Clearances",
            Tab::Library(k) => k.title(),
            Tab::Table(k) => k.title(),
        }
    }
}

enum Message {
    Progress(String),
    Done(Result<Box<Model>, String>),
}

/// The "Move structure" window: the structure, where it was, where it is
/// being moved (previewed live), whether its base stays on the ground.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveDialog {
    /// Structure.
    pub id: String,
    /// Its position when the window opened.
    pub start: [f64; 3],
    /// The position typed.
    pub position: [f64; 3],
    /// The base follows the ground (z from the ground at E, N).
    pub on_ground: bool,
    /// The position shown (previewed) now.
    pub shown: [f64; 3],
}

/// What is shown once the model is loaded.
pub struct Ready {
    /// The model.
    pub model: Model,
    /// Weather case shown.
    pub case: usize,
    /// Selected object.
    pub selection: Selection,
    /// What can be clicked.
    pub snaps: crate::selection::Snaps,
    /// Visible 3D layers.
    pub visible: HashSet<Layer>,
    /// 3D view.
    pub view3d: View3d,
    /// Plan view.
    pub plan: PlanView,
    /// Profile view.
    pub profile: ProfileView,
    /// The "Go to structure" window, when open: the structure chosen.
    pub goto: Option<String>,
    /// What the last edit did (or why it was not done: `true`).
    pub status: Option<(String, bool)>,
    /// A deletion waiting for its confirmation: what it takes with it, and
    /// the deletion confirmed.
    pub confirm: Option<(Box<ols_session::Impact>, crate::edit::Action)>,
    /// The editor window open (structure, string, section).
    pub editor: Option<crate::editors::Form>,
    /// String wire: the strings right-clicked so far.
    pub chain: Vec<ols_line::SectionPoint>,
    /// String wire: the section whose points are picked again.
    pub restring: Option<String>,
    /// Tools: the hotbar (the active tool says what a click does).
    pub hotbar: crate::tools::Hotbar,
    /// The inventory window is open.
    pub inventory: bool,
    /// The "Move structure" window, when open.
    pub move_dialog: Option<MoveDialog>,
    /// Turns wheel events into hotbar steps.
    wheel: crate::tools::WheelSteps,
    /// Where tool windows open (the crosshair, or the mouse).
    pub popup_at: egui::Pos2,
    /// Counts the windows opened: a new one opens at `popup_at` again.
    popup_serial: u64,
    /// A tool window is open.
    popup_open: bool,

    /// A position being typed in the properties: (structure, position).
    pub edit_position: Option<(String, [f64; 3])>,
    /// Clearance checks.
    pub clearances: crate::clearance::Clearances,
    /// Library editor pages opened.
    pub libraries:
        std::collections::BTreeMap<crate::library::LibraryKind, crate::library::LibraryPage>,
    /// Table pages opened.
    pub tables: std::collections::BTreeMap<crate::tables::TableKind, crate::tables::TablePage>,
}

impl Ready {
    /// A model ready to show.
    pub fn new(model: Model) -> Self {
        let mut view3d = View3d::default();
        view3d.rebuild_all();
        let mut visible: HashSet<Layer> = Layer::ALL.into_iter().collect();
        visible.remove(&Layer::OtherCases);
        Self {
            case: 0,
            selection: Selection::None,
            snaps: crate::selection::Snaps::default(),
            visible,
            view3d,
            plan: PlanView::default(),
            profile: ProfileView::default(),
            goto: None,
            status: None,
            confirm: None,
            editor: None,
            chain: Vec::new(),
            restring: None,
            hotbar: crate::tools::Hotbar::default(),
            inventory: false,
            move_dialog: None,
            wheel: Default::default(),
            popup_at: egui::pos2(400.0, 200.0),
            popup_serial: 0,
            popup_open: false,

            edit_position: None,
            libraries: Default::default(),
            tables: Default::default(),
            clearances: crate::clearance::Clearances::new(
                &model.sources.file,
                model.sources.rules.as_deref(),
            ),
            model,
        }
    }
}

enum State {
    Loading {
        rx: Receiver<Message>,
        progress: Vec<String>,
    },
    Ready(Box<Ready>),
    Failed(String),
}

/// The OLS CAD application.
pub struct App {
    sources: Sources,
    /// Navigation scheme to start with.
    pub controls: crate::controls::Controls,
    /// What can be clicked, to start with.
    pub snaps: crate::selection::Snaps,
    /// Hotbar to start with.
    pub hotbar: crate::tools::Hotbar,
    /// The quick bar under the menus.
    pub quickbar: crate::menu::QuickBar,
    /// The "Customize the quick bar" window is open.
    customize: bool,
    state: State,
    dock: DockState<Tab>,
    render_state: Option<egui_wgpu::RenderState>,
}

/// The default arrangement: 3D and plan in the middle, properties right,
/// model and layers left, profile below.
pub fn default_dock() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::View3d, Tab::Plan]);
    let tree = dock.main_surface_mut();
    let [centre, _] = tree.split_right(NodeIndex::root(), 0.74, vec![Tab::Properties]);
    let [centre, _] = tree.split_left(centre, 0.2, vec![Tab::Model, Tab::Clearances]);
    tree.split_below(centre, 0.66, vec![Tab::Profile]);
    dock
}

impl App {
    /// Starts loading `sources`.
    pub fn new(
        sources: Sources,
        render_state: Option<egui_wgpu::RenderState>,
        ctx: &egui::Context,
    ) -> Self {
        let (tx, rx) = channel();
        let s = sources.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let report = |m: String| {
                let _ = tx.send(Message::Progress(m));
                ctx.request_repaint();
            };
            let result = model::load(s, &report).map(Box::new);
            let _ = tx.send(Message::Done(result));
            ctx.request_repaint();
        });
        Self {
            sources,
            state: State::Loading {
                rx,
                progress: Vec::new(),
            },
            dock: default_dock(),
            render_state,
            controls: crate::controls::Controls::default(),
            snaps: crate::selection::Snaps::default(),
            hotbar: crate::tools::Hotbar::default(),
            quickbar: Default::default(),
            customize: false,
        }
    }

    /// An app showing an already loaded model (tests).
    #[cfg(test)]
    pub fn with_model(model: Model, render_state: Option<egui_wgpu::RenderState>) -> Self {
        Self {
            sources: model.sources.clone(),
            state: State::Ready(Box::new(Ready::new(model))),
            dock: default_dock(),
            render_state,
            controls: crate::controls::Controls::default(),
            snaps: crate::selection::Snaps::default(),
            hotbar: crate::tools::Hotbar::default(),
            quickbar: Default::default(),
            customize: false,
        }
    }

    /// The shown model, once loaded.
    #[cfg(test)]
    pub fn ready(&mut self) -> Option<&mut Ready> {
        match &mut self.state {
            State::Ready(r) => Some(r),
            _ => None,
        }
    }
}

struct Viewer<'a> {
    ready: &'a mut Ready,
    rs: Option<&'a egui_wgpu::RenderState>,
}

impl TabViewer for Viewer<'_> {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        let dirty = match tab {
            Tab::Library(k) => self.ready.libraries.get(k).is_some_and(|p| p.dirty()),
            Tab::Table(k) => self.ready.tables.get(k).is_some_and(|p| p.pending() > 0),
            _ => false,
        };
        if dirty {
            format!("{} *", tab.title()).into()
        } else {
            tab.title().into()
        }
    }

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(*tab)
    }

    fn closeable(&mut self, _tab: &mut Tab) -> bool {
        true
    }

    fn on_close(&mut self, tab: &mut Tab) -> egui_dock::tab_viewer::OnCloseResponse {
        // A library page with unsaved changes stays open.
        if let Tab::Library(k) = tab
            && let Some(p) = self.ready.libraries.get_mut(k)
            && p.dirty()
        {
            p.message = Some(("Save or discard the changes before closing".into(), true));
            return egui_dock::tab_viewer::OnCloseResponse::Ignore;
        }
        if let Tab::Table(k) = tab
            && let Some(p) = self.ready.tables.get_mut(k)
            && p.pending() > 0
        {
            p.message = Some(("Apply or revert the changes before closing".into(), true));
            return egui_dock::tab_viewer::OnCloseResponse::Ignore;
        }
        egui_dock::tab_viewer::OnCloseResponse::Close
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        let r = &mut *self.ready;
        let before = r.selection.clone();
        let snaps = r.effective_snaps();
        let used = match tab {
            Tab::View3d => r.view3d.ui(
                ui,
                self.rs,
                &r.model,
                r.case,
                &mut r.selection,
                &r.visible,
                &snaps,
            ),
            Tab::Plan => r.plan.ui(ui, &r.model, r.case, &mut r.selection, &snaps),
            Tab::Profile => r
                .profile
                .ui(ui, &r.model, r.case, &mut r.selection, &snaps)
                .map(|target| crate::selection::Use { target, at: None }),
            Tab::Properties => {
                let editable = r.model.can_edit().is_ok();
                let action = crate::properties::ui(
                    ui,
                    &r.model,
                    r.case,
                    &mut r.selection,
                    editable.then_some(&mut r.edit_position),
                );
                if let Some(a) = action {
                    r.perform(a);
                }
                None
            }
            Tab::Model => {
                model_panel(ui, r);
                None
            }
            Tab::Clearances => {
                clearance_panel(ui, r);
                None
            }
            Tab::Table(kind) => {
                let kind = *kind;
                let page = r.tables.entry(kind).or_default();
                match page.ui(ui, kind, &r.model, r.case, &r.selection) {
                    Some(crate::tables::TableAction::Select(s)) => {
                        r.go_to(s);
                    }
                    Some(crate::tables::TableAction::Apply(commands)) => {
                        r.apply_table(kind, commands);
                    }
                    Some(crate::tables::TableAction::ShowCase(k))
                        if k < r.model.cases.len() && k != r.case =>
                    {
                        r.set_case(k);
                    }
                    Some(crate::tables::TableAction::ShowCase(_)) | None => {}
                }
                None
            }
            Tab::Library(kind) => {
                let kind = *kind;
                let sources = r.model.sources.clone();
                let page = r
                    .libraries
                    .entry(kind)
                    .or_insert_with(|| crate::library::LibraryPage::new(kind, &sources));
                if let Some(action) = page.ui(ui, &sources) {
                    r.library_saved(kind, action);
                }
                None
            }
        };
        if r.selection != before {
            r.view3d.dirty.insert(Layer::Selection);
        }
        // A right-click uses the held tool on what is there.
        if let Some(target) = used {
            r.use_tool(target);
        }
    }
}

impl Ready {
    /// What can be right-clicked now: the Snap menu's choice, and strings
    /// while a tool that works on them is held.
    pub fn effective_snaps(&self) -> crate::selection::Snaps {
        let mut s = self.snaps.clone();
        if self.hotbar.tool().picks_strings() {
            s.0.insert(crate::selection::Snap::Attachment);
        }
        if self.hotbar.tool().picks_alignment() {
            s.0.insert(crate::selection::Snap::AlignmentPoint);
        }
        s
    }

    /// Uses the held tool on what was right-clicked in a view: Select
    /// selects, Go to opens its window, the editing tools act on the object
    /// (or, Add structure, on the ground point).
    pub fn use_tool(&mut self, u: crate::selection::Use) {
        use crate::tools::Tool;
        let tool = self.hotbar.tool();
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
        if let Err(why) = self.model.can_edit() {
            self.status = Some((format!("{}: read-only — {why}", tool.name()), true));
            return;
        }
        use crate::editors::{Form, SectionForm, StringForm, StructureForm};
        let model = &self.model;
        match (tool, &target) {
            (Tool::Move, Selection::Structure(id)) => {
                let id = id.clone();
                self.select(target);
                self.use_tool_on(&id);
            }
            (Tool::Delete, _) => {
                if let Some(action) = delete_action(&self.model.line, &target) {
                    self.select(target);
                    self.perform(action);
                } else {
                    self.hint(tool);
                }
            }
            (Tool::Edit, Selection::Structure(id)) => {
                self.editor = StructureForm::of(model, id).map(Form::Structure);
                self.select(target);
            }
            (
                Tool::Edit,
                Selection::Attachment {
                    structure,
                    set,
                    phase,
                },
            ) => {
                self.editor = StringForm::of(model, structure, *set, *phase).map(Form::String);
                self.select(target);
            }
            (Tool::Edit, Selection::AlignmentPoint { route, index }) => {
                self.editor =
                    crate::editors::AlignmentForm::of(model, route, *index).map(Form::Alignment);
                if self.editor.is_none() {
                    self.status = Some((
                        "Edit: this route follows its structures — give it points first (Add alignment point, or the Alignment points table)".into(),
                        true,
                    ));
                }
                self.select(target);
            }
            (Tool::AddAlignment, _) => match u.at {
                Some(at) => match crate::editors::insert_alignment_point(model, at) {
                    Ok((what, command, route, index)) => {
                        if self.perform(crate::edit::Action::Command { what, command }) {
                            self.select(Selection::AlignmentPoint { route, index });
                        }
                    }
                    Err(e) => self.status = Some((e, true)),
                },
                None => {
                    self.status = Some((
                        "Add alignment point: right-click the ground (3D or plan view)".into(),
                        true,
                    ));
                }
            },
            (Tool::Edit, Selection::Span { section, .. }) => {
                self.editor = SectionForm::of(model, section).map(Form::Section);
                self.select(target);
            }
            (Tool::AddStructure, _) => match u.at {
                Some(at) => {
                    self.editor = Some(Form::Structure(StructureForm::new_at(model, at)));
                }
                None => {
                    self.status = Some((
                        "Add structure: right-click the ground (3D or plan view)".into(),
                        true,
                    ));
                }
            },
            (
                Tool::AddString,
                Selection::Structure(id) | Selection::Attachment { structure: id, .. },
            ) => {
                self.editor = StringForm::new_on(model, id).map(Form::String);
                self.select(Selection::Structure(id.clone()));
            }
            (
                Tool::StringWire,
                Selection::Attachment {
                    structure,
                    set,
                    phase,
                },
            ) => {
                let p = ols_line::SectionPoint {
                    structure: structure.clone(),
                    set: *set,
                    phase: *phase,
                };
                if self.chain.last() == Some(&p) {
                    return;
                }
                if self.chain.contains(&p) {
                    self.status = Some((
                        format!("String wire: {structure} {set}:{phase} is in the section already"),
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
            _ => self.hint(tool),
        }
    }

    /// "Tool: what to right-click" in the status bar.
    fn hint(&mut self, tool: crate::tools::Tool) {
        self.status = Some((format!("{}: {}", tool.name(), tool.help()), true));
    }

    /// After a library page saved a file (or chose the weather cases): the
    /// model reads its libraries again and solves.
    pub fn library_saved(
        &mut self,
        kind: crate::library::LibraryKind,
        action: crate::library::PageAction,
    ) {
        use crate::library::PageAction;
        let (what, previous) = match action {
            PageAction::Saved(p) => (format!("saved {}", p.display()), None),
            PageAction::UseForModel(p) => {
                let before = self.model.sources.clone();
                kind.use_for(&mut self.model.sources, &p);
                (
                    format!("{} from {}", kind.title().to_lowercase(), p.display()),
                    Some(before),
                )
            }
        };
        // A library chosen for the model must have the line's cables.
        let reloaded = self.model.reload_libraries().and_then(|report| {
            let line = &self.model.line;
            let mut missing: Vec<&str> = line
                .sections
                .iter()
                .map(|s| s.cable.as_str())
                .chain(line.jumpers.iter().map(|j| j.cable.as_str()))
                .filter(|c| self.model.library.get(c).is_none())
                .collect();
            missing.sort_unstable();
            missing.dedup();
            match (&previous, missing.is_empty()) {
                (Some(_), false) => Err(format!(
                    "the line's cables {} are not in it",
                    missing.join(", ")
                )),
                _ => Ok(report),
            }
        });
        match reloaded {
            Ok(report) => {
                log::info!("{what}: {report}");
                if previous.is_some() && self.case >= self.model.cases.len() {
                    self.case = 0;
                }
                // The other pages list the files the model now reads.
                let sources = self.model.sources.clone();
                for (k, page) in self.libraries.iter_mut() {
                    if *k != kind {
                        page.files = k.files(&sources);
                    }
                }
                if let Some(page) = self.libraries.get_mut(&kind) {
                    page.message = Some((format!("{what}; {report}"), false));
                }
                self.status = Some((format!("{what}; {report}"), false));
            }
            Err(e) => {
                if let Some(before) = previous {
                    self.model.sources = before;
                    // Back to the libraries it had.
                    if let Err(e) = self.model.reload_libraries() {
                        log::warn!("reading the previous libraries again: {e}");
                    }
                }
                let m = format!("{what}, but the model does not load with it: {e}");
                log::warn!("{m}");
                if let Some(page) = self.libraries.get_mut(&kind) {
                    page.message = Some((m.clone(), true));
                }
                self.status = Some((m, true));
            }
        }
        if !self.selection_exists() {
            self.selection = Selection::None;
        }
        self.clearances.stale = true;
        self.refresh_views();
    }

    /// Selects `target` and brings the views to it.
    pub fn go_to(&mut self, target: Selection) {
        self.select(target);
        if let Some(f) = self.selection.focus(&self.model, self.case) {
            if let Some(cam) = self.view3d.camera.as_mut() {
                cam.target = f;
                cam.distance = cam.distance.min(150.0);
            }
            self.plan.centre_on(f);
        }
    }

    /// Makes a table's changes: the sections' display cases, the session
    /// commands as one, the file written; its pending changes go when done.
    pub fn apply_table(&mut self, kind: crate::tables::TableKind, changes: crate::tables::Changes) {
        let crate::tables::Changes {
            mut commands,
            file,
            display,
            ..
        } = changes;
        let mut done = true;
        let n = commands.len();
        if n > 0 {
            let (what, command) = if n == 1 {
                commands.remove(0)
            } else {
                (
                    format!("{} table: {n} changes", kind.title()),
                    ols_session::Command::Batch {
                        commands: commands.into_iter().map(|c| c.1).collect(),
                    },
                )
            };
            done = self.perform(crate::edit::Action::Command { what, command });
        }
        if done && let Some((path, text)) = file {
            done = self.write_table_file(&path, &text);
        }
        if done && !display.is_empty() {
            for (id, cases) in display {
                if cases.is_empty() {
                    self.model.display_cases.remove(&id);
                } else {
                    self.model.display_cases.insert(id, cases);
                }
            }
            if n == 0 {
                self.status = Some(("display weather cases set".into(), false));
            }
            self.refresh_views();
        }
        if let Some(page) = self.tables.get_mut(&kind) {
            if done {
                page.clear();
                page.message = self.status.clone();
            } else {
                page.message = self
                    .status
                    .clone()
                    .map(|(m, _)| (m, true))
                    .or(Some(("not applied".into(), true)));
            }
        }
    }

    /// Writes a table's file (weather cases), then the model reads its
    /// libraries again; an open editor page of that file reads it again.
    /// Whether it worked.
    pub fn write_table_file(&mut self, path: &std::path::Path, text: &str) -> bool {
        use crate::library::LibraryKind;
        if let Err(e) = std::fs::write(path, text) {
            self.status = Some((format!("{}: {e}", path.display()), true));
            return false;
        }
        let case = self.case;
        self.library_saved(
            LibraryKind::WeatherCases,
            crate::library::PageAction::Saved(path.to_path_buf()),
        );
        if case < self.model.cases.len() {
            self.set_case(case);
        }
        if let Some(page) = self.libraries.get_mut(&LibraryKind::WeatherCases)
            && page.file.as_deref() == Some(path)
            && !page.dirty()
        {
            page.open(path);
        }
        !self.status.as_ref().is_some_and(|s| s.1)
    }

    /// Shows weather case `k`.
    pub fn set_case(&mut self, k: usize) {
        self.case = k;
        self.clearances.stale = true;
        self.view3d
            .dirty
            .extend([Layer::Wires, Layer::OtherCases, Layer::Selection]);
        self.profile.invalidate();
    }

    /// Selects `target`.
    pub fn select(&mut self, target: Selection) {
        self.selection = target;
        self.view3d.dirty.insert(Layer::Selection);
    }

    /// The wire being strung, drawn.
    pub fn update_draft(&mut self) {
        self.view3d.draft = self
            .chain
            .iter()
            .filter_map(|p| self.model.line.attachment(p).map(|a| a.wire_point))
            .collect();
        self.view3d.dirty.insert(Layer::Draft);
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

    /// Gives up the wire being strung.
    pub fn cancel_chain(&mut self) {
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
                    kind: ols_session::Kind::Structure,
                    id: id.to_owned(),
                    confirm: false,
                });
            }
            _ => {}
        }
    }

    /// Ends a Move window's preview without moving.
    fn cancel_move_preview(&mut self) {
        if let Some(d) = self.move_dialog.take()
            && d.shown != d.start
        {
            self.perform(crate::edit::Action::CancelPreview { id: d.id });
        }
    }

    /// Performs an edit and updates the views; whether it was made.
    pub fn perform(&mut self, action: crate::edit::Action) -> bool {
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
            Action::Command { what, command } => self.model.apply(&what, command),
            Action::DeleteAttachment {
                structure,
                set,
                phase,
                confirm,
            } => self.model.apply(
                &format!("deleted string {set}:{phase} of {structure}"),
                ols_session::Command::DeleteAttachment {
                    structure,
                    set,
                    phase,
                    confirm,
                },
            ),
            Action::Move { id, position } => self.model.apply(
                &format!("moved {id}"),
                ols_session::Command::MoveStructure {
                    id: id.clone(),
                    position,
                },
            ),
            Action::Delete { kind, id, confirm } => self.model.apply(
                &format!("deleted {} {id}", kind.as_str()),
                ols_session::Command::Delete { kind, id, confirm },
            ),
            Action::Undo => self.model.undo(),
            Action::Redo => self.model.redo(),
        };
        let done = match result {
            Ok(report) => {
                log::info!("{report}");
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
            .retain(|p| self.model.line.attachment(p).is_some());
        self.update_draft();
        self.refresh_views();
        done
    }

    /// After the line changed: every 3D layer and the profile again.
    fn refresh_views(&mut self) {
        self.view3d.rebuild_all();
        self.profile.invalidate();
    }

    /// Whether the selected object is still in the line.
    fn selection_exists(&self) -> bool {
        let line = &self.model.line;
        match &self.selection {
            Selection::None | Selection::LidarPoint(_) => true,
            Selection::Structure(id) => line.structure(id).is_some(),
            Selection::Jumper(id) => line.jumpers.iter().any(|j| &j.id == id),
            Selection::Span { section, span } => line
                .sections
                .iter()
                .any(|s| &s.id == section && *span + 1 < s.points.len()),
            Selection::Attachment {
                structure,
                set,
                phase,
            } => line
                .attachment(&ols_line::SectionPoint {
                    structure: structure.clone(),
                    set: *set,
                    phase: *phase,
                })
                .is_some(),
            Selection::AlignmentPoint { route, index } => {
                crate::selection::alignment_point(&self.model, route, *index).is_some()
            }
        }
    }

    /// Selects structure `id` and brings every view to it: the 3D camera
    /// within 150 m, the plan centred on it.
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

/// The deletion of a right-clicked object: a structure, a string, a span's
/// section, a jumper.
fn delete_action(line: &ols_line::Line, target: &Selection) -> Option<crate::edit::Action> {
    use crate::edit::Action;
    use ols_session::Kind;
    let del = |kind, id: &String| Action::Delete {
        kind,
        id: id.clone(),
        confirm: false,
    };
    Some(match target {
        Selection::Structure(id) => del(Kind::Structure, id),
        Selection::Span { section, .. } => del(Kind::Section, section),
        Selection::Jumper(id) => del(Kind::Jumper, id),
        Selection::Attachment {
            structure,
            set,
            phase,
        } => Action::DeleteAttachment {
            structure: structure.clone(),
            set: *set,
            phase: *phase,
            confirm: false,
        },
        Selection::AlignmentPoint { route, index } => Action::Command {
            what: format!("deleted alignment point {} of {route}", index + 1),
            command: ols_session::Command::RemoveAlignmentPoint {
                route: crate::editors::route_key(line, route),
                index: *index,
            },
        },
        _ => return None,
    })
}

/// G opens "Go to structure" (not while typing).
fn goto_key(ctx: &egui::Context, r: &mut Ready) {
    let open_key = !ctx.text_edit_focused() && ctx.input(|i| i.key_pressed(egui::Key::G));
    if open_key && r.goto.is_none() {
        let current = match &r.selection {
            Selection::Structure(id) => Some(id.clone()),
            _ => None,
        };
        r.goto = current.or_else(|| r.model.line.structures.first().map(|s| s.id.clone()));
    }
}

/// When a tool window opens (Go to, Move, a deletion's confirmation): it
/// opens at the crosshair in first person (else at the mouse), and a
/// captured mouse is freed — a window needs the cursor.
fn popups(ctx: &egui::Context, r: &mut Ready) {
    let open =
        r.goto.is_some() || r.move_dialog.is_some() || r.confirm.is_some() || r.editor.is_some();
    if open && !r.popup_open {
        let crosshair = r.view3d.rect.map(|x| x.center());
        let at = if r.view3d.captured {
            crosshair
        } else {
            ctx.pointer_latest_pos().or(crosshair)
        };
        r.popup_at = at.unwrap_or(egui::pos2(400.0, 200.0));
        r.popup_serial += 1;
        if r.view3d.captured {
            crate::view3d::View3d::set_captured(&mut r.view3d.captured, ctx, false);
        }
    }
    r.popup_open = open;
}

/// A tool window: opens next to [`Ready::popup_at`] (again each time it
/// opens), can be dragged anywhere, also partly off the screen.
fn tool_window(r: &Ready, key: &str, title: impl Into<egui::WidgetText>) -> egui::Window<'static> {
    egui::Window::new(title)
        .id(egui::Id::new((key.to_owned(), r.popup_serial)))
        .default_pos(r.popup_at + egui::vec2(16.0, 16.0))
        .collapsible(false)
        .resizable(false)
        .constrain(false)
}

/// The Go to window: Enter goes, Escape closes.
fn goto_window(ctx: &egui::Context, r: &mut Ready) {
    let Some(mut chosen) = r.goto.clone() else {
        return;
    };
    let mut go = false;
    let mut close = false;
    tool_window(r, "goto", "Go to structure").show(ctx, |ui| {
        egui::ComboBox::from_id_salt("goto-structure")
            .selected_text(&chosen)
            .width(220.0)
            .height(400.0)
            .show_ui(ui, |ui| {
                for s in &r.model.line.structures {
                    let text = match &s.structure_type {
                        Some(t) => format!("{}  ({t})", s.id),
                        None => s.id.clone(),
                    };
                    ui.selectable_value(&mut chosen, s.id.clone(), text);
                }
            });
        ui.horizontal(|ui| {
            go |= ui.button("Go").clicked();
            close |= ui.button("Close").clicked();
        });
    });
    go |= ctx.input(|i| i.key_pressed(egui::Key::Enter));
    close |= ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if go {
        r.go_to_structure(&chosen);
        r.goto = None;
    } else if close {
        r.goto = None;
    } else {
        r.goto = Some(chosen);
    }
}

/// Shortcuts (not while typing in a text field): ⌘Z undo, Shift+⌘Z redo,
/// Delete or Backspace deletes the selected structure, M holds the Move
/// tool, E opens the inventory, 1–9 choose a hotbar slot; the wheel scrolls
/// the hotbar while the mouse is captured.
fn edit_keys(ctx: &egui::Context, r: &mut Ready) {
    use crate::tools::Tool;
    // The wire being strung: Enter ends it, Backspace takes the last point
    // back, Escape (with a free mouse) gives it up.
    let windows = r.editor.is_some() || r.goto.is_some() || r.move_dialog.is_some();
    if !r.chain.is_empty() && !ctx.text_edit_focused() && !windows {
        let captured = r.view3d.captured;
        let (enter, back, escape) = ctx.input_mut(|i| {
            use egui::{Key, Modifiers};
            (
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Backspace),
                !captured && i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if enter {
            r.finish_chain();
        } else if escape {
            r.cancel_chain();
        } else if back {
            r.chain.pop();
            r.update_draft();
        }
    }
    if !ctx.text_edit_focused() {
        // Redo first: Shift+⌘Z also matches ⌘Z when shift is ignored.
        let (redo, undo, delete, move_key, inventory, slot) = ctx.input_mut(|i| {
            use egui::{Key, Modifiers};
            let redo = i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
            let undo = !redo && i.consume_key(Modifiers::COMMAND, Key::Z);
            let digits = [
                Key::Num1,
                Key::Num2,
                Key::Num3,
                Key::Num4,
                Key::Num5,
                Key::Num6,
                Key::Num7,
                Key::Num8,
                Key::Num9,
            ];
            (
                redo,
                undo,
                i.consume_key(Modifiers::NONE, Key::Delete)
                    || i.consume_key(Modifiers::NONE, Key::Backspace),
                i.consume_key(Modifiers::NONE, Key::M),
                i.consume_key(Modifiers::NONE, Key::E),
                digits
                    .iter()
                    .position(|k| i.consume_key(Modifiers::NONE, *k)),
            )
        });
        let editable = r.model.can_edit().is_ok();
        if undo && editable {
            r.perform(crate::edit::Action::Undo);
        }
        if redo && editable {
            r.perform(crate::edit::Action::Redo);
        }
        if move_key {
            r.hotbar.hold(Tool::Move);
        }
        if r.inventory && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            r.inventory = false;
        }
        if inventory {
            r.inventory = !r.inventory;
            if r.inventory && r.view3d.captured {
                crate::view3d::View3d::set_captured(&mut r.view3d.captured, ctx, false);
            }
        }
        if let Some(k) = slot {
            r.hotbar.active = k;
        }
        if delete
            && editable
            && let Some(action) = delete_action(&r.model.line, &r.selection)
        {
            r.perform(action);
        }
    }
    // Captured mouse: the wheel (without Ctrl) scrolls the hotbar, wheel
    // down to the next slot.
    if r.view3d.captured {
        let (lines, points, now) = ctx.input(|i| {
            let (mut lines, mut points) = (0.0f32, 0.0f32);
            for e in &i.events {
                if let egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } = e
                    && !modifiers.ctrl
                {
                    log::debug!("hotbar wheel: {unit:?} {delta:?}");
                    match unit {
                        egui::MouseWheelUnit::Point => points += delta.y,
                        _ => lines += delta.y.signum(),
                    }
                }
            }
            (lines, points, i.time)
        });
        if let Some(step) = r.wheel.step(lines, points, now) {
            r.hotbar.scroll(step);
        }
    }
}

/// The tool windows: hotbar, inventory, Go to, Move structure, the
/// editor, the confirmation of a deletion.
fn tool_windows(ctx: &egui::Context, r: &mut Ready) {
    // At the bottom of the 3D view.
    let at = r
        .view3d
        .rect
        .map_or(ctx.content_rect().center_bottom(), |v| v.center_bottom());
    crate::tools::hotbar_ui(ctx, &mut r.hotbar, at, r.inventory);
    crate::tools::inventory_ui(ctx, &mut r.inventory, &mut r.hotbar);
    goto_window(ctx, r);
    move_window(ctx, r);
    editor_window(ctx, r);
    let Some((impact, confirmed)) = r.confirm.clone() else {
        return;
    };
    let mut decision: Option<bool> = None;
    tool_window(r, "confirm", format!("Delete {}?", impact.what())).show(ctx, |ui| {
        ui.label("This also deletes or changes:");
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                for (title, items) in impact.groups() {
                    if items.is_empty() {
                        continue;
                    }
                    ui.label(egui::RichText::new(format!("{title} ({})", items.len())).strong());
                    for it in items {
                        ui.label(egui::RichText::new(format!("  {it}")).small());
                    }
                }
            });
        ui.label("It is one log entry: undo brings everything back.");
        ui.horizontal(|ui| {
            if ui.button("Delete all").clicked() {
                decision = Some(true);
            }
            if ui.button("Cancel").clicked() {
                decision = Some(false);
            }
        });
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        decision = Some(false);
    }
    match decision {
        Some(true) => {
            r.perform(confirmed);
        }
        Some(false) => r.confirm = None,
        None => {}
    }
}

/// The editor window (structure, string, section): Apply (Enter) makes one
/// command and closes it — unless the session refuses it (the reason is
/// shown, the window stays); Cancel (Escape) closes it.
fn editor_window(ctx: &egui::Context, r: &mut Ready) {
    use crate::editors::{Form, Outcome};
    let Some(mut form) = r.editor.clone() else {
        return;
    };
    let mut outcome = None;
    tool_window(r, "editor", form.title()).show(ctx, |ui| {
        outcome = form.ui(ui, &r.model, r.case);
        if let Some((text, true)) = &r.status {
            ui.colored_label(egui::Color32::LIGHT_RED, egui::RichText::new(text).small());
        }
    });
    if outcome.is_none() {
        outcome = ctx.input(|i| {
            if i.key_pressed(egui::Key::Escape) {
                Some(Outcome::Cancel)
            } else if i.key_pressed(egui::Key::Enter) {
                Some(Outcome::Apply)
            } else {
                None
            }
        });
    }
    let apply = |r: &mut Ready, form: Form, reset: bool| match form.command(&r.model, reset) {
        Ok((what, command)) => {
            if r.perform(crate::edit::Action::Command { what, command }) {
                r.editor = None;
                // Select what was made.
                match &form {
                    Form::Structure(f) if f.new => r.select(Selection::Structure(f.id.clone())),
                    Form::Section(f) if f.new => r.select(Selection::Span {
                        section: f.id.clone(),
                        span: 0,
                    }),
                    Form::String(f) if f.new => r.select(Selection::Attachment {
                        structure: f.structure.clone(),
                        set: f.set,
                        phase: f.phase,
                    }),
                    _ => {}
                }
            } else {
                r.editor = Some(form);
            }
        }
        Err(e) => {
            r.status = Some((e, true));
            r.editor = Some(form);
        }
    };
    match outcome {
        Some(Outcome::Apply) => apply(r, form, false),
        Some(Outcome::ResetString) => apply(r, form, true),
        Some(Outcome::PickPoints) => {
            r.editor = None;
            if let Form::Section(f) = &form
                && !f.new
            {
                r.restring = Some(f.id.clone());
            }
            r.chain.clear();
            r.hotbar.hold(crate::tools::Tool::StringWire);
            r.status = Some((
                "String wire: right-click the strings in line order, then Enter".into(),
                false,
            ));
        }
        Some(Outcome::Cancel) => {
            r.editor = None;
            r.status = None;
        }
        None => r.editor = Some(form),
    }
}

/// The "Move structure" window: E, N, z (or on the ground), previewed
/// live; Apply (Enter) moves it with one command, Cancel (Escape) puts it
/// back.
fn move_window(ctx: &egui::Context, r: &mut Ready) {
    use crate::edit::Action;
    let Some(mut d) = r.move_dialog.clone() else {
        return;
    };
    let (mut apply, mut cancel) = (false, false);
    tool_window(r, "move", format!("Move structure {}", d.id)).show(ctx, |ui| {
        egui::Grid::new("move-grid").num_columns(3).show(ui, |ui| {
            for (k, name) in ["E", "N", "z"].into_iter().enumerate() {
                ui.label(name);
                let enabled = k < 2 || !d.on_ground;
                ui.add_enabled(
                    enabled,
                    egui::DragValue::new(&mut d.position[k])
                        .speed(0.1)
                        .max_decimals(3),
                );
                ui.label(
                    egui::RichText::new(format!("Δ {:+.3} m", d.position[k] - d.start[k])).small(),
                );
                ui.end_row();
            }
        });
        ui.checkbox(&mut d.on_ground, "base on the ground");
        let horizontal = (d.position[0] - d.start[0]).hypot(d.position[1] - d.start[1]);
        ui.label(egui::RichText::new(format!("moved {horizontal:.3} m horizontally")).small());
        ui.horizontal(|ui| {
            apply |= ui.button("Apply").on_hover_text("Enter").clicked();
            cancel |= ui.button("Cancel").on_hover_text("Escape").clicked();
        });
    });
    if d.on_ground
        && let Some(z) = r.model.ground_at(d.position[0], d.position[1])
    {
        d.position[2] = z;
    }
    apply |= ctx.input(|i| i.key_pressed(egui::Key::Enter));
    cancel |= ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if cancel {
        r.cancel_move_preview();
        return;
    }
    if apply {
        r.move_dialog = None;
        if d.position != d.start {
            r.perform(Action::Move {
                id: d.id,
                position: d.position,
            });
        }
        return;
    }
    if d.position != d.shown {
        d.shown = d.position;
        r.perform(Action::PreviewMove {
            id: d.id.clone(),
            position: d.position,
        });
    }
    r.move_dialog = Some(d);
}

/// Clearances: takes a finished check, starts one when the model changed
/// (with "check after edits" on) or no check ran yet.
fn clearance_frame(ctx: &egui::Context, r: &mut Ready) {
    if r.clearances.poll() {
        let f = r.clearances.findings();
        r.view3d.marks = f.iter().map(|f| (f.at, f.target)).collect();
        r.plan.marks = f.iter().map(|f| f.at).collect();
        r.view3d.dirty.insert(Layer::Clearances);
    }
    // Checks run on demand (Check now, Checks menu), or after every edit
    // when asked.
    if r.clearances.stale && r.clearances.auto && r.clearances.report.is_some() {
        start_clearance_check(ctx, r);
    }
}

fn start_clearance_check(ctx: &egui::Context, r: &mut Ready) {
    let m = &r.model;
    let Some(solved) = m.cases.get(r.case) else {
        return;
    };
    r.clearances.start(ctx, |rules| crate::clearance::Input {
        line: m.line.clone(),
        case: solved.case.name.clone(),
        shapes: solved.shapes.clone(),
        jumpers: solved.jumpers.clone(),
        rules,
        scheme: m.scheme.clone(),
        cloud: m.cloud.clone(),
        ground: m.ground.clone(),
        corridor: m.sources.corridor,
    });
}

/// The Clearances tab: rule file, the violations of the shown case (click
/// one to go there).
fn clearance_panel(ui: &mut egui::Ui, r: &mut Ready) {
    let c = &mut r.clearances;
    if c.files.is_empty() {
        ui.label("No rule file (.olsr) next to the model or in libraries/clearance-rules.");
        ui.label(egui::RichText::new("Start with --rules FILE.olsr.").small());
        return;
    }
    let name = |p: &std::path::Path| {
        p.file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned())
    };
    let mut chosen = c.chosen;
    egui::ComboBox::from_id_salt("rule-file")
        .selected_text(
            chosen
                .and_then(|k| c.files.get(k))
                .map_or("choose rules…".into(), |p| name(p)),
        )
        .width(220.0)
        .show_ui(ui, |ui| {
            for (k, p) in c.files.iter().enumerate() {
                ui.selectable_value(&mut chosen, Some(k), name(p))
                    .on_hover_text(p.display().to_string());
            }
        });
    if chosen != c.chosen
        && let Some(k) = chosen
    {
        c.choose(k);
    }
    let mut check = false;
    ui.horizontal(|ui| {
        check = ui.button("Check now").clicked();
        ui.checkbox(&mut c.auto, "after every edit");
        if c.running() {
            ui.spinner();
        }
    });
    match &c.rules {
        Some(Err(e)) => {
            ui.colored_label(egui::Color32::LIGHT_RED, e);
        }
        Some(Ok(rules)) => {
            ui.label(egui::RichText::new(format!("{} rules", rules.len())).small());
        }
        None => {}
    }
    if c.stale && !c.running() && c.report.is_some() {
        ui.colored_label(egui::Color32::YELLOW, "out of date: the model changed");
    }
    let mut go: Option<crate::clearance::Finding> = None;
    match &c.report {
        Some(Err(e)) => {
            ui.colored_label(egui::Color32::LIGHT_RED, e);
        }
        Some(Ok(rep)) => {
            let n = rep.findings.len();
            let head = format!(
                "case {}: {n} violation{} ({:.1?})",
                rep.case,
                if n == 1 { "" } else { "s" },
                rep.took
            );
            if n == 0 {
                ui.colored_label(egui::Color32::LIGHT_GREEN, head);
            } else {
                ui.colored_label(egui::Color32::from_rgb(255, 120, 100), head);
            }
            for (rule, text) in &rep.rules {
                ui.label(egui::RichText::new(format!("{rule}: {text}")).small());
            }
            for note in &rep.notes {
                ui.label(egui::RichText::new(note).small().weak());
            }
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for f in &rep.findings {
                    let other = f
                        .other
                        .as_ref()
                        .map_or(String::new(), |o| format!(" – {}", o.describe()));
                    let class = f.class.map_or(String::new(), |k| {
                        format!(
                            ", class {k}{}",
                            r.model
                                .scheme
                                .class(k)
                                .map_or(String::new(), |d| format!(" {}", d.name))
                        )
                    });
                    let text = format!(
                        "{}{other} — {}: {} {:.2} m < {:.2} m{class}{}",
                        f.wire.describe(),
                        f.rule,
                        f.measure,
                        f.distance,
                        f.required,
                        if f.count > 1 {
                            format!(" ({} below)", f.count)
                        } else {
                            String::new()
                        }
                    );
                    let selected = r.selection == f.wire;
                    if ui
                        .selectable_label(selected, egui::RichText::new(text).small())
                        .clicked()
                    {
                        go = Some(f.clone());
                    }
                }
            });
        }
        None if c.running() => {
            ui.label("checking…");
        }
        None => {}
    }
    if check {
        r.clearances.stale = true;
        start_clearance_check(ui.ctx(), r);
    }
    if let Some(f) = go {
        r.selection = f.wire;
        r.view3d.dirty.insert(Layer::Selection);
        if let Some(cam) = r.view3d.camera.as_mut() {
            cam.target = f.at;
            cam.distance = cam.distance.min(80.0);
        }
        r.plan.centre_on(f.at);
    }
}

/// Shows `tab`: brings it to the front if it is open; library pages open
/// together in a floating window, the other pages next to the first one.
pub fn open_page(dock: &mut DockState<Tab>, tab: Tab) {
    if let Some(path) = dock.find_tab(&tab) {
        let _ = dock.set_active_tab(path);
        return;
    }
    let group = |t: &Tab| match t {
        Tab::Library(_) => 1,
        Tab::Table(_) => 2,
        _ => 0,
    };
    if group(&tab) > 0 {
        if let Some(path) = dock.find_tab_from(|t| group(t) == group(&tab)) {
            dock.set_focused_node_and_surface(path.node_path());
            dock.push_to_focused_leaf(tab);
        } else {
            let surface = dock.add_window(vec![tab]);
            if let Some(w) = dock.get_window_state_mut(surface) {
                w.set_position(egui::pos2(140.0, 110.0));
                w.set_size(if group(&tab) == 2 {
                    egui::vec2(1250.0, 560.0)
                } else {
                    egui::vec2(900.0, 620.0)
                });
            }
        }
        return;
    }
    dock.push_to_first_leaf(tab);
}

/// Closes `tab` if it is open.
fn close_page(dock: &mut DockState<Tab>, tab: Tab) {
    if let Some(path) = dock.find_tab(&tab) {
        dock.remove_tab(path);
    }
}

/// The menus: File, Edit (with Snap), View (pages, layers, cases), Tools,
/// Checks, Libraries, Tables, Plugins, Help. Commands go to `out`.
fn menus(
    ui: &mut egui::Ui,
    r: &mut Ready,
    dock: &mut DockState<Tab>,
    out: &mut Vec<crate::menu::Command>,
    customize: &mut bool,
) {
    use crate::menu::Command;
    let item = |ui: &mut egui::Ui, c: Command, out: &mut Vec<Command>| {
        if c.menu_item(ui) {
            out.push(c);
            ui.close();
        }
    };
    ui.menu_button("File", |ui| {
        ui.label(egui::RichText::new(r.model.sources.file.display().to_string()).small());
        ui.separator();
        if ui.button("Quit").clicked() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    });
    ui.menu_button("Edit", |ui| {
        let editable = r.model.can_edit().is_ok();
        ui.add_enabled_ui(editable, |ui| {
            item(ui, Command::Undo, out);
            item(ui, Command::Redo, out);
            item(ui, Command::DeleteSelected, out);
        });
        item(ui, Command::ClearSelection, out);
        item(ui, Command::GoTo, out);
        ui.separator();
        ui.menu_button("Snap: what can be clicked", |ui| {
            snap_panel(ui, &mut r.snaps)
        });
    });
    ui.menu_button("View", |ui| {
        item(ui, Command::FitViews, out);
        ui.separator();
        ui.label(egui::RichText::new("Pages").small().weak());
        for tab in Tab::VIEWS {
            let mut open = dock.find_tab(&tab).is_some();
            if ui.checkbox(&mut open, tab.title()).changed() {
                if open {
                    open_page(dock, tab);
                } else {
                    close_page(dock, tab);
                }
            }
        }
        ui.separator();
        ui.menu_button("Layers", |ui| {
            for layer in Layer::ALL {
                let mut on = r.visible.contains(&layer);
                if ui.checkbox(&mut on, layer.name()).changed() {
                    if on {
                        r.visible.insert(layer);
                    } else {
                        r.visible.remove(&layer);
                    }
                }
            }
        });
        ui.menu_button("Weather case", |ui| {
            let mut chosen = None;
            for (k, c) in r.model.cases.iter().enumerate() {
                if ui.radio(r.case == k, &c.case.name).clicked() {
                    chosen = Some(k);
                }
            }
            if let Some(k) = chosen
                && k != r.case
            {
                r.set_case(k);
            }
        });
        ui.separator();
        if ui.button("Customize the quick bar…").clicked() {
            *customize = true;
            ui.close();
        }
    });
    ui.menu_button("Tools", |ui| {
        item(ui, Command::Inventory, out);
        ui.separator();
        for t in crate::tools::Tool::ALL {
            let held = r.hotbar.tool() == t;
            let mut b = egui::Button::selectable(held, format!("{} {}", t.icon(), t.name()));
            if let Some(k) = t.key() {
                b = b.shortcut_text(k);
            }
            if ui.add(b).on_hover_text(t.help()).clicked() {
                out.push(Command::Hold(t));
                ui.close();
            }
        }
    });
    ui.menu_button("Checks", |ui| {
        ui.label(
            egui::RichText::new("Run on demand; the model is not changed")
                .small()
                .weak(),
        );
        ui.separator();
        ui.label(egui::RichText::new("Clearances").strong());
        item(ui, Command::RunChecks, out);
        let c = &mut r.clearances;
        ui.checkbox(&mut c.auto, "also after every edit");
        if !c.files.is_empty() {
            ui.menu_button("Rule file", |ui| {
                let mut chosen = None;
                for (k, p) in c.files.iter().enumerate() {
                    let name = p
                        .file_name()
                        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                    if ui.radio(c.chosen == Some(k), name).clicked() {
                        chosen = Some(k);
                    }
                }
                if let Some(k) = chosen {
                    c.choose(k);
                }
            });
        }
        if ui.button("Show results").clicked() {
            open_page(dock, Tab::Clearances);
            ui.close();
        }
        ui.separator();
        ui.add_enabled(false, egui::Button::new("Tension limits (to come)"));
        ui.add_enabled(false, egui::Button::new("Insulator swing (to come)"));
    });
    ui.menu_button("Libraries", |ui| {
        for k in crate::library::LibraryKind::ALL {
            item(ui, Command::Library(k), out);
        }
    });
    ui.menu_button("Tables", |ui| {
        for k in crate::tables::TableKind::ALL {
            item(ui, Command::Table(k), out);
        }
    });
    ui.menu_button("Plugins", |ui| {
        ui.add_enabled(false, egui::Button::new("Plugin manager (to come)"));
    });
    ui.menu_button("Help", |ui| {
        ui.label(format!("OLS CAD {}", env!("CARGO_PKG_VERSION")));
        if ui
            .button("Keys and mouse")
            .on_hover_text("In the Model page: the 3D controls and every key")
            .clicked()
        {
            open_page(dock, Tab::Model);
            ui.close();
        }
    });
}

/// Runs a command of the menus or the quick bar.
fn run_command(
    ctx: &egui::Context,
    r: &mut Ready,
    dock: &mut DockState<Tab>,
    c: crate::menu::Command,
) {
    use crate::edit::Action;
    use crate::menu::Command;
    let editable = r.model.can_edit().is_ok();
    match c {
        Command::Undo if editable => {
            r.perform(Action::Undo);
        }
        Command::Redo if editable => {
            r.perform(Action::Redo);
        }
        Command::Undo | Command::Redo => {}
        Command::FitViews => {
            r.view3d.camera = None;
            r.plan.refit();
        }
        Command::ClearSelection => r.select(Selection::None),
        Command::DeleteSelected => {
            if editable && let Some(a) = delete_action(&r.model.line, &r.selection) {
                r.perform(a);
            }
        }
        Command::GoTo => {
            r.goto = match &r.selection {
                Selection::Structure(id) => Some(id.clone()),
                _ => r.model.line.structures.first().map(|s| s.id.clone()),
            };
        }
        Command::Inventory => {
            r.inventory = true;
            if r.view3d.captured {
                crate::view3d::View3d::set_captured(&mut r.view3d.captured, ctx, false);
            }
        }
        Command::RunChecks => {
            r.clearances.stale = true;
            start_clearance_check(ctx, r);
            open_page(dock, Tab::Clearances);
        }
        Command::Library(k) => open_page(dock, Tab::Library(k)),
        Command::Table(k) => open_page(dock, Tab::Table(k)),
        Command::Hold(t) => r.hotbar.hold(t),
    }
}

fn snap_panel(ui: &mut egui::Ui, snaps: &mut crate::selection::Snaps) {
    use crate::selection::Snap;
    ui.label(egui::RichText::new("What can be clicked").strong());
    for kind in Snap::ALL {
        let mut on = snaps.on(kind);
        if ui.checkbox(&mut on, kind.name()).changed() {
            if on {
                snaps.0.insert(kind);
            } else {
                snaps.0.remove(&kind);
            }
        }
    }
    ui.horizontal(|ui| {
        if ui.button("All").clicked() {
            snaps.0 = Snap::ALL.into_iter().collect();
        }
        if ui.button("None").clicked() {
            snaps.0.clear();
        }
        if ui.button("Default").clicked() {
            *snaps = crate::selection::Snaps::default();
        }
    });
    ui.label(
        egui::RichText::new(
            "At about the same distance, points win over the lines they sit on, structures over wires.",
        )
        .small(),
    );
}

fn model_panel(ui: &mut egui::Ui, r: &mut Ready) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.label(egui::RichText::new("Layers").strong());
        for layer in Layer::ALL {
            let mut on = r.visible.contains(&layer);
            if ui.checkbox(&mut on, layer.name()).changed() {
                if on {
                    r.visible.insert(layer);
                } else {
                    r.visible.remove(&layer);
                }
            }
        }
        if let Some(cam) = r.view3d.camera.as_mut() {
            ui.add(
                egui::Slider::new(&mut cam.exaggeration, 1.0..=10.0)
                    .text("height ×")
                    .logarithmic(true),
            );
        }
        ui.separator();
        ui.label(egui::RichText::new("Controls (3D)").strong());
        let c = &mut r.view3d.controls;
        egui::ComboBox::from_id_salt("controls")
            .selected_text(c.name)
            .show_ui(ui, |ui| {
                for p in crate::controls::Controls::presets() {
                    if ui.selectable_label(c.name == p.name, p.name).clicked() {
                        log::info!("controls: {}", p.name);
                        // Keep the chosen speed.
                        let speed = c.speed;
                        *c = p;
                        c.speed = speed;
                    }
                }
            });
        ui.add(
            egui::Slider::new(&mut c.speed, 1.0..=2000.0)
                .text("m/s")
                .logarithmic(true),
        );
        if c.capture {
            // Shown ×1000 (milliradians per pixel) for a readable number.
            let mut mrad = c.mouse_sensitivity * 1000.0;
            if ui
                .add(
                    egui::Slider::new(&mut mrad, 0.3..=10.0)
                        .text("look sensitivity")
                        .logarithmic(true),
                )
                .changed()
            {
                c.mouse_sensitivity = mrad / 1000.0;
            }
        }
        egui::CollapsingHeader::new("keys and mouse")
            .id_salt("controls-list")
            .show(ui, |ui| {
                egui::Grid::new("controls-grid")
                    .striped(true)
                    .show(ui, |ui| {
                        for (k, v) in c.describe() {
                            ui.label(egui::RichText::new(k).small().strong());
                            ui.label(egui::RichText::new(v).small());
                            ui.end_row();
                        }
                    });
            });
        ui.separator();
        ui.label(egui::RichText::new("Weather cases").strong());
        let mut chosen = None;
        for (k, c) in r.model.cases.iter().enumerate() {
            let text = format!("{} — {}", c.case.name, c.case.summary());
            if ui.radio(r.case == k, text).clicked() && r.case != k {
                chosen = Some(k);
            }
            if !c.failed.is_empty() {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    format!("{} sections or jumpers failed", c.failed.len()),
                );
            }
        }
        if let Some(k) = chosen {
            r.set_case(k);
        }
        ui.separator();
        ui.label(egui::RichText::new("Loaded").strong());
        ui.label(egui::RichText::new(r.model.sources.file.display().to_string()).small());
        for l in &r.model.log {
            ui.label(egui::RichText::new(l).small());
        }
    });
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        match &self.state {
            State::Ready(r) => {
                r.view3d.controls.save(storage);
                r.snaps.save(storage);
                r.hotbar.save(storage);
            }
            _ => {
                self.controls.save(storage);
                self.snaps.save(storage);
                self.hotbar.save(storage);
            }
        }
        self.quickbar.save(storage);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let State::Loading { rx, progress } = &mut self.state {
            let mut done = None;
            while let Ok(m) = rx.try_recv() {
                match m {
                    Message::Progress(p) => progress.push(p),
                    Message::Done(r) => done = Some(r),
                }
            }
            match done {
                Some(Ok(model)) => {
                    let mut ready = Ready::new(*model);
                    ready.view3d.controls = self.controls.clone();
                    ready.snaps = self.snaps.clone();
                    ready.hotbar = self.hotbar.clone();
                    self.state = State::Ready(Box::new(ready));
                }
                Some(Err(e)) => self.state = State::Failed(e),
                None => {}
            }
        }
        let file = self.sources.file.display().to_string();
        match &mut self.state {
            State::Loading { progress, .. } => {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.heading(format!("Loading {file}"));
                    ui.spinner();
                    for p in progress.iter() {
                        ui.label(p);
                    }
                });
            }
            State::Failed(e) => {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.heading(format!("Could not load {file}"));
                    ui.colored_label(egui::Color32::LIGHT_RED, e.as_str());
                });
            }
            State::Ready(ready) => {
                goto_key(ui.ctx(), ready);
                edit_keys(ui.ctx(), ready);
                clearance_frame(ui.ctx(), ready);
                popups(ui.ctx(), ready);
                tool_windows(ui.ctx(), ready);
                egui::Panel::bottom("status").show(ui, |ui| {
                    ui.horizontal(|ui| {
                        match ready.model.can_edit() {
                            Ok(()) => ui.label(
                                egui::RichText::new(format!(
                                    "editing as {}",
                                    ready.model.editing.as_ref().map_or("", |e| e.user.as_str())
                                ))
                                .small(),
                            ),
                            Err(why) => {
                                ui.label(egui::RichText::new(format!("read-only: {why}")).small())
                            }
                        };
                        if let Some((text, error)) = &ready.status {
                            ui.separator();
                            let t = egui::RichText::new(text).small();
                            if *error {
                                ui.colored_label(egui::Color32::LIGHT_RED, t);
                            } else {
                                ui.label(t);
                            }
                        }
                    });
                });
                let mut commands: Vec<crate::menu::Command> = Vec::new();
                egui::Panel::top("top").show(ui, |ui| {
                    egui::MenuBar::new().ui(ui, |ui| {
                        menus(
                            ui,
                            ready,
                            &mut self.dock,
                            &mut commands,
                            &mut self.customize,
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.strong(&ready.model.line.name);
                        ui.separator();
                        let case_name = ready
                            .model
                            .cases
                            .get(ready.case)
                            .map_or(String::new(), |c| c.case.name.clone());
                        let mut case = ready.case;
                        egui::ComboBox::from_id_salt("case")
                            .selected_text(format!("case: {case_name}"))
                            .show_ui(ui, |ui| {
                                for (k, c) in ready.model.cases.iter().enumerate() {
                                    ui.selectable_value(&mut case, k, &c.case.name);
                                }
                            });
                        if case != ready.case {
                            ready.set_case(case);
                        }
                        ui.separator();
                        ui.label(ready.selection.describe());
                        ui.separator();
                        if let Some(c) = self.quickbar.ui(ui) {
                            commands.push(c);
                        }
                        if ui
                            .small_button("⚙")
                            .on_hover_text("Customize the quick bar")
                            .clicked()
                        {
                            self.customize = true;
                        }
                    });
                });
                for c in commands {
                    run_command(ui.ctx(), ready, &mut self.dock, c);
                }
                if self.customize {
                    let mut open = true;
                    egui::Window::new("Customize the quick bar")
                        .open(&mut open)
                        .collapsible(false)
                        .show(ui.ctx(), |ui| self.quickbar.customize_ui(ui));
                    self.customize = open;
                }
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        DockArea::new(&mut self.dock)
                            .show_close_buttons(true)
                            .show_leaf_close_all_buttons(false)
                            .show_inside(
                                ui,
                                &mut Viewer {
                                    ready,
                                    rs: self.render_state.as_ref(),
                                },
                            );
                    });
            }
        }
    }
}
