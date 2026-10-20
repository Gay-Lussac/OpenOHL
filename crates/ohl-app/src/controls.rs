// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Navigation controls of the 3D view as data: which keys move the camera
//! and what each mouse drag does. Schemes are presets ([`Controls::presets`]);
//! schemes matching other software can be added the same way, and later
//! edited and saved.

use eframe::egui;

/// A camera movement held down with a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    /// Forward along the ground.
    Forward,
    /// Backward along the ground.
    Back,
    /// To the left.
    Left,
    /// To the right.
    Right,
    /// Straight up.
    Up,
    /// Straight down.
    Down,
    /// Faster while held.
    Sprint,
}

impl Move {
    /// Name in the controls list.
    pub fn name(self) -> &'static str {
        match self {
            Move::Forward => "forward",
            Move::Back => "back",
            Move::Left => "left",
            Move::Right => "right",
            Move::Up => "up",
            Move::Down => "down",
            Move::Sprint => "sprint",
        }
    }
}

/// A key or modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// A key.
    Key(egui::Key),
    /// Shift.
    Shift,
    /// Ctrl (⌃).
    Ctrl,
}

impl Input {
    /// Whether it is held down.
    pub fn down(self, i: &egui::InputState) -> bool {
        match self {
            Input::Key(k) => i.key_down(k),
            Input::Shift => i.modifiers.shift,
            Input::Ctrl => i.modifiers.ctrl,
        }
    }

    /// Name in the controls list.
    pub fn name(self) -> String {
        match self {
            Input::Key(k) => k.name().to_owned(),
            Input::Shift => "Shift".into(),
            Input::Ctrl => "Ctrl".into(),
        }
    }
}

/// What a mouse drag does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drag {
    /// Turn around the point looked at.
    Orbit,
    /// Turn the view around the eye (first person).
    Look,
    /// Move in the screen plane.
    Pan,
    /// Nothing.
    None,
}

impl Drag {
    fn name(self) -> &'static str {
        match self {
            Drag::Orbit => "turn around the target",
            Drag::Look => "look around",
            Drag::Pan => "pan",
            Drag::None => "—",
        }
    }
}

/// A navigation scheme.
#[derive(Debug, Clone, PartialEq)]
pub struct Controls {
    /// Name of the scheme.
    pub name: &'static str,
    /// Keys held to move.
    pub keys: Vec<(Move, Input)>,
    /// Left drag.
    pub left: Drag,
    /// Right drag.
    pub right: Drag,
    /// Middle drag.
    pub middle: Drag,
    /// Moving speed, m/s.
    pub speed: f64,
    /// Speed factor while sprinting.
    pub sprint: f64,
    /// Clicking the 3D view captures the mouse (first person, crosshair,
    /// Escape frees it).
    pub capture: bool,
    /// Mouse movement while captured, radians per pixel.
    pub mouse_sensitivity: f64,
    /// Tools act on a left-click (a right-click selects); off (the
    /// default): a left-click selects, a right-click uses the tool held.
    /// For every view (3D, plan, profile); drags are not affected.
    pub tools_on_left: bool,
}

/// The mouse buttons whose clicks (select, use the tool held) in a view:
/// a left-click selects and a right-click uses the tool, or the other way
/// round with `tools_on_left` — except when Select is held (`select_held`):
/// a left-click then selects as always. Drags are not affected (a click
/// is a press and release without moving).
pub fn click_buttons(
    tools_on_left: bool,
    select_held: bool,
) -> (egui::PointerButton, egui::PointerButton) {
    use egui::PointerButton::{Primary, Secondary};
    if tools_on_left && !select_held {
        (Secondary, Primary)
    } else {
        (Primary, Secondary)
    }
}

/// A help or status text as the clicks are set: written for the default
/// ("right-click" uses the tool), with `tools_on_left` its "right-click"
/// and "left-click" are swapped (texts are written once, shown either way).
pub fn click_words(text: &str, tools_on_left: bool) -> std::borrow::Cow<'_, str> {
    if !tools_on_left || !(text.contains("ight-click") || text.contains("eft-click")) {
        return std::borrow::Cow::Borrowed(text);
    }
    const SWAPS: [(&str, &str); 4] = [
        ("right-click", "left-click"),
        ("Right-click", "Left-click"),
        ("left-click", "right-click"),
        ("Left-click", "Right-click"),
    ];
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    'outer: while !rest.is_empty() {
        for (from, to) in SWAPS {
            if let Some(after) = rest.strip_prefix(from) {
                out.push_str(to);
                rest = after;
                continue 'outer;
            }
        }
        let c = rest.chars().next().expect("not empty");
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    std::borrow::Cow::Owned(out)
}

/// The click rows of the controls list: what a left-click and a
/// right-click do (`tools_on_left`: [`Controls::tools_on_left`]).
pub fn click_rows(tools_on_left: bool) -> Vec<(String, String)> {
    if tools_on_left {
        vec![
            (
                "left-click".into(),
                "use the tool held (hotbar; Select held: select)".into(),
            ),
            ("right-click".into(), "select".into()),
        ]
    } else {
        vec![
            ("left-click".into(), "select".into()),
            ("right-click".into(), "use the tool held (hotbar)".into()),
        ]
    }
}

/// W A S D, Space up, Shift down, Ctrl sprint (Minecraft creative).
fn wasd() -> Vec<(Move, Input)> {
    vec![
        (Move::Forward, Input::Key(egui::Key::W)),
        (Move::Left, Input::Key(egui::Key::A)),
        (Move::Back, Input::Key(egui::Key::S)),
        (Move::Right, Input::Key(egui::Key::D)),
        (Move::Up, Input::Key(egui::Key::Space)),
        (Move::Down, Input::Shift),
        (Move::Sprint, Input::Ctrl),
    ]
}

impl Default for Controls {
    fn default() -> Self {
        Self::presets().remove(0)
    }
}

impl Controls {
    /// The schemes to choose from.
    pub fn presets() -> Vec<Controls> {
        vec![
            Controls {
                name: "Orbit",
                keys: wasd(),
                left: Drag::Orbit,
                right: Drag::Pan,
                middle: Drag::Pan,
                speed: 30.0,
                sprint: 4.0,
                capture: false,
                mouse_sensitivity: 0.003,
                tools_on_left: false,
            },
            Controls {
                name: "Minecraft creative",
                keys: wasd(),
                left: Drag::Look,
                right: Drag::Pan,
                middle: Drag::Pan,
                speed: 30.0,
                sprint: 4.0,
                capture: true,
                mouse_sensitivity: 0.003,
                tools_on_left: false,
            },
        ]
    }

    /// The preset whose name starts with `name` (case-insensitive).
    pub fn by_name(name: &str) -> Option<Controls> {
        let wanted = name.trim().to_lowercase();
        Self::presets()
            .into_iter()
            .find(|c| !wanted.is_empty() && c.name.to_lowercase().starts_with(&wanted))
    }

    /// Saves the scheme, speed, sensitivity and the click option.
    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        storage.set_string("controls", self.name.to_owned());
        storage.set_string("controls.tools-on-left", self.tools_on_left.to_string());
        storage.set_string("controls.speed", self.speed.to_string());
        storage.set_string("controls.sensitivity", self.mouse_sensitivity.to_string());
    }

    /// The click option saved by [`Controls::save`] (off when none).
    pub fn load_tools_on_left(storage: &dyn eframe::Storage) -> bool {
        storage
            .get_string("controls.tools-on-left")
            .is_some_and(|v| v == "true")
    }

    /// The scheme saved by [`Controls::save`], if any (with its click
    /// option).
    pub fn load(storage: &dyn eframe::Storage) -> Option<Controls> {
        let mut c = Self::by_name(&storage.get_string("controls")?)?;
        let num = |key: &str| storage.get_string(key).and_then(|v| v.parse::<f64>().ok());
        if let Some(v) = num("controls.speed").filter(|v| v.is_finite() && *v > 0.0) {
            c.speed = v;
        }
        if let Some(v) = num("controls.sensitivity").filter(|v| v.is_finite() && *v > 0.0) {
            c.mouse_sensitivity = v;
        }
        c.tools_on_left = Self::load_tools_on_left(storage);
        Some(c)
    }

    /// The drag action for a pointer button.
    pub fn drag(&self, button: egui::PointerButton) -> Drag {
        match button {
            egui::PointerButton::Primary => self.left,
            egui::PointerButton::Secondary => self.right,
            egui::PointerButton::Middle => self.middle,
            _ => Drag::None,
        }
    }

    /// The movement held down this frame: (forward, right, up) in units of
    /// the speed, and whether sprinting.
    pub fn held(&self, i: &egui::InputState) -> ([f64; 3], bool) {
        let mut v = [0.0; 3];
        let mut sprint = false;
        for (m, input) in &self.keys {
            if !input.down(i) {
                continue;
            }
            match m {
                Move::Forward => v[0] += 1.0,
                Move::Back => v[0] -= 1.0,
                Move::Right => v[1] += 1.0,
                Move::Left => v[1] -= 1.0,
                Move::Up => v[2] += 1.0,
                Move::Down => v[2] -= 1.0,
                Move::Sprint => sprint = true,
            }
        }
        (v, sprint)
    }

    /// The scheme as text lines, for the controls list.
    pub fn describe(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self
            .keys
            .iter()
            .map(|(m, i)| (i.name(), m.name().to_owned()))
            .collect();
        if self.capture {
            let (select, tool) = if self.tools_on_left {
                ("right-click", "left-click")
            } else {
                ("left-click", "right-click")
            };
            out.push(("left-click 3D view".into(), "capture the mouse".into()));
            out.push(("mouse (captured)".into(), "look around".into()));
            out.push((
                format!("{select} (captured)"),
                "select at the crosshair".into(),
            ));
            out.push((
                format!("{tool} (captured)"),
                if self.tools_on_left {
                    "use the tool held at the crosshair (Select held: select)".into()
                } else {
                    "use the tool held at the crosshair".into()
                },
            ));
            out.push(("Ctrl + wheel (captured)".into(), "speed".into()));
            out.push(("Esc".into(), "free the mouse".into()));
        }
        if !self.capture {
            out.extend(click_rows(self.tools_on_left));
        }
        out.push(("left drag".into(), self.left.name().into()));
        out.push(("right drag".into(), self.right.name().into()));
        out.push(("middle drag".into(), self.middle.name().into()));
        // Box and lasso selection of LiDAR points (the selecting button,
        // [`crate::area`]).
        let select = if self.tools_on_left { "right" } else { "left" };
        out.push((
            format!("Alt + {select} drag"),
            "box: the LiDAR points drawn in it, their class changed together (Edit window)".into(),
        ));
        out.push((
            format!("Alt + Shift + {select} drag"),
            "lasso: the same in a free shape (Esc: give up)".into(),
        ));
        out.push(("scroll".into(), "zoom".into()));
        out.push((
            if self.tools_on_left {
                "right double-click".into()
            } else {
                "double-click".into()
            },
            "fly to the object".into(),
        ));
        // The drag handles of the structure being edited (its structure
        // screen open: [`crate::handles`]).
        out.push((
            "drag a handle".into(),
            "move a point of the structure being edited".into(),
        ));
        out.push((
            "X / Y / Z (dragging)".into(),
            "hold it on that axis of the structure (again: free)".into(),
        ));
        out.push((
            "Shift + X / Y / Z (dragging)".into(),
            "hold it in the plane across that axis".into(),
        ));
        out.push(("Alt (dragging)".into(), "no snapping".into()));
        out.push(("Esc (dragging)".into(), "cancel the drag".into()));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_add_up() {
        let c = Controls::default();
        let mut i = egui::InputState::default();
        i.keys_down.insert(egui::Key::W);
        i.keys_down.insert(egui::Key::D);
        i.modifiers.shift = true;
        assert_eq!(c.held(&i), ([1.0, 1.0, -1.0], false));
        i.keys_down.insert(egui::Key::S);
        i.modifiers.ctrl = true;
        assert_eq!(c.held(&i), ([0.0, 1.0, -1.0], true));
        assert_eq!(c.drag(egui::PointerButton::Primary), Drag::Orbit);
        let mc = &Controls::presets()[1];
        assert_eq!(mc.drag(egui::PointerButton::Primary), Drag::Look);
        assert!(mc.describe().iter().any(|(k, v)| k == "Space" && v == "up"));
        // Texts written for right-click read the other way with the option.
        let t = "String wire: right-click the next string (Right-click: …; left-click selects)";
        assert_eq!(click_words(t, false), t);
        assert_eq!(
            click_words(t, true),
            "String wire: left-click the next string (Left-click: …; right-click selects)"
        );
    }
}
