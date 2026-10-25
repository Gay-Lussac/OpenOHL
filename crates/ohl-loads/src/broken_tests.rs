// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The broken-wire load cases (roadmap O9) and the local winds in the
//! structure loads (roadmap O15).

use std::path::Path;

use super::*;

fn library() -> ohl_cables::Library {
    ohl_cables::Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"))
        .unwrap()
}

/// Three level 300 m spans, a suspension string at T2 and T3 (3 m,
/// 800 N), the dead-end wire points fixed at T1 and T4; strung at 28 kN
/// at 15 °C. INVENTED.
fn three_spans() -> Line {
    Line::from_json(
        r#"{
  "format": "ohl-line", "version": 6, "name": "three spans", "crs": "local",
  "structures": [
    {"id": "T1", "position": [0, 0, 0], "orientation_deg": -90,
     "wires": [{"label": "W1", "position": [0, 0, 27], "set": 1}]},
    {"id": "T2", "position": [300, 0, 0], "orientation_deg": -90,
     "points": [{"label": "A1", "position": [300, 0, 30]}],
     "strings": [{"label": "S1", "top": "A1", "wire": "W1", "length": 3.0,
                  "weight": 800.0, "stiffness": 5e7}],
     "wires": [{"label": "W1", "position": [300, 0, 27], "set": 1}]},
    {"id": "T3", "position": [600, 0, 0], "orientation_deg": -90,
     "points": [{"label": "A1", "position": [600, 0, 30]}],
     "strings": [{"label": "S1", "top": "A1", "wire": "W1", "length": 3.0,
                  "weight": 800.0, "stiffness": 5e7}],
     "wires": [{"label": "W1", "position": [600, 0, 27], "set": 1}]},
    {"id": "T4", "position": [900, 0, 0], "orientation_deg": -90,
     "wires": [{"label": "W1", "position": [900, 0, 27], "set": 1}]}
  ],
  "sections": [{"id": "S", "cable": "acsr-drake-26-7", "bundle": 1,
    "points": [{"structure": "T1", "set": 1}, {"structure": "T2", "set": 1},
               {"structure": "T3", "set": 1}, {"structure": "T4", "set": 1}],
    "stringing": {"kind": "tension", "horizontal_tension": 20000.0, "temperature": 15.0}}]
}"#,
    )
    .unwrap()
}

fn weather() -> Vec<WeatherCase> {
    ohl_weather::WeatherCases::parse(
        "[[case]]\nname = \"calm\"\ntemperature = \"10 °C\"\n\
         [[case]]\nname = \"wind\"\ntemperature = \"10 °C\"\nwind_pressure = \"500 Pa\"\n",
    )
    .unwrap()
    .cases
}

fn at<'a>(c: &'a CaseLoads, structure: &str) -> &'a AttachmentLoad {
    c.loads
        .iter()
        .find(|l| l.structure == structure && !l.point.is_empty())
        .unwrap_or_else(|| panic!("no load at {structure}"))
}

/// `[case.broken]`: read (a span from 1, or "every"; a set), checked,
/// written at version 3 only when a case has one.
#[test]
fn broken_wires_are_read_and_checked() {
    let text = "format = \"ohl-load-cases\"\nversion = 3\n[[case]]\nname = \"b\"\nweather = \"w\"\n\
                [case.broken]\nsection = \"S3\"\nspan = 2\nset = 1\n\
                [[case]]\nname = \"e\"\nweather = \"w\"\n\
                [case.broken]\nsection = \"S3\"\nspan = \"every\"\n";
    let c = LoadCases::parse(text).unwrap();
    let b = c.cases[0].broken.as_ref().unwrap();
    assert_eq!(
        (b.section.as_str(), b.span, b.set),
        ("S3", BrokenSpan::One(2), Some(1))
    );
    assert_eq!(b.describe(), "S3 span 2 set 1 broken");
    let e = c.cases[1].broken.as_ref().unwrap();
    assert_eq!(e.span, BrokenSpan::Every);
    assert_eq!(e.describe(), "S3 every span broken, one at a time");
    assert_eq!(c.written_version(), 3);
    // Version 2 files read as before; version 3 is refused by version 2
    // programs ("newer").
    for bad in [
        "[[case]]\nname = \"a\"\nweather = \"w\"\n[case.broken]\nsection = \"S\"\nspan = 0\n",
        "[[case]]\nname = \"a\"\nweather = \"w\"\n[case.broken]\nsection = \"S\"\nspan = \"all\"\n",
        "[[case]]\nname = \"a\"\nweather = \"w\"\n[case.broken]\nsection = \"\"\nspan = 1\n",
        "[[case]]\nname = \"a\"\nweather = \"w\"\n[case.broken]\nsection = \"S\"\n",
        "[[case]]\nname = \"a\"\nweather = \"w\"\n[case.broken]\nsection = \"S\"\nspan = 1\nphase = 2\n",
        "version = 4\n[[case]]\nname = \"a\"\nweather = \"w\"\n",
    ] {
        assert!(LoadCases::parse(bad).is_err(), "{bad}");
    }
    // Against the line: the section, the span and the set.
    let line = three_spans();
    let wire = |span: BrokenSpan, set: Option<u32>| BrokenWire {
        section: "S".into(),
        span,
        set,
    };
    assert_eq!(wire(BrokenSpan::One(2), Some(1)).spans(&line), Ok(vec![1]));
    assert_eq!(
        wire(BrokenSpan::Every, None).spans(&line),
        Ok(vec![0, 1, 2])
    );
    assert!(wire(BrokenSpan::One(4), None).spans(&line).is_err());
    assert!(wire(BrokenSpan::One(1), Some(2)).spans(&line).is_err());
    let none = BrokenWire {
        section: "X".into(),
        ..wire(BrokenSpan::One(1), None)
    };
    assert!(none.spans(&line).unwrap_err().contains("no section X"));
}

/// A wire broken in span 2: T2's string swings back, T3's ahead; each
/// structure takes the pull of the span left on its side along the line,
/// unbalanced (the intact case's balance at both is nearly 0); T1 and T4
/// as their remaining span pulls. In "every" span, each component's
/// largest with the span giving it. A section not in the line fails the
/// case.
#[test]
fn a_broken_wire_gives_the_unbalanced_longitudinal_loads() {
    let line = three_spans();
    let lib = library();
    let weather = weather();
    let budget = default_budget();
    let whole = LoadCase::from_weather("calm");
    let mut broken = LoadCase {
        name: "broken".into(),
        longitudinal: 1.5,
        broken: Some(BrokenWire {
            section: "S".into(),
            span: BrokenSpan::One(2),
            set: Some(1),
        }),
        ..LoadCase::from_weather("calm")
    };
    let mut every = broken.clone();
    every.name = "every".into();
    every.broken.as_mut().unwrap().span = BrokenSpan::Every;
    let r = structure_loads(
        &line,
        &lib,
        &weather,
        &[whole, broken.clone(), every],
        &budget,
    )
    .unwrap();
    for c in &r {
        assert!(c.failed.is_empty(), "{}: {:?}", c.case.name, c.failed);
    }
    let (w, b, e) = (&r[0], &r[1], &r[2]);
    // Whole: balanced along the line at T2 and T3.
    assert!(at(w, "T2").local[1].abs() < 1.0, "{:?}", at(w, "T2").local);
    // Broken in span 2: T2 pulled back (towards T1: −ahead), T3 ahead.
    let (t2, t3) = (at(b, "T2"), at(b, "T3"));
    assert!(t2.local[1] < -1000.0, "{:?}", t2.local);
    assert!(t3.local[1] > 1000.0, "{:?}", t3.local);
    assert!((t2.local[1] + t3.local[1]).abs() < 1e-3 * t3.local[1]);
    assert!((t2.factored[1] - 1.5 * t2.local[1]).abs() < 1e-6);
    assert_eq!(t2.broken_span, Some([2, 2, 2]));
    // T1 still pulled ahead by span 1 (less than whole: the clamp at T2
    // came closer); the loads at T1 and T2 along the line balance.
    let t1 = at(b, "T1");
    assert!(t1.local[1] > 1000.0 && t1.local[1] < at(w, "T1").local[1]);
    assert!(
        (t1.local[1] + t2.local[1]).abs() < 1.0,
        "{} {}",
        t1.local[1],
        t2.local[1]
    );
    // Every span: at T2 the largest pull along the line is a break next
    // to it (span 1 or 2), and its vertical is one of the spans' too.
    let te = at(e, "T2");
    let [_, along, _] = te.broken_span.unwrap();
    assert!(along == 1 || along == 2, "{along}");
    assert!(te.factored[1].abs() >= t2.factored[1].abs() - 1e-6);
    // The CSV says which span.
    let csv = to_csv(&r);
    assert!(csv.lines().next().unwrap().ends_with(",broken_span"));
    assert!(
        csv.lines()
            .any(|l| l.starts_with("broken,T2,") && l.ends_with(",2 2 2"))
    );
    assert!(
        csv.lines()
            .any(|l| l.starts_with("calm,T2,") || l.starts_with("weather: calm,T2,"))
    );
    // A section not in the line fails the case.
    broken.broken.as_mut().unwrap().section = "X".into();
    let err = structure_loads(&line, &lib, &weather, &[broken], &budget).unwrap_err();
    assert!(err.contains("no section X"), "{err}");
}

/// Local winds in the structure loads: a factor on a span changes the
/// wind its structures take across the line by the wind span it adds.
#[test]
fn local_winds_change_the_loads_across_the_line() {
    let mut line = three_spans();
    let lib = library();
    let weather = weather();
    let budget = default_budget();
    let case = [LoadCase::from_weather("wind")];
    let base = structure_loads(&line, &lib, &weather, &case, &budget).unwrap();
    line.sections[0].set_wind_factor(1, 1, 1.5);
    let more = structure_loads(&line, &lib, &weather, &case, &budget).unwrap();
    let across = |r: &[CaseLoads], s: &str| at(&r[0], s).local[0].abs();
    // T2 and T3 take half of span 2 each: about a quarter more.
    for s in ["T2", "T3"] {
        let ratio = across(&more, s) / across(&base, s);
        assert!(ratio > 1.15 && ratio < 1.35, "{s}: {ratio}");
    }
    // T1 and T4 only see span 1 and 3 (and the swing of the strings).
    for s in ["T1", "T4"] {
        let ratio = across(&more, s) / across(&base, s);
        assert!((ratio - 1.0).abs() < 0.05, "{s}: {ratio}");
    }
}
