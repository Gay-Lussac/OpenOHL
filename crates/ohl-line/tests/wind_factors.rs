// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A section's local winds (line file version 9, roadmap O15): a line
//! without them is written as before (version 7, or 8 with strings' own
//! volumes), with them as version 9, read back the same; bad ones are
//! refused; a structure put into a span or taken out leaves the other
//! spans' factors on their spans. Factors INVENTED.

use ohl_line::{LINE_FILE_VERSION, Line, SpanWind};

fn example() -> Line {
    Line::load(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/synthetic-line/synthetic-line.ohl.json"
    )))
    .unwrap()
}

#[test]
fn local_winds_raise_the_version_only_when_given() {
    let mut line = example();
    let text = line.to_json();
    assert!(text.contains("\"version\": 7,"), "{}", &text[..80]);
    assert!(!text.contains("wind_factors"));
    line.sections[0].set_wind_factor(0, 1, 1.2);
    let text = line.to_json();
    assert!(text.contains("\"version\": 9,"), "{}", &text[..80]);
    let back = Line::from_json(&text).unwrap();
    assert_eq!(back.version, LINE_FILE_VERSION);
    assert_eq!(back.sections[0].wind_factors, line.sections[0].wind_factors);
    assert_eq!(back, line);
}

#[test]
fn bad_local_winds_are_refused() {
    let n = example().sections[0].points.len() - 1;
    for (span, factor) in [(n, 1.2), (0, 10.5), (0, -0.1), (0, f64::NAN)] {
        let mut line = example();
        line.sections[0].wind_factors = vec![SpanWind { span, factor }];
        let e = line.validate().unwrap_err().to_string();
        assert!(
            e.contains("local wind") || e.contains("wind_factors"),
            "{e}"
        );
    }
    let mut line = example();
    line.sections[0].wind_factors = vec![
        SpanWind {
            span: 0,
            factor: 1.2,
        },
        SpanWind {
            span: 0,
            factor: 0.8,
        },
    ];
    assert!(line.validate().is_err());
    // An unknown key in a factor is refused, naming it.
    let mut text = example().to_json();
    text = text.replacen(
        "\"stringing\"",
        "\"wind_factors\": [{\"span\": 0, \"factor\": 1.2, \"pressure\": 3}],\n      \"stringing\"",
        1,
    );
    let e = Line::from_json(&text).unwrap_err().to_string();
    assert!(e.contains("pressure"), "{e}");
}

#[test]
fn local_winds_keep_their_spans_when_the_points_change() {
    let mut s = example().sections[0].clone();
    assert!(s.points.len() >= 3);
    s.set_wind_factor(0, 0, 1.3);
    s.set_wind_factor(1, 1, 0.7);
    // The first structure taken out: span 1 becomes span 0, span 0 is
    // gone.
    let points = s.points[1..].to_vec();
    s.set_points(points).unwrap();
    assert_eq!(
        s.wind_factors,
        vec![SpanWind {
            span: 0,
            factor: 0.7
        }]
    );
}
