// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! OpenOHL's **default weather and load cases** in a project (owner,
//! 2026-11-15: "can you have in the software by default a few weather
//! cases … then also have some default load cases for pylons … these
//! values are arbitrary but needed to check the codes"). The sets are
//! shipped in `libraries/` and read by `ohl_weather::default_cases` and
//! `ohl_loads::default_load_cases`; here they are put into a project:
//!
//! - a **new project** starts with them (File › New project…, the tick
//!   [`NEW_PROJECT_TICK`], on by default): the empty project gets the two
//!   files, the starter template's own weather and load case files get the
//!   default cases they lack ([`merge`]);
//! - an existing project adds those it lacks, by name, never replacing a
//!   case of the same name: the Weather cases table's **Add the default
//!   cases…** (new rows, kept by Apply) and the Load cases library's (the
//!   form's cases, kept by Save) — each one change, undone as one.

/// The New project window's tick.
pub const NEW_PROJECT_TICK: &str =
    "Start with the default weather and load cases (arbitrary values for checking)";

/// The tick's hover.
pub const NEW_PROJECT_TIP: &str = "Nine weather cases (−20, 0, 10, 20, 40, 80 °C; 10 °C + 300 Pa \
     wind; 0 °C + 20 N/m and + 60 N/m ice) and eight load cases for the structures (−20, 10, \
     40 °C; the wind from the left, the right, ahead and behind with a factor 1.5 on the wind; \
     the 20 N/m ice with 1.3 on the vertical). Arbitrary values for checking the program and the \
     design codes, not from a design code: replace them with your code's. With the starter \
     template, added to its own cases.";

/// The button of the Weather cases table and of the Load cases library.
pub const ADD_BUTTON: &str = "Add the default cases…";

/// The default cases of the text `defaults` that the case file `text`
/// lacks (by name), appended to it as they are written there (comments
/// and layout of `text` kept); its `version` raised to the defaults' when
/// lower. Returns the new text, the names added and the names already
/// there (not replaced). Both are files of `[[case]]` tables with a
/// `name` (weather cases, load cases).
pub fn merge(text: &str, defaults: &str) -> Result<(String, Vec<String>, Vec<String>), String> {
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("{e}"))?;
    let from: toml_edit::DocumentMut = defaults.parse().map_err(|e| format!("{e}"))?;
    let name_of = |t: &toml_edit::Table| t.get("name").and_then(|v| v.as_str()).map(str::to_owned);
    let have: Vec<String> = doc
        .get("case")
        .and_then(toml_edit::Item::as_array_of_tables)
        .map(|a| a.iter().filter_map(name_of).collect())
        .unwrap_or_default();
    let (mut added, mut kept) = (Vec::new(), Vec::new());
    let mut new = Vec::new();
    for t in from
        .get("case")
        .and_then(toml_edit::Item::as_array_of_tables)
        .into_iter()
        .flat_map(toml_edit::ArrayOfTables::iter)
    {
        let Some(name) = name_of(t) else { continue };
        if have.contains(&name) {
            kept.push(name);
        } else {
            let mut t = t.clone();
            // Its own place at the end of the file (not the position it
            // had in the defaults' file).
            t.set_position(None);
            new.push(t);
            added.push(name);
        }
    }
    if !new.is_empty() {
        // A load case file raised to version 7 by the defaults: its own
        // body angles from the old convention first (2026-11-15; θ = 90° −
        // θ_old), the defaults' already in the new one.
        let version = |d: &toml_edit::DocumentMut| d.get("version").and_then(|v| v.as_integer());
        if version(&from).is_some_and(|v| v >= 7) && version(&doc).is_some_and(|w| w < 7) {
            crate::library::convert_old_directions(&mut doc);
        }
        if doc.get("case").is_none() {
            doc.insert(
                "case",
                toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()),
            );
        }
        let cases = doc
            .get_mut("case")
            .and_then(toml_edit::Item::as_array_of_tables_mut)
            .ok_or("`case` is not a list of tables")?;
        for t in new {
            cases.push(t);
        }
        if let Some(v) = version(&from)
            && version(&doc).is_some_and(|w| w < v)
        {
            doc.insert("version", toml_edit::value(v));
        }
    }
    Ok((doc.to_string(), added, kept))
}

/// [`merge`] of the default weather cases into the weather case file
/// `text`, checked.
pub fn merge_weather(text: &str) -> Result<(String, Vec<String>, Vec<String>), String> {
    let out = merge(text, ohl_weather::DEFAULT_CASES_TEXT)?;
    ohl_weather::WeatherCases::parse(&out.0)?;
    Ok(out)
}

/// [`merge`] of the default load cases into the load case file `text`,
/// checked.
pub fn merge_load_cases(text: &str) -> Result<(String, Vec<String>, Vec<String>), String> {
    let out = merge(text, ohl_loads::DEFAULT_LOAD_CASES_TEXT)?;
    ohl_loads::LoadCases::parse(&out.0)?;
    Ok(out)
}

/// The library files a new project gets with the defaults ticked, given
/// the files it gets otherwise (`files`: kind, name, text — the starter
/// template's, or none): the starter's weather and load case files with
/// the default cases they lack added, and for a kind it has no file of,
/// the default file itself.
pub fn with_defaults(
    mut files: Vec<(String, String, String)>,
) -> Result<Vec<(String, String, String)>, String> {
    for (kind, name, merge_into, shipped) in [
        (
            "weather",
            ohl_weather::DEFAULT_CASES_FILE,
            merge_weather as fn(&str) -> Result<(String, Vec<String>, Vec<String>), String>,
            ohl_weather::DEFAULT_CASES_TEXT,
        ),
        (
            "loadcases",
            ohl_loads::DEFAULT_LOAD_CASES_FILE,
            merge_load_cases,
            ohl_loads::DEFAULT_LOAD_CASES_TEXT,
        ),
    ] {
        match files.iter_mut().find(|(k, _, _)| k == kind) {
            Some((_, n, text)) => {
                *text = merge_into(text).map_err(|e| format!("{n}: {e}"))?.0;
            }
            None => files.push((kind.to_owned(), name.to_owned(), shipped.to_owned())),
        }
    }
    Ok(files)
}

/// The default load cases the load case form's `cases` lack (by name),
/// and the names it has already (not replaced).
pub fn missing_load_cases(cases: &ohl_loads::LoadCases) -> (Vec<ohl_loads::LoadCase>, Vec<String>) {
    let mut kept = Vec::new();
    let missing = ohl_loads::default_load_cases()
        .cases
        .into_iter()
        .filter(|d| {
            let there = cases.cases.iter().any(|c| c.name == d.name);
            if there {
                kept.push(d.name.clone());
            }
            !there
        })
        .collect();
    (missing, kept)
}

/// The weather cases the load cases `cases` name that are not among the
/// project's `weather` (by name), each once.
pub fn weather_missing(
    cases: &[ohl_loads::LoadCase],
    weather: &[ohl_weather::WeatherCase],
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in cases {
        if !weather.iter().any(|w| w.name == c.weather) && !out.contains(&c.weather) {
            out.push(c.weather.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Merged into a file of its own cases: those missing added at the
    /// end, a case of the same name kept as it is (not replaced), comments
    /// kept, the version raised to the defaults'; merged again, nothing
    /// more.
    #[test]
    fn merging_converts_the_files_old_body_angles() {
        // A version-4 file's body angle (0° from the right, towards ahead)
        // raised to 7 by the defaults: converted, the same wind.
        let text = "format = \"ohl-load-cases\"\nversion = 4\n\n[[case]]\nname = \"mine\"\n\
                    weather = \"10 °C\"\n[case.structure]\ndirection = 30\n";
        let before = ohl_loads::LoadCases::parse(text).unwrap().cases[0]
            .on_structure
            .direction
            .unwrap();
        let (out, _, _) = merge_load_cases(text).unwrap();
        let read = ohl_loads::LoadCases::parse(&out).unwrap();
        assert_eq!(read.version, Some(7));
        let after = read.cases[0].on_structure.direction.unwrap();
        assert_eq!(after, ohl_loads::WindDirection::Angle(60.0));
        assert_eq!(after.degrees(), before.degrees());
    }

    #[test]
    fn merging_adds_only_the_missing_cases() {
        let text = "# my cases\nformat = \"ohl-load-cases\"\nversion = 1\n\n[[case]]\n\
                    name = \"10 °C\"\n# mine\nweather = \"10 °C\"\nvertical = 2.0\n";
        let (out, added, kept) = merge_load_cases(text).unwrap();
        assert_eq!(kept, ["10 °C"]);
        assert_eq!(added.len(), ohl_loads::default_load_cases().cases.len() - 1);
        assert!(out.starts_with("# my cases") && out.contains("# mine"));
        let read = ohl_loads::LoadCases::parse(&out).unwrap();
        assert_eq!(read.version, Some(7));
        let mine = read.cases.iter().find(|c| c.name == "10 °C").unwrap();
        assert_eq!(mine.vertical, 2.0);
        assert_eq!(read.cases[0].name, "10 °C");
        let wind = read
            .cases
            .iter()
            .find(|c| c.name == "10 °C + 300 Pa wind ahead")
            .unwrap();
        assert_eq!(
            wind.wind_direction,
            Some(ohl_loads::WindDirection::Angle(0.0))
        );
        let (again, added, _) = merge_load_cases(&out).unwrap();
        assert!(added.is_empty());
        assert_eq!(again, out);
        // Weather cases into an empty file (a header only).
        let (w, added, kept) =
            merge_weather("format = \"ohl-weather-cases\"\nversion = 1\n").unwrap();
        assert_eq!(added.len(), 9);
        assert!(kept.is_empty());
        assert_eq!(
            ohl_weather::WeatherCases::parse(&w).unwrap().cases,
            ohl_weather::default_cases().cases
        );
    }

    /// A new project's files: the starter's weather and load case files
    /// with the defaults added (each its own file still), none: the two
    /// default files.
    #[test]
    fn a_new_project_gets_the_default_files_or_the_starters_with_them() {
        let empty = with_defaults(Vec::new()).unwrap();
        assert_eq!(
            empty
                .iter()
                .map(|(k, n, _)| (k.as_str(), n.as_str()))
                .collect::<Vec<_>>(),
            [
                ("weather", ohl_weather::DEFAULT_CASES_FILE),
                ("loadcases", ohl_loads::DEFAULT_LOAD_CASES_FILE)
            ]
        );
        let starter = crate::starter::library_files(&std::path::PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/starter"
        )))
        .unwrap();
        let n = starter.len();
        let with = with_defaults(starter).unwrap();
        assert_eq!(with.len(), n);
        let (_, _, w) = with.iter().find(|(k, _, _)| k == "weather").unwrap();
        let w = ohl_weather::WeatherCases::parse(w).unwrap();
        assert!(w.cases.iter().any(|c| c.name == "everyday"));
        assert!(w.cases.iter().any(|c| c.name == "0 °C + 60 N/m ice"));
        let (_, _, l) = with.iter().find(|(k, _, _)| k == "loadcases").unwrap();
        let l = ohl_loads::LoadCases::parse(l).unwrap();
        assert!(l.cases.iter().any(|c| c.name == "wind"));
        assert!(l.cases.iter().any(|c| c.name == "10 °C + 300 Pa wind back"));
        // Every load case names a weather case of the file.
        assert!(weather_missing(&l.cases, &w.cases).is_empty());
    }
}
