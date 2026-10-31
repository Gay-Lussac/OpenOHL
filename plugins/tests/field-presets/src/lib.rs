// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Test plugin (`field-preset-plugin` world): **invented**, fixed field
//! presets for the synthetic double-circuit example
//! (`examples/electrical/double-circuit.ohl.json`, structures T1–T2–T3)
//! for the tests (`ohl-plugins`, `ohl electrical magnetic|electric
//! --preset-plugin`) — not a building analysis. Preset `default`: a cut
//! labelled `house-A` in the middle of T1–T2 and one labelled `house-B`
//! at T2–T3's lowest point, points `house-A` (4 m above the ground) and
//! `house-B` (at 405 m), levels 1 µT and 0.5 kV/m, one outline; its notes
//! echo what it received. Preset `points-only`: the two points alone.
//!
//! The parameters choose a fault for the host's checks: `bad = "span"`
//! (a cut in T1–T3, no span), `bad = "fraction"` (1.5), `bad = "name"`
//! (a point named twice), `bad = "file"` (a report file `../x.txt`),
//! `bad = "fail"` (its own error).
//!
//! `report` writes `summary.txt`: the preset's name, its counts, the
//! results' size and whether they are `ohl-field-results`, and how many
//! times each cut's label appears in them.

wit_bindgen::generate!({
    inline: r#"
        package local:field-presets;
        world test {
            include ohl:plugin/field-preset-plugin@0.1.0;
        }
    "#,
    path: "../../../crates/ohl-plugins/wit",
    world: "local:field-presets/test",
    generate_all,
});

use exports::ohl::plugin::field_presets::{Cut, FieldInput, FieldPreset, OutFile, Outline, Place};

struct Presets;

impl Guest for Presets {
    fn describe() -> PluginInfo {
        PluginInfo {
            name: "field-presets".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: "Test plugin: field presets (invented)".into(),
        }
    }
}

/// The fault the parameters ask for (`bad = "…"`), if any.
fn bad(parameters: &str) -> Option<&str> {
    parameters.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == "bad").then(|| v.trim().trim_matches('"'))
    })
}

fn cut(label: &str, from: &str, to: &str, fraction: Option<f64>) -> Cut {
    Cut {
        label: label.into(),
        from: from.into(),
        to: to.into(),
        fraction,
        width: 20.0,
        above: 5.0,
        step: 2.0,
    }
}

impl exports::ohl::plugin::field_presets::Guest for Presets {
    fn parameters_template() -> String {
        "# Invented test parameters\n# bad = \"span\"   # span, fraction, name, file or fail\n"
            .into()
    }

    fn field_presets(input: FieldInput) -> Result<Vec<FieldPreset>, String> {
        let fault = bad(&input.parameters);
        if fault == Some("fail") {
            return Err("asked to fail".into());
        }
        let coded = input.points.iter().filter(|p| !p.code.is_empty()).count();
        let classes: Vec<String> = {
            let mut c: Vec<u8> = input
                .points
                .iter()
                .filter(|p| p.code.is_empty())
                .map(|p| p.class)
                .collect();
            c.sort_unstable();
            c.dedup();
            c.iter().map(u8::to_string).collect()
        };
        let notes = vec![
            format!(
                "received {} points ({coded} survey, classes {}), {} ground points",
                input.points.len(),
                if classes.is_empty() {
                    "none".to_owned()
                } else {
                    classes.join(" ")
                },
                input.ground.len() / 3
            ),
            format!(
                "model: {}",
                if input.model.contains("\"ohl-export-model\"") {
                    "ohl-export-model"
                } else {
                    "unknown"
                }
            ),
        ];
        let mut cuts = vec![
            cut("house-A", "T1", "T2", Some(0.5)),
            cut("house-B", "T2", "T3", None),
        ];
        let mut points = vec![
            Place {
                name: "house-A".into(),
                x: 150.0,
                y: -30.0,
                z: None,
                height: Some(4.0),
            },
            Place {
                name: "house-B".into(),
                x: 450.0,
                y: 25.0,
                z: Some(405.0),
                height: None,
            },
        ];
        match fault {
            Some("span") => cuts.push(cut("x", "T1", "T3", None)),
            Some("fraction") => cuts[0].fraction = Some(1.5),
            Some("name") => points[1].name = "house-A".into(),
            _ => {}
        }
        Ok(vec![
            FieldPreset {
                name: "default".into(),
                description: "Two invented houses beside the line".into(),
                cuts,
                points: points.clone(),
                levels_ut: vec![1.0],
                levels_kv_m: vec![0.5],
                outlines: vec![Outline {
                    id: "house-A".into(),
                    ring: vec![145.0, -35.0, 155.0, -35.0, 155.0, -25.0, 145.0, -25.0],
                    ground: 400.0,
                    top: 408.0,
                }],
                notes,
            },
            FieldPreset {
                name: "points-only".into(),
                description: "The two invented houses' points alone".into(),
                cuts: Vec::new(),
                points,
                levels_ut: Vec::new(),
                levels_kv_m: Vec::new(),
                outlines: Vec::new(),
                notes: Vec::new(),
            },
        ])
    }

    fn report(
        parameters: String,
        preset: FieldPreset,
        results: String,
    ) -> Result<Vec<OutFile>, String> {
        let mut text = format!(
            "preset {}\ncuts {}\npoints {}\nresults bytes {}\nohl-field-results {}\n",
            preset.name,
            preset.cuts.len(),
            preset.points.len(),
            results.len(),
            results.contains("\"ohl-field-results\"")
        );
        for c in &preset.cuts {
            let n = results
                .matches(&format!("\"label\":\"{}\"", c.label))
                .count();
            text.push_str(&format!("label {} {n}\n", c.label));
        }
        let name = if bad(&parameters) == Some("file") {
            "../x.txt"
        } else {
            "summary.txt"
        };
        Ok(vec![OutFile {
            name: name.into(),
            media_type: "text/plain".into(),
            bytes: text.into_bytes(),
        }])
    }
}

export!(Presets);
