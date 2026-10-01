// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Calculations shipped with OLS CAD, exposed to the validation runner.

use crate::{Calculation, Inputs, Outputs, Registry};

pub(crate) fn register(registry: &mut Registry) {
    registry.register(Box::new(UnitsConvert));
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
