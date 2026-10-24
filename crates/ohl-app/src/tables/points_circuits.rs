// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Alignment points, routes, alignments and circuits: their rows and the
//! commands of their edits.

use super::*;

/// The alignment points' rows: on each alignment in order, its points in
/// order (key `alignment\tpoint`); then the points on no alignment (key
/// `\tpoint`).
pub(super) fn point_rows(line: &ohl_line::Line) -> Vec<Row> {
    let row = |a: &str, k: Option<usize>, p: &ohl_line::AlignmentPoint| {
        let also: Vec<String> = crate::editors::point_orders(line, &p.id)
            .into_iter()
            .filter(|(x, _)| x != a)
            .map(|(x, n)| format!("{x} #{n}"))
            .collect();
        Row {
            defaulted: Vec::new(),
            key: format!("{a}\t{}", p.id),
            select: Selection::AlignmentPoint { id: p.id.clone() },
            cells: vec![
                a.to_owned(),
                k.map(|k| (k + 1).to_string()).unwrap_or_default(),
                p.id.clone(),
                f(p.position[0], 3),
                f(p.position[1], 3),
                f(p.position[2], 3),
                p.structure.clone().unwrap_or_default(),
                also.join(", "),
                p.description.clone(),
            ],
        }
    };
    let mut out = Vec::new();
    for a in &line.alignments {
        for (k, p) in line.points_of(a).iter().enumerate() {
            out.push(row(&a.id, Some(k), p));
        }
    }
    for p in &line.alignment_points {
        if line.point_alignments(&p.id).is_empty() {
            out.push(row("", None, p));
        }
    }
    out
}

/// Routes as a cell gives them: "a, b".
pub(super) fn route_list(text: &str) -> Vec<String> {
    let mut out: Vec<String> = text
        .split(',')
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(str::to_owned)
        .collect();
    out.dedup();
    out
}

/// The conductor paths, one row each: P1, P2… in the order of
/// their first sections; keyed by their first section (stable when turned).
pub(super) fn circuit_rows(line: &ohl_line::Line) -> Vec<Row> {
    ohl_line::electrical::circuits(line)
        .into_iter()
        .enumerate()
        .map(|(k, c)| {
            let key = circuit_key(&c);
            let sections: Vec<&ohl_line::Section> = c
                .sections
                .iter()
                .filter_map(|id| line.sections.iter().find(|s| &s.id == id))
                .collect();
            // One value for the circuit, or what its sections say.
            let common = |get: &dyn Fn(&ohl_line::Section) -> String| -> String {
                let mut v: Vec<String> = sections.iter().map(|s| get(s)).collect();
                v.sort();
                v.dedup();
                match &v[..] {
                    [] => String::new(),
                    [one] => one.clone(),
                    many => format!("mixed: {}", many.join(" / ")),
                }
            };
            let list = |v: &[SectionPoint]| v.iter().map(point).collect::<Vec<_>>().join("; ");
            Row {
                defaulted: Vec::new(),
                key,
                select: c
                    .sections
                    .first()
                    .map_or(Selection::None, |s| Selection::Span {
                        section: s.clone(),
                        span: 0,
                    }),
                cells: vec![
                    format!("P{}", k + 1),
                    common(&|s| s.circuit_name.clone()),
                    common(&|s| s.phase_label.clone()),
                    common(&|s| s.system.clone().unwrap_or_default()),
                    list(&c.starts),
                    list(&c.ends),
                    c.sections.join(" "),
                    c.jumpers.join(" "),
                    common(&|s| s.color.clone().unwrap_or_default()),
                ],
            }
        })
        .collect()
}

/// The ends of a circuit's row: its starts and its ends cells.
pub(super) fn circuit_ends(cells: &[String]) -> Vec<String> {
    cells[4]
        .split(';')
        .chain(cells[5].split(';'))
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
        .collect()
}

/// A circuit's key: its first section in the line's order.
pub(super) fn circuit_key(c: &ohl_line::electrical::Circuit) -> String {
    c.sections.iter().min().cloned().unwrap_or_default()
}

/// The circuits (named groups of conductor paths), one row each, in the
/// order of the line's list; keyed by id.
pub(super) fn circuit_list_rows(line: &ohl_line::Line) -> Vec<Row> {
    let mut line = Cow::Borrowed(line);
    if line.sections.iter().any(|s| {
        !s.circuit_name.is_empty() && !line.circuits.iter().any(|c| c.name == s.circuit_name)
    }) {
        line.to_mut().adopt_circuits();
    }
    let notes = line.circuit_notes();
    line.circuits
        .iter()
        .map(|c| {
            let members = line.circuit_members(&c.name);
            let about_it = |n: &&String| {
                n.starts_with(&format!("circuit {} ", c.id))
                    || n.starts_with(&format!("circuit {}:", c.id))
                    || members
                        .iter()
                        .any(|m| n.starts_with(&format!("conductor path {} (", m.key)))
            };
            let mut phases: Vec<&str> = members
                .iter()
                .flat_map(|m| m.phases.iter().map(String::as_str))
                .collect();
            phases.dedup();
            Row {
                defaulted: Vec::new(),
                key: c.id.clone(),
                select: members.first().and_then(|m| m.sections.first()).map_or(
                    Selection::None,
                    |s| Selection::Span {
                        section: s.clone(),
                        span: 0,
                    },
                ),
                cells: vec![
                    c.id.clone(),
                    c.name.clone(),
                    c.system.clone().unwrap_or_default(),
                    members
                        .iter()
                        .map(|m| m.key.as_str())
                        .collect::<Vec<_>>()
                        .join(" "),
                    phases.join(" "),
                    members
                        .iter()
                        .flat_map(|m| m.sections.iter().map(String::as_str))
                        .collect::<Vec<_>>()
                        .join(" "),
                    notes
                        .iter()
                        .filter(about_it)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("; "),
                    c.description.clone(),
                ],
            }
        })
        .collect()
}

/// The Circuits table's changes as one command ([`ohl_session::line_circuits_change`]):
/// circuits deleted (their conductor paths in none after), renamed, given a
/// system, a description or another id, added; their members taken out,
/// then put in (a conductor path still in another circuit is refused).
pub(super) fn circuit_list_commands(
    stored: &ohl_line::Line,
    page: &TablePage,
    out: &mut Changes,
) -> Result<(), String> {
    use ohl_line::circuits::{Circuit, path_key};
    let mut line = stored.clone();
    line.adopt_circuits();
    let paths = ohl_line::electrical::conductor_paths(&line);
    let members_of = |text: &str| -> Result<Vec<String>, String> {
        let mut v: Vec<String> = Vec::new();
        for t in text
            .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
            .filter(|t| !t.is_empty())
        {
            let k = paths
                .iter()
                .find(|p| p.iter().any(|s| s == t))
                .map(|p| path_key(p))
                .ok_or_else(|| format!("members: no section {t}"))?;
            if !v.contains(&k) {
                v.push(k);
            }
        }
        Ok(v)
    };
    for key in &page.deleted {
        let name = line
            .circuit(key)
            .map(|c| c.name.clone())
            .ok_or_else(|| format!("no circuit {key}"))?;
        let n = line.circuit_members(&name).len();
        if n > 0 {
            out.takes.push(format!(
                "circuit {key}: its {n} conductor paths are in no circuit after"
            ));
        }
        line.delete_circuit(key)?;
    }
    let mut wanted: Vec<(String, Vec<String>)> = Vec::new();
    for (key, edit) in page
        .edits
        .iter()
        .filter(|(k, _)| !page.deleted.contains(*k))
    {
        let cell = |k: usize| edit.get(&k).map(|v| v.trim().to_owned());
        if let Some(n) = cell(1) {
            line.rename_circuit(key, &n)?;
        }
        let id = cell(0).unwrap_or_else(|| key.clone());
        if id != *key {
            if id.is_empty() {
                return Err(format!("circuit {key}: give it an id"));
            }
            if line.circuits.iter().any(|c| c.id == id) {
                return Err(format!("circuit {key}: the id {id} is taken"));
            }
        }
        let c = line
            .circuits
            .iter_mut()
            .find(|c| &c.id == key)
            .ok_or_else(|| format!("no circuit {key}"))?;
        c.id = id.clone();
        if let Some(s) = cell(2) {
            c.system = (!s.is_empty()).then_some(s);
        }
        if let Some(d) = cell(7) {
            c.description = d;
        }
        if let Some(m) = cell(3) {
            wanted.push((id, members_of(&m)?));
        }
    }
    for cells in &page.added {
        let get = |k: usize| cells.get(k).map_or(String::new(), |s| s.trim().to_owned());
        let c = Circuit {
            id: get(0),
            name: get(1),
            system: Some(get(2)).filter(|s| !s.is_empty()),
            description: get(7),
        };
        if c.id.is_empty() || c.name.is_empty() {
            return Err("a new circuit: give it an id and a name".into());
        }
        line.add_circuit(c)?;
        if !get(3).is_empty() {
            wanted.push((get(0), members_of(&get(3))?));
        }
    }
    // Members: those taken out first (they may go to another circuit in
    // the same Apply), then those put in.
    let name_of = |line: &ohl_line::Line, id: &str| {
        line.circuit(id)
            .map(|c| c.name.clone())
            .ok_or_else(|| format!("no circuit {id}"))
    };
    for (id, keys) in &wanted {
        let name = name_of(&line, id)?;
        let leaving: Vec<String> = line
            .circuit_members(&name)
            .into_iter()
            .map(|m| m.key)
            .filter(|k| !keys.contains(k))
            .collect();
        line.assign_to_circuit(&leaving, None)?;
    }
    for (id, keys) in &wanted {
        let name = name_of(&line, id)?;
        let mut coming = Vec::new();
        for k in keys {
            let now = line
                .sections
                .iter()
                .find(|s| &s.id == k)
                .map_or("", |s| s.circuit_name.as_str());
            if now == name {
                continue;
            }
            if !now.is_empty() {
                return Err(format!(
                    "circuit {id}: conductor path {k} is in circuit \"{now}\" — take it out \
                     there in the same Apply, or use Assign to circuit in Properties (it moves)"
                ));
            }
            coming.push(k.clone());
        }
        line.assign_to_circuit(&coming, Some(id))
            .map_err(|e| format!("circuit {id}: {e}"))?;
    }
    out.commands.push((
        "changed circuits".into(),
        ohl_session::line_circuits_change(stored, &line)?,
    ));
    Ok(())
}

/// Circuits turned ("reversed" changed): their starts in the line's
/// settings, and their jumpers turned to point along them.
pub(super) fn circuit_commands(
    stored: &ohl_line::Line,
    rows: &[Row],
    page: &TablePage,
    out: &mut Changes,
) -> Result<(), String> {
    let mut line = stored.clone();
    let along = {
        let mut l = stored.clone();
        l.circuit_starts.clear();
        ohl_line::electrical::circuits(&l)
    };
    let mut turned = Vec::new();
    for row in rows {
        let Some(edit) = page.edits.get(&row.key) else {
            continue;
        };
        let Some(v) = edit.get(&4) else { continue };
        let Some(c) = along.iter().find(|c| circuit_key(c) == row.key) else {
            continue;
        };
        let all: Vec<SectionPoint> = c.starts.iter().chain(&c.ends).cloned().collect();
        let mut starts = Vec::new();
        for t in v.split(';').map(str::trim).filter(|t| !t.is_empty()) {
            let p = parse_point(t).map_err(|e| format!("{} starts: {e}", row.cells[0]))?;
            if !all.contains(&p) {
                return Err(format!(
                    "{} starts: {t} is not one of its ends",
                    row.cells[0]
                ));
            }
            starts.push(p);
        }
        if starts.is_empty() {
            return Err(format!("{}: give it at least one start", row.cells[0]));
        }
        if all.len() > 1 && starts.len() == all.len() {
            return Err(format!("{}: leave it at least one end", row.cells[0]));
        }
        // Its ends' roles: none listed when it runs along the line.
        line.circuit_starts.retain(|p| !all.contains(p));
        if starts != c.starts {
            line.circuit_starts.extend(starts);
        }
        turned.push(row.key.clone());
    }
    // Name, phase and system: on all its sections (`sim`: the line as
    // they make it, for the circuits).
    let mut sim = stored.clone();
    for row in rows {
        let Some(edit) = page.edits.get(&row.key) else {
            continue;
        };
        let Some(c) = along.iter().find(|c| circuit_key(c) == row.key) else {
            continue;
        };
        let set = |k: usize| edit.get(&k).map(|v| v.trim().to_owned());
        let (name, phase, system) = (set(1), set(2), set(3));
        let color = match set(8) {
            Some(v) if v.starts_with("mixed:") => {
                return Err(format!(
                    "{}: give one colour for the conductor path",
                    row.cells[0]
                ));
            }
            Some(v) => Some(parse_color(&v, &row.cells[0])?),
            None => None,
        };
        if name.is_none() && phase.is_none() && system.is_none() && color.is_none() {
            continue;
        }
        for v in [&name, &phase].into_iter().flatten() {
            if v.starts_with("mixed:") {
                return Err(format!(
                    "{}: give one value for the conductor path",
                    row.cells[0]
                ));
            }
        }
        let system = match system {
            None => None,
            Some(v) if v.is_empty() => Some(None),
            Some(v) if stored.systems.iter().any(|x| x.id == v) => Some(Some(v)),
            Some(v) => return Err(format!("{}: no electrical system \"{v}\"", row.cells[0])),
        };
        let mut changed = Vec::new();
        for id in &c.sections {
            let Some(mut sec) = stored.sections.iter().find(|s| &s.id == id).cloned() else {
                continue;
            };
            let before = sec.clone();
            if let Some(n) = &name {
                sec.circuit_name = n.clone();
            }
            if let Some(p) = &phase {
                sec.phase_label = p.clone();
            }
            if let Some(sy) = &system {
                sec.system = sy.clone();
            }
            if let Some(col) = &color {
                sec.color = col.clone();
            }
            if sec != before {
                changed.push(id.clone());
                if let Some(x) = sim.sections.iter_mut().find(|x| &x.id == id) {
                    *x = sec.clone();
                }
                out.commands.push((
                    format!("section {id}: as its conductor path {}", row.cells[0]),
                    Command::Put {
                        kind: Kind::Section,
                        id: id.clone(),
                        value: serde_json::to_value(&sec).map_err(|e| e.to_string())?,
                        ord: None,
                    },
                ));
            }
        }
        // Its jumpers with a colour of their own take the circuit's (the
        // others follow their sections).
        if let Some(col) = &color {
            for id in &c.jumpers {
                let Some(mut j) = stored.jumpers.iter().find(|j| &j.id == id).cloned() else {
                    continue;
                };
                if j.color.is_none() || j.color == *col {
                    continue;
                }
                j.color = col.clone();
                changed.push(format!("jumper {id}"));
                out.commands.push((
                    format!(
                        "jumper {id}: the colour of its conductor path {}",
                        row.cells[0]
                    ),
                    Command::Put {
                        kind: Kind::Jumper,
                        id: id.clone(),
                        value: serde_json::to_value(&j).map_err(|e| e.to_string())?,
                        ord: None,
                    },
                ));
            }
        }
        if !changed.is_empty() {
            out.takes.push(format!(
                "conductor path {}: sections {}",
                row.cells[0],
                changed.join(", ")
            ));
        }
    }
    // Its circuit: a new name makes a circuit (in the same command); what
    // contradicts the connections is refused.
    sim.adopt_circuits();
    sim.check_circuit_change(stored)?;
    if sim.circuits != stored.circuits {
        out.commands.push((
            "circuits of the conductor paths".into(),
            ohl_session::line_settings_change(stored, &sim),
        ));
    }
    if turned.is_empty() {
        return Ok(());
    }
    out.commands.push((
        "changed the starts and ends of conductor paths".into(),
        ohl_session::line_settings_change(stored, &line),
    ));
    // Their jumpers along them: `from` the end nearer a start.
    for c in ohl_line::electrical::circuits(&line)
        .iter()
        .filter(|c| turned.contains(&circuit_key(c)))
    {
        let at = |p: &SectionPoint| c.order.iter().position(|q| q == p);
        for id in &c.jumpers {
            let Some(j) = line.jumpers.iter().find(|j| &j.id == id) else {
                continue;
            };
            let (Some(from), Some(to)) = (j.from.point(), j.to.point()) else {
                continue;
            };
            if at(to) < at(from) {
                let mut j = j.clone();
                // Its points (supports among them) from the other end; a
                // support clamp on the same structure as before.
                let (first, last) = (from.structure.clone(), to.structure.clone());
                let horizontal = match (
                    ohl_line::jumper::approximate_end(&line, &j.from, 0),
                    ohl_line::jumper::approximate_end(&line, &j.to, 0),
                ) {
                    (Some(a), Some(b)) => (b[0] - a[0]).hypot(b[1] - a[1]),
                    _ => 0.0,
                };
                let length = j.length;
                for p in &mut j.points {
                    p.at = match p.at {
                        ohl_line::JumperPlace::Fraction(f) => {
                            ohl_line::JumperPlace::Fraction(1.0 - f)
                        }
                        ohl_line::JumperPlace::Length(l) => {
                            ohl_line::JumperPlace::Length(length.map_or(l, |t| t - l))
                        }
                        ohl_line::JumperPlace::Distance(d) => {
                            ohl_line::JumperPlace::Distance(horizontal - d)
                        }
                    };
                    if let Some(ohl_line::objects::Hold::Strings { structure, .. }) =
                        p.object.as_mut().and_then(|o| o.hold.as_mut())
                    {
                        let id = structure.take().unwrap_or_else(|| first.clone());
                        *structure = (id != last).then_some(id);
                    }
                }
                std::mem::swap(&mut j.from, &mut j.to);
                std::mem::swap(&mut j.angle_from_deg, &mut j.angle_to_deg);
                out.commands.push((
                    format!("jumper {} along its conductor path", j.id),
                    Command::Put {
                        kind: Kind::Jumper,
                        id: j.id.clone(),
                        value: serde_json::to_value(&j).map_err(|e| e.to_string())?,
                        ord: None,
                    },
                ));
            }
        }
    }
    Ok(())
}

/// The alignments as edited: names, routes, order; new ones (without
/// points) and deletions (their points stay).
pub(super) fn alignment_list_commands(
    stored: &ohl_line::Line,
    rows: &[Row],
    page: &TablePage,
    out: &mut Changes,
) -> Result<(), String> {
    let known = |r: &str| stored.routes.iter().any(|x| x.id == r);
    // (order, alignment) as the rows give them.
    let mut list: Vec<(f64, ohl_line::Alignment)> = Vec::new();
    for (k, row) in rows.iter().enumerate() {
        if page.deleted.contains(&row.key) {
            continue;
        }
        let c = Cells {
            row: &row.cells,
            typed: page.edits.get(&row.key),
            defaulted: &row.defaulted,
        };
        let mut a = stored
            .alignment_by_id(&row.key)
            .ok_or_else(|| format!("no alignment {}", row.key))?
            .clone();
        let new_id = c.get(0).trim().to_owned();
        if new_id.is_empty() {
            return Err(format!("alignment {}: give it an id", row.key));
        }
        if new_id != row.key {
            out.commands.push((
                format!("renamed alignment {} to {new_id}", row.key),
                Command::Rename {
                    kind: Kind::Alignment,
                    id: row.key.clone(),
                    new_id: new_id.clone(),
                },
            ));
            a.id = new_id;
        }
        a.name = c.get(1).trim().to_owned();
        a.route = c.get(2).trim().to_owned();
        if !known(&a.route) {
            return Err(format!(
                "alignment {}: choose its route (none yet: create one above)",
                a.id
            ));
        }
        a.description = c.get(5).trim().to_owned();
        let mut order = num(&c.get(3), &format!("alignment {} #", a.id))?;
        if c.changed(3) {
            order -= 1e-9;
        }
        let _ = k;
        list.push((order, a));
    }
    for (k, cells) in page.added.iter().enumerate() {
        let c = Cells {
            row: cells,
            typed: None,
            defaulted: &[],
        };
        let id = c.get(0).trim().to_owned();
        if id.is_empty() {
            return Err(format!("new alignment {}: give it an id", k + 1));
        }
        if list.iter().any(|(_, a)| a.id == id) {
            return Err(format!("there is an alignment {id} already"));
        }
        let route = c.get(2).trim().to_owned();
        if !known(&route) {
            return Err(format!(
                "new alignment {id}: choose its route (none yet: create one above)"
            ));
        }
        let mut a = ohl_line::Alignment::new(&id, &route, Vec::new());
        a.name = c.get(1).trim().to_owned();
        a.description = c.get(5).trim().to_owned();
        let order = opt_num(&c.get(3), &format!("new alignment {id} #"))?.unwrap_or(f64::MAX);
        list.push((order + 1e-9, a));
    }
    list.sort_by(|a, b| a.0.total_cmp(&b.0));
    let alignments: Vec<ohl_line::Alignment> = list.into_iter().map(|x| x.1).collect();
    // Renamed ones are put under their new id after the rename.
    let mut renamed = stored.clone();
    for (_, c) in &out.commands {
        if let Command::Rename { id, new_id, .. } = c
            && let Some(a) = renamed.alignments.iter_mut().find(|a| &a.id == id)
        {
            a.id = new_id.clone();
        }
    }
    out.commands.push((
        "changed the alignments".into(),
        crate::editors::alignments_command(&renamed, &alignments)?,
    ));
    Ok(())
}

/// The alignment points as edited: each point's values (the same on every
/// row of it), the alignments' lists of points in the order typed; a row
/// deleted deletes its point (off every alignment it is on).
pub(super) fn point_commands(
    model: &Model,
    stored: &ohl_line::Line,
    rows: &[Row],
    page: &TablePage,
    out: &mut Changes,
) -> Result<(), String> {
    let point_col = TableKind::AlignmentPoints.column("point");
    let structure_col = TableKind::AlignmentPoints.column("structure");
    let description_col = TableKind::AlignmentPoints.column("description");
    let point_of = |c: &Cells, name: &str| -> Result<ohl_line::AlignmentPoint, String> {
        let structure = opt(&c.get(structure_col));
        let position = match &structure {
            Some(s) if c.changed(structure_col) || c.typed.is_none() => {
                model
                    .line
                    .structure(s)
                    .ok_or_else(|| format!("{name}: no structure {s}"))?
                    .position
            }
            Some(s) if model.line.structure(s).is_none() => {
                return Err(format!("{name}: no structure {s}"));
            }
            _ => {
                let e = num(&c.get(3), &format!("{name} E"))?;
                let n = num(&c.get(4), &format!("{name} N"))?;
                let z = match opt_num(&c.get(5), &format!("{name} z"))? {
                    Some(z) => z,
                    None => model.ground_at(e, n).unwrap_or(0.0),
                };
                [e, n, z]
            }
        };
        Ok(ohl_line::AlignmentPoint {
            id: c.get(point_col).trim().to_owned(),
            description: c.get(description_col).trim().to_owned(),
            position,
            structure,
        })
    };
    let alignment_of = |c: &Cells, name: &str| -> Result<Option<String>, String> {
        let a = c.get(0).trim().to_owned();
        if a.is_empty() {
            return Ok(None);
        }
        if stored.alignment_by_id(&a).is_none() {
            return Err(format!(
                "{name}: no alignment {a} (make it in the Alignments table)"
            ));
        }
        Ok(Some(a))
    };
    // A row deleted takes its point off that row's alignment (the others
    // keep their order); the point itself goes only with the last of its
    // rows (the Delete tool takes it off every alignment at once).
    let pid_of = |key: &str| key.split_once('\t').map_or(key, |(_, p)| p).to_owned();
    let deleted: BTreeSet<String> = page
        .deleted
        .iter()
        .map(|k| pid_of(k))
        .filter(|pid| {
            rows.iter()
                .filter(|r| pid_of(&r.key) == *pid)
                .all(|r| page.deleted.contains(&r.key))
        })
        .collect();
    // The points' values (edited on any of their rows) and the places.
    let mut points: BTreeMap<String, ohl_line::AlignmentPoint> = BTreeMap::new();
    let mut edited: BTreeMap<String, String> = BTreeMap::new();
    let mut places: BTreeMap<String, Vec<(f64, String)>> = BTreeMap::new();
    for row in rows {
        let (_, pid) = row.key.split_once('\t').unwrap_or(("", &row.key));
        if deleted.contains(pid) || page.deleted.contains(&row.key) {
            continue;
        }
        let c = Cells {
            row: &row.cells,
            typed: page.edits.get(&row.key),
            defaulted: &row.defaulted,
        };
        let name = format!("point {pid}");
        let p = point_of(&c, &name)?;
        let typed = page
            .edits
            .get(&row.key)
            .is_some_and(|e| e.keys().any(|k| ![0, 1].contains(k)));
        match points.get(pid) {
            Some(q) if typed && edited.contains_key(pid) && *q != p => {
                return Err(format!(
                    "{name}: changed differently on two of its rows ({} and {})",
                    edited[pid], row.cells[0]
                ));
            }
            Some(_) if !typed => {}
            _ => {
                points.insert(pid.to_owned(), p);
                if typed {
                    edited.insert(pid.to_owned(), row.cells[0].clone());
                }
            }
        }
        let Some(a) = alignment_of(&c, &name)? else {
            continue;
        };
        let mut order = opt_num(&c.get(1), &format!("{name} #"))?.unwrap_or(f64::MAX);
        // Moved to another alignment, or renumbered: before the point that
        // has that number there.
        if a != row.cells[0] || c.changed(1) {
            order -= 1e-9;
        }
        places.entry(a).or_default().push((order, pid.to_owned()));
    }
    let mut new_points = Vec::new();
    for (k, cells) in page.added.iter().enumerate() {
        let c = Cells {
            row: cells,
            typed: None,
            defaulted: &[],
        };
        let name = format!("new row {}", k + 1);
        let pid = c.get(point_col).trim().to_owned();
        if pid.is_empty() {
            return Err(format!("{name}: give the point an id"));
        }
        let existing = stored.alignment_point(&pid).is_some() || points.contains_key(&pid);
        if !existing {
            let p = point_of(&c, &name)?;
            points.insert(pid.clone(), p.clone());
            new_points.push(pid.clone());
        }
        if let Some(a) = alignment_of(&c, &name)? {
            let order = opt_num(&c.get(1), &format!("{name} #"))?.unwrap_or(f64::MAX);
            places.entry(a).or_default().push((order + 1e-9, pid));
        }
    }
    // The alignments' new lists.
    let mut alignments = stored.alignments.clone();
    for a in &mut alignments {
        let mut list = places.remove(&a.id).unwrap_or_default();
        list.sort_by(|x, y| x.0.total_cmp(&y.0));
        let ids: Vec<String> = list.into_iter().map(|x| x.1).collect();
        if let Some(d) = ids
            .iter()
            .enumerate()
            .find(|(k, p)| ids[..*k].contains(p))
            .map(|x| x.1)
        {
            return Err(format!("alignment {}: point {d} twice", a.id));
        }
        a.points = ids;
    }
    // Deletions first: they take the points off the stored alignments,
    // the alignments' new lists are put after them.
    let mut commands: Vec<Command> = deleted
        .iter()
        .map(|p| Command::Delete {
            kind: Kind::AlignmentPoint,
            id: p.clone(),
            confirm: true,
        })
        .collect();
    for (pid, p) in &points {
        if stored.alignment_point(pid) != Some(p) {
            commands.push(Command::Put {
                kind: Kind::AlignmentPoint,
                id: pid.clone(),
                value: serde_json::to_value(p).map_err(|e| e.to_string())?,
                ord: None,
            });
        }
    }
    match crate::editors::alignments_command(stored, &alignments)? {
        Command::Batch { commands: c } => commands.extend(c),
        c => commands.push(c),
    }
    if !commands.is_empty() {
        out.commands.push((
            "changed the alignment points".into(),
            if commands.len() == 1 {
                commands.remove(0)
            } else {
                Command::Batch { commands }
            },
        ));
    }
    Ok(())
}
