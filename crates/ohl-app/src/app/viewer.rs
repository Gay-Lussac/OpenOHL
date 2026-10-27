// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The pages: the dock's tab viewer, and pages opened, moved and closed.

use super::*;

pub(super) struct Viewer<'a> {
    pub(super) ready: &'a mut Ready,
    /// The context (the docked tool panels' shared state).
    pub(super) ctx: egui::Context,
    pub(super) rs: Option<&'a egui_wgpu::RenderState>,
    pub(super) lod: &'a mut crate::lod::LodSettings,
    /// The colour tool's palette.
    pub(super) palette: &'a mut crate::color_tool::Palette,
    /// The Layers list's folded groups.
    pub(super) layer_groups: &'a mut crate::scene::LayerGroups,
    /// The Python plugins' scripts (for the console).
    pub(super) scripts: Vec<(String, PathBuf)>,
}

impl TabViewer for Viewer<'_> {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        let dirty = match tab {
            Tab::Library(k) => self.ready.libraries.get(k).is_some_and(|p| p.dirty()),
            Tab::Table(k) => self.ready.tables.get(k).is_some_and(|p| p.pending() > 0),
            _ => false,
        };
        if let Tab::Tool(p) = tab {
            let title = crate::dockable::host(&self.ctx, |h| h.alive.get(p).cloned());
            return title.unwrap_or_else(|| p.name().to_owned()).into();
        }
        if let Tab::CrossSection(id) = tab {
            return self.ready.xsections.title(*id).into();
        }
        if dirty {
            format!("{} *", tab.title()).into()
        } else {
            tab.title().into()
        }
    }

    /// Right-click on a tab: where to put the page.
    fn context_menu(&mut self, ui: &mut egui::Ui, tab: &mut Tab, _path: egui_dock::NodePath) {
        for (place, text) in [
            (Place::Left, "Dock on the left"),
            (Place::Right, "Dock on the right (with Properties)"),
            (Place::Window, "Float in a window of its own"),
        ] {
            if ui.button(text).clicked() {
                self.ready.dock_moves.push((*tab, place));
                ui.close();
            }
        }
        ui.label(
            egui::RichText::new("Or drag its tab anywhere: beside, above or below another page.")
                .small()
                .weak(),
        );
    }

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(*tab)
    }

    fn closeable(&mut self, _tab: &mut Tab) -> bool {
        true
    }

    /// Library pages scroll inside their own panels (the list, the
    /// editor): the dock's scroll area around them moved those panels out
    /// of the tab when scrolled sideways.
    fn scroll_bars(&self, tab: &Tab) -> [bool; 2] {
        match tab {
            // The panel scrolls itself ([`crate::dockable`]).
            Tab::Library(_) | Tab::Tool(_) => [false, false],
            _ => [true, true],
        }
    }

    fn on_close(&mut self, tab: &mut Tab) -> egui_dock::tab_viewer::OnCloseResponse {
        // A tool's panel closed: the tool closes.
        if let Tab::Tool(p) = tab {
            crate::dockable::host(&self.ctx, |h| h.closed.insert(*p));
            self.ctx.request_repaint();
        }
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
        // A cut's tab closed: the cut goes.
        if let Tab::CrossSection(id) = tab {
            self.ready.xsections.views.remove(id);
        }
        egui_dock::tab_viewer::OnCloseResponse::Close
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        let tab_rect = ui.max_rect();
        let r = &mut *self.ready;
        let before = r.selection.clone();
        let snaps = r.effective_snaps();
        // Which click selects and which uses the held tool (by the tool
        // held, [`crate::controls::click_buttons`]).
        let select_held = r.hotbar.tool() == crate::tools::Tool::Select;
        let buttons = crate::controls::click_buttons(select_held);
        let _timed = crate::perf::span(match tab {
            Tab::View3d => "tab 3d",
            Tab::Plan => "tab plan",
            Tab::Profile => "tab profile",
            Tab::CrossSection(_) => "tab cross-section",
            Tab::Properties => "tab properties",
            Tab::Model => "tab model",
            Tab::Clearances => "tab clearances",
            _ => "tab other",
        });
        // A measuring tool held: the views find what is under the cursor
        // and draw the measurement (made only for the views drawing it;
        // the kept ones shared, made when they change).
        let measuring = r.hotbar.tool().measuring();
        let draws = matches!(tab, Tab::View3d | Tab::Plan | Tab::Profile);
        let overlay = if draws
            && (measuring || !r.measuring.done.is_empty() || !r.measuring.kept.is_empty())
        {
            r.measuring
                .overlay(crate::measure::Others::of(&r.model.section_display))
        } else {
            Default::default()
        };
        let used = match tab {
            Tab::View3d => {
                r.view3d.measuring = measuring;
                r.view3d.move_held = r.hotbar.tool() == crate::tools::Tool::Move;
                r.view3d.user_measure = overlay;
                r.view3d.link = r.link_overlay();
                let used = r.view3d.ui(
                    ui,
                    self.rs,
                    &r.model,
                    r.case,
                    &mut r.selection,
                    &r.visible,
                    &snaps,
                    self.lod,
                    select_held,
                );
                if let Some(done) = r.view3d.handles.done.take() {
                    r.handle_done(done);
                }
                if let Some(joint) = r.view3d.link.as_mut().and_then(|l| l.clicked.take()) {
                    r.link_to_joint(&joint);
                }
                // A cut's outline clicked: its panel in front.
                if let Some(id) = r.view3d.picked_cut.take() {
                    r.open_page = Some((Tab::CrossSection(id), None));
                }
                used
            }
            Tab::Plan => {
                r.plan.measuring = measuring;
                r.plan.measure = overlay;
                r.plan.ui(
                    ui,
                    &r.model,
                    r.case,
                    &mut r.selection,
                    &snaps,
                    &r.visible,
                    r.drawings.preview(&r.model, r.case),
                    buttons,
                )
            }
            Tab::Profile => {
                // Right-click: "Cross-section here" is in the object's
                // menu (the app's, [`ContextMenu`]).
                r.profile.cut_menu = false;
                r.profile.vegetation_layers = [
                    r.visible.contains(&Layer::GrowIn),
                    r.visible.contains(&Layer::FallIn),
                ];
                r.profile.measuring = measuring;
                r.profile.measure = overlay;
                r.profile.ui(
                    ui,
                    &r.model,
                    r.case,
                    &mut r.selection,
                    &snaps,
                    &mut r.drawings,
                    buttons,
                )
            }
            Tab::CrossSection(id) => {
                // A 3D view of the slab: the main view's layers, controls
                // and tools.
                let link = r.link_overlay();
                let used = r.xsections.ui(
                    *id,
                    ui,
                    crate::xsection::PanelCtx {
                        rs: self.rs,
                        model: &r.model,
                        case: r.case,
                        selection: &mut r.selection,
                        visible: &r.visible,
                        snaps: &snaps,
                        lod: self.lod,
                        select_held,
                        main: &mut r.view3d,
                        link,
                    },
                );
                // What its view did, as the main view's.
                if let Some(v) = r.xsections.views.get_mut(id) {
                    let view = &mut v.view;
                    if let Some(c) = view.picked_case.take() {
                        r.view3d.picked_case = Some(c);
                    }
                    if let Some(m) = view.picked_member.take() {
                        r.view3d.picked_member = Some(m);
                    }
                    if let Some(a) = view.area_picked.take() {
                        r.view3d.area_picked = Some(a);
                    }
                    let done = view.handles.done.take();
                    let joint = view.link.as_mut().and_then(|l| l.clicked.take());
                    if let Some(done) = done {
                        r.handle_done(done);
                    }
                    if let Some(joint) = joint {
                        r.link_to_joint(&joint);
                    }
                }
                used
            }
            Tab::Properties => {
                let editable = r.model.can_edit().is_ok();
                let mut start_link = None;
                let mut cut_at = None;
                let mut solve_all = false;
                let loads = r.loads.results.as_ref().and_then(|x| x.as_ref().ok());
                let action = crate::properties::ui(
                    ui,
                    &r.model,
                    r.case,
                    &mut r.selection,
                    editable.then_some(&mut r.edit_position),
                    crate::properties::PointExtras {
                        loads: loads.map(Vec::as_slice),
                        editable,
                        start_link: &mut start_link,
                        cut_at: &mut cut_at,
                        solve_all: &mut solve_all,
                    },
                );
                if solve_all {
                    r.solve_all = true;
                }
                if let Some(a) = action {
                    r.perform(a);
                }
                if let Some(id) = cut_at {
                    r.cross_section_at(&id);
                }
                if let Some((structure, label)) = start_link {
                    r.hotbar.hold(crate::tools::Tool::Link);
                    r.use_tool(crate::selection::Use {
                        target: Selection::Point { structure, label },
                        at: None,
                        on: None,
                    });
                }
                None
            }
            Tab::Model => {
                model_panel(ui, r, self.layer_groups);
                None
            }
            Tab::Clearances => {
                clearance_panel(ui, r);
                None
            }
            Tab::NextSteps => {
                egui::ScrollArea::vertical().show(ui, |ui| crate::next_steps::ui(ui, r));
                None
            }
            Tab::Tool(p) => {
                crate::dockable::record(ui, *p);
                None
            }
            Tab::StructureEditor => {
                r.placed_ui(ui, self.rs);
                None
            }
            Tab::Roles => {
                crate::remote::roles_ui(ui, &r.model);
                None
            }
            Tab::People => {
                match crate::remote::people_ui(ui, &mut r.model) {
                    Some(crate::remote::PeopleAction::GoTo(c)) => {
                        if let Some(cam) = r.view3d.camera.as_mut() {
                            cam.target = c;
                        }
                        r.plan.centre_on(c);
                    }
                    Some(crate::remote::PeopleAction::Op(what, op)) => r.project_op(&what, op),
                    None => {}
                }
                None
            }
            Tab::Python => {
                if let Some(path) = r.console.ui(ui, &self.scripts) {
                    r.console.run_file(path);
                }
                None
            }
            Tab::Lod => {
                let view3d = &r.view3d;
                if self
                    .lod
                    .ui(ui, &|l| view3d.lod_stats(l), view3d.stream_stats())
                {
                    ui.ctx().request_repaint();
                }
                None
            }
            Tab::Table(kind) => {
                let kind = *kind;
                let page = r.tables.entry(kind).or_default();
                let mut tools = crate::tables::TableTools {
                    palette: self.palette,
                    rs: self.rs,
                    preview: &mut r.picker_preview,
                };
                let page_action = page.ui(ui, kind, &r.model, r.case, &r.selection, &mut tools);
                let m = ui.input(|i| i.modifiers);
                match page_action {
                    // Shift (adds) or Cmd/Ctrl (toggles) + ⤴: more than one.
                    Some(crate::tables::TableAction::Select(s)) if m.shift || m.command => {
                        r.select_more(s, m.command && !m.shift);
                    }
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
                    Some(crate::tables::TableAction::Replace(id)) => r.start_replace(&id),
                    Some(crate::tables::TableAction::EditInStructureApp(id)) => {
                        r.edit_structure_in_app(&id);
                    }
                    Some(crate::tables::TableAction::CrossSection(id)) => {
                        r.cross_section_at(&id);
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
                let cx = crate::library::PageContext {
                    rs: self.rs,
                    line: Some(&r.model.line),
                    insulators: r.model.types.as_ref().map(|t| &t.1),
                    ahead: r.model.line.display.ahead_deg,
                };
                // The hotbar's Link tool held: the structure screen's too.
                page.workspace.link_held = r.hotbar.tool() == crate::tools::Tool::Link;
                let action = page.ui(ui, &sources, cx);
                let request = page.workspace.request.take();
                if let Some(action) = action {
                    r.library_saved(kind, action);
                }
                if let Some(req) = request {
                    r.workspace_request(req);
                }
                None
            }
        };
        // The weather case of the copy clicked in a view.
        if let Some(c) = r.view3d.picked_case.take().or(r.plan.picked_case.take())
            && c != r.model.selection_case
        {
            r.model.selection_case = c;
            r.view3d.dirty.insert(Layer::Selection);
        }
        // A click in a view: with Shift or Cmd/Ctrl it adds to the
        // selection or takes out; else the one clicked is the only one.
        if let Some(picked) = crate::selection::take_click(ui.ctx()) {
            let m = ui.input(|i| i.modifiers);
            if m.shift || m.command {
                r.selection = before.clone();
                r.select_more(picked, m.command && !m.shift);
            } else if !r.model.selected_also.is_empty() {
                r.model.selected_also.clear();
                r.view3d.dirty.insert(Layer::Selection);
            }
        }
        if r.selection != before {
            r.view3d.dirty.insert(Layer::Selection);
        }
        // A member clicked in the 3D view: selected in the structure screen
        // when it shows the structure's type.
        if let Some((structure, member)) = r.view3d.picked_member.take() {
            r.select_member(&structure, &member);
        }
        // A tool click uses the held tool on what is there.
        // What the cursor is on in this page's view, for the measuring
        // tools.
        let hovered = match tab {
            Tab::View3d => r.view3d.hover_use.clone(),
            Tab::Plan => r.plan.hover_use.clone(),
            Tab::Profile => r.profile.hover_use.clone(),
            _ => None,
        };
        if hovered.is_some() {
            r.measure_hover = hovered;
        }
        // The kept measurement or note under a tool click.
        let hit = match tab {
            Tab::View3d => r.view3d.annotation_hit.take(),
            Tab::Plan => r.plan.annotation_hit.take(),
            Tab::Profile => r.profile.annotation_hit.take(),
            _ => None,
        };
        if used.is_some() {
            r.clicked_annotation = hit;
        }
        if let Some(target) = used {
            r.use_tool(target);
        }
        // A right-click in a view: it selected what is there (with Select
        // held, as its tool use above); its menu opens on it (owner:
        // OHL-Design UI review, 2026-10-26).
        let view = matches!(
            tab,
            Tab::View3d | Tab::Plan | Tab::Profile | Tab::CrossSection(_)
        );
        if view && !r.view3d.captured && !r.captured_before {
            let click = ui.input(|i| {
                i.pointer
                    .button_clicked(egui::PointerButton::Secondary)
                    .then(|| i.pointer.interact_pos())
                    .flatten()
            });
            // In this page, not on a window over it.
            let here = |p: &egui::Pos2| {
                tab_rect.contains(*p) && ui.ctx().layer_id_at(*p) == Some(ui.layer_id())
            };
            if let Some(at) = click.filter(here) {
                let cut = if *tab == Tab::Profile {
                    r.profile.cut_at()
                } else {
                    None
                };
                r.context_menu =
                    (r.selection != Selection::None || cut.is_some()).then(|| super::ContextMenu {
                        at,
                        target: r.selection.clone(),
                        cut,
                        opened: ui.ctx().cumulative_frame_nr(),
                    });
            }
        }
        // The profile's "Cross-section here".
        if let Some((alignment, station)) = r.profile.cut_request.take() {
            r.open_cut(crate::xsection::CutSpec {
                alignment,
                at: crate::xsection::At::Station(station),
                depth: crate::xsection::DEFAULT_DEPTH,
                width: r.drawings.line_width(),
            });
        }
    }
}

/// Shows `tab`: brings it to the front if it is open; library pages open
/// together in a floating window, tables in another, the structure editor
/// in its own (each can be docked), the other pages next to the first one.
pub fn open_page(dock: &mut DockState<Tab>, tab: Tab) {
    if let Some(path) = dock.find_tab(&tab) {
        let _ = dock.set_active_tab(path);
        return;
    }
    // A cut: a tab beside the profile (or of the first group).
    if let Tab::CrossSection(_) = tab {
        let beside = dock
            .find_tab_from(|t| matches!(t, Tab::CrossSection(_)))
            .or_else(|| dock.find_tab(&Tab::Profile));
        match beside {
            Some(path) => {
                dock.set_focused_node_and_surface(path.node_path());
                dock.push_to_focused_leaf(tab);
            }
            None => dock.push_to_first_leaf(tab),
        }
        return;
    }
    let group = |t: &Tab| match t {
        Tab::Library(_) => 1,
        Tab::Table(_) => 2,
        Tab::Python => 3,
        Tab::StructureEditor => 4,
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
                w.set_size(match group(&tab) {
                    2 => egui::vec2(1250.0, 560.0),
                    4 => egui::vec2(1150.0, 760.0),
                    _ => egui::vec2(900.0, 620.0),
                });
            }
        }
        return;
    }
    dock.push_to_first_leaf(tab);
}

/// The tab group of the pages on the left (Model…) or on the right
/// (Properties), if there is one.
pub(super) fn page_group(dock: &DockState<Tab>, place: Place) -> Option<egui_dock::NodePath> {
    let wanted: &[Tab] = match place {
        Place::Left => &[Tab::Model, Tab::Clearances, Tab::NextSteps],
        Place::Right => &[Tab::Properties],
        Place::Window => &[],
    };
    wanted
        .iter()
        .find_map(|t| dock.find_tab(t))
        .map(|p| p.node_path())
}

/// Moves page `tab` to `place` (its tab's context menu).
pub fn move_page(dock: &mut DockState<Tab>, tab: Tab, place: Place) {
    let Some(src) = dock.find_tab(&tab) else {
        return;
    };
    match place {
        Place::Window => {
            let rect =
                egui::Rect::from_min_size(egui::pos2(160.0, 120.0), egui::vec2(820.0, 600.0));
            dock.detach_tab(src, rect);
        }
        _ => {
            let Some(dst) = page_group(dock, place) else {
                return;
            };
            if dst != src.node_path() {
                dock.move_tab(src, (dst, egui_dock::TabInsert::Append));
            }
        }
    }
    if let Some(path) = dock.find_tab(&tab) {
        let _ = dock.set_active_tab(path);
    }
}

/// The docked tool panels follow their tools ([`crate::dockable`]): a
/// panel asked for is docked (on the right, with Properties) or floats
/// again; one whose tool closed goes with it.
pub(super) fn sync_docked(ctx: &egui::Context, dock: &mut DockState<Tab>) {
    use crate::dockable::Panel;
    let (alive, docked) = crate::dockable::host(ctx, |h| {
        for (p, d) in std::mem::take(&mut h.requests) {
            if d {
                h.docked.insert(p);
            } else {
                h.docked.remove(&p);
            }
        }
        h.closed.retain(|p| h.alive.contains_key(p));
        (h.alive.clone(), h.docked.clone())
    });
    for p in Panel::ALL {
        let tab = Tab::Tool(p);
        let want = alive.contains_key(&p) && docked.contains(&p);
        match (dock.find_tab(&tab), want) {
            (Some(path), false) => {
                dock.remove_tab(path);
            }
            (None, true) => {
                match page_group(dock, Place::Right).or_else(|| page_group(dock, Place::Left)) {
                    Some(node) => {
                        dock.set_focused_node_and_surface(node);
                        dock.push_to_focused_leaf(tab);
                    }
                    None => dock.push_to_first_leaf(tab),
                }
                ctx.request_repaint();
            }
            _ => {}
        }
    }
}

/// Closes `tab` if it is open.
pub(super) fn close_page(dock: &mut DockState<Tab>, tab: Tab) {
    if let Some(path) = dock.find_tab(&tab) {
        dock.remove_tab(path);
    }
}
