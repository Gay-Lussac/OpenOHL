// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Calculations shipped with OpenOHL, exposed to the validation runner.

use nalgebra::Vector3;
use ohl_core::units::{
    Area, Density, Force, ForcePerLength, Length, MassPerLength, Quantity, Stress, Temperature,
};
use ohl_sagtension::LoadedSpan;
use ohl_sagtension::catenary::{SpanGeometry, default_solver_budget, tension_for_sag};
use ohl_sagtension::criteria::default_criteria_budget;
use ohl_sagtension::loaded::default_loaded_budget;
use ohl_sagtension::{
    Catenary, ElasticCable, InstalledSpan, LoadState, TensionCriterion, TensionLimit,
    TensionSection, governing_tension, ruling_span,
};
use ohl_weather::WeatherCase;
use ohl_wirefe::catenary_element::default_element_budget;
use ohl_wirefe::{CableElement, CableProperties, Model, Node, default_fe_budget};
use ohl_wireloads::VerticalLoads;

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
    registry.register(Box::new(SpottingLevel));
    registry.register(Box::new(StructureFe));
    registry.register(Box::new(StructurePole));
    registry.register(Box::new(StructureMemberWind));
    registry.register(Box::new(StructureWindRule));
    registry.register(Box::new(GallopingEllipses));
    registry.register(Box::new(CatenaryFit));
    registry.register(Box::new(ClearanceSurface));
    registry.register(Box::new(FallIn));
    registry.register(Box::new(SwingArcCase));
    registry.register(Box::new(SwingArcClearance));
    registry.register(Box::new(SafetyVolumeCase));
    registry.register(Box::new(SpanCasesSection));
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
        let converted = ohl_core::units::convert(
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
        let from = ohl_crs::Crs::parse(string_input(inputs, "from")?)?;
        let to = ohl_crs::Crs::parse(string_input(inputs, "to")?)?;
        let t = ohl_crs::Transformation::new(&from, &to, ohl_crs::Heights::Keep);
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
        "Ice and wind load on a wire (inputs: diameter, weight, temperature, ice_thickness \
         or ice_load, ice_density (optional: glaze ice), wind_pressure); outputs the loads \
         and iced_diameter_mm"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let optional = |k: &str| inputs.contains_key(k);
        let case = WeatherCase {
            ice_thickness: if optional("ice_thickness") {
                quantity_input::<Length>(inputs, "ice_thickness")?
            } else {
                Length::ZERO
            },
            ice_load: optional("ice_load")
                .then(|| quantity_input::<ForcePerLength>(inputs, "ice_load"))
                .transpose()?,
            ice_density: optional("ice_density")
                .then(|| quantity_input::<Density>(inputs, "ice_density"))
                .transpose()?,
            wind_pressure: quantity_input::<Stress>(inputs, "wind_pressure")?,
            ..WeatherCase::still(
                "validation",
                quantity_input::<Temperature>(inputs, "temperature")?,
            )
        };
        case.validate()?;
        let weight = quantity_input::<MassPerLength>(inputs, "weight")?.weight();
        let diameter = quantity_input::<Length>(inputs, "diameter")?;
        let load = case.wire_load(diameter, weight);
        Ok(outputs([
            ("vertical_n_per_m", load.vertical.newtons_per_meter()),
            ("horizontal_n_per_m", load.horizontal.newtons_per_meter()),
            ("resultant_n_per_m", load.resultant().newtons_per_meter()),
            ("swing_angle_deg", load.swing_angle().degrees()),
            (
                "iced_diameter_mm",
                case.iced_diameter(diameter).millimeters(),
            ),
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
        use ohl_cables::mechanics::{MaterialState, Mechanics};
        use ohl_sagtension::nonlinear::{FinalCause, NonlinearSection, final_condition};

        let cable: ohl_cables::Cable = inputs
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
         resistance}], optional kind (cable kind, default acsr: scales a single resistance), \
         emissivity, absorptivity, optional roughness, ambient = one .ohlta case, \
         and max_temperature or current A; outputs: current_a, temperature_c, heat terms W/m, \
         solar_altitude_deg, solar_azimuth_deg)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        use ohl_thermal::{Conductor, Method};
        let method = Method::parse(string_input(inputs, "method")?)?;
        let ac_resistance: Vec<ohl_cables::ResistanceAt> = inputs
            .get("ac_resistance")
            .ok_or("missing input \"ac_resistance\"")?
            .clone()
            .try_into()
            .map_err(|e| format!("input \"ac_resistance\": {e}"))?;
        let kind: ohl_cables::CableKind = match inputs.get("kind") {
            Some(v) => v
                .clone()
                .try_into()
                .map_err(|e| format!("input \"kind\": {e}"))?,
            None => ohl_cables::CableKind::Acsr,
        };
        let conductor = Conductor {
            diameter: quantity_input::<Length>(inputs, "diameter")?.si(),
            kind,
            electrical: ohl_cables::Electrical {
                ac_resistance,
                emissivity: Some(float_input(inputs, "emissivity")?),
                solar_absorptivity: Some(float_input(inputs, "absorptivity")?),
                ..Default::default()
            },
            heat_capacity: Vec::new(),
            roughness: match inputs.get("roughness") {
                Some(_) => float_input(inputs, "roughness")?,
                None => ohl_thermal::DEFAULT_ROUGHNESS,
            },
            defaults: [
                ohl_thermal::DEFAULT_EMISSIVITY,
                ohl_thermal::DEFAULT_ABSORPTIVITY,
                ohl_thermal::ambient::DEFAULT_ALBEDO,
            ],
        };
        let ambient = inputs
            .get("ambient")
            .and_then(|v| v.as_table())
            .ok_or("input \"ambient\" must be a table (one .ohlta case)")?;
        let mut file = toml::Table::new();
        file.insert(
            "case".into(),
            toml::Value::Array(vec![toml::Value::Table(ambient.clone())]),
        );
        let text = toml::to_string(&file).map_err(|e| e.to_string())?;
        let ambient = ohl_thermal::AmbientCases::parse(&text)?.cases.remove(0);
        let balance = if inputs.contains_key("max_temperature") {
            let t = quantity_input::<Temperature>(inputs, "max_temperature")?.celsius();
            ohl_thermal::ampacity(method, &conductor, &ambient, t)
        } else {
            ohl_thermal::temperature(
                method,
                &conductor,
                &ambient,
                float_input(inputs, "current")?,
                &ohl_thermal::default_steady_budget(),
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

/// `structures.fe`: a structure model (`.ohlfe` text) under its own loads.
struct StructureFe;

impl Calculation for StructureFe {
    fn id(&self) -> &'static str {
        "structures.fe"
    }

    fn description(&self) -> &'static str {
        "Structure model under its own loads (inputs: model = .ohlfe text, optional \
         self_weight factor); outputs u_<node>_<dof> (m, rad), n_<member> (N), \
         r_<node>_<dof> (N, N m), usage_<member>, slack_<member> (cables and tension-only \
         members: 1 slack, 0 taut), bowed_<member> (tension-only members: 1 at their own \
         buckling load), iterations, passes (solves of the tension-only members' active-set \
         iterations; 0: none)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let model = ohl_structures_fe::Model::parse(string_input(inputs, "model")?)?;
        let weight = match inputs.get("self_weight") {
            Some(_) => float_input(inputs, "self_weight")?,
            None => 0.0,
        };
        let loads = ohl_structures_fe::Loads {
            nodal: model.loads.clone(),
            self_weight: weight,
        };
        let (s, usage) =
            ohl_structures_fe::analyse(&model, &loads, 1.0, &ohl_structures_fe::default_budget())
                .map_err(|e| e.to_string())?;
        let dofs = ohl_structures_fe::solve::DOF_NAMES;
        let mut out = Outputs::new();
        for (n, d) in model.nodes.iter().zip(&s.displacements) {
            for (k, v) in d.iter().enumerate() {
                out.insert(format!("u_{}_{}", n.id, dofs[k]), *v);
            }
        }
        for (m, f) in model.members.iter().zip(&s.members) {
            out.insert(format!("n_{}", m.id), f.axial);
            if matches!(
                m.kind,
                ohl_structures_fe::MemberKind::Cable | ohl_structures_fe::MemberKind::Tension
            ) {
                out.insert(format!("slack_{}", m.id), f64::from(u8::from(f.slack)));
            }
            if m.kind == ohl_structures_fe::MemberKind::Tension {
                out.insert(format!("bowed_{}", m.id), f64::from(u8::from(f.bowed)));
            }
        }
        for r in &s.reactions {
            let id = &model.nodes[r.node].id;
            for k in 0..3 {
                out.insert(format!("r_{id}_{}", dofs[k]), r.force[k]);
                out.insert(format!("r_{id}_{}", dofs[3 + k]), r.moment[k]);
            }
        }
        for u in &usage {
            if let Some(x) = u.usage {
                out.insert(format!("usage_{}", model.members[u.member].id), x);
            }
        }
        out.insert("iterations".into(), s.iterations as f64);
        out.insert("passes".into(), f64::from(s.passes));
        Ok(out)
    }
}

/// `spotting.level`: optimum spotting on level ground with a dead-end and
/// a tangent type of the same height.
struct SpottingLevel;

impl Calculation for SpottingLevel {
    fn id(&self) -> &'static str {
        "spotting.level"
    }

    fn description(&self) -> &'static str {
        "Optimum spotting on level ground (inputs: length, height, clearance, \
         horizontal_tension, weight, step, dead_end_cost, tangent_cost)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        use ohl_spotting::{Candidate, Problem, default_budget, spot};
        let length = quantity_input::<Length>(inputs, "length")?.si();
        let height = quantity_input::<Length>(inputs, "height")?.si();
        let candidate = |id: &str, cost: f64, dead_end: bool| Candidate {
            structure_type: id.into(),
            extension: None,
            height,
            cost,
            dead_end,
            capacity: None,
        };
        let problem = Problem {
            ground: vec![[0.0, 0.0], [length, 0.0]],
            required: Vec::new(),
            forbidden: Vec::new(),
            candidates: vec![
                candidate("dead-end", float_input(inputs, "dead_end_cost")?, true),
                candidate("tangent", float_input(inputs, "tangent_cost")?, false),
            ],
            step: quantity_input::<Length>(inputs, "step")?.si(),
            max_span: length,
            min_span: 0.0,
            clearance: quantity_input::<Length>(inputs, "clearance")?.si(),
            horizontal_tension: quantity_input::<Force>(inputs, "horizontal_tension")?.si(),
            weight: quantity_input::<ForcePerLength>(inputs, "weight")?.si(),
            span_cost: 0.0,
            wire_cost_per_m: 0.0,
        };
        let r = spot(&problem, &default_budget()).map_err(|e| e.to_string())?;
        Ok(outputs([
            ("structures", r.structures.len() as f64),
            ("total_cost", r.total),
            (
                "longest_span_m",
                r.spans.iter().map(|s| s.length).fold(0.0, f64::max),
            ),
        ]))
    }
}

/// `galloping.ellipses`: the ellipse of one method (`major = major_sag · D
/// + major_constant`, `minor = minor_of_major · A`) at fraction `at` of a
/// span, and the distance to the same ellipse of a second wire offset by
/// (`offset_right`, `offset_up`).
struct GallopingEllipses;

impl Calculation for GallopingEllipses {
    fn id(&self) -> &'static str {
        "galloping.ellipses"
    }

    fn description(&self) -> &'static str {
        "Galloping ellipse of a span and the gap to a second wire's (inputs: sag, span, diameter, \
         loops, major_sag, major_constant, minor_of_major, tilt_deg, centre_above_rest, at, \
         offset_right, offset_up)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        use ohl_clearance::galloping::{Along, AxisFormula, Method, ellipse_distance};
        let len = |k: &str| quantity_input::<Length>(inputs, k).map(|l| l.si());
        let method = Method {
            name: "validation".into(),
            loops: float_input(inputs, "loops")? as u32,
            major: AxisFormula {
                sag: float_input(inputs, "major_sag")?,
                constant: len("major_constant")?,
                ..AxisFormula::default()
            },
            minor: AxisFormula {
                major: float_input(inputs, "minor_of_major")?,
                ..AxisFormula::default()
            },
            tilt_deg: float_input(inputs, "tilt_deg")?,
            centre_above_rest: float_input(inputs, "centre_above_rest")?,
            along: Along::Sine,
        };
        let e = method.ellipse(
            len("sag")?,
            len("span")?,
            len("diameter")?,
            float_input(inputs, "at")?,
        );
        let other = e.moved([len("offset_right")?, len("offset_up")?]);
        Ok(outputs([
            ("major_m", 2.0 * e.semi_major),
            ("minor_m", 2.0 * e.semi_minor),
            ("centre_right_m", e.centre[0]),
            ("centre_up_m", e.centre[1]),
            ("distance_m", ellipse_distance(&e, &other)),
        ]))
    }
}

/// `structures.member_wind`: the member wind on a structure model with
/// the wind factors of a section library (drag per shape, face drag and
/// shielding by solidity), and the faces it finds.
struct StructureMemberWind;

impl Calculation for StructureMemberWind {
    fn id(&self) -> &'static str {
        "structures.member_wind"
    }

    fn description(&self) -> &'static str {
        "Member wind with faces, solidity and shielding (inputs: model = .ohlfe text, sections \
         = .ohlsec text with its [wind] table or none, wind_pressure, wind_direction in degrees \
         from x towards y); outputs fx, fy, fz (N, the sum), faces, shielded (members), \
         solidity_max (of the faces), shielding_min (the smallest factor applied, 1: none)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let library = ohl_sections::SectionLibrary::parse(string_input(inputs, "sections")?)?;
        let model = ohl_structures_fe::Model::parse_with(string_input(inputs, "model")?, &library)?;
        let q = quantity_input::<Stress>(inputs, "wind_pressure")?.si();
        let a = float_input(inputs, "wind_direction")?.to_radians();
        let setup = ohl_structures_fe::wind::MemberWindSetup::new(&model, library.wind.as_ref());
        let w = setup.wind(q, [a.cos(), a.sin(), 0.0]);
        let total = w.total();
        Ok(outputs([
            ("fx", total[0]),
            ("fy", total[1]),
            ("fz", total[2]),
            ("faces", setup.faces().len() as f64),
            (
                "shielded",
                w.members.iter().filter(|r| r.shielded_by.is_some()).count() as f64,
            ),
            (
                "solidity_max",
                setup.faces().iter().map(|f| f.solidity).fold(0.0, f64::max),
            ),
            (
                "shielding_min",
                w.members.iter().map(|r| r.shielding).fold(1.0, f64::min),
            ),
        ]))
    }
}

/// `structures.pole`: the pole check under one load and a wind pressure.
struct StructurePole;

impl Calculation for StructurePole {
    fn id(&self) -> &'static str {
        "structures.pole"
    }

    fn description(&self) -> &'static str {
        "Pole check (inputs: pole = .ohlp text, load_point_{x,y,z}, load_{x,y,z}, wind_pressure, \
         optional p_delta); outputs ground_moment_nm, wind_moment_nm, ground_capacity_nm, \
         ground_usage, usage, amplification"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let pole = ohl_structures_fe::Pole::parse(string_input(inputs, "pole")?)?;
        let at = vector_input::<Length>(inputs, "load_point")?;
        let f = vector_input::<Force>(inputs, "load")?;
        let load = ohl_structures_fe::PoleLoad {
            point: [at.x, at.y, at.z],
            force: [f.x, f.y, f.z],
        };
        let wind = quantity_input::<Stress>(inputs, "wind_pressure")?.si();
        let p_delta = inputs
            .get("p_delta")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let c = ohl_structures_fe::check_pole(
            &pole,
            &[load],
            [wind, 0.0],
            1.0,
            p_delta,
            &ohl_structures_fe::default_budget(),
        )?;
        let cap = pole.capacity_at(0.0);
        Ok(outputs([
            ("ground_moment_nm", c.ground_moment),
            ("wind_moment_nm", c.wind_moment),
            ("ground_capacity_nm", cap),
            ("ground_usage", c.ground_moment / cap),
            ("usage", c.usage),
            ("amplification", c.amplification),
        ]))
    }
}

/// `loads.structure_wind`: the wind on a structure's own body in a load
/// case (`ohl_loads::LoadCase::structure_wind`) and on a wind band of it.
struct StructureWindRule;

impl Calculation for StructureWindRule {
    fn id(&self) -> &'static str {
        "loads.structure_wind"
    }

    fn description(&self) -> &'static str {
        "The structures' wind of a load case (inputs: load_cases = .ohllc text with one case, \
         weather_pressure, band_area, band_drag); outputs pressure_pa, factor, factored_pa, \
         wire_factored_pa (the wires': weather × transverse), band_force_n, band_factored_n"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let cases = ohl_loads::LoadCases::parse(string_input(inputs, "load_cases")?)?;
        let case = cases.cases.first().ok_or("no load case")?;
        let pressure = quantity_input::<Stress>(inputs, "weather_pressure")?;
        let weather: WeatherCase = toml::from_str(&format!(
            "name = \"{}\"\ntemperature = \"15 °C\"\nwind_pressure = \"{} Pa\"\n",
            case.weather,
            pressure.si()
        ))
        .map_err(|e| e.to_string())?;
        let area = quantity_input::<Area>(inputs, "band_area")?.si();
        let drag = float_input(inputs, "band_drag")?;
        let body = case.structure_wind(&weather);
        let band = body.pressure * drag * area;
        Ok(outputs([
            ("pressure_pa", body.pressure),
            ("factor", body.factor),
            ("factored_pa", body.factored()),
            ("wire_factored_pa", pressure.si() * case.transverse),
            ("band_force_n", band),
            ("band_factored_n", band * body.factor),
        ]))
    }
}

/// `wirefit.catenary_fit`: a catenary fitted to points sampled on a known
/// one (`from` to `to` every `step` along the span).
struct CatenaryFit;

impl Calculation for CatenaryFit {
    fn id(&self) -> &'static str {
        "wirefit.catenary_fit"
    }

    fn description(&self) -> &'static str {
        "Catenary fitted to points of a known catenary (inputs: catenary_constant, low_point_s, \
         low_point_z, span, from, to, step)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        use ohl_clearance::wirefit::{PlaneCatenary, fit_catenary};
        let len = |k: &str| quantity_input::<Length>(inputs, k).map(|l| l.si());
        let truth = PlaneCatenary {
            c: len("catenary_constant")?,
            s0: len("low_point_s")?,
            z0: len("low_point_z")?,
        };
        let (from, to, step) = (len("from")?, len("to")?, len("step")?);
        if !(step > 0.0 && to > from) {
            return Err("step must be positive and to above from".into());
        }
        let n = ((to - from) / step).round() as usize;
        let s: Vec<f64> = (0..=n).map(|k| from + k as f64 * step).collect();
        let z: Vec<f64> = s.iter().map(|&v| truth.z(v)).collect();
        let fit = fit_catenary(&s, &z, &ohl_core::budget::Budget::new().max_iterations(200))?;
        Ok(outputs([
            ("catenary_m", fit.curve.c),
            ("low_point_s_m", fit.curve.s0),
            ("low_point_z_m", fit.curve.z0),
            ("sag_m", fit.curve.sag(0.0, len("span")?)),
            ("rms_m", fit.rms),
        ]))
    }
}

/// `clearance.surface`: clearances of a level span to its conductor's
/// surface. The wire is a catenary of `span`, constant `catenary` (H/w)
/// and attachments `height` above flat ground, as 64 pieces even in x (a
/// vertex at the low point), of a conductor of `diameter` with
/// `ice_thickness` (radial, glaze ice). Outputs: the surface radius and
/// the polyline's bound (`ohl_line::solve::polyline_error`), the vertical
/// and radial clearance to the ground and the radial clearance to a point
/// `point_offset` across the low point at its height — each to the
/// surface, less the bound.
struct ClearanceSurface;

impl Calculation for ClearanceSurface {
    fn id(&self) -> &'static str {
        "clearance.surface"
    }

    fn description(&self) -> &'static str {
        "Clearances of a level span to the conductor's surface (inputs: span, catenary, height, \
         diameter, ice_thickness, point_offset); outputs surface_radius_m, polyline_error_m, \
         ground_vertical_m, ground_radial_m, point_radial_m"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        use ohl_clearance::checks::{PointCheck, WireSet, ground_check};
        let len = |k: &str| quantity_input::<Length>(inputs, k).map(|l| l.si());
        let (span, c, height) = (len("span")?, len("catenary")?, len("height")?);
        let offset = len("point_offset")?;
        if !(span > 0.0 && c > 0.0 && height > 0.0 && offset > 0.0) {
            return Err("span, catenary, height and point_offset must be above 0".into());
        }
        let case = WeatherCase {
            ice_thickness: quantity_input::<Length>(inputs, "ice_thickness")?,
            ..WeatherCase::still("validation", Temperature::from_celsius(0.0))
        };
        let radius = case.iced_diameter(quantity_input(inputs, "diameter")?).si() / 2.0;
        let sag = c * ((span / (2.0 * c)).cosh() - 1.0);
        let points: Vec<Vector3<f64>> = (0..=64)
            .map(|k| {
                let x = span * k as f64 / 64.0;
                let z = height - sag + c * (((x - span / 2.0) / c).cosh() - 1.0);
                Vector3::new(x, 0.0, z)
            })
            .collect();
        let error = ohl_line::solve::polyline_error(&points, c);
        let wire = ohl_clearance::Wire {
            section: "S".into(),
            span: 0,
            circuit: String::new(),
            phase: String::new(),
            potential: None,
            bundle: Vec::new(),
            points: points.iter().map(|p| [p.x, p.y, p.z]).collect(),
            ends: Vec::new(),
            surface_radius: radius,
            polyline_error: error,
        };
        let flat = |x: f64, y: f64| ohl_terrain::SurveyPoint {
            x,
            y,
            z: 0.0,
            code: String::new(),
            line: String::new(),
        };
        let ground = ohl_terrain::Terrain::from_points(
            &[
                flat(-10.0, -50.0),
                flat(span + 10.0, -50.0),
                flat(span + 10.0, 50.0),
                flat(-10.0, 50.0),
            ],
            None,
        )
        .map_err(|e| e.to_string())?;
        let rules = ohl_clearance::RuleSet::parse(&format!(
            "[[rule]]\nname = \"g\"\ntarget = \"ground\"\nvertical = {height}\nradial = {height}\n\
             [[rule]]\nname = \"p\"\ntarget = \"points\"\nclasses = [1]\nradial = {offset}\n"
        ))?;
        let g = &ground_check(&rules.rules[0], std::slice::from_ref(&wire), &ground)[0];
        let set = WireSet::new(vec![wire], offset + 10.0);
        let mut points_check = PointCheck::new(&rules.rules[1], &[1], &set);
        points_check.visit(&set, span / 2.0, offset, height - sag, 1);
        let hit = |h: Option<ohl_clearance::Hit>| h.map_or(f64::NAN, |h| h.distance);
        Ok(outputs([
            ("surface_radius_m", radius),
            ("polyline_error_m", error),
            ("ground_vertical_m", hit(g.vertical)),
            ("ground_radial_m", hit(g.radial)),
            ("point_radial_m", hit(points_check.results[0].radial)),
        ]))
    }
}

/// `vegetation.fall_in`: a tree at the origin (base on the ground) of
/// height `tree_height` above it, grown by `growth`, root-ball fraction
/// `root_ball`, allowance `allowance`, falling towards a straight level
/// wire along y at x = `wire_x`, z = `wire_z` (no radius). The ground:
/// `level`, a `slope` (z = slope · x) or a `ridge` (level, 5 m high at
/// x = 10 between x = 8 and 12). Outputs: the resting angle along +x and
/// the fall-in margin.
struct FallIn;

impl Calculation for FallIn {
    fn id(&self) -> &'static str {
        "vegetation.fall_in"
    }

    fn description(&self) -> &'static str {
        "A tree falling about its root ball towards a straight wire (inputs: ground, slope, \
         tree_height, growth, root_ball, allowance, wire_x, wire_z); outputs theta_min_deg, \
         fall_margin_m"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        use ohl_clearance::vegetation::{VegetationCheck, VegetationParams};
        let len = |k: &str| quantity_input::<Length>(inputs, k).map(|l| l.si());
        let num = |k: &str| float_input(inputs, k);
        let ground = string_input(inputs, "ground")?;
        let slope = if ground == "slope" {
            num("slope")?
        } else {
            0.0
        };
        let pts: Vec<[f64; 3]> = match ground {
            "level" | "slope" => [
                [-200.0, -200.0],
                [200.0, -200.0],
                [200.0, 200.0],
                [-200.0, 200.0],
            ]
            .into_iter()
            .map(|[x, y]| [x, y, slope * x])
            .collect(),
            "ridge" => [
                (-50.0, 0.0),
                (8.0, 0.0),
                (10.0, 5.0),
                (12.0, 0.0),
                (80.0, 0.0),
            ]
            .into_iter()
            .flat_map(|(x, z)| [[x, -50.0, z], [x, 50.0, z]])
            .collect(),
            other => return Err(format!("ground \"{other}\": level, slope or ridge")),
        };
        let terrain = ohl_terrain::Terrain::from_xyz(&pts, None).map_err(|e| e.to_string())?;
        let (h, growth, k) = (len("tree_height")?, len("growth")?, num("root_ball")?);
        let (wx, wz) = (len("wire_x")?, len("wire_z")?);
        let wire = ohl_clearance::Wire {
            section: "S".into(),
            span: 0,
            circuit: String::new(),
            phase: String::new(),
            potential: None,
            bundle: Vec::new(),
            points: (0..=100)
                .map(|i| [wx, -100.0 + 2.0 * i as f64, wz])
                .collect(),
            ends: Vec::new(),
            surface_radius: 0.0,
            polyline_error: 0.0,
        };
        let params = VegetationParams {
            clearance: 0.0,
            growth_per_year: growth,
            years: 1.0,
            fall_in: true,
            root_ball: k,
            fall_allowance: len("allowance")?,
            max_tree_height: h + 1.0,
            // The case's ground has its points far from the tree.
            max_base_ground_distance: None,
            ..VegetationParams::default()
        };
        let mut check = VegetationCheck::new(vec![wire], &[5], params);
        let z0 = terrain.height(0.0, 0.0).ok_or("no ground at the tree")?;
        check.visit(Some(&terrain), 0.0, 0.0, z0 + h, 5);
        let r = check.finish().remove(0);
        let margin = r
            .worst_fall
            .and_then(|v| v.fall_margin)
            .ok_or("no fall found")?;
        let tree = ohl_clearance::fall::Tree::new([0.0, 0.0, z0], h + growth, k);
        let theta = tree.theta_min(&terrain, [1.0, 0.0]);
        Ok(outputs([
            ("theta_min_deg", theta.to_degrees()),
            ("fall_margin_m", margin),
        ]))
    }
}

/// `vegetation.swing_arc`: a level span along x from (0, 0, z) to
/// (`span`, 0, z), z = `attachment_height`, hanging as a parabola of sag
/// `still_sag` in still air and `blown_sag` blown by ±`blowout_angle`
/// about its axis (21 points each); the swing arc at the place of point
/// (`point_s`, `point_y`, `point_z`) and the point's distance to it.
struct SwingArcCase;

impl Calculation for SwingArcCase {
    fn id(&self) -> &'static str {
        "vegetation.swing_arc"
    }

    fn description(&self) -> &'static str {
        "The swing arc of a level span and a point's distance to it (inputs: span, \
         attachment_height, still_sag, blown_sag, blowout_angle, point_s, point_y, point_z); \
         outputs arc_radius_m, arc_from_deg, arc_to_deg, distance_m"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        use ohl_clearance::positions::{Axis, SwingArc, WireShape};
        let len = |k: &str| quantity_input::<Length>(inputs, k).map(|l| l.si());
        let (span, z) = (len("span")?, len("attachment_height")?);
        let phi = float_input(inputs, "blowout_angle")?.to_radians();
        let axis = Axis::new([0.0, 0.0, z], [span, 0.0, z]).ok_or("a span of no length")?;
        let wire = |angle: f64, sag: f64| {
            let points = (0..=20)
                .map(|k| {
                    let x = span * k as f64 / 20.0;
                    let r = 4.0 * sag * (x / span) * (1.0 - x / span);
                    [x, -r * angle.sin(), z - r * angle.cos()]
                })
                .collect();
            WireShape::new(
                ohl_clearance::Wire {
                    section: "S".into(),
                    span: 0,
                    circuit: String::new(),
                    phase: String::new(),
                    potential: None,
                    bundle: Vec::new(),
                    points,
                    ends: Vec::new(),
                    surface_radius: 0.0,
                    polyline_error: 0.0,
                },
                &axis,
            )
        };
        let (still, blown) = (len("still_sag")?, len("blown_sag")?);
        let arc = SwingArc::new(
            axis,
            &wire(-phi, blown),
            &wire(0.0, still),
            &wire(phi, blown),
        )
        .ok_or("no arc")?;
        let q = [len("point_s")?, len("point_y")?, len("point_z")?];
        let place = arc.place(q[0]);
        let (d, _) = arc.nearest(q);
        Ok(outputs([
            ("arc_radius_m", place.radius),
            ("arc_from_deg", place.from.to_degrees()),
            ("arc_to_deg", place.to.to_degrees()),
            ("distance_m", d),
        ]))
    }
}

/// `clearance.swing_arc`: a span from (0, 0, z) to (`span`, 0, z +
/// `rise`), z = `attachment_height`, hanging in the plane square to its
/// axis as a parabola of sag `still_sag` (measured square to the axis)
/// in still air and `blown_sag` blown by ±`blowout_angle` about its axis
/// (21 points each); the point (`point_x`, `point_y`, `point_z`)'s
/// radial distance to the swing arc and its vertical clearance to it in
/// the vertical zone of half width `band` and depth `depth` (the
/// vertical and radial clearance checks' swing arc).
struct SwingArcClearance;

impl Calculation for SwingArcClearance {
    fn id(&self) -> &'static str {
        "clearance.swing_arc"
    }

    fn description(&self) -> &'static str {
        "The vertical and radial clearance of a point to a span's swing arc (inputs: span, \
         rise, attachment_height, still_sag, blown_sag, blowout_angle, point_x, point_y, \
         point_z, band, depth); outputs radial_m, vertical_m, vertical_angle_deg"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        use ohl_clearance::positions::{Axis, SwingArc, WireShape};
        let len = |k: &str| quantity_input::<Length>(inputs, k).map(|l| l.si());
        let (span, rise, z) = (len("span")?, len("rise")?, len("attachment_height")?);
        let phi = float_input(inputs, "blowout_angle")?.to_radians();
        let axis = Axis::new([0.0, 0.0, z], [span, 0.0, z + rise]).ok_or("a span of no length")?;
        let wire = |angle: f64, sag: f64| {
            let points = (0..=20)
                .map(|k| {
                    let t = k as f64 / 20.0;
                    let r = 4.0 * sag * t * (1.0 - t);
                    let (c, s) = (angle.cos(), angle.sin());
                    std::array::from_fn(|i| {
                        axis.a1[i]
                            + t * (axis.a2[i] - axis.a1[i])
                            + r * (c * axis.down[i] + s * axis.right[i])
                    })
                })
                .collect();
            WireShape::new(
                ohl_clearance::Wire {
                    section: "S".into(),
                    span: 0,
                    circuit: String::new(),
                    phase: String::new(),
                    potential: None,
                    bundle: Vec::new(),
                    points,
                    ends: Vec::new(),
                    surface_radius: 0.0,
                    polyline_error: 0.0,
                },
                &axis,
            )
        };
        let (still, blown) = (len("still_sag")?, len("blown_sag")?);
        let arc = SwingArc::new(
            axis,
            &wire(-phi, blown),
            &wire(0.0, still),
            &wire(phi, blown),
        )
        .ok_or("no arc")?;
        let q = [len("point_x")?, len("point_y")?, len("point_z")?];
        let (d, _) = arc.nearest(q);
        let v = arc
            .vertical(q, len("band")?, len("depth")?)
            .ok_or("the point is not under or over the arc within the band")?;
        Ok(outputs([
            ("radial_m", d),
            ("vertical_m", v.gap),
            ("vertical_angle_deg", v.angle.to_degrees()),
        ]))
    }
}

/// `line.span_cases`: a section of two level 300 m spans solved by the
/// line's solver ([`ohl_line::solve::solve_section_span_cases`]) with
/// span 0 in a weather case of its own. Dead-end clamps fixed at
/// (0, 0, 27) and (600, 0, 27) m; in the middle a suspension string from
/// (300, 0, 30) m, 3 m, 800 N, axial stiffness 5e7 N. Cable: one
/// component, EA = 400 mm² × 86.6 GPa = 34.64 MN, α = 18.9e-6 /K,
/// 1.6285 kg/m, 28 mm; strung at 28 kN at 15 °C in the blocks. The
/// section's case: still air at `temperature`, no ice; span 0's: still
/// air at `span_temperature` with `span_ice_load` (glaze ice). Outputs:
/// both spans' horizontal tensions, the middle clamp's position along
/// the line and height, the string's swing along the line (positive
/// towards span 1) and its force.
struct SpanCasesSection;

impl Calculation for SpanCasesSection {
    fn id(&self) -> &'static str {
        "line.span_cases"
    }

    fn description(&self) -> &'static str {
        "Two level spans and a suspension string, span 0 in its own weather case (inputs: \
         temperature, span_temperature, span_ice_load); outputs h0_n, h1_n, clamp_x_m, \
         clamp_z_m, swing_deg, string_n"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        use ohl_line::solve::{SpanCases, solve_section_span_cases};
        let cable: ohl_cables::Cable = toml::from_str(
            r#"
id = "span-cases"
name = "Synthetic homogeneous test cable"
kind = "aaac"
source = "Synthetic test data, not a real conductor"
diameter = "28 mm"
mass_per_length = "1.6285 kg/m"
rated_tensile_strength = "120 kN"
outer = { area = "400 mm²", modulus = "86.6 GPa", thermal_expansion = 18.9e-6 }
"#,
        )
        .map_err(|e| e.to_string())?;
        let line = ohl_line::Line::from_json(
            r#"{
  "format": "ohl-line", "version": 6, "name": "span cases", "crs": "local",
  "structures": [
    {"id": "T1", "position": [0, 0, 0], "orientation_deg": -90,
     "wires": [{"label": "W1", "position": [0, 0, 27], "set": 1}]},
    {"id": "T2", "position": [300, 0, 0], "orientation_deg": -90,
     "points": [{"label": "A1", "position": [300, 0, 30]}],
     "strings": [{"label": "S1", "top": "A1", "wire": "W1", "length": 3.0,
                  "weight": 800.0, "stiffness": 5e7}],
     "wires": [{"label": "W1", "position": [300, 0, 27], "set": 1}]},
    {"id": "T3", "position": [600, 0, 0], "orientation_deg": -90,
     "wires": [{"label": "W1", "position": [600, 0, 27], "set": 1}]}
  ],
  "sections": [{"id": "S", "cable": "span-cases", "bundle": 1,
    "points": [{"structure": "T1", "set": 1}, {"structure": "T2", "set": 1},
               {"structure": "T3", "set": 1}],
    "stringing": {"kind": "tension", "horizontal_tension": 28000.0, "temperature": 15.0}}]
}"#,
        )
        .map_err(|e| e.to_string())?;
        let base = WeatherCase::still(
            "base",
            quantity_input::<Temperature>(inputs, "temperature")?,
        );
        let mut own = WeatherCase::still(
            "span 0",
            quantity_input::<Temperature>(inputs, "span_temperature")?,
        );
        let ice = quantity_input::<ForcePerLength>(inputs, "span_ice_load")?;
        if ice.si() > 0.0 {
            own.ice_load = Some(ice);
        }
        let cases = SpanCases::uniform(base).with_span(0, own);
        let shape = solve_section_span_cases(
            &line,
            &line.sections[0],
            &cable,
            &cases,
            &default_fe_budget(),
        )
        .map_err(|e| e.to_string())?;
        let clamp = shape.clamps[1];
        let string = shape.strings[1].first().ok_or("no string")?;
        let (top, bottom) = (string.top, string.bottom);
        let swing = (bottom[0] - top[0]).atan2(top[2] - bottom[2]).to_degrees();
        Ok(outputs([
            ("h0_n", shape.spans[0].horizontal_tension),
            ("h1_n", shape.spans[1].horizontal_tension),
            ("clamp_x_m", clamp[0]),
            ("clamp_z_m", clamp[2]),
            ("swing_deg", swing),
            ("string_n", string.force),
        ]))
    }
}

/// `safety_volume.mesh`: the safety volume of a straight level wire
/// along x (`length`, 30 m up, conductor radius `radius`, distance
/// `distance`, tolerance `tolerance`; `shape` `still`: the wire alone, a
/// capsule; `arc`: the swing arc of the wire turned rigidly by
/// ±`blowout_angle` about an axis `arc_radius` above it). Outputs: the
/// mesh's open edges, whether every sampled point of the exact surface is
/// inside, its volume over the capsule's (`still`), and at mid-span the
/// section's reach to the side and below the axis (`arc`).
struct SafetyVolumeCase;

impl Calculation for SafetyVolumeCase {
    fn id(&self) -> &'static str {
        "safety_volume.mesh"
    }

    fn description(&self) -> &'static str {
        "The safety volume of a straight wire, still or on a swing arc (inputs: shape, length, \
         radius, distance, tolerance, arc_radius, blowout_angle); outputs open_edges, \
         exact_inside, volume_ratio, lateral_m, below_m"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        use ohl_clearance::positions::{Axis, PositionKind, Positions, Shape, SwingArc, WireShape};
        use ohl_clearance::rules::Requirement;
        use ohl_clearance::safety_volume::{SafetyParams, build};
        let len = |k: &str| quantity_input::<Length>(inputs, k).map(|l| l.si());
        let (l, r, d, tol) = (
            len("length")?,
            len("radius")?,
            len("distance")?,
            len("tolerance")?,
        );
        let z0 = 30.0;
        let wire = |y: f64, z: f64| ohl_clearance::Wire {
            section: "S".into(),
            span: 0,
            circuit: "c".into(),
            phase: "L1".into(),
            potential: None,
            bundle: Vec::new(),
            points: (0..=10).map(|k| [l * k as f64 / 10.0, y, z]).collect(),
            ends: Vec::new(),
            surface_radius: r,
            polyline_error: 0.0,
        };
        let params = SafetyParams {
            distance: Requirement::Fixed(d),
            tolerance: tol,
            ..SafetyParams::default()
        };
        let reach = d + r;
        let budget =
            ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(600));
        match string_input(inputs, "shape")? {
            "still" => {
                let v = build(
                    &Positions::of_wires("still", vec![wire(0.0, z0)]),
                    &params,
                    &budget,
                )?;
                let m = &v.volumes[0].mesh;
                let exact = std::f64::consts::PI * reach * reach * (l + 4.0 / 3.0 * reach);
                let mut inside = true;
                for i in 0..=20 {
                    for j in 0..16 {
                        let a = std::f64::consts::TAU * f64::from(j) / 16.0;
                        let x = l * f64::from(i) / 20.0;
                        inside &= m.contains([x, reach * a.cos(), z0 + reach * a.sin()]);
                    }
                }
                Ok(outputs([
                    ("open_edges", m.open_edges() as f64),
                    ("exact_inside", f64::from(u8::from(inside))),
                    ("volume_ratio", m.volume() / exact),
                ]))
            }
            "arc" => {
                let ra = len("arc_radius")?;
                let phi = float_input(inputs, "blowout_angle")?.to_radians();
                let axis = Axis::new([0.0, 0.0, z0], [l, 0.0, z0]).ok_or("no axis")?;
                let turned = |p: f64| WireShape::new(wire(-ra * p.sin(), z0 - ra * p.cos()), &axis);
                let arc = SwingArc::new(axis, &turned(-phi), &turned(0.0), &turned(phi))
                    .ok_or("no arc")?;
                let mut pos = Positions::default();
                pos.cases.push("wind".into());
                let k = pos.add_span(wire(0.0, z0 - ra), axis);
                pos.add(k, 0, PositionKind::Arc, Shape::Arc(arc));
                let v = build(&pos, &params, &budget)?;
                let m = &v.volumes[0].mesh;
                // The section at mid-span: where the edges cross x = l/2.
                let (mut lateral, mut below) = (0.0f64, 0.0f64);
                let x = 0.5 * l;
                for t in &m.triangles {
                    let c = m.corners(t);
                    for e in 0..3 {
                        let (a, b) = (c[e], c[(e + 1) % 3]);
                        if (a[0] - x) * (b[0] - x) < 0.0 {
                            let s = (x - a[0]) / (b[0] - a[0]);
                            lateral = lateral.max((a[1] + s * (b[1] - a[1])).abs());
                            below = below.max(z0 - (a[2] + s * (b[2] - a[2])));
                        }
                    }
                }
                let mut inside = true;
                for k in 0..=20 {
                    let p = -phi + 2.0 * phi * f64::from(k) / 20.0;
                    inside &= m.contains([x, -(ra + reach) * p.sin(), z0 - (ra + reach) * p.cos()]);
                }
                Ok(outputs([
                    ("open_edges", m.open_edges() as f64),
                    ("exact_inside", f64::from(u8::from(inside))),
                    ("lateral_m", lateral),
                    ("below_m", below),
                ]))
            }
            other => Err(format!("shape \"{other}\": still or arc")),
        }
    }
}
