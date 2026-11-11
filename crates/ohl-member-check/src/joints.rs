// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The joints of a structure whose member check is a plugin's method
//! (2026-11-11; built, in active development, not approved yet): the
//! plates', covers', welds' and bolt groups' checks by the plugin's
//! optional `joint-check` export, and the bolt tools' minimum distances
//! by its optional `bolt-distances` export.
//!
//! - **Never another code in its place.** A plugin without the
//!   `joint-check` export, or one that fails: every joint part is
//!   [`not_checked_by`] the plugin, without a usage. A plugin without
//!   `bolt-distances`: no minimum distances, said ([`bolt_rules`]'s
//!   error) — not ASCE 10-97's, not SIA 263's.
//! - **One call per structure** for the joint parts, with every load
//!   case ([`check_joints`]).
//! - **Where the plugin is.** A model built from a file's text alone does
//!   not know its folder: the plugin is looked for where structures were
//!   opened or run in this session ([`crate::roots`] remembers the
//!   folders), then in the user's plugin folder ([`installed`]).
//! - The minimum distances are asked once per different bolt and part in
//!   a session (kept: the bolt tools ask at every redraw).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use ohl_plugins::bolt_distances as dist;
pub use ohl_plugins::bolt_distances::DistanceInput;
use ohl_plugins::joint_check as wit;
pub use ohl_plugins::joint_check::{JointInput, LoadCase, Part, PartResult};
use ohl_structures::connections::WeldKind;
use ohl_structures_fe::Model;
use ohl_structures_fe::bolt_rules::{Basis, BoltRules};
use ohl_structures_fe::model::PluginMethod;
use ohl_structures_fe::plates::{BoltGroup, PlateSteel, WeldInput};

/// Said for a joint part the plugin `p` names did not check: "NOT CHECKED
/// by the plugin, NOT ON THE SAFE SIDE".
pub fn not_checked_by(p: &PluginMethod) -> String {
    format!("NOT CHECKED by {}, NOT ON THE SAFE SIDE", p.plugin)
}

type Found = HashMap<(String, String, usize), Result<PathBuf, String>>;

static FOUND: Mutex<Option<Found>> = Mutex::new(None);

/// Where the plugin `p` names is installed: where its runner found it
/// ([`crate::find`]), else looked for in `Plugins/` beside the structures
/// opened or run in this session and in the user's plugin folder (once
/// per plugin and set of folders). Err: [`crate::resolve`]'s message.
pub fn installed(p: &PluginMethod) -> Result<PathBuf, String> {
    if let Some(dir) = &p.dir {
        return Ok(dir.clone());
    }
    let roots = crate::session_roots();
    let key = (p.plugin.clone(), p.method.clone(), roots.len());
    let mut found = FOUND
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    found
        .get_or_insert_with(HashMap::new)
        .entry(key)
        .or_insert_with(|| crate::resolve(p, &roots))
        .clone()
}

/// A bolt group as the interface takes it.
pub fn bolt_group(g: &BoltGroup) -> wit::BoltGroup {
    wit::BoltGroup {
        count: g.count,
        diameter: g.diameter,
        hole: g.hole,
        bolt_ultimate: g.bolt_ultimate,
        lines: g.lines,
        per_line: g.per_line,
        pitch: g.pitch,
        span: g.span,
        end_tension: g.end[0],
        end_compression: g.end[1],
        edge: g.edge,
        width: g.width,
        width_note: g.width_words.clone(),
    }
}

/// A plate's steel as the interface takes it.
pub fn plate_steel(s: &PlateSteel) -> wit::PlateSteel {
    wit::PlateSteel {
        thickness: s.thickness,
        fy: s.fy,
        fu: s.fu,
    }
}

/// A weld as the interface takes it.
pub fn weld(w: &WeldInput) -> wit::Weld {
    wit::Weld {
        kind: match w.kind {
            WeldKind::Fillet => "fillet",
            WeldKind::FullPenetration => "full_penetration",
        }
        .to_owned(),
        throat: w.size,
        length: w.length,
        length_given: w.length_given,
        angle: w.angle,
        fy: w.fy,
        thicknesses: w.thickness.to_vec(),
    }
}

/// What a plugin's joint check gave.
#[derive(Debug, Clone)]
pub enum JointsChecked {
    /// The plugin is not found, has no `joint-check` export, or failed:
    /// why, in words — every part is then [`not_checked_by`] it.
    NotChecked(String),
    /// Its results, by part number.
    Results(Vec<wit::PartResult>),
}

static JOINT_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// How many times [`check_joints`] called a plugin's `check-joints` in
/// this process (the tests of "one call per structure").
pub fn joint_calls() -> usize {
    JOINT_CALLS.load(std::sync::atomic::Ordering::SeqCst)
}

/// The joint parts of `input` checked by the plugin `p` names: one call
/// with every part and load case.
pub fn check_joints(p: &PluginMethod, input: &wit::JointInput) -> JointsChecked {
    let dir = match installed(p) {
        Ok(d) => d,
        Err(e) => return JointsChecked::NotChecked(e),
    };
    let host = match ohl_plugins::WasmHost::new() {
        Ok(h) => h,
        Err(e) => return JointsChecked::NotChecked(format!("the plugin host does not start: {e}")),
    };
    JOINT_CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    match host.check_joints(&dir, &p.method, input) {
        Ok(Some(r)) => JointsChecked::Results(r),
        Ok(None) => JointsChecked::NotChecked(format!(
            "the plugin {} has no joint check (no joint-check export)",
            p.plugin
        )),
        Err(e) => JointsChecked::NotChecked(format!("the joint check failed: {e}")),
    }
}

/// The minimum distances of one bolt, by bits of the input (kept for the
/// session).
type Distances =
    HashMap<(PathBuf, String, [u64; 4], bool, bool), Result<Option<dist::Distances>, String>>;

static DISTANCES: Mutex<Option<Distances>> = Mutex::new(None);

/// The plugin's minimum distances for `input`. Err: why there are none —
/// "no minimum distances from the plugin: …".
pub fn minimum_distances(
    p: &PluginMethod,
    input: &dist::DistanceInput,
) -> Result<dist::Distances, String> {
    let none = |why: String| format!("no minimum distances from {}: {why}", p.plugin);
    let dir = installed(p).map_err(none)?;
    let key = (
        dir.clone(),
        p.method.clone(),
        [
            input.diameter.to_bits(),
            input.thickness.to_bits(),
            input.ultimate.map_or(u64::MAX, f64::to_bits),
            input.bolt_force.map_or(u64::MAX, f64::to_bits),
        ],
        input.sheared_edge,
        input.redundant,
    );
    let mut kept = DISTANCES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let given = kept
        .get_or_insert_with(HashMap::new)
        .entry(key)
        .or_insert_with(|| {
            let host = ohl_plugins::WasmHost::new()
                .map_err(|e| format!("the plugin host does not start: {e}"))?;
            match host.minimum_distances(&dir, &p.method, input) {
                Ok(Some(d)) => Ok(d),
                Ok(None) => Err("the plugin has no bolt-distances export".to_owned()),
                Err(e) => Err(e.to_string()),
            }
        })
        .clone();
    given
        .map_err(none)?
        .ok_or_else(|| none("its method gives none for this bolt".to_owned()))
}

/// The minimum distances at member `idx`'s end `end` of `model` (whose
/// file names a plugin's method) from that plugin, as the bolt tools take
/// them (arguments as [`ohl_structures_fe::bolt_rules::rules_in`]). Err:
/// the words to show instead — "no minimum distances from the plugin: …" —
/// never another code's values. None: the model names no plugin, or the
/// end has no bolts of known diameter.
pub fn bolt_rules(
    model: &Model,
    idx: usize,
    end: usize,
    force: Option<f64>,
    part: Option<(f64, Option<f64>)>,
) -> Option<Result<BoltRules, String>> {
    let p = model.capacity_plugin.as_ref()?;
    let Basis {
        diameter,
        thickness,
        ultimate,
        force,
        leg,
        root,
        sheared,
        redundant,
    } = ohl_structures_fe::bolt_rules::basis(model, idx, end, force, part)?;
    let input = dist::DistanceInput {
        diameter,
        thickness,
        ultimate,
        bolt_force: force.value(),
        sheared_edge: sheared,
        redundant,
    };
    Some(minimum_distances(p, &input).map(|d| {
        let mut clauses = d.clauses.into_iter();
        let mut next = || clauses.next().unwrap_or_default();
        BoltRules {
            code: format!("{} of the plugin {}", p.method, p.plugin),
            end: d.end,
            spacing: d.spacing,
            edge: d.edge,
            clauses: [next(), next(), next()],
            force,
            diameter,
            leg,
            root,
            sheared,
        }
    }))
}
