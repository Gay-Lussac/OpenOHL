// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Attachment points moved from joint to joint (owner 2026-11-15): the
//! joint chosen in the Points & strings list, the Move attachment point
//! tool (a chooser with several points on the joint, Escape at each
//! step), the question when the point is not on the joint (onto it or
//! its position kept), a new point standing alone, a point and a joint of
//! the same name kept apart, and the placed structure's links the line
//! sends (said, and used for its results).

use eframe::egui;
use ohl_structure_edit::Handle;
use ohl_structure_edit::links::LinkHow;

use crate::app::StructureApp;
use crate::tests::{copy_of, harness};
use crate::tools::Tool;

fn text(h: &egui_kittest::Harness<'_, StructureApp>) -> String {
    h.state().file.as_ref().unwrap().text()
}

fn structure(h: &egui_kittest::Harness<'_, StructureApp>) -> ohl_structures::StructureType {
    h.state().file.as_ref().unwrap().structure().clone()
}

fn position(t: &ohl_structures::StructureType, label: &str) -> [f64; 3] {
    t.points
        .iter()
        .find(|p| p.label == label)
        .unwrap()
        .position()
        .map(|v| v.si())
}

/// The list's joint: point A1 (on joint A1, same name) chosen onto joint
/// A2 — 12 m away: asked, the default onto the joint; applied, A1 is on A2
/// (no offset), its string follows, joint A1 does not move and a lookup
/// of joint A1 finds the joint, not the point. One undo step. Kept
/// position: the offset is the 12 m. The question cancelled (Escape):
/// nothing changes.
#[test]
fn the_list_moves_a_point_to_another_joint() {
    let path = copy_of("type-02.ohls", "list-move");
    let mut h = harness(&path);
    let before = text(&h);
    let t = structure(&h);
    let joint_a1 = t.joint("A1").unwrap();
    assert_eq!(joint_a1.source, ohl_structures::JointSource::Node);
    // Escape on the question: nothing changes.
    h.state_mut().request_link("A1", "A2", "the list");
    h.run();
    let ask = h.state().link_ask.clone().expect("asked");
    assert_eq!(ask.how, LinkHow::Onto);
    assert!((ask.offset[0] - 12.0).abs() < 1e-9, "{:?}", ask.offset);
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.state().link_ask.is_none());
    assert_eq!(text(&h), before);
    // Onto the joint.
    h.state_mut().request_link("A1", "A2", "the list");
    h.state_mut().answer_link(LinkHow::Onto);
    h.run();
    let t = structure(&h);
    assert_eq!(t.point_link("A1"), Some(("A2", [0.0; 3])));
    assert_eq!(position(&t, "A1"), [-6.0, 0.0, 30.0]);
    // The joint of the point's name stays where it was.
    assert_eq!(t.joint("A1").unwrap().position, joint_a1.position);
    // Its string hangs from it there.
    let g = ohl_structure_edit::geometry(&t, &h.state().insulators);
    let s1 = g.strings.iter().find(|s| s.0 == "S1").unwrap();
    assert!((s1.1[0] + 6.0).abs() < 1e-9, "{s1:?}");
    // One undo step.
    h.state_mut().undo();
    h.run();
    assert_eq!(text(&h), before);
    // Its position kept: the offset is the distance.
    h.state_mut().request_link("A1", "A2", "the list");
    h.state_mut().answer_link(LinkHow::KeepPosition);
    h.run();
    let t = structure(&h);
    let (j, o) = t.point_link("A1").unwrap();
    assert_eq!(j, "A2");
    assert!((o[0] - 12.0).abs() < 1e-9, "{o:?}");
    assert_eq!(position(&t, "A1"), [6.0, 0.0, 30.0]);
    // A joint that takes no link: refused, said.
    h.state_mut().request_link("A1", "L10B", "the list");
    let m = h.state().message.clone().unwrap();
    assert!(m.1 && m.0.contains(ohl_structures::NOT_ATTACH), "{m:?}");
}

/// The Move attachment point tool: a joint carrying two points (A1 and a
/// second one moved onto it) opens the chooser; Escape closes it (the
/// tool held), the joint again, A1 chosen, then joint A2: asked, onto it —
/// one undo step. A joint carrying none says so. Escape with a point
/// chosen drops it; again: the tool let go.
#[test]
fn the_move_tool_chooses_among_the_points_on_a_joint() {
    let path = copy_of("type-02.ohls", "move-tool");
    let mut h = harness(&path);
    // A11 onto joint A1 too (12 m: onto it).
    h.state_mut().request_link("A11", "A1", "the list");
    h.state_mut().answer_link(LinkHow::Onto);
    h.run();
    assert_eq!(
        ohl_structure_edit::links::points_on(&structure(&h), "A1"),
        ["A1", "A11"]
    );
    // Joint A2, left with no point, is no attach joint any more (owner
    // 2026-11-16): made one again to take A1.
    assert_eq!(structure(&h).joint("A2").unwrap().kind, None);
    h.state_mut()
        .set_joint_kind("A2", Some(ohl_structures::JointKind::Attach));
    h.run();
    let before = text(&h);
    h.state_mut().hold_tool(Tool::MovePoint);
    h.run();
    assert_eq!(
        h.state().link.as_ref().unwrap().mode,
        crate::link_tool::LinkMode::Move
    );
    // A joint without a point.
    h.state_mut().link_click(Some(Handle::Joint("L10B".into())));
    assert!(h.state().message.as_ref().unwrap().1);
    // Two points on A1: the chooser; Escape closes it.
    h.state_mut().link_click(Some(Handle::Joint("A1".into())));
    h.run();
    let (j, points) = h.state().link.as_ref().unwrap().choose.clone().unwrap();
    assert_eq!((j.as_str(), points.len()), ("A1", 2));
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.state().link.as_ref().unwrap().choose.is_none());
    assert!(h.state().link.is_some(), "the tool still held");
    // Again; A1 chosen (as its button in the chooser does), then joint
    // A2.
    h.state_mut().link_click(Some(Handle::Joint("A1".into())));
    h.run();
    assert!(h.state().link.as_ref().unwrap().choose.is_some());
    h.state_mut().link_click(Some(Handle::Point("A1".into())));
    h.run();
    assert!(h.state().link.as_ref().unwrap().choose.is_none());
    assert_eq!(
        h.state().link.as_ref().unwrap().point.as_deref(),
        Some("A1")
    );
    h.state_mut().link_click(Some(Handle::Joint("A2".into())));
    h.run();
    assert!(h.state().link_ask.is_some());
    h.state_mut().answer_link(LinkHow::Onto);
    h.run();
    assert_eq!(structure(&h).point_link("A1"), Some(("A2", [0.0; 3])));
    assert_eq!(structure(&h).point_link("A11"), Some(("A1", [0.0; 3])));
    h.state_mut().undo();
    h.run();
    assert_eq!(text(&h), before);
    // A point chosen, Escape: dropped; Escape again: the tool let go.
    h.state_mut().link_click(Some(Handle::Point("A1".into())));
    h.key_press(egui::Key::Escape);
    h.run();
    assert_eq!(h.state().link.as_ref().unwrap().point, None);
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.state().link.is_none());
    assert_eq!(h.state().tool(), Tool::Select);
}

/// Geometry › Add a point: the point stands alone — linked to no joint
/// (not the joint at its place, no joint of its own), the file raised to
/// 17; no new joint. Linked from the list, it is no longer alone. A
/// string hanging from an unlinked point on a structure with members is
/// warned about, not on the safe side.
#[test]
fn a_new_point_stands_alone_until_linked() {
    let path = copy_of("type-02.ohls", "point-alone");
    let mut h = harness(&path);
    let joints = structure(&h).joints().len();
    h.state_mut().add_point();
    h.run();
    let t = structure(&h);
    let label = t.points.last().unwrap().label.clone();
    assert!(t.points.last().unwrap().unlinked);
    assert_eq!(t.point_link(&label), None);
    assert_eq!(t.joints().len(), joints, "no joint made");
    let file = text(&h);
    assert!(file.contains("unlinked = true"), "{file}");
    assert!(file.contains("version = 17"), "{file}");
    // A string from it: the warning names it.
    h.state_mut().selected = vec![Handle::Point(label.clone())];
    h.state_mut().add_insulators();
    h.run();
    let w = structure(&h).unlinked_warning().expect("warned");
    assert!(
        w.contains(&label) && w.contains("NOT ON THE SAFE SIDE"),
        "{w}"
    );
    let advice = ohl_structure_edit::missing::missing(&structure(&h), 3);
    assert!(
        advice
            .iter()
            .any(|m| m.reason.contains(&label) && m.reason.contains("linked to no joint")),
        "{advice:?}"
    );
    // Linked: no longer alone.
    h.state_mut().request_link(&label, "A2", "the list");
    if h.state().link_ask.is_some() {
        h.state_mut().answer_link(LinkHow::Onto);
    }
    h.run();
    let t = structure(&h);
    assert_eq!(t.point_link(&label), Some(("A2", [0.0; 3])));
    assert!(!text(&h).contains("unlinked = true"));
    assert!(t.unlinked_warning().is_none());
}

/// The line's links (OHL-Design's relink of A1 of T2 to joint A2, in the
/// line-values file): said when read ("T2: point A1 linked to joint A2
/// (from OHL-Design)"), marked in the list, and written over the type's
/// for the placed structure's solve; a link the type cannot take refuses
/// the run with the reason.
#[test]
fn the_lines_links_are_said_and_used() {
    let path = copy_of("type-02.ohls", "placed-links");
    let mut h = harness(&path);
    let link = ohl_structure_loads::PlacedLink {
        label: "A1".into(),
        joint: Some("A2".into()),
        offset: [0.0, 0.5, 0.0],
        unlinked: false,
    };
    let mut v = ohl_structure_loads::PlacedValues::pending("T2", "line", "type-02", "");
    v.links = vec![link.clone()];
    h.state_mut().placed_links_read(&v);
    let m = h.state().message.clone().unwrap();
    assert_eq!(
        m.0,
        "T2: point A1 linked to joint A2 (offset 0.000, 0.500, 0.000 m) (from OHL-Design)"
    );
    // Read again unchanged: nothing said.
    h.state_mut().message = None;
    h.state_mut().placed_links_read(&v);
    assert!(h.state().message.is_none());
    h.state_mut().loads.placed = Some("T2".into());
    assert!(h.state().placed_link_of("A1").unwrap().contains("joint A2"));
    // Over the type's for the solve.
    let t = structure(&h);
    let (placed, text) =
        crate::link_tool::with_placed_links(&t, &h.state().file.as_ref().unwrap().text(), &[link])
            .unwrap()
            .expect("differs");
    assert_eq!(placed.point_link("A1"), Some(("A2", [0.0, 0.5, 0.0])));
    assert!(text.contains("node = \"A2\""));
    // The same as the type: nothing to change.
    let same = ohl_structure_loads::PlacedLink {
        label: "A11".into(),
        joint: Some("A2".into()),
        offset: [0.0; 3],
        unlinked: false,
    };
    assert!(
        crate::link_tool::with_placed_links(&t, "", &[same])
            .unwrap()
            .is_none()
    );
    // A joint the type cannot take: refused, said.
    let bad = ohl_structure_loads::PlacedLink {
        label: "A1".into(),
        joint: Some("L10B".into()),
        offset: [0.0; 3],
        unlinked: false,
    };
    let e =
        crate::link_tool::with_placed_links(&t, &h.state().file.as_ref().unwrap().text(), &[bad])
            .unwrap_err();
    assert!(e.contains("refused") && e.contains("A1"), "{e}");
}

/// Whether joint `id` has a `kind` written in the file as edited.
fn kind_written(h: &egui_kittest::Harness<'_, StructureApp>, id: &str) -> Option<String> {
    kind_in(&text(h), id)
}

/// Joint `id`'s `kind` written in file text `text`.
fn kind_in(text: &str, id: &str) -> Option<String> {
    let doc: toml_edit::DocumentMut = text.parse().unwrap();
    doc.get("node")?
        .as_array_of_tables()?
        .iter()
        .find(|n| n.get("id").and_then(toml_edit::Item::as_str) == Some(id))?
        .get("kind")?
        .as_str()
        .map(str::to_owned)
}

/// A joint is an attach joint only while attachment points are on it
/// (owner 2026-11-16: "if no attach points are left on the joint then no
/// it shouldn't be attach anymore"), on every path: joints A1 and A2 of
/// type-02 given the kind attach in the Joints table; the list moves
/// point A1 off joint A1 — joint A1 plain (nothing written, no attach
/// joint), A2 still attach; undo brings both back, redo again. The Move
/// attachment point tool takes A11 off A2 to A1: A2 plain. Unlink: the
/// joint left plain. Delete: the joint left plain, undo
/// brings the point and the kind back. The line's links (OHL-Design's
/// move sent in the line-values file): the placed structure's joint left
/// plain too. Point A1 and joint A1 stay apart throughout.
#[test]
fn a_joint_left_by_its_last_point_is_no_attach_joint() {
    use ohl_structures::JointKind;
    let path = copy_of("type-02.ohls", "joint-left");
    let mut h = harness(&path);
    for j in ["A1", "A2"] {
        h.state_mut().set_joint_kind(j, Some(JointKind::Attach));
    }
    h.run();
    assert_eq!(kind_written(&h, "A1").as_deref(), Some("attach"));
    let attach = text(&h);
    let kind = |h: &egui_kittest::Harness<'_, StructureApp>, id: &str| {
        structure(h).joint(id).unwrap().kind
    };
    // The list: A1 onto joint A2 (asked: onto it).
    h.state_mut().request_link("A1", "A2", "the list");
    h.state_mut().answer_link(LinkHow::Onto);
    h.run();
    assert_eq!(structure(&h).point_link("A1"), Some(("A2", [0.0; 3])));
    assert_eq!(kind_written(&h, "A1"), None);
    assert_eq!(kind(&h, "A1"), None, "joint A1 plain");
    assert_eq!(kind(&h, "A2"), Some(JointKind::Attach));
    let moved = text(&h);
    // Undo: the point back on joint A1, joint A1 attach again; redo.
    h.state_mut().undo();
    h.run();
    assert_eq!(text(&h), attach);
    assert_eq!(kind(&h, "A1"), Some(JointKind::Attach));
    assert_eq!(structure(&h).point_link("A1"), Some(("A1", [0.0; 3])));
    h.state_mut().redo();
    h.run();
    assert_eq!(text(&h), moved);
    h.state_mut().undo();
    h.run();
    // The Move attachment point tool: A11 off joint A2 to joint A1.
    h.state_mut().hold_tool(Tool::MovePoint);
    h.run();
    h.state_mut().link_click(Some(Handle::Joint("A2".into())));
    h.run();
    h.state_mut().link_click(Some(Handle::Joint("A1".into())));
    h.run();
    h.state_mut().answer_link(LinkHow::KeepPosition);
    h.run();
    assert_eq!(structure(&h).point_link("A11").map(|l| l.0), Some("A1"));
    assert_eq!(kind_written(&h, "A2"), None);
    assert_eq!(kind(&h, "A2"), None, "joint A2 plain");
    assert_eq!(kind(&h, "A1"), Some(JointKind::Attach));
    h.state_mut().undo();
    h.run();
    assert_eq!(text(&h), attach);
    h.state_mut().hold_tool(Tool::Select);
    h.run();
    // Unlink: joint A1 plain.
    h.state_mut().unlink_point("A1");
    h.run();
    assert_eq!(kind(&h, "A1"), None);
    h.state_mut().undo();
    h.run();
    assert_eq!(text(&h), attach);
    // Delete (what the window removes): joint A2 plain; undo.
    h.state_mut().selected = vec![Handle::Point("A11".into())];
    h.state_mut().ask_delete();
    h.run();
    // What its Delete button does (the window's title is "Delete" too).
    let r = h.state_mut().confirm.take().expect("asked");
    assert_eq!(r.left, ["A2"]);
    h.state_mut()
        .file
        .as_mut()
        .unwrap()
        .edit("delete", |doc, _| ohl_structure_edit::remove(doc, &r));
    h.run();
    // (The file then names a point gone in its capacity, so it is checked
    // as written.)
    assert!(!text(&h).contains("label = \"A11\""));
    assert_eq!(kind_written(&h, "A2"), None);
    assert_eq!(kind_written(&h, "A1").as_deref(), Some("attach"));
    h.state_mut().undo();
    h.run();
    assert_eq!(text(&h), attach);
    assert_eq!(kind(&h, "A2"), Some(JointKind::Attach));
    // The line's links: A1 moved to joint A2 in OHL-Design.
    let link = ohl_structure_loads::PlacedLink {
        label: "A1".into(),
        joint: Some("A2".into()),
        offset: [0.0; 3],
        unlinked: false,
    };
    let (placed, written) = crate::link_tool::with_placed_links(&structure(&h), &attach, &[link])
        .unwrap()
        .expect("differs");
    assert_eq!(placed.point_link("A1"), Some(("A2", [0.0; 3])));
    assert_eq!(placed.joint("A1").unwrap().kind, None, "joint A1 plain");
    assert_eq!(placed.joint("A2").unwrap().kind, Some(JointKind::Attach));
    assert_eq!(kind_in(&written, "A1"), None);
    assert_eq!(kind_in(&written, "A2").as_deref(), Some("attach"));
    // The type itself unchanged.
    assert_eq!(text(&h), attach);
}
