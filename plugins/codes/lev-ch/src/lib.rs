// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! OLS CAD design-code plugin (`code-plugin` world): the load assumptions
//! of the Swiss Leitungsverordnung (LeV / OLEI, SR 734.31) for overhead
//! lines.
//!
//! Method: the normal load case puts an additional (ice / snow) load on
//! the conductors that depends on the conductor diameter and on the
//! altitude zone of the line; a wind case, a reduced combination of both
//! and a minimum temperature case follow. Loads are not factored: the
//! LeV's safety factors divide the strengths (strength factor 1 / S).
//! Every number comes from the parameter file (see
//! `example-parameters.toml` and the README).
//!
//! Build with `cargo build --release --target wasm32-wasip2`.

wit_bindgen::generate!({
    world: "code-plugin",
    path: "../../../crates/ols-plugins/wit",
});

use ols_code_common::{Cases, Condition, ExtraLoad, ExtraWeather, Load, Weather, check};
use serde::Deserialize;

const NAME: &str = "lev-ch";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    project: Project,
    site: Site,
    altitude_zone: Vec<Zone>,
    temperatures: Temperatures,
    load_with_wind: LoadWithWind,
    safety: Safety,
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
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Site {
    altitude: f64,
    reference_diameter: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Zone {
    up_to: f64,
    additional_load_base: f64,
    additional_load_per_mm: f64,
    additional_load_density: f64,
    wind_pressure: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Temperatures {
    additional_load: f64,
    wind: f64,
    minimum: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LoadWithWind {
    load_factor: f64,
    wind_factor: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Safety {
    normal: f64,
}

fn cases(text: &str) -> Result<Cases, String> {
    let p: Params = ols_code_common::parse(text)?;
    check("site.altitude", p.site.altitude, -500.0, 9000.0)?;
    let d = check(
        "site.reference_diameter",
        p.site.reference_diameter,
        1.0,
        200.0,
    )?;
    check(
        "load_with_wind.load_factor",
        p.load_with_wind.load_factor,
        0.0,
        2.0,
    )?;
    check(
        "load_with_wind.wind_factor",
        p.load_with_wind.wind_factor,
        0.0,
        2.0,
    )?;
    let safety = check("safety.normal", p.safety.normal, 1.0, 20.0)?;
    let zone = p
        .altitude_zone
        .iter()
        .find(|z| p.site.altitude <= z.up_to)
        .ok_or_else(|| {
            format!(
                "no altitude zone reaches {} m (the highest up_to)",
                p.site.altitude
            )
        })?;
    let load = zone.additional_load_base + zone.additional_load_per_mm * d;
    check("additional load", load, 0.0, 1000.0)?;
    check(
        "altitude_zone.wind_pressure",
        zone.wind_pressure,
        0.0,
        10_000.0,
    )?;
    check(
        "altitude_zone.additional_load_density",
        zone.additional_load_density,
        1.0,
        1000.0,
    )?;
    let density = Some(zone.additional_load_density);
    let t = &p.temperatures;
    let tags = ["loads", "lev-ch"];

    let mut c = Cases::new(
        &p.project.name,
        NAME,
        env!("CARGO_PKG_VERSION"),
        "Swiss LeV / OLEI load assumptions",
    );
    let normal = Weather::new("LeV normal load (additional load)", t.additional_load)
        .describe("additional load for the reference diameter, no wind")
        .ice_load(load, density)
        .tags(&tags);
    let wind = Weather::new("LeV wind", t.wind)
        .describe("wind on the conductors")
        .wind(zone.wind_pressure)
        .tags(&tags);
    let both = Weather::new("LeV additional load with wind", t.additional_load)
        .describe("reduced additional load with reduced wind")
        .ice_load(load * p.load_with_wind.load_factor, density)
        .wind(zone.wind_pressure * p.load_with_wind.wind_factor)
        .tags(&tags);
    let cold = Weather::new("LeV minimum temperature", t.minimum)
        .describe("minimum temperature, no wind, no additional load")
        .tags(&tags);
    let strength = 1.0 / safety;
    let case = |name: &str, w: &Weather| {
        Load::new(name, w)
            .describe("loads unfactored, strengths / safety factor")
            .strength(strength)
            .condition(p.project.condition)
            .tags(&tags)
    };
    c.loads = vec![
        case("LeV normal load", &normal),
        case("LeV wind", &wind),
        case("LeV additional load with wind", &both),
        case("LeV minimum temperature", &cold),
    ];
    c.weather = vec![normal, wind, both, cold];
    c.notes.push(format!(
        "altitude zone up to {} m: additional load {:.2} N/m for d = {d} mm, wind {:.0} Pa",
        zone.up_to, load, zone.wind_pressure
    ));
    c.notes.push(
        "the additional load is per metre for the reference diameter: wires of another \
         diameter need their own parameter file (or extra weather cases)"
            .into(),
    );
    c.notes.push(format!(
        "strength factor 1/S = {strength:.3} in every load case"
    ));
    c.notes.push(
        "not generated: exceptional load cases (one-sided additional load, broken \
         conductor) — use the broken-span solve and project load cases"
            .into(),
    );
    c.add_extra(&p.extra_weather, &p.extra_load_case);
    Ok(c)
}

ols_code_common::code_plugin!(
    NAME,
    "Swiss LeV / OLEI: additional load by altitude zone and diameter, wind, safety factors",
    cases
);
