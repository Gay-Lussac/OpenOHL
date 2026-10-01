// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Typed physical quantities.
//!
//! Every quantity is a zero-cost wrapper around an `f64` stored in SI units.
//! Conversions to other unit systems (US customary, kN, MPa, …) happen only at
//! the boundaries — input, output and display — so engine code pays nothing
//! for unit safety (ADR-0002).
//!
//! ```
//! use ols_core::units::{Force, ForcePerLength, Length};
//!
//! let w = ForcePerLength::from_newtons_per_meter(15.97);
//! let span = Length::from_feet(1000.0);
//! let total: Force = w * span;
//! assert!((total.kilonewtons() - 4.8677).abs() < 1e-3);
//! ```

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// Standard acceleration of gravity, m/s² (CGPM 1901).
pub const STANDARD_GRAVITY: f64 = 9.806_65;

/// Exact conversion factors to SI.
pub mod factors {
    /// International foot, m.
    pub const FOOT: f64 = 0.3048;
    /// International inch, m.
    pub const INCH: f64 = 0.0254;
    /// International mile, m.
    pub const MILE: f64 = 1609.344;
    /// Avoirdupois pound, kg.
    pub const POUND_MASS: f64 = 0.453_592_37;
    /// Pound-force, N.
    pub const POUND_FORCE: f64 = POUND_MASS * super::STANDARD_GRAVITY;
    /// Circular mil, m².
    pub const CIRCULAR_MIL: f64 = std::f64::consts::FRAC_PI_4 * (INCH * 1e-3) * (INCH * 1e-3);
}

use factors::{CIRCULAR_MIL, FOOT, INCH, MILE, POUND_FORCE, POUND_MASS};

macro_rules! quantity {
    ($(#[$doc:meta])* $name:ident, $si_unit:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
        #[repr(transparent)]
        pub struct $name(f64);

        impl $name {
            /// Zero.
            pub const ZERO: Self = Self(0.0);
            /// Symbol of the SI unit the value is stored in.
            pub const SI_UNIT: &'static str = $si_unit;

            /// Creates the quantity from a value in SI units.
            #[inline]
            pub const fn from_si(value: f64) -> Self {
                Self(value)
            }

            /// Value in SI units.
            #[inline]
            pub const fn si(self) -> f64 {
                self.0
            }

            /// Absolute value.
            #[inline]
            pub fn abs(self) -> Self {
                Self(self.0.abs())
            }

            /// Smaller of two values.
            #[inline]
            pub fn min(self, other: Self) -> Self {
                Self(self.0.min(other.0))
            }

            /// Larger of two values.
            #[inline]
            pub fn max(self, other: Self) -> Self {
                Self(self.0.max(other.0))
            }
        }

        impl Add for $name {
            type Output = Self;
            #[inline]
            fn add(self, rhs: Self) -> Self {
                Self(self.0 + rhs.0)
            }
        }

        impl Sub for $name {
            type Output = Self;
            #[inline]
            fn sub(self, rhs: Self) -> Self {
                Self(self.0 - rhs.0)
            }
        }

        impl AddAssign for $name {
            #[inline]
            fn add_assign(&mut self, rhs: Self) {
                self.0 += rhs.0;
            }
        }

        impl SubAssign for $name {
            #[inline]
            fn sub_assign(&mut self, rhs: Self) {
                self.0 -= rhs.0;
            }
        }

        impl Neg for $name {
            type Output = Self;
            #[inline]
            fn neg(self) -> Self {
                Self(-self.0)
            }
        }

        impl Mul<f64> for $name {
            type Output = Self;
            #[inline]
            fn mul(self, rhs: f64) -> Self {
                Self(self.0 * rhs)
            }
        }

        impl Mul<$name> for f64 {
            type Output = $name;
            #[inline]
            fn mul(self, rhs: $name) -> $name {
                $name(self * rhs.0)
            }
        }

        impl Div<f64> for $name {
            type Output = Self;
            #[inline]
            fn div(self, rhs: f64) -> Self {
                Self(self.0 / rhs)
            }
        }

        /// Ratio of two quantities of the same kind (dimensionless).
        impl Div for $name {
            type Output = f64;
            #[inline]
            fn div(self, rhs: Self) -> f64 {
                self.0 / rhs.0
            }
        }

        impl Sum for $name {
            fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
                Self(iter.map(|q| q.0).sum())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match f.precision() {
                    Some(p) => write!(f, "{:.*} {}", p, self.0, $si_unit),
                    None => write!(f, "{} {}", self.0, $si_unit),
                }
            }
        }
    };
}

/// Defines a constructor/accessor pair for a unit with a pure scale factor to SI.
macro_rules! unit {
    ($name:ident: $from:ident, $to:ident, $factor:expr, $label:literal) => {
        impl $name {
            #[doc = concat!("Creates the quantity from a value in ", $label, ".")]
            #[inline]
            pub fn $from(value: f64) -> Self {
                Self(value * $factor)
            }

            #[doc = concat!("Value in ", $label, ".")]
            #[inline]
            pub fn $to(self) -> f64 {
                self.0 / $factor
            }
        }
    };
}

/// `A * B = C` (and `B * A = C`, `C / A = B`, `C / B = A`) for distinct `A` and `B`.
macro_rules! product {
    ($a:ident * $b:ident = $c:ident) => {
        impl Mul<$b> for $a {
            type Output = $c;
            #[inline]
            fn mul(self, rhs: $b) -> $c {
                $c(self.0 * rhs.0)
            }
        }

        impl Mul<$a> for $b {
            type Output = $c;
            #[inline]
            fn mul(self, rhs: $a) -> $c {
                $c(self.0 * rhs.0)
            }
        }

        impl Div<$a> for $c {
            type Output = $b;
            #[inline]
            fn div(self, rhs: $a) -> $b {
                $b(self.0 / rhs.0)
            }
        }

        impl Div<$b> for $c {
            type Output = $a;
            #[inline]
            fn div(self, rhs: $b) -> $a {
                $a(self.0 / rhs.0)
            }
        }
    };
}

quantity!(
    /// Length, stored in metres.
    Length,
    "m"
);
unit!(Length: from_meters, meters, 1.0, "metres");
unit!(Length: from_millimeters, millimeters, 1e-3, "millimetres");
unit!(Length: from_kilometers, kilometers, 1e3, "kilometres");
unit!(Length: from_feet, feet, FOOT, "feet");
unit!(Length: from_inches, inches, INCH, "inches");
unit!(Length: from_miles, miles, MILE, "miles");

quantity!(
    /// Area, stored in square metres.
    Area,
    "m²"
);
unit!(Area: from_square_meters, square_meters, 1.0, "square metres");
unit!(Area: from_square_millimeters, square_millimeters, 1e-6, "square millimetres");
unit!(Area: from_square_inches, square_inches, INCH * INCH, "square inches");
unit!(Area: from_kcmil, kcmil, CIRCULAR_MIL * 1e3, "thousands of circular mils (kcmil / MCM)");

quantity!(
    /// Force, stored in newtons. Used for tensions and point loads.
    Force,
    "N"
);
unit!(Force: from_newtons, newtons, 1.0, "newtons");
unit!(Force: from_kilonewtons, kilonewtons, 1e3, "kilonewtons");
unit!(Force: from_pounds_force, pounds_force, POUND_FORCE, "pounds-force");
unit!(Force: from_kips, kips, POUND_FORCE * 1e3, "kips");

quantity!(
    /// Force per unit length, stored in newtons per metre. Used for wire
    /// weights, wind and ice loads, and distributed loads on wires.
    ForcePerLength,
    "N/m"
);
unit!(ForcePerLength: from_newtons_per_meter, newtons_per_meter, 1.0, "newtons per metre");
unit!(ForcePerLength: from_pounds_force_per_foot, pounds_force_per_foot, POUND_FORCE / FOOT, "pounds-force per foot");

quantity!(
    /// Stress or pressure, stored in pascals. Used for moduli, stresses and
    /// wind pressures.
    Stress,
    "Pa"
);
unit!(Stress: from_pascals, pascals, 1.0, "pascals");
unit!(Stress: from_megapascals, megapascals, 1e6, "megapascals");
unit!(Stress: from_gigapascals, gigapascals, 1e9, "gigapascals");
unit!(Stress: from_psi, psi, POUND_FORCE / (INCH * INCH), "pounds-force per square inch");
unit!(Stress: from_psf, psf, POUND_FORCE / (FOOT * FOOT), "pounds-force per square foot");

quantity!(
    /// Mass per unit length, stored in kilograms per metre.
    MassPerLength,
    "kg/m"
);
unit!(MassPerLength: from_kilograms_per_meter, kilograms_per_meter, 1.0, "kilograms per metre");
unit!(MassPerLength: from_pounds_per_foot, pounds_per_foot, POUND_MASS / FOOT, "pounds per foot");

impl MassPerLength {
    /// Weight per unit length under standard gravity.
    #[inline]
    pub fn weight(self) -> ForcePerLength {
        ForcePerLength(self.0 * STANDARD_GRAVITY)
    }
}

quantity!(
    /// Plane angle, stored in radians.
    Angle,
    "rad"
);
unit!(Angle: from_radians, radians, 1.0, "radians");
unit!(Angle: from_degrees, degrees, std::f64::consts::PI / 180.0, "degrees");

quantity!(
    /// Speed, stored in metres per second. Used for wind speeds.
    Speed,
    "m/s"
);
unit!(Speed: from_meters_per_second, meters_per_second, 1.0, "metres per second");
unit!(Speed: from_kilometers_per_hour, kilometers_per_hour, 1.0 / 3.6, "kilometres per hour");
unit!(Speed: from_miles_per_hour, miles_per_hour, MILE / 3600.0, "miles per hour");

quantity!(
    /// Thermodynamic temperature, stored in kelvins.
    ///
    /// Differences between two temperatures are also expressed as a
    /// `Temperature` (a difference of 1 K equals a difference of 1 °C).
    Temperature,
    "K"
);

impl Temperature {
    /// Offset between the Kelvin and Celsius scales.
    const CELSIUS_OFFSET: f64 = 273.15;

    /// Creates a temperature from kelvins.
    #[inline]
    pub fn from_kelvin(value: f64) -> Self {
        Self(value)
    }

    /// Value in kelvins.
    #[inline]
    pub fn kelvin(self) -> f64 {
        self.0
    }

    /// Creates a temperature from degrees Celsius.
    #[inline]
    pub fn from_celsius(value: f64) -> Self {
        Self(value + Self::CELSIUS_OFFSET)
    }

    /// Value in degrees Celsius.
    #[inline]
    pub fn celsius(self) -> f64 {
        self.0 - Self::CELSIUS_OFFSET
    }

    /// Creates a temperature from degrees Fahrenheit.
    #[inline]
    pub fn from_fahrenheit(value: f64) -> Self {
        Self::from_celsius((value - 32.0) * 5.0 / 9.0)
    }

    /// Value in degrees Fahrenheit.
    #[inline]
    pub fn fahrenheit(self) -> f64 {
        self.celsius() * 9.0 / 5.0 + 32.0
    }
}

product!(ForcePerLength * Length = Force);
product!(Stress * Area = Force);
product!(Stress * Length = ForcePerLength);

impl Mul for Length {
    type Output = Area;
    #[inline]
    fn mul(self, rhs: Self) -> Area {
        Area(self.0 * rhs.0)
    }
}

impl Div<Length> for Area {
    type Output = Length;
    #[inline]
    fn div(self, rhs: Length) -> Length {
        Length(self.0 / rhs.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f64, expected: f64, rel_tol: f64) {
        let scale = expected.abs().max(1e-300);
        assert!(
            ((actual - expected) / scale).abs() <= rel_tol,
            "actual {actual} differs from expected {expected} by more than {rel_tol} (relative)"
        );
    }

    #[test]
    fn length_conversions() {
        assert_eq!(Length::from_feet(1.0).meters(), 0.3048);
        assert_close(Length::from_inches(12.0).feet(), 1.0, 1e-15);
        assert_close(Length::from_miles(1.0).feet(), 5280.0, 1e-15);
        assert_close(Length::from_kilometers(1.5).millimeters(), 1.5e6, 1e-15);
    }

    #[test]
    fn force_conversions() {
        assert_close(
            Force::from_pounds_force(1.0).newtons(),
            4.448_221_615_260_5,
            1e-15,
        );
        assert_close(
            Force::from_kips(1.0).kilonewtons(),
            4.448_221_615_260_5,
            1e-15,
        );
    }

    #[test]
    fn stress_conversions() {
        assert_close(
            Stress::from_psi(1.0).pascals(),
            6_894.757_293_168_361,
            1e-14,
        );
        assert_close(
            Stress::from_gigapascals(68.9).megapascals(),
            68_900.0,
            1e-15,
        );
    }

    #[test]
    fn conductor_area_kcmil() {
        // 1 kcmil = 0.506 707 479 mm².
        assert_close(
            Area::from_kcmil(1.0).square_millimeters(),
            0.506_707_479_097_5,
            1e-12,
        );
        // Drake ACSR, 795 kcmil aluminium area ≈ 402.8 mm².
        assert_close(
            Area::from_kcmil(795.0).square_millimeters(),
            402.832_45,
            1e-6,
        );
    }

    #[test]
    fn temperature_scales() {
        assert_close(Temperature::from_celsius(100.0).kelvin(), 373.15, 1e-15);
        assert_close(Temperature::from_fahrenheit(212.0).celsius(), 100.0, 1e-12);
        assert_close(Temperature::from_celsius(-40.0).fahrenheit(), -40.0, 1e-12);
        let delta = Temperature::from_celsius(75.0) - Temperature::from_celsius(15.0);
        assert_close(delta.kelvin(), 60.0, 1e-12);
    }

    #[test]
    fn distributed_load_times_length_is_force() {
        let w = ForcePerLength::from_newtons_per_meter(16.0);
        let span = Length::from_meters(400.0);
        let f: Force = w * span;
        assert_close(f.newtons(), 6400.0, 1e-15);
        assert_close((f / span).newtons_per_meter(), 16.0, 1e-15);
        assert_close((f / w).meters(), 400.0, 1e-15);
    }

    #[test]
    fn stress_area_and_wind_pressure() {
        let modulus = Stress::from_gigapascals(70.0);
        let area = Area::from_square_millimeters(400.0);
        assert_close((modulus * area).kilonewtons(), 28_000.0, 1e-12);

        // Wind pressure on a 28 mm conductor gives a load per metre.
        let wind: ForcePerLength = Stress::from_pascals(500.0) * Length::from_millimeters(28.0);
        assert_close(wind.newtons_per_meter(), 14.0, 1e-12);
    }

    #[test]
    fn length_squared_is_area() {
        let d = Length::from_millimeters(10.0);
        assert_close((d * d).square_millimeters(), 100.0, 1e-12);
        assert_close(
            (Area::from_square_meters(6.0) / Length::from_meters(2.0)).meters(),
            3.0,
            1e-15,
        );
    }

    #[test]
    fn mass_to_weight() {
        let m = MassPerLength::from_kilograms_per_meter(1.628);
        assert_close(
            m.weight().newtons_per_meter(),
            1.628 * STANDARD_GRAVITY,
            1e-15,
        );
        assert_close(
            MassPerLength::from_pounds_per_foot(1.0).kilograms_per_meter(),
            1.488_163_943_569_553_8,
            1e-14,
        );
    }

    #[test]
    fn angles_and_speeds() {
        assert_close(
            Angle::from_degrees(180.0).radians(),
            std::f64::consts::PI,
            1e-15,
        );
        assert_close(
            Speed::from_kilometers_per_hour(36.0).meters_per_second(),
            10.0,
            1e-15,
        );
        assert_close(
            Speed::from_miles_per_hour(1.0).meters_per_second(),
            0.447_04,
            1e-15,
        );
    }

    #[test]
    fn arithmetic_and_sum() {
        let spans = [
            Length::from_meters(300.0),
            Length::from_meters(350.0),
            Length::from_meters(410.0),
        ];
        let total: Length = spans.iter().copied().sum();
        assert_close(total.meters(), 1060.0, 1e-15);
        assert_close((total / 2.0).meters(), 530.0, 1e-15);
        assert_close(
            Length::from_meters(3.0) / Length::from_meters(1.5),
            2.0,
            1e-15,
        );
        assert_eq!(-Length::from_meters(1.0), Length::from_meters(-1.0));
    }

    #[test]
    fn display_uses_si_unit_and_precision() {
        assert_eq!(format!("{:.1}", Force::from_kilonewtons(1.25)), "1250.0 N");
        assert_eq!(format!("{}", Length::from_meters(2.5)), "2.5 m");
    }
}
