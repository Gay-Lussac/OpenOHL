// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Test plugin (`field-preset-plugin` world): **invented**, fixed field
//! presets for the synthetic double-circuit example
//! (`examples/electrical/double-circuit.ohl.json`, structures T1–T2–T3)
//! for the tests (`ohl-plugins`, `ohl electrical magnetic|electric
//! --preset-plugin`) — not a building analysis. Preset `default`: a cut
//! labelled `house-A` at station 150 m of the alignment `main` (the
//! middle of T1–T2) and one labelled `house-B` at 450 m (the middle of
//! T2–T3), points `house-A` (4 m above the ground) and `house-B` (at
//! 405 m), levels 1 µT and 0.5 kV/m, one outline; its notes echo what it
//! received. Preset `points-only`: the two points alone.
//!
//! The parameters choose a fault for the host's checks: `bad =
//! "alignment"` (a cut on an alignment `other`, not in the line), `bad =
//! "station"` (a station of 700 m, beyond the alignment), `bad = "name"`
//! (a point named twice), `bad = "file"` (a report file `../x.txt`),
//! `bad = "fail"` (its own error).
//!
//! `report-cut` writes `cut-LABEL.txt` (whether it is an `ohl-field-cut`,
//! how many fields and grids it holds, whether a plan was given) and
//! gives the label as its summary; `report` writes `summary.txt`: the
//! preset's name, its counts, the results' size, whether they are an
//! `ohl-field-results-set`, how many field documents it holds, whether
//! any grid is left in them, how many cut reports it got, whether a plan
//! was given, and how many cut summaries name each cut.

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

use exports::ohl::plugin::field_presets::{
    Cut, CutReport, FieldInput, FieldPreset, OutFile, Outline, Place,
};

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

fn cut(label: &str, alignment: &str, station: f64) -> Cut {
    Cut {
        label: label.into(),
        alignment: alignment.into(),
        station,
        width: 20.0,
        above: 5.0,
        step: 2.0,
    }
}

impl exports::ohl::plugin::field_presets::Guest for Presets {
    fn parameters_template() -> String {
        "# Invented test parameters\n# bad = \"station\"   # alignment, station, name, file or fail\n"
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
        let mut cuts = vec![cut("house-A", "main", 150.0), cut("house-B", "main", 450.0)];
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
            Some("alignment") => cuts.push(cut("x", "other", 10.0)),
            Some("station") => cuts[0].station = 700.0,
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

    fn report_cut(
        parameters: String,
        plan: String,
        preset: FieldPreset,
        cut: String,
    ) -> Result<CutReport, String> {
        let label = preset
            .cuts
            .iter()
            .map(|c| c.label.as_str())
            .find(|l| cut.contains(&format!("\"label\":\"{l}\"")))
            .unwrap_or("?")
            .to_owned();
        let text = format!(
            "cut {label}\nohl-field-cut {}\nfields {}\nvalues {}\nplan {}\n",
            cut.contains("\"ohl-field-cut\""),
            cut.matches("\"quantity\"").count(),
            cut.matches("\"values\"").count(),
            !plan.is_empty()
        );
        let name = if bad(&parameters) == Some("file") {
            "../x.txt".to_owned()
        } else {
            format!("cut-{label}.txt")
        };
        Ok(CutReport {
            files: vec![OutFile {
                name,
                media_type: "text/plain".into(),
                bytes: text.into_bytes(),
            }],
            summary: label,
        })
    }

    fn report(
        _parameters: String,
        plan: String,
        preset: FieldPreset,
        results: String,
        cuts: Vec<String>,
    ) -> Result<Vec<OutFile>, String> {
        let mut text = format!(
            "preset {}\ncuts {}\npoints {}\nresults bytes {}\nohl-field-results-set {}\nfields {}\n\
             grids {}\ncut reports {}\nplan {}\n",
            preset.name,
            preset.cuts.len(),
            preset.points.len(),
            results.len(),
            results.contains("\"ohl-field-results-set\""),
            results.matches("\"format\":\"ohl-field-results\"").count(),
            results.contains("\"values\""),
            cuts.len(),
            !plan.is_empty()
        );
        for c in &preset.cuts {
            let n = cuts.iter().filter(|s| **s == c.label).count();
            text.push_str(&format!("label {} {n}\n", c.label));
        }
        Ok(vec![OutFile {
            name: "summary.txt".into(),
            media_type: "text/plain".into(),
            bytes: text.into_bytes(),
        }])
    }
}

export!(Presets);
