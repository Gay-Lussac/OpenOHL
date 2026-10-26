// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The tools of the 3D view, the hotbar and the inventory, as in
//! OHL-Design (owner, 2026-10-26: "make a toolbar as is the case in the
//! OHL design app"): every tool is in the inventory (E); the hotbar at the
//! bottom of the view holds nine of them (keys 1–9 or a click hold one);
//! the tool held acts on a click in the view, its options in a strip
//! above the view. Escape lets go of a tool (Select is held again).
//!
//! A click in the view only selects with Select (and with every tool
//! that does not act on a click); a joint or point moves only with the
//! Move tool held (owner, 2026-10-26: no drag straight away). The menus
//! keep their entries; the hotbar is the same actions at hand.
//!
//! The arrangement of the slots and the slot held are kept between runs
//! (the window's storage, as OHL-Design's hotbar).

use eframe::egui;
use ohl_structure_edit::Handle;

use crate::app::StructureApp;
use crate::icons::Icon;

/// Key of the hotbar's slots in the window's storage.
pub const HOTBAR_KEY: &str = "ohl-structure-hotbar";

/// Key of the slot held in the window's storage.
pub const ACTIVE_KEY: &str = "ohl-structure-hotbar.active";

/// A tool of the 3D view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    /// A click selects (Shift + click: another).
    Select,
    /// Drag a joint or point to move it.
    Move,
    /// A click adds a joint (above a joint clicked, or where clicked).
    AddJoint,
    /// Two joints clicked: a member between them.
    AddMember,
    /// A click adds a point where strings hang (on a joint clicked, or at
    /// the top).
    AddPoint,
    /// A point clicked: one string below it.
    OneString,
    /// Two points clicked: two strings to one wire point.
    TwoStrings,
    /// A point, then its joint: linked.
    Link,
    /// The symmetry of the body (its options), Make symmetric.
    Symmetry,
    /// A member clicked: a joint inserted on it.
    InsertJoint,
    /// What is clicked is deleted (asked first).
    Delete,
}

impl Tool {
    /// Every tool, in the inventory's order.
    pub const ALL: [Tool; 11] = [
        Tool::Select,
        Tool::Move,
        Tool::Delete,
        Tool::AddPoint,
        Tool::OneString,
        Tool::TwoStrings,
        Tool::Link,
        Tool::AddJoint,
        Tool::AddMember,
        Tool::InsertJoint,
        Tool::Symmetry,
    ];

    /// Its name (the slot's hover text, the strip, the inventory).
    pub fn name(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Move => "Move",
            Tool::AddJoint => "Add joint",
            Tool::AddMember => "Add member",
            Tool::AddPoint => "Add point",
            Tool::OneString => "One string",
            Tool::TwoStrings => "Two strings to one wire point",
            Tool::Link => "Link",
            Tool::Symmetry => "Symmetry",
            Tool::InsertJoint => "Insert a joint on a member",
            Tool::Delete => "Delete",
        }
    }

    /// Its name in the window's storage.
    pub fn key_name(self) -> &'static str {
        match self {
            Tool::Select => "select",
            Tool::Move => "move",
            Tool::AddJoint => "add-joint",
            Tool::AddMember => "add-member",
            Tool::AddPoint => "add-point",
            Tool::OneString => "one-string",
            Tool::TwoStrings => "two-strings",
            Tool::Link => "link",
            Tool::Symmetry => "symmetry",
            Tool::InsertJoint => "insert-joint",
            Tool::Delete => "delete",
        }
    }

    /// What a click (or a drag) in the view does with it.
    pub fn help(self) -> &'static str {
        match self {
            Tool::Select => "Click: select (Shift + click: another); drag: turn the view",
            Tool::Move => "Drag a joint or point: move it (snapped; one undo step); click: select",
            Tool::AddJoint => {
                "Click a joint: a joint above it; click elsewhere: a joint there (in the plane \
                 facing you through the structure's middle, snapped)"
            }
            Tool::AddMember => {
                "Click two joints: a member between them (the next starts at the second)"
            }
            Tool::AddPoint => {
                "Click a joint: a point on it (it moves with it); click elsewhere: a point at the \
                 top of the stick figure"
            }
            Tool::OneString => "Click a point (or a wire point): a string below it",
            Tool::TwoStrings => {
                "Click two points: two strings to one wire point below their middle"
            }
            Tool::Link => {
                "Click an attachment point, then the joint to link it to; right-click: cancel"
            }
            Tool::Symmetry => {
                "The body's symmetry (joints and members take their mirror images along); click: \
                 select"
            }
            Tool::InsertJoint => {
                "Click a member: a joint inserted on it (the member split there, continuous)"
            }
            Tool::Delete => "Click something: delete it with what goes with it (asked first)",
        }
    }

    /// Its icon (of `assets/icons/tools/`).
    pub fn icon(self) -> Icon {
        match self {
            Tool::Select => Icon::Select,
            Tool::Move => Icon::Move,
            Tool::AddJoint => Icon::AddJoint,
            Tool::AddMember => Icon::AddMember,
            Tool::AddPoint => Icon::AddPoint,
            Tool::OneString => Icon::OneString,
            Tool::TwoStrings => Icon::TwoStrings,
            Tool::Link => Icon::Link,
            Tool::Symmetry => Icon::Symmetry,
            Tool::InsertJoint => Icon::InsertJoint,
            Tool::Delete => Icon::Delete,
        }
    }
}

/// Nine slots and the one held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotbar {
    /// The slots (empty: none).
    pub slots: [Option<Tool>; 9],
    /// The slot held.
    pub active: usize,
}

impl Default for Hotbar {
    fn default() -> Self {
        Self {
            slots: Self::DEFAULT.map(Some),
            active: 0,
        }
    }
}

impl Hotbar {
    /// A new user's slots: the nine tools used most (Symmetry and Insert
    /// a joint wait in the inventory).
    pub const DEFAULT: [Tool; 9] = [
        Tool::Select,
        Tool::Move,
        Tool::AddPoint,
        Tool::OneString,
        Tool::TwoStrings,
        Tool::Link,
        Tool::AddJoint,
        Tool::AddMember,
        Tool::Delete,
    ];

    /// The tool held (Select for an empty slot).
    pub fn tool(&self) -> Tool {
        self.slots[self.active].unwrap_or(Tool::Select)
    }

    /// Holds `tool`: its slot if it has one; otherwise it goes into an
    /// empty slot (the one held when empty, else the first empty), else
    /// into the last slot.
    pub fn hold(&mut self, tool: Tool) {
        if let Some(k) = self.slots.iter().position(|s| *s == Some(tool)) {
            self.active = k;
            return;
        }
        let k = if self.slots[self.active].is_none() {
            self.active
        } else {
            self.slots.iter().position(Option::is_none).unwrap_or(8)
        };
        self.slots[k] = Some(tool);
        self.active = k;
    }

    /// Kept in `storage`: a comma list (empty slots blank) and the slot
    /// held.
    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        let slots: Vec<&str> = self
            .slots
            .iter()
            .map(|s| s.map_or("", Tool::key_name))
            .collect();
        storage.set_string(HOTBAR_KEY, slots.join(","));
        storage.set_string(ACTIVE_KEY, self.active.to_string());
    }

    /// As kept by [`Hotbar::save`] (none kept: the default).
    pub fn load(storage: Option<&dyn eframe::Storage>) -> Hotbar {
        let Some(text) = storage.and_then(|s| s.get_string(HOTBAR_KEY)) else {
            return Hotbar::default();
        };
        let mut slots = [None; 9];
        for (k, name) in text.split(',').take(9).enumerate() {
            slots[k] = Tool::ALL.into_iter().find(|t| t.key_name() == name);
        }
        let active = storage
            .and_then(|s| s.get_string(ACTIVE_KEY))
            .and_then(|a| a.parse().ok())
            .filter(|a| *a < 9)
            .unwrap_or(0);
        Hotbar { slots, active }
    }
}

/// A tool dragged from the inventory or a slot.
#[derive(Debug, Clone, Copy)]
struct Dragged {
    tool: Tool,
    from_slot: Option<usize>,
}

/// A slot: its tool's icon on a dark square, its number.
fn slot_frame(
    ui: &mut egui::Ui,
    tool: Option<Tool>,
    held: bool,
    number: Option<usize>,
) -> egui::Response {
    let size = egui::vec2(40.0, 40.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, egui::Color32::from_black_alpha(170));
    p.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(
            if held { 2.5 } else { 1.0 },
            if held {
                egui::Color32::WHITE
            } else {
                egui::Color32::from_gray(110)
            },
        ),
        egui::StrokeKind::Inside,
    );
    if let Some(t) = tool {
        let icon = egui::Rect::from_center_size(rect.center(), egui::vec2(24.0, 24.0));
        t.icon()
            .image(ui.ctx(), 24.0)
            .tint(egui::Color32::WHITE)
            .paint_at(ui, icon);
    }
    if let Some(n) = number {
        p.text(
            rect.left_top() + egui::vec2(3.0, 1.0),
            egui::Align2::LEFT_TOP,
            n.to_string(),
            egui::FontId::proportional(10.0),
            egui::Color32::from_gray(170),
        );
    }
    // Named for screen readers (and tests).
    let label = tool.map_or_else(
        || "slot: empty".to_owned(),
        |t| format!("slot: {}", t.name()),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
    match (tool, number) {
        (Some(t), Some(n)) => response.on_hover_text(format!("{} ({n}) — {}", t.name(), t.help())),
        (Some(t), None) => response.on_hover_text(format!("{} — {}", t.name(), t.help())),
        _ => response,
    }
}

impl StructureApp {
    /// The tool held.
    pub fn tool(&self) -> Tool {
        self.hotbar.tool()
    }

    /// Holds `tool` (its slot, or a slot made for it); the Link tool is
    /// taken up or let go with it.
    pub fn hold_tool(&mut self, tool: Tool) {
        self.hotbar.hold(tool);
        self.tool_changed();
    }

    /// The tool held changed (a slot chosen): the Link tool follows it.
    pub(crate) fn tool_changed(&mut self) {
        let tool = self.tool();
        if self.held == tool {
            return;
        }
        self.held = tool;
        if tool == Tool::Link && self.link.is_none() {
            self.toggle_link_tool();
        } else if tool != Tool::Link && self.link.is_some() {
            self.link = None;
        }
        // What a click does is in the strip above the view: no message.
    }

    /// Escape: the tool let go (Select held again); with the Link tool,
    /// the point chosen is dropped first.
    pub fn let_go_of_tool(&mut self) {
        if self.link.is_some() {
            self.cancel_link();
            if self.link.is_some() {
                return;
            }
        }
        if self.tool() != Tool::Select {
            let was = self.tool();
            self.hotbar.hold(Tool::Select);
            self.held = Tool::Select;
            self.message = Some((format!("{} let go", was.name()), false));
        }
    }

    /// The Link tool taken up or let go elsewhere (the menu, a point's
    /// menu, Escape): the hotbar follows it.
    pub(crate) fn follow_link_tool(&mut self) {
        let linking = self.link.is_some();
        if linking && self.held != Tool::Link {
            self.hotbar.hold(Tool::Link);
            self.held = Tool::Link;
        } else if !linking && self.held == Tool::Link {
            self.hotbar.hold(Tool::Select);
            self.held = Tool::Select;
        }
    }

    /// The hotbar over the bottom of the 3D view (`rect`): a click on a
    /// slot holds its tool; with the inventory open, slots are arranged
    /// by dragging (right-click empties one).
    pub(crate) fn hotbar_ui(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        let width = 9.0 * 43.0 + 30.0;
        let bar = egui::Rect::from_center_size(
            egui::pos2(rect.center().x, rect.bottom() - 30.0),
            egui::vec2(width, 44.0),
        );
        let arranging = self.inventory;
        let mut chosen = None;
        let mut open_inventory = false;
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(bar)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                for k in 0..9 {
                    let tool = self.hotbar.slots[k];
                    let held = k == self.hotbar.active;
                    if !arranging {
                        if slot_frame(ui, tool, held, Some(k + 1)).clicked() {
                            chosen = Some(k);
                        }
                        continue;
                    }
                    let (_, dropped) = ui.dnd_drop_zone::<Dragged, _>(egui::Frame::NONE, |ui| {
                        let r = ui.dnd_drag_source(
                            egui::Id::new(("hotbar-slot", k)),
                            Dragged {
                                tool: tool.unwrap_or(Tool::Select),
                                from_slot: Some(k),
                            },
                            |ui| slot_frame(ui, tool, held, Some(k + 1)),
                        );
                        if r.response.clicked() {
                            chosen = Some(k);
                        }
                        if r.response.secondary_clicked() {
                            self.hotbar.slots[k] = None;
                        }
                    });
                    if let Some(d) = dropped {
                        if let Some(from) = d.from_slot {
                            self.hotbar.slots.swap(from, k);
                        } else {
                            self.hotbar.slots[k] = Some(d.tool);
                        }
                        chosen = Some(k);
                    }
                }
                open_inventory = ui
                    .small_button("E")
                    .on_hover_text("The inventory: every tool, to arrange the hotbar (key E)")
                    .clicked();
            },
        );
        if open_inventory {
            self.inventory = !self.inventory;
        }
        if let Some(k) = chosen {
            self.hotbar.active = k;
            self.tool_changed();
        }
    }

    /// The inventory (E): every tool; drag one onto a slot of the hotbar,
    /// or a slot here to empty it.
    pub(crate) fn inventory_window(&mut self, ctx: &egui::Context) {
        if !self.inventory {
            return;
        }
        let mut open = true;
        let screen = ctx.content_rect();
        let mut emptied = None;
        egui::Window::new("Inventory — tools")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos(screen.center_bottom() + egui::vec2(-220.0, -320.0))
            .show(ctx, |ui| {
                ui.label("Drag a tool onto a hotbar slot; drag a slot here to empty it.");
                let (_, dropped) =
                    ui.dnd_drop_zone::<Dragged, _>(egui::Frame::group(ui.style()), |ui| {
                        egui::Grid::new("inventory").show(ui, |ui| {
                            for (k, t) in Tool::ALL.into_iter().enumerate() {
                                ui.vertical_centered(|ui| {
                                    ui.set_width(76.0);
                                    ui.dnd_drag_source(
                                        egui::Id::new(("inventory", t.key_name())),
                                        Dragged {
                                            tool: t,
                                            from_slot: None,
                                        },
                                        |ui| slot_frame(ui, Some(t), false, None),
                                    );
                                    ui.label(egui::RichText::new(t.name()).small());
                                });
                                if k % 6 == 5 {
                                    ui.end_row();
                                }
                            }
                        });
                    });
                if let Some(d) = dropped {
                    emptied = d.from_slot;
                }
            });
        if let Some(k) = emptied {
            self.hotbar.slots[k] = None;
            self.tool_changed();
        }
        self.inventory = open;
    }

    /// The strip above the 3D view: the tool held, what a click does,
    /// its options (the grid, a string's length, the symmetry, where a
    /// joint is inserted).
    pub(crate) fn tool_strip(&mut self, ui: &mut egui::Ui) {
        let tool = self.tool();
        ui.horizontal_wrapped(|ui| {
            ui.add(tool.icon().image(ui.ctx(), 16.0).tint(ui.visuals().text_color()));
            ui.strong(tool.name()).on_hover_text(tool.help());
            match tool {
                Tool::Move | Tool::AddJoint => {
                    ui.label("snap");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut self.grid)
                            .range(0.0..=5.0)
                            .speed(0.01)
                            .max_decimals(3)
                            .suffix(" m"),
                    )
                    .on_hover_text("The grid a dragged or added joint or point snaps to (0: none), or onto a joint");
                }
                Tool::OneString | Tool::TwoStrings => {
                    ui.label("string length");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut self.string_length)
                            .range(0.05..=50.0)
                            .speed(0.05)
                            .max_decimals(3)
                            .suffix(" m"),
                    )
                    .on_hover_text("The length of a new string");
                }
                Tool::InsertJoint => {
                    ui.label("at");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut self.insert_at)
                            .range(0.001..=0.999)
                            .speed(0.01)
                            .max_decimals(3),
                    )
                    .on_hover_text("Fraction of the member's length from its first joint");
                }
                Tool::Symmetry => {
                    let now = self
                        .file
                        .as_ref()
                        .and_then(|f| ohl_structure_edit::body::symmetry_option(f.structure()));
                    let mut chosen = now;
                    ui.radio_value(&mut chosen, None, "none");
                    for m in ohl_structure_edit::Mirror::ALL {
                        ui.radio_value(&mut chosen, Some(m), m.name());
                    }
                    if chosen != now {
                        self.set_symmetry(chosen);
                    }
                    if ui
                        .add_enabled(now.is_some(), egui::Button::new("Make symmetric"))
                        .on_hover_text("Every joint and member gets the mirror images it lacks")
                        .on_disabled_hover_text("Choose a symmetry first")
                        .clicked()
                    {
                        self.make_symmetric();
                    }
                }
                _ => {}
            }
            ui.label(egui::RichText::new(tool.help()).weak().small());
        });
    }

    /// A click in the view with an acting tool held, on `h` (none:
    /// nothing there; `at`: where the click meets the plane facing the
    /// camera through the structure's middle).
    pub fn tool_click(&mut self, h: Option<Handle>, at: Option<[f64; 3]>) {
        match (self.tool(), h) {
            (Tool::AddJoint, Some(Handle::Joint(id))) => {
                self.selected = vec![Handle::Joint(id)];
                self.add_joint();
            }
            (Tool::AddJoint, None) => {
                if let Some(p) = at {
                    let g = self.grid;
                    let p = if g > 0.0 {
                        p.map(|x| (x / g).round() * g)
                    } else {
                        p
                    };
                    self.add_joint_at(p);
                }
            }
            (Tool::AddMember, Some(Handle::Joint(id))) => {
                let joints = self.picked_joints();
                if joints.len() == 1 && joints[0] != id {
                    self.selected.push(Handle::Joint(id));
                    self.add_member();
                } else {
                    self.selected = vec![Handle::Joint(id.clone())];
                    self.message = Some((
                        format!("Add member from joint {id}: click the second joint"),
                        false,
                    ));
                }
            }
            (Tool::AddPoint, Some(Handle::Joint(id))) => {
                self.selected = vec![Handle::Joint(id)];
                self.point_here();
            }
            (Tool::AddPoint, None) => self.add_point(),
            (Tool::OneString, Some(h @ (Handle::Point(_) | Handle::Wire(_)))) => {
                self.selected = vec![h];
                self.add_insulators();
            }
            (Tool::TwoStrings, Some(h @ Handle::Point(_))) => {
                let tops = self.picked_tops();
                let first = match &h {
                    Handle::Point(l) => l.clone(),
                    _ => String::new(),
                };
                if tops.len() == 1 && tops[0] != first {
                    self.selected.push(h);
                    self.add_insulators();
                } else {
                    self.selected = vec![h];
                    self.message = Some((
                        format!("Two strings from point {first}: click the second point"),
                        false,
                    ));
                }
            }
            (Tool::InsertJoint, Some(Handle::Member(m))) => self.insert_joint(&m),
            (Tool::Delete, Some(h)) => {
                self.selected = vec![h];
                self.ask_delete();
            }
            (tool, None) => {
                self.selected.clear();
                if !matches!(tool, Tool::AddMember | Tool::TwoStrings) {
                    self.message = Some((format!("{}: {}", tool.name(), tool.help()), false));
                }
            }
            (tool, Some(h)) => {
                self.message = Some((
                    format!("{}: not on {} — {}", tool.name(), h.describe(), tool.help()),
                    false,
                ));
            }
        }
    }

    /// A joint at `at` (one edit, its mirror images along).
    pub fn add_joint_at(&mut self, at: [f64; 3]) {
        let Some(f) = &mut self.file else {
            return;
        };
        let ids = f.edit("add joint", |doc, t| {
            ohl_structure_edit::symmetry::add_joint(doc, t, at)
        });
        self.selected = vec![Handle::Joint(ids[0].clone())];
        self.message = Some((
            format!(
                "added joint {} at {:.3}, {:.3}, {:.3} m",
                ids.join(", "),
                at[0],
                at[1],
                at[2]
            ),
            false,
        ));
    }

    /// A joint inserted on member `m` at the fraction chosen (one edit).
    pub fn insert_joint(&mut self, m: &str) {
        let Some(f) = &mut self.file else {
            return;
        };
        let at = self.insert_at;
        let what = format!("insert a joint on member {m}");
        self.message = Some(
            match f.edit(&what, |doc, t| {
                ohl_structure_edit::links::insert_joint(doc, t, m, at)
            }) {
                Ok(id) => {
                    self.selected = vec![Handle::Joint(id.clone())];
                    (
                        format!(
                            "{what}: joint {id} (attach) at {at:.3} of its length; the member is \
                             split there, continuous — link a point to it with the Link tool"
                        ),
                        false,
                    )
                }
                Err(e) => (e, true),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Holding a tool not in the hotbar puts it in an empty slot, else in
    /// the last; a tool in a slot is held there.
    #[test]
    fn holding_a_tool_finds_it_a_slot() {
        let mut h = Hotbar::default();
        assert_eq!(h.tool(), Tool::Select);
        h.hold(Tool::Move);
        assert_eq!(h.active, 1);
        h.hold(Tool::Symmetry);
        assert_eq!(h.active, 8);
        assert_eq!(h.slots[8], Some(Tool::Symmetry));
        h.slots[3] = None;
        h.active = 3;
        h.hold(Tool::InsertJoint);
        assert_eq!(h.slots[3], Some(Tool::InsertJoint));
    }

    /// Every tool has a different stored name and an icon.
    #[test]
    fn tools_have_names_and_icons() {
        let mut names: Vec<&str> = Tool::ALL.iter().map(|t| t.key_name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Tool::ALL.len());
        for t in Tool::ALL {
            assert!(!t.icon().svg().is_empty());
        }
    }
}
