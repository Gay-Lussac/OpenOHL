// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Convective cooling of CIGRE Technical Brochures 601 (2014) and 207
//! (2002), SI form (W/m, m, °C).
//!
//! Both use Nusselt numbers: forced convection across the conductor
//! \\( Nu_{90} = B\,Re^n \\) (Morgan's coefficients by Reynolds number and
//! surface roughness \\( R_s = d / (2(D - d)) \\)), corrected for the angle δ
//! between wind and conductor, \\( Nu_\delta = Nu_{90}(A_1 + B_1 \sin^{m_1}\delta) \\);
//! natural convection \\( Nu_{nat} = A_2 (Gr\,Pr)^{m_2} \\); the heat loss is
//! \\( P_c = \pi \lambda_f (T_s - T_a) Nu \\).
//!
//! Where they differ:
//!
//! - **air properties** — TB 601 uses quadratic fits of the thermal
//!   conductivity and dynamic viscosity with the density at elevation
//!   (the same density formula as IEEE 738); TB 207 a linear conductivity
//!   and kinematic viscosity with a relative density
//!   \\( e^{-1.16 \cdot 10^{-4} y} \\) and a linear Prandtl number;
//! - **low wind** (below 0.5 m/s) — TB 601 takes at least
//!   \\( 0.55\,Nu_{90} \\); TB 207 at least the Nusselt number at δ = 45°.
//!
//! Either way the largest of forced, low-wind and natural convection is
//! used. Reynolds numbers below 100 and Gr·Pr beyond 10⁶ use the nearest
//! range of the tables (extrapolated).

use crate::Convection;

/// Specific heat of air used for the TB 601 Prandtl number, J/(kg·K).
const AIR_SPECIFIC_HEAT: f64 = 1005.0;
/// Standard gravity, m/s².
const G: f64 = 9.806_65;
/// Below this wind speed, m/s, the low-wind rules apply.
pub const LOW_WIND: f64 = 0.5;

/// Air at the film temperature `t_film` °C and `elevation` m, TB 601:
/// thermal conductivity W/(m·K), kinematic viscosity m²/s, Prandtl number.
pub fn air_601(t_film: f64, elevation: f64) -> (f64, f64, f64) {
    let lambda = 2.368e-2 + 7.23e-5 * t_film - 2.763e-8 * t_film * t_film;
    let mu = (17.239 + 4.635e-2 * t_film - 2.03e-5 * t_film * t_film) * 1e-6;
    let rho = (1.293 - 1.525e-4 * elevation + 6.379e-9 * elevation * elevation)
        / (1.0 + 0.00367 * t_film);
    (lambda, mu / rho, AIR_SPECIFIC_HEAT * mu / lambda)
}

/// Air at the film temperature `t_film` °C and `elevation` m, TB 207:
/// thermal conductivity W/(m·K), kinematic viscosity m²/s (at that
/// elevation), Prandtl number.
pub fn air_207(t_film: f64, elevation: f64) -> (f64, f64, f64) {
    let lambda = 2.42e-2 + 7.2e-5 * t_film;
    let nu_sea = 1.32e-5 + 9.5e-8 * t_film;
    let relative_density = (-1.16e-4 * elevation).exp();
    (lambda, nu_sea / relative_density, 0.715 - 2.5e-4 * t_film)
}

/// Forced convection Nusselt number across the conductor for Reynolds
/// number `re` and roughness `rs`.
pub fn nusselt_90(re: f64, rs: f64) -> f64 {
    let (b, n) = if re <= 2650.0 {
        (0.641, 0.471)
    } else if rs <= 0.05 {
        (0.178, 0.633)
    } else {
        (0.048, 0.800)
    };
    b * re.powf(n)
}

/// Factor on \\( Nu_{90} \\) for wind at `delta` degrees to the conductor;
/// any angle is first folded to the acute angle to the axis
/// ([`crate::acute_wind_angle`]): 180° is wind along the line, 270° across.
pub fn angle_factor(delta: f64) -> f64 {
    let delta = crate::acute_wind_angle(delta);
    let s = delta.to_radians().sin();
    if delta <= 24.0 {
        0.42 + 0.68 * s.powf(1.08)
    } else {
        0.42 + 0.58 * s.powf(0.90)
    }
}

/// Natural convection Nusselt number for the product of the Grashof and
/// Prandtl numbers.
pub fn nusselt_natural(gr_pr: f64) -> f64 {
    let (a, m) = if gr_pr < 1e2 {
        (1.02, 0.148)
    } else if gr_pr < 1e4 {
        (0.850, 0.188)
    } else {
        (0.480, 0.250)
    };
    a * gr_pr.max(0.0).powf(m)
}

/// Convective cooling, W/m, of a conductor of diameter `d` m and roughness
/// `rs` at `ts` °C in air at `ta` °C with wind `speed` m/s at `angle`
/// degrees to its axis, at `elevation` m; `tb207` for the older brochure.
#[allow(clippy::too_many_arguments)]
pub fn convection(
    d: f64,
    rs: f64,
    ts: f64,
    ta: f64,
    speed: f64,
    angle: f64,
    elevation: f64,
    tb207: bool,
) -> (f64, Convection) {
    let dt = ts - ta;
    if dt <= 0.0 {
        return (0.0, Convection::Natural);
    }
    let t_film = 0.5 * (ts + ta);
    let (lambda, nu, pr) = if tb207 {
        air_207(t_film, elevation)
    } else {
        air_601(t_film, elevation)
    };
    let re = speed * d / nu;
    let nu_90 = nusselt_90(re, rs);
    let forced = if speed > 0.0 {
        nu_90 * angle_factor(angle)
    } else {
        0.0
    };
    let low_wind = if speed < LOW_WIND && speed > 0.0 {
        if tb207 {
            nu_90 * angle_factor(45.0)
        } else {
            0.55 * nu_90
        }
    } else {
        0.0
    };
    let gr = G * d.powi(3) * dt / ((t_film + 273.0) * nu * nu);
    let natural = nusselt_natural(gr * pr);
    let factor = std::f64::consts::PI * lambda * dt;
    if natural > forced.max(low_wind) {
        (factor * natural, Convection::Natural)
    } else if low_wind > forced {
        (factor * low_wind, Convection::LowWind)
    } else {
        (factor * forced, Convection::Forced)
    }
}

/// Radiated heat loss, W/m: \\( \pi D \sigma \varepsilon
/// [(T_s + 273)^4 - (T_a + 273)^4] \\).
pub fn radiation(d: f64, emissivity: f64, ts: f64, ta: f64) -> f64 {
    const SIGMA: f64 = 5.6697e-8;
    std::f64::consts::PI * d * SIGMA * emissivity * ((ts + 273.0).powi(4) - (ta + 273.0).powi(4))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_continuous_at_their_limits() {
        let a = 0.641 * 2650f64.powf(0.471);
        assert!((nusselt_90(2650.1, 0.1) / a - 1.0).abs() < 0.01);
        assert!((nusselt_90(2650.1, 0.03) / a - 1.0).abs() < 0.01);
        assert!((nusselt_natural(99.999) / nusselt_natural(100.0) - 1.0).abs() < 0.01);
        assert!((nusselt_natural(9999.9) / nusselt_natural(1e4) - 1.0).abs() < 0.01);
        assert!((angle_factor(90.0) - 1.0).abs() < 1e-12);
        assert!((angle_factor(24.0) - angle_factor(24.0001)).abs() < 0.01);
        assert!((angle_factor(120.0) - angle_factor(60.0)).abs() < 1e-12);
    }

    #[test]
    fn angle_factor_folds_the_angle() {
        // Each angle and the acute angle it folds to.
        for (delta, folded) in [
            (0.0, 0.0),
            (90.0, 90.0),
            (180.0, 0.0),
            (270.0, 90.0),
            (-45.0, 45.0),
            (135.0, 45.0),
            (450.0, 90.0),
        ] {
            assert!(
                (angle_factor(delta) - angle_factor(folded)).abs() < 1e-9,
                "{delta}°"
            );
        }
        assert!((angle_factor(270.0) - 1.0).abs() < 1e-9);
        assert!((angle_factor(180.0) - 0.42).abs() < 1e-9);
    }

    #[test]
    fn both_brochures_agree_roughly() {
        let a = convection(0.0281, 0.09, 100.0, 40.0, 0.61, 90.0, 0.0, false).0;
        let b = convection(0.0281, 0.09, 100.0, 40.0, 0.61, 90.0, 0.0, true).0;
        assert!((a / b - 1.0).abs() < 0.05, "{a} {b}");
        // Still air: natural convection.
        let (c, mode) = convection(0.0281, 0.09, 100.0, 40.0, 0.0, 90.0, 0.0, false);
        assert_eq!(mode, Convection::Natural);
        assert!(c > 20.0 && c < a);
    }
}
