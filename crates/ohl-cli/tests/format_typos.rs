// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Every library format refuses a key it does not know (a typo is said,
//! not silently left out): each shipped or example file reads as it is,
//! and is refused with a mistyped key added at its top and in its first
//! item. The structure application's formats (`[[load_case]]`, `.ohlslc`)
//! are checked in `apps/structure/ohl-structure-tests` (core crates do not
//! depend on the application's).

use std::path::{Path, PathBuf};

use ohl_structures::LibraryItem;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

type Parse = fn(&str) -> Result<(), String>;

fn formats() -> Vec<(&'static str, Parse)> {
    vec![
        ("libraries/cables/acsr-examples.ohlw", |t| {
            ohl_cables::Library::check_toml(Path::new("x.ohlw"), t).map_err(|e| e.to_string())
        }),
        ("libraries/objects/templates.ohlload", |t| {
            ohl_line::ObjectLibrary::parse(t).map(drop)
        }),
        ("libraries/lidar-classes/asprs-las-1.4.ohlc", |t| {
            ohl_lidar::ClassScheme::parse(t).map(drop)
        }),
        ("libraries/structure-templates/danube-2c.ohlpt", |t| {
            ohl_structures::templates::PylonTemplate::parse(Path::new("x.ohlpt"), t)
                .map(drop)
                .map_err(|e| e.to_string())
        }),
        ("libraries/insulators/example-two-strings.ohli", |t| {
            ohl_structures::insulators::InsulatorAssembly::parse(Path::new("x.ohli"), t)
                .map(drop)
                .map_err(|e| e.to_string())
        }),
        ("libraries/insulators/example-strain.ohli", |t| {
            ohl_structures::insulators::InsulatorAssembly::parse(Path::new("x.ohli"), t)
                .map(drop)
                .map_err(|e| e.to_string())
        }),
        ("libraries/insulators/example-suspension.ohli", |t| {
            ohl_structures::insulators::InsulatorAssembly::parse(Path::new("x.ohli"), t)
                .map(drop)
                .map_err(|e| e.to_string())
        }),
        ("examples/synthetic-line/Structures/type-01.ohls", |t| {
            ohl_structures::structures::StructureType::parse(Path::new("x.ohls"), t)
                .map(drop)
                .map_err(|e| e.to_string())
        }),
        (
            "examples/synthetic-line/Structures/type-02-model.ohlfe",
            |t| ohl_structures_fe::model::Model::parse(t).map(drop),
        ),
        (
            "examples/synthetic-line/Structures/steel-pole-30m.ohlp",
            |t| ohl_structures_fe::pole::Pole::parse(t).map(drop),
        ),
        ("libraries/sections/example-sections.ohlsec", |t| {
            ohl_sections::SectionLibrary::parse(t).map(drop)
        }),
        ("examples/synthetic-line/weather-cases.ohlwc", |t| {
            ohl_weather::WeatherCases::parse(t).map(drop)
        }),
        ("examples/synthetic-line/load-cases.ohllc", |t| {
            ohl_loads::LoadCases::parse(t).map(drop)
        }),
        ("examples/synthetic-line/clearance-rules.ohlr", |t| {
            ohl_clearance::rules::RuleSet::parse(t).map(drop)
        }),
        ("examples/synthetic-line/galloping-example.ohlg", |t| {
            ohl_clearance::galloping::GallopingParams::parse(t).map(drop)
        }),
        ("examples/synthetic-line/thermal-ambient.ohlta", |t| {
            ohl_thermal::ambient::AmbientCases::parse(t).map(drop)
        }),
        ("examples/synthetic-line/unit-costs.ohlcost", |t| {
            ohl_bom::cost::CostData::from_toml(t).map(drop)
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
