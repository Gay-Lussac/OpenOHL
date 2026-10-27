//! Towers in the project and in the import (owner, 2026-10-27): kept once
//! as the project's — its id, position, orientation and type — and
//! compared set by set, each set of the import added, left out, or (at
//! another place than the project's set of that number) kept as the
//! project's, taken from the import or added under a new number; or (the
//! default in Replace mode) taken as the imported tower, the project's
//! sets it lacks added back where what the line keeps hangs on them.
//!
//! The sets are compared as the towers are in effect: each line resolved
//! with its own library files (a set may live in the tower's type), the
//! wire points' world positions in still air.

use std::collections::{BTreeMap, BTreeSet};

use ohl_line::{InsulatorString, Line, Point, Structure, StructurePoint, WirePoint};
use ohl_structures::{InsulatorLibrary, LibraryItem, StructureLibrary, StructureType};

use super::Resolution;

/// What is done with one set of a tower in both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SetChoice {
    /// A set only in the import: added to the project's tower.
    Add,
    /// A set only in the import: left out (the imported sections and
    /// jumpers at it with it).
    LeaveOut,
    /// A set in both at other places: the project's kept, the imported
    /// sections at that set hang on it.
    KeepExisting,
    /// A set in both at other places: the imported one replaces the
    /// project's wire point and strings of that set.
    TakeImported,
    /// A set in both at other places: the imported one added under a free
    /// set number of the tower, the imported sections at it following.
    NewNumber,
}

impl SetChoice {
    /// In words, for the window.
    pub fn words(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::LeaveOut => "leave out",
            Self::KeepExisting => "keep the project's",
            Self::TakeImported => "take the imported one",
            Self::NewNumber => "add the imported one under a new number",
        }
    }

    /// The word of the command line (`--choose KEY=add`).
    pub fn word(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::LeaveOut => "leave",
            Self::KeepExisting => "keep",
            Self::TakeImported => "imported",
            Self::NewNumber => "new",
        }
    }
}

/// How a set of a tower in both stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetState {
    /// Only the project's tower has it.
    OnlyExisting,
    /// Only the imported tower has it.
    OnlyImported,
    /// Both, at the same place (within the tolerance): the same attachment.
    Same,
    /// Both, at other places.
    Differs,
}

/// One set number of a tower in both.
#[derive(Debug, Clone, PartialEq)]
pub struct SharedSet {
    /// The set number (as both files number it).
    pub set: u32,
    /// Key the choice is given by: `tower:<imported id>:set:<n>`.
    pub key: String,
    /// The project's wire point of the set, in still air, world, m.
    pub existing: Option<Point>,
    /// The imported one, m.
    pub imported: Option<Point>,
    /// How it stands.
    pub state: SetState,
    /// What is done (none: nothing to choose).
    pub choice: Option<SetChoice>,
    /// The imported set's number on the project's tower when added under
    /// a new number.
    pub new_set: Option<u32>,
    /// A set only the project's has, of a tower taken from the import:
    /// what still hangs on it (`section S4`, `jumper J2`), so it is kept
    /// on the imported tower; none: it goes with the project's tower.
    pub kept_for: Vec<String>,
}

impl SharedSet {
    /// The distance between the two wire points, m (both there).
    pub fn distance(&self) -> Option<f64> {
        Some(distance(self.existing?, self.imported?))
    }

    /// The choices it has, the default first.
    pub fn choices(&self) -> &'static [SetChoice] {
        match self.state {
            SetState::OnlyImported => &[SetChoice::Add, SetChoice::LeaveOut],
            SetState::Differs => &[
                SetChoice::KeepExisting,
                SetChoice::TakeImported,
                SetChoice::NewNumber,
            ],
            SetState::OnlyExisting | SetState::Same => &[],
        }
    }
}

/// A tower in the project and in the import: kept once — as the
/// project's, its sets settled one by one, or as the imported one, the
/// project's sets it lacks kept where what stays hangs on them.
#[derive(Debug, Clone, PartialEq)]
pub struct SharedTower {
    /// `tower:<imported id>`: the choice of the tower
    /// ([`Resolution::KeepExisting`] or [`Resolution::TakeImported`]).
    pub key: String,
    /// The project's tower kept, or the imported one taken.
    pub resolution: Resolution,
    /// The project's id (kept).
    pub existing: String,
    /// The imported one's id.
    pub imported: String,
    /// How far apart they stand in plan, m.
    pub distance: f64,
    /// Every set number of either, in order.
    pub sets: Vec<SharedSet>,
}

impl SharedTower {
    /// One line: the sets of each, what is added, what differs.
    pub fn summary(&self) -> String {
        let n = |f: &dyn Fn(&SharedSet) -> bool| self.sets.iter().filter(|s| f(s)).count();
        let ours = n(&|s| s.existing.is_some());
        let theirs = n(&|s| s.imported.is_some());
        let same = n(&|s| s.state == SetState::Same);
        let added = n(&|s| {
            matches!(
                s.choice,
                Some(SetChoice::Add | SetChoice::NewNumber | SetChoice::TakeImported)
            )
        });
        let differ = n(&|s| s.state == SetState::Differs);
        let name = if self.existing == self.imported {
            self.existing.clone()
        } else {
            format!("{} (imported {})", self.existing, self.imported)
        };
        if self.resolution == Resolution::TakeImported {
            let kept = n(&|s| s.state == SetState::OnlyExisting && !s.kept_for.is_empty());
            let gone = n(&|s| s.state == SetState::OnlyExisting && s.kept_for.is_empty());
            return format!(
                "{name}: the imported tower ({theirs} set{}), {ours} in the project's, {same} the \
                 same, {differ} at other places; of the project's own {kept} kept (what stays \
                 hangs on them), {gone} gone",
                if theirs == 1 { "" } else { "s" }
            );
        }
        format!(
            "{name}: {ours} set{} in the project's, {theirs} in the imported, {same} the same, \
             {differ} at other places, {added} taken from the import",
            if ours == 1 { "" } else { "s" }
        )
    }
}

fn distance(a: Point, b: Point) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The structure types and insulator assemblies of library files (kind,
/// name, text) that read; the types' files by type id.
pub(super) fn libraries(
    libs: &[(String, String, String)],
) -> (
    StructureLibrary,
    InsulatorLibrary,
    BTreeMap<String, (String, String)>,
) {
    let mut types = StructureLibrary::default();
    let mut insulators = InsulatorLibrary::default();
    let mut files = BTreeMap::new();
    for (kind, name, text) in libs {
        let path = std::path::Path::new(name);
        match kind.as_str() {
            "structures" if name.ends_with(".ohls") => {
                if let Ok(t) = StructureType::parse(path, text) {
                    files.insert(t.id.clone(), (name.clone(), text.clone()));
                    let _ = types.insert(path, t);
                }
            }
            "insulators" => {
                if let Ok(i) = ohl_structures::InsulatorAssembly::parse(path, text) {
                    let _ = insulators.insert(path, i);
                }
            }
            _ => {}
        }
    }
    (types, insulators, files)
}

/// The structures of `line` as they are in effect (typed ones merged with
/// their types from `libs`), by id: the line resolved, else each
/// structure alone, else as stored.
pub(super) fn effective(
    line: &Line,
    libs: &[(String, String, String)],
) -> BTreeMap<String, Structure> {
    let (types, insulators, _) = libraries(libs);
    if let Ok(r) = line.resolve(&types, &insulators) {
        return r
            .structures
            .into_iter()
            .map(|s| (s.id.clone(), s))
            .collect();
    }
    line.structures
        .iter()
        .map(|s| {
            let mut one = line.clone();
            one.structures = vec![s.clone()];
            one.sections.clear();
            one.jumpers.clear();
            one.spacers.clear();
            one.circuits.clear();
            one.systems.clear();
            one.alignments.clear();
            one.alignment_points.clear();
            one.routes.clear();
            let r = one
                .resolve(&types, &insulators)
                .ok()
                .and_then(|r| r.structures.into_iter().next());
            (s.id.clone(), r.unwrap_or_else(|| s.clone()))
        })
        .collect()
}

/// The sets of the project's tower `ours` and the imported `theirs` (both
/// in effect) compared, the choices given by `choose` (else the
/// defaults).
pub(super) fn compare(
    ours: &Structure,
    theirs: &Structure,
    tolerance: f64,
    choose: impl Fn(&str) -> Option<SetChoice>,
) -> SharedTower {
    let sets: BTreeSet<u32> = ours.sets().chain(theirs.sets()).collect();
    let mut free = sets.iter().max().copied().unwrap_or(0) + 1;
    let at = |s: &Structure, n: u32| s.wire_at_set(n).map(|w| w.position);
    let sets = sets
        .into_iter()
        .map(|n| {
            let (e, i) = (at(ours, n), at(theirs, n));
            let state = match (e, i) {
                (Some(_), None) => SetState::OnlyExisting,
                (None, _) => SetState::OnlyImported,
                (Some(a), Some(b)) if distance(a, b) <= tolerance => SetState::Same,
                _ => SetState::Differs,
            };
            let key = format!("tower:{}:set:{n}", theirs.id);
            let mut set = SharedSet {
                set: n,
                key,
                existing: e,
                imported: i,
                state,
                choice: None,
                new_set: None,
                kept_for: Vec::new(),
            };
            let choices = set.choices();
            if let Some(first) = choices.first() {
                let c = choose(&set.key)
                    .filter(|c| choices.contains(c))
                    .unwrap_or(*first);
                set.choice = Some(c);
                if c == SetChoice::NewNumber {
                    set.new_set = Some(free);
                    free += 1;
                }
            }
            set
        })
        .collect();
    SharedTower {
        key: format!("tower:{}", theirs.id),
        resolution: Resolution::KeepExisting,
        existing: ours.id.clone(),
        imported: theirs.id.clone(),
        distance: (ours.position[0] - theirs.position[0])
            .hypot(ours.position[1] - theirs.position[1]),
        sets,
    }
}

/// `label`, else `label-2`, `label-3`… not in `taken`; then taken.
fn unique(label: &str, taken: &mut BTreeSet<String>) -> String {
    let l = if taken.contains(label) {
        (2..)
            .map(|k| format!("{label}-{k}"))
            .find(|c| !taken.contains(c))
            .expect("a label is free")
    } else {
        label.to_owned()
    };
    taken.insert(l.clone());
    l
}

/// A string's direction in plan of a structure turned `from` degrees in
/// the axes of one turned `to` degrees (the same direction in the world).
fn turned(direction_deg: f64, from: f64, to: f64) -> f64 {
    let d = direction_deg.to_radians();
    // (right, ahead) of the direction: 0° ahead, 90° to the right.
    let local = [d.sin(), d.cos(), 0.0];
    let world = ohl_line::links::from_structure_axes(from, local);
    let back = ohl_line::links::to_structure_axes(to, world);
    back[0].atan2(back[1]).to_degrees().rem_euclid(360.0)
}

/// The project's tower `stored` (as stored; `ours` in effect) with the
/// sets of the imported `theirs` (in effect) as `tower` settles them: a
/// set added (or taken, or added under a new number) brings its wire
/// point and the strings above it, up to the structure points (a point of
/// the project's tower at the same place used instead of a new one), as
/// the structure's own items, labels made unique on the tower, string
/// directions turned into the project tower's axes. Returns notes.
pub(super) fn apply(
    tower: &SharedTower,
    stored: &mut Structure,
    ours: &Structure,
    theirs: &Structure,
) -> Vec<String> {
    let mut notes = Vec::new();
    // Labels of points and wire points (one namespace), of strings.
    let mut nodes: BTreeSet<String> = ours
        .points
        .iter()
        .map(|p| p.label.clone())
        .chain(ours.wires.iter().map(|w| w.label.clone()))
        .chain(stored.points.iter().map(|p| p.label.clone()))
        .chain(stored.wires.iter().map(|w| w.label.clone()))
        .collect();
    let mut strings: BTreeSet<String> = ours
        .strings
        .iter()
        .chain(&stored.strings)
        .map(|s| s.label.clone())
        .collect();
    // The project's own items of a set the imported one replaces.
    for s in tower
        .sets
        .iter()
        .filter(|s| s.choice == Some(SetChoice::TakeImported))
    {
        if let Some(label) = stored.wire_at_set(s.set).map(|w| w.label.clone()) {
            remove_component(stored, &label);
        }
    }
    // The set numbers the imported wire points get (none: not taken).
    let number = |n: u32| -> Option<u32> {
        let s = tower.sets.iter().find(|s| s.set == n)?;
        match s.choice? {
            SetChoice::Add | SetChoice::TakeImported => Some(n),
            SetChoice::NewNumber => s.new_set,
            SetChoice::LeaveOut | SetChoice::KeepExisting => None,
        }
    };
    let mut done: BTreeSet<String> = BTreeSet::new();
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    for s in &tower.sets {
        let Some(to) = number(s.set) else { continue };
        let Some(w) = theirs.wire_at_set(s.set) else {
            continue;
        };
        if done.contains(&w.label) {
            continue;
        }
        let c = theirs.component(&w.label);
        for &k in &c.wires {
            let x = &theirs.wires[k];
            done.insert(x.label.clone());
            let set = match x.set {
                Some(n) if n == s.set => Some(to),
                Some(n) => {
                    let other = number(n);
                    if other.is_none() {
                        notes.push(format!(
                            "{} set {n}: hangs with set {} on the imported tower; added without its set",
                            tower.existing, s.set
                        ));
                    }
                    other
                }
                None => None,
            };
            let label = unique(&x.label, &mut nodes);
            map.insert(x.label.clone(), label.clone());
            stored.wires.push(WirePoint {
                label,
                set,
                ..x.clone()
            });
        }
        for &k in &c.strings {
            let x = &theirs.strings[k];
            // A structure point at its top: the project's at that place,
            // else the imported one added.
            if !map.contains_key(&x.top)
                && let Some(p) = theirs.point(&x.top)
            {
                let near = ours
                    .points
                    .iter()
                    .find(|q| distance(q.position, p.position) < 0.01)
                    .map(|q| q.label.clone());
                let label = match near {
                    Some(l) => l,
                    None => {
                        let label = unique(&p.label, &mut nodes);
                        stored.points.push(StructurePoint {
                            label: label.clone(),
                            joint: None,
                            offset: [0.0; 3],
                            unlinked: false,
                            ..p.clone()
                        });
                        label
                    }
                };
                map.insert(x.top.clone(), label);
            }
            let label = unique(&x.label, &mut strings);
            stored.strings.push(InsulatorString {
                label,
                top: map.get(&x.top).cloned().unwrap_or_else(|| x.top.clone()),
                wire: map.get(&x.wire).cloned().unwrap_or_else(|| x.wire.clone()),
                direction_deg: turned(
                    x.direction_deg,
                    theirs.orientation_deg,
                    stored.orientation_deg,
                ),
                ..x.clone()
            });
        }
    }
    notes
}

/// The structure's own wire point `label` and what moves with it (its
/// own strings and wire points) taken away.
fn remove_component(s: &mut Structure, label: &str) {
    let c = s.component(label);
    let wires: BTreeSet<usize> = c.wires.into_iter().collect();
    let strings: BTreeSet<usize> = c.strings.into_iter().collect();
    let mut k = 0;
    s.wires.retain(|_| {
        k += 1;
        !wires.contains(&(k - 1))
    });
    let mut k = 0;
    s.strings.retain(|_| {
        k += 1;
        !strings.contains(&(k - 1))
    });
}

/// Every `{"structure": structure, "set": from}` in `v` made `set: to`
/// (section points, jumper ends, whatever names a set).
pub(super) fn renumber(v: &mut serde_json::Value, structure: &str, from: u32, to: u32) {
    match v {
        serde_json::Value::Object(m) => {
            if m.get("structure").and_then(|x| x.as_str()) == Some(structure)
                && m.get("set").and_then(|x| x.as_u64()) == Some(u64::from(from))
            {
                m.insert("set".into(), to.into());
            }
            for x in m.values_mut() {
                renumber(x, structure, from, to);
            }
        }
        serde_json::Value::Array(a) => {
            for x in a {
                renumber(x, structure, from, to);
            }
        }
        _ => {}
    }
}

/// The sections and jumpers of `line` at the sets `gone` (structure,
/// set) left out, with the spacers on those sections; their ids.
pub(super) fn leave_out(line: &mut Line, gone: &BTreeSet<(String, u32)>) -> Vec<String> {
    let at = |p: &ohl_line::SectionPoint| gone.contains(&(p.structure.clone(), p.set));
    let mut out = Vec::new();
    let sections: BTreeSet<String> = line
        .sections
        .iter()
        .filter(|s| s.points.iter().any(at))
        .map(|s| s.id.clone())
        .collect();
    line.sections.retain(|s| !sections.contains(&s.id));
    out.extend(sections.iter().cloned());
    line.jumpers.retain(|j| {
        let off = [&j.from, &j.to].iter().any(|e| e.point().is_some_and(at));
        if off {
            out.push(j.id.clone());
        }
        !off
    });
    line.spacers
        .retain(|s| !sections.contains(&s.a.section) && !sections.contains(&s.b.section));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A direction kept in the world when the axes turn: a structure
    /// turned half round sees it from the other side.
    #[test]
    fn directions_turn_with_the_axes() {
        assert!((turned(274.76, 94.76, -85.24) - 94.76).abs() < 1e-9);
        assert!((turned(10.0, 30.0, 30.0) - 10.0).abs() < 1e-9);
        // Turned 90° counter-clockwise: what was ahead is now to the left.
        assert!((turned(0.0, 90.0, 0.0) - 270.0).abs() < 1e-9);
    }
}
