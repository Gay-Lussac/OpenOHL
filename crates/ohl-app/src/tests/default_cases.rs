// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! OpenOHL's default weather and load cases added to an existing project
//! (owner, 2026-11-15): **Add the default cases…** in the Weather cases
//! table and in the Load cases library adds only the cases missing (by
//! name), never replacing one, as one change undone as one.

use super::objects::apply_table;
use super::*;
use crate::library::LibraryKind;
use crate::tables::TableKind;

fn app(model: model::Model) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 1100.0))
        .build_eframe(|_| App::with_model(model, None));
    h.run();
    h
}

fn names(h: &mut Harness<'_, App>) -> Vec<String> {
    let r = h.state_mut().ready().unwrap();
    r.model.cases.iter().map(|c| c.case.name.clone()).collect()
}

/// The Weather cases table: a case of a default's name typed first is
/// kept (not replaced); the others come as new rows, Apply keeps them in
/// one change, one undo takes them all back; once there, nothing more is
/// added.
#[test]
fn the_weather_table_adds_the_missing_default_cases() {
    let mut h = app(synthetic_project("default-weather"));
    h.state_mut().ready().unwrap().open_page =
        Some((crate::app::Tab::Table(TableKind::WeatherCases), None));
    h.run();
    h.run();
    let before_names = names(&mut h);
    let (path, before) = {
        let r = h.state_mut().ready().unwrap();
        let path = r.model.sources.weather.clone().unwrap();
        (path.clone(), std::fs::read_to_string(&path).unwrap())
    };
    // A case named as a default one, typed in first (5 °C, not -20 °C).
    {
        let r = h.state_mut().ready().unwrap();
        let mut row = TableKind::WeatherCases.new_row(&r.model, r.case, &[]);
        row[1] = "-20 °C".into();
        row[10] = "5".into();
        r.tables
            .entry(TableKind::WeatherCases)
            .or_default()
            .added
            .push(row);
    }
    h.run();
    h.get_by_label(crate::default_cases::ADD_BUTTON).click();
    h.run();
    let defaults = ohl_weather::default_cases().cases;
    {
        let r = h.state_mut().ready().unwrap();
        let page = &r.tables[&TableKind::WeatherCases];
        assert_eq!(
            page.added.len(),
            defaults.len(),
            "8 defaults and the one typed"
        );
        let (m, error) = page.message.clone().unwrap();
        assert!(!error && m.contains("kept: -20 °C"), "{m}");
    }
    apply_table(&mut h, TableKind::WeatherCases, true).unwrap();
    let after = names(&mut h);
    assert_eq!(after.len(), before_names.len() + defaults.len());
    assert!(defaults.iter().all(|d| after.contains(&d.name)));
    {
        let r = h.state_mut().ready().unwrap();
        let mine = r
            .model
            .cases
            .iter()
            .find(|c| c.case.name == "-20 °C")
            .unwrap();
        assert_eq!(mine.case.temperature.celsius(), 5.0, "never replaced");
        let wind = r
            .model
            .cases
            .iter()
            .find(|c| c.case.name == "10 °C + 300 Pa wind")
            .unwrap();
        assert_eq!(wind.case.wind_pressure.si(), 300.0);
        let ice = r
            .model
            .cases
            .iter()
            .find(|c| c.case.name == "0 °C + 60 N/m ice")
            .unwrap();
        assert_eq!(ice.case.ice_load.map(|l| l.si()), Some(60.0));
        assert_eq!(ice.case.ice_density, None);
        assert!(
            ice.case
                .description
                .starts_with("default case, arbitrary values")
        );
    }
    // Once there: nothing more.
    h.run();
    h.get_by_label(crate::default_cases::ADD_BUTTON).click();
    h.run();
    {
        let r = h.state_mut().ready().unwrap();
        let page = &r.tables[&TableKind::WeatherCases];
        assert!(page.added.is_empty());
        assert!(page.message.as_ref().unwrap().0.contains("all there"));
    }
    // One undo: the file as before.
    let r = h.state_mut().ready().unwrap();
    r.model.undo().unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
}

/// The Load cases library: the default cases the file lacks, after a look
/// at them (and at the weather cases they name that the project lacks),
/// added; Save keeps them, one change; one undo puts the file back.
#[test]
fn the_load_case_library_adds_the_missing_default_cases() {
    let mut h = app(synthetic_project("default-load-cases"));
    h.state_mut().ready().unwrap().open_page =
        Some((crate::app::Tab::Library(LibraryKind::LoadCases), None));
    h.run();
    h.run();
    let (path, before) = {
        let r = h.state_mut().ready().unwrap();
        let path = r.libraries[&LibraryKind::LoadCases].file.clone().unwrap();
        let before = std::fs::read_to_string(&path).unwrap();
        (path, before)
    };
    let defaults = ohl_loads::default_load_cases().cases;
    h.get_by_label(crate::default_cases::ADD_BUTTON).click();
    h.run();
    // The example's weather cases are not the defaults: said.
    h.get_by_label_contains("They name weather cases the project does not have");
    h.get_by_label(&format!("Add {} case(s)", defaults.len()))
        .click();
    h.run();
    // The 60 N/m ice only at or above 1000 m: its own list's rule put in
    // the line at once, one command (owner 2026-11-15).
    {
        let r = h.state_mut().ready().unwrap();
        let groups = &r.model.line.structure_groups;
        assert_eq!(
            groups
                .iter()
                .filter(|g| g.load_case.as_deref() == Some(ohl_loads::DEFAULT_HIGH_ICE_CASE))
                .map(|g| g.rules.elevation_from)
                .collect::<Vec<_>>(),
            [Some(ohl_loads::DEFAULT_HIGH_ICE_ELEVATION)],
            "{groups:?}"
        );
    }
    h.get_by_label("Save").click();
    h.run();
    let read = ohl_loads::LoadCases::load(&path).unwrap();
    let old = ohl_loads::LoadCases::parse(&before).unwrap();
    assert_eq!(read.cases.len(), old.cases.len() + defaults.len());
    for o in &old.cases {
        assert_eq!(read.cases.iter().find(|c| c.name == o.name), Some(o));
    }
    for d in &defaults {
        assert_eq!(read.cases.iter().find(|c| c.name == d.name), Some(d));
    }
    // Once there: nothing to add.
    h.get_by_label(crate::default_cases::ADD_BUTTON).click();
    h.run();
    h.get_by_label_contains("The default load cases are all there");
    // One undo: the file as before.
    let r = h.state_mut().ready().unwrap();
    r.model.undo().unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    // Another: the default weather cases they named out again (added with
    // them, the tick on by default; 2026-11-15).
    assert!(r.model.cases.iter().any(|c| c.case.name == "-20 °C"));
    r.model.undo().unwrap();
    r.model.reload_libraries().unwrap();
    assert!(!r.model.cases.iter().any(|c| c.case.name == "-20 °C"));
    // Another: the own list out of the line again.
    r.model.undo().unwrap();
    assert!(
        !r.model
            .line
            .structure_groups
            .iter()
            .any(|g| g.load_case.as_deref() == Some(ohl_loads::DEFAULT_HIGH_ICE_CASE))
    );
}
