// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Attachment points linked to joints in OHL-Design (2026-10-21): the
//! Link tool's two clicks, its cancel and undo; a drag changing the
//! offset and relinking on a joint; the rigid link drawn and picked;
//! Replace structure's Attachment points (exact, offered, unmatched); a
//! new string's point offered only attach joints. With `OHL_RENDER_PNG`
//! set (and a GPU), images of the window.

use std::path::PathBuf;

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use ohl_line::links::PointChoice;
use ohl_structure_edit::Handle;
use ohl_structures::structures::to_local;

use crate::app::App;
use crate::selection::Selection;
use crate::tests::{press, typed_project};
use crate::tools::Tool;

fn gpu() -> bool {
    let gpu = ohl_render::headless_device().is_ok();
    assert!(
        gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    gpu
}

fn harness(model: crate::model::Model, gpu: bool) -> Harness<'static, App> {
    let mut b = Harness::builder().with_size(egui::vec2(1500.0, 950.0));
    if gpu {
        b = b.wgpu();
    }
    let mut h = b.build_eframe(move |cc| App::with_model(model, cc.wgpu_render_state.clone()));
    h.run();
    h
}

/// Saves the window as `name`.png in `OHL_RENDER_PNG`.
fn shot(h: &mut Harness<'_, App>, name: &str) {
    if let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG")
        && let Ok(image) = h.render()
    {
        let path = PathBuf::from(dir).join(format!("{name}.png"));
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
}

/// The tool click with `tool` held on `target`.
fn use_tool(h: &mut Harness<'_, App>, tool: Tool, target: Selection) {
    let r = h.state_mut().ready().unwrap();
    r.hotbar.hold(tool);
    r.use_tool(crate::selection::Use {
        target,
        at: None,
        on: None,
    });
    h.run();
}

/// [`typed_project`] with joint L10B of type-02 an attach joint (only
/// attach joints take a link: owner decision 2026-10-21).
fn l10b_attach(name: &str) -> crate::model::Model {
    crate::tests::typed_project_with(name, |dir| {
        let p = dir.join("type-02.ohls");
        let text = std::fs::read_to_string(&p).unwrap().replacen(
            "id = \"L10B\"\n",
            "id = \"L10B\"\nkind = \"attach\"\n",
            1,
        );
        std::fs::write(&p, text).unwrap();
    })
}

fn point(structure: &str, label: &str) -> Selection {
    Selection::Point {
        structure: structure.into(),
        label: label.into(),
    }
}

/// Where `p` is on the screen (the 3D view as last drawn).
fn on_screen(h: &mut Harness<'_, App>, p: [f64; 3]) -> egui::Pos2 {
    let r = h.state_mut().ready().unwrap();
    let rect = r.view3d.rect.unwrap();
    let cam = r.view3d.camera.unwrap();
    let size = [f64::from(rect.width()), f64::from(rect.height())];
    let (q, _) = cam.project(p, size).unwrap();
    rect.min + egui::vec2(q[0] as f32, q[1] as f32)
}

/// The 3D view looking at `p` from close by.
fn look_at(h: &mut Harness<'_, App>, p: [f64; 3], distance: f64) {
    let r = h.state_mut().ready().unwrap();
    let cam = r.view3d.camera.as_mut().unwrap();
    cam.target = p;
    cam.distance = distance;
    h.run();
}

fn click(h: &mut Harness<'_, App>, at: egui::Pos2, button: egui::PointerButton) {
    h.input_mut().events.push(egui::Event::PointerMoved(at));
    h.step();
    for pressed in [true, false] {
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
    }
    h.run();
}

/// Only attach joints take a link (owner decision 2026-10-21): on type-02
/// as shipped, joint L10B (no kind) is offered to the pointer but not
/// clickable, its hover says why, the command is refused.
#[test]
fn only_attach_joints_take_a_link() {
    let m = typed_project("link-attach-only");
    let mut o = crate::link_tool::overlay(&m, "T2", "A1").unwrap();
    let j = o.joints.iter().find(|j| j.id == "L10B").unwrap();
    assert!(!j.attach);
    assert!(o.joints.iter().any(|j| j.attach && j.id == "A2"));
    o.hovered = Some("L10B".into());
    let text = o.hover_text().unwrap();
    assert!(text.contains(ohl_structures::NOT_ATTACH), "{text}");
    assert_eq!(o.click(), None);
    assert_eq!(o.clicked, None);
    let e = crate::link_tool::link_command(&m, "T2", "A1", "L10B").unwrap_err();
    assert!(e.contains(ohl_structures::NOT_ATTACH), "{e}");
    o.hovered = Some("A2".into());
    assert_eq!(o.click().as_deref(), Some("A2"));
    assert!(
        o.hover_text()
            .unwrap()
            .contains("link A1 of T2 to joint A2")
    );
}

/// The Link tool: the tool click on attachment point A1 of T2 chooses it
/// (a wire point is refused, Escape and a click on nothing cancel); the
/// second, on joint L10B of its type (made an attach joint), links it
/// there where it is — one command, the point not moved, its distance the
/// offset; one undo takes it back. With a GPU, through the 3D view: the
/// joint under the pointer is named with the offset and moment arm, a
/// right-click on it links.
#[test]
fn the_link_tool_links_with_two_clicks_cancels_and_undoes() {
    let gpu = gpu();
    let mut h = harness(l10b_attach("link-tool"), gpu);
    let before = h
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .structure("T2")
        .unwrap()
        .point("A1")
        .unwrap()
        .clone();
    // A wire point is not linked.
    use_tool(&mut h, Tool::Link, point("T2", "W1"));
    assert!(h.state_mut().ready().unwrap().link_from.is_none());
    // Chosen, then Escape.
    use_tool(&mut h, Tool::Link, point("T2", "A1"));
    assert_eq!(
        h.state_mut().ready().unwrap().link_from,
        Some(("T2".into(), "A1".into()))
    );
    press(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    {
        let r = h.state_mut().ready().unwrap();
        assert!(r.link_from.is_none());
        assert!(r.status.as_ref().unwrap().0.contains("cancelled"));
    }
    // Chosen, then a click on nothing.
    use_tool(&mut h, Tool::Link, point("T2", "A1"));
    use_tool(&mut h, Tool::Link, Selection::None);
    assert!(h.state_mut().ready().unwrap().link_from.is_none());
    // Chosen, then joint L10B: linked where it is.
    use_tool(&mut h, Tool::Link, point("T2", "A1"));
    let joint = {
        let r = h.state_mut().ready().unwrap();
        let o = crate::link_tool::overlay(&r.model, "T2", "A1").unwrap();
        // Every joint but the supports: the four feet are not offered.
        assert!(
            o.joints.iter().all(|j| !j.id.starts_with("L0")),
            "{:?}",
            o.joints
        );
        o.joints.iter().find(|j| j.id == "L10B").unwrap().clone()
    };
    if gpu {
        // Through the view: the pointer over the joint, a right-click.
        look_at(&mut h, joint.at, 25.0);
        let at = on_screen(&mut h, joint.at);
        h.input_mut().events.push(egui::Event::PointerMoved(at));
        h.run();
        let hovered = h
            .state_mut()
            .ready()
            .unwrap()
            .view3d
            .link
            .as_ref()
            .and_then(|l| l.hovered.clone());
        assert_eq!(hovered.as_deref(), Some("L10B"));
        shot(&mut h, "design-link-tool-hover");
        click(&mut h, at, egui::PointerButton::Secondary);
    } else {
        h.state_mut().ready().unwrap().link_to_joint("L10B");
        h.run();
    }
    {
        let r = h.state_mut().ready().unwrap();
        assert!(r.link_from.is_none());
        let p = r.model.line.structure("T2").unwrap().point("A1").unwrap();
        assert_eq!(p.joint.as_deref(), Some("L10B"));
        assert!((0..3).all(|k| (p.position[k] - before.position[k]).abs() < 1e-6));
        assert!(p.offset_length() > 1.0, "{:?}", p.offset);
        let (status, error) = r.status.clone().unwrap();
        assert!(!error && status.contains("moment arm"), "{status}");
        assert!(r.perform(crate::edit::Action::Undo));
        let p = r.model.line.structure("T2").unwrap().point("A1").unwrap();
        assert_eq!(p.joint.as_deref(), Some("A1"));
        assert_eq!(p.offset, [0.0; 3]);
    }
}

/// A point dragged (its structure screen open) keeps its joint, its
/// offset the distance moved; released on another joint it is relinked
/// there at no offset; each one command. The structure screen's own drag
/// does the same in the type's file.
#[test]
fn dragging_changes_the_offset_and_relinks_on_drop() {
    let m = l10b_attach("link-drag");
    let s = m.line.structure("T2").unwrap().clone();
    let a1 = s.point("A1").unwrap().position;
    let local = to_local(s.position, s.orientation_deg, a1);
    let to = [local[0] + 0.5, local[1], local[2]];
    let (what, command) =
        crate::handles::command_onto(&m, "T2", &Handle::Point("A1".into()), to, None).unwrap();
    assert!(what.contains("on joint A1"), "{what}");
    let ohl_session::Command::Put { value, .. } = command else {
        panic!("{command:?}")
    };
    let put: ohl_line::Structure = serde_json::from_value(value).unwrap();
    let p = put.point("A1").unwrap();
    assert_eq!(p.joint.as_deref(), Some("A1"));
    assert!(
        (p.offset[0] - 0.5).abs() < 1e-9 && p.offset[1].abs() < 1e-9,
        "{:?}",
        p.offset
    );
    // Released on joint L10B: relinked there.
    let j = crate::link_tool::joints(&m, "T2")
        .into_iter()
        .find(|j| j.id == "L10B")
        .unwrap();
    let (what, command) = crate::handles::command_onto(
        &m,
        "T2",
        &Handle::Point("A1".into()),
        to_local(s.position, s.orientation_deg, j.at),
        Some(&Handle::Joint("L10B".into())),
    )
    .unwrap();
    assert!(what.contains("relinked to joint L10B"), "{what}");
    let ohl_session::Command::Put { value, .. } = command else {
        panic!("{command:?}")
    };
    let put: ohl_line::Structure = serde_json::from_value(value).unwrap();
    let p = put.point("A1").unwrap();
    assert_eq!(
        (p.joint.as_deref(), p.offset, p.position),
        (Some("L10B"), [0.0; 3], j.at)
    );
    // Applied: resolved there, one undo.
    let mut h = harness(m, false);
    let r = h.state_mut().ready().unwrap();
    assert!(r.perform(crate::edit::Action::Command {
        what,
        command: ohl_session::Command::Put {
            kind: ohl_session::Kind::Structure,
            id: "T2".into(),
            value: serde_json::to_value(&put).unwrap(),
            ord: None,
        }
    }));
    let p = r.model.line.structure("T2").unwrap().point("A1").unwrap();
    assert_eq!(p.joint.as_deref(), Some("L10B"));
    assert!((0..3).all(|k| (p.position[k] - j.at[k]).abs() < 1e-9));
    assert!(r.perform(crate::edit::Action::Undo));
    assert_eq!(
        r.model
            .line
            .structure("T2")
            .unwrap()
            .point("A1")
            .unwrap()
            .joint
            .as_deref(),
        Some("A1")
    );
    h.run();
}

/// A rigid link (A1 of T2 linked at an offset) is drawn on its layer in
/// its colour, hidden at zero offset, and a click on it (seen from above)
/// picks its attachment point; Properties shows its joint, offset and
/// length. Hidden with its layer, it is not picked.
#[test]
fn the_rigid_link_is_drawn_and_picked() {
    use crate::scene::Layer;
    use crate::selection::{Snap, Snaps, pick_in_model_case};
    let gpu = gpu();
    let mut h = harness(l10b_attach("link-drawn"), gpu);
    // None at zero offset.
    assert!(crate::link_tool::drawn_links(&h.state_mut().ready().unwrap().model).is_empty());
    use_tool(&mut h, Tool::Link, point("T2", "A1"));
    h.state_mut().ready().unwrap().link_to_joint("L10B");
    h.run();
    let (from, to) = {
        let r = h.state_mut().ready().unwrap();
        let links = crate::link_tool::drawn_links(&r.model);
        assert_eq!(links.len(), 1, "{links:?}");
        assert_eq!((links[0].0.as_str(), links[0].1.as_str()), ("T2", "A1"));
        let s = crate::scene::layer(&r.model, Layer::RigidLinks, 0, &Selection::None);
        assert!(s.counts().0 > 0, "drawn");
        (links[0].2, links[0].3)
    };
    // Picked from above at its middle (its layer shown).
    let mid = [0, 1, 2].map(|k| 0.5 * (from[k] + to[k]));
    let mut snaps = Snaps::default();
    snaps.0.insert(Snap::RigidLink);
    let r = h.state_mut().ready().unwrap();
    let (picked, _) = pick_in_model_case(&r.model, 0, &snaps, [mid[0], mid[1]], 0.3, |p| {
        Some([p[0], p[1]])
    });
    assert_eq!(picked, point("T2", "A1"));
    assert!(r.effective_snaps().on(Snap::RigidLink));
    r.visible.remove(&Layer::RigidLinks);
    assert!(
        !r.effective_snaps().on(Snap::RigidLink),
        "hidden: not picked"
    );
    r.visible.insert(Layer::RigidLinks);
    r.hotbar.hold(Tool::Select);
    r.selection = point("T2", "A1");
    h.run();
    h.get_by_label("L10B");
    h.get_by_label("offset length");
    if gpu {
        look_at(&mut h, mid, 30.0);
        h.run();
        shot(&mut h, "design-rigid-link-selected");
    }
}

/// Replace structure's Attachment points: T2 (type-02) by type-04 — both
/// points on joints there: replaced at once; T2 by type-03 — its points
/// 2.5 cm off type-03's: the nearest attach joint offered with the offset,
/// Replace waiting until each is chosen; chosen (one offered, one as the
/// new type has it), one command, one undo; T6 (type-05, its point on the
/// axis) by type-02: two attach joints equally near, none offered — the
/// user picks one, the offset kept.
#[test]
fn replace_maps_attachment_points() {
    use crate::replace::{Replace, Stage, Step};
    let mut h = harness(typed_project("link-replace"), gpu());
    let r = h.state_mut().ready().unwrap();
    // Exact: at once.
    let mut rep = Replace::start(&r.model, "T2").unwrap();
    match rep.chosen(&r.model, "type-04".into(), None) {
        Step::Apply(what, command) => {
            assert!(what.contains("rewired 2 wires"), "{what}");
            assert!(r.perform(crate::edit::Action::Command { what, command }));
        }
        _ => panic!("at once: {:?}", matches!(rep.stage, Stage::Map(_))),
    }
    assert_eq!(
        r.model
            .line
            .structure("T2")
            .unwrap()
            .structure_type
            .as_deref(),
        Some("type-04")
    );
    assert!(r.perform(crate::edit::Action::Undo));
    // Offered.
    let before = r
        .model
        .line
        .structure("T2")
        .unwrap()
        .point("A1")
        .unwrap()
        .position;
    let mut rep = Replace::start(&r.model, "T2").unwrap();
    assert!(matches!(
        rep.chosen(&r.model, "type-03".into(), None),
        Step::Wait
    ));
    let Stage::Map(m) = &mut rep.stage else {
        panic!("the mapping window")
    };
    assert_eq!(m.points.len(), 2);
    let a1 = &m.points[0];
    assert_eq!(a1.label, "A1");
    assert!(a1.matched.exact.is_none());
    let offer = a1.matched.nearest.clone().expect("offered");
    assert_eq!(offer.joint, "A1");
    assert!((offer.distance - 0.025).abs() < 1e-9, "{offer:?}");
    assert!(!m.points_chosen(), "Replace waits");
    assert!(a1.choices().contains(&PointChoice::Remove) == a1.by_type.is_none());
    m.point_choice[0] = a1.matched.offer();
    let by_type = m.points[1].by_type.clone().unwrap();
    m.point_choice[1] = Some(PointChoice::Link {
        joint: by_type.0,
        offset: by_type.1,
    });
    assert!(m.points_chosen());
    let command = m.command();
    let ohl_session::Command::ReplaceStructure { points, .. } = &command else {
        panic!()
    };
    assert_eq!(points.len(), 1, "only the offered one moves: {points:?}");
    assert!(r.perform(crate::edit::Action::Command {
        what: "replace".into(),
        command
    }));
    let p = r
        .model
        .line
        .structure("T2")
        .unwrap()
        .point("A1")
        .unwrap()
        .clone();
    assert_eq!(p.joint.as_deref(), Some("A1"));
    assert!((p.offset_length() - 0.025).abs() < 1e-9);
    assert!(
        (0..3).all(|k| (p.position[k] - before[k]).abs() < 1e-6),
        "stays"
    );
    assert!(r.perform(crate::edit::Action::Undo));
    assert_eq!(
        r.model
            .line
            .structure("T2")
            .unwrap()
            .structure_type
            .as_deref(),
        Some("type-02")
    );
    // A tie: none offered; the user's joint, the offset kept.
    let mut rep = Replace::start(&r.model, "T6").unwrap();
    let step = rep.chosen(&r.model, "type-02".into(), None);
    assert!(matches!(step, Step::Wait));
    let Stage::Map(m) = &mut rep.stage else {
        panic!("the mapping window")
    };
    let row = &m.points[0];
    assert!(row.matched.tie && row.matched.nearest.is_none());
    let choice = row
        .choices()
        .into_iter()
        .find(|c| matches!(c, PointChoice::Link { joint, .. } if joint == "A2"))
        .unwrap();
    m.point_choice[0] = Some(choice);
    let (built, moves) = m.built().unwrap();
    assert_eq!(moves.len(), 1);
    assert_eq!(built.point("A1").unwrap().joint.as_deref(), Some("A2"));
    r.replace = Some(rep);
    h.run();
    h.get_by_label("Attachment points (1)");
    // The window for T3 (type-03) by type-02: A1 and A11 offered 2.5 cm
    // away, A2 offered 6 m away; the first chosen as offered.
    let r = h.state_mut().ready().unwrap();
    let mut rep = Replace::start(&r.model, "T3").unwrap();
    assert!(matches!(
        rep.chosen(&r.model, "type-02".into(), None),
        Step::Wait
    ));
    if let Stage::Map(m) = &mut rep.stage {
        assert_eq!(m.points.len(), 3);
        m.point_choice[0] = m.points[0].matched.offer();
    }
    r.replace = Some(rep);
    h.run();
    h.get_by_label("Attachment points (3)");
    shot(&mut h, "design-replace-attachment-points");
}

/// A new string's point on a typed structure: only the attach joints of
/// its type are offered (T2, type-02: A1 and A2, not the feet nor the
/// plain joints); applied, the point is linked to the joint chosen.
#[test]
fn a_new_point_is_offered_only_attach_joints() {
    let m = typed_project("link-new-point");
    let offered: Vec<String> = crate::link_tool::attach_joints(&m, "T2")
        .into_iter()
        .map(|j| j.0)
        .collect();
    assert_eq!(offered, ["A1", "A2"]);
    let t = m.types.as_ref().unwrap().0.get("type-02").unwrap();
    for j in &offered {
        assert_eq!(
            t.joint(j).unwrap().kind,
            Some(ohl_structures::JointKind::Attach)
        );
    }
    // A structure without a type: none (its points are their own joints).
    let untyped = crate::tests::synthetic_project("link-new-point-untyped");
    assert!(crate::link_tool::attach_joints(&untyped, "T2").is_empty());
    // The form: a new point linked to A2 at 0.3 m.
    let mut f = crate::editors::StringForm::new_on(&m, "T2").unwrap();
    let a2 = crate::link_tool::attach_joints(&m, "T2")
        .into_iter()
        .find(|j| j.0 == "A2")
        .unwrap();
    f.points.push(crate::editors::PointRow {
        label: "A9".into(),
        at: [a2.1[0], a2.1[1], a2.1[2] + 0.3],
        description: String::new(),
    });
    f.joints.insert("A9".into(), a2.clone());
    let mut s = m.stored_line().unwrap().structure("T2").unwrap().clone();
    f.apply_to(&mut s, m.line.structure("T2"));
    let p = s.point("A9").unwrap();
    assert_eq!(p.joint.as_deref(), Some("A2"));
    assert!((p.offset[2] - 0.3).abs() < 1e-9, "{:?}", p.offset);
}

/// The structure screen (Strings and points… on T2, type-02, its joint
/// L10B made an attach joint): the hotbar's Link tool held holds the
/// screen's; a click on attachment point A1 in its view, then on joint
/// L10B, links A1 there in the type's file (as edited) where it is; a
/// right-click cancels a point chosen. Joint L10A (no kind) is not
/// ringed: under the pointer it says it is not an attach joint, a click
/// on it links nothing (owner decision 2026-10-21). The Attachment
/// points table shows the joint, the offset and its length, the rigid
/// link drawn.
#[test]
fn the_structure_screen_links_with_two_clicks() {
    use crate::library::LibraryKind;
    if !gpu() {
        return;
    }
    let mut h = harness(l10b_attach("link-screen"), true);
    h.state_mut().ready().unwrap().selection = Selection::Structure("T2".into());
    h.run();
    h.get_by_label(crate::properties::STRINGS_AND_POINTS)
        .click();
    h.run();
    h.run();
    h.state_mut().ready().unwrap().hotbar.hold(Tool::Link);
    h.run();
    h.run();
    let screen = |h: &mut Harness<'_, App>, p: [f64; 3]| {
        let r = h.state_mut().ready().unwrap();
        r.libraries[&LibraryKind::StructureTypes]
            .workspace
            .screen_of(p)
            .unwrap()
    };
    let workspace = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        let w = &r.libraries[&LibraryKind::StructureTypes].workspace;
        (
            w.link.clone(),
            r.libraries[&LibraryKind::StructureTypes].text().unwrap(),
        )
    };
    assert_eq!(workspace(&mut h).0, Some(None), "held with the hotbar's");
    // A11 chosen, then a right-click: cancelled.
    let a11 = screen(&mut h, [-6.0, 0.0, 30.0]);
    click(&mut h, a11, egui::PointerButton::Primary);
    assert_eq!(workspace(&mut h).0, Some(Some("A11".into())));
    click(&mut h, a11, egui::PointerButton::Secondary);
    assert_eq!(workspace(&mut h).0, Some(None));
    // A1, then L10B.
    let a1 = screen(&mut h, [6.0, 0.0, 30.0]);
    click(&mut h, a1, egui::PointerButton::Primary);
    let t = h.state_mut().ready().unwrap().libraries[&LibraryKind::StructureTypes]
        .workspace
        .shown()
        .unwrap()
        .clone();
    // L10A: not an attach joint, its hover says so, a click links nothing.
    let plain = screen(&mut h, t.joint("L10A").unwrap().position);
    h.input_mut().events.push(egui::Event::PointerMoved(plain));
    h.run();
    assert_eq!(
        h.state_mut().ready().unwrap().libraries[&LibraryKind::StructureTypes]
            .workspace
            .link_hover
            .as_deref(),
        Some("L10A")
    );
    let before = workspace(&mut h).1;
    click(&mut h, plain, egui::PointerButton::Primary);
    assert_eq!(workspace(&mut h), (Some(Some("A1".into())), before));
    let message = h.state_mut().ready().unwrap().libraries[&LibraryKind::StructureTypes]
        .workspace
        .message
        .clone();
    assert!(
        message
            .as_ref()
            .is_some_and(|m| m.0.contains(ohl_structures::NOT_ATTACH)),
        "{message:?}"
    );
    let j = t.joint("L10B").unwrap().position;
    let at = screen(&mut h, j);
    h.input_mut().events.push(egui::Event::PointerMoved(at));
    h.run();
    assert_eq!(
        h.state_mut().ready().unwrap().libraries[&LibraryKind::StructureTypes]
            .workspace
            .link_hover
            .as_deref(),
        Some("L10B")
    );
    click(&mut h, at, egui::PointerButton::Primary);
    let (link, text) = workspace(&mut h);
    assert_eq!(link, Some(None));
    assert!(text.contains("node = \"L10B\""), "{text}");
    assert!(text.contains("offset = ["), "{text}");
    // The table: its joint and the offset's length.
    h.state_mut().ready().unwrap().hotbar.hold(Tool::Select);
    h.run();
    h.run();
    h.get_by_label("offset x");
    {
        let r = h.state_mut().ready().unwrap();
        let w = &mut r
            .libraries
            .get_mut(&LibraryKind::StructureTypes)
            .unwrap()
            .workspace;
        w.selected = vec![Handle::Point("A1".into())];
        w.redraw();
    }
    h.run();
    h.run();
    shot(&mut h, "design-structure-screen-rigid-link");
}

/// Unlink (Properties of the point): it stays where it is, drawn red; the
/// structure's Properties lists it; one undo links it again.
#[test]
fn an_unlinked_point_is_drawn_red_and_listed() {
    use crate::scene::{Layer, prims};
    let mut h = harness(typed_project("link-unlinked"), false);
    let r = h.state_mut().ready().unwrap();
    let before = r
        .model
        .line
        .structure("T2")
        .unwrap()
        .point("A1")
        .unwrap()
        .position;
    let red = |m: &crate::model::Model| {
        prims(m, Layer::Structures, 0)
            .into_iter()
            .filter(|p| p.dot && p.color == crate::link_tool::UNLINKED)
            .count()
    };
    assert_eq!(red(&r.model), 0);
    let (what, command) = crate::link_tool::unlink_command(&r.model, "T2", "A1").unwrap();
    assert!(r.perform(crate::edit::Action::Command { what, command }));
    let p = r.model.line.structure("T2").unwrap().point("A1").unwrap();
    assert!(p.unlinked && !p.is_linked());
    assert_eq!(p.position, before);
    assert_eq!(red(&r.model), 1);
    r.selection = Selection::Structure("T2".into());
    h.run();
    h.get_by_label_contains("Unlinked attachment point(s) A1");
    let r = h.state_mut().ready().unwrap();
    assert!(r.perform(crate::edit::Action::Undo));
    assert_eq!(red(&r.model), 0);
}

/// The link follows edits in the Edit tool's windows (owner decision
/// 2026-10-21): A1 of T2, linked to L10B, moved 0.3 m along x in its
/// structure window stays on L10B, its offset 0.3 m more in x; in a
/// string window a type's point moved stays on its type's joint at the
/// offset moved.
#[test]
fn the_edit_windows_keep_a_moved_point_on_its_joint() {
    let m = l10b_attach("link-edit-windows");
    let mut h = harness(m, false);
    let r = h.state_mut().ready().unwrap();
    let (what, command) = crate::link_tool::link_command(&r.model, "T2", "A1", "L10B").unwrap();
    assert!(r.perform(crate::edit::Action::Command { what, command }));
    let linked = r
        .model
        .line
        .structure("T2")
        .unwrap()
        .point("A1")
        .unwrap()
        .clone();
    let mut f = crate::editors::StructureForm::of(&r.model, "T2").unwrap();
    let row = f.points.iter_mut().find(|p| p.label == "A1").unwrap();
    row.at[0] += 0.3;
    let (_, command) = f.command(r.model.stored_line()).unwrap();
    assert!(r.perform(crate::edit::Action::Command {
        what: "moved".into(),
        command
    }));
    let p = r.model.line.structure("T2").unwrap().point("A1").unwrap();
    assert_eq!(p.joint.as_deref(), Some("L10B"));
    let close = |a: [f64; 3], b: [f64; 3]| (0..3).all(|k| (a[k] - b[k]).abs() < 1e-9);
    assert!(
        close(
            p.offset,
            [linked.offset[0] + 0.3, linked.offset[1], linked.offset[2]]
        ),
        "{:?} from {:?}",
        p.offset,
        linked.offset
    );
    // A string window: A11 (the type's, on joint A2) moved 0.2 m up.
    let mut f = crate::editors::StringForm::new_on(&r.model, "T2").unwrap();
    let row = f.points.iter_mut().find(|p| p.label == "A11").unwrap();
    row.at[2] += 0.2;
    let mut s = r
        .model
        .stored_line()
        .unwrap()
        .structure("T2")
        .unwrap()
        .clone();
    f.apply_to(&mut s, r.model.line.structure("T2"));
    let p = s.point("A11").unwrap();
    assert_eq!(p.joint.as_deref(), Some("A2"));
    assert!(close(p.offset, [0.0, 0.0, 0.2]), "{:?}", p.offset);
}
