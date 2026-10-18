// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! OpenOHL design-code plugin (`code-plugin` world): CSA C22.3 No. 1,
//! overhead systems (deterministic loading).
//!
//! Method: the line's loading zone gives the combined ice, wind and
//! temperature; the construction grade gives the overload factors on the
//! vertical, transverse and longitudinal loads and the strength factor; a
//! minimum temperature case completes them. (The reliability-based
//! CAN/CSA-C22.3 No. 60826 follows IEC 60826: use the `iec60826`
//! plugin.) Every number comes from the parameter file (see
//! `example-parameters.toml` and the README).
//!
//! Build with `cargo build --release --target wasm32-wasip2`.

wit_bindgen::generate!({
    world: "code-plugin",
    path: "../../../crates/ols-plugins/wit",
});

use ols_code_common::{Cases, Condition, ExtraLoad, ExtraWeather, Load, Weather, check};
use serde::Deserialize;

const NAME: &str = "csa-c22-3";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    project: Project,
    zone: Vec<Zone>,
    grade: Vec<Grade>,
    temperatures: Temperatures,
    #[serde(default)]
    extra_weather: Vec<ExtraWeather>,
    #[serde(default)]
    extra_load_case: Vec<ExtraLoad>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Project {
    name: String,
    #[serde(default)]
    condition: Condition,
    zone: String,
    grade: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Zone {
    name: String,
    ice_thickness: f64,
    ice_density: f64,
    wind_pressure: f64,
    temperature: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Grade {
    name: String,
    vertical: f64,
    transverse: f64,
    longitudinal: f64,
    strength: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Temperatures {
    minimum: f64,
}

fn cases(text: &str) -> Result<Cases, String> {
    let p: Params = ols_code_common::parse(text)?;
    let zone = p
        .zone
        .iter()
        .find(|z| z.name == p.project.zone)
        .ok_or_else(|| format!("no [[zone]] named \"{}\"", p.project.zone))?;
    let grade = p
        .grade
        .iter()
        .find(|g| g.name == p.project.grade)
        .ok_or_else(|| format!("no [[grade]] named \"{}\"", p.project.grade))?;
    check("zone.ice_thickness", zone.ice_thickness, 0.0, 200.0)?;
    check("zone.ice_density", zone.ice_density, 1.0, 1000.0)?;
    check("zone.wind_pressure", zone.wind_pressure, 0.0, 10_000.0)?;
    for (what, x) in [
        ("vertical", grade.vertical),
        ("transverse", grade.transverse),
        ("longitudinal", grade.longitudinal),
    ] {
        check(&format!("grade.{what}"), x, 0.1, 10.0)?;
    }
    check("grade.strength", grade.strength, 0.05, 1.0)?;

    let tags = ["loads", "csa-c22-3"];
    let mut c = Cases::new(
        &p.project.name,
        NAME,
        env!("CARGO_PKG_VERSION"),
        "CSA C22.3 No. 1 loading zone and construction grade",
    );
    let mut zw = Weather::new(format!("CSA zone ({})", zone.name), zone.temperature)
        .describe("combined ice and wind of the loading zone")
        .wind(zone.wind_pressure)
        .tags(&tags);
    if zone.ice_thickness > 0.0 {
        zw = zw.ice_thickness(zone.ice_thickness, zone.ice_density);
    }
    let cold = Weather::new("CSA minimum temperature", p.temperatures.minimum)
        .describe("minimum temperature, no wind, no ice")
        .tags(&tags);
    c.loads = vec![
        Load::new("CSA zone loading", &zw)
            .describe(format!(
                "zone {}, grade {} overload factors",
                zone.name, grade.name
            ))
            .factors(grade.vertical, grade.transverse, grade.longitudinal)
            .strength(grade.strength)
            .condition(p.project.condition)
            .tags(&tags),
        Load::new("CSA minimum temperature", &cold)
            .describe("unfactored")
            .strength(grade.strength)
            .condition(p.project.condition)
            .tags(&tags),
    ];
    c.weather = vec![zw, cold];
    c.notes.push(format!(
        "zone {}: {} mm ice, {} Pa wind at {} °C; grade {}: factors {} / {} / {}, strength {}",
        zone.name,
        zone.ice_thickness,
        zone.wind_pressure,
        zone.temperature,
        grade.name,
        grade.vertical,
        grade.transverse,
        grade.longitudinal,
        grade.strength
    ));
    c.notes.push(
        "not generated: broken-conductor and unbalanced loads — use the broken-span solve \
         and project load cases"
            .into(),
    );
    c.add_extra(&p.extra_weather, &p.extra_load_case);
    Ok(c)
}

ols_code_common::code_plugin!(
    NAME,
    "CSA C22.3 No. 1: loading zone cases with the construction grade's overload factors",
    cases
);
