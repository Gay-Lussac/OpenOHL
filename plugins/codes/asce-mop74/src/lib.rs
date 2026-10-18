// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! OpenOHL design-code plugin (`code-plugin` world): ASCE Manual of
//! Practice 74, guidelines for electrical transmission line structural
//! loading (reliability-based).
//!
//! Method: extreme wind on wires with the pressure Q·k_z·K_zt·V²·G_w·C_f
//! (wind speed of the chosen return period), ice with a concurrent wind,
//! and a minimum temperature case; load factors γ adjust to the return
//! period, a strength factor goes to the structures. Every number comes
//! from the parameter file (see `example-parameters.toml` and the README).
//!
//! Build with `cargo build --release --target wasm32-wasip2`.

wit_bindgen::generate!({
    world: "code-plugin",
    path: "../../../crates/ols-plugins/wit",
});

use ols_code_common::{Cases, Condition, ExtraLoad, ExtraWeather, Load, Weather, check};
use serde::Deserialize;

const NAME: &str = "asce-mop74";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    project: Project,
    wind: Wind,
    ice: Ice,
    temperatures: Temperatures,
    load_factors: Factors,
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
    #[serde(default)]
    return_period: Option<f64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wind {
    speed: f64,
    air_density_factor: f64,
    exposure_factor: f64,
    topographic_factor: f64,
    gust_response_factor: f64,
    force_coefficient: f64,
    temperature: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ice {
    thickness: f64,
    density: f64,
    concurrent_wind_speed: f64,
    force_coefficient: f64,
    temperature: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Temperatures {
    minimum: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Factors {
    weight: f64,
    wind: f64,
    ice: f64,
    concurrent_wind: f64,
    strength: f64,
}

fn cases(text: &str) -> Result<Cases, String> {
    let p: Params = ols_code_common::parse(text)?;
    let w = &p.wind;
    check("wind.speed", w.speed, 0.0, 120.0)?;
    check("wind.air_density_factor", w.air_density_factor, 0.1, 2.0)?;
    for (what, x) in [
        ("wind.exposure_factor", w.exposure_factor),
        ("wind.topographic_factor", w.topographic_factor),
        ("wind.gust_response_factor", w.gust_response_factor),
        ("wind.force_coefficient", w.force_coefficient),
        ("ice.force_coefficient", p.ice.force_coefficient),
    ] {
        check(what, x, 0.0, 5.0)?;
    }
    check("ice.thickness", p.ice.thickness, 0.0, 300.0)?;
    check("ice.density", p.ice.density, 1.0, 1000.0)?;
    check(
        "ice.concurrent_wind_speed",
        p.ice.concurrent_wind_speed,
        0.0,
        120.0,
    )?;
    let f = &p.load_factors;
    for (what, x) in [
        ("weight", f.weight),
        ("wind", f.wind),
        ("ice", f.ice),
        ("concurrent_wind", f.concurrent_wind),
        ("strength", f.strength),
    ] {
        check(&format!("load_factors.{what}"), x, 0.05, 10.0)?;
    }

    let pressure = |v: f64, cf: f64| {
        w.air_density_factor
            * w.exposure_factor
            * w.topographic_factor
            * v
            * v
            * w.gust_response_factor
            * cf
    };
    let tags = ["loads", "asce-mop74"];
    let mut c = Cases::new(
        &p.project.name,
        NAME,
        env!("CARGO_PKG_VERSION"),
        "ASCE MOP 74 structural loading",
    );
    let wind = Weather::new("MOP 74 extreme wind", w.temperature)
        .describe("extreme wind on bare wires")
        .wind(pressure(w.speed, w.force_coefficient))
        .air_density_factor(w.air_density_factor)
        .tags(&tags);
    let ice = Weather::new("MOP 74 ice with concurrent wind", p.ice.temperature)
        .describe("radial ice with the concurrent gust")
        .ice_thickness(p.ice.thickness, p.ice.density)
        .wind(pressure(
            p.ice.concurrent_wind_speed,
            p.ice.force_coefficient,
        ))
        .air_density_factor(w.air_density_factor)
        .tags(&tags);
    let cold = Weather::new("MOP 74 minimum temperature", p.temperatures.minimum)
        .describe("minimum temperature, no wind, no ice")
        .tags(&tags);
    let load = |name: &str, weather: &Weather, v: f64, t: f64, l: f64, text: &str| {
        Load::new(name, weather)
            .describe(text)
            .factors(v, t, l)
            .strength(f.strength)
            .condition(p.project.condition)
            .tags(&tags)
    };
    c.loads = vec![
        load(
            "MOP 74 extreme wind",
            &wind,
            f.weight,
            f.wind,
            f.wind,
            "γ on the extreme wind",
        ),
        load(
            "MOP 74 ice with concurrent wind",
            &ice,
            f.ice,
            f.concurrent_wind,
            f.ice,
            "γ on ice and on the concurrent wind",
        ),
        load(
            "MOP 74 minimum temperature",
            &cold,
            f.weight,
            f.weight,
            f.weight,
            "weight factor",
        ),
    ];
    c.weather = vec![wind, ice, cold];
    if let Some(t) = p.project.return_period {
        c.notes.push(format!(
            "return period {t} years: the speeds, ice and γ of the file are taken as its"
        ));
    }
    c.notes.push(format!(
        "extreme wind on wires {:.1} Pa",
        c.weather[0].wind_pressure
    ));
    c.notes.push(
        "not generated: wind on structures, failure containment (longitudinal) and \
         construction and maintenance loads — use the broken-span solve and project load cases"
            .into(),
    );
    c.add_extra(&p.extra_weather, &p.extra_load_case);
    Ok(c)
}

ols_code_common::code_plugin!(
    NAME,
    "ASCE MOP 74: extreme wind and ice with concurrent wind on wires, with load factors",
    cases
);
