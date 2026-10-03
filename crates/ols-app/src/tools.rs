// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Tools, the hotbar and the inventory (as in Minecraft): every tool is in
//! the inventory; the hotbar holds nine of them; choosing a slot only holds
//! its tool — a right-click on an object uses it (a left click always
//! selects). Tools are data, so new ones are added to
//! [`Tool::ALL`] and appear in the inventory.

use eframe::egui;

/// A tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    /// Select objects and see their properties.
    Select,
    /// Click a structure: a window to move it.
    Move,
    /// Click a structure: delete it (with confirmation).
    Delete,
    /// Open the "Go to structure" window.
    GoTo,
}

impl Tool {
    /// Every tool (the inventory).
    pub const ALL: [Tool; 4] = [Tool::Select, Tool::Move, Tool::Delete, Tool::GoTo];

    /// Name.
    pub fn name(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Move => "Move structure",
            Tool::Delete => "Delete structure",
            Tool::GoTo => "Go to structure",
        }
    }

    /// Icon in a slot.
    pub fn icon(self) -> &'static str {
        match self {
            Tool::Select => "⬉",
            Tool::Move => "✋",
            Tool::Delete => "🗑",
            Tool::GoTo => "⤴",
        }
    }

    /// Its shortcut key, if any.
    pub fn key(self) -> Option<&'static str> {
        match self {
            Tool::Select => None,
            Tool::Move => Some("M"),
            Tool::Delete => None,
            Tool::GoTo => Some("G"),
        }
    }

    /// What a click does with it.
    pub fn help(self) -> &'static str {
        match self {
            Tool::Select => "right-click an object to select it",
            Tool::Move => "right-click a structure to move it",
            Tool::Delete => "right-click a structure to delete it",
            Tool::GoTo => "right-click to open the Go to window",
        }
    }

    /// Whether it changes the model (needs editing rights).
    pub fn edits(self) -> bool {
        matches!(self, Tool::Move | Tool::Delete)
    }

    fn key_name(self) -> &'static str {
        match self {
            Tool::Select => "select",
            Tool::Move => "move",
            Tool::Delete => "delete",
            Tool::GoTo => "goto",
        }
    }
}

/// Nine slots and the active one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotbar {
    /// Slots (empty: `None`).
    pub slots: [Option<Tool>; 9],
    /// Active slot.
    pub active: usize,
}

impl Default for Hotbar {
    fn default() -> Self {
        let mut slots = [None; 9];
        for (k, t) in Tool::ALL.into_iter().enumerate() {
            slots[k] = Some(t);
        }
        Self { slots, active: 0 }
    }
}

impl Hotbar {
    /// The active tool (Select for an empty slot).
    pub fn tool(&self) -> Tool {
        self.slots[self.active].unwrap_or(Tool::Select)
    }

    /// Makes `tool` active: its slot if it has one; otherwise it goes into
    /// the active slot.
    pub fn hold(&mut self, tool: Tool) {
        match self.slots.iter().position(|s| *s == Some(tool)) {
            Some(k) => self.active = k,
            None => self.slots[self.active] = Some(tool),
        }
    }

    /// Moves the active slot by `steps` (wrapping).
    pub fn scroll(&mut self, steps: i32) {
        self.active = (self.active as i32 + steps).rem_euclid(9) as usize;
    }

    /// Saved as a comma list (empty slots blank) and the active slot.
    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        let slots: Vec<&str> = self
            .slots
            .iter()
            .map(|s| s.map_or("", Tool::key_name))
            .collect();
        storage.set_string("hotbar", slots.join(","));
        storage.set_string("hotbar.active", self.active.to_string());
    }

    /// As saved by [`Hotbar::save`], if saved.
    pub fn load(storage: &dyn eframe::Storage) -> Option<Hotbar> {
        let text = storage.get_string("hotbar")?;
        let mut slots = [None; 9];
        for (k, name) in text.split(',').take(9).enumerate() {
            slots[k] = Tool::ALL.into_iter().find(|t| t.key_name() == name);
        }
        let active = storage
            .get_string("hotbar.active")
            .and_then(|a| a.parse().ok())
            .filter(|a| *a < 9)
            .unwrap_or(0);
        Some(Hotbar { slots, active })
    }
}

/// A tool dragged from the inventory or a slot.
#[derive(Debug, Clone, Copy)]
struct Dragged {
    tool: Tool,
    from_slot: Option<usize>,
}

fn slot_frame(
    ui: &mut egui::Ui,
    tool: Option<Tool>,
    active: bool,
    number: Option<usize>,
) -> egui::Response {
    let size = egui::vec2(46.0, 46.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, egui::Color32::from_black_alpha(170));
    p.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(
            if active { 3.0 } else { 1.0 },
            if active {
                egui::Color32::WHITE
            } else {
                egui::Color32::from_gray(110)
            },
        ),
        egui::StrokeKind::Inside,
    );
    if let Some(t) = tool {
        p.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            t.icon(),
            egui::FontId::proportional(22.0),
            egui::Color32::WHITE,
        );
    }
    if let Some(n) = number {
        p.text(
            rect.left_top() + egui::vec2(4.0, 2.0),
            egui::Align2::LEFT_TOP,
            n.to_string(),
            egui::FontId::proportional(10.0),
            egui::Color32::from_gray(170),
        );
    }
    match tool {
        Some(t) => response.on_hover_text(match t.key() {
            Some(k) => format!("{} ({k}) — {}", t.name(), t.help()),
            None => format!("{} — {}", t.name(), t.help()),
        }),
        None => response,
    }
}

/// The hotbar at the bottom of the window: click a slot to hold its tool,
/// drop a tool on a slot (from the inventory or another slot); right-click
/// empties a slot.
pub fn hotbar_ui(ctx: &egui::Context, hotbar: &mut Hotbar, at: egui::Pos2, arranging: bool) {
    egui::Area::new(egui::Id::new("hotbar"))
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(at - egui::vec2(0.0, 8.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                for k in 0..9 {
                    let tool = hotbar.slots[k];
                    if !arranging {
                        // Slots stay put unless the inventory is open.
                        if slot_frame(ui, tool, k == hotbar.active, Some(k + 1)).clicked() {
                            hotbar.active = k;
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
                            |ui| slot_frame(ui, tool, k == hotbar.active, Some(k + 1)),
                        );
                        if r.response.clicked() {
                            hotbar.active = k;
                        }
                        if r.response.secondary_clicked() {
                            hotbar.slots[k] = None;
                        }
                    });
                    if let Some(d) = dropped {
                        if let Some(from) = d.from_slot {
                            hotbar.slots.swap(from, k);
                        } else {
                            hotbar.slots[k] = Some(d.tool);
                        }
                        hotbar.active = k;
                    }
                }
            });
            ui.label(
                egui::RichText::new(format!(
                    "{} — {} · 1–9 or wheel (captured) · E: inventory (arrange)",
                    hotbar.tool().name(),
                    hotbar.tool().help()
                ))
                .small()
                .color(egui::Color32::from_gray(200)),
            );
        });
}

/// Hotbar steps from wheel events: one per mouse-wheel notch; trackpad
/// scrolling (points) one per 30 points, at most one every 0.12 s; never
/// more than one per frame — slow turns move, fast ones do not skip.
#[derive(Debug, Clone, Default)]
pub struct WheelSteps {
    points: f32,
    last: f64,
}

impl WheelSteps {
    /// The step (+1 next slot, −1 previous) for this frame's `lines`
    /// (notch directions summed) and `points` at time `now`, s.
    pub fn step(&mut self, lines: f32, points: f32, now: f64) -> Option<i32> {
        // Wheel down (negative y) goes to the next slot.
        if lines != 0.0 {
            self.points = 0.0;
            self.last = now;
            return Some(if lines < 0.0 { 1 } else { -1 });
        }
        if points == 0.0 {
            return None;
        }
        self.points += points;
        if self.points.abs() >= 30.0 && now - self.last >= 0.12 {
            let step = if self.points < 0.0 { 1 } else { -1 };
            self.points = 0.0;
            self.last = now;
            return Some(step);
        }
        None
    }
}

/// The inventory (E): every tool; drag one onto a slot of the hotbar, or
/// drag a slot here to empty it.
pub fn inventory_ui(ctx: &egui::Context, open: &mut bool, hotbar: &mut Hotbar) {
    let mut keep = *open;
    let screen = ctx.content_rect();
    egui::Window::new("Inventory — tools")
        .open(&mut keep)
        .collapsible(false)
        .resizable(false)
        .constrain(false)
        .default_pos(screen.center_bottom() + egui::vec2(-200.0, -300.0))
        .show(ctx, |ui| {
            ui.label("Drag a tool onto a hotbar slot; drag a slot here to empty it.");
            let (_, dropped) =
                ui.dnd_drop_zone::<Dragged, _>(egui::Frame::group(ui.style()), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for t in Tool::ALL {
                            ui.vertical(|ui| {
                                ui.dnd_drag_source(
                                    egui::Id::new(("inventory", t.name())),
                                    Dragged {
                                        tool: t,
                                        from_slot: None,
                                    },
                                    |ui| slot_frame(ui, Some(t), false, None),
                                );
                                ui.label(egui::RichText::new(t.name()).small());
                            });
                        }
                    });
                });
            if let Some(d) = dropped
                && let Some(from) = d.from_slot
            {
                hotbar.slots[from] = None;
            }
        });
    *open = keep && *open;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_steps_are_one_at_a_time() {
        let mut w = WheelSteps::default();
        // A slow notch moves one slot; several notches in a frame, one.
        assert_eq!(w.step(-1.0, 0.0, 0.0), Some(1));
        assert_eq!(w.step(-3.0, 0.0, 0.01), Some(1));
        assert_eq!(w.step(1.0, 0.0, 0.5), Some(-1));
        assert_eq!(w.step(0.0, 0.0, 0.6), None);
        // Trackpad: small movements add up; then at most one per 0.12 s.
        assert_eq!(w.step(0.0, -10.0, 1.0), None);
        assert_eq!(w.step(0.0, -25.0, 1.02), Some(1));
        assert_eq!(w.step(0.0, -80.0, 1.05), None, "too soon");
        assert_eq!(w.step(0.0, -5.0, 1.2), Some(1));
    }

    #[test]
    fn hold_scroll_and_empty_slots() {
        let mut h = Hotbar::default();
        assert_eq!(h.tool(), Tool::Select);
        h.hold(Tool::Move);
        assert_eq!((h.active, h.tool()), (1, Tool::Move));
        h.scroll(-2);
        assert_eq!(h.active, 8);
        // An empty slot holds Select; holding a tool not in the bar puts
        // it in the active slot.
        assert_eq!(h.tool(), Tool::Select);
        h.slots[1] = None;
        h.hold(Tool::Move);
        assert_eq!((h.active, h.slots[8]), (8, Some(Tool::Move)));
        h.scroll(1);
        assert_eq!(h.active, 0);
    }
}
