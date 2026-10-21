// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Attachment points and joints in OHL-Structure (2026-10-21): the Link
//! tool's two clicks, its cancel and undo; a joint's kind set in the
//! Joints table and undone; the rigid link drawn and picked. With
//! `OHL_RENDER_PNG` set, images of the window.

use eframe::egui;
use egui_kittest::kittest::Queryable;
use ohl_structure_edit::Handle;
use ohl_structures::JointKind;

use crate::app::StructureApp;
use crate::tests::{copy_of, harness, menu};

/// Saves the window as `name` in `OHL_RENDER_PNG` (a GPU rendering).
fn shot(h: &mut egui_kittest::Harness<'_, StructureApp>, name: &str) {
    if ohl_render::headless_device().is_ok()
        && let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG")
    {
        let image = h.render().unwrap();
        image.save(std::path::Path::new(&dir).join(name)).unwrap();
    }
}

fn text(h: &egui_kittest::Harness<'_, StructureApp>) -> String {
    h.state().file.as_ref().unwrap().text()
}

/// The Link tool from the Geometry menu: a click on point A1, Escape
/// cancels it (the tool stays held), A1 again, then joint L10B — no kind:
/// refused, not an attach joint (owner decision 2026-10-21) — made an
/// attach joint, then L10B again: A1 is linked there where it is (its
/// distance kept as its offset) in one edit, its rigid link drawn and
/// picked; Undo takes it back. Escape with no point chosen lets go of the
/// tool; a support joint is refused.
#[test]
fn the_link_tool_links_cancels_and_undoes() {
    let path = copy_of("type-02.ohls", "link-tool");
    let mut h = harness(&path);
    let before = text(&h);
    menu(&mut h, "Geometry", "Link tool");
    assert!(h.state().link.is_some());
    h.state_mut().link_click(Some(Handle::Point("A1".into())));
    h.run();
    assert_eq!(
        h.state().link.as_ref().unwrap().point.as_deref(),
        Some("A1")
    );
    // Escape: the point dropped, the tool still held.
    h.key_press(egui::Key::Escape);
    h.run();
    assert_eq!(h.state().link.as_ref().unwrap().point, None);
    // A support joint takes no strings.
    h.state_mut().link_click(Some(Handle::Point("A1".into())));
    h.state_mut().link_click(Some(Handle::Joint("L0A".into())));
    h.run();
    assert_eq!(text(&h), before);
    assert!(h.state().message.as_ref().unwrap().1, "refused");
    // A joint without a kind: not an attach joint, refused.
    h.state_mut().link_click(Some(Handle::Point("A1".into())));
    h.state_mut().link_click(Some(Handle::Joint("L10B".into())));
    h.run();
    assert_eq!(text(&h), before);
    let m = h.state().message.clone().unwrap();
    assert!(m.1 && m.0.contains(ohl_structures::NOT_ATTACH), "{m:?}");
    h.state_mut()
        .file
        .as_mut()
        .unwrap()
        .edit("kind", |doc, t| {
            ohl_structure_edit::links::set_joint_kind(
                doc,
                t,
                "L10B",
                Some(ohl_structures::JointKind::Attach),
            )
        })
        .unwrap();
    let attach = text(&h);
    // Point, then joint: linked where it is.
    h.state_mut().link_click(Some(Handle::Point("A1".into())));
    h.state_mut().link_click(Some(Handle::Joint("L10B".into())));
    h.run();
    let after = text(&h);
    assert!(after.contains("node = \"L10B\""), "{after}");
    assert!(after.contains("offset = ["), "{after}");
    let t = h.state().file.as_ref().unwrap().structure().clone();
    let (joint, offset) = t.point_link("A1").unwrap();
    assert_eq!(joint, "L10B");
    let p = t
        .points
        .iter()
        .find(|p| p.label == "A1")
        .unwrap()
        .position();
    // Where it was: (6, 0, 30) m.
    for (k, v) in [6.0, 0.0, 30.0].into_iter().enumerate() {
        assert!((p[k].si() - v).abs() < 1e-9, "{p:?}");
    }
    assert!(offset != [0.0; 3]);
    let m = &h.state().message.as_ref().unwrap().0;
    assert!(m.contains("moment arm"), "{m}");
    // The rigid link: drawn, and picked as its point.
    let g = ohl_structure_edit::geometry(&t, &h.state().insulators);
    let link = g.links.iter().find(|l| l.label == "A1").expect("drawn");
    assert_eq!(link.joint, "L10B");
    let project = |p: [f64; 3]| Some([p[0] * 10.0, -p[2] * 10.0]);
    let mid = [0, 1, 2].map(|k| 0.5 * (link.from[k] + link.to[k]));
    let at = project(mid).unwrap();
    let mut only_links = g.clone();
    only_links.joints.clear();
    only_links.members.clear();
    only_links.points.clear();
    only_links.wires.clear();
    assert_eq!(
        ohl_structure_edit::pick(&only_links, &project, at, 3.0),
        Some(Handle::Point("A1".into()))
    );
    h.state_mut().selected = vec![Handle::Point("A1".into())];
    h.run();
    shot(&mut h, "structure-rigid-link-selected.png");
    // One undo step.
    h.state_mut().undo();
    h.run();
    assert_eq!(text(&h), attach);
    // Right-click (or Escape) twice: the tool let go.
    h.state_mut().link_click(Some(Handle::Point("A11".into())));
    h.state_mut().cancel_link();
    h.state_mut().cancel_link();
    assert!(h.state().link.is_none());
}

/// The Joints table has a kind column; a joint made an attach joint is
/// one undo step (the file raised to version 6), undone; a joint carrying
/// a point cannot be made a support joint; the Point on the joint offers
/// only joints strings may hang from.
#[test]
fn joint_kinds_are_set_and_undone() {
    let path = copy_of("type-02.ohls", "joint-kinds");
    let mut h = harness(&path);
    let before = text(&h);
    menu(&mut h, "Body", "Joints…");
    h.get_by_label("kind");
    h.state_mut()
        .set_joint_kind("L10B", Some(JointKind::Attach));
    h.run();
    let after = text(&h);
    assert!(after.contains("kind = \"attach\""), "{after}");
    assert!(after.contains("version = 6"), "{after}");
    let t = h.state().file.as_ref().unwrap().structure().clone();
    assert_eq!(t.joint("L10B").unwrap().kind, Some(JointKind::Attach));
    h.state_mut().selected = vec![Handle::Joint("L10B".into())];
    h.state_mut().scroll_to = Some(Handle::Joint("L10B".into()));
    h.run();
    h.run();
    shot(&mut h, "structure-joints-kinds.png");
    h.state_mut().undo();
    h.run();
    assert_eq!(text(&h), before);
    // A1 carries point A1.
    h.state_mut().set_joint_kind("A1", Some(JointKind::Support));
    h.run();
    assert_eq!(text(&h), before);
    assert!(h.state().message.as_ref().unwrap().1);
    // Point here: not on a support joint.
    h.state_mut().selected = vec![Handle::Joint("L0A".into())];
    assert!(!h.state().picked_attachable_joint());
    h.state_mut().selected = vec![Handle::Joint("L10B".into())];
    assert!(h.state().picked_attachable_joint());
}

/// With the Link tool and a point chosen, the joints it may be linked to
/// are ringed and the one under the pointer shows the offset and the
/// moment arm (an image of it with `OHL_RENDER_PNG`).
#[test]
fn the_link_tool_shows_the_joint_under_the_pointer() {
    let path = copy_of("type-02.ohls", "link-hover");
    let mut h = harness(&path);
    menu(&mut h, "Geometry", "Link tool");
    h.state_mut().link_click(Some(Handle::Point("A1".into())));
    h.run();
    h.run();
    let t = h.state().file.as_ref().unwrap().structure().clone();
    let j = t.joint("L10B").unwrap().position;
    if let Some(at) = h.state().view_screen(j) {
        h.input_mut().events.push(egui::Event::PointerMoved(at));
        h.run();
        h.run();
        assert_eq!(h.state().view_link_hover().as_deref(), Some("L10B"));
    }
    shot(&mut h, "structure-link-tool-hover.png");
}

/// Make it a frame joint (offered beside an offset moment left out at a
/// truss-only joint): every member meeting the joint made a frame, one
/// undo step.
#[test]
fn make_it_a_frame_joint_is_one_undo_step() {
    let path = copy_of("type-02.ohls", "frame-joint");
    let mut h = harness(&path);
    let before = text(&h);
    h.state_mut().make_frame_joint("A1");
    h.run();
    let t = h.state().file.as_ref().unwrap().structure().clone();
    let meeting: Vec<_> = t
        .members
        .iter()
        .filter(|m| m.nodes.iter().any(|n| n == "A1"))
        .collect();
    assert!(!meeting.is_empty());
    assert!(
        meeting.iter().all(|m| m.kind.as_deref() == Some("frame")),
        "{meeting:?}"
    );
    h.state_mut().undo();
    assert_eq!(text(&h), before);
}
