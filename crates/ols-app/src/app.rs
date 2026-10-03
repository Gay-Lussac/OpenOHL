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
}

enum Message {
    Progress(String),
    Done(Result<Box<Model>, String>),
}

/// What is shown once the model is loaded.
pub struct Ready {
    /// The model.
    pub model: Model,
    /// Weather case shown.
    pub case: usize,
    /// Selected object.
    pub selection: Selection,
    /// Visible 3D layers.
    pub visible: HashSet<Layer>,
    /// 3D view.
    pub view3d: View3d,
    /// Plan view.
    pub plan: PlanView,
    /// Profile view.
    pub profile: ProfileView,
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
            visible,
            view3d,
            plan: PlanView::default(),
            profile: ProfileView::default(),
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
    let [centre, _] = tree.split_left(centre, 0.2, vec![Tab::Model]);
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
            Tab::View3d => r
                .view3d
                .ui(ui, self.rs, &r.model, r.case, &mut r.selection, &r.visible),
            Tab::Plan => r.plan.ui(ui, &r.model, r.case, &mut r.selection),
            Tab::Profile => r.profile.ui(ui, &r.model, r.case, &mut r.selection),
            Tab::Properties => crate::properties::ui(ui, &r.model, r.case, &mut r.selection),
            Tab::Model => model_panel(ui, r),
        }
        if r.selection != before {
            r.view3d.dirty.insert(Layer::Selection);
        }
    }
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
            State::Ready(r) => r.view3d.controls.save(storage),
            _ => self.controls.save(storage),
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
