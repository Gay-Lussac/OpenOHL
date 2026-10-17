// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Every library format refuses a key it does not know (a typo is said,
//! not silently left out): each shipped or example file reads as it is,
//! and is refused with a mistyped key added at its top and in its first
//! item.

use std::path::{Path, PathBuf};

use ols_structures::LibraryItem;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

type Parse = fn(&str) -> Result<(), String>;

fn formats() -> Vec<(&'static str, Parse)> {
    vec![
        ("libraries/cables/acsr-examples.olsw", |t| {
            ols_cables::Library::default()
                .add_toml(Path::new("x.olsw"), t)
                .map_err(|e| e.to_string())
        }),
        ("libraries/objects/templates.olsload", |t| {
            ols_line::ObjectLibrary::parse(t).map(drop)
        }),
        ("libraries/lidar-classes/asprs-las-1.4.olsc", |t| {
            ols_lidar::ClassScheme::parse(t).map(drop)
        }),
        ("libraries/pylon-templates/danube-2c.olspt", |t| {
            ols_structures::templates::PylonTemplate::parse(Path::new("x.olspt"), t)
                .map(drop)
                .map_err(|e| e.to_string())
        }),
        ("libraries/insulators/example-v-string.olsi", |t| {
            ols_structures::insulators::InsulatorAssembly::parse(Path::new("x.olsi"), t)
                .map(drop)
                .map_err(|e| e.to_string())
        }),
        ("libraries/insulators/example-strain.olsi", |t| {
            ols_structures::insulators::InsulatorAssembly::parse(Path::new("x.olsi"), t)
                .map(drop)
                .map_err(|e| e.to_string())
        }),
        ("libraries/insulators/example-suspension-i.olsi", |t| {
            ols_structures::insulators::InsulatorAssembly::parse(Path::new("x.olsi"), t)
                .map(drop)
                .map_err(|e| e.to_string())
        }),
        ("examples/synthetic-line/Structures/type-01.olss", |t| {
            ols_structures::structures::StructureType::parse(Path::new("x.olss"), t)
                .map(drop)
                .map_err(|e| e.to_string())
        }),
        (
            "examples/synthetic-line/Structures/type-02-model.olsfe",
            |t| ols_structures_fe::model::Model::parse(t).map(drop),
        ),
        (
            "examples/synthetic-line/Structures/steel-pole-30m.olsp",
            |t| ols_structures_fe::pole::Pole::parse(t).map(drop),
        ),
        ("libraries/sections/example-sections.olssec", |t| {
            ols_sections::SectionLibrary::parse(t).map(drop)
        }),
        ("examples/synthetic-line/weather-cases.olswc", |t| {
            ols_weather::WeatherCases::parse(t).map(drop)
        }),
        ("examples/synthetic-line/load-cases.olslc", |t| {
            ols_loads::LoadCases::parse(t).map(drop)
        }),
        ("examples/synthetic-line/clearance-rules.olsr", |t| {
            ols_clearance::rules::RuleSet::parse(t).map(drop)
        }),
        ("examples/synthetic-line/galloping-example.olsg", |t| {
            ols_clearance::galloping::GallopingParams::parse(t).map(drop)
        }),
        ("examples/synthetic-line/thermal-ambient.olsta", |t| {
            ols_thermal::ambient::AmbientCases::parse(t).map(drop)
        }),
        ("examples/synthetic-line/unit-costs.olscost", |t| {
            ols_bom::cost::CostData::from_toml(t).map(drop)
        }),
    ]
}

/// `text` with `typo = 1` after its header (before the first table) and
/// after the first `[[…]]` / `[…]` line.
fn with_typos(text: &str) -> (String, Option<String>) {
    let lines: Vec<&str> = text.lines().collect();
    let first_table = lines
        .iter()
        .position(|l| l.trim_start().starts_with('['))
        .unwrap_or(lines.len());
    let mut top = lines.clone();
    top.insert(first_table, "typo_key = 1");
    let item = (first_table < lines.len()).then(|| {
        let mut v = lines.clone();
        v.insert(first_table + 1, "typo_key = 1");
        v.join("\n")
    });
    (top.join("\n"), item)
}

#[test]
fn every_library_format_refuses_a_mistyped_key() {
    let mut failures = Vec::new();
    for (file, parse) in formats() {
        let text = std::fs::read_to_string(root().join(file)).unwrap();
        if let Err(e) = parse(&text) {
            failures.push(format!("{file}: does not read as it is: {e}"));
            continue;
        }
        let (top, item) = with_typos(&text);
        for (where_, t) in [("at its top", Some(top)), ("in its first item", item)] {
            let Some(t) = t else { continue };
            match parse(&t) {
                Ok(()) => failures.push(format!("{file}: a mistyped key {where_} is not refused")),
                Err(e) if !e.contains("typo_key") => {
                    failures.push(format!("{file}: {where_}: refused without naming it: {e}"))
                }
                Err(_) => {}
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
