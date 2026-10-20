// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Calculations for the textbook and published validation cases
//! (`benchmarks/*/textbook-*.toml`, `benchmarks/sagtension/cigre-tb324.toml`):
//! a whole section of a line as the line's solver gives it, and a cable's
//! composite properties. Adapters only: they call the engines as the
//! applications do and report their results.

use ohl_core::units::{Force, Temperature};

use crate::{Calculation, Inputs, Outputs, Registry};

pub(crate) fn register(registry: &mut Registry) {
    registry.register(Box::new(LineSection));
    registry.register(Box::new(CableProperties));
}

fn table_input<T: serde::de::DeserializeOwned>(inputs: &Inputs, name: &str) -> Result<T, String> {
    inputs
        .get(name)
        .ok_or_else(|| format!("missing input \"{name}\""))?
        .clone()
        .try_into()
        .map_err(|e| format!("input \"{name}\": {e}"))
}

fn string_input<'a>(inputs: &'a Inputs, name: &str) -> Result<&'a str, String> {
    inputs
        .get(name)
        .ok_or_else(|| format!("missing input \"{name}\""))?
        .as_str()
        .ok_or_else(|| format!("input \"{name}\" must be a string"))
}

/// `line.section`: one section of a line (`.ohl.json` text) solved by the
/// line's solver ([`ohl_line::solve::solve_section`]) — its own
/// calculation (condition and method) — with one cable (a wire library
/// entry as a table) in one weather case (a `.ohlwc` case as a table, also
/// the line's only weather case, so the creep and load conditions can name
/// it). With `clip_in = true` a section in the blocks is first clipped in
/// as the app does it ([`ohl_line::solve::clip_lengths`]: each span keeps
/// its unstressed length of the stringing condition, so the suspension
/// strings swing and the spans' tensions differ); otherwise it is solved
/// as given (in the blocks: the same horizontal tension in every span).
///
/// Outputs, numbered from 0 in the section's order:
/// - per span `i`: `h_i_n` (horizontal tension of its first element),
///   `max_tension_i_n`, `sag_i_m` (largest vertical distance below the
///   clamp-to-clamp chord), `mid_{x,y,z}_i_m` (the wire at half the
///   horizontal clamp distance) and `low_{x,y,z}_i_m` (its lowest point).
///   The last three come from the solver's 65 points per element: exact
///   where a point falls there (the middle of a symmetric span);
/// - per section point `k`: `clamp_{x,y,z}_k_m`;
/// - per string `j` holding point `k`: `string_k_j_n` (axial force, tension
///   positive), `string_k_j_slack` (1 slack, 0 taut), the swing across and
///   along the line `string_k_j_across_deg`, `string_k_j_along_deg`, and the
///   string's bottom `string_k_j_{x,y,z}_m`.
struct LineSection;

impl Calculation for LineSection {
    fn id(&self) -> &'static str {
        "line.section"
    }

    fn description(&self) -> &'static str {
        "One section of a line solved by the line's solver (inputs: line = .ohl.json text, \
         cable = wire library entry, weather = one weather case, optional section index); \
         outputs per span h_i_n, max_tension_i_n, sag_i_m, mid_{x,y,z}_i_m, low_{x,y,z}_i_m, \
         per point clamp_{x,y,z}_k_m, per string string_k_j_n, string_k_j_slack, \
         string_k_j_across_deg, string_k_j_along_deg, string_k_j_{x,y,z}_m"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let mut line =
            ohl_line::Line::from_json(string_input(inputs, "line")?).map_err(|e| e.to_string())?;
        let cable: ohl_cables::Cable = table_input(inputs, "cable")?;
        cable.validate().map_err(|e| e.to_string())?;
        let weather: ohl_weather::WeatherCase = table_input(inputs, "weather")?;
        weather.validate()?;
        line.set_weather_cases(std::slice::from_ref(&weather));
        let index = match inputs.get("section") {
            Some(v) => usize::try_from(
                v.as_integer()
                    .ok_or("input \"section\" must be an integer")?,
            )
            .map_err(|e| e.to_string())?,
            None => 0,
        };
        let mut section = line
            .sections
            .get(index)
            .ok_or_else(|| format!("no section {index}"))?
            .clone();
        if inputs
            .get("clip_in")
            .and_then(toml::Value::as_bool)
            .unwrap_or(false)
            && !section.stringing.clipped()
        {
            // As the app's Clip in: each span's unstressed length at the
            // stringing condition, on the current geometry.
            let (lengths, temperature) = ohl_line::solve::clip_lengths(&line, &section, &cable)
                .map_err(|e| e.to_string())?;
            section.stringing = ohl_line::Stringing::UnstressedLengths {
                lengths,
                temperature,
                sagged: Some(Box::new(section.stringing.clone())),
            };
            line.sections[index] = section.clone();
        }
        let shape = ohl_line::solve::solve_section(
            &line,
            &section,
            &cable,
            &weather,
            &ohl_wirefe::default_fe_budget(),
        )
        .map_err(|e| e.to_string())?;
        let mut out = Outputs::new();
        let xyz = ["x", "y", "z"];
        for (i, s) in shape.spans.iter().enumerate() {
            out.insert(format!("h_{i}_n"), s.horizontal_tension);
            out.insert(format!("max_tension_{i}_n"), s.max_tension);
            out.insert(format!("sag_{i}_m"), s.sag);
            for (k, axis) in xyz.iter().enumerate() {
                out.insert(format!("mid_{axis}_{i}_m"), s.mid_point[k]);
                out.insert(format!("low_{axis}_{i}_m"), s.low_point[k]);
            }
        }
        for (k, c) in shape.clamps.iter().enumerate() {
            for (a, axis) in xyz.iter().enumerate() {
                out.insert(format!("clamp_{axis}_{k}_m"), c[a]);
            }
        }
        for (k, strings) in shape.strings.iter().enumerate() {
            for (j, s) in strings.iter().enumerate() {
                out.insert(format!("string_{k}_{j}_n"), s.force);
                out.insert(
                    format!("string_{k}_{j}_slack"),
                    f64::from(u8::from(s.slack)),
                );
                out.insert(format!("string_{k}_{j}_across_deg"), s.swing.across_deg);
                out.insert(format!("string_{k}_{j}_along_deg"), s.swing.along_deg);
                for (a, axis) in xyz.iter().enumerate() {
                    out.insert(format!("string_{k}_{j}_{axis}_m"), s.bottom[a]);
                }
            }
        }
        Ok(out)
    }
}

/// `cables.properties`: a cable's composite properties (a wire library
/// entry as a table): `area_mm2`, `weight_n_per_m`, `axial_stiffness_n`
/// (\\( \sum E_iA_i \\)), `modulus_gpa` (\\( EA/A \\)),
/// `thermal_expansion_per_k` (stiffness-weighted), and with an optional
/// `tension` the total strain the cable's mechanics give at that tension
/// at their reference temperature, initial state (`strain`).
struct CableProperties;

impl Calculation for CableProperties {
    fn id(&self) -> &'static str {
        "cables.properties"
    }

    fn description(&self) -> &'static str {
        "Composite properties of a cable (inputs: cable = wire library entry, optional tension); \
         outputs area_mm2, weight_n_per_m, axial_stiffness_n, modulus_gpa, \
         thermal_expansion_per_k, strain"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let cable: ohl_cables::Cable = table_input(inputs, "cable")?;
        cable.validate().map_err(|e| e.to_string())?;
        let mut out = Outputs::from([
            ("area_mm2".to_owned(), cable.area().si() * 1e6),
            (
                "weight_n_per_m".to_owned(),
                cable.weight().newtons_per_meter(),
            ),
            (
                "axial_stiffness_n".to_owned(),
                cable.axial_stiffness().newtons(),
            ),
            ("modulus_gpa".to_owned(), cable.modulus().si() / 1e9),
            (
                "thermal_expansion_per_k".to_owned(),
                cable.thermal_expansion(),
            ),
        ]);
        if inputs.contains_key("tension") {
            let tension: Force = string_input(inputs, "tension")?
                .parse()
                .map_err(|e| format!("input \"tension\": {e}"))?;
            let mechanics =
                ohl_cables::mechanics::Mechanics::from_cable(&cable).map_err(|e| e.to_string())?;
            let at: Temperature = mechanics.reference_temperature();
            let strain = mechanics
                .strain(tension, at, &ohl_cables::mechanics::MaterialState::Initial)
                .map_err(|e| e.to_string())?;
            out.insert("strain".to_owned(), strain);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(text: &str) -> Inputs {
        toml::from_str(text).unwrap()
    }

    const CABLE: &str = r#"cable = { id = "c", name = "c", kind = "acsr", source = "test", diameter = "28 mm", mass_per_length = "1.6 kg/m", rated_tensile_strength = "120 kN", outer = { area = "400 mm²", modulus = "50 GPa", thermal_expansion = 23e-6 }, core = { area = "100 mm²", modulus = "200 GPa", thermal_expansion = 11e-6 } }"#;

    /// The composite values are the stiffness-weighted sums, and the
    /// strain at a tension is the tension over EA for a linear cable.
    #[test]
    fn cable_properties_are_the_composite_values() {
        let out = CableProperties
            .run(&inputs(&format!("tension = \"40 kN\"\n{CABLE}")))
            .unwrap();
        let ea = 400e-6 * 50e9 + 100e-6 * 200e9;
        assert!((out["axial_stiffness_n"] - ea).abs() < 1e-6 * ea);
        assert!((out["modulus_gpa"] - ea / 500e-6 / 1e9).abs() < 1e-9);
        let alpha = (20e6 * 23e-6 + 20e6 * 11e-6) / ea;
        assert!((out["thermal_expansion_per_k"] - alpha).abs() < 1e-15);
        assert!((out["strain"] - 40e3 / ea).abs() < 1e-12);
    }

    /// Missing or wrong inputs are reported, not panicked on.
    #[test]
    fn line_section_reports_bad_inputs() {
        let err = LineSection.run(&inputs(CABLE)).unwrap_err();
        assert!(err.contains("\"line\""), "{err}");
        let err = LineSection
            .run(&inputs(&format!("line = \"{{}}\"\n{CABLE}")))
            .unwrap_err();
        assert!(!err.is_empty());
    }
}
