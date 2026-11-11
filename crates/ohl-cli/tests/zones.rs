// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl zones` on the synthetic example line, with an invented point
//! cloud: a tree 3 m below the middle of S1's first span.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use ohl_clearance::RuleSet;
use ohl_clearance::zone::{self, Measure, Zone};
use ohl_core::budget::Budget;
use ohl_weather::{WeatherCase, WeatherCases};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn example(name: &str) -> PathBuf {
    root().join("examples/synthetic-line").join(name)
}

/// A rule file with a vertical rule to class 5 (and the ground), in the
/// maximum temperature case only, solved by finite elements.
const RULES: &str = r#"format = "ohl-clearance-rules"
version = 1
name = "Zone test"

[[rule]]
name = "Trees"
target = "points"
classes = [5]
vertical = "5 m"
vertical_band = "2 m"
cases = ["max temperature"]

[[rule]]
name = "Ground"
target = "ground"
vertical = "7 m"
vertical_band = "2 m"
cases = ["max temperature"]

[wind]
steps = false

[check]
method = "fe"
search_distance = "30 m"
"#;

/// A folder of its own with the rule files and the point cloud.
struct Setup {
    dir: PathBuf,
    cloud: String,
}

impl Drop for Setup {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

fn setup(tag: &str) -> Setup {
    let dir = std::env::temp_dir().join(format!("ohl-zones-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let line = ohl_line::Line::load(&example("synthetic-line.ohl.json")).unwrap();
    let library = ohl_cables::Library::load(&root().join("libraries/cables")).unwrap();
    let hot = WeatherCases::load(&example("weather-cases.ohlwc"))
        .unwrap()
        .cases
        .into_iter()
        .find(|c| c.name == "max temperature")
        .unwrap();
    let shapes = ohl_line::solve::solve_line(&line, &library, &hot);
    let s1 = shapes.iter().flatten().find(|s| s.section == "S1").unwrap();
    let mid = s1.spans[0].mid_point;
    let point = |p: [f64; 3]| ohl_lidar::LidarPoint {
        x: p[0],
        y: p[1],
        z: p[2],
        class: 5,
        intensity: 0,
        return_number: 1,
        number_of_returns: 1,
    };
    let file = dir.join("scan.ohl");
    ohl_pointcloud::Store::open(&file)
        .unwrap()
        .import_points(
            "scan",
            "",
            None,
            64.0,
            0.001,
            [point([mid[0], mid[1], mid[2] - 3.0])],
            &Budget::new(),
        )
        .unwrap();
    Setup {
        cloud: format!("{}#scan", file.display()),
        dir,
    }
}

fn ohl(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(args)
        .output()
        .unwrap()
}

fn text(p: &Path) -> String {
    p.display().to_string()
}

#[test]
fn zones_by_a_rule_file_with_its_check() {
    let s = setup("check");
    let rules = s.dir.join("zone.ohlr");
    std::fs::write(&rules, RULES).unwrap();
    let csv = s.dir.join("out.csv");
    let (line, weather, survey) = (
        example("synthetic-line.ohl.json"),
        example("weather-cases.ohlwc"),
        example("synthetic-survey.csv"),
    );
    let out = ohl(&[
        "zones",
        &text(&line),
        "--rules",
        &text(&rules),
        "--weather",
        &text(&weather),
        "--cloud",
        &s.cloud,
        "--terrain",
        &text(&survey),
        "--csv",
        &text(&csv),
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("method fe, points within 30 m"), "{stdout}");
    assert!(stdout.contains("1 wind position(s)"), "{stdout}");
    // The time limit and where the time went (2026-10-27).
    assert!(
        stdout.contains("time limit: 15 min (900 s + 20 s per million"),
        "{stdout}"
    );
    assert!(stdout.contains("reading the points:"), "{stdout}");
    // "Down to the ground" by the terrain's classes (2026-11-11). The
    // trees' class is not one the terrain surface is built from: not
    // taken, and the vertical rule alone is said not on the safe side.
    assert!(!stdout.contains("taken by default"), "{stdout}");
    assert!(
        stdout.contains("NOT ON THE SAFE SIDE: \"Trees\": the vertical measure"),
        "{stdout}"
    );
    // A rule on the ground class (ASPRS 2): taken by default, said, and
    // no longer "not on the safe side"; with the trees' class beside it:
    // left off, said with both sides; switched off in the rule: nothing
    // taken, nothing said of a default.
    let with = |classes: &str, extra: &str| {
        let file = s.dir.join("default.ohlr");
        let rule_text = RULES.replace(
            "classes = [5]\n",
            &format!("classes = [{classes}]\n{extra}"),
        );
        std::fs::write(&file, rule_text).unwrap();
        let out = ohl(&[
            "zones",
            &text(&line),
            "--rules",
            &text(&file),
            "--weather",
            &text(&weather),
            "--cloud",
            &s.cloud,
            "--terrain",
            &text(&survey),
        ]);
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let said = with("2", "");
    assert!(
        said.contains(
            "note: \"Trees\": \"down to the ground\" taken by default: its classes are those \
             the terrain surface is built from (class 2)"
        ),
        "{said}"
    );
    assert!(!said.contains("NOT ON THE SAFE SIDE: \"Trees\""), "{said}");
    let said = with("2, 5", "");
    assert!(
        said.contains("not taken by default") && said.contains("(class 2 are, class 5 are not)"),
        "{said}"
    );
    assert!(said.contains("NOT ON THE SAFE SIDE: \"Trees\""), "{said}");
    let said = with("2", "to_ground = false\n");
    assert!(!said.contains("by default"), "{said}");
    assert!(said.contains("NOT ON THE SAFE SIDE: \"Trees\""), "{said}");
    // A route chosen: only its sections; an unknown one refused.
    let routed = |route: &str| {
        ohl(&[
            "zones",
            &text(&line),
            "--rules",
            &text(&rules),
            "--weather",
            &text(&weather),
            "--no-cloud",
            "--terrain",
            &text(&survey),
            "--route",
            route,
        ])
    };
    let main = routed("main");
    let said = String::from_utf8_lossy(&main.stdout);
    assert!(said.contains("solved 3 section(s)"), "{said}");
    let nowhere = routed("nowhere");
    assert_eq!(nowhere.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&nowhere.stderr).contains("--route nowhere: no such route"),
        "{}",
        String::from_utf8_lossy(&nowhere.stderr)
    );
    let rows = std::fs::read_to_string(&csv).unwrap();
    let tree: Vec<&str> = rows
        .lines()
        .find(|l| l.starts_with("violation,Trees,"))
        .unwrap_or_else(|| panic!("{rows}"))
        .split(',')
        .collect();
    assert_eq!((tree[2], tree[3], tree[8]), ("S1", "1", "5"), "{rows}");
    let found: f64 = tree[9].parse().unwrap();
    assert!((2.5..3.5).contains(&found), "{found}");

    // The same as the library function.
    let base = ohl_line::Line::load(&line).unwrap();
    let library = ohl_cables::Library::load(&root().join("libraries/cables")).unwrap();
    let set = RuleSet::load_with(&rules, &(&base.defaults).into()).unwrap();
    let scheme = ohl_lidar::ClassScheme::asprs();
    let imported = zone::from_rule_set(
        &set,
        Measure::Vertical,
        |r| scheme.codes_with_role(r),
        &zone::line_voltages(&base),
    );
    let check = imported.check.unwrap();
    let mut z = Zone::with_defaults(Measure::Vertical, &base.defaults);
    z.width = imported.width.unwrap();
    z.depth = imported.depth.unwrap();
    z.search = check.search_distance;
    let cases: Vec<WeatherCase> = WeatherCases::load(&weather).unwrap().cases;
    let mut base = base;
    base.set_weather_cases(&cases);
    let states = zone::wind_states(
        &cases,
        |n| imported.rules.iter().any(|r| r.applies(n)),
        &imported.wind.unwrap(),
    );
    let ground = ohl_terrain::Terrain::load(&survey).unwrap();
    let spec = s.cloud.clone();
    let read = |p: &[Vec<[f64; 2]>],
                reach: f64,
                classes: &[u8],
                budget: &Budget,
                visit: &mut dyn FnMut(f64, f64, f64, u8)| {
        ohl_pointcloud::visit_around(&spec, p, reach, classes, budget, visit).map(|_| ())
    };
    let r = zone::run_zone_check(
        &base,
        &library,
        z,
        &check,
        imported.rules,
        states,
        None,
        Some(&read),
        Some(&ground),
        &[],
        None,
        &Budget::new(),
        &ohl_clearance::zone_progress::ZoneProgress::new(),
        &Default::default(),
    )
    .unwrap();
    let violations = rows.lines().filter(|l| l.starts_with("violation,")).count();
    assert_eq!(r.violations(), violations, "{rows}");
    let mine = r.zone.findings.iter().find(|f| f.class == Some(5)).unwrap();
    assert_eq!(format!("{:.3}", mine.distance), tree[9]);
    assert_eq!(r.zone.points, 1);
}

#[test]
fn exit_codes() {
    let s = setup("exit");
    let (line, weather) = (
        example("synthetic-line.ohl.json"),
        example("weather-cases.ohlwc"),
    );
    // Trees only, 1 m: kept.
    let kept = s.dir.join("kept.ohlr");
    std::fs::write(
        &kept,
        "format = \"ohl-clearance-rules\"\nversion = 1\n\n[[rule]]\nname = \"Trees\"\n\
         target = \"points\"\nclasses = [5]\nvertical = \"1 m\"\nvertical_band = \"2 m\"\n\
         cases = [\"max temperature\"]\n\n[wind]\nsteps = false\n",
    )
    .unwrap();
    let csv = s.dir.join("kept.csv");
    let out = ohl(&[
        "zones",
        &text(&line),
        "--rules",
        &text(&kept),
        "--weather",
        &text(&weather),
        "--cloud",
        &s.cloud,
        "--search-distance",
        "20",
        "--csv",
        &text(&csv),
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert!(stdout.contains("0 spans below a rule"), "{stdout}");
    let rows = std::fs::read_to_string(&csv).unwrap();
    assert!(
        rows.lines().nth(1).unwrap().starts_with("ok,Trees,S1,1,"),
        "{rows}"
    );

    // Vertical and radial: refused without --measure.
    let both = s.dir.join("both.ohlr");
    std::fs::write(
        &both,
        "format = \"ohl-clearance-rules\"\nversion = 1\n\n[[rule]]\nname = \"Trees\"\n\
         target = \"points\"\nclasses = [5]\nvertical = \"5 m\"\nradial = \"4 m\"\n\
         cases = [\"max temperature\"]\n\n[wind]\nsteps = false\n",
    )
    .unwrap();
    let args = |extra: &[&str]| {
        let mut a = vec![
            "zones".to_owned(),
            text(&line),
            "--rules".into(),
            text(&both),
            "--weather".into(),
            text(&weather),
            "--cloud".into(),
            s.cloud.clone(),
        ];
        a.extend(extra.iter().map(|x| (*x).to_owned()));
        a
    };
    let refused = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(args(&[]))
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("--measure"));
    let radial = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(args(&["--measure", "radial"]))
        .output()
        .unwrap();
    assert_eq!(
        radial.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&radial.stdout)
    );
    assert!(String::from_utf8_lossy(&radial.stdout).starts_with("radial clearances"));
    // The band's margin (2026-10-21): said with the swing arc; a negative
    // one refused.
    let arc = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(args(&[
            "--measure",
            "radial",
            "--swing-arc",
            "--band-margin",
            "0.3",
            "--jumper-band-margin",
            "0.35",
        ]))
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&arc.stdout);
    assert!(
        said.contains("swing arc (band margin 0.3 m, jumpers 0.35 m)"),
        "{said}"
    );
    let refused = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(args(&["--measure", "radial", "--band-margin=-1"]))
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("--band-margin"));
    let refused = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(args(&["--measure", "radial", "--jumper-band-margin=-1"]))
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("--jumper-band-margin"));
    // No weather case file: an error.
    let none = ohl(&["zones", &text(&line), "--rules", &text(&kept)]);
    assert_eq!(none.status.code(), Some(2));
}

/// A rule's own calculation (owner, 2026-11-01: e.g. the ground at max
/// sag): the same ground rule by each section's own and by the ruling
/// span — each checked on the wires solved by its calculation, both
/// reported.
#[test]
fn a_zone_rule_takes_its_own_calculation() {
    let dir = std::env::temp_dir().join(format!("ohl-zones-calc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let rules = dir.join("rules.ohlr");
    std::fs::write(
        &rules,
        "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"t\"\n\n\
         [[rule]]\nname = \"own\"\ntarget = \"ground\"\nvertical = \"100 m\"\ncases = [\"max temperature\"]\n\n\
         [[rule]]\nname = \"rs\"\ntarget = \"ground\"\nvertical = \"100 m\"\ncases = [\"max temperature\"]\ncalculation = \"initial-rs\"\n\n\
         [wind]\nsteps = false\n",
    )
    .unwrap();
    let csv = dir.join("out.csv");
    let out = ohl(&[
        "zones",
        &text(&example("synthetic-line.ohl.json")),
        "--rules",
        &text(&rules),
        "--weather",
        &text(&example("weather-cases.ohlwc")),
        "--no-cloud",
        "--terrain",
        &text(&example("synthetic-survey.csv")),
        "--csv",
        &text(&csv),
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    let table = std::fs::read_to_string(&csv).unwrap();
    let lowest = |rule: &str| -> f64 {
        table
            .lines()
            .skip(1)
            .map(|l| l.split(',').collect::<Vec<_>>())
            .filter(|f| f[1].trim_matches('"') == rule)
            .map(|f| f[9].parse::<f64>().unwrap())
            .fold(f64::INFINITY, f64::min)
    };
    let (a, b) = (lowest("own"), lowest("rs"));
    assert!(a.is_finite() && b.is_finite(), "{table}");
    assert!(
        (a - b).abs() > 1e-3,
        "the ruling span's wires hang otherwise: {a} and {b}"
    );
    std::fs::remove_dir_all(&dir).ok();
}
