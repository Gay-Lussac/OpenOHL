// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! View › Lines on the ground beside the line, in the application: on
//! the example line, the lines in still air and in high wind (moved by
//! the outermost conductor's blowout), on the ground; the option from the
//! menu (which stays open), shown and hidden; the weather case switched.

use std::path::PathBuf;

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::app::App;
use crate::ground_lines::{self, Settings, Side};
use crate::model::{self, Sources};

fn root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// The example line with every weather case solved.
fn example() -> model::Model {
    let ex = root().join("examples/synthetic-line");
    let mut m = model::load(
        Sources {
            file: ex.join("synthetic-line.ohl.json"),
            library: Some(root().join("libraries/cables")),
            terrain: Some(ex.join("synthetic-survey.csv")),
            weather: Some(ex.join("weather-cases.ohlwc")),
            point_budget: 1000,
            corridor: 60.0,
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    for k in 0..m.cases.len() {
        m.ensure_solved(k);
    }
    m
}

fn case(m: &model::Model, name: &str) -> usize {
    m.cases.iter().position(|c| c.case.name == name).unwrap()
}

/// The first span of the first alignment: its chord's ends in plan.
fn first_chord(m: &model::Model) -> ([f64; 2], [f64; 2]) {
    let view = &m.line.alignment_views()[0];
    let run: Vec<&String> = view.run().collect();
    let p = |id: &str| {
        let s = m.line.structure(id).unwrap();
        [s.position[0], s.position[1]]
    };
    (p(run[0]), p(run[1]))
}

/// The outermost conductor's offset to the left of the chord `(a, b)` at
/// its middle (left positive; the largest for the left side, the
/// smallest for the right): the wires of that span in `case` (each
/// subconductor), interpolated where they cross the perpendicular there.
fn outermost_at_middle(m: &model::Model, k: usize, chord: ([f64; 2], [f64; 2])) -> (f64, f64) {
    outermost_of(&m.cases[k], chord)
}

/// [`outermost_at_middle`] of one solution.
fn outermost_of(solved: &model::Solved, (a, b): ([f64; 2], [f64; 2])) -> (f64, f64) {
    let l = (b[0] - a[0]).hypot(b[1] - a[1]);
    let u = [(b[0] - a[0]) / l, (b[1] - a[1]) / l];
    let n = [-u[1], u[0]];
    let mid = l / 2.0;
    let (mut most, mut least) = (f64::NEG_INFINITY, f64::INFINITY);
    for subs in solved.bundles.spans.values() {
        for pl in subs {
            for w in pl.windows(2) {
                let along = |p: [f64; 3]| (p[0] - a[0]) * u[0] + (p[1] - a[1]) * u[1];
                let off = |p: [f64; 3]| (p[0] - a[0]) * n[0] + (p[1] - a[1]) * n[1];
                let (s0, s1) = (along(w[0]), along(w[1]));
                if (s0 - mid) * (s1 - mid) > 0.0 || s0 == s1 {
                    continue;
                }
                // Only the first span's wires (between its structures).
                let f = (mid - s0) / (s1 - s0);
                let o = off(w[0]) + (off(w[1]) - off(w[0])) * f;
                if o.abs() > 60.0 {
                    continue;
                }
                most = most.max(o);
                least = least.min(o);
            }
        }
    }
    (most, least)
}

/// The line on `side` where it crosses the perpendicular at the middle
/// of the chord: its offset to the left, and its height.
fn line_at_middle(d: &ground_lines::Drawn, side: Side, (a, b): ([f64; 2], [f64; 2])) -> (f64, f64) {
    let l = (b[0] - a[0]).hypot(b[1] - a[1]);
    let u = [(b[0] - a[0]) / l, (b[1] - a[1]) / l];
    let n = [-u[1], u[0]];
    let mid = l / 2.0;
    let line = d.lines.iter().find(|x| x.side == side).unwrap();
    for run in &line.runs {
        for w in run.windows(2) {
            let along = |p: [f64; 3]| (p[0] - a[0]) * u[0] + (p[1] - a[1]) * u[1];
            let (s0, s1) = (along(w[0]), along(w[1]));
            if (s0 - mid) * (s1 - mid) <= 0.0 && s0 != s1 {
                let f = (mid - s0) / (s1 - s0);
                let p = [0, 1, 2].map(|i| w[0][i] + (w[1][i] - w[0][i]) * f);
                // On this span (not a far part of the line).
                let off = (p[0] - a[0]) * n[0] + (p[1] - a[1]) * n[1];
                if off.abs() < 100.0 {
                    return (off, p[2]);
                }
            }
        }
    }
    panic!("no line at the middle of the span on the {side:?}");
}

/// On the example line, 10 m from the outermost conductor: in still air
/// (everyday) and in high wind (600 Pa from the right), each line at the
/// middle of the first span is 10 m beyond the outermost conductor on its
/// side there — so the wind moves it by that conductor's blowout — and
/// on the ground surface (its height there). One solution per case here
/// (the side shown; no other side given): the envelope of both sides is
/// [`the_envelope_over_several_cases_and_both_wind_sides`].
#[test]
fn the_lines_follow_the_outermost_conductor_and_lie_on_the_ground() {
    let m = example();
    let settings = Settings {
        on: true,
        ..Settings::default()
    };
    let chord = first_chord(&m);
    let g = m.ground.as_deref().unwrap();
    let mut shift = Vec::new();
    for name in ["everyday", "high wind"] {
        let k = case(&m, name);
        let d = ground_lines::compute(&m, &[k], &[], &settings);
        assert_eq!(d.ground, ground_lines::Ground::Surface);
        // Two lines per alignment.
        assert_eq!(d.lines.len(), 2 * m.line.alignment_views().len());
        let (most, least) = outermost_at_middle(&m, k, chord);
        let (left, zl) = line_at_middle(&d, Side::Left, chord);
        let (right, zr) = line_at_middle(&d, Side::Right, chord);
        assert!(
            (left - (most + 10.0)).abs() < 0.05,
            "{name}: {left} vs {most} + 10"
        );
        assert!(
            (right - (least - 10.0)).abs() < 0.05,
            "{name}: {right} vs {least} − 10"
        );
        // Draped: every point on the surface.
        for l in &d.lines {
            for run in &l.runs {
                for p in run {
                    let z = g.height(p[0], p[1]).unwrap();
                    assert!((p[2] - z).abs() < 1e-6, "{p:?} {z}");
                }
            }
        }
        assert!(zl.is_finite() && zr.is_finite());
        shift.push((left, most, right, least));
    }
    // The wind from the right blows the wires to the left: both lines
    // move left by the outermost conductors' blowout.
    let (still, windy) = (shift[0], shift[1]);
    assert!(windy.1 - still.1 > 1.0, "blown out: {still:?} {windy:?}");
    assert!(((windy.0 - still.0) - (windy.1 - still.1)).abs() < 0.1);
    assert!(((windy.2 - still.2) - (windy.3 - still.3)).abs() < 0.1);
}

/// From the axis: the lines 25 m from the alignment, on their side (the
/// chords of the round outer corners, in 5° steps, 25·(1 − cos 2.5°) =
/// 24 mm nearer at most).
#[test]
fn from_the_axis_on_the_example() {
    let m = example();
    let settings = Settings {
        on: true,
        from: ground_lines::From::Axis,
        distance: 25.0,
        ..Settings::default()
    };
    let d = ground_lines::compute(&m, &[0], &[], &settings);
    for l in &d.lines {
        let mut n = 0;
        let mut exact = 0;
        for run in &l.runs {
            for p in run {
                let near = l
                    .axis
                    .windows(2)
                    .map(|w| {
                        let e = [w[1][0] - w[0][0], w[1][1] - w[0][1]];
                        let q = [p[0] - w[0][0], p[1] - w[0][1]];
                        let t = ((q[0] * e[0] + q[1] * e[1]) / (e[0] * e[0] + e[1] * e[1]))
                            .clamp(0.0, 1.0);
                        (q[0] - t * e[0]).hypot(q[1] - t * e[1])
                    })
                    .fold(f64::INFINITY, f64::min);
                assert!(near > 25.0 - 0.025 && near < 25.0 + 1e-6, "{p:?}: {near}");
                if (near - 25.0).abs() < 1e-6 {
                    exact += 1;
                }
                // On its side (right of the direction positive).
                let (_, off) = ohl_line::route::project(&l.axis, *p);
                assert_eq!(off > 0.0, l.side == Side::Right, "{p:?}");
                n += 1;
            }
        }
        assert!(n > 10 && exact * 10 > n * 9, "{exact} of {n}");
    }
}

/// Distance in plan from `p` to the segment `s`.
fn to_segment(p: [f64; 2], s: ground_lines::Seg) -> f64 {
    let e = [s[1][0] - s[0][0], s[1][1] - s[0][1]];
    let l2 = e[0] * e[0] + e[1] * e[1];
    let q = [p[0] - s[0][0], p[1] - s[0][1]];
    let t = if l2 > 0.0 {
        ((q[0] * e[0] + q[1] * e[1]) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (q[0] - t * e[0]).hypot(q[1] - t * e[1])
}

/// Whether two plan segments cross (not at a shared end).
fn cross(a: [[f64; 2]; 2], b: [[f64; 2]; 2]) -> bool {
    let c = |o: [f64; 2], p: [f64; 2], q: [f64; 2]| {
        (p[0] - o[0]) * (q[1] - o[1]) - (p[1] - o[1]) * (q[0] - o[0])
    };
    let (d1, d2) = (c(b[0], b[1], a[0]), c(b[0], b[1], a[1]));
    let (d3, d4) = (c(a[0], a[1], b[0]), c(a[0], a[1], b[1]));
    d1 * d2 < -1e-12 && d3 * d4 < -1e-12
}

/// No piece of `d`'s lines crosses itself in plan.
fn assert_no_self_crossing(d: &ground_lines::Drawn) {
    for l in &d.lines {
        for run in &l.runs {
            let segs: Vec<[[f64; 2]; 2]> = run
                .windows(2)
                .map(|w| [[w[0][0], w[0][1]], [w[1][0], w[1][1]]])
                .collect();
            for i in 0..segs.len() {
                for j in i + 2..segs.len() {
                    assert!(
                        !cross(segs[i], segs[j]),
                        "{} {:?}: segments {i} and {j} cross",
                        l.alignment,
                        l.side
                    );
                }
            }
        }
    }
}

/// Several weather cases, both wind sides: everyday and high wind chosen
/// (the case shown not), the views showing the wind from the right only,
/// so the wind from the left is solved for the lines
/// ([`ground_lines::other_sides_now`]). At the middle of the first span
/// each line is 10 m beyond the outermost conductor on its side over the
/// three solutions (everyday; high wind from the right; from the left):
/// the left line beyond the wires the wind from the right blows left,
/// the right line beyond those the wind from the left blows right — so
/// it lies farther out than with either case alone, the right one by the
/// blowout (more than 1 m). Every point of every line is 10 m from the
/// nearest wire or jumper of the three solutions (the band's outer edge)
/// — less only on the straight pieces between the line's points (every
/// 2 m, 5° round a corner: 10 m · (1 − cos 2.5°) = 9.5 mm there), by
/// 5 cm at most where a blown-out conductor curves sharply near its clamp
/// — and never farther; no line crosses itself.
#[test]
fn the_envelope_over_several_cases_and_both_wind_sides() {
    let m = example();
    let (still, windy) = (case(&m, "everyday"), case(&m, "high wind"));
    let settings = Settings {
        on: true,
        shown_case: false,
        cases: ["everyday", "high wind"].map(String::from).into(),
        ..Settings::default()
    };
    let chosen = settings.chosen_in(&m, 0);
    assert_eq!(chosen, vec![still.min(windy), still.max(windy)]);
    let others = ground_lines::other_sides_now(&m, &chosen);
    assert_eq!(
        others.len(),
        1,
        "the wind from the other side of high wind only"
    );
    assert_eq!(others[0].0, windy);
    assert_eq!(m.cases[windy].case.wind_from, ohl_weather::WindSide::Right);
    assert_eq!(others[0].1.case.wind_from, ohl_weather::WindSide::Left);
    let d = ground_lines::compute(&m, &chosen, &others, &settings);
    assert!(d.notes.is_empty(), "{:?}", d.notes);
    let chord = first_chord(&m);
    let all = [&m.cases[still], &m.cases[windy], &*others[0].1];
    let most = all.map(|s| outermost_of(s, chord));
    let left_most = most.iter().map(|x| x.0).fold(f64::NEG_INFINITY, f64::max);
    let right_most = most.iter().map(|x| x.1).fold(f64::INFINITY, f64::min);
    let (left, _) = line_at_middle(&d, Side::Left, chord);
    let (right, _) = line_at_middle(&d, Side::Right, chord);
    assert!(
        (left - (left_most + 10.0)).abs() < 0.05,
        "{left} {left_most}"
    );
    assert!(
        (right - (right_most - 10.0)).abs() < 0.05,
        "{right} {right_most}"
    );
    for k in [still, windy] {
        let one = ground_lines::compute(&m, &[k], &[], &settings);
        let (l, _) = line_at_middle(&one, Side::Left, chord);
        let (r, _) = line_at_middle(&one, Side::Right, chord);
        assert!(
            left >= l - 1e-6 && right <= r + 1e-6,
            "{k}: {left} {l} {right} {r}"
        );
        if k == windy {
            // The wind from the left blows the wires out on the right.
            assert!(r - right > 1.0, "{r} {right}");
        }
    }
    // The band's outer edge, over every wire and jumper of the three.
    let solutions: Vec<&model::Solved> = all.to_vec();
    let (mut n, mut exact) = (0, 0);
    for view in m.line.alignment_views() {
        let run: Vec<&String> = view.run().collect();
        let chain = ground_lines::Chain::of_wires(&m.line, &solutions, &run);
        let sources: Vec<ground_lines::Seg> = chain
            .spans
            .iter()
            .chain(&chain.joints)
            .flatten()
            .copied()
            .collect();
        for l in d.lines.iter().filter(|l| l.alignment == view.id) {
            for p in l.runs.iter().flatten() {
                let near = sources
                    .iter()
                    .map(|s| to_segment([p[0], p[1]], *s))
                    .fold(f64::INFINITY, f64::min);
                assert!(
                    near > 10.0 - 0.05 && near < 10.0 + 1e-6,
                    "{} {:?}: {p:?} at {near}",
                    l.alignment,
                    l.side
                );
                n += 1;
                if (near - 10.0).abs() < 5e-3 {
                    exact += 1;
                }
            }
        }
    }
    assert!(n > 1000 && exact * 10 > n * 9, "{exact} of {n}");
    assert_no_self_crossing(&d);
}

/// The jumpers count as conductors, so the lines go on round the
/// structures. On the example the branch's conductor ends at the Y pylon
/// T3 in a dead-end string (its wire point 6.5 m along the branch from
/// the pylon's centre, on the branch's axis) and the jumper J-T3-branch
/// joins it across the pylon to the main line's conductor. 2 m away in
/// still air, the branch's lines reach back to T3 (their first point
/// within 0.5 m of the branch's start, the jumper passing within 0.5 m
/// of the axis there); without the jumper they would begin only where
/// the conductor comes within 2 m of the cross line: 6.5 − 2 = 4.5 m
/// along, at the next point every 2 m, beyond 4 m. T3 is also an angle
/// tower of the main route (≈ 10°): its lines go on round it in one
/// piece (from 20 m before to 20 m after it), and no line crosses
/// itself.
#[test]
fn the_jumper_counts_at_the_dead_end_and_the_angle_tower() {
    let mut m = example();
    let still = case(&m, "everyday");
    assert!(
        m.cases[still]
            .jumpers
            .iter()
            .any(|j| j.jumper == "J-T3-branch")
    );
    let settings = Settings {
        on: true,
        distance: 2.0,
        ..Settings::default()
    };
    let start = |d: &ground_lines::Drawn, side: Side| {
        let l = d
            .lines
            .iter()
            .find(|l| l.alignment == "branch" && l.side == side)
            .unwrap();
        l.runs
            .iter()
            .flatten()
            .map(|p| ohl_line::route::project(&l.axis, *p).0)
            .fold(f64::INFINITY, f64::min)
    };
    let d = ground_lines::compute(&m, &[still], &[], &settings);
    for side in [Side::Left, Side::Right] {
        let s = start(&d, side);
        assert!(s < 0.5, "{side:?}: begins {s} m along the branch");
    }
    // The main route at T3 (station 640 along it).
    for l in d.lines.iter().filter(|l| l.alignment == "main") {
        let through = l.runs.iter().any(|run| {
            let st: Vec<f64> = run
                .iter()
                .map(|p| ohl_line::route::project(&l.axis, *p).0)
                .collect();
            st.iter().any(|s| *s < 620.0) && st.iter().any(|s| *s > 660.0)
        });
        assert!(through, "{:?}: broken at T3", l.side);
    }
    assert_no_self_crossing(&d);
    // Without the jumper.
    let c = &mut m.cases[still];
    c.jumpers.clear();
    c.bundles.jumpers.clear();
    let bare = ground_lines::compute(&m, &[still], &[], &settings);
    for side in [Side::Left, Side::Right] {
        let s = start(&bare, side);
        assert!(s > 4.0, "{side:?}: begins {s} m along the branch");
    }
}

/// The option from View › Lines on the ground beside the line: ticked,
/// the menu stays open and the views get the lines (the 3D layer made
/// again); another weather case shown makes them for it (a windy one
/// with its wind from both sides), and back again they are the ones kept.
/// The weather cases from the menu: None leaves none (no lines, said); a
/// case ticked by name, the case shown unticked, no longer follows the
/// case shown (the lines are kept by the set of cases); All takes every
/// case; the case shown ticked again. Unticked, they are gone. Remembered
/// with the view settings, not in the project.
#[test]
fn the_option_shows_and_hides_and_follows_the_weather_case() {
    let model = example();
    let windy = case(&model, "high wind");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    h.run();
    h.get_by_label("View").click();
    h.run();
    h.get_by_label_contains("Lines on the ground beside the line")
        .hover();
    h.run();
    h.get_by_label("Show the lines").click();
    h.run();
    // Still open: a tick keeps the menu.
    assert!(h.query_by_label("Show the lines").is_some());
    let r = h.state_mut().ready().unwrap();
    assert!(r.model.ground_lines.on);
    assert!(r.model.view_prefs().ground_lines.on);
    let first = r.ground_lines.drawn.clone().expect("drawn");
    assert!(!first.lines.is_empty());
    assert!(r.view3d.ground_lines.is_some() && r.plan.ground_lines.is_some());
    assert_eq!(r.ground_lines.made, 1);
    // Another case shown (windy: its wind from the left solved for the
    // lines in the background, then taken: made once or twice).
    r.case = windy;
    settle(&mut h);
    let r = h.state_mut().ready().unwrap();
    let second = r.ground_lines.drawn.clone().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&first, &second));
    assert_ne!(first.lines, second.lines);
    assert!(second.notes.is_empty(), "{:?}", second.notes);
    assert!(std::sync::Arc::ptr_eq(
        r.view3d.ground_lines.as_ref().unwrap(),
        &second
    ));
    let made = r.ground_lines.made;
    assert!((2..=3).contains(&made), "{made}");
    // The same as made here with both wind sides.
    let both = ground_lines::other_sides_now(&r.model, &[windy]);
    let here = ground_lines::compute(&r.model, &[windy], &both, &r.model.ground_lines);
    assert_eq!(here.lines, second.lines);
    // Back: kept, not made again.
    r.case = 0;
    h.run();
    let r = h.state_mut().ready().unwrap();
    assert!(std::sync::Arc::ptr_eq(
        r.ground_lines.drawn.as_ref().unwrap(),
        &first
    ));
    assert_eq!(r.ground_lines.made, made);
    // None: no case, no lines, said; the menu stays open.
    h.get_by_label("None").click();
    h.run();
    assert!(h.query_by_label("Show the lines").is_some());
    let r = h.state_mut().ready().unwrap();
    let g = &r.model.ground_lines;
    assert!(!g.shown_case && g.cases.is_empty());
    let none = r.ground_lines.drawn.clone().unwrap();
    assert!(none.lines.is_empty());
    assert!(
        none.notes
            .iter()
            .any(|n| n.contains("no weather case chosen"))
    );
    // A case by name: high wind, whatever case is shown.
    h.get_by_label("high wind").click();
    settle(&mut h);
    let r = h.state_mut().ready().unwrap();
    assert_eq!(
        r.model.view_prefs().ground_lines.cases,
        ["high wind".to_owned()].into()
    );
    let named = r.ground_lines.drawn.clone().unwrap();
    assert_eq!(named.lines, second.lines);
    let made = r.ground_lines.made;
    r.case = windy;
    h.run();
    let r = h.state_mut().ready().unwrap();
    assert!(std::sync::Arc::ptr_eq(
        r.ground_lines.drawn.as_ref().unwrap(),
        &named
    ));
    assert_eq!(r.ground_lines.made, made, "keyed by the set of cases");
    r.case = 0;
    // All: every case.
    h.get_by_label("All").click();
    settle(&mut h);
    let r = h.state_mut().ready().unwrap();
    let names: std::collections::BTreeSet<String> =
        r.model.cases.iter().map(|c| c.case.name.clone()).collect();
    assert_eq!(r.model.ground_lines.cases, names);
    let every = r.ground_lines.drawn.clone().unwrap();
    // Made for this set; on the example high wind from both sides holds
    // every other case's conductors inside its envelope: the same lines.
    assert!(r.ground_lines.made > made);
    assert!(!std::sync::Arc::ptr_eq(&every, &named));
    assert_eq!(every.lines, named.lines);
    assert!(every.notes.is_empty(), "{:?}", every.notes);
    // The case shown again (and the names kept).
    h.get_by_label_contains("The case shown").click();
    h.run();
    let r = h.state_mut().ready().unwrap();
    assert!(r.model.ground_lines.shown_case);
    assert!(r.model.view_prefs().ground_lines.shown_case);
    // Unticked: gone.
    h.get_by_label("Show the lines").click();
    h.run();
    let r = h.state_mut().ready().unwrap();
    assert!(r.ground_lines.drawn.is_none() && r.view3d.ground_lines.is_none());
    assert!(r.plan.ground_lines.is_none() && r.profile.ground_lines.is_none());
    assert!(!r.model.ground_lines.on);
}

/// Runs frames until the weather cases and the other wind sides are
/// solved (at most 20 s).
fn settle(h: &mut Harness<'_, App>) {
    for _ in 0..1000 {
        h.run();
        let r = h.state_mut().ready().unwrap();
        if r.model.solving.is_empty() && !r.ground_lines.solving() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    h.run();
}

/// An edit of the line makes them again. Without a ground surface they
/// take the alignment's heights.
#[test]
fn an_edit_makes_them_again() {
    let mut model = crate::tests::synthetic_project("ground-lines-edit");
    model.ground_lines.on = true;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .build_eframe(|_| App::with_model(model, None));
    h.run();
    let r = h.state_mut().ready().unwrap();
    let before = r.ground_lines.drawn.clone().unwrap();
    assert_eq!(before.ground, ground_lines::Ground::Alignment);
    assert!(before.lines.iter().any(|l| !l.runs.is_empty()));
    // A structure moved 3 m sideways (perpendicular to its span).
    let (a, b) = first_chord(&r.model);
    let l = (b[0] - a[0]).hypot(b[1] - a[1]);
    let n = [-(b[1] - a[1]) / l, (b[0] - a[0]) / l];
    let id = r.model.line.alignment_views()[0]
        .run()
        .nth(1)
        .unwrap()
        .clone();
    let mut p = r.model.line.structure(&id).unwrap().position;
    p[0] += 3.0 * n[0];
    p[1] += 3.0 * n[1];
    assert!(r.perform(crate::edit::Action::Move { id, position: p }));
    for _ in 0..50 {
        h.run();
        let r = h.state_mut().ready().unwrap();
        if r.model.solving.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    h.run();
    let r = h.state_mut().ready().unwrap();
    let after = r.ground_lines.drawn.clone().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&before, &after));
    assert_ne!(before.lines, after.lines);
}

/// In the profile too when ticked: the ground along the lines of both
/// sides; the 3D view, the plan and the profile drawn with them, round
/// several weather cases (high wind shown, everyday and glaze ice ticked,
/// both wind sides) — OHL_RENDER_PNG: images of the window with the 3D
/// view, then the plan, in front, and the plan at the Y pylon T3 (the
/// angle tower where the branch's jumper is) with the lines 5 m away.
#[test]
fn the_profile_shows_both_sides_and_the_views_draw_them() {
    let gpu = ohl_render::headless_device().is_ok();
    assert!(
        gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    let mut model = example();
    model.ground_lines = Settings {
        on: true,
        distance: 15.0,
        profile: true,
        cases: ["everyday", "glaze ice"].map(String::from).into(),
        ..Settings::default()
    };
    let windy = case(&model, "high wind");
    let mut builder = Harness::builder().with_size(egui::vec2(1600.0, 1000.0));
    if gpu {
        builder = builder.wgpu();
    }
    let mut h = builder.build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    h.state_mut().ready().unwrap().case = windy;
    h.run_steps(4);
    settle(&mut h);
    let r = h.state_mut().ready().unwrap();
    assert_eq!(r.model.ground_lines.chosen_in(&r.model, windy).len(), 3);
    let [left, right] = h.state_mut().ready().unwrap().profile.ground_profiles();
    assert!(left > 0 && right > 0, "{left} {right}");
    let dir = ohl_core::env::var_os("OHL_RENDER_PNG").filter(|_| gpu);
    if let Some(dir) = &dir {
        let image = h.render().unwrap();
        let path = PathBuf::from(dir).join("ground-lines-3d.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    h.state_mut().show_tab(crate::app::Tab::Plan);
    h.run_steps(3);
    if let Some(dir) = &dir {
        let image = h.render().unwrap();
        let path = PathBuf::from(dir).join("ground-lines-plan.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    {
        let r = h.state_mut().ready().unwrap();
        r.model.ground_lines.distance = 5.0;
        r.plan.set_view([640.0, 20.0], 9.0);
    }
    settle(&mut h);
    h.run_steps(3);
    if let Some(dir) = &dir {
        let image = h.render().unwrap();
        let path = PathBuf::from(dir).join("ground-lines-plan-t3.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    // Not in the profile when unticked.
    h.state_mut().ready().unwrap().model.ground_lines.profile = false;
    h.run_steps(2);
    assert_eq!(
        h.state_mut().ready().unwrap().profile.ground_profiles(),
        [0, 0]
    );
}
