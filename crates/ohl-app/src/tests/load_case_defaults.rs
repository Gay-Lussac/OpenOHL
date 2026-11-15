// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Load cases library form shows the values a case does not give
//! (owner, 2026-11-15: "the default should show what the value is even if
//! greyed out for example the ice and snow on the structure or the wind
//! factor"), each computed by the function the loads use; and the
//! structures' wind "as the wires" says the direction the wires actually
//! take (owner, 2026-11-15: "as for the wires right … isn't accurate
//! anymore"). Every value here is INVENTED (the synthetic example line).

use super::*;
use crate::load_case_form::{self, Form, FormContext};
use egui::accesskit::Role;
use egui_kittest::kittest::NodeT;
use ohl_weather::WeatherCase;

/// The form alone, on a file's text.
struct State {
    form: Form,
    item: usize,
    text: String,
    weather: Vec<WeatherCase>,
    line: Option<ohl_line::Line>,
}

fn form(
    text: &str,
    weather: Vec<WeatherCase>,
    line: Option<ohl_line::Line>,
) -> Harness<'static, State> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 1600.0))
        .build_ui_state(
            |ui, s: &mut State| {
                s.form.sync(&s.text);
                let cx = FormContext {
                    weather: &s.weather,
                    line: s.line.as_ref(),
                    project: false,
                    dirty: false,
                    in_use: false,
                    known: None,
                    selected: &[],
                    line_version: 0,
                    read_only: None,
                };
                if let Some(t) = load_case_form::ui(ui, &mut s.form, &mut s.item, &cx, None) {
                    s.text = t;
                }
            },
            State {
                form: Form::default(),
                item: 0,
                text: text.to_owned(),
                weather,
                line,
            },
        );
    h.run();
    h
}

fn synthetic_weather() -> Vec<WeatherCase> {
    ohl_weather::WeatherCases::load(&root().join("examples/synthetic-line/weather-cases.ohlwc"))
        .unwrap()
        .cases
}

fn synthetic_line() -> ohl_line::Line {
    ohl_line::Line::load(&root().join("examples/synthetic-line/synthetic-line.ohl.json")).unwrap()
}

/// A text field by its label: (value, greyed).
fn field(h: &Harness<'_, State>, label: &str) -> Vec<(String, bool)> {
    h.query_all_by_role_and_label(Role::TextInput, label)
        .map(|n| {
            let n = n.accesskit_node();
            (n.value().unwrap_or_default(), n.is_disabled())
        })
        .collect()
}

fn case(h: &mut Harness<'_, State>, name: &str) -> ohl_loads::LoadCase {
    ohl_loads::LoadCases::read_unchecked(&h.state().text)
        .unwrap()
        .cases
        .into_iter()
        .find(|c| c.name == name)
        .unwrap()
}

fn weather_named<'a>(w: &'a [WeatherCase], name: &str) -> &'a WeatherCase {
    w.iter().find(|w| w.name == name).unwrap()
}

const CASES: &str = "format = \"ohl-load-cases\"\nversion = 6\n\
    [[case]]\nname = \"wind\"\nweather = \"high wind\"\ntransverse = 1.4\n\
    [[case]]\nname = \"wires' ice\"\nweather = \"glaze ice\"\nvertical = 1.3\n\
    [case.structure]\nice = \"weather\"\nsnow = true\n\
    [[case]]\nname = \"local ice\"\nweather = \"glaze ice\"\nuse_local = true\n\
    [[case]]\nname = \"no local\"\nweather = \"glaze ice\"\n";

/// A case with wind giving nothing for the structures: the pressure and
/// the factor shown greyed (disabled), with their sources, equal to
/// `LoadCase::structure_wind`; the direction "as the wires" with the
/// wires' direction. Ticking "own factor" starts from the value shown.
#[test]
fn the_wind_on_the_structures_shows_its_defaults_and_ticking_starts_from_them() {
    let weather = synthetic_weather();
    let mut h = form(CASES, weather.clone(), Some(synthetic_line()));
    let c = case(&mut h, "wind");
    let body = c.structure_wind(weather_named(&weather, "high wind"));
    assert_eq!((body.pressure, body.factor), (600.0, 1.4));
    assert_eq!(field(&h, "Pa"), [(format!("{}", body.pressure), true)]);
    assert_eq!(field(&h, "×"), [(format!("{}", body.factor), true)]);
    h.get_by_label("(the weather case's)");
    h.get_by_label("(the transverse factor)");
    h.get_by_label("as the wires: from the right (90°)");
    h.get_by_label("= 840 Pa on the bodies");
    // The factors at 1 (no factor): their value shown.
    assert_eq!(field(&h, "vertical ×"), [("1".to_owned(), false)]);
    // Ticked: the case's own factor, from the value shown, editable.
    h.get_by_label("own factor").click();
    h.run();
    h.run();
    let c = case(&mut h, "wind");
    assert_eq!(c.structure.unwrap().wind_factor, Some(body.factor));
    assert_eq!(field(&h, "×"), [("1.4".to_owned(), false)]);
    // The same factor computed: nothing changes but who gives it.
    let own = c.structure_wind(weather_named(&weather, "high wind"));
    assert_eq!(own.factored(), body.factored());
    assert!(own.own_factor);
}

/// Cases with ice: the wires' ice with its thickness × 1 and the glaze
/// density shown (the computation's `structure_ice_at`), the factor 1
/// greyed; the snow's μ C_e C_T and s_k per structure; the default ice of
/// a case taking the local conditions per structure, and of one not
/// taking them: none, with why.
#[test]
fn the_ice_and_snow_on_the_structures_show_what_applies() {
    let weather = synthetic_weather();
    let mut line = synthetic_line();
    line.structures
        .iter_mut()
        .find(|s| s.id == "T3")
        .unwrap()
        .local
        .structure_ice = Some(0.02);
    let mut h = form(CASES, weather.clone(), Some(line.clone()));
    let glaze = weather_named(&weather, "glaze ice");
    h.state_mut().item = 1;
    h.run();
    h.run();
    let c = case(&mut h, "wires' ice");
    let ice = c.structure_ice_at(glaze, None).unwrap().unwrap();
    assert!((ice.thickness - 0.0127).abs() < 1e-12);
    h.get_by_label(&format!(
        "the wires' ice: 12.7 mm × 1 = {:.1} mm on every structure taking the case",
        ice.thickness * 1000.0
    ));
    assert_eq!(field(&h, "kg/m³"), [(format!("{}", ice.density), true)]);
    assert_eq!(ice.density, 913.0);
    h.get_by_label("(the weather case's)");
    // The ice's factor (and the bodies' wind factor, the transverse 1
    // of this windless case, above it), greyed.
    assert_eq!(
        field(&h, "×"),
        [("1".to_owned(), true), ("1".to_owned(), true)]
    );
    h.get_by_label("(none given: 1)");
    h.get_by_label("(the weather case's: no wind)");
    // Snow: the defaults shown, their product the computation's.
    assert_eq!(field(&h, "μ"), [("0.8".to_owned(), false)]);
    assert_eq!(field(&h, "C_e"), [("1".to_owned(), false)]);
    assert_eq!(field(&h, "C_T"), [("1".to_owned(), false)]);
    assert_eq!(c.on_structure.snow_factor(), 0.8);
    h.get_by_label_contains("μ·C_e·C_T = 0.8 (the defaults); s_k per structure");
    // Ticking own density starts from the value shown.
    h.get_by_label("own density").click();
    h.run();
    h.run();
    assert_eq!(
        case(&mut h, "wires' ice").on_structure.ice_density,
        Some(ice.density)
    );
    // The default ice, the case taking the local conditions: T3's.
    h.state_mut().item = 2;
    h.run();
    h.run();
    let c = case(&mut h, "local ice");
    let t3 = line.structures.iter().find(|s| s.id == "T3").unwrap();
    let at_t3 = c.structure_ice_at(glaze, Some(t3)).unwrap().unwrap();
    assert_eq!(at_t3.thickness, 0.02);
    h.get_by_label_contains(&format!(
        "default: each structure's local ice — T3 {:.1} mm (1 of",
        at_t3.thickness * 1000.0
    ));
    assert_eq!(field(&h, "kg/m³"), [("913".to_owned(), true)]);
    // Not taking them: none, and why.
    h.state_mut().item = 3;
    h.run();
    h.run();
    h.get_by_label_contains("default: none here — the case does not take the local conditions");
}

/// The default cases along the line (owner, 2026-11-15): "as the wires"
/// says the direction the wires take in each — from ahead, from behind,
/// from the left — never a fixed "right"; it is the bodies' direction the
/// loads take (`LoadCase::structure_wind`).
#[test]
fn as_the_wires_says_the_wires_direction_of_the_default_cases() {
    let weather = ohl_weather::default_cases().cases;
    let cases = ohl_loads::default_load_cases().cases;
    let mut h = form(ohl_loads::DEFAULT_LOAD_CASES_TEXT, weather.clone(), None);
    for (name, words) in [
        ("10 °C + 300 Pa wind ahead", "from ahead (0°)"),
        ("10 °C + 300 Pa wind back", "from behind (180°)"),
        ("10 °C + 300 Pa wind left", "from the left (270°)"),
        ("10 °C + 300 Pa wind right", "from the right (90°)"),
    ] {
        let k = cases.iter().position(|c| c.name == name).unwrap();
        h.state_mut().item = k;
        h.run();
        h.run();
        let c = &cases[k];
        let w = weather_named(&weather, &c.weather);
        let used = c.structure_wind(w).direction;
        assert_eq!(load_case_form::wires_direction(c, w), used, "{name}");
        h.get_by_label(&format!("as the wires: {words}"));
        assert!(
            h.query_all_by_label_contains("as the wires: from the right")
                .next()
                .is_none()
                || words.contains("right"),
            "{name}"
        );
        h.get_by_label_contains(&format!("The wires: {words}"));
    }
}
