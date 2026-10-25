// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Plan sets** (owner, 2026-10-24; roadmap O14): the drawings to make in
//! one go — the plan-and-profile sheets, the route plan, the situation
//! plans, the phase plan — each with its sheet template, paper and scales.
//! A plan set file (`.ohlset`, TOML) is the user's or a plugin's (its
//! manifest's `[[plan-set]]`, beside its sheet templates):
//!
//! ```toml
//! format = "ohl-plan-set"
//! version = 1
//! name = "Plan submission"
//! description = "The sheets, the situation plans and the phase plan"
//!
//! [[drawing]]
//! kind = "sheets"            # sheets, route, situation, phase
//! template = "esti-235-a1"   # a template's name (the plugin's own first)
//! page = "A1"
//! scale = 2000               # sheets: horizontal; plans: their scale
//! vertical_scale = 200       # sheets only
//!
//! [[drawing]]
//! kind = "situation"
//! page = "A3"
//! scale = 1000
//! ```

use serde::Deserialize;

/// The format name of a plan set file.
pub const PLAN_SET_FORMAT: &str = "ohl-plan-set";

/// The version written.
pub const PLAN_SET_VERSION: u32 = 1;

/// The file extension of plan sets.
pub const PLAN_SET_EXTENSION: &str = "ohlset";

/// What a drawing of a set is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetKind {
    /// The plan-and-profile sheets.
    Sheets,
    /// The route plan.
    Route,
    /// The situation plans.
    Situation,
    /// The phase plan.
    Phase,
}

impl SetKind {
    /// From its name.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "sheets" => Ok(Self::Sheets),
            "route" => Ok(Self::Route),
            "situation" => Ok(Self::Situation),
            "phase" => Ok(Self::Phase),
            other => Err(format!(
                "kind \"{other}\": sheets, route, situation or phase"
            )),
        }
    }

    /// Its name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sheets => "sheets",
            Self::Route => "route",
            Self::Situation => "situation",
            Self::Phase => "phase",
        }
    }
}

/// One drawing of a set.
#[derive(Debug, Clone, PartialEq)]
pub struct SetDrawing {
    /// What it is.
    pub kind: SetKind,
    /// Its sheet template, by name (none: the one chosen for the
    /// drawings).
    pub template: Option<String>,
    /// Its paper (none: the one chosen; the plans: A3).
    pub page: Option<String>,
    /// Its scale (1:…): the sheets' horizontal, the route and situation
    /// plans' own (none: the one chosen, or the plan's usual).
    pub scale: Option<f64>,
    /// The sheets' vertical scale (1:…).
    pub vertical_scale: Option<f64>,
}

/// A plan set.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanSet {
    /// Its name.
    pub name: String,
    /// What it is for.
    pub description: String,
    /// Its drawings, in order.
    pub drawings: Vec<SetDrawing>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSet {
    format: String,
    version: u32,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default, rename = "drawing")]
    drawings: Vec<RawDrawing>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDrawing {
    kind: String,
    #[serde(default)]
    template: Option<String>,
    #[serde(default)]
    page: Option<String>,
    #[serde(default)]
    scale: Option<f64>,
    #[serde(default)]
    vertical_scale: Option<f64>,
}

impl PlanSet {
    /// Reads a plan set file's text, checked: its format and version, at
    /// least one drawing, the kinds, the papers, the scales (the route
    /// plan 1:5000 to 1:25 000, the situation plans 1:500 to 1:2000).
    pub fn parse(text: &str) -> Result<Self, String> {
        let raw: RawSet = toml::from_str(text).map_err(|e| format!("{e}"))?;
        if raw.format != PLAN_SET_FORMAT {
            return Err(format!(
                "format \"{}\": a plan set is \"{PLAN_SET_FORMAT}\"",
                raw.format
            ));
        }
        if raw.version == 0 || raw.version > PLAN_SET_VERSION {
            return Err(format!(
                "version {}: this program reads plan sets up to version {PLAN_SET_VERSION}",
                raw.version
            ));
        }
        if raw.drawings.is_empty() {
            return Err("a plan set needs at least one [[drawing]]".into());
        }
        let mut drawings = Vec::new();
        for (k, d) in raw.drawings.into_iter().enumerate() {
            let at = |e: String| format!("drawing {}: {e}", k + 1);
            let kind = SetKind::parse(&d.kind).map_err(at)?;
            if let Some(p) = &d.page
                && crate::page_format(p).is_none()
            {
                return Err(at(format!("page \"{p}\": a standard paper size")));
            }
            let range = match kind {
                SetKind::Route => Some(5000.0..=25_000.0),
                SetKind::Situation => Some(500.0..=2000.0),
                SetKind::Sheets => Some(10.0..=100_000.0),
                SetKind::Phase => None,
            };
            if let (Some(s), Some(r)) = (d.scale, &range)
                && !r.contains(&s)
            {
                return Err(at(format!(
                    "scale 1:{s}: give 1:{} to 1:{}",
                    r.start(),
                    r.end()
                )));
            }
            if d.vertical_scale.is_some() && kind != SetKind::Sheets {
                return Err(at("vertical_scale: the sheets' only".into()));
            }
            if let Some(v) = d.vertical_scale
                && !(1.0..=100_000.0).contains(&v)
            {
                return Err(at(format!("vertical scale 1:{v}: not a scale")));
            }
            drawings.push(SetDrawing {
                kind,
                template: d.template.filter(|t| !t.trim().is_empty()),
                page: d.page,
                scale: d.scale,
                vertical_scale: d.vertical_scale,
            });
        }
        Ok(Self {
            name: raw.name,
            description: raw.description,
            drawings,
        })
    }
}

/// The file-name suffix of each drawing of `set` (owner, 2026-10-25): the
/// first drawing of a kind none (its files named as when written alone),
/// the second of that kind `-2`, the third `-3` …, so that two drawings
/// of a kind (e.g. situation plans at two scales) do not overwrite each
/// other.
pub fn suffixes(set: &PlanSet) -> Vec<String> {
    set.drawings
        .iter()
        .enumerate()
        .map(|(k, d)| {
            let before = set.drawings[..k]
                .iter()
                .filter(|x| x.kind == d.kind)
                .count();
            if before == 0 {
                String::new()
            } else {
                format!("-{}", before + 1)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plan_set_reads_and_is_checked() {
        let head = "format = \"ohl-plan-set\"\nversion = 1\nname = \"S\"\n";
        let set = PlanSet::parse(&format!(
            "{head}[[drawing]]\nkind = \"sheets\"\ntemplate = \"t\"\npage = \"A1\"\nscale = 2000\n\
             vertical_scale = 200\n[[drawing]]\nkind = \"situation\"\nscale = 1000\n\
             [[drawing]]\nkind = \"phase\"\n"
        ))
        .unwrap();
        assert_eq!(set.drawings.len(), 3);
        assert_eq!(set.drawings[0].kind, SetKind::Sheets);
        assert_eq!(set.drawings[0].template.as_deref(), Some("t"));
        assert_eq!(set.drawings[1].scale, Some(1000.0));
        for bad in [
            "",
            "[[drawing]]\nkind = \"profile\"\n",
            "[[drawing]]\nkind = \"situation\"\nscale = 5000\n",
            "[[drawing]]\nkind = \"route\"\npage = \"A9x\"\n",
            "[[drawing]]\nkind = \"phase\"\nvertical_scale = 200\n",
        ] {
            assert!(PlanSet::parse(&format!("{head}{bad}")).is_err(), "{bad}");
        }
        assert!(PlanSet::parse("format = \"x\"\nversion = 1\nname = \"S\"\n").is_err());
        // Two situation plans: the second named apart.
        let two = PlanSet::parse(&format!(
            "{head}[[drawing]]\nkind = \"situation\"\n[[drawing]]\nkind = \"sheets\"\n\
             [[drawing]]\nkind = \"situation\"\nscale = 500\n"
        ))
        .unwrap();
        assert_eq!(suffixes(&two), ["", "", "-2"]);
    }
}
