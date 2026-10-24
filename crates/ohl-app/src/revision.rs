// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Results out of date: what a result computed on demand (vegetation and
//! danger trees, as-built wires, vertical and radial clearances,
//! galloping, distance between conductors, structure loads, thermal
//! rating, costs, spotting) was computed from, compared with the model
//! now.
//!
//! A [`Revision`] names the state of what results are computed from:
//!
//! - **the line**: a hash of the line as solved (structures with their
//!   types, sections, wires, jumpers, settings — the calculation option
//!   of the ice and wind per stretched or unstretched metre apart, named
//!   on its own when it changes), the wire library, the
//!   weather cases (the wind side shown left out: a view setting) and the
//!   project's library files (rules, galloping and phase distance files,
//!   costs, structure and section libraries …);
//! - **the points**: a hash of the cloud's points as a set (the sum of a
//!   hash of each stored point: a change adds the new point's and takes
//!   off the old one's);
//! - **the ground**: the ground surface (the hash of its ground points
//!   when built again from the cloud, else a new number whenever another
//!   surface is used).
//!
//! The revision is a hash of content, not a count of edits: an undo back
//! to the state a result was computed from gives the same revision, and
//! the result is no longer out of date (it is exactly what a new run would
//! give). A view setting (camera, layers, weather case or wind side shown,
//! classes hidden) changes none of them.
//!
//! Each result keeps the revision it was computed from ([`Stamp`]); its
//! window compares it with the model's ([`Model::revision`]) as it is
//! drawn and shows a banner with **Run again** ([`banner`]); layers drawn
//! from it are dimmed. Nothing is run again by itself and no result is
//! dropped.

use std::cell::Cell;
use std::hash::{Hash, Hasher};

use eframe::egui;

use crate::model::Model;

/// The state of what results are computed from (hashes; see the module).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Revision {
    /// The line, its weather cases and library files.
    pub line: u64,
    /// The point cloud's points.
    pub points: u64,
    /// The ground surface.
    pub ground: u64,
    /// Per what metre the ice and wind were carried (Criteria ›
    /// Calculation options, 2026-10-21): part of the line, said on its own.
    pub loads_per: ohl_line::LoadsPer,
}

/// What a result is computed from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Uses {
    /// The line.
    pub line: bool,
    /// The point cloud.
    pub points: bool,
    /// The ground surface.
    pub ground: bool,
}

impl Uses {
    /// The line only.
    pub const LINE: Uses = Uses {
        line: true,
        points: false,
        ground: false,
    };
    /// The line and the ground.
    pub const LINE_GROUND: Uses = Uses {
        line: true,
        points: false,
        ground: true,
    };
    /// The line and the points.
    pub const LINE_POINTS: Uses = Uses {
        line: true,
        points: true,
        ground: false,
    };
    /// The line, the points and the ground.
    pub const ALL: Uses = Uses {
        line: true,
        points: true,
        ground: true,
    };
}

impl Revision {
    /// What of `uses` differs in `now`, in words ("the line and the LiDAR
    /// points"); `None`: nothing.
    pub fn changed(&self, now: &Revision, uses: Uses) -> Option<String> {
        let mut what = Vec::new();
        if uses.line && self.line != now.line {
            what.push("the line".to_owned());
        }
        if uses.line && self.loads_per != now.loads_per {
            what.push(format!(
                "the calculation options (now {})",
                now.loads_per.describe()
            ));
        }
        if uses.points && self.points != now.points {
            what.push("the LiDAR points".to_owned());
        }
        if uses.ground && self.ground != now.ground {
            what.push("the ground".to_owned());
        }
        match what.as_slice() {
            [] => None,
            [one] => Some(one.clone()),
            [first @ .., last] => Some(format!("{} and {last}", first.join(", "))),
        }
    }
}

/// The revision a result was computed from: taken when its run starts,
/// kept with the result when it arrives.
///
/// Saved results record the calculation option of the ice and wind
/// ([`Stamp::loads_per`]) with their parameters (`loads_per`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Stamp {
    pending: Option<Revision>,
    from: Option<Revision>,
}

impl Stamp {
    /// A run starts on the model at revision `now`.
    pub fn started(&mut self, now: Revision) {
        self.pending = Some(now);
    }

    /// Its result arrived: it was computed from the revision at the start.
    pub fn arrived(&mut self) {
        if let Some(p) = self.pending.take() {
            self.from = Some(p);
        }
    }

    /// The result shown was computed at revision `now` (a saved one read
    /// unchanged since: tracked from now on).
    pub fn set(&mut self, now: Revision) {
        self.pending = None;
        self.from = Some(now);
    }

    /// No revision known (a saved report shown: compared with the line
    /// by its own state).
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// The revision the result shown was computed from.
    #[cfg(test)]
    pub fn from(&self) -> Option<Revision> {
        self.from
    }

    /// Per what metre the ice and wind were carried in the run shown: its
    /// revision's, else (a saved report read back) the line's now.
    pub fn loads_per(&self, model: &Model) -> ohl_line::LoadsPer {
        self.from
            .map_or(model.line.solver.loads_per, |r| r.loads_per)
    }

    /// What of `uses` changed since the result was computed (`None`:
    /// nothing, or no result).
    pub fn out_of_date(&self, model: &Model, uses: Uses) -> Option<String> {
        self.from?.changed(&model.revision(), uses)
    }
}

/// The revision a saved run shown is tracked from: the model's now, with
/// the run's own calculation option of the ice and wind when it recorded
/// one (`loads_per`, 2026-10-21) — so that it is saved again as it was
/// computed.
pub fn saved_revision(parameters: &serde_json::Value, model: &Model) -> Revision {
    let mut now = model.revision();
    if let Some(p) = parameters
        .get("loads_per")
        .and_then(serde_json::Value::as_str)
        .and_then(ohl_line::LoadsPer::parse)
    {
        now.loads_per = p;
    }
    now
}

/// A saved run's calculation option of the ice and wind (its parameter
/// `loads_per`, 2026-10-21) when it differs from the line's now, in words
/// as [`Revision::changed`] says it; `None` when the same, or not
/// recorded (runs saved before).
pub fn saved_loads_per_changed(parameters: &serde_json::Value, model: &Model) -> Option<String> {
    let saved = parameters
        .get("loads_per")
        .and_then(serde_json::Value::as_str)
        .and_then(ohl_line::LoadsPer::parse)?;
    let now = model.line.solver.loads_per;
    (saved != now).then(|| format!("the calculation options (now {})", now.describe()))
}

/// The banner of a result out of date: what changed since it was computed,
/// and **Run again** (greyed while `can_run` is false, e.g. a run under
/// way). True when Run again was clicked.
pub fn banner(ui: &mut egui::Ui, what: &str, can_run: bool) -> bool {
    let warn = ui.visuals().warn_fg_color;
    let mut again = false;
    egui::Frame::new()
        .fill(warn.gamma_multiply(0.12))
        .stroke(egui::Stroke::new(1.0, warn.gamma_multiply(0.6)))
        .inner_margin(egui::Margin::symmetric(6, 4))
        .corner_radius(3.0)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(
                    warn,
                    egui::RichText::new(format!(
                        "Out of date: {what} changed since this run. The results below are \
                         those of the run (layers drawn from them are dimmed)."
                    ))
                    .strong(),
                );
                again = ui
                    .add_enabled(can_run, egui::Button::new("Run again"))
                    .on_hover_text("Compute it again from the model as it is now")
                    .clicked();
            });
        });
    again
}

/// A colour of a layer drawn from a result out of date: greyed and
/// faded (unchanged when `stale` is false).
pub fn dimmed(c: [u8; 4], stale: bool) -> [u8; 4] {
    if !stale {
        return c;
    }
    let grey = |v: u8| ((u16::from(v) + 140) / 2) as u8;
    [
        grey(c[0]),
        grey(c[1]),
        grey(c[2]),
        (u16::from(c[3]) * 2 / 5) as u8,
    ]
}

/// [`dimmed`] for egui.
pub fn dimmed32(c: egui::Color32, stale: bool) -> egui::Color32 {
    let [r, g, b, a] = dimmed([c.r(), c.g(), c.b(), c.a()], stale);
    egui::Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// Every frame: the layers drawn from results (vegetation and danger
/// trees with the isoclearance lines, the clearance violations of a
/// clearance window or of the Clearances tab, the structures by usage)
/// dimmed or marked while their result is out of date.
pub fn frame(r: &mut crate::app::Ready) {
    use crate::scene::Layer;
    let veg = r.vegetation.out_of_date(&r.model).is_some();
    if r.view3d.vegetation.stale != veg {
        r.view3d.vegetation.stale = veg;
        r.plan.vegetation.stale = veg;
        r.profile.vegetation.stale = veg;
        r.view3d
            .dirty
            .extend([Layer::GrowIn, Layer::FallIn, Layer::Isoclearance]);
    }
    let safety = r.safety_volume.out_of_date(&r.model).is_some();
    if r.view3d.safety_volume.stale != safety {
        r.view3d.safety_volume.stale = safety;
        r.plan.safety_volume.stale = safety;
        r.view3d.dirty.insert(Layer::SafetyVolume);
    }
    let marks = match r.marks_from {
        crate::app::MarksFrom::Clearances => {
            r.clearances.stale && !r.clearances.running() && r.clearances.report.is_some()
        }
        crate::app::MarksFrom::Zone(ohl_clearance::Measure::Vertical) => {
            r.vertical_clearances.out_of_date(&r.model).is_some()
        }
        crate::app::MarksFrom::Zone(ohl_clearance::Measure::Radial) => {
            r.radial_clearances.out_of_date(&r.model).is_some()
        }
        crate::app::MarksFrom::Tensions => r.wire_tensions.out_of_date(&r.model).is_some(),
        crate::app::MarksFrom::Strings => r.string_checks.out_of_date(&r.model).is_some(),
    };
    if r.view3d.marks_stale != marks {
        r.view3d.marks_stale = marks;
        r.plan.marks_stale = marks;
        r.profile.marks_stale = marks;
        r.view3d.dirty.insert(Layer::Clearances);
    }
    r.model.usage_out_of_date =
        !r.model.usage_marks.is_empty() && r.loads.out_of_date(&r.model).is_some();
}

/// A hasher fed by `Debug` formatting.
struct HashWriter<'a>(&'a mut std::collections::hash_map::DefaultHasher);

impl std::fmt::Write for HashWriter<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.write(s.as_bytes());
        Ok(())
    }
}

/// A hash of `value` as `Debug` writes it (every field; maps in their
/// order), the same in every run of the program.
pub fn hash_debug(value: &impl std::fmt::Debug) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let _ = std::fmt::write(&mut HashWriter(&mut h), format_args!("{value:?}"));
    h.finish()
}

/// A hash of `value` written as JSON (what it holds, as [`hash_debug`]
/// hashes it, faster for a whole line).
pub fn hash_json(value: &(impl serde::Serialize + std::fmt::Debug)) -> u64 {
    struct W(std::collections::hash_map::DefaultHasher);
    impl std::io::Write for W {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            std::hash::Hasher::write(&mut self.0, b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut w = W(Default::default());
    // (A value that cannot be written: its Debug text instead.)
    if serde_json::to_writer(&mut w, value).is_err() {
        return hash_debug(value);
    }
    std::hash::Hasher::finish(&w.0)
}

/// A hash of a stored point of cloud `cloud` (its part of the cloud's
/// hash).
pub fn point_hash(cloud: &str, p: &ohl_pointcloud::edit::StoredPoint) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    cloud.hash(&mut h);
    p.hash(&mut h);
    h.finish()
}

/// A number no hash of ground points gives in practice: another ground
/// surface used.
pub fn fresh() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(1);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    ("another surface", N.fetch_add(1, Ordering::Relaxed)).hash(&mut h);
    h.finish()
}

/// The revisions a model keeps ([`Model::revision`]).
#[derive(Debug, Default)]
pub struct Revisions {
    /// The point cloud's points (a sum of point hashes, wrapping; 0: as
    /// opened).
    pub points: u64,
    /// The same of the points of the ground classes.
    pub ground_points: u64,
    /// The ground surface.
    pub ground: u64,
    /// Counts the times the library files may have changed (the hash of
    /// their texts is taken again then).
    pub libraries: u64,
    /// The line's hash, by the line's version.
    line_cache: Cell<Option<(u64, u64)>>,
    /// The library files' hash, by `libraries`.
    library_cache: Cell<Option<(u64, u64)>>,
}

impl Revisions {
    /// Points of cloud `cloud` changed: `from` became `to` (`ground`: a
    /// class has the role ground).
    pub fn point_changed(
        &mut self,
        cloud: &str,
        from: &ohl_pointcloud::edit::StoredPoint,
        to: &ohl_pointcloud::edit::StoredPoint,
        ground: impl Fn(u8) -> bool,
    ) {
        let (a, b) = (point_hash(cloud, from), point_hash(cloud, to));
        self.points = self.points.wrapping_sub(a).wrapping_add(b);
        if ground(from.class) {
            self.ground_points = self.ground_points.wrapping_sub(a);
        }
        if ground(to.class) {
            self.ground_points = self.ground_points.wrapping_add(b);
        }
    }
}

impl Model {
    /// The revision of what results are computed from, now. Cost: a hash
    /// of the line when it changed since the last call (a few ms for a
    /// long line), of the library files when they may have changed.
    pub fn revision(&self) -> Revision {
        let r = &self.revisions;
        let line = match r.line_cache.get() {
            Some((v, h)) if v == self.line_version => h,
            _ => {
                // The wind side shown is a view setting.
                let cases: Vec<_> = self
                    .cases
                    .iter()
                    .map(|c| {
                        let mut case = c.case.clone();
                        case.wind_from = ohl_weather::WindSide::Left;
                        case
                    })
                    .collect();
                // The line as stored (its structures' types are in the
                // library files, hashed with them below): the same
                // content as the line resolved, much less to write out.
                let mut l = self.stored_line().unwrap_or(&self.line).clone();
                l.set_weather_cases(&cases);
                // Said on its own (Revision::loads_per).
                l.solver.loads_per = ohl_line::LoadsPer::default();
                // The line as JSON (written out faster than as text: no
                // field names twice, numbers by the fast writer), the
                // wire library and the cases as text.
                let h = hash_debug(&(hash_json(&l), &self.library, &cases));
                r.line_cache.set(Some((self.line_version, h)));
                h
            }
        };
        let libraries = match r.library_cache.get() {
            Some((v, h)) if v == r.libraries => h,
            _ => {
                let mut texts = Vec::new();
                if let Some(s) = self.session() {
                    for (kind, _) in crate::model::PROJECT_LIBRARIES {
                        texts.push((kind, s.library_files(kind).unwrap_or_default()));
                    }
                }
                let h = hash_debug(&texts);
                r.library_cache.set(Some((r.libraries, h)));
                h
            }
        };
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (line, libraries).hash(&mut h);
        Revision {
            line: h.finish(),
            points: r.points,
            ground: r.ground,
            loads_per: self.line.solver.loads_per,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_changed_in_words() {
        let a = Revision {
            line: 1,
            points: 2,
            ground: 3,
            loads_per: ohl_line::LoadsPer::Stretched,
        };
        assert_eq!(a.changed(&a, Uses::ALL), None);
        let b = Revision { points: 5, ..a };
        assert_eq!(
            a.changed(&b, Uses::ALL).as_deref(),
            Some("the LiDAR points")
        );
        assert_eq!(a.changed(&b, Uses::LINE), None, "not used");
        let c = Revision {
            line: 9,
            points: 5,
            ground: 7,
            ..a
        };
        assert_eq!(
            a.changed(&c, Uses::ALL).as_deref(),
            Some("the line, the LiDAR points and the ground")
        );
        assert_eq!(
            a.changed(&c, Uses::LINE_POINTS).as_deref(),
            Some("the line and the LiDAR points")
        );
        // The ice and wind per unstretched metre (2026-10-21): said on
        // its own, with the option now.
        let d = Revision {
            loads_per: ohl_line::LoadsPer::Unstretched,
            ..a
        };
        let said = a.changed(&d, Uses::LINE).unwrap();
        assert!(
            said.starts_with("the calculation options (now ice and wind per unstretched metre"),
            "{said}"
        );
        assert_eq!(
            a.changed(
                &d,
                Uses {
                    line: false,
                    ..Uses::ALL
                }
            ),
            None
        );
        // A stamp: the revision at the start, kept with the result.
        let mut s = Stamp::default();
        s.started(a);
        assert_eq!(s.from(), None);
        s.arrived();
        assert_eq!(s.from(), Some(a));
        s.arrived();
        assert_eq!(s.from(), Some(a));
    }

    #[test]
    fn the_points_hash_is_a_set() {
        use ohl_pointcloud::edit::StoredPoint;
        let p = |z: i64, class: u8| StoredPoint {
            xyz: [1, 2, z],
            class,
            intensity: 0,
            returns: 0,
        };
        let ground = |c: u8| c == 2;
        let mut r = Revisions::default();
        r.point_changed("c", &p(5, 2), &p(7, 2), ground);
        r.point_changed("c", &p(8, 2), &p(8, 5), ground);
        assert_ne!(r.points, 0);
        assert_ne!(r.ground_points, 0);
        // Back, in another order: as before.
        r.point_changed("c", &p(7, 2), &p(5, 2), ground);
        r.point_changed("c", &p(8, 5), &p(8, 2), ground);
        assert_eq!((r.points, r.ground_points), (0, 0));
    }
}
