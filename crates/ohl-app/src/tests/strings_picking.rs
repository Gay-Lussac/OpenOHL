// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: strings hung and edited, structures and jumpers added, picking
//! in every case drawn, spreads deleted.

use super::*;

/// Add string on a point: a structure point gets a new string straight
/// down from it (a new set), a wire point a string below it (a TAK); a
/// window opened or changed by a right-click opens at the mouse; a
/// right-click on nothing says so.
#[test]
fn add_string_hangs_from_the_point_clicked() {
    use crate::editors::Form;
    use crate::tools::Tool;
    let model = synthetic_project("add-string-point");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let (top, sets) = {
        let r = harness.state_mut().ready().unwrap();
        let s = r.model.line.structure("T2").unwrap();
        (s.points[0].clone(), s.sets().max().unwrap())
    };
    let point = |label: &str| Selection::Point {
        structure: "T2".into(),
        label: label.into(),
    };
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(egui::pos2(300.0, 300.0)));
    harness.run();
    use_tool(&mut harness, Tool::AddString, point(&top.label), None);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::String(f)) = &r.editor else {
            panic!("{:?} {:?}", r.editor, r.status);
        };
        assert_eq!(r.popup_at, egui::pos2(300.0, 300.0));
        assert_eq!(f.strings.len(), 1);
        assert_eq!(f.strings[0].top, top.label);
        let w = &f.wires[0];
        assert_eq!(w.set, Some(sets + 1));
        let at = f.points.iter().find(|p| p.label == top.label).unwrap().at;
        assert!((at[2] - w.at[2] - 3.0).abs() < 1e-9 && (at[0] - w.at[0]).abs() < 1e-9);
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let wire = {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let s = r.model.line.structure("T2").unwrap();
        let w = s.wire_at_set(sets + 1).expect("the new set");
        let x = s.strings_to(&w.label).next().unwrap();
        assert_eq!(x.top, top.label);
        w.label.clone()
    };
    // From the new wire point: a string below it, the next set.
    use_tool(&mut harness, Tool::AddString, point(&wire), None);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::String(f)) = &r.editor else {
            panic!("{:?} {:?}", r.editor, r.status);
        };
        let x = f
            .strings
            .iter()
            .find(|x| x.top == wire)
            .expect("hangs from it");
        let w = f.wires.iter().find(|w| w.label == x.wire).unwrap();
        assert_eq!(w.set, Some(sets + 2));
    }
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    // On the structure itself: from one of its points (no new point).
    use_tool(
        &mut harness,
        Tool::AddString,
        Selection::Structure("T2".into()),
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::String(f)) = &r.editor else {
            panic!("{:?} {:?}", r.editor, r.status);
        };
        let s = r.model.line.structure("T2").unwrap();
        assert!(s.point(&f.strings[0].top).is_some(), "{:?}", f.strings[0]);
        assert!(f.points.iter().all(|p| s.point(&p.label).is_some()));
    }
    // Another object right-clicked while it is open: the window follows
    // the mouse.
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(egui::pos2(700.0, 500.0)));
    harness.run();
    use_tool(
        &mut harness,
        Tool::Edit,
        Selection::Span {
            section: "S1".into(),
            span: 0,
        },
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(matches!(r.editor, Some(Form::Section(_))), "{:?}", r.editor);
        // At the mouse; a window taller than the room below it moved
        // up just enough to show whole.
        assert_eq!(r.popup_at.x, 700.0);
        assert!((480.0..=500.0).contains(&r.popup_at.y), "{:?}", r.popup_at);
    }
    use_tool(&mut harness, Tool::Edit, Selection::None, None);
    let r = harness.state_mut().ready().unwrap();
    assert!(
        r.status
            .as_ref()
            .is_some_and(|(t, _)| t.contains("nothing here")),
        "{:?}",
        r.status
    );
}

/// A string hung from another point keeps its length: its wire point (and
/// what hangs below) moves with it; a V's leg keeps both legs' lengths,
/// refused where they cannot meet; a string cannot hang below itself.
#[test]
fn a_string_hung_elsewhere_keeps_its_length() {
    use crate::editors::{PointRow, Preset, StringForm};
    let model = synthetic();
    let dist = |a: [f64; 3], b: [f64; 3]| (0..3).map(|i| (b[i] - a[i]).powi(2)).sum::<f64>().sqrt();
    let at = |f: &StringForm, l: &str| {
        f.points
            .iter()
            .find(|p| p.label == l)
            .map(|p| p.at)
            .or_else(|| f.wires.iter().find(|w| w.label == l).map(|w| w.at))
            .unwrap()
    };
    let point = |f: &mut StringForm, label: &str, p: [f64; 3]| {
        f.points.push(PointRow {
            label: label.into(),
            at: p,
            description: String::new(),
        });
    };
    // An I-string with a string below it (V + string): moved, all move.
    let mut f = StringForm::new_on(&model, "T2").unwrap();
    f.preset(Preset::VAndString, [0.0, 0.0, 20.0], &model);
    point(&mut f, "X", [1.0, 0.5, 25.0]);
    let below = f
        .strings
        .iter()
        .position(|x| f.wires.iter().any(|w| w.label == x.top))
        .unwrap();
    let (tip, low) = (f.strings[below].top.clone(), f.strings[below].wire.clone());
    let (l0, low0) = (dist(at(&f, &tip), at(&f, &low)), at(&f, &low));
    f.move_top(below, "X").unwrap();
    assert_eq!(f.strings[below].top, "X");
    assert!((dist(at(&f, "X"), at(&f, &low)) - l0).abs() < 1e-9);
    assert!(dist(at(&f, &low), low0) > 1.0, "it moved with its top");
    // A V's leg: both lengths kept, the tip where they meet.
    let legs: Vec<usize> = (0..f.strings.len())
        .filter(|&i| f.strings[i].wire == tip)
        .collect();
    let lens: Vec<f64> = legs
        .iter()
        .map(|&i| dist(at(&f, &f.strings[i].top), at(&f, &tip)))
        .collect();
    let a = at(&f, &f.strings[legs[0]].top);
    point(&mut f, "Y", [a[0] - 0.5, a[1], a[2]]);
    f.move_top(legs[0], "Y").unwrap();
    for (n, &i) in legs.iter().enumerate() {
        let l = dist(at(&f, &f.strings[i].top), at(&f, &tip));
        assert!((l - lens[n]).abs() < 1e-9, "{l} {}", lens[n]);
    }
    // Too far for the two legs: refused, nothing changes.
    point(&mut f, "FAR", [40.0, 0.0, 20.0]);
    let before = f.clone();
    let why = f.move_top(legs[0], "FAR").unwrap_err();
    assert!(why.contains("cannot meet"), "{why}");
    assert_eq!(f, before);
    // Not from its own wire point, nor from below it.
    assert!(f.move_top(legs[0], &tip).is_err());
    let s = f.strings.iter().position(|x| x.wire == low).unwrap();
    f.move_top(s, &tip).unwrap();
    assert!(f.move_top(legs[0], &low).is_err());
}

/// Edit on a structure opens its structure screen (as Strings and points…):
/// one without a type in the structure editor; its placement window is
/// Placement… in Properties.
#[test]
fn edit_on_a_structure_opens_its_pylon() {
    use crate::editors::Form;
    use crate::tools::Tool;
    let model = synthetic_project("edit-pylon");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let id = {
        let r = harness.state_mut().ready().unwrap();
        let s = r
            .model
            .line
            .structures
            .iter()
            .find(|s| s.structure_type.is_none());
        s.expect("a structure without a type").id.clone()
    };
    use_tool(
        &mut harness,
        Tool::Edit,
        Selection::Structure(id.clone()),
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.editor);
        assert_eq!(r.placed.as_ref().map(|p| p.id.as_str()), Some(id.as_str()));
    }
    let r = harness.state_mut().ready().unwrap();
    r.perform(crate::edit::Action::EditPlacement { id: id.clone() });
    assert!(matches!(&r.editor, Some(Form::Structure(f)) if f.id == id));
}

/// A tool window opens next to the click even where a window of its id
/// was left elsewhere in an earlier run (window places are remembered
/// between runs): here far off the screen.
#[test]
fn a_tool_window_opens_at_the_click_not_where_it_was_before() {
    let model = synthetic_project("window-place");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    // An earlier run left the first editor window far off the screen.
    let ctx = harness.ctx.clone();
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        egui::Window::new("stale")
            .id(egui::Id::new(("editor".to_owned(), 1_u64)))
            .constrain(false)
            .current_pos(egui::pos2(3000.0, 2000.0))
            .show(ui.ctx(), |ui| ui.label("stale"));
    });
    out.textures_delta.clear();
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(egui::pos2(500.0, 400.0)));
    harness.run();
    use_tool(
        &mut harness,
        crate::tools::Tool::Edit,
        Selection::Span {
            section: "S1".into(),
            span: 0,
        },
        None,
    );
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.editor.is_some());
    let state = egui::AreaState::load(&harness.ctx, egui::Id::new(("editor".to_owned(), 1_u64)))
        .expect("the window");
    let at = state.pivot_pos.unwrap();
    // Next to the click (moved up as much as it reaches past the bottom).
    assert!(
        (at.x - 516.0).abs() < 1.0 && (396.0..=417.0).contains(&at.y),
        "{at:?}"
    );
}

/// A string's length typed: kept, with every other string's — a V's leg
/// moves the point where the legs meet, the string below keeps its length
/// and moves along; a length the legs cannot meet with is refused.
#[test]
fn a_string_length_typed_keeps_the_others() {
    use crate::editors::{Preset, StringForm};
    let model = synthetic();
    let dist = |a: [f64; 3], b: [f64; 3]| (0..3).map(|i| (b[i] - a[i]).powi(2)).sum::<f64>().sqrt();
    let at = |f: &StringForm, l: &str| {
        f.points
            .iter()
            .find(|p| p.label == l)
            .map(|p| p.at)
            .or_else(|| f.wires.iter().find(|w| w.label == l).map(|w| w.at))
            .unwrap()
    };
    let len = |f: &StringForm, k: usize| dist(at(f, &f.strings[k].top), at(f, &f.strings[k].wire));
    let mut f = StringForm::new_on(&model, "T2").unwrap();
    f.preset(Preset::VAndString, [0.0, 0.0, 20.0], &model);
    let below = f
        .strings
        .iter()
        .position(|x| f.wires.iter().any(|w| w.label == x.top))
        .unwrap();
    let tip = f.strings[below].top.clone();
    let legs: Vec<usize> = (0..f.strings.len())
        .filter(|&i| f.strings[i].wire == tip)
        .collect();
    let (l0, l1, lb) = (len(&f, legs[0]), len(&f, legs[1]), len(&f, below));
    let tip0 = at(&f, &tip);
    f.set_length(legs[0], l0 + 0.5).unwrap();
    assert!((len(&f, legs[0]) - (l0 + 0.5)).abs() < 1e-9);
    assert!(
        (len(&f, legs[1]) - l1).abs() < 1e-9,
        "the other leg keeps its length"
    );
    assert!(
        (len(&f, below) - lb).abs() < 1e-9,
        "the string below keeps its length"
    );
    assert!(dist(at(&f, &tip), tip0) > 0.1, "where they meet moved");
    // Too short to reach the other leg: refused, nothing changes.
    let before = f.clone();
    let apart = dist(
        at(&f, &f.strings[legs[0]].top),
        at(&f, &f.strings[legs[1]].top),
    );
    let why = f.set_length(legs[0], (apart - l1) * 0.5).unwrap_err();
    assert!(why.contains("cannot meet"), "{why}");
    assert_eq!(f, before);
    // The string below (an I): its wire point along it.
    f.set_length(below, lb + 1.0).unwrap();
    assert!((len(&f, below) - (lb + 1.0)).abs() < 1e-9);
    assert!((len(&f, legs[0]) - (l0 + 0.5)).abs() < 1e-9);
}

/// Ends at: any other wire point of the structure (strings chain: a V
/// under a V), keeping its length — a second leg makes a V where both
/// lengths meet; a third leg, a loop or the wire point of another set
/// pulled in.
#[test]
fn a_string_ends_at_any_other_wire_point() {
    use crate::editors::{Preset, StringForm, StringRow, WireRow};
    let model = synthetic();
    let dist = |a: [f64; 3], b: [f64; 3]| (0..3).map(|i| (b[i] - a[i]).powi(2)).sum::<f64>().sqrt();
    let at = |f: &StringForm, l: &str| {
        f.points
            .iter()
            .find(|p| p.label == l)
            .map(|p| p.at)
            .or_else(|| f.wires.iter().find(|w| w.label == l).map(|w| w.at))
            .unwrap()
    };
    let len = |f: &StringForm, l: &str| {
        let x = f.strings.iter().find(|x| x.label == l).unwrap();
        dist(at(f, &x.top), at(f, &x.wire))
    };
    let k = |f: &StringForm, l: &str| f.strings.iter().position(|x| x.label == l).unwrap();
    // An upper V (legs to its tip, no set yet).
    let mut f = StringForm::new_on(&model, "T2").unwrap();
    f.preset(Preset::V, [0.0, 0.0, 20.0], &model);
    let tip = f.strings[0].wire.clone();
    let side = f.strings[1].top.clone();
    // Two more strings, each with its own wire point: one from the tip,
    // one from a top of the upper V.
    let add = |f: &mut StringForm, label: &str, top: &str, d: [f64; 3]| {
        let t = at(f, top);
        let wire = format!("{label}-W");
        f.wires.push(WireRow {
            description: String::new(),
            label: wire.clone(),
            at: [t[0] + d[0], t[1] + d[1], t[2] + d[2]],
            set: None,
            weight: 0.0,
            dead_end: false,
            dead_end_was: false,
        });
        f.strings.push(StringRow {
            label: label.into(),
            top: top.into(),
            wire,
            ..Default::default()
        });
    };
    add(&mut f, "X1", &tip, [0.0, 0.0, -2.0]);
    add(&mut f, "X2", &side, [-1.0, 0.0, -4.0]);
    let (l1, l2) = (len(&f, "X1"), len(&f, "X2"));
    // X2 ends at X1's wire point: a lower V, both keeping their lengths.
    let low = "X1-W".to_owned();
    assert!(f.end_choices(&model, k(&f, "X2")).contains(&low));
    f.end_at(&model, k(&f, "X2"), &low).unwrap();
    assert_eq!(f.strings[k(&f, "X2")].wire, low);
    assert!((len(&f, "X1") - l1).abs() < 1e-9 && (len(&f, "X2") - l2).abs() < 1e-9);
    assert!(
        f.wires.iter().all(|w| w.label != "X2-W"),
        "its own wire point went"
    );
    // A third leg there: refused, nothing changes.
    add(&mut f, "X3", &side, [1.0, 0.0, -3.0]);
    let before = f.clone();
    let why = f.end_at(&model, k(&f, "X3"), &low).unwrap_err();
    assert!(why.contains("two strings meet there"), "{why}");
    assert_eq!(f, before);
    // A leg of the upper V cannot end below itself (a loop).
    let leg = f.strings[0].label.clone();
    assert!(!f.end_choices(&model, k(&f, &leg)).contains(&low));
    assert!(f.end_at(&model, k(&f, &leg), &low).is_err());
    // X1 alone holds the lower V's tip: sent to end at X3's wire point,
    // what hangs from the tip (nothing here) and X2 stay; with a string
    // hanging below the tip, it is hung from the new end, lengths kept.
    add(&mut f, "Y", &low, [0.0, 0.0, -1.5]);
    let ly = len(&f, "Y");
    let x2 = k(&f, "X2");
    f.strings.remove(x2);
    let x3w = f.strings[k(&f, "X3")].wire.clone();
    f.end_at(&model, k(&f, "X1"), &x3w).unwrap();
    assert_eq!(f.strings[k(&f, "Y")].top, x3w, "hung from the new end");
    assert!((len(&f, "Y") - ly).abs() < 1e-9, "its length kept");
    assert!((len(&f, "X1") - l1).abs() < 1e-9);
    assert!(f.wires.iter().all(|w| w.label != low), "the old end went");
    // A wire point of another set of the structure: taken into the window.
    let s = model.line.structure("T2").unwrap();
    let other = s
        .wires
        .iter()
        .find(|w| w.set.is_some())
        .unwrap()
        .label
        .clone();
    assert!(f.end_choices(&model, k(&f, "X3")).contains(&other));
    let r = f.end_at(&model, k(&f, "X3"), &other);
    assert!(
        f.wires.iter().any(|w| w.label == other) || r.is_err(),
        "{r:?}"
    );
    if r.is_ok() {
        assert!(f.opened.1.contains(&other));
    }
}

/// A V under a V built in the string window is applied and solved.
#[test]
fn a_v_under_a_v_is_applied() {
    use crate::editors::{Form, PointRow, StringForm, StringRow, WireRow};
    let model = synthetic_project("v-under-v");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let mut f = StringForm::of(&r.model, "T2", 1).unwrap();
        let x0 = f.strings[0].clone();
        let tip = x0.wire.clone();
        // Two new points on the structure, either side of the first top.
        let top0 = f.points.iter().find(|p| p.label == x0.top).unwrap().at;
        let tops = ["PA".to_owned(), "PB".to_owned()];
        for (label, dx) in [("PA", 2.5), ("PB", -2.5)] {
            f.points.push(PointRow {
                label: label.into(),
                at: [top0[0] + dx, top0[1], top0[2]],
                description: String::new(),
            });
        }
        let at = |f: &StringForm, l: &str| {
            f.points
                .iter()
                .find(|p| p.label == l)
                .map(|p| p.at)
                .or_else(|| f.wires.iter().find(|w| w.label == l).map(|w| w.at))
                .unwrap()
        };
        let add = |f: &mut StringForm, label: &str, top: &str, end: [f64; 3]| {
            let wire = format!("{label}-W");
            f.wires.push(WireRow {
                description: String::new(),
                label: wire.clone(),
                at: end,
                set: None,
                weight: 0.0,
                dead_end: false,
                dead_end_was: false,
            });
            f.strings.push(StringRow {
                label: label.into(),
                top: top.into(),
                wire,
                weight: 300.0,
                ..Default::default()
            });
        };
        // Upper V: a second leg to the set's wire point, from another
        // point (its length: to where the first ends).
        let w = at(&f, &tip);
        add(&mut f, "UV", &tops[0], w);
        let n = f.strings.len() - 1;
        f.end_at(&r.model, n, &tip).unwrap();
        // Lower V: from the upper tip and from another point, meeting
        // below; the set moved down to it.
        let t = at(&f, &tip);
        add(&mut f, "L1", &tip, [t[0], t[1], t[2] - 2.0]);
        add(&mut f, "L2", &tops[1], [t[0], t[1], t[2] - 2.0]);
        let k2 = f.strings.len() - 1;
        f.end_at(&r.model, k2, "L1-W").unwrap();
        for w in &mut f.wires {
            w.set = if w.label == "L1-W" {
                Some(1)
            } else if w.label == tip {
                None
            } else {
                w.set
            };
        }
        r.editor = Some(Form::String(f));
    }
    harness.run();
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.editor.is_none(), "{:?}", r.status);
    let s = r.model.line.structure("T2").unwrap();
    assert_eq!(s.strings_to("L1-W").count(), 2);
    assert!(r.model.shape(&r.model.line.sections[0].id, 0).is_some());
}

/// Chains of strings of any length end in a V — four strings on one side,
/// five on the other, meeting at the set's wire point — applied, and
/// solved in every weather case (high wind: by relaxation, then Newton).
#[test]
fn chains_of_strings_end_in_a_v() {
    use crate::editors::{Form, PointRow, StringForm, StringRow, WireRow};
    let model = synthetic_project("chains");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let mut f = StringForm::of(&r.model, "T2", 1).unwrap();
        let x0 = f.strings[0].clone();
        let top0 = f.points.iter().find(|p| p.label == x0.top).unwrap().at;
        let w0 = f.wires.iter().find(|w| w.label == x0.wire).unwrap().at;
        f.strings.clear();
        let n: [usize; 2] = [4, 5];
        // Two chains from points 3 m either side, each hanging straight
        // from its point to the tip's height, then the last strings meet.
        let depth = top0[2] - w0[2] + 3.0;
        let mut ends = Vec::new();
        for (side, dx, count) in [("A", 3.0, n[0]), ("B", -3.0, n[1])] {
            let label = format!("P{side}");
            let p = [top0[0] + dx, top0[1], top0[2]];
            f.points.push(PointRow {
                label: label.clone(),
                at: p,
                description: String::new(),
            });
            let mut top = label;
            for i in 0..count {
                let wire = format!("{side}{i}-W");
                let fr = (i + 1) as f64 / count as f64;
                // Straight from the point towards the tip's spot.
                let at = [p[0] - dx * fr, p[1], p[2] - depth * fr];
                f.wires.push(WireRow {
                    description: String::new(),
                    label: wire.clone(),
                    at,
                    set: None,
                    weight: 0.0,
                    dead_end: false,
                    dead_end_was: false,
                });
                f.strings.push(StringRow {
                    label: format!("{side}{i}"),
                    top: top.clone(),
                    wire: wire.clone(),
                    weight: 100.0,
                    ..Default::default()
                });
                top = wire;
            }
            ends.push(top);
        }
        let k = f
            .strings
            .iter()
            .position(|x| x.label == format!("B{}", n[1] - 1))
            .unwrap();
        f.end_at(&r.model, k, &ends[0]).unwrap();
        f.wires.retain(|w| w.label != x0.wire);
        for w in &mut f.wires {
            if w.label == ends[0] {
                w.set = Some(1);
            }
        }
        r.editor = Some(Form::String(f));
    }
    harness.run();
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.editor.is_none(), "{:?}", r.status);
    let s = r.model.line.structure("T2").unwrap();
    assert_eq!(s.strings_to("A3-W").count(), 2);
    for c in 0..r.model.cases.len() {
        r.model.ensure_solved(c);
        assert!(
            r.model.cases[c].failed.is_empty(),
            "case {c}: {:?}",
            r.model.cases[c].failed
        );
    }
}

/// The new string in the open string window given 900 N.
pub(super) fn with_weight(h: &mut Harness<'_, App>) {
    let r = h.state_mut().ready().unwrap();
    let Some(crate::editors::Form::String(f)) = &mut r.editor else {
        panic!("{:?}", r.editor);
    };
    let row = f.strings.last_mut().expect("the new string");
    assert_eq!(row.weight, 0.0);
    row.weight = 900.0;
}

/// A string added with a weight gets a library label in the same command:
/// its entry written into the project's library (same values: the same
/// entry for the next one), one undo taking both back. One added without a
/// weight (0 N) stays unlabelled: no 0 N entry (2026-10-21).
#[test]
fn a_new_string_is_labelled_from_the_library() {
    use crate::tools::Tool;
    let model = synthetic_project("string-library");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let top = {
        let r = harness.state_mut().ready().unwrap();
        r.model.line.structure("T2").unwrap().points[0]
            .label
            .clone()
    };
    let point = Selection::Point {
        structure: "T2".into(),
        label: top.clone(),
    };
    // Without a weight first: no label, no entry.
    let before = {
        let r = harness.state_mut().ready().unwrap();
        r.model.types.as_ref().map_or(0, |t| t.1.iter().count())
    };
    use_tool(&mut harness, Tool::AddString, point.clone(), None);
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let s = r.model.stored_line().unwrap().structure("T2").unwrap();
        let x = s
            .strings
            .iter()
            .rfind(|x| x.top == top)
            .expect("the new string");
        assert_eq!(x.weight, 0.0);
        assert!(x.insulator.is_none(), "{:?}", x.insulator);
        let lib = &r.model.types.as_ref().expect("libraries").1;
        assert!(
            lib.iter().all(|a| a.weight.si() > 0.0),
            "no 0 N entry ({before} entries before)"
        );
        r.perform(crate::edit::Action::Undo);
    }
    use_tool(&mut harness, Tool::AddString, point.clone(), None);
    with_weight(&mut harness);
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let (label, entries) = {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let s = r
            .model
            .stored_line()
            .unwrap()
            .structure("T2")
            .unwrap()
            .clone();
        let x = s
            .strings
            .iter()
            .rfind(|x| x.top == top)
            .expect("the new string")
            .clone();
        let label = x.insulator.clone().expect("a library label");
        let lib = &r.model.types.as_ref().expect("libraries").1;
        assert!(lib.get(&label).is_some(), "{label} in the library");
        // Every own string of T2 with a weight is labelled.
        assert!(
            s.strings
                .iter()
                .all(|x| x.insulator.is_some() || x.weight == 0.0)
        );
        (label, lib.iter().count())
    };
    // The same again: the same entry.
    use_tool(&mut harness, Tool::AddString, point, None);
    with_weight(&mut harness);
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        let lib = &r.model.types.as_ref().unwrap().1;
        assert_eq!(lib.iter().count(), entries);
        let s = r.model.stored_line().unwrap().structure("T2").unwrap();
        assert_eq!(
            s.strings
                .iter()
                .filter(|x| x.insulator.as_deref() == Some(&label))
                .count(),
            2
        );
    }
    // One undo: the second string goes, with its label.
    let n = {
        let r = harness.state_mut().ready().unwrap();
        r.model
            .stored_line()
            .unwrap()
            .structure("T2")
            .unwrap()
            .strings
            .len()
    };
    let r = harness.state_mut().ready().unwrap();
    r.perform(crate::edit::Action::Undo);
    let s = r.model.stored_line().unwrap().structure("T2").unwrap();
    assert_eq!(s.strings.len(), n - 1);
}

/// The string library as a table: on opening, every own string is
/// labelled; a row's "used" counts its strings; a weight or length typed
/// goes to all of them (their wire points moved); a row added; a used row
/// deleted (confirmed) leaves its strings their values under a matching
/// or new label.
#[test]
fn the_string_library_is_a_table() {
    use crate::tables::TableKind;
    let k = TableKind::Insulators;
    let model = synthetic_project("string-library-table");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().ready().unwrap().catalogue_strings();
    let used = |h: &mut Harness<'_, App>, id: &str| -> Vec<(String, String)> {
        let r = h.state_mut().ready().unwrap();
        r.model
            .line
            .structures
            .iter()
            .flat_map(|s| {
                s.strings
                    .iter()
                    .filter(|x| x.insulator.as_deref() == Some(id))
                    .map(|x| (s.id.clone(), x.label.clone()))
            })
            .collect()
    };
    let (id, rows) = {
        let r = harness.state_mut().ready().unwrap();
        let stored = r.model.stored_line().unwrap();
        assert!(
            stored
                .structures
                .iter()
                .flat_map(|s| &s.strings)
                .all(|x| x.insulator.is_some() || x.weight == 0.0),
            "every own string with a weight labelled (weightless ones keep the estimate)"
        );
        let rows = k.rows(&r.model, r.case);
        let total: usize = rows
            .iter()
            .map(|x| x.cells[k.column("used")].parse::<usize>().unwrap())
            .sum();
        let strings: usize = r
            .model
            .line
            .structures
            .iter()
            .map(|s| s.strings.iter().filter(|x| x.insulator.is_some()).count())
            .sum();
        assert_eq!(total, strings);
        // A suspension entry in use.
        let row = rows
            .iter()
            .find(|x| x.cells[k.column("kind")] == "suspension" && x.cells[k.column("used")] != "0")
            .expect("a suspension entry in use");
        (row.key.clone(), rows.len())
    };
    let users = used(&mut harness, &id);
    assert_eq!(
        k.rows(&harness.state_mut().ready().unwrap().model, 0)
            .iter()
            .find(|x| x.key == id)
            .unwrap()
            .cells[k.column("used")],
        users.len().to_string()
    );
    // Weight and length typed: every string with the label takes them.
    let old_len = {
        let r = harness.state_mut().ready().unwrap();
        let (s, l) = &users[0];
        let st = r.model.line.structure(s).unwrap();
        st.string_length(st.string(l).unwrap()).unwrap()
    };
    set_cell(&mut harness, k, &id, "weight", "1234");
    set_cell(
        &mut harness,
        k,
        &id,
        "length",
        &format!("{:.3}", old_len + 0.5),
    );
    apply_table(&mut harness, k, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        for (s, l) in &users {
            let st = r.model.line.structure(s).unwrap();
            let x = st.string(l).unwrap();
            assert!((x.weight - 1234.0).abs() < 1e-6, "{s} {l}: {}", x.weight);
            let len = st.string_length(x).unwrap();
            assert!((len - (old_len + 0.5)).abs() < 2e-3, "{s} {l}: {len}");
        }
    }
    // A row added.
    {
        let r = harness.state_mut().ready().unwrap();
        let mut row = k.new_row(&r.model, r.case, &[]);
        row[0] = "glass-I-380".into();
        r.tables.entry(k).or_default().added.push(row);
    }
    apply_table(&mut harness, k, false).unwrap();
    assert_eq!(
        k.rows(&harness.state_mut().ready().unwrap().model, 0).len(),
        rows + 1
    );
    // The used row deleted: asked first; its strings keep their values.
    harness
        .state_mut()
        .ready()
        .unwrap()
        .tables
        .entry(k)
        .or_default()
        .deleted
        .insert(id.clone());
    assert!(apply_table(&mut harness, k, false).is_err(), "asks first");
    apply_table(&mut harness, k, true).unwrap();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.types.as_ref().unwrap().1.get(&id).is_none());
    for (s, l) in &users {
        let x = r.model.line.structure(s).unwrap().string(l).unwrap();
        assert!((x.weight - 1234.0).abs() < 1e-6);
        assert!(
            x.insulator.as_deref().is_some_and(|i| i != id),
            "{:?}",
            x.insulator
        );
    }
}

/// Structures › String shapes (roadmap O6): the volumes of the string
/// library's assemblies as a table, drawn as a page; a shape added
/// (INVENTED sizes) goes into its assembly's file (version 4), a kind and
/// size typed change it, each Apply one undoable change; the freedoms and
/// several strings typed in the String library; a cell that does not read
/// refused; a row deleted takes the shape out; the 3D view draws the
/// shapes when the display option is on.
#[test]
fn the_string_shapes_are_a_table() {
    use crate::tables::TableKind;
    let k = TableKind::StringShapes;
    let model = synthetic_project("string-shapes-table");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().ready().unwrap().catalogue_strings();
    harness.state_mut().ready().unwrap().open_page = Some((crate::app::Tab::Table(k), None));
    harness.run();
    harness.run();
    let shapes_of = |h: &mut Harness<'_, App>, id: &str| -> Vec<ohl_structures::StringShape> {
        let r = h.state_mut().ready().unwrap();
        r.model
            .types
            .as_ref()
            .unwrap()
            .1
            .get(id)
            .unwrap()
            .volumes
            .shapes
            .clone()
    };
    // The suspension entry in use, without shapes yet.
    let id = {
        let r = harness.state_mut().ready().unwrap();
        assert!(k.rows(&r.model, r.case).is_empty());
        let i = TableKind::Insulators;
        i.rows(&r.model, r.case)
            .into_iter()
            .find(|x| x.cells[i.column("kind")] == "suspension" && x.cells[i.column("used")] != "0")
            .expect("a suspension entry in use")
            .key
    };
    // A clamp added at the wire point.
    {
        let r = harness.state_mut().ready().unwrap();
        let mut row = k.new_row(&r.model, r.case, &[]);
        let col = |n: &str| k.column(n);
        row[col("library label")] = id.clone();
        row[col("shape")] = "clamp".into();
        row[col("kind")] = "box".into();
        row[col("state")] = "electrified".into();
        row[col("at")] = "1".into();
        row[col("diameter")] = String::new();
        row[col("width")] = "0.15".into();
        row[col("depth")] = "0.3".into();
        row[col("length")] = "0.1".into();
        r.tables.entry(k).or_default().added.push(row);
    }
    apply_table(&mut harness, k, false).unwrap();
    let shapes = shapes_of(&mut harness, &id);
    assert_eq!(shapes.len(), 1);
    assert_eq!(shapes[0].kind, ohl_structures::ShapeKind::Box);
    assert_eq!(shapes[0].at, ohl_structures::Along::Fraction(1.0));
    {
        let r = harness.state_mut().ready().unwrap();
        let path = r
            .model
            .types
            .as_ref()
            .unwrap()
            .1
            .path(&id)
            .unwrap()
            .to_path_buf();
        let text = std::fs::read_to_string(path).unwrap();
        assert!(
            text.contains("version = 4") && text.contains("[[shape]]"),
            "{text}"
        );
    }
    let key = k.rows(&harness.state_mut().ready().unwrap().model, 0)[0]
        .key
        .clone();
    // Typed: a sphere of 0.3 m at 0.2 m from the top.
    set_cell(&mut harness, k, &key, "kind", "sphere");
    set_cell(&mut harness, k, &key, "diameter", "0.3");
    set_cell(&mut harness, k, &key, "at", "0.2 m");
    apply_table(&mut harness, k, false).unwrap();
    let shapes = shapes_of(&mut harness, &id);
    assert_eq!(shapes[0].kind, ohl_structures::ShapeKind::Sphere);
    assert_eq!(shapes[0].at, ohl_structures::Along::Metres(0.2));
    assert!((shapes[0].diameter - 0.3).abs() < 1e-12 && shapes[0].width == 0.0);
    // One undo: the box again.
    harness
        .state_mut()
        .ready()
        .unwrap()
        .perform(crate::edit::Action::Undo);
    assert_eq!(
        shapes_of(&mut harness, &id)[0].kind,
        ohl_structures::ShapeKind::Box
    );
    // A kind that does not read: refused, nothing written.
    set_cell(&mut harness, k, &key, "kind", "torus");
    let e = apply_table(&mut harness, k, false).unwrap_err();
    assert!(e.contains("sphere, cylinder, box or cone"), "{e}");
    harness.state_mut().ready().unwrap().tables.remove(&k);
    // The freedoms and two strings in the String library.
    let i = TableKind::Insulators;
    set_cell(&mut harness, i, &id, "twist", "blocked");
    set_cell(&mut harness, i, &id, "strings", "2");
    set_cell(&mut harness, i, &id, "spacing", "0.45");
    apply_table(&mut harness, i, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let v = &r.model.types.as_ref().unwrap().1.get(&id).unwrap().volumes;
        assert!(!v.twist.is_free() && v.swing_across.is_free());
        assert_eq!((v.strings, v.spacing, v.shapes.len()), (2, 0.45, 1));
        let row = i
            .rows(&r.model, r.case)
            .into_iter()
            .find(|x| x.key == id)
            .unwrap();
        assert_eq!(row.cells[i.column("shapes")], "1");
    }
    // Two strings without a spacing: refused.
    set_cell(&mut harness, i, &id, "spacing", "");
    assert!(apply_table(&mut harness, i, false).is_err());
    harness.state_mut().ready().unwrap().tables.remove(&i);
    // Drawn in 3D with the display option: a filled, near primitive more.
    let count = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        crate::scene::prims(&r.model, crate::scene::Layer::Structures, r.case)
            .iter()
            .filter(|p| p.filled && p.near)
            .count()
    };
    let before = count(&mut harness);
    harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .section_display
        .string_volumes = true;
    assert!(count(&mut harness) > before, "the shapes drawn");
    // The row deleted: no shape left.
    harness
        .state_mut()
        .ready()
        .unwrap()
        .tables
        .entry(k)
        .or_default()
        .deleted
        .insert(key);
    apply_table(&mut harness, k, false).unwrap();
    assert!(shapes_of(&mut harness, &id).is_empty());
    harness.run();
}

/// Add structure on an alignment point without a structure: the window
/// puts it on that point (tied); its list names the points as the views
/// do; Display options write the points' names.
#[test]
fn a_structure_is_added_on_the_point_clicked() {
    use crate::annotations::{View, object_notes};
    use crate::editors::Form;
    use crate::tools::Tool;
    let model = synthetic_project_with("structure-on-point", |l| {
        let a = l.alignments[0].id.clone();
        let p0 = l.points_of(&l.alignments[0])[0].position;
        let p1 = l.points_of(&l.alignments[0])[1].position;
        let mid = [0, 1, 2].map(|k| 0.5 * (p0[k] + p1[k]));
        l.alignment_points
            .push(ohl_line::AlignmentPoint::free("PX", mid));
        let al = l.alignments.iter_mut().find(|x| x.id == a).unwrap();
        al.points.insert(1, "PX".into());
    });
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    use_tool(
        &mut harness,
        Tool::AddStructure,
        Selection::AlignmentPoint { id: "PX".into() },
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Structure(f)) = &r.editor else {
            panic!("{:?}", r.editor);
        };
        let (a, k) = f.on_alignment.clone().expect("on the point");
        let listed = crate::editors::alignment_points(&r.model.line);
        let p = listed.iter().find(|p| p.0 == a && p.1 == k).unwrap();
        assert_eq!(p.4, "PX");
        let at = r.model.line.alignment_point("PX").unwrap().position;
        assert!((f.position[0] - at[0]).abs() < 1e-6 && (f.position[1] - at[1]).abs() < 1e-6);
    }
    let r = harness.state_mut().ready().unwrap();
    r.model.section_display.alignment_point_label[View::Plan as usize] = true;
    let notes = object_notes(&r.model, 0, View::Plan, None);
    assert!(notes.iter().any(|n| n.text == "PX"));
}

/// Add jumper: its first end, then the suspension strings it passes
/// through (to wire points without a set) in order, then its other end —
/// a jumper with a clamp on each, given by the lengths between its clamps
/// (3 % over the straight distances), its window opened to type them; a
/// piece typed sets its length and the clamps' places.
#[test]
fn a_jumper_is_laid_through_suspension_strings() {
    use crate::editors::{Form, jumper_pieces, set_jumper_pieces};
    use crate::tools::Tool;
    let model = synthetic_project_with("jumper-through", |l| {
        let j = l.jumpers.remove(0);
        let (a, b) = (
            l.wire_point(j.from.point().unwrap()).unwrap().position,
            l.wire_point(j.to.point().unwrap()).unwrap().position,
        );
        let mid = [
            0.5 * (a[0] + b[0]),
            0.5 * (a[1] + b[1]),
            0.5 * (a[2] + b[2]) - 1.0,
        ];
        let st = l
            .structures
            .iter_mut()
            .find(|s| Some(s.id.as_str()) == j.from.structure())
            .unwrap();
        st.points.push(ohl_line::StructurePoint {
            description: String::new(),
            label: "A-JH".into(),
            position: [mid[0], mid[1], mid[2] + 2.0],
            ..Default::default()
        });
        st.wires.push(ohl_line::WirePoint {
            label: "JH".into(),
            position: mid,
            ..Default::default()
        });
        st.strings.push(ohl_line::InsulatorString {
            label: "S-JH".into(),
            top: "A-JH".into(),
            wire: "JH".into(),
            weight: 100.0,
            ..Default::default()
        });
        // A string whose wire point was given a set no section uses.
        st.wires.push(ohl_line::WirePoint {
            label: "W99".into(),
            position: [mid[0], mid[1], mid[2] - 0.5],
            set: Some(99),
            ..Default::default()
        });
        st.strings.push(ohl_line::InsulatorString {
            label: "S99".into(),
            top: "A-JH".into(),
            wire: "W99".into(),
            weight: 100.0,
            ..Default::default()
        });
        l.jumpers.insert(0, j);
    });
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let (from, to, n) = {
        let r = harness.state_mut().ready().unwrap();
        let j = r.model.line.jumpers[0].clone();
        (
            j.from.point().unwrap().clone(),
            j.to.point().unwrap().clone(),
            r.model.line.jumpers.len(),
        )
    };
    let end = |p: &ohl_line::SectionPoint| string_at(&p.structure, p.set);
    use_tool(&mut harness, Tool::AddJumper, end(&from), None);
    // A set no section uses: refused, said so; the jumper goes on.
    use_tool(
        &mut harness,
        Tool::AddJumper,
        string_at(&from.structure, 99),
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let (text, error) = r.status.clone().unwrap();
        assert!(error && text.contains("holds no section"), "{text}");
        assert_eq!(r.model.line.jumpers.len(), n);
        assert_eq!(r.jumper_from.as_ref(), Some(&from));
    }
    use_tool(
        &mut harness,
        Tool::AddJumper,
        Selection::String {
            structure: from.structure.clone(),
            label: "S-JH".into(),
        },
        None,
    );
    use_tool(&mut harness, Tool::AddJumper, end(&to), None);
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.model.line.jumpers.len(), n + 1, "{:?}", r.status);
    let j = r.model.line.jumpers.last().unwrap().clone();
    assert_eq!(j.supports().len(), 1);
    assert_eq!(j.supports()[0].wire, "JH");
    assert!(j.sag.is_none() && j.length.is_some());
    let pieces = jumper_pieces(&j);
    assert_eq!(pieces.len(), 2);
    let line = &r.model.line;
    let (a, m, b) = (
        line.wire_point(&from).unwrap().position,
        line.structure(&from.structure)
            .unwrap()
            .wire("JH")
            .unwrap()
            .position,
        line.wire_point(&to).unwrap().position,
    );
    let d = |p: [f64; 3], q: [f64; 3]| (0..3).map(|k| (p[k] - q[k]).powi(2)).sum::<f64>().sqrt();
    assert!((pieces[0] - 1.03 * d(a, m)).abs() < 1e-6, "{pieces:?}");
    assert!((pieces[1] - 1.03 * d(m, b)).abs() < 1e-6);
    assert!(matches!(&r.editor, Some(Form::Jumper(f)) if f.jumper.id == j.id));
    // Pieces typed: the length their sum, the clamp at the first's share.
    let mut k = j.clone();
    set_jumper_pieces(&mut k, &[3.0, 5.0]);
    assert!((k.length.unwrap() - 8.0).abs() < 1e-12);
    assert!((k.supports()[0].at - 3.0 / 8.0).abs() < 1e-12);
    assert_eq!(jumper_pieces(&k), vec![3.0, 5.0]);
}

/// Several weather cases drawn: a span is picked on any of its copies,
/// the case of the copy clicked reported (`None`: the case shown); the
/// app keeps it (highlight, properties) until something is selected
/// otherwise.
#[test]
fn a_span_is_picked_in_any_weather_case_drawn() {
    use crate::selection::{Snaps, pick_in_model_case};
    let mut model = synthetic();
    model.shown_cases = vec![0, 2];
    for k in 0..model.cases.len() {
        model.ensure_solved(k);
    }
    let section = model.line.sections[0].id.clone();
    let mid = |k: usize| {
        let shape = model.cases[k]
            .shapes
            .iter()
            .find(|s| s.section == section)
            .unwrap();
        let p = &shape.spans[0].points;
        p[p.len() / 2]
    };
    let (m0, m2) = (mid(0), mid(2));
    assert!(
        (m0[2] - m2[2]).abs() > 0.5,
        "the ice case sags lower: {m0:?} {m2:?}"
    );
    // Seen across the line (x, z on screen), 10 px per m.
    let project = |p: [f64; 3]| Some([p[0] * 10.0, -p[2] * 10.0]);
    let at = |p: [f64; 3]| project(p).unwrap();
    let span = Selection::Span {
        section: section.clone(),
        span: 0,
    };
    assert_eq!(
        pick_in_model_case(&model, 0, &Snaps::default(), at(m2), 3.0, project),
        (span.clone(), Some(2))
    );
    assert_eq!(
        pick_in_model_case(&model, 0, &Snaps::default(), at(m0), 3.0, project),
        (span.clone(), None)
    );
    // The app keeps the case clicked; a selection made otherwise drops it.
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    r.view3d.picked_case = Some(Some(2));
    r.selection = span.clone();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.model.selection_case, Some(2));
    r.select(Selection::Structure("T2".into()));
    assert_eq!(r.model.selection_case, None);
}

/// Several weather cases drawn: span points, spacers and wire points are
/// picked on any of their copies too (the case of the copy clicked
/// reported), as spans are.
#[test]
fn span_objects_and_wire_points_are_picked_in_any_case_drawn() {
    use crate::selection::{Snap, Snaps, pick_in_model_case};
    let mut model = synthetic_project_with("pick-any-case", |l| {
        let (a, b) = (l.sections[0].id.clone(), l.sections[1].id.clone());
        for s in &mut l.sections[..2] {
            s.span_points.push(ohl_line::SpanPoint {
                label: "M".into(),
                span: 0,
                at: ohl_line::SpanPlace::at_fraction(0.5),
                description: String::new(),
                object: Default::default(),
            });
        }
        let end = |section: &str| ohl_line::SpanPointRef {
            section: section.into(),
            point: "M".into(),
        };
        l.spacers.push(ohl_line::Spacer {
            id: "SP".into(),
            a: end(&a),
            b: end(&b),
            weight: 100.0,
            compression: Some(10_000.0),
            ..Default::default()
        });
    });
    model.shown_cases = vec![0, 2];
    for k in 0..model.cases.len() {
        model.ensure_solved(k);
    }
    let section = model.line.sections[0].id.clone();
    let shape = |k: usize| {
        model.cases[k]
            .shapes
            .iter()
            .find(|s| s.section == section)
            .unwrap()
            .clone()
    };
    let (s0, s2) = (shape(0), shape(2));
    let (p0, p2) = (s0.span_points[0].1, s2.span_points[0].1);
    assert!((p0[2] - p2[2]).abs() > 0.5, "{p0:?} {p2:?}");
    // Seen across the line (x, z on screen), 10 px per m.
    let project = |p: [f64; 3]| Some([p[0] * 10.0, -p[2] * 10.0]);
    let at = |p: [f64; 3]| project(p).unwrap();
    let snaps = Snaps::default();
    assert_eq!(
        pick_in_model_case(&model, 0, &snaps, at(p2), 3.0, project),
        (
            Selection::SpanPoint {
                section: section.clone(),
                label: "M".into()
            },
            Some(2)
        )
    );
    // The spacer, between its ends in the ice case.
    let e = s2.spacers[0].ends;
    let third = [0, 1, 2].map(|i| e[0][i] + (e[1][i] - e[0][i]) / 3.0);
    assert_eq!(
        pick_in_model_case(&model, 0, &snaps, at(third), 3.0, project).0,
        Selection::Spacer("SP".into())
    );
    // A wire point where it hangs in a wind case (the clamp swinging
    // most), seen along the line (y, z on screen).
    let windy = (0..model.cases.len())
        .max_by(|a, b| {
            let p = |k: usize| model.cases[k].case.wind_pressure.si();
            p(*a).total_cmp(&p(*b))
        })
        .unwrap();
    assert!(model.cases[windy].case.wind_pressure.si() > 0.0);
    model.shown_cases = vec![0, windy];
    let side = |p: [f64; 3]| Some([p[1] * 10.0, -p[2] * 10.0]);
    let wires = |k: usize| crate::selection::string_pieces(&model, k);
    let (w0, ww) = (wires(0), wires(windy));
    let apart = |a: [f64; 3], b: [f64; 3]| (a[1] - b[1]).hypot(a[2] - b[2]);
    let (key, clamp, still) = ww
        .iter()
        .filter_map(|(key, v)| Some((key.clone(), v.last()?.1, w0.get(key)?.last()?.1)))
        .max_by(|a, b| apart(a.1, a.2).total_cmp(&apart(b.1, b.2)))
        .unwrap();
    assert!(
        apart(clamp, still) > 0.5,
        "the clamp swings: {clamp:?} {still:?}"
    );
    let mut snaps = Snaps::default();
    snaps.0.insert(Snap::Attachment);
    let (picked, k) = pick_in_model_case(&model, 0, &snaps, side(clamp).unwrap(), 3.0, side);
    assert!(
        matches!(&picked, Selection::Attachment { structure, .. } if *structure == key.0),
        "{picked:?}"
    );
    assert_eq!(k, Some(windy));
}

/// Spread objects: picked all along their stretch on any weather case
/// drawn (the case of the copy clicked reported), highlighted on every
/// copy, listed in the table (selecting the spread, its load on each
/// subconductor said). The Delete tool asks what goes with them: a spread
/// with its bare points, a spread alone (its points stay), and a point a
/// spread ends on with its spreads (the clamp on the other end stays);
/// Cancel leaves everything. The span tool puts a spread on each
/// subconductor.
#[test]
fn spreads_are_picked_and_deleted_with_or_without_their_points() {
    use crate::selection::{Snaps, pick_in_model_case};
    use crate::span_tool::SpanWhat;
    use crate::tables::TableKind;
    use crate::tools::Tool;
    let mut model = synthetic_project_with("spreads", |l| {
        let s = &mut l.sections[0];
        for (label, f, clamp) in [
            ("XA", 0.4, false),
            ("XB", 0.6, false),
            ("XC", 0.65, true),
            ("XD", 0.7, false),
            ("XE", 0.75, false),
            ("XF", 0.85, false),
        ] {
            s.span_points.push(ohl_line::SpanPoint {
                label: label.into(),
                span: 0,
                at: ohl_line::SpanPlace::at_fraction(f),
                description: String::new(),
                object: clamp.then(|| ohl_line::LineObject::clamp(20.0)),
            });
        }
        for (label, from, to, each) in [
            ("AB", "XA", "XB", false),
            ("CD", "XC", "XD", true),
            ("EF", "XE", "XF", false),
        ] {
            s.spreads.push(ohl_line::SpreadObject {
                label: label.into(),
                from: from.into(),
                to: to.into(),
                load: 10.0,
                per_subconductor: each,
                ..Default::default()
            });
        }
    });
    model.shown_cases = vec![0, 1];
    for k in 0..model.cases.len() {
        model.ensure_solved(k);
    }
    let section = model.line.sections[0].id.clone();
    let index = |line: &ohl_line::Line, label: &str| {
        line.sections[0]
            .spreads
            .iter()
            .position(|s| s.label == label)
    };
    let ab = index(&model.line, "AB").unwrap();
    let spread = |index: usize| Selection::Spread {
        section: section.clone(),
        index,
    };
    // Picked on its copy in the hot case (sagging lower), along its
    // stretch.
    let def = model.line.section_with_fractions(&model.line.sections[0]);
    let stretch = |k: usize| {
        let shape = model.cases[k]
            .shapes
            .iter()
            .find(|s| s.section == section)
            .unwrap();
        crate::scene::spreads_drawn(&def, shape)
            .into_iter()
            .find(|(i, _)| *i == ab)
            .unwrap()
            .1
    };
    assert!(crate::selection::spread_stretch(&model, 0, &section, ab).is_some());
    let (s0, s2) = (stretch(0), stretch(1));
    let (m0, m2) = (s0[s0.len() / 2], s2[s2.len() / 2]);
    assert!((m0[2] - m2[2]).abs() > 0.5, "{m0:?} {m2:?}");
    let project = |p: [f64; 3]| Some([p[0] * 10.0, -p[2] * 10.0]);
    let at = |p: [f64; 3]| project(p).unwrap();
    assert_eq!(
        pick_in_model_case(&model, 0, &Snaps::default(), at(m2), 3.0, project),
        (spread(ab), Some(1))
    );
    assert_eq!(
        pick_in_model_case(&model, 0, &Snaps::default(), at(m0), 3.0, project),
        (spread(ab), None)
    );
    // Highlighted on every copy drawn: its stretch and its ends.
    let hi = crate::scene::layer(&model, crate::scene::Layer::Selection, 0, &spread(ab));
    let (lines, points, _) = hi.counts();
    assert!(lines > 0 && points >= 4, "{lines} {points}");
    // The table's row selects it; the load on each subconductor said.
    let rows = TableKind::SpanObjects.rows(&model, 0);
    let cd = index(&model.line, "CD").unwrap();
    let row = rows
        .iter()
        .find(|r| r.select == spread(cd))
        .expect("a row for the spread");
    assert_eq!(row.cells[TableKind::SpanObjects.column("load on")], "each");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // The other tools act on its span: Add on span opens there.
    {
        let r = harness.state_mut().ready().unwrap();
        r.hotbar.hold(Tool::OnSpan);
        r.use_tool(crate::selection::Use {
            target: spread(ab),
            at: None,
            on: None,
        });
        assert!(
            matches!(&r.editor, Some(crate::editors::Form::Span(f)) if f.span == 0 && f.section == section),
            "{:?}",
            r.status
        );
        r.editor = None;
    }
    harness.run();
    let line = |h: &mut Harness<'_, App>| h.state_mut().ready().unwrap().model.line.clone();
    let has_point =
        |l: &ohl_line::Line, p: &str| l.sections[0].span_points.iter().any(|x| x.label == p);
    // EF with its points: both bare, both go.
    let ef = index(&line(&mut harness), "EF").unwrap();
    use_tool(&mut harness, Tool::Delete, spread(ef), None);
    assert!(harness.state_mut().ready().unwrap().spread_delete.is_some());
    harness.get_by_label("Spread and its points").click();
    harness.run();
    {
        let l = line(&mut harness);
        let r = harness.state_mut().ready().unwrap();
        assert!(r.spread_delete.is_none(), "{:?}", r.status);
        assert!(index(&l, "EF").is_none());
        assert!(!has_point(&l, "XE") && !has_point(&l, "XF"));
    }
    // AB alone: its points stay.
    let ab = index(&line(&mut harness), "AB").unwrap();
    use_tool(&mut harness, Tool::Delete, spread(ab), None);
    harness.get_by_label("Spread only").click();
    harness.run();
    {
        let l = line(&mut harness);
        assert!(index(&l, "AB").is_none());
        assert!(has_point(&l, "XA") && has_point(&l, "XB"));
    }
    // A point CD ends on: asked; Cancel leaves it.
    let xd = Selection::SpanPoint {
        section: section.clone(),
        label: "XD".into(),
    };
    use_tool(&mut harness, Tool::Delete, xd.clone(), None);
    assert!(harness.state_mut().ready().unwrap().spread_delete.is_some());
    harness.get_by_label("Cancel").click();
    harness.run();
    {
        let l = line(&mut harness);
        assert!(harness.state_mut().ready().unwrap().spread_delete.is_none());
        assert!(index(&l, "CD").is_some() && has_point(&l, "XD"));
    }
    // As Delete selected: the point and its spread go, the clamp on the
    // spread's other end stays.
    {
        let r = harness.state_mut().ready().unwrap();
        r.select(xd.clone());
        assert!(r.delete_object(&xd));
    }
    harness.run();
    harness
        .get_by_label("Delete the point and its spreads")
        .click();
    harness.run();
    {
        let l = line(&mut harness);
        assert!(index(&l, "CD").is_none());
        assert!(!has_point(&l, "XD") && has_point(&l, "XC"));
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.selection, Selection::None);
        l.validate().unwrap();
    }
    // The span tool: a spread on each subconductor.
    {
        let r = harness.state_mut().ready().unwrap();
        let mut f = crate::span_tool::SpanForm::new(&r.model, r.case, &section, 0, None).unwrap();
        f.what = SpanWhat::Spread;
        (f.from, f.to, f.per_metre, f.per_subconductor) = (10.0, 30.0, 8.0, true);
        r.editor = Some(crate::editors::Form::Span(f));
    }
    harness.run();
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let l = line(&mut harness);
    let last = l.sections[0].spreads.last().unwrap();
    assert!(last.per_subconductor && last.load == 8.0, "{last:?}");
}
