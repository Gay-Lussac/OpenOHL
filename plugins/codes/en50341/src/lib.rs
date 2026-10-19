// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! OpenOHL design-code plugin (`code-wind-plugin` world): EN 50341-1, general
//! approach (Section 4, actions on lines), with a national normative
//! annex (NNA) overlay in the same parameter file.
//!
//! Method (EN 50341-1 §4, EN 1990 §6.4.3.2 for the combinations): the wind
//! pressure on conductors follows from the basic wind velocity through the
//! exposure, structural and drag factors; ice is a load per metre or a
//! radial thickness; the load cases are extreme wind, wind at minimum
//! temperature, ice, combined wind and ice (each action dominant once, the
//! other with its combination factor ψ) and minimum temperature, factored
//! with the partial factors γ. Every number comes from the parameter file
//! (see `example-parameters.toml` and the README).
//!
//! Build with `cargo build --release --target wasm32-wasip2`.

wit_bindgen::generate!({
    world: "code-wind-plugin",
    path: "../../../crates/ohl-plugins/wit",
});

use ohl_code_common::toml;
use ohl_code_common::{
    Cases, Condition, ExtraLoad, ExtraWeather, Load, Weather, check, dynamic_pressure,
};
use serde::Deserialize;

const NAME: &str = "en50341";
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    project: Project,
    wind: Wind,
    ice: Ice,
    ice_wind: IceWind,
    temperatures: Temperatures,
    partial_factors: Factors,
    #[serde(default)]
    nna: Option<Nna>,
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
struct Wind {
    basic_velocity: f64,
    air_density: f64,
    exposure_factor: f64,
    structural_factor: f64,
    drag_factor: f64,
    temperature: f64,
    min_temperature_velocity_factor: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ice {
    #[serde(default)]
    load: Option<f64>,
    #[serde(default)]
    thickness: Option<f64>,
    density: f64,
    temperature: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IceWind {
    ice_factor: f64,
    velocity_factor: f64,
    drag_factor: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Temperatures {
    minimum: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Factors {
    permanent: f64,
    wind: f64,
    ice: f64,
    psi_wind: f64,
    psi_ice: f64,
    material: f64,
}

/// What is left of `[nna]` once its tables replaced the general ones.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Nna {
    country: String,
    #[serde(default)]
    notes: Vec<String>,
    #[serde(default)]
    extra_weather: Vec<ExtraWeather>,
    #[serde(default)]
    extra_load_case: Vec<ExtraLoad>,
}

/// Tables of the general part an NNA may replace.
const SECTIONS: [&str; 6] = [
    "project",
    "wind",
    "ice",
    "ice_wind",
    "temperatures",
    "partial_factors",
];

/// Reads the parameter file, the NNA's tables merged over the general
/// ones; returns the parameters and the keys the NNA replaced.
fn read(text: &str) -> Result<(Params, Vec<String>), String> {
    let mut table = ohl_code_common::parse_table(text)?;
    let mut replaced = Vec::new();
    if let Some(toml::Value::Table(nna)) = table.get_mut("nna") {
        let mut overlay = toml::Table::new();
        for s in SECTIONS {
            if let Some(v) = nna.remove(s) {
                overlay.insert(s.to_owned(), v);
            }
        }
        replaced = ohl_code_common::merge(&mut table, &overlay, "");
    }
    Ok((ohl_code_common::from_table(table)?, replaced))
}

/// The cases of a parameter file.
fn cases(text: &str) -> Result<Cases, String> {
    let (p, replaced) = read(text)?;
    let w = &p.wind;
    check("wind.basic_velocity", w.basic_velocity, 0.0, 100.0)?;
    check("wind.air_density", w.air_density, 0.5, 2.0)?;
    check("wind.exposure_factor", w.exposure_factor, 0.0, 10.0)?;
    check("wind.structural_factor", w.structural_factor, 0.0, 5.0)?;
    check("wind.drag_factor", w.drag_factor, 0.0, 5.0)?;
    check(
        "wind.min_temperature_velocity_factor",
        w.min_temperature_velocity_factor,
        0.0,
        1.0,
    )?;
    check("ice.density", p.ice.density, 1.0, 1000.0)?;
    check("ice_wind.ice_factor", p.ice_wind.ice_factor, 0.0, 2.0)?;
    check(
        "ice_wind.velocity_factor",
        p.ice_wind.velocity_factor,
        0.0,
        1.0,
    )?;
    check("ice_wind.drag_factor", p.ice_wind.drag_factor, 0.0, 5.0)?;
    let f = &p.partial_factors;
    for (what, x) in [
        ("permanent", f.permanent),
        ("wind", f.wind),
        ("ice", f.ice),
        ("material", f.material),
    ] {
        check(&format!("partial_factors.{what}"), x, 0.1, 10.0)?;
    }
    check("partial_factors.psi_wind", f.psi_wind, 0.0, 1.0)?;
    check("partial_factors.psi_ice", f.psi_ice, 0.0, 1.0)?;

    let mut c = Cases::new(
        &p.project.name,
        NAME,
        VERSION,
        "EN 50341-1 general approach with a national annex",
    );
    // Pressure on the conductor at a fraction of v_b with a drag factor.
    let pressure = |velocity_factor: f64, drag: f64| {
        w.exposure_factor
            * dynamic_pressure(w.air_density, velocity_factor * w.basic_velocity)
            * w.structural_factor
            * drag
    };
    let q = 0.5 * w.air_density;
    let ice = |w: Weather, factor: f64| match (p.ice.load, p.ice.thickness) {
        (Some(l), None) => Ok(w.ice_load(l * factor, Some(p.ice.density))),
        (None, Some(t)) => Ok(w.ice_thickness(t * factor, p.ice.density)),
        _ => Err("ice: give either load (N/m) or thickness (mm)".to_owned()),
    };
    let tags = ["loads", "en50341"];

    let wind = Weather::new("EN extreme wind", w.temperature)
        .describe("extreme wind on bare conductors")
        .wind(pressure(1.0, w.drag_factor))
        .air_density_factor(q)
        .tags(&tags);
    let wind_cold = Weather::new("EN wind at minimum temperature", p.temperatures.minimum)
        .describe("reduced wind at the minimum temperature")
        .wind(pressure(w.min_temperature_velocity_factor, w.drag_factor))
        .air_density_factor(q)
        .tags(&tags);
    let iced = ice(
        Weather::new("EN ice", p.ice.temperature).describe("characteristic ice, no wind"),
        1.0,
    )?
    .tags(&tags);
    let ice_wind = ice(
        Weather::new("EN ice with wind", p.ice.temperature)
            .describe("ice with the wind velocity of the combination"),
        p.ice_wind.ice_factor,
    )?
    .wind(pressure(p.ice_wind.velocity_factor, p.ice_wind.drag_factor))
    .air_density_factor(q)
    .tags(&tags);
    let cold = Weather::new("EN minimum temperature", p.temperatures.minimum)
        .describe("minimum temperature, no wind, no ice")
        .tags(&tags);

    let strength = 1.0 / f.material;
    let cond = p.project.condition;
    let load = |name: &str, weather: &Weather, v: f64, t: f64, l: f64, text: &str| {
        Load::new(name, weather)
            .describe(text)
            .factors(v, t, l)
            .strength(strength)
            .condition(cond)
            .tags(&tags)
    };
    c.loads = vec![
        load(
            "EN 1a extreme wind",
            &wind,
            f.permanent,
            f.wind,
            f.wind,
            "γ_G on weight, γ_W on the wind",
        ),
        load(
            "EN 1b wind at minimum temperature",
            &wind_cold,
            f.permanent,
            f.wind,
            f.wind,
            "γ_G on weight, γ_W on the wind",
        ),
        load(
            "EN 2 ice",
            &iced,
            f.ice,
            f.ice,
            f.ice,
            "γ_I on weight and ice",
        ),
        load(
            "EN 3a ice dominant with wind",
            &ice_wind,
            f.ice,
            f.wind * f.psi_wind,
            f.ice,
            "γ_I on ice, γ_W·ψ_W on the accompanying wind",
        ),
        load(
            "EN 3b wind dominant with ice",
            &ice_wind,
            f.ice * f.psi_ice,
            f.wind,
            f.wind,
            "γ_I·ψ_I on the accompanying ice, γ_W on the wind",
        ),
        load(
            "EN 4 minimum temperature",
            &cold,
            f.permanent,
            f.permanent,
            f.permanent,
            "γ_G",
        ),
    ];
    c.weather = vec![wind, wind_cold, iced, ice_wind, cold];
    c.notes.push(format!(
        "extreme wind on bare conductors: {:.1} Pa (q_p = c_e·½ρv_b² = {:.1} Pa, × G_c·C_c)",
        c.weather[0].wind_pressure,
        w.exposure_factor * dynamic_pressure(w.air_density, w.basic_velocity)
    ));
    c.notes.push(format!(
        "strength factor 1/γ_M = {strength:.3} in every load case"
    ));
    c.notes.push(
        "a load case's vertical factor acts on wire weight and ice together: in the ice \
         cases the weight gets γ_I (conservative when γ_I ≥ γ_G)"
            .into(),
    );
    c.notes.push(
        "not generated: security loads (broken wire, unbalanced ice) and construction / \
         maintenance loads — use the broken-span solve and project load cases"
            .into(),
    );
    if let Some(nna) = &p.nna {
        c.notes.push(format!("national annex: {}", nna.country));
        if !replaced.is_empty() {
            c.notes.push(format!(
                "national annex values used: {}",
                replaced.join(", ")
            ));
        }
        c.notes.extend(nna.notes.iter().cloned());
        c.add_extra(&nna.extra_weather, &nna.extra_load_case);
    }
    Ok(c)
}

ohl_code_common::code_plugin!(
    NAME,
    "EN 50341-1 general approach + national annex: wind, ice, combined and \
     low-temperature cases with partial factors γ and ψ",
    cases
);

// No wind factors for structures from this plugin yet: OpenOHL keeps its
// generic drag factor and no shielding (the `structure-wind` stub).
ohl_code_common::no_structure_wind!();
