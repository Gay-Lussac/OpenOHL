// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! OpenOHL design-code plugin (`code-plugin` world): California Public
//! Utilities Commission General Order 95, overhead line construction.
//!
//! Method: Rule 43 loading districts by elevation (ice, wind pressure on
//! the projected area, temperature); loads are not factored, the Rule 44
//! safety factors divide the strengths, once at installation and once at
//! replacement (strength factor 1 / factor). US customary input,
//! converted to SI. Every number comes from the parameter file (see
//! `example-parameters.toml` and the README).
//!
//! Build with `cargo build --release --target wasm32-wasip2`.

wit_bindgen::generate!({
    world: "code-plugin",
    path: "../../../crates/ols-plugins/wit",
});

use ols_code_common::us::{self, KG_M3_PER_LB_FT3, MM_PER_INCH, PA_PER_PSF};
use ols_code_common::{Cases, Condition, ExtraLoad, ExtraWeather, Load, Weather, check};
use serde::Deserialize;

const NAME: &str = "go95";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    project: Project,
    district: Vec<District>,
    safety_factors: Safety,
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
    site_elevation: Option<f64>,
    #[serde(default)]
    district: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct District {
    name: String,
    from_elevation: f64,
    ice_thickness: f64,
    ice_density: f64,
    wind_pressure: f64,
    temperature: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Safety {
    installation: f64,
    replacement: f64,
}

fn cases(text: &str) -> Result<Cases, String> {
    let p: Params = ols_code_common::parse(text)?;
    let district = match (&p.project.district, p.project.site_elevation) {
        (Some(name), _) => p
            .district
            .iter()
            .find(|d| &d.name == name)
            .ok_or_else(|| format!("no [[district]] named \"{name}\""))?,
        (None, Some(elevation)) => p
            .district
            .iter()
            .filter(|d| d.from_elevation <= elevation)
            .max_by(|a, b| a.from_elevation.total_cmp(&b.from_elevation))
            .ok_or_else(|| format!("no [[district]] starts at or below {elevation} ft"))?,
        (None, None) => return Err("project: give site_elevation or district".into()),
    };
    check("district.ice_thickness", district.ice_thickness, 0.0, 10.0)?;
    check("district.ice_density", district.ice_density, 1.0, 62.4)?;
    check("district.wind_pressure", district.wind_pressure, 0.0, 200.0)?;
    let s = &p.safety_factors;
    check("safety_factors.installation", s.installation, 1.0, 20.0)?;
    check("safety_factors.replacement", s.replacement, 1.0, 20.0)?;

    let tags = ["loads", "go95"];
    let mut c = Cases::new(
        &p.project.name,
        NAME,
        env!("CARGO_PKG_VERSION"),
        "GO 95 Rule 43 loading and Rule 44 safety factors",
    );
    let mut w = Weather::new(
        format!("GO 95 {} loading", district.name),
        us::celsius(district.temperature),
    )
    .describe("Rule 43 loading district")
    .wind(district.wind_pressure * PA_PER_PSF)
    .tags(&tags);
    if district.ice_thickness > 0.0 {
        w = w.ice_thickness(
            district.ice_thickness * MM_PER_INCH,
            district.ice_density * KG_M3_PER_LB_FT3,
        );
    }
    let load = |name: &str, factor: f64, text: &str| {
        Load::new(name, &w)
            .describe(text)
            .strength(1.0 / factor)
            .condition(p.project.condition)
            .tags(&tags)
    };
    c.loads = vec![
        load(
            "GO 95 at installation",
            s.installation,
            "Rule 44 safety factor at installation",
        ),
        load(
            "GO 95 at replacement",
            s.replacement,
            "Rule 44 safety factor at replacement",
        ),
    ];
    c.weather = vec![w];
    c.notes.push(format!(
        "loading district {} (from {} ft): {} in ice, {} psf wind at {} °F",
        district.name,
        district.from_elevation,
        district.ice_thickness,
        district.wind_pressure,
        district.temperature
    ));
    c.notes.push(format!(
        "loads unfactored; strength factors 1/{} at installation, 1/{} at replacement",
        s.installation, s.replacement
    ));
    c.notes.push(
        "not generated: longitudinal (dead-end, broken conductor) and vertical clearance \
         cases — use the broken-span solve and project cases"
            .into(),
    );
    c.add_extra(&p.extra_weather, &p.extra_load_case);
    Ok(c)
}

ols_code_common::code_plugin!(
    NAME,
    "CPUC GO 95: Rule 43 loading district by elevation, Rule 44 safety factors",
    cases
);
