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
//! let span: Length = "1000 ft".parse().unwrap();
//! let total: Force = w * span;
//! assert!((total.kilonewtons() - 4.8677).abs() < 1e-3);
//! ```
//!
//! Quantities serialize as their plain SI value, so project files always
//! store SI numbers (ADR-0004). Text input must always carry a unit: a bare
//! number is rejected rather than guessed.

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

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

/// Unit system used to display quantities and to interpret user preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitSystem {
    /// SI with engineering prefixes (m, kN, N/m, MPa, °C, …).
    #[default]
    Metric,
    /// US customary (ft, lbf, lbf/ft, psi, °F, …).
    UsCustomary,
}

/// Error returned when parsing a quantity from text such as `"1000 ft"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseQuantityError {
    /// The text has no unit. Units are mandatory on input.
    MissingUnit,
    /// The numeric part could not be parsed.
    InvalidNumber(String),
    /// The kind of quantity is not known (see [`QUANTITY_KINDS`]).
    UnknownQuantity(String),
    /// The unit is not known for this kind of quantity.
    UnknownUnit {
        /// The unit as written.
        unit: String,
        /// The kind of quantity that was expected.
        quantity: &'static str,
    },
}

impl fmt::Display for ParseQuantityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingUnit => write!(f, "missing unit (e.g. \"12.5 m\")"),
            Self::InvalidNumber(n) => write!(f, "invalid number \"{n}\""),
            Self::UnknownQuantity(kind) => write!(f, "unknown quantity \"{kind}\""),
            Self::UnknownUnit { unit, quantity } => {
                write!(f, "unknown unit \"{unit}\" for {quantity}")
            }
        }
    }
}

impl std::error::Error for ParseQuantityError {}

/// Splits `"12.5 kN"` / `"12.5kN"` into its number and unit.
fn split_value(text: &str) -> Result<(f64, &str), ParseQuantityError> {
    let text = text.trim();
    let end = text
        .char_indices()
        .find(|&(i, c)| {
            !(c.is_ascii_digit()
                || c == '.'
                || ((c == '+' || c == '-')
                    && (i == 0 || matches!(text.as_bytes()[i - 1], b'e' | b'E')))
                || ((c == 'e' || c == 'E')
                    && text[i + 1..]
                        .starts_with(|n: char| n.is_ascii_digit() || n == '+' || n == '-')))
        })
        .map_or(text.len(), |(i, _)| i);
    let (number, unit) = text.split_at(end);
    let unit = unit.trim();
    if unit.is_empty() {
        return Err(ParseQuantityError::MissingUnit);
    }
    let value = number
        .parse::<f64>()
        .map_err(|_| ParseQuantityError::InvalidNumber(number.to_owned()))?;
    Ok((value, unit))
}

/// A value with a display unit, produced by [`Quantity::display_in`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayValue {
    /// Numeric value in `unit`.
    pub value: f64,
    /// Unit symbol.
    pub unit: &'static str,
}

impl fmt::Display for DisplayValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match f.precision() {
            Some(p) => write!(f, "{:.*} {}", p, self.value, self.unit),
            None => write!(f, "{} {}", self.value, self.unit),
        }
    }
}

/// Behaviour shared by all quantity types.
pub trait Quantity: Copy + FromStr<Err = ParseQuantityError> {
    /// Human-readable name of the kind of quantity (e.g. `"length"`).
    const KIND: &'static str;

    /// Creates the quantity from a value in SI units.
    fn from_si(value: f64) -> Self;

    /// Value in SI units.
    fn si(self) -> f64;

    /// Creates the quantity from a value in the unit with the given symbol.
    fn from_value(value: f64, unit: &str) -> Option<Self>;

    /// Value in the unit with the given symbol, if the unit is known.
    fn value_in(self, unit: &str) -> Option<f64>;

    /// Preferred display unit symbol for a unit system.
    fn display_unit(system: UnitSystem) -> &'static str;

    /// The value converted to the preferred unit of a unit system.
    fn display_in(self, system: UnitSystem) -> DisplayValue {
        let unit = Self::display_unit(system);
        DisplayValue {
            value: self.value_in(unit).expect("display unit is a known unit"),
            unit,
        }
    }
}

/// Defines a quantity type stored in SI, its units and their symbols.
///
/// Each unit line is `from_fn, to_fn, factor_to_si, "label", ["symbol", "alias", …]`.
/// The first symbol of a unit is its canonical symbol.
macro_rules! quantity {
    (
        $(#[$doc:meta])*
        $name:ident, kind = $kind:literal, si = $si_unit:literal,
        display = ($metric:literal, $us:literal),
        units = [ $( $from:ident, $to:ident, $factor:expr, $label:literal, [$($sym:literal),+] );+ $(;)? ]
    ) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize)]
        #[serde(transparent)]
        #[repr(transparent)]
        pub struct $name(f64);

        impl $name {
            /// Zero.
            pub const ZERO: Self = Self(0.0);
            /// Symbol of the SI unit the value is stored in.
            pub const SI_UNIT: &'static str = $si_unit;
            /// Known unit symbols (including aliases) and their factor to SI.
            pub const UNITS: &'static [(&'static str, f64)] = &[ $( $( ($sym, $factor), )+ )+ ];

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

            $(
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
            )+

            fn factor(unit: &str) -> Option<f64> {
                Self::UNITS.iter().find(|(s, _)| *s == unit).map(|&(_, f)| f)
            }
        }

        impl Quantity for $name {
            const KIND: &'static str = $kind;

            #[inline]
            fn from_si(value: f64) -> Self {
                Self(value)
            }

            #[inline]
            fn si(self) -> f64 {
                self.0
            }

            fn from_value(value: f64, unit: &str) -> Option<Self> {
                Self::factor(unit).map(|f| Self(value * f))
            }

            fn value_in(self, unit: &str) -> Option<f64> {
                Self::factor(unit).map(|f| self.0 / f)
            }

            fn display_unit(system: UnitSystem) -> &'static str {
                match system {
                    UnitSystem::Metric => $metric,
                    UnitSystem::UsCustomary => $us,
                }
            }
        }

        impl FromStr for $name {
            type Err = ParseQuantityError;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                let (value, unit) = split_value(text)?;
                <Self as Quantity>::from_value(value, unit).ok_or_else(|| ParseQuantityError::UnknownUnit {
                    unit: unit.to_owned(),
                    quantity: $kind,
                })
            }
        }

        arithmetic!($name);
        deserialize_quantity!($name);

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

/// Accepts either a plain number (SI units, as written by OpenOHL) or text
/// with a unit (`"28.14 mm"`, as written by people in library files).
struct QuantityVisitor<Q>(std::marker::PhantomData<Q>);

impl<Q: Quantity> serde::de::Visitor<'_> for QuantityVisitor<Q> {
    type Value = Q;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a {} as an SI number or as text with a unit", Q::KIND)
    }

    fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Q, E> {
        Ok(Q::from_si(value))
    }

    fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Q, E> {
        Ok(Q::from_si(value as f64))
    }

    fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Q, E> {
        Ok(Q::from_si(value as f64))
    }

    fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<Q, E> {
        text.parse().map_err(E::custom)
    }
}

macro_rules! deserialize_quantity {
    ($name:ident) => {
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                deserializer.deserialize_any(QuantityVisitor(std::marker::PhantomData))
            }
        }
    };
}

/// Same-kind arithmetic and scaling by plain numbers.
macro_rules! arithmetic {
    ($name:ident) => {
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
    Length, kind = "length", si = "m", display = ("m", "ft"),
    units = [
        from_meters, meters, 1.0, "metres", ["m"];
        from_millimeters, millimeters, 1e-3, "millimetres", ["mm"];
        from_kilometers, kilometers, 1e3, "kilometres", ["km"];
        from_feet, feet, FOOT, "feet", ["ft"];
        from_inches, inches, INCH, "inches", ["in"];
        from_miles, miles, MILE, "miles", ["mi"];
    ]
);

quantity!(
    /// Area, stored in square metres.
    Area, kind = "area", si = "m²", display = ("mm²", "in²"),
    units = [
        from_square_meters, square_meters, 1.0, "square metres", ["m²", "m2"];
        from_square_millimeters, square_millimeters, 1e-6, "square millimetres", ["mm²", "mm2"];
        from_square_centimeters, square_centimeters, 1e-4, "square centimetres", ["cm²", "cm2"];
        from_square_inches, square_inches, INCH * INCH, "square inches", ["in²", "in2"];
        from_kcmil, kcmil, CIRCULAR_MIL * 1e3, "thousands of circular mils (kcmil / MCM)", ["kcmil", "MCM"];
    ]
);

quantity!(
    /// Force, stored in newtons. Used for tensions and point loads.
    Force, kind = "force", si = "N", display = ("kN", "lbf"),
    units = [
        from_newtons, newtons, 1.0, "newtons", ["N"];
        from_kilonewtons, kilonewtons, 1e3, "kilonewtons", ["kN"];
        from_pounds_force, pounds_force, POUND_FORCE, "pounds-force", ["lbf", "lb"];
        from_kips, kips, POUND_FORCE * 1e3, "kips", ["kip", "kips"];
    ]
);

quantity!(
    /// Force per unit length, stored in newtons per metre. Used for wire
    /// weights, wind and ice loads, and distributed loads on wires.
    ForcePerLength, kind = "force per length", si = "N/m", display = ("N/m", "lbf/ft"),
    units = [
        from_newtons_per_meter, newtons_per_meter, 1.0, "newtons per metre", ["N/m"];
        from_kilonewtons_per_meter, kilonewtons_per_meter, 1e3, "kilonewtons per metre", ["kN/m"];
        from_pounds_force_per_foot, pounds_force_per_foot, POUND_FORCE / FOOT, "pounds-force per foot", ["lbf/ft", "lb/ft"];
    ]
);

quantity!(
    /// Stress or pressure, stored in pascals. Used for moduli, stresses and
    /// wind pressures.
    Stress, kind = "stress", si = "Pa", display = ("MPa", "psi"),
    units = [
        from_pascals, pascals, 1.0, "pascals", ["Pa"];
        from_kilopascals, kilopascals, 1e3, "kilopascals", ["kPa"];
        from_megapascals, megapascals, 1e6, "megapascals", ["MPa", "N/mm²", "N/mm2"];
        from_gigapascals, gigapascals, 1e9, "gigapascals", ["GPa"];
        from_psi, psi, POUND_FORCE / (INCH * INCH), "pounds-force per square inch", ["psi"];
        from_ksi, ksi, POUND_FORCE * 1e3 / (INCH * INCH), "kips per square inch", ["ksi"];
        from_psf, psf, POUND_FORCE / (FOOT * FOOT), "pounds-force per square foot", ["psf"];
    ]
);

quantity!(
    /// Mass per unit length, stored in kilograms per metre.
    MassPerLength, kind = "mass per length", si = "kg/m", display = ("kg/m", "lbm/ft"),
    units = [
        from_kilograms_per_meter, kilograms_per_meter, 1.0, "kilograms per metre", ["kg/m"];
        from_pounds_per_foot, pounds_per_foot, POUND_MASS / FOOT, "pounds (mass) per foot", ["lbm/ft"];
    ]
);

quantity!(
    /// Electrical resistance per unit length, stored in ohms per metre.
    ResistancePerLength, kind = "resistance per length", si = "Ω/m", display = ("Ω/km", "Ω/mi"),
    units = [
        from_ohms_per_meter, ohms_per_meter, 1.0, "ohms per metre", ["Ω/m", "ohm/m"];
        from_ohms_per_kilometer, ohms_per_kilometer, 1e-3, "ohms per kilometre", ["Ω/km", "ohm/km"];
        from_ohms_per_mile, ohms_per_mile, 1.0 / MILE, "ohms per mile", ["Ω/mi", "ohm/mi"];
        from_ohms_per_kilofoot, ohms_per_kilofoot, 1.0 / (1000.0 * FOOT), "ohms per 1000 feet", ["Ω/kft", "ohm/kft"];
    ]
);

quantity!(
    /// Density (mass per unit volume), stored in kilograms per cubic metre.
    /// Used for ice and snow.
    Density, kind = "density", si = "kg/m³", display = ("kg/m³", "lbm/ft³"),
    units = [
        from_kilograms_per_cubic_meter, kilograms_per_cubic_meter, 1.0, "kilograms per cubic metre", ["kg/m³", "kg/m3"];
        from_pounds_per_cubic_foot, pounds_per_cubic_foot, POUND_MASS / (FOOT * FOOT * FOOT), "pounds (mass) per cubic foot", ["lbm/ft³", "lbm/ft3"];
    ]
);

impl Density {
    /// Weight per unit length of a cross-section `area` of this material,
    /// under standard gravity.
    #[inline]
    pub fn weight_per_length(self, area: Area) -> ForcePerLength {
        ForcePerLength(self.0 * area.0 * STANDARD_GRAVITY)
    }
}

impl MassPerLength {
    /// Weight per unit length under standard gravity.
    #[inline]
    pub fn weight(self) -> ForcePerLength {
        ForcePerLength(self.0 * STANDARD_GRAVITY)
    }
}

quantity!(
    /// Plane angle, stored in radians.
    Angle, kind = "angle", si = "rad", display = ("°", "°"),
    units = [
        from_radians, radians, 1.0, "radians", ["rad"];
        from_degrees, degrees, std::f64::consts::PI / 180.0, "degrees", ["°", "deg"];
    ]
);

quantity!(
    /// Speed, stored in metres per second. Used for wind speeds.
    Speed, kind = "speed", si = "m/s", display = ("m/s", "mph"),
    units = [
        from_meters_per_second, meters_per_second, 1.0, "metres per second", ["m/s"];
        from_kilometers_per_hour, kilometers_per_hour, 1.0 / 3.6, "kilometres per hour", ["km/h"];
        from_miles_per_hour, miles_per_hour, MILE / 3600.0, "miles per hour", ["mph"];
    ]
);

quantity!(
    /// Second moment of area (and torsion constant), stored in m⁴. Used for
    /// cross-sections.
    SecondMoment, kind = "second moment of area", si = "m⁴", display = ("cm⁴", "in⁴"),
    units = [
        from_m4, m4, 1.0, "metres to the fourth", ["m⁴", "m4"];
        from_cm4, cm4, 1e-8, "centimetres to the fourth", ["cm⁴", "cm4"];
        from_mm4, mm4, 1e-12, "millimetres to the fourth", ["mm⁴", "mm4"];
        from_in4, in4, INCH * INCH * INCH * INCH, "inches to the fourth", ["in⁴", "in4"];
    ]
);

quantity!(
    /// Section modulus (elastic or plastic), stored in m³. Used for
    /// cross-sections.
    SectionModulus, kind = "section modulus", si = "m³", display = ("cm³", "in³"),
    units = [
        from_m3, m3, 1.0, "cubic metres", ["m³", "m3"];
        from_cm3, cm3, 1e-6, "cubic centimetres", ["cm³", "cm3"];
        from_mm3, mm3, 1e-9, "cubic millimetres", ["mm³", "mm3"];
        from_in3, in3, INCH * INCH * INCH, "cubic inches", ["in³", "in3"];
    ]
);

/// Thermodynamic temperature, stored in kelvins.
///
/// Differences between two temperatures are also expressed as a `Temperature`
/// (a difference of 1 K equals a difference of 1 °C).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct Temperature(f64);

impl Temperature {
    /// Symbol of the SI unit the value is stored in.
    pub const SI_UNIT: &'static str = "K";
    /// Offset between the Kelvin and Celsius scales.
    const CELSIUS_OFFSET: f64 = 273.15;

    /// Creates a temperature from kelvins.
    #[inline]
    pub const fn from_kelvin(value: f64) -> Self {
        Self(value)
    }

    /// Value in kelvins.
    #[inline]
    pub const fn kelvin(self) -> f64 {
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

impl Quantity for Temperature {
    const KIND: &'static str = "temperature";

    #[inline]
    fn from_si(value: f64) -> Self {
        Self(value)
    }

    #[inline]
    fn si(self) -> f64 {
        self.0
    }

    fn from_value(value: f64, unit: &str) -> Option<Self> {
        match unit {
            "K" => Some(Self::from_kelvin(value)),
            "°C" | "degC" => Some(Self::from_celsius(value)),
            "°F" | "degF" => Some(Self::from_fahrenheit(value)),
            _ => None,
        }
    }

    fn value_in(self, unit: &str) -> Option<f64> {
        match unit {
            "K" => Some(self.kelvin()),
            "°C" | "degC" => Some(self.celsius()),
            "°F" | "degF" => Some(self.fahrenheit()),
            _ => None,
        }
    }

    fn display_unit(system: UnitSystem) -> &'static str {
        match system {
            UnitSystem::Metric => "°C",
            UnitSystem::UsCustomary => "°F",
        }
    }
}

impl FromStr for Temperature {
    type Err = ParseQuantityError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (value, unit) = split_value(text)?;
        Self::from_value(value, unit).ok_or_else(|| ParseQuantityError::UnknownUnit {
            unit: unit.to_owned(),
            quantity: Self::KIND,
        })
    }
}

arithmetic!(Temperature);
deserialize_quantity!(Temperature);

impl fmt::Display for Temperature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match f.precision() {
            Some(p) => write!(f, "{:.*} K", p, self.0),
            None => write!(f, "{} K", self.0),
        }
    }
}

/// Names of every quantity kind, as accepted by [`convert`].
pub const QUANTITY_KINDS: &[&str] = &[
    Length::KIND,
    Area::KIND,
    Force::KIND,
    ForcePerLength::KIND,
    Stress::KIND,
    MassPerLength::KIND,
    ResistancePerLength::KIND,
    Density::KIND,
    Angle::KIND,
    Speed::KIND,
    Temperature::KIND,
    SecondMoment::KIND,
    SectionModulus::KIND,
];

/// Serde helpers for a plain `f64` field kept in SI: read from an SI number
/// or from text with a unit of quantity `Q` (`"0.6 m"`, `"70 N"`), as the
/// quantity types are; written as the plain number.
///
/// ```
/// #[derive(serde::Deserialize)]
/// struct T {
///     #[serde(deserialize_with = "ols_core::units::si::de::<_, ols_core::units::Length>")]
///     d: f64,
/// }
/// let t: T = serde_json::from_str(r#"{"d": "600 mm"}"#).unwrap();
/// assert_eq!(t.d, 0.6);
/// ```
pub mod si {
    use super::Quantity;
    use serde::{Deserialize, Deserializer};

    /// An `f64` in SI from a number or text with a unit of `Q`.
    pub fn de<'de, D: Deserializer<'de>, Q: Quantity + Deserialize<'de>>(
        d: D,
    ) -> Result<f64, D::Error> {
        Q::deserialize(d).map(Quantity::si)
    }

    /// An angle in degrees: a plain number is degrees (files give angles
    /// so), text takes its unit (`"10°"`, `"0.17 rad"`).
    pub fn de_degrees<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Given {
            Number(f64),
            Text(String),
        }
        match Given::deserialize(d)? {
            Given::Number(x) => Ok(x),
            Given::Text(t) => t
                .parse::<super::Angle>()
                .map(|a| a.degrees())
                .map_err(serde::de::Error::custom),
        }
    }

    /// The same for an optional field (with `default`).
    pub fn de_opt<'de, D: Deserializer<'de>, Q: Quantity + Deserialize<'de>>(
        d: D,
    ) -> Result<Option<f64>, D::Error> {
        Option::<Q>::deserialize(d).map(|q| q.map(Quantity::si))
    }
}

/// Converts text such as `"1000 ft"` of the named quantity kind to the unit `to`.
///
/// Meant for dynamic callers (scripts, validation cases, file import); Rust
/// code should use the typed quantities directly.
///
/// ```
/// assert_eq!(ols_core::units::convert("length", "1 ft", "m"), Ok(0.3048));
/// ```
pub fn convert(quantity: &str, value: &str, to: &str) -> Result<f64, ParseQuantityError> {
    fn run<Q: Quantity>(value: &str, to: &str) -> Result<f64, ParseQuantityError> {
        let parsed: Q = value.parse()?;
        parsed
            .value_in(to)
            .ok_or_else(|| ParseQuantityError::UnknownUnit {
                unit: to.to_owned(),
                quantity: Q::KIND,
            })
    }
    match quantity {
        Length::KIND => run::<Length>(value, to),
        Area::KIND => run::<Area>(value, to),
        Force::KIND => run::<Force>(value, to),
        ForcePerLength::KIND => run::<ForcePerLength>(value, to),
        Stress::KIND => run::<Stress>(value, to),
        MassPerLength::KIND => run::<MassPerLength>(value, to),
        ResistancePerLength::KIND => run::<ResistancePerLength>(value, to),
        Density::KIND => run::<Density>(value, to),
        Angle::KIND => run::<Angle>(value, to),
        Speed::KIND => run::<Speed>(value, to),
        Temperature::KIND => run::<Temperature>(value, to),
        SecondMoment::KIND => run::<SecondMoment>(value, to),
        SectionModulus::KIND => run::<SectionModulus>(value, to),
        other => Err(ParseQuantityError::UnknownQuantity(other.to_owned())),
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
            Stress::from_ksi(1.0).megapascals(),
            6.894_757_293_168_361,
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
    fn density_and_ice_weight() {
        // 1 lbm/ft³ = 16.018 463 kg/m³ (NIST SP 811).
        assert_close(
            Density::from_pounds_per_cubic_foot(1.0).kilograms_per_cubic_meter(),
            16.018_463,
            1e-7,
        );
        let ice = Density::from_kilograms_per_cubic_meter(913.0);
        let w = ice.weight_per_length(Area::from_square_millimeters(1000.0));
        assert_close(w.newtons_per_meter(), 0.913 * STANDARD_GRAVITY, 1e-12);
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

    #[test]
    fn parse_with_and_without_space() {
        assert_close("1000 ft".parse::<Length>().unwrap().meters(), 304.8, 1e-15);
        assert_close("1000ft".parse::<Length>().unwrap().meters(), 304.8, 1e-15);
        assert_close(
            "  -2.5e3 N ".parse::<Force>().unwrap().kilonewtons(),
            -2.5,
            1e-15,
        );
        assert_close(
            "1.2E+1kN".parse::<Force>().unwrap().kilonewtons(),
            12.0,
            1e-15,
        );
        assert_close(
            "15.5 cm2".parse::<Area>().unwrap().square_millimeters(),
            1550.0,
            1e-9,
        );
        assert_close(
            "795 kcmil".parse::<Area>().unwrap().square_millimeters(),
            402.832_45,
            1e-6,
        );
        assert_close(
            "1.09 lbf/ft"
                .parse::<ForcePerLength>()
                .unwrap()
                .newtons_per_meter(),
            15.907_354,
            1e-6,
        );
        assert_close(
            "-20 °C".parse::<Temperature>().unwrap().kelvin(),
            253.15,
            1e-12,
        );
        assert_close(
            "32 degF".parse::<Temperature>().unwrap().celsius(),
            0.0,
            1e-12,
        );
        assert_close(
            "90 deg".parse::<Angle>().unwrap().radians(),
            std::f64::consts::FRAC_PI_2,
            1e-15,
        );
    }

    #[test]
    fn parse_errors() {
        assert_eq!(
            "12.5".parse::<Length>(),
            Err(ParseQuantityError::MissingUnit)
        );
        assert_eq!(
            "12.5 kN".parse::<Length>(),
            Err(ParseQuantityError::UnknownUnit {
                unit: "kN".into(),
                quantity: "length"
            })
        );
        assert!(matches!(
            "1.2.3 m".parse::<Length>(),
            Err(ParseQuantityError::InvalidNumber(_))
        ));
        assert_eq!(
            " ft".parse::<Length>(),
            Err(ParseQuantityError::InvalidNumber(String::new()))
        );
    }

    #[test]
    fn display_in_unit_system() {
        let t = Force::from_kilonewtons(25.0);
        assert_eq!(
            format!("{:.2}", t.display_in(UnitSystem::Metric)),
            "25.00 kN"
        );
        assert_eq!(
            format!("{:.0}", t.display_in(UnitSystem::UsCustomary)),
            "5620 lbf"
        );
        let temp = Temperature::from_celsius(75.0);
        assert_eq!(
            format!("{:.0}", temp.display_in(UnitSystem::UsCustomary)),
            "167 °F"
        );
    }

    #[test]
    fn every_display_unit_is_known() {
        fn check<Q: Quantity>() {
            for system in [UnitSystem::Metric, UnitSystem::UsCustomary] {
                assert!(
                    Q::from_value(1.0, Q::display_unit(system)).is_some(),
                    "{} / {system:?}",
                    Q::KIND
                );
            }
        }
        check::<Length>();
        check::<Area>();
        check::<Force>();
        check::<ForcePerLength>();
        check::<Stress>();
        check::<MassPerLength>();
        check::<ResistancePerLength>();
        check::<Density>();
        check::<Angle>();
        check::<Speed>();
        check::<Temperature>();
        check::<SecondMoment>();
        check::<SectionModulus>();
    }

    #[test]
    fn section_quantities() {
        assert_close(
            "116 cm4".parse::<SecondMoment>().unwrap().si(),
            1.16e-6,
            1e-12,
        );
        assert_close(
            "2e6 mm⁴".parse::<SecondMoment>().unwrap().cm4(),
            200.0,
            1e-12,
        );
        assert_close(
            "18 cm³".parse::<SectionModulus>().unwrap().mm3(),
            18e3,
            1e-12,
        );
        assert_close(SectionModulus::from_in3(1.0).cm3(), 2.54f64.powi(3), 1e-12);
        assert!("3 kg".parse::<SectionModulus>().is_err());
    }

    #[test]
    fn convert_by_kind_name() {
        assert_close(
            convert("force", "1 kip", "kN").unwrap(),
            4.448_221_615_260_5,
            1e-15,
        );
        assert_close(convert("temperature", "0 °C", "°F").unwrap(), 32.0, 1e-12);
        assert_eq!(
            convert("volume", "1 m", "m"),
            Err(ParseQuantityError::UnknownQuantity("volume".into()))
        );
        assert!(matches!(
            convert("length", "1 m", "kN"),
            Err(ParseQuantityError::UnknownUnit { .. })
        ));
        for kind in QUANTITY_KINDS {
            assert!(!matches!(
                convert(kind, "1 x", "x"),
                Err(ParseQuantityError::UnknownQuantity(_))
            ));
        }
    }

    #[test]
    fn serializes_as_plain_si_number() {
        let span = Length::from_feet(100.0);
        assert_eq!(serde_json::to_string(&span).unwrap(), "30.48");
        let back: Length = serde_json::from_str("30.48").unwrap();
        assert_eq!(back, span);
        assert_eq!(
            serde_json::to_string(&UnitSystem::UsCustomary).unwrap(),
            "\"us_customary\""
        );
    }

    #[test]
    fn deserializes_text_with_unit() {
        let d: Length = serde_json::from_str("\"28.14 mm\"").unwrap();
        assert_close(d.millimeters(), 28.14, 1e-15);
        let t: Temperature = serde_json::from_str("\"-20 °C\"").unwrap();
        assert_close(t.celsius(), -20.0, 1e-12);
        let n: Force = serde_json::from_str("1000").unwrap();
        assert_close(n.newtons(), 1000.0, 1e-15);
        let error = serde_json::from_str::<Length>("\"12\"").unwrap_err();
        assert!(error.to_string().contains("missing unit"), "{error}");
    }
}
