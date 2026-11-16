// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: the project lines shown next to the line worked on draw both
//! wind sides of each case, by the line worked on's rule (owner
//! 2026-11-16).

use super::*;
use crate::model::{Model, Solved, mirror_case};

/// The synthetic project with a line "phase 1" made from main and one of
/// its structures moved there (it differs in it and its sections), main
/// worked on.
fn with_phase_line(name: &str) -> Model {
    let mut m = synthetic_project(name);
    m.project_op(
        "phase",
        ohl_server::Op::CreateLine {
            name: "phase 1".into(),
            from: "main".into(),
        },
    )
    .unwrap();
    switch(&mut m, "phase 1");
    let id = m.line.structures[1].id.clone();
    let p = m.line.structure(&id).unwrap().position;
    m.apply(
        "shift",
        ohl_session::Command::MoveStructure {
            id,
            position: [p[0] + 3.0, p[1], p[2]],
        },
    )
    .unwrap();
    switch(&mut m, "main");
    m
}

/// The cases `shown` shown for the line, solved (the line worked on at
/// once, the project lines shown in the background as the application
/// does, each frame's `want_others` and `poll_others`) until each line
/// shown holds every side drawn (exit clause: a minute).
fn show(m: &mut Model, shown: &[usize]) {
    m.shown_cases = shown.to_vec();
    for &k in shown {
        m.ensure_solved(k);
    }
    let ctx = egui::Context::default();
    let start = std::time::Instant::now();
    loop {
        m.want_others(shown, &ctx);
        m.poll_others();
        let done = m.other_lines.iter().all(|o| {
            o.solving.is_empty()
                && shown
                    .iter()
                    .all(|&k| o.cases[k].solved_for(m.mirror_wanted(k)))
        });
        if done {
            break;
        }
        assert!(start.elapsed().as_secs() < 60, "not solved in a minute");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Weather cases with winds of every kind: still air, the two directed
/// copies of one source wind (90°, 270°), along the line (0°), at an
/// angle (30°); the model's cases and the project line "phase 1" shown in
/// them.
fn with_winds(name: &str) -> Model {
    let mut m = with_phase_line(name);
    let file = ohl_weather::WeatherCases::parse(
        "format = \"ohl-weather-cases\"\nversion = 2\n\
         [[case]]\nname = \"calm\"\ntemperature = \"15 °C\"\n\
         [[case]]\nname = \"storm — from the right (90°)\"\ntemperature = \"15 °C\"\n\
         wind_pressure = \"600 Pa\"\nwind_direction = 90\n\
         [[case]]\nname = \"storm — from the left (270°)\"\ntemperature = \"15 °C\"\n\
         wind_pressure = \"600 Pa\"\nwind_direction = 270\n\
         [[case]]\nname = \"ahead\"\ntemperature = \"15 °C\"\nwind_pressure = \"600 Pa\"\n\
         wind_direction = 0\n\
         [[case]]\nname = \"gust\"\ntemperature = \"15 °C\"\nwind_pressure = \"600 Pa\"\n\
         wind_direction = 30\n",
    )
    .unwrap()
    .cases;
    m.cases = file.into_iter().map(Solved::pending).collect();
    m.show_line("phase 1").unwrap();
    m
}

const CALM: usize = 0;
const RIGHT: usize = 1;
const LEFT: usize = 2;
const AHEAD: usize = 3;
const GUST: usize = 4;

/// A windy case of a project line shown: drawn as it is and mirrored, as
/// the line worked on is — its mirror solved (only then) from the line
/// worked on's, equal to a full solve of the mirror; whole lines drawn
/// copy for copy as the line worked on.
#[test]
fn a_project_line_shown_draws_both_wind_sides() {
    let mut m = with_winds("other-both-sides");
    show(&mut m, &[CALM]);
    let o = &m.other_lines[0];
    assert_eq!(o.cases[CALM].drawn().count(), 1, "still air: once");
    assert!(o.cases[GUST].mirror.is_none(), "not drawn: not solved");
    show(&mut m, &[GUST]);
    let o = &m.other_lines[0];
    let gust = &o.cases[GUST];
    assert_eq!(gust.drawn().count(), 2);
    assert_eq!(m.cases[GUST].drawn().count(), 2);
    let mirror = gust.mirror.as_deref().unwrap();
    assert_eq!(
        mirror.case.wind_direction,
        Some(ohl_weather::WindDirection::Angle(330.0))
    );
    let full = Solved::new(&o.line, &m.library, mirror_case(&m.cases[GUST].case));
    assert_eq!(mirror.shapes, full.shapes);
    assert_eq!(mirror.jumpers, full.jumpers);
    assert_ne!(mirror.shapes, gust.shapes, "the other side");
    // Whole lines asked for: every copy the line worked on draws, it
    // draws too.
    m.other_lines_whole = true;
    let count = |m: &Model, l: crate::scene::Layer| {
        crate::scene::layer(m, l, GUST, &Selection::None).counts().0
    };
    let working =
        count(&m, crate::scene::Layer::Structures) + count(&m, crate::scene::Layer::Wires);
    assert_eq!(count(&m, crate::scene::Layer::OtherLines), working);
}

/// A wind along the line (from ahead, 0°) mirrors to itself: the project
/// line shown draws it once, no mirror solved.
#[test]
fn a_project_line_shown_draws_a_wind_along_the_line_once() {
    let mut m = with_winds("other-along");
    show(&mut m, &[AHEAD]);
    let c = &m.other_lines[0].cases[AHEAD];
    assert!(c.ready);
    assert!(c.mirror.is_none());
    assert_eq!(c.drawn().count(), 1);
}

/// The two directed copies of one source wind (90°, 270°) shown together:
/// each drawn once on the project line shown, their mirrors not solved;
/// one shown alone, its mirror solved and drawn; both shown again, that
/// mirror hidden (kept, not drawn) and the views asked to draw again.
#[test]
fn a_project_line_shown_draws_a_mirror_pair_once() {
    let mut m = with_winds("other-pair");
    show(&mut m, &[RIGHT, LEFT]);
    let o = &m.other_lines[0];
    for k in [RIGHT, LEFT] {
        assert!(o.cases[k].ready);
        assert!(o.cases[k].mirror.is_none(), "{}", o.cases[k].case.name);
        assert_eq!(o.cases[k].drawn().count(), 1);
    }
    show(&mut m, &[RIGHT]);
    assert_eq!(m.other_lines[0].cases[RIGHT].drawn().count(), 2);
    // The 90° case's mirror: the 270° case, bit for bit.
    let o = &m.other_lines[0];
    assert_eq!(
        o.cases[RIGHT].mirror.as_deref().unwrap().shapes,
        o.cases[LEFT].shapes
    );
    m.mirrors_changed = false;
    show(&mut m, &[RIGHT, LEFT]);
    let c = &m.other_lines[0].cases[RIGHT];
    assert_eq!(c.drawn().count(), 1);
    assert_eq!(c.sides().count(), 2, "kept, hidden");
    assert!(m.mirrors_changed, "the views drawn again");
}

/// A project saved before (its weather cases without directions, the old
/// wind choice still in this user's view) opens with the lines it showed,
/// nothing solved of them until shown; still air drawn once as before,
/// a case with wind from the right as before and now its mirror from the
/// left too, each equal to a full solve.
#[test]
fn an_old_project_opens_with_its_lines_shown_unchanged() {
    let m = with_phase_line("other-old");
    let mut sources = m.sources.clone();
    drop(m);
    let prefs: model::ViewPrefs =
        serde_json::from_str(r#"{"wind": "left", "line": "main", "shown_lines": ["phase 1"]}"#)
            .unwrap();
    sources.view = Some(prefs);
    let mut m = model::load(sources, &|_| {}).unwrap();
    assert_eq!(m.project_line(), "main");
    assert_eq!(m.other_lines.len(), 1);
    assert!(m.other_lines[0].cases.iter().all(|c| !c.ready));
    let k = |m: &Model, name: &str| m.cases.iter().position(|c| c.case.name == name).unwrap();
    let (everyday, wind) = (k(&m, "everyday"), k(&m, "high wind"));
    show(&mut m, &[everyday]);
    let o = &m.other_lines[0];
    let c = &o.cases[everyday];
    assert!(c.mirror.is_none());
    assert_eq!(c.drawn().count(), 1);
    let full = Solved::new(&o.line, &m.library, m.cases[everyday].case.clone());
    assert_eq!(c.shapes, full.shapes);
    show(&mut m, &[wind]);
    let o = &m.other_lines[0];
    let c = &o.cases[wind];
    assert_eq!(c.case.wind_from, ohl_weather::WindSide::Right);
    assert_eq!(c.case.wind_direction, None);
    let mirror = c.mirror.as_deref().unwrap();
    assert_eq!(mirror.case.wind_from, ohl_weather::WindSide::Left);
    assert_eq!(c.drawn().count(), 2);
    for side in c.sides() {
        let full = Solved::new(&o.line, &m.library, side.case.clone());
        assert_eq!(side.shapes, full.shapes, "{:?}", side.case.wind_from);
    }
}
