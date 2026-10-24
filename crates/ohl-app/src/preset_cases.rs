// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A design-code preset's weather cases matched to the project's, before
//! the preset is applied (owner, 2026-10-21): **by physics first, then by
//! name**.
//!
//! - A preset case with the same physics as one of the project's
//!   ([`ohl_weather::WeatherCase::same_physics`]: temperature, ice, wind and
//!   the rest a solve reads; never the name) **uses the project's case**:
//!   the preset's rules are rewritten to name it, and it is ticked for the
//!   run. With several of the same physics, the first in the project's
//!   order, and the preview says so.
//! - Only a case with no equivalent is created. One whose name the project
//!   already gives to a case of other values is created as
//!   `"<name> (preset)"` (the project's case is kept), the rules naming it
//!   so.
//!
//! Every place that applies a preset goes through here: the rule editor's
//! Load preset…, the Vertical / Radial clearance windows (plugin presets
//! and the project's preset files), the vegetation panel. Each lists only
//! the presets for its check and takes only its **part** of one ([`Part`],
//! owner, 2026-10-21: presets are per check): the vertical window the
//! vertical rules and the cases they name, the radial window the radial
//! ones, the vegetation panel its `[vegetation]`, `[wind]` and own cases,
//! the rule editor every rule — no case only another check uses is
//! created.

use ohl_clearance::RuleSet;
use ohl_clearance::rules::{PresetCheck, Rule, Target as RuleTarget};
use ohl_io::codes::RulePresetChecked;
use ohl_weather::{WeatherCase, WeatherCases};
use toml_edit::{DocumentMut, Item, Value};

/// What a preset case created under a taken name is called: its name and
/// this (then a number, should that be taken too).
pub const SUFFIX: &str = " (preset)";

/// What becomes of one preset case.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// The project's case of the same physics is used: its name, and the
    /// project's other cases of the same physics (not used: the first in
    /// the project's order is).
    Uses {
        /// The project's case used.
        name: String,
        /// The project's other cases of the same physics.
        also: Vec<String>,
    },
    /// The case is created under `name`; `clash`: the project has a case
    /// of the preset's name with other values (kept as the project's).
    Creates {
        /// The name it is created under.
        name: String,
        /// Renamed for a project case of its name with other values.
        clash: bool,
    },
}

impl Target {
    /// The project's name of the case after the preset is applied.
    pub fn name(&self) -> &str {
        match self {
            Target::Uses { name, .. } | Target::Creates { name, .. } => name,
        }
    }
}

/// One preset case and what becomes of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Matched {
    /// The preset's name of the case.
    pub preset: String,
    /// What becomes of it.
    pub target: Target,
}

/// The preset's weather cases matched to the project's, in the preset's
/// order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// Each preset case.
    pub cases: Vec<Matched>,
}

impl Plan {
    /// Matches the cases of `preset` to `project` (the project's weather
    /// cases, in its order): the first project case of the same physics,
    /// else created (renamed with [`SUFFIX`] when its name is taken).
    pub fn new(project: &[WeatherCase], preset: &WeatherCases) -> Self {
        let mut taken: Vec<String> = project
            .iter()
            .chain(&preset.cases)
            .map(|c| c.name.clone())
            .collect();
        let cases = preset
            .cases
            .iter()
            .map(|c| {
                let mut same = project.iter().filter(|x| x.same_physics(c));
                let target = match same.next() {
                    Some(first) => Target::Uses {
                        name: first.name.clone(),
                        also: same.map(|x| x.name.clone()).collect(),
                    },
                    None => {
                        let clash = project.iter().any(|x| x.name == c.name);
                        let name = if clash {
                            let mut name = format!("{}{SUFFIX}", c.name);
                            let mut k = 2;
                            while taken.contains(&name) {
                                name = format!("{} (preset {k})", c.name);
                                k += 1;
                            }
                            taken.push(name.clone());
                            name
                        } else {
                            c.name.clone()
                        };
                        Target::Creates { name, clash }
                    }
                };
                Matched {
                    preset: c.name.clone(),
                    target,
                }
            })
            .collect();
        Self { cases }
    }

    /// The project's names of the preset's cases after it is applied, in
    /// the preset's order, each once: the cases to tick for a run.
    pub fn names(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for m in &self.cases {
            let n = m.target.name();
            if !out.iter().any(|x| x == n) {
                out.push(n.to_owned());
            }
        }
        out
    }

    /// Whether applying it changes the preset (a case used or renamed).
    pub fn changes(&self) -> bool {
        self.cases.iter().any(|m| match &m.target {
            Target::Uses { .. } => true,
            Target::Creates { name, .. } => *name != m.preset,
        })
    }

    /// One line per preset case for the previews: "uses your case X" or
    /// "creates Y".
    pub fn lines(&self) -> Vec<String> {
        self.cases
            .iter()
            .map(|m| {
                let p = &m.preset;
                match &m.target {
                    Target::Uses { name, also } => {
                        let mut s = if name == p {
                            format!("weather case \"{p}\": uses your case \"{name}\"")
                        } else {
                            format!(
                                "weather case \"{p}\": uses your case \"{name}\" (the same weather)"
                            )
                        };
                        if !also.is_empty() {
                            let others: Vec<String> =
                                also.iter().map(|a| format!("\"{a}\"")).collect();
                            s = format!(
                                "{s} — {} the same too; the first in your list is used",
                                others.join(", ")
                            );
                        }
                        s
                    }
                    Target::Creates { name, clash: false } => {
                        format!("weather case \"{p}\": creates \"{name}\"")
                    }
                    Target::Creates { name, clash: true } => format!(
                        "weather case \"{p}\": creates \"{name}\" (your \"{p}\" has other \
                         values and is kept)"
                    ),
                }
            })
            .collect()
    }

    /// The cases a rule names (`cases`, names or `*` `?` patterns) in the
    /// project's names: an entry matching a preset case used or renamed
    /// becomes the project's names of every preset case it matches; the
    /// others stay as written. Each name once.
    pub fn entries(&self, entries: &[String]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut push = |s: &str| {
            if !out.iter().any(|x| x == s) {
                out.push(s.to_owned());
            }
        };
        for e in entries {
            let matched: Vec<&Matched> = self
                .cases
                .iter()
                .filter(|m| ohl_core::pattern::matches(e, &m.preset))
                .collect();
            if matched.iter().all(|m| m.target.name() == m.preset) {
                push(e);
            } else {
                for m in matched {
                    push(m.target.name());
                }
            }
        }
        out
    }

    /// A rule's `case_pairs` in the project's names ([`Plan::entries`] on
    /// each side; a pattern standing for several becomes a pair each).
    pub fn pairs(&self, pairs: &[[String; 2]]) -> Vec<[String; 2]> {
        let mut out: Vec<[String; 2]> = Vec::new();
        for [a, b] in pairs {
            for x in self.entries(std::slice::from_ref(a)) {
                for y in self.entries(std::slice::from_ref(b)) {
                    let pair = [x.clone(), y];
                    if !out.contains(&pair) {
                        out.push(pair);
                    }
                }
            }
        }
        out
    }

    /// The rules of `set` naming the project's cases.
    pub fn rename_rule_set(&self, set: &mut RuleSet) {
        for r in &mut set.rules {
            r.cases = self.entries(&r.cases);
            r.case_pairs = self.pairs(&r.case_pairs);
        }
        for r in &mut set.tension_rules {
            r.cases = self.entries(&r.cases);
        }
    }

    /// A rule file's text with its rules naming the project's cases; the
    /// rest of the text (comments, order) as written.
    pub fn rules_text(&self, text: &str) -> Result<String, String> {
        let mut doc: DocumentMut = text.parse().map_err(|e| format!("{e}"))?;
        // The clearance rules and (2026-10-22) the tension rules.
        for key in ["rule", "tension_rule"] {
            if let Some(list) = doc.get_mut(key).and_then(Item::as_array_of_tables_mut) {
                self.rename_tables(list);
            }
        }
        Ok(doc.to_string())
    }

    /// [`Plan::rules_text`] of one array of tables.
    fn rename_tables(&self, list: &mut toml_edit::ArrayOfTables) {
        let strings = |v: &Value| -> Option<Vec<String>> {
            v.as_array()?
                .iter()
                .map(|x| x.as_str().map(str::to_owned))
                .collect()
        };
        for t in list.iter_mut() {
            if let Some(v) = t.get("cases").and_then(Item::as_value)
                && let Some(old) = strings(v)
            {
                let new = self.entries(&old);
                if new != old {
                    t["cases"] = toml_edit::value(new.into_iter().collect::<toml_edit::Array>());
                }
            }
            if let Some(v) = t.get("case_pairs").and_then(Item::as_value)
                && let Some(arr) = v.as_array()
            {
                let old: Option<Vec<[String; 2]>> = arr
                    .iter()
                    .map(|p| {
                        let p = strings(p)?;
                        <[String; 2]>::try_from(p).ok()
                    })
                    .collect();
                if let Some(old) = old {
                    let new = self.pairs(&old);
                    if new != old {
                        let arr: toml_edit::Array = new
                            .into_iter()
                            .map(|p| Value::Array(p.into_iter().collect()))
                            .collect();
                        t["case_pairs"] = toml_edit::value(arr);
                    }
                }
            }
        }
    }

    /// A weather case file's text holding only the cases to create, under
    /// their project names (those used are the project's).
    pub fn weather_text(&self, text: &str) -> Result<String, String> {
        let mut doc: DocumentMut = text.parse().map_err(|e| format!("{e}"))?;
        if let Some(list) = doc.get_mut("case").and_then(Item::as_array_of_tables_mut) {
            list.retain(|t| {
                let name = t.get("name").and_then(Item::as_str);
                self.cases.iter().any(|m| {
                    Some(m.preset.as_str()) == name && matches!(m.target, Target::Creates { .. })
                })
            });
            for t in list.iter_mut() {
                let name = t.get("name").and_then(Item::as_str).map(str::to_owned);
                let to = self
                    .cases
                    .iter()
                    .find(|m| Some(&m.preset) == name.as_ref())
                    .map(|m| m.target.name().to_owned());
                if let (Some(name), Some(to)) = (name, to)
                    && name != to
                {
                    t["name"] = toml_edit::value(to);
                }
            }
        }
        Ok(doc.to_string())
    }

    /// A load case file's text with each case's `weather` naming the
    /// project's case; the rest as written.
    pub fn load_cases_text(&self, text: &str) -> Result<String, String> {
        let mut doc: DocumentMut = text.parse().map_err(|e| format!("{e}"))?;
        if let Some(list) = doc.get_mut("case").and_then(Item::as_array_of_tables_mut) {
            for t in list.iter_mut() {
                let name = t.get("weather").and_then(Item::as_str).map(str::to_owned);
                let to = self
                    .cases
                    .iter()
                    .find(|m| Some(&m.preset) == name.as_ref())
                    .map(|m| m.target.name().to_owned());
                if let (Some(name), Some(to)) = (name, to)
                    && name != to
                {
                    t["weather"] = toml_edit::value(to);
                }
            }
        }
        Ok(doc.to_string())
    }

    /// The project's weather case file `project` with the cases to create
    /// of `preset` (a weather case file's text) added at its end, under
    /// their project names; the project's cases, comments and order kept.
    pub fn add_to_project(&self, project: &str, preset: &str) -> Result<String, String> {
        let created: DocumentMut = self
            .weather_text(preset)?
            .parse()
            .map_err(|e| format!("{e}"))?;
        project
            .parse::<DocumentMut>()
            .map_err(|e| format!("the project's weather case file: {e}"))?;
        let mut out = project.to_owned();
        // Appended as text: the project's file as written, the new cases
        // after its own.
        if let Some(new) = created.get("case").and_then(Item::as_array_of_tables) {
            for t in new.iter() {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                let mut t = t.clone();
                t.decor_mut().clear();
                out.push_str(&format!("\n[[case]]\n{t}"));
            }
        }
        Ok(out)
    }

    /// The preset as applied to the project: its rules naming the
    /// project's cases, its weather cases only those to create (under
    /// their project names).
    pub fn adapt(&self, p: &RulePresetChecked) -> Result<RulePresetChecked, String> {
        if !self.changes() || p.weather.is_none() {
            return Ok(p.clone());
        }
        let rules_text = self.rules_text(&p.rules_text)?;
        let rules = RuleSet::parse(&rules_text)?;
        let weather_text = self.weather_text(&p.weather_text)?;
        let weather = Some(WeatherCases::parse(&weather_text)?);
        Ok(RulePresetChecked {
            name: p.name.clone(),
            description: p.description.clone(),
            rules_text,
            weather_text,
            rules,
            weather,
        })
    }
}

/// What a place takes of a preset (owner, 2026-10-21: presets are per
/// check — applying one brings only what that check needs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// A check's window: the vertical or radial window takes the rules of
    /// its measure and the cases they name; the vegetation panel its
    /// `[vegetation]`, `[wind]` and its own cases.
    Check(PresetCheck),
    /// The rule editor: every rule and the cases they name (no
    /// `[vegetation]`).
    Rules,
}

impl Part {
    /// The part of a clearance window.
    pub fn of(measure: ohl_clearance::Measure) -> Self {
        Part::Check(match measure {
            ohl_clearance::Measure::Vertical => PresetCheck::Vertical,
            ohl_clearance::Measure::Radial => PresetCheck::Radial,
        })
    }

    /// Whether its place lists preset `rules`: the rule editor every one,
    /// a check those for it ([`RuleSet::is_for`]).
    pub fn lists(self, rules: &RuleSet) -> bool {
        match self {
            Part::Rules => true,
            Part::Check(c) => rules.is_for(c),
        }
    }

    /// Whether it takes rule `r`.
    pub fn takes(self, r: &Rule) -> bool {
        match self {
            Part::Rules => true,
            Part::Check(PresetCheck::Vertical) => r.vertical.is_some(),
            Part::Check(PresetCheck::Radial) => r.radial.is_some(),
            Part::Check(PresetCheck::Vegetation | PresetCheck::Tension) => false,
        }
    }

    /// The names of the preset's weather cases it uses: those its rules
    /// name (`cases`, `case_pairs`; a rule naming none needs none of the
    /// preset's); the vegetation check its `[vegetation] cases`, else the
    /// cases no rule names, else (no rules) all.
    pub fn cases(self, rules: &RuleSet, weather: &WeatherCases) -> Vec<String> {
        let matching = |patterns: &[&String]| -> Vec<String> {
            weather
                .cases
                .iter()
                .filter(|c| {
                    patterns
                        .iter()
                        .any(|p| ohl_core::pattern::matches(p, &c.name))
                })
                .map(|c| c.name.clone())
                .collect()
        };
        let named_by = |keep: &dyn Fn(&Rule) -> bool| -> Vec<&String> {
            rules
                .rules
                .iter()
                .filter(|r| keep(r))
                .flat_map(|r| r.cases.iter().chain(r.case_pairs.iter().flatten()))
                .collect()
        };
        match self {
            Part::Check(PresetCheck::Vegetation) => {
                let own: Vec<&String> = rules.vegetation.iter().flat_map(|v| &v.cases).collect();
                if !own.is_empty() {
                    return matching(&own);
                }
                let by_rules = matching(&named_by(&|_| true));
                weather
                    .cases
                    .iter()
                    .filter(|c| !by_rules.contains(&c.name))
                    .map(|c| c.name.clone())
                    .collect()
            }
            // The tension check: the cases its tension rules name.
            Part::Check(PresetCheck::Tension) => {
                let named: Vec<&String> =
                    rules.tension_rules.iter().flat_map(|r| &r.cases).collect();
                matching(&named)
            }
            _ => matching(&named_by(&|r| self.takes(r))),
        }
    }

    /// Preset `p` with only this part: its rules (and `[vegetation]` for
    /// the vegetation check, `[wind]` / `[check]` as given) and the
    /// weather cases they use, the texts edited in place.
    pub fn of_preset(self, p: &RulePresetChecked) -> Result<RulePresetChecked, String> {
        let mut doc: DocumentMut = p.rules_text.parse().map_err(|e| format!("{e}"))?;
        doc.remove("checks");
        if self != Part::Rules && self != Part::Check(PresetCheck::Tension) {
            // The tension rules are the tension check's only.
            doc.remove("tension_rule");
        }
        if self == Part::Check(PresetCheck::Tension) {
            // Its tension rules only (2026-10-22).
            for key in ["rule", "check", "wind", "vegetation"] {
                doc.remove(key);
            }
        } else if self == Part::Check(PresetCheck::Vegetation) {
            doc.remove("rule");
            doc.remove("check");
        } else {
            doc.remove("vegetation");
            // The rules kept, read as the preset's (same order).
            let keep: Vec<bool> = p.rules.rules.iter().map(|r| self.takes(r)).collect();
            if let Some(list) = doc.get_mut("rule").and_then(Item::as_array_of_tables_mut) {
                let mut k = 0;
                list.retain(|_| {
                    k += 1;
                    keep.get(k - 1).copied().unwrap_or(false)
                });
                if list.is_empty() {
                    doc.remove("rule");
                }
            }
        }
        let rules_text = doc.to_string();
        let rules = RuleSet::parse(&rules_text)?;
        let (weather_text, weather) = match &p.weather {
            Some(w) => {
                let names = self.cases(&p.rules, w);
                let text = keep_cases_text(&p.weather_text, &names)?;
                let set = WeatherCases::parse(&text)?;
                (text, Some(set))
            }
            None => (p.weather_text.clone(), None),
        };
        Ok(RulePresetChecked {
            name: p.name.clone(),
            description: p.description.clone(),
            rules_text,
            weather_text,
            rules,
            weather,
        })
    }
}

/// A weather case file's text with only the cases named `names` (the
/// rest of the text as written).
pub fn keep_cases_text(text: &str, names: &[String]) -> Result<String, String> {
    let mut doc: DocumentMut = text.parse().map_err(|e| format!("{e}"))?;
    if let Some(list) = doc.get_mut("case").and_then(Item::as_array_of_tables_mut) {
        list.retain(|t| {
            t.get("name")
                .and_then(Item::as_str)
                .is_some_and(|n| names.iter().any(|x| x == n))
        });
    }
    Ok(doc.to_string())
}

/// The target of a rule in words, for grouping a preview: "ground",
/// "LiDAR classes 11, 12", "the line's own wires", "other lines".
pub fn target_label(t: &RuleTarget) -> String {
    match t {
        RuleTarget::Ground => "ground".into(),
        RuleTarget::Points { classes, roles } => {
            let mut parts: Vec<String> = Vec::new();
            if !classes.is_empty() {
                let c: Vec<String> = classes.iter().map(u8::to_string).collect();
                parts.push(format!("LiDAR classes {}", c.join(", ")));
            }
            if !roles.is_empty() {
                parts.push(format!("roles {}", roles.join(", ")));
            }
            parts.join(", ")
        }
        RuleTarget::SameLine { .. } => "the line's own wires".into(),
        RuleTarget::OtherLines => "other lines".into(),
    }
}

/// The weather cases a rule uses, in words: "cases: a, b", "case pairs: a
/// / b", or "every case" when it names none.
pub fn cases_label(r: &Rule) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !r.cases.is_empty() {
        parts.push(format!("cases: {}", r.cases.join(", ")));
    }
    if !r.case_pairs.is_empty() {
        let p: Vec<String> = r
            .case_pairs
            .iter()
            .map(|[a, b]| format!("{a} / {b}"))
            .collect();
        parts.push(format!("case pairs: {}", p.join(", ")));
    }
    if parts.is_empty() {
        "every case of the project".into()
    } else {
        parts.join("; ")
    }
}

/// The rules grouped by target (in the order each first appears).
pub fn grouped(rules: &[Rule]) -> Vec<(String, Vec<&Rule>)> {
    let mut out: Vec<(String, Vec<&Rule>)> = Vec::new();
    for r in rules {
        let label = target_label(&r.target);
        match out.iter_mut().find(|(l, _)| *l == label) {
            Some((_, list)) => list.push(r),
            None => out.push((label, vec![r])),
        }
    }
    out
}

/// The project's weather cases a preset is matched to: those of its
/// weather case file, in its order (none without one: the still-air case
/// the model then solves is not the project's).
pub fn project_cases(model: &crate::model::Model) -> Vec<WeatherCase> {
    if model.sources.weather.is_none() {
        return Vec::new();
    }
    model.cases.iter().map(|c| c.case.clone()).collect()
}

/// The `part` of preset `p` ([`Part::of_preset`]) matched to the
/// project's weather cases and adapted ([`Plan::adapt`]); without weather
/// cases, the part as it is.
pub fn for_project(
    model: &crate::model::Model,
    p: &RulePresetChecked,
    part: Part,
) -> Result<(RulePresetChecked, Plan), String> {
    let p = part.of_preset(p)?;
    let Some(wc) = &p.weather else {
        return Ok((p, Plan::default()));
    };
    let plan = Plan::new(&project_cases(model), wc);
    Ok((plan.adapt(&p)?, plan))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use ohl_core::units::{Density, ForcePerLength, Length, Temperature};

    const PROJECT: &str = r#"format = "ohl-weather-cases"
version = 1
[[case]]
name = "80 °C"
temperature = "80 °C"
[[case]]
name = "max temperature"
temperature = "80 °C"
[[case]]
name = "ice"
temperature = "-5 °C"
ice_thickness = "10 mm"
[[case]]
name = "cold"
temperature = "-20 °C"
"#;

    const PRESET: &str = r#"format = "ohl-weather-cases"
version = 1
[[case]]
name = "Utility vegetation 80 °C"
description = "hot"
temperature = "80 °C"
# The cold case (a comment kept).
[[case]]
name = "cold"
temperature = "-10 °C"
[[case]]
name = "ice by load"
temperature = "-5 °C"
ice_load = "10 N/m"
"#;

    const RULES: &str = r#"format = "ohl-clearance-rules"
version = 1
name = "p"

# Ground (a comment kept).
[[rule]]
name = "g"
target = "ground"
vertical = "7 m"
cases = ["Utility vegetation 80 °C", "cold"]

[[rule]]
name = "w"
target = "same_line"
radial = "3 m"
cases = ["ice*"]
case_pairs = [["ice by load", "Utility*"]]
"#;

    fn plan() -> Plan {
        let project = WeatherCases::parse(PROJECT).unwrap().cases;
        Plan::new(&project, &WeatherCases::parse(PRESET).unwrap())
    }

    /// The same physics uses the project's first such case (the second
    /// said); a same-named case of other values gives the suffix; ice as a
    /// load is never the project's thickness.
    #[test]
    fn cases_match_by_physics_then_name() {
        let p = plan();
        assert_eq!(
            p.cases[0].target,
            Target::Uses {
                name: "80 °C".into(),
                also: vec!["max temperature".into()]
            }
        );
        assert_eq!(
            p.cases[1].target,
            Target::Creates {
                name: "cold (preset)".into(),
                clash: true
            }
        );
        assert_eq!(
            p.cases[2].target,
            Target::Creates {
                name: "ice by load".into(),
                clash: false
            }
        );
        assert_eq!(p.names(), ["80 °C", "cold (preset)", "ice by load"]);
        let lines = p.lines();
        assert!(
            lines[0].contains("uses your case \"80 °C\"")
                && lines[0].contains("\"max temperature\" the same too"),
            "{lines:?}"
        );
        assert!(lines[1].contains("creates \"cold (preset)\""), "{lines:?}");
        assert!(lines[2].contains("creates \"ice by load\""), "{lines:?}");
        // A suffix already taken: numbered.
        let mut project = WeatherCases::parse(PROJECT).unwrap().cases;
        let mut taken = project[3].clone();
        taken.name = "cold (preset)".into();
        taken.temperature = Temperature::from_celsius(-30.0);
        project.push(taken);
        let p = Plan::new(&project, &WeatherCases::parse(PRESET).unwrap());
        assert_eq!(p.cases[1].target.name(), "cold (preset 2)");
    }

    /// Ice as a thickness in the project, the preset's as the load of that
    /// thickness on some wire: created, not used (the load of a thickness
    /// depends on the wire); the same load and density: used.
    #[test]
    fn ice_in_mm_is_not_ice_in_newtons_per_metre() {
        let project = WeatherCases::parse(PROJECT).unwrap().cases;
        let d = Length::from_millimeters(28.0);
        let load = project[2].wire_load(d, ForcePerLength::ZERO).vertical;
        let mut as_load = WeatherCase::still("ice (N/m)", project[2].temperature);
        as_load.ice_load = Some(load);
        let set = WeatherCases {
            format: None,
            version: None,
            name: String::new(),
            cases: vec![as_load.clone()],
        };
        let p = Plan::new(&project, &set);
        assert!(matches!(p.cases[0].target, Target::Creates { .. }));
        // The project with that load (glaze ice given): used.
        let mut with_load = as_load.clone();
        with_load.name = "ice load".into();
        with_load.ice_density = Some(Density::from_kilograms_per_cubic_meter(913.0));
        let p = Plan::new(&[with_load], &set);
        assert_eq!(p.cases[0].target.name(), "ice load");
    }

    /// The rules name the project's cases (a pattern matching a renamed
    /// case becomes the names), comments kept; the weather cases hold only
    /// those created, under their names.
    #[test]
    fn rules_and_cases_are_rewritten() {
        let p = plan();
        let text = p.rules_text(RULES).unwrap();
        assert!(text.contains("# Ground (a comment kept)."), "{text}");
        let set = RuleSet::parse(&text).unwrap();
        assert_eq!(set.rules[0].cases, ["80 °C", "cold (preset)"]);
        // "ice*" matches only "ice by load", not renamed: kept.
        assert_eq!(set.rules[1].cases, ["ice*"]);
        assert_eq!(
            set.rules[1].case_pairs,
            [["ice by load".to_string(), "80 °C".to_string()]]
        );
        let mut by_struct = RuleSet::parse(RULES).unwrap();
        p.rename_rule_set(&mut by_struct);
        assert_eq!(by_struct.rules[0].cases, set.rules[0].cases);
        assert_eq!(by_struct.rules[1].case_pairs, set.rules[1].case_pairs);
        let w = p.weather_text(PRESET).unwrap();
        assert!(w.contains("# The cold case (a comment kept)."), "{w}");
        let names: Vec<String> = WeatherCases::parse(&w)
            .unwrap()
            .cases
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names, ["cold (preset)", "ice by load"]);
    }

    /// Nothing the project has: the preset as it is.
    /// A design code's cases into the project's file: only those to
    /// create added after the project's (its text as written); the load
    /// cases naming the project's case of the same physics.
    #[test]
    fn code_cases_join_the_project_and_load_cases_follow() {
        let p = plan();
        let merged = p.add_to_project(PROJECT, PRESET).unwrap();
        assert!(merged.starts_with(PROJECT), "{merged}");
        let names: Vec<String> = WeatherCases::parse(&merged)
            .unwrap()
            .cases
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(
            names,
            [
                "80 °C",
                "max temperature",
                "ice",
                "cold",
                "cold (preset)",
                "ice by load"
            ]
        );
        let loads = "format = \"ohl-load-cases\"\nversion = 1\n# kept\n\
                     [[case]]\nname = \"hot\"\nweather = \"Utility vegetation 80 °C\"\n\
                     [[case]]\nname = \"c\"\nweather = \"cold\"\n";
        let t = p.load_cases_text(loads).unwrap();
        assert!(t.contains("# kept"), "{t}");
        let w: Vec<String> = ohl_loads::LoadCases::parse(&t)
            .unwrap()
            .cases
            .into_iter()
            .map(|c| c.weather)
            .collect();
        assert_eq!(w, ["80 °C", "cold (preset)"]);
    }

    #[test]
    fn nothing_to_match_changes_nothing() {
        let set = WeatherCases::parse(PRESET).unwrap();
        let p = Plan::new(&[], &set);
        assert!(!p.changes());
        assert_eq!(p.rules_text(RULES).unwrap(), RULES);
        assert_eq!(p.entries(&["Utility*".into()]), ["Utility*"]);
    }

    /// One big combined preset (invented values, as a plugin may give
    /// one for many checks in a row): 40 rules — vertical rules to the
    /// ground and LiDAR classes in "V hot" / "V ice", radial rules between
    /// wires and to other lines in "R wind" / "R ice" (pairs), some with
    /// both distances in "B cold" — and a `[vegetation]` table with its
    /// own "Veg hot" / "Veg wind".
    pub(crate) fn big_preset(checks: &str) -> RulePresetChecked {
        use std::fmt::Write;
        let mut rules = format!(
            "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"big\"\n{checks}\
             [wind]\nsteps = true\nstep = \"100 Pa\"\n\
             [vegetation]\nclasses = [5]\ncases = [\"Veg *\"]\n"
        );
        for k in 0..40 {
            let _ = match k % 5 {
                0 => write!(
                    rules,
                    "[[rule]]\nname = \"ground {k}\"\ntarget = \"ground\"\n\
                     vertical = \"{} m\"\ncases = [\"V hot\"]\n",
                    6 + k % 3
                ),
                1 => write!(
                    rules,
                    "[[rule]]\nname = \"roads {k}\"\ntarget = \"points\"\nclasses = [{}]\n\
                     vertical = {{ by_voltage = [[110, \"6 m\"], [380, \"8 m\"]] }}\n\
                     cases = [\"V hot\", \"V ice\"]\n",
                    10 + k % 4
                ),
                2 => write!(
                    rules,
                    "[[rule]]\nname = \"wires {k}\"\ntarget = \"same_line\"\nradial = \"3 m\"\n\
                     cases = [\"R wind\"]\n"
                ),
                3 => write!(
                    rules,
                    "[[rule]]\nname = \"crossing {k}\"\ntarget = \"other_lines\"\n\
                     radial = {{ base = \"2 m\", per_kv = \"0.01 m\", above_kv = 0 }}\n\
                     case_pairs = [[\"R ice\", \"R wind\"]]\n"
                ),
                _ => write!(
                    rules,
                    "[[rule]]\nname = \"houses {k}\"\ntarget = \"points\"\nclasses = [6]\n\
                     vertical = \"5 m\"\nradial = \"4 m\"\ncases = [\"B cold\"]\n"
                ),
            };
        }
        let mut weather = String::from("format = \"ohl-weather-cases\"\nversion = 1\n");
        for (name, t, extra) in [
            ("V hot", 80, ""),
            ("V ice", 0, "ice_load = \"20 N/m\"\n"),
            ("R wind", 10, "wind_pressure = \"650 Pa\"\n"),
            ("R ice", -5, "ice_thickness = \"10 mm\"\n"),
            ("B cold", -20, ""),
            ("Veg hot", 80, ""),
            ("Veg wind", 60, "wind_pressure = \"200 Pa\"\n"),
        ] {
            let _ = write!(
                weather,
                "[[case]]\nname = \"{name}\"\ntemperature = \"{t} °C\"\n{extra}"
            );
        }
        ohl_io::codes::check_rule_presets(
            "code-big",
            vec![ohl_plugins::RulePreset {
                name: "big".into(),
                description: "invented test values".into(),
                rules,
                weather_cases: weather,
            }],
        )
        .unwrap()
        .remove(0)
    }

    fn case_names(p: &RulePresetChecked) -> Vec<String> {
        p.weather
            .as_ref()
            .map(|w| w.cases.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default()
    }

    /// The wire tension check (2026-10-22) takes only the tension rules of
    /// a preset (invented factors) and the weather cases they name; the
    /// clearance windows leave them out; the rule editor keeps them; a
    /// preset without tension rules is not listed for it.
    #[test]
    fn the_tension_check_takes_only_the_tension_rules() {
        let rules = "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"mixed\"\n\
            checks = [\"vertical\", \"tension\"]\n\
            [[rule]]\nname = \"ground\"\ntarget = \"ground\"\nvertical = \"7 m\"\ncases = [\"V hot\"]\n\
            [[tension_rule]]\nname = \"iced, invented 0.4\"\ncases = [\"T ice\"]\nfactor = 0.4\n\
            [[tension_rule]]\nname = \"everyday H, invented 0.2\"\ncases = [\"T ice\"]\nfactor = 0.2\n\
            measure = \"horizontal\"\n"
            .to_owned();
        let weather = "format = \"ohl-weather-cases\"\nversion = 1\n\
            [[case]]\nname = \"V hot\"\ntemperature = \"80 °C\"\n\
            [[case]]\nname = \"T ice\"\ntemperature = \"-5 °C\"\nice_thickness = \"10 mm\"\n"
            .to_owned();
        let p = ohl_io::codes::check_rule_presets(
            "code-t",
            vec![ohl_plugins::RulePreset {
                name: "mixed".into(),
                description: "invented test values".into(),
                rules,
                weather_cases: weather,
            }],
        )
        .unwrap()
        .remove(0);
        let tension = Part::Check(PresetCheck::Tension);
        assert!(tension.lists(&p.rules));
        let t = tension.of_preset(&p).unwrap();
        assert!(t.rules.rules.is_empty());
        assert_eq!(t.rules.tension_rules.len(), 2);
        assert_eq!(t.rules.tension_rules[0].factor, 0.4);
        assert_eq!(case_names(&t), ["T ice"]);
        let v = Part::Check(PresetCheck::Vertical).of_preset(&p).unwrap();
        assert_eq!(v.rules.rules.len(), 1);
        assert!(v.rules.tension_rules.is_empty());
        assert_eq!(case_names(&v), ["V hot"]);
        let all = Part::Rules.of_preset(&p).unwrap();
        assert_eq!(all.rules.tension_rules.len(), 2);
        assert!(!tension.lists(&big_preset("").rules));
    }

    /// Each place takes only its part of a big combined preset: the
    /// vertical window its vertical rules and their cases, the radial
    /// window its radial ones (pairs included), the vegetation panel no
    /// rule and its own cases, the rule editor every rule and the cases
    /// they name.
    #[test]
    fn each_check_takes_only_its_part_of_a_big_preset() {
        let p = big_preset("");
        assert_eq!(p.rules.rules.len(), 40);
        let vertical = Part::Check(PresetCheck::Vertical).of_preset(&p).unwrap();
        assert_eq!(vertical.rules.rules.len(), 24);
        assert!(vertical.rules.rules.iter().all(|r| r.vertical.is_some()));
        assert!(vertical.rules.vegetation.is_none());
        assert!(vertical.rules.wind.is_some());
        assert_eq!(case_names(&vertical), ["V hot", "V ice", "B cold"]);
        let radial = Part::Check(PresetCheck::Radial).of_preset(&p).unwrap();
        assert_eq!(radial.rules.rules.len(), 24);
        assert!(radial.rules.rules.iter().all(|r| r.radial.is_some()));
        assert_eq!(case_names(&radial), ["R wind", "R ice", "B cold"]);
        let veg = Part::Check(PresetCheck::Vegetation).of_preset(&p).unwrap();
        assert!(veg.rules.rules.is_empty());
        assert_eq!(veg.rules.vegetation.as_ref().unwrap().classes, [5]);
        assert!(veg.rules.wind.is_some());
        assert_eq!(case_names(&veg), ["Veg hot", "Veg wind"]);
        let all = Part::Rules.of_preset(&p).unwrap();
        assert_eq!(all.rules.rules.len(), 40);
        assert!(all.rules.vegetation.is_none());
        assert_eq!(
            case_names(&all),
            ["V hot", "V ice", "R wind", "R ice", "B cold"]
        );
        // The previews group the rules by target.
        let groups: Vec<String> = grouped(&vertical.rules.rules)
            .into_iter()
            .map(|(g, l)| format!("{g}: {}", l.len()))
            .collect();
        assert_eq!(
            groups,
            [
                "ground: 8",
                "LiDAR classes 11: 2",
                "LiDAR classes 6: 8",
                "LiDAR classes 12: 2",
                "LiDAR classes 13: 2",
                "LiDAR classes 10: 2"
            ]
        );
        assert_eq!(
            cases_label(&radial.rules.rules[1]),
            "case pairs: R ice / R wind"
        );
        // Without a vegetation case list: the cases no rule names.
        let mut q = p.clone();
        q.rules.vegetation.as_mut().unwrap().cases.clear();
        let veg = Part::Check(PresetCheck::Vegetation).of_preset(&q).unwrap();
        assert_eq!(case_names(&veg), ["Veg hot", "Veg wind"]);
        // Matched to a project with an 80 °C case: the vegetation part's
        // "Veg hot" uses it, "Veg wind" is created; no clearance case.
        let project = WeatherCases::parse(PROJECT).unwrap().cases;
        let plan = Plan::new(&project, veg.weather.as_ref().unwrap());
        assert_eq!(plan.names(), ["80 °C", "Veg wind"]);
    }

    /// The lists per place: the rule editor every preset; a window those
    /// for its check, by content or as `checks` declare.
    #[test]
    fn the_lists_are_filtered_per_window() {
        let p = big_preset("");
        for part in [
            Part::Rules,
            Part::Check(PresetCheck::Vertical),
            Part::Check(PresetCheck::Radial),
            Part::Check(PresetCheck::Vegetation),
        ] {
            assert!(part.lists(&p.rules), "{part:?}");
        }
        let veg_only = big_preset("checks = [\"vegetation\"]\n");
        assert!(Part::Rules.lists(&veg_only.rules));
        assert!(Part::Check(PresetCheck::Vegetation).lists(&veg_only.rules));
        assert!(!Part::Check(PresetCheck::Vertical).lists(&veg_only.rules));
        assert!(!Part::Check(PresetCheck::Radial).lists(&veg_only.rules));
        let ground =
            RuleSet::parse("[[rule]]\nname = \"g\"\ntarget = \"ground\"\nvertical = \"7 m\"\n")
                .unwrap();
        assert!(Part::Check(PresetCheck::Vertical).lists(&ground));
        assert!(!Part::Check(PresetCheck::Radial).lists(&ground));
        assert!(!Part::Check(PresetCheck::Vegetation).lists(&ground));
    }
}
