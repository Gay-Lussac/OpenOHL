// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Calculations shipped with OLS CAD, exposed to the validation runner.

use ols_core::units::{Force, ForcePerLength, Length, Quantity};
use ols_sagtension::Catenary;
use ols_sagtension::catenary::{SpanGeometry, default_solver_budget, tension_for_sag};

use crate::{Calculation, Inputs, Outputs, Registry};

pub(crate) fn register(registry: &mut Registry) {
    registry.register(Box::new(UnitsConvert));
    registry.register(Box::new(CatenarySpan));
    registry.register(Box::new(CatenaryTensionForSag));
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
