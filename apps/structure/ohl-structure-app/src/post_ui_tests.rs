// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Post insulators on a structure type in OHL-Structure (2026-11-15): a
//! string of the Strings table made a fixed post, rated and given its
//! clamp's exit angles, then hinged with a brace — each change one undo
//! step, the file raised to version 13 — and drawn in the 3D view (a thick
//! post, its base plate or its hinge, its clamp's exit direction). With
//! `OHL_RENDER_PNG` set, an image of the window.

use egui_kittest::kittest::Queryable;
use ohl_structure_edit::{StringChange, geometry};

use crate::app::{Sheet, StructureApp};
use crate::layout::Panel;
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

/// Whether a widget of the window shows `words` (a label or a value).
fn shows(h: &egui_kittest::Harness<'_, StructureApp>, words: &str) -> bool {
    h.query_all_by_label(words).next().is_some() || h.query_all_by_value(words).next().is_some()
}

fn text(h: &egui_kittest::Harness<'_, StructureApp>) -> String {
    h.state().file.as_ref().unwrap().text()
}

/// A change of string `label` as the Strings table makes it (one edit).
fn change(
    h: &mut egui_kittest::Harness<'static, StructureApp>,
    label: &str,
    c: StringChange,
) -> Result<(), String> {
    let app = h.state_mut();
    let insulators = app.insulators.clone();
    let f = app.file.as_mut().unwrap();
    let r = f.edit(&format!("string {label}"), |doc, t| {
        let g = geometry(t, &insulators);
        ohl_structure_edit::set_string(doc, t, &g, &insulators, label, c)
    });
    h.run();
    r
}

/// type-02's S1 (from A1 to W1, hanging 3.5 m) made an upright fixed post
/// in the Strings table, then hinged with a brace from a new point: the
/// table shows it, the 3D view draws it, every step undone in turn.
#[test]
fn a_type_string_is_made_a_post_drawn_and_undone() {
    let path = copy_of("type-02.ohls", "post-ui");
    let mut h = harness(&path);
    menu(&mut h, "Geometry", "Points & strings");
    h.state_mut()
        .sheets
        .insert(Panel::PointsInsulators, Sheet::Strings);
    h.run();
    let start = text(&h);
    assert!(start.contains("version = 8"));
    // Upright, 1.5 m: then a fixed post with its rating and exit angles.
    change(&mut h, "S1", StringChange::Length(1.5)).unwrap();
    change(&mut h, "S1", StringChange::Direction(180.0, 0.0)).unwrap();
    let upright = text(&h);
    change(&mut h, "S1", StringChange::Post(Some(false))).unwrap();
    change(&mut h, "S1", StringChange::CantileverLoad(Some(8000.0))).unwrap();
    change(&mut h, "S1", StringChange::ExitDirection(Some(90.0))).unwrap();
    let fixed = text(&h);
    assert!(
        fixed.contains("version = 13") && fixed.contains("post = true"),
        "{fixed}"
    );
    // The table shows it a fixed post, rated.
    assert!(shows(&h, "fixed"), "fixed");
    assert!(shows(&h, "rating"), "rating");
    // Drawn: from A1 to W1 1.5 m up, thick, its base plate and its exit.
    let (t, insulators) = {
        let f = h.state().file.as_ref().unwrap();
        (f.structure().clone(), h.state().insulators.clone())
    };
    let g = geometry(&t, &insulators);
    assert_eq!(g.posts.len(), 1);
    let p = &g.posts[0];
    assert_eq!((p.label.as_str(), p.hinged), ("S1", false));
    assert!((p.top[2] - p.base[2] - 1.5).abs() < 1e-9);
    let lines = crate::view::post_lines(&g);
    assert!(
        lines
            .iter()
            .any(|(l, w)| *w == crate::view::POST_WIDTH && l.as_slice() == [p.base, p.top]),
        "{lines:?}"
    );
    // The base plate (a closed square at the base) and the exit line.
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(lines.iter().any(|(l, _)| l.len() == 5));
    // Its exit to the right (+x), level.
    let exit = lines
        .iter()
        .find(|(l, _)| l.len() == 2 && l[0] == p.top && l[1] != p.base);
    let e = exit.expect("the exit direction").0[1];
    assert!(
        e[0] > p.top[0] + 0.1 && (e[2] - p.top[2]).abs() < 1e-9,
        "{e:?}"
    );
    // Its scene has more lines than the string alone.
    let (with_post, _, _) = crate::view::scene_for_tests(&g, &[], &[]);
    let mut plain = g.clone();
    plain.posts.clear();
    let (without, _, _) = crate::view::scene_for_tests(&plain, &[], &[]);
    assert!(with_post > without, "{with_post} {without}");
    shot(&mut h, "structure-post-fixed.png");
    // A brace to a fixed post's wire point: refused, the file unchanged.
    let a2 = {
        let f = h.state_mut().file.as_mut().unwrap();
        f.edit("point", |doc, t| {
            ohl_structure_edit::add_point(doc, t, [7.0, 0.0, 33.0])
        })
    };
    let with_point = text(&h);
    let brace = {
        let f = h.state_mut().file.as_mut().unwrap();
        f.edit("new string", |doc, t| {
            ohl_structure_edit::new_string(doc, t, &a2, None, 2.0, None)
        })
        .unwrap()
        .0
    };
    let before = text(&h);
    assert!(change(&mut h, &brace, StringChange::Wire("W1".into())).is_err());
    assert_eq!(text(&h), before);
    h.state_mut().undo();
    assert_eq!(text(&h), with_point);
    // Hinged: not a post, the brace from A2 to W1, then hinged.
    change(&mut h, "S1", StringChange::Post(None)).unwrap();
    {
        let f = h.state_mut().file.as_mut().unwrap();
        f.edit("new string", |doc, t| {
            ohl_structure_edit::new_string(doc, t, &a2, None, 3.0, Some("W1"))
        })
        .unwrap();
    }
    change(&mut h, "S1", StringChange::Post(Some(true))).unwrap();
    let hinged = text(&h);
    assert!(hinged.contains("hinged = true"), "{hinged}");
    assert!(shows(&h, "hinged"), "hinged");
    let t = h.state().file.as_ref().unwrap().structure().clone();
    let g = geometry(&t, &insulators);
    assert!(g.posts[0].hinged);
    let lines = crate::view::post_lines(&g);
    // The hinge: a circle at the base (25 points), no base plate.
    assert!(lines.iter().any(|(l, _)| l.len() == 25));
    assert!(lines.iter().all(|(l, _)| l.len() != 5));
    shot(&mut h, "structure-post-hinged.png");
    // Undone step by step: back to the file as opened.
    for _ in 0..3 {
        h.state_mut().undo();
    }
    assert_eq!(text(&h), with_point);
    h.state_mut().undo();
    assert_eq!(text(&h), fixed);
    for _ in 0..3 {
        h.state_mut().undo();
    }
    assert_eq!(text(&h), upright);
    h.state_mut().undo();
    h.state_mut().undo();
    assert_eq!(text(&h), start);
}
