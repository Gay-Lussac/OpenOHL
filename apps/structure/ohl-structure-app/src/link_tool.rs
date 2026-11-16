// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Attachment points linked to joints (2026-10-21; owner 2026-11-15): the
//! **Link** tool, the **Move attachment point** tool, the joint chosen in
//! the Points & strings list, and the question asked when a point is
//! linked to a joint it is not on.
//!
//! - **Link** (Edit › Tools › Link): click an attachment point (where a
//!   string hangs), then a joint it may be linked to.
//! - **Move attachment point**: click a joint carrying attachment points
//!   (or a point itself); with several on the joint, a small window lists
//!   them (label, the sets and strings hanging there) and one is chosen;
//!   then click the joint to move it to.
//! - **The list** (Geometry › Points & strings): a point's joint chosen
//!   in its row (a list of the joints with a search, or its id typed).
//!
//! All three end the same way ([`StructureApp::request_link`]): on the
//! joint (within [`ohl_structures::ON_JOINT`], 1 mm), linked onto it;
//! farther, a window asks (owner 2026-11-15) **Move the point onto the
//! joint** (the default: the point and the strings hanging from it move
//! there, no offset) or **Keep its position** (an offset from the joint:
//! a rigid link, whose moment the structure analysis applies at the
//! joint). The answer is part of the one edit (one undo step). Only
//! attach joints of the structure (`[[node]]`s) take a link — an
//! attachment point and a joint are separate objects, even of the same
//! name (owner 2026-11-15); another point's own place is no joint.
//! Escape (or a right-click in the view) cancels at any step: the window,
//! then the point chosen, then the tool.
//!
//! A point is unlinked from its context menu (in the view or the Points
//! table): it keeps its place, standing alone, linked to no joint — its
//! strings are not attached to the 3D structure until it is linked again
//! (said, not on the safe side). A point cannot be put on a member:
//! **Insert a joint** on the member first, then link the point.

use eframe::egui;
use ohl_structure_edit::Handle;
use ohl_structure_edit::links::LinkHow;
use ohl_structures::StructureType;

use crate::app::StructureApp;
use crate::tools::Tool;

/// Which of the two tools is held.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LinkMode {
    /// Link: a point, then a joint.
    #[default]
    Link,
    /// Move attachment point: a joint carrying points (one chosen), then
    /// a joint.
    Move,
}

impl LinkMode {
    /// The mode of tool `t` (Link for any other).
    pub fn of(t: Tool) -> Self {
        if t == Tool::MovePoint {
            LinkMode::Move
        } else {
            LinkMode::Link
        }
    }

    /// Its tool.
    pub fn tool(self) -> Tool {
        match self {
            LinkMode::Link => Tool::Link,
            LinkMode::Move => Tool::MovePoint,
        }
    }

    /// Its name in messages.
    pub fn name(self) -> &'static str {
        self.tool().name()
    }
}

/// The Link or Move attachment point tool, held.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LinkTool {
    /// The attachment point chosen (first click), waiting for its joint.
    pub point: Option<String>,
    /// Which tool.
    pub mode: LinkMode,
    /// Move attachment point on a joint carrying several points: the
    /// joint and its points, one to choose (a small window).
    pub choose: Option<(String, Vec<String>)>,
}

/// A link waiting for its answer: the point is not on the joint (owner
/// 2026-11-15: "ask if we want to move it on the joint or create an
/// offset").
#[derive(Debug, Clone, PartialEq)]
pub struct LinkAsk {
    /// The attachment point.
    pub label: String,
    /// The joint.
    pub joint: String,
    /// From the joint to the point as it is, local axes, m.
    pub offset: [f64; 3],
    /// The answer chosen (default: onto the joint).
    pub how: LinkHow,
    /// What asked (Link, Move attachment point, the list).
    pub from: String,
}

/// What the 3D view asks of the application about links.
#[derive(Debug, Clone, PartialEq)]
pub enum LinkAction {
    /// Unlink point `label` (its context menu).
    Unlink(String),
    /// Start the Link tool on point `label` (its context menu).
    Start(String),
}

/// The joints of the structure (its `[[node]]`s, not a point's own place)
/// but support joints, with their positions (local axes, m) and whether
/// each is an attach joint (only those take a link; the others say why
/// not under the pointer).
pub fn linkable(t: &StructureType) -> Vec<(String, [f64; 3], bool)> {
    t.linkable_joints()
        .into_iter()
        .filter(|j| j.source == ohl_structures::JointSource::Node)
        .map(|j| {
            let attach = j.kind == Some(ohl_structures::JointKind::Attach);
            (j.id, j.position, attach)
        })
        .collect()
}

/// The attach joints a point may be linked to (ids, in order): the
/// list's choices.
pub fn attach_joints(t: &StructureType) -> Vec<String> {
    linkable(t)
        .into_iter()
        .filter(|j| j.2)
        .map(|j| j.0)
        .collect()
}

/// What hangs from attachment point `label`: its strings and the sets of
/// their wire points, in words (`string S1, S2 (set 3)`; none: `nothing
/// hangs from it`).
pub fn hanging(t: &StructureType, label: &str) -> String {
    let strings: Vec<&ohl_structures::TypeString> =
        t.strings.iter().filter(|s| s.top == label).collect();
    if strings.is_empty() {
        return "nothing hangs from it".into();
    }
    let mut sets: Vec<u32> = strings
        .iter()
        .filter_map(|s| t.wires.iter().find(|w| w.label == s.wire)?.set)
        .collect();
    sets.sort_unstable();
    sets.dedup();
    let names: Vec<&str> = strings.iter().map(|s| s.label.as_str()).collect();
    match sets.as_slice() {
        [] => format!("string {}", names.join(", ")),
        _ => format!(
            "string {} (set {})",
            names.join(", "),
            sets.iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// The offset's length and its moment arm for a vertical load (its
/// length across the vertical), m, in words.
pub fn offset_words(o: [f64; 3]) -> String {
    let length = (o[0] * o[0] + o[1] * o[1] + o[2] * o[2]).sqrt();
    if length < ohl_structures::SAME_JOINT {
        return "on it (no offset)".into();
    }
    format!(
        "offset ({:.3}, {:.3}, {:.3}) m, {:.3} m long; moment arm {:.3} m for a vertical load",
        o[0],
        o[1],
        o[2],
        length,
        o[0].hypot(o[1])
    )
}

impl StructureApp {
    /// The Link tool: held (or let go).
    pub fn toggle_link_tool(&mut self) {
        if self.link.take().is_some() {
            self.link_ask = None;
            self.message = Some(("Link tool let go".into(), false));
        } else {
            self.take_link_tool(LinkMode::Link);
        }
    }

    /// Takes up the Link or Move attachment point tool.
    pub fn take_link_tool(&mut self, mode: LinkMode) {
        self.link = Some(LinkTool {
            mode,
            ..LinkTool::default()
        });
        self.link_ask = None;
        self.message = Some((
            match mode {
                LinkMode::Link => {
                    "Link tool: click an attachment point, then the joint to link it \
                                   to (right-click or Escape: cancel)"
                        .into()
                }
                LinkMode::Move => "Move attachment point: click a joint carrying an attachment \
                                   point (or the point), then the joint to move it to (Escape: \
                                   cancel)"
                    .into(),
            },
            false,
        ));
    }

    /// Right-click or Escape with the Link or Move attachment point tool:
    /// the question, else the chooser, else the point chosen is dropped;
    /// with none of them, the tool is let go.
    pub fn cancel_link(&mut self) {
        if self.link_ask.take().is_some() {
            self.message = Some(("link cancelled: nothing changed".into(), false));
            return;
        }
        let Some(l) = &mut self.link else {
            return;
        };
        let name = l.mode.name();
        if l.choose.take().is_some() || l.point.take().is_some() {
            self.message = Some((format!("{name}: cancelled"), false));
        } else {
            self.link = None;
            self.message = Some((format!("{name} let go"), false));
        }
    }

    /// A click with the Link or Move attachment point tool on `h` (a
    /// point picked, a joint carrying points, or a joint to link to).
    pub fn link_click(&mut self, h: Option<Handle>) {
        let Some(tool) = &mut self.link else {
            return;
        };
        let mode = tool.mode;
        let name = mode.name();
        match (tool.point.clone(), h) {
            (_, Some(Handle::Point(l))) => {
                tool.point = Some(l.clone());
                tool.choose = None;
                self.selected = vec![Handle::Point(l.clone())];
                self.message = Some((
                    format!(
                        "{name} {l}: click the joint to {} it to (ringed; Escape: cancel)",
                        if mode == LinkMode::Move {
                            "move"
                        } else {
                            "link"
                        }
                    ),
                    false,
                ));
            }
            (Some(l), Some(Handle::Joint(j))) => {
                let from = name.to_owned();
                self.request_link(&l, &j, &from);
            }
            // Move attachment point: the joint carrying the point(s).
            (None, Some(Handle::Joint(j))) if mode == LinkMode::Move => self.move_from_joint(&j),
            (Some(_), _) => {
                self.message = Some((
                    format!(
                        "{name}: click a ringed joint (an attach joint; a support joint takes no \
                         strings)"
                    ),
                    true,
                ));
            }
            (None, _) => {
                self.message = Some((
                    match mode {
                        LinkMode::Link => {
                            "Link: click an attachment point first (where a string hangs)".into()
                        }
                        LinkMode::Move => "Move attachment point: click a joint carrying an \
                                           attachment point (or the point) first"
                            .into(),
                    },
                    true,
                ));
            }
        }
    }

    /// Move attachment point on joint `joint`: its one point chosen; with
    /// several, the chooser opens; with none, said.
    pub fn move_from_joint(&mut self, joint: &str) {
        let Some(f) = &self.file else {
            return;
        };
        let on = ohl_structure_edit::links::points_on(f.structure(), joint);
        let Some(tool) = &mut self.link else {
            return;
        };
        match on.as_slice() {
            [] => {
                self.message = Some((
                    format!(
                        "Move attachment point: joint {joint} carries no attachment point — click \
                         a joint with one (a point standing alone: click the point)"
                    ),
                    true,
                ));
            }
            [one] => {
                let one = one.clone();
                tool.choose = None;
                self.link_click(Some(Handle::Point(one)));
            }
            _ => {
                tool.choose = Some((joint.to_owned(), on.clone()));
                self.selected = vec![Handle::Joint(joint.to_owned())];
                self.message = Some((
                    format!(
                        "Move attachment point: joint {joint} carries {} points ({}) — choose one",
                        on.len(),
                        on.join(", ")
                    ),
                    false,
                ));
            }
        }
    }

    /// Point `label` to be linked to joint `joint` (from the tool or list
    /// `from`): refused when the joint takes no link; on the joint,
    /// linked at once; farther, asked first ([`LinkAsk`]).
    pub fn request_link(&mut self, label: &str, joint: &str, from: &str) {
        let Some(f) = &self.file else {
            return;
        };
        let t = f.structure();
        if !t.points.iter().any(|p| p.label == label) {
            self.message = Some((format!("no attachment point {label}"), true));
            return;
        }
        if let Some(why) = ohl_structure_edit::links::not_attach(t, joint) {
            self.message = Some((format!("{from}: {why}"), true));
            return;
        }
        if t.point_link(label)
            .is_some_and(|(j, o)| j == joint && o == [0.0; 3])
        {
            self.message = Some((format!("point {label} is on joint {joint} already"), false));
            self.end_link_tool_step(label);
            return;
        }
        match ohl_structure_edit::links::needs_asking(t, label, joint) {
            Some(offset) => {
                self.link_ask = Some(LinkAsk {
                    label: label.to_owned(),
                    joint: joint.to_owned(),
                    offset,
                    how: LinkHow::Onto,
                    from: from.to_owned(),
                });
                self.message = Some((
                    format!(
                        "point {label} is not on joint {joint} ({:.3} m from it): move it onto \
                         the joint, or keep its position at an offset?",
                        offset.iter().map(|v| v * v).sum::<f64>().sqrt()
                    ),
                    false,
                ));
            }
            None => self.apply_link(label, joint, LinkHow::Onto),
        }
    }

    /// Links point `label` to joint `joint` as `how` says (one edit, one
    /// undo step); the strings hanging from it follow.
    pub fn apply_link(&mut self, label: &str, joint: &str, how: LinkHow) {
        let Some(f) = &mut self.file else {
            return;
        };
        let before = f.structure().point_link(label).map(|(j, _)| j.to_owned());
        let what = match &before {
            Some(j) if j != joint && !j.is_empty() => {
                format!("move point {label} from joint {j} to joint {joint}")
            }
            _ => format!("link point {label} to joint {joint}"),
        };
        let r = f.edit(&what, |doc, t| {
            ohl_structure_edit::links::relink_point(doc, t, label, joint, how)
        });
        self.message = Some(match r {
            Ok(()) => {
                let o = f.structure().point_link(label).map_or([0.0; 3], |(_, o)| o);
                (
                    format!(
                        "{what}: {}{}",
                        offset_words(o),
                        match how {
                            LinkHow::Onto => " (moved onto the joint, its strings with it)",
                            LinkHow::KeepPosition => " (its position kept)",
                        }
                    ),
                    false,
                )
            }
            Err(e) => (e, true),
        });
        self.link_ask = None;
        self.end_link_tool_step(label);
    }

    /// The tool's step done: the point chosen dropped, the point picked.
    fn end_link_tool_step(&mut self, label: &str) {
        if let Some(l) = &mut self.link {
            l.point = None;
            l.choose = None;
        }
        self.selected = vec![Handle::Point(label.to_owned())];
    }

    /// The question's answer `how` taken (its window's Link button).
    pub fn answer_link(&mut self, how: LinkHow) {
        if let Some(a) = self.link_ask.clone() {
            self.apply_link(&a.label, &a.joint, how);
        }
    }

    /// Unlinks point `label`: it keeps its place, standing alone (one
    /// edit).
    pub fn unlink_point(&mut self, label: &str) {
        let Some(f) = &mut self.file else {
            return;
        };
        let what = format!("unlink point {label}");
        self.message = Some(
            match f.edit(&what, |doc, t| {
                ohl_structure_edit::links::unlink_point(doc, t, label)
            }) {
                Ok(()) => (
                    format!(
                        "{what}: it keeps its place, linked to no joint — the strings hanging \
                         from it are not attached to the 3D structure until it is linked again"
                    ),
                    false,
                ),
                Err(e) => (e, true),
            },
        );
    }

    /// What the view asked about links.
    pub fn link_action(&mut self, a: LinkAction) {
        match a {
            LinkAction::Unlink(label) => self.unlink_point(&label),
            LinkAction::Start(label) => {
                self.take_link_tool(LinkMode::Link);
                self.follow_link_tool();
                self.link_click(Some(Handle::Point(label)));
            }
        }
    }

    /// The windows of the Link and Move attachment point tools: the
    /// chooser (several points on the joint clicked) and the question (a
    /// point not on the joint it is linked to). Escape closes either
    /// ([`StructureApp::cancel_link`]).
    pub(crate) fn link_windows(&mut self, ctx: &egui::Context) {
        self.chooser_window(ctx);
        self.link_ask_window(ctx);
    }

    fn chooser_window(&mut self, ctx: &egui::Context) {
        let Some((joint, points)) = self.link.as_ref().and_then(|l| l.choose.clone()) else {
            return;
        };
        let Some(f) = &self.file else {
            return;
        };
        let t = f.structure_arc();
        let mut chosen = None;
        let mut cancel = false;
        let mut open = true;
        egui::Window::new(format!("Move attachment point — joint {joint}"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .default_pos(ctx.content_rect().center() - egui::vec2(180.0, 80.0))
            .show(ctx, |ui| {
                crate::help::tool_intro(
                    ui,
                    &format!(
                        "Joint {joint} carries {} attachment points: choose the one to move.",
                        points.len()
                    ),
                    "Each row is an attachment point linked to this joint, with the strings \
                     hanging from it and the sets of their wire points. The point chosen is \
                     moved to the joint you click next; the strings hanging from it follow. \
                     Escape: nothing changes.",
                    &Tool::MovePoint.guide(),
                );
                egui::Grid::new("move-point-chooser")
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("point");
                        ui.strong("what hangs there");
                        ui.end_row();
                        for l in &points {
                            if ui
                                .button(l.as_str())
                                .on_hover_text(format!("Move {l}: then click its new joint"))
                                .clicked()
                            {
                                chosen = Some(l.clone());
                            }
                            ui.label(hanging(&t, l));
                            ui.end_row();
                        }
                    });
                ui.separator();
                if ui
                    .button("Cancel")
                    .on_hover_text("Escape: nothing changes")
                    .clicked()
                {
                    cancel = true;
                }
            });
        if let Some(l) = chosen {
            if let Some(tool) = &mut self.link {
                tool.choose = None;
            }
            self.link_click(Some(Handle::Point(l)));
        } else if cancel || !open {
            self.cancel_link();
        }
    }

    fn link_ask_window(&mut self, ctx: &egui::Context) {
        let Some(mut ask) = self.link_ask.clone() else {
            return;
        };
        let o = ask.offset;
        let mut pressed = None;
        let mut open = true;
        egui::Window::new(format!("Link point {} to joint {}", ask.label, ask.joint))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .default_pos(ctx.content_rect().center() - egui::vec2(200.0, 80.0))
            .show(ctx, |ui| {
                crate::help::tool_intro(
                    ui,
                    &ohl_ui::link_question::asked(&ask.label, &ask.joint, o),
                    "Onto the joint: the point moves there, and the strings hanging from it \
                     with it (no offset). Keep its position: the point stays where it is, its \
                     distance from the joint kept as its offset — a rigid link whose moment \
                     the structure analysis applies at the joint (where a frame member meets \
                     it; else left out and said). Within 1 mm the point goes onto the joint \
                     without this question. One undo step either way.",
                    &match ask.from.as_str() {
                        "Move attachment point" => Tool::MovePoint.guide(),
                        _ => Tool::Link.guide(),
                    },
                );
                ohl_ui::link_question::choices(
                    ui,
                    &mut ask.how,
                    LinkHow::Onto,
                    LinkHow::KeepPosition,
                    &ask.joint,
                    o,
                    &offset_words(o),
                );
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    pressed = Some(ohl_ui::tool_window::Pressed::Action);
                }
                if let Some(p) = ohl_ui::tool_window::buttons(
                    ui,
                    "Link",
                    true,
                    None,
                    ohl_ui::tool_window::Keys::Both,
                ) {
                    pressed = Some(p);
                }
            });
        match pressed {
            Some(ohl_ui::tool_window::Pressed::Action) => self.answer_link(ask.how),
            Some(ohl_ui::tool_window::Pressed::Cancel) => self.cancel_link(),
            None if !open => self.cancel_link(),
            None => self.link_ask = Some(ask),
        }
    }

    /// Make the members meeting joint `joint` frames (the offset moment of
    /// a point there is then carried): one edit; their sections need a
    /// bending stiffness (iy, iz, torsion).
    pub fn make_frame_joint(&mut self, joint: &str) {
        let Some(f) = &mut self.file else {
            return;
        };
        let members: Vec<String> = f
            .structure()
            .members
            .iter()
            .filter(|m| m.nodes.iter().any(|n| n == joint))
            .map(|m| m.id.clone())
            .collect();
        if members.is_empty() {
            self.message = Some((format!("no member meets joint {joint}"), true));
            return;
        }
        let what = format!("frame joint {joint}");
        f.edit(&what, |doc, _| {
            for m in &members {
                ohl_structure_edit::set_member_kind(doc, m, "frame");
            }
        });
        self.message = Some((
            format!(
                "{what}: members {} made frames (their sections need iy, iz and torsion); run \
                 the results again",
                members.join(", ")
            ),
            false,
        ));
    }
}

/// Structure `t` (its file's text `text`) as placed structure links its
/// points: the line's links (`links`, sent by OHL-Design in the
/// line-values file) written over the type's, for the placed structure's
/// solve — the line's link wins over the type's, as in the line's
/// structure check. None: no link differs (solved as written). Err: a
/// link that cannot be applied (the point or joint gone from the type,
/// a joint that takes no link): the run is refused rather than leaving
/// the point's loads on the type's joint (owner 2026-11-15: a moved point
/// never leaves loads silently on its old joint).
pub fn with_placed_links(
    t: &StructureType,
    text: &str,
    links: &[ohl_structure_loads::PlacedLink],
) -> Result<Option<(StructureType, String)>, String> {
    let differs = |l: &ohl_structure_loads::PlacedLink| {
        let now = t.points.iter().find(|p| p.label == l.label);
        match (now, &l.joint, l.unlinked) {
            (None, _, _) => true,
            (Some(p), _, true) => !p.unlinked,
            (Some(_), Some(j), false) => t
                .point_link(&l.label)
                .is_none_or(|(tj, o)| tj != j || (0..3).any(|k| (o[k] - l.offset[k]).abs() > 1e-9)),
            (Some(_), None, false) => false,
        }
    };
    let wanted: Vec<&ohl_structure_loads::PlacedLink> =
        links.iter().filter(|l| differs(l)).collect();
    if wanted.is_empty() {
        return Ok(None);
    }
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| e.to_string())?;
    for l in wanted {
        let refused = |why: String| {
            format!(
                "the line links {} differently from this structure ({why}): the run is refused \
                 until the type and the line agree (relink it in OHL-Design, or here)",
                l.describe()
            )
        };
        if !t.points.iter().any(|p| p.label == l.label) {
            return Err(refused(format!("no attachment point {} here", l.label)));
        }
        let r = match (&l.joint, l.unlinked) {
            (Some(j), false) => {
                ohl_structure_edit::links::link_point(&mut doc, t, &l.label, j, l.offset)
            }
            _ => ohl_structure_edit::links::unlink_point(&mut doc, t, &l.label),
        };
        r.map_err(refused)?;
    }
    let text = doc.to_string();
    let placed = <StructureType as ohl_structures::LibraryItem>::parse(
        std::path::Path::new("placed.ohls"),
        &text,
    )
    .map_err(|e| e.to_string())?;
    Ok(Some((placed, text)))
}

impl StructureApp {
    /// The line-values file read (again): the placed structure's links
    /// that changed since it was last read, said in the message line
    /// (`T7: point A1 linked to joint 42Y (from OHL-Design)`); the first
    /// reading says the links the line has, if any.
    pub fn placed_links_read(&mut self, v: &ohl_structure_loads::PlacedValues) {
        let changed =
            ohl_structure_loads::links_changed(&v.structure, &self.placed_links, &v.links);
        self.placed_links.clone_from(&v.links);
        if !changed.is_empty() {
            self.message = Some((format!("{} (from OHL-Design)", changed.join("; ")), false));
        }
    }

    /// The placed structure's link of point `label` where it differs from
    /// the type's, in words (none: as the type).
    pub fn placed_link_of(&self, label: &str) -> Option<String> {
        let l = self.placed_links.iter().find(|l| l.label == label)?;
        let t = self.file.as_ref()?.structure();
        let same = match (&l.joint, l.unlinked) {
            (_, true) => t.points.iter().any(|p| p.label == label && p.unlinked),
            (Some(j), false) => t.point_link(label).is_some_and(|(tj, o)| {
                tj == j && (0..3).all(|k| (o[k] - l.offset[k]).abs() <= 1e-9)
            }),
            (None, false) => true,
        };
        (!same).then(|| {
            let whose = self.loads.placed.clone().unwrap_or_default();
            match (&l.joint, l.unlinked) {
                (Some(j), false) => format!("{whose} in the line: joint {j}"),
                _ => format!("{whose} in the line: unlinked"),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_words_give_length_and_arm() {
        assert_eq!(offset_words([0.0; 3]), "on it (no offset)");
        let w = offset_words([0.3, 0.4, 1.0]);
        assert!(w.contains("0.500 m for a vertical load"), "{w}");
        assert!(w.contains("1.118 m long"), "{w}");
    }
}
