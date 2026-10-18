// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! OpenOHL design-code plugin (`code-plugin` world): IEC 60826, design
//! criteria of overhead transmission lines (reliability-based design).
//!
//! Method: the climatic loads have a return period set by the reliability
//! level; the reference wind speed gives the dynamic pressure
//! q0 = ½·τ·μ·(K_R·V_R)², on conductors q0·C_xc·G_c·G_L; ice is a weight
//! per metre with a density; ice and wind combine as a high-probability
//! ice with a low wind and a low ice with a high wind. Use factors γ
//! bring the loads to the return period, the global strength factor φ_R
//! goes to the strength. Every number comes from the parameter file (see
//! `example-parameters.toml` and the README).
//!
//! Build with `cargo build --release --target wasm32-wasip2`.

wit_bindgen::generate!({
    world: "code-plugin",
    path: "../../../crates/ols-plugins/wit",
});

use ols_code_common::{Cases, Condition, ExtraLoad, ExtraWeather, Load, Weather, check};
use serde::Deserialize;

const NAME: &str = "iec60826";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    project: Project,
    wind: Wind,
    ice: Ice,
    combined: Combined,
    temperatures: Temperatures,
    factors: Factors,
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
    reliability_level: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wind {
    reference_speed: f64,
    roughness_factor: f64,
    air_density_correction: f64,
    air_density: f64,
    drag_factor: f64,
    gust_factor: f64,
    span_factor: f64,
    temperature: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ice {
    weight: f64,
    density: f64,
    temperature: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Combined {
    high_ice_factor: f64,
    low_wind_factor: f64,
    low_ice_factor: f64,
    high_wind_factor: f64,
    drag_factor: f64,
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
    strength: f64,
}

fn cases(text: &str) -> Result<Cases, String> {
    let p: Params = ols_code_common::parse(text)?;
    let w = &p.wind;
    check("wind.reference_speed", w.reference_speed, 0.0, 100.0)?;
    check("wind.roughness_factor", w.roughness_factor, 0.0, 5.0)?;
    check(
        "wind.air_density_correction",
        w.air_density_correction,
        0.1,
        2.0,
    )?;
    check("wind.air_density", w.air_density, 0.5, 2.0)?;
    for (what, x) in [
        ("wind.drag_factor", w.drag_factor),
        ("wind.gust_factor", w.gust_factor),
        ("wind.span_factor", w.span_factor),
        ("combined.drag_factor", p.combined.drag_factor),
    ] {
        check(what, x, 0.0, 5.0)?;
    }
    check("ice.weight", p.ice.weight, 0.0, 1000.0)?;
    check("ice.density", p.ice.density, 1.0, 1000.0)?;
    let k = &p.combined;
    for (what, x) in [
        ("combined.high_ice_factor", k.high_ice_factor),
        ("combined.low_wind_factor", k.low_wind_factor),
        ("combined.low_ice_factor", k.low_ice_factor),
        ("combined.high_wind_factor", k.high_wind_factor),
    ] {
        check(what, x, 0.0, 2.0)?;
    }
    let f = &p.factors;
    for (what, x) in [
        ("factors.weight", f.weight),
        ("factors.wind", f.wind),
        ("factors.ice", f.ice),
        ("factors.strength", f.strength),
    ] {
        check(what, x, 0.1, 10.0)?;
    }

    // Pressure on conductors at a fraction of V_R with a drag factor.
    let pressure = |speed_factor: f64, drag: f64| {
        let v = w.roughness_factor * w.reference_speed * speed_factor;
        0.5 * w.air_density_correction
            * w.air_density
            * v
            * v
            * drag
            * w.gust_factor
            * w.span_factor
    };
    let q = 0.5 * w.air_density_correction * w.air_density;
    let tags = ["loads", "iec60826"];
    let mut c = Cases::new(
        &p.project.name,
        NAME,
        env!("CARGO_PKG_VERSION"),
        "IEC 60826 reliability-based loading",
    );
    let wind = Weather::new("IEC wind", w.temperature)
        .describe("reference wind on bare conductors")
        .wind(pressure(1.0, w.drag_factor))
        .air_density_factor(q)
        .tags(&tags);
    let ice = Weather::new("IEC ice", p.ice.temperature)
        .describe("reference ice, no wind")
        .ice_load(p.ice.weight, Some(p.ice.density))
        .tags(&tags);
    let high_ice = Weather::new("IEC high ice with low wind", k.temperature)
        .ice_load(p.ice.weight * k.high_ice_factor, Some(p.ice.density))
        .wind(pressure(k.low_wind_factor, k.drag_factor))
        .air_density_factor(q)
        .tags(&tags);
    let high_wind = Weather::new("IEC low ice with high wind", k.temperature)
        .ice_load(p.ice.weight * k.low_ice_factor, Some(p.ice.density))
        .wind(pressure(k.high_wind_factor, k.drag_factor))
        .air_density_factor(q)
        .tags(&tags);
    let cold = Weather::new("IEC minimum temperature", p.temperatures.minimum)
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
            "IEC wind",
            &wind,
            f.weight,
            f.wind,
            f.wind,
            "γ_W on the wind",
        ),
        load(
            "IEC ice",
            &ice,
            f.ice,
            f.ice,
            f.ice,
            "γ_I on weight and ice",
        ),
        load(
            "IEC high ice with low wind",
            &high_ice,
            f.ice,
            f.ice,
            f.ice,
            "γ_I, ice governing",
        ),
        load(
            "IEC low ice with high wind",
            &high_wind,
            f.weight,
            f.wind,
            f.wind,
            "γ_W, wind governing",
        ),
        load(
            "IEC minimum temperature",
            &cold,
            f.weight,
            f.weight,
            f.weight,
            "unfactored",
        ),
    ];
    c.weather = vec![wind, ice, high_ice, high_wind, cold];
    if let Some(level) = p.project.reliability_level {
        c.notes.push(format!(
            "reliability level {level}: the factors of the file are taken as that level's"
        ));
    }
    c.notes.push(format!(
        "wind on conductors {:.1} Pa (q0 = {:.1} Pa × C_xc·G_c·G_L)",
        c.weather[0].wind_pressure,
        pressure(1.0, 1.0) / (w.gust_factor * w.span_factor)
    ));
    c.notes.push(format!(
        "global strength factor φ_R = {} in every load case",
        f.strength
    ));
    c.notes.push(
        "not generated: security (failure containment) and safety (construction and \
         maintenance) loads — use the broken-span solve and project load cases"
            .into(),
    );
    c.add_extra(&p.extra_weather, &p.extra_load_case);
    Ok(c)
}

ols_code_common::code_plugin!(
    NAME,
    "IEC 60826: reliability-based wind, ice and combined ice-wind cases with use factors",
    cases
);
