// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Calculations shipped with OLS CAD, exposed to the validation runner.

use nalgebra::Vector3;
use ols_core::units::{
    Density, Force, ForcePerLength, Length, MassPerLength, Quantity, Stress, Temperature,
};
use ols_sagtension::LoadedSpan;
use ols_sagtension::catenary::{SpanGeometry, default_solver_budget, tension_for_sag};
use ols_sagtension::criteria::default_criteria_budget;
use ols_sagtension::loaded::default_loaded_budget;
use ols_sagtension::{
    Catenary, ElasticCable, InstalledSpan, LoadState, TensionCriterion, TensionLimit,
    TensionSection, governing_tension, ruling_span,
};
use ols_weather::WeatherCase;
use ols_wirefe::catenary_element::default_element_budget;
use ols_wirefe::{CableElement, CableProperties, Model, Node, default_fe_budget};
use ols_wireloads::VerticalLoads;

use crate::{Calculation, Inputs, Outputs, Registry};

pub(crate) fn register(registry: &mut Registry) {
    registry.register(Box::new(UnitsConvert));
    registry.register(Box::new(CatenarySpan));
    registry.register(Box::new(CatenaryTensionForSag));
    registry.register(Box::new(ChangeOfState));
    registry.register(Box::new(GoverningTension));
    registry.register(Box::new(WeatherWireLoad));
    registry.register(Box::new(CrsTransform));
    registry.register(Box::new(Section));
    registry.register(Box::new(LoadedShape));
    registry.register(Box::new(LoadedChangeOfState));
    registry.register(Box::new(WireFeSpan));
    registry.register(Box::new(WireFePointLoad));
    registry.register(Box::new(Nonlinear));
    registry.register(Box::new(ThermalSteady));
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
            &[geometry],
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

/// `crs.transform`: a point from one coordinate system to another.
struct CrsTransform;

impl Calculation for CrsTransform {
    fn id(&self) -> &'static str {
        "crs.transform"
    }

    fn description(&self) -> &'static str {
        "Coordinate transformation (inputs: from, to — EPSG:<code> or proj4; x, y; geographic \
         in degrees, longitude first); outputs x, y"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let from = ols_crs::Crs::parse(string_input(inputs, "from")?)?;
        let to = ols_crs::Crs::parse(string_input(inputs, "to")?)?;
        let t = ols_crs::Transformation::new(&from, &to, ols_crs::Heights::Keep);
        let p = t.point([float_input(inputs, "x")?, float_input(inputs, "y")?, 0.0])?;
        Ok(outputs([("x", p[0]), ("y", p[1])]))
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
            ice_thickness: quantity_input::<Length>(inputs, "ice_thickness")?,
            ice_density: Some(quantity_input::<Density>(inputs, "ice_density")?),
            wind_pressure: quantity_input::<Stress>(inputs, "wind_pressure")?,
            ..WeatherCase::still(
                "validation",
                quantity_input::<Temperature>(inputs, "temperature")?,
            )
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

/// `sagtension.section`: tension section, exact equal horizontal tension.
struct Section;

impl Calculation for Section {
    fn id(&self) -> &'static str {
        "sagtension.section"
    }

    fn description(&self) -> &'static str {
        "Tension section (inputs: spans[{span, rise}], axial_stiffness, thermal_expansion, \
         known_tension, known_load, known_temperature, load, temperature)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let spans = inputs
            .get("spans")
            .and_then(|v| v.as_array())
            .ok_or("input \"spans\" must be an array of tables")?
            .iter()
            .map(|v| {
                let t = v.as_table().ok_or("each span must be a table")?;
                Ok(SpanGeometry::inclined(
                    quantity_input::<Length>(t, "span")?,
                    quantity_input::<Length>(t, "rise")?,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
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
        let rs = ruling_span(&spans).map_err(|e| e.to_string())?;
        let solution = TensionSection::from_known_state(
            spans,
            cable,
            known,
            quantity_input::<Force>(inputs, "known_tension")?,
        )
        .and_then(|section| section.solve(state, &default_solver_budget()))
        .map_err(|e| e.to_string())?;
        Ok(outputs([
            (
                "horizontal_tension_n",
                solution.horizontal_tension.newtons(),
            ),
            ("ruling_span_m", rs.meters()),
        ]))
    }
}

fn table_array<'a>(inputs: &'a Inputs, name: &str) -> Result<Vec<&'a Inputs>, String> {
    inputs
        .get(name)
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("input \"{name}\" must be an array of tables"))?
        .iter()
        .map(|v| {
            v.as_table()
                .ok_or_else(|| format!("each entry of \"{name}\" must be a table"))
        })
        .collect()
}

fn loaded_span_input(inputs: &Inputs) -> Result<LoadedSpan, String> {
    let point = table_array(inputs, "points")?
        .into_iter()
        .map(|t| {
            Ok((
                quantity_input::<Length>(t, "x")?,
                quantity_input::<Force>(t, "load")?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let distributed = table_array(inputs, "distributed")?
        .into_iter()
        .map(|t| {
            Ok((
                quantity_input::<Length>(t, "from")?,
                quantity_input::<Length>(t, "to")?,
                quantity_input::<ForcePerLength>(t, "load")?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(LoadedSpan {
        geometry: SpanGeometry::inclined(
            quantity_input::<Length>(inputs, "span")?,
            quantity_input::<Length>(inputs, "rise")?,
        ),
        base_load: quantity_input::<ForcePerLength>(inputs, "base_load")?,
        loads: VerticalLoads { point, distributed },
    })
}

/// `sagtension.loaded_span`: shape of a span with point/distributed loads.
struct LoadedShape;

impl Calculation for LoadedShape {
    fn id(&self) -> &'static str {
        "sagtension.loaded_span"
    }

    fn description(&self) -> &'static str {
        "Span with vertical point/distributed loads at a given H (inputs: span, rise, \
         horizontal_tension, base_load, points[{x, load}], distributed[{from, to, load}])"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let span = loaded_span_input(inputs)?;
        let solution = span
            .shape(
                quantity_input::<Force>(inputs, "horizontal_tension")?,
                &default_solver_budget(),
            )
            .map_err(|e| e.to_string())?;
        Ok(outputs([
            ("length_m", solution.length().meters()),
            ("sag_m", solution.max_sag().1.meters()),
            ("tension_left_n", solution.tension_left().newtons()),
            ("tension_right_n", solution.tension_right().newtons()),
        ]))
    }
}

/// `sagtension.loaded_change_of_state`: strung bare, solved with loads.
struct LoadedChangeOfState;

impl Calculation for LoadedChangeOfState {
    fn id(&self) -> &'static str {
        "sagtension.loaded_change_of_state"
    }

    fn description(&self) -> &'static str {
        "Cable strung bare (known_tension, known_temperature), solved with loads at temperature"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let loaded = loaded_span_input(inputs)?;
        let cable = ElasticCable {
            axial_stiffness: quantity_input::<Force>(inputs, "axial_stiffness")?,
            thermal_expansion: float_input(inputs, "thermal_expansion")?,
        };
        let inner = default_solver_budget();
        let bare = LoadedSpan::unloaded(loaded.geometry, loaded.base_load);
        let reference_temperature = quantity_input::<Temperature>(inputs, "known_temperature")?;
        let temperature = quantity_input::<Temperature>(inputs, "temperature")?;
        let solution = bare
            .unstrained_length(
                quantity_input::<Force>(inputs, "known_tension")?,
                cable,
                &inner,
            )
            .and_then(|l0| {
                loaded.solve_state(
                    cable,
                    l0,
                    reference_temperature,
                    temperature,
                    &default_loaded_budget(),
                    &inner,
                )
            })
            .map_err(|e| e.to_string())?;
        Ok(outputs([(
            "horizontal_tension_n",
            solution.horizontal_tension.newtons(),
        )]))
    }
}

fn vector_input<Q: Quantity>(inputs: &Inputs, prefix: &str) -> Result<Vector3<f64>, String> {
    Ok(Vector3::new(
        quantity_input::<Q>(inputs, &format!("{prefix}_x"))?.si(),
        quantity_input::<Q>(inputs, &format!("{prefix}_y"))?.si(),
        quantity_input::<Q>(inputs, &format!("{prefix}_z"))?.si(),
    ))
}

/// `wirefe.span`: one exact elastic catenary element between fixed ends.
struct WireFeSpan;

impl Calculation for WireFeSpan {
    fn id(&self) -> &'static str {
        "wirefe.span"
    }

    fn description(&self) -> &'static str {
        "Elastic catenary element (inputs: unstressed_length, chord_{x,y,z}, axial_stiffness, load_{x,y,z})"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let props = CableProperties {
            unstressed_length: quantity_input::<Length>(inputs, "unstressed_length")?.si(),
            axial_stiffness: quantity_input::<Force>(inputs, "axial_stiffness")?.si(),
            load: vector_input::<ForcePerLength>(inputs, "load")?,
        };
        let state = props
            .solve(
                vector_input::<Length>(inputs, "chord")?,
                None,
                &default_element_budget(),
            )
            .map_err(|e| e.to_string())?;
        Ok(outputs([
            ("horizontal_n", state.horizontal),
            ("tension_i_n", state.tension_i),
            ("tension_j_n", state.tension_j),
        ]))
    }
}

/// `wirefe.point_load`: a level span split by a free node carrying a load.
struct WireFePointLoad;

impl Calculation for WireFePointLoad {
    fn id(&self) -> &'static str {
        "wirefe.point_load"
    }

    fn description(&self) -> &'static str {
        "Level span with a point load at unstressed arc length load_position (two elements, Newton)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let span = quantity_input::<Length>(inputs, "span")?.si();
        let l0 = quantity_input::<Length>(inputs, "unstressed_length")?.si();
        let at = quantity_input::<Length>(inputs, "load_position")?.si();
        let p = quantity_input::<Force>(inputs, "point_load")?.si();
        let w = quantity_input::<ForcePerLength>(inputs, "weight")?.si();
        let ea = quantity_input::<Force>(inputs, "axial_stiffness")?.si();
        let props = |length: f64| CableProperties {
            unstressed_length: length,
            axial_stiffness: ea,
            load: Vector3::new(0.0, 0.0, -w),
        };
        let mut load_node = Node::free(Vector3::new(span * at / l0, 0.0, -5.0));
        load_node.load = Vector3::new(0.0, 0.0, -p);
        let model = Model {
            nodes: vec![
                Node::fixed(Vector3::zeros()),
                load_node,
                Node::fixed(Vector3::new(span, 0.0, 0.0)),
            ],
            cables: vec![
                CableElement {
                    i: 0,
                    j: 1,
                    properties: props(at),
                },
                CableElement {
                    i: 1,
                    j: 2,
                    properties: props(l0 - at),
                },
            ],
            trusses: vec![],
        };
        let s = model
            .solve(&default_fe_budget())
            .map_err(|e| e.to_string())?;
        Ok(outputs([
            ("horizontal_n", s.cables[0].horizontal),
            ("tension_i_n", s.cables[0].tension_i),
            ("tension_j_n", s.cables[1].tension_j),
            ("load_point_x_m", s.positions[1].x),
            ("load_point_z_m", s.positions[1].z),
        ]))
    }
}

/// `sagtension.nonlinear`: initial and final conditions with a non-linear cable.
struct Nonlinear;

impl Calculation for Nonlinear {
    fn id(&self) -> &'static str {
        "sagtension.nonlinear"
    }

    fn description(&self) -> &'static str {
        "Level span with non-linear stress-strain/creep: initial and final (inputs: cable table, span, \
         known_*, creep_*, max_*, load, temperature)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        use ols_cables::mechanics::{MaterialState, Mechanics};
        use ols_sagtension::nonlinear::{FinalCause, NonlinearSection, final_condition};

        let cable: ols_cables::Cable = inputs
            .get("cable")
            .ok_or("missing input \"cable\"")?
            .clone()
            .try_into()
            .map_err(|e| format!("input \"cable\": {e}"))?;
        cable.validate().map_err(|e| e.to_string())?;
        let mechanics = Mechanics::from_cable(&cable).map_err(|e| e.to_string())?;
        let state = |load: &str, temperature: &str| -> Result<LoadState, String> {
            Ok(LoadState {
                load: quantity_input::<ForcePerLength>(inputs, load)?,
                temperature: quantity_input::<Temperature>(inputs, temperature)?,
            })
        };
        let budget = default_solver_budget();
        let section = NonlinearSection::from_known_state(
            vec![SpanGeometry::level(quantity_input::<Length>(
                inputs, "span",
            )?)],
            mechanics,
            state("known_load", "known_temperature")?,
            quantity_input::<Force>(inputs, "known_tension")?,
            &MaterialState::Initial,
        )
        .map_err(|e| e.to_string())?;
        let (final_state, cause) = final_condition(
            &section,
            state("creep_load", "creep_temperature")?,
            state("max_load", "max_temperature")?,
            &budget,
        )
        .map_err(|e| e.to_string())?;
        let target = state("load", "temperature")?;
        let initial = section
            .solve(target, &MaterialState::Initial, &budget)
            .map_err(|e| e.to_string())?;
        let final_ = section
            .solve(target, &final_state, &budget)
            .map_err(|e| e.to_string())?;
        Ok(outputs([
            ("h_initial_n", initial.horizontal_tension.newtons()),
            ("h_final_n", final_.horizontal_tension.newtons()),
            ("sag_initial_m", initial.spans[0].sag().meters()),
            ("sag_final_m", final_.spans[0].sag().meters()),
            (
                "final_cause",
                if cause == FinalCause::Load { 1.0 } else { 0.0 },
            ),
        ]))
    }
}

/// `thermal.steady`: steady-state thermal rating of a conductor.
struct ThermalSteady;

impl Calculation for ThermalSteady {
    fn id(&self) -> &'static str {
        "thermal.steady"
    }

    fn description(&self) -> &'static str {
        "Steady-state thermal rating (inputs: method, diameter, ac_resistance [{temperature, \
         resistance}], emissivity, absorptivity, optional roughness, ambient = one .olsta case, \
         and max_temperature or current A; outputs: current_a, temperature_c, heat terms W/m, \
         solar_altitude_deg, solar_azimuth_deg)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        use ols_thermal::{Conductor, Method};
        let method = Method::parse(string_input(inputs, "method")?)?;
        let ac_resistance: Vec<ols_cables::ResistanceAt> = inputs
            .get("ac_resistance")
            .ok_or("missing input \"ac_resistance\"")?
            .clone()
            .try_into()
            .map_err(|e| format!("input \"ac_resistance\": {e}"))?;
        let conductor = Conductor {
            diameter: quantity_input::<Length>(inputs, "diameter")?.si(),
            electrical: ols_cables::Electrical {
                ac_resistance,
                emissivity: Some(float_input(inputs, "emissivity")?),
                solar_absorptivity: Some(float_input(inputs, "absorptivity")?),
                ..Default::default()
            },
            heat_capacity: Vec::new(),
            roughness: match inputs.get("roughness") {
                Some(_) => float_input(inputs, "roughness")?,
                None => ols_thermal::DEFAULT_ROUGHNESS,
            },
        };
        let ambient = inputs
            .get("ambient")
            .and_then(|v| v.as_table())
            .ok_or("input \"ambient\" must be a table (one .olsta case)")?;
        let mut file = toml::Table::new();
        file.insert(
            "case".into(),
            toml::Value::Array(vec![toml::Value::Table(ambient.clone())]),
        );
        let text = toml::to_string(&file).map_err(|e| e.to_string())?;
        let ambient = ols_thermal::AmbientCases::parse(&text)?.cases.remove(0);
        let balance = if inputs.contains_key("max_temperature") {
            let t = quantity_input::<Temperature>(inputs, "max_temperature")?.celsius();
            ols_thermal::ampacity(method, &conductor, &ambient, t)
        } else {
            ols_thermal::temperature(
                method,
                &conductor,
                &ambient,
                float_input(inputs, "current")?,
                &ols_thermal::default_steady_budget(),
            )
        }
        .map_err(|e| e.to_string())?;
        let mut out = outputs([
            ("current_a", balance.current),
            ("temperature_c", balance.temperature),
            ("joule_w_per_m", balance.joule),
            ("solar_w_per_m", balance.solar.heat),
            ("convection_w_per_m", balance.convection),
            ("radiation_w_per_m", balance.radiation),
            ("irradiance_w_per_m2", balance.solar.irradiance),
        ]);
        if let Some(sun) = balance.solar.sun {
            out.insert("solar_altitude_deg".into(), sun.altitude);
            out.insert("solar_azimuth_deg".into(), sun.azimuth);
        }
        Ok(out)
    }
}
