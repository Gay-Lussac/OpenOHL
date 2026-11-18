// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The method's parameters: the resistance factors γ_M1 and γ_M2 (SIA
//! 263:2013 4.1.3) and the filler metal's tensile strength f_uE (6.3.2.2),
//! from the structure's parameter file for this plugin
//! (`Plugins/sia263.toml` beside the structure, the design-code plugins'
//! mechanism); without a file, or a key not given, the standard's own
//! values — those the core used before the move (2026-11-18), so a
//! structure without a parameter file is checked exactly as before.

use std::cell::Cell;

/// The standard's γ_M1 (4.1.3): members.
pub const GAMMA_M1: f64 = 1.05;

/// The standard's γ_M2 (4.1.3): net sections and connections.
pub const GAMMA_M2: f64 = 1.25;

/// The standard's filler metal strength f_uE (6.3.2.2: "en général
/// 510 N/mm²"), Pa.
pub const FILLER_ULTIMATE: f64 = 510e6;

/// The parameters of one call.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    /// γ_M1.
    pub gamma_m1: f64,
    /// γ_M2.
    pub gamma_m2: f64,
    /// f_uE, Pa.
    pub filler_ultimate: f64,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            gamma_m1: GAMMA_M1,
            gamma_m2: GAMMA_M2,
            filler_ultimate: FILLER_ULTIMATE,
        }
    }
}

thread_local! {
    static NOW: Cell<Params> = Cell::new(Params::default());
}

/// γ_M1 of the call.
pub fn gamma_m1() -> f64 {
    NOW.with(|p| p.get().gamma_m1)
}

/// γ_M2 of the call.
pub fn gamma_m2() -> f64 {
    NOW.with(|p| p.get().gamma_m2)
}

/// f_uE of the call, Pa.
pub fn filler_ultimate() -> f64 {
    NOW.with(|p| p.get().filler_ultimate)
}

/// The parameters of the calls that follow.
pub fn set(p: Params) {
    NOW.with(|c| c.set(p));
}

/// The parameter file as a template: the standard's values, with what
/// each is.
pub const TEMPLATE: &str = "\
# Parameters of the SIA 263:2013 member check (plugin sia263), for the
# structures beside this file's Plugins/ folder: save it there as
# sia263.toml. Every key may be left out: the standard's own value is then
# taken (the values below are the standard's).

[resistance]
# γ_M1, resistance factor of members (4.1.3).
gamma_m1 = 1.05
# γ_M2, resistance factor of net sections and connections (4.1.3).
gamma_m2 = 1.25

[welds]
# f_uE, the filler metal's tensile strength (6.3.2.2: \"en général 510
# N/mm²\"), MPa.
filler_ultimate = 510
";

/// The parameters of a parameter file's text ("" none: the standard's).
/// Err: the file does not read, a key is not a positive number, or a key
/// is not known (a misspelt one would otherwise be ignored silently).
pub fn parse(text: &str) -> Result<Params, String> {
    let mut p = Params::default();
    if text.trim().is_empty() {
        return Ok(p);
    }
    let v: toml::Table = text.parse().map_err(|e| format!("parameter file: {e}"))?;
    for (table, keys) in &v {
        let toml::Value::Table(keys) = keys else {
            return Err(format!("parameter file: {table} is not a table"));
        };
        for (key, value) in keys {
            let x = value
                .as_float()
                .or_else(|| value.as_integer().map(|i| i as f64))
                .filter(|x| x.is_finite() && *x > 0.0)
                .ok_or_else(|| format!("parameter file: {table}.{key} must be a number above 0"))?;
            match (table.as_str(), key.as_str()) {
                ("resistance", "gamma_m1") => p.gamma_m1 = x,
                ("resistance", "gamma_m2") => p.gamma_m2 = x,
                ("welds", "filler_ultimate") => p.filler_ultimate = x * 1e6,
                _ => {
                    return Err(format!(
                        "parameter file: {table}.{key} is not a parameter of this method \
                         (resistance.gamma_m1, resistance.gamma_m2, welds.filler_ultimate)"
                    ));
                }
            }
        }
    }
    Ok(p)
}

/// The parameters in words, for the run's note.
pub fn words(p: &Params) -> String {
    let standard = *p == Params::default();
    format!(
        "design values with γ_M1 {}, γ_M2 {}{}",
        p.gamma_m1,
        p.gamma_m2,
        if standard {
            String::new()
        } else {
            format!(
                ", f_uE {:.0} MPa (from the parameter file, for the members, the joints' \
                 plates and welds and Member details alike; the standard's: γ_M1 {GAMMA_M1}, \
                 γ_M2 {GAMMA_M2}, f_uE {:.0} MPa)",
                p.filler_ultimate / 1e6,
                FILLER_ULTIMATE / 1e6
            )
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_template_gives_the_standards_values() {
        assert_eq!(parse(TEMPLATE), Ok(Params::default()));
        assert_eq!(parse(""), Ok(Params::default()));
        let p = parse("[resistance]\ngamma_m1 = 1.1\n").unwrap();
        assert_eq!(p.gamma_m1, 1.1);
        assert_eq!(p.gamma_m2, GAMMA_M2);
        assert!(parse("[resistance]\ngama_m1 = 1.1\n").is_err());
        assert!(parse("[resistance]\ngamma_m1 = -1\n").is_err());
    }
}
