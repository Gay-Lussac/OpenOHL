// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! OpenOHL design-code plugin (`code-rules-plugin` world): NESC (ANSI C2),
//! Section 25 loadings for grades B and C.
//!
//! Method: Rule 250B combined ice and wind of the line's loading district
//! (radial ice, wind pressure, temperature, constant k), Rule 250C
//! extreme wind and Rule 250D extreme ice with concurrent wind, with the
//! wind pressure constant·k_z·G·C_f·V²; each with the load factors of the
//! construction grade and a strength factor. US customary input,
//! converted to SI. Every number comes from the parameter file (see
//! `example-parameters.toml` and the README).
//!
//! Build with `cargo build --release --target wasm32-wasip2`.

wit_bindgen::generate!({
    world: "code-rules-plugin",
    path: "../../../crates/ohl-plugins/wit",
});

use ohl_code_common::us::{
    self, KG_M3_PER_LB_FT3, MM_PER_INCH, N_PER_M_PER_LBF_PER_FT, PA_PER_PSF,
};
use ohl_code_common::{Cases, Condition, ExtraLoad, ExtraWeather, Load, Weather, check};
use serde::Deserialize;

const NAME: &str = "nesc";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    project: Project,
    district: Vec<District>,
    ice: Ice,
    extreme_wind: ExtremeWind,
    extreme_ice: ExtremeIce,
    load_factors: LoadFactors,
    strength_factors: StrengthFactors,
    #[serde(default)]
    extra_weather: Vec<ExtraWeather>,
    #[serde(default)]
    extra_load_case: Vec<ExtraLoad>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct District {
    name: String,
    ice_thickness: f64,
    wind_pressure: f64,
    temperature: f64,
    constant_k: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Project {
    name: String,
    #[serde(default)]
    condition: Condition,
    #[serde(default)]
    grade: Option<String>,
    district: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ice {
    density: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtremeWind {
    enabled: bool,
    speed: f64,
    velocity_pressure_constant: f64,
    exposure_factor: f64,
    gust_response_factor: f64,
    force_coefficient: f64,
    temperature: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtremeIce {
    enabled: bool,
    ice_thickness: f64,
    wind_speed: f64,
    temperature: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Factors {
    vertical: f64,
    transverse: f64,
    longitudinal: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LoadFactors {
    district: Factors,
    extreme_wind: Factors,
    extreme_ice: Factors,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrengthFactors {
    district: f64,
    extreme: f64,
}

fn cases(text: &str) -> Result<Cases, String> {
    let p: Params = ohl_code_common::parse(text)?;
    let district = p
        .district
        .iter()
        .find(|d| d.name == p.project.district)
        .ok_or_else(|| format!("no [[district]] named \"{}\"", p.project.district))?;
    check("district.ice_thickness", district.ice_thickness, 0.0, 10.0)?;
    check("district.wind_pressure", district.wind_pressure, 0.0, 200.0)?;
    check("district.constant_k", district.constant_k, 0.0, 10.0)?;
    let density = check("ice.density", p.ice.density, 1.0, 62.4)? * KG_M3_PER_LB_FT3;
    let ew = &p.extreme_wind;
    check("extreme_wind.speed", ew.speed, 0.0, 250.0)?;
    check(
        "extreme_ice.wind_speed",
        p.extreme_ice.wind_speed,
        0.0,
        250.0,
    )?;
    check(
        "extreme_ice.ice_thickness",
        p.extreme_ice.ice_thickness,
        0.0,
        10.0,
    )?;
    let lf = &p.load_factors;
    for (what, f) in [
        ("district", &lf.district),
        ("extreme_wind", &lf.extreme_wind),
        ("extreme_ice", &lf.extreme_ice),
    ] {
        for (axis, x) in [
            ("vertical", f.vertical),
            ("transverse", f.transverse),
            ("longitudinal", f.longitudinal),
        ] {
            check(&format!("load_factors.{what}.{axis}"), x, 0.1, 10.0)?;
        }
    }
    let sf = &p.strength_factors;
    check("strength_factors.district", sf.district, 0.05, 1.0)?;
    check("strength_factors.extreme", sf.extreme, 0.05, 1.0)?;

    // Wind pressure, Pa, at `mph` with the extreme-wind factors.
    let pressure = |mph: f64| {
        ew.velocity_pressure_constant
            * ew.exposure_factor
            * ew.gust_response_factor
            * ew.force_coefficient
            * mph
            * mph
            * PA_PER_PSF
    };
    let tags = ["loads", "nesc"];
    let mut c = Cases::new(
        &p.project.name,
        NAME,
        env!("CARGO_PKG_VERSION"),
        "NESC Rule 250 loadings",
    );
    let mut weather = vec![
        Weather::new(
            format!("NESC district ({})", district.name),
            us::celsius(district.temperature),
        )
        .describe("Rule 250B combined ice and wind of the loading district")
        .ice_thickness(district.ice_thickness * MM_PER_INCH, density)
        .wind(district.wind_pressure * PA_PER_PSF)
        .tags(&tags),
    ];
    let condition = p.project.condition;
    let load = |name: &str, w: &Weather, f: &Factors, strength: f64, text: &str| {
        Load::new(name, w)
            .describe(text)
            .factors(f.vertical, f.transverse, f.longitudinal)
            .strength(strength)
            .condition(condition)
            .tags(&tags)
    };
    let mut loads = vec![load(
        "NESC 250B district loading",
        &weather[0],
        &lf.district,
        sf.district,
        "Rule 250B, grade load factors",
    )];
    if ew.enabled {
        let w = Weather::new("NESC extreme wind", us::celsius(ew.temperature))
            .describe("Rule 250C extreme wind, no ice")
            .wind(pressure(ew.speed))
            .tags(&tags);
        loads.push(load(
            "NESC 250C extreme wind",
            &w,
            &lf.extreme_wind,
            sf.extreme,
            "Rule 250C",
        ));
        weather.push(w);
    }
    let ei = &p.extreme_ice;
    if ei.enabled {
        let w = Weather::new("NESC extreme ice with wind", us::celsius(ei.temperature))
            .describe("Rule 250D extreme ice with concurrent wind")
            .ice_thickness(ei.ice_thickness * MM_PER_INCH, density)
            .wind(pressure(ei.wind_speed))
            .tags(&tags);
        loads.push(load(
            "NESC 250D extreme ice with wind",
            &w,
            &lf.extreme_ice,
            sf.extreme,
            "Rule 250D",
        ));
        weather.push(w);
    }
    c.weather = weather;
    c.loads = loads;
    if let Some(g) = &p.project.grade {
        c.notes.push(format!(
            "construction grade {g}: the file's factors are taken as its"
        ));
    }
    c.notes.push(format!(
        "district {}: the constant k = {} lb/ft ({:.3} N/m) added to the resultant is NOT in \
         the weather case (no such field): add it to the structure loads by hand",
        district.name,
        district.constant_k,
        district.constant_k * N_PER_M_PER_LBF_PER_FT
    ));
    c.notes.push(
        "Rule 250C/D apply above the height the NESC gives; the file decides whether they \
         are generated (enabled)"
            .into(),
    );
    c.notes.push(
        "not generated: longitudinal (broken wire, dead-end) and construction loads — use \
         the broken-span solve and project load cases"
            .into(),
    );
    c.add_extra(&p.extra_weather, &p.extra_load_case);
    Ok(c)
}

ohl_code_common::code_plugin!(
    NAME,
    "NESC (ANSI C2) Rule 250: district, extreme wind and extreme ice cases with load factors",
    cases
);

// No wind factors for structures from this plugin yet: OpenOHL keeps its
// generic drag factor and no shielding (the `structure-wind` stub).
ohl_code_common::no_structure_wind!();
ohl_code_common::no_rule_presets!();
