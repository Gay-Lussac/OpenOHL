// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The wind at an angle on a wire (`benchmarks/weather/wind_direction.toml`,
//! theory: Wind on the line): the force per metre of a horizontal wind at
//! an angle to a straight, possibly inclined, piece of wire, by the
//! program's own function (`ohl_weather::oblique_wire_force`). An adapter
//! only.

use crate::{Calculation, Inputs, Outputs, Registry};

pub(crate) fn register(registry: &mut Registry) {
    registry.register(Box::new(ObliqueWireLoad));
}

fn number(inputs: &Inputs, name: &str) -> Result<Option<f64>, String> {
    inputs
        .get(name)
        .map(|v| {
            v.as_float()
                .or_else(|| v.as_integer().map(|i| i as f64))
                .ok_or_else(|| format!("input \"{name}\" must be a number"))
        })
        .transpose()
}

fn required(inputs: &Inputs, name: &str) -> Result<f64, String> {
    number(inputs, name)?.ok_or_else(|| format!("missing input \"{name}\""))
}

/// `weather.oblique_wire_load`: a wire of diameter `diameter_mm` rising at
/// `incline_deg` (0: level) along +x in plan, a horizontal wind of
/// `wind_pressure_pa` blowing at `wind_to_wire_deg` to the wire's
/// direction in plan, by `law` (`projected`, default, or `cross_flow`),
/// with `friction` (none: no force along the wire).
struct ObliqueWireLoad;

impl Calculation for ObliqueWireLoad {
    fn id(&self) -> &'static str {
        "weather.oblique_wire_load"
    }

    fn description(&self) -> &'static str {
        "A wind at an angle on a wire (inputs: wind_pressure_pa, diameter_mm, wind_to_wire_deg, \
         incline_deg (optional, 0), law (projected or cross_flow, optional), friction \
         (optional), span_m (optional)); outputs normal_n_per_m, across_n_per_m, \
         vertical_n_per_m, along_n_per_m, span_normal_n"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let qd = required(inputs, "wind_pressure_pa")? * required(inputs, "diameter_mm")? / 1000.0;
        let theta = required(inputs, "wind_to_wire_deg")?.to_radians();
        let incline = number(inputs, "incline_deg")?.unwrap_or(0.0).to_radians();
        let law = match inputs.get("law").and_then(|v| v.as_str()) {
            None => ohl_weather::ObliqueLaw::Projected,
            Some(t) => ohl_weather::ObliqueLaw::parse(t)
                .ok_or_else(|| format!("law \"{t}\": projected or cross_flow"))?,
        };
        let friction = number(inputs, "friction")?;
        let tangent = [incline.cos(), 0.0, incline.sin()];
        let towards = [theta.cos(), theta.sin(), 0.0];
        let f = ohl_weather::oblique_wire_force(qd, towards, tangent, law, friction);
        let along: f64 = (0..3).map(|k| f[k] * tangent[k]).sum();
        let normal = [0, 1, 2].map(|k| f[k] - along * tangent[k]);
        let n = normal.iter().map(|x| x * x).sum::<f64>().sqrt();
        let mut out = Outputs::new();
        out.insert("normal_n_per_m".into(), n);
        out.insert("across_n_per_m".into(), f[1]);
        out.insert("vertical_n_per_m".into(), f[2]);
        out.insert("along_n_per_m".into(), along);
        if let Some(l) = number(inputs, "span_m")? {
            out.insert("span_normal_n".into(), n * l);
        }
        Ok(out)
    }
}
