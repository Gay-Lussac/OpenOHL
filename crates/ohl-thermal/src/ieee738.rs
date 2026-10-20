// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Heat terms of IEEE Std 738-2012, SI form (W/m, m, °C).
//!
//! - Air at the film temperature \\( T_{film} = (T_s + T_a)/2 \\): dynamic
//!   viscosity (eq. 13a), density at elevation \\( H_e \\) (eq. 14a),
//!   thermal conductivity (eq. 15a).
//! - Forced convection: the larger of eqs. 3a and 3b (low and high
//!   Reynolds number), times the wind direction factor of eq. 4a.
//! - Natural convection: eq. 5a. The larger of forced and natural
//!   convection is used (clause 4.4.3.1).
//! - Radiated heat: eq. 7a.

use crate::Convection;

/// Air properties at the film temperature `t_film` °C and `elevation` m:
/// dynamic viscosity Pa·s, density kg/m³, thermal conductivity W/(m·K).
pub fn air(t_film: f64, elevation: f64) -> (f64, f64, f64) {
    let mu = 1.458e-6 * (t_film + 273.0).powf(1.5) / (t_film + 383.4);
    let rho = (1.293 - 1.525e-4 * elevation + 6.379e-9 * elevation * elevation)
        / (1.0 + 0.00367 * t_film);
    let k = 2.424e-2 + 7.477e-5 * t_film - 4.407e-9 * t_film * t_film;
    (mu, rho, k)
}

/// Wind direction factor \\( K_{angle} \\) for the angle `phi` (degrees)
/// between the wind and the conductor axis (eq. 4a): 1 across the
/// conductor, 0.388 along it. The formula holds for 0–90°, so any angle
/// is first folded to the acute angle to the axis
/// ([`crate::acute_wind_angle`]): 180° is wind along the line, 270° across.
pub fn wind_direction_factor(phi: f64) -> f64 {
    let p = crate::acute_wind_angle(phi).to_radians();
    1.194 - p.cos() + 0.194 * (2.0 * p).cos() + 0.368 * (2.0 * p).sin()
}

/// Convective cooling, W/m, of a conductor of diameter `d` m at `ts` °C in
/// air at `ta` °C with wind `speed` m/s at `angle` degrees to its axis, at
/// `elevation` m; and which convection governs.
pub fn convection(
    d: f64,
    ts: f64,
    ta: f64,
    speed: f64,
    angle: f64,
    elevation: f64,
) -> (f64, Convection) {
    let dt = ts - ta;
    if dt <= 0.0 {
        return (0.0, Convection::Natural);
    }
    let (mu, rho, k) = air(0.5 * (ts + ta), elevation);
    let re = d * rho * speed / mu;
    let k_angle = wind_direction_factor(angle);
    let low = k_angle * (1.01 + 1.35 * re.powf(0.52)) * k * dt;
    let high = k_angle * 0.754 * re.powf(0.6) * k * dt;
    let forced = if speed > 0.0 { low.max(high) } else { 0.0 };
    let natural = 3.645 * rho.sqrt() * d.powf(0.75) * dt.powf(1.25);
    if forced >= natural {
        (forced, Convection::Forced)
    } else {
        (natural, Convection::Natural)
    }
}

/// Radiated heat loss, W/m (eq. 7a): \\( 17.8 D \varepsilon
/// [((T_s + 273)/100)^4 - ((T_a + 273)/100)^4] \\).
pub fn radiation(d: f64, emissivity: f64, ts: f64, ta: f64) -> f64 {
    let k4 = |t: f64| ((t + 273.0) / 100.0).powi(4);
    17.8 * d * emissivity * (k4(ts) - k4(ta))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wind_factor_across_and_along() {
        assert!((wind_direction_factor(90.0) - 1.0).abs() < 1e-12);
        assert!((wind_direction_factor(0.0) - 0.388).abs() < 1e-12);
    }

    #[test]
    fn wind_factor_folds_the_angle() {
        // Each angle and the acute angle it folds to.
        for (phi, folded) in [
            (0.0, 0.0),
            (90.0, 90.0),
            (180.0, 0.0),
            (270.0, 90.0),
            (-45.0, 45.0),
            (135.0, 45.0),
            (450.0, 90.0),
        ] {
            let k = wind_direction_factor(phi);
            assert!(
                (k - wind_direction_factor(folded)).abs() < 1e-9,
                "{phi}°: {k}"
            );
            assert!((0.388 - 1e-3..=1.0 + 1e-3).contains(&k), "{phi}°: {k}");
        }
        assert!((wind_direction_factor(180.0) - 0.388).abs() < 1e-3);
        assert!((wind_direction_factor(270.0) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn air_at_70_degrees() {
        let (mu, rho, k) = air(70.0, 0.0);
        assert!((mu - 2.043e-5).abs() < 2e-8, "{mu}");
        assert!((rho - 1.029).abs() < 1e-3, "{rho}");
        assert!((k - 0.02945).abs() < 1e-5, "{k}");
    }
}
