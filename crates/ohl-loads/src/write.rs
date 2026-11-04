// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Load case files written (`.ohllc`): at the lowest version their cases
//! need ([`crate::written_version`]), so an older program still reads a
//! file that uses nothing newer.

use std::fmt::Write as _;

use crate::{BrokenSpan, BrokenWire, LoadCase, LoadCases, StructureIce};

/// A string as TOML writes it (quoted, escaped).
fn quoted(s: &str) -> String {
    toml::Value::String(s.to_owned()).to_string()
}

/// A number as TOML reads it back exactly (`1.0`, `0.35`, `900.0`).
fn number(x: f64) -> String {
    format!("{x:?}")
}

fn broken_table(out: &mut String, b: &BrokenWire) {
    let _ = writeln!(out, "section = {}", quoted(&b.section));
    match b.span {
        BrokenSpan::One(n) => {
            let _ = writeln!(out, "span = {n}");
        }
        BrokenSpan::Every => out.push_str("span = \"every\"\n"),
    }
    if let Some(set) = b.set {
        let _ = writeln!(out, "set = {set}");
    }
}

impl LoadCase {
    /// The case as a `[[case]]` table of a file of version `version`
    /// (its broken wire as the version-3 `[case.broken]` table below
    /// version 4).
    pub fn to_toml(&self, version: u32) -> String {
        let mut out = String::from("[[case]]\n");
        let _ = writeln!(out, "name = {}", quoted(&self.name));
        if !self.description.is_empty() {
            let _ = writeln!(out, "description = {}", quoted(&self.description));
        }
        let _ = writeln!(out, "weather = {}", quoted(&self.weather));
        let _ = writeln!(out, "condition = {}", quoted(self.condition.as_str()));
        for (name, f) in [
            ("vertical", self.vertical),
            ("transverse", self.transverse),
            ("longitudinal", self.longitudinal),
            ("strength", self.strength),
        ] {
            let _ = writeln!(out, "{name} = {}", number(f));
        }
        if !self.tags.is_empty() {
            let tags: Vec<String> = self.tags.iter().map(|t| quoted(t)).collect();
            let _ = writeln!(out, "tags = [{}]", tags.join(", "));
        }
        if let Some(side) = self.wind_from {
            let _ = writeln!(out, "wind_from = {}", quoted(side.as_str()));
        }
        if self.use_local {
            out.push_str("use_local = true\n");
        }
        let wind = self.structure.unwrap_or_default();
        let o = &self.on_structure;
        if !wind.is_empty() || !o.is_empty() || self.structure.is_some() {
            out.push_str("\n[case.structure]\n");
            if let Some(p) = wind.wind_pressure {
                let _ = writeln!(out, "wind_pressure = {}   # Pa", number(p));
            }
            if let Some(f) = wind.wind_factor {
                let _ = writeln!(out, "wind_factor = {}", number(f));
            }
            if let Some(d) = o.direction {
                let _ = writeln!(out, "direction = {}", d.text());
            }
            match o.ice {
                Some(StructureIce::Local) => out.push_str("ice = true\n"),
                Some(StructureIce::NoIce) => {
                    out.push_str("ice = false   # none, not even the local ice\n");
                }
                Some(StructureIce::Thickness(t)) => {
                    let _ = writeln!(out, "ice = {}   # m", number(t));
                }
                None => {}
            }
            if let Some(r) = o.ice_density {
                let _ = writeln!(out, "ice_density = {}   # kg/m³", number(r));
            }
            if o.snow {
                out.push_str("snow = true\n");
            }
            for (name, v) in [
                ("snow_shape", o.snow_shape),
                ("snow_exposure", o.snow_exposure),
                ("snow_thermal", o.snow_thermal),
            ] {
                if let Some(v) = v {
                    let _ = writeln!(out, "{name} = {}", number(v));
                }
            }
        }
        for r in &self.removed {
            out.push_str("\n[[case.removed]]\n");
            let _ = writeln!(out, "section = {}", quoted(&r.section));
            if let Some(set) = r.set {
                let _ = writeln!(out, "set = {set}");
            }
        }
        if version < 4 {
            if let Some(b) = &self.broken {
                out.push_str("\n[case.broken]\n");
                broken_table(&mut out, b);
            }
        } else {
            for b in self.broken_wires() {
                out.push_str("\n[[case.broken]]\n");
                broken_table(&mut out, b);
            }
        }
        out
    }
}

impl LoadCases {
    /// The file's text, at [`LoadCases::written_version`] (4 only when a
    /// case uses a version-4 value): read back by [`LoadCases::parse`] as
    /// the same cases.
    pub fn to_toml(&self) -> String {
        let version = self.written_version();
        let mut out = format!(
            "format = {}\nversion = {version}\n",
            quoted(crate::LOAD_CASE_FILE_FORMAT)
        );
        if !self.name.is_empty() {
            let _ = writeln!(out, "name = {}", quoted(&self.name));
        }
        for c in &self.cases {
            out.push('\n');
            out.push_str(&c.to_toml(version));
        }
        out
    }
}
