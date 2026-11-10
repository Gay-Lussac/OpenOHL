// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests of the profile's side lines (2026-11-10): along the corridor's
//! edges as before, at a distance from the axis, under the outermost
//! conductors of the lowest arm; on the synthetic example's main route
//! (along +x: circuit "Example 1" with S1 6 m right of the axis and S2
//! 6 m left, both on the one arm level).

use std::sync::Arc;

use ohl_core::units::Temperature;
use ohl_line::route::point_at;
use ohl_line::solve::SectionShape;
use ohl_line::{Line, SectionPoint};
use ohl_terrain::Terrain;
use ohl_weather::WeatherCase;

use crate::model::{Item, Layer};
use crate::side_lines::{ProfileLines, SideLine, circuits, still_case};
use crate::template::SheetTemplate;
use crate::tests::solved;
use crate::{Options, RouteDrawing, build, ground_profile, page_format, sheets};

const HEAD: &str = "format = \"ohl-sheet-template\"\nversion = 1\n";

fn template(corridor: &str) -> Arc<SheetTemplate> {
    Arc::new(SheetTemplate::parse(&format!("{HEAD}[corridor]\n{corridor}"), None, &[]).unwrap())
}

fn options(corridor: &str) -> Options {
    Options::default().with_template(template(corridor))
}

fn solve(line: &Line) -> Vec<SectionShape> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables");
    let library = ohl_cables::Library::load(&root).unwrap();
    let case = WeatherCase::still("t", Temperature::from_celsius(15.0));
    ohl_line::solve::solve_line(line, &library, &case)
        .into_iter()
        .map(Result::unwrap)
        .collect()
}

/// The main route of `line` with `shapes` (in still air) drawn with `o`,
/// the lowest arm's side lines set from `still`.
fn drawn(
    line: &Line,
    shapes: &[SectionShape],
    still: Option<&[SectionShape]>,
    terrain: &Terrain,
    o: &Options,
) -> RouteDrawing {
    let route = &line.alignment_views()[0];
    let mut d = build(line, route, shapes, &[], Some(terrain), &[], o).unwrap();
    d.set_lowest_arm_lines(line, route, still, Some(terrain), o);
    d
}

/// The synthetic line with a third wire on its main route: section S4 of
/// circuit "Outer" on new points 3 m outside S1's (9 m right of the
/// axis), `dz` m above them.
fn with_outer(dz: f64) -> Line {
    let (mut line, ..) = solved();
    let mut section = line.sections.iter().find(|s| s.id == "S1").unwrap().clone();
    for p in &section.points {
        let s = line
            .structures
            .iter_mut()
            .find(|s| s.id == p.structure)
            .unwrap();
        let at = |label: &str| s.points.iter().find(|q| q.label == label).unwrap().position;
        let (a, b) = (at("A1"), at("A11"));
        let shift = [(a[0] - b[0]) / 4.0, (a[1] - b[1]) / 4.0, dz];
        let moved = |q: [f64; 3]| [q[0] + shift[0], q[1] + shift[1], q[2] + shift[2]];
        let mut point = s.points.iter().find(|q| q.label == "A1").unwrap().clone();
        point.label = "A21".into();
        point.position = moved(point.position);
        s.points.push(point);
        let mut string = s.strings.iter().find(|q| q.label == "S1").unwrap().clone();
        string.label = "S21".into();
        string.top = "A21".into();
        string.wire = "W21".into();
        s.strings.push(string);
        let mut wire = s.wires.iter().find(|q| q.label == "W1").unwrap().clone();
        wire.label = "W21".into();
        wire.set = Some(21);
        wire.position = moved(wire.position);
        s.wires.push(wire);
    }
    section.id = "S4".into();
    section.circuit_name = "Outer".into();
    section.phase_label = "L3".into();
    for p in &mut section.points {
        *p = SectionPoint::new(p.structure.clone(), 21);
    }
    line.sections.push(section);
    line
}

/// The value at station `s` of `runs` ((station, value) points), if a run
/// covers it.
fn value_at(runs: &[Vec<[f64; 2]>], s: f64) -> Option<f64> {
    runs.iter().flat_map(|r| r.windows(2)).find_map(|w| {
        (w[0][0] <= s && s <= w[1][0] && w[1][0] > w[0][0])
            .then(|| w[0][1] + (w[1][1] - w[0][1]) * (s - w[0][0]) / (w[1][0] - w[0][0]))
    })
}

/// The offset at station `s` of the solid wire of `d` whose label starts
/// with `section`.
fn wire_offset(d: &RouteDrawing, section: &str, s: f64) -> f64 {
    let runs: Vec<Vec<[f64; 2]>> = d
        .wires
        .iter()
        .filter(|w| w.dash.is_none() && w.label.starts_with(&format!("{section} span")))
        .map(|w| w.points.iter().map(|p| [p[0], p[1]]).collect())
        .collect();
    value_at(&runs, s).unwrap_or_else(|| panic!("no wire {section} at {s}"))
}

fn side(d: &RouteDrawing, right: bool) -> Option<&SideLine> {
    d.profile_lines.iter().find(|l| l.right == right)
}

/// Stations within the spans of the main route, clear of its structures
/// and of the bend at T3 (640 m).
const STATIONS: [f64; 6] = [60.0, 150.0, 260.0, 450.0, 800.0, 1100.0];

/// Side line `l` follows the wire of `section` in plan, and its ground is
/// the terrain's height under it.
fn follows(d: &RouteDrawing, l: &SideLine, section: &str, shift: f64, terrain: &Terrain) {
    for s in STATIONS {
        let o = value_at(&l.plan, s).unwrap_or_else(|| panic!("no side line at {s}"));
        let wire = wire_offset(d, section, s) + shift;
        assert!((o - wire).abs() < 0.01, "{section} at {s}: {o} vs {wire}");
        let p = point_at(&d.centreline, s, o);
        let z = terrain.height(p[0], p[1]).unwrap();
        let g = value_at(&l.ground, s).unwrap();
        assert!((g - z).abs() < 0.02, "ground at {s}: {g} vs {z}");
    }
}

/// Without the new keys nothing changes: the ground along the corridor's
/// edges as before, no other side line, and the same with
/// `profile_lines = "corridor"` written out.
#[test]
fn the_default_is_the_corridors_edges() {
    let (line, shapes, _, terrain) = solved();
    let o = options("width = \"25 m\"\n");
    assert_eq!(o.profile_line_mode(), ProfileLines::Corridor);
    let d = drawn(&line, &shapes, Some(&shapes), &terrain, &o);
    assert!(d.profile_lines.is_empty() && d.profile_line_notes.is_empty());
    let edges: Vec<_> = [25.0, -25.0]
        .into_iter()
        .map(|w| (w, ground_profile(&d.centreline, w, &terrain)))
        .collect();
    assert_eq!(d.corridor_ground, edges);
    assert!(d.corridor_ground.iter().all(|c| !c.1.is_empty()));
    let written = options("width = \"25 m\"\nprofile_lines = \"corridor\"\n");
    let same = drawn(&line, &shapes, Some(&shapes), &terrain, &written);
    assert_eq!(same, d);
    let page = page_format("A3").unwrap();
    assert_eq!(
        sheets(&same, &written, &page)[0].drawing.svg(),
        sheets(&d, &o, &page)[0].drawing.svg()
    );
}

/// At a distance: the ground 10 m either side of the axis whatever the
/// corridor's width, the corridor's edges kept in the plan, the two
/// lines drawn in the plan too and named in the legend's words.
#[test]
fn at_a_distance_from_the_axis() {
    let (line, shapes, _, terrain) = solved();
    let o = options(
        "width = \"25 m\"\nprofile_lines = \"distance\"\nprofile_distance = \"10 m\"\n\
         near = { colour = \"#2a7f3f\" }\nfar = { colour = \"#7a3fa0\" }\n",
    );
    assert_eq!(o.profile_line_mode(), ProfileLines::Distance(10.0));
    let d = drawn(&line, &shapes, Some(&shapes), &terrain, &o);
    assert!(d.corridor_ground.is_empty());
    assert_eq!(
        d.profile_lines.iter().map(|l| l.right).collect::<Vec<_>>(),
        [true, false],
        "the near side (right) first"
    );
    for l in &d.profile_lines {
        let offset = if l.right { 10.0 } else { -10.0 };
        assert_eq!(l.plan, [vec![[0.0, offset], [d.length, offset]]]);
        assert_eq!(l.ground, ground_profile(&d.centreline, offset, &terrain));
        for s in STATIONS {
            let p = point_at(&d.centreline, s, offset);
            let z = terrain.height(p[0], p[1]).unwrap();
            let g = value_at(&l.ground, s).unwrap();
            assert!((g - z).abs() < 0.02, "{offset} at {s}: {g} vs {z}");
        }
    }
    let s = sheets(&d, &o, &page_format("A1").unwrap());
    let flat = s[0].drawing.flat();
    let titles = |layer: Layer| -> Vec<String> {
        flat.iter()
            .filter_map(|(i, _)| match i {
                Item::Path(p) if p.layer == layer => p.title.clone(),
                _ => None,
            })
            .collect()
    };
    // Profile and plan: each line twice on its layer, in its words.
    let ground = titles(Layer::GroundSide);
    for words in [
        "terrain 10 m right of the axis",
        "terrain 10 m left of the axis",
    ] {
        assert!(
            ground.iter().filter(|t| *t == words).count() >= 2,
            "{words}: {ground:?}"
        );
    }
    assert_eq!(titles(Layer::Corridor), ["corridor ±25 m"; 2]);
    // The plan's side lines in the pens of the profile's.
    let colours: Vec<_> = flat
        .iter()
        .filter_map(|(i, _)| match i {
            Item::Path(p) if p.layer == Layer::GroundSide => Some(p.pen.color),
            _ => None,
        })
        .collect();
    for c in [
        crate::model::Color(0x2a, 0x7f, 0x3f),
        crate::model::Color(0x7a, 0x3f, 0xa0),
    ] {
        assert!(colours.iter().filter(|x| **x == c).count() >= 2, "{c:?}");
    }
}

/// Under the lowest arm: on the example's one arm level the right line
/// follows S1 and the left one S2 as solved in still air, the ground
/// taken under them; continuous from the first structure to the last.
#[test]
fn under_the_outermost_conductors_of_the_lowest_arm() {
    let (line, shapes, _, terrain) = solved();
    let o = options("profile_lines = \"lowest_arm\"\n");
    assert_eq!(o.profile_line_mode(), ProfileLines::LowestArm);
    let d = drawn(&line, &shapes, Some(&shapes), &terrain, &o);
    assert!(d.corridor_ground.is_empty() && d.profile_line_notes.is_empty());
    let (right, left) = (side(&d, true).unwrap(), side(&d, false).unwrap());
    follows(&d, right, "S1", 0.0, &terrain);
    follows(&d, left, "S2", 0.0, &terrain);
    for l in [right, left] {
        assert_eq!(l.plan.len(), 1, "one run, joined at the structures");
        let run = &l.plan[0];
        assert!(run[0][0] < 10.0 && run[run.len() - 1][0] > d.length - 10.0);
        assert!(!l.ground.is_empty());
    }
    // Drawn in the profile and in the plan, in the legend's words.
    let s = sheets(&d, &o, &page_format("A1").unwrap());
    let flat = s[0].drawing.flat();
    for words in [
        "terrain under the outermost conductor of the lowest arm, right",
        "terrain under the outermost conductor of the lowest arm, left",
    ] {
        let n = flat
            .iter()
            .filter(|(i, _)| {
                matches!(i, Item::Path(p) if p.layer == Layer::GroundSide
                    && p.title.as_deref() == Some(words))
            })
            .count();
        assert!(n >= 2, "{words}: {n}");
    }
    // Without a still-air case: the attachment points' chords, said so.
    let chords = drawn(&line, &shapes, None, &terrain, &o);
    assert!(chords.profile_line_notes[0].contains("no weather case without wind"));
    assert!(
        sheets(&chords, &o, &page_format("A1").unwrap())[0]
            .warnings
            .iter()
            .any(|w| w.contains("straight chord"))
    );
    let at = value_at(&side(&chords, true).unwrap().plan, 150.0).unwrap();
    assert!((at - 6.0).abs() < 0.01, "{at}");
}

/// The outermost of the lowest arm, and the circuits ticked: a third
/// wire 3 m outside S1 at its height takes the right line; unticked, the
/// line goes back to S1; 2 m higher it is not on the lowest arm; alone
/// ticked, the left side has no line.
#[test]
fn the_lowest_arm_and_the_circuits_ticked() {
    let (.., terrain) = solved();
    let line = with_outer(0.0);
    let shapes = solve(&line);
    assert_eq!(
        circuits(&line),
        [
            ("Example 1".to_owned(), true),
            ("Branch".to_owned(), true),
            ("Outer".to_owned(), true)
        ]
    );
    let all = options("profile_lines = \"lowest_arm\"\n");
    let d = drawn(&line, &shapes, Some(&shapes), &terrain, &all);
    follows(&d, side(&d, true).unwrap(), "S4", 0.0, &terrain);
    follows(&d, side(&d, false).unwrap(), "S2", 0.0, &terrain);
    // The export's ticks over the template's.
    let unticked = Options {
        profile_circuits: Some(vec!["Example 1".into()]),
        ..all.clone()
    };
    let d = drawn(&line, &shapes, Some(&shapes), &terrain, &unticked);
    follows(&d, side(&d, true).unwrap(), "S1", 0.0, &terrain);
    // The template's list: the outer circuit alone has no wire on the left.
    let alone = options("profile_lines = \"lowest_arm\"\nprofile_circuits = [\"Outer\"]\n");
    let d = drawn(&line, &shapes, Some(&shapes), &terrain, &alone);
    follows(&d, side(&d, true).unwrap(), "S4", 0.0, &terrain);
    assert!(side(&d, false).is_none());
    // No circuit ticked: no line, said on the sheets.
    let none = options("profile_lines = \"lowest_arm\"\nprofile_circuits = []\n");
    let d = drawn(&line, &shapes, Some(&shapes), &terrain, &none);
    assert!(d.profile_lines.is_empty());
    assert!(d.profile_line_notes[0].contains("no conductor"));
    // 2 m above S1's arm: beyond the tolerance, S1 is the lowest arm's.
    let line = with_outer(2.0);
    let shapes = solve(&line);
    let d = drawn(&line, &shapes, Some(&shapes), &terrain, &all);
    follows(&d, side(&d, true).unwrap(), "S1", 0.0, &terrain);
    // 0.3 m above: within it, the outer wire again.
    let line = with_outer(0.3);
    let shapes = solve(&line);
    let d = drawn(&line, &shapes, Some(&shapes), &terrain, &all);
    follows(&d, side(&d, true).unwrap(), "S4", 0.0, &terrain);
}

/// A bundle counts to its outer subconductor: S1 as a twin bundle 0.4 m
/// apart moves the right line 0.2 m outwards.
#[test]
fn a_bundle_counts_to_its_outer_subconductor() {
    let (mut line, _, _, terrain) = solved();
    let s1 = line.sections.iter_mut().find(|s| s.id == "S1").unwrap();
    s1.bundle = 2;
    s1.bundle_spacing = Some(0.4);
    let shapes = solve(&line);
    let o = options("profile_lines = \"lowest_arm\"\n");
    let d = drawn(&line, &shapes, Some(&shapes), &terrain, &o);
    follows(&d, side(&d, true).unwrap(), "S1", 0.2, &terrain);
    follows(&d, side(&d, false).unwrap(), "S2", 0.0, &terrain);
}

/// The template's keys read (and again from its own text), a template
/// without them as before, and what is refused.
#[test]
fn the_templates_keys() {
    let t = template(
        "width = \"50 m\"\nprofile_lines = \"lowest_arm\"\nprofile_distance = \"10 m\"\n\
         profile_circuits = [\"North\", \"South\"]\n",
    );
    let c = &t.corridor;
    assert_eq!(c.profile_lines, "lowest_arm");
    assert_eq!(c.profile_distance.map(|d| d.0), Some(10.0));
    assert_eq!(
        c.profile_circuits.as_deref(),
        Some(&["North".to_owned(), "South".to_owned()][..])
    );
    assert_eq!(SheetTemplate::parse(&t.source, None, &[]).unwrap(), *t);
    let old = template("width = \"50 m\"\n");
    assert_eq!(old.corridor.profile_line_mode(), ProfileLines::Corridor);
    assert_eq!(old.corridor.profile_circuits, None);
    assert_eq!(
        SheetTemplate::builtin().corridor.profile_line_mode(),
        ProfileLines::Corridor
    );
    let refused = |corridor: &str| {
        SheetTemplate::parse(&format!("{HEAD}[corridor]\n{corridor}"), None, &[]).unwrap_err()
    };
    assert_eq!(
        refused("profile_lines = \"arm\"\n"),
        "corridor.profile_lines: profile lines \"arm\": give corridor, distance or lowest_arm"
    );
    assert!(refused("profile_lines = \"distance\"\n").contains("give the distance"));
    assert!(
        refused("profile_lines = \"distance\"\nprofile_distance = \"-3 m\"\n")
            .contains("positive distance")
    );
    assert!(refused("profile_distance = \"0 m\"\n").contains("positive distance"));
    // The legend's words are the template's.
    let words = Arc::new(
        SheetTemplate::parse(
            &format!(
                "{HEAD}[legend.labels]\nprofile_distance = \"Gelände {{distance}} m {{side}}\"\n\
                 profile_lowest_arm = \"Gelände unter dem Leiter ({{side}})\"\n"
            ),
            None,
            &[],
        )
        .unwrap(),
    );
    assert_eq!(
        words
            .legend
            .label("profile_distance", &[("side", "left"), ("distance", "10")]),
        "Gelände 10 m left"
    );
}

/// The still-air case: the case drawn when it has no wind, else the
/// first without; none when every case has wind.
#[test]
fn the_still_air_case() {
    assert_eq!(still_case(&[true, false, false], 2), Some(2));
    assert_eq!(still_case(&[true, false, false], 0), Some(1));
    assert_eq!(still_case(&[true, true], 0), None);
    assert_eq!(still_case(&[], 0), None);
}
