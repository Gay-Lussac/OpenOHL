// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The bending moment (N·m), which `ohl_core::units` does not have (second
//! moments and section moduli are its `SecondMoment`, `SectionModulus`).
//! Read like the other quantities: a plain number in SI, or text with a
//! unit.

use std::fmt;

use serde::Deserialize;

/// Units of one kind: symbol and factor to SI.
type Units = &'static [(&'static str, f64)];

/// Pound-force, N.
const LBF: f64 = 4.448_221_615_260_5;

const MOMENT: Units = &[
    ("N m", 1.0),
    ("N·m", 1.0),
    ("Nm", 1.0),
    ("kN m", 1e3),
    ("kN·m", 1e3),
    ("kNm", 1e3),
    ("MN m", 1e6),
    ("MN·m", 1e6),
    ("lbf ft", LBF * 0.3048),
    ("lbf·ft", LBF * 0.3048),
    ("kip ft", LBF * 304.8),
    ("kip·ft", LBF * 304.8),
];

/// Parses `"12.5 cm4"` with the units of a kind.
fn parse(text: &str, units: Units, kind: &str) -> Result<f64, String> {
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
    let value: f64 = number
        .parse()
        .map_err(|_| format!("{kind} \"{text}\": not a number"))?;
    // "kN m", "kN  m" and "kN*m" are the same unit.
    let unit = unit.split_whitespace().collect::<Vec<_>>().join(" ");
    let unit = unit.replace('*', "·");
    units
        .iter()
        .find(|(s, _)| *s == unit)
        .map(|(_, f)| value * f)
        .ok_or_else(|| {
            format!(
                "{kind} \"{text}\": unknown unit (use one of {})",
                units
                    .iter()
                    .map(|u| u.0)
                    .filter(|u| u.is_ascii())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

struct Visitor {
    units: Units,
    kind: &'static str,
}

impl serde::de::Visitor<'_> for Visitor {
    type Value = f64;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a {} as an SI number or as text with a unit", self.kind)
    }

    fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<f64, E> {
        Ok(v)
    }

    fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<f64, E> {
        Ok(v as f64)
    }

    fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<f64, E> {
        Ok(v as f64)
    }

    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<f64, E> {
        parse(v, self.units, self.kind).map_err(E::custom)
    }
}

macro_rules! kind {
    ($(#[$doc:meta])* $name:ident, $units:expr, $kind:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Default)]
        pub(crate) struct $name(pub f64);

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                d.deserialize_any(Visitor {
                    units: $units,
                    kind: $kind,
                })
                .map($name)
            }
        }
    };
}

kind!(
    /// Bending moment, N·m.
    Moment,
    MOMENT,
    "moment"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_are_read() {
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-12 * b.abs();
        assert!(close(parse("120 kN m", MOMENT, "x").unwrap(), 1.2e5));
        assert!(close(parse("120 kN·m", MOMENT, "x").unwrap(), 1.2e5));
        assert!(close(parse("120 kN*m", MOMENT, "x").unwrap(), 1.2e5));
        assert!(close(parse("1.5e3 N m", MOMENT, "x").unwrap(), 1.5e3));
        assert!(parse("3 kg", MOMENT, "x").is_err());
        assert!(parse("abc kN m", MOMENT, "x").is_err());
    }
}
