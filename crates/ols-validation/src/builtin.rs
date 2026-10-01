// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Calculations shipped with OLS CAD, exposed to the validation runner.

use ols_core::units::{
    Density, Force, ForcePerLength, Length, MassPerLength, Quantity, Stress, Temperature,
};
use ols_sagtension::catenary::{SpanGeometry, default_solver_budget, tension_for_sag};
use ols_sagtension::criteria::default_criteria_budget;
use ols_sagtension::{
    Catenary, ElasticCable, InstalledSpan, LoadState, TensionCriterion, TensionLimit,
    governing_tension,
};
use ols_weather::WeatherCase;

use crate::{Calculation, Inputs, Outputs, Registry};

pub(crate) fn register(registry: &mut Registry) {
    registry.register(Box::new(UnitsConvert));
    registry.register(Box::new(CatenarySpan));
    registry.register(Box::new(CatenaryTensionForSag));
    registry.register(Box::new(ChangeOfState));
    registry.register(Box::new(GoverningTension));
    registry.register(Box::new(WeatherWireLoad));
}

fn float_input(inputs: &Inputs, name: &str) -> Result<f64, String> {
    let value = inputs
        .get(name)
        .ok_or_else(|| format!("missing input \"{name}\""))?;
    value
        .as_float()
        .or_else(|| value.as_integer().map(|i| i as f64))
        .ok_or_else(|| format!("input \"{name}\" must be a number"))
}

/// Reads an input given as text with a unit, e.g. `span = "300 m"`.
fn quantity_input<Q: Quantity>(inputs: &Inputs, name: &str) -> Result<Q, String> {
    string_input(inputs, name)?
        .parse()
        .map_err(|e| format!("input \"{name}\": {e}"))
}

fn outputs<const N: usize>(values: [(&str, f64); N]) -> Outputs {
    values.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

fn string_input<'a>(inputs: &'a Inputs, name: &str) -> Result<&'a str, String> {
    inputs
        .get(name)
        .ok_or_else(|| format!("missing input \"{name}\""))?
        .as_str()
        .ok_or_else(|| format!("input \"{name}\" must be a string"))
}

/// `units.convert`: converts `value` (text with unit) of kind `quantity` to unit `to`.
struct UnitsConvert;

impl Calculation for UnitsConvert {
    fn id(&self) -> &'static str {
        "units.convert"
    }

    fn description(&self) -> &'static str {
        "Unit conversion of a quantity (inputs: quantity, value, to; output: value)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let converted = ols_core::units::convert(
            string_input(inputs, "quantity")?,
            string_input(inputs, "value")?,
            string_input(inputs, "to")?,
        )
        .map_err(|e| e.to_string())?;
        Ok(Outputs::from([("value".to_owned(), converted)]))
    }
}

/// `catenary.span`: exact catenary of one span.
struct CatenarySpan;

impl Calculation for CatenarySpan {
    fn id(&self) -> &'static str {
        "catenary.span"
    }

    fn description(&self) -> &'static str {
        "Catenary of one span (inputs: span, rise, horizontal_tension, weight)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let geometry = SpanGeometry::inclined(
            quantity_input::<Length>(inputs, "span")?,
            quantity_input::<Length>(inputs, "rise")?,
        );
        let solution = Catenary::new(
            quantity_input::<Force>(inputs, "horizontal_tension")?,
            quantity_input::<ForcePerLength>(inputs, "weight")?,
        )
        .and_then(|c| c.solve(geometry))
        .map_err(|e| e.to_string())?;
        Ok(outputs([
            ("sag_m", solution.sag().meters()),
            ("midspan_sag_m", solution.midspan_sag().meters()),
            ("length_m", solution.length().meters()),
            ("tension_left_n", solution.tension_left().newtons()),
            ("tension_right_n", solution.tension_right().newtons()),
            ("low_point_x_m", solution.low_point_x().meters()),
        ]))
    }
}

/// `catenary.tension_for_sag`: horizontal tension giving a maximum sag.
struct CatenaryTensionForSag;

impl Calculation for CatenaryTensionForSag {
    fn id(&self) -> &'static str {
        "catenary.tension_for_sag"
    }

    fn description(&self) -> &'static str {
        "Horizontal tension for a given sag (inputs: span, rise, weight, sag)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let geometry = SpanGeometry::inclined(
            quantity_input::<Length>(inputs, "span")?,
            quantity_input::<Length>(inputs, "rise")?,
        );
        let catenary = tension_for_sag(
            geometry,
            quantity_input::<ForcePerLength>(inputs, "weight")?,
            quantity_input::<Length>(inputs, "sag")?,
            &default_solver_budget(),
        )
        .map_err(|e| e.to_string())?;
        Ok(outputs([(
            "horizontal_tension_n",
            catenary.horizontal_tension.newtons(),
        )]))
    }
}

/// `sagtension.change_of_state`: single span, linear elastic + thermal.
struct ChangeOfState;

impl Calculation for ChangeOfState {
    fn id(&self) -> &'static str {
        "sagtension.change_of_state"
    }

    fn description(&self) -> &'static str {
        "Single-span change of state (inputs: span, rise, axial_stiffness, thermal_expansion, \
         known_tension, known_load, known_temperature, load, temperature)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let geometry = SpanGeometry::inclined(
            quantity_input::<Length>(inputs, "span")?,
            quantity_input::<Length>(inputs, "rise")?,
        );
        let cable = ElasticCable {
            axial_stiffness: quantity_input::<Force>(inputs, "axial_stiffness")?,
            thermal_expansion: float_input(inputs, "thermal_expansion")?,
        };
        let known = LoadState {
            load: quantity_input::<ForcePerLength>(inputs, "known_load")?,
            temperature: quantity_input::<Temperature>(inputs, "known_temperature")?,
        };
        let state = LoadState {
            load: quantity_input::<ForcePerLength>(inputs, "load")?,
            temperature: quantity_input::<Temperature>(inputs, "temperature")?,
        };
        let solution = InstalledSpan::from_known_state(
            geometry,
            cable,
            known,
            quantity_input::<Force>(inputs, "known_tension")?,
        )
        .and_then(|span| span.solve(state, &default_solver_budget()))
        .map_err(|e| e.to_string())?;
        Ok(outputs([
            (
                "horizontal_tension_n",
                solution.catenary.horizontal_tension.newtons(),
            ),
            ("sag_m", solution.sag().meters()),
            ("length_m", solution.length().meters()),
            ("tension_left_n", solution.tension_left().newtons()),
            ("tension_right_n", solution.tension_right().newtons()),
        ]))
    }
}

/// `sagtension.governing_tension`: stringing tension from tension criteria.
struct GoverningTension;

fn criterion_input(table: &Inputs) -> Result<TensionCriterion, String> {
    let label = string_input(table, "label")?.to_owned();
    let state = LoadState {
        load: quantity_input::<ForcePerLength>(table, "load")?,
        temperature: quantity_input::<Temperature>(table, "temperature")?,
    };
    let limit = match string_input(table, "limit")? {
        "support_ratio" => TensionLimit::SupportTensionRatio(float_input(table, "value")?),
        "horizontal_ratio" => TensionLimit::HorizontalTensionRatio(float_input(table, "value")?),
        "horizontal" => TensionLimit::HorizontalTension(quantity_input::<Force>(table, "value")?),
        "catenary_constant" => {
            TensionLimit::CatenaryConstant(quantity_input::<Length>(table, "value")?)
        }
        other => return Err(format!("unknown limit \"{other}\"")),
    };
    Ok(TensionCriterion {
        label,
        state,
        limit,
    })
}

impl Calculation for GoverningTension {
    fn id(&self) -> &'static str {
        "sagtension.governing_tension"
    }

    fn description(&self) -> &'static str {
        "Governing stringing tension (inputs: span, rise, axial_stiffness, thermal_expansion, \
         rated_tensile_strength, reference_load, reference_temperature, criteria[])"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let geometry = SpanGeometry::inclined(
            quantity_input::<Length>(inputs, "span")?,
            quantity_input::<Length>(inputs, "rise")?,
        );
        let cable = ElasticCable {
            axial_stiffness: quantity_input::<Force>(inputs, "axial_stiffness")?,
            thermal_expansion: float_input(inputs, "thermal_expansion")?,
        };
        let reference = LoadState {
            load: quantity_input::<ForcePerLength>(inputs, "reference_load")?,
            temperature: quantity_input::<Temperature>(inputs, "reference_temperature")?,
        };
        let criteria = inputs
            .get("criteria")
            .and_then(|v| v.as_array())
            .ok_or("input \"criteria\" must be an array of tables")?
            .iter()
            .map(|v| {
                v.as_table()
                    .ok_or("each criterion must be a table".to_owned())
                    .and_then(criterion_input)
            })
            .collect::<Result<Vec<_>, String>>()?;
        let result = governing_tension(
            geometry,
            cable,
            quantity_input::<Force>(inputs, "rated_tensile_strength")?,
            reference,
            &criteria,
            &default_criteria_budget(),
        )
        .map_err(|e| e.to_string())?;
        let mut out = outputs([
            ("reference_tension_n", result.reference_tension.newtons()),
            ("governing_index", result.governing as f64),
        ]);
        for (i, c) in result.criteria.iter().enumerate() {
            out.insert(format!("utilisation_{i}"), c.utilisation);
        }
        Ok(out)
    }
}

/// `weather.wire_load`: ice and wind loads on a wire.
struct WeatherWireLoad;

impl Calculation for WeatherWireLoad {
    fn id(&self) -> &'static str {
        "weather.wire_load"
    }

    fn description(&self) -> &'static str {
        "Ice and wind load on a wire (inputs: diameter, weight, temperature, ice_thickness, \
         ice_density, wind_pressure)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let case = WeatherCase {
            name: "validation".into(),
            temperature: quantity_input::<Temperature>(inputs, "temperature")?,
            ice_thickness: quantity_input::<Length>(inputs, "ice_thickness")?,
            ice_density: quantity_input::<Density>(inputs, "ice_density")?,
            wind_pressure: quantity_input::<Stress>(inputs, "wind_pressure")?,
        };
        let weight = quantity_input::<MassPerLength>(inputs, "weight")?.weight();
        let load = case.wire_load(quantity_input::<Length>(inputs, "diameter")?, weight);
        Ok(outputs([
            ("vertical_n_per_m", load.vertical.newtons_per_meter()),
            ("horizontal_n_per_m", load.horizontal.newtons_per_meter()),
            ("resultant_n_per_m", load.resultant().newtons_per_meter()),
            ("swing_angle_deg", load.swing_angle().degrees()),
        ]))
    }
}
