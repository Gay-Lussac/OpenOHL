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

/// The tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// What can be clicked.
    Snap,
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
    /// A deletion waiting for its confirmation: what it takes with it.
    pub confirm: Option<Box<ols_session::Impact>>,
    /// Tools: the hotbar (the active tool says what a click does).
    pub hotbar: crate::tools::Hotbar,
    /// The inventory window is open.
    pub inventory: bool,
    /// The "Move structure" window, when open.
    pub move_dialog: Option<MoveDialog>,
    /// Wheel movement not yet a whole hotbar step.
    wheel_rest: f32,
    /// The slot before the Go to tool was chosen.
    previous_slot: usize,
    /// A position being typed in the properties: (structure, position).
    pub edit_position: Option<(String, [f64; 3])>,
}

impl Ready {
    /// A model ready to show.
    pub fn new(model: Model) -> Self {
        let mut view3d = View3d::default();
        view3d.rebuild_all();
        let mut visible: HashSet<Layer> = Layer::ALL.into_iter().collect();
        visible.remove(&Layer::OtherCases);
        Self {
            model,
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
            hotbar: crate::tools::Hotbar::default(),
            inventory: false,
            move_dialog: None,
            wheel_rest: 0.0,
            previous_slot: 0,
            edit_position: None,
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
    let [centre, _] = tree.split_left(centre, 0.2, vec![Tab::Model, Tab::Snap]);
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
        }
    }

    /// Brings a tab to the front (tests).
    #[cfg(test)]
    pub fn show_tab(&mut self, tab: Tab) {
        if let Some(path) = self.dock.find_tab(&tab) {
            let _ = self.dock.set_active_tab(path);
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
        match tab {
            Tab::View3d => "3D",
            Tab::Plan => "Plan",
            Tab::Profile => "Profile",
            Tab::Properties => "Properties",
            Tab::Model => "Model",
            Tab::Snap => "Snap",
        }
        .into()
    }

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(*tab as u8)
    }

    fn closeable(&mut self, _tab: &mut Tab) -> bool {
        false
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        let r = &mut *self.ready;
        let before = r.selection.clone();
        match tab {
            Tab::View3d => r.view3d.ui(
                ui,
                self.rs,
                &r.model,
                r.case,
                &mut r.selection,
                &r.visible,
                &r.snaps,
            ),
            Tab::Plan => {
                r.plan.ui(ui, &r.model, r.case, &mut r.selection, &r.snaps);
            }
            Tab::Profile => r
                .profile
                .ui(ui, &r.model, r.case, &mut r.selection, &r.snaps),
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
            }
            Tab::Model => model_panel(ui, r),
            Tab::Snap => snap_panel(ui, &mut r.snaps),
        }
        if r.selection != before {
            r.view3d.dirty.insert(Layer::Selection);
            // A click with the active tool.
            if let Selection::Structure(id) = r.selection.clone() {
                r.use_tool_on(ui.ctx(), &id);
            }
        }
    }
}

impl Ready {
    /// The active tool used on structure `id` (clicked in a view).
    pub fn use_tool_on(&mut self, ctx: &egui::Context, id: &str) {
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
                    // The window needs the mouse.
                    if self.view3d.captured {
                        crate::view3d::View3d::set_captured(&mut self.view3d.captured, ctx, false);
                    }
                    self.move_dialog = Some(MoveDialog {
                        id: id.to_owned(),
                        start: p,
                        position: p,
                        on_ground: false,
                        shown: p,
                    });
                }
            }
            Tool::Delete => self.perform(crate::edit::Action::Delete {
                kind: ols_session::Kind::Structure,
                id: id.to_owned(),
                confirm: false,
            }),
            Tool::Select | Tool::GoTo => {}
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

    /// Performs an edit and updates the views.
    pub fn perform(&mut self, action: crate::edit::Action) {
        use crate::edit::{Action, EditError};
        let result = match action {
            Action::PreviewMove { id, position } => {
                self.model.preview_move(self.case, &id, position);
                self.refresh_views();
                return;
            }
            Action::CancelPreview { id } => {
                self.model.cancel_preview(self.case, &id);
                self.refresh_views();
                return;
            }
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
        match result {
            Ok(report) => {
                log::info!("{report}");
                self.status = Some((report, false));
                self.confirm = None;
            }
            Err(EditError::NeedsConfirmation(impact)) => self.confirm = Some(impact),
            Err(e) => {
                log::warn!("{e}");
                self.status = Some((e.to_string(), true));
            }
        }
        self.edit_position = None;
        if !self.selection_exists() {
            self.selection = Selection::None;
        }
        self.refresh_views();
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

/// G opens "Go to structure" (not while typing; a captured mouse is
/// freed first); Enter goes, Escape closes.
fn goto_window(ctx: &egui::Context, r: &mut Ready) {
    let open_key = !ctx.text_edit_focused() && ctx.input(|i| i.key_pressed(egui::Key::G));
    if open_key && r.goto.is_none() {
        if r.view3d.captured {
            crate::view3d::View3d::set_captured(&mut r.view3d.captured, ctx, false);
        }
        let current = match &r.selection {
            Selection::Structure(id) => Some(id.clone()),
            _ => None,
        };
        r.goto = current.or_else(|| r.model.line.structures.first().map(|s| s.id.clone()));
    }
    let Some(mut chosen) = r.goto.clone() else {
        return;
    };
    let mut go = false;
    let mut close = false;
    egui::Window::new("Go to structure")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 80.0))
        .show(ctx, |ui| {
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

/// Shortcuts (not while typing in a text field): ⌘Z undo, ⇧⌘Z redo,
/// Delete or Backspace deletes the selected structure, M holds the Move
/// tool, E opens the inventory, 1–9 choose a hotbar slot; the wheel scrolls
/// the hotbar while the mouse is captured. Then the windows: hotbar,
/// inventory, Move structure, the confirmation of a deletion.
fn edit_keys_and_windows(ctx: &egui::Context, r: &mut Ready) {
    use crate::edit::Action;
    use crate::tools::Tool;
    if !ctx.text_edit_focused() {
        // Redo first: ⇧⌘Z also matches ⌘Z when shift is ignored.
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
            r.perform(Action::Undo);
        }
        if redo && editable {
            r.perform(Action::Redo);
        }
        if move_key {
            r.hotbar.hold(Tool::Move);
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
            && let Selection::Structure(id) = r.selection.clone()
        {
            r.perform(Action::Delete {
                kind: ols_session::Kind::Structure,
                id,
                confirm: false,
            });
        }
    }
    // Captured mouse: the wheel (without Ctrl) scrolls the hotbar, wheel
    // down to the next slot.
    if r.view3d.captured {
        let notches: f32 = ctx.input(|i| {
            i.events
                .iter()
                .map(|e| match e {
                    egui::Event::MouseWheel {
                        unit,
                        delta,
                        modifiers,
                        ..
                    } if !modifiers.ctrl => match unit {
                        egui::MouseWheelUnit::Point => delta.y / 40.0,
                        _ => delta.y,
                    },
                    _ => 0.0,
                })
                .sum()
        });
        r.wheel_rest += notches;
        let steps = r.wheel_rest.trunc();
        if steps != 0.0 {
            r.hotbar.scroll(-steps as i32);
            r.wheel_rest -= steps;
        }
    }
    // The Go to tool opens its window and gives the hand back.
    if r.hotbar.tool() == Tool::GoTo {
        r.hotbar.active = r.previous_slot;
        if r.goto.is_none() {
            r.goto = Some(
                r.model
                    .line
                    .structures
                    .first()
                    .map(|s| s.id.clone())
                    .unwrap_or_default(),
            );
        }
    } else {
        r.previous_slot = r.hotbar.active;
    }
    crate::tools::hotbar_ui(ctx, &mut r.hotbar);
    crate::tools::inventory_ui(ctx, &mut r.inventory, &mut r.hotbar);
    move_window(ctx, r);
    let Some(impact) = r.confirm.clone() else {
        return;
    };
    let mut decision: Option<bool> = None;
    egui::Window::new(format!("Delete {} {}?", impact.kind.as_str(), impact.id))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label("This also deletes or changes:");
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    for (title, items) in impact.groups() {
                        if items.is_empty() {
                            continue;
                        }
                        ui.label(
                            egui::RichText::new(format!("{title} ({})", items.len())).strong(),
                        );
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
        Some(true) => r.perform(Action::Delete {
            kind: impact.kind,
            id: impact.id.clone(),
            confirm: true,
        }),
        Some(false) => r.confirm = None,
        None => {}
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
    egui::Window::new(format!("Move structure {}", d.id))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-20.0, 80.0))
        .show(ctx, |ui| {
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
                        egui::RichText::new(format!("Δ {:+.3} m", d.position[k] - d.start[k]))
                            .small(),
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
        for (k, c) in r.model.cases.iter().enumerate() {
            let text = format!("{} — {}", c.case.name, c.case.summary());
            if ui.radio(r.case == k, text).clicked() && r.case != k {
                r.case = k;
                r.view3d
                    .dirty
                    .extend([Layer::Wires, Layer::OtherCases, Layer::Selection]);
                r.profile.invalidate();
            }
            if !c.failed.is_empty() {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    format!("{} sections or jumpers failed", c.failed.len()),
                );
            }
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
                goto_window(ui.ctx(), ready);
                edit_keys_and_windows(ui.ctx(), ready);
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
                egui::Panel::top("top").show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong("OLS CAD");
                        ui.label(&ready.model.line.name);
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
                            ready.case = case;
                            ready.view3d.dirty.extend([
                                Layer::Wires,
                                Layer::OtherCases,
                                Layer::Selection,
                            ]);
                            ready.profile.invalidate();
                        }
                        ui.separator();
                        ui.label(ready.selection.describe());
                        if ui.button("Fit views").clicked() {
                            ready.view3d.camera = None;
                            ready.plan = PlanView::default();
                        }
                        if ready.selection != Selection::None && ui.button("Clear").clicked() {
                            ready.selection = Selection::None;
                            ready.view3d.dirty.insert(Layer::Selection);
                        }
                        ui.separator();
                        let editable = ready.model.can_edit().is_ok();
                        ui.add_enabled_ui(editable, |ui| {
                            if ui.button("Undo").on_hover_text("⌘Z").clicked() {
                                ready.perform(crate::edit::Action::Undo);
                            }
                            if ui.button("Redo").on_hover_text("⇧⌘Z").clicked() {
                                ready.perform(crate::edit::Action::Redo);
                            }
                        });
                    });
                });
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        DockArea::new(&mut self.dock)
                            .show_close_buttons(false)
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
