// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Position of the sun and solar irradiance on a conductor.
//!
//! Solar position (IEEE Std 738-2012, clause 4.4.3 / Annex A, equations
//! 16a–17b; CIGRE TB 601, section 3.3): declination
//! \\( \delta = A \sin(360° (284 + N) / 365) \\) with \\( A = 23.46° \\)
//! (IEEE) or 23.3° (CIGRE), hour angle \\( \omega = 15° (t - 12) \\) of the
//! local solar time t, altitude
//! \\( \sin H_c = \cos\varphi \cos\delta \cos\omega + \sin\varphi \sin\delta \\)
//! and azimuth \\( Z_c = C + \arctan\chi \\),
//! \\( \chi = \sin\omega / (\sin\varphi \cos\omega - \cos\varphi \tan\delta) \\),
//! where the constant C of the standard's table is what `atan2` gives.
//!
//! The beam meets the conductor at the angle θ,
//! \\( \cos\theta = \cos H_c \cos(Z_c - Z_l) \\) (IEEE eq. 9) for a line of
//! azimuth \\( Z_l \\).
//!
//! Irradiance: IEEE's total heat flux on a surface normal to the beam is a
//! polynomial of the altitude for a clear or an industrial atmosphere
//! (IEEE eq. 18, Table 4), times an elevation factor (eq. 20). CIGRE TB 601
//! splits it into the direct beam and the diffuse sky radiation
//! (TB 601 eqs. for \\( I_B \\) and \\( I_d \\), section 3.3).

use serde::Deserialize;

/// Atmosphere for the IEEE 738 solar heat flux (and the CIGRE clearness
/// ratio when none is given).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Atmosphere {
    /// Clear air.
    #[default]
    Clear,
    /// Industrial (polluted) air.
    Industrial,
}

impl Atmosphere {
    /// Its name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Atmosphere::Clear => "clear",
            Atmosphere::Industrial => "industrial",
        }
    }

    /// CIGRE clearness ratio \\( N_s \\) assumed for this atmosphere: 1.0
    /// clear, 0.8 industrial (an assumption of OpenOHL; give
    /// `clearness_ratio` to choose).
    pub fn clearness_ratio(self) -> f64 {
        match self {
            Atmosphere::Clear => 1.0,
            Atmosphere::Industrial => 0.8,
        }
    }
}

/// Where the sun is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolarPosition {
    /// Solar declination, degrees.
    pub declination: f64,
    /// Hour angle, degrees (negative before noon).
    pub hour_angle: f64,
    /// Altitude above the horizon, degrees (negative: below).
    pub altitude: f64,
    /// Azimuth, degrees clockwise from north.
    pub azimuth: f64,
}

/// IEEE 738 declination amplitude, degrees (eq. 16b).
pub const IEEE_DECLINATION: f64 = 23.46;
/// CIGRE TB 601 declination amplitude, degrees.
pub const CIGRE_DECLINATION: f64 = 23.3;

/// Day of the year (1 = 1 January) of a date of the Gregorian calendar.
pub fn day_of_year(year: i32, month: u32, day: u32) -> Result<u32, String> {
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=12).contains(&month) || day == 0 || day > days[month as usize - 1] {
        return Err(format!("{year}-{month:02}-{day:02} is not a date"));
    }
    Ok(days[..month as usize - 1].iter().sum::<u32>() + day)
}

/// The sun at `latitude` (degrees, north positive) on day `day` of the
/// year at local solar time `hour` (11.5 = 11:30), with the declination
/// amplitude `amplitude` ([`IEEE_DECLINATION`] or [`CIGRE_DECLINATION`]).
pub fn position(latitude: f64, day: u32, hour: f64, amplitude: f64) -> SolarPosition {
    let declination = amplitude * (360.0 * (284.0 + day as f64) / 365.0).to_radians().sin();
    let hour_angle = 15.0 * (hour - 12.0);
    let (lat, dec, w) = (
        latitude.to_radians(),
        declination.to_radians(),
        hour_angle.to_radians(),
    );
    let altitude = (lat.cos() * dec.cos() * w.cos() + lat.sin() * dec.sin())
        .clamp(-1.0, 1.0)
        .asin()
        .to_degrees();
    // IEEE eq. 17: Zc = C + arctan(χ); atan2 picks the quadrant of C.
    let azimuth = (180.0
        + w.sin()
            .atan2(lat.sin() * w.cos() - lat.cos() * dec.tan())
            .to_degrees())
    .rem_euclid(360.0);
    SolarPosition {
        declination,
        hour_angle,
        altitude,
        azimuth,
    }
}

/// Angle between the sun's beam and a conductor of azimuth `line_azimuth`
/// (degrees from north), degrees (IEEE eq. 9); without an azimuth, 90°
/// (the beam across the conductor: the most heating). Between 0° and 90°:
/// a line has no direction.
pub fn incidence(sun: &SolarPosition, line_azimuth: Option<f64>) -> f64 {
    match line_azimuth {
        Some(zl) => (sun.altitude.to_radians().cos() * (sun.azimuth - zl).to_radians().cos())
            .abs()
            .min(1.0)
            .acos()
            .to_degrees(),
        None => 90.0,
    }
}

/// IEEE 738 total solar heat flux on a surface normal to the beam at sea
/// level, W/m², for a solar altitude in degrees (eq. 18, SI coefficients of
/// Table 4); 0 with the sun below the horizon.
pub fn ieee_flux(altitude: f64, atmosphere: Atmosphere) -> f64 {
    if altitude <= 0.0 {
        return 0.0;
    }
    let c: [f64; 7] = match atmosphere {
        Atmosphere::Clear => [
            -42.2391,
            63.8044,
            -1.9220,
            3.46921e-2,
            -3.61118e-4,
            1.94318e-6,
            -4.07608e-9,
        ],
        Atmosphere::Industrial => [
            53.1821, 14.2110, 6.6138e-1, -3.1658e-2, 5.4654e-4, -4.3446e-6, 1.3236e-8,
        ],
    };
    c.iter()
        .rev()
        .fold(0.0, |acc, k| acc * altitude + k)
        .max(0.0)
}

/// IEEE 738 elevation correction of the solar flux (eq. 20) at `elevation`
/// m above sea level.
pub fn ieee_elevation_factor(elevation: f64) -> f64 {
    1.0 + 1.148e-4 * elevation - 1.108e-8 * elevation * elevation
}

/// CIGRE TB 601 direct beam \\( I_B \\) (on a surface normal to the beam)
/// and diffuse \\( I_d \\) (on a horizontal surface) irradiance, W/m², at a
/// solar altitude in degrees, `elevation` m and clearness ratio
/// `clearness`; zero with the sun below the horizon.
pub fn cigre_irradiance(altitude: f64, elevation: f64, clearness: f64) -> (f64, f64) {
    if altitude <= 0.0 {
        return (0.0, 0.0);
    }
    let s = altitude.to_radians().sin();
    let beam0 = clearness * 1280.0 * s / (s + 0.314);
    let beam = if beam0 > 0.0 {
        beam0 * (1.0 + 1.4e-4 * elevation * (1367.0 / beam0 - 1.0))
    } else {
        0.0
    };
    let diffuse = ((430.5 - 0.3288 * beam) * s).max(0.0);
    (beam, diffuse)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_of_the_year() {
        assert_eq!(day_of_year(2023, 1, 1).unwrap(), 1);
        assert_eq!(day_of_year(2023, 6, 10).unwrap(), 161);
        assert_eq!(day_of_year(2024, 6, 10).unwrap(), 162);
        assert_eq!(day_of_year(2023, 12, 31).unwrap(), 365);
        assert!(day_of_year(2023, 2, 29).is_err());
        assert!(day_of_year(2023, 13, 1).is_err());
    }

    #[test]
    fn noon_sun_is_south_at_the_complement_of_latitude_minus_declination() {
        let sun = position(47.0, 80, 12.0, IEEE_DECLINATION);
        assert!((sun.altitude - (90.0 - 47.0 + sun.declination)).abs() < 1e-9);
        assert!((sun.azimuth - 180.0).abs() < 1e-9);
        // Morning in the east half, afternoon in the west half.
        assert!(position(47.0, 172, 9.0, IEEE_DECLINATION).azimuth < 180.0);
        assert!(position(47.0, 172, 15.0, IEEE_DECLINATION).azimuth > 180.0);
        // Southern hemisphere: the noon sun is north.
        let south = position(-35.0, 172, 12.0, IEEE_DECLINATION);
        assert!(south.azimuth.abs() < 1e-9 || (south.azimuth - 360.0).abs() < 1e-9);
    }

    #[test]
    fn early_summer_morning_sun_is_north_of_east() {
        // At 30° N in June the sun rises north of east.
        let sun = position(30.0, 161, 6.5, IEEE_DECLINATION);
        assert!(sun.altitude > 0.0 && sun.azimuth < 90.0, "{sun:?}");
    }

    #[test]
    fn flux_polynomials() {
        // Clear air at 90°: about 1 kW/m²; nothing at night.
        let top = ieee_flux(90.0, Atmosphere::Clear);
        assert!((1000.0..1150.0).contains(&top), "{top}");
        assert!(ieee_flux(90.0, Atmosphere::Industrial) < top);
        assert_eq!(ieee_flux(-5.0, Atmosphere::Clear), 0.0);
        assert!((ieee_elevation_factor(0.0) - 1.0).abs() < 1e-15);
        assert!(ieee_elevation_factor(1000.0) > 1.1);
        let (b, d) = cigre_irradiance(60.0, 0.0, 1.0);
        assert!(b > 800.0 && d > 0.0 && d < 200.0, "{b} {d}");
        let (b_high, _) = cigre_irradiance(60.0, 2000.0, 1.0);
        assert!(b_high > b);
    }
}
