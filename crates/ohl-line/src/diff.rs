// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What differs between two versions of a line, as the objects to draw
//! and the sections to solve again: one rule for the project's dirty
//! sections after an edit (`ohl-session`) and for the project lines shown
//! next to the one worked on (OHL-Design). A section differs when it does,
//! when a structure it hangs on does, when its wire type's data differ,
//! when a setting every solve reads differs (the solver's, the defaults,
//! the bimetallic and creep-stretch models, the systems, the condition
//! cases), when a spacer joining it differs, and when the routes give it
//! the wind from the other side.

use crate::Line;

/// What a project line shown differs in from the line worked on.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Differences {
    /// Its sections changed or added, or through a structure that is
    /// (by id).
    pub sections: std::collections::BTreeSet<String>,
    /// Its structures changed or added (by id; types resolved).
    pub structures: std::collections::BTreeSet<String>,
    /// Its jumpers changed or added, or at a section that differs.
    pub jumpers: std::collections::BTreeSet<String>,
}

impl Differences {
    /// `line` with only what differs kept (its sections, structures and
    /// jumpers that differ): what is drawn of a project line shown.
    pub fn of_line(&self, line: &Line) -> Line {
        let mut l = line.clone();
        l.sections.retain(|s| self.sections.contains(&s.id));
        l.structures.retain(|s| self.structures.contains(&s.id));
        l.jumpers.retain(|j| self.jumpers.contains(&j.id));
        l
    }

    /// What `other` differs in from `ours` (both resolved): compared
    /// object by object, a section also when one of its structures
    /// differs or its wire type is among `cables_changed` (its data differ
    /// between the two lines' wire libraries).
    ///
    /// Expected cost: one comparison per object (milliseconds for a few
    /// thousand).
    pub fn between_with(
        ours: &Line,
        other: &Line,
        cables_changed: &std::collections::BTreeSet<String>,
    ) -> Self {
        // Ours by id (the first of an id, as a search finds it).
        fn by_id<T>(items: &[T], id: impl Fn(&T) -> &str) -> std::collections::HashMap<&str, &T> {
            let mut m = std::collections::HashMap::with_capacity(items.len());
            for x in items {
                m.entry(id(x)).or_insert(x);
            }
            m
        }
        let our_structures = by_id(&ours.structures, |s| s.id.as_str());
        let our_sections = by_id(&ours.sections, |s| s.id.as_str());
        let our_jumpers = by_id(&ours.jumpers, |j| j.id.as_str());
        let structures: std::collections::BTreeSet<String> = other
            .structures
            .iter()
            .filter(|s| our_structures.get(s.id.as_str()).copied() != Some(*s))
            .map(|s| s.id.clone())
            .collect();
        // The line's settings every solve reads (as the session's dirty
        // sections after an edit): every section differs when one does.
        let settings = ours.solver != other.solver
            || ours.defaults != other.defaults
            || ours.bimetallic != other.bimetallic
            || ours.creep_stretch != other.creep_stretch
            || ours.systems != other.systems
            || ours.condition_cases != other.condition_cases;
        // The routes give each section its wind's side: those whose side
        // differs.
        let sides: std::collections::BTreeSet<String> = if ours.routes != other.routes
            || ours.alignments != other.alignments
            || ours.alignment_points != other.alignment_points
        {
            let a = crate::solve::sections_against_the_line(ours);
            crate::solve::sections_against_the_line(other)
                .into_iter()
                .filter(|(id, side)| a.get(id) != Some(side))
                .map(|(id, _)| id)
                .collect()
        } else {
            Default::default()
        };
        // The sections a spacer joins, where the spacer differs (or is
        // only in one of them).
        let mut spaced: std::collections::BTreeSet<&str> = Default::default();
        for (a, b) in [(ours, other), (other, ours)] {
            for s in &a.spacers {
                if b.spacers.iter().find(|x| x.id == s.id) != Some(s) {
                    spaced.insert(s.a.section.as_str());
                    spaced.insert(s.b.section.as_str());
                }
            }
        }
        let sections: std::collections::BTreeSet<String> = other
            .sections
            .iter()
            .filter(|s| {
                settings
                    || our_sections.get(s.id.as_str()).copied() != Some(*s)
                    || s.points.iter().any(|p| structures.contains(&p.structure))
                    || cables_changed.contains(&s.cable)
                    || spaced.contains(s.id.as_str())
                    || sides.contains(&s.id)
            })
            .map(|s| s.id.clone())
            .collect();
        // The points of the sections that differ.
        let changed_points: std::collections::HashSet<&crate::SectionPoint> = other
            .sections
            .iter()
            .filter(|s| sections.contains(&s.id))
            .flat_map(|s| &s.points)
            .collect();
        let at_changed = |p: &crate::SectionPoint| {
            structures.contains(&p.structure) || changed_points.contains(p)
        };
        let jumpers = other
            .jumpers
            .iter()
            .filter(|j| {
                our_jumpers.get(j.id.as_str()).copied() != Some(*j)
                    || cables_changed.contains(&j.cable)
                    || [&j.from, &j.to].into_iter().any(|e| match e {
                        crate::JumperEnd::Wire(p) => at_changed(p),
                        _ => false,
                    })
            })
            .map(|j| j.id.clone())
            .collect();
        Self {
            sections,
            structures,
            jumpers,
        }
    }
}

/// The sections of `other` to solve again after `ours` became `other`
/// (an edit): those that differ ([`Differences::between_with`], the wire
/// library the same), and those at the ends of a jumper itself changed,
/// added or deleted (its weight hangs on their clamps; a jumper that
/// differs only because a section at its end does leaves its other end).
pub fn sections_to_solve_again(ours: &Line, other: &Line) -> std::collections::BTreeSet<String> {
    let d = Differences::between_with(ours, other, &Default::default());
    let mut out = d.sections;
    let same =
        |a: &crate::Jumper, line: &Line| line.jumpers.iter().find(|x| x.id == a.id) == Some(a);
    let mut ends: Vec<&crate::Jumper> = other.jumpers.iter().filter(|j| !same(j, ours)).collect();
    ends.extend(ours.jumpers.iter().filter(|j| !same(j, other)));
    for j in ends {
        for s in &other.sections {
            if s.points.iter().any(|p| j.is_at(p)) {
                out.insert(s.id.clone());
            }
        }
    }
    out
}
