// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A solve's lookups (roadmap O42, owner 2026-11-19): what a solve of a
//! line looks up in it again and again — a structure, section or jumper by
//! its id, the sections at a wire point or through a structure, the line's
//! route views ([`Line::alignment_views`]), the jumpers' estimated loads
//! in a weather case ([`crate::jumper::end_loads`]) — **worked out once
//! while the solve runs** instead of searched through the whole line for
//! every section and jumper it solves.
//!
//! [`indexed`] runs a closure with the lookups of a line: inside it, the
//! line's own methods ([`Line::structure`], [`Line::jumper_section`],
//! [`Line::wind_factor_at_structure`], …) answer from them. The answers
//! are the **same** as the searches give — the first object of an id in
//! the line's order, the sections in the line's order, the same route
//! views and loads, computed by the same code — only found faster, so
//! every result stays the same bit for bit (quality before speed). The
//! line cannot change meanwhile: it is borrowed for the whole closure, and
//! the lookups are found by the line's address only while it is.
//!
//! The lookups are kept per thread, as [`crate::jumper::with_threads`]
//! keeps the threads: a solve's worker threads (its sections and jumpers
//! solved side by side) take their caller's. Each piece is made when
//! first asked for. Another line (a copy, a part of it) has none: it is
//! searched as before.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use ohl_weather::WeatherCase;

use crate::route::AlignmentView;
use crate::solve::{JumperLoadMap, SectionShape};
use crate::{Line, SectionPoint};

thread_local! {
    /// The lookups open on this thread, innermost last.
    static OPEN: RefCell<Vec<Open>> = const { RefCell::new(Vec::new()) };
}

/// The lookups of one line (see the module).
#[derive(Debug)]
pub(crate) struct Lookups {
    /// The line's address (its identity while it is borrowed).
    line: usize,
    /// The wire library's address the solve runs with (0: none given):
    /// the jumpers' loads are kept for it only.
    library: usize,
    ids: OnceLock<Ids>,
    views: OnceLock<Arc<Vec<AlignmentView>>>,
    route_grid: OnceLock<crate::route::RouteGrid>,
    local_wind: OnceLock<bool>,
    end_loads: Mutex<Vec<(String, Arc<JumperLoadMap>)>>,
    end_loads_on: Mutex<Vec<(String, Arc<JumperLoadMap>)>>,
}

/// Where the line's objects are, by id and by wire point (indices in the
/// line's lists, in the line's order).
#[derive(Debug, Default)]
pub(crate) struct Ids {
    structures: HashMap<String, Vec<usize>>,
    sections: HashMap<String, Vec<usize>>,
    jumpers: HashMap<String, Vec<usize>>,
    /// The sections holding each wire point (each once).
    at_point: HashMap<SectionPoint, Vec<usize>>,
    /// The sections through each structure (each once).
    through: HashMap<String, Vec<usize>>,
}

impl Ids {
    fn of(line: &Line) -> Self {
        let mut ids = Self::default();
        for (k, s) in line.structures.iter().enumerate() {
            ids.structures.entry(s.id.clone()).or_default().push(k);
        }
        for (k, s) in line.sections.iter().enumerate() {
            ids.sections.entry(s.id.clone()).or_default().push(k);
            for p in &s.points {
                let v = ids.at_point.entry(p.clone()).or_default();
                if v.last() != Some(&k) && !v.contains(&k) {
                    v.push(k);
                }
                let v = ids.through.entry(p.structure.clone()).or_default();
                if v.last() != Some(&k) && !v.contains(&k) {
                    v.push(k);
                }
            }
        }
        for (k, j) in line.jumpers.iter().enumerate() {
            ids.jumpers.entry(j.id.clone()).or_default().push(k);
        }
        ids
    }

    /// Every structure of id `id`, in the line's order.
    pub(crate) fn structures(&self, id: &str) -> &[usize] {
        self.structures.get(id).map_or(&[], Vec::as_slice)
    }

    /// Every section of id `id`, in the line's order.
    pub(crate) fn sections(&self, id: &str) -> &[usize] {
        self.sections.get(id).map_or(&[], Vec::as_slice)
    }

    /// Every jumper of id `id`, in the line's order.
    pub(crate) fn jumpers(&self, id: &str) -> &[usize] {
        self.jumpers.get(id).map_or(&[], Vec::as_slice)
    }

    /// The sections holding wire point `p`, in the line's order.
    pub(crate) fn at_point(&self, p: &SectionPoint) -> &[usize] {
        self.at_point.get(p).map_or(&[], Vec::as_slice)
    }

    /// The sections through structure `id`, in the line's order.
    pub(crate) fn through(&self, id: &str) -> &[usize] {
        self.through.get(id).map_or(&[], Vec::as_slice)
    }
}

impl Lookups {
    /// Where the line's objects are.
    pub(crate) fn ids(&self, line: &Line) -> &Ids {
        self.ids.get_or_init(|| Ids::of(line))
    }

    /// The nearest route segment to `at` on the routes `views`
    /// ([`Line::nearest_on`]) found through the routes' grid, when `views`
    /// are these lookups' route views; none otherwise (searched then).
    pub(crate) fn nearest_on(
        &self,
        views: &[AlignmentView],
        at: [f64; 2],
    ) -> Option<Option<crate::route::OnRoute>> {
        let own = self.views.get()?;
        if !std::ptr::eq(own.as_slice(), views) {
            return None;
        }
        self.route_grid
            .get_or_init(|| crate::route::RouteGrid::of(own))
            .nearest(at)
    }

    /// The line's route views ([`Line::alignment_views`]).
    pub(crate) fn views(&self, line: &Line) -> Arc<Vec<AlignmentView>> {
        self.views
            .get_or_init(|| Arc::new(line.alignment_views_searched()))
            .clone()
    }

    /// Whether a section of the line has local winds.
    pub(crate) fn local_wind(&self, line: &Line) -> bool {
        *self
            .local_wind
            .get_or_init(|| line.sections.iter().any(crate::Section::has_local_wind))
    }

    /// The jumpers' estimated loads in `case` with the solve's wire
    /// library `library` ([`crate::jumper::end_loads`]), worked out once
    /// per case; none when `library` is not the solve's.
    pub(crate) fn end_loads(
        &self,
        line: &Line,
        case: &WeatherCase,
        library: &ohl_cables::Library,
    ) -> Option<Arc<JumperLoadMap>> {
        if self.library == 0 || std::ptr::from_ref(library) as usize != self.library {
            return None;
        }
        // (Keyed by the case's every number as written: bit for bit.)
        let key = format!("{case:?}");
        if let Ok(kept) = self.end_loads.lock()
            && let Some((_, m)) = kept.iter().find(|(c, _)| *c == key)
        {
            return Some(m.clone());
        }
        let m = Arc::new(crate::jumper::end_loads(line, case, |id| library.get(id)));
        if let Ok(mut kept) = self.end_loads.lock() {
            if kept.len() >= 64 {
                kept.clear();
            }
            kept.push((key, m.clone()));
        }
        Some(m)
    }

    /// The loads of the jumpers on wire type `id` alone (its data
    /// `cable`) in `case`, as a section of that wire type is solved with
    /// them when not given ([`crate::jumper::end_loads`]), worked out once
    /// per case and wire.
    pub(crate) fn end_loads_on(
        &self,
        line: &Line,
        case: &WeatherCase,
        id: &str,
        cable: &ohl_cables::Cable,
    ) -> Arc<JumperLoadMap> {
        // (Keyed by the case's and the wire's every number as written: bit
        // for bit.)
        let key = format!("{case:?} {id:?} {cable:?}");
        if let Ok(kept) = self.end_loads_on.lock()
            && let Some((_, m)) = kept.iter().find(|(k, _)| *k == key)
        {
            return m.clone();
        }
        let m = Arc::new(crate::jumper::end_loads(line, case, |x| {
            (x == id).then_some(cable)
        }));
        if let Ok(mut kept) = self.end_loads_on.lock() {
            if kept.len() >= 256 {
                kept.clear();
            }
            kept.push((key, m.clone()));
        }
        m
    }
}

/// Runs `f` with the lookups of `line` (see the module), solved with the
/// wire library `library` (the jumpers' loads are kept for it). Already
/// open for `line` on this thread: those.
pub fn indexed<R>(line: &Line, library: Option<&ohl_cables::Library>, f: impl FnOnce() -> R) -> R {
    let at = std::ptr::from_ref(line) as usize;
    let lib = library.map_or(0, |l| std::ptr::from_ref(l) as usize);
    let open = OPEN.with(|o| {
        o.borrow().iter().any(|l| match l {
            Open::Line(l) => l.line == at && (l.library == lib || lib == 0),
            Open::Shapes(_) => false,
            Open::Off => true,
        })
    });
    if open {
        return f();
    }
    let look = Arc::new(Lookups {
        line: at,
        library: lib,
        ids: OnceLock::new(),
        views: OnceLock::new(),
        route_grid: OnceLock::new(),
        local_wind: OnceLock::new(),
        end_loads: Mutex::new(Vec::new()),
        end_loads_on: Mutex::new(Vec::new()),
    });
    within(vec![Open::Line(look)], f)
}

/// Where each section's solved shape is in a list of solved shapes (the
/// jumpers are solved between them): positions by section id, in the
/// list's order.
#[derive(Debug)]
pub(crate) struct ShapeIds {
    /// The list's address and length (its identity while it is borrowed).
    at: (usize, usize),
    by_section: OnceLock<HashMap<String, Vec<usize>>>,
}

impl ShapeIds {
    /// The positions of the shapes of section `id` in `shapes` (the list
    /// these are of), in its order.
    pub(crate) fn of<'a>(&'a self, shapes: &[SectionShape], id: &str) -> &'a [usize] {
        self.by_section
            .get_or_init(|| {
                let mut m: HashMap<String, Vec<usize>> = HashMap::new();
                for (k, s) in shapes.iter().enumerate() {
                    m.entry(s.section.clone()).or_default().push(k);
                }
                m
            })
            .get(id)
            .map_or(&[], Vec::as_slice)
    }
}

/// Runs `f` with the solved shapes `shapes` looked up by section
/// ([`shapes_with`]): for a solve of many jumpers between them. Already
/// open for them on this thread, or a few: as they are.
pub(crate) fn shapes_indexed<R>(shapes: &[SectionShape], f: impl FnOnce() -> R) -> R {
    let at = (shapes.as_ptr() as usize, shapes.len());
    let open = OPEN.with(|o| {
        o.borrow()
            .iter()
            .any(|l| matches!(l, Open::Off) || matches!(l, Open::Shapes(s) if s.at == at))
    });
    if open || shapes.len() < 16 {
        return f();
    }
    let ids = Arc::new(ShapeIds {
        at,
        by_section: OnceLock::new(),
    });
    within(vec![Open::Shapes(ids)], f)
}

/// `f` of the positions of `shapes` by section, when they are open on
/// this thread ([`shapes_indexed`]); none otherwise.
pub(crate) fn shapes_with<R>(shapes: &[SectionShape], f: impl FnOnce(&ShapeIds) -> R) -> Option<R> {
    let at = (shapes.as_ptr() as usize, shapes.len());
    let ids = OPEN.with(|o| {
        o.borrow()
            .iter()
            .rev()
            .take_while(|l| !matches!(l, Open::Off))
            .find_map(|l| match l {
                Open::Shapes(s) if s.at == at => Some(s.clone()),
                _ => None,
            })
    })?;
    Some(f(&ids))
}

/// What is open on a thread: a line's lookups, a list of solved shapes
/// looked up by section, or none to be opened ([`searched`]).
#[derive(Debug, Clone)]
pub(crate) enum Open {
    Line(Arc<Lookups>),
    Shapes(Arc<ShapeIds>),
    Off,
}

/// Runs `f` with every lookup searched through the line as before the
/// lookups were made (none opened within it, also on its solve's worker
/// threads): the reference the tests compare the lookups with — the
/// same results, bit for bit.
pub fn searched<R>(f: impl FnOnce() -> R) -> R {
    within(vec![Open::Off], f)
}

/// What is open on this thread (for a worker thread to take it:
/// [`within`]).
pub(crate) fn open() -> Vec<Open> {
    OPEN.with(|o| o.borrow().clone())
}

/// Runs `f` with `looks` open as well (what a worker thread takes from its
/// caller, [`open`]). They are closed again after it, also when it
/// panics.
pub(crate) fn within<R>(looks: Vec<Open>, f: impl FnOnce() -> R) -> R {
    if looks.is_empty() {
        return f();
    }
    struct Close(usize);
    impl Drop for Close {
        fn drop(&mut self) {
            OPEN.with(|o| {
                let mut o = o.borrow_mut();
                let keep = o.len().saturating_sub(self.0);
                o.truncate(keep);
            });
        }
    }
    let n = looks.len();
    OPEN.with(|o| o.borrow_mut().extend(looks));
    let _close = Close(n);
    f()
}

/// `f` of the lookups open for `line` on this thread (the innermost);
/// none when there are none.
pub(crate) fn with<R>(line: &Line, f: impl FnOnce(&Lookups) -> R) -> Option<R> {
    let at = std::ptr::from_ref(line) as usize;
    let look = OPEN.with(|o| {
        o.borrow()
            .iter()
            .rev()
            .take_while(|l| !matches!(l, Open::Off))
            .find_map(|l| match l {
                Open::Line(l) if l.line == at => Some(l.clone()),
                _ => None,
            })
    })?;
    Some(f(&look))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Everything the line's methods answer about every object, as text
    /// (objects by their address: the very object a search finds).
    fn answers(line: &Line) -> String {
        let at = |x: Option<&dyn std::any::Any>| x.map(|x| std::ptr::from_ref(x).cast::<u8>());
        let all = |v: Vec<*const u8>| format!("{v:?}");
        let mut out = String::new();
        let structure_ids: Vec<&str> = line.structures.iter().map(|s| s.id.as_str()).collect();
        for s in &line.structures {
            out += &format!(
                "{:?} {} {}\n",
                at(line.structure(&s.id).map(|x| x as _)),
                all(line
                    .sections_through(&s.id)
                    .map(|x| std::ptr::from_ref(x).cast())
                    .collect()),
                line.wind_factor_at_structure(&s.id).to_bits(),
            );
        }
        out += &all(line
            .structures_of_ids(&structure_ids[..structure_ids.len() / 2])
            .into_iter()
            .map(|x| std::ptr::from_ref(x).cast())
            .collect());
        for s in &line.sections {
            out += &format!("{:?}\n", at(line.section_by_id(&s.id).map(|x| x as _)));
            for p in &s.points {
                out += &format!(
                    "{:?} {} {}\n",
                    at(line.first_section_at(p).map(|x| x as _)),
                    all(line
                        .sections_at_point(p)
                        .map(|x| std::ptr::from_ref(x).cast())
                        .collect()),
                    all(line
                        .sections_at_points(&[p, &s.points[0]])
                        .into_iter()
                        .map(|x| std::ptr::from_ref(x).cast())
                        .collect()),
                );
            }
        }
        for j in &line.jumpers {
            out += &format!(
                "{:?} {:?} {} {:?}\n",
                at(line.jumper_by_id(&j.id).map(|x| x as _)),
                at(line.jumper_section(j).map(|x| x as _)),
                line.jumper_wind_factor(j).to_bits(),
                line.end_structures(&j.to),
            );
        }
        out += &format!("{:?}\n{:?}\n", line.alignment_views(), line.views());
        for id in ["nothing", "S1"] {
            out += &format!("{:?}\n", at(line.structure(id).map(|x| x as _)));
        }
        out
    }

    /// The line's methods answer the same with the lookups open as
    /// searched — the first object of an id where ids are given twice,
    /// the sections in the line's order, local winds — and the lookups are
    /// open only for the line they were made of, inside [`indexed`], not
    /// inside [`searched`], and on the solve's worker threads.
    #[test]
    fn the_lookups_answer_as_the_searches() {
        let mut line = crate::tests::example();
        // Ids given twice (refused by the validation, answered all the
        // same), a local wind.
        let mut twin = line.structures[1].clone();
        twin.position[0] += 5.0;
        line.structures.push(twin);
        let twin = line.sections[0].clone();
        line.sections.push(twin);
        line.sections[1].wind_factors = vec![crate::SpanWind {
            span: 0,
            factor: 1.3,
        }];
        let searched = answers(&line);
        assert!(with(&line, |_| ()).is_none());
        let looked = indexed(&line, None, || {
            assert!(with(&line, |_| ()).is_some());
            let copy = line.clone();
            assert!(with(&copy, |_| ()).is_none(), "another line: none");
            // Within `searched`: none, and none opened.
            super::searched(|| {
                assert!(with(&line, |_| ()).is_none());
                indexed(&line, None, || assert!(with(&line, |_| ()).is_none()));
            });
            // The worker threads take them.
            let items: Vec<usize> = (0..64).collect();
            let open = crate::jumper::in_parallel(&items, |_| with(&line, |_| ()).is_some());
            assert!(open.into_iter().all(|b| b));
            answers(&line)
        });
        assert!(with(&line, |_| ()).is_none(), "closed after");
        assert_eq!(looked, searched);
    }
}
