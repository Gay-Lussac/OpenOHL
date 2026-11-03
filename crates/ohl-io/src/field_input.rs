// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The points a field-presets plugin asks for in its manifest's
//! `[field-input]` table ([`ohl_plugins::FieldInputTable`]; 2026-10-31),
//! gathered by the host before the plugin is called (the plugin calls
//! nothing back): the point cloud's points of some class roles, its
//! ground thinned to the lowest point per cell, and survey points of some
//! codes, all within a reach of the spans in plan.

use ohl_core::budget::Budget;
use ohl_lidar::{ClassScheme, Corridor, LowestPerCell};
use ohl_plugins::{FieldInputTable, InputPoint};
use ohl_pointcloud::PointSource;
use ohl_terrain::SurveyPoint;

/// What was gathered for a plugin.
#[derive(Debug, Clone, Default)]
pub struct Gathered {
    /// The points of the roles (class as read, no code) and the survey
    /// points of the codes (class 0, their code and breakline).
    pub points: Vec<InputPoint>,
    /// The ground: the lowest point of the ground roles per cell, x, y, z
    /// one after the other.
    pub ground: Vec<f64>,
    /// What was read and from where, in words.
    pub notes: Vec<String>,
}

/// The classes of `roles` in `scheme`; a role no class has is an error
/// naming it and the scheme.
fn classes(scheme: &ClassScheme, roles: &[String], what: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for r in roles {
        let c = scheme.codes_with_role(r);
        if c.is_empty() {
            return Err(format!(
                "field-input {what}: no class of the point cloud's class scheme \"{}\" has the role \"{r}\"",
                scheme.name
            ));
        }
        out.extend(c);
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

/// Gathers what `table` asks for within its reach of `spans` (plan
/// polylines, e.g. each span's structures): from `cloud` (its class
/// scheme, else ASPRS) the points of the roles and the ground roles'
/// lowest point per cell, from `survey` the points whose code matches one
/// of the survey codes. More points than the table's `max-points` (roles
/// and codes together) is an error saying so; so is a role the scheme
/// does not have. Without a cloud, roles give no points (said).
pub fn gather(
    table: &FieldInputTable,
    cloud: Option<&dyn PointSource>,
    survey: &[SurveyPoint],
    spans: &[Vec<[f64; 2]>],
    budget: &Budget,
) -> Result<Gathered, String> {
    gather_classes(table, None, cloud, survey, spans, budget)
}

/// [`gather`] with the classes chosen instead of the table's roles
/// (`chosen`: the points' classes and the ground's; owner 2026-11-03: a
/// tick list of the point cloud's classes), the survey codes the table's.
pub fn gather_classes(
    table: &FieldInputTable,
    chosen: Option<(&[u8], &[u8])>,
    cloud: Option<&dyn PointSource>,
    survey: &[SurveyPoint],
    spans: &[Vec<[f64; 2]>],
    budget: &Budget,
) -> Result<Gathered, String> {
    table.check()?;
    let mut out = Gathered::default();
    let corridor = Corridor::new(spans, table.reach);
    let max = usize::try_from(table.max_points).unwrap_or(usize::MAX);
    let too_many = || {
        format!(
            "field-input: more than {} points (max-points) within {} m of the spans: \
             raise max-points in the plugin's manifest or narrow its reach",
            table.max_points, table.reach
        )
    };
    let wants_cloud = match chosen {
        Some((p, g)) => !p.is_empty() || !g.is_empty(),
        None => !table.roles.is_empty() || !table.ground_roles.is_empty(),
    };
    match cloud {
        Some(source) if wants_cloud && !corridor.is_empty() => {
            let scheme = source
                .class_scheme()
                .and_then(|t| ClassScheme::parse(&t).ok())
                .unwrap_or_else(ClassScheme::asprs);
            let (points, ground) = match chosen {
                Some((p, g)) => (p.to_vec(), g.to_vec()),
                None => (
                    classes(&scheme, &table.roles, "roles")?,
                    classes(&scheme, &table.ground_roles, "ground-roles")?,
                ),
            };
            let named = |c: &[u8]| -> Vec<String> { c.iter().map(|k| k.to_string()).collect() };
            let (roles, ground_roles) = match chosen {
                Some(_) => (
                    format!("the classes {}", list(&named(&points))),
                    format!("classes {}", list(&named(&ground))),
                ),
                None => (
                    format!("the roles {}", list(&table.roles)),
                    format!("roles {}", list(&table.ground_roles)),
                ),
            };
            let mut all: Vec<u8> = points.iter().chain(&ground).copied().collect();
            all.sort_unstable();
            all.dedup();
            let mut is_point = [false; 256];
            let mut is_ground = [false; 256];
            for &c in &points {
                is_point[c as usize] = true;
            }
            for &c in &ground {
                is_ground[c as usize] = true;
            }
            let mut cells = LowestPerCell::new(Some(table.ground_cell));
            let mut over = false;
            source.visit(Some(&corridor), Some(&all), budget, &mut |x, y, z, c| {
                if is_point[c as usize] {
                    if out.points.len() >= max {
                        over = true;
                        return;
                    }
                    out.points.push(InputPoint {
                        x,
                        y,
                        z,
                        class: c,
                        code: String::new(),
                        line: String::new(),
                    });
                }
                if is_ground[c as usize] {
                    cells.add(x, y, z);
                }
            })?;
            if over {
                return Err(too_many());
            }
            out.ground = cells.points().iter().flatten().copied().collect();
            out.notes.push(format!(
                "{} points of {} and {} ground cells of {} m ({}) from {}, within {} m of the spans",
                out.points.len(),
                roles,
                cells.points().len(),
                table.ground_cell,
                ground_roles,
                source.describe(),
                table.reach
            ));
        }
        None if wants_cloud => out.notes.push(format!(
            "no point cloud: no points of the roles {} or ground (roles {})",
            list(&table.roles),
            list(&table.ground_roles)
        )),
        _ => {}
    }
    if !table.survey_codes.is_empty() {
        let before = out.points.len();
        for p in survey {
            if !table
                .survey_codes
                .iter()
                .any(|c| ohl_core::pattern::matches(c, &p.code))
                || !corridor.contains(p.x, p.y)
            {
                continue;
            }
            if out.points.len() >= max {
                return Err(too_many());
            }
            out.points.push(InputPoint {
                x: p.x,
                y: p.y,
                z: p.z,
                class: 0,
                code: p.code.clone(),
                line: p.line.clone(),
            });
        }
        out.notes.push(format!(
            "{} survey points of the codes {} (of {} read) within {} m of the spans",
            out.points.len() - before,
            list(&table.survey_codes),
            survey.len(),
            table.reach
        ));
    }
    Ok(out)
}

/// A preset's cuts as the fields take them: at their alignment stations.
pub fn preset_cuts(p: &ohl_plugins::FieldPreset) -> Vec<ohl_electrical::magnetic::CutSpec> {
    p.cuts
        .iter()
        .map(|c| ohl_electrical::magnetic::CutSpec {
            at: ohl_electrical::magnetic::CutAt::Station {
                alignment: c.alignment.clone(),
                station: c.station,
            },
            width: c.width,
            above: c.above,
            step: c.step,
            label: (!c.label.is_empty()).then(|| c.label.clone()),
        })
        .collect()
}

/// A preset's points as the fields take them.
pub fn preset_points(p: &ohl_plugins::FieldPreset) -> Vec<ohl_electrical::magnetic::GivenPoint> {
    p.points
        .iter()
        .map(|q| ohl_electrical::magnetic::GivenPoint {
            name: q.name.clone(),
            x: q.x,
            y: q.y,
            z: q.z,
            height: q.height,
        })
        .collect()
}

fn list(items: &[String]) -> String {
    if items.is_empty() {
        "none".into()
    } else {
        items.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use ohl_pointcloud::Stats;

    use super::*;

    /// An invented point cloud in memory (ASPRS classes unless a scheme
    /// is given).
    struct Cloud {
        points: Vec<[f64; 3]>,
        classes: Vec<u8>,
        scheme: Option<String>,
    }

    impl PointSource for Cloud {
        fn describe(&self) -> String {
            "test cloud".into()
        }
        fn class_scheme(&self) -> Option<String> {
            self.scheme.clone()
        }
        fn visit(
            &self,
            area: Option<&Corridor>,
            classes: Option<&[u8]>,
            _budget: &Budget,
            visit: &mut dyn FnMut(f64, f64, f64, u8),
        ) -> Result<Stats, String> {
            for (p, &c) in self.points.iter().zip(&self.classes) {
                if area.is_none_or(|a| a.contains(p[0], p[1]))
                    && classes.is_none_or(|k| k.contains(&c))
                {
                    visit(p[0], p[1], p[2], c);
                }
            }
            Ok(Stats::default())
        }
    }

    /// A span 0–300 m along x; buildings (6) near and far, ground (2) in
    /// two cells, vegetation (5) left out; survey points by code.
    fn cloud() -> Cloud {
        Cloud {
            points: vec![
                [100.0, 20.0, 405.0],
                [101.0, 20.5, 406.0],
                [100.0, 400.0, 405.0],
                [10.2, 0.4, 400.5],
                [10.8, 1.1, 400.2],
                [13.0, 0.5, 401.0],
                [50.0, 5.0, 410.0],
            ],
            classes: vec![6, 6, 6, 2, 2, 2, 5],
            scheme: None,
        }
    }

    fn table() -> FieldInputTable {
        FieldInputTable {
            roles: vec!["building".into()],
            ground_roles: vec!["ground".into()],
            survey_codes: vec!["BLD*".into()],
            ..FieldInputTable::default()
        }
    }

    fn survey(code: &str, x: f64, y: f64) -> SurveyPoint {
        SurveyPoint {
            x,
            y,
            z: 404.0,
            code: code.into(),
            line: "r1".into(),
        }
    }

    #[test]
    fn gathers_roles_ground_and_survey_codes() {
        let spans = vec![vec![[0.0, 0.0], [300.0, 0.0]]];
        let survey = [
            survey("bld-roof", 120.0, -30.0),
            survey("TREE", 120.0, -30.0),
            survey("BLD2", 120.0, 300.0),
        ];
        let c = cloud();
        let g = gather(&table(), Some(&c), &survey, &spans, &Budget::new()).unwrap();
        // The two buildings within 100 m, then the survey point (any case).
        assert_eq!(g.points.len(), 3);
        assert!(
            g.points[..2]
                .iter()
                .all(|p| p.class == 6 && p.code.is_empty())
        );
        assert_eq!(
            (
                g.points[2].class,
                g.points[2].code.as_str(),
                g.points[2].line.as_str()
            ),
            (0, "bld-roof", "r1")
        );
        // The ground: the lowest point per 2 m cell (10.8 m in the first).
        assert_eq!(g.ground, [10.8, 1.1, 400.2, 13.0, 0.5, 401.0]);
        assert_eq!(g.notes.len(), 2);
        // Too many points: refused, said.
        let small = FieldInputTable {
            max_points: 2,
            ..table()
        };
        let e = gather(&small, Some(&c), &survey, &spans, &Budget::new()).unwrap_err();
        assert!(e.contains("more than 2 points (max-points)"), "{e}");
        let e = gather(
            &FieldInputTable {
                max_points: 1,
                ..table()
            },
            Some(&c),
            &[],
            &spans,
            &Budget::new(),
        )
        .unwrap_err();
        assert!(e.contains("max-points"), "{e}");
        // A role the cloud's scheme does not have: named.
        let scheme = "id = \"s\"\nname = \"Survey\"\n[[class]]\ncode = 2\nname = \"G\"\nroles = [\"ground\"]\n";
        let other = Cloud {
            scheme: Some(scheme.into()),
            ..cloud()
        };
        let e = gather(&table(), Some(&other), &[], &spans, &Budget::new()).unwrap_err();
        assert!(
            e.contains("role \"building\"") && e.contains("\"Survey\""),
            "{e}"
        );
        // No cloud: the survey points alone, said.
        let g = gather(&table(), None, &survey, &spans, &Budget::new()).unwrap();
        assert_eq!(g.points.len(), 1);
        assert!(g.ground.is_empty() && g.notes[0].contains("no point cloud"));
    }
}
