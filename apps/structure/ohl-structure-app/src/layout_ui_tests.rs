// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Lay out joints (2026-11-18): Joint details › Lay out joint… opens the
//! window, its preview is worked out on a copy, Apply is one undo step,
//! the joint shows what is automatic, and a later edit near it is
//! followed in the same undo step.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use ohl_structure_edit::Handle;

use crate::app::StructureApp;
use crate::joint_ui_tests::ANGLES;
use crate::tests::{copy_of, harness, menu};

/// The angles fixture (D1 and H1 running into each other at N2) open,
/// Joint details at N2.
fn open(tag: &str) -> (Harness<'static, StructureApp>, std::path::PathBuf) {
    let path = copy_of("type-02.ohls", tag);
    let dir = path.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
        dir.join("Sections/example-sections.ohlsec"),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("Bolts")).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/bolts/example-bolts.ohlb"),
        dir.join("Bolts/example-bolts.ohlb"),
    )
    .unwrap();
    // D1 and H1 turned half a turn: on the outer face of the leg (as
    // written they lie on its inner face, where no plate is laid out).
    let text = ANGLES
        .replace(
            "orientation = \"y-z+\"\ndesign = { class = \"bracing\" }",
            "orientation = \"y-z+\"\nroll = \"180 deg\"\ndesign = { class = \"bracing\" }",
        )
        .replace(
            "orientation = \"z+y-\"\n",
            "orientation = \"z+y-\"\nroll = \"180 deg\"\n",
        );
    let tower = dir.join("angles.ohls");
    std::fs::write(&tower, text).unwrap();
    let mut h = harness(&tower);
    menu(&mut h, "Design", "Joint details");
    h.state_mut().selected = vec![Handle::Joint("N2".into())];
    h.run_steps(3);
    (h, dir)
}

/// Runs frames until the window's preview is there (at most ~30 s).
fn wait_preview(h: &mut Harness<'static, StructureApp>) {
    for _ in 0..600 {
        h.step();
        let ready = h
            .state()
            .layout_window
            .as_ref()
            .is_some_and(|w| w.preview.is_some());
        if ready {
            h.run_steps(2);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("no preview");
}

/// Runs frames until Apply is enabled: the preview is the window's — its
/// joints (with their similar ones once the joint details are worked out
/// on their own thread: the preview is worked out again then) and the
/// file's revision; at most ~30 s.
fn wait_apply(h: &mut Harness<'static, StructureApp>) {
    use egui_kittest::kittest::NodeT;
    for _ in 0..600 {
        h.step();
        let ready = h
            .query_all_by_label_contains("Apply (")
            .next()
            .is_some_and(|n| !n.accesskit_node().is_disabled());
        if ready {
            // (A few frames more: the button as the window now draws it.)
            h.run_steps(4);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("Apply never enabled");
}

#[test]
fn a_joint_is_laid_out_from_its_details_in_one_undo_step() {
    let (mut h, dir) = open("layout-window");
    let before = h.state().file.as_ref().unwrap().text();
    // D1 and H1 run into each other at N2: a clash, said.
    let d = h.state_mut().joint_details().unwrap();
    let clashes = d
        .as_ref()
        .as_ref()
        .unwrap()
        .joint("N2")
        .unwrap()
        .clashes
        .len();
    assert!(clashes > 0);
    h.get_by_label("Lay out joint…").click();
    h.step();
    assert!(h.state().layout_window.is_some());
    wait_preview(&mut h);
    {
        let w = h.state().layout_window.as_ref().unwrap();
        let p = w.preview.as_ref().unwrap();
        let n2 = p.run.joints.iter().find(|o| o.joint == "N2").unwrap();
        assert!(n2.written, "{:?}", n2.hand);
        assert!(p.run.clashes.1 < p.run.clashes.0, "{:?}", p.run.clashes);
    }
    // The window's picture for the user guide (OHL_RENDER_PNG).
    crate::tool_window_tests::window_shot(&mut h, "Lay out joints", "lay-out-joints");
    if ohl_core::env::var_os("OHL_RENDER_PNG").is_some() {
        // The joint before and after, scrolled into view.
        h.get_by_label("Before and after:").scroll_to_me();
        h.run_steps(3);
        crate::tool_window_tests::window_shot(
            &mut h,
            "Lay out joints",
            "lay-out-joints-before-after",
        );
    }
    // Nothing written before Apply.
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    wait_apply(&mut h);
    h.get_all_by_label_contains("Apply (")
        .next()
        .unwrap()
        .click();
    h.run_steps(2);
    assert!(h.state().layout_window.is_none());
    let f = h.state().file.as_ref().unwrap();
    assert_ne!(f.text(), before);
    assert!(f.undo_name().unwrap().starts_with("Lay out joints"));
    let rec = ohl_structure_link::LayoutRecord::of(f.structure());
    assert!(rec.joints.contains("N2"));
    let d = h.state_mut().joint_details().unwrap();
    let after = d
        .as_ref()
        .as_ref()
        .unwrap()
        .joint("N2")
        .unwrap()
        .clashes
        .len();
    assert!(after < clashes, "{after} {clashes}");
    // Joint details says what is automatic.
    h.run_steps(2);
    assert!(
        h.query_all_by_label_contains("laid out automatically")
            .next()
            .is_some()
    );
    // One undo step takes it all back.
    h.state_mut().file.as_mut().unwrap().undo();
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn escape_closes_the_window_and_changes_nothing() {
    let (mut h, dir) = open("layout-escape");
    let before = h.state().file.as_ref().unwrap().text();
    h.state_mut()
        .open_layout_window(crate::layout_window::Scope::Joint("N2".into()));
    h.step();
    h.key_press(eframe::egui::Key::Escape);
    h.step();
    assert!(h.state().layout_window.is_none());
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn an_edit_near_a_laid_out_joint_is_followed_in_the_same_undo_step() {
    let (mut h, dir) = open("layout-follow");
    h.state_mut()
        .open_layout_window(crate::layout_window::Scope::Joint("N2".into()));
    wait_preview(&mut h);
    // A few frames more: Apply is enabled once the preview is the
    // window's (its key and the file's revision).
    h.run_steps(4);
    h.get_all_by_label_contains("Apply (")
        .next()
        .unwrap()
        .click();
    h.run_steps(2);
    let laid = h.state().file.as_ref().unwrap().text();
    let steps = h.state().file.as_ref().unwrap().undo_steps();
    // H1's section changed: N2 is laid out again with it, in that edit's
    // undo step; the message says so.
    h.state_mut()
        .file
        .as_mut()
        .unwrap()
        .edit("Section of H1", |doc, _| {
            ohl_structure_edit::edits::set_member_section(doc, "H1", Some("EA-61x7"))
        });
    h.run_steps(2);
    let f = h.state().file.as_ref().unwrap();
    assert_eq!(f.undo_steps(), steps + 1);
    let msg = h.state().message.clone().map(|m| m.0).unwrap_or_default();
    assert!(
        msg.contains("laid out again") || msg.contains("needs a hand detail"),
        "{msg}"
    );
    // Undo: the layout as it was.
    h.state_mut().file.as_mut().unwrap().undo();
    assert_eq!(h.state().file.as_ref().unwrap().text(), laid);
    std::fs::remove_dir_all(dir).ok();
}

/// J2 (2026-11-19): a joint not laid out whole names the members left —
/// the rest laid out — and those whose place is kept free; a joint left
/// whole says so.
#[test]
fn the_hand_list_names_the_members_left_and_the_place_kept() {
    let plan = ohl_structure_link::JointPlan {
        joint: "N2".into(),
        hand_members: vec!["R1".into(), "R2".into()],
        kept_free: vec!["R1".into()],
        hand: vec!["R1: too short".into(), "R2: no room".into()],
        ..Default::default()
    };
    let mut o = ohl_structure_link::JointOutcome {
        joint: "N2".into(),
        hand: plan.hand.clone(),
        plan,
        written: true,
        turned: Vec::new(),
        plates: Vec::new(),
    };
    assert_eq!(
        crate::layout_window::hand_words(&o),
        "N2: by hand R1 (its place kept free), R2, the rest laid out — R1: too short; R2: no room"
    );
    o.written = false;
    assert_eq!(
        crate::layout_window::hand_words(&o),
        "N2: left whole — R1: too short; R2: no room"
    );
}

/// An invented tower for the bent plate (J2b, 2026-11-19): a horizontal
/// H1–H2 running through N2, one leg pointing +y, the other down, and a
/// plan member P1 arriving from −y on the inner face of its horizontal
/// leg, over the heel.
const BENT: &str = r#"format = "ohl-structure"
version = 18
id = "bent-example"
name = "Bent plate example"
kind = "lattice"

[[node]]
id = "N1"
point = ["0 m", "0 m", "3 m"]
support = "fixed"

[[node]]
id = "N2"
point = ["3 m", "0 m", "3 m"]

[[node]]
id = "N3"
point = ["6 m", "0 m", "3 m"]
support = "fixed"

[[node]]
id = "N4"
point = ["3 m", "-2 m", "3 m"]
support = "fixed"

[[member]]
id = "H1"
nodes = ["N1", "N2"]
section = "EA-100x10"
orientation = "y+z-"
design = { class = "bracing" }

[[member]]
id = "H2"
nodes = ["N2", "N3"]
section = "EA-100x10"
orientation = "y+z-"
design = { class = "bracing" }

[[member]]
id = "P1"
nodes = ["N4", "N2"]
section = "EA-61x7"
orientation = "x+z-"
design = { class = "redundant" }

[[connection]]
member = "P1"
joint = "N2"
face = "inside"
"#;

/// J2b (2026-11-19): the window lays P1 out onto a bent plate round H's
/// other leg; its picture for the user guide.
#[test]
fn an_end_over_the_heel_is_laid_out_on_a_bent_plate_in_the_window() {
    let path = copy_of("type-02.ohls", "layout-bent");
    let dir = path.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    let lib = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
    )
    .unwrap();
    std::fs::write(
        dir.join("Sections/example-sections.ohlsec"),
        format!(
            "{lib}\n[[section]]\nid = \"EA-100x10\"\nname = \"Equal angle 100 × 10 \
             (invented)\"\nshape = \"equal_angle\"\nh = \"100 mm\"\nt = \"10 mm\"\nr = \"12 mm\"\n"
        ),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("Bolts")).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/bolts/example-bolts.ohlb"),
        dir.join("Bolts/example-bolts.ohlb"),
    )
    .unwrap();
    let tower = dir.join("bent.ohls");
    std::fs::write(&tower, BENT).unwrap();
    let mut h = harness(&tower);
    menu(&mut h, "Design", "Joint details");
    h.state_mut().selected = vec![Handle::Joint("N2".into())];
    h.run_steps(3);
    h.get_by_label("Lay out joint…").click();
    h.step();
    wait_preview(&mut h);
    {
        let w = h.state().layout_window.as_ref().unwrap();
        let p = w.preview.as_ref().unwrap();
        let n2 = p.run.joints.iter().find(|o| o.joint == "N2").unwrap();
        assert!(n2.written && n2.hand.is_empty(), "{:?}", n2.hand);
        assert!(
            n2.plan
                .plates
                .iter()
                .any(|q| q.own.as_ref().is_some_and(|o| o.bend.is_some())),
            "{:?}",
            n2.plan
        );
    }
    if ohl_core::env::var_os("OHL_RENDER_PNG").is_some() {
        h.get_by_label("Before and after:").scroll_to_me();
        h.run_steps(3);
        crate::tool_window_tests::window_shot(
            &mut h,
            "Lay out joints",
            "lay-out-joints-bent-plate",
        );
    }
    wait_apply(&mut h);
    h.get_all_by_label_contains("Apply (")
        .next()
        .unwrap()
        .click();
    h.run_steps(2);
    let f = h.state().file.as_ref().unwrap();
    assert_eq!(f.structure().bent_pairs("N2").len(), 1);
}

/// An invented K-brace for the plate bent by a few degrees (L3,
/// 2026-11-20): the legs L1–L2 turned 8° (`roll`), the diagonal D1 lying
/// flat on the horizontals H0–H1 at its foot N4, so a few degrees off the
/// leg's flange at N2 — no one roll lays both its ends flat.
const SMALL_BEND: &str = r#"id = "K"
name = "Small bend example"
kind = "lattice"

[[node]]
id = "N1"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "N2"
point = ["0 m", "0 m", "3 m"]

[[node]]
id = "N3"
point = ["0 m", "0 m", "6 m"]
support = "fixed"

[[node]]
id = "N4"
point = ["2.5 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "N5"
point = ["2.5 m", "0 m", "6 m"]
support = "fixed"

[[node]]
id = "N6"
point = ["5 m", "0 m", "0 m"]
support = "fixed"

[[member]]
id = "L1"
nodes = ["N1", "N2"]
section = "EA-61x7"
orientation = "x+y+"
roll = "8 deg"
design = { class = "leg" }

[[member]]
id = "L2"
nodes = ["N2", "N3"]
section = "EA-61x7"
orientation = "x+y+"
roll = "8 deg"
design = { class = "leg" }

[[member]]
id = "D1"
nodes = ["N4", "N2"]
section = "EA-61x7"
orientation = "y-z+"
design = { class = "bracing" }

[[member]]
id = "D2"
nodes = ["N5", "N2"]
section = "EA-61x7"
orientation = "z-y-"
design = { class = "bracing" }

[[member]]
id = "H0"
nodes = ["N1", "N4"]
section = "EA-61x7"
design = { class = "bracing" }

[[member]]
id = "H1"
nodes = ["N4", "N6"]
section = "EA-61x7"
design = { class = "bracing" }
"#;

/// L3 (2026-11-20): the window lays D1 out onto a plate bent by its few
/// degrees from the flat plate on the leg's flange; its picture (the
/// close-up before and after) for the user guide.
#[test]
fn an_end_a_few_degrees_off_is_laid_out_on_a_small_bend_in_the_window() {
    let path = copy_of("type-02.ohls", "layout-small-bend");
    let dir = path.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
        dir.join("Sections/example-sections.ohlsec"),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("Bolts")).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/bolts/example-bolts.ohlb"),
        dir.join("Bolts/example-bolts.ohlb"),
    )
    .unwrap();
    let tower = dir.join("small-bend.ohls");
    std::fs::write(&tower, SMALL_BEND).unwrap();
    let mut h = harness(&tower);
    menu(&mut h, "Design", "Joint details");
    h.state_mut().selected = vec![Handle::Joint("N2".into())];
    h.run_steps(3);
    h.get_by_label("Lay out joint…").click();
    h.step();
    wait_preview(&mut h);
    {
        let w = h.state().layout_window.as_ref().unwrap();
        let p = w.preview.as_ref().unwrap();
        let n2 = p.run.joints.iter().find(|o| o.joint == "N2").unwrap();
        assert!(n2.written, "{:?}", n2.hand);
        let bend = n2
            .plan
            .plates
            .iter()
            .find_map(|q| q.own.as_ref().and_then(|o| o.bend.as_ref()))
            .unwrap_or_else(|| panic!("no bent plate: {:?}", n2.plan));
        assert!(
            bend.angle > ohl_structure_link::FLAT_OFF
                && bend.angle <= ohl_structure_link::BEND_SMALL_MAX,
            "{bend:?}"
        );
    }
    if ohl_core::env::var_os("OHL_RENDER_PNG").is_some() {
        h.get_by_label("Before and after:").scroll_to_me();
        h.run_steps(3);
        // Turned to look down the leg, the bend's few degrees seen.
        if let Some(w) = h.state_mut().layout_window.as_mut() {
            for v in [&mut w.views.0, &mut w.views.1] {
                v.yaw += 0.6;
                v.pitch += 0.45;
                v.zoom = 0.6;
            }
        }
        h.run_steps(3);
        crate::tool_window_tests::window_shot(
            &mut h,
            "Lay out joints",
            "lay-out-joints-small-bend",
        );
    }
    wait_apply(&mut h);
    h.get_all_by_label_contains("Apply (")
        .next()
        .unwrap()
        .click();
    h.run_steps(2);
    let f = h.state().file.as_ref().unwrap();
    assert!(
        f.structure()
            .plates
            .iter()
            .any(|p| p
                .bend
                .as_ref()
                .is_some_and(|b| b.angle.si().to_degrees() < 20.0)),
        "{}",
        f.text()
    );
    std::fs::remove_dir_all(dir).ok();
}

/// Pieces in line (O44 J9, D15): a horizontal in two pieces of one section
/// through a mid-panel joint (an invented tower). The preview proposes them
/// one part and writes nothing of it; *Confirm all* writes both ends one
/// part in one undo step, the preview is worked out again with nothing
/// left to propose; undo takes it back.
#[test]
fn pieces_in_line_are_proposed_and_confirmed_in_one_undo_step() {
    let path = copy_of("type-02.ohls", "layout-in-line");
    let dir = path.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
        dir.join("Sections/example-sections.ohlsec"),
    )
    .unwrap();
    let tower = dir.join("split.ohls");
    std::fs::write(&tower, IN_LINE).unwrap();
    let mut h = harness(&tower);
    h.run_steps(3);
    let before = h.state().file.as_ref().unwrap().text();
    h.state_mut()
        .open_layout_window(crate::layout_window::Scope::Tower);
    wait_preview(&mut h);
    h.run_steps(4);
    {
        let p = h.state().layout_window.as_ref().unwrap();
        let p = p.preview.as_ref().unwrap();
        assert_eq!(p.run.in_line.len(), 1, "{:?}", p.run.in_line);
        assert!(p.run.in_line[0].proposed());
    }
    if ohl_core::env::var_os("OHL_RENDER_PNG").is_some() {
        // The proposal per joint opened and scrolled into view.
        h.get_by_label_contains("Each joint (").click();
        h.run_steps(2);
        h.get_by_label("Confirm all (1)").scroll_to_me();
        h.run_steps(3);
        crate::tool_window_tests::window_shot(&mut h, "Lay out joints", "lay-out-joints-in-line");
    }
    // Nothing written by the preview.
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    h.get_by_label("Confirm all (1)").click();
    h.run_steps(2);
    let f = h.state().file.as_ref().unwrap();
    assert_eq!(f.structure().continuous_with("H1", "M"), Some("H2"));
    assert!(
        f.undo_name()
            .unwrap()
            .starts_with("Pieces in line one part")
    );
    // The preview again: nothing left to propose.
    let mut fresh = false;
    for _ in 0..600 {
        h.step();
        let revision = h.state().file.as_ref().unwrap().revision();
        fresh = h
            .state()
            .layout_window
            .as_ref()
            .and_then(|w| w.preview.as_ref())
            .is_some_and(|p| p.revision == revision);
        if fresh {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(fresh, "no preview after confirming");
    let w = h.state().layout_window.as_ref().unwrap();
    let p = w.preview.as_ref().unwrap();
    assert!(p.run.in_line.is_empty(), "{:?}", p.run.in_line);
    h.state_mut().file.as_mut().unwrap().undo();
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    std::fs::remove_dir_all(dir).ok();
}

/// An invented tower: a horizontal in two pieces (H1, H2) of one section
/// through a mid-panel joint M, a redundant (V) standing on it.
const IN_LINE: &str = r#"id = "S"
name = "pieces in line"
kind = "lattice"

[[node]]
id = "A"
point = ["-2 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "M"
point = ["0 m", "0 m", "0 m"]

[[node]]
id = "B"
point = ["2 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "T"
point = ["0 m", "0 m", "1.5 m"]
support = "fixed"

[[member]]
id = "H1"
nodes = ["A", "M"]
section = "EA-61x7"
orientation = "z+y-"
design = { class = "bracing", bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "H2"
nodes = ["M", "B"]
section = "EA-61x7"
orientation = "z+y-"
design = { class = "bracing", bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "V"
nodes = ["M", "T"]
section = "EA-61x7"
orientation = "y-x+"
design = { class = "redundant" }
"#;

/// Leg splices (O44 J7, the owner's decisions of 2026-11-19): a leg in two
/// pieces of other sections (C1 an L 150 × 12, C2 an L 150 × 15) through J
/// with a brace D on it (an invented tower). The preview proposes the
/// splice and writes nothing of it; *Confirm all* writes it in one undo
/// step, the preview is worked out again with nothing left to propose and
/// D bolted through the outer cover; undo takes it back.
#[test]
fn leg_splices_are_proposed_and_confirmed_in_one_undo_step() {
    let path = copy_of("type-02.ohls", "layout-leg-splices");
    let dir = path.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    let mut sections = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
    )
    .unwrap();
    for (id, t) in [("EA-150x12", 12), ("EA-150x15", 15)] {
        sections.push_str(&format!(
            "\n[[section]]\nid = \"{id}\"\nname = \"Equal angle 150 × {t} (invented)\"\n\
             shape = \"equal_angle\"\nh = \"150 mm\"\nt = \"{t} mm\"\nr = \"16 mm\"\n"
        ));
    }
    std::fs::write(dir.join("Sections/example-sections.ohlsec"), sections).unwrap();
    let tower = dir.join("leg.ohls");
    std::fs::write(&tower, LEG).unwrap();
    let mut h = harness(&tower);
    h.run_steps(3);
    let before = h.state().file.as_ref().unwrap().text();
    h.state_mut()
        .open_layout_window(crate::layout_window::Scope::Tower);
    wait_preview(&mut h);
    h.run_steps(4);
    {
        let p = h.state().layout_window.as_ref().unwrap();
        let p = p.preview.as_ref().unwrap();
        assert_eq!(p.run.splices.len(), 1, "{:?}", p.run.splices);
        assert!(p.run.splices[0].proposed());
    }
    if ohl_core::env::var_os("OHL_RENDER_PNG").is_some() {
        // The proposal per joint opened and scrolled into view.
        h.get_by_label_contains("Each leg joint (").click();
        h.run_steps(2);
        h.get_all_by_label_contains("Confirm all (1)")
            .next()
            .unwrap()
            .scroll_to_me();
        h.run_steps(3);
        crate::tool_window_tests::window_shot(
            &mut h,
            "Lay out joints",
            "lay-out-joints-leg-splices",
        );
    }
    // Nothing written by the preview.
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    h.get_all_by_label_contains("Confirm all (1)")
        .next()
        .unwrap()
        .click();
    h.run_steps(2);
    let f = h.state().file.as_ref().unwrap();
    assert_eq!(f.structure().splices.len(), 1, "{}", f.text());
    assert!(f.undo_name().unwrap().starts_with("Leg splice places"));
    // The preview again: nothing left to propose; D through the cover.
    let mut fresh = false;
    for _ in 0..600 {
        h.step();
        let revision = h.state().file.as_ref().unwrap().revision();
        fresh = h
            .state()
            .layout_window
            .as_ref()
            .and_then(|w| w.preview.as_ref())
            .is_some_and(|p| p.revision == revision);
        if fresh {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(fresh, "no preview after confirming");
    let w = h.state().layout_window.as_ref().unwrap();
    let p = w.preview.as_ref().unwrap();
    assert!(p.run.splices.is_empty(), "{:?}", p.run.splices);
    let d = p
        .after
        .joint("J")
        .and_then(|j| j.arrivals.iter().find(|a| a.member == "D"))
        .expect("D at J");
    assert!(d.through.is_some(), "{d:?}");
    h.state_mut().file.as_mut().unwrap().undo();
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    std::fs::remove_dir_all(dir).ok();
}

/// An invented tower: a leg in two pieces of other sections (C1, C2)
/// through J, both with their bolts (the import's, two a piece), and a
/// brace D from J.
const LEG: &str = r#"id = "L"
name = "leg splice"
kind = "lattice"

[[node]]
id = "A"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "J"
point = ["0 m", "0 m", "3 m"]

[[node]]
id = "B"
point = ["0 m", "0 m", "6 m"]
support = "fixed"

[[node]]
id = "E"
point = ["1.2 m", "0 m", "6 m"]
support = "fixed"

[[member]]
id = "C1"
nodes = ["A", "J"]
section = "EA-150x12"
orientation = "x+y+"
design = { class = "leg", bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "C2"
nodes = ["J", "B"]
section = "EA-150x15"
orientation = "x+y+"
design = { class = "leg", bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "D"
nodes = ["E", "J"]
section = "EA-61x7"
orientation = "y-z+"
design = { class = "bracing" }
"#;

/// An invented K-brace for the ends moved to bolt directly (L5,
/// 2026-11-20): the leg an invented 160 × 14 angle running through N2 (one
/// part), the two diagonals with two bolts each. Without the moves both go
/// onto a plate on the leg (pulled back clear of each other, their bolts
/// past its flange); with them one lies at the work point and the other is
/// pulled back further clear of it (moved a few millimetres where that is
/// not enough) — both bolted directly, no plate.
const WIDE_K: &str = r#"id = "K"
name = "Wide leg example"
kind = "lattice"

[[node]]
id = "N1"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "N2"
point = ["0 m", "0 m", "3 m"]

[[node]]
id = "N3"
point = ["0 m", "0 m", "6 m"]
support = "fixed"

[[node]]
id = "N4"
point = ["3.6 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "N5"
point = ["3.6 m", "0 m", "6 m"]
support = "fixed"

[[member]]
id = "L1"
nodes = ["N1", "N2"]
section = "EA-160x14"
orientation = "x+y+"
design = { class = "leg" }

[[member]]
id = "L2"
nodes = ["N2", "N3"]
section = "EA-160x14"
orientation = "x+y+"
design = { class = "leg" }

[[member]]
id = "D1"
nodes = ["N4", "N2"]
section = "EA-61x7"
orientation = "y-z+"
design = { class = "bracing", bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "D2"
nodes = ["N5", "N2"]
section = "EA-61x7"
orientation = "z-y-"
design = { class = "bracing", bolts = { diameter = "16 mm", count = 2 } }

[[connection]]
member = "L1"
joint = "N2"
kind = "continuous"
onto = "L2"
"#;

/// L5 (2026-11-20): the window bolts both diagonals of the wide-leg K-brace
/// directly onto the leg, no plate; its picture (the close-up before and after) for the user
/// guide.
#[test]
fn ends_moved_to_bolt_directly_in_the_window() {
    let path = copy_of("type-02.ohls", "layout-moved");
    let dir = path.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    // The example sections and an invented wide leg angle beside them.
    let sections = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
    )
    .unwrap();
    std::fs::write(
        dir.join("Sections/example-sections.ohlsec"),
        format!(
            "{sections}\n[[section]]\nid = \"EA-160x14\"\nname = \"Equal angle 160 × 14 (invented)\"\n\
             shape = \"equal_angle\"\nh = \"160 mm\"\nt = \"14 mm\"\nr = \"12 mm\"\n"
        ),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("Bolts")).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/bolts/example-bolts.ohlb"),
        dir.join("Bolts/example-bolts.ohlb"),
    )
    .unwrap();
    let tower = dir.join("wide-leg.ohls");
    std::fs::write(&tower, WIDE_K).unwrap();
    let mut h = harness(&tower);
    menu(&mut h, "Design", "Joint details");
    h.state_mut().selected = vec![Handle::Joint("N2".into())];
    h.run_steps(3);
    h.get_by_label("Lay out joint…").click();
    h.step();
    wait_preview(&mut h);
    {
        let w = h.state().layout_window.as_ref().unwrap();
        let p = w.preview.as_ref().unwrap();
        let n2 = p.run.joints.iter().find(|o| o.joint == "N2").unwrap();
        assert!(n2.written && n2.hand.is_empty(), "{:?}", n2.hand);
        assert!(n2.plan.plates.is_empty(), "{:?}", n2.plan);
        assert_eq!(n2.plan.ends.len(), 2, "{:?}", n2.plan);
        // Both onto the leg directly: D1 at the work point, D2 pulled back
        // clear of it (without the moves both went onto a plate).
        assert!(
            n2.plan
                .ends
                .iter()
                .all(|e| matches!(&e.onto, ohl_structure_link::Onto::Member(_))),
            "{:?}",
            n2.plan
        );
    }
    if ohl_core::env::var_os("OHL_RENDER_PNG").is_some() {
        h.get_by_label("Before and after:").scroll_to_me();
        h.run_steps(3);
        crate::tool_window_tests::window_shot(&mut h, "Lay out joints", "lay-out-joints-moved");
    }
    wait_apply(&mut h);
    h.get_all_by_label_contains("Apply (")
        .next()
        .unwrap()
        .click();
    h.run_steps(2);
    let f = h.state().file.as_ref().unwrap();
    assert!(f.structure().plates.is_empty(), "{}", f.text());
    std::fs::remove_dir_all(dir).ok();
}
